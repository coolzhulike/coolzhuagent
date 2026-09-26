//! provider 调用编号只在一份模型回复内关联；执行编号由宿主的真实请求作用域确定。
use sha2::{Digest, Sha256};
use std::future::Future;

tokio::task_local! { static SOURCE_REQUEST_KEY: String; }

pub(crate) struct ModelToolIdentity {
    pub(crate) execution_id: String,
    pub(crate) provider_tool_call_id: String,
    /// 宿主模型回复作用域（trace + feedback round），不是臆造的网络 request_attempt_id。
    pub(crate) source_request_key: String,
}
impl ModelToolIdentity {
    pub(crate) fn from_source(run_id: Option<&str>, source_request_key: &str, provider_id: &str) -> Result<Self, String> {
        if source_request_key.trim().is_empty() || provider_id.trim().is_empty()
            || source_request_key.len() > 4096 || provider_id.len() > 4096 {
            return Err("模型工具调用缺少有效的来源请求或 provider 编号".into());
        }
        // 长度/转义由 JSON 承担，不用有歧义的字符串拼接，也不纳入可改写的工具参数。
        let canonical = serde_json::to_vec(&("model-tool-v1", run_id, source_request_key, provider_id))
            .map_err(|error| error.to_string())?;
        Ok(Self { execution_id: format!("tool-{:x}", Sha256::digest(canonical)),
            provider_tool_call_id: provider_id.into(), source_request_key: source_request_key.into() })
    }
}
pub(crate) fn current() -> Option<String> { SOURCE_REQUEST_KEY.try_with(Clone::clone).ok() }
pub(crate) async fn scope<F: Future>(source: String, future: F) -> F::Output {
    SOURCE_REQUEST_KEY.scope(source, future).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_id_is_correlated_without_colliding_across_actual_requests() {
        let a = ModelToolIdentity::from_source(Some("run-1"), "trace-1/response-1", "call_1").unwrap();
        let replay = ModelToolIdentity::from_source(Some("run-1"), "trace-1/response-1", "call_1").unwrap();
        let feedback = ModelToolIdentity::from_source(Some("run-1"), "trace-1/response-2", "call_1").unwrap();
        let next = ModelToolIdentity::from_source(Some("run-2"), "trace-1/response-1", "call_1").unwrap();
        assert_eq!(a.execution_id, replay.execution_id);
        assert_ne!(a.execution_id, feedback.execution_id);
        assert_ne!(a.execution_id, next.execution_id);
        assert_eq!(a.provider_tool_call_id, "call_1");
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("calls.sqlite3");
        let store = crate::computer_use_store::ComputerUseRunStore::open(&db).unwrap();
        store.register_model_tool_call(&a, Some("run-1"), "write_file", "digest-a").unwrap();
        assert!(store.register_model_tool_call(&replay, Some("run-1"), "write_file", "changed-arguments").is_err());
        store.register_model_tool_call(&feedback, Some("run-1"), "write_file", "digest-b").unwrap();
        store.register_model_tool_call(&next, Some("run-2"), "write_file", "digest-c").unwrap();
        assert_eq!(store.tool_calls_for_run("run-1").unwrap().len(), 2);
        assert_eq!(store.tool_calls_for_run("run-2").unwrap().len(), 1);
        let connection = crate::open_session_connection(&db).unwrap();
        let saved = connection.query_row("SELECT provider_tool_call_id,source_request_key,request_attempt_id,arguments_digest FROM tool_calls WHERE tool_call_id=?1",
            [&a.execution_id], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,String>(3)?))).unwrap();
        assert_eq!(saved, ("call_1".into(), "trace-1/response-1".into(), None, "digest-a".into()));
    }
}
