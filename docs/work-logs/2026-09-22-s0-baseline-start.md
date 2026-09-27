# 2026-09-22 S0 基线与最小门禁启动

## 本次完成

- 新增 S0 证据基线，明确当前工作区没有可用于发行追踪的 Git commit 或匹配 MSI，真实模型、Paint 和安装验证未执行。
- 建立六类脱敏合成 fixture 的 JSON 契约及根工作区回放完整性测试；所有工具结果均只能对应已录制 call_id。
- 修正 Web Console 两处假成功：插件安装入口在未实现真实安装前返回 HTTP 501；流式诊断将静态源码声明标为 `declared`，不再标为 `ok`。
- 增加 Windows CI 的最小构建、Web 回归、fixture 与模块链接门禁。
- 新增只在测试编译的本地假模型服务，基于 fixture 回放 OpenAI 兼容的流式/非流式响应；主控制台两个真实模型入口已经在隔离环境接入回放，并断言未暴露、未执行工具。
- 启动 S1.1：在 `core-runtime` 定义运行身份、动作事实回执、终态、错误责任与 deadline 预算的公共 v1 契约；现有执行器和 SQLite 尚未迁移，兼容边界已单列说明。
- 推进 S1.2 首段：`computer_use_steps` 追加 SQLite v12 可空事实列，保留历史 `NULL`；现有执行路径只投影已确认的发送和验收事实，未推断路径、释放或无效果。
- 扩展 S1.2：`StepExecution` 支持结构化路径/释放回执；受控桌面笔画成功时将完整路径、确认点数和释放事实写入 SQLite，即使后截图失败也不丢失输入事实。无结构化回执的 helper 保持 `NULL`。
- 启动 S1.3：新增 Windows 登录会话范围、fencing epoch 的进程内输入 lease；正式桌面 CU run 持有 lease，竞争 run 以 `input_owner_busy` 零输入终止。当前不宣称覆盖跨进程 helper 或人手输入。
- 验证 S1.3 lease 边界：补齐 executor 层验收用例 `desktop_run_yields_to_existing_input_owner_with_zero_input`，断言竞争 run 为 `Blocked`/`input_owner_busy` 且 `action_count == 0`（真的零输入），并在显式释放后断言同一会话可再次执行成功；在 `dropping_running_controller_persists_cancelled_instead_of_running_forever` 内补充持有者被丢弃后 lease 必须立即可重新获得（Drop 释放而非泄漏）。
- 修复 S1.3 引入的测试并行争用：broker 是进程级单例并按真实 Windows 登录会话解析 scope，导致 4 个桌面用例在并行执行时把彼此的持有误判为 `input_owner_busy` 并在 Supervisor 阶段提前终止（串行执行 16/16 通过、并行 12/16）。新增串行 guard 并接入全部 9 个驱动桌面 surface 的用例，恢复并行稳定。
- 修复 S1.3 的输入通道旁路：`main.rs` 的 `verify_cursor_on_target`（经 `move_cursor_to` → `SetCursorPos`）会**真实移动用户光标**，由 `POST /api/vision/locate/verify` 与 `POST /api/tools/execute` 触达，原先完全不经过 broker——正式桌面 run 持有 lease 时它仍会插入输入，违反"至多一个有效输入所有者"。现改为先取得同一 login-session 的 lease：被持有则拒绝移动光标、记 `[VERIFY] refuse to move cursor ...` 并返回未通过（不把"没有验证"宣称为验证成功），取得后随作用域释放。新增 `incidental_cursor_input_yields_to_existing_owner_and_recovers` 覆盖拒绝与释放后恢复。
- 测试锁收敛为单一来源：guard 从 `computer_use_executor::tests` 移到 `crate::tests::desktop_input_lease_test_guard`（`main.rs` 测试模块，`pub(crate)`），桌面 CU 用例与旁路用例共用同一把锁，避免跨模块各持一把锁而继续争用。
- 本轮机禁：Web 988/988、core-runtime 185/185、S0 fixture 2/2、模块链接 4/4、windows-process-guard 1/1 全部通过。
- 契约文档 `docs/analysis/2026-09-21-integration-review/s1-run-contract-v1.md` 的 `S1-CU-INPUT-LEASE-V1` 行与「输入 owner lease 边界」节同步更新：登记新增用例、记录旁路修复、测试隔离要求，并明确 `input_lease_lost` 属纵深防御、不作可复现验收路径。
- 实施 S1.4（PR-08）工具最低权限解析：新增 `PermissionMode::Unspecified`，把三个入口各自"发明默认值"的行为统一为 fail-closed —— Web 未知工具由 `ReadOnly`（自动放行）改为拒绝；CLI 由 `DangerFullAccess` 改为拒绝；`PermissionPolicy::required_mode_for` 由 `DangerFullAccess` 改为明确配置错误。拒绝判定放在 `profile == FullAccess` 之前，因此 dev-open/debug 的 FullAccess 与显式 Allow 都不能放行缺元数据的工具。已声明工具判定结果不变（回滚要求：保留已知兼容行为）。
- 修正插件权限串的失败方式：`permission_mode_from_plugin` 原为非法串 `panic!`（首个调用即崩溃进程），现注册期在 `with_plugin_tools` 明确报错，函数本身退化为 `Unspecified`；新增 `is_supported_plugin_permission` 供注册校验。
- 拒绝原因不再被压成泛化文案：`runtime_tool_execute` 在 `required.is_unspecified()` 时把"缺少最低权限元数据"写入 summary，回灌模型与审计都能看到真实原因。
- S1.4 连带更新的既有测试（均为旧行为编码，已按新契约改写并保留原意图）：`conversation.rs` 4 个用例改为显式声明所用工具（`add`/`blocked`）；`run_model_tool_dispatch_unknown_tool_returns_failed_not_panic` 断言由 `failed` 改为 `rejected` 且要求 notes 指向缺失元数据。新增 `undeclared_tool_is_a_configuration_error_in_every_mode`（覆盖含 Allow 的全档位）、`declared_tools_keep_their_existing_decisions`、`unknown_dynamic_tools_fail_closed_in_every_permission_profile`（覆盖三个 profile，含 FullAccess）。
- 本轮机禁（S1.4 阶段）：Web 989/989、core-runtime 187/187、tool-registry 43/43、plugin-system 28/28、command-line 81/81、S0 fixture 2/2、模块链接 4/4、windows-process-guard 1/1 全部通过。
- 推进 S1.5（PR-09）deadline/取消：先确认现状——`RunBudgetGuard` **已经**接在真实控制循环（`before_action` 每次动作前判 deadline/no_progress/动作上限/重复签名，`record_replan` 判重规划上限，`after_verification` 累积无进展），因此"deadline 后无新业务动作"对动作路径成立，此前缺的是回归而非实现。
- 补 S1.5 阶段级故障注入（`computer-use-core/src/controller.rs`）：Observation / Planning / Execution / Verification 四个阶段各注入 `cancelled`，断言终态为 `Cancelled` 且取消后不再产生新输入（前置阶段 `action_count == 0`；执行/校验阶段为 1，即不重试、不追加动作）；另注入 deadline 在首个动作前到期，断言零输入且错误码为 `deadline_exceeded`。写这些用例时发现控制循环在规划前还有一次**前置校验 verify**（判断目标是否已达成），因此 `FakeAdapter` 的 verifications 队列必须先给一个"未达成"结果——这一点已写进用例注释，避免后续误判。
- 本轮机禁（S1.5 阶段，全量）：Web 998/998、core-runtime 187/187、computer-use-core 47/47、tool-registry 43/43、plugin-system 28/28、command-line 81/81、llm-adapter 119/119、vision-service 36/36、windows-process-guard 1/1、S0 fixture 2/2、模块链接 4/4 全部通过。
- 收口 S1.5 的释放事实：**修正上一轮的一处过重判断**——上轮写"释放失败进入隔离尚未实现"，深查后确认大部分已存在（native helper 在 `finally` 捕获 `driver.Up()` 失败抛 `mouse_release_failed`；桥已单独分类且 `retryable=false` → 控制器映射为 `Blocked` 终态；轮级 `computer_use_terminal_failure` 会挡住同轮后续 UI 族调用）。真正缺的是**失败路径的四维事实**：`TracingAdapter::act` 的 `Err` 分支此前只写 `status`/`error_code`，`input_delivery`/`input_release_status` 留 NULL，违反 §2.2"最终必须明确三值之一"。现按 `failure_may_have_sent_input` 分类写入：输入前就失败的码记 `not_sent`+`not_needed`，其余记 `may_have_been_sent`+释放 `unknown`，不得把"可能已发送"写成零输入或已释放。新增 `failed_input_records_may_have_been_sent_with_unknown_release`、`pre_input_failure_codes_are_not_may_have_been_sent`。
- 本轮机禁（S1.5 收口，全量）：Web 991/991、core-runtime 187/187、computer-use-core 47/47、tool-registry 43/43、plugin-system 28/28、command-line 81/81、llm-adapter 119/119、vision-service 36/36、windows-process-guard 1/1、S0 fixture 2/2、模块链接 4/4 全部通过。
- 修 S1.5 的释放恢复缺口（并**再次修正上一轮的过重判断**：上轮写"释放宽限未实现、当前立即判定"也是错的——`run_helper` 早已在取消/超时后先写取消哨兵、**再等 2 秒**让 helper 自行收尾释放才强杀）。真正的缺口是**强杀之外**的路径：`emergency_release()` 原先只在 `forced_kill` 时补发，helper **自己以非零状态退出**（崩溃/被外部杀掉，`finally` 可能没跑到）时既不补发释放、也只给泛化报错，左键可能一直按着。现改为只要 helper 没**自己确认过**释放结果（既非正常退出、也未明确报 `mouse_release_failed`）就由本进程独立补发一次释放；补发仍失败则返回 `input_release_unconfirmed`，桥把它归入**不可重试**的释放未确认（→ 终态 `Blocked`，轮级隔离）。抽出可测纯函数 `needs_emergency_release` 与 `classify_stroke_failure`，回归 2 条。
- 本轮明确**不改**并留给产品判断的一点：helper 已明确报 `mouse_release_failed` 时是否再由本进程补发一次独立释放（维持既有语义，未改）。
- 本轮机禁（S1.5 释放恢复，全量逐套件确认）：Web 1001/1001、core-runtime 187/187、computer-use-core 48/48、tool-registry 43/43、plugin-system 28/28、command-line 81/81、llm-adapter 119/119、vision-service 36/36、windows-process-guard 1/1、S0 fixture 2/2、模块链接 4/4 全部通过。
- 门禁聚合方法修正：批处理里用 `grep -c` 统计时，某套件若**没有产出任何 `test result` 行**（未真的跑到）会被算成 "passed=0 failed=0"，看起来像通过。已改为先判定是否产出结果行，未产出则显式报 `NO-RESULT`（本次复跑发现 tool-registry 在批处理中曾出现该情形，单跑确认为 43/43）。后续门禁统计都必须保留这一步，避免假绿。
- 补 S1.5 的"执行者失联"独立分类：先纠正一个易错前提——**非零退出不等于失联**，native helper 报错时同样是 `throw` 后非零退出并把原因写 stderr（那属于"报告了失败"，应保留 stale/取消/泛化失败的原分类）。新增 `is_helper_lost(forced_kill, helper_exited_ok, helper_reported_failure)`，只在"被本进程强杀"或"非零退出且未给出任何原因"时成立，单独报 `helper_lost`。因为此时释放已由本进程补发成功（否则会是 `input_release_unconfirmed`），按 §2.2「确认执行者退出和释放后重新观察、由恢复政策决定」判为可重试，但事实仍记 `may_have_been_sent`。回归 `helper_lost_excludes_reported_failures` 覆盖五种组合（含"报错退出不算失联"的反例）。
- 本轮机禁（S1.5 失联分类，逐套件带 NO-RESULT 判定）：Web 992/992、core-runtime 187/187、computer-use-core 49/49、tool-registry 43/43、plugin-system 28/28、command-line 81/81、llm-adapter 119/119、vision-service 36/36、windows-process-guard 1/1、S0 fixture 2/2、模块链接 4/4 全部通过。
- 核实并补测 S1.5 的"无盲重放"：结论是**大部分已实现、但回归覆盖有洞**。动作级防护是预算型的（`before_action` 先判 `no_progress`、再判签名界 `max_same_signature`＝surface＋观察代际＋动作＋目标＋参数，命中即开熔断）；新观察恒记 `visible_progress=false`，所以"重复输入补截图"不会因新截图重置无进展计数——§2.2 那条要求成立。但既有两个用例都**先命中 `no_progress`**，因此**签名界从未被独立验证**（而它才是在进展可见时仍在拦的那条）。新增 `repeated_identical_action_against_the_same_observation_is_blocked_despite_progress`：让无进展计数保持 0，断言命中签名界（文案为 `identical action signature`）、熔断已开、且换观察代际后允许再试。
- 登记一个结构性事实供后续注意：§2.2「sent＋partial=true」的部分路径恢复规则在生产上**目前空转**——唯一有结构化回执的 helper（受控桌面笔画）要么整段成功（写死 `partial=false/path_completed=true`）、要么返回 `Err`，**没有任何 helper 会产出 `partial=true`**。后续"实现"它之前应先确认是否存在会报部分完成的 helper。
- 本轮机禁（S1.5 无盲重放核实，逐套件带 NO-RESULT 判定）：computer-use-core 50/50，其余同上全部通过。
- 实现 S1.5 的 **deadline 全链传播**（§2.3「子请求上限不超过剩余预算」）：核实确认桥里的 `INPUT_TIMEOUT = 8s`、笔画 10s、截图 10s 全是**硬编码常量**，与 run 剩余 deadline 无关——只剩 3 秒时一次点击仍按 8 秒上限发起。改动：`RunBudgetGuard::remaining_ms(now)` 暴露剩余量；`ComputerUseAdapter` 的 `observe/act/verify` 三个方法加 `remaining: Duration`（控制器在每个阶段发起前重算并传入，共 6 处）；`DesktopBridge::execute` 同样接收 `remaining`，桥内以 `clamp_stage_timeout(remaining, cap) = min(remaining, cap)` 收紧 **7 处简单输入**与**笔画超时**。回归 `stage_timeout_never_exceeds_the_remaining_budget`。
- 过程说明（可复用的经验）：这次是跨 3 个文件约 40 处改动的机械签名传播，我按"先整体改签名与调用点（新参数先被忽略）、再让实现真正使用"两片推进，每片都编译验证；中途踩到两个坑——**多行签名的缩进假设**（正则要求 8 空格，实际 12 空格）和**同形调用点归属判断错**（`self.bridge.execute(...)` 在浏览器适配器与桌面适配器里文本相同，我按出现顺序误判了归属，靠编译器给出的行号纠正）。后续同类改动应一律按行号定位、不要按出现顺序推断。
- 本轮机禁（S1.5 deadline 传播，逐套件带 NO-RESULT 判定）：Web 993/993、computer-use-core 50/50，其余同上全部通过。
- 闭环 S0.1 的"版本—产物哈希—源码提交"对应（`docs/analysis/2026-09-21-integration-review/s0-baseline.md`）：采集四个 MSI 的 SHA-256；用 `msiexec /a` 提取 0.2.0 包内 `coolzhu-cli.exe` 读内嵌 `GIT_SHA`；从已安装的 0.2.14 读到 `e314500a34df0b39918371feb00fb65e7855874c`——**该提交正是本轮同步的 9 个上游提交之一**，于是"已安装构建出自哪份源码"现在可回答（0.2.14 ↔ 上游 `e314500a`）。同时记录本轮源码快照的 18 个文件哈希（并标注它**尚未打包成 MSI**，故无对应产物身份），以及四项**未获得**的证据：0.2.12/0.2.13 的源码提交、全部签名链（四个 MSI 均 `signed=false`）、上游完整历史与 CI 结果、真实桌面/真实模型验收。S0-BASELINE-IDENTITY 由"进行中：缺 Git/安装包身份"改为"部分完成"并列出剩余缺项。
- 更正一条我自己推断的缺口：先前把"各阶段**独立**超时预算"列为未覆盖。核对 §1.5 WBS 原文后确认**无此要求**（原文是"**统一** deadline"），规范 §2.3 要的正是"一个单调 deadline + 子请求上限不超过剩余预算"，已实现。**S1.5 四项 WBS 要求现均满足**（统一 deadline、四阶段取消、释放与收尾宽限、拆参数纠错/stale 刷新/replan 预算各自独立且有回归）。这已是本会话第 N 次"grep/推断式缺口"被读原文推翻，后续一律先读 WBS/规范原文再判定缺口。
- **开始 S2**，并交付第一片（S2.2 的"未知可表达"）：先写 `s2-entry-plan.md`（五个工作包的起点认定、第一片切分、需产品判断的三点）。核实现状时**又发现自己在计划里写错了一处**——我把 S2.2 判成"事实层从零起"，实际 `core-runtime/src/usage.rs` **已有** `TokenUsage`/`UsageTracker`/`ModelPricing`/成本估算与会话重建；已更正该行。真实缺口是更精确的一条：`TokenUsage` 是纯 `u32` + `Default`，**"缺 usage"与"usage 为 0"不可区分**，而 §2.4 要求"缺 usage 为 unknown，不是 0"——与 S1.4 修 `PermissionMode` 是同一类"契约无法表达未知"。按同一先例实现：新增 `ReportedUsage`（各维度 `Option`）+ `UsageTracker::{record_reported, unknown_usage_turns, usage_is_complete}`，语义是"已知维度照常求和，但只要有任一维度未知就把汇总标记为不完整"，消费方不得把不完整汇总当完整账单。回归 4 条：缺 usage 记未知而非 0、完整上报不标不完整、部分维度未知也标不完整、迟到未知轮不抹掉已记录的已知量。
- **一条关于门禁自身的重要观察**：本轮批处理中 `coolzhu-tool-registry` 报 `NO-RESULT (exit=127)`——`NO-RESULT` 判定（前一轮加的）**正确阻止了一次假绿**。查日志发现不是断言失败，而是测试二进制**崩溃**：`exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN`。随后独立重跑 2 次均 **43/43 通过、exit=0**，说明是在 9 套件连续执行、大量子进程churn 下的**偶发崩溃**，非稳定失败、也不由本轮改动引起（本轮未触碰该 crate 的代码）。已如实登记；后续若再现，应先用 `--test-threads=1` 与逐用例定位是哪个用例触发 abort。
- 交付 S2.2 第二片（**低风险、避开 `main.rs`**）：在 `core-runtime/src/usage.rs` 新增 `UsageLedger` / `UsageAttempt` / `UsageAttemptOutcome`，实现 §2.4 的"逻辑请求与网络重试**分账**、失败/超时**也保留**登记、供应商与估算 token **分列**、**无价格版本或版本混杂时拒绝产出账单数字**"。回归 5 条（重试分账、失败/超时保留、未知不污染汇总、无价格拒绝出数、版本混杂拒绝出数），core-runtime 由 191 增至 **196** 用例。**未接线**，因此不得表述为"用量分账已上线"。
- 交付 §2.1「显式恢复/新尝试」身份契约（低风险 additive、未接线）：新增 `core-runtime/src/recovery.rs`——`RecoveryAttempt::derive(parent_attempt_id, parent_run_id, attempt_id, remaining_budget, parent_budget)` 与 `RecoveryError`。它把 §2.1 的两条禁令做成可回归规则：① **必须换身份**（沿用父 attempt_id 会命中 `TurnComputerUseSupervisor` 的 `terminal_cache`、永远拿到旧的失败终态，故拒绝 `recovery_attempt_id_reused`）；② **不得放宽预算**（deadline 或任一项上限比父更宽即拒绝 `recovery_budget_widened`，只允许收紧或完全结转）。回归 5 条，core-runtime 由 201 增至 **206**。
  - **继续修 llm-adapter 的测试隔离（本轮第二批）**：全量门禁中 llm-adapter 出现 2 个失败，且是**级联**——`env lock: PoisonError` 之后紧接 `MissingCredentials`。定位到两个独立缺陷：**A. 同一个 crate 里有三把独立的 env 锁**（`client.rs` / `openai_compat.rs` / `claw_provider.rs` 各定义自己的 `static LOCK`），所以**跨模块**的 `set_var`/`remove_var` 仍能并发执行（锁只在模块内串行）——这正是 `MissingCredentials` 的来源；**B. 毒化处理不一致**（前两处 `.expect("env lock")`，任一线程持锁 panic 就把同模块后续用例级联成 `PoisonError`；`claw_provider.rs` 已是容错的）。修法：在 `lib.rs` 新增 crate 级 `process_env_lock()`（一把锁 + 容错毒化，与 `tool-registry` 的 `process_state_lock` 同一约定），三处 `env_lock()` 全部委托它；顺手清掉因此变得未使用的 `use std::sync::{Mutex, OnceLock}`（警告 3→0）。llm-adapter 连续通过，随后全量门禁全绿。
  - 这两条与上一批的"非阻塞 socket 继承"是**同一个测试基础设施问题族**：都是**进程级共享状态没有全 crate 共用一把锁**。tool-registry 早已做对（一把 `process_state_lock`），llm-adapter 是漏的。
- 交付 §2.1「迟到事实」契约（低风险 additive、未接线）：新增 `core-runtime/src/late_facts.rs`——`LateFact`（带 `source` 与 `received_at_unix_ms`，可标 `observed_at_unix_ms`）、`LateFactKind`（Input/Capture/Usage/Verdict）、`LateFactAppend`（`revives_run()` 恒 false）、`append_late_fact`。回归 5 条：**五个控制终态下追加一律不改变控制状态**、输入与计费类事实都接受、缺来源或接收时间拒绝、缺 run_id 拒绝、迟到由观察/接收时间推出（发生时刻未知时保守记为迟到）。core-runtime 由 212 增至 **217**。
  - 核实说明：S1.1 起已有 `RunTerminalStatus::accepts_late_facts()` 这个**判定**（恒 true，且有回归）；缺的是"追加"本身的承载物与"控制状态不变"的强约束——所以补的是后一半，不是重复 S1.1。
- **修掉一个真实的测试缺陷（低风险、根因修复而非加 retry）**：全量门禁中 `coolzhu-llm-adapter` 出现 1 次失败，报 `读取HTTP请求: Os { code: 10035, kind: WouldBlock }`。定位到根因：测试自建 mock server 用 `set_nonblocking(true)` 的 listener，而**Windows 上 accept 出来的 socket 会继承非阻塞模式**；代码只设了 `set_read_timeout` 却没恢复阻塞，于是数据未到就 `read` 返回 `WouldBlock` → `.expect("读取HTTP请求")` panic。**同一缺陷也在 `tool-registry` 的 `TestServer`**（`stream.read(...).expect("read request")`）。两处各加一行 `set_nonblocking(false)` 恢复阻塞读后，llm-adapter 与 tool-registry 各连续 2 次全绿。
  - 这条同时解释了我此前登记为"原因不明"的那次 `tool-registry` 异常（`STATUS_STACK_BUFFER_OVERRUN`）：同族竞态（子线程 panic / 进程异常终止），当时没能稳定复现。现已从根因上消除，而**不是**用重试掩盖。
  - 教训记录：先前把这类现象归为"负载下偶发"是偷懒——两者的报错都指向同一个可解释的机制（非阻塞模式继承），读一眼实现就能定位。
- 交付 §2.1「动作重传」身份契约（低风险 additive、未接线）：新增 `core-runtime/src/action_injection.rs`——`ActionInjectionRegistry::{begin_injection, resolve, awaiting_reconciliation}` 与 `InjectionDecision`（`Inject` / `AlreadyInjected{outcome_ref}` / `ReconcileFirst`）。同 `action_id` 永不二次注入；结果未知时后续传输走"先对账、不自动重放"；`resolve` 对未登记的 ID 返回 false 而非静默成功。回归 6 条，core-runtime 由 206 增至 **212**。
  - 核实说明：`action_id` 此前**没有**被当作去重键（全仓 grep 无此用法），现有防护是 `ActionFingerprint` + `max_same_signature` 的**内容指纹**，同 ID 但参数或观察代际略变即指纹不同、仍会二次注入——所以这是真缺口，不是重复劳动。
- 同批核实：§2.1 的另一条「未执行参数纠错归同 run 的独立预算、不计为真实输入」**已在 executor 层实现**（`input_correction_budget_exhausted` + `repeated_invalid_inputs_have_a_separate_finite_correction_budget`），本次未改，也未把它写成缺口。
- 按"记录高风险待决策项"的要求新增 `docs/analysis/2026-09-21-integration-review/decision-required.md`，集中登记 8 项需要产品/安全/运维判断的事项（未确认释放是否补发、本地模型端口绑定副作用、按 provider 关工具、`partial=true` 空转规则、密钥三态默认、切换权威 writer 的停机窗口、CLI 真流式兼容边界、未签名发布与上游历史缺失），每项写明现象/证据、不决策后果、临时处理与影响面。
- 本轮机禁（S2.2 二片）：core-runtime **196/196**；全量复跑见下条。
- 交付 §2.1「网络重复提交」的去重契约（同样是**低风险 additive 契约、尚未接线**）：新增 `core-runtime/src/submission_dedup.rs`——`MessageSubmissionKey`（`client_message_id + scope + content_digest`）、`SubmissionDecision`（`New` / `ReturnExistingReceipt` / `RejectConflict`）、`MessageSubmissionRegistry`。回归 5 条：首次放行、同 ID 同内容返回**既有收据**且不重复登记、**同 ID 不同内容拒绝**、不同 ID 同内容互不影响、作用域隔离。core-runtime 由 196 增至 **201**。
  - 为什么这是真缺口：现有聊天去重是 `main.rs::check_chat_request_duplicate` 的**纯内容指纹 + 时间窗**，它能拦"同样内容重复提交"，但**拦不住"同 ID 下内容被改"**——那时两次指纹不同，两次都会执行。规范要求的正是补上客户端消息 ID 与作用域这一维。
  - 放在**新模块**而非 `run_contract.rs`：后者是 S1.1 契约且其 SHA-256 已被 S1 契约文档与 S0 快照表引用，改它会同时作废两处记录。

- 本轮机禁（S2.2 首片，逐套件带 NO-RESULT 判定）：Web 1002/1002、core-runtime **191/191**、computer-use-core 50/50、plugin-system 28/28、command-line 81/81、llm-adapter 119/119、vision-service 36/36、windows-process-guard 1/1、S0 fixture 2/2、模块链接 4/4 全部通过；`coolzhu-tool-registry` 43/43（独立重跑，批处理中曾出现上述偶发崩溃）。
- 完成 S1.5 deadline 传播的剩余两处（桌面侧）：`DesktopBridge::snapshot`/`verify` 接收 `remaining`；`capture_image(identity, remaining)` 以 `clamp_stage_timeout(remaining, CAPTURE_TIMEOUT)` 收紧，**`execute` 内的输入后截图与 `snapshot` 内的观察截图都已接入**。（浏览器侧 `BrowserBridge` 仍未接入，保留为已知缺口。）
- **已修复上一条记录的测试隔离竞争**：`env_lock()`（= `process_state_lock()`）是本 crate 既有的共享串行锁，改 cwd/env 的用例本来就持它（`5064/5163/5175/5235/5309/5336`、`powershell_errors_when_shell_is_missing`），**唯独会以进程 cwd 启动子 shell 的 `bash_tool_reports_success_exit_failure_timeout_and_background` 没有持锁**——于是并发的 `set_current_dir` + `remove_dir_all` 可以让它的 cwd 短暂指向已删除目录，子进程启动报 `os error 267`。修法就是给该用例接上同一把锁（一处改动）。复跑 3 次均 43/43 通过，套件耗时从 ~3s 升到 ~5.2s（串行化的预期代价）。
  - 诚实边界：这是**偶发**竞争，无法用"跑几次都过"证明它已根除；但机制匹配明确——它是唯一未持锁的 shell 启动用例，而失败形态正是并发 cwd 变更会产生的那种。
  - 另一条独立发现（未动）：`ScopedCurrentDir` 这个 RAII 辅助的 `Drop` 顺序是**正确**的（先恢复 cwd 再删目录，`lib.rs:3805`），但它当前**无人使用**，几个改 cwd 的用例仍在手写裸 `set_current_dir`/`restore`（顺序也正确）。真正脆弱的是"panic 时不会恢复 cwd"这一点——本轮的锁修复把它一并覆盖了（持锁后并发不再受影响），但把裸调用迁移到 `ScopedCurrentDir` 会更稳，留作后续清理。

## 风险未完成项（单独成文）

新增 `docs/analysis/2026-09-21-integration-review/risk-open-items.md`，把此前散落在各增量文档里的"未覆盖/未获得/未接线"集中成五类并逐项标注风险性质与影响面：

- **A 类（12 项）已交付但带已知边界**：六份 S2 契约**全部未接线**、`input_lease_lost` 无回归、helper 报释放失败不补发、`helper_lost` 缺迟到事实回归、`partial=true` 规则空转、S1.4 真值表未覆盖独立注册来源、hook 重排/审批失效未做、S1.3 跨进程与进程级死亡未覆盖、`execute_tab_action` 未接剩余预算、S0.3 审计与 S0.2 fixture 未扩完、`ScopedCurrentDir` 未被使用。
- **B 类未开始的工作包**：S2.1/2.3/2.4/2.5 与 S3–S7，附人日估算、前置条件与阻塞原因；剩余合计约 **150+ 人日**。
- **C 类需拍板项**：指向 `decision-required.md` 的 8 项，并标注每项解锁本文哪些条目。
- **D 类证据缺口**：0.2.12/0.2.13 源码提交未获得、四个 MSI 未签名、上游历史与 CI 缺失、**当前源码快照尚未打包成 MSI**、真实模型/桌面验收未执行。
- **E 类已闭环缺陷**：本会话修掉的 6 项（含 Windows 非阻塞 socket 继承、llm-adapter 三把锁与毒化级联、tool-registry cwd 竞争），列出以说明门禁可信度的变化。

三份文档已互相引用：`risk-open-items.md`（未完成与风险）、`decision-required.md`（需拍板）、`s2-entry-plan.md`（S2 起点与已交付契约）。

## 未完成 / 下一步

- 获取可追溯源码提交、构建产物与 MSI，再补 hash 对应表和安装验证。
- 继续 S0.3 的其余诊断状态审计，逐项区分真实探测、静态声明和未实现。
- 扩展 fixture 到完整聊天 SSE 消费、工具循环反馈及其余四个场景；仍须保持 recorded-only，不能以回放替代真实模型或桌面验证。
- S1.3 其余验收项仍未覆盖，不得据此宣称输入所有权完成：跨进程 helper 存活与失联对账、父进程/持有者进程级死亡回收、释放未知的接管策略、以及人手输入的不可约束边界。
- S1.4 其余验收项仍未覆盖：插件/MCP/调度/子 Agent 作为**独立注册来源**的来源身份与默认档位真值表（尚未接线）、「重排 hook 不能反转拒绝」的故障注入、「输入变更使审批失效」、以及 `prompt`/`allow` 旧语义在 UI/审计的展示迁移。
- S1.5 其余验收项仍未覆盖：释放宽限与「释放失败进入隔离」未实现、`partial`/`unknown` 的"重放前确认前次未生效"未回归、各阶段超时（区别于总 deadline）未注入、helper 侧 deadline 传播未验证。
- 下一顺位（按计划 5.1 的首批 PR）：PR-09 剩余部分（释放宽限、预算分类、无盲重放）收口后，S1 即可按 §5.1 的退出条件复核；随后进入 S2（共享服务、SQLite 写者与异步主循环），其中 S2.1 依赖已冻结的 1.1/1.4 契约。
