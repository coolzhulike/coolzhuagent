**结论：方案合理。** 不改父运行的 `session_id`，只额外核对\"本轮实际模型是接纳时登记的发送目标\"，职责清楚，也没有冒造运行关系。本轮没有调用工具，只看了你贴的片段。

**有一个可能更直接的已有接口，建议先核对**
- `accept` 里已经按 `result.targets` 构造了 `parent.host_model_snapshots`，按 agent.id 存放。
- 父上下文是宿主在接纳时冻结的，不是调用参数。所以 `capture_mode` 可以直接要求 `parent.host_model_snapshots.contains_key(&scope.agent_id)`，不需要新增事件。
- 前提是 `HostModelSnapshot::capture` 对 Devin ACP 会话也能成功。它失败时会被 `filter_map` 丢掉，结果是保守拒绝，但报错原因会指错。
- 如果需要留落库审计，或者需要跨进程核验，再用 `chat.target_agents` 事件。

**必要的负向边界**
1. 目标事件要在 `accept` 里、任何执行开始前写入，每个 run 只能有一条。出现多条，或者事件属于别的 run，都拒绝。
2. 其它核对一律不放宽：
   - `validate_frozen_parent_relations` 仍然按主 session 核对；
   - `scope.agent_id` 必须在 sessions 表里存在，并且是本聊天室的成员。
3. 自动 handoff 后续跳转里的 agent 不在初始 targets 中，会被拒绝。这是保守失败，应写明，不要为它自动补登记。
4. CU 路径要单独核对：`run_model_tool_dispatch_for_session_with_identity` 带着 `policy.parent` 往下走，内部可能还有 \"session 必须等于 agent\" 的比较，非主目标调用 CU 必须实测。
5. 确认停止按钮按 turn 或 run 取消，而不是按 conversation session 取消。

**必要验收**
- Opus 聊天室真实发送一次只读审查任务，结果是 end_turn/drained。
- 伪造一个不在 targets 里的 agent，被拒绝。
- 原来 session 等于目标的调用照常通过。