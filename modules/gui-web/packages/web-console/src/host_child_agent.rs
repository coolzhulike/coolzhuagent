//! Agent 工具在 Web 宿主中的同步子执行。所有身份和能力均来自父工具调用快照。

use super::*;
use std::collections::BTreeSet;

pub(super) struct AgentGateExecutor;

impl ToolInvocationExecutor for AgentGateExecutor {
    fn handles(&self, name: &str) -> bool { name == "Agent" }

    fn execute(&self, invoke: &ToolInvoke) -> ToolOutcome {
        ToolOutcome {
            call_id: invoke.call_id.clone(),
            tool_name: invoke.tool_name.clone(),
            status: ToolOutcomeStatus::Ok,
            output: JsonValue::Null,
            summary_text: String::from("Agent 宿主执行资格已通过"),
            elapsed_ms: 0,
            permission_gate: runtime::PermissionGateReport::deny(
                PermissionMode::DangerFullAccess, "等待宿主权限闸门回填",
            ),
            evidence: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct FrozenModelRequestConfig {
    pub max_tokens: u32,
    pub context_window: u32,
    pub tools_enabled: bool,
    pub reasoning_mode: Option<String>,
}

impl FrozenModelRequestConfig {
    fn capture(agent: &AgentSessionDto) -> Self {
        let limit = effective_model_limit_for_agent(agent);
        Self {
            max_tokens: request_max_tokens_for_limit(limit),
            context_window: limit.0,
            tools_enabled: llm_tools_enabled_for_session(&agent.id),
            reasoning_mode: session_model_settings_for(&agent.id).reasoning_mode,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedModel {
    agent: AgentSessionDto,
    pub(super) client: ProviderClient,
    pub(super) request: FrozenModelRequestConfig,
}

#[derive(Debug, Clone)]
pub(super) struct HostModelSnapshot {
    pub(super) parent_model: ResolvedModel,
    override_models: Vec<ResolvedModel>,
    parent_tools: BTreeSet<String>,
    pub(super) parent_definitions: Vec<ToolDefinition>,
    pub(super) permission: PermissionMode,
    pub(super) workspace_root: PathBuf,
    usage_db_path: PathBuf,
    /// 只继承已授权父房间的历史，不能通过选择模型读取其它聊天室。
    context_history: Vec<PersistedChatMessage>,
}

impl HostModelSnapshot {
    pub(super) fn agent_session_id(&self) -> &str { &self.parent_model.agent.id }
    /// 冻结授权是执行上限，后续会话设置不能扩大本次运行的工具集合。
    pub(super) fn permits_tool(&self, name: &str) -> bool {
        let required = required_permission_for_tool(name);
        self.parent_tools.contains(name) && !required.is_unspecified() && required <= self.permission
    }
    pub(super) fn permits_parent_agent(&self) -> bool {
        self.parent_tools.contains("Agent")
            && required_permission_for_tool("Agent") <= self.permission
    }
    pub(super) fn capture(
        agent: &AgentSessionDto,
        room_id: Option<&str>,
        parent: &FrozenParentContext,
    ) -> Result<Self, String> {
        let workspace_root = active_workspace_path();
        if workspace_identity(&workspace_root) != parent.workspace_id.as_str() {
            return Err(String::from("父执行工作区已变化，Agent 未执行"));
        }
        let parent_model = ResolvedModel {
            agent: agent.clone(),
            client: provider_client_for_agent(agent).map_err(|error| error.to_string())?,
            request: FrozenModelRequestConfig::capture(agent),
        };
        let context_history = {
            let path = parent.runtime_db_path.clone().unwrap_or_else(default_session_sqlite_path);
            let connection = open_session_connection(&path).map_err(|error|error.to_string())?;
            match room_id {
                Some(room) => load_chat_room_messages(&connection,room).map_err(|error|error.to_string())?,
                None => Vec::new(),
            }
        };
        let override_models = default_agent_sessions().into_iter()
            .filter(|candidate| candidate.selectable && candidate.enabled && !candidate.system
                && candidate.id != agent.id)
            .filter_map(|candidate| {
                let client = provider_client_for_agent(&candidate).ok()?;
                let request = FrozenModelRequestConfig::capture(&candidate);
                Some(ResolvedModel { agent: candidate, client, request })
            })
            .collect();
        let mut parent_definitions = llm_tool_definitions_for_session(&agent.id, room_id)
            .unwrap_or_default();
        retain_admitted_plugin_definitions(&mut parent_definitions, Some(parent));
        // 与定义生成使用同一显式授权范围；真正执行仍复核授权存活及当前策略。
        let mut permission = if scheduled_execution::current().is_some_and(|grant|
            grant.authorizes(parent.workspace_id.as_str(), Some(&agent.id), room_id)) {
            PermissionMode::DangerFullAccess
        } else {
            llm_tool_permission_for_room(room_id)
        };
        // 重规划仅检查事实和生成修订步骤，不能继承执行房间或调度授权的写能力。
        if let Some(goal) = parent.goal_phase.as_ref() {
            if goal.is_replanning()? {
                permission = permission.min(PermissionMode::ReadOnly);
                parent_definitions.retain(|definition| {
                    let required = required_permission_for_tool(&definition.name);
                    !required.is_unspecified() && required <= permission
                });
            }
        }
        let parent_tools = parent_definitions.iter().map(|definition| definition.name.clone()).collect();
        Ok(Self {
            parent_model, override_models, parent_tools, parent_definitions,
            permission, workspace_root,
            usage_db_path: parent.runtime_db_path.clone().unwrap_or_else(default_session_sqlite_path),
            context_history,
        })
    }

    fn choose_model(&self, requested: Option<&str>) -> Result<ResolvedModel, String> {
        let Some(requested) = requested else { return Ok(self.parent_model.clone()); };
        let candidates = std::iter::once(&self.parent_model)
            .chain(self.override_models.iter()).collect::<Vec<_>>();
        if let Some(model) = candidates.iter().find(|candidate| candidate.agent.id == requested) {
            return Ok((**model).clone());
        }
        let matches = candidates.into_iter()
            .filter(|candidate| candidate.agent.model == requested)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [model] => Ok((*model).clone()),
            [] => Err(format!("Agent 模型覆盖 `{requested}` 不在父运行已解析的可用配置中")),
            _ => Err(format!("Agent 模型覆盖 `{requested}` 对应多个配置，请使用会话 ID")),
        }
    }

    fn child_tools(&self, requested: &BTreeSet<String>) -> BTreeSet<String> {
        requested.intersection(&self.parent_tools)
            .filter(|name| name.as_str() != "Agent" && name.as_str() != "SendUserMessage")
            .filter(|name| {
                let required = required_permission_for_tool(name);
                !required.is_unspecified() && required <= self.permission
            })
            .cloned().collect()
    }
}

#[derive(Clone)]
pub(super) struct HostChildScope {
    pub parent: FrozenParentContext,
    pub parent_call_id: String,
    pub parent_turn_id: String,
    pub tool_session_id: String,
    pub usage_session_id: String,
    pub room_id: String,
    pub allowed_tools: BTreeSet<String>,
    pub definitions: Vec<ToolDefinition>,
    pub permission: PermissionMode,
    pub workspace_root: PathBuf,
    pub usage_db_path: PathBuf,
    pub client: ProviderClient,
    pub request: FrozenModelRequestConfig,
    pub cancellation: Arc<ChatTurnCancellation>,
}

impl HostChildScope {
    pub(super) fn validate_live(&self) -> Result<(), String> {
        if self.cancellation.is_requested() {
            return Err(String::from("父运行已取消，子 Agent 未执行"));
        }
        if self.parent.root_budget.as_ref().is_none_or(|budget| budget.is_expired()) {
            return Err(String::from("父运行已超时，子 Agent 未执行"));
        }
        if let Some(goal) = self.parent.goal_phase.as_ref() {
            goal.validate_live()?;
        }
        Ok(())
    }

    pub(super) fn permits(&self, name: &str) -> bool {
        self.allowed_tools.contains(name)
            && required_permission_for_tool(name) <= self.permission
            && self.validate_live().is_ok()
    }

    pub(super) fn definitions(&self) -> Option<Vec<ToolDefinition>> {
        let definitions = self.definitions.clone();
        (!definitions.is_empty()).then_some(definitions)
    }
}

#[derive(Clone)]
pub(super) enum HostToolScope {
    Parent(Arc<HostModelSnapshot>),
    Child(Arc<HostChildScope>),
}

/// 把已保存且接纳时实际解析成功的模型会话作为Agent工具参数目录。
/// 仅修改本轮已经开放的Agent定义，不能新增工具或提升子执行权限。
pub(super) fn bind_session_tool_catalog(request: &mut MessageRequest,
    parent: Option<&FrozenParentContext>, agent_id: &str) {
    let Some(snapshot)=parent.and_then(|p|p.host_model_snapshots.get(agent_id)) else {return;};
    let Some(definitions)=request.tools.as_mut() else {return;};
    let Some(tool)=definitions.iter_mut().find(|tool|tool.name=="Agent") else {return;};
    let sessions=std::iter::once(&snapshot.parent_model).chain(snapshot.override_models.iter())
        .map(|model|model.agent.id.clone()).collect::<Vec<_>>();
    tool.description=Some("通过已接纳的模型会话执行同步子任务，复用原配置、凭据和父房间上下文；返回持久executionId与真实结果。model请使用本工具列出的会话ID。只能使用父权限允许的工具，不能递归委派。".into());
    tool.input_schema["properties"]["model"]=json!({"type":"string","enum":sessions,
        "description":"接纳时已解析的模型会话ID；省略时使用当前模型会话"});
}

pub(super) struct WebHostAgentRunner {
    pub snapshot: Arc<HostModelSnapshot>,
    pub parent: FrozenParentContext,
    pub parent_call_id: String,
    pub parent_turn_id: String,
    pub caller_agent_session_id: String,
    pub cancellation: Arc<ChatTurnCancellation>,
}

impl tools::HostAgentRunner for WebHostAgentRunner {
    fn run<'a>(&'a self, request: tools::HostAgentRequest)
        -> std::pin::Pin<Box<dyn Future<Output = Result<JsonValue, String>> + Send + 'a>> {
        Box::pin(async move {
            if self.snapshot.agent_session_id() != self.caller_agent_session_id {
                return Err(String::from("父运行 Agent 会话快照不匹配，未执行"));
            }
            let usage_session_id = self.parent.session_id.clone()
                .unwrap_or_else(|| self.caller_agent_session_id.clone());
            let room_id = self.parent.room_id.clone()
                .ok_or_else(|| String::from("父运行缺少聊天室身份，Agent 未执行"))?;
            let parent_run_id = self.parent.parent_run_id.clone()
                .ok_or_else(|| String::from("父运行缺少 run 身份，Agent 未执行"))?;
            let budget = self.parent.root_budget.clone()
                .ok_or_else(|| String::from("父运行缺少根预算，Agent 未执行"))?;
            if self.parent_call_id.is_empty() || self.parent_turn_id.is_empty() {
                return Err(String::from("父运行缺少工具调用或 turn 身份，Agent 未执行"));
            }
            if self.cancellation.is_requested() || budget.is_expired() {
                return Err(String::from("父运行已取消或超时，Agent 未执行"));
            }
            if let Some(goal) = self.parent.goal_phase.as_ref() {
                goal.validate_live()?;
            }
            let model = self.snapshot.choose_model(request.model.as_deref())?;
            crate::execution_service::require_capability(&model.agent.provider,
                crate::execution_service::Source::SessionTool)?;
            let child_cancel = Arc::new(ChatTurnCancellation::new());
            let allowed_tools = self.snapshot.child_tools(&request.allowed_tools);
            let definitions = self.snapshot.parent_definitions.iter()
                .filter(|definition| allowed_tools.contains(&definition.name)).cloned().collect();
            let child = Arc::new(HostChildScope {
                parent: self.parent.clone(),
                parent_call_id: self.parent_call_id.clone(),
                parent_turn_id: self.parent_turn_id.clone(),
                tool_session_id: self.caller_agent_session_id.clone(),
                usage_session_id: usage_session_id.clone(),
                room_id: room_id.clone(),
                allowed_tools,
                definitions,
                permission: self.snapshot.permission,
                workspace_root: self.snapshot.workspace_root.clone(),
                usage_db_path: self.snapshot.usage_db_path.clone(),
                client: model.client.clone(),
                request: model.request.clone(),
                cancellation: Arc::clone(&child_cancel),
            });
            // 宿主工具调用ID已含真实run和来源请求；重复委派不能换随机ID绕过领取。
            let child_id = format!("child-execution:{}", self.parent_call_id);
            let prompt = format!("子任务：{}\n\n{}", request.description, request.prompt);
            let spec = crate::execution_service::ExecutionSpec {
                id: child_id.clone(), workspace_id: self.parent.workspace_id.as_str().into(),
                room_id: room_id.clone(), session_id: model.agent.id.clone(),
                provider: model.agent.provider.clone(), model: model.agent.model.clone(),
                source: crate::execution_service::Source::SessionTool,
                source_id: self.parent_call_id.clone(), parent_run_id: Some(parent_run_id.clone()),
                parent_execution_id: None, call_id: Some(self.parent_call_id.clone()),
                summary: request.description.clone(), deadline_ms: budget.deadline_unix_ms(),
            };
            crate::execution_service::TurnRunner::run(&self.snapshot.usage_db_path, spec,
                Arc::clone(&child_cancel), Box::pin(async {
            let execution = TURN_TRACE.scope(self.parent_turn_id.clone(),
                Box::pin(call_agent_model_with_tool_loop(
                    &model.agent, &prompt, &[], &self.snapshot.context_history, None, Some(&room_id),
                    Some(&self.parent), Some(Arc::clone(&child)),
                )));
            let execution = root_execution_budget::scope(budget.clone(),
                budget.run_cancellable(
                    await_chat_turn(child_cancel.as_ref(), execution),
                    || child_cancel.request_timeout(),
                ));
            let execution = crate::execution_service::TurnRunner::child(
                Arc::clone(&self.cancellation), Arc::clone(&child_cancel), budget.clone(), async {
            let result = if let Some(goal) = self.parent.goal_phase.clone() {
                tokio::select! {
                    result = execution => result,
                    error = wait_until_goal_phase_invalid(goal) => {
                        child_cancel.request();
                        return Err(error);
                    }
                }
            } else {
                execution.await
            }
                .map_err(|_| String::from("父运行已超时，子 Agent 已停止"))?
                .map_err(|_| String::from("父运行已取消，子 Agent 已停止"))?
                .map_err(|error| error.to_string())?;
            Ok(result)
            });
            let result = execution.await?;
            if child_cancel.is_requested() || self.cancellation.is_requested()
                || self.parent.root_budget.as_ref().is_some_and(|budget| budget.is_expired()) {
                return Err(String::from("父运行取消或超时，子 Agent 结果不再提交"));
            }
            if let Some(goal) = self.parent.goal_phase.as_ref() {
                goal.validate_live()?;
            }
            if result.execution_failed {
                return Err(format!("子 Agent 执行失败：{}", result.answer_text));
            }
            Ok(json!({
                "agentId": child_id,
                "name": request.name,
                "description": request.description,
                "subagentType": request.subagent_type,
                "model": model.agent.model,
                "status": "completed",
                "result": result.answer_text,
                "parentRunId": parent_run_id,
                "parentToolCallId": self.parent_call_id,
                "workspaceId": self.parent.workspace_id.as_str(),
                "sessionId": usage_session_id,
                "agentSessionId": self.caller_agent_session_id,
                "roomId": room_id,
                "turnId": self.parent_turn_id,
                "executionId": child_id,
            }))
            }), |value| value["result"].as_str().unwrap_or("").to_string()).await
        })
    }
}

/// 仅在当前已接纳的 Goal 子请求存活期间监视原 claim；future 结束即停止监视。
async fn wait_until_goal_phase_invalid(goal: goal_execution_parent::FrozenGoalPhaseParent) -> String {
    loop {
        if let Err(error) = goal.validate_live() {
            return error;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{config_test_guard, context_test_agent, DevOpenPermissionsTestGuard};

    /// 即使用例 panic，也恢复全局配置，避免后续用例访问已删除的临时目录。
    struct HostTestEnvironment {
        workspace: std::path::PathBuf,
        config: crate::WorkspaceConfig,
    }
    impl HostTestEnvironment {
        fn install(workspace: &Path, config: crate::WorkspaceConfig) -> Self {
            let previous_workspace = std::mem::replace(&mut crate::workspace_state().lock().unwrap().current, workspace.to_path_buf());
            let previous_config = std::mem::replace(&mut *crate::workspace_config().lock().unwrap(), config);
            Self { workspace: previous_workspace, config: previous_config }
        }
    }
    impl Drop for HostTestEnvironment {
        fn drop(&mut self) {
            *crate::workspace_config().lock().unwrap_or_else(std::sync::PoisonError::into_inner) = self.config.clone();
            crate::workspace_state().lock().unwrap_or_else(std::sync::PoisonError::into_inner).current = self.workspace.clone();
        }
    }

    #[test]
    fn goal_phase_pin_blocks_workspace_change_and_rejects_stale_workspace() {
        let _guard = config_test_guard();
        let active = tempfile::tempdir().unwrap();
        let stale = tempfile::tempdir().unwrap();
        let previous = {
            let mut state = crate::workspace_state().lock().unwrap();
            std::mem::replace(&mut state.current, active.path().to_path_buf())
        };
        let active_id = crate::workspace_identity(active.path());
        let pin = crate::pin_goal_phase_workspace(&active_id).unwrap();
        assert!(crate::workspace_activity::begin_workspace_change().is_err(),
            "已接纳 Goal 阶段期间不得切换工程");
        assert!(crate::pin_goal_phase_workspace(&crate::workspace_identity(stale.path())).is_err(),
            "阶段开始前若工程身份已不符，不得使用旧配置执行");
        drop(pin);
        crate::workspace_state().lock().unwrap().current = previous;
    }

    fn seed_goal_parent(db: &Path, workspace_id: &str, session_id: &str, room_id: &str)
        -> goal_execution_parent::FrozenGoalPhaseParent {
        let connection = crate::open_session_connection(db).unwrap();
        crate::initialize_session_schema(&connection).unwrap();
        let now = crate::unix_timestamp_millis() as i64;
        connection.execute(
            "INSERT INTO goals(id,workspace_id,chat_room_id,title,status,max_iterations,current_iteration,background,completion_condition_json,created_at,updated_at)
             VALUES ('host-goal',?1,?2,'宿主测试','running',3,1,0,'{}',?3,?3)",
            rusqlite::params![workspace_id, room_id, now],
        ).unwrap();
        connection.execute(
            "INSERT INTO goal_phases(id,goal_id,title,assigned_role,status,depends_on_json,skills_json,output_artifacts_json,created_at,updated_at,active_run_id,claim_token,claim_owner)
             VALUES ('host-phase','host-goal','子执行','implementer','running','[]','[]','[]',?1,?1,'host-goal-run','host-claim','host-owner')",
            rusqlite::params![now],
        ).unwrap();
        connection.execute(
            "INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,goal_id,phase_id,state,owner_id,claim_token,created_at,started_at)
             VALUES ('host-goal-run','goal_phase',?1,?2,?3,'host-goal','host-phase','running','host-owner','host-claim',?4,?4)",
            rusqlite::params![workspace_id, session_id, room_id, now],
        ).unwrap();
        let pin = crate::goal_stage_pin::StagePin { original_role: "implementer".into(),
            route_hint: None, retry_count: 0, last_verdict: None, target_session_id: session_id.into() };
        connection.execute(
            "INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES ('host-goal-run','run.accepted',?1,?2)",
            rusqlite::params![serde_json::json!({"goal_phase_pin":pin}).to_string(), now],
        ).unwrap();
        goal_execution_parent::FrozenGoalPhaseParent::capture(db, workspace_id,
            "host-goal", "host-phase", "host-goal-run", "host-claim", session_id, room_id).unwrap()
    }

    #[tokio::test]
    async fn goal_child_stops_on_phase_stop_and_rejects_lost_claim_before_tool_dispatch() {
        let _guard = config_test_guard();
        let _dev_open = DevOpenPermissionsTestGuard::enable();
        let temp = tempfile::tempdir().unwrap();
        let mut config = crate::WorkspaceConfig::default();
        config.model.enable_llm_tools = true;
        config.model.llm_tool_exposure = Some("all".to_string());
        let _environment = HostTestEnvironment::install(temp.path(), config);
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let router = axum::Router::new().route("/v1/chat/completions", axum::routing::post({
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            move || {
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                async move {
                    started.notify_one();
                    release.notified().await;
                    axum::Json(serde_json::json!({
                        "id":"goal-child-model", "object":"chat.completion", "model":"local-goal-child",
                        "choices":[{"index":0,"message":{"role":"assistant","content":"不应交付"},"finish_reason":"stop"}],
                        "usage":{"prompt_tokens":7,"completion_tokens":4,"total_tokens":11}
                    }))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        let mut agent = context_test_agent();
        agent.id = String::from("host-goal-agent");
        agent.provider = String::from("Custom");
        agent.model = String::from("local-goal-child");
        agent.base_url = Some(format!("http://{address}/v1"));
        agent.memory_beads.clear();
        let workspace_id = crate::workspace_identity(temp.path());
        let db = temp.path().join("goal-host.sqlite3");
        let goal = seed_goal_parent(&db, &workspace_id, &agent.id, "host-goal-room");
        let mut parent = crate::FrozenParentContext::new("goal-phase", &workspace_id,
            Some("host-goal-room"), Some(&agent.id), None, Some("host-goal-run")).unwrap();
        parent.root_budget = Some(goal.root_budget());
        parent.goal_phase = Some(goal);
        parent.runtime_db_path = Some(db.clone());
        let snapshot = Arc::new(HostModelSnapshot::capture(&agent, Some("host-goal-room"), &parent).unwrap());
        assert!(snapshot.permits_parent_agent());
        let allowed = snapshot.child_tools(&BTreeSet::from([String::from("write_file")]));
        assert!(allowed.contains("write_file"), "测试须证实写工具原本可用");
        parent.host_model_snapshots = Arc::new(std::collections::HashMap::from([(
            agent.id.clone(), Arc::clone(&snapshot),
        )]));
        let cancellation = Arc::new(ChatTurnCancellation::new());
        let child_scope = Arc::new(HostChildScope {
            parent: parent.clone(), parent_call_id: String::from("goal-tool-call"),
            parent_turn_id: String::from("goal-turn"), tool_session_id: agent.id.clone(),
            usage_session_id: agent.id.clone(), room_id: String::from("host-goal-room"),
            allowed_tools: allowed, definitions: snapshot.parent_definitions.clone(),
            permission: snapshot.permission, workspace_root: temp.path().to_path_buf(),
            usage_db_path: db.clone(), client: snapshot.parent_model.client.clone(),
            request: snapshot.parent_model.request.clone(), cancellation: Arc::clone(&cancellation),
        });
        assert!(child_scope.permits("write_file"));
        let runner = WebHostAgentRunner {
            snapshot, parent: parent.clone(), parent_call_id: String::from("goal-tool-call"),
            parent_turn_id: String::from("goal-turn"), caller_agent_session_id: agent.id.clone(),
            cancellation,
        };
        let inflight = tokio::spawn(async move {
            tools::execute_agent_with_host(&serde_json::json!({
                "description":"Goal 在途停用", "prompt":"等待模型返回", "subagent_type":"general-purpose",
                "allowed_tools":["write_file"]
            }), Some(&runner)).await
        });
        // 首次 SQLite 初始化和 HTTP 调度在繁忙 CI 上也计入等待；取消响应仍单独限时。
        tokio::time::timeout(std::time::Duration::from_secs(30), started.notified())
            .await.expect("真实子模型请求应到达本地端点");
        {
            let connection = crate::open_session_connection(&db).unwrap();
            connection.execute("UPDATE runtime_runs SET state='stop_requested',stop_requested_at=?1 WHERE id='host-goal-run'",
                rusqlite::params![crate::unix_timestamp_millis() as i64]).unwrap();
        }
        let stopped = tokio::time::timeout(std::time::Duration::from_secs(3), inflight)
            .await.expect("Goal 停用须中断在途子模型").unwrap().unwrap_err();
        assert!(stopped.contains("失效") || stopped.contains("取消"), "{stopped}");
        let linked_usage: i64 = crate::open_session_connection(&db).unwrap().query_row(
            "SELECT COUNT(*) FROM chat_usage_events WHERE request_kind='child_agent' AND run_id='host-goal-run'",
            [], |row| row.get(0)).unwrap();
        assert_eq!(linked_usage, 1, "Goal 子请求即使取消，也须指向原持久 run");
        assert!(!child_scope.permits("write_file"));
        release.notify_waiters();
        {
            let connection = crate::open_session_connection(&db).unwrap();
            connection.execute("UPDATE runtime_runs SET state='running',claim_token='other-claim',stop_requested_at=NULL WHERE id='host-goal-run'", []).unwrap();
            connection.execute("UPDATE goal_phases SET claim_token='other-claim' WHERE goal_id='host-goal' AND id='host-phase'", []).unwrap();
        }
        assert!(!child_scope.permits("write_file"), "旧 claim 不得因运行再次可见而恢复资格");
        let denied_path = temp.path().join("goal-claim-lost-must-not-write.txt");
        let denied = crate::tool_invocation_identity::scope(String::from("goal-turn/claim-lost"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "write_file", &serde_json::json!({"path":denied_path,"content":"forbidden"}),
                Some(&agent.id), Some("goal-tool-use"), Some("goal-turn"), Some("host-goal-room"),
                Some(&parent), Some(&HostToolScope::Child(child_scope)),
            )).await;
        assert!(denied.is_err(), "失去原 claim 后实际工具派发必须拒绝");
        assert!(!denied_path.exists());
        // 独立有效运行只改变路由，避免旧 claim 或已取消子作用域造成假阴性。
        let drift_db = temp.path().join("goal-stage-drift.sqlite3");
        let drift_goal = seed_goal_parent(&drift_db, &workspace_id, &agent.id, "host-goal-room");
        assert!(drift_goal.validate_live().is_ok());
        let mut drift_parent = parent.clone();
        drift_parent.root_budget = Some(drift_goal.root_budget());
        drift_parent.goal_phase = Some(drift_goal);
        drift_parent.runtime_db_path = Some(drift_db.clone());
        assert!(drift_parent.host_model_snapshots.get(&agent.id).unwrap().permits_tool("write_file"));
        crate::open_session_connection(&drift_db).unwrap().execute(
            "UPDATE goal_phases SET route_hint='planner' WHERE goal_id='host-goal' AND id='host-phase'", [],
        ).unwrap();
        let drift_path = temp.path().join("goal-stage-drift-must-not-write.txt");
        let drift_denied = crate::tool_invocation_identity::scope(String::from("goal-turn/stage-drift"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "write_file", &serde_json::json!({"path":drift_path,"content":"forbidden"}),
                Some(&agent.id), Some("goal-stage-drift-use"), Some("goal-turn"), Some("host-goal-room"),
                Some(&drift_parent), None,
            )).await;
        assert!(drift_denied.is_err(), "同一会话阶段路由漂移后实际写工具必须拒绝");
        assert!(!drift_path.exists());
        // 新接纳的规划阶段即使房间允许写，也只能看到并执行只读工具。
        let planning_db = temp.path().join("goal-planning-permissions.sqlite3");
        let _initial = seed_goal_parent(&planning_db, &workspace_id, &agent.id, "host-goal-room");
        let connection = crate::open_session_connection(&planning_db).unwrap();
        connection.execute("UPDATE goal_phases SET route_hint='planner',retry_count=1,last_verdict='fail'",[]).unwrap();
        let planning_pin = crate::goal_stage_pin::StagePin { original_role:"implementer".into(),
            route_hint:Some("planner".into()), retry_count:1, last_verdict:Some("fail".into()),
            target_session_id:agent.id.clone() };
        connection.execute("UPDATE runtime_run_events SET payload_json=?1 WHERE event_type='run.accepted'",
            rusqlite::params![serde_json::json!({"goal_phase_pin":planning_pin}).to_string()]).unwrap();
        let planning_goal = goal_execution_parent::FrozenGoalPhaseParent::capture(&planning_db,
            &workspace_id,"host-goal","host-phase","host-goal-run","host-claim",&agent.id,"host-goal-room").unwrap();
        assert!(planning_goal.is_replanning().unwrap());
        let mut planning_parent = parent.clone();
        planning_parent.root_budget = Some(planning_goal.root_budget());
        planning_parent.goal_phase = Some(planning_goal);
        planning_parent.runtime_db_path = Some(planning_db);
        let planning_snapshot = HostModelSnapshot::capture(&agent,Some("host-goal-room"),&planning_parent).unwrap();
        assert!(planning_snapshot.permits_tool("read_file"),"规划阶段应能检查事实");
        assert!(!planning_snapshot.permits_tool("write_file"));
        assert!(!planning_snapshot.permits_parent_agent());
        assert!(planning_snapshot.parent_definitions.iter().all(|definition| {
            let required = required_permission_for_tool(&definition.name);
            !required.is_unspecified() && required <= PermissionMode::ReadOnly
        }));
        planning_parent.host_model_snapshots = Arc::new(std::collections::HashMap::from([
            (agent.id.clone(),Arc::new(planning_snapshot))]));
        let planning_path = temp.path().join("planning-must-not-write.txt");
        let planning_denied = crate::tool_invocation_identity::scope(String::from("goal-turn/planning-write"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "write_file", &serde_json::json!({"path":planning_path,"content":"forbidden"}),
                Some(&agent.id),Some("planning-write-use"),Some("goal-turn"),Some("host-goal-room"),
                Some(&planning_parent),None,
            )).await;
        assert!(planning_denied.is_err());
        assert!(!planning_path.exists());
        server.abort();
    }

    #[tokio::test]
    async fn shared_http_loop_and_stream_seed_preserve_history_and_dispatch_each_response_once() {
        let _guard = config_test_guard();
        let _dev_open = DevOpenPermissionsTestGuard::enable();
        let temp = tempfile::tempdir().unwrap();
        let mut config = crate::WorkspaceConfig::default();
        config.model.enable_llm_tools = true;
        config.model.llm_tool_exposure = Some("all".into());
        let _environment = HostTestEnvironment::install(temp.path(), config);
        let first_path = temp.path().join("first.txt");
        let second_path = temp.path().join("second.txt");
        std::fs::write(&first_path, "FIRST_PRIVATE_TEST_RESULT").unwrap();
        std::fs::write(&second_path, "SECOND_PRIVATE_TEST_RESULT").unwrap();
        for (seeded, parent_execution) in [(false, false), (true, false), (false, true), (true, true)] {
            let bodies = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let captured = Arc::clone(&bodies);
            let first = first_path.clone(); let second = second_path.clone();
            let router = axum::Router::new().route("/v1/chat/completions", axum::routing::post(
                move |axum::Json(body): axum::Json<serde_json::Value>| {
                    let captured = Arc::clone(&captured); let first = first.clone(); let second = second.clone();
                    async move {
                        let round = { let mut bodies = captured.lock().unwrap(); bodies.push(body); bodies.len() + usize::from(seeded) };
                        let message = if round <= 2 {
                            serde_json::json!({"role":"assistant","content":null,"tool_calls":[{
                                "id":"same-provider-tool-id","type":"function","function":{"name":"read_file",
                                "arguments":serde_json::json!({"path":if round == 1 {first} else {second}}).to_string()}}]})
                        } else { serde_json::json!({"role":"assistant","content":"两文件读取完成"}) };
                        axum::Json(serde_json::json!({"id":format!("shared-response-{round}"),"object":"chat.completion","model":"local-shared",
                            "choices":[{"index":0,"message":message,"finish_reason":if round <= 2 {"tool_calls"} else {"stop"}}],
                            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}))
                    }
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
            let mut agent = context_test_agent();
            agent.id = format!("shared-loop-{seeded}-{parent_execution}"); agent.provider = "Custom".into();
            agent.model = "local-shared".into(); agent.base_url = Some(format!("http://{address}/v1")); agent.memory_beads.clear();
            let db = temp.path().join(format!("shared-loop-{seeded}-{parent_execution}.sqlite3"));
            let workspace = crate::workspace_identity(temp.path());
            let turn = format!("shared-turn-{seeded}"); let run = format!("shared-run-{seeded}");
            crate::create_chat_runtime_run_sqlite(&db, &run, "shared-claim", &workspace, Some(&agent.id), "shared-room", &turn).unwrap();
            crate::start_chat_runtime_run_sqlite(&db, &run, "shared-claim").unwrap();
            let mut parent = crate::FrozenParentContext::new("shared-loop-test", &workspace, Some("shared-room"), Some(&agent.id), Some(&turn), Some(&run)).unwrap();
            parent.root_budget = Some(crate::root_execution_budget::RootExecutionBudget::establish(60_000)); parent.runtime_db_path = Some(db.clone());
            let snapshot = HostModelSnapshot::capture(&agent, Some("shared-room"), &parent).unwrap();
            let child = Arc::new(HostChildScope { parent: parent.clone(), parent_call_id: "shared-parent-call".into(), parent_turn_id: turn.clone(),
                tool_session_id: agent.id.clone(), usage_session_id: agent.id.clone(), room_id: "shared-room".into(),
                allowed_tools: BTreeSet::from(["read_file".into()]),
                definitions: snapshot.parent_definitions.iter().filter(|tool| tool.name == "read_file").cloned().collect(),
                permission: snapshot.permission, workspace_root: temp.path().to_path_buf(), usage_db_path: db.clone(),
                client: snapshot.parent_model.client.clone(), request: snapshot.parent_model.request.clone(), cancellation: Arc::new(ChatTurnCancellation::new()) });
            assert!(child.permits("read_file"));
            parent.host_model_snapshots = Arc::new(std::collections::HashMap::from([(agent.id.clone(), Arc::new(snapshot))]));
            let prompt = "请读取两个文件，并根据真实工具结果回答";
            let mut assembly = crate::build_context_assembly_with_roster(&agent, &[], prompt, &[],
                crate::context_build_options_for_agent_with_floor_and_room(&agent, None, Some("shared-room")), None);
            assembly.messages.insert(0, crate::InputMessage::user_text("历史上下文哨兵，不得丢失")); assembly.turn_id = Some(turn.clone());
            // 捕获后会话端点发生变化，父执行与子执行都必须继续使用原模型快照。
            agent.base_url = Some("http://127.0.0.1:1/v1".into());
            let initial = seeded.then(|| (api::MessageResponse { id: None, kind: "message".into(), role: "assistant".into(),
                content: vec![crate::OutputContentBlock::ToolUse { id: "same-provider-tool-id".into(), name: "read_file".into(), input: serde_json::json!({"path":first_path}) }],
                model: agent.model.clone(), stop_reason: Some("tool_use".into()), stop_sequence: None,
                usage: api::Usage { input_tokens: 0, output_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 }, request_id: None }, false));
            let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
            let response = crate::TURN_TRACE.scope(turn.clone(), Box::pin(crate::run_shared_model_tool_loop(&agent, prompt,
                assembly, Some("shared-room"), Some(&parent), if parent_execution {None} else {Some(child)}, turn.clone(), initial,
                Some(crate::SharedModelTurnObserver { sender, message_turn_key: "shared-user-message".into(), chat_turn_id: turn.clone(), assistant_message_id: "shared-test-assistant".into() })))).await.unwrap();
            assert_eq!(response.answer_text, "两文件读取完成"); assert!(response.tool_requests.is_empty()); assert!(response.model_tool_calls_executed);
            let bodies = bodies.lock().unwrap(); assert_eq!(bodies.len(), if seeded {2} else {3});
            assert!(bodies.iter().all(|body| body.to_string().contains("历史上下文哨兵")));
            assert!(bodies.last().unwrap().to_string().contains("FIRST_PRIVATE_TEST_RESULT"));
            assert!(bodies.last().unwrap().to_string().contains("SECOND_PRIVATE_TEST_RESULT"));
            let mut results = Vec::new();
            while let Ok(event) = events.try_recv() { if let crate::SharedModelTurnEvent::Message(message) = event { if message.kind == "tool-result" {results.push(message);} } }
            assert_eq!(results.len(), 2); assert_ne!(results[0].id, results[1].id, "重复provider ID必须按真实响应身份分开");
            let connection = rusqlite::Connection::open(&db).unwrap();
            if !parent_execution {
                let attempts: i64 = connection.query_row("SELECT count(*) FROM chat_usage_events WHERE call_id='shared-parent-call'", [], |row| row.get(0)).unwrap();
                assert_eq!(attempts, if seeded {2} else {3}, "流式首响应不得被再次请求或伪造账目");
            }
            drop(bodies);
            if parent_execution {
                let recovered = crate::call_agent_model_text_recovery(&agent, "冻结恢复测试", &[], prompt,
                    "未开放的工具", Some("shared-room"), None, Some(&run),
                    parent.host_model_snapshots.get(&agent.id).map(Arc::as_ref)).await.unwrap();
                assert_eq!(crate::answer_text(&recovered.content), "两文件读取完成");
            }
            server.abort();
        }
    }

    #[tokio::test]
    async fn host_agent_dispatch_uses_two_targets_and_records_each_child_request_once() {
        let _guard = config_test_guard();
        let _dev_open = DevOpenPermissionsTestGuard::enable();
        let temp = tempfile::tempdir().unwrap();
        let mut config = crate::WorkspaceConfig::default();
        config.model.enable_llm_tools = true;
        config.model.llm_tool_exposure = Some("all".to_string());
        let _environment = HostTestEnvironment::install(temp.path(), config);
        let slow_request_started = Arc::new(tokio::sync::Notify::new());
        let release_slow_request = Arc::new(tokio::sync::Notify::new());
        let router = axum::Router::new().route("/v1/chat/completions", axum::routing::post(
            {
                let started = Arc::clone(&slow_request_started);
                let release = Arc::clone(&release_slow_request);
                move |axum::Json(body): axum::Json<serde_json::Value>| {
                    let started = Arc::clone(&started);
                    let release = Arc::clone(&release);
                    async move {
                        if body.to_string().contains("慢速取消测试") {
                            started.notify_one();
                            release.notified().await;
                        }
                axum::Json(serde_json::json!({
                    "id":"child-host-test", "object":"chat.completion", "model":"local-child",
                    "choices":[{"index":0,"message":{"role":"assistant","content":"子任务真实完成"},"finish_reason":"stop"}],
                    "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
                }))
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        let mut agents = (1..=2).map(|index| {
            let mut agent = context_test_agent();
            agent.id = format!("host-child-fixture-{index}");
            agent.provider = "Custom".into();
            agent.model = "local-child".into();
            agent.base_url = Some(format!("http://{address}/v1"));
            agent.memory_beads.clear();
            agent
        }).collect::<Vec<_>>();
        let workspace_id = crate::workspace_identity(temp.path());
        let db = temp.path().join("host-agent.sqlite3");
        let run_id = "host-run-test";
        let turn_id = "host-turn-test";
        let room_id = "host-room-test";
        let conversation_id = "host-conversation-session";
        crate::create_chat_runtime_run_sqlite(&db, run_id, "host-claim", &workspace_id,
            Some(conversation_id), room_id, turn_id).unwrap();
        crate::start_chat_runtime_run_sqlite(&db, run_id, "host-claim").unwrap();
        let budget = crate::root_execution_budget::RootExecutionBudget::establish(60_000);
        let mut parent = crate::FrozenParentContext::new("host-test", &workspace_id,
            Some(room_id), Some(conversation_id), Some(turn_id), Some(run_id)).unwrap();
        parent.root_budget = Some(budget);
        parent.runtime_db_path = Some(db.clone());
        let snapshots = agents.iter().map(|agent| {
            let snapshot = HostModelSnapshot::capture(agent, Some(room_id), &parent).unwrap();
            (agent.id.clone(), Arc::new(snapshot))
        }).collect::<std::collections::HashMap<_,_>>();
        parent.host_model_snapshots = Arc::new(snapshots.clone());
        let cancellation = std::sync::Arc::new(crate::ChatTurnCancellation::new());
        let _cancellation_scope = crate::ToolTurnCancellationScope::install(turn_id, Arc::clone(&cancellation));
        let input = serde_json::json!({"description":"汇报测试", "prompt":"请直接给出结果", "subagent_type":"Explore", "allowed_tools":[]});
        for (index, agent) in agents.drain(..).enumerate() {
            let snapshot = snapshots.get(&agent.id).unwrap().clone();
            let result = crate::tool_invocation_identity::scope(format!("{turn_id}/response-{index}"),
                crate::run_model_tool_dispatch_for_session_with_identity(
                    "Agent", &input, Some(&agent.id), Some("toolu-child"), Some(turn_id),
                    Some(room_id), Some(&parent),
                    Some(&HostToolScope::Parent(snapshot)),
                )).await.unwrap();
            assert_eq!(result.status, "ok", "{}", result.notes.join("; "));
            let output = result.tool_result_text.as_deref().unwrap_or_default();
            assert!(output.contains("子任务真实完成"));
            assert!(output.contains(&agent.id));
            assert!(output.contains(run_id));
            assert!(output.contains(conversation_id));
            assert!(output.contains(&workspace_id));
        }
        let target_id = snapshots.keys().next().unwrap().clone();
        let snapshot = snapshots.get(&target_id).unwrap();
        assert!(snapshot.choose_model(Some("unknown-model-config")).is_err());
        let mut limited = (**snapshot).clone();
        limited.permission = PermissionMode::ReadOnly;
        let requested = BTreeSet::from([String::from("read_file"), String::from("write_file")]);
        assert!(!limited.permits_parent_agent());
        assert!(!limited.child_tools(&requested).contains("write_file"));
        let denied_parent = crate::tool_invocation_identity::scope(format!("{turn_id}/parent-denied"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "Agent", &input, Some(&target_id), Some("toolu-agent-denied"), Some(turn_id),
                Some(room_id), Some(&parent), Some(&HostToolScope::Parent(Arc::new(limited.clone()))),
            )).await;
        assert!(denied_parent.is_err(), "接纳时的只读权限不得被后续设置或调度授权放大");
        let denied_path = temp.path().join("must-not-write.txt");
        let frozen_parent_scope = HostToolScope::Parent(Arc::new(limited.clone()));
        let denied_frozen_write = crate::tool_invocation_identity::scope(format!("{turn_id}/frozen-parent-write"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "write_file", &serde_json::json!({"path":denied_path,"content":"forbidden"}),
                Some(&target_id), Some("toolu-frozen-parent-write"), Some(turn_id), Some(room_id),
                Some(&parent), Some(&frozen_parent_scope),
            )).await;
        assert!(denied_frozen_write.is_err(), "普通父工具也必须受冻结只读权限约束");
        assert!(!denied_path.exists(), "拒绝必须发生在文件副作用之前");

        let mut meta_only = (**snapshot).clone();
        meta_only.parent_tools.remove("write_file");
        assert!(meta_only.permits_tool("tools_semantic_dispatch"), "夹具须真实授予元工具");
        let meta_scope = HostToolScope::Parent(Arc::new(meta_only));
        let semantic_input = serde_json::json!({"path":denied_path,"content":"forbidden"});
        assert!(crate::file_write_runtime_input_from_semantic_dispatch_input(&semantic_input).is_some());
        let denied_expanded_write = crate::tool_invocation_identity::scope(format!("{turn_id}/expanded-write"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "tools_semantic_dispatch", &semantic_input, Some(&target_id), Some("toolu-expanded-write"),
                Some(turn_id), Some(room_id), Some(&parent), Some(&meta_scope),
            )).await;
        assert!(denied_expanded_write.is_err(), "元工具授权不得替代展开写工具授权");
        assert!(!denied_path.exists());
        let limited_scope = Arc::new(HostChildScope {
            parent: parent.clone(), parent_call_id: String::from("tool-child-denied"),
            parent_turn_id: turn_id.to_string(), tool_session_id: target_id.clone(),
            usage_session_id: conversation_id.to_string(), room_id: room_id.to_string(),
            allowed_tools: limited.child_tools(&requested),
            definitions: limited.parent_definitions.iter()
                .filter(|tool| tool.name == "read_file").cloned().collect(),
            permission: limited.permission, workspace_root: temp.path().to_path_buf(),
            usage_db_path: db.clone(), client: limited.parent_model.client.clone(),
            request: limited.parent_model.request.clone(), cancellation: Arc::clone(&cancellation),
        });
        let denied = crate::tool_invocation_identity::scope(format!("{turn_id}/denied"),
            crate::run_model_tool_dispatch_for_session_with_identity(
                "write_file", &serde_json::json!({"path":denied_path,"content":"forbidden"}),
                Some(&target_id), Some("toolu-denied"), Some(turn_id), Some(room_id),
                Some(&parent), Some(&HostToolScope::Child(limited_scope)),
            )).await;
        assert!(denied.is_err(), "子执行必须在真实分发入口拒绝超权工具");
        assert!(!denied_path.exists());
        let invoke_runner = |parent: FrozenParentContext, cancellation: Arc<ChatTurnCancellation>| {
            WebHostAgentRunner {
                snapshot: Arc::clone(snapshot), parent,
                parent_call_id: String::from("tool-child-boundary"),
                parent_turn_id: turn_id.to_string(), caller_agent_session_id: target_id.clone(),
                cancellation,
            }
        };
        let cancelled = Arc::new(ChatTurnCancellation::new());
        cancelled.request();
        assert!(tools::execute_agent_with_host(&input,
            Some(&invoke_runner(parent.clone(), cancelled))).await.unwrap_err().contains("取消"));
        let mut expired_parent = parent.clone();
        expired_parent.root_budget = Some(crate::root_execution_budget::RootExecutionBudget::establish(0));
        assert!(tools::execute_agent_with_host(&input,
            Some(&invoke_runner(expired_parent, Arc::new(ChatTurnCancellation::new()))))
            .await.unwrap_err().contains("超时"));
        let connection = crate::open_session_connection(&db).unwrap();
        let usage = connection.query_row("SELECT workspace_id,room_id,session_id,turn_id,COUNT(*),SUM(input_tokens),SUM(output_tokens)
            FROM chat_usage_events WHERE request_kind='child_agent' GROUP BY workspace_id,room_id,session_id,turn_id", [],
            |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,String>(3)?, row.get::<_,i64>(4)?, row.get::<_,i64>(5)?, row.get::<_,i64>(6)?))).unwrap();
        assert_eq!(usage.0, workspace_id);
        assert_eq!(usage.1, room_id);
        assert_eq!(usage.2, conversation_id);
        assert_eq!(usage.3, turn_id);
        assert_eq!(usage.4, 2);
        assert_eq!(usage.5, 22);
        assert_eq!(usage.6, 14);
        let distinct_calls: i64 = connection.query_row(
            "SELECT COUNT(DISTINCT call_id) FROM chat_usage_events WHERE request_kind='child_agent'",
            [], |row| row.get(0)).unwrap();
        assert_eq!(distinct_calls, 2, "各子请求须归属不同的父工具调用");
        let linked_to_parent: i64 = connection.query_row(
            "SELECT COUNT(*) FROM chat_usage_events WHERE request_kind='child_agent' AND run_id=?1",
            [run_id], |row| row.get(0)).unwrap();
        assert_eq!(linked_to_parent, 2, "每个子请求都须指向父持久 run");
        let inflight_cancellation = Arc::new(ChatTurnCancellation::new());
        let inflight_runner = invoke_runner(parent.clone(), Arc::clone(&inflight_cancellation));
        let inflight = tokio::spawn(async move {
            tools::execute_agent_with_host(&serde_json::json!({
                "description":"慢速取消测试", "prompt":"等待父运行取消", "allowed_tools":[]
            }), Some(&inflight_runner)).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), slow_request_started.notified())
            .await.expect("子请求应到达真实本地模型端点");
        inflight_cancellation.request();
        let aborted = tokio::time::timeout(std::time::Duration::from_secs(3), inflight)
            .await.expect("父取消须及时终止子执行").unwrap().unwrap_err();
        assert!(aborted.contains("取消"), "{aborted}");
        let mut expiring_parent = parent.clone();
        expiring_parent.root_budget = Some(crate::root_execution_budget::RootExecutionBudget::establish(600));
        let replay = tools::execute_agent_with_host(&input,
            Some(&invoke_runner(parent.clone(), Arc::new(ChatTurnCancellation::new())))).await;
        assert!(replay.unwrap_err().contains("不重放"), "同一父工具来源不能重放已取消委派");
        let mut expiring_runner = invoke_runner(expiring_parent, Arc::new(ChatTurnCancellation::new()));
        // 超时用例是另一条真实委派来源，不能复用已取消调用的持久ID。
        expiring_runner.parent_call_id = String::from("tool-child-timeout");
        let expiring = tokio::spawn(async move {
            tools::execute_agent_with_host(&serde_json::json!({
                "description":"慢速取消测试", "prompt":"等待父运行截止", "allowed_tools":[]
            }), Some(&expiring_runner)).await
        });
        // 根短预算可在网络派发前耗尽；取消用例已单独验证执行中的真实请求。
        // 此处核截止终止和不重复记账，不能要求到期前一定完成端点握手。
        let timed_out = tokio::time::timeout(std::time::Duration::from_secs(3), expiring)
            .await.expect("根预算截止须终止子执行").unwrap().unwrap_err();
        assert!(timed_out.contains("超时"), "{timed_out}");
        let completed: i64 = connection.query_row(
            "SELECT COUNT(*) FROM chat_usage_events WHERE request_kind='child_agent' AND status='completed'",
            [], |row| row.get(0)).unwrap();
        assert_eq!(completed, 2, "取消与超时不能重复记为已完成请求");
        release_slow_request.notify_waiters();
        server.abort();
    }

}
