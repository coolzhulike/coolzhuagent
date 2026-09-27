//! 定时任务的单次权限上下文。不写入会话授权表，也不随新 tokio task 隐式传播。
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct ScheduledExecutionGrant {
    workspace_id: String,
    session_id: String,
    room_id: String,
    expires_at: Instant,
    revoked: AtomicBool,
}

impl ScheduledExecutionGrant {
    pub(super) fn new(
        workspace_id: String,
        session_id: String,
        room_id: String,
        ttl: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            workspace_id,
            session_id,
            room_id,
            expires_at: Instant::now() + ttl,
            revoked: AtomicBool::new(false),
        })
    }

    /// 执行器在真正开始时复核，而非在排队前缓存一个永不过期的 bool。
    pub(super) fn authorizes(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        room_id: Option<&str>,
    ) -> bool {
        !self.revoked.load(Ordering::Acquire)
            && Instant::now() < self.expires_at
            && self.workspace_id == workspace_id
            && session_id == Some(self.session_id.as_str())
            && room_id == Some(self.room_id.as_str())
    }
}

tokio::task_local! {
    static CURRENT: Option<Arc<ScheduledExecutionGrant>>;
}

pub(super) fn current() -> Option<Arc<ScheduledExecutionGrant>> {
    CURRENT.try_with(Clone::clone).ok().flatten()
}

struct RevokeOnDrop(Option<Arc<ScheduledExecutionGrant>>);

impl Drop for RevokeOnDrop {
    fn drop(&mut self) {
        if let Some(grant) = &self.0 {
            grant.revoked.store(true, Ordering::Release);
        }
    }
}

/// 正常返回、异常返回和取消丢弃 future 均撤销；已捕获的执行器引用也立即失效。
pub(super) async fn run<F: Future>(
    grant: Option<Arc<ScheduledExecutionGrant>>,
    future: F,
) -> F::Output {
    let _revoke = RevokeOnDrop(grant.clone());
    CURRENT.scope(grant, future).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(ttl: Duration) -> Arc<ScheduledExecutionGrant> {
        ScheduledExecutionGrant::new("workspace".into(), "session".into(), "room".into(), ttl)
    }

    #[tokio::test]
    async fn grant_stays_with_one_future_and_checks_all_scope_dimensions() {
        let permit = grant(Duration::from_secs(30));
        run(Some(permit.clone()), async {
            let captured = current().expect("本次投递权限");
            assert!(captured.authorizes("workspace", Some("session"), Some("room")));
            assert!(!captured.authorizes("other", Some("session"), Some("room")));
            assert!(!captured.authorizes("workspace", Some("other"), Some("room")));
            assert!(!captured.authorizes("workspace", Some("session"), Some("other")));
            assert!(!captured.authorizes("workspace", Some("session"), None));
            assert!(tokio::spawn(async { current().is_none() }).await.unwrap());
        }).await;
        assert!(current().is_none());
        assert!(!permit.authorizes("workspace", Some("session"), Some("room")));
    }

    #[tokio::test]
    async fn cancelled_delivery_revokes_a_captured_blocking_worker_reference() {
        let permit = grant(Duration::from_secs(30));
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run(Some(permit), async move {
            sender.send(current().unwrap()).unwrap();
            std::future::pending::<()>().await;
        }));
        let captured = receiver.await.unwrap();
        assert!(captured.authorizes("workspace", Some("session"), Some("room")));
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!captured.authorizes("workspace", Some("session"), Some("room")));
    }

    #[test]
    fn expired_delivery_does_not_authorize_a_late_worker() {
        let permit = grant(Duration::ZERO);
        assert!(!permit.authorizes("workspace", Some("session"), Some("room")));
    }
}
