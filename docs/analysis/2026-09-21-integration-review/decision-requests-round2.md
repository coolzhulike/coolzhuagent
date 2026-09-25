# 第二轮决策请求单（13 项）

用途：把本轮执行中发现的、**只能由你（或裁决作者）决定**的问题集中列出，便于一次答完。
每条给出：问题 / 为什么不能由实施模型自定 / 源码证据 / 可选方案 / 我的建议。
背景文档：`risk-decision-review.md`（上一轮提问）、`rpr-execution-blockers.md`（执行台账 §B/§C）、`rpr-11c-budget-chain.md`。

**状态**：裁决已授权且不受这些问题阻塞的工作，已经全部执行完并通过门禁（web 1012/1012、cu-core 60/60、core-runtime 232/232、guard 6/6、tool-registry 46/46、linkage 4/4）。下面 11 项是继续往下走（RPR-04c / 05c-d / 06a / 11c 实施 / 11a 补强）的前置。

---

## A. 解锁 RPR-04c（回执全链落到事实存储）

### 1. `RunIdentity` 的 10 字段映射规则
- **不能自定的原因**：turn 级事实要凑满 10 个非空字段，就必须为不存在的维度编造值——本批明令禁止（裁决第 19 行、§2.2）。
- **证据**：`RunIdentity` = `workspace_id`/`room_id`/`session_id`/`public_turn_id`/`run_id`/`step_id`/`request_attempt_id`/`tool_call_id`/`action_id`/`owner_epoch`，`validate()` 要求全部非空（`run_contract.rs:11-40`）；而 `ChatTurnGuard` 只持有 `turn_id`/`run_id`/`claim_token`/`db_path`（`main.rs:462-469`）。该结构本质是**动作/步骤级**身份。
- **选项**：(a) 给 `RunIdentity` 加场景范围 + `validate_for(scope)`（turn 级只校验 turn 级真实维度）；(b) 只在 step/action 级写事实，turn 级终态不写（可能无法表达 turn 终态）；(c) 为不适用维度定义合法哨兵值。
- **我的建议**：(a)。它不新增同义类型、不填假值、不放宽既有 fail-closed。附带确认：`ChatTurnStatus`（`main.rs:183-191`）→ `RunTerminalStatus`（`run_contract.rs:144-150`）的自然映射是 `Completed→Succeeded`、`Failed→Failed`、`Interrupted→Cancelled`，非终态不写。

### 2. 生产 SQLite 适配器的落点
- **不能自定的原因**：决定 core-runtime 是否要引入 `rusqlite`（bundled，带 C 编译链与打包体积）。
- **证据**：RPR-03 只留了 `FactLogBackend` 接缝（`fact_store.rs:319`），未实现 SQLite；core-runtime 现无 rusqlite；web-console 侧已有 rusqlite bundled；`s2-entry-plan.md` 已把"单写者 + outbox + epoch"规划为独立 crate。
- **选项**：(a) 放 S2.4 的独立单写者 crate；(b) 放 web-console 侧实现 `FactLogBackend`；(c) 现在就把 rusqlite 拉进 core-runtime。
- **我的建议**：(b) 先落地（零新依赖、可立刻接线），S2.4 建成后再把实现搬到那个 crate（接口不变，只换后端）。

## B. 解锁 RPR-11c（预算门禁实施）

### 3. 外层根 deadline 怎么处置
- **不能自定的原因**：决定是"接线 `RunBudget`（触及所有入口与 UI 状态）"还是"显式承认当前没有根 deadline"——这是架构取舍。
- **证据**：`RunBudget`（`run_contract.rs:201-216`）只有契约与单测，**web-console 零引用**；CU 预算纯来自 config（默认 120s，`main.rs:5610-5621`），`execute_with_current_runtime`（`computer_use_executor.rs:1092-1096`）无 deadline 入参。于是 `effective_call_budget = min(...)` 的"父运行剩余时间"一项**无符号可依**。
- **选项**：(a) 接线 `RunBudget`（工作量大）；(b) 显式声明"当前无根 deadline"，min 链去掉该项并在准入文档中写明；(c) 先做 CU 级独立预算，根 deadline 留到 S3+。
- **我的建议**：(c) → 中期 (a)。先让 CU 级预算正确且可验证，不假装有根 deadline。

### 4. 是否授权给 `ComputerUsePlanner` trait 加 `remaining`
- **不能自定的原因**：跨 `computer-use-core` 的公开 API 变更。
- **证据**：`ComputerUsePlanner` 在 `controller.rs:11-35`，**没有** `remaining`；而真正发起模型请求的就是 planner。本轮给 `ComputerUseAdapter` 加了 `remaining`，覆盖不到 planner。
- **我的建议**：授权。不加则"预算守卫"在最关键的模型请求环节是空的。

### 5. 取消收尾的固定 2 秒宽限是否也要受 `remaining` 约束
- **证据**：`input_stroke.rs:170` 的宽限是固定 2 秒、不被 `remaining` 约束；`remaining = 0` 时仍会等满 2 秒才强杀。即"`min` 保证不会超过 remaining，固定宽限保证一定超过"。
- **选项**：(a) 保持固定 2 秒（保证释放动作完整性，但会超出预算）；(b) 改为 `min(2s, remaining)`（不超预算，但剩余不足时释放可能不完整）；(c) 分级：先尝试受预算约束的释放，失败再走不受约束的兜底释放并记录。
- **我的建议**：(c)。裁决 §6.5 只要求"独立且受限"，(c) 同时满足"不超预算"与"释放必须做完"。

### 6. 裁决 §6.6 六项准入门禁的原文
- **不能自定的原因**：仓库内 grep `6.6`/`准入门禁`/`六项` **零命中**，子代理只能按技术面向自行组织，**与 §6.6 的一一对应关系未验证**。
- **我的建议**：请提供原文，或确认由本轮组织的六个面向（请求链清单 / 实际 wire 体量 / 小范围能力校准 / 最低闭环可行性 / 超预算行为 / 原有输入安全门禁）替代。

## C. 解锁 RPR-11a 补强与对话侧工作

### 7. 切换前排空的方案与兜底
- **不能自定的原因**："在途"在**流式**路径上语义有歧义；硬做会得到"看起来排空、实际把模型从生成中杀掉"的假保证。且"无法判定是否排空时怎么办"是产品决定。
- **证据**：`Provider` trait（`providers/mod.rs:13-25`）不暴露 endpoint；`MessageStream` 是公开两变体 enum（`client.rs:352-355`），chat 主路径走流式（`main.rs:17371`/`54899`），握手期计数覆盖不到长生成窗口；有利事实是 main.rs **从不匹配变体**，只调 `next_event()`。详见台账 §B-14。
- **选项**：(a) 客户端计数（给 `Provider` 加带默认实现的方法 + 把 `MessageStream` 包成结构体 + `api::local_endpoint_drain()`，约 1 人日、版本无关）；(b) 轮询托管 llama-server 的 `/slots`（零 API 变更，但依赖版本相关端点且**必须用真实二进制实测**）；(c) 两者叠加。
- **我的建议**：(a) 为主、(b) 作为附加校验；兜底策略选"**无法判定时拒绝切换并提示用户**"（而不是照旧终止），因为误杀生成中的模型会直接产出不可解释的失败事实。

### 8. 浏览器面的"部分输入回执"要不要做
- **证据**：DOM 输入没有事实生产者（扩展协议不返回事实），浏览器桥只能走错误码启发式（RPR-04b 报告 §5.3）。
- **选项**：(a) 改扩展协议新增事实回报（独立工单）；(b) 接受浏览器面回执弱于桌面面，但**在事实里显式标注来源为启发式**。
- **我的建议**：(b) 当下 + (a) 列为后续；关键是不要让人误以为浏览器面的事实与桌面面同等可信。

### 9. `try_acquire_incidental_input_lease` 是否也要输入前复检
- **证据**：该路径（短时光标输入，`verify_cursor_on_target` 用）只在**取得 lease 时**校验 `is_current()`，之后到真正移动光标之间不复检（RPR-02a 报告 §5）。
- **选项**：(a) 补复检：失效即拒绝移动光标（与正式 CU run 同等强度）；(b) 保持现状，接受"_取得后短暂窗口内可能移动一次光标_"，但记录该窗口。
- **我的建议**：(a)。窗口虽小，但它是**唯一**绕过 lease 语义的正式输入通道。

### 10. 环境变量"手写 set→restore"约 15 处是否立为独立工单
- **证据**：`tool-registry` 多处 + `core-runtime/src/prompt.rs`；panic 会**永久污染** HOME/CLAW_CONFIG_HOME/PATH/CODEX_HOME，且因锁是 poison-tolerant 而**静默**级联（RPR-01 报告 §6.1）。
- **我的建议**：立为 RPR-01b。它与本轮已修的 cwd 缺陷同类，修法同为 RAII，且属测试基础设施（不影响产品行为），风险低、收益明确。

### 11. 未确认释放互锁的解除入口是否需要我这边先做最小实现
- **背景**：RPR-05b-1 正在实现"跨 run 互锁 + 可审计解除事实"，但**原生 UI 入口属于 RPR-05c**。若在没有 UI 的情况下上线互锁，用户将无法解除隔离。
- **选项**：(a) 等 RPR-05c 一起交付再启用互锁默认阻断；(b) 先允许"带审计的 API/CLI 解除"作为过渡。
- **我的建议**：(b)，并在 UI 到位后收敛到原生确认（裁决 §5.2 要求"普通 Web 请求最多打开对话框，不能仅提交 confirmed=true 就完成恢复"——所以过渡期的解除必须是**本机操作者可审计**的，不能是任意 Web 调用）。

### 12. hook 的"独立授权来源"由谁提供（**会导致功能变化，优先看这条**）
- **背景**：A-7b 的执行结果（台账 §B-15）。核对发现改前 hook **完全没有独立授权**——只要配置里出现命令就会 spawn shell，且对异常退出码/启动失败是 **fail-open**（措辞明写 "allowing tool execution to continue"）。按裁决 §7.3"没有独立授权和效果边界的外部 hook 不得在高风险路径自动扩张、不加宽松开关"，现在改为**默认未授权即不运行**。
- **后果（必须知情）**：CLI 的 hook 合并路径（`command-line/src/main.rs:3163→3176`，hooks 来自 `:2787`）没有接线授权来源，而 CLI 不在本工单允许范围内 ⇒ **CLI / 子 Agent 的 hook 目前不会运行**，只留一条 "not run" 事实。这是按裁决执行的禁用，不是遗漏。
- **不能自定的原因**：裁决只说了"没有独立授权就不放行"，**没有指定授权从哪来**。
- **选项**：(a) settings 里新增一个显式的 hook 授权字段（例如 `hooks.authorize: [...]`，可按 hook 粒度授予）；(b) 每次运行前由用户一次性批准（原生控制面确认）；(c) 只对低风险 hook 放行、高风险路径维持禁用。
- **我的建议**：(a) 为最小可行（可审计、可版本化、与既有 `with_tool_requirement` 命名空间一致），并在 RPR-05c 的原生确认面里补 (b) 作为一次性升级授权。在此之前 CLI 侧维持"按裁决禁用"。
- **另注**：`plugin-system` 里还有一份同名独立实现（`modules/tooling/packages/plugin-system/src/hooks.rs`）仍是"配置即运行"语义，不在 conversation 路径上，留给 S2.3 的"合并重复 hook 语义"处理。

### 13. 未确认释放互锁的作用域要不要扩到 session
- **背景**：RPR-05b-1 的交付（台账 §B-17）。互锁现在只在同一 `(session_id, turn_id)` 内阻断，而**新 turn 就是新 scope** ⇒ 上一 turn 遗留的未确认释放**不会阻断下一 turn**。这是当前最大的残余缺口。
- **不能自定的原因**：这是安全边界的产品决定；而"未确认释放"本质是**桌面级**状态（可能已有按键流到了系统），不因用户开了新 turn 而消失。
- **证据**：判据 SQL 含 `AND r.turn_id = ?2`（`computer_use_store.rs`）；解除表以 `(session_id, turn_id)` 为作用域；桌面输入所有权的作用域是 `current_interactive_session_scope()`（Windows 登录会话）——两者粒度现在不一致。
- **选项**：(a) 扩到 session 维度（store + 执行器各一处小改）；(b) 保持 turn 维度、把"新 turn 可绕过"记为已接受风险；(c) 扩到整个 Windows 登录会话维度，与输入所有权 scope 对齐。
- **我的建议**：(c) 最贴合语义，(a) 为最小可行；不建议 (b)。
- **另需知晓的既有行为**（非本次引入）：被互锁阻断的尝试**仍会创建 run 行**（`state=blocked`）并消耗 per-turn 调用额度；解除后以同一任务身份重复提交会命中幂等缓存返回那条 blocked 终态（不产生新输入），所以"新 attempt"必须换任务身份。

---

### 14. 原生动作算不算"有模型请求尝试"（**这一条卡着 CU 事实，见台账 §B-28**）
- **背景**：`RunIdentityScope::StepAction` 的必填维度是 **9 个**，含 `RequestAttemptId` 与 `ToolCallId`（`run_contract.rs:168-179`），构造器也把 `request_attempt_id` 当必需参数（`:473-479`）。CU 执行器里 `ToolCallId` 有真实来源（`provider_tool_call_id`），但 **`RequestAttemptId` 没有**——执行器不持有"哪一次模型请求尝试产生了这个动作"。按契约"缺失 ⇒ 身份不完整错误"、"禁止凑字段"，**不能伪造**，所以 CU 动作事实目前一条都写不了。
- **选项**：(a) 为宿主发起的原生动作新增一个 scope（如 `NativeAction`），必填维度只含容器 + `step_id` + `action_id` + `tool_call_id`；(b) 在 `StepAction` 内把 `request_attempt_id` 降为可选，规定"缺省即表示该动作不是由某次模型请求直接产生的"；(c) 把请求尝试 id 一路 plumb 到动作路径（成本最高、表达最完整）。
- **我的建议**：(b) 最小且语义清楚；(a) 更严格但要动契约枚举。

### 15. 缺会话/轮次维度时 CU 该拒绝执行还是照旧执行但不写事实（见台账 §B-28）
- **背景**：`main.rs:33952` 用 `caller_session_id.unwrap_or("session-missing")` 构造 CU 身份，缺维度时填入哨兵字符串继续执行。这对取消/落库是既有兜底，但对**事实**就是伪造维度（`"session-missing"` 不在占位值黑名单里，会通过校验进入事实）。
- **选项**：(a) 缺维度时 CU **拒绝执行**（fail-closed）；(b) **照旧执行但不写事实**，把事实缺口如实记录。
- **我的建议**：(b)——执行能力不应因审计维度缺失而丧失；但同时**必须把哨兵值从身份里去掉、改为显式 `Option`**，否则"缺维度"和"值是 session-missing"无法区分。

## 回答方式

直接按序号回答即可（例如"1 选 a；3 选 c；6 原文如下…"）。未回答的项我会维持现状并在台账标注"待裁决"，不会自行选一个。
