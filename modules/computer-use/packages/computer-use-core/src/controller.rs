use crate::{
    ActionFingerprint, CleanupReleaseStatus, CleanupReport, ComputerUseAction, ComputerUseBudgets,
    ComputerUseCapabilities, ComputerUseError, ComputerUseRequest, ComputerUseResult,
    ComputerUseRetryOwner, ComputerUseRunState, ComputerUseStage, ComputerUseSurface,
    ComputerUseTerminalStatus, CuBudgetFacts, CuDeadline, Observation, RunBudgetGuard,
    StepExecution, Verification,
};
use serde_json::Value as JsonValue;

pub type PlannerFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// 保留过去一次宿主验证，不能把部分满足或旧观察改为终态成功。
fn retain_last_verification(mut result: ComputerUseResult, summary: Option<String>) -> ComputerUseResult {
    if let Some(summary) = summary {
        result.summary = serde_json::json!({"terminal_reason":result.summary,
            "last_verification":serde_json::from_str::<JsonValue>(&summary).unwrap_or(JsonValue::String(summary)),
            "verification_is_last_observation":true}).to_string();
    }
    result
}

/// 规划者：所有可能直接或间接发起模型调用的方法都**必须**接收剩余预算。
///
/// `remaining` 与本 crate 的 [`ComputerUseAdapter`] 使用同一个类型与单位
/// （`std::time::Duration`，单调 deadline 的剩余量），不允许再造毫秒/秒/时长的第二套约定。
///
/// 实现方必须遵守：
///
/// 1. **不得**忽略 `remaining`（本 trait 刻意不为任何方法提供"为了兼容而忽略 remaining"
///    的默认实现）；
/// 2. 输入为零或不足以开始该阶段时返回**预算不足**，且模型 HTTP 请求次数必须为**零**；
/// 3. 一次调用内部有多个子请求时必须**连续扣减**：第二个请求拿到的是第一个请求消耗后的
///    余量，不能各自拿到入口时的同一份完整 remaining。
pub trait ComputerUsePlanner: Send + Sync {
    fn classify<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        observation: &'a Observation,
        remaining: std::time::Duration,
    ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>>;

    fn next_action<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        observation: &'a Observation,
        step: usize,
        remaining: std::time::Duration,
    ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>>;

    /// 宿主可用最新视觉证据复核适配器结果。
    ///
    /// 该方法是模型请求的入口之一（`ComputerUseSurface::Desktop` 上的每次验收都会发请求），
    /// 因此同样必须收紧到 `remaining` 以内；本 trait **不提供**忽略 remaining 的默认实现。
    fn verify<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        before: &'a Observation,
        after: &'a Observation,
        verification: Verification,
        remaining: std::time::Duration,
    ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>>;

    /// 最近一次**产生执行计划**的模型请求 attempt（宿主因果元数据）。
    ///
    /// 执行器在写动作事实时用它构造 `ActionOrigin` 的模型规划来源；因此这里给出的必须是
    /// **真实规划请求**的身份。默认返回 `None` 表示"不知道"——**不得**用 provider trace、
    /// 外层工具调用的 call id 或"最近一次请求"顶替（第三轮裁决第 14.6 项：接口上已有 attempt
    /// 概念，不代表可以用近似物冒充）。
    fn last_plan_request_attempt(&self) -> Option<runtime::PlannedRequestAttempt> {
        None
    }
}

pub trait ComputerUseAdapter: Send + Sync {
    fn surface(&self) -> ComputerUseSurface;
    fn capabilities(&self) -> ComputerUseCapabilities;
    /// `remaining` 是本 run 单调 deadline 的剩余预算；实现方**必须**用它收紧自己的
    /// 子请求超时上限（§2.3「子请求上限不超过剩余预算」），不得用固定常量硬顶。
    fn observe(
        &self,
        request: &ComputerUseRequest,
        remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError>;
    fn act(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError>;
    /// 桌面实现必须把授权端口传给原生 helper；浏览器及内存适配器仍使用各自的执行路径。
    fn act_authorized(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
        _authorization: &dyn crate::prepared_input::NativeInputAuthorization,
    ) -> Result<StepExecution, ComputerUseError> {
        self.act(action, expected_generation, remaining)
    }
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError>;
}

pub trait ComputerUseEventSink {
    fn state_changed(&mut self, state: ComputerUseRunState);
}

pub trait ComputerUseClock {
    fn now_ms(&mut self) -> u64;
}

/// 由宿主提供的动作审批策略。
///
/// 策略只在动作风险要求明确审批时调用。实现方应将批准绑定到当前用户、
/// 会话和具体动作，不能直接信任模型输出的风险等级或批准声明。
///
/// 宿主语义分类器识别出的删除、支付、外发、授权安装、凭据等敏感动作不会
/// 交给这一通用策略放行；它们必须由更窄、可恢复且绑定具体动作的审批流程处理。
pub trait ComputerUseApprovalPolicy: Send + Sync {
    fn is_action_approved(
        &self,
        request: &ComputerUseRequest,
        action: &ComputerUseAction,
        observation: &Observation,
    ) -> bool;
}

impl<F> ComputerUseApprovalPolicy for F
where
    F: Fn(&ComputerUseRequest, &ComputerUseAction, &Observation) -> bool + Send + Sync,
{
    fn is_action_approved(
        &self,
        request: &ComputerUseRequest,
        action: &ComputerUseAction,
        observation: &Observation,
    ) -> bool {
        self(request, action, observation)
    }
}

fn host_sensitive_semantic_category(
    request: &ComputerUseRequest,
    action: &ComputerUseAction,
    observation: &Observation,
) -> Option<&'static str> {
    let mut corpus = String::new();
    let objective = normalize_objective_preapproval_claim(&request.objective);
    push_bounded_semantic_text(&mut corpus, &normalize_objective_input_delivery(&objective));
    push_bounded_semantic_text(&mut corpus, &action.target);

    let mut remaining_argument_strings = 32usize;
    collect_argument_strings(
        &action.arguments,
        &mut corpus,
        &mut remaining_argument_strings,
    );
    collect_target_node_text(&observation.state, &action.target, &mut corpus);

    classify_sensitive_semantics(&corpus)
}

/// 开头的既有许可声明不是“执行授权”；只改前缀动词，任务、目标和参数仍完整审查。
/// 该规整不生成批准，也不能豁免声明之后的安装、授权、删除或外发动作。
fn normalize_objective_preapproval_claim(text: &str) -> String {
    let text = text.trim_start();
    for (claim, neutral) in [
        ("用户已明确授权本次", "用户已明确许可本次"),
        ("用户已授权本次", "用户已许可本次"),
        ("用户已明确授权本轮", "用户已明确许可本轮"),
        ("用户已授权本轮", "用户已许可本轮"),
    ] {
        if let Some(rest) = text.strip_prefix(claim) {
            return format!("{neutral}{rest}");
        }
    }
    text.to_owned()
}

/// 意图中“发送一次普通点击”描述输入投递，不是外发消息；目标节点和动作参数仍原样审查。
/// 仅识别句尾/分隔符前的完整机械输入宾语，“发送一次点击结果”等外发宾语不得豁免。
fn normalize_objective_input_delivery(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(offset) = rest.find("发送") {
        normalized.push_str(&rest[..offset]);
        let following = &rest[offset + "发送".len()..];
        let mechanical = ["一次普通点击", "一次鼠标点击", "一次点击", "鼠标输入", "键盘输入", "任何输入", "输入"]
            .iter().any(|object| following.strip_prefix(object).is_some_and(|tail|
                tail.trim_start().chars().next().is_none_or(|c| "，。；：,.;:!?！？)）".contains(c))));
        normalized.push_str(if mechanical { "执行" } else { "发送" });
        rest = following;
    }
    normalized.push_str(rest);
    normalized
}

fn push_bounded_semantic_text(corpus: &mut String, text: &str) {
    const MAX_CORPUS_BYTES: usize = 32 * 1024;
    const MAX_FIELD_CHARS: usize = 2_048;
    if corpus.len() >= MAX_CORPUS_BYTES {
        return;
    }
    corpus.push(' ');
    for character in text.chars().take(MAX_FIELD_CHARS) {
        if corpus.len().saturating_add(character.len_utf8()) > MAX_CORPUS_BYTES {
            break;
        }
        corpus.push(character);
    }
}

fn collect_argument_strings(value: &JsonValue, corpus: &mut String, remaining: &mut usize) {
    if *remaining == 0 {
        return;
    }
    match value {
        JsonValue::String(text) => {
            *remaining = remaining.saturating_sub(1);
            push_bounded_semantic_text(corpus, text);
        }
        JsonValue::Array(values) => {
            for value in values {
                collect_argument_strings(value, corpus, remaining);
                if *remaining == 0 {
                    break;
                }
            }
        }
        JsonValue::Object(object) => {
            for (key, value) in object {
                push_bounded_semantic_text(corpus, key);
                collect_argument_strings(value, corpus, remaining);
                if *remaining == 0 {
                    break;
                }
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => {}
    }
}

fn collect_target_node_text(state: &JsonValue, target: &str, corpus: &mut String) {
    const REFERENCE_FIELDS: &[&str] = &["reference", "ref", "id"];
    const VISIBLE_TEXT_FIELDS: &[&str] = &[
        "name",
        "label",
        "text",
        "title",
        "description",
        "accessible_name",
        "aria_label",
        "automation_id",
        "control_type",
        "role",
        "tag",
        "input_type",
        "value",
    ];

    match state {
        JsonValue::Array(values) => {
            for value in values {
                collect_target_node_text(value, target, corpus);
            }
        }
        JsonValue::Object(object) => {
            let target_matches = REFERENCE_FIELDS
                .iter()
                .any(|field| object.get(*field).and_then(JsonValue::as_str) == Some(target));
            if target_matches {
                for field in VISIBLE_TEXT_FIELDS {
                    if let Some(text) = object.get(*field).and_then(JsonValue::as_str) {
                        push_bounded_semantic_text(corpus, text);
                    }
                }
                return;
            }
            for value in object.values() {
                collect_target_node_text(value, target, corpus);
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_) => {}
    }
}

fn classify_sensitive_semantics(corpus: &str) -> Option<&'static str> {
    let lower = corpus.to_lowercase();
    let ascii_tokens = lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let contains_word = |words: &[&str]| words.iter().any(|word| ascii_tokens.contains(word));
    let contains_phrase = |phrase: &[&str]| {
        ascii_tokens
            .windows(phrase.len())
            .any(|window| window == phrase)
    };
    let contains_text = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));

    if contains_text(&[
        "删除",
        "刪除",
        "移除",
        "销毁",
        "銷毀",
        "注销账号",
        "註銷帳號",
    ]) || contains_word(&["delete", "remove", "erase", "destroy", "wipe"])
    {
        return Some("destructive_change");
    }
    if contains_text(&[
        "购买", "購買", "付款", "支付", "下单", "下單", "结账", "結賬", "订阅", "訂閱", "转账",
        "轉賬", "充值", "提现", "提現",
    ]) || contains_word(&[
        "buy",
        "purchase",
        "pay",
        "checkout",
        "subscribe",
        "transfer",
        "withdraw",
    ]) || contains_phrase(&["place", "order"])
    {
        return Some("purchase_or_payment");
    }
    if contains_text(&[
        "发送", "發送", "发布", "發布", "发帖", "發帖", "推送", "分享",
    ]) || contains_word(&["send", "publish", "post", "share"])
    {
        return Some("external_communication");
    }
    if contains_text(&[
        "授权",
        "授權",
        "允许访问",
        "允許存取",
        "安装",
        "安裝",
        "卸载",
        "解除安裝",
    ]) || contains_word(&["authorize", "install", "uninstall"])
        || contains_phrase(&["grant", "access"])
        || contains_phrase(&["allow", "access"])
    {
        return Some("authorization_or_installation");
    }
    if contains_text(&[
        "密码",
        "密碼",
        "口令",
        "密钥",
        "密鑰",
        "私钥",
        "私鑰",
        "助记词",
        "助記詞",
        "恢复码",
        "恢復碼",
    ]) || contains_word(&[
        "password",
        "passcode",
        "credential",
        "credentials",
        "otp",
        "secret",
    ]) || contains_phrase(&["api", "key"])
        || contains_phrase(&["private", "key"])
        || contains_phrase(&["secret", "key"])
        || contains_phrase(&["access", "key"])
        || contains_phrase(&["api", "token"])
        || contains_phrase(&["access", "token"])
        || contains_phrase(&["auth", "token"])
        || contains_phrase(&["recovery", "code"])
        || contains_phrase(&["seed", "phrase"])
    {
        return Some("credential_or_secret");
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerUseRunContext {
    pub call_id: String,
    pub provider_tool_call_id: Option<String>,
}

pub struct ComputerUseController<P, A, E, C> {
    planner: P,
    adapter: A,
    events: E,
    clock: C,
    budgets: ComputerUseBudgets,
    approval_policy: Option<Box<dyn ComputerUseApprovalPolicy>>,
    /// 由宿主在任务**被接纳并进入调度**时建立的唯一 CU 截止时间。
    ///
    /// 未显式设置时，控制器退化为"在 `run` 入口建立"，此时租约等待/模型切换的耗时
    /// 不在预算内——这是宿主尚未接线的情形，不是"没有预算"。
    cu_deadline: Option<CuDeadline>,
}

impl<P, A, E, C> ComputerUseController<P, A, E, C>
where
    P: ComputerUsePlanner,
    A: ComputerUseAdapter,
    E: ComputerUseEventSink,
    C: ComputerUseClock,
{
    #[must_use]
    pub fn new(planner: P, adapter: A, events: E, clock: C, budgets: ComputerUseBudgets) -> Self {
        Self {
            planner,
            adapter,
            events,
            clock,
            budgets,
            approval_policy: None,
            cu_deadline: None,
        }
    }

    /// 接收宿主在任务被接纳时建立的 CU 截止时间。
    ///
    /// 建立点必须早于该任务的租约等待、模型切换、初始观察与规划请求，否则这些耗时
    /// 会从预算里消失。此后观察、规划、验收、重试、重规划与受限恢复共用它。
    #[must_use]
    pub fn with_cu_deadline(mut self, deadline: CuDeadline) -> Self {
        self.cu_deadline = Some(deadline);
        self
    }

    /// 设置宿主审批策略。未设置时，所有需要明确审批的动作都会在输入前阻断。
    #[must_use]
    pub fn with_approval_policy<T>(mut self, approval_policy: T) -> Self
    where
        T: ComputerUseApprovalPolicy + 'static,
    {
        self.approval_policy = Some(Box::new(approval_policy));
        self
    }

    #[must_use]
    pub const fn adapter(&self) -> &A {
        &self.adapter
    }

    #[must_use]
    pub const fn planner(&self) -> &P {
        &self.planner
    }

    #[must_use]
    pub const fn event_sink(&self) -> &E {
        &self.events
    }

    pub async fn run(
        &mut self,
        request: &ComputerUseRequest,
        context: ComputerUseRunContext,
    ) -> ComputerUseResult {
        self.events.state_changed(ComputerUseRunState::Requested);
        let run_facts = self.run_facts();
        if let Err(error) = request.validate() {
            return self.terminal(
                &context,
                request.surface,
                ComputerUseStage::IntentGuard,
                error,
                0,
                0,
                Vec::new(),
                crate::SupervisorSnapshot::default(),
                &run_facts,
            );
        }

        let mut budgets = self.budgets;
        if let Some(max_actions) = request.max_actions {
            budgets.max_actions = budgets.max_actions.min(max_actions);
        }
        let mut guard = RunBudgetGuard::with_deadline(budgets, run_facts.deadline);
        let mut attempts = 0usize;
        let mut steps_completed = 0usize;
        let mut evidence = Vec::new();
        let mut stale_recovered = false;
        let mut last_verification;

        self.events.state_changed(ComputerUseRunState::Observing);
        let remaining = std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
        let mut observation = match self.adapter.observe(request, remaining) {
            Ok(observation) => observation,
            Err(error) => {
                return self.terminal(
                    &context,
                    request.surface,
                    ComputerUseStage::Observation,
                    error,
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
            }
        };
        evidence.extend(observation.evidence.clone());

        let surface = if request.surface == ComputerUseSurface::Auto {
            let remaining =
                std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
            match self.planner.classify(request, &observation, remaining).await {
                Ok(surface) => surface,
                Err(error) => {
                    return self.terminal(
                        &context,
                        ComputerUseSurface::Auto,
                        ComputerUseStage::Classification,
                        error,
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
            }
        } else {
            request.surface
        };
        self.events.state_changed(ComputerUseRunState::Classified);

        if surface == ComputerUseSurface::Auto || surface != self.adapter.surface() {
            return self.terminal(
                &context,
                surface,
                ComputerUseStage::Classification,
                ComputerUseError::blocked(
                    "surface_unavailable",
                    "no adapter is available for the selected surface",
                    ComputerUseRetryOwner::System,
                ),
                attempts,
                steps_completed,
                evidence,
                guard.snapshot(),
                &run_facts,
            );
        }
        if observation.surface != surface {
            return self.terminal(
                &context,
                surface,
                ComputerUseStage::Observation,
                ComputerUseError::recoverable(
                    "surface_mismatch",
                    "observation does not belong to the selected surface",
                ),
                attempts,
                steps_completed,
                evidence,
                guard.snapshot(),
                &run_facts,
            );
        }

        self.events.state_changed(ComputerUseRunState::Verifying);
        let remaining = std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
        let initial_verification =
            match self
                .adapter
                .verify(&request.success_criteria, &observation, &observation, remaining)
            {
                Ok(verification) => {
                    self.planner
                        .verify(request, &observation, &observation, verification, remaining)
                        .await
                }
                Err(error) => Err(error),
            };
        match initial_verification {
            Ok(verification) => {
                evidence.extend(verification.evidence.clone());
                if verification.achieved {
                    return self.success(
                        &context,
                        surface,
                        verification.summary,
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
                last_verification = Some(verification.summary);
            }
            Err(error) => {
                return self.terminal(
                    &context,
                    surface,
                    ComputerUseStage::Verification,
                    error,
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
            }
        }

        loop {
            if let Err(error) = guard.before_planning(self.clock.now_ms()) {
                let result = self.terminal(
                    &context,
                    surface,
                    ComputerUseStage::Supervisor,
                    error,
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
                // 保留最后已取得的验收事实，不能因动作预算耗尽把“部分满足”变成未知的零满足。
                // 这是过去一次观察的报告，不是新的成功证明，也不触发额外模型或输入请求。
                return retain_last_verification(result, last_verification);
            }
            self.events.state_changed(ComputerUseRunState::Planning);
            let remaining =
                std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
            let action = match self
                .planner
                .next_action(request, &observation, attempts, remaining)
                .await
            {
                Ok(Some(action)) => action,
                Ok(None) => {
                    let result = self.terminal(
                        &context,
                        surface,
                        ComputerUseStage::Verification,
                        ComputerUseError::blocked(
                            "verification_failed",
                            "planner stopped before all success criteria were verified",
                            ComputerUseRetryOwner::Model,
                        ),
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                    return retain_last_verification(result, last_verification);
                }
                Err(error) => {
                    return self.terminal(
                        &context,
                        surface,
                        ComputerUseStage::Planning,
                        error,
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
            };

            self.events.state_changed(ComputerUseRunState::PolicyCheck);
            if !self.adapter.capabilities().supports(action.kind) {
                return self.terminal(
                    &context,
                    surface,
                    ComputerUseStage::PolicyCheck,
                    ComputerUseError::blocked(
                        "unsupported_action",
                        format!("adapter does not support {:?}", action.kind),
                        ComputerUseRetryOwner::Model,
                    ),
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
            }

            let semantic_risk = host_sensitive_semantic_category(request, &action, &observation);
            if action.risk.requires_explicit_approval() || semantic_risk.is_some() {
                self.events
                    .state_changed(ComputerUseRunState::AwaitingApproval);
                // full-access 等通用授权只能批准风险等级本身要求审批的普通动作。
                // 宿主从目标节点/参数/意图识别出的敏感语义不能被通用策略覆盖，
                // 防止模型把“删除/支付/发送”等动作伪装成 ReversibleLocal 绕过看护。
                let approved = semantic_risk.is_none()
                    && self.approval_policy.as_ref().is_some_and(|policy| {
                        policy.is_action_approved(request, &action, &observation)
                    });
                if !approved {
                    return self.terminal(
                        &context,
                        surface,
                        ComputerUseStage::Approval,
                        ComputerUseError::blocked(
                            "approval_required",
                            semantic_risk.map_or_else(
                                || {
                                    format!(
                                        "action {:?} with risk {:?} requires explicit host approval",
                                        action.kind, action.risk
                                    )
                                },
                                |category| {
                                    format!(
                                        "action {:?} matches host-sensitive category {category} \
                                         and requires explicit host approval",
                                        action.kind
                                    )
                                },
                            ),
                            ComputerUseRetryOwner::User,
                        ),
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
                self.events.state_changed(ComputerUseRunState::PolicyCheck);
            }

            let arguments = serde_json::to_string(&action.arguments).unwrap_or_default();
            let fingerprint = ActionFingerprint::new(
                surface.as_str(),
                observation.generation,
                &format!("{:?}", action.kind),
                &action.target,
                &arguments,
            );
            if let Err(error) = guard.before_action(&fingerprint, self.clock.now_ms()) {
                return self.terminal(
                    &context,
                    surface,
                    ComputerUseStage::Supervisor,
                    error,
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
            }

            attempts = attempts.saturating_add(1);
            self.events.state_changed(ComputerUseRunState::Executing);
            let remaining = std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
            let execution = match self.adapter.act(&action, observation.generation, remaining) {
                Ok(execution) => execution,
                // 允许一次重新观察的前提是"还没注入输入"。回执若表明输入可能已经发出
                // （含身份不匹配或自相矛盾的协议异常），重新规划等于在未知按键状态下重放输入，
                // 因此直接按终态失败处理，不做静默恢复。
                Err(error)
                    if error.code == "stale_observation"
                        && !stale_recovered
                        && !error.receipt_shows_input_may_have_been_sent(&crate::contracts::action_attempt_id(surface, &action)) =>
                {
                    if let Err(budget_error) = guard.record_replan() {
                        return self.terminal(
                            &context,
                            surface,
                            ComputerUseStage::Supervisor,
                            budget_error,
                            attempts,
                            steps_completed,
                            evidence,
                            guard.snapshot(),
                            &run_facts,
                        );
                    }
                    stale_recovered = true;
                    self.events.state_changed(ComputerUseRunState::Observing);
                    let remaining = std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
                    observation = match self.adapter.observe(request, remaining) {
                        Ok(observation) => observation,
                        Err(observe_error) => {
                            return self.terminal(
                                &context,
                                surface,
                                ComputerUseStage::Observation,
                                observe_error,
                                attempts,
                                steps_completed,
                                evidence,
                                guard.snapshot(),
                                &run_facts,
                            );
                        }
                    };
                    evidence.extend(observation.evidence.clone());
                    continue;
                }
                Err(error) => {
                    return self.terminal(
                        &context,
                        surface,
                        ComputerUseStage::Execution,
                        error,
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
            };

            evidence.extend(execution.evidence.clone());
            if !execution.input_sent {
                return self.terminal(
                    &context,
                    surface,
                    ComputerUseStage::Execution,
                    ComputerUseError::recoverable(
                        "input_not_sent",
                        "adapter returned without sending the requested input",
                    ),
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
            }
            steps_completed = steps_completed.saturating_add(1);

            self.events.state_changed(ComputerUseRunState::Observing);
            let remaining = std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
            let next_observation = match self.adapter.observe(request, remaining) {
                Ok(observation) => observation,
                Err(error) => {
                    return self.terminal(
                        &context,
                        surface,
                        ComputerUseStage::Observation,
                        error,
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
            };
            evidence.extend(next_observation.evidence.clone());

            self.events.state_changed(ComputerUseRunState::Verifying);
            let remaining = std::time::Duration::from_millis(guard.remaining_ms(self.clock.now_ms()));
            let adapter_verification =
                self.adapter
                    .verify(&request.success_criteria, &observation, &next_observation, remaining);
            let checked_verification = match adapter_verification {
                Ok(verification) => {
                    self.planner
                        .verify(request, &observation, &next_observation, verification, remaining)
                        .await
                }
                Err(error) => Err(error),
            };
            let verification = match checked_verification {
                Ok(verification) => verification,
                Err(error) => {
                    return self.terminal(
                        &context,
                        surface,
                        ComputerUseStage::Verification,
                        error,
                        attempts,
                        steps_completed,
                        evidence,
                        guard.snapshot(),
                        &run_facts,
                    );
                }
            };
            evidence.extend(verification.evidence.clone());
            guard.after_verification(verification.visible_progress);
            if verification.achieved {
                return self.success(
                    &context,
                    surface,
                    verification.summary,
                    attempts,
                    steps_completed,
                    evidence,
                    guard.snapshot(),
                    &run_facts,
                );
            }
            observation = next_observation;
            last_verification = Some(verification.summary);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn terminal(
        &mut self,
        context: &ComputerUseRunContext,
        surface: ComputerUseSurface,
        stage: ComputerUseStage,
        error: ComputerUseError,
        attempts: usize,
        steps_completed: usize,
        evidence: Vec<String>,
        supervisor: crate::SupervisorSnapshot,
        run_facts: &RunFacts,
    ) -> ComputerUseResult {
        let status = if error.code == "cancelled" {
            ComputerUseTerminalStatus::Cancelled
        } else if error.code == "deadline_exceeded" {
            ComputerUseTerminalStatus::TimedOut
        } else if !error.retryable {
            ComputerUseTerminalStatus::Blocked
        } else {
            ComputerUseTerminalStatus::Failed
        };
        self.events.state_changed(match status {
            ComputerUseTerminalStatus::Succeeded => ComputerUseRunState::Succeeded,
            ComputerUseTerminalStatus::Failed => ComputerUseRunState::Failed,
            ComputerUseTerminalStatus::Blocked => ComputerUseRunState::Blocked,
            ComputerUseTerminalStatus::Cancelled => ComputerUseRunState::Cancelled,
            ComputerUseTerminalStatus::TimedOut => ComputerUseRunState::TimedOut,
        });
        let stop_observed_at_ms = self.clock.now_ms();
        let cleanup = self.cleanup_report(run_facts, status, stage, &error, stop_observed_at_ms);
        ComputerUseResult {
            call_id: context.call_id.clone(),
            provider_tool_call_id: context.provider_tool_call_id.clone(),
            status,
            stage,
            goal_achieved: false,
            surface,
            summary: error.message.clone(),
            error: Some(error),
            attempts,
            steps_completed,
            input_steps: None,
            evidence,
            supervisor,
            cu_budget: Some(run_facts.cu_budget),
            cleanup,
        }
    }

    /// 收尾报告：把业务截止时刻、停止新业务输入时刻、收尾起止时刻、释放状态与是否隔离
    /// **分开**记录。收尾允许有限超出业务期限，但不得借收尾继续规划或重做任务。
    ///
    /// `release` 的取值只依据可确认的事实：helper 层给出的事实优先，其次看该次失败携带的
    /// 回执与阶段（输入前的失败没有释放义务）；**确认不了就写未确认**，绝不承诺"释放一定成功"。
    fn cleanup_report(
        &self,
        run_facts: &RunFacts,
        status: ComputerUseTerminalStatus,
        stage: ComputerUseStage,
        error: &ComputerUseError,
        now_ms: u64,
    ) -> Option<CleanupReport> {
        let helper = error.cleanup().copied();
        let release = match helper {
            Some(facts) => facts.release,
            None => match error.receipt() {
                Some(receipt) => match receipt.input_release {
                    runtime::InputReleaseStatus::Released => CleanupReleaseStatus::Confirmed,
                    runtime::InputReleaseStatus::NotNeeded => CleanupReleaseStatus::NotNeeded,
                    runtime::InputReleaseStatus::Unknown => CleanupReleaseStatus::Unconfirmed,
                },
                // 没有回执时，只有"输入阶段之前就结束"的失败才能声明没有释放义务。
                None if stage == ComputerUseStage::Execution => CleanupReleaseStatus::Unconfirmed,
                None => CleanupReleaseStatus::NotNeeded,
            },
        };
        let needs_report = matches!(
            status,
            ComputerUseTerminalStatus::Cancelled | ComputerUseTerminalStatus::TimedOut
        ) || helper.is_some()
            || release.quarantines();
        if !needs_report {
            return None;
        }
        let stopped_at = helper.map_or(now_ms, |facts| facts.stopped_new_input_at_ms);
        let cleanup_started_at = helper.map_or(stopped_at, |facts| facts.cleanup_started_at_ms);
        let cleanup_finished_at = helper.map_or(now_ms, |facts| facts.cleanup_finished_at_ms);
        Some(CleanupReport::assemble(
            run_facts.deadline.cu_deadline_ms(),
            stopped_at,
            cleanup_started_at,
            cleanup_finished_at,
            release,
            helper,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn success(
        &mut self,
        context: &ComputerUseRunContext,
        surface: ComputerUseSurface,
        summary: String,
        attempts: usize,
        steps_completed: usize,
        evidence: Vec<String>,
        supervisor: crate::SupervisorSnapshot,
        run_facts: &RunFacts,
    ) -> ComputerUseResult {
        self.events.state_changed(ComputerUseRunState::Succeeded);
        ComputerUseResult {
            call_id: context.call_id.clone(),
            provider_tool_call_id: context.provider_tool_call_id.clone(),
            status: ComputerUseTerminalStatus::Succeeded,
            stage: ComputerUseStage::Terminal,
            goal_achieved: true,
            surface,
            summary,
            error: None,
            attempts,
            steps_completed,
            input_steps: None,
            evidence,
            supervisor,
            cu_budget: Some(run_facts.cu_budget),
            cleanup: None,
        }
    }

    /// 本次 run 的固定事实：唯一 CU 截止时间及其可序列化投影。
    ///
    /// 截止时间在 `run` 入口只解析一次；宿主若在任务被接纳时已经建立它（推荐），
    /// 这里使用的就是那一个——租约等待、模型切换、初始观察与规划全部落在同一个预算内。
    fn run_facts(&mut self) -> RunFacts {
        let deadline = self
            .cu_deadline
            .unwrap_or_else(|| {
                CuDeadline::establish_without_root(&self.budgets, self.clock.now_ms())
            });
        RunFacts {
            deadline,
            cu_budget: deadline.facts(),
        }
    }
}

/// 一次 run 的固定预算事实。
struct RunFacts {
    deadline: CuDeadline,
    cu_budget: CuBudgetFacts,
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

    use serde_json::json;

    use super::*;
    use crate::{
        ComputerUseAction, ComputerUseActionKind, ComputerUseCapabilities, ComputerUseError,
        ComputerUseRequest, ComputerUseRetryOwner, ComputerUseRiskClass, ComputerUseStage,
        ComputerUseSurface, ComputerUseTerminalStatus, Observation, StepExecution, Verification,
    };

    #[derive(Default)]
    struct FakePlanner {
        surface: ComputerUseSurface,
        actions: Mutex<VecDeque<Result<Option<ComputerUseAction>, ComputerUseError>>>,
    }

    impl ComputerUsePlanner for FakePlanner {
        fn classify<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            _remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
            Box::pin(async move { Ok(self.surface) })
        }

        fn next_action<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            _step: usize,
            _remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
            Box::pin(async move { self.actions.lock().unwrap().pop_front().unwrap_or(Ok(None)) })
        }

        fn verify<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _before: &'a Observation,
            _after: &'a Observation,
            verification: Verification,
            _remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
            Box::pin(async move { Ok(verification) })
        }
    }

    struct FakeAdapter {
        surface: ComputerUseSurface,
        capabilities: ComputerUseCapabilities,
        observations: Mutex<VecDeque<Result<Observation, ComputerUseError>>>,
        executions: Mutex<VecDeque<Result<StepExecution, ComputerUseError>>>,
        verifications: Mutex<VecDeque<Result<Verification, ComputerUseError>>>,
        action_count: AtomicUsize,
    }

    impl ComputerUseAdapter for FakeAdapter {
        fn surface(&self) -> ComputerUseSurface {
            self.surface
        }

        fn capabilities(&self) -> ComputerUseCapabilities {
            self.capabilities
        }

        fn observe(
        &self,
        _request: &ComputerUseRequest,
        _remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
            self.observations
                .lock()
                .unwrap()
                .pop_front()
                .expect("test observation")
        }

        fn act(
            &self,
            _action: &ComputerUseAction,
            _expected_generation: u64,
            _remaining: std::time::Duration,
        ) -> Result<StepExecution, ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            self.executions
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(execution("input sent")))
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &Observation,
            _after: &Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, ComputerUseError> {
            self.verifications
                .lock()
                .unwrap()
                .pop_front()
                .expect("test verification")
        }
    }

    #[derive(Default)]
    struct RecordingEvents(Vec<ComputerUseRunState>);

    impl ComputerUseEventSink for RecordingEvents {
        fn state_changed(&mut self, state: ComputerUseRunState) {
            self.0.push(state);
        }
    }

    #[derive(Default)]
    struct TickClock(u64);

    impl ComputerUseClock for TickClock {
        fn now_ms(&mut self) -> u64 {
            self.0 += 1;
            self.0
        }
    }

    fn request() -> ComputerUseRequest {
        serde_json::from_value(json!({
            "objective": "点击提交并确认成功",
            "surface": "browser",
            "target": { "element": "submit" },
            "success_criteria": ["页面显示提交成功"]
        }))
        .unwrap()
    }

    fn request_with_objective(objective: &str) -> ComputerUseRequest {
        let mut request = request();
        request.objective = objective.into();
        request
    }

    fn observation(generation: u64, state: &str) -> Observation {
        observation_with_state(generation, json!({ "state": state }))
    }

    fn observation_with_state(generation: u64, state: JsonValue) -> Observation {
        Observation {
            generation,
            surface: ComputerUseSurface::Browser,
            surface_identity: "tab-1".into(),
            state,
            evidence: vec![format!("generation-{generation}")],
        }
    }

    fn click() -> ComputerUseAction {
        click_target("submit")
    }

    fn click_target(target: &str) -> ComputerUseAction {
        ComputerUseAction {
            kind: ComputerUseActionKind::Click,
            target: target.into(),
            arguments: json!({}),
            risk: ComputerUseRiskClass::ReversibleLocal,
        }
    }

    fn drag() -> ComputerUseAction {
        ComputerUseAction {
            kind: ComputerUseActionKind::Drag,
            target: "slider".into(),
            arguments: json!({ "to": 80 }),
            risk: ComputerUseRiskClass::ReversibleLocal,
        }
    }

    fn action_with_risk(risk: ComputerUseRiskClass) -> ComputerUseAction {
        ComputerUseAction { risk, ..click() }
    }

    fn execution(summary: &str) -> StepExecution {
        StepExecution {
            input_sent: true,
            summary: summary.into(),
            evidence: vec![summary.into()],
            ..StepExecution::default()
        }
    }

    fn verification(achieved: bool, visible_progress: bool, summary: &str) -> Verification {
        Verification {
            achieved,
            visible_progress,
            summary: summary.into(),
            evidence: vec![summary.into()],
        }
    }

    fn context() -> ComputerUseRunContext {
        ComputerUseRunContext {
            call_id: "cu-1".into(),
            provider_tool_call_id: Some("tool-call-1".into()),
        }
    }

    fn adapter(
        observations: Vec<Result<Observation, ComputerUseError>>,
        verifications: Vec<Result<Verification, ComputerUseError>>,
    ) -> FakeAdapter {
        FakeAdapter {
            surface: ComputerUseSurface::Browser,
            capabilities: ComputerUseCapabilities {
                click: true,
                ..ComputerUseCapabilities::default()
            },
            observations: Mutex::new(observations.into()),
            executions: Mutex::new(VecDeque::new()),
            verifications: Mutex::new(verifications.into()),
            action_count: AtomicUsize::new(0),
        }
    }

    #[tokio::test]
    async fn successful_input_without_verified_goal_is_a_failure() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(None)].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "after"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(false, true, "changed but not complete")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(result.stage, ComputerUseStage::Verification);
        assert!(!result.goal_achieved);
        assert_eq!(result.error.as_ref().unwrap().code, "verification_failed");
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 1);
        let summary: JsonValue = serde_json::from_str(&result.summary).unwrap();
        assert_eq!(summary["last_verification"], "changed but not complete");
        assert_eq!(summary["verification_is_last_observation"], true);
    }

    #[tokio::test]
    async fn verified_goal_is_the_only_success_path() {
        let mut request = request();
        request.max_actions = Some(1);
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click()))].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "success"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(true, true, "success visible")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request, context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        assert!(result.goal_achieved);
        assert_eq!(result.steps_completed, 1);
        assert_eq!(result.provider_tool_call_id.as_deref(), Some("tool-call-1"));
    }

    #[tokio::test]
    async fn request_action_limit_stops_before_another_plan_and_cannot_expand_host_budget() {
        for (requested, host_limit) in [(1, 12), (99, 1)] {
            let mut request = request_with_objective("展开帮助详情");
            request.max_actions = Some(requested);
            let planner = FakePlanner {
                surface: ComputerUseSurface::Browser,
                actions: Mutex::new(vec![
                    Ok(Some(click_target("help-1"))),
                    Ok(Some(click_target("help-2"))),
                ].into()),
            };
            let adapter = adapter(
                vec![Ok(observation(1, "before")), Ok(observation(2, "changed"))],
                vec![
                    Ok(verification(false, false, "not yet")),
                    Ok(verification(false, true, "changed but incomplete")),
                ],
            );
            let mut controller = ComputerUseController::new(
                planner,
                adapter,
                RecordingEvents::default(),
                TickClock::default(),
                crate::ComputerUseBudgets {
                    max_actions: host_limit,
                    ..crate::ComputerUseBudgets::default()
                },
            );
            let result = controller.run(&request, context()).await;
            assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
            assert_eq!(result.stage, ComputerUseStage::Supervisor);
            assert_eq!(result.error.as_ref().unwrap().code, "budget_exhausted");
            assert_eq!(result.error.as_ref().unwrap().retry_owner, ComputerUseRetryOwner::None);
            assert!(!result.goal_achieved);
            assert_eq!(result.supervisor.action_count, 1);
            assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 1);
            assert!(controller.adapter().verifications.lock().unwrap().is_empty());
            assert_eq!(controller.planner().actions.lock().unwrap().len(), 1);
            let summary: JsonValue = serde_json::from_str(&result.summary).unwrap();
            assert_eq!(summary["last_verification"], "changed but incomplete");
            assert_eq!(summary["verification_is_last_observation"], true);
        }
    }

    #[tokio::test]
    async fn unsupported_action_is_blocked_before_input() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(drag()))].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before"))],
            vec![Ok(verification(false, false, "not yet"))],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(result.stage, ComputerUseStage::PolicyCheck);
        assert_eq!(result.error.as_ref().unwrap().code, "unsupported_action");
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn risky_actions_without_host_approval_are_blocked_before_input() {
        for risk in [
            ComputerUseRiskClass::Stateful,
            ComputerUseRiskClass::Sensitive,
            ComputerUseRiskClass::ForbiddenOrAmbiguous,
        ] {
            let planner = FakePlanner {
                surface: ComputerUseSurface::Browser,
                actions: Mutex::new(vec![Ok(Some(action_with_risk(risk)))].into()),
            };
            let adapter = adapter(
                vec![Ok(observation(1, "before"))],
                vec![Ok(verification(false, false, "not yet"))],
            );
            let mut controller = ComputerUseController::new(
                planner,
                adapter,
                RecordingEvents::default(),
                TickClock::default(),
                crate::ComputerUseBudgets::default(),
            );

            let result = controller.run(&request(), context()).await;

            assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
            assert_eq!(result.stage, ComputerUseStage::Approval);
            assert_eq!(result.error.as_ref().unwrap().code, "approval_required");
            assert_eq!(
                result.error.as_ref().unwrap().retry_owner,
                ComputerUseRetryOwner::User
            );
            assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 0);
            assert!(controller
                .event_sink()
                .0
                .contains(&ComputerUseRunState::AwaitingApproval));
        }
    }

    #[tokio::test]
    async fn explicit_host_approval_allows_the_exact_risky_action_to_execute() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(
                vec![Ok(Some(action_with_risk(ComputerUseRiskClass::Stateful)))].into(),
            ),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "success"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(true, true, "success visible")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        )
        .with_approval_policy(
            |_request: &ComputerUseRequest,
             action: &ComputerUseAction,
             observation: &Observation| {
                action.kind == ComputerUseActionKind::Click
                    && action.target == "submit"
                    && action.risk == ComputerUseRiskClass::Stateful
                    && observation.surface_identity == "tab-1"
                    && observation.generation == 1
            },
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 1);
        assert!(controller
            .event_sink()
            .0
            .contains(&ComputerUseRunState::AwaitingApproval));
    }

    #[tokio::test]
    async fn host_semantics_cannot_be_bypassed_by_a_broad_approval_policy() {
        let cases = vec![
            (
                "target_node_delete",
                request_with_objective("管理项目列表"),
                click_target("dom-7"),
                observation_with_state(
                    1,
                    json!({
                        "page": {
                            "nodes": [{
                                "reference": "dom-7",
                                "role": "button",
                                "name": "永久删除项目"
                            }]
                        }
                    }),
                ),
                "destructive_change",
            ),
            (
                "objective_purchase",
                request_with_objective("购买当前商品"),
                click_target("dom-8"),
                observation_with_state(
                    1,
                    json!({
                        "page": {
                            "nodes": [{
                                "reference": "dom-8",
                                "role": "button",
                                "name": "继续"
                            }]
                        }
                    }),
                ),
                "purchase_or_payment",
            ),
            (
                "action_target_send",
                request_with_objective("每一步只发送一次普通点击，操作当前按钮"),
                click_target("send-message"),
                observation_with_state(
                    1,
                    json!({
                        "page": {
                            "nodes": [{
                                "reference": "send-message",
                                "role": "button",
                                "name": "继续"
                            }]
                        }
                    }),
                ),
                "external_communication",
            ),
        ];

        for (name, request, action, before, expected_category) in cases {
            assert_eq!(action.risk, ComputerUseRiskClass::ReversibleLocal);
            let planner = FakePlanner {
                surface: ComputerUseSurface::Browser,
                actions: Mutex::new(vec![Ok(Some(action))].into()),
            };
            let adapter = adapter(
                vec![Ok(before)],
                vec![Ok(verification(false, false, "not yet"))],
            );
            let mut controller = ComputerUseController::new(
                planner,
                adapter,
                RecordingEvents::default(),
                TickClock::default(),
                crate::ComputerUseBudgets::default(),
            )
            .with_approval_policy(
                |_request: &ComputerUseRequest,
                 _action: &ComputerUseAction,
                 _observation: &Observation| true,
            );

            let result = controller.run(&request, context()).await;

            assert_eq!(result.status, ComputerUseTerminalStatus::Blocked, "{name}");
            assert_eq!(result.stage, ComputerUseStage::Approval, "{name}");
            assert_eq!(
                result.error.as_ref().map(|error| error.code.as_str()),
                Some("approval_required"),
                "{name}"
            );
            assert!(result.summary.contains(expected_category), "{name}");
            assert_eq!(
                controller.adapter().action_count.load(Ordering::SeqCst),
                0,
                "{name}"
            );
            assert!(
                controller
                    .event_sink()
                    .0
                    .contains(&ComputerUseRunState::AwaitingApproval),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn ordinary_harmless_click_is_not_falsely_blocked_by_host_semantics() {
        let request = request_with_objective("每一步只发送一次普通点击，不批量点击，展开帮助详情");
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click_target("dom-42")))].into()),
        };
        let adapter = adapter(
            vec![
                Ok(observation_with_state(
                    1,
                    json!({
                        "page": {
                            "nodes": [{
                                "reference": "dom-42",
                                "role": "button",
                                "name": "查看详情"
                            }]
                        }
                    }),
                )),
                Ok(observation_with_state(
                    2,
                    json!({
                        "page": {
                            "nodes": [{
                                "reference": "dom-42",
                                "role": "button",
                                "name": "收起详情"
                            }]
                        }
                    }),
                )),
            ],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(true, true, "details visible")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request, context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 1);
        assert!(!controller
            .event_sink()
            .0
            .contains(&ComputerUseRunState::AwaitingApproval));
    }

    #[test]
    fn host_semantic_classifier_covers_arguments_authorization_install_and_secrets() {
        for (corpus, expected) in [
            ("grant access", "authorization_or_installation"),
            ("安装浏览器扩展", "authorization_or_installation"),
            ("输入 API key", "credential_or_secret"),
            ("password field", "credential_or_secret"),
        ] {
            assert_eq!(classify_sensitive_semantics(corpus), Some(expected));
        }
        for objective in ["发送一次点击结果给他人", "发送一次普通点击 结果给他人", "发送鼠标输入记录", "发送一次普通点击，发布消息"] {
            assert_eq!(classify_sensitive_semantics(&normalize_objective_input_delivery(objective)), Some("external_communication"));
        }

        let request = request_with_objective("完成下一步");
        let mut action = click_target("dom-9");
        action.arguments = json!({"confirmation_label": "授权安装"});
        assert_eq!(
            host_sensitive_semantic_category(
                &request,
                &action,
                &observation_with_state(1, json!({"page":{"nodes":[]}}))
            ),
            Some("authorization_or_installation")
        );
    }

    #[test]
    fn mechanical_input_description_keeps_outbound_action_checks() {
        let observation = observation_with_state(1, json!({"page":{"nodes":[]}}));
        let action = click_target("dom-9");
        for objective in [
            "在来源页单击进入目标页，仅观察网页自然跳转的结果，不向目标页发送任何输入",
            "在当前右栏浏览器单击打开目标链接一次，观察网页通过window.open产生的链接在内置浏览器中的显示结果，不向目标页发送输入",
        ] {
            assert_eq!(host_sensitive_semantic_category(&request_with_objective(objective), &action, &observation), None);
        }
        for objective in ["发送任何输入记录给他人", "发送任何输入结果", "发送输入记录给他人", "发送输入结果", "发送任何输入，发布消息", "发送任何输入、分享记录"] {
            assert_eq!(host_sensitive_semantic_category(
                &request_with_objective(objective), &action, &observation,
            ), Some("external_communication"), "{objective}");
        }
        let request = request_with_objective("不向目标页发送任何输入");
        let sensitive_node = observation_with_state(1, json!({"page":{"nodes":[
            {"reference":"dom-9", "name":"发送消息"}
        ]}}));
        assert_eq!(host_sensitive_semantic_category(&request, &action, &sensitive_node), Some("external_communication"));
        for value in ["post", "share"] {
            let mut parameter_action = action.clone();
            parameter_action.arguments = json!({"operation":value});
            assert_eq!(host_sensitive_semantic_category(&request, &parameter_action, &observation), Some("external_communication"));
        }
    }

    #[test]
    fn preapproval_claim_is_not_an_authorization_action_or_an_approval_bypass() {
        let observation = observation_with_state(1, json!({"page":{"nodes":[]}}));
        let action = click_target("window-canvas:8890a54");
        for objective in [
            "用户已明确授权本次 Paint 手绘测试。在白色画布内拖动绘画",
            "用户已授权本次鼠标拖动。在画布内画一个闭合矩形",
            "用户已明确授权本轮 Paint 手绘验收。在画布内拖动绘画",
            "用户已授权本轮鼠标拖动。在画布内画一个闭合矩形",
        ] {
            assert_eq!(host_sensitive_semantic_category(
                &request_with_objective(objective), &action, &observation,
            ), None);
        }
        for (operation, category) in [
            ("安装浏览器扩展", "authorization_or_installation"),
            ("点击授权按钮", "authorization_or_installation"),
            ("删除文件", "destructive_change"),
            ("支付订单", "purchase_or_payment"),
            ("发送消息给他人", "external_communication"),
            ("输入 API key", "credential_or_secret"),
        ] {
            for claim in ["用户已明确授权本次", "用户已明确授权本轮", "用户已授权本轮"] {
                let objective = format!("{claim}{operation}");
                assert_eq!(host_sensitive_semantic_category(
                    &request_with_objective(&objective), &action, &observation,
                ), Some(category), "{objective}");
            }
        }
        let request = request_with_objective("用户已明确授权本次鼠标点击");
        let sensitive_node = observation_with_state(1, json!({"page":{"nodes":[
            {"reference":"dom-9", "name":"授权访问"}
        ]}}));
        assert_eq!(host_sensitive_semantic_category(
            &request, &click_target("dom-9"), &sensitive_node,
        ), Some("authorization_or_installation"));
        let mut parameter_action = action;
        parameter_action.arguments = json!({"label":"用户已明确授权本次访问"});
        assert_eq!(host_sensitive_semantic_category(
            &request, &parameter_action, &observation,
        ), Some("authorization_or_installation"));
        parameter_action.arguments = json!({"label":"用户已明确授权本轮访问"});
        assert_eq!(host_sensitive_semantic_category(
            &request, &parameter_action, &observation,
        ), Some("authorization_or_installation"));
    }

    #[tokio::test]
    async fn stale_observation_is_reobserved_once_by_the_controller() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(Some(click()))].into()),
        };
        let stale =
            ComputerUseError::recoverable("stale_observation", "surface changed before input");
        let adapter = adapter(
            vec![
                Ok(observation(1, "before")),
                Ok(observation(2, "refreshed")),
                Ok(observation(3, "success")),
            ],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(true, true, "success visible")),
            ],
        );
        *adapter.executions.lock().unwrap() = vec![Err(stale), Ok(execution("input sent"))].into();
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(result.attempts, 2);
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 2);
        assert_eq!(result.supervisor.replan_count, 1);
    }

    /// RPR-04b 行为变更：`stale_observation` 的"重新观察一次"只适用于确认还没注入输入的情形。
    /// 回执一旦表明输入可能已经发出，继续规划等于在未知按键状态下重放输入。
    #[tokio::test]
    async fn stale_error_with_injected_input_receipt_is_not_silently_replanned() {
        use runtime::{ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus};

        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(Some(click()))].into()),
        };
        let action = click();
        let receipt = ActionReceipt {
            action_id: crate::contracts::action_attempt_id(ComputerUseSurface::Browser, &action),
            input_delivery: InputDelivery::Sent,
            partial: Some(true),
            path_completed: Some(false),
            confirmed_point_count: Some(1),
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::Released,
        };
        receipt.validate().expect("测试回执必须自洽");
        let stale = ComputerUseError::recoverable("stale_observation", "surface changed mid input")
            .with_receipt(receipt);
        let adapter = adapter(
            vec![
                Ok(observation(1, "before")),
                Ok(observation(2, "refreshed")),
            ],
            vec![Ok(verification(false, false, "not yet"))],
        );
        *adapter.executions.lock().unwrap() = vec![Err(stale), Ok(execution("second input"))].into();
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Failed);
        // 只注入过一次：没有重新观察、没有第二次输入，也没有重规划。
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 1);
        assert_eq!(result.supervisor.replan_count, 0);
        let error = result.error.expect("必须留下终态失败");
        assert_eq!(error.code, "stale_observation");
        assert_eq!(
            error.receipt().and_then(|receipt| receipt.confirmed_point_count),
            Some(1),
            "部分输入事实必须原样留在终态错误里"
        );
    }

    /// 身份不匹配的回执属于协议异常：同样不得触发静默重放。
    #[tokio::test]
    async fn stale_error_with_a_mismatched_receipt_is_not_silently_replanned() {
        use runtime::{ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus};

        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click()))].into()),
        };
        let receipt = ActionReceipt {
            action_id: "browser:submit:0000000000000000".to_string(),
            input_delivery: InputDelivery::MayHaveBeenSent,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::Unknown,
        };
        let stale = ComputerUseError::recoverable("stale_observation", "surface changed")
            .with_receipt(receipt);
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "refreshed"))],
            vec![Ok(verification(false, false, "not yet"))],
        );
        *adapter.executions.lock().unwrap() = vec![Err(stale), Ok(execution("second input"))].into();
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Failed);
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn repeated_no_progress_stops_before_a_third_input() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(
                vec![Ok(Some(click())), Ok(Some(click())), Ok(Some(click()))].into(),
            ),
        };
        let adapter = adapter(
            vec![
                Ok(observation(1, "same")),
                Ok(observation(2, "same")),
                Ok(observation(3, "same")),
            ],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(false, false, "still same")),
                Ok(verification(false, false, "still same")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(result.error.as_ref().unwrap().code, "no_progress");
        assert_eq!(controller.adapter().action_count.load(Ordering::SeqCst), 2);
        assert_eq!(result.supervisor.no_progress_count, 2);
    }

    #[tokio::test]
    async fn adapter_errors_keep_retry_ownership_for_the_model_result() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click()))].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before"))],
            vec![Ok(verification(false, false, "not yet"))],
        );
        *adapter.executions.lock().unwrap() = vec![Err(ComputerUseError::new(
            "target_not_found",
            "submit disappeared",
            true,
            ComputerUseRetryOwner::Model,
        ))]
        .into();
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Failed);
        assert_eq!(result.stage, ComputerUseStage::Execution);
        assert_eq!(
            result.error.as_ref().unwrap().retry_owner,
            ComputerUseRetryOwner::Model
        );
    }

    #[test]
    fn controller_contracts_are_exported_from_crate_root() {
        let context = crate::ComputerUseRunContext {
            call_id: "cu-root".into(),
            provider_tool_call_id: None,
        };
        fn accepts_planner<T: crate::ComputerUsePlanner>(_planner: &T) {}
        fn accepts_adapter<T: crate::ComputerUseAdapter>(_adapter: &T) {}

        let planner = FakePlanner::default();
        let adapter = adapter(
            vec![Ok(observation(1, "before"))],
            vec![Ok(verification(true, true, "done"))],
        );
        assert_eq!(context.call_id, "cu-root");
        accepts_planner(&planner);
        accepts_adapter(&adapter);
    }

    // ---- S1.5：每阶段取消 / 超时故障注入 ----

    /// 宿主取消以 `cancelled` 错误码到达；控制器必须把它当作终态，
    /// 且不得再产生任何新输入。
    fn cancelled_error() -> ComputerUseError {
        ComputerUseError::new(
            "cancelled",
            "host cancelled the run",
            false,
            ComputerUseRetryOwner::None,
        )
    }

    fn adapter_with_executions(
        observations: Vec<Result<Observation, ComputerUseError>>,
        executions: Vec<Result<StepExecution, ComputerUseError>>,
        verifications: Vec<Result<Verification, ComputerUseError>>,
    ) -> FakeAdapter {
        FakeAdapter {
            surface: ComputerUseSurface::Browser,
            capabilities: ComputerUseCapabilities {
                click: true,
                ..ComputerUseCapabilities::default()
            },
            observations: Mutex::new(observations.into()),
            executions: Mutex::new(executions.into()),
            verifications: Mutex::new(verifications.into()),
            action_count: AtomicUsize::new(0),
        }
    }

    #[tokio::test]
    async fn cancellation_during_observation_is_terminal_without_input() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(VecDeque::new()),
        };
        let adapter = adapter(vec![Err(cancelled_error())], vec![]);
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Cancelled);
        assert_eq!(result.stage, ComputerUseStage::Observation);
        assert_eq!(
            controller.adapter().action_count.load(Ordering::SeqCst),
            0,
            "cancellation before planning must not inject input"
        );
    }

    #[tokio::test]
    async fn cancellation_during_planning_is_terminal_without_input() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Err(cancelled_error())].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before"))],
            // 前置校验（目标是否已达成）先于规划发生，必须给它一个"未达成"结果。
            vec![Ok(verification(false, false, "not yet"))],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Cancelled);
        assert_eq!(result.stage, ComputerUseStage::Planning);
        assert_eq!(
            controller.adapter().action_count.load(Ordering::SeqCst),
            0,
            "cancellation during planning must not inject input"
        );
    }

    #[tokio::test]
    async fn cancellation_during_execution_is_terminal_cancelled() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(None)].into()),
        };
        let adapter = adapter_with_executions(
            vec![Ok(observation(1, "before")), Ok(observation(2, "after"))],
            vec![Err(cancelled_error())],
            vec![Ok(verification(false, false, "not yet"))],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Cancelled);
        assert_eq!(result.error.as_ref().unwrap().code, "cancelled");
        assert_eq!(
            controller.adapter().action_count.load(Ordering::SeqCst),
            1,
            "the cancelled execution must not be retried or followed by another input"
        );
    }

    #[tokio::test]
    async fn cancellation_during_verification_is_terminal_cancelled() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(None)].into()),
        };
        let adapter = adapter_with_executions(
            vec![Ok(observation(1, "before")), Ok(observation(2, "after"))],
            vec![Ok(execution("input sent"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Err(cancelled_error()),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Cancelled);
        assert_eq!(result.error.as_ref().unwrap().code, "cancelled");
        assert!(!result.goal_achieved);
        assert_eq!(
            controller.adapter().action_count.load(Ordering::SeqCst),
            1,
            "a cancelled verification must not trigger a second action"
        );
    }

    /// deadline 到期后不得再产生业务动作：预算守卫在**动作前**判定，
    /// 因此时钟一越过 deadline，首个动作就必须被拒绝且零输入。
    #[tokio::test]
    async fn deadline_expiry_before_first_action_performs_no_input() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(None)].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before"))],
            // 前置校验必须"未达成"，否则 run 会在首个动作前正常结束，测不到 deadline。
            vec![Ok(verification(false, false, "not yet"))],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets {
                // TickClock 每次自增 1ms；timeout_ms = 1 时首个动作前已到期。
                timeout_ms: 1,
                ..crate::ComputerUseBudgets::default()
            },
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.error.as_ref().unwrap().code, "deadline_exceeded");
        assert!(!result.goal_achieved);
        assert_eq!(
            controller.adapter().action_count.load(Ordering::SeqCst),
            0,
            "an expired deadline must not allow a new business action"
        );
    }

    // ---- RPR-11c：CU 截止时间、planner 剩余预算、收尾报告 ----

    /// 记录每次调用收到的剩余预算（毫秒），并可选地在规划时"消耗"一段时间。
    #[derive(Default)]
    struct BudgetRecordingPlanner {
        surface: ComputerUseSurface,
        /// 每次模型调用之前推进的毫秒数（模拟模型请求/切换耗时）。
        burn_before_each_call_ms: u64,
        seen_remaining_ms: Mutex<Vec<u64>>,
        clock: std::sync::Arc<AtomicUsize>,
    }

    impl BudgetRecordingPlanner {
        fn new(clock: std::sync::Arc<AtomicUsize>) -> Self {
            Self {
                surface: ComputerUseSurface::Browser,
                burn_before_each_call_ms: 0,
                seen_remaining_ms: Mutex::new(Vec::new()),
                clock,
            }
        }

        fn record(&self, remaining: std::time::Duration) {
            if self.burn_before_each_call_ms > 0 {
                let now = self.clock.load(Ordering::SeqCst) as u64;
                self.clock.store(
                    usize::try_from(now + self.burn_before_each_call_ms).unwrap_or(usize::MAX),
                    Ordering::SeqCst,
                );
            }
            self.seen_remaining_ms
                .lock()
                .unwrap()
                .push(remaining.as_millis().min(u128::from(u64::MAX)) as u64);
        }

        fn seen(&self) -> Vec<u64> {
            self.seen_remaining_ms.lock().unwrap().clone()
        }
    }

    impl ComputerUsePlanner for BudgetRecordingPlanner {
        fn classify<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
            self.record(remaining);
            Box::pin(async move { Ok(self.surface) })
        }

        fn next_action<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            _step: usize,
            remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
            self.record(remaining);
            Box::pin(async move { Ok(None) })
        }

        fn verify<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _before: &'a Observation,
            _after: &'a Observation,
            verification: Verification,
            remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
            self.record(remaining);
            Box::pin(async move { Ok(verification) })
        }
    }

    /// 单调时钟的共享游标：既能推进时间，也能让规划者看到真实剩余。
    fn advancing_clock() -> (std::sync::Arc<AtomicUsize>, SharedClock) {
        let shared = std::sync::Arc::new(AtomicUsize::new(0));
        (
            shared.clone(),
            SharedClock {
                cursor: shared,
                step_ms: 1,
            },
        )
    }

    struct SharedClock {
        cursor: std::sync::Arc<AtomicUsize>,
        step_ms: usize,
    }

    impl ComputerUseClock for SharedClock {
        fn now_ms(&mut self) -> u64 {
            self.cursor.fetch_add(self.step_ms, Ordering::SeqCst) as u64
        }
    }

    /// 规划者收到的剩余预算来自**控制器建立的 CU 截止时间**：起点提前，
    /// 剩余就必须相应减少——模型切换/租约等待的耗时不允许被隐藏。
    #[tokio::test]
    async fn planner_receives_the_remaining_of_the_established_cu_deadline() {
        let budgets = crate::ComputerUseBudgets {
            timeout_ms: 10_000,
            ..crate::ComputerUseBudgets::default()
        };
        // 任务在 1_000ms 被接纳并建立截止时间（= 11_000ms）；控制器在 3_000ms 才开始跑。
        let deadline = crate::CuDeadline::establish_without_root(&budgets, 1_000);
        let (cursor, clock) = advancing_clock();
        cursor.store(3_000, Ordering::SeqCst);
        let planner = BudgetRecordingPlanner::new(cursor.clone());
        let adapter = adapter(
            vec![Ok(observation(1, "before"))],
            vec![Ok(verification(false, false, "not yet"))],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            clock,
            budgets,
        )
        .with_cu_deadline(deadline);

        let result = controller.run(&request(), context()).await;

        let seen = controller.planner().seen();
        assert!(!seen.is_empty(), "规划者必须收到剩余预算");
        assert!(
            seen.iter().all(|remaining| *remaining < 10_000),
            "接纳到开始执行之间的 2_000ms 必须从预算里扣掉，实际收到的剩余：{seen:?}"
        );
        assert_eq!(
            result.cu_budget.unwrap().cu_deadline_established_at_ms,
            Some(1_000),
            "运行记录必须保留接纳时建立的截止时间，而不是控制器入口的时刻"
        );
    }

    /// 规划者连续收到递减的剩余：同一个 run 内第二次调用不许重新起算。
    #[tokio::test]
    async fn consecutive_planner_calls_see_strictly_decreasing_remaining() {
        let budgets = crate::ComputerUseBudgets {
            timeout_ms: 10_000,
            ..crate::ComputerUseBudgets::default()
        };
        let (cursor, clock) = advancing_clock();
        let mut planner = BudgetRecordingPlanner::new(cursor.clone());
        // 每次模型调用之前推进 500ms：等价于"模型切换/规划耗时计入预算"。
        planner.burn_before_each_call_ms = 500;
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "after"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(false, false, "still not")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            clock,
            budgets,
        );

        let _ = controller.run(&request(), context()).await;

        let seen = controller.planner().seen();
        assert!(
            seen.len() >= 2,
            "本 run 至少要有初始验收 + 规划两次模型请求，实际：{seen:?}"
        );
        assert!(
            seen.windows(2).all(|pair| pair[1] < pair[0]),
            "同一 run 内的连续请求必须看到严格递减的剩余：{seen:?}"
        );
    }

    /// 内部恢复（stale 后重新观察）不重置时钟，也不允许触发新的模型请求。
    #[tokio::test]
    async fn internal_recovery_does_not_reset_the_remaining_budget() {
        let budgets = crate::ComputerUseBudgets {
            timeout_ms: 10_000,
            ..crate::ComputerUseBudgets::default()
        };
        let (cursor, clock) = advancing_clock();
        let mut planner = BudgetRecordingPlanner::new(cursor.clone());
        planner.burn_before_each_call_ms = 400;
        let adapter = adapter(
            vec![
                Ok(observation(1, "before")),
                Ok(observation(2, "refreshed")),
                Ok(observation(3, "after")),
            ],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(false, false, "still not")),
            ],
        );
        *adapter.executions.lock().unwrap() =
            vec![Err(ComputerUseError::recoverable(
                "stale_observation",
                "surface changed before input",
            ))]
            .into();
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            clock,
            budgets,
        );

        let _ = controller.run(&request(), context()).await;

        let seen = controller.planner().seen();
        assert!(seen.len() >= 2);
        assert!(
            seen.windows(2).all(|pair| pair[1] < pair[0]),
            "重新观察不得把剩余预算重置回起点：{seen:?}"
        );
        assert!(cursor.load(Ordering::SeqCst) < 10_000);
    }

    /// 未确认释放必须按"未知即隔离"记录：收尾报告的五项分开给出，且不承诺释放成功。
    #[tokio::test]
    async fn unconfirmed_release_is_reported_as_quarantined_cleanup() {
        use runtime::{ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus};

        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click())), Ok(None)].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "after"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(false, false, "still not")),
            ],
        );
        // helper 失联：路径只注入了一部分，释放未确认。
        let action = click();
        let receipt = ActionReceipt {
            action_id: crate::contracts::action_attempt_id(ComputerUseSurface::Browser, &action),
            input_delivery: InputDelivery::Sent,
            partial: Some(true),
            path_completed: Some(false),
            confirmed_point_count: Some(1),
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::Unknown,
        };
        receipt.validate().expect("样例回执必须自洽");
        *adapter.executions.lock().unwrap() = vec![Err(ComputerUseError::new(
            "helper_lost",
            "helper did not confirm release",
            true,
            ComputerUseRetryOwner::System,
        )
        .with_receipt(receipt))]
        .into();
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Failed);
        let cleanup = result.cleanup.expect("未确认释放必须留下收尾报告");
        assert_eq!(cleanup.release.as_str(), "unconfirmed");
        assert!(cleanup.quarantined, "未知即隔离");
        assert_eq!(
            cleanup.business_deadline_ms,
            result.cu_budget.unwrap().cu_deadline_ms.unwrap(),
            "业务截止时刻必须就是本次 CU 的截止时间"
        );
        assert!(cleanup.cleanup_finished_at_ms >= cleanup.cleanup_started_at_ms);
    }

    /// 输入前的取消没有释放义务，但同样要分别给出停止输入/收尾时刻。
    #[tokio::test]
    async fn cancellation_before_input_reports_cleanup_without_release_obligation() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(VecDeque::new()),
        };
        let adapter = adapter(vec![Err(cancelled_error())], vec![]);
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        let cleanup = result.cleanup.expect("取消必须留下收尾报告");
        assert_eq!(cleanup.release.as_str(), "not_needed");
        assert!(!cleanup.quarantined);
        assert_eq!(
            cleanup.new_business_input_stopped_at_ms,
            cleanup.cleanup_started_at_ms
        );
    }

    /// 成功的 run 不写收尾报告，但必须写 CU 预算事实。
    #[tokio::test]
    async fn successful_run_records_budget_facts_without_a_cleanup_report() {
        let planner = FakePlanner {
            surface: ComputerUseSurface::Browser,
            actions: Mutex::new(vec![Ok(Some(click()))].into()),
        };
        let adapter = adapter(
            vec![Ok(observation(1, "before")), Ok(observation(2, "success"))],
            vec![
                Ok(verification(false, false, "not yet")),
                Ok(verification(true, true, "success visible")),
            ],
        );
        let mut controller = ComputerUseController::new(
            planner,
            adapter,
            RecordingEvents::default(),
            TickClock::default(),
            crate::ComputerUseBudgets::default(),
        );

        let result = controller.run(&request(), context()).await;

        assert!(result.goal_achieved);
        assert!(result.cleanup.is_none());
        let facts = result.cu_budget.expect("成功路径也必须写预算事实");
        assert_eq!(facts.root_deadline_state.as_str(), "not_wired");
        assert_eq!(facts.root_deadline, crate::RootDeadline::Absent);
        assert!(facts.is_accepted());
    }
}
