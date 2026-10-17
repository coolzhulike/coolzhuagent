//! 启动宿主提供的工具 hook 配置；工作区文件和目标工具权限均不能自行授予 hook 权限。
use std::{io::Read, path::{Path, PathBuf}, sync::OnceLock};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const CONFIG_ENV: &str = "COOLZHU_TOOL_HOOKS_CONFIG";
const PRE_GRANT_ENV: &str = "COOLZHU_TOOL_HOOKS_PRE_SHA256";
const POST_GRANT_ENV: &str = "COOLZHU_TOOL_HOOKS_POST_SHA256";
const MAX_BYTES: u64 = 65_536;
static CONFIG: OnceLock<Option<AuthorizedHooks>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HookFile {
    version: u32,
    workspace_root: PathBuf,
    hooks: Events,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Events {
    #[serde(default, rename="PreToolUse")]
    pre: Vec<String>,
    #[serde(default, rename="PostToolUse")]
    post: Vec<String>,
}

struct AuthorizedHooks {
    workspace_id: String,
    runner: runtime::HookRunner,
}

fn grant_matches(value: Option<&str>, digest: &str) -> Result<bool, String> {
    match value {
        None => Ok(false),
        Some(value) if value.len()==64 && value.eq_ignore_ascii_case(digest) => Ok(true),
        Some(_) => Err("hook 独立授权摘要与配置文件不一致；未启动执行服务".into()),
    }
}

fn load(path: &Path, workspace: &Path, pre: Option<&str>, post: Option<&str>) -> Result<AuthorizedHooks, String> {
    if !path.is_absolute() { return Err("hook 配置必须使用明确的绝对路径".into()); }
    let file = std::fs::File::open(path).map_err(|error|format!("读取 hook 配置失败：{error}"))?;
    if !file.metadata().map_err(|e|e.to_string())?.is_file() { return Err("hook 配置不是普通文件".into()); }
    let mut bytes=Vec::new();
    file.take(MAX_BYTES+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len() as u64>MAX_BYTES { return Err("hook 配置超过64KiB上限".into()); }
    // 摘要与解析取自同一份字节，不再读取路径，防止配置检查与使用之间被替换。
    let digest=format!("{:x}",Sha256::digest(&bytes));
    let config: HookFile=serde_json::from_slice(&bytes).map_err(|e|format!("hook 配置无效：{e}"))?;
    if config.version!=1 || !config.workspace_root.is_absolute() { return Err("hook 配置需version=1及绝对workspace_root".into()); }
    let root=config.workspace_root.canonicalize().map_err(|e|e.to_string())?;
    if !root.is_dir() || root!=workspace.canonicalize().map_err(|e|e.to_string())? {
        return Err("hook 配置不属于启动时工作区，未授予跨工程执行权限".into());
    }
    for commands in [&config.hooks.pre,&config.hooks.post] {
        if commands.len()>16 || commands.iter().any(|s|s.trim().is_empty() || s.len()>8192 || s.contains('\0')) {
            return Err("hook 每个事件最多16条非空命令，每条最多8KiB且不能含空字符".into());
        }
    }
    let allow_pre=grant_matches(pre,&digest)?;
    let allow_post=grant_matches(post,&digest)?;
    let mut authorization=runtime::PermissionPolicy::new(runtime::PermissionMode::DangerFullAccess);
    if allow_pre { authorization=authorization.with_tool_requirement(runtime::HOOK_CAPABILITY_PRE_TOOL_USE,runtime::PermissionMode::DangerFullAccess); }
    if allow_post { authorization=authorization.with_tool_requirement(runtime::HOOK_CAPABILITY_POST_TOOL_USE,runtime::PermissionMode::DangerFullAccess); }
    tracing::info!(config_sha256=%digest,pre_authorized=allow_pre,post_authorized=allow_post,"工具hook配置已冻结；无独立授权的事件不会执行");
    Ok(AuthorizedHooks { workspace_id:super::workspace_identity(&root),
        runner:runtime::HookRunner::new(runtime::RuntimeHookConfig::new(config.hooks.pre,config.hooks.post)).with_authorization(authorization) })
}

/// 只在服务启动时读取宿主环境，不从项目配置、HTTP请求或模型参数获取授权。
pub(super) fn initialize(workspace: &Path) -> Result<(), String> {
    let path=std::env::var_os(CONFIG_ENV);
    let read_grant=|name|std::env::var(name).map(Some).or_else(|e|match e {
        std::env::VarError::NotPresent=>Ok(None), _=>Err(format!("{name}必须为SHA256文本")),
    });
    let pre=read_grant(PRE_GRANT_ENV)?; let post=read_grant(POST_GRANT_ENV)?;
    let loaded=match path {
        Some(path)=>Some(load(Path::new(&path),workspace,pre.as_deref(),post.as_deref())?),
        None if pre.is_some() || post.is_some()=>return Err("hook授权缺少对应配置文件".into()),
        None=>None,
    };
    CONFIG.set(loaded).map_err(|_|"hook配置已初始化，不能在运行轮次中替换授权".to_owned())
}

pub(super) fn runner(workspace_id: &str) -> runtime::HookRunner {
    CONFIG.get().and_then(|config|config.as_ref()).filter(|config|config.workspace_id==workspace_id)
        .map(|config|config.runner.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_grants_bind_exact_bytes_and_workspace() {
        let root=tempfile::tempdir().unwrap(); let other=tempfile::tempdir().unwrap();
        let path=root.path().join("hooks.json");
        let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"workspace_root":root.path(),"hooks":{"PreToolUse":["echo pre"],"PostToolUse":["echo post"]}})).unwrap();
        std::fs::write(&path,&bytes).unwrap(); let digest=format!("{:x}",Sha256::digest(&bytes));
        assert!(load(&path,root.path(),Some(&digest),None).is_ok());
        assert!(load(&path,other.path(),Some(&digest),None).is_err());
        std::fs::write(&path,[bytes.as_slice(),b" "].concat()).unwrap();
        assert!(load(&path,root.path(),Some(&digest),None).is_err());
        assert!(load(&path,root.path(),None,None).is_ok());
    }
}
