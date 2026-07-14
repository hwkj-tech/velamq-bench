use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchMode {
    Conn,
    Sub,
    Pub,
}

impl BenchMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Conn => "conn",
            Self::Sub => "sub",
            Self::Pub => "pub",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QosLevel {
    Qos0,
    Qos1,
    Qos2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkBindMode {
    System,
    AutoRandom,
    AutoRoundRobin,
    ManualRandom,
    ManualRoundRobin,
}

impl NetworkBindMode {
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::System)
    }

    pub fn is_manual(&self) -> bool {
        matches!(self, Self::ManualRandom | Self::ManualRoundRobin)
    }

    pub fn is_random(&self) -> bool {
        matches!(self, Self::AutoRandom | Self::ManualRandom)
    }
}

impl Default for NetworkBindMode {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BenchConfig {
    pub mode: BenchMode,
    pub protocol: super::BrokerProtocol,
    pub host: String,
    pub port: u16,
    pub websocket_path: Option<String>,
    pub clients: usize,
    pub start_number: u64,
    pub connect_rate: u32,
    pub client_interval_ms: u64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub keepalive_secs: u16,
    pub clean_session: bool,
    pub client_id_template: String,
    pub topic: String,
    pub qos: QosLevel,
    pub retain: bool,
    pub payload_size: usize,
    pub payload_timestamp: bool,
    pub message_interval_ms: u64,
    pub duration_secs: u64,
    pub sample_interval_ms: u64,
    pub network_bind_mode: NetworkBindMode,
    pub bind_interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SpecimenDraft {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl SpecimenDraft {
    pub fn normalized(self, fallback_name: String) -> Self {
        let name = self.name.and_then(|value| {
            let value = value.trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        });
        let description = self.description.map(|value| value.trim().to_string());
        let tags = self
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .take(32)
            .collect();

        Self {
            name: Some(name.unwrap_or(fallback_name)),
            description,
            tags,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.as_deref().unwrap_or_default().len() > 120 {
            return Err("specimen name must be <= 120 characters".to_string());
        }
        if self.description.as_deref().unwrap_or_default().len() > 2000 {
            return Err("specimen description must be <= 2000 characters".to_string());
        }
        if self.tags.iter().any(|tag| tag.len() > 64) {
            return Err("specimen tags must be <= 64 characters each".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StartBenchRequest {
    #[serde(flatten)]
    pub config: BenchConfig,
    pub specimen: SpecimenDraft,
}

impl Default for StartBenchRequest {
    fn default() -> Self {
        Self {
            config: BenchConfig::default(),
            specimen: SpecimenDraft::default(),
        }
    }
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            mode: BenchMode::Pub,
            protocol: super::BrokerProtocol::Mqtt,
            host: "127.0.0.1".to_string(),
            port: 1883,
            websocket_path: None,
            clients: 100,
            start_number: 1,
            connect_rate: 100,
            client_interval_ms: 0,
            username: None,
            password: None,
            keepalive_secs: 30,
            clean_session: true,
            client_id_template: "velamq-{mode}-{i}".to_string(),
            topic: "velamq/bench/{i}".to_string(),
            qos: QosLevel::Qos0,
            retain: false,
            payload_size: 256,
            payload_timestamp: true,
            message_interval_ms: 1000,
            duration_secs: 60,
            sample_interval_ms: 1000,
            network_bind_mode: NetworkBindMode::System,
            bind_interfaces: Vec::new(),
        }
    }
}

impl BenchConfig {
    pub fn normalized(mut self) -> Self {
        if self.host.trim().is_empty() {
            self.host = "127.0.0.1".to_string();
        }
        if self.clients == 0 {
            self.clients = 1;
        }
        if self.port == 0 {
            self.port = self.protocol.default_port();
        }
        if self.protocol.is_websocket() {
            self.websocket_path = Some(normalize_websocket_path(self.websocket_path.as_deref()));
        } else {
            self.websocket_path = None;
        }
        if self.keepalive_secs == 0 {
            self.keepalive_secs = 30;
        }
        if self.client_id_template.trim().is_empty() {
            self.client_id_template = "velamq-{mode}-{i}".to_string();
        }
        if self.topic.trim().is_empty() {
            self.topic = "velamq/bench/{i}".to_string();
        }
        if self.payload_timestamp && self.payload_size < 40 {
            self.payload_size = 40;
        }
        if self.message_interval_ms == 0 {
            self.message_interval_ms = 1;
        }
        if self.sample_interval_ms < 250 {
            self.sample_interval_ms = 250;
        }
        self.bind_interfaces = normalize_bind_interfaces(self.bind_interfaces);
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.clients > 100_000 {
            return Err("clients must be <= 100000".to_string());
        }
        if self.payload_size > 1024 * 1024 {
            return Err("payload_size must be <= 1048576".to_string());
        }
        if self.keepalive_secs > 3600 {
            return Err("keepalive_secs must be <= 3600".to_string());
        }
        if self.sample_interval_ms > 60_000 {
            return Err("sample_interval_ms must be <= 60000".to_string());
        }
        if self.network_bind_mode.is_manual() && self.bind_interfaces.is_empty() {
            return Err("bind_interfaces is required for manual network binding".to_string());
        }
        if self.bind_interfaces.len() > 64 {
            return Err("bind_interfaces must contain <= 64 entries".to_string());
        }
        for interface in &self.bind_interfaces {
            if interface.len() > 64 || !is_valid_interface_name(interface) {
                return Err("bind_interfaces contains an invalid interface name".to_string());
            }
        }
        Ok(())
    }

    pub fn client_id_for(&self, index: u64) -> String {
        render_template(&self.client_id_template, self.mode.as_str(), index)
    }

    pub fn topic_for(&self, index: u64) -> String {
        render_template(&self.topic, self.mode.as_str(), index)
    }

    pub fn to_workload(&self, broker_id: &str, payload_id: Option<&str>) -> super::Workload {
        super::Workload {
            id: format!("wl-{}", uuid::Uuid::new_v4()),
            name: format!("{} workload", self.mode.as_str()),
            kind: match self.mode {
                BenchMode::Conn => super::WorkloadKind::Conn,
                BenchMode::Sub => super::WorkloadKind::Sub,
                BenchMode::Pub => super::WorkloadKind::Pub,
            },
            broker_profile_id: broker_id.to_string(),
            payload_profile_id: payload_id.map(ToOwned::to_owned),
            clients: self.clients as u64,
            start_number: self.start_number,
            client_id_template: self.client_id_template.clone(),
            topics: super::TopicDistribution {
                topic_template: self.topic.clone(),
                partitions: 1,
                group_strategy: super::TopicGroupStrategy::ClientId,
            },
            qos: self.qos,
            retain: self.retain,
            load: super::LoadProfile {
                connect_shape: super::LoadShape::Flat {
                    rate: self.connect_rate as f64,
                },
                message_shape: super::LoadShape::Flat {
                    rate: interval_ms_to_rate(self.message_interval_ms),
                },
                total_duration_ms: self.duration_secs.saturating_mul(1000),
            },
            network_bind_mode: self.network_bind_mode.clone(),
            bind_interfaces: self.bind_interfaces.clone(),
            sample_interval_ms: self.sample_interval_ms,
        }
    }
}

impl super::Workload {
    pub fn flatten_to_legacy(&self) -> Option<BenchConfig> {
        let super::LoadShape::Flat { rate: connect_rate } = self.load.connect_shape else {
            return None;
        };
        let super::LoadShape::Flat { rate: message_rate } = self.load.message_shape else {
            return None;
        };

        let mut config = BenchConfig::default();
        config.mode = match self.kind {
            super::WorkloadKind::Conn => BenchMode::Conn,
            super::WorkloadKind::Sub => BenchMode::Sub,
            super::WorkloadKind::Pub => BenchMode::Pub,
        };
        config.clients = self.clients as usize;
        config.start_number = self.start_number;
        config.connect_rate = connect_rate.max(0.0).round() as u32;
        config.client_id_template = self.client_id_template.clone();
        config.topic = self.topics.topic_template.clone();
        config.qos = self.qos;
        config.retain = self.retain;
        config.message_interval_ms = rate_to_interval_ms(message_rate);
        config.duration_secs = self.load.total_duration_ms / 1000;
        config.sample_interval_ms = self.sample_interval_ms;
        config.network_bind_mode = self.network_bind_mode.clone();
        config.bind_interfaces = self.bind_interfaces.clone();
        Some(config)
    }
}

fn interval_ms_to_rate(interval_ms: u64) -> f64 {
    if interval_ms == 0 {
        0.0
    } else {
        1000.0 / interval_ms as f64
    }
}

fn rate_to_interval_ms(rate: f64) -> u64 {
    if rate <= 0.0 {
        0
    } else {
        (1000.0 / rate).round().max(1.0) as u64
    }
}

pub fn normalize_websocket_path(path: Option<&str>) -> String {
    let path = path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("/mqtt");
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn normalize_bind_interfaces(interfaces: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for interface in interfaces {
        let interface = interface.trim();
        if interface.is_empty() || !is_valid_interface_name(interface) {
            continue;
        }
        if !normalized.iter().any(|value| value == interface) {
            normalized.push(interface.to_string());
        }
    }
    normalized
}

fn is_valid_interface_name(interface: &str) -> bool {
    !interface.is_empty()
        && interface
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
}

fn render_template(template: &str, mode: &str, index: u64) -> String {
    template
        .replace("{i}", &index.to_string())
        .replace("%i", &index.to_string())
        .replace("{mode}", mode)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchStatus {
    Idle,
    Starting,
    Running,
    Stopping,
    Completed,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub run_id: String,
    #[serde(default)]
    pub run_workload_id: Option<String>,
    pub ts: DateTime<Utc>,
    pub elapsed_ms: u64,
    pub connected: u64,
    pub published: u64,
    pub received: u64,
    pub errors: u64,
    pub publish_rate: f64,
    pub receive_rate: f64,
    pub connect_rate: f64,
    pub error_rate: f64,
    pub latency_count: u64,
    pub latency_avg_ms: f64,
    pub latency_min_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p90_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub latency_p999_ms: f64,
    pub latency_max_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRun {
    pub id: String,
    pub status: String,
    pub mode: String,
    pub config: BenchConfig,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub sample_count: u64,
    pub specimen: Option<BenchSpecimen>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSpecimen {
    pub id: String,
    pub run_id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub config: BenchConfig,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SpecimenUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

impl SpecimenUpdate {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.as_deref().unwrap_or_default().len() > 120 {
            return Err("specimen name must be <= 120 characters".to_string());
        }
        if self.description.as_deref().unwrap_or_default().len() > 2000 {
            return Err("specimen description must be <= 2000 characters".to_string());
        }
        if self
            .tags
            .as_ref()
            .map(|tags| tags.iter().any(|tag| tag.len() > 64))
            .unwrap_or(false)
        {
            return Err("specimen tags must be <= 64 characters each".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub config: BenchConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateDraft {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub config: BenchConfig,
}

impl Default for TemplateDraft {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            tags: Vec::new(),
            config: BenchConfig::default(),
        }
    }
}

impl TemplateDraft {
    pub fn normalized(self, fallback_name: String) -> Result<Self, String> {
        let name = self.name.and_then(|value| {
            let value = value.trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        });
        let description = self.description.map(|value| value.trim().to_string());
        let tags = self
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .take(32)
            .collect::<Vec<_>>();
        let config = self.config.normalized();
        config.validate()?;

        let normalized = Self {
            name: Some(name.unwrap_or(fallback_name)),
            description,
            tags,
            config,
        };
        normalized.validate()?;
        Ok(normalized)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.as_deref().unwrap_or_default().len() > 120 {
            return Err("template name must be <= 120 characters".to_string());
        }
        if self.description.as_deref().unwrap_or_default().len() > 2000 {
            return Err("template description must be <= 2000 characters".to_string());
        }
        if self.tags.iter().any(|tag| tag.len() > 64) {
            return Err("template tags must be <= 64 characters each".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunStats {
    pub duration_ms: u64,
    pub sample_count: u64,
    pub max_connected: u64,
    pub total_published: u64,
    pub total_received: u64,
    pub total_errors: u64,
    pub avg_publish_rate: f64,
    pub avg_receive_rate: f64,
    pub avg_connect_rate: f64,
    pub avg_error_rate: f64,
    pub max_publish_rate: f64,
    pub max_receive_rate: f64,
    pub max_connect_rate: f64,
    pub max_error_rate: f64,
    pub latency_count: u64,
    pub latency_avg_ms: f64,
    pub latency_min_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p90_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub latency_p999_ms: f64,
    pub latency_max_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub run: BenchRun,
    pub stats: RunStats,
    pub snapshots: Vec<MetricSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub addresses: Vec<String>,
    pub loopback: bool,
    pub bind_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub ts: DateTime<Utc>,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeView {
    pub status: BenchStatus,
    pub run_id: Option<String>,
    pub config: Option<BenchConfig>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub specimen: Option<BenchSpecimen>,
    pub latest: Option<MetricSnapshot>,
    pub logs: Vec<LogLine>,
}

impl Default for RuntimeView {
    fn default() -> Self {
        Self {
            status: BenchStatus::Idle,
            run_id: None,
            config: None,
            started_at: None,
            stopped_at: None,
            specimen: None,
            latest: None,
            logs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum BenchEvent {
    State(RuntimeView),
    Metrics(MetricSnapshot),
    Log(LogLine),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartResponse {
    pub run_id: String,
    pub specimen: BenchSpecimen,
    pub state: RuntimeView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_config_converts_to_workload_and_back_for_flat_load() {
        let mut config = BenchConfig::default();
        config.mode = BenchMode::Pub;
        config.clients = 42;
        config.start_number = 7;
        config.connect_rate = 21;
        config.client_id_template = "bench-{mode}-{i}".to_string();
        config.topic = "bench/topic/{i}".to_string();
        config.retain = true;
        config.message_interval_ms = 250;
        config.duration_secs = 30;
        config.sample_interval_ms = 500;
        config.network_bind_mode = NetworkBindMode::ManualRoundRobin;
        config.bind_interfaces = vec!["lo0".to_string()];

        let workload = config.to_workload("broker-a", Some("payload-a"));
        assert_eq!(workload.kind, crate::model::WorkloadKind::Pub);
        assert_eq!(workload.broker_profile_id, "broker-a");
        assert_eq!(workload.payload_profile_id.as_deref(), Some("payload-a"));
        assert_eq!(workload.clients, 42);

        let flattened = workload.flatten_to_legacy().unwrap();
        assert_eq!(flattened.mode, config.mode);
        assert_eq!(flattened.clients, config.clients);
        assert_eq!(flattened.start_number, config.start_number);
        assert_eq!(flattened.connect_rate, config.connect_rate);
        assert_eq!(flattened.client_id_template, config.client_id_template);
        assert_eq!(flattened.topic, config.topic);
        assert_eq!(flattened.retain, config.retain);
        assert_eq!(flattened.message_interval_ms, config.message_interval_ms);
        assert_eq!(flattened.duration_secs, config.duration_secs);
        assert_eq!(flattened.sample_interval_ms, config.sample_interval_ms);
        assert_eq!(flattened.network_bind_mode, config.network_bind_mode);
        assert_eq!(flattened.bind_interfaces, config.bind_interfaces);
    }

    #[test]
    fn bench_config_normalizes_websocket_path_and_default_port() {
        let mut config = BenchConfig {
            protocol: crate::model::BrokerProtocol::Ws,
            port: 0,
            websocket_path: Some("mqtt".to_string()),
            ..BenchConfig::default()
        };

        config = config.normalized();

        assert_eq!(config.port, 8083);
        assert_eq!(config.websocket_path.as_deref(), Some("/mqtt"));
    }

    #[test]
    fn bench_config_clears_websocket_path_for_tcp_protocols() {
        let config = BenchConfig {
            protocol: crate::model::BrokerProtocol::Mqtts,
            port: 0,
            websocket_path: Some("/mqtt".to_string()),
            ..BenchConfig::default()
        }
        .normalized();

        assert_eq!(config.port, 8883);
        assert_eq!(config.websocket_path, None);
    }
}
