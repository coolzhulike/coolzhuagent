use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::session::{MessageRole, Session};

/// 跨 UI、CLI 和 provider 适配层共享的线程标识。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(String);

impl ThreadId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new("thread-default")
    }
}

impl From<&str> for ThreadId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ThreadId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// 回合标识。一个线程可以包含多个单调递增的 turn。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TurnId(String);

impl TurnId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TurnId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for TurnId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// 流式 item 标识。没有 item 级别语义的事件可以把它置为 None。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ItemId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ItemId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// 统一的 Agent 事件类别。payload 保留 provider/UI 特有字段，避免公共契约膨胀。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventKind {
    TurnStarted,
    ContextSnapshot,
    ReasoningDelta,
    MessageDone,
    ToolCall,
    TurnCompleted,
}

/// 可被 CLI、Web Console 和日志管道共同消费的最小事件信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEvent {
    pub schema_version: u16,
    pub sequence: u64,
    pub thread_id: ThreadId,
    pub turn_id: TurnId,
    pub item_id: Option<ItemId>,
    pub kind: AgentEventKind,
    pub payload: Value,
}

impl AgentEvent {
    #[must_use]
    pub fn new(
        sequence: u64,
        thread_id: ThreadId,
        turn_id: TurnId,
        item_id: Option<ItemId>,
        kind: AgentEventKind,
        payload: Value,
    ) -> Self {
        Self {
            schema_version: 1,
            sequence,
            thread_id,
            turn_id,
            item_id,
            kind,
            payload,
        }
    }
}

/// 本轮实际送入模型的上下文快照摘要。
///
/// 快照只保存计数和不可逆 revision，不把完整 prompt 再复制到事件流，
/// 从而既能检测上下文是否变化，又不会因为日志重复保存用户内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub snapshot_id: String,
    pub revision: String,
    pub session_version: u32,
    pub message_count: usize,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    pub tool_message_count: usize,
}

impl ContextSnapshot {
    #[must_use]
    pub fn from_session(thread_id: &ThreadId, session: &Session) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(thread_id.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(session.to_json().render().as_bytes());
        let digest = hasher.finalize();
        let revision = hex_digest(&digest);
        let snapshot_id = format!("ctx-{}", &revision[..16]);
        let mut user_message_count = 0;
        let mut assistant_message_count = 0;
        let mut tool_message_count = 0;
        for message in &session.messages {
            match message.role {
                MessageRole::User => user_message_count += 1,
                MessageRole::Assistant => assistant_message_count += 1,
                MessageRole::Tool => tool_message_count += 1,
                MessageRole::System => {}
            }
        }
        Self {
            snapshot_id,
            revision,
            session_version: session.version,
            message_count: session.messages.len(),
            user_message_count,
            assistant_message_count,
            tool_message_count,
        }
    }

    #[must_use]
    pub fn event_payload(&self) -> Value {
        json!({
            "snapshot_id": self.snapshot_id,
            "revision": self.revision,
            "session_version": self.session_version,
            "message_count": self.message_count,
            "user_message_count": self.user_message_count,
            "assistant_message_count": self.assistant_message_count,
            "tool_message_count": self.tool_message_count,
        })
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{AgentEvent, AgentEventKind, ContextSnapshot, ItemId, ThreadId, TurnId};
    use crate::session::{ConversationMessage, Session};
    use serde_json::json;

    #[test]
    fn snapshot_revision_changes_when_session_changes() {
        let thread = ThreadId::new("thread-test");
        let mut session = Session::new();
        let first = ContextSnapshot::from_session(&thread, &session);
        session
            .messages
            .push(ConversationMessage::user_text("hello"));
        let second = ContextSnapshot::from_session(&thread, &session);

        assert_ne!(first.revision, second.revision);
        assert_eq!(second.message_count, 1);
        assert_eq!(second.user_message_count, 1);
    }

    #[test]
    fn event_envelope_round_trips_with_typed_ids() {
        let event = AgentEvent::new(
            3,
            ThreadId::new("thread-1"),
            TurnId::new("turn-2"),
            Some(ItemId::new("item-3")),
            AgentEventKind::ReasoningDelta,
            json!({"text": "先检查工具"}),
        );
        let encoded = serde_json::to_string(&event).expect("event serializes");
        let decoded: AgentEvent = serde_json::from_str(&encoded).expect("event deserializes");

        assert_eq!(decoded, event);
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.thread_id.as_str(), "thread-1");
    }
}
