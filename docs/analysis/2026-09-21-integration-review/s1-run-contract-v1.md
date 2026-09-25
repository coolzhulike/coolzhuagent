# S1.1 运行与动作事实契约 v1

状态：实施中。公共类型已在 `coolzhu-core-runtime` 中定义；Web Console 的 Computer Use 步骤表已完成首段兼容迁移并写入部分投影。受控桌面笔画已可上报完整路径与释放回执；其他 helper、所有入口和 UI 展示尚未迁移，因此不得把本增量表述为“所有正式输入已采用”。

本轮源码标识：`run_contract.rs` SHA-256 为 `CBAF072C2D21AABDA42922D0F110902C8344090F7565AD1A7AA1777A2C987868`；工作区缺少可用 Git 提交基线，不能以本 hash 代替发布身份。

## 所属代码与验证

| feature_id | 代码 | 验证 | 当前状态 |
| --- | --- | --- | --- |
| S1-RUN-IDENTITY-V1 | `modules/core-runtime/packages/core-runtime/src/run_contract.rs` 的 `RunIdentity` | `identity_requires_all_scopes_and_attempt_identifiers` | 已定义：十项身份均不可为空 |
| S1-ACTION-RECEIPT-V1 | 同文件的 `ActionReceipt` | `receipt_preserves_known_partial_input_without_claiming_completion`、`receipt_rejects_impossible_input_combinations` | 已定义：动作事实维度与矛盾拒绝规则可回归 |
| S1-TERMINAL-BUDGET-V1 | 同文件的 `RunTerminalStatus`、`RunBudget` | `terminal_control_never_rejects_late_facts_and_budget_has_single_deadline` | 已定义：控制终态不复活，迟到事实可追加；预算以一个 deadline 判断 |
| S1-CU-RECEIPT-PROJECTION-V1 | `computer-use-core/src/contracts.rs`、`web-console/src/computer_use_store.rs`、`computer_use_executor.rs`、`computer_use_desktop_bridge.rs` | `migration_v12_adds_nullable_receipt_columns_without_rewriting_legacy_rows`、`step_trace_records_real_action_and_distinct_evidence_before_verified_progress`、`successful_stroke_keeps_input_fact_when_after_capture_fails` | 部分接入：SQLite v12 可空列、helper 结构化回执和受控笔画投影已实现 |
| S1-CU-INPUT-LEASE-V1 | `windows-process-guard/src/lib.rs`、`web-console/src/computer_use_executor.rs`、`web-console/src/main.rs` | `input_lease_blocks_competing_owner_and_fences_released_epoch`、`valid_desktop_and_browser_runs_succeed_only_after_visible_verification`、`desktop_run_yields_to_existing_input_owner_with_zero_input`、`dropping_running_controller_persists_cancelled_instead_of_running_forever`、`incidental_cursor_input_yields_to_existing_owner_and_recovers` | 部分接入：正式桌面 CU run 与视觉/工具链路的短时光标输入共用同一 broker，同一 Windows 登录会话内串行持有 owner epoch；busy 方以 `input_owner_busy` 零输入终止/拒绝；显式释放与持有者丢弃后同一会话均可立即重新获得 |
| S1-TOOL-PERMISSION-MINIMUM-V1 | `core-runtime/src/permissions.rs`、`permission_gate.rs`、`tool.rs`；`tool-registry/src/lib.rs`；`web-console/src/main.rs`；`command-line/src/main.rs` | `undeclared_tool_is_a_configuration_error_in_every_mode`、`declared_tools_keep_their_existing_decisions`、`unknown_dynamic_tools_fail_closed_in_every_permission_profile`、`required_permission_for_tool_matches_spec_defaults`、`run_model_tool_dispatch_unknown_tool_returns_failed_not_panic` | 已定义：新增 `PermissionMode::Unspecified`；未声明最低权限的工具在**所有档位**（含 `Allow`/`FullAccess`）都给出明确配置错误并 fail-closed，不再回退成 `ReadOnly`（Web）或 `DangerFullAccess`（CLI）；插件权限串非法改为注册期明确报错而非调用期 panic；已声明工具行为不变 |
| S1-CU-DEADLINE-CANCEL-V1 | `computer-use-core/src/controller.rs`、`supervisor.rs`、`input_stroke.rs`；`web-console/src/computer_use_executor.rs`、`computer_use_desktop_bridge.rs` | `cancellation_during_observation_is_terminal_without_input`、`cancellation_during_planning_is_terminal_without_input`、`cancellation_during_execution_is_terminal_cancelled`、`cancellation_during_verification_is_terminal_cancelled`、`deadline_expiry_before_first_action_performs_no_input`、`action_replan_and_deadline_budgets_are_hard_limits`、`failed_input_records_may_have_been_sent_with_unknown_release`、`pre_input_failure_codes_are_not_may_have_been_sent`、`emergency_release_is_needed_whenever_the_helper_did_not_confirm_release`、`stroke_failure_classification_covers_release_and_retryability` | 部分接入：`RunBudgetGuard` 已接在真实循环；四阶段取消与 deadline 到期已注入回归；失败路径按 §2.2 明确四维事实；helper 未确认释放时由本进程补发释放，补发失败按不可重试的释放未确认分类 |

## v1 身份

每项身份均是字符串稳定标识，`RunIdentity::validate` 拒绝空值：

`workspace_id / room_id / session_id / public_turn_id / run_id / step_id / request_attempt_id / tool_call_id / action_id / owner_epoch`

provider 的 request/trace ID 不在此集合中；它只能作为关联证据，不能替代公开回合或动作身份。

## 动作回执口径

| 维度 | 字段 | 值 |
| --- | --- | --- |
| 输入发送 | `input_delivery` | `not_sent` / `may_have_been_sent` / `sent` |
| 过程细节 | `partial`、`path_completed`、`confirmed_point_count` | 可空；`partial` 同时适用于路径与非路径动作 |
| 应用效果 | `effect` | `not_observed` / `effect_observed` / `no_effect_observed` / `inconclusive` |
| 目标验收 | `goal_verdict` | `not_checked` / `passed` / `failed` / `inconclusive` |
| 输入释放 | `input_release` | `not_needed` / `released` / `unknown` |

已拒绝的矛盾包括：`not_sent + partial=true`、`not_sent + path_completed=true`、`not_sent + confirmed_point_count>0`、`may_have_been_sent + path_completed=true`，以及没有 `path_completed` 却填写路径点数。`sent + partial=true + path_completed=false + confirmed_point_count=0` 合法，用于确认按下后、首个采样点前中止的路径。

## 兼容迁移边界

`computer-use-core::StepExecution` 现可携带可空的 `partial`、`path_completed`、`confirmed_point_count` 和 `input_release_status` helper 回执。Web Console 的 `computer_use_steps` 已以 SQLite v12 追加同义可空列：`input_delivery`、`partial`、`path_completed`、`confirmed_point_count`、`effect_status`、`goal_verdict`、`input_release_status`。既有行不会回填，全部保持 `NULL`；未升级 helper 的新结果也只在 `input_sent` 已知时写入 `sent/not_sent`。

已接通的现有执行路径在验收通过时写入 `effect_observed/passed`；验收未通过但没有明确“无效果”证据时写入 `inconclusive/failed`。受控桌面笔画成功后写入 `partial=false`、`path_completed=true`、已确认点数及 `released`；即使后截图失败，这些输入事实仍保留。其余路径细节与释放状态继续为 `NULL`，不从摘要文字猜测。

下一工作包必须：

- 将旧布尔值按证据来源映射为 `sent` 或保留未知，不能把缺失历史统一写成 `not_sent`；
- 新列保持可空并保留旧审计 JSON；
- 扩展取消、helper 失联和部分路径回执；输入不确定时须写入 `may_have_been_sent` 或保持未知，不能以错误结果替代事实；
- 从实际 helper 回执写入后再让 UI/统计使用 v1 字段；
- 终态 first-wins 仅限制控制状态，迟到输入、截图和 usage 仍以带来源的事实追加。

## 输入 owner lease 边界

Windows 正式桌面 CU run 在 controller 生命周期内向 `InteractiveInputLeaseBroker` 取得当前登录会话的 lease；竞争者得到 `input_owner_busy` 并在进入 adapter 前终止。每次实际输入前还检查 owner epoch 仍有效。该 broker 仅约束接入它的当前进程正式输入路径，不能阻止人手或任意外部程序，也尚未完成父进程死亡、helper 存活、跨进程协调、释放未知对账或接管策略；这些仍属于 S1.3/S1.5 后续验收。

本轮补齐的旁路：`main.rs` 中视觉/工具链路会**真实移动用户光标**的 `verify_cursor_on_target`（经 `move_cursor_to` → `SetCursorPos`，由 `POST /api/vision/locate/verify` 与 `POST /api/tools/execute` 触达）原先不经过 broker。现已改为先取得同一 login-session 的 lease：已被正式桌面 run 持有时拒绝移动光标、记 `[VERIFY] refuse to move cursor ...` 并返回未通过，不把"没有验证"宣称为验证成功；取得后随函数作用域释放。这样"同一 Windows 登录会话至多一个有效输入所有者"才覆盖到当前进程已发现的全部正式输入路径。

已验证：竞争 run 在进入 adapter 前终止、状态为 `Blocked` 且 `action_count == 0`（`desktop_run_yields_to_existing_input_owner_with_zero_input`）；显式 `release()` 与持有者丢弃后，同一登录会话都能立即重新获得 lease（前者在同一用例内断言，后者在 `dropping_running_controller_persists_cancelled_instead_of_running_forever` 内断言）；旁路路径在 owner 持有时拒绝、释放后可取得（`incidental_cursor_input_yields_to_existing_owner_and_recovers`）。`input_lease_lost` 是每次输入前的 epoch 复核分支：正常持有路径不会触发它，当前作为纵深防御保留，不冒充可复现的验收路径。

测试隔离要求：broker 是进程级单例，且按真实 Windows 登录会话解析 scope，因此**所有会取得输入所有权的测试必须共用同一把串行 guard**。该 guard 定义在 `crate::tests::desktop_input_lease_test_guard`（即 `main.rs` 的测试模块），桌面 CU 用例与旁路用例都从那里取锁。否则并行执行时，竞争用例会把彼此的持有误判为 `input_owner_busy`，并在 Supervisor 阶段提前终止。

## 工具最低权限解析边界（S1.4）

依据 §3.2「未知/动态工具缺少最低权限元数据时，返回明确配置错误，不默认 ReadOnly」，本轮把三个入口各自的"发明默认值"统一为 fail-closed：

| 入口 | 旧行为（已移除） | 现行为 |
| --- | --- | --- |
| Web（`required_permission_for_tool`） | 未知 → `ReadOnly`（**自动放行**） | 未知 → `Unspecified` → 闸门拒绝 |
| CLI（`required_permission_for_cli_tool`） | 未知 → `DangerFullAccess`（按最高权限放行） | 未知 → `Unspecified` → 闸门拒绝 |
| `PermissionPolicy::required_mode_for` | 未知 → `DangerFullAccess`（走审批提示） | 未知 → `Unspecified` → 明确配置错误 |
| 插件权限串（`permission_mode_from_plugin`） | 非法串 `panic!`（调用期崩溃） | 注册期明确报错；函数本身退化为 `Unspecified` 不再 panic |

拒绝先于 `profile == FullAccess` 判定，因此 **dev-open/debug 的 `FullAccess` 与显式 `Allow` 都不能放行缺元数据的工具**——要拒绝的不是"权限不够"，而是"没有权威元数据可判定"。已声明工具（内置与已注册插件）的判定结果保持不变。`runtime_tool_execute` 在拒绝时会把配置错误写进 summary，不再压成泛化的 "denied by permission gate"。闸门结果仍强制回写 `outcome.permission_gate = gate`，执行器无法伪造。

**本轮未覆盖（不得据此宣称 S1.4 完成）**：① 真值表只覆盖"已声明（内置/已注册插件）× 各 profile"，插件、MCP、调度任务、子 Agent 作为**独立注册来源**尚未接线，其来源身份与默认档位未逐项验收；② 「重排 hook 不能反转拒绝」尚未有专门回归——当前依赖"闸门结果强制回写"这一结构事实，未做故障注入；③ 「输入变更使审批失效」未实现；④ `PermissionMode` 新增 `Unspecified` 后，UI/审计对 `prompt`/`allow` 这类旧语义的展示迁移仍未完成。

## deadline / 取消 / 释放边界（S1.5）

已确认的现状（不是本轮新增，但此前没有回归）：`RunBudgetGuard` 已接在真实控制循环里——`before_action(fingerprint, now_ms)` 在**每次动作前**判定 deadline、`no_progress`、动作上限与重复签名；`record_replan()` 判重规划上限；`after_verification()` 累积无进展计数。因此"deadline 后无新业务动作"对**动作**路径成立（不是只在测试里成立）。

本轮补的回归：四个阶段的取消注入（Observation / Planning / Execution / Verification）断言 run 以 `Cancelled` 终态收束、且取消后不再产生新输入（前置阶段 `action_count == 0`，执行/校验阶段为 1，即不重试、不追加动作）；以及 deadline 在首个动作前到期时零输入、错误码为 `deadline_exceeded`。

**修正上一轮的一处不准确表述**：上一轮本节写"释放失败进入隔离尚未实现"。深查后确认**大部分已存在**，当时的判断过重：native helper 在 `input_stroke_native.cs` 的 `finally` 里捕获 `driver.Up()` 失败并抛 `mouse_release_failed`；`computer_use_desktop_bridge.rs` 已把它单独分类为 `mouse_release_failed`、`retryable=false`，因此控制器把它映射为终态 `Blocked`（不是 `Failed`），单次 run 内不再有动作；轮级上 `main.rs` 的 `computer_use_terminal_failure` 由"第一个 `route=computer-use-task-controller` 且 `is_error` 的派发"置位，同轮后续 UI 族调用会被 `recursive_call_blocked` 挡住。所以成功路径写 `released` 是**可信的**（helper 释放失败会抛错，走不到成功分支），不存在"由已发送推断已释放"。

本轮真正补的是**失败路径的四维事实缺失**：`TracingAdapter::act` 的 `Err` 分支此前只写 `status="failed"` 与 `error_code`，`input_delivery`/`input_release_status` 保持 NULL——违反 §2.2 的"最终必须明确三值之一"。现按 `failure_may_have_sent_input` 分类：只有执行器在进入 adapter **之前**就会拒绝的码（`input_lease_lost`/`input_lease_scope_unavailable`/`input_owner_busy`/`persistence_error`）记 `not_sent` + `not_needed`；其余（helper 中途失败、释放未确认等）记 `may_have_been_sent` + 释放 `unknown`，**不允许把"可能已发送"写成零输入或已释放**。回归：`failed_input_records_may_have_been_sent_with_unknown_release`、`pre_input_failure_codes_are_not_may_have_been_sent`。

**本轮已核实"无盲重放"大部分已实现**（不是缺失，而是**回归覆盖有洞**）：动作级防护是预算型的——`RunBudgetGuard::before_action` 先判 `no_progress`（`max_no_progress_steps`），再判**签名界** `max_same_signature`（同一 `ActionFingerprint`＝surface＋**观察代际**＋动作＋目标＋参数）；命中即 `stop()` 打开熔断。新观察 `visible_progress` 恒为 false（executor 对账时显式写死），所以"重复输入补截图"的循环**不会**因为拿到新截图而重置无进展计数，会累到 `no_progress` 停——这正是 §2.2「完整输入后截图失败，不通过重复输入来补截图」。

**发现的覆盖洞**：既有 `repeated_action_without_progress_is_blocked` 与端到端用例都**先命中 `no_progress` 分支**（因为可见进展为零），因此**签名界从未被独立验证过**——而它才是最紧的那条反重放规则（进展可见时 `no_progress` 被清零，只有签名界还在拦）。新增 `repeated_identical_action_against_the_same_observation_is_blocked_despite_progress`：每次 `after_verification(true)` 让无进展计数保持 0，命中签名界并断言报错文案是 `identical action signature`（而非无进展）、熔断已开，且**换一个观察代际后允许再试**（对应"重新观察后由恢复政策决定"）。

**一个需要后续注意的结构性事实**：§2.2「确认执行部分路径 | sent＋partial=true」这条恢复规则在生产路径上**目前是空转的**——唯一有结构化回执的 helper 是受控桌面笔画，它要么整段成功（写死 `partial=false, path_completed=true`）、要么返回 `Err`（不生成步骤事实），因此**没有任何 helper 会产出 `partial=true`**。所以这条规则今天没有产生者可供防护，后续模型在"实现"它之前应先确认是否有 helper 会报部分完成。

**本轮新增的 deadline 全链传播**：规范 §2.3 明确"一个单调 deadline 贯穿观察、规划、验收和输入，**子请求上限不超过剩余预算**"。核实发现：桥里的 `INPUT_TIMEOUT = 8s`、笔画 10s、截图 10s 都是**硬编码常量**，不看 run 的剩余 deadline——只剩 3 秒时一次点击仍可按 8 秒上限发起。已实现：

- `RunBudgetGuard::remaining_ms(now)` 暴露单调 deadline 的剩余量；
- `ComputerUseAdapter` 的 `observe`/`act`/`verify` 增加 `remaining: Duration` 参数，控制器在**每个阶段发起前**重新计算并传入；
- `DesktopBridge::execute` 同样接收 `remaining`，桥内用 `clamp_stage_timeout(remaining, cap) = min(remaining, cap)` 收紧：**7 处简单输入调用**与**笔画超时**都已接入。

回归：`stage_timeout_never_exceeds_the_remaining_budget`（充裕时仍用固定上限、不足时收紧到剩余、耗尽时保持 0）。

**本轮补齐（桌面侧）**：`DesktopBridge::snapshot`/`verify` 也接收 `remaining`；`capture_image(identity, remaining)` 以 `clamp_stage_timeout(remaining, CAPTURE_TIMEOUT)` 收紧——`execute` 内的**输入后截图**与 `snapshot` 内的**观察截图**都已按剩余预算封顶。至此桌面 surface 的**观察 / 执行 / 验收 / 输入**四个阶段及其截图子请求都受同一单调 deadline 约束。

**本轮补齐（浏览器侧）**：`BrowserBridge` 的 `snapshot`/`execute`/`verify` 也接收 `remaining`；桥内 `request()` 以 `clamp_stage_timeout(remaining, BRIDGE_TIMEOUT)` 收紧往返等待，并把超时文案里的固定 "10 seconds" 改成**实际**超时值（`within {} ms (remaining budget capped)`）——否则文案会在预算被收紧后说谎。共用钳制函数已从桌面桥模块提到 `computer_use_adapters::clamp_stage_timeout`（`pub(crate)`），两个桥共用一份。

至此**桌面与浏览器两个 surface** 的观察 / 执行 / 验收 / 输入四个阶段及其子请求（截图、桥往返）都受同一单调 deadline 约束。

**本轮未覆盖**：① helper **明确报过** `mouse_release_failed` 时不再由本进程补发一次独立释放（维持既有语义）——是否该补发需要产品判断；② `helper_lost` 的"追加迟到事实"只有分类与事实，没有迟到事实的专门回归；③ `BrowserNativeBridge::execute_tab_action`（非 trait 的辅助入口）仍用桥自身的 `BRIDGE_TIMEOUT`，因为它拿不到 run 的剩余预算（不在适配器调用链上）。

**更正一条我自己推断出来的"缺口"**：先前本文把"各阶段的**独立**超时预算（区别于共享总 deadline）"列为未覆盖项。核对 §1.5 的 WBS 原文后确认——原文是"**统一** deadline、取消、释放和收尾宽限；拆参数纠错、stale 刷新、replan 预算"，**并没有要求各阶段各自一套超时预算**；规范 §2.3 要求的正是"**一个**单调 deadline 贯穿观察、规划、验收和输入，子请求上限不超过剩余预算"，这一条现已实现。因此该项不是缺口，是过度推断。S1.5 的四项 WBS 要求现均已满足：统一 deadline ✔、四阶段取消 ✔、释放与收尾宽限（helper 自身 2s 宽限 + 未确认释放时本进程补发）✔、拆参数纠错/stale 刷新/replan 预算（各自独立且有回归）✔。

**本轮新增的失联分类**：非零退出**不等于**失联——native helper 报错时同样是 `throw` 后非零退出并把原因写 stderr。因此 `is_helper_lost(forced_kill, helper_exited_ok, helper_reported_failure)` 只在两种情形成立：被本进程强杀，或非零退出却**没有给出任何原因**（外部杀掉/崩溃）。这类情形单独报 `helper_lost`，与"helper 报告了操作失败"区分开；因为此时释放已由本进程补发成功（否则会是 `input_release_unconfirmed`），按 §2.2「确认执行者退出和释放后重新观察、由恢复政策决定」判为**可重试**，但事实仍记 `may_have_been_sent`（不能填零输入）。回归：`helper_lost_excludes_reported_failures`、`stroke_failure_classification_covers_release_and_retryability`。

**再修正一处不准确表述**：上一轮本节写"释放宽限（release grace）未实现，当前立即判定"，也是错的。`input_stroke.rs::run_helper` 早已实现宽限：取消或超时后先写取消哨兵文件，**再等 2 秒**让 helper 自行收尾与释放（`cancellation_at.elapsed() >= 2s`）才强杀。真正缺的是**强杀之外**的情形：`emergency_release()` 原先只在 `forced_kill` 时补发，而 helper **自己以非零状态退出**（崩溃/被外部杀掉，`finally` 可能没跑到）时既不补发释放、也只给泛化报错——左键可能一直按着。现已改为：只要 helper **没有自己确认过**释放结果（既没成功退出、也没明确报 `mouse_release_failed`），就由本进程独立补发一次释放；补发也失败则返回 `input_release_unconfirmed`，桥按**不可重试的释放未确认**分类（→ 终态 `Blocked`，轮级隔离）。回归：`emergency_release_is_needed_whenever_the_helper_did_not_confirm_release`、`stroke_failure_classification_covers_release_and_retryability`。
