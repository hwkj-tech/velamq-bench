use serde::{Deserialize, Serialize};

use super::{LoadProfile, NetworkBindMode, QosLevel};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadKind {
    Pub,
    Sub,
    Conn,
}

impl Default for WorkloadKind {
    fn default() -> Self {
        Self::Pub
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TopicDistribution {
    pub topic_template: String,
    pub partitions: u32,
    pub group_strategy: TopicGroupStrategy,
}

impl Default for TopicDistribution {
    fn default() -> Self {
        Self {
            topic_template: "velamq/bench/{i}".to_string(),
            partitions: 1,
            group_strategy: TopicGroupStrategy::ClientId,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopicGroupStrategy {
    RoundRobin,
    Hash,
    ClientId,
}

impl Default for TopicGroupStrategy {
    fn default() -> Self {
        Self::ClientId
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Workload {
    pub id: String,
    pub name: String,
    pub kind: WorkloadKind,
    pub broker_profile_id: String,
    pub payload_profile_id: Option<String>,
    pub clients: u64,
    pub start_number: u64,
    pub client_id_template: String,
    pub topics: TopicDistribution,
    pub qos: QosLevel,
    pub retain: bool,
    pub load: LoadProfile,
    pub network_bind_mode: NetworkBindMode,
    pub bind_interfaces: Vec<String>,
    pub sample_interval_ms: u64,
}

impl Default for Workload {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: WorkloadKind::Pub,
            broker_profile_id: String::new(),
            payload_profile_id: None,
            clients: 100,
            start_number: 1,
            client_id_template: "velamq-{mode}-{i}".to_string(),
            topics: TopicDistribution::default(),
            qos: QosLevel::Qos0,
            retain: false,
            load: LoadProfile::default(),
            network_bind_mode: NetworkBindMode::System,
            bind_interfaces: Vec::new(),
            sample_interval_ms: 1000,
        }
    }
}
