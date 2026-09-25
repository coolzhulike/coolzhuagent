# 2026-08-23 AgentEvent/JSONL 事件契约

## 本轮目标

补齐会话 history 到可重放 AgentEvent 的稳定投影，为 CLI、headless 测试和前端回放提供统一的 JSONL 契约，并在记忆知识窗口提供可见校验结果。

## 实现

- 新增 `GET /api/sessions/{session_id}/events?limit=&before=`，响应类型为 `application/x-ndjson`。
- 每行使用 `coolzhu.agent.event.v1` envelope，包含稳定 `event_id`、连续页内 `sequence`、`thread_id`、可选 `turn_id/item_id`、时间戳和结构化 `payload`。
- 当前投影事件包括 `thread.snapshot`、`turn.started`、`item.completed`、`turn.completed`；压缩记忆以 `item.completed` 的 `kind=compaction` 保留在回放中。
- 在 Thread history 标题栏增加“校验 JSONL”，前端逐行解析并检查 schema、事件 id、sequence 连续性，成功状态显示事件数和“JSONL 有序”。

## 本地验证

- `cargo fmt --package coolzhu-web-console -- --check`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- 定向测试 `session_agent_events_jsonl_is_stable_and_typed`：通过。
- 前端静态契约测试 `web_frontend_memory_window_connects_summary_prompt_context_and_governance`：通过。
- web-console 全量测试：`834 passed; 0 failed`。
- `cargo build -p coolzhu-web-console --offline`：通过（仅保留仓库既有 warning）。
- 隔离运行时（`COOLZHU_RUNTIME_DIR=tmp/memory-jobs-runtime-20260823`，端口 8801）：
  - `/api/sessions/session-fork-1787498733821/events?limit=6` 返回 200；
  - `Content-Type` 为 `application/x-ndjson; charset=utf-8`，12 行事件 sequence 从 0 连续到 11；
  - 所有事件 schema 为 `coolzhu.agent.event.v1`，thread id 与目标会话一致；
  - 记忆知识窗口点击“校验 JSONL”后显示 `12 events · coolzhu.agent.event.v1 · JSONL 有序`。

## 证据

- API 证据：`tmp/memory-agent-events-round4.json`。
- 浏览器实机截图：`tmp/memory-agent-events-ui-evidence.png`，显示 Thread history 的 12 events、schema 和有序校验状态。

## 备注

本轮仅使用独立临时 workspace 和 8801 端口，完成截图后已停止隔离进程；用户当前 8765 端口的已安装实例未修改。真实模型凭据未配置时未触发外部模型调用，事件读取与 UI 校验仍走真实持久化数据链路。
