use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::WorkloadKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub scenario_id: Option<String>,
    pub name: String,
    pub tags: Vec<String>,
    pub description: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub workloads: Vec<RunWorkload>,
    pub baseline_of_scenario_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunWorkload {
    pub id: String,
    pub run_id: String,
    pub workload_id: String,
    pub kind: WorkloadKind,
    pub config_snapshot_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub run_id: String,
    pub run_workload_id: Option<String>,
    pub ts: DateTime<Utc>,
    pub category: AnnotationCategory,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationCategory {
    Manual,
    BrokerEvent,
    SlaBreach,
    ConfigChange,
}
