# PR-5：Run Detail 重做与图表升级

## 目标

把 Run Detail 从「五张并列大图 + 一组旁白卡」改造成 **8 个 Tab + 主次图分层 + 可标注时间轴** 的观测台，并把图表层从手写 canvas 替换为 ECharts 封装，统一支持 tooltip、zoom、pan、图例切换、明暗主题。每个 Tab 聚焦一种问题诊断（吞吐 / 延迟 / 连接 / 错误 / 日志 / 配置 / 备注 / 概览）。

## 前置依赖

- PR-2（v2 SSE + workload 维度的 metric_snapshots）。
- PR-3（前端 shell，含 token / theme 切换）。

## 涉及文件

新增

- `web/src/pages/RunDetail.vue`：主容器（含 hero + AppTabs）。
- `web/src/pages/run-detail/OverviewTab.vue`、`LatencyTab.vue`、`ThroughputTab.vue`、`ConnectionsTab.vue`、`ErrorsTab.vue`、`LogsTab.vue`、`ConfigTab.vue`、`NotesTab.vue`。
- `web/src/components/charts/EChartBase.vue`：ECharts 薄封装（resize observer + theme 同步 + i18n 数字格式化）。
- `web/src/components/charts/ThroughputChart.vue` / `LatencyChart.vue` / `LatencyHistogram.vue` / `ConnectionsChart.vue` / `ErrorChart.vue` / `LatencyHeatmap.vue`。
- `web/src/components/charts/AnnotationLayer.vue`：在 ECharts `markLine + markArea` 上叠用户 / 系统 annotation。
- `web/src/components/charts/LegendBar.vue`：统一图例切换组件（hover、点选、双击只看一项）。
- `web/src/components/run/RunHero.vue`：顶部信息条（status / duration / workload pills / actions）。
- `web/src/components/run/WorkloadMiniCard.vue`：Overview Tab 的每 workload 一行迷你卡。
- `web/src/components/run/SlowClientsList.vue`：Latency Tab 右侧 TopN 慢客户端。
- `web/src/components/run/AnnotationDialog.vue`：手动添加 annotation 的对话框。
- `web/src/composables/useRunDetail.ts`：聚合 `useSse(runId)` + 历史 `GET /runs/{id}/snapshots` + `useRunsStore.loadOne(id)`。
- `web/src/charts/echarts-theme-light.ts` / `echarts-theme-dark.ts`：把 `theme/tokens.css` 里的颜色注册成 ECharts theme。

修改

- `web/src/router/index.ts`：`/runs/:id` 子路由 `/runs/:id/:tab(overview|latency|throughput|connections|errors|logs|config|notes)`。
- `web/src/api/types.ts`：补 `Annotation` 类型。
- `web/src/main.ts`：`echarts/core` 按需注册组件 + 注册 light / dark theme。
- 后端：`src/api/runs.rs` 增加 `POST /api/v2/runs/{id}/annotations`、`GET /api/v2/runs/{id}/annotations`（PR-2 已建表，本 PR 实装 handler）。
- 后端：`src/runtime/run.rs` 在 `LoadShape::Step` 切换时自动写一条 `AnnotationCategory::ConfigChange`。

依赖

```bash
npm i echarts
npm i -D @types/echarts
```

ECharts 用「按需引入」：`use([ LineChart, BarChart, HeatmapChart, GridComponent, TooltipComponent, LegendComponent, MarkLineComponent, MarkAreaComponent, DataZoomComponent ])`，避免引入全包。

## 数据 / 接口契约

```
GET  /api/v2/runs/{id}                        -> Run + RunWorkload[]
GET  /api/v2/runs/{id}/snapshots
       ?run_workload_id=xxx&since_ms=N&limit=M -> MetricSnapshot[]
GET  /api/v2/runs/{id}/annotations            -> Annotation[]
POST /api/v2/runs/{id}/annotations            -> body { ts, category, title, detail, run_workload_id? }
GET  /api/v2/runs/{id}/events                 -> SSE（PR-2 已就绪）
```

## 实施步骤

### 1. RunDetail 容器 & Hero

```vue
<template>
  <div class="run-detail">
    <RunHero :run="run" :status="status" @stop="stopRun" @add-annotation="openAnnotationDialog" />
    <AppTabs :model-value="tab" :tabs="tabDefs" @change="onTabChange" />
    <RouterView v-slot="{ Component }">
      <KeepAlive include="OverviewTab,LatencyTab,ThroughputTab,ConnectionsTab,ErrorsTab,LogsTab">
        <component :is="Component" />
      </KeepAlive>
    </RouterView>
    <AnnotationDialog ref="annDialog" />
  </div>
</template>
```

- Hero 信息：`name`、`scenario name`、`workload pills`（每个 workload 颜色 + kind icon）、`duration`（实时滚动）、`status pill`、`Stop` 按钮、`Add Annotation` 按钮、`Mark as Baseline`（PR-6 启用，本 PR 占位禁用）、Export 菜单（JSON/CSV/SVG/PDF/Bundle）。
- KeepAlive 让 Tab 之间切换不丢图表 / state。

### 2. Overview Tab

布局：左 6 卡 KPI grid（Pub / Recv / P95 / Err rate / Connected / Workloads）+ 右两张主图（Throughput last 60s / Latency band last 60s）+ 底部 N 个 `WorkloadMiniCard`。

KPI 卡显示：当前值、与本 run 平均的差值徽标、一个 60s mini sparkline；明确写出"和 baseline 差异"是 PR-6 的事。

### 3. Latency Tab

主区双图：

- 上：`LatencyChart`（line chart）— avg/p50/p90/p95/p99/p99.9/max 七条曲线，图例可单独开关；右上角 `[Linear] [Log]` 切 Y 轴。
- 下：`LatencyHistogram`（bar chart）— 当前最近 1 分钟内 latency 分布（用 ECharts `dataset` + bin 分桶 client 侧算）。

右侧侧栏：`SlowClientsList`，按 latency_p99 排序的 TopN 客户端（基于后端额外暴露的 `GET /api/v2/runs/{id}/clients/top?metric=p99`，后端如未就绪则该侧栏给"PR-7 后端补"占位）。

补充：`LatencyHeatmap.vue`（time × bucket × count 热力图），通过 Tab 顶端的 segmented 切「Lines / Histogram / Heatmap」三种视图。

### 4. Throughput Tab

```
[ Per workload | Aggregate ] [ Linear | Log ]
┌── Throughput pub / recv ────────────────────────┐
│ stacked area or split lines                     │
│ DataZoom slider 在 X 轴下方                     │
└─────────────────────────────────────────────────┘
┌── Cumulative totals ────────────────────────────┐
└─────────────────────────────────────────────────┘
┌── Bytes/s (publish_rate × payload_size) ────────┐
└─────────────────────────────────────────────────┘
```

聚合 vs 拆分：用同一份 `points by run_workload_id`，要么各 workload 一条线，要么相加为单条总线。

### 5. Connections Tab

```
[Connected current  +  Connect/s]
[Disconnect events (annotation marker)]
[NIC distribution donut: bind 模式下 client 在不同 NIC 的比例]
```

Disconnect 计数走 `AnnotationCategory::BrokerEvent`（后端运行时检测到 client 断开时自动写入；本 PR 在运行时补这一处 hook）。

### 6. Errors Tab

```
[Error count / s line chart]
[Error breakdown bar (按错误码)]
[Recent errors table 50 行]
```

Recent errors 来自 `LogLine` 中 level=error 的子集（`useSse` 已提供）。

### 7. Logs Tab

替换老的 textarea-style logs，改为：

- 顶部 toolbar：level filter（all / warn / error）、search input、`Pause` 按钮。
- 主体：虚拟滚动列表（`vue-virtual-scroller` 或自写小型虚拟化），每行 `level pill + ts + workload pill + message`。
- 单行点击展开 detail；导出 `.log` 文件按钮。

### 8. Config Tab

只读展示 run 启动时 frozen 的 scenario 副本（来自 `run_workloads.config_snapshot_json`）。结构：

- Scenario 元信息（name / tags / desc）。
- 每个 workload 折叠卡片，展开看完整字段。
- 顶部按钮 `Run with this config again`：等价于复制 scenario + 立即运行。

### 9. Notes Tab

- run 的 `name / tags / description` 可编辑（`PATCH /runs/{id}`）。
- annotations 时间线：所有 annotation 列表 + "Add" 按钮 + 可删除。
- 自动 annotation 用不同颜色 chip 区分（manual=灰、broker_event=蓝、sla_breach=红、config_change=橙）。

### 10. ECharts 封装

`EChartBase.vue` 关键点：

```vue
<script setup lang="ts">
const props = defineProps<{ option: EChartsOption; height?: string }>();
const el = ref<HTMLDivElement>();
let chart: echarts.ECharts | null = null;
const ui = useUIStore();

onMounted(() => {
  chart = echarts.init(el.value!, ui.theme === 'dark' ? 'velamq-dark' : 'velamq-light');
  chart.setOption(props.option, true);
});
watch(() => props.option, (opt) => chart?.setOption(opt, false), { deep: true });
watch(() => ui.theme, (t) => {
  chart?.dispose();
  chart = echarts.init(el.value!, t === 'dark' ? 'velamq-dark' : 'velamq-light');
  chart.setOption(props.option, true);
});

const ro = new ResizeObserver(() => chart?.resize());
onMounted(() => ro.observe(el.value!));
onBeforeUnmount(() => { ro.disconnect(); chart?.dispose(); });
</script>

<template>
  <div ref="el" :style="{ width: '100%', height: height ?? '320px' }" />
</template>
```

每张业务图（Throughput / Latency / ...）只负责生成 `option`，不直接操作 chart 实例。所有 series 颜色从 `--chart-1..8` token 拿（同步注册到 ECharts theme）。

### 11. Annotation 渲染

时间序列图统一插一段：

```ts
markLine: {
  silent: false,
  symbol: ['none', 'none'],
  data: annotations.map(a => ({
    xAxis: new Date(a.ts).getTime(),
    label: { show: true, formatter: a.title, color: '...' },
    lineStyle: { type: 'dashed', color: colorByCategory(a.category) },
  })),
}
```

点击 markLine 弹 `AnnotationDialog` 详情。

### 12. 历史回填 + 实时拼接

`useRunDetail(id)` 的数据流：

1. mount 时：`runs.loadOne(id)` 取 run 元；`GET /runs/{id}/snapshots?limit=600`（每个 workload 单独取）；`GET /runs/{id}/annotations`。
2. 状态为 `running` 时再 `useSse(runId, sinceMs=lastSnapshotElapsed)` 接续。
3. SSE `workload_metric` 进来：append；`run_state` 进来：合并；`annotation` 进来：append。

所有 reactive `Map<run_workload_id, MetricSnapshot[]>`，图表组件按 run_workload_id 分组生成 series。

## 验证

```bash
cd web
npm run typecheck
npm run lint
```

新增前端 unit test（vitest）：

- `composables/useRunDetail`：mock fetch + EventSource，断言 snapshot 拼接 / 重连去重正确。
- `charts/Throughput`：`option` 生成 snapshot test，覆盖 Aggregate / Per Workload / Linear / Log 四组合。

手动：

- 跑一个 pub+sub 双 workload run：8 个 Tab 全部能切，KeepAlive 切回不丢曲线。
- Overview KPI 实时变化；Latency Tab 切 Lines/Histogram/Heatmap 都能正常渲染。
- 在 Logs Tab 滚到 5000 行无明显卡顿（虚拟滚动）。
- 手动 Add Annotation，曲线上立即出现虚线 + 标签；刷新页面持久化在 Notes Tab。
- 切 dark theme 后图表 series 颜色与 token 同步，Tooltip / Legend 不出现白底黑字残留。

性能：

- 单图 600 个点 × 7 series 在 4K 屏 60fps（开 throttle 100ms 推送）。
- Tab 切换 < 100ms（KeepAlive 已生效）。

## 风险与回滚

- **风险**：ECharts gzip 后约 250KB+，对入口 JS 体积有影响。缓解：路由懒加载 RunDetail，按需 import 子模块；首页（Dashboard）不加载 ECharts，只用 `AppSparkline`（手写 SVG）。
- **风险**：8 个 Tab 内容多，移动端不友好。缓解：Tab 在窄屏切换为下拉 select；本期不做完整移动端，但保证 ≥1024 宽屏可用。
- **回滚**：`/runs/:id` 切回单页堆栈式布局只需在 `RunDetail.vue` 不挂 `AppTabs`、所有子组件 `v-show` 显示；图表层封装独立可单独保留。
