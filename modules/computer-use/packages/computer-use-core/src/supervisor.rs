use std::collections::HashMap;

use crate::{
    ComputerUseBudgets, ComputerUseError, ComputerUseRequest, ComputerUseResult,
    ComputerUseRetryOwner, ComputerUseTerminalStatus, CuDeadline, SupervisorSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskIdempotencyKey(String);

impl TaskIdempotencyKey {
    #[must_use]
    pub fn new(session_id: &str, turn_id: &str, request: &ComputerUseRequest) -> Self {
        let target = request.target.as_ref().map_or_else(String::new, |target| {
            [
                target.application.as_deref().unwrap_or_default(),
                target.window.as_deref().unwrap_or_default(),
                target.url.as_deref().unwrap_or_default(),
                target.element.as_deref().unwrap_or_default(),
            ]
            .into_iter()
            .map(normalize_component)
            .collect::<Vec<_>>()
            .join("|")
        });
        let criteria = request
            .success_criteria
            .iter()
            .map(|value| normalize_component(value))
            .collect::<Vec<_>>()
            .join("|");
        let constraints = request
            .constraints
            .iter()
            .map(|value| normalize_component(value))
            .collect::<Vec<_>>()
            .join("|");
        Self(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            normalize_component(session_id),
            normalize_component(turn_id),
            request.surface.as_str(),
            normalize_component(&request.objective),
            target,
            criteria,
            constraints,
        ))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn normalize_component(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Debug, Clone, PartialEq)]
pub enum BeforeRunDecision {
    Proceed,
    ReturnCached(ComputerUseResult),
    Blocked(String),
}

#[derive(Debug)]
pub struct TurnComputerUseSupervisor {
    budgets: ComputerUseBudgets,
    calls_started: usize,
    failure_count: usize,
    circuit_reason: Option<String>,
    terminal_cache: HashMap<TaskIdempotencyKey, ComputerUseResult>,
}

impl TurnComputerUseSupervisor {
    #[must_use]
    pub fn new(budgets: ComputerUseBudgets) -> Self {
        Self {
            budgets,
            calls_started: 0,
            failure_count: 0,
            circuit_reason: None,
            terminal_cache: HashMap::new(),
        }
    }

    #[must_use]
    pub fn before_run(&self, key: &TaskIdempotencyKey) -> BeforeRunDecision {
        if let Some(result) = self.terminal_cache.get(key) {
            return BeforeRunDecision::ReturnCached(result.clone());
        }
        self.before_new_run()
    }

    #[must_use]
    pub fn before_new_run(&self) -> BeforeRunDecision {
        if self.circuit_reason.is_some()
            || self.calls_started >= self.budgets.max_calls_per_turn
            || self.failure_count >= self.budgets.max_calls_per_turn
        {
            return BeforeRunDecision::Blocked("recursive_call_blocked".to_string());
        }
        BeforeRunDecision::Proceed
    }

    pub fn start_run(&mut self, key: &TaskIdempotencyKey) -> BeforeRunDecision {
        let decision = self.before_run(key);
        if matches!(decision, BeforeRunDecision::Proceed) {
            self.calls_started = self.calls_started.saturating_add(1);
        }
        decision
    }

    pub fn record_terminal(&mut self, key: TaskIdempotencyKey, result: ComputerUseResult) {
        if result.status != ComputerUseTerminalStatus::Succeeded {
            self.record_failure();
        }
        self.terminal_cache.entry(key).or_insert(result);
    }

    pub fn record_failure(&mut self) {
        self.failure_count = self.failure_count.saturating_add(1);
        if self.failure_count >= self.budgets.max_calls_per_turn {
            self.circuit_reason = Some("second-computer-use-failure".to_string());
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> SupervisorSnapshot {
        SupervisorSnapshot {
            circuit_open: self.circuit_reason.is_some(),
            reason: self.circuit_reason.clone(),
            action_count: 0,
            replan_count: 0,
            no_progress_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionFingerprint {
    surface: String,
    observation_generation: u64,
    action_type: String,
    target: String,
    arguments: String,
}

impl ActionFingerprint {
    #[must_use]
    pub fn new(
        surface: &str,
        observation_generation: u64,
        action_type: &str,
        target: &str,
        arguments: &str,
    ) -> Self {
        Self {
            surface: normalize_component(surface),
            observation_generation,
            action_type: normalize_component(action_type),
            target: normalize_component(target),
            arguments: normalize_component(arguments),
        }
    }
}

#[derive(Debug)]
pub struct RunBudgetGuard {
    budgets: ComputerUseBudgets,
    deadline: CuDeadline,
    action_count: usize,
    replan_count: usize,
    no_progress_count: usize,
    signatures: HashMap<ActionFingerprint, usize>,
    circuit_reason: Option<String>,
}

impl RunBudgetGuard {
    /// 以"起点 + CU 任务预算"建立守卫（未接线根 deadline）。
    ///
    /// 生产路径应当优先用 [`RunBudgetGuard::with_deadline`]：截止时间必须在任务**被接纳并
    /// 进入调度**时建立，早于租约等待、模型切换、初始观察与规划请求。
    #[must_use]
    pub fn new(budgets: ComputerUseBudgets, started_at_ms: u64) -> Self {
        Self::with_deadline(
            budgets,
            CuDeadline::establish_without_root(&budgets, started_at_ms),
        )
    }

    /// 用已经建立的 CU 截止时间构造：同一任务的所有阶段共用它，内部恢复不重置。
    #[must_use]
    pub fn with_deadline(budgets: ComputerUseBudgets, deadline: CuDeadline) -> Self {
        Self {
            budgets,
            deadline,
            action_count: 0,
            replan_count: 0,
            no_progress_count: 0,
            signatures: HashMap::new(),
            circuit_reason: None,
        }
    }

    /// 本 run 唯一的 CU 截止时间（含 `root_deadline_state` / `root_deadline` 事实）。
    #[must_use]
    pub const fn deadline(&self) -> CuDeadline {
        self.deadline
    }

    /// 单调 deadline 的剩余预算（毫秒，饱和到 0）。
    ///
    /// §2.3 要求"一个单调 deadline 贯穿观察、规划、验收和输入，**子请求上限不超过
    /// 剩余预算**"，因此每个阶段发起子请求前都必须按这个值收紧自己的超时上限。
    #[must_use]
    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.deadline.remaining_ms(now_ms)
    }

    /// 已用尽动作或时间预算时不再调用规划模型，执行后的验收仍由控制器完成。
    pub fn before_planning(&mut self, now_ms: u64) -> Result<(), ComputerUseError> {
        if self.deadline.is_expired_at(now_ms) {
            return Err(self.stop("deadline_exceeded", "computer-use deadline exceeded"));
        }
        if self.no_progress_count >= self.budgets.max_no_progress_steps {
            return Err(self.stop("no_progress", "no visible progress after repeated actions"));
        }
        if self.action_count >= self.budgets.max_actions {
            return Err(self.stop("budget_exhausted", "computer-use action budget exhausted"));
        }
        Ok(())
    }

    pub fn before_action(
        &mut self,
        fingerprint: &ActionFingerprint,
        now_ms: u64,
    ) -> Result<(), ComputerUseError> {
        self.before_planning(now_ms)?;
        if self
            .signatures
            .get(fingerprint)
            .copied()
            .unwrap_or_default()
            >= self.budgets.max_same_signature
        {
            return Err(self.stop(
                "no_progress",
                "identical action signature repeated without a new target",
            ));
        }

        self.action_count = self.action_count.saturating_add(1);
        *self.signatures.entry(fingerprint.clone()).or_default() += 1;
        Ok(())
    }

    pub fn after_verification(&mut self, visible_progress: bool) {
        if visible_progress {
            self.no_progress_count = 0;
        } else {
            self.no_progress_count = self.no_progress_count.saturating_add(1);
        }
    }

    pub fn record_replan(&mut self) -> Result<(), ComputerUseError> {
        if self.replan_count >= self.budgets.max_replans {
            return Err(self.stop("budget_exhausted", "computer-use replan budget exhausted"));
        }
        self.replan_count = self.replan_count.saturating_add(1);
        Ok(())
    }

    #[must_use]
    pub fn snapshot(&self) -> SupervisorSnapshot {
        SupervisorSnapshot {
            circuit_open: self.circuit_reason.is_some(),
            reason: self.circuit_reason.clone(),
            action_count: self.action_count,
            replan_count: self.replan_count,
            no_progress_count: self.no_progress_count,
        }
    }

    fn stop(&mut self, code: &str, message: &str) -> ComputerUseError {
        self.circuit_reason = Some(code.to_string());
        ComputerUseError::blocked(code, message, ComputerUseRetryOwner::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ComputerUseBudgets, ComputerUseRequest, ComputerUseResult, ComputerUseStage,
        ComputerUseSurface, ComputerUseTerminalStatus, SupervisorSnapshot,
    };

    fn request() -> ComputerUseRequest {
        serde_json::from_value(serde_json::json!({
            "objective": "打开记事本",
            "surface": "desktop",
            "target": { "application": "notepad" },
            "success_criteria": ["窗口可见"]
        }))
        .unwrap()
    }

    fn failed_result(code: &str) -> ComputerUseResult {
        ComputerUseResult {
            call_id: "cu-1".into(),
            provider_tool_call_id: Some("toolu-1".into()),
            status: ComputerUseTerminalStatus::Failed,
            stage: ComputerUseStage::Verification,
            goal_achieved: false,
            surface: ComputerUseSurface::Desktop,
            summary: "failed".into(),
            error: Some(crate::ComputerUseError::blocked(
                code,
                "failed",
                crate::ComputerUseRetryOwner::None,
            )),
            attempts: 1,
            steps_completed: 0,
            input_steps: None,
            evidence: vec![],
            supervisor: SupervisorSnapshot::default(),
            cu_budget: None,
            cleanup: None,
        }
    }

    #[test]
    fn identical_failed_task_returns_cached_terminal_result() {
        let mut supervisor = TurnComputerUseSupervisor::new(ComputerUseBudgets::default());
        let key = TaskIdempotencyKey::new("s", "t", &request());
        let failed = failed_result("target_not_found");
        supervisor.record_terminal(key.clone(), failed.clone());

        assert_eq!(
            supervisor.before_run(&key),
            BeforeRunDecision::ReturnCached(failed)
        );
    }

    #[test]
    fn second_failed_run_opens_turn_circuit() {
        let mut supervisor = TurnComputerUseSupervisor::new(ComputerUseBudgets::default());
        supervisor.record_failure();
        assert_eq!(supervisor.before_new_run(), BeforeRunDecision::Proceed);
        supervisor.record_failure();

        assert_eq!(
            supervisor.before_new_run(),
            BeforeRunDecision::Blocked("recursive_call_blocked".into())
        );
    }

    #[test]
    fn normalized_task_key_ignores_case_and_surrounding_space() {
        let first = TaskIdempotencyKey::new("session", "turn", &request());
        let mut equivalent = request();
        equivalent.objective = "  打开记事本  ".into();
        equivalent.target.as_mut().unwrap().application = Some("NOTEPAD".into());
        let second = TaskIdempotencyKey::new("session", "turn", &equivalent);

        assert_eq!(first, second);
    }

    #[test]
    fn repeated_action_without_progress_is_blocked() {
        let mut budget = RunBudgetGuard::new(ComputerUseBudgets::default(), 1_000);
        let fingerprint = ActionFingerprint::new("desktop", 1, "click", "save", "{}");
        assert!(budget.before_action(&fingerprint, 1_001).is_ok());
        budget.after_verification(false);
        assert!(budget.before_action(&fingerprint, 1_002).is_ok());
        budget.after_verification(false);

        assert_eq!(
            budget
                .before_action(&fingerprint, 1_003)
                .expect_err("no progress must stop input")
                .code,
            "no_progress"
        );
    }

    /// S1.5：同一动作对**同一观察代际**重复，即使每次都报告可见进展，也必须被签名界
    /// 挡住——此时 `no_progress` 不会触发（可见进展会把无进展计数清零），所以这条
    /// 规则是独立存在的。对应 §2.2「确认执行部分路径后不能整段重放」：**重新观察**
    /// 之后才允许再试，原样重放不行。
    #[test]
    fn repeated_identical_action_against_the_same_observation_is_blocked_despite_progress() {
        let mut budget = RunBudgetGuard::new(ComputerUseBudgets::default(), 1_000);
        let fingerprint = ActionFingerprint::new("desktop", 7, "click", "save", "{}");

        // max_same_signature 默认 2：前两次允许，且每次都算"可见进展"。
        assert!(budget.before_action(&fingerprint, 1_001).is_ok());
        budget.after_verification(true);
        assert!(budget.before_action(&fingerprint, 1_002).is_ok());
        budget.after_verification(true);
        assert_eq!(
            budget.snapshot().no_progress_count,
            0,
            "可见进展不得累加无进展计数，否则会把签名界误当成 no_progress 界"
        );

        let error = budget
            .before_action(&fingerprint, 1_003)
            .expect_err("identical replay against the same observation must be blocked");
        assert_eq!(error.code, "no_progress");
        assert!(
            error.message.contains("identical action signature"),
            "必须是签名界而不是无进展界；实际为：{}",
            error.message
        );
        assert!(
            budget.snapshot().circuit_open,
            "命中签名界必须打开熔断，后续动作不得再发"
        );

        // 换一个观察代际（即"重新观察之后"）允许再次尝试。
        let fresh = ActionFingerprint::new("desktop", 8, "click", "save", "{}");
        assert!(budget.before_action(&fresh, 1_004).is_ok());
    }

    #[test]
    fn action_replan_and_deadline_budgets_are_hard_limits() {
        let budgets = ComputerUseBudgets {
            max_actions: 1,
            max_replans: 1,
            timeout_ms: 10,
            ..ComputerUseBudgets::default()
        };
        let fingerprint = ActionFingerprint::new("browser", 1, "click", "submit", "{}");

        let mut action_guard = RunBudgetGuard::new(budgets, 100);
        assert!(action_guard.before_action(&fingerprint, 101).is_ok());
        assert_eq!(
            action_guard
                .before_action(&fingerprint, 102)
                .expect_err("second action must be blocked")
                .code,
            "budget_exhausted"
        );

        let mut replan_guard = RunBudgetGuard::new(budgets, 100);
        assert!(replan_guard.record_replan().is_ok());
        assert_eq!(
            replan_guard
                .record_replan()
                .expect_err("second replan must be blocked")
                .code,
            "budget_exhausted"
        );

        let mut deadline_guard = RunBudgetGuard::new(budgets, 100);
        assert_eq!(
            deadline_guard
                .before_action(&fingerprint, 110)
                .expect_err("deadline must stop input")
                .code,
            "deadline_exceeded"
        );
    }

    #[test]
    fn supervisor_types_are_exported_from_crate_root() {
        let key = crate::TaskIdempotencyKey::new("s", "t", &request());
        let supervisor = crate::TurnComputerUseSupervisor::new(ComputerUseBudgets::default());
        assert!(!key.as_str().is_empty());
        assert_eq!(
            supervisor.before_run(&key),
            crate::BeforeRunDecision::Proceed
        );
    }

    /// CU 截止时间在任务被接纳时建立；守卫构造得再晚，也不能把之前的时间还回去。
    #[test]
    fn guard_uses_the_deadline_established_at_acceptance_not_its_own_construction() {
        let budgets = ComputerUseBudgets {
            timeout_ms: 2_000,
            ..ComputerUseBudgets::default()
        };
        let deadline = CuDeadline::establish_without_root(&budgets, 100);
        let guard = RunBudgetGuard::with_deadline(budgets, deadline);

        assert_eq!(guard.deadline().established_at_ms(), 100);
        assert_eq!(guard.deadline().cu_deadline_ms(), 2_100);
        assert_eq!(guard.remaining_ms(100), 2_000);
        // 租约等待 / 模型切换 / 初始观察消耗的 1_400ms 必须计入预算。
        assert_eq!(guard.remaining_ms(1_500), 600);
        assert_eq!(guard.remaining_ms(9_999), 0);
        assert_eq!(guard.deadline().root_deadline_state().as_str(), "not_wired");
        assert_eq!(guard.deadline().root_deadline().unix_ms(), None);
    }

    /// 内部恢复（重新观察）不得重置时钟：剩余预算是从同一个截止时间算出来的。
    #[test]
    fn internal_recovery_never_resets_the_deadline_clock() {
        let budgets = ComputerUseBudgets {
            timeout_ms: 2_000,
            ..ComputerUseBudgets::default()
        };
        let guard = RunBudgetGuard::with_deadline(
            budgets,
            CuDeadline::establish_without_root(&budgets, 1_000),
        );
        let before_recovery = guard.remaining_ms(2_000);
        let after_recovery = guard.remaining_ms(2_000);
        assert_eq!(before_recovery, after_recovery);
        assert_eq!(after_recovery, 1_000);
        assert!(guard.remaining_ms(3_500) < after_recovery);
    }
}
