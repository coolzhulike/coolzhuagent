use crate::error::ApiError;
use crate::providers::claw_provider::{self, AuthSource, ClawApiClient};
use crate::providers::openai_compat::{self, OpenAiCompatClient, OpenAiCompatConfig};
use crate::providers::{Provider, ProviderKind};
use crate::registry::{ModelRegistry, ResolvedModel};
use crate::types::{MessageRequest, MessageResponse, StreamEvent};

async fn send_via_provider<P: Provider>(
    provider: &P,
    request: &MessageRequest,
) -> Result<MessageResponse, ApiError> {
    provider.send_message(request).await
}

async fn stream_via_provider<P: Provider>(
    provider: &P,
    request: &MessageRequest,
) -> Result<P::Stream, ApiError> {
    provider.stream_message(request).await
}

#[derive(Debug, Clone)]
pub enum ProviderClient {
    ClawApi(ClawApiClient),
    Anthropic(ClawApiClient),
    Xai(OpenAiCompatClient),
    OpenAi(OpenAiCompatClient),
    ZhipuAi(OpenAiCompatClient),
    AlibabaBailian(OpenAiCompatClient),
    #[deprecated(since = "0.2.0", note = "Use AlibabaBailian instead")]
    AlibabaCloud(OpenAiCompatClient),
    BaiduQianfan(OpenAiCompatClient),
    ByteDanceArk(OpenAiCompatClient),
    DeepSeek(OpenAiCompatClient),
    Custom(OpenAiCompatClient),
}

impl ProviderClient {
    /// 按显式协议构造会话连接，不要求模型提前进入内置目录。
    pub fn from_session_endpoint(
        model: &str,
        endpoint: &str,
        anthropic: bool,
        legacy_provider: ProviderKind,
        api_key: Option<String>,
    ) -> Result<Self, ApiError> {
        if anthropic {
            let auth = api_key.filter(|value| !value.trim().is_empty())
                .map(AuthSource::ApiKey).unwrap_or(AuthSource::None);
            Ok(Self::Anthropic(ClawApiClient::from_auth(auth)
                .with_endpoint(endpoint).with_model_alias(model, model)))
        } else {
            let config = match legacy_provider {
                ProviderKind::OpenAi => OpenAiCompatConfig::openai(),
                ProviderKind::Xai => OpenAiCompatConfig::xai(),
                ProviderKind::ZhipuAi => OpenAiCompatConfig::zhipu(),
                ProviderKind::DeepSeek => OpenAiCompatConfig::deepseek(),
                ProviderKind::AlibabaBailian => OpenAiCompatConfig::alibaba_bailian(),
                ProviderKind::BaiduQianfan => OpenAiCompatConfig::baidu(),
                ProviderKind::ByteDanceArk => OpenAiCompatConfig::bytedance(),
                _ => OpenAiCompatConfig::custom(),
            };
            Ok(Self::Custom(OpenAiCompatClient::new_optional(api_key, config)
                .with_endpoint(endpoint).with_model_alias(model, model)))
        }
    }

    #[must_use]
    pub fn with_request_parameters(self, parameters: crate::RequestParameters) -> Self {
        match self {
            Self::ClawApi(client) => Self::ClawApi(client.with_request_parameters(parameters)),
            Self::Anthropic(client) => Self::Anthropic(client.with_request_parameters(parameters)),
            Self::Xai(client) => Self::Xai(client.with_request_parameters(parameters)),
            Self::OpenAi(client) => Self::OpenAi(client.with_request_parameters(parameters)),
            Self::ZhipuAi(client) => Self::ZhipuAi(client.with_request_parameters(parameters)),
            Self::AlibabaBailian(client) => Self::AlibabaBailian(client.with_request_parameters(parameters)),
            #[allow(deprecated)]
            Self::AlibabaCloud(client) => Self::AlibabaCloud(client.with_request_parameters(parameters)),
            Self::BaiduQianfan(client) => Self::BaiduQianfan(client.with_request_parameters(parameters)),
            Self::ByteDanceArk(client) => Self::ByteDanceArk(client.with_request_parameters(parameters)),
            Self::DeepSeek(client) => Self::DeepSeek(client.with_request_parameters(parameters)),
            Self::Custom(client) => Self::Custom(client.with_request_parameters(parameters)),
        }
    }

    pub fn from_model(model: &str) -> Result<Self, ApiError> {
        Self::from_model_with_default_auth(model, None)
    }

    pub fn from_model_provider_and_key(
        model: &str,
        provider: ProviderKind,
        api_key: Option<String>,
    ) -> Result<Self, ApiError> {
        let registry = ModelRegistry::global();
        let resolved_model = registry.resolve_model_for_provider(model, provider);
        Self::from_resolved_model_with_key(model, &resolved_model, api_key)
    }

    pub fn from_custom_openai_compatible(
        model: &str,
        base_url: &str,
        api_key: Option<String>,
    ) -> Result<Self, ApiError> {
        let client = OpenAiCompatClient::new_optional(
            api_key.filter(|value| !value.trim().is_empty()),
            OpenAiCompatConfig::custom(),
        )
        .with_base_url(base_url.trim().trim_end_matches('/').to_string())
        .with_model_alias(model, model);
        Ok(Self::Custom(client))
    }

    pub fn from_model_with_default_auth(
        model: &str,
        default_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        Self::from_registry_model(ModelRegistry::global(), model, default_auth)
    }

    pub fn from_registry_model(
        registry: &ModelRegistry,
        model: &str,
        default_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        let resolved_model = registry.resolve_model(model);
        Self::from_resolved_model(model, &resolved_model, default_auth)
    }

    fn from_resolved_model(
        requested_model: &str,
        resolved: &ResolvedModel,
        default_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        match resolved.provider {
            ProviderKind::ClawApi => Ok(Self::ClawApi(configure_claw_client(
                claw_client(default_auth)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::Anthropic => Ok(Self::Anthropic(configure_claw_client(
                claw_client(default_auth)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::Xai => Ok(Self::Xai(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::xai())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::OpenAi => Ok(Self::OpenAi(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::openai())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::ZhipuAi => Ok(Self::ZhipuAi(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::zhipu())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::AlibabaBailian => Ok(Self::AlibabaBailian(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::alibaba_bailian())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::BaiduQianfan => Ok(Self::BaiduQianfan(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::baidu())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::ByteDanceArk => Ok(Self::ByteDanceArk(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::bytedance())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::DeepSeek => Ok(Self::DeepSeek(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::deepseek())?,
                requested_model,
                resolved,
            ))),
            ProviderKind::Custom => Ok(Self::Custom(configure_openai_client(
                OpenAiCompatClient::from_env(OpenAiCompatConfig::custom())?,
                requested_model,
                resolved,
            ))),
        }
    }

    fn from_resolved_model_with_key(
        requested_model: &str,
        resolved: &ResolvedModel,
        api_key: Option<String>,
    ) -> Result<Self, ApiError> {
        let api_key = api_key.filter(|value| !value.trim().is_empty());
        match resolved.provider {
            ProviderKind::ClawApi => Ok(Self::ClawApi(configure_claw_client(
                claw_client(api_key.map(AuthSource::ApiKey))?,
                requested_model,
                resolved,
            ))),
            ProviderKind::Anthropic => Ok(Self::Anthropic(configure_claw_client(
                claw_client(api_key.map(AuthSource::ApiKey))?,
                requested_model,
                resolved,
            ))),
            ProviderKind::Xai => Ok(Self::Xai(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::xai(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::OpenAi => Ok(Self::OpenAi(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::openai(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::ZhipuAi => Ok(Self::ZhipuAi(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::zhipu(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::AlibabaBailian => Ok(Self::AlibabaBailian(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::alibaba_bailian(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::BaiduQianfan => Ok(Self::BaiduQianfan(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::baidu(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::ByteDanceArk => Ok(Self::ByteDanceArk(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::bytedance(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::DeepSeek => Ok(Self::DeepSeek(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::deepseek(), api_key)?,
                requested_model,
                resolved,
            ))),
            ProviderKind::Custom => Ok(Self::Custom(configure_openai_client(
                openai_client_with_optional_key(OpenAiCompatConfig::custom(), api_key)?,
                requested_model,
                resolved,
            ))),
        }
    }

    #[must_use]
    pub const fn provider_kind(&self) -> ProviderKind {
        match self {
            Self::ClawApi(_) => ProviderKind::ClawApi,
            Self::Anthropic(_) => ProviderKind::Anthropic,
            Self::Xai(_) => ProviderKind::Xai,
            Self::OpenAi(_) => ProviderKind::OpenAi,
            Self::ZhipuAi(_) => ProviderKind::ZhipuAi,
            Self::AlibabaBailian(_) => ProviderKind::AlibabaBailian,
            #[allow(deprecated)]
            Self::AlibabaCloud(_) => ProviderKind::AlibabaBailian,
            Self::BaiduQianfan(_) => ProviderKind::BaiduQianfan,
            Self::ByteDanceArk(_) => ProviderKind::ByteDanceArk,
            Self::DeepSeek(_) => ProviderKind::DeepSeek,
            Self::Custom(_) => ProviderKind::Custom,
        }
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        match self {
            Self::ClawApi(client) | Self::Anthropic(client) => {
                send_via_provider(client, request).await
            }
            #[allow(deprecated)]
            Self::Xai(client)
            | Self::OpenAi(client)
            | Self::ZhipuAi(client)
            | Self::AlibabaBailian(client)
            | Self::AlibabaCloud(client)
            | Self::BaiduQianfan(client)
            | Self::ByteDanceArk(client)
            | Self::DeepSeek(client)
            | Self::Custom(client) => send_via_provider(client, request).await,
        }
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        match self {
            Self::ClawApi(client) | Self::Anthropic(client) => stream_via_provider(client, request)
                .await
                .map(MessageStream::ClawApi),
            #[allow(deprecated)]
            Self::Xai(client)
            | Self::OpenAi(client)
            | Self::ZhipuAi(client)
            | Self::AlibabaBailian(client)
            | Self::AlibabaCloud(client)
            | Self::BaiduQianfan(client)
            | Self::ByteDanceArk(client)
            | Self::DeepSeek(client)
            | Self::Custom(client) => stream_via_provider(client, request)
                .await
                .map(MessageStream::OpenAiCompat),
        }
    }
}

fn claw_client(default_auth: Option<AuthSource>) -> Result<ClawApiClient, ApiError> {
    Ok(match default_auth {
        Some(auth) => ClawApiClient::from_auth(auth),
        None => ClawApiClient::from_env()?,
    })
}

fn openai_client_with_optional_key(
    config: OpenAiCompatConfig,
    api_key: Option<String>,
) -> Result<OpenAiCompatClient, ApiError> {
    Ok(match api_key {
        Some(api_key) => OpenAiCompatClient::new(api_key, config),
        None => OpenAiCompatClient::from_env(config)?,
    })
}

fn configure_claw_client(
    client: ClawApiClient,
    requested_model: &str,
    resolved: &ResolvedModel,
) -> ClawApiClient {
    client
        .with_base_url(resolved.base_url.clone())
        .with_model_alias(requested_model, resolved.api_model_id.clone())
        .with_model_alias(resolved.canonical_id.clone(), resolved.api_model_id.clone())
}

fn configure_openai_client(
    client: OpenAiCompatClient,
    requested_model: &str,
    resolved: &ResolvedModel,
) -> OpenAiCompatClient {
    client
        .with_base_url(resolved.base_url.clone())
        .with_model_alias(requested_model, resolved.api_model_id.clone())
        .with_model_alias(resolved.canonical_id.clone(), resolved.api_model_id.clone())
}

#[derive(Debug)]
pub enum MessageStream {
    ClawApi(claw_provider::MessageStream),
    OpenAiCompat(openai_compat::MessageStream),
}

impl MessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::ClawApi(stream) => stream.request_id(),
            Self::OpenAiCompat(stream) => stream.request_id(),
        }
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        match self {
            Self::ClawApi(stream) => stream.next_event().await,
            Self::OpenAiCompat(stream) => stream.next_event().await,
        }
    }
}

pub use claw_provider::{
    oauth_token_is_expired, resolve_saved_oauth_token, resolve_startup_auth_source, OAuthTokenSet,
};
#[must_use]
pub fn read_base_url() -> String {
    claw_provider::read_base_url()
}

#[must_use]
pub fn read_xai_base_url() -> String {
    openai_compat::read_base_url(OpenAiCompatConfig::xai())
}

#[must_use]
pub fn read_zhipu_base_url() -> String {
    openai_compat::read_base_url(OpenAiCompatConfig::zhipu())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use crate::config::{AdapterConfig, ModelConfig, ProviderConfig};
    use crate::providers::{detect_provider_kind, resolve_model_alias, ProviderKind};
    use crate::registry::ModelRegistry;
    use crate::types::{InputMessage, MessageRequest};
    use crate::ProviderClient;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn resolves_existing_and_grok_aliases() {
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
    }

    #[test]
    fn provider_detection_prefers_model_family() {
        assert_eq!(detect_provider_kind("grok-3"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::ClawApi
        );
    }

    #[test]
    fn provider_client_builds_custom_openai_compatible_endpoint() {
        let client = ProviderClient::from_custom_openai_compatible(
            "qwen2.5-vl-3b",
            "http://127.0.0.1:8000/v1",
            None,
        )
        .expect("custom OpenAI-compatible client should build");
        assert_eq!(client.provider_kind(), ProviderKind::Custom);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn custom_openai_compatible_without_key_omits_authorization_header() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should expose local addr");

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept should succeed");
            let request = read_http_request(&mut socket).await;
            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));

            let response_body = r#"{
  "id":"chatcmpl-custom-no-auth",
  "model":"qwen2.5-vl-3b",
  "choices":[
    {
      "message":{"role":"assistant","content":"custom ok","tool_calls":[]},
      "finish_reason":"stop"
    }
  ],
  "usage":{"prompt_tokens":1,"completion_tokens":2}
}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("response should write");
        });

        let client = ProviderClient::from_custom_openai_compatible(
            "qwen2.5-vl-3b",
            &format!("http://{address}/v1"),
            None,
        )
        .expect("custom client should build without a key");
        let response = client
            .send_message(&MessageRequest {
                model: "qwen2.5-vl-3b".to_string(),
                max_tokens: 128,
                messages: vec![InputMessage::user_text("hello")],
                system: None,
                tools: None,
                tool_choice: None,
                reasoning_effort: None,
                stream: false,
            })
            .await
            .expect("request should succeed");

        assert_eq!(response.model, "qwen2.5-vl-3b");
        server.await.expect("server task should finish");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provider_client_uses_registry_model_config_for_request_routing() {
        let _lock = env_lock();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should expose local addr");

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept should succeed");
            let request = read_http_request(&mut socket).await;
            assert!(request.starts_with("POST /compatible/chat/completions HTTP/1.1"));
            assert!(request.contains("authorization: Bearer zhipu-test-key"));
            assert!(request.contains("\"model\":\"glm-api-id\""));

            let response_body = r#"{
  "id":"chatcmpl-registry-test",
  "model":"glm-api-id",
  "choices":[
    {
      "message":{"role":"assistant","content":"registry ok","tool_calls":[]},
      "finish_reason":"stop"
    }
  ],
  "usage":{"prompt_tokens":1,"completion_tokens":2}
}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("response should write");
        });

        std::env::set_var("ZAI_API_KEY", "zhipu-test-key");
        let registry = ModelRegistry::with_config(AdapterConfig {
            providers: HashMap::from([(
                "zhipuai".to_string(),
                ProviderConfig {
                    base_url: Some(format!("http://{address}/compatible")),
                    api_key_env: None,
                },
            )]),
            models: HashMap::from([(
                "glm-custom".to_string(),
                ModelConfig {
                    provider: Some("zhipuai".to_string()),
                    base_url: None,
                    api_model_id: Some("glm-api-id".to_string()),
                },
            )]),
        });

        let client = ProviderClient::from_registry_model(&registry, "glm-custom", None)
            .expect("registry model should build client");
        let response = client
            .send_message(&MessageRequest {
                model: "glm-custom".to_string(),
                max_tokens: 128,
                messages: vec![InputMessage::user_text("hello")],
                system: None,
                tools: None,
                tool_choice: None,
                reasoning_effort: None,
                stream: false,
            })
            .await
            .expect("request should succeed");

        assert_eq!(response.model, "glm-api-id");
        server.await.expect("server task should finish");
        std::env::remove_var("ZAI_API_KEY");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unified_session_parameters_reach_both_native_endpoints() {
        for anthropic in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut socket).await;
                let expected_path = "/custom/exact-endpoint";
                assert!(request.starts_with(&format!("POST {expected_path} HTTP/1.1")));
                let body: serde_json::Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                assert_eq!(body["model"], "unregistered-model-2026");
                if anthropic {
                    assert_eq!(body["thinking"]["budget_tokens"], 2048);
                    assert!(body.get("reasoning_effort").is_none());
                    assert!(request.contains("x-api-key: test-secret"));
                } else {
                    assert_eq!(body["reasoning_effort"], "xhigh");
                    assert_eq!(body["temperature"], 0.4);
                    assert_eq!(body["top_p"], 0.8);
                    assert!(request.contains("authorization: Bearer test-secret"));
                }
                let response_body = if anthropic {
                    r#"{"id":"test","type":"message","role":"assistant","model":"unregistered-model-2026","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#
                } else {
                    r#"{"id":"test","model":"unregistered-model-2026","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}", response_body.len(), response_body).as_bytes()).await.unwrap();
            });
            let client = ProviderClient::from_session_endpoint(
                "unregistered-model-2026", &format!("http://{address}/custom/exact-endpoint"), anthropic,
                ProviderKind::Custom, Some("test-secret".into()),
            ).unwrap().with_request_parameters(crate::RequestParameters {
                temperature: (!anthropic).then_some(0.4), top_p: (!anthropic).then_some(0.8),
                reasoning_mode: Some(if anthropic { "budget" } else { "effort" }.into()),
                thinking_budget: anthropic.then_some(2048),
            });
            let request = MessageRequest {
                model: "unregistered-model-2026".into(), max_tokens: 4096,
                messages: vec![InputMessage::user_text("测试")], system: None,
                tools: None, tool_choice: None, reasoning_effort: Some("xhigh".into()), stream: false,
            };
            let response = client.send_message(&request).await.unwrap();
            assert_eq!(response.model, "unregistered-model-2026");
            server.await.unwrap();
        }
    }

    async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        let mut expected_total = None;

        loop {
            let read = socket.read(&mut chunk).await.expect("request should read");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);

            if let Some(header_end) = find_header_end(&buffer) {
                let content_length = parse_content_length(&buffer[..header_end]);
                let total = header_end + 4 + content_length;
                if buffer.len() >= total {
                    expected_total = Some(total);
                }
            }

            if let Some(total) = expected_total {
                if buffer.len() >= total {
                    break;
                }
            }
        }

        String::from_utf8(buffer).expect("request should be utf8")
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn parse_content_length(header: &[u8]) -> usize {
        String::from_utf8_lossy(header)
            .lines()
            .find_map(|line| {
                let lower = line.to_ascii_lowercase();
                lower
                    .strip_prefix("content-length:")
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0)
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }
}
