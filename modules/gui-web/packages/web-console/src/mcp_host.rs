//! Web MCP Host：复用 core-runtime 的受监督 stdio 连接与统一权限门禁。
//! 每个连接绑定规范化工作区；网页只能提交当前会话/聊天室身份，授权由后端重新求值。

use super::*;
use runtime::{
    mcp_tool_name, ConfigSource, McpServerConfig as RuntimeMcpServerConfig, McpServerManager,
    McpStdioServerConfig, ScopedMcpServerConfig,
};

#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
pub(super) struct McpServerConfig {
    id: String,
    name: String,
    #[serde(default)]
    category: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default = "default_mcp_transport")]
    transport: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    notes: String,
}

fn default_mcp_transport() -> String { "stdio".to_string() }

pub(super) const DEFAULT_MCP_SERVERS_JSON: &str = include_str!("mcp_default_servers.json");

fn mcp_servers_config_path(root: &Path) -> PathBuf {
    root.join(".coolzhu").join("mcp_servers.json")
}

fn load_mcp_server_configs(root: &Path) -> ApiResult<Vec<McpServerConfig>> {
    let path = mcp_servers_config_path(root);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DEFAULT_MCP_SERVERS_JSON.to_string(),
        Err(error) => return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("MCP 配置读取失败：{error}"))),
    };
    let configs: Vec<McpServerConfig> = serde_json::from_str(&text)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &format!("MCP 配置无效：{error}")))?;
    let mut names = HashSet::new();
    for config in &configs {
        if config.id.trim().is_empty() || config.command.trim().is_empty() || !names.insert(config.id.as_str()) {
            return Err(api_error(StatusCode::BAD_REQUEST, "MCP 配置 ID 重复/为空或命令为空"));
        }
    }
    Ok(configs)
}

fn enabled_config(root: &Path, server_id: &str) -> ApiResult<McpServerConfig> {
    let config = load_mcp_server_configs(root)?.into_iter()
        .find(|config| config.id == server_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "MCP server 配置不存在"))?;
    if !config.enabled {
        return Err(api_error(StatusCode::FORBIDDEN, "MCP server 未启用"));
    }
    if config.transport != "stdio" {
        return Err(api_error(StatusCode::NOT_IMPLEMENTED, "此 MCP Host 目前仅支持 stdio"));
    }
    Ok(config)
}

#[derive(Clone, Deserialize)]
pub(super) struct McpManualContextRequest {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    chat_room_id: String,
}

#[derive(Clone)]
pub(super) struct McpManualContext {
    workspace_root: PathBuf,
    workspace_id: String,
    db_path: PathBuf,
    session_id: String,
    approval_session_id: String,
    chat_room_id: String,
}

fn current_context(request: &McpManualContextRequest) -> ApiResult<McpManualContext> {
    let session_id = request.session_id.trim();
    let chat_room_id = request.chat_room_id.trim();
    if session_id.is_empty() || chat_room_id.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "MCP 操作须指定当前会话和聊天室"));
    }
    let store = session_store().lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
    if store.active_session_id().as_deref() != Some(session_id)
        || store.active_chat_room_id().as_deref() != Some(chat_room_id)
        || !store.state.sessions.iter().any(|session| session.id == session_id)
        || !store.state.chat_rooms.iter().any(|room| room.id == chat_room_id)
    {
        return Err(api_error(StatusCode::CONFLICT, "MCP 请求的会话或聊天室已不是当前活动对象，请刷新后重试"));
    }
    drop(store);
    let workspace_root = active_workspace_path().canonicalize()
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &format!("MCP 工作区不可访问：{error}")))?;
    let workspace_id = workspace_identity(&workspace_root);
    Ok(McpManualContext {
        workspace_root,
        workspace_id,
        db_path: default_session_sqlite_path(),
        session_id: session_id.to_string(),
        approval_session_id: session_id.to_string(),
        chat_room_id: chat_room_id.to_string(),
    })
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct McpConnectionKey { workspace_root: PathBuf, server_id: String }

struct McpConnection {
    config: McpServerConfig,
    tools: Vec<runtime::ManagedMcpTool>,
    server_info: JsonValue,
    manager: tokio::sync::Mutex<McpServerManager>,
    cancelled: AtomicBool,
    cancel_signal: Notify,
}

struct McpConnectionSlot {
    generation: u64,
    connection: Option<Arc<McpConnection>>,
}

#[derive(Default)]
struct McpConnectionRegistry {
    slots: HashMap<McpConnectionKey, McpConnectionSlot>,
    next_generation: u64,
}

fn connections() -> &'static Mutex<McpConnectionRegistry> {
    static CONNECTIONS: OnceLock<Mutex<McpConnectionRegistry>> = OnceLock::new();
    CONNECTIONS.get_or_init(|| Mutex::new(McpConnectionRegistry::default()))
}

fn connection_key(ctx: &McpManualContext, server_id: &str) -> McpConnectionKey {
    McpConnectionKey { workspace_root: ctx.workspace_root.clone(), server_id: server_id.to_string() }
}

// 接纳时只冻结本工作区已显式连接、实际发现的工具代际；同名重连不会继承旧授权。
fn model_tools_for_workspace(workspace_id: &str) -> Vec<(runtime::ManagedMcpTool, u64)> {
    let Ok(root) = active_workspace_path().canonicalize() else { return Vec::new(); };
    if workspace_identity(&root) != workspace_id { return Vec::new(); }
    let Ok(configs) = load_mcp_server_configs(&root) else { return Vec::new(); };
    let Ok(registry) = connections().lock() else { return Vec::new(); };
    let mut tools = Vec::new();
    for config in configs.iter().filter(|config| config.enabled && config.transport == "stdio") {
        let key = McpConnectionKey { workspace_root: root.clone(), server_id: config.id.clone() };
        let Some(slot) = registry.slots.get(&key) else { continue; };
        let Some(connection) = slot.connection.as_ref() else { continue; };
        if connection.cancelled.load(Ordering::SeqCst) || connection.config != *config { continue; }
        if connection.manager.try_lock().is_ok_and(|mut manager| !manager.is_server_alive(&config.id)) {
            continue;
        }
        tools.extend(connection.tools.iter().cloned().map(|tool| (tool, slot.generation)));
    }
    tools
}

pub(super) fn capture_model_bindings(workspace_id: &str) -> HashMap<String, u64> {
    model_tools_for_workspace(workspace_id).into_iter()
        .map(|(tool, generation)| (tool.qualified_name, generation)).collect()
}

pub(super) fn discovered_model_tool_permission(tool_name: &str) -> Option<PermissionMode> {
    let workspace_id = workspace_identity(&active_workspace_path());
    model_tools_for_workspace(&workspace_id).iter()
        .any(|(tool, _)| tool.qualified_name == tool_name)
        .then_some(PermissionMode::DangerFullAccess)
}

pub(super) fn model_tool_definitions(
    workspace_id: &str,
    explicit_allowlist: Option<&Vec<String>>,
) -> Vec<ToolDefinition> {
    let mut names = HashSet::new();
    model_tools_for_workspace(workspace_id).into_iter()
        .filter(|(tool, _)| explicit_allowlist.map_or(true, |allowlist| allowlist.contains(&tool.qualified_name)))
        .filter(|(tool, _)| names.insert(tool.qualified_name.clone()))
        .filter_map(|(tool, _)| {
            let input_schema = tool.tool.input_schema.unwrap_or_else(|| json!({"type": "object"}));
            if input_schema.get("type").and_then(JsonValue::as_str) != Some("object") { return None; }
            Some(ToolDefinition {
                name: tool.qualified_name,
                description: Some(format!("MCP 服务 {} 的工具。{}",
                    tool.server_name,
                    tool.tool.description.unwrap_or_default().chars().take(500).collect::<String>())),
                input_schema,
            })
        })
        .take(128)
        .collect()
}

fn cancel_connection(connection: &McpConnection) {
    connection.cancelled.store(true, Ordering::SeqCst);
    // 单连接的 manager 锁只允许一个实际在途请求；notify_one 保留 permit，
    // 覆盖状态检查与 select 注册之间的断开竞态。其余等锁请求随后看到 cancelled。
    connection.cancel_signal.notify_one();
}

fn close_stale_workspaces(root: &Path) {
    if let Ok(mut registry) = connections().lock() {
        let keys = registry.slots.keys().filter(|key| key.workspace_root != root).cloned().collect::<Vec<_>>();
        for key in keys {
            if let Some(slot) = registry.slots.remove(&key) {
                if let Some(connection) = slot.connection { cancel_connection(&connection); }
            }
        }
    }
}

pub(super) fn invalidate_workspace() {
    let root = active_workspace_path().canonicalize().unwrap_or_else(|_| active_workspace_path());
    close_stale_workspaces(&root);
}

fn reserve_connection(key: &McpConnectionKey) -> Result<u64, String> {
    let mut registry = connections().lock().map_err(|_| "MCP 连接表不可用".to_string())?;
    registry.next_generation = registry.next_generation.saturating_add(1);
    let generation = registry.next_generation;
    let slot = registry.slots.entry(key.clone()).or_insert(McpConnectionSlot { generation, connection: None });
    if let Some(previous) = slot.connection.take() { cancel_connection(&previous); }
    slot.generation = generation;
    Ok(generation)
}

fn install_connection(key: McpConnectionKey, generation: u64, connection: Arc<McpConnection>) -> Result<(), String> {
    let mut registry = connections().lock().map_err(|_| "MCP 连接表不可用".to_string())?;
    if registry.slots.get(&key).is_none_or(|slot| slot.generation != generation) {
        return Err("MCP 连接已被断开或新连接取代".to_string());
    }
    let mut names = HashSet::new();
    for tool in &connection.tools {
        if !names.insert(tool.qualified_name.as_str()) {
            return Err("MCP 工具名归一后在同一服务内重复，未建立连接".to_string());
        }
    }
    for (other_key, slot) in &registry.slots {
        if other_key.workspace_root != key.workspace_root || other_key.server_id == key.server_id { continue; }
        if let Some(other) = slot.connection.as_ref().filter(|other| !other.cancelled.load(Ordering::SeqCst)) {
            if other.tools.iter().any(|tool| names.contains(tool.qualified_name.as_str())) {
                return Err("MCP 工具名归一后与当前工作区另一服务重复，未建立连接".to_string());
            }
        }
    }
    let slot = registry.slots.get_mut(&key).expect("上方已检查连接代际");
    slot.connection = Some(connection);
    Ok(())
}

fn current_connection(key: &McpConnectionKey) -> Option<Arc<McpConnection>> {
    connections().lock().ok()?.slots.get(key)?.connection.clone()
}

fn current_connection_with_generation(key: &McpConnectionKey) -> Option<(Arc<McpConnection>, u64)> {
    let registry = connections().lock().ok()?;
    let slot = registry.slots.get(key)?;
    Some((slot.connection.clone()?, slot.generation))
}

fn remove_connection(key: &McpConnectionKey) -> Option<Arc<McpConnection>> {
    let mut registry = connections().lock().ok()?;
    registry.next_generation = registry.next_generation.saturating_add(1);
    let generation = registry.next_generation;
    let slot = registry.slots.entry(key.clone()).or_insert(McpConnectionSlot { generation, connection: None });
    slot.generation = generation;
    let connection = slot.connection.take();
    if let Some(connection) = &connection { cancel_connection(connection); }
    connection
}

fn grant_for(ctx: &McpManualContext, tool_name: &str) -> SessionGrantView {
    let room = room_permission_grant_view_for_path(&ctx.db_path, Some(&ctx.chat_room_id));
    if room.session_authorized { room } else {
        session_grant_view_for(&ctx.workspace_id, Some(&ctx.session_id), tool_name)
    }
}

fn new_invoke(ctx: &McpManualContext, tool_name: String, input: JsonValue) -> ToolInvoke {
    static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1);
    ToolInvoke {
        call_id: format!("mcp-{}-{}", unix_timestamp_millis(), NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed)),
        tool_name,
        input,
        caller: ToolCaller::WebUi,
        workspace_id: ctx.workspace_id.clone(),
        session_id: Some(ctx.session_id.clone()),
        user_authorized: false,
        user_confirmed_twice: false,
    }
}

fn denied_or_pending_outcome(invoke: &ToolInvoke, gate: runtime::PermissionGateReport) -> ToolOutcome {
    ToolOutcome {
        call_id: invoke.call_id.clone(),
        tool_name: invoke.tool_name.clone(),
        status: if gate.decision.requires_ui() { ToolOutcomeStatus::DryRunOnly } else { ToolOutcomeStatus::Rejected },
        output: JsonValue::Null,
        summary_text: format!("MCP 操作等待权限确认：{}", gate.reason),
        elapsed_ms: 0,
        permission_gate: gate,
        evidence: None,
    }
}

fn failed_outcome(invoke: &ToolInvoke, gate: runtime::PermissionGateReport, message: String) -> ToolOutcome {
    ToolOutcome {
        call_id: invoke.call_id.clone(), tool_name: invoke.tool_name.clone(),
        status: ToolOutcomeStatus::Failed, output: json!({"error": message}),
        summary_text: message, elapsed_ms: 0, permission_gate: gate, evidence: None,
    }
}

#[derive(Clone)]
pub(super) enum McpPendingAction {
    Connect { config: McpServerConfig },
    Call {
        config: McpServerConfig,
        generation: u64,
        model_context: Option<McpManualContext>,
        model_parent: Option<FrozenParentContext>,
        model_cancellation: Option<Arc<ChatTurnCancellation>>,
    },
}

impl McpPendingAction {
    pub(super) fn approval_session_id(&self) -> Option<&str> {
        match self {
            Self::Call { model_context: Some(ctx), .. } => Some(&ctx.approval_session_id),
            _ => None,
        }
    }
}

impl std::fmt::Debug for McpPendingAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect { config } => f.debug_tuple("McpConnect").field(&config.id).finish(),
            Self::Call { config, .. } => f.debug_tuple("McpCall").field(&config.id).finish(),
        }
    }
}

fn connect_gate(ctx: &McpManualContext, invoke: &ToolInvoke) -> runtime::PermissionGateReport {
    evaluate_permission(
        invoke,
        PermissionMode::DangerFullAccess,
        &[],
        &ctx.workspace_root,
        &effective_protected_rules(),
        &grant_for(ctx, &invoke.tool_name),
        active_permission_profile(),
    )
}

async fn connect_internal(ctx: &McpManualContext, config: &McpServerConfig) -> Result<JsonValue, String> {
    let key = connection_key(ctx, &config.id);
    let generation = reserve_connection(&key)?;
    let scoped = ScopedMcpServerConfig {
        scope: ConfigSource::Local,
        config: RuntimeMcpServerConfig::Stdio(McpStdioServerConfig {
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
        }),
    };
    let servers = BTreeMap::from([(config.id.clone(), scoped)]);
    let mut manager = McpServerManager::from_servers(&servers)
        .for_workspace(&ctx.workspace_root).map_err(|error| error.to_string())?;
    let tools = manager.discover_tools().await.map_err(|error| error.to_string())?;
    // 握手期间工作区或启用配置可能变化；旧结果不可登记到新上下文。
    let request = McpManualContextRequest {
        session_id: ctx.session_id.clone(), chat_room_id: ctx.chat_room_id.clone(),
    };
    let still_current = current_context(&request).map_err(|_| "MCP 握手期间活动工作区/会话已变化".to_string())?;
    let current_config = enabled_config(&still_current.workspace_root, &config.id)
        .map_err(|_| "MCP 握手期间服务配置已失效".to_string())?;
    if still_current.workspace_root != ctx.workspace_root || current_config != *config {
        return Err("MCP 握手期间工作区或服务配置已变化".to_string());
    }
    let server_info = manager.server_info(&config.id)
        .and_then(|info| serde_json::to_value(info).ok()).unwrap_or(JsonValue::Null);
    let listed = tools.iter().map(|tool| json!({
        "name": tool.raw_name,
        "description": tool.tool.description.as_deref().unwrap_or("").chars().take(160).collect::<String>(),
    })).collect::<Vec<_>>();
    let tool_count = listed.len();
    let connection = Arc::new(McpConnection {
        config: config.clone(), tools, server_info: server_info.clone(),
        manager: tokio::sync::Mutex::new(manager),
        cancelled: AtomicBool::new(false), cancel_signal: Notify::new(),
    });
    install_connection(key, generation, connection)?;
    Ok(json!({
        "connected": true, "server_id": config.id, "server_info": server_info,
        "tool_count": tool_count, "tools": listed,
    }))
}

pub(super) async fn api_mcp_servers() -> ApiResult<Json<JsonValue>> {
    let _workspace_pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let root = active_workspace_path().canonicalize()
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &format!("MCP 工作区不可访问：{error}")))?;
    close_stale_workspaces(&root);
    let configs = load_mcp_server_configs(&root)?;
    let mut servers = Vec::with_capacity(configs.len());
    for config in &configs {
        let key = McpConnectionKey { workspace_root: root.clone(), server_id: config.id.clone() };
        let existing = current_connection(&key);
        if existing.as_ref().is_some_and(|connection| !config.enabled || connection.config != *config) {
            remove_connection(&key);
        }
        let connection = existing.filter(|connection| config.enabled && connection.config == *config);
        let connected = if let Some(connection) = &connection {
            let mut manager = connection.manager.lock().await;
            !connection.cancelled.load(Ordering::SeqCst) && manager.is_server_alive(&config.id)
        } else { false };
        if !connected && connection.is_some() { remove_connection(&key); }
        servers.push(json!({
            "id": config.id, "name": config.name, "category": config.category,
            "command": "已配置（启动参数隐藏）",
            "transport": config.transport, "enabled": config.enabled,
            "supported": config.transport == "stdio", "notes": config.notes,
            "connected": connected,
            "tool_count": if connected { connection.as_ref().map_or(0, |connection| connection.tools.len()) } else { 0 },
        }));
    }
    Ok(Json(json!({
        "servers": servers,
        "config_path": mcp_servers_config_path(&root).display().to_string(),
    })))
}

pub(super) async fn api_mcp_server_connect(
    AxumPath(server_id): AxumPath<String>,
    Json(request): Json<McpManualContextRequest>,
) -> ApiResult<Json<JsonValue>> {
    let _workspace_pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let ctx = current_context(&request)?;
    close_stale_workspaces(&ctx.workspace_root);
    let config = enabled_config(&ctx.workspace_root, &server_id)?;
    let invoke = new_invoke(&ctx, format!("mcp-connect:{}", server_id), json!({"server_id": server_id}));
    let gate = connect_gate(&ctx, &invoke);
    if !gate.decision.is_allowed() {
        let outcome = denied_or_pending_outcome(&invoke, gate);
        if outcome.permission_gate.decision.requires_ui() {
            enqueue_pending_approval_for_room_with_mcp(
                &invoke.call_id, &invoke, &outcome.permission_gate, Some(&ctx.chat_room_id),
                Some(McpPendingAction::Connect { config }),
            );
        }
        append_tool_audit_record(&invoke, &outcome);
        return Ok(Json(json!({
            "connected": false, "server_id": server_id,
            "pending_call_id": outcome.permission_gate.decision.requires_ui().then_some(&invoke.call_id),
            "outcome": outcome,
        })));
    }
    let started = Instant::now();
    let result = match connect_internal(&ctx, &config).await {
        Ok(result) => result,
        Err(_) => {
            let outcome = failed_outcome(
                &invoke, gate, "MCP 连接失败，请检查本地服务配置与诊断日志".to_string(),
            );
            append_tool_audit_record(&invoke, &outcome);
            return Ok(Json(json!({
                "connected": false, "server_id": server_id, "outcome": outcome,
            })));
        }
    };
    let outcome = ToolOutcome {
        call_id: invoke.call_id.clone(), tool_name: invoke.tool_name.clone(),
        status: ToolOutcomeStatus::Ok, output: result.clone(),
        summary_text: format!("MCP server '{}' 已连接", server_id),
        elapsed_ms: elapsed_millis(started), permission_gate: gate, evidence: None,
    };
    append_tool_audit_record(&invoke, &outcome);
    Ok(Json(result))
}

pub(super) async fn api_mcp_server_disconnect(
    AxumPath(server_id): AxumPath<String>,
    Json(request): Json<McpManualContextRequest>,
) -> ApiResult<Json<JsonValue>> {
    let _workspace_pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let ctx = current_context(&request)?;
    let key = connection_key(&ctx, &server_id);
    let previous = remove_connection(&key);
    let Some(connection) = previous else {
        return Ok(Json(json!({"disconnected": false, "server_id": server_id, "error": "未连接"})));
    };
    // 移除句柄并发出取消信号；在途请求的 future 被丢弃后由进程守卫收回子树。
    drop(connection);
    Ok(Json(json!({"disconnected": true, "server_id": server_id})))
}

#[derive(Deserialize)]
pub(super) struct McpCallRequest {
    #[serde(flatten)]
    context: McpManualContextRequest,
    server_id: String,
    tool: String,
    #[serde(default)]
    arguments: JsonValue,
}

async fn call_internal(
    ctx: &McpManualContext,
    config: &McpServerConfig,
    invoke: ToolInvoke,
    expected_generation: Option<u64>,
) -> ApiResult<ToolOutcome> {
    let key = connection_key(ctx, &config.id);
    let (connection, generation) = current_connection_with_generation(&key)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "MCP server 未连接，请先连接"))?;
    if expected_generation.is_some_and(|expected| expected != generation) {
        return Err(api_error(StatusCode::CONFLICT, "MCP 服务已重新连接，旧模型/审批调用未执行"));
    }
    if connection.cancelled.load(Ordering::SeqCst) || connection.config != *config {
        return Err(api_error(StatusCode::CONFLICT, "MCP 连接已断开或配置已变化，请重新连接"));
    }
    let mut manager = connection.manager.lock().await;
    if connection.cancelled.load(Ordering::SeqCst)
        || !manager.is_server_alive(&config.id)
        || !manager.has_discovered_tool(&invoke.tool_name)
    {
        return Err(api_error(StatusCode::CONFLICT, "MCP 工具未在当前工作区的已启用服务中发现，请重新连接"));
    }
    let grant = grant_for(ctx, &invoke.tool_name);
    let protected_rules = effective_protected_rules();
    let call = manager.call_tool_through_runtime_with_profile(
        invoke,
        &ctx.workspace_root,
        PermissionMode::DangerFullAccess,
        vec![],
        &protected_rules,
        grant,
        active_permission_profile(),
    );
    tokio::select! {
        biased;
        () = connection.cancel_signal.notified() => Err(api_error(StatusCode::CONFLICT, "MCP 连接已断开，在途请求已隔离")),
        outcome = call => {
            if connection.cancelled.load(Ordering::SeqCst) {
                Err(api_error(StatusCode::CONFLICT, "MCP 连接已断开，旧结果已隔离"))
            } else { Ok(outcome) }
        },
    }
}

pub(super) async fn api_mcp_call(Json(request): Json<McpCallRequest>) -> ApiResult<Json<JsonValue>> {
    let _workspace_pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let ctx = current_context(&request.context)?;
    close_stale_workspaces(&ctx.workspace_root);
    let config = enabled_config(&ctx.workspace_root, &request.server_id)?;
    let tool = request.tool.trim();
    if tool.is_empty() { return Err(api_error(StatusCode::BAD_REQUEST, "MCP 工具名不能为空")); }
    let qualified = mcp_tool_name(&request.server_id, tool);
    let arguments = if request.arguments.is_null() { json!({}) } else { request.arguments };
    let invoke = new_invoke(&ctx, qualified, arguments);
    let generation = current_connection_with_generation(&connection_key(&ctx, &config.id))
        .map(|(_, generation)| generation)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "MCP server 未连接，请先连接"))?;
    let started = Instant::now();
    let outcome = call_internal(&ctx, &config, invoke.clone(), Some(generation)).await?;
    if matches!(outcome.status, ToolOutcomeStatus::DryRunOnly) && outcome.permission_gate.decision.requires_ui() {
        enqueue_pending_approval_for_room_with_mcp(
            &invoke.call_id, &invoke, &outcome.permission_gate, Some(&ctx.chat_room_id),
            Some(McpPendingAction::Call {
                config, generation, model_context: None,
                model_parent: None, model_cancellation: None,
            }),
        );
    }
    append_tool_audit_record(&invoke, &outcome);
    emit_pet_event_for_tool_outcome(&invoke.tool_name, &outcome);
    Ok(Json(json!({
        "server_id": request.server_id, "tool": tool,
        "result": outcome.output.clone(), "outcome": outcome,
        "pending_call_id": outcome.permission_gate.decision.requires_ui().then_some(&invoke.call_id),
        "elapsed_ms": elapsed_millis(started),
    })))
}

pub(super) async fn call_from_model(
    name: &str,
    input: &JsonValue,
    session_id: Option<&str>,
    chat_room_id: Option<&str>,
    execution_call_id: Option<&str>,
    turn_id: Option<&str>,
    parent: Option<&FrozenParentContext>,
) -> ApiResult<ToolOutcome> {
    let (Some(parent), Some(session_id), Some(chat_room_id), Some(call_id), Some(turn_id)) =
        (parent, session_id, chat_room_id, execution_call_id, turn_id)
    else {
        return Err(api_error(StatusCode::CONFLICT, "MCP 模型调用缺少真实父运行/会话/聊天室/工具身份"));
    };
    if !parent.host_model_snapshots.get(session_id)
            .is_some_and(|snapshot| snapshot.agent_session_id() == session_id)
        || parent.room_id.as_deref() != Some(chat_room_id)
        || parent.runtime_db_path.is_none()
    {
        return Err(api_error(StatusCode::CONFLICT, "MCP 模型调用与接纳时父运行身份不一致"));
    }
    let Some(expected_generation) = parent.mcp_bindings.get(name).copied() else {
        return Err(api_error(StatusCode::FORBIDDEN, "MCP 工具未在父运行接纳时绑定，调用未执行"));
    };
    let _workspace_pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let root = active_workspace_path().canonicalize()
        .map_err(|_| api_error(StatusCode::CONFLICT, "MCP 模型调用的工作区已不可用"))?;
    if workspace_identity(&root) != parent.workspace_id.as_str() {
        return Err(api_error(StatusCode::CONFLICT, "MCP 父运行工作区已变化，旧工具调用未执行"));
    }
    let ctx = McpManualContext {
        workspace_root: root,
        workspace_id: parent.workspace_id.as_str().to_string(),
        db_path: parent.runtime_db_path.clone().expect("上方已检查父运行数据库"),
        session_id: session_id.to_string(),
        approval_session_id: parent.session_id.as_deref().unwrap_or(session_id).to_string(),
        chat_room_id: chat_room_id.to_string(),
    };
    let server_id = model_tools_for_workspace(&ctx.workspace_id).into_iter()
        .find(|(tool, generation)| tool.qualified_name == name && *generation == expected_generation)
        .map(|(tool, _)| tool.server_name)
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "MCP 父运行绑定的工具已失效"))?;
    let config = enabled_config(&ctx.workspace_root, &server_id)?;
    let key = connection_key(&ctx, &server_id);
    if current_connection_with_generation(&key)
        .is_none_or(|(connection, generation)| generation != expected_generation
            || connection.cancelled.load(Ordering::SeqCst)
            || connection.config != config
            || !connection.tools.iter().any(|tool| tool.qualified_name == name))
    {
        return Err(api_error(StatusCode::CONFLICT, "MCP 连接或工具与父运行绑定不符，旧调用未执行"));
    }
    let invoke = ToolInvoke {
        call_id: call_id.to_string(), tool_name: name.to_string(),
        input: if input.is_null() { json!({}) } else { input.clone() },
        caller: ToolCaller::Llm,
        workspace_id: ctx.workspace_id.clone(), session_id: Some(ctx.session_id.clone()),
        user_authorized: false, user_confirmed_twice: false,
    };
    let cancellation = tool_turn_cancellation_registry().lock().ok()
        .and_then(|entries| entries.get(turn_id).cloned())
        .or_else(|| CHAT_CANCELLATION.try_with(Arc::clone).ok())
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "MCP 模型调用缺少父运行取消信号"))?;
    if cancellation.is_requested() || parent.root_budget.as_ref().is_none_or(|budget| budget.is_expired())
        || parent.goal_phase.as_ref().is_some_and(|goal| goal.validate_live().is_err())
    {
        return Err(api_error(StatusCode::CONFLICT, "MCP 模型父运行已停止、过期或阶段归属失效"));
    }
    let outcome = tokio::select! {
        biased;
        () = cancellation.cancelled() => {
            return Err(api_error(StatusCode::CONFLICT, "父运行已停止，MCP 在途请求已隔离"));
        },
        result = call_internal(&ctx, &config, invoke.clone(), Some(expected_generation)) => result?,
    };
    if outcome.status == ToolOutcomeStatus::DryRunOnly && outcome.permission_gate.decision.requires_ui() {
        enqueue_pending_approval_for_room_with_mcp(
            &invoke.call_id, &invoke, &outcome.permission_gate, Some(chat_room_id),
            Some(McpPendingAction::Call {
                config, generation: expected_generation, model_context: Some(ctx.clone()),
                model_parent: Some(parent.clone()), model_cancellation: Some(cancellation.clone()),
            }),
        );
    }
    append_tool_audit_record(&invoke, &outcome);
    emit_pet_event_for_tool_outcome(name, &outcome);
    Ok(outcome)
}

pub(super) async fn execute_approved_pending_record(
    record: &PendingApprovalRecord,
    confirmed_twice: bool,
) -> ToolOutcome {
    let _workspace_pin = match workspace_activity::pin_workspace() {
        Ok(pin) => pin,
        Err(message) => return failed_outcome(
            &record.invoke,
            runtime::PermissionGateReport::deny(PermissionMode::DangerFullAccess, "工程正在切换"),
            message,
        ),
    };
    let Some(action) = record.mcp_action.clone() else {
        return failed_outcome(
            &record.invoke,
            runtime::PermissionGateReport::deny(PermissionMode::DangerFullAccess, "MCP 审批动作缺失"),
            "MCP 审批动作缺失".to_string(),
        );
    };
    let request = McpManualContextRequest {
        session_id: record.session_id.clone().unwrap_or_default(),
        chat_room_id: record.chat_room_id.clone().unwrap_or_default(),
    };
    let resolved = match &action {
        McpPendingAction::Call { model_context: Some(ctx), .. } => {
            let active_root = active_workspace_path().canonicalize().ok();
            (active_root.as_ref() == Some(&ctx.workspace_root)
                && record.session_id.as_deref() == Some(ctx.approval_session_id.as_str())
                && record.invoke.session_id.as_deref() == Some(ctx.session_id.as_str())
                && record.chat_room_id.as_deref() == Some(ctx.chat_room_id.as_str()))
                .then_some(ctx.clone())
        }
        _ => current_context(&request).ok(),
    };
    let ctx = match resolved {
        Some(ctx) if ctx.workspace_root == record.workspace_root.canonicalize().unwrap_or_default()
            && ctx.workspace_id == record.workspace_id => ctx,
        _ => return failed_outcome(
            &record.invoke,
            runtime::PermissionGateReport::deny(PermissionMode::DangerFullAccess, "MCP 审批的工作区/会话已变化"),
            "MCP 审批的工作区/会话已变化".to_string(),
        ),
    };
    let config = match &action {
        McpPendingAction::Connect { config } | McpPendingAction::Call { config, .. } => config.clone(),
    };
    if !matches!(enabled_config(&ctx.workspace_root, &config.id), Ok(ref current) if current == &config) {
        return failed_outcome(
            &record.invoke,
            runtime::PermissionGateReport::deny(PermissionMode::DangerFullAccess, "MCP 服务配置已变化"),
            "MCP 服务配置已变化，请重新发起".to_string(),
        );
    }
    let mut invoke = record.invoke.clone();
    invoke.user_authorized = true;
    invoke.user_confirmed_twice = confirmed_twice;
    match action {
        McpPendingAction::Connect { .. } => {
            let gate = connect_gate(&ctx, &invoke);
            if !gate.decision.is_allowed() {
                return denied_or_pending_outcome(&invoke, gate);
            }
            let started = Instant::now();
            match connect_internal(&ctx, &config).await {
                Ok(result) => ToolOutcome {
                    call_id: invoke.call_id.clone(), tool_name: invoke.tool_name.clone(),
                    status: ToolOutcomeStatus::Ok, output: result,
                    summary_text: format!("MCP server '{}' 已连接", config.id),
                    elapsed_ms: elapsed_millis(started), permission_gate: gate, evidence: None,
                },
                Err(error) => failed_outcome(&invoke, gate, error),
            }
        }
        McpPendingAction::Call { generation, model_parent, model_cancellation, .. } => {
            let deny = |message: &str| failed_outcome(
                &invoke,
                runtime::PermissionGateReport::deny(PermissionMode::DangerFullAccess, message),
                message.to_string(),
            );
            if let Some(parent) = model_parent {
                let Some(cancellation) = model_cancellation else {
                    return deny("MCP 审批缺少模型父运行取消身份");
                };
                let Some(budget) = parent.root_budget.as_ref() else {
                    return deny("MCP 审批缺少模型父运行时限");
                };
                if cancellation.is_requested() || budget.is_expired()
                    || parent.workspace_id.as_str() != ctx.workspace_id
                    || parent.room_id.as_deref() != Some(ctx.chat_room_id.as_str())
                    || !parent.host_model_snapshots.get(&ctx.session_id)
                        .is_some_and(|snapshot| snapshot.agent_session_id() == ctx.session_id)
                    || parent.goal_phase.as_ref().is_some_and(|goal| goal.validate_live().is_err())
                {
                    return deny("MCP 模型父运行已停止、过期或归属失效，旧审批未执行");
                }
                let call = async {
                    tokio::select! {
                        result = call_internal(&ctx, &config, invoke.clone(), Some(generation)) => result,
                        () = cancellation.cancelled() => Err(api_error(StatusCode::CONFLICT, "MCP 模型父运行已取消")),
                    }
                };
                match budget.run_cancellable(call, || { cancellation.request_timeout(); }).await {
                    Ok(Ok(outcome)) => outcome,
                    _ => deny("MCP 模型父运行或连接已失效，旧审批未执行"),
                }
            } else {
                match call_internal(&ctx, &config, invoke.clone(), Some(generation)).await {
                    Ok(outcome) => outcome,
                    Err(_) => deny("MCP 连接或工具已失效，请重新连接"),
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{config_test_guard, context_test_agent, DevOpenPermissionsTestGuard};
    use std::collections::BTreeSet;

    struct McpHostTestScope {
        workspace: PathBuf,
        previous_workspace: PathBuf,
        previous_config: WorkspaceConfig,
        previous_db: Option<PathBuf>,
    }

    impl McpHostTestScope {
        fn install(workspace: &Path, db: PathBuf) -> Self {
            let previous_workspace = {
                let mut state = workspace_state().lock().unwrap();
                std::mem::replace(&mut state.current, workspace.to_path_buf())
            };
            let mut config = WorkspaceConfig::default();
            config.model.enable_llm_tools = true;
            config.model.llm_tool_exposure = Some("all".into());
            let previous_config = std::mem::replace(&mut *workspace_config().lock().unwrap(), config);
            let previous_db = replace_session_db_path_override_for_test(Some(db));
            Self { workspace: workspace.to_path_buf(), previous_workspace, previous_config, previous_db }
        }
    }

    impl Drop for McpHostTestScope {
        fn drop(&mut self) {
            let key = McpConnectionKey {
                workspace_root: self.workspace.clone(), server_id: "official".into(),
            };
            remove_connection(&key);
            clear_pending_approvals_for_test();
            replace_session_db_path_override_for_test(self.previous_db.take());
            *workspace_config().lock().unwrap() = self.previous_config.clone();
            workspace_state().lock().unwrap().current = self.previous_workspace.clone();
        }
    }

    /// 真实官方 SDK 服务的 Web 模型派发验证，需要事先在本机提供离线安装的入口。
    /// 该测试只调用无副作用 echo，隔离工作区与会话库，不触碰已安装版或用户数据。
    #[tokio::test]
    #[ignore = "需要 MCP_OFFICIAL_SERVER_ENTRY 指向本机已安装的官方 SDK Everything Server"]
    async fn official_sdk_model_dispatch_two_targets_child_and_stale_approval() {
        let _serial = config_test_guard();
        let entry = PathBuf::from(std::env::var("MCP_OFFICIAL_SERVER_ENTRY").expect("官方 SDK 入口"));
        assert!(entry.is_file());
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let db = root.join("web-sessions.sqlite3");
        let _scope = McpHostTestScope::install(&root, db.clone());
        let _dev_open = DevOpenPermissionsTestGuard::enable();
        std::fs::create_dir_all(root.join(".coolzhu")).unwrap();
        let config = McpServerConfig {
            id: "official".into(), name: "官方 SDK Everything".into(), category: "test".into(),
            command: "node".into(), args: vec![entry.display().to_string(), "stdio".into()],
            env: BTreeMap::new(), transport: "stdio".into(), enabled: true, notes: String::new(),
        };
        std::fs::write(mcp_servers_config_path(&root), serde_json::to_vec(&vec![config.clone()]).unwrap()).unwrap();
        let scoped = ScopedMcpServerConfig {
            scope: ConfigSource::Local,
            config: RuntimeMcpServerConfig::Stdio(McpStdioServerConfig {
                command: config.command.clone(), args: config.args.clone(), env: config.env.clone(),
            }),
        };
        let mut manager = McpServerManager::from_servers(&BTreeMap::from([(config.id.clone(), scoped)]))
            .for_workspace(&root).unwrap();
        let tools = manager.discover_tools().await.expect("官方 SDK 握手与发现");
        let echo = mcp_tool_name("official", "echo");
        assert!(tools.iter().any(|tool| tool.qualified_name == echo));
        let key = McpConnectionKey { workspace_root: root.clone(), server_id: config.id.clone() };
        let generation = reserve_connection(&key).unwrap();
        install_connection(key.clone(), generation, Arc::new(McpConnection {
            config: config.clone(), tools, server_info: JsonValue::Null,
            manager: tokio::sync::Mutex::new(manager), cancelled: AtomicBool::new(false),
            cancel_signal: Notify::new(),
        })).unwrap();

        let workspace_id = workspace_identity(&root);
        let room = "mcp-isolated-room";
        let management_session = "mcp-management-a";
        let target_session = "mcp-target-b";
        let turn = "mcp-official-turn";
        create_chat_runtime_run_sqlite(&db, "mcp-official-run", "mcp-claim", &workspace_id,
            Some(management_session), room, turn).unwrap();
        start_chat_runtime_run_sqlite(&db, "mcp-official-run", "mcp-claim").unwrap();
        let mut parent = FrozenParentContext::new("mcp-model-test", &workspace_id,
            Some(room), Some(management_session), Some(turn), Some("mcp-official-run")).unwrap();
        parent.runtime_db_path = Some(db.clone());
        parent.root_budget = Some(root_execution_budget::RootExecutionBudget::establish(60_000));
        assert_eq!(parent.mcp_bindings.get(&echo), Some(&generation));
        let mut agent = context_test_agent();
        agent.id = target_session.into();
        agent.provider = "Custom".into();
        agent.model = "local-mcp-test".into();
        agent.base_url = Some("http://127.0.0.1:1/v1".into());
        agent.memory_beads.clear();
        let snapshot = Arc::new(host_child_agent::HostModelSnapshot::capture(&agent, Some(room), &parent).unwrap());
        assert!(snapshot.parent_definitions.iter().any(|tool| tool.name == echo));
        parent.host_model_snapshots = Arc::new(HashMap::from([(target_session.to_string(), Arc::clone(&snapshot))]));
        let cancellation = Arc::new(ChatTurnCancellation::new());
        let _cancellation_scope = ToolTurnCancellationScope::install(turn, Arc::clone(&cancellation));
        let parent_scope = host_child_agent::HostToolScope::Parent(Arc::clone(&snapshot));
        let result = tool_invocation_identity::scope(format!("{turn}/parent"),
            run_model_tool_dispatch_for_session_with_identity(&echo,
                &json!({"message":"web-model-two-targets"}), Some(target_session), Some("mcp-tool-parent"),
                Some(turn), Some(room), Some(&parent), Some(&parent_scope))).await.unwrap();
        assert_eq!(result.status, "ok", "{}", result.notes.join("; "));
        assert!(result.tool_result_text.as_deref().unwrap_or_default().contains("Echo: web-model-two-targets"));

        let child = Arc::new(host_child_agent::HostChildScope {
            parent: parent.clone(), parent_call_id: "mcp-child-parent".into(),
            parent_turn_id: turn.into(), tool_session_id: target_session.into(),
            usage_session_id: management_session.into(), room_id: room.into(),
            allowed_tools: BTreeSet::from([echo.clone()]),
            definitions: snapshot.parent_definitions.iter().filter(|tool| tool.name == echo).cloned().collect(),
            permission: snapshot.permission, workspace_root: root.clone(), usage_db_path: db.clone(),
            client: snapshot.parent_model.client.clone(), request: snapshot.parent_model.request.clone(),
            cancellation: Arc::clone(&cancellation),
        });
        let child_scope = host_child_agent::HostToolScope::Child(child);
        let child_result = tool_invocation_identity::scope(format!("{turn}/child"),
            run_model_tool_dispatch_for_session_with_identity(&echo,
                &json!({"message":"web-child-two-targets"}), Some(target_session), Some("mcp-tool-child"),
                Some(turn), Some(room), Some(&parent), Some(&child_scope))).await.unwrap();
        assert_eq!(child_result.status, "ok", "{}", child_result.notes.join("; "));
        assert!(child_result.tool_result_text.as_deref().unwrap_or_default().contains("Echo: web-child-two-targets"));

        drop(_dev_open);
        let approval_call_id = "mcp-model-needs-approval";
        let pending = call_from_model(&echo, &json!({"message":"must-not-execute-after-cancel"}),
            Some(target_session), Some(room), Some(approval_call_id), Some(turn), Some(&parent))
            .await.unwrap();
        assert_eq!(pending.status, ToolOutcomeStatus::DryRunOnly, "未授权模型调用必须进入现有审批门禁");
        let record = pending_approvals().lock().unwrap().get(approval_call_id).cloned()
            .expect("模型工具应生成真实待审批记录");
        assert_eq!(record.session_id.as_deref(), Some(management_session));
        assert_eq!(record.invoke.session_id.as_deref(), Some(target_session));
        assert!(matches!(record.mcp_action.as_ref(), Some(McpPendingAction::Call { generation: frozen, .. }) if *frozen == generation));
        cancellation.request();
        let replay = execute_approved_pending_record(&record, true).await;
        assert_eq!(replay.status, ToolOutcomeStatus::Failed, "取消后审批不得执行");
        assert!(!replay.output.to_string().contains("Echo:"));
    }
}
