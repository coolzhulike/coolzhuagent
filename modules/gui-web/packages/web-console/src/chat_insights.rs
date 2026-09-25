//! 聊天记录索引、逐轮耗时与服务端实际返回的 token 用量。
use super::*;

pub(super) fn merge_stream_usage(total: &mut api::Usage, snapshot: &api::Usage) {
    // 流式协议可能在 start 提供输入、在 delta 提供累计输出；重复快照不可累加。
    total.input_tokens = total.input_tokens.max(snapshot.input_tokens);
    total.output_tokens = total.output_tokens.max(snapshot.output_tokens);
    total.cache_read_input_tokens = total.cache_read_input_tokens.max(snapshot.cache_read_input_tokens);
    total.cache_creation_input_tokens = total.cache_creation_input_tokens.max(snapshot.cache_creation_input_tokens);
}

/// 一个流式请求只写一条用量事实；提前取消或流被丢弃也保存已收到的快照。
pub(super) struct StreamUsageRecorder {
    session_id: String,
    room_id: String,
    usage: Option<api::Usage>,
}

impl StreamUsageRecorder {
    pub(super) fn new(session_id: &str, room_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            room_id: room_id.to_string(),
            usage: Some(api::Usage { input_tokens: 0, output_tokens: 0,
                cache_read_input_tokens: 0, cache_creation_input_tokens: 0 }),
        }
    }

    pub(super) fn merge(&mut self, snapshot: &api::Usage) {
        if let Some(total) = self.usage.as_mut() { merge_stream_usage(total, snapshot); }
    }

    pub(super) fn finish(&mut self) {
        // take 是唯一写入出口，显式完成后 Drop 不再累计第二次。
        if let Some(usage) = self.usage.take() {
            record_usage(&self.session_id, Some(&self.room_id), &usage);
        }
    }
}

impl Drop for StreamUsageRecorder {
    fn drop(&mut self) { self.finish(); }
}

fn ensure_tables(connection: &Connection) -> rusqlite::Result<()> {
    // 独立事实表不参与会话快照重写，刷新或重启不会丢失统计。
    connection.execute_batch("CREATE TABLE IF NOT EXISTS chat_usage_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT, workspace_id TEXT NOT NULL,
        room_id TEXT NOT NULL, session_id TEXT NOT NULL, created_at INTEGER NOT NULL,
        input_tokens INTEGER NOT NULL, output_tokens INTEGER NOT NULL,
        cache_read_tokens INTEGER NOT NULL, cache_write_tokens INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS idx_chat_usage_scope ON chat_usage_events(workspace_id, room_id);
        CREATE TABLE IF NOT EXISTS chat_message_timing (
        message_id TEXT PRIMARY KEY, room_id TEXT NOT NULL, elapsed_ms INTEGER NOT NULL);")?;
    let columns = connection.prepare("PRAGMA table_info(chat_usage_events)")?
        .query_map([], |row| row.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for column in ["turn_id", "call_id", "request_kind"] {
        if !columns.iter().any(|name| name == column) {
            if let Err(error) = connection.execute_batch(&format!("ALTER TABLE chat_usage_events ADD COLUMN {column} TEXT")) {
                // 两个并发请求可能都读到旧列清单；仅在另一连接已成功加列时忽略重复迁移。
                let now_exists = connection.prepare("PRAGMA table_info(chat_usage_events)")?
                    .query_map([], |row| row.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?
                    .iter().any(|name| name == column);
                if !now_exists { return Err(error); }
            }
        }
    }
    Ok(())
}

pub(super) fn record_usage(session_id: &str, room_id: Option<&str>, usage: &api::Usage) {
    record_usage_with_context(session_id, room_id, usage, None);
}

pub(super) struct UsageContext<'a> {
    pub turn_id: &'a str,
    pub call_id: &'a str,
    pub kind: &'a str,
}

pub(super) fn record_usage_with_context(session_id: &str, room_id: Option<&str>, usage: &api::Usage, context: Option<&UsageContext<'_>>) {
    let Some(room_id) = room_id else { return };
    if usage.input_tokens == 0 && usage.output_tokens == 0
        && usage.cache_read_input_tokens == 0 && usage.cache_creation_input_tokens == 0 {
        return;
    }
    let result = (|| -> rusqlite::Result<()> {
        let connection = open_session_connection(&default_session_sqlite_path())?;
        ensure_tables(&connection)?;
        connection.execute("INSERT INTO chat_usage_events
            (workspace_id, room_id, session_id, created_at, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, turn_id, call_id, request_kind)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)", params![
                workspace_identity(&active_workspace_path()), room_id, session_id,
                unix_timestamp_millis() as i64, usage.input_tokens, usage.output_tokens,
                usage.cache_read_input_tokens, usage.cache_creation_input_tokens,
                context.map(|value| value.turn_id), context.map(|value| value.call_id), context.map(|value| value.kind)])?;
        Ok(())
    })();
    if let Err(error) = result { diag_log(&format!("[CHAT-USAGE] 保存用量失败: {error}")); }
}

pub(super) fn record_timing(room_id: &str, message_ids: &[String], elapsed_ms: u64) {
    let result = (|| -> rusqlite::Result<()> {
        let mut connection = open_session_connection(&default_session_sqlite_path())?;
        ensure_tables(&connection)?;
        let transaction = connection.transaction()?;
        for id in message_ids {
            transaction.execute("INSERT OR REPLACE INTO chat_message_timing (message_id, room_id, elapsed_ms) VALUES (?1, ?2, ?3)",
                params![id, room_id, elapsed_ms.min(i64::MAX as u64) as i64])?;
        }
        transaction.commit()
    })();
    if let Err(error) = result { diag_log(&format!("[CHAT-TIMING] 保存耗时失败: {error}")); }
}

fn visible_message(message: &PersistedChatMessage) -> bool {
    // 与前端正文过滤一致；历史回放还可能携带下划线格式的工具类型。
    let kind = message.kind.trim().to_ascii_lowercase().replace('_', "-");
    !matches!(kind.as_str(), "reasoning" | "tool-call" | "tool-summary" | "tool-result" | "computer-use" | "vision-computer-use" | "goal-phase")
        && !message.role.trim().eq_ignore_ascii_case("tool")
}

pub(super) fn reply_ids(messages: &[ChatMessageDto]) -> Vec<String> {
    messages.iter().filter(|message| matches!(message.kind.as_str(), "assistant-reply" | "assistant-fallback"))
        .map(|message| message.id.clone()).collect()
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SearchQuery {
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    around: Option<String>,
}

fn search_messages(messages: &[PersistedChatMessage], query: &SearchQuery) -> JsonValue {
    let visible: Vec<_> = messages.iter().filter(|message| visible_message(message)).collect();
    if let Some(id) = &query.around {
        let Some(position) = visible.iter().position(|message| &message.id == id) else {
            return json!({"messages": [], "found": false});
        };
        let start = position.saturating_sub(20);
        let end = (position + 21).min(visible.len());
        return json!({"messages": visible[start..end], "found": true,
            "position": position + 1, "total": visible.len(), "has_older": start > 0, "has_newer": end < visible.len()});
    }
    let needle = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let matches: Vec<_> = visible.iter().enumerate().filter(|(_, message)| {
        needle.is_empty() || message.content.to_lowercase().contains(&needle)
            || message.author.to_lowercase().contains(&needle)
    }).collect();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let items: Vec<_> = matches.iter().rev().skip(offset).take(limit).map(|(index, message)| {
        json!({"id": message.id, "index": index + 1, "author": message.author,
            "role": message.role, "created_at": message.created_at,
            "snippet": compact_message_snippet(&strip_context_usage_footer(&message.content), 200)})
    }).collect();
    json!({"items": items, "total": matches.len(), "message_count": visible.len(),
        "has_more": offset.saturating_add(limit) < matches.len()})
}

pub(super) async fn search(AxumPath(room_id): AxumPath<String>, Query(query): Query<SearchQuery>) -> ApiResult<Json<JsonValue>> {
    let store = session_store().lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
    let room = store.state.chat_rooms.iter().find(|room| room.id == room_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "聊天室不存在"))?;
    let response = search_messages(&room.messages, &query);
    if query.around.is_some() && response["found"] == false {
        return Err(api_error(StatusCode::NOT_FOUND, "消息已删除或不属于当前聊天室"));
    }
    Ok(Json(response))
}

pub(super) async fn insights(AxumPath(room_id): AxumPath<String>) -> ApiResult<Json<JsonValue>> {
    let indices = {
        let store = session_store().lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
        let room = store.state.chat_rooms.iter().find(|room| room.id == room_id)
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "聊天室不存在"))?;
        room.messages.iter().filter(|message| visible_message(message)).enumerate()
            .map(|(index, message)| (message.id.clone(), index + 1)).collect::<BTreeMap<_, _>>()
    };
    let connection = open_session_connection(&default_session_sqlite_path()).map_err(sqlite_api_error)?;
    ensure_tables(&connection).map_err(sqlite_api_error)?;
    let mut statement = connection.prepare("SELECT session_id, COUNT(*), SUM(input_tokens), SUM(output_tokens), SUM(cache_read_tokens), SUM(cache_write_tokens)
        FROM chat_usage_events WHERE room_id = ?1 AND workspace_id = ?2 GROUP BY session_id").map_err(sqlite_api_error)?;
    let rows = statement.query_map(params![room_id, workspace_identity(&active_workspace_path())], |row| {
        Ok(json!({"session_id": row.get::<_, String>(0)?, "requests": row.get::<_, u64>(1)?,
            "input_tokens": row.get::<_, u64>(2)?, "output_tokens": row.get::<_, u64>(3)?,
            "cache_read_tokens": row.get::<_, u64>(4)?, "cache_write_tokens": row.get::<_, u64>(5)?}))
    }).map_err(sqlite_api_error)?;
    let usage = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
    let mut timing_statement = connection.prepare("SELECT message_id, elapsed_ms FROM chat_message_timing WHERE room_id = ?1").map_err(sqlite_api_error)?;
    let timings = timing_statement.query_map(params![room_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
    }).map_err(sqlite_api_error)?.collect::<rusqlite::Result<BTreeMap<_, _>>>().map_err(sqlite_api_error)?;
    Ok(Json(json!({"room_id": room_id, "usage": usage, "timings": timings, "indices": indices,
        "source": "provider_reported", "note": "从此版本开始累计接口已返回且适配器可解析的用量；无 usage 的请求与此前历史不计入。缓存字段单列，不重复相加；未提供或未解析的缓存明细显示 0。"})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, content: &str, kind: &str) -> PersistedChatMessage {
        PersistedChatMessage { id: id.into(), author: "助手".into(), role: "assistant".into(), target: "".into(),
            content: content.into(), kind: kind.into(), attachments: vec![], created_at: 1 }
    }

    #[test]
    fn searches_entire_history_with_stable_indices_and_hides_internal_events() {
        let messages = vec![message("a", "旧的设计 100%", "assistant-reply"), message("b", "秘密思考", "reasoning"), message("c", "新的设计", "assistant-reply")];
        let result = search_messages(&messages, &SearchQuery { q: Some("100%".into()), ..SearchQuery::default() });
        assert_eq!(result["total"], 1);
        assert_eq!(result["items"][0]["index"], 1);
        let located = search_messages(&messages, &SearchQuery { around: Some("c".into()), ..SearchQuery::default() });
        assert_eq!(located["position"], 2);
        assert_eq!(located["messages"].as_array().unwrap().len(), 2);
        assert_eq!(search_messages(&messages, &SearchQuery { q: Some("秘密".into()), ..SearchQuery::default() })["total"], 0);
    }

    #[test]
    fn tool_results_and_legacy_aliases_are_absent_from_search_and_message_indices() {
        let mut messages = vec![message("before", "正常回复", "assistant-reply")];
        for (index, kind) in ["tool-call", "tool-summary", "tool-result", "computer-use",
            "vision-computer-use", "tool_call", "tool_summary", "tool_result", " TOOL_RESULT "]
            .into_iter().enumerate() {
            messages.push(message(&format!("tool-{index}"), "工具内部结果", kind));
        }
        let mut legacy = message("legacy-role", "工具内部结果", "text");
        legacy.role = " Tool ".into();
        messages.push(legacy);
        messages.push(message("after", "最终回复", "assistant-reply"));

        let result = search_messages(&messages, &SearchQuery::default());
        assert_eq!(result["total"], 2);
        assert_eq!(result["message_count"], 2);
        assert_eq!(result["items"][0]["id"], "after");
        assert_eq!(result["items"][0]["index"], 2);
        assert_eq!(search_messages(&messages, &SearchQuery { q: Some("工具内部结果".into()), ..SearchQuery::default() })["total"], 0);
        assert_eq!(search_messages(&messages, &SearchQuery { around: Some("tool-2".into()), ..SearchQuery::default() })["found"], false);
        let located = search_messages(&messages, &SearchQuery { around: Some("after".into()), ..SearchQuery::default() });
        assert_eq!(located["position"], 2);
        assert_eq!(located["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn usage_tables_survive_reinitialization() {
        let connection = Connection::open_in_memory().unwrap();
        ensure_tables(&connection).unwrap();
        connection.execute("INSERT INTO chat_usage_events(id,workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,turn_id,call_id,request_kind) VALUES(1,'w','r','s',1,12,3,4,0,'turn','call','computer_use_planning')", []).unwrap();
        ensure_tables(&connection).unwrap();
        let count: u32 = connection.query_row("SELECT COUNT(*) FROM chat_usage_events", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
        let context: (String,String,String) = connection.query_row("SELECT turn_id,call_id,request_kind FROM chat_usage_events WHERE id=1", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(context, ("turn".into(),"call".into(),"computer_use_planning".into()));
    }

    #[test]
    fn stream_usage_merges_partial_cumulative_snapshots_without_double_counting() {
        let mut total = api::Usage { input_tokens: 120, output_tokens: 0,
            cache_read_input_tokens: 40, cache_creation_input_tokens: 5 };
        let delta = api::Usage { input_tokens: 0, output_tokens: 15,
            cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };
        merge_stream_usage(&mut total, &delta);
        merge_stream_usage(&mut total, &delta);
        assert_eq!(total.input_tokens, 120);
        assert_eq!(total.output_tokens, 15);
        assert_eq!(total.cache_read_input_tokens, 40);
        assert_eq!(total.cache_creation_input_tokens, 5);
        merge_stream_usage(&mut total, &api::Usage { output_tokens: 20, ..delta });
        assert_eq!(total.output_tokens, 20);
    }

    #[test]
    fn stream_usage_recorder_records_completion_and_cancelled_scope_once_each() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("usage-fixture.sqlite3");
        struct RestorePath(Option<PathBuf>);
        impl Drop for RestorePath {
            fn drop(&mut self) { crate::replace_session_db_path_override_for_test(self.0.take()); }
        }
        let _restore = RestorePath(crate::replace_session_db_path_override_for_test(Some(db_path.clone())));
        let usage = api::Usage { input_tokens: 10, output_tokens: 3,
            cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };
        {
            let mut complete = StreamUsageRecorder::new("agent", "room");
            complete.merge(&usage);
            complete.finish();
            complete.finish();
        }
        {
            // 取消时只收到输入快照，不能凭已显示文本估算输出 token。
            let mut cancelled = StreamUsageRecorder::new("agent", "room");
            cancelled.merge(&api::Usage { input_tokens: 12, output_tokens: 0, ..usage });
        }
        drop(StreamUsageRecorder::new("agent", "room"));
        let connection = Connection::open(db_path).unwrap();
        let totals: (u64, u64, u64) = connection.query_row(
            "SELECT COUNT(*), SUM(input_tokens), SUM(output_tokens) FROM chat_usage_events", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
        assert_eq!(totals, (2, 22, 3));
    }
}
