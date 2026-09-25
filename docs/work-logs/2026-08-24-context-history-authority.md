# 2026-08-24 上下文历史权威边界与加载证据

## 目标

继续 Codex harness 功能差异计划中的 P0 上下文边界项，明确模型下一轮实际读取的历史来源，避免把 session messages 的持久化/回放投影误认为模型输入。

## 实现

- `ContextAssembly` 新增 `history_selection`：
  - `source` 固定为 `chat_room.messages`，明确聊天室消息投影是模型上下文的权威历史来源；
  - `candidate_ids` 记录进入历史选择器的聊天室消息；
  - `selected_ids` 记录真正转换为 `InputMessage` 并发送给模型的消息；
  - `excluded_ids` 记录因 context reset floor、临时消息、旧工具拒绝污染或 token 预算等原因未注入的候选。
- 记忆窗口 Context Preview 增加 `history_source / candidates / loaded / excluded` 行，与已有 memory selection 证据并列展示。
- 未改变模型请求内容；session messages 仍保留用于历史、事件和恢复投影，但不会自动越过聊天室历史边界进入下一轮模型输入。

## 本地验证

- `cargo fmt --package coolzhu-web-console`
- `node --check modules/gui-web/packages/web-console/src/app.js`
- 上下文构建、历史 floor 和记忆窗口静态接线定向测试通过。
- `cargo build -p coolzhu-web-console --offline` 通过（既有 warning，无新增错误）。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：835 passed / 0 failed，日志见 `tmp/round7-web-tests.log`。

## 隔离运行证据

使用 `tmp/reasoning-history-runtime-20260824` 和 `127.0.0.1:8801` 启动最新构建，调用：

`GET /api/sessions/mario-demo/context-preview?room_id=main-room&prompt=请继续处理当前任务`

返回结果确认：

- `history_selection.source = chat_room.messages`；
- 聊天室候选 15 条，经过 reset floor 后实际加载 11 条，排除 4 条；
- 记忆召回策略为 `prompt_fallback`，2 条 bead 均进入本轮；
- `runtime_snapshot.chat_room_id = main-room`，`context_snapshot_id` 与预览一致；
- 同一隔离运行目录下 session messages 为 12 条、聊天室消息为 15 条，模型上下文只从聊天室投影选择，未把 session-only 的 `reasoning-refresh-evidence-1` 自动注入。

证据文件：

- `tmp/context-authority-api-evidence-round7.json`
- `tmp/context-authority-sources-round7.json`
- `tmp/context-authority-ui-evidence-round7.png`
- `tmp/context-authority-ui-evidence-round7-context-bottom.png`

运行结束后已停止隔离进程并确认 8801 无监听；用户已安装的 8765 进程未触碰。

## 后续

下一轮可将该选择证据关联到统一 AgentEvent/turn projection，使 provider 请求日志、恢复后的下一轮输入和 Context Preview 共用同一 snapshot/sequence；本轮不改变已有协议字段，避免破坏旧客户端。
