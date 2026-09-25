//! 最小事实存储：存储端口（`FactStore`）+ 一个真实可运行的追加日志实现（RPR-03）。
//!
//! 本模块要钉死的不是"把三类事实存起来"这件事本身，而是**事实的口径**：写进去的
//! 未知、缺失与"迟到"，读回来必须一模一样——不得在往返过程中被补齐成 0、被升级成
//! "确定"，也不得被后到的结论改写。三类事实：
//!
//! 1. **run/action 事实**：动作身份与 `ActionReceipt`、控制终态、迟到事实、用量事实；
//! 2. **提交幂等**：`MessageSubmissionKey` 的"同 ID 同内容返回同收据、同 ID 不同内容拒绝"；
//! 3. **恢复身份**：恢复 attempt 与它的**最终决定**（含被拒绝的决定），旧 run 不复活。
//!
//! 三条硬约束（裁决 §5.2 精神）：
//!
//! - **不新建第二套同义类型**：端口与实现里每个事实都原样复用既有契约类型
//!   （`RunIdentity` / `ActionReceipt` / `LateFact` / `UsageAttempt` /
//!   `MessageSubmissionKey` / `RecoveryAttempt`），为此只给上述类型补了 `serde` 派生。
//! - **不把事实降级成随意的 `serde_json::Value`**：JSON 只出现在追加日志的**行编码**
//!   上，`FactLogRecord` 是个信封枚举，每个变体原样承载上面那些类型。
//! - **不引入新依赖**：只用 `serde` / `serde_json`（本就是本 crate 的直接依赖）。
//!
//! # 不变量（端口契约，实现与调用方都必须遵守）
//!
//! - **I1 只追加**：任何写入都不得改写或删除既有事实；读回永远返回完整历史
//!   （同 action 的新回执是一条**修订**，旧回执留在历史里）。
//! - **I2 缺失即 unknown**：缺失的事实读作 `FactLookup::Unknown` / `Option::None` /
//!   `ReportedUsage` 的逐维 `None`；**不得**用 0、默认终态或 `false` 补齐。
//!   缺用量时不得产生账单数字（`UsageFactSummary::billable_provider_tokens` 返回 `None`）。
//! - **I3 读回不得比写入更确定**：序列化、反序列化与重放都不得把未知升级成已知
//!   （`None` 不会变成 `Some(false)`，`ReportedUsage::unknown()` 不会变成 `Some(0)`）。
//! - **I4 终态 first-wins**：同一 run 的控制终态以**第一条**为准；写入不同的终态会被
//!   拒绝（`TerminalControlDecision::ConflictingTerminal`），日志里若出现冲突记录，
//!   读回保留第一条并把冲突计入 `FactSnapshot::conflicts`，绝不改写既有结论。
//! - **I5 迟到事实不改结论**：追加迟到事实只增不改，控制终态与既有账本结论一律不变
//!   （`LateFactAppend::revives_run()` 恒为 `false`）；挂到没有终态记录的 run
//!   上会被拒绝，因为"没有终态事实"是 unknown，不能靠默认值补齐。
//! - **I6 幂等**：同一提交重复到达只登记一次并返回同一收据；同 ID 不同内容被拒绝且
//!   不登记；同一 action 的同一回执重复写入不追加；同一次恢复重复请求不产生第二次恢复。
//! - **I7 恢复只能开新 attempt**：恢复必须换 attempt 身份、保留父关联、预算只能收紧或
//!   完全结转；旧 run 的终态记录与旧 attempt 的登记原样保留，**不复活**。
//! - **I8 不得静默丢弃**：无法解析或违反契约的记录在读回时报错
//!   （`FactStoreError::CorruptRecord` / `FactStoreError::InvalidRecord`），不得跳过。
//!
//! # 已实现 / 已接线 / 未接线（诚实标注）
//!
//! - **已实现且可运行**：`FactStore` 端口、`AppendOnlyFactStore`（唯一的规则实现体）、
//!   两个 `FactLogBackend`——`JsonlFactLog`（文件追加日志，真实落盘）与
//!   `InMemoryFactLog`（内存）。全部不变量都有回归覆盖。
//! - **未接线**：**没有任何请求入口在调用本模块**。也就是说：即使它写了日志，
//!   也只是测试或未来调用方写进去的——现有 `main.rs` 的终态落库
//!   （`finalize_chat_runtime_run_sqlite`）与聊天去重（`check_chat_request_duplicate`）
//!   **都还没走这里**。不得表述为"事实已持久化到生产链路"。
//! - **接口就绪、实现待接线**：生产用 `SQLite` 适配器**没有实现**。它只需要实现
//!   `FactLogBackend` 两个方法（按 `seq` 追加 / 按 `seq` 读回），即可复用本模块的
//!   全部规则与不变量；不需要另写一套。
//!   **为什么不在这里直接做 `SQLite`**：`core-runtime` 当前**没有** `rusqlite` 依赖
//!   （web-console 侧才有 bundled 版本）。为一个"最小、可测、additive"的存储接口
//!   往核心 crate 引入 `SQLite`（bundled 会带 C 编译链与打包体积，与裁决对打包量的
//!   关注冲突）代价明显高于收益；且 `docs/analysis/2026-09-21-integration-review/s2-entry-plan.md`
//!   已把"单写者 + outbox + epoch"规划为**独立的 `session-store-sqlite` crate**（S2.4）。
//!   结论：适配器应当实现为 `FactLogBackend` 的一个后端（放在那个 crate 或
//!   web-console 侧），而不是把 `rusqlite` 拉进 `core-runtime`。
//!
//! ## 已知范围限制（如实说明）
//!
//! - 每次追加都会**整体重放**日志来重建投影（`O(n)`），换取"重放规则只有一份、
//!   增量与全量不可能分叉"。最小实现够用；`SQLite` 适配器接好后可改为增量投影。
//! - 日志**不盖到达时间戳**：事实自身的时间（例如 `LateFact::received_at_unix_ms`）
//!   由调用方提供，存储不代它发明时间。需要"事实何时落到本存储"的诊断时，
//!   由后端（文件 `mtime` / `SQLite` 行时间）提供，属后端职责。
//! - 崩溃可能留下**半行**：读回时报 `CorruptRecord` 而不是静默丢弃（宁可响亮地失败）。
//!   截断到最后一条完整行的恢复策略留给接线阶段显式决定。
//!
//! # RPR-04c：作用域、终态与证据在本模块的落地
//!
//! - **身份随事实带 scope 与版本**：每条持久化身份都带 `RunIdentityScope` 与
//!   `RUN_IDENTITY_SCHEMA_VERSION`。写入路径用 `validate_for(scope)`（作用域由**事实类型**
//!   决定：控制终态 / attempt 登记用 `Turn`，动作事实用 `StepAction`）；读回路径用
//!   `validate_persisted()`——**缺 scope 的旧记录按旧版严格规则解释，不默认视为 `Turn`**。
//! - **终态只由宿主观察写入**：`record_host_outcome` 接受 `HostRunOutcome`，
//!   非终态（`Running` / `CancelRequested`）返回 `TerminalControlDecision::NotTerminal`
//!   **且不写事实**；只有宿主确认的取消才映射成 `Cancelled`。
//! - **动作事实走 `record_action_fact`**：身份完整才准入；身份不完整时按
//!   `admit_action_fact` 的口径分流——输入前拒绝（明确缺省、不获得输入资格）与
//!   "已经开始输入却缺身份"（**事实必须保留** + 异常标注 + 停止后续输入）。
//!   投影里的 `blocks_further_input` 是"停止后续输入"的执行点：本 run 有未解除的
//!   身份异常时，声明输入资格的新动作事实会被拒绝（`InputBlockedByIdentityAnomaly`）。
//! - **证据与回执一起落盘并互相校验**：证据由 `ActionEvidence::validate` /
//!   `validate_against(receipt)` 校验，不一致直接**拒绝写入**（不允许贴错标签混过）。
//! - **动作来源（`ActionSource` / `ContextKind` / `ActionOrigin`，见 `crate::run_contract`）
//!   本轮只做到契约侧，尚未落盘**：把它挂到 `ActionFact` 需要同时给
//!   `FactLogRecord::ActionReceipt` 加字段，而那会让 `fact_log_sqlite.rs` 的穷尽 match
//!   与结构体字面量编译失败（本轮不允许改那个文件）。因此**不得**在本模块里另造一套
//!   来源字段或第二套回执；接线时按 `admit_action_origin` 的两级校验（结构 + 可信关联）
//!   校验来源记录，再随动作事实一起落盘。
//! - **父子关联**：身份显式声明 `parent_run` 时，父运行必须已在本存储登记过
//!   （查不到 → `UnknownParentRun`），不同运行实体不得同名合并。

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::action_evidence::ActionEvidence;
use crate::late_facts::{append_late_fact, LateFact, LateFactAppend, LateFactError};
use crate::recovery::{RecoveryAttempt, RecoveryError};
use crate::run_contract::{
    admit_action_fact, ActionIdentityAdmission, ActionOrigin, ActionReceipt,
    EffectiveRunIdentityScope, HostRunOutcome, IdentityAnomaly, IdentityDimension,
    LegacyCuRunConvergenceFact, LegacyCuRunConvergenceKey, LegacyCuRunSubject, RunBudget,
    RunContractError, RunIdentity, RunIdentityScope, RunTerminalStatus,
};
use crate::submission_dedup::{
    MessageSubmissionKey, MessageSubmissionRegistry, SubmissionDecision, SubmissionRecord,
};
use crate::usage::{TokenUsage, UsageAttempt, UsageLedger};

// ---------------------------------------------------------------------------
// 读回视图：缺失一律是 unknown，不用默认值补齐（I2）
// ---------------------------------------------------------------------------

/// 事实查询结果。
///
/// 存在的意义就是 I2：`Unknown` 与 `Known` 是**两个不同的回答**，调用方无法把它读成
/// "某个默认事实"（也就不能顺手把缺 usage 读成 0，或把没记录过的 run 读成某个终态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactLookup<T> {
    /// 从未写入过该事实。**不是**某种默认值。
    Unknown,
    /// 已写入的事实。
    Known(T),
}

impl<T> FactLookup<T> {
    #[must_use]
    pub fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }

    /// 转成 `Option`：`None` 只表示"没有该事实"，不表示任何默认值。
    #[must_use]
    pub fn known(self) -> Option<T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown => None,
        }
    }
}

/// 一个 run 的用量事实汇总（读回视图）。
///
/// `known_provider_tokens` 只是**已知维度的求和**（未知维度不参与，`UsageLedger` 口径）。
/// 把它当账单数字用之前必须先看 `usage_is_complete`——因此请优先使用
/// `billable_provider_tokens()`，它在事实不完整或压根没有事实时返回 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageFactSummary {
    /// 网络尝试数（含失败与超时）。
    pub network_attempts: usize,
    /// 逻辑请求数（重试不增加这个数）。
    pub logical_requests: usize,
    /// 用量事实不完整的尝试数（任一维度未知）。
    pub unknown_usage_attempts: usize,
    /// **仅已知**维度的求和；未知维度不参与。不得单独据此出账单数字。
    pub known_provider_tokens: TokenUsage,
    /// 本地估算 token 的求和（与供应商值分列）。
    pub estimated_tokens: TokenUsage,
    /// 仅当**存在尝试且每条尝试的用量事实都完整**时为 `true`。
    /// 没有任何尝试时为 `false`——"没有事实"不等于"事实为零"。
    pub usage_is_complete: bool,
}

impl UsageFactSummary {
    /// 账单可用的供应商 token 数。
    ///
    /// 只有用量事实完整时才返回数值；否则 `None`（I2：缺 usage 必须表现为 unknown，
    /// **绝不**允许用 0 冒充完整账单）。计价本身仍由 `UsageLedger::cost_for_version`
    /// 决定，本视图不出金额。
    #[must_use]
    pub fn billable_provider_tokens(&self) -> Option<TokenUsage> {
        if self.usage_is_complete {
            Some(self.known_provider_tokens)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 事实日志的编码信封与后端
// ---------------------------------------------------------------------------

/// 一次 run attempt 的登记事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunAttemptFact {
    /// attempt 身份；`run_id` 与 `request_attempt_id` 共同构成登记键。
    pub identity: RunIdentity,
    /// 本次 attempt 的预算快照。
    pub budget: RunBudget,
    /// 由显式恢复派生时的父关联；首次尝试为 `None`（不是"父为某个默认值"）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_of: Option<RecoveryLink>,
}

/// 恢复 attempt 与父 attempt 的关联。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryLink {
    pub parent_run_id: String,
    pub parent_attempt_id: String,
}

/// 一次恢复的最终决定（这是**要被持久化的事实**，被拒绝也是决定）。
// 有意不让变体退化为 `Box`：恢复事实是低频的领域事实（每次恢复一条），保持平面结构
// 让线格式与模式匹配都直白；为省指针而装箱只会把身份挪到堆上。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryFactOutcome {
    /// 已登记新 attempt（旧 run 未被复活）。
    Granted { attempt: RunAttemptFact },
    /// 拒绝；未登记任何 attempt，旧 run 同样未被复活。
    Refused { error: RecoveryError },
}

/// 恢复事实：请求了什么、决定了什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryFact {
    pub parent_run_id: String,
    pub parent_attempt_id: String,
    pub attempt_id: String,
    /// 请求结转的预算。`Refused` 时这是**被拒绝的请求值**，不得当作生效预算
    /// （用 `effective_budget()` 取值可避免这个坑）。
    pub requested_budget: RunBudget,
    pub outcome: RecoveryFactOutcome,
}

impl RecoveryFact {
    #[must_use]
    pub fn is_granted(&self) -> bool {
        matches!(self.outcome, RecoveryFactOutcome::Granted { .. })
    }

    #[must_use]
    pub fn granted_attempt(&self) -> Option<&RunAttemptFact> {
        match &self.outcome {
            RecoveryFactOutcome::Granted { attempt } => Some(attempt),
            RecoveryFactOutcome::Refused { .. } => None,
        }
    }

    #[must_use]
    pub fn refusal(&self) -> Option<&RecoveryError> {
        match &self.outcome {
            RecoveryFactOutcome::Granted { .. } => None,
            RecoveryFactOutcome::Refused { error } => Some(error),
        }
    }

    /// 生效预算：只有 `Granted` 才有。`Refused` 时返回 `None`——
    /// 被拒绝的请求值不得冒充生效预算。
    #[must_use]
    pub fn effective_budget(&self) -> Option<RunBudget> {
        self.granted_attempt().map(|attempt| attempt.budget)
    }

    /// 自校验：`Granted` 携带的 attempt 必须与本条事实的父关联、attempt id 与预算一致。
    /// 不一致说明日志被外部改写或写入者违约——宁可报错，不要"猜哪边是真的"。
    fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("parent_run_id", &self.parent_run_id),
            ("parent_attempt_id", &self.parent_attempt_id),
            ("attempt_id", &self.attempt_id),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{field} 不能为空"));
            }
        }
        let Some(attempt) = self.granted_attempt() else {
            return Ok(());
        };
        if attempt.identity.run_id != self.parent_run_id {
            return Err("恢复登记的 attempt 与父 run 不一致".to_string());
        }
        if attempt.identity.request_attempt_id.as_deref() != Some(self.attempt_id.as_str()) {
            return Err("恢复登记的 attempt 与 attempt_id 不一致".to_string());
        }
        if attempt.budget != self.requested_budget {
            return Err("恢复登记的预算与 requested_budget 不一致".to_string());
        }
        let expected_parent = Some(RecoveryLink {
            parent_run_id: self.parent_run_id.clone(),
            parent_attempt_id: self.parent_attempt_id.clone(),
        });
        if attempt.recovery_of != expected_parent {
            return Err("恢复登记的 attempt 缺少正确的父关联".to_string());
        }
        Ok(())
    }
}

/// 一条动作事实：身份 + 回执 + （可选）证据元数据。
///
/// 三者必须**一起**落盘：只留回执会丢掉"谁说的"，只留证据会丢掉"发生了什么"。
/// 证据元数据不挂在 `ActionReceipt` 上（那会让既有生产者的结构体字面量编译失败，
/// 见 `crate::action_evidence` 的模块文档），但它们的**一致性**由本存储强制校验。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionFact {
    pub identity: RunIdentity,
    pub receipt: ActionReceipt,
    /// 证据元数据（`surface` / `evidence_basis` / 原始引用 / 推断规则版本 / `request_id`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ActionEvidence>,
    /// 动作**来源**与执行上下文类型（P-01 契约，第三轮裁决第 14 项）。
    ///
    /// 缺省表示这条事实没有随附来源判定（旧记录、或生产者尚未接线）。**不得**据此推断它是
    /// 宿主动作，也**不得**据此推断它不是模型规划动作——"缺来源"与"来源是宿主"是两件事。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ActionOrigin>,
}

impl ActionFact {
    #[must_use]
    pub fn new(identity: RunIdentity, receipt: ActionReceipt) -> Self {
        Self {
            identity,
            receipt,
            evidence: None,
            origin: None,
        }
    }

    #[must_use]
    pub fn with_origin(mut self, origin: ActionOrigin) -> Self {
        self.origin = Some(origin);
        self
    }

    #[must_use]
    pub fn with_evidence(mut self, evidence: ActionEvidence) -> Self {
        self.evidence = Some(evidence);
        self
    }

    #[must_use]
    pub fn action_id(&self) -> &str {
        &self.receipt.action_id
    }
}

/// 事实日志中的一条记录。
///
/// 它是**信封**而不是新的事实类型：每个变体原样承载既有契约类型，因此既不需要
/// "第二套同义类型"，也不需要把事实降级成自由 JSON。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
// 同 `RecoveryFactOutcome`：日志记录是低频追加的领域事实，保持平面结构不做装箱。
#[allow(clippy::large_enum_variant)]
pub enum FactLogRecord {
    /// run 的控制终态（first-wins）。
    TerminalControl {
        identity: RunIdentity,
        status: RunTerminalStatus,
    },
    /// 首次尝试（无父）的登记。
    AttemptOpened { attempt: RunAttemptFact },
    /// 一次恢复的最终决定（含被拒绝的决定）。
    RecoveryDecided { fact: RecoveryFact },
    /// 一条动作回执（含可选证据元数据、可选来源判定与身份异常标注）。
    ActionReceipt {
        identity: RunIdentity,
        receipt: ActionReceipt,
        /// 身份不完整却已经开始输入：**事实必须保留**，异常必须一起保留。
        ///
        /// 缺省表示身份完整（旧记录也走这条：旧记录按旧版严格规则解释）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity_anomaly: Option<IdentityAnomaly>,
        /// 证据元数据；缺省表示这条事实没有随附证据（不得据此推断依据）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence: Option<ActionEvidence>,
        /// 动作来源与上下文类型（P-01 契约）；缺省表示没有随附来源判定。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<ActionOrigin>,
    },
    /// 一条迟到事实。
    LateFact { fact: LateFact },
    /// 一次网络尝试的用量事实。
    UsageAttempt { attempt: UsageAttempt },
    /// 一次消息提交的登记。
    MessageSubmission {
        key: MessageSubmissionKey,
        receipt_id: String,
    },
}

/// 事实日志的持久化后端。
///
/// 实现必须满足：
///
/// 1. **只能追加**：不得改写或删除任何既有记录；
/// 2. **按写入顺序读回**：`read_all` 的顺序必须与 `append` 的顺序一致；
/// 3. **不得静默丢弃**：读回时遇到无法解析的记录必须报错，不能跳过；
/// 4. `append` 返回 `Ok` 即表示该记录对后续 `read_all` 可见。
///
/// 生产用 `SQLite` 适配器只需实现本 trait（见模块文档"未接线"一节）。
pub trait FactLogBackend {
    fn append(&mut self, record: &FactLogRecord) -> Result<(), FactStoreError>;
    fn read_all(&mut self) -> Result<Vec<FactLogRecord>, FactStoreError>;
}

/// 内存后端：用于测试与"进程内一次性事实"。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemoryFactLog {
    records: Vec<FactLogRecord>,
}

impl InMemoryFactLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn records(&self) -> &[FactLogRecord] {
        &self.records
    }
}

impl FactLogBackend for InMemoryFactLog {
    fn append(&mut self, record: &FactLogRecord) -> Result<(), FactStoreError> {
        self.records.push(record.clone());
        Ok(())
    }

    fn read_all(&mut self) -> Result<Vec<FactLogRecord>, FactStoreError> {
        Ok(self.records.clone())
    }
}

/// 文件后端：一行一条 JSON 的**追加日志**（真实落盘，不依赖任何外部数据库）。
///
/// 行编码是 `FactLogRecord` 的 `serde_json` 表示；一行的内容要么完整、要么可判定
/// 为损坏，不存在"看起来成功的半条"被当成事实读回的情况（I8）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlFactLog {
    path: PathBuf,
}

impl JsonlFactLog {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl FactLogBackend for JsonlFactLog {
    fn append(&mut self, record: &FactLogRecord) -> Result<(), FactStoreError> {
        let line = serde_json::to_string(record).map_err(|error| FactStoreError::Encode {
            message: error.to_string(),
        })?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| io_error(Some(&self.path), &error))?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| io_error(Some(&self.path), &error))?;
        // 整行一次写入再落盘：崩溃最多留下"可判定为损坏"的半行，不会留下错行。
        file.write_all(line.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|error| io_error(Some(&self.path), &error))
    }

    fn read_all(&mut self) -> Result<Vec<FactLogRecord>, FactStoreError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            // 文件不存在 = 还没有任何事实（不是错误，也不是"空事实已存在"）。
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(Some(&self.path), &error)),
        };
        let mut records = Vec::with_capacity(text.lines().count());
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record = serde_json::from_str::<FactLogRecord>(line).map_err(|error| {
                FactStoreError::CorruptRecord {
                    path: Some(self.path.clone()),
                    line: index + 1,
                    message: error.to_string(),
                }
            })?;
            records.push(record);
        }
        Ok(records)
    }
}

fn io_error(path: Option<&Path>, error: &std::io::Error) -> FactStoreError {
    FactStoreError::Io {
        path: path.map(Path::to_path_buf),
        message: error.to_string(),
    }
}

// ---------------------------------------------------------------------------
// 端口
// ---------------------------------------------------------------------------

/// 事实存储端口。
///
/// 语义与不变量见模块文档（I1–I8）。要点：
///
/// - 每个 `record_*` 方法在**首次生效**时向日志追加事实，重复到达时按 I6 幂等；
/// - 判定规则一律复用既有纯逻辑契约（`run_contract` / `late_facts` / `recovery` /
///   `submission_dedup` / `usage`），本模块不另立第二套规则；
/// - 读回走 `snapshot()`，缺失表现为 `FactLookup::Unknown`（I2），
///   且读回值不得比写入时更确定（I3）。
///
/// 方法只取 `&mut self` / `&self` 且不接受泛型参数，因此 `Box<dyn FactStore>` 可用。
pub trait FactStore {
    /// 登记 run 的控制终态。first-wins：已有**相同**终态返回 `AlreadyTerminal`（幂等），
    /// 已有**不同**终态返回 `ConflictingTerminal` 且**不改写**既有事实（I4）。
    fn record_terminal_control(
        &mut self,
        identity: &RunIdentity,
        status: RunTerminalStatus,
    ) -> Result<TerminalControlDecision, FactStoreError>;

    /// 按**宿主观察到的结局**登记终态（生产侧应当走这里）。
    ///
    /// `HostRunOutcome` 携带"为什么结束"：非终态（仍在运行 / 刚请求取消）返回
    /// `TerminalControlDecision::NotTerminal` 并且**不写终态事实**；只有宿主确认的取消
    /// （`confirmed_cancel`）才映射成 `Cancelled`，取消原因不明 / 异常中断映射成
    /// `Interrupted`（不得冒充取消）。
    fn record_host_outcome(
        &mut self,
        identity: &RunIdentity,
        outcome: &HostRunOutcome,
    ) -> Result<TerminalControlDecision, FactStoreError>;

    /// 登记一次 attempt 的开启（首次尝试，无父）。恢复走 `open_recovery_attempt`。
    fn open_attempt(
        &mut self,
        identity: &RunIdentity,
        budget: RunBudget,
    ) -> Result<AttemptDecision, FactStoreError>;

    /// 登记一次**显式恢复**并给出最终决定。
    ///
    /// `identity.request_attempt_id` 是**新 attempt** 的身份，`identity.run_id` 必须与父
    /// attempt 同属一个 run（恢复是同一 run 下的新 attempt，不是新 run）。
    /// 父 attempt 的预算必须已被 `open_attempt` / 本方法登记过；**查不到就拒绝**
    /// （`FactStoreError::UnknownParentAttempt`）——缺失是 unknown，不允许假设一个父预算
    /// 然后据此放行（那正是"靠换 ID 重置预算"的漏洞）。
    fn open_recovery_attempt(
        &mut self,
        identity: &RunIdentity,
        parent_attempt_id: &str,
        remaining_budget: RunBudget,
    ) -> Result<RecoveryDecision, FactStoreError>;

    /// 记录一条动作回执。首次为 `Recorded`；与既有最后一条完全相同为
    /// `AlreadyRecorded`（幂等、不追加）；不同则追加为一条**修订**并交回旧值
    /// （`Revision { previous }`，旧回执仍留在历史中，I1）。
    ///
    /// 本方法等价于不带证据的 `record_action_fact`：身份不完整时的分流口径完全一致。
    fn record_action_receipt(
        &mut self,
        identity: &RunIdentity,
        receipt: &ActionReceipt,
    ) -> Result<ReceiptDecision, FactStoreError>;

    /// 记录一条动作事实（身份 + 回执 + 可选证据）。
    ///
    /// 除回执自身的自洽性与证据一致性外，身份侧按 `admit_action_fact` 分流：
    ///
    /// - 身份完整 → 正常记录（`Recorded` / `AlreadyRecorded` / `Revision`）；
    /// - 输入前就被拒绝 → `RecordedPreInputRejection`（**明确缺省**，但**不获得输入资格**）；
    /// - 已经开始输入却缺身份 → `RecordedWithIdentityAnomaly`：事实**必须保留**，
    ///   异常随记录落盘，并且此后该 run 声明输入资格的新动作事实会被拒
    ///   （`InputBlockedByIdentityAnomaly`，即"停止后续输入"的执行点）。
    fn record_action_fact(&mut self, fact: &ActionFact) -> Result<ReceiptDecision, FactStoreError>;

    /// 追加一条迟到事实。要求该 run 已有终态事实（否则 `UnknownRunTerminal`），
    /// 复用 `late_facts::append_late_fact` 的来源校验，且**不改变**控制终态（I5）。
    fn append_late_fact(&mut self, fact: &LateFact) -> Result<LateFactAppend, FactStoreError>;

    /// 登记一次网络尝试的用量事实。未知维度原样保留为 `None`（I2/I3）。
    fn record_usage_attempt(
        &mut self,
        attempt: &UsageAttempt,
    ) -> Result<UsageAttemptDecision, FactStoreError>;

    /// 提交一条消息。只有 `SubmissionDecision::New` 会登记事实并允许继续执行；
    /// 同 ID 同内容返回既有收据（不追加、不重复产生副作用），同 ID 不同内容拒绝（I6）。
    fn submit_message(
        &mut self,
        key: &MessageSubmissionKey,
        receipt_id: &str,
    ) -> Result<SubmissionDecision, FactStoreError>;

    /// 只读投影：读回的事实。缺失一律 `FactLookup::Unknown`（I2）。
    fn snapshot(&self) -> &FactSnapshot;
}

// ---------------------------------------------------------------------------
// 判决与决定
// ---------------------------------------------------------------------------

/// 终态登记的判决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalControlDecision {
    /// 首次登记，已追加事实。
    Recorded(RunTerminalStatus),
    /// 相同终态已存在：幂等，未追加。
    AlreadyTerminal(RunTerminalStatus),
    /// 已存在**不同**终态：拒绝改写，返回既有终态（I4）。
    ConflictingTerminal(RunTerminalStatus),
    /// 宿主还没给出终局（仍在运行 / 刚请求取消）：**不写终态事实**。
    ///
    /// 这不是错误：没有终态就是没有终态，不能用默认值凑一条。
    NotTerminal,
}

/// attempt 登记的判决。
// 同 `RecoveryFactOutcome`：冲突判决要交回**既有登记**本身（省略它等于让调用方
// 看不到为什么被拒），因此不做装箱。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptDecision {
    /// 首次登记。
    Opened,
    /// 完全相同的登记已存在：幂等，未追加。
    AlreadyOpened,
    /// 同 `(run_id, request_attempt_id)` 已存在但内容不同：拒绝，返回既有登记。
    ConflictingAttempt { existing: RunAttemptFact },
}

/// 恢复请求的最终决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryDecision {
    /// 已登记新 attempt：旧 run 的终态与旧 attempt 的登记均未被改写。
    GrantedNewAttempt { attempt: RecoveryAttempt },
    /// 同一次恢复此前已登记：幂等，不产生第二次恢复。
    AlreadyRecorded { attempt_id: String },
    /// 拒绝；未登记任何 attempt，也未复活旧 run。
    Refused(RecoveryError),
}

/// 回执记录的判决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptDecision {
    /// 首次记录该 action 的回执。
    Recorded,
    /// 与既有最后一条完全相同：幂等，未追加。
    AlreadyRecorded,
    /// 追加了一条修订回执；`previous` 是历史里上一条（仍被保留，I1）。
    Revision { previous: ActionReceipt },
    /// 输入已经开始却缺身份：事实**已保留**（原回执 + 身份异常），
    /// 并且**必须停止后续输入**（调用方不得忽略这个判决）。
    ///
    /// 重复到达时仍返回本判决：异常不会因为幂等而消失。
    RecordedWithIdentityAnomaly { anomaly: IdentityAnomaly },
    /// 输入前就被拒绝：该拒绝事实可以记录，但**不获得输入资格**。
    ///
    /// `absent_dimensions` 是明确缺省（不是"本应存在却缺失"）的动作维度；
    /// 重复到达时仍返回本判决（它描述的是这条事实的性质）。
    RecordedPreInputRejection {
        absent_dimensions: Vec<IdentityDimension>,
    },
}

/// 用量尝试登记的判决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageAttemptDecision {
    /// 首次登记该 `(run, 逻辑请求, 尝试)`。
    Recorded,
    /// 完全相同的一条已存在：幂等，未追加。
    AlreadyRecorded,
    /// 同一身份已存在但内容不同：拒绝，返回既有事实（不覆盖既有观测）。
    ConflictingAttempt { existing: UsageAttempt },
}

// ---------------------------------------------------------------------------
// RD4-01（第五轮裁决 A-1）：遗留 CU 运行收敛的规则
// ---------------------------------------------------------------------------
//
// 契约（事实的形状、允许说什么）在 `crate::run_contract` 的"遗留 CU 运行的收敛契约"一节。
// 本节只放**规则**：给定"来源数据库里独立读到的旧 run 现状"与"已经存在的收敛事实"，
// 判定这次收敛请求该写、该幂等跳过、还是该拒绝。
//
// 三条规则（裁决 A-1.5 / A-1.6 的落点）：
//
// 1. **幂等**：同一条收敛的重复请求（重复启动 / 双实例 / 响应丢失后重发）**不再写第二份**，
//    返回既有的那条事实。
// 2. **不覆盖**：该行已有**真实终态提交**、已有**待确认的终态提交候选**、或已被别的提交
//    更新过（原状态 / revision 变了）时，一律**拒绝**，绝不合成 `Interrupted` 覆盖它。
//    同一对账键上已有**内容不同**的收敛事实时同样拒绝（把冲突交给调用方，不静默改写）。
// 3. **只认账面对账键**：对账键是"来源对象 + 原状态·revision"，不是时间、不是随机 ID、
//    不是"当前工作区"。因此双实例并发时两条请求会落到同一个键上，由规则给出同一结论。
//
// 本节的函数是纯逻辑：不持有状态、不写任何存储。真正的写入（单事务里"先建资源阻断、
// 再写旧 run 非成功终态 + 收敛事实"）由来源数据库的存储适配器负责，见 web-console 的
// `computer_use_store`。

/// 来源数据库里**独立读到的**旧 run 现状。
///
/// 三个字段都是观测值，不是推断值：读不到就是读不到，调用方不得用默认值补齐。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyCuRunObservedState {
    /// 读到的最新状态字面量。
    pub state: String,
    /// 读到的最新 `state_version`。
    pub state_version: u64,
    /// 该行是否已有**真实**终态结果（例如 `terminal_result_json IS NOT NULL`，
    /// 或状态本身就是终态）。
    pub has_real_terminal: bool,
    /// 该行是否存在**待确认的终态提交候选**（执行已结束、提交未确认那一类事实）。
    pub has_commit_candidate: bool,
}

/// 收敛请求的判决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyCuRunConvergenceRuleDecision {
    /// 首次收敛：允许写入（调用方仍需按 A-1.6 的顺序在同一事务里落库）。
    Recorded,
    /// 同一次收敛已存在：**幂等，不写第二份**，也不得改写旧 run 行。
    AlreadyConverged {
        existing: Box<LegacyCuRunConvergenceFact>,
    },
    /// 拒绝：不写任何事实、不改旧 run 行。
    Refused(LegacyCuRunConvergenceRefusal),
}

/// 拒绝收敛的原因。每个原因都可区分，便于审计与纠错（不允许一句"拒绝了"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyCuRunConvergenceRefusal {
    /// 该行已有真实终态提交：**不得**合成 `Interrupted` 覆盖。
    AlreadyHasRealTerminal { observed_state: String },
    /// 该行存在待确认的终态提交候选（CommitPending 口径）：**不得**合成 `Interrupted` 覆盖。
    CommitCandidatePending,
    /// 同一对账键上已有**内容不同**的收敛事实：**不得**覆盖，交回既有事实。
    ConflictingConvergence {
        existing: Box<LegacyCuRunConvergenceFact>,
    },
    /// 观测到的原状态 / revision 与请求不一致（该行已被别的提交更新）：**不得**覆盖。
    OriginalRevisionChanged {
        observed_state: String,
        observed_state_version: u64,
    },
    /// 同一次恢复的操作 ID 已被**另一个**对象使用：混淆操作 ID 会让对账错位。
    RecoveryOperationReused { other_run_id: String },
    /// 同一对账键上已存在**可重入的收敛意图**，但它的操作 ID / 登记时刻与本次不同：
    /// **不覆盖**既有意图。重复启动 / 双实例应当**重新进入**已登记的那次收敛
    /// （用记录里的操作 ID），而不是另起一次。
    ConflictingIntent {
        existing_operation_id: String,
        existing_recorded_at_unix_ms: u64,
    },
}

impl LegacyCuRunConvergenceRefusal {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::AlreadyHasRealTerminal { .. } => "legacy_convergence_already_terminal",
            Self::CommitCandidatePending => "legacy_convergence_commit_candidate_pending",
            Self::ConflictingConvergence { .. } => "legacy_convergence_conflicting_fact",
            Self::OriginalRevisionChanged { .. } => "legacy_convergence_original_revision_changed",
            Self::RecoveryOperationReused { .. } => "legacy_convergence_operation_id_reused",
            Self::ConflictingIntent { .. } => "legacy_convergence_conflicting_intent",
        }
    }
}

/// 在来源数据库里按 `subject` 读回**最新的**收敛事实。
///
/// 与其它读回口径一致：缺失是 `Unknown`，不是"没有收敛过"之外的任何默认值。
#[must_use]
pub fn latest_legacy_cu_run_convergence<'a>(
    existing: &'a [LegacyCuRunConvergenceFact],
    subject: &LegacyCuRunSubject,
) -> FactLookup<&'a LegacyCuRunConvergenceFact> {
    existing
        .iter()
        .rev()
        .find(|fact| {
            fact.subject.source_database_identity == subject.source_database_identity
                && fact.subject.original_run_id == subject.original_run_id
        })
        .map_or(FactLookup::Unknown, FactLookup::Known)
}

/// 收敛规则：判定这次收敛请求该写、该幂等跳过、还是该拒绝。
///
/// `existing` 是来源数据库里该对象（乃至整个库）已有的收敛事实；`observed` 是本次独立
/// 读到的旧 run 现状；`candidate` 是本次请求写入的收敛事实。
///
/// 判定顺序（顺序本身是语义的一部分，测试逐条覆盖）：
///
/// 1. 请求自身必须成立（`candidate.validate()`）：不成立直接报错，**不写任何东西**；
/// 2. 真实终态 / 提交候选 → 拒绝（不允许把已有结论合成 `Interrupted`）；
/// 3. 原状态·revision 与观测不一致 → 拒绝（该行已被别的提交更新，不覆盖）；
/// 4. 同一对账键已有收敛事实：内容相同 → `AlreadyConverged`（幂等）；内容不同 → 拒绝；
/// 5. 同一次恢复的操作 ID 已用在**别的对象**上 → 拒绝；
/// 6. 其余 → `Recorded`。
pub fn reconcile_legacy_cu_run_convergence(
    existing: &[LegacyCuRunConvergenceFact],
    observed: &LegacyCuRunObservedState,
    candidate: &LegacyCuRunConvergenceFact,
) -> Result<LegacyCuRunConvergenceRuleDecision, FactStoreError> {
    candidate.validate().map_err(|error| {
        FactStoreError::InvalidRecord {
            index: 0,
            message: format!(
                "遗留运行收敛事实不成立（{}）：{}",
                error.code, error.message
            ),
        }
    })?;
    if observed.has_real_terminal {
        return Ok(LegacyCuRunConvergenceRuleDecision::Refused(
            LegacyCuRunConvergenceRefusal::AlreadyHasRealTerminal {
                observed_state: observed.state.clone(),
            },
        ));
    }
    if observed.has_commit_candidate {
        return Ok(LegacyCuRunConvergenceRuleDecision::Refused(
            LegacyCuRunConvergenceRefusal::CommitCandidatePending,
        ));
    }
    let key: LegacyCuRunConvergenceKey = candidate.reconciliation_key();
    if observed.state != key.original_state || observed.state_version != key.original_state_version {
        return Ok(LegacyCuRunConvergenceRuleDecision::Refused(
            LegacyCuRunConvergenceRefusal::OriginalRevisionChanged {
                observed_state: observed.state.clone(),
                observed_state_version: observed.state_version,
            },
        ));
    }
    if let Some(existing_fact) = existing
        .iter()
        .find(|fact| fact.reconciliation_key() == key)
    {
        return Ok(if existing_fact.same_convergence_as(candidate) {
            LegacyCuRunConvergenceRuleDecision::AlreadyConverged {
                existing: Box::new(existing_fact.clone()),
            }
        } else {
            LegacyCuRunConvergenceRuleDecision::Refused(
                LegacyCuRunConvergenceRefusal::ConflictingConvergence {
                    existing: Box::new(existing_fact.clone()),
                },
            )
        });
    }
    let operation_id = &candidate.decision.recovery_operation_id;
    if let Some(other) = existing.iter().find(|fact| {
        fact.decision.recovery_operation_id == *operation_id
            && fact.subject.original_run_id != key.original_run_id
    }) {
        return Ok(LegacyCuRunConvergenceRuleDecision::Refused(
            LegacyCuRunConvergenceRefusal::RecoveryOperationReused {
                other_run_id: other.subject.original_run_id.clone(),
            },
        ));
    }
    Ok(LegacyCuRunConvergenceRuleDecision::Recorded)
}

// ---------------------------------------------------------------------------
// 投影：重放出的只读视图
// ---------------------------------------------------------------------------
/// 事实日志的只读投影。
///
/// 由 `replay` 从记录序列重建，只保留**投影所需**的信息；缺失一律是 unknown。
/// 同一份记录序列重放出的投影必然相同（确定性），因此"内存投影"与"重新打开日志"
/// 的结果一致——这是往返一致性的可断言基础。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactSnapshot {
    /// `run_id` -> 首个终态（first-wins）。
    terminal: BTreeMap<String, RunTerminalStatus>,
    /// `(run_id, request_attempt_id)` -> attempt 登记。
    attempts: BTreeMap<(String, String), RunAttemptFact>,
    /// `action_id` -> 回执历史（追加顺序，保留全部修订）。
    receipts: BTreeMap<String, Vec<ActionReceipt>>,
    /// `action_id` -> 证据元数据历史（追加顺序，保留全部修订）。
    evidences: BTreeMap<String, Vec<ActionEvidence>>,
    /// 按 `action_id` 保留的动作**来源**投影（与证据同一口径：读不到就是 `Unknown`）。
    origins: BTreeMap<String, Vec<ActionOrigin>>,
    /// `run_id` -> 身份异常（追加顺序）。非空表示该 run **不得再发输入**。
    identity_anomalies: BTreeMap<String, Vec<IdentityAnomaly>>,
    /// `run_id` -> 迟到事实（追加顺序）。
    late_facts: BTreeMap<String, Vec<LateFact>>,
    /// `run_id` -> 用量事实（追加顺序）。
    usage: BTreeMap<String, Vec<UsageAttempt>>,
    /// 提交去重表（复用 `MessageSubmissionRegistry` 的判决口径）。
    submissions: MessageSubmissionRegistry,
    /// `parent_run_id` -> 恢复事实（追加顺序）。
    recoveries: BTreeMap<String, Vec<RecoveryFact>>,
    /// 日志中与首个结论冲突的记录计数（应为 0；>0 表示日志被外部改写或来自违约写入者）。
    conflicts: BTreeMap<String, u32>,
}

impl FactSnapshot {
    /// 从记录序列重建投影。
    ///
    /// 结构性违约（身份为空、回执自相矛盾、恢复事实自相矛盾、迟到事实缺来源、
    /// 迟到事实挂到没有终态记录的 run）一律**报错**而不是跳过（I8）。
    /// 结论冲突（同一 run 的第二个不同终态、同一提交槽位的不同摘要）不报错：
    /// 保留第一条并计入 `conflicts`（I4）。
    pub fn replay(records: &[FactLogRecord]) -> Result<Self, FactStoreError> {
        let mut snapshot = Self::default();
        for (index, record) in records.iter().enumerate() {
            snapshot.apply(index, record)?;
        }
        Ok(snapshot)
    }

    fn apply(&mut self, index: usize, record: &FactLogRecord) -> Result<(), FactStoreError> {
        match record {
            FactLogRecord::TerminalControl { identity, status } => {
                self.apply_terminal_control(index, identity, *status)
            }
            FactLogRecord::AttemptOpened { attempt } => {
                if attempt.recovery_of.is_some() {
                    return Err(invalid_record(
                        index,
                        "首次尝试的登记不得携带父关联（恢复走 RecoveryDecided）",
                    ));
                }
                self.insert_attempt(index, attempt)
            }
            FactLogRecord::RecoveryDecided { fact } => self.apply_recovery_decided(index, fact),
            FactLogRecord::ActionReceipt {
                identity,
                receipt,
                identity_anomaly,
                evidence,
                origin,
            } => self.apply_action_receipt(
                index,
                identity,
                receipt,
                identity_anomaly.as_ref(),
                evidence.as_ref(),
                origin.as_ref(),
            ),
            FactLogRecord::LateFact { fact } => self.apply_late_fact(index, fact),
            FactLogRecord::UsageAttempt { attempt } => self.apply_usage_attempt(index, attempt),
            FactLogRecord::MessageSubmission { key, receipt_id } => {
                self.apply_message_submission(index, key, receipt_id)
            }
        }
    }

    fn apply_terminal_control(
        &mut self,
        index: usize,
        identity: &RunIdentity,
        status: RunTerminalStatus,
    ) -> Result<(), FactStoreError> {
        validate_identity_for_record(index, identity, RunIdentityScope::Turn)?;
        match self.terminal.get(&identity.run_id).copied() {
            Some(existing) if existing == status => {}
            Some(_) => self.note_conflict(format!("terminal:{}", identity.run_id)),
            None => {
                self.terminal.insert(identity.run_id.clone(), status);
            }
        }
        Ok(())
    }

    fn apply_recovery_decided(
        &mut self,
        index: usize,
        fact: &RecoveryFact,
    ) -> Result<(), FactStoreError> {
        fact.validate()
            .map_err(|message| invalid_record(index, message))?;
        self.recoveries
            .entry(fact.parent_run_id.clone())
            .or_default()
            .push(fact.clone());
        match fact.granted_attempt() {
            Some(attempt) => self.insert_attempt(index, attempt),
            None => Ok(()),
        }
    }

    fn apply_action_receipt(
        &mut self,
        index: usize,
        identity: &RunIdentity,
        receipt: &ActionReceipt,
        identity_anomaly: Option<&IdentityAnomaly>,
        evidence: Option<&ActionEvidence>,
        origin: Option<&ActionOrigin>,
    ) -> Result<(), FactStoreError> {
        // 读回一侧按记录**自身**的口径解释：
        // - 声明了 scope 的新记录 → 走准入规则（`admit_action_fact`：身份完整 /
        //   输入前拒绝（明确缺省）/ 已开始输入却缺身份（异常保留）三条分流只有一处定义）；
        // - 缺 scope 的旧记录 → 旧版严格规则（十维全必填），**不得**默认视为任一 scope。
        let admission = match identity.resolved_scope() {
            EffectiveRunIdentityScope::LegacyUnscoped => {
                validate_identity_for_record(index, identity, RunIdentityScope::StepAction)?;
                if identity_anomaly.is_some() || evidence.is_some() {
                    return Err(invalid_record(
                        index,
                        "缺 scope 的旧记录不得携带身份异常或证据元数据（它们是 scope 显式化之后的概念）",
                    ));
                }
                receipt
                    .validate()
                    .map_err(|error| invalid_record(index, error.message))?;
                if let Some(declared) = identity.action_id() {
                    if declared != receipt.action_id {
                        return Err(invalid_record(
                            index,
                            "回执的 action_id 与身份中的 action_id 不一致",
                        ));
                    }
                }
                None
            }
            EffectiveRunIdentityScope::Declared(_) => Some(admit_action_fact(identity, receipt).map_err(
                |error| invalid_record(index, format!("{}：{}", error.code, error.message)),
            )?),
        };
        match (identity_anomaly, &admission) {
            (
                Some(stored),
                Some(ActionIdentityAdmission::PreservedWithAnomaly {
                    anomaly: computed,
                }),
            ) => {
                stored.validate().map_err(|error| {
                    invalid_record(index, format!("身份异常自身不自洽：{}", error.message))
                })?;
                if stored != computed {
                    return Err(invalid_record(
                        index,
                        "身份异常标注与契约算出的异常不一致（异常必须来自准入判决，不得自行编造）",
                    ));
                }
            }
            (Some(_), _) => {
                return Err(invalid_record(
                    index,
                    "带身份异常标注的记录，其身份与该回执并不缺身份：异常标注没有依据",
                ))
            }
            (None, Some(ActionIdentityAdmission::PreservedWithAnomaly { .. })) => {
                // 缺身份却已经开始输入：必须带异常一起落盘，否则就是"静默丢事实"。
                return Err(invalid_record(
                    index,
                    "已经开始输入却缺身份的事实必须带身份异常标注（不得静默接受）",
                ));
            }
            (None, _) => {}
        }
        if let Some(evidence) = evidence {
            // 证据不是展示标签：自洽性 + 与回执的一致性都要过，否则拒绝而不是"记下了事"。
            evidence
                .validate()
                .map_err(|error| invalid_record(index, format!("证据不自洽：{}", error.message)))?;
            evidence.validate_against(receipt).map_err(|error| {
                invalid_record(index, format!("证据与回执矛盾：{}", error.message))
            })?;
        }
        let history = self.receipts.entry(receipt.action_id.clone()).or_default();
        // 与最后一条相同 = 同一事实重复到达（不重复追加）；不同 = 一次**修订**，
        // 旧回执必须留在历史里（I1）。
        if history.last() != Some(receipt) {
            history.push(receipt.clone());
        }
        if let Some(evidence) = evidence {
            let list = self.evidences.entry(receipt.action_id.clone()).or_default();
            if !list.contains(evidence) {
                list.push(evidence.clone());
            }
        }
        if let Some(origin) = origin {
            // 读回一侧同样只做**结构层**校验（可信关联层需要真实父对象，读回时没有该上下文）；
            // 结构不合法的来源**不得**被静默接受进投影。
            origin.validate_structure().map_err(|error| {
                invalid_record(index, format!("动作来源不自洽：{}", error.message))
            })?;
            let list = self.origins.entry(receipt.action_id.clone()).or_default();
            if !list.contains(origin) {
                list.push(origin.clone());
            }
        }
        if let Some(anomaly) = identity_anomaly {
            let list = self
                .identity_anomalies
                .entry(anomaly.run_id.clone())
                .or_default();
            if !list.contains(anomaly) {
                list.push(anomaly.clone());
            }
        }
        Ok(())
    }

    fn apply_late_fact(&mut self, index: usize, fact: &LateFact) -> Result<(), FactStoreError> {
        let Some(status) = self.terminal.get(&fact.run_id).copied() else {
            return Err(invalid_record(
                index,
                format!(
                    "迟到事实挂到了没有终态记录的 run：{}（缺失是 unknown，不得默认某个终态）",
                    fact.run_id
                ),
            ));
        };
        append_late_fact(status, fact).map_err(|error| invalid_record(index, error.code()))?;
        let list = self.late_facts.entry(fact.run_id.clone()).or_default();
        // 同一事实重复到达不重复追加（幂等），但不丢弃任何**不同**的事实。
        if !list.contains(fact) {
            list.push(fact.clone());
        }
        Ok(())
    }

    fn apply_usage_attempt(
        &mut self,
        index: usize,
        attempt: &UsageAttempt,
    ) -> Result<(), FactStoreError> {
        for (field, value) in [
            ("run_id", &attempt.run_id),
            ("logical_request_id", &attempt.logical_request_id),
            ("attempt_id", &attempt.attempt_id),
        ] {
            if value.trim().is_empty() {
                return Err(invalid_record(index, format!("用量事实缺 {field}")));
            }
        }
        // 身份是 `(run_id, logical_request_id, attempt_id)`：`attempt_id` 在同一个 run 内
        // 按逻辑请求重新计数，三者共同才能唯一确定一次网络尝试。
        let existing = self.find_usage_attempt(&attempt.run_id, attempt).cloned();
        match existing {
            Some(existing) if existing == *attempt => {}
            Some(_) => self.note_conflict(format!(
                "usage:{}:{}:{}",
                attempt.run_id, attempt.logical_request_id, attempt.attempt_id
            )),
            None => self
                .usage
                .entry(attempt.run_id.clone())
                .or_default()
                .push(attempt.clone()),
        }
        Ok(())
    }

    fn apply_message_submission(
        &mut self,
        index: usize,
        key: &MessageSubmissionKey,
        receipt_id: &str,
    ) -> Result<(), FactStoreError> {
        for (field, value) in [
            ("client_message_id", &key.client_message_id),
            ("scope", &key.scope),
            ("content_digest", &key.content_digest),
        ] {
            if value.trim().is_empty() {
                return Err(invalid_record(index, format!("提交事实缺 {field}")));
            }
        }
        match self.submissions.lookup(key) {
            // 首次登记。
            SubmissionDecision::New => {
                self.submissions.submit(key, receipt_id.to_string());
            }
            // 完全相同的重复登记：幂等，不重复追加（也不计冲突）。
            SubmissionDecision::ReturnExistingReceipt { .. } => {}
            // 同 ID 不同内容：保留第一条并计冲突（写入路径本该拒绝它）。
            SubmissionDecision::RejectConflict { .. } => self.note_conflict(format!(
                "submission:{}@{}",
                key.client_message_id, key.scope
            )),
        }
        Ok(())
    }

    /// 在某个 run 已登记的用量事实里按 `(逻辑请求, 尝试)` 查找该尝试的既有事实。
    fn find_usage_attempt(&self, run_id: &str, attempt: &UsageAttempt) -> Option<&UsageAttempt> {
        self.usage.get(run_id).and_then(|list| {
            list.iter().find(|existing| {
                existing.logical_request_id == attempt.logical_request_id
                    && existing.attempt_id == attempt.attempt_id
            })
        })
    }

    fn insert_attempt(&mut self, index: usize, attempt: &RunAttemptFact) -> Result<(), FactStoreError> {
        // attempt 登记是**运行级**事实（不属于某个 step/action），因此按 Turn scope 校验。
        validate_identity_for_record(index, &attempt.identity, RunIdentityScope::Turn)?;
        // 登记键是 `(run_id, request_attempt_id)`：没有 attempt id 的登记事实无法定位，
        // 这不是"不适用"而是"本应存在却缺失"。
        let Some(attempt_id) = attempt.identity.request_attempt_id.clone() else {
            return Err(invalid_record(
                index,
                "attempt 登记必须有 request_attempt_id：登记键的一半不能缺省",
            ));
        };
        let key = (attempt.identity.run_id.clone(), attempt_id);
        match self.attempts.get(&key).cloned() {
            Some(existing) if existing == *attempt => Ok(()),
            Some(_) => {
                self.note_conflict(format!("attempt:{}:{}", key.0, key.1));
                Ok(())
            }
            None => {
                self.attempts.insert(key, attempt.clone());
                Ok(())
            }
        }
    }

    fn note_conflict(&mut self, key: String) {
        *self.conflicts.entry(key).or_insert(0) += 1;
    }

    /// run 的控制终态。first-wins：返回**第一条**终态事实；`Unknown` 表示没有任何
    /// 终态事实（不是某个默认终态）。
    #[must_use]
    pub fn terminal_control(&self, run_id: &str) -> FactLookup<RunTerminalStatus> {
        match self.terminal.get(run_id).copied() {
            Some(status) => FactLookup::Known(status),
            None => FactLookup::Unknown,
        }
    }

    /// 某个 attempt 的登记事实。
    #[must_use]
    pub fn run_attempt(&self, run_id: &str, attempt_id: &str) -> FactLookup<&RunAttemptFact> {
        match self
            .attempts
            .get(&(run_id.to_string(), attempt_id.to_string()))
        {
            Some(attempt) => FactLookup::Known(attempt),
            None => FactLookup::Unknown,
        }
    }

    /// 某个 action 的回执历史（追加顺序）。空切片表示**没有回执事实**，
    /// 不代表"该动作没有效果"。
    #[must_use]
    pub fn action_receipt_history(&self, action_id: &str) -> &[ActionReceipt] {
        self.receipts.get(action_id).map_or(&[], Vec::as_slice)
    }

    /// 某个 action 最后一条回执事实。历史仍可用 `action_receipt_history` 取回，
    /// 不得只读最新一条而丢弃修订记录。
    #[must_use]
    pub fn latest_action_receipt(&self, action_id: &str) -> FactLookup<&ActionReceipt> {
        match self.receipts.get(action_id).and_then(|history| history.last()) {
            Some(receipt) => FactLookup::Known(receipt),
            None => FactLookup::Unknown,
        }
    }

    /// 某个 action 的证据元数据历史（追加顺序）。空切片表示**没有证据事实**，
    /// 不表示"依据不明"（要表达依据不明请写 `EvidenceBasis::Unknown` 的证据）。
    #[must_use]
    pub fn action_evidence_history(&self, action_id: &str) -> &[ActionEvidence] {
        self.evidences.get(action_id).map_or(&[], Vec::as_slice)
    }

    /// 某个 action 最后一条证据元数据。
    #[must_use]
    pub fn latest_action_evidence(&self, action_id: &str) -> FactLookup<&ActionEvidence> {
        match self.evidences.get(action_id).and_then(|history| history.last()) {
            Some(evidence) => FactLookup::Known(evidence),
            None => FactLookup::Unknown,
        }
    }

    /// 某个 action 的来源判定历史（读回口径与证据一致：缺失即 `Unknown`，不是"宿主动作"）。
    #[must_use]
    pub fn action_origin_history(&self, action_id: &str) -> &[ActionOrigin] {
        self.origins.get(action_id).map_or(&[], Vec::as_slice)
    }

    /// 某个 action 最后一条来源判定。
    #[must_use]
    pub fn latest_action_origin(&self, action_id: &str) -> FactLookup<&ActionOrigin> {
        match self.origins.get(action_id).and_then(|history| history.last()) {
            Some(origin) => FactLookup::Known(origin),
            None => FactLookup::Unknown,
        }
    }

    /// 某个 run 已登记的身份异常（追加顺序）。
    #[must_use]
    pub fn identity_anomalies(&self, run_id: &str) -> &[IdentityAnomaly] {
        self.identity_anomalies
            .get(run_id)
            .map_or(&[], Vec::as_slice)
    }

    /// 某个 action 上的身份异常。
    #[must_use]
    pub fn action_identity_anomalies(&self, action_id: &str) -> Vec<&IdentityAnomaly> {
        self.identity_anomalies
            .values()
            .flatten()
            .filter(|anomaly| anomaly.action_id.as_deref() == Some(action_id))
            .collect()
    }

    /// 该 run 是否**禁止继续输入**：存在任何 `must_stop_input` 的身份异常即为真。
    ///
    /// 这是"保留事实但不许继续动手"的执行点：异常不会因为后续事实到达而被清除
    /// （解除需要人类显式处理，本轮未定义该解除路径，见模块文档）。
    #[must_use]
    pub fn blocks_further_input(&self, run_id: &str) -> bool {
        self.identity_anomalies(run_id)
            .iter()
            .any(|anomaly| anomaly.must_stop_input)
    }

    /// 该 run 是否"本 scope 运行正常结束"。
    ///
    /// `None` = 没有终态事实（未知），**不是**成功；`Some(false)` 表示终态不是成功
    /// （失败 / 取消 / 中断 / 超时 / 阻塞都不算成功）。
    #[must_use]
    pub fn scope_succeeded(&self, run_id: &str) -> Option<bool> {
        self.terminal
            .get(run_id)
            .map(|status| status.is_scope_success())
    }

    /// 某个 run 已追加的迟到事实（追加顺序）。
    #[must_use]
    pub fn late_facts(&self, run_id: &str) -> &[LateFact] {
        self.late_facts.get(run_id).map_or(&[], Vec::as_slice)
    }

    /// 某个 run 已登记的用量事实（追加顺序）。
    #[must_use]
    pub fn usage_attempts(&self, run_id: &str) -> &[UsageAttempt] {
        self.usage.get(run_id).map_or(&[], Vec::as_slice)
    }

    /// 某个 run 的用量汇总。没有任何用量事实时 `usage_is_complete` 为 `false`、
    /// `billable_provider_tokens()` 为 `None`——缺 usage 是 unknown，不是 0。
    #[must_use]
    pub fn run_usage_summary(&self, run_id: &str) -> UsageFactSummary {
        let attempts = self.usage_attempts(run_id);
        let mut ledger = UsageLedger::new();
        for attempt in attempts {
            ledger.record(attempt.clone());
        }
        let network_attempts = ledger.network_attempts();
        UsageFactSummary {
            network_attempts,
            logical_requests: ledger.logical_requests(),
            unknown_usage_attempts: ledger.unknown_usage_attempts(),
            known_provider_tokens: ledger.provider_tokens(),
            estimated_tokens: ledger.estimated_tokens(),
            // "没有尝试"是**未知**，不得读成"完整且为 0"。
            usage_is_complete: network_attempts > 0 && ledger.usage_is_complete(),
        }
    }

    /// 某个提交槽位已登记的记录。`Unknown` 表示从未提交过。
    #[must_use]
    pub fn submission(&self, key: &MessageSubmissionKey) -> FactLookup<&SubmissionRecord> {
        match self.submissions.record(key) {
            Some(record) => FactLookup::Known(record),
            None => FactLookup::Unknown,
        }
    }

    /// 某个父 run 下已登记的恢复事实（追加顺序）。
    #[must_use]
    pub fn recoveries(&self, parent_run_id: &str) -> &[RecoveryFact] {
        self.recoveries
            .get(parent_run_id)
            .map_or(&[], Vec::as_slice)
    }

    /// 某一次具体恢复的事实（该身份上的**第一条**决定；被拒绝的决定也算）。
    #[must_use]
    pub fn recovery(
        &self,
        parent_run_id: &str,
        parent_attempt_id: &str,
        attempt_id: &str,
    ) -> FactLookup<&RecoveryFact> {
        let found = self.recoveries(parent_run_id).iter().find(|fact| {
            fact.parent_attempt_id == parent_attempt_id && fact.attempt_id == attempt_id
        });
        match found {
            Some(fact) => FactLookup::Known(fact),
            None => FactLookup::Unknown,
        }
    }

    /// 某个 attempt 身份上**已授予**的恢复事实。
    ///
    /// 与 `recovery` 的区别很重要：被拒绝的决定**不占用** attempt 身份——它没有创建
    /// attempt，所以收紧预算后换同一次恢复再来，仍然应当被允许（历史里的拒绝决定保留）。
    #[must_use]
    pub fn granted_recovery(
        &self,
        parent_run_id: &str,
        parent_attempt_id: &str,
        attempt_id: &str,
    ) -> FactLookup<&RecoveryFact> {
        let found = self.recoveries(parent_run_id).iter().find(|fact| {
            fact.parent_attempt_id == parent_attempt_id
                && fact.attempt_id == attempt_id
                && fact.is_granted()
        });
        match found {
            Some(fact) => FactLookup::Known(fact),
            None => FactLookup::Unknown,
        }
    }

    /// 该 run 是否已被本存储登记过（终态事实或 attempt 登记任一存在）。
    #[must_use]
    pub fn run_is_known(&self, run_id: &str) -> bool {
        self.terminal.contains_key(run_id)
            || self
                .attempts
                .keys()
                .any(|(attempt_run_id, _)| attempt_run_id == run_id)
    }

    /// 重放时发现的结论冲突（键 -> 次数）。正常情况下为空；非空表示日志被外部
    /// 改写或来自违约写入者，读回一侧**保持第一条结论不变**（I4）。
    #[must_use]
    pub fn conflicts(&self) -> &BTreeMap<String, u32> {
        &self.conflicts
    }
}

// ---------------------------------------------------------------------------
// 实现体：追加日志之上的事实存储
// ---------------------------------------------------------------------------

/// 事实存储的唯一实现体：把规则判定放在内存投影上，把事实落在可插拔的追加日志里。
///
/// 所有判定都复用既有纯逻辑契约；本类型只负责"先判定、再落盘、最后提交内存"
/// 的顺序，使**内存永远不会领先于磁盘**（落盘失败时状态不变）。
pub struct AppendOnlyFactStore<B: FactLogBackend> {
    log: B,
    records: Vec<FactLogRecord>,
    snapshot: FactSnapshot,
}

impl<B: FactLogBackend> std::fmt::Debug for AppendOnlyFactStore<B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppendOnlyFactStore")
            .field("appended_records", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl<B: FactLogBackend> AppendOnlyFactStore<B> {
    /// 打开（或创建）日志并重放出现状。日志不存在等同于"还没有任何事实"。
    pub fn open(mut log: B) -> Result<Self, FactStoreError> {
        let records = log.read_all()?;
        let snapshot = FactSnapshot::replay(&records)?;
        Ok(Self {
            log,
            records,
            snapshot,
        })
    }

    /// 已追加的日志记录数（诊断用）。注意一条恢复决定 = 一条记录。
    #[must_use]
    pub fn appended_records(&self) -> usize {
        self.records.len()
    }

    /// 先验证（内存中重放候选序列），再落盘，最后提交内存。
    /// 任一步失败都不会让内存投影领先于磁盘。
    fn append_and_replay(&mut self, record: &FactLogRecord) -> Result<(), FactStoreError> {
        let mut records = self.records.clone();
        records.push(record.clone());
        let snapshot = FactSnapshot::replay(&records)?;
        self.log.append(record)?;
        self.records = records;
        self.snapshot = snapshot;
        Ok(())
    }
}

impl AppendOnlyFactStore<InMemoryFactLog> {
    /// 只存在于进程内的事实存储（测试与"无需跨进程对账"的场景）。
    #[must_use]
    pub fn in_memory() -> Self {
        Self::open(InMemoryFactLog::new()).expect("内存事实日志不可能读失败")
    }
}

impl<B: FactLogBackend> FactStore for AppendOnlyFactStore<B> {
    fn record_terminal_control(
        &mut self,
        identity: &RunIdentity,
        status: RunTerminalStatus,
    ) -> Result<TerminalControlDecision, FactStoreError> {
        // 控制终态是运行级事实：按 Turn scope 校验（作用域由事实类型决定，不由调用方自选）。
        identity
            .validate_for(RunIdentityScope::Turn)
            .map_err(FactStoreError::Contract)?;
        match self.snapshot.terminal_control(&identity.run_id) {
            FactLookup::Known(existing) if existing == status => {
                Ok(TerminalControlDecision::AlreadyTerminal(existing))
            }
            FactLookup::Known(existing) => Ok(TerminalControlDecision::ConflictingTerminal(existing)),
            FactLookup::Unknown => {
                self.append_and_replay(&FactLogRecord::TerminalControl {
                    identity: identity.clone(),
                    status,
                })?;
                Ok(TerminalControlDecision::Recorded(status))
            }
        }
    }

    fn record_host_outcome(
        &mut self,
        identity: &RunIdentity,
        outcome: &HostRunOutcome,
    ) -> Result<TerminalControlDecision, FactStoreError> {
        outcome.validate().map_err(FactStoreError::Contract)?;
        // 非终态 → 没有终态可写：**不写终态事实**（不得用默认终态凑一条）。
        let Some(status) = outcome.terminal_status() else {
            return Ok(TerminalControlDecision::NotTerminal);
        };
        // 取消只有"宿主确认 + 写明来源"才会走到 Cancelled；
        // 原因不明 / 异常中断落到 Interrupted，不冒充取消。
        self.record_terminal_control(identity, status)
    }

    fn open_attempt(
        &mut self,
        identity: &RunIdentity,
        budget: RunBudget,
    ) -> Result<AttemptDecision, FactStoreError> {
        identity.validate_for(RunIdentityScope::Turn).map_err(FactStoreError::Contract)?;
        let attempt_id = request_attempt_id_of(identity)?;
        let attempt = RunAttemptFact {
            identity: identity.clone(),
            budget,
            recovery_of: None,
        };
        match self
            .snapshot
            .run_attempt(&identity.run_id, &attempt_id)
            .known()
        {
            Some(existing) if *existing == attempt => Ok(AttemptDecision::AlreadyOpened),
            Some(existing) => Ok(AttemptDecision::ConflictingAttempt {
                existing: existing.clone(),
            }),
            None => {
                self.append_and_replay(&FactLogRecord::AttemptOpened {
                    attempt: attempt.clone(),
                })?;
                Ok(AttemptDecision::Opened)
            }
        }
    }

    fn open_recovery_attempt(
        &mut self,
        identity: &RunIdentity,
        parent_attempt_id: &str,
        remaining_budget: RunBudget,
    ) -> Result<RecoveryDecision, FactStoreError> {
        identity.validate_for(RunIdentityScope::Turn).map_err(FactStoreError::Contract)?;
        let parent_run_id = identity.run_id.clone();
        let attempt_id = request_attempt_id_of(identity)?;
        if parent_attempt_id.trim().is_empty() {
            return Err(FactStoreError::UnknownParentAttempt {
                run_id: parent_run_id,
                attempt_id: parent_attempt_id.to_string(),
            });
        }
        // 父预算必须来自**已登记的事实**：查不到就拒绝，绝不假设一个父预算
        // （否则"不得靠换 ID 放宽预算"就形同虚设）。
        let Some(parent_budget) = self
            .snapshot
            .run_attempt(&parent_run_id, parent_attempt_id)
            .known()
            .map(|parent| parent.budget)
        else {
            return Err(FactStoreError::UnknownParentAttempt {
                run_id: parent_run_id,
                attempt_id: parent_attempt_id.to_string(),
            });
        };

        let derived = RecoveryAttempt::derive(
            parent_attempt_id,
            &parent_run_id,
            attempt_id.clone(),
            remaining_budget,
            parent_budget,
        );

        // 已被授予的同一身份：幂等，或说明该 attempt 身份已经被占用。
        let granted = self
            .snapshot
            .granted_recovery(&parent_run_id, parent_attempt_id, &attempt_id)
            .known()
            .cloned();

        let derived = match derived {
            Ok(derived) => derived,
            Err(error) => {
                // 幂等：完全相同的拒绝（同请求、同结论）不重复记；效果一致
                // （都没有 attempt、都没有复活 run）。
                let already_refused_identically =
                    self.snapshot.recoveries(&parent_run_id).iter().any(|fact| {
                        fact.parent_attempt_id == parent_attempt_id
                            && fact.attempt_id == attempt_id
                            && fact.requested_budget == remaining_budget
                            && fact.refusal() == Some(&error)
                    });
                if already_refused_identically {
                    return Ok(RecoveryDecision::Refused(error));
                }
                self.append_and_replay(&FactLogRecord::RecoveryDecided {
                    fact: RecoveryFact {
                        parent_run_id,
                        parent_attempt_id: parent_attempt_id.to_string(),
                        attempt_id,
                        requested_budget: remaining_budget,
                        outcome: RecoveryFactOutcome::Refused { error: error.clone() },
                    },
                })?;
                return Ok(RecoveryDecision::Refused(error));
            }
        };

        if let Some(granted) = granted {
            // 同一次恢复重复到达：幂等。
            if granted.effective_budget() == Some(derived.remaining_budget) {
                return Ok(RecoveryDecision::AlreadyRecorded {
                    attempt_id: derived.attempt_id,
                });
            }
            // 该 attempt 身份已被占用且条件不同：这不是"新 attempt"，按身份复用拒绝。
            return Ok(RecoveryDecision::Refused(RecoveryError::SameAttemptId));
        }

        let attempt = RunAttemptFact {
            identity: identity.clone(),
            budget: derived.remaining_budget,
            recovery_of: Some(RecoveryLink {
                parent_run_id: derived.parent_run_id.clone(),
                parent_attempt_id: derived.parent_attempt_id.clone(),
            }),
        };
        self.append_and_replay(&FactLogRecord::RecoveryDecided {
            fact: RecoveryFact {
                parent_run_id: derived.parent_run_id.clone(),
                parent_attempt_id: derived.parent_attempt_id.clone(),
                attempt_id: derived.attempt_id.clone(),
                requested_budget: derived.remaining_budget,
                outcome: RecoveryFactOutcome::Granted { attempt },
            },
        })?;
        Ok(RecoveryDecision::GrantedNewAttempt { attempt: derived })
    }

    fn record_action_receipt(
        &mut self,
        identity: &RunIdentity,
        receipt: &ActionReceipt,
    ) -> Result<ReceiptDecision, FactStoreError> {
        self.record_action_fact(&ActionFact::new(identity.clone(), receipt.clone()))
    }

    fn record_action_fact(&mut self, fact: &ActionFact) -> Result<ReceiptDecision, FactStoreError> {
        fact.receipt.validate().map_err(FactStoreError::Contract)?;
        if let Some(evidence) = &fact.evidence {
            // 证据不是展示标签：自洽性 + 与回执的一致性都要过，否则拒绝写入。
            evidence.validate().map_err(FactStoreError::Contract)?;
            evidence
                .validate_against(&fact.receipt)
                .map_err(FactStoreError::Contract)?;
        }
        if let Some(origin) = &fact.origin {
            // 这里只做**结构层**校验。可信关联层（核对真实父对象、防止"只有模型输入声称
            // 用户点击"这类伪造）需要 `ActionOriginAuthority`，由调用方在写入前用
            // `admit_action_origin` 完成——本方法没有该上下文，**不得**在此假装做过第二层。
            origin.validate_structure().map_err(FactStoreError::Contract)?;
        }
        let admission = admit_action_fact(&fact.identity, &fact.receipt).map_err(|error| {
            // 身份与回执的 action_id 不一致是身份冲突，其余按契约错误透出。
            if error.code == "identity_conflict" {
                FactStoreError::IdentityConflict {
                    field: "action_id",
                    message: error.message,
                }
            } else {
                FactStoreError::Contract(error)
            }
        })?;

        // 停止后续输入的执行点：本 run 已有未解除的身份异常时，
        // **声明输入资格**的新事实一律拒绝（输入前拒绝类事实不受影响，因为它不动手）。
        if admission.gains_input_qualification()
            && self.snapshot.blocks_further_input(&fact.identity.run_id)
        {
            return Err(FactStoreError::InputBlockedByIdentityAnomaly {
                run_id: fact.identity.run_id.clone(),
            });
        }
        // 父子关联：显式声明的父运行必须已登记过（查不到就是"挂到不存在的运行上"）。
        if let Some(parent) = fact.identity.parent_run() {
            if !self.snapshot.run_is_known(&parent.parent_run_id) {
                return Err(FactStoreError::UnknownParentRun {
                    run_id: parent.parent_run_id.clone(),
                });
            }
        }

        let action_id = fact.receipt.action_id.clone();
        let previous = self
            .snapshot
            .latest_action_receipt(&action_id)
            .known()
            .cloned();
        let receipt_is_new = previous.as_ref() != Some(&fact.receipt);
        let anomaly = admission.anomaly().cloned();
        let evidence_is_new = fact.evidence.as_ref().is_some_and(|evidence| {
            !self
                .snapshot
                .action_evidence_history(&action_id)
                .contains(evidence)
        });
        let anomaly_is_new = anomaly.as_ref().is_some_and(|anomaly| {
            !self
                .snapshot
                .identity_anomalies(&anomaly.run_id)
                .contains(anomaly)
        });
        let origin_is_new = fact.origin.as_ref().is_some_and(|origin| {
            !self
                .snapshot
                .action_origin_history(&action_id)
                .contains(origin)
        });

        let fact_is_new = receipt_is_new || evidence_is_new || anomaly_is_new || origin_is_new;
        if fact_is_new {
            self.append_and_replay(&FactLogRecord::ActionReceipt {
                identity: fact.identity.clone(),
                receipt: fact.receipt.clone(),
                identity_anomaly: anomaly.clone(),
                evidence: fact.evidence.clone(),
                origin: fact.origin.clone(),
            })?;
        }

        // 判决优先级：异常保留（必须停止输入）> 输入前拒绝（不获得输入资格）> 常规记录。
        // 前两者描述的是**这条事实的性质**，因此重复到达时判决不变（异常/拒绝不会因为
        // 幂等而消失）；后两者描述的是这次写入是否产生了新的回执事实。
        if let Some(anomaly) = anomaly {
            return Ok(ReceiptDecision::RecordedWithIdentityAnomaly { anomaly });
        }
        if let ActionIdentityAdmission::AdmittedPreInputRejection { absent_dimensions } = &admission
        {
            return Ok(ReceiptDecision::RecordedPreInputRejection {
                absent_dimensions: absent_dimensions.clone(),
            });
        }
        if !fact_is_new {
            return Ok(ReceiptDecision::AlreadyRecorded);
        }
        match previous {
            Some(previous) if receipt_is_new => Ok(ReceiptDecision::Revision { previous }),
            _ => Ok(ReceiptDecision::Recorded),
        }
    }

    fn append_late_fact(&mut self, fact: &LateFact) -> Result<LateFactAppend, FactStoreError> {
        // 迟到事实是"终态之后到达的事实"：没有终态事实就没有可挂的结论，
        // 缺失必须是 unknown，不能默认某个终态来"凑"一次成功追加。
        let Some(status) = self
            .snapshot
            .terminal_control(&fact.run_id)
            .known()
        else {
            return Err(FactStoreError::UnknownRunTerminal {
                run_id: fact.run_id.clone(),
            });
        };
        let append = append_late_fact(status, fact).map_err(FactStoreError::LateFact)?;
        if self.snapshot.late_facts(&fact.run_id).contains(fact) {
            // 同一事实重复到达：幂等，不重复追加。
            return Ok(append);
        }
        self.append_and_replay(&FactLogRecord::LateFact { fact: fact.clone() })?;
        Ok(append)
    }

    fn record_usage_attempt(
        &mut self,
        attempt: &UsageAttempt,
    ) -> Result<UsageAttemptDecision, FactStoreError> {
        for (field, value) in [
            ("run_id", &attempt.run_id),
            ("logical_request_id", &attempt.logical_request_id),
            ("attempt_id", &attempt.attempt_id),
        ] {
            if value.trim().is_empty() {
                return Err(FactStoreError::EmptyField { field });
            }
        }
        // 身份是 `(run_id, logical_request_id, attempt_id)`：`attempt_id` 在同一个 run 内
        // 按逻辑请求重新计数，因此三者共同才能唯一确定一次网络尝试。
        let found = self
            .snapshot
            .usage_attempts(&attempt.run_id)
            .iter()
            .find(|existing| {
                existing.logical_request_id == attempt.logical_request_id
                    && existing.attempt_id == attempt.attempt_id
            })
            .cloned();
        match found {
            Some(existing) if existing == *attempt => Ok(UsageAttemptDecision::AlreadyRecorded),
            Some(existing) => Ok(UsageAttemptDecision::ConflictingAttempt { existing }),
            None => {
                self.append_and_replay(&FactLogRecord::UsageAttempt {
                    attempt: attempt.clone(),
                })?;
                Ok(UsageAttemptDecision::Recorded)
            }
        }
    }

    fn submit_message(
        &mut self,
        key: &MessageSubmissionKey,
        receipt_id: &str,
    ) -> Result<SubmissionDecision, FactStoreError> {
        for (field, value) in [
            ("client_message_id", &key.client_message_id),
            ("scope", &key.scope),
            ("content_digest", &key.content_digest),
        ] {
            if value.trim().is_empty() {
                return Err(FactStoreError::EmptyField { field });
            }
        }
        if receipt_id.trim().is_empty() {
            return Err(FactStoreError::EmptyField {
                field: "receipt_id",
            });
        }
        let decision = self.snapshot.submissions.lookup(key);
        if decision == SubmissionDecision::New {
            // 只有首次提交才登记事实，因此只有它才允许调用方继续执行（I6）。
            self.append_and_replay(&FactLogRecord::MessageSubmission {
                key: key.clone(),
                receipt_id: receipt_id.to_string(),
            })?;
        }
        Ok(decision)
    }

    fn snapshot(&self) -> &FactSnapshot {
        &self.snapshot
    }
}

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

/// 事实存储的错误。**任何不确定性都表现为错误或 `Unknown`，绝不静默降级。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactStoreError {
    /// 后端 `I/O` 失败。
    Io {
        path: Option<PathBuf>,
        message: String,
    },
    /// 记录编码失败。
    Encode { message: String },
    /// 日志中某一行无法解析（I8：不得跳过）。
    CorruptRecord {
        path: Option<PathBuf>,
        /// 1 起的行号。
        line: usize,
        message: String,
    },
    /// 记录在语义上违反契约（I8：不得静默接受）。
    InvalidRecord { index: usize, message: String },
    /// 缺少必需的非空字段。
    EmptyField { field: &'static str },
    /// 身份字段冲突（例如回执与身份里的 `action_id` 不一致）。
    IdentityConflict {
        field: &'static str,
        message: String,
    },
    /// 追加迟到事实时该 run 没有任何终态事实：缺失是 unknown，不能靠默认终态补齐。
    UnknownRunTerminal { run_id: String },
    /// 恢复找不到父 attempt 的**已登记预算**：无法核对结转，因此拒绝而不是假设。
    UnknownParentAttempt { run_id: String, attempt_id: String },
    /// 身份显式声明的父运行在本存储里查不到登记：拒绝，不得把事实挂到不存在的运行上。
    UnknownParentRun { run_id: String },
    /// 该 run 存在未解除的身份异常（已经开始输入却缺身份）：**停止后续输入**，
    /// 声明输入资格的新动作事实一律被拒。
    InputBlockedByIdentityAnomaly { run_id: String },
    /// 既有契约的校验失败（`RunIdentity` / `ActionReceipt`）。
    Contract(RunContractError),
    /// 迟到事实契约的拒绝。
    LateFact(LateFactError),
}

impl FactStoreError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "fact_store_io",
            Self::Encode { .. } => "fact_store_encode",
            Self::CorruptRecord { .. } => "fact_store_corrupt_record",
            Self::InvalidRecord { .. } => "fact_store_invalid_record",
            Self::EmptyField { .. } => "fact_store_empty_field",
            Self::IdentityConflict { .. } => "fact_store_identity_conflict",
            Self::UnknownRunTerminal { .. } => "fact_store_unknown_run_terminal",
            Self::UnknownParentAttempt { .. } => "fact_store_unknown_parent_attempt",
            Self::UnknownParentRun { .. } => "fact_store_unknown_parent_run",
            Self::InputBlockedByIdentityAnomaly { .. } => {
                "fact_store_input_blocked_by_identity_anomaly"
            }
            // 既有契约的码原样透出；其余归为通用违约码，具体信息在 `Display` 里，
            // 不在这里编造。
            Self::Contract(error) => match error.code.as_str() {
                "invalid_identity" => "invalid_identity",
                "incomplete_identity" => "incomplete_identity",
                "placeholder_identity_value" => "placeholder_identity_value",
                "identity_dimension_not_applicable" => "identity_dimension_not_applicable",
                "identity_scope_undeclared" => "identity_scope_undeclared",
                "identity_scope_mismatch" => "identity_scope_mismatch",
                "identity_schema_version_missing" => "identity_schema_version_missing",
                "unsupported_identity_schema_version" => "unsupported_identity_schema_version",
                "identity_context_mismatch" => "identity_context_mismatch",
                "identity_conflict" => "identity_conflict",
                "contradictory_action_receipt" => "contradictory_action_receipt",
                "invalid_action_evidence" => "invalid_action_evidence",
                "placeholder_action_evidence" => "placeholder_action_evidence",
                "incomplete_host_outcome" => "incomplete_host_outcome",
                _ => "fact_store_contract_violation",
            },
            Self::LateFact(error) => error.code(),
        }
    }
}

impl Display for FactStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io {
                path,
                message,
            } => match path {
                Some(path) => write!(f, "事实日志 I/O 失败 {}：{message}", path.display()),
                None => write!(f, "事实日志 I/O 失败：{message}"),
            },
            Self::Encode { message } => write!(f, "事实记录编码失败：{message}"),
            Self::CorruptRecord {
                path,
                line,
                message,
            } => match path {
                Some(path) => write!(
                    f,
                    "事实日志第 {line} 行无法解析（{}）：{message}",
                    path.display()
                ),
                None => write!(f, "事实日志第 {line} 行无法解析：{message}"),
            },
            Self::InvalidRecord { index, message } => {
                write!(f, "事实记录 #{index} 违反契约：{message}")
            }
            Self::EmptyField { field } => write!(f, "事实缺少必需字段：{field}"),
            Self::IdentityConflict { field, message } => {
                write!(f, "事实身份冲突（{field}）：{message}")
            }
            Self::UnknownRunTerminal { run_id } => write!(
                f,
                "run {run_id} 没有终态事实：缺失是 unknown，不得默认某个终态"
            ),
            Self::UnknownParentAttempt { run_id, attempt_id } => write!(
                f,
                "父 attempt {run_id}/{attempt_id} 没有已登记预算：无法核对结转，拒绝恢复"
            ),
            Self::UnknownParentRun { run_id } => write!(
                f,
                "父运行 {run_id} 在本存储没有登记：拒绝把事实挂到不存在的运行上"
            ),
            Self::InputBlockedByIdentityAnomaly { run_id } => write!(
                f,
                "run {run_id} 存在未解除的身份异常（已开始输入却缺身份）：停止后续输入"
            ),
            Self::Contract(error) => write!(f, "{}：{}", error.code, error.message),
            Self::LateFact(error) => write!(f, "迟到事实被拒绝：{}", error.code()),
        }
    }
}

impl std::error::Error for FactStoreError {}

fn invalid_record(index: usize, message: impl Into<String>) -> FactStoreError {
    FactStoreError::InvalidRecord {
        index,
        message: message.into(),
    }
}

/// attempt 登记类事实的登记键：`request_attempt_id` 必须存在。
///
/// 它在 Turn scope 下是"缺省维度"（普通轮次级事实没有它），但**attempt 登记事实**的
/// 身份就是这个 attempt，缺了不是"不适用"而是"本应存在却缺失"。
fn request_attempt_id_of(identity: &RunIdentity) -> Result<String, FactStoreError> {
    identity.request_attempt_id.clone().ok_or_else(|| {
        FactStoreError::Contract(RunContractError::incomplete_identity(
            "request_attempt_id（attempt 登记事实的登记键）",
        ))
    })
}

/// 按**记录所属事实类型**校验身份（写回读回共用）。
///
/// - 新记录：scope 必须与事实类型一致（控制终态 / attempt 登记 = `Turn`），
///   再按该 scope 校验维度；
/// - 旧记录（缺 scope）：按**旧版严格规则**解释（十维全必填），**不得**默认视为 `Turn`
///   ——否则"缺 scope"会变成一次静默的规则放松。
///
/// 维度校验本身交给 `RunIdentity::validate_persisted()`：它按记录**自身**的口径分流
/// （声明了 scope 就按 scope，缺 scope 就按旧严格规则），因此读回一侧只有这一份规则。
fn validate_identity_for_record(
    index: usize,
    identity: &RunIdentity,
    expected_scope: RunIdentityScope,
) -> Result<(), FactStoreError> {
    if let EffectiveRunIdentityScope::Declared(scope) = identity.resolved_scope() {
        if scope != expected_scope {
            return Err(invalid_record(
                index,
                format!(
                    "事实记录的作用域与身份声明的 scope 不一致（事实={}，身份={}）：scope 由宿主按事实类型决定",
                    expected_scope.as_str(),
                    scope.as_str()
                ),
            ));
        }
    }
    identity
        .validate_persisted()
        .map_err(|error| invalid_record(index, error.message))
}

#[cfg(test)]
mod tests {
    use super::{
        latest_legacy_cu_run_convergence, reconcile_legacy_cu_run_convergence, ActionFact,
        AppendOnlyFactStore, AttemptDecision, FactLogBackend, FactLookup, FactStore,
        FactStoreError, InMemoryFactLog, JsonlFactLog, LegacyCuRunConvergenceFact,
        LegacyCuRunConvergenceRefusal, LegacyCuRunConvergenceRuleDecision, LegacyCuRunObservedState,
        ReceiptDecision, RecoveryDecision, RecoveryFactOutcome, RecoveryLink,
        TerminalControlDecision, UsageAttemptDecision,
    };
    use crate::late_facts::{LateFact, LateFactKind};
    use crate::recovery::RecoveryError;
    use crate::action_evidence::{
        ActionEvidence, ActionSurface, EvidenceBasis, EvidenceReference, EvidenceReferenceKind,
    };
    use crate::run_contract::{
        ActionReceipt, CancelOrigin, CurrentResourceCandidateBasis, CurrentResourceSafetyCheck,
        EffectStatus, GoalVerdict, HostRunOutcome, InputDelivery, InputOwnerEpoch,
        InputReleaseStatus, LegacyCuRunConvergenceDecision, LegacyCuRunConvergenceEvidence,
        LegacyCuRunMissingDimensions, LegacyCuRunOriginalFacts, LegacyCuRunRecoveryOperator,
        LegacyCuRunSideEffectLimits, LegacyCuRunSubject, LegacyResourceScope, LegacyRunControlState,
        LegacyRunHistoricalOutcome, LegacyRunInputResourceState, ParentRunLink, PartialObservation,
        RunBudget, RunIdentity, RunIdentityScope, RunParentRelation, RunScopeContext,
        RunTerminalStatus,
    };
    use crate::submission_dedup::{
        MessageSubmissionKey, SubmissionDecision, SubmissionRecord,
    };
    use crate::usage::{ReportedUsage, UsageAttempt, UsageAttemptOutcome};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_log_path(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!(
                "runtime-fact-store-{}-{nanos}-{unique}",
                std::process::id()
            ))
            .join(format!("{name}.jsonl"))
    }

    fn memory_store() -> AppendOnlyFactStore<InMemoryFactLog> {
        AppendOnlyFactStore::in_memory()
    }

    fn context() -> RunScopeContext {
        RunScopeContext::new("workspace-1", "room-1", "session-1", "turn-public-1")
    }

    /// 运行级（`Turn` scope）身份：动作维度缺省，只带 attempt 登记的键。
    fn run_identity(run_id: &str, attempt_id: &str) -> RunIdentity {
        context()
            .turn_fact(run_id)
            .with_request_attempt_id(attempt_id)
    }

    /// step/action 级身份：动作维度齐全 + 真实输入所有权 epoch。
    fn action_identity(run_id: &str, attempt_id: &str, action_id: &str) -> RunIdentity {
        context()
            .step_action_fact(run_id, "step-1", attempt_id, action_id)
            .with_tool_call_id("tool-call-1")
            .with_owner_epoch(InputOwnerEpoch::from_host_counter(1).expect("host epoch"))
    }

    fn budget(deadline: u64, actions: u32, replans: u32, attempts: u32) -> RunBudget {
        RunBudget {
            deadline_unix_ms: deadline,
            max_actions: actions,
            max_replans: replans,
            max_request_attempts: attempts,
        }
    }

    /// 三个可选字段都**未知**（`None`）的回执：未知必须原样往返，不得被补成 `false`/0。
    fn receipt_with_unknown_fields(action_id: &str) -> ActionReceipt {
        ActionReceipt {
            action_id: action_id.to_string(),
            input_delivery: InputDelivery::MayHaveBeenSent,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::Unknown,
        }
    }

    fn late_fact(run_id: &str, source: &str, received_at: u64) -> LateFact {
        LateFact {
            run_id: run_id.to_string(),
            source: source.to_string(),
            received_at_unix_ms: received_at,
            kind: LateFactKind::Usage,
            observed_at_unix_ms: None,
        }
    }

    fn usage_attempt(
        run_id: &str,
        logical: &str,
        attempt_id: &str,
        provider_usage: ReportedUsage,
    ) -> UsageAttempt {
        UsageAttempt {
            run_id: run_id.to_string(),
            logical_request_id: logical.to_string(),
            attempt_id: attempt_id.to_string(),
            outcome: UsageAttemptOutcome::Completed,
            provider_usage,
            estimated_usage: None,
            price_version: None,
        }
    }

    fn known_usage(input: u32, output: u32) -> ReportedUsage {
        ReportedUsage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: Some(0),
        }
    }

    /// (a) 往返一致性：六类事实写进**文件**日志，重新打开后逐项一致，
    /// 且重开前的投影与重开后的投影完全相等（未知字段也不得被补齐）。
    #[test]
    fn every_fact_kind_round_trips_through_the_append_log() {
        let path = temp_log_path("round-trip");
        let root = path.parent().expect("temp root").to_path_buf();
        let attempt = run_identity("run-1", "attempt-0");
        let action = action_identity("run-1", "attempt-0", "action-1");
        let receipt = receipt_with_unknown_fields("action-1");
        let usage = usage_attempt("run-1", "logical-1", "attempt-0", ReportedUsage::unknown());
        let fact = late_fact("run-1", "web-console", 2_000);
        let key = MessageSubmissionKey::new("msg-1", "room-1", "digest-a");

        let before = {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            assert_eq!(
                store.open_attempt(&attempt, budget(10_000, 4, 2, 3)),
                Ok(AttemptDecision::Opened)
            );
            assert_eq!(
                store.record_action_receipt(&action, &receipt),
                Ok(ReceiptDecision::Recorded)
            );
            assert_eq!(
                store.record_usage_attempt(&usage),
                Ok(UsageAttemptDecision::Recorded)
            );
            assert_eq!(
                store.record_terminal_control(&attempt, RunTerminalStatus::Failed),
                Ok(TerminalControlDecision::Recorded(RunTerminalStatus::Failed))
            );
            assert!(store.append_late_fact(&fact).is_ok());
            assert_eq!(
                store.submit_message(&key, "receipt-1"),
                Ok(SubmissionDecision::New)
            );
            assert_eq!(store.appended_records(), 6, "六类事实各一条记录");
            store.snapshot().clone()
        };

        // 重新打开：读回的必须与写入后读到的逐项相同。
        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        let after = reopened.snapshot().clone();
        assert_eq!(before, after, "读回的事实必须与写入后的投影完全一致");

        let snapshot = reopened.snapshot();
        assert_eq!(
            snapshot.terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Failed)
        );
        assert_eq!(
            snapshot.latest_action_receipt("action-1"),
            FactLookup::Known(&receipt)
        );
        assert_eq!(
            snapshot.run_attempt("run-1", "attempt-0").known(),
            Some(&crate::fact_store::RunAttemptFact {
                identity: attempt.clone(),
                budget: budget(10_000, 4, 2, 3),
                recovery_of: None,
            })
        );
        assert_eq!(snapshot.late_facts("run-1"), &[fact]);
        assert_eq!(snapshot.usage_attempts("run-1"), &[usage]);
        assert_eq!(
            snapshot.submission(&key),
            FactLookup::Known(&SubmissionRecord {
                content_digest: "digest-a".to_string(),
                receipt_id: "receipt-1".to_string(),
            })
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// (b) 缺 usage：必须是 unknown，**不得**写成 0，也不得给出账单数字。
    #[test]
    fn missing_usage_is_stored_as_unknown_and_never_billed_as_zero() {
        let path = temp_log_path("unknown-usage");
        let root = path.parent().expect("temp root").to_path_buf();
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            assert_eq!(
                store.record_usage_attempt(&usage_attempt(
                    "run-1",
                    "logical-1",
                    "attempt-0",
                    ReportedUsage::unknown()
                )),
                Ok(UsageAttemptDecision::Recorded)
            );
        }

        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        let snapshot = reopened.snapshot();
        let summary = snapshot.run_usage_summary("run-1");
        assert_eq!(summary.network_attempts, 1);
        assert_eq!(summary.unknown_usage_attempts, 1);
        assert!(!summary.usage_is_complete);
        assert_eq!(
            summary.billable_provider_tokens(),
            None,
            "缺 usage 必须表现为未知，绝不允许用 0 冒充账单数字"
        );

        // 逐维仍是 unknown：往返不得把 None 升级成 Some(0)。
        let stored = &snapshot.usage_attempts("run-1")[0];
        assert_eq!(stored.provider_usage, ReportedUsage::unknown());
        assert!(stored.provider_usage.has_unknown());
        assert_eq!(stored.provider_usage.input_tokens, None);
        assert_eq!(stored.price_version, None, "缺价格不得被补成某个版本");

        // 从来没有用量事实的 run 同样是未知，不是"零用量"。
        let never_seen = snapshot.run_usage_summary("run-never-seen");
        assert_eq!(never_seen.network_attempts, 0);
        assert!(!never_seen.usage_is_complete);
        assert_eq!(never_seen.billable_provider_tokens(), None);

        // 落盘的行里不得出现凭空造出的 0。
        let text = fs::read_to_string(&path).expect("read fact log");
        assert!(
            !text.contains("\"input_tokens\":0") && !text.contains("\"input_tokens\": 0"),
            "未知用量不得被写成 0：{text}"
        );
        assert!(text.contains("usage_attempt"), "用量事实应当真的落盘：{text}");

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// (b') 只有**每条**尝试的用量都完整时，汇总才允许出现在账单口径里。
    #[test]
    fn usage_is_billable_only_when_every_attempt_is_known() {
        let mut partial = memory_store();
        partial
            .record_usage_attempt(&usage_attempt(
                "run-1",
                "logical-1",
                "attempt-0",
                known_usage(100, 20),
            ))
            .expect("record known usage");
        assert_eq!(
            partial
                .snapshot()
                .run_usage_summary("run-1")
                .billable_provider_tokens()
                .map(|tokens| tokens.input_tokens),
            Some(100)
        );

        // 追加一条未知用量的尝试：已知量不被污染，但汇总不再可计入账单。
        partial
            .record_usage_attempt(&usage_attempt(
                "run-1",
                "logical-2",
                "attempt-0",
                ReportedUsage::unknown(),
            ))
            .expect("record unknown usage");
        let summary = partial.snapshot().run_usage_summary("run-1");
        assert_eq!(summary.network_attempts, 2);
        assert_eq!(summary.known_provider_tokens.input_tokens, 100);
        assert_eq!(summary.billable_provider_tokens(), None);
        // 迟到的未知轮不得抹掉已知量，也不得让它看起来完整。
        assert_eq!(summary.unknown_usage_attempts, 1);
    }

    /// (b'') 回执里的未知字段往返后仍是未知（不得被补成 `false` / 0）。
    #[test]
    fn unknown_receipt_fields_are_not_filled_with_defaults_on_read_back() {
        let path = temp_log_path("unknown-receipt");
        let root = path.parent().expect("temp root").to_path_buf();
        let action = action_identity("run-1", "attempt-0", "action-1");
        let receipt = receipt_with_unknown_fields("action-1");
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            assert_eq!(
                store.record_action_receipt(&action, &receipt),
                Ok(ReceiptDecision::Recorded)
            );
        }

        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        let stored = reopened
            .snapshot()
            .latest_action_receipt("action-1")
            .known()
            .expect("recorded receipt");
        assert_eq!(stored.partial, None);
        assert_eq!(stored.path_completed, None);
        assert_eq!(stored.confirmed_point_count, None);
        assert_eq!(stored.input_release, InputReleaseStatus::Unknown);
        assert_eq!(stored, &receipt);

        let text = fs::read_to_string(&path).expect("read fact log");
        assert!(
            !text.contains("\"partial\"") && !text.contains("\"confirmed_point_count\""),
            "未知字段不得被序列化成默认值：{text}"
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// (c) 幂等：同一提交两次只生效一次；同 ID 不同内容被拒且不登记。
    #[test]
    fn the_same_submission_is_recorded_once_and_returns_the_same_receipt() {
        let path = temp_log_path("submission-dedup");
        let root = path.parent().expect("temp root").to_path_buf();
        let key = MessageSubmissionKey::new("msg-1", "room-1", "digest-a");
        let changed = MessageSubmissionKey::new("msg-1", "room-1", "digest-b");
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            assert_eq!(
                store.submit_message(&key, "receipt-1"),
                Ok(SubmissionDecision::New)
            );
            // 同 ID 同内容再次到达（网络重传）：返回既有收据，**不产生第二次副作用**。
            assert_eq!(
                store.submit_message(&key, "receipt-2"),
                Ok(SubmissionDecision::ReturnExistingReceipt {
                    receipt_id: "receipt-1".to_string()
                })
            );
            // 同 ID 不同内容：拒绝，且不登记。
            assert_eq!(
                store.submit_message(&changed, "receipt-3"),
                Ok(SubmissionDecision::RejectConflict {
                    existing_digest: "digest-a".to_string()
                })
            );
            assert_eq!(store.appended_records(), 1, "只有首次提交才产生事实");
        }

        // 重启（重新打开日志）后仍然是"已登记过"，不会因为进程重启而重复执行。
        let mut reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        assert_eq!(reopened.appended_records(), 1);
        assert_eq!(
            reopened.submit_message(&key, "receipt-9"),
            Ok(SubmissionDecision::ReturnExistingReceipt {
                receipt_id: "receipt-1".to_string()
            })
        );
        assert_eq!(reopened.appended_records(), 1);
        assert_eq!(
            reopened.snapshot().submission(&key),
            FactLookup::Known(&SubmissionRecord {
                content_digest: "digest-a".to_string(),
                receipt_id: "receipt-1".to_string(),
            })
        );
        assert_eq!(
            reopened.snapshot().submission(&MessageSubmissionKey::new(
                "msg-2", "room-1", "digest-a"
            )),
            FactLookup::Unknown,
            "从未提交过的槽位是 unknown，不是某条默认记录"
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 终态 first-wins：相同终态幂等，不同终态被拒绝且不改写既有事实。
    #[test]
    fn terminal_control_is_first_wins_and_never_rewritten() {
        let mut store = memory_store();
        let attempt = run_identity("run-1", "attempt-0");
        assert_eq!(
            store.record_terminal_control(&attempt, RunTerminalStatus::Cancelled),
            Ok(TerminalControlDecision::Recorded(RunTerminalStatus::Cancelled))
        );
        assert_eq!(
            store.record_terminal_control(&attempt, RunTerminalStatus::Cancelled),
            Ok(TerminalControlDecision::AlreadyTerminal(
                RunTerminalStatus::Cancelled
            ))
        );
        assert_eq!(
            store.record_terminal_control(&attempt, RunTerminalStatus::Succeeded),
            Ok(TerminalControlDecision::ConflictingTerminal(
                RunTerminalStatus::Cancelled
            ))
        );
        assert_eq!(store.appended_records(), 1, "被拒绝的终态不得落成事实");
        assert_eq!(
            store.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Cancelled)
        );
        assert_eq!(
            store.snapshot().terminal_control("run-never-seen"),
            FactLookup::Unknown,
            "没有终态事实是 unknown，不是某个默认终态"
        );
    }

    /// (d) 迟到事实：追加后不改变既有结论，且缺终态时不被默认补齐。
    #[test]
    fn late_facts_are_appended_without_changing_the_recorded_conclusion() {
        let mut store = memory_store();
        let attempt = run_identity("run-1", "attempt-0");
        store
            .record_terminal_control(&attempt, RunTerminalStatus::Cancelled)
            .expect("record terminal");
        let fact = late_fact("run-1", "web-console", 2_000);

        let append = store.append_late_fact(&fact).expect("append late fact");
        assert_eq!(
            append.control_status,
            RunTerminalStatus::Cancelled,
            "迟到事实不得改写控制终态"
        );
        assert!(!append.revives_run(), "迟到事实不得复活运行");
        assert_eq!(
            store.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Cancelled)
        );
        assert_eq!(store.snapshot().late_facts("run-1"), std::slice::from_ref(&fact));

        // 同一事实重复到达：幂等，不重复追加。
        assert!(store.append_late_fact(&fact).is_ok());
        assert_eq!(store.snapshot().late_facts("run-1").len(), 1);

        // 迟到的用量事实同样只增不改。
        store
            .record_usage_attempt(&usage_attempt(
                "run-1",
                "logical-1",
                "attempt-0",
                known_usage(7, 3),
            ))
            .expect("record late usage");
        assert_eq!(
            store.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Cancelled)
        );
        assert_eq!(
            store
                .snapshot()
                .run_usage_summary("run-1")
                .known_provider_tokens
                .total_tokens(),
            10
        );

        // 缺来源/接收时间的迟到事实被契约拒绝，不会落成事实。
        let mut anonymous = fact.clone();
        anonymous.source = "   ".to_string();
        assert_eq!(
            store.append_late_fact(&anonymous),
            Err(FactStoreError::LateFact(
                crate::late_facts::LateFactError::MissingProvenance
            ))
        );
        assert_eq!(store.snapshot().late_facts("run-1").len(), 1);

        // 挂到没有终态记录的 run：拒绝，不得默认某个终态来"凑"成功。
        assert_eq!(
            store.append_late_fact(&late_fact("run-never-seen", "provider", 3_000)),
            Err(FactStoreError::UnknownRunTerminal {
                run_id: "run-never-seen".to_string()
            })
        );
    }

    /// (e) 恢复：只开新 attempt，旧 run 与旧 attempt 原样保留（不复活）。
    #[test]
    fn recovery_opens_a_new_attempt_and_never_revives_the_old_run() {
        let path = temp_log_path("recovery");
        let root = path.parent().expect("temp root").to_path_buf();
        let parent = run_identity("run-1", "attempt-0");
        let recovered = run_identity("run-1", "attempt-1");
        let parent_budget = budget(10_000, 4, 2, 3);
        let remaining = budget(9_000, 2, 1, 2);
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            store
                .open_attempt(&parent, parent_budget)
                .expect("open parent attempt");
            store
                .record_terminal_control(&parent, RunTerminalStatus::Failed)
                .expect("record terminal");

            let decision = store
                .open_recovery_attempt(&recovered, "attempt-0", remaining)
                .expect("recovery must be derivable");
            let RecoveryDecision::GrantedNewAttempt { attempt } = decision else {
                panic!("显式恢复应当被接受：{decision:?}");
            };
            assert!(attempt.has_new_identity());
            assert_eq!(attempt.parent_attempt_id, "attempt-0");
            assert_eq!(attempt.parent_run_id, "run-1");
            assert_eq!(attempt.remaining_budget, remaining);

            // 旧 run 的终态事实逐项未变（没有复活），旧的预算也没有被换 ID 重置。
            assert_eq!(
                store.snapshot().terminal_control("run-1"),
                FactLookup::Known(RunTerminalStatus::Failed)
            );
            assert_eq!(
                store
                    .snapshot()
                    .run_attempt("run-1", "attempt-0")
                    .known()
                    .expect("parent attempt")
                    .budget,
                parent_budget
            );
            // 新 attempt 带父关联登记。
            let recorded = store
                .snapshot()
                .run_attempt("run-1", "attempt-1")
                .known()
                .expect("recovered attempt")
                .clone();
            assert_eq!(
                recorded.recovery_of,
                Some(RecoveryLink {
                    parent_run_id: "run-1".to_string(),
                    parent_attempt_id: "attempt-0".to_string(),
                })
            );
            assert_eq!(recorded.budget, remaining);

            // 同一次恢复重复到达：幂等，不产生第二次恢复。
            assert_eq!(
                store.open_recovery_attempt(&recovered, "attempt-0", remaining),
                Ok(RecoveryDecision::AlreadyRecorded {
                    attempt_id: "attempt-1".to_string()
                })
            );
            assert_eq!(store.snapshot().recoveries("run-1").len(), 1);
        }

        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        assert_eq!(
            reopened.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Failed)
        );
        assert_eq!(
            reopened
                .snapshot()
                .recovery("run-1", "attempt-0", "attempt-1")
                .known()
                .expect("recovery fact")
                .effective_budget(),
            Some(remaining)
        );
        assert!(reopened
            .snapshot()
            .recovery("run-1", "attempt-0", "attempt-1")
            .known()
            .expect("recovery fact")
            .is_granted());

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// (e') 沿用父身份被拒绝：不登记任何 attempt，旧 run 不复活。
    #[test]
    fn recovery_reusing_the_parent_attempt_id_is_refused_without_a_new_attempt() {
        let mut store = memory_store();
        let parent = run_identity("run-1", "attempt-0");
        store
            .open_attempt(&parent, budget(10_000, 4, 2, 3))
            .expect("open parent attempt");

        let reused = run_identity("run-1", "attempt-0");
        assert_eq!(
            store.open_recovery_attempt(&reused, "attempt-0", budget(10_000, 4, 2, 3)),
            Ok(RecoveryDecision::Refused(RecoveryError::SameAttemptId))
        );
        assert_eq!(
            store.snapshot().run_attempt("run-1", "attempt-0").known(),
            Some(&crate::fact_store::RunAttemptFact {
                identity: parent,
                budget: budget(10_000, 4, 2, 3),
                recovery_of: None,
            }),
            "被拒绝的恢复不得改写既有 attempt 登记"
        );
        assert_eq!(
            store.snapshot().recoveries("run-1").len(),
            1,
            "被拒绝的决定也要留痕（一条 Refused 事实）"
        );
        assert_eq!(
            store.snapshot().recoveries("run-1")[0].outcome,
            RecoveryFactOutcome::Refused {
                error: RecoveryError::SameAttemptId
            }
        );
    }

    /// (e'') 放宽预算被拒绝；父预算未知时**拒绝**而不是假设一个宽预算。
    #[test]
    fn recovery_cannot_widen_the_budget_or_assume_an_unknown_parent() {
        let mut store = memory_store();
        let parent = run_identity("run-1", "attempt-0");
        store
            .open_attempt(&parent, budget(10_000, 4, 2, 3))
            .expect("open parent attempt");

        let recovered = run_identity("run-1", "attempt-1");
        assert_eq!(
            store.open_recovery_attempt(&recovered, "attempt-0", budget(20_000, 4, 2, 3)),
            Ok(RecoveryDecision::Refused(RecoveryError::BudgetNotCarriedOver))
        );
        assert_eq!(
            store.open_recovery_attempt(&recovered, "attempt-0", budget(10_000, 5, 2, 3)),
            Ok(RecoveryDecision::Refused(RecoveryError::BudgetNotCarriedOver))
        );
        assert_eq!(
            store.snapshot().run_attempt("run-1", "attempt-1"),
            FactLookup::Unknown,
            "被拒绝的恢复不得登记 attempt"
        );
        assert!(store
            .snapshot()
            .recoveries("run-1")
            .iter()
            .all(|fact| !fact.is_granted()));

        // 父 attempt 从未登记 → 无法核对结转 → 拒绝（不许假设父预算）。
        assert_eq!(
            store.open_recovery_attempt(&recovered, "attempt-9", budget(1_000, 1, 1, 1)),
            Err(FactStoreError::UnknownParentAttempt {
                run_id: "run-1".to_string(),
                attempt_id: "attempt-9".to_string(),
            })
        );
        // 被拒绝的决定**不占用** attempt 身份：收紧预算后再来应当被授予。
        let granted = store
            .open_recovery_attempt(&recovered, "attempt-0", budget(1_000, 1, 1, 1))
            .expect("parent budget is known");
        assert!(matches!(granted, RecoveryDecision::GrantedNewAttempt { .. }));
        assert_eq!(
            store
                .snapshot()
                .granted_recovery("run-1", "attempt-0", "attempt-1")
                .known()
                .and_then(crate::fact_store::RecoveryFact::effective_budget),
            Some(budget(1_000, 1, 1, 1))
        );
        assert!(
            store
                .snapshot()
                .recoveries("run-1")
                .iter()
                .any(|fact| !fact.is_granted()),
            "历史里的拒绝决定必须保留（只增不改）"
        );
    }

    /// 回执修订：新回执只追加，旧回执必须留在历史里（I1）。
    #[test]
    fn a_revised_receipt_is_appended_and_the_previous_one_is_kept() {
        let mut store = memory_store();
        let attempt = action_identity("run-1", "attempt-0", "action-1");
        let uncertain = receipt_with_unknown_fields("action-1");
        let reconciled = ActionReceipt {
            input_delivery: InputDelivery::Sent,
            ..uncertain.clone()
        };

        assert_eq!(
            store.record_action_receipt(&attempt, &uncertain),
            Ok(ReceiptDecision::Recorded)
        );
        assert_eq!(
            store.record_action_receipt(&attempt, &uncertain),
            Ok(ReceiptDecision::AlreadyRecorded)
        );
        assert_eq!(
            store.record_action_receipt(&attempt, &reconciled),
            Ok(ReceiptDecision::Revision {
                previous: uncertain.clone()
            })
        );
        assert_eq!(
            store.snapshot().action_receipt_history("action-1"),
            &[uncertain, reconciled.clone()],
            "修订不得丢弃历史回执"
        );
        assert_eq!(
            store.snapshot().latest_action_receipt("action-1"),
            FactLookup::Known(&reconciled)
        );

        // 身份与回执的 action_id 不一致必须被拒绝（否则事实会挂错动作）。
        let other = action_identity("run-1", "attempt-0", "action-2");
        match store.record_action_receipt(&other, &reconciled) {
            Err(FactStoreError::IdentityConflict { field, message }) => {
                assert_eq!(field, "action_id");
                assert!(
                    message.contains("action-1") && message.contains("action-2"),
                    "错误必须点明两个不一致的 action_id：{message}"
                );
            }
            other => panic!("身份与回执的 action_id 不一致必须被拒绝：{other:?}"),
        }
    }

    /// I8：损坏的行必须报错，不得静默跳过；冲突记录保留第一条并留痕。
    #[test]
    fn unparsable_or_conflicting_records_are_reported_not_skipped() {
        let path = temp_log_path("corrupt");
        let root = path.parent().expect("temp root").to_path_buf();
        let attempt = run_identity("run-1", "attempt-0");
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            store
                .record_terminal_control(&attempt, RunTerminalStatus::Failed)
                .expect("record terminal");
        }

        // 手工塞入一条损坏行：必须报错并指出行号。
        let mut text = fs::read_to_string(&path).expect("read fact log");
        text.push_str("{ this is not json }\n");
        fs::write(&path, text).expect("write corrupt log");
        let error = AppendOnlyFactStore::open(JsonlFactLog::new(&path))
            .expect_err("损坏的行必须被报告");
        assert_eq!(error.code(), "fact_store_corrupt_record");
        assert!(matches!(
            error,
            FactStoreError::CorruptRecord { line: 2, .. }
        ));

        // 手工塞入一条与首个终态冲突的记录：读回保留第一条，并把冲突计入 conflicts。
        let mut text = fs::read_to_string(&path).expect("read fact log");
        text.truncate(text.find("{ this is not json }").expect("corrupt line offset"));
        let conflicting = serde_json::json!({
            "record": "terminal_control",
            "identity": attempt.clone(),
            "status": "succeeded",
        });
        text.push_str(&conflicting.to_string());
        text.push('\n');
        fs::write(&path, text).expect("write conflicting log");

        let reopened = AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen");
        assert_eq!(
            reopened.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Failed),
            "冲突记录不得改写第一条结论"
        );
        assert_eq!(
            reopened.snapshot().conflicts().values().copied().sum::<u32>(),
            1
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 两个后端（内存 / 文件）在完全相同的写入序列下给出相同的投影。
    #[test]
    fn memory_and_file_backends_agree_on_the_same_writes() {
        let path = temp_log_path("backend-parity");
        let root = path.parent().expect("temp root").to_path_buf();
        let attempt = run_identity("run-1", "attempt-0");
        let action = action_identity("run-1", "attempt-0", "action-1");
        let key = MessageSubmissionKey::new("msg-1", "room-1", "digest-a");

        let mut in_memory = memory_store();
        let mut on_disk =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
        // 两个后端共用同一段写入序列：投影必须逐项相同。
        let stores: [&mut dyn FactStore; 2] = [&mut in_memory, &mut on_disk];
        for store in stores {
            store
                .open_attempt(&attempt, budget(10_000, 4, 2, 3))
                .expect("open attempt");
            store
                .record_action_receipt(&action, &receipt_with_unknown_fields("action-1"))
                .expect("record receipt");
            store
                .record_usage_attempt(&usage_attempt(
                    "run-1",
                    "logical-1",
                    "attempt-0",
                    ReportedUsage::unknown(),
                ))
                .expect("record usage");
            store
                .record_terminal_control(&attempt, RunTerminalStatus::TimedOut)
                .expect("record terminal");
            store
                .append_late_fact(&late_fact("run-1", "provider", 4_000))
                .expect("append late fact");
            store
                .submit_message(&key, "receipt-1")
                .expect("submit message");
        }
        assert_eq!(in_memory.snapshot(), on_disk.snapshot());
        assert_eq!(in_memory.appended_records(), on_disk.appended_records());

        // 后端只能追加：同一路径再开一个存储看到的是同一份历史。
        let reopened = AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen");
        assert_eq!(reopened.snapshot(), on_disk.snapshot());
        assert_eq!(reopened.appended_records(), 6);
        // 内存后端的记录只能追加，且读回顺序即写入顺序。
        let mut log = InMemoryFactLog::new();
        assert_eq!(log.read_all().expect("empty log").len(), 0);
        assert!(log.records().is_empty());

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 动作事实按 step/action scope 校验；把动作维度塞进轮次级事实（降 scope）会被拒绝。
    #[test]
    fn action_dimensions_cannot_be_written_through_the_turn_fact_path() {
        let mut store = memory_store();
        let turn = run_identity("run-1", "attempt-0");
        assert_eq!(turn.declared_scope(), Some(RunIdentityScope::Turn));

        let mut smuggled = turn.clone();
        smuggled.action_id = Some("action-1".to_string());
        assert_eq!(
            store
                .record_terminal_control(&smuggled, RunTerminalStatus::Succeeded)
                .expect_err("降 scope 必须被拒")
                .code(),
            "identity_dimension_not_applicable"
        );

        let mut smuggled_step = turn.clone();
        smuggled_step.step_id = Some("step-1".to_string());
        assert_eq!(
            store
                .open_attempt(&smuggled_step, budget(10_000, 4, 2, 3))
                .expect_err("attempt 登记也不得携带动作维度")
                .code(),
            "identity_dimension_not_applicable"
        );

        // 动作事实走动作路径；尝试用 turn scope 的身份写动作回执会被拒。
        assert_eq!(
            store
                .record_action_receipt(&turn, &receipt_with_unknown_fields("action-1"))
                .expect_err("动作事实不得用轮次级身份")
                .code(),
            "identity_scope_mismatch"
        );
        assert_eq!(store.appended_records(), 0, "被拒绝的事实一律不得落盘");
        // 正常路径：本 scope 控制终态可写。
        assert_eq!(
            store.record_terminal_control(&turn, RunTerminalStatus::Blocked),
            Ok(TerminalControlDecision::Recorded(RunTerminalStatus::Blocked))
        );
        assert_eq!(store.appended_records(), 1);
    }

    /// 终态只由宿主观察写入：非终态不写事实，只有确认的取消才映射成 cancelled。
    #[test]
    fn host_outcomes_write_terminal_facts_only_when_terminal() {
        let mut store = memory_store();
        let run = run_identity("run-1", "attempt-0");
        assert_eq!(
            store.record_host_outcome(&run, &HostRunOutcome::Running),
            Ok(TerminalControlDecision::NotTerminal)
        );
        assert_eq!(
            store.record_host_outcome(&run, &HostRunOutcome::CancelRequested),
            Ok(TerminalControlDecision::NotTerminal)
        );
        assert_eq!(store.appended_records(), 0, "非终态不得写终态事实");
        assert_eq!(
            store.snapshot().terminal_control("run-1"),
            FactLookup::Unknown
        );
        assert_eq!(
            store.snapshot().scope_succeeded("run-1"),
            None,
            "没有终态事实是未知，不是成功"
        );

        // 异常中断（没有宿主确认的取消）→ interrupted，不冒充 cancelled。
        assert_eq!(
            store.record_host_outcome(
                &run,
                &HostRunOutcome::interrupted(Some("runner exited without terminal fact".to_string()))
            ),
            Ok(TerminalControlDecision::Recorded(
                RunTerminalStatus::Interrupted
            ))
        );
        assert_eq!(store.snapshot().scope_succeeded("run-1"), Some(false));
        // first-wins：事后再来一次"用户取消"也改不了已记录的结论。
        assert_eq!(
            store.record_host_outcome(&run, &HostRunOutcome::cancelled(CancelOrigin::User)),
            Ok(TerminalControlDecision::ConflictingTerminal(
                RunTerminalStatus::Interrupted
            ))
        );
        assert_eq!(
            store.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::Interrupted)
        );

        // 另一个 run：宿主确认的取消 → cancelled，来源被记录（不是默认"用户取消"）。
        let confirmed = run_identity("run-2", "attempt-0");
        let outcome = HostRunOutcome::cancelled(CancelOrigin::BudgetExhausted);
        assert_eq!(outcome.cancel_origin(), Some(&CancelOrigin::BudgetExhausted));
        assert!(!outcome.cancel_origin().expect("origin").is_user_initiated());
        assert_eq!(
            store.record_host_outcome(&confirmed, &outcome),
            Ok(TerminalControlDecision::Recorded(
                RunTerminalStatus::Cancelled
            ))
        );

        // 正常收尾只表示本 scope 结束（Succeeded），不是"用户目标已完成"。
        let completed = run_identity("run-3", "attempt-0");
        assert_eq!(
            store.record_host_outcome(&completed, &HostRunOutcome::Completed),
            Ok(TerminalControlDecision::Recorded(
                RunTerminalStatus::Succeeded
            ))
        );
        assert_eq!(store.snapshot().scope_succeeded("run-3"), Some(true));
        assert!(store
            .snapshot()
            .terminal_control("run-3")
            .known()
            .is_some_and(|status| status.is_scope_success() && !status.claims_goal_completion()));
        // 宿主给不出原因时不许落一条"失败"。
        assert_eq!(
            store
                .record_host_outcome(
                    &completed,
                    &HostRunOutcome::Failed {
                        reason: String::new(),
                        error: None,
                    }
                )
                .expect_err("没有真实原因不得记失败")
                .code(),
            "incomplete_host_outcome"
        );
    }

    /// 证据与回执一起往返；证据与回执矛盾 / 依据缺原始引用时拒绝写入。
    #[test]
    fn action_evidence_is_persisted_with_the_receipt_and_validated_against_it() {
        let path = temp_log_path("evidence");
        let root = path.parent().expect("temp root").to_path_buf();
        let action = action_identity("run-1", "attempt-0", "action-1");
        let receipt = ActionReceipt {
            input_delivery: InputDelivery::Sent,
            ..receipt_with_unknown_fields("action-1")
        };
        let evidence = ActionEvidence {
            action_id: "action-1".to_string(),
            surface: ActionSurface::Desktop,
            basis: EvidenceBasis::NativeHelperReceipt,
            request_id: Some("helper-call-7".to_string()),
            raw_error_ref: None,
            raw_response_ref: Some(EvidenceReference::new(
                EvidenceReferenceKind::Response,
                "helper#exit/0",
            )),
            inference_rule_version: None,
        };
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            let fact =
                ActionFact::new(action.clone(), receipt.clone()).with_evidence(evidence.clone());
            assert_eq!(store.record_action_fact(&fact), Ok(ReceiptDecision::Recorded));
            // 幂等：同一事实（含同一份证据）重复到达不再追加。
            assert_eq!(
                store.record_action_fact(&fact),
                Ok(ReceiptDecision::AlreadyRecorded)
            );
            assert_eq!(store.appended_records(), 1);

            // 依据与回执矛盾：DOM 协议响应不得断言鼠标已释放 → 拒绝写入。
            let mut released = receipt.clone();
            released.input_release = InputReleaseStatus::Released;
            let protocol = ActionEvidence {
                action_id: "action-1".to_string(),
                surface: ActionSurface::Browser,
                basis: EvidenceBasis::BrowserProtocolResponse,
                request_id: Some("protocol-request-1".to_string()),
                raw_error_ref: None,
                raw_response_ref: Some(EvidenceReference::new(
                    EvidenceReferenceKind::Response,
                    "browser_bridge#dispatch/9",
                )),
                inference_rule_version: None,
            };
            assert_eq!(
                store
                    .record_action_fact(
                        &ActionFact::new(action.clone(), released).with_evidence(protocol)
                    )
                    .expect_err("矛盾证据必须被拒")
                    .code(),
                "invalid_action_evidence"
            );
            // 依据缺原始引用：同样拒绝（只有一个标签不算证据）。
            let label_only = ActionEvidence {
                request_id: None,
                ..evidence.clone()
            };
            assert_eq!(
                store
                    .record_action_fact(
                        &ActionFact::new(action.clone(), receipt.clone()).with_evidence(label_only)
                    )
                    .expect_err("缺原始引用的依据必须被拒")
                    .code(),
                "invalid_action_evidence"
            );
            assert_eq!(store.appended_records(), 1, "被拒的证据不得落盘");
        }

        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        assert_eq!(
            reopened.snapshot().latest_action_evidence("action-1"),
            FactLookup::Known(&evidence),
            "证据必须原样往返（不是只写日志不读回）"
        );
        assert_eq!(
            reopened.snapshot().latest_action_receipt("action-1"),
            FactLookup::Known(&receipt)
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 旧 JSON 回执（缺所有可选键）读成未知，绝不默认 `false` / 0；
    /// 记录级的证据与异常键缺省时按"没有证据 / 无异常"读取。
    #[test]
    fn legacy_receipt_json_reads_optionals_as_unknown() {
        let path = temp_log_path("legacy-receipt");
        let root = path.parent().expect("temp root").to_path_buf();
        fs::create_dir_all(&root).expect("create temp root");
        let action = action_identity("run-1", "attempt-0", "action-1");
        let legacy_line = serde_json::json!({
            "record": "action_receipt",
            "identity": serde_json::to_value(&action).expect("encode identity"),
            "receipt": {
                "action_id": "action-1",
                "input_delivery": "may_have_been_sent",
                "effect": "not_observed",
                "goal_verdict": "not_checked",
                "input_release": "unknown"
            }
        });
        fs::write(&path, format!("{legacy_line}\n")).expect("write legacy log");

        let store =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("旧 JSON 必须仍可读取");
        let receipt = store
            .snapshot()
            .latest_action_receipt("action-1")
            .known()
            .expect("回执事实必须被读回");
        assert_eq!(receipt.partial, None, "缺键必须读成未知，不能默认 false");
        assert_eq!(receipt.partial_observation(), PartialObservation::Unknown);
        assert_eq!(receipt.path_completed, None);
        assert_eq!(receipt.confirmed_point_count, None);
        assert!(!receipt.path_proven_complete(), "未知不等于完整执行");
        assert!(receipt.may_have_started_input(), "未知必须按可能已开始处理");
        assert_eq!(
            store.snapshot().latest_action_evidence("action-1"),
            FactLookup::Unknown,
            "没有证据键 = 没有证据事实（不是某个默认依据）"
        );
        assert!(store.snapshot().identity_anomalies("run-1").is_empty());
        assert!(!store.snapshot().blocks_further_input("run-1"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 旧记录（缺 scope）按旧版严格规则解释：缺维度报错，**不得**默认视为 Turn。
    #[test]
    fn scope_less_records_are_read_by_the_old_strict_rules() {
        let path = temp_log_path("legacy-scope");
        let root = path.parent().expect("temp root").to_path_buf();
        fs::create_dir_all(&root).expect("create temp root");

        // ① 只填容器维度的旧记录：若被当成 Turn 事实就会被接受；旧严格规则要求十维 → 报错。
        let legacy_short = serde_json::json!({
            "record": "terminal_control",
            "identity": {
                "workspace_id": "workspace-1",
                "room_id": "room-1",
                "session_id": "session-1",
                "public_turn_id": "turn-public-1",
                "run_id": "run-1"
            },
            "status": "succeeded"
        });
        fs::write(&path, format!("{legacy_short}\n")).expect("write legacy log");
        let error = AppendOnlyFactStore::open(JsonlFactLog::new(&path))
            .expect_err("旧记录缺维度必须报错，而不是被当成 Turn 事实放行");
        assert_eq!(error.code(), "fact_store_invalid_record");
        assert!(
            error.to_string().contains("本应存在却缺失"),
            "错误必须说明是身份不完整：{error}"
        );

        // ② 十维齐全的旧记录：按旧严格规则通过（但语义仍是"旧记录"，不是声明的 scope）。
        let legacy_full = serde_json::json!({
            "record": "terminal_control",
            "identity": {
                "workspace_id": "workspace-1",
                "room_id": "room-1",
                "session_id": "session-1",
                "public_turn_id": "turn-public-1",
                "run_id": "run-1",
                "step_id": "step-1",
                "request_attempt_id": "attempt-0",
                "tool_call_id": "tool-call-1",
                "action_id": "action-1",
                "owner_epoch": "7"
            },
            "status": "timed_out"
        });
        fs::write(&path, format!("{legacy_full}\n")).expect("write legacy log");
        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("旧记录按旧严格规则可读");
        assert_eq!(
            reopened.snapshot().terminal_control("run-1"),
            FactLookup::Known(RunTerminalStatus::TimedOut)
        );
        assert_eq!(reopened.snapshot().scope_succeeded("run-1"), Some(false));

        // ③ 十维齐全的旧**动作**记录（无 scope、无新增键）：同样按旧严格规则读回，
        //    并保留旧口径的 action_id 一致性校验。
        let legacy_receipt = serde_json::json!({
            "record": "action_receipt",
            "identity": {
                "workspace_id": "workspace-1",
                "room_id": "room-1",
                "session_id": "session-1",
                "public_turn_id": "turn-public-1",
                "run_id": "run-1",
                "step_id": "step-1",
                "request_attempt_id": "attempt-0",
                "tool_call_id": "tool-call-1",
                "action_id": "action-1",
                "owner_epoch": "7"
            },
            "receipt": {
                "action_id": "action-1",
                "input_delivery": "may_have_been_sent",
                "effect": "not_observed",
                "goal_verdict": "not_checked",
                "input_release": "unknown"
            }
        });
        fs::write(&path, format!("{legacy_receipt}\n")).expect("write legacy log");
        let reopened = AppendOnlyFactStore::open(JsonlFactLog::new(&path))
            .expect("旧动作记录按旧严格规则可读");
        let receipt = reopened
            .snapshot()
            .latest_action_receipt("action-1")
            .known()
            .expect("旧回执必须被读回");
        assert_eq!(receipt.partial, None);
        assert_eq!(receipt.partial_observation(), PartialObservation::Unknown);
        assert_eq!(
            reopened.snapshot().latest_action_evidence("action-1"),
            FactLookup::Unknown
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 日志里出现不认识的终态变体：读回报错，**绝不**当成成功（旧消费者的硬约束）。
    #[test]
    fn unknown_terminal_variants_in_the_log_are_never_read_as_success() {
        let path = temp_log_path("unknown-terminal");
        let root = path.parent().expect("temp root").to_path_buf();
        fs::create_dir_all(&root).expect("create temp root");
        let identity = serde_json::json!({
            "workspace_id": "workspace-1",
            "room_id": "room-1",
            "session_id": "session-1",
            "public_turn_id": "turn-public-1",
            "run_id": "run-1",
            "scope": "turn",
            "schema_version": 2
        });
        for unknown in ["aborted", "succeeded_v2", "ok"] {
            let line = serde_json::json!({
                "record": "terminal_control",
                "identity": identity,
                "status": unknown
            });
            fs::write(&path, format!("{line}\n")).expect("write log");
            let error = AppendOnlyFactStore::open(JsonlFactLog::new(&path))
                .expect_err("不认识的终态必须报错");
            assert_eq!(error.code(), "fact_store_corrupt_record");
            assert!(
                error.to_string().contains(unknown),
                "错误必须点明不认识的变体：{error}"
            );
        }

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 已经开始输入却缺身份：事实必须保留（带异常），并且此后不得再发输入。
    #[test]
    fn identity_anomalies_preserve_the_fact_and_stop_further_input() {
        let path = temp_log_path("identity-anomaly");
        let root = path.parent().expect("temp root").to_path_buf();
        let mut missing_epoch = action_identity("run-1", "attempt-0", "action-1");
        missing_epoch.owner_epoch = None;
        let sent = ActionReceipt {
            input_delivery: InputDelivery::Sent,
            ..receipt_with_unknown_fields("action-1")
        };
        let blocked = ActionReceipt {
            action_id: "action-3".to_string(),
            input_delivery: InputDelivery::NotSent,
            partial: Some(false),
            path_completed: Some(false),
            confirmed_point_count: Some(0),
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::NotNeeded,
        };
        {
            let mut store =
                AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("open fact log");
            let decision = store
                .record_action_fact(&ActionFact::new(missing_epoch.clone(), sent.clone()))
                .expect("已发生的事实必须保留，不得因结构校验失败而丢弃");
            let ReceiptDecision::RecordedWithIdentityAnomaly { anomaly } = &decision else {
                panic!("必须是异常保留判决：{decision:?}");
            };
            assert_eq!(anomaly.code(), "incomplete_identity");
            assert!(anomaly.must_stop_input);
            assert_eq!(anomaly.missing_dimensions, vec!["owner_epoch".to_string()]);
            assert_eq!(
                store.snapshot().latest_action_receipt("action-1"),
                FactLookup::Known(&sent),
                "原回执必须保留"
            );

            // "停止后续输入"：同一 run 里声明输入资格的新事实被拒。
            let mut next_receipt = sent.clone();
            next_receipt.action_id = "action-2".to_string();
            let next = action_identity("run-1", "attempt-0", "action-2");
            assert_eq!(
                store.record_action_fact(&ActionFact::new(next, next_receipt)),
                Err(FactStoreError::InputBlockedByIdentityAnomaly {
                    run_id: "run-1".to_string()
                })
            );
            // 输入前拒绝类事实不受影响（它不动手，也不获得输入资格）。
            let rejection_identity = action_identity("run-1", "attempt-0", "action-3");
            assert_eq!(
                store.record_action_fact(&ActionFact::new(rejection_identity, blocked.clone())),
                Ok(ReceiptDecision::Recorded),
                "输入前拒绝必须仍可记录"
            );
            assert_eq!(store.appended_records(), 2);
        }

        let reopened =
            AppendOnlyFactStore::open(JsonlFactLog::new(&path)).expect("reopen fact log");
        assert!(
            reopened.snapshot().blocks_further_input("run-1"),
            "身份异常必须随记录落盘，重开后仍然挡住后续输入"
        );
        assert_eq!(reopened.snapshot().identity_anomalies("run-1").len(), 1);
        assert_eq!(
            reopened
                .snapshot()
                .action_identity_anomalies("action-1")
                .len(),
            1
        );
        assert_eq!(
            reopened.snapshot().latest_action_receipt("action-3"),
            FactLookup::Known(&blocked)
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 没有依据的身份异常标注（身份其实完整）必须被拒绝：异常不是可以随便贴的标签。
    #[test]
    fn fabricated_identity_anomaly_marks_are_rejected() {
        let path = temp_log_path("fabricated-anomaly");
        let root = path.parent().expect("temp root").to_path_buf();
        fs::create_dir_all(&root).expect("create temp root");
        let action = action_identity("run-1", "attempt-0", "action-1");
        let line = serde_json::json!({
            "record": "action_receipt",
            "identity": serde_json::to_value(&action).expect("encode identity"),
            "receipt": serde_json::to_value(receipt_with_unknown_fields("action-1")).expect("encode"),
            "identity_anomaly": {
                "error": {
                    "code": "incomplete_identity",
                    "message": "自己写的异常",
                    "retryable": false,
                    "retry_owner": "controller"
                },
                "missing_dimensions": ["owner_epoch"],
                "run_id": "run-1",
                "action_id": "action-1",
                "input_may_have_started": true,
                "must_stop_input": true
            }
        });
        fs::write(&path, format!("{line}\n")).expect("write log");
        let error = AppendOnlyFactStore::open(JsonlFactLog::new(&path))
            .expect_err("没有依据的身份异常必须被拒");
        assert_eq!(error.code(), "fact_store_invalid_record");
        assert!(
            error.to_string().contains("身份异常"),
            "错误必须点明异常标注的问题：{error}"
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    /// 父运行关联必须指向已登记的运行：不同运行实体各自保有自己的 `run_id`。
    #[test]
    fn parent_run_links_must_point_to_a_registered_run() {
        let mut store = memory_store();
        let mut child = action_identity("run-cu", "attempt-0", "action-1");
        child.parent_run = Some(ParentRunLink::new(
            "run-turn",
            RunParentRelation::DrivenByTurnRun,
        ));
        let sent = ActionReceipt {
            input_delivery: InputDelivery::Sent,
            ..receipt_with_unknown_fields("action-1")
        };
        assert_eq!(
            store.record_action_fact(&ActionFact::new(child.clone(), sent.clone())),
            Err(FactStoreError::UnknownParentRun {
                run_id: "run-turn".to_string()
            }),
            "父运行没登记过就不许挂上去"
        );
        assert_eq!(store.appended_records(), 0);

        store
            .open_attempt(
                &run_identity("run-turn", "attempt-turn"),
                budget(10_000, 4, 2, 3),
            )
            .expect("open parent run");
        assert_eq!(
            store.record_action_fact(&ActionFact::new(child, sent)),
            Ok(ReceiptDecision::Recorded),
            "父运行登记过之后子运行的动作事实可以写入"
        );
        assert_eq!(store.appended_records(), 2);
    }

    // -----------------------------------------------------------------------
    // RD4-01：遗留 CU 运行收敛的规则
    // -----------------------------------------------------------------------

    fn legacy_convergence_candidate() -> LegacyCuRunConvergenceFact {
        LegacyCuRunConvergenceFact {
            subject: LegacyCuRunSubject {
                source_database_identity: "session-db:/tmp/ws/sessions.sqlite3".to_string(),
                source_database_identity_registered_now: false,
                original_run_id: "cu-legacy-1".to_string(),
                source_database_identity_not_past_attribution: None,
            },
            original: LegacyCuRunOriginalFacts {
                original_state: "executing".to_string(),
                original_state_version: 7,
                session_id: Some("session-1".to_string()),
                turn_id: Some("turn-1".to_string()),
            },
            missing: LegacyCuRunMissingDimensions::workspace_unrecorded(),
            decision: LegacyCuRunConvergenceDecision::interrupted("recovery-op-1"),
            evidence: LegacyCuRunConvergenceEvidence::default(),
            operator: LegacyCuRunRecoveryOperator {
                recovery_service_instance: "recovery-1".to_string(),
                recovery_control_authority: "single-writer-claim:recovery-1".to_string(),
                operated_at_unix_ms: 5_000,
            },
            control: LegacyRunControlState::continuation_terminated_by_recovery(),
            historical_outcome: LegacyRunHistoricalOutcome::Unknown,
            input_resource: LegacyRunInputResourceState {
                old_executor_may_be_present: None,
                unconfirmed_release_obligations: 1,
                blocking_event_refs: vec!["input-safety:incident-9".to_string()],
                safe_for_new_input: false,
            },
            historical_resource_scope: LegacyResourceScope::Unrecorded,
            current_resource_safety_check: CurrentResourceSafetyCheck {
                basis: CurrentResourceCandidateBasis::CurrentMayBeAffectedNotHistoricalScope,
                session_id: "session-1".to_string(),
                turn_id: "turn-1".to_string(),
                checked_at_unix_ms: 4_900,
            },
            side_effects: LegacyCuRunSideEffectLimits::none(),
            reconciled_at_unix_ms: 5_100,
        }
    }

    fn legacy_convergence_observed() -> LegacyCuRunObservedState {
        LegacyCuRunObservedState {
            state: "executing".to_string(),
            state_version: 7,
            has_real_terminal: false,
            has_commit_candidate: false,
        }
    }

    /// 同一条收敛的重复请求不再写第二份。
    #[test]
    fn repeat_legacy_convergence_requests_never_write_a_second_fact() {
        let candidate = legacy_convergence_candidate();
        let existing = vec![candidate.clone()];
        let decision = reconcile_legacy_cu_run_convergence(
            &existing,
            &legacy_convergence_observed(),
            &candidate,
        )
        .expect("规则必须给出结论");
        assert!(matches!(
            decision,
            LegacyCuRunConvergenceRuleDecision::AlreadyConverged { .. }
        ));

        // 重复请求带上新的操作 ID / 新的对账时刻，仍然是同一次收敛（键相同、内容相同）。
        let mut repeat = candidate.clone();
        repeat.decision.recovery_operation_id = "recovery-op-2".to_string();
        repeat.operator.recovery_service_instance = "recovery-2".to_string();
        repeat.operator.operated_at_unix_ms = 6_000;
        repeat.reconciled_at_unix_ms = 6_100;
        assert!(matches!(
            reconcile_legacy_cu_run_convergence(
                &existing,
                &legacy_convergence_observed(),
                &repeat
            )
            .expect("规则必须给出结论"),
            LegacyCuRunConvergenceRuleDecision::AlreadyConverged { .. }
        ));

        // 首次收敛（没有任何既有事实）才是 `Recorded`。
        assert_eq!(
            reconcile_legacy_cu_run_convergence(
                &[],
                &legacy_convergence_observed(),
                &candidate
            )
            .expect("规则必须给出结论"),
            LegacyCuRunConvergenceRuleDecision::Recorded
        );
    }

    /// 已有真实终态 / 提交候选 / 原 revision 变了 / 冲突事实 / 操作 ID 复用：一律不覆盖。
    #[test]
    fn legacy_convergence_never_overwrites_a_real_terminal_or_a_conflicting_fact() {
        let candidate = legacy_convergence_candidate();

        let mut terminal = legacy_convergence_observed();
        terminal.has_real_terminal = true;
        assert_eq!(
            reconcile_legacy_cu_run_convergence(&[], &terminal, &candidate).expect("判决"),
            LegacyCuRunConvergenceRuleDecision::Refused(
                LegacyCuRunConvergenceRefusal::AlreadyHasRealTerminal {
                    observed_state: "executing".to_string()
                }
            )
        );

        let mut pending_commit = legacy_convergence_observed();
        pending_commit.has_commit_candidate = true;
        assert_eq!(
            reconcile_legacy_cu_run_convergence(&[], &pending_commit, &candidate).expect("判决"),
            LegacyCuRunConvergenceRuleDecision::Refused(
                LegacyCuRunConvergenceRefusal::CommitCandidatePending
            )
        );

        // 两读之间该行被别的真实终态提交更新过（revision 变了）。
        let mut moved = legacy_convergence_observed();
        moved.state_version = 8;
        assert_eq!(
            reconcile_legacy_cu_run_convergence(&[], &moved, &candidate).expect("判决"),
            LegacyCuRunConvergenceRuleDecision::Refused(
                LegacyCuRunConvergenceRefusal::OriginalRevisionChanged {
                    observed_state: "executing".to_string(),
                    observed_state_version: 8,
                }
            )
        );

        // 同一对账键上已有一条**内容不同**的收敛事实（结论/观测被改写）：拒绝覆盖。
        let mut conflicting = candidate.clone();
        conflicting.input_resource.unconfirmed_release_obligations = 0;
        conflicting.input_resource.safe_for_new_input = true;
        conflicting.input_resource.blocking_event_refs.clear();
        assert!(matches!(
            reconcile_legacy_cu_run_convergence(
                &[conflicting],
                &legacy_convergence_observed(),
                &candidate
            )
            .expect("判决"),
            LegacyCuRunConvergenceRuleDecision::Refused(
                LegacyCuRunConvergenceRefusal::ConflictingConvergence { .. }
            )
        ));

        // 同一个恢复操作 ID 出现在另一个对象上：拒绝（否则对账会错位）。
        let mut other_object = candidate.clone();
        other_object.subject.original_run_id = "cu-legacy-2".to_string();
        assert_eq!(
            reconcile_legacy_cu_run_convergence(
                &[other_object],
                &legacy_convergence_observed(),
                &candidate
            )
            .expect("判决"),
            LegacyCuRunConvergenceRuleDecision::Refused(
                LegacyCuRunConvergenceRefusal::RecoveryOperationReused {
                    other_run_id: "cu-legacy-2".to_string()
                }
            )
        );
        assert_eq!(
            LegacyCuRunConvergenceRefusal::CommitCandidatePending.code(),
            "legacy_convergence_commit_candidate_pending"
        );
    }

    /// 请求自身不成立时直接报错（**不**返回"拒绝写入"这种可继续的判决），
    /// 且缺失是 unknown：没有收敛过就是 `Unknown`，不是任何默认值。
    #[test]
    fn legacy_convergence_rejects_invalid_requests_and_reads_absence_as_unknown() {
        let mut invalid = legacy_convergence_candidate();
        invalid.decision.explanation = "目标已完成".to_string();
        assert!(matches!(
            reconcile_legacy_cu_run_convergence(&[], &legacy_convergence_observed(), &invalid),
            Err(FactStoreError::InvalidRecord { .. })
        ));
        assert_eq!(
            latest_legacy_cu_run_convergence(&[], &legacy_convergence_candidate().subject),
            FactLookup::Unknown
        );
        let candidate = legacy_convergence_candidate();
        assert!(matches!(
            latest_legacy_cu_run_convergence(&[candidate.clone()], &candidate.subject),
            FactLookup::Known(_)
        ));
        // 换一个对象读，仍然是 unknown（不得把别人的收敛读成自己的）。
        let mut other = candidate.subject.clone();
        other.original_run_id = "cu-legacy-9".to_string();
        assert_eq!(
            latest_legacy_cu_run_convergence(&[candidate], &other),
            FactLookup::Unknown
        );
    }
}
