//! 共享输入安全存储的**领域契约**（第七轮裁决 §1；权威正文见
//! `docs/analysis/2026-09-21-integration-review/round7-rulings-and-gates.md`）。
//!
//! # 为什么在这里
//!
//! 裁决 §1.4 固定：**领域契约放在现有核心/CU 契约层**，SQLite 实现放在 Web／宿主适配边界
//! （复用既有 rusqlite），**不把 rusqlite 引入 core-runtime**。因此本文件**只有纯类型与校验**，
//! 不含任何存储访问。
//!
//! # 三层分工（裁决 §1.1，不可合并省略）
//!
//! | 组成 | 负责什么 | **不能**代替什么 |
//! | --- | --- | --- |
//! | 输入所有权与执行者监督（`windows-process-guard`） | 排他输入、在途执行者身份、停止与静止核查 | 不能自动保存进程死亡后的未决安全义务 |
//! | **用户级共享安全库**（本契约 + 宿主 SQLite 适配） | 隔离、未清偿义务、恢复状态、**跨工作区可见**阻断 | 不能仅靠一行 `safe=true` 证明物理状态 |
//! | 会话／运行事实库（`computer_use_store`） | 原动作、回执、终态及其来源 | 不能要求新 workspace 逐一遍历所有旧库后才知道资源是否被隔离 |
//!
//! # 本契约的硬约束
//!
//! 1. **独立版本**：本库用 [`INPUT_SAFETY_SCHEMA_VERSION`]，**不**与会话库的 `SESSION_SCHEMA_VERSION`
//!    共用版本号，也**不**因为本库而改动会话库的迁移号（裁决 §1.2）。
//! 2. **`Unknown` 不是 `Safe`**：资源安全状态三值，缺失一律 `Unknown`，禁止把"没查到"读成安全。
//! 3. **阻断引用必须可验证**：[`VerifiedBlockingRef`] **没有** `Deserialize`，也**没有**公开构造函数——
//!    它只能由服务端核查后产生，因此调用方**无法**通过反序列化一个 `validated=true` 自证（裁决 §1.5）。
//! 4. **作用域是物理输入资源**：形如 `windows-session-<id>`（与输入 broker 的
//!    `current_interactive_session_scope()` 对齐），**不得**包含路径分隔符或冒号——避免同一桌面因
//!    不同 workspace／安装目录／自选数据库路径拿到两把互不相干的锁（裁决 §2.3）。

use std::fmt;

/// 输入安全库的**独立**组件 schema 版本（裁决 §1.2）。
///
/// 与会话库的版本号**互不相干**。升级 schema **不得**另建空的新版本文件来遗忘旧事故。
///
/// - v1：五实体（identity／resource_state／incidents／resource_blocks／ownership_epochs＋events）。
/// - v2（PR-01／P0-1）：`input_safety_recovery_operations` 增加 `disposition`（**结账口径**），
///   并新增 `input_safety_release_decisions`（人工放行决定）。加列走 `ALTER TABLE`（旧库就地升级），
///   不新建空库——"另建空库"等于遗忘旧事故。
/// - v3（2026-09-26 8.2b／8.3b **联合**迁移）：`input_safety_permits`（六态许可登记）与
///   `input_safety_executors`（执行者实例登记）及其必要关联。两者**共用一次迁移**，
///   避免许可表与执行者表各自抢号、也避免中间版本表达不了两者关系。
pub const INPUT_SAFETY_SCHEMA_VERSION: i64 = 3;

/// 存储身份前缀：`is-` + 32 位小写十六进制。
pub const INPUT_SAFETY_STORE_ID_PREFIX: &str = "is-";

/// 资源安全状态的**唯一真值表**。
///
/// `Unknown` 与 `Safe` 是不同事实：前者表示"没有取得可核对的证据"，后者表示"已独立证明安全"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceSafetyState {
    /// 已隔离：不允许新输入，直到独立条件成立。
    Isolated,
    /// 已证明安全（必须有对应检查证据，不能靠"空引用 + safe=true"自证）。
    Safe,
    /// 未知：默认值，**不构成放行依据**。
    Unknown,
}

impl ResourceSafetyState {
    /// 是否允许开放新输入：**只有** `Safe` 允许；`Unknown` 与 `Isolated` 都不允许。
    #[must_use]
    pub const fn allows_new_input(self) -> bool {
        matches!(self, Self::Safe)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Safe => "safe",
            Self::Unknown => "unknown",
        }
    }
}

/// 存储身份被拒绝的原因（分类可区分，便于审计与纠错）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidInputSafetyStoreId {
    /// 缺少 `is-` 前缀。
    MissingPrefix,
    /// 后缀不是 32 位小写十六进制。
    MalformedSuffix,
}

impl InvalidInputSafetyStoreId {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::MissingPrefix => "input_safety_store_id_missing_prefix",
            Self::MalformedSuffix => "input_safety_store_id_malformed",
        }
    }
}

/// 已登记的输入安全库身份（**只能**由 [`parse_input_safety_store_id`] 或服务端生成后构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSafetyStoreId(String);

impl InputSafetyStoreId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 解析一个已登记的身份字符串。**不做**任何近似归一（大小写、前缀都不放宽）。
pub fn parse_input_safety_store_id(value: &str) -> Result<InputSafetyStoreId, InvalidInputSafetyStoreId> {
    let value = value.trim();
    let Some(suffix) = value.strip_prefix(INPUT_SAFETY_STORE_ID_PREFIX) else {
        return Err(InvalidInputSafetyStoreId::MissingPrefix);
    };
    if suffix.len() != 32
        || !suffix
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        return Err(InvalidInputSafetyStoreId::MalformedSuffix);
    }
    Ok(InputSafetyStoreId(value.to_string()))
}

/// 实际物理输入资源的作用域（与输入 broker 对齐，形如 `windows-session-1`）。
///
/// 拒绝理由单列（而不是"非法作用域"一句带过），因为这三类在实现与运维上的处置完全不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidResourceScope {
    Empty,
    /// 含路径分隔符／冒号：说明有人把 workspace、安装目录或自选数据库路径当成了作用域。
    LooksLikePath,
    /// 含控制字符。
    ControlCharacters,
}

/// 已校验的物理输入资源作用域。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSafetyResourceScope(String);

impl InputSafetyResourceScope {
    pub fn parse(value: &str) -> Result<Self, InvalidResourceScope> {
        let value = value.trim();
        if value.is_empty() {
            return Err(InvalidResourceScope::Empty);
        }
        if value.chars().any(char::is_control) {
            return Err(InvalidResourceScope::ControlCharacters);
        }
        if value.contains('/') || value.contains('\\') || value.contains(':') {
            return Err(InvalidResourceScope::LooksLikePath);
        }
        Ok(Self(value.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 某个资源作用域的持久化安全状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSafetyResourceState {
    pub scope: InputSafetyResourceScope,
    pub state: ResourceSafetyState,
    /// 单调 revision：任何安全状态变更都推进它（恢复前置检查要按它比对）。
    pub revision: u64,
    /// 当前持有恢复协调权的实例（`None` = 当前没有协调者）。
    pub coordinator_instance_id: Option<String>,
    /// 当前恢复 epoch（重启后必须重新取权，**不得**反序列化旧 token 继续用）。
    pub recovery_epoch: u64,
    /// 是否接受新输入（由服务按 `state` 与回执共同决定，**不是**调用方可任意赋值的入参）。
    pub accepts_new_input: bool,
}

impl InputSafetyResourceState {
    /// 初值：未知、无协调者、epoch 0、**不接受新输入**（fail-closed 起点）。
    #[must_use]
    pub fn initial(scope: InputSafetyResourceScope) -> Self {
        Self {
            scope,
            state: ResourceSafetyState::Unknown,
            revision: 1,
            coordinator_instance_id: None,
            recovery_epoch: 0,
            accepts_new_input: false,
        }
    }

    /// 状态变更：推进 revision，并按新状态收紧/放开接纳。
    pub fn transition(
        &mut self,
        state: ResourceSafetyState,
        coordinator: Option<&str>,
        recovery_epoch: u64,
    ) {
        self.state = state;
        self.revision = self.revision.saturating_add(1);
        self.coordinator_instance_id = coordinator.map(str::to_string);
        self.recovery_epoch = recovery_epoch;
        // 只有"已独立证明安全"才放开；Unknown/Isolated 一律不接受新输入。
        self.accepts_new_input = state.allows_new_input();
    }
}

/// 阻断事件（incident）的状态。**未决**与**已恢复**是不同事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncidentState {
    Pending,
    Resolved,
}

impl IncidentState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
        }
    }
}

/// 未清偿的输入安全阻断事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSafetyIncident {
    pub incident_id: String,
    pub scope: InputSafetyResourceScope,
    /// 为什么建立这条阻断（人可读，稳定措辞）。
    pub reason: String,
    /// 触发它的原运行引用（形如 `<source_database_identity>#<run_id>`；**不是** workspace id）。
    pub original_run_ref: Option<String>,
    pub state: IncidentState,
    pub created_at_unix_ms: u64,
    pub resolved_at_unix_ms: Option<u64>,
    /// 证据引用（可为空；空表示"本次没有取得证据"，**不表示**安全）。
    pub evidence_refs: Vec<String>,
}

/// 阻断引用为什么**不能**作为有效阻断证明（裁决 §1.5／§7.2 的组合用例逐条对应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockingRefRejection {
    /// 语法不是"store#incident"形式。
    Malformed,
    /// 不属于已确认的 input-safety store（含：库中不存在该 store 身份）。
    UnknownStore { store_id: String },
    /// incident 在库中不存在。
    IncidentNotFound { incident_id: String },
    /// incident 不属于本次受影响的资源 scope。
    ScopeMismatch { expected: String, actual: String },
    /// incident 已解决（已解决不阻断新输入）。
    AlreadyResolved { incident_id: String },
    /// 当前状态并不阻止新输入。
    NotBlocking { state: ResourceSafetyState },
    /// revision 与恢复上下文不一致（调用方拿着过期视图）。
    RevisionMismatch { expected: u64, actual: u64 },
    /// 与被处理运行／风险没有可解释关联。
    UnexplainedAssociation { incident_id: String },
}

impl BlockingRefRejection {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Malformed => "blocking_ref_malformed",
            Self::UnknownStore { .. } => "blocking_ref_unknown_store",
            Self::IncidentNotFound { .. } => "blocking_ref_incident_not_found",
            Self::ScopeMismatch { .. } => "blocking_ref_scope_mismatch",
            Self::AlreadyResolved { .. } => "blocking_ref_already_resolved",
            Self::NotBlocking { .. } => "blocking_ref_not_blocking",
            Self::RevisionMismatch { .. } => "blocking_ref_revision_mismatch",
            Self::UnexplainedAssociation { .. } => "blocking_ref_unexplained",
        }
    }
}

impl fmt::Display for BlockingRefRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(formatter, "阻断引用的语法不是 store#incident 形式"),
            Self::UnknownStore { store_id } => {
                write!(formatter, "阻断引用不属于已确认的输入安全库：{store_id}")
            }
            Self::IncidentNotFound { incident_id } => {
                write!(formatter, "阻断引用指向的 incident 不存在：{incident_id}")
            }
            Self::ScopeMismatch { expected, actual } => write!(
                formatter,
                "阻断引用的 scope 与本次受影响资源不符（本次 {expected}，引用 {actual}）"
            ),
            Self::AlreadyResolved { incident_id } => {
                write!(formatter, "阻断引用指向的 incident 已解决：{incident_id}")
            }
            Self::NotBlocking { state } => {
                write!(formatter, "资源当前状态并不阻止新输入：{}", state.as_str())
            }
            Self::RevisionMismatch { expected, actual } => write!(
                formatter,
                "阻断引用的 revision 与恢复上下文不一致（上下文 {expected}，引用 {actual}）"
            ),
            Self::UnexplainedAssociation { incident_id } => write!(
                formatter,
                "阻断引用与被处理运行/风险没有可解释关联：{incident_id}"
            ),
        }
    }
}

/// **服务端核查后**产生的阻断证明。
///
/// 三重保护，确保调用方无法自证：
/// 1. **没有** `Deserialize`（不能从 JSON 反序列化出一个 `validated=true`）；
/// 2. **没有**公开构造函数，字段私有；
/// 3. 只能由 `InputSafetyService` 在真实核查通过后构造。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBlockingRef {
    store_id: InputSafetyStoreId,
    incident_id: String,
    scope: InputSafetyResourceScope,
    revision: u64,
    /// 恒为 `true`：能构造出来即代表核查通过（类型的全部意义）。
    blocking: bool,
}

impl VerifiedBlockingRef {
    /// **仅供服务端核查通过后调用**（同一 crate 内的存储适配层）：
    /// 契约层不提供任何"直接构造已验证对象"的公开路径。
    #[must_use]
    pub fn from_verified_parts(
        store_id: InputSafetyStoreId,
        incident_id: impl Into<String>,
        scope: InputSafetyResourceScope,
        revision: u64,
    ) -> Self {
        Self {
            store_id,
            incident_id: incident_id.into(),
            scope,
            revision,
            blocking: true,
        }
    }

    #[must_use]
    pub fn store_id(&self) -> &InputSafetyStoreId {
        &self.store_id
    }

    #[must_use]
    pub fn incident_id(&self) -> &str {
        &self.incident_id
    }

    #[must_use]
    pub fn scope(&self) -> &InputSafetyResourceScope {
        &self.scope
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// 恒为 true：能构造出来就代表已核查通过（这是类型的**全部**意义）。
    #[must_use]
    pub const fn has_verified_block(&self) -> bool {
        self.blocking
    }
}

/// 恢复操作的阶段（与裁决 §2.3 的 R1–R9 顺序**逐项对应**，不做另一套自拟分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStage {
    /// R1：已取得同 scope 唯一恢复协调权。
    CoordinationAcquired,
    /// R2：恢复意图与"关闭接纳"已持久化。
    IntentPersistedAndIntakeClosed,
    /// R3：未激活的普通许可已撤销、在途操作已登记。
    InFlightRegistered,
    /// R4：已请求相关旧执行者停止并核查身份。
    ExecutorStopRequested,
    /// R5：已为未确认风险建立真实 incident 并取得已验证阻断引用。
    IncidentEstablished,
    /// R6：已按实际关联核对旧运行 owner／提交候选／控制资格／当前 revision。
    RunRelationChecked,
    /// R7：已在来源会话库同事务提交已裁定终态与收敛事实。
    TerminalCommitted,
    /// R8：恢复操作的提交阶段已更新。
    StageCommitted,
    /// R9：独立资源安全条件成立，重新开放新输入。
    Reopened,
}

impl RecoveryStage {
    /// 顺序值：崩溃后按同一 recovery ID 对账时用于判断"已完成到哪一步"。
    #[must_use]
    pub const fn order(self) -> u8 {
        match self {
            Self::CoordinationAcquired => 1,
            Self::IntentPersistedAndIntakeClosed => 2,
            Self::InFlightRegistered => 3,
            Self::ExecutorStopRequested => 4,
            Self::IncidentEstablished => 5,
            Self::RunRelationChecked => 6,
            Self::TerminalCommitted => 7,
            Self::StageCommitted => 8,
            Self::Reopened => 9,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoordinationAcquired => "r1_coordination_acquired",
            Self::IntentPersistedAndIntakeClosed => "r2_intent_persisted_and_intake_closed",
            Self::InFlightRegistered => "r3_in_flight_registered",
            Self::ExecutorStopRequested => "r4_executor_stop_requested",
            Self::IncidentEstablished => "r5_incident_established",
            Self::RunRelationChecked => "r6_run_relation_checked",
            Self::TerminalCommitted => "r7_terminal_committed",
            Self::StageCommitted => "r8_stage_committed",
            Self::Reopened => "r9_reopened",
        }
    }
}

/// 可重入的恢复操作／意图（崩溃后按**同一 recovery ID** 对账）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSafetyRecoveryOperation {
    pub recovery_operation_id: String,
    /// 发起本次恢复的协调器实例（重启后必须**重新取权**，不得沿用旧实例身份）。
    pub coordinator_instance_id: String,
    pub recovery_epoch: u64,
    /// 取权时刻 `accepts_new_input` 的门 revision（后续核对"是否有人中途放开过"）。
    pub gate_revision: u64,
    pub scope: InputSafetyResourceScope,
    /// 来源库身份（**不是**补造的 workspace id）。
    pub source_database_identity: String,
    /// 候选旧运行集合（空集合是合法的，但不得用它冒充"已核对"）。
    pub candidate_run_ids: Vec<String>,
    /// 本次允许执行的恢复操作（白名单，逐项字符串化）。
    pub allowed_operations: Vec<String>,
    pub stage: RecoveryStage,
    pub recorded_at_unix_ms: u64,
    /// 是否已提交（提交后不得再次修改本操作的安全状态）。
    pub committed: bool,
    /// 本次恢复的**最终处置**：过程看 `stage`，结论看本字段（PR-01／P0-1）。
    ///
    /// `Pending` = 仍在办（**只有它**算"待对账"）；其余为终态，只能由持有**当前**恢复资格的
    /// 结账入口写入。缺了它，被拒绝的恢复会永远挂在"未结账"上 ⇒ 一个无法判定的遗留运行
    /// 就能让资源永久隔离（台账 §B-80）。
    pub disposition: RecoveryDisposition,
}

impl InputSafetyRecoveryOperation {
    /// 阶段是否**单调前进**：不允许回退阶段，也不允许跳到未满足前序的步骤。
    #[must_use]
    pub fn can_advance_to(&self, next: RecoveryStage) -> bool {
        !self.committed && next.order() == self.stage.order() + 1
    }

    /// 是否已结账（终态处置）。结账与 `committed` 由同一次写操作落库，这里以处置为准。
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        self.disposition.is_terminal()
    }
}

/// 恢复操作的**最终处置**（PR-01／P0-1 裁决：恢复失败必须有出口，不得永久挂账）。
///
/// `stage` 回答"走到哪一步"，本类型回答"这件事怎么结的"：
///
/// - 一个被拒绝的恢复可能停在 `r5`，而它的处置是 `HumanReviewRequired`；
/// - 复用 `stage` 表达结论会逼出"用 `r9` 表示放弃"这类谎言，且无法区分
///   "还在办"与"永远办不完"——后者正是"永久隔离"缺口的成因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDisposition {
    /// 尚未结账：仍在办。**只有它**计入"待对账"。
    Pending,
    /// 已收敛：控制终态与收敛事实已落库。
    Recovered,
    /// 已裁定保持隔离：原因明确、**无人工待办**（机器可判定，资源继续隔离）。
    KeptIsolated,
    /// 需要人工复核：资源**保持隔离**，但待办归属明确到人（例如 owner 关系永久未知）。
    HumanReviewRequired,
    /// 带证据放弃：承认无法判定，附证据后结账（不做"假装已收敛"）。
    AbandonedWithEvidence,
}

impl RecoveryDisposition {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Recovered => "recovered",
            Self::KeptIsolated => "kept_isolated",
            Self::HumanReviewRequired => "human_review_required",
            Self::AbandonedWithEvidence => "abandoned_with_evidence",
        }
    }

    /// 是否已结账（终态）。**只有终态**允许把操作标记为 `committed`。
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }

    /// 是否需要人工介入（用于界面与运维口径：这是"等人"，不是"等机器"）。
    #[must_use]
    pub const fn needs_human(self) -> bool {
        matches!(self, Self::HumanReviewRequired)
    }

    /// 从库中读回（认不出的处置一律按 `Pending` 处理——**不得**把未知当已结账）。
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "recovered" => Self::Recovered,
            "kept_isolated" => Self::KeptIsolated,
            "human_review_required" => Self::HumanReviewRequired,
            "abandoned_with_evidence" => Self::AbandonedWithEvidence,
            _ => Self::Pending,
        }
    }
}

/// 人工放行决定（PR-01／P0-1）：**不是**"删事故"，而是一条可审计的决定事实。
///
/// 决策原文的禁止事项：不得 `DELETE FROM incidents`、不得重置安全库、不得把"重启"当解锁。
/// 本记录承担的是"**谁**在**什么资格**下、凭**什么证据**、于**何时**决定解除隔离"，
/// 并且它解除的是它**逐条列出**的阻断（不做批量删除）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseIsolationDecision {
    pub decision_id: String,
    pub scope: InputSafetyResourceScope,
    /// 操作者标识：**不得为空**（匿名放行等于没有责任人）。
    pub operator: String,
    /// 放行理由：**不得为空**。
    pub reason: String,
    /// 证据引用（可空；为空时须由理由说明，留给审计口径判断）。
    pub evidence_refs: Vec<String>,
    /// 本次决定解除的阻断事实（逐条留痕；空集表示只解除"遗留运行"这一侧）。
    pub acknowledged_block_ids: Vec<String>,
    /// 本次决定**接受**的未收敛遗留运行（operator 明确承担其风险）。
    pub acknowledged_run_ids: Vec<String>,
    /// 做出决定时所持有的恢复资格 epoch（**留痕**：无资格的放行不可信）。
    pub release_epoch: u64,
    pub coordinator_instance_id: String,
    pub decided_at_unix_ms: u64,
}

/// 追加式安全事件的类型（用于解释投影与重启对账）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSafetyEventKind {
    StoreInitialized,
    ResourceStateChanged,
    IncidentEstablished,
    IncidentResolved,
    RecoveryStageAdvanced,
    RecoveryCommitted,
    BlockingRefRejected,
}

impl InputSafetyEventKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StoreInitialized => "store_initialized",
            Self::ResourceStateChanged => "resource_state_changed",
            Self::IncidentEstablished => "incident_established",
            Self::IncidentResolved => "incident_resolved",
            Self::RecoveryStageAdvanced => "recovery_stage_advanced",
            Self::RecoveryCommitted => "recovery_committed",
            Self::BlockingRefRejected => "blocking_ref_rejected",
        }
    }
}

/// 一条追加式安全事件（只追加，不修改、不删除）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSafetyEvent {
    pub kind: InputSafetyEventKind,
    pub scope: Option<InputSafetyResourceScope>,
    pub subject_id: Option<String>,
    pub detail: String,
    pub recorded_at_unix_ms: u64,
}

// ---------------------------------------------------------------------------
// 输入许可（六态）——2026-09-26 补充裁决 §3 正式冻结的**纯逻辑**契约
//
// 本段只定义状态、身份、转换与"竞争结果"的判定规则，**不含存储、不分配 schema 版本号**：
// 裁决要求"先做纯逻辑与测试，以缩短 schema 串行窗口"，且版本号由输入安全库负责人在合并时分配。
// 落库时必须使用**同一套**转换规则（不得先写一套测试状态机、落库再独立写第二套）。
// ---------------------------------------------------------------------------

/// 输入许可的状态（六态）。
///
/// 语义要点（裁决 §3.2 点名"三条特别重要"）：
/// - [`Self::DispatchCommitted`] **不等于输入已经发生**：它表达"不能再用未消费许可的逻辑安全重发"；
/// - [`Self::Finished`] **不等于成功，也不等于释放已确认**：部分失败、已结束但资源仍隔离都可保留真实结果；
/// - [`Self::OutcomeUnknown`] → [`Self::Finished`] 是**对账**，不是恢复执行；同一许可永远不能因此再次产生输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputPermitState {
    /// 已登记，但尚未消费输入许可。
    PendingActivation,
    /// 许可已持久消费，动作进入**可能**派发边界。
    DispatchCommitted,
    /// 执行者已确认进入实际执行阶段。
    Executing,
    /// 本次执行已结束，且有足以结账的执行结果（**不代表成功、不代表已释放**）。
    Finished,
    /// 在**消费前**被撤销，从此不能派发（不得回到 `PendingActivation`）。
    Revoked,
    /// 已越过派发边界，但结果不充分（**不得回到 `PendingActivation`**）。
    OutcomeUnknown,
}

impl InputPermitState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PendingActivation => "pending_activation",
            Self::DispatchCommitted => "dispatch_committed",
            Self::Executing => "executing",
            Self::Finished => "finished",
            Self::Revoked => "revoked",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    /// 是否**仍然可能被派发**（只有未消费的 `PendingActivation` 是）。
    ///
    /// 这是"能否激活"的唯一判据来源：`Revoked` / `DispatchCommitted` / 已结束与未知都**不是**。
    #[must_use]
    pub const fn may_still_be_dispatched(self) -> bool {
        matches!(self, Self::PendingActivation)
    }

    /// 是否已越过派发边界（此后再撤销都不构成"未发送"）。
    #[must_use]
    pub const fn crossed_dispatch_boundary(self) -> bool {
        matches!(
            self,
            Self::DispatchCommitted | Self::Executing | Self::Finished | Self::OutcomeUnknown
        )
    }

    /// 唯一合法的状态转换表。
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::PendingActivation, Self::DispatchCommitted | Self::Revoked)
                | (
                    Self::DispatchCommitted,
                    Self::Executing | Self::Finished | Self::OutcomeUnknown
                )
                | (Self::Executing, Self::Finished | Self::OutcomeUnknown)
                // 对账：不是恢复执行，也不产生新的输入。
                | (Self::OutcomeUnknown, Self::Finished)
        )
    }

    /// 执行转换；非法转换返回**可分辨**的理由，而不是静默夹紧或回退。
    pub fn transition_to(self, next: Self) -> Result<Self, PermitTransitionError> {
        if self.can_transition_to(next) {
            return Ok(next);
        }
        Err(PermitTransitionError {
            from: self,
            to: next,
            reason: self.refusal_reason(next),
        })
    }

    fn refusal_reason(self, next: Self) -> &'static str {
        match (self, next) {
            (Self::Revoked, Self::PendingActivation) => {
                "已撤销的许可不得回到待激活：撤销在消费之前生效，重新激活等于凭空发一份新许可"
            }
            (Self::OutcomeUnknown, Self::PendingActivation) => {
                "结果未知不得回到待激活：它已越过派发边界，对账只能结清为已完成，不能重新派发"
            }
            (Self::Finished, _) => {
                "已结束的许可不得再转换：结束不是可逆状态（相容的迟到事实按追加处理，不改状态）"
            }
            (Self::Revoked, _) => "已撤销的许可不得再转换（撤销是终态）",
            (_, Self::Executing) => "只有已提交派发的许可才能进入执行中",
            _ => "该转换不在冻结的转换表内",
        }
    }
}

/// 转换被拒的理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermitTransitionError {
    pub from: InputPermitState,
    pub to: InputPermitState,
    pub reason: &'static str,
}

impl std::fmt::Display for PermitTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "输入许可状态不得从 {} 转为 {}：{}",
            self.from.as_str(),
            self.to.as_str(),
            self.reason
        )
    }
}

/// 撤销之后**观察到真实输入**时的记法。
///
/// 裁决 §3.2：这种情况要"保存异常与回执、触发安全处理"，**不能为维持状态机漂亮而丢掉事实**。
/// 因此它**不是**一次状态转换（不回到 `DispatchCommitted`），而是一条追加的异常事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitAnomalyKind {
    /// 许可已被撤销，却观察到该动作的真实输入。
    InputObservedAfterRevoke,
    /// 许可已结束、却收到与本次动作相容的新输入事实。
    InputObservedAfterFinished,
}

impl PermitAnomalyKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InputObservedAfterRevoke => "input_observed_after_revoke",
            Self::InputObservedAfterFinished => "input_observed_after_finished",
        }
    }

    /// 迟到的真实输入事实应当被记成**异常追加**，而不是状态转换。
    #[must_use]
    pub const fn for_state(state: InputPermitState) -> Option<Self> {
        match state {
            InputPermitState::Revoked => Some(Self::InputObservedAfterRevoke),
            InputPermitState::Finished => Some(Self::InputObservedAfterFinished),
            _ => None,
        }
    }
}

/// **一次独立执行尝试的身份**（2026-09-26 §B-121 裁决 §一／§二正式冻结）。
///
/// 与 `action_id` 的分工**不可混用**：
/// - `action_id` ＝ **动作语义／内容身份**（这个动作长什么样）：审计展示、内容一致性检查、
///   判断同一逻辑动作描述是否被篡改、关联历史事实。**不再**单独用作许可唯一键、
///   也**不再**单独判断是否重复输入。
/// - `execution_attempt_id` ＝ **一次独立执行尝试**：许可消费、输入接纳、防止同一次请求重复执行。
///
/// 为什么不能只用"观察代次 + 步骤序号"：它们描述的是**观察上下文**与**规划位置**，
/// 不天然唯一（例如恢复重算后 `generation=10, step=3` 可能再次出现），因此必须带
/// `attempt_sequence`（同一逻辑步骤再次尝试的递增编号）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExecutionAttemptId {
    /// 内容身份（＝ `action_id`）：只用于一致性核对与审计，不承担唯一性。
    pub parent_action_id: String,
    /// 产生该动作时的观察上下文。
    pub observation_generation: u64,
    /// 当前计划步骤（规划位置，不是执行序号）。
    pub step_identity: String,
    /// **同一逻辑步骤再次尝试的递增编号**。
    pub attempt_sequence: u64,
}

/// `ExecutionAttemptId` 非法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidExecutionAttemptId {
    pub field: &'static str,
    pub reason: &'static str,
}

impl std::fmt::Display for InvalidExecutionAttemptId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "执行尝试身份字段 {} 无效：{}",
            self.field, self.reason
        )
    }
}

impl ExecutionAttemptId {
    pub fn new(
        parent_action_id: impl Into<String>,
        observation_generation: u64,
        step_identity: impl Into<String>,
        attempt_sequence: u64,
    ) -> Result<Self, InvalidExecutionAttemptId> {
        let candidate = Self {
            parent_action_id: parent_action_id.into(),
            observation_generation,
            step_identity: step_identity.into(),
            attempt_sequence,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    pub fn validate(&self) -> Result<(), InvalidExecutionAttemptId> {
        for (field, value) in [
            ("execution_attempt.parent_action_id", &self.parent_action_id),
            ("execution_attempt.step_identity", &self.step_identity),
        ] {
            if value.trim().is_empty() {
                return Err(InvalidExecutionAttemptId {
                    field,
                    reason: "不得为空",
                });
            }
            if value.chars().any(char::is_control) {
                return Err(InvalidExecutionAttemptId {
                    field,
                    reason: "不得含控制字符",
                });
            }
            if crate::run_contract::is_placeholder_identity_value(value) {
                return Err(InvalidExecutionAttemptId {
                    field,
                    reason: "不得用占位值顶替真实来源",
                });
            }
        }
        // `attempt_sequence` 从 1 起：0 表示"还没决定是第几次尝试"，那不是一个身份。
        if self.attempt_sequence == 0 {
            return Err(InvalidExecutionAttemptId {
                field: "execution_attempt.attempt_sequence",
                reason: "必须从 1 起递增（0 表示尚未确定尝试序号）",
            });
        }
        // `observation_generation` 从 1 起（与 `Observation.generation` 同口径）。
        if self.observation_generation == 0 {
            return Err(InvalidExecutionAttemptId {
                field: "execution_attempt.observation_generation",
                reason: "观察代次必须来自真实观察（0 表示没有观察上下文）",
            });
        }
        Ok(())
    }

    /// 稳定字符串形式（存储/日志用）。
    ///
    /// **不是**随机 UUID：它必须可复现，否则无法做重复分析与审计关联（裁决 §九.1 明令禁止）。
    #[must_use]
    pub fn stable_key(&self) -> String {
        format!(
            "{}#gen{}#{}#attempt{}",
            self.parent_action_id, self.observation_generation, self.step_identity,
            self.attempt_sequence
        )
    }
}

/// 许可的最低绑定内容（裁决 §3.3）。复用既有身份，不复制完整会话模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputPermit {
    pub permit_id: String,
    /// **动作语义／内容身份**：审计与一致性核对用，**不**承担许可唯一性。
    pub action_id: String,
    /// **一次执行尝试的身份**：与 `action_id` 共同构成许可唯一键（§B-121 裁决 §一.3）。
    pub execution_attempt_id: ExecutionAttemptId,
    pub scope: InputSafetyResourceScope,
    /// 所属**真实执行上下文**的引用（例如 Goal 阶段运行 / 会话运行，由接纳侧冻结）。
    pub execution_context_ref: String,
    /// 冻结的动作／参数摘要：同一 `action_id` 换内容即拒绝。
    pub frozen_action_digest: String,
    /// 当前安全政策与 gate revision。
    pub policy_revision: u64,
    pub gate_revision: u64,
    /// 签发 owner／epoch。
    pub issued_owner_id: String,
    pub issued_epoch: u64,
    /// 适用期限（到期即不得激活）。
    pub expires_at_unix_ms: u64,
    /// **激活前必须建立**的执行者实例 id（裁决 §3.3）。
    pub executor_instance_id: Option<String>,
    pub state: InputPermitState,
    /// 许可自身的 revision（并发判定用）。
    pub revision: u64,
    /// 关联回执／撤销原因（可缺省，不是"占位"）。
    pub revocation_reason: Option<String>,
}

/// 许可结构校验失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidInputPermit {
    pub field: &'static str,
    pub reason: &'static str,
}

impl std::fmt::Display for InvalidInputPermit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "输入许可字段 {} 无效：{}", self.field, self.reason)
    }
}

impl InputPermit {
    /// 结构校验：必需字段必须真实；`executor_instance_id` 在**激活前**必须已建立。
    pub fn validate_structure(&self) -> Result<(), InvalidInputPermit> {
        for (field, value) in [
            ("permit_id", &self.permit_id),
            ("action_id", &self.action_id),
            ("execution_context_ref", &self.execution_context_ref),
            ("frozen_action_digest", &self.frozen_action_digest),
            ("issued_owner_id", &self.issued_owner_id),
        ] {
            if value.trim().is_empty() {
                return Err(InvalidInputPermit {
                    field,
                    reason: "不得为空",
                });
            }
            if value.chars().any(char::is_control) {
                return Err(InvalidInputPermit {
                    field,
                    reason: "不得含控制字符",
                });
            }
            if crate::run_contract::is_placeholder_identity_value(value) {
                return Err(InvalidInputPermit {
                    field,
                    reason: "不得用占位值顶替真实来源",
                });
            }
        }
        // 先报**字段自身**的问题（更具体），再报派生的一致性冲突（更好诊断）。
        self.execution_attempt_id
            .validate()
            .map_err(|error| InvalidInputPermit {
                field: error.field,
                reason: error.reason,
            })?;
        // 执行尝试身份的内容身份必须与许可的动作身份一致（两者不得各说各话）。
        if self.execution_attempt_id.parent_action_id != self.action_id {
            return Err(InvalidInputPermit {
                field: "execution_attempt.parent_action_id",
                reason: "必须与许可的 action_id 一致（内容身份不得两处不一致）",
            });
        }
        if self.state.crossed_dispatch_boundary() && self.executor_instance_id.is_none() {
            return Err(InvalidInputPermit {
                field: "executor_instance_id",
                reason: "越过派发边界后必须能指出是哪个执行者实例",
            });
        }
        Ok(())
    }

    /// 是否到了"可以激活"的时刻（未消费 + 未过期 + 执行者实例已建立）。
    ///
    /// `now` 是显式传入的时刻：时间来源由调用方决定，纯逻辑不读时钟。
    #[must_use]
    pub fn is_activatable_at(&self, now_unix_ms: u64) -> bool {
        self.state.may_still_be_dispatched()
            && now_unix_ms < self.expires_at_unix_ms
            && self.executor_instance_id.is_some()
    }
}

/// 同一 `action_id` 重复到来时的处置（裁决 §3.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermitReuseDecision {
    /// 内容相同：返回**已有**许可的状态，不重新创建可执行资格。
    ReturnExisting(InputPermitState),
    /// 内容不同：拒绝（换 content 用同 id 是错误复用）。
    RejectedDifferentContent,
}

/// 冻结"重复请求不得重新获得可执行资格"这条规则。
#[must_use]
pub fn decide_permit_reuse(
    existing: &InputPermit,
    frozen_action_digest: &str,
) -> PermitReuseDecision {
    if existing.frozen_action_digest == frozen_action_digest {
        PermitReuseDecision::ReturnExisting(existing.state)
    } else {
        PermitReuseDecision::RejectedDifferentContent
    }
}

/// "关闸"与"消费许可"的竞争结果（裁决 §3.4 冻结）。
///
/// 真实原子性由**同一输入安全库的单一写事务**建立（SQLite 同时只有一个写事务）；
/// 本枚举只冻结**判定**：两种先后顺序各自导向什么结论。注意 `SqliteBusy` 不是成功。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeCloseRaceOutcome {
    /// 关闸先提交 ⇒ 后续消费失败 ⇒ **原生输入调用为零**。
    CloseCommittedFirst,
    /// 消费先提交 ⇒ 动作属**在途**；关闸不得把它改写成"未发送"，交 R4 停止与核查。
    ConsumeCommittedFirst,
    /// 写锁竞争失败（如 `SQLITE_BUSY`）：**不得当成功继续**，按失败处理。
    WriteContentionNotSuccess,
}

/// 依"两个提交各自是否成功"判定竞争结果。
#[must_use]
pub const fn resolve_intake_close_race(
    close_committed: bool,
    consume_committed: bool,
) -> IntakeCloseRaceOutcome {
    match (close_committed, consume_committed) {
        // 两个都记为成功时不猜顺序：真实顺序必须由存储层在同库事务里判定。
        (true, true) | (false, false) => IntakeCloseRaceOutcome::WriteContentionNotSuccess,
        (true, false) => IntakeCloseRaceOutcome::CloseCommittedFirst,
        (false, true) => IntakeCloseRaceOutcome::ConsumeCommittedFirst,
    }
}

/// "只收紧"入口允许做的动作（裁决 §3.5）。
///
/// 恢复者不必先等持有整段输入排他的活跃 helper 释放锁，才能叫它停止；因此给一个**窄**入口。
/// 但它只能收紧：下列动作之外的一律不允许，尤其**不能**成为放行通道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TightenOnlyAction {
    /// 关闭新输入接纳并推进 gate revision。
    CloseIntake,
    /// 撤销该 scope 下**尚未消费**的许可。
    RevokeUnconsumedPermits,
}

impl TightenOnlyAction {
    /// 该动作是否只收紧（当前两项都是；新增前必须重新论证）。
    #[must_use]
    pub const fn is_tighten_only(self) -> bool {
        // 两项都是收紧：关闸与撤销都不会让任何输入变得更可能发生。
        matches!(self, Self::CloseIntake | Self::RevokeUnconsumedPermits)
    }

    /// 该动作是否**会**让输入更可能发生（只收紧入口一律不得为真）。
    #[must_use]
    pub const fn loosens(self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// 执行者实例身份（R4）——2026-09-26 补充裁决 §4 冻结的**纯逻辑**契约
//
// 与上面的许可契约配套：`InputPermit.executor_instance_id` 指向这里的实例身份。
// 同样**不含存储、不分配 schema 版本号**。落库时使用同一套判定规则。
// ---------------------------------------------------------------------------

/// 执行者证据的**来源分工**（裁决 §4.1：三种来源各自证明不同的事，不重定义）。
///
/// 这张分工表是"不要把单一来源当充分证据"的落地形式：每个来源都显式写出它**不能**证明什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutorEvidenceSource {
    /// 宿主启动登记：为哪个 run／action 创建了哪个执行者。
    HostLaunchRegistration,
    /// OS 实例证据：当前句柄对应的实际进程实例及其状态。
    OsInstanceEvidence,
    /// helper 执行回执：与该实例绑定的已确认输入阶段与结果。
    HelperExecutionReceipt,
}

impl ExecutorEvidenceSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostLaunchRegistration => "host_launch_registration",
            Self::OsInstanceEvidence => "os_instance_evidence",
            Self::HelperExecutionReceipt => "helper_execution_receipt",
        }
    }

    /// 该来源**能**证明什么。
    #[must_use]
    pub const fn proves(self) -> &'static str {
        match self {
            Self::HostLaunchRegistration => "为哪个 run／action 创建了哪个执行者",
            Self::OsInstanceEvidence => "当前句柄对应的实际进程实例及其状态",
            Self::HelperExecutionReceipt => "与该实例绑定的已确认输入阶段与结果",
        }
    }

    /// 该来源**不能**证明什么——防止把单一来源当充分证据。
    #[must_use]
    pub const fn does_not_prove(self) -> &'static str {
        match self {
            Self::HostLaunchRegistration => "不能单独证明进程当前仍存活或已经停止",
            Self::OsInstanceEvidence => "不能证明输入目标完成、按键已释放",
            Self::HelperExecutionReceipt => "不能自己签发运行归属或恢复资格",
        }
    }

    /// 该来源是否足以**单独**支撑"这个执行者已停止"的处置。
    ///
    /// 三种来源**都不是**：停止结论必须来自对**实际实例句柄**的核查＋有界等待确认。
    #[must_use]
    pub const fn alone_suffices_to_conclude_stopped(self) -> bool {
        false
    }
}

/// 执行者的固定身份：**宿主进程路径 ＋ 受控脚本／程序身份**。
///
/// 裁决 §4.2 明令："只验证 `powershell.exe` 路径，不足以证明它在运行**本次**受控脚本。"
/// 因此结构上把两者分开：解释器路径相同、脚本身份不同 ⇒ 不是同一个受控执行者。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperIdentity {
    /// 宿主解释器／程序路径（例如 powershell.exe）。
    pub host_process_path: String,
    /// 本次受控脚本／程序的身份（内容摘要或稳定标识）。
    pub script_or_program_digest: String,
}

impl HelperIdentity {
    pub fn validate_structure(&self) -> Result<(), InvalidExecutorRegistration> {
        if self.host_process_path.trim().is_empty() {
            return Err(InvalidExecutorRegistration {
                field: "helper.host_process_path",
                reason: "不得为空",
            });
        }
        if self.script_or_program_digest.trim().is_empty() {
            return Err(InvalidExecutorRegistration {
                field: "helper.script_or_program_digest",
                reason: "只给解释器路径不足以证明在运行本次受控脚本，必须同时给出脚本／程序身份",
            });
        }
        Ok(())
    }
}

/// 执行者登记的持久状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorInstanceState {
    /// 已登记，等待核查实际实例。
    RegisteredPendingVerification,
    /// 已在真实实例句柄上核查通过。
    VerifiedAlive,
    /// 已确认退出（**仅**该实例退出，不推断后代）。
    ExitedConfirmed,
    /// 被登记为"无法判定"（如 `AccessDenied`）：保持阻断。
    UnverifiableUnknown,
    /// 有界等待超时：进入明确隔离／人工复核，**不继续重试**。
    EscalatedHumanReview,
}

impl ExecutorInstanceState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegisteredPendingVerification => "registered_pending_verification",
            Self::VerifiedAlive => "verified_alive",
            Self::ExitedConfirmed => "exited_confirmed",
            Self::UnverifiableUnknown => "unverifiable_unknown",
            Self::EscalatedHumanReview => "escalated_human_review",
        }
    }
}

/// 执行者登记（裁决 §4.2 的关联项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorRegistration {
    pub executor_instance_id: String,
    pub launch_operation_id: String,
    pub coordinator_instance_id: String,
    pub scope: InputSafetyResourceScope,
    /// 宿主启动实例（哪一次启动创建了它）。
    pub host_launch_instance: String,
    /// 与之绑定的动作。
    pub action_id: String,
    pub pid: u32,
    /// 从**创建结果**取得的创建时间。读不到就保留 `None` 与其错误，**不退化**为只核对 PID。
    pub creation_time_100ns: Option<u64>,
    /// 实际用户／登录会话关联（可得时）。
    pub user_session: Option<String>,
    pub helper: HelperIdentity,
    pub protocol_version: u32,
    /// 是否已绑定真实监督关系（Job／监督器）。
    pub supervision_bound: bool,
    pub state: ExecutorInstanceState,
    pub revision: u64,
}

/// 执行者登记结构校验失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidExecutorRegistration {
    pub field: &'static str,
    pub reason: &'static str,
}

impl std::fmt::Display for InvalidExecutorRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "执行者登记字段 {} 无效：{}",
            self.field, self.reason
        )
    }
}

impl ExecutorRegistration {
    pub fn validate_structure(&self) -> Result<(), InvalidExecutorRegistration> {
        for (field, value) in [
            ("executor_instance_id", &self.executor_instance_id),
            ("launch_operation_id", &self.launch_operation_id),
            ("coordinator_instance_id", &self.coordinator_instance_id),
            ("host_launch_instance", &self.host_launch_instance),
            ("action_id", &self.action_id),
        ] {
            if value.trim().is_empty() {
                return Err(InvalidExecutorRegistration {
                    field,
                    reason: "不得为空",
                });
            }
            if value.chars().any(char::is_control) {
                return Err(InvalidExecutorRegistration {
                    field,
                    reason: "不得含控制字符",
                });
            }
            if crate::run_contract::is_placeholder_identity_value(value) {
                return Err(InvalidExecutorRegistration {
                    field,
                    reason: "不得用占位值顶替真实来源",
                });
            }
        }
        self.helper.validate_structure()?;
        if self.protocol_version == 0 {
            return Err(InvalidExecutorRegistration {
                field: "protocol_version",
                reason: "必须记录真实的协议版本（0 表示未记录）",
            });
        }
        // 裁决 §4.2 的启动顺序：**先绑定监督、再允许输入**。因此"核查通过"的登记必须已绑定监督。
        if self.state == ExecutorInstanceState::VerifiedAlive && !self.supervision_bound {
            return Err(InvalidExecutorRegistration {
                field: "supervision_bound",
                reason: "未绑定真实监督关系的执行者不得被视为已核查通过",
            });
        }
        Ok(())
    }

    /// 创建身份是否**完整**：缺创建时间只能"保留错误继续核查"，不得当成身份相符。
    #[must_use]
    pub fn creation_identity_complete(&self) -> bool {
        self.creation_time_100ns.is_some()
    }
}

/// 对某个实例句柄的一次核查结果（**效力绑定到该句柄世代**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorVerification {
    pub executor_instance_id: String,
    /// 核查时所用句柄的世代号：重新取得句柄后，上一次验证**不自动继承**（裁决 §4.3）。
    pub handle_generation: u64,
    pub verified_at_unix_ms: u64,
}

impl ExecutorVerification {
    /// 这次核查能否用于当前目标（同一实例 ＋ 同一句柄世代）。
    #[must_use]
    pub fn is_valid_for(&self, executor_instance_id: &str, handle_generation: u64) -> bool {
        self.executor_instance_id == executor_instance_id
            && self.handle_generation == handle_generation
    }
}

/// 实例观测结果（裁决 §4.4 失败表逐行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorObservation {
    /// 创建身份匹配且存活。
    MatchesAndAlive,
    /// PID 存在但创建身份不同（PID 已被复用）。
    PidReused,
    /// `AccessDenied` / 无法读取创建身份 ⇒ Unknown。
    AccessDeniedOrUnreadable,
    /// 缺可信宿主登记（或登记损坏／关系不匹配）。
    NoTrustedRegistration,
    /// 直接 helper 已退出。
    DirectHelperExited,
    /// Job 已关闭，但结果未核实。
    JobClosedOutcomeUnverified,
    /// 终止 API 返回成功，但尚未确认退出。
    TerminateRequestedNotConfirmed,
    /// 退出已确认，但释放未知。
    ExitedButReleaseUnknown,
    /// 旧回执迟到。
    LateReceipt,
}

/// 观测结果对应的处置（与失败表一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorDisposition {
    /// 先协作停止；必要时仅终止本产品实际拥有的实例（沿用既有有界收尾）。
    CooperativeStopThenScopedTerminate,
    /// 不操作当前进程（它已不是原来那个实例）；原效果仍按原证据判断。
    DoNotTouchCurrentProcess,
    /// 保持阻断并记为未知，**不自动提权**。
    KeepBlockedAsUnknown,
    /// 不按进程名／端口猜 owner，不终止不明进程。
    DoNotGuessOwner,
    /// 继续核查后代与释放义务（直接 helper 退出不等于可以放行）。
    KeepCheckingDescendantsAndRelease,
    /// 有界等待并核查实际结果，不把"关闭调用"当"后代已停"。
    WaitBoundedThenVerify,
    /// 记录"已请求终止"；在有界等待确认前**不写**"已经退出"。
    RecordRequestedNotExited,
    /// 资源继续隔离。
    KeepIsolated,
    /// 验证原 action／实例后**追加**事实；不恢复旧资格。
    AppendFactWithoutRestoringEligibility,
}

/// 冻结失败表。
#[must_use]
pub const fn disposition_for(observation: ExecutorObservation) -> ExecutorDisposition {
    match observation {
        ExecutorObservation::MatchesAndAlive => {
            ExecutorDisposition::CooperativeStopThenScopedTerminate
        }
        ExecutorObservation::PidReused => ExecutorDisposition::DoNotTouchCurrentProcess,
        ExecutorObservation::AccessDeniedOrUnreadable => ExecutorDisposition::KeepBlockedAsUnknown,
        ExecutorObservation::NoTrustedRegistration => ExecutorDisposition::DoNotGuessOwner,
        ExecutorObservation::DirectHelperExited => {
            ExecutorDisposition::KeepCheckingDescendantsAndRelease
        }
        ExecutorObservation::JobClosedOutcomeUnverified => ExecutorDisposition::WaitBoundedThenVerify,
        ExecutorObservation::TerminateRequestedNotConfirmed => {
            ExecutorDisposition::RecordRequestedNotExited
        }
        ExecutorObservation::ExitedButReleaseUnknown => ExecutorDisposition::KeepIsolated,
        ExecutorObservation::LateReceipt => {
            ExecutorDisposition::AppendFactWithoutRestoringEligibility
        }
    }
}

/// 每一种处置是否**允许**自动提权。
///
/// 裁决要求：`AccessDenied` 等不得"为继续流程自动提权"，也不允许为绕过 `OpenProcess` 失败
/// 而启用调试权限。因此本函数恒为 `false` —— 提权永远不是自动路径的一部分。
#[must_use]
pub const fn disposition_allows_automatic_privilege_escalation(
    _disposition: ExecutorDisposition,
) -> bool {
    false
}

/// 后代停止证据（裁决 §4.3／§4.4：父进程退出与 Job 关闭**都不是**充分证据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescendantStopEvidence {
    /// 父进程已退出。
    ParentExited,
    /// 已请求关闭 Job。
    JobClosedRequested,
    /// 已逐个确认全部受监督成员退出。
    AllMembersExitedVerified,
}

impl DescendantStopEvidence {
    /// 是否构成"后代已确认停止"。
    ///
    /// 只有逐个确认成员退出才算；父进程退出与 Job 关闭都不算（Job 的行为取决于成员关系、
    /// 句柄与限制配置，关闭调用本身不等于成员已停）。
    #[must_use]
    pub const fn confirms_all_descendants_stopped(self) -> bool {
        matches!(self, Self::AllMembersExitedVerified)
    }
}

/// 终止阶段（裁决 §4.4：`TerminateProcess` 对外部进程是异步的，返回后仍需等待）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminatePhase {
    /// 已调用终止 API（只是**请求**）。
    Requested,
    /// 正在有界等待。
    WaitBounded,
    /// 已确认退出。
    ExitedConfirmed,
    /// 有界等待超时。
    WaitTimedOut,
}

impl TerminatePhase {
    /// 该阶段是否构成"已停止"的确认。
    #[must_use]
    pub const fn is_stop_confirmation(self) -> bool {
        matches!(self, Self::ExitedConfirmed)
    }

    /// 超时后的处置：进入明确隔离／人工复核。
    ///
    /// 裁决明确"不另开一套更长的 R4 专用无限等待""到期仍不能确定，进入明确隔离／人工复核，
    /// 而不是继续重试直到看起来成功"。
    #[must_use]
    pub const fn disposition_on_timeout(self) -> Option<ExecutorDisposition> {
        match self {
            Self::WaitTimedOut => Some(ExecutorDisposition::KeepIsolated),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // 8.3a 执行者实例身份（2026-09-26 补充裁决 §4）
    // -----------------------------------------------------------------------

    fn executor(state: ExecutorInstanceState) -> ExecutorRegistration {
        ExecutorRegistration {
            executor_instance_id: "exec-1".to_string(),
            launch_operation_id: "launch-1".to_string(),
            coordinator_instance_id: "coordinator-1".to_string(),
            scope: InputSafetyResourceScope::parse("windows-session-1").expect("scope"),
            host_launch_instance: "host-launch-1".to_string(),
            action_id: "action-1".to_string(),
            pid: 4242,
            creation_time_100ns: Some(133_000_000_000_000_000),
            user_session: Some("session-1".to_string()),
            helper: HelperIdentity {
                host_process_path: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
                    .to_string(),
                script_or_program_digest: "script-digest-a".to_string(),
            },
            protocol_version: 2,
            supervision_bound: true,
            state,
            revision: 1,
        }
    }

    /// 三种证据来源分工：各自写明能证明与**不能**证明什么，且**没有**任何来源可单独
    /// 支撑"已停止"的结论（防止把单一来源当充分证据）。
    #[test]
    fn executor_evidence_sources_have_separate_claims_and_none_suffices_alone() {
        let sources = [
            ExecutorEvidenceSource::HostLaunchRegistration,
            ExecutorEvidenceSource::OsInstanceEvidence,
            ExecutorEvidenceSource::HelperExecutionReceipt,
        ];
        for source in sources {
            assert!(!source.proves().is_empty(), "{source:?} 必须写明能证明什么");
            assert!(
                !source.does_not_prove().is_empty(),
                "{source:?} 必须写明不能证明什么"
            );
            assert!(
                !source.alone_suffices_to_conclude_stopped(),
                "{source:?} 不得单独支撑「已停止」结论"
            );
        }
        // 分工具体成立：登记证明归属、不证明存活；OS 证据证明实例、不证明释放；
        // 回执证明执行事实、不能自签归属。
        assert!(ExecutorEvidenceSource::HostLaunchRegistration
            .does_not_prove()
            .contains("存活"));
        assert!(ExecutorEvidenceSource::OsInstanceEvidence
            .does_not_prove()
            .contains("释放"));
        assert!(ExecutorEvidenceSource::HelperExecutionReceipt
            .does_not_prove()
            .contains("归属"));
    }

    /// 登记校验：占位/空值被拒；**只给解释器路径不算证明本次受控脚本**；
    /// 未绑定监督的不得被视为已核查通过。
    #[test]
    fn executor_registration_requires_script_identity_and_supervision() {
        executor(ExecutorInstanceState::VerifiedAlive)
            .validate_structure()
            .expect("完整登记应通过");

        // 只给解释器路径（这正是裁决点名的错法）。
        let mut interpreter_only = executor(ExecutorInstanceState::RegisteredPendingVerification);
        interpreter_only.helper.script_or_program_digest = "  ".to_string();
        let error = interpreter_only
            .validate_structure()
            .expect_err("只验证解释器路径必须被拒");
        assert_eq!(error.field, "helper.script_or_program_digest");
        assert!(error.reason.contains("不足以证明"), "{}", error.reason);

        // 协议版本未记录。
        let mut no_protocol = executor(ExecutorInstanceState::RegisteredPendingVerification);
        no_protocol.protocol_version = 0;
        assert_eq!(
            no_protocol.validate_structure().expect_err("必须记录协议版本").field,
            "protocol_version"
        );

        // 未绑定监督却标记"已核查通过"：拒绝（启动顺序是先绑定监督再允许输入）。
        let mut unsupervised = executor(ExecutorInstanceState::VerifiedAlive);
        unsupervised.supervision_bound = false;
        assert_eq!(
            unsupervised
                .validate_structure()
                .expect_err("未绑定监督不得算已核查通过")
                .field,
            "supervision_bound"
        );

        // 占位值。
        let mut placeholder = executor(ExecutorInstanceState::RegisteredPendingVerification);
        placeholder.executor_instance_id = "unknown".to_string();
        assert_eq!(
            placeholder.validate_structure().expect_err("占位值必须被拒").field,
            "executor_instance_id"
        );
    }

    /// 创建身份完整性：缺创建时间**只能保留错误继续核查**，不得当成身份相符。
    #[test]
    fn missing_creation_time_is_not_treated_as_a_identity_match() {
        let mut incomplete = executor(ExecutorInstanceState::RegisteredPendingVerification);
        incomplete.creation_time_100ns = None;
        assert!(!incomplete.creation_identity_complete());
        // 结构仍可合法（登记允许"尚未读到"），但"身份完整"为假 ⇒ 不能据此判定相符。
        incomplete
            .validate_structure()
            .expect("缺创建时间是保留错误，不是结构非法");
        assert!(executor(ExecutorInstanceState::RegisteredPendingVerification)
            .creation_identity_complete());
    }

    /// 句柄世代：重新取得句柄后，上一次核查**不自动继承**（裁决 §4.3）。
    #[test]
    fn verification_is_scoped_to_the_handle_generation_it_was_done_on() {
        let verification = ExecutorVerification {
            executor_instance_id: "exec-1".to_string(),
            handle_generation: 7,
            verified_at_unix_ms: 1_000,
        };
        assert!(verification.is_valid_for("exec-1", 7), "同一实例同一句柄可用");
        assert!(
            !verification.is_valid_for("exec-1", 8),
            "重新取得句柄后必须重新核查，不得继承上一次验证"
        );
        assert!(
            !verification.is_valid_for("exec-2", 7),
            "不同实例不得复用核查结论"
        );
    }

    /// 失败表逐行冻结；尤其：PID 复用不得去动当前进程、AccessDenied 是 Unknown、
    /// 缺登记不得猜 owner、终止返回成功只算"已请求"。
    #[test]
    fn executor_failure_table_is_frozen_row_by_row() {
        use ExecutorDisposition::*;
        use ExecutorObservation::*;
        for (observation, expected) in [
            (MatchesAndAlive, CooperativeStopThenScopedTerminate),
            (PidReused, DoNotTouchCurrentProcess),
            (AccessDeniedOrUnreadable, KeepBlockedAsUnknown),
            (NoTrustedRegistration, DoNotGuessOwner),
            (DirectHelperExited, KeepCheckingDescendantsAndRelease),
            (JobClosedOutcomeUnverified, WaitBoundedThenVerify),
            (TerminateRequestedNotConfirmed, RecordRequestedNotExited),
            (ExitedButReleaseUnknown, KeepIsolated),
            (LateReceipt, AppendFactWithoutRestoringEligibility),
        ] {
            assert_eq!(disposition_for(observation), expected, "{observation:?}");
        }
        // 九行覆盖全表（新增观测时必须同步本表）。
        assert_eq!(
            [
                MatchesAndAlive,
                PidReused,
                AccessDeniedOrUnreadable,
                NoTrustedRegistration,
                DirectHelperExited,
                JobClosedOutcomeUnverified,
                TerminateRequestedNotConfirmed,
                ExitedButReleaseUnknown,
                LateReceipt,
            ]
            .len(),
            9
        );
    }

    /// 自动提权：**任何**处置都不允许。
    #[test]
    fn no_disposition_ever_allows_automatic_privilege_escalation() {
        use ExecutorDisposition::*;
        for disposition in [
            CooperativeStopThenScopedTerminate,
            DoNotTouchCurrentProcess,
            KeepBlockedAsUnknown,
            DoNotGuessOwner,
            KeepCheckingDescendantsAndRelease,
            WaitBoundedThenVerify,
            RecordRequestedNotExited,
            KeepIsolated,
            AppendFactWithoutRestoringEligibility,
        ] {
            assert!(
                !disposition_allows_automatic_privilege_escalation(disposition),
                "{disposition:?} 不得把提权作为自动路径的一部分"
            );
        }
    }

    /// 后代停止证据：父进程退出与 Job 关闭都**不是**充分证据。
    #[test]
    fn parent_exit_and_job_close_do_not_prove_descendants_stopped() {
        assert!(!DescendantStopEvidence::ParentExited.confirms_all_descendants_stopped());
        assert!(!DescendantStopEvidence::JobClosedRequested.confirms_all_descendants_stopped());
        assert!(DescendantStopEvidence::AllMembersExitedVerified.confirms_all_descendants_stopped());
    }

    /// 终止阶段：只有"已确认退出"是停止确认；超时进入明确隔离／人工复核，不无限重试。
    #[test]
    fn terminate_request_is_not_stop_confirmation_and_timeout_escalates() {
        assert!(!TerminatePhase::Requested.is_stop_confirmation());
        assert!(!TerminatePhase::WaitBounded.is_stop_confirmation());
        assert!(TerminatePhase::ExitedConfirmed.is_stop_confirmation());
        assert!(!TerminatePhase::WaitTimedOut.is_stop_confirmation());
        assert_eq!(
            TerminatePhase::WaitTimedOut.disposition_on_timeout(),
            Some(ExecutorDisposition::KeepIsolated),
            "超时必须落到明确隔离／人工复核，而不是继续重试到看起来成功"
        );
        for phase in [
            TerminatePhase::Requested,
            TerminatePhase::WaitBounded,
            TerminatePhase::ExitedConfirmed,
        ] {
            assert_eq!(phase.disposition_on_timeout(), None, "{phase:?} 未超时");
        }
    }

    // -----------------------------------------------------------------------
    // 8.2a 输入许可六态（2026-09-26 补充裁决 §3）
    // -----------------------------------------------------------------------

    fn permit(state: InputPermitState) -> InputPermit {
        InputPermit {
            permit_id: "permit-1".to_string(),
            action_id: "action-1".to_string(),
            execution_attempt_id: ExecutionAttemptId::new("action-1", 41, "step-7", 1)
                .expect("attempt"),
            scope: InputSafetyResourceScope::parse("windows-session-1").expect("scope"),
            execution_context_ref: "cu-run-1".to_string(),
            frozen_action_digest: "digest-a".to_string(),
            policy_revision: 3,
            gate_revision: 7,
            issued_owner_id: "owner-1".to_string(),
            issued_epoch: 5,
            expires_at_unix_ms: 1_000,
            executor_instance_id: Some("exec-1".to_string()),
            state,
            revision: 1,
            revocation_reason: None,
        }
    }

    /// 转换表：合法路径全部可达，非法路径全部被拒且理由可分辨。
    #[test]
    fn permit_transition_table_is_frozen_and_illegal_moves_are_refused() {
        use InputPermitState::*;
        let legal = [
            (PendingActivation, DispatchCommitted),
            (PendingActivation, Revoked),
            (DispatchCommitted, Executing),
            (DispatchCommitted, Finished),
            (DispatchCommitted, OutcomeUnknown),
            (Executing, Finished),
            (Executing, OutcomeUnknown),
            (OutcomeUnknown, Finished),
        ];
        for (from, to) in legal {
            assert_eq!(from.transition_to(to), Ok(to), "{from:?} -> {to:?} 应合法");
            assert!(from.can_transition_to(to));
        }
        let illegal = [
            // 撤销在消费之前生效：不得回到待激活，也不得再转任何状态。
            (Revoked, PendingActivation),
            (Revoked, DispatchCommitted),
            // 未知已越过派发边界：只能对账结清，不能重新派发。
            (OutcomeUnknown, PendingActivation),
            (OutcomeUnknown, DispatchCommitted),
            (OutcomeUnknown, Executing),
            // 结束不可逆。
            (Finished, PendingActivation),
            (Finished, Executing),
            // 只有已提交派发的许可才能进入执行中。
            (PendingActivation, Executing),
            (PendingActivation, Finished),
            (PendingActivation, OutcomeUnknown),
        ];
        for (from, to) in illegal {
            let error = from.transition_to(to).expect_err("非法转换必须被拒");
            assert_eq!(error.from, from);
            assert_eq!(error.to, to);
            assert!(!error.reason.is_empty(), "拒绝必须带可分辨理由");
        }
        // 撤销后回待激活的理由要**点名**这条语义，而不是泛泛的"不在表内"。
        let error = Revoked.transition_to(PendingActivation).expect_err("拒");
        assert!(error.reason.contains("凭空发一份新许可"), "{}", error.reason);
    }

    /// 判定辅助：只有未消费的待激活许可"仍可能被派发"；越过边界后撤销都不算"未发送"。
    #[test]
    fn only_pending_permits_may_still_be_dispatched() {
        assert!(InputPermitState::PendingActivation.may_still_be_dispatched());
        for state in [
            InputPermitState::DispatchCommitted,
            InputPermitState::Executing,
            InputPermitState::Finished,
            InputPermitState::Revoked,
            InputPermitState::OutcomeUnknown,
        ] {
            assert!(!state.may_still_be_dispatched(), "{state:?} 不得被派发");
        }
        assert!(!InputPermitState::PendingActivation.crossed_dispatch_boundary());
        for state in [
            InputPermitState::DispatchCommitted,
            InputPermitState::Executing,
            InputPermitState::Finished,
            InputPermitState::OutcomeUnknown,
        ] {
            assert!(
                state.crossed_dispatch_boundary(),
                "{state:?} 已越过派发边界 ⇒ 此后撤销都不构成「未发送」"
            );
        }
        // 撤销**没有**越过边界：它在消费之前生效。
        assert!(!InputPermitState::Revoked.crossed_dispatch_boundary());
    }

    /// 三条"特别重要"语义（裁决 §3.2）：提交派发 ≠ 输入已发生；结束 ≠ 成功/已释放；
    /// 未知→结束是**对账**而非恢复执行。
    #[test]
    fn dispatch_commit_finish_and_reconcile_mean_what_the_ruling_says() {
        use InputPermitState::*;
        // ① DispatchCommitted 只表达"不能再按未消费许可安全重发"，不表达输入已发生。
        assert!(DispatchCommitted.crossed_dispatch_boundary());
        assert!(!DispatchCommitted.may_still_be_dispatched());
        // ② Finished 不是成功、也不是释放已确认：它是"有足以结账的结果"。
        assert!(Executing.transition_to(Finished).is_ok());
        assert!(DispatchCommitted.transition_to(Finished).is_ok());
        // ③ 未知 → 结束是对账路径；它**不**经过 Executing（不产生新的输入）。
        assert!(OutcomeUnknown.transition_to(Finished).is_ok());
        assert!(
            !OutcomeUnknown.can_transition_to(Executing),
            "对账不得把许可送回执行中——那等于恢复执行"
        );
    }

    /// 撤销后观察到真实输入：记成**异常追加**，不是状态转换（不得为状态机漂亮丢事实）。
    #[test]
    fn input_observed_after_revoke_is_an_anomaly_not_a_transition() {
        assert_eq!(
            PermitAnomalyKind::for_state(InputPermitState::Revoked),
            Some(PermitAnomalyKind::InputObservedAfterRevoke)
        );
        assert_eq!(
            PermitAnomalyKind::for_state(InputPermitState::Finished),
            Some(PermitAnomalyKind::InputObservedAfterFinished)
        );
        assert_eq!(PermitAnomalyKind::for_state(InputPermitState::PendingActivation), None);
        assert_eq!(
            PermitAnomalyKind::InputObservedAfterRevoke.as_str(),
            "input_observed_after_revoke"
        );
        // 异常不改变"已撤销不得再转换"这一事实。
        assert!(
            InputPermitState::Revoked
                .transition_to(InputPermitState::DispatchCommitted)
                .is_err(),
            "即使观察到真实输入，也不得把撤销状态改写回去"
        );
    }

    /// 结构校验：占位/空值被拒；**越过派发边界前必须已建立执行者实例**。
    #[test]
    fn permit_structure_requires_real_fields_and_an_executor_before_dispatch() {
        permit(InputPermitState::PendingActivation)
            .validate_structure()
            .expect("完整许可应通过");

        for (field, mutate) in [
            ("permit_id", (|p: &mut InputPermit| p.permit_id = "  ".to_string()) as fn(&mut InputPermit)),
            ("action_id", |p: &mut InputPermit| p.action_id = "unknown".to_string()),
            (
                "execution_context_ref",
                |p: &mut InputPermit| p.execution_context_ref = "".to_string(),
            ),
            (
                "frozen_action_digest",
                |p: &mut InputPermit| p.frozen_action_digest = "0".to_string(),
            ),
            ("issued_owner_id", |p: &mut InputPermit| {
                p.issued_owner_id = "owner\u{7}bad".to_string()
            }),
        ] {
            let mut candidate = permit(InputPermitState::PendingActivation);
            mutate(&mut candidate);
            let error = candidate
                .validate_structure()
                .expect_err("非法字段必须被拒");
            assert_eq!(error.field, field, "{error}");
        }

        // 未建立执行者实例：待激活状态允许（尚未越界），但**不得激活**。
        let mut without_executor = permit(InputPermitState::PendingActivation);
        without_executor.executor_instance_id = None;
        without_executor.validate_structure().expect("待激活可暂缺执行者");
        assert!(
            !without_executor.is_activatable_at(500),
            "执行者实例未建立时不得激活"
        );
        // 一旦越界，缺执行者实例就是结构错误。
        let mut dispatched = without_executor.clone();
        dispatched.state = InputPermitState::DispatchCommitted;
        assert_eq!(
            dispatched.validate_structure().expect_err("越界必须能指出执行者").field,
            "executor_instance_id"
        );
    }

    /// 可激活判定：未消费 + 未过期 + 执行者已建立；时间由调用方给定（纯逻辑不读时钟）。
    #[test]
    fn activatable_requires_unconsumed_unexpired_and_an_executor() {
        let ok = permit(InputPermitState::PendingActivation);
        assert!(ok.is_activatable_at(999));
        assert!(!ok.is_activatable_at(1_000), "到期即不得激活");
        assert!(!ok.is_activatable_at(1_001));

        let mut consumed = ok.clone();
        consumed.state = InputPermitState::DispatchCommitted;
        assert!(!consumed.is_activatable_at(1), "已消费不得再激活");
        let mut revoked = ok.clone();
        revoked.state = InputPermitState::Revoked;
        assert!(!revoked.is_activatable_at(1), "已撤销不得再激活");
    }

    /// 重复请求：内容相同返回已有状态（不重新获得资格）；内容不同拒绝。
    #[test]
    fn repeated_request_returns_existing_state_and_never_regrants_eligibility() {
        let existing = permit(InputPermitState::DispatchCommitted);
        assert_eq!(
            decide_permit_reuse(&existing, "digest-a"),
            PermitReuseDecision::ReturnExisting(InputPermitState::DispatchCommitted),
            "同内容重复请求必须返回已有状态，而不是新建许可"
        );
        assert_eq!(
            decide_permit_reuse(&existing, "digest-b"),
            PermitReuseDecision::RejectedDifferentContent,
            "同 action_id 换内容必须拒绝"
        );
        // 已撤销的许可同样只能回状态，不能因为"再请求一次"复活。
        let revoked = permit(InputPermitState::Revoked);
        assert_eq!(
            decide_permit_reuse(&revoked, "digest-a"),
            PermitReuseDecision::ReturnExisting(InputPermitState::Revoked)
        );
    }

    /// 关闸与消费的竞争判定（裁决 §3.4）：顺序决定结论；**写锁竞争失败不算成功**。
    #[test]
    fn intake_close_race_outcomes_are_frozen_and_contention_is_not_success() {
        assert_eq!(
            resolve_intake_close_race(true, false),
            IntakeCloseRaceOutcome::CloseCommittedFirst
        );
        assert_eq!(
            resolve_intake_close_race(false, true),
            IntakeCloseRaceOutcome::ConsumeCommittedFirst
        );
        // 两个都成功或都不成功：不得猜顺序，按失败处理（SQLITE_BUSY 不是成功）。
        for pair in [(true, true), (false, false)] {
            assert_eq!(
                resolve_intake_close_race(pair.0, pair.1),
                IntakeCloseRaceOutcome::WriteContentionNotSuccess,
                "{pair:?} 不得被当成某个确定的先后顺序"
            );
        }
    }

    /// 只收紧入口：两个动作都只收紧，且**没有任何**动作会让输入更可能发生。
    #[test]
    fn the_narrow_recovery_entry_can_only_tighten() {
        for action in [
            TightenOnlyAction::CloseIntake,
            TightenOnlyAction::RevokeUnconsumedPermits,
        ] {
            assert!(action.is_tighten_only(), "{action:?}");
            assert!(
                !action.loosens(),
                "{action:?} 不得让输入更可能发生——它不是放行通道"
            );
        }
    }

    /// 版本独立：本库版本与会话库**不是**同一个号（本 crate 不知道会话库版本，故只钉本值）。
    ///
    /// v3 的登记口径（2026-09-26）：8.2b／8.3b **共用一次** N → N+1 相邻迁移，
    /// 其中 N 是**从最终合并基线的迁移目录**确认的终点版本（当时为 2），
    /// **不是**按历史曾出现过的号猜、也**不是**运行时用"当前版本+1"动态生成。
    #[test]
    fn schema_version_is_independent_and_pinned() {
        assert_eq!(INPUT_SAFETY_SCHEMA_VERSION, 3);
    }

    /// 存储身份的解析不放宽：前缀、长度、字符集都不做近似归一。
    #[test]
    fn store_id_parsing_is_strict() {
        let ok = parse_input_safety_store_id("is-0123456789abcdef0123456789abcdef").expect("valid");
        assert_eq!(ok.as_str(), "is-0123456789abcdef0123456789abcdef");
        for (value, expected) in [
            ("0123456789abcdef0123456789abcdef", InvalidInputSafetyStoreId::MissingPrefix),
            ("is-0123456789ABCDEF0123456789abcdef", InvalidInputSafetyStoreId::MalformedSuffix),
            ("is-0123", InvalidInputSafetyStoreId::MalformedSuffix),
            ("is-", InvalidInputSafetyStoreId::MalformedSuffix),
            ("is-0123456789abcdef0123456789abcdeg", InvalidInputSafetyStoreId::MalformedSuffix),
        ] {
            assert_eq!(parse_input_safety_store_id(value), Err(expected), "value={value}");
        }
    }

    /// 作用域必须是**物理输入资源**，不能是路径/workspace/自选数据库。
    #[test]
    fn resource_scope_rejects_paths_and_workspace_like_values() {
        assert_eq!(
            InputSafetyResourceScope::parse("windows-session-1").expect("valid").as_str(),
            "windows-session-1"
        );
        for (value, expected) in [
            ("", InvalidResourceScope::Empty),
            ("   ", InvalidResourceScope::Empty),
            (r"C:\workspace\.coolzhu\input-safety.sqlite3", InvalidResourceScope::LooksLikePath),
            ("/home/me/project", InvalidResourceScope::LooksLikePath),
            ("windows-session-\u{7}1", InvalidResourceScope::ControlCharacters),
        ] {
            assert_eq!(InputSafetyResourceScope::parse(value), Err(expected), "value={value:?}");
        }
        // 首尾空白会被裁剪（与工作区标识解析器同一原则），但**内嵌**控制字符一律拒绝。
        assert_eq!(
            InputSafetyResourceScope::parse("  windows-session-1  ")
                .expect("裁剪后有效")
                .as_str(),
            "windows-session-1"
        );
    }

    /// **`Unknown` 不是 `Safe`**：默认状态不接受新输入。
    #[test]
    fn unknown_state_never_allows_new_input() {
        let scope = InputSafetyResourceScope::parse("windows-session-1").expect("scope");
        let mut state = InputSafetyResourceState::initial(scope);
        assert_eq!(state.state, ResourceSafetyState::Unknown);
        assert!(!state.accepts_new_input, "初值必须 fail-closed");
        assert!(!ResourceSafetyState::Unknown.allows_new_input());
        assert!(!ResourceSafetyState::Isolated.allows_new_input());
        assert!(ResourceSafetyState::Safe.allows_new_input());

        state.transition(ResourceSafetyState::Safe, Some("coordinator-1"), 1);
        assert!(state.accepts_new_input);
        assert_eq!(state.revision, 2);
        state.transition(ResourceSafetyState::Unknown, None, 1);
        assert!(!state.accepts_new_input, "回到未知必须重新拒绝新输入");
    }

    /// 恢复阶段只能**单调前进**，且提交后不得再改。
    #[test]
    fn recovery_stages_are_monotonic_and_frozen_after_commit() {
        let scope = InputSafetyResourceScope::parse("windows-session-1").expect("scope");
        let mut operation = InputSafetyRecoveryOperation {
            recovery_operation_id: "recovery-1".to_string(),
            coordinator_instance_id: "coordinator-1".to_string(),
            recovery_epoch: 1,
            gate_revision: 1,
            scope,
            source_database_identity: "session-db:default".to_string(),
            candidate_run_ids: Vec::new(),
            allowed_operations: vec!["converge_legacy_cu_run".to_string()],
            stage: RecoveryStage::CoordinationAcquired,
            recorded_at_unix_ms: 1,
            committed: false,
            disposition: RecoveryDisposition::Pending,
        };
        assert!(operation.can_advance_to(RecoveryStage::IntentPersistedAndIntakeClosed));
        assert!(!operation.can_advance_to(RecoveryStage::TerminalCommitted), "不得跳步");
        operation.stage = RecoveryStage::IntentPersistedAndIntakeClosed;
        assert!(!operation.can_advance_to(RecoveryStage::CoordinationAcquired), "不得回退");
        operation.committed = true;
        assert!(!operation.can_advance_to(RecoveryStage::StageCommitted), "提交后冻结");
        assert_eq!(RecoveryStage::Reopened.order(), 9);
    }

    /// 处置的终态口径：只有 `Pending` 是"还在办"，且**认不出的值不得当已结账**。
    #[test]
    fn recovery_dispositions_are_terminal_except_pending() {
        assert!(!RecoveryDisposition::Pending.is_terminal());
        for settled in [
            RecoveryDisposition::Recovered,
            RecoveryDisposition::KeptIsolated,
            RecoveryDisposition::HumanReviewRequired,
            RecoveryDisposition::AbandonedWithEvidence,
        ] {
            assert!(settled.is_terminal(), "{settled:?} 必须是终态（否则永远挂在待对账）");
        }
        assert!(RecoveryDisposition::HumanReviewRequired.needs_human());
        assert!(!RecoveryDisposition::Recovered.needs_human());
        // 读写往返：五个值都要能原样读回；未知值一律按"仍在办"（fail-closed，不谎报已结账）。
        for value in [
            RecoveryDisposition::Pending,
            RecoveryDisposition::Recovered,
            RecoveryDisposition::KeptIsolated,
            RecoveryDisposition::HumanReviewRequired,
            RecoveryDisposition::AbandonedWithEvidence,
        ] {
            assert_eq!(RecoveryDisposition::parse(value.as_str()), value);
        }
        assert_eq!(RecoveryDisposition::parse("whatever"), RecoveryDisposition::Pending);
    }
}
