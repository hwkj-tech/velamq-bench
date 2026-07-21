# VelaMQ Bench

VelaMQ Bench 是一个带 Web 控制台的 MQTT 压测平台，支持本机压测和多节点分布式压测。平台可以管理 MQTT Broker、复用压测场景、自动把任务切分到多个远程 Agent，并汇总连接数、吞吐、错误和延迟曲线。

## 功能概览

- MQTT 3.1.1、MQTT 5.0。
- MQTT、MQTTS、WebSocket、Secure WebSocket。
- 用户名密码、系统 CA、自定义 CA、跳过证书校验、mTLS、ALPN。
- 连接、发布、订阅压测，以及平稳、爬坡、阶梯、浸泡、尖峰负载。
- 本机执行或多节点 Selected、Even、Capacity Weighted 调度。
- 节点注册、心跳、标签、容量、Drain、禁用和任务停止。
- 全局与每节点实时图表，延迟 P50/P90/P95/P99/P99.9 聚合。
- PDF、SVG、CSV、本地运行 Bundle，以及分布式 CSV 导出。

## Web 控制台体验

Web 控制台围绕“配置 → 执行 → 观察 → 对比”设计：

- 仪表盘集中展示控制面状态、实时连接数、发布速率、P95 延迟和最近运行。
- 顶栏 `快速压测` 可以不创建场景，直接使用 Broker 地址发起临时 pub/sub/conn 压测。
- 按 `Ctrl+K`（macOS 为 `⌘K`）可搜索页面、切换主题或打开快速压测。
- Runs 页面支持按名称、描述、标签和状态筛选，并可选择 2–4 次运行进行对比。
- Scenarios 页面展示每个场景的 workload 数、总客户端数和标签，并支持快速搜索与直接运行。
- 小屏设备使用抽屉式导航，运行详情、表格和 KPI 会自动切换为单列布局。

完整操作流程、快捷键与常见问题参见 [Web 控制台使用手册](docs/web-console-guide.md)。

## Release 中的两个安装包

每个平台会生成两个独立安装包，不需要在同一台机器上同时安装：

| 安装包 | 部署位置 | 主要内容 |
| --- | --- | --- |
| `velamq-bench-<version>-<platform>` | 中心服务器/管理机 | `velamq-bench` 服务、Web 控制台、`velamq-connbench` 辅助工具 |
| `velamq-bench-agent-<version>-<platform>` | 每台远程压测机 | `velamq-bench-agent`、Agent 部署文档、systemd 模板 |

支持的产物：

- `linux-x86_64-musl`：Linux x86_64 静态链接包。
- `linux-aarch64-musl`：Linux ARM64 静态链接包。
- `macos-x86_64`：Intel Mac。
- `macos-aarch64`：Apple Silicon Mac。
- `windows-x86_64`：Windows x86_64。

Linux/macOS 使用 `.tar.gz`，Windows 使用 `.zip`。Release 同时提供 `SHA256SUMS`。

## 一、启动 VelaMQ Bench 服务

### 1. 解压服务包

Linux x86_64 示例：

```bash
tar -xzf velamq-bench-v0.3.0-linux-x86_64-musl.tar.gz
cd velamq-bench-v0.3.0-linux-x86_64-musl
```

包内目录结构：

```text
velamq-bench
velamq-connbench
web/dist/
README.md
CHANGELOG.md
```

不要把 `web/dist` 移出当前目录，否则服务无法加载 Web 控制台。

### 2. 仅使用本机压测

```bash
./velamq-bench
```

默认监听 `127.0.0.1:8088`，浏览器打开：

```text
http://127.0.0.1:8088
```

SQLite 数据默认保存在服务程序旁边的 `data/velamq-bench.sqlite3`。

### 3. 开启远程 Agent 注册

需要分布式压测时，给控制面设置首次注册口令：

```bash
export VELAMQ_BIND=0.0.0.0:8088
export VELAMQ_DATA_DIR=/var/lib/velamq-bench
export VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN='replace-with-a-long-random-secret'
./velamq-bench
```

生产环境建议让 `velamq-bench` 仍监听内网地址，在前面使用 Nginx、Caddy 或网关提供 HTTPS。Agent 会把 Broker 用户名、密码、证书和压测结果传给控制面，因此不要通过明文公网 HTTP 使用。

### 服务端环境变量

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `VELAMQ_BIND` | `127.0.0.1:8088` | HTTP 监听地址 |
| `VELAMQ_DATA_DIR` | 程序目录下的 `data` | SQLite 数据目录 |
| `VELAMQ_WEB_ROOT` | 自动检测程序目录下的 `web` | 该目录下必须存在 `dist/index.html` |
| `VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN` | 空 | Agent 首次注册口令；为空时禁止新节点注册 |

## 二、安装远程 Bench Agent

每台负载机只需要下载与操作系统、CPU 架构匹配的 `velamq-bench-agent-*` 包。

### 1. Linux/macOS 前台启动

```bash
tar -xzf velamq-bench-agent-v0.3.0-linux-x86_64-musl.tar.gz
cd velamq-bench-agent-v0.3.0-linux-x86_64-musl

export VELAMQ_CONTROL_URL=https://bench.example.com
export VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN='same-bootstrap-token-as-server'
export VELAMQ_BENCH_AGENT_NAME=load-node-01
export VELAMQ_BENCH_AGENT_LABELS=region=cn-east,tier=load
export VELAMQ_BENCH_AGENT_MAX_CLIENTS=50000
export VELAMQ_BENCH_AGENT_DATA_DIR=/var/lib/velamq-bench-agent
./velamq-bench-agent
```

Agent 只主动访问 `VELAMQ_CONTROL_URL`，不需要开放 Agent 入站端口。

### 2. Windows PowerShell 启动

```powershell
$env:VELAMQ_CONTROL_URL = 'https://bench.example.com'
$env:VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN = 'same-bootstrap-token-as-server'
$env:VELAMQ_BENCH_AGENT_NAME = $env:COMPUTERNAME
$env:VELAMQ_BENCH_AGENT_LABELS = 'region=cn-east,tier=load'
$env:VELAMQ_BENCH_AGENT_MAX_CLIENTS = '50000'
$env:VELAMQ_BENCH_AGENT_DATA_DIR = 'C:\ProgramData\VelaMQ Bench Agent'
.\velamq-bench-agent.exe
```

### 3. 确认注册成功

1. 登录 VelaMQ Bench Web 控制台。
2. 打开 `Nodes` 页面。
3. 确认节点状态为 `Online`，并检查名称、标签、版本和最大客户端数。
4. 节点注册成功后，Agent 会把独立身份写入 `<data-dir>/identity.json`。
5. 从 Agent 启动环境中删除 `VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN`，然后重启 Agent。后续心跳使用节点独立 token，不再需要 bootstrap token。

`identity.json` 包含节点凭证，只允许 Agent 运行账号读取。不要把它复制到其他节点，也不要提交到 Git。

### Agent 环境变量

| 环境变量 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `VELAMQ_CONTROL_URL` | 是 | 无 | 控制面地址，例如 `https://bench.example.com` |
| `VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN` | 首次注册 | 空 | 与服务端一致的首次注册口令 |
| `VELAMQ_BENCH_AGENT_DATA_DIR` | 否 | `data/agent` | Agent 身份、SQLite 和运行数据目录 |
| `VELAMQ_BENCH_AGENT_IDENTITY` | 否 | `<data-dir>/identity.json` | 自定义身份文件路径 |
| `VELAMQ_BENCH_AGENT_NAME` | 否 | 系统主机名 | Nodes 页面显示名称 |
| `VELAMQ_BENCH_AGENT_LABELS` | 否 | 空 | 逗号分隔的调度标签 |
| `VELAMQ_BENCH_AGENT_MAX_CLIENTS` | 否 | `50000` | Capacity Weighted 调度权重，不是操作系统硬限制 |

Linux systemd、macOS launchd、Windows 服务化、升级和故障排查参见 [Bench Agent 部署指南](docs/bench-agent-deployment.md)。

## 三、配置 MQTT Broker

打开 `Settings -> Brokers`，新建 Broker Profile：

1. 选择 MQTT 3.1.1 或 MQTT 5.0。
2. 选择 `mqtt`、`mqtts`、`ws` 或 `wss`。
3. 填写主机、端口、WebSocket Path、Keep Alive 和连接超时。
4. 按需配置用户名密码。
5. TLS 可配置系统根证书、自定义 CA、客户端证书和私钥、ALPN。
6. MQTT 5 可配置 Session Expiry、Receive Maximum、Maximum Packet Size 等属性。
7. 保存后点击连接测试。

生产环境应保持证书校验开启。“跳过证书校验”只用于隔离测试环境。

配置完成后建议先执行“连接测试”，确认协议、端口、认证和 TLS 配置无误，再将 Broker Profile 用于正式场景。

## 四、创建并运行压测场景

### 本机运行

1. 打开 `Scenarios`，创建场景。
2. 添加 Connection、Publish 或 Subscribe workload。
3. 选择 Broker、客户端数、Client ID 模板、Topic、QoS、负载曲线和持续时间。
4. 保存场景后选择本机运行。
5. 在 Run Detail 查看连接、吞吐、延迟、错误和日志。

### 分布式运行

1. 确认需要的 Agent 在 `Nodes` 页面为 Online，且没有被 Drain/Disable。
2. 打开已保存场景的详情页，选择“分布式运行”。
3. 选择调度方式：
   - `Selected`：只使用手工选中的节点，平均分配。
   - `Even`：使用所有满足标签条件的可用节点，平均分配。
   - `Capacity Weighted`：按各节点 `MAX_CLIENTS` 比例分配。
4. 可填写必需标签，例如 `region=cn-east`，只选择匹配节点。
5. 启动后进入 Distributed Run Detail。

控制面会自动切分客户端数、Client ID 起始编号和发送速率，保证同一个 workload 在不同节点的客户端编号不重叠。结果页同时展示全局聚合曲线和每节点执行明细。

## 五、结果与导出

本机 Run Detail 支持：

- PDF 完整报告。
- SVG 图表。
- CSV 指标数据。
- 包含场景、Broker、Payload、指标和备注的 ZIP Bundle。

分布式 Run Detail 支持导出全局和每节点对齐后的 CSV 数据。

API 示例：

```bash
curl 'http://127.0.0.1:8088/api/v2/runs/run-id/report.svg?lang=zh-CN' -o report.svg
curl 'http://127.0.0.1:8088/api/v2/runs/run-id/report.pdf?lang=zh-CN' -o report.pdf
curl 'http://127.0.0.1:8088/api/v2/runs/run-id/report.csv' -o report.csv
curl 'http://127.0.0.1:8088/api/v2/distributed-runs/run-id/report.csv' -o distributed.csv
```

## 六、节点运维

- 维护节点：先在 Nodes 页面执行 Drain，等待当前任务完成，再停止 Agent。
- 恢复节点：启动 Agent 后取消 Drain。
- 永久下线：Drain、停止 Agent，然后从 Nodes 页面删除。
- 身份泄露：删除控制面的旧节点和 Agent 本地 `identity.json`，再用 bootstrap token 重新注册。
- 升级顺序：先升级 `velamq-bench` 服务，再逐台 Drain 并升级 `velamq-bench-agent`。
- 服务端必须备份 `VELAMQ_DATA_DIR`；Agent 的身份文件也应安全备份，但不能在多台机器间复用。

## 七、版本与自动 Release

发布版本同时由 `Cargo.toml` 的 `package.version` 和 Git Tag 控制，两者必须一致。工作流会拒绝版本不一致的构建。

发布 `0.3.0` 示例：

1. 修改 `Cargo.toml`：

   ```toml
   [package]
   version = "0.3.0"
   ```

2. 更新 `Cargo.lock` 和 `CHANGELOG.md`：

   ```bash
   cargo check
   git add Cargo.toml Cargo.lock CHANGELOG.md
   git commit -m 'chore: prepare v0.3.0'
   ```

3. 创建并推送 Tag：

   ```bash
   git tag v0.3.0
   git push origin master
   git push origin v0.3.0
   ```

GitHub Actions 将为每个平台构建服务包和 Agent 包，生成 SHA-256 校验文件并发布到 GitHub Releases。也可以在 Actions 页面手动运行 Release workflow，但输入的 Tag 必须已经存在且与 `Cargo.toml` 一致。

## 八、源码开发

后端：

```bash
cargo run --bin velamq-bench
```

前端：

```bash
cd web
npm install
npm run lint
npm run lint:i18n
npm run lint:a11y
npm run typecheck
npm run build
```

前端本地启动后访问 `http://127.0.0.1:5173`。Vite 会把 `/api` 请求代理到本地 Bench 服务；应先在仓库根目录启动 `cargo run --bin velamq-bench`。

构建全部二进制：

```bash
cargo build --release --bins
```

API 摘要参见 [docs/api.md](docs/api.md)，分布式架构和后续加固项参见 [分布式压测实施计划](docs/distributed-benchmark-plan.md)。旧 `/api/bench/*` API 仍暂时保留兼容性。
