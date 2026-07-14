CREATE TABLE IF NOT EXISTS annotations (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    run_workload_id TEXT,
    ts TEXT NOT NULL,
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(run_id) REFERENCES runs_v2(id)
);

CREATE INDEX IF NOT EXISTS idx_annotations_run_ts ON annotations(run_id, ts);
