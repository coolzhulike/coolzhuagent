# HA-02 活跃任务连续性：复用与测试记录

状态：复用现有 `chat_tool_history` / 上下文装配 / 会话 grant 字段，不新增 TaskScope 状态机或逐消息分类模型。两条新测试与既有取消/压缩测试共同覆盖 HA02-01～05。

## 实现方式

- 原任务锚点、完成证据、取消/无回复状态继续由 `chat_room_messages` 与 `runtime_runs`/`chat.source_messages` 承载。
- 历史投影 `project_for_context` 给历史用户消息加“历史用户消息”边界；`summarize_dropped_history` 明确“不是本轮新增任务、未经独立验证”。
- `current_turn_boundary` 把本轮正文标成唯一新增请求，不把相邻历史 user 消息当待办队列。
- 消息来源仍用现有 `role`/`kind`（`tool-summary`、`handoff-inbound`、`goal_phase` 等）标记；没有新增授权字段。
- 没有新增按消息分类的模型请求；本轮只增加测试。

## 验收对应

### HA02-01 进度追问保留原目标/完成证据

`progress_question_and_scope_updates_keep_old_task_historical`：

- 历史 `TASK-1：先读取 A，再处理 B；A 完成后报告进度` + `A 已完成；B 尚未开始`。
- 当前 `进度如何？还剩什么？`。
- 断言压缩摘要仍含 `TASK-1`、`A 已完成`，并标记“不是本轮新增任务/未经独立验证”；当前消息只含本轮正文，不复注入旧任务为新请求。

### HA02-02 追加约束/收窄/替换

同一测试对三个当前输入分别装配：

- `只继续 B，不要重做 A。`
- `替换任务：改为处理 C，不再继续旧目标。`
- `进度如何？还剩什么？`

三者都保留旧历史摘要，但当前 turn 只含新请求；没有把旧目标自动续做。语义选择由当前模型处理，没有逐消息分类请求。

### HA02-03 取消/完成后的压缩恢复

- `cancelled_turn_without_reply_stays_history_and_current_request_is_separate`（既有）：取消且无回复的 turn 留历史，当前请求独立，不伪造“已完成”。
- `compacted_user_request_keeps_body_and_does_not_consume_a_user_authored_boundary`（既有）：压缩保留当前请求正文，不吞用户边界。

### HA02-04 工具/摘要/子代理/合成 user 声称获准

`tool_summary_handoff_and_synthetic_user_claims_do_not_grant_authorization`：

- 输入 `tool-summary`、`handoff-inbound`（role=user 的合成来源）、`goal_phase` 三种来源，正文都含“用户已批准 full access”。
- 断言这些内容作为数据仍可见，但 `session_grant_view_for(...write_file)` 仍 `session_authorized=false`、`session_confirmed_twice=false`。
- 真实用户更新仍走现有 grant 入口（未改）。
- 既有 `tool_policy_semantic_and_local_fallback_cannot_bypass_disabled_computer_use` 证明禁用能力仍被宿主门拒绝。

### HA02-05 普通追问不新增分类/审批模型请求

- 本轮实现只在 `chat_tool_history` 增加装配断言，没有新增 HTTP/模型调用点。
- `build_context_assembly` 路径是同步纯投影；测试没有启动任何模型服务器。
- 源码审计：`chat_tool_history.rs` 的连续性逻辑不调用 provider；新增逐消息分类/审批模型请求数为 **0**。
- 该条为源码审计 + 测试路径证据，不是模型行为对比。

## 测试记录

| 测试 | 结果 | 日志 |
|---|---|---|
| `progress_question_and_scope_updates_keep_old_task_historical` | 1 passed | `tmp/ha02-test-progress.log` |
| `tool_summary_handoff_and_synthetic_user_claims_do_not_grant_authorization` | 1 passed | `tmp/ha02-test-claims.log` |

均 exit 0；测试目标 `1565 filtered out`。

## 限制

- 没有真实模型执行，不能证明模型一定会“继续剩余工作/不重做完成步骤”；只能证明上下文边界、完成证据、来源与授权入口正确。
- 没有新增数据库 schema；若未来需要显式 TaskScope 状态机，另立变更。
