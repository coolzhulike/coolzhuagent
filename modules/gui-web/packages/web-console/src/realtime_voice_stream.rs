#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct PcmFrame {
    pub session_id: String,
    pub sequence: u64,
    pub captured_at_ms: u64,
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub samples: Vec<i16>,
}

pub(crate) fn validate_pcm_frame(frame: &PcmFrame) -> Result<(), StreamError> {
    if frame.sample_rate_hz != 16_000 || frame.channels != 1 || frame.samples.is_empty() {
        return Err(StreamError {
            code: "unsupported_pcm_format".into(),
            message: "realtime STT requires non-empty 16 kHz mono PCM16 frames".into(),
            retryable: false,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderTranscript {
    pub text: String,
    pub is_final: bool,
}

pub(crate) fn parse_aliyun_event(text: &str) -> Option<ProviderTranscript> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if value.pointer("/header/event")?.as_str()? != "result-generated" {
        return None;
    }
    let sentence = value.pointer("/payload/output/sentence")?;
    let transcript = sentence.get("text")?.as_str()?.trim();
    if transcript.is_empty() {
        return None;
    }
    Some(ProviderTranscript {
        text: transcript.to_string(),
        is_final: sentence
            .get("sentence_end")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub(crate) struct StreamingSttEvidence {
    pub pcm_frames_sent: u64,
    pub pcm_bytes_sent: u64,
    pub partial_events: u64,
    pub final_events: u64,
    pub reconnects: u64,
    pub dropped_frames: u64,
    pub max_frame_gap_ms: u64,
}

fn evidence_store() -> &'static Mutex<HashMap<String, StreamingSttEvidence>> {
    static STORE: OnceLock<Mutex<HashMap<String, StreamingSttEvidence>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn evidence_for_session(session_id: &str) -> StreamingSttEvidence {
    evidence_store()
        .lock()
        .ok()
        .and_then(|store| store.get(session_id).cloned())
        .unwrap_or_default()
}

fn update_evidence(session_id: &str, update: impl FnOnce(&mut StreamingSttEvidence)) {
    if let Ok(mut store) = evidence_store().lock() {
        update(store.entry(session_id.to_string()).or_default());
    }
}

impl StreamingSttEvidence {
    pub(crate) fn provider_native_ready(&self) -> bool {
        self.pcm_frames_sent > 0 && (self.partial_events > 0 || self.final_events > 0)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct StreamError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Default)]
pub(crate) struct FrameSequenceGuard {
    last: Option<u64>,
}

impl FrameSequenceGuard {
    pub(crate) fn accept(&mut self, sequence: u64) -> Result<(), StreamError> {
        if self.last.is_some_and(|last| sequence <= last) {
            return Err(StreamError {
                code: "duplicate_pcm_frame".into(),
                message: "PCM sequence did not advance".into(),
                retryable: false,
            });
        }
        self.last = Some(sequence);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientStreamMessage {
    Start {
        session_id: String,
        sample_rate_hz: u32,
        channels: u8,
        frame_ms: u32,
    },
    PcmFrame {
        frame: PcmFrame,
    },
    Stop {
        session_id: String,
    },
}

fn active_session_matches(session_id: &str) -> bool {
    let audio_matches = crate::audio_realtime_state()
        .lock()
        .ok()
        .is_some_and(|state| state.running && state.session_id.as_deref() == Some(session_id));
    let realtime_matches = crate::realtime_session_state()
        .lock()
        .ok()
        .is_some_and(|state| state.running && state.session_id.as_deref() == Some(session_id));
    audio_matches || realtime_matches
}

fn provider_error(code: &str, message: impl Into<String>, retryable: bool) -> StreamError {
    StreamError {
        code: code.to_string(),
        message: message.into(),
        retryable,
    }
}

fn task_id() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let fallback = crate::uuid_timestamp();
        bytes[..8].copy_from_slice(&fallback.to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

fn pcm_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

async fn send_browser_json(
    sink: &mut futures_util::stream::SplitSink<WebSocket, BrowserMessage>,
    value: serde_json::Value,
) -> Result<(), axum::Error> {
    sink.send(BrowserMessage::Text(value.to_string().into()))
        .await
}

async fn record_transcript(session_id: &str, transcript: &ProviderTranscript) {
    update_evidence(session_id, |evidence| {
        if transcript.is_final {
            evidence.final_events = evidence.final_events.saturating_add(1);
        } else {
            evidence.partial_events = evidence.partial_events.saturating_add(1);
        }
    });
    let _ = crate::api_audio_realtime_partial(axum::Json(crate::AudioRealtimePartialRequest {
        session_id: Some(session_id.to_string()),
        text: Some(transcript.text.clone()),
        confidence: None,
        is_final: Some(transcript.is_final),
        provider: Some("provider_native_streaming_asr:aliyun_paraformer_realtime_v2".to_string()),
        language: Some("zh-CN".to_string()),
        speech_ms: None,
        tts_playing: Some(false),
        echo_correlation: None,
        vad_active: Some(true),
    }))
    .await;
}

async fn connect_aliyun(
    websocket_url: &str,
    api_key: &str,
    task_id: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    StreamError,
> {
    let mut request = websocket_url
        .into_client_request()
        .map_err(|error| provider_error("invalid_stt_websocket_url", error.to_string(), false))?;
    let authorization = HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|_| {
        provider_error(
            "invalid_stt_api_key",
            "STT API key contains invalid header bytes",
            false,
        )
    })?;
    request.headers_mut().insert(AUTHORIZATION, authorization);
    let (mut provider, _) = tokio::time::timeout(Duration::from_secs(20), connect_async(request))
        .await
        .map_err(|_| {
            provider_error(
                "stt_connect_timeout",
                "streaming STT connection timed out",
                true,
            )
        })?
        .map_err(|error| provider_error("stt_connect_failed", error.to_string(), true))?;
    provider
        .send(ProviderMessage::Text(
            json!({
                "header": {
                    "action": "run-task",
                    "task_id": task_id,
                    "streaming": "duplex"
                },
                "payload": {
                    "task_group": "audio",
                    "task": "asr",
                    "function": "recognition",
                    "model": "paraformer-realtime-v2",
                    "parameters": {
                        "format": "pcm",
                        "sample_rate": 16000,
                        "language_hints": ["zh", "en"]
                    },
                    "input": {}
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .map_err(|error| provider_error("stt_start_failed", error.to_string(), true))?;

    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), provider.next())
            .await
            .map_err(|_| {
                provider_error(
                    "stt_start_timeout",
                    "streaming STT did not acknowledge task",
                    true,
                )
            })?
            .ok_or_else(|| {
                provider_error("stt_closed", "streaming STT closed before task start", true)
            })?
            .map_err(|error| provider_error("stt_start_failed", error.to_string(), true))?;
        if let ProviderMessage::Text(text) = message {
            let value: serde_json::Value = serde_json::from_str(text.as_str()).unwrap_or_default();
            match value
                .pointer("/header/event")
                .and_then(serde_json::Value::as_str)
            {
                Some("task-started") => return Ok(provider),
                Some("task-failed") => {
                    let message = value
                        .pointer("/header/error_message")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("streaming STT task failed");
                    return Err(provider_error("stt_task_failed", message, false));
                }
                _ => {}
            }
        }
    }
}

pub(crate) async fn handle_browser_stream(socket: WebSocket) {
    let (mut browser_sink, mut browser_stream) = socket.split();
    let first = match browser_stream.next().await {
        Some(Ok(BrowserMessage::Text(text))) => {
            serde_json::from_str::<ClientStreamMessage>(text.as_str())
        }
        _ => {
            let _ = send_browser_json(
                &mut browser_sink,
                json!({"type":"error","code":"stream_start_required","message":"first message must start the stream","retryable":false}),
            )
            .await;
            return;
        }
    };
    let (session_id, sample_rate_hz, channels, frame_ms) = match first {
        Ok(ClientStreamMessage::Start {
            session_id,
            sample_rate_hz,
            channels,
            frame_ms,
        }) => (session_id, sample_rate_hz, channels, frame_ms),
        _ => {
            let _ = send_browser_json(
                &mut browser_sink,
                json!({"type":"error","code":"invalid_stream_start","message":"invalid stream start message","retryable":false}),
            )
            .await;
            return;
        }
    };
    if !active_session_matches(&session_id) {
        let _ = send_browser_json(
            &mut browser_sink,
            json!({"type":"error","code":"stale_realtime_session","message":"stream session is not active","retryable":false}),
        )
        .await;
        return;
    }
    if sample_rate_hz != 16_000 || channels != 1 || !(20..=40).contains(&frame_ms) {
        let _ = send_browser_json(
            &mut browser_sink,
            json!({"type":"error","code":"unsupported_pcm_format","message":"expected 16 kHz mono PCM with 20-40 ms frames","retryable":false}),
        )
        .await;
        return;
    }

    let config = crate::read_config(|workspace| workspace.audio.realtime.clone());
    let provider_name = config.stt_provider.trim().to_ascii_lowercase();
    if !provider_name.is_empty()
        && provider_name != "aliyun"
        && provider_name != "aliyun_paraformer_realtime_v2"
    {
        let _ = send_browser_json(
            &mut browser_sink,
            json!({"type":"error","code":"unsupported_stt_provider","message":"only aliyun_paraformer_realtime_v2 is configured","retryable":false}),
        )
        .await;
        return;
    }
    let api_key = config
        .stt_api_key
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("DASHSCOPE_API_KEY").ok())
        .unwrap_or_default();
    if api_key.is_empty() {
        let _ = send_browser_json(
            &mut browser_sink,
            json!({"type":"error","code":"stt_credentials_missing","message":"DASHSCOPE_API_KEY or audio.realtime.stt_api_key is required","retryable":false}),
        )
        .await;
        return;
    }
    let websocket_url = config
        .stt_websocket_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "wss://dashscope.aliyuncs.com/api-ws/v1/inference/".to_string());
    let task_id = task_id();
    let provider = match connect_aliyun(&websocket_url, &api_key, &task_id).await {
        Ok(provider) => provider,
        Err(error) => {
            let _ = send_browser_json(
                &mut browser_sink,
                json!({"type":"error","code":error.code,"message":error.message,"retryable":error.retryable}),
            )
            .await;
            return;
        }
    };
    let (mut provider_sink, mut provider_stream) = provider.split();
    let _ = send_browser_json(
        &mut browser_sink,
        json!({"type":"ready","provider":"aliyun_paraformer_realtime_v2","session_id":session_id}),
    )
    .await;
    let mut sequence = FrameSequenceGuard::default();

    loop {
        tokio::select! {
            browser_message = browser_stream.next() => {
                let Some(Ok(BrowserMessage::Text(text))) = browser_message else { break; };
                match serde_json::from_str::<ClientStreamMessage>(text.as_str()) {
                    Ok(ClientStreamMessage::PcmFrame { frame }) => {
                        if frame.session_id != session_id {
                            let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":"stale_realtime_session","message":"PCM frame session mismatch","retryable":false})).await;
                            break;
                        }
                        if let Err(error) = validate_pcm_frame(&frame).and_then(|_| sequence.accept(frame.sequence)) {
                            let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":error.code,"message":error.message,"retryable":error.retryable})).await;
                            break;
                        }
                        let bytes = pcm_bytes(&frame.samples);
                        update_evidence(&session_id, |evidence| {
                            evidence.pcm_frames_sent = evidence.pcm_frames_sent.saturating_add(1);
                            evidence.pcm_bytes_sent = evidence.pcm_bytes_sent.saturating_add(bytes.len() as u64);
                        });
                        if let Err(error) = provider_sink.send(ProviderMessage::Binary(bytes.into())).await {
                            let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":"stt_send_failed","message":error.to_string(),"retryable":true})).await;
                            break;
                        }
                    }
                    Ok(ClientStreamMessage::Stop { session_id: stop_session }) if stop_session == session_id => {
                        let _ = provider_sink.send(ProviderMessage::Text(json!({
                            "header": {"action":"finish-task","task_id":task_id,"streaming":"duplex"},
                            "payload": {"input": {}}
                        }).to_string().into())).await;
                        break;
                    }
                    _ => {
                        let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":"invalid_stream_message","message":"unsupported stream message","retryable":false})).await;
                        break;
                    }
                }
            }
            provider_message = provider_stream.next() => {
                match provider_message {
                    Some(Ok(ProviderMessage::Text(text))) => {
                        if let Some(transcript) = parse_aliyun_event(text.as_str()) {
                            record_transcript(&session_id, &transcript).await;
                            let event_type = if transcript.is_final { "final" } else { "partial" };
                            if send_browser_json(&mut browser_sink, json!({
                                "type":event_type,
                                "text":transcript.text,
                                "provider":"provider_native_streaming_asr:aliyun_paraformer_realtime_v2",
                                "language":"zh-CN"
                            })).await.is_err() {
                                break;
                            }
                        } else {
                            let value: serde_json::Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                            if value.pointer("/header/event").and_then(serde_json::Value::as_str) == Some("task-failed") {
                                let message = value.pointer("/header/error_message").and_then(serde_json::Value::as_str).unwrap_or("streaming STT task failed");
                                let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":"stt_task_failed","message":message,"retryable":false})).await;
                                break;
                            }
                        }
                    }
                    Some(Ok(ProviderMessage::Close(_))) | None => {
                        let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":"stt_provider_closed","message":"streaming STT provider closed","retryable":true})).await;
                        break;
                    }
                    Some(Err(error)) => {
                        let _ = send_browser_json(&mut browser_sink, json!({"type":"error","code":"stt_provider_error","message":error.to_string(),"retryable":true})).await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    let _ = provider_sink.close().await;
    let _ = browser_sink.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_monotonic_pcm_sequence() {
        let mut guard = FrameSequenceGuard::default();
        assert!(guard.accept(1).is_ok());
        let error = guard.accept(1).unwrap_err();
        assert_eq!(error.code, "duplicate_pcm_frame");
    }

    #[test]
    fn provider_native_gate_requires_audio_and_transcript_evidence() {
        let evidence = StreamingSttEvidence {
            pcm_frames_sent: 4,
            partial_events: 0,
            final_events: 0,
            ..StreamingSttEvidence::default()
        };
        assert!(!evidence.provider_native_ready());
        let ready = StreamingSttEvidence {
            partial_events: 1,
            ..evidence
        };
        assert!(ready.provider_native_ready());
    }

    #[test]
    fn parses_aliyun_partial_and_final_events() {
        let partial = parse_aliyun_event(
            r#"{"header":{"event":"result-generated"},"payload":{"output":{"sentence":{"text":"你好","sentence_end":false}}}}"#,
        )
        .expect("partial event");
        assert_eq!(partial.text, "你好");
        assert!(!partial.is_final);

        let final_event = parse_aliyun_event(
            r#"{"header":{"event":"result-generated"},"payload":{"output":{"sentence":{"text":"你好 Coolzhu","sentence_end":true}}}}"#,
        )
        .expect("final event");
        assert_eq!(final_event.text, "你好 Coolzhu");
        assert!(final_event.is_final);
    }

    #[test]
    fn rejects_wrong_pcm_shape_before_provider_send() {
        let frame = PcmFrame {
            session_id: "session-1".into(),
            sequence: 1,
            captured_at_ms: 1,
            sample_rate_hz: 48_000,
            channels: 2,
            samples: vec![0; 320],
        };
        let error = validate_pcm_frame(&frame).unwrap_err();
        assert_eq!(error.code, "unsupported_pcm_format");
    }
}
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::extract::ws::{Message as BrowserMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{header::AUTHORIZATION, HeaderValue};
use tokio_tungstenite::tungstenite::Message as ProviderMessage;
