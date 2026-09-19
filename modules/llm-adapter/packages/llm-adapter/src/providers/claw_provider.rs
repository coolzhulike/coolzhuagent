use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use runtime::{
    load_oauth_credentials, save_oauth_credentials, OAuthConfig, OAuthRefreshRequest,
    OAuthTokenExchangeRequest,
};
use serde::Deserialize;

use crate::error::ApiError;

use super::{canonical_claude_model_id, Provider, ProviderFuture};
use crate::resolver::{EndpointResolver, ProviderProtocol};
use crate::reasoning::{resolve_reasoning, ReasoningWire};
use crate::sse::SseParser;
use crate::types::{MessageRequest, MessageResponse, StreamEvent};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const REQUEST_ID_HEADER: &str = "request-id";
const ALT_REQUEST_ID_HEADER: &str = "x-request-id";
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(2);
const DEFAULT_MAX_RETRIES: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSource {
    None,
    ApiKey(String),
    BearerToken(String),
    ApiKeyAndBearer {
        api_key: String,
        bearer_token: String,
    },
}

impl AuthSource {
    pub fn from_env() -> Result<Self, ApiError> {
        let api_key = read_env_non_empty("ANTHROPIC_API_KEY")?;
        let auth_token = read_env_non_empty("ANTHROPIC_AUTH_TOKEN")?;
        match (api_key, auth_token) {
            (Some(api_key), Some(bearer_token)) => Ok(Self::ApiKeyAndBearer {
                api_key,
                bearer_token,
            }),
            (Some(api_key), None) => Ok(Self::ApiKey(api_key)),
            (None, Some(bearer_token)) => Ok(Self::BearerToken(bearer_token)),
            (None, None) => Err(ApiError::missing_credentials(
                "Claw",
                &["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"],
            )),
        }
    }

    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::ApiKey(api_key) | Self::ApiKeyAndBearer { api_key, .. } => Some(api_key),
            Self::None | Self::BearerToken(_) => None,
        }
    }

    #[must_use]
    pub fn bearer_token(&self) -> Option<&str> {
        match self {
            Self::BearerToken(token)
            | Self::ApiKeyAndBearer {
                bearer_token: token,
                ..
            } => Some(token),
            Self::None | Self::ApiKey(_) => None,
        }
    }

    #[must_use]
    pub fn masked_authorization_header(&self) -> &'static str {
        if self.bearer_token().is_some() {
            "Bearer [REDACTED]"
        } else {
            "<absent>"
        }
    }

    pub fn apply(&self, mut request_builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(api_key) = self.api_key() {
            request_builder = request_builder.header("x-api-key", api_key);
        }
        if let Some(token) = self.bearer_token() {
            request_builder = request_builder.bearer_auth(token);
        }
        request_builder
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OAuthTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl From<OAuthTokenSet> for AuthSource {
    fn from(value: OAuthTokenSet) -> Self {
        Self::BearerToken(value.access_token)
    }
}

#[derive(Debug, Clone)]
pub struct ClawApiClient {
    http: reqwest::Client,
    auth: AuthSource,
    base_url: String,
    endpoint: Option<String>,
    model_aliases: HashMap<String, String>,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    request_parameters: crate::RequestParameters,
}

/// 带超时的 HTTP 客户端：避免上游挂起导致请求 await 无限期阻塞（与 openai_compat 一致，#6 根因之一）。
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

impl ClawApiClient {
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: build_http_client(),
            auth: AuthSource::ApiKey(api_key.into()),
            base_url: DEFAULT_BASE_URL.to_string(),
            endpoint: None,
            model_aliases: HashMap::new(),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            request_parameters: crate::RequestParameters::default(),
        }
    }

    #[must_use]
    pub fn from_auth(auth: AuthSource) -> Self {
        Self {
            http: build_http_client(),
            auth,
            base_url: DEFAULT_BASE_URL.to_string(),
            endpoint: None,
            model_aliases: HashMap::new(),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            request_parameters: crate::RequestParameters::default(),
        }
    }

    pub fn from_env() -> Result<Self, ApiError> {
        Ok(Self::from_auth(AuthSource::from_env_or_saved()?).with_base_url(read_base_url()))
    }

    #[must_use]
    pub fn with_auth_source(mut self, auth: AuthSource) -> Self {
        self.auth = auth;
        self
    }

    #[must_use]
    pub fn with_auth_token(mut self, auth_token: Option<String>) -> Self {
        match (
            self.auth.api_key().map(ToOwned::to_owned),
            auth_token.filter(|token| !token.is_empty()),
        ) {
            (Some(api_key), Some(bearer_token)) => {
                self.auth = AuthSource::ApiKeyAndBearer {
                    api_key,
                    bearer_token,
                };
            }
            (Some(api_key), None) => {
                self.auth = AuthSource::ApiKey(api_key);
            }
            (None, Some(bearer_token)) => {
                self.auth = AuthSource::BearerToken(bearer_token);
            }
            (None, None) => {
                self.auth = AuthSource::None;
            }
        }
        self
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    #[must_use]
    pub fn with_request_parameters(mut self, parameters: crate::RequestParameters) -> Self {
        self.request_parameters = parameters;
        self
    }

    #[must_use]
    pub fn with_model_alias(
        mut self,
        requested_model: impl Into<String>,
        api_model_id: impl Into<String>,
    ) -> Self {
        self.model_aliases
            .insert(requested_model.into(), api_model_id.into());
        self
    }

    #[must_use]
    pub fn with_retry_policy(
        mut self,
        max_retries: u32,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        self.max_retries = max_retries;
        self.initial_backoff = initial_backoff;
        self.max_backoff = max_backoff;
        self
    }

    #[must_use]
    pub fn auth_source(&self) -> &AuthSource {
        &self.auth
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        let request = MessageRequest {
            model: self.api_model_for(&request.model),
            stream: false,
            ..request.clone()
        };
        let response = self.send_with_retry(&request).await?;
        let request_id = request_id_from_headers(response.headers());
        let mut response = response
            .json::<MessageResponse>()
            .await
            .map_err(ApiError::from)?;
        if response.request_id.is_none() {
            response.request_id = request_id;
        }
        Ok(response)
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        let request = MessageRequest {
            model: self.api_model_for(&request.model),
            ..request.clone()
        }
        .with_streaming();
        let response = self.send_with_retry(&request).await?;
        Ok(MessageStream {
            request_id: request_id_from_headers(response.headers()),
            response,
            parser: SseParser::new(),
            pending: VecDeque::new(),
            done: false,
        })
    }

    fn api_model_for(&self, requested_model: &str) -> String {
        self.model_aliases
            .get(requested_model)
            .map(|model| canonical_claude_model_id(model))
            .unwrap_or_else(|| canonical_claude_model_id(requested_model))
    }

    pub async fn exchange_oauth_code(
        &self,
        config: &OAuthConfig,
        request: &OAuthTokenExchangeRequest,
    ) -> Result<OAuthTokenSet, ApiError> {
        let response = self
            .http
            .post(&config.token_url)
            .header("content-type", "application/x-www-form-urlencoded")
            .form(&request.form_params())
            .send()
            .await
            .map_err(ApiError::from)?;
        let response = expect_success(response).await?;
        response
            .json::<OAuthTokenSet>()
            .await
            .map_err(ApiError::from)
    }

    pub async fn refresh_oauth_token(
        &self,
        config: &OAuthConfig,
        request: &OAuthRefreshRequest,
    ) -> Result<OAuthTokenSet, ApiError> {
        let response = self
            .http
            .post(&config.token_url)
            .header("content-type", "application/x-www-form-urlencoded")
            .form(&request.form_params())
            .send()
            .await
            .map_err(ApiError::from)?;
        let response = expect_success(response).await?;
        response
            .json::<OAuthTokenSet>()
            .await
            .map_err(ApiError::from)
    }

    async fn send_with_retry(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let mut attempts = 0;
        let mut last_error: Option<ApiError>;

        loop {
            attempts += 1;
            match self.send_raw_request(request).await {
                Ok(response) => match expect_success(response).await {
                    Ok(response) => return Ok(response),
                    Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => {
                        last_error = Some(error);
                    }
                    Err(error) => return Err(error),
                },
                Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }

            if attempts > self.max_retries {
                break;
            }

            tokio::time::sleep(self.backoff_for_attempt(attempts)?).await;
        }

        Err(ApiError::RetriesExhausted {
            attempts,
            last_error: Box::new(last_error.expect("retry loop must capture an error")),
        })
    }

    async fn send_raw_request(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let request_url = match &self.endpoint {
            Some(endpoint) => endpoint.clone(),
            None => EndpointResolver::resolve(&self.base_url, ProviderProtocol::AnthropicMessages, None)?,
        };
        let request_builder = self
            .http
            .post(&request_url)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");
        let mut request_builder = self.auth.apply(request_builder);

        let mut payload = build_anthropic_messages_request(request);
        self.request_parameters.apply(&mut payload, request.reasoning_effort.as_deref(), true);
        validate_anthropic_image_sources(&payload)?;
        request_builder = request_builder.json(&payload);
        request_builder.send().await.map_err(ApiError::from)
    }

    fn backoff_for_attempt(&self, attempt: u32) -> Result<Duration, ApiError> {
        let Some(multiplier) = 1_u32.checked_shl(attempt.saturating_sub(1)) else {
            return Err(ApiError::BackoffOverflow {
                attempt,
                base_delay: self.initial_backoff,
            });
        };
        Ok(self
            .initial_backoff
            .checked_mul(multiplier)
            .map_or(self.max_backoff, |delay| delay.min(self.max_backoff)))
    }
}

/// 构造 Anthropic Messages payload，并映射原生 thinking 与 image/source 图片块。
///
/// 特别重要的是移除旧的顶层 `reasoning_effort`：Anthropic Messages 不接受该
/// OpenAI-compatible 字段。未知/不支持值由 resolver 安全回到 auto 并省略。
pub fn build_anthropic_messages_request(request: &MessageRequest) -> serde_json::Value {
    let mut payload = serde_json::to_value(request).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = payload.as_object_mut() {
        object.remove("reasoning_effort");
        if let Some(model) = object
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(canonical_claude_model_id)
        {
            object.insert("model".to_string(), serde_json::json!(model));
        }
    }
    if let Some(messages) = payload.get_mut("messages").and_then(serde_json::Value::as_array_mut) {
        for message in messages {
            let Some(content) = message.get_mut("content").and_then(serde_json::Value::as_array_mut) else {
                continue;
            };
            for block in content {
                if block.get("type").and_then(serde_json::Value::as_str) != Some("image_url") {
                    continue;
                }
                let url = block.get("url").and_then(serde_json::Value::as_str).unwrap_or("");
                let source = if let Some((media_type, data)) = url
                    .strip_prefix("data:")
                    .and_then(|value| value.split_once(";base64,"))
                {
                    serde_json::json!({"type": "base64", "media_type": media_type, "data": data})
                } else {
                    serde_json::json!({"type": "url", "url": url})
                };
                // detail 是 OpenAI 专有字段，Anthropic 的 image 块不得透传它。
                *block = serde_json::json!({"type": "image", "source": source});
            }
        }
    }
    let wire = resolve_reasoning(
        "clawapi",
        &request.model,
        request.reasoning_effort.as_deref(),
    )
    .map(|resolution| resolution.preflight_wire)
    .unwrap_or(ReasoningWire::Omit);
    wire.apply_to_payload(&mut payload);
    payload
}

/// 发送前拒绝不能按原生协议表达的图片来源，不静默丢图，也不回显图片或URL内容。
fn validate_anthropic_image_sources(payload: &serde_json::Value) -> Result<(), ApiError> {
    let Some(messages) = payload.get("messages").and_then(serde_json::Value::as_array) else {
        return Ok(());
    };
    for (message_index, message) in messages.iter().enumerate() {
        let Some(content) = message.get("content").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for (block_index, block) in content.iter().enumerate() {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("image") {
                continue;
            }
            let source = &block["source"];
            let valid = match source.get("type").and_then(serde_json::Value::as_str) {
                Some("base64") => {
                    matches!(
                        source.get("media_type").and_then(serde_json::Value::as_str),
                        Some("image/jpeg" | "image/png" | "image/gif" | "image/webp")
                    ) && source.get("data").and_then(serde_json::Value::as_str)
                        .is_some_and(|data| !data.is_empty())
                }
                Some("url") => source.get("url").and_then(serde_json::Value::as_str)
                    .and_then(|url| reqwest::Url::parse(url).ok())
                    .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.has_host()),
                _ => false,
            };
            if !valid {
                return Err(ApiError::ConfigError {
                    path: format!("messages[{message_index}].content[{block_index}].image.source"),
                    message: "Anthropic 图片需要非空的 JPEG/PNG/GIF/WebP base64 data URI 或 HTTP(S) URL".to_string(),
                });
            }
        }
    }
    Ok(())
}

impl AuthSource {
    pub fn from_env_or_saved() -> Result<Self, ApiError> {
        if let Some(api_key) = read_env_non_empty("ANTHROPIC_API_KEY")? {
            return match read_env_non_empty("ANTHROPIC_AUTH_TOKEN")? {
                Some(bearer_token) => Ok(Self::ApiKeyAndBearer {
                    api_key,
                    bearer_token,
                }),
                None => Ok(Self::ApiKey(api_key)),
            };
        }
        if let Some(bearer_token) = read_env_non_empty("ANTHROPIC_AUTH_TOKEN")? {
            return Ok(Self::BearerToken(bearer_token));
        }
        match load_saved_oauth_token() {
            Ok(Some(token_set)) if oauth_token_is_expired(&token_set) => {
                if token_set.refresh_token.is_some() {
                    Err(ApiError::Auth(
                        "saved OAuth token is expired; load runtime OAuth config to refresh it"
                            .to_string(),
                    ))
                } else {
                    Err(ApiError::ExpiredOAuthToken)
                }
            }
            Ok(Some(token_set)) => Ok(Self::BearerToken(token_set.access_token)),
            Ok(None) => Err(ApiError::missing_credentials(
                "Claw",
                &["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"],
            )),
            Err(error) => Err(error),
        }
    }
}

#[must_use]
pub fn oauth_token_is_expired(token_set: &OAuthTokenSet) -> bool {
    token_set
        .expires_at
        .is_some_and(|expires_at| expires_at <= now_unix_timestamp())
}

pub fn resolve_saved_oauth_token(config: &OAuthConfig) -> Result<Option<OAuthTokenSet>, ApiError> {
    let Some(token_set) = load_saved_oauth_token()? else {
        return Ok(None);
    };
    resolve_saved_oauth_token_set(config, token_set).map(Some)
}

pub fn has_auth_from_env_or_saved() -> Result<bool, ApiError> {
    Ok(read_env_non_empty("ANTHROPIC_API_KEY")?.is_some()
        || read_env_non_empty("ANTHROPIC_AUTH_TOKEN")?.is_some()
        || load_saved_oauth_token()?.is_some())
}

pub fn resolve_startup_auth_source<F>(load_oauth_config: F) -> Result<AuthSource, ApiError>
where
    F: FnOnce() -> Result<Option<OAuthConfig>, ApiError>,
{
    if let Some(api_key) = read_env_non_empty("ANTHROPIC_API_KEY")? {
        return match read_env_non_empty("ANTHROPIC_AUTH_TOKEN")? {
            Some(bearer_token) => Ok(AuthSource::ApiKeyAndBearer {
                api_key,
                bearer_token,
            }),
            None => Ok(AuthSource::ApiKey(api_key)),
        };
    }
    if let Some(bearer_token) = read_env_non_empty("ANTHROPIC_AUTH_TOKEN")? {
        return Ok(AuthSource::BearerToken(bearer_token));
    }

    let Some(token_set) = load_saved_oauth_token()? else {
        return Err(ApiError::missing_credentials(
            "Claw",
            &["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"],
        ));
    };
    if !oauth_token_is_expired(&token_set) {
        return Ok(AuthSource::BearerToken(token_set.access_token));
    }
    if token_set.refresh_token.is_none() {
        return Err(ApiError::ExpiredOAuthToken);
    }

    let Some(config) = load_oauth_config()? else {
        return Err(ApiError::Auth(
            "saved OAuth token is expired; runtime OAuth config is missing".to_string(),
        ));
    };
    Ok(AuthSource::from(resolve_saved_oauth_token_set(
        &config, token_set,
    )?))
}

fn resolve_saved_oauth_token_set(
    config: &OAuthConfig,
    token_set: OAuthTokenSet,
) -> Result<OAuthTokenSet, ApiError> {
    if !oauth_token_is_expired(&token_set) {
        return Ok(token_set);
    }
    let Some(refresh_token) = token_set.refresh_token.clone() else {
        return Err(ApiError::ExpiredOAuthToken);
    };
    let client = ClawApiClient::from_auth(AuthSource::None).with_base_url(read_base_url());
    let refreshed = client_runtime_block_on(async {
        client
            .refresh_oauth_token(
                config,
                &OAuthRefreshRequest::from_config(
                    config,
                    refresh_token,
                    Some(token_set.scopes.clone()),
                ),
            )
            .await
    })?;
    let resolved = OAuthTokenSet {
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token.or(token_set.refresh_token),
        expires_at: refreshed.expires_at,
        scopes: refreshed.scopes,
    };
    save_oauth_credentials(&runtime::OAuthTokenSet {
        access_token: resolved.access_token.clone(),
        refresh_token: resolved.refresh_token.clone(),
        expires_at: resolved.expires_at,
        scopes: resolved.scopes.clone(),
    })
    .map_err(ApiError::from)?;
    Ok(resolved)
}

fn client_runtime_block_on<F, T>(future: F) -> Result<T, ApiError>
where
    F: std::future::Future<Output = Result<T, ApiError>>,
{
    tokio::runtime::Runtime::new()
        .map_err(ApiError::from)?
        .block_on(future)
}

fn load_saved_oauth_token() -> Result<Option<OAuthTokenSet>, ApiError> {
    let token_set = load_oauth_credentials().map_err(ApiError::from)?;
    Ok(token_set.map(|token_set| OAuthTokenSet {
        access_token: token_set.access_token,
        refresh_token: token_set.refresh_token,
        expires_at: token_set.expires_at,
        scopes: token_set.scopes,
    }))
}

fn now_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

#[cfg(test)]
fn read_api_key() -> Result<String, ApiError> {
    let auth = AuthSource::from_env_or_saved()?;
    auth.api_key()
        .or_else(|| auth.bearer_token())
        .map(ToOwned::to_owned)
        .ok_or(ApiError::missing_credentials(
            "Claw",
            &["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"],
        ))
}

#[cfg(test)]
fn read_auth_token() -> Option<String> {
    read_env_non_empty("ANTHROPIC_AUTH_TOKEN")
        .ok()
        .and_then(std::convert::identity)
}

#[must_use]
pub fn read_base_url() -> String {
    std::env::var("ANTHROPIC_BASE_URL")
        .or_else(|_| std::env::var("apiBaseUrl"))
        .or_else(|_| std::env::var("API_BASE_URL"))
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

fn request_id_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .or_else(|| headers.get(ALT_REQUEST_ID_HEADER))
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

impl Provider for ClawApiClient {
    type Stream = MessageStream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse> {
        Box::pin(async move { self.send_message(request).await })
    }

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream> {
        Box::pin(async move { self.stream_message(request).await })
    }
}

#[derive(Debug)]
pub struct MessageStream {
    request_id: Option<String>,
    response: reqwest::Response,
    parser: SseParser,
    pending: VecDeque<StreamEvent>,
    done: bool,
}

impl MessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }

            if self.done {
                let remaining = self.parser.finish()?;
                self.pending.extend(remaining);
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            match self.response.chunk().await? {
                Some(chunk) => {
                    self.pending.extend(self.parser.push(&chunk)?);
                }
                None => {
                    self.done = true;
                }
            }
        }
    }
}

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await.unwrap_or_else(|_| String::new());
    let parsed_error = serde_json::from_str::<ApiErrorEnvelope>(&body).ok();
    let retryable = is_retryable_status(status);

    Err(ApiError::Api {
        status,
        error_type: parsed_error
            .as_ref()
            .map(|error| error.error.error_type.clone()),
        message: parsed_error
            .as_ref()
            .map(|error| error.error.message.clone()),
        body,
        retryable,
    })
}

const fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 409 | 429 | 500 | 502 | 503 | 504)
}

#[derive(Debug, Deserialize)]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::{ALT_REQUEST_ID_HEADER, REQUEST_ID_HEADER};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use runtime::{clear_oauth_credentials, save_oauth_credentials, OAuthConfig};

    use super::{
        build_anthropic_messages_request, now_unix_timestamp, oauth_token_is_expired,
        resolve_saved_oauth_token, resolve_startup_auth_source, AuthSource, ClawApiClient,
        OAuthTokenSet,
    };
    use crate::types::{ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn temp_config_home() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "api-oauth-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    fn cleanup_temp_config_home(config_home: &std::path::Path) {
        match std::fs::remove_dir_all(config_home) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("cleanup temp dir: {error}"),
        }
    }

    fn sample_oauth_config(token_url: String) -> OAuthConfig {
        OAuthConfig {
            client_id: "runtime-client".to_string(),
            authorize_url: "https://console.test/oauth/authorize".to_string(),
            token_url,
            callback_port: Some(4545),
            manual_redirect_url: Some("https://console.test/oauth/callback".to_string()),
            scopes: vec!["org:read".to_string(), "user:write".to_string()],
        }
    }

    fn spawn_token_server(response_body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let address = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        format!("http://{address}/oauth/token")
    }

    #[test]
    fn read_api_key_requires_presence() {
        let _guard = env_lock();
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("CLAW_CONFIG_HOME");
        let error = super::read_api_key().expect_err("missing key should error");
        assert!(matches!(
            error,
            crate::error::ApiError::MissingCredentials { .. }
        ));
    }

    #[test]
    fn read_api_key_requires_non_empty_value() {
        let _guard = env_lock();
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "");
        std::env::remove_var("ANTHROPIC_API_KEY");
        let error = super::read_api_key().expect_err("empty key should error");
        assert!(matches!(
            error,
            crate::error::ApiError::MissingCredentials { .. }
        ));
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn read_api_key_prefers_api_key_env() {
        let _guard = env_lock();
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "auth-token");
        std::env::set_var("ANTHROPIC_API_KEY", "legacy-key");
        assert_eq!(
            super::read_api_key().expect("api key should load"),
            "legacy-key"
        );
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn read_auth_token_reads_auth_token_env() {
        let _guard = env_lock();
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "auth-token");
        assert_eq!(super::read_auth_token().as_deref(), Some("auth-token"));
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn oauth_token_maps_to_bearer_auth_source() {
        let auth = AuthSource::from(OAuthTokenSet {
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: Some(123),
            scopes: vec!["scope:a".to_string()],
        });
        assert_eq!(auth.bearer_token(), Some("access-token"));
        assert_eq!(auth.api_key(), None);
    }

    #[test]
    fn auth_source_from_env_combines_api_key_and_bearer_token() {
        let _guard = env_lock();
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "auth-token");
        std::env::set_var("ANTHROPIC_API_KEY", "legacy-key");
        let auth = AuthSource::from_env().expect("env auth");
        assert_eq!(auth.api_key(), Some("legacy-key"));
        assert_eq!(auth.bearer_token(), Some("auth-token"));
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn auth_source_from_saved_oauth_when_env_absent() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "saved-access-token".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: Some(now_unix_timestamp() + 300),
            scopes: vec!["scope:a".to_string()],
        })
        .expect("save oauth credentials");

        let auth = AuthSource::from_env_or_saved().expect("saved auth");
        assert_eq!(auth.bearer_token(), Some("saved-access-token"));

        clear_oauth_credentials().expect("clear credentials");
        std::env::remove_var("CLAW_CONFIG_HOME");
        cleanup_temp_config_home(&config_home);
    }

    #[test]
    fn oauth_token_expiry_uses_expires_at_timestamp() {
        assert!(oauth_token_is_expired(&OAuthTokenSet {
            access_token: "access-token".to_string(),
            refresh_token: None,
            expires_at: Some(1),
            scopes: Vec::new(),
        }));
        assert!(!oauth_token_is_expired(&OAuthTokenSet {
            access_token: "access-token".to_string(),
            refresh_token: None,
            expires_at: Some(now_unix_timestamp() + 60),
            scopes: Vec::new(),
        }));
    }

    #[test]
    fn resolve_saved_oauth_token_refreshes_expired_credentials() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "expired-access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(1),
            scopes: vec!["scope:a".to_string()],
        })
        .expect("save expired oauth credentials");

        let token_url = spawn_token_server(
            "{\"access_token\":\"refreshed-token\",\"refresh_token\":\"fresh-refresh\",\"expires_at\":9999999999,\"scopes\":[\"scope:a\"]}",
        );
        let resolved = resolve_saved_oauth_token(&sample_oauth_config(token_url))
            .expect("resolve refreshed token")
            .expect("token set present");
        assert_eq!(resolved.access_token, "refreshed-token");
        let stored = runtime::load_oauth_credentials()
            .expect("load stored credentials")
            .expect("stored token set");
        assert_eq!(stored.access_token, "refreshed-token");

        clear_oauth_credentials().expect("clear credentials");
        std::env::remove_var("CLAW_CONFIG_HOME");
        cleanup_temp_config_home(&config_home);
    }

    #[test]
    fn resolve_startup_auth_source_uses_saved_oauth_without_loading_config() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "saved-access-token".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: Some(now_unix_timestamp() + 300),
            scopes: vec!["scope:a".to_string()],
        })
        .expect("save oauth credentials");

        let auth = resolve_startup_auth_source(|| panic!("config should not be loaded"))
            .expect("startup auth");
        assert_eq!(auth.bearer_token(), Some("saved-access-token"));

        clear_oauth_credentials().expect("clear credentials");
        std::env::remove_var("CLAW_CONFIG_HOME");
        cleanup_temp_config_home(&config_home);
    }

    #[test]
    fn resolve_startup_auth_source_errors_when_refreshable_token_lacks_config() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "expired-access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(1),
            scopes: vec!["scope:a".to_string()],
        })
        .expect("save expired oauth credentials");

        let error =
            resolve_startup_auth_source(|| Ok(None)).expect_err("missing config should error");
        assert!(
            matches!(error, crate::error::ApiError::Auth(message) if message.contains("runtime OAuth config is missing"))
        );

        let stored = runtime::load_oauth_credentials()
            .expect("load stored credentials")
            .expect("stored token set");
        assert_eq!(stored.access_token, "expired-access-token");
        assert_eq!(stored.refresh_token.as_deref(), Some("refresh-token"));

        clear_oauth_credentials().expect("clear credentials");
        std::env::remove_var("CLAW_CONFIG_HOME");
        cleanup_temp_config_home(&config_home);
    }

    #[test]
    fn resolve_saved_oauth_token_preserves_refresh_token_when_refresh_response_omits_it() {
        let _guard = env_lock();
        let config_home = temp_config_home();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_API_KEY");
        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "expired-access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(1),
            scopes: vec!["scope:a".to_string()],
        })
        .expect("save expired oauth credentials");

        let token_url = spawn_token_server(
            "{\"access_token\":\"refreshed-token\",\"expires_at\":9999999999,\"scopes\":[\"scope:a\"]}",
        );
        let resolved = resolve_saved_oauth_token(&sample_oauth_config(token_url))
            .expect("resolve refreshed token")
            .expect("token set present");
        assert_eq!(resolved.access_token, "refreshed-token");
        assert_eq!(resolved.refresh_token.as_deref(), Some("refresh-token"));
        let stored = runtime::load_oauth_credentials()
            .expect("load stored credentials")
            .expect("stored token set");
        assert_eq!(stored.refresh_token.as_deref(), Some("refresh-token"));

        clear_oauth_credentials().expect("clear credentials");
        std::env::remove_var("CLAW_CONFIG_HOME");
        cleanup_temp_config_home(&config_home);
    }

    #[test]
    fn message_request_stream_helper_sets_stream_true() {
        let request = MessageRequest {
            model: "claude-opus-4-6".to_string(),
            max_tokens: 64,
            messages: vec![],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: false,
        };

        assert!(request.with_streaming().stream);
    }

    #[test]
    fn backoff_doubles_until_maximum() {
        let client = ClawApiClient::new("test-key").with_retry_policy(
            3,
            Duration::from_millis(10),
            Duration::from_millis(25),
        );
        assert_eq!(
            client.backoff_for_attempt(1).expect("attempt 1"),
            Duration::from_millis(10)
        );
        assert_eq!(
            client.backoff_for_attempt(2).expect("attempt 2"),
            Duration::from_millis(20)
        );
        assert_eq!(
            client.backoff_for_attempt(3).expect("attempt 3"),
            Duration::from_millis(25)
        );
    }

    #[test]
    fn retryable_statuses_are_detected() {
        assert!(super::is_retryable_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(super::is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!super::is_retryable_status(
            reqwest::StatusCode::UNAUTHORIZED
        ));
    }

    #[test]
    fn tool_delta_variant_round_trips() {
        let delta = ContentBlockDelta::InputJsonDelta {
            partial_json: "{\"city\":\"Paris\"}".to_string(),
        };
        let encoded = serde_json::to_string(&delta).expect("delta should serialize");
        let decoded: ContentBlockDelta =
            serde_json::from_str(&encoded).expect("delta should deserialize");
        assert_eq!(decoded, delta);
    }

    #[test]
    fn request_id_uses_primary_or_fallback_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, "req_primary".parse().expect("header"));
        assert_eq!(
            super::request_id_from_headers(&headers).as_deref(),
            Some("req_primary")
        );

        headers.clear();
        headers.insert(
            ALT_REQUEST_ID_HEADER,
            "req_fallback".parse().expect("header"),
        );
        assert_eq!(
            super::request_id_from_headers(&headers).as_deref(),
            Some("req_fallback")
        );
    }

    #[test]
    fn auth_source_applies_headers() {
        let auth = AuthSource::ApiKeyAndBearer {
            api_key: "test-key".to_string(),
            bearer_token: "proxy-token".to_string(),
        };
        let request = auth
            .apply(reqwest::Client::new().post("https://example.test"))
            .build()
            .expect("request build");
        let headers = request.headers();
        assert_eq!(
            headers.get("x-api-key").and_then(|v| v.to_str().ok()),
            Some("test-key")
        );
        assert_eq!(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            Some("Bearer proxy-token")
        );
    }

    fn image_request(urls: &[&str]) -> MessageRequest {
        MessageRequest {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text_with_image_urls("比较这些图片", urls.iter().copied())],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: false,
        }
    }

    #[test]
    fn anthropic_images_translate_data_uris_and_urls_without_openai_fields() {
        let mut request = image_request(&[
            "data:image/png;base64,aGVsbG8=",
            "data:image/jpeg;base64,aGVsbG8=",
            "data:image/gif;base64,aGVsbG8=",
            "data:image/webp;base64,aGVsbG8=",
            "https://example.test/reference.png?version=2",
        ]);
        if let InputContentBlock::ImageUrl { detail, .. } = &mut request.messages[0].content[1] {
            *detail = Some("high".to_string());
        }
        request.messages.push(InputMessage {
            role: "assistant".to_string(),
            content: vec![InputContentBlock::ToolUse {
                id: "call-image".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "reference.txt"}),
            }],
        });
        request.messages.push(InputMessage::user_tool_result("call-image", "读取完成", false));
        let payload = build_anthropic_messages_request(&request);
        super::validate_anthropic_image_sources(&payload).expect("图片来源应有效");
        let content = payload["messages"][0]["content"].as_array().expect("图片内容数组");
        assert_eq!(content.len(), 6);
        assert_eq!(content[0], serde_json::json!({"type":"text","text":"比较这些图片"}));
        for (index, media_type) in ["image/png", "image/jpeg", "image/gif", "image/webp"].iter().enumerate() {
            assert_eq!(content[index + 1], serde_json::json!({
                "type":"image", "source":{"type":"base64","media_type":media_type,"data":"aGVsbG8="}
            }));
        }
        assert_eq!(content[5], serde_json::json!({
            "type":"image", "source":{"type":"url","url":"https://example.test/reference.png?version=2"}
        }));
        assert_eq!(payload["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(payload["messages"][2]["content"][0]["tool_use_id"], "call-image");
        assert!(!payload.to_string().contains("image_url"));
        assert!(content[1].get("detail").is_none());
    }

    #[tokio::test]
    async fn anthropic_invalid_image_sources_fail_before_network_send() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地测试端口");
        listener.set_nonblocking(true).expect("非阻塞监听");
        let client = ClawApiClient::from_auth(AuthSource::None)
            .with_base_url(format!("http://{}", listener.local_addr().expect("本地端口")));
        for invalid in [
            "data:image/svg+xml;base64,PHN2Zz4=",
            "data:image/png;base64,",
            "data:image/png,not-base64",
            "file:///C:/private.png",
            "C:\\private.png",
            "javascript:invalid",
            "",
        ] {
            let error = client.send_raw_request(&image_request(&[invalid])).await
                .expect_err("无效来源应在发送前失败");
            assert!(matches!(error, crate::error::ApiError::ConfigError { .. }));
            assert!(!error.is_retryable());
        }
        assert_eq!(listener.accept().expect_err("不应发出任何请求").kind(), std::io::ErrorKind::WouldBlock);
    }

    #[tokio::test]
    async fn anthropic_http_send_uses_native_image_sources_for_stream_and_nonstream() {
        // 只连接进程内本地接收端：验证真实发送路径，绝不使用真实鉴权或模型服务。
        for streaming in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地测试端口");
            listener.set_nonblocking(true).expect("非阻塞监听");
            let address = listener.local_addr().expect("本地端口");
            let receiver = thread::spawn(move || {
                let started = std::time::Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(5) => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("等待本地协议请求失败: {error}"),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(5))).expect("读取期限");
                let mut bytes = Vec::new();
                let (header_end, content_length) = loop {
                    let mut chunk = [0_u8; 4096];
                    let count = stream.read(&mut chunk).expect("读取HTTP请求");
                    assert!(count > 0, "请求头未完成即断开");
                    bytes.extend_from_slice(&chunk[..count]);
                    assert!(bytes.len() < 1024 * 1024, "测试请求不应超限");
                    if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..offset]);
                        assert!(headers.starts_with("POST /v1/messages HTTP/1.1"));
                        let length = headers.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().expect("内容长度"))
                        }).expect("content-length");
                        break (offset + 4, length);
                    }
                };
                while bytes.len() < header_end + content_length {
                    let mut chunk = [0_u8; 4096];
                    let count = stream.read(&mut chunk).expect("读取HTTP正文");
                    assert!(count > 0, "请求正文未完成即断开");
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let body: serde_json::Value = serde_json::from_slice(&bytes[header_end..header_end + content_length]).expect("请求JSON");
                stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}").expect("回复本地请求");
                body
            });
            let mut request = image_request(&[
                "data:image/png;base64,aGVsbG8=",
                "https://example.test/image.png",
            ]);
            request.stream = streaming;
            let response = ClawApiClient::from_auth(AuthSource::None)
                .with_base_url(format!("http://{address}"))
                .send_raw_request(&request).await.expect("本地协议请求成功");
            assert!(response.status().is_success());
            let payload = receiver.join().expect("接收线程结束");
            assert_eq!(payload["messages"][0]["content"][1], serde_json::json!({
                "type":"image", "source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}
            }));
            assert_eq!(payload["messages"][0]["content"][2], serde_json::json!({
                "type":"image", "source":{"type":"url","url":"https://example.test/image.png"}
            }));
            assert_eq!(payload.get("stream").and_then(serde_json::Value::as_bool).unwrap_or(false), streaming);
        }
    }

    #[test]
    fn anthropic_payload_uses_native_thinking_and_never_top_level_effort() {
        let adaptive = build_anthropic_messages_request(&MessageRequest {
            model: "claude-opus-4-6".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hello")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some("high".to_string()),
            stream: false,
        });
        assert_eq!(adaptive["thinking"]["type"], "adaptive");
        assert_eq!(adaptive["output_config"]["effort"], "high");
        assert!(adaptive.get("reasoning_effort").is_none());

        let disabled = build_anthropic_messages_request(&MessageRequest {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hello")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some("none".to_string()),
            stream: false,
        });
        assert_eq!(disabled["thinking"]["type"], "disabled");
        assert!(disabled.get("reasoning_effort").is_none());

        let legacy_auto = build_anthropic_messages_request(&MessageRequest {
            model: "claude-haiku-4-5-20251213".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hello")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: false,
        });
        assert_eq!(legacy_auto["model"], "claude-haiku-4-5-20251001");
        assert!(legacy_auto.get("thinking").is_none());

        let legacy_disabled = build_anthropic_messages_request(&MessageRequest {
            model: "claude-haiku-4-5-20251213".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hello")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some("none".to_string()),
            stream: false,
        });
        assert_eq!(legacy_disabled["model"], "claude-haiku-4-5-20251001");
        assert_eq!(legacy_disabled["thinking"]["type"], "disabled");
        assert!(legacy_disabled.get("reasoning_effort").is_none());
    }
}
