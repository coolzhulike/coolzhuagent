//! 视频提交/轮询脱离 HTTP 生命周期，但不能脱离接纳时冻结的根预算。
use std::future::Future;
use crate::root_execution_budget;

struct VideoWaitGuard { job_key: String, settled: bool }
impl VideoWaitGuard {
    fn unresolved(&mut self, reason: &str) {
        let detail = format!("{reason}；已提交的云端生成可能继续运行，结果未确认，请勿自动重试。");
        crate::persist_video_failure_message(&self.job_key, &detail);
        // 沿用前端能识别的 failed 终态，error 明确区分本地等待结束与云端生成失败。
        crate::set_video_job(&self.job_key, "failed", None, Some(detail));
        self.settled = true;
    }
}
impl Drop for VideoWaitGuard {
    fn drop(&mut self) {
        if !self.settled { self.unresolved("视频本地等待异常结束"); }
    }
}

pub(crate) fn spawn(job_key: String, scope: crate::video_job_control::VideoJobScope, work: impl Future<Output = ()> + Send + 'static) {
    // 必须在 spawn 之前复制 task-local；后台不按当前配置另开一份新预算。
    let budget = root_execution_budget::current();
    let cancellation = crate::CHAT_CANCELLATION.try_with(std::sync::Arc::clone).ok();
    let mut guard = VideoWaitGuard { job_key, settled: false };
    let Some(budget) = budget else {
        guard.unresolved("缺少视频请求的接纳时限，未发送新请求");
        return;
    };
    // 父聊天可以先返回 pending；视频仍使用本工程的消息/配置，必须持有活动 pin 到真实收尾。
    let workspace_pin = match crate::workspace_activity::pin_workspace() {
        Ok(pin) => pin,
        Err(reason) => { guard.unresolved(&reason); return; },
    };
    let registration = match crate::video_job_control::register(&guard.job_key, scope) {
        Ok(registration) => registration,
        Err(reason) => { guard.unresolved(&reason); return; },
    };
    tokio::spawn(async move {
        let _workspace_pin = workspace_pin;
        let registration = registration;
        root_execution_budget::scope(budget.clone(), async {
            let parent_cancelled = async {
                match &cancellation {
                    Some(token) => token.cancelled().await,
                    None => std::future::pending::<()>().await,
                }
            };
            let cancelled = async {
                tokio::select! {
                    _ = parent_cancelled => {},
                    _ = registration.cancellation.cancelled() => {},
                }
            };
            tokio::select! {
                biased;
                _ = cancelled => guard.unresolved("视频本地等待已停止，不再发送后续提交或轮询请求"),
                result = budget.run(work) => match result {
                    Ok(()) => guard.settled = true,
                    Err(_) => guard.unresolved(root_execution_budget::EXPIRED_REASON),
                }
            }
        }).await;
        drop(registration);
    });
}
