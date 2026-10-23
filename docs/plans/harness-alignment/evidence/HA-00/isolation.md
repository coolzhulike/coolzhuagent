# HA-00 隔离方式核对

状态：**静态核对完成，尚未启动隔离服务**。任何进程级实验启动前必须重新探测端口/日常实例。

## 1. 日常实例（不得触碰）

- 8765 监听：PID `5268`，`C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe`，启动时间 2026-10-17 21:07:34。
- 本节只记录，不发送重启/关闭请求，不替换安装版，不写日常 session DB。

## 2. 本次隔离参数（建议值）

| 项 | 值 |
|---|---|
| 端口 | `127.0.0.1:58611`（2026-10-18 重新探测时空闲；启动前再探测） |
| workspace | `D:\coolzhuagent\tmp\ha00-isolation\workspace` |
| session DB | `...\data\web-sessions.sqlite3` |
| legacy store | `...\data\web-sessions.json` |
| attachments | `...\attachments` |
| user state | `...\user-state` |
| input safety | `...\input-safety` |
| logs | `...\logs` |

环境变量（子进程显式设置）：

```text
COOLZHU_RUNTIME_DIR=<workspace>
COOLZHU_WEB_SESSION_DB=<data>\web-sessions.sqlite3
COOLZHU_WEB_SESSION_STORE=<data>\web-sessions.json
COOLZHU_WEB_ATTACHMENT_STORE=<attachments>
COOLZHU_USER_STATE_ROOT=<user-state>
COOLZHU_INPUT_SAFETY_STATE_ROOT=<input-safety>
COOLZHU_LOG_DIR=<logs>
```

`<workspace>\coolzhu.toml` 需设置：

```toml
[web]
bind_addr = "127.0.0.1:58611"
```

## 3. 为什么不能只换端口

`main.rs:1326` 在取得 writer 锁之前调用：

```rust
app_launcher::instance_restart::stop_owned_instance(&startup_health, &state_root)
```

`instance_restart.rs:24-57` 会：

1. 连接启动地址；
2. 核对监听者是否为当前用户的 `coolzhu-web-console.exe`；
3. 读取该 PID 的重启凭据；
4. 向旧实例发 `POST /api/system/restart`，等待进程结束。

因此如果只换端口、但仍用日常 `user-state`/健康地址，可能命中或影响日常实例。本次隔离同时改端口、workspace、DB/store、user-state、input-safety、log；并且启动前确认目标端口空闲。仍不能排除插件/账号/附件等其他子进程解析日常目录，需在启动后只读核对实际路径。

## 4. 启动前检查清单

- [ ] 目标端口无监听（`Get-NetTCPConnection -LocalPort 58611` 为空）。
- [ ] 日常实例 PID 仍为 5268（或记录新 PID），不被本次操作触碰。
- [ ] 隔离目录为空或已确认可清理；`<workspace>/coolzhu.toml` 的 `bind_addr` 正确。
- [ ] 不设置 `COOLZHU_WEB_SESSION_DB` 为日常库；不设置 `COOLZHU_RUNTIME_DIR` 为日常 workspace。
- [ ] 启动命令使用隔离环境，stdout/stderr 写隔离日志。
- [ ] 启动后只读核对 `/api/workspace`、`/api/diagnostics/health` 的 workspace/DB/build/port 都是隔离值。
- [ ] 验证日常库 mtime/行数未变化，日常实例仍存活。
- [ ] 实验结束停止隔离进程，不向日常实例发重启。

## 5. 未完成

- 尚未创建/启动隔离服务；无进程级隔离回执。
- 尚未验证 `COOLZHU_LOG_DIR` 之外的其他子进程（插件、账号、附件、native recovery、CU）是否完全隔离；启动后需只读核对。
