//! 消息缓存淘汰不是历史删除。显式删除与消息更新共用一个 SQLite 提交。
use super::*;
use std::collections::BTreeSet;

pub(super) fn messages(path: &Path, room: bool, parent_id: &str) -> ApiResult<Vec<PersistedChatMessage>> {
    let connection = open_session_connection(path).map_err(sqlite_api_error)?;
    let (table, parent) = if room { ("chat_room_messages", "room_id") } else { ("session_messages", "session_id") };
    let mut statement = connection.prepare(&format!("SELECT id,author,role,target,content,kind,attachments_json,created_at FROM {table} WHERE {parent}=?1 ORDER BY created_at,id")).map_err(sqlite_api_error)?;
    let rows = statement.query_map(params![parent_id], persisted_message_from_row).map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
    Ok(rows)
}

pub(super) fn message_exists(path: &Path, room: bool, parent_id: &str, id: &str) -> ApiResult<bool> {
    let connection = open_session_connection(path).map_err(sqlite_api_error)?;
    let (table, parent) = if room { ("chat_room_messages", "room_id") } else { ("session_messages", "session_id") };
    connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {parent}=?1 AND id=?2)"), params![parent_id,id], |row| row.get(0)).map_err(sqlite_api_error)
}

pub(super) fn message_ids(path: &Path, room: bool, parent_id: &str) -> ApiResult<BTreeSet<String>> {
    let connection = open_session_connection(path).map_err(sqlite_api_error)?;
    let (table, parent) = if room { ("chat_room_messages", "room_id") } else { ("session_messages", "session_id") };
    let mut statement = connection.prepare(&format!("SELECT id FROM {table} WHERE {parent}=?1")).map_err(sqlite_api_error)?;
    let ids = statement.query_map(params![parent_id], |row| row.get(0)).map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<BTreeSet<_>>>().map_err(sqlite_api_error)?;
    Ok(ids)
}

#[derive(Debug, Clone)]
pub(super) enum HistoryEdit {
    ClearSession(String),
    AfterSessionCursor { session_id: String, created_at: u64, message_id: String },
    Message { room: bool, parent_id: String, message_id: String },
}

pub(super) fn apply(tx: &rusqlite::Transaction<'_>, edits: &[HistoryEdit]) -> rusqlite::Result<()> {
    for edit in edits {
        match edit {
            HistoryEdit::ClearSession(id) => {
                tx.execute("DELETE FROM session_messages WHERE session_id=?1", params![id])?;
            }
            HistoryEdit::AfterSessionCursor { session_id, created_at, message_id } => {
                tx.execute("DELETE FROM session_messages WHERE session_id=?1 AND (created_at>?2 OR (created_at=?2 AND id>?3))",
                    params![session_id, u64_to_i64(*created_at), message_id])?;
            }
            HistoryEdit::Message { room, parent_id, message_id } => {
                let sql = if *room { "DELETE FROM chat_room_messages WHERE room_id=?1 AND id=?2" }
                    else { "DELETE FROM session_messages WHERE session_id=?1 AND id=?2" };
                tx.execute(sql, params![parent_id,message_id])?;
            }
        }
    }
    Ok(())
}

pub(super) fn remove_orphans(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM session_messages WHERE session_id NOT IN (SELECT id FROM sessions)", [])?;
    tx.execute("DELETE FROM chat_room_messages WHERE room_id NOT IN (SELECT id FROM chat_rooms)", [])?;
    tx.execute("DELETE FROM attachment_refs WHERE (message_tbl='session_messages' AND NOT EXISTS(SELECT 1 FROM session_messages m WHERE m.id=attachment_refs.message_id)) OR (message_tbl='chat_room_messages' AND NOT EXISTS(SELECT 1 FROM chat_room_messages m WHERE m.id=attachment_refs.message_id))", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &Path) -> SessionStore {
        let mut session = seed_session();
        session.id = "history-agent".into();
        session.messages = (0..250).map(|index| PersistedChatMessage {
            id: format!("message-{index:05}"), author: "测试".into(), role: "user".into(),
            target: "history-agent".into(), content: format!("历史中文𠮷😀 {index}"), kind: "user-message".into(),
            attachments: Vec::new(), created_at: 1000+index,
        }).collect();
        let mut capacity = SessionStoreCapacity::default();
        capacity.max_session_messages = 100;
        let mut store = SessionStore {
            history_edits: Vec::new(), committed_state: None,
            path: path.join("history.sqlite3"), legacy_json_path: path.join("history.json"), capacity,
            state: PersistedSessionState { sessions: vec![session], ..Default::default() },
        };
        store.save().unwrap();
        store
    }

    #[test]
    fn cache_eviction_never_deletes_history_and_explicit_reset_does() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = fixture(directory.path());
        assert_eq!(store.state.sessions[0].messages.len(),100);
        store.save().unwrap();
        assert_eq!(messages(&store.path,false,"history-agent").unwrap().len(),250);
        let mut restored = SessionStore::load_from_paths(store.path.clone(),store.legacy_json_path.clone(),store.capacity);
        assert_eq!(messages(&restored.path,false,"history-agent").unwrap().len(),250);
        restored.reset_session("history-agent").unwrap();
        restored.save().unwrap();
        assert!(messages(&restored.path,false,"history-agent").unwrap().is_empty());
    }

    #[test]
    fn database_abort_does_not_leak_uncommitted_cache_into_next_save() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = fixture(directory.path());
        let connection = open_session_connection(&store.path).unwrap();
        connection.execute_batch("CREATE TRIGGER refuse_test_message BEFORE INSERT ON session_messages WHEN new.id='not-committed' BEGIN SELECT RAISE(ABORT,'模拟磁盘提交失败'); END;").unwrap();
        let mut pending = store.state.sessions[0].messages[0].clone();
        pending.id = "not-committed".into();
        store.state.sessions[0].messages.push(pending);
        assert!(store.save().is_err());
        assert!(!store.state.sessions[0].messages.iter().any(|message|message.id=="not-committed"));
        connection.execute_batch("DROP TRIGGER refuse_test_message;").unwrap();
        store.save().unwrap();
        assert!(!message_exists(&store.path,false,"history-agent","not-committed").unwrap());
        assert_eq!(messages(&store.path,false,"history-agent").unwrap().len(),250);
    }

    #[test]
    fn old_history_can_be_deleted_forked_and_rolled_back_outside_cache() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = fixture(directory.path());
        store.delete_session_message_with_attachment_dir("history-agent","message-00000",directory.path()).unwrap();
        assert!(!message_exists(&store.path,false,"history-agent","message-00000").unwrap());
        let cursor = history_turn_id("history-agent","message-00002");
        let fork = store.fork_session("history-agent",SessionForkRequest { before:Some(cursor.clone()),name:None }).unwrap();
        assert_eq!(fork.copied_messages,2);
        assert_eq!(messages(&store.path,false,&fork.session.id).unwrap().len(),2);
        let result=store.rollback_session("history-agent",SessionRollbackRequest {before:cursor,reason:None}).unwrap();
        assert_eq!(result.kept_messages,2);
        assert_eq!(result.removed_messages,247);
        store.save().unwrap();
        assert_eq!(messages(&store.path,false,"history-agent").unwrap().len(),2);
    }

    #[test]
    fn deleting_one_fork_does_not_collect_an_attachment_still_referenced_elsewhere() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = fixture(directory.path());
        let name = "att-history-shared.png";
        std::fs::write(directory.path().join(name),b"shared-attachment-test").unwrap();
        let mut shared = store.state.sessions[0].messages[0].clone();
        shared.attachments.push(ChatAttachmentDto { text_snapshot: None, kind:"image".into(),name:name.into(),
            url:format!("/api/attachments/files/{name}"),mime_type:Some("image/png".into()) });
        store.state.sessions[0].messages[0]=shared.clone();
        let mut fork = store.state.sessions[0].clone();
        fork.id="second-agent".into();
        fork.messages=vec![shared.clone()];
        store.state.sessions.push(fork);
        store.save().unwrap();
        let first=store.delete_session_message_with_attachment_dir("history-agent",&shared.id,directory.path()).unwrap();
        assert_eq!(first.deleted_attachments,0);
        assert!(directory.path().join(name).exists());
        assert!(message_exists(&store.path,false,"second-agent",&shared.id).unwrap());
        let last=store.delete_session_message_with_attachment_dir("second-agent",&shared.id,directory.path()).unwrap();
        assert_eq!(last.deleted_attachments,0);
        assert!(directory.path().join(name).exists(), "GC 只读演练：最后消息引用消失也不删除磁盘对象");
    }
}
