use crate::error::ApiError;
use crate::types::{MessageStartEvent, StreamEvent};

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    usage_evidence: crate::UsageEvidence,
    /// **连接级兼容位**（COMPAT-ID §2.3）：允许流式 `message_start.message.id` 缺失。
    ///
    /// 流式必须与**非流式**分开验（裁决 §2.5）：起始事件里的 Message 对象带的是同一个顶层
    /// message ID，因此同一个归一化器在这里同样适用——严格模式必须拒绝，兼容模式可继续聚合、
    /// **不得**提前判结束。
    allow_missing_top_level_message_id: bool,
}

impl SseParser {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 按连接的兼容能力构造（默认严格）。
    #[must_use]
    pub fn with_allow_missing_top_level_message_id(allow: bool) -> Self {
        Self {
            buffer: Vec::new(),
            usage_evidence: crate::UsageEvidence::default(),
            allow_missing_top_level_message_id: allow,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<StreamEvent>, ApiError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(frame) = self.next_frame() {
            self.usage_evidence.merge(crate::UsageEvidence::anthropic_frame(&frame));
            if let Some(event) = parse_frame_with_compat(&frame, self.allow_missing_top_level_message_id)?
            {
                events.push(event);
            }
        }

        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<StreamEvent>, ApiError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }

        let trailing = std::mem::take(&mut self.buffer);
        self.usage_evidence.merge(crate::UsageEvidence::anthropic_frame(&String::from_utf8_lossy(&trailing)));
        match parse_frame_with_compat(
            &String::from_utf8_lossy(&trailing),
            self.allow_missing_top_level_message_id,
        )? {
            Some(event) => Ok(vec![event]),
            None => Ok(Vec::new()),
        }
    }

    pub fn usage_evidence(&self) -> crate::UsageEvidence { self.usage_evidence }

    fn next_frame(&mut self) -> Option<String> {
        let separator = self
            .buffer
            .windows(2)
            .position(|window| window == b"\n\n")
            .map(|position| (position, 2))
            .or_else(|| {
                self.buffer
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|position| (position, 4))
            })?;

        let (position, separator_len) = separator;
        let frame = self
            .buffer
            .drain(..position + separator_len)
            .collect::<Vec<_>>();
        let frame_len = frame.len().saturating_sub(separator_len);
        Some(String::from_utf8_lossy(&frame[..frame_len]).into_owned())
    }
}

/// 严格模式的帧解析（等价于 `parse_frame_with_compat(frame, false)`）。
pub fn parse_frame(frame: &str) -> Result<Option<StreamEvent>, ApiError> {
    parse_frame_with_compat(frame, false)
}

/// 帧解析（带连接级兼容位）。
///
/// `message_start` 的 Message 对象走 **非流式同一个**归一化器：严格模式缺 ID ⇒ 拒绝；
/// 兼容模式 ⇒ `id = None`（未提供）并**继续聚合**（不得因为缺身份就提前判结束）。
pub fn parse_frame_with_compat(
    frame: &str,
    allow_missing_top_level_message_id: bool,
) -> Result<Option<StreamEvent>, ApiError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut data_lines = Vec::new();
    let mut event_name: Option<&str> = None;

    for line in trimmed.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(name) = line.strip_prefix("event:") {
            event_name = Some(name.trim());
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }

    if matches!(event_name, Some("ping")) {
        return Ok(None);
    }

    if data_lines.is_empty() {
        return Ok(None);
    }

    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(None);
    }

    // 先取原始 JSON：`message_start` 需要按顶层 ID 规则判定（serde 默认值会吃掉"缺失"）。
    let value: serde_json::Value = serde_json::from_str(&payload).map_err(ApiError::from)?;
    let event_type = value.get("type").and_then(serde_json::Value::as_str);
    if event_type == Some("message_start") {
        if let Some(message) = value.get("message").cloned() {
            let decoded = crate::providers::claw_provider::decode_message_response_body(
                message,
                allow_missing_top_level_message_id,
            )?;
            return Ok(Some(StreamEvent::MessageStart(MessageStartEvent {
                message: decoded,
            })));
        }
    }
    serde_json::from_value::<StreamEvent>(value)
        .map(Some)
        .map_err(ApiError::from)
}

#[cfg(test)]
mod tests {
    use super::{parse_frame, SseParser};
    use crate::types::{ContentBlockDelta, MessageDelta, OutputContentBlock, StreamEvent, Usage};

    #[test]
    fn parses_single_frame() {
        let frame = concat!(
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"Hi\"}}\n\n"
        );

        let event = parse_frame(frame).expect("frame should parse");
        assert_eq!(
            event,
            Some(StreamEvent::ContentBlockStart(
                crate::types::ContentBlockStartEvent {
                    index: 0,
                    content_block: OutputContentBlock::Text {
                        text: "Hi".to_string(),
                    },
                },
            ))
        );
    }

    #[test]
    fn parses_chunked_stream() {
        let mut parser = SseParser::new();
        let first = b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel";
        let second = b"lo\"}}\n\n";

        assert!(parser
            .push(first)
            .expect("first chunk should buffer")
            .is_empty());
        let events = parser.push(second).expect("second chunk should parse");

        assert_eq!(
            events,
            vec![StreamEvent::ContentBlockDelta(
                crate::types::ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta {
                        text: "Hello".to_string(),
                    },
                }
            )]
        );
    }

    #[test]
    fn ignores_ping_and_done() {
        let mut parser = SseParser::new();
        let payload = concat!(
            ": keepalive\n",
            "event: ping\n",
            "data: {\"type\":\"ping\"}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
            "data: [DONE]\n\n"
        );

        let events = parser
            .push(payload.as_bytes())
            .expect("parser should succeed");
        assert_eq!(
            events,
            vec![
                StreamEvent::MessageDelta(crate::types::MessageDeltaEvent {
                    delta: MessageDelta {
                        stop_reason: Some("tool_use".to_string()),
                        stop_sequence: None,
                    },
                    usage: Usage {
                        input_tokens: 1,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        output_tokens: 2,
                    },
                }),
                StreamEvent::MessageStop(crate::types::MessageStopEvent {}),
            ]
        );
    }

    #[test]
    fn ignores_data_less_event_frames() {
        let frame = "event: ping\n\n";
        let event = parse_frame(frame).expect("frame without data should be ignored");
        assert_eq!(event, None);
    }

    #[test]
    fn parses_split_json_across_data_lines() {
        let frame = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\n",
            "data: \"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n"
        );

        let event = parse_frame(frame).expect("frame should parse");
        assert_eq!(
            event,
            Some(StreamEvent::ContentBlockDelta(
                crate::types::ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta {
                        text: "Hello".to_string(),
                    },
                }
            ))
        );
    }

    #[test]
    fn parses_thinking_content_block_start() {
        let frame = concat!(
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":null}}\n\n"
        );

        let event = parse_frame(frame).expect("frame should parse");
        assert_eq!(
            event,
            Some(StreamEvent::ContentBlockStart(
                crate::types::ContentBlockStartEvent {
                    index: 0,
                    content_block: OutputContentBlock::Thinking {
                        thinking: String::new(),
                        signature: None,
                    },
                },
            ))
        );
    }

    #[test]
    fn parses_thinking_related_deltas() {
        let thinking = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"step 1\"}}\n\n"
        );
        let signature = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig_123\"}}\n\n"
        );

        let thinking_event = parse_frame(thinking).expect("thinking delta should parse");
        let signature_event = parse_frame(signature).expect("signature delta should parse");

        assert_eq!(
            thinking_event,
            Some(StreamEvent::ContentBlockDelta(
                crate::types::ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::ThinkingDelta {
                        thinking: "step 1".to_string(),
                    },
                }
            ))
        );
        assert_eq!(
            signature_event,
            Some(StreamEvent::ContentBlockDelta(
                crate::types::ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::SignatureDelta {
                        signature: "sig_123".to_string(),
                    },
                }
            ))
        );
    }
}

// COMPAT-ID：流式起始消息 + 并发不混淆（裁决 §2.5 的两行）。
#[cfg(test)]
mod compat_id_streaming_tests {
    use super::*;

    fn message_start_frame(id: Option<serde_json::Value>) -> String {
        let mut message = serde_json::json!({
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": "claude-sonnet-4-6",
            "usage": {"input_tokens": 1, "output_tokens": 0}
        });
        if let Some(id) = id {
            message["id"] = id;
        }
        format!(
            "event: message_start
data: {}

",
            serde_json::json!({"type": "message_start", "message": message})
        )
    }

    /// 流式起始：严格模式缺 ID ⇒ **拒绝**；兼容模式 ⇒ `id = None`（未提供）且**继续聚合**。
    #[test]
    fn message_start_without_id_is_rejected_strictly_and_accepted_in_compat_mode() {
        let frame = message_start_frame(None);
        assert!(
            parse_frame_with_compat(&frame, false).is_err(),
            "严格模式必须拒绝缺 ID 的 message_start"
        );
        let event = parse_frame_with_compat(&frame, true)
            .expect("兼容模式必须接受")
            .expect("必须产出一个事件");
        match event {
            StreamEvent::MessageStart(start) => {
                assert_eq!(start.message.id, None, "缺失必须显式为 None");
                assert_eq!(start.message.usage.input_tokens, 1, "其余字段不受影响");
            }
            other => panic!("必须是 message_start：{other:?}"),
        }
        // 有 ID 时两种模式都保留原值。
        let with_id = message_start_frame(Some(serde_json::json!("msg_1")));
        for allow in [false, true] {
            let event = parse_frame_with_compat(&with_id, allow)
                .expect("有效 ID 必须成功")
                .expect("事件");
            match event {
                StreamEvent::MessageStart(start) => {
                    assert_eq!(start.message.id.as_deref(), Some("msg_1"))
                }
                other => panic!("必须是 message_start：{other:?}"),
            }
        }
        // 空串仍拒绝（两种模式）。
        let empty = message_start_frame(Some(serde_json::json!("")));
        for allow in [false, true] {
            assert!(parse_frame_with_compat(&empty, allow).is_err(), "空串必须拒绝");
        }
    }

    /// 兼容模式下缺 ID **不得**让聚合提前结束：后续内容块与结束事件照常产出。
    #[test]
    fn compat_mode_keeps_aggregating_after_an_id_less_start() {
        let mut parser = SseParser::with_allow_missing_top_level_message_id(true);
        let mut chunk = message_start_frame(None);
        chunk.push_str("event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}

");
        chunk.push_str("event: message_stop
data: {\"type\":\"message_stop\"}

");
        let events = parser.push(chunk.as_bytes()).expect("兼容模式必须继续解析");
        assert!(
            matches!(events.first(), Some(StreamEvent::MessageStart(_))),
            "起始事件必须产出：{events:?}"
        );
        assert!(
            matches!(events.last(), Some(StreamEvent::MessageStop(_))),
            "结束事件必须照常产出（缺 ID 不得提前判结束）：{events:?}"
        );
    }

    /// **并发不混淆**：两份都缺 ID 的响应，内容与用量各自独立，不因"都没有身份"互相覆盖。
    ///
    /// 结构上的保证：解析/映射**不以 `id` 为键**（没有任何按 id 分桶的代码），
    /// 因此缺 ID 不会把两个响应并到同一个"空键桶"里。
    #[test]
    fn two_id_less_responses_do_not_share_identity_or_overwrite_each_other() {
        let first = crate::providers::claw_provider::decode_message_response_body(
            serde_json::json!({
                "type": "message", "role": "assistant",
                "content": [{"type": "text", "text": "first"}],
                "model": "m1", "usage": {"input_tokens": 10, "output_tokens": 1}
            }),
            true,
        )
        .expect("兼容解码");
        let second = crate::providers::claw_provider::decode_message_response_body(
            serde_json::json!({
                "type": "message", "role": "assistant",
                "content": [{"type": "text", "text": "second"}],
                "model": "m2", "usage": {"input_tokens": 20, "output_tokens": 2}
            }),
            true,
        )
        .expect("兼容解码");
        assert_eq!(first.id, None);
        assert_eq!(second.id, None);
        assert_eq!(first.usage.input_tokens, 10, "用量不得互相覆盖");
        assert_eq!(second.usage.input_tokens, 20);
        assert_eq!(first.model, "m1");
        assert_eq!(second.model, "m2");
        assert_ne!(first.content, second.content, "正文必须各自独立");
    }
}
