# Agent Goal / Todo 触发机制 — 需求与风险方案（评估前稿）

> 状态：**需求稿，未经技术评审**。实施前需再做一轮评估与技术方案改进（风险点章节即评估入口）。
> 日期：2026-10-31 · 范围：web-console（`modules/gui-web/packages/web-console`）+ tool-registry + core-runtime

## 1. 背景与外部调研结论

主流 agent 的 goal/todo 机制调研结论（Codex CLI、Claude Code、Devin 生态）：

| 维度 | Claude Code | Codex CLI | 共性启示 |
|---|---|---|---|
| 触发 | 模型自主调 `TodoWrite`（全量替换 `{content,status,activeForm}`）/ V2 `TaskCreate/Update`（依赖图、持久化到 `~/.claude/tasks/`） | 模型自主调 `update_plan`（`{plan:[{step,status}]}` + 改计划必须给 `explanation`） | **todo 是模型侧写的结构化状态**，不是 UI 产物 |
| 约束 | 单 in_progress；全完成即清空列表；≥3 项全完无验证步 → verification nudge 注进 tool_result；3 轮未更新 → harness 注入 reminder | 完成一步标一步；不改计划不重复复述 | harness 负责**校验 + 防"自说自话宣布完成"** |
| 引导 | system prompt 写明"3+ 步 / 用户点名 / 有歧义才用" | prompt.md 同款边界条款 | 不引导 → 模型不会自发用 |
| UI | 消息流 tool_use → checklist；V2 文件态支持多 agent 共享 | `PlanUpdate` 事件 → TUI 面板（已知缺陷：空闲时消失，社区要求常驻） | 状态需**常驻展示位** + 生命周期刷新 |
| 正交性 | 与 Plan Mode 互不绑定 | `update_plan` 在 Plan Mode 显式拒绝 | 轻量 checklist 与重型 goal 管线是**两套机制，别混** |

## 2. 本仓现状盘点

已具备（复用面大）：

- **tool-registry 已移植 TodoWrite**：`execute_todo_write`（tool-registry/lib.rs:2042）含校验、单 in_progress、全完清空、`verificationNudgeNeeded` 计算；`TodoWrite` 已在 `mvp_tool_specs` 目录（lib.rs:444），权限档 `WorkspaceWrite`。
- **暴露链已通**：`llm_tool_definitions_with_settings`（main.rs:35290）按房间权限档过滤 mvp specs → WorkspaceWrite 档以上房间模型已能调到 TodoWrite。
- **工具执行环完整**：`model_tool_calls` 流式解析 → `run_shared_model_tool_loop` → `RegistryExecutor::execute` → `tools::execute_tool`。
- **重型 Goal 管线完整**：四角色（commander/planner/implementer/verifier）、phase 状态机、sqlite 持久化（`*_goal_*_sqlite`）、SSE `broadcast_goal_event`、loop 引擎、定时任务投递、`create_goal_sqlite`/`api_set_goal_plan` 可复用。
- **前端 Goal 展示已在**：任务卡（`task-card-progress`）、任务链窗口（`chat-task-chain`）、goal 列表 + 角色分配面板；SSE 订阅链 `syncGoalEventSources` 自动刷新。
- **会话级工具先例**：`chat_handoff` 不走 mvp specs，在 `llm_tool_definitions_with_settings` 手动 `defs.push()`（main.rs:35327）——新增 goal 工具可照搬。

## 3. 需求点（缺口清单）

| ID | 缺口 | 需求描述 | 验收要点 |
|---|---|---|---|
| G1 | todo 存储为进程 CWD 单文件，多会话/房间互相覆盖 | 按 `workspace + room + session` 分文件存储；路径由宿主注入，模型不可见不可伪造 | 两个房间并发写 todo 互不覆盖；模型 input_schema 不含路径字段 |
| G2 | 前端无 todo 展示位 | 聊天室工具栏加「任务清单」弹层，按会话分组渲染三态 | 弹层能看到模型写入的清单；in_progress 高亮；数据随工具执行刷新 |
| G3 | 系统提示无 todo 使用边界 | 提示词补 Claude/Codex 式使用条款 | 仅在 TodoWrite 暴露时注入；真实会话多步任务能自发立清单 |
| G4 | `verificationNudgeNeeded` 模型看不懂 | flag 为 true 时输出追加自然语言提醒 | tool_result 文本含可读提醒句 |
| G5 | 模型无法自立 goal | 新会话级工具 `GoalCreate`/`GoalStatus`，复用 create_goal_sqlite + set_plan + 事件广播 | 模型立 goal 后任务卡经 SSE 自动刷新；权限档低于 WorkspaceWrite 的房间不暴露 |
| G6 | 无"闷头干活不更新"防呆 | 可选：轮次内连续 N 轮工具调用未碰 TodoWrite → ToolResult 注入更新提醒 | 仅当存在 in_progress todo 时触发；可后置 |

## 4. 技术方案落点

### G1 存储作用域化

- `TodoWriteInput` 加 `#[serde(default)] store: Option<PathBuf>`（serde 默认不拒未知字段，模型 schema 不含此字段 → 宿主注入后模型无法感知/伪造）。
- 注入点：`RegistryExecutor::execute`（main.rs:3641-3645）`tool_name=="TodoWrite"` 分支，向 `invoke.input` 合入 `store`。路径 = `session_db_dir()/todos/{room_id}__{session_id}.json`（session_db_dir 即 `.coolzhu/` 旁路，天然 workspace 隔离）。
- `execute_todo_write` 改签名：`input.store` 优先 → `CLAW_TODO_STORE` env → CWD（保留后两者供 CLI/独立入口使用）。

### G2 API + 前端

- `GET /api/chat/rooms/{room_id}/todos`：扫 `todos/{room}__*.json` → `{sessions:[{session_id, todos:[...]}]}`；挂既有 chat-room 路由族，`session_store` 校验房间存在。
- 前端弹层复用代码图谱面板结构（`chat-tool-window` 模式）；刷新 = 打开拉取 + 打开期间 3s 轮询 + 渲染 TodoWrite 工具消息时触发一次。

### G3 提示词

`request_tool_policy_instruction`（main.rs:34145）加条件分支：definitions 含 `TodoWrite` → 追加使用边界条款（≥3 步/长时程/用户点名才建清单；单 in_progress；变更重发全量；完成即清空）。

### G4 nudge 可读化

`execute_todo_write` flag 为 true 时在输出 JSON 追加一行中文提醒（"连续关闭 3+ 项无验证步骤，建议补验证再宣布完成"）。

### G5 模型侧 goal 工具

- `goal_tool_definitions()`：`GoalCreate`（`{title, description, phases:[{title}]}`，phases ≤8）+ `GoalStatus`（查询）；`WorkspaceWrite` 档以上 + `name_allowed` 检查。
- 执行：`RegistryExecutor::execute` 的 Skill 同款特判分支 → `goal_tooling::execute(invoke)` → `create_goal_sqlite` + set_plan + `broadcast_goal_event`；goal 标 `origin:"model"`，总数封顶。
- `RegistryExecutor::from_registry()` allowlist 需加 Goal*（或特判绕过）。

### G6 更新提醒（可选/后置）

`run_model_tool_dispatch` 环内维护 `rounds_since_todo`：存在 in_progress todo 且连续 3 轮未碰 → ToolResult 前缀注 `<reminder>`。

## 5. 风险点（评估入口）

| ID | 风险 | 影响 | 缓解/待评估项 |
|---|---|---|---|
| R1 | **注入字段逃逸**：`store` 字段若被模型侧 schema/日志/审计链暴露，可被伪造指向任意路径 | 模型把 todo 写到仓库外敏感路径 | 注入必须在 schema 校验之后、execute 之前；`store` 路径强制限定在 session_db_dir 子树（前缀校验）；审计链是否会把注入后 input 记入日志需查 `append_tool_audit_record` |
| R2 | **多会话并发写同房间清单**：relay 群发时多个 session 同时 TodoWrite | 文件竞争、互相覆盖 | 按 session 分文件（本方案）天然规避；若改房间聚合视图需评估合并语义——**评估点：展示按 session 分组合不合用户预期** |
| R3 | **模型过度建清单**：弱模型对纯问答也立 todo，噪音/刷屏 | 弹层常驻无意义状态 | 提示词边界条款（G3）+ 观察期——**评估点：要不要加"单会话清单 TTL/房间切换时保留策略"** |
| R4 | **GoalCreate 滥用**：模型自立重型 goal 触发四角色编排，成本失控 | 真实 token 消耗飙升、角色会话被无意占用 | phases≤8、goal 总数封顶、origin 标记、WorkspaceWrite+ 门槛——**评估点：要不要二次确认闸门（RequireApproval）** |
| R5 | **暴露面回归**：`RegistryExecutor` allowlist / 小窗口裁剪白名单不含新工具 → 暴露了却执行被拒 | 模型困惑、错误噪声 | select_tools_for_request 的小窗口保留名单需同步加 Goal*——**评估点：统一收口工具名单定义，避免三处手工对齐** |
| R6 | **todo 文件生命周期**：房间删除/清空后文件残留 | 弹层展示幽灵清单 | `/clear`、删房间时联动清理 todos 目录——**评估点：生命周期归并到 CascadeDelete 链还是独立 GC** |
| R7 | **与 Goal 管线概念混淆**：模型可能用 GoalCreate 代替 todo，或反之 | 两套机制语义漂移 | 文档与工具 description 写清分工（todo=个人进度清单；goal=多角色编排目标）——Codex 也是靠描述边界防混用 |
| R8 | **nudge 提醒被当事实复述**：模型把提醒句当用户消息引用 | 输出污染 | 提醒文案注明"系统提醒，勿向用户复述"——**评估点：ToolResult 注入文本的既有污染防控策略** |

## 6. 实施批次（评估通过后）

| 批 | 内容 | 验收 |
|---|---|---|
| PR-1 | G1+G3+G4 | tool-registry 单测（注入路径、CWD 回退）、web-console 注入点单测、真实会话验证自发立清单 |
| PR-2 | G2 | CDP：弹层三态渲染、多会话分组、轮询刷新 |
| PR-3 | G5 | 会话内立 goal → SSE 自动刷新任务卡 → 编排正常跑；权限门槛矩阵 |
| G6 | 视 PR-1 后模型自觉性 | — |

## 7. 后续评估待办（下一轮）

- [ ] R1：审计链/日志是否会把注入后的 input 落盘（`append_tool_audit_record` 输入来源核实）
- [ ] R2：房间聚合 vs 会话分组的展示语义定稿
- [ ] R4：GoalCreate 是否需要 RequireApproval 闸门与配额模型
- [ ] R5：工具暴露名单是否统一收口（`RegistryExecutor::from_registry`、`select_tools_for_request` 小窗口名单、`llm_tool_definitions_with_settings`）
- [ ] R6：todo 文件生命周期归属（CascadeDelete vs 独立 GC）
- [ ] 备选架构评估：todo 存文件（现状移植）vs 存 sqlite 表（与 chat_room_messages 同库，天然级联、事务一致）
- [ ] Devin ACP 会话是否共享该能力（当前 Devin 限单目标，goal/todo 归属需单独定）
