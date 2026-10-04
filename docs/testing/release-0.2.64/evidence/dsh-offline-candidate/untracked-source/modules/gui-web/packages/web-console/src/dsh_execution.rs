//! DSH冻结工具与实际宿主执行。权限由调用方原闸门裁决，本模块不生成模型身份或续期预算。
use api::ToolDefinition;
use plugins::{DshActivationSnapshot, PluginManager};
use runtime::dsh_host_process::{self, CallContext, HostManifest};
use runtime::dsh_runtime::VerifiedRuntime;
use runtime::managed_process::{current_execution_control, with_execution_control, Interruption};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PREFIX: &str = "dsh__";

/// 原取消和截止逐次检查；持久安装/停用世代复核最多间隔50ms，首次失败永久撤销。
/// 接纳、启动及收尾仍直接做完整复核，不能用这里的短暂缓存授予新执行资格。
pub fn revocation_probe(
    immediate: Arc<dyn Fn() -> bool + Send + Sync>,
    qualification_lost: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Arc<dyn Fn() -> bool + Send + Sync> {
    let state = Mutex::new((Instant::now(), false));
    Arc::new(move || {
        if immediate() {
            return true;
        }
        let Ok(mut state) = state.lock() else {
            return true;
        };
        if state.1 {
            return true;
        }
        if Instant::now() >= state.0 {
            state.1 = qualification_lost();
            state.0 = Instant::now() + Duration::from_millis(50);
        }
        state.1
    })
}
#[derive(Clone, Debug)]
pub struct Binding {
    pub snapshot: DshActivationSnapshot,
    pub root: PathBuf,
    pub raw_name: String,
    pub definition: ToolDefinition,
}
#[derive(Debug)]
pub struct ExecutionError {
    pub message: String,
    pub interruption: Option<Interruption>,
    pub cleanup_confirmed: Option<bool>,
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ExecutionError {}
fn rejected(message: impl Into<String>) -> ExecutionError {
    ExecutionError {
        message: message.into(),
        interruption: current_execution_control().and_then(|c| c.interruption()),
        cleanup_confirmed: None,
    }
}

/// 命名包含插件完整身份与实际工具名的摘要，不截断原SDK工具名或改写schema。
pub fn exposed_name(plugin_id: &str, raw: &str) -> String {
    let identity = serde_json::to_vec(&(plugin_id, raw)).expect("字符串对可以序列化");
    format!("{PREFIX}{:x}", Sha256::digest(identity))[..PREFIX.len() + 32].to_string()
}

pub fn bindings(
    manager: &PluginManager,
    workspace: &str,
    verified: &VerifiedRuntime,
) -> Result<BTreeMap<String, Binding>, String> {
    let mut result = BTreeMap::new();
    for plugin in manager
        .list_installed_plugins()
        .map_err(|e| e.to_string())?
    {
        if plugin.metadata.dsh.is_none() || !plugin.enabled {
            continue;
        }
        let Some(snapshot) = manager
            .dsh_snapshot(&plugin.metadata.id, workspace)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        if snapshot.runtime_lock_sha256 != verified.runtime_lock_sha256
            || snapshot.package.receipt.sdk_lock_sha256 != verified.sdk_lock_sha256
        {
            return Err("DSH固定资源与启用快照不一致，工具未加载".into());
        }
        let ticket = manager
            .dsh_activation_ticket(&plugin.metadata.id, workspace)
            .map_err(|e| e.to_string())?;
        let manifest: HostManifest =
            serde_json::from_value(snapshot.host_manifest.clone()).map_err(|e| e.to_string())?;
        for tool in manifest.tools {
            let name = exposed_name(&snapshot.plugin_id, &tool.name);
            let binding = Binding {
                snapshot: snapshot.clone(),
                root: ticket.root.clone(),
                raw_name: tool.name.clone(),
                definition: ToolDefinition {
                    name: name.clone(),
                    description: Some(format!(
                        "{} / {}：{}",
                        snapshot.package.receipt.name, tool.name, tool.description
                    )),
                    input_schema: tool.input_schema,
                },
            };
            if result.insert(name, binding).is_some() {
                return Err("DSH工具命名冲突，工具未加载".into());
            }
        }
    }
    Ok(result)
}

pub fn still_current(manager: &PluginManager, binding: &Binding) -> bool {
    matches!(manager.dsh_snapshot(&binding.snapshot.plugin_id, &binding.snapshot.workspace_id), Ok(Some(ref snapshot)) if snapshot == &binding.snapshot)
        && matches!(manager.dsh_activation_ticket(&binding.snapshot.plugin_id, &binding.snapshot.workspace_id), Ok(ref ticket) if ticket.root == binding.root)
}
pub fn lifecycle_current(manager: &PluginManager, binding: &Binding) -> bool {
    manager
        .dsh_snapshot_lifecycle_current(&binding.snapshot, &binding.root)
        .unwrap_or(false)
}

/// 必须由原执行监督器提供ExecutionControl。调用者保留真实父轮/provider来源及权限审计。
pub fn execute(
    manager: Arc<PluginManager>,
    binding: &Binding,
    verified: &VerifiedRuntime,
    context: &CallContext,
    input: &Value,
    revoked: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Value, ExecutionError> {
    let control = current_execution_control()
        .ok_or_else(|| rejected("DSH缺少已有执行取消/截止，未启动宿主"))?;
    if context.workspace_id != binding.snapshot.workspace_id || !still_current(&manager, binding) {
        return Err(rejected("DSH冻结快照已停用、卸载、重装或归属变化，未执行"));
    }
    let expected = binding.clone();
    let live_manager = manager.clone();
    let control = control.with_additional_cancel(revocation_probe(
        revoked,
        Arc::new(move || !lifecycle_current(&live_manager, &expected)),
    ));
    with_execution_control(control.clone(), || {
        if control.interruption().is_some() {
            return Err(rejected("DSH父轮或快照资格已撤销，未执行"));
        }
        let verified = verified.reverify().map_err(|e| rejected(e.to_string()))?;
        if verified.runtime_lock_sha256 != binding.snapshot.runtime_lock_sha256
            || verified.sdk_lock_sha256 != binding.snapshot.package.receipt.sdk_lock_sha256
        {
            return Err(rejected("DSH固定资源身份变化，未执行"));
        }
        let manifest: HostManifest = serde_json::from_value(binding.snapshot.host_manifest.clone())
            .map_err(|e| rejected(e.to_string()))?;
        let receipt = serde_json::to_value(&binding.snapshot.package.receipt)
            .map_err(|e| rejected(e.to_string()))?;
        let result = dsh_host_process::execute(
            &verified.paths(binding.root.clone()),
            &receipt,
            &binding.snapshot.config,
            context,
            &manifest,
            &binding.raw_name,
            input,
            Duration::from_secs(30),
        )
        .map_err(|e| ExecutionError {
            message: e.to_string(),
            interruption: control.interruption(),
            cleanup_confirmed: Some(e.cleanup_confirmed),
        })?;
        verified.reverify().map_err(|e| {
            let mut error = rejected(e.to_string());
            // exchange成功仅在真实子进程和Job收尾确认后返回。
            error.cleanup_confirmed = Some(true);
            error
        })?;
        if control.interruption().is_some() || !still_current(&manager, binding) {
            let mut error = rejected("DSH父轮或启用资格已经结束，迟到结果不采纳，未自动重放");
            error.cleanup_confirmed = Some(true);
            return Err(error);
        }
        Ok(result)
    })
}
