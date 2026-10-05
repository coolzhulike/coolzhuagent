//! 使用账号真实目录发现模型；不生成任务，不读取/回显 CLI 凭据。
use axum::{Json, http::StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, path::{Path, PathBuf}, sync::OnceLock, time::Duration};

const MAX_MODELS: usize = 5000;

#[derive(Debug, Serialize)]
pub(super) struct DiscoveredModel {
    id: String,
    name: String,
    image_input: Option<bool>,
    input_modalities: Option<Vec<String>>,
    capability_source: &'static str,
    context_window: Option<u64>,
    max_output_tokens: Option<u64>,
    cost_tier: Option<String>,
    cost_summary: Option<String>,
    family: String,
    reasoning_effort: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct DiscoveryResponse {
    models: Vec<DiscoveredModel>,
    provider_hint: &'static str,
    backend_kind: super::super::agent_session_backend::AgentSessionBackend,
    discovery_source: &'static str,
    cli_version: String,
    version_pinned: bool,
    authentication: &'static str,
    checked_at: u64,
    complete: bool,
    agent_execution_ready: bool,
    text_chat_ready: bool,
    warnings: Vec<String>,
}

/// 桌面版捆绑的 CLI 与 Electron 桌面入口是两个不同的可执行文件。
#[cfg(windows)]
fn bundled_cli(local_app_data: &Path) -> Option<PathBuf> {
    if !local_app_data.is_absolute() {
        return None;
    }
    let path = local_app_data.join("Programs/Devin/resources/app/extensions/windsurf/devin/bin/devin.exe");
    path.is_file().then_some(path)
}

#[cfg(windows)]
fn is_desktop_entry(path: &Path) -> bool {
    path.parent().is_some_and(|parent| {
        parent.join("resources/app/out/main.js").is_file()
            && parent.join("chrome_100_percent.pak").is_file()
    })
}

pub(super) fn binary() -> Result<PathBuf, &'static str> {
    if let Some(path) = std::env::var_os("COOLZHU_DEVIN_CLI") {
        let path = PathBuf::from(path);
        if !path.is_absolute() || !path.is_file() {
            return Err("COOLZHU_DEVIN_CLI 必须指向已安装 CLI 的绝对文件路径。");
        }
        #[cfg(windows)]
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err("Windows Devin 接入要求原生 .exe，不能通过脚本或 shell 包装启动。");
        }
        #[cfg(windows)]
        if is_desktop_entry(&path) {
            return Err("COOLZHU_DEVIN_CLI 指向 Devin 桌面入口；请改为安装目录 resources/app/extensions/windsurf/devin/bin/devin.exe 中的 CLI。");
        }
        return Ok(path);
    }
    let executable = if cfg!(windows) { "devin.exe" } else { "devin" };
    let path_binary = std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths)
        // 不接受相对 PATH 条目，避免当前工程放置同名程序被执行。
        .filter(|path| path.is_absolute()).map(|path| path.join(executable)).find(|path| {
            if !path.is_file() { return false; }
            #[cfg(windows)]
            if is_desktop_entry(path) { return false; }
            true
        }));
    if path_binary.is_some() {
        return path_binary.ok_or("Devin CLI 路径不可用。");
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA")
        .and_then(|root| bundled_cli(Path::new(&root)))
    {
        return Ok(path);
    }
    Err("未找到 Devin CLI。请先安装并登录 CLI，或设置 COOLZHU_DEVIN_CLI 的绝对路径，然后重启本地服务。")
}

pub(crate) async fn status() -> Json<Value> {
    let availability = binary();
    Json(serde_json::json!({
        "backend_kind": "devin_acp", "cli_available": availability.is_ok(),
        "version_pin_configured": std::env::var("COOLZHU_DEVIN_CLI_VERSION").ok().is_some_and(|v| !v.trim().is_empty()),
        "authentication": "not_checked", "agent_execution_ready": false,
        "reason": availability.err().unwrap_or("CLI 已找到；工具桥与本地执行隔离尚未验收，Agent 任务入口保持关闭。")
    }))
}

fn model_label(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= 1024 && !v.chars().any(char::is_control))
        .map(str::to_string)
}

pub(super) fn parse_catalog(value: &Value) -> Result<Vec<DiscoveredModel>, &'static str> {
    // 固定 CLI 3000.10.48 的真实目录按 families/variants 分组。
    let grouped;
    let entries = if let Some(families) = value.get("families") {
        let families = families.as_array().ok_or("Devin 模型家族目录格式无效。")?;
        grouped = families.iter().map(|family| family.get("variants").and_then(Value::as_array)
            .ok_or("Devin 模型家族缺少 variants，未接受部分目录。"))
            .collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect::<Vec<_>>();
        grouped
    } else { value
        .as_array()
        .or_else(|| value.get("models").and_then(Value::as_array))
        .ok_or("Devin CLI 模型目录结构不受支持。请保留 CLI 版本并检查适配器；没有采用猜测模型。")?
        .iter().collect::<Vec<_>>() };
    if entries.is_empty() {
        return Err("Devin CLI 没有返回可用模型，请检查账号权限。 ");
    }
    if entries.len() > MAX_MODELS {
        return Err("Devin 模型目录超过 5000 项限制。 ");
    }
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        let id = model_label(Some(entry))
            .or_else(|| model_label(entry.get("id")))
            .or_else(|| model_label(entry.get("model_id")))
            .or_else(|| model_label(entry.get("model_uid")))
            .ok_or("Devin CLI 模型目录包含无效 ID，未接受部分或推测结果。")?;
        if !seen.insert(id.clone()) {
            continue;
        }
        let name = model_label(entry.get("name"))
            .or_else(|| model_label(entry.get("display_name")))
            .or_else(|| model_label(entry.get("label")))
            .unwrap_or_else(|| id.clone());
        // 目录可访问只证明可发现；图片、采样和底模身份必须以实际 ACP 协商为准。
        models.push(DiscoveredModel {
            family: model_family(&id).to_string(),
            reasoning_effort: model_effort(&id).map(str::to_string),
            id,
            name,
            image_input: None,
            input_modalities: None,
            capability_source: "devin_cli_catalog",
            context_window: None,
            max_output_tokens: None,
            cost_tier: model_label(entry.get("cost_tier")),
            cost_summary: model_label(entry.get("cost_summary")),
        });
    }
    Ok(models)
}

// CLI 将思考档位编码在精确模型变体中；只在真实目录内查找同家族变体，不合成 ID。
pub(crate) fn model_effort(model: &str) -> Option<&str> {
    let (_, effort) = model.rsplit_once('-')?;
    matches!(effort, "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max").then_some(effort)
}

fn model_family(model: &str) -> &str {
    if model_effort(model).is_some() { model.rsplit_once('-').unwrap().0 } else { model }
}

pub(super) fn catalog_contains(value: &Value, model: &str) -> bool {
    parse_catalog(value).is_ok_and(|models| models.iter().any(|entry| entry.id == model))
}

pub(crate) async fn discover() -> super::super::ApiResult<Json<DiscoveryResponse>> {
    let bad = |message: &str| super::super::api_error(StatusCode::BAD_GATEWAY, message);
    static QUERIES: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    let _permit = QUERIES
        .get_or_init(|| tokio::sync::Semaphore::new(2))
        .try_acquire()
        .map_err(|_| {
            super::super::api_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Devin 模型查询正在进行，请稍后重试。",
            )
        })?;
    let binary = binary().map_err(bad)?;
    // 与真实工作区隔离，只调用官方只读子命令，不在用户工程里初始化会话。
    let sandbox = tempfile::Builder::new()
        .prefix("coolzhu-devin-discovery-")
        .tempdir()
        .map_err(|_| bad("无法准备 Devin 查询临时目录。"))?;
    let version = super::transport::run_readonly(
        &binary,
        &["--version"],
        sandbox.path(),
        Duration::from_secs(5),
    )
    .await
    .map_err(bad)?;
    let version = std::str::from_utf8(&version)
        .ok()
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| bad("无法识别 Devin CLI 版本；未继续查询。"))?
        .to_string();
    let pinned = std::env::var("COOLZHU_DEVIN_CLI_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty());
    if pinned.as_ref().is_some_and(|pin| pin.trim() != version) {
        return Err(bad(
            "Devin CLI 版本与 COOLZHU_DEVIN_CLI_VERSION 固定值不一致，未继续查询。",
        ));
    }
    let bytes = super::transport::run_readonly(
        &binary,
        &["models", "list", "--format", "json"],
        sandbox.path(),
        Duration::from_secs(20),
    )
    .await
    .map_err(bad)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| bad("Devin CLI 未返回合法 JSON 模型目录；请检查固定版本。"))?;
    let models = parse_catalog(&value).map_err(bad)?;
    let text_chat_ready = cfg!(windows) && version == super::chat::CLI_VERSION;
    let mut warnings = vec!["可选择账号目录中的模型及思考变体；每轮发送前核对目录与 ACP 生效模型。费用按 Devin 账号计费。工具、附件、Goal 和子 Agent 尚未开放。".into()];
    if pinned.is_none() {
        warnings.push("CLI 版本尚未固定。完成验证后将 COOLZHU_DEVIN_CLI_VERSION 设置为此次 cli_version 的完整值。".into());
    }
    Ok(Json(DiscoveryResponse {
        models,
        provider_hint: "Devin",
        backend_kind: super::super::agent_session_backend::AgentSessionBackend::DevinAcp,
        discovery_source: "devin models list --format json",
        cli_version: version,
        version_pinned: pinned.is_some(),
        authentication: "catalog_access_confirmed",
        checked_at: super::super::unix_timestamp_millis(),
        complete: true,
        agent_execution_ready: false,
        text_chat_ready,
        warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn real_cli_family_variants_preserve_free_tier_and_exact_model_uid() {
        let models = parse_catalog(&json!({"families":[{"family_label":"SWE-2","variants":[
            {"model_uid":"swe-2-high","label":"SWE-2 High","cost_tier":"Free","max_context_tokens":262000},
            {"model_uid":"swe-2-medium","label":"SWE-2 Medium","cost_tier":"Free"}]}]})).unwrap();
        assert_eq!(models.len(), 2); assert_eq!(models[0].id, "swe-2-high");
        assert_eq!(models[0].name, "SWE-2 High"); assert_eq!(models[0].cost_tier.as_deref(), Some("Free"));
        assert!(models[0].context_window.is_none());
        assert!(parse_catalog(&json!({"families":[{"variants":[{"model_uid":"ok"}]},{}]})).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn installed_desktop_discovery_finds_only_the_bundled_native_cli() {
        let local = tempfile::tempdir().unwrap();
        assert!(bundled_cli(local.path()).is_none());
        assert!(bundled_cli(Path::new("relative-local-data")).is_none());
        let desktop = local.path().join("Programs/Devin");
        std::fs::create_dir_all(desktop.join("resources/app/out")).unwrap();
        std::fs::write(desktop.join("resources/app/out/main.js"), "").unwrap();
        std::fs::write(desktop.join("chrome_100_percent.pak"), "").unwrap();
        std::fs::write(desktop.join("Devin.exe"), "").unwrap();
        assert!(is_desktop_entry(&desktop.join("Devin.exe")));
        assert!(bundled_cli(local.path()).is_none());
        let cli = desktop.join("resources/app/extensions/windsurf/devin/bin/devin.exe");
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "").unwrap();
        assert_eq!(bundled_cli(local.path()), Some(cli.clone()));
        assert!(!is_desktop_entry(&cli));
    }
    #[test]
    fn catalog_uses_actual_ids_deduplicates_and_does_not_invent_capabilities() {
        let models = parse_catalog(&json!({"models":[{"id":"alias-a","name":"实际选项"},{"id":"alias-a"},{"model_id":"route-b"}]})).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "实际选项");
        assert!(
            models
                .iter()
                .all(|m| m.image_input.is_none() && m.context_window.is_none())
        );
    }
    #[test]
    fn invalid_unknown_and_partial_catalogs_are_not_presented_as_verified() {
        for value in [
            json!({"families":{"some-model":{}}}),
            json!([]),
            json!([{"id":"valid"},{"name":"unknown"}]),
            json!(["bad\nmodel"]),
        ] {
            assert!(parse_catalog(&value).is_err());
        }
        assert_eq!(parse_catalog(&json!(["alias"])).unwrap()[0].id, "alias");
    }
}
