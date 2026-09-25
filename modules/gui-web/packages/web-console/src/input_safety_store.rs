//! 共享输入安全库的**宿主侧 SQLite 适配**（第七轮裁决 §1；权威正文见
//! `docs/analysis/2026-09-21-integration-review/round7-rulings-and-gates.md`）。
//!
//! # 位置与依据
//!
//! 裁决 §1.4：领域契约在核心契约层（`runtime::input_safety`），**SQLite 实现在 Web／宿主适配边界**
//! （本文件，复用既有 rusqlite），**不把 rusqlite 引入 core-runtime**。唯一逻辑写入口是本 store。
//!
//! # 固定路径（裁决 §1.2）
//!
//! ```text
//! 根目录： ResolvedLaunchPaths.input_safety_state_root = <user_state_root>\input-safety
//! 数据库： <input_safety_state_root>\input-safety.sqlite3
//! ```
//!
//! **禁止**改用工作区 `.coolzhu`、当前工作目录、从 `log_dir` 再次推导的目录，或测试夹具路径作为生产默认。
//! 本模块**不自行推导**该根：它由宿主注入（[`input_safety_state_root`] 读注入的环境变量）；
//! **未注入即视为"存储不可用"**，绝不回退到自造路径。
//!
//! # 身份与"不得静默建空库"
//!
//! 首次初始化时在用户级根写一份**侧车标记** `<root>/store-identity.json`（只写一次、不删除），
//! 数据库自身也存一份身份。任何"标记在、库不在/库空/身份缺失"的组合都是**完整性事故**，
//! 必须拒绝（进入维护／恢复状态），**不得**静默建一个空库放行（裁决 §1.6）。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use runtime::{
    parse_input_safety_store_id, BlockingRefRejection, IncidentState, InputSafetyEvent,
    InputSafetyEventKind, InputSafetyIncident, InputSafetyRecoveryOperation,
    InputSafetyResourceScope, InputSafetyResourceState, InputSafetyStoreId, RecoveryDisposition,
    RecoveryStage, ReleaseIsolationDecision, ResourceSafetyState, VerifiedBlockingRef,
    INPUT_SAFETY_SCHEMA_VERSION,
};

/// 宿主注入的环境变量名：**输入安全状态根**（由 launcher 按 `input_safety_state_root_for` 注入）。
pub(crate) const INPUT_SAFETY_STATE_ROOT_ENV: &str = "COOLZHU_INPUT_SAFETY_STATE_ROOT";

/// 数据库文件名（稳定；升级 schema 不另建新文件）。
pub(crate) const INPUT_SAFETY_DB_FILE: &str = "input-safety.sqlite3";

/// 侧车身份标记文件名（只写一次、不删除）。
pub(crate) const INPUT_SAFETY_IDENTITY_FILE: &str = "store-identity.json";

/// 从宿主注入解析输入安全状态根。
///
/// **未注入即 `None`**（fail-closed）：不推导、不猜测、不回退到工作区或当前目录。
#[must_use]
pub(crate) fn input_safety_state_root() -> Option<PathBuf> {
    input_safety_state_root_from(std::env::var_os(INPUT_SAFETY_STATE_ROOT_ENV))
}

/// 解析的**纯函数**部分：给定注入值（可为 `None`）得出状态根。
///
/// 拆出这一层是为了让测试用**显式取值**验证，而**不必改动进程环境**——
/// 后者正是第七轮裁决 §5.2 要求治理的"测试污染"，新增测试不得再制造这种污染。
#[must_use]
pub(crate) fn input_safety_state_root_from(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

/// 打开／初始化失败的原因：每一类都有可区分的码，便于审计与运维处置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputSafetyStoreError {
    /// 宿主未注入状态根（不是"库不存在"，是"根本不知道库在哪"）。
    RootNotInjected,
    /// sqlite 层错误。
    Sqlite(String),
    /// 已登记过身份，但数据库文件缺失：**不得**静默建空库。
    IdentityRegisteredButDatabaseMissing { store_id: String },
    /// 数据库里没有身份行，但库并非空：来源不明，拒绝接管。
    UnknownProvenance,
    /// 身份标记与数据库内的身份不一致。
    IdentityMismatch { marker: String, database: String },
    /// schema 版本超前于本二进制。
    SchemaFromTheFuture { found: i64, supported: i64 },
    /// 侧车标记损坏（无法解析）。
    MarkerCorrupted,
    /// 同一资源 scope 已有活着的协调者：**epoch 冲突**（第八轮 §4 组合用例 2）。
    EpochConflict {
        scope: String,
        held_epoch: u64,
        held_by: String,
    },
    /// 调用方**未持有**该 scope 的当前恢复资格：**不得继续修改安全状态**（同上，组合用例 1）。
    RecoveryUnauthorized {
        scope: String,
        presented_epoch: Option<u64>,
        current_epoch: Option<u64>,
    },
}

impl InputSafetyStoreError {
    #[must_use]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::RootNotInjected => "input_safety_root_not_injected",
            Self::Sqlite(_) => "input_safety_sqlite_error",
            Self::IdentityRegisteredButDatabaseMissing { .. } => {
                "input_safety_database_missing_after_registration"
            }
            Self::UnknownProvenance => "input_safety_unknown_provenance",
            Self::IdentityMismatch { .. } => "input_safety_identity_mismatch",
            Self::SchemaFromTheFuture { .. } => "input_safety_schema_from_the_future",
            Self::MarkerCorrupted => "input_safety_marker_corrupted",
            Self::EpochConflict { .. } => "input_safety_epoch_conflict",
            Self::RecoveryUnauthorized { .. } => "input_safety_recovery_unauthorized",
        }
    }
}

impl std::fmt::Display for InputSafetyStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootNotInjected => write!(
                formatter,
                "宿主未注入输入安全状态根（{INPUT_SAFETY_STATE_ROOT_ENV}）——按裁决不得自行推导路径"
            ),
            Self::Sqlite(message) => write!(formatter, "输入安全库 sqlite 错误：{message}"),
            Self::IdentityRegisteredButDatabaseMissing { store_id } => write!(
                formatter,
                "已登记输入安全库身份 {store_id}，但数据库文件缺失：不静默建空库，进入维护/恢复状态"
            ),
            Self::UnknownProvenance => write!(
                formatter,
                "输入安全库存在但无身份行：来源不明，拒绝接管"
            ),
            Self::IdentityMismatch { marker, database } => write!(
                formatter,
                "输入安全库身份不一致（标记 {marker}，库内 {database}）"
            ),
            Self::SchemaFromTheFuture { found, supported } => write!(
                formatter,
                "输入安全库 schema 比本二进制更新（{found} > {supported}）：拒绝在不认识的 schema 上操作"
            ),
            Self::MarkerCorrupted => write!(formatter, "输入安全库身份标记损坏"),
            Self::EpochConflict {
                scope,
                held_epoch,
                held_by,
            } => write!(
                formatter,
                "资源 scope {scope} 已有活着的协调者（epoch {held_epoch}，实例 {held_by}）：拒绝第二个恢复者写竞争终态"
            ),
            Self::RecoveryUnauthorized {
                scope,
                presented_epoch,
                current_epoch,
            } => write!(
                formatter,
                "未持有 scope {scope} 的当前恢复资格（持有 epoch {presented_epoch:?}，当前 {current_epoch:?}）：不得修改安全状态"
            ),
        }
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// 生成一个新的存储身份：`is-` + 32 位小写十六进制（来源为时间戳 + 进程 + 地址熵）。
fn generate_store_id() -> InputSafetyStoreId {
    let mut seed = String::new();
    seed.push_str(&now_unix_ms().to_string());
    seed.push_str(&std::process::id().to_string());
    seed.push_str(&format!("{:p}", &seed));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&seed, &mut hasher);
    let first = std::hash::Hasher::finish(&hasher);
    let mut hasher2 = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&(seed.len(), first), &mut hasher2);
    let second = std::hash::Hasher::finish(&hasher2);
    let id = format!("is-{first:016x}{second:016x}");
    parse_input_safety_store_id(&id).expect("生成的 store id 必须可被契约层解析")
}

/// 侧车标记的最小形状（只用最朴素的 JSON 字段，避免把序列化依赖引入这条路径）。
fn marker_contents(store_id: &InputSafetyStoreId) -> String {
    format!(
        "{{\"store_id\":\"{}\",\"schema_version\":{},\"created_at_unix_ms\":{}}}",
        store_id.as_str(),
        INPUT_SAFETY_SCHEMA_VERSION,
        now_unix_ms()
    )
}

fn read_marker_store_id(marker_path: &Path) -> Result<Option<String>, InputSafetyStoreError> {
    if !marker_path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(marker_path)
        .map_err(|_| InputSafetyStoreError::MarkerCorrupted)?;
    // 极简解析：只取 store_id 字段；解析不出来即视为损坏（不猜）。
    let Some(start) = raw.find("\"store_id\":\"") else {
        return Err(InputSafetyStoreError::MarkerCorrupted);
    };
    let rest = &raw[start + "\"store_id\":\"".len()..];
    let Some(end) = rest.find('"') else {
        return Err(InputSafetyStoreError::MarkerCorrupted);
    };
    Ok(Some(rest[..end].to_string()))
}

/// 用户级共享输入安全库（唯一逻辑写入口）。
pub(crate) struct InputSafetyStore {
    connection: Connection,
    root: PathBuf,
    store_id: InputSafetyStoreId,
    /// 持有者存活判定（生产＝真实进程身份；测试可注入以模拟"持有者已崩溃"）。
    holder_is_alive: fn(u32, u64) -> bool,
}

impl InputSafetyStore {
    /// 按注入的根打开（未注入即 [`InputSafetyStoreError::RootNotInjected`]）。
    pub(crate) fn open_from_injection() -> Result<Self, InputSafetyStoreError> {
        let root = input_safety_state_root().ok_or(InputSafetyStoreError::RootNotInjected)?;
        Self::open_at(&root)
    }

    /// 在**显式给定的用户级根**打开（生产走注入；测试传临时目录）。
    ///
    /// 初始化与完整性规则（裁决 §1.6）：
    /// - 标记与库都不存在 ⇒ 首次初始化：建库、生成身份、写标记、追加 `StoreInitialized` 事件；
    /// - 标记在、库不在 ⇒ **拒绝**（不静默建空库）；
    /// - 库在、标记不在 ⇒ **拒绝**（来源不明）；
    /// - 两者都在但身份不一致 / 库内无身份行 ⇒ **拒绝**；
    /// - schema 超前 ⇒ **拒绝**（不靠降版本号"修复"）。
    pub(crate) fn open_at(root: &Path) -> Result<Self, InputSafetyStoreError> {
        let db_path = root.join(INPUT_SAFETY_DB_FILE);
        let marker_path = root.join(INPUT_SAFETY_IDENTITY_FILE);
        let marker_id = read_marker_store_id(&marker_path)?;
        let db_exists = db_path.exists();

        if let Some(marker) = marker_id.as_deref() {
            let marker_id = parse_input_safety_store_id(marker)
                .map_err(|_| InputSafetyStoreError::MarkerCorrupted)?;
            if !db_exists {
                return Err(InputSafetyStoreError::IdentityRegisteredButDatabaseMissing {
                    store_id: marker_id.as_str().to_string(),
                });
            }
            let connection = Self::open_connection(&db_path)?;
            Self::ensure_schema(&connection)?;
            let database_id = Self::read_identity(&connection)?;
            match database_id {
                None => return Err(InputSafetyStoreError::UnknownProvenance),
                Some(database_id) if database_id != marker_id => {
                    return Err(InputSafetyStoreError::IdentityMismatch {
                        marker: marker_id.as_str().to_string(),
                        database: database_id.as_str().to_string(),
                    });
                }
                Some(_) => {}
            }
            return Ok(Self {
                connection,
                root: root.to_path_buf(),
                store_id: marker_id,
                holder_is_alive: default_holder_is_alive,
            });
        }

        // 标记不存在。
        if db_exists {
            // 库在却没有标记：可能是别人建的库，或标记被删过——来源不明，拒绝接管。
            let connection = Self::open_connection(&db_path)?;
            Self::ensure_schema(&connection)?;
            if Self::read_identity(&connection)?.is_some() {
                return Err(InputSafetyStoreError::UnknownProvenance);
            }
            return Err(InputSafetyStoreError::UnknownProvenance);
        }

        // 首次初始化。
        std::fs::create_dir_all(root).map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        let connection = Self::open_connection(&db_path)?;
        Self::ensure_schema(&connection)?;
        let store_id = generate_store_id();
        connection
            .execute(
                "INSERT INTO input_safety_store_identity (singleton, store_id, schema_version, initialized_at_unix_ms)
                 VALUES (1, ?1, ?2, ?3)",
                params![store_id.as_str(), INPUT_SAFETY_SCHEMA_VERSION, now_unix_ms() as i64],
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        std::fs::write(&marker_path, marker_contents(&store_id))
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        let store = Self {
            connection,
            root: root.to_path_buf(),
            store_id,
            holder_is_alive: default_holder_is_alive,
        };
        store.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::StoreInitialized,
            scope: None,
            subject_id: None,
            detail: "首次初始化：生成存储身份并写入侧车标记".to_string(),
            recorded_at_unix_ms: now_unix_ms(),
        })?;
        Ok(store)
    }

    fn open_connection(db_path: &Path) -> Result<Connection, InputSafetyStoreError> {
        let connection = Connection::open(db_path)
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        Ok(connection)
    }

    /// 本库的**独立** schema（裁决 §1.2：与会话库版本号互不相干）。
    fn ensure_schema(connection: &Connection) -> Result<(), InputSafetyStoreError> {
        let current: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        if current > INPUT_SAFETY_SCHEMA_VERSION {
            return Err(InputSafetyStoreError::SchemaFromTheFuture {
                found: current,
                supported: INPUT_SAFETY_SCHEMA_VERSION,
            });
        }
        connection
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS input_safety_store_identity (
                    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                    store_id TEXT NOT NULL,
                    schema_version INTEGER NOT NULL,
                    initialized_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS input_safety_resource_state (
                    scope TEXT PRIMARY KEY,
                    state TEXT NOT NULL,
                    revision INTEGER NOT NULL,
                    coordinator_instance_id TEXT,
                    recovery_epoch INTEGER NOT NULL,
                    accepts_new_input INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS input_safety_incidents (
                    incident_id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    original_run_ref TEXT,
                    state TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL,
                    resolved_at_unix_ms INTEGER,
                    evidence_refs_json TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS input_safety_recovery_operations (
                    recovery_operation_id TEXT PRIMARY KEY,
                    coordinator_instance_id TEXT NOT NULL,
                    recovery_epoch INTEGER NOT NULL,
                    gate_revision INTEGER NOT NULL,
                    scope TEXT NOT NULL,
                    source_database_identity TEXT NOT NULL,
                    candidate_run_ids_json TEXT NOT NULL,
                    allowed_operations_json TEXT NOT NULL,
                    stage TEXT NOT NULL,
                    recorded_at_unix_ms INTEGER NOT NULL,
                    committed INTEGER NOT NULL,
                    -- v2（PR-01／P0-1）：最终处置。**过程看 stage，结论看本列**；
                    -- 旧行由 ALTER 补成 'pending'（未知不得当已结账）。
                    disposition TEXT NOT NULL DEFAULT 'pending'
                );
                -- 资源阻断事实（谁开的、为什么、何时关）：与 incident 分开——
                -- incident 是"事故"，本表是"当前该资源是否被阻断"的事实行。
                CREATE TABLE IF NOT EXISTS input_safety_resource_blocks (
                    block_id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    source_kind TEXT NOT NULL,
                    source_ref TEXT NOT NULL,
                    state TEXT NOT NULL,
                    opened_at_unix_ms INTEGER NOT NULL,
                    closed_at_unix_ms INTEGER
                );
                CREATE INDEX IF NOT EXISTS idx_input_safety_blocks_scope_state
                    ON input_safety_resource_blocks(scope, state);
                -- 恢复资格的 ownership epoch：同一 scope 同一时刻**只能有一个活着的协调者**（跨进程由本表的唯一性与短事务保证）。
                CREATE TABLE IF NOT EXISTS input_safety_ownership_epochs (
                    scope TEXT PRIMARY KEY,
                    epoch INTEGER NOT NULL,
                    coordinator_instance_id TEXT NOT NULL,
                    acquired_at_unix_ms INTEGER NOT NULL,
                    released_at_unix_ms INTEGER,
                    -- 持有者进程身份：用于判定"持有者是否已消失/是否被 PID 复用"。
                    -- 取不到（capture 失败）时存 0：**未知不等于已死亡**，此时不回收（fail-closed）。
                    holder_pid INTEGER NOT NULL DEFAULT 0,
                    holder_creation_filetime INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS input_safety_events (
                    event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind TEXT NOT NULL,
                    scope TEXT,
                    subject_id TEXT,
                    detail TEXT NOT NULL,
                    recorded_at_unix_ms INTEGER NOT NULL
                );
                -- v2（PR-01／P0-1）：**人工放行决定**。这是"谁在什么资格下、凭什么证据、
                -- 于何时解除隔离"的追加事实；它**不**删除 incident、**不**重置库、
                -- 也不等于"重启即解锁"——放开新输入仍须另行走按资格开放。
                CREATE TABLE IF NOT EXISTS input_safety_release_decisions (
                    decision_id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    operator TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    evidence_refs_json TEXT NOT NULL,
                    acknowledged_block_ids_json TEXT NOT NULL,
                    acknowledged_run_ids_json TEXT NOT NULL,
                    release_epoch INTEGER NOT NULL,
                    coordinator_instance_id TEXT NOT NULL,
                    decided_at_unix_ms INTEGER NOT NULL
                );
                "#,
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        // v1 → v2 **就地升级**：旧库（user_version=1）的恢复操作表没有 `disposition` 列。
        // 加列而不是另建空库——"另建空库"等于遗忘旧事故（本模块开篇口径）。
        if !Self::column_exists(connection, "input_safety_recovery_operations", "disposition")? {
            connection
                .execute_batch(
                    "ALTER TABLE input_safety_recovery_operations
                         ADD COLUMN disposition TEXT NOT NULL DEFAULT 'pending';",
                )
                .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        }
        if current < INPUT_SAFETY_SCHEMA_VERSION {
            connection
                .execute_batch(&format!("PRAGMA user_version = {INPUT_SAFETY_SCHEMA_VERSION};"))
                .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        }
        Ok(())
    }

    /// 列是否存在（用于**就地**迁移判定；只读 PRAGMA，不改任何状态）。
    fn column_exists(
        connection: &Connection,
        table: &str,
        column: &str,
    ) -> Result<bool, InputSafetyStoreError> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        Ok(names.iter().any(|name| name == column))
    }

    fn read_identity(
        connection: &Connection,
    ) -> Result<Option<InputSafetyStoreId>, InputSafetyStoreError> {
        let raw: Option<String> = connection
            .query_row(
                "SELECT store_id FROM input_safety_store_identity WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        match raw {
            None => Ok(None),
            Some(value) => parse_input_safety_store_id(&value)
                .map(Some)
                .map_err(|_| InputSafetyStoreError::MarkerCorrupted),
        }
    }

    #[must_use]
    pub(crate) fn store_id(&self) -> &InputSafetyStoreId {
        &self.store_id
    }

    /// **仅测试**：注入"持有者是否存活"探针，用于模拟"恢复者已崩溃"。
    ///
    /// 生产 `open_at` 永远使用真实进程身份判定；这里不提供任何生产可用的旁路。
    #[cfg(test)]
    pub(crate) fn with_liveness_probe_for_test(mut self, probe: fn(u32, u64) -> bool) -> Self {
        self.holder_is_alive = probe;
        self
    }

    #[must_use]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// 追加一条安全事件（只追加，不修改、不删除）。
    pub(crate) fn append_event(&self, event: InputSafetyEvent) -> Result<(), InputSafetyStoreError> {
        self.connection
            .execute(
                "INSERT INTO input_safety_events (kind, scope, subject_id, detail, recorded_at_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    event.kind.as_str(),
                    event.scope.as_ref().map(InputSafetyResourceScope::as_str),
                    event.subject_id,
                    event.detail,
                    event.recorded_at_unix_ms as i64
                ],
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        Ok(())
    }

    pub(crate) fn event_count(&self) -> Result<usize, InputSafetyStoreError> {
        let count: i64 = self
            .connection
            .query_row("SELECT count(*) FROM input_safety_events", [], |row| row.get(0))
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        Ok(count as usize)
    }

    /// 读回资源状态：**没有记录即 `Unknown`**（不是 Safe）。
    pub(crate) fn resource_state(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<InputSafetyResourceState, InputSafetyStoreError> {
        let row: Option<(String, i64, Option<String>, i64, i64)> = self
            .connection
            .query_row(
                "SELECT state, revision, coordinator_instance_id, recovery_epoch, accepts_new_input
                 FROM input_safety_resource_state WHERE scope = ?1",
                [scope.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        let Some((state, revision, coordinator, epoch, accepts)) = row else {
            return Ok(InputSafetyResourceState::initial(scope.clone()));
        };
        let parsed = match state.as_str() {
            "safe" => ResourceSafetyState::Safe,
            "isolated" => ResourceSafetyState::Isolated,
            // 认不出的状态一律按 Unknown 处理（绝不升级成 Safe）。
            _ => ResourceSafetyState::Unknown,
        };
        Ok(InputSafetyResourceState {
            scope: scope.clone(),
            state: parsed,
            revision: revision as u64,
            coordinator_instance_id: coordinator,
            recovery_epoch: epoch as u64,
            accepts_new_input: accepts != 0 && parsed.allows_new_input(),
        })
    }

    /// 写入资源状态（revision 必须**严格大于**已存 revision，防止旧视图回写）。
    pub(crate) fn put_resource_state(
        &self,
        state: &InputSafetyResourceState,
    ) -> Result<(), InputSafetyStoreError> {
        // 规则：**收紧不需要资格，放开需要资格**。放开新输入的唯一入口是
        // [`Self::reopen_new_input_authorized`]（它要求可信 [`RecoveryControlGuard`]）；
        // 本方法一律拒绝"接受新输入"，避免调用方用"填一个 epoch 字段"自证安全。
        if state.accepts_new_input {
            return Err(InputSafetyStoreError::RecoveryUnauthorized {
                scope: state.scope.as_str().to_string(),
                presented_epoch: Some(state.recovery_epoch),
                current_epoch: self
                    .current_recovery_epoch(&state.scope)?
                    .map(|value| value.epoch),
            });
        }
        let existing = self.resource_state(&state.scope)?;
        if state.revision <= existing.revision {
            return Err(InputSafetyStoreError::Sqlite(format!(
                "资源状态 revision 必须前进（现有 {}，收到 {}）",
                existing.revision, state.revision
            )));
        }
        self.connection
            .execute(
                "INSERT INTO input_safety_resource_state
                     (scope, state, revision, coordinator_instance_id, recovery_epoch, accepts_new_input, updated_at_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(scope) DO UPDATE SET
                     state = excluded.state,
                     revision = excluded.revision,
                     coordinator_instance_id = excluded.coordinator_instance_id,
                     recovery_epoch = excluded.recovery_epoch,
                     accepts_new_input = excluded.accepts_new_input,
                     updated_at_unix_ms = excluded.updated_at_unix_ms",
                params![
                    state.scope.as_str(),
                    state.state.as_str(),
                    state.revision as i64,
                    state.coordinator_instance_id,
                    state.recovery_epoch as i64,
                    i64::from(state.accepts_new_input),
                    now_unix_ms() as i64
                ],
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::ResourceStateChanged,
            scope: Some(state.scope.clone()),
            subject_id: state.coordinator_instance_id.clone(),
            detail: format!(
                "state={} revision={} accepts_new_input={}",
                state.state.as_str(),
                state.revision,
                state.accepts_new_input
            ),
            recorded_at_unix_ms: now_unix_ms(),
        })
    }

    /// **按资格**建立阻断（恢复路径 R5 用）：没有当前 epoch 就拒绝。
    pub(crate) fn establish_incident_authorized(
        &self,
        incident: &InputSafetyIncident,
        guard: &RecoveryControlGuard,
    ) -> Result<(), InputSafetyStoreError> {
        self.require_recovery_authorization(&incident.scope, guard)?;
        self.establish_incident(incident)
    }

    /// 建立一条**未决**阻断（incident）。
    pub(crate) fn establish_incident(
        &self,
        incident: &InputSafetyIncident,
    ) -> Result<(), InputSafetyStoreError> {
        self.connection
            .execute(
                "INSERT INTO input_safety_incidents
                     (incident_id, scope, reason, original_run_ref, state, created_at_unix_ms, resolved_at_unix_ms, evidence_refs_json)
                 VALUES (?1, ?2, ?3, ?4, 'pending', ?5, NULL, ?6)
                 ON CONFLICT(incident_id) DO NOTHING",
                params![
                    incident.incident_id,
                    incident.scope.as_str(),
                    incident.reason,
                    incident.original_run_ref,
                    incident.created_at_unix_ms as i64,
                    incident.evidence_refs.join("\n")
                ],
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::IncidentEstablished,
            scope: Some(incident.scope.clone()),
            subject_id: Some(incident.incident_id.clone()),
            detail: incident.reason.clone(),
            recorded_at_unix_ms: now_unix_ms(),
        })
    }

    /// 解决一条阻断：**解决不自动恢复新输入**（资源状态另行独立判定，裁决 §2.5）。
    pub(crate) fn resolve_incident(
        &self,
        incident_id: &str,
    ) -> Result<(), InputSafetyStoreError> {
        self.connection
            .execute(
                "UPDATE input_safety_incidents SET state = 'resolved', resolved_at_unix_ms = ?2
                 WHERE incident_id = ?1 AND state = 'pending'",
                params![incident_id, now_unix_ms() as i64],
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::IncidentResolved,
            scope: None,
            subject_id: Some(incident_id.to_string()),
            detail: "阻断已解决（不自动恢复新输入）".to_string(),
            recorded_at_unix_ms: now_unix_ms(),
        })
    }

    pub(crate) fn incident(
        &self,
        incident_id: &str,
    ) -> Result<Option<InputSafetyIncident>, InputSafetyStoreError> {
        let row: Option<(String, String, String, Option<String>, String, i64, Option<i64>, String)> = self
            .connection
            .query_row(
                "SELECT incident_id, scope, reason, original_run_ref, state, created_at_unix_ms, resolved_at_unix_ms, evidence_refs_json
                 FROM input_safety_incidents WHERE incident_id = ?1",
                [incident_id],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        let Some((incident_id, scope, reason, run_ref, state, created, resolved, evidence)) = row
        else {
            return Ok(None);
        };
        let scope = InputSafetyResourceScope::parse(&scope)
            .map_err(|_| InputSafetyStoreError::UnknownProvenance)?;
        Ok(Some(InputSafetyIncident {
            incident_id,
            scope,
            reason,
            original_run_ref: run_ref,
            state: if state == "resolved" {
                IncidentState::Resolved
            } else {
                IncidentState::Pending
            },
            created_at_unix_ms: created as u64,
            resolved_at_unix_ms: resolved.map(|value| value as u64),
            evidence_refs: if evidence.is_empty() {
                Vec::new()
            } else {
                evidence.split('\n').map(str::to_string).collect()
            },
        }))
    }

    /// 登记（或幂等读回）一条恢复操作。
    ///
    /// **已结账（`committed = 1`）的操作不受本函数影响**：结账是历史，不得被后续登记复活
    /// 或改判（`ON CONFLICT ... WHERE committed = 0`）。
    pub(crate) fn put_recovery_operation(
        &self,
        operation: &InputSafetyRecoveryOperation,
    ) -> Result<(), InputSafetyStoreError> {
        self.connection
            .execute(
                "INSERT INTO input_safety_recovery_operations
                     (recovery_operation_id, coordinator_instance_id, recovery_epoch, gate_revision, scope,
                      source_database_identity, candidate_run_ids_json, allowed_operations_json, stage,
                      recorded_at_unix_ms, committed, disposition)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(recovery_operation_id) DO UPDATE SET
                     stage = excluded.stage,
                     committed = excluded.committed
                 WHERE input_safety_recovery_operations.committed = 0
                   AND (excluded.stage != input_safety_recovery_operations.stage
                        OR excluded.committed != input_safety_recovery_operations.committed)",
                params![
                    operation.recovery_operation_id,
                    operation.coordinator_instance_id,
                    operation.recovery_epoch as i64,
                    operation.gate_revision as i64,
                    operation.scope.as_str(),
                    operation.source_database_identity,
                    operation.candidate_run_ids.join("\n"),
                    operation.allowed_operations.join("\n"),
                    operation.stage.as_str(),
                    operation.recorded_at_unix_ms as i64,
                    i64::from(operation.committed),
                    operation.disposition.as_str()
                ],
            )
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        Ok(())
    }

    pub(crate) fn recovery_operation(
        &self,
        recovery_operation_id: &str,
    ) -> Result<Option<InputSafetyRecoveryOperation>, InputSafetyStoreError> {
        #[allow(clippy::type_complexity)]
        let row: Option<(
            String,
            String,
            i64,
            i64,
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            String,
        )> = self
            .connection
            .query_row(
                "SELECT recovery_operation_id, coordinator_instance_id, recovery_epoch, gate_revision, scope,
                        source_database_identity, candidate_run_ids_json, allowed_operations_json, stage,
                        recorded_at_unix_ms, committed, disposition
                 FROM input_safety_recovery_operations WHERE recovery_operation_id = ?1",
                [recovery_operation_id],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                        row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?;
        let Some((
            id,
            coordinator,
            epoch,
            gate,
            scope,
            source_db,
            runs,
            ops,
            stage,
            recorded,
            committed,
            disposition,
        )) = row
        else {
            return Ok(None);
        };
        let scope = InputSafetyResourceScope::parse(&scope)
            .map_err(|_| InputSafetyStoreError::UnknownProvenance)?;
        let stage = match stage.as_str() {
            "r1_coordination_acquired" => RecoveryStage::CoordinationAcquired,
            "r2_intent_persisted_and_intake_closed" => RecoveryStage::IntentPersistedAndIntakeClosed,
            "r3_in_flight_registered" => RecoveryStage::InFlightRegistered,
            "r4_executor_stop_requested" => RecoveryStage::ExecutorStopRequested,
            "r5_incident_established" => RecoveryStage::IncidentEstablished,
            "r6_run_relation_checked" => RecoveryStage::RunRelationChecked,
            "r7_terminal_committed" => RecoveryStage::TerminalCommitted,
            "r8_stage_committed" => RecoveryStage::StageCommitted,
            "r9_reopened" => RecoveryStage::Reopened,
            _ => return Err(InputSafetyStoreError::UnknownProvenance),
        };
        Ok(Some(InputSafetyRecoveryOperation {
            recovery_operation_id: id,
            coordinator_instance_id: coordinator,
            recovery_epoch: epoch as u64,
            gate_revision: gate as u64,
            scope,
            source_database_identity: source_db,
            candidate_run_ids: split_lines(&runs),
            allowed_operations: split_lines(&ops),
            stage,
            recorded_at_unix_ms: recorded as u64,
            committed: committed != 0,
            disposition: RecoveryDisposition::parse(&disposition),
        }))
    }

    /// **核查一条阻断引用**（裁决 §1.5 的六项要求）。
    ///
    /// 只有全部通过才产出 [`VerifiedBlockingRef`]；任何一项不满足都返回**可区分**的拒绝原因。
    /// 序列化层可以继续把引用当字符串传递，但**生产接纳必须调用本函数**，不得只检查非空。
    pub(crate) fn verify_blocking_ref(
        &self,
        raw: &str,
        expected_scope: &InputSafetyResourceScope,
        expected_revision: u64,
        explained_by_run: Option<&str>,
    ) -> Result<VerifiedBlockingRef, BlockingRefRejection> {
        // 语法：`<store_id>#<incident_id>`；测试里那种 `input-safety:incident-9` 必须落在这里。
        let Some((store_part, incident_id)) = raw.split_once('#') else {
            return Err(BlockingRefRejection::Malformed);
        };
        let store_id = parse_input_safety_store_id(store_part)
            .map_err(|_| BlockingRefRejection::Malformed)?;
        if store_id != self.store_id {
            return Err(BlockingRefRejection::UnknownStore {
                store_id: store_id.as_str().to_string(),
            });
        }
        let Some(incident) = self
            .incident(incident_id)
            .map_err(|_| BlockingRefRejection::IncidentNotFound {
                incident_id: incident_id.to_string(),
            })?
        else {
            return Err(BlockingRefRejection::IncidentNotFound {
                incident_id: incident_id.to_string(),
            });
        };
        if incident.scope != *expected_scope {
            return Err(BlockingRefRejection::ScopeMismatch {
                expected: expected_scope.as_str().to_string(),
                actual: incident.scope.as_str().to_string(),
            });
        }
        if incident.state == IncidentState::Resolved {
            return Err(BlockingRefRejection::AlreadyResolved {
                incident_id: incident.incident_id.clone(),
            });
        }
        // 当前资源状态必须**确实阻止**新输入。
        let state = self
            .resource_state(expected_scope)
            .map_err(|_| BlockingRefRejection::NotBlocking {
                state: ResourceSafetyState::Unknown,
            })?;
        if state.accepts_new_input {
            return Err(BlockingRefRejection::NotBlocking { state: state.state });
        }
        if state.revision != expected_revision {
            return Err(BlockingRefRejection::RevisionMismatch {
                expected: expected_revision,
                actual: state.revision,
            });
        }
        // 可解释关联：引用必须能解释"为什么这条阻断与被处理运行/风险有关"。
        let explained = incident
            .original_run_ref
            .as_deref()
            .is_some_and(|run_ref| run_ref.trim() == explained_by_run.unwrap_or("").trim());
        if !explained {
            return Err(BlockingRefRejection::UnexplainedAssociation {
                incident_id: incident.incident_id.clone(),
            });
        }
        Ok(VerifiedBlockingRef::from_verified_parts(
            self.store_id.clone(),
            incident.incident_id.clone(),
            incident.scope.clone(),
            state.revision,
        ))
    }
}

fn split_lines(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split('\n').map(str::to_string).collect()
    }
}


/// 已持有的恢复资格（ownership epoch）视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnedRecoveryEpoch {
    pub scope: InputSafetyResourceScope,
    pub epoch: u64,
    pub coordinator_instance_id: String,
}

/// 启动对账时一条未提交恢复操作的去向。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecoveryReconcileOutcome {
    /// 本实例仍持有它当初的 epoch ⇒ 可按原 recovery ID 继续。
    StillAuthorized { recovery_operation_id: String },
    /// 资格已易主或本实例重启后尚未取权 ⇒ **必须重新取得协调权**，不得沿用旧 token 继续。
    NeedsReacquire {
        recovery_operation_id: String,
        previous_epoch: u64,
        previous_coordinator: String,
    },
}

/// 持有者存活判定：`(pid, creation_time_filetime) -> alive`。
///
/// 生产实现走 `windows-process-guard` 的真实进程身份（PID 复用会被识别为"换了人"）；
/// **取不到身份即视为未知** ⇒ 不回收（与"`WAIT_ABANDONED` 只表明需要核查"同一口径）。
fn default_holder_is_alive(pid: u32, creation_filetime: u64) -> bool {
    if pid == 0 || creation_filetime == 0 {
        return true;
    }
    match windows_process_guard::capture_process_identity(pid) {
        Ok(identity) => identity.creation_time_filetime() == creation_filetime,
        Err(_) => false,
    }
}

fn current_holder_identity() -> (u32, u64) {
    let pid = std::process::id();
    match windows_process_guard::capture_process_identity(pid) {
        Ok(identity) => (identity.pid(), identity.creation_time_filetime()),
        // 取不到自身身份：记 0，后续判定按"未知 ⇒ 不回收"处理。
        Err(_) => (pid, 0),
    }
}

fn sqlite_error(error: rusqlite::Error) -> InputSafetyStoreError {
    InputSafetyStoreError::Sqlite(error.to_string())
}

impl InputSafetyStore {
    /// 取得某 scope 的**唯一**恢复资格（第八轮 §3／§4 组合用例 2）。
    ///
    /// 已有**活着的**协调者且不是本实例 ⇒ [`InputSafetyStoreError::EpochConflict`]，**不得**抢写。
    /// 同一实例重新取得会推进 epoch（重启后必须重新取权，不得沿用旧 token）。
    pub(crate) fn acquire_recovery_epoch(
        &self,
        scope: &InputSafetyResourceScope,
        coordinator_instance_id: &str,
    ) -> Result<OwnedRecoveryEpoch, InputSafetyStoreError> {
        if coordinator_instance_id.trim().is_empty() {
            return Err(InputSafetyStoreError::Sqlite(
                "协调者实例标识不得为空（空标识等于没有协调者）".to_string(),
            ));
        }
        let transaction = self.connection.unchecked_transaction().map_err(sqlite_error)?;
        let existing: Option<(i64, String, Option<i64>)> = transaction
            .query_row(
                "SELECT epoch, coordinator_instance_id, released_at_unix_ms
                 FROM input_safety_ownership_epochs WHERE scope = ?1",
                [scope.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sqlite_error)?;
        let (holder_pid, holder_creation) = current_holder_identity();
        let holder_identity: Option<(u32, u64)> = transaction
            .query_row(
                "SELECT holder_pid, holder_creation_filetime FROM input_safety_ownership_epochs WHERE scope = ?1",
                [scope.as_str()],
                |row| Ok((row.get::<_, i64>(0)? as u32, row.get::<_, i64>(1)? as u64)),
            )
            .optional()
            .map_err(sqlite_error)?;
        let mut reclaimed_from: Option<(u64, String)> = None;
        let next_epoch = match &existing {
            Some((epoch, holder, None)) if holder != coordinator_instance_id => {
                // 冲突：只有在**持有者已消失**（进程不存在或 PID 被复用）时才允许回收陈旧资格；
                // "未知"（拿不到身份）一律不回收（fail-closed），仍报 epoch 冲突。
                let holder_gone = holder_identity
                    .is_some_and(|(pid, created)| !(self.holder_is_alive)(pid, created));
                if !holder_gone {
                    return Err(InputSafetyStoreError::EpochConflict {
                        scope: scope.as_str().to_string(),
                        held_epoch: *epoch as u64,
                        held_by: holder.clone(),
                    });
                }
                reclaimed_from = Some((*epoch as u64, holder.clone()));
                (*epoch as u64).saturating_add(1)
            }
            Some((epoch, _, _)) => (*epoch as u64).saturating_add(1),
            None => 1,
        };
        transaction
            .execute(
                "INSERT INTO input_safety_ownership_epochs
                     (scope, epoch, coordinator_instance_id, acquired_at_unix_ms, released_at_unix_ms,
                      holder_pid, holder_creation_filetime)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6)
                 ON CONFLICT(scope) DO UPDATE SET
                     epoch = excluded.epoch,
                     coordinator_instance_id = excluded.coordinator_instance_id,
                     acquired_at_unix_ms = excluded.acquired_at_unix_ms,
                     released_at_unix_ms = NULL,
                     holder_pid = excluded.holder_pid,
                     holder_creation_filetime = excluded.holder_creation_filetime",
                params![
                    scope.as_str(),
                    next_epoch as i64,
                    coordinator_instance_id,
                    now_unix_ms() as i64,
                    holder_pid as i64,
                    holder_creation as i64
                ],
            )
            .map_err(sqlite_error)?;
        let detail = match &reclaimed_from {
            Some((previous_epoch, previous_holder)) => format!(
                "取得恢复资格：epoch={next_epoch}（回收陈旧资格：原持有者 {previous_holder} 的 epoch {previous_epoch} 已消失；                 回收只表明需要核查，**不是**\"原状态已安全\"）"
            ),
            None => format!("取得恢复资格：epoch={next_epoch}"),
        };
        transaction
            .execute(
                "INSERT INTO input_safety_events (kind, scope, subject_id, detail, recorded_at_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    InputSafetyEventKind::RecoveryStageAdvanced.as_str(),
                    scope.as_str(),
                    coordinator_instance_id,
                    detail,
                    now_unix_ms() as i64
                ],
            )
            .map_err(sqlite_error)?;
        transaction.commit().map_err(sqlite_error)?;
        Ok(OwnedRecoveryEpoch {
            scope: scope.clone(),
            epoch: next_epoch,
            coordinator_instance_id: coordinator_instance_id.to_string(),
        })
    }

    /// 当前资格的**持有者是否仍存活**（用于崩溃后的对账；"未知"按存活处理，不误回收）。
    fn current_holder_still_alive(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<bool, InputSafetyStoreError> {
        let row: Option<(i64, i64)> = self
            .connection
            .query_row(
                "SELECT holder_pid, holder_creation_filetime FROM input_safety_ownership_epochs
                 WHERE scope = ?1 AND released_at_unix_ms IS NULL",
                [scope.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sqlite_error)?;
        Ok(match row {
            Some((pid, created)) => (self.holder_is_alive)(pid as u32, created as u64),
            None => false,
        })
    }

    /// 当前**活着**的恢复资格（没有则 `None`）。
    pub(crate) fn current_recovery_epoch(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<Option<OwnedRecoveryEpoch>, InputSafetyStoreError> {
        let row: Option<(i64, String)> = self
            .connection
            .query_row(
                "SELECT epoch, coordinator_instance_id FROM input_safety_ownership_epochs
                 WHERE scope = ?1 AND released_at_unix_ms IS NULL",
                [scope.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sqlite_error)?;
        Ok(row.map(|(epoch, coordinator)| OwnedRecoveryEpoch {
            scope: scope.clone(),
            epoch: epoch as u64,
            coordinator_instance_id: coordinator,
        }))
    }

    /// 释放自己持有的资格（只有持有者本人可释放）。
    pub(crate) fn release_recovery_epoch(
        &self,
        owned: &OwnedRecoveryEpoch,
    ) -> Result<(), InputSafetyStoreError> {
        let changed = self
            .connection
            .execute(
                "UPDATE input_safety_ownership_epochs SET released_at_unix_ms = ?3
                 WHERE scope = ?1 AND epoch = ?2 AND coordinator_instance_id = ?4
                   AND released_at_unix_ms IS NULL",
                params![
                    owned.scope.as_str(),
                    owned.epoch as i64,
                    now_unix_ms() as i64,
                    owned.coordinator_instance_id
                ],
            )
            .map_err(sqlite_error)?;
        if changed != 1 {
            return Err(InputSafetyStoreError::RecoveryUnauthorized {
                scope: owned.scope.as_str().to_string(),
                presented_epoch: Some(owned.epoch),
                current_epoch: self
                    .current_recovery_epoch(&owned.scope)?
                    .map(|current| current.epoch),
            });
        }
        Ok(())
    }

    /// 校验"调用方确实持有该 scope 的当前资格"（第八轮 §4 组合用例 1）。
    ///
    /// 这是取代 `paused=true` / `authority="xxx"` 式自证的唯一入口：
    /// 拿不出**当前活着的 epoch** 就 [`InputSafetyStoreError::RecoveryUnauthorized`]。
    pub(crate) fn require_recovery_authorization(
        &self,
        scope: &InputSafetyResourceScope,
        guard: &RecoveryControlGuard,
    ) -> Result<OwnedRecoveryEpoch, InputSafetyStoreError> {
        // 证明对象必须真是"这一份"：范围相同 + 持有当前 epoch + 是当前协调者。
        if guard.resource_scope != *scope {
            return Err(InputSafetyStoreError::RecoveryUnauthorized {
                scope: scope.as_str().to_string(),
                presented_epoch: Some(guard.epoch),
                current_epoch: None,
            });
        }
        let current = self.current_recovery_epoch(scope)?;
        match current {
            Some(current)
                if current.epoch == guard.epoch
                    && current.coordinator_instance_id == guard.coordinator_id =>
            {
                Ok(current)
            }
            current => Err(InputSafetyStoreError::RecoveryUnauthorized {
                scope: scope.as_str().to_string(),
                presented_epoch: Some(guard.epoch),
                current_epoch: current.map(|value| value.epoch),
            }),
        }
    }

    /// **按资格**推进恢复操作阶段：先校验持有当前 epoch，再按单调规则前进。
    pub(crate) fn advance_recovery_operation_authorized(
        &self,
        recovery_operation_id: &str,
        guard: &RecoveryControlGuard,
        next: RecoveryStage,
    ) -> Result<(), InputSafetyStoreError> {
        let mut operation = self
            .recovery_operation(recovery_operation_id)?
            .ok_or_else(|| {
                InputSafetyStoreError::Sqlite("恢复操作不存在".to_string())
            })?;
        let owned = self.require_recovery_authorization(&operation.scope, guard)?;
        // 资格失效后不得继续修改安全状态：操作若仍绑在**旧** epoch/协调者上，
        // 必须先经 `rebind_recovery_operation_authorized` 重新绑定（裁决 §2.2／§4-3）。
        if operation.recovery_epoch != owned.epoch
            || operation.coordinator_instance_id != owned.coordinator_instance_id
        {
            return Err(InputSafetyStoreError::RecoveryUnauthorized {
                scope: operation.scope.as_str().to_string(),
                presented_epoch: Some(operation.recovery_epoch),
                current_epoch: Some(owned.epoch),
            });
        }
        if !operation.can_advance_to(next) {
            return Err(InputSafetyStoreError::Sqlite(format!(
                "恢复阶段不得跳步或回退（当前 {} → 请求 {}）",
                operation.stage.as_str(),
                next.as_str()
            )));
        }
        operation.stage = next;
        if next == RecoveryStage::StageCommitted {
            operation.committed = true;
        }
        self.put_recovery_operation(&operation)?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::RecoveryStageAdvanced,
            scope: Some(operation.scope.clone()),
            subject_id: Some(recovery_operation_id.to_string()),
            detail: format!("阶段推进到 {}", next.as_str()),
            recorded_at_unix_ms: now_unix_ms(),
        })
    }

    /// **按资格重新绑定**一条恢复操作到当前 epoch/协调者（重启后"continue"的唯一入口）。
    ///
    /// 裁决 §2.2：资格失效后不得继续修改安全状态；重启读取旧意图时**必须重新取得协调权**。
    /// 因此这里要求调用方持有**当前** epoch，才允许把操作重新绑定到新的协调者与 epoch
    /// （`put_recovery_operation` 本身刻意只允许推进 stage/committed，不允许换绑）。
    pub(crate) fn rebind_recovery_operation_authorized(
        &self,
        recovery_operation_id: &str,
        guard: &RecoveryControlGuard,
    ) -> Result<InputSafetyRecoveryOperation, InputSafetyStoreError> {
        let operation = self
            .recovery_operation(recovery_operation_id)?
            .ok_or_else(|| InputSafetyStoreError::Sqlite("恢复操作不存在".to_string()))?;
        let owned = self.require_recovery_authorization(&operation.scope, guard)?;
        if operation.committed {
            return Err(InputSafetyStoreError::Sqlite(
                "已提交的恢复操作不得重新绑定".to_string(),
            ));
        }
        self.connection
            .execute(
                "UPDATE input_safety_recovery_operations
                    SET coordinator_instance_id = ?2, recovery_epoch = ?3, gate_revision = ?4
                  WHERE recovery_operation_id = ?1 AND committed = 0",
                params![
                    recovery_operation_id,
                    owned.coordinator_instance_id,
                    owned.epoch as i64,
                    owned.epoch as i64
                ],
            )
            .map_err(sqlite_error)?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::RecoveryStageAdvanced,
            scope: Some(operation.scope.clone()),
            subject_id: Some(recovery_operation_id.to_string()),
            detail: format!(
                "重新绑定恢复操作到 epoch={}（协调者 {}）",
                owned.epoch, owned.coordinator_instance_id
            ),
            recorded_at_unix_ms: now_unix_ms(),
        })?;
        self.recovery_operation(recovery_operation_id)?
            .ok_or_else(|| InputSafetyStoreError::Sqlite("重新绑定后读不到恢复操作".to_string()))
    }

    /// **结账**一条恢复操作：写入终态处置（PR-01／P0-1）。
    ///
    /// 为什么必须持有当前资格：结账决定"这件事怎么结的"，属于**安全状态**的修改。拿不出当前
    /// epoch 就结账，等于任何人写一行 `human_review_required` 就能把"永久隔离"包装成"已处置"。
    ///
    /// 规则：
    /// - `Pending` **不是**结账（拒收），避免"用结账掩盖仍在办"；
    /// - 已结账的操作**不得改判**（历史不得回写）；同值重复结账按幂等成功返回；
    /// - 结账同时写 `committed = 1`，此后不再出现在"待对账"集合里（这正是永久隔离的出口）。
    pub(crate) fn settle_recovery_operation_authorized(
        &self,
        recovery_operation_id: &str,
        guard: &RecoveryControlGuard,
        disposition: RecoveryDisposition,
        reason: &str,
    ) -> Result<InputSafetyRecoveryOperation, InputSafetyStoreError> {
        if !disposition.is_terminal() {
            return Err(InputSafetyStoreError::Sqlite(
                "Pending 不是结账：仍在办的操作不得写入终态处置".to_string(),
            ));
        }
        let operation = self
            .recovery_operation(recovery_operation_id)?
            .ok_or_else(|| InputSafetyStoreError::Sqlite("恢复操作不存在".to_string()))?;
        let owned = self.require_recovery_authorization(&operation.scope, guard)?;
        if operation.disposition.is_terminal() {
            if operation.disposition == disposition {
                return Ok(operation);
            }
            return Err(InputSafetyStoreError::Sqlite(format!(
                "已结账（{}）的操作不得改判为 {}：历史不得回写",
                operation.disposition.as_str(),
                disposition.as_str()
            )));
        }
        let changed = self
            .connection
            .execute(
                // 结账的唯一守门是"处置仍为 pending"（**不是** `committed = 0`）：
                // R8（`stage_committed`）本身就会把 `committed` 置 1，而处置与它是**两个维度**
                // ——"阶段走完了"不等于"结论写下了"。用 `committed = 0` 当守门会让已走完
                // R8 的收敛操作永远结不了账（现场实测：`presented_epoch == current_epoch`
                // 却报未持有资格，因为 UPDATE 打空）。
                "UPDATE input_safety_recovery_operations
                    SET disposition = ?2, committed = 1
                  WHERE recovery_operation_id = ?1 AND disposition = 'pending'",
                params![recovery_operation_id, disposition.as_str()],
            )
            .map_err(sqlite_error)?;
        if changed != 1 {
            return Err(InputSafetyStoreError::RecoveryUnauthorized {
                scope: operation.scope.as_str().to_string(),
                presented_epoch: Some(owned.epoch),
                current_epoch: self
                    .current_recovery_epoch(&operation.scope)?
                    .map(|value| value.epoch),
            });
        }
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::RecoveryStageAdvanced,
            scope: Some(operation.scope.clone()),
            subject_id: Some(recovery_operation_id.to_string()),
            detail: format!(
                "恢复操作结账：处置={} epoch={} 理由={}",
                disposition.as_str(),
                owned.epoch,
                reason.trim()
            ),
            recorded_at_unix_ms: now_unix_ms(),
        })?;
        self.recovery_operation(recovery_operation_id)?
            .ok_or_else(|| InputSafetyStoreError::Sqlite("结账后读不到恢复操作".to_string()))
    }

    /// 记录一条**人工放行决定**，并逐条关闭它声明的阻断（PR-01／P0-1）。
    ///
    /// 禁止事项照抄决策：**不**删除 incident、**不**重置安全库、**不**把"重启"当解锁。
    /// 因此这里做的是"追加一条决定事实 + 把它声明的阻断逐条置为 closed"，事故记录原样保留。
    ///
    /// 它**不**放开新输入：放开仍须经 [`Self::reopen_new_input_authorized`]（独立评估 + 当前资格）。
    /// 本函数只负责"人事"这一半——解除人工阻断；"机器"那一半留给开放路径。
    pub(crate) fn release_isolation_authorized(
        &self,
        decision: &ReleaseIsolationDecision,
        guard: &RecoveryControlGuard,
    ) -> Result<ReleaseIsolationDecision, InputSafetyStoreError> {
        if decision.operator.trim().is_empty() {
            return Err(InputSafetyStoreError::Sqlite(
                "放行必须署名：operator 不得为空".to_string(),
            ));
        }
        if decision.reason.trim().is_empty() {
            return Err(InputSafetyStoreError::Sqlite(
                "放行必须给出理由：reason 不得为空".to_string(),
            ));
        }
        let owned = self.require_recovery_authorization(&decision.scope, guard)?;
        // 资格与时刻由**库侧**落定，不接受调用方自报（否则可回填旧决定去套新阻断）。
        let decision = ReleaseIsolationDecision {
            release_epoch: owned.epoch,
            coordinator_instance_id: owned.coordinator_instance_id.clone(),
            decided_at_unix_ms: now_unix_ms(),
            ..decision.clone()
        };
        let existed: Option<String> = self
            .connection
            .query_row(
                "SELECT decision_id FROM input_safety_release_decisions WHERE decision_id = ?1",
                [decision.decision_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        if existed.is_some() {
            return Err(InputSafetyStoreError::Sqlite(format!(
                "放行决定 ID 已存在（{}）：决定是追加事实，不得覆盖",
                decision.decision_id
            )));
        }
        let mut closed_block_ids = Vec::new();
        for block_id in &decision.acknowledged_block_ids {
            let open: Option<String> = self
                .connection
                .query_row(
                    "SELECT block_id FROM input_safety_resource_blocks
                      WHERE block_id = ?1 AND scope = ?2 AND state = 'open'",
                    params![block_id.as_str(), decision.scope.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sqlite_error)?;
            let Some(block_id) = open else {
                return Err(InputSafetyStoreError::Sqlite(format!(
                    "阻断 {block_id} 不存在／已关闭／不属于 {}：不得放行未声明的阻断",
                    decision.scope.as_str()
                )));
            };
            self.close_resource_block(&block_id)?;
            closed_block_ids.push(block_id);
        }
        self.connection
            .execute(
                "INSERT INTO input_safety_release_decisions
                     (decision_id, scope, operator, reason, evidence_refs_json, acknowledged_block_ids_json,
                      acknowledged_run_ids_json, release_epoch, coordinator_instance_id, decided_at_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    decision.decision_id.as_str(),
                    decision.scope.as_str(),
                    decision.operator,
                    decision.reason,
                    decision.evidence_refs.join("\n"),
                    decision.acknowledged_block_ids.join("\n"),
                    decision.acknowledged_run_ids.join("\n"),
                    decision.release_epoch as i64,
                    decision.coordinator_instance_id,
                    decision.decided_at_unix_ms as i64
                ],
            )
            .map_err(sqlite_error)?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::IncidentResolved,
            scope: Some(decision.scope.clone()),
            subject_id: Some(decision.decision_id.clone()),
            detail: format!(
                "人工放行决定：operator={} epoch={} 解除阻断={:?} 接受遗留运行={:?} 理由={}",
                decision.operator,
                decision.release_epoch,
                closed_block_ids,
                decision.acknowledged_run_ids,
                decision.reason.trim()
            ),
            recorded_at_unix_ms: decision.decided_at_unix_ms,
        })?;
        Ok(decision)
    }

    /// 最近一次人工放行决定（不存在则 `None`）。
    pub(crate) fn latest_release_decision(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<Option<ReleaseIsolationDecision>, InputSafetyStoreError> {
        #[allow(clippy::type_complexity)]
        let row: Option<(String, String, String, String, String, String, String, i64, String, i64)> = self
            .connection
            .query_row(
                "SELECT decision_id, scope, operator, reason, evidence_refs_json,
                        acknowledged_block_ids_json, acknowledged_run_ids_json, release_epoch,
                        coordinator_instance_id, decided_at_unix_ms
                 FROM input_safety_release_decisions
                 WHERE scope = ?1
                 ORDER BY decided_at_unix_ms DESC, decision_id DESC
                 LIMIT 1",
                [scope.as_str()],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                        row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite_error)?;
        let Some((
            decision_id,
            scope_raw,
            operator,
            reason,
            evidence,
            blocks,
            runs,
            epoch,
            coordinator,
            decided,
        )) = row
        else {
            return Ok(None);
        };
        let scope = InputSafetyResourceScope::parse(&scope_raw)
            .map_err(|_| InputSafetyStoreError::UnknownProvenance)?;
        Ok(Some(ReleaseIsolationDecision {
            decision_id,
            scope,
            operator,
            reason,
            evidence_refs: split_lines(&evidence),
            acknowledged_block_ids: split_lines(&blocks),
            acknowledged_run_ids: split_lines(&runs),
            release_epoch: epoch as u64,
            coordinator_instance_id: coordinator,
            decided_at_unix_ms: decided as u64,
        }))
    }

    /// 该 scope **仍未获放行**的开启阻断。
    ///
    /// 判定规则：被最近一次放行决定**逐条声明**、且该决定**不早于**阻断开启时刻 ⇒ 视为已获放行。
    /// 用"不早于"而不是"晚于"：同毫秒内先开阻断、后做放行是正常次序（放行总是发生在看到阻断之后）；
    /// 反过来（旧决定被拿来套新阻断）必须无效——否则一条历史决定就能永久放行未来的一切阻断。
    pub(crate) fn unacknowledged_open_block_ids(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<Vec<String>, InputSafetyStoreError> {
        let decision = self.latest_release_decision(scope)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT block_id, opened_at_unix_ms FROM input_safety_resource_blocks
                 WHERE scope = ?1 AND state = 'open'
                 ORDER BY opened_at_unix_ms, block_id",
            )
            .map_err(sqlite_error)?;
        let rows = statement
            .query_map([scope.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(sqlite_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sqlite_error)?;
        Ok(match decision {
            None => rows.into_iter().map(|(id, _)| id).collect(),
            Some(decision) => rows
                .into_iter()
                .filter(|(id, opened)| {
                    !(decision.acknowledged_block_ids.contains(id)
                        && decision.decided_at_unix_ms >= (*opened as u64))
                })
                .map(|(id, _)| id)
                .collect(),
        })
    }

    /// 最近一次放行决定所**接受**的未收敛遗留运行（operator 已明确承担其风险）。
    pub(crate) fn acknowledged_run_ids(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<Vec<String>, InputSafetyStoreError> {
        Ok(self
            .latest_release_decision(scope)?
            .map(|decision| decision.acknowledged_run_ids)
            .unwrap_or_default())
    }

    /// 某 scope 上**未结账**（处置 `Pending`）的恢复操作：只有它们算"待对账"。
    pub(crate) fn unsettled_recovery_operations(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<Vec<InputSafetyRecoveryOperation>, InputSafetyStoreError> {
        self.recovery_operations_where(
            "scope = ?1 AND committed = 0 AND disposition = 'pending'",
            Some(scope.as_str()),
        )
    }

    /// 处置为 `human_review_required` 的操作（界面与运维口径：这是"等人"）。
    pub(crate) fn human_review_required_operations(
        &self,
    ) -> Result<Vec<InputSafetyRecoveryOperation>, InputSafetyStoreError> {
        self.recovery_operations_where("disposition = 'human_review_required'", None)
    }

    /// 按给定条件读回操作（内部工具：只用于上面两个口径查询）。
    fn recovery_operations_where(
        &self,
        predicate: &str,
        scope: Option<&str>,
    ) -> Result<Vec<InputSafetyRecoveryOperation>, InputSafetyStoreError> {
        let sql = format!(
            "SELECT recovery_operation_id FROM input_safety_recovery_operations
             WHERE {predicate}
             ORDER BY recorded_at_unix_ms, recovery_operation_id"
        );
        let mut statement = self.connection.prepare(&sql).map_err(sqlite_error)?;
        let ids = match scope {
            Some(scope) => statement
                .query_map([scope], |row| row.get::<_, String>(0))
                .map_err(sqlite_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sqlite_error)?,
            None => statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sqlite_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sqlite_error)?,
        };
        let mut operations = Vec::new();
        for id in ids {
            if let Some(operation) = self.recovery_operation(&id)? {
                operations.push(operation);
            }
        }
        Ok(operations)
    }

    /// 启动对账（第八轮 §4 组合用例 3）：读**未提交**的恢复操作，判断能否继续。
    ///
    /// **只读不删**：崩溃恢复**禁止**删除 recovery 行；资格已易主或本实例未取权时，
    /// 调用方必须**重新取得协调权**再继续。
    pub(crate) fn reconcile_recovery_operations_on_startup(
        &self,
    ) -> Result<Vec<RecoveryReconcileOutcome>, InputSafetyStoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT recovery_operation_id FROM input_safety_recovery_operations
                 WHERE committed = 0 ORDER BY recorded_at_unix_ms, recovery_operation_id",
            )
            .map_err(sqlite_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sqlite_error)?;
        let mut outcomes = Vec::new();
        for id in ids {
            let Some(operation) = self.recovery_operation(&id)? else {
                continue;
            };
            let current = self.current_recovery_epoch(&operation.scope)?;
            let holder_alive = self.current_holder_still_alive(&operation.scope)?;
            match current {
                Some(current)
                    if current.coordinator_instance_id == operation.coordinator_instance_id
                        && current.epoch == operation.recovery_epoch
                        && holder_alive =>
                {
                    outcomes.push(RecoveryReconcileOutcome::StillAuthorized {
                        recovery_operation_id: id,
                    });
                }
                _ => outcomes.push(RecoveryReconcileOutcome::NeedsReacquire {
                    recovery_operation_id: id,
                    previous_epoch: operation.recovery_epoch,
                    previous_coordinator: operation.coordinator_instance_id,
                }),
            }
        }
        Ok(outcomes)
    }

    /// 建立一条资源阻断事实（谁开的、为什么）。
    pub(crate) fn open_resource_block(
        &self,
        block_id: &str,
        scope: &InputSafetyResourceScope,
        source_kind: &str,
        source_ref: &str,
    ) -> Result<(), InputSafetyStoreError> {
        self.connection
            .execute(
                "INSERT INTO input_safety_resource_blocks
                     (block_id, scope, source_kind, source_ref, state, opened_at_unix_ms, closed_at_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, 'open', ?5, NULL)
                 ON CONFLICT(block_id) DO NOTHING",
                params![block_id, scope.as_str(), source_kind, source_ref, now_unix_ms() as i64],
            )
            .map_err(sqlite_error)?;
        Ok(())
    }

    pub(crate) fn close_resource_block(&self, block_id: &str) -> Result<(), InputSafetyStoreError> {
        self.connection
            .execute(
                "UPDATE input_safety_resource_blocks SET state = 'closed', closed_at_unix_ms = ?2
                 WHERE block_id = ?1 AND state = 'open'",
                params![block_id, now_unix_ms() as i64],
            )
            .map_err(sqlite_error)?;
        Ok(())
    }

    /// 该 scope 未关闭的阻断事实**条数**（诊断/评估用；比布尔更有信息量）。
    pub(crate) fn open_block_count(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<usize, InputSafetyStoreError> {
        let count: i64 = self
            .connection
            .query_row(
                "SELECT count(*) FROM input_safety_resource_blocks WHERE scope = ?1 AND state = 'open'",
                [scope.as_str()],
                |row| row.get(0),
            )
            .map_err(sqlite_error)?;
        Ok(count as usize)
    }

    /// 该 scope 是否仍有未关闭的阻断事实。
    pub(crate) fn has_open_resource_block(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<bool, InputSafetyStoreError> {
        let count: i64 = self
            .connection
            .query_row(
                "SELECT count(*) FROM input_safety_resource_blocks WHERE scope = ?1 AND state = 'open'",
                [scope.as_str()],
                |row| row.get(0),
            )
            .map_err(sqlite_error)?;
        Ok(count > 0)
    }

    /// **按资格**重新开放新输入（第八轮 §7 的"禁止自证"精神：放开必须有人负责）。
    ///
    /// 规则：**收紧方向不需要资格**（`Isolated`/`Unknown` 可随时写入），
    /// **放开方向必须持有当前 epoch**——只有协调者能证明"安全条件已独立成立"。
    pub(crate) fn reopen_new_input_authorized(
        &self,
        scope: &InputSafetyResourceScope,
        guard: &RecoveryControlGuard,
    ) -> Result<InputSafetyResourceState, InputSafetyStoreError> {
        self.require_recovery_authorization(scope, guard)?;
        if self.has_open_resource_block(scope)? {
            return Err(InputSafetyStoreError::Sqlite(
                "仍有未关闭的阻断事实：不得开放新输入".to_string(),
            ));
        }
        let mut state = self.resource_state(scope)?;
        state.revision = state.revision.saturating_add(1);
        state.state = ResourceSafetyState::Safe;
        state.accepts_new_input = true;
        self.connection
            .execute(
                "INSERT INTO input_safety_resource_state
                     (scope, state, revision, coordinator_instance_id, recovery_epoch, accepts_new_input, updated_at_unix_ms)
                 VALUES (?1, 'safe', ?2, ?3, ?4, 1, ?5)
                 ON CONFLICT(scope) DO UPDATE SET
                     state = 'safe', revision = excluded.revision,
                     coordinator_instance_id = excluded.coordinator_instance_id,
                     recovery_epoch = excluded.recovery_epoch,
                     accepts_new_input = 1, updated_at_unix_ms = excluded.updated_at_unix_ms",
                params![
                    scope.as_str(),
                    state.revision as i64,
                    state.coordinator_instance_id,
                    guard.epoch as i64,
                    now_unix_ms() as i64
                ],
            )
            .map_err(sqlite_error)?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::ResourceStateChanged,
            scope: Some(scope.clone()),
            subject_id: state.coordinator_instance_id.clone(),
            detail: format!("按资格重新开放新输入：revision={}", state.revision),
            recorded_at_unix_ms: now_unix_ms(),
        })?;
        self.resource_state(scope)
    }

    /// **收紧方向**不需要资格：隔离/未知可以随时写入（fail-closed 永远允许）。
    pub(crate) fn isolate_resource(
        &self,
        scope: &InputSafetyResourceScope,
        reason: &str,
        coordinator_instance_id: Option<&str>,
        epoch: u64,
    ) -> Result<InputSafetyResourceState, InputSafetyStoreError> {
        let mut state = self.resource_state(scope)?;
        state.transition(ResourceSafetyState::Isolated, coordinator_instance_id, epoch);
        self.put_resource_state(&state)?;
        self.append_event(InputSafetyEvent {
            kind: InputSafetyEventKind::ResourceStateChanged,
            scope: Some(scope.clone()),
            subject_id: coordinator_instance_id.map(str::to_string),
            detail: format!("隔离（收紧不需要资格）：{reason}"),
            recorded_at_unix_ms: now_unix_ms(),
        })?;
        Ok(state)
    }
}


// ---------------------------------------------------------------------------
// Phase 2（PR-RD4-02B）：跨进程资源协调器 + 可信 RecoveryControlGuard（第八轮 §3）
// ---------------------------------------------------------------------------

/// 恢复协调的**资源范围**：物理输入资源 = `windows-session-{id}` + `physical-input-resource`。
///
/// **不是** workspace／session_id／turn_id（裁决 §3）——否则同一桌面会因不同 workspace
/// 拿到两把互不相干的锁。
pub(crate) const PHYSICAL_INPUT_RESOURCE: &str = "physical-input-resource";

/// 本机的恢复协调范围字符串。
pub(crate) fn recovery_coordination_scope() -> Result<String, InputSafetyStoreError> {
    let session = windows_process_guard::current_interactive_session_scope()
        .map_err(|error| InputSafetyStoreError::Sqlite(format!("取交互会话 scope 失败：{error}")))?;
    Ok(format!("{session}|{PHYSICAL_INPUT_RESOURCE}"))
}

/// 本进程实例的协调者身份：**每次启动都不同**（重启必须重新取权，不得沿用旧 token）。
pub(crate) fn coordinator_instance_identity() -> String {
    let (pid, created) = current_holder_identity();
    format!("coordinator-{pid}-{created}")
}

/// 取得协调资格失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoordinatorError {
    /// 同一资源范围已有**活着的**协调者（进程内或跨进程）：不得抢写。
    Busy { scope: String },
    /// 存储层错误（含 epoch 冲突、无资格等）。
    Store(InputSafetyStoreError),
}

impl CoordinatorError {
    #[must_use]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::Busy { .. } => "input_safety_coordinator_busy",
            Self::Store(error) => error.code(),
        }
    }
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy { scope } => write!(
                formatter,
                "资源范围 {scope} 已有活着的恢复协调者：第二个恢复者不得写竞争终态"
            ),
            Self::Store(error) => write!(formatter, "{error}"),
        }
    }
}

/// **可信恢复资格证明**（取代 `paused=true` / `authority="xxx"` 式自证）。
///
/// 三重保护：字段私有、**无公开构造函数**（只能由 [`InputSafetyCoordinator`] 产生）、
/// **无 `Deserialize`**。存储层的所有"修改安全状态"入口都只收本类型，
/// 因此调用方**不可能**凭一个字符串或布尔值获得资格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoveryControlGuard {
    resource_scope: InputSafetyResourceScope,
    recovery_id: String,
    epoch: u64,
    coordinator_id: String,
    allowed_actions: Vec<String>,
}

impl RecoveryControlGuard {
    #[must_use]
    pub(crate) fn resource_scope(&self) -> &InputSafetyResourceScope {
        &self.resource_scope
    }

    #[must_use]
    pub(crate) fn recovery_id(&self) -> &str {
        &self.recovery_id
    }

    #[must_use]
    pub(crate) const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub(crate) fn coordinator_id(&self) -> &str {
        &self.coordinator_id
    }

    #[must_use]
    pub(crate) fn allowed_actions(&self) -> &[String] {
        &self.allowed_actions
    }

    /// 本次恢复**允许**执行某个具体操作吗（白名单）。
    #[must_use]
    pub(crate) fn allows_action(&self, action: &str) -> bool {
        self.allowed_actions.iter().any(|allowed| allowed == action)
    }
}

/// 跨进程资源协调器：**唯一**产生 [`RecoveryControlGuard`] 的地方。
///
/// 职责顺序（裁决 §3）：① 取得资源锁（跨进程排他）→ ② 写 `RecoveryOperationStarted`
/// → ③ 创建 `RecoveryControlGuard`。持有期间本对象保活命名互斥体（所有权固定在 guard 的
/// keeper 原生线程上，`Drop` 可从任意线程释放）。
pub(crate) struct InputSafetyCoordinator {
    store: InputSafetyStore,
    #[allow(dead_code)] // 保活：Drop 时由 guard 自身释放命名互斥体。
    lock: windows_process_guard::CrossProcessInputScopeGuard,
    guard: RecoveryControlGuard,
    /// 取得时是否观察到 `WAIT_ABANDONED`（前一个持有者崩溃/被强杀未释放）。
    abandoned: bool,
}

impl InputSafetyCoordinator {
    /// 开始一次协调：取锁 → 取 epoch → 写 `RecoveryOperationStarted` → 产出资格。
    ///
    /// `timeout` 为取锁等待上限（`Duration::ZERO` = 只做非阻塞探测）。
    pub(crate) fn begin(
        root: &Path,
        resource_scope: &InputSafetyResourceScope,
        recovery_id: &str,
        allowed_actions: &[&str],
        timeout: std::time::Duration,
    ) -> Result<Self, CoordinatorError> {
        let coordination_scope =
            recovery_coordination_scope().map_err(CoordinatorError::Store)?;
        Self::begin_with_coordination_scope(
            root,
            &coordination_scope,
            resource_scope,
            recovery_id,
            allowed_actions,
            timeout,
        )
    }

    /// **仅测试**：在显式协调范围之外再注入"持有者存活判定"（模拟"恢复者已崩溃"）。
    ///
    /// 生产 `begin` 永远使用真实进程身份；这里不提供任何生产可用的旁路。
    #[cfg(test)]
    pub(crate) fn begin_with_liveness_probe_for_test(
        root: &Path,
        coordination_scope: &str,
        resource_scope: &InputSafetyResourceScope,
        recovery_id: &str,
        allowed_actions: &[&str],
        probe: fn(u32, u64) -> bool,
    ) -> Result<Self, CoordinatorError> {
        let store = InputSafetyStore::open_at(root)
            .map_err(CoordinatorError::Store)?
            .with_liveness_probe_for_test(probe);
        Self::begin_on_store(store, coordination_scope, resource_scope, recovery_id, allowed_actions)
    }

    /// 显式协调范围版本：生产 `begin` 用它并传入真实作用域；
    /// 测试用它让**每个用例有唯一锁名**（否则同进程用例会互相争同一把命名锁）。
    pub(crate) fn begin_with_coordination_scope(
        root: &Path,
        coordination_scope: &str,
        resource_scope: &InputSafetyResourceScope,
        recovery_id: &str,
        allowed_actions: &[&str],
        timeout: std::time::Duration,
    ) -> Result<Self, CoordinatorError> {
        let lock = windows_process_guard::acquire_input_scope_across_processes(
            coordination_scope,
            timeout,
        )
        .map_err(|error| match error.is_busy() {
            true => CoordinatorError::Busy {
                scope: coordination_scope.to_string(),
            },
            false => CoordinatorError::Store(InputSafetyStoreError::Sqlite(format!(
                "取资源锁失败：{error}"
            ))),
        })?;
        let abandoned = lock.recovered_from_abandoned();
        let store = InputSafetyStore::open_at(root).map_err(CoordinatorError::Store)?;
        Self::begin_on_store_with_lock(store, lock, abandoned, coordination_scope, resource_scope, recovery_id, allowed_actions)
    }

    #[cfg(test)]
    fn begin_on_store(
        store: InputSafetyStore,
        coordination_scope: &str,
        resource_scope: &InputSafetyResourceScope,
        recovery_id: &str,
        allowed_actions: &[&str],
    ) -> Result<Self, CoordinatorError> {
        let lock = windows_process_guard::acquire_input_scope_across_processes(
            coordination_scope,
            std::time::Duration::from_millis(750),
        )
        .map_err(|error| match error.is_busy() {
            true => CoordinatorError::Busy {
                scope: coordination_scope.to_string(),
            },
            false => CoordinatorError::Store(InputSafetyStoreError::Sqlite(format!(
                "取资源锁失败：{error}"
            ))),
        })?;
        let abandoned = lock.recovered_from_abandoned();
        Self::begin_on_store_with_lock(store, lock, abandoned, coordination_scope, resource_scope, recovery_id, allowed_actions)
    }

    fn begin_on_store_with_lock(
        store: InputSafetyStore,
        lock: windows_process_guard::CrossProcessInputScopeGuard,
        abandoned: bool,
        coordination_scope: &str,
        resource_scope: &InputSafetyResourceScope,
        recovery_id: &str,
        allowed_actions: &[&str],
    ) -> Result<Self, CoordinatorError> {
        let coordinator_id = coordinator_instance_identity();
        let owned = store
            .acquire_recovery_epoch(resource_scope, &coordinator_id)
            .map_err(CoordinatorError::Store)?;
        // ② 写 RecoveryOperationStarted（= 登记这次恢复操作，阶段 R1）。
        store
            .put_recovery_operation(&InputSafetyRecoveryOperation {
                recovery_operation_id: recovery_id.to_string(),
                coordinator_instance_id: coordinator_id.clone(),
                recovery_epoch: owned.epoch,
                gate_revision: store
                    .resource_state(resource_scope)
                    .map_err(CoordinatorError::Store)?
                    .revision,
                scope: resource_scope.clone(),
                source_database_identity: "session-db:default".to_string(),
                candidate_run_ids: Vec::new(),
                allowed_operations: allowed_actions.iter().map(|value| (*value).to_string()).collect(),
                stage: RecoveryStage::CoordinationAcquired,
                recorded_at_unix_ms: now_unix_ms(),
                committed: false,
                disposition: RecoveryDisposition::Pending,
            })
            .map_err(CoordinatorError::Store)?;
        if abandoned {
            // `WAIT_ABANDONED` **只表明需要核查**，不是"资源已安全"（裁决 §2.3 原文口径）。
            store
                .append_event(InputSafetyEvent {
                    kind: InputSafetyEventKind::RecoveryStageAdvanced,
                    scope: Some(resource_scope.clone()),
                    subject_id: Some(recovery_id.to_string()),
                    detail: "取得资源锁时观察到 WAIT_ABANDONED：前一个协调者异常退出，受保护状态需要核查"
                        .to_string(),
                    recorded_at_unix_ms: now_unix_ms(),
                })
                .map_err(CoordinatorError::Store)?;
        }
        Ok(Self {
            store,
            lock,
            guard: RecoveryControlGuard {
                resource_scope: resource_scope.clone(),
                recovery_id: recovery_id.to_string(),
                epoch: owned.epoch,
                coordinator_id,
                allowed_actions: allowed_actions.iter().map(|value| (*value).to_string()).collect(),
            },
            abandoned,
        })
    }

    #[must_use]
    pub(crate) fn control(&self) -> &RecoveryControlGuard {
        &self.guard
    }

    #[must_use]
    pub(crate) fn store(&self) -> &InputSafetyStore {
        &self.store
    }

    /// 取得锁时是否观察到"前一个协调者异常退出"（需要核查，**不是**已安全）。
    #[must_use]
    pub(crate) const fn observed_abandoned_lock(&self) -> bool {
        self.abandoned
    }

    /// 让资格失效（例如等待人工确认期间主动让出排他）：**资格失效后不得再修改安全状态**。
    pub(crate) fn relinquish(self) -> Result<(), InputSafetyStoreError> {
        self.store.release_recovery_epoch(&OwnedRecoveryEpoch {
            scope: self.guard.resource_scope.clone(),
            epoch: self.guard.epoch,
            coordinator_instance_id: self.guard.coordinator_id.clone(),
        })
    }
}


/// 正式输入入口被拒绝的原因（第八轮 §1.4：**所有正式输入入口**都必须经本库检查状态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NewInputRefusal {
    /// 宿主未注入库根：**不得**回退到自造路径。
    RootNotInjected,
    /// 库不可读／完整性事故。
    StoreUnavailable { code: &'static str, message: String },
    /// 资源当前**不接受**新输入（`Unknown` 与 `Isolated` 都不接受）。
    ResourceNotAccepting { state: &'static str, revision: u64 },
    /// 取不到本机物理输入资源作用域。
    ScopeUnavailable { message: String },
}

impl NewInputRefusal {
    #[must_use]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::RootNotInjected => "input_safety_root_not_injected",
            Self::StoreUnavailable { code, .. } => code,
            Self::ResourceNotAccepting { .. } => "input_safety_resource_not_accepting_new_input",
            Self::ScopeUnavailable { .. } => "input_safety_scope_unavailable",
        }
    }

    #[must_use]
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::RootNotInjected => format!(
                "宿主未注入输入安全库根（{INPUT_SAFETY_STATE_ROOT_ENV}）：按裁决不得自行推导路径，故拒绝正式输入"
            ),
            Self::StoreUnavailable { code, message } => {
                format!("输入安全库不可用（{code}）：{message}")
            }
            Self::ResourceNotAccepting { state, revision } => format!(
                "物理输入资源当前不接受新输入（state={state} revision={revision}）：只有独立证明安全后才开放"
            ),
            Self::ScopeUnavailable { message } => {
                format!("无法确定本机物理输入资源作用域：{message}")
            }
        }
    }
}

/// 本机**物理输入资源**作用域（`windows-session-{id}`，与 broker 对齐）。
pub(crate) fn physical_input_resource_scope() -> Result<InputSafetyResourceScope, NewInputRefusal> {
    let session = windows_process_guard::current_interactive_session_scope().map_err(|error| {
        NewInputRefusal::ScopeUnavailable {
            message: error.to_string(),
        }
    })?;
    InputSafetyResourceScope::parse(&session).map_err(|_| NewInputRefusal::ScopeUnavailable {
        message: format!("交互会话 scope 不是有效资源作用域：{session}"),
    })
}

/// **正式输入入口的共享状态检查**（第八轮 §1.4）。
///
/// 语义：解析注入的库根 → 打开库 → 读该资源的当前状态；**只有** `accepts_new_input`
/// 为真才放行。`Unknown`（含"从未登记过该资源"）与 `Isolated` 一律拒绝——默认即 fail-closed。
pub(crate) fn require_resource_accepts_new_input(
    resource_scope: &InputSafetyResourceScope,
) -> Result<(), NewInputRefusal> {
    let root = input_safety_state_root().ok_or(NewInputRefusal::RootNotInjected)?;
    let store = InputSafetyStore::open_at(&root).map_err(|error| {
        NewInputRefusal::StoreUnavailable {
            code: error.code(),
            message: error.to_string(),
        }
    })?;
    let state = store
        .resource_state(resource_scope)
        .map_err(|error| NewInputRefusal::StoreUnavailable {
            code: error.code(),
            message: error.to_string(),
        })?;
    if state.accepts_new_input {
        Ok(())
    } else {
        Err(NewInputRefusal::ResourceNotAccepting {
            state: state.state.as_str(),
            revision: state.revision,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    fn scope() -> InputSafetyResourceScope {
        InputSafetyResourceScope::parse("windows-session-1").expect("scope")
    }

    /// 按用例唯一的资源 scope 与协调范围（避免同进程用例互相争锁/争 epoch）。
    fn unique_scope(tag: &str) -> InputSafetyResourceScope {
        InputSafetyResourceScope::parse(&format!("windows-session-{tag}")).expect("scope")
    }

    fn coordination_scope_for(tag: &str) -> String {
        format!("windows-session-{tag}|{PHYSICAL_INPUT_RESOURCE}")
    }

    /// 测试用协调器：走**生产同一入口**（取锁 → 取 epoch → 写 RecoveryOperationStarted → 出资格）。
    fn coordinator_for_scope(
        root: &std::path::Path,
        tag: &str,
        scope: &InputSafetyResourceScope,
        recovery_id: &str,
    ) -> InputSafetyCoordinator {
        InputSafetyCoordinator::begin_with_coordination_scope(
            root,
            &coordination_scope_for(tag),
            scope,
            recovery_id,
            &["converge_legacy_cu_run"],
            std::time::Duration::from_millis(750),
        )
        .expect("coordinator")
    }

    fn coordinator_for(
        root: &std::path::Path,
        tag: &str,
        recovery_id: &str,
    ) -> (InputSafetyCoordinator, InputSafetyResourceScope) {
        let scope = unique_scope(tag);
        let coordinator = coordinator_for_scope(root, tag, &scope, recovery_id);
        (coordinator, scope)
    }

    /// **PR-01／P0-1**：结账必须持有当前资格；`Pending` 不是结账；结账后不得改判、不得被登记复活。
    ///
    /// 这组规则合起来才构成"永久隔离"的出口：处置是**终态**（所以不再挂账），
    /// 又是**一次性**的（所以不会被后续登记悄悄改回"还在办"）。
    #[test]
    fn settling_is_authorized_final_and_not_resurrectable() {
        let root = temp_root();
        let (coordinator, scope) = coordinator_for(root.path(), "settle", "recovery-settle");
        let operation_id = "recovery-settle";

        // ① `Pending` 不是结账。
        let refused = coordinator
            .store()
            .settle_recovery_operation_authorized(
                operation_id,
                coordinator.control(),
                RecoveryDisposition::Pending,
                "试图用结账掩盖仍在办",
            )
            .err()
            .expect("Pending 必须被拒");
        assert!(refused.to_string().contains("Pending 不是结账"), "{refused}");

        // ② 正常结账：owner 未知 ⇒ human_review_required，且同时提交。
        let settled = coordinator
            .store()
            .settle_recovery_operation_authorized(
                operation_id,
                coordinator.control(),
                RecoveryDisposition::HumanReviewRequired,
                "owner 关系永久未知",
            )
            .expect("结账");
        assert_eq!(settled.disposition, RecoveryDisposition::HumanReviewRequired);
        assert!(settled.committed, "结账必须同时提交");

        // ③ 幂等：同值重复结账成功返回。
        coordinator
            .store()
            .settle_recovery_operation_authorized(
                operation_id,
                coordinator.control(),
                RecoveryDisposition::HumanReviewRequired,
                "重复结账（幂等）",
            )
            .expect("同值重复结账必须成功");

        // ④ 改判：拒绝（历史不得回写）。
        let rejudged = coordinator
            .store()
            .settle_recovery_operation_authorized(
                operation_id,
                coordinator.control(),
                RecoveryDisposition::Recovered,
                "试图把保持隔离改判成已收敛",
            )
            .err()
            .expect("改判必须被拒");
        assert!(rejudged.to_string().contains("不得改判"), "{rejudged}");

        // ⑤ 登记（幂等 upsert）不得复活/降级已结账的操作。
        let mut resurrect =
            pending_operation(&scope, coordinator.control().epoch(), coordinator.control().coordinator_id());
        resurrect.recovery_operation_id = operation_id.to_string();
        coordinator
            .store()
            .put_recovery_operation(&resurrect)
            .expect("put");
        let read_back = coordinator
            .store()
            .recovery_operation(operation_id)
            .expect("read")
            .expect("exists");
        assert_eq!(
            read_back.disposition,
            RecoveryDisposition::HumanReviewRequired,
            "已结账的操作不得被后续登记复活"
        );
        assert!(read_back.committed, "已结账的 committed 不得被登记回退");

        // ⑥ 已结账 ⇒ 移出"待对账"，但仍在"待人工复核"里可见（这是"等人"，不是"等机器"）。
        assert!(
            coordinator
                .store()
                .unsettled_recovery_operations(&scope)
                .expect("unsettled")
                .is_empty(),
            "结账后不得再算待对账（这正是永久隔离的出口）"
        );
        assert!(
            coordinator
                .store()
                .human_review_required_operations()
                .expect("human")
                .iter()
                .any(|operation| operation.recovery_operation_id == operation_id),
            "human_review_required 必须可查询（待办要能归属到人）"
        );
    }

    /// **PR-01／P0-1**：人工放行必须署名 + 理由；只解除**逐条声明**的阻断；事故记录不得删除。
    #[test]
    fn release_decision_requires_signature_and_releases_only_declared_blocks() {
        let root = temp_root();
        let (coordinator, scope) = coordinator_for(root.path(), "release", "recovery-release");
        let store = coordinator.store();
        // 一条真实事故 + 两条阻断：放行只解除声明的那条，事故必须原样保留。
        store
            .establish_incident_authorized(&incident_for(&scope, "incident-a"), coordinator.control())
            .expect("incident");
        store
            .open_resource_block("block-a", &scope, "legacy_recovery", "incident-a")
            .expect("block-a");
        store
            .open_resource_block("block-b", &scope, "legacy_recovery", "incident-b")
            .expect("block-b");

        let base = ReleaseIsolationDecision {
            decision_id: "release-1".to_string(),
            scope: scope.clone(),
            operator: String::new(),
            reason: "已人工核对：旧执行者确认不在场".to_string(),
            evidence_refs: vec!["manual-check-2026-09-25".to_string()],
            acknowledged_block_ids: vec!["block-a".to_string()],
            acknowledged_run_ids: vec!["legacy-cu-1".to_string()],
            // 资格与时刻由库侧落定；这里刻意填占位值，验证库会覆盖它。
            release_epoch: 0,
            coordinator_instance_id: String::new(),
            decided_at_unix_ms: 0,
        };

        // ① 匿名放行 ⇒ 拒绝。
        let anonymous = store
            .release_isolation_authorized(&base, coordinator.control())
            .err()
            .expect("匿名必须被拒");
        assert!(anonymous.to_string().contains("署名"), "{anonymous}");

        // ② 无理由 ⇒ 拒绝。
        let named = ReleaseIsolationDecision {
            operator: "ops-zhang".to_string(),
            ..base.clone()
        };
        let no_reason = store
            .release_isolation_authorized(
                &ReleaseIsolationDecision {
                    reason: "   ".to_string(),
                    ..named.clone()
                },
                coordinator.control(),
            )
            .err()
            .expect("无理由必须被拒");
        assert!(no_reason.to_string().contains("理由"), "{no_reason}");

        // ③ 正常放行：只解除 block-a；block-b 仍算未获放行；epoch/时刻由库侧落定。
        let recorded = store
            .release_isolation_authorized(&named, coordinator.control())
            .expect("release");
        assert_eq!(
            recorded.release_epoch,
            coordinator.control().epoch(),
            "留痕 epoch 必须来自库侧（不接受调用方自报）"
        );
        assert!(recorded.decided_at_unix_ms > 0, "时刻也必须由库侧落定");
        assert_eq!(
            store.unacknowledged_open_block_ids(&scope).expect("open"),
            vec!["block-b".to_string()],
            "未声明的阻断不得被解除"
        );
        assert_eq!(
            store.acknowledged_run_ids(&scope).expect("runs"),
            vec!["legacy-cu-1".to_string()],
            "operator 接受的遗留运行必须可读回"
        );
        // ④ **禁止删事故**：放行后 incident 仍然在，且状态未被篡改。
        let incident = store.incident("incident-a").expect("read").expect("事故不得被删除");
        assert_eq!(incident.state, IncidentState::Pending);
        assert!(store.latest_release_decision(&scope).expect("latest").is_some());

        // ⑤ 决定是追加事实：同 ID 覆盖 ⇒ 拒绝。
        let duplicate = store
            .release_isolation_authorized(&named, coordinator.control())
            .err()
            .expect("覆盖必须被拒");
        assert!(duplicate.to_string().contains("不得覆盖"), "{duplicate}");

        // ⑥ 声明不存在/已关闭的阻断 ⇒ 拒绝（不得放行未声明的阻断）。
        let bogus = store
            .release_isolation_authorized(
                &ReleaseIsolationDecision {
                    decision_id: "release-2".to_string(),
                    acknowledged_block_ids: vec!["block-a".to_string()],
                    ..named
                },
                coordinator.control(),
            )
            .err()
            .expect("已关闭的阻断不得再次放行");
        assert!(bogus.to_string().contains("不得放行未声明的阻断"), "{bogus}");
    }

    /// **PR-01**：v1 库**就地升级**到 v2（加列 + 建新表），旧行按"仍在办"读回——**不另建空库**。
    #[test]
    fn v1_database_is_upgraded_in_place_and_old_rows_read_as_pending() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let store_id = store.store_id().as_str().to_string();
        drop(store);
        // 把库降回 **v1 形态**：没有 disposition 列、没有放行决定表、版本号 1，并留一条旧行。
        {
            let connection =
                Connection::open(root.path().join(INPUT_SAFETY_DB_FILE)).expect("reopen raw");
            connection
                .execute_batch(
                    "DROP TABLE IF EXISTS input_safety_release_decisions;
                     ALTER TABLE input_safety_recovery_operations DROP COLUMN disposition;
                     INSERT INTO input_safety_recovery_operations
                         (recovery_operation_id, coordinator_instance_id, recovery_epoch, gate_revision, scope,
                          source_database_identity, candidate_run_ids_json, allowed_operations_json, stage,
                          recorded_at_unix_ms, committed)
                     VALUES ('recovery-old', 'coordinator-old', 1, 1, 'windows-session-1', 'session-db:default',
                             '', '', 'r5_incident_established', 1, 0);
                     PRAGMA user_version = 1;",
                )
                .expect("v1 shape");
        }
        let upgraded = InputSafetyStore::open_at(root.path()).expect("就地升级必须成功");
        assert_eq!(upgraded.store_id().as_str(), store_id, "就地升级不得换身份");
        let old = upgraded
            .recovery_operation("recovery-old")
            .expect("read")
            .expect("旧行必须可读");
        assert_eq!(
            old.disposition,
            RecoveryDisposition::Pending,
            "旧行必须按'仍在办'读回（不得把未知当已结账）"
        );
        assert!(!old.committed);
        assert!(
            upgraded
                .latest_release_decision(&scope())
                .expect("v2 放行决定表必须已被建出")
                .is_none(),
            "升级后放行决定表可查询（空集）"
        );
    }

    /// 首次初始化：生成身份、写侧车标记、记一条初始化事件；重开身份不变。
    #[test]
    fn first_init_registers_identity_once_and_reopen_keeps_it() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let store_id = store.store_id().as_str().to_string();
        assert!(store_id.starts_with("is-"), "身份必须带前缀：{store_id}");
        assert_eq!(store.event_count().expect("events"), 1);
        assert!(root.path().join(INPUT_SAFETY_IDENTITY_FILE).exists());
        drop(store);

        let reopened = InputSafetyStore::open_at(root.path()).expect("reopen");
        assert_eq!(reopened.store_id().as_str(), store_id, "重开不得换身份");
        assert_eq!(reopened.event_count().expect("events"), 1, "重开不得重复初始化");
    }

    /// **已登记身份后数据库丢失 ⇒ 拒绝，不静默建空库**（裁决 §1.6）。
    #[test]
    fn missing_database_after_registration_is_refused_instead_of_silently_reinitialized() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let store_id = store.store_id().as_str().to_string();
        drop(store);
        std::fs::remove_file(root.path().join(INPUT_SAFETY_DB_FILE)).expect("remove db");

        let error = InputSafetyStore::open_at(root.path()).err().expect("必须拒绝");
        assert_eq!(
            error.code(),
            "input_safety_database_missing_after_registration"
        );
        match error {
            InputSafetyStoreError::IdentityRegisteredButDatabaseMissing { store_id: found } => {
                assert_eq!(found, store_id);
            }
            other => panic!("错误分类不符：{other:?}"),
        }
        // 拒绝之后不得留下一个被"顺手重建"的空库。
        assert!(!root.path().join(INPUT_SAFETY_DB_FILE).exists());
    }

    /// 库在却没有侧车标记 ⇒ 来源不明，拒绝接管。
    #[test]
    fn database_without_marker_is_refused_as_unknown_provenance() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        drop(store);
        std::fs::remove_file(root.path().join(INPUT_SAFETY_IDENTITY_FILE)).expect("remove marker");
        let error = InputSafetyStore::open_at(root.path()).err().expect("必须拒绝");
        assert_eq!(error.code(), "input_safety_unknown_provenance");
    }

    /// 未注入根 ⇒ 明确报"未注入"，不推导路径；解析层用**显式取值**验证，**不改进程环境**。
    #[test]
    fn un_injected_root_is_reported_as_not_injected() {
        use std::ffi::OsString;
        assert_eq!(input_safety_state_root_from(None), None);
        assert_eq!(input_safety_state_root_from(Some(OsString::new())), None, "空值等于未注入");
        let injected = input_safety_state_root_from(Some(OsString::from(r"C:\state\input-safety")))
            .expect("显式取值应解析出路径");
        assert!(injected.ends_with("input-safety"));
        assert_eq!(
            InputSafetyStoreError::RootNotInjected.code(),
            "input_safety_root_not_injected"
        );
    }

    /// 阻断引用必须**逐项**通过核查；测试样式字符串必须落回 Malformed（裁决 §7.2 用例 1/2）。
    #[test]
    fn blocking_refs_are_verified_not_merely_non_empty() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let scope = scope();
        // 先建立阻断并把资源置为隔离。
        store
            .establish_incident(&InputSafetyIncident {
                incident_id: "incident-1".to_string(),
                scope: scope.clone(),
                reason: "旧执行者未确认释放".to_string(),
                original_run_ref: Some("session-db:default#run-legacy-1".to_string()),
                state: IncidentState::Pending,
                created_at_unix_ms: now_unix_ms(),
                resolved_at_unix_ms: None,
                evidence_refs: Vec::new(),
            })
            .expect("incident");
        let mut state = store.resource_state(&scope).expect("state");
        state.transition(ResourceSafetyState::Isolated, Some("coordinator-1"), 1);
        store.put_resource_state(&state).expect("put state");
        let revision = state.revision;
        let run_ref = "session-db:default#run-legacy-1";

        // 正例：六项全过。
        let good = format!("{}#incident-1", store.store_id().as_str());
        let verified = store
            .verify_blocking_ref(&good, &scope, revision, Some(run_ref))
            .expect("必须通过");
        assert!(verified.has_verified_block());
        assert_eq!(verified.incident_id(), "incident-1");
        assert_eq!(verified.revision(), revision);

        // 反例逐条对齐裁决 §7.2 的组合用例。
        assert_eq!(
            store
                .verify_blocking_ref("input-safety:incident-9", &scope, revision, Some(run_ref))
                .expect_err("测试样式字符串不得进生产"),
            BlockingRefRejection::Malformed
        );
        assert_eq!(
            store
                .verify_blocking_ref(
                    "is-ffffffffffffffffffffffffffffffff#incident-1",
                    &scope,
                    revision,
                    Some(run_ref)
                )
                .expect_err("别的 store"),
            BlockingRefRejection::UnknownStore {
                store_id: "is-ffffffffffffffffffffffffffffffff".to_string()
            }
        );
        assert_eq!(
            store
                .verify_blocking_ref(
                    &format!("{}#incident-missing", store.store_id().as_str()),
                    &scope,
                    revision,
                    Some(run_ref)
                )
                .expect_err("不存在的 incident"),
            BlockingRefRejection::IncidentNotFound {
                incident_id: "incident-missing".to_string()
            }
        );
        let other_scope = InputSafetyResourceScope::parse("windows-session-2").expect("scope");
        assert_eq!(
            store
                .verify_blocking_ref(&good, &other_scope, revision, Some(run_ref))
                .expect_err("别的 scope"),
            BlockingRefRejection::ScopeMismatch {
                expected: "windows-session-2".to_string(),
                actual: "windows-session-1".to_string()
            }
        );
        assert_eq!(
            store
                .verify_blocking_ref(&good, &scope, revision + 1, Some(run_ref))
                .expect_err("revision 不符"),
            BlockingRefRejection::RevisionMismatch {
                expected: revision + 1,
                actual: revision
            }
        );
        assert_eq!(
            store
                .verify_blocking_ref(&good, &scope, revision, Some("session-db:default#other-run"))
                .expect_err("无可解释关联"),
            BlockingRefRejection::UnexplainedAssociation {
                incident_id: "incident-1".to_string()
            }
        );

        // 已解决之后不得再作为阻断证明。
        store.resolve_incident("incident-1").expect("resolve");
        assert_eq!(
            store
                .verify_blocking_ref(&good, &scope, revision, Some(run_ref))
                .expect_err("已解决"),
            BlockingRefRejection::AlreadyResolved {
                incident_id: "incident-1".to_string()
            }
        );
    }

    /// 只有**已独立证明安全**才放开新输入；Unknown/Isolated 一律不接受。
    #[test]
    fn only_independently_proven_safe_reopens_new_input() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let scope = scope();
        let initial = store.resource_state(&scope).expect("state");
        assert_eq!(initial.state, ResourceSafetyState::Unknown);
        assert!(!initial.accepts_new_input);

        let mut isolated = initial.clone();
        isolated.transition(ResourceSafetyState::Isolated, Some("coordinator-1"), 1);
        store.put_resource_state(&isolated).expect("isolated");
        assert!(!store.resource_state(&scope).expect("state").accepts_new_input);

        // revision 不得回退。
        let mut stale = initial.clone();
        stale.transition(ResourceSafetyState::Safe, None, 1);
        stale.revision = isolated.revision;
        assert!(store.put_resource_state(&stale).is_err(), "不得用旧 revision 回写");

        // 放开方向**不能**靠直接写状态：即使填了 epoch 字段也一律拒绝（唯一放开口是协调器）。
        store
            .put_resource_state(&{
                let mut open = isolated.clone();
                open.transition(ResourceSafetyState::Safe, Some("coordinator-1"), 1);
                open
            })
            .expect_err("不得用填 epoch 字段的方式自证\"接受新输入\"");
        let (coordinator, scoped) = coordinator_for(root.path(), "reopen", "recovery-reopen");
        let reopened = coordinator
            .store()
            .reopen_new_input_authorized(&scoped, coordinator.control())
            .expect("按资格放开");
        assert!(reopened.accepts_new_input);
        assert!(coordinator
            .store()
            .resource_state(&scoped)
            .expect("state")
            .accepts_new_input);
    }


    fn incident_for(scope: &InputSafetyResourceScope, id: &str) -> InputSafetyIncident {
        InputSafetyIncident {
            incident_id: id.to_string(),
            scope: scope.clone(),
            reason: "旧执行者未确认释放".to_string(),
            original_run_ref: Some("session-db:default#run-legacy-1".to_string()),
            state: IncidentState::Pending,
            created_at_unix_ms: now_unix_ms(),
            resolved_at_unix_ms: None,
            evidence_refs: Vec::new(),
        }
    }

    fn pending_operation(scope: &InputSafetyResourceScope, epoch: u64, coordinator: &str) -> InputSafetyRecoveryOperation {
        InputSafetyRecoveryOperation {
            recovery_operation_id: "recovery-1".to_string(),
            coordinator_instance_id: coordinator.to_string(),
            recovery_epoch: epoch,
            gate_revision: 1,
            scope: scope.clone(),
            source_database_identity: "session-db:default".to_string(),
            candidate_run_ids: vec!["run-legacy-1".to_string()],
            allowed_operations: vec!["converge_legacy_cu_run".to_string()],
            stage: RecoveryStage::CoordinationAcquired,
            recorded_at_unix_ms: now_unix_ms(),
            committed: false,
            disposition: RecoveryDisposition::Pending,
        }
    }

    /// **组合用例 1**（第八轮 §4）：`paused=true` 而闸门未关闭 ⇒ `RecoveryUnauthorized`，**不写终态**。
    #[test]
    fn combined_1_recovery_without_a_held_epoch_is_unauthorized() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let scope = scope();
        store.put_recovery_operation(&pending_operation(&scope, 0, "coordinator-1")).expect("put");

        // "调用方传 paused=true 但闸门没关"的等价形态：拿不出可信资格。
        // ① 连资格对象都没有 ⇒ 类型上就调不动（这里用"别的 scope 的资格"模拟伪造尝试）。
        let (other_coordinator, _other_scope) = coordinator_for(root.path(), "c1-other", "recovery-other");
        let forged = other_coordinator.control();
        let error = store
            .advance_recovery_operation_authorized(
                "recovery-1",
                forged,
                RecoveryStage::IntentPersistedAndIntakeClosed,
            )
            .expect_err("别的 scope 的资格不得用于本 scope");
        assert_eq!(error.code(), "input_safety_recovery_unauthorized");
        // 不写终态：阶段必须原地不动。
        assert_eq!(
            store.recovery_operation("recovery-1").expect("read").expect("exists").stage,
            RecoveryStage::CoordinationAcquired,
            "未授权时不得推进阶段（更不得写终态）"
        );

        // 取得资格后放行（走协调器 = 生产入口）；注意操作必须**绑定到当前资格**，
        // 否则会被"绑在旧 epoch 的操作不得直接推进"这条规则拒绝（这正是设计意图）。
        let coordinator = coordinator_for_scope(root.path(), "c1", &scope, "recovery-1");
        // 操作行已按旧 epoch 存在，而 `put_recovery_operation` **刻意不能换绑**，
        // 因此"重新取权 → continue"必须走按资格的重新绑定（文档化的唯一路径）。
        coordinator
            .store()
            .rebind_recovery_operation_authorized("recovery-1", coordinator.control())
            .expect("重新绑定到当前资格");
        coordinator
            .store()
            .advance_recovery_operation_authorized(
                "recovery-1",
                coordinator.control(),
                RecoveryStage::IntentPersistedAndIntakeClosed,
            )
            .expect("持有资格后应放行");
    }

    /// **组合用例 2**（第八轮 §4）：双进程竞争 ⇒ epoch 冲突被拒，失败方**不能**写 incident／改状态／开新输入。
    #[test]
    fn combined_2_second_recovery_instance_is_rejected_by_epoch_conflict() {
        let root = temp_root();
        let scope = scope();
        let store_a = InputSafetyStore::open_at(root.path()).expect("open A");
        let store_b = InputSafetyStore::open_at(root.path()).expect("open B（同一库，模拟另一个进程）");
        assert_eq!(store_a.store_id(), store_b.store_id(), "同一库同一身份");

        // A 经生产入口取得协调资格（已持有跨进程命名锁）。
        let (coordinator_a, scoped) = coordinator_for(root.path(), "c2", "recovery-1");
        let owned = coordinator_a
            .store()
            .current_recovery_epoch(&scoped)
            .expect("current")
            .expect("A 持有");
        assert_eq!(owned.coordinator_instance_id, coordinator_a.control().coordinator_id());

        // ② 第二个协调器在**同一协调范围**上 begin ⇒ 被跨进程锁挡成 Busy（唯一协调者的直接证据）。
        let busy = InputSafetyCoordinator::begin_with_coordination_scope(
            root.path(),
            &coordination_scope_for("c2"),
            &scoped,
            "recovery-2",
            &["converge_legacy_cu_run"],
            std::time::Duration::from_millis(200),
        )
        .err()
        .expect("第二个协调器必须失败");
        assert_eq!(busy.code(), "input_safety_coordinator_busy", "{busy}");

        // ③ 不经协调器直接抢 epoch ⇒ epoch 冲突。
        let conflict = store_b
            .acquire_recovery_epoch(&scoped, "coordinator-B")
            .expect_err("B 必须被 epoch 冲突拒绝");
        assert_eq!(conflict.code(), "input_safety_epoch_conflict");
        match conflict {
            InputSafetyStoreError::EpochConflict { held_epoch, held_by, .. } => {
                assert_eq!(held_epoch, owned.epoch);
                assert_eq!(held_by, owned.coordinator_instance_id);
            }
            other => panic!("分类不符：{other:?}"),
        }

        // ④ B 拿不出可信资格：伪造的"别的 scope 资格"与任何非当前资格一律被拒，
        //    且**类型上**无法凭空构造 RecoveryControlGuard（字段私有、无公开构造）。
        let (other_coordinator, _other) = coordinator_for(root.path(), "c2-other", "recovery-other");
        assert_eq!(
            store_b
                .establish_incident_authorized(
                    &incident_for(&scoped, "incident-b"),
                    other_coordinator.control()
                )
                .expect_err("B 无资格")
                .code(),
            "input_safety_recovery_unauthorized"
        );
        assert!(coordinator_a.store().incident("incident-b").expect("read").is_none(), "B 不得留下事故行");
        assert_eq!(
            store_b
                .reopen_new_input_authorized(&scoped, other_coordinator.control())
                .expect_err("B 无资格")
                .code(),
            "input_safety_recovery_unauthorized"
        );
        assert!(
            !coordinator_a
                .store()
                .resource_state(&scoped)
                .expect("state")
                .accepts_new_input,
            "B 的尝试不得放开新输入"
        );
        // A 仍持有资格。
        assert_eq!(
            coordinator_a
                .store()
                .current_recovery_epoch(&scoped)
                .expect("current")
                .map(|value| value.epoch),
            Some(owned.epoch)
        );
    }

    /// **组合用例 3**（第八轮 §4）：恢复者死亡 ⇒ 启动对账要求**重新取权**；**禁止**删除 recovery 行。
    #[test]
    fn combined_3_restart_reconcile_needs_a_new_epoch_and_never_deletes_rows() {
        let root = temp_root();
        let scope = scope();
        {
            let first = InputSafetyStore::open_at(root.path()).expect("open 1");
            let owned = first.acquire_recovery_epoch(&scope, "coordinator-instance-1").expect("acquire");
            first
                .put_recovery_operation(&pending_operation(&scope, owned.epoch, "coordinator-instance-1"))
                .expect("put");
            // 崩溃：不释放资格、不提交（真实进程死亡就是这样）。
        }

        // 新实例启动：同一实例 id 仍视为"原协调者活着"，新实例 id 则要求重新取权。
        // 新实例启动，且探针判定"原持有者进程已消失"（模拟崩溃后的真实存活核查结果）。
        let restarted = InputSafetyStore::open_at(root.path())
            .expect("open 2")
            .with_liveness_probe_for_test(|_pid, _created| false);
        let live = restarted.current_recovery_epoch(&scope).expect("current").expect("仍活着");
        assert_eq!(live.coordinator_instance_id, "coordinator-instance-1");
        let reconciled = restarted.reconcile_recovery_operations_on_startup().expect("reconcile");
        assert_eq!(reconciled.len(), 1);
        match &reconciled[0] {
            RecoveryReconcileOutcome::NeedsReacquire { recovery_operation_id, .. } => {
                assert_eq!(recovery_operation_id, "recovery-1");
            }
            other => panic!("重启后必须要求重新取权：{other:?}"),
        }
        // 行**不得**被删除。
        assert!(restarted.recovery_operation("recovery-1").expect("read").is_some());

        // 重新取权（新实例 id ⇒ 新 epoch）：持有者已消失时可以回收陈旧资格，而不是被永久挡住。
        let new_coordinator = InputSafetyCoordinator::begin_with_liveness_probe_for_test(
            root.path(),
            &coordination_scope_for("c3"),
            &scope,
            "recovery-1",
            &["converge_legacy_cu_run"],
            |_pid, _created| false,
        )
        .expect("持有者已消失时应当能取得新资格");
        let reacquired = new_coordinator
            .store()
            .current_recovery_epoch(&scope)
            .expect("current")
            .expect("持有");
        assert!(reacquired.epoch > live.epoch, "重新取权必须推进 epoch");
        // 但操作仍绑在**旧** epoch/协调者上 ⇒ 未重新绑定前不得推进。
        assert_eq!(
            new_coordinator
                .store()
                .advance_recovery_operation_authorized(
                    "recovery-1",
                    new_coordinator.control(),
                    RecoveryStage::IntentPersistedAndIntakeClosed,
                )
                .expect_err("绑在旧 epoch 的操作不得被新协调者直接推进")
                .code(),
            "input_safety_recovery_unauthorized"
        );
        // 新 epoch 且由新协调者持有 ⇒ 对账转为"可继续"。
        let reconciled = restarted.reconcile_recovery_operations_on_startup().expect("reconcile");
        assert!(matches!(
            reconciled.as_slice(),
            [RecoveryReconcileOutcome::NeedsReacquire { .. }]
        ), "旧操作记录的仍是旧 epoch/协调者 ⇒ 需重新登记后才算可继续");
        // 按新资格**重新绑定**同一 recovery ID（这正是裁决 §4-3 的"重新取权 → continue"）。
        new_coordinator
            .store()
            .rebind_recovery_operation_authorized("recovery-1", new_coordinator.control())
            .expect("重新绑定");
        // 绑定后即可推进。
        new_coordinator
            .store()
            .advance_recovery_operation_authorized(
                "recovery-1",
                new_coordinator.control(),
                RecoveryStage::IntentPersistedAndIntakeClosed,
            )
            .expect("重新绑定后应可继续");
        // "仍可继续"的前提是**持有者存活**，因此只能由能证明其存活的实例判定：
        // 这里用一个走**真实进程身份**的新句柄（而非"所有持有者都已死亡"的测试探针）。
        let verifier = InputSafetyStore::open_at(root.path()).expect("open 3");
        let reconciled = verifier.reconcile_recovery_operations_on_startup().expect("reconcile");
        assert!(matches!(
            reconciled.as_slice(),
            [RecoveryReconcileOutcome::StillAuthorized { .. }]
        ), "新协调者存活且已重新登记同一 recovery ID ⇒ 可继续");
    }

    /// 恢复操作：登记幂等、阶段单调、提交后冻结。
    #[test]
    fn recovery_operations_are_idempotent_and_monotonic() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let operation = InputSafetyRecoveryOperation {
            recovery_operation_id: "recovery-1".to_string(),
            coordinator_instance_id: "coordinator-1".to_string(),
            recovery_epoch: 1,
            gate_revision: 1,
            scope: scope(),
            source_database_identity: "session-db:default".to_string(),
            candidate_run_ids: vec!["run-legacy-1".to_string()],
            allowed_operations: vec!["converge_legacy_cu_run".to_string()],
            stage: RecoveryStage::CoordinationAcquired,
            recorded_at_unix_ms: now_unix_ms(),
            committed: false,
            disposition: RecoveryDisposition::Pending,
        };
        store.put_recovery_operation(&operation).expect("put");
        store.put_recovery_operation(&operation).expect("幂等");
        let read_back = store
            .recovery_operation("recovery-1")
            .expect("read")
            .expect("exists");
        assert_eq!(read_back.stage, RecoveryStage::CoordinationAcquired);
        assert_eq!(read_back.candidate_run_ids, vec!["run-legacy-1".to_string()]);

        let mut advanced = read_back.clone();
        advanced.stage = RecoveryStage::IntentPersistedAndIntakeClosed;
        store.put_recovery_operation(&advanced).expect("advance");
        assert_eq!(
            store
                .recovery_operation("recovery-1")
                .expect("read")
                .expect("exists")
                .stage,
            RecoveryStage::IntentPersistedAndIntakeClosed
        );
        assert!(advanced.can_advance_to(RecoveryStage::InFlightRegistered));
        assert!(!advanced.can_advance_to(RecoveryStage::TerminalCommitted), "不得跳步");
    }
}
