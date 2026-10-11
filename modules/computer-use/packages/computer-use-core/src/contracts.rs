use runtime::{ActionReceipt, InputDelivery, InputReleaseStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseSurface {
    #[default]
    Auto,
    Desktop,
    Browser,
}

impl ComputerUseSurface {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Desktop => "desktop",
            Self::Browser => "browser",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerUseTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerUseRequest {
    pub objective: String,
    #[serde(default)]
    pub surface: ComputerUseSurface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ComputerUseTarget>,
    pub success_criteria: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    /// 单次请求的动作尝试上限，只能收紧宿主预算；一次连续拖动计一个动作。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_actions: Option<usize>,
}

impl ComputerUseRequest {
    pub fn validate(&self) -> Result<(), ComputerUseError> {
        if self.max_actions == Some(0) {
            return Err(ComputerUseError::blocked(
                "invalid_max_actions",
                "max_actions must be at least 1",
                ComputerUseRetryOwner::Model,
            ));
        }
        if self.objective.trim().is_empty() {
            return Err(ComputerUseError::blocked(
                "invalid_objective",
                "objective is empty",
                ComputerUseRetryOwner::Model,
            ));
        }
        if self.success_criteria.is_empty()
            || self
                .success_criteria
                .iter()
                .any(|criterion| criterion.trim().is_empty())
        {
            return Err(ComputerUseError::blocked(
                "invalid_success_criteria",
                "at least one non-empty success criterion is required",
                ComputerUseRetryOwner::Model,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseRunState {
    Requested,
    Classified,
    Observing,
    Planning,
    PolicyCheck,
    AwaitingApproval,
    Executing,
    Verifying,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    TimedOut,
}

impl ComputerUseRunState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Blocked | Self::Cancelled | Self::TimedOut
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseTerminalStatus {
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    TimedOut,
}

impl ComputerUseTerminalStatus {
    #[must_use]
    pub const fn is_error(self) -> bool {
        !matches!(self, Self::Succeeded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseStage {
    IntentGuard,
    Classification,
    Observation,
    Planning,
    PolicyCheck,
    Approval,
    Execution,
    Verification,
    Supervisor,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseRetryOwner {
    Controller,
    Model,
    User,
    System,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerUseError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub retry_owner: ComputerUseRetryOwner,
    /// 失败发生时执行 helper 已经给出的动作回执；`None` 表示没有事实，不能猜测。
    ///
    /// 旧 JSON 缺少该键时按 `None` 读取，保证向后兼容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ActionReceipt>,
    /// helper 层的有界收尾事实；`None` 表示这次失败没有进入取消/异常收尾。
    ///
    /// 旧 JSON 缺少该键时按 `None` 读取，保证向后兼容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<crate::cleanup::HelperCleanupFacts>,
}

impl ComputerUseError {
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        retry_owner: ComputerUseRetryOwner,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            retry_owner,
            receipt: None,
            cleanup: None,
        }
    }

    #[must_use]
    pub fn blocked(
        code: impl Into<String>,
        message: impl Into<String>,
        retry_owner: ComputerUseRetryOwner,
    ) -> Self {
        Self::new(code, message, false, retry_owner)
    }

    #[must_use]
    pub fn recoverable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, message, true, ComputerUseRetryOwner::Controller)
    }

    /// 附加执行 helper 已给出的动作回执，不改动错误分类本身。
    #[must_use]
    pub fn with_receipt(mut self, receipt: ActionReceipt) -> Self {
        self.receipt = Some(receipt);
        self
    }

    /// 只读访问附带的回执；没有事实时返回 `None`。
    #[must_use]
    pub fn receipt(&self) -> Option<&ActionReceipt> {
        self.receipt.as_ref()
    }

    /// 附加 helper 层的有界收尾事实，不改动错误分类本身。
    #[must_use]
    pub fn with_cleanup(mut self, cleanup: crate::cleanup::HelperCleanupFacts) -> Self {
        self.cleanup = Some(cleanup);
        self
    }

    /// 只读访问收尾事实；这次失败没有进入取消/异常收尾时返回 `None`。
    #[must_use]
    pub fn cleanup(&self) -> Option<&crate::cleanup::HelperCleanupFacts> {
        self.cleanup.as_ref()
    }

    /// 回执是否属于给定的动作身份。
    ///
    /// 身份不匹配的回执不能被接纳到别的动作；这里把它当成"没有可用事实"。
    #[must_use]
    pub fn receipt_matches(&self, current_action_id: &str) -> bool {
        self.receipt
            .as_ref()
            .is_some_and(|receipt| receipt.action_id == current_action_id)
    }

    /// 回执是否表明"输入可能已经发出"。
    ///
    /// 身份不匹配或自相矛盾的回执一律按最保守的方向处理（当作可能已发送），
    /// 而不是因为读不懂就退回"零输入"。
    ///
    /// CU-F01：`NotSent` 还必须带着 **`input_release = NotNeeded`** 才算"没开始输入"。
    /// 旧记录里存在 `NotSent + Unknown` 这种形状（零输入的结论与未结清的释放义务并存），
    /// 它**不是**零输入证明——按"可能已发送"复核，而不是把历史记录当成放行依据。
    #[must_use]
    pub fn receipt_shows_input_may_have_been_sent(&self, current_action_id: &str) -> bool {
        self.receipt.as_ref().is_some_and(|receipt| {
            receipt.action_id != current_action_id
                || receipt.validate().is_err()
                || receipt.input_delivery != InputDelivery::NotSent
                || receipt.input_release != InputReleaseStatus::NotNeeded
        })
    }
}

/// 一次动作尝试的稳定身份：由受信执行链按动作内容计算，模型 JSON 无法改写。
///
/// 回执必须带着它进入消费者：身份不匹配属于协议异常，不得把事实接纳到别的动作，
/// 也不得丢掉异常后报告"零输入"。指纹是内容指纹（FNV-1a 64），跨进程/跨版本可复算。
#[must_use]
pub fn action_attempt_id(surface: ComputerUseSurface, action: &ComputerUseAction) -> String {
    let payload = serde_json::to_string(action).unwrap_or_default();
    format!(
        "{}:{}:{:016x}",
        surface.as_str(),
        action.target,
        fnv1a64(payload.as_bytes())
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerUseBudgets {
    pub max_actions: usize,
    pub max_replans: usize,
    pub max_same_signature: usize,
    pub max_no_progress_steps: usize,
    pub timeout_ms: u64,
    pub max_calls_per_turn: usize,
}

impl Default for ComputerUseBudgets {
    fn default() -> Self {
        Self {
            max_actions: 12,
            max_replans: 2,
            max_same_signature: 2,
            max_no_progress_steps: 2,
            timeout_ms: 120_000,
            max_calls_per_turn: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerUseCapabilities {
    pub navigate: bool,
    pub click: bool,
    pub double_click: bool,
    pub text_input: bool,
    pub select: bool,
    pub check: bool,
    pub submit: bool,
    pub scroll: bool,
    pub history: bool,
    pub drag: bool,
    pub slider_drag: bool,
    pub key_combinations: bool,
    pub multiple_tabs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseActionKind {
    Navigate,
    Click,
    DoubleClick,
    TextInput,
    Select,
    Check,
    Submit,
    Scroll,
    HistoryBack,
    HistoryForward,
    Drag,
    SliderDrag,
    KeyCombination,
    OpenTab,
    ActivateTab,
    CloseTab,
}

impl ComputerUseCapabilities {
    #[must_use]
    pub const fn supports(self, action: ComputerUseActionKind) -> bool {
        match action {
            ComputerUseActionKind::Navigate => self.navigate,
            ComputerUseActionKind::Click => self.click,
            ComputerUseActionKind::DoubleClick => self.double_click,
            ComputerUseActionKind::TextInput => self.text_input,
            ComputerUseActionKind::Select => self.select,
            ComputerUseActionKind::Check => self.check,
            ComputerUseActionKind::Submit => self.submit,
            ComputerUseActionKind::Scroll => self.scroll,
            ComputerUseActionKind::HistoryBack | ComputerUseActionKind::HistoryForward => {
                self.history
            }
            ComputerUseActionKind::Drag => self.drag,
            ComputerUseActionKind::SliderDrag => self.slider_drag,
            ComputerUseActionKind::KeyCombination => self.key_combinations,
            ComputerUseActionKind::OpenTab
            | ComputerUseActionKind::ActivateTab
            | ComputerUseActionKind::CloseTab => self.multiple_tabs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseRiskClass {
    Observe,
    ReversibleLocal,
    Stateful,
    Sensitive,
    ForbiddenOrAmbiguous,
}

impl ComputerUseRiskClass {
    /// 返回该风险等级是否必须由宿主明确批准后才能执行输入动作。
    ///
    /// 模型给出的风险等级只能触发更严格的保护，不能替代宿主审批。
    #[must_use]
    pub const fn requires_explicit_approval(self) -> bool {
        matches!(
            self,
            Self::Stateful | Self::Sensitive | Self::ForbiddenOrAmbiguous
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerUseAction {
    pub kind: ComputerUseActionKind,
    pub target: String,
    #[serde(default)]
    pub arguments: Value,
    pub risk: ComputerUseRiskClass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub generation: u64,
    pub surface: ComputerUseSurface,
    pub surface_identity: String,
    #[serde(default)]
    pub state: Value,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// 执行 helper 对输入释放的可观察回执；`None` 表示 helper 没有给出事实，不能猜测。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepInputReleaseStatus {
    NotNeeded,
    Released,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StepExecution {
    pub input_sent: bool,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    /// 仅 helper 明确报告时写入；`sent` 本身不代表完整完成。
    ///
    /// 下列四个字段是 `receipt` 的投影（兼容旧消费者）；当 `receipt` 存在时，
    /// 它们必须与该回执一致，避免"平行字段"各说一套。
    #[serde(default)]
    pub partial: Option<bool>,
    /// 只适用于路径输入，非路径动作和未知均为 `None`。
    #[serde(default)]
    pub path_completed: Option<bool>,
    /// 已确认注入的路径采样点数量，不把 down/up 混入其中。
    #[serde(default)]
    pub confirmed_point_count: Option<u32>,
    /// helper 可确认的自有输入释放状态。
    #[serde(default)]
    pub input_release_status: Option<StepInputReleaseStatus>,
    /// 本次成功执行的动作回执（受信执行链产出）；身份由 `action_attempt_id` 决定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ActionReceipt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verification {
    pub achieved: bool,
    pub visible_progress: bool,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorSnapshot {
    pub circuit_open: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub action_count: usize,
    pub replan_count: usize,
    pub no_progress_count: usize,
}

/// 已保存步骤的输入事实；投递、释放和效果分别表达，不由任务终态推断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerUseInputStep {
    pub step_index: usize,
    pub action_kind: Option<ComputerUseActionKind>,
    pub input_delivery: Option<InputDelivery>,
    pub input_release_status: Option<InputReleaseStatus>,
    pub partial: Option<bool>,
    pub effect_status: Option<runtime::EffectStatus>,
    pub goal_verdict: Option<runtime::GoalVerdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerUseResult {
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_tool_call_id: Option<String>,
    pub status: ComputerUseTerminalStatus,
    pub stage: ComputerUseStage,
    pub goal_achieved: bool,
    pub surface: ComputerUseSurface,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ComputerUseError>,
    pub attempts: usize,
    pub steps_completed: usize,
    /// 宿主从同一运行的步骤记录投影。None表示未取得事实，空数组表示已读取且无步骤。
    /// 兼容旧回执；不含节点、输入正文、网址、图片或推测的跨进程时序。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_steps: Option<Vec<ComputerUseInputStep>>,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub supervisor: SupervisorSnapshot,
    /// CU 级预算事实：根 deadline 的接线状态、真实的 CU 截止时间。
    ///
    /// 旧 JSON 缺少该键时按 `None` 读取；运行记录必须能分别表达
    /// `root_deadline_state = not_wired`、`root_deadline = absent` 与真实的 `cu_deadline`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cu_budget: Option<crate::budget::CuBudgetFacts>,
    /// 收尾报告：业务截止时刻、停止新业务输入时刻、收尾起止时刻、释放状态、是否隔离
    /// 分开记录。没有进入取消/异常收尾时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<crate::cleanup::CleanupReport>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus};

    fn valid_request_json() -> serde_json::Value {
        serde_json::json!({
            "objective": "打开记事本",
            "surface": "desktop",
            "target": { "application": "notepad" },
            "success_criteria": ["记事本窗口可见"],
            "constraints": ["不要关闭已有窗口"]
        })
    }

    #[test]
    fn task_request_accepts_task_level_contract() {
        let request: ComputerUseRequest =
            serde_json::from_value(valid_request_json()).expect("valid task request");

        assert_eq!(request.surface, ComputerUseSurface::Desktop);
        assert_eq!(
            request
                .target
                .as_ref()
                .and_then(|target| target.application.as_deref()),
            Some("notepad")
        );
        assert!(request.validate().is_ok());
        assert_eq!(request.max_actions, None);
        let mut bounded = request;
        bounded.max_actions = Some(1);
        assert!(bounded.validate().is_ok());
        bounded.max_actions = Some(0);
        assert_eq!(bounded.validate().unwrap_err().code, "invalid_max_actions");
    }

    #[test]
    fn task_request_rejects_coordinates_and_requires_success_criteria() {
        let request: ComputerUseRequest = serde_json::from_value(serde_json::json!({
            "objective": "打开记事本",
            "surface": "desktop",
            "success_criteria": []
        }))
        .expect("schema is valid before semantic validation");
        assert_eq!(
            request
                .validate()
                .expect_err("empty criteria must fail")
                .code,
            "invalid_success_criteria"
        );

        assert!(
            serde_json::from_value::<ComputerUseRequest>(serde_json::json!({
                "objective": "点击",
                "x": 10,
                "y": 20,
                "success_criteria": ["窗口已打开"]
            }))
            .is_err()
        );
    }

    #[test]
    fn task_request_rejects_empty_objective_and_target_extensions() {
        let request: ComputerUseRequest = serde_json::from_value(serde_json::json!({
            "objective": "   ",
            "success_criteria": ["完成"]
        }))
        .expect("schema is valid before semantic validation");
        assert_eq!(
            request
                .validate()
                .expect_err("empty objective must fail")
                .code,
            "invalid_objective"
        );

        let mut json = valid_request_json();
        json["target"]["coordinates"] = serde_json::json!([1, 2]);
        assert!(serde_json::from_value::<ComputerUseRequest>(json).is_err());
    }

    #[test]
    fn awaiting_approval_is_running_not_terminal() {
        assert!(!ComputerUseRunState::AwaitingApproval.is_terminal());
        assert!(ComputerUseRunState::Succeeded.is_terminal());
        assert!(ComputerUseRunState::Failed.is_terminal());
        assert!(ComputerUseRunState::Blocked.is_terminal());
        assert!(ComputerUseRunState::Cancelled.is_terminal());
        assert!(ComputerUseRunState::TimedOut.is_terminal());
    }

    #[test]
    fn risky_actions_require_explicit_host_approval() {
        assert!(!ComputerUseRiskClass::Observe.requires_explicit_approval());
        assert!(!ComputerUseRiskClass::ReversibleLocal.requires_explicit_approval());
        assert!(ComputerUseRiskClass::Stateful.requires_explicit_approval());
        assert!(ComputerUseRiskClass::Sensitive.requires_explicit_approval());
        assert!(ComputerUseRiskClass::ForbiddenOrAmbiguous.requires_explicit_approval());
    }

    #[test]
    fn terminal_status_serializes_as_stable_snake_case() {
        assert_eq!(
            serde_json::to_string(&ComputerUseTerminalStatus::TimedOut).unwrap(),
            "\"timed_out\""
        );
        assert_eq!(
            serde_json::to_string(&ComputerUseTerminalStatus::Succeeded).unwrap(),
            "\"succeeded\""
        );
    }

    #[test]
    fn default_budgets_match_runtime_safety_contract() {
        let budgets = ComputerUseBudgets::default();
        assert_eq!(budgets.max_actions, 12);
        assert_eq!(budgets.max_replans, 2);
        assert_eq!(budgets.max_same_signature, 2);
        assert_eq!(budgets.max_no_progress_steps, 2);
        assert_eq!(budgets.timeout_ms, 120_000);
        assert_eq!(budgets.max_calls_per_turn, 2);
    }

    #[test]
    fn multiple_tab_capability_gates_all_tab_lifecycle_actions() {
        let disabled = ComputerUseCapabilities::default();
        for action in [
            ComputerUseActionKind::OpenTab,
            ComputerUseActionKind::ActivateTab,
            ComputerUseActionKind::CloseTab,
        ] {
            assert!(!disabled.supports(action));
        }

        let enabled = ComputerUseCapabilities {
            multiple_tabs: true,
            ..ComputerUseCapabilities::default()
        };
        for action in [
            ComputerUseActionKind::OpenTab,
            ComputerUseActionKind::ActivateTab,
            ComputerUseActionKind::CloseTab,
        ] {
            assert!(enabled.supports(action));
        }
    }

    #[test]
    fn contracts_are_exported_from_crate_root() {
        let request: crate::ComputerUseRequest =
            serde_json::from_value(valid_request_json()).expect("root export");
        assert_eq!(request.surface, crate::ComputerUseSurface::Desktop);
    }

    /// 构造一个事实齐全的回执，用于错误携带回执的往返测试。
    fn sample_receipt() -> ActionReceipt {
        ActionReceipt {
            action_id: "act-7".to_string(),
            input_delivery: InputDelivery::MayHaveBeenSent,
            partial: Some(true),
            path_completed: Some(false),
            confirmed_point_count: Some(3),
            effect: EffectStatus::Inconclusive,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::Released,
        }
    }

    #[test]
    fn legacy_error_json_without_receipt_reads_as_none() {
        let legacy = serde_json::json!({
            "code": "desktop_link_unavailable",
            "message": "桌面通道不可用",
            "retryable": false,
            "retry_owner": "user"
        });
        let error: ComputerUseError =
            serde_json::from_value(legacy).expect("旧 JSON 必须仍可读取");
        assert_eq!(error.code, "desktop_link_unavailable");
        assert_eq!(error.retry_owner, ComputerUseRetryOwner::User);
        assert!(!error.retryable);
        assert!(error.receipt().is_none(), "缺键必须读成 None，不能猜测");
        assert!(error.receipt.is_none());
    }

    #[test]
    fn error_with_receipt_round_trips_identity_and_facts() {
        let receipt = sample_receipt();
        receipt.validate().expect("样例回执自身必须自洽");
        let error = ComputerUseError::blocked(
            "input_partially_injected",
            "输入可能只注入了一部分",
            ComputerUseRetryOwner::Model,
        )
        .with_receipt(receipt.clone());

        let encoded = serde_json::to_string(&error).expect("序列化失败");
        let decoded: ComputerUseError =
            serde_json::from_str(&encoded).expect("反序列化失败");

        assert_eq!(decoded.code, error.code);
        assert_eq!(decoded.message, error.message);
        assert_eq!(decoded.retryable, error.retryable);
        assert_eq!(decoded.retry_owner, error.retry_owner);
        assert_eq!(decoded.receipt(), Some(&receipt));
        assert_eq!(decoded, error, "整条错误必须完整往返");
    }

    #[test]
    fn error_without_receipt_omits_receipt_key() {
        let error = ComputerUseError::recoverable("stale_observation", "观测已过期");
        assert!(error.receipt().is_none());

        let value = serde_json::to_value(&error).expect("序列化失败");
        let object = value.as_object().expect("错误必须序列化为对象");
        assert!(
            !object.contains_key("receipt"),
            "receipt=None 时不得写出 receipt 键，实际输出：{value}"
        );
    }
}
