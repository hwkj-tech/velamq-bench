# PR-1：领域模型与存储迁移

## 目标

把现有 `BenchConfig + Specimen + Template` 拆为四类一等公民——`BrokerProfile` / `PayloadProfile` / `Workload` / `Scenario`，引入 `LoadProfile` 描述速率剖面，并落 sqlite migration。**本 PR 只改后端 model + storage，不改运行时与 API**（运行时改造放到 PR-2），保持现有 `/api/bench/*` 行为完全兼容。

## 前置依赖

无。

## 涉及文件

新增

- `src/devices/` 不存在；本仓库唯一的 crate 是 `velamq-bench`，所以新增都在 `src/` 顶层。
- `src/model/mod.rs`（新模块化拆分入口）。
- `src/model/broker.rs`：`BrokerProfile` / `TlsConfig` / `AuthConfig`。
- `src/model/payload.rs`：`PayloadProfile` 及 4 种 `PayloadKind` 变体。
- `src/model/load.rs`：`LoadProfile` / `LoadShape::{Flat,Ramp,Step,Soak,Spike}`。
- `src/model/workload.rs`：`Workload` / `WorkloadKind::{Pub,Sub,Conn}` / `TopicDistribution`。
- `src/model/scenario.rs`：`Scenario` / `ScenarioStage`（顺序 vs 并发）。
- `src/model/run.rs`：`Run` / `RunWorkload` / `RunStatus` / `Annotation`。
- `src/storage/migrations/0002_scenarios.sql`、`0003_runs_v2.sql`、`0004_annotations.sql`。

修改

- 现有 `src/model.rs` 改成 `src/model/legacy.rs`，作为旧字段的 `From`/`Into` 适配层；外部 `pub use` 入口移到 `src/model/mod.rs`。
- `src/storage.rs`：`init` 内调用 `run_migrations()`；新增 `BrokerProfileRepo` / `PayloadProfileRepo` / `ScenarioRepo` / `RunRepo` / `AnnotationRepo`。
- `src/bench.rs`：仅做 import path 更新与 `BenchConfig::to_workload()` 兼容方法；行为不变。

兼容（保留至少 1 个 PR）

- 旧表 `runs` / `metric_snapshots` / `bench_specimens` / `bench_templates` 全部保留；新增 `scenarios` / `workloads` / `runs_v2` / `run_workloads` / `broker_profiles` / `payload_profiles` / `annotations` 不替换原表。
- 一次性迁移脚本把每条旧 `runs` 行映射为 1 个 `scenarios + 1 个 workload + 1 个 runs_v2` 行（见 §3.2）。

## 数据 / 接口契约

### 1. 类型骨架

```rust
// src/model/broker.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: Option<TlsConfig>,
    pub auth: Option<AuthConfig>,
    pub keepalive_secs: u16,
    pub clean_session: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthConfig {
    UserPassword { username: String, password: String },
    ClientCert { cert_pem: String, key_pem: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_pem: Option<String>,
    pub server_name: Option<String>,
    pub insecure_skip_verify: bool,
}
```

```rust
// src/model/load.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum LoadShape {
    Flat { rate: f64 },
    Ramp { from: f64, to: f64, duration_ms: u64 },
    Step { stages: Vec<LoadStage> },
    Soak { rate: f64, duration_ms: u64 },
    Spike { baseline: f64, peak: f64, peak_duration_ms: u64, period_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadStage {
    pub rate: f64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadProfile {
    pub connect_shape: LoadShape, // 控制 connect_rate 随时间
    pub message_shape: LoadShape, // 控制 publish 速率随时间
    pub total_duration_ms: u64,   // 0 = run until stop
}
```

```rust
// src/model/payload.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PayloadKind {
    FixedBytes { size: usize, with_timestamp: bool },
    JsonTemplate { template: String, vars: BTreeMap<String, String> },
    CsvReplay { path: PathBuf, column: String, loop_when_done: bool },
    Counter { width: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadProfile {
    pub id: String,
    pub name: String,
    pub kind: PayloadKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

```rust
// src/model/workload.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadKind { Pub, Sub, Conn }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicDistribution {
    pub topic_template: String,
    pub partitions: u32,         // 1 = 单 topic, >1 = round-robin / hash
    pub group_strategy: TopicGroupStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopicGroupStrategy { RoundRobin, Hash, ClientId }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workload {
    pub id: String,
    pub name: String,
    pub kind: WorkloadKind,
    pub broker_profile_id: String,
    pub payload_profile_id: Option<String>, // sub/conn 不需要
    pub clients: u64,
    pub start_number: u64,
    pub client_id_template: String,
    pub topics: TopicDistribution,
    pub qos: QosLevel,
    pub retain: bool,
    pub load: LoadProfile,
    pub network_bind_mode: NetworkBindMode,
    pub bind_interfaces: Vec<String>,
    pub sample_interval_ms: u64,
}
```

```rust
// src/model/scenario.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStage {
    Parallel { workloads: Vec<Workload> },
    Sequential { workloads: Vec<Workload> },
}
```

```rust
// src/model/run.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub scenario_id: Option<String>, // ad-hoc run 可为空
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
    pub config_snapshot_json: String, // 冻结副本，便于复跑
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus { Pending, Running, Completed, Stopped, Failed }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationCategory { Manual, BrokerEvent, SlaBreach, ConfigChange }
```

### 2. Schema migration

`0002_scenarios.sql`

```sql
CREATE TABLE broker_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    tls_json TEXT,
    auth_json TEXT,
    keepalive_secs INTEGER NOT NULL,
    clean_session INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE payload_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE scenarios (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    stages_json TEXT NOT NULL,
    baseline_run_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_scenarios_updated ON scenarios(updated_at DESC);
```

`0003_runs_v2.sql`

```sql
CREATE TABLE runs_v2 (
    id TEXT PRIMARY KEY,
    scenario_id TEXT,
    name TEXT NOT NULL,
    tags_json TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    stopped_at TEXT,
    legacy_run_id TEXT,                -- 指向旧 runs.id 用于回查
    FOREIGN KEY(scenario_id) REFERENCES scenarios(id)
);

CREATE TABLE run_workloads (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    config_snapshot_json TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES runs_v2(id)
);

ALTER TABLE metric_snapshots ADD COLUMN run_workload_id TEXT;
CREATE INDEX idx_metric_snapshots_workload ON metric_snapshots(run_workload_id, ts);
```

`0004_annotations.sql`

```sql
CREATE TABLE annotations (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    run_workload_id TEXT,
    ts TEXT NOT NULL,
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(run_id) REFERENCES runs_v2(id)
);

CREATE INDEX idx_annotations_run_ts ON annotations(run_id, ts);
```

## 实施步骤

### 1. 拆 model

把 `src/model.rs` 改名为 `src/model/legacy.rs`，新建 `src/model/mod.rs`：

```rust
pub mod broker;
pub mod payload;
pub mod load;
pub mod workload;
pub mod scenario;
pub mod run;
pub mod legacy;

pub use broker::*;
pub use payload::*;
pub use load::*;
pub use workload::*;
pub use scenario::*;
pub use run::*;
// 旧类型继续暴露：
pub use legacy::{BenchConfig, BenchMode, BenchRun, BenchSpecimen, ...};
```

`legacy.rs` 内为旧 `BenchConfig` 实现：

```rust
impl BenchConfig {
    pub fn to_workload(&self, broker_id: &str, payload_id: Option<&str>) -> Workload { ... }
}

impl Workload {
    pub fn flatten_to_legacy(&self) -> Option<BenchConfig> { ... } // 仅当 LoadShape::Flat
}
```

### 2. 实现 migration

`src/storage.rs::init` 内：

```rust
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_initial",     include_str!("storage/migrations/0001_initial.sql")),
    ("0002_scenarios",   include_str!("storage/migrations/0002_scenarios.sql")),
    ("0003_runs_v2",     include_str!("storage/migrations/0003_runs_v2.sql")),
    ("0004_annotations", include_str!("storage/migrations/0004_annotations.sql")),
];

fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_versions (id TEXT PRIMARY KEY, applied_at TEXT NOT NULL)")?;
    for (id, sql) in MIGRATIONS {
        let already: bool = conn.query_row("SELECT 1 FROM schema_versions WHERE id = ?", [id], |_| Ok(true)).optional()?.unwrap_or(false);
        if !already {
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO schema_versions (id, applied_at) VALUES (?, ?)", params![id, Utc::now().to_rfc3339()])?;
        }
    }
    Ok(())
}
```

把现有 `CREATE TABLE IF NOT EXISTS runs / metric_snapshots / bench_specimens / bench_templates` DDL 抽到 `0001_initial.sql` 文件，行为完全等价（这一步是机械搬运）。

### 3. 数据回填

新增 `src/storage/backfill.rs`，在 `Storage::new` 末尾调用一次：

```rust
pub fn backfill_legacy_runs(conn: &Connection) -> Result<()> {
    let already: bool = conn.query_row(
        "SELECT 1 FROM schema_versions WHERE id = '_backfill_runs_v2'",
        [], |_| Ok(true),
    ).optional()?.unwrap_or(false);
    if already { return Ok(()); }

    // 对每条 runs 行：
    //   1) 读出 config_json -> BenchConfig
    //   2) 创建 broker_profiles 行（dedup by host:port:auth）
    //   3) 创建 payload_profiles 行（dedup by size:timestamp）
    //   4) 把 specimen 的 name/tags/description 提到 runs_v2
    //   5) 写一条 runs_v2 + 一条 run_workloads
    //   6) UPDATE metric_snapshots SET run_workload_id = ?
    //   7) INSERT INTO schema_versions (...)
    Ok(())
}
```

回填策略：
- broker dedup key = `(host, port, tls.enabled, auth.kind, auth.username, tls.server_name)`。
- payload dedup key = `(size, with_timestamp)`。
- 一条旧 run 始终映射为「一个 stage（Parallel）+ 一个 workload」，不创建 scenarios 行（保留为 ad-hoc run）。

### 4. Repo 抽象

每张新表建一个 repo struct：

```rust
pub struct BrokerProfileRepo<'a> { conn: &'a Connection }
impl BrokerProfileRepo<'_> {
    pub fn list(&self) -> Result<Vec<BrokerProfile>> { ... }
    pub fn get(&self, id: &str) -> Result<Option<BrokerProfile>> { ... }
    pub fn upsert(&self, profile: &BrokerProfile) -> Result<()> { ... }
    pub fn delete(&self, id: &str) -> Result<bool> { ... }
}
```

`Storage` 暴露 `with_conn(|conn| repos(...))` 模式，运行时（PR-2）和 API 层（PR-2）共用。

### 5. 旧 API 不改

`src/main.rs` 中所有路由保持原样；`BenchManager` 仅修正 `use crate::model::*;` 路径。

## 验证

```bash
cargo check
cargo test
```

新增 unit test：

- `src/model/load.rs`：5 种 LoadShape 的 round-trip serde + `instant_rate(elapsed)` 数学验证。
- `src/model/legacy.rs`：`BenchConfig::to_workload` ↔ `Workload::flatten_to_legacy` 的双向回归。
- `src/storage/backfill.rs`：在内存 sqlite 里塞 3 条旧 run，调用回填，断言 broker / payload / runs_v2 / run_workloads 行数符合预期，且 metric_snapshots 都填上了 run_workload_id。

手动：

- 删 `data/velamq-bench.sqlite3` 启动一次，跑一个老接口的 bench，确认 `runs` / `metric_snapshots` 仍写入；同时 `runs_v2` 也有对应行。
- 在已有 sqlite 上启动，确认 backfill 一次性补齐，`schema_versions` 有 `_backfill_runs_v2`。

## 风险与回滚

- **风险**：backfill 在大数据库（>10k runs）上首启慢。缓解：在 backfill 内分批 commit（每 200 行一批），并显式 log 进度。
- **风险**：现有用户的 `bench_templates` 字段被新 model 覆盖。缓解：本 PR 不动 `bench_templates`，只读其 config，新建对应的 broker/payload profile 但保留原表。
- **回滚**：删除新表 + 回滚 `metric_snapshots.run_workload_id` 列 + 删除 `schema_versions` 内的本 PR 行；现有运行时不会受影响。
