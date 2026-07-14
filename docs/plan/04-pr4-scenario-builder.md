# PR-4：Scenario Builder（多 Workload 编排）

## 目标

把"新建 run"从一个 20+ 字段的大表单，改造成「三步向导 + 多 workload 编排 + 负载剖面可视化」的 Scenario Builder，并落地 `/scenarios` 列表页与 `/scenarios/:id` 详情页。本 PR 完整覆盖：从 0 创建一个 Scenario、保存为模板、立即运行、并对运行中的 active workload 在 Builder 里做轻度调整（仅 stop 个别 workload）。

## 前置依赖

- PR-2（v2 API：scenario / run / broker / payload CRUD）。
- PR-3（前端 shell + 路由 + i18n）。

## 涉及文件

新增

- `web/src/pages/Scenarios.vue`：scenario 列表页。
- `web/src/pages/ScenarioDetail.vue`：scenario 详情（含历史 run 时间线 + 一键复跑）。
- `web/src/pages/ScenarioBuilder.vue`：三步向导主容器。
- `web/src/components/builder/StepBroker.vue`：选 / 新建 BrokerProfile。
- `web/src/components/builder/StepWorkloads.vue`：N 个 workload 卡片编排。
- `web/src/components/builder/StepProfileSchedule.vue`：duration / sample / NIC / save-as-template。
- `web/src/components/builder/WorkloadCard.vue`：单个 workload 编辑（含 mode 切换、payload pick、topic、QoS）。
- `web/src/components/builder/LoadShapeEditor.vue`：负载剖面编辑器（5 种 shape 切换 + 即时预览图）。
- `web/src/components/builder/LoadShapePreview.vue`：用 `<canvas>` 或 `AppSparkline` 把 LoadShape 转成时序曲线。
- `web/src/components/builder/TopicDistributionInput.vue`：topic 模板 + partitions + 分发策略。
- `web/src/components/builder/QuickBenchSheet.vue`（替换 PR-3 占位实现）。
- `web/src/components/scenario/ScenarioCard.vue` / `ScenarioRunTimeline.vue` / `ScenarioBaselineBadge.vue`。
- `web/src/composables/useScenarioForm.ts`：管理 builder 多步状态、跨步骤校验。

修改

- `web/src/api/client.ts`：补 `scenarios.*` / `brokerProfiles.list` / `payloadProfiles.list`。
- `web/src/stores/scenarios.ts`：补充 CRUD action。
- `web/src/router/index.ts`：`/scenarios/new` / `/scenarios/:id/edit` / `/scenarios/:id/run`。

## 数据 / 接口契约

- 后端 PR-2 已就绪，本 PR 只对接，不再新增 endpoint。
- Scenario JSON shape 沿用 `01-pr1-domain-model.md` 中定义。
- Builder 的中间态保存在 `useScenarioForm()` store（不 Pinia 全局，仅作用于路由生命周期）；进入 `/scenarios/:id/edit` 自动从后端 hydrate；按 `Cmd+S` 保存草稿到 `localStorage`。

## 实施步骤

### 1. 列表页 `Scenarios.vue`

```
+--------------------------------------------------+
| [+ New Scenario]   [Filter ▾]   [Sort ▾]   [⌕]   |
+--------------------------------------------------+
| Card grid (3 cols on >=1280px, 2 on >=768)       |
|                                                  |
|  ┌─ ScenarioCard ────────────────┐               |
|  │ Name                Tag1 Tag2  │               |
|  │ ⚡ pub  + ✉ sub                  │               |
|  │ broker: ec2-emqx-prod          │               |
|  │ Last run · 3h ago · ✅          │               |
|  │ Baseline: run-1230            │               |
|  │ [Run] [Edit] [⋯]               │               |
|  └────────────────────────────────┘               |
+--------------------------------------------------+
```

`ScenarioCard.vue` 显示：name + tags + workload 摘要（icons & 数量）+ 关联 broker + 最近一次 run 状态时间 + baseline 提示 + 三个动作按钮。

### 2. 详情页 `ScenarioDetail.vue`

左右分栏：

- 左：scenario 基本信息（name/tags/description）+ 配置预览（多 workload 折叠列表，可点开 inline 看完整字段）。
- 右：历史 run 时间线，每条 run 一行（state pill + duration + KPI 缩略 + open）。底部"Run again"按钮直接 `POST /scenarios/{id}/run`。

时间线组件 `ScenarioRunTimeline.vue` 复用 `useRunsStore.list` 加 `scenarioId` 过滤。

### 3. Builder 主容器 `ScenarioBuilder.vue`

```vue
<template>
  <div class="builder">
    <BuilderStepper :steps="3" :current="step" @jump="onJump" />
    <component :is="currentStep" v-model="form" />
    <BuilderFooter :step="step" :valid="stepValid" @prev="prev" @next="next" @run="runNow" @save="saveTemplate" />
  </div>
</template>
```

进入路由：

- `/scenarios/new`：空表单。
- `/scenarios/:id/edit`：先 GET 详情填充。
- `/scenarios/:id/run`：直接走"立即运行"分支（跳过保存）。

完整跳出 builder 前若有 dirty state，弹 confirm。

### 4. Step 1：Broker

```vue
<StepBroker>
  <BrokerProfileList v-model="form.brokerProfileId" />
  <button @click="openInline = true">+ Use one-off broker</button>
  <BrokerProfileForm v-if="openInline" v-model="form.adhocBroker" />
</StepBroker>
```

- 保存的 BrokerProfile 列表（CRUD 在 PR-7，本 PR 已经能选）。
- 临时 broker：可填一次性 host/port/auth，提交时后端把它即时 upsert 为名为 `ad-hoc-{ts}` 的 profile。
- "Test connection" 按钮：调 `POST /api/v2/broker-profiles/{id}/test-connection`，3s 超时显示 `Connected` / `Refused` / `Auth failed`。

### 5. Step 2：Workloads

每个 workload 一张 `WorkloadCard.vue`：

```
┌─ Workload 1 (pub) ──────── [⋯] [×] ┐
│ Name: pub-leg                       │
│ Mode: ●pub  ○sub  ○conn            │
│ Topic: bench/{i}     QoS: 1         │
│ Clients: 1000   Start#: 1           │
│ Payload: random-256 ▾  [+ new]      │
│ ── Load Shape ──────────────────── │
│ ●Flat 500/s                         │
│ ○Ramp 0→500/s in 60s                │
│ ○Step  …  ○Soak  …  ○Spike …        │
│ [LoadShapePreview canvas]           │
│ ── Topic distribution ──────────── │
│ Partitions: 16   Strategy: hash     │
│ ── Advanced ▸ collapsed ──         │
└────────────────────────────────────┘
```

底部"+ Add Workload" 触发新增。多个 workload 的执行模式（默认 Parallel）与并发策略放在 `Step 3`。

校验规则：

- 至少 1 个 workload。
- pub 必须选 PayloadProfile（或填 inline）。
- topic 模板里若包含 `{i}`，需明示 partitions 数量。
- 同一 broker 上同名 client_id_template 之间冲突 → warning（不阻止保存，给提示）。

### 6. LoadShape 编辑器与预览

`LoadShapeEditor.vue` 用 segmented control 切换 5 种 shape，每种 shape 给参数表单：

| shape | 字段 |
| --- | --- |
| Flat | `rate` |
| Ramp | `from`, `to`, `duration_ms` |
| Step | 动态行 `[rate, duration_ms]` |
| Soak | `rate`, `duration_ms` |
| Spike | `baseline`, `peak`, `peak_duration_ms`, `period_ms` |

`LoadShapePreview.vue` 接收 `LoadShape` 值，本地用 `instant_rate(t)` 算 200 个采样点画一条曲线（与后端算法一致，通过单元测试保证）。

### 7. Step 3：Profile & Schedule

字段：

- Total duration（0 = 手动停止）。
- Sample interval ms。
- Stage strategy：Parallel（默认）/ Sequential（如果 workload 数 > 1）。
- NIC bind（系统 / 自动 random / 自动 RR / 手动 random / 手动 RR + IP 选择器）。
- Annotations on start：可勾选"自动在 P95 > X ms 时插入 SLA Breach annotation"（PR-5 真正消费）。
- Save as template name + tags（仅"Save Template"按钮使用）。

底部三个按钮：

```
[Save as Template]   [Save & Run]   [Run Once Without Saving]
```

行为：

- `Save as Template`：`POST /scenarios`（status 草稿）。
- `Save & Run`：`POST /scenarios` + `POST /scenarios/{id}/run`，跳转 `/runs/:run_id`。
- `Run Once Without Saving`：`POST /api/v2/runs` 内联 ad-hoc scenario。

### 8. Active Run 中的轻度编辑

进入正在运行 run 对应 scenario 的 `/scenarios/:id` 页时，详情显示 active workload 列表，每个 workload 可单独"Stop this workload"（PR-2 后端补 `POST /api/v2/runs/{id}/workloads/{wid}/stop`，若未实现则禁用按钮）。本 PR 仅在 UI 上预留，按钮后端未就绪时给 tooltip 说明。

## 验证

```bash
cd web
npm run typecheck
npm run lint
```

手动：

- `/scenarios/new` 走完三步，能创建一个 pub+sub 双 workload 的 scenario，存为 template。
- 在 `/scenarios/:id/run` 触发一次运行，跳转到 `/runs/:id`，PR-5 实装前至少能在 Overview 占位看到两个 workload 的 mini 卡。
- 切换 LoadShape 类型，预览 canvas 实时变化；切到 Step 增减 stage，曲线随之更新。
- "Test connection" 对错地址给红色徽章 + 错误细节。

可访问性：

- 整个 builder 可仅用键盘走完（`Tab / Shift+Tab / Space / Enter`）。
- 每步 `aria-current="step"`，错误使用 `role="alert"` + `aria-live="polite"`。

## 风险与回滚

- **风险**：多 workload 编排 UI 复杂度高，初版易乱。缓解：本 PR 强制每屏只展示一个 workload 的详细字段，其它折叠为卡片摘要；`Edit` 时进入抽屉式编辑器单一焦点。
- **风险**：临时 broker 与持久 broker 混用易污染 Settings 列表。缓解：临时 broker 在 Settings 页加一个"临时"过滤开关，默认隐藏；超过 30 天未被 run 引用的临时 broker 后台 GC（PR-7 实现）。
- **回滚**：路由层下线 `/scenarios/*`，老的 Templates 页继续工作；`web/legacy/` 也仍可作为 fallback。
