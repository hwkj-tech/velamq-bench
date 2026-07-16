CREATE TABLE IF NOT EXISTS distributed_runs (
    id TEXT PRIMARY KEY,
    scenario_id TEXT NOT NULL,
    name TEXT NOT NULL,
    scenario_snapshot_json TEXT NOT NULL,
    strategy TEXT NOT NULL,
    node_ids_json TEXT NOT NULL,
    required_labels_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    started_at TEXT,
    stopped_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_distributed_runs_created ON distributed_runs(created_at DESC);
