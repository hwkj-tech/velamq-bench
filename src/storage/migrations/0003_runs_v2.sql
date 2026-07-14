CREATE TABLE IF NOT EXISTS runs_v2 (
    id TEXT PRIMARY KEY,
    scenario_id TEXT,
    name TEXT NOT NULL,
    tags_json TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    stopped_at TEXT,
    legacy_run_id TEXT,
    FOREIGN KEY(scenario_id) REFERENCES scenarios(id)
);

CREATE TABLE IF NOT EXISTS run_workloads (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    config_snapshot_json TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES runs_v2(id)
);

ALTER TABLE metric_snapshots ADD COLUMN run_workload_id TEXT;
CREATE INDEX IF NOT EXISTS idx_metric_snapshots_workload ON metric_snapshots(run_workload_id, ts);
