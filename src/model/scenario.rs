use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Workload;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub stages: Vec<ScenarioStage>,
    pub baseline_run_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for Scenario {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            tags: Vec::new(),
            stages: Vec::new(),
            baseline_run_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStage {
    Parallel { workloads: Vec<Workload> },
    Sequential { workloads: Vec<Workload> },
}
