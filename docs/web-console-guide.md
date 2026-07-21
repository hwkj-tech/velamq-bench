# VelaMQ Bench Web 控制台使用手册

本手册适用于 VelaMQ Bench `v0.3.x` Web 控制台，覆盖首次配置、本机压测、分布式压测、结果分析和日常运维。

## 1. 打开控制台

启动服务：

```bash
./velamq-bench
```

默认访问地址：

```text
http://127.0.0.1:8088
```

生产环境建议通过 HTTPS 网关访问控制台。不要在公网使用明文 HTTP 传输 Broker 密码、证书或 Agent 任务。

## 2. 界面导航

| 页面 | 用途 |
| --- | --- |
| 仪表盘 | 查看控制面状态、实时遥测、最近运行和常用入口 |
| 运行记录 | 搜索、筛选、删除、查看和对比历史运行 |
| 场景 | 创建和复用多 workload 压测计划 |
| 模板 | 从预置负载模型快速创建场景 |
| 节点 | 管理远程 Agent、容量、标签、Drain 和启停状态 |
| 设置 | 管理 Broker、Payload、网卡、偏好和导入导出 |

在桌面端，主导航固定在左侧；在窄屏设备上，点击左上角菜单按钮打开抽屉导航。

### 快捷操作

- `Ctrl+K`：Windows/Linux 打开快捷操作。
- `⌘K`：macOS 打开快捷操作。
- `Esc`：关闭快捷操作、移动端导航或弹层。
- 在快捷操作中输入页面名称可直接跳转，也可以切换主题或打开快速压测。

## 3. 第一次压测

### 3.1 配置 Broker Profile

1. 打开 `设置 → Broker`。
2. 新建 Broker Profile，填写名称、协议、主机和端口。
3. 如果使用 WebSocket，填写 WebSocket Path。
4. 按需设置 MQTT 版本、用户名密码、TLS、mTLS 和 ALPN。
5. 点击连接测试。
6. 测试成功后保存。

生产环境应验证 Broker 证书。“跳过证书校验”只应用于隔离测试环境。

### 3.2 使用快速压测

快速压测适合连通性验证或临时负载测试：

1. 点击顶栏 `快速压测`，也可以按 `Ctrl/⌘+K` 后选择快速压测。
2. 选择已有场景，或者使用临时运行。
3. 临时运行需要填写协议、Broker 地址、端口、模式和客户端数。
4. 点击启动运行。
5. 控制台会自动进入 Run Detail，并开始接收实时指标。

临时运行默认持续 60 秒，使用 QoS 0 和固定负载。需要自定义 Topic、负载曲线、Payload、QoS 或多个 workload 时，应创建正式场景。

## 4. 创建可复用场景

1. 打开 `场景 → 新建场景`。
2. 在 Broker 步骤选择已保存的 Broker Profile。
3. 添加 Connection、Publish 或 Subscribe workload。
4. 设置客户端数、Client ID 模板、Topic、QoS、Payload 和采样间隔。
5. 选择负载曲线：平稳、爬坡、阶梯、浸泡或尖峰。
6. 多个 workload 可以并行或串行执行。
7. 保存后从场景列表直接运行。

场景列表会显示 workload 数、总客户端数和标签。使用顶部搜索框可以按名称、描述或标签过滤。

## 5. 查看运行记录

Runs 页面支持：

- 按名称、描述或标签搜索。
- 按等待中、运行中、已完成、已停止或失败筛选。
- 打开 Run Detail 查看实时和历史指标。
- 删除不再需要的记录；运行中的任务不能删除。
- 选择最多 4 次运行进行对比。

### 5.1 Run Detail

Run Detail 按 Tab 展示：

- Overview：KPI 与 workload 概况。
- Latency：P50/P90/P95/P99/P99.9、直方图和热力图。
- Throughput：发布、接收和连接速率。
- Connections：连接变化与累计连接。
- Errors：错误速率和错误分类。
- Logs：运行日志。
- Config：本次运行的配置快照。
- Notes：运行备注和事件标注。

### 5.2 对比运行

1. 打开运行记录，点击“选择对比”。
2. 勾选 2–4 次运行。
3. 选择基线运行。
4. 查看发布速率、P95、P99 和错误率的 KPI 差异与叠加曲线。
5. 退出对比会清空当前选择。

## 6. 分布式压测

1. 在 Nodes 页面确认 Agent 为 Online。
2. 检查 CPU、内存、最大客户端容量和标签。
3. 打开场景详情，选择分布式运行。
4. 选择 Selected、Even 或 Capacity Weighted 调度方式。
5. 如有需要，填写 `region=cn-east` 等必需标签。
6. 启动后在 Distributed Run Detail 查看全局汇总和每节点明细。

维护节点时，应先执行 Drain，等待当前任务完成后再停止 Agent。离线或禁用节点不会接收新任务。

## 7. 导出与报告

本机运行支持：

- PDF 完整报告。
- SVG 图表。
- CSV 指标。
- 包含场景、Broker、Payload、指标和备注的 ZIP Bundle。

分布式运行支持导出全局及每节点对齐后的 CSV。

## 8. 主题、语言与可访问性

- 顶栏可以切换浅色/深色主题。
- 支持简体中文和 English。
- 主导航、快捷操作、表单和运行 Tab 支持键盘操作。
- 页面提供“跳到主要内容”链接，并适配减少动态效果的系统设置。

## 9. 常见问题

### 页面能打开，但 API 请求失败

确认 `velamq-bench` 服务正在运行，并检查浏览器请求的控制台域名是否与 API 反向代理配置一致。

### 快速压测启动失败

检查 Broker 地址、端口和协议。MQTTS/WSS 还需要确认 CA、Server Name 和证书配置。复杂认证建议先保存 Broker Profile 并执行连接测试。

### 看不到远程节点

检查服务端和 Agent 是否使用相同的 `VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN`，以及 Agent 是否能访问 `VELAMQ_CONTROL_URL`。首次注册后，Agent 会使用 `identity.json` 中的独立凭证。

### 指标没有实时更新

确认浏览器到 `/api/v2/runs/<run-id>/events` 的 SSE 连接没有被代理缓存或超时关闭。Nginx 应关闭该路由的响应缓冲。

### 场景执行规模不符合预期

检查每个 workload 的客户端数和阶段策略。分布式运行还需要检查节点容量、必需标签与调度方式。

## 10. 推荐工作流

```text
连接测试 → 保存 Broker → 创建场景 → 小规模试跑
→ 检查延迟与错误 → 扩大客户端数 → 分布式压测
→ 对比基线 → 导出报告
```

Agent 安装与服务化参见 [Bench Agent 部署指南](bench-agent-deployment.md)，API 调用参见 [API 文档](api.md)。
