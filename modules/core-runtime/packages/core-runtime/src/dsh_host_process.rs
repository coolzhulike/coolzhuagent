//! DSH 一次性宿主进程桥。只负责有界 IPC 与生命周期，不做安装、权限批准或会话循环。
use crate::managed_process::{Interruption, ProtocolChild};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_LIFETIME: Duration = Duration::from_secs(180);
const CANCEL_GRACE: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct HostPaths {
    pub node_binary: PathBuf,
    pub entry_script: PathBuf,
    pub plugin_root: PathBuf,
}

/// 由调用者冻结实际父运行归属；不得从插件的回包采纳新的归属。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallContext {
    pub workspace_id: String,
    pub room_id: String,
    pub run_id: String,
    pub call_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginIdentity { pub name: String, pub version: String }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolSchema { pub name: String, pub description: String, pub input_schema: Value }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostManifest {
    pub protocol: u32,
    pub generation: String,
    pub revision: String,
    pub plugin: PluginIdentity,
    pub services: Vec<String>,
    pub tools: Vec<ToolSchema>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HostError {
    pub code: String,
    pub message: String,
    pub interruption: Option<String>,
    pub cleanup_confirmed: bool,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}：{}", self.code, self.message) }
}
impl std::error::Error for HostError {}

fn error(code: &str, message: impl Into<String>) -> HostError {
    HostError { code: code.into(), message: message.into(), interruption: None, cleanup_confirmed: false }
}
fn io_error(cause: io::Error) -> HostError { error("host_io_failed", cause.to_string()) }

fn read_message(directory: &Path, name: &str) -> Result<Option<Value>, HostError> {
    let file = directory.join(name);
    let metadata = match fs::symlink_metadata(&file) {
        Ok(metadata) => metadata,
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(cause) => return Err(io_error(cause)),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_MESSAGE_BYTES as u64 {
        return Err(error("protocol_invalid", "协议文件类型或大小无效"));
    }
    let mut bytes = Vec::new();
    fs::File::open(&file).map_err(io_error)?.take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() > MAX_MESSAGE_BYTES { return Err(error("protocol_invalid", "协议读取超出大小限制")); }
    serde_json::from_slice(&bytes).map(Some).map_err(|_| error("protocol_invalid", "协议文件不是有效 JSON"))
}

fn publish(directory: &Path, name: &str, value: &Value) -> Result<(), HostError> {
    let bytes = serde_json::to_vec(value).map_err(|_| error("protocol_invalid", "协议请求无法序列化"))?;
    if bytes.len() > MAX_MESSAGE_BYTES { return Err(error("protocol_invalid", "协议请求超出大小限制")); }
    let temporary = directory.join(format!("{name}.tmp"));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);
    fs::rename(&temporary, directory.join(name)).map_err(io_error)
}

fn validate_envelope(value: &Value, nonce: &str, context: &CallContext) -> Result<(), HostError> {
    let received = serde_json::from_value::<CallContext>(value["context"].clone()).ok();
    if value["protocol"] != 1 || value["nonce"].as_str() != Some(nonce) || received.as_ref() != Some(context) {
        return Err(error("protocol_stale", "宿主回包的协议或父运行身份不符"));
    }
    Ok(())
}

fn validate_manifest(manifest: &HostManifest) -> Result<(), HostError> {
    if manifest.protocol != 1 || manifest.generation.is_empty() || manifest.generation.len() > 128
        || manifest.revision.len() != 64 || !manifest.revision.bytes().all(|b| b.is_ascii_hexdigit())
        || manifest.services != ["tools"] || manifest.tools.is_empty() || manifest.tools.len() > 24 {
        return Err(error("manifest_invalid", "宿主工具清单身份、服务或数量无效"));
    }
    let mut seen = std::collections::HashSet::new();
    for tool in &manifest.tools {
        if tool.name.is_empty() || tool.name.len() > 57
            || !tool.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !seen.insert(&tool.name) || tool.input_schema["type"] != "object" {
            return Err(error("manifest_invalid", "宿主工具名、重名或参数 schema 无效"));
        }
    }
    Ok(())
}

pub fn describe(paths: &HostPaths, receipt: &Value, config: &Value, context: &CallContext,
    timeout: Duration) -> Result<HostManifest, HostError> {
    let result = exchange(paths, receipt, config, context, timeout, None)?;
    let manifest = serde_json::from_value(result["manifest"].clone())
        .map_err(|_| error("manifest_invalid", "宿主未返回可用清单"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// 本次 live 握手的世代只用于本次调用；跨进程沿用的是冻结来源修订和真实工具定义。
pub fn execute(paths: &HostPaths, receipt: &Value, config: &Value, context: &CallContext,
    expected: &HostManifest, name: &str, arguments: &Value, timeout: Duration) -> Result<Value, HostError> {
    validate_manifest(expected)?;
    if !expected.tools.iter().any(|tool| tool.name == name) {
        return Err(error("tool_unavailable", "冻结的插件清单未注册此工具"));
    }
    let result = exchange(paths, receipt, config, context, timeout, Some((expected, name, arguments)))?;
    Ok(result["result"].clone())
}

fn exchange(paths: &HostPaths, receipt: &Value, config: &Value, context: &CallContext,
    timeout: Duration, execution: Option<(&HostManifest, &str, &Value)>) -> Result<Value, HostError> {
    if timeout.is_zero() || timeout > MAX_LIFETIME
        || [&context.workspace_id, &context.room_id, &context.run_id, &context.call_id]
            .iter().any(|text| text.is_empty() || text.len() > 192)
        || !paths.node_binary.is_absolute() || !paths.node_binary.is_file()
        || !paths.entry_script.is_absolute() || !paths.entry_script.is_file()
        || !paths.plugin_root.is_absolute() || !paths.plugin_root.is_dir() {
        return Err(error("host_request_invalid", "宿主路径、时限或父运行身份无效"));
    }
    let directory = tempfile::Builder::new().prefix("coolzhu-dsh-call-").tempdir().map_err(io_error)?;
    let mut nonce_bytes = [0_u8; 24];
    getrandom::fill(&mut nonce_bytes).map_err(|_| error("host_identity_failed", "无法生成调用身份"))?;
    let nonce = nonce_bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| error("host_clock_invalid", "系统时间早于纪元"))?;
    let deadline_ms = (now + timeout).as_millis().min(u128::from(u64::MAX)) as u64;
    let envelope = |extra: Value| {
        let mut value = json!({"protocol":1, "nonce":nonce, "context":context});
        value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        value
    };
    publish(directory.path(), "request.json", &envelope(json!({
        "mode": if execution.is_some() {"execute"} else {"describe"},
        "deadline_ms":deadline_ms, "root":paths.plugin_root, "receipt":receipt, "config":config,
    })))?;
    let mut command = Command::new(&paths.node_binary);
    command.arg(&paths.entry_script).arg(directory.path()).current_dir(&paths.plugin_root)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // 不继承模型密钥、预载脚本或用户搜索路径；仅保留 Node/Windows 运行需要的系统和临时目录。
    let environment = ["SystemRoot", "WINDIR", "TEMP", "TMP"].into_iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (name, value))).collect::<Vec<_>>();
    command.env_clear().envs(environment);
    let mut child = ProtocolChild::spawn(&mut command, timeout).map_err(io_error)?;
    let mut dispatched = false;
    let result = (|| {
        loop {
            if let Some(reason) = child.interruption() {
                let notification = publish(directory.path(), "cancel.json", &envelope(json!({})));
                let drain_until = Instant::now() + CANCEL_GRACE;
                while notification.is_ok() && Instant::now() < drain_until {
                    if child.try_wait().map_err(io_error)?.is_some() { break; }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let cleaned = child.stop().is_ok();
                return Err(HostError { code: "host_interrupted".into(),
                    message: "父调用已结束，插件迟到结果不再采纳；未自动重放".into(),
                    interruption: Some(match reason { Interruption::Cancelled => "cancelled", Interruption::TimedOut => "timed_out" }.into()),
                    cleanup_confirmed: cleaned });
            }
            if !dispatched {
                if let Some(value) = read_message(directory.path(), "manifest.json")? {
                    validate_envelope(&value, &nonce, context)?;
                    let manifest: HostManifest = serde_json::from_value(value["manifest"].clone())
                        .map_err(|_| error("manifest_invalid", "宿主握手清单无效"))?;
                    validate_manifest(&manifest)?;
                    if let Some((expected, name, arguments)) = execution {
                        if manifest.revision != expected.revision || manifest.plugin != expected.plugin
                            || manifest.tools != expected.tools || manifest.services != expected.services {
                            return Err(error("host_stale", "启用插件来源或工具定义已变化，未执行"));
                        }
                        // 世代取自本次真实运行的宿主，不将 catalog 查询的旧世代用于新进程。
                        publish(directory.path(), "execute.json", &envelope(json!({
                            "generation":manifest.generation, "revision":manifest.revision,
                            "name":name, "arguments":arguments,
                        })))?;
                    }
                    dispatched = true;
                }
            }
            // 先核取消，再核主进程退出和终态；插件声称完成不能覆盖父调用取消。
            if let Some(status) = child.try_wait().map_err(io_error)? {
                let terminal = read_message(directory.path(), "result.json")?
                    .ok_or_else(|| error("host_crashed", "宿主退出且无完整收尾回执"))?;
                validate_envelope(&terminal, &nonce, context)?;
                if child.interruption().is_some() { continue; }
                if !status.success() || terminal["status"] != if execution.is_some() {"executed"} else {"described"} {
                    return Err(error("host_failed", terminal["error"]["message"].as_str().unwrap_or("宿主未完成执行或资源释放")));
                }
                return Ok(terminal);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    // 正常退出仍关闭所属 Job，避免插件派生进程保留资格；错误与 panic 由 Drop 同样收尾。
    let cleanup = child.stop();
    match (result, cleanup) {
        (Ok(value), Ok(_)) => Ok(value),
        (Err(mut failed), cleanup) => { failed.cleanup_confirmed |= cleanup.is_ok(); Err(failed) },
        (Ok(_), Err(cause)) => Err(io_error(cause)),
    }
}
