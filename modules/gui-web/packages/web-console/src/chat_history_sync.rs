//! 可重建的历史失效投影；正文与分页仍由原 SQLite 历史服务提供。
use super::*;

fn ensure_projection(connection: &Connection) -> rusqlite::Result<()> {
    let ready: i64 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('chat_history_revision_v1','chat_history_insert_v1','chat_history_update_v1','chat_history_delete_v1')", [], |row| row.get(0))?;
    if ready == 4 { return Ok(()); }
    let tx = connection.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS chat_history_revision_v1 (room_id TEXT PRIMARY KEY, revision INTEGER NOT NULL);
        CREATE TRIGGER IF NOT EXISTS chat_history_insert_v1 AFTER INSERT ON chat_room_messages BEGIN
          INSERT INTO chat_history_revision_v1 VALUES(new.room_id,1) ON CONFLICT(room_id) DO UPDATE SET revision=revision+1; END;
        CREATE TRIGGER IF NOT EXISTS chat_history_update_v1 AFTER UPDATE ON chat_room_messages
        WHEN old.author IS NOT new.author OR old.role IS NOT new.role OR old.target IS NOT new.target
          OR old.content IS NOT new.content OR old.kind IS NOT new.kind OR old.attachments_json IS NOT new.attachments_json
          OR old.created_at IS NOT new.created_at OR old.room_id IS NOT new.room_id OR old.id IS NOT new.id BEGIN
          INSERT INTO chat_history_revision_v1 VALUES(old.room_id,1) ON CONFLICT(room_id) DO UPDATE SET revision=revision+1;
          INSERT INTO chat_history_revision_v1 SELECT new.room_id,1 WHERE new.room_id != old.room_id
            ON CONFLICT(room_id) DO UPDATE SET revision=revision+1; END;
        CREATE TRIGGER IF NOT EXISTS chat_history_delete_v1 AFTER DELETE ON chat_room_messages BEGIN
          INSERT INTO chat_history_revision_v1 VALUES(old.room_id,1) ON CONFLICT(room_id) DO UPDATE SET revision=revision+1; END;")?;
    tx.commit()
}

fn revision(connection: &Connection, room_id: &str) -> rusqlite::Result<(i64, bool)> {
    connection.query_row("SELECT COALESCE((SELECT revision FROM chat_history_revision_v1 WHERE room_id=?1),0), EXISTS(SELECT 1 FROM chat_rooms WHERE id=?1)", params![room_id], |row| Ok((row.get(0)?,row.get(1)?)))
}

// 复用房间监听连接，只通知已提交的权限失效；实际状态仍由权限接口读取。
fn permission_revision(connection: &Connection, room_id: &str) -> rusqlite::Result<(String, i64)> {
    connection.query_row(
        "SELECT permission_profile, updated_at FROM chat_room_permissions WHERE room_id=?1",
        params![room_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional().map(|value| value.unwrap_or((ROOM_PERMISSION_WORKSPACE_WRITE.into(), 0)))
}

#[derive(Deserialize)]
pub(super) struct ScopeQuery { workspace: String }

#[derive(Deserialize)]
pub(super) struct PageQuery {
    workspace: String,
    limit: Option<usize>,
    before: Option<String>,
}

pub(super) async fn messages(AxumPath(room_id): AxumPath<String>, Query(query): Query<PageQuery>)
    -> ApiResult<Json<ChatRoomMessagesResponse>> {
    let store = session_store().lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR,"会话存储锁已损坏"))?;
    if workspace_identity(Path::new(&query.workspace)) != workspace_identity(&active_workspace_path()) {
        return Err(api_error(StatusCode::CONFLICT,"工作区已切换，请重新选择聊天室"));
    }
    store.chat_room_messages(&room_id,MessageQuery {limit:query.limit,before:query.before})
}

pub(super) async fn events(AxumPath(room_id): AxumPath<String>, Query(query): Query<ScopeQuery>)
    -> ApiResult<Sse<impl futures_core::Stream<Item=Result<Event,std::convert::Infallible>>>> {
    let (path, workspace_id) = {
        let store = session_store().lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR,"会话存储锁已损坏"))?;
        let workspace_id = workspace_identity(&active_workspace_path());
        if workspace_identity(Path::new(&query.workspace)) != workspace_id {
            return Err(api_error(StatusCode::CONFLICT,"工作区已切换，请重新选择聊天室"));
        }
        if !store.state.chat_rooms.iter().any(|room| room.id == room_id) {
            return Err(api_error(StatusCode::NOT_FOUND,"聊天室不存在"));
        }
        (store.path.clone(),workspace_id)
    };
    let setup = tokio::task::spawn_blocking(move || -> rusqlite::Result<Connection> {
        let connection = open_session_connection(&path)?;
        connection.busy_timeout(Duration::from_millis(100))?;
        ensure_projection(&connection)?;
        Ok(connection)
    }).await.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,&error.to_string()))?
        .map_err(sqlite_api_error)?;
    let stream = async_stream::stream! {
        let mut connection = Some(setup);
        let mut last = None;
        let mut last_permission = None;
        yield Ok(sse_json_event("hello",&json!({"room_id":room_id,"workspace_id":workspace_id})));
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let owned = connection.take().expect("历史监听连接仅有一个在途读取");
            let requested_room = room_id.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                let value = revision(&owned,&requested_room)
                    .and_then(|history| permission_revision(&owned,&requested_room)
                        .map(|permission| (history,permission)));
                (owned,value)
            }).await;
            match outcome {
                Ok((owned,Ok((value,permission)))) => {
                    connection = Some(owned);
                    if !value.1 {
                        yield Ok(sse_json_event("room-deleted",&json!({"room_id":room_id})));
                        break;
                    }
                    if last.is_some_and(|previous| previous != value.0) {
                        yield Ok(sse_json_event("history-changed",&json!({"room_id":room_id})));
                    }
                    // hello 后首次读也必须通知，覆盖 hello/GET 与首次版本基线之间的提交。
                    if last.is_none() {
                        yield Ok(sse_json_event("history-changed",&json!({"room_id":room_id})));
                    }
                    last = Some(value.0);
                    if last_permission.as_ref().is_none_or(|previous| previous != &permission) {
                        yield Ok(sse_json_event("permission-changed",&json!({"room_id":room_id})));
                    }
                    last_permission = Some(permission);
                }
                _ => break, // 浏览器正常重连后以 hello 重读，失败不发布虚假正文。
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_query_uses_numeric_limit_without_flatten_deserialization() {
        let uri = "/?workspace=C%3A%2Ftest&limit=200&before=m1".parse().unwrap();
        let Query(query) = Query::<PageQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.workspace,"C:/test");
        assert_eq!(query.limit,Some(200)); assert_eq!(query.before.as_deref(),Some("m1"));
    }
    fn fixture(path: &Path) -> Connection {
        let connection = Connection::open(path).unwrap();
        connection.execute_batch("CREATE TABLE chat_rooms(id TEXT PRIMARY KEY); INSERT INTO chat_rooms VALUES('a'),('b');
          CREATE TABLE chat_room_messages(room_id TEXT,id TEXT,author TEXT,role TEXT,target TEXT,content TEXT,kind TEXT,attachments_json TEXT,created_at INTEGER,PRIMARY KEY(room_id,id));").unwrap();
        ensure_projection(&connection).unwrap(); connection
    }
    #[test]
    fn cross_connection_observes_only_committed_content_changes() {
        let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("sync.sqlite3");
        let mut writer = fixture(&path); let reader = Connection::open(&path).unwrap();
        writer.execute_batch("INSERT INTO chat_room_messages VALUES('a','same','作者','user','','初稿','user-message','[]',1)").unwrap();
        assert_eq!(revision(&reader,"a").unwrap(),(1,true));
        writer.execute_batch("UPDATE chat_room_messages SET content=content").unwrap();
        assert_eq!(revision(&reader,"a").unwrap().0,1);
        let tx = writer.transaction().unwrap();
        tx.execute("UPDATE chat_room_messages SET content='未提交'",[]).unwrap(); tx.rollback().unwrap();
        assert_eq!(revision(&reader,"a").unwrap().0,1);
        writer.execute("UPDATE chat_room_messages SET attachments_json='[{}]'",[]).unwrap();
        assert_eq!(revision(&reader,"a").unwrap().0,2);
    }
    #[test]
    fn deletion_and_recreated_room_do_not_reuse_previous_revision() {
        let directory = tempfile::tempdir().unwrap(); let connection = fixture(&directory.path().join("sync.sqlite3"));
        connection.execute_batch("INSERT INTO chat_room_messages VALUES('a','same','','user','','a','','[]',1),('b','same','','user','','b','','[]',1);").unwrap();
        connection.execute_batch("DELETE FROM chat_room_messages WHERE room_id='a'; DELETE FROM chat_rooms WHERE id='a';").unwrap();
        assert_eq!(revision(&connection,"a").unwrap(),(2,false));
        assert_eq!(revision(&connection,"b").unwrap(),(1,true));
        connection.execute_batch("INSERT INTO chat_rooms VALUES('a'); INSERT INTO chat_room_messages VALUES('a','same','','user','','new','','[]',1);").unwrap();
        assert_eq!(revision(&connection,"a").unwrap(),(3,true));
        ensure_projection(&connection).unwrap(); assert_eq!(revision(&connection,"a").unwrap().0,3);
    }

    #[test]
    fn permission_commit_is_visible_without_a_history_write() {
        let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("permissions.sqlite3");
        let mut writer = fixture(&path); let reader = Connection::open(&path).unwrap();
        writer.execute_batch("CREATE TABLE chat_room_permissions(room_id TEXT PRIMARY KEY, permission_profile TEXT NOT NULL, updated_at INTEGER NOT NULL);").unwrap();
        assert_eq!(permission_revision(&reader,"a").unwrap(),(ROOM_PERMISSION_WORKSPACE_WRITE.into(),0));
        let tx = writer.transaction().unwrap();
        tx.execute("INSERT INTO chat_room_permissions VALUES('a',?1,1)",params![ROOM_PERMISSION_FULL_ACCESS]).unwrap();
        assert_eq!(permission_revision(&reader,"a").unwrap().0,ROOM_PERMISSION_WORKSPACE_WRITE);
        tx.commit().unwrap();
        assert_eq!(permission_revision(&reader,"a").unwrap(),(ROOM_PERMISSION_FULL_ACCESS.into(),1));
        assert_eq!(permission_revision(&reader,"b").unwrap(),(ROOM_PERMISSION_WORKSPACE_WRITE.into(),0));
        assert_eq!(revision(&reader,"a").unwrap(),(0,true));
    }
}
