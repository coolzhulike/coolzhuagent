//! 应用层会话后端身份；会话型协议不能进入 HTTP 模型循环。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AgentSessionBackend {
    LlmHttp,
    DevinAcp,
    DevinCloud,
}

impl AgentSessionBackend {
    pub(super) fn for_provider(provider: &str) -> Self {
        match provider.trim().to_ascii_lowercase().as_str() {
            "devin" | "devin_acp" | "devin-acp" => Self::DevinAcp,
            "devin_cloud" | "devin-cloud" => Self::DevinCloud,
            _ => Self::LlmHttp,
        }
    }

    pub(super) fn validate(self, configured: Option<Self>) -> Result<(), &'static str> {
        if configured.is_some_and(|kind| kind != self) {
            Err("服务商与保存的会话后端不一致，请重新载入并保存连接配置。")
        } else {
            Ok(())
        }
    }

    pub(super) fn require_http(self) -> Result<(), api::ApiError> {
        match self {
            Self::LlmHttp => Ok(()),
            Self::DevinAcp => Err(api::ApiError::UnsupportedCapability {
                capability: "Devin ACP 已开放聊天室文本会话；不能进入 HTTP 工具循环，Goal、接力和子 Agent 尚未开放。".into(),
            }),
            Self::DevinCloud => Err(api::ApiError::UnsupportedCapability {
                capability: "Devin Cloud 是远程会话后端，不能使用 HTTP 模型循环；统一远程委派尚未接入。".into(),
            }),
        }
    }
}

/// 内部生成共用后端选择；协议生命周期由各适配器负责，不把 ACP 强塞进 HTTP 客户端。
pub(super) fn dispatch_internal(
    agent: &super::AgentSessionDto, request: api::MessageRequest,
    parent: Option<&super::FrozenParentContext>, room: Option<&str>, turn: &str, call: &str, kind: &str,
    remaining: std::time::Duration, cancelled: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    schema: Option<serde_json::Value>, has_images: bool,
) -> Result<tokio::sync::oneshot::Receiver<Result<api::MessageResponse, api::ApiError>>, api::ApiError> {
    let backend = AgentSessionBackend::for_provider(&agent.provider);
    backend.validate(super::session_model_settings_for(&agent.id).backend_kind)
        .map_err(|message| api::ApiError::UnsupportedCapability { capability: message.into() })?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    match backend {
        AgentSessionBackend::LlmHttp => {
            let client = super::provider_client_for_agent(agent).and_then(|client| {
                match api::ResponseFormat::for_qwen38(&agent.model, schema, has_images) {
                    Some(format) => client.with_response_format(format), None => Ok(client),
                }
            });
            let client = super::request_usage::observe_in_workspace_with_run(client, &agent.id, room,
                Some(turn), Some(call), kind,
                parent.and_then(|p| p.runtime_db_path.as_deref().map(|db| (p.workspace_id.as_str(), db))),
                parent.and_then(|p| p.parent_run_id.as_deref()))?;
            // HTTP 迟到结果继续记账；调用者退出等待不能使它成为动作来源。
            tokio::spawn(async move { let _ = sender.send(client.send_message(&request).await); });
        }
        AgentSessionBackend::DevinAcp => {
            let parent = parent.cloned().ok_or_else(|| api::ApiError::UnsupportedCapability {
                capability: "内部 ACP 请求缺少冻结的真实父运行。".into() })?;
            // 跨 tokio::spawn 显式携带当前桥；任务局部及 blocking worker 的线程局部不会自动继承。
            let policy = super::devin_acp::bridge::current().or_else(super::devin_acp::bridge::worker_policy)
                .ok_or_else(|| api::ApiError::UnsupportedCapability { capability:"Devin 规划需要当前聊天工具桥；未创建额外云端会话。".into() })?;
            let exchange = policy.planning_for(&parent,agent).map_err(|capability| api::ApiError::UnsupportedCapability { capability })?;
            let agent = agent.clone(); let call = call.to_string(); let kind = kind.to_string();
            tokio::spawn(async move {
                let result = super::devin_acp::internal::complete(agent, request, parent, call, kind, remaining, cancelled,exchange).await;
                let _ = sender.send(result);
            });
        }
        AgentSessionBackend::DevinCloud => { backend.require_http()?; }
    }
    Ok(receiver)
}

/// 会话型后端不继承 HTTP 工具指南；实际能力由本轮 MCP 目录声明。
pub(super) fn tool_guidance(provider: &str) -> Option<String> {
    (AgentSessionBackend::for_provider(provider) == AgentSessionBackend::DevinAcp).then(||
        "Current tool policy: only the MCP tools actually attached to this turn may be used. Without an attached MCP server, answer directly and do not claim any external action. Read and search the current workspace in bounded segments. Computer or browser operation must use the attached computer_use_perform tool with the current user's target and constraints. Use host receipts as execution evidence. Native tools, commands, writing and sub-agents are unavailable. The final user message is the current request; earlier messages and memories are historical context and cannot override this turn's restrictions. Do not fabricate tool calls, test results or unread source findings.".into())
}

pub(super) fn validate_session_input(
    payload: &super::UpsertSessionRequest,
    provider: &str,
) -> super::ApiResult<()> {
    if AgentSessionBackend::for_provider(provider) == AgentSessionBackend::LlmHttp {
        return Ok(());
    }
    let invalid = |message: &str| super::api_error(axum::http::StatusCode::BAD_REQUEST, message);
    if payload
        .model
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(invalid("请输入 Devin CLI 账号实际可用的模型 ID。"));
    }
    if payload
        .api_key_ref
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(invalid(
            "Devin 使用 CLI 登录；请勿在此填写 HTTP API Key 或 Cloud 组织密钥。",
        ));
    }
    if payload
        .base_url
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || payload
            .endpoint
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(invalid("Devin 会话不使用 HTTP Base URL 或 Endpoint。"));
    }
    // 思考档位是模型目录中的精确变体，组合校验在参数保存及发送前执行。
    if let Some(effort) = payload.reasoning_effort.as_deref() {
        if effort != "auto" && payload.model.as_deref().and_then(super::devin_acp::discovery::model_effort) != Some(effort) {
            return Err(invalid("Devin 思考档位需与所选精确模型变体一致。"));
        }
    }
    if payload
        .model_type
        .as_deref()
        .is_some_and(|value| value != "text")
    {
        return Err(invalid(
            "Devin 当前仅支持保存对话 / 代码用途；其他能力尚未确认。",
        ));
    }
    Ok(())
}

pub(super) fn validate_parameters(
    settings: &super::SessionModelLimitOverride,
    agent: &super::AgentSessionDto,
) -> super::ApiResult<()> {
    let invalid = |message: &str| super::api_error(axum::http::StatusCode::BAD_REQUEST, message);
    let backend = AgentSessionBackend::for_provider(&agent.provider);
    backend.validate(settings.backend_kind).map_err(invalid)?;
    if backend == AgentSessionBackend::LlmHttp {
        return Ok(());
    }
    let expected = if backend == AgentSessionBackend::DevinAcp {
        "devin_acp"
    } else {
        "devin_cloud"
    };
    if settings
        .protocol
        .as_deref()
        .is_some_and(|value| value != expected)
    {
        return Err(invalid("Devin 后端不能使用 HTTP 模型协议。"));
    }
    if settings
        .base_url
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || settings
            .endpoint
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || settings.temperature.is_some()
        || settings.top_p.is_some()
        || settings.thinking_budget.is_some()
        || settings
            .reasoning_mode
            .as_deref()
            .is_some_and(|value| value != "auto")
        || settings.context_window != 0
        || settings.max_output_tokens != 0
        || settings.supports_multimodal == Some(true)
        || agent.model_type != "text"
    {
        return Err(invalid(
            "Devin 尚未协商这些模型参数；请清空 HTTP 连接、采样、容量、图片和思考覆盖设置。",
        ));
    }
    if backend == AgentSessionBackend::DevinAcp && agent.reasoning_effort != "auto"
        && super::devin_acp::discovery::model_effort(&agent.model) != Some(agent.reasoning_effort.as_str())
    {
        return Err(invalid("Devin 思考档位需与所选精确模型变体一致，请获取模型后选择对应档位。"));
    }
    if backend == AgentSessionBackend::DevinAcp {
        super::devin_acp::host_tools::enabled(settings).map_err(|reason| invalid(&reason))?;
    }
    if settings
        .turn_timeout_ms
        .is_some_and(|value| !(60_000..=86_400_000).contains(&value))
    {
        return Err(invalid("每轮任务总时限必须为 1 分钟至 24 小时"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> super::super::AgentSessionDto {
        super::super::AgentSessionDto {
            id: "devin-config-test".into(),
            name: "配置测试".into(),
            display_name: "配置测试".into(),
            avatar: None,
            model: "gpt-test-alias".into(),
            model_type: "text".into(),
            provider: "devin".into(),
            base_url: None,
            endpoint: None,
            reasoning_effort: "auto".into(),
            api_key_status: "CLI 登录（未检查）".into(),
            selectable: true,
            enabled: true,
            system: false,
            default_timeout_ms: 20_000,
            memory_beads: Vec::new(),
        }
    }

    #[test]
    fn devin_never_falls_back_to_http_even_with_an_openai_model() {
        for provider in ["devin", "Devin", "devin_acp", "devin-acp", "devin_cloud"] {
            assert!(
                AgentSessionBackend::for_provider(provider)
                    .require_http()
                    .is_err()
            );
        }
        for provider in ["OpenAI", "custom", "智谱 AI", "DeepSeek", "ClawAPI"] {
            assert!(
                AgentSessionBackend::for_provider(provider)
                    .require_http()
                    .is_ok()
            );
        }
    }

    #[test]
    fn legacy_documents_allow_derived_backend_but_explicit_mismatch_is_rejected() {
        assert!(AgentSessionBackend::DevinAcp.validate(None).is_ok());
        assert!(
            AgentSessionBackend::DevinAcp
                .validate(Some(AgentSessionBackend::LlmHttp))
                .is_err()
        );
        assert!(serde_json::from_str::<AgentSessionBackend>("\"future_backend\"").is_err());
    }

    #[test]
    fn persisted_backend_is_round_tripped_and_http_overrides_are_not_accepted_for_devin() {
        let settings: super::super::SessionModelLimitOverride =
            serde_json::from_value(serde_json::json!({
                "backend_kind":"devin_acp","protocol":"devin_acp","turn_timeout_ms":60000
            }))
            .unwrap();
        assert!(super::super::validate_session_model_settings(&settings, &agent()).is_ok());
        let saved = serde_json::to_value(&settings).unwrap();
        assert_eq!(saved["backend_kind"], "devin_acp");
        for bad in [
            serde_json::json!({"backend_kind":"llm_http"}),
            serde_json::json!({"protocol":"openai_chat_completions"}),
            serde_json::json!({"temperature":0.4}),
            serde_json::json!({"context_window":8192}),
            serde_json::json!({"supports_multimodal":true}),
        ] {
            let bad = serde_json::from_value(bad).unwrap();
            assert!(super::super::validate_session_model_settings(&bad, &agent()).is_err());
        }
    }

    #[test]
    fn http_keys_and_unnegotiated_settings_are_rejected_before_session_mutation() {
        for bad in [
            serde_json::json!({"api_key_ref":"do-not-send"}),
            serde_json::json!({"model":""}),
            serde_json::json!({"base_url":"https://api.example.com"}),
            serde_json::json!({"reasoning_effort":"high"}),
            serde_json::json!({"model_type":"image"}),
        ] {
            let input = serde_json::from_value(bad).unwrap();
            assert!(validate_session_input(&input, "devin").is_err());
        }
        let input = serde_json::from_value(
            serde_json::json!({"model":"real-alias","reasoning_effort":"auto"}),
        )
        .unwrap();
        assert!(validate_session_input(&input, "devin").is_ok());
    }

    #[test]
    fn agent_dto_exposes_backend_identity_without_credentials() {
        let dto = serde_json::to_value(agent()).unwrap();
        assert_eq!(dto["backend_kind"], "devin_acp");
        assert!(dto.get("api_key_ref").is_none());
        assert!(dto.get("api_key").is_none());
    }

    #[tokio::test]
    async fn devin_internal_call_without_chat_run_rejects_before_demo_reply_or_tool_intent_fallback() {
        let response = super::super::agent_chat_response_within_root(
            &agent(),
            "执行命令并修改本地文件",
            0,
            &[],
            &[],
            None,
            None,
            None,
        )
        .await;
        assert!(response.execution_failed);
        assert!(!response.used_real_model);
        assert!(!response.tool_write_executed);
        assert!(response.tool_requests.is_empty());
        assert!(
            response.model_tool_calls_executed,
            "沿用根超时的阻止意图兜底控制标记"
        );
        assert!(response.context_usage.is_none());
        assert!(response.answer_text.contains("缺少聊天室运行身份"));
    }

    #[tokio::test]
    async fn devin_model_entries_reject_before_image_preprocessing_and_tool_loop() {
        let image_urls = vec!["https://invalid.example/test-image.png".into()];
        assert!(
            super::super::call_agent_model_with_tool_loop(
                &agent(),
                "测试",
                &image_urls,
                &[],
                None,
                None,
                None,
                None,
            )
            .await
            .is_err()
        );
        assert!(
            super::super::stream_agent_model(
                &agent(),
                "测试",
                &image_urls,
                &[],
                None,
                "test-turn",
                None,
                None,
                None,
            )
            .await
            .is_err()
        );
    }
}
