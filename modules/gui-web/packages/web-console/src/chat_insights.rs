//! 聊天记录索引、逐轮耗时与服务端实际返回的 token 用量。
use super::*;
#[path = "chat_search_index.rs"]
mod search_index;

pub(super) fn ensure_tables(connection: &Connection) -> rusqlite::Result<()> {
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
    request_usage::migrate(connection)?;
    Ok(())
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

pub(super) fn reply_ids(messages: &[ChatMessageDto]) -> Vec<String> {
    messages.iter().filter(|message| matches!(message.kind.as_str(), "assistant-reply" | "assistant-fallback" | "assistant-partial"))
        .map(|message| message.id.clone()).collect()
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SearchQuery {
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    around: Option<String>,
}

// SQLite 是完整历史的真值；内存容量只是工作集，不是历史保留期限。
const VISIBLE_SQL: &str = "lower(trim(role)) <> 'tool' AND lower(replace(trim(kind),'_','-')) NOT IN ('reasoning','tool-call','tool-summary','tool-result','computer-use','vision-computer-use','goal-phase')";

fn history_path(room_id: &str) -> ApiResult<PathBuf> {
    let store = session_store().lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
    if !store.state.chat_rooms.iter().any(|room| room.id == room_id) {
        return Err(api_error(StatusCode::NOT_FOUND, "聊天室不存在"));
    }
    Ok(store.path.clone())
}

fn search_sqlite(connection: &Connection, room_id: &str, query: &SearchQuery) -> rusqlite::Result<JsonValue> {
    // 只含轻量字段的可见消息索引，同时服务计数、位置与游标定位。
    connection.execute_batch(&format!("CREATE INDEX IF NOT EXISTS idx_chat_visible_position_v1 ON chat_room_messages(room_id,created_at,id) WHERE {VISIBLE_SQL}"))?;
    let needle=query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let mut phrase=if query.around.is_none() { search_index::phrase(&needle) } else { None };
    if phrase.is_some() {
        match search_index::prewarm(connection) {
            Ok(true) => (),
            Ok(false) => phrase=None,
            Err(error) => {
                diag_log(&format!("[CHAT-SEARCH] 派生索引不可用，退回完整扫描: {error}"));
                phrase=None;
            }
        }
    }
    let transaction=connection.unchecked_transaction()?;
    // 预热后新增的 dirty 与候选读取共用最后的短事务；索引不完整则扫描真值快照。
    if phrase.is_some() {
        match search_index::prepare(&transaction) {
            Ok(true) => (),
            Ok(false) => phrase=None,
            Err(error) => {
                diag_log(&format!("[CHAT-SEARCH] 索引快照刷新失败，退回完整扫描: {error}"));
                phrase=None;
            }
        }
    }
    let result = search_snapshot(&transaction, room_id, query, phrase.as_deref())?;
    transaction.commit()?;
    Ok(result)
}

fn search_snapshot(connection: &Connection, room_id: &str, query: &SearchQuery, phrase: Option<&str>) -> rusqlite::Result<JsonValue> {
    let total: usize = connection.query_row(&format!("SELECT COUNT(*) FROM chat_room_messages INDEXED BY idx_chat_visible_position_v1 WHERE room_id=?1 AND {VISIBLE_SQL}"), params![room_id], |row| row.get(0))?;
    if let Some(id) = &query.around {
        let target: Option<(i64, String)> = connection.query_row(&format!("SELECT created_at,id FROM chat_room_messages WHERE room_id=?1 AND id=?2 AND {VISIBLE_SQL}"), params![room_id,id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
        let Some((created, id)) = target else { return Ok(json!({"found":false,"messages":[]})); };
        let position: usize = connection.query_row(&format!("SELECT COUNT(*) FROM chat_room_messages INDEXED BY idx_chat_visible_position_v1 WHERE room_id=?1 AND {VISIBLE_SQL} AND (created_at < ?2 OR (created_at=?2 AND id<=?3))"), params![room_id,created,id], |row| row.get(0))?;
        let mut previous = connection.prepare(&format!("SELECT id,author,role,target,content,kind,attachments_json,created_at FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL} AND (created_at<?2 OR (created_at=?2 AND id<?3)) ORDER BY created_at DESC,id DESC LIMIT 20"))?;
        let mut messages = previous.query_map(params![room_id,created,id], persisted_message_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse(); let older_count=messages.len(); let start=position-older_count-1;
        let mut following = connection.prepare(&format!("SELECT id,author,role,target,content,kind,attachments_json,created_at FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL} AND (created_at>?2 OR (created_at=?2 AND id>=?3)) ORDER BY created_at,id LIMIT ?4"))?;
        messages.extend(following.query_map(params![room_id,created,id,(41-older_count) as i64], persisted_message_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?);
        return Ok(json!({"messages":messages,"found":true,"position":position,"start_index":start+1,"total":total,"has_older":start>0,"has_newer":start+messages.len()<total}));
    }
    let needle = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let offset = query.offset.unwrap_or(0); let limit = query.limit.unwrap_or(50).clamp(1,100);
    let mut items = Vec::new(); let mut matched = 0usize;
    if needle.is_empty() {
        let mut statement = connection.prepare(&format!("SELECT id,author,role,content,created_at FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL} ORDER BY created_at DESC,id DESC LIMIT ?2 OFFSET ?3"))?;
        let mut rows = statement.query(params![room_id,limit as i64,offset.min(i64::MAX as usize) as i64])?;
        while let Some(row) = rows.next()? {
            let content: String = row.get(3)?;
            items.push(json!({"id":row.get::<_,String>(0)?,"index":total-offset-items.len(),"author":row.get::<_,String>(1)?,"role":row.get::<_,String>(2)?,"created_at":i64_to_u64(row.get(4)?),"snippet":compact_message_snippet(&strip_context_usage_footer(&content),200)}));
        }
        matched = total;
    } else {
        // 已索引数据也按 Rust lowercase 精确校验，FTS 仅负责减少候选。
        let sql = if phrase.is_some() {
            format!("SELECT id,author,role,content,created_at FROM chat_room_messages WHERE rowid IN (SELECT rowid FROM chat_search_fts_v1 WHERE chat_search_fts_v1 MATCH ?2) AND room_id=?1 AND {VISIBLE_SQL} ORDER BY created_at DESC,id DESC")
        } else {
            format!("SELECT id,author,role,content,created_at FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL} ORDER BY created_at DESC,id DESC")
        };
        let mut statement=connection.prepare(&sql)?;
        let mut rows = if let Some(phrase)=phrase { statement.query(params![room_id,phrase])? } else { statement.query(params![room_id])? };
        let mut index=total;
        while let Some(row)=rows.next()? {
            let author:String=row.get(1)?; let content:String=row.get(3)?;
            if content.to_lowercase().contains(&needle) || author.to_lowercase().contains(&needle) {
                if matched>=offset && items.len()<limit {
                    items.push(json!({"id":row.get::<_,String>(0)?,"index":index,"author":author,"role":row.get::<_,String>(2)?,"created_at":i64_to_u64(row.get(4)?),"snippet":compact_message_snippet(&strip_context_usage_footer(&content),200)}));
                }
                matched+=1;
            }
            index=index.saturating_sub(1);
        }
        if phrase.is_some() && !items.is_empty() {
            // 只扫 covering index 中的 id，避免再读取十万条正文或每个匹配重复计算 COUNT。
            let wanted:BTreeMap<String,usize>=items.iter().enumerate().map(|(i,item)|(item["id"].as_str().unwrap_or_default().to_string(),i)).collect();
            let mut statement=connection.prepare(&format!("SELECT id FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL} ORDER BY created_at,id"))?;
            let mut rows=statement.query(params![room_id])?; let mut position=0usize;
            while let Some(row)=rows.next()? {
                position+=1; let id:String=row.get(0)?;
                if let Some(item)=wanted.get(&id) { items[*item]["index"]=json!(position); }
            }
        }
    }
    Ok(json!({"items":items,"total":matched,"message_count":total,"has_more":offset.saturating_add(limit)<matched}))
}

pub(super) async fn search(AxumPath(room_id): AxumPath<String>, Query(query): Query<SearchQuery>) -> ApiResult<Json<JsonValue>> {
    let path = history_path(&room_id)?; let around = query.around.is_some();
    let response = tokio::task::spawn_blocking(move || {
        let connection = open_session_connection(&path).map_err(sqlite_api_error)?;
        search_sqlite(&connection,&room_id,&query).map_err(sqlite_api_error)
    }).await.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,&error.to_string()))??;
    if around && response["found"] == false { return Err(api_error(StatusCode::NOT_FOUND, "消息已删除或不属于当前聊天室")); }
    Ok(Json(response))
}

pub(super) fn message_page(path: &Path, room_id: &str, query: MessageQuery) -> ApiResult<(Vec<PersistedChatMessage>,bool,Option<String>)> {
    let connection = open_session_connection(path).map_err(sqlite_api_error)?;
    let limit = query.limit.unwrap_or(DEFAULT_MESSAGE_LIMIT).clamp(1,200);
    let before = if let Some(id) = query.before {
        Some(connection.query_row("SELECT created_at,id FROM chat_room_messages WHERE room_id=?1 AND id=?2",params![room_id,id],|row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?))).optional().map_err(sqlite_api_error)?.ok_or_else(|| api_error(StatusCode::NOT_FOUND,"消息游标已删除或不属于当前聊天室"))?)
    } else { None };
    let mut statement = connection.prepare("SELECT id,author,role,target,content,kind,attachments_json,created_at FROM chat_room_messages WHERE room_id=?1 AND (?2 IS NULL OR created_at<?2 OR (created_at=?2 AND id<?3)) ORDER BY created_at DESC,id DESC LIMIT ?4").map_err(sqlite_api_error)?;
    let mut messages = statement.query_map(params![room_id,before.as_ref().map(|value| value.0),before.as_ref().map(|value| value.1.as_str()),(limit+1) as i64],persisted_message_from_row).map_err(sqlite_api_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
    let has_more = messages.len()>limit; if has_more { messages.pop(); } messages.reverse();
    let next_before = has_more.then(|| messages.first().map(|message| message.id.clone())).flatten();
    Ok((messages,has_more,next_before))
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct InsightsQuery { message_ids: Option<String> }

pub(super) async fn insights(AxumPath(room_id): AxumPath<String>, Query(query): Query<InsightsQuery>) -> ApiResult<Json<JsonValue>> {
    let path=history_path(&room_id)?; let workspace=workspace_identity(&active_workspace_path());
    let result=tokio::task::spawn_blocking(move || -> ApiResult<JsonValue> {
        let connection=open_session_connection(&path).map_err(sqlite_api_error)?;
        connection.execute_batch(&format!("CREATE INDEX IF NOT EXISTS idx_chat_visible_position_v1 ON chat_room_messages(room_id,created_at,id) WHERE {VISIBLE_SQL}")).map_err(sqlite_api_error)?;
        ensure_tables(&connection).map_err(sqlite_api_error)?;
        let ids=if let Some(ids)=query.message_ids {
            ids.split(',').filter(|id| !id.is_empty() && id.len()<=256).take(500).map(str::to_string).collect::<Vec<_>>()
        } else {
            let mut statement=connection.prepare(&format!("SELECT id FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL} ORDER BY created_at DESC,id DESC LIMIT 100")).map_err(sqlite_api_error)?;
            let rows=statement.query_map(params![room_id],|row| row.get::<_,String>(0)).map_err(sqlite_api_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
            rows
        };
        let ids_json=json!(ids).to_string();
        let total:usize=connection.query_row(&format!("SELECT COUNT(*) FROM chat_room_messages INDEXED BY idx_chat_visible_position_v1 WHERE room_id=?1 AND {VISIBLE_SQL}"),params![room_id],|row| row.get(0)).map_err(sqlite_api_error)?;
        let mut index_statement=connection.prepare(&format!("WITH ranked AS (SELECT id,row_number() OVER (ORDER BY created_at,id) AS position FROM chat_room_messages WHERE room_id=?1 AND {VISIBLE_SQL}) SELECT id,position FROM ranked WHERE id IN (SELECT value FROM json_each(?2))")).map_err(sqlite_api_error)?;
        let indices=if ids.is_empty() { BTreeMap::new() } else {
            index_statement.query_map(params![room_id,ids_json],|row| Ok((row.get::<_,String>(0)?,row.get::<_,usize>(1)?))).map_err(sqlite_api_error)?.collect::<rusqlite::Result<BTreeMap<_,_>>>().map_err(sqlite_api_error)?
        };
        let usage=request_usage::summary(&connection,&room_id,&workspace).map_err(sqlite_api_error)?;
        let mut timing_statement=connection.prepare("SELECT message_id,elapsed_ms FROM chat_message_timing WHERE room_id=?1 AND message_id IN (SELECT value FROM json_each(?2))").map_err(sqlite_api_error)?;
        let timings=timing_statement.query_map(params![room_id,ids_json],|row| Ok((row.get::<_,String>(0)?,row.get::<_,u64>(1)?))).map_err(sqlite_api_error)?.collect::<rusqlite::Result<BTreeMap<_,_>>>().map_err(sqlite_api_error)?;
        Ok(json!({"room_id":room_id,"usage":usage,"timings":timings,"indices":indices,"message_count":total,
            "source":"provider_reported/acp_journal","note":"HTTP请求与Devin请求台账按唯一请求去重；网络尝试只计明确已派发，未发送、结果未知和派发未知单独显示。只合计供应商已返回的用量，未知字段显示未知，部分用量不代表完整账单。旧版记录独立列出，旧零值无法证明已知用量；缓存是明细，不重复相加，不推算费用。"}))
    }).await.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,&error.to_string()))??;
    Ok(Json(result))
}

/// 只记录接纳时已有的消息标识；查询时还要验证消息已经落盘且仍属于同一房间。
pub(super) fn record_source_messages(path: &Path, run_id: &str, messages: &[ChatMessageDto]) -> rusqlite::Result<()> {
    let ids=messages.iter().filter(|message| message.role=="user").map(|message|message.id.as_str()).collect::<Vec<_>>();
    if !ids.is_empty() { append_runtime_run_event(path,run_id,"chat.source_messages",json!({"message_ids":ids}))?; }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct TraceQuery {
    before: Option<String>,
    limit: Option<usize>,
    run_id: Option<String>,
    message_id: Option<String>,
}

impl TraceQuery {
    pub(super) fn for_run(mut self, run: &str) -> Self {
        self.run_id = Some(run.to_owned());
        self.before = None;
        self.message_id = None;
        self
    }
}

pub(super) async fn trace(AxumPath(room_id): AxumPath<String>, Query(query): Query<TraceQuery>) -> ApiResult<Json<JsonValue>> {
    let path = history_path(&room_id)?; let workspace = workspace_identity(&active_workspace_path());
    let result = tokio::task::spawn_blocking(move || -> ApiResult<JsonValue> {
        let connection = open_session_connection(&path).map_err(sqlite_api_error)?;
        ensure_tables(&connection).map_err(sqlite_api_error)?;
        let mut query = query;
        if let Some(message_id) = query.message_id.as_deref() {
            if message_id.trim().is_empty() || query.run_id.is_some() || query.before.is_some() {
                return Err(api_error(StatusCode::BAD_REQUEST,"回复定位不能同时指定其他运行或分页游标"));
            }
            // 只使用宿主已落盘的回复归属；不按作者、时间或消息ID格式猜运行。
            let mut statement = connection.prepare(
                "SELECT DISTINCT r.id FROM runtime_runs r
                 JOIN runtime_run_events e ON e.run_id=r.id AND e.event_type='chat.context_outputs'
                 JOIN json_each(e.payload_json,'$.messages') stamp
                 JOIN chat_room_messages m ON m.id=json_extract(stamp.value,'$.id')
                 WHERE r.workspace_id=?1 AND r.chat_room_id=?2 AND m.room_id=?2 AND m.id=?3
                   AND json_type(stamp.value,'$.id')='text' AND lower(trim(m.role))='assistant'
                   AND lower(replace(trim(m.kind),'_','-')) IN ('assistant-reply','bot')
                 LIMIT 2"
            ).map_err(sqlite_api_error)?;
            let runs = statement.query_map(params![workspace,room_id,message_id], |row| row.get::<_,String>(0))
                .map_err(sqlite_api_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
            query.run_id = Some(match runs.as_slice() {
                [run] => run.clone(),
                [] => return Err(api_error(StatusCode::NOT_FOUND,"这条回复没有可定位的运行记录")),
                _ => return Err(api_error(StatusCode::CONFLICT,"这条回复对应多个运行，请从运行列表查看")),
            });
        }
        let limit = if query.message_id.is_some() { 1 } else { query.limit.unwrap_or(30).clamp(1,100) };
        if let Some(run) = query.run_id.as_deref() {
            let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM runtime_runs WHERE id=?1 AND workspace_id=?2 AND chat_room_id=?3)",
                params![run,workspace,room_id], |row| row.get(0)).map_err(sqlite_api_error)?;
            if !exists { return Err(api_error(StatusCode::NOT_FOUND,"运行轨迹不存在或不属于本工程房间")); }
        }
        let before = if let Some(id) = query.before {
            Some(connection.query_row("SELECT created_at,id FROM runtime_runs WHERE id=?1 AND workspace_id=?2 AND chat_room_id=?3",params![id,workspace,room_id],|row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?))).optional().map_err(sqlite_api_error)?.ok_or_else(|| api_error(StatusCode::NOT_FOUND,"轨迹游标已失效"))?)
        } else { None };
        let mut statement = connection.prepare("SELECT id,session_id,legacy_turn_id,state,created_at,started_at,finished_at,stop_reason FROM runtime_runs WHERE workspace_id=?1 AND chat_room_id=?2 AND (?6 IS NULL OR id=?6) AND (?3 IS NULL OR created_at<?3 OR (created_at=?3 AND id<?4)) ORDER BY created_at DESC,id DESC LIMIT ?5").map_err(sqlite_api_error)?;
        let mut runs = statement.query_map(params![workspace,room_id,before.as_ref().map(|v|v.0),before.as_ref().map(|v|v.1.as_str()),(limit+1) as i64,query.run_id], |row| Ok(json!({"run_id":row.get::<_,String>(0)?,"session_id":row.get::<_,Option<String>>(1)?,"turn_id":row.get::<_,Option<String>>(2)?,"status":row.get::<_,String>(3)?,"created_at":row.get::<_,i64>(4)?,"started_at":row.get::<_,Option<i64>>(5)?,"finished_at":row.get::<_,Option<i64>>(6)?,"stop_reason":row.get::<_,Option<String>>(7)?}))).map_err(sqlite_api_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
        let has_more = runs.len()>limit; if has_more { runs.pop(); }
        for run in &mut runs {
            let run_id = run["run_id"].as_str().unwrap_or_default().to_string();
            let source:Option<String>=connection.query_row(&format!("SELECT m.id FROM runtime_run_events e JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value AND m.room_id=?2 WHERE e.run_id=?1 AND e.event_type='chat.source_messages' AND {VISIBLE_SQL} ORDER BY m.created_at,m.id LIMIT 1"),params![run_id,room_id],|row| row.get(0)).optional().map_err(sqlite_api_error)?;
            run["source_message_id"]=json!(source);
            let mut tools_statement = connection.prepare("SELECT tool_call_id,tool_name,status,created_at_unix_ms,updated_at_unix_ms FROM tool_calls WHERE run_id=?1 ORDER BY created_at_unix_ms,tool_call_id").map_err(sqlite_api_error)?;
            let calls = tools_statement.query_map(params![run_id], |row| Ok(json!({"call_id":row.get::<_,String>(0)?,"tool_name":row.get::<_,String>(1)?,"status":row.get::<_,String>(2)?,"started_at":row.get::<_,i64>(3)?,"updated_at":row.get::<_,i64>(4)?}))).map_err(sqlite_api_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sqlite_api_error)?;
            run["calls"] = json!(calls);
            let usage = request_usage::requests_for_run(&connection,&workspace,&room_id,&run_id,run["turn_id"].as_str()).map_err(sqlite_api_error)?;
            run["requests"] = json!(usage);
        }
        let next_before = if has_more { runs.last().and_then(|run| run["run_id"].as_str()).map(str::to_string) } else { None };
        Ok(json!({"runs":runs,"has_more":has_more,"next_before":next_before,
            "requested_message_id":query.message_id,"source":"runtime_runs/tool_calls/chat_usage_events/devin_acp_attempts"}))
    }).await.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,&error.to_string()))??;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exact_run_trace_finds_old_run_and_rejects_foreign_scope() {
        let _lock = crate::tests::config_test_guard();
        let _state = crate::multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1/v1");
        {
            let mut store = session_store().lock().unwrap();
            store.state.chat_rooms.push(PersistedChatRoom { id: "exact-trace-room".into(),
                name: "轨迹测试".into(), created_at: 1, updated_at: 1, messages: Vec::new() });
            store.save().unwrap();
        }
        let path = default_session_sqlite_path();
        let workspace = workspace_identity(&active_workspace_path());
        let connection = open_session_connection(&path).unwrap();
        for index in 0..=120 {
            connection.execute("INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,state,claim_token,created_at) VALUES(?1,'chat_turn',?2,'target-text','exact-trace-room','completed','trace-claim',?3)",
                params![format!("trace-{index}"),workspace,index]).unwrap();
        }
        for (id, ws, room) in [("foreign-workspace", "other-workspace", "exact-trace-room"),
            ("foreign-room", workspace.as_str(), "other-room")] {
            connection.execute("INSERT INTO runtime_runs(id,kind,workspace_id,chat_room_id,state,claim_token,created_at) VALUES(?1,'chat_turn',?2,?3,'completed','trace-claim',999)", params![id,ws,room]).unwrap();
        }
        let query = TraceQuery { before: Some("invalid-cursor".into()), limit: Some(1),
            run_id: Some("foreign-room".into()), message_id: Some("ignored-by-run-route".into()) }.for_run("trace-0");
        let Json(result) = trace(AxumPath("exact-trace-room".into()), Query(query)).await.unwrap();
        assert_eq!(result["runs"].as_array().unwrap().len(), 1);
        assert_eq!(result["runs"][0]["run_id"], "trace-0");
        assert_eq!(result["has_more"], false);
        assert!(result["next_before"].is_null());
        for run in ["foreign-workspace", "foreign-room", "missing-run"] {
            let error = trace(AxumPath("exact-trace-room".into()), Query(TraceQuery::default().for_run(run))).await.unwrap_err();
            assert_eq!(error.0, StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn reply_trace_uses_persisted_ownership_and_rejects_foreign_ambiguous_or_deleted_messages() {
        let _lock = crate::tests::config_test_guard();
        let _state = crate::multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1/v1");
        {
            let mut store = session_store().lock().unwrap();
            for id in ["reply-room", "other-reply-room"] {
                store.state.chat_rooms.push(PersistedChatRoom { id:id.into(), name:id.into(),
                    created_at:1, updated_at:1, messages:Vec::new() });
            }
            store.save().unwrap();
        }
        let path = default_session_sqlite_path();
        let workspace = workspace_identity(&active_workspace_path());
        let connection = open_session_connection(&path).unwrap();
        for (id, ws, room) in [
            ("old-reply-run", workspace.as_str(), "reply-room"),
            ("newer-reply-run", workspace.as_str(), "reply-room"),
            ("other-workspace-run", "foreign-workspace", "reply-room"),
            ("other-room-run", workspace.as_str(), "other-reply-room"),
        ] {
            connection.execute("INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,state,claim_token,created_at)
                VALUES(?1,'chat_turn',?2,'target-text',?3,'completed','reply-claim',1)", params![id,ws,room]).unwrap();
        }
        for id in ["reply-owned", "reply-foreign-workspace", "reply-foreign-room", "reply-unmapped"] {
            connection.execute("INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at)
                VALUES('reply-room',?1,'同名显示作者','assistant','chat','已完成回复','assistant-reply','[]',1)", params![id]).unwrap();
        }
        for (run, message) in [("old-reply-run","reply-owned"),
            ("other-workspace-run","reply-foreign-workspace"), ("other-room-run","reply-foreign-room")] {
            crate::devin_acp::context::record_outputs(&path,run,"reply-room","target-text",&[message.into()]).unwrap();
        }
        let request = |message: &str| TraceQuery {message_id:Some(message.into()),..TraceQuery::default()};
        let Json(found) = trace(AxumPath("reply-room".into()),Query(request("reply-owned"))).await.unwrap();
        assert_eq!(found["requested_message_id"],"reply-owned");
        assert_eq!(found["runs"].as_array().unwrap().len(),1);
        assert_eq!(found["runs"][0]["run_id"],"old-reply-run");
        assert_eq!(found["has_more"],false);
        for message in ["reply-foreign-workspace","reply-foreign-room","reply-unmapped","missing-reply"] {
            assert_eq!(trace(AxumPath("reply-room".into()),Query(request(message))).await.unwrap_err().0,StatusCode::NOT_FOUND);
        }
        let conflict = TraceQuery {message_id:Some("reply-owned".into()),run_id:Some("newer-reply-run".into()),..TraceQuery::default()};
        assert_eq!(trace(AxumPath("reply-room".into()),Query(conflict)).await.unwrap_err().0,StatusCode::BAD_REQUEST);
        crate::devin_acp::context::record_outputs(&path,"newer-reply-run","reply-room","target-text",&["reply-owned".into()]).unwrap();
        assert_eq!(trace(AxumPath("reply-room".into()),Query(request("reply-owned"))).await.unwrap_err().0,StatusCode::CONFLICT);
        connection.execute("DELETE FROM chat_room_messages WHERE id='reply-owned'",[]).unwrap();
        assert_eq!(trace(AxumPath("reply-room".into()),Query(request("reply-owned"))).await.unwrap_err().0,StatusCode::NOT_FOUND);
    }

    fn search_messages(messages: &[PersistedChatMessage], query: &SearchQuery) -> JsonValue {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE chat_room_messages(room_id TEXT,id TEXT,author TEXT,role TEXT,target TEXT,content TEXT,kind TEXT,attachments_json TEXT,created_at INTEGER,PRIMARY KEY(room_id,id));").unwrap();
        for (index,message) in messages.iter().enumerate() {
            connection.execute("INSERT INTO chat_room_messages VALUES('fixture',?1,?2,?3,?4,?5,?6,'[]',?7)", params![message.id,message.author,message.role,message.target,message.content,message.kind,index as i64]).unwrap();
        }
        search_sqlite(&connection,"fixture",query).unwrap()
    }

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
    fn trigram_matches_rust_unicode_and_literal_queries() {
        let messages=vec![message("u","CAFÉΩ İSTANBUL STRAẞE 😊骑车🚲 中文测试 a\"b%_c","assistant-reply")];
        for query in ["caféω","i\u{307}sta","straße","😊骑车","中文测","a\"b%","骑车"] {
            let result=search_messages(&messages,&SearchQuery{q:Some(query.into()),..Default::default()});
            assert_eq!(result["total"],1,"Unicode/literal query {query}");
        }
    }

    #[test]
    fn dirty_index_tracks_update_delete_rowid_reuse_and_rollback() {
        let connection=Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE chat_room_messages(room_id TEXT,id TEXT,author TEXT,role TEXT,target TEXT,content TEXT,kind TEXT,attachments_json TEXT,created_at INTEGER,PRIMARY KEY(room_id,id));
            INSERT INTO chat_room_messages VALUES('r','a','助手','assistant','','old-needle','assistant-reply','[]',1);").unwrap();
        let query=|q:&str| SearchQuery{q:Some(q.into()),..Default::default()};
        assert_eq!(search_sqlite(&connection,"r",&query("old-needle")).unwrap()["total"],1);
        connection.execute_batch("UPDATE chat_room_messages SET content='new-needle' WHERE id='a';").unwrap();
        assert_eq!(search_sqlite(&connection,"r",&query("old-needle")).unwrap()["total"],0);
        assert_eq!(search_sqlite(&connection,"r",&query("new-needle")).unwrap()["total"],1);
        connection.execute_batch("BEGIN; DELETE FROM chat_room_messages; ROLLBACK;").unwrap();
        assert_eq!(search_sqlite(&connection,"r",&query("new-needle")).unwrap()["total"],1);
        connection.execute_batch("DELETE FROM chat_room_messages;
            INSERT INTO chat_room_messages VALUES('r','b','助手','assistant','','reused-needle','assistant-reply','[]',2);").unwrap();
        assert_eq!(search_sqlite(&connection,"r",&query("new-needle")).unwrap()["total"],0);
        assert_eq!(search_sqlite(&connection,"r",&query("reused-needle")).unwrap()["items"][0]["id"],"b");
        connection.execute_batch("UPDATE chat_room_messages SET kind='tool-result';").unwrap();
        assert_eq!(search_sqlite(&connection,"r",&query("reused-needle")).unwrap()["total"],0);
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
    fn live_search_reads_while_progress_writes_stay_bounded() {
        // 持续 UPSERT 期间读者走真实 search_sqlite：无 SQL 异常、结果不越房间边界。
        let directory = tempfile::tempdir().unwrap();
        let db = directory.path().join("sessions.sqlite3");
        let schema = "CREATE TABLE chat_room_messages(room_id TEXT,id TEXT,author TEXT,role TEXT,target TEXT,content TEXT,kind TEXT,attachments_json TEXT,created_at INTEGER,PRIMARY KEY(room_id,id));";
        fn live_upsert(connection: &Connection, room: &str, id: &str, content: &str, created_at: i64) -> rusqlite::Result<usize> {
            connection.execute(
                "INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at)
                 VALUES(?1,?2,'助手','assistant','',?3,'assistant-partial','[]',?4)
                 ON CONFLICT(room_id,id) DO UPDATE SET author=excluded.author, role=excluded.role,
                     target=excluded.target, content=excluded.content, kind=excluded.kind,
                     attachments_json=excluded.attachments_json, created_at=excluded.created_at",
                params![room, id, content, created_at])
        }
        {
            let seed = Connection::open(&db).unwrap();
            seed.execute_batch(schema).unwrap();
            // 异房间含同词干扰：搜索必须被房间边界挡住。
            live_upsert(&seed, "b", "foreign", "live-write-marker 外部房间正文", 1).unwrap();
            live_upsert(&seed, "a", "m1", "live-write-marker v0", 1).unwrap();
            // 先跑一次真实搜索建立索引与派生结构，避免并发窗口内首次 DDL 竞争。
            search_sqlite(&seed, "a", &SearchQuery { q: Some("live-write-marker".into()), ..Default::default() }).unwrap();
        }
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            let writer_db = &db;
            let barrier_ref = &barrier;
            scope.spawn(move || {
                let connection = Connection::open(writer_db).unwrap();
                connection.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                barrier_ref.wait();
                for pass in 1..=24 {
                    live_upsert(&connection, "a", "m1", &format!("live-write-marker v{pass}"), pass).unwrap();
                }
            });
            let reader_db = &db;
            let barrier_ref = &barrier;
            scope.spawn(move || {
                let connection = Connection::open(reader_db).unwrap();
                connection.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                barrier_ref.wait();
                let query = SearchQuery { q: Some("live-write-marker".into()), ..Default::default() };
                for _ in 0..24 {
                    let result = search_sqlite(&connection, "a", &query)
                        .expect("并发读取不得产生 SQL 异常");
                    for item in result["items"].as_array().unwrap() {
                        assert_eq!(item["id"].as_str().unwrap(), "m1", "结果必须只属于当前房间");
                        assert!(!item["snippet"].as_str().unwrap_or_default().contains("外部房间"),
                            "片段不得泄漏异房间正文");
                    }
                }
            });
            barrier.wait();
        });
        // 写完后的终态检索：最终版本命中，被覆盖的旧版本不再命中。
        let connection = Connection::open(&db).unwrap();
        connection.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        let result = search_sqlite(&connection, "a", &SearchQuery { q: Some("v24".into()), ..Default::default() }).unwrap();
        assert_eq!(result["total"].as_u64().unwrap(), 1);
        assert_eq!(result["items"][0]["id"].as_str().unwrap(), "m1");
        let stale = search_sqlite(&connection, "a", &SearchQuery { q: Some("v0".into()), ..Default::default() }).unwrap();
        assert_eq!(stale["total"].as_u64().unwrap(), 0);
        let foreign = search_sqlite(&connection, "b", &SearchQuery { q: Some("live-write-marker".into()), ..Default::default() }).unwrap();
        for item in foreign["items"].as_array().unwrap() {
            assert_eq!(item["id"].as_str().unwrap(), "foreign");
        }
    }

}
