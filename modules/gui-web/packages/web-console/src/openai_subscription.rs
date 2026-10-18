//! OpenAI 订阅通过官方 Codex app-server 协议接入，不提取/转发 OAuth token，不冒充 HTTP API。
//! 首批仅对话。CLI 独立目录、只读沙箱、禁用原生 shell/联网/多 Agent；宿主工具未开放。
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

static RUNNING: AtomicUsize = AtomicUsize::new(0);
static ACCOUNT_CHANGE: AsyncMutex<()> = AsyncMutex::const_new(());
fn fail(message: impl Into<String>) -> api::ApiError {
    api::ApiError::UnsupportedCapability {
        capability: message.into(),
    }
}
fn prepare_account_directories(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let cwd = root.join("conversation");
    let auth = root.join("auth");
    // 官方 CLI 要求自定义 CODEX_HOME 已存在，首次启动前一并创建。
    std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&auth).map_err(|e| e.to_string())?;
    Ok((cwd, auth))
}
struct Rpc {
    child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    next: u64,
    login: Option<String>,
    pending: std::collections::VecDeque<JsonValue>,
}
impl Rpc {
    async fn spawn() -> Result<Self, String> {
        let root = workspace_projects::state_root()?.join("openai-subscription");
        let (cwd, auth) = prepare_account_directories(&root)?;
        let binary = env::var_os("COOLZHU_CODEX_CLI")
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .map(|p| p.join("Programs/OpenAI/Codex/bin/codex.exe"))
                    .filter(|p| p.is_file())
            })
            .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "codex.exe" } else { "codex" }));
        let mut command = tokio::process::Command::new(binary);
        command
            .args([
                "app-server",
                "-c",
                "features.shell_tool=false",
                "-c",
                "features.multi_agent=false",
                "-c",
                "features.multi_agent_v2=false",
                "-c",
                "web_search=\"disabled\"",
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "sandbox_mode=\"read-only\"",
            ])
            .env("CODEX_HOME", auth)
            .env_remove("OPENAI_API_KEY")
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|_| "未找到或无法启动官方 Codex CLI，请安装 CLI 后重试。".to_string())?;
        let input = child.stdin.take().ok_or("CLI 输入不可用")?;
        let output = BufReader::new(child.stdout.take().ok_or("CLI 输出不可用")?).lines();
        let mut rpc = Self {
            child,
            input,
            output,
            next: 1,
            login: None,
            pending: Default::default(),
        };
        rpc.call("initialize",json!({"clientInfo":{"name":"coolzhuagent","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}})).await?;
        rpc.write(json!({"method":"initialized"})).await?;
        Ok(rpc)
    }
    async fn write(&mut self, value: JsonValue) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| "CLI 消息无法编码")?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|_| "CLI 连接已关闭".to_string())
    }
    async fn read(&mut self) -> Result<JsonValue, String> {
        if let Some(value) = self.pending.pop_front() {
            return Ok(value);
        }
        self.read_wire().await
    }
    async fn read_wire(&mut self) -> Result<JsonValue, String> {
        loop {
            let line = self
                .output
                .next_line()
                .await
                .map_err(|_| "CLI 输出读取失败")?
                .ok_or("CLI 连接结束，未收到完成回执")?;
            if line.len() > 8 * 1024 * 1024 {
                return Err("CLI 单条消息超过大小上限".into());
            }
            let value: JsonValue =
                serde_json::from_str(&line).map_err(|_| "CLI 返回无效协议消息")?;
            if value.get("method").is_some() && value.get("id").is_some() {
                self.write(json!({"id":value["id"],"error":{"code":-32601,"message":"此订阅连接仅开放对话；宿主未授权原生工具或审批。"}})).await?;
                continue;
            }
            return Ok(value);
        }
    }
    async fn call(&mut self, method: &str, params: JsonValue) -> Result<JsonValue, String> {
        let id = self.next;
        self.next += 1;
        self.write(json!({"id":id,"method":method,"params":params}))
            .await?;
        tokio::time::timeout(Duration::from_secs(35), async {
            loop {
                let value = self.read_wire().await?;
                if value["id"].as_u64() != Some(id) {
                    if self.pending.len() >= 2048 {
                        return Err("CLI 通知队列已满，未自动重发。".into());
                    }
                    self.pending.push_back(value);
                    continue;
                }
                if value.get("error").is_some() {
                    return Err(format!(
                        "官方 CLI 拒绝 {method}：{}",
                        value["error"]["message"].as_str().unwrap_or("协议错误")
                    ));
                }
                return Ok(value["result"].clone());
            }
        })
        .await
        .map_err(|_| format!("官方 CLI {method} 超时；本次操作未自动重试"))?
    }
}
fn account() -> &'static AsyncMutex<Option<Rpc>> {
    static VALUE: OnceLock<AsyncMutex<Option<Rpc>>> = OnceLock::new();
    VALUE.get_or_init(|| AsyncMutex::new(None))
}
async fn account_call(method: &str, params: JsonValue) -> Result<JsonValue, String> {
    let mut guard = account().lock().await;
    if guard
        .as_mut()
        .is_some_and(|rpc| rpc.child.try_wait().ok().flatten().is_some())
    {
        *guard = None;
    }
    if guard.is_none() {
        *guard = Some(Rpc::spawn().await?);
    }
    let result = guard.as_mut().unwrap().call(method, params).await;
    if result.is_err() {
        *guard = None;
    }
    result
}
pub(super) fn routes() -> Router {
    Router::new()
        .route("/api/backends/openai/status", get(status))
        .route("/api/backends/openai/login", post(login))
        .route("/api/backends/openai/cancel", post(cancel))
        .route("/api/backends/openai/logout", post(logout))
        .route("/api/backends/openai/models", get(models).post(models))
}
fn error(reason: String) -> ApiError {
    api_error(StatusCode::BAD_GATEWAY, &reason)
}
async fn status() -> ApiResult<Json<JsonValue>> {
    let data = account_call("account/read", json!({"refreshToken":false}))
        .await
        .map_err(error)?;
    let logged = data["account"]["type"] == "chatgpt";
    let mut guard = account().lock().await;
    if logged {
        if let Some(rpc) = guard.as_mut() {
            rpc.login = None;
        }
    }
    let pending = guard.as_ref().is_some_and(|rpc| rpc.login.is_some());
    drop(guard);
    let limits = if logged {
        account_call("account/rateLimits/read", json!({}))
            .await
            .ok()
    } else {
        None
    };
    Ok(Json(
        json!({"logged_in":logged,"active":pending,"account":data["account"],"limits":limits,
        "message":if logged{"ChatGPT 账号已登录 · 官方 CLI 会话"}else if pending{"等待浏览器完成登录"}else{"尚未登录 ChatGPT 账号"}}),
    ))
}
async fn login() -> ApiResult<Json<JsonValue>> {
    let _account_change = ACCOUNT_CHANGE.lock().await;
    if RUNNING.load(Ordering::Acquire) > 0 {
        return Err(api_error(
            StatusCode::CONFLICT,
            "订阅会话正在生成，请结束后再切换账号。",
        ));
    }
    let result = account_call("account/login/start", json!({"type":"chatgpt"}))
        .await
        .map_err(error)?;
    let url = result["authUrl"]
        .as_str()
        .filter(|value| {
            value.starts_with("https://auth.openai.com/")
                || value.starts_with("https://auth0.openai.com/")
        })
        .ok_or_else(|| error("官方 CLI 未返回可识别的登录地址。".into()))?;
    if let Some(rpc) = account().lock().await.as_mut() {
        rpc.login = result["loginId"].as_str().map(str::to_owned);
    }
    Ok(Json(
        json!({"auth_url":url,"active":true,"message":"请在官方页面登录，完成后刷新状态。"}),
    ))
}
async fn cancel() -> ApiResult<Json<JsonValue>> {
    let _account_change = ACCOUNT_CHANGE.lock().await;
    let id = account()
        .lock()
        .await
        .as_ref()
        .and_then(|rpc| rpc.login.clone());
    if let Some(id) = id {
        account_call("account/login/cancel", json!({"loginId":id}))
            .await
            .map_err(error)?;
    }
    if let Some(rpc) = account().lock().await.as_mut() {
        rpc.login = None;
    }
    Ok(Json(json!({"active":false,"message":"登录已取消"})))
}
async fn logout() -> ApiResult<Json<JsonValue>> {
    let _account_change = ACCOUNT_CHANGE.lock().await;
    if RUNNING.load(Ordering::Acquire) > 0 {
        return Err(api_error(
            StatusCode::CONFLICT,
            "订阅会话正在生成，请结束后再退出账号。",
        ));
    }
    account_call("account/logout", json!({}))
        .await
        .map_err(error)?;
    Ok(Json(
        json!({"logged_in":false,"active":false,"message":"已退出账号"}),
    ))
}
async fn catalog(rpc: &mut Rpc) -> Result<Vec<JsonValue>, String> {
    let account = rpc
        .call("account/read", json!({"refreshToken":false}))
        .await?;
    if account["account"]["type"] != "chatgpt" {
        return Err("请先登录 ChatGPT 订阅账号。".into());
    }
    let mut models = Vec::new();
    let mut cursor = JsonValue::Null;
    for _ in 0..10 {
        let result = rpc
            .call("model/list", json!({"limit":100,"cursor":cursor}))
            .await?;
        models.extend(result["data"].as_array().cloned().unwrap_or_default());
        cursor = result["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    Ok(models)
}
async fn models() -> ApiResult<Json<JsonValue>> {
    let mut guard = account().lock().await;
    if guard.is_none() {
        *guard = Some(Rpc::spawn().await.map_err(error)?);
    }
    let models = catalog(guard.as_mut().unwrap()).await.map_err(error)?;
    Ok(Json(
        json!({"models":models.into_iter().map(|m|json!({"id":m["model"],"name":m["displayName"],"family":m["displayName"],
        "supported_reasoning_efforts":m["supportedReasoningEfforts"].as_array().map(|a|a.iter().filter_map(|v|v["reasoningEffort"].as_str()).collect::<Vec<_>>()),
        "image_input":false,"tool_calling":false,"description":m["description"],"source":"official_cli_catalog"})).collect::<Vec<_>>(),
        "note":"官方 CLI 返回的模型目录；实际账号可用性以发送回执为准。当前适配器开放文本会话，工具与图片直传尚未开放。"}),
    ))
}

pub(super) fn validate_input(payload: &UpsertSessionRequest) -> ApiResult<()> {
    if payload
        .api_key_ref
        .as_deref()
        .is_some_and(|s| !s.is_empty())
        || payload.base_url.as_deref().is_some_and(|s| !s.is_empty())
        || payload.endpoint.as_deref().is_some_and(|s| !s.is_empty())
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "OpenAI 订阅使用官方 CLI 登录，不接受 API Key 或 HTTP 地址。",
        ));
    }
    if payload.model_type.as_deref().is_some_and(|v| v != "text") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "此订阅连接当前仅开放文本会话。",
        ));
    }
    Ok(())
}
pub(super) fn reasoning(model: &str, requested: &str) -> ReasoningResolutionDto {
    ReasoningResolutionDto {
        requested: requested.into(),
        effective: requested.into(),
        status: "pending".into(),
        strategy: "official_cli".into(),
        protocol: "openai_subscription".into(),
        reason: format!("{model}：发送前核对官方 CLI 目录中的模型及档位。"),
        supported_options: vec![ReasoningOptionDto {
            value: "auto".into(),
            label: "模型默认".into(),
            note: None,
        }],
    }
}

enum Item {
    Delta(String),
    End,
    Error(String),
}
pub(crate) struct TextStream {
    events: mpsc::Receiver<Item>,
    cancel: Arc<ChatTurnCancellation>,
    pub(crate) completed: bool,
    ended: bool,
}
impl TextStream {
    pub(crate) async fn next(&mut self) -> Result<Option<api::StreamEvent>, api::ApiError> {
        if self.ended {
            return Ok(None);
        }
        match self.events.recv().await {
            Some(Item::Delta(text)) => Ok(Some(api::StreamEvent::ContentBlockDelta(
                api::ContentBlockDeltaEvent {
                    index: 0,
                    delta: api::ContentBlockDelta::TextDelta { text },
                },
            ))),
            Some(Item::End) => {
                self.completed = true;
                self.ended = true;
                Ok(None)
            }
            Some(Item::Error(reason)) => {
                self.ended = true;
                Err(fail(reason))
            }
            None => {
                self.ended = true;
                Err(fail("官方 CLI 连接结束但没有完成回执；本轮未重发。"))
            }
        }
    }
}
impl Drop for TextStream {
    fn drop(&mut self) {
        if !self.ended {
            self.cancel.request();
        }
    }
}
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) async fn start(
    agent: &AgentSessionDto,
    assembly: &ContextAssembly,
    parent: Option<&FrozenParentContext>,
) -> Result<devin_acp::chat::ModelStream, api::ApiError> {
    let parent = parent
        .cloned()
        .ok_or_else(|| fail("订阅会话缺少冻结运行身份。"))?;
    let cancel = CHAT_CANCELLATION
        .try_with(Arc::clone)
        .map_err(|_| fail("订阅会话缺少取消控制。"))?;
    let gate = ACCOUNT_CHANGE.lock().await;
    RUNNING.fetch_add(1, Ordering::AcqRel);
    let active = Active;
    drop(gate);
    let mut rpc = Rpc::spawn().await.map_err(fail)?;
    let models = catalog(&mut rpc).await.map_err(fail)?;
    let model = models
        .iter()
        .find(|m| m["model"] == agent.model)
        .ok_or_else(|| fail("所选模型不在官方 CLI 当前目录中，请刷新目录。"))?;
    if agent.reasoning_effort != "auto"
        && !model["supportedReasoningEfforts"]
            .as_array()
            .is_some_and(|a| {
                a.iter()
                    .any(|v| v["reasoningEffort"] == agent.reasoning_effort)
            })
    {
        return Err(fail("当前模型未声明此思考档位。"));
    }
    let prompt = serde_json::to_string(&assembly.messages).map_err(|_| fail("无法装配订阅消息"))?;
    let root = workspace_projects::state_root()
        .map_err(fail)?
        .join("openai-subscription/conversation");
    let thread=rpc.call("thread/start",json!({"model":agent.model,"cwd":root,"ephemeral":true,"approvalPolicy":"never","sandbox":"read-only",
        "baseInstructions":assembly.system_prompt,"developerInstructions":"请根据提供的历史及最后一条用户消息回答。此连接只有文本能力，不运行工具或子 Agent，不声称已执行任何文件、命令或浏览器操作。"})).await.map_err(fail)?;
    verify_started_thread(&thread, &agent.model).map_err(fail)?;
    let thread_id = thread["thread"]["id"]
        .as_str()
        .ok_or_else(|| fail("CLI 未返回会话 ID"))?
        .to_owned();
    let effort = if agent.reasoning_effort == "auto" {
        JsonValue::Null
    } else {
        json!(agent.reasoning_effort)
    };
    if cancel.is_requested() {
        return Err(fail("本轮已停止，未发送订阅请求。"));
    }
    let started = unix_timestamp_millis();
    let result=rpc.call("turn/start",json!({"threadId":thread_id,"model":agent.model,"effort":effort,"input":[{"type":"text","text":prompt}]})).await.map_err(fail)?;
    let turn = result["turn"]["id"]
        .as_str()
        .ok_or_else(|| fail("CLI 未返回回合 ID"))?
        .to_owned();
    let remaining = parent
        .root_budget
        .as_ref()
        .map_or(Duration::from_secs(900), |b| b.remaining());
    let (sender, events) = mpsc::channel(2048);
    let token = cancel.clone();
    let model = agent.model.clone();
    let session = agent.id.clone();
    tokio::spawn(async move {
        let _active = active;
        let deadline = tokio::time::Instant::now() + remaining;
        let mut first = None;
        let mut usage = JsonValue::Null;
        let outcome = loop {
            if token.is_requested() || tokio::time::Instant::now() >= deadline {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    rpc.call(
                        "turn/interrupt",
                        json!({"threadId":thread_id,"turnId":turn}),
                    ),
                )
                .await;
                break Err("订阅回合已停止等待；远端结果未确认为完成。".to_string());
            }
            let value = tokio::select! {value=rpc.read()=>match value {Ok(v)=>v,Err(e)=>break Err(e)},_=tokio::time::sleep(Duration::from_millis(100))=>continue};
            let p = &value["params"];
            if p["threadId"] != thread_id {
                continue;
            }
            match value["method"].as_str().unwrap_or("") {
                "item/agentMessage/delta" if p["turnId"] == turn => {
                    if let Some(text) = p["delta"].as_str() {
                        first.get_or_insert(unix_timestamp_millis());
                        if sender.try_send(Item::Delta(text.into())).is_err() {
                            break Err("订阅输出消费中断，未自动重发。".into());
                        }
                    }
                }
                "thread/tokenUsage/updated" => {
                    usage = p["tokenUsage"]["last"].clone();
                }
                "turn/completed" if p["turn"]["id"] == turn => {
                    break if p["turn"]["status"] == "completed" {
                        Ok(())
                    } else {
                        Err(format!("订阅回合未完成：{}", p["turn"]["status"]))
                    };
                }
                _ => {}
            }
        };
        let _ = rpc.child.kill().await;
        record(
            &parent,
            &session,
            &model,
            &turn,
            started,
            first,
            &usage,
            outcome.is_ok(),
        );
        let _ = sender
            .send(match outcome {
                Ok(()) => Item::End,
                Err(e) => Item::Error(e),
            })
            .await;
    });
    Ok(devin_acp::chat::ModelStream::OpenAi(TextStream {
        events,
        cancel,
        completed: false,
        ended: false,
    }))
}
fn record(
    parent: &FrozenParentContext,
    session: &str,
    model: &str,
    attempt: &str,
    started: u64,
    first: Option<u64>,
    usage: &JsonValue,
    success: bool,
) {
    let result = (|| -> rusqlite::Result<()> {
        let Some(path) = parent.runtime_db_path.as_ref() else {
            return Ok(());
        };
        let c = open_session_connection(path)?;
        chat_insights::ensure_tables(&c)?;
        let input = usage["inputTokens"].as_u64();
        let output = usage["outputTokens"].as_u64();
        let cache = usage["cachedInputTokens"].as_u64();
        c.execute("INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,attempt_id,logical_request_id,turn_id,run_id,status,dispatched,usage_known_mask,model_snapshot,protocol_snapshot,started_at,first_token_at,finished_at,request_kind)
          VALUES(?1,?2,?3,?4,?5,?6,?7,0,?8,?8,?9,?10,?11,1,?12,?13,'openai_chat_completions',?4,?14,?15,'openai_subscription')",params![parent.workspace_id.as_str(),parent.room_id.as_deref().unwrap_or(""),session,started,input.unwrap_or(0),output.unwrap_or(0),cache.unwrap_or(0),format!("openai:{attempt}"),parent.public_turn_id,parent.parent_run_id,if success{"completed"}else{"remote_unknown"},u8::from(input.is_some())|(u8::from(output.is_some())<<1)|(u8::from(cache.is_some())<<2),model,first,unix_timestamp_millis()])?;
        Ok(())
    })();
    if let Err(e) = result {
        diag_log(&format!("[OPENAI] 用量记录失败：{e}"));
    }
}

fn verify_started_thread(response: &JsonValue, model: &str) -> Result<(), String> {
    if response["model"].as_str() != Some(model) {
        return Err("官方 CLI 生效模型与所选 ID 不符；未发送本轮消息，请刷新模型目录。".into());
    }
    if response["sandbox"]["type"] != "readOnly"
        || response["sandbox"]["networkAccess"]
            .as_bool()
            .unwrap_or(false)
        || response["approvalPolicy"] != "never"
    {
        return Err("官方 CLI 未确认此连接要求的只读权限，未发送本轮消息。".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn 首次连接创建独立账号目录并拒绝文件冲突() {
        let root = tempfile::tempdir().unwrap();
        let (cwd, auth) = prepare_account_directories(root.path()).unwrap();
        assert!(cwd.is_dir()); assert!(auth.is_dir()); assert_ne!(cwd, auth);
        assert!(prepare_account_directories(root.path()).is_ok());
        let invalid = tempfile::tempdir().unwrap();
        std::fs::write(invalid.path().join("auth"), b"existing").unwrap();
        assert!(prepare_account_directories(invalid.path()).is_err());
        assert_eq!(std::fs::read(invalid.path().join("auth")).unwrap(), b"existing");
    }
    #[test]
    fn 官方会话必须确认精确模型和权限且不能静默降级() {
        let response = json!({"model":"model-exact","approvalPolicy":"never","sandbox":{"type":"readOnly","networkAccess":false}});
        assert!(verify_started_thread(&response, "model-exact").is_ok());
        assert!(verify_started_thread(&response, "other-model").is_err());
        assert!(verify_started_thread(&json!({}), "model-exact").is_err());
        let mut wrong = response.clone();
        wrong["sandbox"]["type"] = json!("dangerFullAccess");
        assert!(verify_started_thread(&wrong, "model-exact").is_err());
        wrong = response;
        wrong["sandbox"]["networkAccess"] = json!(true);
        assert!(verify_started_thread(&wrong, "model-exact").is_err());
    }
}
