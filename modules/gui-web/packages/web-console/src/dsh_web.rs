//! DSH正式启用与模型入口，复用原权限审批、父轮事实与工具监督器。
use super::*;
use plugins::DshActivationTicket;
use runtime::dsh_host_process::CallContext;

#[cfg(test)]
#[path = "dsh_web_tests.rs"]
mod tests;

pub(super) fn model_bindings(
    root: &Path,
) -> Result<BTreeMap<String, dsh_execution::Binding>, String> {
    let manager = extension_market::manager(root).map_err(|_| "DSH工程配置无法读取")?;
    if !manager
        .list_installed_plugins()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|item| item.enabled && item.metadata.dsh.is_some())
    {
        return Ok(BTreeMap::new());
    }
    let verified = runtime::dsh_runtime::installed().map_err(|e| e.to_string())?;
    dsh_execution::bindings(&manager, &workspace_identity(root), &verified)
}

pub(super) fn capture_parent_bindings(workspace: &str) -> BTreeMap<String, dsh_execution::Binding> {
    let root = active_workspace_path();
    if workspace_identity(&root) != workspace {
        return BTreeMap::new();
    }
    model_bindings(&root).unwrap_or_else(|error| {
        diag_log(&format!("[DSH-TOOLS] 本轮未加载：{error}"));
        BTreeMap::new()
    })
}

#[derive(Clone, Debug)]
pub(super) struct FrozenPermission {
    profile: runtime::PermissionProfile,
    rules: Vec<ProtectedRule>,
    grants: BTreeMap<(String, String), SessionGrantView>,
}
pub(super) fn capture_permission(
    bindings: &BTreeMap<String, dsh_execution::Binding>,
    workspace: &str,
    session: Option<&str>,
    room: Option<&str>,
) -> FrozenPermission {
    if bindings.is_empty() {
        return FrozenPermission {
            profile: active_permission_profile(),
            rules: effective_protected_rules(),
            grants: BTreeMap::new(),
        };
    }
    let room_grant = room_permission_grant_view_for_path(&default_session_sqlite_path(), room);
    let grants = bindings
        .keys()
        .map(|name| {
            let grant = if room_grant.session_authorized {
                room_grant.clone()
            } else {
                session_grant_view_for(workspace, session, name)
            };
            ((session.unwrap_or_default().to_string(), name.clone()), grant)
        })
        .collect();
    FrozenPermission {
        profile: active_permission_profile(),
        rules: effective_protected_rules(),
        grants,
    }
}

/// 聊天接纳时为每个实际发送目标冻结授权；后续审批不重新读取配置扩权。
pub(super) fn capture_target_permissions(
    frozen: &mut FrozenPermission,
    bindings: &BTreeMap<String, dsh_execution::Binding>,
    workspace: &str,
    targets: &[AgentSessionDto],
    room: &str,
    db: &Path,
) {
    if bindings.is_empty() { return; }
    let room_grant = room_permission_grant_view_for_path(db, Some(room));
    frozen.grants.clear();
    for target in targets {
        for name in bindings.keys() {
            let grant = if room_grant.session_authorized { room_grant.clone() }
                else { session_tool_grant_view_for(workspace, Some(&target.id), name) };
            frozen.grants.insert((target.id.clone(), name.clone()), grant);
        }
    }
}

#[derive(Clone)]
pub(super) enum PendingAction {
    Enable {
        root: PathBuf,
        ticket: DshActivationTicket,
        session: String,
        room: String,
        config: JsonValue,
    },
    Model {
        root: PathBuf,
        binding: dsh_execution::Binding,
        parent: FrozenParentContext,
        session: String,
        room: String,
        turn: String,
        execution_id: String,
        provider_id: String,
        cancellation: Arc<ChatTurnCancellation>,
    },
}
impl std::fmt::Debug for PendingAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enable { ticket, .. } => {
                f.debug_tuple("DshEnable").field(&ticket.plugin_id).finish()
            }
            Self::Model { binding, .. } => f
                .debug_tuple("DshModel")
                .field(&binding.snapshot.plugin_id)
                .finish(),
        }
    }
}
impl PendingAction {
    pub(super) fn approval_session_id(&self) -> &str {
        match self {
            Self::Enable { session, .. } => session,
            Self::Model { session, .. } => session,
        }
    }
    fn root(&self) -> &Path {
        match self {
            Self::Enable { root, .. } | Self::Model { root, .. } => root,
        }
    }
    fn room(&self) -> &str {
        match self {
            Self::Enable { room, .. } | Self::Model { room, .. } => room,
        }
    }
    fn cancelled_or_expired(&self) -> bool {
        matches!(self, Self::Model { parent, cancellation, .. }
            if cancellation.is_requested() || parent.root_budget.as_ref().is_none_or(|budget| budget.is_expired()))
    }
    fn valid(&self) -> Result<(), String> {
        self.validate(true)
    }
    fn validate(&self, full_source: bool) -> Result<(), String> {
        if workspace_identity(&active_workspace_path()) != workspace_identity(self.root()) {
            return Err("DSH原工程已经切换".into());
        }
        match self {
            Self::Enable {
                root,
                ticket,
                session,
                room,
                ..
            } => {
                validate_active_context(session, room)?;
                let manager = extension_market::manager(root).map_err(|_| "DSH工程配置无法读取")?;
                let current = if full_source {
                    manager
                        .dsh_activation_ticket(&ticket.plugin_id, &ticket.workspace_id)
                        .map_err(|e| e.to_string())?
                        == *ticket
                } else {
                    manager
                        .dsh_ticket_lifecycle_current(ticket)
                        .map_err(|e| e.to_string())?
                };
                if !current {
                    return Err("DSH原启用请求的安装或停用世代已经变化".into());
                }
            }
            Self::Model {
                root,
                binding,
                parent,
                session,
                room,
                turn,
                execution_id,
                provider_id,
                cancellation,
            } => {
                if cancellation.is_requested()
                    || parent
                        .root_budget
                        .as_ref()
                        .is_none_or(|budget| budget.is_expired())
                    || parent.room_id.as_deref() != Some(room.as_str())
                    || parent.public_turn_id.as_deref() != Some(turn.as_str())
                    || parent.workspace_id.as_str() != binding.snapshot.workspace_id
                    || execution_id.is_empty()
                    || provider_id.is_empty()
                    || provider_id.len() > 192
                    || !(parent.host_model_snapshots.get(session)
                        .is_some_and(|host| host.agent_session_id() == session)
                        || parent.session_backend_model.as_ref().is_some_and(|agent|
                            agent.id == *session && agent_session_backend::AgentSessionBackend::for_provider(&agent.provider)
                                == agent_session_backend::AgentSessionBackend::DevinAcp))
                    || parent
                        .goal_phase
                        .as_ref()
                        .is_some_and(|goal| goal.validate_live().is_err())
                {
                    return Err("DSH父轮身份、时限或取消资格已经失效".into());
                }
                let db = parent
                    .runtime_db_path
                    .as_ref()
                    .ok_or("DSH没有父轮事实数据库")?;
                validate_frozen_parent_relations(db, parent).map_err(|e| e.reason())?;
                if parent.session_backend_model.as_ref().is_some_and(|agent| agent.id == *session) {
                    let connection = open_session_connection(db).map_err(|e| e.to_string())?;
                    if !chat_run_admission::target_is_admitted_on(&connection,
                        parent.parent_run_id.as_deref().ok_or("DSH缺少真实父运行")?, session)
                        .map_err(|e| e.to_string())? {
                        return Err("DSH模型不是本轮接纳的发送目标".into());
                    }
                }
                let manager = extension_market::manager(root).map_err(|_| "DSH工程配置无法读取")?;
                if !(if full_source {
                    dsh_execution::still_current(&manager, binding)
                } else {
                    dsh_execution::lifecycle_current(&manager, binding)
                }) {
                    return Err("DSH原工具快照已失效（停用、配置更新、卸载或重装），未执行".into());
                }
                if !parent
                    .dsh_bindings
                    .get(&binding.definition.name)
                    .is_some_and(|frozen| {
                        frozen.snapshot == binding.snapshot
                            && frozen.root == binding.root
                            && frozen.raw_name == binding.raw_name
                            && frozen.definition.input_schema == binding.definition.input_schema
                    })
                {
                    return Err("DSH工具未在父轮接纳时冻结".into());
                }
            }
        }
        Ok(())
    }
}

fn validate_active_context(session: &str, room: &str) -> Result<(), String> {
    if session.trim().is_empty() || room.trim().is_empty() {
        return Err("DSH启用需要当前会话和聊天室".into());
    }
    let store = session_store().lock().map_err(|_| "DSH会话上下文不可读")?;
    if store.active_session_id().as_deref() != Some(session)
        || store.active_chat_room_id().as_deref() != Some(room)
        || !store.state.sessions.iter().any(|item| item.id == session)
        || !store.state.chat_rooms.iter().any(|item| item.id == room)
    {
        return Err("DSH启用会话或聊天室已经切换".into());
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnableRequest {
    expected_workspace: String,
    id: String,
    expected_source_sha256: String,
    session_id: String,
    chat_room_id: String,
    #[serde(default = "empty_config")]
    config: JsonValue,
}
fn empty_config() -> JsonValue {
    json!({})
}

pub(super) async fn enable(Json(request): Json<EnableRequest>) -> ApiResult<Json<JsonValue>> {
    let pin = Arc::new(
        workspace_activity::pin_workspace().map_err(|e| api_error(StatusCode::CONFLICT, &e))?,
    );
    let root = active_workspace_path();
    if workspace_identity(&root) != request.expected_workspace {
        return Err(api_error(StatusCode::CONFLICT, "DSH启用工程已经切换"));
    }
    validate_active_context(&request.session_id, &request.chat_room_id)
        .map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
    if !request.config.is_object()
        || serde_json::to_vec(&request.config).map_or(true, |bytes| bytes.len() > 64 * 1024)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "DSH启用配置必须是有界对象",
        ));
    }
    let manager = extension_market::manager(&root)?;
    let ticket = manager
        .dsh_activation_ticket(&request.id, &request.expected_workspace)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e.to_string()))?;
    if ticket
        .package
        .source_fingerprint()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e.to_string()))?
        != request.expected_source_sha256
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "DSH已安装来源与页面确认不一致，请刷新",
        ));
    }
    let action = PendingAction::Enable {
        root: root.clone(),
        ticket,
        session: request.session_id.clone(),
        room: request.chat_room_id.clone(),
        config: request.config,
    };
    let invoke = ToolInvoke {
        call_id: format!(
            "dsh-enable-{}",
            random_hex_identifier(16, "dsh enable")
                .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e))?
        ),
        tool_name: format!("dsh__enable_{}", &request.expected_source_sha256[..32]),
        input: json!({"plugin_id":request.id,"source_sha256":request.expected_source_sha256}),
        caller: ToolCaller::WebUi,
        workspace_id: request.expected_workspace,
        session_id: Some(request.session_id),
        user_authorized: false,
        user_confirmed_twice: false,
    };
    let outcome = run_action(action.clone(), invoke.clone(), pin).await;
    finish_action(&action, &invoke, &outcome, true);
    Ok(Json(
        json!({"enabled":outcome.status == ToolOutcomeStatus::Ok,
        "pending_call_id":outcome.permission_gate.decision.requires_ui().then_some(&invoke.call_id), "outcome":outcome}),
    ))
}

pub(super) async fn call_from_model(
    name: &str,
    input: &JsonValue,
    session: Option<&str>,
    room: Option<&str>,
    execution_id: Option<&str>,
    provider_id: Option<&str>,
    turn: Option<&str>,
    parent: Option<&FrozenParentContext>,
    settlement: &mut tool_dispatch_settlement::ToolDispatchSettlement,
) -> ApiResult<ToolOutcome> {
    let (
        Some(parent),
        Some(session),
        Some(room),
        Some(execution_id),
        Some(provider_id),
        Some(turn),
    ) = (parent, session, room, execution_id, provider_id, turn)
    else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "DSH模型调用缺少真实父轮/provider调用身份",
        ));
    };
    let binding = parent
        .dsh_bindings
        .get(name)
        .cloned()
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, "DSH工具未在本轮接纳时冻结，未执行"))?;
    let cancellation = tool_turn_cancellation_registry()
        .lock()
        .ok()
        .and_then(|items| items.get(turn).cloned())
        .or_else(|| CHAT_CANCELLATION.try_with(Arc::clone).ok())
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "DSH缺少父轮取消令牌"))?;
    let pin = Arc::new(
        workspace_activity::pin_workspace().map_err(|e| api_error(StatusCode::CONFLICT, &e))?,
    );
    let action = PendingAction::Model {
        root: active_workspace_path(),
        binding,
        parent: parent.clone(),
        session: session.into(),
        room: room.into(),
        turn: turn.into(),
        execution_id: execution_id.into(),
        provider_id: provider_id.into(),
        cancellation,
    };
    if let Err(mut reason) = action.valid() {
        // 调用已接纳，但尚未启动executor；记录固定字段，不落参数或动态错误正文。
        if let (Some(db), Some(run)) = (parent.runtime_db_path.as_deref(), parent.parent_run_id.as_deref()) {
            if append_runtime_run_event(db, run, "tool.dispatch_rejected", json!({
                "stage":"after_admission_before_executor","reason_code":"dsh_action_not_live",
                "executed":false,"tool_name":name,"tool_call_id":execution_id
            })).is_err() {
                reason.push_str("（拒绝审计保存失败，仍未执行。）");
            }
        }
        return Err(api_error(StatusCode::CONFLICT, &reason));
    }
    let invoke = ToolInvoke {
        call_id: execution_id.into(),
        tool_name: name.into(),
        input: input.clone(),
        caller: ToolCaller::Llm,
        workspace_id: parent.workspace_id.as_str().into(),
        session_id: Some(session.into()),
        user_authorized: false,
        user_confirmed_twice: false,
    };
    let mut outcome = run_action(action.clone(), invoke.clone(), pin).await;
    let acp = devin_acp::bridge::current().is_some();
    if acp && outcome.status == ToolOutcomeStatus::DryRunOnly
        && outcome.permission_gate.decision.requires_ui() {
        // ACP没有跨回合审批续接协议；不留下回合结束后执行、结果无法回传的请求。
        // 必须同时返回终态拒绝；仅改文案仍会被外层记成 awaiting_approval 并锁住远端绑定。
        outcome.status = ToolOutcomeStatus::Rejected;
        outcome.summary_text = "本轮插件未执行：请先完成聊天室授权再重新发起；未保留延期执行请求。".into();
    }
    if outcome.status == ToolOutcomeStatus::DryRunOnly
        && outcome.permission_gate.decision.requires_ui() && !acp
    {
        settlement
            .handoff_approval()
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e))?;
    }
    finish_action(&action, &invoke, &outcome, !acp);
    Ok(outcome)
}

async fn run_action(
    action: PendingAction,
    invoke: ToolInvoke,
    pin: Arc<workspace_activity::WorkspacePin>,
) -> ToolOutcome {
    if let Err(e) = action.valid() {
        return runtime_tool_failed_outcome(&invoke, e);
    }
    let timeout = match &action {
        PendingAction::Model { parent, .. } => parent
            .root_budget
            .as_ref()
            .map_or(0, |budget| budget.limit_ms(30_000)),
        _ => 30_000,
    };
    if timeout == 0 {
        return runtime_tool_failed_outcome(&invoke, "DSH父轮执行预算已经到期".into());
    }
    let frozen_gate = if let PendingAction::Model { parent, .. } = &action {
        let Some(grant) = parent
            .dsh_permission
            .grants
            .get(&(invoke.session_id.clone().unwrap_or_default(), invoke.tool_name.clone()))
            .cloned()
        else { return runtime_tool_failed_outcome(&invoke, "DSH未冻结实际发送对象的权限，未执行".into()); };
        let gate = evaluate_permission(
            &invoke,
            PermissionMode::DangerFullAccess,
            &[],
            action.root(),
            &parent.dsh_permission.rules,
            &grant,
            parent.dsh_permission.profile,
        );
        if !gate.decision.is_allowed() {
            return if gate.decision.requires_ui() {
                ToolOutcome::dry_run_only(&invoke, gate, "DSH等待本次父轮权限确认")
            } else {
                ToolOutcome::rejected(&invoke, gate, "DSH父轮冻结权限未放行")
            };
        }
        Some(gate)
    } else {
        None
    };
    let root = action.root().to_path_buf();
    let room = action.room().to_string();
    let executor = Arc::new(Executor {
        action,
        name: invoke.tool_name.clone(),
        _pin: pin,
    });
    let mut outcome = runtime_tool_supervision::execute_with_executor(
        invoke,
        root,
        timeout,
        Some(room),
        Some(executor),
    )
    .await;
    if let Some(gate) = frozen_gate {
        outcome.evidence = Some(json!({"frozen_parent_permission":gate}));
    }
    outcome
}
fn finish_action(
    action: &PendingAction,
    invoke: &ToolInvoke,
    outcome: &ToolOutcome,
    enqueue: bool,
) {
    if enqueue
        && outcome.status == ToolOutcomeStatus::DryRunOnly
        && outcome.permission_gate.decision.requires_ui()
    {
        enqueue_pending_approval_for_room_with_dsh(
            &invoke.call_id,
            invoke,
            &outcome.permission_gate,
            Some(action.room()),
            action.clone(),
        );
    }
    append_tool_audit_record(invoke, outcome);
    emit_pet_event_for_tool_outcome(&invoke.tool_name, outcome);
}

pub(super) async fn execute_approved(
    record: &PendingApprovalRecord,
    confirmed_twice: bool,
) -> ToolOutcome {
    let Some(action) = record.dsh_action.clone() else {
        return runtime_tool_failed_outcome(&record.invoke, "DSH审批动作缺失".into());
    };
    if record.workspace_id != workspace_identity(action.root())
        || record.session_id.as_deref() != Some(action.approval_session_id())
        || record.chat_room_id.as_deref() != Some(action.room())
    {
        return runtime_tool_failed_outcome(&record.invoke, "DSH审批归属不一致".into());
    }
    // 模型的等待审批不是执行成功。原调用只能被领取一次，重放同一审批不能再次启动 Node。
    let mut claim = match ApprovalClaim::claim(&action, &record.invoke) {
        Ok(claim) => claim,
        Err(error) => return runtime_tool_failed_outcome(&record.invoke, error),
    };
    if let Err(e) = action.valid() {
        let mut outcome = runtime_tool_failed_outcome(&record.invoke, e);
        settle_approval(&mut claim, &mut outcome);
        return outcome;
    }
    let pin = match workspace_activity::pin_workspace() {
        Ok(pin) => Arc::new(pin),
        Err(e) => {
            let mut outcome = runtime_tool_failed_outcome(&record.invoke, e);
            settle_approval(&mut claim, &mut outcome);
            return outcome;
        }
    };
    let mut invoke = record.invoke.clone();
    invoke.user_authorized = true;
    invoke.user_confirmed_twice = confirmed_twice;
    let mut outcome = run_action(action, invoke, pin).await;
    settle_approval(&mut claim, &mut outcome);
    outcome
}

struct ApprovalClaim {
    db: PathBuf,
    execution_id: String,
    parent_run_id: String,
    tool: String,
    digest: String,
    settled: bool,
}
impl ApprovalClaim {
    fn claim(action: &PendingAction, invoke: &ToolInvoke) -> Result<Option<Self>, String> {
        let PendingAction::Model {
            parent,
            execution_id,
            provider_id,
            ..
        } = action
        else {
            return Ok(None);
        };
        if invoke.call_id != *execution_id {
            return Err("DSH审批原调用标识不一致".into());
        }
        let db = parent
            .runtime_db_path
            .clone()
            .ok_or("DSH审批缺少原事实库")?;
        let parent_run_id = parent.parent_run_id.clone().ok_or("DSH审批缺少原父运行")?;
        let digest = tool_arguments_digest(&invoke.input);
        let connection = open_session_connection(&db).map_err(|e| e.to_string())?;
        let changed = connection.execute(
            "UPDATE tool_calls SET status='approval_running',updated_at_unix_ms=?1 WHERE tool_call_id=?2 AND run_id=?3 AND tool_name=?4 AND arguments_digest=?5 AND provider_tool_call_id=?6 AND status='awaiting_approval'",
            params![unix_timestamp_millis() as i64, execution_id, parent_run_id, invoke.tool_name, digest, provider_id],
        ).map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("DSH原模型调用没有待审批执行资格，已拒绝重放".into());
        }
        Ok(Some(Self {
            db,
            execution_id: execution_id.clone(),
            parent_run_id,
            tool: invoke.tool_name.clone(),
            digest,
            settled: false,
        }))
    }
    fn finish(&mut self, status: &str) -> Result<(), String> {
        let connection = open_session_connection(&self.db).map_err(|e| e.to_string())?;
        let changed = connection.execute(
            "UPDATE tool_calls SET status=?1,updated_at_unix_ms=?2 WHERE tool_call_id=?3 AND run_id=?4 AND tool_name=?5 AND arguments_digest=?6 AND status='approval_running'",
            params![status, unix_timestamp_millis() as i64, self.execution_id, self.parent_run_id, self.tool, self.digest],
        ).map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("DSH审批终态归属或状态已经变化，未覆盖原记录".into());
        }
        self.settled = true;
        Ok(())
    }
}
impl Drop for ApprovalClaim {
    fn drop(&mut self) {
        if !self.settled {
            if let Err(error) = self.finish("cancelled_outcome_unknown") {
                diag_log(&format!("[DSH-APPROVAL] 审批执行收尾未确认：{error}"));
            }
        }
    }
}
fn settle_approval(claim: &mut Option<ApprovalClaim>, outcome: &mut ToolOutcome) {
    if let Some(claim) = claim {
        let status = match outcome.status {
            ToolOutcomeStatus::Ok => "completed",
            ToolOutcomeStatus::Timeout => "timed_out_outcome_unknown",
            _ => "failed",
        };
        if let Err(error) = claim.finish(status) {
            outcome.status = ToolOutcomeStatus::Failed;
            outcome.summary_text =
                format!("DSH操作已结束等待，但终态未确认：{error}；不可自动重试");
            outcome.output["settlement_confirmed"] = json!(false);
        }
    }
}

struct Executor {
    action: PendingAction,
    name: String,
    _pin: Arc<workspace_activity::WorkspacePin>,
}
impl ToolInvocationExecutor for Executor {
    fn handles(&self, name: &str) -> bool {
        name == self.name
    }
    fn execute(&self, invoke: &ToolInvoke) -> ToolOutcome {
        let started = Instant::now();
        let mut interruption = None;
        let mut cleanup_confirmed = None;
        let action = self.action.clone();
        let immediate = self.action.clone();
        let revoked = dsh_execution::revocation_probe(
            Arc::new(move || immediate.cancelled_or_expired()),
            Arc::new(move || action.validate(false).is_err()),
        );
        let result = (|| -> Result<JsonValue, String> {
            self.action.valid()?;
            if invoke.workspace_id != workspace_identity(self.action.root()) {
                return Err("DSH调用的工程归属变化".into());
            }
            let control = runtime::managed_process::current_execution_control()
                .ok_or("DSH执行缺少原监督控制")?
                .with_additional_cancel(revoked.clone());
            runtime::managed_process::with_execution_control(control.clone(), || {
                let verified = runtime::dsh_runtime::installed().map_err(|e| e.to_string())?;
                let mut manager = extension_market::manager(self.action.root())
                    .map_err(|_| "DSH工程配置无法读取")?;
                match &self.action {
                    PendingAction::Enable {
                        ticket,
                        room,
                        config,
                        ..
                    } => {
                        let context = CallContext {
                            workspace_id: invoke.workspace_id.clone(),
                            room_id: room.clone(),
                            run_id: invoke.call_id.clone(),
                            call_id: invoke.call_id.clone(),
                        };
                        let snapshot = dsh_activation_host::enable_ticket_verified(
                            &mut manager,
                            ticket,
                            &context,
                            config,
                            &verified,
                        )?;
                        Ok(
                            json!({"enabled":true,"plugin_id":snapshot.plugin_id,"activation_id":snapshot.activation_id}),
                        )
                    }
                    PendingAction::Model {
                        binding,
                        parent,
                        room,
                        execution_id,
                        provider_id,
                        ..
                    } => {
                        if execution_id != &invoke.call_id {
                            return Err("DSH原宿主调用ID已变化".into());
                        }
                        let context = CallContext {
                            workspace_id: parent.workspace_id.as_str().into(),
                            room_id: room.clone(),
                            run_id: parent.parent_run_id.clone().ok_or("DSH没有真实父运行")?,
                            call_id: provider_id.clone(),
                        };
                        let result = match dsh_execution::execute(
                            Arc::new(manager),
                            binding,
                            &verified,
                            &context,
                            &invoke.input,
                            revoked.clone(),
                        ) {
                            Ok(result) => result,
                            Err(error) => {
                                interruption = error.interruption;
                                cleanup_confirmed = error.cleanup_confirmed;
                                return Err(error.to_string());
                            }
                        };
                        Ok(
                            json!({"result":result,"provider_tool_call_id":provider_id,"execution_id":execution_id,
                            "activation_id":binding.snapshot.activation_id,"cleanup_confirmed":true}),
                        )
                    }
                }
            })
        })();
        let mut outcome = match result {
            Ok(output) => ToolOutcome {
                call_id: invoke.call_id.clone(),
                tool_name: invoke.tool_name.clone(),
                status: if output["result"]["isError"] == true {
                    ToolOutcomeStatus::Failed
                } else {
                    ToolOutcomeStatus::Ok
                },
                output,
                summary_text: "DSH实际宿主操作已收尾".into(),
                elapsed_ms: 0,
                permission_gate: runtime::PermissionGateReport::deny(
                    PermissionMode::DangerFullAccess,
                    "由原运行时闸门回填",
                ),
                evidence: None,
            },
            Err(error) => {
                let mut failed = runtime_tool_failed_outcome(invoke, error);
                failed.output["code"] = json!(match interruption {
                    Some(runtime::managed_process::Interruption::TimedOut) => "timed_out",
                    Some(runtime::managed_process::Interruption::Cancelled) => "cancelled",
                    None => "execution_failed",
                });
                failed.output["cleanup_confirmed"] = json!(cleanup_confirmed);
                if interruption == Some(runtime::managed_process::Interruption::TimedOut) {
                    failed.status = ToolOutcomeStatus::Timeout;
                }
                failed
            }
        };
        if runtime::managed_process::current_execution_control().and_then(|c| c.interruption())
            == Some(runtime::managed_process::Interruption::TimedOut)
        {
            outcome.status = ToolOutcomeStatus::Timeout;
        }
        outcome.elapsed_ms = elapsed_millis(started);
        outcome
    }
}
