//! 原发送请求随响应流转；摘要只记录 adapter 真正发送的工具和参数，不保存授权。
use api::{MessageRequest, RequestAttemptSnapshot};
use serde::{Deserialize, Serialize};
use std::{future::Future, sync::{Arc, Mutex}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RequestBinding {
    pub(crate) request_id: String,
    pub(crate) attempt_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parent_request_id: Option<String>,
    pub(crate) turn_id: String,
    pub(crate) model: String,
    pub(crate) protocol: String,
    pub(crate) tool_names: Vec<String>,
    pub(crate) tools_digest: String,
    pub(crate) parameters_digest: String,
    pub(crate) stream: bool,
}

impl RequestBinding {
    pub(crate) fn from_attempt(request_id: &str, parent: Option<&str>, turn_id: &str,
        attempt: &RequestAttemptSnapshot) -> Option<Self> {
        let wire = attempt.wire.as_ref()?;
        if !attempt.dispatched { return None; }
        Some(Self {
            request_id: request_id.into(),
            attempt_id: format!("{request_id}:{}", attempt.attempt_no),
            parent_request_id: parent.map(str::to_owned), turn_id: turn_id.into(),
            model: wire.model.clone(), protocol: attempt.protocol.clone().unwrap_or_default(),
            tool_names: wire.tool_names.clone(), tools_digest: wire.tools_digest.clone(),
            parameters_digest: wire.parameters_digest.clone(), stream: wire.stream,
        })
    }
}

/// 每个真实 SDK 请求自己的回执，既有 observer 更新它；不使用全局 turn 映射。
#[derive(Debug, Clone, Default)]
pub(crate) struct Receipt(Arc<Mutex<Option<RequestBinding>>>);
impl Receipt {
    pub(crate) fn update(&self, binding: RequestBinding) {
        *self.0.lock().unwrap_or_else(|error| error.into_inner()) = Some(binding);
    }
    pub(crate) fn sent(&self, request: MessageRequest) -> Result<SentRequest, api::ApiError> {
        let binding = self.0.lock().unwrap_or_else(|error| error.into_inner()).clone()
            .ok_or_else(|| api::ApiError::ConfigError {
                path: "request_binding".into(), message: "实际请求回执缺失，不能推导旧响应的工具集合".into(),
            })?;
        Ok(SentRequest { request, binding })
    }
}

#[derive(Debug)]
pub(crate) struct SentRequest {
    pub(crate) request: MessageRequest,
    pub(crate) binding: RequestBinding,
}
impl SentRequest {
    pub(crate) fn exposes(&self, name: &str) -> bool {
        self.request.tools.as_deref().is_some_and(|definitions| definitions.iter()
            .filter(|definition| self.binding.tool_names.contains(&definition.name))
            .any(|definition| crate::tool_call_name_is_exposed(name, Some(std::slice::from_ref(definition)))))
    }
}
pub(crate) struct InitialResponse {
    pub(crate) response: api::MessageResponse,
    pub(crate) usage_known: bool,
    pub(crate) sent: SentRequest,
}

pub(crate) type Collector = Arc<Mutex<Vec<RequestBinding>>>;
tokio::task_local! { static COLLECTOR: Collector; }
pub(crate) fn current_collector() -> Option<Collector> { COLLECTOR.try_with(Arc::clone).ok() }
pub(crate) async fn with_collector<F: Future>(collector: Option<Collector>, future: F) -> F::Output {
    match collector { Some(collector) => COLLECTOR.scope(collector, future).await, None => future.await }
}
pub(crate) fn record(collector: Option<&Collector>, binding: &RequestBinding) {
    if let Some(collector) = collector {
        let mut bindings = collector.lock().unwrap_or_else(|error| error.into_inner());
        if !bindings.iter().any(|entry| entry.attempt_id == binding.attempt_id) { bindings.push(binding.clone()); }
    }
}
pub(crate) async fn collect<F: Future>(future: F) -> (F::Output, Vec<RequestBinding>) {
    let collector = Arc::new(Mutex::new(Vec::new()));
    let result = COLLECTOR.scope(Arc::clone(&collector), future).await;
    let bindings = collector.lock().unwrap_or_else(|error| error.into_inner()).clone();
    (result, bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_distinguishes_transport_attempts_from_new_requests_without_secrets() {
        let event = RequestAttemptSnapshot { dispatched: true,
            wire: Some(api::RequestWireSummary::from_payload(&serde_json::json!({
                "model":"fixture", "stream":true, "temperature":0.3,
                "messages":[{"role":"user","content":"private body"}], "system":"private system",
                "tools":[{"type":"function","function":{"name":"read_file","parameters":{"type":"object"}}}]
            }))), ..Default::default() };
        let first = RequestBinding::from_attempt("actual-1", None, "turn", &event).unwrap();
        let retry = RequestBinding::from_attempt("actual-1", None, "turn", &RequestAttemptSnapshot {
            attempt_no:2, ..event.clone() }).unwrap();
        let sampled = RequestBinding::from_attempt("actual-2", Some(&first.request_id), "turn", &event).unwrap();
        assert_eq!(first.request_id, retry.request_id);
        assert_ne!(first.attempt_id, retry.attempt_id);
        assert_eq!(first.tools_digest, retry.tools_digest);
        assert_ne!(first.request_id, sampled.request_id);
        assert_eq!(sampled.parent_request_id.as_deref(), Some("actual-1"));
        let encoded = serde_json::to_string(&first).unwrap();
        for forbidden in ["private body", "private system", "authorized", "permission"] { assert!(!encoded.contains(forbidden)); }
        assert!(RequestBinding::from_attempt("unsent", None, "turn", &RequestAttemptSnapshot::default()).is_none());
    }
}
