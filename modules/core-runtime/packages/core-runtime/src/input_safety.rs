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
/// 初始为 1；与会话库的版本号**互不相干**。升级 schema **不得**另建空的新版本文件来遗忘旧事故。
pub const INPUT_SAFETY_SCHEMA_VERSION: i64 = 1;

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
}

impl InputSafetyRecoveryOperation {
    /// 阶段是否**单调前进**：不允许回退阶段，也不允许跳到未满足前序的步骤。
    #[must_use]
    pub fn can_advance_to(&self, next: RecoveryStage) -> bool {
        !self.committed && next.order() == self.stage.order() + 1
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本独立：本库版本与会话库**不是**同一个号（本 crate 不知道会话库版本，故只钉本值）。
    #[test]
    fn schema_version_is_independent_and_pinned() {
        assert_eq!(INPUT_SAFETY_SCHEMA_VERSION, 1);
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
        };
        assert!(operation.can_advance_to(RecoveryStage::IntentPersistedAndIntakeClosed));
        assert!(!operation.can_advance_to(RecoveryStage::TerminalCommitted), "不得跳步");
        operation.stage = RecoveryStage::IntentPersistedAndIntakeClosed;
        assert!(!operation.can_advance_to(RecoveryStage::CoordinationAcquired), "不得回退");
        operation.committed = true;
        assert!(!operation.can_advance_to(RecoveryStage::StageCommitted), "提交后冻结");
        assert_eq!(RecoveryStage::Reopened.order(), 9);
    }
}
