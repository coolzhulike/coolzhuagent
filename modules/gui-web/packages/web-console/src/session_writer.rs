//! 会话库写者由操作系统句柄持有；同进程重载复用句柄，退出或崩溃自动释放。
//! 锁文件不删除，避免两个实例锁住不同文件；epoch在SQLite内递增并参与保存事务校验。
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

const EPOCH_KEY: &str = "session_writer_epoch";

#[derive(Debug)]
pub(super) struct Writer {
    _file: File,
    epoch: Mutex<Option<u64>>,
}

/// 必须在迁移、恢复、缓存加载和后台任务启动之前取得；不等待另一进程退出。
pub(super) fn acquire(db: &Path) -> io::Result<Arc<Writer>> {
    let parent = db.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let db = if db.exists() { db.canonicalize()? } else {
        parent.canonicalize()?.join(db.file_name().ok_or_else(|| io::Error::other("会话库路径缺少文件名"))?)
    };
    static WRITERS: OnceLock<Mutex<HashMap<PathBuf, Weak<Writer>>>> = OnceLock::new();
    let mut writers = WRITERS.get_or_init(Default::default).lock()
        .map_err(|_| io::Error::other("会话写者登记锁已损坏"))?;
    if let Some(writer) = writers.get(&db).and_then(Weak::upgrade) { return Ok(writer); }
    writers.retain(|_, writer| writer.strong_count() > 0);
    let mut lock_name = db.as_os_str().to_os_string(); lock_name.push(".writer.lock");
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        // 允许另一实例打开并尝试锁定，不允许持有期间删除/替换锁文件。
        options.share_mode(3);
    }
    let file = options.open(PathBuf::from(lock_name))?;
    file.try_lock().map_err(|error| io::Error::other(format!("会话库已由另一进程写入或无法取得写者锁：{error}")))?;
    let writer = Arc::new(Writer { _file: file, epoch: Mutex::new(None) });
    writers.insert(db, Arc::downgrade(&writer));
    Ok(writer)
}

impl Writer {
    /// 已由统一迁移创建metadata表；不在请求中创建第二套会话表。
    pub(super) fn claim_epoch(&self, connection: &mut Connection) -> Result<u64, String> {
        let mut owned = self.epoch.lock().map_err(|_| "会话写者版本锁已损坏")?;
        if let Some(epoch) = *owned { return Ok(epoch); }
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
        let previous: Option<String> = tx.query_row("SELECT value FROM metadata WHERE key=?1", [EPOCH_KEY], |row| row.get(0))
            .optional().map_err(|e| e.to_string())?;
        let previous = match previous { Some(value) => value.parse::<u64>().map_err(|_| "会话写者版本损坏")?, None => 0 };
        let epoch = previous.checked_add(1).ok_or("会话写者版本已耗尽")?;
        tx.execute("INSERT INTO metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![EPOCH_KEY, epoch.to_string()]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        *owned = Some(epoch);
        Ok(epoch)
    }

    pub(super) fn epoch(&self) -> Result<u64, String> {
        self.epoch.lock().map_err(|_| "会话写者版本锁已损坏".to_string())?
            .ok_or_else(|| "会话写者尚未取得持久版本".to_string())
    }
}

pub(super) fn check_epoch(connection: &Connection, expected: u64) -> Result<(), String> {
    let actual: Option<String> = connection.query_row("SELECT value FROM metadata WHERE key=?1", [EPOCH_KEY], |row| row.get(0))
        .optional().map_err(|e| e.to_string())?;
    if actual.as_deref() != Some(expected.to_string().as_str()) {
        return Err("会话库写者版本已变化，未提交旧缓存；请重启并核对数据库".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn lock_child_probe() {
        let Some(path) = std::env::var_os("COOLZHU_WRITER_TEST_PATH") else { return; };
        let path = PathBuf::from(path);
        let mode = std::env::var("COOLZHU_WRITER_TEST_MODE").unwrap();
        if mode == "reject" { assert!(acquire(&path).is_err()); return; }
        let _writer = acquire(&path).expect("子进程取得写者锁");
        if mode == "hold" {
            std::fs::write(path.with_extension("ready"), "已持锁").unwrap();
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }

    fn child(path: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "session_writer::tests::lock_child_probe", "--nocapture"])
            .env("COOLZHU_WRITER_TEST_PATH", path).env("COOLZHU_WRITER_TEST_MODE", mode);
        command
    }

    #[test]
    fn competing_process_rejected_and_crash_releases_lock() {
        let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("session.sqlite3");
        let first = acquire(&path).unwrap();
        assert!(Arc::ptr_eq(&first, &acquire(&path).unwrap()));
        assert!(child(&path, "reject").status().unwrap().success());
        drop(first);
        assert!(child(&path, "acquire").status().unwrap().success());
        let mut holder = child(&path, "hold").spawn().unwrap();
        let start = std::time::Instant::now();
        while !path.with_extension("ready").exists() && start.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let ready = path.with_extension("ready").exists();
        let refused = acquire(&path).is_err();
        holder.kill().unwrap(); holder.wait().unwrap();
        assert!(ready && refused, "子进程确实持锁且父进程被拒绝");
        assert!(acquire(&path).is_ok(), "崩溃后由操作系统释放锁");
    }

    #[test]
    fn epoch_survives_reload_and_fences_stale_save() {
        let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("session.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);").unwrap();
        let writer = acquire(&path).unwrap();
        assert_eq!(writer.claim_epoch(&mut connection).unwrap(), 1);
        assert_eq!(acquire(&path).unwrap().claim_epoch(&mut connection).unwrap(), 1);
        check_epoch(&connection, writer.epoch().unwrap()).unwrap();
        drop(writer);
        let next = acquire(&path).unwrap();
        assert_eq!(next.claim_epoch(&mut connection).unwrap(), 2);
        assert!(check_epoch(&connection, 1).is_err());
        check_epoch(&connection, 2).unwrap();
    }
}
