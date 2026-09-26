//! 运行、动作与回执的公共契约。
//!
//! 此模块只定义跨入口共享的事实口径，不拥有数据库、调度循环或桌面输入。
//! 旧入口可先通过兼容适配器写入；缺失的历史事实必须保留为未知，不能猜测补全。
//!
//! # RPR-04c（第二轮裁决第 1、8 项）在这一层的口径
//!
//! ## 1. 唯一 `RunIdentity` + 显式事实作用域
//!
//! - `RunIdentityScope::{Turn, StepAction}` 决定哪些维度**必填**、哪些维度**不适用**；
//!   不适用维度必须以缺省表示（`Option::None` → 序列化时省略 / 存储为 NULL），
//!   并且**拒绝** `""`、`"unknown"`、`"n/a"`、`"0"`、复制 `public_turn_id` / `run_id`
//!   这类"凑字段"的哨兵值（`is_placeholder_identity_value`）。
//! - **本应存在却缺失**（例如 action 事实缺 `action_id`）→ `incomplete_identity` 错误，
//!   **不是**"不适用"；两种情况的区分依据是**事实本身**：输入前就被拒绝的动作从来没有
//!   动作维度（明确缺省），已经开始输入的动作本应带上身份（本应存在却缺失）。
//! - 已提供的字段一律照校验（格式、作用域、父子关联），不会因为"不是本 scope 的必填项"
//!   就被忽略；`scope` 不匹配、`scope` 未声明、schema 版本不认识也都是错误。
//! - **scope 由宿主按所写事实的类型决定**，模型与普通客户端不得自选：生产侧请用
//!   `RunScopeContext::turn_fact`（轮次级事实）与 `RunScopeContext::step_action_fact`
//!   （step/action 级事实）这两个构造入口；把动作事实写成轮次事实会被
//!   `identity_dimension_not_applicable` 直接拒绝。
//!
//! ## 2. 持久化 scope 与版本；旧记录按旧版严格规则解释
//!
//! 新记录必须把 `scope` 与 `schema_version`（`RUN_IDENTITY_SCHEMA_VERSION`）随身份落盘；
//! 缺 `scope` 的旧记录用 `validate_legacy_strict()`（十维全必填的**旧严格规则**）解释，
//! **不得默认视为 `Turn`**（`resolved_scope() == EffectiveRunIdentityScope::LegacyUnscoped`）。
//! `validate()` 保留旧严格语义作为旧调用方兼容入口；新事实写入路径必须显式调用
//! `validate_for(scope)`，读回一侧用 `validate_persisted()` 按记录自身的口径解释。
//!
//! ## 3. 终态映射（`HostRunOutcome`）
//!
//! - `Completed → Succeeded`：**只表示该 scope 的运行正常结束**，不表示用户目标已完成
//!   （目标完成由 `GoalVerdict` 承载；`RunTerminalStatus::claims_goal_completion()` 恒为 false）。
//! - `Failed → Failed`：必须携带真实原因（`reason`）与错误（可选 `error`）。
//! - `Interrupted`：**只有**宿主给出"确认的取消事实"（`confirmed_cancel: Some(origin)`）才 → `Cancelled`，
//!   且取消来源必须写明（`CancelOrigin`，**不默认"用户取消"**）；取消原因不明 / 异常中断 → `Interrupted`
//!   （本枚举值是本轮授权新增的）。
//! - 非终态（`Running` / `CancelRequested`）→ `terminal_status()` 返回 `None`，**不写终态事实**。
//! - 新增终态要同步的四处：**序列化**（自定义 `Deserialize`：不认识的变体报错，
//!   **不得**当成功）、**状态机**（`accepts_late_facts` / `is_scope_success`）、
//!   **投影**（事实存储的 `scope_succeeded`：unknown 不是成功）、**消费者**（只认 `Succeeded`
//!   为成功，`Interrupted` / `Cancelled` / `TimedOut` / `Blocked` / `Failed` 都不是）。
//!
//! ## 4. 凭据域不得混用
//!
//! 输入所有权 epoch（`InputOwnerEpoch`）、会话 writer epoch（`SessionWriterEpoch`）、
//! 运行 claim token（`RunClaimToken`）是**三个不同的类型、三个不同的域**，彼此没有 `From`
//! 转换，不得互相代用。`claim_token` **不进入公开身份对象**（`RunIdentity` 里没有它），
//! 它是运行 claim 的校验凭据，不是身份维度。

use serde::{de, Deserialize, Deserializer, Serialize};

/// `RunIdentity` 的身份 schema 版本。
///
/// 1 = 十维全必填、无 scope（RPR-04c 之前）；2 = 显式 scope + 维度可分缺省（本轮）。
pub const RUN_IDENTITY_SCHEMA_VERSION: u32 = 2;

/// 身份维度（十维）。
///
/// 用它表达"本 scope 必填 / 不适用 / 可选"的表格，避免把规则散在 `if` 里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityDimension {
    WorkspaceId,
    RoomId,
    SessionId,
    PublicTurnId,
    RunId,
    StepId,
    RequestAttemptId,
    ToolCallId,
    ActionId,
    OwnerEpoch,
}

impl IdentityDimension {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceId => "workspace_id",
            Self::RoomId => "room_id",
            Self::SessionId => "session_id",
            Self::PublicTurnId => "public_turn_id",
            Self::RunId => "run_id",
            Self::StepId => "step_id",
            Self::RequestAttemptId => "request_attempt_id",
            Self::ToolCallId => "tool_call_id",
            Self::ActionId => "action_id",
            Self::OwnerEpoch => "owner_epoch",
        }
    }

    /// 是否属于"动作/步骤"维度（turn 级事实里它们不适用）。
    #[must_use]
    pub const fn is_action_dimension(self) -> bool {
        matches!(
            self,
            Self::StepId | Self::RequestAttemptId | Self::ToolCallId | Self::ActionId
        )
    }
}

impl std::fmt::Display for IdentityDimension {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 明显的"凑字段"占位值。
///
/// 这些值出现在**任何**身份维度上都说明该维度不是真实事实——要么是被硬填的，
/// 要么是把别的维度的值抄过来的（`0` 也在此列：宿主侧的输入所有权 epoch 分配器
/// 从 1 开始，`0` 只表示"没有所有权"，必须缺省而不是写 0）。
#[must_use]
pub fn is_placeholder_identity_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "unknown" | "n/a" | "na" | "none" | "null" | "nil" | "undefined" | "-" | "0"
    )
}

/// 事实的作用域：**由宿主按所写事实的类型决定**，不由模型或普通客户端自选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunIdentityScope {
    /// 轮次/运行级事实：运行登记、控制终态、恢复登记。
    /// 该事实不指向具体 step/action，因此**动作维度一律不适用**（必须缺省）。
    Turn,
    /// step/action 级事实：动作回执、动作证据。
    /// 动作维度在**本 scope 必填**（输入前就被拒绝的动作走明确的缺省口径，见 `admit_action_fact`）。
    StepAction,
}

impl RunIdentityScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::StepAction => "step_action",
        }
    }

    /// 声明该 scope 的身份所应携带的 schema 版本。
    #[must_use]
    pub const fn schema_version(self) -> u32 {
        RUN_IDENTITY_SCHEMA_VERSION
    }

    /// 本 scope 是否携带动作维度。
    #[must_use]
    pub const fn carries_action_dimensions(self) -> bool {
        matches!(self, Self::StepAction)
    }

    /// 本 scope 的必填维度表（裁决第 9 条的十字段映射规则）。
    #[must_use]
    pub const fn required_dimensions(self) -> &'static [IdentityDimension] {
        match self {
            Self::Turn => &[
                IdentityDimension::WorkspaceId,
                IdentityDimension::RoomId,
                IdentityDimension::SessionId,
                IdentityDimension::PublicTurnId,
                IdentityDimension::RunId,
            ],
            Self::StepAction => &[
                IdentityDimension::WorkspaceId,
                IdentityDimension::RoomId,
                IdentityDimension::SessionId,
                IdentityDimension::PublicTurnId,
                IdentityDimension::RunId,
                IdentityDimension::StepId,
                IdentityDimension::RequestAttemptId,
                IdentityDimension::ToolCallId,
                IdentityDimension::ActionId,
            ],
        }
    }

    /// 本 scope 下**不适用**的维度：必须缺省，提供即拒绝。
    ///
    /// `Turn` scope 里 `request_attempt_id` **不在**此列：它在轮次级事实里默认缺省，
    /// 但 attempt 登记事实的登记键正是它（缺了该事实无法定位），因此它是"可选维度"
    /// 而不是"不适用维度"。相反，`step_id` / `tool_call_id` / `action_id` 指向具体动作，
    /// 轮次级事实带上它们就说明这条事实其实是动作事实（降 scope 的信号）→ 提供即拒绝。
    #[must_use]
    pub const fn not_applicable_dimensions(self) -> &'static [IdentityDimension] {
        match self {
            Self::Turn => &[
                IdentityDimension::StepId,
                IdentityDimension::ToolCallId,
                IdentityDimension::ActionId,
            ],
            Self::StepAction => &[],
        }
    }

    /// 本 scope 下**可选**的维度：给了就要有效，没给就是"不适用/未知"。
    ///
    /// `owner_epoch` 的"必填"条件是**动作是否声明了执行资格**（见 `admit_action_fact`），
    /// 单看 scope 说不出来：输入前就被拒绝的动作本来就没有所有权。
    #[must_use]
    pub const fn optional_dimensions(self) -> &'static [IdentityDimension] {
        match self {
            Self::Turn => &[
                IdentityDimension::RequestAttemptId,
                IdentityDimension::OwnerEpoch,
            ],
            Self::StepAction => &[IdentityDimension::OwnerEpoch],
        }
    }
}

/// 一条记录**实际**采用的口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveRunIdentityScope {
    /// 记录显式声明了 scope（新记录）。
    Declared(RunIdentityScope),
    /// 记录没有 scope（RPR-04c 之前的旧记录）：按**旧版严格规则**解释，
    /// **不得**默认视为 `Turn`。
    LegacyUnscoped,
}

// ---------------------------------------------------------------------------
// 凭据域：三个域三个类型，互不代用
// ---------------------------------------------------------------------------

/// 输入**执行所有权** epoch（谁在当前持有桌面输入的独占权）。
///
/// 它与会话 writer epoch（谁在写会话）、运行 claim token（谁持有这次运行的 claim）
/// 是**不同的域**：本类型没有从另外两者来的转换，混用必须写出一次显式的
/// `as_str()` + `parse()`，在代码里一眼可见。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InputOwnerEpoch(String);

impl InputOwnerEpoch {
    /// 解析一个 epoch 值：空串、占位值与 `0`（宿主分配器从 1 开始）一律拒绝——
    /// "没有所有权"必须用缺省（`None`）表示，不能用 0 冒充一个真的 epoch。
    pub fn parse(value: impl Into<String>) -> Result<Self, RunContractError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(RunContractError::invalid_identity("owner_epoch"));
        }
        if is_placeholder_identity_value(&value) {
            return Err(RunContractError::placeholder_identity_value(
                "owner_epoch",
            ));
        }
        Ok(Self(value))
    }

    /// 由宿主 epoch 分配器的计数构造。
    ///
    /// 分配器口径（`windows-process-guard`）：`fetch_add(1) + 1`，因此**从 1 开始**；
    /// 传 0 表示调用方其实没有所有权，直接拒绝。
    pub fn from_host_counter(counter: u64) -> Result<Self, RunContractError> {
        if counter == 0 {
            return Err(RunContractError::placeholder_identity_value(
                "owner_epoch",
            ));
        }
        Self::parse(counter.to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InputOwnerEpoch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// 会话 writer epoch（谁在写这条会话）。与输入所有权 epoch **不是一回事**。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionWriterEpoch(String);

impl SessionWriterEpoch {
    pub fn parse(value: impl Into<String>) -> Result<Self, RunContractError> {
        let value = value.into();
        if value.trim().is_empty() || is_placeholder_identity_value(&value) {
            return Err(RunContractError::invalid_identity("session_writer_epoch"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 运行 claim 的校验**凭据**（token）。
///
/// 它的用途只有一个：校验"这次运行是不是同一个 claim 在推"。它**不进入公开身份对象**
/// （`RunIdentity` 没有这个字段），也不取代任何 epoch。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunClaimToken(String);

impl RunClaimToken {
    pub fn parse(value: impl Into<String>) -> Result<Self, RunContractError> {
        let value = value.into();
        if value.trim().is_empty() || is_placeholder_identity_value(&value) {
            return Err(RunContractError::invalid_identity("claim_token"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// 身份
// ---------------------------------------------------------------------------

/// 父运行关联的语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunParentRelation {
    /// 本运行由父**轮次运行**驱动（例如 Web turn run 驱动一个 CU run）。
    DrivenByTurnRun,
    /// 本运行是父运行下的 step 级子运行。
    StepOfRun,
}

/// 显式父运行关联。
///
/// Web run 与 CU run **若是不同实体，不得强行同名合并**成一个 `run_id`：
/// 两者各保有自己的 `run_id`，用本类型显式关联（`parent_run_id` 必须与
/// 本条身份的 `run_id` 不同——相同就是"合并"，直接拒绝）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParentRunLink {
    pub parent_run_id: String,
    pub relation: RunParentRelation,
}

impl ParentRunLink {
    #[must_use]
    pub fn new(parent_run_id: impl Into<String>, relation: RunParentRelation) -> Self {
        Self {
            parent_run_id: parent_run_id.into(),
            relation,
        }
    }

    fn validate(&self, own_run_id: &str) -> Result<(), RunContractError> {
        if self.parent_run_id.trim().is_empty() {
            return Err(RunContractError::invalid_identity("parent_run_id"));
        }
        if is_placeholder_identity_value(&self.parent_run_id) {
            return Err(RunContractError::placeholder_identity_value(
                "parent_run_id",
            ));
        }
        if self.parent_run_id == own_run_id {
            return Err(RunContractError::invalid_identity(
                "parent_run_id 不得等于自身的 run_id（不同运行实体不得同名合并）",
            ));
        }
        Ok(())
    }
}

/// 一次受控运行中不可混用的身份集合。
///
/// 十个维度的完整映射规则见 `RunIdentityScope::required_dimensions` /
/// `not_applicable_dimensions` / `optional_dimensions`。
/// `scope` 与 `schema_version`（`RUN_IDENTITY_SCHEMA_VERSION`）随身份**持久化**；
/// 旧记录缺它们时按旧版严格规则解释（`validate_legacy_strict`），不视为 `Turn`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunIdentity {
    pub workspace_id: String,
    pub room_id: String,
    pub session_id: String,
    /// 公开轮次 ID。
    ///
    /// 只有**确认**宿主侧的 `ChatTurnGuard.turn_id` 正是这个公开轮次 ID 之后，
    /// 才允许把它直接映射到这里；guard 持有的是内部 turn id，核对不了就不得拿它顶替。
    pub public_turn_id: String,
    pub run_id: String,
    /// 不适用（turn 级事实）时必须缺省；`None` 序列化时省略 / 存储为 NULL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    /// 输入执行所有权 epoch：没有对应执行所有权时必须缺省；
    /// **不得**用会话 writer epoch 或 claim token 顶替。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_epoch: Option<InputOwnerEpoch>,
    /// 显式父运行关联（两个运行实体各自保有自己的 `run_id`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run: Option<ParentRunLink>,
    /// 事实作用域（由宿主按事实类型写入）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<RunIdentityScope>,
    /// 身份 schema 版本（新记录必填）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
}

/// 宿主侧的作用域上下文：当前工作区 / 房间 / 会话 / 公开轮次。
///
/// 两个用途：
///
/// 1. **构造**身份——`turn_fact` / `step_action_fact` 把 scope 与版本一起写好，
///    生产者不需要（也不应该）自己拼 `RunIdentity` 字面量；
/// 2. **核对**身份——`validate_identity` 拒绝"外来房间/工作区"的关联
///    （事实里的容器维度必须与宿主当前上下文一致，否则这条事实挂错了地方）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunScopeContext {
    pub workspace_id: String,
    pub room_id: String,
    pub session_id: String,
    pub public_turn_id: String,
}

impl RunScopeContext {
    #[must_use]
    pub fn new(
        workspace_id: impl Into<String>,
        room_id: impl Into<String>,
        session_id: impl Into<String>,
        public_turn_id: impl Into<String>,
    ) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            room_id: room_id.into(),
            session_id: session_id.into(),
            public_turn_id: public_turn_id.into(),
        }
    }

    /// 轮次级事实（运行登记、控制终态）：动作维度一律缺省。
    #[must_use]
    pub fn turn_fact(&self, run_id: impl Into<String>) -> RunIdentity {
        RunIdentity {
            workspace_id: self.workspace_id.clone(),
            room_id: self.room_id.clone(),
            session_id: self.session_id.clone(),
            public_turn_id: self.public_turn_id.clone(),
            run_id: run_id.into(),
            step_id: None,
            request_attempt_id: None,
            tool_call_id: None,
            action_id: None,
            owner_epoch: None,
            parent_run: None,
            scope: Some(RunIdentityScope::Turn),
            schema_version: Some(RUN_IDENTITY_SCHEMA_VERSION),
        }
    }

    /// step/action 级事实：动作维度必填（缺一个就是"本应存在却缺失"）。
    #[must_use]
    pub fn step_action_fact(
        &self,
        run_id: impl Into<String>,
        step_id: impl Into<String>,
        request_attempt_id: impl Into<String>,
        action_id: impl Into<String>,
    ) -> RunIdentity {
        RunIdentity {
            workspace_id: self.workspace_id.clone(),
            room_id: self.room_id.clone(),
            session_id: self.session_id.clone(),
            public_turn_id: self.public_turn_id.clone(),
            run_id: run_id.into(),
            step_id: Some(step_id.into()),
            request_attempt_id: Some(request_attempt_id.into()),
            tool_call_id: None,
            action_id: Some(action_id.into()),
            owner_epoch: None,
            parent_run: None,
            scope: Some(RunIdentityScope::StepAction),
            schema_version: Some(RUN_IDENTITY_SCHEMA_VERSION),
        }
    }

    /// 核对身份里的容器维度是否就是宿主当前上下文。
    ///
    /// 不一致 = 事实挂到了别的房间/工作区/会话/轮次上 → 拒绝（不是"宽容关联"）。
    pub fn validate_identity(&self, identity: &RunIdentity) -> Result<(), RunContractError> {
        for (field, expected, actual) in [
            (
                "workspace_id",
                &self.workspace_id,
                &identity.workspace_id,
            ),
            ("room_id", &self.room_id, &identity.room_id),
            ("session_id", &self.session_id, &identity.session_id),
            (
                "public_turn_id",
                &self.public_turn_id,
                &identity.public_turn_id,
            ),
        ] {
            if expected != actual {
                return Err(RunContractError::context_mismatch(field, expected, actual));
            }
        }
        Ok(())
    }

    /// 关联是否一致（不返回错误的便捷判断）。
    #[must_use]
    pub fn matches_identity(&self, identity: &RunIdentity) -> bool {
        self.validate_identity(identity).is_ok()
    }
}

impl RunIdentity {
    /// 记录实际采用的口径（缺 `scope` = 旧记录，按旧严格规则解释）。
    #[must_use]
    pub fn resolved_scope(&self) -> EffectiveRunIdentityScope {
        match self.scope {
            Some(scope) => EffectiveRunIdentityScope::Declared(scope),
            None => EffectiveRunIdentityScope::LegacyUnscoped,
        }
    }

    #[must_use]
    pub fn declared_scope(&self) -> Option<RunIdentityScope> {
        self.scope
    }

    #[must_use]
    pub fn step_id(&self) -> Option<&str> {
        self.step_id.as_deref()
    }

    #[must_use]
    pub fn request_attempt_id(&self) -> Option<&str> {
        self.request_attempt_id.as_deref()
    }

    #[must_use]
    pub fn tool_call_id(&self) -> Option<&str> {
        self.tool_call_id.as_deref()
    }

    #[must_use]
    pub fn action_id(&self) -> Option<&str> {
        self.action_id.as_deref()
    }

    #[must_use]
    pub fn owner_epoch(&self) -> Option<&InputOwnerEpoch> {
        self.owner_epoch.as_ref()
    }

    #[must_use]
    pub fn parent_run(&self) -> Option<&ParentRunLink> {
        self.parent_run.as_ref()
    }

    #[must_use]
    pub fn with_request_attempt_id(mut self, request_attempt_id: impl Into<String>) -> Self {
        self.request_attempt_id = Some(request_attempt_id.into());
        self
    }

    #[must_use]
    pub fn with_tool_call_id(mut self, tool_call_id: impl Into<String>) -> Self {
        self.tool_call_id = Some(tool_call_id.into());
        self
    }

    #[must_use]
    pub fn with_owner_epoch(mut self, epoch: InputOwnerEpoch) -> Self {
        self.owner_epoch = Some(epoch);
        self
    }

    #[must_use]
    pub fn with_parent_run(mut self, parent: ParentRunLink) -> Self {
        self.parent_run = Some(parent);
        self
    }

    /// 旧调用方的兼容入口：**保留原来的严格语义**（十维全部必须存在且有效）。
    ///
    /// 新事实写入路径必须调用 `validate_for(scope)`；本函数只作为旧调用方的入口保留，
    /// 不得被当作"放松后的统一校验"。
    pub fn validate(&self) -> Result<(), RunContractError> {
        self.validate_legacy_strict()
    }

    /// 旧版严格规则：十维全必填（缺 scope 的旧记录按此解释）。
    pub fn validate_legacy_strict(&self) -> Result<(), RunContractError> {
        for dimension in [
            IdentityDimension::WorkspaceId,
            IdentityDimension::RoomId,
            IdentityDimension::SessionId,
            IdentityDimension::PublicTurnId,
            IdentityDimension::RunId,
            IdentityDimension::StepId,
            IdentityDimension::RequestAttemptId,
            IdentityDimension::ToolCallId,
            IdentityDimension::ActionId,
            IdentityDimension::OwnerEpoch,
        ] {
            match self.dimension_value(dimension) {
                Some(value) => self.validate_dimension_value(dimension, value)?,
                // 旧记录里"缺失"就是"没有这个值"：不允许当成"不适用"。
                None => return Err(RunContractError::incomplete_identity(dimension)),
            }
        }
        self.validate_parent_run()
    }

    /// 按**声明的作用域**校验（新事实写入路径必须走这里）。
    pub fn validate_for(&self, scope: RunIdentityScope) -> Result<(), RunContractError> {
        self.ensure_declared_scope(scope)?;
        self.validate_dimensions(scope, MissingDimensionPolicy::Reject)
    }

    /// 读回一侧的校验入口：按记录**自身**的口径解释。
    ///
    /// 新记录按它声明的 scope；旧记录（无 scope）按旧版严格规则——**不视为 Turn**。
    pub fn validate_persisted(&self) -> Result<(), RunContractError> {
        match self.resolved_scope() {
            EffectiveRunIdentityScope::Declared(scope) => self.validate_for(scope),
            EffectiveRunIdentityScope::LegacyUnscoped => self.validate_legacy_strict(),
        }
    }

    /// 保守校验：允许**动作维度缺省**（输入前拒绝 / 已发生事实的异常保留路径），
    /// 但基础维度、scope/版本一致性、以及**所有已提供字段**照样严格校验。
    ///
    /// 这条路径永远**不**给出输入资格（见 `admit_action_fact`）。
    pub fn validate_conservative_for(
        &self,
        scope: RunIdentityScope,
    ) -> Result<(), RunContractError> {
        self.ensure_declared_scope(scope)?;
        self.validate_dimensions(scope, MissingDimensionPolicy::TolerateActionDimensions)
    }

    /// 本 scope 下"本应存在却缺失"的维度（空表示不缺失）。
    #[must_use]
    pub fn missing_required_dimensions(&self, scope: RunIdentityScope) -> Vec<IdentityDimension> {
        scope
            .required_dimensions()
            .iter()
            .copied()
            .filter(|dimension| self.dimension_value(*dimension).is_none())
            .collect()
    }

    fn ensure_declared_scope(&self, scope: RunIdentityScope) -> Result<(), RunContractError> {
        match self.scope {
            None => Err(RunContractError::identity_scope_undeclared()),
            Some(declared) if declared != scope => {
                Err(RunContractError::identity_scope_mismatch(scope, declared))
            }
            Some(_) => match self.schema_version {
                None => Err(RunContractError::identity_schema_version_missing()),
                Some(version) if version != RUN_IDENTITY_SCHEMA_VERSION => Err(
                    RunContractError::unsupported_identity_schema_version(version),
                ),
                Some(_) => Ok(()),
            },
        }
    }

    fn validate_dimensions(
        &self,
        scope: RunIdentityScope,
        policy: MissingDimensionPolicy,
    ) -> Result<(), RunContractError> {
        // 不适用维度必须缺省：提供了就说明事实被写错了 scope
        // （"把动作事实降级成 turn 事实"最直接的可见信号）。
        for dimension in scope.not_applicable_dimensions() {
            if let Some(value) = self.dimension_value(*dimension) {
                return Err(RunContractError::dimension_not_applicable(
                    *dimension, scope, value,
                ));
            }
        }
        for dimension in scope.required_dimensions() {
            if let Some(value) = self.dimension_value(*dimension) {
                self.validate_dimension_value(*dimension, value)?;
                continue;
            }
            if policy == MissingDimensionPolicy::TolerateActionDimensions
                && dimension.is_action_dimension()
            {
                continue;
            }
            return Err(RunContractError::incomplete_identity(*dimension));
        }
        // 可选维度：已提供就必须有效（"不是必填项"不等于"可以忽略其中的错误值"）。
        for dimension in scope.optional_dimensions() {
            if let Some(value) = self.dimension_value(*dimension) {
                self.validate_dimension_value(*dimension, value)?;
            }
        }
        self.validate_parent_run()
    }

    fn validate_parent_run(&self) -> Result<(), RunContractError> {
        if let Some(parent) = &self.parent_run {
            parent.validate(&self.run_id)?;
        }
        Ok(())
    }

    fn dimension_value(&self, dimension: IdentityDimension) -> Option<&str> {
        match dimension {
            IdentityDimension::WorkspaceId => Some(self.workspace_id.as_str()),
            IdentityDimension::RoomId => Some(self.room_id.as_str()),
            IdentityDimension::SessionId => Some(self.session_id.as_str()),
            IdentityDimension::PublicTurnId => Some(self.public_turn_id.as_str()),
            IdentityDimension::RunId => Some(self.run_id.as_str()),
            IdentityDimension::StepId => self.step_id.as_deref(),
            IdentityDimension::RequestAttemptId => self.request_attempt_id.as_deref(),
            IdentityDimension::ToolCallId => self.tool_call_id.as_deref(),
            IdentityDimension::ActionId => self.action_id.as_deref(),
            IdentityDimension::OwnerEpoch => self.owner_epoch.as_ref().map(InputOwnerEpoch::as_str),
        }
    }

    fn validate_dimension_value(
        &self,
        dimension: IdentityDimension,
        value: &str,
    ) -> Result<(), RunContractError> {
        if value.trim().is_empty() {
            // 与旧语义一致：空串仍然是 `invalid_identity`。
            return Err(RunContractError::invalid_identity(dimension.as_str()));
        }
        if is_placeholder_identity_value(value) {
            return Err(RunContractError::placeholder_identity_value(
                dimension.as_str(),
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(RunContractError::invalid_identity(format!(
                "{}（含控制字符，不是有效的身份值）",
                dimension.as_str()
            )));
        }
        // 把容器维度（公开轮次 / run）抄进动作维度或 epoch 也是凑字段。
        if (dimension.is_action_dimension() || dimension == IdentityDimension::OwnerEpoch)
            && (value == self.public_turn_id || value == self.run_id)
        {
            return Err(RunContractError::placeholder_identity_value(
                dimension.as_str(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingDimensionPolicy {
    /// 缺失即"本应存在却缺失"。
    Reject,
    /// 允许动作维度缺省（输入前拒绝 / 已发生事实的异常保留路径）。
    TolerateActionDimensions,
}

// ---------------------------------------------------------------------------
// 动作事实的身份准入（裁决第 11 条）
// ---------------------------------------------------------------------------

/// 身份异常：**事实必须保留**，但必须带着这条异常一起保留，并且停止后续输入。
///
/// 它是可以持久化的事实（落在动作事实记录里），不是日志字符串：
/// 消费方据此判断"这个 run 还能不能再发输入"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityAnomaly {
    /// 契约给出的**身份不完整错误**（原始对象，不改写成一句人话）。
    pub error: RunContractError,
    /// 本应存在却缺失的维度。
    pub missing_dimensions: Vec<String>,
    pub run_id: String,
    /// 相关动作（身份里没有 `action_id` 时为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    /// 回执是否表明输入已经开始（未知一律按已开始处理）。
    pub input_may_have_started: bool,
    /// **恒为 true**：这类记录一律不得继续输入。
    pub must_stop_input: bool,
}

impl IdentityAnomaly {
    #[must_use]
    pub fn code(&self) -> &str {
        &self.error.code
    }

    /// 异常自身是否自洽。事实存储在校验记录时会调用它，
    /// 因此"把 `must_stop_input` 改成 false 想混过去"会在读回时被拒。
    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.error.code.trim().is_empty() {
            return Err(RunContractError::invalid_identity("identity_anomaly.code"));
        }
        if self.missing_dimensions.is_empty() {
            return Err(RunContractError::incomplete_identity(
                "identity_anomaly.missing_dimensions（异常必须写明缺失的维度）",
            ));
        }
        if self.run_id.trim().is_empty() {
            return Err(RunContractError::invalid_identity("identity_anomaly.run_id"));
        }
        if !self.must_stop_input {
            return Err(RunContractError::contradiction(
                "身份异常必须停止后续输入（must_stop_input 不得为 false）",
            ));
        }
        Ok(())
    }
}

/// 动作事实的身份准入判决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionIdentityAdmission {
    /// 身份完整，按 step/action scope 通过。
    ///
    /// `gains_input_qualification` 为 true 的前提是：本回执**声明了执行资格**
    /// （已经开始/可能已经开始输入）**且**绑定了真实的输入所有权 epoch。
    Admitted { gains_input_qualification: bool },
    /// 输入前就被拒绝：动作维度本来就不存在（**明确缺省**，不是"本应存在却缺失"）。
    /// 允许记录该拒绝事实，但**不获得任何输入资格**。
    AdmittedPreInputRejection {
        absent_dimensions: Vec<IdentityDimension>,
    },
    /// 输入已经开始却缺身份：原回执与异常证据必须保留，且**必须停止后续输入**。
    PreservedWithAnomaly { anomaly: IdentityAnomaly },
}

impl ActionIdentityAdmission {
    /// 本次事实是否给出输入执行资格。**只有**完整身份 + 真实 epoch 才为 true。
    #[must_use]
    pub const fn gains_input_qualification(&self) -> bool {
        matches!(
            self,
            Self::Admitted {
                gains_input_qualification: true
            }
        )
    }

    /// 是否必须停止后续输入（异常保留路径恒为 true）。
    #[must_use]
    pub const fn must_stop_input(&self) -> bool {
        matches!(self, Self::PreservedWithAnomaly { .. })
    }

    #[must_use]
    pub fn anomaly(&self) -> Option<&IdentityAnomaly> {
        match self {
            Self::PreservedWithAnomaly { anomaly } => Some(anomaly),
            _ => None,
        }
    }

    #[must_use]
    pub fn absent_dimensions(&self) -> &[IdentityDimension] {
        match self {
            Self::AdmittedPreInputRejection { absent_dimensions } => absent_dimensions,
            _ => &[],
        }
    }
}

/// 动作事实的身份准入。
///
/// 规则（裁决第 4、5、6、11 条）：
///
/// 1. 回执自身必须自洽，且身份若声明了 `action_id` 必须与回执一致；
/// 2. 身份完整 → `Admitted`（声明执行资格的动作还必须绑定真实 `owner_epoch`，
///    否则按"本应存在却缺失"走异常保留）；
/// 3. 身份缺口是**动作维度 / epoch 的缺失**（`incomplete_identity`）时：
///    - 输入已经开始 → `PreservedWithAnomaly`：事实必须保留，异常必须一起落盘，
///      必须停止后续输入；
///    - 输入前拒绝 → `AdmittedPreInputRejection`：允许记录该拒绝事实
///      （明确缺省，不是缺失），但不获得输入资格，且已提供字段仍照校验；
/// 4. 其余身份问题一律**硬拒绝**，因为它们是**写入方**的声明错误而不是"缺少身份"：
///    `scope` 未声明 / 与写入路径不符、schema 版本不认识、不适用维度被提供了值、
///    占位值，以及**容器维度**（工作区 / 房间 / 会话 / 公开轮次 / run）缺失。
///    容器维度缺了，这条事实就没有可挂的地方——补一个凭空的值正是裁决禁止的"凑字段"。
///    此时写入方应当把身份写对再写一次：原始回执仍在写入方手里，
///    本函数拒绝的是一条**贴错身份**的记录，不是否认输入发生过。
pub fn admit_action_fact(
    identity: &RunIdentity,
    receipt: &ActionReceipt,
) -> Result<ActionIdentityAdmission, RunContractError> {
    receipt.validate()?;
    if let (Some(declared), actual) = (identity.action_id(), receipt.action_id.as_str()) {
        if declared != actual {
            return Err(RunContractError::identity_conflict(
                "action_id",
                format!("身份里的 action_id（{declared}）与回执的 action_id（{actual}）不一致"),
            ));
        }
    }

    let input_may_have_started = receipt.may_have_started_input();
    match identity.validate_for(RunIdentityScope::StepAction) {
        Ok(()) => {
            if input_may_have_started && identity.owner_epoch.is_none() {
                // 已声明执行资格却没有输入所有权 epoch：本应存在却缺失。
                return Ok(ActionIdentityAdmission::PreservedWithAnomaly {
                    anomaly: identity_anomaly(
                        RunContractError::incomplete_identity(
                            "owner_epoch（已声明执行资格的动作必须绑定真实 epoch）",
                        ),
                        vec!["owner_epoch".to_string()],
                        identity,
                        input_may_have_started,
                    ),
                });
            }
            Ok(ActionIdentityAdmission::Admitted {
                gains_input_qualification: input_may_have_started,
            })
        }
        Err(error) => {
            let missing = identity.missing_required_dimensions(RunIdentityScope::StepAction);
            if !identity_gap_is_preservable(&error, &missing) {
                return Err(error);
            }
            if input_may_have_started {
                Ok(ActionIdentityAdmission::PreservedWithAnomaly {
                    anomaly: identity_anomaly(
                        error,
                        missing
                            .iter()
                            .map(|dimension| dimension.as_str().to_string())
                            .collect(),
                        identity,
                        true,
                    ),
                })
            } else {
                // 输入前拒绝：缺动作维度可以，但已提供字段必须没有错值。
                identity.validate_conservative_for(RunIdentityScope::StepAction)?;
                Ok(ActionIdentityAdmission::AdmittedPreInputRejection {
                    absent_dimensions: missing,
                })
            }
        }
    }
}

/// 身份缺口是否"可以带着异常保留"。
///
/// 只有**动作维度 / epoch 的缺失**（`incomplete_identity`）才是"已经发生的事实缺了身份"；
/// 容器维度缺失、scope / 版本 / 不适用维度 / 占位值这些是写入方的声明错误，必须硬拒绝。
fn identity_gap_is_preservable(error: &RunContractError, missing: &[IdentityDimension]) -> bool {
    error.code == "incomplete_identity"
        && !missing.is_empty()
        && missing.iter().all(|dimension| {
            dimension.is_action_dimension() || *dimension == IdentityDimension::OwnerEpoch
        })
}

fn identity_anomaly(
    error: RunContractError,
    missing_dimensions: Vec<String>,
    identity: &RunIdentity,
    input_may_have_started: bool,
) -> IdentityAnomaly {
    IdentityAnomaly {
        error,
        missing_dimensions,
        run_id: identity.run_id.clone(),
        action_id: identity.action_id.clone(),
        input_may_have_started,
        must_stop_input: true,
    }
}

/// P-01 动作来源（`ActionSource`）与执行上下文（`ContextKind`）契约。
///
/// 这一节是 **additive 契约**：本轮只定义口径与两级校验入口，没有生产者 / 消费者
/// （接线在后续轮次，见 `crate::fact_store` 模块文档的接线段），因此整节
/// `#![allow(dead_code)]`。**接线落地（`lib.rs` 重导出 + 生产侧调用 + 随动作事实落盘）
/// 之后必须删掉这个 allow。**
#[allow(unused_imports)]
pub use action_origin_contract::*;

/// 动作来源与执行上下文的契约实现（对外名字由上面一行重导出到本模块）。
mod action_origin_contract {
    //! 动作的**来源**（谁产生了执行计划）与动作发生的**上下文类型**。
    #![allow(dead_code)]

    use super::{
        is_placeholder_identity_value, ParentRunLink, RunContractError, RunIdentity,
        RunIdentityScope, RunScopeContext,
    };
    /// 复用 `UsageAttempt{run_id, logical_request_id, attempt_id}` 之前先核对其唯一性
    /// （局部 attempt 编号不能当全局 ID，见 [`PlannedRequestAttempt`]）。
    use crate::usage::UsageAttempt;
    use serde::{Deserialize, Serialize};



    // 概念纠正（裁决第 1、2 条——写成口径，不是备注）：
    //
    // 1. **"原生动作"是执行方式，不是动作来源。** 模型生成点击计划、由 Rust / 原生 helper
    //    执行，来源仍然是 `ModelPlanned`：执行器是不是宿主，不改变"这个动作的执行计划来自
    //    哪一次模型请求"这个事实。因此**不能**因为执行器是宿主就把它标成宿主动作，
    //    更不能借此省略模型请求身份。
    // 2. **`tool_call_id` 存在只证明存在工具调用关系，不能证明动作不是模型规划的。**
    //    工具调用关系与动作来源是两个维度，不能互相证明：来源由 `ActionSource` 声明、
    //    由本节的两级校验核对；工具关系只决定 `tool_call_id` 是否必填（B-7）。

    /// 动作的**来源**：这个动作的**执行计划**是谁产生的。
    ///
    /// 判据是"谁产生了实际执行计划"，**不是**"谁执行了它"：模型规划 + 宿主
    /// （Rust / 原生 helper）执行 ⇒ 仍然是 [`ActionSource::ModelPlanned`]。
    /// 同理，`tool_call_id` 只说明存在工具调用关系，不改变来源。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ActionSource {
        /// 模型产生了**实际执行计划**。
        ///
        /// 包括被宿主做过坐标变换 / 受控展开的计划：变换**保留**原模型请求的因果关联，
        /// 不因为做过一次转换就改成"无模型来源"（裁决第 4 条）。
        ModelPlanned,
        /// 观察 / 校验过程中的**宿主辅助动作**（不是模型计划）。
        HostIncidental,
        /// 释放、停止等**安全收尾**：只有宿主**确定性执行**的安全操作属于这里。
        SafetyCleanup,
        /// 真实用户直接触发的操作。
        UserDirect,
    }

    /// 来源记录的字段（把"必填 / 不适用"写成表，避免规则散在 `if` 里）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ActionOriginField {
        RequestAttemptId,
        ToolCallId,
        ParentStepOperation,
        HostAlgorithmVersion,
        ResourceScope,
        CleanupRelation,
        UserDirectRelation,
    }

    impl ActionOriginField {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::RequestAttemptId => "request_attempt_id",
                Self::ToolCallId => "tool_call_id",
                Self::ParentStepOperation => "parent_step_operation",
                Self::HostAlgorithmVersion => "host_algorithm_version",
                Self::ResourceScope => "resource_scope",
                Self::CleanupRelation => "cleanup",
                Self::UserDirectRelation => "user_direct",
            }
        }
    }

    impl std::fmt::Display for ActionOriginField {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.as_str())
        }
    }

    impl ActionSource {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::ModelPlanned => "model_planned",
                Self::HostIncidental => "host_incidental",
                Self::SafetyCleanup => "safety_cleanup",
                Self::UserDirect => "user_direct",
            }
        }

        /// 该来源是否**必须**给出"产生执行计划的模型请求身份"（`request_attempt_id`）。
        ///
        /// 只有 `ModelPlanned` 是。其余三种来源的 `request_attempt_id` **不适用**：
        /// 提供即拒绝——把那三种来源说成模型规划，正是最典型的错标来源。
        #[must_use]
        pub const fn requires_model_request_identity(self) -> bool {
            matches!(self, Self::ModelPlanned)
        }

        /// 模型规划之后的**恢复动作**的来源判定（裁决第 3 条）：恢复不改变来源。
        ///
        /// - 恢复动作的执行计划来自某次模型请求 ⇒ 仍是 `ModelPlanned`；
        /// - 只有宿主**确定性执行**的释放 / 停止等安全操作 ⇒ `SafetyCleanup`。
        #[must_use]
        pub const fn for_recovery(plan_came_from_a_model_request: bool) -> Self {
            if plan_came_from_a_model_request {
                Self::ModelPlanned
            } else {
                Self::SafetyCleanup
            }
        }

        /// 本来源的**必填**关联。
        ///
        /// `tool_call_id` **不在**表里：它是否必填由**可信上下文**决定（B-7）——
        /// 该动作有真实工具调用关系时必填，没有工具链关系时"不适用"。
        /// 可选性不得由调用方自己挑。
        #[must_use]
        pub const fn required_relations(self) -> &'static [ActionOriginField] {
            match self {
                Self::ModelPlanned => &[ActionOriginField::RequestAttemptId],
                Self::HostIncidental => &[
                    ActionOriginField::ParentStepOperation,
                    ActionOriginField::HostAlgorithmVersion,
                    ActionOriginField::ResourceScope,
                ],
                Self::SafetyCleanup => &[ActionOriginField::CleanupRelation],
                Self::UserDirect => &[
                    ActionOriginField::UserDirectRelation,
                    ActionOriginField::ResourceScope,
                ],
            }
        }

        /// 本来源下**不适用**的关联：必须缺省，提供即拒绝。
        #[must_use]
        pub const fn not_applicable_relations(self) -> &'static [ActionOriginField] {
            match self {
                Self::ModelPlanned => &[],
                Self::HostIncidental | Self::SafetyCleanup | Self::UserDirect => {
                    &[ActionOriginField::RequestAttemptId]
                }
            }
        }
    }

    impl std::fmt::Display for ActionSource {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.as_str())
        }
    }

    /// **执行上下文类型**：这个动作发生在哪种上下文里。
    ///
    /// 它与**事实层级**（[`RunIdentityScope`]：`Turn` / `StepAction`）是**正交**的两个
    /// 概念（裁决第 8 条）：两者独立成立、并存，不互相顶替——层级由**事实本身**决定，
    /// 上下文由**动作发生在哪里**决定。`applicable_hierarchies` 把正交关系写成一行。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ContextKind {
        /// 聊天运行上下文：正常的聊天 CU。
        ///
        /// 要求**真实**的工作区 / 房间 / 会话 / 公开轮次与运行关系（裁决第 9 条）。
        Conversation,
        /// 控制面上下文：人工直接操作、资源恢复等。
        ///
        /// **可以没有聊天 session / turn**，但必须在输入前建立**真实的控制操作**
        /// （run / step / action、资源与授权记录齐备，见 [`ControlPlaneOperations::establish`]）：
        /// 这是实际创建一个控制操作，不是生成几个随机 ID 假装它属于聊天轮次。
        /// 本分支的结构里**没有**会话 / 轮次字段，因此"假装属于聊天轮次"无法表达。
        ControlPlane,
        /// **Goal 阶段上下文**（2026-09-26 补充裁决 §5.3 授权新增）：自主 Goal 的阶段执行。
        ///
        /// 与聊天 CU 的关键差别：**强父关系是「哪一次真实 Goal 阶段运行」，而发起它的聊天轮次
        /// 是可缺省的因果引用**——自主 Goal 根本没有 chat turn，不得因此无法表达，
        /// 也不得为凑字段去制造一个 chat turn 或复制 ID 充数。
        ///
        /// 本分支**不等于** [`Self::ControlPlane`]：Goal 内由模型规划的动作仍必须关联**真实规划
        /// attempt**，不能标成控制面来逃避父关系要求，也不能标成「用户直操」。
        GoalPhase,
    }

    impl ContextKind {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Conversation => "conversation",
                Self::ControlPlane => "control_plane",
                Self::GoalPhase => "goal_phase",
            }
        }

        /// 本上下文是否要求真实的聊天会话与公开轮次。
        #[must_use]
        pub const fn requires_chat_session_and_turn(self) -> bool {
            matches!(self, Self::Conversation)
        }

        /// 上下文类型 × 事实层级：两者正交，因此每一格都合法，且必须**分别**成立。
        #[must_use]
        pub const fn applicable_hierarchies(self) -> &'static [RunIdentityScope] {
            &[RunIdentityScope::Turn, RunIdentityScope::StepAction]
        }
    }

    /// 会话上下文：正常聊天 CU 的真实容器与运行关系。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ConversationActionContext {
        /// 事实层级（`Turn` / `StepAction`）。
        pub hierarchy: RunIdentityScope,
        /// 真实身份：工作区 / 房间 / 会话 / 公开轮次 / run / 父子运行关系。
        pub identity: RunIdentity,
    }

    impl ConversationActionContext {
        /// 按身份**自己声明的** scope 建立上下文（未声明 scope 的旧身份不适用于新事实）。
        pub fn new(identity: RunIdentity) -> Result<Self, RunContractError> {
            let hierarchy = identity
                .declared_scope()
                .ok_or_else(RunContractError::identity_scope_undeclared)?;
            Ok(Self {
                hierarchy,
                identity,
            })
        }
    }

    /// **Goal 阶段上下文**（2026-09-26 补充裁决 §5.2／§5.4）。
    ///
    /// 字段按「强关系必须真实存在、弱关系允许缺省」两分：
    ///
    /// - **强父执行关系**：`goal_id` / `phase_id` / `phase_run_id` 三者必须来自**真实**的 Goal
    ///   阶段运行记录；第三者是"本次阶段尝试"的身份，**已有运行 id 足以区分时不再造冗余 id**。
    /// - **可缺省的因果引用**：`initiating_chat_turn`。自主 Goal 为 `None` 是**合法**的，
    ///   不是缺失；它存在时只是因果记录，**不要求它在放行时仍处于运行中**。
    ///
    /// 刻意**没有**的字段：任何"当前活跃运行"或全局唯一的聊天标识。后者若被占用，多个 Goal
    /// 阶段会为同一个 chat 标识竞争（裁决 §5.4 明令不要复用 `legacy_turn_id` 那类列）。
    /// 也刻意**没有** `step_id` / `action_id`：这两个属于 [`ActionOrigin`] 的事实层级，
    /// 不在这里重复一份（否则同一事实有两个来源，迟早漂移）。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct GoalPhaseActionContext {
        /// 事实层级（`Turn` / `StepAction`），与上下文类型正交。
        pub hierarchy: RunIdentityScope,
        /// 所属 Goal 的真实 id。
        pub goal_id: String,
        /// 所属阶段的真实 id。
        pub phase_id: String,
        /// **本次阶段尝试**的真实运行 id（阶段重试会产生新的运行尝试，见 [`Self::is_same_attempt_as`]）。
        pub phase_run_id: String,
        /// 工作区归属（非可选：Goal 一定在某工作区里执行）。
        pub workspace_id: String,
        /// 真实存在的房间 / 会话关联；自主 Goal 可以为 `None`（**不造占位值**）。
        pub room_id: Option<String>,
        pub session_id: Option<String>,
        /// 发起它的聊天轮次（**可缺省的因果引用**）：自主 Goal 为 `None`。
        pub initiating_chat_turn: Option<String>,
        /// 本次 CU 运行的 run id。
        pub run_id: String,
    }

    impl GoalPhaseActionContext {
        /// 结构校验：必需字段必须真实；可选字段**允许缺省**，但一旦存在就必须真实。
        ///
        /// 这一级只看自身，**不足**以受理事实——父运行是否真的存在由宿主权威核对
        /// （见 [`ActionOrigin::validate_against`]）。因此这里**不**做"看起来像 Goal 就行"的放松。
        pub fn validate_structure(&self) -> Result<(), RunContractError> {
            for (field, value) in [
                ("goal_phase.goal_id", &self.goal_id),
                ("goal_phase.phase_id", &self.phase_id),
                ("goal_phase.phase_run_id", &self.phase_run_id),
                ("goal_phase.workspace_id", &self.workspace_id),
                ("goal_phase.run_id", &self.run_id),
            ] {
                validate_origin_reference(field, value)?;
            }
            // 可选字段：缺省合法；存在则必须真实（不得拿占位值顶替"没有"）。
            for (field, value) in [
                ("goal_phase.room_id", &self.room_id),
                ("goal_phase.session_id", &self.session_id),
                ("goal_phase.initiating_chat_turn", &self.initiating_chat_turn),
            ] {
                if let Some(value) = value {
                    validate_origin_reference(field, value)?;
                }
            }
            Ok(())
        }

        /// 两个上下文是否描述**同一次阶段尝试**。
        ///
        /// 阶段重试会产生新的 `phase_run_id` ⇒ 返回 `false`；调用方据此**不得**让新尝试继承
        /// 旧尝试的输入许可 / 审批 / 动作（裁决 §5.2）。
        #[must_use]
        pub fn is_same_attempt_as(&self, other: &Self) -> bool {
            self.goal_id == other.goal_id
                && self.phase_id == other.phase_id
                && self.phase_run_id == other.phase_run_id
        }
    }

    /// 操作者来源：谁**真的**发起了这次控制面操作。
    ///    /// 只有宿主可观察的真实操作者；**模型不在这个表里**（模型最多提出计划，
    /// 不能声明"用户点击了"）。表外的值（例如 `"model"`）在反序列化时报错，
    /// 不会落到某个"未知操作者"兜底。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum OperatorOrigin {
        /// 宿主自己的用户界面。
        HostUserInterface,
        /// 操作系统层面的输入事件（用户直接在桌面 / 窗口上操作）。
        OperatingSystemInput,
        /// 语音指令。
        VoiceCommand,
        /// 外部控制接口（运维脚本 / 远程控制面）：调用方身份必须由宿主核验。
        ExternalControlApi,
    }

    impl OperatorOrigin {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::HostUserInterface => "host_user_interface",
                Self::OperatingSystemInput => "operating_system_input",
                Self::VoiceCommand => "voice_command",
                Self::ExternalControlApi => "external_control_api",
            }
        }
    }

    /// 资源 scope 的种类（动作落在哪一类资源上）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ResourceScopeKind {
        DesktopSession,
        BrowserSession,
        Workspace,
        Room,
        FilePath,
        Process,
    }

    impl ResourceScopeKind {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::DesktopSession => "desktop_session",
                Self::BrowserSession => "browser_session",
                Self::Workspace => "workspace",
                Self::Room => "room",
                Self::FilePath => "file_path",
                Self::Process => "process",
            }
        }
    }

    /// 资源 scope：这条动作动的是哪个**具体**资源。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ResourceScope {
        pub kind: ResourceScopeKind,
        /// 资源的真实定位符（不是"随便一个字符串"）。
        pub reference: String,
    }

    impl ResourceScope {
        #[must_use]
        pub fn new(kind: ResourceScopeKind, reference: impl Into<String>) -> Self {
            Self {
                kind,
                reference: reference.into(),
            }
        }

        fn validate(&self) -> Result<(), RunContractError> {
            validate_origin_reference("resource_scope.reference", &self.reference)
        }
    }

    /// 宿主辅助动作的父步骤 / 操作记录（观察、校验过程中的宿主算法操作）。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct HostOperationRecord {
        pub operation_id: String,
        pub run_id: String,
        pub step_id: String,
        /// 宿主算法版本：**核对**用途，不是展示标签。
        pub algorithm_version: String,
        pub kind: HostOperationKind,
    }

    /// 宿主辅助操作的种类。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum HostOperationKind {
        Observation,
        Verification,
        Inspection,
    }

    impl HostOperationKind {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Observation => "observation",
                Self::Verification => "verification",
                Self::Inspection => "inspection",
            }
        }
    }

    /// 清理来源的原始事实：incident / 原 action / 恢复资格。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct CleanupIncidentRecord {
        pub incident_id: String,
        pub run_id: String,
        /// 原始动作（清理要收尾的那一次动作）。
        pub original_action_id: String,
        /// 原动作的工具调用 id（有工具链归属时才有）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub original_tool_call_id: Option<String>,
        /// 恢复资格：没有资格就不允许作为清理执行的来源。
        pub recovery_eligible: bool,
    }

    /// 清理关系（`SafetyCleanup` 必填）：这次清理针对哪一个 incident / 原 action。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct CleanupRelation {
        pub incident_id: String,
        pub original_action_id: String,
        /// 恢复资格（与可信上下文里的 incident 记录**核对**后才能成立）。
        pub recovery_eligible: bool,
    }

    /// 用户直接操作关系（`UserDirect` 必填）：哪个真实控制操作、谁、凭什么权限。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct UserDirectRelation {
        pub control_operation_id: String,
        pub operator: OperatorOrigin,
        pub permission_decision_id: String,
    }

    /// 宿主对模型计划的变换记录（坐标变换 / 受控展开）。
    ///
    /// 有它**不改变**来源：计划仍属 `ModelPlanned`，只是被宿主按算法版本变换过。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct HostTransformRecord {
        pub algorithm_version: String,
        /// 变换前的模型计划动作。
        pub source_action_id: String,
        /// 是否**保留**原模型请求因果关联（必须为 true：宿主变换不得切断模型来源）。
        pub preserves_model_request: bool,
    }

    /// 因果引用的种类：指向真实存在的父对象（不是把内容抄一遍）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum CausalReferenceKind {
        RequestAttempt,
        ToolCall,
        Action,
        Incident,
        ControlOperation,
        Observation,
        Artifact,
    }

    impl CausalReferenceKind {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::RequestAttempt => "request_attempt",
                Self::ToolCall => "tool_call",
                Self::Action => "action",
                Self::Incident => "incident",
                Self::ControlOperation => "control_operation",
                Self::Observation => "observation",
                Self::Artifact => "artifact",
            }
        }
    }

    /// 因果引用的角色。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum CausalRole {
        /// 产生执行计划的请求：**主要来源**，只能由 `request_attempt_id` 承载。
        PlanProducer,
        /// 视觉转述请求（附加因果）。
        VisualDescription,
        /// 最终验收请求（附加因果）。
        FinalVerification,
        /// 原模型请求（例如清理动作引用它要收尾的那次规划）：**不得**当主要来源。
        OriginalModelRequest,
        /// 原工具调用（附加因果）。
        OriginalToolCall,
        /// 来自**模型输入**的断言：只是资料，**永远**不构成来源。
        ModelAssertion,
    }

    impl CausalRole {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::PlanProducer => "plan_producer",
                Self::VisualDescription => "visual_description",
                Self::FinalVerification => "final_verification",
                Self::OriginalModelRequest => "original_model_request",
                Self::OriginalToolCall => "original_tool_call",
                Self::ModelAssertion => "model_assertion",
            }
        }

        /// 该角色是否可以作为**主要来源**（只有 `PlanProducer`，且必须走 `request_attempt_id`）。
        #[must_use]
        pub const fn may_be_primary_source(self) -> bool {
            matches!(self, Self::PlanProducer)
        }
    }

    /// 引用是谁给的。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum CausalClaimant {
        /// 宿主侧的真实记录。
        Host,
        /// 模型输入里的断言（不可信资料）。
        Model,
    }

    impl CausalClaimant {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Host => "host",
                Self::Model => "model",
            }
        }

        /// 该引用是否属于**可信证据**（模型断言不是）。
        #[must_use]
        pub const fn is_trusted_evidence(self) -> bool {
            matches!(self, Self::Host)
        }
    }

    /// 一条**附加**因果引用：补全"这个动作还和哪些真实对象有关"。
    ///
    /// 它只做补充：主要来源是 `request_attempt_id`；视觉转述 / 最终验收 / 原模型请求
    /// 这类关系放这里，**不得**顶替主要来源，也不得随手把最近一次请求填进来。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct CausalReference {
        pub role: CausalRole,
        pub claimant: CausalClaimant,
        pub kind: CausalReferenceKind,
        /// 真实对象的定位符。
        pub reference: String,
    }

    impl CausalReference {
        #[must_use]
        pub fn host(role: CausalRole, kind: CausalReferenceKind, reference: impl Into<String>) -> Self {
            Self {
                role,
                claimant: CausalClaimant::Host,
                kind,
                reference: reference.into(),
            }
        }

        /// 模型输入里的断言（记录它的存在，但**不构成**来源）。
        #[must_use]
        pub fn model_assertion(kind: CausalReferenceKind, reference: impl Into<String>) -> Self {
            Self {
                role: CausalRole::ModelAssertion,
                claimant: CausalClaimant::Model,
                kind,
                reference: reference.into(),
            }
        }

        fn validate(&self) -> Result<(), RunContractError> {
            validate_origin_reference("causal_reference.reference", &self.reference)?;
            if self.role.may_be_primary_source() {
                return Err(RunContractError::action_origin_conflict(
                    "additional_causal_refs",
                    "产生执行计划的请求只能作**主要来源**（request_attempt_id），不得放进附加因果引用",
                ));
            }
            if !self.claimant.is_trusted_evidence() && self.role != CausalRole::ModelAssertion {
                return Err(RunContractError::action_origin_conflict(
                    "causal_reference.claimant",
                    "来自模型输入的引用只能是 model_assertion：模型不是宿主事实的见证者",
                ));
            }
            if self.role == CausalRole::ModelAssertion && self.claimant.is_trusted_evidence() {
                return Err(RunContractError::action_origin_conflict(
                    "causal_reference.role",
                    "model_assertion 只能由模型输入携带：宿主不要伪造「模型这么说」的记录",
                ));
            }
            Ok(())
        }
    }

    /// 控制面上下文：动作发生在控制面，**没有聊天 session / turn**。
    ///
    /// 它不是"聊天轮次的第二种写法"：结构里只有控制操作自己的身份
    /// （控制操作 id、run / step / action、操作者、授权、资源），
    /// 因此"生成几个随机 ID 假装它属于聊天轮次"在这里根本无法表达。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ControlPlaneContext {
        pub workspace_id: String,
        pub room_id: String,
        /// **输入前已建立**的控制操作 id（由 [`ControlPlaneOperations::establish`] 分配）。
        pub control_operation_id: String,
        pub run_id: String,
        pub step_id: String,
        pub action_id: String,
        pub operator: OperatorOrigin,
        pub permission_decision_id: String,
        pub resource_scope: ResourceScope,
    }

    impl ControlPlaneContext {
        /// 从**已登记**的控制操作记录读回上下文（读回一侧；写入一侧走
        /// [`ControlPlaneOperations::establish`]）。
        pub fn from_record(record: &ControlOperationRecord) -> Result<Self, RunContractError> {
            record.validate()?;
            Ok(Self {
                workspace_id: record.workspace_id.clone(),
                room_id: record.room_id.clone(),
                control_operation_id: record.control_operation_id.clone(),
                run_id: record.run_id.clone(),
                step_id: record.step_id.clone(),
                action_id: record.action_id.clone(),
                operator: record.operator,
                permission_decision_id: record.permission_decision_id.clone(),
                resource_scope: record.resource_scope.clone(),
            })
        }

        pub fn validate_structure(&self) -> Result<(), RunContractError> {
            validate_origin_reference("control_plane.workspace_id", &self.workspace_id)?;
            validate_origin_reference("control_plane.room_id", &self.room_id)?;
            validate_origin_reference(
                "control_plane.control_operation_id",
                &self.control_operation_id,
            )?;
            validate_origin_reference("control_plane.run_id", &self.run_id)?;
            validate_origin_reference("control_plane.step_id", &self.step_id)?;
            validate_origin_reference("control_plane.action_id", &self.action_id)?;
            validate_origin_reference(
                "control_plane.permission_decision_id",
                &self.permission_decision_id,
            )?;
            self.resource_scope.validate()
        }
    }

    /// 宿主在**输入前**建立的真实控制操作记录。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ControlOperationRecord {
        pub control_operation_id: String,
        pub workspace_id: String,
        pub room_id: String,
        pub run_id: String,
        pub step_id: String,
        pub action_id: String,
        pub operator: OperatorOrigin,
        pub permission_decision_id: String,
        pub resource_scope: ResourceScope,
        /// 宿主分配顺序（从 1 开始，单调递增）：登记发生在输入之前。
        pub establish_sequence: u64,
    }

    impl ControlOperationRecord {
        fn validate(&self) -> Result<(), RunContractError> {
            validate_origin_reference("control_operation_id", &self.control_operation_id)?;
            validate_origin_reference("control_operation.workspace_id", &self.workspace_id)?;
            validate_origin_reference("control_operation.room_id", &self.room_id)?;
            validate_origin_reference("control_operation.run_id", &self.run_id)?;
            validate_origin_reference("control_operation.step_id", &self.step_id)?;
            validate_origin_reference("control_operation.action_id", &self.action_id)?;
            validate_origin_reference(
                "control_operation.permission_decision_id",
                &self.permission_decision_id,
            )?;
            self.resource_scope.validate()?;
            if self.establish_sequence == 0 {
                return Err(RunContractError::incomplete_action_origin(
                    "控制操作必须带宿主分配的登记顺序（establish_sequence 从 1 开始）：没有登记顺序就说不清「输入前已建立」",
                ));
            }
            Ok(())
        }

        /// 登记顺序（消费方据此判断"先建立控制操作、后执行输入"）。
        #[must_use]
        pub const fn establish_sequence(&self) -> u64 {
            self.establish_sequence
        }

        /// 逐字段核对控制面上下文的声明是否就是这条真实记录（不是"名字对上"）。
        pub fn matches_claim(&self, context: &ControlPlaneContext) -> Result<(), RunContractError> {
            self.validate()?;
            for (field, expected, actual) in [
                ("workspace_id", &self.workspace_id, &context.workspace_id),
                ("room_id", &self.room_id, &context.room_id),
                ("run_id", &self.run_id, &context.run_id),
                ("step_id", &self.step_id, &context.step_id),
                ("action_id", &self.action_id, &context.action_id),
                (
                    "permission_decision_id",
                    &self.permission_decision_id,
                    &context.permission_decision_id,
                ),
            ] {
                if expected != actual {
                    return Err(RunContractError::action_origin_conflict(
                        field,
                        format!(
                            "控制面上下文的 {field}（{actual}）与真实控制操作记录（{expected}）不一致"
                        ),
                    ));
                }
            }
            if self.operator != context.operator {
                return Err(RunContractError::action_origin_conflict(
                    "operator",
                    format!(
                        "控制面上下文声称的操作者（{}）与真实控制操作记录（{}）不一致",
                        context.operator.as_str(),
                        self.operator.as_str()
                    ),
                ));
            }
            if self.resource_scope != context.resource_scope {
                return Err(RunContractError::action_origin_conflict(
                    "resource_scope",
                    "控制面上下文的资源 scope 与真实控制操作记录不一致",
                ));
            }
            Ok(())
        }
    }

    /// 建立控制操作的请求：操作者来源、权限决定、资源与真实 run / step / action 都要写明。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ControlOperationRequest {
        pub workspace_id: String,
        pub room_id: String,
        pub run_id: String,
        pub step_id: String,
        pub action_id: String,
        pub operator: OperatorOrigin,
        pub permission_decision_id: String,
        pub resource_scope: ResourceScope,
    }

    /// 控制面操作的宿主登记表。
    ///
    /// 它回答"这个控制操作是不是真的存在过"：
    ///
    /// 1. id 由**宿主按登记顺序分配**（`control-op:{workspace}:{room}:{sequence}`），
    ///    调用方**不能**自带一个随机 id——所以"生成几个随机 ID 假装属于聊天轮次"
    ///    在这里拿不到上下文；
    /// 2. 登记时必须写明操作者来源、权限决定、资源 scope 与真实 run / step / action，
    ///    缺一个（或带占位值）就拒绝登记，不会"先登记再补"；
    /// 3. 登记发生在**输入之前**：`establish_sequence` 从 1 开始单调递增，
    ///    事后的补登记会拿到更大的序号。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ControlPlaneOperations {
        operations: std::collections::BTreeMap<String, ControlOperationRecord>,
        next_sequence: u64,
    }

    impl Default for ControlPlaneOperations {
        /// `Default` 与 `new()` 必须是同一件事：登记顺序从 1 开始
        /// （`0` 表示"没有登记顺序"，不是有效的控制操作）。
        fn default() -> Self {
            Self::new()
        }
    }

    impl ControlPlaneOperations {
        #[must_use]
        pub fn new() -> Self {
            Self {
                operations: std::collections::BTreeMap::new(),
                next_sequence: 1,
            }
        }

        /// 建立（登记）一个控制操作：id 由宿主分配，返回可直接使用的上下文。
        pub fn establish(
            &mut self,
            request: ControlOperationRequest,
        ) -> Result<ControlPlaneContext, RunContractError> {
            validate_origin_reference("control_operation.workspace_id", &request.workspace_id)?;
            validate_origin_reference("control_operation.room_id", &request.room_id)?;
            validate_origin_reference("control_operation.run_id", &request.run_id)?;
            validate_origin_reference("control_operation.step_id", &request.step_id)?;
            validate_origin_reference("control_operation.action_id", &request.action_id)?;
            validate_origin_reference(
                "control_operation.permission_decision_id",
                &request.permission_decision_id,
            )?;
            request.resource_scope.validate()?;
            let sequence = self.next_sequence;
            let control_operation_id = format!(
                "control-op:{}:{}:{}",
                request.workspace_id, request.room_id, sequence
            );
            let record = ControlOperationRecord {
                control_operation_id: control_operation_id.clone(),
                workspace_id: request.workspace_id,
                room_id: request.room_id,
                run_id: request.run_id,
                step_id: request.step_id,
                action_id: request.action_id,
                operator: request.operator,
                permission_decision_id: request.permission_decision_id,
                resource_scope: request.resource_scope,
                establish_sequence: sequence,
            };
            record.validate()?;
            self.operations.insert(control_operation_id.clone(), record);
            self.next_sequence = sequence.saturating_add(1);
            let established = self
                .operations
                .get(&control_operation_id)
                .expect("刚登记的控制操作必须存在");
            ControlPlaneContext::from_record(established)
        }

        /// 采纳宿主**已有**的控制操作记录（例如宿主 UI 自己的操作表）。
        ///
        /// 允许外部 id，但记录必须带齐授权与资源、且 id 不是占位值：
        /// 采纳不是"免登记"，而是"把已存在的真实记录接进来"。
        pub fn adopt(&mut self, record: ControlOperationRecord) -> Result<(), RunContractError> {
            record.validate()?;
            self.operations
                .insert(record.control_operation_id.clone(), record);
            Ok(())
        }

        #[must_use]
        pub fn operation(&self, control_operation_id: &str) -> Option<&ControlOperationRecord> {
            self.operations.get(control_operation_id)
        }

        #[must_use]
        pub fn len(&self) -> usize {
            self.operations.len()
        }

        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.operations.is_empty()
        }
    }

    /// 动作的**执行上下文**（`Conversation` / `ControlPlane` / `GoalPhase` 三者之一）。
    ///
    /// 反序列化**不做未知值兜底**：`ContextKind` 用 `snake_case` 枚举，表外的值会直接报错，
    /// 不会落成某个"未知上下文"——否则一个未来版本的上下文会在旧读者里被静默当成受信面。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ActionContext {
        Conversation(ConversationActionContext),
        ControlPlane(ControlPlaneContext),
        GoalPhase(GoalPhaseActionContext),
    }

    impl ActionContext {
        pub fn conversation(identity: RunIdentity) -> Result<Self, RunContractError> {
            Ok(Self::Conversation(ConversationActionContext::new(
                identity,
            )?))
        }

        #[must_use]
        pub fn control_plane(context: ControlPlaneContext) -> Self {
            Self::ControlPlane(context)
        }

        #[must_use]
        pub const fn kind(&self) -> ContextKind {
            match self {
                Self::Conversation(_) => ContextKind::Conversation,
                Self::ControlPlane(_) => ContextKind::ControlPlane,
                Self::GoalPhase(_) => ContextKind::GoalPhase,
            }
        }

        #[must_use]
        pub fn workspace_id(&self) -> &str {
            match self {
                Self::Conversation(conversation) => &conversation.identity.workspace_id,
                Self::ControlPlane(context) => &context.workspace_id,
                Self::GoalPhase(context) => &context.workspace_id,
            }
        }

        /// 房间的真实归属。
        ///
        /// 返回 `Option`：**Goal 阶段上下文里房间可以真实地不存在**（自主 Goal 没有聊天房间），
        /// 这时必须是 `None`——返回空串会把"没有"伪装成一个值，调用方也无从分辨。
        #[must_use]
        pub fn room_id(&self) -> Option<&str> {
            match self {
                Self::Conversation(conversation) => Some(&conversation.identity.room_id),
                Self::ControlPlane(context) => Some(&context.room_id),
                Self::GoalPhase(context) => context.room_id.as_deref(),
            }
        }

        /// 会话的真实归属（语义同 [`Self::room_id`]：允许真实地不存在）。
        #[must_use]
        pub fn session_id(&self) -> Option<&str> {
            match self {
                Self::Conversation(conversation) => Some(&conversation.identity.session_id),
                Self::ControlPlane(_) => None,
                Self::GoalPhase(context) => context.session_id.as_deref(),
            }
        }

        #[must_use]
        pub fn run_id(&self) -> &str {
            match self {
                Self::Conversation(conversation) => &conversation.identity.run_id,
                Self::ControlPlane(context) => &context.run_id,
                Self::GoalPhase(context) => &context.run_id,
            }
        }

        /// 本上下文里的步骤（`Turn` 级会话事实没有步骤）。
        #[must_use]
        pub fn step_id(&self) -> Option<&str> {
            match self {
                Self::Conversation(conversation) => conversation.identity.step_id(),
                Self::ControlPlane(context) => Some(context.step_id.as_str()),
                // Goal 阶段上下文不重复承载 step：它属于事实层级（见 `ActionOrigin`）。
                Self::GoalPhase(_) => None,
            }
        }

        /// 控制操作 id（只有控制面上下文有）。
        #[must_use]
        pub fn control_operation_id(&self) -> Option<&str> {
            match self {
                Self::Conversation(_) => None,
                Self::ControlPlane(context) => Some(context.control_operation_id.as_str()),
                // Goal 阶段**不是**控制面：不得借道这条访问器把自己当成控制操作。
                Self::GoalPhase(_) => None,
            }
        }

        /// **强父执行关系**里的"本次阶段尝试"运行 id（只有 Goal 阶段上下文有）。
        ///
        /// 这是"这条 CU 属于哪一次真实 Goal 阶段执行"的锚点；宿主必须按它核对真实父运行。
        #[must_use]
        pub fn parent_goal_phase_run_id(&self) -> Option<&str> {
            match self {
                Self::GoalPhase(context) => Some(context.phase_run_id.as_str()),
                _ => None,
            }
        }
    }

    /// 一条动作事实的**来源记录**。
    ///
    /// 它不是展示标签：结构校验（[`ActionOrigin::validate_structure`]）只看这条记录与
    /// 来源表；可信关联校验（[`ActionOrigin::validate_against`] / [`admit_action_origin`]）
    /// **必须**消费可信上下文，核对真实父对象——不能只信传入的 `source` 字符串。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ActionOrigin {
        /// 本来源记录描述的动作（必须与回执 / 身份的 `action_id` 一致）。
        pub action_id: String,
        pub source: ActionSource,
        pub context: ActionContext,
        /// 产生执行计划的**模型请求**身份：`ModelPlanned` 必填，其余来源不适用。
        ///
        /// 值是 [`PlannedRequestAttempt::stable_key`]（`run#logical#attempt` 复合键），
        /// 不是局部 attempt 编号、provider trace、逻辑请求 ID 或外层工具调用 id。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub request_attempt_id: Option<String>,
        /// 工具链归属：**来自工具链时必填**（是否来自工具链由可信上下文判定）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tool_call_id: Option<String>,
        /// 宿主辅助动作的父步骤 / 操作（`HostIncidental` 必填）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parent_step_operation: Option<String>,
        /// 宿主算法版本（`HostIncidental` 必填）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub host_algorithm_version: Option<String>,
        /// 资源 scope（`HostIncidental` / `UserDirect` 必填）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub resource_scope: Option<ResourceScope>,
        /// 清理关系（`SafetyCleanup` 必填）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cleanup: Option<CleanupRelation>,
        /// 用户直接操作关系（`UserDirect` 必填）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub user_direct: Option<UserDirectRelation>,
        /// 宿主对模型计划的变换记录：有它**不改变**来源。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub host_transform: Option<HostTransformRecord>,
        /// **附加**因果引用（视觉转述 / 最终验收 / 原模型请求 …）。
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub additional_causal_refs: Vec<CausalReference>,
    }

    /// 动作来源的两级准入判决。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ActionOriginAdmission {
        pub source: ActionSource,
        pub context: ContextKind,
        /// 已**核对到**真实规划请求时的复合键（只有 `ModelPlanned` 有值）。
        pub verified_model_request: Option<PlannedRequestAttempt>,
    }

    impl ActionOriginAdmission {
        /// 模型请求身份是否**已核对**（不是"字段填了"，而是"与真实规划请求一致"）。
        #[must_use]
        pub fn model_request_identity_verified(&self) -> bool {
            self.verified_model_request.is_some()
        }
    }

    impl ActionOrigin {
        /// 第一级：**结构校验**——按来源与上下文类型检查哪些字段必须有、哪些确实不适用。
        ///
        /// 这一级只看这条记录自身，**不足**以受理事实：它看不出来源与真实父对象是否一致
        /// （传入的 `source = HostIncidental` 只是一句话）。受理必须走
        /// [`ActionOrigin::validate_against`] / [`admit_action_origin`]。
        pub fn validate_structure(&self) -> Result<(), RunContractError> {
            validate_origin_reference("action_id", &self.action_id)?;
            if let ActionContext::Conversation(conversation) = &self.context {
                self.validate_conversation_context(conversation)?;
            }
            if let ActionContext::GoalPhase(context) = &self.context {
                context.validate_structure()?;
            }
            if let ActionContext::ControlPlane(context) = &self.context {
                context.validate_structure()?;
                if context.action_id != self.action_id {
                    return Err(RunContractError::action_origin_conflict(
                        "action_id",
                        "控制面上下文里的动作与来源记录描述的动作不一致",
                    ));
                }
                if let Some(relation) = &self.user_direct {
                    if relation.control_operation_id != context.control_operation_id {
                        return Err(RunContractError::action_origin_conflict(
                            "user_direct.control_operation_id",
                            "用户直接操作关系引用的控制操作与上下文里的控制操作不是同一个",
                        ));
                    }
                    if relation.operator != context.operator {
                        return Err(RunContractError::action_origin_conflict(
                            "user_direct.operator",
                            "用户直接操作关系里的操作者与上下文里的操作者不一致",
                        ));
                    }
                    if relation.permission_decision_id != context.permission_decision_id {
                        return Err(RunContractError::action_origin_conflict(
                            "user_direct.permission_decision_id",
                            "用户直接操作关系里的权限决定与上下文里的权限决定不一致",
                        ));
                    }
                }
            }

            for field in self.source.required_relations() {
                self.require_relation(*field)?;
            }
            for field in self.source.not_applicable_relations() {
                if self.has_relation(*field) {
                    return Err(RunContractError::action_origin_dimension_not_applicable(
                        field.as_str(),
                        self.source,
                    ));
                }
            }
            self.validate_relations_structure()?;

            // 工具链归属的可选性由**可信上下文**决定（B-7）：这里只校验"给了就必须有效"。
            if let Some(tool_call_id) = &self.tool_call_id {
                validate_origin_reference("tool_call_id", tool_call_id)?;
            }

            // 宿主对模型计划的变换（坐标变换 / 受控展开）**保留**原模型请求因果关联：
            // 有变换就必须仍然给出产生计划的请求，不得因为"转换过一次"改成无模型来源。
            if let Some(transform) = &self.host_transform {
                validate_origin_reference(
                    "host_transform.algorithm_version",
                    &transform.algorithm_version,
                )?;
                validate_origin_reference("host_transform.source_action_id", &transform.source_action_id)?;
                if !transform.preserves_model_request {
                    return Err(RunContractError::action_origin_conflict(
                        "host_transform.preserves_model_request",
                        "宿主对模型计划的变换必须保留原模型请求因果关联，不得改成无模型来源",
                    ));
                }
                if self.request_attempt_id.is_none() {
                    return Err(RunContractError::incomplete_action_origin(
                        "有宿主变换的动作仍属 ModelPlanned，必须给出产生执行计划的模型请求身份（request_attempt_id）",
                    ));
                }
            }

            for reference in &self.additional_causal_refs {
                reference.validate()?;
            }
            Ok(())
        }

        fn validate_conversation_context(
            &self,
            conversation: &ConversationActionContext,
        ) -> Result<(), RunContractError> {
            // 事实层级由身份自己声明：新事实写入必须显式声明 scope。
            let declared = conversation
                .identity
                .declared_scope()
                .ok_or_else(RunContractError::identity_scope_undeclared)?;
            if declared != conversation.hierarchy {
                return Err(RunContractError::action_origin_conflict(
                    "hierarchy",
                    format!(
                        "事实层级（{}）与身份声明的 scope（{}）不一致：层级由事实本身决定，不随上下文类型改变",
                        conversation.hierarchy.as_str(),
                        declared.as_str()
                    ),
                ));
            }
            conversation
                .identity
                .validate_for(conversation.hierarchy)?;
            if conversation.hierarchy == RunIdentityScope::StepAction
                && conversation.identity.action_id() != Some(self.action_id.as_str())
            {
                return Err(RunContractError::action_origin_conflict(
                    "action_id",
                    "来源记录描述的动作必须与身份里的 action_id 一致",
                ));
            }
            Ok(())
        }

        fn validate_relations_structure(&self) -> Result<(), RunContractError> {
            if let Some(scope) = &self.resource_scope {
                scope.validate()?;
            }
            if let Some(cleanup) = &self.cleanup {
                validate_origin_reference("cleanup.incident_id", &cleanup.incident_id)?;
                validate_origin_reference("cleanup.original_action_id", &cleanup.original_action_id)?;
                if cleanup.original_action_id == self.action_id {
                    return Err(RunContractError::action_origin_conflict(
                        "cleanup.original_action_id",
                        "清理动作必须是一条**独立**的 cleanup action，不能把原 action 自己当成清理",
                    ));
                }
                if !cleanup.recovery_eligible {
                    return Err(RunContractError::action_source_not_established(
                        "清理来源声明自己没有恢复资格：没有恢复资格就不允许作为清理执行",
                    ));
                }
            }
            if let Some(relation) = &self.user_direct {
                validate_origin_reference(
                    "user_direct.control_operation_id",
                    &relation.control_operation_id,
                )?;
                validate_origin_reference(
                    "user_direct.permission_decision_id",
                    &relation.permission_decision_id,
                )?;
            }
            Ok(())
        }

        fn has_relation(&self, field: ActionOriginField) -> bool {
            match field {
                ActionOriginField::RequestAttemptId => self.request_attempt_id.is_some(),
                ActionOriginField::ToolCallId => self.tool_call_id.is_some(),
                ActionOriginField::ParentStepOperation => self.parent_step_operation.is_some(),
                ActionOriginField::HostAlgorithmVersion => self.host_algorithm_version.is_some(),
                ActionOriginField::ResourceScope => self.resource_scope.is_some(),
                ActionOriginField::CleanupRelation => self.cleanup.is_some(),
                ActionOriginField::UserDirectRelation => self.user_direct.is_some(),
            }
        }

        fn require_relation(&self, field: ActionOriginField) -> Result<(), RunContractError> {
            let message = match field {
                ActionOriginField::RequestAttemptId => {
                    "ModelPlanned 必须给出产生执行计划的模型请求身份（request_attempt_id）：执行者是宿主不改变来源，也不能借此省略模型请求身份"
                }
                ActionOriginField::ParentStepOperation => {
                    "HostIncidental 必须给出宿主父步骤 / 操作（parent_step_operation）：观察 / 校验过程中的辅助动作不属于模型计划，必须能核对到真实宿主操作"
                }
                ActionOriginField::HostAlgorithmVersion => {
                    "HostIncidental 必须给出宿主算法版本（host_algorithm_version）：宿主辅助动作要能按版本复核"
                }
                ActionOriginField::ResourceScope => {
                    "本来源必须给出资源 scope（resource_scope）：要说清动的是哪个具体资源"
                }
                ActionOriginField::CleanupRelation => {
                    "SafetyCleanup 必须给出清理关系（incident / 原 action / 恢复资格）"
                }
                ActionOriginField::UserDirectRelation => {
                    "UserDirect 必须给出用户直接操作关系（真实控制操作 + 操作者来源 + 权限决定）"
                }
                ActionOriginField::ToolCallId => {
                    "缺少工具链归属（tool_call_id）：本来源必填"
                }
            };
            match field {
                ActionOriginField::RequestAttemptId => match &self.request_attempt_id {
                    Some(value) => validate_origin_reference(field.as_str(), value),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
                ActionOriginField::ParentStepOperation => match &self.parent_step_operation {
                    Some(value) => validate_origin_reference(field.as_str(), value),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
                ActionOriginField::HostAlgorithmVersion => match &self.host_algorithm_version {
                    Some(value) => validate_origin_reference(field.as_str(), value),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
                ActionOriginField::ResourceScope => match &self.resource_scope {
                    Some(scope) => scope.validate(),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
                ActionOriginField::CleanupRelation => match &self.cleanup {
                    Some(_) => Ok(()),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
                ActionOriginField::UserDirectRelation => match &self.user_direct {
                    Some(_) => Ok(()),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
                ActionOriginField::ToolCallId => match &self.tool_call_id {
                    Some(value) => validate_origin_reference(field.as_str(), value),
                    None => Err(RunContractError::incomplete_action_origin(message)),
                },
            }
        }

        /// 这条来源记录描述的是不是这个动作（与回执 / 身份的 `action_id` 绑定）。
        pub fn belongs_to(&self, action_id: &str) -> Result<(), RunContractError> {
            if self.action_id != action_id {
                return Err(RunContractError::action_origin_conflict(
                    "action_id",
                    format!(
                        "来源记录的动作（{}）与回执的动作（{action_id}）不一致",
                        self.action_id
                    ),
                ));
            }
            Ok(())
        }

        /// 第二级：**可信关联校验**（在前一级之上，必须消费可信上下文）。
        ///
        /// 逐条核对来源记录与**真实父对象**，不能只信传入的 `source` 字符串：
        ///
        /// - `ModelPlanned` 且没有 `request_attempt_id` ⇒ 身份不完整（第一级已拦）；
        ///   给了但与**真正产生该执行计划的请求**不一致 ⇒ 拒绝（不得随手选最近一次请求）。
        /// - `HostIncidental` 且核对不到宿主父操作记录 ⇒ 来源不成立。
        /// - `UserDirect` 且宿主登记里没有这条控制操作（模型输入声称"用户点击"）⇒ 来源不成立。
        /// - `SafetyCleanup` 且没有原 action / incident / 恢复资格 ⇒ 拒绝作为清理执行。
        /// - 请求属于其它运行且没有有效父子关联 ⇒ 拒绝。
        /// - 存在工具调用关系却故意不携带 `tool_call_id` ⇒ 身份不完整（B-7）。
        pub fn validate_against(
            &self,
            authority: &dyn ActionOriginAuthority,
        ) -> Result<ActionOriginAdmission, RunContractError> {
            // 两层缺一不可：结构不过就没有必要去核对父对象。
            self.validate_structure()?;

            // ---- 上下文核对：来源记录必须挂在真实上下文上 ----
            match &self.context {
                ActionContext::Conversation(conversation) => {
                    let scope = authority.conversation_scope().ok_or_else(|| {
                        RunContractError::action_source_not_established(
                            "聊天 CU 的动作来源必须有宿主当前的会话上下文（工作区 / 房间 / 会话 / 公开轮次）：核对不到就不是聊天上下文里的动作",
                        )
                    })?;
                    // 外来房间 / 工作区 / 会话 / 轮次一律拒绝。
                    scope.validate_identity(&conversation.identity)?;
                }
                ActionContext::ControlPlane(context) => {
                    let record = authority
                        .control_operation(&context.control_operation_id)
                        .ok_or_else(|| {
                            RunContractError::action_source_not_established(format!(
                                "控制面动作引用的控制操作 `{}` 在宿主登记里不存在：ControlPlane 上下文必须来自**输入前已建立**的真实控制操作，随机 ID 不构成上下文",
                                context.control_operation_id
                            ))
                        })?;
                    record.matches_claim(context)?;
                }
                // **Goal 阶段：结构可以表达，但受理必须等宿主能核对真实父运行 —— 当前一律拒绝。**
                //
                // 这是刻意的 fail-closed，不是"未实现所以先放过"：强父关系的全部意义就是
                // 「这条 CU 属于**哪一次真实 Goal 阶段执行**」，而宿主权威目前**没有**按
                // `phase_run_id` 核对真实阶段运行的能力。若在这里放行，`phase_run_id` 就退化成
                // 一句自称——正是裁决 §5.2 明令禁止的"现场在同房间的活跃运行里选一个"。
                //
                // 因此：表达力（`ContextKind::GoalPhase`）已经具备，受理能力等宿主登记补齐后
                // 再在此处改为真实核对；在那之前，任何 Goal 阶段来源都**进不来**。
                ActionContext::GoalPhase(context) => {
                    context.validate_structure()?;
                    return Err(RunContractError::action_source_not_established(format!(
                        "Goal 阶段动作的来源暂不能被受理：宿主尚未能按真实阶段运行 `{}`（goal `{}` / phase `{}`）核对强父关系；\
                         在补齐之前不得放行，也不得改用 ControlPlane 或 UserDirect 绕过",
                        context.phase_run_id, context.goal_id, context.phase_id
                    )));
                }
            }

            let run_id = self.context.run_id().to_string();

            // ---- 来源核对：按来源核对真实父对象 ----
            let verified_model_request = match self.source {
                ActionSource::ModelPlanned => {
                    let claimed = self
                        .request_attempt_id
                        .as_deref()
                        .expect("结构校验已保证 ModelPlanned 带 request_attempt_id");
                    let producer = authority.plan_producer(&self.action_id).ok_or_else(|| {
                        RunContractError::action_origin_conflict(
                            "request_attempt_id",
                            format!(
                                "核对不到真正产生动作 {} 执行计划的请求：不得随手把最近一次请求填进来当主要来源",
                                self.action_id
                            ),
                        )
                    })?;
                    if producer.stable_key() != claimed {
                        return Err(RunContractError::action_origin_conflict(
                            "request_attempt_id",
                            format!(
                                "来源声明的规划请求 `{claimed}` 与真正产生该执行计划的请求 `{}` 不一致：局部 attempt 编号、provider trace、逻辑请求 ID、外层 `computer_use_perform` 的 call id 都不能冒充内部规划 attempt",
                                producer.stable_key()
                            ),
                        ));
                    }
                    ensure_runs_are_related(authority, &run_id, producer.run_id())?;
                    Some(producer.clone())
                }
                ActionSource::HostIncidental => {
                    let operation_id = self
                        .parent_step_operation
                        .as_deref()
                        .expect("结构校验已保证 HostIncidental 带 parent_step_operation");
                    let record = authority.host_operation(operation_id).ok_or_else(|| {
                        RunContractError::action_source_not_established(format!(
                            "HostIncidental 的来源不成立：核对不到宿主父操作记录 `{operation_id}`——宿主辅助动作必须有真实父操作，不是自报一个来源字符串"
                        ))
                    })?;
                    if Some(record.algorithm_version.as_str()) != self.host_algorithm_version.as_deref() {
                        return Err(RunContractError::action_origin_conflict(
                            "host_algorithm_version",
                            format!(
                                "宿主算法版本与真实宿主操作记录不一致（记录为 {}）",
                                record.algorithm_version
                            ),
                        ));
                    }
                    if record.run_id != run_id {
                        return Err(RunContractError::action_origin_conflict(
                            "parent_step_operation",
                            format!(
                                "宿主父操作属于运行 {}，本来源记录属于运行 {run_id}：辅助动作必须挂在它真正所属的运行 / 步骤上",
                                record.run_id
                            ),
                        ));
                    }
                    if let Some(step_id) = self.context.step_id() {
                        if record.step_id != step_id {
                            return Err(RunContractError::action_origin_conflict(
                                "parent_step_operation",
                                format!(
                                    "宿主父操作的步骤（{}）与本来源记录的步骤（{step_id}）不一致",
                                    record.step_id
                                ),
                            ));
                        }
                    }
                    None
                }
                ActionSource::SafetyCleanup => {
                    let cleanup = self
                        .cleanup
                        .as_ref()
                        .expect("结构校验已保证 SafetyCleanup 带 cleanup");
                    let incident = authority
                        .cleanup_incident(&cleanup.incident_id)
                        .ok_or_else(|| {
                            RunContractError::action_source_not_established(format!(
                                "SafetyCleanup 不能成立：核对不到 incident `{}` 与原 action `{}`——释放 / 停止这类安全收尾必须能追溯到真的发生过的原动作",
                                cleanup.incident_id, cleanup.original_action_id
                            ))
                        })?;
                    if incident.original_action_id != cleanup.original_action_id {
                        return Err(RunContractError::action_origin_conflict(
                            "cleanup.original_action_id",
                            format!(
                                "清理引用的原 action（{}）与 incident 记录里的原 action（{}）不一致",
                                cleanup.original_action_id, incident.original_action_id
                            ),
                        ));
                    }
                    if !incident.recovery_eligible {
                        return Err(RunContractError::action_source_not_established(format!(
                            "incident `{}` 不具备恢复资格：不得作为清理执行的来源",
                            incident.incident_id
                        )));
                    }
                    ensure_runs_are_related(authority, &run_id, &incident.run_id)?;
                    None
                }
                ActionSource::UserDirect => {
                    let relation = self
                        .user_direct
                        .as_ref()
                        .expect("结构校验已保证 UserDirect 带 user_direct");
                    let record = authority
                        .control_operation(&relation.control_operation_id)
                        .ok_or_else(|| {
                            RunContractError::action_source_not_established(format!(
                                "UserDirect 的来源不成立：宿主登记里没有控制操作 `{}`——模型输入里声称「用户点击」不构成来源",
                                relation.control_operation_id
                            ))
                        })?;
                    if record.operator != relation.operator {
                        return Err(RunContractError::action_origin_conflict(
                            "user_direct.operator",
                            format!(
                                "操作者来源（{}）与真实控制操作记录（{}）不一致",
                                relation.operator.as_str(),
                                record.operator.as_str()
                            ),
                        ));
                    }
                    if record.permission_decision_id != relation.permission_decision_id {
                        return Err(RunContractError::action_origin_conflict(
                            "user_direct.permission_decision_id",
                            "权限决定与真实控制操作记录不一致",
                        ));
                    }
                    if let Some(scope) = &self.resource_scope {
                        if &record.resource_scope != scope {
                            return Err(RunContractError::action_origin_conflict(
                                "resource_scope",
                                "资源 scope 与真实控制操作记录不一致",
                            ));
                        }
                    }
                    if record.action_id != self.action_id {
                        return Err(RunContractError::action_origin_conflict(
                            "action_id",
                            format!(
                                "控制操作登记的动作（{}）与实际动作（{}）不是同一个",
                                record.action_id, self.action_id
                            ),
                        ));
                    }
                    ensure_runs_are_related(authority, &run_id, &record.run_id)?;
                    None
                }
            };

            // ---- 工具调用关系（B-7）：可选性由**可信上下文**决定，不由调用方挑 ----
            match (
                authority.tool_call_relation(&self.action_id),
                self.tool_call_id.as_deref(),
            ) {
                (None, None) => {}
                (None, Some(claimed)) => {
                    return Err(RunContractError::action_origin_conflict(
                        "tool_call_id",
                        format!(
                            "工具归属不得虚构：动作 `{}` 在可信上下文里没有工具调用关系，来源却声称 `{claimed}`",
                            self.action_id
                        ),
                    ));
                }
                (Some(relation), None) => {
                    return Err(RunContractError::incomplete_action_origin(format!(
                        "动作 `{}` 存在真实工具调用关系（tool_call_id = {}），来源却缺省工具归属：字段可选不等于工具链里的动作可以漏传",
                        self.action_id, relation.tool_call_id
                    )));
                }
                (Some(relation), Some(claimed)) if relation.tool_call_id != claimed => {
                    return Err(RunContractError::action_origin_conflict(
                        "tool_call_id",
                        format!(
                            "工具归属与真实工具调用关系不一致（真实为 {}，来源声称 {claimed}）",
                            relation.tool_call_id
                        ),
                    ));
                }
                (Some(_), Some(_)) => {}
            }

            // ---- 附加因果引用：补全关系，但不成立来源；宿主引用必须指向真实登记过的请求 ----
            for reference in &self.additional_causal_refs {
                if !reference.claimant.is_trusted_evidence() {
                    // 模型输入里的断言只是资料：它既不构成来源，也不参与父对象核对。
                    continue;
                }
                if reference.kind == CausalReferenceKind::RequestAttempt
                    && authority.known_request(&reference.reference).is_none()
                {
                    return Err(RunContractError::action_origin_conflict(
                        "additional_causal_refs",
                        format!(
                            "附加因果引用指向的请求 `{}` 在宿主登记里不存在：附加引用同样不得随手填",
                            reference.reference
                        ),
                    ));
                }
            }

            Ok(ActionOriginAdmission {
                source: self.source,
                context: self.context.kind(),
                verified_model_request,
            })
        }
    }

    /// 动作来源的**两级准入**（结构校验 + 可信关联校验，缺一不可）。
    ///
    /// 它**不是**旧 [`RunIdentity::validate`] 的放松版：旧无参 `validate()` 的严格语义
    /// 原样保留，`Turn` 校验也原样不动。本入口只适用于"带来源记录的动作事实"，
    /// 且**必须**消费可信上下文（[`ActionOriginAuthority`]）。
    pub fn admit_action_origin(
        origin: &ActionOrigin,
        authority: &dyn ActionOriginAuthority,
    ) -> Result<ActionOriginAdmission, RunContractError> {
        origin.validate_against(authority)
    }

    /// 运行关联记录：run 属于哪个容器、是否挂在父运行下。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RunRelationRecord {
        pub run_id: String,
        pub workspace_id: String,
        pub room_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parent_run: Option<ParentRunLink>,
    }

    impl RunRelationRecord {
        #[must_use]
        pub fn new(run_id: impl Into<String>, workspace_id: impl Into<String>, room_id: impl Into<String>) -> Self {
            Self {
                run_id: run_id.into(),
                workspace_id: workspace_id.into(),
                room_id: room_id.into(),
                parent_run: None,
            }
        }

        #[must_use]
        pub fn with_parent(mut self, parent: ParentRunLink) -> Self {
            self.parent_run = Some(parent);
            self
        }
    }

    /// 工具调用关系：该动作是否经由工具链、真实 `tool_call_id` 是什么。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ToolCallRelation {
        pub action_id: String,
        pub tool_call_id: String,
    }

    impl ToolCallRelation {
        #[must_use]
        pub fn new(action_id: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
            Self {
                action_id: action_id.into(),
                tool_call_id: tool_call_id.into(),
            }
        }
    }

    /// **可信因果上下文**：来源记录必须核对的真实父对象。
    ///
    /// 生产实现应当从事实存储 / 控制操作登记读取**真实记录**；实现不得由调用方声称
    /// （只信传入的字符串正是本节要禁止的做法）。测试与进程内事实用
    /// [`TrustedOriginContext`]。
    pub trait ActionOriginAuthority {
        /// 宿主当前的聊天上下文（工作区 / 房间 / 会话 / 公开轮次）。
        fn conversation_scope(&self) -> Option<&RunScopeContext>;
        fn run_relation(&self, run_id: &str) -> Option<&RunRelationRecord>;
        /// **真正产生该动作执行计划**的内部规划请求。
        fn plan_producer(&self, action_id: &str) -> Option<&PlannedRequestAttempt>;
        /// 已登记的内部规划请求（按稳定复合键查）。
        fn known_request(&self, stable_key: &str) -> Option<&PlannedRequestAttempt>;
        fn tool_call_relation(&self, action_id: &str) -> Option<&ToolCallRelation>;
        fn host_operation(&self, operation_id: &str) -> Option<&HostOperationRecord>;
        fn cleanup_incident(&self, incident_id: &str) -> Option<&CleanupIncidentRecord>;
        fn control_operation(&self, control_operation_id: &str) -> Option<&ControlOperationRecord>;
    }

    /// 内存可信上下文（测试与"进程内一次性事实"用；生产实现接事实存储）。
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct TrustedOriginContext {
        conversation_scope: Option<RunScopeContext>,
        runs: std::collections::BTreeMap<String, RunRelationRecord>,
        plan_producers: std::collections::BTreeMap<String, PlannedRequestAttempt>,
        requests: PlannedAttemptRegistry,
        tool_relations: std::collections::BTreeMap<String, ToolCallRelation>,
        host_operations: std::collections::BTreeMap<String, HostOperationRecord>,
        cleanup_incidents: std::collections::BTreeMap<String, CleanupIncidentRecord>,
        control_operations: ControlPlaneOperations,
    }

    impl TrustedOriginContext {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        #[must_use]
        pub fn with_conversation_scope(mut self, scope: RunScopeContext) -> Self {
            self.conversation_scope = Some(scope);
            self
        }

        pub fn register_run(&mut self, record: RunRelationRecord) -> Result<(), RunContractError> {
            validate_origin_reference("run_relation.run_id", &record.run_id)?;
            validate_origin_reference("run_relation.workspace_id", &record.workspace_id)?;
            validate_origin_reference("run_relation.room_id", &record.room_id)?;
            if let Some(parent) = &record.parent_run {
                parent.validate(&record.run_id)?;
            }
            self.runs.insert(record.run_id.clone(), record);
            Ok(())
        }

        /// 登记"这一次请求产生了这个动作的计划"（**主要来源**的唯一合法出处）。
        pub fn register_plan_producer(
            &mut self,
            action_id: impl Into<String>,
            attempt: PlannedRequestAttempt,
        ) -> StablePlannedAttemptId {
            let stable = self.requests.establish(attempt.clone());
            self.plan_producers.insert(action_id.into(), attempt);
            stable
        }

        pub fn register_tool_call_relation(
            &mut self,
            action_id: impl Into<String>,
            tool_call_id: impl Into<String>,
        ) -> Result<(), RunContractError> {
            let relation = ToolCallRelation::new(action_id, tool_call_id);
            validate_origin_reference("tool_call_relation.action_id", &relation.action_id)?;
            validate_origin_reference("tool_call_relation.tool_call_id", &relation.tool_call_id)?;
            self.tool_relations
                .insert(relation.action_id.clone(), relation);
            Ok(())
        }

        pub fn register_host_operation(
            &mut self,
            record: HostOperationRecord,
        ) -> Result<(), RunContractError> {
            validate_origin_reference("host_operation.operation_id", &record.operation_id)?;
            validate_origin_reference("host_operation.run_id", &record.run_id)?;
            validate_origin_reference("host_operation.step_id", &record.step_id)?;
            validate_origin_reference("host_operation.algorithm_version", &record.algorithm_version)?;
            self.host_operations
                .insert(record.operation_id.clone(), record);
            Ok(())
        }

        pub fn register_cleanup_incident(
            &mut self,
            record: CleanupIncidentRecord,
        ) -> Result<(), RunContractError> {
            validate_origin_reference("cleanup_incident.incident_id", &record.incident_id)?;
            validate_origin_reference("cleanup_incident.run_id", &record.run_id)?;
            validate_origin_reference(
                "cleanup_incident.original_action_id",
                &record.original_action_id,
            )?;
            self.cleanup_incidents
                .insert(record.incident_id.clone(), record);
            Ok(())
        }

        /// 建立控制操作（见 [`ControlPlaneOperations::establish`] 的三条要求）。
        pub fn establish_control_operation(
            &mut self,
            request: ControlOperationRequest,
        ) -> Result<ControlPlaneContext, RunContractError> {
            self.control_operations.establish(request)
        }

        pub fn adopt_control_operation(
            &mut self,
            record: ControlOperationRecord,
        ) -> Result<(), RunContractError> {
            self.control_operations.adopt(record)
        }

        #[must_use]
        pub fn control_operations(&self) -> &ControlPlaneOperations {
            &self.control_operations
        }
    }

    impl ActionOriginAuthority for TrustedOriginContext {
        fn conversation_scope(&self) -> Option<&RunScopeContext> {
            self.conversation_scope.as_ref()
        }

        fn run_relation(&self, run_id: &str) -> Option<&RunRelationRecord> {
            self.runs.get(run_id)
        }

        fn plan_producer(&self, action_id: &str) -> Option<&PlannedRequestAttempt> {
            self.plan_producers.get(action_id)
        }

        fn known_request(&self, stable_key: &str) -> Option<&PlannedRequestAttempt> {
            self.requests.lookup(stable_key)
        }

        fn tool_call_relation(&self, action_id: &str) -> Option<&ToolCallRelation> {
            self.tool_relations.get(action_id)
        }

        fn host_operation(&self, operation_id: &str) -> Option<&HostOperationRecord> {
            self.host_operations.get(operation_id)
        }

        fn cleanup_incident(&self, incident_id: &str) -> Option<&CleanupIncidentRecord> {
            self.cleanup_incidents.get(incident_id)
        }

        fn control_operation(&self, control_operation_id: &str) -> Option<&ControlOperationRecord> {
            self.control_operations.operation(control_operation_id)
        }
    }

    /// "看起来像请求身份"的 ID 的角色。
    ///
    /// 只有真正的**内部规划 attempt** 才能锚定"产生执行计划的请求"；供应商 trace、
    /// 逻辑请求 ID、外层工具调用（例如 `computer_use_perform` 的 call id）都不是。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RequestIdRole {
        ProviderTraceId,
        LogicalRequestId,
        OuterToolCallId,
        PlanningAttemptId,
    }

    impl RequestIdRole {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::ProviderTraceId => "provider_trace_id",
                Self::LogicalRequestId => "logical_request_id",
                Self::OuterToolCallId => "outer_tool_call_id",
                Self::PlanningAttemptId => "planning_attempt_id",
            }
        }

        /// 该角色的 ID 是否可以用来锚定一次内部规划 attempt。
        #[must_use]
        pub const fn may_anchor_a_planning_attempt(self) -> bool {
            matches!(self, Self::PlanningAttemptId)
        }
    }

    /// 核对某个外部 ID 的角色：**不得冒充**内部规划 attempt（裁决第 12 条）。
    pub fn validate_request_id_role(
        role: RequestIdRole,
        value: &str,
    ) -> Result<(), RunContractError> {
        if !role.may_anchor_a_planning_attempt() {
            return Err(RunContractError::not_a_planning_attempt(role, value));
        }
        validate_origin_reference("request_attempt_id", value)
    }

    /// 内部规划请求的**稳定复合键**：`run_id + logical_request_id + attempt_id`。
    ///
    /// 局部 attempt 编号（"第 2 次请求"）只在同一逻辑请求内唯一，**不能**当全局 ID；
    /// 复用它之前必须先建立这一次稳定映射（见 [`PlannedAttemptRegistry`]）。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PlannedRequestAttempt {
        run_id: String,
        logical_request_id: String,
        attempt_id: String,
    }

    impl PlannedRequestAttempt {
        pub fn new(
            run_id: impl Into<String>,
            logical_request_id: impl Into<String>,
            attempt_id: impl Into<String>,
        ) -> Result<Self, RunContractError> {
            let attempt = Self {
                run_id: run_id.into(),
                logical_request_id: logical_request_id.into(),
                attempt_id: attempt_id.into(),
            };
            attempt.validate()?;
            Ok(attempt)
        }

        /// 复用 `UsageAttempt{run_id, logical_request_id, attempt_id}` **之前**先核对
        /// 三个维度都是有效值：局部 attempt 编号、逻辑请求 ID 都不能单独当身份。
        pub fn from_usage_attempt(attempt: &UsageAttempt) -> Result<Self, RunContractError> {
            Self::new(
                attempt.run_id.clone(),
                attempt.logical_request_id.clone(),
                attempt.attempt_id.clone(),
            )
        }

        fn validate(&self) -> Result<(), RunContractError> {
            validate_origin_reference("planned_request.run_id", &self.run_id)?;
            validate_origin_reference(
                "planned_request.logical_request_id",
                &self.logical_request_id,
            )?;
            validate_origin_reference("planned_request.attempt_id", &self.attempt_id)
        }

        #[must_use]
        pub fn run_id(&self) -> &str {
            &self.run_id
        }

        #[must_use]
        pub fn logical_request_id(&self) -> &str {
            &self.logical_request_id
        }

        #[must_use]
        pub fn attempt_id(&self) -> &str {
            &self.attempt_id
        }

        /// 稳定复合键（唯一标识这一次真实的规划请求）。
        #[must_use]
        pub fn stable_key(&self) -> String {
            format!("{}#{}#{}", self.run_id, self.logical_request_id, self.attempt_id)
        }
    }

    /// 稳定映射结果（复合键的一次性映射），**不是**"最近一次请求"的编号。
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct StablePlannedAttemptId(String);

    impl StablePlannedAttemptId {
        #[must_use]
        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    /// 规划 attempt 的登记表：把复合键**一次性**映射成稳定 id 之后复用。
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct PlannedAttemptRegistry {
        attempts: std::collections::BTreeMap<String, PlannedRequestAttempt>,
    }

    impl PlannedAttemptRegistry {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// 登记一次真实的规划请求并返回它的稳定 id。
        ///
        /// 幂等：同一个复合键（run + 逻辑请求 + attempt）重复登记返回**同一个**稳定 id；
        /// 同一 attempt 编号在不同逻辑请求下是**不同**的规划请求，不会互相顶替。
        pub fn establish(&mut self, attempt: PlannedRequestAttempt) -> StablePlannedAttemptId {
            let key = attempt.stable_key();
            self.attempts.entry(key.clone()).or_insert(attempt);
            StablePlannedAttemptId(key)
        }

        #[must_use]
        pub fn lookup(&self, stable_key: &str) -> Option<&PlannedRequestAttempt> {
            self.attempts.get(stable_key)
        }

        #[must_use]
        pub fn len(&self) -> usize {
            self.attempts.len()
        }

        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.attempts.is_empty()
        }
    }

    /// **宿主因果元数据**：planner → executor 返回包装上的宿主侧字段。
    ///
    /// 由**宿主**在包装层填写，**模型不可声明**：模型的动作 JSON 里没有这些字段
    /// （也**不要**要求模型在动作 JSON 里填 `request_attempt_id`），任何来自模型输入的
    /// 同名字段一律是不可信资料。为了让这条规则成为类型级事实，填充者字段是只有一个
    /// 宿主变体的枚举：`"produced_by":"model"` 在反序列化时**报错**，
    /// 不会被读成别的意思。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct HostCausalMetadata {
        pub attempt: PlannedRequestAttempt,
        pub role: CausalRole,
        /// 这次请求**产出的**动作（宿主填写的宿主侧关联）；没产出动作就是空。
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub produced_action_ids: Vec<String>,
        pub produced_by: HostCausalProducer,
    }

    /// 宿主因果元数据的填充者。**只有宿主**这一种取值。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum HostCausalProducer {
        /// 宿主 planner 包装层。
        PlannerHost,
    }

    impl HostCausalProducer {
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::PlannerHost => "planner_host",
            }
        }
    }

    impl HostCausalMetadata {
        /// 规划请求的包装（**产生执行计划**的那一次请求）。
        pub fn for_plan_producer(
            attempt: PlannedRequestAttempt,
            produced_action_ids: Vec<String>,
        ) -> Self {
            Self {
                attempt,
                role: CausalRole::PlanProducer,
                produced_action_ids,
                produced_by: HostCausalProducer::PlannerHost,
            }
        }

        /// 视觉转述请求的包装（附加因果，**不**是动作的主要来源）。
        #[must_use]
        pub fn for_visual_description(attempt: PlannedRequestAttempt) -> Self {
            Self {
                attempt,
                role: CausalRole::VisualDescription,
                produced_action_ids: Vec::new(),
                produced_by: HostCausalProducer::PlannerHost,
            }
        }

        /// 最终验收请求的包装（附加因果）。
        #[must_use]
        pub fn for_final_verification(attempt: PlannedRequestAttempt) -> Self {
            Self {
                attempt,
                role: CausalRole::FinalVerification,
                produced_action_ids: Vec::new(),
                produced_by: HostCausalProducer::PlannerHost,
            }
        }

        /// 该元数据是否可以作为动作的**主要来源**（只有规划请求可以）。
        #[must_use]
        pub const fn may_be_primary_source(&self) -> bool {
            self.role.may_be_primary_source()
        }

        /// 转成附加因果引用（供 `additional_causal_refs` 使用）。
        #[must_use]
        pub fn as_causal_reference(&self, kind: CausalReferenceKind) -> CausalReference {
            CausalReference::host(self.role, kind, self.attempt.stable_key())
        }

        fn validate(&self) -> Result<(), RunContractError> {
            self.attempt.validate()?;
            for action_id in &self.produced_action_ids {
                validate_origin_reference("host_causal_metadata.produced_action_ids", action_id)?;
            }
            if self.role == CausalRole::PlanProducer && self.produced_action_ids.is_empty() {
                return Err(RunContractError::incomplete_action_origin(
                    "规划请求的宿主因果元数据必须写明它产出的动作：没有产出动作的请求不是任何动作的主要来源",
                ));
            }
            Ok(())
        }
    }

    /// planner → executor 的返回包装：动作 + **宿主填写的**因果元数据。
    ///
    /// 模型只产出动作（其 JSON 里没有 attempt 身份）；动作与元数据的一致性由宿主核对，
    /// 不靠模型声明。
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PlannedActionEnvelope {
        /// 模型产出的动作（`None` = 这次请求没有产出动作，例如"无安全动作"）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub action_id: Option<String>,
        pub causal: HostCausalMetadata,
    }

    impl PlannedActionEnvelope {
        /// 宿主侧构造器：包装里的动作必须出现在元数据声明的产出动作里。
        pub fn new(
            action_id: Option<String>,
            causal: HostCausalMetadata,
        ) -> Result<Self, RunContractError> {
            causal.validate()?;
            if let Some(action_id) = &action_id {
                validate_origin_reference("action_id", action_id)?;
                if !causal.produced_action_ids.iter().any(|id| id == action_id) {
                    return Err(RunContractError::action_origin_conflict(
                        "causal.produced_action_ids",
                        format!(
                            "包装里的动作 `{action_id}` 不在宿主因果元数据声明的产出动作里：动作与因果元数据必须由宿主一起写"
                        ),
                    ));
                }
            }
            Ok(Self { action_id, causal })
        }

        /// 该包装是否可以作为动作 `action_id` 的**主要来源**。
        #[must_use]
        pub fn is_primary_source_for(&self, action_id: &str) -> bool {
            self.causal.may_be_primary_source()
                && self
                    .causal
                    .produced_action_ids
                    .iter()
                    .any(|id| id == action_id)
        }
    }

    fn ensure_runs_are_related(
        authority: &dyn ActionOriginAuthority,
        own_run_id: &str,
        other_run_id: &str,
    ) -> Result<(), RunContractError> {
        if own_run_id == other_run_id {
            return Ok(());
        }
        let own = authority.run_relation(own_run_id);
        let other = authority
            .run_relation(other_run_id)
            .ok_or_else(|| {
                RunContractError::action_source_not_established(format!(
                    "关联的请求 / 原动作属于运行 `{other_run_id}`，这个运行在宿主登记里不存在：不能挂在没登记过的运行上"
                ))
            })?;
        let own_links_to_other = own
            .and_then(|record| record.parent_run.as_ref())
            .is_some_and(|parent| parent.parent_run_id == other_run_id);
        let other_links_to_own = other
            .parent_run
            .as_ref()
            .is_some_and(|parent| parent.parent_run_id == own_run_id);
        if own_links_to_other || other_links_to_own {
            return Ok(());
        }
        Err(RunContractError::action_origin_conflict(
            "run_id",
            format!(
                "关联对象属于其它运行（{other_run_id}）且与本运行（{own_run_id}）没有有效父子关联：拒绝"
            ),
        ))
    }

    /// 来源关联值的通用校验：空串 / 占位值 / 控制字符都不是有效的关联。
    fn validate_origin_reference(field: &str, value: &str) -> Result<(), RunContractError> {
        if value.trim().is_empty() {
            return Err(RunContractError::invalid_identity(field));
        }
        if is_placeholder_identity_value(value) {
            return Err(RunContractError::placeholder_identity_value(field));
        }
        if value.chars().any(char::is_control) {
            return Err(RunContractError::invalid_identity(format!(
                "{field}（含控制字符，不是有效的来源关联值）"
            )));
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
}
// 回执
// ---------------------------------------------------------------------------

/// 对外部输入是否已真正发出的事实；未知不能被降格为未发送。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputDelivery {
    NotSent,
    MayHaveBeenSent,
    Sent,
}

/// 动作对目标表面的可观察效果，不等同于输入已经发送或验收通过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    NotObserved,
    EffectObserved,
    NoEffectObserved,
    Inconclusive,
}

/// 本轮目标是否被验收通过；准备动作和已有内容不得冒充通过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalVerdict {
    NotChecked,
    Passed,
    Failed,
    Inconclusive,
}

/// 本执行器自身的输入释放事实，不代表整个桌面没有残留输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputReleaseStatus {
    NotNeeded,
    Released,
    Unknown,
}

/// `partial` 的三态读法。
///
/// **没有"完整执行"这个变体**——这是有意的：`partial = false` 只表示
/// "没有观测到部分执行"，**不构成**"完整执行"的证据（完整执行由
/// `path_completed` / `input_delivery` 承载，见 `ActionReceipt::path_proven_complete`）。
/// 因此"把 `false` 读成完整执行"在类型上无法表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartialObservation {
    /// 无法确定（字段缺省 / 旧 JSON 里没有这个键）。**不是**"未部分"。
    Unknown,
    /// 明确观测到部分执行。
    Partial,
    /// 明确没有观测到部分执行（仍不等于完整执行）。
    NotPartial,
}

/// 一次动作的事实回执。路径专用字段只可用于路径动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReceipt {
    pub action_id: String,
    pub input_delivery: InputDelivery,
    /// 三态：`Some(true)` = 明确部分执行；`Some(false)` = 明确**未观测到**部分执行
    /// （**不等于**完整执行）；`None` / 缺键 = 未知。老 JSON 缺键读成 `None`，
    /// 绝不会被默认成 `false`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_completed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_point_count: Option<u32>,
    pub effect: EffectStatus,
    pub goal_verdict: GoalVerdict,
    pub input_release: InputReleaseStatus,
}

impl ActionReceipt {
    /// 拒绝已知矛盾，保留真正未知值而不是强行推断。
    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.action_id.trim().is_empty() {
            return Err(RunContractError::invalid_identity("action_id"));
        }
        if self.input_delivery == InputDelivery::NotSent {
            if self.partial == Some(true) {
                return Err(RunContractError::contradiction(
                    "not_sent 与 partial=true 不可同时成立",
                ));
            }
            if self.path_completed == Some(true) {
                return Err(RunContractError::contradiction(
                    "not_sent 与 path_completed=true 不可同时成立",
                ));
            }
            if self.confirmed_point_count.unwrap_or(0) > 0 {
                return Err(RunContractError::contradiction(
                    "not_sent 与已确认路径点不可同时成立",
                ));
            }
        }
        if self.input_delivery == InputDelivery::MayHaveBeenSent
            && self.path_completed == Some(true)
        {
            return Err(RunContractError::contradiction(
                "may_have_been_sent 与 path_completed=true 不可同时成立",
            ));
        }
        if self.path_completed.is_none() && self.confirmed_point_count.is_some() {
            return Err(RunContractError::contradiction(
                "confirmed_point_count 需要同时声明 path_completed",
            ));
        }
        Ok(())
    }

    /// `partial` 的三态读法（旧 JSON 缺键 → `Unknown`）。
    #[must_use]
    pub fn partial_observation(&self) -> PartialObservation {
        match self.partial {
            Some(true) => PartialObservation::Partial,
            Some(false) => PartialObservation::NotPartial,
            None => PartialObservation::Unknown,
        }
    }

    /// 输入是否**可能已经开始**（输入前拒绝的对立面）。
    ///
    /// 未知一律按"可能已经开始"处理（保守方向）：`may_have_been_sent`、
    /// `partial` / `path_completed` 未知、还有未确认的释放，都不算"没开始"。
    #[must_use]
    pub fn may_have_started_input(&self) -> bool {
        self.input_delivery != InputDelivery::NotSent
            || self.partial == Some(true)
            || self.path_completed == Some(true)
            || self.confirmed_point_count.unwrap_or(0) > 0
            || self.input_release != InputReleaseStatus::NotNeeded
    }

    /// 这条回执是否**证明了**路径执行完成。
    ///
    /// 注意：`partial = Some(false)` **不**参与判断（它只表示"没观测到部分执行"）。
    #[must_use]
    pub fn path_proven_complete(&self) -> bool {
        self.input_delivery == InputDelivery::Sent && self.path_completed == Some(true)
    }
}

// ---------------------------------------------------------------------------
// 终态
// ---------------------------------------------------------------------------

/// 控制终态 first-wins；迟到事实可以附加到旧 run，但不能令其重新运行。
///
/// 变体含义见模块级文档"终态映射"一节。`Succeeded` **只**表示该 scope 的运行
/// 正常结束，不表示用户目标已完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTerminalStatus {
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    TimedOut,
    /// 异常中断 / 取消原因不明（**不是**取消）。本轮新增。
    Interrupted,
}

impl RunTerminalStatus {
    #[must_use]
    pub const fn accepts_late_facts(self) -> bool {
        true
    }

    /// 是否表示"该 scope 的运行正常结束"（**不**表示用户目标已完成）。
    #[must_use]
    pub const fn is_scope_success(self) -> bool {
        matches!(self, Self::Succeeded)
    }

    /// 是否据此宣称用户目标已完成。按契约**恒为 false**：目标由 `GoalVerdict` 承载。
    #[must_use]
    pub const fn claims_goal_completion(self) -> bool {
        false
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Interrupted => "interrupted",
        }
    }

    /// 解析已认识的变体；**不认识的返回 `None`**（不得当成功）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "blocked" => Some(Self::Blocked),
            "cancelled" => Some(Self::Cancelled),
            "timed_out" => Some(Self::TimedOut),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// 手写反序列化：**不认识的终态变体一律报错**。
///
/// 这一条是为"旧消费者"写的约束：旧消费者遇到新终态必须响亮地失败，
/// 不得把不认识的终态读成成功（也不得读成任何一个默认值）。
impl<'de> Deserialize<'de> for RunTerminalStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| {
            de::Error::custom(format!(
                "未知的运行终态变体 `{raw}`：不得当作成功或任何默认值"
            ))
        })
    }
}

/// **宿主确认的**取消来源。
///
/// 取消来源必须由宿主写成事实；不写来源的取消一律按"原因不明"
/// （`Interrupted`）处理，**不默认"用户取消"**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelOrigin {
    /// 明确由用户取消（必须由宿主确认；没有确认事实时不得用这个值）。
    User,
    /// 由人类操作员取消（人工解围 / 运维）。
    Operator { operator_id: String },
    /// 预算耗尽。
    BudgetExhausted,
    /// 权限 / 策略撤销。
    PolicyRevoked,
    /// 宿主进程退出 / 迁移（不是用户取消）。
    HostShutdown { reason: String },
    /// 其它宿主可识别的原因码。
    Other { code: String },
}

impl CancelOrigin {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Operator { .. } => "operator",
            Self::BudgetExhausted => "budget_exhausted",
            Self::PolicyRevoked => "policy_revoked",
            Self::HostShutdown { .. } => "host_shutdown",
            Self::Other { .. } => "other",
        }
    }

    /// 是否"用户主动取消"（只有 `User` 是）。
    #[must_use]
    pub const fn is_user_initiated(&self) -> bool {
        matches!(self, Self::User)
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        match self {
            Self::Operator { operator_id } => {
                if operator_id.trim().is_empty() || is_placeholder_identity_value(operator_id) {
                    return Err(RunContractError::incomplete_host_outcome(
                        "operator 取消必须给出 operator_id",
                    ));
                }
            }
            Self::HostShutdown { reason } => {
                if reason.trim().is_empty() {
                    return Err(RunContractError::incomplete_host_outcome(
                        "host_shutdown 取消必须给出原因",
                    ));
                }
            }
            Self::Other { code } => {
                if code.trim().is_empty() || is_placeholder_identity_value(code) {
                    return Err(RunContractError::incomplete_host_outcome(
                        "other 取消必须给出宿主可识别的原因码",
                    ));
                }
            }
            Self::User | Self::BudgetExhausted | Self::PolicyRevoked => {}
        }
        Ok(())
    }
}

/// 宿主**观察到的**运行结局（事实，不是模型自述的结论）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostRunOutcome {
    /// 本 scope 运行正常收尾（**不**表示用户目标已完成）。
    Completed,
    /// 失败：必须保留真实终止原因（`reason`），错误对象可选。
    Failed {
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<RunContractError>,
    },
    /// 中断：取消**已确认**时携带 `confirmed_cancel`（来源必须写明），
    /// 否则只有可得证据（`evidence`，可缺）——取消原因不明就走这一支。
    Interrupted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confirmed_cancel: Option<CancelOrigin>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence: Option<String>,
    },
    /// 仍在运行（非终态）。
    Running,
    /// 已请求取消但尚未结束（非终态）：请求 ≠ 取消，此时**不写终态事实**。
    CancelRequested,
}

impl HostRunOutcome {
    /// 宿主确认的取消。
    #[must_use]
    pub fn cancelled(origin: CancelOrigin) -> Self {
        Self::Interrupted {
            confirmed_cancel: Some(origin),
            evidence: None,
        }
    }

    /// 原因不明 / 异常中断（可以带证据）。
    #[must_use]
    pub fn interrupted(evidence: Option<String>) -> Self {
        Self::Interrupted {
            confirmed_cancel: None,
            evidence,
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed { .. } | Self::Interrupted { .. }
        )
    }

    /// 终态映射；非终态返回 `None`（**不写终态事实**）。
    ///
    /// - `Completed → Succeeded`
    /// - `Failed → Failed`
    /// - 只有**宿主确认的取消** → `Cancelled`；否则 → `Interrupted`
    #[must_use]
    pub fn terminal_status(&self) -> Option<RunTerminalStatus> {
        match self {
            Self::Completed => Some(RunTerminalStatus::Succeeded),
            Self::Failed { .. } => Some(RunTerminalStatus::Failed),
            Self::Interrupted {
                confirmed_cancel: Some(_),
                ..
            } => Some(RunTerminalStatus::Cancelled),
            Self::Interrupted { .. } => Some(RunTerminalStatus::Interrupted),
            Self::Running | Self::CancelRequested => None,
        }
    }

    /// 取消来源：只有确认过的取消才有。
    #[must_use]
    pub fn cancel_origin(&self) -> Option<&CancelOrigin> {
        match self {
            Self::Interrupted {
                confirmed_cancel: Some(origin),
                ..
            } => Some(origin),
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        match self {
            Self::Failed { reason, error } => {
                if reason.trim().is_empty() || is_placeholder_identity_value(reason) {
                    return Err(RunContractError::incomplete_host_outcome(
                        "failed 必须携带真实的终止原因",
                    ));
                }
                if let Some(error) = error {
                    if error.code.trim().is_empty() {
                        return Err(RunContractError::incomplete_host_outcome(
                            "failed 携带的错误必须有 code",
                        ));
                    }
                }
            }
            Self::Interrupted {
                confirmed_cancel,
                evidence,
            } => {
                if let Some(origin) = confirmed_cancel {
                    origin.validate()?;
                }
                if let Some(evidence) = evidence {
                    if evidence.trim().is_empty() || is_placeholder_identity_value(evidence) {
                        return Err(RunContractError::incomplete_host_outcome(
                            "中断证据为空或为占位值",
                        ));
                    }
                }
            }
            Self::Completed | Self::Running | Self::CancelRequested => {}
        }
        Ok(())
    }
}

/// 统一错误分类，保留恢复责任而不由 UI 猜测重试方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunContractError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub retry_owner: RetryOwner,
}

impl RunContractError {
    fn new(
        code: &str,
        message: impl Into<String>,
        retryable: bool,
        retry_owner: RetryOwner,
    ) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retryable,
            retry_owner,
        }
    }

    #[must_use]
    pub fn invalid_identity(field: impl std::fmt::Display) -> Self {
        Self::new(
            "invalid_identity",
            format!("{field} 不能为空"),
            false,
            RetryOwner::None,
        )
    }

    /// 本应存在却缺失的维度：**不能**当成"不适用"。
    #[must_use]
    pub fn incomplete_identity(dimension: impl std::fmt::Display) -> Self {
        Self::new(
            "incomplete_identity",
            format!("{dimension} 本应存在却缺失：身份不完整，不能当成「不适用」"),
            false,
            RetryOwner::Controller,
        )
    }

    /// 用 `""` / `"unknown"` / `"n/a"` / `"0"` / 复制容器 id 等方式凑出来的维度值。
    #[must_use]
    pub fn placeholder_identity_value(field: &str) -> Self {
        Self::new(
            "placeholder_identity_value",
            format!("{field} 是占位值（空、unknown/n-a/0 或抄自别的维度）：不作为事实受理"),
            false,
            RetryOwner::Controller,
        )
    }

    /// 不适用维度被提供了值（最典型的场景：把动作事实降级成 turn 事实）。
    #[must_use]
    pub fn dimension_not_applicable(
        dimension: IdentityDimension,
        scope: RunIdentityScope,
        value: &str,
    ) -> Self {
        Self::new(
            "identity_dimension_not_applicable",
            format!(
                "{} 在 {} scope 下不适用，必须缺省；实际给了 `{value}`（降 scope 绕校验会被拒绝）",
                dimension.as_str(),
                scope.as_str()
            ),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn identity_scope_undeclared() -> Self {
        Self::new(
            "identity_scope_undeclared",
            "身份没有声明事实作用域：新事实写入必须显式声明 scope（旧记录请走 validate_persisted）"
                .to_string(),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn identity_scope_mismatch(expected: RunIdentityScope, declared: RunIdentityScope) -> Self {
        Self::new(
            "identity_scope_mismatch",
            format!(
                "事实写入路径要求 {} scope，身份声明的却是 {}：scope 由宿主按事实类型决定，不得自选",
                expected.as_str(),
                declared.as_str()
            ),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn identity_schema_version_missing() -> Self {
        Self::new(
            "identity_schema_version_missing",
            format!("新记录必须持久化身份 schema 版本（当前为 {RUN_IDENTITY_SCHEMA_VERSION}）"),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn unsupported_identity_schema_version(version: u32) -> Self {
        Self::new(
            "unsupported_identity_schema_version",
            format!(
                "身份 schema 版本 {version} 不认识（当前为 {RUN_IDENTITY_SCHEMA_VERSION}）：不得用今天的规则解释未来版本"
            ),
            false,
            RetryOwner::System,
        )
    }

    #[must_use]
    pub fn context_mismatch(field: &str, expected: &str, actual: &str) -> Self {
        Self::new(
            "identity_context_mismatch",
            format!("{field} 与宿主上下文不一致（期望 {expected}，实际 {actual}）：事实挂错了房间/工作区"),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn identity_conflict(field: &str, message: impl Into<String>) -> Self {
        Self::new(
            "identity_conflict",
            message.into(),
            false,
            RetryOwner::Controller,
        )
        .with_field_hint(field)
    }

    #[must_use]
    pub fn invalid_action_evidence(message: impl Into<String>) -> Self {
        Self::new(
            "invalid_action_evidence",
            message.into(),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn placeholder_action_evidence(message: impl Into<String>) -> Self {
        Self::new(
            "placeholder_action_evidence",
            message.into(),
            false,
            RetryOwner::Controller,
        )
    }

    #[must_use]
    pub fn incomplete_host_outcome(message: impl Into<String>) -> Self {
        Self::new(
            "incomplete_host_outcome",
            message.into(),
            false,
            RetryOwner::System,
        )
    }

    #[must_use]
    pub fn contradiction(message: &str) -> Self {
        Self::new(
            "contradictory_action_receipt",
            message,
            false,
            RetryOwner::System,
        )
    }

    /// 来源记录缺了本来源/本上下文必填的关联（例如 `ModelPlanned` 缺
    /// `request_attempt_id`，或工具链里的动作漏传 `tool_call_id`）。
    #[must_use]
    pub fn incomplete_action_origin(message: impl Into<String>) -> Self {
        Self::new(
            "incomplete_action_origin",
            message.into(),
            false,
            RetryOwner::Controller,
        )
    }

    /// 来源**不成立**：核对不到真实父对象（宿主辅助动作没有父操作记录、
    /// 清理没有原 action / 恢复资格、用户直操在宿主登记里没有那条控制操作 …）。
    #[must_use]
    pub fn action_source_not_established(message: impl Into<String>) -> Self {
        Self::new(
            "action_source_not_established",
            message.into(),
            false,
            RetryOwner::Controller,
        )
    }

    /// 来源维度**不适用**：例如给非 `ModelPlanned` 的来源填 `request_attempt_id`
    /// （那正是把动作来源说成套上一层皮）。
    #[must_use]
    pub fn action_origin_dimension_not_applicable(field: &str, source: ActionSource) -> Self {
        Self::new(
            "action_origin_dimension_not_applicable",
            format!(
                "{field} 在 {} 来源下不适用，必须缺省：填上它就等于改写了动作来源",
                source.as_str()
            ),
            false,
            RetryOwner::Controller,
        )
    }

    /// 来源记录与**真实父对象**冲突（工具 id 对不上、请求不属于本运行、
    /// 变换不保留模型来源、声明的规划请求不是真正的产出者 …）。
    #[must_use]
    pub fn action_origin_conflict(field: &str, message: impl Into<String>) -> Self {
        Self::new(
            "action_origin_conflict",
            message.into(),
            false,
            RetryOwner::Controller,
        )
        .with_field_hint(field)
    }

    /// 用"看起来像请求身份的别的 ID"冒充内部规划 attempt
    /// （provider trace / 逻辑请求 ID / 外层工具调用 id）。
    #[must_use]
    pub fn not_a_planning_attempt(role: RequestIdRole, value: &str) -> Self {
        Self::new(
            "not_a_planning_attempt",
            format!(
                "`{value}` 是 {}，不是内部规划 attempt：不得用它冒充产生执行计划的请求",
                role.as_str()
            ),
            false,
            RetryOwner::System,
        )
    }

    /// 遗留运行收敛事实**缺了必须有的事实**（原状态、恢复操作 ID、执行者与资格、
    /// 对账时刻、带证据的历史结论的证据引用 …）。缺失是 unknown，不能补默认值凑过去。
    #[must_use]
    pub fn incomplete_legacy_cu_run_convergence(message: impl Into<String>) -> Self {
        Self::new(
            "incomplete_legacy_cu_run_convergence",
            message.into(),
            false,
            RetryOwner::System,
        )
    }

    /// 遗留运行收敛的**决定不成立**：不是 `Interrupted`、原因码/解释文案/规则版本被改写、
    /// 解释文案断言了禁用措辞、声明了仍允许继续执行、或声明了被禁止的副作用。
    #[must_use]
    pub fn invalid_legacy_cu_run_convergence_decision(message: impl Into<String>) -> Self {
        Self::new(
            "invalid_legacy_cu_run_convergence_decision",
            message.into(),
            false,
            RetryOwner::System,
        )
    }

    fn with_field_hint(self, field: &str) -> Self {
        Self {
            message: format!("{}（{field}）", self.message),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryOwner {
    Controller,
    Model,
    User,
    System,
    None,
}

/// 预算是一次 run 的上限快照；执行器应在每个安全点比较 `deadline_unix_ms`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunBudget {
    pub deadline_unix_ms: u64,
    pub max_actions: u32,
    pub max_replans: u32,
    pub max_request_attempts: u32,
}

impl RunBudget {
    #[must_use]
    pub const fn is_expired_at(self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.deadline_unix_ms
    }
}

// ---------------------------------------------------------------------------
// RD4-01（第五轮裁决 A-1）：遗留 CU 运行的收敛契约
// ---------------------------------------------------------------------------
//
// 这一节只定义**契约**：一条窄范围的"遗留运行收敛事实"的形状、它允许说什么、
// 以及它**不允许**推导什么。"启动时读取候选、拿恢复协调权、按顺序落库"的恢复流程属
// RD4-03，不在本节。
//
// 被处理的对象只有一种：**迁移 v22 之前写入、`workspace_id IS NULL` 且未收尾**的 CU 运行
// （见 `computer_use_store::unrecorded_workspace_active_runs` 的判据）。这类行会永久阻断
// 该 scope 的新 CU 输入，而没有任何机制关闭它们（A-1 的现象）。
//
// 四条硬边界（裁决 A-1 / A-1.5 / A-1.6 / A-1.7）：
//
// 1. **不进 `RunIdentity`**：本事实**不是**身份事实，也没有 workspace 维度。裁决明确
//    禁止把缺 `workspace_id` 的历史行塞进正常 `RunIdentity`（那会逼出哨兵值），因此这里
//    用一个窄范围的独立事实承载它。**同时不得**因此新增任何"缺 workspace 就放行"的通用
//    逃生路径：普通 `Turn` / `StepAction` 身份校验保持原样严格，本事实不进身份准入
//    （`admit_action_fact` 不认识它）。
// 2. **只有一个决定**：`RunTerminalStatus::Interrupted` + 唯一原因码
//    [`LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE`]。它的含义**恰好**是
//    [`LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION`] 那段文案：宿主终止了该遗留运行的
//    继续执行资格；历史工作区归属未记录，原任务结果不能由本次收敛证明。
//    它**不是**用户取消、**不是**"任务执行失败已被完整确认"、**不是**"输入没有发生"、
//    **不是**"鼠标已经释放"、**不是**"目标已完成"——禁用措辞由
//    [`legacy_cu_run_explanation_claims_forbidden_wording`] 逐条拦住。
// 3. **三个维度分开记录、互不推导**：运行控制状态（[`LegacyRunControlState`]）、
//    历史执行结果（[`LegacyRunHistoricalOutcome`]）、输入资源状态
//    （[`LegacyRunInputResourceState`]）是三个独立字段。本模块**不提供**任何从一个推出
//    另一个的函数，因此下面这个组合是可表达的，也是必须可表达的：
//    控制 = 已终止 / 历史目标结果 = 未知 / 工作区 = 未记录 / 输入资源 = 仍隔离。
// 4. **不伪造时间**：`reconciled_at_unix_ms` 是**本次**对账时刻；原实际执行结束时刻只能由
//    带证据的迟到事实补充（`LegacyRunHistoricalOutcome::Evidenced` 必须带证据引用），
//    没有证据就保持未知。**不得**把恢复时间写成当时结束时间。

/// 裁决 A-1 指定的**唯一**原因码。
///
/// 它就是本仓库的稳定码：本节**不再新造**第二个同义码（例如 `legacy_run_closed` 之类），
/// 原因码在事实里是字符串字段，比对时与这个常量**逐字节相等**。
pub const LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE: &str = "legacy_cu_run_closed_at_recovery";

/// 原因码的**唯一**解释文案（逐字取自裁决 A-1）。
///
/// 不许改写、不许增补、不许把它当成"用户取消 / 失败已确认 / 输入没有发生 / 资源已释放 /
/// 目标已完成"中的任何一条。落库与读回都按**逐字节相等**校验
/// （见 [`LegacyCuRunConvergenceDecision::validate`]）。
pub const LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION: &str =
    "宿主在恢复过程中终止了该遗留运行的继续执行资格；历史工作区归属未记录，原任务结果不能由本次收敛证明。";

/// 收敛规则的版本号。规则（判据、允许的结论、幂等口径）变化时必须递增，
/// 这样读回方能区分"哪一版规则写下的收敛事实"，而不是靠猜。
pub const LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION: u32 = 1;

/// 裁决 A-1 逐条列出的**禁用措辞族**：每族第一项是裁决原文的说法，其余是同一断言的
/// 其他写法。任何一项出现在解释文案里都会被拦住。
const LEGACY_CU_RUN_FORBIDDEN_WORDING_FAMILIES: [&[&str]; 5] = [
    &["用户取消", "用户中止"],
    &["失败已确认", "已确认失败", "失败已被", "已被完整确认"],
    &["输入没有发生", "没有发生输入", "未发生输入"],
    &["已释放", "已经释放", "鼠标已经释放"],
    &["目标完成", "目标已完成", "已完成目标"],
];

/// 五条禁用措辞的代表写法（每族第一项），供测试与文档逐条引用。
pub const LEGACY_CU_RUN_FORBIDDEN_CLAIM_WORDINGS: [&str; 5] =
    ["用户取消", "失败已确认", "输入没有发生", "已释放", "目标完成"];

/// 这段文案是否在断言某条禁用结论。命中则返回被命中的那一族代表写法。
///
/// 存在的意义：把"不得写成那五种说法"从注释里的纪律变成**可执行的判定**——
/// 解释文案是常量、随事实落库，写入与读回两侧都跑这个判定。
#[must_use]
pub fn legacy_cu_run_explanation_claims_forbidden_wording(text: &str) -> Option<&'static str> {
    for family in LEGACY_CU_RUN_FORBIDDEN_WORDING_FAMILIES {
        for wording in family {
            if text.contains(wording) {
                return Some(family[0]);
            }
        }
    }
    None
}

/// 来源数据库标识**由本次维护新登记**时必须写明的提示语。
///
/// 新登记的"来源数据库标识"只说明"本事实写在哪个库里"，**不代表过去的归属**：
/// 它不是工作区、不是旧运行的资源 scope，也不能倒推"当时是在哪个工作区跑的"。
pub const LEGACY_CU_RUN_SOURCE_DATABASE_IDENTITY_NOT_PAST_ATTRIBUTION: &str =
    "本次新登记的来源数据库标识只标识本事实所在的数据库，不代表过去的归属";

/// 被处理的遗留运行对象：已确认的来源数据库标识 + 原 CU run ID。
///
/// "已确认"是要求：来源数据库标识必须来自本次恢复读到的库身份，不能由模型或调用方自选。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunSubject {
    /// 本事实**所在**的来源数据库标识。
    pub source_database_identity: String,
    /// 该标识是否由**本次维护新登记**（登记前不存在）。
    ///
    /// 为 true 时 [`Self::source_database_identity_not_past_attribution`] 必须一起写明；
    /// 为 false 时不得给旧身份贴那句话（旧身份就是旧身份，不需要额外否认什么）。
    pub source_database_identity_registered_now: bool,
    /// 原 CU run ID（`computer_use_runs.call_id`）。
    pub original_run_id: String,
    /// 见 [`LegacyCuRunSubject::source_database_identity_registered_now`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_database_identity_not_past_attribution: Option<String>,
}

impl LegacyCuRunSubject {
    /// 自校验：标识与 run id 必须真实（非空、非占位值）。
    pub fn validate(&self) -> Result<(), RunContractError> {
        for (field, value) in [
            ("source_database_identity", &self.source_database_identity),
            ("original_run_id", &self.original_run_id),
        ] {
            if value.trim().is_empty() || is_placeholder_identity_value(value) {
                return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                    format!("{field} 必须是真实值（空或占位值不是身份）"),
                ));
            }
        }
        match (
            self.source_database_identity_registered_now,
            self.source_database_identity_not_past_attribution.as_deref(),
        ) {
            (true, Some(note)) if note == LEGACY_CU_RUN_SOURCE_DATABASE_IDENTITY_NOT_PAST_ATTRIBUTION => {}
            (true, _) => {
                return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                    "来源数据库标识是本次新登记的，必须写明它不代表过去的归属",
                ));
            }
            (false, None) => {}
            (false, Some(_)) => {
                return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                    "来源数据库标识不是本次新登记的，不得贴'不代表过去归属'的说明",
                ));
            }
        }
        Ok(())
    }
}

/// 原始信息：原状态、原 revision、**确实存在的** session／turn 关联。
///
/// 三类字段都是"读过什么就记什么"：读不到就是缺省（`None`），**不得**补默认值、
/// 也不得把别的维度抄过来凑。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunOriginalFacts {
    /// 收敛**之前**该行的状态字面量（原样保存：`state` 列的历史值）。
    pub original_state: String,
    /// 收敛之前该行的 `state_version`（原 revision）。
    pub original_state_version: u64,
    /// 行上确实存在的 session 关联；没有就是 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 行上确实存在的 turn 关联；没有就是 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

impl LegacyCuRunOriginalFacts {
    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.original_state.trim().is_empty() {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "必须记录原状态：没有原状态就无法对账",
            ));
        }
        for (field, value) in [
            ("session_id", self.session_id.as_deref()),
            ("turn_id", self.turn_id.as_deref()),
        ] {
            if let Some(value) = value {
                if value.trim().is_empty() || is_placeholder_identity_value(value) {
                    return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                        format!("{field} 若存在必须是真实关联；空或占位值应缺省"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 事务/崩溃对账键里用到的 "原状态·revision" 部分。
    #[must_use]
    pub fn state_and_revision(&self) -> (&str, u64) {
        (&self.original_state, self.original_state_version)
    }
}

/// 缺失维度：明确记录 `workspace_id` 未记录，以及其他真实缺口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunMissingDimensions {
    /// `workspace_id` 未记录。对本事实处理的遗留行**恒为 true**：
    /// 它只处理"迁移 v22 之前、`workspace_id IS NULL`"的历史行，因此本字段为 false 的
    /// 事实不是遗留收敛（读回方应当据此拒绝）。
    pub workspace_id_unrecorded: bool,
    /// 已知的其它真实缺口（例如历史资源 scope 未记录、原始执行结束时刻未知）。
    ///
    /// 空列表只表示"本次没有识别到别的缺口"，**不表示**"没有缺口"。
    #[serde(default)]
    pub other_gaps: Vec<String>,
}

impl LegacyCuRunMissingDimensions {
    /// 本类事实唯一允许的缺失维度：工作区归属未记录。
    #[must_use]
    pub fn workspace_unrecorded() -> Self {
        Self {
            workspace_id_unrecorded: true,
            other_gaps: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        if !self.workspace_id_unrecorded {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "遗留收敛只处理 workspace_id 未记录的历史行；该字段为 false 不是本类事实",
            ));
        }
        Ok(())
    }
}

/// 本次决定：`Interrupted`、原因码、规则版本、恢复操作 ID。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunConvergenceDecision {
    /// 收敛后的运行控制状态。按裁决**只有** `Interrupted` 一种。
    ///
    /// 其它任何变体都被 [`Self::validate`] 拒绝——包括"看起来更保守"的
    /// `Failed` / `Blocked` / `Cancelled` / `TimedOut` / `Succeeded`：它们都在替历史
    /// 编一个具体结论（失败已确认、被策略拦截、被取消、超时、成功），而本次收敛
    /// **没有**这些证据。
    pub terminal_status: RunTerminalStatus,
    /// 原因码：必须**逐字节**等于 [`LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE`]。
    pub reason_code: String,
    /// 解释文案：必须**逐字节**等于 [`LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION`]。
    pub explanation: String,
    /// 规则版本：必须等于 [`LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION`]。
    pub rule_version: u32,
    /// 本次恢复操作 ID：一次恢复运行一条，用于跨库对账与幂等关联。
    pub recovery_operation_id: String,
}

impl LegacyCuRunConvergenceDecision {
    /// 唯一允许的决定。
    #[must_use]
    pub fn interrupted(recovery_operation_id: impl Into<String>) -> Self {
        Self {
            terminal_status: RunTerminalStatus::Interrupted,
            reason_code: LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE.to_string(),
            explanation: LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION.to_string(),
            rule_version: LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION,
            recovery_operation_id: recovery_operation_id.into(),
        }
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.terminal_status != RunTerminalStatus::Interrupted {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(format!(
                "遗留收敛只允许 Interrupted，不接受 `{}`：其它终态都在替历史编结论",
                self.terminal_status.as_str()
            )));
        }
        if self.reason_code != LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(format!(
                "原因码必须是 `{LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE}`，不接受 `{}`",
                self.reason_code
            )));
        }
        if self.explanation != LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(
                "解释文案必须与契约常量逐字节相同（不得改写、不得增补）",
            ));
        }
        if let Some(claimed) = legacy_cu_run_explanation_claims_forbidden_wording(&self.explanation) {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(format!(
                "解释文案不得断言 `{claimed}`：本次收敛没有这个证据"
            )));
        }
        if self.rule_version != LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(format!(
                "规则版本必须是 {}，不接受 {}",
                LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION, self.rule_version
            )));
        }
        if self.recovery_operation_id.trim().is_empty()
            || is_placeholder_identity_value(&self.recovery_operation_id)
        {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "必须给出真实的恢复操作 ID：幂等与对账都靠它",
            ));
        }
        Ok(())
    }
}

/// 依据：owner／执行者核查、已有回执、提交候选、资源事故引用。
///
/// 四组都是**引用列表**（ID / 路径 / 说明），本事实不复制被引用的内容本身。
/// 空列表表示"本次没有取得这一组依据"，**不得**读成"该组事实不存在"。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunConvergenceEvidence {
    /// owner 侧核查（谁持有过这次运行、有没有别的控制者）。
    #[serde(default)]
    pub owner_checks: Vec<String>,
    /// 执行者侧核查（执行进程 / 服务实例 / 宿主状态）。
    #[serde(default)]
    pub executor_checks: Vec<String>,
    /// 已有的回执 / 动作事实引用。
    #[serde(default)]
    pub existing_receipts: Vec<String>,
    /// **提交候选**：可以证明该行已有真实终态提交（或待确认提交）的候选引用。
    ///
    /// 非空时收敛必须被拒绝（不允许合成 `Interrupted` 覆盖真实终态）——这条判据的
    /// 执行点在 [`crate::fact_store`] 的收敛规则里。
    #[serde(default)]
    pub commit_candidates: Vec<String>,
    /// 资源事故 / 恢复待决阻断的引用。
    #[serde(default)]
    pub resource_incidents: Vec<String>,
}

impl LegacyCuRunConvergenceEvidence {
    /// 是否有任何一组依据。四组全空表示这次收敛**没有留下任何依据引用**——允许，
    /// 但读回方不得据此推断"事实不存在"。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.owner_checks.is_empty()
            && self.executor_checks.is_empty()
            && self.existing_receipts.is_empty()
            && self.commit_candidates.is_empty()
            && self.resource_incidents.is_empty()
    }
}

/// 本次执行者：实际恢复服务实例、恢复控制资格、操作时间。
///
/// 三个字段都是"谁做的这件事"：没有它们，收敛事实就不可追责。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunRecoveryOperator {
    /// 实际写入这条事实的恢复服务实例标识。
    pub recovery_service_instance: String,
    /// 它凭什么做这件事：本次取得的恢复控制资格（授权 / claim / 单实例互斥凭据）。
    pub recovery_control_authority: String,
    /// 操作时刻。
    pub operated_at_unix_ms: u64,
}

impl LegacyCuRunRecoveryOperator {
    pub fn validate(&self) -> Result<(), RunContractError> {
        for (field, value) in [
            (
                "recovery_service_instance",
                &self.recovery_service_instance,
            ),
            (
                "recovery_control_authority",
                &self.recovery_control_authority,
            ),
        ] {
            if value.trim().is_empty() || is_placeholder_identity_value(value) {
                return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                    format!("{field} 必须是真实值：收敛要能追责到具体实例与资格"),
                ));
            }
        }
        if self.operated_at_unix_ms == 0 {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "操作时间不能是 0：它必须是一次真实时刻",
            ));
        }
        Ok(())
    }
}

/// 维度①：运行控制状态——是否还允许继续规划 / 派发 / 执行。
///
/// 收敛**决定**的结果：终止继续执行资格。它与另外两个维度没有推导关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyRunControlState {
    /// 是否仍允许继续规划 / 派发 / 执行。收敛后**恒为 false**。
    pub continuation_allowed: bool,
}

impl LegacyRunControlState {
    /// 收敛决定的控制状态：终止继续执行资格。
    #[must_use]
    pub const fn continuation_terminated_by_recovery() -> Self {
        Self {
            continuation_allowed: false,
        }
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.continuation_allowed {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(
                "收敛事实不得声明该 run 仍允许继续执行",
            ));
        }
        Ok(())
    }
}

/// 维度②：历史执行结果——曾输入什么、是否完成、何时结束。
///
/// **只能**依真实证据补充，没有证据就是 `Unknown`。它**不**由维度①（控制状态）或
/// 维度③（资源状态）推导：`Interrupted` 不蕴含"没完成"，`Unknown` 也不蕴含"没输入"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "snake_case")]
pub enum LegacyRunHistoricalOutcome {
    /// 没有证据：历史结果**未知**。
    ///
    /// 这不是"没有发生输入"、不是"失败"、不是"成功"——它是"不知道"，读回方必须原样呈现。
    Unknown,
    /// 有证据的历史结论（迟到事实补充时使用）。
    Evidenced {
        /// 证据引用（回执 / 观察 / 用量）；**必须非空**，否则就是无证据的结论。
        evidence_refs: Vec<String>,
        /// 有证据的那部分目标结论；**仍然不得**据此宣称用户目标完成
        /// （`GoalVerdict::Passed` 只表示这一次验收通过）。
        goal_verdict: GoalVerdict,
        /// 真实观测到的执行结束时刻；没有观测就缺省。
        ///
        /// **不得**用收敛时刻冒充：恢复时间不是当时结束时间。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_ended_at_unix_ms: Option<u64>,
    },
}

impl LegacyRunHistoricalOutcome {
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// 原实际执行结束时刻：只有带证据的历史结论才有；否则未知（`None`）。
    #[must_use]
    pub fn execution_ended_at_unix_ms(&self) -> Option<u64> {
        match self {
            Self::Unknown => None,
            Self::Evidenced {
                execution_ended_at_unix_ms,
                ..
            } => *execution_ended_at_unix_ms,
        }
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        match self {
            Self::Unknown => Ok(()),
            Self::Evidenced {
                evidence_refs,
                execution_ended_at_unix_ms,
                ..
            } => {
                if evidence_refs.is_empty() {
                    return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                        "有证据的历史结论必须给出证据引用：没有证据就不是结论",
                    ));
                }
                if execution_ended_at_unix_ms == &Some(0) {
                    return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                        "执行结束时刻为 0 不是真实时刻；未知必须缺省",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// 维度③：输入资源状态——是否还有旧执行者、未结清的释放义务或其他阻断事件。
///
/// **必须独立检查与恢复**：它**不随**维度①的终态自动清零。收敛把控制状态收成
/// `Interrupted` 之后，这里的 `safe_for_new_input` 仍可能是 false（资源仍隔离）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyRunInputResourceState {
    /// 是否可能仍有旧执行者在输入。`None` = 未知（**不得**读成"没有"）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_executor_may_be_present: Option<bool>,
    /// 本次独立检查到的**未结清释放义务**条数（例如 `input_release_status = 'unknown'`
    /// 且未被人工解除的 step）。它是观测值，不是由控制终态推出的 0。
    pub unconfirmed_release_obligations: usize,
    /// 其它阻断事件引用（共享资源事故 / 恢复待决阻断记录）。
    #[serde(default)]
    pub blocking_event_refs: Vec<String>,
    /// 本次独立检查是否判定"资源已可安全接纳新输入"。
    ///
    /// **只有它**为 true 才允许开放新输入（见 [`legacy_cu_run_convergence_reopen_is_allowed`]）；
    /// 控制终态为 `Interrupted` **不构成**它可以为 true 的理由。
    pub safe_for_new_input: bool,
}

impl LegacyRunInputResourceState {
    /// 自校验：未结清的释放义务存在时不得同时声明资源可安全接纳新输入。
    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.unconfirmed_release_obligations > 0 && self.safe_for_new_input {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(
                "仍有未结清释放义务时不得声明资源可安全接纳新输入",
            ));
        }
        if self.safe_for_new_input
            && self
                .blocking_event_refs
                .iter()
                .any(|reference| !reference.trim().is_empty())
            && self.old_executor_may_be_present.unwrap_or(true)
        {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(
                "存在阻断事件且旧执行者可能存在时不得声明资源可安全接纳新输入",
            ));
        }
        Ok(())
    }
}

/// 历史资源 scope 的记录状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "snake_case")]
pub enum LegacyResourceScope {
    /// 历史资源 scope **未记录**（本类事实处理的遗留行就是这种）。
    ///
    /// 此时**不得**把当前 Windows 会话 / 当前 scope 写成旧运行的真实 scope。
    Unrecorded,
    /// 有记录的历史资源 scope（值来自当时的记录本身）。
    Recorded { scope_id: String },
}

impl LegacyResourceScope {
    #[must_use]
    pub const fn is_unrecorded(&self) -> bool {
        matches!(self, Self::Unrecorded)
    }
}

/// 本次为"当前可能受影响的资源"建立的**新**安全检查记录的基准。
///
/// 类型上**只有一个变体**是有意的：这条记录**是**"当前可能受影响"，不是"历史真实 scope"。
/// 因此"把当前会话写成旧运行的历史 scope"在类型上不可表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "snake_case")]
pub enum CurrentResourceCandidateBasis {
    /// 当前可能受影响的资源（新建立的检查记录，不代表历史归属）。
    CurrentMayBeAffectedNotHistoricalScope,
}

/// 本次建立的新安全检查记录：以"当前可能受影响的资源"为基准。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentResourceSafetyCheck {
    /// 基准：恒为"当前可能受影响"，不是历史 scope。
    pub basis: CurrentResourceCandidateBasis,
    /// 本次检查覆盖的 session / turn（**当前**候选 scope，不是旧运行的真实 scope）。
    pub session_id: String,
    pub turn_id: String,
    /// 本次检查时刻。
    pub checked_at_unix_ms: u64,
}

impl CurrentResourceSafetyCheck {
    pub fn validate(&self) -> Result<(), RunContractError> {
        for (field, value) in [
            ("session_id", &self.session_id),
            ("turn_id", &self.turn_id),
        ] {
            if value.trim().is_empty() {
                return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                    format!("{field} 是当前候选 scope 的组成部分，必须给出"),
                ));
            }
        }
        if self.checked_at_unix_ms == 0 {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "安全检查时刻不能是 0",
            ));
        }
        Ok(())
    }
}

/// 副作用限制：本次收敛**不得**做什么。
///
/// 三个字段都只有一种合法取值（false），写入侧无法声明"允许复活 / 允许重放 / 允许推断成功"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunSideEffectLimits {
    /// 是否恢复旧任务 / 以"失败恢复"为由自动重派旧副作用任务。**恒为 false**。
    pub revives_old_task: bool,
    /// 是否重放原 action。**恒为 false**。
    pub replays_actions: bool,
    /// 是否据此推断目标成功（含成功记忆提取）。**恒为 false**。
    pub infers_goal_success: bool,
}

impl LegacyCuRunSideEffectLimits {
    /// 本次收敛允许的唯一副作用限制：什么都不做。
    #[must_use]
    pub const fn none() -> Self {
        Self {
            revives_old_task: false,
            replays_actions: false,
            infers_goal_success: false,
        }
    }

    /// 本次收敛是否让旧 run 重新获得执行资格。按契约**恒为 false**。
    #[must_use]
    pub const fn revives_run(self) -> bool {
        false
    }

    /// 是否据此宣称目标完成。按契约**恒为 false**。
    #[must_use]
    pub const fn claims_goal_completion(self) -> bool {
        false
    }

    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.revives_old_task || self.replays_actions || self.infers_goal_success {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(
                "收敛的副作用限制只有一种合法取值：不恢复旧任务、不重放动作、不推断目标成功",
            ));
        }
        Ok(())
    }
}

impl Default for LegacyCuRunSideEffectLimits {
    fn default() -> Self {
        Self::none()
    }
}

/// 一条**遗留运行收敛事实**（窄范围；复用既有事实存储的追加与幂等口径）。
///
/// 它承载裁决 A-1.5 列出的七组内容：被处理对象、原始信息、缺失维度、本次决定、依据、
/// 本次执行者、副作用限制；三个维度（控制 / 历史结果 / 输入资源）另作独立字段。
///
/// 它**不是** `RunIdentity`：没有 workspace、room、turn 这些容器维度，也不进任何身份准入。
/// 因此"缺 workspace 的历史行"不需要（也不允许）被塞进正常身份——那会逼出哨兵值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunConvergenceFact {
    /// 被处理对象。
    pub subject: LegacyCuRunSubject,
    /// 原始信息。
    pub original: LegacyCuRunOriginalFacts,
    /// 缺失维度。
    pub missing: LegacyCuRunMissingDimensions,
    /// 本次决定。
    pub decision: LegacyCuRunConvergenceDecision,
    /// 依据。
    pub evidence: LegacyCuRunConvergenceEvidence,
    /// 本次执行者。
    pub operator: LegacyCuRunRecoveryOperator,
    /// 维度①：运行控制状态。
    pub control: LegacyRunControlState,
    /// 维度②：历史执行结果。
    pub historical_outcome: LegacyRunHistoricalOutcome,
    /// 维度③：输入资源状态。
    pub input_resource: LegacyRunInputResourceState,
    /// 历史资源 scope 的记录状态（遗留行是 `Unrecorded`）。
    pub historical_resource_scope: LegacyResourceScope,
    /// 本次建立的新安全检查记录（以"当前可能受影响"为基准）。
    pub current_resource_safety_check: CurrentResourceSafetyCheck,
    /// 副作用限制。
    pub side_effects: LegacyCuRunSideEffectLimits,
    /// **本次**对账时刻（实际写入时刻）。它与"原执行结束时刻"是两件事。
    pub reconciled_at_unix_ms: u64,
}

impl LegacyCuRunConvergenceFact {
    /// 事务/崩溃对账键：**来源对象 + 原状态·revision**（不是时间，也不是随机 ID）。
    #[must_use]
    pub fn reconciliation_key(&self) -> LegacyCuRunConvergenceKey {
        LegacyCuRunConvergenceKey {
            source_database_identity: self.subject.source_database_identity.clone(),
            original_run_id: self.subject.original_run_id.clone(),
            original_state: self.original.original_state.clone(),
            original_state_version: self.original.original_state_version,
        }
    }

    /// 两条事实是否是**同一次收敛**（除"本次对账时刻 / 本次执行者 / 本次操作 ID"之外
    /// 的全部内容相同）。
    ///
    /// 这三个字段被排除，是因为重复启动、双实例或响应丢失后重发时它们**必然**可能不同：
    /// 把它们算进内容比较，就会把"同一次收敛的重复请求"误判成冲突，从而写出第二份事实。
    /// 反过来，任何**结论性或观测性**内容的差异都算不同（不得静默覆盖）。
    #[must_use]
    pub fn same_convergence_as(&self, other: &Self) -> bool {
        self.subject == other.subject
            && self.original == other.original
            && self.missing == other.missing
            && self.decision.terminal_status == other.decision.terminal_status
            && self.decision.reason_code == other.decision.reason_code
            && self.decision.explanation == other.decision.explanation
            && self.decision.rule_version == other.decision.rule_version
            && self.evidence == other.evidence
            && self.control == other.control
            && self.historical_outcome == other.historical_outcome
            && self.input_resource == other.input_resource
            && self.historical_resource_scope == other.historical_resource_scope
            && self.current_resource_safety_check == other.current_resource_safety_check
            && self.side_effects == other.side_effects
    }

    /// 原实际执行结束时刻：**只能**来自带证据的历史结论。
    #[must_use]
    pub fn original_execution_ended_at_unix_ms(&self) -> Option<u64> {
        self.historical_outcome.execution_ended_at_unix_ms()
    }

    /// 自校验：任何一条不成立都不得写入（宁可拒绝，不要写一条说不清的收敛事实）。
    pub fn validate(&self) -> Result<(), RunContractError> {
        self.subject.validate()?;
        self.original.validate()?;
        self.missing.validate()?;
        self.decision.validate()?;
        self.operator.validate()?;
        self.control.validate()?;
        self.historical_outcome.validate()?;
        self.input_resource.validate()?;
        self.current_resource_safety_check.validate()?;
        self.side_effects.validate()?;
        if self.reconciled_at_unix_ms == 0 {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "reconciled_at 必须是本次实际时刻（0 不是时刻）",
            ));
        }
        // 本次对账时刻不得被当成"原执行结束时刻"：只有带证据的历史结论才允许给出结束时刻，
        // 而带证据时证据引用必须非空（`LegacyRunHistoricalOutcome::validate` 已强制）。
        if self.operator.operated_at_unix_ms > self.reconciled_at_unix_ms {
            return Err(RunContractError::invalid_legacy_cu_run_convergence_decision(
                "操作时刻不得晚于对账时刻",
            ));
        }
        if matches!(&self.historical_resource_scope, LegacyResourceScope::Recorded { scope_id } if scope_id.trim().is_empty())
        {
            return Err(RunContractError::incomplete_legacy_cu_run_convergence(
                "历史资源 scope 若记录则必须是真实值；否则保持未记录",
            ));
        }
        Ok(())
    }

    /// 本次收敛是否允许开放新输入。
    ///
    /// **只**看维度③的独立检查结果（并且要求历史资源 scope 的诚实标注已经给出）。
    /// 控制终态是 `Interrupted` 不构成理由——这正是"三维度互不推导"在接口上的落点。
    #[must_use]
    pub fn reopen_new_input_is_allowed(&self) -> bool {
        legacy_cu_run_convergence_reopen_is_allowed(&self.control, &self.input_resource)
    }
}

/// 是否允许在收敛之后开放新输入。
///
/// 只看资源维度的独立检查结果；控制维度只作为**附加**条件参与（它必须已经不再允许继续），
/// 绝不由它推出"资源安全"。
#[must_use]
pub fn legacy_cu_run_convergence_reopen_is_allowed(
    control: &LegacyRunControlState,
    resource: &LegacyRunInputResourceState,
) -> bool {
    !control.continuation_allowed
        && resource.safe_for_new_input
        && resource.unconfirmed_release_obligations == 0
}

/// 对账键：同一来源对象 + 原状态·revision。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunConvergenceKey {
    pub source_database_identity: String,
    pub original_run_id: String,
    pub original_state: String,
    pub original_state_version: u64,
}

impl LegacyCuRunConvergenceKey {
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

/// 收敛**意图**（A-1.6：会话库与输入安全库不是同一事务 ⇒ 必须有可重入的意图 + 幂等关联）。
///
/// 意图在"改旧 run / 写收敛事实"**之前**落盘，用于崩溃或响应丢失后的对账：
/// 意图存在而事实缺失 = 崩溃在上一步，本次可以继续（可重入）；
/// 意图与事实一致 = 同一次收敛，**不再写第二份**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCuRunConvergenceIntent {
    pub recovery_operation_id: String,
    pub source_database_identity: String,
    pub original_run_id: String,
    pub original_state: String,
    pub original_state_version: u64,
    /// 意图落盘时刻。
    pub recorded_at_unix_ms: u64,
    /// 意图是否已与收敛事实对账完成。
    pub reconciled: bool,
}

impl LegacyCuRunConvergenceIntent {
    /// 对账键：与收敛事实使用**同一个**键（来源对象 + 原状态·revision）。
    #[must_use]
    pub fn reconciliation_key(&self) -> LegacyCuRunConvergenceKey {
        LegacyCuRunConvergenceKey {
            source_database_identity: self.source_database_identity.clone(),
            original_run_id: self.original_run_id.clone(),
            original_state: self.original_state.clone(),
            original_state_version: self.original_state_version,
        }
    }

    /// 意图是否与给定事实指向同一次收敛。
    #[must_use]
    pub fn matches_fact(&self, fact: &LegacyCuRunConvergenceFact) -> bool {
        self.reconciliation_key() == fact.reconciliation_key()
    }
}

/// A-1.6 要求的安全步骤顺序（契约层可表达）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyCuRunConvergenceStep {
    /// 1. 暂停相关新输入接纳、取得恢复协调权。
    PauseNewInputIntakeAndTakeRecoveryAuthority,
    /// 2. 记录**可重入的**收敛意图。
    RecordReentrantConvergenceIntent,
    /// 3. 必要时**先**建立共享资源事故 / 恢复待决阻断。
    EstablishSharedResourceIncidentOrRecoveryPendingBlock,
    /// 4. 在**来源数据库同一事务**写入"旧 run 的非成功终态 + 本次收敛事实"。
    WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
    /// 5. 完成意图对账。
    ReconcileConvergenceIntent,
    /// 6. **仅在独立安全条件满足后**开放新输入。
    ReopenNewInputOnlyAfterIndependentSafetyConditions,
}

impl LegacyCuRunConvergenceStep {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PauseNewInputIntakeAndTakeRecoveryAuthority => {
                "pause_new_input_intake_and_take_recovery_authority"
            }
            Self::RecordReentrantConvergenceIntent => "record_reentrant_convergence_intent",
            Self::EstablishSharedResourceIncidentOrRecoveryPendingBlock => {
                "establish_shared_resource_incident_or_recovery_pending_block"
            }
            Self::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction => {
                "write_run_non_success_terminal_and_convergence_fact_in_source_transaction"
            }
            Self::ReconcileConvergenceIntent => "reconcile_convergence_intent",
            Self::ReopenNewInputOnlyAfterIndependentSafetyConditions => {
                "reopen_new_input_only_after_independent_safety_conditions"
            }
        }
    }

    /// 这一步是否属于**危险**顺序（见 [`FORBIDDEN_LEGACY_CU_RUN_CONVERGENCE_ORDER`]）。
    #[must_use]
    pub const fn is_dangerous(self) -> bool {
        false
    }
}

/// A-1.6 要求的步骤顺序（唯一允许的实现顺序）。
pub const LEGACY_CU_RUN_CONVERGENCE_STEPS: [LegacyCuRunConvergenceStep; 6] = [
    LegacyCuRunConvergenceStep::PauseNewInputIntakeAndTakeRecoveryAuthority,
    LegacyCuRunConvergenceStep::RecordReentrantConvergenceIntent,
    LegacyCuRunConvergenceStep::EstablishSharedResourceIncidentOrRecoveryPendingBlock,
    LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
    LegacyCuRunConvergenceStep::ReconcileConvergenceIntent,
    LegacyCuRunConvergenceStep::ReopenNewInputOnlyAfterIndependentSafetyConditions,
];

/// A-1.6 **明文禁止**的顺序（逐条取自裁决）：
/// 先把旧 run 改终态 → 旧 SQL 不再命中 → 尚未建立资源阻断 → 新动作开始输入。
pub const FORBIDDEN_LEGACY_CU_RUN_CONVERGENCE_ORDER: [&str; 4] = [
    "write_run_terminal_first",
    "old_sql_stops_matching",
    "resource_block_not_yet_established",
    "new_actions_start_inputing",
];

/// 给定一串步骤，判断它是否是**安全**的收敛顺序。
///
/// 安全 = 恰好是 [`LEGACY_CU_RUN_CONVERGENCE_STEPS`] 的一个前缀（按序、不重排），
/// 或者等于完整顺序。任何把"写终态"提前到"建立资源阻断"之前、或把"开放新输入"
/// 提前到达成之前的顺序都不是安全顺序。
#[must_use]
pub fn legacy_cu_run_convergence_order_is_safe(steps: &[LegacyCuRunConvergenceStep]) -> bool {
    if steps.len() > LEGACY_CU_RUN_CONVERGENCE_STEPS.len() {
        return false;
    }
    steps
        .iter()
        .zip(LEGACY_CU_RUN_CONVERGENCE_STEPS.iter())
        .all(|(actual, expected)| actual == expected)
}

/// 完整顺序的"写终态"步之前是否已经包含资源阻断步。危险顺序的机器可判定形式。
#[must_use]
pub fn legacy_cu_run_convergence_block_precedes_terminal_write(
    steps: &[LegacyCuRunConvergenceStep],
) -> bool {
    let block = steps.iter().position(|step| {
        *step == LegacyCuRunConvergenceStep::EstablishSharedResourceIncidentOrRecoveryPendingBlock
    });
    let terminal = steps.iter().position(|step| {
        *step == LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction
    });
    match (block, terminal) {
        (Some(block), Some(terminal)) => block < terminal,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{ReportedUsage, UsageAttempt, UsageAttemptOutcome};

    fn receipt(input_delivery: InputDelivery) -> ActionReceipt {
        ActionReceipt {
            action_id: "action-1".to_string(),
            input_delivery,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::NotNeeded,
        }
    }

    fn context() -> RunScopeContext {
        RunScopeContext::new("workspace-1", "room-1", "session-1", "turn-public-1")
    }

    fn turn_identity() -> RunIdentity {
        context().turn_fact("run-1")
    }

    fn action_identity() -> RunIdentity {
        context()
            .step_action_fact("run-1", "step-1", "attempt-1", "action-1")
            .with_tool_call_id("tool-call-1")
            .with_owner_epoch(InputOwnerEpoch::from_host_counter(7).expect("epoch"))
    }

    /// 已经发出输入的回执（含真实的输入所有权 epoch，可用作"完整的动作事实"）。
    fn sent_receipt(action_id: &str) -> ActionReceipt {
        ActionReceipt {
            action_id: action_id.to_string(),
            input_delivery: InputDelivery::Sent,
            partial: Some(false),
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::NotNeeded,
        }
    }

    #[test]
    fn receipt_preserves_known_partial_input_without_claiming_completion() {
        let mut value = receipt(InputDelivery::Sent);
        value.partial = Some(true);
        value.path_completed = Some(false);
        value.confirmed_point_count = Some(0);
        value.input_release = InputReleaseStatus::Unknown;
        assert!(value.validate().is_ok());
    }

    #[test]
    fn receipt_rejects_impossible_input_combinations() {
        let mut not_sent = receipt(InputDelivery::NotSent);
        not_sent.partial = Some(true);
        assert_eq!(
            not_sent.validate().expect_err("must reject").code,
            "contradictory_action_receipt"
        );

        let mut uncertain = receipt(InputDelivery::MayHaveBeenSent);
        uncertain.path_completed = Some(true);
        assert!(uncertain.validate().is_err());
    }

    #[test]
    fn identity_requires_all_scopes_and_attempt_identifiers() {
        let identity = action_identity();
        assert!(identity.validate().is_ok());
        let mut missing = identity.clone();
        missing.owner_epoch = None;
        assert_eq!(
            missing.validate().expect_err("must reject").code,
            "incomplete_identity"
        );

        // 空串仍按旧语义是 `invalid_identity`（旧严格语义没有被放松）。
        let mut empty = identity;
        empty.step_id = Some(String::new());
        assert_eq!(
            empty.validate().expect_err("must reject").code,
            "invalid_identity"
        );
    }

    #[test]
    fn terminal_control_never_rejects_late_facts_and_budget_has_single_deadline() {
        assert!(RunTerminalStatus::Cancelled.accepts_late_facts());
        assert!(RunTerminalStatus::Interrupted.accepts_late_facts());
        let budget = RunBudget {
            deadline_unix_ms: 100,
            max_actions: 12,
            max_replans: 2,
            max_request_attempts: 3,
        };
        assert!(!budget.is_expired_at(99));
        assert!(budget.is_expired_at(100));
    }

    /// turn 级事实的动作维度必须缺省，且缺省后仍能通过校验（可写入）。
    #[test]
    fn turn_fact_defaults_every_action_dimension() {
        let identity = turn_identity();
        identity
            .validate_for(RunIdentityScope::Turn)
            .expect("turn 事实的动作维度缺省即可写入");
        assert_eq!(identity.step_id(), None);
        assert_eq!(identity.action_id(), None);
        assert_eq!(identity.owner_epoch(), None);

        // 序列化后不适用维度必须**省略**（不是写空串，也不是写 null）。
        let json = serde_json::to_value(&identity).expect("encode turn identity");
        for absent in [
            "step_id",
            "request_attempt_id",
            "tool_call_id",
            "action_id",
            "owner_epoch",
        ] {
            assert!(
                json.get(absent).is_none(),
                "不适用的 {absent} 必须被省略：{json}"
            );
        }
        assert_eq!(json.get("scope"), Some(&serde_json::json!("turn")));
        assert_eq!(
            json.get("schema_version"),
            Some(&serde_json::json!(RUN_IDENTITY_SCHEMA_VERSION))
        );

        // 反向：turn 事实不得携带动作维度（降级绕校验的可见信号）。
        let mut downgraded = turn_identity();
        downgraded.action_id = Some("action-1".to_string());
        assert_eq!(
            downgraded
                .validate_for(RunIdentityScope::Turn)
                .expect_err("must reject")
                .code,
            "identity_dimension_not_applicable"
        );
    }

    /// 本应必填却缺失 → 身份不完整错误（不是"不适用"）。
    #[test]
    fn step_action_fact_reports_incomplete_identity_instead_of_not_applicable() {
        let mut missing_action = context()
            .step_action_fact("run-1", "step-1", "attempt-1", "action-1")
            .with_tool_call_id("tool-call-1");
        missing_action.action_id = None;
        let error = missing_action
            .validate_for(RunIdentityScope::StepAction)
            .expect_err("must reject");
        assert_eq!(error.code, "incomplete_identity");
        assert!(error.message.contains("action_id"), "{}", error.message);
        assert_eq!(
            missing_action.missing_required_dimensions(RunIdentityScope::StepAction),
            vec![IdentityDimension::ActionId]
        );

        // 缺 scope / 缺版本 / scope 与写入路径不符，全部拒绝。
        let mut undeclared = turn_identity();
        undeclared.scope = None;
        assert_eq!(
            undeclared
                .validate_for(RunIdentityScope::Turn)
                .expect_err("must reject")
                .code,
            "identity_scope_undeclared"
        );

        let mut versionless = turn_identity();
        versionless.schema_version = None;
        assert_eq!(
            versionless
                .validate_for(RunIdentityScope::Turn)
                .expect_err("must reject")
                .code,
            "identity_schema_version_missing"
        );

        let mut future = turn_identity();
        future.schema_version = Some(RUN_IDENTITY_SCHEMA_VERSION + 1);
        assert_eq!(
            future
                .validate_for(RunIdentityScope::Turn)
                .expect_err("must reject")
                .code,
            "unsupported_identity_schema_version"
        );

        let action = action_identity();
        assert_eq!(
            action
                .validate_for(RunIdentityScope::Turn)
                .expect_err("动作事实不得按 turn scope 校验")
                .code,
            "identity_scope_mismatch"
        );
    }

    /// 哨兵值（空串 / `unknown` / `n/a` / `0` / 抄 `turn_id`）一律拒绝。
    #[test]
    fn placeholder_identity_values_are_rejected() {
        for placeholder in ["", "  ", "unknown", "n/a", "N/A", "null", "-", "0"] {
            let mut identity = action_identity();
            identity.step_id = Some(placeholder.to_string());
            let code = identity
                .validate_for(RunIdentityScope::StepAction)
                .expect_err("must reject")
                .code;
            let expected = if placeholder.trim().is_empty() {
                "invalid_identity"
            } else {
                "placeholder_identity_value"
            };
            assert_eq!(code, expected, "占位值 {placeholder:?} 必须被拒绝");
        }

        // 抄公开轮次 ID / run_id 到动作维度或 epoch 上同样是凑字段。
        let mut copied_turn = action_identity();
        copied_turn.action_id = Some("turn-public-1".to_string());
        assert_eq!(
            copied_turn
                .validate_for(RunIdentityScope::StepAction)
                .expect_err("must reject")
                .code,
            "placeholder_identity_value"
        );

        let mut copied_run = action_identity();
        copied_run.owner_epoch = Some(InputOwnerEpoch::parse("run-1").expect("parse"));
        assert_eq!(
            copied_run
                .validate_for(RunIdentityScope::StepAction)
                .expect_err("must reject")
                .code,
            "placeholder_identity_value"
        );

        // 输入所有权 epoch 由宿主计数器给出；0 表示"没有所有权"，必须缺省。
        assert_eq!(
            InputOwnerEpoch::from_host_counter(0)
                .expect_err("counter 0 must be rejected")
                .code,
            "placeholder_identity_value"
        );
        assert_eq!(
            InputOwnerEpoch::from_host_counter(7).expect("counter 7").as_str(),
            "7"
        );
    }

    /// 旧记录（缺 scope）按旧版严格规则解释，**不得**默认视为 Turn。
    #[test]
    fn legacy_records_without_scope_are_read_by_the_old_strict_rules() {
        // 只填 5 个容器维度的旧记录：旧严格规则要求十维 → 必须报身份不完整，
        // 而不是因为"看起来像 turn 事实"就放行。
        let mut legacy = turn_identity();
        legacy.scope = None;
        legacy.schema_version = None;
        assert_eq!(
            legacy.resolved_scope(),
            EffectiveRunIdentityScope::LegacyUnscoped
        );
        assert_eq!(
            legacy
                .validate_persisted()
                .expect_err("旧记录缺维度必须被拒")
                .code,
            "incomplete_identity"
        );

        // 十维齐全的旧记录（旧 JSON）按旧严格规则通过。
        let legacy_full = RunIdentity {
            workspace_id: "workspace-1".to_string(),
            room_id: "room-1".to_string(),
            session_id: "session-1".to_string(),
            public_turn_id: "turn-public-1".to_string(),
            run_id: "run-1".to_string(),
            step_id: Some("step-1".to_string()),
            request_attempt_id: Some("attempt-1".to_string()),
            tool_call_id: Some("tool-call-1".to_string()),
            action_id: Some("action-1".to_string()),
            owner_epoch: Some(InputOwnerEpoch::from_host_counter(3).expect("epoch")),
            parent_run: None,
            scope: None,
            schema_version: None,
        };
        legacy_full
            .validate_persisted()
            .expect("旧记录按旧严格规则通过");
        // 旧记录即使十维齐全，也不会被当成 Turn scope（否则动作维度就成了"不适用"）。
        legacy_full
            .validate_for(RunIdentityScope::Turn)
            .expect_err("旧记录不得被默认视为 Turn");
    }

    /// 外来房间 / 工作区 / 会话 / 轮次的关联必须被拒绝。
    #[test]
    fn scope_context_rejects_foreign_room_or_workspace() {
        let identity = action_identity();
        context()
            .validate_identity(&identity)
            .expect("同房间同工作区");
        let mut foreign_room = identity.clone();
        foreign_room.room_id = "room-2".to_string();
        let error = context()
            .validate_identity(&foreign_room)
            .expect_err("必须拒绝外来房间");
        assert_eq!(error.code, "identity_context_mismatch");
        assert!(!context().matches_identity(&foreign_room));

        let mut foreign_workspace = identity;
        foreign_workspace.workspace_id = "workspace-2".to_string();
        assert_eq!(
            context()
                .validate_identity(&foreign_workspace)
                .expect_err("必须拒绝外来工作区")
                .code,
            "identity_context_mismatch"
        );
    }

    /// 输入前拒绝：没有 epoch 也能记录该拒绝事实，但**不获得输入资格**。
    #[test]
    fn pre_input_rejection_records_without_gaining_input_qualification() {
        let mut identity = action_identity();
        identity.owner_epoch = None;
        identity.action_id = None; // 拒绝发生在"哪个动作"确定之前：明确缺省
        identity.step_id = None;
        identity.request_attempt_id = None;
        identity.tool_call_id = None;
        let rejection = ActionReceipt {
            action_id: "run-scope:call-1".to_string(),
            input_delivery: InputDelivery::NotSent,
            partial: Some(false),
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::NotNeeded,
        };

        let admission = admit_action_fact(&identity, &rejection).expect("admission");
        assert!(!admission.gains_input_qualification());
        assert!(!admission.must_stop_input());
        assert!(admission.anomaly().is_none());
        assert_eq!(
            admission.absent_dimensions(),
            &[
                IdentityDimension::StepId,
                IdentityDimension::RequestAttemptId,
                IdentityDimension::ToolCallId,
                IdentityDimension::ActionId
            ]
        );

        // 但"已提供字段有错值"不能靠缺省掩盖。
        let mut broken = identity.clone();
        broken.request_attempt_id = Some("n/a".to_string());
        assert_eq!(
            admit_action_fact(&broken, &rejection)
                .expect_err("占位值必须被拒")
                .code,
            "placeholder_identity_value"
        );
    }

    /// 已经开始输入却缺身份：事实必须保留（带异常），且必须停止后续输入。
    #[test]
    fn started_input_with_incomplete_identity_is_preserved_with_an_anomaly() {
        // 缺 owner_epoch（但其余维度齐全）。
        let identity = context()
            .step_action_fact("run-1", "step-1", "attempt-1", "action-1")
            .with_tool_call_id("tool-call-1");
        let admission = admit_action_fact(&identity, &sent_receipt("action-1")).expect("admission");
        let anomaly = admission.anomaly().expect("必须给出身份异常").clone();
        assert!(admission.must_stop_input());
        assert!(!admission.gains_input_qualification());
        assert_eq!(anomaly.code(), "incomplete_identity");
        assert_eq!(anomaly.missing_dimensions, vec!["owner_epoch".to_string()]);
        assert!(anomaly.input_may_have_started);
        assert!(anomaly.must_stop_input);
        anomaly.validate().expect("异常自身必须自洽");

        // 缺 action_id（其余齐全）：本应存在却缺失 → 同样是异常保留，不是"不适用"。
        let mut missing_action = action_identity();
        missing_action.action_id = None;
        let admission =
            admit_action_fact(&missing_action, &sent_receipt("action-1")).expect("admission");
        assert_eq!(
            admission.anomaly().expect("异常").missing_dimensions,
            vec!["action_id".to_string()]
        );

        // 完整身份 + 真实 epoch：正常准入，并取得输入资格。
        let complete = action_identity();
        let admission = admit_action_fact(&complete, &sent_receipt("action-1")).expect("admission");
        assert!(admission.gains_input_qualification());
        assert!(!admission.must_stop_input());

        // 身份里的 action_id 与回执不一致：直接拒绝。
        let mismatched = action_identity();
        assert_eq!(
            admit_action_fact(&mismatched, &sent_receipt("other-action"))
                .expect_err("不一致必须拒绝")
                .code,
            "identity_conflict"
        );
    }

    /// 非终态不写终态事实；Completed 只表示本 scope 正常结束。
    #[test]
    fn host_outcome_maps_terminal_statuses_without_overclaiming() {
        assert_eq!(
            HostRunOutcome::Completed.terminal_status(),
            Some(RunTerminalStatus::Succeeded)
        );
        assert!(!RunTerminalStatus::Succeeded.claims_goal_completion());
        assert!(RunTerminalStatus::Succeeded.is_scope_success());

        assert_eq!(
            HostRunOutcome::Failed {
                reason: "provider 5xx".to_string(),
                error: None,
            }
            .terminal_status(),
            Some(RunTerminalStatus::Failed)
        );
        // 失败必须带真实原因。
        assert_eq!(
            HostRunOutcome::Failed {
                reason: "  ".to_string(),
                error: None,
            }
            .validate()
            .expect_err("空原因必须被拒")
            .code,
            "incomplete_host_outcome"
        );

        assert_eq!(HostRunOutcome::Running.terminal_status(), None);
        assert_eq!(HostRunOutcome::CancelRequested.terminal_status(), None);
        assert!(!HostRunOutcome::CancelRequested.is_terminal());
        HostRunOutcome::Completed.validate().expect("completed 自洽");
    }

    /// 异常中断不得冒充取消；只有宿主确认（且写明来源）才映射成 Cancelled。
    #[test]
    fn interruption_is_not_cancellation_without_host_confirmation() {
        let unattributed = HostRunOutcome::interrupted(Some("runner exited without terminal fact".to_string()));
        assert_eq!(
            unattributed.terminal_status(),
            Some(RunTerminalStatus::Interrupted)
        );
        assert_eq!(unattributed.cancel_origin(), None);
        assert!(unattributed.validate().is_ok());

        // 取消来源不明的空证据不写"证据"（要么给出真实证据，要么什么都不写）。
        assert_eq!(
            HostRunOutcome::interrupted(Some("unknown".to_string()))
                .validate()
                .expect_err("占位证据必须被拒")
                .code,
            "incomplete_host_outcome"
        );

        let confirmed = HostRunOutcome::cancelled(CancelOrigin::User);
        assert_eq!(
            confirmed.terminal_status(),
            Some(RunTerminalStatus::Cancelled)
        );
        assert!(confirmed.cancel_origin().expect("origin").is_user_initiated());

        // 归属不明的"某人取消"不能默认写成"用户取消"。
        let operator = HostRunOutcome::cancelled(CancelOrigin::Operator {
            operator_id: String::new(),
        });
        assert_eq!(
            operator.validate().expect_err("缺 operator_id 必须被拒").code,
            "incomplete_host_outcome"
        );
        assert_eq!(
            HostRunOutcome::cancelled(CancelOrigin::Other {
                code: "n/a".to_string()
            })
            .validate()
            .expect_err("占位原因码必须被拒")
            .code,
            "incomplete_host_outcome"
        );
        assert!(!CancelOrigin::HostShutdown {
            reason: "host process exited".to_string()
        }
        .is_user_initiated());
    }

    /// 不认识的终态**不得**被读成成功（也不得读成任何默认值）。
    #[test]
    fn unknown_terminal_variants_are_never_read_as_success() {
        for known in [
            "succeeded",
            "failed",
            "blocked",
            "cancelled",
            "timed_out",
            "interrupted",
        ] {
            assert_eq!(
                RunTerminalStatus::parse(known).map(RunTerminalStatus::as_str),
                Some(known)
            );
        }
        assert_eq!(RunTerminalStatus::parse("aborted"), None);
        assert!(serde_json::from_str::<RunTerminalStatus>("\"aborted\"").is_err());
        assert!(serde_json::from_str::<RunTerminalStatus>("\"succeeded\"").is_ok());

        // 序列化出的新终态必须能被同一版本读回（序列化与反序列化同步更新）。
        for status in [
            RunTerminalStatus::Succeeded,
            RunTerminalStatus::Failed,
            RunTerminalStatus::Blocked,
            RunTerminalStatus::Cancelled,
            RunTerminalStatus::TimedOut,
            RunTerminalStatus::Interrupted,
        ] {
            let encoded = serde_json::to_string(&status).expect("encode");
            assert_eq!(
                serde_json::from_str::<RunTerminalStatus>(&encoded).expect("decode"),
                status
            );
        }
        assert!(!RunTerminalStatus::Interrupted.is_scope_success());
        assert!(!RunTerminalStatus::TimedOut.is_scope_success());
    }

    /// `partial` 三态：缺键是未知；`false` 不等于完整执行。
    #[test]
    fn partial_unknown_is_distinct_from_not_partial_and_from_completion() {
        let legacy_json = serde_json::json!({
            "action_id": "action-1",
            "input_delivery": "sent",
            "effect": "not_observed",
            "goal_verdict": "not_checked",
            "input_release": "not_needed"
        });
        let legacy: ActionReceipt =
            serde_json::from_value(legacy_json).expect("旧 JSON 必须仍可读取");
        assert_eq!(legacy.partial, None, "缺键必须读成未知，不能默认 false");
        assert_eq!(legacy.partial_observation(), PartialObservation::Unknown);
        assert_eq!(legacy.path_completed, None);
        assert_eq!(legacy.confirmed_point_count, None);

        let mut not_partial = legacy.clone();
        not_partial.partial = Some(false);
        assert_eq!(
            not_partial.partial_observation(),
            PartialObservation::NotPartial
        );
        assert!(
            !not_partial.path_proven_complete(),
            "partial=false 不构成完整执行的证据（必须由 path_completed 承载）"
        );

        let mut proven = not_partial.clone();
        proven.path_completed = Some(true);
        assert!(proven.path_proven_complete());

        // 未知一律按"输入可能已经开始"处理（保守）。
        assert!(legacy.may_have_started_input());
        assert!(!ActionReceipt {
            partial: None,
            ..receipt(InputDelivery::NotSent)
        }
        .may_have_started_input());
    }

    /// 三个凭据域不得互相代用：类型不同、无跨域转换、claim token 不进身份对象。
    #[test]
    fn credential_domains_are_not_interchangeable() {
        let identity = action_identity();
        let json = serde_json::to_value(&identity).expect("encode identity");
        assert!(
            json.get("claim_token").is_none() && json.get("session_writer_epoch").is_none(),
            "claim token 与会话 writer epoch 不得进入公开身份对象：{json}"
        );
        assert_eq!(
            json.get("owner_epoch"),
            Some(&serde_json::json!("7")),
            "只有输入所有权 epoch 才是身份维度"
        );

        // 域内校验：空 / 占位一律拒绝（想混用就得显式写出来）。
        assert!(RunClaimToken::parse("  ").is_err());
        assert_eq!(
            RunClaimToken::parse("claim-1").expect("parse").as_str(),
            "claim-1"
        );
        assert_eq!(
            SessionWriterEpoch::parse("unknown")
                .expect_err("占位值必须被拒")
                .code,
            "invalid_identity"
        );
    }

    /// 父运行关联：不同实体必须显式关联，不得同名合并。
    #[test]
    fn parent_run_link_must_point_to_a_different_run() {
        let child = action_identity()
            .with_parent_run(ParentRunLink::new("run-turn-1", RunParentRelation::DrivenByTurnRun));
        child
            .validate_for(RunIdentityScope::StepAction)
            .expect("显式父关联的 CU run 身份有效");

        let merged = action_identity().with_parent_run(ParentRunLink::new(
            "run-1",
            RunParentRelation::DrivenByTurnRun,
        ));
        assert_eq!(
            merged
                .validate_for(RunIdentityScope::StepAction)
                .expect_err("同名合并必须被拒")
                .code,
            "invalid_identity"
        );

        let placeholder = action_identity()
            .with_parent_run(ParentRunLink::new("n/a", RunParentRelation::StepOfRun));
        assert_eq!(
            placeholder
                .validate_for(RunIdentityScope::StepAction)
                .expect_err("占位父 id 必须被拒")
                .code,
            "placeholder_identity_value"
        );
    }

    /// scope 决定必填表：调换 scope 会同时改变"必填"与"不适用"两侧。
    #[test]
    fn scope_dimension_tables_are_single_sourced() {
        assert!(!RunIdentityScope::Turn.carries_action_dimensions());
        assert!(RunIdentityScope::StepAction.carries_action_dimensions());
        assert!(RunIdentityScope::Turn
            .not_applicable_dimensions()
            .contains(&IdentityDimension::ActionId));
        assert!(RunIdentityScope::StepAction
            .not_applicable_dimensions()
            .is_empty());
        assert!(RunIdentityScope::StepAction
            .required_dimensions()
            .contains(&IdentityDimension::ToolCallId));
        assert_eq!(RunIdentityScope::StepAction.schema_version(), RUN_IDENTITY_SCHEMA_VERSION);
    }

    // ------------------------------------------------------------------
    // P-01：动作来源（ActionSource）与执行上下文类型（ContextKind）
    // ------------------------------------------------------------------

    fn planned_attempt() -> PlannedRequestAttempt {
        PlannedRequestAttempt::new("run-1", "logical-1", "attempt-1").expect("attempt")
    }

    fn desktop_scope() -> ResourceScope {
        ResourceScope::new(ResourceScopeKind::DesktopSession, "desktop-session-1")
    }

    fn empty_origin(action_id: &str, source: ActionSource) -> ActionOrigin {
        ActionOrigin {
            action_id: action_id.to_string(),
            source,
            context: ActionContext::conversation(action_identity()).expect("会话上下文"),
            request_attempt_id: None,
            tool_call_id: None,
            parent_step_operation: None,
            host_algorithm_version: None,
            resource_scope: None,
            cleanup: None,
            user_direct: None,
            host_transform: None,
            additional_causal_refs: Vec::new(),
        }
    }

    /// 按来源表把四种来源的必填关联都填齐（`action-1` 是当前动作）。
    fn origin_for(source: ActionSource, action_id: &str) -> ActionOrigin {
        let mut origin = empty_origin(action_id, source);
        match source {
            ActionSource::ModelPlanned => {
                origin.request_attempt_id = Some(planned_attempt().stable_key());
            }
            ActionSource::HostIncidental => {
                origin.parent_step_operation = Some("host-op-1".to_string());
                origin.host_algorithm_version = Some("uia-diff-v3".to_string());
                origin.resource_scope = Some(desktop_scope());
            }
            ActionSource::SafetyCleanup => {
                origin.cleanup = Some(CleanupRelation {
                    incident_id: "incident-1".to_string(),
                    original_action_id: "action-0".to_string(),
                    recovery_eligible: true,
                });
            }
            ActionSource::UserDirect => {
                origin.user_direct = Some(UserDirectRelation {
                    control_operation_id: "control-op:workspace-1:room-1:1".to_string(),
                    operator: OperatorOrigin::HostUserInterface,
                    permission_decision_id: "perm-1".to_string(),
                });
                origin.resource_scope = Some(desktop_scope());
            }
        }
        origin
    }

    fn remove_relation(origin: &mut ActionOrigin, field: ActionOriginField) {
        match field {
            ActionOriginField::RequestAttemptId => origin.request_attempt_id = None,
            ActionOriginField::ToolCallId => origin.tool_call_id = None,
            ActionOriginField::ParentStepOperation => origin.parent_step_operation = None,
            ActionOriginField::HostAlgorithmVersion => origin.host_algorithm_version = None,
            ActionOriginField::ResourceScope => origin.resource_scope = None,
            ActionOriginField::CleanupRelation => origin.cleanup = None,
            ActionOriginField::UserDirectRelation => origin.user_direct = None,
        }
    }

    fn control_operation_request(action_id: &str, step_id: &str) -> ControlOperationRequest {
        ControlOperationRequest {
            workspace_id: "workspace-1".to_string(),
            room_id: "room-1".to_string(),
            run_id: "run-1".to_string(),
            step_id: step_id.to_string(),
            action_id: action_id.to_string(),
            operator: OperatorOrigin::HostUserInterface,
            permission_decision_id: "perm-1".to_string(),
            resource_scope: desktop_scope(),
        }
    }

    /// 可信上下文：会话容器 + 已登记的运行 + `action-1` 的真实规划请求。
    fn trusted_context() -> TrustedOriginContext {
        let mut authority = TrustedOriginContext::new().with_conversation_scope(context());
        authority
            .register_run(RunRelationRecord::new("run-1", "workspace-1", "room-1"))
            .expect("run");
        authority.register_plan_producer("action-1", planned_attempt());
        authority
    }

    /// 四种来源的字段矩阵：必填缺了就拒、不适用填了就拒。
    #[test]
    fn action_source_field_matrix_is_enforced_by_structure() {
        // 表格本身是单一来源，且与裁决的字段表一致。
        assert_eq!(
            ActionSource::ModelPlanned.required_relations().to_vec(),
            vec![ActionOriginField::RequestAttemptId]
        );
        assert_eq!(
            ActionSource::HostIncidental.required_relations().to_vec(),
            vec![
                ActionOriginField::ParentStepOperation,
                ActionOriginField::HostAlgorithmVersion,
                ActionOriginField::ResourceScope
            ]
        );
        assert_eq!(
            ActionSource::SafetyCleanup.required_relations().to_vec(),
            vec![ActionOriginField::CleanupRelation]
        );
        assert_eq!(
            ActionSource::UserDirect.required_relations().to_vec(),
            vec![
                ActionOriginField::UserDirectRelation,
                ActionOriginField::ResourceScope
            ]
        );
        // `tool_call_id` 不在任何必填表里：它是否必填由可信上下文决定（B-7）。
        for source in [
            ActionSource::ModelPlanned,
            ActionSource::HostIncidental,
            ActionSource::SafetyCleanup,
            ActionSource::UserDirect,
        ] {
            assert!(!source
                .required_relations()
                .contains(&ActionOriginField::ToolCallId));
            assert!(source.requires_model_request_identity()
                == (source == ActionSource::ModelPlanned));
        }

        for source in [
            ActionSource::ModelPlanned,
            ActionSource::HostIncidental,
            ActionSource::SafetyCleanup,
            ActionSource::UserDirect,
        ] {
            let origin = origin_for(source, "action-1");
            origin
                .validate_structure()
                .unwrap_or_else(|error| panic!("{} 的必填齐全时必须通过结构校验：{}", source.as_str(), error.message));

            for field in source.required_relations() {
                let mut broken = origin.clone();
                remove_relation(&mut broken, *field);
                assert_eq!(
                    broken
                        .validate_structure()
                        .expect_err("必填缺失必须被拒")
                        .code,
                    "incomplete_action_origin",
                    "{} 缺 {} 必须报身份不完整",
                    source.as_str(),
                    field.as_str()
                );
            }

            for field in source.not_applicable_relations() {
                let mut misplaced = origin.clone();
                match field {
                    ActionOriginField::RequestAttemptId => {
                        misplaced.request_attempt_id = Some(planned_attempt().stable_key());
                    }
                    ActionOriginField::ToolCallId => {
                        misplaced.tool_call_id = Some("tool-call-1".to_string());
                    }
                    other => panic!("{} 不该出现在不适用表里", other.as_str()),
                }
                assert_eq!(
                    misplaced
                        .validate_structure()
                        .expect_err("不适用维度被填必须被拒")
                        .code,
                    "action_origin_dimension_not_applicable",
                    "{} 不得携带 {}",
                    source.as_str(),
                    field.as_str()
                );
            }
        }
    }

    /// 判定示例 1：ModelPlanned 且 `request_attempt_id = None` ⇒ 身份不完整。
    #[test]
    fn model_planned_without_request_attempt_is_incomplete() {
        let authority = trusted_context();
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.request_attempt_id = None;
        let error = admit_action_origin(&origin, &authority).expect_err("缺规划请求身份必须被拒");
        assert_eq!(error.code, "incomplete_action_origin");
        assert!(!error.retryable);

        let origin = origin_for(ActionSource::ModelPlanned, "action-1");
        let admission = admit_action_origin(&origin, &authority).expect("完整身份必须被受理");
        assert!(admission.model_request_identity_verified());
        assert_eq!(
            admission
                .verified_model_request
                .as_ref()
                .expect("已核对的规划请求")
                .stable_key(),
            planned_attempt().stable_key()
        );
        assert_eq!(admission.source, ActionSource::ModelPlanned);
        assert_eq!(admission.context, ContextKind::Conversation);
    }

    /// 判定示例 2：HostIncidental 且无宿主父操作记录 ⇒ 来源不成立。
    #[test]
    fn host_incidental_without_a_real_parent_operation_is_not_established() {
        let mut authority = trusted_context();
        let origin = origin_for(ActionSource::HostIncidental, "action-1");
        // 只信传入的 `source = host_incidental` 字符串是不够的：宿主登记里没有 host-op-1。
        assert_eq!(
            admit_action_origin(&origin, &authority)
                .expect_err("没有宿主父操作记录就不能成立")
                .code,
            "action_source_not_established"
        );

        authority
            .register_host_operation(HostOperationRecord {
                operation_id: "host-op-1".to_string(),
                run_id: "run-1".to_string(),
                step_id: "step-1".to_string(),
                algorithm_version: "uia-diff-v3".to_string(),
                kind: HostOperationKind::Observation,
            })
            .expect("宿主辅助操作登记");
        admit_action_origin(&origin, &authority).expect("有真实父操作的宿主辅助动作成立");

        // 算法版本对不上（只贴了个"看起来像"的版本号）必须拒绝。
        let mut wrong_version = origin.clone();
        wrong_version.host_algorithm_version = Some("uia-diff-v2".to_string());
        assert_eq!(
            wrong_version
                .validate_against(&authority)
                .expect_err("宿主算法版本对不上必须拒绝")
                .code,
            "action_origin_conflict"
        );

        // 父操作属于别的运行也必须拒绝。
        let mut wrong_run = origin.clone();
        authority
            .register_host_operation(HostOperationRecord {
                operation_id: "host-op-2".to_string(),
                run_id: "run-2".to_string(),
                step_id: "step-1".to_string(),
                algorithm_version: "uia-diff-v3".to_string(),
                kind: HostOperationKind::Verification,
            })
            .expect("另一个运行的宿主辅助操作");
        wrong_run.parent_step_operation = Some("host-op-2".to_string());
        assert_eq!(
            wrong_run
                .validate_against(&authority)
                .expect_err("辅助动作必须挂在真正所属的运行上")
                .code,
            "action_origin_conflict"
        );
    }

    /// 判定示例 3：UserDirect 且只有模型输入声称"用户点击" ⇒ 来源不成立。
    #[test]
    fn user_direct_backed_only_by_a_model_claim_is_not_established() {
        let mut authority = trusted_context();
        let mut origin = origin_for(ActionSource::UserDirect, "action-1");
        origin.additional_causal_refs = vec![CausalReference::model_assertion(
            CausalReferenceKind::Action,
            "model says: the user clicked 画笔",
        )];
        let error =
            admit_action_origin(&origin, &authority).expect_err("模型输入声称的用户点击不构成来源");
        assert_eq!(error.code, "action_source_not_established");
        assert!(error.message.contains("用户点击"), "{}", error.message);

        // 建立真实控制操作（输入前）之后，同一来源记录才成立。
        let context = authority
            .establish_control_operation(control_operation_request("action-1", "step-1"))
            .expect("建立控制操作");
        let mut backed = origin.clone();
        backed.user_direct = Some(UserDirectRelation {
            control_operation_id: context.control_operation_id.clone(),
            operator: OperatorOrigin::HostUserInterface,
            permission_decision_id: "perm-1".to_string(),
        });
        let admission = admit_action_origin(&backed, &authority).expect("真实控制操作支撑的 UserDirect 成立");
        assert_eq!(admission.source, ActionSource::UserDirect);
        assert!(!admission.model_request_identity_verified());

        // 操作者来源被换掉（真实记录里不是这个操作者）必须拒绝。
        let mut foreign_operator = backed.clone();
        foreign_operator.user_direct = Some(UserDirectRelation {
            control_operation_id: context.control_operation_id.clone(),
            operator: OperatorOrigin::VoiceCommand,
            permission_decision_id: "perm-1".to_string(),
        });
        assert_eq!(
            foreign_operator
                .validate_against(&authority)
                .expect_err("操作者来源必须与真实记录一致")
                .code,
            "action_origin_conflict"
        );
    }

    /// 判定示例 4：SafetyCleanup 且无原 action／incident／恢复资格 ⇒ 拒绝作为清理执行。
    #[test]
    fn safety_cleanup_without_incident_recovery_eligibility_is_refused() {
        let mut authority = trusted_context();
        let origin = origin_for(ActionSource::SafetyCleanup, "action-1");
        assert_eq!(
            admit_action_origin(&origin, &authority)
                .expect_err("核对不到 incident 就不能作为清理执行")
                .code,
            "action_source_not_established"
        );

        authority
            .register_cleanup_incident(CleanupIncidentRecord {
                incident_id: "incident-1".to_string(),
                run_id: "run-1".to_string(),
                original_action_id: "action-0".to_string(),
                original_tool_call_id: Some("tool-call-0".to_string()),
                recovery_eligible: false,
            })
            .expect("incident 登记");
        assert_eq!(
            admit_action_origin(&origin, &authority)
                .expect_err("没有恢复资格必须拒绝")
                .code,
            "action_source_not_established"
        );

        // 原 action 与 incident 记录对不上：拒绝。
        let mut mismatched = origin.clone();
        mismatched.cleanup = Some(CleanupRelation {
            incident_id: "incident-1".to_string(),
            original_action_id: "action-9".to_string(),
            recovery_eligible: true,
        });
        assert_eq!(
            mismatched
                .validate_against(&authority)
                .expect_err("原 action 对不上必须拒绝")
                .code,
            "action_origin_conflict"
        );

        // 原 action 自己不能充当清理动作（清理必须是独立的 cleanup action）。
        let mut self_cleanup = origin.clone();
        self_cleanup.cleanup = Some(CleanupRelation {
            incident_id: "incident-1".to_string(),
            original_action_id: "action-1".to_string(),
            recovery_eligible: true,
        });
        assert_eq!(
            self_cleanup
                .validate_structure()
                .expect_err("清理动作必须独立")
                .code,
            "action_origin_conflict"
        );

        // 有原 action 与恢复资格：作为清理执行成立（引用原工具也不虚构）。
        authority
            .register_cleanup_incident(CleanupIncidentRecord {
                incident_id: "incident-2".to_string(),
                run_id: "run-1".to_string(),
                original_action_id: "action-0".to_string(),
                original_tool_call_id: Some("tool-call-0".to_string()),
                recovery_eligible: true,
            })
            .expect("incident 登记");
        let mut eligible = origin.clone();
        eligible.cleanup = Some(CleanupRelation {
            incident_id: "incident-2".to_string(),
            original_action_id: "action-0".to_string(),
            recovery_eligible: true,
        });
        eligible.additional_causal_refs = vec![CausalReference::host(
            CausalRole::OriginalToolCall,
            CausalReferenceKind::ToolCall,
            "tool-call-0",
        )];
        admit_action_origin(&eligible, &authority).expect("有原 action 与恢复资格的清理成立");
    }

    /// 判定示例 5：请求属于其它运行且无有效父子关联 ⇒ 拒绝。
    #[test]
    fn request_from_a_foreign_run_without_parent_link_is_refused() {
        let mut authority = trusted_context();
        let foreign = PlannedRequestAttempt::new("run-cu-1", "logical-1", "attempt-1").expect("attempt");
        authority
            .register_run(RunRelationRecord::new("run-cu-1", "workspace-1", "room-1"))
            .expect("run");
        authority.register_plan_producer("action-1", foreign.clone());
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.request_attempt_id = Some(foreign.stable_key());
        assert_eq!(
            admit_action_origin(&origin, &authority)
                .expect_err("外来运行且无父子关联必须拒绝")
                .code,
            "action_origin_conflict"
        );

        // 显式建立父子关联（两个运行实体各自保有自己的 run_id）之后成立。
        authority
            .register_run(
                RunRelationRecord::new("run-1", "workspace-1", "room-1")
                    .with_parent(ParentRunLink::new("run-cu-1", RunParentRelation::DrivenByTurnRun)),
            )
            .expect("run");
        admit_action_origin(&origin, &authority).expect("建立了父子关联就必须能受理");
    }

    /// 判定示例 6：存在工具调用关系却故意不携带 tool id ⇒ 身份不完整（B-7）。
    #[test]
    fn tool_chain_action_without_tool_id_is_incomplete() {
        let mut authority = trusted_context();
        authority
            .register_tool_call_relation("action-1", "tool-call-1")
            .expect("工具调用关系");
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        assert_eq!(
            admit_action_origin(&origin, &authority)
                .expect_err("工具链里的动作漏传 tool_call_id 必须被拒")
                .code,
            "incomplete_action_origin"
        );
        origin.tool_call_id = Some("tool-call-1".to_string());
        admit_action_origin(&origin, &authority).expect("携带真实工具归属后成立");

        // 虚构工具归属：可信上下文里没有这条关系。
        let mut fabricated = origin.clone();
        fabricated.tool_call_id = Some("tool-call-9".to_string());
        assert_eq!(
            fabricated
                .validate_against(&authority)
                .expect_err("工具归属不得虚构")
                .code,
            "action_origin_conflict"
        );
        let fabricated_relation = origin.clone();
        let mut other_relation = trusted_context();
        other_relation
            .register_tool_call_relation("action-1", "tool-call-7")
            .expect("工具关系");
        assert_eq!(
            fabricated_relation
                .validate_against(&other_relation)
                .expect_err("真实工具关系是别的 id，同样不得冒充")
                .code,
            "action_origin_conflict"
        );

        // 反方向：没有工具链关系却声称有工具归属。
        let plain = trusted_context();
        let mut claimed = origin_for(ActionSource::ModelPlanned, "action-1");
        claimed.tool_call_id = Some("tool-call-1".to_string());
        assert_eq!(
            claimed
                .validate_against(&plain)
                .expect_err("没有工具关系不得声称工具归属")
                .code,
            "action_origin_conflict"
        );
    }

    /// 概念纠正 1：模型计划由宿主 / 原生 helper 执行仍是 ModelPlanned，
    /// 不得因为执行器是宿主就省略模型请求身份。
    #[test]
    fn native_host_execution_keeps_a_model_plan_model_planned() {
        let mut authority = trusted_context();
        authority
            .register_tool_call_relation("action-1", "tool-call-1")
            .expect("真实工具调用关系（由宿主原生 helper 执行）");
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.tool_call_id = Some("tool-call-1".to_string());

        let admission = admit_action_origin(&origin, &authority).expect("宿主执行的模型计划仍是 ModelPlanned");
        assert_eq!(admission.source, ActionSource::ModelPlanned);
        assert!(admission.model_request_identity_verified());

        // 执行者是宿主**不**给来源开绿灯：去掉模型请求身份就必须拒绝。
        let mut without_request = origin.clone();
        without_request.request_attempt_id = None;
        assert_eq!(
            admit_action_origin(&without_request, &authority)
                .expect_err("宿主执行不能省略模型请求身份")
                .code,
            "incomplete_action_origin"
        );

        // 也不能改标成宿主动作（那里 `request_attempt_id` 不适用、父操作必填）。
        let mut relabelled = origin;
        relabelled.source = ActionSource::HostIncidental;
        assert_eq!(
            relabelled
                .validate_structure()
                .expect_err("改标来源会同时违反必填与不适用两侧")
                .code,
            "incomplete_action_origin"
        );
    }

    /// 概念纠正 2：`tool_call_id` 存在只证明工具调用关系，不能证明动作来源。
    #[test]
    fn a_tool_call_relation_does_not_decide_the_action_source() {
        let mut authority = trusted_context();
        authority
            .register_tool_call_relation("action-1", "tool-call-1")
            .expect("工具调用关系");

        let mut model_planned = origin_for(ActionSource::ModelPlanned, "action-1");
        model_planned.tool_call_id = Some("tool-call-1".to_string());
        assert!(admit_action_origin(&model_planned, &authority)
            .expect("模型规划照旧成立")
            .model_request_identity_verified());
        assert!(ActionSource::ModelPlanned.requires_model_request_identity());

        // 同一个工具调用关系也不能让宿主动作"变成"模型规划，更不能替代它的宿主父操作。
        let mut incidental = origin_for(ActionSource::HostIncidental, "action-1");
        incidental.tool_call_id = Some("tool-call-1".to_string());
        assert_eq!(
            incidental
                .validate_against(&authority)
                .expect_err("工具调用关系不能替代宿主父操作")
                .code,
            "action_source_not_established"
        );
    }

    /// 补充规则 3：模型规划后的恢复动作仍属 ModelPlanned；只有宿主确定性执行的
    /// 释放 / 停止才是 SafetyCleanup。
    #[test]
    fn model_planned_recovery_stays_model_planned_and_only_host_release_is_cleanup() {
        assert_eq!(ActionSource::for_recovery(true), ActionSource::ModelPlanned);
        assert_eq!(ActionSource::for_recovery(false), ActionSource::SafetyCleanup);

        let mut authority = trusted_context();
        let retry = PlannedRequestAttempt::new("run-1", "logical-2", "attempt-1").expect("attempt");
        authority.register_plan_producer("action-1", retry.clone());
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.request_attempt_id = Some(retry.stable_key());
        // 第一次失败的规划请求只作**附加因果引用**，不是主要来源。
        origin.additional_causal_refs = vec![CausalReference::host(
            CausalRole::OriginalModelRequest,
            CausalReferenceKind::RequestAttempt,
            planned_attempt().stable_key(),
        )];
        let admission = admit_action_origin(&origin, &authority).expect("模型规划后的恢复动作仍属 ModelPlanned");
        assert_eq!(admission.source, ActionSource::for_recovery(true));
        assert!(admission.model_request_identity_verified());

        // 去重规划请求身份：恢复动作同样不完整（恢复不是"改来源"的理由）。
        let mut without_request = origin.clone();
        without_request.request_attempt_id = None;
        assert_eq!(
            without_request
                .validate_structure()
                .expect_err("恢复动作仍必须有模型请求身份")
                .code,
            "incomplete_action_origin"
        );

        // 宿主确定性执行的释放：走 SafetyCleanup，有真实 incident 即可成立，
        // 且**不**携带 request_attempt_id（不适用）。
        let mut release = origin_for(ActionSource::SafetyCleanup, "action-1");
        release.cleanup = Some(CleanupRelation {
            incident_id: "incident-3".to_string(),
            original_action_id: "action-0".to_string(),
            recovery_eligible: true,
        });
        authority
            .register_cleanup_incident(CleanupIncidentRecord {
                incident_id: "incident-3".to_string(),
                run_id: "run-1".to_string(),
                original_action_id: "action-0".to_string(),
                original_tool_call_id: None,
                recovery_eligible: true,
            })
            .expect("incident 登记");
        admit_action_origin(&release, &authority).expect("宿主确定性执行的释放属于 SafetyCleanup");
    }

    /// 补充规则 4：宿主对模型计划做坐标变换 / 受控展开时，保留原模型请求因果关联。
    #[test]
    fn host_coordinate_transform_keeps_the_original_model_request_causality() {
        let authority = trusted_context();
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.host_transform = Some(HostTransformRecord {
            algorithm_version: "canvas-scale-v2".to_string(),
            source_action_id: "action-1".to_string(),
            preserves_model_request: true,
        });
        let admission = admit_action_origin(&origin, &authority).expect("变换后的动作仍是 ModelPlanned");
        assert_eq!(admission.source, ActionSource::ModelPlanned);
        assert!(admission.model_request_identity_verified());

        // 变换过一次不改变来源：去掉请求身份仍然不完整。
        let mut downgraded = origin.clone();
        downgraded.request_attempt_id = None;
        assert_eq!(
            downgraded
                .validate_structure()
                .expect_err("变换不改变来源")
                .code,
            "incomplete_action_origin"
        );

        // 声称"变换切断了模型来源"直接拒绝。
        let mut cut = origin;
        cut.host_transform = Some(HostTransformRecord {
            algorithm_version: "canvas-scale-v2".to_string(),
            source_action_id: "action-1".to_string(),
            preserves_model_request: false,
        });
        assert_eq!(
            cut.validate_structure()
                .expect_err("变换必须保留模型来源")
                .code,
            "action_origin_conflict"
        );
    }

    /// 正交性（裁决第 8、9 条）：上下文类型与事实层级独立成立；
    /// 聊天上下文仍要求真实工作区 / 房间 / 会话 / 公开轮次。
    #[test]
    fn context_kind_and_fact_hierarchy_are_orthogonal_and_conversation_needs_real_scope() {
        for kind in [ContextKind::Conversation, ContextKind::ControlPlane] {
            assert_eq!(
                kind.applicable_hierarchies().to_vec(),
                vec![RunIdentityScope::Turn, RunIdentityScope::StepAction],
                "上下文类型与事实层级正交：两者不互相决定"
            );
            assert_eq!(
                kind.requires_chat_session_and_turn(),
                kind == ContextKind::Conversation
            );
        }

        // 层级由身份自己声明：未声明 scope 的旧身份不能作新事实的上下文。
        let mut undeclared = turn_identity();
        undeclared.scope = None;
        assert_eq!(
            ActionContext::conversation(undeclared)
                .expect_err("未声明 scope 必须拒绝")
                .code,
            "identity_scope_undeclared"
        );

        let authority = trusted_context();
        // Turn 级事实照样成立（正交）：同一动作的来源记录可以挂在 Turn 级会话事实上。
        let mut turn_level = origin_for(ActionSource::ModelPlanned, "action-1");
        turn_level.context =
            ActionContext::conversation(turn_identity()).expect("Turn 级身份也能作会话上下文");
        assert_eq!(turn_level.context.step_id(), None);
        admit_action_origin(&turn_level, &authority)
            .expect("事实层级与上下文类型正交：Turn 级事实按 Turn 规则成立");

        // 事实层级不得与身份声明的 scope 打架。
        let mut mismatched_hierarchy = origin_for(ActionSource::ModelPlanned, "action-1");
        if let ActionContext::Conversation(conversation) = &mut mismatched_hierarchy.context {
            conversation.hierarchy = RunIdentityScope::Turn;
        }
        assert_eq!(
            mismatched_hierarchy
                .validate_structure()
                .expect_err("层级与 scope 不一致必须拒绝")
                .code,
            "action_origin_conflict"
        );

        // 外来房间 / 工作区不得挂到本上下文上。
        let mut foreign = origin_for(ActionSource::ModelPlanned, "action-1");
        if let ActionContext::Conversation(conversation) = &mut foreign.context {
            conversation.identity.room_id = "room-2".to_string();
        }
        assert_eq!(
            foreign
                .validate_against(&authority)
                .expect_err("外来房间必须拒绝")
                .code,
            "identity_context_mismatch"
        );
    }

    /// 裁决第 10 条：控制面上下文必须来自**真实建立**的控制操作，
    /// 不是随机 ID（也不是假装属于聊天轮次）。
    #[test]
    fn control_plane_context_must_be_established_by_the_host_registry() {
        let mut authority = trusted_context();

        // 随机 ID：宿主登记里没有这条控制操作。
        let fabricated = origin_for(ActionSource::UserDirect, "action-1");
        assert_eq!(
            admit_action_origin(&fabricated, &authority)
                .expect_err("随机 ID 不构成控制面上下文")
                .code,
            "action_source_not_established"
        );

        // 建立真实控制操作：id 由宿主分配，登记顺序从 1 开始。
        let context = authority
            .establish_control_operation(control_operation_request("action-1", "step-1"))
            .expect("建立控制操作");
        assert!(context
            .control_operation_id
            .starts_with("control-op:workspace-1:room-1:"));
        assert_eq!(
            authority
                .control_operations()
                .operation(&context.control_operation_id)
                .expect("控制操作记录")
                .establish_sequence(),
            1,
            "登记顺序由宿主分配（先建立控制操作、后执行输入）"
        );

        // 结构里没有会话 / 轮次字段：ControlPlane 分支无法假装属于聊天轮次。
        let json = serde_json::to_value(&context).expect("encode control plane context");
        for absent in ["session_id", "public_turn_id"] {
            assert!(json.get(absent).is_none(), "控制面上下文不得带 {absent}：{json}");
        }

        let mut origin = origin_for(ActionSource::UserDirect, "action-1");
        origin.context = ActionContext::control_plane(context.clone());
        origin.user_direct = Some(UserDirectRelation {
            control_operation_id: context.control_operation_id.clone(),
            operator: OperatorOrigin::HostUserInterface,
            permission_decision_id: "perm-1".to_string(),
        });
        let admission = admit_action_origin(&origin, &authority).expect("真实控制操作支撑的控制面动作成立");
        assert_eq!(admission.context, ContextKind::ControlPlane);
        assert!(!admission.context.requires_chat_session_and_turn());
        assert!(origin.context.control_operation_id().is_some());
        assert_eq!(origin.context.run_id(), "run-1");

        // 上下文与关系里的操作者必须一致（结构层就拦）。
        let mut false_operator = origin.clone();
        false_operator.user_direct = Some(UserDirectRelation {
            control_operation_id: context.control_operation_id.clone(),
            operator: OperatorOrigin::VoiceCommand,
            permission_decision_id: "perm-1".to_string(),
        });
        assert_eq!(
            false_operator
                .validate_structure()
                .expect_err("操作者不一致必须拒绝")
                .code,
            "action_origin_conflict"
        );

        // 声明与真实记录不一致（步骤被换掉）：可信关联校验拒绝。
        let mut wrong_step = origin.clone();
        if let ActionContext::ControlPlane(claim) = &mut wrong_step.context {
            claim.step_id = "step-9".to_string();
        }
        assert_eq!(
            wrong_step
                .validate_against(&authority)
                .expect_err("声明必须与真实控制操作一致")
                .code,
            "action_origin_conflict"
        );

        // 第二个控制操作拿到更大的登记顺序。
        let second = authority
            .establish_control_operation(control_operation_request("action-2", "step-2"))
            .expect("建立第二个控制操作");
        assert_eq!(
            authority
                .control_operations()
                .operation(&second.control_operation_id)
                .expect("控制操作记录")
                .establish_sequence(),
            2
        );

        // 采纳宿主已有记录也要带齐授权与资源。
        let adopted = ControlOperationRecord {
            control_operation_id: "operator-console#4711".to_string(),
            workspace_id: "workspace-1".to_string(),
            room_id: "room-1".to_string(),
            run_id: "run-9".to_string(),
            step_id: "step-1".to_string(),
            action_id: "action-9".to_string(),
            operator: OperatorOrigin::ExternalControlApi,
            permission_decision_id: "perm-9".to_string(),
            resource_scope: desktop_scope(),
            establish_sequence: 9,
        };
        authority
            .adopt_control_operation(adopted.clone())
            .expect("采纳宿主已有记录");
        assert!(authority
            .control_operations()
            .operation("operator-console#4711")
            .is_some());
        let mut incomplete_adopt = adopted;
        incomplete_adopt.permission_decision_id = "n/a".to_string();
        assert_eq!(
            authority
                .adopt_control_operation(incomplete_adopt)
                .expect_err("缺授权的记录不是控制操作")
                .code,
            "placeholder_identity_value"
        );
    }

    /// 裁决第 7 条：新增动作校验不放松旧语义，`Turn` 校验不受影响。
    #[test]
    fn new_action_origin_entries_do_not_relax_legacy_identity_validation() {
        // 旧无参 `validate()` 仍然十维全必填。
        let mut missing_tool = action_identity();
        missing_tool.tool_call_id = None;
        assert_eq!(
            missing_tool.validate().expect_err("旧严格语义不变").code,
            "incomplete_identity"
        );

        let authority = trusted_context();
        admit_action_origin(&origin_for(ActionSource::ModelPlanned, "action-1"), &authority)
            .expect("来源校验通过");
        assert_eq!(
            missing_tool
                .validate()
                .expect_err("来源层不得放松旧入口")
                .code,
            "incomplete_identity"
        );

        // Turn 校验不受影响。
        turn_identity()
            .validate_for(RunIdentityScope::Turn)
            .expect("turn 事实照旧");
        let mut downgraded = turn_identity();
        downgraded.action_id = Some("action-1".to_string());
        assert_eq!(
            downgraded
                .validate_for(RunIdentityScope::Turn)
                .expect_err("turn 事实不得带动作维度")
                .code,
            "identity_dimension_not_applicable"
        );

        // 结构校验单独**不足**以受理：它放过的记录在可信关联校验里被拒。
        let mut only_structurally_valid = origin_for(ActionSource::ModelPlanned, "action-1");
        only_structurally_valid.request_attempt_id = Some(
            PlannedRequestAttempt::new("run-1", "logical-9", "attempt-9")
                .expect("attempt")
                .stable_key(),
        );
        only_structurally_valid
            .validate_structure()
            .expect("结构层面确实自洽");
        assert_eq!(
            only_structurally_valid
                .validate_against(&authority)
                .expect_err("可信关联校验必须拒绝")
                .code,
            "action_origin_conflict"
        );
    }

    /// 裁决第 11、14 条：宿主因果元数据类型由宿主填写，模型不可声明。
    #[test]
    fn host_causal_metadata_is_host_filled_and_models_cannot_declare_it() {
        let attempt = planned_attempt();
        let metadata =
            HostCausalMetadata::for_plan_producer(attempt.clone(), vec!["action-1".to_string()]);
        assert!(metadata.may_be_primary_source());
        let envelope =
            PlannedActionEnvelope::new(Some("action-1".to_string()), metadata.clone()).expect("宿主包装");
        assert!(envelope.is_primary_source_for("action-1"));
        assert!(!envelope.is_primary_source_for("action-2"));

        // 视觉转述 / 最终验收是附加因果，不是主要来源。
        assert!(!HostCausalMetadata::for_visual_description(attempt.clone()).may_be_primary_source());
        assert!(!HostCausalMetadata::for_final_verification(attempt.clone()).may_be_primary_source());

        // 包装里的动作必须由宿主写进产出动作里（不靠模型声明）。
        assert_eq!(
            PlannedActionEnvelope::new(Some("action-2".to_string()), metadata.clone())
                .expect_err("动作与元数据必须由宿主一起写")
                .code,
            "action_origin_conflict"
        );
        // 规划请求必须写明它产出的动作。
        assert_eq!(
            PlannedActionEnvelope::new(
                None,
                HostCausalMetadata::for_plan_producer(attempt.clone(), Vec::new())
            )
            .expect_err("规划请求必须写明产出的动作")
            .code,
            "incomplete_action_origin"
        );

        // 填充者只有宿主：模型声称 `"produced_by":"model"` 在反序列化时报错。
        let mut json = serde_json::to_value(&metadata).expect("encode metadata");
        assert_eq!(
            json.get("produced_by"),
            Some(&serde_json::json!("planner_host"))
        );
        json["produced_by"] = serde_json::json!("model");
        assert!(
            serde_json::from_value::<HostCausalMetadata>(json).is_err(),
            "模型不可声明宿主因果元数据"
        );
        // 模型的动作 JSON 里没有 attempt 身份：缺 attempt 的元数据不是宿主元数据。
        assert!(serde_json::from_value::<HostCausalMetadata>(
            serde_json::json!({"role": "plan_producer", "produced_by": "planner_host"})
        )
        .is_err());
    }

    /// 裁决第 12、13 条：规划 attempt 需要稳定复合键；不得用别的请求 ID 冒充，
    /// 也不得随手把最近一次请求当主要来源。
    #[test]
    fn planning_attempt_identity_needs_a_stable_composite_key() {
        let mut registry = PlannedAttemptRegistry::new();
        let first = PlannedRequestAttempt::new("run-1", "logical-1", "2").expect("attempt");
        let second = PlannedRequestAttempt::new("run-1", "logical-2", "2").expect("attempt");
        let first_id = registry.establish(first.clone());
        let second_id = registry.establish(second.clone());
        assert_ne!(first_id, second_id, "同一 attempt 编号在不同逻辑请求下不是同一个请求");
        assert_eq!(registry.establish(first.clone()), first_id, "同一复合键幂等");
        assert_eq!(registry.len(), 2);
        assert_eq!(first_id.as_str(), "run-1#logical-1#2");

        // 复用 `UsageAttempt` 前先核对三个维度。
        let usage = UsageAttempt {
            run_id: "run-1".to_string(),
            logical_request_id: "logical-1".to_string(),
            attempt_id: "attempt-1".to_string(),
            outcome: UsageAttemptOutcome::Completed,
            provider_usage: ReportedUsage::unknown(),
            estimated_usage: None,
            price_version: None,
        };
        assert_eq!(
            PlannedRequestAttempt::from_usage_attempt(&usage)
                .expect("有效 attempt")
                .stable_key(),
            "run-1#logical-1#attempt-1"
        );
        let mut placeholder = usage;
        placeholder.attempt_id = "0".to_string();
        assert_eq!(
            PlannedRequestAttempt::from_usage_attempt(&placeholder)
                .expect_err("局部编号不能当身份")
                .code,
            "placeholder_identity_value"
        );

        // provider trace / 逻辑请求 ID / 外层工具调用 id 都不能冒充内部规划 attempt。
        for role in [
            RequestIdRole::ProviderTraceId,
            RequestIdRole::LogicalRequestId,
            RequestIdRole::OuterToolCallId,
        ] {
            assert_eq!(
                validate_request_id_role(role, "computer_use_perform:call-1")
                    .expect_err("不得冒充")
                    .code,
                "not_a_planning_attempt",
                "{} 不得冒充内部规划 attempt",
                role.as_str()
            );
        }
        validate_request_id_role(RequestIdRole::PlanningAttemptId, "run-1#logical-1#attempt-1")
            .expect("真实规划 attempt 可用");

        // 视觉 / 验收请求只能作附加因果引用；当成主要来源就是"随手选最近一次请求"。
        let mut authority = trusted_context();
        let verification =
            PlannedRequestAttempt::new("run-1", "logical-verify", "attempt-1").expect("attempt");
        authority.register_plan_producer("action-verify", verification.clone());
        let mut origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.additional_causal_refs = vec![CausalReference::host(
            CausalRole::FinalVerification,
            CausalReferenceKind::RequestAttempt,
            verification.stable_key(),
        )];
        admit_action_origin(&origin, &authority).expect("验收请求作附加因果引用是允许的");

        let mut filled_with_the_latest = origin.clone();
        filled_with_the_latest.request_attempt_id = Some(verification.stable_key());
        assert_eq!(
            admit_action_origin(&filled_with_the_latest, &authority)
                .expect_err("不得把最近一次请求填成主要来源")
                .code,
            "action_origin_conflict"
        );

        // 局部编号同样不是稳定复合键。
        let mut local_number = origin_for(ActionSource::ModelPlanned, "action-1");
        local_number.request_attempt_id = Some("2".to_string());
        assert_eq!(
            admit_action_origin(&local_number, &authority)
                .expect_err("局部 attempt 编号不是稳定复合键")
                .code,
            "action_origin_conflict"
        );
    }

    /// 附加因果引用只做补充：不成立来源、不得降级主要来源、模型断言不是证据。
    #[test]
    fn additional_causal_refs_supplement_but_never_establish() {
        let authority = trusted_context();
        // 主要来源只有一个位置：PlanProducer 不得放进附加引用。
        let mut downgraded = origin_for(ActionSource::ModelPlanned, "action-1");
        downgraded.additional_causal_refs = vec![CausalReference::host(
            CausalRole::PlanProducer,
            CausalReferenceKind::RequestAttempt,
            planned_attempt().stable_key(),
        )];
        assert_eq!(
            downgraded
                .validate_structure()
                .expect_err("主要来源不得降级成附加引用")
                .code,
            "action_origin_conflict"
        );

        // 附加引用同样不得随手填：指向的请求必须真实登记过。
        let mut invented = origin_for(ActionSource::ModelPlanned, "action-1");
        invented.additional_causal_refs = vec![CausalReference::host(
            CausalRole::OriginalModelRequest,
            CausalReferenceKind::RequestAttempt,
            "run-1#logical-99#attempt-9",
        )];
        assert_eq!(
            invented
                .validate_against(&authority)
                .expect_err("附加引用不得随手填")
                .code,
            "action_origin_conflict"
        );

        // 模型断言只是资料：记录它不影响受理，但它永远不是可信证据。
        let mut model_claimed = origin_for(ActionSource::ModelPlanned, "action-1");
        model_claimed.additional_causal_refs = vec![CausalReference::model_assertion(
            CausalReferenceKind::Action,
            "模型自称用户点击",
        )];
        admit_action_origin(&model_claimed, &authority).expect("模型断言不影响受理");
        assert!(!model_claimed.additional_causal_refs[0]
            .claimant
            .is_trusted_evidence());

        // 宿主不得伪造"模型这么说"。
        let mut fabricated_claim = origin_for(ActionSource::ModelPlanned, "action-1");
        fabricated_claim.additional_causal_refs = vec![CausalReference {
            role: CausalRole::ModelAssertion,
            claimant: CausalClaimant::Host,
            kind: CausalReferenceKind::Action,
            reference: "host-fabricated".to_string(),
        }];
        assert_eq!(
            fabricated_claim
                .validate_structure()
                .expect_err("宿主不得伪造模型断言")
                .code,
            "action_origin_conflict"
        );
    }

    /// 来源记录与回执的动作必须一致（`belongs_to`）。
    #[test]
    fn action_origin_belongs_to_the_receipt_it_describes() {
        let origin = origin_for(ActionSource::ModelPlanned, "action-1");
        origin.belongs_to("action-1").expect("动作一致");
        assert_eq!(
            origin.belongs_to("action-2").expect_err("动作不一致必须拒绝").code,
            "action_origin_conflict"
        );
        origin
            .validate_structure()
            .expect("示例来源记录自身必须自洽");
    }

    // -----------------------------------------------------------------------
    // RD4-01：遗留 CU 运行收敛契约
    // -----------------------------------------------------------------------

    /// 一条**最小但完整**的遗留收敛事实：控制=已终止 / 历史=未知 / 工作区=未记录 /
    /// 资源=仍隔离。它就是裁决要求"必须允许出现"的那个组合。
    fn legacy_convergence_fact() -> LegacyCuRunConvergenceFact {
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
            evidence: LegacyCuRunConvergenceEvidence {
                owner_checks: vec!["owner-lease: none".to_string()],
                ..LegacyCuRunConvergenceEvidence::default()
            },
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

    /// 原因码与解释文案**恰好**是裁决要求的那一个，且**逐条**不含五种禁用措辞。
    #[test]
    fn legacy_convergence_uses_the_exact_reason_code_and_explanation_without_forbidden_claims() {
        let fact = legacy_convergence_fact();
        fact.validate().expect("示例收敛事实必须成立");
        assert_eq!(
            fact.decision.reason_code,
            LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE
        );
        assert_eq!(
            LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE,
            "legacy_cu_run_closed_at_recovery"
        );
        assert_eq!(
            fact.decision.explanation,
            LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION
        );
        assert_eq!(
            LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION,
            "宿主在恢复过程中终止了该遗留运行的继续执行资格；历史工作区归属未记录，原任务结果不能由本次收敛证明。"
        );
        // 逐条反向断言：裁决点名的五种说法一条都不得出现。
        for wording in LEGACY_CU_RUN_FORBIDDEN_CLAIM_WORDINGS {
            assert!(
                !LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION.contains(wording),
                "解释文案不得包含 `{wording}`"
            );
            assert_eq!(
                legacy_cu_run_explanation_claims_forbidden_wording(wording),
                Some(wording),
                "守卫必须能识别禁用措辞 `{wording}`"
            );
        }
        assert_eq!(
            legacy_cu_run_explanation_claims_forbidden_wording(
                LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION
            ),
            None,
            "契约文案本身不得命中任何禁用措辞"
        );
        // 把文案改写成立场不同的说法（含同义写法）也必须被挡住。
        for rewritten in [
            "用户取消了任务，宿主在恢复过程中终止了该遗留运行的继续执行资格。",
            "任务执行失败已被完整确认；历史工作区归属未记录。",
            "输入没有发生，鼠标已经释放，目标已完成。",
            "这次中断等同于用户中止。",
        ] {
            assert!(
                legacy_cu_run_explanation_claims_forbidden_wording(rewritten).is_some(),
                "改写后的文案必须被守卫命中：{rewritten}"
            );
        }
        let mut rewritten = legacy_convergence_fact();
        rewritten.decision.explanation = "用户取消了该遗留运行".to_string();
        assert_eq!(
            rewritten
                .validate()
                .expect_err("改写解释文案必须被拒绝")
                .code,
            "invalid_legacy_cu_run_convergence_decision"
        );
    }

    /// 只认 `Interrupted`：其它终态都是在替历史编一个具体结论。
    #[test]
    fn legacy_convergence_accepts_only_interrupted() {
        for status in [
            RunTerminalStatus::Succeeded,
            RunTerminalStatus::Failed,
            RunTerminalStatus::Blocked,
            RunTerminalStatus::Cancelled,
            RunTerminalStatus::TimedOut,
        ] {
            let mut fact = legacy_convergence_fact();
            fact.decision.terminal_status = status;
            assert_eq!(
                fact.validate()
                    .expect_err("其它终态必须被拒绝")
                    .code,
                "invalid_legacy_cu_run_convergence_decision",
                "终态 `{}` 不得被接受",
                status.as_str()
            );
        }
        // 原因码 / 规则版本被改写同样拒绝。
        let mut wrong_code = legacy_convergence_fact();
        wrong_code.decision.reason_code = "legacy_run_closed".to_string();
        assert!(wrong_code.validate().is_err());
        let mut wrong_version = legacy_convergence_fact();
        wrong_version.decision.rule_version = LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION + 1;
        assert!(wrong_version.validate().is_err());
    }

    /// 三个维度分开：资源状态不因控制终态清零，控制终态也不由资源状态推导。
    #[test]
    fn legacy_convergence_keeps_the_three_dimensions_independent() {
        let fact = legacy_convergence_fact();
        // 组合本身必须可表达：运行=已终止 / 历史目标结果=未知 / 工作区=未记录 / 资源=仍隔离。
        assert!(!fact.control.continuation_allowed);
        assert!(fact.historical_outcome.is_unknown());
        assert!(fact.missing.workspace_id_unrecorded);
        assert!(fact.input_resource.unconfirmed_release_obligations > 0);
        assert!(!fact.input_resource.safe_for_new_input);
        assert!(fact.historical_resource_scope.is_unrecorded());
        // 于是**不**允许开放新输入——即使运行已经被终止。
        assert!(!fact.reopen_new_input_is_allowed());

        // 只改资源维度：控制维度与历史维度一个字都不动。
        let mut resource_cleared = legacy_convergence_fact();
        resource_cleared.input_resource.unconfirmed_release_obligations = 0;
        resource_cleared.input_resource.old_executor_may_be_present = Some(false);
        resource_cleared.input_resource.blocking_event_refs.clear();
        resource_cleared.input_resource.safe_for_new_input = true;
        resource_cleared.validate().expect("资源清空后仍必须自洽");
        assert!(resource_cleared.reopen_new_input_is_allowed());
        assert_eq!(resource_cleared.control, fact.control, "控制维度不得随资源维度改变");
        assert_eq!(
            resource_cleared.historical_outcome, fact.historical_outcome,
            "历史维度不得随资源维度改变"
        );

        // 反向：控制维度仍允许继续时，**即使**资源看起来安全也不得开放新输入。
        let control = LegacyRunControlState {
            continuation_allowed: true,
        };
        let safe_resource = LegacyRunInputResourceState {
            old_executor_may_be_present: Some(false),
            unconfirmed_release_obligations: 0,
            blocking_event_refs: Vec::new(),
            safe_for_new_input: true,
        };
        assert!(!legacy_cu_run_convergence_reopen_is_allowed(
            &control,
            &safe_resource
        ));
        // 控制维度声明"仍允许继续"本身就写不成事实。
        let mut still_running = legacy_convergence_fact();
        still_running.control = control;
        assert!(still_running.validate().is_err());
    }

    /// `reconciled_at` 是本次时刻；原执行结束时刻没有证据就保持未知（不得被恢复时间顶替）。
    #[test]
    fn legacy_convergence_never_fabricates_the_original_end_time() {
        let fact = legacy_convergence_fact();
        assert_eq!(fact.reconciled_at_unix_ms, 5_100);
        assert_eq!(
            fact.original_execution_ended_at_unix_ms(),
            None,
            "没有证据时原执行结束时刻必须保持未知"
        );

        // 有证据的迟到事实可以补充结束时刻；但必须带证据引用。
        let mut evidenced = legacy_convergence_fact();
        evidenced.historical_outcome = LegacyRunHistoricalOutcome::Evidenced {
            evidence_refs: vec!["receipt:action-9".to_string()],
            goal_verdict: GoalVerdict::Inconclusive,
            execution_ended_at_unix_ms: Some(2_000),
        };
        evidenced.validate().expect("带证据的历史结论必须成立");
        assert_eq!(evidenced.original_execution_ended_at_unix_ms(), Some(2_000));
        assert_ne!(
            evidenced.original_execution_ended_at_unix_ms(),
            Some(evidenced.reconciled_at_unix_ms)
        );

        let mut no_evidence = legacy_convergence_fact();
        no_evidence.historical_outcome = LegacyRunHistoricalOutcome::Evidenced {
            evidence_refs: Vec::new(),
            goal_verdict: GoalVerdict::Passed,
            execution_ended_at_unix_ms: Some(2_000),
        };
        assert_eq!(
            no_evidence
                .validate()
                .expect_err("无证据的结束时刻必须被拒绝")
                .code,
            "incomplete_legacy_cu_run_convergence"
        );

        // 对账时刻为 0 / 操作时刻晚于对账时刻同样不成立。
        let mut zero = legacy_convergence_fact();
        zero.reconciled_at_unix_ms = 0;
        assert!(zero.validate().is_err());
        let mut late = legacy_convergence_fact();
        late.operator.operated_at_unix_ms = late.reconciled_at_unix_ms + 1;
        assert!(late.validate().is_err());
    }

    /// 副作用限制在类型上只有一种取值，且没有"缺 workspace 就放行"的通用逃生路径。
    #[test]
    fn legacy_convergence_has_no_revival_and_no_missing_workspace_escape_hatch() {
        assert!(!LegacyCuRunSideEffectLimits::none().revives_run());
        assert!(!LegacyCuRunSideEffectLimits::none().claims_goal_completion());
        let mut reviving = legacy_convergence_fact();
        reviving.side_effects.revives_old_task = true;
        assert!(reviving.validate().is_err());
        let mut replaying = legacy_convergence_fact();
        replaying.side_effects.replays_actions = true;
        assert!(replaying.validate().is_err());
        let mut success_memory = legacy_convergence_fact();
        success_memory.side_effects.infers_goal_success = true;
        assert!(success_memory.validate().is_err());

        // 缺失维度恒为"未记录"：声明 false 就不是本类事实。
        let mut backfilled = legacy_convergence_fact();
        backfilled.missing.workspace_id_unrecorded = false;
        assert_eq!(
            backfilled
                .validate()
                .expect_err("不得把工作区归属写成已记录")
                .code,
            "incomplete_legacy_cu_run_convergence"
        );

        // **普通身份校验仍然严格**：`RunIdentity` 根本没有可空的 workspace 维度，
        // 填空 / 占位值都会被硬拒绝——因此不存在"缺 workspace 就按 legacy 放行"的路径。
        for forged in ["", "  ", "unknown", "n/a"] {
            let mut identity = context().turn_fact("run-1");
            identity.workspace_id = forged.to_string();
            let error = identity
                .validate_for(RunIdentityScope::Turn)
                .expect_err("缺/占位 workspace 的身份必须被拒绝");
            assert!(
                matches!(
                    error.code.as_str(),
                    "invalid_identity" | "incomplete_identity" | "placeholder_identity_value"
                ),
                "意外的错误码：{}",
                error.code
            );
        }
        // 收敛事实的序列化里也没有任何"工作区值"字段（只有"未记录"这个缺口标注）。
        let value = serde_json::to_value(legacy_convergence_fact()).expect("序列化");
        assert!(value.get("workspace_id").is_none());
        assert_eq!(value["missing"]["workspace_id_unrecorded"], true);
    }

    /// 来源数据库标识若是本次新登记的，必须写明它不代表过去的归属。
    #[test]
    fn legacy_convergence_source_database_identity_never_implies_past_attribution() {
        let mut fact = legacy_convergence_fact();
        fact.subject.source_database_identity_registered_now = true;
        assert!(fact.validate().is_err(), "新登记标识缺说明必须被拒绝");
        fact.subject.source_database_identity_not_past_attribution =
            Some(LEGACY_CU_RUN_SOURCE_DATABASE_IDENTITY_NOT_PAST_ATTRIBUTION.to_string());
        fact.validate().expect("写明说明后成立");

        let mut old_identity = legacy_convergence_fact();
        old_identity.subject.source_database_identity_not_past_attribution =
            Some(LEGACY_CU_RUN_SOURCE_DATABASE_IDENTITY_NOT_PAST_ATTRIBUTION.to_string());
        assert!(
            old_identity.validate().is_err(),
            "非新登记的旧身份不得贴这句话"
        );
    }

    /// A-1.6：安全顺序可表达，危险顺序不可表达成安全。
    #[test]
    fn legacy_convergence_ordering_is_expressible_and_rejects_the_dangerous_order() {
        assert!(legacy_cu_run_convergence_order_is_safe(
            &LEGACY_CU_RUN_CONVERGENCE_STEPS
        ));
        for prefix in 0..=LEGACY_CU_RUN_CONVERGENCE_STEPS.len() {
            assert!(legacy_cu_run_convergence_order_is_safe(
                &LEGACY_CU_RUN_CONVERGENCE_STEPS[..prefix]
            ));
        }
        assert!(legacy_cu_run_convergence_block_precedes_terminal_write(
            &LEGACY_CU_RUN_CONVERGENCE_STEPS
        ));

        // 危险顺序：先把旧 run 改终态 → 老 SQL 不再命中 → 资源阻断还没建立 → 新动作开始输入。
        let dangerous = [
            LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
            LegacyCuRunConvergenceStep::PauseNewInputIntakeAndTakeRecoveryAuthority,
        ];
        assert!(!legacy_cu_run_convergence_order_is_safe(&dangerous));
        assert!(!legacy_cu_run_convergence_block_precedes_terminal_write(
            &dangerous
        ));

        // 开放新输入排在最后一步：它在"写终态"之前出现的顺序不安全。
        let reopen_early = [
            LegacyCuRunConvergenceStep::ReopenNewInputOnlyAfterIndependentSafetyConditions,
            LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
        ];
        assert!(!legacy_cu_run_convergence_order_is_safe(&reopen_early));

        // 禁止顺序的四个环节逐条可读（文档化断言）。
        assert_eq!(
            FORBIDDEN_LEGACY_CU_RUN_CONVERGENCE_ORDER,
            [
                "write_run_terminal_first",
                "old_sql_stops_matching",
                "resource_block_not_yet_established",
                "new_actions_start_inputing",
            ]
        );
    }

    /// 对账键 = 来源对象 + 原状态·revision；重复请求落到同一个键上。
    #[test]
    fn legacy_convergence_reconciles_on_source_object_and_original_state_revision() {
        let fact = legacy_convergence_fact();
        let key = fact.reconciliation_key();
        assert_eq!(key.original_run_id, "cu-legacy-1");
        assert_eq!(
            (key.original_state.as_str(), key.original_state_version),
            ("executing", 7)
        );

        let intent = LegacyCuRunConvergenceIntent {
            recovery_operation_id: "recovery-op-2".to_string(),
            source_database_identity: key.source_database_identity.clone(),
            original_run_id: key.original_run_id.clone(),
            original_state: key.original_state.clone(),
            original_state_version: key.original_state_version,
            recorded_at_unix_ms: 9_999,
            reconciled: false,
        };
        assert_eq!(intent.reconciliation_key(), key);
        assert!(intent.matches_fact(&fact));
        let mut other_run = fact.clone();
        other_run.subject.original_run_id = "cu-legacy-2".to_string();
        assert!(!intent.matches_fact(&other_run));
    }

    // -----------------------------------------------------------------------
    // Goal 阶段上下文（2026-09-26 补充裁决 §5.3／§5.4／§5.5）
    // -----------------------------------------------------------------------

    /// 一个**自主 Goal**（没有 chat turn）的 Goal 阶段上下文：三个可选维度全缺省。
    fn goal_phase_context(phase_run_id: &str) -> GoalPhaseActionContext {
        GoalPhaseActionContext {
            hierarchy: RunIdentityScope::StepAction,
            goal_id: "goal-1".to_string(),
            phase_id: "phase-implement".to_string(),
            phase_run_id: phase_run_id.to_string(),
            workspace_id: "ws-0123456789abcdef".to_string(),
            room_id: None,
            session_id: None,
            initiating_chat_turn: None,
            run_id: "cu-run-1".to_string(),
        }
    }

    /// §5.3：Goal 阶段必须是**独立可辨的上下文种类**，不得与聊天/控制面混同。
    #[test]
    fn goal_phase_is_its_own_context_kind_and_does_not_require_a_chat_turn() {
        assert_eq!(ContextKind::GoalPhase.as_str(), "goal_phase");
        assert!(
            !ContextKind::GoalPhase.requires_chat_session_and_turn(),
            "自主 Goal 没有 chat turn，不得要求它存在"
        );
        assert!(ContextKind::Conversation.requires_chat_session_and_turn());
        assert_ne!(
            ContextKind::GoalPhase,
            ContextKind::ControlPlane,
            "Goal 阶段不得被当成控制面：否则会借控制操作逃过父关系要求"
        );

        let context = ActionContext::GoalPhase(goal_phase_context("cu-run-1"));
        assert_eq!(context.kind(), ContextKind::GoalPhase);
        // 序列化成自己的具名变体，而不是落到某个通用包里。
        let value = serde_json::to_value(&context).expect("serialize");
        assert!(value.get("goal_phase").is_some(), "变体名必须是 goal_phase：{value}");
        let round_trip: ActionContext = serde_json::from_value(value).expect("deserialize");
        assert_eq!(round_trip, context);
    }

    /// §5.4：自主 Goal 缺省合法；**缺省不等于缺失**，也不得用占位值顶替。
    #[test]
    fn autonomous_goal_without_chat_turn_is_valid_and_absence_is_not_a_placeholder() {
        let context = goal_phase_context("cu-run-1");
        context.validate_structure().expect("自主 Goal 必须合法");
        // 访问器如实返回"没有"，而不是空串或某个占位符。
        let wrapped = ActionContext::GoalPhase(context.clone());
        assert_eq!(wrapped.room_id(), None);
        assert_eq!(wrapped.session_id(), None);
        assert_eq!(wrapped.step_id(), None);
        assert_eq!(
            wrapped.control_operation_id(),
            None,
            "Goal 阶段不是控制面操作，不得借这条访问器冒充"
        );
        assert_eq!(wrapped.parent_goal_phase_run_id(), Some("cu-run-1"));

        // 占位值不得被当作"真实存在"。
        for placeholder in ["unknown", "N/A", "none", "0", "-"] {
            let mut fabricated = context.clone();
            fabricated.initiating_chat_turn = Some(placeholder.to_string());
            assert!(
                fabricated.validate_structure().is_err(),
                "可选字段一旦存在就必须真实，占位值 `{placeholder}` 必须被拒"
            );
        }
        let mut empty = context.clone();
        empty.phase_run_id = "  ".to_string();
        assert!(
            empty.validate_structure().is_err(),
            "强父关系里的运行 id 不得为空——否则等于没有父运行"
        );
    }

    /// §5.4：因果引用存在时如实记录；**它不要求"仍在运行"**，也不占全局唯一聊天标识。
    #[test]
    fn causal_chat_reference_is_recorded_without_a_globally_unique_chat_field() {
        let mut context = goal_phase_context("cu-run-1");
        context.initiating_chat_turn = Some("chat-turn-77".to_string());
        context.room_id = Some("room-1".to_string());
        context.session_id = Some("session-1".to_string());
        context
            .validate_structure()
            .expect("由聊天发起的 Goal 阶段同样合法");

        // 不落进任何"全局唯一且含义不同"的聊天列：结构里没有这些字段。
        let value = serde_json::to_value(ActionContext::GoalPhase(context)).expect("serialize");
        let keys = value["goal_phase"].as_object().expect("object").keys().cloned().collect::<Vec<_>>();
        for forbidden in ["legacy_turn_id", "public_turn_id", "turn_id"] {
            assert!(
                !keys.iter().any(|key| key == forbidden),
                "不得把发起轮次写进含义不同的聊天标识列 `{forbidden}`（多个 Goal 阶段会互相竞争）"
            );
        }
        assert!(keys.iter().any(|key| key == "initiating_chat_turn"));
    }

    /// §5.2：阶段重试是**新的运行尝试**，因此旧尝试的许可/审批/动作不得被继承。
    #[test]
    fn phase_retry_is_a_different_attempt_so_old_permissions_are_not_inherited() {
        let first = goal_phase_context("cu-run-1");
        let same = goal_phase_context("cu-run-1");
        let retried = goal_phase_context("cu-run-2");
        assert!(first.is_same_attempt_as(&same), "同一阶段运行视为同一尝试");
        assert!(
            !first.is_same_attempt_as(&retried),
            "阶段重试必须被识别为不同尝试，否则新尝试会继承旧许可"
        );
    }

    /// §5.3：**未知上下文不得被默认解释成受信面**（旧读者遇到未来变体必须报错）。
    #[test]
    fn unknown_or_future_context_kind_never_deserializes_into_a_trusted_one() {
        for raw in [
            r#"{"goal_phase_v2":{"hierarchy":"step_action"}}"#,
            r#"{"some_future_context":{}}"#,
            r#"{"conversation_v2":{}}"#,
        ] {
            assert!(
                serde_json::from_str::<ActionContext>(raw).is_err(),
                "未知上下文变体 `{raw}` 必须报错，不得兜底成任何已受信上下文"
            );
        }
        for raw in [r#""future_kind""#, r#""goal_phase_v2""#] {
            assert!(
                serde_json::from_str::<ContextKind>(raw).is_err(),
                "未知上下文种类 {raw} 必须报错"
            );
        }
        assert_eq!(
            serde_json::from_str::<ContextKind>(r#""goal_phase""#).expect("已知种类"),
            ContextKind::GoalPhase
        );
    }

    /// **强父关系必须由宿主核对**：在宿主具备该能力之前，Goal 阶段来源一律进不来；
    /// 且模型规划动作的义务不因换上下文而免除。
    #[test]
    fn goal_phase_admission_fails_closed_until_the_host_can_verify_the_parent_run() {
        let authority = trusted_context();
        let mut origin = empty_origin("action-1", ActionSource::ModelPlanned);
        origin.context = ActionContext::GoalPhase(goal_phase_context("cu-run-1"));
        origin.request_attempt_id = Some(planned_attempt().stable_key());
        origin.action_id = "action-1".to_string();
        let refusal = admit_action_origin(&origin, &authority)
            .expect_err("宿主尚不能核对阶段运行 ⇒ 不得受理");
        assert_eq!(refusal.code, "action_source_not_established");
        assert!(
            refusal.message.contains("phase-implement") || refusal.message.contains("goal-1"),
            "拒绝理由必须点名缺的是哪条强父关系：{}",
            refusal.message
        );

        // 模型规划动作仍必须携带真实规划 attempt —— 换上下文不免除该义务。
        let mut without_attempt = origin.clone();
        without_attempt.request_attempt_id = None;
        assert!(
            without_attempt.validate_structure().is_err(),
            "Goal 阶段的模型规划动作仍必须有真实规划 attempt"
        );
    }
}
