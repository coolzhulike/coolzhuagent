use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clawbot_channel::ClawbotConversationBinding;
use crate::wechat_authorization::{WechatCapabilitySet, WechatMemberGrant, WechatMemberPreset};
use crate::wechat_group::{
    WechatGroupDetachAudit, WechatGroupLifecycleState, WechatGroupRecord, WechatObservedContact,
    WechatOperationAdministrator,
};

const OUTBOX_CLAIM_LEASE_MS: u64 = 30_000;

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
pub struct ClawbotLoginSnapshot {
    pub state: ClawbotLoginState,
    pub account_id: Option<String>,
    pub qr_code_data_url: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub last_error: Option<String>,
    pub generation: u64,
    pub updated_at_ms: u64,
}

impl Default for ClawbotLoginSnapshot {
    fn default() -> Self {
        Self {
            state: ClawbotLoginState::LoggedOut,
            account_id: None,
            qr_code_data_url: None,
            expires_at_ms: None,
            last_error: None,
            generation: 0,
            updated_at_ms: 0,
        }
    }
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

impl ClawbotLoginSnapshot {
    pub fn request_refresh(&mut self, now_ms: u64) {
        self.generation = self.generation.saturating_add(1);
        self.state = ClawbotLoginState::RefreshRequested;
        self.qr_code_data_url = None;
        self.expires_at_ms = None;
        self.last_error = None;
        self.updated_at_ms = now_ms;
    }

    pub fn report(&mut self, report: ClawbotLoginReport, now_ms: u64) -> Result<(), String> {
        if report.generation != self.generation {
            return Err(format!(
                "ClawBot 登录报告 generation={} 已过期，当前 generation={}",
                report.generation, self.generation
            ));
        }
        if report.state == ClawbotLoginState::AwaitingScan {
            let valid_qr = report
                .qr_code_data_url
                .as_deref()
                .is_some_and(is_valid_login_qr_value);
            if !valid_qr {
                return Err(
                    "等待扫码状态必须包含 data:image/ 二维码或微信 LiteApp 扫码链接".to_string(),
                );
            }
        }

        self.state = report.state;
        self.account_id = report.account_id;
        self.qr_code_data_url = report.qr_code_data_url;
        self.expires_at_ms = report.expires_at_ms;
        self.last_error = report.last_error;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub fn effective_state(&self, now_ms: u64) -> ClawbotLoginState {
        if self.state == ClawbotLoginState::AwaitingScan
            && self
                .expires_at_ms
                .is_some_and(|expires_at_ms| expires_at_ms <= now_ms)
        {
            ClawbotLoginState::Expired
        } else {
            self.state.clone()
        }
    }

    pub fn logout(&mut self, now_ms: u64) {
        self.state = ClawbotLoginState::LoggedOut;
        self.account_id = None;
        self.qr_code_data_url = None;
        self.expires_at_ms = None;
        self.last_error = None;
        self.updated_at_ms = now_ms;
    }
}

fn is_valid_login_qr_value(value: &str) -> bool {
    value.starts_with("data:image/") || value.starts_with("https://liteapp.weixin.qq.com/q/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClawbotInboxInput {
    pub account_id: String,
    pub external_msg_id: String,
    pub peer_id: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxState {
    Received,
    Dispatching,
    Completed,
    Rejected,
    Failed,
}

impl InboxState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Dispatching => "dispatching",
            Self::Completed => "completed",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Result<Self, String> {
        match value {
            "received" => Ok(Self::Received),
            "dispatching" => Ok(Self::Dispatching),
            "completed" => Ok(Self::Completed),
            "rejected" => Ok(Self::Rejected),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("未知 ClawBot inbox 状态：{value}")),
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Rejected | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WechatRequestState {
    Received,
    Authorized,
    Running,
    AwaitingApproval,
    Succeeded,
    Denied,
    Cancelled,
    Failed,
}

impl WechatRequestState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Authorized => "authorized",
            Self::Running => "running",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Succeeded => "succeeded",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Result<Self, String> {
        match value {
            "received" => Ok(Self::Received),
            "authorized" => Ok(Self::Authorized),
            "running" => Ok(Self::Running),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "succeeded" => Ok(Self::Succeeded),
            "denied" => Ok(Self::Denied),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("未知微信请求状态：{value}")),
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Denied | Self::Cancelled | Self::Failed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatArtifactRef {
    pub id: String,
    pub display_name: String,
    pub path: Option<String>,
    pub mime: Option<String>,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatCommandResult {
    pub request_id: String,
    pub state: WechatRequestState,
    pub code: String,
    pub summary: String,
    pub details: Vec<String>,
    pub artifacts: Vec<WechatArtifactRef>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotInboxItem {
    pub id: i64,
    pub account_id: String,
    pub external_msg_id: String,
    pub peer_id: String,
    pub payload_json: String,
    pub state: InboxState,
    pub request_state: WechatRequestState,
    pub error_code: Option<String>,
    pub request_id: String,
    pub parent_request_id: Option<String>,
    pub operation_fingerprint: Option<String>,
    pub result_json: Option<String>,
    pub last_error: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxRegistration {
    Accepted(ClawbotInboxItem),
    Duplicate(ClawbotInboxItem),
    Conflict(ClawbotInboxItem),
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
pub struct ClawbotOutboundMessage {
    pub account_id: String,
    pub peer_id: String,
    pub context_token: Option<String>,
    pub source_external_msg_id: Option<String>,
    pub body: String,
    #[serde(default)]
    pub file: Option<ClawbotOutboundFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxState {
    Pending,
    Sending,
    Sent,
    DeadLetter,
}

impl OutboxState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::DeadLetter => "dead_letter",
        }
    }

    fn from_db(value: &str) -> Result<Self, String> {
        match value {
            "pending" => Ok(Self::Pending),
            "sending" => Ok(Self::Sending),
            "sent" => Ok(Self::Sent),
            "dead_letter" => Ok(Self::DeadLetter),
            _ => Err(format!("未知 ClawBot outbox 状态：{value}")),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1_000,
            max_delay_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotGatewayMetrics {
    pub inbox_received: u64,
    pub inbox_dispatching: u64,
    pub inbox_completed: u64,
    pub inbox_rejected: u64,
    pub inbox_failed: u64,
    pub outbox_pending: u64,
    pub outbox_sending: u64,
    pub outbox_sent: u64,
    pub outbox_dead_letter: u64,
}

pub struct ClawbotGatewayStore {
    connection: Connection,
}

impl ClawbotGatewayStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "创建 ClawBot gateway 数据目录失败：{} ({error})",
                    parent.display()
                )
            })?;
        }
        let connection = Connection::open(path).map_err(|error| {
            format!(
                "打开 ClawBot gateway 数据库失败：{} ({error})",
                path.display()
            )
        })?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let connection = Connection::open_in_memory()
            .map_err(|error| format!("打开 ClawBot 内存数据库失败：{error}"))?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> Result<(), String> {
        self.connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS clawbot_login_snapshot (
                    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                    snapshot_json TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS clawbot_inbox (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_id TEXT NOT NULL,
                    external_msg_id TEXT NOT NULL,
                    peer_id TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    state TEXT NOT NULL,
                    request_state TEXT NOT NULL DEFAULT 'received',
                    error_code TEXT,
                    request_id TEXT,
                    parent_request_id TEXT,
                    operation_fingerprint TEXT,
                    result_json TEXT,
                    last_error TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    UNIQUE(account_id, external_msg_id)
                );
                CREATE INDEX IF NOT EXISTS idx_clawbot_inbox_state_updated
                    ON clawbot_inbox(state, updated_at_ms);
                CREATE TABLE IF NOT EXISTS clawbot_outbox (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_id TEXT NOT NULL,
                    peer_id TEXT NOT NULL,
                    context_token TEXT,
                    source_external_msg_id TEXT,
                    body TEXT NOT NULL,
                    file_json TEXT,
                    state TEXT NOT NULL,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    next_attempt_at_ms INTEGER NOT NULL,
                    last_error TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_clawbot_outbox_due
                    ON clawbot_outbox(state, next_attempt_at_ms, id);
                CREATE TABLE IF NOT EXISTS clawbot_group_member_grants (
                    account_id TEXT NOT NULL,
                    group_id TEXT NOT NULL,
                    member_id TEXT NOT NULL,
                    grant_json TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(account_id, group_id, member_id)
                );
                CREATE INDEX IF NOT EXISTS idx_clawbot_group_member_grants_group
                    ON clawbot_group_member_grants(account_id, group_id, enabled, member_id);
                CREATE TABLE IF NOT EXISTS clawbot_contacts (
                    account_id TEXT NOT NULL,
                    peer_id TEXT NOT NULL,
                    contact_json TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(account_id, peer_id)
                );
                CREATE TABLE IF NOT EXISTS clawbot_operation_administrators (
                    account_id TEXT PRIMARY KEY,
                    administrator_json TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS clawbot_groups (
                    account_id TEXT NOT NULL,
                    group_id TEXT NOT NULL,
                    group_json TEXT NOT NULL,
                    lifecycle_state TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(account_id, group_id)
                );
                CREATE INDEX IF NOT EXISTS idx_clawbot_groups_account_state
                    ON clawbot_groups(account_id, lifecycle_state, updated_at_ms DESC);
                CREATE TABLE IF NOT EXISTS clawbot_group_audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_id TEXT NOT NULL,
                    group_id TEXT NOT NULL,
                    audit_json TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_clawbot_group_audit_group
                    ON clawbot_group_audit(account_id, group_id, id);
                CREATE TABLE IF NOT EXISTS clawbot_denial_notices (
                    account_id TEXT NOT NULL,
                    group_id TEXT NOT NULL,
                    member_id TEXT NOT NULL,
                    code TEXT NOT NULL,
                    last_notice_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(account_id, group_id, member_id, code)
                );
                "#,
            )
            .map_err(|error| format!("初始化 ClawBot gateway 数据库失败：{error}"))?;
        self.ensure_inbox_request_tracking_schema()?;
        self.ensure_outbox_file_schema()
    }

    fn ensure_inbox_request_tracking_schema(&self) -> Result<(), String> {
        for (column, definition) in [
            (
                "request_state",
                "ALTER TABLE clawbot_inbox ADD COLUMN request_state TEXT",
            ),
            (
                "error_code",
                "ALTER TABLE clawbot_inbox ADD COLUMN error_code TEXT",
            ),
            (
                "request_id",
                "ALTER TABLE clawbot_inbox ADD COLUMN request_id TEXT",
            ),
            (
                "parent_request_id",
                "ALTER TABLE clawbot_inbox ADD COLUMN parent_request_id TEXT",
            ),
            (
                "operation_fingerprint",
                "ALTER TABLE clawbot_inbox ADD COLUMN operation_fingerprint TEXT",
            ),
        ] {
            if !self.table_has_column("clawbot_inbox", column)? {
                self.connection
                    .execute(definition, [])
                    .map_err(|error| format!("迁移 ClawBot inbox 字段 {column} 失败：{error}"))?;
            }
        }
        self.connection
            .execute_batch(
                r#"
                UPDATE clawbot_inbox
                   SET request_state = CASE state
                        WHEN 'received' THEN 'received'
                        WHEN 'dispatching' THEN 'running'
                        WHEN 'completed' THEN 'succeeded'
                        WHEN 'rejected' THEN 'denied'
                        WHEN 'failed' THEN 'failed'
                        ELSE 'failed'
                   END
                 WHERE request_state IS NULL OR trim(request_state) = '';
                UPDATE clawbot_inbox
                   SET request_id = 'wxreq:' || account_id || ':' || external_msg_id
                 WHERE request_id IS NULL OR trim(request_id) = '';
                UPDATE clawbot_inbox
                   SET operation_fingerprint = 'inbound:' || account_id || ':' || peer_id || ':' || external_msg_id
                 WHERE operation_fingerprint IS NULL OR trim(operation_fingerprint) = '';
                CREATE INDEX IF NOT EXISTS idx_clawbot_inbox_request_state_updated
                    ON clawbot_inbox(request_state, updated_at_ms);
                CREATE INDEX IF NOT EXISTS idx_clawbot_inbox_request_id
                    ON clawbot_inbox(account_id, request_id);
                CREATE INDEX IF NOT EXISTS idx_clawbot_inbox_operation_fingerprint
                    ON clawbot_inbox(account_id, operation_fingerprint, updated_at_ms);
                "#,
            )
            .map_err(|error| format!("补齐 ClawBot inbox 请求追踪字段失败：{error}"))?;
        Ok(())
    }

    fn ensure_outbox_file_schema(&self) -> Result<(), String> {
        if !self.table_has_column("clawbot_outbox", "file_json")? {
            self.connection
                .execute("ALTER TABLE clawbot_outbox ADD COLUMN file_json TEXT", [])
                .map_err(|error| format!("迁移 ClawBot outbox 文件字段失败：{error}"))?;
        }
        Ok(())
    }

    fn table_has_column(&self, table: &str, column: &str) -> Result<bool, String> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|error| format!("读取 ClawBot 表 {table} 结构失败：{error}"))?;
        let mut rows = statement
            .query([])
            .map_err(|error| format!("查询 ClawBot 表 {table} 结构失败：{error}"))?;
        while let Some(row) = rows
            .next()
            .map_err(|error| format!("遍历 ClawBot 表 {table} 结构失败：{error}"))?
        {
            let name: String = row
                .get(1)
                .map_err(|error| format!("解析 ClawBot 表 {table} 字段失败：{error}"))?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn load_login_snapshot(&self) -> Result<ClawbotLoginSnapshot, String> {
        let json = self
            .connection
            .query_row(
                "SELECT snapshot_json FROM clawbot_login_snapshot WHERE singleton_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取 ClawBot 登录快照失败：{error}"))?;
        json.map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| format!("解析 ClawBot 登录快照失败：{error}"))
        })
        .transpose()
        .map(|snapshot| snapshot.unwrap_or_default())
    }

    pub fn save_login_snapshot(&self, snapshot: &ClawbotLoginSnapshot) -> Result<(), String> {
        let json = serde_json::to_string(snapshot)
            .map_err(|error| format!("序列化 ClawBot 登录快照失败：{error}"))?;
        self.connection
            .execute(
                "INSERT INTO clawbot_login_snapshot(singleton_id, snapshot_json, updated_at_ms) \
                 VALUES (1, ?1, ?2) \
                 ON CONFLICT(singleton_id) DO UPDATE SET \
                   snapshot_json = excluded.snapshot_json, \
                   updated_at_ms = excluded.updated_at_ms",
                params![json, u64_to_i64(snapshot.updated_at_ms)],
            )
            .map_err(|error| format!("保存 ClawBot 登录快照失败：{error}"))?;
        Ok(())
    }

    pub fn register_inbox(
        &self,
        input: &ClawbotInboxInput,
        now_ms: u64,
    ) -> Result<InboxRegistration, String> {
        if input.account_id.trim().is_empty()
            || input.external_msg_id.trim().is_empty()
            || input.peer_id.trim().is_empty()
        {
            return Err(
                "ClawBot inbox 的 account_id、external_msg_id、peer_id 不能为空".to_string(),
            );
        }
        if let Some(existing) = self.inbox_by_key(&input.account_id, &input.external_msg_id)? {
            return Ok(if existing.payload_json == input.payload_json {
                InboxRegistration::Duplicate(existing)
            } else {
                InboxRegistration::Conflict(existing)
            });
        }
        let request_id = request_id_for_input(input);
        let operation_fingerprint = operation_fingerprint_for_input(input);
        self.connection
            .execute(
                "INSERT INTO clawbot_inbox(\
                    account_id, external_msg_id, peer_id, payload_json, state, \
                    request_state, error_code, request_id, parent_request_id, operation_fingerprint, \
                    result_json, last_error, created_at_ms, updated_at_ms\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, ?8, NULL, NULL, ?9, ?9)",
                params![
                    input.account_id,
                    input.external_msg_id,
                    input.peer_id,
                    input.payload_json,
                    InboxState::Received.as_str(),
                    WechatRequestState::Received.as_str(),
                    request_id,
                    operation_fingerprint,
                    u64_to_i64(now_ms),
                ],
            )
            .map_err(|error| format!("登记 ClawBot inbox 失败：{error}"))?;
        let item = self.inbox_by_id(self.connection.last_insert_rowid())?;
        Ok(InboxRegistration::Accepted(item))
    }

    pub fn inbox_item(&self, id: i64) -> Result<ClawbotInboxItem, String> {
        self.inbox_by_id(id)
    }

    pub fn set_inbox_operation_fingerprint(
        &self,
        id: i64,
        operation_fingerprint: &str,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        let fingerprint = operation_fingerprint.trim();
        if fingerprint.is_empty() {
            return Err("ClawBot inbox operation_fingerprint 不能为空".to_string());
        }
        let current = self.inbox_by_id(id)?;
        if current.state.is_terminal() || current.request_state.is_terminal() {
            return Ok(current);
        }
        self.connection
            .execute(
                "UPDATE clawbot_inbox SET operation_fingerprint = ?1, updated_at_ms = ?2 \
                 WHERE id = ?3",
                params![fingerprint, u64_to_i64(now_ms), id],
            )
            .map_err(|error| format!("更新 ClawBot inbox #{id} 业务指纹失败：{error}"))?;
        self.inbox_by_id(id)
    }

    pub fn recent_failed_operation(
        &self,
        account_id: &str,
        operation_fingerprint: &str,
        excluding_inbox_id: i64,
        since_ms: u64,
    ) -> Result<Option<ClawbotInboxItem>, String> {
        let fingerprint = operation_fingerprint.trim();
        if account_id.trim().is_empty() || fingerprint.is_empty() {
            return Ok(None);
        }
        let row = self
            .connection
            .query_row(
                "SELECT id, account_id, external_msg_id, peer_id, payload_json, state, \
                 request_state, error_code, request_id, parent_request_id, operation_fingerprint, \
                 result_json, last_error, created_at_ms, updated_at_ms \
                 FROM clawbot_inbox \
                 WHERE account_id = ?1 \
                   AND operation_fingerprint = ?2 \
                   AND id <> ?3 \
                   AND updated_at_ms >= ?4 \
                   AND COALESCE(error_code, '') <> 'repeated_failure_guard' \
                   AND (request_state IN ('failed', 'denied') OR state IN ('failed', 'rejected')) \
                 ORDER BY updated_at_ms DESC, id DESC \
                 LIMIT 1",
                params![
                    account_id,
                    fingerprint,
                    excluding_inbox_id,
                    u64_to_i64(since_ms),
                ],
                inbox_row,
            )
            .optional()
            .map_err(|error| format!("查询 ClawBot 重复失败业务指纹失败：{error}"))?;
        row.map(inbox_item_from_row).transpose()
    }

    pub fn mark_inbox_dispatching(&self, id: i64, now_ms: u64) -> Result<ClawbotInboxItem, String> {
        self.update_inbox(id, InboxState::Dispatching, None, None, now_ms)
    }

    pub fn mark_inbox_authorized(&self, id: i64, now_ms: u64) -> Result<ClawbotInboxItem, String> {
        self.update_inbox_request_state(
            id,
            InboxState::Received,
            WechatRequestState::Authorized,
            None,
            None,
            now_ms,
        )
    }

    pub fn mark_inbox_awaiting_approval(
        &self,
        id: i64,
        result_json: &str,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        serde_json::from_str::<serde_json::Value>(result_json)
            .map_err(|error| format!("ClawBot inbox result_json 不是有效 JSON：{error}"))?;
        self.update_inbox_request_state(
            id,
            InboxState::Dispatching,
            WechatRequestState::AwaitingApproval,
            Some(result_json),
            None,
            now_ms,
        )
    }

    pub fn mark_inbox_completed(
        &self,
        id: i64,
        result_json: &str,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        serde_json::from_str::<serde_json::Value>(result_json)
            .map_err(|error| format!("ClawBot inbox result_json 不是有效 JSON：{error}"))?;
        self.update_inbox(id, InboxState::Completed, Some(result_json), None, now_ms)
    }

    pub fn mark_inbox_rejected(
        &self,
        id: i64,
        error: &str,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        let code = error_code_from_feedback(error);
        let result_json =
            self.terminal_result_json(id, WechatRequestState::Denied, &code, error, false)?;
        self.update_inbox(
            id,
            InboxState::Rejected,
            Some(&result_json),
            Some(error),
            now_ms,
        )
    }

    pub fn mark_inbox_failed(
        &self,
        id: i64,
        error: &str,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        let code = error_code_from_feedback(error);
        let result_json =
            self.terminal_result_json(id, WechatRequestState::Failed, &code, error, false)?;
        self.update_inbox(
            id,
            InboxState::Failed,
            Some(&result_json),
            Some(error),
            now_ms,
        )
    }

    pub fn mark_inbox_cancelled(
        &self,
        id: i64,
        error: &str,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        let code = error_code_from_feedback(error);
        let result_json =
            self.terminal_result_json(id, WechatRequestState::Cancelled, &code, error, false)?;
        self.update_inbox_request_state(
            id,
            InboxState::Failed,
            WechatRequestState::Cancelled,
            Some(&result_json),
            Some(error),
            now_ms,
        )
    }

    fn terminal_result_json(
        &self,
        id: i64,
        state: WechatRequestState,
        code: &str,
        summary: &str,
        retryable: bool,
    ) -> Result<String, String> {
        let current = self.inbox_by_id(id)?;
        serde_json::to_string(&WechatCommandResult {
            request_id: current.request_id,
            state,
            code: code.to_string(),
            summary: summary.to_string(),
            details: Vec::new(),
            artifacts: Vec::new(),
            retryable,
        })
        .map_err(|error| format!("序列化微信命令结果失败：{error}"))
    }

    pub fn enqueue_outbox(
        &self,
        message: &ClawbotOutboundMessage,
        now_ms: u64,
    ) -> Result<i64, String> {
        if message.account_id.trim().is_empty() || message.peer_id.trim().is_empty() {
            return Err("ClawBot outbox 的 account_id、peer_id 不能为空".to_string());
        }
        if message.body.trim().is_empty() && message.file.is_none() {
            return Err("ClawBot outbox 的文本和文件不能同时为空".to_string());
        }
        if let Some(file) = &message.file {
            if file.local_path.trim().is_empty() || file.display_name.trim().is_empty() {
                return Err("ClawBot outbox 文件的 local_path、display_name 不能为空".to_string());
            }
        }
        let file_json = message
            .file
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| format!("序列化 ClawBot outbox 文件失败：{error}"))?;
        self.connection
            .execute(
                "INSERT INTO clawbot_outbox(\
                    account_id, peer_id, context_token, source_external_msg_id, body, file_json, state, \
                    attempts, next_attempt_at_ms, last_error, created_at_ms, updated_at_ms\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, NULL, ?8, ?8)",
                params![
                    message.account_id,
                    message.peer_id,
                    message.context_token,
                    message.source_external_msg_id,
                    message.body,
                    file_json,
                    OutboxState::Pending.as_str(),
                    u64_to_i64(now_ms),
                ],
            )
            .map_err(|error| format!("写入 ClawBot outbox 失败：{error}"))?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn mark_outbox_failed(
        &self,
        id: i64,
        error: &str,
        now_ms: u64,
        policy: RetryPolicy,
    ) -> Result<ClawbotOutboxItem, String> {
        let current = self.outbox_by_id(id)?;
        if matches!(current.state, OutboxState::Sent | OutboxState::DeadLetter) {
            return Ok(current);
        }
        let attempts = current.attempts.saturating_add(1);
        let max_attempts = policy.max_attempts.max(1);
        let (state, next_attempt_at_ms) = if attempts >= max_attempts {
            (OutboxState::DeadLetter, now_ms)
        } else {
            let shift = attempts.saturating_sub(1).min(31);
            let factor = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
            let delay = policy
                .base_delay_ms
                .saturating_mul(factor)
                .min(policy.max_delay_ms.max(policy.base_delay_ms));
            (OutboxState::Pending, now_ms.saturating_add(delay))
        };
        self.connection
            .execute(
                "UPDATE clawbot_outbox SET state = ?1, attempts = ?2, \
                 next_attempt_at_ms = ?3, last_error = ?4, updated_at_ms = ?5 WHERE id = ?6",
                params![
                    state.as_str(),
                    i64::from(attempts),
                    u64_to_i64(next_attempt_at_ms),
                    error,
                    u64_to_i64(now_ms),
                    id,
                ],
            )
            .map_err(|db_error| format!("更新 ClawBot outbox 失败状态失败：{db_error}"))?;
        self.outbox_by_id(id)
    }

    pub fn mark_outbox_sent(&self, id: i64, now_ms: u64) -> Result<ClawbotOutboxItem, String> {
        let current = self.outbox_by_id(id)?;
        if current.state == OutboxState::Sent {
            return Ok(current);
        }
        if current.state != OutboxState::Sending {
            return Err(format!(
                "ClawBot outbox #{id} 当前状态 {:?}，不能确认发送",
                current.state
            ));
        }
        self.connection
            .execute(
                "UPDATE clawbot_outbox SET state = 'sent', last_error = NULL, updated_at_ms = ?1 \
                 WHERE id = ?2 AND state = 'sending'",
                params![u64_to_i64(now_ms), id],
            )
            .map_err(|error| format!("确认 ClawBot outbox #{id} 发送成功失败：{error}"))?;
        self.outbox_by_id(id)
    }

    pub fn claim_due_outbox(
        &self,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<ClawbotOutboxItem>, String> {
        let limit = limit.clamp(1, 100);
        let stale_sending_before_ms = now_ms.saturating_sub(OUTBOX_CLAIM_LEASE_MS);
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("开始 ClawBot outbox claim 事务失败：{error}"))?;
        let retry_policy = RetryPolicy::default();
        transaction
            .execute(
                "UPDATE clawbot_outbox SET \
                   attempts = attempts + 1, \
                   state = CASE WHEN attempts + 1 >= ?1 THEN 'dead_letter' ELSE 'pending' END, \
                   next_attempt_at_ms = ?2, \
                   last_error = 'sidecar claim lease expired without ack/fail', \
                   updated_at_ms = ?2 \
                 WHERE state = 'sending' AND updated_at_ms <= ?3",
                params![
                    i64::from(retry_policy.max_attempts.max(1)),
                    u64_to_i64(now_ms),
                    u64_to_i64(stale_sending_before_ms),
                ],
            )
            .map_err(|error| format!("回收 ClawBot outbox 过期发送租约失败：{error}"))?;
        let ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT id FROM clawbot_outbox \
                     WHERE state = 'pending' AND next_attempt_at_ms <= ?1 \
                     ORDER BY next_attempt_at_ms, id LIMIT ?2",
                )
                .map_err(|error| format!("准备 ClawBot outbox claim 查询失败：{error}"))?;
            let rows = statement
                .query_map(params![u64_to_i64(now_ms), limit as i64], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|error| format!("查询 ClawBot outbox claim 失败：{error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("读取 ClawBot outbox claim 失败：{error}"))?;
            rows
        };
        for id in &ids {
            transaction
                .execute(
                    "UPDATE clawbot_outbox SET state = 'sending', updated_at_ms = ?1 \
                     WHERE id = ?2 AND state = 'pending'",
                    params![u64_to_i64(now_ms), id],
                )
                .map_err(|error| format!("认领 ClawBot outbox 失败：{error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("提交 ClawBot outbox claim 失败：{error}"))?;
        ids.into_iter().map(|id| self.outbox_by_id(id)).collect()
    }

    pub fn upsert_group_member_grant(&self, grant: &WechatMemberGrant) -> Result<(), String> {
        if grant.account_id.trim().is_empty()
            || grant.group_id.trim().is_empty()
            || grant.member_id.trim().is_empty()
        {
            return Err("微信群成员授权的 account_id、group_id、member_id 不能为空".to_string());
        }
        if matches!(
            grant.preset,
            WechatMemberPreset::Collaborator | WechatMemberPreset::Administrator
        ) {
            return Err(
                "群成员角色只允许 none、chat_member、operator；操作管理员必须单独认领".to_string(),
            );
        }
        let mut grant = grant.clone();
        grant.capabilities = WechatCapabilitySet::for_group_member_role(grant.preset);
        let grant_json = serde_json::to_string(&grant)
            .map_err(|error| format!("序列化微信群成员授权失败：{error}"))?;
        self.connection
            .execute(
                "INSERT INTO clawbot_group_member_grants(\
                    account_id, group_id, member_id, grant_json, enabled, updated_at_ms\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(account_id, group_id, member_id) DO UPDATE SET \
                    grant_json = excluded.grant_json, \
                    enabled = excluded.enabled, \
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    grant.account_id,
                    grant.group_id,
                    grant.member_id,
                    grant_json,
                    if grant.enabled { 1 } else { 0 },
                    u64_to_i64(grant.updated_at_ms),
                ],
            )
            .map_err(|error| format!("保存微信群成员授权失败：{error}"))?;
        Ok(())
    }

    pub fn set_group_member_role(
        &self,
        account_id: &str,
        group_id: &str,
        member_id: &str,
        preset: WechatMemberPreset,
        actor: &str,
        now_ms: u64,
    ) -> Result<WechatMemberGrant, String> {
        if actor.trim().is_empty() {
            return Err("设置群成员角色必须记录操作主体".to_string());
        }
        if matches!(
            preset,
            WechatMemberPreset::Collaborator | WechatMemberPreset::Administrator
        ) {
            return Err("群成员角色只允许 none、chat_member、operator".to_string());
        }
        let administrator = self
            .operation_administrator(account_id)?
            .ok_or_else(|| "当前微信账号尚未认领操作管理员".to_string())?;
        if administrator.peer_id != actor {
            return Err("只有已认领的操作管理员可以设置群成员角色".to_string());
        }
        if administrator.peer_id == member_id {
            return Err("操作管理员由独立认领记录控制，不能通过成员角色命令降级".to_string());
        }
        let mut grant = self
            .group_member_grant(account_id, group_id, member_id)?
            .ok_or_else(|| "未找到当前群已识别成员".to_string())?;
        grant.preset = preset;
        grant.capabilities = WechatCapabilitySet::for_group_member_role(preset);
        grant.enabled = true;
        grant.created_by = Some(actor.to_string());
        grant.updated_at_ms = now_ms;
        grant.source = "role_management".to_string();
        self.upsert_group_member_grant(&grant)?;
        Ok(grant)
    }

    pub fn group_member_grant(
        &self,
        account_id: &str,
        group_id: &str,
        member_id: &str,
    ) -> Result<Option<WechatMemberGrant>, String> {
        self.connection
            .query_row(
                "SELECT grant_json FROM clawbot_group_member_grants \
                 WHERE account_id = ?1 AND group_id = ?2 AND member_id = ?3",
                params![account_id, group_id, member_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取微信群成员授权失败：{error}"))?
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|error| format!("解析微信群成员授权失败：{error}"))
            })
            .transpose()
    }

    pub fn list_group_member_grants(
        &self,
        account_id: &str,
        group_id: &str,
    ) -> Result<Vec<WechatMemberGrant>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT grant_json FROM clawbot_group_member_grants \
                 WHERE account_id = ?1 AND group_id = ?2 ORDER BY member_id ASC",
            )
            .map_err(|error| format!("准备微信群成员授权查询失败：{error}"))?;
        let rows = statement
            .query_map(params![account_id, group_id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("查询微信群成员授权失败：{error}"))?;
        let mut grants = Vec::new();
        for row in rows {
            let value = row.map_err(|error| format!("读取微信群成员授权行失败：{error}"))?;
            grants.push(
                serde_json::from_str(&value)
                    .map_err(|error| format!("解析微信群成员授权失败：{error}"))?,
            );
        }
        Ok(grants)
    }

    pub fn delete_group_member_grant(
        &self,
        account_id: &str,
        group_id: &str,
        member_id: &str,
    ) -> Result<bool, String> {
        self.connection
            .execute(
                "DELETE FROM clawbot_group_member_grants \
                 WHERE account_id = ?1 AND group_id = ?2 AND member_id = ?3",
                params![account_id, group_id, member_id],
            )
            .map(|changed| changed > 0)
            .map_err(|error| format!("删除微信群成员授权失败：{error}"))
    }

    pub fn observe_direct_contact(
        &self,
        account_id: &str,
        peer_id: &str,
        peer_name: Option<&str>,
        now_ms: u64,
    ) -> Result<WechatObservedContact, String> {
        validate_identity_parts(account_id, peer_id, "微信联系人")?;
        let existing = self
            .connection
            .query_row(
                "SELECT contact_json FROM clawbot_contacts WHERE account_id = ?1 AND peer_id = ?2",
                params![account_id, peer_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取微信联系人失败：{error}"))?
            .map(|value| {
                serde_json::from_str::<WechatObservedContact>(&value)
                    .map_err(|error| format!("解析微信联系人失败：{error}"))
            })
            .transpose()?;
        let mut contact = existing.unwrap_or_else(|| WechatObservedContact {
            account_id: account_id.to_string(),
            peer_id: peer_id.to_string(),
            peer_name: None,
            first_seen_at_ms: now_ms,
            last_seen_at_ms: now_ms,
        });
        if let Some(name) = non_empty(peer_name) {
            contact.peer_name = Some(name.to_string());
        }
        contact.last_seen_at_ms = now_ms;
        let contact_json = serde_json::to_string(&contact)
            .map_err(|error| format!("序列化微信联系人失败：{error}"))?;
        self.connection
            .execute(
                "INSERT INTO clawbot_contacts(account_id, peer_id, contact_json, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(account_id, peer_id) DO UPDATE SET \
                    contact_json = excluded.contact_json, updated_at_ms = excluded.updated_at_ms",
                params![account_id, peer_id, contact_json, u64_to_i64(now_ms)],
            )
            .map_err(|error| format!("保存微信联系人失败：{error}"))?;
        Ok(contact)
    }

    pub fn list_contacts(&self, account_id: &str) -> Result<Vec<WechatObservedContact>, String> {
        if account_id.trim().is_empty() {
            return Err("微信账号不能为空".to_string());
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT contact_json FROM clawbot_contacts \
                 WHERE account_id = ?1 ORDER BY updated_at_ms DESC, peer_id ASC",
            )
            .map_err(|error| format!("准备微信联系人查询失败：{error}"))?;
        let contacts = statement
            .query_map(params![account_id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("查询微信联系人失败：{error}"))?
            .map(|row| {
                let value = row.map_err(|error| format!("读取微信联系人行失败：{error}"))?;
                serde_json::from_str(&value).map_err(|error| format!("解析微信联系人失败：{error}"))
            })
            .collect();
        contacts
    }

    pub fn claim_operation_administrator(
        &self,
        account_id: &str,
        peer_id: &str,
        bot_mention_aliases: &[String],
        now_ms: u64,
    ) -> Result<WechatOperationAdministrator, String> {
        validate_identity_parts(account_id, peer_id, "操作管理员")?;
        let contact = self
            .connection
            .query_row(
                "SELECT contact_json FROM clawbot_contacts WHERE account_id = ?1 AND peer_id = ?2",
                params![account_id, peer_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取管理员候选联系人失败：{error}"))?
            .ok_or_else(|| "操作管理员必须从已识别私聊联系人中选择".to_string())?;
        let contact: WechatObservedContact = serde_json::from_str(&contact)
            .map_err(|error| format!("解析管理员候选联系人失败：{error}"))?;
        let aliases = normalize_aliases(bot_mention_aliases);
        let previous = self.operation_administrator(account_id)?;
        let administrator = WechatOperationAdministrator {
            account_id: account_id.to_string(),
            peer_id: peer_id.to_string(),
            peer_name: contact.peer_name,
            bot_mention_aliases: if aliases.is_empty() {
                vec!["ClawBot".to_string()]
            } else {
                aliases
            },
            claimed_at_ms: previous
                .as_ref()
                .filter(|value| value.peer_id == peer_id)
                .map(|value| value.claimed_at_ms)
                .unwrap_or(now_ms),
            updated_at_ms: now_ms,
        };
        let administrator_json = serde_json::to_string(&administrator)
            .map_err(|error| format!("序列化操作管理员失败：{error}"))?;
        self.connection
            .execute(
                "INSERT INTO clawbot_operation_administrators(account_id, administrator_json, updated_at_ms) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(account_id) DO UPDATE SET \
                    administrator_json = excluded.administrator_json, updated_at_ms = excluded.updated_at_ms",
                params![account_id, administrator_json, u64_to_i64(now_ms)],
            )
            .map_err(|error| format!("保存操作管理员失败：{error}"))?;
        Ok(administrator)
    }

    pub fn operation_administrator(
        &self,
        account_id: &str,
    ) -> Result<Option<WechatOperationAdministrator>, String> {
        self.connection
            .query_row(
                "SELECT administrator_json FROM clawbot_operation_administrators WHERE account_id = ?1",
                params![account_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取操作管理员失败：{error}"))?
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|error| format!("解析操作管理员失败：{error}"))
            })
            .transpose()
    }

    pub fn clear_operation_administrator(&self, account_id: &str) -> Result<bool, String> {
        self.connection
            .execute(
                "DELETE FROM clawbot_operation_administrators WHERE account_id = ?1",
                params![account_id],
            )
            .map(|changed| changed > 0)
            .map_err(|error| format!("撤销操作管理员失败：{error}"))
    }

    pub fn observe_group_identity(
        &self,
        account_id: &str,
        group_id: &str,
        group_name: Option<&str>,
        member_id: &str,
        member_name: Option<&str>,
        now_ms: u64,
    ) -> Result<(WechatGroupRecord, WechatMemberGrant), String> {
        validate_identity_parts(account_id, group_id, "微信群")?;
        if member_id.trim().is_empty() {
            return Err("微信群成员 ID 不能为空".to_string());
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("开始微信群身份观察事务失败：{error}"))?;

        let existing_group_json = transaction
            .query_row(
                "SELECT group_json FROM clawbot_groups WHERE account_id = ?1 AND group_id = ?2",
                params![account_id, group_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取微信群记录失败：{error}"))?;
        let mut group = existing_group_json
            .map(|value| {
                serde_json::from_str::<WechatGroupRecord>(&value)
                    .map_err(|error| format!("解析微信群记录失败：{error}"))
            })
            .transpose()?
            .unwrap_or_else(|| WechatGroupRecord {
                account_id: account_id.to_string(),
                group_id: group_id.to_string(),
                group_name: None,
                first_seen_at_ms: now_ms,
                last_seen_at_ms: now_ms,
                lifecycle_state: WechatGroupLifecycleState::Active,
                sync_evidence: "inbound_message".to_string(),
                binding: None,
                recognized_member_count: 0,
            });
        if group.lifecycle_state == WechatGroupLifecycleState::Removed {
            return Err(
                "微信群已标记移出；只有新的 bot_added 生命周期事件可以重新激活".to_string(),
            );
        }
        if let Some(name) = non_empty(group_name) {
            group.group_name = Some(name.to_string());
        }
        group.last_seen_at_ms = now_ms;
        group.lifecycle_state = WechatGroupLifecycleState::Active;
        group.sync_evidence = "inbound_message".to_string();

        let existing_member_json = transaction
            .query_row(
                "SELECT grant_json FROM clawbot_group_member_grants \
                 WHERE account_id = ?1 AND group_id = ?2 AND member_id = ?3",
                params![account_id, group_id, member_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取微信群成员失败：{error}"))?;
        let mut member = existing_member_json
            .map(|value| {
                serde_json::from_str::<WechatMemberGrant>(&value)
                    .map_err(|error| format!("解析微信群成员失败：{error}"))
            })
            .transpose()?
            .unwrap_or_else(|| {
                let mut value = WechatMemberGrant::from_preset(
                    account_id,
                    group_id,
                    member_id,
                    member_name.map(ToOwned::to_owned),
                    WechatMemberPreset::None,
                    now_ms,
                );
                value.source = "inbound_message".to_string();
                value
            });
        if member.first_seen_at_ms == 0 {
            member.first_seen_at_ms = member.created_at_ms.max(now_ms);
        }
        if let Some(name) = non_empty(member_name) {
            member.member_name = Some(name.to_string());
        }
        member.last_seen_at_ms = now_ms;
        member.updated_at_ms = member.updated_at_ms.max(now_ms);
        member.source = "inbound_message".to_string();
        member.capabilities = safe_member_capabilities(member.preset);
        let member_json = serde_json::to_string(&member)
            .map_err(|error| format!("序列化微信群成员失败：{error}"))?;
        transaction
            .execute(
                "INSERT INTO clawbot_group_member_grants(\
                    account_id, group_id, member_id, grant_json, enabled, updated_at_ms\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(account_id, group_id, member_id) DO UPDATE SET \
                    grant_json = excluded.grant_json, enabled = excluded.enabled, updated_at_ms = excluded.updated_at_ms",
                params![
                    account_id,
                    group_id,
                    member_id,
                    member_json,
                    if member.enabled { 1 } else { 0 },
                    u64_to_i64(member.updated_at_ms),
                ],
            )
            .map_err(|error| format!("保存微信群成员观察结果失败：{error}"))?;
        group.recognized_member_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM clawbot_group_member_grants WHERE account_id = ?1 AND group_id = ?2",
                params![account_id, group_id],
                |row| row.get::<_, i64>(0),
            )
            .map(i64_to_u64)
            .map_err(|error| format!("统计微信群已识别成员失败：{error}"))?;
        write_group_record(&transaction, &group, now_ms)?;
        transaction
            .commit()
            .map_err(|error| format!("提交微信群身份观察事务失败：{error}"))?;
        Ok((group, member))
    }

    pub fn observe_group_lifecycle(
        &self,
        account_id: &str,
        group_id: &str,
        group_name: Option<&str>,
        state: WechatGroupLifecycleState,
        evidence: &str,
        now_ms: u64,
    ) -> Result<WechatGroupRecord, String> {
        validate_identity_parts(account_id, group_id, "微信群")?;
        let mut group =
            self.group_record(account_id, group_id)?
                .unwrap_or_else(|| WechatGroupRecord {
                    account_id: account_id.to_string(),
                    group_id: group_id.to_string(),
                    group_name: None,
                    first_seen_at_ms: now_ms,
                    last_seen_at_ms: now_ms,
                    lifecycle_state: state,
                    sync_evidence: evidence.to_string(),
                    binding: None,
                    recognized_member_count: 0,
                });
        if let Some(name) = non_empty(group_name) {
            group.group_name = Some(name.to_string());
        }
        group.last_seen_at_ms = now_ms;
        group.lifecycle_state = state;
        group.sync_evidence = evidence.to_string();
        write_group_record(&self.connection, &group, now_ms)?;
        Ok(group)
    }

    pub fn group_record(
        &self,
        account_id: &str,
        group_id: &str,
    ) -> Result<Option<WechatGroupRecord>, String> {
        let value = self
            .connection
            .query_row(
                "SELECT group_json FROM clawbot_groups WHERE account_id = ?1 AND group_id = ?2",
                params![account_id, group_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取微信群记录失败：{error}"))?;
        value
            .map(|value| {
                let mut group: WechatGroupRecord = serde_json::from_str(&value)
                    .map_err(|error| format!("解析微信群记录失败：{error}"))?;
                group.recognized_member_count = self
                    .connection
                    .query_row(
                        "SELECT COUNT(*) FROM clawbot_group_member_grants WHERE account_id = ?1 AND group_id = ?2",
                        params![account_id, group_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(i64_to_u64)
                    .map_err(|error| format!("统计微信群已识别成员失败：{error}"))?;
                Ok(group)
            })
            .transpose()
    }

    pub fn list_groups(
        &self,
        account_id: &str,
        include_removed: bool,
    ) -> Result<Vec<WechatGroupRecord>, String> {
        let sql = if include_removed {
            "SELECT group_id FROM clawbot_groups WHERE account_id = ?1 ORDER BY updated_at_ms DESC"
        } else {
            "SELECT group_id FROM clawbot_groups WHERE account_id = ?1 AND lifecycle_state != 'removed' ORDER BY updated_at_ms DESC"
        };
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| format!("准备微信群列表查询失败：{error}"))?;
        let ids = statement
            .query_map(params![account_id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("查询微信群列表失败：{error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取微信群列表失败：{error}"))?;
        let mut groups = Vec::new();
        for group_id in ids {
            if let Some(group) = self.group_record(account_id, &group_id)? {
                groups.push(group);
            }
        }
        Ok(groups)
    }

    pub fn upsert_group_binding(
        &self,
        account_id: &str,
        group_id: &str,
        binding: &ClawbotConversationBinding,
        now_ms: u64,
    ) -> Result<WechatGroupRecord, String> {
        if binding.account_id != account_id || binding.peer_id != group_id {
            return Err("微信群绑定正文必须与 account_id/group_id 一致".to_string());
        }
        let mut group = self
            .group_record(account_id, group_id)?
            .ok_or_else(|| "微信群尚未由真实事件识别，不能预先伪造群绑定".to_string())?;
        if group.lifecycle_state == WechatGroupLifecycleState::Removed {
            return Err("微信群已标记移出；收到新的真实群事件后才能重新绑定".to_string());
        }
        group.binding = Some(binding.clone());
        group.last_seen_at_ms = group.last_seen_at_ms.max(now_ms);
        write_group_record(&self.connection, &group, now_ms)?;
        Ok(group)
    }

    pub fn detach_group_transaction(
        &self,
        account_id: &str,
        group_id: &str,
        reason: &str,
        actor: &str,
        now_ms: u64,
    ) -> Result<WechatGroupDetachAudit, String> {
        validate_identity_parts(account_id, group_id, "微信群")?;
        if reason.trim().is_empty() || actor.trim().is_empty() {
            return Err("群清理必须包含确定性原因和操作主体".to_string());
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("开始微信群清理事务失败：{error}"))?;
        let group_json = transaction
            .query_row(
                "SELECT group_json FROM clawbot_groups WHERE account_id = ?1 AND group_id = ?2",
                params![account_id, group_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("读取待清理微信群失败：{error}"))?
            .ok_or_else(|| "未找到待清理微信群".to_string())?;
        let mut group: WechatGroupRecord = serde_json::from_str(&group_json)
            .map_err(|error| format!("解析待清理微信群失败：{error}"))?;
        group.lifecycle_state = WechatGroupLifecycleState::Removed;
        group.sync_evidence = reason.to_string();
        group.last_seen_at_ms = now_ms;
        group.binding = None;
        group.recognized_member_count = 0;
        write_group_record(&transaction, &group, now_ms)?;
        transaction
            .execute(
                "DELETE FROM clawbot_group_member_grants WHERE account_id = ?1 AND group_id = ?2",
                params![account_id, group_id],
            )
            .map_err(|error| format!("清理微信群成员授权失败：{error}"))?;
        transaction
            .execute(
                "UPDATE clawbot_outbox SET state = 'dead_letter', last_error = ?1, updated_at_ms = ?2 \
                 WHERE account_id = ?3 AND peer_id = ?4 AND state IN ('pending', 'sending')",
                params![reason, u64_to_i64(now_ms), account_id, group_id],
            )
            .map_err(|error| format!("清理微信群待发送消息失败：{error}"))?;
        transaction
            .execute(
                "DELETE FROM clawbot_denial_notices WHERE account_id = ?1 AND group_id = ?2",
                params![account_id, group_id],
            )
            .map_err(|error| format!("清理微信群临时拒绝状态失败：{error}"))?;
        let audit = WechatGroupDetachAudit {
            account_id: account_id.to_string(),
            group_id: group_id.to_string(),
            group_name: group.group_name.clone(),
            reason: reason.to_string(),
            actor: actor.to_string(),
            detached_at_ms: now_ms,
        };
        let audit_json = serde_json::to_string(&audit)
            .map_err(|error| format!("序列化微信群清理审计失败：{error}"))?;
        transaction
            .execute(
                "INSERT INTO clawbot_group_audit(account_id, group_id, audit_json, created_at_ms) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![account_id, group_id, audit_json, u64_to_i64(now_ms)],
            )
            .map_err(|error| format!("保存微信群清理审计失败：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("提交微信群清理事务失败：{error}"))?;
        Ok(audit)
    }

    pub fn list_group_audit(
        &self,
        account_id: &str,
        group_id: &str,
    ) -> Result<Vec<WechatGroupDetachAudit>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT audit_json FROM clawbot_group_audit \
                 WHERE account_id = ?1 AND group_id = ?2 ORDER BY id ASC",
            )
            .map_err(|error| format!("准备微信群审计查询失败：{error}"))?;
        let audits = statement
            .query_map(params![account_id, group_id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("查询微信群审计失败：{error}"))?
            .map(|row| {
                let value = row.map_err(|error| format!("读取微信群审计行失败：{error}"))?;
                serde_json::from_str(&value).map_err(|error| format!("解析微信群审计失败：{error}"))
            })
            .collect();
        audits
    }

    pub fn claim_denial_notice_window(
        &self,
        account_id: &str,
        group_id: &str,
        member_id: &str,
        code: &str,
        now_ms: u64,
        window_ms: u64,
    ) -> Result<bool, String> {
        let previous = self
            .connection
            .query_row(
                "SELECT last_notice_at_ms FROM clawbot_denial_notices \
                 WHERE account_id = ?1 AND group_id = ?2 AND member_id = ?3 AND code = ?4",
                params![account_id, group_id, member_id, code],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| format!("读取微信群拒绝提示限流失败：{error}"))?
            .map(i64_to_u64);
        if previous.is_some_and(|value| now_ms.saturating_sub(value) < window_ms) {
            return Ok(false);
        }
        self.connection
            .execute(
                "INSERT INTO clawbot_denial_notices(account_id, group_id, member_id, code, last_notice_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(account_id, group_id, member_id, code) DO UPDATE SET \
                    last_notice_at_ms = excluded.last_notice_at_ms",
                params![account_id, group_id, member_id, code, u64_to_i64(now_ms)],
            )
            .map_err(|error| format!("保存微信群拒绝提示限流失败：{error}"))?;
        Ok(true)
    }

    pub fn metrics(&self) -> Result<ClawbotGatewayMetrics, String> {
        Ok(ClawbotGatewayMetrics {
            inbox_received: self.count_state("clawbot_inbox", "received")?,
            inbox_dispatching: self.count_state("clawbot_inbox", "dispatching")?,
            inbox_completed: self.count_state("clawbot_inbox", "completed")?,
            inbox_rejected: self.count_state("clawbot_inbox", "rejected")?,
            inbox_failed: self.count_state("clawbot_inbox", "failed")?,
            outbox_pending: self.count_state("clawbot_outbox", "pending")?,
            outbox_sending: self.count_state("clawbot_outbox", "sending")?,
            outbox_sent: self.count_state("clawbot_outbox", "sent")?,
            outbox_dead_letter: self.count_state("clawbot_outbox", "dead_letter")?,
        })
    }

    fn update_inbox(
        &self,
        id: i64,
        state: InboxState,
        result_json: Option<&str>,
        last_error: Option<&str>,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        let request_state = request_state_for_inbox_state(&state);
        self.update_inbox_request_state(id, state, request_state, result_json, last_error, now_ms)
    }

    fn update_inbox_request_state(
        &self,
        id: i64,
        state: InboxState,
        request_state: WechatRequestState,
        result_json: Option<&str>,
        last_error: Option<&str>,
        now_ms: u64,
    ) -> Result<ClawbotInboxItem, String> {
        let current = self.inbox_by_id(id)?;
        if current.state.is_terminal() || current.request_state.is_terminal() {
            return Ok(current);
        }
        let error_code = last_error
            .map(error_code_from_feedback)
            .or_else(|| match request_state {
                WechatRequestState::Denied => Some("denied".to_string()),
                WechatRequestState::Cancelled => Some("cancelled".to_string()),
                WechatRequestState::Failed => Some("failed".to_string()),
                _ => None,
            });
        self.connection
            .execute(
                "UPDATE clawbot_inbox SET state = ?1, request_state = ?2, error_code = ?3, \
                 result_json = ?4, last_error = ?5, updated_at_ms = ?6 WHERE id = ?7",
                params![
                    state.as_str(),
                    request_state.as_str(),
                    error_code,
                    result_json,
                    last_error,
                    u64_to_i64(now_ms),
                    id,
                ],
            )
            .map_err(|error| format!("更新 ClawBot inbox #{id} 状态失败：{error}"))?;
        self.inbox_by_id(id)
    }

    fn count_state(&self, table: &str, state: &str) -> Result<u64, String> {
        let sql = match table {
            "clawbot_inbox" => "SELECT COUNT(*) FROM clawbot_inbox WHERE state = ?1",
            "clawbot_outbox" => "SELECT COUNT(*) FROM clawbot_outbox WHERE state = ?1",
            _ => return Err(format!("不支持的 ClawBot 指标表：{table}")),
        };
        let count = self
            .connection
            .query_row(sql, params![state], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("统计 ClawBot {table}.{state} 失败：{error}"))?;
        Ok(i64_to_u64(count))
    }

    fn inbox_by_key(
        &self,
        account_id: &str,
        external_msg_id: &str,
    ) -> Result<Option<ClawbotInboxItem>, String> {
        let row = self
            .connection
            .query_row(
                "SELECT id, account_id, external_msg_id, peer_id, payload_json, state, \
                 request_state, error_code, request_id, parent_request_id, operation_fingerprint, \
                 result_json, last_error, created_at_ms, updated_at_ms \
                 FROM clawbot_inbox WHERE account_id = ?1 AND external_msg_id = ?2",
                params![account_id, external_msg_id],
                inbox_row,
            )
            .optional()
            .map_err(|error| format!("读取 ClawBot inbox 失败：{error}"))?;
        row.map(inbox_item_from_row).transpose()
    }

    fn inbox_by_id(&self, id: i64) -> Result<ClawbotInboxItem, String> {
        let row = self
            .connection
            .query_row(
                "SELECT id, account_id, external_msg_id, peer_id, payload_json, state, \
                 request_state, error_code, request_id, parent_request_id, operation_fingerprint, \
                 result_json, last_error, created_at_ms, updated_at_ms \
                 FROM clawbot_inbox WHERE id = ?1",
                params![id],
                inbox_row,
            )
            .map_err(|error| format!("读取 ClawBot inbox #{id} 失败：{error}"))?;
        inbox_item_from_row(row)
    }

    fn outbox_by_id(&self, id: i64) -> Result<ClawbotOutboxItem, String> {
        let row = self
            .connection
            .query_row(
                "SELECT id, account_id, peer_id, context_token, source_external_msg_id, body, file_json, \
                 state, attempts, next_attempt_at_ms, last_error, created_at_ms, updated_at_ms \
                 FROM clawbot_outbox WHERE id = ?1",
                params![id],
                outbox_row,
            )
            .map_err(|error| format!("读取 ClawBot outbox #{id} 失败：{error}"))?;
        outbox_item_from_row(row)
    }
}

type InboxDbRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

fn inbox_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<InboxDbRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
    ))
}

fn inbox_item_from_row(row: InboxDbRow) -> Result<ClawbotInboxItem, String> {
    let state = InboxState::from_db(&row.5)?;
    let request_state = row
        .6
        .as_deref()
        .map(WechatRequestState::from_db)
        .transpose()?
        .unwrap_or_else(|| request_state_for_inbox_state(&state));
    let request_id = row
        .8
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("wxreq:{}:{}", row.1, row.2));
    Ok(ClawbotInboxItem {
        id: row.0,
        account_id: row.1,
        external_msg_id: row.2,
        peer_id: row.3,
        payload_json: row.4,
        state,
        request_state,
        error_code: row.7,
        request_id,
        parent_request_id: row.9,
        operation_fingerprint: row.10,
        result_json: row.11,
        last_error: row.12,
        created_at_ms: i64_to_u64(row.13),
        updated_at_ms: i64_to_u64(row.14),
    })
}

type OutboxDbRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    i64,
    i64,
    Option<String>,
    i64,
    i64,
);

fn outbox_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxDbRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
    ))
}

fn outbox_item_from_row(row: OutboxDbRow) -> Result<ClawbotOutboxItem, String> {
    let file = row
        .6
        .as_deref()
        .map(serde_json::from_str::<ClawbotOutboundFile>)
        .transpose()
        .map_err(|error| format!("解析 ClawBot outbox 文件失败：{error}"))?;
    Ok(ClawbotOutboxItem {
        id: row.0,
        account_id: row.1,
        peer_id: row.2,
        context_token: row.3,
        source_external_msg_id: row.4,
        body: row.5,
        file,
        state: OutboxState::from_db(&row.7)?,
        attempts: row.8.max(0) as u32,
        next_attempt_at_ms: i64_to_u64(row.9),
        last_error: row.10,
        created_at_ms: i64_to_u64(row.11),
        updated_at_ms: i64_to_u64(row.12),
    })
}

fn validate_identity_parts(account_id: &str, identity_id: &str, label: &str) -> Result<(), String> {
    if account_id.trim().is_empty() || identity_id.trim().is_empty() {
        Err(format!("{label}的 account_id 和标识不能为空"))
    } else {
        Ok(())
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn normalize_aliases(values: &[String]) -> Vec<String> {
    let mut aliases = Vec::<String>::new();
    for value in values {
        let value = value.trim();
        if value.is_empty()
            || aliases
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(value))
        {
            continue;
        }
        aliases.push(value.to_string());
    }
    aliases
}

fn safe_member_capabilities(preset: WechatMemberPreset) -> WechatCapabilitySet {
    WechatCapabilitySet::for_group_member_role(preset)
}

fn group_lifecycle_state_str(state: WechatGroupLifecycleState) -> &'static str {
    match state {
        WechatGroupLifecycleState::AwaitingConfirmation => "awaiting_confirmation",
        WechatGroupLifecycleState::Active => "active",
        WechatGroupLifecycleState::SyncLimited => "sync_limited",
        WechatGroupLifecycleState::Removed => "removed",
    }
}

fn write_group_record(
    connection: &Connection,
    group: &WechatGroupRecord,
    now_ms: u64,
) -> Result<(), String> {
    let group_json =
        serde_json::to_string(group).map_err(|error| format!("序列化微信群记录失败：{error}"))?;
    connection
        .execute(
            "INSERT INTO clawbot_groups(account_id, group_id, group_json, lifecycle_state, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(account_id, group_id) DO UPDATE SET \
                group_json = excluded.group_json, \
                lifecycle_state = excluded.lifecycle_state, \
                updated_at_ms = excluded.updated_at_ms",
            params![
                group.account_id,
                group.group_id,
                group_json,
                group_lifecycle_state_str(group.lifecycle_state),
                u64_to_i64(now_ms),
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("保存微信群记录失败：{error}"))
}

pub fn is_terminal_group_send_error(error: &str) -> bool {
    let normalized = error.to_lowercase();
    [
        "机器人不在群",
        "已被移出群",
        "群不存在",
        "群聊不存在",
        "无发送权限",
        "group not found",
        "bot not in group",
        "removed from group",
        "send permission denied",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn i64_to_u64(value: i64) -> u64 {
    value.max(0) as u64
}

fn request_state_for_inbox_state(state: &InboxState) -> WechatRequestState {
    match state {
        InboxState::Received => WechatRequestState::Received,
        InboxState::Dispatching => WechatRequestState::Running,
        InboxState::Completed => WechatRequestState::Succeeded,
        InboxState::Rejected => WechatRequestState::Denied,
        InboxState::Failed => WechatRequestState::Failed,
    }
}

fn request_id_for_input(input: &ClawbotInboxInput) -> String {
    format!("wxreq:{}:{}", input.account_id, input.external_msg_id)
}

fn operation_fingerprint_for_input(input: &ClawbotInboxInput) -> String {
    format!(
        "inbound:{}:{}:{}",
        input.account_id, input.peer_id, input.external_msg_id
    )
}

fn error_code_from_feedback(feedback: &str) -> String {
    let trimmed = feedback.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some((code, _)) = rest.split_once(']') {
            let normalized = normalize_error_code(code);
            if !normalized.is_empty() {
                return normalized;
            }
        }
    }
    let first_token = trimmed
        .split(|ch: char| ch.is_whitespace() || ch == ':' || ch == '：')
        .next()
        .unwrap_or_default();
    let normalized = normalize_error_code(first_token);
    if normalized.is_empty() {
        "failed".to_string()
    } else {
        normalized
    }
}

fn normalize_error_code(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if matches!(ch, '_' | '-' | '.') {
                Some(ch)
            } else {
                None
            }
        })
        .take(64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_report_and_logout_form_a_safe_login_lifecycle() {
        let mut snapshot = ClawbotLoginSnapshot::default();

        snapshot.request_refresh(100);
        assert_eq!(snapshot.state, ClawbotLoginState::RefreshRequested);
        assert_eq!(snapshot.generation, 1);

        snapshot
            .report(
                ClawbotLoginReport {
                    generation: 1,
                    account_id: Some("wx-main".to_string()),
                    state: ClawbotLoginState::AwaitingScan,
                    qr_code_data_url: Some("data:image/png;base64,AA==".to_string()),
                    expires_at_ms: Some(1_000),
                    last_error: None,
                },
                200,
            )
            .expect("report awaiting scan");

        assert_eq!(
            snapshot.effective_state(500),
            ClawbotLoginState::AwaitingScan
        );
        assert_eq!(snapshot.effective_state(1_001), ClawbotLoginState::Expired);

        snapshot.logout(1_100);
        assert_eq!(snapshot.state, ClawbotLoginState::LoggedOut);
        assert!(snapshot.qr_code_data_url.is_none());
        assert!(snapshot.last_error.is_none());
    }

    #[test]
    fn login_report_rejects_stale_generation_and_invalid_qr() {
        let mut snapshot = ClawbotLoginSnapshot::default();
        snapshot.request_refresh(100);
        snapshot.request_refresh(110);

        let stale = snapshot.report(
            ClawbotLoginReport {
                generation: 1,
                account_id: None,
                state: ClawbotLoginState::Online,
                qr_code_data_url: None,
                expires_at_ms: None,
                last_error: None,
            },
            120,
        );
        assert!(stale.is_err());

        let invalid_qr = snapshot.report(
            ClawbotLoginReport {
                generation: 2,
                account_id: None,
                state: ClawbotLoginState::AwaitingScan,
                qr_code_data_url: Some("https://example.invalid/qr.png".to_string()),
                expires_at_ms: Some(1_000),
                last_error: None,
            },
            121,
        );
        assert!(invalid_qr.is_err());
    }

    #[test]
    fn login_report_accepts_weixin_liteapp_qr_link() {
        let mut snapshot = ClawbotLoginSnapshot::default();
        snapshot.request_refresh(100);

        snapshot
            .report(
                ClawbotLoginReport {
                    generation: 1,
                    account_id: Some("wx-ilink".to_string()),
                    state: ClawbotLoginState::AwaitingScan,
                    qr_code_data_url: Some(
                        "https://liteapp.weixin.qq.com/q/7GiQu1?qrcode=abc&bot_type=3".to_string(),
                    ),
                    expires_at_ms: Some(60_100),
                    last_error: None,
                },
                120,
            )
            .expect("liteapp qr link should be accepted");

        assert_eq!(snapshot.state, ClawbotLoginState::AwaitingScan);
        assert!(snapshot
            .qr_code_data_url
            .as_deref()
            .unwrap_or_default()
            .starts_with("https://liteapp.weixin.qq.com/q/"));
    }

    #[test]
    fn login_snapshot_persists_across_store_reopen() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("gateway.sqlite3");
        let store = ClawbotGatewayStore::open(&path).expect("open store");
        let mut snapshot = ClawbotLoginSnapshot::default();
        snapshot.request_refresh(100);
        store
            .save_login_snapshot(&snapshot)
            .expect("save login snapshot");
        drop(store);

        let reopened = ClawbotGatewayStore::open(&path).expect("reopen store");
        assert_eq!(
            reopened.load_login_snapshot().expect("load login snapshot"),
            snapshot
        );
    }

    fn inbound(external_msg_id: &str, payload_json: &str) -> ClawbotInboxInput {
        ClawbotInboxInput {
            account_id: "wx-main".to_string(),
            external_msg_id: external_msg_id.to_string(),
            peer_id: "peer-1".to_string(),
            payload_json: payload_json.to_string(),
        }
    }

    #[test]
    fn inbox_external_message_id_is_durable_and_conflict_safe() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("gateway.sqlite3");
        let store = ClawbotGatewayStore::open(&path).expect("open store");

        let first = store
            .register_inbox(&inbound("msg-1", r#"{"text":"hello"}"#), 100)
            .expect("first inbound");
        let duplicate = store
            .register_inbox(&inbound("msg-1", r#"{"text":"hello"}"#), 101)
            .expect("duplicate inbound");
        let conflict = store
            .register_inbox(&inbound("msg-1", r#"{"text":"changed"}"#), 102)
            .expect("conflicting inbound");

        assert!(matches!(first, InboxRegistration::Accepted(_)));
        assert!(matches!(duplicate, InboxRegistration::Duplicate(_)));
        assert!(matches!(conflict, InboxRegistration::Conflict(_)));

        drop(store);
        let reopened = ClawbotGatewayStore::open(&path).expect("reopen store");
        assert!(matches!(
            reopened
                .register_inbox(&inbound("msg-1", r#"{"text":"hello"}"#), 103)
                .expect("durable duplicate"),
            InboxRegistration::Duplicate(_)
        ));
    }

    fn outbound(body: &str) -> ClawbotOutboundMessage {
        ClawbotOutboundMessage {
            account_id: "wx-main".to_string(),
            peer_id: "peer-1".to_string(),
            context_token: Some("ctx-1".to_string()),
            source_external_msg_id: Some("msg-1".to_string()),
            body: body.to_string(),
            file: None,
        }
    }

    #[test]
    fn file_outbox_round_trips_attachment_metadata() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let mut message = outbound("发送文件：Cargo.toml");
        message.file = Some(ClawbotOutboundFile {
            local_path: r"C:\workspace\Cargo.toml".to_string(),
            display_name: "Cargo.toml".to_string(),
            mime: Some("text/plain".to_string()),
            size_bytes: 28,
            checksum: Some("fnv64:1234".to_string()),
        });

        store.enqueue_outbox(&message, 100).expect("enqueue file");
        let claimed = store.claim_due_outbox(100, 1).expect("claim file");

        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].file, message.file);
    }

    #[test]
    fn outbox_retries_three_times_then_enters_dead_letter() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let id = store
            .enqueue_outbox(&outbound("reply"), 100)
            .expect("enqueue outbox");
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 10,
            max_delay_ms: 100,
        };

        let first = store
            .mark_outbox_failed(id, "net-1", 100, policy)
            .expect("first failure");
        let second = store
            .mark_outbox_failed(id, "net-2", 111, policy)
            .expect("second failure");
        let third = store
            .mark_outbox_failed(id, "net-3", 132, policy)
            .expect("third failure");

        assert_eq!(first.state, OutboxState::Pending);
        assert_eq!(first.attempts, 1);
        assert_eq!(first.next_attempt_at_ms, 110);
        assert_eq!(second.state, OutboxState::Pending);
        assert_eq!(second.attempts, 2);
        assert_eq!(second.next_attempt_at_ms, 131);
        assert_eq!(third.state, OutboxState::DeadLetter);
        assert_eq!(third.attempts, 3);
        assert!(store
            .claim_due_outbox(1_000, 10)
            .expect("claim after dead letter")
            .is_empty());
    }

    #[test]
    fn inbox_terminal_state_and_gateway_metrics_are_observable() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let item = match store
            .register_inbox(&inbound("msg-observable", r#"{"text":"hello"}"#), 100)
            .expect("register inbound")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };

        store
            .mark_inbox_dispatching(item.id, 110)
            .expect("mark dispatching");
        let completed = store
            .mark_inbox_completed(item.id, r#"{"reply":"queued"}"#, 120)
            .expect("mark completed");
        store
            .enqueue_outbox(&outbound("reply"), 121)
            .expect("enqueue reply");

        assert_eq!(completed.state, InboxState::Completed);
        assert_eq!(
            completed.result_json.as_deref(),
            Some(r#"{"reply":"queued"}"#)
        );
        let metrics = store.metrics().expect("gateway metrics");
        assert_eq!(metrics.inbox_completed, 1);
        assert_eq!(metrics.inbox_failed, 0);
        assert_eq!(metrics.outbox_pending, 1);
        assert_eq!(metrics.outbox_dead_letter, 0);
    }

    #[test]
    fn inbox_terminal_state_cannot_reenter_dispatch() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let item = match store
            .register_inbox(&inbound("msg-terminal", r#"{"text":"hello"}"#), 100)
            .expect("register inbound")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };

        store
            .mark_inbox_dispatching(item.id, 110)
            .expect("mark dispatching");
        let completed = store
            .mark_inbox_completed(item.id, r#"{"reply":"queued"}"#, 120)
            .expect("mark completed");
        let after_duplicate_dispatch = store
            .mark_inbox_dispatching(item.id, 130)
            .expect("duplicate dispatch should be idempotent");

        assert_eq!(completed.state, InboxState::Completed);
        assert_eq!(after_duplicate_dispatch.state, InboxState::Completed);
        assert_eq!(
            after_duplicate_dispatch.result_json.as_deref(),
            Some(r#"{"reply":"queued"}"#)
        );
        assert_eq!(
            after_duplicate_dispatch.updated_at_ms,
            completed.updated_at_ms
        );
    }

    #[test]
    fn inbox_schema_has_request_tracking_columns() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let mut statement = store
            .connection
            .prepare("PRAGMA table_info(clawbot_inbox)")
            .expect("pragma table_info");
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("read table columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns");

        for expected in [
            "request_state",
            "error_code",
            "request_id",
            "parent_request_id",
            "operation_fingerprint",
        ] {
            assert!(
                columns.iter().any(|column| column == expected),
                "missing request tracking column {expected}; columns={columns:?}"
            );
        }
    }

    #[test]
    fn repeated_failed_operation_can_be_replayed_without_dispatch() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let first = match store
            .register_inbox(&inbound("msg-failed-1", r#"{"text":"/room missing"}"#), 100)
            .expect("register first")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };
        store
            .set_inbox_operation_fingerprint(first.id, "command:/room:missing", 101)
            .expect("set first fingerprint");
        let failed = store
            .mark_inbox_failed(first.id, "[room_not_found] 聊天室不存在", 110)
            .expect("mark failed");

        let second = match store
            .register_inbox(&inbound("msg-failed-2", r#"{"text":"/room missing"}"#), 200)
            .expect("register second")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };
        store
            .set_inbox_operation_fingerprint(second.id, "command:/room:missing", 201)
            .expect("set second fingerprint");
        let replay = store
            .recent_failed_operation("wx-main", "command:/room:missing", second.id, 0)
            .expect("query repeated failure")
            .expect("previous failure should be replayable");

        assert_eq!(replay.id, failed.id);
        assert_eq!(replay.request_state, WechatRequestState::Failed);
        assert_eq!(replay.error_code.as_deref(), Some("room_not_found"));
        assert_eq!(
            replay
                .result_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<WechatCommandResult>(json).ok())
                .map(|result| result.code),
            Some("room_not_found".to_string())
        );
    }

    #[test]
    fn repeated_failure_guard_does_not_extend_replay_window() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let original = match store
            .register_inbox(&inbound("msg-original", r#"{"text":"same request"}"#), 100)
            .expect("register original")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };
        store
            .set_inbox_operation_fingerprint(original.id, "chat:same-request", 101)
            .expect("set original fingerprint");
        store
            .mark_inbox_failed(original.id, "[model_dispatch_failed] 上游失败", 110)
            .expect("mark original failure");

        let guarded = match store
            .register_inbox(&inbound("msg-guarded", r#"{"text":"same request"}"#), 200)
            .expect("register guarded request")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };
        store
            .set_inbox_operation_fingerprint(guarded.id, "chat:same-request", 201)
            .expect("set guarded fingerprint");
        store
            .mark_inbox_failed(guarded.id, "[repeated_failure_guard] 已阻止重复调度", 210)
            .expect("mark guard feedback");

        let retry = match store
            .register_inbox(&inbound("msg-retry", r#"{"text":"same request"}"#), 300)
            .expect("register retry")
        {
            InboxRegistration::Accepted(item) => item,
            other => panic!("expected accepted inbox, got {other:?}"),
        };
        store
            .set_inbox_operation_fingerprint(retry.id, "chat:same-request", 301)
            .expect("set retry fingerprint");

        let replay = store
            .recent_failed_operation("wx-main", "chat:same-request", retry.id, 150)
            .expect("query repeated failure");
        assert!(
            replay.is_none(),
            "看护反馈不能在原始失败窗口结束后继续刷新阻断窗口"
        );
    }

    #[test]
    fn due_outbox_is_claimed_once_and_acknowledged() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let id = store
            .enqueue_outbox(&outbound("reply"), 100)
            .expect("enqueue outbox");

        let claimed = store.claim_due_outbox(100, 10).expect("claim due outbox");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, id);
        assert_eq!(claimed[0].state, OutboxState::Sending);
        assert!(store
            .claim_due_outbox(100, 10)
            .expect("second claim")
            .is_empty());

        let sent = store.mark_outbox_sent(id, 110).expect("ack outbox");
        assert_eq!(sent.state, OutboxState::Sent);
        let metrics = store.metrics().expect("gateway metrics");
        assert_eq!(metrics.outbox_sent, 1);
        assert_eq!(metrics.outbox_pending, 0);
    }

    #[test]
    fn sending_outbox_is_reclaimed_after_sidecar_lease_expires() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let id = store
            .enqueue_outbox(&outbound("reply"), 100)
            .expect("enqueue outbox");
        assert_eq!(
            store.claim_due_outbox(100, 10).expect("initial claim")[0].id,
            id
        );

        assert!(store
            .claim_due_outbox(100 + OUTBOX_CLAIM_LEASE_MS - 1, 10)
            .expect("claim before lease expiry")
            .is_empty());
        let reclaimed = store
            .claim_due_outbox(100 + OUTBOX_CLAIM_LEASE_MS, 10)
            .expect("claim after lease expiry");
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].id, id);
        assert_eq!(reclaimed[0].state, OutboxState::Sending);
        assert_eq!(reclaimed[0].attempts, 1);

        let reclaimed_again = store
            .claim_due_outbox(100 + OUTBOX_CLAIM_LEASE_MS * 2, 10)
            .expect("second expired lease");
        assert_eq!(reclaimed_again.len(), 1);
        assert_eq!(reclaimed_again[0].attempts, 2);
        assert!(store
            .claim_due_outbox(100 + OUTBOX_CLAIM_LEASE_MS * 3, 10)
            .expect("third expired lease")
            .is_empty());
        assert_eq!(
            store.metrics().expect("gateway metrics").outbox_dead_letter,
            1
        );
    }

    #[test]
    fn group_member_grants_are_scoped_persisted_and_revocable() {
        use crate::wechat_authorization::{WechatMemberGrant, WechatMemberPreset};

        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        assert!(store
            .list_group_member_grants("wx-main", "group-1")
            .expect("initial list")
            .is_empty());

        let mut member = WechatMemberGrant::from_preset(
            "wx-main",
            "group-1",
            "member-1",
            Some("测试成员".to_string()),
            WechatMemberPreset::Operator,
            100,
        );
        member.created_by = Some("console-user".to_string());
        store
            .upsert_group_member_grant(&member)
            .expect("upsert member");

        let loaded = store
            .group_member_grant("wx-main", "group-1", "member-1")
            .expect("read member")
            .expect("member exists");
        assert_eq!(loaded, member);
        assert_eq!(
            store
                .list_group_member_grants("wx-main", "group-1")
                .expect("list group"),
            vec![member]
        );
        assert!(store
            .list_group_member_grants("wx-main", "group-2")
            .expect("other group")
            .is_empty());

        assert!(store
            .delete_group_member_grant("wx-main", "group-1", "member-1")
            .expect("delete member"));
        assert!(store
            .group_member_grant("wx-main", "group-1", "member-1")
            .expect("read deleted")
            .is_none());
    }

    #[test]
    fn observed_group_member_defaults_to_none_and_preserves_role_on_next_message() {
        use crate::wechat_authorization::{WechatCapabilitySet, WechatMemberPreset};
        use crate::wechat_group::WechatGroupLifecycleState;

        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        let (group, first) = store
            .observe_group_identity(
                "wx-main",
                "group-1",
                Some("测试群"),
                "member-1",
                Some("测试成员"),
                100,
            )
            .expect("observe first message");
        assert_eq!(group.lifecycle_state, WechatGroupLifecycleState::Active);
        assert_eq!(first.preset, WechatMemberPreset::None);
        assert_eq!(first.first_seen_at_ms, 100);
        assert_eq!(first.last_seen_at_ms, 100);

        let mut operator = first;
        operator.preset = WechatMemberPreset::Operator;
        operator.capabilities = WechatCapabilitySet::for_preset(operator.preset);
        operator.updated_at_ms = 150;
        store
            .upsert_group_member_grant(&operator)
            .expect("promote member");

        let (_, observed_again) = store
            .observe_group_identity(
                "wx-main",
                "group-1",
                Some("测试群"),
                "member-1",
                Some("成员新名称"),
                200,
            )
            .expect("observe next message");
        assert_eq!(observed_again.preset, WechatMemberPreset::Operator);
        assert_eq!(observed_again.member_name.as_deref(), Some("成员新名称"));
        assert_eq!(observed_again.first_seen_at_ms, 100);
        assert_eq!(observed_again.last_seen_at_ms, 200);
    }

    #[test]
    fn operation_administrator_must_be_claimed_from_observed_direct_contacts() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        assert!(store
            .claim_operation_administrator(
                "wx-main",
                "peer-admin",
                &["ClawBot".to_string(), "库珠".to_string()],
                100,
            )
            .is_err());

        store
            .observe_direct_contact("wx-main", "peer-admin", Some("管理员"), 110)
            .expect("observe direct contact");
        let administrator = store
            .claim_operation_administrator(
                "wx-main",
                "peer-admin",
                &["ClawBot".to_string(), "库珠".to_string()],
                120,
            )
            .expect("claim administrator");
        assert_eq!(administrator.peer_name.as_deref(), Some("管理员"));
        assert_eq!(administrator.bot_mention_aliases, vec!["ClawBot", "库珠"]);
        assert_eq!(
            store
                .operation_administrator("wx-main")
                .expect("load administrator"),
            Some(administrator)
        );
    }

    #[test]
    fn group_detach_cascades_and_keeps_audit_tombstone() {
        use crate::clawbot_channel::ClawbotConversationBinding;
        use crate::wechat_group::WechatGroupLifecycleState;

        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        store
            .observe_group_identity(
                "wx-main",
                "group-1",
                Some("测试群"),
                "member-1",
                Some("测试成员"),
                100,
            )
            .expect("observe group");
        store
            .upsert_group_binding(
                "wx-main",
                "group-1",
                &ClawbotConversationBinding {
                    account_id: "wx-main".to_string(),
                    peer_id: "group-1".to_string(),
                    peer_name: Some("测试群".to_string()),
                    chat_room_id: Some("main-room".to_string()),
                    default_session_id: Some("session-glm".to_string()),
                    target_agent_ids: vec![],
                    workspace_id: "default".to_string(),
                    last_context_token: None,
                    allowlisted: true,
                    enabled: true,
                },
                110,
            )
            .expect("bind group");
        let outbox_id = store
            .enqueue_outbox(
                &ClawbotOutboundMessage {
                    account_id: "wx-main".to_string(),
                    peer_id: "group-1".to_string(),
                    context_token: None,
                    source_external_msg_id: None,
                    body: "待发送".to_string(),
                    file: None,
                },
                120,
            )
            .expect("enqueue group output");

        let audit = store
            .detach_group_transaction(
                "wx-main",
                "group-1",
                "manual_confirmed_removed",
                "peer-admin",
                130,
            )
            .expect("detach group");
        assert_eq!(audit.group_id, "group-1");
        let removed = store
            .group_record("wx-main", "group-1")
            .expect("read removed group")
            .expect("removed group tombstone");
        assert_eq!(removed.lifecycle_state, WechatGroupLifecycleState::Removed);
        assert!(removed.binding.is_none());
        assert!(store
            .list_group_member_grants("wx-main", "group-1")
            .expect("members removed")
            .is_empty());
        assert_eq!(
            store.outbox_by_id(outbox_id).expect("outbox state").state,
            OutboxState::DeadLetter
        );
        assert_eq!(
            store
                .list_group_audit("wx-main", "group-1")
                .expect("audit list"),
            vec![audit]
        );
        assert!(store
            .observe_group_identity(
                "wx-main",
                "group-1",
                Some("测试群"),
                "member-1",
                Some("测试成员"),
                140,
            )
            .is_err());
    }

    #[test]
    fn terminal_group_send_error_detaches_but_network_error_does_not() {
        assert!(is_terminal_group_send_error("机器人不在群聊中"));
        assert!(is_terminal_group_send_error("group not found"));
        assert!(is_terminal_group_send_error("send permission denied"));
        assert!(!is_terminal_group_send_error("provider offline"));
        assert!(!is_terminal_group_send_error("request timed out"));
    }

    #[test]
    fn denial_notice_window_allows_only_one_notice_per_member_and_code() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        assert!(store
            .claim_denial_notice_window(
                "wx-main",
                "group-1",
                "member-1",
                "capability_denied",
                1_000,
                300_000,
            )
            .expect("first notice"));
        assert!(!store
            .claim_denial_notice_window(
                "wx-main",
                "group-1",
                "member-1",
                "capability_denied",
                2_000,
                300_000,
            )
            .expect("second notice throttled"));
        assert!(store
            .claim_denial_notice_window(
                "wx-main",
                "group-1",
                "member-1",
                "room_permission_denied",
                2_000,
                300_000,
            )
            .expect("different code has its own window"));
        assert!(store
            .claim_denial_notice_window(
                "wx-main",
                "group-1",
                "member-1",
                "capability_denied",
                301_001,
                300_000,
            )
            .expect("window expired"));
    }

    #[test]
    fn clawbot_group_commands_enforce_operation_administrator() {
        let store = ClawbotGatewayStore::open_in_memory().expect("memory store");
        store
            .observe_direct_contact("wx-main", "peer-admin", Some("操作管理员"), 10)
            .expect("observe admin contact");
        store
            .claim_operation_administrator("wx-main", "peer-admin", &[], 20)
            .expect("claim admin");
        store
            .observe_group_identity(
                "wx-main",
                "group-1",
                Some("测试群"),
                "peer-admin",
                Some("操作管理员"),
                30,
            )
            .expect("observe admin in group");
        store
            .observe_group_identity(
                "wx-main",
                "group-1",
                Some("测试群"),
                "member-1",
                Some("普通成员"),
                40,
            )
            .expect("observe ordinary member");

        assert!(store
            .set_group_member_role(
                "wx-main",
                "group-1",
                "member-1",
                WechatMemberPreset::Operator,
                "member-1",
                50,
            )
            .is_err());
        assert!(store
            .set_group_member_role(
                "wx-main",
                "group-1",
                "peer-admin",
                WechatMemberPreset::None,
                "peer-admin",
                50,
            )
            .is_err());
        let updated = store
            .set_group_member_role(
                "wx-main",
                "group-1",
                "member-1",
                WechatMemberPreset::Operator,
                "peer-admin",
                50,
            )
            .expect("administrator updates role");
        assert_eq!(updated.preset, WechatMemberPreset::Operator);
        assert!(updated.capabilities.tools_write);
        assert!(!updated.capabilities.tasks_control);
    }
}
