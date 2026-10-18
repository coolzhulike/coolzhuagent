//! 用量统计只读取供应商事实。模型、首 token 与协议口径在请求发生时保存。
use super::*;

#[derive(Default, Deserialize)]
pub(super) struct QueryOptions {
    room_id: Option<String>,
    hours: Option<u32>,
}

pub(super) async fn get(Query(query): Query<QueryOptions>) -> ApiResult<Json<JsonValue>> {
    let path = default_session_sqlite_path();
    let workspace = workspace_identity(&active_workspace_path());
    let room = query.room_id.filter(|id| !id.is_empty());
    let since = query.hours.filter(|hours| *hours > 0).map(|hours| {
        unix_timestamp_millis().saturating_sub(u64::from(hours.min(24 * 366)) * 3_600_000)
    });
    let result = tokio::task::spawn_blocking(move || -> rusqlite::Result<JsonValue> {
        let connection = open_session_connection(&path)?;
        chat_insights::ensure_tables(&connection)?;
        aggregate(&connection, &workspace, room.as_deref(), since)
    })
    .await
    .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?
    .map_err(sqlite_api_error)?;
    Ok(Json(result))
}

pub(super) fn aggregate(
    connection: &Connection,
    workspace: &str,
    room: Option<&str>,
    since: Option<u64>,
) -> rusqlite::Result<JsonValue> {
    let facts = request_usage::facts_sql(connection)?;
    let prefix=format!("WITH facts AS ({facts}), measured AS (
      SELECT f.*,u.model_snapshot,u.protocol_snapshot,u.started_at,u.first_token_at,u.finished_at,
        CASE WHEN f.usage_known_mask&3=3 AND u.protocol_snapshot='openai_chat_completions' THEN f.input_tokens+f.output_tokens
             WHEN f.usage_known_mask=15 AND u.protocol_snapshot='anthropic_messages' THEN f.input_tokens+f.output_tokens+f.cache_read_tokens+f.cache_write_tokens END AS total,
        CASE WHEN f.status='completed' AND f.usage_known_mask&2=2 AND u.first_token_at IS NOT NULL AND u.finished_at>u.first_token_at THEN u.finished_at-u.first_token_at END AS decode_ms
      FROM facts f LEFT JOIN chat_usage_events u ON u.attempt_id=f.attempt_id
      WHERE f.workspace_id=?1 AND (?2 IS NULL OR f.room_id=?2) AND (?3 IS NULL OR f.created_at>=?3)) ");
    let fields="COUNT(*),SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),
      SUM(CASE WHEN status IN ('failed','http_error','network_error','timeout','protocol_error','configuration_error') THEN 1 ELSE 0 END),
      COUNT(total),SUM(total),SUM(CASE WHEN usage_known_mask&1!=0 THEN input_tokens END),
      SUM(CASE WHEN usage_known_mask&2!=0 THEN output_tokens END),SUM(CASE WHEN usage_known_mask&4!=0 THEN cache_read_tokens END),
      SUM(CASE WHEN usage_known_mask&8!=0 THEN cache_write_tokens END),SUM(decode_ms),
      SUM(CASE WHEN decode_ms IS NOT NULL THEN output_tokens END),
      AVG(CASE WHEN first_token_at>=started_at THEN first_token_at-started_at END),COUNT(decode_ms)";
    let mut groups = BTreeMap::new();
    for (label,key) in [("summary","'全部'"),("sessions","session_id"),("models","COALESCE(model_snapshot,'历史模型未记录')"),("series","CASE WHEN created_at IS NULL THEN '时间未记录' ELSE strftime('%Y-%m-%d %H:00',created_at/1000,'unixepoch') END")] {
        let sql=format!("{prefix} SELECT {key},{fields} FROM measured GROUP BY {key} ORDER BY {key}");
        let mut statement=connection.prepare(&sql)?;
        let rows=statement.query_map(params![workspace,room,since],|row| {
            let count:i64=row.get(1)?; let known:i64=row.get(4)?;
            let decode:Option<i64>=row.get(10)?; let output:Option<i64>=row.get(11)?;
            Ok(json!({"key":row.get::<_,String>(0)?,"requests":count,"completed":row.get::<_,Option<i64>>(2)?,"failed":row.get::<_,Option<i64>>(3)?,
                "known_requests":known,"unknown_requests":count-known,"known_total_tokens":row.get::<_,Option<i64>>(5)?,
                "total_tokens":if known==count {row.get::<_,Option<i64>>(5)?} else {None},
                "input_tokens":row.get::<_,Option<i64>>(6)?,"output_tokens":row.get::<_,Option<i64>>(7)?,
                "cache_read_tokens":row.get::<_,Option<i64>>(8)?,"cache_write_tokens":row.get::<_,Option<i64>>(9)?,
                "decode_ms":decode,"decode_tokens":output,"ttft_ms":row.get::<_,Option<f64>>(12)?,"timed_requests":row.get::<_,i64>(13)?,
                "tokens_per_second":decode.zip(output).filter(|(ms,_)| *ms>0).map(|(ms,n)| n as f64*1000.0/ms as f64)}))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        groups.insert(label,rows);
    }
    let mut media = Vec::new();
    let has_media: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='media_usage_events')",
        [],
        |r| r.get(0),
    )?;
    if has_media {
        let mut statement=connection.prepare("SELECT kind,COUNT(*),SUM(units),SUM(duration_seconds),SUM(cost),COUNT(cost) FROM media_usage_events WHERE (?1 IS NULL OR created_at>=?1) GROUP BY kind")?;
        media=statement.query_map([since],|r|Ok(json!({"kind":r.get::<_,String>(0)?,"jobs":r.get::<_,i64>(1)?,"units":r.get::<_,i64>(2)?,"duration_seconds":r.get::<_,Option<i64>>(3)?,"reported_cost":r.get::<_,Option<f64>>(4)?,"known_cost_jobs":r.get::<_,i64>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(
        json!({"media":media,"media_scope":"workspace","workspace_id":workspace,"room_id":room,"since":since,"summary":groups["summary"].first(),
        "sessions":groups["sessions"],"models":groups["models"],"series":groups["series"],
        "note":"来自供应商返回与订阅请求台账；未知不等于零。总量按协议消除缓存重复计数。速率仅用已完成且有首 token 和输出 token 的请求：Σ输出 token / Σ解码秒数；未计入工具、排队时间。历史缺失的模型和时间不推测，订阅余额不折算本地费用。"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_tokens_are_not_added_twice_and_rate_uses_decode_time() {
        let c = Connection::open_in_memory().unwrap();
        chat_insights::ensure_tables(&c).unwrap();
        c.execute_batch("INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,attempt_id,status,usage_known_mask,protocol_snapshot,model_snapshot,started_at,first_token_at,finished_at)
          VALUES('w','r','s',1000,100,40,60,0,'a','completed',15,'openai_chat_completions','old-model',1000,4000,6000);").unwrap();
        let result = aggregate(&c, "w", Some("r"), None).unwrap();
        assert_eq!(result["summary"]["total_tokens"], 140);
        assert_eq!(result["summary"]["tokens_per_second"], 20.0);
        assert_eq!(result["summary"]["ttft_ms"], 3000.0);
        assert_eq!(result["models"][0]["key"], "old-model");
        assert!(aggregate(&c, "other", Some("r"), None).unwrap()["summary"].is_null());
    }
    #[test]
    fn missing_usage_and_nonstream_timings_are_not_zero_rate() {
        let c = Connection::open_in_memory().unwrap();
        chat_insights::ensure_tables(&c).unwrap();
        c.execute_batch("INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,attempt_id,status,usage_known_mask) VALUES('w','r','s',1000,0,0,0,0,'a','completed',0)").unwrap();
        let result = aggregate(&c, "w", Some("r"), None).unwrap();
        assert!(result["summary"]["total_tokens"].is_null());
        assert!(result["summary"]["tokens_per_second"].is_null());
        assert_eq!(result["summary"]["unknown_requests"], 1);
    }
}
