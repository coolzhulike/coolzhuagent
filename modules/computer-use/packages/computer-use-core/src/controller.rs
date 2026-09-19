use crate::{
    ActionFingerprint, ComputerUseAction, ComputerUseBudgets, ComputerUseCapabilities,
    ComputerUseError, ComputerUseRequest, ComputerUseResult, ComputerUseRetryOwner,
    ComputerUseRunState, ComputerUseStage, ComputerUseSurface, ComputerUseTerminalStatus,
    Observation, RunBudgetGuard, StepExecution, Verification,
};
use serde_json::Value as JsonValue;

pub type PlannerFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub trait ComputerUsePlanner: Send + Sync {
    fn classify<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        observation: &'a Observation,
    ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>>;

    fn next_action<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        observation: &'a Observation,
        step: usize,
    ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>>;

    /// 宿主可用最新视觉证据复核适配器结果；默认保留已有纯 DOM/测试行为。
    fn verify<'a>(
        &'a self,
        _request: &'a ComputerUseRequest,
        _before: &'a Observation,
        _after: &'a Observation,
        verification: Verification,
    ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
        Box::pin(async move { Ok(verification) })
    }
}

pub trait ComputerUseAdapter: Send + Sync {
    fn surface(&self) -> ComputerUseSurface;
    fn capabilities(&self) -> ComputerUseCapabilities;
    fn observe(&self, request: &ComputerUseRequest) -> Result<Observation, ComputerUseError>;
    fn act(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
    ) -> Result<StepExecution, ComputerUseError>;
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
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
    push_bounded_semantic_text(&mut corpus, &request.objective);
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
        }
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
    pub const fn event_sink(&self) -> &E {
        &self.events
    }

    pub async fn run(
        &mut self,
        request: &ComputerUseRequest,
        context: ComputerUseRunContext,
    ) -> ComputerUseResult {
        self.events.state_changed(ComputerUseRunState::Requested);
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
            );
        }

        let started_at_ms = self.clock.now_ms();
        let mut guard = RunBudgetGuard::new(self.budgets, started_at_ms);
        let mut attempts = 0usize;
        let mut steps_completed = 0usize;
        let mut evidence = Vec::new();
        let mut stale_recovered = false;

        self.events.state_changed(ComputerUseRunState::Observing);
        let mut observation = match self.adapter.observe(request) {
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
                );
            }
        };
        evidence.extend(observation.evidence.clone());

        let surface = if request.surface == ComputerUseSurface::Auto {
            match self.planner.classify(request, &observation).await {
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
            );
        }

        self.events.state_changed(ComputerUseRunState::Verifying);
        let initial_verification = match self
            .adapter
            .verify(&request.success_criteria, &observation, &observation)
        {
            Ok(verification) => self.planner.verify(request, &observation, &observation, verification).await,
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
                    );
                }
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
                );
            }
        }

        loop {
            self.events.state_changed(ComputerUseRunState::Planning);
            let action = match self
                .planner
                .next_action(request, &observation, attempts)
                .await
            {
                Ok(Some(action)) => action,
                Ok(None) => {
                    return self.terminal(
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
                    );
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
                );
            }

            attempts = attempts.saturating_add(1);
            self.events.state_changed(ComputerUseRunState::Executing);
            let execution = match self.adapter.act(&action, observation.generation) {
                Ok(execution) => execution,
                Err(error) if error.code == "stale_observation" && !stale_recovered => {
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
                        );
                    }
                    stale_recovered = true;
                    self.events.state_changed(ComputerUseRunState::Observing);
                    observation = match self.adapter.observe(request) {
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
                );
            }
            steps_completed = steps_completed.saturating_add(1);

            self.events.state_changed(ComputerUseRunState::Observing);
            let next_observation = match self.adapter.observe(request) {
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
                    );
                }
            };
            evidence.extend(next_observation.evidence.clone());

            self.events.state_changed(ComputerUseRunState::Verifying);
            let adapter_verification = self.adapter.verify(
                &request.success_criteria,
                &observation,
                &next_observation,
            );
            let checked_verification = match adapter_verification {
                Ok(verification) => self.planner.verify(request, &observation, &next_observation, verification).await,
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
                );
            }
            observation = next_observation;
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
            evidence,
            supervisor,
        }
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
            evidence,
            supervisor,
        }
    }
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
        ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
            Box::pin(async move { Ok(self.surface) })
        }

        fn next_action<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            _step: usize,
        ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
            Box::pin(async move { self.actions.lock().unwrap().pop_front().unwrap_or(Ok(None)) })
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

        fn observe(&self, _request: &ComputerUseRequest) -> Result<Observation, ComputerUseError> {
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
    }

    #[tokio::test]
    async fn verified_goal_is_the_only_success_path() {
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

        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        assert!(result.goal_achieved);
        assert_eq!(result.steps_completed, 1);
        assert_eq!(result.provider_tool_call_id.as_deref(), Some("tool-call-1"));
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
                request_with_objective("完成消息操作"),
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
        let request = request_with_objective("展开帮助详情");
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
}
