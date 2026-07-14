# PR-6：Compare 视图 + Baseline / Regression

## 目标

引入「跨 Run 对比」与「Baseline + 自动 Regression 检测」两条互相支撑的能力：

- Compare 页：手动挑选 2–4 个 run，KPI delta 表 + 多图 overlay。
- Baseline：每个 Scenario 可标记一个 baseline run；后续 run 自动给出与 baseline 的相对偏差（P95 / Err / Pub rate）。
- Dashboard 上 "Regressions vs Baseline" 列表占位（PR-3 留空）由本 PR 实装。

## 前置依赖

- PR-2（v2 API：scenario / run、metric snapshots 可按 run_workload 取）。
- PR-3（Dashboard 页骨架）。
- PR-5（Run Detail Hero 上 "Mark as Baseline" 占位按钮）。

## 涉及文件

新增

- `web/src/pages/Compare.vue`：Compare 主页面。
- `web/src/components/compare/RunPicker.vue`：从最近 50 个 run 中勾选 2–4 个。
- `web/src/components/compare/KpiDeltaTable.vue`：KPI 表格，含 ↑ / ↓ / pp / % 显示。
- `web/src/components/compare/OverlayChart.vue`：基于 PR-5 的 EChartBase，多 series overlay；选某条线为 baseline 后其它显示 delta tooltip。
- `web/src/components/run/BaselineBadge.vue`：在 Run 列表 / Run Detail Hero 显示 baseline 状态。
- `web/src/composables/useCompareData.ts`：批量拉 run + snapshots，归并为 series。
- `src/api/scenarios.rs`：`POST /api/v2/scenarios/{id}/baseline`、`DELETE /api/v2/scenarios/{id}/baseline`。
- `src/runtime/regression.rs`：在 run 完成时把 stats 与 baseline 对比，产出 `RegressionReport`，写到 `runs_v2.regression_json`。
- `src/storage/migrations/0005_regression.sql`：在 `runs_v2` 增加 `baseline_run_id TEXT`、`regression_json TEXT`。
- `src/api/dashboard.rs`：`GET /api/v2/dashboard/summary`，聚合最近 N 天的 KPI + active runs + regressions。

修改

- `web/src/pages/Dashboard.vue`：实装 `Regressions vs Baseline` 列表与 KPI 卡的 delta 数字。
- `web/src/pages/RunDetail.vue`：Hero 上 `Mark as Baseline` 实际可用；KPI 旁补充 baseline delta 徽章。
- `src/api/runs.rs`：在 `GET /runs/{id}` 响应里附 `regression_summary` 字段（如有）。

## 数据 / 接口契约

### 1. Baseline mark

```
POST /api/v2/scenarios/{scenario_id}/baseline
body: { "run_id": "..." }
resp: { "scenario_id": "...", "baseline_run_id": "...", "applied_at": "..." }
```

后端校验：

- `run_id` 必须属于该 scenario，且 `status` 是 `completed` 或 `stopped`。
- 替换 baseline 时记一条 `audit_log`（可选，本 PR 仅 console log）。

```
DELETE /api/v2/scenarios/{scenario_id}/baseline
resp: 204
```

### 2. Regression Report

`runs_v2.regression_json` 形如：

```jsonc
{
  "baseline_run_id": "abc",
  "computed_at": "2026-05-04T10:12:33Z",
  "metrics": {
    "p95_ms":           { "current": 18.2, "baseline": 16.5, "delta_abs": 1.7,  "delta_pct": 10.3 },
    "p99_ms":           { "current": 26.0, "baseline": 22.1, "delta_abs": 3.9,  "delta_pct": 17.6 },
    "publish_rate":     { "current": 940,  "baseline": 990,  "delta_abs": -50,  "delta_pct": -5.05 },
    "error_rate":       { "current": 0.0007, "baseline": 0.0001, "delta_abs": 0.0006, "delta_pct": 600 }
  },
  "verdict": "regression"   // "improvement" | "neutral" | "regression"
}
```

verdict 规则（可在 `Settings → Preferences` 调整阈值）：

| metric | regression 阈值 | improvement 阈值 |
| --- | --- | --- |
| `p95_ms`, `p99_ms` | +10% | -10% |
| `publish_rate`, `receive_rate` | -10% | +10% |
| `error_rate` | +0.05pp 或 +50% | 反向 |

任一关键指标越线即标 `regression`，全部好于阈值标 `improvement`，否则 `neutral`。

### 3. Dashboard summary

```
GET /api/v2/dashboard/summary?window_days=7
resp: {
  "kpis": {
    "total_published": 12480000,
    "total_received":  12450000,
    "p95_ms_avg":      18.4,
    "error_rate_avg":  0.0004,
    "active_runs":     1
  },
  "active_runs": [ { "id": "...", "scenario_id": "...", "progress_pct": 64 } ],
  "regressions": [
    { "scenario_id": "...", "scenario_name": "iot-prod", "run_id": "...", "verdict": "regression", "p95_delta_pct": 18, "err_delta_pp": 0.02 }
  ],
  "recent_runs": [ ... up to 12 ... ]
}
```

## 实施步骤

### 1. Schema migration

新增 `0005_regression.sql`：

```sql
ALTER TABLE runs_v2 ADD COLUMN baseline_run_id TEXT;
ALTER TABLE runs_v2 ADD COLUMN regression_json TEXT;
ALTER TABLE scenarios ADD COLUMN baseline_run_id TEXT;
CREATE INDEX idx_runs_v2_baseline ON runs_v2(baseline_run_id);
```

`scenarios.baseline_run_id` 是 PR-1 已有字段，本 PR 仅在 handler 层正式启用。

### 2. RegressionReport 计算

`src/runtime/regression.rs`：

```rust
pub fn compute_regression(
    run: &Run,
    run_stats: &RunStats,
    baseline_stats: &RunStats,
    thresholds: &RegressionThresholds,
) -> RegressionReport {
    let p95 = delta_pct(run_stats.latency_p95_ms, baseline_stats.latency_p95_ms);
    ...
    let verdict = classify(&[p95, p99, pub_rate, err_rate], thresholds);
    RegressionReport { ... }
}
```

`ScenarioRuntime::finalize_run` 内：

```rust
if let Some(scenario_id) = run.scenario_id.as_ref() {
    if let Some(baseline_id) = scenario_repo.baseline_run_id(scenario_id)? {
        let baseline_stats = run_repo.stats_v2(&baseline_id)?;
        let report = compute_regression(&run, &run.stats, &baseline_stats, &thresholds);
        run_repo.update_regression(&run.id, &report)?;
    }
}
```

阈值默认硬编码，后续 PR-7 / PR-8 接入 `Settings → Preferences`。

### 3. Compare 页 UI

布局：

```
┌── RunPicker (top, sticky) ──────────────────────────────────────┐
│ Scenario filter ▾   Status ▾   Search                            │
│ ┌ Run row (checkbox) ──────────────┐  …                          │
└─────────────────────────────────────────────────────────────────┘
┌── Once 2–4 selected: ──────────────────────────────────────────┐
│  KpiDeltaTable                                                   │
│  OverlayChart × 4 (Throughput / P95 / P99 / Errors)              │
│  [ Set baseline reference: run-1230 ▾ ]                          │
└─────────────────────────────────────────────────────────────────┘
```

行为：

- URL `?ids=a,b,c` 与 RunPicker 双向绑定，便于分享。
- `KpiDeltaTable` 默认以"第一个选中 run"为 baseline 列；可下拉切换。delta 列格式：`+10.3% ↑`（绿/红/灰按方向 + 阈值）。
- OverlayChart：每 run 一种颜色（从 `--chart-1..8` 取），workload 用线型区分（实线 = pub，虚线 = sub）；超过 4 run 不允许（强约束以保证可读性）。
- `useCompareData(ids[])` 并行 `GET /runs/{id}` + `GET /runs/{id}/snapshots`，结果按 `(run_id, run_workload_id)` 二级分组。

### 4. Run Detail "Mark as Baseline"

Hero 按钮：

- 当前 run `status == completed | stopped` 且属于某 scenario 时启用。
- 点击 → `POST /scenarios/{id}/baseline { run_id }`，成功后 toast + Hero 上加 `BaselineBadge`。
- 移除：`Marked as baseline` 徽章上的 `×` → `DELETE /scenarios/{id}/baseline`。

### 5. KPI delta 徽章

Run Detail Overview Tab 的 6 个 KPI 卡，如果 run 有 `regression_summary`，每张卡右上角显示：

```
▲ +10.3%   <- baseline
```

颜色由 metric 类型决定（latency / err 上升红色、pub_rate 下降红色）。

### 6. Dashboard 实装

`useDashboardStore.load()` 调 `GET /api/v2/dashboard/summary`，渲染：

- KPI strip：4 个数值 + 与上一个窗口环比的 ↑↓（baseline 不是窗口环比，写明 "vs last 7d"）。
- Regressions 卡片：每行 scenario 名 + verdict 徽章 + 关键 delta 一行。
- Active runs 卡片：复用 PR-3 占位组件。
- Recent runs：每张卡多一个 `BaselineBadge`（如果该 run 是 scenario 的 baseline）。

### 7. 阈值 Settings（最小化）

`/settings/preferences` 出 4 个滑块（p95 / p99 / rate / error）+ 重置默认。本 PR 把值写入 `localStorage`，后端阈值仍硬编码（PR-8 接入 user preferences 持久化）。前端在显示 delta 时按 user 阈值上色，但服务端 `verdict` 用默认阈值——文案上要写明这点。

## 验证

后端

```bash
cargo test runtime::regression
cargo test storage::run_repo
```

前端

```bash
npm run typecheck
npm run lint
```

新增 vitest：

- `composables/useCompareData`：mock 3 个 run + snapshots，断言 series 排列正确，色板分配稳定。
- `KpiDeltaTable.test.ts`：固定输入下数值 / 颜色一致。

手动：

- 跑 3 次同一个 scenario：第一次正常，第二次故意把 message_interval 减半（提速），第三次故意把 broker port 改错。
- 把第一次 mark 为 baseline；
  - 第二次 run 完成后 Dashboard 出现 `improvement`（pub_rate 上升）。
  - 第三次 run 完成后 Dashboard 出现 `regression`（error_rate 上升 + connect 失败）。
- Compare 页选三个 run：KPI 表 delta 对应；OverlayChart 三色清晰可辨；`?ids=a,b,c` 分享链接刷新后状态保留。
- Run Detail 第三次 run 顶端徽章显示 `▲ regression`，Overview KPI 卡上有红色 delta。

## 风险与回滚

- **风险**：阈值过严导致 false-positive regression。缓解：默认阈值偏宽（10%），且 `verdict=regression` 不阻塞流程，仅在 UI 提示。
- **风险**：scenario.baseline_run_id 与 run 关系级联问题：删 baseline run 时不能直接删。缓解：`DELETE /runs/{id}` 在后端检查是否被 scenario 引用为 baseline，如果是则 409 + 提示先解绑。
- **回滚**：`/compare` 路由下线 + Hero 按钮禁用即可，DB schema 留作前向兼容；`regression_json` 字段可置空。
