//! 适配器请求事实投影：仅写 chat_usage_events，不另设或累加第二份账本。
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

pub(super) fn summary(connection:&Connection,room:&str,workspace:&str)->rusqlite::Result<Vec<JsonValue>> {
    let mut statement=connection.prepare("SELECT session_id,COUNT(DISTINCT logical_request_id),
        SUM(CASE WHEN dispatched=1 THEN 1 ELSE 0 END),SUM(CASE WHEN attempt_id IS NULL THEN 1 ELSE 0 END),
        SUM(CASE WHEN attempt_id IS NOT NULL AND usage_known_mask!=15 THEN 1 ELSE 0 END),
        SUM(CASE WHEN status NOT IN ('legacy','prepared','dispatched','completed') THEN 1 ELSE 0 END),
        SUM(CASE WHEN status IN ('prepared','dispatched') THEN 1 ELSE 0 END),
        CASE WHEN SUM(CASE WHEN (attempt_id IS NULL AND input_tokens>0) OR usage_known_mask&1!=0 THEN 1 ELSE 0 END)>0 THEN SUM(input_tokens) END,
        CASE WHEN SUM(CASE WHEN (attempt_id IS NULL AND output_tokens>0) OR usage_known_mask&2!=0 THEN 1 ELSE 0 END)>0 THEN SUM(output_tokens) END,
        CASE WHEN SUM(CASE WHEN (attempt_id IS NULL AND cache_read_tokens>0) OR usage_known_mask&4!=0 THEN 1 ELSE 0 END)>0 THEN SUM(cache_read_tokens) END,
        CASE WHEN SUM(CASE WHEN (attempt_id IS NULL AND cache_write_tokens>0) OR usage_known_mask&8!=0 THEN 1 ELSE 0 END)>0 THEN SUM(cache_write_tokens) END
        FROM chat_usage_events WHERE room_id=?1 AND workspace_id=?2 GROUP BY session_id")?;
    let rows=statement.query_map(params![room,workspace],|row|Ok(json!({
        "session_id":row.get::<_,String>(0)?,"requests":row.get::<_,u64>(1)?,"attempts":row.get::<_,u64>(2)?,
        "legacy_records":row.get::<_,u64>(3)?,"partial_usage_attempts":row.get::<_,u64>(4)?,
        "failed_attempts":row.get::<_,u64>(5)?,"pending_attempts":row.get::<_,u64>(6)?,
        "input_tokens":row.get::<_,Option<u64>>(7)?,"output_tokens":row.get::<_,Option<u64>>(8)?,
        "cache_read_tokens":row.get::<_,Option<u64>>(9)?,"cache_write_tokens":row.get::<_,Option<u64>>(10)?,
    })))?.collect();
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
    }
}
