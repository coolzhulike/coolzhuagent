//! 聊天后端已接通的宿主能力；配置保存和执行入口共用同一边界。
use crate::SessionModelLimitOverride;

pub(crate) fn extension(name: &str) -> bool {
    (name.starts_with("plugin__") && !name.starts_with("plugin__devin_")
        || name.starts_with("dsh__"))
        && name.len() <= 64
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn supported(name: &str, computer: bool) -> bool {
    super::bridge::REVIEW_TOOLS.contains(&name)
        || name == "computer_use_perform" && computer
        || extension(name)
}

pub(crate) fn enabled(settings: &SessionModelLimitOverride) -> Result<bool, String> {
    if settings.enable_llm_tools != Some(true) { return Ok(false); }
    if settings.llm_tool_exposure.as_deref() != Some("whitelist")
        || !settings.tool_allowlist.as_ref().is_some_and(|tools| !tools.is_empty()
            && tools.iter().all(|name| supported(name, settings.computer_use_enabled == Some(true)))) {
        return Err("Devin 请明确选择工程只读、Computer Use 或已启用插件工具；其它工具尚未接入。".into());
    }
    Ok(true)
}
