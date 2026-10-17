//! 配置文件与会话事务的崩溃恢复日志，复用原 SQLite 的 metadata。
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

const KEY: &str = "session_config_journal";

#[derive(Serialize, Deserialize)]
struct Journal {
    operation: String,
    config_path: std::path::PathBuf,
    committed: bool,
    before: String,
    after: String,
}

fn read(connection: &Connection) -> Result<Option<Journal>, String> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='metadata')",
        [], |row| row.get(0),
    ).map_err(|_| "无法检查配置恢复日志")?;
    if !exists { return Ok(None); }
    let value: Option<String> = connection.query_row(
        "SELECT value FROM metadata WHERE key=?1", [KEY], |row| row.get(0),
    ).optional().map_err(|_| "无法读取配置恢复日志")?;
    value.map(|value| serde_json::from_str(&value).map_err(|_| "配置恢复日志损坏".to_string())).transpose()
}

pub(super) fn ensure_resolved(db: &Path) -> Result<(), String> {
    if !db.exists() { return Ok(()); }
    let connection = super::open_session_connection(db).map_err(|_| "无法检查配置发布状态")?;
    if read(&connection)?.is_some() { return Err("配置操作日志尚未恢复，暂不接受其他配置写入".into()); }
    Ok(())
}

pub(super) fn verify_rollback_allowed(db: &Path) -> Result<(), String> {
    let connection = super::open_session_connection(db).map_err(|_| "无法核对配置事务结果，保留日志等待重启恢复")?;
    if read(&connection)?.is_some_and(|journal| journal.committed) {
        return Err("会话写入返回错误，但提交标记已存在；保留日志，请重启核对已提交状态".into());
    }
    Ok(())
}

pub(super) fn begin(db: &Path, config: &Path, before: &str, after: &str) -> Result<String, String> {
    let mut connection = super::open_session_connection(db).map_err(|_| "无法打开配置恢复库")?;
    let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| "无法领取配置发布事务")?;
    if read(&tx)?.is_some() { return Err("已有配置操作尚未恢复，未发布新配置".into()); }
    let operation = super::random_hex_identifier(24, "配置发布")?;
    let value = serde_json::to_string(&Journal {
        operation: operation.clone(), committed: false,
        config_path: config.canonicalize().map_err(|_| "配置发布路径无法冻结")?,
        before: super::credential_store::protect(before)?,
        after: super::credential_store::protect(after)?,
    }).map_err(|_| "配置恢复日志无法编码")?;
    tx.execute("INSERT INTO metadata(key,value) VALUES (?1,?2)", rusqlite::params![KEY,value])
        .map_err(|_| "配置恢复日志未写入，配置未发布")?;
    tx.commit().map_err(|_| "配置恢复日志未提交，配置未发布")?;
    Ok(operation)
}

/// 必须由写入会话的同一个 SQLite 事务调用。
pub(super) fn commit(connection: &Connection, operation: &str) -> Result<(), String> {
    let mut journal = read(connection)?.ok_or("配置发布日志丢失，拒绝会话提交")?;
    if journal.operation != operation || journal.committed {
        return Err("配置发布日志不属于当前操作，拒绝会话提交".into());
    }
    journal.committed = true;
    let value = serde_json::to_string(&journal).map_err(|_| "配置提交标记无法编码")?;
    connection.execute("UPDATE metadata SET value=?2 WHERE key=?1", rusqlite::params![KEY,value])
        .map_err(|_| "配置提交标记无法写入")?;
    Ok(())
}

/// 调用者已占有库写者。恢复前不得启动任务或加载可见配置。
pub(super) fn recover(db: &Path, config: &Path) -> Result<(), String> {
    if !db.exists() { return Ok(()); }
    let connection = super::open_session_connection(db).map_err(|_| "无法打开配置恢复库")?;
    let Some(journal) = read(&connection)? else { return Ok(()); };
    if config.canonicalize().map_err(|_| "恢复配置路径无法核对，保留日志")? != journal.config_path {
        return Err("恢复日志属于其他配置文件，未修改当前工程配置".into());
    }
    let before = super::credential_store::reveal(&journal.before)?;
    let after = super::credential_store::reveal(&journal.after)?;
    let current = std::fs::read_to_string(config).map_err(|_| "无法读取待恢复配置，保留操作日志")?;
    if current != before && current != after {
        return Err("配置文件被独立修改，未覆盖外部内容；请核对配置恢复日志".into());
    }
    let expected = if journal.committed { &after } else { &before };
    if &current != expected { super::config_publication::publish(config, expected)?; }
    connection.execute("DELETE FROM metadata WHERE key=?1", [KEY])
        .map_err(|_| "配置已恢复，但操作日志清理失败")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn service_child_probe() {
        if std::env::var_os("COOLZHU_CONFIG_SERVICE_TEST").is_none() { return; }
        let sessions = super::super::session_store();
        let id = sessions.lock().unwrap().state.sessions[0].id.clone();
        let service = super::super::session_config_service::SessionConfigService::new(sessions, &axum::http::HeaderMap::new()).unwrap();
        let payload = serde_json::from_value(serde_json::json!({
            "parameters":{"context_window":8192,"max_output_tokens":1024},
            "session":{"name":"崩溃恢复接线测试","provider":"custom","model":"journal-test-model"}
        })).unwrap();
        service.save(id.clone(), payload).unwrap();
        let snapshot = service.read(id.clone()).unwrap();
        assert_eq!(snapshot.session.name, "崩溃恢复接线测试");
        assert_eq!(snapshot.parameters.context_window, 8192);
        let db = super::super::default_session_sqlite_path();
        super::ensure_resolved(&db).unwrap();
        let connection = Connection::open(&db).unwrap();
        let model: String = connection.query_row("SELECT model FROM sessions WHERE id=?1", [&id], |row| row.get(0)).unwrap();
        assert_eq!(model, "journal-test-model");
        connection.execute_batch("CREATE TRIGGER reject_config_session BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT, 'test rollback'); END;").unwrap();
        let failing = serde_json::from_value(serde_json::json!({
            "parameters":{"context_window":16384,"max_output_tokens":2048},
            "session":{"name":"不应提交的名称"},
            "expected_revision":snapshot.configuration_revision
        })).unwrap();
        assert!(service.save(id.clone(), failing).is_err());
        connection.execute_batch("DROP TRIGGER reject_config_session").unwrap();
        let restored = service.read(id).unwrap();
        assert_eq!(restored.session.name, "崩溃恢复接线测试");
        assert_eq!(restored.parameters.context_window, 8192);
        assert_eq!(restored.configuration_revision, snapshot.configuration_revision);
        let config = super::super::load_workspace_config_at(&super::super::active_workspace_path());
        assert_eq!(config.configuration_revision, snapshot.configuration_revision);
        super::ensure_resolved(&db).unwrap();
    }

    #[test]
    fn real_service_publishes_session_and_parameters_without_lock_reentry() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "session_config_journal::tests::service_child_probe", "--nocapture"])
            .env("COOLZHU_CONFIG_SERVICE_TEST", "yes")
            .env("COOLZHU_RUNTIME_DIR", dir.path())
            .env("COOLZHU_WEB_SESSION_DB", dir.path().join("sessions.sqlite3"))
            .spawn().unwrap();
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() { assert!(status.success()); break; }
            if start.elapsed().as_secs() >= 30 {
                child.kill().unwrap(); child.wait().unwrap();
                panic!("配置服务疑似锁重入，测试子进程已回收");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn crash_child_probe() {
        let Some(root) = std::env::var_os("COOLZHU_CONFIG_JOURNAL_TEST_ROOT") else { return; };
        let root = std::path::PathBuf::from(root);
        let db = root.join("sessions.sqlite3");
        let config = root.join("coolzhu.toml");
        let _writer = super::super::session_writer::acquire(&db).unwrap();
        let operation = super::begin(&db, &config, "version=1\n", "version=2\n").unwrap();
        super::super::config_publication::publish(&config, "version=2\n").unwrap();
        let mut connection = Connection::open(&db).unwrap();
        let tx = connection.transaction().unwrap();
        tx.execute("INSERT INTO session_probe(value) VALUES (2)", []).unwrap();
        super::commit(&tx, &operation).unwrap();
        if std::env::var("COOLZHU_CONFIG_JOURNAL_TEST_COMMIT").unwrap() == "yes" {
            tx.commit().unwrap();
        }
        // 退出不运行析构：SQLite 未提交事务由下一进程恢复，操作系统释放写者锁。
        std::process::exit(71);
    }

    #[test]
    fn abrupt_child_exit_recovers_both_sides_of_transaction_boundary() {
        for committed in [false, true] {
            let (dir, db, config) = setup();
            Connection::open(&db).unwrap().execute_batch("CREATE TABLE session_probe(value INTEGER)").unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "session_config_journal::tests::crash_child_probe", "--nocapture"])
                .env("COOLZHU_CONFIG_JOURNAL_TEST_ROOT", dir.path())
                .env("COOLZHU_CONFIG_JOURNAL_TEST_COMMIT", if committed { "yes" } else { "no" })
                .status().unwrap();
            assert_eq!(status.code(), Some(71), "子进程必须到达故障注入点");
            let _writer = super::super::session_writer::acquire(&db).unwrap();
            super::recover(&db, &config).unwrap();
            super::recover(&db, &config).unwrap();
            let count: i64 = Connection::open(&db).unwrap().query_row("SELECT COUNT(*) FROM session_probe", [], |row| row.get(0)).unwrap();
            assert_eq!(count, if committed { 1 } else { 0 });
            assert_eq!(std::fs::read_to_string(config).unwrap(), if committed { "version=2\n" } else { "version=1\n" });
        }
    }

    fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let config = dir.path().join("coolzhu.toml");
        Connection::open(&db).unwrap().execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)").unwrap();
        std::fs::write(&config, "version=1\n").unwrap();
        (dir, db, config)
    }
    #[test]
    fn restart_before_session_commit_restores_configuration_and_is_idempotent() {
        let (_dir, db, config) = setup();
        super::begin(&db,&config,"version=1\n","version=2\n").unwrap();
        super::super::config_publication::publish(&config,"version=2\n").unwrap();
        super::recover(&db,&config).unwrap();
        super::recover(&db,&config).unwrap();
        assert_eq!(std::fs::read_to_string(config).unwrap(),"version=1\n");
    }
    #[test]
    fn committed_marker_is_atomic_with_session_transaction() {
        let (_dir, db, config) = setup();
        let operation=super::begin(&db,&config,"version=1\n","version=2\n").unwrap();
        super::super::config_publication::publish(&config,"version=2\n").unwrap();
        let mut connection=Connection::open(&db).unwrap();
        { let tx=connection.transaction().unwrap(); super::commit(&tx,&operation).unwrap(); }
        super::recover(&db,&config).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(),"version=1\n");
        let operation=super::begin(&db,&config,"version=1\n","version=2\n").unwrap();
        super::super::config_publication::publish(&config,"version=2\n").unwrap();
        let tx=connection.transaction().unwrap(); super::commit(&tx,&operation).unwrap(); tx.commit().unwrap();
        assert!(super::verify_rollback_allowed(&db).is_err(), "提交结果不可作为失败回退处理");
        super::recover(&db,&config).unwrap();
        assert_eq!(std::fs::read_to_string(config).unwrap(),"version=2\n");
    }
    #[test]
    fn external_edit_and_unfinished_operation_are_not_overwritten() {
        let (_dir, db, config) = setup();
        super::begin(&db,&config,"version=1\n","version=2\n").unwrap();
        assert!(super::begin(&db,&config,"version=1\n","version=3\n").is_err());
        let wrong = config.with_file_name("other.toml");
        std::fs::write(&wrong, "version=1\n").unwrap();
        assert!(super::recover(&db, &wrong).is_err());
        assert_eq!(std::fs::read_to_string(wrong).unwrap(), "version=1\n");
        std::fs::write(&config,"external=3\n").unwrap();
        assert!(super::recover(&db,&config).is_err());
        assert_eq!(std::fs::read_to_string(config).unwrap(),"external=3\n");
    }
}
