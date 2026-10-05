//! 每轮专用的本机 MCP 桥。秘密只出现在协议连接参数，不进入日志和 Agent DTO。
//! 工具进入既有监督、权限、审批和台账；远端 tool_call 通知不经过此入口。
use super::{journal::Journal, protocol::ExecutionScope};
use crate::*;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::post,
};
use std::{cell::RefCell, collections::BTreeMap};

const MCP_VERSION: &str = "2025-06-18";
const INITIAL_TOOLS: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "glob_search",
    "grep_search",
    "bash",
    "computer_use_perform",
];
pub(super) const REVIEW_TOOLS: &[&str] = &["read_file", "glob_search", "grep_search"];

tokio::task_local! {static FROZEN_POLICY:Arc<FrozenPolicy>;}
thread_local! {static WORKER_POLICY:RefCell<Option<Arc<FrozenPolicy>>>=const{RefCell::new(None)};}

pub(crate) fn current() -> Option<Arc<FrozenPolicy>> {
    FROZEN_POLICY.try_with(Arc::clone).ok()
}
pub(crate) fn worker_policy() -> Option<Arc<FrozenPolicy>> {
    WORKER_POLICY.with(|p| p.borrow().clone())
}
pub(crate) fn with_worker<T>(policy: Option<Arc<FrozenPolicy>>, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<FrozenPolicy>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            WORKER_POLICY.with(|p| *p.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(WORKER_POLICY.with(|p| std::mem::replace(&mut *p.borrow_mut(), policy)));
    run()
}

pub(crate) struct FrozenPolicy {
    parent: FrozenParentContext,
    scope: ExecutionScope,
    journal: Journal,
    cancellation: Arc<ChatTurnCancellation>,
    root: PathBuf,
    profile: PermissionProfile,
    rules: Vec<ProtectedRule>,
    grants: BTreeMap<String, SessionGrantView>,
    parent_claim: (Option<String>, String),
    readonly: bool,
    /// 固定工程在 blocking worker 真实收尾前仍被持有。
    _pin: workspace_activity::WorkspacePin,
}

impl FrozenPolicy {
    fn tool_live(&self, name: &str) -> Result<(), String> {
        if !self.grants.contains_key(name)
            || required_permission_for_tool(name)
                > llm_tool_permission_for_room(Some(&self.scope.room_id))
            || !llm_tool_definitions_for_session(&self.scope.agent_id, Some(&self.scope.room_id))
                .is_some_and(|tools| tools.iter().any(|tool| tool.name == name))
        {
            return Err("ACP 工具已被当前会话或房间撤销，未执行。".into());
        }
        Ok(())
    }
    fn live(&self) -> Result<(), String> {
        if self.cancellation.is_requested() {
            return Err("本轮已停止，工具未执行。".into());
        }
        if self
            .parent
            .root_budget
            .as_ref()
            .is_none_or(|b| b.is_expired())
        {
            return Err("ACP 工具缺少有效根预算或已超时。".into());
        }
        self.journal.can_dispatch(&self.scope)?;
        if let Some(goal) = &self.parent.goal_phase {
            goal.validate_live()?;
        }
        let db = self
            .parent
            .runtime_db_path
            .as_deref()
            .ok_or("ACP 工具缺少父运行数据库。")?;
        validate_frozen_parent_relations(db, &self.parent)
            .map_err(|_| "ACP 父运行已失效。".to_string())?;
        if parent_claim(db, &self.scope.run_id)? != self.parent_claim {
            return Err("ACP 父运行的 owner/claim 已改变，旧工具资格失效。".into());
        }
        if workspace_identity(&active_workspace_path()) != self.scope.workspace_id {
            return Err("ACP 当前工程已改变，工具未执行。".into());
        }
        Ok(())
    }

    /// 在 blocking worker 中、真实 executor 之前再次裁决冻结策略。
    /// 外层仍会评估实时策略，两个门都通过才能执行，开发开关也不能跳过冻结门。
    pub(crate) fn gate(&self, invoke: &ToolInvoke, root: &Path) -> Option<ToolOutcome> {
        let failure = self
            .live()
            .and_then(|_| self.tool_live(&invoke.tool_name))
            .err()
            .or_else(|| {
                if root != self.root
                    || invoke.workspace_id != self.scope.workspace_id
                    || invoke.session_id.as_deref() != Some(self.scope.agent_id.as_str())
                    || !self.grants.contains_key(&invoke.tool_name)
                {
                    Some("ACP 工具超出冻结的身份或工具集合。".into())
                } else {
                    None
                }
            });
        if let Some(reason) = failure {
            return Some(runtime_tool_failed_outcome(invoke, reason));
        }
        let mut frozen_invoke = invoke.clone();
        frozen_invoke.user_authorized = false;
        frozen_invoke.user_confirmed_twice = false;
        let grant = &self.grants[&invoke.tool_name];
        let report = evaluate_permission(
            &frozen_invoke,
            required_permission_for_tool(&invoke.tool_name),
            &extract_path_targets(&invoke.tool_name, &invoke.input),
            &self.root,
            &self.rules,
            grant,
            self.profile,
        );
        if matches!(
            report.decision,
            PermissionDecision::AllowAuto | PermissionDecision::AllowApproved
        ) {
            return None;
        }
        Some(ToolOutcome {
            call_id: invoke.call_id.clone(),
            tool_name: invoke.tool_name.clone(),
            status: ToolOutcomeStatus::DryRunOnly,
            output: json!({"executed":false,"reason":report.reason}),
            summary_text: "接纳时的权限未允许该工具，未执行。".into(),
            elapsed_ms: 0,
            permission_gate: report,
            evidence: None,
        })
    }
}

pub(super) struct ToolBridge {
    policy: Arc<FrozenPolicy>,
    definitions: Vec<ToolDefinition>,
    token: String,
    initialized: AtomicBool,
    calls: Arc<tokio::sync::Mutex<()>>,
    cu_job: std::sync::Mutex<Option<super::tool_wait::PendingTool>>,
    /// 仅承载 bridge 与监督者的本地取消映射，不复用另一个 turn 的 token。
    _cancel_scope: ToolTurnCancellationScope,
}

impl ToolBridge {
    pub(super) fn capture(
        parent: FrozenParentContext,
        scope: ExecutionScope,
        journal: Journal,
        cancellation: Arc<ChatTurnCancellation>,
    ) -> Result<Arc<Self>, String> {
        Self::capture_mode(parent, scope, journal, cancellation, false)
    }

    pub(super) fn capture_review(
        parent: FrozenParentContext, scope: ExecutionScope, journal: Journal,
        cancellation: Arc<ChatTurnCancellation>,
    ) -> Result<Arc<Self>, String> {
        Self::capture_mode(parent, scope, journal, cancellation, true)
    }

    fn capture_mode(
        parent: FrozenParentContext, scope: ExecutionScope, journal: Journal,
        cancellation: Arc<ChatTurnCancellation>, readonly: bool,
    ) -> Result<Arc<Self>, String> {
        let pin = workspace_activity::pin_workspace()?;
        let root = active_workspace_path();
        if !scope.accepts(&scope)
            || parent.workspace_id.as_str() != scope.workspace_id
            || parent.room_id.as_deref() != Some(scope.room_id.as_str())
            || parent.parent_run_id.as_deref() != Some(scope.run_id.as_str())
            || parent.public_turn_id.as_deref() != Some(scope.turn_id.as_str())
            || workspace_identity(&root) != scope.workspace_id
            || parent.runtime_db_path.as_deref() != Some(journal.path())
        {
            return Err("ACP 桥身份必须来自同库的真实父接纳，不能由调用参数补造。".into());
        }
        journal.binding(&scope)?;
        if parent.root_budget.as_ref().is_none_or(|b| b.is_expired()) {
            return Err("ACP 桥缺少有效父预算。".into());
        }
        validate_frozen_parent_relations(journal.path(), &parent)
            .map_err(|_| "ACP 父运行不可执行。")?;
        let connection = journal.connection()?;
        if !chat_run_admission::target_is_admitted_on(&connection, &scope.run_id, &scope.agent_id)
            .map_err(|_| "ACP 本轮发送目标记录不可用。")? {
            return Err("ACP 当前模型不是本轮已接纳的发送对象。".into());
        }
        let parent_claim = parent_claim(journal.path(), &scope.run_id)?;
        let permission = llm_tool_permission_for_room(Some(&scope.room_id));
        let definitions = llm_tool_definitions_for_session(&scope.agent_id, Some(&scope.room_id))
            .unwrap_or_default()
            .into_iter()
            .filter(|tool| {
                (if readonly { REVIEW_TOOLS } else { INITIAL_TOOLS }).contains(&tool.name.as_str())
                    && required_permission_for_tool(&tool.name) <= permission
            })
            .collect::<Vec<_>>();
        // 仅开放明确接入的内置工具；CU 复用异步控制器，插件与外部 MCP 尚未接入。
        let dev_open = dev_open_tool_permissions_enabled();
        let profile = if dev_open {
            PermissionProfile::FullAccess
        } else {
            active_permission_profile()
        };
        let rules = if dev_open {
            vec![]
        } else {
            effective_protected_rules()
        };
        let grants = definitions
            .iter()
            .map(|tool| {
                let room =
                    room_permission_grant_view_for_path(journal.path(), Some(&scope.room_id));
                let grant = if dev_open {
                    dev_open_session_grant()
                } else if room.session_authorized {
                    room
                } else {
                    session_tool_grant_view_for(
                        &scope.workspace_id,
                        Some(&scope.agent_id),
                        &tool.name,
                    )
                };
                (tool.name.clone(), grant)
            })
            .collect();
        let existing = tool_turn_cancellation_registry()
            .lock()
            .map_err(|_| "取消映射不可用。")?
            .get(&scope.turn_id)
            .cloned();
        if existing
            .as_ref()
            .is_some_and(|token| !Arc::ptr_eq(token, &cancellation))
        {
            return Err("ACP turn 已绑定另一取消身份。".into());
        }
        // 独立桥 trace 不覆盖现有公共 turn 映射；监督器获取同一个取消 token。
        let trace = bridge_trace(&scope)?;
        let cancel_scope = ToolTurnCancellationScope::install(&trace, cancellation.clone());
        Ok(Arc::new(Self {
            policy: Arc::new(FrozenPolicy {
                parent,
                scope,
                journal,
                cancellation,
                root,
                profile,
                rules,
                grants,
                parent_claim,
                readonly,
                _pin: pin,
            }),
            definitions,
            token: random_hex_identifier(64, "ACP 桥")?,
            initialized: AtomicBool::new(false),
            calls: Arc::new(tokio::sync::Mutex::new(())),
            cu_job: std::sync::Mutex::new(None),
            _cancel_scope: cancel_scope,
        }))
    }

    fn check_prepared_or_submitted(&self) -> Result<(), String> {
        self.policy.journal.binding(&self.policy.scope)?;
        let state = self.policy.journal.status(&self.policy.scope)?;
        if self.policy.cancellation.is_requested()
            || state.cancel_requested
            || state.process_drained
            || !matches!(state.state.as_str(), "prepared" | "submitted")
        {
            return Err("ACP 桥已失效。".into());
        }
        Ok(())
    }

    async fn rpc(&self, request: JsonValue) -> Result<Option<JsonValue>, String> {
        self.check_prepared_or_submitted()?;
        if request.get("jsonrpc").and_then(JsonValue::as_str) != Some("2.0") {
            return Err("桥请求格式无效。".into());
        }
        let method = request
            .get("method")
            .and_then(JsonValue::as_str)
            .ok_or("桥请求缺少方法。")?;
        let id = request.get("id");
        if method == "notifications/initialized" && id.is_none() {
            return Ok(None);
        }
        let id = id
            .filter(|v| v.is_string() || v.is_i64() || v.is_u64())
            .ok_or("桥请求 ID 无效。")?;
        let result = match method {
            "initialize" => {
                if !request["params"]["protocolVersion"].as_str().is_some_and(|version|
                    version.len() == 10 && version.bytes().all(|byte| byte.is_ascii_digit() || byte == b'-')) {
                    return Ok(Some(
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"MCP 协议版本无效"}}),
                    ));
                }
                // 客户端可提出更新版本；按协议回传本端支持的版本，由客户端确认兼容。
                self.initialized.store(true, Ordering::Release);
                json!({"protocolVersion":MCP_VERSION,"capabilities":{"tools":{"listChanged":false}},
                    "serverInfo":{"name":"coolzhu-agent-tools","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" if self.initialized.load(Ordering::Acquire) => {
                let mut tools=self.definitions.iter().map(|tool|
                    json!({"name":tool.name,"description":tool.description,"inputSchema":tool.input_schema})).collect::<Vec<_>>();
                if self.policy.grants.contains_key("computer_use_perform") {
                    tools.push(json!({"name":"computer_use_wait",
                        "description":"等待 computer_use_perform 返回的同一 job_id。running 不是完成；继续等待到最终回执，禁止重复提交 perform。此工具不创建新的电脑动作任务。",
                        "inputSchema":{"type":"object","properties":{"job_id":{"type":"string"}},"required":["job_id"],"additionalProperties":false}}));
                }
                json!({"tools":tools})
            }
            "tools/call" if self.initialized.load(Ordering::Acquire) => {
                let name = request["params"]["name"]
                    .as_str()
                    .ok_or("桥工具缺少名称。")?;
                let input = request["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !input.is_object() {
                    return Err("桥工具参数必须是对象。".into());
                }
                match name {
                    "computer_use_perform" => self.submit_cu(id,&input).await?,
                    "computer_use_wait" => {
                        self.policy.live()?;
                        self.policy.tool_live("computer_use_perform")?;
                        let job_id=input["job_id"].as_str().ok_or("等待工具缺少 job_id。")?;
                        super::tool_wait::wait(&self.cu_job,job_id,Duration::from_secs(25)).await?
                    },
                    _ => self.call(id, name, &input).await?,
                }
            }
            _ => {
                return Ok(Some(
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"未开放该 MCP 方法"}}),
                ));
            }
        };
        Ok(Some(json!({"jsonrpc":"2.0","id":id,"result":result})))
    }

    async fn call(
        &self,
        id: &JsonValue,
        name: &str,
        input: &JsonValue,
    ) -> Result<JsonValue, String> {
        dispatch(self.policy.clone(),self.calls.clone(),id,name,input).await
    }

    async fn submit_cu(&self,id:&JsonValue,input:&JsonValue) -> Result<JsonValue,String> {
        self.policy.live()?;
        self.policy.tool_live("computer_use_perform")?;
        let job_id={
            let mut slot=self.cu_job.lock().map_err(|_|"CU 等待状态不可用。")?;
            let existing=if let Some(job)=slot.as_ref() {
                if &job.request_id == id && &job.input != input {
                    return Err("同一工具请求不能变更参数。".into());
                }
                if &job.request_id == id || job.unfinished() && &job.input == input {
                    Some(job.id.clone())
                } else if job.unfinished() {
                    return Err("已有 CU 任务未完成，请等待原 job_id；未重复派发。".into());
                } else { None }
            } else { None };
            if let Some(job_id)=existing { job_id } else {
                let job_id=random_hex_identifier(48,"CU 等待")?;
                let policy=self.policy.clone(); let calls=self.calls.clone();
                let request_id=id.clone(); let arguments=input.clone();
                let future=async move { dispatch(policy,calls,&request_id,"computer_use_perform",&arguments).await };
                *slot=Some(super::tool_wait::PendingTool::new(job_id.clone(),id.clone(),input.clone(),future));
                job_id
            }
        };
        super::tool_wait::wait(&self.cu_job,&job_id,Duration::from_secs(25)).await
    }

    /// 只在宿主显式持有桥对象时监听随机本机端口，不挂到主控制台路由。
    pub(super) async fn start(self: Arc<Self>) -> Result<BridgeServer, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "ACP 桥监听失败。")?;
        let address = listener.local_addr().map_err(|_| "ACP 桥端口读取失败。")?;
        let config = json!({"type":"http","name":"coolzhu-agent","url":format!("http://{address}/mcp"),
            "headers":[{"name":"Authorization","value":format!("Bearer {}",self.token)}]});
        let router = axum::Router::new()
            .route("/mcp", post(http_rpc))
            .layer(DefaultBodyLimit::max(1024 * 1024))
            .with_state((self.clone(), address.to_string()));
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(BridgeServer { config, task, bridge:self })
    }
}

async fn dispatch(policy:Arc<FrozenPolicy>,calls:Arc<tokio::sync::Mutex<()>>,
    id:&JsonValue,name:&str,input:&JsonValue) -> Result<JsonValue,String> {
        // 串行队列只约束本轮，阻止共享 MCP 连接的并发动作互相覆盖。
        let _call = calls.lock().await;
        policy.live()?;
        policy.tool_live(name)?;
        if !policy.grants.contains_key(name) {
            return Err("工具未在父接纳时开放。".into());
        }
        // 仓库审查只读且只能访问当前工程；完全访问权限也不会扩大此通道的范围。
        if policy.readonly {
            if !REVIEW_TOOLS.contains(&name) {
                return Err("仓库审查未开放该工具。".into());
            }
            if name == "glob_search" {
                // glob 的 pattern 也可携带路径；复用工程搜索的现有边界检查。
                validate_workspace_relative_pattern(input.get("pattern").and_then(JsonValue::as_str).unwrap_or(""))
                    .map_err(|_| "仓库审查的搜索模式不能越过当前工程。".to_string())?;
            }
            let root = policy.root.canonicalize().map_err(|_| "审查工程不可用。")?;
            let target = input.get("path").and_then(JsonValue::as_str).map(PathBuf::from)
                .unwrap_or_else(|| policy.root.clone());
            let target = if target.is_absolute() { target } else { policy.root.join(target) };
            if !target.canonicalize().map_err(|_| "审查路径不存在。")?.starts_with(&root) {
                return Err("仓库审查不能读取当前工程之外的路径。".into());
            }
        }
        if name == "bash"
            && (input.get("run_in_background").and_then(JsonValue::as_bool) == Some(true)
                || input
                    .get("dangerouslyDisableSandbox")
                    .and_then(JsonValue::as_bool)
                    == Some(true))
        {
            return Err("ACP 桥尚未开放后台命令或沙箱旁路。".into());
        }
        let scope = &policy.scope;
        let raw_id = serde_json::to_string(id).map_err(|_| "工具关联编号编码失败。")?;
        let source = serde_json::to_string(scope).map_err(|_| "工具身份编码失败。")?;
        let identity = tool_invocation_identity::ModelToolIdentity::from_source(
            Some(&scope.run_id),
            &source,
            &raw_id,
        )?;
        if name == "computer_use_perform" {
            // CU 复用模型工具的异步入口，不进入同步工具 worker，也不另登记一份调用。
            let invoke = ToolInvoke { call_id: identity.execution_id.clone(), tool_name: name.into(),
                input: input.clone(), caller: ToolCaller::Llm, workspace_id: scope.workspace_id.clone(), session_id: Some(scope.agent_id.clone()),
                user_authorized: false, user_confirmed_twice: false };
            if let Some(outcome) = policy.gate(&invoke, &policy.root) {
                return Ok(json!({"content":[{"type":"text","text":serde_json::to_string(&outcome).map_err(|_| "CU 拒绝回执编码失败。")?}],"isError":true}));
            }
            let run = tool_invocation_identity::scope(source,
                run_model_tool_dispatch_for_session_with_identity(name, input, Some(&scope.agent_id),
                    Some(&raw_id), Some(&scope.turn_id), Some(&scope.room_id), Some(&policy.parent), None));
            let response = CHAT_CANCELLATION.scope(policy.cancellation.clone(), run)
                .await.map_err(|(status, _)| format!("CU 宿主派发失败：{status}"))?;
            let failed = response.status != "ok";
            let text = serde_json::to_string(&response).map_err(|_| "CU 结果编码失败。")?;
            return Ok(json!({"content":[{"type":"text","text":truncate_tool_result_for_context(text,Some(&policy.parent))}],"isError":failed}));
        }
        let budget = policy
            .parent
            .root_budget
            .clone()
            .ok_or("工具缺少根预算。")?;
        let mut settlement = register_tool_call_at_dispatch(
            policy.journal.path(),
            &identity.execution_id,
            name,
            input,
            Some(&scope.run_id),
            Some(budget.clone()),
            Some(&identity),
        )
        .map_err(|_| "工具台账拒绝接纳，未执行。")?;
        let invoke = ToolInvoke {
            call_id: identity.execution_id.clone(),
            tool_name: name.into(),
            input: runtime_tool_input_with_defaults(name, input),
            caller: ToolCaller::Llm,
            workspace_id: scope.workspace_id.clone(),
            session_id: Some(scope.agent_id.clone()),
            user_authorized: false,
            user_confirmed_twice: false,
        };
        let timeout = budget.limit_ms(tool_timeout_ms_for(name, &invoke.input));
        let run = runtime_tool_supervision::execute(
            invoke.clone(),
            policy.root.clone(),
            timeout,
            Some(scope.room_id.clone()),
        );
        let outcome = FROZEN_POLICY
            .scope(
                policy.clone(),
                TURN_TRACE.scope(
                    bridge_trace(scope)?,
                    root_execution_budget::scope(budget, run),
                ),
            )
            .await;
        // 旧审批队列尚不携带 ACP scope/epoch。此阶段只回传真实拒绝，不能让迟到批准裸执行。
        append_tool_audit_record(&invoke, &outcome);
        let pending = outcome
            .output
            .get("execution_state")
            .and_then(JsonValue::as_str)
            == Some("running_unconfirmed");
        if pending {
            policy
                .journal
                .transition(scope, &["submitted"], "unknown", None, false)?;
        }
        let status = if pending {
            "unknown"
        } else if outcome.status == ToolOutcomeStatus::Ok {
            "completed"
        } else {
            "failed"
        };
        if settlement.finish(status).is_err() {
            let _ = policy
                .journal
                .transition(scope, &["submitted"], "unknown", None, false);
            return Err("工具已结束等待但落盘未确认，禁止重试。".into());
        }
        let is_error = outcome.status != ToolOutcomeStatus::Ok;
        let text = serde_json::to_string(&outcome).map_err(|_| "工具结果编码失败。")?;
        let text = truncate_tool_result_for_context(text, Some(&policy.parent));
        Ok(json!({"content":[{"type":"text","text":text}],"isError":is_error}))
    }

fn bridge_trace(scope: &ExecutionScope) -> Result<String, String> {
    let identity = tool_invocation_identity::ModelToolIdentity::from_source(
        Some(&scope.run_id),
        &serde_json::to_string(scope).map_err(|_| "ACP 身份编码失败。")?,
        "bridge",
    )?;
    Ok(identity.execution_id)
}

fn parent_claim(db: &Path, run: &str) -> Result<(Option<String>, String), String> {
    open_session_connection(db)
        .map_err(|_| "ACP 父运行库不可用。")?
        .query_row(
            "SELECT owner_id,claim_token FROM runtime_runs WHERE id=?1",
            [run],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "ACP 父运行 claim 不存在。".into())
}

pub(super) struct BridgeServer {
    config: JsonValue,
    task: tokio::task::JoinHandle<()>,
    bridge: Arc<ToolBridge>,
}
impl BridgeServer {
    pub(super) fn has_pending(&self) -> bool {
        self.bridge.cu_job.lock().map(|slot|slot.as_ref().is_some_and(|job|job.unfinished())).unwrap_or(true)
    }
    pub(super) fn config(&self) -> JsonValue {
        self.config.clone()
    }
}
impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.task.abort();
        // 终态、取消或服务退出都撤销尚未结束的 future；不会在 HTTP 断连后脱管输入。
        self.bridge.cu_job.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    }
}

async fn http_rpc(
    State((bridge, host)): State<(Arc<ToolBridge>, String)>,
    headers: HeaderMap,
    Json(request): Json<JsonValue>,
) -> Response {
    let method = match request["method"].as_str() {
        Some("initialize") => "initialize", Some("notifications/initialized") => "initialized",
        Some("tools/list") => "tools/list", Some("tools/call") => "tools/call",
        Some("ping") => "ping", _ => "other",
    };
    let clean_version = |value: Option<&str>| value.filter(|value| value.len() == 10
        && value.bytes().all(|byte| byte.is_ascii_digit() || byte == b'-')).unwrap_or("other").to_owned();
    tracing::info!(method, request_protocol = %clean_version(request["params"]["protocolVersion"].as_str()),
        header_protocol = %clean_version(headers.get("mcp-protocol-version").and_then(|value| value.to_str().ok())),
        host_valid = headers.get("host").and_then(|value| value.to_str().ok()) == Some(host.as_str()),
        token_valid = headers.get("authorization").and_then(|value| value.to_str().ok()) == Some(format!("Bearer {}", bridge.token).as_str()),
        origin_present = headers.contains_key("origin"), "ACP 只读工具桥协议诊断");
    // CLI 无 Origin；拒绝浏览器页面请求和 DNS 重绑定。token 不作错误回显。
    if headers.contains_key("origin")
        || headers.get("host").and_then(|v| v.to_str().ok()) != Some(host.as_str())
        || headers.get("authorization").and_then(|v| v.to_str().ok())
            != Some(format!("Bearer {}", bridge.token).as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request["method"].as_str() != Some("initialize")
        && headers
            .get("mcp-protocol-version")
            .and_then(|v| v.to_str().ok())
            != Some(MCP_VERSION)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if method == "tools/call" && headers.get("accept").and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|part| part.trim().split(';').next() == Some("text/event-stream"))) {
        // 长程 CU 使用 Streamable HTTP：立即建立响应并持续输出等待心跳，
        // 不让无响应正文触发客户端的 HTTP 空闲超时。没有 progressToken 时
        // 只发 SSE 注释；有 token 才发协议进度，数值为实际等待毫秒数而非完成率。
        let token = request["params"]["_meta"]["progressToken"].clone();
        let request_id = request["id"].clone();
        let stream = async_stream::stream! {
            let started = tokio::time::Instant::now();
            let pending = bridge.rpc(request);
            tokio::pin!(pending);
            let mut ticks = tokio::time::interval(Duration::from_secs(5));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased;
                    result = &mut pending => {
                        let response = match result {
                            Ok(Some(value)) => value,
                            Ok(None) => break,
                            Err(_) => json!({"jsonrpc":"2.0","id":request_id,
                                "error":{"code":-32000,"message":"宿主工具未完成，请查看运行轨迹；未重试"}}),
                        };
                        yield Ok::<_, std::convert::Infallible>(Event::default().event("message").data(response.to_string()));
                        break;
                    },
                    _ = ticks.tick() => {
                        let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                        if token.is_string() || token.is_i64() || token.is_u64() {
                            let update = json!({"jsonrpc":"2.0","method":"notifications/progress",
                                "params":{"progressToken":token,"progress":elapsed,
                                    "message":"宿主工具执行中，等待最终回执"}});
                            yield Ok(Event::default().event("message").data(update.to_string()));
                        } else {
                            yield Ok(Event::default().comment("宿主工具仍在等待最终回执"));
                        }
                    },
                }
            }
        };
        return Sse::new(stream).into_response();
    }
    match bridge.rpc(request).await {
        Ok(Some(result)) => Json(result).into_response(),
        Ok(None) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::journal::Binding;
    use super::*;
    struct Environment {
        workspace: PathBuf,
        config: WorkspaceConfig,
        db: Option<PathBuf>,
    }
    impl Environment {
        fn install(root: &Path, db: &Path) -> Self {
            // 初始化配置可能读取工程；每次替换后先释放锁，不能在 struct 表达式内保留临时 guard。
            let workspace = std::mem::replace(
                &mut workspace_state().lock().unwrap().current,
                root.to_owned(),
            );
            let mut fixture_config = WorkspaceConfig::default();
            fixture_config.model.enable_llm_tools = true;
            fixture_config.model.llm_tool_exposure = Some("all".into());
            let config =
                std::mem::replace(&mut *workspace_config().lock().unwrap(), fixture_config);
            let previous_db = replace_session_db_path_override_for_test(Some(db.to_owned()));
            Self {
                workspace,
                config,
                db: previous_db,
            }
        }
    }
    impl Drop for Environment {
        fn drop(&mut self) {
            workspace_state()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .current = self.workspace.clone();
            *workspace_config()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = self.config.clone();
            replace_session_db_path_override_for_test(self.db.take());
            tools::set_project_config_root(None);
        }
    }
    fn fixture(root: &Path, db: &Path) -> (Arc<ToolBridge>, Journal, ExecutionScope) {
        let connection = open_session_connection(db).unwrap();
        initialize_session_schema(&connection).unwrap();
        connection.execute("INSERT INTO sessions(id,name,provider,model,api_key_ref,created_at,updated_at) VALUES('bridge-agent','fixture','devin','actual','',1,1)",[]).unwrap();
        connection.execute("INSERT INTO chat_rooms(id,name,created_at,updated_at) VALUES('bridge-room','fixture',1,1)",[]).unwrap();
        let workspace_id = workspace_identity(root);
        connection.execute("INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,legacy_turn_id,state,claim_token,created_at)
            VALUES('bridge-run','chat_turn',?1,'bridge-agent','bridge-room','bridge-turn','running','claim-1',1)",[&workspace_id]).unwrap();
        let journal = Journal::open(db).unwrap();
        let scope = journal
            .claim(
                ExecutionScope {
                    workspace_id: workspace_id.clone(),
                    room_id: "bridge-room".into(),
                    agent_id: "bridge-agent".into(),
                    lane: String::new(),
                    run_id: "bridge-run".into(),
                    turn_id: "bridge-turn".into(),
                    attempt_id: "bridge-attempt".into(),
                    owner_epoch: 0,
                    generation: 0,
                },
                &Binding {
                    remote_session_id: None,
                    cwd: root.to_string_lossy().into(),
                    cli_identity: "offline-fixture".into(),
                    context_digest: "context-1".into(),
                },
            )
            .unwrap();
        let mut parent = FrozenParentContext::new(
            "bridge-test",
            &workspace_id,
            Some(&scope.room_id),
            Some(&scope.agent_id),
            Some(&scope.turn_id),
            Some(&scope.run_id),
        )
        .unwrap();
        parent.root_budget = Some(root_execution_budget::RootExecutionBudget::establish(30000));
        parent.runtime_db_path = Some(db.to_owned());
        let cancellation = Arc::new(ChatTurnCancellation::new());
        // fixture 提供固定目录以独立验收真实 executor；不声称真实 CLI 发现已完成。
        let definitions = mvp_tool_specs()
            .into_iter()
            .filter(|t| INITIAL_TOOLS.contains(&t.name))
            .map(|t| ToolDefinition {
                name: t.name.into(),
                description: Some(t.description.into()),
                input_schema: t.input_schema,
            })
            .collect::<Vec<_>>();
        let grants = definitions
            .iter()
            .map(|d| (d.name.clone(), SessionGrantView::default()))
            .collect();
        let trace = bridge_trace(&scope).unwrap();
        let bridge = Arc::new(ToolBridge {
            policy: Arc::new(FrozenPolicy {
                parent,
                scope: scope.clone(),
                journal: journal.clone(),
                cancellation: cancellation.clone(),
                root: root.to_owned(),
                profile: PermissionProfile::WorkspaceAuto,
                rules: default_protected_rules(),
                grants,
                parent_claim: parent_claim(db, &scope.run_id).unwrap(),
                readonly: false,
                _pin: workspace_activity::pin_workspace().unwrap(),
            }),
            definitions,
            token: random_hex_identifier(64, "fixture").unwrap(),
            initialized: AtomicBool::new(false),
            calls: Arc::new(tokio::sync::Mutex::new(())),
            cu_job: std::sync::Mutex::new(None),
            _cancel_scope: ToolTurnCancellationScope::install(&trace, cancellation),
        });
        (bridge, journal, scope)
    }
    #[test]
    fn admitted_target_is_distinct_from_primary_and_cannot_be_added_by_another_run() {
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (_bridge, _, _) = fixture(dir.path(), &db);
        let connection = open_session_connection(&db).unwrap();
        connection.execute("INSERT INTO sessions(id,name,provider,model,api_key_ref,created_at,updated_at) VALUES('other-agent','target','devin','actual','',1,1)", []).unwrap();
        assert!(chat_run_admission::target_is_admitted_on(&connection, "bridge-run", "bridge-agent").unwrap());
        assert!(!chat_run_admission::target_is_admitted_on(&connection, "bridge-run", "other-agent").unwrap());
        append_runtime_run_event(&db, "bridge-run", "chat.target_agents", json!({"agent_ids":["other-agent","missing-agent"]})).unwrap();
        assert!(chat_run_admission::target_is_admitted_on(&connection, "bridge-run", "other-agent").unwrap());
        assert!(!chat_run_admission::target_is_admitted_on(&connection, "bridge-run", "bridge-agent").unwrap());
        assert!(!chat_run_admission::target_is_admitted_on(&connection, "bridge-run", "missing-agent").unwrap());
        assert!(!chat_run_admission::target_is_admitted_on(&connection, "other-run", "other-agent").unwrap());
        append_runtime_run_event(&db, "bridge-run", "chat.target_agents", json!({"agent_ids":["bridge-agent"]})).unwrap();
        assert!(!chat_run_admission::target_is_admitted_on(&connection, "bridge-run", "other-agent").unwrap());
    }

    #[tokio::test]
    async fn streamable_http_returns_single_real_tool_result_with_matching_request_id() {
        let _guard = crate::tests::config_test_guard();
        let dir=tempfile::tempdir().unwrap();
        let db=dir.path().join("sessions.sqlite3");
        let _env=Environment::install(dir.path(),&db);
        let (bridge,journal,scope)=fixture(dir.path(),&db);
        journal.transition(&scope,&["prepared"],"submitted",None,true).unwrap();
        bridge.initialized.store(true,Ordering::Release);
        let file=dir.path().join("read.txt");
        std::fs::write(&file,"协议响应验收").unwrap();
        let mut headers=HeaderMap::new();
        headers.insert("host","127.0.0.1:12345".parse().unwrap());
        headers.insert("authorization",format!("Bearer {}",bridge.token).parse().unwrap());
        headers.insert("mcp-protocol-version",MCP_VERSION.parse().unwrap());
        headers.insert("accept","application/json, text/event-stream".parse().unwrap());
        let response=http_rpc(State((bridge.clone(),"127.0.0.1:12345".into())),headers,
            Json(json!({"jsonrpc":"2.0","id":"real-read","method":"tools/call",
                "params":{"name":"read_file","arguments":{"path":file},"_meta":{"progressToken":"progress-read"}}}))).await;
        assert_eq!(response.headers()["content-type"],"text/event-stream");
        let body=axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap();
        let text=std::str::from_utf8(&body).unwrap();
        assert!(!text.contains(&bridge.token));
        let messages=text.lines().filter_map(|line|line.strip_prefix("data: "))
            .map(|line|serde_json::from_str::<JsonValue>(line).unwrap()).collect::<Vec<_>>();
        let replies=messages.iter().filter(|value|value["id"]=="real-read").collect::<Vec<_>>();
        assert_eq!(replies.len(),1);
        assert_eq!(replies[0]["result"]["isError"],false);
        assert!(replies[0].to_string().contains("协议响应验收"));
        for value in messages.iter().filter(|value|value["method"]=="notifications/progress") {
            assert_eq!(value["params"]["progressToken"],"progress-read");
            assert!(value["params"].get("total").is_none());
        }
    }

    #[tokio::test]
    async fn bridge_uses_real_executor_and_ledger_and_blocks_duplicate_or_cancelled_writes() {
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (bridge, journal, scope) = fixture(dir.path(), &db);
        journal
            .transition(&scope, &["prepared"], "submitted", None, true)
            .unwrap();
        let file = dir.path().join("producer.txt");
        let result = bridge
            .call(
                &json!("write-1"),
                "write_file",
                &json!({"path":file,"content":"真实本地执行"}),
            )
            .await
            .unwrap();
        assert_eq!(result["isError"], false);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "真实本地执行");
        let store = computer_use_store::ComputerUseRunStore::open(&db).unwrap();
        let rows = store.tool_calls_for_run(&scope.run_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "completed");
        assert!(
            bridge
                .call(
                    &json!("write-1"),
                    "write_file",
                    &json!({"path":file,"content":"重复"})
                )
                .await
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "真实本地执行");
        journal.request_cancel(&scope).unwrap();
        assert!(
            bridge
                .call(
                    &json!("write-2"),
                    "write_file",
                    &json!({"path":file,"content":"停止后"})
                )
                .await
                .is_err()
        );
        assert!(
            bridge
                .call(&json!("escape"), "tools_semantic_dispatch", &json!({}))
                .await
                .is_err()
        );
        assert_eq!(store.tool_calls_for_run(&scope.run_id).unwrap().len(), 1);
    }
    #[tokio::test]
    async fn frozen_gate_survives_live_full_access_and_produces_real_denial() {
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (bridge, journal, scope) = fixture(dir.path(), &db);
        journal
            .transition(&scope, &["prepared"], "submitted", None, true)
            .unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("must-not-exist.txt");
        let _dev_open = crate::tests::DevOpenPermissionsTestGuard::enable();
        let result = bridge
            .call(
                &json!("outside"),
                "write_file",
                &json!({"path":file,"content":"越权"}),
            )
            .await
            .unwrap();
        assert_eq!(result["isError"], true);
        assert!(!file.exists());
        let text = result["content"][0]["text"].as_str().unwrap();
        let outcome: JsonValue = serde_json::from_str(text).unwrap();
        assert_eq!(outcome["status"], "dry-run-only");
        assert_ne!(outcome["permission_gate"]["reason"], "full-access-profile");
    }
    #[tokio::test]
    async fn acp_turn_roundtrip_uses_http_bridge_and_real_host_producer() {
        use super::super::{
            session::{SessionService, SessionTransport, Update},
            transport::read_frame,
        };
        use tokio::io::{AsyncWriteExt, BufReader};
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (bridge, journal, scope) = fixture(dir.path(), &db);
        let cancellation = bridge.policy.cancellation.clone();
        let server = bridge.clone().start().await.unwrap();
        let config = server.config();
        let expected_config = config.clone();
        let file = dir.path().join("roundtrip.txt");
        let peer_file = file.clone();
        let (host, peer) = tokio::io::duplex(8192);
        let (host_reader, w) = tokio::io::split(host);
        let (peer_reader, mut writer) = tokio::io::split(peer);
        let peer = tokio::spawn(async move {
            let mut reader = BufReader::new(peer_reader);
            let options = json!([{"id":"model","category":"model","type":"select","currentValue":"fixture-model","options":[{"value":"fixture-model"}]}]);
            for (method, result) in [
                (
                    "initialize",
                    json!({"protocolVersion":1,"agentCapabilities":{"mcpCapabilities":{"http":true}}}),
                ),
                (
                    "session/new",
                    json!({"sessionId":"fixture-remote","configOptions":options}),
                ),
                (
                    "session/set_config_option",
                    json!({"configOptions":options}),
                ),
            ] {
                let request = read_frame(&mut reader).await.unwrap();
                assert_eq!(request["method"], method);
                if method == "session/new" {
                    assert_eq!(request["params"]["mcpServers"][0], expected_config);
                }
                let mut bytes = serde_json::to_vec(
                    &json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
                )
                .unwrap();
                bytes.push(b'\n');
                writer.write_all(&bytes).await.unwrap();
            }
            let prompt = read_frame(&mut reader).await.unwrap();
            assert_eq!(prompt["method"], "session/prompt");
            let client = reqwest::Client::new();
            let url = expected_config["url"].as_str().unwrap();
            let token = expected_config["headers"][0]["value"].as_str().unwrap();
            let initialize=client.post(url).header("Authorization",token).json(&json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":MCP_VERSION}})).send().await.unwrap();
            assert_eq!(initialize.status(), StatusCode::OK);
            let result:JsonValue=client.post(url).header("Authorization",token).header("MCP-Protocol-Version",MCP_VERSION)
                .json(&json!({"jsonrpc":"2.0","id":"tool-producer-1","method":"tools/call","params":{"name":"write_file","arguments":{"path":peer_file,"content":"ACP → MCP → 本地真实执行"}}})).send().await.unwrap().json().await.unwrap();
            assert_eq!(result["result"]["isError"], false);
            for update in [
                json!({"sessionUpdate":"tool_call_update","toolCallId":"remote-only-observation","status":"completed"}),
                json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"已写入"}}),
            ] {
                let mut bytes=serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fixture-remote","update":update}})).unwrap();
                bytes.push(b'\n');
                writer.write_all(&bytes).await.unwrap();
            }
            let mut bytes = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}}),
            )
            .unwrap();
            bytes.push(b'\n');
            writer.write_all(&bytes).await.unwrap();
        });
        let mut events = vec![];
        let mut service = SessionService::connect(
            SessionTransport::new(BufReader::new(host_reader), w),
            journal.clone(),
            scope.clone(),
            "fixture-model",
            vec![config],
            tokio::time::Instant::now() + Duration::from_secs(5),
            |_| Ok(()),
        )
        .await
        .unwrap();
        let result = service
            .prompt(
                "写入指定文件",
                cancellation,
                Duration::from_secs(1),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .await
            .unwrap();
        peer.await.unwrap();
        assert_eq!(result.model.effective.as_deref(), Some("fixture-model"));
        assert_eq!(result.stop_reason, "end_turn");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "ACP → MCP → 本地真实执行"
        );
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].update, Update::ToolObservation { .. }));
        let calls = computer_use_store::ComputerUseRunStore::open(&db)
            .unwrap()
            .tool_calls_for_run(&scope.run_id)
            .unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].status, "completed");
        assert!(!journal.status(&scope).unwrap().process_drained);
        // duplex 没有真实 CLI 进程，此用例不填写排空证据或解除会话锁。
        drop(service);
        drop(server);
        drop(bridge);
    }
    #[tokio::test]
    async fn revoking_tools_live_blocks_admission_before_side_effect_or_tool_record() {
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (bridge, journal, scope) = fixture(dir.path(), &db);
        journal
            .transition(&scope, &["prepared"], "submitted", None, true)
            .unwrap();
        workspace_config().lock().unwrap().model.enable_llm_tools = false;
        let file = dir.path().join("must-not-exist.txt");
        assert!(
            bridge
                .call(
                    &json!("write-1"),
                    "write_file",
                    &json!({"path":file,"content":"撤销后"})
                )
                .await
                .is_err()
        );
        assert!(!file.exists());
        assert!(
            computer_use_store::ComputerUseRunStore::open(&db)
                .unwrap()
                .tool_calls_for_run(&scope.run_id)
                .unwrap()
                .is_empty()
        );
    }
    #[tokio::test]
    async fn parent_claim_change_blocks_execution_before_file_write() {
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (bridge, journal, scope) = fixture(dir.path(), &db);
        journal
            .transition(&scope, &["prepared"], "submitted", None, true)
            .unwrap();
        open_session_connection(&db)
            .unwrap()
            .execute(
                "UPDATE runtime_runs SET claim_token='other-owner' WHERE id=?1",
                [&scope.run_id],
            )
            .unwrap();
        let file = dir.path().join("must-not-exist.txt");
        assert!(
            bridge
                .call(
                    &json!("write-1"),
                    "write_file",
                    &json!({"path":file,"content":"错误所有权"})
                )
                .await
                .is_err()
        );
        assert!(!file.exists());
        assert!(
            computer_use_store::ComputerUseRunStore::open(&db)
                .unwrap()
                .tool_calls_for_run(&scope.run_id)
                .unwrap()
                .is_empty()
        );
    }
    #[tokio::test]
    async fn http_bridge_rejects_browser_missing_token_wrong_host_and_unlisted_methods() {
        let _guard = crate::tests::config_test_guard();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let _env = Environment::install(dir.path(), &db);
        let (bridge, _, _) = fixture(dir.path(), &db);
        let server = bridge.clone().start().await.unwrap();
        let config = server.config();
        let url = config["url"].as_str().unwrap();
        let token = config["headers"][0]["value"].as_str().unwrap();
        let client = reqwest::Client::new();
        // 真实 Devin 客户端请求较新版本；服务应协商本端支持的版本。
        let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}});
        assert_eq!(
            client
                .post(url)
                .json(&initialize)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .post(url)
                .header("Authorization", token)
                .header("Origin", "https://example.com")
                .json(&initialize)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .post(url)
                .header("Authorization", token)
                .header("Host", "attacker.example")
                .json(&initialize)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let response = client
            .post(url)
            .header("Authorization", token)
            .json(&initialize)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result: JsonValue = response.json().await.unwrap();
        assert_eq!(result["result"]["protocolVersion"], MCP_VERSION);
        let response = client
            .post(url)
            .header("Authorization", token)
            .header("MCP-Protocol-Version", MCP_VERSION)
            .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .send()
            .await
            .unwrap();
        let result: JsonValue = response.json().await.unwrap();
        assert_eq!(result["result"]["tools"].as_array().unwrap().len(), 6);
        let response = client
            .post(url)
            .header("Authorization", token)
            .header("MCP-Protocol-Version", MCP_VERSION)
            .json(&json!({"jsonrpc":"2.0","id":3,"method":"resources/read"}))
            .send()
            .await
            .unwrap();
        let result: JsonValue = response.json().await.unwrap();
        assert_eq!(result["error"]["code"], -32601);
        assert!(workspace_activity::begin_workspace_change().is_err());
        drop(server);
        drop(bridge);
    }
}
