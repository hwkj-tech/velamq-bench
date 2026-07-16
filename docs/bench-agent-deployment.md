# VelaMQ Bench Agent 部署指南

`velamq-bench-agent` 是专用于 MQTT 分布式压测的无界面执行节点。它只主动访问控制面，不需要向控制面开放入站端口。

## 1. 控制面

首次接入节点前，为控制面设置强随机 bootstrap token，并通过 HTTPS 暴露服务：

```bash
export VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN='replace-with-a-long-random-secret'
./velamq-bench
```

bootstrap token 只用于首次注册。注册成功后，每个节点都会得到独立 token；控制面数据库只保存 token 的 SHA-256 摘要。

## 2. Agent 配置

| 环境变量 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `VELAMQ_CONTROL_URL` | 是 | `http://127.0.0.1:8088` | 控制面地址，生产使用 HTTPS |
| `VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN` | 首次注册 | 空 | 首次注册口令，注册后可移除 |
| `VELAMQ_BENCH_AGENT_DATA_DIR` | 否 | `data/agent` | 身份、SQLite 和执行数据目录 |
| `VELAMQ_BENCH_AGENT_IDENTITY` | 否 | `<data-dir>/identity.json` | 节点身份文件路径 |
| `VELAMQ_BENCH_AGENT_NAME` | 否 | 主机名 | Nodes 页面显示名称 |
| `VELAMQ_BENCH_AGENT_LABELS` | 否 | 空 | 逗号分隔标签，例如 `region=cn-east,tier=load` |
| `VELAMQ_BENCH_AGENT_MAX_CLIENTS` | 否 | `50000` | 调度容量权重，不是系统资源硬限制 |

身份文件包含节点 token，权限应限制为运行用户可读。不要把身份文件、bootstrap token 或 Broker 私钥提交到 Git。

## 3. Linux systemd

将 Agent 发布包解压到 `/opt/velamq-bench-agent`，把 `docs/deploy/velamq-bench-agent.service` 安装到 `/etc/systemd/system/`，并创建 `/etc/velamq-bench/agent.env`：

```ini
VELAMQ_CONTROL_URL=https://bench.example.com
VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN=replace-for-first-registration
VELAMQ_BENCH_AGENT_NAME=load-node-01
VELAMQ_BENCH_AGENT_LABELS=region=cn-east,tier=load
VELAMQ_BENCH_AGENT_MAX_CLIENTS=50000
```

```bash
sudo install -d -o velamq-bench -g velamq-bench /var/lib/velamq-bench-agent
sudo systemctl daemon-reload
sudo systemctl enable --now velamq-bench-agent
sudo journalctl -u velamq-bench-agent -f
```

节点首次出现在 Nodes 页面后，从 `agent.env` 删除 bootstrap token 并重启服务。

## 4. macOS

可先以前台进程验证，再用 launchd 或进程管理器托管：

```bash
VELAMQ_CONTROL_URL=https://bench.example.com \
VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN=replace-for-first-registration \
VELAMQ_BENCH_AGENT_DATA_DIR="$HOME/Library/Application Support/VelaMQ Bench Agent" \
./velamq-bench-agent
```

使用 launchd 时，把环境变量写入受限权限的 plist，并将 `KeepAlive` 设为 `true`。首次注册后删除 plist 中的 bootstrap token。

## 5. Windows

在 PowerShell 中先验证连接：

```powershell
$env:VELAMQ_CONTROL_URL = 'https://bench.example.com'
$env:VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN = 'replace-for-first-registration'
$env:VELAMQ_BENCH_AGENT_DATA_DIR = 'C:\ProgramData\VelaMQ Bench Agent'
$env:VELAMQ_BENCH_AGENT_NAME = $env:COMPUTERNAME
.\velamq-bench-agent.exe
```

生产环境可通过 Windows 服务包装器或任务计划程序以专用低权限账号启动，并设置“失败后重新启动”。首次注册后从服务环境中删除 bootstrap token。

## 6. 运维流程

- 扩容：启动新 Agent，设置标签和容量，在 Nodes 页面确认在线。
- 维护：先将节点设为 Drain，等待当前任务结束，再停止服务。
- 下线：Drain、停止服务，然后在 Nodes 页面删除节点。
- token 泄露或身份文件丢失：删除控制面中的旧节点和本地身份文件，使用 bootstrap token 重新注册。
- 排障：检查控制面 HTTPS/DNS、系统时钟、Nodes 的最后心跳、节点任务状态和任务日志。

控制面与 Agent 当前使用协议版本 `1`。升级时先升级控制面，再滚动升级已 Drain 的 Agent。
