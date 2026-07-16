CREATE TABLE IF NOT EXISTS agent_tasks (
    id TEXT PRIMARY KEY,
    distributed_run_id TEXT,
    node_id TEXT NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 0,
    idempotency_key TEXT NOT NULL UNIQUE,
    spec_json TEXT NOT NULL,
    status TEXT NOT NULL,
    lease_id TEXT,
    lease_expires_at TEXT,
    stop_requested INTEGER NOT NULL DEFAULT 0,
    started_at TEXT,
    finished_at TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(node_id) REFERENCES agent_nodes(id)
);

CREATE INDEX IF NOT EXISTS idx_agent_tasks_node_status ON agent_tasks(node_id, status, created_at);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_distributed_run ON agent_tasks(distributed_run_id, created_at);
