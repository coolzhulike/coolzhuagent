//! Devin 云会话原生插件：复用本地工具闸门，不参与模型 ProviderClient 循环。
mod client;
mod store;
use super::*;
use sha2::Digest;

const ID: &str = "devin-cloud@native";
const PREFIX: &str = "plugin__devin_";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    enabled: bool,
    org_id: String,
    mode: String,
    api_key: String,
    placeholder: bool,
    revision: u64,
}

fn gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn path(root: &Path) -> PathBuf {
    root.join(".coolzhu").join("devin-plugin.json")
}
fn load(root: &Path) -> Result<Option<Config>, String> {
    match std::fs::read_to_string(path(root)) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| "Devin 配置损坏，请恢复配置文件".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Devin 配置不可读".into()),
    }
}
fn publish(root: &Path, config: &Config) -> Result<(), String> {
    config_publication::publish(
        &path(root),
        &serde_json::to_string_pretty(config).map_err(|_| "Devin 配置序列化失败")?,
    )
}
fn ensure(root: &Path) -> Result<Config, String> {
    if let Some(config) = load(root)? {
        return Ok(config);
    }
    let mut random = [0u8; 24];
    getrandom::fill(&mut random).map_err(|_| "无法生成 Devin 占位密钥")?;
    let placeholder = format!(
        "DEVIN_PENDING_{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let config = Config {
        enabled: false,
        org_id: String::new(),
        mode: String::new(),
        api_key: credential_store::protect(&placeholder)?,
        placeholder: true,
        revision: 1,
    };
    publish(root, &config)?;
    Ok(config)
}

fn ready(config: &Config) -> Result<(), String> {
    if !config.enabled {
        return Err("Devin 云会话插件未启用".into());
    }
    if config.placeholder || config.api_key.is_empty() {
        return Err("Devin API Key 是随机占位值，请由本人输入真实密钥后保存；未联网".into());
    }
    if !client::valid_id(&config.org_id, "org-") {
        return Err("请填写 Devin 组织 ID（org- 开头）".into());
    }
    if !client::valid_mode(&config.mode) {
        return Err("Devin Agent 模式配置无效".into());
    }
    Ok(())
}

fn public(root: &Path, config: &Config) -> Result<JsonValue, String> {
    Ok(
        json!({"workspace_id":workspace_identity(root),"plugin_id":ID,"enabled":config.enabled,
        "org_id":config.org_id,"mode":config.mode,"revision":config.revision,
        "credential_state":if config.placeholder {"placeholder"} else {"configured_unverified"},
        "base_url":client::BASE_URL,"model_selection_supported":false,"reasoning_effort_supported":false,
        "sessions":store::list(&store::open(root)?,&config.org_id)?}),
    )
}

pub(super) fn catalog(root: &Path) -> JsonValue {
    let config = load(root).ok().flatten();
    let enabled = config.as_ref().is_some_and(|config| config.enabled);
    let loaded = config.as_ref().is_some_and(|config| ready(config).is_ok());
    json!({"id":ID,"name":"Devin 云会话","version":"0.1.0",
        "description":"官方 v3 异步云端 Agent 会话；在本页独立配置。",
        "source":"内置会话插件","enabled":enabled,"installed":true,"kind":"builtin",
        "installable":false,"manageable":false,"loaded_in_chat":loaded})
}

pub(super) fn routes() -> Router {
    Router::new()
        .route("/api/devin/config", get(get_config).post(save_config))
        .route("/api/devin/action", post(action))
}

fn fail(message: String) -> ApiError {
    api_error(StatusCode::BAD_REQUEST, &message)
}
fn root(expected: &str) -> ApiResult<PathBuf> {
    let root = active_workspace_path()
        .canonicalize()
        .map_err(|_| fail("工程目录不可访问".into()))?;
    if workspace_identity(&root) != expected {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工程已切换，请重新打开 Devin 配置",
        ));
    }
    Ok(root)
}

async fn get_config() -> ApiResult<Json<JsonValue>> {
    let _pin = workspace_activity::pin_workspace().map_err(fail)?;
    let _guard = gate().lock().await;
    let root = active_workspace_path()
        .canonicalize()
        .map_err(|_| fail("工程目录不可访问".into()))?;
    let config = ensure(&root).map_err(fail)?;
    Ok(Json(public(&root, &config).map_err(fail)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    expected_workspace: String,
    revision: u64,
    enabled: bool,
    org_id: String,
    mode: String,
    #[serde(default)]
    api_key: String,
}

async fn save_config(Json(payload): Json<Save>) -> ApiResult<Json<JsonValue>> {
    let _pin = workspace_activity::pin_workspace().map_err(fail)?;
    let _guard = gate().lock().await;
    let root = root(&payload.expected_workspace)?;
    let mut config = ensure(&root).map_err(fail)?;
    if config.revision != payload.revision {
        return Err(api_error(
            StatusCode::CONFLICT,
            "Devin 配置已变更，请刷新后保存",
        ));
    }
    let org = payload.org_id.trim();
    let mode = payload.mode.trim();
    if (!org.is_empty() && !client::valid_id(org, "org-")) || !client::valid_mode(mode) {
        return Err(fail("组织 ID 或 Agent 模式无效".into()));
    }
    let key = payload.api_key.trim();
    if !key.is_empty() {
        if key.len() > 4096
            || key.starts_with(credential_store::PREFIX)
            || key.starts_with("DEVIN_PENDING_")
            || key.bytes().any(|b| b.is_ascii_control())
        {
            return Err(fail(
                "请输入真实 API Key；不要粘贴密文、占位值或控制字符".into(),
            ));
        }
        config.api_key = credential_store::protect(key).map_err(fail)?;
        config.placeholder = false;
    }
    config.enabled = payload.enabled;
    config.org_id = org.into();
    config.mode = mode.into();
    config.revision = config
        .revision
        .checked_add(1)
        .ok_or_else(|| fail("配置版本达到上限".into()))?;
    publish(&root, &config).map_err(fail)?;
    Ok(Json(public(&root, &config).map_err(fail)?))
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Action {
    expected_workspace: String,
    revision: u64,
    action: String,
    #[serde(default)]
    operation_id: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    after: String,
}

async fn action(Json(payload): Json<Action>) -> ApiResult<Json<JsonValue>> {
    // pin 在整个 HTTP 请求与写账本期间有效，防止工程切换后投影到新工程。
    let _pin = workspace_activity::pin_workspace().map_err(fail)?;
    let root = root(&payload.expected_workspace)?;
    perform(&root, &payload, None, true)
        .await
        .map(Json)
        .map_err(fail)
}

async fn perform(
    root: &Path,
    payload: &Action,
    control: Option<runtime::managed_process::ExecutionControl>,
    manual: bool,
) -> Result<JsonValue, String> {
    let _guard = gate().lock().await;
    let config = load(root)?.ok_or("请先打开 Devin 配置")?;
    if workspace_identity(root) != payload.expected_workspace || config.revision != payload.revision
    {
        return Err("Devin 工程或配置版本已变更，未执行".into());
    }
    ready(&config)?;
    if control.as_ref().is_some_and(|c| c.interruption().is_some()) {
        return Err("Devin 工具已取消或截止，未执行".into());
    }
    let mutation = matches!(payload.action.as_str(), "create" | "send");
    if !matches!(
        payload.action.as_str(),
        "create" | "send" | "get" | "messages" | "bind"
    ) || (payload.action == "bind" && !manual)
    {
        return Err("Devin 操作无效".into());
    }
    if payload.action != "create" && !client::valid_id(&payload.session_id, "devin-") {
        return Err("请指定 Devin 会话 ID".into());
    }
    if payload.after.len() > 512
        || payload.text.len() > 64 * 1024
        || (mutation && payload.text.trim().is_empty())
    {
        return Err("消息为空或超过 64 KiB，或分页游标过长".into());
    }
    if mutation
        && (payload.operation_id.is_empty()
            || payload.operation_id.len() > 180
            || !payload
                .operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    {
        return Err("Devin 写操作缺少有效的唯一操作编号".into());
    }
    let db = store::open(root)?;
    if !matches!(payload.action.as_str(), "create" | "bind") {
        store::bound(&db, &payload.session_id, &config.org_id)?;
    }
    let key = credential_store::reveal(&config.api_key)?;
    if key.is_empty() || key.starts_with("DEVIN_PENDING_") {
        return Err("Devin 密钥尚未配置，未联网".into());
    }
    if mutation {
        let fingerprint = format!(
            "{:x}",
            sha2::Sha256::digest(
                serde_json::to_vec(&(payload, &config.org_id, &config.mode))
                    .map_err(|_| "操作序列化失败")?
            )
        );
        if let Some(cached) = store::reserve(&db, &payload.operation_id, &fingerprint)? {
            return Ok(cached);
        }
    }
    let request = client::request(
        &key,
        &config.org_id,
        &payload.session_id,
        &payload.action,
        &payload.text,
        &config.mode,
        &payload.after,
    );
    tokio::pin!(request);
    let raw = loop {
        tokio::select! {
            result=&mut request => break result?,
            _=tokio::time::sleep(Duration::from_millis(100)), if control.is_some() => {
                if control.as_ref().is_some_and(|c|c.interruption().is_some()) {
                    return Err("Devin 请求已取消或截止；写操作结果未知，请先到官网核对".into());
                }
            }
        }
    };
    // 密钥不进入任何工具结果/前端回执，连远端消息中的意外回显也遮盖。
    let mut raw = raw;
    redact(&mut raw, &key);
    let result = if payload.action == "messages" {
        json!({"session_id":payload.session_id,"messages":raw})
    } else {
        let expected = if payload.action == "create" {
            ""
        } else {
            &payload.session_id
        };
        let fact = client::session_fact(&raw, &config.org_id, expected)?;
        store::bind(
            &db,
            &fact,
            &config.org_id,
            if manual { "manual" } else { "model-tool" },
        )?;
        fact
    };
    if mutation {
        store::settle(&db, &payload.operation_id, &result)?;
    }
    Ok(result)
}

pub(super) fn definitions(root: &Path) -> Vec<ToolDefinition> {
    if !load(root)
        .ok()
        .flatten()
        .is_some_and(|config| ready(&config).is_ok())
    {
        return vec![];
    }
    [
        ("create","创建 Devin 异步云端 Agent 会话，仅在用户明确要求委派云端任务时调用。成功只表示创建，请随后读取状态。",json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false})),
        ("send","向当前工程已绑定的 Devin 会话发送消息；不要因超时自动重发。",json!({"type":"object","properties":{"session_id":{"type":"string"},"text":{"type":"string"}},"required":["session_id","text"],"additionalProperties":false})),
        ("get","读取当前工程已绑定 Devin 会话的真实状态和结构化结果。",json!({"type":"object","properties":{"session_id":{"type":"string"}},"required":["session_id"],"additionalProperties":false})),
        ("messages","读取当前工程已绑定 Devin 会话的消息分页；需要时传 after 游标。",json!({"type":"object","properties":{"session_id":{"type":"string"},"after":{"type":"string"}},"required":["session_id"],"additionalProperties":false})),
    ].into_iter().map(|(name,description,input_schema)|ToolDefinition{name:format!("{PREFIX}{name}"),description:Some(description.into()),input_schema}).collect()
}

pub(super) fn executor(
    root: &Path,
    name: &str,
) -> Result<Option<Arc<dyn ToolInvocationExecutor>>, String> {
    if !name.starts_with(PREFIX) {
        return Ok(None);
    }
    if !definitions(root).iter().any(|tool| tool.name == name) {
        return Err("Devin 插件未就绪或工具无效".into());
    }
    let revision = load(root)?.ok_or("Devin 未配置")?.revision;
    let root = root.canonicalize().map_err(|_| "Devin 工程目录不可访问")?;
    Ok(Some(Arc::new(Executor {
        root,
        name: name.into(),
        revision,
    })))
}

fn redact(value: &mut JsonValue, key: &str) {
    match value {
        JsonValue::String(text) => *text = text.replace(key, "[密钥已隐藏]"),
        JsonValue::Array(items) => items.iter_mut().for_each(|item| redact(item, key)),
        JsonValue::Object(items) => {
            let previous = std::mem::take(items);
            for (name, mut value) in previous {
                redact(&mut value, key);
                items.insert(name.replace(key, "[密钥已隐藏]"), value);
            }
        }
        _ => {}
    }
}

struct Executor {
    root: PathBuf,
    name: String,
    revision: u64,
}
impl ToolInvocationExecutor for Executor {
    fn handles(&self, name: &str) -> bool {
        self.name == name
    }
    fn execute(&self, invoke: &ToolInvoke) -> ToolOutcome {
        let started = Instant::now();
        let result = (|| -> Result<JsonValue, String> {
            if invoke.workspace_id != workspace_identity(&self.root) {
                return Err("Devin 工具工程身份已变化".into());
            }
            let _pin = workspace_activity::pin_workspace()?;
            if workspace_identity(&active_workspace_path()) != workspace_identity(&self.root) {
                return Err("当前工程已变化".into());
            }
            let action = self
                .name
                .strip_prefix(PREFIX)
                .ok_or("工具名称无效")?
                .to_string();
            let mut input = invoke
                .input
                .as_object()
                .cloned()
                .ok_or("工具参数必须是对象")?;
            input.insert("expected_workspace".into(), json!(invoke.workspace_id));
            input.insert("revision".into(), json!(self.revision));
            input.insert("action".into(), json!(action));
            let operation_scope =
                serde_json::to_vec(&(&invoke.workspace_id, &invoke.session_id, &invoke.call_id))
                    .map_err(|_| "工具操作编号生成失败")?;
            input.insert(
                "operation_id".into(),
                json!(format!("tool-{:x}", sha2::Sha256::digest(operation_scope))),
            );
            let payload: Action =
                serde_json::from_value(json!(input)).map_err(|_| "Devin 工具参数无效")?;
            let control = runtime::managed_process::current_execution_control();
            // 同步工具在受监督 blocking worker 中执行；独立线程隔离 Tokio 嵌套运行时。
            let root = self.root.clone();
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| "Devin 工具运行时不可用".to_string())?
                    .block_on(perform(&root, &payload, control, false))
            })
            .join()
            .map_err(|_| "Devin 工具异常退出，写操作结果可能未知")?
        })();
        let (status, output, summary_text) = match result {
            Ok(result) => (
                ToolOutcomeStatus::Ok,
                result,
                "Devin 请求已返回；远端任务状态见结果".to_string(),
            ),
            Err(message) => (
                ToolOutcomeStatus::Failed,
                json!({"error":message}),
                format!("Devin 请求未确认成功：{message}"),
            ),
        };
        ToolOutcome {
            call_id: invoke.call_id.clone(),
            tool_name: invoke.tool_name.clone(),
            status,
            output,
            summary_text,
            elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            permission_gate: runtime::PermissionGateReport::deny(
                PermissionMode::DangerFullAccess,
                "placeholder-overwritten-by-runtime",
            ),
            evidence: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secret_redaction_preserves_json_structure() {
        let key = "cog_quote\"\\";
        let mut value = json!({key:[format!("message {key}"),{"nested":key}]});
        redact(&mut value, key);
        assert_eq!(value["[密钥已隐藏]"][1]["nested"], "[密钥已隐藏]");
        assert!(!value.to_string().contains("cog_quote"));
    }
    #[tokio::test]
    async fn placeholder_is_encrypted_and_blocks_network_before_journal() {
        let directory = tempfile::tempdir().unwrap();
        let config = ensure(directory.path()).unwrap();
        assert!(config.placeholder);
        #[cfg(windows)]
        assert!(config.api_key.starts_with(credential_store::PREFIX));
        let view = public(directory.path(), &config).unwrap();
        assert!(view.get("api_key").is_none());
        assert!(definitions(directory.path()).is_empty());
        let mut config = config;
        config.enabled = true;
        config.org_id = "org-local-check".into();
        publish(directory.path(), &config).unwrap();
        let request = Action {
            expected_workspace: workspace_identity(directory.path()),
            revision: config.revision,
            action: "create".into(),
            operation_id: "one-create".into(),
            session_id: String::new(),
            text: "测试".into(),
            after: String::new(),
        };
        let result = perform(directory.path(), &request, None, true)
            .await
            .unwrap_err();
        assert!(result.contains("占位"));
        let db = store::open(directory.path()).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM operations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
