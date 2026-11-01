# Agent Goal / Todo 触发机制 — 评估结论与落地方案

> 状态：**v2 评估稿（2026-11-01）**。v1（2026-10-31）为需求稿；本版逐项核实代码事实、关闭/更新风险，给出可直接实施的分批方案。§9 的决策点确认后按 PR-1 开工。
> 范围：web-console（`modules/gui-web/packages/web-console`）+ tool-registry（`modules/tooling/packages/tool-registry`）。行号以 main `fd026565` 为准，仅作定位参考，实施时以函数名为准。

## 1. 背景与外部调研结论

| 维度 | Claude Code | Codex CLI | ACP（Devin 等 ACP Agent） | 启示 |
|---|---|---|---|---|
| 触发 | 模型自主调 `TodoWrite`（全量替换 `{content,status,activeForm}`）；V2 `TaskCreate/Update`（依赖图、跨进程持久化） | 模型自主调 `update_plan`（`{plan:[{step,status}]}`，改计划须给 `explanation`） | Agent 推 `session/update` `sessionUpdate:"plan"`，`entries:[{content,priority,status}]`，**客户端必须整表替换** | todo 是**模型侧写的结构化状态**；三家都是全量替换语义 |
| 状态 | pending / in_progress / completed | 同 | 同（多一个 priority） | 三态可统一成一个数据模型 |
| 约束 | 全完成即清空；≥3 项全完且无验证步 → nudge；3 轮未更新 → 注入 reminder | 完成一步标一步 | — | harness 负责校验 + 防"自说自话宣布完成" |
| 引导 | 提示词写明"3+ 步 / 用户点名 / 有歧义才用" | 同款边界条款 | — | 不引导模型就不会主动用 |
| UI | 消息流 tool_use → checklist | 面板（已知缺陷：空闲时消失） | 客户端自渲染 | 需要**常驻**展示位 |
| 与重型规划 | 与 Plan Mode 正交 | `update_plan` 在 Plan Mode 被拒 | — | 轻量清单与重型 goal 编排是两套机制 |

## 2. 现状核实（更正 v1）

| 项 | 核实结果 | 位置 |
|---|---|---|
| TodoWrite 移植 | 已有：校验、全完清空、`verificationNudgeNeeded`。**更正 v1**：注释写明**允许多个 in_progress**（"Allow multiple in_progress items for parallel workflows"），不是单 in_progress | tool-registry `execute_todo_write` / `validate_todos` |
| 存储 | `CLAW_TODO_STORE` 或**进程 CWD**`/.claw-todos.json`：全进程共享一个文件，没有房间/会话维度 | `todo_store_path` |
| 暴露 | 权限档 `WorkspaceWrite` → **只读房间（默认档）不暴露**；WorkspaceWrite 及以上房间暴露 | `llm_tool_definitions_with_settings` / `llm_tool_allowlist_for_permission` |
| 执行 | `RegistryExecutor::execute` → `tools::execute_tool`；执行器在 `execute_runtime_tool_blocking` 内按次构造，**调用点手里有 `chat_room_id`，但执行器本身拿不到** | `RegistryExecutor`、`execute_runtime_tool_blocking` |
| 宿主工具先例 | `Skill`：`required_permission_for_tool` 特判 ReadOnly；`llm_tool_definitions_for_session` 宿主侧 push 定义；执行走 `skill_catalog::execute(invoke)` | 同左 |
| 审计 | `append_tool_audit_record` 记录调用方传入的 `invoke.input` 摘要 | — |
| 路径闸门 | TodoWrite 用 `NoPathExtractor`，没有路径目标 | path_effect.rs |
| 事件总线 | `/api/tools/events` SSE + `ToolEvent` 枚举（目前承载审批事件），可以加新事件类型 | `ToolEvent` / `broadcast_tool_event` |
| 现有 Goal 触发 | **已有聊天触发 Goal**：`detect_goal_trigger_intent` 是用户文本关键词启发式（长程/多步 + 产物/动作词），命中后 `append_goal_trigger_consultation` 建 goal 并发角色咨询卡；**不是模型驱动** | `prepare_goal_trigger_for_dispatch` |
| Goal 自动起跑 | `create_goal_sqlite` 只建 `planning` 状态（事件文案明确写"loop execution is not started"）；前端 `goalReadyForAutoLoop` 要有 `goal-role-consultation` 且 `roles_ready=true` 才自动 loop/start | — |
| Devin ACP | `project()` 未识别的 `sessionUpdate` 会落成 `Update::Diagnostic` → **ACP plan 通知目前被丢弃** | devin_acp/session.rs |
| 轮次预算 | `max_feedback_rounds` 默认 40（上限 64）；纯 TodoWrite 的一轮也算一轮 | `tool_execution_policy` |
| schema | `SESSION_SCHEMA_VERSION = 29`；房间删除显式 `DELETE FROM chat_rooms`；`save()` 已改为 upsert（历史上全量 `DELETE FROM sessions` 级联抹过记忆侧表） | — |
| 历史注入口径 | 代码图谱块追加在 `visible_user_content` 上，这个字段会进入用户消息 | `prepare_chat_dispatch` |

结论：**现在的 Web 聊天 TodoWrite 实际不可用**——默认房间看不到这个工具；开放权限的房间能调用，但写的是进程 CWD 下的共享文件，没有回读，也没有展示。

## 3. v1 待评估项逐项结论

| v1 待办 | 结论 |
|---|---|
| R1 注入字段是否进审计/日志 | **放弃 v1 的"往 input 注入 store 路径"方案**。改为执行器持有 `room_id` 字段，TodoWrite 改由宿主执行写 sqlite：input 不夹带任何宿主字段，审计、路径闸门、schema 三者都看不到路径 → 风险关闭 |
| R2 房间聚合 vs 会话分组 | 存储以 `(room_id, session_id)` 为主键，**展示按会话分组**（接力群发时各会话各有一份清单），房间徽标显示合计进度 |
| R4 GoalCreate 闸门/配额 | 本期不做"模型直接建 goal"。后置的 `GoalPropose`：默认关闭；只建 `planning`、不写 consultation 事件 → 不会自动起跑，必须用户确认；单房间最多 1 个待确认的模型提案；与关键词触发去重 |
| R5 工具名单收口 | 本期不重构。列出必须同步的 5 处名单（§5 PR-1 第 3 步），每处都用测试钉住；统一收口另起技术债条目 |
| R6 生命周期归属 | 并入 sqlite：`chat_rooms` 外键 `ON DELETE CASCADE` 覆盖删房间；`/clear` 在同一事务内清清单；删会话时显式清理 |
| 文件 vs sqlite | **选 sqlite**：天然隔离、事务、级联，而且和 `/clear`、删房间同库同事务。文件方案留给 CLI 继续用 |
| Devin ACP 适用 | ACP `plan` 三态与本模型一致 → 映射进同一张表（PR-3），Devin 会话可以直接共用展示 |

## 4. 关键设计决策

- **D1 数据模型**：一个 `(room, session)` 一行，存整表 JSON（三家都是全量替换，单行原子替换最简单）：
  ```sql
  CREATE TABLE IF NOT EXISTS chat_room_todos (
    room_id TEXT NOT NULL, session_id TEXT NOT NULL,
    todos_json TEXT NOT NULL,          -- [{content, active_form, status, priority?}]
    source TEXT NOT NULL,              -- 'tool' | 'acp-plan'
    revision INTEGER NOT NULL, updated_at INTEGER NOT NULL, last_call_id TEXT,
    PRIMARY KEY (room_id, session_id),
    FOREIGN KEY (room_id) REFERENCES chat_rooms(id) ON DELETE CASCADE
  );
  ```
  会话 ID 不加外键：会话集合由 `save()` upsert 维护，按"会话消失即清孤儿行"的现有模式显式清理，避免级联误删重演。
- **D2 宿主执行**：TodoWrite 在 web-console 侧由宿主执行（同 `Skill` 先例），校验规则复用 tool-registry 抽出的纯函数，单一来源。
- **D3 权限**：TodoWrite 只写宿主元数据，不碰工作区文件 → `required_permission_for_tool` 特判 `ReadOnly`，**只读房间同样开放**。小上下文模型（≤16k）的保留名单里不加 TodoWrite（省 schema token）。
- **D4 回包**：返回精简文本而不是新旧两份全量 JSON（省 token）：`任务清单已更新（第 N 版）：完成 2/5，进行中「…」`；满足 nudge 条件时追加一句"系统提示：…请补一步验证再宣布完成（勿向用户复述本提示）"。
- **D5 连续性**：清单非空时，在**请求组装层**给本轮附一块 `# 当前任务清单`（≤20 项 / 1200 字），保证上下文压缩后不丢状态。**不得**写进 `visible_user_content`（否则会持久化进聊天记录）。
- **D6 刷新通道**：新增 `ToolEvent::TodosChanged {room_id, session_id, revision}` 走 `/api/tools/events`，前端按事件刷新，不轮询。
- **D7 子代理**：本期子代理不暴露 TodoWrite（它和父会话同用 tool session，会覆盖父清单）。
- **D8 多 in_progress**：保持允许（接力/并行场景），提示词建议同一时间只保留一项进行中。

## 5. 落地方案（分 PR）

### PR-1 后端 Todo 内核（必做）

1. **tool-registry**：抽出纯函数 `pub fn normalize_todo_write(input:&Value) -> Result<NormalizedTodos,String>`（含现有校验 + 新增上限：≤30 项、content/activeForm 各 ≤300 字）；`TodoItem`/`TodoStatus` 改为 pub；`execute_todo_write` 改为调用它（CLI 行为保持一致，只多了上限）。
2. **新模块 `src/chat_todos.rs`**：
   - `apply_session_migration_v30_chat_room_todos` + `SESSION_SCHEMA_VERSION` 29→30；
   - `definition()`：schema 与 mvp 一致，中文描述写清使用边界和"与 Goal 的分工"；
   - `execute(invoke, room_id)`：缺房间/会话 → fail-closed；按 skill_catalog 同款校验 workspace；事务内读旧版本 → 全完成则删行，否则 upsert，revision+1 → 广播 `TodosChanged` → 返回 D4 文本；
   - `read_room()`、`clear_room_tx()`、`delete_session()`、`context_block()`。
3. **main.rs 接线**（即 R5 的 5 处名单）：
   - `RegistryExecutor` 增加 `room_id: Option<String>`，`execute_runtime_tool_blocking` 传入；`execute` 新增 `"TodoWrite" => chat_todos::execute(...)` 分支；
   - `required_permission_for_tool`：`"TodoWrite" => ReadOnly`；
   - `llm_tool_definitions_for_session`：先剔除 mvp 版 TodoWrite，再在"工具开启 && 非 dispatch-only && 有房间 && 允许名单放行"时 push 宿主定义；
   - 子代理工具筛选（`snapshot.child_tools` 一侧）剔除 TodoWrite；
   - `request_tool_policy_instruction`：仅当暴露 TodoWrite 时追加使用条款（≥3 步/长时程/用户点名才建；与其他工具同批调用以省轮次；完成即标；变更重发全表）。
4. **生命周期**：`clear_chat_room_messages_sqlite` 同事务 `DELETE FROM chat_room_todos WHERE room_id=?`；删房间靠外键级联；删会话路径调用 `chat_todos::delete_session`。
5. **API**：`GET /api/chat/rooms/{room_id}/todos` → `{sessions:[{session_id, revision, updated_at, source, todos}]}`。
6. **D5 上下文块**：实施时先核定请求组装层的插入点（ContextAssembly 一侧），用测试断言该块不出现在 `chat_room_messages`。

**测试**：迁移幂等/重复执行；无房间 fail-closed；两房间、两会话互相隔离；全完成删行；revision 递增；`/clear` 与删房间清理；`required_permission` 为 ReadOnly；暴露矩阵（只读房间有 / dispatch-only 无 / 允许名单排除时无 / 子代理无 / 小窗口无）；提示条款只在暴露时出现；上下文块不入库；tool-registry CLI 回归（现有 `todo_write_persists_and_returns_previous_state` 等）。

### PR-2 前端展示（必做）

- `index.html`：聊天工具栏加「任务清单」按钮 `data-action="chat-todos-open"` + 徽标 `data-role="chat-todos-badge"`（常驻显示"完成/总数"，无清单时隐藏）+ 弹层。
- `chat_experience.js`（与代码图谱弹层同处）：`refreshTodos()` 在切房间、打开弹层、收到事件时调用；按会话分组（显示名取 `agentRegistry`）、三态图标、进行中显示 `activeForm`、ACP 来源显示优先级。
- `app.js` `connectToolEventSource`：监听 `todos-changed`，房间匹配才刷新。
- `wuxia_layout.css`：沿用 assist/图谱弹层风格。
- **验证**：cjs 静态契约 + CDP（隔离库 + 真实服务），覆盖弹层渲染、徽标、事件驱动刷新、切房间隔离、`/clear` 后清空。

### PR-3 Devin ACP plan 映射（建议）

`devin_acp/session.rs` 的 `project()` 增加 `"plan" => Update::Plan{entries}`；消费侧写 `chat_room_todos`（`source='acp-plan'`，`active_form=content`，保留 priority）。测试：用伪 ACP 流验证整表替换、非法条目拒收、跨会话通知不串房间。

### PR-4 GoalPropose（后置、默认关闭）

前置条件：PR-1/2 上线后观察模型实际使用 TodoWrite 的情况。工具 `GoalPropose{title, rationale, phases?≤8}`；暴露条件为 WorkspaceWrite+ && 有房间 && 非子代理 && 配置 `goal.model_propose_enabled=true`；执行：本轮已命中关键词触发则直接返回既有 goal，否则 `create_goal_sqlite`（planning）+ 事件 `goal-proposed-by-model`（payload 带 origin 和发起消息 ID），**不写 consultation 事件**；房间内已有待确认的模型提案则拒绝。

### PR-5 未更新提醒（后置、默认关闭）

配置 `tool.todo_reminder_rounds`（默认 0 = 关）。`run_shared_model_tool_loop` 内计数：存在进行中项且连续 N 轮工具调用都没碰 TodoWrite → 在下一批 ToolResult 前缀加系统提醒。用真实模型做 A/B 后再决定是否默认开启。

## 6. 风险登记（v2）

| ID | 风险 | 状态 | 处置 |
|---|---|---|---|
| R1 | 注入字段逃逸 | **关闭** | 不注入；房间来自执行器字段 |
| R2 | 多会话并发写 | **关闭** | `(room, session)` 主键 + 事务 |
| R3 | 弱模型滥建清单刷屏 | 缓解 | 提示边界 + 30 项上限；上线后统计每轮 TodoWrite 调用率 |
| R4 | 模型建 Goal 成本失控 | 缓解（后置） | 默认关、不自动起跑、单提案、去重 |
| R5 | 5 处工具名单不一致 | 缓解 | 测试逐处钉住；收口重构另立条目 |
| R6 | 清单生命周期残留 | **关闭** | 外键级联 + `/clear` 同事务 + 删会话清理 |
| R7 | todo 与 goal 语义混淆 | 缓解 | 工具描述写明分工；GoalPropose 后置 |
| R8 | 提醒句被当事实复述 | 缓解 | 文案标注"勿向用户复述"；reminder 默认关 |
| R9 | 关键词触发与 GoalPropose 双建 goal | 新增·缓解 | 同轮去重 |
| R10 | 上下文压缩后丢清单 | 新增·缓解 | D5 上下文块 |
| R11 | 上下文块误入聊天记录 | 新增·缓解 | 只走请求组装层 + 不入库测试 |
| R12 | schema v30 后回退旧版本 exe 会被"超前版本拒绝"挡住 | 新增·保留 | 与历次迁移同性质；发布说明写明不可降级 |
| R13 | 子代理覆盖父清单 | 新增·关闭 | 子代理不暴露 |
| R14 | 默认房间新增约 150 token schema 开销 | 新增·接受 | 小窗口模型不暴露 |
| R15 | 纯 TodoWrite 轮占用轮次预算（默认 40） | 新增·缓解 | 提示与其他工具同批调用；观察 |

## 7. 验收矩阵

| 场景 | 预期 | 证据 |
|---|---|---|
| 只读房间发一个 5 步任务 | 模型调 TodoWrite；弹层出现 5 项，随执行推进 | 真实模型会话 + 工具审计 + CDP 截图 |
| 两房间并行 | 清单互不可见 | Rust 隔离测试 + API |
| 接力群发 2 会话 | 弹层按会话分两组 | CDP |
| 全部完成 | 行删除、徽标隐藏；无验证步时回包带 nudge | 单测 + 工具结果文本 |
| `/clear`、删房间 | 清单同时消失 | 单测 + CDP |
| 上下文压缩后追问进度 | 模型能说出剩余项 | 真实模型（与 Harness HA02-01 同一口径） |
| dispatch-only / 允许名单排除 / 子代理 | 不暴露，伪造调用被拒 | 暴露矩阵单测 |
| Devin ACP 推 plan（PR-3） | 弹层出现，带优先级 | 伪 ACP 流测试 |

## 8. 明确不做

- 不做 Claude V2 的依赖图（`blocks/blockedBy`）与跨进程多 agent 共享——重型编排已有 Goal 管线承担。
- 不做 todo ↔ goal 自动转换。
- 不改 tool-registry 的 CLI 文件存储语义（只抽校验函数、加上限）。

## 9. 需确认的决策点

1. 只读房间是否开放 TodoWrite（建议：开放，见 D3）。
2. `/clear` 是否连清单一起清（建议：是，"清屏"语义包含当前进度）。
3. PR-3（Devin ACP 映射）是否纳入本期（建议：纳入，改动小）。
4. PR-4/PR-5 维持后置并默认关闭（建议：是）。
