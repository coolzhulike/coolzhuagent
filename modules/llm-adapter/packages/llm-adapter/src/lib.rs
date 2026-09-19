mod client;
mod config;
mod embeddings;
mod error;
mod model_info;
mod providers;
mod registry;
mod reasoning;
mod request_parameters;
mod resolver;
mod sse;
mod types;

pub use client::{
    oauth_token_is_expired, read_base_url, read_xai_base_url, read_zhipu_base_url,
    resolve_saved_oauth_token, resolve_startup_auth_source, MessageStream, OAuthTokenSet,
    ProviderClient,
};
pub use config::{load_config, AdapterConfig, ModelConfig, ProviderConfig};
pub use embeddings::embed_texts;
pub use error::ApiError;
pub use model_info::{
    Modality, ModelCost, ModelInfo, ModelLimit, ModelModalities, ModelStatus, ProviderInfo,
};
pub use providers::claw_provider::{
    build_anthropic_messages_request, AuthSource, ClawApiClient, ClawApiClient as ApiClient,
};
pub use providers::openai_compat::{
    build_chat_completion_request_for, OpenAiCompatClient, OpenAiCompatConfig,
};
pub use providers::{
    context_tokens_for_model, detect_provider_kind, max_tokens_for_model, model_token_limit,
    provider_catalog, provider_kind_from_name, provider_option, resolve_model_alias,
    ModelTokenLimit, ProviderKind, ProviderMetadata, ProviderOption,
};
pub use registry::{ModelRegistry, ResolvedModel};
pub use request_parameters::RequestParameters;
pub use reasoning::{
    parse_legacy_reasoning_effort, parse_reasoning_effort, reasoning_capability_catalog,
    reasoning_capability_for_provider, reasoning_model_aliases, resolve_legacy_reasoning,
    resolve_reasoning,
    LegacyReasoningValue, ReasoningCapability, ReasoningCapabilityStatus, ReasoningEffort,
    ReasoningOption, ReasoningParseError, ReasoningResolution, ReasoningResolutionStatus,
    ReasoningStrategy, ReasoningWire,
};
pub use resolver::{
    AuthPolicy, EndpointResolver, ProviderProtocol, RequestCapability, ResolvedProviderRoute,
};
pub use sse::{parse_frame, SseParser};
pub use types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

#[cfg(test)]
mod resolver_contract_tests {
    use crate::{EndpointResolver, ProviderProtocol, RequestCapability};

    #[test]
    fn resolver_deduplicates_anthropic_messages_endpoint() {
        assert_eq!(
            EndpointResolver::resolve(
                "https://api.anthropic.com/v1",
                ProviderProtocol::AnthropicMessages,
                None
            )
            .expect("anthropic endpoint should resolve"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            EndpointResolver::resolve(
                "https://api.anthropic.com/v1/messages",
                ProviderProtocol::AnthropicMessages,
                None
            )
            .expect("full anthropic endpoint should be stable"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn resolver_supports_openai_base_host_base_path_and_full_endpoint() {
        assert_eq!(
            EndpointResolver::resolve(
                "https://api.openai.example",
                ProviderProtocol::OpenAiChatCompletions,
                None
            )
            .expect("host should resolve"),
            "https://api.openai.example/v1/chat/completions"
        );
        assert_eq!(
            EndpointResolver::resolve(
                "https://api.openai.example/v1",
                ProviderProtocol::OpenAiChatCompletions,
                None
            )
            .expect("base path should resolve"),
            "https://api.openai.example/v1/chat/completions"
        );
        assert_eq!(
            EndpointResolver::resolve(
                "https://api.openai.example/v1/chat/completions",
                ProviderProtocol::OpenAiChatCompletions,
                None
            )
            .expect("full endpoint should be stable"),
            "https://api.openai.example/v1/chat/completions"
        );
    }

    /// 锁定线上真实 base_url 常量：多数常量自带版本段（/v1、/v2、/v3、/v4），
    /// resolver 只能补终端路径；仅当 base_url 是裸主机时才注入 v1/。
    #[test]
    fn resolver_appends_version_segment_at_most_once_for_shipped_base_urls() {
        use crate::providers::openai_compat::{
            DEFAULT_ALIBABA_BASE_URL, DEFAULT_BAIDU_BASE_URL, DEFAULT_BYTEDANCE_BASE_URL,
            DEFAULT_CUSTOM_BASE_URL, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_OPENAI_BASE_URL,
            DEFAULT_XAI_BASE_URL, DEFAULT_ZHIPU_BASE_URL,
        };

        let cases = [
            (DEFAULT_XAI_BASE_URL, "https://api.x.ai/v1/chat/completions"),
            (
                DEFAULT_OPENAI_BASE_URL,
                "https://api.openai.com/v1/chat/completions",
            ),
            (
                DEFAULT_ZHIPU_BASE_URL,
                "https://open.bigmodel.cn/api/paas/v4/chat/completions",
            ),
            (
                DEFAULT_ALIBABA_BASE_URL,
                "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
            ),
            (
                DEFAULT_BAIDU_BASE_URL,
                "https://qianfan.baidubce.com/v2/chat/completions",
            ),
            (
                DEFAULT_BYTEDANCE_BASE_URL,
                "https://ark.cn-beijing.volces.com/api/v3/chat/completions",
            ),
            // 裸主机：resolver 补上 OpenAI 标准的 /v1 挂载点
            (
                DEFAULT_DEEPSEEK_BASE_URL,
                "https://api.deepseek.com/v1/chat/completions",
            ),
            (
                DEFAULT_CUSTOM_BASE_URL,
                "http://127.0.0.1:11434/v1/chat/completions",
            ),
        ];

        for (base_url, expected) in cases {
            let resolved =
                EndpointResolver::resolve(base_url, ProviderProtocol::OpenAiChatCompletions, None)
                    .expect("shipped base url should resolve");
            assert_eq!(resolved, expected, "base_url={base_url}");
            assert!(
                !resolved.contains("/v1/v1"),
                "版本段重复拼接 base_url={base_url}: {resolved}"
            );
            // 已解析出的完整 endpoint 再次解析必须幂等
            assert_eq!(
                EndpointResolver::resolve(
                    &resolved,
                    ProviderProtocol::OpenAiChatCompletions,
                    None
                )
                .expect("resolved endpoint should be stable"),
                resolved,
                "重复解析不幂等 base_url={base_url}"
            );
        }
    }

    #[test]
    fn resolver_deduplicates_media_endpoints_and_rejects_unsupported_capability() {
        assert_eq!(
            EndpointResolver::resolve(
                "https://api.openai.example/v1/images/generations",
                ProviderProtocol::OpenAiImagesGenerations,
                None
            )
            .expect("image endpoint should be stable"),
            "https://api.openai.example/v1/images/generations"
        );

        let error = EndpointResolver::protocol_for_capability(RequestCapability::Audio);
        assert!(error.is_err());
    }
}
