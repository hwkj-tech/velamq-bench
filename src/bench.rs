use std::{
    collections::{BTreeMap, VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use futures_util::future::join_all;
use get_if_addrs::{IfAddr, get_if_addrs};
use rumqttc::{
    AsyncClient, Event as MqttEvent, MqttOptions, Packet, QoS, TlsConfiguration, Transport,
};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as RustlsError, RootCertStore, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use tokio::{
    sync::{Mutex, RwLock, broadcast, watch},
    task::JoinHandle,
    time::{Instant, MissedTickBehavior},
};
use uuid::Uuid;

use crate::{
    model::{
        Annotation, AuthConfig, BenchConfig, BenchEvent, BenchMode, BenchReport, BenchRun,
        BenchSpecimen, BenchStatus, BenchTemplate, BrokerProfile, BrokerProtocol, LoadProfile,
        LoadShape, LogLine, MetricSnapshot, MqttVersion, NetworkBindMode, NetworkInterfaceInfo,
        PayloadKind, PayloadProfile, QosLevel, Run, RunStatus, RunWorkload, RuntimeView, Scenario,
        ScenarioStage, SpecimenUpdate, StartBenchRequest, StartResponse, TemplateDraft, Workload,
        WorkloadKind, normalize_websocket_path,
    },
    runtime::{
        load_clock::LoadClock,
        sampler::{CounterSample, WorkloadSampler},
        sse::RunEvent,
    },
    storage::Storage,
};

const LOG_LIMIT: usize = 300;
const TIMESTAMP_PREFIX: &[u8] = b"velamq-ts-ns=";
const TIMESTAMP_SUFFIX: u8 = b';';

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::CryptoProvider::get_default()
            .expect("rustls crypto provider is installed")
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[derive(Debug)]
pub struct BenchManager {
    storage: Storage,
    runtime: RwLock<RuntimeView>,
    logs: Mutex<VecDeque<LogLine>>,
    events: broadcast::Sender<BenchEvent>,
    run_events: broadcast::Sender<RunEvent>,
    current: Mutex<Option<BenchHandle>>,
}

#[derive(Debug)]
struct BenchHandle {
    run_id: String,
    stop_tx: watch::Sender<bool>,
    join: JoinHandle<()>,
}

#[derive(Debug, Clone)]
struct NetworkBindingPlan {
    mode: NetworkBindMode,
    devices: Vec<String>,
}

#[derive(Debug)]
struct InterfaceAccumulator {
    addresses: Vec<String>,
    loopback: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BrokerConnectionTest {
    pub ok: bool,
    pub profile_id: String,
    pub host: String,
    pub port: u16,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeSummary {
    pub active_run_id: Option<String>,
    pub state: RuntimeView,
}

#[derive(Debug, Clone)]
struct RuntimeWorkloadPlan {
    run_workload_id: String,
    name: String,
    config: BenchConfig,
    load: LoadProfile,
}

impl NetworkBindingPlan {
    fn from_config(config: &BenchConfig) -> Result<Self> {
        let devices = match config.network_bind_mode {
            NetworkBindMode::System => Vec::new(),
            NetworkBindMode::AutoRandom | NetworkBindMode::AutoRoundRobin => {
                auto_bind_devices(list_network_interfaces()?)
            }
            NetworkBindMode::ManualRandom | NetworkBindMode::ManualRoundRobin => {
                manual_bind_devices(&config.bind_interfaces, list_network_interfaces()?)
            }
        };

        Ok(Self {
            mode: config.network_bind_mode.clone(),
            devices,
        })
    }

    fn is_enabled(&self) -> bool {
        self.mode.is_enabled()
    }

    fn device_for(&self, run_id: &str, index: u64, offset: usize) -> Option<String> {
        if !self.is_enabled() || self.devices.is_empty() {
            return None;
        }

        let selected = if self.mode.is_random() {
            let mut hasher = DefaultHasher::new();
            run_id.hash(&mut hasher);
            index.hash(&mut hasher);
            (hasher.finish() as usize) % self.devices.len()
        } else {
            offset % self.devices.len()
        };

        self.devices.get(selected).cloned()
    }
}

fn auto_bind_devices(interfaces: Vec<NetworkInterfaceInfo>) -> Vec<String> {
    let non_loopback = interfaces
        .iter()
        .filter(|interface| !interface.loopback)
        .map(|interface| interface.name.clone())
        .collect::<Vec<_>>();
    if non_loopback.is_empty() {
        interfaces
            .into_iter()
            .map(|interface| interface.name)
            .collect()
    } else {
        non_loopback
    }
}

fn manual_bind_devices(entries: &[String], interfaces: Vec<NetworkInterfaceInfo>) -> Vec<String> {
    let mut devices = Vec::new();
    for entry in entries {
        let Some(device) = interfaces.iter().find_map(|interface| {
            if interface.name == *entry
                || interface.addresses.iter().any(|address| address == entry)
            {
                Some(interface.name.clone())
            } else {
                None
            }
        }) else {
            devices.push(entry.clone());
            continue;
        };

        if !devices.iter().any(|value| value == &device) {
            devices.push(device);
        }
    }
    devices
}

impl BenchManager {
    pub fn new(storage: Storage) -> Arc<Self> {
        let (events, _) = broadcast::channel(512);
        let (run_events, _) = broadcast::channel(2048);
        Arc::new(Self {
            storage,
            runtime: RwLock::new(RuntimeView::default()),
            logs: Mutex::new(VecDeque::with_capacity(LOG_LIMIT)),
            events,
            run_events,
            current: Mutex::new(None),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BenchEvent> {
        self.events.subscribe()
    }

    pub fn subscribe_run_events(&self) -> broadcast::Receiver<RunEvent> {
        self.run_events.subscribe()
    }

    pub async fn state(&self) -> RuntimeView {
        self.runtime.read().await.clone()
    }

    pub async fn history(
        &self,
        run_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<MetricSnapshot>> {
        self.storage.recent_snapshots(run_id, limit).await
    }

    pub async fn runs(&self, limit: usize) -> Result<Vec<BenchRun>> {
        self.storage.list_runs(limit).await
    }

    pub async fn report(&self, run_id: &str) -> Result<Option<BenchReport>> {
        self.storage.report(run_id).await
    }

    pub async fn interfaces(&self) -> Result<Vec<NetworkInterfaceInfo>> {
        tokio::task::spawn_blocking(list_network_interfaces).await?
    }

    pub async fn specimens(&self, limit: usize) -> Result<Vec<BenchSpecimen>> {
        self.storage.list_specimens(limit).await
    }

    pub async fn update_specimen(
        &self,
        specimen_id: &str,
        update: SpecimenUpdate,
    ) -> Result<Option<BenchSpecimen>, String> {
        update.validate()?;
        self.storage
            .update_specimen(specimen_id, update)
            .await
            .map_err(|err| format!("failed to update specimen: {err:#}"))
    }

    pub async fn delete_specimen(&self, specimen_id: &str) -> Result<bool, String> {
        self.storage
            .delete_specimen(specimen_id)
            .await
            .map_err(|err| format!("failed to delete specimen: {err:#}"))
    }

    pub async fn templates(&self, limit: usize) -> Result<Vec<BenchTemplate>> {
        self.storage.list_templates(limit).await
    }

    pub async fn create_template(&self, draft: TemplateDraft) -> Result<BenchTemplate, String> {
        let draft = draft.normalized("Custom Template".to_string())?;
        let now = Utc::now();
        let template = BenchTemplate {
            id: Uuid::new_v4().to_string(),
            name: draft.name.unwrap_or_else(|| "Custom Template".to_string()),
            description: draft.description.unwrap_or_default(),
            tags: draft.tags,
            config: draft.config,
            created_at: now,
            updated_at: now,
        };

        self.storage
            .create_template(template)
            .await
            .map_err(|err| format!("failed to create template: {err:#}"))
    }

    pub async fn update_template(
        &self,
        template_id: &str,
        draft: TemplateDraft,
    ) -> Result<Option<BenchTemplate>, String> {
        let draft = draft.normalized("Custom Template".to_string())?;
        self.storage
            .update_template(template_id, draft)
            .await
            .map_err(|err| format!("failed to update template: {err:#}"))
    }

    pub async fn delete_template(&self, template_id: &str) -> Result<bool, String> {
        self.storage
            .delete_template(template_id)
            .await
            .map_err(|err| format!("failed to delete template: {err:#}"))
    }

    pub async fn broker_profiles(&self) -> Result<Vec<BrokerProfile>> {
        self.storage.list_broker_profiles().await
    }

    pub async fn broker_profile(&self, id: &str) -> Result<Option<BrokerProfile>> {
        self.storage.get_broker_profile(id).await
    }

    pub async fn upsert_broker_profile(&self, profile: BrokerProfile) -> Result<BrokerProfile> {
        self.storage.upsert_broker_profile(profile).await
    }

    pub async fn delete_broker_profile(&self, id: &str) -> Result<bool> {
        self.storage.delete_broker_profile(id).await
    }

    pub async fn test_broker_connection(&self, id: &str) -> Result<Option<BrokerConnectionTest>> {
        let Some(profile) = self.storage.get_broker_profile(id).await? else {
            return Ok(None);
        };
        let config = config_from_broker_profile(&profile);
        let started = Instant::now();
        if config.mqtt_version == MqttVersion::V5_0 {
            return self
                .test_broker_connection_v5(profile, config, started)
                .await
                .map(Some);
        }
        let mut mqtt_options =
            mqtt_options_for_client(format!("velamq-test-{}", Uuid::new_v4()), &config)?;
        mqtt_options.set_keep_alive(Duration::from_secs(config.keepalive_secs.into()));
        mqtt_options.set_clean_session(true);
        if let Some(username) = config.username.as_deref().filter(|value| !value.is_empty()) {
            mqtt_options.set_credentials(username, config.password.clone().unwrap_or_default());
        }
        let (_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
        eventloop
            .network_options
            .set_connection_timeout(config.connection_timeout_secs.into());
        let result = tokio::time::timeout(
            Duration::from_secs(config.connection_timeout_secs.into()),
            async {
                loop {
                    match eventloop.poll().await {
                        Ok(MqttEvent::Incoming(Packet::ConnAck(_))) => return Ok(()),
                        Ok(_) => {}
                        Err(err) => return Err(err.to_string()),
                    }
                }
            },
        )
        .await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        Ok(Some(match result {
            Ok(Ok(())) => BrokerConnectionTest {
                ok: true,
                profile_id: profile.id,
                host: profile.host,
                port: profile.port,
                elapsed_ms,
                error: None,
            },
            Ok(Err(err)) => BrokerConnectionTest {
                ok: false,
                profile_id: profile.id,
                host: profile.host,
                port: profile.port,
                elapsed_ms,
                error: Some(err),
            },
            Err(_) => BrokerConnectionTest {
                ok: false,
                profile_id: profile.id,
                host: profile.host,
                port: profile.port,
                elapsed_ms,
                error: Some(format!(
                    "connection timed out after {}s",
                    config.connection_timeout_secs
                )),
            },
        }))
    }

    async fn test_broker_connection_v5(
        &self,
        profile: BrokerProfile,
        config: BenchConfig,
        started: Instant,
    ) -> Result<BrokerConnectionTest> {
        use rumqttc::v5::mqttbytes::v5::Packet as PacketV5;
        use rumqttc::v5::{AsyncClient as AsyncClientV5, Event as EventV5};

        let options =
            mqtt5_options_for_client(format!("velamq-test-{}", Uuid::new_v4()), &config, None)?;
        let (_client, mut eventloop) = AsyncClientV5::new(options, 10);
        let result = tokio::time::timeout(
            Duration::from_secs(config.connection_timeout_secs.into()),
            async {
                loop {
                    match eventloop.poll().await {
                        Ok(EventV5::Incoming(PacketV5::ConnAck(_))) => return Ok(()),
                        Ok(_) => {}
                        Err(err) => return Err(err.to_string()),
                    }
                }
            },
        )
        .await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(err)) => Some(err),
            Err(_) => Some(format!(
                "connection timed out after {}s",
                config.connection_timeout_secs
            )),
        };
        Ok(BrokerConnectionTest {
            ok: error.is_none(),
            profile_id: profile.id,
            host: profile.host,
            port: profile.port,
            elapsed_ms,
            error,
        })
    }

    pub async fn payload_profiles(&self) -> Result<Vec<PayloadProfile>> {
        self.storage.list_payload_profiles().await
    }

    pub async fn payload_profile(&self, id: &str) -> Result<Option<PayloadProfile>> {
        self.storage.get_payload_profile(id).await
    }

    pub async fn upsert_payload_profile(&self, profile: PayloadProfile) -> Result<PayloadProfile> {
        self.storage.upsert_payload_profile(profile).await
    }

    pub async fn delete_payload_profile(&self, id: &str) -> Result<bool> {
        self.storage.delete_payload_profile(id).await
    }

    pub async fn scenarios(&self) -> Result<Vec<Scenario>> {
        self.storage.list_scenarios().await
    }

    pub async fn scenario(&self, id: &str) -> Result<Option<Scenario>> {
        self.storage.get_scenario(id).await
    }

    pub async fn upsert_scenario(&self, scenario: Scenario) -> Result<Scenario> {
        self.storage.upsert_scenario(scenario).await
    }

    pub async fn delete_scenario(&self, id: &str) -> Result<bool> {
        self.storage.delete_scenario(id).await
    }

    pub async fn set_scenario_baseline(
        &self,
        id: &str,
        run_id: Option<String>,
    ) -> Result<Option<Scenario>> {
        self.storage.set_scenario_baseline(id, run_id).await
    }

    pub async fn runs_v2(
        &self,
        scenario_id: Option<String>,
        status: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::model::Run>> {
        self.storage.list_runs_v2(scenario_id, status, limit).await
    }

    pub async fn run_v2(&self, id: &str) -> Result<Option<crate::model::Run>> {
        self.storage.get_run_v2(id).await
    }

    pub async fn upsert_run_v2(&self, run: crate::model::Run) -> Result<crate::model::Run> {
        self.storage.upsert_run_v2(run).await
    }

    pub async fn report_v2(&self, id: &str) -> Result<Option<BenchReport>> {
        self.storage.report_v2(id).await
    }

    pub async fn runtime_state_v2(&self) -> RuntimeSummary {
        let state = self.state().await;
        let active_run_id = {
            let current = self.current.lock().await;
            current.as_ref().map(|handle| handle.run_id.clone())
        };
        RuntimeSummary {
            active_run_id,
            state,
        }
    }

    pub async fn update_run_v2_metadata(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<Option<crate::model::Run>> {
        self.storage
            .update_run_v2_metadata(id, name, description, tags)
            .await
    }

    pub async fn delete_run_v2(&self, id: &str) -> Result<bool> {
        self.storage.delete_run_v2(id).await
    }

    pub async fn snapshots_v2(
        &self,
        run_id: &str,
        run_workload_id: Option<String>,
        since_ms: Option<u64>,
        limit: usize,
    ) -> Result<Vec<MetricSnapshot>> {
        self.storage
            .snapshots_v2(run_id, run_workload_id, since_ms, limit)
            .await
    }

    pub async fn import_snapshot(&self, snapshot: MetricSnapshot) -> Result<()> {
        self.storage.insert_snapshot(snapshot).await
    }

    pub async fn annotations(&self, run_id: &str) -> Result<Vec<Annotation>> {
        self.storage.list_annotations(run_id).await
    }

    pub async fn upsert_annotation(&self, annotation: Annotation) -> Result<Annotation> {
        let annotation = self.storage.upsert_annotation(annotation).await?;
        let _ = self.run_events.send(RunEvent::RunAnnotation {
            run_id: annotation.run_id.clone(),
            annotation: annotation.clone(),
        });
        Ok(annotation)
    }

    pub async fn start_scenario(
        self: &Arc<Self>,
        scenario: Scenario,
    ) -> Result<StartResponse, String> {
        self.start_scenario_inner(scenario, true).await
    }

    pub async fn start_ad_hoc_scenario(
        self: &Arc<Self>,
        scenario: Scenario,
    ) -> Result<StartResponse, String> {
        self.start_scenario_inner(scenario, false).await
    }

    async fn start_scenario_inner(
        self: &Arc<Self>,
        scenario: Scenario,
        bind_scenario_id: bool,
    ) -> Result<StartResponse, String> {
        let workload_count = scenario_workload_count(&scenario);
        if workload_count == 0 {
            return Err("scenario must contain at least one workload".to_string());
        }

        let mut current = self.current.lock().await;
        if let Some(handle) = current.as_ref() {
            if !handle.join.is_finished() {
                return Err(format!("benchmark {} is already running", handle.run_id));
            }
        }

        let run_id = Uuid::new_v4().to_string();
        let started_at = Utc::now();
        let mut run_workloads = Vec::with_capacity(workload_count);
        let mut stage_configs = Vec::with_capacity(scenario.stages.len());
        let mut first_config = None;

        for stage in &scenario.stages {
            let workloads = stage_workloads(stage);
            let mut configs = Vec::with_capacity(workloads.len());
            for workload in workloads {
                let config = self
                    .legacy_config_from_workload(workload)
                    .await
                    .map_err(|err| format!("failed to prepare scenario run: {err:#}"))?;
                let workload_id = if workload.id.trim().is_empty() {
                    Uuid::new_v4().to_string()
                } else {
                    workload.id.clone()
                };
                let run_workload_id = format!("rw-{}", Uuid::new_v4());
                run_workloads.push(RunWorkload {
                    id: run_workload_id.clone(),
                    run_id: run_id.clone(),
                    workload_id: workload_id.clone(),
                    kind: workload.kind.clone(),
                    config_snapshot_json: serde_json::to_string(workload)
                        .map_err(|err| format!("failed to snapshot workload: {err}"))?,
                });
                first_config.get_or_insert_with(|| config.clone());
                configs.push(RuntimeWorkloadPlan {
                    run_workload_id,
                    name: workload.name.clone(),
                    config,
                    load: workload.load.clone(),
                });
            }
            stage_configs.push((stage_is_parallel(stage), configs));
        }

        let first_config = first_config.unwrap_or_default();
        let specimen = BenchSpecimen {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.clone(),
            name: scenario.name.clone(),
            description: scenario.description.clone(),
            tags: scenario.tags.clone(),
            config: first_config.clone(),
            created_at: started_at,
        };
        let run = Run {
            id: run_id.clone(),
            scenario_id: bind_scenario_id.then(|| scenario.id.clone()),
            name: scenario.name.clone(),
            tags: scenario.tags.clone(),
            description: scenario.description.clone(),
            status: RunStatus::Running,
            started_at,
            stopped_at: None,
            workloads: run_workloads,
            baseline_of_scenario_id: None,
        };
        self.storage
            .upsert_run_v2(run)
            .await
            .map_err(|err| format!("failed to create v2 run: {err:#}"))?;

        {
            let mut logs = self.logs.lock().await;
            logs.clear();
        }
        let state = RuntimeView {
            status: BenchStatus::Running,
            run_id: Some(run_id.clone()),
            config: Some(first_config),
            started_at: Some(started_at),
            stopped_at: None,
            specimen: Some(specimen.clone()),
            latest: None,
            logs: Vec::new(),
        };
        self.replace_state(state).await;

        let (stop_tx, stop_rx) = watch::channel(false);
        let manager = Arc::clone(self);
        let task_run_id = run_id.clone();
        let task_stop_tx = stop_tx.clone();
        let join = tokio::spawn(async move {
            manager
                .run_scenario_stages(
                    task_run_id,
                    stage_configs,
                    started_at,
                    task_stop_tx,
                    stop_rx,
                )
                .await;
        });

        *current = Some(BenchHandle {
            run_id: run_id.clone(),
            stop_tx,
            join,
        });

        self.push_log(
            "info",
            format!(
                "scenario run {run_id} started: scenario={}, workloads={workload_count}",
                scenario.name
            ),
        )
        .await;

        Ok(StartResponse {
            run_id,
            specimen,
            state: self.state().await,
        })
    }

    async fn legacy_config_from_workload(&self, workload: &Workload) -> Result<BenchConfig> {
        let mut config = workload
            .flatten_to_legacy()
            .unwrap_or_else(|| config_from_workload(workload));

        if let Some(profile) = self
            .storage
            .get_broker_profile(&workload.broker_profile_id)
            .await?
        {
            config.host = profile.host;
            config.port = profile.port;
            config.protocol = profile.protocol;
            config.websocket_path = profile.websocket_path;
            config.keepalive_secs = profile.keepalive_secs;
            config.clean_session = profile.clean_session;
            if let Some(AuthConfig::UserPassword { username, password }) = profile.auth {
                config.username = Some(username);
                config.password = Some(password);
            }
        }

        if let Some(payload_id) = &workload.payload_profile_id {
            if let Some(profile) = self.storage.get_payload_profile(payload_id).await? {
                if let PayloadKind::FixedBytes {
                    size,
                    with_timestamp,
                } = profile.kind
                {
                    config.payload_size = size;
                    config.payload_timestamp = with_timestamp;
                }
            }
        }

        Ok(config)
    }

    pub async fn start(
        self: &Arc<Self>,
        request: StartBenchRequest,
    ) -> Result<StartResponse, String> {
        let config = request.config.clone().normalized();
        config.validate()?;

        let mut current = self.current.lock().await;
        if let Some(handle) = current.as_ref() {
            if !handle.join.is_finished() {
                return Err(format!("benchmark {} is already running", handle.run_id));
            }
        }

        let run_id = Uuid::new_v4().to_string();
        let started_at = Utc::now();
        let specimen = create_specimen(run_id.clone(), config.clone(), request, started_at)?;
        self.storage
            .start_run(&run_id, &config, started_at, &specimen)
            .await
            .map_err(|err| format!("failed to create run and specimen: {err:#}"))?;

        {
            let mut logs = self.logs.lock().await;
            logs.clear();
        }
        let state = RuntimeView {
            status: BenchStatus::Running,
            run_id: Some(run_id.clone()),
            config: Some(config.clone()),
            started_at: Some(started_at),
            stopped_at: None,
            specimen: Some(specimen.clone()),
            latest: None,
            logs: Vec::new(),
        };
        self.replace_state(state.clone()).await;

        let (stop_tx, stop_rx) = watch::channel(false);
        let manager = Arc::clone(self);
        let task_run_id = run_id.clone();
        let task_config = config.clone();
        let task_stop_tx = stop_tx.clone();
        let join = tokio::spawn(async move {
            manager
                .run_benchmark(task_run_id, task_config, started_at, task_stop_tx, stop_rx)
                .await;
        });

        *current = Some(BenchHandle {
            run_id: run_id.clone(),
            stop_tx,
            join,
        });

        self.push_log(
            "info",
            format!(
                "run {run_id} started: specimen={}, mode={}, clients={}, broker={}:{}",
                specimen.name,
                config.mode.as_str(),
                config.clients,
                config.host,
                config.port
            ),
        )
        .await;

        Ok(StartResponse {
            run_id,
            specimen,
            state: self.state().await,
        })
    }

    pub async fn stop(&self) -> Result<RuntimeView, String> {
        self.request_stop(None).await
    }

    pub async fn stop_run(&self, run_id: &str) -> Result<RuntimeView, String> {
        self.request_stop(Some(run_id)).await
    }

    async fn request_stop(&self, expected_run_id: Option<&str>) -> Result<RuntimeView, String> {
        let current = self.current.lock().await;
        let Some(handle) = current.as_ref() else {
            return Err("no benchmark is running".to_string());
        };
        if let Some(expected_run_id) = expected_run_id {
            if handle.run_id != expected_run_id {
                return Err(format!("run {expected_run_id} is not active"));
            }
        }
        handle
            .stop_tx
            .send(true)
            .map_err(|_| "benchmark task already stopped".to_string())?;
        drop(current);

        self.set_status(BenchStatus::Stopping, None).await;
        self.push_log("info", "stop requested".to_string()).await;
        Ok(self.state().await)
    }

    async fn run_benchmark(
        self: Arc<Self>,
        run_id: String,
        config: BenchConfig,
        started_at: chrono::DateTime<Utc>,
        stop_tx: watch::Sender<bool>,
        stop_rx: watch::Receiver<bool>,
    ) {
        let counters = Arc::new(WorkloadSampler::legacy(&run_id));
        let worker_handles = Arc::new(Mutex::new(Vec::with_capacity(config.clients)));
        let binding_plan = match NetworkBindingPlan::from_config(&config) {
            Ok(plan) => plan,
            Err(err) => {
                self.push_log(
                    "warn",
                    format!("failed to scan network interfaces: {err:#}; using system routing"),
                )
                .await;
                NetworkBindingPlan {
                    mode: NetworkBindMode::System,
                    devices: Vec::new(),
                }
            }
        };
        if binding_plan.is_enabled() {
            if binding_plan.devices.is_empty() {
                self.push_log(
                    "warn",
                    format!(
                        "network binding {} has no usable interfaces; using system routing",
                        network_bind_mode_label(&binding_plan.mode)
                    ),
                )
                .await;
            } else {
                self.push_log(
                    "info",
                    format!(
                        "network binding {} over {}",
                        network_bind_mode_label(&binding_plan.mode),
                        binding_plan.devices.join(", ")
                    ),
                )
                .await;
                if !network_bind_supported() {
                    self.push_log(
                        "warn",
                        "interface binding requires rumqttc bind_device support on Linux/Android/Fuchsia; this platform will use system routing".to_string(),
                    )
                    .await;
                }
            }
        }
        let config = Arc::new(config);
        let binding_plan = Arc::new(binding_plan);

        let launcher = {
            let manager = Arc::clone(&self);
            let run_id = run_id.clone();
            let config = Arc::clone(&config);
            let binding_plan = Arc::clone(&binding_plan);
            let counters = Arc::clone(&counters);
            let worker_handles = Arc::clone(&worker_handles);
            let mut stop_rx = stop_rx.clone();
            tokio::spawn(async move {
                manager
                    .launch_clients(
                        run_id,
                        config,
                        binding_plan,
                        counters,
                        worker_handles,
                        None,
                        None,
                        &mut stop_rx,
                    )
                    .await;
            })
        };

        let mut ticker = tokio::time::interval(Duration::from_millis(config.sample_interval_ms));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let mut previous = CounterSample::default();
        let mut previous_tick = Instant::now();
        let started_instant = Instant::now();
        let duration_sleep = tokio::time::sleep(Duration::from_secs(config.duration_secs));
        tokio::pin!(duration_sleep);
        let mut stop_rx = stop_rx;
        let final_status;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let snapshot =
                        counters.snapshot(started_instant, &mut previous, &mut previous_tick);
                    self.publish_snapshot(snapshot).await;
                }
                changed = stop_rx.changed() => {
                    if changed.is_ok() && *stop_rx.borrow() {
                        final_status = BenchStatus::Stopped;
                        break;
                    }
                }
                _ = &mut duration_sleep, if config.duration_secs > 0 => {
                    let _ = stop_tx.send(true);
                    final_status = BenchStatus::Completed;
                    break;
                }
            }
        }

        let _ = stop_tx.send(true);
        let _ = launcher.await;

        let handles = {
            let mut handles = worker_handles.lock().await;
            std::mem::take(&mut *handles)
        };
        let aborts = handles
            .iter()
            .map(JoinHandle::abort_handle)
            .collect::<Vec<_>>();
        if tokio::time::timeout(Duration::from_secs(5), join_all(handles))
            .await
            .is_err()
        {
            for abort in aborts {
                abort.abort();
            }
            self.push_log(
                "warn",
                "some client tasks were aborted during shutdown".to_string(),
            )
            .await;
        }

        let snapshot = counters.snapshot(started_instant, &mut previous, &mut previous_tick);
        self.publish_snapshot(snapshot).await;

        let stopped_at = Utc::now();
        if let Err(err) = self
            .storage
            .finish_run(&run_id, final_status.clone(), stopped_at)
            .await
        {
            self.push_log("error", format!("failed to update run in sqlite: {err:#}"))
                .await;
        }

        self.set_status(final_status.clone(), Some(stopped_at))
            .await;
        self.push_log(
            "info",
            format!(
                "run {run_id} finished with status {} after {} ms",
                status_name(&final_status),
                (stopped_at - started_at).num_milliseconds()
            ),
        )
        .await;

        let mut current = self.current.lock().await;
        if current
            .as_ref()
            .map(|handle| handle.run_id == run_id)
            .unwrap_or(false)
        {
            *current = None;
        }
    }

    async fn run_scenario_stages(
        self: Arc<Self>,
        run_id: String,
        stages: Vec<(bool, Vec<RuntimeWorkloadPlan>)>,
        started_at: chrono::DateTime<Utc>,
        _stop_tx: watch::Sender<bool>,
        stop_rx: watch::Receiver<bool>,
    ) {
        let mut final_status = BenchStatus::Completed;

        for (parallel, workloads) in stages {
            if *stop_rx.borrow() {
                final_status = BenchStatus::Stopped;
                break;
            }

            if parallel {
                let handles = workloads
                    .into_iter()
                    .map(|plan| {
                        let manager = Arc::clone(&self);
                        let run_id = run_id.clone();
                        let stop_rx = stop_rx.clone();
                        tokio::spawn(async move {
                            manager.run_workload_benchmark(run_id, plan, stop_rx).await
                        })
                    })
                    .collect::<Vec<_>>();
                for result in join_all(handles).await {
                    if matches!(result, Ok(BenchStatus::Stopped)) {
                        final_status = BenchStatus::Stopped;
                    }
                }
            } else {
                for plan in workloads {
                    let status = Arc::clone(&self)
                        .run_workload_benchmark(run_id.clone(), plan, stop_rx.clone())
                        .await;
                    if status == BenchStatus::Stopped {
                        final_status = BenchStatus::Stopped;
                        break;
                    }
                }
            }
        }

        let stopped_at = Utc::now();
        if let Err(err) = self
            .storage
            .finish_run_v2(
                &run_id,
                match final_status {
                    BenchStatus::Stopped => RunStatus::Stopped,
                    BenchStatus::Failed => RunStatus::Failed,
                    _ => RunStatus::Completed,
                },
                stopped_at,
            )
            .await
        {
            self.push_log(
                "error",
                format!("failed to update v2 run in sqlite: {err:#}"),
            )
            .await;
        }

        self.set_status(final_status.clone(), Some(stopped_at))
            .await;
        self.push_log(
            "info",
            format!(
                "scenario run {run_id} finished with status {} after {} ms",
                status_name(&final_status),
                (stopped_at - started_at).num_milliseconds()
            ),
        )
        .await;

        let mut current = self.current.lock().await;
        if current
            .as_ref()
            .map(|handle| handle.run_id == run_id)
            .unwrap_or(false)
        {
            *current = None;
        }
    }

    async fn run_workload_benchmark(
        self: Arc<Self>,
        run_id: String,
        plan: RuntimeWorkloadPlan,
        stop_rx: watch::Receiver<bool>,
    ) -> BenchStatus {
        if *stop_rx.borrow() {
            return BenchStatus::Stopped;
        }

        let RuntimeWorkloadPlan {
            run_workload_id,
            name: workload_name,
            config,
            load,
        } = plan;
        let sampler = Arc::new(WorkloadSampler::new(&run_id, &run_workload_id));
        let worker_handles = Arc::new(Mutex::new(Vec::with_capacity(config.clients)));
        let (local_stop_tx, local_stop_rx) = watch::channel(false);
        let local_stop_bridge = {
            let local_stop_tx = local_stop_tx.clone();
            let mut stop_rx = stop_rx.clone();
            tokio::spawn(async move {
                if stop_rx.changed().await.is_ok() && *stop_rx.borrow() {
                    let _ = local_stop_tx.send(true);
                }
            })
        };
        let binding_plan = match NetworkBindingPlan::from_config(&config) {
            Ok(plan) => plan,
            Err(err) => {
                self.push_log(
                    "warn",
                    format!(
                        "workload {workload_name} failed to scan network interfaces: {err:#}; using system routing"
                    ),
                )
                .await;
                NetworkBindingPlan {
                    mode: NetworkBindMode::System,
                    devices: Vec::new(),
                }
            }
        };
        let config = Arc::new(config);
        let binding_plan = Arc::new(binding_plan);

        let launcher = {
            let manager = Arc::clone(&self);
            let run_id = run_id.clone();
            let config = Arc::clone(&config);
            let binding_plan = Arc::clone(&binding_plan);
            let sampler = Arc::clone(&sampler);
            let worker_handles = Arc::clone(&worker_handles);
            let mut stop_rx = local_stop_rx.clone();
            tokio::spawn(async move {
                manager
                    .launch_clients(
                        run_id,
                        config,
                        binding_plan,
                        sampler,
                        worker_handles,
                        Some(load.connect_shape.clone()),
                        Some(load.message_shape.clone()),
                        &mut stop_rx,
                    )
                    .await;
            })
        };

        let mut ticker = tokio::time::interval(Duration::from_millis(config.sample_interval_ms));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let mut previous = CounterSample::default();
        let mut previous_tick = Instant::now();
        let started_instant = Instant::now();
        let duration_sleep = tokio::time::sleep(Duration::from_secs(config.duration_secs));
        tokio::pin!(duration_sleep);
        let mut stop_rx = local_stop_rx;
        let final_status;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let snapshot =
                        sampler.snapshot(started_instant, &mut previous, &mut previous_tick);
                    self.publish_snapshot(snapshot).await;
                }
                changed = stop_rx.changed() => {
                    if changed.is_ok() && *stop_rx.borrow() {
                        final_status = BenchStatus::Stopped;
                        break;
                    }
                }
                _ = &mut duration_sleep, if config.duration_secs > 0 => {
                    let _ = local_stop_tx.send(true);
                    final_status = BenchStatus::Completed;
                    break;
                }
            }
        }

        let _ = local_stop_tx.send(true);
        local_stop_bridge.abort();
        let _ = launcher.await;

        let handles = {
            let mut handles = worker_handles.lock().await;
            std::mem::take(&mut *handles)
        };
        let aborts = handles
            .iter()
            .map(JoinHandle::abort_handle)
            .collect::<Vec<_>>();
        if tokio::time::timeout(Duration::from_secs(5), join_all(handles))
            .await
            .is_err()
        {
            for abort in aborts {
                abort.abort();
            }
            self.push_log(
                "warn",
                format!("some client tasks for workload {workload_name} were aborted"),
            )
            .await;
        }

        let snapshot = sampler.snapshot(started_instant, &mut previous, &mut previous_tick);
        self.publish_snapshot(snapshot).await;
        final_status
    }

    async fn launch_clients(
        self: Arc<Self>,
        run_id: String,
        config: Arc<BenchConfig>,
        binding_plan: Arc<NetworkBindingPlan>,
        counters: Arc<WorkloadSampler>,
        worker_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
        connect_shape: Option<LoadShape>,
        message_shape: Option<LoadShape>,
        stop_rx: &mut watch::Receiver<bool>,
    ) {
        let connect_clock = connect_shape.map(LoadClock::new);

        for offset in 0..config.clients {
            if *stop_rx.borrow() {
                break;
            }

            let index = config.start_number + offset as u64;
            let bind_device = binding_plan.device_for(&run_id, index, offset);
            let worker = tokio::spawn(run_client(
                index,
                run_id.clone(),
                Arc::clone(&config),
                bind_device,
                Arc::clone(&counters),
                Arc::clone(&self),
                message_shape.clone(),
                stop_rx.clone(),
            ));
            worker_handles.lock().await.push(worker);

            if let Some(delay) = launch_delay(&config, connect_clock.as_ref()) {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        break;
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }

        self.push_log("info", format!("client launcher finished for run {run_id}"))
            .await;
    }

    async fn publish_snapshot(&self, snapshot: MetricSnapshot) {
        {
            let mut state = self.runtime.write().await;
            state.latest = Some(snapshot.clone());
        }
        let _ = self.events.send(BenchEvent::Metrics(snapshot.clone()));
        let _ = self.run_events.send(RunEvent::WorkloadMetric {
            run_id: snapshot.run_id.clone(),
            run_workload_id: snapshot
                .run_workload_id
                .clone()
                .unwrap_or_else(|| format!("legacy-run-workload-{}", snapshot.run_id)),
            snapshot: snapshot.clone(),
        });

        if let Err(err) = self.storage.insert_snapshot(snapshot).await {
            self.push_log("error", format!("failed to save metric snapshot: {err:#}"))
                .await;
        }
    }

    async fn replace_state(&self, state: RuntimeView) {
        {
            let mut runtime = self.runtime.write().await;
            *runtime = state.clone();
        }
        let _ = self.events.send(BenchEvent::State(state));
        self.send_run_state_event().await;
    }

    async fn set_status(&self, status: BenchStatus, stopped_at: Option<chrono::DateTime<Utc>>) {
        let state = {
            let mut runtime = self.runtime.write().await;
            runtime.status = status;
            if let Some(stopped_at) = stopped_at {
                runtime.stopped_at = Some(stopped_at);
            }
            runtime.clone()
        };
        let _ = self.events.send(BenchEvent::State(state));
        self.send_run_state_event().await;
    }

    async fn push_log(&self, level: impl Into<String>, message: String) {
        let log = LogLine {
            ts: Utc::now(),
            level: level.into(),
            message,
        };

        let logs = {
            let mut logs = self.logs.lock().await;
            if logs.len() >= LOG_LIMIT {
                logs.pop_front();
            }
            logs.push_back(log.clone());
            logs.iter().cloned().collect::<Vec<_>>()
        };

        {
            let mut runtime = self.runtime.write().await;
            runtime.logs = logs;
        }

        let _ = self.events.send(BenchEvent::Log(log.clone()));
        if let Some(run_id) = self.runtime.read().await.run_id.clone() {
            let _ = self.run_events.send(RunEvent::WorkloadLog {
                run_id,
                run_workload_id: None,
                log,
            });
        }
    }

    async fn send_run_state_event(&self) {
        let state = self.runtime.read().await.clone();
        if let Some(run_id) = state.run_id.clone() {
            let _ = self
                .run_events
                .send(RunEvent::RunStateChanged { run_id, run: state });
        }
    }
}

fn scenario_workload_count(scenario: &Scenario) -> usize {
    scenario
        .stages
        .iter()
        .map(stage_workloads)
        .map(Vec::len)
        .sum()
}

fn stage_workloads(stage: &ScenarioStage) -> &Vec<Workload> {
    match stage {
        ScenarioStage::Parallel { workloads } | ScenarioStage::Sequential { workloads } => {
            workloads
        }
    }
}

fn stage_is_parallel(stage: &ScenarioStage) -> bool {
    matches!(stage, ScenarioStage::Parallel { .. })
}

async fn run_client(
    index: u64,
    run_id: String,
    config: Arc<BenchConfig>,
    bind_device: Option<String>,
    counters: Arc<WorkloadSampler>,
    manager: Arc<BenchManager>,
    message_shape: Option<LoadShape>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let client_id = config.client_id_for(index);
    let topic = config.topic_for(index);
    let static_payload = if config.payload_timestamp {
        Vec::new()
    } else {
        build_payload(config.payload_size, index, None)
    };
    let qos = qos(config.qos);

    if config.mqtt_version == MqttVersion::V5_0 {
        run_client_v5(
            index,
            run_id,
            config,
            bind_device,
            counters,
            manager,
            message_shape,
            stop_rx,
        )
        .await;
        return;
    }

    let mut mqtt_options = match mqtt_options_for_client(client_id.clone(), &config) {
        Ok(options) => options,
        Err(err) => {
            counters.error();
            manager
                .push_log(
                    "error",
                    format!("client {client_id} configuration error: {err:#}"),
                )
                .await;
            return;
        }
    };
    mqtt_options.set_keep_alive(Duration::from_secs(config.keepalive_secs.into()));
    mqtt_options.set_clean_session(config.clean_session);
    if let Some(username) = config.username.as_deref().filter(|value| !value.is_empty()) {
        mqtt_options.set_credentials(username, config.password.clone().unwrap_or_default());
    }

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);
    eventloop
        .network_options
        .set_connection_timeout(config.connection_timeout_secs.into());
    apply_bind_device(&mut eventloop, bind_device.as_deref());
    if config.mode == BenchMode::Sub {
        if let Err(err) = client.subscribe(topic.clone(), qos).await {
            counters.error();
            manager
                .push_log(
                    "error",
                    format!("client {client_id} failed to enqueue subscribe: {err}"),
                )
                .await;
            return;
        }
    }

    let message_clock = message_shape.map(LoadClock::new);
    let publish_sleep = tokio::time::sleep(publish_delay(&config, message_clock.as_ref()));
    tokio::pin!(publish_sleep);

    let mut connected = false;
    let mut reported_errors = 0_u8;

    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_ok() && *stop_rx.borrow() {
                    break;
                }
            }
            _ = &mut publish_sleep, if config.mode == BenchMode::Pub => {
                let payload = if config.payload_timestamp {
                    build_payload(config.payload_size, index, Some(unix_timestamp_nanos()))
                } else {
                    static_payload.clone()
                };
                match client.publish(topic.clone(), qos, config.retain, payload.clone()).await {
                    Ok(()) => counters.published(),
                    Err(err) => {
                        counters.error();
                        if reported_errors < 3 {
                            reported_errors += 1;
                            manager.push_log("error", format!("client {client_id} publish error: {err}")).await;
                        }
                    }
                }
                publish_sleep.as_mut().reset(Instant::now() + publish_delay(&config, message_clock.as_ref()));
            }
            event = eventloop.poll() => {
                match event {
                    Ok(MqttEvent::Incoming(Packet::ConnAck(_))) => {
                        if !connected {
                            connected = true;
                            counters.client_connected();
                        }
                    }
                    Ok(MqttEvent::Incoming(Packet::Publish(publish))) => {
                        counters.received();
                        if config.payload_timestamp {
                            if let Some(sent_ns) = parse_payload_timestamp(&publish.payload) {
                                if let Some(latency) = latency_since(sent_ns) {
                                    counters.latency(latency);
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        counters.error();
                        if connected {
                            connected = false;
                            counters.client_disconnected();
                        }
                        if reported_errors < 3 {
                            reported_errors += 1;
                            let binding = bind_device
                                .as_deref()
                                .map(|device| format!(" bound to {device}"))
                                .unwrap_or_default();
                            manager.push_log(
                                "error",
                                format!("client {client_id}{binding} eventloop error in run {run_id}: {err}")
                            ).await;
                        }
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                }
            }
        }
    }

    if connected {
        counters.client_disconnected();
    }
}

async fn run_client_v5(
    index: u64,
    run_id: String,
    config: Arc<BenchConfig>,
    bind_device: Option<String>,
    counters: Arc<WorkloadSampler>,
    manager: Arc<BenchManager>,
    message_shape: Option<LoadShape>,
    mut stop_rx: watch::Receiver<bool>,
) {
    use rumqttc::v5::mqttbytes::v5::Packet as PacketV5;
    use rumqttc::v5::{AsyncClient as AsyncClientV5, Event as EventV5};

    let client_id = config.client_id_for(index);
    let topic = config.topic_for(index);
    let static_payload = if config.payload_timestamp {
        Vec::new()
    } else {
        build_payload(config.payload_size, index, None)
    };
    let options = match mqtt5_options_for_client(client_id.clone(), &config, bind_device.as_deref())
    {
        Ok(options) => options,
        Err(err) => {
            counters.error();
            manager
                .push_log(
                    "error",
                    format!("client {client_id} MQTT 5 configuration error: {err:#}"),
                )
                .await;
            return;
        }
    };
    let (client, mut eventloop) = AsyncClientV5::new(options, 100);
    if config.mode == BenchMode::Sub {
        if let Err(err) = client.subscribe(topic.clone(), qos_v5(config.qos)).await {
            counters.error();
            manager
                .push_log(
                    "error",
                    format!("client {client_id} failed to enqueue MQTT 5 subscribe: {err}"),
                )
                .await;
            return;
        }
    }

    let message_clock = message_shape.map(LoadClock::new);
    let publish_sleep = tokio::time::sleep(publish_delay(&config, message_clock.as_ref()));
    tokio::pin!(publish_sleep);
    let mut connected = false;
    let mut reported_errors = 0_u8;

    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_ok() && *stop_rx.borrow() { break; }
            }
            _ = &mut publish_sleep, if config.mode == BenchMode::Pub => {
                let payload = if config.payload_timestamp {
                    build_payload(config.payload_size, index, Some(unix_timestamp_nanos()))
                } else {
                    static_payload.clone()
                };
                match client.publish(topic.clone(), qos_v5(config.qos), config.retain, payload).await {
                    Ok(()) => counters.published(),
                    Err(err) => {
                        counters.error();
                        if reported_errors < 3 {
                            reported_errors += 1;
                            manager.push_log("error", format!("client {client_id} MQTT 5 publish error: {err}")).await;
                        }
                    }
                }
                publish_sleep.as_mut().reset(Instant::now() + publish_delay(&config, message_clock.as_ref()));
            }
            event = eventloop.poll() => {
                match event {
                    Ok(EventV5::Incoming(PacketV5::ConnAck(_))) => {
                        if !connected { connected = true; counters.client_connected(); }
                    }
                    Ok(EventV5::Incoming(PacketV5::Publish(publish))) => {
                        counters.received();
                        if config.payload_timestamp {
                            if let Some(sent_ns) = parse_payload_timestamp(&publish.payload) {
                                if let Some(latency) = latency_since(sent_ns) { counters.latency(latency); }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        counters.error();
                        if connected { connected = false; counters.client_disconnected(); }
                        if reported_errors < 3 {
                            reported_errors += 1;
                            let binding = bind_device.as_deref().map(|device| format!(" bound to {device}")).unwrap_or_default();
                            manager.push_log("error", format!("client {client_id}{binding} MQTT 5 eventloop error in run {run_id}: {err}")).await;
                        }
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                }
            }
        }
    }
    if connected {
        counters.client_disconnected();
    }
}

fn mqtt5_options_for_client(
    client_id: String,
    config: &BenchConfig,
    _bind_device: Option<&str>,
) -> Result<rumqttc::v5::MqttOptions> {
    let broker_addr = broker_address_for_config(config);
    let mut options = rumqttc::v5::MqttOptions::new(client_id, broker_addr, config.port);
    options.set_keep_alive(Duration::from_secs(config.keepalive_secs.into()));
    options.set_clean_start(config.clean_session);
    options.set_connection_timeout(config.connection_timeout_secs.into());
    if let Some(username) = config.username.as_deref().filter(|value| !value.is_empty()) {
        options.set_credentials(username, config.password.clone().unwrap_or_default());
    }
    match config.protocol {
        BrokerProtocol::Mqtt => {}
        BrokerProtocol::Mqtts => {
            options.set_transport(Transport::tls_with_config(tls_configuration(config)?));
        }
        BrokerProtocol::Ws => {
            options.set_transport(Transport::Ws);
        }
        BrokerProtocol::Wss => {
            options.set_transport(Transport::wss_with_config(tls_configuration(config)?));
        }
    }
    if let Some(mqtt5) = &config.mqtt5 {
        options.set_session_expiry_interval(mqtt5.session_expiry_interval_secs);
        options.set_receive_maximum(mqtt5.receive_maximum);
        options.set_max_packet_size(mqtt5.maximum_packet_size);
        options.set_topic_alias_max(mqtt5.topic_alias_maximum);
        options.set_request_problem_info(Some(u8::from(mqtt5.request_problem_information)));
    }
    let mut network_options = rumqttc::NetworkOptions::new();
    network_options.set_connection_timeout(config.connection_timeout_secs.into());
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    if let Some(bind_device) = _bind_device.filter(|value| !value.is_empty()) {
        network_options.set_bind_device(bind_device);
    }
    options.set_network_options(network_options);
    Ok(options)
}

fn qos_v5(value: QosLevel) -> rumqttc::v5::mqttbytes::QoS {
    match value {
        QosLevel::Qos0 => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
        QosLevel::Qos1 => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        QosLevel::Qos2 => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
    }
}

#[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
fn apply_bind_device(eventloop: &mut rumqttc::EventLoop, bind_device: Option<&str>) {
    if let Some(bind_device) = bind_device.filter(|value| !value.is_empty()) {
        eventloop.network_options.set_bind_device(bind_device);
    }
}

#[cfg(not(any(target_os = "android", target_os = "fuchsia", target_os = "linux")))]
fn apply_bind_device(_eventloop: &mut rumqttc::EventLoop, _bind_device: Option<&str>) {}

fn config_from_broker_profile(profile: &BrokerProfile) -> BenchConfig {
    let mut config = BenchConfig::default();
    config.protocol = profile.protocol;
    config.mqtt_version = profile.mqtt_version;
    config.host = profile.host.clone();
    config.port = profile.port;
    config.websocket_path = profile.websocket_path.clone();
    config.keepalive_secs = profile.keepalive_secs;
    config.connection_timeout_secs = profile.connection_timeout_secs;
    config.clean_session = profile.clean_session;
    config.tls = profile.tls.clone();
    config.mqtt5 = profile.mqtt5.clone();
    if let Some(AuthConfig::UserPassword { username, password }) = &profile.auth {
        config.username = Some(username.clone());
        config.password = Some(password.clone());
    }
    if let Some(AuthConfig::ClientCert { cert_pem, key_pem }) = &profile.auth {
        let tls = config.tls.get_or_insert_with(Default::default);
        tls.enabled = true;
        tls.client_cert_pem = Some(cert_pem.clone());
        tls.client_key_pem = Some(key_pem.clone());
    }
    config.normalized()
}

fn mqtt_options_for_client(client_id: String, config: &BenchConfig) -> Result<MqttOptions> {
    let broker_addr = broker_address_for_config(config);
    let mut mqtt_options = MqttOptions::new(client_id, broker_addr, config.port);
    match config.protocol {
        BrokerProtocol::Mqtt => {}
        BrokerProtocol::Mqtts => {
            mqtt_options.set_transport(Transport::tls_with_config(tls_configuration(config)?));
        }
        BrokerProtocol::Ws => {
            mqtt_options.set_transport(Transport::Ws);
        }
        BrokerProtocol::Wss => {
            mqtt_options.set_transport(Transport::wss_with_config(tls_configuration(config)?));
        }
    }
    Ok(mqtt_options)
}

fn tls_configuration(config: &BenchConfig) -> Result<TlsConfiguration> {
    use std::io::{BufReader, Cursor};

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let tls = config.tls.clone().unwrap_or_default();
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    roots.add_parsable_certificates(native.certs);

    if let Some(ca_pem) = tls
        .ca_pem
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let certs = rustls_pemfile::certs(&mut BufReader::new(Cursor::new(ca_pem.as_bytes())))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("invalid CA certificate PEM")?;
        if certs.is_empty() {
            return Err(anyhow!("CA certificate PEM contains no certificates"));
        }
        roots.add_parsable_certificates(certs);
    }

    let builder = ClientConfig::builder().with_root_certificates(roots);
    let mut rustls_config = match (
        tls.client_cert_pem
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
        tls.client_key_pem
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
    ) {
        (Some(cert_pem), Some(key_pem)) => {
            let certs =
                rustls_pemfile::certs(&mut BufReader::new(Cursor::new(cert_pem.as_bytes())))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .context("invalid client certificate PEM")?;
            let key =
                rustls_pemfile::private_key(&mut BufReader::new(Cursor::new(key_pem.as_bytes())))
                    .context("invalid client private key PEM")?
                    .ok_or_else(|| anyhow!("client private key PEM contains no private key"))?;
            builder
                .with_client_auth_cert(certs, key)
                .context("client certificate and private key do not match")?
        }
        (None, None) => builder.with_no_client_auth(),
        _ => {
            return Err(anyhow!(
                "both client certificate and private key are required for mTLS"
            ));
        }
    };

    rustls_config.alpn_protocols = tls
        .alpn_protocols
        .iter()
        .map(|value| value.as_bytes().to_vec())
        .collect();
    if tls.insecure_skip_verify {
        rustls_config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertificateVerification));
    }
    Ok(TlsConfiguration::Rustls(Arc::new(rustls_config)))
}

fn broker_address_for_config(config: &BenchConfig) -> String {
    if !config.protocol.is_websocket() {
        return config.host.clone();
    }

    let host = config.host.trim();
    if host.starts_with("ws://") || host.starts_with("wss://") {
        return host.to_string();
    }

    let path = normalize_websocket_path(config.websocket_path.as_deref());
    let host = if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    format!(
        "{}://{}:{}{}",
        config.protocol.as_str(),
        host,
        config.port,
        path
    )
}

fn launch_delay(config: &BenchConfig, clock: Option<&LoadClock>) -> Option<Duration> {
    if let Some(clock) = clock {
        let rate = clock.instant_rate(Instant::now());
        return Some(LoadClock::interval_for_rate(rate));
    }
    if config.client_interval_ms > 0 {
        Some(Duration::from_millis(config.client_interval_ms))
    } else if config.connect_rate > 0 {
        Some(Duration::from_secs_f64(1.0 / config.connect_rate as f64))
    } else {
        None
    }
}

fn publish_delay(config: &BenchConfig, clock: Option<&LoadClock>) -> Duration {
    if let Some(clock) = clock {
        return LoadClock::interval_for_rate(clock.instant_rate(Instant::now()));
    }
    Duration::from_millis(config.message_interval_ms.max(1))
}

fn config_from_workload(workload: &Workload) -> BenchConfig {
    let mut config = BenchConfig::default();
    config.mode = match workload.kind {
        WorkloadKind::Conn => BenchMode::Conn,
        WorkloadKind::Sub => BenchMode::Sub,
        WorkloadKind::Pub => BenchMode::Pub,
    };
    config.clients = workload.clients as usize;
    config.start_number = workload.start_number;
    config.client_id_template = workload.client_id_template.clone();
    config.topic = workload.topics.topic_template.clone();
    config.qos = workload.qos;
    config.retain = workload.retain;
    config.connect_rate = workload.load.connect_shape.instant_rate(0).max(0.0).round() as u32;
    config.message_interval_ms =
        interval_ms_for_rate(workload.load.message_shape.instant_rate(0)).max(1);
    config.duration_secs = workload.load.total_duration_ms / 1000;
    config.sample_interval_ms = workload.sample_interval_ms;
    config.network_bind_mode = workload.network_bind_mode.clone();
    config.bind_interfaces = workload.bind_interfaces.clone();
    config
}

fn interval_ms_for_rate(rate: f64) -> u64 {
    if rate <= 0.0 {
        1000
    } else {
        (1000.0 / rate).round().max(1.0) as u64
    }
}

fn create_specimen(
    run_id: String,
    config: BenchConfig,
    request: StartBenchRequest,
    created_at: chrono::DateTime<Utc>,
) -> Result<BenchSpecimen, String> {
    let fallback_name = format!(
        "{} {}:{} {}",
        config.mode.as_str(),
        config.host,
        config.port,
        created_at.format("%Y-%m-%d %H:%M:%S")
    );
    let draft = request.specimen.normalized(fallback_name);
    draft.validate()?;

    Ok(BenchSpecimen {
        id: Uuid::new_v4().to_string(),
        run_id,
        name: draft
            .name
            .unwrap_or_else(|| "benchmark specimen".to_string()),
        description: draft.description.unwrap_or_default(),
        tags: draft.tags,
        config,
        created_at,
    })
}

fn build_payload(size: usize, index: u64, timestamp_ns: Option<i64>) -> Vec<u8> {
    if size == 0 && timestamp_ns.is_none() {
        return Vec::new();
    }

    let seed = format!("velamq-bench:{index}:");
    let timestamp = timestamp_ns.map(|value| format!("velamq-ts-ns={value};"));
    let target_size = timestamp
        .as_ref()
        .map(|value| size.max(value.len()))
        .unwrap_or(size);
    let mut payload = Vec::with_capacity(target_size);
    if let Some(timestamp) = timestamp {
        payload.extend_from_slice(timestamp.as_bytes());
    }
    while payload.len() < size {
        payload.extend_from_slice(seed.as_bytes());
    }
    payload.truncate(target_size);
    payload
}

fn parse_payload_timestamp(payload: &[u8]) -> Option<i64> {
    let timestamp = payload.strip_prefix(TIMESTAMP_PREFIX)?;
    let end = timestamp
        .iter()
        .position(|byte| *byte == TIMESTAMP_SUFFIX)?;
    std::str::from_utf8(&timestamp[..end]).ok()?.parse().ok()
}

fn latency_since(sent_ns: i64) -> Option<Duration> {
    let now_ns = unix_timestamp_nanos();
    let latency_ns = now_ns.checked_sub(sent_ns)?;
    if latency_ns < 0 {
        return None;
    }
    Some(Duration::from_nanos(latency_ns as u64))
}

fn unix_timestamp_nanos() -> i64 {
    Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_micros() * 1000)
}

fn qos(qos: QosLevel) -> QoS {
    match qos {
        QosLevel::Qos0 => QoS::AtMostOnce,
        QosLevel::Qos1 => QoS::AtLeastOnce,
        QosLevel::Qos2 => QoS::ExactlyOnce,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_payload_round_trips() {
        let payload = build_payload(64, 7, Some(1_775_000_000_000_000_000));
        assert_eq!(
            parse_payload_timestamp(&payload),
            Some(1_775_000_000_000_000_000)
        );
        assert_eq!(payload.len(), 64);
    }

    #[test]
    fn timestamp_payload_grows_when_size_is_too_small() {
        let payload = build_payload(4, 7, Some(1_775_000_000_000_000_000));
        assert_eq!(
            parse_payload_timestamp(&payload),
            Some(1_775_000_000_000_000_000)
        );
        assert!(payload.len() > 4);
    }

    #[test]
    fn mqtt5_options_apply_connect_properties() {
        let config = BenchConfig {
            mqtt_version: MqttVersion::V5_0,
            connection_timeout_secs: 12,
            clean_session: false,
            mqtt5: Some(crate::model::Mqtt5Config {
                session_expiry_interval_secs: Some(3600),
                receive_maximum: Some(100),
                maximum_packet_size: Some(1_048_576),
                topic_alias_maximum: Some(16),
                request_problem_information: true,
            }),
            ..BenchConfig::default()
        };

        let options = mqtt5_options_for_client("mqtt5-test".to_string(), &config, None).unwrap();
        assert_eq!(options.connection_timeout(), 12);
        assert_eq!(options.session_expiry_interval(), Some(3600));
        assert_eq!(options.receive_maximum(), Some(100));
        assert_eq!(options.max_packet_size(), Some(1_048_576));
        assert_eq!(options.topic_alias_max(), Some(16));
        assert_eq!(options.request_problem_info(), Some(1));
    }

    #[test]
    fn tls_configuration_accepts_system_roots_and_debug_verifier() {
        let config = BenchConfig {
            protocol: BrokerProtocol::Mqtts,
            tls: Some(crate::model::TlsConfig {
                enabled: true,
                insecure_skip_verify: true,
                alpn_protocols: vec!["mqtt".to_string()],
                ..Default::default()
            }),
            ..BenchConfig::default()
        };
        assert!(matches!(
            tls_configuration(&config),
            Ok(TlsConfiguration::Rustls(_))
        ));
    }

    #[test]
    fn non_flat_workload_creates_legacy_runtime_config() {
        let workload = Workload {
            id: "workload-a".to_string(),
            name: "ramp pub".to_string(),
            kind: WorkloadKind::Pub,
            broker_profile_id: "broker-a".to_string(),
            payload_profile_id: Some("payload-a".to_string()),
            clients: 12,
            start_number: 5,
            client_id_template: "client-{i}".to_string(),
            topics: crate::model::TopicDistribution {
                topic_template: "topic/{i}".to_string(),
                partitions: 1,
                group_strategy: crate::model::TopicGroupStrategy::ClientId,
            },
            qos: QosLevel::Qos1,
            retain: true,
            load: LoadProfile {
                connect_shape: LoadShape::Ramp {
                    from: 2.0,
                    to: 10.0,
                    duration_ms: 1000,
                },
                message_shape: LoadShape::Spike {
                    baseline: 4.0,
                    peak: 20.0,
                    peak_duration_ms: 100,
                    period_ms: 1000,
                },
                total_duration_ms: 3000,
            },
            network_bind_mode: NetworkBindMode::System,
            bind_interfaces: Vec::new(),
            sample_interval_ms: 500,
        };

        let config = config_from_workload(&workload);

        assert_eq!(config.mode, BenchMode::Pub);
        assert_eq!(config.clients, 12);
        assert_eq!(config.start_number, 5);
        assert_eq!(config.connect_rate, 2);
        assert_eq!(config.message_interval_ms, 50);
        assert_eq!(config.duration_secs, 3);
        assert_eq!(config.qos, QosLevel::Qos1);
        assert!(config.retain);
    }
}

fn status_name(status: &BenchStatus) -> &'static str {
    match status {
        BenchStatus::Idle => "idle",
        BenchStatus::Starting => "starting",
        BenchStatus::Running => "running",
        BenchStatus::Stopping => "stopping",
        BenchStatus::Completed => "completed",
        BenchStatus::Stopped => "stopped",
        BenchStatus::Failed => "failed",
    }
}

fn list_network_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
    let mut interfaces = BTreeMap::<String, InterfaceAccumulator>::new();

    for interface in get_if_addrs()? {
        let (address, loopback) = match interface.addr {
            IfAddr::V4(addr) => (addr.ip.to_string(), addr.ip.is_loopback()),
            IfAddr::V6(addr) => (addr.ip.to_string(), addr.ip.is_loopback()),
        };

        let entry = interfaces
            .entry(interface.name)
            .or_insert_with(|| InterfaceAccumulator {
                addresses: Vec::new(),
                loopback: true,
            });
        if !entry.addresses.iter().any(|value| value == &address) {
            entry.addresses.push(address);
        }
        entry.loopback &= loopback;
    }

    Ok(interfaces
        .into_iter()
        .map(|(name, mut interface)| {
            interface.addresses.sort();
            NetworkInterfaceInfo {
                name,
                addresses: interface.addresses,
                loopback: interface.loopback,
                bind_supported: network_bind_supported(),
            }
        })
        .collect())
}

fn network_bind_supported() -> bool {
    cfg!(any(
        target_os = "android",
        target_os = "fuchsia",
        target_os = "linux"
    ))
}

fn network_bind_mode_label(mode: &NetworkBindMode) -> &'static str {
    match mode {
        NetworkBindMode::System => "system",
        NetworkBindMode::AutoRandom => "auto_random",
        NetworkBindMode::AutoRoundRobin => "auto_round_robin",
        NetworkBindMode::ManualRandom => "manual_random",
        NetworkBindMode::ManualRoundRobin => "manual_round_robin",
    }
}
