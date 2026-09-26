//! 请求事实观察接口；不负责排空、取消或计费，也不持有请求正文与凭据。
use std::sync::Arc;
use serde_json::Value;
use crate::{ApiError, Usage};

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
}
impl Default for RequestAttemptSnapshot {
    fn default() -> Self { Self { attempt_no:1,status:"prepared",dispatched:false,usage:UsageEvidence::default(),http_status:None,terminal:false } }
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
        self.snapshot=RequestAttemptSnapshot { attempt_no,..RequestAttemptSnapshot::default() }; self.publish();
    }
    pub(crate) fn dispatch(&mut self) { self.snapshot.dispatched=true;self.snapshot.status="dispatched";self.publish(); }
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
