//! 只限制宿主已确认的连续失败；相同参数本身不能证明没有进展。
//! 等待、查询及未知语义沿用原根预算/反馈上限，不引入额外模型判断。
use crate::ModelToolDispatchResult;

pub(crate) const MAX_IDENTICAL_FAILURES: u32 = 3;

#[derive(Default)]
pub(crate) struct FailureGuard {
    last_failed_batch: Vec<(String, String, String)>,
    consecutive_failures: u32,
}

impl FailureGuard {
    pub(crate) fn observe(&mut self, results: &[ModelToolDispatchResult]) -> Option<String> {
        // 运行中/成功读取均不是失败；不解析工具正文来臆造任务进展或权限。
        if results.is_empty() || results.iter().any(|result| !result.is_error) {
            self.last_failed_batch.clear();
            self.consecutive_failures = 0;
            return None;
        }
        // error正文中的时间戳、耗时和provider调用编号不能续期同一失败尝试。
        let mut signature = results.iter().map(|result|
            (result.name.clone(), result.input.to_string(), result.route.clone())).collect::<Vec<_>>();
        signature.sort();
        if signature == self.last_failed_batch {
            self.consecutive_failures += 1;
        } else {
            self.last_failed_batch = signature;
            self.consecutive_failures = 1;
        }
        (self.consecutive_failures >= MAX_IDENTICAL_FAILURES).then(|| format!(
            "status=blocked, error_code=tool_repeat_failure_limit, attempts={}。相同工具调用已连续返回失败，达到本轮有限重试上限，已停止追加尝试。已有结果和可能发生的效果以宿主回执为准；未确认任务完成。",
            self.consecutive_failures))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result(error: bool, stamp: u32) -> ModelToolDispatchResult {
        ModelToolDispatchResult {tool_use_id:format!("provider-{stamp}"),name:"read_file".into(),
            input:serde_json::json!({"path":"status.json"}),route:"runtime-executed".into(),
            summary_text:format!("timestamp={stamp}"),tool_result_text:format!("timestamp={stamp}"),is_error:error,recovery_stop:None}
    }
    #[test]
    fn timestamps_do_not_renew_failure_limit_but_success_resets_it() {
        let mut guard=FailureGuard::default();
        assert!(guard.observe(&[result(true,1)]).is_none());
        assert!(guard.observe(&[result(true,2)]).is_none());
        assert!(guard.observe(&[result(true,3)]).unwrap().contains("attempts=3"));
        assert!(guard.observe(&[result(false,4)]).is_none());
        assert!(guard.observe(&[result(true,5)]).is_none());
        assert!(guard.observe(&[result(true,6)]).is_none());
    }
    #[test]
    fn identical_success_or_unknown_content_never_implies_stall() {
        let mut guard=FailureGuard::default();
        for _ in 0..10 {assert!(guard.observe(&[result(false,0)]).is_none());}
        assert!(guard.observe(&[result(true,1),result(false,1)]).is_none());
    }
}
