# PR-2：运行时与 API 重做

## 目标

让一个 Run 内可以并发执行 N 个 Workload（pub/sub/conn 任意组合），每个 workload 独立采样、独立向 SSE 推送，并落到 `metric_snapshots(run_workload_id)`。同步落地新的 REST 路由（broker / payload / scenario / run / annotation），保留旧 `/api/bench/*` 作为兼容入口。

## 前置依赖

- PR-1（领域模型与存储迁移）已合并。

## 涉及文件

新增

- `src/runtime/mod.rs`：导出 `ScenarioRuntime` / `RunHandle`。
- `src/runtime/run.rs`：`RunHandle`（一次 Run 的句柄），管理 N 个 workload task 的生命周期。
- `src/runtime/workload.rs`：单 workload 执行循环（pub/sub/conn 三个分支），独立采样器。
- `src/runtime/load_clock.rs`：把 `LoadProfile` 转成 `tokio::time::Interval` 的实时速率（每秒更新）。
- `src/runtime/sampler.rs`：原子计数 + latency 直方 + 采样输出（从 `bench.rs` 抽取通用部分）。
- `src/runtime/sse.rs`：把 `BenchEvent` 升级为 `RunEvent::{RunStateChanged, WorkloadMetric, WorkloadLog, RunAnnotation, Lagged}`。
- `src/api/mod.rs` / `src/api/{bench,scenarios,runs,brokers,payloads,annotations}.rs`：按资源拆分的路由 handler。
- `src/main.rs` 内 `Router` 重组（仍单文件）。

修改

- 把 `src/bench.rs` 内 `BenchManager` 改造为「`ScenarioRuntime` 的薄封装 + 旧接口适配器」；旧 `start(StartBenchRequest)` 内部转换为 `Scenario { single workload }` 后丢给 `ScenarioRuntime::start_run`。
- `src/main.rs` 路由：保留旧 `/api/bench/*`，新增 `/api/v2/*` 全部走新 handler。
- `src/storage.rs`：补 `RunRepo::list_v2 / get_v2 / start_v2 / finish_v2 / append_workload_metric / append_annotation`。

## 数据 / 接口契约

### 1. SSE 事件升级

老 `BenchEvent::{State, Metrics, Log}` 保留在 `/api/bench/events`（旧 UI 使用）。新增 `/api/v2/runs/{run_id}/events`：

```jsonc
event: run_state
data: { "run": { "id": "...", "status": "running", ... } }

event: workload_metric
data: { "run_workload_id": "...", "snapshot": { "ts": "...", "elapsed_ms": 1200, "publish_rate": 124.5, ... } }

event: workload_log
data: { "run_workload_id": "...", "log": { "level": "warn", "message": "..." } }

event: annotation
data: { "annotation": { ... } }

event: lagged
data: { "skipped": 12 }
```

每个 SSE 连接绑定一个 `run_id`，断线后客户端可用 `?since=<elapsed_ms>` 重新拉取增量；`broadcast::Receiver` 的 lag 通过 `Lagged` 事件告知。

### 2. REST 路由（v2）

```
# Broker / Payload Profile（CRUD，PR-7 详化）
GET    /api/v2/broker-profiles
POST   /api/v2/broker-profiles
PATCH  /api/v2/broker-profiles/{id}
DELETE /api/v2/broker-profiles/{id}
POST   /api/v2/broker-profiles/{id}/test-connection

GET    /api/v2/payload-profiles
POST   /api/v2/payload-profiles
PATCH  /api/v2/payload-profiles/{id}
DELETE /api/v2/payload-profiles/{id}

# Scenario
GET    /api/v2/scenarios
POST   /api/v2/scenarios
GET    /api/v2/scenarios/{id}
PATCH  /api/v2/scenarios/{id}
DELETE /api/v2/scenarios/{id}
POST   /api/v2/scenarios/{id}/run            -> 创建 run（异步启动）
POST   /api/v2/scenarios/{id}/baseline       -> 标记 baseline（PR-6 用）

# Run
GET    /api/v2/runs?scenario_id=&status=&limit=&cursor=
GET    /api/v2/runs/{id}
GET    /api/v2/runs/{id}/snapshots?run_workload_id=&since_ms=&limit=
GET    /api/v2/runs/{id}/report
GET    /api/v2/runs/{id}/events              -> SSE
POST   /api/v2/runs/{id}/stop
POST   /api/v2/runs/{id}/annotations
GET    /api/v2/runs/{id}/annotations
PATCH  /api/v2/runs/{id}                     -> 修改 name/tags/description
DELETE /api/v2/runs/{id}

# 即时压测（不绑定 scenario）
POST   /api/v2/runs                          -> body 内联一个 ad-hoc Scenario，立即启动

# 元能力
GET    /api/v2/network-interfaces            -> 沿用原 interfaces handler
GET    /api/v2/runtime/state                 -> 当前所有 active run 概览（理论 1 个，预留 N）
```

### 3. 启动 / 停止流程

```
client                       axum                              ScenarioRuntime
  |  POST /api/v2/runs        |                                   |
  |---------------------------> validate scenario                  |
  |                            |  start_run(scenario)              |
  |                            |---------------------------------->|
  |                            |  insert runs_v2 + run_workloads   |
  |                            |  spawn N workload tasks           |
  |                            |  return RunSummary                |
  |  open SSE  /events ---------->                                 |
  |                            |  broadcast RunStateChanged        |
  |                            |  broadcast WorkloadMetric * N/s   |
  |  POST /stop  ------------->|                                   |
  |                            |  set stop_tx -> all tasks join    |
  |                            |  flush final stats -> RunStateChanged(completed)
```

## 实施步骤

### 1. 抽出 sampler

把 `bench.rs` 里的 `Counters / LatencyWindow / drain_latency` 搬到 `runtime/sampler.rs`，`pub struct WorkloadSampler { counters: Counters, run_workload_id: String, sample_interval_ms: u64 }`，输出 `MetricSnapshot { run_workload_id, ts, elapsed_ms, ... }`。

把 `metric_snapshots` 的 INSERT 也下沉到 `WorkloadSampler::flush(&Storage)`。

### 2. 实现 LoadClock

```rust
pub struct LoadClock {
    started: Instant,
    shape: LoadShape,
}

impl LoadClock {
    pub fn instant_rate(&self, now: Instant) -> f64 {
        let elapsed_ms = now.duration_since(self.started).as_millis() as u64;
        match &self.shape {
            LoadShape::Flat { rate } => *rate,
            LoadShape::Ramp { from, to, duration_ms } => {
                let t = (elapsed_ms.min(*duration_ms) as f64) / (*duration_ms as f64);
                from + (to - from) * t
            }
            LoadShape::Step { stages } => {
                let mut acc = 0u64;
                let mut current = 0.0;
                for s in stages {
                    if elapsed_ms < acc + s.duration_ms { current = s.rate; break; }
                    acc += s.duration_ms;
                    current = s.rate;
                }
                current
            }
            LoadShape::Soak { rate, .. } => *rate,
            LoadShape::Spike { baseline, peak, peak_duration_ms, period_ms } => {
                let phase = elapsed_ms % period_ms;
                if phase < *peak_duration_ms { *peak } else { *baseline }
            }
        }
    }
}
```

每个 workload 任务循环里每 100 ms 重读一次 rate，调整 `tokio::time::interval` 的周期。

### 3. ScenarioRuntime

```rust
pub struct ScenarioRuntime {
    storage: Storage,
    events: broadcast::Sender<RunEvent>,
    runs: Mutex<HashMap<String, RunHandle>>,
}

pub struct RunHandle {
    run_id: String,
    stop_tx: watch::Sender<bool>,
    workloads: Vec<JoinHandle<()>>, // 一个 workload 一个任务
    started: Instant,
}

impl ScenarioRuntime {
    pub async fn start_run(self: &Arc<Self>, scenario: &Scenario, run_meta: RunDraft) -> Result<RunSummary>;
    pub async fn stop_run(self: &Arc<Self>, run_id: &str) -> Result<()>;
    pub async fn subscribe(&self) -> broadcast::Receiver<RunEvent>;
}
```

- `Parallel` stage：所有 workload 同时 spawn。
- `Sequential` stage：前一个 workload 的所有 client 跑完后再 spawn 下一个；本 PR 至少跑通 Parallel，Sequential 可只验证 stub。

### 4. workload 任务三分支

```
match workload.kind {
    WorkloadKind::Conn => connect_loop(...).await,
    WorkloadKind::Sub  => subscribe_loop(...).await,
    WorkloadKind::Pub  => publish_loop(...).await,
}
```

`publish_loop` 内：

```rust
loop {
    if *stop_rx.borrow() { break; }
    let rate = load_clock.instant_rate(Instant::now()).max(1.0);
    let interval = Duration::from_secs_f64(1.0 / rate);
    let next = Instant::now() + interval;
    publish_one(&client, &topic, &payload).await?;
    sampler.published();
    tokio::time::sleep_until(next.into()).await;
}
```

NIC 绑定、payload 渲染、topic 渲染都把现有 `bench.rs` 内的实现搬过来，但接收 `Workload` 而非旧 `BenchConfig`。

### 5. SSE multiplex

`runtime/sse.rs` 内提供 `pub fn run_event_stream(&self, run_id: &str) -> impl Stream<Item = SseEvent>`，过滤 `broadcast::Receiver<RunEvent>` 上的事件：只保留 `run_id` 匹配的。

`/api/v2/runs/{id}/events` handler：

```rust
async fn run_events(State(rt): State<Arc<ScenarioRuntime>>, Path(run_id): Path<String>) -> impl IntoResponse {
    let stream = rt.run_event_stream(&run_id);
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

支持 `?since_ms=N`：先一次性把 storage 内 `run_workload_metrics` 中 elapsed_ms ≥ N 的 snapshot 用 `WorkloadMetric` 事件发回，再切到 broadcast 流（防断线丢失）。

### 6. 旧 API 兼容

`POST /api/bench/start`：

```rust
async fn start_bench(...) -> impl IntoResponse {
    let scenario = Scenario::ad_hoc_from_legacy(&request)?;
    let summary = scenario_runtime.start_run(&scenario, RunDraft::from_legacy(&request)).await?;
    // 把 run_id 写入旧 runs 表的 legacy 视图（已经在 PR-1 backfill 处理）
    Json(StartResponse { run_id: summary.run_id, ... })
}
```

旧 `/api/bench/events` 内部转发新事件，丢弃 `run_workload_id` 字段，输出兼容形态。

旧 `bench_specimens` / `bench_templates` 的 CRUD：保留 handler，写入旧表的同时镜像写到 `runs_v2.name/tags/description` 与 `scenarios`。

### 7. 错误模型

新增 `src/api/error.rs`：

```rust
pub enum ApiError {
    NotFound(&'static str),
    Validation(String),
    Conflict(String),       // run already running for scenario
    Internal(anyhow::Error),
}
```

统一映射到 `{ "error": "...", "code": "validation" }` JSON。所有 v2 handler 用 `Result<Json<T>, ApiError>` 而不是分散 `match`。

## 验证

```bash
cargo check
cargo test --features runtime-tests
```

新增 unit / integration test：

- `runtime/load_clock.rs`：5 种 shape 在多个时间点的 rate 计算。
- `runtime/run.rs`：`MockBroker` + `ScenarioRuntime`，跑一个 Parallel(2 workload) Scenario 5 秒，断言两个 workload 各自的 metric_snapshots 行数 ≥ 4 且 `run_workload_id` 不同。
- `api/v2`：用 `tower::ServiceExt` 测一遍核心 handler 的 happy path 和 4xx 路径。

手动：

- 起一个本地 `mosquitto`，POST `/api/v2/runs` 内联一个 Parallel(pub+sub) scenario，跑 30s，检查：
  - `runs_v2` 1 行 `status=completed`。
  - `run_workloads` 2 行 / `metric_snapshots` 行数 = 30s/sample_interval × 2。
  - SSE 在浏览器 devtools 里能看到两路 `workload_metric` 流。
- 旧 UI 不动，跑一遍 `POST /api/bench/start`，确认 `runs_v2` 也有镜像行，旧 UI 仍展示。

## 风险与回滚

- **风险**：双写新旧表导致 storage 慢。缓解：所有写入用同一个 `tokio::task::spawn_blocking`，事务批量提交。
- **风险**：广播频率高（N workload × ~10 Hz 采样）触发 broadcast 通道 lag。缓解：`broadcast::channel(2048)`，并在 SSE handler 中处理 `Lagged` 事件回放最近 1 秒数据。
- **回滚**：`/api/v2/*` 路由可以一次性下线（注释 router 段），旧 UI / 旧 API 不受影响；`runs_v2` / `run_workloads` 数据保留供后续重启恢复。
