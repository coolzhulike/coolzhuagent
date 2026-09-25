//! S0 合成模型回放服务。
//!
//! 该模块只在测试中编译。它读取仓库内的脱敏 fixture，提供 OpenAI 兼容的
//! `/v1/chat/completions` 最小端点，并记录每个请求；不会读取用户配置、密钥、
//! 附件或执行任何工具。

use std::sync::{Arc, Mutex};

use axum::{
    http::header,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

const S0_GOLDEN_FIXTURE: &str =
    include_str!("../../../../../tests/fixtures/s0-golden/manifest.json");

pub(crate) struct FixtureModelServer {
    pub(crate) base_url: String,
    captured_requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl FixtureModelServer {
    pub(crate) async fn spawn(scenario_id: &str) -> Result<Self, String> {
        let fixture: Value = serde_json::from_str(S0_GOLDEN_FIXTURE)
            .map_err(|error| format!("S0 fixture JSON 无法解析：{error}"))?;
        if fixture["schema_version"].as_str() != Some("coolzhu.s0-golden-fixture.v1") {
            return Err("S0 fixture schema_version 不受支持".to_string());
        }
        let scenario = fixture["scenarios"]
            .as_array()
            .and_then(|scenarios| {
                scenarios
                    .iter()
                    .find(|candidate| candidate["id"].as_str() == Some(scenario_id))
            })
            .cloned()
            .ok_or_else(|| format!("S0 fixture 中不存在场景：{scenario_id}"))?;
        let captured_requests = Arc::new(Mutex::new(Vec::new()));
        let captured_for_handler = Arc::clone(&captured_requests);
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |Json(request): Json<Value>| {
                let captured_requests = Arc::clone(&captured_for_handler);
                let scenario = scenario.clone();
                async move {
                    captured_requests
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(request.clone());
                    fixture_model_response(&scenario, &request)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("无法绑定 S0 假模型端口：{error}"))?;
        let base_url = format!(
            "http://{}",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        let task = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("S0 假模型服务异常退出：{error}");
            }
        });
        Ok(Self {
            base_url,
            captured_requests,
            task,
        })
    }

    pub(crate) fn captured_requests(&self) -> Vec<Value> {
        self.captured_requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl Drop for FixtureModelServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn fixture_model_response(scenario: &Value, request: &Value) -> Response {
    let scenario_id = scenario["id"].as_str().unwrap_or("unknown-scenario");
    let tool_call = fixture_tool_call(scenario);
    let streaming = request["stream"].as_bool().unwrap_or(false);
    if streaming {
        let delta = match tool_call {
            Some((call_id, tool_name)) => json!({
                "role": "assistant",
                "tool_calls": [{
                    "index": 0,
                    "id": call_id,
                    "type": "function",
                    "function": {"name": tool_name, "arguments": "{}"}
                }]
            }),
            None => {
                json!({"role": "assistant", "content": format!("S0 fixture stream: {scenario_id}")})
            }
        };
        let finish_reason = if tool_call.is_some() {
            "tool_calls"
        } else {
            "stop"
        };
        let chunk = json!({
            "id": format!("fixture-{scenario_id}"),
            "object": "chat.completion.chunk",
            "model": request["model"],
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}]
        });
        return (
            [(header::CONTENT_TYPE, "text/event-stream; charset=utf-8")],
            format!("data: {chunk}\n\ndata: [DONE]\n\n"),
        )
            .into_response();
    }

    let message = match tool_call {
        Some((call_id, tool_name)) => json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": {"name": tool_name, "arguments": "{}"}
            }]
        }),
        None => {
            json!({"role": "assistant", "content": format!("S0 fixture response: {scenario_id}")})
        }
    };
    Json(json!({
        "id": format!("fixture-{scenario_id}"),
        "object": "chat.completion",
        "model": request["model"],
        "choices": [{"index": 0, "message": message, "finish_reason": if fixture_tool_call(scenario).is_some() { "tool_calls" } else { "stop" }}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
    }))
    .into_response()
}

fn fixture_tool_call(scenario: &Value) -> Option<(&str, &str)> {
    scenario["events"].as_array()?.iter().find_map(|event| {
        if !matches!(
            event["type"].as_str(),
            Some("assistant.tool_call") | Some("tool.intent")
        ) {
            return None;
        }
        Some((event["call_id"].as_str()?, event["name"].as_str()?))
    })
}

#[cfg(test)]
mod tests {
    use super::FixtureModelServer;
    use serde_json::json;

    #[tokio::test]
    async fn fixture_server_replays_stream_and_nonstream_tool_pair_contracts() {
        let stream_server = FixtureModelServer::spawn("BASE-STREAM-TOOL-PAIR")
            .await
            .expect("start stream fixture server");
        let stream = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", stream_server.base_url))
            .json(&json!({"model": "fixture-model", "stream": true, "messages": []}))
            .send()
            .await
            .expect("stream response");
        assert!(stream.status().is_success());
        let stream_body = stream.text().await.expect("read stream response");
        assert!(stream_body.contains("fixture-call-001"));
        assert!(stream_body.contains("read_file"));
        assert!(stream_body.contains("data: [DONE]"));
        assert_eq!(stream_server.captured_requests().len(), 1);

        let nonstream_server = FixtureModelServer::spawn("BASE-NONSTREAM-TOOL-PAIR")
            .await
            .expect("start nonstream fixture server");
        let nonstream = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", nonstream_server.base_url))
            .json(&json!({"model": "fixture-model", "stream": false, "messages": []}))
            .send()
            .await
            .expect("nonstream response");
        assert!(nonstream.status().is_success());
        let response: serde_json::Value = nonstream.json().await.expect("parse JSON response");
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["id"],
            "fixture-call-002"
        );
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "list_directory"
        );
        assert_eq!(nonstream_server.captured_requests().len(), 1);
    }
}
