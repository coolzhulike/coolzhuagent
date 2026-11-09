# 2026-11-09 Fusion Team P0 实现 work-log

## 背景

设计文档 `docs/plans/2026-11-09-fusion-team-design.md`（v3）定稿后进入 P0 实现。
三角色精简版：主持（强模型，规划/审核/验收/用户接口）、执行者（写代码/跑测试）、
查询者（只读检索，可与执行者同模型但上下文独立）。评审结论：无默认 Team，
设置界面启用+角色配置完整才出现在发送对象；问答不落 fusion_tasks；多方案按
默认规格询问；主持联网默认关闭、设置界面可开。

## 本批改动

### 后端（modules/gui-web/packages/web-console/src/fusion/）

- `store.rs`：迁移 v31（teams/members/tasks/work_items/briefs/role_messages/
  decisions/artifacts）+ 全部 CRUD；brief 支持 task_id 为空（问答路径）。
- `mod.rs`：FusionRole、FusionToolScope（task-local）、角色工具策略
  （role_keeps_builtin_tool / role_permission / role_tool_definitions）、
  team_* 派发入口。
- `team_tools.rs`：team_plan/team_dispatch/team_query/team_report/team_escalate/
  team_accept 工具定义与执行；派发矩阵由宿主校验，brief 不携带/放大授权。
- `runtime.rs`：admit/send/stream_events 入口，编排器 spawn_orchestrator，
  角色轮次复用 run_shared_model_tool_loop + build_context_assembly_with_roster；
  FrozenParentContext 不带 host 模型快照；复用 create/start_chat_runtime_run_sqlite
  + register_chat_turn_with_run + ChatTurnGuard + ChatTurnCancellation + root budget。
- 消息 kinds：fusion-reply / fusion-plan / fusion-brief / fusion-result /
  fusion-status（fusion-decision 决策卡片留 P1，主持澄清先用 fusion-reply）。

### main.rs 钩子

- `mod fusion;`；路由 /api/fusion/teams(+/{id})、/api/fusion/tasks、
  /api/fusion/tasks/{id}/cancel。
- api_chat_send / api_chat_send_stream 顶部判 `fusion:` 前缀分流；
  stream 侧用 futures_util::future::Either 合流（fusion 流发
  started/message/done 即收，编排器后台跑，角色消息经房间 SSE 增量到达）。
- llm_tool_definitions_for_session 加 fusion scope 分支（角色工具表，不走
  会话默认表与 MCP/插件追加）。
- run_model_tool_dispatch_within_root 加 `team_` 前缀派发分支。
- dispatch_model_tool_calls_parallel 捕获 FUSION_SCOPE 并在 JoinSet::spawn
  内重挂（spawn 不继承 task-local，同 request_key 模式）。
- apply_session_schema_steps 调 apply_session_migration_v31。

### 前端

- index.html：任务面板新增 Fusion Team 设置区（启用开关、团队名、
  主持/执行者/查询者会话下拉、主持联网开关），复用 goal-role 网格样式。
- app.js：fusionTeams 缓存 + sendableTargets()（agents ∪ 可发送 Team）；
  loadAgents/loadSessions/loadChatRooms 全部走 sendableTargets；
  renderAgentOptions 给 fusion 目标加 fusion-team-target 样式类；
  restoreAgentTargets/agentTargetSelectionError/updateAgentTriggerText
  处理 `fusion:<id>`；保存按钮 POST/PUT /api/fusion/teams 并联动刷新
  发送对象；TTS 屏蔽 fusion-brief/result/status/plan（fusion-reply 可朗读，
  与设计 §一致）。

## 验证

- `cargo build -p coolzhu-web-console --offline`：dev 全量通过（13m02s），
  产物 74.7MB。
- 新增 7 个测试全过：
  - store：team upsert/get/list/delete 回环；task+work_items+verdict；
    brief 生命周期 + 问答路径 task_id 为空的入队/claim/finish/sweep。
  - mod：fusion: 伪目标必须单选；主持联网开关（关=无 WebSearch/WebFetch，
    开=有；执行者/查询者不受开关影响）；角色工具策略屏蔽
    chat_handoff/computer_use/TodoWrite(非主持)/Sleep等(非执行者)；
    主持/查询者权限恒 ReadOnly、执行者沿用房间权限。
- `cargo test ... fusion`：7/7 通过。

## 进行中/风险

- ha04_tests::actual_agent_child_request_* 在默认测试线程栈下
  STATUS_STACK_OVERFLOW；RUST_MIN_STACK=4MB/64MB 通过（2.4s），2MB 仍爆。
  需确认是否本批改动把本来贴边的栈深度推过界（bisect 中），或既有环境性
  贴边。仓库先例（docs/analysis/2026-10-10-execution-coordinator-wiring.md）
  对同类问题用 Box::pin 削嵌套 future 栈占用而非提 RUST_MIN_STACK。
- fusion-decision 决策卡片、验收确定性门、重试上限细节、重启恢复为 P1 项。

## 真实端到端验证（dev 实例，AI_temp 工作区）

Team：主持=magpie(devin/swe-2 本地代理)，执行者=agnes(agnes-3.0-flash)，
查询者同执行者；`sendable=true, issues=[]`。

| 场景 | 结果 |
|---|---|
| 伪目标混选（fusion:+session） | HTTP 400 拒绝 ✓ |
| 问答路径（"hi"） | 主持直接 fusion-reply；fusion_tasks=0、fusion_briefs=0 ✓ |
| 任务路径（创建文件） | fusion-plan → host→executor brief(exec_task) → 执行者真实写入 fusion-e2e-probe.txt("ok") → fusion-result(done) → 主持验收 fusion-reply；task/work_item/brief 全 done ✓ |
| 流式端点 | started(turn_id+run_id) → fusion-status"已接管" → done(completed) 即收，编排器后台跑 ✓ |
| 连续消息 | "ping"→"pong"、"再说一次pong"→"ping 🏓"，事件队列按序处理 ✓ |
| 角色权限 | 执行者可写文件；validate_member_session 拒绝非 LlmHttp 后端（SWE-2 devin_acp 不可选）✓ |
| 消息可见性 | 全部 fusion-* 出现在 visible 分页 ✓ |

遗留：fusion_tasks.title 为空（admit 未传标题，P1 补）；决策卡片/验收门/重试上限/重启恢复属 P1。

## 后续批次：栈溢出修复 + 全量回归收敛（同日）

### HA04/HA05 栈溢出

- 复现：默认测试线程栈下 `actual_agent_child_request_inherits_parent_identity_and_capability_limit`
  与 `host_waits_once_and_cancel_or_deadline_stops_followup_without_new_budget`
  STATUS_STACK_OVERFLOW；4MB+ 栈可通过。
- 定性：既有深链 async future 嵌套贴边，本批 fusion 分支（每层派发多几个
  enum/字符串字段）把占用推过界；基线二分证实非本批语义回归。
- 修复：按仓库先例 Box::pin 把深链 future 堆化；两个测试改为
  `#[test]` 包装大栈线程跑 inner async（与 ha04 既有大栈测试同模式）。
  默认栈下 21 项 HA04/HA05/相关测试通过。

### 全量回归 4 个失败收敛

全量 `cargo test` 曾 1641 过 / 8 ignored / 4 失败，逐项处置：

1. `app_update::tests::local_get_and_check_failure_keep_the_json_http_contract`
   —— 单测独立通过；失败实为 `default_workspace_path` 内
   `workspace_projects::remember` 的 CAS 冲突 panic
   （"无法保存默认工程选择"）。测试态 `state_root()` 是全进程共享
   OnceLock TempDir，并行测试并发写 launcher-user.json：
   `select_workspace` 的 load 在锁外、`store_user_state` 的 CAS 在锁内，
   两个 remember 各拿 revision N → 一个成功 N+1 → 另一个
   SelectionConflict → panic。
   修复：`workspace_projects::remember` 对 SelectionConflict /
   SelectionLocked 做 3 次退避重试（"记录当前选择"语义本就是
   last-writer-wins；其余错误立即返回）。
   文件：src/workspace_projects.rs。

2. `tests::chat_recipient_selection_is_persisted_per_workspace_and_room`
   —— 静态前端契约断言过期：仍找
   `restoreAgentTargets(selectable, registry.active_agent_ids`。
   本批为 fusion 目标把签名改成
   `restoreAgentTargets(sendableTargets(), registry.active_agent_ids, ..., roomId)`。
   修复：断言更新为 `restoreAgentTargets(sendableTargets(), registry.active_agent_ids`。

3. `tests::run_model_tool_dispatch_sleep_respects_runtime_timeout`
   —— 测试预期过期：断言 default_timeout_ms=20 能让 200ms Sleep
   `runtime-timeout`，但 6ddde36c 起 Sleep 自带预算
   （duration_ms + WAIT_TOOL_BUDGET_SLACK_MS，本例 5.2s），
   不被默认超时拦腰杀——这是防"等待轮询烧反馈轮"的现行契约。
   修复：测试改名 `run_model_tool_dispatch_sleep_uses_own_wait_budget`，
   断言改为 `tool_timeout_ms_for("Sleep", {duration_ms:200}) == 200 + SLACK`
   且 dispatch 返回 runtime-executed/ok。

4. `tests::web_frontend_left_chat_rail_uses_workspace_navigation_and_search`
   —— 静态契约字符串过期（10-10 改动）：期望
   `const display = full || "未设置";`，现为
   `const display = workspaceDisplayPath(full) || "未设置";`。
   修复：断言更新为现行字面量。

### 前端可见性一致性复核（任务5收口）

- `shouldRenderCompletedMessage` 黑名单制：只滤 reasoning/tool-*，
  fusion-* 天然可见；与后端 VISIBLE_SQL（同为黑名单）一致。
- `isGoalPhaseMessage` 只认 goal-* 标记，不误伤 fusion-*。
- TTS blockedKinds 屏蔽 fusion-brief/result/status/plan；
  fusion-reply 与 fusion-decision 走 assistant role 可播报。
- 前端无独立系统通知通道，TTS 即"播报"全部——满足"仅回复/决策播报"。
