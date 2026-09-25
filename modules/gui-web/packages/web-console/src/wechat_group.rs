use serde::{Deserialize, Serialize};

use crate::clawbot_channel::ClawbotConversationBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WechatGroupLifecycleState {
    AwaitingConfirmation,
    Active,
    SyncLimited,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WechatGroupEventKind {
    BotAdded,
    BotRemoved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatObservedContact {
    pub account_id: String,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub first_seen_at_ms: u64,
    pub last_seen_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatOperationAdministrator {
    pub account_id: String,
    pub peer_id: String,
    pub peer_name: Option<String>,
    #[serde(default)]
    pub bot_mention_aliases: Vec<String>,
    pub claimed_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatGroupRecord {
    pub account_id: String,
    pub group_id: String,
    pub group_name: Option<String>,
    pub first_seen_at_ms: u64,
    pub last_seen_at_ms: u64,
    pub lifecycle_state: WechatGroupLifecycleState,
    pub sync_evidence: String,
    #[serde(default)]
    pub binding: Option<ClawbotConversationBinding>,
    #[serde(default)]
    pub recognized_member_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatGroupDetachAudit {
    pub account_id: String,
    pub group_id: String,
    pub group_name: Option<String>,
    pub reason: String,
    pub actor: String,
    pub detached_at_ms: u64,
}

pub fn resolve_bot_mention(structured: bool, text: &str, aliases: &[String]) -> bool {
    if structured {
        return true;
    }
    let normalized = text.trim_start().to_lowercase();
    aliases.iter().any(|alias| {
        let alias = alias.trim().to_lowercase();
        if alias.is_empty() {
            return false;
        }
        let prefix = format!("@{alias}");
        let Some(rest) = normalized.strip_prefix(&prefix) else {
            return false;
        };
        rest.is_empty()
            || rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, ':' | '：' | ',' | '，'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_alias_requires_explicit_prefix() {
        let aliases = vec!["ClawBot".to_string(), "库珠".to_string()];

        assert!(resolve_bot_mention(false, " @ClawBot 继续", &aliases));
        assert!(resolve_bot_mention(false, "@库珠：查看状态", &aliases));
        assert!(resolve_bot_mention(true, "没有文本前缀", &aliases));
        assert!(!resolve_bot_mention(false, "请问 ClawBot 在吗", &aliases));
        assert!(!resolve_bot_mention(false, "@其他机器人 继续", &aliases));
    }

    #[test]
    fn group_domain_types_use_stable_snake_case_protocol_values() {
        let event: WechatGroupEventKind = serde_json::from_str("\"bot_removed\"").unwrap();
        assert_eq!(event, WechatGroupEventKind::BotRemoved);

        let record = WechatGroupRecord {
            account_id: "wx-main".to_string(),
            group_id: "group-1".to_string(),
            group_name: Some("测试群".to_string()),
            first_seen_at_ms: 100,
            last_seen_at_ms: 200,
            lifecycle_state: WechatGroupLifecycleState::Active,
            sync_evidence: "inbound_message".to_string(),
            binding: None,
            recognized_member_count: 1,
        };
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["lifecycle_state"], "active");
        assert_eq!(json["recognized_member_count"], 1);
    }
}
