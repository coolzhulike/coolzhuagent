//! ACP 使用官方 CLI 登录；健康提示与 HTTP 密钥及模型名称推断分开。
use super::auth::{self, Authentication};

fn auth_labels(authentication: Authentication) -> (&'static str, &'static str, Option<&'static str>) {
    match authentication {
        Authentication::Authenticated => ("ok", "Devin 已登录（最近确认）", None),
        Authentication::Unauthenticated => ("error", "Devin 尚未登录", Some("请在模型配置中完成 Devin 官方登录。")),
        Authentication::Unavailable => ("error", "Devin 登录组件不可用", Some("请检查 Devin CLI 安装并刷新登录状态。")),
        Authentication::CheckFailed => ("warn", "Devin 登录状态核对失败", Some("暂时无法核对登录状态，请从模型配置刷新；这不代表已注销。")),
        Authentication::Unknown => ("warn", "Devin 登录状态未确认", Some("请从模型配置刷新 Devin 登录状态；无需填写 API Key。")),
    }
}

pub(crate) fn key_label() -> String {
    auth_labels(auth::cached_authentication()).1.to_string()
}

pub(crate) fn for_agent(id: &str, provider: &str, model: &str) -> crate::AgentDiagnosticsResponse {
    let (status, label, issue) = auth_labels(auth::cached_authentication());
    crate::AgentDiagnosticsResponse {
        agent_id: id.into(), provider_label: provider.into(),
        provider_kind: "devin_acp".into(), selected_provider_kind: Some("devin_acp".into()),
        detected_provider_kind: "devin_acp".into(), provider_match: true, provider_slug: "devin".into(),
        model: model.into(), canonical_model: model.into(), api_model_id: model.into(),
        // ACP 地址由官方 CLI 管理，不能拿 Anthropic/OpenAI 的默认 HTTP 地址比较。
        base_url: String::new(), base_url_default: None, base_url_env: None,
        base_url_env_present: false, base_url_source: "official-cli".into(), base_url_matches_default: true,
        // 不把已登录伪装成存在密钥，也不继承无关 HTTP Provider 的环境变量。
        api_key_present: false, api_key_source: None, api_key_status: label.into(), env_keys: Vec::new(),
        recommended_models: Vec::new(), provider_status: status.into(), provider_issue: issue.map(str::to_string),
    }
}
