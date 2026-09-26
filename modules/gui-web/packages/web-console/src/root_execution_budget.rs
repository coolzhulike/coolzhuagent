//! 轮次/Goal 阶段共享执行时限；单调时钟负责截止，Unix 时刻仅用于协议和审计。
use std::{future::Future, pin::Pin, task::{Context, Poll}, time::{Duration, Instant}};
use futures_core::Stream;

pub(crate) const DEFAULT_TURN_TIMEOUT_MS: u64 = 900_000;
pub(crate) const MIN_TURN_TIMEOUT_MS: u64 = 60_000;
pub(crate) const MAX_TURN_TIMEOUT_MS: u64 = 86_400_000;
pub(crate) const EXPIRED_REASON: &str = "本轮整体执行时限已到，已停止新的模型请求与工具操作；已发生的结果仍保留";

#[derive(Clone, Debug)]
pub(crate) struct RootExecutionBudget {
    established_at_unix_ms: u64,
    deadline_unix_ms: u64,
    monotonic_deadline: Instant,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RootBudgetExpired;
impl std::fmt::Display for RootBudgetExpired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(EXPIRED_REASON)
    }
}

tokio::task_local! {
    static ACTIVE_ROOT_BUDGET: RootExecutionBudget;
}

impl RootExecutionBudget {
    pub(crate) fn for_session(session_id: Option<&str>) -> Self {
        let timeout_ms = session_id.and_then(|id| crate::session_model_settings_for(id).turn_timeout_ms)
            .unwrap_or(DEFAULT_TURN_TIMEOUT_MS).clamp(MIN_TURN_TIMEOUT_MS, MAX_TURN_TIMEOUT_MS);
        Self::establish(timeout_ms)
    }
    /// 接纳时建立一次；后续重试、工具反馈、接力与 CU 只能 clone 同一个值。
    pub(crate) fn establish(timeout_ms: u64) -> Self {
        Self::from_started_at(crate::unix_timestamp_millis(), timeout_ms)
    }

    /// 已持久化开始时间的 Goal 阶段从真实 started_at 计算剩余时间，不能从查询时刻续期。
    pub(crate) fn from_started_at(started_at_unix_ms: u64, timeout_ms: u64) -> Self {
        let now = crate::unix_timestamp_millis();
        let deadline_unix_ms = started_at_unix_ms.saturating_add(timeout_ms);
        // 将过去耗时扣除后锚定到单调时钟。未来的错误 started_at 不能增加配置预算。
        let remaining_ms = deadline_unix_ms.saturating_sub(now).min(timeout_ms);
        Self {
            established_at_unix_ms: started_at_unix_ms,
            deadline_unix_ms,
            monotonic_deadline: Instant::now() + Duration::from_millis(remaining_ms),
        }
    }

    pub(crate) fn remaining(&self) -> Duration {
        self.monotonic_deadline.saturating_duration_since(Instant::now())
    }

    pub(crate) fn is_expired(&self) -> bool { self.remaining().is_zero() }
    pub(crate) fn deadline_unix_ms(&self) -> u64 { self.deadline_unix_ms }
    pub(crate) fn established_at_unix_ms(&self) -> u64 { self.established_at_unix_ms }

    pub(crate) fn limit_ms(&self, requested_ms: u64) -> u64 {
        requested_ms.min(self.remaining().as_millis().min(u128::from(u64::MAX)) as u64)
    }

    /// 给独立进程的截止不能晚于单调剩余时间，系统时钟回拨也不能续期输入许可。
    pub(crate) fn effective_protocol_deadline_unix_ms(&self) -> u64 {
        self.deadline_unix_ms.min(crate::unix_timestamp_millis().saturating_add(self.limit_ms(u64::MAX)))
    }

    pub(crate) async fn run<F: Future>(&self, future: F) -> Result<F::Output, RootBudgetExpired> {
        if self.is_expired() { return Err(RootBudgetExpired); }
        tokio::time::timeout_at(tokio::time::Instant::from_std(self.monotonic_deadline), future)
            .await.map_err(|_| RootBudgetExpired)
    }

    pub(crate) async fn expired(&self) {
        tokio::time::sleep_until(tokio::time::Instant::from_std(self.monotonic_deadline)).await;
    }

    /// 先传播取消，再丢弃业务等待；已被监督的子进程依靠同一 token/Drop 收尾并记录事实。
    pub(crate) async fn run_cancellable<F: Future>(&self, future: F, cancel: impl FnOnce()) -> Result<F::Output, RootBudgetExpired> {
        if self.is_expired() { cancel(); return Err(RootBudgetExpired); }
        tokio::pin!(future);
        tokio::select! {
            biased;
            _ = self.expired() => { cancel(); Err(RootBudgetExpired) },
            output = &mut future => Ok(output),
        }
    }
}

pub(crate) fn current() -> Option<RootExecutionBudget> {
    ACTIVE_ROOT_BUDGET.try_with(Clone::clone).ok()
}

/// 接力/嵌套操作已有更早根时限时沿用它；显式派生只允许缩短，不允许重置。
pub(crate) async fn scope<F: Future>(budget: RootExecutionBudget, future: F) -> F::Output {
    let effective = current().filter(|parent| parent.monotonic_deadline <= budget.monotonic_deadline).unwrap_or(budget);
    ACTIVE_ROOT_BUDGET.scope(effective, future).await
}

pub(crate) fn limit_current_ms(requested_ms: u64) -> u64 {
    current().map_or(requested_ms, |budget| budget.limit_ms(requested_ms))
}

pub(crate) fn expired_reason() -> Option<&'static str> {
    current().filter(RootExecutionBudget::is_expired).map(|_| EXPIRED_REASON)
}

/// SSE 的执行跨多次 poll；仅包住 stream 构造函数不会把 task-local 带进其内部。
/// 每次 poll 都恢复同一个预算，不主动丢弃流，使原有 guard/终态提交仍有机会运行。
pub(crate) struct ScopedStream<S> {
    inner: Pin<Box<S>>,
    budget: RootExecutionBudget,
}
impl<S: Stream> Stream for ScopedStream<S> {
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        ACTIVE_ROOT_BUDGET.sync_scope(this.budget.clone(), || this.inner.as_mut().poll_next(cx))
    }
}
pub(crate) fn scope_stream<S: Stream>(budget: RootExecutionBudget, stream: S) -> ScopedStream<S> {
    let effective = current().filter(|parent| parent.monotonic_deadline <= budget.monotonic_deadline).unwrap_or(budget);
    ScopedStream { inner: Box::pin(stream), budget: effective }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn nested_scope_and_retry_cannot_renew_an_expired_root() {
        let expired = RootExecutionBudget::from_started_at(1, 1);
        scope(expired.clone(), async {
            scope(RootExecutionBudget::establish(60_000), async {
                let root = current().unwrap();
                assert_eq!(root.deadline_unix_ms(), 2);
                assert_eq!(root.limit_ms(30_000), 0);
                let touched = std::cell::Cell::new(false);
                assert!(root.run(async { touched.set(true); }).await.is_err());
                assert!(!touched.get(), "过期的根不能轮询会触发网络/工具副作用的 future");
            }).await;
        }).await;
        assert!(current().is_none(), "作用域退出后不得泄漏到另一轮次");
        use futures_util::StreamExt;
        let stream = futures_util::stream::poll_fn(|_| {
            assert_eq!(expired_reason(), Some(EXPIRED_REASON));
            Poll::Ready(Some(current().unwrap().deadline_unix_ms()))
        });
        let mut stream = scope_stream(expired, stream);
        assert_eq!(stream.next().await, Some(2));
        assert!(current().is_none(), "SSE poll 后作用域不得泄漏给其他请求");
    }
}
