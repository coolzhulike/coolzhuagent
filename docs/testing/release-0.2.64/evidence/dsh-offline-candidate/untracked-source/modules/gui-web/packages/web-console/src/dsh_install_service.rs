//! 固定来源确认后复用原静态安装事务；不执行插件、不修改输入安全或模型配置。
use crate::dsh_source_download::PreparedPackage;
use plugins::PluginManager;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct InstallReceipt {
    pub workspace_id: String,
    pub plugin_id: String,
    pub operation_id: String,
    pub name: String,
    pub version: String,
    pub commit: String,
    pub source_sha256: String,
    pub installed: bool,
    pub enabled: bool,
    pub cleanup_confirmed: bool,
    pub cleanup_error: Option<String>,
}

/// 调用者先冻结工程，且不得因HTTP关闭而重发。进入提交后以持久登记为准。
pub fn install_confirmed(
    manager: &mut PluginManager,
    prepared: PreparedPackage,
    expected_source: &str,
    workspace_id: &str,
    description: &str,
) -> Result<InstallReceipt, String> {
    let fingerprint = prepared
        .package
        .source_fingerprint()
        .map_err(|e| e.to_string())?;
    if fingerprint != expected_source {
        let cleanup = prepared.discard();
        return Err(format!(
            "固定来源与刚才确认的版本不一致，未安装；暂存清理确认：{}",
            cleanup.is_ok()
        ));
    }
    let package = prepared.package.clone();
    let outcome = match manager.install_dsh(prepared.path(), &package, description) {
        Ok(outcome) => outcome,
        Err(error) => {
            let cleanup = prepared.discard();
            return Err(format!(
                "安装事务返回失败，请核对持久登记：{error}；暂存清理确认：{}",
                cleanup.is_ok()
            ));
        }
    };
    // 安装已提交：清理失败须保留真实安装成功事实，不能冒称事务回滚或自动重装。
    let cleanup = prepared.discard();
    Ok(InstallReceipt {
        workspace_id: workspace_id.into(),
        plugin_id: outcome.plugin_id,
        operation_id: outcome.operation_id,
        name: package.receipt.name,
        version: package.receipt.version,
        commit: package.commit,
        source_sha256: fingerprint,
        installed: true,
        enabled: false,
        cleanup_confirmed: cleanup.is_ok(),
        cleanup_error: cleanup.err().map(|e| e.to_string()),
    })
}
