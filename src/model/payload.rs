use std::{collections::BTreeMap, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PayloadKind {
    FixedBytes {
        size: usize,
        with_timestamp: bool,
    },
    JsonTemplate {
        template: String,
        vars: BTreeMap<String, String>,
    },
    CsvReplay {
        path: PathBuf,
        column: String,
        loop_when_done: bool,
    },
    Counter {
        width: usize,
    },
}

impl Default for PayloadKind {
    fn default() -> Self {
        Self::FixedBytes {
            size: 256,
            with_timestamp: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PayloadProfile {
    pub id: String,
    pub name: String,
    pub kind: PayloadKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for PayloadProfile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            kind: PayloadKind::default(),
            created_at: now,
            updated_at: now,
        }
    }
}
