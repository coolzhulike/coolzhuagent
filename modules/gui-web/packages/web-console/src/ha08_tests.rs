//! HA08 并发测试观测：只在 test 构建中统计读写重叠；不参与生产决策。
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicUsize, Ordering};

static ACTIVE_READS: AtomicUsize = AtomicUsize::new(0);
static MAX_READS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_WRITES: AtomicUsize = AtomicUsize::new(0);
static MAX_WRITES: AtomicUsize = AtomicUsize::new(0);
static READ_BARRIER: OnceLock<Mutex<Option<Arc<tokio::sync::Barrier>>>> = OnceLock::new();

fn read_barrier() -> &'static Mutex<Option<Arc<tokio::sync::Barrier>>> {
    READ_BARRIER.get_or_init(|| Mutex::new(None))
}

pub(super) fn reset() {
    ACTIVE_READS.store(0, Ordering::SeqCst);
    MAX_READS.store(0, Ordering::SeqCst);
    ACTIVE_WRITES.store(0, Ordering::SeqCst);
    MAX_WRITES.store(0, Ordering::SeqCst);
    if let Ok(mut barrier) = read_barrier().lock() { *barrier = None; }
}

pub(super) fn install_read_barrier(barrier: Arc<tokio::sync::Barrier>) {
    if let Ok(mut slot) = read_barrier().lock() { *slot = Some(barrier); }
}

pub(super) fn max_reads() -> usize { MAX_READS.load(Ordering::SeqCst) }
pub(super) fn max_writes() -> usize { MAX_WRITES.load(Ordering::SeqCst) }

pub(super) async fn before_dispatch(safe_read: bool) {
    if safe_read {
        let active = ACTIVE_READS.fetch_add(1, Ordering::SeqCst) + 1;
        MAX_READS.fetch_max(active, Ordering::SeqCst);
        let barrier = read_barrier().lock().ok().and_then(|slot| slot.clone());
        if let Some(barrier) = barrier { barrier.wait().await; }
    } else {
        let active = ACTIVE_WRITES.fetch_add(1, Ordering::SeqCst) + 1;
        MAX_WRITES.fetch_max(active, Ordering::SeqCst);
    }
}

pub(super) fn after_dispatch(safe_read: bool) {
    if safe_read { ACTIVE_READS.fetch_sub(1, Ordering::SeqCst); }
    else { ACTIVE_WRITES.fetch_sub(1, Ordering::SeqCst); }
}
