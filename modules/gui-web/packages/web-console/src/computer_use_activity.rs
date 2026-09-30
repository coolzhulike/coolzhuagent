//! 当前进程的真实 CU 活动展示租约；不从历史库或模型自述推断活动，不参与权限决策。
use std::{collections::HashMap, sync::{Mutex, OnceLock, atomic::{AtomicU64, Ordering}}, time::{Duration, Instant}};
use axum::{http::{HeaderMap, StatusCode, header}, response::IntoResponse, routing::get, Json, Router};
use native_browser_protocol::{ComputerUseActivityReceipt, ACTIVITY_PATH, ACTIVITY_LEASE_MILLIS};

fn activities() -> &'static Mutex<HashMap<u64, Instant>> {
    static ACTIVITIES: OnceLock<Mutex<HashMap<u64, Instant>>> = OnceLock::new();
    ACTIVITIES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) struct ActivityLease(u64);
impl ActivityLease {
    /// 调用方必须已通过接纳、互锁、输入所有权和适配器准备。
    pub(super) fn begin(remaining: Duration) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut active) = activities().lock() {
            active.insert(id, Instant::now() + remaining);
        }
        Self(id)
    }
}
impl Drop for ActivityLease {
    fn drop(&mut self) {
        if let Ok(mut active) = activities().lock() { active.remove(&self.0); }
    }
}

fn receipt() -> ComputerUseActivityReceipt {
    let now = Instant::now();
    let remaining = activities().lock().ok().and_then(|active|
        active.values().map(|deadline| deadline.saturating_duration_since(now)).max())
        .unwrap_or_default().as_millis().min(ACTIVITY_LEASE_MILLIS as u128) as u64;
    ComputerUseActivityReceipt { active:remaining > 0, lease_ms:remaining }
}

async fn activity(headers: HeaderMap) -> Result<axum::response::Response, StatusCode> {
    if !crate::native_browser_host::authenticated(&headers) { return Err(StatusCode::UNAUTHORIZED); }
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(receipt())).into_response())
}

pub(super) fn routes() -> Router { Router::new().route(ACTIVITY_PATH, get(activity)) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_scope_revokes_on_return_unwind_and_deadline() {
        let lease = ActivityLease::begin(Duration::from_secs(10));
        assert!(activities().lock().unwrap().contains_key(&lease.0));
        let id = lease.0;
        drop(lease);
        assert!(!activities().lock().unwrap().contains_key(&id));
        let id = std::cell::Cell::new(0);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let lease = ActivityLease::begin(Duration::from_secs(10));
            id.set(lease.0);
            panic!("验证展开时租约释放");
        }));
        assert!(!activities().lock().unwrap().contains_key(&id.get()));
        let expired = ActivityLease::begin(Duration::ZERO);
        assert!(activities().lock().unwrap()[&expired.0] <= Instant::now());
        assert!(receipt().valid_shape());
        // 真实路由始终需要宿主凭据，网页不能无认证查询或触发展示。
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert_eq!(rt.block_on(activity(HeaderMap::new())).unwrap_err(), StatusCode::UNAUTHORIZED);
    }
}
