# 2026-08-24 ToolCall/ToolResult 事件契约

## 本轮目标

继续 AgentEvent/JSONL 优先级，补齐工具调用与工具结果的可回放类型，确保一次模型工具请求不会只留下摘要而丢失终态。

## 实现

- `SessionHistoryItemDto` 增加可选 `tool_call_id/tool_name/tool_status/tool_route` 元数据；兼容旧会话的 `tool-summary`、`tool-result` 文本格式，不回填原始参数，避免把潜在凭据扩散到事件回放。
- history 事件按 item kind 投影为 `tool.call`、`tool.result` 或普通 `item.completed`；工具 payload 同时保留 item 和稳定 tool_call 摘要。
- 模型工具调用现在持久化成成对的 `tool-summary`（ToolCall）与安全截断的 `tool-result` 消息，使用同一稳定 call id 关联；流式和 Goal 路径同步使用该成对消息。
- 记忆窗口 JSONL 校验状态显示 `ToolCall N / ToolResult M` 计数，便于实机检查工具事件是否丢失。

## 本地验证

- `cargo fmt --package coolzhu-web-console`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `session_agent_events_preserve_tool_call_and_result_contract`：通过。
- 前端静态契约测试：通过。
- `cargo build -p coolzhu-web-console --offline`：通过（仅保留仓库既有 warning）。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：通过，835 passed、0 failed。
- 流式 ToolCall/ToolResult 两个 dispatch 分支也完成同一对消息的 SSE 发出和上下文持久化；`node --check` 与格式化复跑通过。
- 隔离运行时（`COOLZHU_RUNTIME_DIR=tmp/memory-jobs-runtime-20260823`，端口 8801）：
  - 发送一次 `computer_use.perform` 浏览器点击意图；因聊天室不是 full-access，真实执行按安全闸门阻断；
  - `/api/sessions/session-fork-1787498733821/events?limit=10` 返回 200、`application/x-ndjson`，25 行 sequence 0..24；
  - 事件包含 `tool.call` 2 条和 `tool.result` 1 条，最新 ToolCall/ToolResult 共享 `tool-call-27cc24d6042c5ab4`；
  - 记忆窗口校验状态显示 `25 events · coolzhu.agent.event.v1 · JSONL 有序 · ToolCall 2 / ToolResult 1`。

## 证据

- API 证据：`tmp/tool-events-api-round5-latest.json`、`tmp/tool-events-computer-use-send-round5-latest.json`。
- 浏览器实机截图：`tmp/tool-events-ui-evidence-round5.png`。

## 备注

本轮只使用隔离运行目录和端口，computer-use 未获得 full-access，不触碰设备真实窗口；完成截图后已停止隔离服务。真实 GLM 凭据未配置，模型请求走项目既有本地回退，但工具权限与事件持久化链路按真实后端执行。
