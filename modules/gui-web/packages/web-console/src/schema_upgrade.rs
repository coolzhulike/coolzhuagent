//! 会话库升级的备份与事务边界；不自动恢复、不降低前向版本、不删除迁移后的新数据。
use std::{path::{Path, PathBuf}, time::{Duration, Instant}};
use rusqlite::{backup::{Backup, StepResult}, Connection, OpenFlags, OptionalExtension};

fn failure(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(message.into())))
}

/// 调用方先检查前向版本，迁移步骤仍由既有唯一阶梯持有。
pub(crate) fn run(connection: &Connection, target: i64, steps: impl FnOnce(&Connection) -> rusqlite::Result<()>, validate: impl Fn(&Connection) -> rusqlite::Result<()>) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current > target { return Err(failure("会话库版本超前，未备份、未修改")); }
    if current == target && complete(connection, target)? && validate(connection).is_ok() { return Ok(()); }
    if !connection.is_autocommit() { return Err(failure("会话库升级不能嵌入业务写事务")); }
    connection.execute_batch("BEGIN IMMEDIATE")?;
    struct Rollback<'a>(&'a Connection, bool);
    impl Drop for Rollback<'_> { fn drop(&mut self) { if !self.1 { let _ = self.0.execute_batch("ROLLBACK"); } } }
    let mut guard = Rollback(connection, false);
    // 等待写锁时另一实例可能已完成升级，必须在锁内再次检查。
    let from: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if from > target { return Err(failure("等待期间会话库已由更新版本升级")); }
    if from == target && complete(connection, target)? && validate(connection).is_ok() {
        connection.execute_batch("COMMIT")?; guard.1 = true; return Ok(());
    }
    let table_count: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |row| row.get(0))?;
    if table_count > 0 {
        if let Some(path) = connection.path().filter(|path| !path.is_empty()) {
            backup_while_write_locked(Path::new(path), from, target)?;
        }
    }
    steps(connection)?;
    let reached: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if reached != target { return Err(failure("迁移阶梯没有到达目标版本")); }
    validate(connection)?;
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" { return Err(failure("迁移后完整性检查失败，已回滚")); }
    connection.execute("INSERT OR REPLACE INTO metadata(key,value) VALUES ('schema_contract_version',?1)", [target.to_string()])?;
    connection.execute_batch("COMMIT")?;
    guard.1 = true;
    Ok(())
}

fn complete(connection: &Connection, target: i64) -> rusqlite::Result<bool> {
    let exists: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='metadata'", [], |row| row.get(0))?;
    if exists == 0 { return Ok(false); }
    Ok(connection.query_row("SELECT value FROM metadata WHERE key='schema_contract_version'", [], |row| row.get::<_, String>(0))
        .optional()?.as_deref() == Some(target.to_string().as_str()))
}

fn backup_while_write_locked(path: &Path, from: i64, target: i64) -> rusqlite::Result<PathBuf> {
    let root = path.parent().ok_or_else(|| failure("数据库路径没有父目录"))?.join("schema-backups");
    std::fs::create_dir_all(&root).map_err(|error| failure(error.to_string()))?;
    if !std::fs::symlink_metadata(&root).map_err(|error| failure(error.to_string()))?.file_type().is_dir() {
        return Err(failure("迁移备份目录不能是链接"));
    }
    let temporary = tempfile::NamedTempFile::new_in(&root).map_err(|error| failure(error.to_string()))?;
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_millis(100))?;
    let mut destination = Connection::open(temporary.path())?;
    let started = Instant::now();
    {
        let backup = Backup::new(&source, &mut destination)?;
        loop {
            if started.elapsed() > Duration::from_secs(60) { return Err(failure("迁移前备份超时，未执行迁移")); }
            match backup.step(256)? {
                StepResult::Done => break,
                StepResult::More => {},
                StepResult::Busy | StepResult::Locked => std::thread::sleep(Duration::from_millis(10)),
                _ => return Err(failure("未知备份状态，未执行迁移")),
            }
        }
    }
    let integrity: String = destination.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    let version: i64 = destination.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if integrity != "ok" || version != from { return Err(failure("迁移前备份校验失败，未执行迁移")); }
    destination.execute_batch("PRAGMA journal_mode=DELETE")?;
    destination.close().map_err(|(_, error)| error)?;
    temporary.as_file().sync_all().map_err(|error| failure(error.to_string()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|error| failure(error.to_string()))?.as_nanos();
    let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or("sessions");
    let saved = root.join(format!("{name}-before-v{from}-to-v{target}-{stamp}.sqlite3"));
    temporary.persist_noclobber(&saved).map_err(|error| failure(error.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(&root).and_then(|directory| directory.sync_all()).map_err(|error| failure(error.to_string()))?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    #[test]
    fn 未检查点的历史进入备份且迁移失败回滚() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.sqlite3");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE history(content TEXT); INSERT INTO history VALUES ('中文旧记录'); PRAGMA user_version=1;").unwrap();
        let failed = super::run(&connection, 2, |connection| {
            connection.execute_batch("ALTER TABLE history ADD COLUMN changed TEXT; PRAGMA user_version=2;")?;
            Err(super::failure("注入迁移失败"))
        }, |_| Ok(()));
        assert!(failed.is_err());
        assert_eq!(connection.query_row("PRAGMA user_version", [], |row| row.get::<_,i64>(0)).unwrap(), 1);
        assert!(connection.prepare("SELECT changed FROM history").is_err());
        let backup = std::fs::read_dir(temp.path().join("schema-backups")).unwrap().next().unwrap().unwrap().path();
        let saved = rusqlite::Connection::open(&backup).unwrap();
        assert_eq!(saved.query_row("SELECT content FROM history", [], |row| row.get::<_,String>(0)).unwrap(), "中文旧记录");
        assert_eq!(saved.query_row("PRAGMA quick_check", [], |row| row.get::<_,String>(0)).unwrap(), "ok");
        drop(saved);
        super::run(&connection, 2, |connection| connection.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT); ALTER TABLE history ADD COLUMN changed TEXT; PRAGMA user_version=2;"), |connection| connection.prepare("SELECT changed FROM history").map(|_| ())).unwrap();
        let count = std::fs::read_dir(temp.path().join("schema-backups")).unwrap().count();
        super::run(&connection, 2, |_| panic!("完整当前库不可重复迁移"), |connection| connection.prepare("SELECT changed FROM history").map(|_| ())).unwrap();
        assert_eq!(std::fs::read_dir(temp.path().join("schema-backups")).unwrap().count(), count);
    }
}
