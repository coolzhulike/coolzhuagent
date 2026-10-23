//! 非权威诊断异步写出，避免日志轮转/配额扫描占用会话锁和启动关键路径。
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, OnceLock,
    },
    time::Duration,
};

enum Entry {
    Line(PathBuf, String),
    Flush(mpsc::Sender<()>),
}
struct Writer {
    sender: mpsc::SyncSender<Entry>,
    omitted: Arc<AtomicU64>,
}
static WRITER: OnceLock<Writer> = OnceLock::new();

fn writer() -> &'static Writer {
    WRITER.get_or_init(|| {
        // 512 × 64 KiB 上限；日志不是聊天/用量/权限的事实来源。
        let (sender, receiver) = mpsc::sync_channel(512);
        let omitted = Arc::new(AtomicU64::new(0));
        let count = omitted.clone();
        std::thread::Builder::new()
            .name("diagnostic-writer".into())
            .spawn(move || {
                for entry in receiver {
                    match entry {
                        Entry::Line(path, message) => {
                            let skipped = count.swap(0, Ordering::Relaxed);
                            if skipped > 0 {
                                eprintln!(
                                    "[DIAG] 诊断队列已满，省略 {skipped} 条；权威业务记录不受影响"
                                );
                            }
                            if let Err(error) = diagnostics::append_diagnostic_line(&path, &message)
                            {
                                eprintln!("[DIAG] 写入诊断失败：{error}");
                            }
                        }
                        Entry::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .expect("创建诊断写入线程");
        Writer { sender, omitted }
    })
}

pub(super) fn append(path: PathBuf, message: &str) {
    let message = if message.len() >= 64 * 1024 {
        format!("diagnostic_record_omitted original_bytes={}", message.len())
    } else {
        message.to_string()
    };
    let writer = writer();
    // 冻结入队时的工程路径，切换工程不会把旧记录写入新工程。
    if writer.sender.try_send(Entry::Line(path, message)).is_err()
        && writer.omitted.fetch_add(1, Ordering::Relaxed) == 0
    {
        eprintln!("[DIAG] 诊断队列拥塞，开始计数省略记录");
    }
}

pub(super) fn flush() {
    let Some(writer) = WRITER.get() else { return };
    let (sender, receiver) = mpsc::channel();
    if writer.sender.try_send(Entry::Flush(sender)).is_err()
        || receiver.recv_timeout(Duration::from_secs(2)).is_err()
    {
        eprintln!("[DIAG] 退出前诊断写入未完全排空");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn 按入队时路径写入且退出排空并脱敏() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("first.log");
        let b = root.path().join("second.log");
        append(a.clone(), "first api_key=sk-123456789012345678901234567890");
        append(b.clone(), "second");
        flush();
        let text = std::fs::read_to_string(a).unwrap();
        assert!(text.contains("first"));
        assert!(!text.contains("12345678901234567890"));
        assert_eq!(std::fs::read_to_string(b).unwrap(), "second\n");
    }
}
