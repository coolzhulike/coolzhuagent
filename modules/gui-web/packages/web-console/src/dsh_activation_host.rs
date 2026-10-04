//! 真实describe与原插件设置快照接线；不生成模型工具调用身份或接管Agent循环。
use plugins::{DshActivationSnapshot, DshActivationTicket, PluginManager};
use runtime::dsh_host_process::{self, CallContext};
use runtime::dsh_runtime::VerifiedRuntime;
use serde_json::Value;
use std::time::Duration;

/// 正式入口只认可执行文件旁的固定资源；缺失即拒绝，不回退开发SDK或全局Node。
pub fn enable_installed(
    manager: &mut PluginManager,
    plugin_id: &str,
    context: &CallContext,
    config: &Value,
) -> Result<DshActivationSnapshot, String> {
    let runtime = runtime::dsh_runtime::installed().map_err(|e| e.to_string())?;
    enable_verified(manager, plugin_id, context, config, &runtime)
}

/// 工程探针可显式提供已经通过生产核验的资源；页面没有提交资源路径的接口。
pub fn enable_verified(
    manager: &mut PluginManager,
    plugin_id: &str,
    context: &CallContext,
    config: &Value,
    verified: &VerifiedRuntime,
) -> Result<DshActivationSnapshot, String> {
    let ticket = manager
        .dsh_activation_ticket(plugin_id, &context.workspace_id)
        .map_err(|e| e.to_string())?;
    enable_ticket_verified(manager, &ticket, context, config, verified)
}

/// 审批等待期间不得重新捕获一个新的安装世代来替代已确认ticket。
pub fn enable_ticket_verified(
    manager: &mut PluginManager,
    ticket: &DshActivationTicket,
    context: &CallContext,
    config: &Value,
    verified: &VerifiedRuntime,
) -> Result<DshActivationSnapshot, String> {
    if ticket.workspace_id != context.workspace_id
        || manager
            .dsh_activation_ticket(&ticket.plugin_id, &context.workspace_id)
            .map_err(|e| e.to_string())?
            != *ticket
    {
        return Err("DSH启用请求的原安装/工程资格已变化，未启动宿主".into());
    }
    if !config.is_object()
        || serde_json::to_vec(config).map_err(|e| e.to_string())?.len() > 64 * 1024
    {
        return Err("DSH配置不是有界对象，未启动描述宿主".into());
    }
    let before = verified.reverify().map_err(|e| e.to_string())?;
    if ticket.package.receipt.sdk_lock_sha256 != before.sdk_lock_sha256 {
        return Err("DSH来源与固定宿主SDK不一致，未启用".into());
    }
    let receipt = serde_json::to_value(&ticket.package.receipt).map_err(|e| e.to_string())?;
    let manifest = dsh_host_process::describe(
        &before.paths(ticket.root.clone()),
        &receipt,
        config,
        context,
        Duration::from_secs(30),
    )
    .map_err(|e| e.to_string())?;
    let after = before.reverify().map_err(|e| e.to_string())?;
    if before.runtime_lock_sha256 != after.runtime_lock_sha256 {
        return Err("DSH描述期间固定资源身份变化，未启用".into());
    }
    if runtime::managed_process::current_execution_control()
        .is_some_and(|control| control.interruption().is_some())
    {
        return Err("DSH描述已结束，但本次启用已取消或截止，未发布快照".into());
    }
    manager
        .enable_dsh_snapshot(
            &ticket,
            config.clone(),
            after.runtime_lock_sha256,
            serde_json::to_value(manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
}
