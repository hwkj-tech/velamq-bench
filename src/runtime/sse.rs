use serde::{Deserialize, Serialize};

use crate::model::{Annotation, LogLine, MetricSnapshot, RuntimeView};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RunEvent {
    RunStateChanged {
        run_id: String,
        run: RuntimeView,
    },
    WorkloadMetric {
        run_id: String,
        run_workload_id: String,
        snapshot: MetricSnapshot,
    },
    WorkloadLog {
        run_id: String,
        run_workload_id: Option<String>,
        log: LogLine,
    },
    RunAnnotation {
        run_id: String,
        annotation: Annotation,
    },
}

impl RunEvent {
    pub fn run_id(&self) -> &str {
        match self {
            Self::RunStateChanged { run_id, .. }
            | Self::WorkloadMetric { run_id, .. }
            | Self::WorkloadLog { run_id, .. }
            | Self::RunAnnotation { run_id, .. } => run_id,
        }
    }

    pub fn event_name(&self) -> &'static str {
        match self {
            Self::RunStateChanged { .. } => "run_state",
            Self::WorkloadMetric { .. } => "workload_metric",
            Self::WorkloadLog { .. } => "workload_log",
            Self::RunAnnotation { .. } => "annotation",
        }
    }
}
