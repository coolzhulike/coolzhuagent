# 风险未完成项登记（Risk-Open Items）

状态：本文集中登记**尚未完成**、且带风险的事项。与 `decision-required.md` 的分工是——
本文回答"**还有什么没做完、风险是什么**"，`decision-required.md` 回答"**哪几件必须由人拍板**"（本文 C 类直接指向那里）。
口径沿用 `s0-baseline.md` 的诚实状态规则：`declared` 不等于运行时成功，未执行不得写成通过，缺产物不得混入成功率分母。

---

## A 类：已交付但带已知边界（风险性质：安全 / 正确性）

这些是**当前代码里就存在的**已知局限，任何后续增量都必须先看这一节，不能因为"某项已交付"就跳过。

| # | 未完成项 | 风险性质 | 影响面 | 当前缓解 |
| --- | --- | --- | --- | --- |
| A-1 | **S2 的六份契约全部未接线**：`ReportedUsage`/`UsageTracker` 不完整标记、`UsageLedger`、`MessageSubmissionRegistry`、`ActionInjectionRegistry`、`RecoveryAttempt`、`LateFact` | 正确性（能力实际不可用） | 六个契约只在 `core-runtime` 内可测，**没有任何请求入口消费它们**：usage 聚合仍是 `main.rs:30509` 的临时 `saturating_add`；聊天去重仍是"纯内容指纹 + 时间窗"（拦不住同 ID 改内容）；`action_id` 仍**未**作为去重键；`TurnComputerUseSupervisor` 的 `terminal_cache` 仍用同一 key（恢复身份契约未生效） | 契约与回归已就位，接线属 S2.1 之后的迁移；文档明令**不得**表述为"用量分账/幂等/恢复已上线" |
| A-2 | `input_lease_lost`（每次输入前的 epoch 复核分支）**没有触发型回归** | 安全（纵深防御无覆盖） | 正常持有路径下不可达，属防御性分支。**已有的**只是它作为"输入前失败码"被正确分类的回归（`pre_input_failure_codes_are_not_may_have_been_sent`），**不是**该分支真的被触发的用例 | 已在契约文档明确标注为"纵深防御、不冒充可复现验收路径" |
| A-3 | helper **明确报过** `mouse_release_failed` 时，不再由本进程补发一次独立释放 | 安全（左键可能保持按下） | 罕见情形下鼠标左键卡住，用户后续点击变拖拽 | 详见 `decision-required.md` D-1；已实现的是"强杀/非零退出且无原因"时的补发 |
| A-4 | `helper_lost` **缺"追加迟到事实"的专门回归** | 正确性（对账链路未验证） | 只有分类与事实，迟到事实的追加路径没有用例 | 分类与事实已回归（`helper_lost_excludes_reported_failures`） |
| A-5 | §2.2「`sent＋partial=true` 的部分路径恢复」**在生产上无产生者** | 正确性（规则空转） | 唯一有结构化回执的 helper（受控桌面笔画）要么整段成功（写死 `partial=false`）、要么返回 `Err`；**没有任何 helper 会产出 `partial=true`** | 已在契约文档登记；详见 `decision-required.md` D-4 |
| A-6 | S1.4 的工具真值表只覆盖"**已声明**（内置 / 已注册插件）× 各 profile" | 安全（暴露面未逐来源验收） | 插件、MCP、调度任务、子 Agent 作为**独立注册来源**尚未接线，其来源身份与默认档位未逐项验收 | `dev_open` 与显式配置已解耦、未知工具已 fail-closed（这两条有回归） |
| A-7 | 「**重排 hook 不能反转拒绝**」无故障注入；「**输入变更使审批失效**」未实现 | 安全（审批语义缺口） | 前者目前只依赖"闸门结果强制回写 `outcome.permission_gate = gate`"这一结构事实；后者完全没有 | 结构事实已在代码中（执行器无法伪造 gate），但未做注入验证 |
| A-8 | S1.3 未覆盖：跨进程 helper 存活与失联对账、**进程级**死亡回收、释放未知的接管策略、人手输入边界 | 安全（所有权模型不完整） | 同一 Windows 会话内已保证"至多一个有效输入所有者"（进程内）；跨进程与进程级死亡未覆盖 | 进程内 lease + epoch fencing + 旁路统一已实现并有回归 |
| A-9 | `BrowserNativeBridge::execute_tab_action`（非 trait 的辅助入口）仍用固定 `BRIDGE_TIMEOUT` | 正确性（预算不严） | 它拿不到 run 的剩余预算（不在适配器调用链上），极端情况下可超出 deadline | 桌面与浏览器**正式**输入/观察/验收路径已全部按剩余预算钳制 |
| A-10 | S0.3 其余诊断状态审计未完成 | 可信度（真假成功未逐项区分） | 尚未逐项区分"真实探测 / 静态声明 / 未实现" | 已修的两处：`/api/plugins/install` 返回 501、`/api/diagnostics/stream` 标 `declared` |
| A-11 | S0.2 fixture 未扩展到完整聊天 SSE 消费、工具循环反馈及其余四场景 | 回归覆盖不足 | 现有六类合成事件只覆盖部分链路 | fixture 为 `recorded-only`，不得用它替代真实模型/桌面验证 |
| A-12 | `tool-registry` 里**仍有几个用例手写裸 `set_current_dir`/`restore`**（`lib.rs:5064/5163`、`5175/5235`、`5309/5336`），未走已有的 `ScopedCurrentDir` | 可维护性 | 这些手写点**顺序是对的**（先恢复再删目录），但**用例 panic 时不会恢复 cwd**，会把进程 cwd 留在临时目录里 | 已用全 crate 共用的 `env_lock` 覆盖并发风险；把这几个点迁移到 `ScopedCurrentDir`（其 `Drop` 已正确处理顺序）属清理工作 |

---

## B 类：明确未开始的工作包（风险性质：进度 / 依赖）

按计划 WBS 原文的人日估算，**尚未开始**的部分：

| 工作包 | 估算 | 前置条件 | 阻塞原因 |
| --- | --- | --- | --- |
| S2.1 `SessionConfigService`（`saved→resolved→wire` 三层 + revision + 非秘密快照） | 5–8 人日 | S1.1 / S1.4 契约已冻结 ✔ | **要动 `main.rs` 配置解析主线**（巨型单文件、§5.2 明令串行合并）；且 D-5（密钥三态）待决 |
| S2.2 剩余：UsageLedger **接线**、请求 attempt 登记 | 含在 6–10 人日内 | 本会话已交付契约部分（A-1） | 接线依赖 S2.1 完成后再动 `main.rs` |
| S2.3 `ToolDispatchService` + registry 实例化 | 6–10 人日 | S1.4 ✔ | 需先实例化 registry（当前 `GlobalToolRegistry::builtin()` 无插件），并合并重复 hook 语义 |
| S2.4 `session-store-sqlite` 单写者 + outbox + 权威 epoch | 8–13 人日 | S1.1/S1.2 ✔ | **需要停机窗口**（D-6）；不能在活动会话上切权威模式 |
| S2.5 真正增量异步 `TurnRunner` | 8–14 人日 | S2.1–2.4 | CLI 改真流式需确认输出契约兼容边界（D-7） |
| **S3.1–3.5** CU 可靠性 + 受控 Paint 实验（Frame 坐标/UIA 能力/planner 反馈/失配分层/E1–E5 实验） | 28–46 人日 | S1 | 未开始；Paint 真实实验需串行且需干净桌面 |
| **S4** 产品结构、上下文与编排 | 30–51 人日 | S2 | 未开始 |
| **S5** 凭据、安全与连接器 | 31–52 人日 | S2 | 未开始 |
| **S6** 运维、迁移、安装与发布 | 20–33 人日 | S2 | 未开始 |
| **S7** 可选 DSH 试点 | 16–27 人日 | 另计 | 未开始 |

**剩余合计约 150+ 人日**（S2 剩余 + S3–S7）。这不是一个会话能完成的量；任何"已完成"的表述都必须限定到具体工作包。

---

## C 类：必须先由人决策（风险性质：产品 / 安全 / 运维）

见 `decision-required.md`，共 8 项。按对本文 A/B 类的解锁作用排序：

| 决策 | 解锁什么 |
| --- | --- |
| **D-1** 未确认释放是否补发 | 收口 A-3 |
| **D-3** 是否引入按 provider/会话的工具开关 | 解锁 A-6，并决定本地模型长任务能否稳定使用（全局关工具会伤云端 agent） |
| **D-4** 是否存在/将接入会报部分完成的 helper | 决定 A-5 是"空转规则"还是要补实现 |
| **D-5** 密钥三态默认（空表单是否保持） | S2.1 的前置 |
| **D-6** 切换权威 writer 的停机窗口 | S2.4 的前置 |
| **D-7** CLI 真流式的兼容边界 | S2.5 的前置 |
| **D-2** 本地模型端口绑定副作用（关 agent 会停 bonsai） | 影响当前本地模型配置是否维持绑 8080 |
| **D-8** 是否引入代码签名 / 可追溯 git 流程 | 收口 D 类证据缺口 |

---

## D 类：证据与可追踪性缺口（风险性质：可信度）

| # | 缺口 | 影响 |
| --- | --- | --- |
| D-1 | **0.2.12 / 0.2.13 的源码提交未获得** | 这两个产物的"出自哪份源码"无法回答（0.2.0 与 0.2.14 已可回答） |
| D-2 | **四个 MSI 全部未签名**（`signed=false`） | 无签名链可验；用户侧可能触发 SmartScreen 提示 |
| D-3 | 上游完整提交历史与 CI 结果本机没有（只取了 `main` 的 tar 快照） | 无法核对"某个能力从哪个提交引入"；`.github/workflows` 未在远端跑过 |
| D-4 | **当前源码快照（含本会话全部改动）尚未打包成 MSI** | 现有验证数字只对 `s0-baseline.md` 记录的源码哈希成立，**没有对应产物身份** |
| D-5 | 真实模型、浏览器桥、Paint、安装后桌面验收**均未执行** | 这些能力只有 `declared` / 合成回放证据，不得计入成功率 |

---

## E 类：本会话已闭环的缺陷（列出以说明门禁可信度）

不属于"未完成项"，但直接决定上面 A/B 类的验证是否可信，故登记：

| 已修 | 性质 | 说明 |
| --- | --- | --- |
| Windows 上 accept 出的 socket **继承非阻塞模式** | 测试基础设施（真缺陷） | 只设读超时未恢复阻塞 → 数据未到即 `WouldBlock` panic。`llm-adapter` 与 `tool-registry` 各一处，已加 `set_nonblocking(false)` |
| `llm-adapter` 有**三把独立 env 锁** + 毒化级联 | 测试基础设施（真缺陷） | 跨模块 env 变更并发（`MissingCredentials` 来源）；`.expect("env lock")` 让一次 panic 级联。已统一为 crate 级 `process_env_lock()` |
| `tool-registry` 的 bash 用例**未持 env_lock** | 测试隔离 | 并发 `set_current_dir` + `remove_dir_all` 使子进程 cwd 失效（`os error 267`）。已接入既有锁 |
| `dev_open` 使 `enable_llm_tools` 静默失效 | 配置语义（S1.4 已解耦） | 显式配置不再被单向覆盖 |
| 未知工具回退 `ReadOnly`（Web）/ `DangerFullAccess`（CLI） | 安全（S1.4 已 fail-closed） | 改为明确配置错误，FullAccess/Allow 也不能放行 |
| 插件非法权限串 `panic!` | 健壮性（S1.4 已修） | 改为注册期明确报错 |

**本文自身的一处更正**：初稿把 A-12 写成"`ScopedCurrentDir` 辅助**未被使用**"，依据是只 grep 了 `ScopedCurrentDir::new` 与 `ScopedCurrentDir {`（都不存在，因为构造入口叫 `enter`）。复查 `ScopedCurrentDir::enter` 后发现它**被多处测试使用**（`lib.rs:3967/4017/4081/4122/4227/4275` 等），故该结论错误、已改为上面那条更精确的描述。这与本会话反复出现的同一教训一致：**grep 级判断不足以下结论，必须读实现**。

**门禁口径**：以上修复前的数字里，`tool-registry` 与 `llm-adapter` 都出现过偶发失败/崩溃；修完后本轮全量复跑为 Web 1002 / core-runtime 217 / computer-use-core 50 / tool-registry 43 / plugin-system 28 / command-line 81 / llm-adapter 119 / vision-service 36 / windows-process-guard 1 / S0 fixture 2 / 模块链接 4，**0 失败、0 个 NO-RESULT**。
