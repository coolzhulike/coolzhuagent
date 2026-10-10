//! 回合失败也保存已记录的会话事实；保存失败不得抹去原执行错误。
use std::{error::Error, path::Path};
use runtime::{AgentEvent, RuntimeError, Session, TurnSummary};

pub(super) trait PersistedTurn {
    fn events(&self) -> &[AgentEvent];
}

impl PersistedTurn for TurnSummary {
    fn events(&self) -> &[AgentEvent] { &self.events }
}

#[cfg(test)]
impl PersistedTurn for () {
    fn events(&self) -> &[AgentEvent] { &[] }
}

/// 恢复持久轨迹；旧会话没有轨迹目录时兼容为空，不忽略已存在的损坏记录。
pub(super) fn load_event_history(path: &Path) -> std::io::Result<Vec<Vec<AgentEvent>>> {
    use std::io::Read;
    let event_path = path.with_extension("events.json");
    let directory = path.parent().unwrap_or_else(|| Path::new("."))
        .join("internal-turns").join(event_path.file_name().ok_or_else(|| std::io::Error::other("会话缺少文件名"))?);
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut history = Vec::new();
    let mut total_bytes = 0_usize;
    let mut identities = std::collections::BTreeSet::new();
    for entry in entries {
        if history.len() >= 10_000 { return Err(std::io::Error::other("回合历史超过恢复数量上限")); }
        let entry = entry?;
        if entry.path().extension().is_some_and(|extension| extension == "pending") {
            return Err(std::io::Error::other("发现未提交回合轨迹，执行结果未知，需恢复处置"));
        }
        if !entry.file_type()?.is_file() { return Err(std::io::Error::other("回合历史包含非普通文件")); }
        let mut bytes = Vec::new();
        std::fs::File::open(entry.path())?.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 8 * 1024 * 1024 { return Err(std::io::Error::other("回合轨迹超过恢复大小上限")); }
        total_bytes += bytes.len();
        if total_bytes > 64 * 1024 * 1024 { return Err(std::io::Error::other("回合历史超过恢复总大小上限")); }
        let events: Vec<AgentEvent> = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        let first = events.first().ok_or_else(|| std::io::Error::other("回合轨迹为空"))?;
        use runtime::AgentEventKind::{TurnStarted, TurnCompleted, TurnFailed, TurnInterrupted};
        if first.kind != TurnStarted || first.thread_id.as_str().trim().is_empty() || first.turn_id.as_str().trim().is_empty()
            || !identities.insert((first.thread_id.clone(), first.turn_id.clone())) {
            return Err(std::io::Error::other("回合起点或历史身份重复"));
        }
        for (index, event) in events.iter().enumerate() {
            let terminal = matches!(event.kind, TurnCompleted | TurnFailed | TurnInterrupted);
            if event.schema_version != 1 || event.sequence != (index + 1) as u64 || event.thread_id != first.thread_id || event.turn_id != first.turn_id
                || (index > 0 && event.kind == TurnStarted) || terminal != (index + 1 == events.len()) {
                return Err(std::io::Error::other("回合轨迹序号、身份或唯一终态不一致"));
            }
        }
        history.push(events);
    }
    Ok(history)
}

/// 内部回合保留独立快照，先独占预留文件；不会覆盖主会话或已有内部回合。
pub(super) fn reserve_internal_path(parent_session: &Path) -> std::io::Result<std::path::PathBuf> {
    reserve_internal_file(parent_session).map(|(path, _file)| path)
}

fn reserve_internal_file(parent_session: &Path) -> std::io::Result<(std::path::PathBuf, std::fs::File)> {
    reserve_internal_file_with_extension(parent_session, "json")
}

fn reserve_internal_file_with_extension(parent_session: &Path, extension: &str) -> std::io::Result<(std::path::PathBuf, std::fs::File)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let name = parent_session.file_name().ok_or_else(|| std::io::Error::other("主会话路径缺少文件名"))?;
    let directory = parent_session.parent().unwrap_or_else(|| Path::new("."))
        .join("internal-turns").join(name);
    std::fs::create_dir_all(&directory)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?.as_nanos();
    for _ in 0..32 {
        let path = directory.join(format!("{now}-{}-{}.{extension}", std::process::id(), SEQUENCE.fetch_add(1, Ordering::Relaxed)));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("无法独占预留内部会话文件"))
}

pub(super) fn persist_result<T: PersistedTurn>(
    result: Result<T, RuntimeError>, session: &Session, path: &Path,
) -> Result<T, Box<dyn Error>> {
    let session_saved = session.save_to_path(path);
    let events_saved = match &result {
        Err(error) => save_events(error.events(), path),
        Ok(summary) => save_events(summary.events(), path),
    };
    if let Err(events_error) = events_saved {
        let storage = session_saved.err().map(|error| format!("；会话保存也失败：{error}")).unwrap_or_default();
        let execution = match &result {
            Err(error) => format!("回合执行失败：{error}；"),
            Ok(_) => String::new(),
        };
        return Err(Box::new(std::io::Error::other(format!("{execution}终态轨迹保存失败：{events_error}{storage}"))));
    }
    match (result, session_saved) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(Box::new(error)),
        (Ok(_), Err(error)) => Err(Box::new(error)),
        (Err(execution), Err(storage)) => Err(Box::new(std::io::Error::other(
            format!("回合执行失败：{execution}；会话事实保存也失败：{storage}"),
        ))),
    }
}

/// 回合轨迹与会话消息分别留存；独占文件避免后续回合覆盖历史。
fn save_events(events: &[AgentEvent], path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    if events.is_empty() { return Ok(()); }
    let bytes = serde_json::to_vec_pretty(events).map_err(std::io::Error::other)?;
    let (event_path, mut file) = reserve_internal_file_with_extension(&path.with_extension("events.json"), "pending")?;
    let saved = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = saved {
        // 清理本次独占文件，避免普通写入失败留下可误读的半份轨迹。
        return match std::fs::remove_file(&event_path) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(std::io::Error::other(format!("{error}；未完成轨迹清理失败：{cleanup}"))),
        };
    }
    // 同目录硬链接以不覆盖方式发布完整记录；失败保留 pending，恢复时明确报未知。
    publish_pending_events(&event_path)
}

fn publish_pending_events(path: &Path) -> std::io::Result<()> {
    std::fs::hard_link(path, path.with_extension("json"))?;
    std::fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, PermissionMode,
        PermissionPolicy, ToolExecutor, ToolError};

    #[test]
    fn pending_publication_never_overwrites_existing_history() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp")
            .join(format!("cli-pending-collision-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let pending = directory.join("turn.pending");
        let committed = directory.join("turn.json");
        std::fs::write(&pending, b"new-complete-record").unwrap();
        std::fs::write(&committed, b"old-record").unwrap();
        assert!(publish_pending_events(&pending).is_err());
        assert_eq!(std::fs::read(&committed).unwrap(), b"old-record");
        assert_eq!(std::fs::read(&pending).unwrap(), b"new-complete-record");
        std::fs::remove_file(&committed).unwrap();
        publish_pending_events(&pending).unwrap();
        assert!(!pending.exists());
        assert_eq!(std::fs::read(&committed).unwrap(), b"new-complete-record");
        std::fs::remove_file(committed).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn completed_turn_is_saved_and_storage_failure_is_not_success() {
        struct Client;
        impl ApiClient for Client {
            fn stream(&mut self, _: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                Ok(vec![AssistantEvent::TextDelta("已完成".into()), AssistantEvent::MessageStop])
            }
        }
        struct Executor;
        impl ToolExecutor for Executor {
            fn execute(&mut self, _: &str, _: &str) -> Result<String, ToolError> { unreachable!() }
        }
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp")
            .join(format!("cli-completed-events-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("main.json");
        assert!(load_event_history(&path).unwrap().is_empty());
        let mut runtime = ConversationRuntime::new(Session::new(), Client, Executor,
            PermissionPolicy::new(PermissionMode::ReadOnly), Vec::new());
        let summary = persist_result(runtime.run_turn("完成", None), runtime.session(), &path).unwrap();
        let event_directory = directory.join("internal-turns/main.events.json");
        let files = std::fs::read_dir(&event_directory).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(files.len(), 1);
        let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(files[0].path()).unwrap()).unwrap();
        assert_eq!(actual, serde_json::to_value(&summary.events).unwrap());
        assert_eq!(actual.as_array().unwrap().last().unwrap()["kind"], "turn_completed");
        assert_eq!(load_event_history(&path).unwrap(), vec![summary.events.clone()]);
        let pending = event_directory.join("unfinished.pending");
        std::fs::write(&pending, b"{").unwrap();
        let pending_error = load_event_history(&path).unwrap_err();
        assert!(pending_error.to_string().contains("未提交回合轨迹"));
        std::fs::remove_file(pending).unwrap();
        let mut malformed = actual.clone();
        malformed.as_array_mut().unwrap().pop();
        std::fs::write(files[0].path(), serde_json::to_vec(&malformed).unwrap()).unwrap();
        assert!(load_event_history(&path).is_err());
        std::fs::write(files[0].path(), b"{").unwrap();
        assert!(load_event_history(&path).is_err());
        std::fs::remove_file(files[0].path()).unwrap();
        std::fs::remove_dir(event_directory).unwrap();
        std::fs::remove_dir(directory.join("internal-turns")).unwrap();
        std::fs::write(directory.join("internal-turns"), b"blocked").unwrap();
        let error = persist_result(runtime.run_turn("再次完成", None), runtime.session(), &path).unwrap_err();
        assert!(error.to_string().contains("终态轨迹保存失败"));
        assert!(!error.to_string().contains("回合执行失败"));
        assert_eq!(Session::load_from_path(&path).unwrap(), *runtime.session());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(directory.join("internal-turns")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

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
        let expected_events = serde_json::to_value(result.as_ref().unwrap_err().events()).unwrap();
        let error = persist_result(result, runtime.session(), &path).unwrap_err();
        assert!(error.to_string().contains("模型反馈请求失败"));
        let saved = Session::load_from_path(&path).unwrap();
        assert_eq!(saved, *runtime.session());
        let event_directory = path.parent().unwrap().join("internal-turns")
            .join(path.with_extension("events.json").file_name().unwrap());
        let event_files = std::fs::read_dir(&event_directory).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(event_files.len(), 1);
        let persisted: serde_json::Value = serde_json::from_slice(&std::fs::read(event_files[0].path()).unwrap()).unwrap();
        assert_eq!(persisted, expected_events);
        assert_eq!(serde_json::to_value(load_event_history(&path).unwrap()[0].clone()).unwrap(), expected_events);
        let events = persisted.as_array().unwrap();
        assert_eq!(events.last().unwrap()["kind"], "turn_failed");
        assert!(!events.iter().any(|event| event["kind"] == "turn_completed"));
        assert!(saved.messages.iter().flat_map(|message| &message.blocks).any(|block| matches!(block,
            runtime::ContentBlock::ToolResult { tool_use_id, output, is_error: false, .. }
            if tool_use_id == "actual-call" && output == "已执行工具事实")));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(event_files[0].path()).unwrap();
        std::fs::remove_dir(event_directory).unwrap();
    }

    #[test]
    fn event_storage_error_keeps_original_execution_error() {
        struct Client;
        impl ApiClient for Client {
            fn stream(&mut self, _: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                Err(RuntimeError::new("模型请求原错误"))
            }
        }
        struct Executor;
        impl ToolExecutor for Executor {
            fn execute(&mut self, _: &str, _: &str) -> Result<String, ToolError> { unreachable!() }
        }
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp")
            .join(format!("cli-event-storage-error-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("main.json");
        // 用普通文件阻止轨迹目录创建，同时允许会话正常保存。
        std::fs::write(directory.join("internal-turns"), b"blocked").unwrap();
        let mut runtime = ConversationRuntime::new(Session::new(), Client, Executor,
            PermissionPolicy::new(PermissionMode::ReadOnly), Vec::new());
        let result = runtime.run_turn("request", None);
        assert!(!result.as_ref().unwrap_err().events().is_empty());
        let error = persist_result(result, runtime.session(), &path).unwrap_err();
        assert!(error.to_string().contains("模型请求原错误"));
        assert!(error.to_string().contains("终态轨迹保存失败"));
        assert_eq!(Session::load_from_path(&path).unwrap(), *runtime.session());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(directory.join("internal-turns")).unwrap();
        std::fs::remove_dir(directory).unwrap();
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
