# P6-E.2 聊天流 durable run 生命周期

日期：2026-08-30

## 本阶段范围

本轮只把现有 `/api/chat/send/stream` 接入 P6-E.1 已落地的 `runtime_runs` / `runtime_run_events` 持久生命周期，不接入新的聊天协议，不修改 Goal、Queue、Approval 或前端，也没有触碰运行中的 8765 服务。

## 实现记录

- 在 `main.rs` 的聊天 turn 注册表中记录可选 `run_id`，并用 `getrandom` 生成不可猜的 `run-chat-*` id 与 claim token；随机源失败直接拒绝创建。
- 新流创建时以 SQLite Immediate 事务同时写入 `runtime_runs(state=accepted)` 与 `run.accepted`；worker 启动时以 claim-token CAS 推进到 `running` 并追加 `run.started`。
- `ChatTurnGuard` 的正常完成和 Drop 都经由 SQLite 终态 CAS；`stop_requested` 优先收敛为 `interrupted`，Drop 在未取消时收敛为 `failed`，终态事件最多一条。事务提交成功后才广播事件。
- 旧 `/api/chat/turn/interrupt` 保留原 DTO、精确 session/room/turn 校验和 ACK 语义；新 durable row 先按完整 scope 执行 DB CAS，再唤醒内存 cancellation；无 row 时才保留旧 registry 迁移 fallback。通用 `/api/runs/{run_id}/interrupt` 也会唤醒匹配的聊天 worker。
- `started` / `done` SSE 保持原 `turn_id` 与状态字段，新增可选 `run_id`；新流始终写入 `Some(run_id)`。流末尾只有 SQLite 确认 `Completed` 后才发 `chat.completed` 宠物事件，避免终结持久化失败时对客户端声称完成。
- 增加生命周期、stop-first、complete-first、作用域隔离、通用中断唤醒、SSE JSON 兼容和 guard Drop/终结失败测试；将旧静态契约改为按 `api_chat_send_stream` 函数片段检查，不依赖单行空白布局。

## 验证

- `cargo test -p coolzhu-web-console chat_runtime_ --offline`：7 passed，0 failed。
- `cargo test -p coolzhu-web-console chat_stream_ --offline`：2 passed，0 failed。
- `cargo test -p coolzhu-web-console --offline`：868 passed，0 failed；doc-tests 0 passed，0 failed。
- `cargo build -p coolzhu-web-console --offline`：成功，生成 `target/debug/coolzhu-web-console.exe`。
- `git diff --check`：退出码 0；仅有现有 checkout 的 LF/CRLF 提示。

本轮未做浏览器视觉验收、未启动或停止服务、未提交或推送；编译器报告的既有 dead-code/unreachable warnings 不影响构建与测试结果。
