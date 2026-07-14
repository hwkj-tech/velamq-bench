# velamq-bench 重设计落地文件集

本目录把 [`docs/redesign-plan.md`](../redesign-plan.md) 里的总规划拆成 PR 级别的可执行文档。每个文件对应一个 PR / 一段独立工作，包含目标、依赖、文件清单、实施步骤、验证。

> 实施顺序按文件名前缀编号 `01..08`，依赖关系见每份文档的「前置依赖」段。
>
> PR-1 / PR-2 是后端基座（领域模型 + 运行时 + API），PR-3 ~ PR-6 是前端 IA + 主流程，PR-7 是横切能力（payload / broker profile），PR-8 收尾（导出 / i18n / 打磨）。后端两个 PR 必须先合并；前端 PR-3 之后可以与 PR-7 并行。

## PR 列表

| 文件 | 主题 | 范围 |
| --- | --- | --- |
| [00-design-overview.md](00-design-overview.md) | 设计总览 | 信息架构、术语表、布局草图、视觉 token |
| [01-pr1-domain-model.md](01-pr1-domain-model.md) | 领域模型与存储 | `BrokerProfile` / `PayloadProfile` / `Workload` / `Scenario` / `LoadProfile` 类型 + sqlite migration |
| [02-pr2-runtime-api.md](02-pr2-runtime-api.md) | 运行时与 API | 多 workload 并发执行 + 多通道 SSE + 新 REST 路由 |
| [03-pr3-frontend-shell.md](03-pr3-frontend-shell.md) | 前端 Shell & Dashboard | Vite + TS 工程化 + 六区导航 + Dashboard 聚合 |
| [04-pr4-scenario-builder.md](04-pr4-scenario-builder.md) | Scenario Builder | 三步向导 + 多 workload 编排 + 负载剖面预览 |
| [05-pr5-run-detail-charts.md](05-pr5-run-detail-charts.md) | Run Detail & 图表 | Tab 化详情 + ECharts + annotation + TopN 慢客户端 |
| [06-pr6-compare-baseline.md](06-pr6-compare-baseline.md) | Compare & Baseline | 多 run overlay + KPI delta + baseline / regression |
| [07-pr7-payload-broker-profiles.md](07-pr7-payload-broker-profiles.md) | Payload & Broker Profile | payload 模板 + broker profile CRUD + 引用关系 |
| [08-pr8-export-i18n-polish.md](08-pr8-export-i18n-polish.md) | 导出 / i18n / 收尾 | bench bundle 导入导出 + i18n 全覆盖 + a11y |

## 文档模板

每份 PR 文档统一使用以下章节：

1. **目标** — 1-2 句话说清楚交付什么。
2. **前置依赖** — 必须先完成哪些 PR / 哪些文档。
3. **涉及文件** — 全部新增 / 修改 / 删除的文件路径。
4. **数据 / 接口契约** — 新表、新字段、新 API、新事件。
5. **实施步骤** — 按编号小节展开，关键处给代码骨架或 UI 草图。
6. **验证** — `cargo` / `npm` 命令、手动操作清单、回归点。
7. **风险与回滚** — 失败时如何回退或灰度。

## 约定

- velamq-bench 定位为**单进程压测控制台**，本目录所有 PR 均不引入多 agent / 集群协调；如需扩展再起单独立项。
- 后端命令默认在仓库根 `velamq-bench/` 下执行（`cargo check` / `cargo test`）。
- 前端 PR-3 之后默认在 `velamq-bench/web/` 下执行 `npm run dev` / `npm run build`，构建产物落到 `web/dist/`，axum 改为 serve `dist`。
- 所有 schema 变更走 migration（`storage.rs::run_migrations`），不就地改 DDL。
- 老的 `bench_specimens` / `bench_templates` 表保留为兼容视图，至少经过 1 个 PR 周期再下线。
- 新枚举字符串 key（如 `LoadShape::Ramp`）一旦确定不再改名；新协议字段一律 `#[serde(default)]` 防止破坏旧数据。
- i18n 新文案必须同时在 `web/locales/en.json` 和 `web/locales/zh-CN.json` 中补齐。
- 提交前缀：`feat(api):` / `feat(web):` / `refactor(model):` / `chore(docs):` / `chore(i18n):`。

## 与总文档的关系

本目录是 [`docs/redesign-plan.md`](../redesign-plan.md) 的**操作分册**。如果总文档与本目录的描述出现冲突，以本目录的 PR 文档为准；总文档作为产品意图与决策依据保留。
