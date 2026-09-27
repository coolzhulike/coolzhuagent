# LSP 聊天室授权接线与隔离验收

日期：2026-09-27。范围：本轮源码构建，尚未打包或安装。设计审查见 `docs/analysis/2026-09-27-lsp-room-authorization-review.md`。

## 修复行为

- 启动请求必须携带 `expected_workspace`、当前 `session_id` 和 `chat_room_id`。后端核对已捕获工程与当前会话、聊天室；缺失或过期请求不会借用当前页面状态。
- 在复用已运行实例之前检查授权。房间完全访问记录优先；否则回退到当前会话已有的 PowerShell 双确认授权。rustup 和 rust-analyzer 都走既有 `preview_tool_permission`，客户端附带的 `confirmed_twice` 或 `user_authorized` 不参与授权。默认房间写入档位不自动授予外部程序执行。
- 实例保存启动时的工程、会话、聊天室及该工程会话库路径。所有 LSP HTTP 入口持有工程活动 pin，后续状态、诊断、跳转和关闭按实例归属和实时授权检查。诊断与跳转的磁盘同步及服务调用前后均复核；旧请求只可撤销自己捕获的实例。
- 工程、会话或聊天室切换和房间权限更新会撤销旧实例；会话临时完整访问授权主动撤销时，若实例已失去实际授权则关闭。启动期间还检查代际与当前归属，旧启动结果不得在切换后安装。
- 前端启动操作在异步返回后核对捕获的工程、项目代际、会话、房间和文件；旧错误不会覆盖新视图，旧请求不会提前启用新请求的按钮。

## 隔离验证

执行 `cargo build -p coolzhu-web-console --offline` 成功；`node --check`、`rustfmt`、`git diff --check` 通过。最终源码构建 Web SHA-256：`B6A3D5826501681A86CBE5984827EC4AC0240EFE6093EBBC0DADA761D9728325`。构建日志在 `tmp/lsp-room-auth-build-final.log`。

打包前执行现有 Web 全量测试：首次默认并行运行中，`run_model_tool_dispatch_uses_caller_session_for_audit` 遇到一次 SQLite `database is locked`，结果为主程序测试 1255 通过、1 失败、2 忽略，原始日志保留在 `tmp/lsp-room-auth-web-tests.log`。该失败用例单独运行通过；随后用 `--test-threads=1` 串行复验，库测试 8/8、浏览器宿主测试 1/1、主程序测试 1256/1256（另 2 忽略），文档测试 0，日志分别为 `tmp/lsp-room-auth-single-test.log`、`tmp/lsp-room-auth-web-tests-serial.log`。未修改产品代码，Web 二进制 SHA-256 仍为上述值。上一组合的 `module_linkage_smoke` 已有 8/8 通过记录，见 `tmp/final-combined-module-linkage-smoke.log`，本轮未重复运行。

用 `tmp/lsp_room_authorization_verify.py` 复制该二进制到 `tmp/lsp-room-auth-verify/6cde355e36`，在随机端口 `58794` 启动纯临时 Rust 工程。夹具显式配置 `dev_open_permissions=false`，并把会话库、LOCALAPPDATA 指向临时目录；无真实模型、密钥或用户工作区访问。该调试构建若省略此配置会默认开放调试权限，首次无效试跑已排除，不计入以下判定。

| 场景 | 实际结果 |
| --- | --- |
| 缺聊天室、缺工程绑定 | 各 `422` |
| 错聊天室、错工程绑定 | 各 `409` |
| 默认无授权，额外伪造客户端确认字段 | `403`，未取得启动句柄 |
| 当前房间完成双确认后启动、同房间复用 | 各 `200`，复用同一句柄 |
| 已授权房间诊断与定义跳转 | 各 `200` |
| 房间权限撤回后旧句柄诊断/跳转、再次启动 | 旧句柄各 `409`，启动 `403`，状态不再运行 |
| 默认房间下已有会话完整访问授权回退 | 启动 `200`；主动撤销后状态停止、旧句柄 `409` |
| 切至房间 B 后，房间 A 的启动和旧句柄操作 | 旧请求 `409`；B 可单独授权启动，旧句柄不能关闭 B，新实例仍运行 |
| 关闭房间 B 实例 | `200` |

完整脱敏状态码记录：`tmp/lsp-room-auth-http-final.log` 与 `tmp/lsp-room-auth-verify/6cde355e36/result.json`。私有 Web PID `11904` 在脚本结束后确认退出。启动在途切换再切回的 ABA 时序没有用此 HTTP 夹具定时触发；该路径依据代际与归属检查做了代码复核，不能记作运行通过。测试模拟正常 HTTP 操作，不能替代新候选安装版原生窗口的启动、诊断、跳转与停止截图；0.2.19 安装版此前的 `403 danger-outside` 仍是旧包观察结果。
