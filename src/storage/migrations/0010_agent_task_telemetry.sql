CREATE TABLE IF NOT EXISTS agent_task_metrics (
    task_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    received_at TEXT NOT NULL,
    PRIMARY KEY(task_id, sequence),
    FOREIGN KEY(task_id) REFERENCES agent_tasks(id)
);

CREATE TABLE IF NOT EXISTS agent_task_logs (
    task_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    run_workload_id TEXT,
    log_json TEXT NOT NULL,
    received_at TEXT NOT NULL,
    PRIMARY KEY(task_id, sequence),
    FOREIGN KEY(task_id) REFERENCES agent_tasks(id)
);

CREATE INDEX IF NOT EXISTS idx_agent_task_metrics_task ON agent_task_metrics(task_id, sequence);
CREATE INDEX IF NOT EXISTS idx_agent_task_logs_task ON agent_task_logs(task_id, sequence);
