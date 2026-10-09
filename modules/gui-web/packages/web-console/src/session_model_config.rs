//! 会话参数的只读规则；不读取全局配置、不持久化、不决定HTTP状态或工具执行权限。
use super::{agent_session_backend, AgentSessionDto, request_max_tokens_for_limit};
use api::ProviderKind;
use serde::{Deserialize, Serialize};

/// custom provider 会话的 token 限制覆盖项（按会话 id 生效）。字段为 0 表示该项不覆盖、沿用默认表。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct SessionModelLimitOverride {
    #[serde(default)]
    pub(super) backend_kind: Option<agent_session_backend::AgentSessionBackend>,
    #[serde(default)]
    pub(super) context_window: u32,
    #[serde(default)]
    pub(super) max_output_tokens: u32,
    #[serde(default)]
    pub(super) protocol: Option<String>,
    #[serde(default)]
    pub(super) base_url: Option<String>,
    #[serde(default)]
    pub(super) endpoint: Option<String>,
    #[serde(default)]
    pub(super) temperature: Option<f64>,
    #[serde(default)]
    pub(super) top_p: Option<f64>,
    #[serde(default)]
    pub(super) reasoning_mode: Option<String>,
    #[serde(default)]
    pub(super) thinking_budget: Option<u32>,
    #[serde(default)]
    pub(super) turn_timeout_ms: Option<u64>,
    #[serde(default)]
    pub(super) supports_multimodal: Option<bool>,
    #[serde(default)]
    pub(super) enable_llm_tools: Option<bool>,
    #[serde(default)]
    pub(super) llm_tool_exposure: Option<String>,
    #[serde(default)]
    pub(super) computer_use_enabled: Option<bool>,
    #[serde(default)]
    pub(super) tool_allowlist: Option<Vec<String>>,
}

pub(super) fn model_settings_protocol(agent: &AgentSessionDto, settings: &SessionModelLimitOverride) -> &'static str {
    match agent_session_backend::AgentSessionBackend::for_provider(&agent.provider) {
        agent_session_backend::AgentSessionBackend::DevinAcp => return "devin_acp",
        agent_session_backend::AgentSessionBackend::DevinCloud => return "devin_cloud",
        agent_session_backend::AgentSessionBackend::LlmHttp => {}
    }
    match settings.protocol.as_deref() {
        Some("anthropic_messages") => "anthropic_messages",
        Some("openai_chat_completions") => "openai_chat_completions",
        _ => match api::provider_kind_from_name(&agent.provider) {
            Some(ProviderKind::Anthropic | ProviderKind::ClawApi) => "anthropic_messages",
            _ => "openai_chat_completions",
        },
    }
}

pub(super) fn model_settings_base_url(agent: &AgentSessionDto, settings: &SessionModelLimitOverride) -> String {
    if agent_session_backend::AgentSessionBackend::for_provider(&agent.provider) != agent_session_backend::AgentSessionBackend::LlmHttp { return String::new(); }
    settings.base_url.as_deref().filter(|value| !value.trim().is_empty())
        .or(agent.base_url.as_deref().filter(|value| !value.trim().is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| {
            let kind = api::provider_kind_from_name(&agent.provider)
                .unwrap_or_else(|| api::detect_provider_kind(&agent.model));
            api::ModelRegistry::global().resolve_model_for_provider(&agent.model, kind).base_url
        })
}

pub(super) fn validate_http_parameters(settings: &SessionModelLimitOverride, agent: &AgentSessionDto) -> Result<(), String> {
    let invalid = |message: &str| message.to_owned();
    if !matches!(settings.protocol.as_deref(), None | Some("openai_chat_completions" | "anthropic_messages")) {
        return Err(invalid("不支持该模型协议"));
    }
    if settings.turn_timeout_ms.is_some_and(|value| !(60_000..=86_400_000).contains(&value)) {
        return Err(invalid("每轮任务总时限必须为 1 分钟至 24 小时"));
    }
    let anthropic = model_settings_protocol(agent, settings) == "anthropic_messages";
    let mode = settings.reasoning_mode.as_deref().unwrap_or("auto");
    if !matches!(mode, "auto" | "effort" | "thinking" | "budget" | "adaptive")
        || (anthropic && matches!(mode, "effort" | "thinking"))
        || (!anthropic && matches!(mode, "budget" | "adaptive")) {
        return Err(invalid("思考参数编码与选择的协议不匹配"));
    }
    if settings.context_window > 4_000_000 || settings.max_output_tokens > 1_000_000 {
        return Err(invalid("上下文容量上限为 4000000，最大输出上限为 1000000"));
    }
    let defaults = api::model_token_limit(&agent.model);
    let ctx = if settings.context_window > 0 { settings.context_window } else { defaults.context_tokens };
    let out = if settings.max_output_tokens > 0 { settings.max_output_tokens } else { defaults.max_output_tokens };
    if settings.max_output_tokens > 0 && out > ctx {
        return Err(invalid("最大输出不能超过上下文容量"));
    }
    if settings.temperature.is_some_and(|value| !value.is_finite() || value < 0.0 || value > if anthropic { 1.0 } else { 2.0 }) {
        return Err(invalid("温度必须在协议允许的范围内：OpenAI 0–2，Anthropic 0–1"));
    }
    if settings.top_p.is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 1.0) {
        return Err(invalid("Top P 必须大于 0 且不超过 1"));
    }
    if anthropic && settings.temperature.is_some() && settings.top_p.is_some() {
        return Err(invalid("Anthropic 请只设置温度或 Top P 中的一项"));
    }
    let auto_anthropic_thinking = mode == "auto" && anthropic && matches!(
        api::resolve_legacy_reasoning("clawapi", &agent.model, Some(&agent.reasoning_effort)).preflight_wire,
        api::ReasoningWire::AnthropicAdaptive { .. }
    );
    if anthropic && ((matches!(mode, "budget" | "adaptive") && agent.reasoning_effort != "none") || auto_anthropic_thinking)
        && (settings.temperature.is_some() || settings.top_p.is_some()) {
        return Err(invalid("开启 Anthropic 思考时请将采样参数留空，使用模型默认值"));
    }
    if mode == "adaptive" && !matches!(agent.reasoning_effort.as_str(), "auto" | "none" | "low" | "medium" | "high" | "max") {
        return Err(invalid("Adaptive 思考支持自动、关闭、低、中、高、最大"));
    }
    if mode == "budget" && agent.reasoning_effort != "none" {
        let budget = settings.thinking_budget.unwrap_or_default();
        if budget < 1024 || budget >= request_max_tokens_for_limit((ctx, out)) {
            return Err(invalid("思考预算至少为 1024，并且必须小于本轮实际输出预算（受上下文预留限制）"));
        }
    }
    if !matches!(settings.llm_tool_exposure.as_deref(), None | Some("all" | "whitelist" | "dispatch-only")) {
        return Err(invalid("工具范围必须为 all、whitelist 或 dispatch-only"));
    }
    if let Some(list) = &settings.tool_allowlist {
        if list.len() > 256 || list.iter().any(|name| name.trim().is_empty() || name.len() > 128) {
            return Err(invalid("工具清单最多 256 项，工具名称不能为空且不能超过 128 字节"));
        }
    }
    let base = model_settings_base_url(agent, settings);
    let url = reqwest::Url::parse(&base).map_err(|_| invalid("接口地址必须是完整的 HTTP 或 HTTPS URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("接口地址须使用 HTTP/HTTPS；密钥请填写在独立的 API Key 输入框中"));
    }
    if let Some(endpoint) = settings.endpoint.as_deref().filter(|value| !value.trim().is_empty()) {
        if endpoint.contains("://") {
            let endpoint_url = reqwest::Url::parse(endpoint).map_err(|_| invalid("Endpoint 地址无效"))?;
            if !matches!(endpoint_url.scheme(), "http" | "https") || endpoint_url.host_str().is_none()
                || !endpoint_url.username().is_empty() || endpoint_url.password().is_some() {
                return Err(invalid("Endpoint 须使用 HTTP/HTTPS 且不能包含密钥"));
            }
        }
    }
    Ok(())
}

pub(super) fn constrain_session_model_limit(limit: (u32, u32), settings: &SessionModelLimitOverride) -> (u32, u32) {
    // 本地服务上限优先，但用户选择更小的预算仍必须生效。
    (
        if settings.context_window > 0 { limit.0.min(settings.context_window) } else { limit.0 },
        if settings.max_output_tokens > 0 { limit.1.min(settings.max_output_tokens) } else { limit.1 },
    )
}

/// 使用已捕获的参数计算容量，避免同一次响应再次读取可变配置。
pub(super) fn model_limit_for_settings(model: &str, settings: &SessionModelLimitOverride) -> (u32, u32) {
    let defaults = api::model_token_limit(model);
    (
        if settings.context_window > 0 { settings.context_window } else { defaults.context_tokens },
        if settings.max_output_tokens > 0 { settings.max_output_tokens } else { defaults.max_output_tokens },
    )
}
