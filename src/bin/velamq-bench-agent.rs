use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;
use velamq_bench::{
    bench::BenchManager,
    cluster::AGENT_PROTOCOL_VERSION,
    model::{
        AgentCapabilities, AgentHeartbeat, AgentLogBatch, AgentLogPoint, AgentMetricBatch,
        AgentMetricPoint, AgentRegistration, AgentRegistrationResponse, AgentTask, AgentTaskAck,
        AgentTaskComplete, AgentTaskControl, AgentTaskLease, AgentTaskStatus, BenchStatus,
    },
    runtime::sse::RunEvent,
    storage::Storage,
};

#[derive(Debug, Clone)]
struct AgentConfig {
    control_url: String,
    bootstrap_token: Option<String>,
    name: String,
    labels: Vec<String>,
    max_clients: u64,
    identity_path: PathBuf,
    data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentIdentity {
    instance_id: String,
    node_id: String,
    token: String,
}

struct AgentRuntime {
    config: AgentConfig,
    identity: AgentIdentity,
    capabilities: AgentCapabilities,
    client: Client,
    manager: Arc<BenchManager>,
    storage: Storage,
}

#[tokio::main]
async fn main() -> Result<()> {
    velamq_bench::install_crypto_provider();
    tracing_subscriber::fmt::init();
    let config = AgentConfig::from_env()?;
    tokio::fs::create_dir_all(&config.data_dir).await?;
    let client = Client::builder().timeout(Duration::from_secs(12)).build()?;
    let capabilities = capabilities(&config);
    let identity = load_or_register_identity(&config, &client, &capabilities).await?;
    let storage = Storage::new(config.data_dir.join("velamq-bench-agent.sqlite3")).await?;
    let manager = BenchManager::new(storage.clone());
    let runtime = AgentRuntime {
        config,
        identity,
        capabilities,
        client,
        manager,
        storage,
    };
    tracing::info!(node_id = %runtime.identity.node_id, "velamq-bench-agent connected to control plane");
    runtime.run().await
}

impl AgentConfig {
    fn from_env() -> Result<Self> {
        let control_url = std::env::var("VELAMQ_CONTROL_URL")
            .context("VELAMQ_CONTROL_URL is required")?
            .trim_end_matches('/')
            .to_string();
        let data_dir = std::env::var_os("VELAMQ_BENCH_AGENT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data/agent"));
        let identity_path = std::env::var_os("VELAMQ_BENCH_AGENT_IDENTITY")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("identity.json"));
        let name = std::env::var("VELAMQ_BENCH_AGENT_NAME").unwrap_or_else(|_| host_name());
        let labels = std::env::var("VELAMQ_BENCH_AGENT_LABELS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let max_clients = std::env::var("VELAMQ_BENCH_AGENT_MAX_CLIENTS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(50_000);
        Ok(Self {
            control_url,
            bootstrap_token: std::env::var("VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN").ok(),
            name,
            labels,
            max_clients,
            identity_path,
            data_dir,
        })
    }
}

impl AgentRuntime {
    async fn run(&self) -> Result<()> {
        loop {
            if let Err(err) = self.heartbeat(None, None).await {
                tracing::warn!(error = %err, "agent heartbeat failed");
                sleep(Duration::from_secs(3)).await;
                continue;
            }
            match self.next_task().await {
                Ok(Some(lease)) => {
                    tracing::info!(task_id = %lease.task.id, "leased remote benchmark task");
                    if let Err(err) = self.execute(lease).await {
                        tracing::error!(error = %err, "remote benchmark task failed");
                    }
                }
                Ok(None) => sleep(Duration::from_secs(2)).await,
                Err(err) => {
                    tracing::warn!(error = %err, "failed to poll remote task");
                    sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    async fn execute(&self, lease: AgentTaskLease) -> Result<()> {
        let task = lease.task;
        self.ack(&task, &lease.lease_id).await?;
        let result = self.execute_inner(&task, &lease.lease_id).await;
        let (status, error) = match &result {
            Ok(status) => (status.clone(), None),
            Err(err) => (AgentTaskStatus::Failed, Some(format!("{err:#}"))),
        };
        if let Err(err) = self.complete(&task, &lease.lease_id, status, error).await {
            tracing::error!(task_id = %task.id, error = %err, "failed to report task completion");
        }
        self.heartbeat(None, None).await.ok();
        result.map(|_| ())
    }

    async fn execute_inner(&self, task: &AgentTask, lease_id: &str) -> Result<AgentTaskStatus> {
        for profile in &task.spec.broker_profiles {
            self.storage.upsert_broker_profile(profile.clone()).await?;
        }
        for profile in &task.spec.payload_profiles {
            self.storage.upsert_payload_profile(profile.clone()).await?;
        }
        let mut events = self.manager.subscribe_run_events();
        let mut metric_sequence = 0_u64;
        let mut log_sequence = 0_u64;
        self.manager
            .start_ad_hoc_scenario(task.spec.scenario.clone())
            .await
            .map_err(|err| anyhow!(err))?;

        loop {
            sleep(Duration::from_secs(2)).await;
            self.heartbeat(Some(&task.id), Some(lease_id)).await?;
            self.flush_events(
                task,
                lease_id,
                &mut events,
                &mut metric_sequence,
                &mut log_sequence,
            )
            .await?;
            if self.control(&task.id).await?.stop_requested {
                self.manager.stop().await.map_err(|err| anyhow!(err))?;
            }
            let state = self.manager.state().await;
            match state.status {
                BenchStatus::Completed => return Ok(AgentTaskStatus::Completed),
                BenchStatus::Stopped => return Ok(AgentTaskStatus::Stopped),
                BenchStatus::Failed => return Err(anyhow!("local benchmark runtime failed")),
                _ => {}
            }
        }
    }

    async fn flush_events(
        &self,
        task: &AgentTask,
        lease_id: &str,
        events: &mut tokio::sync::broadcast::Receiver<RunEvent>,
        metric_sequence: &mut u64,
        log_sequence: &mut u64,
    ) -> Result<()> {
        let mut metrics = Vec::new();
        let mut logs = Vec::new();
        while metrics.len() < 500 && logs.len() < 1000 {
            match events.try_recv() {
                Ok(RunEvent::WorkloadMetric { snapshot, .. }) => {
                    *metric_sequence += 1;
                    metrics.push(AgentMetricPoint {
                        sequence: *metric_sequence,
                        snapshot,
                    });
                }
                Ok(RunEvent::WorkloadLog {
                    run_workload_id,
                    log,
                    ..
                }) => {
                    *log_sequence += 1;
                    logs.push(AgentLogPoint {
                        sequence: *log_sequence,
                        run_workload_id,
                        log,
                    });
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(task_id = %task.id, skipped, "agent telemetry buffer lagged");
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            }
        }
        if !metrics.is_empty() {
            self.authenticated(self.client.post(format!(
                "{}/api/v2/agent-tasks/{}/metrics",
                self.config.control_url, task.id
            )))
            .json(&AgentMetricBatch {
                lease_id: lease_id.to_string(),
                points: metrics,
            })
            .send()
            .await?
            .error_for_status()?;
        }
        if !logs.is_empty() {
            self.authenticated(self.client.post(format!(
                "{}/api/v2/agent-tasks/{}/logs",
                self.config.control_url, task.id
            )))
            .json(&AgentLogBatch {
                lease_id: lease_id.to_string(),
                points: logs,
            })
            .send()
            .await?
            .error_for_status()?;
        }
        Ok(())
    }

    async fn next_task(&self) -> Result<Option<AgentTaskLease>> {
        let response = self
            .authenticated(self.client.get(format!(
                "{}/api/v2/agents/{}/tasks/next",
                self.config.control_url, self.identity.node_id
            )))
            .send()
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let response = response.error_for_status()?;
        Ok(Some(response.json().await?))
    }

    async fn ack(&self, task: &AgentTask, lease_id: &str) -> Result<()> {
        self.authenticated(self.client.post(format!(
            "{}/api/v2/agent-tasks/{}/ack",
            self.config.control_url, task.id
        )))
        .json(&AgentTaskAck {
            lease_id: lease_id.to_string(),
        })
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    async fn heartbeat(&self, task_id: Option<&str>, lease_id: Option<&str>) -> Result<()> {
        self.authenticated(self.client.post(format!(
            "{}/api/v2/agents/{}/heartbeat",
            self.config.control_url, self.identity.node_id
        )))
        .json(&AgentHeartbeat {
            capabilities: Some(self.capabilities.clone()),
            current_task_id: task_id.map(ToOwned::to_owned),
            lease_id: lease_id.map(ToOwned::to_owned),
        })
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    async fn control(&self, task_id: &str) -> Result<AgentTaskControl> {
        Ok(self
            .authenticated(self.client.get(format!(
                "{}/api/v2/agent-tasks/{task_id}/control",
                self.config.control_url
            )))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn complete(
        &self,
        task: &AgentTask,
        lease_id: &str,
        status: AgentTaskStatus,
        error: Option<String>,
    ) -> Result<()> {
        self.authenticated(self.client.post(format!(
            "{}/api/v2/agent-tasks/{}/complete",
            self.config.control_url, task.id
        )))
        .json(&AgentTaskComplete {
            lease_id: lease_id.to_string(),
            status,
            error,
        })
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    fn authenticated(&self, request: RequestBuilder) -> RequestBuilder {
        request
            .bearer_auth(&self.identity.token)
            .header("x-velamq-agent-id", &self.identity.node_id)
            .header("x-velamq-protocol-version", AGENT_PROTOCOL_VERSION)
    }
}

async fn load_or_register_identity(
    config: &AgentConfig,
    client: &Client,
    capabilities: &AgentCapabilities,
) -> Result<AgentIdentity> {
    if let Ok(data) = tokio::fs::read(&config.identity_path).await {
        return serde_json::from_slice(&data).context("invalid agent identity file");
    }
    let bootstrap_token = config
        .bootstrap_token
        .as_deref()
        .context("VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN is required for first registration")?;
    let instance_id = Uuid::new_v4().to_string();
    let response = client
        .post(format!("{}/api/v2/agents/register", config.control_url))
        .bearer_auth(bootstrap_token)
        .json(&AgentRegistration {
            instance_id: instance_id.clone(),
            name: config.name.clone(),
            labels: config.labels.clone(),
            capabilities: capabilities.clone(),
        })
        .send()
        .await?
        .error_for_status()?
        .json::<AgentRegistrationResponse>()
        .await?;
    let identity = AgentIdentity {
        instance_id,
        node_id: response.node.id,
        token: response.token,
    };
    if let Some(parent) = config.identity_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&config.identity_path, serde_json::to_vec_pretty(&identity)?).await?;
    Ok(identity)
}

fn capabilities(config: &AgentConfig) -> AgentCapabilities {
    AgentCapabilities {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        cpu_cores: std::thread::available_parallelism()
            .map(|value| value.get() as u32)
            .unwrap_or(1),
        memory_bytes: 0,
        max_clients: config.max_clients,
        features: vec![
            "mqtt3".to_string(),
            "mqtt5".to_string(),
            "tls".to_string(),
            "websocket".to_string(),
        ],
    }
}

fn host_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| format!("velamq-bench-agent-{}", Uuid::new_v4().simple()))
}
