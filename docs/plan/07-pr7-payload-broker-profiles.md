# PR-7：Payload 生成器 + Broker / Payload Profile CRUD

## 目标

把 Payload 与 Broker 从 Scenario 的内联字段升级为可复用的 Profile，并实现完整 UI（CRUD、引用关系视图、临时 profile 自动 GC）。同时把 Payload 生成器从单一"固定大小随机字节"扩展到 4 种生成器：`fixed_bytes / json_template / csv_replay / counter`。

## 前置依赖

- PR-1（model）。
- PR-2（v2 API：broker / payload CRUD endpoint 已建，但 handler 仅 stub）。
- PR-3（前端 shell + Settings 路由）。
- PR-4（Scenario Builder 已能选 BrokerProfile / PayloadProfile）。

## 涉及文件

新增

- `src/runtime/payload.rs`：4 种 generator 实现（接收 `(client_index, sequence_index)` 输出 `Bytes`）。
- `src/runtime/payload_csv.rs`：CSV 列读取 + 内存缓存（启动期一次性 load，过大时按行 mmap）。
- `src/api/broker_profiles.rs` / `src/api/payload_profiles.rs`：完整 handler（list / get / create / update / delete / test_connection）。
- `web/src/pages/settings/BrokerProfiles.vue` / `PayloadProfiles.vue` / `NetworkBind.vue` / `Preferences.vue`。
- `web/src/components/profiles/BrokerProfileForm.vue` / `BrokerProfileTable.vue` / `BrokerProfileTestButton.vue`。
- `web/src/components/profiles/PayloadProfileForm.vue` / `PayloadKindEditor.vue` / `PayloadPreviewBox.vue`。
- `web/src/components/profiles/UsageBadge.vue`：标"被 N 个 scenario 引用"。

修改

- `src/runtime/workload.rs`：调用 `payload::Generator::next_payload(...)` 而非内联 timestamp/byte 拼接。
- `src/storage/`：`broker_profiles_repo.rs` / `payload_profiles_repo.rs`，含"used by"反查（join scenarios.stages_json 太重，改为单独维护 `payload_profile_uses` / `broker_profile_uses` 视图，每次 scenario upsert 时同步）。
- `src/runtime/garbage.rs`：每小时跑一次"未被任何 scenario / 历史 run 引用、且 30 天未更新、name 以 `ad-hoc-` 开头"的 profile 清理。
- 后端 `POST /api/v2/broker-profiles/{id}/test-connection`：用 rumqttc 建立 `connect → ping → disconnect` 循环（5 秒超时），返回结构化结果。

## 数据 / 接口契约

### 1. PayloadKind 字段

| kind | 字段 | 说明 |
| --- | --- | --- |
| `fixed_bytes` | `size: usize`, `with_timestamp: bool` | 当前唯一形态，行为对齐旧实现 |
| `json_template` | `template: String`, `vars: Map<String, JsonVarSpec>` | 见 §2 模板语法 |
| `csv_replay` | `path: String`, `column: String`, `loop_when_done: bool`, `as_json: bool` | 从 CSV 文件取值 |
| `counter` | `width: usize`, `prefix: String` | 输出 `{prefix}{seq:0width}` |

### 2. JSON template 语法

```jsonc
{
  "kind": "json_template",
  "template": "{\"id\": \"{{id}}\", \"ts\": {{ts}}, \"value\": {{val}}, \"tag\": \"{{tag}}\"}",
  "vars": {
    "id":  { "kind": "client_id" },
    "ts":  { "kind": "now_ms" },
    "val": { "kind": "rand_int", "min": 0, "max": 100 },
    "tag": { "kind": "rand_choice", "values": ["A", "B", "C"] }
  }
}
```

变量类型：`client_id / now_ms / now_iso / counter / rand_int / rand_float / rand_choice / fixed`。

后端用编译期模板（启动 workload 前把 `template` 拆成 `Vec<TemplatePart>`，每次只做拼接），不进入 mustache 解释器。

### 3. Test connection

```
POST /api/v2/broker-profiles/{id}/test-connection
resp 200: { "ok": true,  "connected_in_ms": 134 }
resp 200: { "ok": false, "stage": "auth",   "error": "Connection refused: bad_username_or_password" }
```

stage ∈ `{ "tcp", "tls_handshake", "mqtt_connect", "auth", "ping" }`，便于诊断。

### 4. Usage 反查

```
GET /api/v2/broker-profiles/{id}?include=usage
resp: {
  "profile": {...},
  "usage": {
    "scenarios": [{ "id": "...", "name": "..." }],
    "active_runs": 0,
    "historical_runs": 12,
    "ad_hoc": false
  }
}
```

PayloadProfile 同形。

## 实施步骤

### 1. PayloadGenerator 抽象

```rust
pub trait PayloadGenerator: Send + Sync {
    fn next(&self, ctx: &PayloadCtx) -> Bytes;
    fn min_size(&self) -> usize;
}

pub struct PayloadCtx {
    pub client_index: u64,
    pub sequence_index: u64,
    pub run_id: Arc<str>,
}

pub fn build(profile: &PayloadProfile, run_id: Arc<str>) -> Result<Box<dyn PayloadGenerator>>;
```

四个 impl：

- `FixedBytesGen` 沿用现有 `payload_template + ts marker` 实现。
- `JsonTemplateGen` 持有预编译 `Vec<TemplatePart>`。
- `CsvReplayGen` 启动时 `load_column(path, column)` -> `Vec<Bytes>`，运行时按 `sequence_index % len` 取；`loop_when_done=false` 跑完后 `errors.fetch_add` 并停止。
- `CounterGen` 简单格式化。

带时间戳能力作为可选 `wrap_with_timestamp(inner)` 装饰器。

### 2. Broker / Payload Profile Repo

```rust
pub struct BrokerProfileRepo<'a> { conn: &'a Connection }
impl BrokerProfileRepo<'_> {
    pub fn list(&self, include_ad_hoc: bool) -> Result<Vec<BrokerProfile>>;
    pub fn get_with_usage(&self, id: &str) -> Result<Option<(BrokerProfile, ProfileUsage)>>;
    pub fn upsert(&self, profile: &BrokerProfile) -> Result<BrokerProfile>;
    pub fn delete(&self, id: &str) -> Result<bool>;  // 引用中的拒绝
    pub fn touch_used_by(&self, profile_id: &str, scenario_id: &str) -> Result<()>;
}
```

`payload_profile_uses(scenario_id, payload_profile_id)` 表在 `scenarios::upsert` 内同步写入：先 `DELETE WHERE scenario_id=?` 再批量 `INSERT`。

### 3. Test Connection

`src/api/broker_profiles.rs::test_connection`：

```rust
let mqtt_options = build_mqtt_options(&profile)?;
let (client, mut event_loop) = AsyncClient::new(mqtt_options, 5);
let started = Instant::now();
let result = tokio::time::timeout(Duration::from_secs(5), async {
    while let Ok(notification) = event_loop.poll().await {
        match notification {
            Event::Incoming(Packet::ConnAck(_)) => return Ok(()),
            _ => continue,
        }
    }
    Err(anyhow!("event loop closed"))
}).await;
match result {
    Ok(Ok(_)) => Json(TestResult::ok(started.elapsed().as_millis() as u64)),
    Ok(Err(e)) => Json(TestResult::fail(classify_stage(&e), e.to_string())),
    Err(_)   => Json(TestResult::fail("timeout", "5s timeout".into())),
}
```

`classify_stage` 用 `e.to_string()` 关键字粗分（`tls`, `auth`, `connection refused`, `not connected`, `dns` ...）。

### 4. Settings 页 UI

`SettingsLayout.vue`（PR-3 已建占位）填充：

```
┌── Sub-nav ──┐ ┌── Pane ─────────────────────────────┐
│ Brokers     │ │  Heading + [+ New]                  │
│ Payloads    │ │  ┌─ Table ──────────────────────┐    │
│ Network     │ │  │ Name | Host | Auth | Used by │    │
│ Preferences │ │  │ ...                          │    │
└─────────────┘ │  └──────────────────────────────┘    │
                └─────────────────────────────────────┘
```

`BrokerProfiles.vue`：

- 表格列：Name / Host:Port / TLS pill / Auth pill / Used by pill (`3 scenarios`) / 状态 ⚪ / Actions。
- `+ New` 打开右抽屉 `BrokerProfileForm.vue`，含 Test Connection 按钮（与 PR-4 复用同组件）。
- Delete：若 `usage.active_runs > 0` 或 `usage.scenarios.length > 0` 弹拒绝；`historical_runs > 0` 弹 confirm（删后历史 run 仍可读，但来源信息为 deleted）。

`PayloadProfiles.vue`：

- 同结构。`PayloadKindEditor` 切换 4 种 kind，每种 kind 给独立子表单。
- `PayloadPreviewBox.vue`：右侧实时预览生成 1 条样本 + size + first-line。
  - 后端补 `POST /api/v2/payload-profiles/preview { profile, count: 3 }` 返回 3 条样本，方便预览。

### 5. NetworkBind 子页

把现 `BenchForm` 里的 NIC 选择器搬到 `Settings → Network Bind`：

- 显示当前 NIC 列表（loopback 灰显）+ 操作系统名。
- 配置默认 bind 模式（auto_random / auto_round_robin），保存到 `user_preferences`，作为 Scenario Builder 的默认值。
- 每个 NIC 后展示"过去 7 天内绑定的 client 总数"统计，便于查看负载分布。

### 6. Garbage Collection

`src/runtime/garbage.rs`：

```rust
pub async fn gc_loop(storage: Storage) {
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tick.tick().await;
        if let Err(err) = sweep_orphaned_profiles(&storage).await {
            warn!("gc sweep failed: {err:#}");
        }
    }
}

async fn sweep_orphaned_profiles(storage: &Storage) -> Result<()> {
    // delete from broker_profiles where name like 'ad-hoc-%'
    //   and updated_at < now-30d
    //   and id not in (select broker_profile_id from broker_profile_uses)
    //   and id not in (select broker_profile_id from run_workloads_workload_json /* explored via runtime */)
    Ok(())
}
```

启动时 `tokio::spawn(gc_loop(storage.clone()))`。

### 7. 旧字段映射

PR-2 旧 `/api/bench/start` 的 `BenchConfig` 在转 Scenario 时：

- 已有 host/port → upsert 名为 `legacy-{host}-{port}` 的 BrokerProfile（dedup 与 PR-1 backfill 共用 key）。
- payload_size + with_timestamp → upsert 名为 `legacy-fixed-{size}{-ts?}` 的 PayloadProfile。
- 新引用关系写入 `*_profile_uses` 表。

## 验证

```bash
cargo test runtime::payload
cargo test runtime::garbage
cargo test api::broker_profiles
```

Payload generator 单测覆盖：

- `JsonTemplateGen`：固定 vars + seed 后输出与 fixture 字节级一致。
- `CsvReplayGen`：`loop=true` 在末尾循环；`loop=false` 触发 error。
- `CounterGen`：`width=8 prefix="bench-"` 序列正确。

前端

```bash
npm run typecheck
npm run lint
```

手动：

- 在 Settings 创建一个本地 mosquitto BrokerProfile，Test Connection 通过，绿色徽章。
- 改成错的 port 再 test，结果显示 `tcp / connection refused`。
- 创建 `csv_replay` PayloadProfile，加载 `samples/iot-100.csv`，Preview 显示前 3 行。
- 在 Scenario Builder 里能看到这两个 profile，引用计数 ≥1；删除时被拒绝。
- 关掉所有引用后再删，成功。
- 跑一个使用 `json_template` 的 scenario，Run Detail Logs 抽几条 payload 检查实际生成内容（PR-5 logs Tab 展开样本）。

## 风险与回滚

- **风险**：CSV 文件路径可被外部用户控制，可能读到敏感文件。缓解：`csv_replay.path` 限制为 `data/samples/` 前缀，相对路径解析；后端拒绝绝对路径或带 `..`。
- **风险**：JSON template 高速渲染 hot path 性能。缓解：编译期拆 `Vec<TemplatePart>`，运行时只做 `Bytes` 拼接，benchmark 至少做到 1M ops/s 单线程。
- **风险**：删 broker profile 误删历史 run 引用。缓解：删除时不真正物理删除 row，改成 `is_archived = 1`，Settings 列表加"Show archived"过滤。
- **回滚**：删除 Settings 路由 + payload generator 退回单一 fixed_bytes 实现；profile 表保留。
