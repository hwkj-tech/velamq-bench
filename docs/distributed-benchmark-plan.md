# VelaMQ Bench 分布式压测实施计划

## 当前实施状态

截至当前版本，Phase 1-5 的核心链路已经落地：节点注册与心跳、节点管理、任务租约与 ACK、远程场景快照执行、Selected/Even/Capacity Weighted 调度、非重叠客户端编号、指标与日志幂等上传、全局/节点曲线、延迟直方图合并、停止控制和 CSV 导出。Linux musl、macOS、Windows Release 均分别生成 `velamq-bench` 服务包和 `velamq-bench-agent` 节点包，并已使用两个真实 Agent 完成端到端验证。

Phase 6 中仍作为后续加固项保留：磁盘级离线上传队列、Agent 进程重启后恢复未完成租约、统一 `start_at` 屏障、token 轮换/吊销、控制面 RBAC、故障注入与大规模 Soak 验证。这些项目不阻塞当前的基础分布式压测，但在跨公网或无人值守生产环境投用前应完成。

## 1. 目标与边界

把当前单进程压测控制台升级为“中心控制面 + 多个远程执行 Agent”的分布式压测平台。控制面负责节点管理、场景快照、调度、任务状态、结果汇总和审计；Agent 只负责确定性执行已签名/鉴权的压测任务、缓存运行数据并回传结果。

首个可用版本必须支持：

- 动态注册 Linux、Windows、macOS Agent，展示在线状态、版本、能力、标签和当前任务。
- 指定节点、节点组、平均分配、按最大客户端容量加权分配。
- 将一个 Scenario 固化为不可变快照并拆分为多个 NodeTask。
- Agent 拉取任务、获取租约、幂等执行、续租、停止和回传最终状态。
- 按节点查看连接数、吞吐、错误、日志和延迟；中心端生成全局汇总与导出报告。
- Agent 失联时不重复启动同一任务；租约过期后按任务策略标记失败或重新排队。

控制通道不得依赖正在被压测的 MQTT Broker。第一阶段使用 HTTPS Long Poll + Bearer Token，便于穿透 NAT、代理与防火墙；生产部署由反向代理终止 TLS。后续可升级为 mTLS gRPC Stream，但保持同一消息契约。

## 2. 部署拓扑

```text
Browser
   |
VelaMQ Control Plane ---- SQLite(MVP) / PostgreSQL(HA)
   | HTTPS outbound long poll
   +---- velamq-bench-agent @ node-a ---- target MQTT broker
   +---- velamq-bench-agent @ node-b ---- target MQTT broker
   +---- velamq-bench-agent @ node-c ---- target MQTT broker
```

- Control Plane：Web/API、NodeRegistry、Scheduler、TaskCoordinator、MetricAggregator、ReportService。
- Agent：ControlClient、LeaseKeeper、LocalExecutor、MetricBuffer、LogBuffer、LocalState。
- Broker：仅作为被压测对象，不承载控制消息。

## 3. 核心数据模型

### AgentNode

字段：`id`、`name`、`token_hash`、`status`、`enabled`、`labels`、`os`、`arch`、`version`、`cpu_cores`、`memory_bytes`、`max_clients`、`features`、`remote_addr`、`last_seen_at`、`created_at`、`updated_at`。

状态：`online | busy | draining | offline | disabled`。服务端以最后心跳时间计算在线状态，不信任 Agent 自报的 `online`。

### DistributedRun

字段：`id`、`scenario_id`、`scenario_snapshot_json`、`strategy`、`requested_nodes_json`、`status`、`failure_policy`、`created_at`、`started_at`、`stopped_at`。

状态：`pending -> scheduling -> running -> completed | partial | failed | stopped`。

### AgentTask

字段：`id`、`distributed_run_id`、`node_id`、`attempt`、`idempotency_key`、`scenario_slice_json`、`status`、`lease_id`、`lease_expires_at`、`not_before`、`started_at`、`finished_at`、`error`。

状态：`queued -> leased -> running -> completed | failed | stopped | expired`。

### TaskMetric / TaskLog / TaskEvent

- Metric 使用 `(task_id, sequence)` 去重，保存节点时间与服务端接收时间。
- Log 使用单调递增 sequence，支持断线续传与分段压缩。
- 延迟使用可合并直方图桶，不上传后再平均 P95/P99。

## 4. Agent 控制协议

所有 Agent 请求携带：

```text
Authorization: Bearer <agent-token>
X-VelaMQ-Agent-Id: <agent-id>
X-VelaMQ-Protocol-Version: 1
```

接口契约：

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| POST | `/api/v2/agents/register` | 使用 bootstrap token 首次注册或恢复身份 |
| POST | `/api/v2/agents/{id}/heartbeat` | 上报能力、当前任务并续租 |
| GET | `/api/v2/agents/{id}/tasks/next?wait=25` | 长轮询领取任务 |
| POST | `/api/v2/agent-tasks/{id}/ack` | ACK 租约并进入 running |
| POST | `/api/v2/agent-tasks/{id}/metrics` | 批量上传指标和延迟桶 |
| POST | `/api/v2/agent-tasks/{id}/logs` | 批量上传日志 |
| POST | `/api/v2/agent-tasks/{id}/complete` | 上传最终状态与摘要 |
| GET | `/api/v2/agent-tasks/{id}/control` | 查询 stop/drain 控制指令 |

幂等规则：

- 注册以持久化的 `agent_instance_id` 为幂等键。
- 任务以 `idempotency_key = distributed_run_id/node_id/attempt` 去重。
- 指标和日志以 `(task_id, sequence)` 去重。
- Agent 重启后读取本地状态；同一租约有效时恢复上传，不重新创建运行。

## 5. 调度算法

调度前过滤：`enabled=true`、心跳未过期、协议版本兼容、功能满足场景要求、标签匹配、没有 draining。

- 指定节点：只使用用户选择的节点。
- 平均分配：每个 workload 的 `clients` 以商和余数分配，保证总数精确不变。
- 容量加权：`share_i = total * max_clients_i / sum(max_clients)`，最后按最大余数法补齐。
- 连接速率和消息速率按客户端份额同比拆分；`start_number` 使用连续不重叠区间。
- 每个任务保存完整场景切片，不依赖后续被编辑的 Scenario/Broker/Payload 配置。
- 同一分布式运行使用控制面生成的 `start_at`，Agent 根据 UTC 时钟等待；记录实际启动偏差。

默认失败策略为 `partial`：其他节点继续执行，汇总明确标记缺失节点。可选 `fail_fast` 和 `retry_once`。

## 6. 指标与汇总

Agent 每秒上传一次批量快照，网络中断时写本地有界缓冲，恢复后按 sequence 补传。

- `connected/published/received/errors`：节点计数求和。
- `publish_rate/receive_rate/connect_rate/error_rate`：同一时间窗口求和。
- `latency_avg`：使用 `sum_latency / latency_count` 加权。
- P50/P90/P95/P99/P99.9/Max：合并 HDR 风格指数桶后重新计算。
- 时间轴：控制端按统一的分布式运行 `started_at` 对齐，保留 Agent 原始时间戳用于诊断时钟漂移。
- 汇总报告同时包含全局视图、节点贡献、失败节点、启动偏差、丢失序列和补传情况。

## 7. 安全设计

- bootstrap token 与 Agent token 分离；数据库只保存 token hash，明文仅注册时返回一次。
- Agent API 和用户 API 权限分离；生产环境必须使用 HTTPS。
- 支持 token 轮换、禁用节点、drain、删除以及审计记录。
- 任务只允许声明式 Scenario，不执行 shell、脚本或任意文件路径。
- Broker 密码和私钥按最小范围随任务下发；后续接入密钥加密或外部 Secret Manager。
- 日志对密码、token、私钥和 Authorization 头脱敏。
- 请求限制、payload 上限、sequence 窗口和租约校验在服务端强制执行。

## 8. UI 信息架构

- 新增“节点”页面：状态卡片、节点表格、标签、能力、最后心跳、当前任务、启停与 drain。
- Scenario 运行对话框增加“本机 / 分布式”、调度策略、节点筛选和预估分配。
- Run Detail 增加“汇总 / 节点 / 日志 / 调度”页签。
- 图表支持全局曲线、选中节点 overlay、贡献占比和异常节点标记。
- 导出 ZIP/PDF/CSV 包含 `summary`、`nodes/*`、`logs/*` 和调度快照。

## 9. 实施阶段

### Phase 1：节点控制面

- 新增 cluster domain、数据库迁移、NodeRegistry、Agent 认证中间件。
- 完成注册、心跳、列表、编辑标签、enable/disable、离线判定。
- 增加节点管理 UI。

### Phase 2：Agent 与任务闭环

- 新增 `velamq-bench-agent` 二进制和本地身份文件。
- 完成任务队列、租约、长轮询、ACK、续租、停止、完成状态。
- 单节点远程执行 Scenario 并上传快照/日志。

### Phase 3：多节点调度

- 实现场景切片、三种策略、统一 start_at、失败策略。
- 增加分布式运行 API 与运行入口 UI。

### Phase 4：精确指标汇总

- Sampler 输出可合并延迟桶。
- 批量指标、日志补传、中心聚合、节点明细与图表。

### Phase 5：可靠性与安全

- 离线、租约过期、Agent 重启、重复消息、网络抖动、停止竞态测试。
- token 轮换、审计、限流、脱敏、反向代理 TLS 部署示例。

### Phase 6：发布

- 每个平台 Release 分别生成 `velamq-bench` 服务包和 `velamq-bench-agent` 节点包。
- Linux 提供 systemd 示例，Windows 提供服务安装说明，macOS 提供 launchd 示例。
- 双 Agent 端到端测试通过后升级次版本并发布。

## 10. 验收标准

- 两个远程 Agent 注册后 10 秒内显示在线，停止 Agent 后在阈值内显示离线。
- 1001 客户端平均拆到三个节点时总数仍为 1001，client id 区间无重叠。
- Agent 重复领取、重复 ACK、重复上传不会重复执行或重复计数。
- 任一 Agent 断网恢复后补传数据，中心端能检测缺失 sequence。
- 全局计数等于节点计数之和；合并直方图计算结果与离线基准一致。
- 分布式运行可停止、可查看每节点错误、可导出全局和节点明细。
- Linux musl、Windows、macOS Release 均提供独立且可运行的 Agent 包。

## 11. 兼容与回滚

- 保留现有本机运行 API 和 UI，分布式运行是新增模式。
- 所有新增字段使用 serde default，数据库仅追加迁移。
- Agent 协议带版本号；控制面至少兼容当前和前一个协议版本。
- 每阶段可通过 `VELAMQ_CLUSTER_ENABLED=false` 隐藏集群入口并回退到本机运行。
