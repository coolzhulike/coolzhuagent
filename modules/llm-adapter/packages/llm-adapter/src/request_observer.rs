//! 请求事实观察接口；不负责排空、取消或计费，也不持有请求正文与凭据。
use std::sync::Arc;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use crate::{ApiError, Usage};

/// 只投影实际发送的工具和有效参数；不保存正文、system、端点凭据或原始 schema。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestWireSummary {
    pub model: String,
    pub stream: bool,
    pub tool_names: Vec<String>,
    pub tools_digest: String,
    pub parameters_digest: String,
}

impl RequestWireSummary {
    pub fn from_payload(payload: &Value) -> Self {
        let tools = payload.get("tools").unwrap_or(&Value::Null);
        let parameters = Value::Object(payload.as_object().into_iter().flat_map(|object| object.iter())
            .filter(|(key, _)| !matches!(key.as_str(), "messages" | "system" | "tools"))
            .map(|(key, value)| (key.clone(), value.clone())).collect());
        let digest = |value: &Value| format!("sha256:{:x}", Sha256::digest(value.to_string().as_bytes()));
        Self {
            model: payload["model"].as_str().unwrap_or_default().into(),
            stream: payload["stream"].as_bool().unwrap_or(false),
            tool_names: tools.as_array().into_iter().flatten().filter_map(|tool|
                tool.pointer("/function/name").or_else(|| tool.get("name")).and_then(Value::as_str).map(str::to_owned)).collect(),
            tools_digest: digest(tools),
            parameters_digest: digest(&parameters),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageEvidence {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
    pub cache_write_tokens: Option<u32>,
}

impl UsageEvidence {
    pub fn merge(&mut self, other: Self) {
        fn merge(a: &mut Option<u32>, b: Option<u32>) { if let Some(b) = b { *a = Some(a.unwrap_or(0).max(b)); } }
        merge(&mut self.input_tokens, other.input_tokens);
        merge(&mut self.output_tokens, other.output_tokens);
        merge(&mut self.cache_read_tokens, other.cache_read_tokens);
        merge(&mut self.cache_write_tokens, other.cache_write_tokens);
    }
    pub fn known_mask(self) -> u8 {
        u8::from(self.input_tokens.is_some()) | (u8::from(self.output_tokens.is_some()) << 1)
            | (u8::from(self.cache_read_tokens.is_some()) << 2) | (u8::from(self.cache_write_tokens.is_some()) << 3)
    }
    pub fn usage(self) -> Usage { Usage { input_tokens: self.input_tokens.unwrap_or(0), output_tokens: self.output_tokens.unwrap_or(0),
        cache_read_input_tokens: self.cache_read_tokens.unwrap_or(0), cache_creation_input_tokens: self.cache_write_tokens.unwrap_or(0) } }
    pub fn anthropic(value: &Value) -> Self {
        fn n(v: &Value, key: &str) -> Option<u32> { v.get(key)?.as_u64()?.try_into().ok() }
        Self { input_tokens: n(value,"input_tokens"), output_tokens: n(value,"output_tokens"),
            cache_read_tokens: n(value,"cache_read_input_tokens"), cache_write_tokens: n(value,"cache_creation_input_tokens") }
    }
    pub(crate) fn anthropic_frame(frame: &str) -> Self {
        let payload = frame.lines().filter_map(|line| line.strip_prefix("data:").map(str::trim_start)).collect::<Vec<_>>().join("\n");
        let Ok(value) = serde_json::from_str::<Value>(&payload) else { return Self::default() };
        Self::anthropic(value.pointer("/message/usage").or_else(|| value.get("usage")).unwrap_or(&Value::Null))
    }
}

#[derive(Debug, Clone)]
pub struct RequestAttemptSnapshot {
    pub attempt_no: u32,
    pub status: &'static str,
    pub dispatched: bool,
    pub usage: UsageEvidence,
    pub http_status: Option<u16>,
    pub terminal: bool,
    pub model: Option<String>,
    pub protocol: Option<String>,
    pub started_at_ms: Option<u64>,
    pub first_token_at_ms: Option<u64>,
    pub wire: Option<RequestWireSummary>,
}
impl Default for RequestAttemptSnapshot {
    fn default() -> Self { Self { attempt_no:1,status:"prepared",dispatched:false,usage:UsageEvidence::default(),http_status:None,terminal:false,model:None,protocol:None,started_at_ms:None,first_token_at_ms:None,wire:None } }
}
pub trait RequestObserver: std::fmt::Debug + Send + Sync {
    fn update(&self, snapshot: &RequestAttemptSnapshot);
}

/// 每次 SDK 调用独占一个观察生命周期；每次 HTTP 重试独占一个编号。
#[derive(Debug)]
pub(crate) struct RequestObservation {
    observer: Option<Arc<dyn RequestObserver>>,
    snapshot: RequestAttemptSnapshot,
}
impl RequestObservation {
    pub(crate) fn new(observer: Option<Arc<dyn RequestObserver>>) -> Self {
        let this=Self { observer,snapshot:RequestAttemptSnapshot::default() }; this.publish(); this
    }
    fn publish(&self) { if let Some(observer)=&self.observer { observer.update(&self.snapshot); } }
    pub(crate) fn begin(&mut self, attempt_no:u32) {
        self.snapshot=RequestAttemptSnapshot { attempt_no,model:self.snapshot.model.clone(),protocol:self.snapshot.protocol.clone(),started_at_ms:Some(now_ms()),..RequestAttemptSnapshot::default() }; self.publish();
    }
    pub(crate) fn configure(&mut self, model: &str, protocol: &str) {
        self.snapshot.model=Some(model.into()); self.snapshot.protocol=Some(protocol.into());
    }
    pub(crate) fn stream_events<'a>(&mut self, events: impl IntoIterator<Item=&'a crate::StreamEvent>) {
        if self.snapshot.first_token_at_ms.is_none() && events.into_iter().any(|event| matches!(event,crate::StreamEvent::ContentBlockDelta(_))) {
            self.snapshot.first_token_at_ms=Some(now_ms()); self.publish();
        }
    }
    pub(crate) fn dispatch(&mut self, payload: &Value) {
        // 与下面 send 使用同一个最终 payload；重试沿用逻辑请求，只更新真实 attempt。
        if self.observer.is_some() { self.snapshot.wire = Some(RequestWireSummary::from_payload(payload)); }
        self.snapshot.dispatched=true;self.snapshot.status="dispatched";self.publish();
    }
    pub(crate) fn usage(&mut self, usage:UsageEvidence) {
        let before=self.snapshot.usage;
        self.snapshot.usage.merge(usage);
        if before!=self.snapshot.usage { self.publish(); }
    }
    pub(crate) fn evidence(&self) -> UsageEvidence { self.snapshot.usage }
    pub(crate) fn finish(&mut self, status:&'static str) {
        if self.snapshot.terminal { return; }
        self.snapshot.status=status;self.snapshot.terminal=true;self.publish();
    }
    pub(crate) fn fail(&mut self, error:&ApiError) {
        let status=match error {
            ApiError::Api {status,..} => {self.snapshot.http_status=Some(status.as_u16());"http_error"},
            ApiError::Http(error) if error.is_timeout()=>"timeout",
            ApiError::Http(_)=>"network_error",
            ApiError::Json(_) | ApiError::InvalidSseFrame(_)=>"protocol_error",
            ApiError::RetriesExhausted {last_error,..}=>{self.fail(last_error);return;},
            _=>"configuration_error",
        }; self.finish(status);
    }
}
impl Drop for RequestObservation {
    fn drop(&mut self) { self.finish(if self.snapshot.dispatched {"remote_unknown"} else {"abandoned_before_dispatch"}); }
}

fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64 }
