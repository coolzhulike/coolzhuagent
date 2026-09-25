use std::collections::HashMap;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use computer_use::{
    ComputerUseAction, ComputerUseActionKind, ComputerUseError, ComputerUseRetryOwner,
    ComputerUseRiskClass, Observation, StepExecution, Verification,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::computer_use_adapters::clamp_stage_timeout;
use serde_json::{json, Value as JsonValue};
use tokio::sync::mpsc as tokio_mpsc;

use crate::browser_bridge_protocol::{
    BridgeRequest, BridgeResponse, BrowserAction, BrowserTabAction, ScrollDirection,
};
use crate::computer_use_adapters::{BrowserBridge, BrowserSnapshot};

const BRIDGE_TIMEOUT: Duration = Duration::from_secs(10);
const SNAPSHOT_RETRY_BACKOFF_MS: [u64; 5] = [100, 200, 400, 800, 1_000];

struct ActiveConnection {
    id: u64,
    sender: tokio_mpsc::UnboundedSender<Message>,
}

struct PendingBridgeResponse {
    sender: mpsc::Sender<BridgeResponse>,
    reply_token: String,
}

struct BrowserBridgeBroker {
    connection: Mutex<Option<ActiveConnection>>,
    pending: Mutex<HashMap<String, PendingBridgeResponse>>,
    next_request: AtomicU64,
    next_connection: AtomicU64,
}

impl BrowserBridgeBroker {
    fn new() -> Self {
        Self {
            connection: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_request: AtomicU64::new(1),
            next_connection: AtomicU64::new(1),
        }
    }

    fn connected(&self) -> bool {
        self.connection
            .lock()
            .is_ok_and(|connection| connection.is_some())
    }

    fn request(
        &self,
        remaining: std::time::Duration,
        make: impl FnOnce(String) -> BridgeRequest,
    ) -> Result<BridgeResponse, ComputerUseError> {
        let request_id = format!(
            "browser-{}",
            self.next_request.fetch_add(1, Ordering::SeqCst)
        );
        let reply_token = reply_token_for(&request_id);
        let request = make(request_id.clone()).with_reply_token(reply_token.clone());
        request
            .validate()
            .map_err(|code| blocked(code, "browser bridge request failed validation"))?;
        let encoded = serde_json::to_string(&request)
            .map_err(|error| backend_error("bridge_protocol_error", error.to_string()))?;
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| backend_error("browser_bridge_failed", "pending map lock failed"))?
            .insert(
                request_id.clone(),
                PendingBridgeResponse {
                    sender: tx,
                    reply_token,
                },
            );
        let sent = self
            .connection
            .lock()
            .ok()
            .and_then(|connection| {
                connection
                    .as_ref()
                    .map(|connection| connection.sender.clone())
            })
            .is_some_and(|sender| sender.send(Message::Text(encoded.into())).is_ok());
        if !sent {
            self.pending
                .lock()
                .ok()
                .map(|mut pending| pending.remove(&request_id));
            return Err(extension_unavailable());
        }
        let timeout = clamp_stage_timeout(remaining, BRIDGE_TIMEOUT);
        let response = wait_for_bridge_response(&rx, timeout).map_err(|error| {
            self.pending
                .lock()
                .ok()
                .map(|mut pending| pending.remove(&request_id));
            match error {
                mpsc::RecvTimeoutError::Timeout => backend_error(
                    "browser_bridge_timeout",
                    format!(
                        "browser extension did not respond within {} ms (remaining budget capped)",
                        timeout.as_millis()
                    ),
                ),
                mpsc::RecvTimeoutError::Disconnected => extension_unavailable(),
            }
        })?;
        if !response.ok {
            let error = response
                .error
                .unwrap_or(crate::browser_bridge_protocol::BridgeError {
                    code: "browser_bridge_failed".to_string(),
                    message: "browser bridge returned a failed response".to_string(),
                });
            return Err(response_error(error.code, error.message));
        }
        Ok(response)
    }

    fn complete(&self, response: BridgeResponse) {
        if let Some(pending) = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&response.request_id))
        {
            let _ = pending.sender.send(response);
        }
    }

    fn complete_with_token(&self, reply_token: &str, response: BridgeResponse) -> bool {
        let Some(pending) = self.pending.lock().ok().and_then(|mut pending| {
            let matches = pending
                .get(&response.request_id)
                .is_some_and(|pending| pending.reply_token == reply_token);
            matches
                .then(|| pending.remove(&response.request_id))
                .flatten()
        }) else {
            return false;
        };
        pending.sender.send(response).is_ok()
    }

    fn pending_response_count(&self) -> usize {
        self.pending
            .lock()
            .map(|pending| pending.len())
            .unwrap_or_default()
    }

    fn disconnect(&self, connection_id: u64) {
        let _cleared = self.connection.lock().ok().is_some_and(|mut connection| {
            if connection
                .as_ref()
                .is_some_and(|active| active.id == connection_id)
            {
                *connection = None;
                true
            } else {
                false
            }
        });
    }
}

fn broker() -> &'static BrowserBridgeBroker {
    static BROKER: OnceLock<BrowserBridgeBroker> = OnceLock::new();
    BROKER.get_or_init(BrowserBridgeBroker::new)
}

pub(crate) fn runtime_nonce_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("CoolzhuAgent")
        .join("runtime")
        .join("browser-bridge-nonce")
}

fn runtime_nonce() -> &'static str {
    static NONCE: OnceLock<String> = OnceLock::new();
    NONCE.get_or_init(|| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let process = std::process::id();
        let mut left = std::collections::hash_map::DefaultHasher::new();
        now.hash(&mut left);
        process.hash(&mut left);
        let mut right = std::collections::hash_map::DefaultHasher::new();
        process.hash(&mut right);
        now.rotate_left(47).hash(&mut right);
        format!("{:016x}{:016x}", left.finish(), right.finish())
    })
}

fn reply_token_for(request_id: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut left = std::collections::hash_map::DefaultHasher::new();
    runtime_nonce().hash(&mut left);
    request_id.hash(&mut left);
    now.hash(&mut left);
    let mut right = std::collections::hash_map::DefaultHasher::new();
    now.rotate_left(23).hash(&mut right);
    request_id.hash(&mut right);
    runtime_nonce().hash(&mut right);
    format!("{:016x}{:016x}", left.finish(), right.finish())
}

fn wait_for_bridge_response(
    rx: &mpsc::Receiver<BridgeResponse>,
    timeout: Duration,
) -> Result<BridgeResponse, mpsc::RecvTimeoutError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        let wait = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tokio::task::block_in_place(|| rx.recv_timeout(timeout))
        }));
        if let Ok(result) = wait {
            return result;
        }
    }
    rx.recv_timeout(timeout)
}

pub(crate) fn ensure_runtime_nonce_file() -> Result<PathBuf, String> {
    let path = runtime_nonce_path();
    let parent = path.parent().ok_or("browser nonce path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    file.write_all(runtime_nonce().as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(path)
}

pub(crate) async fn handle_native_socket(mut socket: WebSocket) {
    let first = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        let _ = socket.close().await;
        return;
    };
    let Ok(BridgeRequest::Hello { request_id, nonce }) = serde_json::from_str(&text) else {
        let _ = socket.close().await;
        return;
    };
    if nonce != runtime_nonce() {
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&BridgeResponse::failure(
                    request_id,
                    "handshake_failed",
                    "browser bridge nonce is invalid",
                ))
                .unwrap_or_default()
                .into(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    }
    let hello = BridgeResponse {
        request_id,
        ok: true,
        browser_id: Some("native-host".to_string()),
        evidence: Some("native_bridge_authenticated".to_string()),
        ..BridgeResponse::default()
    };
    if socket
        .send(Message::Text(
            serde_json::to_string(&hello).unwrap_or_default().into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    let connection_id = broker().next_connection.fetch_add(1, Ordering::SeqCst);
    let (mut websocket_sender, mut websocket_receiver) = socket.split();
    let (sender, mut receiver) = tokio_mpsc::unbounded_channel();
    if let Ok(mut connection) = broker().connection.lock() {
        *connection = Some(ActiveConnection {
            id: connection_id,
            sender,
        });
    }
    let writer = tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            if websocket_sender.send(message).await.is_err() {
                break;
            }
        }
    });
    while let Some(message) = websocket_receiver.next().await {
        let Ok(Message::Text(text)) = message else {
            continue;
        };
        if let Ok(response) = serde_json::from_str::<BridgeResponse>(&text) {
            broker().complete(response);
        }
    }
    broker().disconnect(connection_id);
    writer.abort();
}

#[derive(Debug)]
pub(crate) struct BrowserNativeBridge {
    tab_id: Mutex<Option<String>>,
    known_tabs: Mutex<HashMap<String, KnownBrowserTab>>,
    owner_token: String,
}

#[derive(Debug, Clone, Serialize)]
struct KnownBrowserTab {
    tab_id: String,
    window_id: Option<String>,
    url: Option<String>,
    title: Option<String>,
    owned: bool,
}

fn record_known_tab(
    tabs: &mut HashMap<String, KnownBrowserTab>,
    tab_id: String,
    window_id: Option<String>,
    url: Option<String>,
    title: Option<String>,
    owned: bool,
) {
    let entry = tabs.entry(tab_id.clone()).or_insert(KnownBrowserTab {
        tab_id,
        window_id: None,
        url: None,
        title: None,
        owned: false,
    });
    if window_id.is_some() {
        entry.window_id = window_id;
    }
    if url.is_some() {
        entry.url = url;
    }
    if title.is_some() {
        entry.title = title;
    }
    entry.owned |= owned;
}

fn known_tab_inventory(tabs: &HashMap<String, KnownBrowserTab>) -> Vec<JsonValue> {
    let mut tabs = tabs.values().cloned().collect::<Vec<_>>();
    tabs.sort_by(|left, right| {
        left.tab_id
            .parse::<u64>()
            .unwrap_or(u64::MAX)
            .cmp(&right.tab_id.parse::<u64>().unwrap_or(u64::MAX))
            .then_with(|| left.tab_id.cmp(&right.tab_id))
    });
    tabs.into_iter()
        .filter_map(|tab| serde_json::to_value(tab).ok())
        .collect()
}

impl Default for BrowserNativeBridge {
    fn default() -> Self {
        Self {
            tab_id: Mutex::new(None),
            known_tabs: Mutex::new(HashMap::new()),
            owner_token: reply_token_for("tab-owner"),
        }
    }
}

impl BrowserNativeBridge {
    pub(crate) fn connected() -> bool {
        broker().connected()
    }

    pub(crate) fn preflight() -> Result<(), ComputerUseError> {
        Self::connected()
            .then_some(())
            .ok_or_else(extension_unavailable)
    }
}

impl BrowserBridge for BrowserNativeBridge {
    fn snapshot(&self, remaining: std::time::Duration) -> Result<BrowserSnapshot, ComputerUseError> {
        let pinned_tab_id = self
            .tab_id
            .lock()
            .map_err(|_| backend_error("browser_bridge_failed", "tab lease lock failed"))?
            .clone();
        let response = broker().request(remaining, |request_id| BridgeRequest::Snapshot {
            request_id,
            reply_token: None,
            tab_id: pinned_tab_id.clone(),
        })?;
        let tab_id = response
            .tab_id
            .clone()
            .ok_or_else(|| backend_error("invalid_browser_snapshot", "tab id is missing"))?;
        if pinned_tab_id
            .as_ref()
            .is_some_and(|expected| expected != &tab_id)
        {
            return Err(ComputerUseError::recoverable(
                "tab_lease_changed",
                "browser snapshot returned a different tab",
            ));
        }
        *self
            .tab_id
            .lock()
            .map_err(|_| backend_error("browser_bridge_failed", "tab lease lock failed"))? =
            Some(tab_id.clone());
        let document_id = response
            .document_id
            .clone()
            .ok_or_else(|| backend_error("invalid_browser_snapshot", "document id is missing"))?;
        let url = response
            .url
            .clone()
            .ok_or_else(|| backend_error("invalid_browser_snapshot", "URL is missing"))?;
        let tabs = {
            let mut known_tabs = self.known_tabs.lock().map_err(|_| {
                backend_error("browser_bridge_failed", "known tab inventory lock failed")
            })?;
            record_known_tab(
                &mut known_tabs,
                tab_id.clone(),
                response.window_id.clone(),
                Some(url.clone()),
                response.title.clone(),
                false,
            );
            known_tab_inventory(&known_tabs)
        };
        Ok(BrowserSnapshot {
            page_id: format!(
                "{}:{}:{}",
                response.window_id.as_deref().unwrap_or("window"),
                tab_id,
                document_id
            ),
            url,
            dom_revision: response.dom_revision,
            state: json!({
                "document_id": document_id,
                "tab_id": tab_id,
                "window_id": response.window_id,
                "title": response.title,
                "tabs": tabs,
                "nodes": response.nodes,
            }),
            evidence: vec![response
                .evidence
                .unwrap_or_else(|| "dom_snapshot".to_string())],
        })
    }

    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &BrowserSnapshot,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        if matches!(
            action.kind,
            ComputerUseActionKind::OpenTab
                | ComputerUseActionKind::ActivateTab
                | ComputerUseActionKind::CloseTab
        ) {
            // tab 生命周期动作同样受剩余预算约束：不能在这里丢掉 remaining 用满常量上限。
            return self.execute_tab_action(action, remaining);
        }
        let document_id = expected
            .state
            .get("document_id")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                ComputerUseError::recoverable("stale_observation", "document id missing")
            })?
            .to_string();
        let tab_id = expected
            .state
            .get("tab_id")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| ComputerUseError::recoverable("stale_observation", "tab id missing"))?
            .to_string();
        let browser_action = browser_action(action)?;
        let response = broker().request(remaining, |request_id| BridgeRequest::Act {
            request_id,
            reply_token: None,
            tab_id: Some(tab_id),
            expected_document_id: document_id,
            action: browser_action,
        })?;
        Ok(StepExecution {
            input_sent: true,
            summary: format!("browser {:?} sent to {}", action.kind, action.target),
            evidence: vec![response
                .evidence
                .unwrap_or_else(|| format!("dom_action:{:?}:{}", action.kind, action.target))],
            ..StepExecution::default()
        })
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        _remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError> {
        let visible_progress =
            before.state != after.state || before.surface_identity != after.surface_identity;
        let corpus = after.state.to_string().to_lowercase();
        let achieved = !criteria.is_empty()
            && criteria
                .iter()
                .all(|criterion| criterion_visible(criterion, &corpus));
        Ok(Verification {
            achieved,
            visible_progress,
            summary: if achieved {
                "browser success criteria are visible in the fresh DOM snapshot".to_string()
            } else if visible_progress {
                "browser DOM changed but success criteria are not all visible".to_string()
            } else {
                "browser DOM shows no visible progress".to_string()
            },
            evidence: after.evidence.clone(),
        })
    }
}

impl BrowserNativeBridge {
    fn execute_tab_action(
        &self,
        action: &ComputerUseAction,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        let string_argument = |name: &str| {
            action
                .arguments
                .get(name)
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| blocked("invalid_action", format!("{name} is missing")))
        };
        let tab_action = match action.kind {
            ComputerUseActionKind::OpenTab => BrowserTabAction::Open {
                url: string_argument("url")?,
                activate: action
                    .arguments
                    .get("activate")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(true),
                owner_token: self.owner_token.clone(),
            },
            ComputerUseActionKind::ActivateTab => BrowserTabAction::Activate {
                tab_id: string_argument("tab_id")?,
            },
            ComputerUseActionKind::CloseTab => BrowserTabAction::CloseOwned {
                tab_id: string_argument("tab_id")?,
                owner_token: self.owner_token.clone(),
            },
            _ => return Err(blocked("unsupported_action", "not a tab lifecycle action")),
        };
        tab_action
            .validate()
            .map_err(|code| blocked(code, "browser tab action failed validation"))?;
        let response = broker().request(clamp_stage_timeout(remaining, BRIDGE_TIMEOUT), |request_id| BridgeRequest::Tab {
            request_id,
            reply_token: None,
            action: tab_action,
        })?;
        let affected_tab_id = response.tab_id.clone();
        match action.kind {
            ComputerUseActionKind::OpenTab | ComputerUseActionKind::ActivateTab => {
                let tab_id = affected_tab_id
                    .clone()
                    .ok_or_else(|| backend_error("invalid_tab_response", "tab id is missing"))?;
                let mut known_tabs = self.known_tabs.lock().map_err(|_| {
                    backend_error("browser_bridge_failed", "known tab inventory lock failed")
                })?;
                record_known_tab(
                    &mut known_tabs,
                    tab_id.clone(),
                    response.window_id.clone(),
                    response.url.clone(),
                    response.title.clone(),
                    action.kind == ComputerUseActionKind::OpenTab,
                );
                *self.tab_id.lock().map_err(|_| {
                    backend_error("browser_bridge_failed", "tab lease lock failed")
                })? = Some(tab_id);
            }
            ComputerUseActionKind::CloseTab => {
                let closed = action.arguments.get("tab_id").and_then(JsonValue::as_str);
                if let Some(closed) = closed {
                    self.known_tabs
                        .lock()
                        .map_err(|_| {
                            backend_error(
                                "browser_bridge_failed",
                                "known tab inventory lock failed",
                            )
                        })?
                        .remove(closed);
                }
                let mut lease = self
                    .tab_id
                    .lock()
                    .map_err(|_| backend_error("browser_bridge_failed", "tab lease lock failed"))?;
                if lease.as_deref() == closed {
                    *lease = None;
                }
            }
            _ => {}
        }
        Ok(StepExecution {
            input_sent: true,
            summary: format!("browser {:?} completed", action.kind),
            evidence: vec![response
                .evidence
                .unwrap_or_else(|| format!("tab_action:{:?}", action.kind))],
            ..StepExecution::default()
        })
    }
}

fn browser_action(action: &ComputerUseAction) -> Result<BrowserAction, ComputerUseError> {
    let target = action.target.clone();
    let value = |name: &str| {
        action
            .arguments
            .get(name)
            .and_then(JsonValue::as_str)
            .map(str::to_string)
            .ok_or_else(|| blocked("invalid_action", format!("{name} is missing")))
    };
    let bounded_percent = |name: &str| {
        let value = action
            .arguments
            .get(name)
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| blocked("invalid_action", format!("{name} is missing")))?;
        u8::try_from(value).map_err(|_| blocked("invalid_action", format!("{name} is invalid")))
    };
    let result = match action.kind {
        ComputerUseActionKind::Navigate => BrowserAction::Navigate {
            target,
            url: value("url")?,
        },
        ComputerUseActionKind::Click => BrowserAction::Click { target },
        ComputerUseActionKind::TextInput => BrowserAction::TextInput {
            target,
            text: value("text")?,
        },
        ComputerUseActionKind::Select => BrowserAction::Select {
            target,
            value: value("value")?,
        },
        ComputerUseActionKind::Check => BrowserAction::Check {
            target,
            checked: action
                .arguments
                .get("checked")
                .and_then(JsonValue::as_bool)
                .ok_or_else(|| blocked("invalid_action", "checked is missing"))?,
        },
        ComputerUseActionKind::Submit => BrowserAction::Submit { target },
        ComputerUseActionKind::Drag => BrowserAction::Drag {
            target,
            drop_target: value("drop_target")?,
        },
        ComputerUseActionKind::SliderDrag => BrowserAction::SliderDrag {
            target,
            value: bounded_percent("value")?,
        },
        ComputerUseActionKind::KeyCombination => {
            let keys = action
                .arguments
                .get("keys")
                .and_then(JsonValue::as_array)
                .ok_or_else(|| blocked("invalid_action", "keys is missing"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| blocked("invalid_action", "keys must be strings"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            BrowserAction::KeyCombination { target, keys }
        }
        ComputerUseActionKind::Scroll => BrowserAction::Scroll {
            target,
            direction: match value("direction")?.as_str() {
                "up" => ScrollDirection::Up,
                "down" => ScrollDirection::Down,
                "left" => ScrollDirection::Left,
                "right" => ScrollDirection::Right,
                _ => return Err(blocked("invalid_action", "scroll direction is invalid")),
            },
            amount: action
                .arguments
                .get("amount")
                .and_then(JsonValue::as_u64)
                .unwrap_or(1)
                .try_into()
                .map_err(|_| blocked("invalid_action", "scroll amount is invalid"))?,
        },
        ComputerUseActionKind::HistoryBack => BrowserAction::HistoryBack { target },
        ComputerUseActionKind::HistoryForward => BrowserAction::HistoryForward { target },
        ComputerUseActionKind::DoubleClick => {
            return Err(blocked(
                "unsupported_action",
                "browser action is not allowlisted",
            ))
        }
        ComputerUseActionKind::OpenTab
        | ComputerUseActionKind::ActivateTab
        | ComputerUseActionKind::CloseTab => {
            return Err(blocked(
                "unsupported_action",
                "tab lifecycle actions use the dedicated bridge request",
            ))
        }
    };
    result
        .validate()
        .map_err(|code| blocked(code, "browser action failed validation"))?;
    Ok(result)
}

fn criterion_visible(criterion: &str, corpus: &str) -> bool {
    let normalized = criterion.trim().to_lowercase();
    if normalized.len() >= 4 && corpus.contains(&normalized) {
        return true;
    }
    if criterion_key_value_marker_visible(&normalized, corpus) {
        return true;
    }
    let tokens = normalized
        .split(|character: char| {
            !character.is_alphanumeric() && character != '-' && character != '_'
        })
        .filter(|token| token.len() >= 4)
        .filter(|token| {
            !matches!(
                *token,
                "visible"
                    | "result"
                    | "success"
                    | "browser"
                    | "page"
                    | "snapshot"
                    | "show"
                    | "shows"
                    | "showing"
                    | "title"
                    | "dom"
                    | "node"
                    | "element"
                    | "field"
                    | "input"
                    | "value"
                    | "text"
                    | "search"
                    | "contains"
                    | "contain"
                    | "搜索输入框"
                    | "输入框"
                    | "文本"
                    | "包含"
            )
        })
        .collect::<Vec<_>>();
    let evidence_tokens = tokens
        .iter()
        .copied()
        .filter(|token| is_strong_evidence_token(token))
        .collect::<Vec<_>>();
    if !evidence_tokens.is_empty() {
        return evidence_tokens.iter().all(|token| corpus.contains(*token));
    }
    !tokens.is_empty() && tokens.iter().all(|token| corpus.contains(*token))
}

fn criterion_key_value_marker_visible(normalized: &str, corpus: &str) -> bool {
    for (key, value) in criterion_key_value_markers(normalized) {
        if key_value_marker_visible(&key, &value, corpus) {
            return true;
        }
    }
    false
}

fn criterion_key_value_markers(normalized: &str) -> Vec<(String, String)> {
    let mut markers = Vec::new();
    let chars = normalized.chars().collect::<Vec<_>>();
    for (index, character) in chars.iter().enumerate() {
        if *character != '=' && *character != '：' && *character != ':' {
            continue;
        }
        let key = read_marker_token_backward(&chars, index);
        let value = read_marker_token_forward(&chars, index + 1);
        if valid_marker_pair(&key, &value) {
            markers.push((key, value));
        }
    }

    for phrase in [
        "value is",
        "value equals",
        "value 为",
        "value 是",
        "值为",
        "值是",
    ] {
        let Some(index) = normalized.find(phrase) else {
            continue;
        };
        let char_index = normalized[..index].chars().count() + phrase.chars().count();
        let value = read_marker_token_forward(&chars, char_index);
        if valid_marker_pair("value", &value) {
            markers.push(("value".to_string(), value));
        }
    }
    markers
}

fn read_marker_token_backward(chars: &[char], end: usize) -> String {
    let mut start = end;
    while start > 0 && chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut cursor = start;
    while cursor > 0 && is_marker_token_char(chars[cursor - 1]) {
        cursor -= 1;
    }
    chars[cursor..start].iter().collect::<String>()
}

fn read_marker_token_forward(chars: &[char], start: usize) -> String {
    let mut cursor = start;
    while cursor < chars.len()
        && (chars[cursor].is_whitespace() || matches!(chars[cursor], '"' | '\'' | '`' | '“' | '”'))
    {
        cursor += 1;
    }
    let value_start = cursor;
    while cursor < chars.len() && is_marker_token_char(chars[cursor]) {
        cursor += 1;
    }
    chars[value_start..cursor].iter().collect::<String>()
}

fn is_marker_token_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '+')
}

fn valid_marker_pair(key: &str, value: &str) -> bool {
    (2..=64).contains(&key.len())
        && (1..=128).contains(&value.len())
        && value
            .chars()
            .any(|character| character.is_ascii_alphanumeric())
}

fn key_value_marker_visible(key: &str, value: &str, corpus: &str) -> bool {
    let json_property = format!("\"{key}\":\"{value}\"");
    let text_marker = format!("{key}={value}");
    corpus.contains(&json_property)
        || corpus.contains(&text_marker)
        || (key == "range" && corpus.contains(&format!("\"value\":\"{value}\"")))
}

fn is_strong_evidence_token(token: &str) -> bool {
    let has_digit = token.chars().any(|character| character.is_ascii_digit());
    let has_ascii_alpha = token
        .chars()
        .any(|character| character.is_ascii_alphabetic());
    let has_separator = token.contains('-') || token.contains('_');
    token.starts_with("coolzhu-")
        || (token.len() >= 12 && has_digit && has_ascii_alpha && has_separator)
        || (token.len() >= 24 && has_digit && has_ascii_alpha)
}

fn extension_unavailable() -> ComputerUseError {
    ComputerUseError::blocked(
        "extension_unavailable",
        "Coolzhu browser extension/native host is not connected",
        ComputerUseRetryOwner::User,
    )
}

fn response_error(code: String, message: String) -> ComputerUseError {
    match code.as_str() {
        "stale_document" => ComputerUseError::recoverable(code, message),
        "extension_unavailable" | "native_host_unavailable" | "restricted_page" => {
            ComputerUseError::blocked(code, message, ComputerUseRetryOwner::User)
        }
        _ => ComputerUseError::blocked(code, message, ComputerUseRetryOwner::Model),
    }
}

fn backend_error(code: impl Into<String>, message: impl Into<String>) -> ComputerUseError {
    ComputerUseError::new(code, message, true, ComputerUseRetryOwner::System)
}

fn blocked(code: impl Into<String>, message: impl Into<String>) -> ComputerUseError {
    ComputerUseError::blocked(code, message, ComputerUseRetryOwner::Model)
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowserBridgeHealth {
    pub(crate) connected: bool,
    pub(crate) nonce_file: String,
    pub(crate) setup_required: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowserBridgeProbe {
    pub(crate) ok: bool,
    pub(crate) connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dom_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) node_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_message: Option<String>,
}

impl BrowserBridgeProbe {
    fn from_response(response: BridgeResponse) -> Self {
        Self {
            ok: response.ok,
            connected: BrowserNativeBridge::connected(),
            request_id: Some(response.request_id),
            url: response.url,
            title: response.title,
            dom_revision: Some(response.dom_revision),
            node_count: Some(response.nodes.len()),
            evidence: response.evidence,
            error_code: response.error.as_ref().map(|error| error.code.clone()),
            error_message: response.error.map(|error| error.message),
        }
    }

    fn from_error(error: ComputerUseError) -> Self {
        Self {
            ok: false,
            connected: BrowserNativeBridge::connected(),
            request_id: None,
            url: None,
            title: None,
            dom_revision: None,
            node_count: None,
            evidence: None,
            error_code: Some(error.code),
            error_message: Some(error.message),
        }
    }
}

pub(crate) fn health() -> BrowserBridgeHealth {
    BrowserBridgeHealth {
        connected: BrowserNativeBridge::connected(),
        nonce_file: runtime_nonce_path().display().to_string(),
        setup_required: !BrowserNativeBridge::connected(),
    }
}

pub(crate) fn probe_snapshot() -> BrowserBridgeProbe {
    match broker().request(std::time::Duration::from_secs(30), |request_id| BridgeRequest::Snapshot {
        request_id,
        reply_token: None,
        tab_id: None,
    }) {
        Ok(response) => BrowserBridgeProbe::from_response(response),
        Err(error) => BrowserBridgeProbe::from_error(error),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct BrowserBridgeSelfTestRequest {
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) value: Option<u8>,
    #[serde(default)]
    pub(crate) url: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowserBridgeSelfTestResponse {
    pub(crate) ok: bool,
    pub(crate) kind: String,
    pub(crate) stage: String,
    pub(crate) summary: String,
    pub(crate) url: Option<String>,
    pub(crate) before_revision: Option<u64>,
    pub(crate) after_revision: Option<u64>,
    pub(crate) before_evidence: Vec<String>,
    pub(crate) action_evidence: Vec<String>,
    pub(crate) after_evidence: Vec<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
}

impl BrowserBridgeSelfTestResponse {
    fn failed(
        kind: impl Into<String>,
        stage: impl Into<String>,
        error: ComputerUseError,
        before: Option<&BrowserSnapshot>,
    ) -> Self {
        Self {
            ok: false,
            kind: kind.into(),
            stage: stage.into(),
            summary: error.message.clone(),
            url: before.map(|snapshot| snapshot.url.clone()),
            before_revision: before.map(|snapshot| snapshot.dom_revision),
            after_revision: None,
            before_evidence: before
                .map(|snapshot| snapshot.evidence.clone())
                .unwrap_or_default(),
            action_evidence: Vec::new(),
            after_evidence: Vec::new(),
            error_code: Some(error.code),
            error_message: Some(error.message),
        }
    }
}

pub(crate) fn self_test(request: BrowserBridgeSelfTestRequest) -> BrowserBridgeSelfTestResponse {
    let kind = request
        .kind
        .as_deref()
        .unwrap_or("slider")
        .trim()
        .to_ascii_lowercase();
    let kind = if kind.is_empty() {
        "slider".to_string()
    } else {
        kind
    };
    let value = request.value.unwrap_or(80).min(100);
    if kind == "tab_lifecycle" {
        return tab_lifecycle_self_test(&kind, request.url.as_deref());
    }
    if let Some(url) = browser_self_test_target_url(&request, &kind) {
        return owned_page_action_self_test(&kind, value, url);
    }
    let bridge = BrowserNativeBridge::default();
    let before = match bridge.snapshot(std::time::Duration::from_secs(30)) {
        Ok(snapshot) => snapshot,
        Err(error) => return BrowserBridgeSelfTestResponse::failed(kind, "observe", error, None),
    };
    let action = match browser_self_test_action(&kind, value, &before) {
        Ok(action) => action,
        Err(error) => {
            return BrowserBridgeSelfTestResponse::failed(kind, "plan", error, Some(&before));
        }
    };
    let execution = match bridge.execute(&action, &before, std::time::Duration::from_secs(30)) {
        Ok(execution) => execution,
        Err(error) => {
            return BrowserBridgeSelfTestResponse::failed(kind, "execute", error, Some(&before));
        }
    };
    let after = match bridge.snapshot(std::time::Duration::from_secs(30)) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let mut response =
                BrowserBridgeSelfTestResponse::failed(kind, "verify_observe", error, Some(&before));
            response.action_evidence = execution.evidence;
            return response;
        }
    };
    let before_observation = browser_snapshot_observation(&before);
    let after_observation = browser_snapshot_observation(&after);
    let verification = bridge
        .verify(
            &browser_self_test_criteria(&kind, value),
            &before_observation,
            &after_observation,
            std::time::Duration::from_secs(30),
        )
        .unwrap_or_else(|error| Verification {
            achieved: false,
            visible_progress: false,
            summary: error.message,
            evidence: after.evidence.clone(),
        });
    BrowserBridgeSelfTestResponse {
        ok: verification.achieved,
        kind,
        stage: if verification.achieved {
            "terminal".to_string()
        } else {
            "verify".to_string()
        },
        summary: verification.summary,
        url: Some(after.url.clone()),
        before_revision: Some(before.dom_revision),
        after_revision: Some(after.dom_revision),
        before_evidence: before.evidence,
        action_evidence: execution.evidence,
        after_evidence: after.evidence,
        error_code: None,
        error_message: None,
    }
}

fn browser_self_test_target_url<'a>(
    request: &'a BrowserBridgeSelfTestRequest,
    kind: &str,
) -> Option<&'a str> {
    if kind == "tab_lifecycle" {
        return None;
    }
    request.url.as_deref().map(str::trim).filter(|url| {
        !url.is_empty()
            && url.len() <= 2_048
            && (url.starts_with("http://") || url.starts_with("https://"))
    })
}

fn optional_original_snapshot(
    result: Result<BrowserSnapshot, ComputerUseError>,
) -> Result<Option<BrowserSnapshot>, ComputerUseError> {
    match result {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) if error.code == "restricted_page" => Ok(None),
        Err(error) => Err(error),
    }
}

fn owned_page_action_self_test(
    kind: &str,
    value: u8,
    requested_url: &str,
) -> BrowserBridgeSelfTestResponse {
    let bridge = BrowserNativeBridge::default();
    let original = match optional_original_snapshot(bridge.snapshot(std::time::Duration::from_secs(30))) {
        Ok(snapshot) => snapshot,
        Err(error) => return BrowserBridgeSelfTestResponse::failed(kind, "observe", error, None),
    };
    let original_tab_id = match original.as_ref() {
        Some(original) => {
            let Some(tab_id) = snapshot_tab_id(original) else {
                return BrowserBridgeSelfTestResponse::failed(
                    kind,
                    "observe",
                    backend_error("invalid_browser_snapshot", "original tab id is missing"),
                    Some(original),
                );
            };
            Some(tab_id)
        }
        None => None,
    };
    let open_execution = match bridge.execute_tab_action(&tab_open_action(requested_url), BRIDGE_TIMEOUT) {
        Ok(execution) => execution,
        Err(error) => {
            return BrowserBridgeSelfTestResponse::failed(kind, "open", error, original.as_ref())
        }
    };
    let Some(opened_tab_id) = bridge.tab_id.lock().ok().and_then(|tab_id| tab_id.clone()) else {
        return BrowserBridgeSelfTestResponse::failed(
            kind,
            "open",
            backend_error("invalid_tab_response", "opened tab id is missing"),
            original.as_ref(),
        );
    };
    let mut action_evidence = open_execution.evidence;
    action_evidence.push(format!("opened_tab_id={opened_tab_id}"));
    if original.is_none() {
        action_evidence.push("original_page=restricted".to_string());
    }
    let mut action_before = None;

    let result = (|| -> Result<(BrowserSnapshot, Verification), (String, ComputerUseError)> {
        let before = snapshot_for_url(&bridge, requested_url)
            .map_err(|error| ("observe_owned".to_string(), error))?;
        action_before = Some(before.clone());
        let action = browser_self_test_action(kind, value, &before)
            .map_err(|error| ("plan".to_string(), error))?;
        action_evidence.extend(
            bridge
                .execute(&action, &before, std::time::Duration::from_secs(30))
                .map_err(|error| ("execute".to_string(), error))?
                .evidence,
        );
        let after = bridge
            .snapshot(std::time::Duration::from_secs(30))
            .map_err(|error| ("verify_observe".to_string(), error))?;
        let verification = bridge
            .verify(
                &browser_self_test_criteria(kind, value),
                &browser_snapshot_observation(&before),
                &browser_snapshot_observation(&after),
                std::time::Duration::from_secs(30),
            )
            .map_err(|error| ("verify".to_string(), error))?;

        if let Some(original_tab_id) = original_tab_id.as_deref() {
            action_evidence.extend(
                bridge
                    .execute_tab_action(&tab_id_action(
                        ComputerUseActionKind::ActivateTab,
                        original_tab_id,
                    ), BRIDGE_TIMEOUT)
                    .map_err(|error| ("restore_original".to_string(), error))?
                    .evidence,
            );
            let restored = bridge
                .snapshot(std::time::Duration::from_secs(30))
                .map_err(|error| ("observe_restored".to_string(), error))?;
            if snapshot_tab_id(&restored).as_deref() != Some(original_tab_id) {
                return Err((
                    "observe_restored".to_string(),
                    ComputerUseError::recoverable(
                        "tab_lease_changed",
                        "restoring the original tab returned a different tab",
                    ),
                ));
            }
        }
        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_id_action(
                    ComputerUseActionKind::CloseTab,
                    &opened_tab_id,
                ), BRIDGE_TIMEOUT)
                .map_err(|error| ("close_owned".to_string(), error))?
                .evidence,
        );
        if original.is_none() {
            action_evidence.push("original_page_return=browser_natural".to_string());
        }
        Ok((after, verification))
    })();

    match result {
        Ok((after, verification)) => BrowserBridgeSelfTestResponse {
            ok: verification.achieved,
            kind: kind.to_string(),
            stage: if verification.achieved {
                "terminal".to_string()
            } else {
                "verify".to_string()
            },
            summary: verification.summary,
            url: Some(after.url.clone()),
            before_revision: action_before.as_ref().map(|snapshot| snapshot.dom_revision),
            after_revision: Some(after.dom_revision),
            before_evidence: action_before
                .map(|snapshot| snapshot.evidence)
                .unwrap_or_default(),
            action_evidence,
            after_evidence: after.evidence,
            error_code: None,
            error_message: None,
        },
        Err((stage, error)) => {
            best_effort_restore_and_close_owned_tabs(
                &bridge,
                original_tab_id.as_deref(),
                std::slice::from_ref(&opened_tab_id),
            );
            let failure_before = action_before.as_ref().or(original.as_ref());
            let mut response =
                BrowserBridgeSelfTestResponse::failed(kind, stage, error, failure_before);
            response.action_evidence = action_evidence;
            response
        }
    }
}

fn tab_lifecycle_self_test(
    kind: &str,
    requested_url: Option<&str>,
) -> BrowserBridgeSelfTestResponse {
    let Some(requested_url) = requested_url.map(str::trim).filter(|url| {
        !url.is_empty()
            && url.len() <= 2_048
            && (url.starts_with("http://") || url.starts_with("https://"))
    }) else {
        return BrowserBridgeSelfTestResponse::failed(
            kind,
            "plan",
            blocked(
                "invalid_action",
                "tab_lifecycle self-test requires a bounded http or https url",
            ),
            None,
        );
    };
    let bridge = BrowserNativeBridge::default();
    let before = match optional_original_snapshot(bridge.snapshot(std::time::Duration::from_secs(30))) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => return restricted_tab_lifecycle_self_test(kind, requested_url, &bridge),
        Err(error) => return BrowserBridgeSelfTestResponse::failed(kind, "observe", error, None),
    };
    let Some(original_tab_id) = snapshot_tab_id(&before) else {
        return BrowserBridgeSelfTestResponse::failed(
            kind,
            "observe",
            backend_error("invalid_browser_snapshot", "original tab id is missing"),
            Some(&before),
        );
    };
    let open = tab_open_action(requested_url);
    let open_execution = match bridge.execute_tab_action(&open, BRIDGE_TIMEOUT) {
        Ok(execution) => execution,
        Err(error) => {
            return BrowserBridgeSelfTestResponse::failed(kind, "open", error, Some(&before))
        }
    };
    let Some(opened_tab_id) = bridge.tab_id.lock().ok().and_then(|tab_id| tab_id.clone()) else {
        return BrowserBridgeSelfTestResponse::failed(
            kind,
            "open",
            backend_error("invalid_tab_response", "opened tab id is missing"),
            Some(&before),
        );
    };
    let mut action_evidence = open_execution.evidence;
    action_evidence.push(format!("opened_tab_id={opened_tab_id}"));

    let result = (|| -> Result<BrowserSnapshot, (String, ComputerUseError)> {
        let opened = snapshot_for_url(&bridge, requested_url)
            .map_err(|error| ("observe_opened".to_string(), error))?;
        if snapshot_tab_id(&opened).as_deref() != Some(opened_tab_id.as_str()) {
            return Err((
                "observe_opened".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "opened tab snapshot did not preserve the task lease",
                ),
            ));
        }

        let activate_original = tab_id_action(ComputerUseActionKind::ActivateTab, &original_tab_id);
        action_evidence.extend(
            bridge
                .execute_tab_action(&activate_original, BRIDGE_TIMEOUT)
                .map_err(|error| ("activate_original".to_string(), error))?
                .evidence,
        );
        let original_again = bridge
            .snapshot(std::time::Duration::from_secs(30))
            .map_err(|error| ("observe_original".to_string(), error))?;
        if snapshot_tab_id(&original_again).as_deref() != Some(original_tab_id.as_str()) {
            return Err((
                "observe_original".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "activating the original tab returned a different tab",
                ),
            ));
        }

        let activate_opened = tab_id_action(ComputerUseActionKind::ActivateTab, &opened_tab_id);
        action_evidence.extend(
            bridge
                .execute_tab_action(&activate_opened, BRIDGE_TIMEOUT)
                .map_err(|error| ("activate_opened".to_string(), error))?
                .evidence,
        );
        let _opened_again = snapshot_for_url(&bridge, requested_url)
            .map_err(|error| ("observe_reactivated".to_string(), error))?;

        action_evidence.extend(
            bridge
                .execute_tab_action(&activate_original, BRIDGE_TIMEOUT)
                .map_err(|error| ("restore_original".to_string(), error))?
                .evidence,
        );
        let _restored = bridge
            .snapshot(std::time::Duration::from_secs(30))
            .map_err(|error| ("observe_restored".to_string(), error))?;
        let close_opened = tab_id_action(ComputerUseActionKind::CloseTab, &opened_tab_id);
        action_evidence.extend(
            bridge
                .execute_tab_action(&close_opened, BRIDGE_TIMEOUT)
                .map_err(|error| ("close_owned".to_string(), error))?
                .evidence,
        );
        let after = bridge
            .snapshot(std::time::Duration::from_secs(30))
            .map_err(|error| ("verify_closed".to_string(), error))?;
        if snapshot_tab_id(&after).as_deref() != Some(original_tab_id.as_str()) {
            return Err((
                "verify_closed".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "closing the owned tab did not preserve the restored original tab",
                ),
            ));
        }
        Ok(after)
    })();

    match result {
        Ok(after) => BrowserBridgeSelfTestResponse {
            ok: true,
            kind: kind.to_string(),
            stage: "terminal".to_string(),
            summary: "opened, activated, restored, and closed one task-owned browser tab"
                .to_string(),
            url: Some(after.url.clone()),
            before_revision: Some(before.dom_revision),
            after_revision: Some(after.dom_revision),
            before_evidence: before.evidence,
            action_evidence,
            after_evidence: after.evidence,
            error_code: None,
            error_message: None,
        },
        Err((stage, error)) => {
            best_effort_restore_and_close_owned_tabs(
                &bridge,
                Some(&original_tab_id),
                std::slice::from_ref(&opened_tab_id),
            );
            let mut response =
                BrowserBridgeSelfTestResponse::failed(kind, stage, error, Some(&before));
            response.action_evidence = action_evidence;
            response
        }
    }
}

fn restricted_tab_lifecycle_self_test(
    kind: &str,
    requested_url: &str,
    bridge: &BrowserNativeBridge,
) -> BrowserBridgeSelfTestResponse {
    let first_open = match bridge.execute_tab_action(&tab_open_action(requested_url), BRIDGE_TIMEOUT) {
        Ok(execution) => execution,
        Err(error) => return BrowserBridgeSelfTestResponse::failed(kind, "open", error, None),
    };
    let Some(first_tab_id) = bridge.tab_id.lock().ok().and_then(|tab_id| tab_id.clone()) else {
        return BrowserBridgeSelfTestResponse::failed(
            kind,
            "open",
            backend_error("invalid_tab_response", "first opened tab id is missing"),
            None,
        );
    };
    let mut opened_tab_ids = vec![first_tab_id.clone()];
    let mut action_evidence = first_open.evidence;
    action_evidence.push("original_page=restricted".to_string());
    action_evidence.push(format!("opened_tab_id={first_tab_id}"));
    let mut first_observation = None;

    let result = (|| -> Result<(BrowserSnapshot, BrowserSnapshot), (String, ComputerUseError)> {
        let first = snapshot_for_url(bridge, requested_url)
            .map_err(|error| ("observe_first_owned".to_string(), error))?;
        if snapshot_tab_id(&first).as_deref() != Some(first_tab_id.as_str()) {
            return Err((
                "observe_first_owned".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "first owned tab snapshot did not preserve the task lease",
                ),
            ));
        }
        first_observation = Some(first.clone());

        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_open_action(requested_url), BRIDGE_TIMEOUT)
                .map_err(|error| ("open_second_owned".to_string(), error))?
                .evidence,
        );
        let second_tab_id = bridge
            .tab_id
            .lock()
            .ok()
            .and_then(|tab_id| tab_id.clone())
            .ok_or_else(|| {
                (
                    "open_second_owned".to_string(),
                    backend_error("invalid_tab_response", "second opened tab id is missing"),
                )
            })?;
        if second_tab_id == first_tab_id {
            return Err((
                "open_second_owned".to_string(),
                backend_error(
                    "invalid_tab_response",
                    "second owned tab reused the first tab id",
                ),
            ));
        }
        opened_tab_ids.push(second_tab_id.clone());
        action_evidence.push(format!("opened_tab_id={second_tab_id}"));

        let second = snapshot_for_url(bridge, requested_url)
            .map_err(|error| ("observe_second_owned".to_string(), error))?;
        if snapshot_tab_id(&second).as_deref() != Some(second_tab_id.as_str()) {
            return Err((
                "observe_second_owned".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "second owned tab snapshot did not preserve the task lease",
                ),
            ));
        }

        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_id_action(
                    ComputerUseActionKind::ActivateTab,
                    &first_tab_id,
                ), BRIDGE_TIMEOUT)
                .map_err(|error| ("activate_first_owned".to_string(), error))?
                .evidence,
        );
        let first_again = snapshot_for_url(bridge, requested_url)
            .map_err(|error| ("observe_first_reactivated".to_string(), error))?;
        if snapshot_tab_id(&first_again).as_deref() != Some(first_tab_id.as_str()) {
            return Err((
                "observe_first_reactivated".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "reactivating the first owned tab returned a different tab",
                ),
            ));
        }

        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_id_action(
                    ComputerUseActionKind::ActivateTab,
                    &second_tab_id,
                ), BRIDGE_TIMEOUT)
                .map_err(|error| ("activate_second_owned".to_string(), error))?
                .evidence,
        );
        let second_again = snapshot_for_url(bridge, requested_url)
            .map_err(|error| ("observe_second_reactivated".to_string(), error))?;
        if snapshot_tab_id(&second_again).as_deref() != Some(second_tab_id.as_str()) {
            return Err((
                "observe_second_reactivated".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "reactivating the second owned tab returned a different tab",
                ),
            ));
        }

        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_id_action(
                    ComputerUseActionKind::ActivateTab,
                    &first_tab_id,
                ), BRIDGE_TIMEOUT)
                .map_err(|error| ("activate_first_for_close".to_string(), error))?
                .evidence,
        );
        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_id_action(
                    ComputerUseActionKind::CloseTab,
                    &second_tab_id,
                ), BRIDGE_TIMEOUT)
                .map_err(|error| ("close_second_owned".to_string(), error))?
                .evidence,
        );
        let first_after_close = snapshot_for_url(bridge, requested_url)
            .map_err(|error| ("verify_second_closed".to_string(), error))?;
        if snapshot_tab_id(&first_after_close).as_deref() != Some(first_tab_id.as_str()) {
            return Err((
                "verify_second_closed".to_string(),
                ComputerUseError::recoverable(
                    "tab_lease_changed",
                    "closing the second owned tab did not reveal the first owned tab",
                ),
            ));
        }

        action_evidence.extend(
            bridge
                .execute_tab_action(&tab_id_action(
                    ComputerUseActionKind::CloseTab,
                    &first_tab_id,
                ), BRIDGE_TIMEOUT)
                .map_err(|error| ("close_first_owned".to_string(), error))?
                .evidence,
        );
        action_evidence.push("original_page_return=browser_natural".to_string());
        Ok((first, first_after_close))
    })();

    match result {
        Ok((before, after)) => BrowserBridgeSelfTestResponse {
            ok: true,
            kind: kind.to_string(),
            stage: "terminal".to_string(),
            summary: "opened, activated, and closed two task-owned browser tabs; the restricted \
                      original page resumed naturally"
                .to_string(),
            url: Some(after.url.clone()),
            before_revision: Some(before.dom_revision),
            after_revision: Some(after.dom_revision),
            before_evidence: before.evidence,
            action_evidence,
            after_evidence: after.evidence,
            error_code: None,
            error_message: None,
        },
        Err((stage, error)) => {
            best_effort_restore_and_close_owned_tabs(bridge, None, &opened_tab_ids);
            let mut response = BrowserBridgeSelfTestResponse::failed(
                kind,
                stage,
                error,
                first_observation.as_ref(),
            );
            response.action_evidence = action_evidence;
            response
        }
    }
}

fn snapshot_tab_id(snapshot: &BrowserSnapshot) -> Option<String> {
    snapshot
        .state
        .get("tab_id")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
}

fn snapshot_for_url(
    bridge: &BrowserNativeBridge,
    expected_url: &str,
) -> Result<BrowserSnapshot, ComputerUseError> {
    retry_snapshot_for_url(
        expected_url,
        || bridge.snapshot(std::time::Duration::from_secs(30)),
        |delay| std::thread::sleep(delay),
    )
}

fn retry_snapshot_for_url<SnapshotFn, SleepFn>(
    expected_url: &str,
    mut snapshot: SnapshotFn,
    mut sleep: SleepFn,
) -> Result<BrowserSnapshot, ComputerUseError>
where
    SnapshotFn: FnMut() -> Result<BrowserSnapshot, ComputerUseError>,
    SleepFn: FnMut(Duration),
{
    let mut last_error = None;
    for attempt in 0..=SNAPSHOT_RETRY_BACKOFF_MS.len() {
        match snapshot() {
            Ok(snapshot) if snapshot.url == expected_url => return Ok(snapshot),
            Ok(snapshot) => {
                last_error = Some(ComputerUseError::recoverable(
                    "tab_navigation_pending",
                    format!(
                        "opened tab URL is not ready: expected {expected_url}, got {}",
                        snapshot.url
                    ),
                ));
            }
            Err(error) => last_error = Some(error),
        }
        if let Some(delay_ms) = SNAPSHOT_RETRY_BACKOFF_MS.get(attempt) {
            sleep(Duration::from_millis(*delay_ms));
        }
    }
    // 多层包装不得丢回执：内层失败携带的输入事实必须跟着外层错误一起留给上层。
    let (last_error, last_receipt) = match last_error {
        Some(error) => (format!("{}: {}", error.code, error.message), error.receipt),
        None => ("no snapshot response".to_string(), None),
    };
    let mut timeout = ComputerUseError::recoverable(
        "browser_snapshot_timeout",
        format!(
            "opened tab did not reach expected URL {expected_url} after {} attempts; \
             last_error={last_error}",
            SNAPSHOT_RETRY_BACKOFF_MS.len() + 1
        ),
    );
    if let Some(receipt) = last_receipt {
        timeout = timeout.with_receipt(receipt);
    }
    Err(timeout)
}

fn tab_open_action(url: &str) -> ComputerUseAction {
    ComputerUseAction {
        kind: ComputerUseActionKind::OpenTab,
        target: "browser-tabs".to_string(),
        arguments: json!({ "url": url, "activate": true }),
        risk: ComputerUseRiskClass::Stateful,
    }
}

fn tab_id_action(kind: ComputerUseActionKind, tab_id: &str) -> ComputerUseAction {
    ComputerUseAction {
        kind,
        target: "browser-tabs".to_string(),
        arguments: json!({ "tab_id": tab_id }),
        risk: if kind == ComputerUseActionKind::CloseTab {
            ComputerUseRiskClass::Stateful
        } else {
            ComputerUseRiskClass::ReversibleLocal
        },
    }
}

fn self_test_cleanup_actions(
    original_tab_id: Option<&str>,
    opened_tab_ids: &[String],
) -> Vec<ComputerUseAction> {
    let mut actions =
        Vec::with_capacity(opened_tab_ids.len() + usize::from(original_tab_id.is_some()));
    if let Some(original_tab_id) = original_tab_id {
        actions.push(tab_id_action(
            ComputerUseActionKind::ActivateTab,
            original_tab_id,
        ));
    }
    actions.extend(
        opened_tab_ids
            .iter()
            .rev()
            .map(|tab_id| tab_id_action(ComputerUseActionKind::CloseTab, tab_id)),
    );
    actions
}

fn best_effort_restore_and_close_owned_tabs(
    bridge: &BrowserNativeBridge,
    original_tab_id: Option<&str>,
    opened_tab_ids: &[String],
) {
    for action in self_test_cleanup_actions(original_tab_id, opened_tab_ids) {
        // 自检收尾是独立于 CU 运行的诊断路径，没有 CU 剩余预算可依，用桥自身的常量上限。
        let _ = bridge.execute_tab_action(&action, BRIDGE_TIMEOUT);
    }
}

fn browser_snapshot_observation(snapshot: &BrowserSnapshot) -> Observation {
    Observation {
        generation: snapshot.dom_revision,
        surface: computer_use::ComputerUseSurface::Browser,
        surface_identity: snapshot.page_id.clone(),
        state: json!({ "page": snapshot.state.clone() }),
        evidence: snapshot.evidence.clone(),
    }
}

fn browser_self_test_criteria(kind: &str, value: u8) -> Vec<String> {
    match kind {
        "slider" | "slider_drag" => vec![format!("\"value\":\"{value}\"")],
        "drag" => vec!["drop-complete".to_string()],
        "key" | "key_combination" => vec!["key=ctrl+a".to_string()],
        "enter" => vec!["key=enter".to_string(), "choice=alpha".to_string()],
        _ => vec!["unsupported-self-test-kind".to_string()],
    }
}

fn browser_self_test_action(
    kind: &str,
    value: u8,
    snapshot: &BrowserSnapshot,
) -> Result<ComputerUseAction, ComputerUseError> {
    let nodes = snapshot
        .state
        .get("nodes")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| blocked("target_not_found", "browser snapshot has no nodes"))?;
    match kind {
        "slider" | "slider_drag" => {
            let target = nodes
                .iter()
                .find(|node| {
                    node.get("tag").and_then(JsonValue::as_str) == Some("input")
                        && node.get("input_type").and_then(JsonValue::as_str) == Some("range")
                })
                .and_then(|node| node.get("reference").and_then(JsonValue::as_str))
                .ok_or_else(|| blocked("target_not_found", "range slider was not found"))?;
            Ok(ComputerUseAction {
                kind: ComputerUseActionKind::SliderDrag,
                target: target.to_string(),
                arguments: json!({ "value": value }),
                risk: ComputerUseRiskClass::Stateful,
            })
        }
        "drag" => {
            let source = nodes
                .iter()
                .find(|node| {
                    node.get("text")
                        .and_then(JsonValue::as_str)
                        .is_some_and(|text| text.contains("drag-me"))
                })
                .and_then(|node| node.get("reference").and_then(JsonValue::as_str))
                .ok_or_else(|| blocked("target_not_found", "drag source was not found"))?;
            let drop_target = nodes
                .iter()
                .find(|node| {
                    node.get("text")
                        .and_then(JsonValue::as_str)
                        .is_some_and(|text| text.contains("drop-here"))
                })
                .and_then(|node| node.get("reference").and_then(JsonValue::as_str))
                .ok_or_else(|| blocked("target_not_found", "drop target was not found"))?;
            Ok(ComputerUseAction {
                kind: ComputerUseActionKind::Drag,
                target: source.to_string(),
                arguments: json!({ "drop_target": drop_target }),
                risk: ComputerUseRiskClass::Stateful,
            })
        }
        "key" | "key_combination" => {
            let target = nodes
                .iter()
                .find(|node| {
                    node.get("tag").and_then(JsonValue::as_str) == Some("input")
                        && node.get("input_type").and_then(JsonValue::as_str) == Some("text")
                })
                .and_then(|node| node.get("reference").and_then(JsonValue::as_str))
                .ok_or_else(|| blocked("target_not_found", "text input was not found"))?;
            Ok(ComputerUseAction {
                kind: ComputerUseActionKind::KeyCombination,
                target: target.to_string(),
                arguments: json!({ "keys": ["ctrl", "a"] }),
                risk: ComputerUseRiskClass::ReversibleLocal,
            })
        }
        "enter" => {
            let target = nodes
                .iter()
                .find(|node| {
                    node.get("tag").and_then(JsonValue::as_str) == Some("input")
                        && node.get("input_type").and_then(JsonValue::as_str) == Some("text")
                })
                .and_then(|node| node.get("reference").and_then(JsonValue::as_str))
                .ok_or_else(|| blocked("target_not_found", "text input was not found"))?;
            Ok(ComputerUseAction {
                kind: ComputerUseActionKind::KeyCombination,
                target: target.to_string(),
                arguments: json!({ "keys": ["enter"] }),
                risk: ComputerUseRiskClass::ReversibleLocal,
            })
        }
        _ => Err(blocked(
            "unsupported_self_test",
            "browser self-test kind must be slider, drag, key, or enter",
        )),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowserBridgeResponseAck {
    pub(crate) ok: bool,
    pub(crate) accepted: bool,
    pub(crate) pending_count: usize,
}

pub(crate) fn complete_http_response(
    reply_token: &str,
    response: BridgeResponse,
) -> BrowserBridgeResponseAck {
    let accepted = if reply_token.is_empty() {
        false
    } else {
        broker().complete_with_token(reply_token, response)
    };
    BrowserBridgeResponseAck {
        ok: accepted,
        accepted,
        pending_count: broker().pending_response_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn browser_action_mapping_never_emits_javascript_or_coordinates() {
        let action = ComputerUseAction {
            kind: ComputerUseActionKind::TextInput,
            target: "dom-7".into(),
            arguments: json!({"text":"OpenAI"}),
            risk: computer_use::ComputerUseRiskClass::ReversibleLocal,
        };
        assert!(matches!(
            browser_action(&action).unwrap(),
            BrowserAction::TextInput { .. }
        ));
        let serialized = serde_json::to_string(&browser_action(&action).unwrap()).unwrap();
        assert!(!serialized.contains("javascript"));
        assert!(!serialized.contains("\"x\""));
    }

    #[test]
    fn browser_action_mapping_supports_complex_dom_actions_without_coordinates() {
        let drag = ComputerUseAction {
            kind: ComputerUseActionKind::Drag,
            target: "dom-7".into(),
            arguments: json!({"drop_target":"dom-8"}),
            risk: computer_use::ComputerUseRiskClass::Stateful,
        };
        assert!(matches!(
            browser_action(&drag).unwrap(),
            BrowserAction::Drag { .. }
        ));

        let slider = ComputerUseAction {
            kind: ComputerUseActionKind::SliderDrag,
            target: "dom-9".into(),
            arguments: json!({"value":75}),
            risk: computer_use::ComputerUseRiskClass::Stateful,
        };
        assert!(matches!(
            browser_action(&slider).unwrap(),
            BrowserAction::SliderDrag { value: 75, .. }
        ));

        let keys = ComputerUseAction {
            kind: ComputerUseActionKind::KeyCombination,
            target: "dom-10".into(),
            arguments: json!({"keys":["ctrl","a"]}),
            risk: computer_use::ComputerUseRiskClass::ReversibleLocal,
        };
        let serialized = serde_json::to_string(&browser_action(&keys).unwrap()).unwrap();
        assert!(serialized.contains("key_combination"));
        assert!(!serialized.contains("\"x\""));
        assert!(!serialized.contains("\"y\""));
    }

    #[test]
    fn browser_enter_self_test_uses_one_enter_key_and_visible_marker() {
        let snapshot = BrowserSnapshot {
            page_id: "window:42:document".into(),
            url: "https://example.test/form".into(),
            dom_revision: 1,
            state: json!({
                "nodes": [{
                    "reference": "dom-input-1",
                    "tag": "input",
                    "input_type": "text"
                }]
            }),
            evidence: vec!["dom_snapshot:test".into()],
        };

        let action = browser_self_test_action("enter", 0, &snapshot).unwrap();
        assert_eq!(action.kind, ComputerUseActionKind::KeyCombination);
        assert_eq!(action.arguments["keys"], json!(["enter"]));
        assert_eq!(
            browser_self_test_criteria("enter", 0),
            vec!["key=enter", "choice=alpha"]
        );
    }

    #[test]
    fn known_tab_inventory_preserves_task_ownership_and_is_stable() {
        let mut tabs = HashMap::new();
        record_known_tab(
            &mut tabs,
            "42".to_string(),
            Some("7".to_string()),
            Some("https://example.test/owned".to_string()),
            None,
            true,
        );
        record_known_tab(
            &mut tabs,
            "41".to_string(),
            Some("7".to_string()),
            Some("https://example.test/user".to_string()),
            Some("User tab".to_string()),
            false,
        );
        record_known_tab(
            &mut tabs,
            "42".to_string(),
            Some("7".to_string()),
            Some("https://example.test/owned-ready".to_string()),
            Some("Owned tab".to_string()),
            false,
        );

        let inventory = known_tab_inventory(&tabs);

        assert_eq!(inventory[0]["tab_id"], "41");
        assert_eq!(inventory[1]["tab_id"], "42");
        assert_eq!(inventory[1]["owned"], true);
        assert_eq!(inventory[1]["url"], "https://example.test/owned-ready");
    }

    #[test]
    fn browser_self_test_request_accepts_bounded_lifecycle_url() {
        let request: BrowserBridgeSelfTestRequest = serde_json::from_value(json!({
            "kind": "tab_lifecycle",
            "url": "http://127.0.0.1:8765/tests/fixtures/computer-use-browser.html"
        }))
        .unwrap();

        assert_eq!(request.kind.as_deref(), Some("tab_lifecycle"));
        assert_eq!(
            request.url.as_deref(),
            Some("http://127.0.0.1:8765/tests/fixtures/computer-use-browser.html")
        );
    }

    #[test]
    fn browser_enter_self_test_selects_explicit_owned_page_url() {
        let request: BrowserBridgeSelfTestRequest = serde_json::from_value(json!({
            "kind": "enter",
            "url": "http://127.0.0.1:8765/tests/fixtures/computer-use-browser.html"
        }))
        .unwrap();

        assert_eq!(
            browser_self_test_target_url(&request, "enter"),
            Some("http://127.0.0.1:8765/tests/fixtures/computer-use-browser.html")
        );
    }

    #[test]
    fn restricted_original_page_is_optional_but_other_snapshot_errors_propagate() {
        let restricted = optional_original_snapshot(Err(blocked(
            "restricted_page",
            "the active page cannot be inspected",
        )))
        .unwrap();
        assert!(restricted.is_none());

        let error = optional_original_snapshot(Err(blocked(
            "extension_unavailable",
            "the browser bridge is disconnected",
        )))
        .unwrap_err();
        assert_eq!(error.code, "extension_unavailable");
    }

    #[test]
    fn restricted_lifecycle_cleanup_closes_two_owned_tabs_without_touching_original() {
        let opened_tab_ids = vec!["101".to_string(), "102".to_string()];

        let actions = self_test_cleanup_actions(None, &opened_tab_ids);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].kind, ComputerUseActionKind::CloseTab);
        assert_eq!(actions[0].arguments["tab_id"], "102");
        assert_eq!(actions[1].kind, ComputerUseActionKind::CloseTab);
        assert_eq!(actions[1].arguments["tab_id"], "101");
        assert!(actions
            .iter()
            .all(|action| action.kind != ComputerUseActionKind::ActivateTab));
    }

    #[test]
    fn normal_lifecycle_cleanup_restores_original_and_only_closes_owned_tab() {
        let opened_tab_ids = vec!["101".to_string()];

        let actions = self_test_cleanup_actions(Some("7"), &opened_tab_ids);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].kind, ComputerUseActionKind::ActivateTab);
        assert_eq!(actions[0].arguments["tab_id"], "7");
        assert_eq!(actions[1].kind, ComputerUseActionKind::CloseTab);
        assert_eq!(actions[1].arguments["tab_id"], "101");
        assert!(!actions.iter().any(|action| {
            action.kind == ComputerUseActionKind::CloseTab && action.arguments["tab_id"] == "7"
        }));
    }

    #[test]
    fn disconnected_browser_bridge_is_terminal_and_user_owned() {
        let error = BrowserNativeBridge::preflight().unwrap_err();
        assert_eq!(error.code, "extension_unavailable");
        assert!(!error.retryable);
        assert_eq!(error.retry_owner, ComputerUseRetryOwner::User);
    }

    #[test]
    fn broker_disconnect_preserves_pending_for_http_reply_fallback() {
        let broker = BrowserBridgeBroker::new();
        let (connection_tx, _connection_rx) = tokio_mpsc::unbounded_channel();
        *broker.connection.lock().unwrap() = Some(ActiveConnection {
            id: 7,
            sender: connection_tx,
        });
        let (response_tx, _response_rx) = mpsc::channel();
        broker.pending.lock().unwrap().insert(
            "browser-fallback-1".to_string(),
            PendingBridgeResponse {
                sender: response_tx,
                reply_token: "reply-token-1".to_string(),
            },
        );

        broker.disconnect(7);

        assert!(!broker.connected());
        assert_eq!(broker.pending_response_count(), 1);
    }

    #[test]
    fn browser_probe_summary_does_not_expose_dom_values() {
        let response = BridgeResponse {
            request_id: "probe-1".to_string(),
            ok: true,
            url: Some("https://www.wikipedia.org/".to_string()),
            title: Some("Wikipedia".to_string()),
            dom_revision: 9,
            nodes: vec![crate::browser_bridge_protocol::DomNode {
                reference: "dom-1".to_string(),
                tag: "input".to_string(),
                value: Some("secret query".to_string()),
                text: Some("secret visible text".to_string()),
                ..Default::default()
            }],
            evidence: Some("dom_snapshot:doc:9:nodes=1".to_string()),
            ..Default::default()
        };

        let probe = BrowserBridgeProbe::from_response(response);

        assert!(probe.ok);
        assert_eq!(probe.node_count, Some(1));
        let encoded = serde_json::to_string(&probe).unwrap();
        assert!(encoded.contains("Wikipedia"));
        assert!(!encoded.contains("secret query"));
        assert!(!encoded.contains("secret visible text"));
    }

    #[test]
    fn browser_criterion_requires_marker_not_json_field_name() {
        let corpus_without_marker = serde_json::json!({
            "nodes": [
                {"tag": "input", "value": "", "text": "", "name": "search"}
            ]
        })
        .to_string()
        .to_lowercase();
        let corpus_with_marker = serde_json::json!({
            "nodes": [
                {
                    "tag": "input",
                    "value": "COOLZHU-BROWSER-E2E-959864560ad0",
                    "text": ""
                }
            ]
        })
        .to_string()
        .to_lowercase();
        let criterion = "搜索输入框 value 包含 COOLZHU-BROWSER-E2E-959864560ad0";

        assert!(!criterion_visible(criterion, &corpus_without_marker));
        assert!(criterion_visible(criterion, &corpus_with_marker));
    }

    #[test]
    fn browser_criterion_extracts_key_value_marker_from_natural_language() {
        let corpus = serde_json::json!({
            "nodes": [
                {"tag": "input", "input_type": "range", "value": "80"},
                {"tag": "p", "text": "dropped=false; range=80"}
            ]
        })
        .to_string()
        .to_lowercase();

        assert!(criterion_visible(
            "the DOM snapshot shows range=80.",
            &corpus
        ));
        assert!(criterion_visible("the range slider value is 80", &corpus));
    }

    #[test]
    fn browser_criterion_ignores_snapshot_instruction_words_for_visible_markers() {
        let corpus = serde_json::json!({
            "nodes": [
                {"tag": "div", "text": "drop-complete"}
            ]
        })
        .to_string()
        .to_lowercase();

        assert!(criterion_visible(
            "the DOM snapshot shows drop-complete.",
            &corpus
        ));
    }

    #[test]
    fn snapshot_url_retry_uses_bounded_exponential_backoff_until_ready() {
        let mut urls = std::collections::VecDeque::from([
            "about:blank",
            "https://example.test/loading",
            "https://example.test/ready",
        ]);
        let mut delays = Vec::new();

        let snapshot = retry_snapshot_for_url(
            "https://example.test/ready",
            || {
                let url = urls.pop_front().expect("bounded snapshot attempt");
                Ok(BrowserSnapshot {
                    page_id: "page-1".into(),
                    url: url.into(),
                    dom_revision: 1,
                    state: json!({}),
                    evidence: vec![],
                })
            },
            |delay| delays.push(delay.as_millis() as u64),
        )
        .unwrap();

        assert_eq!(snapshot.url, "https://example.test/ready");
        assert_eq!(delays, vec![100, 200]);
        assert_eq!(SNAPSHOT_RETRY_BACKOFF_MS, [100, 200, 400, 800, 1_000]);
    }

    #[test]
    fn snapshot_url_retry_returns_explicit_timeout_after_bounded_attempts() {
        let mut attempts = 0usize;
        let mut delays = Vec::new();

        let error = retry_snapshot_for_url(
            "https://example.test/ready",
            || {
                attempts += 1;
                Ok(BrowserSnapshot {
                    page_id: "page-1".into(),
                    url: "about:blank".into(),
                    dom_revision: attempts as u64,
                    state: json!({}),
                    evidence: vec![],
                })
            },
            |delay| delays.push(delay.as_millis() as u64),
        )
        .unwrap_err();

        assert_eq!(error.code, "browser_snapshot_timeout");
        assert!(error.retryable);
        assert_eq!(attempts, SNAPSHOT_RETRY_BACKOFF_MS.len() + 1);
        assert_eq!(delays, SNAPSHOT_RETRY_BACKOFF_MS);
        assert!(error.message.contains("after 6 attempts"));
        assert!(error.message.contains("tab_navigation_pending"));
    }

    /// RPR-04b：重试包装不得丢掉内层错误携带的回执（多层包装丢事实 = 上层退回猜测）。
    #[test]
    fn snapshot_url_retry_keeps_the_inner_receipt_through_the_wrapper() {
        let receipt = runtime::ActionReceipt {
            action_id: "browser:open_tab:abc".to_string(),
            input_delivery: runtime::InputDelivery::Sent,
            partial: Some(true),
            path_completed: None,
            confirmed_point_count: None,
            effect: runtime::EffectStatus::NotObserved,
            goal_verdict: runtime::GoalVerdict::NotChecked,
            input_release: runtime::InputReleaseStatus::Unknown,
        };
        receipt.validate().expect("样例回执必须自洽");

        let error = retry_snapshot_for_url(
            "https://example.test/ready",
            || {
                Err(ComputerUseError::recoverable(
                    "stale_document",
                    "document changed while waiting for the tab",
                )
                .with_receipt(receipt.clone()))
            },
            |_| {},
        )
        .unwrap_err();

        assert_eq!(error.code, "browser_snapshot_timeout");
        assert_eq!(
            error.receipt(),
            Some(&receipt),
            "包装后的错误必须仍然带着内层的输入事实"
        );
        assert!(!error.receipt_matches("browser:open_tab:other"));
        assert!(error.receipt_matches("browser:open_tab:abc"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn bridge_response_wait_yields_tokio_worker_for_http_fallback() {
        let (tx, rx) = mpsc::channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            tx.send(BridgeResponse {
                request_id: "probe-yield".to_string(),
                ok: true,
                ..Default::default()
            })
            .unwrap();
        });

        let response = wait_for_bridge_response(&rx, Duration::from_secs(1)).unwrap();

        assert_eq!(response.request_id, "probe-yield");
    }
}
