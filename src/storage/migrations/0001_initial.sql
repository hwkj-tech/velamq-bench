CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    mode TEXT NOT NULL,
    config_json TEXT NOT NULL,
    started_at TEXT NOT NULL,
    stopped_at TEXT
);

CREATE TABLE IF NOT EXISTS metric_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    ts TEXT NOT NULL,
    elapsed_ms INTEGER NOT NULL,
    connected INTEGER NOT NULL,
    published INTEGER NOT NULL,
    received INTEGER NOT NULL,
    errors INTEGER NOT NULL,
    publish_rate REAL NOT NULL,
    receive_rate REAL NOT NULL,
    connect_rate REAL NOT NULL,
    error_rate REAL NOT NULL,
    latency_count INTEGER NOT NULL DEFAULT 0,
    latency_avg_ms REAL NOT NULL DEFAULT 0,
    latency_min_ms REAL NOT NULL DEFAULT 0,
    latency_p50_ms REAL NOT NULL DEFAULT 0,
    latency_p90_ms REAL NOT NULL DEFAULT 0,
    latency_p95_ms REAL NOT NULL DEFAULT 0,
    latency_p99_ms REAL NOT NULL DEFAULT 0,
    latency_p999_ms REAL NOT NULL DEFAULT 0,
    latency_max_ms REAL NOT NULL DEFAULT 0,
    FOREIGN KEY(run_id) REFERENCES runs(id)
);

CREATE TABLE IF NOT EXISTS bench_specimens (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    config_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES runs(id)
);

CREATE TABLE IF NOT EXISTS bench_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    config_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_metric_snapshots_run_ts
    ON metric_snapshots(run_id, ts);

CREATE INDEX IF NOT EXISTS idx_bench_specimens_created_at
    ON bench_specimens(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_bench_templates_updated_at
    ON bench_templates(updated_at DESC);
