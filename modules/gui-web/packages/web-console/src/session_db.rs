//! 会话库连接复用：WAL 模式下最后一个连接关闭会触发检查点并删除/重建
//! `-wal`/`-shm`，外置或机械盘上一次"打开+提交+关闭"可达数百毫秒到数秒。
//! 池化后归还即复用，热路径保持页缓存与 WAL 索引常驻。
//!
//! 安全边界：
//! - 借出即恢复连接级默认状态（外键/query_only/synchronous/busy_timeout/temp 表）；
//! - 处于事务中的连接不归还（事务未结束说明调用方泄漏，直接弃用）；
//! - 借用方仍可依赖 WAL 读一致性：连接上的写入对其他借用方按常规可见；
//! - `cfg(test)` 下 `checkout` 恒为 None、Drop 不归还，保住测试隔离与"删库重建"习惯。

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

use rusqlite::Connection;

const MAX_POOLED_PER_PATH: usize = 4;

static FRESH_OPENS: AtomicU64 = AtomicU64::new(0);
static REUSES: AtomicU64 = AtomicU64::new(0);

fn pool() -> &'static Mutex<HashMap<PathBuf, Vec<Connection>>> {
    static POOL: OnceLock<Mutex<HashMap<PathBuf, Vec<Connection>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

/// (新建连接数, 复用次数)；供轮次级诊断与基准断言。
pub(crate) fn counters() -> (u64, u64) {
    (FRESH_OPENS.load(Ordering::Relaxed), REUSES.load(Ordering::Relaxed))
}

fn reset_temp_objects(connection: &Connection) {
    let mut stmt = match connection.prepare(
        "SELECT type, name FROM temp.sqlite_master WHERE name NOT LIKE 'sqlite_%'",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return,
    };
    let objects: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    drop(stmt);
    for (kind, name) in objects {
        let keyword = match kind.as_str() {
            "table" => "TABLE",
            "view" => "VIEW",
            "trigger" => "TRIGGER",
            _ => continue,
        };
        let _ = connection.execute_batch(&format!(
            "DROP {keyword} IF EXISTS temp.\"{}\"",
            name.replace('"', "\"\"")
        ));
    }
}

/// 归还前把连接恢复到新建时的默认状态。任一步失败视为连接损坏，弃用不池化。
fn reset_connection(connection: &Connection) -> rusqlite::Result<()> {
    reset_temp_objects(connection);
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;\
         PRAGMA query_only = OFF;\
         PRAGMA synchronous = NORMAL;\
         PRAGMA busy_timeout = 5000;",
    )
}

fn return_to_pool(path: PathBuf, connection: Connection) {
    if !connection.is_autocommit() {
        return;
    }
    if reset_connection(&connection).is_err() {
        return;
    }
    let mut guard = pool().lock().unwrap_or_else(|error| error.into_inner());
    let entries = guard.entry(path).or_default();
    if entries.len() < MAX_POOLED_PER_PATH {
        entries.push(connection);
    }
}

fn checkout_inner(path: &Path) -> Option<PooledConnection> {
    let mut guard = pool().lock().unwrap_or_else(|error| error.into_inner());
    let connection = guard.get_mut(path).and_then(Vec::pop)?;
    REUSES.fetch_add(1, Ordering::Relaxed);
    Some(PooledConnection::new(connection, path.to_path_buf(), true))
}

/// 借出同路径的已校验连接；没有可用连接时返回 None，由调用方走完整打开流程。
/// `cfg(test)` 恒为 None：测试里每个 `open_session_connection` 仍是新连接。
pub(crate) fn checkout(path: &Path) -> Option<PooledConnection> {
    #[cfg(test)]
    {
        let _ = path;
        None
    }
    #[cfg(not(test))]
    {
        checkout_inner(path)
    }
}

pub(crate) fn record_fresh_open() {
    FRESH_OPENS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) struct PooledConnection {
    connection: Option<Connection>,
    path: PathBuf,
}

impl PooledConnection {
    pub(crate) fn new(connection: Connection, path: PathBuf, _reused: bool) -> Self {
        Self { connection: Some(connection), path }
    }

    /// 显式放弃归还（连接损坏或需要独占语义时使用）。
    pub(crate) fn discard(mut self) {
        self.connection.take();
    }

    /// 拆出底层连接、放弃归还；给长期持有方（如 Mutex<Connection> 存储体）使用。
    pub(crate) fn into_inner(mut self) -> Connection {
        self.connection.take().expect("pooled connection already taken")
    }
}

impl Deref for PooledConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.connection.as_ref().expect("pooled connection already taken")
    }
}

impl DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().expect("pooled connection already taken")
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        let Some(connection) = self.connection.take() else { return };
        #[cfg(not(test))]
        return_to_pool(self.path.clone(), connection);
        #[cfg(test)]
        drop(connection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returned_connection_comes_back_with_clean_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pool.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE data(value INTEGER); INSERT INTO data VALUES (1);
                 CREATE TEMP TABLE scratch(value INTEGER);
                 PRAGMA query_only = ON;",
            )
            .unwrap();
        return_to_pool(path.clone(), connection);

        let reused = checkout_inner(&path).expect("归还后必须可复用");
        assert!(reused.is_autocommit());
        let temp_count: i64 = reused
            .query_row("SELECT COUNT(*) FROM temp.sqlite_master", [], |row| row.get(0))
            .unwrap();
        assert_eq!(temp_count, 0, "temp 表必须随归还清理");
        // query_only 已复位，写操作可用。
        reused.execute("INSERT INTO data VALUES (2)", []).unwrap();
        let count: i64 = reused.query_row("SELECT COUNT(*) FROM data", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn mid_transaction_connection_is_not_pooled() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pool.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE data(value INTEGER)").unwrap();
        connection.execute_batch("BEGIN").unwrap();
        return_to_pool(path.clone(), connection);
        assert!(checkout_inner(&path).is_none(), "事务中的连接不得入池");
    }
}
