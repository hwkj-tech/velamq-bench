# 设计总览

## 目标

把 velamq-bench 从「单 run + 大表单 + 五张并排图」升级成「Scenario 编排 + Tab 化 Run Detail + 跨 run 对比」的压测控制台。本文先固化术语、信息架构、视觉规范，后续 PR 文档默认沿用。

## 1. 术语与领域模型

| 术语 | 角色 | 关系 |
| --- | --- | --- |
| **BrokerProfile** | 一个可复用的 broker 连接定义（host/port/TLS/auth/keepalive） | 被 Workload 引用 |
| **PayloadProfile** | payload 生成策略（fixed_bytes / json_template / csv_replay / counter） | 被 Workload 引用 |
| **LoadProfile** | 速率剖面（flat / ramp / step / soak / spike），驱动连接速率与消息速率 | Workload 内联 |
| **Workload** | 一个原子压测单元：mode（pub/sub/conn）+ broker + payload + load profile + topic 分布 | Scenario 内嵌；可在 Scenario 内并发 |
| **Scenario** | 一个或多个 Workload 的编排（顺序 / 并发） + 元数据（名称、tag、描述） | 可保存为 Template；执行后产生 Run |
| **Template** | 已保存的 Scenario 草稿（不绑定到具体 Run） | 旧的 `bench_templates` 升级版 |
| **Run** | 一次 Scenario 执行，包含 N 个 RunWorkload + 全部 metric snapshots + annotations | 可标记为 Baseline |
| **RunWorkload** | 某次 Run 中某个 Workload 的执行记录（独立采样） | 与 Run 1\:N |
| **Annotation** | 时间轴上的一个事件标记（手动 / 自动 SLA 触发） | 与 Run 1\:N |
| **Baseline** | 被标记为基准的 Run（每个 Scenario 最多 1 个） | 通过 `scenarios.baseline_run_id` 引用 |

旧概念映射

- `BenchConfig` → `Workload` + `BrokerProfile` + `PayloadProfile` + `LoadProfile` 拆分。
- `Specimen`（运行元数据）→ `Run` 自身字段 (`name`, `tags`, `description`)，不再单独建表。
- `BenchMode::{Conn,Sub,Pub}` → `Workload.kind`，多个 Workload 共存于一个 Scenario。

## 2. 信息架构（IA）

```
┌────────────────────────────── Topbar ──────────────────────────────────┐
│ Logo  Quick Bench  ●Status  RunId  Lang  Theme  Help                   │
├────────┬────────────────────────────────────────────────────────────────┤
│        │                                                               │
│ Side   │   Page workspace                                               │
│ Nav    │                                                                │
│        │                                                                │
│ ▸ Dashboard                                                             │
│ ▸ Runs                                                                  │
│ ▸ Compare                                                               │
│ ▸ Scenarios                                                             │
│ ▸ Templates                                                             │
│ ▸ Settings                                                              │
│   ├ Broker Profiles                                                     │
│   ├ Payload Profiles                                                    │
│   └ Network Bind                                                        │
│                                                                         │
└────────┴────────────────────────────────────────────────────────────────┘
```

页面职责

- **Dashboard**：最近 7 天 KPI（总 published / 总 received / P95 延迟 / 错误率），最近 10 个 run 卡片，与 baseline 的偏离 top3，正在运行的 run 实时简卡。
- **Runs**：列表 + 过滤 + 排序，行点击进入 Run Detail；批量勾选可触发 Compare。
- **Compare**：选 2–4 个 run，KPI delta 表格 + 多图 overlay（吞吐 / 延迟 / 错误率 / 连接）。
- **Scenarios**：scenario 卡片网格，点击进入 Scenario Detail（含历史 run 时间线 + 配置预览 + 一键复跑）。
- **Templates**：保存的 scenario 草稿管理（旧 templates 兼容入口）。
- **Settings → Broker / Payload Profiles**：可复用配置 CRUD；Network Bind 把现有 NIC 选择器收进来。

二级页面

- `Run Detail`：`Overview / Latency / Throughput / Connections / Errors / Logs / Config / Notes` 共 8 个 Tab。
- `Scenario Builder`：三步向导（Broker → Workload(s) → Profile & Schedule），每步带预览。
- `Quick Bench`：Topbar 上的一键弹窗，2 个字段（broker + 模式）即可启动一次默认 Workload。

## 3. 关键页面布局草图

### 3.1 Dashboard

```
┌─ KPI strip ────────────────────────────────────────────────────────┐
│ Pub 12.4M ↑ │ Recv 12.3M │ P95 18ms ↓3% │ Err 0.04% ↑0.01pp │ Run 4 │
└────────────────────────────────────────────────────────────────────┘
┌─ Active Runs ─────────────────────┐ ┌─ Regressions vs Baseline ───┐
│ ● run-1234 pub  64% / 03:12 left  │ │ scn-iot-prod  P95 +18% 🔺   │
│ ● run-1235 sub  100% pending stop │ │ scn-edge-1k   Err +0.2pp 🔺 │
└───────────────────────────────────┘ └─────────────────────────────┘
┌─ Recent Runs ───────────────────────────────────────────────────────┐
│ [card] [card] [card] [card] [card] [card]                           │
└────────────────────────────────────────────────────────────────────┘
```

### 3.2 Run Detail（Tab 化）

```
┌ Run-1234 · scenario "iot-prod" · pub+sub · ⏱ 04:12 ──── [Stop] [⋯] ┐
│ Overview │ Latency │ Throughput │ Connections │ Errors │ Logs │ Config │ Notes │
├────────────────────────────────────────────────────────────────────────────────┤
│   Overview Tab                                                                  │
│   ┌── KPI grid (6 卡) ──┐  ┌── 主图 throughput 60s ──────────────────────┐    │
│   │ Pub  864k  ↑12%     │  │   pub line / recv line / annotations         │    │
│   │ Recv 862k  ↑12%     │  └─────────────────────────────────────────────┘    │
│   │ P95  18ms  ↓3%      │  ┌── Latency percentile band 60s ───────────────┐    │
│   │ Err  0.04% ↑0.01pp  │  │   shaded p50/p90/p95/p99/p99.9                │    │
│   │ Conn 1024            │  └─────────────────────────────────────────────┘    │
│   │ Workloads 2 ●●       │                                                       │
│   └─────────────────────┘                                                       │
│   Workload mini cards：每个 workload 一行迷你卡（吞吐 / 延迟 / 状态）          │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 3.3 Scenario Builder

```
Step 1 / 3  ●○○   Broker
┌── Saved profiles ──┐  ┌── Inline form ──┐
│ ○ local-mosquitto  │  │ host  port  tls │
│ ○ ec2-emqx-prod    │  │ user  password  │
│ + New              │  └─────────────────┘
└────────────────────┘

Step 2 / 3  ●●○   Workloads (可叠加)
┌── Workload card 1 (pub) ─────────────────────────────┐  [+ Add Workload]
│ Topic: bench/{i}   QoS 1   Payload: random-256       │
│ Load:  ramp 0→500/s in 60s, hold 5m, ramp down 30s   │
│                                          [Edit][Del] │
└──────────────────────────────────────────────────────┘
┌── Workload card 2 (sub) ─────────────────────────────┐
│ Topic: bench/+    Group: default   QoS 1             │
│ Clients: 200, all subscribed in 5s                   │
└──────────────────────────────────────────────────────┘

Step 3 / 3  ●●●   Profile & Schedule
  duration / sample interval / NIC bind / annotations / save as template
```

### 3.4 Compare

```
[Select runs]  run-1230 ✓  run-1234 ✓  run-1241 ✓
┌── KPI delta table ──────────────────────────────────────────────────┐
│ Metric        run-1230  run-1234       Δ        run-1241       Δ    │
│ Pub total      864k      872k       +0.9%        801k        −7.3%  │
│ P95 ms          18         16        −11%          22          +22% │
│ Err rate       0.04%    0.05%      +0.01pp       0.30%      +0.26pp │
└─────────────────────────────────────────────────────────────────────┘
┌── Overlay charts (throughput / p95 / error rate / connections) ─────┐
│ 4 stacked charts, each with 3 colored lines + legend toggle         │
└─────────────────────────────────────────────────────────────────────┘
```

## 4. 视觉规范（Design Tokens）

把现有 `:root` 重做（合并两份），统一到一组 token，落到 `web/src/theme/tokens.css`。

| Token | Light | Dark | 用途 |
| --- | --- | --- | --- |
| `--bg-canvas` | `#f5f7fb` | `#0b1220` | 页面背景 |
| `--bg-surface` | `#ffffff` | `#121b2d` | Panel 背景 |
| `--bg-surface-soft` | `#f1f5fb` | `#0f1727` | Panel 内嵌背景 |
| `--fg-default` | `#111827` | `#e6ecf6` | 主文本 |
| `--fg-muted` | `#64748b` | `#9aa8bd` | 次级文本 |
| `--fg-inverse` | `#f8fafc` | `#0b1220` | 深色按钮上的字 |
| `--border-default` | `#d8e1ec` | `#1f2a3d` | 默认边框 |
| `--border-strong` | `#bdc9d8` | `#2d3a52` | 强调边框 |
| `--accent-primary` | `#2563eb` | `#3b82f6` | 主色（按钮 / 链接 / 图表 1） |
| `--accent-success` | `#059669` | `#10b981` | 完成 |
| `--accent-warning` | `#d97706` | `#f59e0b` | 警告 / regression |
| `--accent-danger` | `#dc3f59` | `#f43f5e` | 错误 / 停止 |
| `--chart-1..8` | 见下表 | 见下表 | 图表系列调色板（色盲安全） |

图表调色板（8 色 / 同时光暗适配）

```
1 #2563eb  primary blue       5 #db2777  pink
2 #0891b2  cyan                6 #f59e0b  amber
3 #16a34a  green               7 #6366f1  indigo
4 #dc3f59  red                 8 #14b8a6  teal
```

间距 / 圆角 / 阴影

- 间距：`4 / 8 / 12 / 16 / 24 / 32 / 48`，token 名 `--space-1..7`。
- 圆角：`--radius-sm 6px / --radius-md 10px / --radius-lg 14px / --radius-xl 22px`。
- 阴影：`--shadow-sm / --shadow-md / --shadow-lg`，沿用现 `0 16px 42px rgba(15,23,42,0.08)` 的方向。
- 字体：`Inter, "PingFang SC", "Microsoft YaHei", system-ui` 默认；等宽用 `JetBrains Mono`。

## 5. 关键交互原则

1. **渐进披露**：所有「高级」字段（NIC 绑定 / TLS / payload generator 高级参数）默认折叠，给一个 `Advanced` 开关。
2. **一致的 Empty State**：每张列表 / 每个 Tab 在没有数据时都给「插画 + 一句话 + 主操作按钮」。
3. **可逆操作**：Stop run / 删除 template / 删除 broker profile 都用 `confirm dialog + undo toast`。
4. **可访问**：所有交互组件支持键盘 (`Tab / Enter / Esc / ↑↓`)，主流程过 axe 自检。
5. **实时反馈**：SSE 中断时 Topbar 红点 + Toast；reconnect 后自动 catch-up 缺失的 snapshot。
6. **明暗双主题**：Topbar 切换器，写入 `localStorage`；遵循 `prefers-color-scheme` 默认值。
7. **i18n 兜底**：缺 key 时显示 `[key]`（不显示英文残留），方便翻译扫描。

## 6. 范围与不做的事

不做：

- 多机分布式压测（agent fan-out）。
- 内置 broker（仍依赖外部 MQTT broker）。
- 自定义脚本编程式负载（k6 / Locust 那种 JS / Python DSL，本期不引入）。
- WebSocket / WSS 直连（保留，但不在本期重设计计划之内）。

只做：

- 单进程内的 Scenario 编排和多 workload 并发。
- 既有 metrics 维度的更深度可视化与对比。
- 配置复用与 baseline 回归。
