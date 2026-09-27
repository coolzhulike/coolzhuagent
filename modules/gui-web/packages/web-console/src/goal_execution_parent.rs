//! Goal 阶段的冻结父关系。只能由真实运行/claim 联合查询建立，不能由模型或当前房间推断。
use std::path::{Path, PathBuf};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use crate::root_execution_budget::RootExecutionBudget;

#[derive(Clone, Debug)]
pub(crate) struct FrozenGoalPhaseParent {
    db_path: PathBuf,
    pub(crate) workspace_id: String,
    pub(crate) goal_id: String,
    pub(crate) phase_id: String,
    pub(crate) phase_run_id: String,
    claim_token: String,
    pub(crate) session_id: String,
    pub(crate) room_id: String,
    root_budget: RootExecutionBudget,
}

impl FrozenGoalPhaseParent {
    pub(crate) fn capture(db_path: &Path, workspace_id: &str, goal_id: &str, phase_id: &str,
        phase_run_id: &str, claim_token: &str, session_id: &str, room_id: &str) -> Result<Self, String> {
        crate::canonical_workspace_identity(workspace_id).map_err(|error| error.code().to_string())?;
        for value in [goal_id, phase_id, phase_run_id, claim_token, session_id, room_id] {
            if value.trim().is_empty() { return Err("Goal 阶段执行身份不完整".into()); }
        }
        let connection = read_connection(db_path)?;
        let row = read_phase(&connection, workspace_id, goal_id, phase_id, phase_run_id, claim_token, session_id, room_id)?;
        Ok(Self { db_path: db_path.to_owned(), workspace_id: workspace_id.into(), goal_id: goal_id.into(),
            phase_id: phase_id.into(), phase_run_id: phase_run_id.into(), claim_token: claim_token.into(),
            session_id: session_id.into(), room_id: room_id.into(),
            root_budget: RootExecutionBudget::from_started_at(row.0, row.1) })
    }

    pub(crate) fn db_path(&self) -> &Path { &self.db_path }
    pub(crate) fn root_budget(&self) -> RootExecutionBudget { self.root_budget.clone() }

    pub(crate) fn action_context(&self, cu_run_id: &str) -> runtime::GoalPhaseActionContext {
        runtime::GoalPhaseActionContext { hierarchy: runtime::RunIdentityScope::StepAction,
            workspace_id: self.workspace_id.clone(), goal_id: self.goal_id.clone(), phase_id: self.phase_id.clone(),
            phase_run_id: self.phase_run_id.clone(), claim_token: self.claim_token.clone(),
            room_id: Some(self.room_id.clone()), session_id: Some(self.session_id.clone()),
            initiating_chat_turn: None, run_id: cu_run_id.into() }
    }

    pub(crate) fn action_identity(&self, cu_run_id: &str, step_id: &str, attempt: &str, action_id: &str, tool_id: &str) -> runtime::RunIdentity {
        runtime::RunIdentity { workspace_id: self.workspace_id.clone(), room_id: self.room_id.clone(),
            session_id: self.session_id.clone(), public_turn_id: String::new(), run_id: cu_run_id.into(),
            step_id: Some(step_id.into()), request_attempt_id: Some(attempt.into()), tool_call_id: Some(tool_id.into()),
            action_id: Some(action_id.into()), owner_epoch: None, parent_run: None,
            scope: Some(runtime::RunIdentityScope::StepAction), schema_version: Some(runtime::RUN_IDENTITY_SCHEMA_VERSION) }
    }

    /// 每次受理动作前复核同一个真实 claim；stop/retry/换阶段不会继承输入资格。
    pub(crate) fn validate_live(&self) -> Result<(), String> {
        if self.root_budget.is_expired() { return Err(crate::root_execution_budget::EXPIRED_REASON.into()); }
        let connection = read_connection(&self.db_path)?;
        read_phase(&connection, &self.workspace_id, &self.goal_id, &self.phase_id,
            &self.phase_run_id, &self.claim_token, &self.session_id, &self.room_id).map(|_| ())
    }
}

fn read_connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("Goal 父运行库无法只读核验：{error}"))?;
    connection.busy_timeout(std::time::Duration::from_millis(500)).map_err(|error| error.to_string())?;
    Ok(connection)
}

fn read_phase(connection: &Connection, workspace: &str, goal: &str, phase: &str,
    run: &str, claim: &str, session: &str, room: &str) -> Result<(u64, u64), String> {
    let row: Option<(i64, i64)> = connection.query_row(
        "SELECT r.started_at, COALESCE(c.task_timeout_ms, 600000)
         FROM runtime_runs r
         JOIN goal_phases p ON p.goal_id=r.goal_id AND p.id=r.phase_id
         JOIN goals g ON g.id=p.goal_id AND g.workspace_id=r.workspace_id
         LEFT JOIN goal_role_configs c ON c.session_id=r.session_id
         WHERE r.id=?1 AND r.kind='goal_phase' AND r.state='running'
           AND r.workspace_id=?2 AND r.goal_id=?3 AND r.phase_id=?4
           AND r.claim_token=?5 AND r.session_id=?6 AND r.chat_room_id=?7
           AND r.started_at IS NOT NULL AND r.finished_at IS NULL
           AND p.status='running' AND p.active_run_id=r.id AND p.claim_token=r.claim_token
           AND p.claim_owner=r.owner_id AND g.status='running' AND g.chat_room_id=r.chat_room_id",
        params![run, workspace, goal, phase, claim, session, room], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional().map_err(|error| format!("Goal 阶段父关系查询失败：{error}"))?;
    let (started, timeout) = row.ok_or("Goal 阶段父关系已失效或与真实运行/claim 不符，未获得输入资格")?;
    if started <= 0 || !(60_000..=86_400_000).contains(&timeout) { return Err("Goal 阶段开始时间或任务时限无效".into()); }
    Ok((started as u64, timeout as u64))
}

/// 查询均发生在宿主短 lease 之外；completed 必须保留原 helper 的真实退出事实。
pub(crate) struct GoalNativeAuthorization<'a> {
    pub(crate) parent: &'a FrozenGoalPhaseParent,
    pub(crate) inner: &'a dyn computer_use::prepared_input::NativeInputAuthorization,
}
impl computer_use::prepared_input::NativeInputAuthorization for GoalNativeAuthorization<'_> {
    fn authorize(&self, prepared: &computer_use::prepared_input::PreparedNativeInput)
        -> Result<computer_use::prepared_input::NativeInputPermit, String> {
        self.parent.validate_live()?;
        self.inner.authorize(prepared)
    }
    fn dispatch(&self, prepared: &computer_use::prepared_input::PreparedNativeInput,
        permit: &computer_use::prepared_input::NativeInputPermit,
        notify: &mut dyn FnMut() -> Result<(), String>) -> Result<(), String> {
        self.parent.validate_live()?;
        self.inner.dispatch(prepared, permit, notify)
    }
    fn completed(&self, completion: &computer_use::prepared_input::NativeInputCompletion) -> Result<(), String> {
        self.inner.completed(completion)
    }
}
