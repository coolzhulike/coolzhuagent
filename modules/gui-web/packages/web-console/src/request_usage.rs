//! HTTP请求投影与ACP发送前台账的统一读取；不重复持久化ACP事实。
use super::*;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

#[derive(Debug)]
struct LedgerObserver {
    path: PathBuf, workspace: String, room: String, session: String,
    turn: Option<String>, run: Option<String>, call: Option<String>, kind: String, logical_id: String,
}

pub(super) fn observe(
    client: Result<ProviderClient, api::ApiError>, session: &str, room: Option<&str>,
    turn: Option<&str>, call: Option<&str>, kind: &str,
) -> Result<ProviderClient, api::ApiError> {
    observe_in_workspace_with_run(client, session, room, turn, call, kind, None, None)
}

pub(super) fn observe_with_run(
    client: Result<ProviderClient, api::ApiError>, session: &str, room: Option<&str>,
    turn: Option<&str>, call: Option<&str>, kind: &str, run: Option<&str>,
) -> Result<ProviderClient, api::ApiError> {
    observe_in_workspace_with_run(client, session, room, turn, call, kind, None, run)
}

pub(super) fn observe_in_workspace(
    client: Result<ProviderClient, api::ApiError>, session: &str, room: Option<&str>,
    turn: Option<&str>, call: Option<&str>, kind: &str,
    frozen_workspace: Option<(&str, &Path)>,
) -> Result<ProviderClient, api::ApiError> {
    observe_in_workspace_with_run(client, session, room, turn, call, kind, frozen_workspace, None)
}

pub(super) fn observe_in_workspace_with_run(
    client: Result<ProviderClient, api::ApiError>, session: &str, room: Option<&str>,
    turn: Option<&str>, call: Option<&str>, kind: &str,
    frozen_workspace: Option<(&str, &Path)>, run: Option<&str>,
) -> Result<ProviderClient, api::ApiError> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let (path, workspace) = frozen_workspace
        .map(|(workspace, path)| (path.to_path_buf(), workspace.to_string()))
        .unwrap_or_else(|| (default_session_sqlite_path(), workspace_identity(&active_workspace_path())));
    let observer=Arc::new(LedgerObserver {
        path,workspace,
        room:room.unwrap_or("").to_string(),session:session.to_string(),
        turn:turn.map(str::to_string),run:run.map(str::to_string),
        call:call.map(str::to_string),kind:kind.to_string(),
        logical_id:format!("request-{}-{}-{}",unix_timestamp_millis(),std::process::id(),SEQUENCE.fetch_add(1,Ordering::Relaxed)),
    });
    use api::RequestObserver;
    observer.update(&api::RequestAttemptSnapshot::default());
    match client {
        Ok(client)=>Ok(client.with_request_observer(observer)),
        Err(error)=>{
            observer.update(&api::RequestAttemptSnapshot {status:"configuration_error",terminal:true,..Default::default()});
            Err(error)
        }
    }
}

impl api::RequestObserver for LedgerObserver {
    fn update(&self, event:&api::RequestAttemptSnapshot) {
        let result=(||->rusqlite::Result<()> {
            let connection=open_session_connection(&self.path)?;
            chat_insights::ensure_tables(&connection)?;
            let timestamp=unix_timestamp_millis() as i64;
            let usage=event.usage;
            connection.execute("INSERT INTO chat_usage_events
                (workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,
                 turn_id,run_id,call_id,request_kind,logical_request_id,attempt_id,attempt_no,status,dispatched,usage_known_mask,http_status,finished_at)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
                ON CONFLICT(attempt_id) DO UPDATE SET
                  input_tokens=MAX(input_tokens,excluded.input_tokens),output_tokens=MAX(output_tokens,excluded.output_tokens),
                  cache_read_tokens=MAX(cache_read_tokens,excluded.cache_read_tokens),cache_write_tokens=MAX(cache_write_tokens,excluded.cache_write_tokens),
                  status=excluded.status,dispatched=excluded.dispatched,usage_known_mask=usage_known_mask|excluded.usage_known_mask,
                  http_status=excluded.http_status,finished_at=COALESCE(finished_at,excluded.finished_at)",
                params![self.workspace,self.room,self.session,timestamp,usage.input_tokens.unwrap_or(0),usage.output_tokens.unwrap_or(0),
                    usage.cache_read_tokens.unwrap_or(0),usage.cache_write_tokens.unwrap_or(0),self.turn,self.run,self.call,self.kind,self.logical_id,
                    format!("{}:{}",self.logical_id,event.attempt_no),event.attempt_no,event.status,event.dispatched,usage.known_mask(),
                    event.http_status,event.terminal.then_some(timestamp)])?;
            Ok(())
        })();
        if let Err(error)=result { diag_log(&format!("[REQUEST-USAGE] 请求事实保存失败: {error}")); }
    }
}

impl Drop for LedgerObserver {
    fn drop(&mut self) {
        // 取消信号可能在 send future 首次被轮询前到达；此时仍须收尾入口的 prepared 行。
        // Arc 的最后一个持有者消失才执行，不影响仍在后台等待真实响应的 CU 请求。
        let result=(||->rusqlite::Result<()> {
            let connection=open_session_connection(&self.path)?;
            connection.execute("UPDATE chat_usage_events SET
                status=CASE WHEN dispatched=1 THEN 'remote_unknown' ELSE 'abandoned_before_dispatch' END,
                finished_at=COALESCE(finished_at,?2)
                WHERE logical_request_id=?1 AND status IN ('prepared','dispatched')",
                params![self.logical_id,unix_timestamp_millis() as i64])?;
            Ok(())
        })();
        if let Err(error)=result {diag_log(&format!("[REQUEST-USAGE] 请求观察结束保存失败: {error}"));}
    }
}

pub(super) fn migrate(connection:&Connection)->rusqlite::Result<()> {
    let columns=connection.prepare("PRAGMA table_info(chat_usage_events)")?.query_map([],|row|row.get::<_,String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (name,definition) in [
        ("run_id","TEXT"),
        ("logical_request_id","TEXT"),("attempt_id","TEXT"),("attempt_no","INTEGER"),
        ("status","TEXT NOT NULL DEFAULT 'legacy'"),("dispatched","INTEGER"),
        ("usage_known_mask","INTEGER"),("http_status","INTEGER"),("finished_at","INTEGER"),
    ] {
        if !columns.iter().any(|column|column==name) {
            if let Err(error)=connection.execute_batch(&format!("ALTER TABLE chat_usage_events ADD COLUMN {name} {definition}")) {
                let exists=connection.prepare("PRAGMA table_info(chat_usage_events)")?.query_map([],|row|row.get::<_,String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?.iter().any(|column|column==name);
                if !exists {return Err(error);}
            }
        }
    }
    connection.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_chat_usage_attempt ON chat_usage_events(attempt_id);
        CREATE INDEX IF NOT EXISTS idx_chat_usage_run ON chat_usage_events(run_id) WHERE run_id IS NOT NULL;")
}

/// 同一attempt优先取ACP权威归属和状态，只从同作用域旧投影复用真实用量。
/// 旧库没有ACP表时保持HTTP读取；损坏JSON/缺失身份显式失败，不能返回虚假零。
fn facts_sql(connection: &Connection) -> rusqlite::Result<String> {
    let http = "SELECT workspace_id,room_id,session_id,created_at,turn_id,run_id,call_id,request_kind,
        logical_request_id,attempt_id,status,dispatched,usage_known_mask,
        input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,
        'http' AS fact_source,NULL AS protocol_stop,NULL AS process_drained FROM chat_usage_events";
    let has_acp = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='devin_acp_attempts')", [], |row| row.get::<_,bool>(0))?;
    if !has_acp { return Ok(http.into()); }
    Ok(format!("{http} u WHERE NOT EXISTS(SELECT 1 FROM devin_acp_attempts a
          WHERE substr(u.attempt_id,1,4)='acp:' AND a.attempt_id=substr(u.attempt_id,5))
        UNION ALL SELECT
        json_extract(a.scope_json,'$.workspace_id'),json_extract(a.scope_json,'$.room_id'),
        json_extract(a.scope_json,'$.agent_id'),u.created_at,
        json_extract(a.scope_json,'$.turn_id'),json_extract(a.scope_json,'$.run_id'),u.call_id,
        COALESCE(u.request_kind,CASE WHEN json_extract(a.scope_json,'$.lane')='internal' THEN 'devin_internal' ELSE 'devin_chat' END),
        'acp:'||a.attempt_id,'acp:'||a.attempt_id,
        CASE a.state WHEN 'prepared' THEN 'prepared' WHEN 'submitted' THEN 'dispatched'
          WHEN 'not_sent' THEN 'not_sent' WHEN 'unknown' THEN 'remote_unknown'
          WHEN 'terminal' THEN CASE a.protocol_stop WHEN 'end_turn' THEN 'completed'
            WHEN 'cancelled' THEN 'cancelled' WHEN 'refusal' THEN 'refusal'
            WHEN 'max_tokens' THEN 'max_tokens' WHEN 'max_turn_requests' THEN 'max_turn_requests'
            ELSE 'remote_unknown' END ELSE 'remote_unknown' END,
        CASE a.state WHEN 'prepared' THEN 0 WHEN 'not_sent' THEN 0 WHEN 'submitted' THEN 1 WHEN 'terminal' THEN 1 ELSE NULL END,
        COALESCE(u.usage_known_mask,0),COALESCE(u.input_tokens,0),COALESCE(u.output_tokens,0),
        COALESCE(u.cache_read_tokens,0),COALESCE(u.cache_write_tokens,0),
        'acp',a.protocol_stop,a.process_drained
        FROM devin_acp_attempts a LEFT JOIN chat_usage_events u
          ON u.attempt_id='acp:'||a.attempt_id
          AND u.workspace_id=json_extract(a.scope_json,'$.workspace_id')
          AND u.room_id=json_extract(a.scope_json,'$.room_id')
          AND u.session_id=json_extract(a.scope_json,'$.agent_id')
          AND (u.run_id IS NULL OR u.run_id=json_extract(a.scope_json,'$.run_id'))
          AND (u.turn_id IS NULL OR u.turn_id=json_extract(a.scope_json,'$.turn_id'))"))
}

pub(super) fn summary(connection:&Connection,room:&str,workspace:&str)->rusqlite::Result<Vec<JsonValue>> {
    let mut statement=connection.prepare(&format!("WITH usage_facts AS ({}) SELECT session_id,COUNT(DISTINCT logical_request_id),
        SUM(CASE WHEN dispatched=1 THEN 1 ELSE 0 END),SUM(CASE WHEN attempt_id IS NULL THEN 1 ELSE 0 END),
        SUM(CASE WHEN attempt_id IS NOT NULL AND COALESCE(usage_known_mask,0)!=15 THEN 1 ELSE 0 END),
        SUM(CASE WHEN status NOT IN ('legacy','prepared','dispatched','completed','not_sent','remote_unknown') THEN 1 ELSE 0 END),
        SUM(CASE WHEN status IN ('prepared','dispatched') THEN 1 ELSE 0 END),
        SUM(CASE WHEN (attempt_id IS NULL AND input_tokens>0) OR usage_known_mask&1!=0 THEN input_tokens END),
        SUM(CASE WHEN (attempt_id IS NULL AND output_tokens>0) OR usage_known_mask&2!=0 THEN output_tokens END),
        SUM(CASE WHEN (attempt_id IS NULL AND cache_read_tokens>0) OR usage_known_mask&4!=0 THEN cache_read_tokens END),
        SUM(CASE WHEN (attempt_id IS NULL AND cache_write_tokens>0) OR usage_known_mask&8!=0 THEN cache_write_tokens END),
        SUM(CASE WHEN status='not_sent' THEN 1 ELSE 0 END),
        SUM(CASE WHEN status='remote_unknown' THEN 1 ELSE 0 END),
        SUM(CASE WHEN attempt_id IS NOT NULL AND dispatched IS NULL THEN 1 ELSE 0 END)
        FROM usage_facts WHERE room_id=?1 AND workspace_id=?2 GROUP BY session_id", facts_sql(connection)?))?;
    let rows=statement.query_map(params![room,workspace],|row|Ok(json!({
        "session_id":row.get::<_,String>(0)?,"requests":row.get::<_,u64>(1)?,"attempts":row.get::<_,u64>(2)?,
        "legacy_records":row.get::<_,u64>(3)?,"partial_usage_attempts":row.get::<_,u64>(4)?,
        "failed_attempts":row.get::<_,u64>(5)?,"pending_attempts":row.get::<_,u64>(6)?,
        "input_tokens":row.get::<_,Option<u64>>(7)?,"output_tokens":row.get::<_,Option<u64>>(8)?,
        "cache_read_tokens":row.get::<_,Option<u64>>(9)?,"cache_write_tokens":row.get::<_,Option<u64>>(10)?,
        "not_sent_attempts":row.get::<_,u64>(11)?,"unknown_outcome_attempts":row.get::<_,u64>(12)?,
        "unknown_dispatch_attempts":row.get::<_,u64>(13)?,
    })))?.collect();
    rows
}

pub(super) fn requests_for_run(connection: &Connection, workspace: &str, room: &str, run: &str, turn: Option<&str>) -> rusqlite::Result<Vec<JsonValue>> {
    let mut statement = connection.prepare(&format!("WITH usage_facts AS ({})
        SELECT attempt_id,call_id,request_kind,status,input_tokens,output_tokens,usage_known_mask,turn_id,
            dispatched,fact_source,protocol_stop,process_drained,created_at
        FROM usage_facts u WHERE workspace_id=?1 AND room_id=?2 AND attempt_id IS NOT NULL
          AND (run_id=?3 OR (fact_source='http' AND run_id IS NULL AND (
            turn_id=?4 OR EXISTS (SELECT 1 FROM tool_calls c WHERE c.tool_call_id=u.call_id AND c.run_id=?3))))
        ORDER BY created_at IS NULL,created_at,attempt_id", facts_sql(connection)?))?;
    let rows = statement.query_map(params![workspace,room,run,turn], |row| {
        let mask = row.get::<_,Option<i64>>(6)?.unwrap_or(0);
        Ok(json!({"attempt_id":row.get::<_,String>(0)?,"call_id":row.get::<_,Option<String>>(1)?,
            "purpose":row.get::<_,Option<String>>(2)?,"status":row.get::<_,String>(3)?,
            "input_tokens":if mask&1!=0 {Some(row.get::<_,i64>(4)?)} else {None},
            "output_tokens":if mask&2!=0 {Some(row.get::<_,i64>(5)?)} else {None},
            "source_turn_id":row.get::<_,Option<String>>(7)?,"dispatched":row.get::<_,Option<bool>>(8)?,
            "source":row.get::<_,String>(9)?,"protocol_stop":row.get::<_,Option<String>>(10)?,
            "process_drained":row.get::<_,Option<bool>>(11)?,"created_at":row.get::<_,Option<i64>>(12)?}))
    })?.collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_usage_rows_remain_unlinked_and_run_migration_is_idempotent() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE chat_usage_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT, workspace_id TEXT NOT NULL,
            room_id TEXT NOT NULL, session_id TEXT NOT NULL, created_at INTEGER NOT NULL,
            input_tokens INTEGER NOT NULL, output_tokens INTEGER NOT NULL,
            cache_read_tokens INTEGER NOT NULL, cache_write_tokens INTEGER NOT NULL,
            turn_id TEXT, call_id TEXT, request_kind TEXT);
            INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,
                input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,turn_id)
            VALUES ('old-workspace','old-room','old-session',1,3,2,0,0,'old-provider-turn');").unwrap();
        chat_insights::ensure_tables(&connection).unwrap();
        chat_insights::ensure_tables(&connection).unwrap();
        let (turn, run): (String, Option<String>) = connection.query_row(
            "SELECT turn_id,run_id FROM chat_usage_events WHERE id=1", [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(turn, "old-provider-turn");
        assert!(run.is_none(), "旧记录没有可靠父运行时不得猜测补齐");
        let rows=summary(&connection,"old-room","old-workspace").unwrap();
        assert_eq!(rows[0]["legacy_records"],1);
        assert_eq!(rows[0]["requests"],0);
        assert_eq!(rows[0]["input_tokens"],3);
        assert!(rows[0]["cache_read_tokens"].is_null());
    }

    fn usage_db() -> Connection {
        let connection=Connection::open_in_memory().unwrap();
        chat_insights::ensure_tables(&connection).unwrap();
        connection.execute_batch("CREATE TABLE devin_acp_attempts(attempt_id TEXT PRIMARY KEY,scope_json TEXT,state TEXT,protocol_stop TEXT,process_drained INTEGER);
            CREATE TABLE tool_calls(tool_call_id TEXT,run_id TEXT);").unwrap();
        connection
    }

    fn acp(connection:&Connection,id:&str,room:&str,state:&str,stop:Option<&str>) {
        let scope=json!({"workspace_id":"w","room_id":room,"agent_id":"s","run_id":format!("run-{id}"),"turn_id":format!("turn-{id}")});
        connection.execute("INSERT INTO devin_acp_attempts VALUES(?1,?2,?3,?4,1)",params![id,scope.to_string(),state,stop]).unwrap();
    }

    #[test]
    fn acp_read_model_deduplicates_and_separates_unknown_dispatch_without_forging_tokens() {
        let connection=usage_db();
        for (id,state,stop) in [("done","terminal",Some("end_turn")),("cancel","terminal",Some("cancelled")),
            ("prepared","prepared",None),("submitted","submitted",None),("not-sent","not_sent",None),
            ("unknown","unknown",Some("end_turn"))] { acp(&connection,id,"r",state,stop); }
        acp(&connection,"foreign","other","terminal",Some("end_turn"));
        connection.execute_batch("INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,
            input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,attempt_id,logical_request_id,status,dispatched,usage_known_mask)
            VALUES('w','r','s',10,7,0,0,0,'acp:done','acp:done','prepared',0,1),
              ('w','r','s',11,999,999,0,0,'acp:foreign','acp:foreign','completed',1,15);").unwrap();
        let rows=summary(&connection,"r","w").unwrap();let row=&rows[0];
        assert_eq!(rows.len(),1);
        for (key,count) in [("requests",6),("attempts",3),("failed_attempts",1),("pending_attempts",2),
            ("not_sent_attempts",1),("unknown_outcome_attempts",1),("unknown_dispatch_attempts",1),("partial_usage_attempts",6)] {
            assert_eq!(row[key],count,"{key}");
        }
        assert_eq!(row["input_tokens"],7);assert!(row["output_tokens"].is_null());
        let trace=requests_for_run(&connection,"w","r","run-done",Some("turn-done")).unwrap();
        assert_eq!(trace.len(),1);assert_eq!(trace[0]["status"],"completed");assert_eq!(trace[0]["source"],"acp");
        let unknown=requests_for_run(&connection,"w","r","run-unknown",None).unwrap();
        assert!(unknown[0]["dispatched"].is_null());assert!(unknown[0]["created_at"].is_null());
        assert_eq!(unknown[0]["status"],"remote_unknown");
        assert!(requests_for_run(&connection,"w","r","another-run",Some("turn-unknown")).unwrap().is_empty(),"ACP不得按turn猜配");
        assert_eq!(summary(&connection,"other","w").unwrap()[0]["input_tokens"],JsonValue::Null,"错误归属投影的999不可带入权威room");
    }

    #[test]
    fn http_retries_and_run_fallback_survive_acp_merge_and_corruption_is_not_zero() {
        let connection=usage_db();
        connection.execute_batch("INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,
            input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,turn_id,logical_request_id,attempt_id,status,dispatched,usage_known_mask)
            VALUES('w','r','http',1,2,0,0,0,'old-turn','request','request:1','completed',1,1),
              ('w','r','http',2,99,99,0,0,'old-turn','request','request:2','remote_unknown',1,NULL);").unwrap();
        let rows=summary(&connection,"r","w").unwrap();
        assert_eq!(rows[0]["requests"],1);assert_eq!(rows[0]["attempts"],2);
        assert_eq!(rows[0]["input_tokens"],2);assert!(rows[0]["output_tokens"].is_null());
        assert_eq!(rows[0]["unknown_outcome_attempts"],1);assert_eq!(rows[0]["failed_attempts"],0);
        assert_eq!(requests_for_run(&connection,"w","r","old-run",Some("old-turn")).unwrap().len(),2);
        connection.execute_batch("INSERT INTO devin_acp_attempts VALUES('broken','{','unknown',NULL,1)").unwrap();
        assert!(summary(&connection,"r","w").is_err(),"损坏台账不能变成虚假零或成功");
    }
}
