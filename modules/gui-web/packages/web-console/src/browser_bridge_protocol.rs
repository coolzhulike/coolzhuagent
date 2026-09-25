use serde::{Deserialize, Serialize};

pub const NATIVE_MESSAGE_MAX_BYTES: usize = 1024 * 1024;
pub const BROWSER_BRIDGE_HOST_NAME: &str = "com.coolzhu.agent.browser_bridge";
pub const BROWSER_EXTENSION_ID: &str = "akpgmkdkaofanikngahmfbhddpppicfi";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BridgeRequest {
    Hello {
        request_id: String,
        nonce: String,
    },
    Snapshot {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tab_id: Option<String>,
    },
    Act {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tab_id: Option<String>,
        expected_document_id: String,
        action: BrowserAction,
    },
    Tab {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_token: Option<String>,
        action: BrowserTabAction,
    },
    Ping {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_token: Option<String>,
    },
}

impl BridgeRequest {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Hello { request_id, .. }
            | Self::Snapshot { request_id, .. }
            | Self::Act { request_id, .. }
            | Self::Tab { request_id, .. }
            | Self::Ping { request_id, .. } => request_id,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.request_id().is_empty() || self.request_id().len() > 128 {
            return Err("invalid_request_id");
        }
        if let Some(reply_token) = self.reply_token() {
            if reply_token.is_empty() || reply_token.len() > 256 {
                return Err("invalid_reply_token");
            }
        }
        match self {
            Self::Hello { nonce, .. } if nonce.len() < 16 || nonce.len() > 256 => {
                Err("invalid_nonce")
            }
            Self::Act {
                expected_document_id,
                action,
                ..
            } if expected_document_id.is_empty()
                || expected_document_id.len() > 256
                || action.validate().is_err() =>
            {
                Err("invalid_action")
            }
            Self::Snapshot {
                tab_id: Some(tab_id),
                ..
            }
            | Self::Act {
                tab_id: Some(tab_id),
                ..
            } if !valid_tab_id(tab_id) => Err("invalid_tab_id"),
            Self::Tab { action, .. } if action.validate().is_err() => Err("invalid_tab_action"),
            _ => Ok(()),
        }
    }

    pub fn reply_token(&self) -> Option<&str> {
        match self {
            Self::Snapshot { reply_token, .. }
            | Self::Act { reply_token, .. }
            | Self::Tab { reply_token, .. }
            | Self::Ping { reply_token, .. } => reply_token.as_deref(),
            Self::Hello { .. } => None,
        }
    }

    pub fn with_reply_token(mut self, token: String) -> Self {
        match &mut self {
            Self::Snapshot { reply_token, .. }
            | Self::Act { reply_token, .. }
            | Self::Tab { reply_token, .. }
            | Self::Ping { reply_token, .. } => {
                *reply_token = Some(token);
            }
            Self::Hello { .. } => {}
        }
        self
    }
}

fn valid_tab_id(tab_id: &str) -> bool {
    !tab_id.is_empty()
        && tab_id.len() <= 32
        && tab_id.chars().all(|character| character.is_ascii_digit())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserTabAction {
    Open {
        url: String,
        #[serde(default)]
        activate: bool,
        owner_token: String,
    },
    Activate {
        tab_id: String,
    },
    CloseOwned {
        tab_id: String,
        owner_token: String,
    },
}

impl BrowserTabAction {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Open {
                url, owner_token, ..
            } if !valid_http_url(url) || !valid_owner_token(owner_token) => Err("invalid_open_tab"),
            Self::Activate { tab_id } if !valid_tab_id(tab_id) => Err("invalid_tab_id"),
            Self::CloseOwned {
                tab_id,
                owner_token,
            } if !valid_tab_id(tab_id) || !valid_owner_token(owner_token) => {
                Err("invalid_owned_tab")
            }
            _ => Ok(()),
        }
    }
}

fn valid_http_url(url: &str) -> bool {
    url.len() <= 2_048 && (url.starts_with("https://") || url.starts_with("http://"))
}

fn valid_owner_token(owner_token: &str) -> bool {
    (16..=128).contains(&owner_token.len())
        && owner_token
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserAction {
    Navigate {
        target: String,
        url: String,
    },
    Click {
        target: String,
    },
    TextInput {
        target: String,
        text: String,
    },
    Select {
        target: String,
        value: String,
    },
    Check {
        target: String,
        checked: bool,
    },
    Submit {
        target: String,
    },
    Drag {
        target: String,
        drop_target: String,
    },
    SliderDrag {
        target: String,
        value: u8,
    },
    KeyCombination {
        target: String,
        keys: Vec<String>,
    },
    Scroll {
        target: String,
        direction: ScrollDirection,
        #[serde(default = "default_scroll_amount")]
        amount: u8,
    },
    HistoryBack {
        target: String,
    },
    HistoryForward {
        target: String,
    },
}

fn default_scroll_amount() -> u8 {
    1
}

impl BrowserAction {
    pub fn target(&self) -> &str {
        match self {
            Self::Navigate { target, .. }
            | Self::Click { target }
            | Self::TextInput { target, .. }
            | Self::Select { target, .. }
            | Self::Check { target, .. }
            | Self::Submit { target }
            | Self::Drag { target, .. }
            | Self::SliderDrag { target, .. }
            | Self::KeyCombination { target, .. }
            | Self::Scroll { target, .. }
            | Self::HistoryBack { target }
            | Self::HistoryForward { target } => target,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.target().starts_with("dom-") || self.target().len() > 128 {
            return Err("invalid_target");
        }
        match self {
            Self::Navigate { url, .. } if !valid_http_url(url) => Err("invalid_url"),
            Self::TextInput { text, .. } if text.is_empty() || text.len() > 4_000 => {
                Err("invalid_text")
            }
            Self::Select { value, .. } if value.len() > 1_024 => Err("invalid_value"),
            Self::Drag {
                target,
                drop_target,
            } if !drop_target.starts_with("dom-")
                || drop_target.len() > 128
                || drop_target == target =>
            {
                Err("invalid_drop_target")
            }
            Self::SliderDrag { value, .. } if *value > 100 => Err("invalid_slider_value"),
            Self::KeyCombination { keys, .. } if !valid_key_combination(keys) => {
                Err("invalid_key_combination")
            }
            Self::Scroll { amount, .. } if !(1..=5).contains(amount) => {
                Err("invalid_scroll_amount")
            }
            _ => Ok(()),
        }
    }
}

fn valid_key_combination(keys: &[String]) -> bool {
    const ALLOWED_KEYS: &[&str] = &[
        "ctrl", "shift", "alt", "enter", "escape", "tab", "home", "end", "a", "c", "v", "x", "z",
        "y",
    ];
    !keys.is_empty()
        && keys.len() <= 4
        && keys
            .iter()
            .all(|key| ALLOWED_KEYS.contains(&key.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeResponse {
    pub request_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BridgeError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub dom_revision: u64,
    #[serde(default)]
    pub nodes: Vec<DomNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

impl BridgeResponse {
    pub fn failure(
        request_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            ok: false,
            error: Some(BridgeError {
                code: code.into(),
                message: message.into(),
            }),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomNode {
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_bridge_protocol_round_trips_closed_actions() {
        let request = BridgeRequest::Act {
            request_id: "r2".into(),
            reply_token: Some("reply-token-2".into()),
            tab_id: Some("7".into()),
            expected_document_id: "doc-1".into(),
            action: BrowserAction::TextInput {
                target: "dom-7".into(),
                text: "OpenAI".into(),
            },
        };
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: BridgeRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, request);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn browser_bridge_protocol_allows_complex_dom_actions() {
        for action in [
            BrowserAction::Drag {
                target: "dom-7".into(),
                drop_target: "dom-8".into(),
            },
            BrowserAction::SliderDrag {
                target: "dom-9".into(),
                value: 75,
            },
            BrowserAction::KeyCombination {
                target: "dom-10".into(),
                keys: vec!["ctrl".into(), "a".into()],
            },
        ] {
            let request = BridgeRequest::Act {
                request_id: format!("r-{}", action.target()),
                reply_token: Some("reply-token-complex".into()),
                tab_id: Some("7".into()),
                expected_document_id: "doc-1".into(),
                action,
            };
            let encoded = serde_json::to_string(&request).unwrap();
            let decoded: BridgeRequest = serde_json::from_str(&encoded).unwrap();
            assert!(decoded.validate().is_ok());
        }
    }

    #[test]
    fn snapshot_request_carries_one_time_reply_token() {
        let request = BridgeRequest::Snapshot {
            request_id: "r1".into(),
            reply_token: Some("reply-token-1".into()),
            tab_id: None,
        };
        let encoded = serde_json::to_string(&request).unwrap();
        assert!(encoded.contains("reply_token"));
        let decoded: BridgeRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, request);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn snapshot_can_pin_a_specific_tab_and_tab_lifecycle_actions_are_closed() {
        let snapshot = BridgeRequest::Snapshot {
            request_id: "snapshot-pinned".into(),
            reply_token: None,
            tab_id: Some("42".into()),
        };
        assert!(snapshot.validate().is_ok());
        assert!(serde_json::to_string(&snapshot)
            .unwrap()
            .contains("\"tab_id\":\"42\""));

        for action in [
            BrowserTabAction::Open {
                url: "https://example.test/new".into(),
                activate: true,
                owner_token: "task-owner-123456".into(),
            },
            BrowserTabAction::Activate {
                tab_id: "42".into(),
            },
            BrowserTabAction::CloseOwned {
                tab_id: "42".into(),
                owner_token: "task-owner-123456".into(),
            },
        ] {
            let request = BridgeRequest::Tab {
                request_id: "tab-action".into(),
                reply_token: None,
                action,
            };
            assert!(request.validate().is_ok());
            let encoded = serde_json::to_string(&request).unwrap();
            let decoded: BridgeRequest = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, request);
        }
    }

    #[test]
    fn close_tab_requires_a_task_owner_token() {
        let request = BridgeRequest::Tab {
            request_id: "tab-close".into(),
            reply_token: None,
            action: BrowserTabAction::CloseOwned {
                tab_id: "42".into(),
                owner_token: String::new(),
            },
        };
        assert_eq!(request.validate(), Err("invalid_tab_action"));
    }

    #[test]
    fn browser_bridge_protocol_rejects_unknown_and_javascript_payloads() {
        let unknown = r#"{"type":"act","request_id":"r","expected_document_id":"d","action":{"action":"evaluate_javascript","target":"dom-1","script":"alert(1)"}}"#;
        assert!(serde_json::from_str::<BridgeRequest>(unknown).is_err());
        let javascript = r#"{"type":"act","request_id":"r","expected_document_id":"d","action":{"action":"navigate","target":"dom-1","url":"javascript:alert(1)"}}"#;
        let request: BridgeRequest = serde_json::from_str(javascript).unwrap();
        assert_eq!(request.validate(), Err("invalid_action"));
    }
}
