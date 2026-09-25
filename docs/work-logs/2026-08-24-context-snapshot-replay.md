# 2026-08-24 上下文快照与 history/AgentEvent 回放关联

## 目标

在上一轮明确聊天室历史权威来源后，把模型回复、session history 和 JSONL AgentEvent 关联到同一个 `context_snapshot_id`，便于排查 provider 请求与恢复后的上下文是否错位。

## 实现

- Context usage 尾部增加机器可读诊断行：
  - `Context snapshot: ctx-*`
  - `history source: chat_room.messages`
  - `history loaded: N`
  - `memory revision: mem-*`
- 该尾部仍由 `strip_context_usage_footer` 在下一轮模型输入前整体移除，因此不会污染语义上下文。
- `SessionHistoryItemDto` 从已持久化回复尾部解析 `context_snapshot_id`；旧消息没有尾部时保持 `null`，兼容历史数据。
- `AgentEventEnvelope` 在 item payload 含快照时同步输出顶层 `context_snapshot_id`，普通消息、reasoning、ToolCall/ToolResult 均保持原有事件类型和顺序。

## 验证

- 定向修复：初次测试断言错误地把普通 `item.completed` 当成嵌套 payload，已按实际 schema 修正，并重新通过。
- `cargo fmt --package coolzhu-web-console`
- `node --check modules/gui-web/packages/web-console/src/app.js`
- Context usage footer 测试通过，确认 snapshot/source/loaded/revision 均可见。
- `session_agent_events_jsonl_is_stable_and_typed` 通过，确认同一 history 重放稳定，assistant item 与 envelope 顶层均为 `ctx-events`。
- `session_agent_events_preserve_tool_call_and_result_contract` 通过，确认新增字段未破坏 reasoning、ToolCall、ToolResult 配对。
- `cargo build -p coolzhu-web-console --offline` 通过（既有 warning，无新增错误）。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：835 passed / 0 failed，日志见 `tmp/round8-web-tests.log`。

## 运行证据

使用隔离运行目录和 8801 端口重新加载 Memory / Context Preview，保留上一轮的聊天室历史选择证据，并保存本轮截图：

- `tmp/context-snapshot-api-evidence-round8.json`
- `tmp/context-snapshot-ui-evidence-round8.png`

本轮未触碰用户已安装的 8765 进程；隔离服务验证结束后停止并确认端口释放。

## 后续

若继续扩展，应让流式/非流式 API 的 turn ID 与该 snapshot 一起进入统一 turn projection；当前已先完成兼容的快照关联，不改变既有客户端字段和事件序列。
