# velamq-bench 交互 / 功能 / UI 重设计方案

> 本文是 velamq-bench 第二阶段产品重做的总规划。详细 PR 拆分见 [`docs/plan/`](./plan/README.md)。
> 范围：后端运行时 + 数据模型 + HTTP/SSE API + 前端 IA / 页面 / 图表 / i18n 全量翻新。

## 1. 当前问题诊断

后端

- `BenchManager`（`src/bench.rs` ~1100 行）一次只支持**单 run、单 workload**：`mode` 只能在 `conn / sub / pub` 中三选一，无法 pub+sub 同时压测同一个 broker。
- 只有恒定速率（`message_interval_ms` / `connect_rate`），缺少 ramp / step / soak / spike 等负载剖面。
- "Specimen"（运行元数据）和 "Template"（保存的配置）字段几乎一模一样，两套 CRUD UI 反而让用户困惑。
- broker / 认证 / TLS 没有抽成 profile，每次新建 run 都要重输 host/port/username/password。
- payload 只有"固定大小随机字节 + 可选时间戳"，缺少 JSON 模板 / CSV 回放 / 多主题分布。
- SSE 事件结构是单流 (`state` / `metrics` / `log`)，要扩展到多 workload 必须重做。

前端

- `web/index.html` 449 行 + `app.js` 2498 行 + `styles.css` 4022 行全部手写、无构建工具，没有组件复用，CSS 已经堆出**两份 `:root`** token。
- 主要导航只有 `Bench Runs / Templates`，缺少 Dashboard、Compare、Settings 等聚合视图。
- "新建 Run" 是一个超长大表单（broker / 流量 / payload / NIC / specimen 全堆在一起），无渐进披露。
- Run 详情把 5 张大图一次性铺开（吞吐 / 延迟 / 连接 / 总量 / 错误），延迟图与吞吐图无法 overlay 或缩放，没有标注（broker 重启、配置切换等关键事件）。
- 没有 Run 之间的对比能力，没有 baseline、regression 提示。
- Chart 全是手写 canvas，缺少 hover tooltip / zoom / pan / 图例切换；难以扩展。
- i18n 只覆盖了 UI label，dashboard 文字、空状态、错误提示仍硬编码英文。

## 2. 重设计目标

1. **领域模型升级**：把 `BenchConfig + Specimen + Template` 拆为四类一等公民——`BrokerProfile`、`PayloadProfile`、`Workload`、`Scenario`，`Run` 是某个 Scenario 的一次执行。
2. **多 workload 并发**：一个 Scenario 可以并行运行 N 个 workload（pub/sub/conn），每个 workload 独立采样和呈现。
3. **负载剖面（Load Profile）**：恒定 / ramp / step / soak / spike 五种剖面，作用于连接速率与消息速率。
4. **新前端信息架构**：`Dashboard / Runs / Compare / Scenarios / Templates / Settings` 六大区，左侧导航配 Topbar 状态条。
5. **更强的 Run Detail**：分 Tab（Overview / Latency / Throughput / Connections / Errors / Logs / Config / Notes），图表带 tooltip / zoom / annotation。
6. **Run Compare**：可挑选 2–4 个 Run，KPI delta 表 + 多图 overlay。
7. **Baseline & Regression**：可把某 run 标记为 baseline；后续 run 自动展示 P95/P99/Error 的相对偏差，并在 Dashboard 上提示。
8. **Payload 生成器**：fixed bytes / json template / csv replay / 序号自增。
9. **Broker Profile**：保存可复用的 broker 连接（host/port/TLS/auth），Scenario 直接引用。
10. **图表升级**：引入轻量构建链（Vite + TS + ECharts）替代手写 canvas，统一 token、暗色支持、可访问性。
11. **导出包**：`bench bundle` 单 JSON 包含 run + scenario + 全量 snapshots，可在另一台机器导入对比。
12. **i18n / a11y**：保留 en / zh-CN，文案 JSON 化；颜色对比、键盘导航、aria-live 都过一遍。

## 3. 整体架构

```mermaid
flowchart LR
    subgraph backend [Backend: axum + tokio]
        Storage[(SQLite\nbroker_profiles\npayload_profiles\nscenarios\nworkloads\nruns\nrun_workloads\nmetric_snapshots\nannotations)]
        BrokerProfileSvc[BrokerProfileService]
        PayloadSvc[PayloadProfileService]
        ScenarioSvc[ScenarioService]
        Runtime[ScenarioRuntime\n(N workloads concurrent)]
        Sse[SSE multiplex\nworkload_id 维度]
        BrokerProfileSvc --> Storage
        PayloadSvc --> Storage
        ScenarioSvc --> Storage
        Runtime --> Storage
        Runtime --> Sse
    end

    subgraph frontend [Frontend: Vite + TS + ECharts]
        Shell[App Shell\nDashboard/Runs/Compare/Scenarios/Templates/Settings]
        Builder[Scenario Builder]
        Detail[Run Detail Tabs]
        Compare[Compare View]
        ChartKit[Chart Kit\n(ECharts wrappers)]
        Shell --> Builder
        Shell --> Detail
        Shell --> Compare
        Detail --> ChartKit
        Compare --> ChartKit
    end

    Sse --> Detail
    Sse --> Compare
    ScenarioSvc --> Builder
```

## 4. 关键交互改造

| 场景 | 当前流程 | 重设计后 |
| --- | --- | --- |
| 第一次压测 | 大表单填 20+ 字段 → Start | Topbar `Quick Bench` → 三步向导（Broker → Workload → Profile） |
| 复用配置 | 模板下拉框 → 填 specimen 字段 | 选 Scenario / Template → 直接 Start，元数据可后补 |
| 看延迟尾部 | 单图 + 多曲线挤在一起 | Latency Tab：曲线 + 直方图 + TopN 慢客户端列表 |
| pub+sub 同测 | 只能跑两次再脑补对比 | 一个 Scenario 内同时 1 个 pub + 1 个 sub workload，独立采样 |
| 跨 run 对比 | 不支持 | Compare 视图：勾选 2–4 个 run，自动 overlay |
| broker 切换提醒 | 无 | Run Detail 上插入 annotation（手动 / 自动 SLA 触发） |
| 回归报警 | 无 | Mark baseline → Dashboard 顶端展示最近 P95/Error 偏离 |

## 5. 阶段拆分（PR 级别）

| 序号 | 主题 | 关键交付 |
| --- | --- | --- |
| PR-1 | [领域模型与存储迁移](./plan/01-pr1-domain-model.md) | broker / payload / scenario / workload / load profile 类型 + sqlite migration |
| PR-2 | [运行时与 API 重做](./plan/02-pr2-runtime-api.md) | 多 workload 并发 + 多通道 SSE + 新 REST 路由 |
| PR-3 | [前端 Shell 与 Dashboard](./plan/03-pr3-frontend-shell.md) | Vite + TS 工程化 + 6 区导航 + Dashboard |
| PR-4 | [Scenario Builder](./plan/04-pr4-scenario-builder.md) | 三步向导 + 多 workload 编排 + load profile 可视化 |
| PR-5 | [Run Detail & 图表](./plan/05-pr5-run-detail-charts.md) | Tab 化详情 + ECharts 图表 + annotations |
| PR-6 | [Compare & Baseline](./plan/06-pr6-compare-baseline.md) | 多 run overlay + KPI delta + baseline / regression |
| PR-7 | [Payload & Broker Profile](./plan/07-pr7-payload-broker-profiles.md) | payload 模板 + broker profile CRUD + 引用关系 |
| PR-8 | [导出包 / i18n / 收尾](./plan/08-pr8-export-i18n-polish.md) | bench bundle 导入导出 + i18n 全覆盖 + a11y |

## 6. 范围与约束

- **单进程产品**：不引入分布式压测（agent fan-out 留作后续立项），仅在单台机器上扩 workload 并发。
- **依赖最小化**：后端不新增重型依赖；前端引入 Vite + TypeScript + ECharts 三件套，仍打包成静态资源由 axum 直接 serve。
- **数据兼容**：新 schema 一律走迁移脚本，老的 `runs` / `metric_snapshots` / `bench_specimens` / `bench_templates` 数据自动映射到新模型，**不丢历史 run**。
- **不破坏脚本调用**：旧 `POST /api/bench/start` 在 PR-2 后保留为兼容入口，跑完 deprecation 周期再移除。
- **i18n**：维持 en / zh-CN 双语；新增文案必须同时落两份 JSON。
- 提交信息约定：`feat(api): ...` / `feat(web): ...` / `refactor(model): ...` / `chore(docs): ...`。

## 7. 与 docs/plan 的关系

本文件是产品级总规划。所有具体改动落在 [`docs/plan/`](./plan/README.md) 下编号 PR 文档，文档之间出现冲突以 `docs/plan/` 为准。
