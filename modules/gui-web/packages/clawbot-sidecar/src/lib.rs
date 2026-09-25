use std::{
    env,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotSidecarConfig {
    pub bind_addr: String,
    pub gateway_base_url: String,
    pub account_id: String,
    pub outbox_limit: usize,
    pub poll_interval_ms: u64,
}

impl Default for ClawbotSidecarConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8787".to_string(),
            gateway_base_url: "http://127.0.0.1:8765".to_string(),
            account_id: "mock-clawbot".to_string(),
            outbox_limit: 20,
            poll_interval_ms: 1_000,
        }
    }
}

impl ClawbotSidecarConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let default = Self::default();
        Self {
            bind_addr: env::var("COOLZHU_CLAWBOT_SIDECAR_BIND").unwrap_or(default.bind_addr),
            gateway_base_url: env::var("COOLZHU_WEB_CONSOLE_URL")
                .unwrap_or(default.gateway_base_url),
            account_id: env::var("COOLZHU_CLAWBOT_ACCOUNT_ID").unwrap_or(default.account_id),
            outbox_limit: env::var("COOLZHU_CLAWBOT_OUTBOX_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default.outbox_limit)
                .clamp(1, 100),
            poll_interval_ms: env::var("COOLZHU_CLAWBOT_POLL_INTERVAL_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default.poll_interval_ms)
                .clamp(100, 60_000),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpClawbotProviderConfig {
    pub base_url: String,
    pub bearer_token: Option<String>,
}

impl HttpClawbotProviderConfig {
    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        self.bearer_token
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderSelection {
    Mock,
    Http(HttpClawbotProviderConfig),
}

pub fn provider_selection_from_values<F>(mut get: F) -> Result<ProviderSelection, String>
where
    F: FnMut(&str) -> Option<String>,
{
    let kind = get("COOLZHU_CLAWBOT_PROVIDER_KIND")
        .unwrap_or_else(|| "mock".to_string())
        .trim()
        .to_ascii_lowercase();
    match kind.as_str() {
        "" | "mock" => Ok(ProviderSelection::Mock),
        "http" => {
            let base_url = get("COOLZHU_CLAWBOT_PROVIDER_URL")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "COOLZHU_CLAWBOT_PROVIDER_KIND=http 时必须设置 COOLZHU_CLAWBOT_PROVIDER_URL"
                        .to_string()
                })?;
            Ok(ProviderSelection::Http(HttpClawbotProviderConfig {
                base_url,
                bearer_token: get("COOLZHU_CLAWBOT_PROVIDER_TOKEN")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
            }))
        }
        other => Err(format!("未知 ClawBot provider kind：{other}")),
    }
}

pub fn provider_selection_from_env() -> Result<ProviderSelection, String> {
    provider_selection_from_values(|key| env::var(key).ok())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotProviderHealth {
    pub provider: String,
    pub provider_version: String,
    pub account_id: Option<String>,
    pub online: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotSidecarHealth {
    pub available: bool,
    pub sidecar_version: String,
    pub gateway_base_url: String,
    pub provider: ClawbotProviderHealth,
    pub last_tick_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotLoginState {
    LoggedOut,
    RefreshRequested,
    AwaitingScan,
    Online,
    Expired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotLoginReport {
    pub generation: u64,
    pub account_id: Option<String>,
    pub state: ClawbotLoginState,
    pub qr_code_data_url: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotMessageKind {
    Text,
    Image,
    Video,
    File,
    Voice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotMediaRef {
    pub media_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotInboundMessage {
    pub account_id: String,
    pub peer_id: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub peer_name: Option<String>,
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub sender_name: Option<String>,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub mentioned_bot: bool,
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub raw_payload_summary: Option<String>,
    pub context_token: Option<String>,
    pub external_msg_id: String,
    pub kind: ClawbotMessageKind,
    pub text: Option<String>,
    pub media_refs: Vec<ClawbotMediaRef>,
    pub received_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotInboundSource {
    WeixinUser,
    GatewayEcho,
    CoolzhuOutbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotInboundEnvelope {
    pub source: ClawbotInboundSource,
    pub hop_count: u32,
    pub message: ClawbotInboundMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxState {
    Pending,
    Sending,
    Sent,
    DeadLetter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotOutboundFile {
    pub local_path: String,
    pub display_name: String,
    pub mime: Option<String>,
    pub size_bytes: u64,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotOutboxItem {
    pub id: i64,
    pub account_id: String,
    pub peer_id: String,
    pub context_token: Option<String>,
    pub source_external_msg_id: Option<String>,
    pub body: String,
    #[serde(default)]
    pub file: Option<ClawbotOutboundFile>,
    pub state: OutboxState,
    pub attempts: u32,
    pub next_attempt_at_ms: u64,
    pub last_error: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotOutboxFailRequest {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSendReceipt {
    pub provider_message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotProviderProbeRequest {
    pub generation: u64,
    pub now_ms: u64,
    pub send_peer_id: Option<String>,
    pub send_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotProviderProbeStep {
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotProviderProbeReport {
    pub ok: bool,
    pub health: ClawbotProviderHealth,
    pub login_state: Option<ClawbotLoginState>,
    pub inbound_count: usize,
    pub send_provider_message_id: Option<String>,
    pub steps: Vec<ClawbotProviderProbeStep>,
}

#[must_use]
pub fn provider_probe_request_from_values<F>(
    mut get: F,
    fallback_now_ms: u64,
) -> ClawbotProviderProbeRequest
where
    F: FnMut(&str) -> Option<String>,
{
    ClawbotProviderProbeRequest {
        generation: get("COOLZHU_CLAWBOT_PROBE_GENERATION")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(1),
        now_ms: get("COOLZHU_CLAWBOT_PROBE_NOW_MS")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(fallback_now_ms),
        send_peer_id: get("COOLZHU_CLAWBOT_PROBE_PEER_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        send_text: get("COOLZHU_CLAWBOT_PROBE_TEXT")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

#[must_use]
pub fn provider_probe_request_from_env() -> ClawbotProviderProbeRequest {
    provider_probe_request_from_values(|key| env::var(key).ok(), unix_timestamp_millis())
}

pub fn redact_secret(value: &str, secrets: &[String]) -> String {
    let mut redacted = value.to_string();
    for secret in secrets {
        let trimmed = secret.trim();
        if trimmed.len() >= 4 {
            redacted = redacted.replace(trimmed, "[redacted]");
        }
    }
    redact_assignment_patterns(&redacted)
}

fn redact_assignment_patterns(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut first = true;
    for token in value.split_whitespace() {
        if !first {
            output.push(' ');
        }
        first = false;
        let lower = token.to_ascii_lowercase();
        if lower == "bearer" {
            output.push_str(token);
            continue;
        }
        if lower.starts_with("bearer ") {
            output.push_str("Bearer [redacted]");
        } else if lower.starts_with("token=")
            || lower.starts_with("cookie=")
            || lower.starts_with("password=")
            || lower.starts_with("secret=")
        {
            if let Some((key, _)) = token.split_once('=') {
                output.push_str(key);
                output.push_str("=[redacted]");
            } else {
                output.push_str("[redacted]");
            }
        } else if token == "[redacted]" {
            output.push_str(token);
        } else {
            output.push_str(token);
        }
    }
    output.replace("Bearer [redacted]", "Bearer [redacted]")
}

pub trait ClawbotProvider: Clone + Send + Sync + 'static {
    fn health(&self, config: &ClawbotSidecarConfig) -> ClawbotProviderHealth;
    fn refresh_login(&self, generation: u64, now_ms: u64) -> Result<ClawbotLoginReport, String>;
    fn logout(&self) -> Result<(), String>;
    fn poll_updates(
        &self,
        config: &ClawbotSidecarConfig,
        now_ms: u64,
    ) -> Result<Vec<ClawbotInboundEnvelope>, String>;
    fn send_text(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String>;
    fn send_message(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
        self.send_text(item)
    }
}

#[must_use]
pub fn run_provider_probe<P: ClawbotProvider>(
    provider: &P,
    config: &ClawbotSidecarConfig,
    request: ClawbotProviderProbeRequest,
) -> ClawbotProviderProbeReport {
    let mut steps = Vec::new();
    let wants_send = request.send_peer_id.is_some() && request.send_text.is_some();
    let health = provider.health(config);
    steps.push(ClawbotProviderProbeStep {
        name: "health".to_string(),
        ok: health.last_error.is_none(),
        error: health.last_error.clone(),
    });

    let login_result = provider.refresh_login(request.generation, request.now_ms);
    let mut login_state = None;
    steps.push(match login_result {
        Ok(report) => {
            login_state = Some(report.state);
            ClawbotProviderProbeStep {
                name: "login_refresh".to_string(),
                ok: report.last_error.is_none(),
                error: report.last_error,
            }
        }
        Err(error) => ClawbotProviderProbeStep {
            name: "login_refresh".to_string(),
            ok: false,
            error: Some(error),
        },
    });

    if wants_send && login_state.as_ref() == Some(&ClawbotLoginState::AwaitingScan) {
        let confirm_result = provider.refresh_login(
            request.generation.saturating_add(1),
            request.now_ms.saturating_add(1),
        );
        steps.push(match confirm_result {
            Ok(report) => {
                login_state = Some(report.state);
                ClawbotProviderProbeStep {
                    name: "login_refresh_confirm".to_string(),
                    ok: report.last_error.is_none(),
                    error: report.last_error,
                }
            }
            Err(error) => ClawbotProviderProbeStep {
                name: "login_refresh_confirm".to_string(),
                ok: false,
                error: Some(error),
            },
        });
    }

    let mut inbound_count = 0;
    match provider.poll_updates(config, request.now_ms) {
        Ok(updates) => {
            inbound_count = updates.len();
            steps.push(ClawbotProviderProbeStep {
                name: "updates".to_string(),
                ok: true,
                error: None,
            });
        }
        Err(error) => steps.push(ClawbotProviderProbeStep {
            name: "updates".to_string(),
            ok: false,
            error: Some(error),
        }),
    }

    let mut send_provider_message_id = None;
    if let (Some(peer_id), Some(body)) = (request.send_peer_id, request.send_text) {
        let item = ClawbotOutboxItem {
            id: -1,
            account_id: config.account_id.clone(),
            peer_id,
            context_token: None,
            source_external_msg_id: None,
            body,
            file: None,
            state: OutboxState::Sending,
            attempts: 0,
            next_attempt_at_ms: request.now_ms,
            last_error: None,
            created_at_ms: request.now_ms,
            updated_at_ms: request.now_ms,
        };
        match provider.send_text(&item) {
            Ok(receipt) => {
                send_provider_message_id = Some(receipt.provider_message_id);
                steps.push(ClawbotProviderProbeStep {
                    name: "send_text".to_string(),
                    ok: true,
                    error: None,
                });
            }
            Err(error) => steps.push(ClawbotProviderProbeStep {
                name: "send_text".to_string(),
                ok: false,
                error: Some(error),
            }),
        }
    }

    let ok = steps.iter().all(|step| step.ok);
    ClawbotProviderProbeReport {
        ok,
        health,
        login_state,
        inbound_count,
        send_provider_message_id,
        steps,
    }
}

#[derive(Debug, Clone)]
pub struct MockClawbotProvider {
    configured: bool,
    online: bool,
    send_fails: bool,
    inbound_text: Option<String>,
}

impl Default for MockClawbotProvider {
    fn default() -> Self {
        Self {
            configured: false,
            online: false,
            send_fails: false,
            inbound_text: None,
        }
    }
}

impl MockClawbotProvider {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            configured: env::var("COOLZHU_CLAWBOT_MOCK_CONFIGURED")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
            online: env::var("COOLZHU_CLAWBOT_MOCK_ONLINE")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
            send_fails: env::var("COOLZHU_CLAWBOT_MOCK_SEND_FAILS")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
            inbound_text: env::var("COOLZHU_CLAWBOT_MOCK_INBOUND_TEXT").ok(),
        }
    }

    #[must_use]
    pub fn configured_online() -> Self {
        Self {
            configured: true,
            online: true,
            send_fails: false,
            inbound_text: Some("来自 mock provider 的微信消息".to_string()),
        }
    }

    #[must_use]
    pub fn configured_with_send_failure() -> Self {
        Self {
            configured: true,
            online: true,
            send_fails: true,
            inbound_text: None,
        }
    }
}

impl ClawbotProvider for MockClawbotProvider {
    fn health(&self, config: &ClawbotSidecarConfig) -> ClawbotProviderHealth {
        ClawbotProviderHealth {
            provider: "mock-clawbot".to_string(),
            provider_version: "0.2.0".to_string(),
            account_id: self.configured.then(|| config.account_id.clone()),
            online: self.online,
            last_error: (!self.configured)
                .then(|| "ClawBot provider 尚未配置真实微信/iLink 凭据".to_string()),
        }
    }

    fn refresh_login(&self, generation: u64, now_ms: u64) -> Result<ClawbotLoginReport, String> {
        if !self.configured {
            return Ok(ClawbotLoginReport {
                generation,
                account_id: None,
                state: ClawbotLoginState::Error,
                qr_code_data_url: None,
                expires_at_ms: None,
                last_error: Some("ClawBot provider 尚未配置真实微信/iLink 凭据".to_string()),
            });
        }
        Ok(ClawbotLoginReport {
            generation,
            account_id: None,
            state: ClawbotLoginState::AwaitingScan,
            qr_code_data_url: Some("data:image/png;base64,bW9jay1xci1jb2Rl".to_string()),
            expires_at_ms: Some(now_ms.saturating_add(120_000)),
            last_error: None,
        })
    }

    fn logout(&self) -> Result<(), String> {
        Ok(())
    }

    fn poll_updates(
        &self,
        config: &ClawbotSidecarConfig,
        now_ms: u64,
    ) -> Result<Vec<ClawbotInboundEnvelope>, String> {
        let Some(text) = &self.inbound_text else {
            return Ok(Vec::new());
        };
        Ok(vec![ClawbotInboundEnvelope {
            source: ClawbotInboundSource::WeixinUser,
            hop_count: 0,
            message: ClawbotInboundMessage {
                account_id: config.account_id.clone(),
                peer_id: "mock-peer".to_string(),
                conversation_id: None,
                peer_name: Some("Mock 微信联系人".to_string()),
                sender_id: Some("mock-peer".to_string()),
                sender_name: Some("Mock 微信联系人".to_string()),
                is_group: false,
                mentioned_bot: false,
                mentions: Vec::new(),
                raw_payload_summary: None,
                context_token: None,
                external_msg_id: format!("mock-{now_ms}"),
                kind: ClawbotMessageKind::Text,
                text: Some(text.clone()),
                media_refs: Vec::new(),
                received_at_ms: now_ms,
            },
        }])
    }

    fn send_text(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
        if self.send_fails {
            Err("mock provider 发送失败".to_string())
        } else {
            Ok(ProviderSendReceipt {
                provider_message_id: format!("mock-sent-{}", item.id),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpClawbotProvider {
    config: HttpClawbotProviderConfig,
}

impl HttpClawbotProvider {
    pub fn new(config: HttpClawbotProviderConfig) -> Self {
        Self { config }
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn request_with_timeout(
        &self,
        method: reqwest::Method,
        path: &str,
        timeout: Duration,
    ) -> reqwest::blocking::RequestBuilder {
        let http = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .expect("blocking reqwest client should build");
        let request = http.request(method, self.endpoint(path));
        if let Some(token) = self.config.bearer_token.as_deref() {
            request.bearer_auth(token)
        } else {
            request
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::blocking::RequestBuilder {
        self.request_with_timeout(method, path, Duration::from_secs(10))
    }

    fn redact_error(&self, error: impl ToString) -> String {
        redact_secret(&error.to_string(), &self.config.secrets())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpRefreshLoginRequest {
    generation: u64,
    account_id: String,
    now_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpLogoutRequest {
    account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpUpdatesResponse {
    updates: Vec<ClawbotInboundEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpSendTextRequest {
    account_id: String,
    peer_id: String,
    context_token: Option<String>,
    source_external_msg_id: Option<String>,
    body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpSendFileRequest {
    account_id: String,
    peer_id: String,
    context_token: Option<String>,
    source_external_msg_id: Option<String>,
    body: String,
    local_path: String,
    display_name: String,
    mime: Option<String>,
    size_bytes: u64,
    checksum: Option<String>,
}

impl ClawbotProvider for HttpClawbotProvider {
    fn health(&self, _config: &ClawbotSidecarConfig) -> ClawbotProviderHealth {
        match self.request(reqwest::Method::GET, "/health").send() {
            Ok(response) => match response.error_for_status() {
                Ok(ok) => ok.json::<ClawbotProviderHealth>().unwrap_or_else(|error| {
                    ClawbotProviderHealth {
                        provider: "http-clawbot".to_string(),
                        provider_version: "unknown".to_string(),
                        account_id: None,
                        online: false,
                        last_error: Some(
                            self.redact_error(format!("解析 provider health 失败：{error}")),
                        ),
                    }
                }),
                Err(error) => ClawbotProviderHealth {
                    provider: "http-clawbot".to_string(),
                    provider_version: "unknown".to_string(),
                    account_id: None,
                    online: false,
                    last_error: Some(
                        self.redact_error(format!("provider health 返回错误：{error}")),
                    ),
                },
            },
            Err(error) => ClawbotProviderHealth {
                provider: "http-clawbot".to_string(),
                provider_version: "unknown".to_string(),
                account_id: None,
                online: false,
                last_error: Some(self.redact_error(format!("provider health 请求失败：{error}"))),
            },
        }
    }

    fn refresh_login(&self, generation: u64, now_ms: u64) -> Result<ClawbotLoginReport, String> {
        let account_id = String::new();
        self.request(reqwest::Method::POST, "/login/refresh")
            .json(&HttpRefreshLoginRequest {
                generation,
                account_id,
                now_ms,
            })
            .send()
            .map_err(|error| self.redact_error(format!("请求 provider 刷新登录失败：{error}")))?
            .error_for_status()
            .map_err(|error| self.redact_error(format!("provider 刷新登录返回错误：{error}")))?
            .json::<ClawbotLoginReport>()
            .map_err(|error| self.redact_error(format!("解析 provider 登录报告失败：{error}")))
    }

    fn logout(&self) -> Result<(), String> {
        self.request(reqwest::Method::POST, "/login/logout")
            .json(&HttpLogoutRequest {
                account_id: String::new(),
            })
            .send()
            .map_err(|error| self.redact_error(format!("请求 provider 登出失败：{error}")))?
            .error_for_status()
            .map_err(|error| self.redact_error(format!("provider 登出返回错误：{error}")))?;
        Ok(())
    }

    fn poll_updates(
        &self,
        config: &ClawbotSidecarConfig,
        now_ms: u64,
    ) -> Result<Vec<ClawbotInboundEnvelope>, String> {
        let response = self
            .request_with_timeout(reqwest::Method::GET, "/updates", Duration::from_secs(45))
            .query(&[
                ("account_id", config.account_id.as_str()),
                ("since_ms", &now_ms.to_string()),
            ])
            .send()
            .map_err(|error| self.redact_error(format!("请求 provider updates 失败：{error}")))?
            .error_for_status()
            .map_err(|error| self.redact_error(format!("provider updates 返回错误：{error}")))?
            .json::<HttpUpdatesResponse>()
            .map_err(|error| self.redact_error(format!("解析 provider updates 失败：{error}")))?;
        Ok(response.updates)
    }

    fn send_text(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
        let response = self
            .request(reqwest::Method::POST, "/send_text")
            .json(&HttpSendTextRequest {
                account_id: item.account_id.clone(),
                peer_id: item.peer_id.clone(),
                context_token: item.context_token.clone(),
                source_external_msg_id: item.source_external_msg_id.clone(),
                body: item.body.clone(),
            })
            .send()
            .map_err(|error| self.redact_error(format!("请求 provider 发送失败：{error}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|error| format!("读取错误响应失败：{error}"));
            return Err(self.redact_error(format!("provider 发送失败：HTTP {status}; {body}")));
        }
        response
            .json::<ProviderSendReceipt>()
            .map_err(|error| self.redact_error(format!("解析 provider 发送回执失败：{error}")))
    }

    fn send_message(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
        let Some(file) = &item.file else {
            return self.send_text(item);
        };
        let response = self
            .request(reqwest::Method::POST, "/send_file")
            .json(&HttpSendFileRequest {
                account_id: item.account_id.clone(),
                peer_id: item.peer_id.clone(),
                context_token: item.context_token.clone(),
                source_external_msg_id: item.source_external_msg_id.clone(),
                body: item.body.clone(),
                local_path: file.local_path.clone(),
                display_name: file.display_name.clone(),
                mime: file.mime.clone(),
                size_bytes: file.size_bytes,
                checksum: file.checksum.clone(),
            })
            .send()
            .map_err(|error| self.redact_error(format!("请求 provider 发送文件失败：{error}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|error| format!("读取错误响应失败：{error}"));
            return Err(self.redact_error(format!(
                "provider 发送文件失败：HTTP {status}; {body}"
            )));
        }
        response
            .json::<ProviderSendReceipt>()
            .map_err(|error| self.redact_error(format!("解析 provider 文件回执失败：{error}")))
    }
}

#[derive(Debug, Clone)]
pub enum AnyClawbotProvider {
    Mock(MockClawbotProvider),
    Http(HttpClawbotProvider),
}

impl AnyClawbotProvider {
    pub fn from_selection(selection: ProviderSelection) -> Self {
        match selection {
            ProviderSelection::Mock => Self::Mock(MockClawbotProvider::from_env()),
            ProviderSelection::Http(config) => Self::Http(HttpClawbotProvider::new(config)),
        }
    }
}

impl ClawbotProvider for AnyClawbotProvider {
    fn health(&self, config: &ClawbotSidecarConfig) -> ClawbotProviderHealth {
        match self {
            Self::Mock(provider) => provider.health(config),
            Self::Http(provider) => provider.health(config),
        }
    }

    fn refresh_login(&self, generation: u64, now_ms: u64) -> Result<ClawbotLoginReport, String> {
        match self {
            Self::Mock(provider) => provider.refresh_login(generation, now_ms),
            Self::Http(provider) => provider.refresh_login(generation, now_ms),
        }
    }

    fn logout(&self) -> Result<(), String> {
        match self {
            Self::Mock(provider) => provider.logout(),
            Self::Http(provider) => provider.logout(),
        }
    }

    fn poll_updates(
        &self,
        config: &ClawbotSidecarConfig,
        now_ms: u64,
    ) -> Result<Vec<ClawbotInboundEnvelope>, String> {
        match self {
            Self::Mock(provider) => provider.poll_updates(config, now_ms),
            Self::Http(provider) => provider.poll_updates(config, now_ms),
        }
    }

    fn send_text(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
        match self {
            Self::Mock(provider) => provider.send_text(item),
            Self::Http(provider) => provider.send_text(item),
        }
    }

    fn send_message(&self, item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
        match self {
            Self::Mock(provider) => provider.send_message(item),
            Self::Http(provider) => provider.send_message(item),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GatewayClient {
    base_url: String,
    http: reqwest::Client,
}

impl GatewayClient {
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client should build"),
        }
    }

    async fn put_login_report(&self, report: &ClawbotLoginReport) -> Result<(), String> {
        self.http
            .put(format!(
                "{}/api/channels/clawbot/gateway/login/report",
                self.base_url
            ))
            .json(report)
            .send()
            .await
            .map_err(|error| format!("上报 ClawBot 登录状态失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("web-console 拒绝 ClawBot 登录状态：{error}"))?;
        Ok(())
    }

    async fn dispatch_inbound(&self, envelope: &ClawbotInboundEnvelope) -> Result<(), String> {
        self.http
            .post(format!(
                "{}/api/channels/clawbot/inbound/dispatch",
                self.base_url
            ))
            .timeout(Duration::from_secs(300))
            .json(envelope)
            .send()
            .await
            .map_err(|error| format!("转发 ClawBot 入站消息失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("web-console 拒绝 ClawBot 入站消息：{error}"))?;
        Ok(())
    }

    async fn claim_outbox(&self, limit: usize) -> Result<Vec<ClawbotOutboxItem>, String> {
        let response = self
            .http
            .get(format!(
                "{}/api/channels/clawbot/outbox?limit={}",
                self.base_url, limit
            ))
            .send()
            .await
            .map_err(|error| format!("认领 ClawBot outbox 失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("web-console 拒绝 ClawBot outbox claim：{error}"))?;
        response
            .json()
            .await
            .map_err(|error| format!("解析 ClawBot outbox claim 失败：{error}"))
    }

    async fn ack_outbox(&self, id: i64) -> Result<(), String> {
        self.http
            .post(format!(
                "{}/api/channels/clawbot/outbox/{id}/ack",
                self.base_url
            ))
            .send()
            .await
            .map_err(|error| format!("确认 ClawBot outbox 发送成功失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("web-console 拒绝 ClawBot outbox ack：{error}"))?;
        Ok(())
    }

    async fn fail_outbox(&self, id: i64, error: &str) -> Result<(), String> {
        self.http
            .post(format!(
                "{}/api/channels/clawbot/outbox/{id}/fail",
                self.base_url
            ))
            .json(&ClawbotOutboxFailRequest {
                error: error.to_string(),
            })
            .send()
            .await
            .map_err(|send_error| format!("上报 ClawBot outbox 失败状态失败：{send_error}"))?
            .error_for_status()
            .map_err(|status_error| {
                format!("web-console 拒绝 ClawBot outbox fail：{status_error}")
            })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SidecarRuntime<P> {
    config: ClawbotSidecarConfig,
    provider: P,
    gateway: GatewayClient,
    last_tick_error: Arc<Mutex<Option<String>>>,
    tick_lock: Arc<tokio::sync::Mutex<()>>,
}

impl<P: ClawbotProvider> SidecarRuntime<P> {
    #[must_use]
    pub fn new(config: ClawbotSidecarConfig, provider: P) -> Self {
        let gateway = GatewayClient::new(config.gateway_base_url.clone());
        Self {
            config,
            provider,
            gateway,
            last_tick_error: Arc::new(Mutex::new(None)),
            tick_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    #[must_use]
    pub fn health_snapshot(&self) -> ClawbotSidecarHealth {
        let provider = self.provider.health(&self.config);
        self.health_snapshot_from_provider(provider)
    }

    fn health_snapshot_from_provider(
        &self,
        provider: ClawbotProviderHealth,
    ) -> ClawbotSidecarHealth {
        let last_tick_error = self
            .last_tick_error
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| Some("ClawBot sidecar tick 状态锁已损坏".to_string()));
        ClawbotSidecarHealth {
            available: provider.online
                && provider.last_error.is_none()
                && last_tick_error.is_none(),
            sidecar_version: env!("CARGO_PKG_VERSION").to_string(),
            gateway_base_url: self.config.gateway_base_url.clone(),
            provider,
            last_tick_error,
        }
    }

    pub async fn health_snapshot_async(&self) -> ClawbotSidecarHealth {
        let provider = self.provider.clone();
        let config = self.config.clone();
        let provider = match tokio::task::spawn_blocking(move || provider.health(&config)).await {
            Ok(provider) => provider,
            Err(error) => ClawbotProviderHealth {
                provider: "unknown".to_string(),
                provider_version: "unknown".to_string(),
                account_id: Some(self.config.account_id.clone()),
                online: false,
                last_error: Some(format!("ClawBot provider health 任务崩溃：{error}")),
            },
        };
        self.health_snapshot_from_provider(provider)
    }

    pub async fn refresh_login(
        &self,
        generation: u64,
        now_ms: u64,
    ) -> Result<ClawbotLoginReport, String> {
        let provider = self.provider.clone();
        let report =
            tokio::task::spawn_blocking(move || provider.refresh_login(generation, now_ms))
                .await
                .map_err(|error| format!("ClawBot provider 登录刷新任务崩溃：{error}"))??;
        self.gateway.put_login_report(&report).await?;
        Ok(report)
    }

    pub async fn tick(&self, now_ms: u64) -> Result<ClawbotTickSummary, String> {
        let _tick_guard = self.tick_lock.lock().await;
        let result = self.tick_once(now_ms).await;
        if let Ok(mut last_tick_error) = self.last_tick_error.lock() {
            *last_tick_error = result.as_ref().err().cloned();
        }
        result
    }

    async fn tick_once(&self, now_ms: u64) -> Result<ClawbotTickSummary, String> {
        let mut summary = ClawbotTickSummary::default();
        let provider = self.provider.clone();
        let config = self.config.clone();
        let updates = tokio::task::spawn_blocking(move || provider.poll_updates(&config, now_ms))
            .await
            .map_err(|error| format!("ClawBot provider 拉取入站消息任务崩溃：{error}"))??;
        for envelope in updates {
            self.gateway.dispatch_inbound(&envelope).await?;
            summary.inbound_dispatched += 1;
        }
        for item in self.gateway.claim_outbox(self.config.outbox_limit).await? {
            let provider = self.provider.clone();
            let provider_item = item.clone();
            let send_result =
                tokio::task::spawn_blocking(move || provider.send_message(&provider_item))
                    .await
                    .map_err(|error| format!("ClawBot provider 发送消息任务崩溃：{error}"))?;
            match send_result {
                Ok(_) => {
                    self.gateway.ack_outbox(item.id).await?;
                    summary.outbox_sent += 1;
                }
                Err(error) => {
                    self.gateway.fail_outbox(item.id, &error).await?;
                    summary.outbox_failed += 1;
                }
            }
        }
        Ok(summary)
    }

    pub async fn logout(&self) -> Result<(), String> {
        let provider = self.provider.clone();
        tokio::task::spawn_blocking(move || provider.logout())
            .await
            .map_err(|error| format!("ClawBot provider 退出登录任务崩溃：{error}"))?
    }
}

/// 启动 sidecar 自有的串行轮询循环。provider 的长轮询完成后再等待一个短间隔，
/// 保证没有外部调用 `/tick` 时也能持续收取微信消息并发送 outbox。
pub fn spawn_polling_loop<P: ClawbotProvider>(
    runtime: SidecarRuntime<P>,
) -> tokio::task::JoinHandle<()> {
    let poll_interval_ms = runtime.config.poll_interval_ms;
    tokio::spawn(async move {
        loop {
            if let Err(error) = runtime.tick(unix_timestamp_millis()).await {
                tracing::warn!("ClawBot sidecar 后台轮询失败：{error}");
            }
            tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;
        }
    })
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotTickSummary {
    pub inbound_dispatched: u32,
    pub outbox_sent: u32,
    pub outbox_failed: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarLoginRefreshRequest {
    pub generation: u64,
    #[serde(default)]
    pub now_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SidecarLogoutResponse {
    pub ok: bool,
}

pub async fn health_handler<P: ClawbotProvider>(
    State(runtime): State<SidecarRuntime<P>>,
) -> Json<ClawbotSidecarHealth> {
    Json(runtime.health_snapshot_async().await)
}

pub async fn tick_handler<P: ClawbotProvider>(
    State(runtime): State<SidecarRuntime<P>>,
) -> Json<Result<ClawbotTickSummary, String>> {
    Json(runtime.tick(unix_timestamp_millis()).await)
}

pub async fn login_refresh_handler<P: ClawbotProvider>(
    State(runtime): State<SidecarRuntime<P>>,
    Json(payload): Json<SidecarLoginRefreshRequest>,
) -> Json<Result<ClawbotLoginReport, String>> {
    Json(
        runtime
            .refresh_login(
                payload.generation,
                payload.now_ms.unwrap_or_else(unix_timestamp_millis),
            )
            .await,
    )
}

pub async fn logout_handler<P: ClawbotProvider>(
    State(runtime): State<SidecarRuntime<P>>,
) -> Json<Result<SidecarLogoutResponse, String>> {
    Json(
        runtime
            .logout()
            .await
            .map(|()| SidecarLogoutResponse { ok: true }),
    )
}

#[must_use]
pub fn unix_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use axum::{
        extract::State,
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post, put},
        Json, Router,
    };
    use serde_json::{json, Value};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Default, Debug)]
    struct GatewayCapture {
        requests: Mutex<Vec<(String, Value)>>,
        outbox: Mutex<Vec<ClawbotOutboxItem>>,
        inbound_delay_ms: AtomicU64,
    }

    async fn capture_login(
        State(capture): State<Arc<GatewayCapture>>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        capture
            .requests
            .lock()
            .expect("capture lock")
            .push(("login_report".to_string(), payload));
        Json(json!({"ok": true}))
    }

    async fn capture_inbound(
        State(capture): State<Arc<GatewayCapture>>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        let delay_ms = capture.inbound_delay_ms.load(Ordering::SeqCst);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        capture
            .requests
            .lock()
            .expect("capture lock")
            .push(("inbound_dispatch".to_string(), payload));
        Json(json!({"status": "accepted"}))
    }

    async fn claim_outbox(
        State(capture): State<Arc<GatewayCapture>>,
    ) -> Json<Vec<ClawbotOutboxItem>> {
        let mut outbox = capture.outbox.lock().expect("outbox lock");
        Json(std::mem::take(&mut *outbox))
    }

    async fn ack_outbox(
        State(capture): State<Arc<GatewayCapture>>,
        axum::extract::Path(id): axum::extract::Path<i64>,
    ) -> Json<Value> {
        capture
            .requests
            .lock()
            .expect("capture lock")
            .push((format!("outbox_ack:{id}"), json!({})));
        Json(json!({"ok": true}))
    }

    async fn fail_outbox(
        State(capture): State<Arc<GatewayCapture>>,
        axum::extract::Path(id): axum::extract::Path<i64>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        capture
            .requests
            .lock()
            .expect("capture lock")
            .push((format!("outbox_fail:{id}"), payload));
        Json(json!({"ok": true}))
    }

    async fn test_gateway(capture: Arc<GatewayCapture>) -> String {
        let app = Router::new()
            .route(
                "/api/channels/clawbot/gateway/login/report",
                put(capture_login),
            )
            .route(
                "/api/channels/clawbot/inbound/dispatch",
                post(capture_inbound),
            )
            .route("/api/channels/clawbot/outbox", get(claim_outbox))
            .route("/api/channels/clawbot/outbox/{id}/ack", post(ack_outbox))
            .route("/api/channels/clawbot/outbox/{id}/fail", post(fail_outbox))
            .with_state(capture);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve test gateway");
        });
        format!("http://{addr}")
    }

    #[derive(Default, Debug)]
    struct ProviderCapture {
        auth_headers: Mutex<Vec<Option<String>>>,
        send_paths: Mutex<Vec<String>>,
        send_text_status: Mutex<Option<u16>>,
        updates_delay_ms: AtomicU64,
    }

    async fn provider_health(
        State(capture): State<Arc<ProviderCapture>>,
        headers: axum::http::HeaderMap,
    ) -> Json<Value> {
        capture.auth_headers.lock().expect("auth lock").push(
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string),
        );
        Json(json!({
            "provider": "http-clawbot-test",
            "provider_version": "1.2.3",
            "account_id": "wx-http",
            "online": true,
            "last_error": null
        }))
    }

    async fn provider_refresh_login(
        State(capture): State<Arc<ProviderCapture>>,
        headers: axum::http::HeaderMap,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        capture.auth_headers.lock().expect("auth lock").push(
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string),
        );
        Json(json!({
            "generation": payload["generation"],
            "account_id": "wx-http",
            "state": "awaiting_scan",
            "qr_code_data_url": "data:image/png;base64,aHR0cC1xcg==",
            "expires_at_ms": 123456,
            "last_error": null
        }))
    }

    async fn provider_updates(State(capture): State<Arc<ProviderCapture>>) -> Json<Value> {
        let delay_ms = capture.updates_delay_ms.load(Ordering::SeqCst);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        Json(json!({
            "updates": [{
                "source": "weixin_user",
                "hop_count": 0,
                "message": {
                    "account_id": "wx-http",
                    "peer_id": "group-http",
                    "conversation_id": "group-http",
                    "peer_name": "HTTP 测试群",
                    "sender_id": "member-http",
                    "sender_name": "HTTP 群成员",
                    "is_group": true,
                    "mentioned_bot": true,
                    "mentions": ["wx-http"],
                    "raw_payload_summary": "{\"keys\":[\"conversation_id\",\"sender_id\"]}",
                    "context_token": null,
                    "external_msg_id": "http-msg-1",
                    "kind": "text",
                    "text": "来自 HTTP provider",
                    "media_refs": [],
                    "received_at_ms": 222
                }
            }]
        }))
    }

    async fn provider_send_text(
        State(capture): State<Arc<ProviderCapture>>,
        Json(payload): Json<Value>,
    ) -> axum::response::Response {
        capture
            .auth_headers
            .lock()
            .expect("auth lock")
            .push(Some(format!(
                "send_text:{}",
                payload["peer_id"].as_str().unwrap_or("")
            )));
        let status = capture
            .send_text_status
            .lock()
            .expect("status lock")
            .unwrap_or(200);
        if status >= 400 {
            (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                "发送失败 token=\"test-provider-token\"",
            )
                .into_response()
        } else {
            Json(json!({"provider_message_id": "provider-sent-1"})).into_response()
        }
    }

    async fn provider_send_file(
        State(capture): State<Arc<ProviderCapture>>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        capture
            .send_paths
            .lock()
            .expect("send paths lock")
            .push(format!(
                "send_file:{}:{}",
                payload["peer_id"].as_str().unwrap_or(""),
                payload["display_name"].as_str().unwrap_or("")
            ));
        Json(json!({"provider_message_id": "provider-file-1"}))
    }

    async fn test_provider(capture: Arc<ProviderCapture>) -> String {
        let app = Router::new()
            .route("/health", get(provider_health))
            .route("/login/refresh", post(provider_refresh_login))
            .route("/updates", get(provider_updates))
            .route("/send_text", post(provider_send_text))
            .route("/send_file", post(provider_send_file))
            .with_state(capture);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve test provider");
        });
        format!("http://{addr}")
    }

    fn outbox_item(id: i64) -> ClawbotOutboxItem {
        ClawbotOutboxItem {
            id,
            account_id: "mock-clawbot".to_string(),
            peer_id: "mock-peer".to_string(),
            context_token: None,
            source_external_msg_id: Some("in-1".to_string()),
            body: "回复文本".to_string(),
            file: None,
            state: OutboxState::Sending,
            attempts: 0,
            next_attempt_at_ms: 0,
            last_error: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn health_snapshot_reports_unconfigured_provider() {
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig::default(),
            MockClawbotProvider::default(),
        );

        let health = runtime.health_snapshot();

        assert!(!health.available);
        assert_eq!(health.provider.provider, "mock-clawbot");
        assert!(health
            .provider
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("尚未配置"));
    }

    #[test]
    fn redacts_provider_token_from_errors() {
        let secret = "secret-token-1234567890".to_string();
        let error = r#"request failed Authorization: Bearer secret-token-1234567890 token="secret-token-1234567890" cookie=secret-token-1234567890 password='secret-token-1234567890'"#;

        let redacted = redact_secret(error, &[secret.clone()]);

        assert!(!redacted.contains(&secret));
        assert!(redacted.contains("[redacted]"));
        assert!(redacted.contains("Bearer [redacted]"));
    }

    #[tokio::test]
    async fn http_provider_health_and_refresh_login_use_bearer_token() {
        let capture = Arc::new(ProviderCapture::default());
        let provider_url = test_provider(capture.clone()).await;
        let config = ClawbotSidecarConfig {
            account_id: "wx-http".to_string(),
            ..ClawbotSidecarConfig::default()
        };

        let (health, report) = tokio::task::spawn_blocking(move || {
            let provider = HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            });
            let health = provider.health(&config);
            let report = provider.refresh_login(9, 1_000).expect("refresh login");
            (health, report)
        })
        .await
        .expect("blocking provider task");

        assert_eq!(health.provider, "http-clawbot-test");
        assert!(health.online);
        assert_eq!(report.generation, 9);
        assert_eq!(report.state, ClawbotLoginState::AwaitingScan);
        assert!(report
            .qr_code_data_url
            .as_deref()
            .expect("qr")
            .starts_with("data:image/"));
        let auth_headers = capture.auth_headers.lock().expect("auth lock");
        assert_eq!(auth_headers.len(), 2);
        assert!(auth_headers
            .iter()
            .all(|value| value.as_deref() == Some("Bearer test-provider-token")));
    }

    #[tokio::test]
    async fn http_provider_polls_updates_and_sends_text() {
        let capture = Arc::new(ProviderCapture::default());
        let provider_url = test_provider(capture.clone()).await;
        let item = outbox_item(88);
        let (updates, receipt) = tokio::task::spawn_blocking(move || {
            let provider = HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            });
            let config = ClawbotSidecarConfig {
                account_id: "wx-http".to_string(),
                ..ClawbotSidecarConfig::default()
            };
            let updates = provider.poll_updates(&config, 1_234).expect("poll updates");
            let receipt = provider.send_text(&item).expect("send text");
            (updates, receipt)
        })
        .await
        .expect("blocking provider task");

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].message.external_msg_id, "http-msg-1");
        assert_eq!(
            updates[0].message.text.as_deref(),
            Some("来自 HTTP provider")
        );
        assert_eq!(
            updates[0].message.conversation_id.as_deref(),
            Some("group-http")
        );
        assert_eq!(updates[0].message.sender_id.as_deref(), Some("member-http"));
        assert!(updates[0].message.is_group);
        assert!(updates[0].message.mentioned_bot);
        assert_eq!(updates[0].message.mentions, vec!["wx-http"]);
        assert_eq!(receipt.provider_message_id, "provider-sent-1");
    }

    #[tokio::test]
    async fn http_provider_routes_file_outbox_to_send_file() {
        let capture = Arc::new(ProviderCapture::default());
        let provider_url = test_provider(capture.clone()).await;
        let mut item = outbox_item(90);
        item.file = Some(ClawbotOutboundFile {
            local_path: r"C:\workspace\Cargo.toml".to_string(),
            display_name: "Cargo.toml".to_string(),
            mime: Some("text/plain".to_string()),
            size_bytes: 28,
            checksum: Some("fnv64:1234".to_string()),
        });

        let receipt = tokio::task::spawn_blocking(move || {
            HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            })
            .send_message(&item)
            .expect("send file")
        })
        .await
        .expect("blocking provider task");

        assert_eq!(receipt.provider_message_id, "provider-file-1");
        assert_eq!(
            capture.send_paths.lock().expect("send paths lock").as_slice(),
            ["send_file:mock-peer:Cargo.toml"]
        );
    }

    #[tokio::test]
    async fn http_provider_allows_official_long_poll_to_exceed_ten_seconds() {
        let capture = Arc::new(ProviderCapture::default());
        capture.updates_delay_ms.store(11_000, Ordering::SeqCst);
        let provider_url = test_provider(capture).await;
        let updates = tokio::task::spawn_blocking(move || {
            let provider = HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: None,
            });
            provider
                .poll_updates(&ClawbotSidecarConfig::default(), 1_234)
                .expect("sidecar 不应在官方 35 秒长轮询完成前按普通 10 秒请求超时")
        })
        .await
        .expect("blocking provider task");

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].message.external_msg_id, "http-msg-1");
    }

    #[tokio::test]
    async fn http_provider_send_text_failure_is_redacted() {
        let capture = Arc::new(ProviderCapture::default());
        *capture.send_text_status.lock().expect("status lock") = Some(500);
        let provider_url = test_provider(capture).await;
        let item = outbox_item(89);

        let error = tokio::task::spawn_blocking(move || {
            let provider = HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            });
            provider.send_text(&item).expect_err("send should fail")
        })
        .await
        .expect("blocking provider task");

        assert!(error.contains("发送失败"), "{error}");
        assert!(!error.contains("test-provider-token"), "{error}");
        assert!(error.contains("[redacted]"), "{error}");
    }

    #[tokio::test]
    async fn provider_probe_reports_health_login_updates_and_optional_send() {
        let capture = Arc::new(ProviderCapture::default());
        let provider_url = test_provider(capture).await;
        let config = ClawbotSidecarConfig {
            account_id: "wx-http".to_string(),
            ..ClawbotSidecarConfig::default()
        };

        let report = tokio::task::spawn_blocking(move || {
            let provider = HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            });
            run_provider_probe(
                &provider,
                &config,
                ClawbotProviderProbeRequest {
                    generation: 42,
                    now_ms: 9_999,
                    send_peer_id: Some("peer-http".to_string()),
                    send_text: Some("探针发送文本".to_string()),
                },
            )
        })
        .await
        .expect("blocking probe task");

        assert!(report.ok, "{report:?}");
        assert_eq!(report.health.provider, "http-clawbot-test");
        assert_eq!(report.login_state, Some(ClawbotLoginState::AwaitingScan));
        assert_eq!(report.inbound_count, 1);
        assert_eq!(
            report.send_provider_message_id.as_deref(),
            Some("provider-sent-1")
        );
        assert!(report.steps.iter().all(|step| step.ok), "{report:?}");
    }

    #[tokio::test]
    async fn provider_probe_redacts_send_failure_without_leaking_token() {
        let capture = Arc::new(ProviderCapture::default());
        *capture.send_text_status.lock().expect("status lock") = Some(500);
        let provider_url = test_provider(capture).await;
        let config = ClawbotSidecarConfig {
            account_id: "wx-http".to_string(),
            ..ClawbotSidecarConfig::default()
        };

        let report = tokio::task::spawn_blocking(move || {
            let provider = HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            });
            run_provider_probe(
                &provider,
                &config,
                ClawbotProviderProbeRequest {
                    generation: 42,
                    now_ms: 9_999,
                    send_peer_id: Some("peer-http".to_string()),
                    send_text: Some("探针发送文本".to_string()),
                },
            )
        })
        .await
        .expect("blocking probe task");

        assert!(!report.ok, "{report:?}");
        let errors = report
            .steps
            .iter()
            .filter_map(|step| step.error.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(errors.contains("[redacted]"), "{errors}");
        assert!(!errors.contains("test-provider-token"), "{errors}");
    }

    #[test]
    fn provider_kind_from_env_selects_http_when_configured() {
        let values = [
            ("COOLZHU_CLAWBOT_PROVIDER_KIND", "http"),
            ("COOLZHU_CLAWBOT_PROVIDER_URL", "http://127.0.0.1:8790"),
            ("COOLZHU_CLAWBOT_PROVIDER_TOKEN", "provider-secret"),
        ];

        let selection = provider_selection_from_values(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        })
        .expect("provider selection");

        match selection {
            ProviderSelection::Http(config) => {
                assert_eq!(config.base_url, "http://127.0.0.1:8790");
                assert_eq!(config.bearer_token.as_deref(), Some("provider-secret"));
            }
            ProviderSelection::Mock => panic!("expected http provider"),
        }

        let invalid = provider_selection_from_values(|key| {
            (key == "COOLZHU_CLAWBOT_PROVIDER_KIND").then(|| "unknown".to_string())
        })
        .expect_err("unknown provider kind should fail");
        assert!(invalid.contains("未知 ClawBot provider kind"));
    }

    #[test]
    fn provider_probe_request_from_values_parses_optional_send_target() {
        let values = [
            ("COOLZHU_CLAWBOT_PROBE_GENERATION", "88"),
            ("COOLZHU_CLAWBOT_PROBE_NOW_MS", "123456"),
            ("COOLZHU_CLAWBOT_PROBE_PEER_ID", "peer-http"),
            ("COOLZHU_CLAWBOT_PROBE_TEXT", "真实探针文本"),
        ];

        let request = provider_probe_request_from_values(
            |key| {
                values
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| (*value).to_string())
            },
            9_999,
        );

        assert_eq!(request.generation, 88);
        assert_eq!(request.now_ms, 123456);
        assert_eq!(request.send_peer_id.as_deref(), Some("peer-http"));
        assert_eq!(request.send_text.as_deref(), Some("真实探针文本"));

        let defaulted = provider_probe_request_from_values(|_| None, 9_999);
        assert_eq!(defaulted.generation, 1);
        assert_eq!(defaulted.now_ms, 9_999);
        assert!(defaulted.send_peer_id.is_none());
        assert!(defaulted.send_text.is_none());
    }

    #[tokio::test]
    async fn refresh_login_posts_generation_report_to_web_console() {
        let capture = Arc::new(GatewayCapture::default());
        let gateway_base_url = test_gateway(capture.clone()).await;
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            MockClawbotProvider::configured_online(),
        );

        runtime
            .refresh_login(7, 1_000)
            .await
            .expect("refresh login");

        let requests = capture.requests.lock().expect("capture lock");
        let (_, payload) = requests
            .iter()
            .find(|(kind, _)| kind == "login_report")
            .expect("login report captured");
        assert_eq!(payload["generation"], 7);
        assert_eq!(payload["state"], "awaiting_scan");
        assert!(payload["qr_code_data_url"]
            .as_str()
            .expect("qr data url")
            .starts_with("data:image/"));
    }

    #[tokio::test]
    async fn refresh_login_with_http_provider_does_not_panic_inside_async_runtime() {
        let provider_capture = Arc::new(ProviderCapture::default());
        let provider_url = test_provider(provider_capture).await;
        let gateway_capture = Arc::new(GatewayCapture::default());
        let gateway_base_url = test_gateway(gateway_capture.clone()).await;
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                account_id: "wx-http".to_string(),
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            HttpClawbotProvider::new(HttpClawbotProviderConfig {
                base_url: provider_url,
                bearer_token: Some("test-provider-token".to_string()),
            }),
        );

        let report = runtime
            .refresh_login(7, 1_000)
            .await
            .expect("http provider refresh should finish without runtime-drop panic");

        assert_eq!(report.generation, 7);
        assert_eq!(report.state, ClawbotLoginState::AwaitingScan);
        let requests = gateway_capture.requests.lock().expect("capture lock");
        assert!(requests.iter().any(|(kind, _)| kind == "login_report"));
    }

    #[tokio::test]
    async fn tick_dispatches_inbound_and_acks_outbox() {
        let capture = Arc::new(GatewayCapture::default());
        capture
            .outbox
            .lock()
            .expect("outbox lock")
            .push(outbox_item(42));
        let gateway_base_url = test_gateway(capture.clone()).await;
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            MockClawbotProvider::configured_online(),
        );

        let summary = runtime.tick(1_234).await.expect("tick");

        assert_eq!(summary.inbound_dispatched, 1);
        assert_eq!(summary.outbox_sent, 1);
        let requests = capture.requests.lock().expect("capture lock");
        assert!(requests.iter().any(|(kind, _)| kind == "inbound_dispatch"));
        assert!(requests.iter().any(|(kind, _)| kind == "outbox_ack:42"));
    }

    #[tokio::test]
    async fn tick_allows_model_dispatch_to_exceed_ten_seconds() {
        let capture = Arc::new(GatewayCapture::default());
        capture.inbound_delay_ms.store(11_000, Ordering::SeqCst);
        let gateway_base_url = test_gateway(capture.clone()).await;
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            MockClawbotProvider::configured_online(),
        );

        let summary = runtime
            .tick(1_234)
            .await
            .expect("真实模型调度超过十秒时仍应完成入站转发");

        assert_eq!(summary.inbound_dispatched, 1);
        let requests = capture.requests.lock().expect("capture lock");
        assert!(requests.iter().any(|(kind, _)| kind == "inbound_dispatch"));
    }

    #[tokio::test]
    async fn tick_marks_outbox_failed_when_provider_send_fails() {
        let capture = Arc::new(GatewayCapture::default());
        capture
            .outbox
            .lock()
            .expect("outbox lock")
            .push(outbox_item(43));
        let gateway_base_url = test_gateway(capture.clone()).await;
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            MockClawbotProvider::configured_with_send_failure(),
        );

        let summary = runtime.tick(1_234).await.expect("tick");

        assert_eq!(summary.outbox_failed, 1);
        let requests = capture.requests.lock().expect("capture lock");
        let (_, payload) = requests
            .iter()
            .find(|(kind, _)| kind == "outbox_fail:43")
            .expect("fail captured");
        assert!(payload["error"]
            .as_str()
            .expect("error text")
            .contains("发送失败"));
    }

    #[tokio::test]
    async fn polling_loop_runs_tick_without_http_request() {
        let capture = Arc::new(GatewayCapture::default());
        let gateway_base_url = test_gateway(capture.clone()).await;
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                poll_interval_ms: 10,
                ..ClawbotSidecarConfig::default()
            },
            MockClawbotProvider::configured_online(),
        );

        let polling = spawn_polling_loop(runtime);
        tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                let inbound_count = capture
                    .requests
                    .lock()
                    .expect("capture lock")
                    .iter()
                    .filter(|(kind, _)| kind == "inbound_dispatch")
                    .count();
                if inbound_count >= 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("后台轮询应在无需调用 /tick 时持续分发消息");
        polling.abort();
    }

    #[derive(Clone)]
    struct TogglePollProvider {
        fail_poll: Arc<AtomicBool>,
    }

    impl ClawbotProvider for TogglePollProvider {
        fn health(&self, config: &ClawbotSidecarConfig) -> ClawbotProviderHealth {
            ClawbotProviderHealth {
                provider: "toggle-provider".to_string(),
                provider_version: "test".to_string(),
                account_id: Some(config.account_id.clone()),
                online: true,
                last_error: None,
            }
        }

        fn refresh_login(
            &self,
            generation: u64,
            _now_ms: u64,
        ) -> Result<ClawbotLoginReport, String> {
            Ok(ClawbotLoginReport {
                generation,
                account_id: Some("toggle-account".to_string()),
                state: ClawbotLoginState::Online,
                qr_code_data_url: None,
                expires_at_ms: None,
                last_error: None,
            })
        }

        fn logout(&self) -> Result<(), String> {
            Ok(())
        }

        fn poll_updates(
            &self,
            _config: &ClawbotSidecarConfig,
            _now_ms: u64,
        ) -> Result<Vec<ClawbotInboundEnvelope>, String> {
            if self.fail_poll.load(Ordering::SeqCst) {
                Err("测试轮询失败".to_string())
            } else {
                Ok(Vec::new())
            }
        }

        fn send_text(&self, _item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
            Ok(ProviderSendReceipt {
                provider_message_id: "unused".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn tick_error_is_reported_by_health_and_cleared_after_recovery() {
        let capture = Arc::new(GatewayCapture::default());
        let gateway_base_url = test_gateway(capture).await;
        let fail_poll = Arc::new(AtomicBool::new(true));
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            TogglePollProvider {
                fail_poll: fail_poll.clone(),
            },
        );

        runtime.tick(1).await.expect_err("首次轮询应失败");
        let failed_health = runtime.health_snapshot();
        assert!(!failed_health.available);
        assert_eq!(
            failed_health.last_tick_error.as_deref(),
            Some("测试轮询失败")
        );

        fail_poll.store(false, Ordering::SeqCst);
        runtime.tick(2).await.expect("恢复后的轮询应成功");
        let recovered_health = runtime.health_snapshot();
        assert!(recovered_health.available);
        assert!(recovered_health.last_tick_error.is_none());
    }

    #[derive(Clone)]
    struct SlowPollProvider {
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    }

    impl ClawbotProvider for SlowPollProvider {
        fn health(&self, config: &ClawbotSidecarConfig) -> ClawbotProviderHealth {
            ClawbotProviderHealth {
                provider: "slow-provider".to_string(),
                provider_version: "test".to_string(),
                account_id: Some(config.account_id.clone()),
                online: true,
                last_error: None,
            }
        }

        fn refresh_login(
            &self,
            generation: u64,
            _now_ms: u64,
        ) -> Result<ClawbotLoginReport, String> {
            Ok(ClawbotLoginReport {
                generation,
                account_id: Some("slow-account".to_string()),
                state: ClawbotLoginState::Online,
                qr_code_data_url: None,
                expires_at_ms: None,
                last_error: None,
            })
        }

        fn logout(&self) -> Result<(), String> {
            Ok(())
        }

        fn poll_updates(
            &self,
            _config: &ClawbotSidecarConfig,
            _now_ms: u64,
        ) -> Result<Vec<ClawbotInboundEnvelope>, String> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(Vec::new())
        }

        fn send_text(&self, _item: &ClawbotOutboxItem) -> Result<ProviderSendReceipt, String> {
            Ok(ProviderSendReceipt {
                provider_message_id: "unused".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn concurrent_manual_and_background_ticks_are_serialized() {
        let capture = Arc::new(GatewayCapture::default());
        let gateway_base_url = test_gateway(capture).await;
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let runtime = SidecarRuntime::new(
            ClawbotSidecarConfig {
                gateway_base_url,
                ..ClawbotSidecarConfig::default()
            },
            SlowPollProvider {
                active,
                max_active: max_active.clone(),
            },
        );

        let (first, second) = tokio::join!(runtime.tick(1), runtime.tick(2));
        first.expect("first tick");
        second.expect("second tick");
        assert_eq!(
            max_active.load(Ordering::SeqCst),
            1,
            "provider getupdates 不得并发，避免 cursor 竞态与重复消费"
        );
    }
}
