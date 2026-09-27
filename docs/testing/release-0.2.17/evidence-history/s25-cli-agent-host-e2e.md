# S2.5 默认 CLI → Web → Agent 宿主端到端证据

2026-09-27，在本仓库当前源码离线构建 `coolzhu-web-console` 与 `coolzhu-command-line` 后，用 `tmp/s25_cli_agent_e2e.py` 启动隔离工作区、随机本机端口的 Web 进程及本机假模型，再运行真实 `coolzhu-cli.exe chat`。请求内容与会话均为脚本生成的测试数据。

- CLI 退出码 0；共享聊天 `done.status=completed`。
- CLI 事件依次出现 `Agent` 工具 `requested → running → completed`，工具结果带父 run、父 call、工作区、会话、房间与 turn 身份，子结果为测试标记 `CLI-AGENT-E2E-CHILD-OK`。
- 假模型共收到 3 次请求，其中子 Agent 真实请求 1 次。隔离 SQLite 的 `chat_usage_events` 有 1 条 `request_kind=child_agent, status=completed`，输入 11 tokens、输出 7 tokens；会话和房间均与父请求一致。
- 证据原件留在忽略目录 `tmp/s25-cli-e2e-07197b1dc2/`：`result.json`、`cli.stdout.jsonl`、`cli.stderr.log`、`web.log`。构建日志为 `tmp/s25-cli-build.log`。

首轮脚本把配置写在隔离工作区根，却把 `COOLZHU_RUNTIME_DIR` 指向该工作区子目录，导致自建 debug Web 使用默认 8765；此轮脚本的请求目标始终是预先选定的随机端口，未连上 Web，因此未创建测试 session/room，未启动 CLI，也未向已安装服务发请求。首轮 debug Web 的日志只有启动信息；它会按应用启动逻辑打开用户目录下的结构化诊断日志，并在隔离子目录生成默认配置。仅按可执行文件完整路径和 PID 终止了该轮 debug Web；已安装 Web 与桌面壳未操作。修正运行目录后第二轮隔离端口测试通过。

CLI 最终消息复用了子 Agent 的结果标记；本证据验证宿主工具执行、身份与 usage 归属，不验证模型生成的自然语言收尾质量。显式 `--legacy-runtime` 是另一入口，没有宿主时 Agent 会明确拒绝。
