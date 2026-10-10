//! 回合失败也保存已记录的会话事实；保存失败不得抹去原执行错误。
use std::{error::Error, path::Path};
use runtime::{RuntimeError, Session};

/// 内部回合保留独立快照，先独占预留文件；不会覆盖主会话或已有内部回合。
pub(super) fn reserve_internal_path(parent_session: &Path) -> std::io::Result<std::path::PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let name = parent_session.file_name().ok_or_else(|| std::io::Error::other("主会话路径缺少文件名"))?;
    let directory = parent_session.parent().unwrap_or_else(|| Path::new("."))
        .join("internal-turns").join(name);
    std::fs::create_dir_all(&directory)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?.as_nanos();
    for _ in 0..32 {
        let path = directory.join(format!("{now}-{}-{}.json", std::process::id(), SEQUENCE.fetch_add(1, Ordering::Relaxed)));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("无法独占预留内部会话文件"))
}

pub(super) fn persist_result<T>(
    result: Result<T, RuntimeError>, session: &Session, path: &Path,
) -> Result<T, Box<dyn Error>> {
    match (result, session.save_to_path(path)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(Box::new(error)),
        (Ok(_), Err(error)) => Err(Box::new(error)),
        (Err(execution), Err(storage)) => Err(Box::new(std::io::Error::other(
            format!("回合执行失败：{execution}；会话事实保存也失败：{storage}"),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, PermissionMode,
        PermissionPolicy, ToolExecutor, ToolError};

    #[test]
    fn model_error_after_tool_preserves_saved_execution_fact() {
        struct Client(usize);
        impl ApiClient for Client {
            fn stream(&mut self, _: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                self.0 += 1;
                if self.0 == 1 {
                    Ok(vec![AssistantEvent::ToolUse { id: "actual-call".into(), name: "Read".into(), input: "{}".into() }, AssistantEvent::MessageStop])
                } else { Err(RuntimeError::new("模型反馈请求失败")) }
            }
        }
        struct Executor;
        impl ToolExecutor for Executor {
            fn execute(&mut self, _: &str, _: &str) -> Result<String, ToolError> { Ok("已执行工具事实".into()) }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("cli-error-facts-{}.json", std::process::id()));
        let mut runtime = ConversationRuntime::new(Session::new(), Client(0), Executor,
            PermissionPolicy::new(PermissionMode::DangerFullAccess)
                .with_tool_requirement("Read", PermissionMode::ReadOnly), Vec::new());
        let result = runtime.run_turn("read", None);
        let error = persist_result(result, runtime.session(), &path).unwrap_err();
        assert!(error.to_string().contains("模型反馈请求失败"));
        let saved = Session::load_from_path(&path).unwrap();
        assert_eq!(saved, *runtime.session());
        assert!(saved.messages.iter().flat_map(|message| &message.blocks).any(|block| matches!(block,
            runtime::ContentBlock::ToolResult { tool_use_id, output, is_error: false, .. }
            if tool_use_id == "actual-call" && output == "已执行工具事实")));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn storage_error_preserves_execution_error_and_never_reports_success() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp");
        std::fs::create_dir_all(&root).unwrap();
        let error = persist_result::<()>(Err(RuntimeError::new("执行原错误")), &Session::new(), &root).unwrap_err();
        assert!(error.to_string().contains("执行原错误"));
        assert!(error.to_string().contains("保存也失败"));
        assert!(persist_result(Ok(()), &Session::new(), &root).is_err());
    }

    #[test]
    fn internal_snapshots_keep_parent_and_previous_turn_unchanged() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp")
            .join(format!("cli-internal-snapshots-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let parent = directory.join("main.json");
        let original = Session::new();
        original.save_to_path(&parent).unwrap();
        let before = std::fs::read(&parent).unwrap();
        let first = reserve_internal_path(&parent).unwrap();
        let mut internal = original.clone();
        internal.messages.push(runtime::ConversationMessage::tool_result("internal-id", "Read", "独立执行事实", false));
        assert!(persist_result::<()>(Err(RuntimeError::new("内部请求失败")), &internal, &first).is_err());
        let first_bytes = std::fs::read(&first).unwrap();
        let second = reserve_internal_path(&parent).unwrap();
        assert_ne!(first, second);
        persist_result(Ok(()), &original, &second).unwrap();
        assert_eq!(std::fs::read(&parent).unwrap(), before);
        assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
        assert_eq!(Session::load_from_path(&first).unwrap(), internal);
        for file in [first, second, parent] { std::fs::remove_file(file).unwrap(); }
        std::fs::remove_dir(directory.join("internal-turns/main.json")).unwrap();
        std::fs::remove_dir(directory.join("internal-turns")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
