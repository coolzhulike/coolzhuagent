use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::resolver::{EndpointResolver, ProviderProtocol};
use crate::reasoning::{resolve_reasoning, ReasoningWire};
use crate::types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

use super::{Provider, ProviderFuture};

pub const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_ZHIPU_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";
pub const DEFAULT_ALIBABA_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
pub const DEFAULT_BAIDU_BASE_URL: &str = "https://qianfan.baidubce.com/v2";
pub const DEFAULT_BYTEDANCE_BASE_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";
pub const DEFAULT_DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_CUSTOM_BASE_URL: &str = "http://127.0.0.1:11434/v1";
const REQUEST_ID_HEADER: &str = "request-id";
const ALT_REQUEST_ID_HEADER: &str = "x-request-id";
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(2);
const DEFAULT_MAX_RETRIES: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAiCompatConfig {
    pub provider_name: &'static str,
    pub api_key_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

const XAI_ENV_VARS: &[&str] = &["XAI_API_KEY"];
const OPENAI_ENV_VARS: &[&str] = &["OPENAI_API_KEY"];
const ZHIPU_ENV_VARS: &[&str] = &["ZAI_API_KEY", "BIGMODEL_API_KEY", "OPENAI_API_KEY"];
const ALIBABA_ENV_VARS: &[&str] = &["DASHSCOPE_API_KEY", "ALIBABA_API_KEY", "QWEN_API_KEY"];
const BAIDU_ENV_VARS: &[&str] = &["QIANFAN_API_KEY", "BAIDU_API_KEY", "WENXIN_API_KEY"];
const BYTEDANCE_ENV_VARS: &[&str] = &["ARK_API_KEY", "VOLCENGINE_API_KEY", "DOUBAO_API_KEY"];
const DEEPSEEK_ENV_VARS: &[&str] = &["DEEPSEEK_API_KEY"];
const CUSTOM_ENV_VARS: &[&str] = &["CUSTOM_API_KEY", "OPENAI_API_KEY"];
const XAI_BASE_URL_ENV_VARS: &[&str] = &["XAI_BASE_URL"];
const OPENAI_BASE_URL_ENV_VARS: &[&str] = &["OPENAI_BASE_URL"];
const ZHIPU_BASE_URL_ENV_VARS: &[&str] = &["ZAI_BASE_URL", "BIGMODEL_BASE_URL", "OPENAI_BASE_URL"];
const ALIBABA_BASE_URL_ENV_VARS: &[&str] = &["DASHSCOPE_BASE_URL", "ALIBABA_BASE_URL"];
const BAIDU_BASE_URL_ENV_VARS: &[&str] = &["QIANFAN_BASE_URL", "BAIDU_BASE_URL"];
const BYTEDANCE_BASE_URL_ENV_VARS: &[&str] = &["ARK_BASE_URL", "VOLCENGINE_BASE_URL"];
const DEEPSEEK_BASE_URL_ENV_VARS: &[&str] = &["DEEPSEEK_BASE_URL"];
const CUSTOM_BASE_URL_ENV_VARS: &[&str] = &[
    "CUSTOM_BASE_URL",
    "OPENAI_COMPATIBLE_BASE_URL",
    "OPENAI_BASE_URL",
];

impl OpenAiCompatConfig {
    /// 从既有配置对象得到 canonical provider id，避免在请求构造处依赖显示名。
    #[must_use]
    pub fn provider_id(self) -> &'static str {
        match self.provider_name {
            "xAI" => "xai",
            "OpenAI" => "openai",
            "ZhipuAI" => "zhipuai",
            "AlibabaDashScope" | "AlibabaBailian" => "alibaba-bailian",
            "BaiduQianfan" => "baidu",
            "ByteDanceArk" => "bytedance",
            "DeepSeek" => "deepseek",
            "CustomOpenAICompatible" => "custom",
            _ => "unknown",
        }
    }

    #[must_use]
    pub const fn xai() -> Self {
        Self {
            provider_name: "xAI",
            api_key_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: DEFAULT_XAI_BASE_URL,
        }
    }

    #[must_use]
    pub const fn openai() -> Self {
        Self {
            provider_name: "OpenAI",
            api_key_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: DEFAULT_OPENAI_BASE_URL,
        }
    }

    #[must_use]
    pub const fn zhipu() -> Self {
        Self {
            provider_name: "ZhipuAI",
            api_key_env: "ZAI_API_KEY",
            base_url_env: "ZAI_BASE_URL",
            default_base_url: DEFAULT_ZHIPU_BASE_URL,
        }
    }

    #[must_use]
    pub const fn alibaba() -> Self {
        Self {
            provider_name: "AlibabaDashScope",
            api_key_env: "DASHSCOPE_API_KEY",
            base_url_env: "DASHSCOPE_BASE_URL",
            default_base_url: DEFAULT_ALIBABA_BASE_URL,
        }
    }

    #[must_use]
    pub const fn alibaba_bailian() -> Self {
        Self {
            provider_name: "AlibabaBailian",
            api_key_env: "DASHSCOPE_API_KEY",
            base_url_env: "DASHSCOPE_BASE_URL",
            default_base_url: DEFAULT_ALIBABA_BASE_URL,
        }
    }

    #[must_use]
    pub const fn baidu() -> Self {
        Self {
            provider_name: "BaiduQianfan",
            api_key_env: "QIANFAN_API_KEY",
            base_url_env: "QIANFAN_BASE_URL",
            default_base_url: DEFAULT_BAIDU_BASE_URL,
        }
    }

    #[must_use]
    pub const fn bytedance() -> Self {
        Self {
            provider_name: "ByteDanceArk",
            api_key_env: "ARK_API_KEY",
            base_url_env: "ARK_BASE_URL",
            default_base_url: DEFAULT_BYTEDANCE_BASE_URL,
        }
    }

    #[must_use]
    pub const fn deepseek() -> Self {
        Self {
            provider_name: "DeepSeek",
            api_key_env: "DEEPSEEK_API_KEY",
            base_url_env: "DEEPSEEK_BASE_URL",
            default_base_url: DEFAULT_DEEPSEEK_BASE_URL,
        }
    }

    #[must_use]
    pub const fn custom() -> Self {
        Self {
            provider_name: "CustomOpenAICompatible",
            api_key_env: "CUSTOM_API_KEY",
            base_url_env: "CUSTOM_BASE_URL",
            default_base_url: DEFAULT_CUSTOM_BASE_URL,
        }
    }

    #[must_use]
    pub fn credential_env_vars(self) -> &'static [&'static str] {
        match self.provider_name {
            "xAI" => XAI_ENV_VARS,
            "OpenAI" => OPENAI_ENV_VARS,
            "ZhipuAI" => ZHIPU_ENV_VARS,
            "AlibabaDashScope" | "AlibabaBailian" => ALIBABA_ENV_VARS,
            "BaiduQianfan" => BAIDU_ENV_VARS,
            "ByteDanceArk" => BYTEDANCE_ENV_VARS,
            "DeepSeek" => DEEPSEEK_ENV_VARS,
            "CustomOpenAICompatible" => CUSTOM_ENV_VARS,
            _ => &[],
        }
    }

    #[must_use]
    pub fn base_url_env_vars(self) -> &'static [&'static str] {
        match self.provider_name {
            "xAI" => XAI_BASE_URL_ENV_VARS,
            "OpenAI" => OPENAI_BASE_URL_ENV_VARS,
            "ZhipuAI" => ZHIPU_BASE_URL_ENV_VARS,
            "AlibabaDashScope" | "AlibabaBailian" => ALIBABA_BASE_URL_ENV_VARS,
            "BaiduQianfan" => BAIDU_BASE_URL_ENV_VARS,
            "ByteDanceArk" => BYTEDANCE_BASE_URL_ENV_VARS,
            "DeepSeek" => DEEPSEEK_BASE_URL_ENV_VARS,
            "CustomOpenAICompatible" => CUSTOM_BASE_URL_ENV_VARS,
            _ => &[],
        }
    }
}

/// 构造带超时的 HTTP 客户端：connect_timeout 让不可达端点快速失败，
/// timeout 给单次请求（含 reasoning_effort=max 深思考）一个 10 分钟上界，避免上游挂起导致
/// `send_message().await` 无限期阻塞、拖垮整个会话回合并使 web-console 长时间无响应（平台并发缺陷 #6 根因之一）。
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    http: reqwest::Client,
    config: OpenAiCompatConfig,
    api_key: Option<String>,
    base_url: String,
    endpoint: Option<String>,
    model_aliases: HashMap<String, String>,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    request_parameters: crate::RequestParameters,
}

impl OpenAiCompatClient {
    #[must_use]
    pub fn new(api_key: impl Into<String>, config: OpenAiCompatConfig) -> Self {
        Self::new_optional(Some(api_key.into()), config)
    }

    #[must_use]
    pub fn new_optional(api_key: Option<String>, config: OpenAiCompatConfig) -> Self {
        Self {
            http: build_http_client(),
            config,
            api_key: api_key.filter(|value| !value.trim().is_empty()),
            base_url: read_base_url(config),
            endpoint: None,
            model_aliases: HashMap::new(),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            request_parameters: crate::RequestParameters::default(),
        }
    }

    pub fn from_env(config: OpenAiCompatConfig) -> Result<Self, ApiError> {
        let Some(api_key) = read_first_env_non_empty(config.credential_env_vars())? else {
            return Err(ApiError::missing_credentials(
                config.provider_name,
                config.credential_env_vars(),
            ));
        };
        Ok(Self::new(api_key, config))
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
        let payload = response.json::<ChatCompletionResponse>().await?;
        let mut normalized = normalize_response(&request.model, payload)?;
        if normalized.request_id.is_none() {
            normalized.request_id = request_id;
        }
        Ok(normalized)
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
            parser: OpenAiSseParser::new(),
            pending: VecDeque::new(),
            done: false,
            state: StreamState::new(request.model.clone()),
        })
    }

    fn api_model_for(&self, requested_model: &str) -> String {
        self.model_aliases
            .get(requested_model)
            .cloned()
            .unwrap_or_else(|| requested_model.to_string())
    }

    async fn send_with_retry(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let mut attempts = 0;

        let last_error = loop {
            attempts += 1;
            let retryable_error = match self.send_raw_request(request).await {
                Ok(response) => match expect_success(response).await {
                    Ok(response) => return Ok(response),
                    Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => error,
                    Err(error) => return Err(error),
                },
                Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => error,
                Err(error) => return Err(error),
            };

            if attempts > self.max_retries {
                break retryable_error;
            }

            tokio::time::sleep(self.backoff_for_attempt(attempts)?).await;
        };

        Err(ApiError::RetriesExhausted {
            attempts,
            last_error: Box::new(last_error),
        })
    }

    async fn send_raw_request(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let request_url = self.endpoint.clone().unwrap_or_else(|| chat_completions_endpoint(&self.base_url));
        diagnostics::debug(
            "api.openai_compat",
            "send_request",
            "sending OpenAI-compatible chat completion request",
            &[
                ("provider", self.config.provider_name.to_string()),
                ("model", request.model.clone()),
                ("url", request_url.clone()),
            ],
        );
        self.request_parameters.validate_for_model(&request.model, request.reasoning_effort.as_deref(), false)?;
        let mut payload = build_chat_completion_request_for(self.config.provider_id(), request);
        self.request_parameters.apply(&mut payload, request.reasoning_effort.as_deref(), false);
        let mut request_builder = self
            .http
            .post(&request_url)
            .header("content-type", "application/json")
            .json(&payload);
        if let Some(api_key) = self
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            request_builder = request_builder.bearer_auth(api_key);
        }
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

impl Provider for OpenAiCompatClient {
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
    parser: OpenAiSseParser,
    pending: VecDeque<StreamEvent>,
    done: bool,
    state: StreamState,
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
                self.pending.extend(self.state.finish()?);
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            match self.response.chunk().await? {
                Some(chunk) => {
                    for parsed in self.parser.push(&chunk)? {
                        self.pending.extend(self.state.ingest_chunk(parsed)?);
                    }
                }
                None => {
                    self.done = true;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct OpenAiSseParser {
    buffer: Vec<u8>,
}

impl OpenAiSseParser {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatCompletionChunk>, ApiError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(frame) = next_sse_frame(&mut self.buffer) {
            if let Some(event) = parse_sse_frame(&frame)? {
                events.push(event);
            }
        }

        Ok(events)
    }
}

#[derive(Debug)]
struct StreamState {
    model: String,
    message_started: bool,
    thinking_started: bool,
    thinking_finished: bool,
    text_started: bool,
    text_finished: bool,
    finished: bool,
    stop_reason: Option<String>,
    usage: Option<Usage>,
    tool_calls: BTreeMap<u32, ToolCallState>,
}

impl StreamState {
    fn new(model: String) -> Self {
        Self {
            model,
            message_started: false,
            thinking_started: false,
            thinking_finished: false,
            text_started: false,
            text_finished: false,
            finished: false,
            stop_reason: None,
            usage: None,
            tool_calls: BTreeMap::new(),
        }
    }

    fn ingest_chunk(&mut self, chunk: ChatCompletionChunk) -> Result<Vec<StreamEvent>, ApiError> {
        let mut events = Vec::new();
        if !self.message_started {
            self.message_started = true;
            events.push(StreamEvent::MessageStart(MessageStartEvent {
                message: MessageResponse {
                    id: chunk.id.clone(),
                    kind: "message".to_string(),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    model: chunk.model.clone().unwrap_or_else(|| self.model.clone()),
                    stop_reason: None,
                    stop_sequence: None,
                    usage: Usage {
                        input_tokens: 0,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        output_tokens: 0,
                    },
                    request_id: None,
                },
            }));
        }

        if let Some(usage) = chunk.usage {
            self.usage = Some(Usage {
                input_tokens: usage.prompt_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                output_tokens: usage.completion_tokens,
            });
        }

        for choice in chunk.choices {
            if let Some(thinking) = choice
                .delta
                .reasoning_content
                .filter(|value| !value.is_empty())
            {
                if !self.thinking_started {
                    self.thinking_started = true;
                    events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                        index: 0,
                        content_block: OutputContentBlock::Thinking {
                            thinking: String::new(),
                            signature: None,
                        },
                    }));
                }
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::ThinkingDelta { thinking },
                }));
            }

            if let Some(content) = choice.delta.content.filter(|value| !value.is_empty()) {
                if !self.text_started {
                    self.text_started = true;
                    events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                        index: 0,
                        content_block: OutputContentBlock::Text {
                            text: String::new(),
                        },
                    }));
                }
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta { text: content },
                }));
            }

            for tool_call in choice.delta.tool_calls {
                let state = self.tool_calls.entry(tool_call.index).or_default();
                state.apply(tool_call);
                let block_index = state.block_index();
                if !state.started {
                    if let Some(start_event) = state.start_event()? {
                        state.started = true;
                        events.push(StreamEvent::ContentBlockStart(start_event));
                    } else {
                        continue;
                    }
                }
                if let Some(delta_event) = state.delta_event() {
                    events.push(StreamEvent::ContentBlockDelta(delta_event));
                }
                if choice.finish_reason.as_deref() == Some("tool_calls") && !state.stopped {
                    state.stopped = true;
                    events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                        index: block_index,
                    }));
                }
            }

            if let Some(finish_reason) = choice.finish_reason {
                self.stop_reason = Some(normalize_finish_reason(&finish_reason));
                if finish_reason == "tool_calls" {
                    for state in self.tool_calls.values_mut() {
                        if state.started && !state.stopped {
                            state.stopped = true;
                            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                                index: state.block_index(),
                            }));
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<StreamEvent>, ApiError> {
        if self.finished {
            return Ok(Vec::new());
        }
        self.finished = true;

        let mut events = Vec::new();
        if self.thinking_started && !self.thinking_finished {
            self.thinking_finished = true;
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: 0,
            }));
        }
        if self.text_started && !self.text_finished {
            self.text_finished = true;
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: 0,
            }));
        }

        for state in self.tool_calls.values_mut() {
            if !state.started {
                if let Some(start_event) = state.start_event()? {
                    state.started = true;
                    events.push(StreamEvent::ContentBlockStart(start_event));
                    if let Some(delta_event) = state.delta_event() {
                        events.push(StreamEvent::ContentBlockDelta(delta_event));
                    }
                }
            }
            if state.started && !state.stopped {
                state.stopped = true;
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: state.block_index(),
                }));
            }
        }

        if self.message_started {
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some(
                        self.stop_reason
                            .clone()
                            .unwrap_or_else(|| "end_turn".to_string()),
                    ),
                    stop_sequence: None,
                },
                usage: self.usage.clone().unwrap_or(Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                }),
            }));
            events.push(StreamEvent::MessageStop(MessageStopEvent {}));
        }
        Ok(events)
    }
}

#[derive(Debug, Default)]
struct ToolCallState {
    openai_index: u32,
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    emitted_len: usize,
    started: bool,
    stopped: bool,
}

impl ToolCallState {
    fn apply(&mut self, tool_call: DeltaToolCall) {
        self.openai_index = tool_call.index;
        if let Some(id) = tool_call.id {
            self.id = Some(id);
        }
        if let Some(name) = tool_call.function.name {
            self.name = Some(name);
        }
        if let Some(arguments) = tool_call.function.arguments {
            self.arguments.push_str(&arguments);
        }
    }

    const fn block_index(&self) -> u32 {
        self.openai_index + 1
    }

    fn start_event(&self) -> Result<Option<ContentBlockStartEvent>, ApiError> {
        let Some(name) = self.name.clone() else {
            return Ok(None);
        };
        let id = self
            .id
            .clone()
            .unwrap_or_else(|| format!("tool_call_{}", self.openai_index));
        Ok(Some(ContentBlockStartEvent {
            index: self.block_index(),
            content_block: OutputContentBlock::ToolUse {
                id,
                name,
                input: json!({}),
            },
        }))
    }

    fn delta_event(&mut self) -> Option<ContentBlockDeltaEvent> {
        if self.emitted_len >= self.arguments.len() {
            return None;
        }
        let delta = self.arguments[self.emitted_len..].to_string();
        self.emitted_len = self.arguments.len();
        Some(ContentBlockDeltaEvent {
            index: self.block_index(),
            delta: ContentBlockDelta::InputJsonDelta {
                partial_json: delta,
            },
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    model: String,
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

#[derive(Debug, Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseToolFunction,
}

#[derive(Debug, Deserialize)]
struct ResponseToolFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    id: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCall {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: DeltaFunction,
}

#[derive(Debug, Default, Deserialize)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(rename = "type")]
    error_type: Option<String>,
    message: Option<String>,
}

/// 构造 provider-aware Chat Completions payload。
///
/// `MessageRequest.reasoning_effort` 是兼容旧调用方的字符串，但不会被直接
/// 序列化；先交给统一 resolver，再按 provider/model 的原生协议应用字段。
pub fn build_chat_completion_request_for(provider_id: &str, request: &MessageRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = request.system.as_ref().filter(|value| !value.is_empty()) {
        messages.push(json!({
            "role": "system",
            "content": system,
        }));
    }
    for message in &request.messages {
        messages.extend(translate_message(message));
    }

    let mut payload = json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": request.stream,
    });
    if request.stream {
        payload["stream_options"] = json!({ "include_usage": true });
    }

    if let Some(tools) = &request.tools {
        payload["tools"] =
            Value::Array(tools.iter().map(openai_tool_definition).collect::<Vec<_>>());
    }
    if let Some(tool_choice) = &request.tool_choice {
        payload["tool_choice"] = openai_tool_choice(tool_choice);
    }
    let wire = resolve_reasoning(
        provider_id,
        &request.model,
        request.reasoning_effort.as_deref(),
    )
    .map(|resolution| resolution.preflight_wire)
    .unwrap_or(ReasoningWire::Omit);
    wire.apply_to_payload(&mut payload);

    payload
}

/// 兼容旧的仅 request 入口；没有 provider 身份时按未知/custom 安全处理，
/// 不主动添加 reasoning 字段。真实客户端一律调用带 canonical provider id 的入口。
#[cfg(test)]
fn build_chat_completion_request(request: &MessageRequest) -> Value {
    let model = request.model.trim().to_ascii_lowercase();
    let provider = if model.starts_with("deepseek") {
        "deepseek"
    } else if model.starts_with("glm") {
        "zhipuai"
    } else if model.starts_with("gpt") {
        "openai"
    } else if model.starts_with("grok") {
        "xai"
    } else if model.starts_with("claude") {
        "clawapi"
    } else {
        "custom"
    };
    build_chat_completion_request_for(provider, request)
}

fn translate_message(message: &InputMessage) -> Vec<Value> {
    match message.role.as_str() {
        "assistant" => {
            let mut text = String::new();
            let mut reasoning_content = String::new();
            let mut tool_calls = Vec::new();
            for block in &message.content {
                match block {
                    InputContentBlock::Text { text: value } => text.push_str(value),
                    InputContentBlock::ImageUrl { .. } => {}
                    // 历史里的工具名同样要过一遍协议归一，否则与 tools 定义中的名字对不上，
                    // 模型会认为自己调用过一个未声明的工具。
                    InputContentBlock::ToolUse { id, name, input } => tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": sanitize_openai_tool_name(name),
                            "arguments": input.to_string(),
                        }
                    })),
                    InputContentBlock::Thinking { thinking } => {
                        reasoning_content.push_str(thinking);
                    }
                    InputContentBlock::ToolResult { .. } => {}
                }
            }
            if text.is_empty() && reasoning_content.is_empty() && tool_calls.is_empty() {
                Vec::new()
            } else {
                let mut assistant = json!({
                    "role": "assistant",
                    "content": (!text.is_empty()).then_some(text),
                });
                if !reasoning_content.is_empty() {
                    assistant["reasoning_content"] = Value::String(reasoning_content);
                }
                if !tool_calls.is_empty() {
                    assistant["tool_calls"] = Value::Array(tool_calls);
                }
                vec![assistant]
            }
        }
        _ => translate_user_message(message),
    }
}

fn translate_user_message(message: &InputMessage) -> Vec<Value> {
    let mut translated = Vec::new();
    let mut content_parts = Vec::new();

    for block in &message.content {
        match block {
            InputContentBlock::Text { text } => {
                content_parts.push(UserContentPart::Text(text.clone()))
            }
            InputContentBlock::ImageUrl { url, detail } => {
                content_parts.push(UserContentPart::Image {
                    url: url.clone(),
                    detail: detail.clone(),
                });
            }
            InputContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error: _,
            } => {
                if !content_parts.is_empty() {
                    translated.push(json!({
                        "role": "user",
                        "content": build_openai_user_content(&content_parts),
                    }));
                    content_parts.clear();
                }
                translated.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": flatten_tool_result_content(content),
                }));
            }
            InputContentBlock::ToolUse { .. } => {}
            InputContentBlock::Thinking { .. } => {}
        }
    }

    if !content_parts.is_empty() {
        translated.push(json!({
            "role": "user",
            "content": build_openai_user_content(&content_parts),
        }));
    }

    translated
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UserContentPart {
    Text(String),
    Image { url: String, detail: Option<String> },
}

fn build_openai_user_content(parts: &[UserContentPart]) -> Value {
    let contains_image = parts
        .iter()
        .any(|part| matches!(part, UserContentPart::Image { .. }));
    if !contains_image {
        return Value::String(
            parts
                .iter()
                .filter_map(|part| match part {
                    UserContentPart::Text(text) => Some(text.as_str()),
                    UserContentPart::Image { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    Value::Array(
        parts
            .iter()
            .map(|part| match part {
                UserContentPart::Text(text) => json!({
                    "type": "text",
                    "text": text,
                }),
                UserContentPart::Image { url, detail } => {
                    let mut image_url = json!({ "url": url });
                    if let Some(detail) = detail {
                        image_url["detail"] = Value::String(detail.clone());
                    }
                    json!({
                        "type": "image_url",
                        "image_url": image_url,
                    })
                }
            })
            .collect(),
    )
}

fn flatten_tool_result_content(content: &[ToolResultContentBlock]) -> String {
    content
        .iter()
        .map(|block| match block {
            ToolResultContentBlock::Text { text } => text.clone(),
            ToolResultContentBlock::Json { value } => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// OpenAI Chat Completions 协议（含 DeepSeek / 百炼 / 智谱等兼容端）要求工具名满足
/// `^[a-zA-Z0-9_-]+$`。上游若用点号做命名空间（如 `computer_use.perform`），服务端会直接
/// 返回 `400 Invalid 'tools[i].function.name'`，整轮请求失败、上层降级成本地回退文案，
/// 用户视角就是"会话没有回复内容"。故在协议边界统一把非法字符替换为 `_`。
///
/// 注意这是**兜底**：替换不可逆（`a.b` 与 `a_b` 会撞名），模型回传的也是替换后的名字。
/// 正确做法仍是上游直接用合法工具名，此处只保证请求不会因为命名被整体拒绝。
fn sanitize_openai_tool_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn openai_tool_definition(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": sanitize_openai_tool_name(&tool.name),
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

fn openai_tool_choice(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::Any => Value::String("required".to_string()),
        ToolChoice::Tool { name } => json!({
            "type": "function",
            "function": { "name": sanitize_openai_tool_name(name) },
        }),
    }
}

fn normalize_response(
    model: &str,
    response: ChatCompletionResponse,
) -> Result<MessageResponse, ApiError> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or(ApiError::InvalidSseFrame(
            "chat completion response missing choices",
        ))?;
    let mut content = Vec::new();
    if let Some(thinking) = choice
        .message
        .reasoning_content
        .filter(|value| !value.is_empty())
    {
        content.push(OutputContentBlock::Thinking {
            thinking,
            signature: None,
        });
    }
    if let Some(text) = choice.message.content.filter(|value| !value.is_empty()) {
        content.push(OutputContentBlock::Text { text });
    }
    for tool_call in choice.message.tool_calls {
        content.push(OutputContentBlock::ToolUse {
            id: tool_call.id,
            name: tool_call.function.name,
            input: parse_tool_arguments(&tool_call.function.arguments),
        });
    }

    Ok(MessageResponse {
        id: response.id,
        kind: "message".to_string(),
        role: choice.message.role,
        content,
        model: response.model.if_empty_then(model.to_string()),
        stop_reason: choice
            .finish_reason
            .map(|value| normalize_finish_reason(&value)),
        stop_sequence: None,
        usage: Usage {
            input_tokens: response
                .usage
                .as_ref()
                .map_or(0, |usage| usage.prompt_tokens),
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: response
                .usage
                .as_ref()
                .map_or(0, |usage| usage.completion_tokens),
        },
        request_id: None,
    })
}

fn parse_tool_arguments(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| json!({ "raw": arguments }))
}

fn next_sse_frame(buffer: &mut Vec<u8>) -> Option<String> {
    let separator = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| (position, 4))
        })?;

    let (position, separator_len) = separator;
    let frame = buffer.drain(..position + separator_len).collect::<Vec<_>>();
    let frame_len = frame.len().saturating_sub(separator_len);
    Some(String::from_utf8_lossy(&frame[..frame_len]).into_owned())
}

fn parse_sse_frame(frame: &str) -> Result<Option<ChatCompletionChunk>, ApiError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut data_lines = Vec::new();
    for line in trimmed.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(None);
    }
    serde_json::from_str(&payload)
        .map(Some)
        .map_err(ApiError::from)
}

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn read_first_env_non_empty(keys: &[&str]) -> Result<Option<String>, ApiError> {
    for key in keys {
        if let Some(value) = read_env_non_empty(key)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

#[must_use]
pub fn has_api_key(key: &str) -> bool {
    read_env_non_empty(key)
        .ok()
        .and_then(std::convert::identity)
        .is_some()
}

#[must_use]
pub fn read_base_url(config: OpenAiCompatConfig) -> String {
    config
        .base_url_env_vars()
        .iter()
        .find_map(|env| std::env::var(env).ok())
        .or_else(|| std::env::var("apiBaseUrl").ok())
        .or_else(|| std::env::var("API_BASE_URL").ok())
        .unwrap_or_else(|| config.default_base_url.to_string())
}

fn chat_completions_endpoint(base_url: &str) -> String {
    EndpointResolver::resolve(base_url, ProviderProtocol::OpenAiChatCompletions, None)
        .unwrap_or_else(|_| {
            let trimmed = base_url.trim_end_matches('/');
            if trimmed.ends_with("/chat/completions") {
                trimmed.to_string()
            } else {
                format!("{trimmed}/chat/completions")
            }
        })
}

fn request_id_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .or_else(|| headers.get(ALT_REQUEST_ID_HEADER))
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await.unwrap_or_default();
    let parsed_error = serde_json::from_str::<ErrorEnvelope>(&body).ok();
    let retryable = is_retryable_status(status);

    Err(ApiError::Api {
        status,
        error_type: parsed_error
            .as_ref()
            .and_then(|error| error.error.error_type.clone()),
        message: parsed_error
            .as_ref()
            .and_then(|error| error.error.message.clone()),
        body,
        retryable,
    })
}

const fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 409 | 429 | 500 | 502 | 503 | 504)
}

fn normalize_finish_reason(value: &str) -> String {
    match value {
        "stop" => "end_turn",
        "tool_calls" => "tool_use",
        other => other,
    }
    .to_string()
}

trait StringExt {
    fn if_empty_then(self, fallback: String) -> String;
}

impl StringExt for String {
    fn if_empty_then(self, fallback: String) -> String {
        if self.is_empty() {
            fallback
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_chat_completion_request, build_chat_completion_request_for, chat_completions_endpoint,
        normalize_finish_reason, openai_tool_choice, parse_tool_arguments, read_base_url,
        OpenAiCompatClient, OpenAiCompatConfig,
    };
    use crate::error::ApiError;
    use crate::types::{
        InputContentBlock, InputMessage, MessageRequest, ToolChoice, ToolDefinition,
        ToolResultContentBlock,
    };
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn request_translation_uses_openai_compatible_shape() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "deepseek-v4-pro".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage {
                role: "user".to_string(),
                content: vec![
                    InputContentBlock::Text {
                        text: "hello".to_string(),
                    },
                    InputContentBlock::ToolResult {
                        tool_use_id: "tool_1".to_string(),
                        content: vec![ToolResultContentBlock::Json {
                            value: json!({"ok": true}),
                        }],
                        is_error: false,
                    },
                ],
            }],
            system: Some("be helpful".to_string()),
            tools: Some(vec![ToolDefinition {
                name: "weather".to_string(),
                description: Some("Get weather".to_string()),
                input_schema: json!({"type": "object"}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            reasoning_effort: Some("high".to_string()),
            stream: false,
        });

        assert_eq!(payload["messages"][0]["role"], json!("system"));
        assert_eq!(payload["messages"][1]["role"], json!("user"));
        assert_eq!(payload["messages"][2]["role"], json!("tool"));
        assert_eq!(payload["tools"][0]["type"], json!("function"));
        assert_eq!(payload["tool_choice"], json!("auto"));
        assert_eq!(payload["reasoning_effort"], json!("high"));
    }

    #[test]
    fn tool_names_with_illegal_chars_are_sanitized_for_protocol() {
        // 带点号的工具名（如 computer_use.perform）不满足 OpenAI 兼容端要求的
        // ^[a-zA-Z0-9_-]+$，直接下发会被 400 拒绝，整轮请求失败。
        let payload = build_chat_completion_request(&MessageRequest {
            model: "deepseek-v4-pro".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage {
                role: "assistant".to_string(),
                content: vec![InputContentBlock::ToolUse {
                    id: "cu-1".to_string(),
                    name: "computer_use.perform".to_string(),
                    input: json!({}),
                }],
            }],
            system: None,
            tools: Some(vec![
                ToolDefinition {
                    name: "tools_semantic_dispatch".to_string(),
                    description: None,
                    input_schema: json!({"type": "object"}),
                },
                ToolDefinition {
                    name: "computer_use.perform".to_string(),
                    description: None,
                    input_schema: json!({"type": "object"}),
                },
            ]),
            tool_choice: Some(ToolChoice::Tool {
                name: "computer_use.perform".to_string(),
            }),
            reasoning_effort: None,
            stream: false,
        });

        // 合法名原样保留，非法名的点号归一成下划线。
        assert_eq!(payload["tools"][0]["function"]["name"], json!("tools_semantic_dispatch"));
        assert_eq!(payload["tools"][1]["function"]["name"], json!("computer_use_perform"));
        assert_eq!(payload["tool_choice"]["function"]["name"], json!("computer_use_perform"));
        // 历史 tool_calls 也要一致，否则模型会看到未声明的工具名。
        assert_eq!(
            payload["messages"][0]["tool_calls"][0]["function"]["name"],
            json!("computer_use_perform")
        );

        // 出站的每个工具名都必须满足协议正则。
        for tool in payload["tools"].as_array().expect("tools") {
            let name = tool["function"]["name"].as_str().expect("name");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "illegal tool name leaked to protocol: {name}"
            );
        }
    }

    #[test]
    fn reasoning_effort_uses_provider_native_mapping() {
        // DeepSeek V4 的 medium/xhigh 均有明确的预解析结果，不能再统一钳位或
        // 把未知值默认为 medium。
        for (raw, expected) in [("max", "max"), ("xhigh", "high")] {
            let payload = build_chat_completion_request(&MessageRequest {
                model: "deepseek-v4-pro".to_string(),
                max_tokens: 64,
                messages: vec![InputMessage::user_text("hi")],
                system: None,
                tools: None,
                tool_choice: None,
                reasoning_effort: Some(raw.to_string()),
                stream: false,
            });
            assert_eq!(
                payload["reasoning_effort"],
                json!(expected),
                "reasoning_effort={raw} 应使用 DeepSeek 原生映射"
            );
        }
        // Zhipu GLM-5.2 的 xhigh 按兼容语义映射为 max。
        let payload = build_chat_completion_request_for(
            "zhipuai",
            &MessageRequest {
                model: "glm-5.2".to_string(),
                max_tokens: 64,
                messages: vec![InputMessage::user_text("hi")],
                system: None,
                tools: None,
                tool_choice: None,
                reasoning_effort: Some("xhigh".to_string()),
                stream: false,
            },
        );
        assert_eq!(payload["reasoning_effort"], json!("max"));

        // 未知值不能静默变 medium，安全结果是 omission。
        let payload = build_chat_completion_request(&MessageRequest {
            model: "deepseek-v4-pro".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hi")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some("turbo".to_string()),
            stream: false,
        });
        assert!(payload.get("reasoning_effort").is_none());
    }

    #[test]
    fn xai_current_models_emit_only_explicit_native_reasoning_effort() {
        let request = |model: &str, effort: &str| MessageRequest {
            model: model.to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hi")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some(effort.to_string()),
            stream: false,
        };
        let grok46 = build_chat_completion_request_for("xai", &request("grok-4.6", "xhigh"));
        assert_eq!(grok46["reasoning_effort"], json!("xhigh"));
        assert!(grok46.get("thinking").is_none());

        let grok45 = build_chat_completion_request_for("xai", &request("grok-4.5", "xhigh"));
        assert_eq!(grok45["reasoning_effort"], json!("high"));

        let old = build_chat_completion_request_for("xai", &request("grok-3", "max"));
        assert!(old.get("reasoning_effort").is_none());
    }

    #[test]
    fn provider_payload_reasoning_safety_matrix_has_no_cross_provider_leaks() {
        let request = |model: &str, effort: &str| MessageRequest {
            model: model.to_string(),
            max_tokens: 64,
            messages: vec![InputMessage::user_text("hi")],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some(effort.to_string()),
            stream: false,
        };

        let openai = build_chat_completion_request_for("openai", &request("gpt-4.1", "medium"));
        assert!(openai.get("reasoning_effort").is_none());

        let deepseek =
            build_chat_completion_request_for("deepseek", &request("deepseek-v4-pro", "none"));
        assert_eq!(deepseek["thinking"]["type"], json!("disabled"));
        assert!(deepseek.get("reasoning_effort").is_none());

        let custom = build_chat_completion_request_for("custom", &request("my-local-model", "high"));
        assert!(custom.get("reasoning_effort").is_none());

        let grok43 = build_chat_completion_request_for("xai", &request("grok-4.3", "none"));
        assert_eq!(grok43["reasoning_effort"], json!("none"));
    }

    #[test]
    fn tool_result_translation_uses_standard_openai_shape_without_is_error() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "gpt-4.1".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage {
                role: "user".to_string(),
                content: vec![InputContentBlock::ToolResult {
                    tool_use_id: "call_123".to_string(),
                    content: vec![ToolResultContentBlock::Text {
                        text: "failed safely".to_string(),
                    }],
                    is_error: true,
                }],
            }],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: false,
        });

        assert_eq!(payload["messages"][0]["role"], json!("tool"));
        assert_eq!(payload["messages"][0]["tool_call_id"], json!("call_123"));
        assert_eq!(payload["messages"][0]["content"], json!("failed safely"));
        assert!(payload["messages"][0].get("is_error").is_none());
    }

    #[test]
    fn streaming_request_asks_provider_to_include_usage() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "glm-5.1".to_string(),
            max_tokens: 512,
            messages: vec![InputMessage::user_text("hello".to_string())],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: true,
        });

        assert_eq!(payload["stream"], json!(true));
        assert_eq!(payload["stream_options"]["include_usage"], json!(true));
    }

    #[test]
    fn request_translation_supports_multimodal_user_content() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "glm-4.6v-flash".to_string(),
            max_tokens: 512,
            messages: vec![InputMessage {
                role: "user".to_string(),
                content: vec![
                    InputContentBlock::Text {
                        text: "请描述这张图".to_string(),
                    },
                    InputContentBlock::ImageUrl {
                        url: "data:image/png;base64,AAA".to_string(),
                        detail: Some("high".to_string()),
                    },
                ],
            }],
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: false,
        });

        assert_eq!(payload["messages"][0]["role"], json!("user"));
        assert_eq!(payload["messages"][0]["content"][0]["type"], json!("text"));
        assert_eq!(
            payload["messages"][0]["content"][1]["type"],
            json!("image_url")
        );
        assert_eq!(
            payload["messages"][0]["content"][1]["image_url"]["url"],
            json!("data:image/png;base64,AAA")
        );
        assert_eq!(
            payload["messages"][0]["content"][1]["image_url"]["detail"],
            json!("high")
        );
    }

    #[test]
    fn assistant_text_history_omits_empty_tool_calls() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "deepseek-v4-pro".to_string(),
            max_tokens: 256,
            messages: vec![InputMessage {
                role: "assistant".to_string(),
                content: vec![InputContentBlock::Text {
                    text: "上一轮普通回复，没有调用工具。".to_string(),
                }],
            }],
            system: None,
            tools: Some(vec![ToolDefinition {
                name: "read_file".to_string(),
                description: Some("Read a file".to_string()),
                input_schema: json!({"type": "object"}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            reasoning_effort: None,
            stream: false,
        });

        assert_eq!(payload["messages"][0]["role"], json!("assistant"));
        assert_eq!(
            payload["messages"][0]["content"],
            json!("上一轮普通回复，没有调用工具。")
        );
        assert!(payload["messages"][0].get("tool_calls").is_none());
    }

    #[test]
    fn assistant_tool_history_preserves_reasoning_content() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "deepseek-v4-pro".to_string(),
            max_tokens: 256,
            messages: vec![InputMessage {
                role: "assistant".to_string(),
                content: vec![
                    InputContentBlock::Thinking {
                        thinking: "I should inspect the file before summarizing.".to_string(),
                    },
                    InputContentBlock::ToolUse {
                        id: "call_read".to_string(),
                        name: "read_file".to_string(),
                        input: json!({"path":"coolzhu.toml"}),
                    },
                ],
            }],
            system: None,
            tools: Some(vec![ToolDefinition {
                name: "read_file".to_string(),
                description: Some("Read a file".to_string()),
                input_schema: json!({"type": "object"}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            reasoning_effort: None,
            stream: false,
        });

        assert_eq!(
            payload["messages"][0]["reasoning_content"],
            json!("I should inspect the file before summarizing.")
        );
        assert_eq!(
            payload["messages"][0]["tool_calls"][0]["id"],
            json!("call_read")
        );
    }

    #[test]
    fn tool_choice_translation_supports_required_function() {
        assert_eq!(openai_tool_choice(&ToolChoice::Any), json!("required"));
        assert_eq!(
            openai_tool_choice(&ToolChoice::Tool {
                name: "weather".to_string(),
            }),
            json!({"type": "function", "function": {"name": "weather"}})
        );
    }

    #[test]
    fn parses_tool_arguments_fallback() {
        assert_eq!(
            parse_tool_arguments("{\"city\":\"Paris\"}"),
            json!({"city": "Paris"})
        );
        assert_eq!(parse_tool_arguments("not-json"), json!({"raw": "not-json"}));
    }

    #[test]
    fn missing_xai_api_key_is_provider_specific() {
        let _lock = env_lock();
        std::env::remove_var("XAI_API_KEY");
        let error = OpenAiCompatClient::from_env(OpenAiCompatConfig::xai())
            .expect_err("missing key should error");
        assert!(matches!(
            error,
            ApiError::MissingCredentials {
                provider: "xAI",
                ..
            }
        ));
    }

    #[test]
    fn endpoint_builder_accepts_base_urls_and_full_endpoints() {
        assert_eq!(
            chat_completions_endpoint("https://api.x.ai/v1"),
            "https://api.x.ai/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://api.x.ai/v1/"),
            "https://api.x.ai/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://api.x.ai/v1/chat/completions"),
            "https://api.x.ai/v1/chat/completions"
        );
    }

    #[test]
    fn zhipu_config_prefers_provider_specific_env_vars() {
        let _lock = env_lock();
        std::env::remove_var("ZAI_BASE_URL");
        std::env::remove_var("BIGMODEL_BASE_URL");
        std::env::remove_var("OPENAI_BASE_URL");
        std::env::set_var("OPENAI_BASE_URL", "https://fallback.example/v1");
        std::env::set_var("ZAI_BASE_URL", "https://zhipu.example/v4");
        std::env::set_var("BIGMODEL_API_KEY", "bigmodel-key");
        std::env::remove_var("ZAI_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");

        let client = OpenAiCompatClient::from_env(OpenAiCompatConfig::zhipu())
            .expect("zhipu config should accept fallback credentials");

        assert_eq!(
            read_base_url(OpenAiCompatConfig::zhipu()),
            "https://zhipu.example/v4"
        );
        drop(client);

        std::env::remove_var("ZAI_BASE_URL");
        std::env::remove_var("BIGMODEL_API_KEY");
        std::env::remove_var("OPENAI_BASE_URL");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn zhipu_client_sends_openai_compatible_requests() {
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
            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(request.contains("authorization: Bearer zhipu-test-key"));
            assert!(request.contains("\"model\":\"glm-4.7-flash\""));

            let response_body = r#"{
  "id":"chatcmpl-zhipu-test",
  "model":"glm-4.7-flash",
  "choices":[
    {
      "message":{"role":"assistant","content":"hello from zhipu","tool_calls":[]},
      "finish_reason":"stop"
    }
  ],
  "usage":{"prompt_tokens":5,"completion_tokens":7}
}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nx-request-id: req-zhipu-test\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("response should write");
        });

        std::env::set_var("ZAI_API_KEY", "zhipu-test-key");
        std::env::set_var("ZAI_BASE_URL", format!("http://{address}/v1"));

        let client = OpenAiCompatClient::from_env(OpenAiCompatConfig::zhipu())
            .expect("zhipu client should build");
        let response = client
            .send_message(&MessageRequest {
                model: "glm-4.7-flash".to_string(),
                max_tokens: 128,
                messages: vec![InputMessage::user_text("hello")],
                system: Some("be helpful".to_string()),
                tools: None,
                tool_choice: None,
                reasoning_effort: None,
                stream: false,
            })
            .await
            .expect("request should succeed");

        assert_eq!(response.model, "glm-4.7-flash");
        assert_eq!(response.request_id.as_deref(), Some("req-zhipu-test"));
        assert_eq!(response.usage.input_tokens, 5);
        assert_eq!(response.usage.output_tokens, 7);

        server.await.expect("server task should finish");
        std::env::remove_var("ZAI_API_KEY");
        std::env::remove_var("ZAI_BASE_URL");
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

    #[test]
    fn normalizes_stop_reasons() {
        assert_eq!(normalize_finish_reason("stop"), "end_turn");
        assert_eq!(normalize_finish_reason("tool_calls"), "tool_use");
    }
}
