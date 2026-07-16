use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{BrokerProfile, LogLine, MetricSnapshot, PayloadProfile, Scenario};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Online,
    Busy,
    Draining,
    Offline,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AgentCapabilities {
    pub os: String,
    pub arch: String,
    pub version: String,
    pub cpu_cores: u32,
    pub memory_bytes: u64,
    pub max_clients: u64,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub id: String,
    pub instance_id: String,
    pub name: String,
    pub status: AgentStatus,
    pub enabled: bool,
    pub draining: bool,
    pub labels: Vec<String>,
    pub capabilities: AgentCapabilities,
    pub current_task_id: Option<String>,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub instance_id: String,
    pub name: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub capabilities: AgentCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistrationResponse {
    pub node: AgentNode,
    pub token: String,
    pub heartbeat_interval_secs: u64,
    pub protocol_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AgentHeartbeat {
    pub capabilities: Option<AgentCapabilities>,
    pub current_task_id: Option<String>,
    pub lease_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AgentNodeUpdate {
    pub name: Option<String>,
    pub labels: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub draining: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Queued,
    Leased,
    Running,
    Completed,
    Failed,
    Stopped,
    Expired,
}

impl AgentTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Leased => "leased",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Expired => "expired",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "leased" => Self::Leased,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "stopped" => Self::Stopped,
            "expired" => Self::Expired,
            _ => Self::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskSpec {
    pub scenario: Scenario,
    #[serde(default)]
    pub broker_profiles: Vec<BrokerProfile>,
    #[serde(default)]
    pub payload_profiles: Vec<PayloadProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub distributed_run_id: Option<String>,
    pub node_id: String,
    pub attempt: u32,
    pub idempotency_key: String,
    pub spec: AgentTaskSpec,
    pub status: AgentTaskStatus,
    pub lease_id: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub stop_requested: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskCreate {
    pub node_id: String,
    pub distributed_run_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub spec: AgentTaskSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskLease {
    pub task: AgentTask,
    pub lease_id: String,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskAck {
    pub lease_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskComplete {
    pub lease_id: String,
    pub status: AgentTaskStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskControl {
    pub stop_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingStrategy {
    Selected,
    Even,
    CapacityWeighted,
}

impl SchedulingStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::Even => "even",
            Self::CapacityWeighted => "capacity_weighted",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "selected" => Self::Selected,
            "capacity_weighted" => Self::CapacityWeighted,
            _ => Self::Even,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DistributedRunStatus {
    Pending,
    Running,
    Completed,
    Partial,
    Failed,
    Stopped,
}

impl DistributedRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "partial" => Self::Partial,
            "failed" => Self::Failed,
            "stopped" => Self::Stopped,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedRun {
    pub id: String,
    pub scenario_id: String,
    pub name: String,
    pub strategy: SchedulingStrategy,
    pub node_ids: Vec<String>,
    pub required_labels: Vec<String>,
    pub status: DistributedRunStatus,
    pub tasks: Vec<AgentTask>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedRunCreate {
    pub scenario_id: String,
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub required_labels: Vec<String>,
    pub strategy: SchedulingStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricPoint {
    pub sequence: u64,
    pub snapshot: MetricSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricBatch {
    pub lease_id: String,
    pub points: Vec<AgentMetricPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogPoint {
    pub sequence: u64,
    pub run_workload_id: Option<String>,
    pub log: LogLine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogBatch {
    pub lease_id: String,
    pub points: Vec<AgentLogPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskMetrics {
    pub task_id: String,
    pub node_id: String,
    pub snapshots: Vec<MetricSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedMetrics {
    pub run_id: String,
    pub summary: Vec<MetricSnapshot>,
    pub nodes: Vec<AgentTaskMetrics>,
}

impl AgentRegistration {
    pub fn validate(&self) -> Result<(), String> {
        if self.instance_id.trim().is_empty() || self.instance_id.len() > 128 {
            return Err("agent instance_id is required and must be <= 128 characters".to_string());
        }
        if self.name.trim().is_empty() || self.name.len() > 120 {
            return Err("agent name is required and must be <= 120 characters".to_string());
        }
        validate_labels(&self.labels)?;
        if self.capabilities.max_clients > 1_000_000 {
            return Err("agent max_clients must be <= 1000000".to_string());
        }
        Ok(())
    }
}

pub fn normalize_labels(labels: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    for label in labels {
        let label = label.trim().to_ascii_lowercase();
        if !label.is_empty() && !output.contains(&label) {
            output.push(label);
        }
    }
    output.sort();
    output
}

pub fn validate_labels(labels: &[String]) -> Result<(), String> {
    if labels.len() > 32 {
        return Err("agent labels must contain <= 32 entries".to_string());
    }
    if labels.iter().any(|label| {
        label.is_empty()
            || label.len() > 64
            || !label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '='))
    }) {
        return Err("agent labels contain an invalid value".to_string());
    }
    Ok(())
}
