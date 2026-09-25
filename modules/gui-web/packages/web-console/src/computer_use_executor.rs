use std::hash::{Hash, Hasher};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

use computer_use::{
    ComputerUseAction, ComputerUseAdapter, ComputerUseApprovalPolicy, ComputerUseBudgets,
    ComputerUseCapabilities, ComputerUseClock, ComputerUseController, ComputerUseError,
    ComputerUseEventSink, ComputerUsePlanner, ComputerUseRequest, ComputerUseResult,
    ComputerUseRetryOwner, ComputerUseRiskClass, ComputerUseRunContext, ComputerUseRunState,
    ComputerUseStage, ComputerUseSurface, ComputerUseTerminalStatus, CuBudgetFacts, CuDeadline,
    Observation, PlannerFuture, StepExecution, SupervisorSnapshot, TaskIdempotencyKey, Verification,
};
use serde_json::Value as JsonValue;

use crate::browser_bridge::BrowserNativeBridge;
use crate::computer_use_adapters::{
    backend_error, route_computer_use_surface, BrowserComputerUseAdapter, BrowserComputerUsePolicy,
    DesktopComputerUseAdapter, SurfaceRoutingContext,
};
use crate::computer_use_desktop_bridge::DesktopNativeBridge;
use crate::computer_use_planner::CurrentSessionComputerUsePlanner;
use crate::computer_use_store::{
    ComputerUseRunStore, ComputerUseStepRecord, CuWorkspaceAttribution, NewComputerUseRun,
};
use crate::tool_loop_coordinator::ToolCallIdentity;

// 参数尚未进入观察/规划时允许有限纠正；真实 CU 调用仍遵循配置的独立额度。
const MAX_INVALID_INPUTS_PER_TURN: usize = 2;

pub(crate) struct DynComputerUseAdapter(Box<dyn ComputerUseAdapter>);

impl DynComputerUseAdapter {
    pub(crate) fn new(adapter: impl ComputerUseAdapter + 'static) -> Self {
        Self(Box::new(adapter))
    }
}

impl ComputerUseAdapter for DynComputerUseAdapter {
    fn surface(&self) -> ComputerUseSurface {
        self.0.surface()
    }

    fn capabilities(&self) -> ComputerUseCapabilities {
        self.0.capabilities()
    }

    fn observe(
        &self,
        request: &ComputerUseRequest,
        remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
        self.0.observe(request, remaining)
    }

    fn act(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        self.0.act(action, expected_generation, remaining)
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError> {
        self.0.verify(criteria, before, after, remaining)
    }
}

struct TraceState {
    observation: Option<Observation>,
    next_index: usize,
    pending: Option<(ComputerUseStepRecord, String)>,
}

/// 外层聊天取消会丢弃 controller future；仍需将已创建的 CU run 收敛为终态。
struct PendingRunGuard<'a> {
    store: &'a ComputerUseRunStore,
    identity: &'a ToolCallIdentity,
    surface: ComputerUseSurface,
    /// 接纳时建立的 CU 预算事实：即使 future 被丢弃，记录里也必须有真实的截止时间。
    cu_budget: CuBudgetFacts,
}
impl Drop for PendingRunGuard<'_> {
    fn drop(&mut self) {
        if let Ok(Some(run)) = self.store.load(&self.identity.call_id) {
            if run.terminal_result.is_none() {
                let mut result = terminal_result(
                    self.identity,
                    self.surface,
                    ComputerUseStage::Supervisor,
                    cancelled_error(),
                );
                result.cu_budget = Some(self.cu_budget);
                if let Ok((attempts, steps)) = self.store.action_counts(&self.identity.call_id) {
                    result.attempts = attempts;
                    result.steps_completed = steps;
                    result.supervisor.action_count = attempts;
                }
                let _ = self
                    .store
                    .finish(&self.identity.call_id, run.state_version, &result);
            }
        }
    }
}

struct TracingAdapter<'a> {
    inner: DynComputerUseAdapter,
    store: &'a ComputerUseRunStore,
    call_id: String,
    state: Mutex<TraceState>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    #[cfg(windows)]
    input_lease: Option<&'a windows_process_guard::ScopedInputOwnership<'static>>,
}

impl ComputerUseAdapter for TracingAdapter<'_> {
    fn surface(&self) -> ComputerUseSurface {
        self.inner.surface()
    }
    fn capabilities(&self) -> ComputerUseCapabilities {
        self.inner.capabilities()
    }
    fn observe(
        &self,
        request: &ComputerUseRequest,
        remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
        if (self.cancelled)() {
            return Err(cancelled_error());
        }
        let outcome = self.inner.observe(request, remaining).map(|mut observation| {
            if let Some(object) = observation.state.as_object_mut() {
                object.insert(
                    "capabilities".into(),
                    serde_json::to_value(self.inner.capabilities()).unwrap_or(JsonValue::Null),
                );
            }
            observation
        });
        let mut state = self
            .state
            .lock()
            .map_err(|_| persistence_error("step trace lock"))?;
        if let Some((mut step, action)) = state.pending.take() {
            match &outcome {
                Ok(after) => {
                    step.after_evidence_ref =
                        Some(serde_json::to_string(&after.evidence).unwrap_or_default());
                    // 新观察的时间、generation、截图路径变化不代表任务进展；由异步验收写入。
                    step.visible_progress = false;
                    step.status = "input_sent_observed".into();
                }
                Err(error) => {
                    step.status = "observation_failed".into();
                    step.error_code = Some(error.code.clone());
                }
            }
            self.store
                .record_step(&step, &action)
                .map_err(persistence_error)?;
        }
        if let Ok(observation) = &outcome {
            state.observation = Some(observation.clone());
        }
        outcome
    }
    fn act(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        if (self.cancelled)() {
            return Err(cancelled_error());
        }
        // 安全向预检：**只可能提前拒绝，不可能放行**（权威判定在下面的派发边界内）。
        // 它存在的意义是让"已被撤销"这类常见情形不留下审计噪音（步骤行）。
        #[cfg(windows)]
        if self.input_lease.is_some_and(|lease| !lease.is_current()) {
            return Err(lease_lost_error());
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| persistence_error("step trace lock"))?;
        let index = state.next_index;
        state.next_index += 1;
        // 回执身份：与生产者（桌面桥）用同一个函数按动作内容计算，两者必须一致。
        let expected_action_id = computer_use::action_attempt_id(self.surface(), action);
        let action_json = crate::computer_use_store::sanitized_action_json(
            &serde_json::to_string(action).unwrap_or_default(),
        );
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        (self.surface().as_str(), expected_generation, &action_json).hash(&mut fingerprint);
        let mut step = ComputerUseStepRecord {
            run_id: self.call_id.clone(),
            step_index: index,
            observation_generation: expected_generation,
            action_type: serde_json::to_value(action.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| format!("{:?}", action.kind)),
            normalized_target: action.target.clone(),
            action_fingerprint: format!("{:016x}", fingerprint.finish()),
            status: "executing".into(),
            error_code: None,
            before_evidence_ref: state
                .observation
                .as_ref()
                .map(|value| serde_json::to_string(&value.evidence).unwrap_or_default()),
            after_evidence_ref: None,
            visible_progress: false,
            // 这些 v1 字段在输入执行前均未知；旧记录也保持 NULL，不倒灌猜测值。
            input_delivery: None,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect_status: None,
            goal_verdict: None,
            input_release_status: None,
            started_at_ms: now_ms(),
            completed_at_ms: None,
        };
        // 持久化失败时尚未发送输入，避免执行后没有对应审计记录。
        self.store
            .record_step(&step, &action_json)
            .map_err(persistence_error)?;
        // 复检与派发进入**同一 broker 序列化边界**：上面的 `record_step` 是落库（可能长时间
        // 等待），必须在边界之外完成，否则又回到"复检通过 → 等待 → 已撤销 → 仍派发"的交错。
        #[cfg(windows)]
        let dispatched = match self.input_lease {
            Some(lease) => {
                // 守卫约定：`check()` 返回 **true 表示允许派发**，false 表示拒绝
                // （见 `InputDispatchGuard::new` 的文档）；这里两条都是"允许条件"。
                let not_cancelled = || !(self.cancelled)();
                let has_execution_budget = || !remaining.is_zero();
                let guards = [
                    windows_process_guard::InputDispatchGuard::new("cancelled", &not_cancelled),
                    windows_process_guard::InputDispatchGuard::new(
                        "execution_budget_exhausted",
                        &has_execution_budget,
                    ),
                ];
                // 隔离 / 恢复中状态在**运行接纳时**已由未确认释放互锁把关；此处不查库——
                // 边界内出现 DB 查询这类长等待，会重新制造本边界要消除的交错窗口。
                lease.dispatch_if_current(&guards, || {
                    self.inner.act(action, expected_generation, remaining)
                })
            }
            None => Ok(self.inner.act(action, expected_generation, remaining)),
        };
        #[cfg(not(windows))]
        let dispatched = Ok(self.inner.act(action, expected_generation, remaining));
        let result = match dispatched {
            Ok(result) => result,
            Err(refusal) => {
                let error = input_dispatch_refusal_error(&refusal, remaining);
                // 边界拒绝发生在步骤行落库之后：**修正该行**，不留"执行中"的陈旧记录。
                step.status = "input_not_sent".into();
                step.error_code = Some(error.code.clone());
                step.completed_at_ms = Some(now_ms());
                self.store
                    .record_step(&step, &action_json)
                    .map_err(|_| persistence_error("step correction after dispatch refusal"))?;
                return Err(error);
            }
        };
        step.completed_at_ms = Some(now_ms());
        match &result {
            Ok(execution) => {
                step.status = if execution.input_sent {
                    "input_sent"
                } else {
                    "input_not_sent"
                }
                .into();
                match execution.receipt.as_ref() {
                    // 有回执就以回执为准（身份必须匹配，否则按协议异常保守处理）。
                    Some(receipt) => match trusted_receipt_facts(receipt, &expected_action_id) {
                        Some(facts) => apply_stepped_facts(&mut step, facts),
                        None => {
                            step.status = "receipt_protocol_anomaly".into();
                            apply_stepped_facts(&mut step, conservative_facts(execution.input_sent));
                        }
                    },
                    // 没有回执的适配器（旧实现、浏览器桥）：沿用其逐字段声明。
                    None => {
                        step.input_delivery = Some(if execution.input_sent {
                            runtime::InputDelivery::Sent
                        } else {
                            runtime::InputDelivery::NotSent
                        });
                        // helper 没有逐字段回执时保持 NULL；不得由 sent 推导完整路径或释放。
                        step.partial = execution.partial;
                        step.path_completed = execution.path_completed;
                        step.confirmed_point_count = execution.confirmed_point_count;
                        step.input_release_status =
                            execution.input_release_status.map(step_release_status);
                    }
                }
                if execution.input_sent {
                    state.pending = Some((step.clone(), action_json.clone()));
                }
            }
            Err(error) => {
                step.status = "failed".into();
                step.error_code = Some(error.code.clone());
                match error.receipt.as_ref() {
                    // §2.2：失败路径的四个维度由 helper 自己的回执决定，而不是由错误码猜。
                    Some(receipt) => match trusted_receipt_facts(receipt, &expected_action_id) {
                        Some(facts) => apply_stepped_facts(&mut step, facts),
                        None => {
                            // 回执属于别的动作或自相矛盾：记录协议异常，
                            // 既不接纳到当前动作，也不因为读不懂而报告"零输入"。
                            step.status = "receipt_protocol_anomaly".into();
                            apply_stepped_facts(&mut step, conservative_facts(false));
                        }
                    },
                    // 旧错误没有回执：沿用既有按错误码判定的保守规则。
                    None => {
                        // §2.2 恢复表要求失败路径也明确"是否可能已发送"与释放事实：
                        // 只有确认"输入前就失败"的码才能记 not_sent，其余按 may_have_been_sent；
                        // 释放一律 unknown——不得由"可能已发送"推断出 released。
                        if failure_may_have_sent_input(&error.code) {
                            step.input_delivery = Some(runtime::InputDelivery::MayHaveBeenSent);
                            step.input_release_status = Some(runtime::InputReleaseStatus::Unknown);
                        } else {
                            step.input_delivery = Some(runtime::InputDelivery::NotSent);
                            step.input_release_status = Some(runtime::InputReleaseStatus::NotNeeded);
                        }
                    }
                }
            }
        }
        self.store
            .record_step(&step, &action_json)
            .map_err(persistence_error)?;
        result
    }
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError> {
        self.inner.verify(criteria, before, after, remaining)
    }
}

pub(crate) trait ComputerUseAdapterFactory: Send + Sync {
    fn routing_context(&self) -> SurfaceRoutingContext;

    fn build(&self, surface: ComputerUseSurface)
        -> Result<DynComputerUseAdapter, ComputerUseError>;
}

struct PlannerRef<'a> {
    planner: &'a dyn ComputerUsePlanner,
    store: &'a ComputerUseRunStore,
    call_id: &'a str,
}

impl ComputerUsePlanner for PlannerRef<'_> {
    fn classify<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        observation: &'a Observation,
        remaining: std::time::Duration,
    ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
        self.planner.classify(request, observation, remaining)
    }

    fn next_action<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        observation: &'a Observation,
        step: usize,
        remaining: std::time::Duration,
    ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
        self.planner.next_action(request, observation, step, remaining)
    }

    fn verify<'a>(
        &'a self,
        request: &'a ComputerUseRequest,
        before: &'a Observation,
        after: &'a Observation,
        verification: Verification,
        remaining: std::time::Duration,
    ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
        Box::pin(async move {
            let verification = self
                .planner
                .verify(request, before, after, verification, remaining)
                .await?;
            if before.generation != after.generation {
                self.store
                    .record_step_verification(self.call_id, before.generation, &verification)
                    .map_err(persistence_error)?;
            }
            Ok(verification)
        })
    }
}

struct SystemClock;

impl ComputerUseClock for SystemClock {
    fn now_ms(&mut self) -> u64 {
        now_ms()
    }
}

/// 聊天室 full-access 对 Computer Use 的受限批准映射。
///
/// 它只批准普通本地可逆动作和有状态动作；Sensitive/ForbiddenOrAmbiguous 永不放行。
/// core 控制器还会在调用此策略前独立拦截删除、支付、外发、授权安装、凭据等宿主
/// 语义敏感动作，因此 full-access 不能被用作这些高风险动作的通配批准。
struct RoomFullAccessApprovalPolicy;

impl ComputerUseApprovalPolicy for RoomFullAccessApprovalPolicy {
    fn is_action_approved(
        &self,
        _request: &ComputerUseRequest,
        action: &ComputerUseAction,
        _observation: &Observation,
    ) -> bool {
        matches!(
            action.risk,
            ComputerUseRiskClass::ReversibleLocal | ComputerUseRiskClass::Stateful
        )
    }
}

struct PersistingEventSink<'a> {
    store: &'a ComputerUseRunStore,
    call_id: &'a str,
    state_version: u64,
    persistence_error: Option<String>,
}

impl<'a> PersistingEventSink<'a> {
    fn new(store: &'a ComputerUseRunStore, call_id: &'a str) -> Self {
        Self {
            store,
            call_id,
            state_version: 0,
            persistence_error: None,
        }
    }
}

impl ComputerUseEventSink for PersistingEventSink<'_> {
    fn state_changed(&mut self, state: ComputerUseRunState) {
        if self.persistence_error.is_some() {
            return;
        }
        match self
            .store
            .transition(self.call_id, self.state_version, state, now_ms())
        {
            Ok(true) => self.state_version = self.state_version.saturating_add(1),
            Ok(false) => {
                self.persistence_error = Some(format!(
                    "computer-use state transition conflict at version {}",
                    self.state_version
                ));
            }
            Err(error) => {
                self.persistence_error = Some(format!(
                    "computer-use state transition could not be persisted: {error}"
                ));
            }
        }
    }
}

pub(crate) struct ComputerUseExecutor<'a> {
    planner: &'a dyn ComputerUsePlanner,
    adapters: &'a dyn ComputerUseAdapterFactory,
    store: &'a ComputerUseRunStore,
    budgets: ComputerUseBudgets,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    /// 接纳时冻结的工作区归属。**非 `Option`**：执行器在类型层面无法"没有归属"，
    /// 且构造后没有任何 setter——运行中不存在改写它的入口。
    workspace: CuWorkspaceAttribution,
}

impl<'a> ComputerUseExecutor<'a> {
    pub(crate) fn new(
        planner: &'a dyn ComputerUsePlanner,
        adapters: &'a dyn ComputerUseAdapterFactory,
        store: &'a ComputerUseRunStore,
        budgets: ComputerUseBudgets,
        workspace: CuWorkspaceAttribution,
    ) -> Self {
        Self {
            planner,
            adapters,
            store,
            budgets,
            cancelled: Arc::new(|| false),
            workspace,
        }
    }

    /// **仅供测试替身**：用显式固定的测试归属构造执行器，走与生产完全相同的写入路径。
    ///
    /// 生产代码无法取得"未归属的执行器"：唯一的非测试构造函数要求传入归属，
    /// 而归属只能由 [`CuWorkspaceAttribution::from_parent_run`] 从父运行的标识构造。
    #[cfg(test)]
    pub(crate) fn new_for_test(
        planner: &'a dyn ComputerUsePlanner,
        adapters: &'a dyn ComputerUseAdapterFactory,
        store: &'a ComputerUseRunStore,
        budgets: ComputerUseBudgets,
    ) -> Self {
        Self::new(planner, adapters, store, budgets, CuWorkspaceAttribution::test_fixture())
    }

    fn with_cancelled(mut self, cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        self.cancelled = cancelled;
        self
    }

    pub(crate) fn handles(&self, tool_name: &str) -> bool {
        // 正式名是 `computer_use_perform`（OpenAI 兼容协议要求 ^[a-zA-Z0-9_-]+$）；
        // 历史会话里持久化的是旧的点号写法，回放时同样要认。
        tool_name == crate::COMPUTER_USE_TOOL_NAME || tool_name == "computer_use.perform"
    }

    pub(crate) async fn execute(
        &self,
        input: &JsonValue,
        identity: &ToolCallIdentity,
    ) -> ComputerUseResult {
        self.execute_in_room(input, identity, None).await
    }

    pub(crate) async fn execute_in_room(
        &self,
        input: &JsonValue,
        identity: &ToolCallIdentity,
        chat_room_id: Option<&str>,
    ) -> ComputerUseResult {
        self.execute_in_room_with_policy(input, identity, chat_room_id, false)
            .await
    }

    /// 仅供已经完成聊天室 full-access 双确认校验的宿主路径调用。
    pub(crate) async fn execute_in_room_with_full_access(
        &self,
        input: &JsonValue,
        identity: &ToolCallIdentity,
        chat_room_id: Option<&str>,
    ) -> ComputerUseResult {
        self.execute_in_room_with_policy(input, identity, chat_room_id, true)
            .await
    }

    async fn execute_in_room_with_policy(
        &self,
        input: &JsonValue,
        identity: &ToolCallIdentity,
        chat_room_id: Option<&str>,
        room_full_access_approved: bool,
    ) -> ComputerUseResult {
        if let Ok(Some(existing)) = self.store.load(&identity.call_id) {
            if let Some(result) = existing.terminal_result {
                return result;
            }
            return terminal_result(
                identity,
                existing.surface,
                ComputerUseStage::Supervisor,
                ComputerUseError::blocked(
                    "duplicate_call_in_progress",
                    "the same provider tool call is already running",
                    ComputerUseRetryOwner::None,
                ),
            );
        }

        let (turn_count, invalid_inputs) = match self
            .store
            .turn_budget_counts(&identity.session_id, &identity.turn_id)
        {
            Ok(counts) => counts,
            Err(error) => {
                return terminal_result(
                    identity,
                    ComputerUseSurface::Auto,
                    ComputerUseStage::Supervisor,
                    persistence_error(error),
                )
            }
        };
        if invalid_inputs > MAX_INVALID_INPUTS_PER_TURN {
            let result = terminal_result(
                identity,
                ComputerUseSurface::Auto,
                ComputerUseStage::IntentGuard,
                limit_input_correction(
                    ComputerUseError::blocked(
                        "invalid_tool_input",
                        "input budget exhausted",
                        ComputerUseRetryOwner::None,
                    ),
                    invalid_inputs,
                ),
            );
            self.create_and_finish(
                input,
                identity,
                ComputerUseSurface::Auto,
                chat_room_id,
                &result,
            );
            return result;
        }
        let parsed = serde_json::from_value::<ComputerUseRequest>(input.clone());
        let requested_surface = parsed
            .as_ref()
            .map_or(ComputerUseSurface::Auto, |request| request.surface);

        let request = match parsed {
            Ok(request) => normalize_request_for_verification(request),
            Err(error) => {
                let result = terminal_result(
                    identity,
                    ComputerUseSurface::Auto,
                    ComputerUseStage::IntentGuard,
                    limit_input_correction(
                        ComputerUseError::blocked(
                            "invalid_tool_input",
                            format!("invalid computer-use request: {error}"),
                            ComputerUseRetryOwner::Model,
                        ),
                        invalid_inputs,
                    ),
                );
                self.create_and_finish(
                    input,
                    identity,
                    ComputerUseSurface::Auto,
                    chat_room_id,
                    &result,
                );
                return result;
            }
        };

        if let Err(error) = request.validate() {
            let result = terminal_result(
                identity,
                request.surface,
                ComputerUseStage::IntentGuard,
                limit_input_correction(error, invalid_inputs),
            );
            self.create_and_finish(input, identity, request.surface, chat_room_id, &result);
            return result;
        }

        let idempotency_key =
            TaskIdempotencyKey::new(&identity.session_id, &identity.turn_id, &request)
                .as_str()
                .to_string();

        if turn_count >= self.budgets.max_calls_per_turn {
            let result = terminal_result(
                identity,
                requested_surface,
                ComputerUseStage::Supervisor,
                ComputerUseError::blocked(
                    "recursive_call_blocked",
                    "computer-use call budget for this model turn is exhausted",
                    ComputerUseRetryOwner::None,
                ),
            );
            self.create_and_finish(input, identity, requested_surface, chat_room_id, &result);
            return result;
        }

        match self.store.load_by_idempotency_key(
            &identity.session_id,
            &identity.turn_id,
            &idempotency_key,
        ) {
            Ok(Some(existing)) => {
                let result = if let Some(result) = existing.terminal_result {
                    rebound_cached_result(result, identity)
                } else {
                    terminal_result(
                        identity,
                        existing.surface,
                        ComputerUseStage::Supervisor,
                        ComputerUseError::blocked(
                            "duplicate_task_in_progress",
                            "an identical computer-use task is already running in this model turn",
                            ComputerUseRetryOwner::None,
                        ),
                    )
                };
                self.create_and_finish(input, identity, result.surface, chat_room_id, &result);
                return result;
            }
            Err(error) => {
                let result = terminal_result(
                    identity,
                    request.surface,
                    ComputerUseStage::Supervisor,
                    persistence_error(error),
                );
                self.create_and_finish(input, identity, request.surface, chat_room_id, &result);
                return result;
            }
            Ok(None) => {}
        }

        let surface = match route_computer_use_surface(&request, &self.adapters.routing_context()) {
            Ok(surface) => surface,
            Err(error) => {
                let result = terminal_result(
                    identity,
                    request.surface,
                    ComputerUseStage::Classification,
                    error,
                );
                self.create_and_finish(input, identity, request.surface, chat_room_id, &result);
                return result;
            }
        };

        // ---- RPR-11c 第 3 项：CU 截止时间在**任务被接纳并进入调度**时建立 ----
        //
        // 位置是刻意的：这一行早于未确认释放互锁、租约等待、适配器构建与本地模型切换，
        // 也早于初始观察与规划请求。因此这些耗时全部落在同一个预算里，不会被隐藏。
        // 根 deadline 当前**未接线**：记录写成 `root_deadline_state = not_wired` +
        // `root_deadline = absent`，这不是"用户选择了无限时间"，只是本项目还没有把根
        // deadline 接进来。校验锚点见测试
        // `cu_deadline_is_established_before_interlock_lease_and_observation`。
        let cu_deadline = CuDeadline::establish_without_root(&self.budgets, now_ms());
        if !self.create_run(
            input,
            identity,
            surface,
            chat_room_id,
            Some(&idempotency_key),
            Some(&cu_deadline),
        ) {
            if let Ok(Some(existing)) = self.store.load(&identity.call_id) {
                if let Some(result) = existing.terminal_result {
                    return result;
                }
            }
            return terminal_result(
                identity,
                surface,
                ComputerUseStage::Supervisor,
                ComputerUseError::blocked(
                    "persistence_conflict",
                    "computer-use run could not be created exactly once",
                    ComputerUseRetryOwner::None,
                ),
            );
        }

        let _pending_run = PendingRunGuard {
            store: self.store,
            identity,
            surface,
            cu_budget: cu_deadline.facts(),
        };

        // ---- CU-F03（裁决 §5.1 第 6 条）：旧活动运行缺工作区归属的互锁 ----
        //
        // "旧活动运行"= 同一 scope 内 `terminal_result_json IS NULL` 且 state 非终态、
        // 且 `workspace_id IS NULL` 的运行（迁移之前写入、归属确实没有记录的那种）。
        // 位置与释放互锁相同且同样早于任何输入：只查证、不回填——历史归属永远保持 NULL。
        // 命中时**不启动新的业务输入**，但本次 run 照常落终态（必要收尾与原始证据保存）。
        match unrecorded_workspace_interlock(self.store.unrecorded_workspace_active_runs(
            &identity.session_id,
            &identity.turn_id,
            &identity.call_id,
        )) {
            UnrecordedWorkspaceInterlock::Clear => {}
            UnrecordedWorkspaceInterlock::UnrecordedActiveRuns(count) => {
                let result = terminal_result(
                    identity,
                    surface,
                    ComputerUseStage::Supervisor,
                    unrecorded_workspace_interlock_error(&identity.call_id, count),
                );
                self.finish_at_version(&result, 0);
                return result;
            }
            UnrecordedWorkspaceInterlock::Unverifiable(detail) => {
                let result = terminal_result(
                    identity,
                    surface,
                    ComputerUseStage::Supervisor,
                    unrecorded_workspace_check_failed_error(&identity.call_id, &detail),
                );
                self.finish_at_version(&result, 0);
                return result;
            }
        }

        // ---- RPR-05b-1：未确认释放的跨 run 互锁 ----
        //
        // 位置是刻意的：这里还没有取得桌面输入 lease、还没有构建 adapter、
        // 也没有任何原生输入边界被调用（下面那段 acquire 才第一次接触输入所有权）。
        // 事实来源只有 sqlite：历史 `input_release_status='unknown'` 只追加不改写，
        // 因此重启不会清空互锁；解除只能由人工追加解除事实（store 侧 API）。
        match release_interlock_decision(
            self.store
                .unconfirmed_release_facts(&identity.session_id, &identity.turn_id),
        ) {
            ReleaseInterlockDecision::Clear => {}
            ReleaseInterlockDecision::Unresolved(unresolved_runs) => {
                let result = terminal_result(
                    identity,
                    surface,
                    ComputerUseStage::Supervisor,
                    unconfirmed_release_interlock_error(&identity.call_id, unresolved_runs),
                );
                self.finish_at_version(&result, 0);
                return result;
            }
            ReleaseInterlockDecision::Unverifiable(detail) => {
                let result = terminal_result(
                    identity,
                    surface,
                    ComputerUseStage::Supervisor,
                    unconfirmed_release_check_failed_error(&identity.call_id, &detail),
                );
                self.finish_at_version(&result, 0);
                return result;
            }
        }

        #[cfg(windows)]
        let desktop_input_lease = if surface == ComputerUseSurface::Desktop {
            let scope = match windows_process_guard::current_interactive_session_scope() {
                Ok(scope) => scope,
                Err(error) => {
                    let result = terminal_result(
                        identity,
                        surface,
                        ComputerUseStage::Supervisor,
                        ComputerUseError::blocked(
                            "input_lease_scope_unavailable",
                            format!("unable to resolve Windows input session scope: {error}"),
                            ComputerUseRetryOwner::System,
                        ),
                    );
                    self.finish_at_version(&result, 0);
                    return result;
                }
            };
            // 跨进程所有权：桌面 CU 取得的是"进程内 lease + 命名内核对象"的合成所有权，
            // 因此另一个进程也无法在本次 run 期间取得同一交互 scope。
            match windows_process_guard::ScopedInputOwnership::acquire(
                windows_process_guard::interactive_input_lease_broker(),
                &scope,
                &identity.call_id,
                std::time::Duration::ZERO,
            ) {
                Ok(ownership) => Some(ownership),
                Err(error) => {
                    let result = terminal_result(
                        identity,
                        surface,
                        ComputerUseStage::Supervisor,
                        ComputerUseError::blocked(
                            if error.is_busy() {
                                "input_owner_busy"
                            } else {
                                "input_lease_scope_unavailable"
                            },
                            format!("desktop input is not available: {error}"),
                            ComputerUseRetryOwner::System,
                        ),
                    );
                    self.finish_at_version(&result, 0);
                    return result;
                }
            }
        } else {
            None
        };
        let adapter = match self.adapters.build(surface) {
            Ok(adapter) => adapter,
            Err(error) => {
                let result =
                    terminal_result(identity, surface, ComputerUseStage::Observation, error);
                self.finish_at_version(&result, 0);
                return result;
            }
        };

        let adapter = TracingAdapter {
            inner: adapter,
            store: self.store,
            call_id: identity.call_id.clone(),
            state: Mutex::new(TraceState {
                observation: None,
                next_index: 0,
                pending: None,
            }),
            cancelled: self.cancelled.clone(),
            #[cfg(windows)]
            input_lease: desktop_input_lease.as_ref(),
        };
        let event_sink = PersistingEventSink::new(self.store, &identity.call_id);
        let mut controller = ComputerUseController::new(
            PlannerRef {
                planner: self.planner,
                store: self.store,
                call_id: &identity.call_id,
            },
            adapter,
            event_sink,
            SystemClock,
            self.budgets,
        );
        if room_full_access_approved {
            controller = controller.with_approval_policy(RoomFullAccessApprovalPolicy);
        }
        // 控制器只用这一个截止时间：租约等待、适配器构建（含模型切换）之后才走到这里，
        // 但剩余的预算仍然是"接纳时刻"起算的那一份，不会被重新起算。
        let mut controller = controller.with_cu_deadline(cu_deadline);
        let mut result = controller
            .run(
                &ComputerUseRequest { surface, ..request },
                ComputerUseRunContext {
                    call_id: identity.call_id.clone(),
                    provider_tool_call_id: Some(identity.provider_tool_call_id.clone()),
                },
            )
            .await;
        let sink = controller.event_sink();
        let state_version = sink.state_version;
        if let Some(error) = &sink.persistence_error {
            result = terminal_result(
                identity,
                surface,
                ComputerUseStage::Supervisor,
                ComputerUseError::new(
                    "persistence_error",
                    error.clone(),
                    true,
                    ComputerUseRetryOwner::System,
                ),
            );
        }

        self.finish_at_version(&result, state_version)
    }

    fn create_and_finish(
        &self,
        input: &JsonValue,
        identity: &ToolCallIdentity,
        surface: ComputerUseSurface,
        chat_room_id: Option<&str>,
        result: &ComputerUseResult,
    ) {
        // 参数未通过 / 尚未接纳执行的请求：沿用既有参数纠错规则，
        // 不建立 CU 截止时间（记录里的 `cu_deadline_ms` 因此为 `None`），
        // 也**不伪装成已经启动的 CU 任务**。
        if self.create_run(input, identity, surface, chat_room_id, None, None) {
            let _ = self.store.finish(&identity.call_id, 0, result);
        }
    }

    fn create_run(
        &self,
        input: &JsonValue,
        identity: &ToolCallIdentity,
        surface: ComputerUseSurface,
        chat_room_id: Option<&str>,
        idempotency_key: Option<&str>,
        cu_deadline: Option<&CuDeadline>,
    ) -> bool {
        let created_at_ms = cu_deadline.map_or_else(now_ms, |deadline| deadline.established_at_ms());
        let deadline_ms = cu_deadline.map_or_else(
            || created_at_ms.saturating_add(self.budgets.timeout_ms),
            |deadline| deadline.cu_deadline_ms(),
        );
        let idempotency_key = idempotency_key
            .map(str::to_string)
            .or_else(|| {
                serde_json::from_value::<ComputerUseRequest>(input.clone())
                    .ok()
                    .map(|request| {
                        TaskIdempotencyKey::new(&identity.session_id, &identity.turn_id, &request)
                            .as_str()
                            .to_string()
                    })
            })
            .unwrap_or_else(|| identity.call_id.clone());
        self.store
            .create_run(&NewComputerUseRun {
                call_id: identity.call_id.clone(),
                provider_tool_call_id: Some(identity.provider_tool_call_id.clone()),
                session_id: identity.session_id.clone(),
                turn_id: identity.turn_id.clone(),
                chat_room_id: chat_room_id.map(str::to_string),
                idempotency_key,
                objective_json: serde_json::to_string(input).unwrap_or_else(|_| "null".into()),
                surface,
                deadline_ms,
                created_at_ms,
                workspace: self.workspace.clone(),
            })
            .unwrap_or(false)
    }

    fn finish_at_version(
        &self,
        result: &ComputerUseResult,
        state_version: u64,
    ) -> ComputerUseResult {
        match self.store.finish(&result.call_id, state_version, result) {
            Ok(true) => result.clone(),
            Ok(false) => self
                .store
                .load(&result.call_id)
                .ok()
                .flatten()
                .and_then(|run| run.terminal_result)
                .unwrap_or_else(|| {
                    // 落盘冲突也必须保留已经建立过的 CU 预算事实。
                    preserve_run_facts(
                        terminal_result(
                            &identity_from_result(result),
                            result.surface,
                            ComputerUseStage::Supervisor,
                            ComputerUseError::blocked(
                                "persistence_conflict",
                                "terminal computer-use result was not written exactly once",
                                ComputerUseRetryOwner::None,
                            ),
                        ),
                        result,
                    )
                }),
            Err(error) => {
                // 落盘失败也不能抹掉已经发生的输入事实：把原结果里的回执带到新错误上。
                let receipt = result
                    .error
                    .as_ref()
                    .and_then(|error| error.receipt.clone());
                let mut failure = persistence_error(error);
                if let Some(receipt) = receipt {
                    failure = failure.with_receipt(receipt);
                }
                preserve_run_facts(
                    terminal_result(
                        &identity_from_result(result),
                        result.surface,
                        ComputerUseStage::Supervisor,
                        failure,
                    ),
                    result,
                )
            }
        }
    }
}

/// 把已经建立过的 CU 预算/收尾事实带到替换后的结果上，避免落盘路径把它们抹掉。
fn preserve_run_facts(mut replacement: ComputerUseResult, source: &ComputerUseResult) -> ComputerUseResult {
    if replacement.cu_budget.is_none() {
        replacement.cu_budget = source.cu_budget;
    }
    if replacement.cleanup.is_none() {
        replacement.cleanup = source.cleanup;
    }
    replacement
}

/// 步骤记录里的输入事实列：`ActionReceipt` 在持久化层的投影。
struct SteppedFacts {
    input_delivery: runtime::InputDelivery,
    partial: Option<bool>,
    path_completed: Option<bool>,
    confirmed_point_count: Option<u32>,
    input_release_status: runtime::InputReleaseStatus,
}

fn apply_stepped_facts(step: &mut ComputerUseStepRecord, facts: SteppedFacts) {
    step.input_delivery = Some(facts.input_delivery);
    step.partial = facts.partial;
    step.path_completed = facts.path_completed;
    step.confirmed_point_count = facts.confirmed_point_count;
    step.input_release_status = Some(facts.input_release_status);
}

/// 校验回执身份与自洽性之后才投影成持久化列。
///
/// 身份不匹配（回执属于别的动作）或自相矛盾时返回 `None`：这是协议异常，
/// 既不能把事实接纳到当前动作，也不能因为读不懂就退化成"没有输入"。
fn trusted_receipt_facts(
    receipt: &runtime::ActionReceipt,
    expected_action_id: &str,
) -> Option<SteppedFacts> {
    if receipt.action_id != expected_action_id || receipt.validate().is_err() {
        return None;
    }
    Some(SteppedFacts {
        input_delivery: receipt.input_delivery,
        partial: receipt.partial,
        path_completed: receipt.path_completed,
        confirmed_point_count: receipt.confirmed_point_count,
        input_release_status: receipt.input_release,
    })
}

/// 协议异常（或适配器只报了"发出去了"）时的保守取值：
/// 不写路径事实，释放必须未确认，输入绝不降格成"零输入"。
fn conservative_facts(input_confirmed_sent: bool) -> SteppedFacts {
    SteppedFacts {
        input_delivery: if input_confirmed_sent {
            runtime::InputDelivery::Sent
        } else {
            runtime::InputDelivery::MayHaveBeenSent
        },
        partial: None,
        path_completed: None,
        confirmed_point_count: None,
        input_release_status: runtime::InputReleaseStatus::Unknown,
    }
}

fn step_release_status(value: computer_use::StepInputReleaseStatus) -> runtime::InputReleaseStatus {
    match value {
        computer_use::StepInputReleaseStatus::NotNeeded => runtime::InputReleaseStatus::NotNeeded,
        computer_use::StepInputReleaseStatus::Released => runtime::InputReleaseStatus::Released,
        computer_use::StepInputReleaseStatus::Unknown => runtime::InputReleaseStatus::Unknown,
    }
}

/// 跨 run 互锁的判定结果。
///
/// 关键性质：**读不到事实时绝不放行**。互锁是安全闸门，"查不出来"与"没有未确认释放"
/// 必须区别对待，否则一次读失败就等于给桌面开了后门。
enum ReleaseInterlockDecision {
    /// 本 scope 没有未解除的未确认释放，可以继续。
    Clear,
    /// 存在 `n` 个未解除的未确认释放：阻断本次 run。
    Unresolved(usize),
    /// 事实读不出来：按最保守方向阻断（附具体原因）。
    Unverifiable(String),
}

fn release_interlock_decision(
    facts: Result<crate::computer_use_store::UnconfirmedReleaseFacts, rusqlite::Error>,
) -> ReleaseInterlockDecision {
    match facts {
        Ok(facts) if facts.is_empty() => ReleaseInterlockDecision::Clear,
        Ok(facts) => ReleaseInterlockDecision::Unresolved(facts.run_ids.len()),
        Err(error) => ReleaseInterlockDecision::Unverifiable(error.to_string()),
    }
}

/// run 级（尚无具体动作）的输入前拒绝回执身份。
///
/// 以 `call_id` 为主体，与 `action_attempt_id` 的 `surface:target:hash` 形式不可能重合，
/// 因此不会被误当成某个动作的回执接纳。
fn run_scope_pre_input_receipt(call_id: &str) -> runtime::ActionReceipt {
    computer_use::input::pre_input_receipt(&format!("run-scope:{call_id}"), false)
}

/// RPR-05b-1：本 turn 内还有未解除的未确认释放 → 拒绝新输入。
///
/// 这是**输入前拒绝**，回执按既有的 `pre_input(...)` 语义写成"可证明未发送、
/// 无释放义务"，不留任何"可能已发送"的含混空间。
fn unconfirmed_release_interlock_error(call_id: &str, unresolved_runs: usize) -> ComputerUseError {
    ComputerUseError::blocked(
        "input_release_unconfirmed_interlock",
        format!(
            "an earlier computer-use run in this turn left input with an unconfirmed release \
             ({unresolved_runs} run(s) still unresolved); new input is blocked until a human \
             resolves the residual input and records that resolution"
        ),
        ComputerUseRetryOwner::User,
    )
    .with_receipt(run_scope_pre_input_receipt(call_id))
}

/// 无法读取释放事实时的阻断：同属输入前拒绝，绝不因为读不出来而放行输入。
fn unconfirmed_release_check_failed_error(call_id: &str, detail: &str) -> ComputerUseError {
    ComputerUseError::blocked(
        "input_release_interlock_check_failed",
        format!("computer-use input-release facts could not be read: {detail}"),
        ComputerUseRetryOwner::System,
    )
    .with_receipt(run_scope_pre_input_receipt(call_id))
}

/// 旧活动运行的**工作区归属缺失**互锁的判定结果（CU-F03，裁决 §5.1 第 6 条）。
///
/// 判据与释放互锁同款：**读不到事实时绝不放行**（"查不出来"≠"没有这类运行"）。
enum UnrecordedWorkspaceInterlock {
    /// 本 scope 没有"仍未收尾且归属未记录"的旧运行。
    Clear,
    /// 存在这样的旧运行：不启动新的业务输入，只允许必要收尾与原始证据保存。
    UnrecordedActiveRuns(usize),
    /// 读不出来：按最保守方向阻断。
    Unverifiable(String),
}

fn unrecorded_workspace_interlock(
    active: Result<Vec<String>, rusqlite::Error>,
) -> UnrecordedWorkspaceInterlock {
    match active {
        Ok(active) if active.is_empty() => UnrecordedWorkspaceInterlock::Clear,
        Ok(active) => UnrecordedWorkspaceInterlock::UnrecordedActiveRuns(active.len()),
        Err(error) => UnrecordedWorkspaceInterlock::Unverifiable(error.to_string()),
    }
}

/// 旧活动运行缺工作区归属 → 拒绝新输入。
///
/// 这是**输入前拒绝**：本次 run 会记一条终态（收尾与证据保存照常），但不产生任何物理输入。
fn unrecorded_workspace_interlock_error(call_id: &str, count: usize) -> ComputerUseError {
    ComputerUseError::blocked(
        "workspace_unrecorded_active_run",
        format!(
            "an earlier computer-use run in this turn is still open without a recorded workspace \
             ({count} run(s)); new business input is blocked until those runs are wrapped up"
        ),
        ComputerUseRetryOwner::System,
    )
    .with_receipt(run_scope_pre_input_receipt(call_id))
}

/// 无法读取归属列时的阻断：同属输入前拒绝，绝不因为读不出来而放行输入。
fn unrecorded_workspace_check_failed_error(call_id: &str, detail: &str) -> ComputerUseError {
    ComputerUseError::blocked(
        "workspace_attribution_check_failed",
        format!("computer-use run workspace attribution could not be read: {detail}"),
        ComputerUseRetryOwner::System,
    )
    .with_receipt(run_scope_pre_input_receipt(call_id))
}

fn limit_input_correction(error: ComputerUseError, invalid_inputs: usize) -> ComputerUseError {
    if invalid_inputs >= MAX_INVALID_INPUTS_PER_TURN {
        let receipt = error.receipt.clone();
        let mut replaced = ComputerUseError::blocked("input_correction_budget_exhausted",
            "computer-use input correction budget is exhausted; no observation or UI input was performed for this request",
            ComputerUseRetryOwner::None);
        // 换错误码不等于可以丢掉已知的输入事实：包装点必须把回执一起带走。
        if let Some(receipt) = receipt {
            replaced = replaced.with_receipt(receipt);
        }
        replaced
    } else {
        error
    }
}

fn identity_from_result(result: &ComputerUseResult) -> ToolCallIdentity {
    ToolCallIdentity {
        call_id: result.call_id.clone(),
        provider_tool_call_id: result.provider_tool_call_id.clone().unwrap_or_default(),
        session_id: String::new(),
        turn_id: String::new(),
    }
}

fn persistence_error(error: impl std::fmt::Display) -> ComputerUseError {
    ComputerUseError::new(
        "persistence_error",
        format!("computer-use persistence failed: {error}"),
        true,
        ComputerUseRetryOwner::System,
    )
}

fn cancelled_error() -> ComputerUseError {
    ComputerUseError::blocked(
        "cancelled",
        "originating chat turn was interrupted; no further input is allowed",
        ComputerUseRetryOwner::None,
    )
}

/// 该失败是否可能发生在输入已经发出之后。
///
/// §2.2 恢复表要求四个维度最终都明确：只有**执行器在进入 adapter 之前**就会拒绝的码
/// 才能记 `not_sent`；其余（helper 中途失败、释放未确认等）一律记 `may_have_been_sent`，
/// 并把释放记为 `unknown`——"可能已发送"不能推出"已释放"，更不能填 `released`。
fn failure_may_have_sent_input(code: &str) -> bool {
    !matches!(
        code,
        "input_lease_lost"
            | "input_lease_scope_unavailable"
            | "input_owner_busy"
            | "persistence_error"
            // 派发边界（同一临界区复核 + 派发）自身的拒绝：dispatch 从未被调用。
            | "input_dispatch_reentrant"
            | "budget_exhausted"
    )
}

fn lease_lost_error() -> ComputerUseError {
    ComputerUseError::blocked(
        "input_lease_lost",
        "desktop input owner epoch is no longer current",
        ComputerUseRetryOwner::System,
    )
}

/// 派发边界拒绝 → 既有错误码（复用已登记的 pre-input 语义，不新增语义层级）。
///
/// 这些拒绝**全部发生在输入发出之前**（`dispatch` 从未被调用），因此它们都属于
/// pre-input 拒绝，必须出现在 `failure_may_have_sent_input` 的例外表里。
fn input_dispatch_refusal_error(
    refusal: &windows_process_guard::InputDispatchRefusal,
    remaining: std::time::Duration,
) -> ComputerUseError {
    if let Some(label) = refusal.guard_label() {
        return match label {
            "cancelled" => cancelled_error(),
            "execution_budget_exhausted" => ComputerUseError::blocked(
                "budget_exhausted",
                format!(
                    "execution cannot start: remaining computer-use budget {} ms is exhausted; \
                     no input was sent",
                    remaining.as_millis()
                ),
                ComputerUseRetryOwner::None,
            ),
            _ => ComputerUseError::blocked(
                "input_lease_lost",
                format!("input dispatch refused by guard {label}"),
                ComputerUseRetryOwner::System,
            ),
        };
    }
    if refusal.is_reentrant() {
        return ComputerUseError::blocked(
            "input_dispatch_reentrant",
            format!("input dispatch refused: {refusal}"),
            ComputerUseRetryOwner::System,
        );
    }
    lease_lost_error()
}

fn terminal_result(
    identity: &ToolCallIdentity,
    surface: ComputerUseSurface,
    stage: ComputerUseStage,
    error: ComputerUseError,
) -> ComputerUseResult {
    let status = if error.code == "cancelled" {
        ComputerUseTerminalStatus::Cancelled
    } else if error.code == "deadline_exceeded" {
        ComputerUseTerminalStatus::TimedOut
    } else if error.retryable {
        ComputerUseTerminalStatus::Failed
    } else {
        ComputerUseTerminalStatus::Blocked
    };
    ComputerUseResult {
        call_id: identity.call_id.clone(),
        provider_tool_call_id: Some(identity.provider_tool_call_id.clone()),
        status,
        stage,
        goal_achieved: false,
        surface,
        summary: error.message.clone(),
        error: Some(error),
        attempts: 0,
        steps_completed: 0,
        evidence: Vec::new(),
        supervisor: SupervisorSnapshot {
            circuit_open: false,
            reason: None,
            action_count: 0,
            replan_count: 0,
            no_progress_count: 0,
        },
        // 默认是"还没被接纳成 CU 任务"：没有 CU 截止时间，也不伪造成已经启动。
        cu_budget: Some(CuBudgetFacts::not_accepted()),
        cleanup: None,
    }
}

fn rebound_cached_result(
    mut result: ComputerUseResult,
    identity: &ToolCallIdentity,
) -> ComputerUseResult {
    result.call_id = identity.call_id.clone();
    result.provider_tool_call_id = Some(identity.provider_tool_call_id.clone());
    result.summary = format!(
        "cached terminal result for identical computer-use task in this model turn: {}",
        result.summary
    );
    result
}

fn normalize_request_for_verification(mut request: ComputerUseRequest) -> ComputerUseRequest {
    let mut success_criteria = Vec::new();
    for criterion in std::mem::take(&mut request.success_criteria) {
        let criterion = criterion.trim();
        if criterion.is_empty() {
            continue;
        }
        if is_execution_constraint_criterion(criterion) {
            if !request
                .constraints
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(criterion))
            {
                request.constraints.push(criterion.to_string());
            }
        } else {
            success_criteria.push(criterion.to_string());
        }
    }
    if success_criteria.is_empty() {
        success_criteria.push(request.objective.trim().to_string());
    }
    request.success_criteria = success_criteria;
    request
}

fn is_execution_constraint_criterion(criterion: &str) -> bool {
    let lower = criterion.to_lowercase();
    let mentions_coordinate = lower.contains("coordinate")
        || lower.contains("desktop coord")
        || criterion.contains("坐标");
    let mentions_dom = lower.contains(" dom")
        || lower.contains("dom ")
        || lower.contains("dom引用")
        || lower.contains("dom 引用")
        || lower.contains("dom reference");
    let forbids_or_limits = lower.contains("only")
        || lower.contains("without")
        || lower.contains("not use")
        || lower.contains("do not use")
        || lower.contains("never use")
        || criterion.contains("只")
        || criterion.contains("仅")
        || criterion.contains("不能")
        || criterion.contains("不使用")
        || criterion.contains("未使用")
        || criterion.contains("禁止");
    (mentions_coordinate || mentions_dom) && forbids_or_limits
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

struct ProductionAdapterFactory {
    desktop_enabled: bool,
    browser_enabled: bool,
    browser_policy: BrowserComputerUsePolicy,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl ComputerUseAdapterFactory for ProductionAdapterFactory {
    fn routing_context(&self) -> SurfaceRoutingContext {
        SurfaceRoutingContext {
            foreground_is_webview2: false,
            desktop_available: self.desktop_enabled,
            browser_available: self.browser_enabled,
        }
    }

    fn build(
        &self,
        surface: ComputerUseSurface,
    ) -> Result<DynComputerUseAdapter, ComputerUseError> {
        match surface {
            ComputerUseSurface::Desktop if self.desktop_enabled => {
                DesktopNativeBridge::preflight()?;
                Ok(DynComputerUseAdapter::new(DesktopComputerUseAdapter::new(
                    DesktopNativeBridge::with_cancelled(self.cancelled.clone()),
                )))
            }
            ComputerUseSurface::Browser if self.browser_enabled => {
                BrowserNativeBridge::preflight()?;
                Ok(DynComputerUseAdapter::new(
                    BrowserComputerUseAdapter::with_policy(
                        BrowserNativeBridge::default(),
                        self.browser_policy,
                    ),
                ))
            }
            ComputerUseSurface::Auto => Err(backend_error(
                "computer-use adapter cannot be built for an automatic surface",
            )),
            _ => Err(backend_error(&format!(
                "{} adapter is disabled by configuration",
                surface.as_str()
            ))),
        }
    }
}

pub(crate) async fn execute_with_current_runtime(
    input: &JsonValue,
    identity: &ToolCallIdentity,
    chat_room_id: Option<&str>,
    // 父运行**接纳时**冻结的上下文（裁决 §5.1 / B-5）：工作区归属只能取自它。
    // 上下文缺失或工作区为空即 fail-closed 拒绝，**不得**回落到「当前工作区」。
    parent: Option<&crate::FrozenParentContext>,
) -> ComputerUseResult {
    let config = crate::read_config(|config| config.computer_use.clone());
    if !config.enabled || config.tool_mode != "task-controller" {
        return terminal_result(
            identity,
            ComputerUseSurface::Auto,
            ComputerUseStage::IntentGuard,
            ComputerUseError::blocked(
                "computer_use_disabled",
                "computer-use task controller is disabled by configuration",
                ComputerUseRetryOwner::User,
            ),
        );
    }
    // 裁决 §5.1 + A-2：普通 CU 在**输入前**缺必需执行上下文即拒绝。工作区归属必须来自
    // 父运行的**接纳上下文**；缺失时不得执行，也**不得**用「当前工作区」顶替。
    // 上下文的工作区已由**源头唯一解析器**校验并封装成 `CanonicalWorkspaceId`，因此执行器这里只剩
    // "有/没有上下文"一个问题：**值是否合规不再由 CU 判定**（CU 不再持有第二份格式规则）。
    // 注意：四维（room/session/turn/parent_run）目前只被携带、尚未在此消费，其可信关系核对属 A2-T5/T6，不得假装已校验。
    let Some(parent) = parent else {
        return terminal_result(
            identity,
            ComputerUseSurface::Auto,
            ComputerUseStage::IntentGuard,
            ComputerUseError::blocked(
                "input_context_incomplete",
                "computer-use requires a frozen workspace from the parent run; none was provided",
                ComputerUseRetryOwner::User,
            ),
        );
    };
    // 归属值从冻结上下文里取出即定型；此后本函数只引用这一个已解析值。
    let frozen_workspace = &parent.workspace_id;
    // ---- CU-F03（裁决 §5.1）：把父运行的工作区**冻结**成不可变归属 ----
    //
    // 1. 归属值在**这一行**定型：此后执行器与 store 全程沿用同一个值（`workspace`），
    //    中途切换 UI 工作区不会改变它——这里**没有**第二次读取「当前工作区」的路径。
    //    （planner 的规划请求与诊断不写工作区维度，今天没有可传之处；一旦它开始写带工作区
    //    维度的事实，必须沿用这一个值，而不是自己再解析一次。）
    // 2. 禁止值（数据库路径 / 当前目录 / 窗口名 / 模型提供字符串 / 占位值）在**接纳点**就被
    //    源头解析器拒绝（口径 §10.4 决定一），因此这里取到的值**已是合规标识**，
    //    不再需要也不得再做第二次格式判定。
    let workspace = CuWorkspaceAttribution::from_parent_run(frozen_workspace);
    // 运行库路径同样在**接纳时**定型：本次运行只写这一个库，运行期间切换工作区不会把
    // 后续操作（含房间授权复核）指向另一个库（裁决 T13 的「数据库不变」面）。
    let admission_db_path = crate::default_session_sqlite_path();
    // A-2 §3 / A2-T5：四维**可信关系核对**。必须在**任何 store 操作与任何原生输入之前**，
    // 且不以"同一结构体字段互相比"代替权威查询。
    if let Err(violation) = crate::validate_frozen_parent_relations(&admission_db_path, parent) {
        return terminal_result(
            identity,
            ComputerUseSurface::Auto,
            ComputerUseStage::IntentGuard,
            ComputerUseError::blocked(
                violation.code(),
                format!("computer-use parent context rejected: {}", violation.reason()),
                ComputerUseRetryOwner::User,
            ),
        );
    }
    // 第八轮 §1.4：**所有正式输入入口**都必须经共享输入安全库检查资源状态。
    // 顺序上排在身份/关系校验**之后**（先确认"是谁在问"，再问"现在能不能输入"），
    // 位置在任何 store 写入与任何原生输入**之前**；未注入库根 / 库不可读 / 资源未开放
    // ⇒ 一律 fail-closed（`Unknown` 与 `Isolated` 都不接受新输入）。
    let input_resource_scope = match crate::input_safety_store::physical_input_resource_scope() {
        Ok(scope) => scope,
        Err(refusal) => {
            return terminal_result(
                identity,
                ComputerUseSurface::Auto,
                ComputerUseStage::IntentGuard,
                ComputerUseError::blocked(
                    refusal.code(),
                    format!("computer-use input admission rejected: {}", refusal.reason()),
                    ComputerUseRetryOwner::User,
                ),
            );
        }
    };
    if let Err(refusal) =
        crate::input_safety_store::require_resource_accepts_new_input(&input_resource_scope)
    {
        return terminal_result(
            identity,
            ComputerUseSurface::Auto,
            ComputerUseStage::IntentGuard,
            ComputerUseError::blocked(
                refusal.code(),
                format!("computer-use input admission rejected: {}", refusal.reason()),
                ComputerUseRetryOwner::User,
            ),
        );
    }
    let store = match ComputerUseRunStore::open(&admission_db_path) {
        Ok(store) => store,
        Err(error) => {
            return terminal_result(
                identity,
                ComputerUseSurface::Auto,
                ComputerUseStage::Supervisor,
                persistence_error(format!("open computer-use run store: {error}")),
            );
        }
    };
    let host_cancelled = crate::tool_turn_cancellation_checker(&identity.turn_id);
    let dropped = Arc::new(AtomicBool::new(false));
    struct CancelOnDrop(Arc<AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let _cancel_on_drop = CancelOnDrop(dropped.clone());
    let cancelled: Arc<dyn Fn() -> bool + Send + Sync> =
        Arc::new(move || host_cancelled() || dropped.load(Ordering::SeqCst));
    let adapters = ProductionAdapterFactory {
        desktop_enabled: config.desktop.enabled,
        browser_enabled: config.browser.enabled,
        browser_policy: BrowserComputerUsePolicy {
            allow_drag: config.browser.allow_drag,
            allow_key_combinations: config.browser.allow_key_combinations,
            allow_multiple_tabs: config.browser.allow_multiple_tabs,
        },
        cancelled: cancelled.clone(),
    };
    let planner = CurrentSessionComputerUsePlanner::with_context(identity, chat_room_id, &store)
        .with_cancelled(cancelled.clone());
    let executor =
        ComputerUseExecutor::new(&planner, &adapters, &store, config.budgets(), workspace)
            .with_cancelled(cancelled);
    // 授权复核也读**同一份冻结的库路径**：运行期间切换工作区不会把这次复核指向另一个库。
    let room_grant =
        crate::room_permission_grant_view_for_path(&admission_db_path, chat_room_id);
    if !room_grant.session_authorized || !room_grant.session_confirmed_twice {
        let result = terminal_result(
            identity,
            ComputerUseSurface::Auto,
            ComputerUseStage::IntentGuard,
            ComputerUseError::blocked(
                "computer_use_room_full_access_required",
                "computer-use requires full-access authorization for the originating chat room",
                ComputerUseRetryOwner::User,
            ),
        );
        executor.create_and_finish(
            input,
            identity,
            ComputerUseSurface::Auto,
            chat_room_id,
            &result,
        );
        return result;
    }
    executor
        .execute_in_room_with_full_access(input, identity, chat_room_id)
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use computer_use::{ComputerUseActionKind, ComputerUseRiskClass, ComputerUseTerminalStatus};
    use rusqlite::Connection;
    use serde_json::json;

    use super::*;
    use crate::computer_use_store::apply_session_migration_v11;
    use crate::computer_use_store::apply_session_migration_v22;

    #[test]
    fn task_controller_runtime_does_not_use_unavailable_backends() {
        let source = include_str!("computer_use_executor.rs");
        let runtime = source
            .split("pub(crate) async fn execute_with_current_runtime")
            .nth(1)
            .expect("runtime function")
            .split("#[cfg(test)]")
            .next()
            .expect("runtime function body");
        assert!(!runtime.contains("UnavailablePlanner"));
        assert!(!runtime.contains("UnavailableAdapterFactory"));
        assert!(runtime.contains("CurrentSessionComputerUsePlanner"));
        assert!(runtime.contains("ProductionAdapterFactory"));
        assert!(runtime.contains("room_permission_grant_view_for_path"));
        assert!(runtime.contains("computer_use_room_full_access_required"));
        assert!(runtime.contains("execute_in_room_with_full_access(input, identity, chat_room_id)"));
    }

    #[test]
    fn request_normalization_moves_dom_coordinate_constraints_out_of_success_criteria() {
        let request: ComputerUseRequest = serde_json::from_value(json!({
            "objective": "读取 Wikipedia 页面标题",
            "surface": "browser",
            "success_criteria": [
                "页面标题包含 Wikipedia",
                "仅使用 DOM 引用，未使用桌面坐标"
            ]
        }))
        .unwrap();

        let normalized = normalize_request_for_verification(request);

        assert_eq!(normalized.success_criteria, vec!["页面标题包含 Wikipedia"]);
        assert_eq!(
            normalized.constraints,
            vec!["仅使用 DOM 引用，未使用桌面坐标"]
        );
    }

    struct FakePlanner {
        actions: Mutex<Vec<ComputerUseAction>>,
    }

    impl FakePlanner {
        fn one_click() -> Self {
            Self::one_action(
                ComputerUseActionKind::Click,
                "submit",
                ComputerUseRiskClass::ReversibleLocal,
            )
        }

        fn one_action(
            kind: ComputerUseActionKind,
            target: impl Into<String>,
            risk: ComputerUseRiskClass,
        ) -> Self {
            Self {
                actions: Mutex::new(vec![ComputerUseAction {
                    kind,
                    target: target.into(),
                    arguments: json!({}),
                    risk,
                }]),
            }
        }
    }

    impl ComputerUsePlanner for FakePlanner {
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

        fn classify<'a>(
            &'a self,
            request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            _remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
            Box::pin(async move { Ok(request.surface) })
        }

        fn next_action<'a>(
            &'a self,
            _request: &'a ComputerUseRequest,
            _observation: &'a Observation,
            _step: usize,
            _remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
            Box::pin(async move { Ok(self.actions.lock().unwrap().pop()) })
        }
    }

    struct FakeAdapter {
        surface: ComputerUseSurface,
        generation: AtomicU64,
        action_count: Arc<AtomicUsize>,
        verify_count: AtomicUsize,
        achieve_after_action: bool,
        observe_delay_ms: u64,
    }

    impl ComputerUseAdapter for FakeAdapter {
        fn surface(&self) -> ComputerUseSurface {
            self.surface
        }

        fn capabilities(&self) -> ComputerUseCapabilities {
            ComputerUseCapabilities {
                navigate: true,
                click: true,
                submit: true,
                multiple_tabs: true,
                ..ComputerUseCapabilities::default()
            }
        }

        fn observe(
        &self,
        _request: &ComputerUseRequest,
        _remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
            // 模拟"本地模型切换 / 初始观察"的真实耗时：这部分时间必须落在 CU 预算里。
            if self.observe_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(self.observe_delay_ms));
            }
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Observation {
                generation,
                surface: self.surface,
                surface_identity: format!("{:?}-surface", self.surface),
                state: json!({"generation": generation}),
                evidence: vec![format!("evidence-{generation}.png")],
            })
        }

        fn act(
            &self,
            _action: &ComputerUseAction,
            _expected_generation: u64,
            _remaining: std::time::Duration,
        ) -> Result<StepExecution, ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            Ok(StepExecution {
                input_sent: true,
                summary: "input sent".into(),
                evidence: vec!["input.json".into()],
                partial: Some(false),
                path_completed: Some(true),
                confirmed_point_count: Some(2),
                input_release_status: Some(computer_use::StepInputReleaseStatus::Released),
                // 该替身模拟"没有回执"的旧适配器：由执行器回落到逐字段声明。
                receipt: None,
            })
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &Observation,
            _after: &Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, ComputerUseError> {
            let call = self.verify_count.fetch_add(1, Ordering::SeqCst);
            let achieved = self.achieve_after_action && call > 0;
            Ok(Verification {
                achieved,
                visible_progress: call > 0,
                summary: if achieved {
                    "success criterion is visibly satisfied".into()
                } else {
                    "success criterion is not yet visible".into()
                },
                evidence: vec![format!("verify-{call}.json")],
            })
        }
    }

    struct FakeFactory {
        context: SurfaceRoutingContext,
        achieve_after_action: bool,
        action_count: Arc<AtomicUsize>,
        backend_unavailable: bool,
        observe_delay_ms: u64,
    }

    impl ComputerUseAdapterFactory for FakeFactory {
        fn routing_context(&self) -> SurfaceRoutingContext {
            self.context
        }

        fn build(
            &self,
            surface: ComputerUseSurface,
        ) -> Result<DynComputerUseAdapter, ComputerUseError> {
            if self.backend_unavailable {
                return Err(backend_error("test backend unavailable"));
            }
            Ok(DynComputerUseAdapter::new(FakeAdapter {
                surface,
                generation: AtomicU64::new(0),
                action_count: Arc::clone(&self.action_count),
                verify_count: AtomicUsize::new(0),
                achieve_after_action: self.achieve_after_action,
                observe_delay_ms: self.observe_delay_ms,
            }))
        }
    }

    fn store() -> ComputerUseRunStore {
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();
        apply_session_migration_v22(&connection).unwrap();
        ComputerUseRunStore::from_connection(connection)
    }

    /// **只经 SQL** 造一条"旧活动运行"：归属未记录（`workspace_id IS NULL`）、仍非终态。
    ///
    /// 迁移之前写入的行就是这样；写入侧**不可能**造出这种行（`NewComputerUseRun::workspace`
    /// 非 `Option`），所以只能靠 store 提供的测试接缝。
    fn seed_unrecorded_active_run(store: &ComputerUseRunStore, call_id: &str) {
        crate::computer_use_store::seed_legacy_unrecorded_run_for_test(
            store, call_id, "session-1", "turn-1", true,
        );
    }

    fn identity(provider: &str) -> ToolCallIdentity {
        ToolCallIdentity::from_provider(provider, "session-1", "turn-1")
    }

    /// **正式输入入口的共享状态前置**：注入输入安全库根，并按资格把该资源开放。
    ///
    /// 这不是测试旁路：它走生产同一入口（协调器取锁 + 取 epoch + 按资格开放），
    /// 因此"资源接受新输入"是**真实发生过的事实**。返回库根路径供用例断言使用。
    /// 注入库根的**环境守卫**：drop 时恢复原值，避免进程级 env 泄漏到其它用例
    /// （此前 helper 只 `set_var` 不恢复，后续用例会读到已删除临时目录的库根）。
    struct InputSafetyEnvGuard {
        #[allow(dead_code)]
        coordination: String,
        previous: Option<std::ffi::OsString>,
    }

    impl Drop for InputSafetyEnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(
                    crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV,
                    value,
                ),
                None => std::env::remove_var(crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV),
            }
        }
    }

    fn open_input_resource_for_test(safety_root: &std::path::Path) -> InputSafetyEnvGuard {
        let previous = std::env::var_os(crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV);
        let scope = crate::input_safety_store::physical_input_resource_scope()
            .expect("physical input resource scope");
        let coordination = format!(
            "{}|test-{}",
            scope.as_str(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        );
        std::env::set_var(
            crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV,
            safety_root.as_os_str(),
        );
        let coordinator =
            crate::input_safety_store::InputSafetyCoordinator::begin_with_coordination_scope(
                safety_root,
                &coordination,
                &scope,
                "test-open-input",
                &["open_new_input"],
                std::time::Duration::from_millis(750),
            )
            .expect("coordinator");
        coordinator
            .store()
            .reopen_new_input_authorized(&scope, coordinator.control())
            .expect("按资格开放资源");
        std::mem::forget(coordinator); // 用例期内保持持有（释放会归还资源）
        InputSafetyEnvGuard {
            coordination,
            previous,
        }
    }

    /// 建一组**关系完整**的真实事实：会话行 + 房间行 + `chat_turn` 运行行（accepted）。
    /// 这是 A2-T5 的前提：只有库里真有这些行，"关系核对"才有意义。
    fn seed_relation_complete_admission(
        db_path: &std::path::Path,
        workspace_id: &str,
        room_id: &str,
        session_id: &str,
        run_id: &str,
    ) {
        let connection = crate::open_session_connection(db_path).expect("open");
        crate::initialize_session_schema(&connection).expect("schema");
        connection
            .execute_batch(&format!(
                "INSERT INTO sessions (id, name, provider, model, api_key_ref, created_at, updated_at) \
                 VALUES ('{session_id}', 'n', 'Custom', 'm', 'env', 1, 1);\
                 INSERT INTO chat_rooms (id, name, created_at, updated_at) \
                 VALUES ('{room_id}', 'room', 1, 1);\
                 INSERT INTO sessions (id, name, provider, model, api_key_ref, created_at, updated_at) \
                 VALUES ('session-2', 'n2', 'Custom', 'm', 'env', 1, 1);\
                 INSERT INTO chat_rooms (id, name, created_at, updated_at) \
                 VALUES ('room-2', 'room2', 1, 1);"
            ))
            .expect("seed session and room");
        drop(connection);
        crate::create_chat_runtime_run_sqlite(
            db_path,
            run_id,
            "claim-token",
            workspace_id,
            Some(session_id),
            room_id,
            "turn-1",
        )
        .expect("seed runtime run");
    }

    /// 父运行接纳上下文的最小构造：只有工作区维度，其余维度按真实缺省留空。
    ///
    /// 走的是**生产同一个构造器**：源头解析不通过就得不到上下文（A-2 合并顺序下没有测试侧旁路）。
    fn parent_context(workspace_id: &str) -> Option<crate::FrozenParentContext> {
        crate::FrozenParentContext::new("test-fixture", workspace_id, None, None, None, None).ok()
    }

    fn input(surface: &str) -> JsonValue {
        let target = if surface == "browser" {
            json!({"url": "https://example.invalid", "element": "submit"})
        } else {
            json!({"application": "notepad", "window": "Untitled"})
        };
        json!({
            "objective": "click submit",
            "surface": surface,
            "target": target,
            "success_criteria": ["success is visible"]
        })
    }

    fn factory(achieve_after_action: bool) -> FakeFactory {
        FakeFactory {
            context: SurfaceRoutingContext::default(),
            achieve_after_action,
            action_count: Arc::new(AtomicUsize::new(0)),
            backend_unavailable: false,
            observe_delay_ms: 0,
        }
    }

    /// RPR-11c：让初始观察/模型切换真实耗时，用于证明这段时间计入 CU 预算。
    fn slow_observe_factory(observe_delay_ms: u64) -> FakeFactory {
        FakeFactory {
            observe_delay_ms,
            ..factory(true)
        }
    }

    #[test]
    fn computer_use_executor_handles_only_the_formal_task_tool() {
        let store = store();
        let planner = FakePlanner::one_click();
        let factory = factory(true);
        let executor =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default());
        // 正式名（协议合法）与历史会话里的旧点号写法都要认。
        assert!(executor.handles(crate::COMPUTER_USE_TOOL_NAME));
        assert!(executor.handles("computer_use.perform"));
        // 其它工具一律不接管。
        assert!(!executor.handles("tools_semantic_dispatch"));
        assert!(!executor.handles("computer.left_click"));
    }

    #[tokio::test]
    async fn valid_desktop_and_browser_runs_succeed_only_after_visible_verification() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        for surface in ["desktop", "browser"] {
            let store = store();
            let planner = FakePlanner::one_click();
            let factory = factory(true);
            let result =
                ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                    .execute(&input(surface), &identity(&format!("tool-{surface}")))
                    .await;

            assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
            assert!(result.goal_achieved);
            assert_eq!(result.stage, ComputerUseStage::Terminal);
            assert_eq!(result.attempts, 1);
            assert_eq!(result.steps_completed, 1);
            assert!(!result.evidence.is_empty());
            assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);
            let persisted = store.load(&result.call_id).unwrap().unwrap();
            assert_eq!(persisted.terminal_result.as_ref(), Some(&result));
        }
    }

    /// 同一 Windows 登录会话已有正式输入所有者时，竞争的桌面 run 必须让位：
    /// 以 `input_owner_busy` 终止，且**零输入**。释放所有者后同一路径必须能再次执行。
    #[cfg(windows)]
    #[tokio::test]
    async fn desktop_run_yields_to_existing_input_owner_with_zero_input() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let scope = windows_process_guard::current_interactive_session_scope()
            .expect("resolve windows input session scope");
        let other_owner = windows_process_guard::interactive_input_lease_broker()
            .acquire(scope, "run-other-room")
            .expect("hold the desktop input lease");

        let store = store();
        let factory = factory(true);
        let planner = FakePlanner::one_click();
        let blocked =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity("tool-busy-competitor"))
                .await;

        assert_eq!(blocked.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            blocked.error.as_ref().map(|error| error.code.as_str()),
            Some("input_owner_busy")
        );
        assert_eq!(
            factory.action_count.load(Ordering::SeqCst),
            0,
            "the competing run must not inject input while another owner holds the lease"
        );

        other_owner.release();
        // A distinct turn: the blocked attempt already claimed (session-1, turn-1).
        let retry_identity =
            ToolCallIdentity::from_provider("tool-after-lease-release", "session-1", "turn-2");
        let retry = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &retry_identity)
        .await;
        assert_eq!(retry.status, ComputerUseTerminalStatus::Succeeded);
    }

    #[tokio::test]
    async fn step_trace_records_real_action_and_distinct_evidence_before_verified_progress() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let factory = factory(true);
        let identity = identity("trace-check");
        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;
        assert!(result.goal_achieved);
        let connection = Connection::open(path).unwrap();
        let trace:(String,String,String,String,bool,u64,u64) = connection.query_row(
            "SELECT action_type,normalized_target,before_evidence_ref,after_evidence_ref,visible_progress,started_at_ms,completed_at_ms FROM computer_use_steps WHERE run_id=?1",
            [&identity.call_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).unwrap();
        assert_eq!(trace.0, "click");
        assert_eq!(trace.1, "submit");
        assert_ne!(trace.2, trace.3);
        assert!(trace.2.contains("evidence-1"));
        assert!(trace.3.contains("verify-1"));
        assert!(trace.4);
        assert!(trace.5 > 0 && trace.6 >= trace.5);
        let receipt: (
            String,
            Option<bool>,
            Option<bool>,
            Option<u32>,
            String,
            String,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT input_delivery,partial,path_completed,confirmed_point_count,effect_status,goal_verdict,input_release_status FROM computer_use_steps WHERE run_id=?1",
                [&identity.call_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
            )
            .unwrap();
        assert_eq!(receipt.0, "sent");
        assert_eq!(receipt.1, Some(false));
        assert_eq!(receipt.2, Some(true));
        assert_eq!(receipt.3, Some(2));
        assert_eq!(receipt.4, "effect_observed");
        assert_eq!(receipt.5, "passed");
        assert_eq!(receipt.6.as_deref(), Some("released"));
        let action: String = connection
            .query_row(
                "SELECT action_json FROM computer_use_step_details WHERE run_id=?1",
                [&identity.call_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(action.contains("click"));
        assert!(!action.contains("controller_action"));
    }

    #[tokio::test]
    async fn host_cancellation_after_planning_prevents_input_and_persists_cancelled() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        struct CancellingPlanner(Arc<AtomicBool>);
        impl ComputerUsePlanner for CancellingPlanner {
            fn classify<'a>(
                &'a self,
                request: &'a ComputerUseRequest,
                _: &'a Observation,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
                Box::pin(async move { Ok(request.surface) })
            }
            fn next_action<'a>(
                &'a self,
                _: &'a ComputerUseRequest,
                _: &'a Observation,
                _: usize,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>>
            {
                Box::pin(async move {
                    self.0.store(true, Ordering::SeqCst);
                    Ok(Some(ComputerUseAction {
                        kind: ComputerUseActionKind::Click,
                        target: "submit".into(),
                        arguments: json!({}),
                        risk: ComputerUseRiskClass::ReversibleLocal,
                    }))
                })
            }
            fn verify<'a>(
                &'a self,
                _: &'a ComputerUseRequest,
                _: &'a Observation,
                _: &'a Observation,
                verification: Verification,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
                Box::pin(async move { Ok(verification) })
            }
        }
        let flag = Arc::new(AtomicBool::new(false));
        let probe = flag.clone();
        let planner = CancellingPlanner(flag);
        let store = store();
        let factory = factory(false);
        let identity = identity("cancel-before-input");
        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .with_cancelled(Arc::new(move || probe.load(Ordering::SeqCst)))
                .execute(&input("desktop"), &identity)
                .await;
        assert_eq!(result.status, ComputerUseTerminalStatus::Cancelled);
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 0);
        assert_eq!(store.action_counts(&identity.call_id).unwrap(), (0, 0));
        assert_eq!(
            store
                .load(&identity.call_id)
                .unwrap()
                .unwrap()
                .terminal_result
                .unwrap()
                .status,
            ComputerUseTerminalStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn dropping_running_controller_persists_cancelled_instead_of_running_forever() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        struct WaitingPlanner;
        impl ComputerUsePlanner for WaitingPlanner {
            fn classify<'a>(
                &'a self,
                request: &'a ComputerUseRequest,
                _: &'a Observation,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
                Box::pin(async move { Ok(request.surface) })
            }
            fn next_action<'a>(
                &'a self,
                _: &'a ComputerUseRequest,
                _: &'a Observation,
                _: usize,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>>
            {
                Box::pin(std::future::pending())
            }
            fn verify<'a>(
                &'a self,
                _: &'a ComputerUseRequest,
                _: &'a Observation,
                _: &'a Observation,
                verification: Verification,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
                Box::pin(async move { Ok(verification) })
            }
        }
        let planner = WaitingPlanner;
        let store = store();
        let factory = factory(false);
        let identity = identity("cancel-dropped-future");
        let executor =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default());
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            executor.execute(&input("desktop"), &identity)
        )
        .await
        .is_err());
        assert_eq!(
            store
                .load(&identity.call_id)
                .unwrap()
                .unwrap()
                .terminal_result
                .unwrap()
                .status,
            ComputerUseTerminalStatus::Cancelled
        );
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 0);

        // 持有者崩溃（此处为 future 被丢弃）必须释放 lease 而不是泄漏：
        // 同一 Windows 登录会话应当立即可被再次获得。
        #[cfg(windows)]
        {
            let scope = windows_process_guard::current_interactive_session_scope()
                .expect("resolve windows input session scope");
            let reacquired = windows_process_guard::interactive_input_lease_broker()
                .acquire(scope, "after-holder-drop");
            assert!(
                reacquired.is_ok(),
                "a dropped holder must release the desktop input lease"
            );
            drop(reacquired); // 释放，避免影响其它桌面用例
        }
    }

    #[tokio::test]
    async fn originating_chat_room_is_persisted_with_the_run() {
        let store = store();
        let planner = FakePlanner::one_click();
        let factory = factory(true);
        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute_in_room(
                    &input("browser"),
                    &identity("tool-room-audit"),
                    Some("room-full-access"),
                )
                .await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        let persisted = store.load(&result.call_id).unwrap().unwrap();
        assert_eq!(persisted.chat_room_id.as_deref(), Some("room-full-access"));
    }

    #[tokio::test]
    async fn full_access_room_policy_executes_ordinary_stateful_browser_actions() {
        for (label, kind) in [
            ("open-tab", ComputerUseActionKind::OpenTab),
            ("navigate", ComputerUseActionKind::Navigate),
            ("submit", ComputerUseActionKind::Submit),
        ] {
            let store = store();
            let planner = FakePlanner::one_action(
                kind,
                format!("safe-{label}"),
                ComputerUseRiskClass::Stateful,
            );
            let factory = factory(true);
            let result =
                ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                    .execute_in_room_with_full_access(
                        &input("browser"),
                        &identity(&format!("tool-full-access-{label}")),
                        Some("room-full-access"),
                    )
                    .await;

            assert_eq!(
                result.status,
                ComputerUseTerminalStatus::Succeeded,
                "{label}"
            );
            assert_eq!(factory.action_count.load(Ordering::SeqCst), 1, "{label}");
        }
    }

    #[tokio::test]
    async fn full_access_room_policy_never_executes_sensitive_or_semantic_sensitive_actions() {
        let sensitive_store = store();
        let sensitive_planner = FakePlanner::one_action(
            ComputerUseActionKind::Submit,
            "confirm-settings",
            ComputerUseRiskClass::Sensitive,
        );
        let sensitive_factory = factory(true);
        let sensitive_result = ComputerUseExecutor::new_for_test(
            &sensitive_planner,
            &sensitive_factory,
            &sensitive_store,
            ComputerUseBudgets::default(),
        )
        .execute_in_room_with_full_access(
            &input("browser"),
            &identity("tool-full-access-sensitive"),
            Some("room-full-access"),
        )
        .await;
        assert_eq!(sensitive_result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            sensitive_result
                .error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("approval_required")
        );
        assert_eq!(sensitive_factory.action_count.load(Ordering::SeqCst), 0);

        let semantic_store = store();
        let semantic_planner = FakePlanner::one_action(
            ComputerUseActionKind::Click,
            "delete-project",
            ComputerUseRiskClass::ReversibleLocal,
        );
        let semantic_factory = factory(true);
        let mut semantic_input = input("browser");
        semantic_input["objective"] = json!("永久删除项目");
        let semantic_result = ComputerUseExecutor::new_for_test(
            &semantic_planner,
            &semantic_factory,
            &semantic_store,
            ComputerUseBudgets::default(),
        )
        .execute_in_room_with_full_access(
            &semantic_input,
            &identity("tool-full-access-semantic-sensitive"),
            Some("room-full-access"),
        )
        .await;
        assert_eq!(semantic_result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            semantic_result
                .error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("approval_required")
        );
        assert_eq!(semantic_factory.action_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn action_ok_but_verify_false_is_a_terminal_failure_not_success() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let store = store();
        let planner = FakePlanner::one_click();
        let factory = factory(false);
        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity("tool-verify-false"))
                .await;

        assert!(!result.goal_achieved);
        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(result.stage, ComputerUseStage::Verification);
        assert_eq!(result.error.as_ref().unwrap().code, "verification_failed");
        assert!(!result.error.as_ref().unwrap().retryable);
        assert_eq!(
            result.error.as_ref().unwrap().retry_owner,
            ComputerUseRetryOwner::Model
        );
        assert_eq!(result.attempts, 1);
        assert_eq!(result.steps_completed, 1);
    }

    #[tokio::test]
    async fn ambiguous_surface_and_unavailable_backend_return_structured_terminal_results() {
        let routing_store = store();
        let planner = FakePlanner::one_click();
        let routing_factory = factory(true);
        let ambiguous = ComputerUseExecutor::new_for_test(
            &planner,
            &routing_factory,
            &routing_store,
            ComputerUseBudgets::default(),
        )
        .execute(
            &json!({
                "objective": "click mixed target",
                "surface": "auto",
                "target": {"application": "pet", "element": "submit"},
                "success_criteria": ["success visible"]
            }),
            &identity("tool-ambiguous"),
        )
        .await;
        assert_eq!(ambiguous.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(ambiguous.stage, ComputerUseStage::Classification);
        assert_eq!(ambiguous.error.as_ref().unwrap().code, "surface_conflict");
        assert_eq!(routing_factory.action_count.load(Ordering::SeqCst), 0);

        let backend_store = store();
        let unavailable_base = factory(true);
        let unavailable = FakeFactory {
            backend_unavailable: true,
            ..unavailable_base
        };
        let result = ComputerUseExecutor::new_for_test(
            &planner,
            &unavailable,
            &backend_store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("browser"), &identity("tool-unavailable"))
        .await;
        assert_eq!(result.status, ComputerUseTerminalStatus::Failed);
        assert_eq!(result.error.as_ref().unwrap().code, "backend_unavailable");
        assert!(result.error.as_ref().unwrap().retryable);
        assert_eq!(
            result.error.as_ref().unwrap().retry_owner,
            ComputerUseRetryOwner::System
        );
    }

    #[tokio::test]
    async fn exact_terminal_fields_are_serialized_and_duplicate_call_is_idempotent() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let store = store();
        let planner = FakePlanner::one_click();
        let factory = factory(true);
        let executor =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default());
        let ids = identity("tool-idempotent");
        let first = executor.execute(&input("desktop"), &ids).await;
        let second = executor.execute(&input("desktop"), &ids).await;
        assert_eq!(second, first);
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);

        let value = serde_json::to_value(&first).unwrap();
        for field in [
            "stage",
            "goal_achieved",
            "attempts",
            "steps_completed",
            "evidence",
            "supervisor",
        ] {
            assert!(value.get(field).is_some(), "missing result field {field}");
        }
    }

    #[tokio::test]
    async fn invalid_input_can_be_corrected_without_consuming_the_single_execution_budget() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let store = store();
        let factory = factory(true);
        let planner = FakePlanner::one_click();
        let budgets = ComputerUseBudgets {
            max_calls_per_turn: 1,
            ..ComputerUseBudgets::default()
        };
        let executor = ComputerUseExecutor::new_for_test(&planner, &factory, &store, budgets);
        let malformed = json!({"objective":"click submit","surface":"desktop"});
        let invalid_id = identity("tool-invalid-before-correction");
        let first = executor.execute(&malformed, &invalid_id).await;
        assert_eq!(first.error.as_ref().unwrap().code, "invalid_tool_input");
        assert_eq!(first.attempts, 0);
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 0);
        // 同一 provider call 重放仍命中缓存，不额外消耗纠错次数。
        assert_eq!(executor.execute(&malformed, &invalid_id).await, first);
        assert_eq!(
            store
                .turn_budget_counts(&invalid_id.session_id, &invalid_id.turn_id)
                .unwrap(),
            (0, 1)
        );
        let empty_objective =
            json!({"objective":"","surface":"desktop","success_criteria":["visible"]});
        let second_invalid = executor
            .execute(&empty_objective, &identity("tool-empty-objective"))
            .await;
        assert_eq!(
            second_invalid.error.as_ref().unwrap().code,
            "invalid_objective"
        );
        assert_eq!(
            store
                .turn_budget_counts(&invalid_id.session_id, &invalid_id.turn_id)
                .unwrap(),
            (0, 2)
        );
        let corrected = executor
            .execute(&input("desktop"), &identity("tool-corrected"))
            .await;
        assert_eq!(corrected.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);
        let mut next = input("desktop");
        next["objective"] = json!("another independent UI task");
        let exhausted = executor
            .execute(&next, &identity("tool-after-execution-budget"))
            .await;
        assert_eq!(
            exhausted.error.as_ref().unwrap().code,
            "recursive_call_blocked"
        );
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn repeated_invalid_inputs_have_a_separate_finite_correction_budget() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let store = store();
        let factory = factory(true);
        let planner = FakePlanner::one_click();
        let budgets = ComputerUseBudgets {
            max_calls_per_turn: 1,
            ..ComputerUseBudgets::default()
        };
        let executor = ComputerUseExecutor::new_for_test(&planner, &factory, &store, budgets);
        let malformed = json!({"objective":"draw","surface":"desktop"});
        for index in 0..MAX_INVALID_INPUTS_PER_TURN {
            let result = executor
                .execute(&malformed, &identity(&format!("tool-invalid-{index}")))
                .await;
            assert_eq!(result.error.as_ref().unwrap().code, "invalid_tool_input");
        }
        let exhausted = executor
            .execute(&malformed, &identity("tool-too-many-invalid"))
            .await;
        assert_eq!(
            exhausted.error.as_ref().unwrap().code,
            "input_correction_budget_exhausted"
        );
        assert_eq!(
            exhausted.error.as_ref().unwrap().retry_owner,
            ComputerUseRetryOwner::None
        );
        let stopped = executor
            .execute(&input("desktop"), &identity("tool-after-invalid-limit"))
            .await;
        assert_eq!(
            stopped.error.as_ref().unwrap().code,
            "input_correction_budget_exhausted"
        );
        let scope = identity("scope");
        assert_eq!(
            store
                .turn_budget_counts(&scope.session_id, &scope.turn_id)
                .unwrap(),
            (0, 4)
        );
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn turn_watchdog_blocks_recursive_calls_without_more_ui_input() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let store = store();
        let factory = factory(false);
        let budgets = ComputerUseBudgets {
            max_calls_per_turn: 2,
            ..ComputerUseBudgets::default()
        };
        let first_planner = FakePlanner::one_click();
        let first = ComputerUseExecutor::new_for_test(&first_planner, &factory, &store, budgets)
            .execute(&input("desktop"), &identity("tool-failure-1"))
            .await;
        assert!(!first.goal_achieved);
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);

        let second_planner = FakePlanner::one_click();
        let second = ComputerUseExecutor::new_for_test(&second_planner, &factory, &store, budgets)
            .execute(&input("desktop"), &identity("tool-failure-2"))
            .await;
        assert!(!second.goal_achieved);
        assert_eq!(second.call_id, identity("tool-failure-2").call_id);
        assert!(second.summary.contains("cached terminal result"));
        assert_eq!(
            second.error.as_ref().unwrap().code,
            first.error.as_ref().unwrap().code
        );
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);

        let planner = FakePlanner::one_click();
        let third = ComputerUseExecutor::new_for_test(&planner, &factory, &store, budgets)
            .execute(&input("desktop"), &identity("tool-failure-3"))
            .await;
        assert_eq!(third.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(third.error.as_ref().unwrap().code, "recursive_call_blocked");
        assert_eq!(
            third.error.as_ref().unwrap().retry_owner,
            ComputerUseRetryOwner::None
        );
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);
    }

    // ---- S1.5：失败路径的四维事实 ----

    /// 中途失败的适配器：观察/校验正常，动作固定以 `input_failed` 失败。
    struct FailingActAdapter {
        surface: ComputerUseSurface,
        generation: AtomicU64,
    }

    impl ComputerUseAdapter for FailingActAdapter {
        fn surface(&self) -> ComputerUseSurface {
            self.surface
        }
        fn capabilities(&self) -> ComputerUseCapabilities {
            ComputerUseCapabilities {
                click: true,
                ..ComputerUseCapabilities::default()
            }
        }
        fn observe(
        &self,
        _request: &ComputerUseRequest,
        _remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
            self.generation.fetch_add(1, Ordering::SeqCst);
            Ok(Observation {
                generation: self.generation.load(Ordering::SeqCst),
                surface: self.surface,
                surface_identity: "window-desktop:1".to_string(),
                state: json!({ "screen": "desktop" }),
                evidence: vec!["evidence-1".to_string()],
            })
        }
        fn act(
            &self,
            _action: &ComputerUseAction,
            _expected_generation: u64,
            _remaining: std::time::Duration,
        ) -> Result<StepExecution, ComputerUseError> {
            Err(ComputerUseError::new(
                "input_failed",
                "cursor_move_failed",
                true,
                ComputerUseRetryOwner::System,
            ))
        }
        fn verify(
            &self,
            _criteria: &[String],
            _before: &Observation,
            _after: &Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, ComputerUseError> {
            Ok(Verification {
                achieved: false,
                visible_progress: false,
                summary: "not yet".to_string(),
                evidence: vec!["verify-1".to_string()],
            })
        }
    }

    struct FailingActFactory;

    impl ComputerUseAdapterFactory for FailingActFactory {
        fn routing_context(&self) -> SurfaceRoutingContext {
            SurfaceRoutingContext::default()
        }
        fn build(
            &self,
            surface: ComputerUseSurface,
        ) -> Result<DynComputerUseAdapter, ComputerUseError> {
            Ok(DynComputerUseAdapter::new(FailingActAdapter {
                surface,
                generation: AtomicU64::new(0),
            }))
        }
    }

    /// §2.2 恢复表：中途失败必须留下明确事实——`may_have_been_sent` + 释放 `unknown`。
    /// 不允许把"可能已发送"写成零输入（`not_sent`），也不允许写成已释放（`released`）。
    #[tokio::test]
    async fn failed_input_records_may_have_been_sent_with_unknown_release() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("failure-facts.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let identity = identity("failure-facts");

        let result =
            ComputerUseExecutor::new_for_test(&planner, &FailingActFactory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;

        assert!(!result.goal_achieved);
        assert_eq!(result.status, ComputerUseTerminalStatus::Failed);
        let connection = Connection::open(path).unwrap();
        let (delivery, release): (String, String) = connection
            .query_row(
                "SELECT input_delivery,input_release_status FROM computer_use_steps WHERE run_id=?1",
                [&identity.call_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            delivery, "may_have_been_sent",
            "a mid-input failure must not be recorded as zero input"
        );
        assert_eq!(
            release, "unknown",
            "an unconfirmed release must be unknown, never released"
        );
    }

    /// 输入前就失败（lease 被别处持有、持久化失败等）是**确定**零输入，
    /// 不能一律记成 `may_have_been_sent`。
    #[test]
    fn pre_input_failure_codes_are_not_may_have_been_sent() {
        for code in [
            "input_lease_lost",
            "input_lease_scope_unavailable",
            "input_owner_busy",
            "persistence_error",
        ] {
            assert!(
                !failure_may_have_sent_input(code),
                "{code} happens before any input and must be not_sent"
            );
        }
        for code in [
            "input_failed",
            "mouse_release_failed",
            "cursor_move_failed",
            "cancelled",
        ] {
            assert!(
                failure_may_have_sent_input(code),
                "{code} may happen after input was sent"
            );
        }
    }

    /// §3.3 端到端回归的原生输入边界替身：`act` 每被调用一次就记一次"输入已落盘"。
    ///
    /// 它被注入到**真实** `TracingAdapter` 之下，因此断言 `action_count == 0` 等价于
    /// "执行器在进入原生输入边界之前就拦下了这一步"。
    struct LeaseProbeAdapter {
        surface: ComputerUseSurface,
        generation: AtomicU64,
        action_count: Arc<AtomicUsize>,
        observe_count: AtomicUsize,
        verify_count: AtomicUsize,
        /// 在第一次观察时执行的失效动作；`None` 表示不失效（阴性对照）。
        revoke: Option<Arc<dyn Fn() + Send + Sync>>,
    }

    impl ComputerUseAdapter for LeaseProbeAdapter {
        fn surface(&self) -> ComputerUseSurface {
            self.surface
        }

        fn capabilities(&self) -> ComputerUseCapabilities {
            ComputerUseCapabilities {
                navigate: true,
                click: true,
                submit: true,
                multiple_tabs: true,
                ..ComputerUseCapabilities::default()
            }
        }

        fn observe(
            &self,
            _request: &ComputerUseRequest,
            _remaining: std::time::Duration,
        ) -> Result<Observation, ComputerUseError> {
            // 规划之前先让别处使本次 run 的 lease 失效：这是"输入前"的准确位置。
            if self.observe_count.fetch_add(1, Ordering::SeqCst) == 0 {
                if let Some(revoke) = &self.revoke {
                    revoke();
                }
            }
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Observation {
                generation,
                surface: self.surface,
                surface_identity: format!("{:?}-surface", self.surface),
                state: json!({"generation": generation}),
                evidence: vec![format!("evidence-{generation}.png")],
            })
        }

        fn act(
            &self,
            _action: &ComputerUseAction,
            _expected_generation: u64,
            _remaining: std::time::Duration,
        ) -> Result<StepExecution, ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            Ok(StepExecution {
                input_sent: true,
                summary: "input sent".into(),
                evidence: vec!["input.json".into()],
                partial: Some(false),
                path_completed: Some(true),
                confirmed_point_count: Some(2),
                input_release_status: Some(computer_use::StepInputReleaseStatus::Released),
                // 该替身模拟"没有回执"的旧适配器：由执行器回落到逐字段声明。
                receipt: None,
            })
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &Observation,
            _after: &Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, ComputerUseError> {
            // 与 FakeAdapter 一致：进入循环前的首次验收必须"未达成"，否则控制器会
            // 直接判成功而根本不会规划输入。
            let call = self.verify_count.fetch_add(1, Ordering::SeqCst);
            Ok(Verification {
                achieved: call > 0,
                visible_progress: call > 0,
                summary: if call > 0 {
                    "success criterion is visibly satisfied".into()
                } else {
                    "success criterion is not yet visible".into()
                },
                evidence: vec![format!("verify-{call}.json")],
            })
        }
    }

    struct LeaseProbeFactory {
        action_count: Arc<AtomicUsize>,
        revoke: Option<Arc<dyn Fn() + Send + Sync>>,
    }

    impl ComputerUseAdapterFactory for LeaseProbeFactory {
        fn routing_context(&self) -> SurfaceRoutingContext {
            SurfaceRoutingContext::default()
        }

        fn build(
            &self,
            surface: ComputerUseSurface,
        ) -> Result<DynComputerUseAdapter, ComputerUseError> {
            Ok(DynComputerUseAdapter::new(LeaseProbeAdapter {
                surface,
                generation: AtomicU64::new(0),
                action_count: Arc::clone(&self.action_count),
                observe_count: AtomicUsize::new(0),
                verify_count: AtomicUsize::new(0),
                revoke: self.revoke.clone(),
            }))
        }
    }

    /// 让**别人**使当前 run 的 lease 失效。
    ///
    /// 执行器把 lease 藏在 `TracingAdapter` 内部，外部只能借 `acquire` 的 busy 错误读到
    /// 当轮 owner epoch，再按 epoch 精确失效——不碰其它 scope，也不清空 broker。
    #[cfg(windows)]
    fn revoke_current_desktop_input_owner(scope: &str) {
        let busy = windows_process_guard::interactive_input_lease_broker()
            .acquire(scope, "e2e-lease-revoker")
            .expect_err("running desktop run must already hold the input lease");
        assert!(
            windows_process_guard::interactive_input_lease_broker()
                .revoke_owner_for_test(scope, busy.owner_epoch),
            "the live owner epoch {} must be revocable",
            busy.owner_epoch
        );
    }

    struct LeaseProbeOutcome {
        result: ComputerUseResult,
        action_count: usize,
        run_path: std::path::PathBuf,
        call_id: String,
        _directory: tempfile::TempDir,
    }

    /// 走 `ComputerUseExecutor::execute` 的**生产执行路径**（内含真实 `TracingAdapter`
    /// 与输入前 epoch 检查屏障），只把最内层原生适配器换成可计数的替身。
    #[cfg(windows)]
    async fn run_desktop_with_lease_probe(
        provider: &str,
        revoke_before_input: bool,
    ) -> LeaseProbeOutcome {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let run_path = directory.path().join(format!("{provider}.sqlite3"));
        let store = ComputerUseRunStore::open(&run_path).unwrap();
        let action_count = Arc::new(AtomicUsize::new(0));
        let revoke = revoke_before_input.then(|| {
            let scope = windows_process_guard::current_interactive_session_scope()
                .expect("resolve windows input session scope");
            Arc::new(move || revoke_current_desktop_input_owner(&scope))
                as Arc<dyn Fn() + Send + Sync>
        });
        let factory = LeaseProbeFactory {
            action_count: Arc::clone(&action_count),
            revoke,
        };
        let identity = identity(provider);
        let result = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &identity)
        .await;
        LeaseProbeOutcome {
            result,
            action_count: action_count.load(Ordering::SeqCst),
            run_path,
            call_id: identity.call_id,
            _directory: directory,
        }
    }

    fn step_row_count(run_path: &std::path::Path, call_id: &str) -> i64 {
        Connection::open(run_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM computer_use_steps WHERE run_id=?1",
                [call_id],
                |row| row.get(0),
            )
            .unwrap()
    }

    /// §3.3 正向：真实 `TracingAdapter` 路径上，输入前 lease 被失效 → 以 `input_lease_lost`
    /// 终止，且**原生输入边界调用次数为零**（连 step 都不落，因为屏障在记录之前）。
    #[cfg(windows)]
    #[tokio::test]
    async fn tracing_adapter_blocks_input_when_lease_is_revoked_before_input() {
        let outcome = run_desktop_with_lease_probe("e2e-lease-revoked", true).await;

        assert_eq!(
            outcome
                .result
                .error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("input_lease_lost"),
            "输入前失效必须以 input_lease_lost 拒绝，而不是别的失败"
        );
        assert_eq!(outcome.result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(outcome.result.stage, ComputerUseStage::Execution);
        assert!(!outcome.result.goal_achieved);
        assert_eq!(
            outcome.action_count, 0,
            "输入前失效必须让原生输入边界调用次数为零"
        );
        assert_eq!(
            step_row_count(&outcome.run_path, &outcome.call_id),
            0,
            "屏障在写 step 记录之前返回，因此不得留下任何 step（更不能留下已发送的 step）"
        );
    }

    /// §3.3 阴性对照：同一路径、同一替身，只是不失效 → 正常落一次输入并成功。
    /// 没有这条对照，"正向测试通过"可能只是因为链路本身跑不起来。
    #[cfg(windows)]
    #[tokio::test]
    async fn tracing_adapter_sends_input_when_the_lease_is_intact() {
        let outcome = run_desktop_with_lease_probe("e2e-lease-intact", false).await;

        assert_eq!(outcome.result.status, ComputerUseTerminalStatus::Succeeded);
        assert!(outcome.result.goal_achieved);
        assert_eq!(
            outcome.action_count, 1,
            "lease 有效时该路径必须真的走到原生输入边界一次"
        );
        assert_eq!(step_row_count(&outcome.run_path, &outcome.call_id), 1);
    }

    /// ---- RPR-04b：回执在生产 → 消费 → 持久化三段上的回归 ----

    /// 回执事实规格（与 `ActionReceipt` 一一对应，便于构造各种组合）。
    #[derive(Clone, Copy)]
    struct ReceiptSpec {
        input_delivery: runtime::InputDelivery,
        partial: Option<bool>,
        path_completed: Option<bool>,
        confirmed_point_count: Option<u32>,
        input_release: runtime::InputReleaseStatus,
    }

    /// 可注入"带回执的失败/成功"的适配器替身；回执身份按传入动作计算（除非故意写错）。
    struct ReceiptActAdapter {
        surface: ComputerUseSurface,
        generation: AtomicU64,
        executions: Arc<AtomicUsize>,
        failure: Option<ComputerUseError>,
        /// 失败时是否附加按当前动作计算出来的回执（旧错误 JSON 场景不附加）。
        attach_receipt: bool,
        mismatch_action_id: bool,
        spec: ReceiptSpec,
    }

    impl ReceiptActAdapter {
        fn receipt(&self, action: &ComputerUseAction) -> runtime::ActionReceipt {
            let action_id = if self.mismatch_action_id {
                "desktop:some-other-action:0000000000000000".to_string()
            } else {
                computer_use::action_attempt_id(self.surface, action)
            };
            runtime::ActionReceipt {
                action_id,
                input_delivery: self.spec.input_delivery,
                partial: self.spec.partial,
                path_completed: self.spec.path_completed,
                confirmed_point_count: self.spec.confirmed_point_count,
                effect: runtime::EffectStatus::NotObserved,
                goal_verdict: runtime::GoalVerdict::NotChecked,
                input_release: self.spec.input_release,
            }
        }
    }

    impl ComputerUseAdapter for ReceiptActAdapter {
        fn surface(&self) -> ComputerUseSurface {
            self.surface
        }
        fn capabilities(&self) -> ComputerUseCapabilities {
            ComputerUseCapabilities {
                click: true,
                ..ComputerUseCapabilities::default()
            }
        }
        fn observe(
            &self,
            _request: &ComputerUseRequest,
            _remaining: std::time::Duration,
        ) -> Result<Observation, ComputerUseError> {
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Observation {
                generation,
                surface: self.surface,
                surface_identity: format!("{:?}-surface", self.surface),
                state: json!({"generation": generation}),
                evidence: vec![format!("evidence-{generation}.png")],
            })
        }
        fn act(
            &self,
            action: &ComputerUseAction,
            _expected_generation: u64,
            _remaining: std::time::Duration,
        ) -> Result<StepExecution, ComputerUseError> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            let receipt = self.receipt(action);
            match &self.failure {
                Some(base) => {
                    let error = base.clone();
                    Err(if self.attach_receipt {
                        error.with_receipt(receipt)
                    } else {
                        error
                    })
                }
                None => Ok(StepExecution {
                    input_sent: true,
                    summary: "input sent".into(),
                    evidence: vec!["input.json".into()],
                    partial: receipt.partial,
                    path_completed: receipt.path_completed,
                    confirmed_point_count: receipt.confirmed_point_count,
                    input_release_status: Some(match receipt.input_release {
                        runtime::InputReleaseStatus::NotNeeded => {
                            computer_use::StepInputReleaseStatus::NotNeeded
                        }
                        runtime::InputReleaseStatus::Released => {
                            computer_use::StepInputReleaseStatus::Released
                        }
                        runtime::InputReleaseStatus::Unknown => {
                            computer_use::StepInputReleaseStatus::Unknown
                        }
                    }),
                    receipt: Some(receipt),
                }),
            }
        }
        fn verify(
            &self,
            _criteria: &[String],
            _before: &Observation,
            _after: &Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, ComputerUseError> {
            Ok(Verification {
                achieved: false,
                visible_progress: false,
                summary: "not yet".into(),
                evidence: vec!["verify-1".into()],
            })
        }
    }

    struct ReceiptActFactory {
        executions: Arc<AtomicUsize>,
        failure: Option<ComputerUseError>,
        attach_receipt: bool,
        mismatch_action_id: bool,
        spec: ReceiptSpec,
    }

    impl ComputerUseAdapterFactory for ReceiptActFactory {
        fn routing_context(&self) -> SurfaceRoutingContext {
            SurfaceRoutingContext::default()
        }
        fn build(
            &self,
            surface: ComputerUseSurface,
        ) -> Result<DynComputerUseAdapter, ComputerUseError> {
            Ok(DynComputerUseAdapter::new(ReceiptActAdapter {
                surface,
                generation: AtomicU64::new(0),
                executions: Arc::clone(&self.executions),
                failure: self.failure.clone(),
                attach_receipt: self.attach_receipt,
                mismatch_action_id: self.mismatch_action_id,
                spec: self.spec,
            }))
        }
    }

    /// 失败替身：`(错误码, 是否可重试)` → 带回执的错误。
    fn failing_receipt_factory(
        executions: &Arc<AtomicUsize>,
        code: &'static str,
        retryable: bool,
        spec: ReceiptSpec,
    ) -> ReceiptActFactory {
        ReceiptActFactory {
            executions: Arc::clone(executions),
            failure: Some(ComputerUseError::new(
                code,
                "injected by the receipt regression adapter",
                retryable,
                ComputerUseRetryOwner::System,
            )),
            attach_receipt: true,
            mismatch_action_id: false,
            spec,
        }
    }

    fn partial_path_spec() -> ReceiptSpec {
        ReceiptSpec {
            input_delivery: runtime::InputDelivery::Sent,
            partial: Some(true),
            path_completed: Some(false),
            confirmed_point_count: Some(1),
            input_release: runtime::InputReleaseStatus::Released,
        }
    }

    fn unknown_facts_spec() -> ReceiptSpec {
        ReceiptSpec {
            input_delivery: runtime::InputDelivery::MayHaveBeenSent,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            input_release: runtime::InputReleaseStatus::Unknown,
        }
    }

    /// 单个 step 的持久化事实列。
    struct StepFacts {
        status: Option<String>,
        delivery: Option<String>,
        partial: Option<i64>,
        path_completed: Option<i64>,
        point_count: Option<i64>,
        release: Option<String>,
    }

    fn step_facts(run_path: &std::path::Path, call_id: &str) -> StepFacts {
        Connection::open(run_path)
            .unwrap()
            .query_row(
                "SELECT status,input_delivery,partial,path_completed,confirmed_point_count,input_release_status FROM computer_use_steps WHERE run_id=?1",
                [call_id],
                |row| {
                    Ok(StepFacts {
                        status: row.get(0).ok(),
                        delivery: row.get(1).ok(),
                        partial: row.get(2).ok(),
                        path_completed: row.get(3).ok(),
                        point_count: row.get(4).ok(),
                        release: row.get(5).ok(),
                    })
                },
            )
            .unwrap()
    }

    /// RPR-04b §2.4：失败路径的四个维度必须来自 helper 回执，而不是由错误码猜出来的
    /// "可能已发送 + 释放未知"。
    #[tokio::test]
    async fn failure_receipt_facts_are_persisted_instead_of_the_code_heuristic() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt-facts.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = failing_receipt_factory(
            &executions,
            "stale_observation",
            false,
            partial_path_spec(),
        );
        let identity = identity("receipt-facts");

        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;

        assert!(!result.goal_achieved);
        let error = result.error.as_ref().expect("必须留下终态失败");
        let receipt = error.receipt().expect("终态错误必须保留 helper 回执");
        assert_eq!(receipt.confirmed_point_count, Some(1));

        let facts = step_facts(&path, &identity.call_id);
        assert_ne!(facts.status.as_deref(), Some("receipt_protocol_anomaly"));
        assert_eq!(
            facts.delivery.as_deref(),
            Some("sent"),
            "回执说已注入就不能记成未知"
        );
        assert_eq!(facts.partial, Some(1));
        assert_eq!(facts.path_completed, Some(0), "路径未完成必须明确落盘为 0");
        assert_eq!(facts.point_count, Some(1));
        assert_eq!(facts.release.as_deref(), Some("released"));

        // 持久化真正保留：从磁盘重新读回终态结果，回执的身份与事实必须一字不差。
        let stored = ComputerUseRunStore::open(&path)
            .unwrap()
            .load(&identity.call_id)
            .unwrap()
            .expect("run 必须已落盘");
        let stored_error = stored
            .terminal_result
            .expect("终态结果必须已落盘")
            .error
            .expect("失败必须留在终态结果里");
        let stored_receipt = stored_error.receipt().expect("磁盘上的终态错误必须带回执");
        assert_eq!(stored_receipt, receipt, "回执必须在持久化往返后保持不变");
    }

    /// RPR-04b：回执身份与当前动作不匹配属于协议异常——保守处理，且绝不报告"零输入"。
    #[tokio::test]
    async fn mismatched_receipt_identity_is_recorded_as_an_anomaly_not_as_zero_input() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt-mismatch.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = ReceiptActFactory {
            executions: Arc::clone(&executions),
            failure: Some(ComputerUseError::new(
                "input_failed",
                "injected by the receipt regression adapter",
                true,
                ComputerUseRetryOwner::System,
            )),
            attach_receipt: true,
            mismatch_action_id: true,
            spec: partial_path_spec(),
        };
        let identity = identity("receipt-mismatch");

        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;

        assert!(!result.goal_achieved);
        let facts = step_facts(&path, &identity.call_id);
        assert_eq!(facts.status.as_deref(), Some("receipt_protocol_anomaly"));
        assert_eq!(
            facts.delivery.as_deref(),
            Some("may_have_been_sent"),
            "读不懂的回执不得退化成 not_sent"
        );
        assert_eq!(facts.partial, None, "异常路径不写路径事实");
        assert_eq!(facts.path_completed, None);
        assert_eq!(facts.point_count, None);
        assert_eq!(facts.release.as_deref(), Some("unknown"));
    }

    /// RPR-04b：helper 失联且没有任何事实时，已知部分保留、未知后缀不被猜成完整，
    /// 且未确认释放会阻断后续动作（终态 Blocked + 只发生过一次输入）。
    #[tokio::test]
    async fn unknown_helper_facts_and_unconfirmed_release_block_the_next_action() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt-unknown.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = failing_receipt_factory(
            &executions,
            "mouse_release_failed",
            false,
            unknown_facts_spec(),
        );
        let identity = identity("receipt-unknown");

        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            executions.load(Ordering::SeqCst),
            1,
            "释放未确认必须阻断下一个动作，不得再次注入"
        );
        let facts = step_facts(&path, &identity.call_id);
        assert_ne!(facts.status.as_deref(), Some("receipt_protocol_anomaly"));
        assert_eq!(facts.delivery.as_deref(), Some("may_have_been_sent"));
        assert_eq!(facts.partial, None, "未知不得被猜成部分或完整");
        assert_eq!(facts.path_completed, None);
        assert_eq!(facts.point_count, None, "没有事实就不许写点数");
        assert_eq!(facts.release.as_deref(), Some("unknown"));
    }

    /// RPR-04b：同一动作/同一任务重复提交不得再次注入输入。
    #[tokio::test]
    async fn repeated_submission_of_the_same_task_never_injects_again() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt-repeat.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = failing_receipt_factory(
            &executions,
            "stale_observation",
            false,
            partial_path_spec(),
        );
        let executor =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default());
        // 同一 session/turn/objective，但 provider tool call id 不同（模型重发同一动作）。
        let first = executor
            .execute(&input("desktop"), &identity("repeat-a"))
            .await;
        let second = executor
            .execute(&input("desktop"), &identity("repeat-b"))
            .await;

        assert_eq!(
            executions.load(Ordering::SeqCst),
            1,
            "同一任务重复提交不得再次注入输入"
        );
        assert_eq!(
            second.error.as_ref().map(|error| error.code.as_str()),
            first.error.as_ref().map(|error| error.code.as_str())
        );
        assert_eq!(
            second.status, first.status,
            "重复提交必须复用同一个终态结果"
        );
    }

    /// RPR-04b：成功路径也必须携带回执，且持久化的兼容字段与回执一致。
    #[tokio::test]
    async fn successful_step_persists_the_same_facts_as_its_receipt() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt-success.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = ReceiptActFactory {
            executions: Arc::clone(&executions),
            failure: None,
            attach_receipt: true,
            mismatch_action_id: false,
            spec: ReceiptSpec {
                input_delivery: runtime::InputDelivery::Sent,
                partial: Some(false),
                path_completed: Some(true),
                confirmed_point_count: Some(5),
                input_release: runtime::InputReleaseStatus::Released,
            },
        };
        let identity = identity("receipt-success");

        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;

        assert!(!result.goal_achieved, "验收未通过不能算成功");
        let facts = step_facts(&path, &identity.call_id);
        assert_ne!(facts.status.as_deref(), Some("receipt_protocol_anomaly"));
        assert_eq!(facts.delivery.as_deref(), Some("sent"));
        assert_eq!(facts.partial, Some(0));
        assert_eq!(facts.path_completed, Some(1));
        assert_eq!(facts.point_count, Some(5));
        assert_eq!(facts.release.as_deref(), Some("released"));
    }

    /// RPR-04b：旧错误 JSON（完全没有 receipt 键）必须被新消费者兼容读取，
    /// 但**不得凭空虚构输入事实**——只能维持"可能已发送 + 释放未知"的保守分类。
    #[tokio::test]
    async fn legacy_error_json_without_receipt_is_read_but_never_invents_input_facts() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt-legacy.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = FakePlanner::one_click();
        let executions = Arc::new(AtomicUsize::new(0));
        // 旧生产者的错误 JSON：没有 receipt 键。
        let legacy = serde_json::json!({
            "code": "input_failed",
            "message": "cursor_move_failed",
            "retryable": true,
            "retry_owner": "system"
        });
        let parsed: ComputerUseError =
            serde_json::from_value(legacy).expect("旧错误 JSON 必须仍可读取");
        assert!(parsed.receipt().is_none());
        let factory = ReceiptActFactory {
            executions: Arc::clone(&executions),
            failure: Some(parsed),
            attach_receipt: false,
            mismatch_action_id: false,
            spec: partial_path_spec(),
        };
        let identity = identity("receipt-legacy");

        let result =
            ComputerUseExecutor::new_for_test(&planner, &factory, &store, ComputerUseBudgets::default())
                .execute(&input("desktop"), &identity)
                .await;

        assert!(!result.goal_achieved);
        assert!(
            result.error.as_ref().is_some_and(|error| error.receipt().is_none()),
            "没有回执就不得凭空补一个"
        );
        let facts = step_facts(&path, &identity.call_id);
        assert_ne!(facts.status.as_deref(), Some("receipt_protocol_anomaly"));
        assert_eq!(
            facts.delivery.as_deref(),
            Some("may_have_been_sent"),
            "旧错误不得被当成确定零输入"
        );
        assert_eq!(facts.partial, None);
        assert_eq!(facts.path_completed, None);
        assert_eq!(facts.point_count, None);
        assert_eq!(facts.release.as_deref(), Some("unknown"));
    }

    /// RPR-04b §2.4：错误被多层包装（纠正预算耗尽时换错误码）不得丢掉回执。
    #[test]
    fn error_wrappers_never_drop_an_attached_receipt() {        let receipt = runtime::ActionReceipt {
            action_id: "desktop:drag:abc".into(),
            input_delivery: runtime::InputDelivery::Sent,
            partial: Some(true),
            path_completed: Some(false),
            confirmed_point_count: Some(2),
            effect: runtime::EffectStatus::NotObserved,
            goal_verdict: runtime::GoalVerdict::NotChecked,
            input_release: runtime::InputReleaseStatus::Unknown,
        };
        receipt.validate().expect("样例回执必须自洽");
        let error = ComputerUseError::recoverable("stale_observation", "mid input")
            .with_receipt(receipt.clone());

        let wrapped = limit_input_correction(error, MAX_INVALID_INPUTS_PER_TURN);
        assert_eq!(wrapped.code, "input_correction_budget_exhausted");
        assert_eq!(
            wrapped.receipt(),
            Some(&receipt),
            "换错误码不得丢掉已知输入事实"
        );

        let kept = limit_input_correction(
            ComputerUseError::recoverable("stale_observation", "x").with_receipt(receipt.clone()),
            0,
        );
        assert_eq!(kept.receipt(), Some(&receipt));
    }

    // ---- RPR-05b-1：未确认释放的跨 run 互锁 + 可审计解除 ----

    use crate::computer_use_store::{ReleaseResolutionOperator, ReleaseResolutionRequest};

    fn identity_in(provider: &str, session_id: &str, turn_id: &str) -> ToolCallIdentity {
        ToolCallIdentity::from_provider(provider, session_id, turn_id)
    }

    /// 只经 store API 落一条"释放事实"（不起执行器），用于精确构造判据组合。
    fn seed_release_fact(
        store: &ComputerUseRunStore,
        identity: &ToolCallIdentity,
        delivery: Option<runtime::InputDelivery>,
        release: Option<runtime::InputReleaseStatus>,
    ) {
        assert!(store
            .create_run(&NewComputerUseRun {
                call_id: identity.call_id.clone(),
                provider_tool_call_id: Some(identity.provider_tool_call_id.clone()),
                session_id: identity.session_id.clone(),
                turn_id: identity.turn_id.clone(),
                chat_room_id: Some("room-1".into()),
                idempotency_key: format!("seeded-{}", identity.call_id),
                objective_json: "{\"objective\":\"seeded unconfirmed release\"}".into(),
                surface: ComputerUseSurface::Desktop,
                deadline_ms: 60_000,
                created_at_ms: 1,
                workspace: crate::computer_use_store::CuWorkspaceAttribution::test_fixture(),
            })
            .unwrap());
        assert!(store
            .append_step(&ComputerUseStepRecord {
                run_id: identity.call_id.clone(),
                step_index: 0,
                observation_generation: 1,
                action_type: "click".into(),
                normalized_target: "seeded-target".into(),
                action_fingerprint: "0000000000000000".into(),
                status: "failed".into(),
                error_code: Some("mouse_release_failed".into()),
                before_evidence_ref: None,
                after_evidence_ref: None,
                visible_progress: false,
                input_delivery: delivery,
                partial: None,
                path_completed: None,
                confirmed_point_count: None,
                effect_status: None,
                goal_verdict: None,
                input_release_status: release,
                started_at_ms: 5,
                completed_at_ms: Some(6),
            })
            .unwrap());
    }

    /// run 行的不可变快照（state + 终态结果）：用于证明解除不改写旧 run。
    fn run_row_snapshot(run_path: &std::path::Path, call_id: &str) -> (String, Option<String>) {
        Connection::open(run_path)
            .unwrap()
            .query_row(
                "SELECT state, terminal_result_json FROM computer_use_runs WHERE call_id=?1",
                [call_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    /// 解除请求：`(前置, 后置)` 输入所有者 epoch 是现场检查事实，测试按真实入口的样子传。
    fn resolution_request<'a>(
        session_id: &'a str,
        turn_id: &'a str,
    ) -> ReleaseResolutionRequest<'a> {
        ReleaseResolutionRequest {
            session_id,
            turn_id,
            operator_source: ReleaseResolutionOperator::NativeUi,
            operator_id: Some("local-operator"),
            reason: "已在桌面确认残留按键已抬起并释放",
            operator_check_note: "肉眼确认鼠标左键与键盘修饰键均未处于按下状态",
            input_owner_epoch_before: Some(7),
            input_owner_epoch_after: Some(8),
        }
    }

    /// (a) `unknown` + `may_have_been_sent` → 同一 turn 的**新 run** 被互锁阻断，
    /// 且阻断发生在取得输入 lease 之前：原生输入零调用、无 step、broker 仍空闲。
    #[cfg(windows)]
    #[tokio::test]
    async fn unconfirmed_release_interlocks_a_new_run_before_the_input_lease() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interlock.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = failing_receipt_factory(
            &executions,
            "mouse_release_failed",
            false,
            unknown_facts_spec(),
        );
        let first_identity = identity("interlock-first");
        let first = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &first_identity)
        .await;
        assert_eq!(first.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            first.error.as_ref().map(|error| error.code.as_str()),
            Some("mouse_release_failed")
        );
        let seeded = step_facts(&path, &first_identity.call_id);
        assert_eq!(seeded.delivery.as_deref(), Some("may_have_been_sent"));
        assert_eq!(seeded.release.as_deref(), Some("unknown"));
        assert!(store
            .has_unconfirmed_release(&first_identity.session_id, &first_identity.turn_id)
            .unwrap());

        // 新 run：不同 provider call id 且不同 objective（否则会命中同一任务的终态缓存）。
        let mut second_input = input("desktop");
        second_input["objective"] = json!("a different independent UI task");
        let second_identity = identity("interlock-second");
        let blocked = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&second_input, &second_identity)
        .await;

        assert_eq!(blocked.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(blocked.stage, ComputerUseStage::Supervisor);
        let error = blocked.error.as_ref().expect("互锁必须留下终态失败");
        assert_eq!(error.code, "input_release_unconfirmed_interlock");
        assert!(!error.retryable);
        assert_eq!(error.retry_owner, ComputerUseRetryOwner::User);
        // 输入前拒绝的回执语义：可证明未发送、无释放义务。
        let receipt = error.receipt().expect("互锁必须带输入前回执");
        assert_eq!(receipt.input_delivery, runtime::InputDelivery::NotSent);
        assert_eq!(receipt.input_release, runtime::InputReleaseStatus::NotNeeded);
        assert!(receipt.validate().is_ok(), "回执必须自洽");
        // 原生输入零调用：唯一一次 act 属于第一个 run。
        assert_eq!(
            executions.load(Ordering::SeqCst),
            1,
            "互锁必须在原生输入边界之前返回"
        );
        assert_eq!(
            step_row_count(&path, &second_identity.call_id),
            0,
            "被阻断的 run 不得留下任何 step"
        );
        // 阻断发生在取得输入 lease 之前：broker 此刻仍然空闲，别人能立刻取到。
        let scope = windows_process_guard::current_interactive_session_scope()
            .expect("resolve windows input session scope");
        let probe = windows_process_guard::interactive_input_lease_broker()
            .acquire(scope, "interlock-lease-probe")
            .expect("被互锁阻断的 run 不得占用输入 lease");
        probe.release();
        // 阻断结果照常落盘，便于审计。
        let stored = store.load(&second_identity.call_id).unwrap().unwrap();
        assert_eq!(stored.terminal_result.as_ref(), Some(&blocked));
    }

    /// (b) `not_sent` → 不阻断。两条都覆盖：生产配对（`not_sent` + `not_needed`），
    /// 以及判据第二条（投递明确 `not_sent` 时释放未知也不算"未确认释放"）。
    #[cfg(windows)]
    #[tokio::test]
    async fn not_sent_release_facts_do_not_interlock_the_next_run() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let first_store = store();
        let seeded = identity("seeded-not-sent");
        seed_release_fact(
            &first_store,
            &seeded,
            Some(runtime::InputDelivery::NotSent),
            Some(runtime::InputReleaseStatus::NotNeeded),
        );
        assert!(!first_store
            .has_unconfirmed_release("session-1", "turn-1")
            .unwrap());
        let allowed_factory = factory(true);
        let allowed = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &allowed_factory,
            &first_store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &identity("after-not-sent"))
        .await;
        assert_eq!(allowed.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(
            allowed_factory.action_count.load(Ordering::SeqCst),
            1,
            "not_sent 不得阻断新的输入"
        );

        let second_store = store();
        let second_seeded = identity_in("seeded-not-sent-unknown", "session-1", "turn-2");
        seed_release_fact(
            &second_store,
            &second_seeded,
            Some(runtime::InputDelivery::NotSent),
            Some(runtime::InputReleaseStatus::Unknown),
        );
        assert!(!second_store
            .has_unconfirmed_release("session-1", "turn-2")
            .unwrap());
        let second_factory = factory(true);
        let second_allowed = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &second_factory,
            &second_store,
            ComputerUseBudgets::default(),
        )
        .execute(
            &input("desktop"),
            &identity_in("after-not-sent-unknown", "session-1", "turn-2"),
        )
        .await;
        assert_eq!(second_allowed.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(second_factory.action_count.load(Ordering::SeqCst), 1);
    }

    /// (c) 解除事实落库后允许新 run，但旧 run 不被复活、历史 `unknown` 不被改写。
    #[cfg(windows)]
    #[tokio::test]
    async fn a_resolved_unconfirmed_release_allows_a_new_run_without_reviving_the_old_one() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resolve.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        // 三个 run 都在本测试里创建，所以把 per-turn 额度放宽（与本工单的判据无关）。
        let budgets = ComputerUseBudgets {
            max_calls_per_turn: 4,
            ..ComputerUseBudgets::default()
        };
        let old = identity("resolve-old");
        seed_release_fact(
            &store,
            &old,
            Some(runtime::InputDelivery::MayHaveBeenSent),
            Some(runtime::InputReleaseStatus::Unknown),
        );
        let old_terminal = terminal_result(
            &old,
            ComputerUseSurface::Desktop,
            ComputerUseStage::Execution,
            ComputerUseError::blocked(
                "mouse_release_failed",
                "release could not be confirmed",
                ComputerUseRetryOwner::System,
            ),
        );
        assert!(store.finish(&old.call_id, 0, &old_terminal).unwrap());
        let before = run_row_snapshot(&path, &old.call_id);
        assert!(store.has_unconfirmed_release("session-1", "turn-1").unwrap());

        // 未解除 → 新 run 被互锁阻断。
        let blocked_factory = factory(true);
        let blocked = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &blocked_factory,
            &store,
            budgets,
        )
        .execute(&input("desktop"), &identity("resolve-blocked"))
        .await;
        assert_eq!(
            blocked.error.as_ref().map(|error| error.code.as_str()),
            Some("input_release_unconfirmed_interlock")
        );
        assert_eq!(
            blocked_factory.action_count.load(Ordering::SeqCst),
            0,
            "互锁必须在原生输入之前返回"
        );

        // 人工解除：追加事实。
        let outcome = store
            .resolve_unconfirmed_release(&resolution_request("session-1", "turn-1"))
            .unwrap();
        assert!(outcome.recorded);
        assert_eq!(outcome.previous_epoch, 0);
        assert_eq!(outcome.epoch, 1);
        assert_eq!(outcome.covered_run_ids, vec![old.call_id.clone()]);
        assert_eq!(outcome.covered_step_count, 1);
        assert_eq!(outcome.remaining_unconfirmed_runs, 0);
        assert!(!store.has_unconfirmed_release("session-1", "turn-1").unwrap());

        // 解除后允许新任务/新 attempt（同一 turn 内，但必须是不同的任务身份）。
        let mut next = input("desktop");
        next["objective"] = json!("a fresh UI task after the resolution");
        let after_resolution_factory = factory(true);
        let allowed = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &after_resolution_factory,
            &store,
            budgets,
        )
        .execute(&next, &identity("resolve-after"))
        .await;
        assert_eq!(allowed.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(
            after_resolution_factory.action_count.load(Ordering::SeqCst),
            1
        );

        // 旧 run 未被复活：state 与终态结果一字不差，历史释放事实仍是 unknown。
        assert_eq!(run_row_snapshot(&path, &old.call_id), before);
        let facts = step_facts(&path, &old.call_id);
        assert_eq!(facts.status.as_deref(), Some("failed"));
        assert_eq!(facts.delivery.as_deref(), Some("may_have_been_sent"));
        assert_eq!(facts.release.as_deref(), Some("unknown"));
    }

    /// 解除只覆盖"当时那批 run"：解除之后新产生的未确认释放必须重新武装互锁
    /// （它不是一次性永久豁免）。
    #[cfg(windows)]
    #[tokio::test]
    async fn a_new_unconfirmed_release_after_a_resolution_re_arms_the_interlock() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rearm.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let budgets = ComputerUseBudgets {
            max_calls_per_turn: 8,
            ..ComputerUseBudgets::default()
        };
        let executions = Arc::new(AtomicUsize::new(0));
        let factory = failing_receipt_factory(
            &executions,
            "mouse_release_failed",
            false,
            unknown_facts_spec(),
        );
        let mut first_input = input("desktop");
        first_input["objective"] = json!("first unconfirmed task");
        let first = ComputerUseExecutor::new_for_test(&FakePlanner::one_click(), &factory, &store, budgets)
            .execute(&first_input, &identity("rearm-first"))
            .await;
        assert_eq!(first.status, ComputerUseTerminalStatus::Blocked);
        let outcome = store
            .resolve_unconfirmed_release(&resolution_request("session-1", "turn-1"))
            .unwrap();
        assert!(outcome.recorded);
        assert_eq!(outcome.epoch, 1);

        // 解除之后又出现一次未确认释放 → 互锁重新生效。
        let mut second_input = input("desktop");
        second_input["objective"] = json!("second unconfirmed task");
        let second = ComputerUseExecutor::new_for_test(&FakePlanner::one_click(), &factory, &store, budgets)
            .execute(&second_input, &identity("rearm-second"))
            .await;
        assert_eq!(second.status, ComputerUseTerminalStatus::Blocked);
        let mut third_input = input("desktop");
        third_input["objective"] = json!("third task after the re-armed interlock");
        let third = ComputerUseExecutor::new_for_test(&FakePlanner::one_click(), &factory, &store, budgets)
            .execute(&third_input, &identity("rearm-third"))
            .await;
        assert_eq!(
            third.error.as_ref().map(|error| error.code.as_str()),
            Some("input_release_unconfirmed_interlock")
        );
        assert_eq!(
            executions.load(Ordering::SeqCst),
            2,
            "两次真实输入之后不得再注入第三次"
        );
        let second_resolution = store
            .resolve_unconfirmed_release(&resolution_request("session-1", "turn-1"))
            .unwrap();
        assert_eq!(second_resolution.previous_epoch, 1);
        assert_eq!(second_resolution.epoch, 2);
        assert_eq!(second_resolution.covered_run_ids.len(), 1);
    }

    /// (d) 重启不能清空：互锁事实来自 sqlite 行，重新打开同一个库后依然生效。
    #[cfg(windows)]
    #[tokio::test]
    async fn the_unconfirmed_release_interlock_survives_a_store_reopen() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("restart.sqlite3");
        let old = identity("restart-old");
        {
            let store = ComputerUseRunStore::open(&path).unwrap();
            seed_release_fact(
                &store,
                &old,
                Some(runtime::InputDelivery::MayHaveBeenSent),
                Some(runtime::InputReleaseStatus::Unknown),
            );
            assert!(store.has_unconfirmed_release("session-1", "turn-1").unwrap());
            // store 连同连接在这里销毁：等价于进程退出。
        }

        // 重新打开同一个 sqlite（schema 初始化会再跑一次）。
        let reopened = ComputerUseRunStore::open(&path).unwrap();
        let facts = reopened
            .unconfirmed_release_facts("session-1", "turn-1")
            .unwrap();
        assert_eq!(facts.run_ids, vec![old.call_id.clone()]);
        assert_eq!(facts.step_count, 1);
        let factory = factory(true);
        let blocked = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &reopened,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &identity("restart-new"))
        .await;
        assert_eq!(
            blocked.error.as_ref().map(|error| error.code.as_str()),
            Some("input_release_unconfirmed_interlock")
        );
        assert_eq!(
            factory.action_count.load(Ordering::SeqCst),
            0,
            "重启之后仍必须是零原生输入"
        );
        // 历史行同样没有被 schema 初始化改写。
        let facts = step_facts(&path, &old.call_id);
        assert_eq!(facts.delivery.as_deref(), Some("may_have_been_sent"));
        assert_eq!(facts.release.as_deref(), Some("unknown"));
    }

    /// 结构证据：互锁判据在源码里必须排在 `interactive_input_lease_broker()` 之前。
    ///
    /// 这条测试锁的是"取得输入 lease 之前"这个顺序约束本身，避免后续重构把闸门
    /// 挪到 lease 之后（那样就可能先取得所有权再拒绝，语义完全不同）。
    #[test]
    fn input_release_interlock_guard_is_positioned_before_the_input_lease_acquisition() {
        let source = include_str!("computer_use_executor.rs");
        let body = source
            .split("async fn execute_in_room_with_policy")
            .nth(1)
            .expect("execute_in_room_with_policy")
            .split("fn create_and_finish")
            .next()
            .expect("execute_in_room_with_policy body");
        // 只比"代码出现的先后"：两个探针都取实际调用点，避免注释里的名字造成假顺序。
        let guard = body
            .find("unconfirmed_release_interlock_error(&identity.call_id")
            .expect("互锁判据必须写在 execute_in_room_with_policy 里");
        let lease = body
            .find("windows_process_guard::interactive_input_lease_broker()")
            .expect("桌面路径必须仍然真实取得输入 lease");
        assert!(
            guard < lease,
            "互锁必须在取得输入 lease 之前触发（当前顺序：判据 {guard}，lease {lease}）"
        );
    }

    /// 判据的失败方向：读不到事实时按最保守方向阻断，绝不因为"查不出来"而放行输入。
    #[test]
    fn interlock_decision_fails_closed_when_the_facts_cannot_be_read() {
        assert!(matches!(
            release_interlock_decision(Ok(Default::default())),
            ReleaseInterlockDecision::Clear
        ));
        let facts = crate::computer_use_store::UnconfirmedReleaseFacts {
            run_ids: vec!["cu-a".into()],
            ..Default::default()
        };
        assert!(matches!(
            release_interlock_decision(Ok(facts)),
            ReleaseInterlockDecision::Unresolved(1)
        ));
        assert!(matches!(
            release_interlock_decision(Err(rusqlite::Error::InvalidQuery)),
            ReleaseInterlockDecision::Unverifiable(_)
        ));
    }

    /// 输入前互锁回执与动作回执不可能混淆：身份以 run 为主体，且自洽。
    #[test]
    fn run_scope_interlock_receipt_is_not_mistakable_for_an_action_receipt() {
        let receipt = run_scope_pre_input_receipt("cu-session-1-turn-1-toolu-1");
        assert_eq!(receipt.action_id, "run-scope:cu-session-1-turn-1-toolu-1");
        assert!(receipt.validate().is_ok());
        assert_eq!(receipt.input_delivery, runtime::InputDelivery::NotSent);
        assert_eq!(receipt.input_release, runtime::InputReleaseStatus::NotNeeded);
        assert_eq!(receipt.partial, Some(false));
        assert_eq!(receipt.path_completed, None);
    }

    /// 两个互锁错误都是输入前拒绝：码可区分，消息带上前置检查事实，回执一律 not_sent。
    #[test]
    fn interlock_errors_are_pre_input_rejections_with_distinct_codes() {
        let interlock = unconfirmed_release_interlock_error("cu-x", 2);
        assert_eq!(interlock.code, "input_release_unconfirmed_interlock");
        assert!(
            interlock.message.contains('2'),
            "消息必须带上未解除 run 数这一前置检查事实"
        );
        assert_eq!(
            interlock.receipt().map(|receipt| receipt.input_delivery),
            Some(runtime::InputDelivery::NotSent)
        );

        let check_failed = unconfirmed_release_check_failed_error("cu-x", "database is locked");
        assert_eq!(check_failed.code, "input_release_interlock_check_failed");
        assert!(check_failed.message.contains("database is locked"));
        assert_eq!(
            check_failed.receipt().map(|receipt| receipt.input_delivery),
            Some(runtime::InputDelivery::NotSent)
        );
    }

    // ---- RPR-11c 第 3 项：CU 截止时间的建立点与预算事实 ----

    /// 源码顺序：CU 截止时间必须在**任务被接纳并进入调度**时建立，且早于
    /// 未确认释放互锁、租约等待、适配器构建（含本地模型切换）、初始观察与规划请求。
    #[test]
    fn cu_deadline_is_established_before_interlock_lease_and_observation() {
        let source = include_str!("computer_use_executor.rs");
        let accepted_path = source
            .split("async fn execute_in_room_with_policy")
            .nth(1)
            .expect("accepted run path")
            .split("#[cfg(test)]")
            .next()
            .expect("runtime body");
        let position = |needle: &str| {
            accepted_path
                .find(needle)
                .unwrap_or_else(|| panic!("缺少锚点：{needle}"))
        };

        let established = position("CuDeadline::establish_without_root");
        assert!(
            established < position("match release_interlock_decision("),
            "截止时间必须早于未确认释放互锁（互锁失败会直接终态）"
        );
        assert!(
            established < position("windows_process_guard::ScopedInputOwnership::acquire("),
            "截止时间必须早于租约等待：等待耗时不能被藏在预算之外"
        );
        assert!(
            established < position("self.adapters.build(surface)"),
            "截止时间必须早于适配器构建与随之发生的本地模型切换"
        );
        assert!(
            established < position(".with_cu_deadline(cu_deadline)"),
            "控制器必须接收这同一个截止时间"
        );
        assert!(
            established < position("ComputerUseRunContext {"),
            "截止时间必须早于控制器入口（初始观察与规划都在它里面）"
        );
    }

    /// 初始观察/模型切换真实耗时 250ms：这段时间必须从 CU 预算里扣掉，
    /// 且运行记录必须能分别表达 `not_wired` / `absent` / 真实的 `cu_deadline`。
    #[tokio::test]
    async fn cu_deadline_covers_initial_observation_and_is_recorded_once() {
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        struct ProbePlanner {
            seen_ms: Mutex<Vec<u64>>,
        }
        impl ComputerUsePlanner for ProbePlanner {
            fn classify<'a>(
                &'a self,
                request: &'a ComputerUseRequest,
                _: &'a Observation,
                _: std::time::Duration,
            ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
                Box::pin(async move { Ok(request.surface) })
            }
            fn next_action<'a>(
                &'a self,
                _: &'a ComputerUseRequest,
                _: &'a Observation,
                _: usize,
                remaining: std::time::Duration,
            ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
                self.seen_ms
                    .lock()
                    .unwrap()
                    .push(remaining.as_millis().min(u128::from(u64::MAX)) as u64);
                Box::pin(async move { Ok(None) })
            }
            fn verify<'a>(
                &'a self,
                _: &'a ComputerUseRequest,
                _: &'a Observation,
                _: &'a Observation,
                verification: Verification,
                remaining: std::time::Duration,
            ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
                self.seen_ms
                    .lock()
                    .unwrap()
                    .push(remaining.as_millis().min(u128::from(u64::MAX)) as u64);
                Box::pin(async move { Ok(verification) })
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cu-budget-facts.sqlite3");
        let store = ComputerUseRunStore::open(&path).unwrap();
        let planner = ProbePlanner {
            seen_ms: Mutex::new(Vec::new()),
        };
        let factory = slow_observe_factory(250);
        let identity = identity("cu-budget-facts");

        let result = ComputerUseExecutor::new_for_test(
            &planner,
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &identity)
        .await;

        // 1) 初始观察的 250ms 必须计入预算：规划者看到的剩余明显小于完整预算。
        let seen = planner.seen_ms.lock().unwrap().clone();
        assert!(!seen.is_empty(), "规划者必须被调用过");
        assert!(
            seen.iter().all(|remaining| *remaining <= 120_000 - 250),
            "接纳之后消耗的 250ms 不允许从预算里消失：{seen:?}"
        );

        // 2) 运行结果必须分别表达三个事实。
        let facts = result.cu_budget.expect("被接纳的 CU 任务必须写预算事实");
        assert_eq!(facts.root_deadline_state.as_str(), "not_wired");
        assert_eq!(facts.root_deadline, computer_use::RootDeadline::Absent);
        let cu_deadline_ms = facts.cu_deadline_ms.expect("必须建立真实的 CU 截止时间");
        assert_eq!(
            facts.cu_deadline_established_at_ms,
            Some(cu_deadline_ms - 120_000),
            "CU 截止时间 = 接纳时刻 + CU 任务预算"
        );

        // 3) 落库的 deadline_ms 必须就是同一个 CU 截止时间（不是另行计算的第二个值）。
        let connection = Connection::open(&path).unwrap();
        let (created_at_ms, deadline_ms): (u64, u64) = connection
            .query_row(
                "SELECT created_at_ms,deadline_ms FROM computer_use_runs WHERE call_id=?1",
                [&identity.call_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(deadline_ms, cu_deadline_ms);
        assert_eq!(deadline_ms, created_at_ms + 120_000);

        // 4) 运行记录自身（terminal_result_json）就能表达这三件事。
        let json: String = connection
            .query_row(
                "SELECT terminal_result_json FROM computer_use_runs WHERE call_id=?1",
                [&identity.call_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(json.contains("\"root_deadline_state\":\"not_wired\""), "{json}");
        assert!(json.contains("\"root_deadline\":\"absent\""), "{json}");
        assert!(
            json.contains(&format!("\"cu_deadline_ms\":{cu_deadline_ms}")),
            "{json}"
        );
    }

    /// 参数未通过、尚未接纳执行的请求：沿用既有参数纠错规则，
    /// **不得**伪装成已经启动的 CU 任务（没有 CU 截止时间）。
    #[tokio::test]
    async fn rejected_request_never_pretends_a_cu_task_was_started() {
        let store = store();
        let planner = FakePlanner::one_click();
        let factory = factory(true);
        let identity = identity("rejected-budget-facts");

        let result = ComputerUseExecutor::new_for_test(
            &planner,
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(
            &json!({"objective": "   ", "surface": "desktop", "success_criteria": []}),
            &identity,
        )
        .await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("invalid_objective")
        );
        let facts = result.cu_budget.expect("必须留下预算事实");
        assert!(!facts.is_accepted(), "未接纳的请求没有 CU 截止时间");
        assert_eq!(facts.cu_deadline_ms, None);
        assert_eq!(facts.root_deadline_state.as_str(), "not_wired");
        assert_eq!(facts.root_deadline, computer_use::RootDeadline::Absent);
        assert_eq!(
            factory.action_count.load(Ordering::SeqCst),
            0,
            "未接纳的请求不得产生任何输入"
        );
    }

    // ---- CU-F03（裁决 §5.1）：工作区归属的冻结、写入、读回与互锁 ----

    /// 在**第一次规划动作**时切换"当前工作区"的替身：注入点刻意选在运行开始之后，
    /// 用来模拟"用户在 CU 运行期间切换 UI 工作区"。
    struct SwitchWorkspaceOnFirstAction {
        inner: FakePlanner,
        target: std::path::PathBuf,
        switched: AtomicBool,
    }

    impl SwitchWorkspaceOnFirstAction {
        fn new(target: std::path::PathBuf) -> Self {
            Self {
                inner: FakePlanner::one_click(),
                target,
                switched: AtomicBool::new(false),
            }
        }
    }

    impl ComputerUsePlanner for SwitchWorkspaceOnFirstAction {
        fn verify<'a>(
            &'a self,
            request: &'a ComputerUseRequest,
            before: &'a Observation,
            after: &'a Observation,
            verification: Verification,
            remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Verification, ComputerUseError>> {
            self.inner.verify(request, before, after, verification, remaining)
        }

        fn classify<'a>(
            &'a self,
            request: &'a ComputerUseRequest,
            observation: &'a Observation,
            remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<ComputerUseSurface, ComputerUseError>> {
            self.inner.classify(request, observation, remaining)
        }

        fn next_action<'a>(
            &'a self,
            request: &'a ComputerUseRequest,
            observation: &'a Observation,
            step: usize,
            remaining: std::time::Duration,
        ) -> PlannerFuture<'a, Result<Option<ComputerUseAction>, ComputerUseError>> {
            if !self.switched.swap(true, Ordering::SeqCst) {
                if let Ok(mut state) = crate::workspace_state().lock() {
                    state.current = self.target.clone();
                }
            }
            self.inner.next_action(request, observation, step, remaining)
        }
    }

    /// 恢复进程级"当前工作区"的 RAII 守卫（测试不得把全局状态留在别的目录上）。
    struct CurrentWorkspaceGuard {
        previous: std::path::PathBuf,
    }

    impl CurrentWorkspaceGuard {
        fn install(target: std::path::PathBuf) -> Self {
            let mut state = crate::workspace_state().lock().unwrap();
            let previous = std::mem::replace(&mut state.current, target);
            Self { previous }
        }
    }

    impl Drop for CurrentWorkspaceGuard {
        fn drop(&mut self) {
            if let Ok(mut state) = crate::workspace_state().lock() {
                state.current = self.previous.clone();
            }
        }
    }

    /// 测试专属的"当前会话库"覆盖点守卫（避免污染真实工作区与会话库）。
    struct SessionDbPathGuard {
        previous: Option<std::path::PathBuf>,
    }

    impl SessionDbPathGuard {
        fn install(path: std::path::PathBuf) -> Self {
            Self {
                previous: crate::replace_session_db_path_override_for_test(Some(path)),
            }
        }
    }

    impl Drop for SessionDbPathGuard {
        fn drop(&mut self) {
            crate::replace_session_db_path_override_for_test(self.previous.take());
        }
    }

    /// 该 run 落盘的 step 行数（0 = 没有任何动作被记录，即零输入）。
    fn step_row_count_in_store(store: &ComputerUseRunStore, call_id: &str) -> usize {
        store.action_counts(call_id).expect("action counts").0
    }

    /// **T13（必测）**：CU 运行中切换 UI 工作区 ⇒ 原运行的 **workspace** 与 **数据库**不变。
    ///
    /// 注入点在运行开始之后（第一次规划动作时切换"当前工作区"），因此这条测试证明的是
    /// "接纳之后不再有第二次解析"：归属与库都已经定型。
    #[tokio::test(flavor = "current_thread")]
    async fn switching_the_ui_workspace_mid_run_keeps_workspace_and_database() {
        let _guard = crate::tests::config_test_guard();
        // 该 run 会真实走到桌面输入边界：与其它桌面 CU 测试共用同一把输入所有权锁。
        let _desktop_lease = crate::tests::desktop_input_lease_test_guard().await;
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("frozen-workspace.sqlite3");
        let store = ComputerUseRunStore::open(&db_path).unwrap();
        let frozen = CuWorkspaceAttribution::from_parent_run(
            &crate::canonical_workspace_identity("ws-0123456789abcdef").expect("规范标识"),
        );
        let switched_to = directory.path().join("another-workspace");
        let _workspace = CurrentWorkspaceGuard::install(directory.path().join("initial-workspace"));

        let factory = factory(true);
        let identity = identity("workspace-switch-mid-run");
        let result = ComputerUseExecutor::new(
            &SwitchWorkspaceOnFirstAction::new(switched_to.clone()),
            &factory,
            &store,
            ComputerUseBudgets::default(),
            frozen.clone(),
        )
        .execute(&input("desktop"), &identity)
        .await;

        // 运行确实执行到了输入（否则这条测试证明不了"运行中"）。
        assert_eq!(result.status, ComputerUseTerminalStatus::Succeeded);
        assert_eq!(factory.action_count.load(Ordering::SeqCst), 1);
        // 前置条件成立：运行期间"当前工作区"确实已经切到别处。
        assert_eq!(
            crate::active_workspace_path(),
            switched_to,
            "注入点必须真的切换了当前工作区"
        );

        // 归属不变：落库的仍是接纳时冻结的那个标识（读回口径也是"已记录"）。
        let stored = store.load(&identity.call_id).unwrap().expect("run row");
        assert_eq!(stored.workspace.workspace_id(), Some("ws-0123456789abcdef"));
        assert_eq!(stored.workspace.context_version(), Some(1));
        // 数据库不变：事实就在接纳时定型的这个库里，切换后的目录下没有被创建的运行库。
        drop(store);
        let reopened = ComputerUseRunStore::open(&db_path).unwrap();
        assert_eq!(
            reopened.load(&identity.call_id).unwrap().unwrap().workspace.workspace_id(),
            Some("ws-0123456789abcdef")
        );
        assert!(
            !switched_to.join(".coolzhu/web-sessions.sqlite3").exists(),
            "运行期间切换工作区不得让 CU 另建/改用另一个会话库"
        );
    }

    /// 接纳函数里**没有**第二次读取"当前工作区"，且运行库路径只解析一次（结构性守卫）。
    #[test]
    fn admission_freezes_workspace_and_database_without_reading_the_current_workspace_again() {
        let source = include_str!("computer_use_executor.rs");
        let admission = source
            .split("pub(crate) async fn execute_with_current_runtime")
            .nth(1)
            .expect("admission function")
            .split("#[cfg(test)]")
            .next()
            .expect("admission body");
        assert!(
            !admission.contains("active_workspace_path"),
            "接纳处不得读取\"当前工作区\"（归属只能来自父运行的冻结上下文）"
        );
        assert!(
            !admission.contains("workspace_identity"),
            "接纳处不得自行生成工作区标识（必须复用父运行的规范标识）"
        );
        assert!(
            admission.contains("parent: Option<&crate::FrozenParentContext>"),
            "接纳入口必须接收父运行的冻结上下文载体（裁决 B-5：不得改回只含 Option<String> 的临时工作区传参）"
        );
        assert_eq!(
            admission.matches("default_session_sqlite_path()").count(),
            1,
            "运行库路径必须只解析一次并冻结（避免运行中切工作区改变库归属）"
        );
    }

    /// 写入失败 ⇒ **不开始普通输入**（这里注入的是"库只读"：行写不进去 ⇒ 输入前阻断，零 step）。
    #[tokio::test(flavor = "current_thread")]
    async fn run_write_failure_blocks_before_any_input() {
        let _guard = crate::tests::config_test_guard();
        // 迁移完整（读得到），但写入被拒：验收点就是"接纳事实写不进去"。
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();
        apply_session_migration_v22(&connection).unwrap();
        connection.execute_batch("PRAGMA query_only = ON;").unwrap();
        let store = ComputerUseRunStore::from_connection(connection);
        let factory = factory(true);
        let identity = identity("write-failure");

        let result = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &identity)
        .await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(result.stage, ComputerUseStage::Supervisor);
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("persistence_conflict")
        );
        assert_eq!(
            factory.action_count.load(Ordering::SeqCst),
            0,
            "归属写不进去就不得产生任何物理输入"
        );
        assert_eq!(step_row_count_in_store(&store, &identity.call_id), 0);
        assert!(store.load(&identity.call_id).unwrap().is_none());
    }

    /// 旧活动运行缺归属 ⇒ 不启动新的业务输入；本次 run 仍收尾（证据保存）且历史行不被回填。
    #[tokio::test(flavor = "current_thread")]
    async fn legacy_open_run_without_workspace_blocks_new_input_without_backfilling() {
        let _guard = crate::tests::config_test_guard();
        let store = store();
        seed_unrecorded_active_run(&store, "legacy-open-run");
        let factory = factory(true);
        let identity = identity("after-legacy-open-run");

        let result = ComputerUseExecutor::new_for_test(
            &FakePlanner::one_click(),
            &factory,
            &store,
            ComputerUseBudgets::default(),
        )
        .execute(&input("desktop"), &identity)
        .await;

        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(result.stage, ComputerUseStage::Supervisor);
        let error = result.error.as_ref().expect("互锁必须留下终态失败");
        assert_eq!(error.code, "workspace_unrecorded_active_run");
        assert!(!error.retryable);
        assert_eq!(error.retry_owner, ComputerUseRetryOwner::System);
        let receipt = error.receipt().expect("互锁必须带输入前回执");
        assert_eq!(receipt.input_delivery, runtime::InputDelivery::NotSent);
        assert_eq!(receipt.input_release, runtime::InputReleaseStatus::NotNeeded);
        assert_eq!(
            factory.action_count.load(Ordering::SeqCst),
            0,
            "互锁必须在原生输入边界之前返回"
        );
        assert_eq!(step_row_count_in_store(&store, &identity.call_id), 0);

        // 收尾与证据保存照常：本次 run 有终态，且自己的归属是**已记录**的。
        let stored = store.load(&identity.call_id).unwrap().expect("run row");
        assert_eq!(stored.terminal_result.as_ref(), Some(&result));
        assert!(matches!(
            stored.workspace,
            crate::computer_use_store::StoredWorkspaceAttribution::Recorded { .. }
        ));
        // 历史行的归属仍然是 NULL（阻断不是回填）。
        assert_eq!(
            store.load("legacy-open-run").unwrap().unwrap().workspace,
            crate::computer_use_store::StoredWorkspaceAttribution::Unrecorded
        );
        assert_eq!(
            store
                .load("legacy-open-run")
                .unwrap()
                .unwrap()
                .workspace
                .text(),
            "历史归属未记录"
        );
    }

    /// 判据与错误契约：读不出来时**绝不放行**（与释放互锁同款），并带输入前回执。
    #[test]
    fn unrecorded_workspace_interlock_fails_closed_when_facts_cannot_be_read() {
        assert!(matches!(
            unrecorded_workspace_interlock(Ok(Vec::new())),
            UnrecordedWorkspaceInterlock::Clear
        ));
        assert!(matches!(
            unrecorded_workspace_interlock(Ok(vec!["legacy-open-run".to_string()])),
            UnrecordedWorkspaceInterlock::UnrecordedActiveRuns(1)
        ));
        match unrecorded_workspace_interlock(Err(rusqlite::Error::QueryReturnedNoRows)) {
            UnrecordedWorkspaceInterlock::Unverifiable(detail) => {
                assert!(!detail.is_empty(), "读失败必须带出原因")
            }
            _ => panic!("读不出事实时必须按最保守方向阻断"),
        }

        let blocked = unrecorded_workspace_interlock_error("cu-x", 2);
        assert_eq!(blocked.code, "workspace_unrecorded_active_run");
        assert_eq!(blocked.retry_owner, ComputerUseRetryOwner::System);
        let receipt = blocked.receipt().expect("必须带输入前回执");
        assert_eq!(receipt.input_delivery, runtime::InputDelivery::NotSent);
        assert_eq!(receipt.input_release, runtime::InputReleaseStatus::NotNeeded);

        let unverifiable = unrecorded_workspace_check_failed_error("cu-x", "boom");
        assert_eq!(unverifiable.code, "workspace_attribution_check_failed");
        assert_eq!(
            unverifiable.receipt().expect("必须带输入前回执").input_delivery,
            runtime::InputDelivery::NotSent
        );
    }

    /// 生产入口端到端：缺归属 / 空白 / 禁止值一律在**任何 store 操作之前**拒绝，零输入零运行行。
    #[tokio::test(flavor = "current_thread")]
    async fn admission_entry_refuses_forbidden_or_missing_workspace_with_zero_input() {
        let _guard = crate::tests::config_test_guard();
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("web-sessions.sqlite3");
        let _db = SessionDbPathGuard::install(db_path.clone());

        // A-2 后：执行器只剩"有/没有冻结上下文"一个拒绝条件。
        // "值是否合规"已由源头解析器在**接纳点**判定（工归属的审计码
        // 要求见 `a2_t1_source_parser_rejects_invalid_structures_with_stable_codes`
        // 与 `a2_t4_ordinary_input_cannot_construct_a_trusted_context`），不再经执行器。
        for (workspace, expected_code) in [(None, "input_context_incomplete")] {
            let identity = identity(&format!("entry-refusal-{expected_code}"));
            let result = execute_with_current_runtime(
                &input("browser"),
                &identity,
                Some("room-1"),
                workspace.as_ref(),
            )
            .await;
            assert_eq!(
                result.status,
                ComputerUseTerminalStatus::Blocked,
                "workspace={workspace:?}"
            );
            assert_eq!(result.stage, ComputerUseStage::IntentGuard);
            assert_eq!(
                result.error.as_ref().map(|error| error.code.as_str()),
                Some(expected_code),
                "workspace={workspace:?}"
            );
            assert_eq!(result.attempts, 0);
            assert_eq!(result.steps_completed, 0);
        }

        // 被拒绝的请求**没有**建立任何运行行：不存在"归属未记录的新运行"这种中间状态。
        let store = ComputerUseRunStore::open(&db_path).unwrap();
        // A-2 后执行器侧只会产生下面这些拒绝码（值合规性已在源头）；
        // 每一个都必须不留运行行。
        for code in [
            "input_context_incomplete",
            "parent_context_identity_missing",
            "parent_context_run_unknown",
            "parent_context_run_workspace_mismatch",
            "parent_context_run_session_mismatch",
            "parent_context_run_room_mismatch",
            "parent_context_run_turn_mismatch",
            "parent_context_run_not_executable",
            "parent_context_room_unknown",
            "parent_context_session_unknown",
        ] {
            let call_id = identity(&format!("entry-refusal-{code}")).call_id;
            assert!(
                store.load(&call_id).unwrap().is_none(),
                "被拒绝的 CU 请求不得留下运行行（{code}）"
            );
        }
    }

    /// 生产入口端到端：授权不够时也照常落一条**归属已记录**的运行行（在冻结的库里），
    /// 且零输入；随后切换当前工作区不影响该行的归属。
    #[tokio::test(flavor = "current_thread")]
    async fn admission_entry_records_the_frozen_workspace_in_the_frozen_database() {
        let _guard = crate::tests::config_test_guard();
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("web-sessions.sqlite3");
        let _db = SessionDbPathGuard::install(db_path.clone());
        let identity = identity("entry-frozen-workspace");
        let switched_to = directory.path().join("switched-workspace");
        // A-2 §3：接纳上下文的四维必须与**库中真实行**相符，因此先把会话/房间/运行行
        // 真实建出来（而不是在结构体内部自比）。
        seed_relation_complete_admission(&db_path, "ws-0123456789abcdef", "room-without-full-access", "session-1", "run-frozen-workspace");
        // 正式输入入口现在还要经共享输入安全库：注入库根并按资格开放该资源。
        let _safety_root = open_input_resource_for_test(&directory.path().join("input-safety"));
        let parent = crate::FrozenParentContext::new(
            "test-fixture",
            "ws-0123456789abcdef",
            Some("room-without-full-access"),
            Some("session-1"),
            None,
            Some("run-frozen-workspace"),
        )
        .ok();

        let result = execute_with_current_runtime(
            &input("browser"),
            &identity,
            Some("room-without-full-access"),
            parent.as_ref(),
        )
        .await;
        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("computer_use_room_full_access_required")
        );

        // 运行行在**冻结的库**里，归属就是冻结的那个值（新运行永不写 NULL）。
        let store = ComputerUseRunStore::open(&db_path).unwrap();
        let stored = store.load(&identity.call_id).unwrap().expect("run row");
        assert_eq!(stored.workspace.workspace_id(), Some("ws-0123456789abcdef"));
        assert_eq!(step_row_count_in_store(&store, &identity.call_id), 0);

        // 事后切换当前工作区：归属仍是接纳时那个值。
        let _workspace = CurrentWorkspaceGuard::install(switched_to);
        let reloaded = ComputerUseRunStore::open(&db_path).unwrap();
        assert_eq!(
            reloaded
                .load(&identity.call_id)
                .unwrap()
                .unwrap()
                .workspace
                .workspace_id(),
            Some("ws-0123456789abcdef")
        );
    }

    /// **第八轮 §1.4**：CU 是正式输入入口，因此**必须**经共享输入安全库检查资源状态——
    /// 库未注入、资源未登记（Unknown）、资源被隔离，三种情形都在**任何写入与输入之前**拒绝。
    #[tokio::test(flavor = "current_thread")]
    async fn input_admission_goes_through_the_shared_safety_store() {
        let _guard = crate::tests::config_test_guard();
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("web-sessions.sqlite3");
        let _db = SessionDbPathGuard::install(db_path.clone());
        let safety_root = directory.path().join("input-safety");
        seed_relation_complete_admission(
            &db_path,
            "ws-0123456789abcdef",
            "room-1",
            "session-1",
            "run-safety-gate",
        );
        let parent = crate::FrozenParentContext::new(
            "test-fixture",
            "ws-0123456789abcdef",
            Some("room-1"),
            Some("session-1"),
            None,
            Some("run-safety-gate"),
        )
        .ok();

        // ① 未注入库根 ⇒ 拒绝（不得回退到自造路径）。
        let saved = std::env::var_os(crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV);
        std::env::remove_var(crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV);
        let no_root_identity = identity("safety-gate-no-root");
        let result = execute_with_current_runtime(
            &input("browser"),
            &no_root_identity,
            Some("room-1"),
            parent.as_ref(),
        )
        .await;
        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("input_safety_root_not_injected")
        );
        if let Some(saved) = saved {
            std::env::set_var(crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV, saved);
        }

        // ② 注入了库根但资源从未登记（Unknown）⇒ 拒绝。
        // ③ 资源被隔离 ⇒ 拒绝。两者都必须在任何写入之前。
        let scope = crate::input_safety_store::physical_input_resource_scope().expect("scope");
        assert_eq!(scope.as_str(), crate::input_safety_store::physical_input_resource_scope().unwrap().as_str());
        std::env::set_var(
            crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV,
            safety_root.as_os_str(),
        );
        {
            let connection = crate::open_session_connection(&db_path).unwrap();
            let _ = connection;
        }
        // 建立库（首次初始化）但**不开放**资源。
        let _store = crate::input_safety_store::InputSafetyStore::open_at(&safety_root).expect("store");
        let unknown_identity = identity("safety-gate-unknown");
        let result = execute_with_current_runtime(
            &input("browser"),
            &unknown_identity,
            Some("room-1"),
            parent.as_ref(),
        )
        .await;
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("input_safety_resource_not_accepting_new_input"),
            "未登记/未开放的资源必须拒绝新输入"
        );
        assert_eq!(result.attempts, 0);
        let store = crate::computer_use_store::ComputerUseRunStore::open(&db_path).unwrap();
        assert!(
            store.load(&unknown_identity.call_id).unwrap().is_none(),
            "被输入安全门拒绝时不得留下运行行"
        );

        // ④ 按资格开放后放行（走到下一个门，而不是被输入安全门拒绝）。
        let _coordination = open_input_resource_for_test(&safety_root);
        let open_identity = identity("safety-gate-open");
        let result = execute_with_current_runtime(
            &input("browser"),
            &open_identity,
            Some("room-1"),
            parent.as_ref(),
        )
        .await;
        assert_ne!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("input_safety_resource_not_accepting_new_input"),
            "资源按资格开放后不得再被输入安全门拒绝"
        );
    }

    /// **A2-T5**：工作区正确但房间/会话/轮次/父运行关系不符时必须拒绝，
    /// 拒绝发生在**原生输入之前**（零尝试、不留运行行、不留步骤）。
    #[tokio::test(flavor = "current_thread")]
    async fn a2_t5_relation_mismatches_are_rejected_before_any_native_input() {
        let _guard = crate::tests::config_test_guard();
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("web-sessions.sqlite3");
        let _db = SessionDbPathGuard::install(db_path.clone());
        seed_relation_complete_admission(
            &db_path,
            "ws-0123456789abcdef",
            "room-1",
            "session-1",
            "run-relation",
        );

        // 每一行：(标签, 上下文参数, 期望拒绝码)
        let cases: Vec<(&str, Option<crate::FrozenParentContext>, &str)> = vec![
            (
                "run-unknown",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-0123456789abcdef",
                    Some("room-1"),
                    Some("session-1"),
                    None,
                    Some("run-does-not-exist"),
                )
                .ok(),
                "parent_context_run_unknown",
            ),
            (
                "workspace-mismatch",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-ffffffffffffffff",
                    Some("room-1"),
                    Some("session-1"),
                    None,
                    Some("run-relation"),
                )
                .ok(),
                "parent_context_run_workspace_mismatch",
            ),
            (
                "session-mismatch",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-0123456789abcdef",
                    Some("room-1"),
                    Some("session-2"),
                    None,
                    Some("run-relation"),
                )
                .ok(),
                "parent_context_run_session_mismatch",
            ),
            (
                "room-mismatch",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-0123456789abcdef",
                    Some("room-2"),
                    Some("session-1"),
                    None,
                    Some("run-relation"),
                )
                .ok(),
                "parent_context_run_room_mismatch",
            ),
            (
                "turn-mismatch",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-0123456789abcdef",
                    Some("room-1"),
                    Some("session-1"),
                    Some("turn-other"),
                    Some("run-relation"),
                )
                .ok(),
                "parent_context_run_turn_mismatch",
            ),
            (
                "unknown-room",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-0123456789abcdef",
                    Some("room-missing"),
                    Some("session-1"),
                    None,
                    Some("run-relation"),
                )
                .ok(),
                "parent_context_room_unknown",
            ),
            (
                "unknown-session",
                crate::FrozenParentContext::new(
                    "test-fixture",
                    "ws-0123456789abcdef",
                    None,
                    Some("session-missing"),
                    None,
                    Some("run-relation"),
                )
                .ok(),
                "parent_context_session_unknown",
            ),
        ];

        for (label, parent, expected_code) in cases {
            let identity = identity(&format!("a2-t5-{label}"));
            let result = execute_with_current_runtime(
                &input("browser"),
                &identity,
                Some("room-1"),
                parent.as_ref(),
            )
            .await;
            assert_eq!(
                result.status,
                ComputerUseTerminalStatus::Blocked,
                "{label} 必须被拒绝"
            );
            assert_eq!(
                result.error.as_ref().map(|error| error.code.as_str()),
                Some(expected_code),
                "{label} 的拒绝码必须指明具体维度"
            );
            // 零输入：拒绝发生在任何原生动作之前。
            assert_eq!(result.attempts, 0, "{label} 不得产生任何尝试");
            assert_eq!(result.steps_completed, 0, "{label} 不得完成任何步骤");
            let store = ComputerUseRunStore::open(&db_path).unwrap();
            assert!(
                store.load(&identity.call_id).unwrap().is_none(),
                "{label} 被拒绝后不得留下运行行"
            );
            assert_eq!(
                step_row_count_in_store(&store, &identity.call_id),
                0,
                "{label} 被拒绝后不得留下步骤"
            );
        }
    }

    /// **A2-T6**：普通模型 CU 缺**必需执行身份**时 fail-closed，且拒绝信息必须
    /// **指明缺哪个维度、哪个接纳入口尚未建立它**（不得因"入口没建"而豁免）。
    #[tokio::test(flavor = "current_thread")]
    async fn a2_t6_missing_parent_run_is_refused_with_the_dimension_and_entry_named() {
        let _guard = crate::tests::config_test_guard();
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("web-sessions.sqlite3");
        let _db = SessionDbPathGuard::install(db_path.clone());
        seed_relation_complete_admission(
            &db_path,
            "ws-0123456789abcdef",
            "room-1",
            "session-1",
            "run-relation",
        );
        // 非流式路径（不建 chat runtime run）的真实缺省：工作区/房间/会话都在，只缺父运行。
        let parent = crate::FrozenParentContext::new(
            "chat-send",
            "ws-0123456789abcdef",
            Some("room-1"),
            Some("session-1"),
            None,
            None,
        )
        .ok();
        let missing_run_identity = identity("a2-t6-missing-run");
        let result = execute_with_current_runtime(
            &input("browser"),
            &missing_run_identity,
            Some("room-1"),
            parent.as_ref(),
        )
        .await;
        assert_eq!(result.status, ComputerUseTerminalStatus::Blocked);
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("parent_context_identity_missing")
        );
        let message = result
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_default();
        assert!(message.contains("父运行"), "必须指明缺的维度：{message}");
        assert!(
            message.contains("chat-send"),
            "必须指明哪个接纳入口尚未建立它：{message}"
        );
        assert_eq!(result.attempts, 0);
        let store = ComputerUseRunStore::open(&db_path).unwrap();
        assert!(store.load(&missing_run_identity.call_id).unwrap().is_none());

        // 同一个接纳上下文只要**补齐真实父运行**就能通过关系核对（证明拒绝不是"永远拒绝"）。
        let parent_with_run = crate::FrozenParentContext::new(
            "chat-send-stream",
            "ws-0123456789abcdef",
            Some("room-1"),
            Some("session-1"),
            None,
            Some("run-relation"),
        )
        .ok();
        let with_run_identity = identity("a2-t6-with-run");
        let result = execute_with_current_runtime(
            &input("browser"),
            &with_run_identity,
            Some("room-1"),
            parent_with_run.as_ref(),
        )
        .await;
        assert_ne!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("parent_context_identity_missing"),
            "补齐真实父运行后不得再因身份缺失而拒绝"
        );
    }
}
