# 2026-08-24 推理过程历史回放

## 本轮目标

修复“流式期间能看到 reasoning、刷新聊天室后却消失”的回放断点，并把已脱敏的 reasoning 摘要纳入会话 JSONL 事件类型。

## 实现

- 前端历史消息加载和非流式回退明确以 `includeReasoning: true` 保留已完成的 `kind=reasoning` 卡片；TTS 及普通完成态过滤仍默认排除 reasoning，避免把思考过程朗读为最终答复。
- `history_message_item_kind` 将 reasoning 映射为独立 `reasoning` item；JSONL 投影新增 `reasoning.completed`，payload 标记 `visibility=summary`、`redacted=false`，并保留稳定 item id/顺序。
- 记忆窗口 JSONL 校验状态增加 Reasoning 计数，与 ToolCall/ToolResult 并列显示。

## 本地验证

- `cargo fmt --package coolzhu-web-console`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `session_agent_events_preserve_tool_call_and_result_contract`：通过，覆盖 reasoning、ToolCall、ToolResult 顺序和 payload。
- `web_frontend_keeps_completed_reasoning_cards_and_tool_results`：通过。
- `web_frontend_memory_window_connects_summary_prompt_context_and_governance`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过（仅保留仓库既有 warning）。
- 隔离运行时 `COOLZHU_RUNTIME_DIR=tmp/reasoning-history-runtime-20260824`、`127.0.0.1:8801`：
  - `/api/chat/rooms/main-room/messages?limit=80` 返回 reasoning 消息；
  - `/api/sessions/session-fork-1787498733821/events?limit=80` 返回 200、26 行 sequence 0..25，包含 1 条 `reasoning.completed`，payload 可回读摘要和 item id；
  - 浏览器首次加载及刷新后均显示“模型思考摘要”卡片，截图保留。

## 证据

- API/JSONL：`tmp/reasoning-history-api-evidence-round6.json`。
- 首次加载截图：`tmp/reasoning-history-ui-evidence-round6.png`。
- 刷新后截图：`tmp/reasoning-history-ui-evidence-round6-refresh.png`。

## 备注

fixture 仅使用已脱敏的本地 reasoning 文本；未调用真实 GLM-5.2/agnes，也未执行真实键鼠或浏览器动作。隔离服务验证完成后停止，不影响已安装的 8765 服务。
