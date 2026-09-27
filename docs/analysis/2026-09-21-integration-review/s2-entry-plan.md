# S2 入场计划（共享服务、SQLite 写者与异步主循环）

状态：S1 的首批 PR（PR-01…PR-09）均已落地并有回归；本文只做 S2 的**起点认定与第一片切分**，不宣称 S2 已有进展。
依据：计划 §S2（33–55 人日）、§2.1（身份与幂等）、§2.4（配置/用量/事务）。

配套文档：`risk-open-items.md`（A 类列出本计划已交付但**未接线**的六份契约）、`decision-required.md`（需拍板项）。

## 1. 起点认定（已核对的现状）

| S2 工作包 | 现有基础 | 缺口 | 结论 |
| --- | --- | --- | --- |
| **2.1** SessionConfigService（`saved→resolved→wire` 三层 + revision + 非秘密快照） | 配置解析目前**内联在 `main.rs`**；`llm-adapter` 按解析后的值发请求 | 无独立服务、无 revision、无 wire 快照、无 resolved 来源标注 | **从零起**；符合"从 `main.rs` 提取到 `llm-adapter` 边界"的要求 |
| **2.2** 历史投影 / UsageLedger / 请求尝试登记 | `chat_tool_history`、`chat_insights` 已存在；**`core-runtime/src/usage.rs` 已有 `TokenUsage`/`UsageTracker`/`ModelPricing`/成本估算**（含会话重建 `from_session`）；usage 聚合是 `main.rs:30509` 的临时 `saturating_add` | **无法表达"未知"**：`TokenUsage` 是纯 `u32` + `Default`，缺失与 0 不可区分，而 §2.4 要求"缺 usage 为 unknown，不是 0"（**本轮已修**：新增 `ReportedUsage` + `unknown_usage_turns`/`usage_is_complete`，4 条回归）。仍缺：UsageLedger（逻辑请求 vs 网络重试**分账**）、请求 attempt 登记（只有 `run_contract.rs` 的身份字段与预算） | **部分基础可用**（先前本文误判为"从零起"，已更正），本轮完成"未知可表达"这一片 |
| **2.3** ToolDispatchService + 实例化 registry | `tool_loop_coordinator.rs` 已拆出；权限解析已在 S1.4 统一 | registry 仍非实例化（`GlobalToolRegistry::builtin()` 无插件）；审批仍散在调用点 | 部分基础可用，**服务化从零起** |
| **2.4** `session-store-sqlite` 单写者 + outbox + epoch | 会话库为 `web-sessions.sqlite3`，迁移到 v12；`run_contract` 有 owner_epoch 概念 | 无独立 crate、无单写者、无 outbox、无"按会话切换权威" | 从零起 |
| **2.5** 异步 TurnRunner | 现有两条聊天入口（流式/非流式）**各自维护运行循环** | 无共享 TurnRunner；CLI 仍靠"一次返回 Vec"模拟流式 | 从零起 |

**前置条件已满足**：§S2 的进入条件是"身份和权限契约冻结"——S1.1 的身份/预算原语与 S1.4 的最低权限解析均已落地并有回归，可作 S2 的依赖。

## 2. 建议的第一片：S2.2 的 UsageLedger（而不是 S2.1）

理由：S2.1 要动 `main.rs` 的配置解析主线（1.36–3.4 MB 单文件、多处调用点、且 §5.2 明令"`main.rs` 产品接线由单一集成负责人串行合并"），风险与审阅成本都高；S2.2 的 UsageLedger 是**新增事实层**，可以做到"先加账本与回归，再逐步接线"，符合规范"先事实，后策略"的顺序，也不与其它工作包抢 `main.rs`。

### 已交付（本轮）

| feature_id | 代码 | 验证 | 当前状态 |
| --- | --- | --- | --- |
| S2-USAGE-LEDGER-V1 | `core-runtime/src/usage.rs` 的 `ReportedUsage`、`UsageTracker::{record_reported, unknown_usage_turns, usage_is_complete}`、`UsageLedger`、`UsageAttempt`、`UsageAttemptOutcome` | `missing_usage_is_unknown_not_zero`、`fully_known_usage_keeps_the_total_complete`、`partially_known_usage_is_flagged_incomplete`、`later_unknown_turn_keeps_earlier_known_totals_and_flags_incompleteness`、`retries_share_one_logical_request_but_count_as_separate_attempts`、`failed_and_timed_out_attempts_are_retained`、`unknown_provider_usage_never_contaminates_totals_as_zero`、`cost_is_refused_when_no_price_version_is_recorded`、`cost_requires_every_attempt_to_share_one_price_version` | **契约已定义 + 回归通过**：缺 usage 记 `Unknown` 而非 0；已知/未知与供应商/估算**分列**；逻辑请求与网络尝试**分账**；失败/超时尝试保留；无价格版本或版本混杂时**拒绝出账单数字** |
| （未接线） | — | — | **不得**表述为"用量分账已上线"：账本尚未被 `main.rs` 的模型入口消费，现有 usage 聚合仍是 `main.rs:30509` 的临时求和 |
| S2-SUBMISSION-DEDUP-V1 | `core-runtime/src/submission_dedup.rs` 的 `MessageSubmissionKey`、`SubmissionDecision`、`MessageSubmissionRegistry` | `first_submission_is_new`、`same_id_same_content_returns_the_existing_receipt`、`same_id_different_content_is_rejected`、`different_ids_with_identical_content_are_independent`、`scope_isolates_identical_client_ids` | **契约已定义 + 回归通过**：§2.1「同 ID 同内容返回同收据、同 ID 不同内容拒绝」；补上了现有"纯内容指纹 + 时间窗"拦不住的那一维（同 ID 下内容被改会两次都执行） |
| S2-RECOVERY-IDENTITY-V1 | `core-runtime/src/recovery.rs` 的 `RecoveryAttempt`、`RecoveryError` | `recovery_with_a_new_identity_and_tightened_budget_is_allowed`、`reusing_the_parent_attempt_id_is_rejected`、`extending_the_deadline_is_rejected`、`inflating_any_upper_bound_is_rejected`、`exactly_carrying_over_the_parent_budget_is_allowed` | **契约已定义 + 回归通过**：§2.1 的两条禁令——恢复必须换身份（否则命中 `TerminalCache` 的旧失败终态、永远无法恢复），且不得靠换 ID 放宽 deadline 或任一项上限（只允许收紧或完全结转） |
| S2-ACTION-INJECTION-V1 | `core-runtime/src/action_injection.rs` 的 `ActionInjectionRegistry`、`InjectionDecision` | `first_injection_is_allowed_and_marked_unknown`、`retransmission_with_unknown_outcome_requires_reconciliation`、`retransmission_after_a_known_outcome_returns_the_existing_outcome`、`an_action_id_is_never_injected_twice`、`distinct_action_ids_are_independent`、`resolving_an_unknown_action_reports_a_sequencing_error` | **契约已定义 + 回归通过**：§2.1「同 action_id 不重新注入输入；未知输入结果先观察/对账，不自动重放」。补的是**按身份**去重——现有 `ActionFingerprint` + `max_same_signature` 只是**内容指纹**，同 ID 但参数/代际略变仍会二次注入 |
| （§2.1 已既有，非缺口） | `web-console/src/computer_use_executor.rs` 的 `input_correction_budget_exhausted` | `repeated_invalid_inputs_have_a_separate_finite_correction_budget`、`invalid_input_can_be_corrected_without_consuming_the_single_execution_budget` | §2.1「未执行参数纠错：新的纠错 attempt，仍归同 run 的独立纠错预算、不计为真实输入」**已在 executor 层实现并有回归**，本次未改 |
| S2-LATE-FACTS-V1 | `core-runtime/src/late_facts.rs` 的 `LateFact`、`LateFactKind`、`LateFactAppend`、`append_late_fact` | `late_facts_never_change_the_control_status`、`input_and_usage_facts_are_both_accepted`、`missing_provenance_is_rejected`、`missing_run_id_is_rejected`、`lateness_is_derived_from_observation_and_receipt_times` | **契约已定义 + 回归通过**：§2.1「终态 first-wins 仅指控制状态；迟到事实标明迟到/来源/接收时间追加，不复活运行，不丢已发生的输入与计费用量」。已有的是 `accepts_late_facts()` 判定（恒 true，S1.1 起就有回归），缺的是**追加这件事的承载物**——现已补上，并强制"五个终态下控制状态一律不变、来源与接收时间缺一不可" |
| （以上均未接线） | — | — | **不得**表述为"幂等/恢复已上线"：三个契约都还没有被请求入口消费；接线属 S2.1 之后的迁移 |

### 第二片建议（仍避开 `main.rs`）

按 §2.1 的"网络重复提交"与"未执行参数纠错"两条，补齐**幂等收据**与**纠错 attempt 预算**的纯逻辑契约与回归；接线同样留到 S2.1 之后统一做，避免两个工作包同时改 `main.rs`。

### 第一片的具体切分

1. **在 `core-runtime` 定义 `UsageLedger` 契约**（与 S1.1 同一风格：纯数据结构 + 纯函数 + 单测）
   - 记录维度（按 §2.4）：逻辑请求 vs 网络重试**分账**；供应商 token / 估算 token；已知 / **未知**；价格版本。
   - 关键约束：**缺 usage 记为 `Unknown`，不是 0**；无价格时**不得**把输入占比当账单占比。
   - 身份关联：复用 `RunIdentity` 的 `run_id` / `request_attempt_id`。
2. **新增回归**（纯函数级，无需真实模型）
   - 缺 usage → `Unknown`（且 `unknown_count` 增加，token 总量不被 0 污染）。
   - 同一逻辑请求下多次重试 → 分账计入 attempts，不重复计入逻辑请求数。
   - 有 usage 无价格 → 只报 token，不产生金额字段。
   - 迟到 usage 追加到原 run，不改变既有汇总（对应 §2.1 "迟到事实可追加"）。
3. **暂不接线**：第一片只交付契约 + 回归 + 文档；接线（`main.rs` 的模型入口消费账本）留作第二片，避免与 S2.1 抢 `main.rs`。

### 验收口径（沿用本仓库既有做法）

- 契约文档新增 feature 行（`S2-USAGE-LEDGER-V1`），列出代码位置与用例名。
- 全量门禁保持全绿（当前：Web 1002 / core-runtime 187 / CU-core 50 / tool-registry 43 / plugin-system 28 / command-line 81 / llm-adapter 119 / vision-service 36 / windows-process-guard 1 / S0 fixture 2 / 模块链接 4）。
- 不把"新契约已定义"表述为"用量分账已完成"。

## 3. 需要产品判断、不能自行决定的点

- S2.1 的 `saved→resolved→wire` 里"保持/替换/明确清空密钥"三态在 UI 上的默认（§2.4 说"空表单默认保持"）——需确认是否沿用现行为。
- S2.4 "按会话切换权威前先停写、排空 run、备份"需要一个可接受的**停机窗口**，属运维决策。
- S2.5 CLI 改造成真流式会影响 CLI 的既有输出契约，需要确认兼容边界。

## 4. 与本文相关的既有事实（避免重复劳动）

- `modules/gui-web/packages/web-console/src/main.rs` 是巨型单文件，**已有前科**（仓库存在 `main-rs-corrupted-*` 备份）。任何 S2.1/S2.5 的提取都必须分段读、按上游 SHA 复核，且一次只动一处接线。
- S0.1 的源码快照哈希见 `s0-baseline.md`；**当前树尚未打包成 MSI**，因此 S2 的任何改动目前都没有对应的安装产物身份。
