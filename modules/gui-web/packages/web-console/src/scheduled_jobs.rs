//! 定时任务单次触发的持久领取；配置仍负责计划，数据库负责已经领取和发生的事实。
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

// 保留定时器调用名，所有入口共享同一工程活动锁。
pub(crate) use crate::workspace_activity::{pin_workspace, begin_workspace_change};

pub(crate) struct JobKey<'a> {
    pub workspace_id: &'a str,
    pub task_id: &'a str,
    pub scheduled_for_ms: u64,
}
pub(crate) enum ClaimResult {
    Acquired(JobClaim),
    Settled { outcome_json: String, started_at_ms: u64 },
    Blocked(String),
}
pub(crate) struct JobClaim {
    path: PathBuf,
    workspace_id: String,
    task_id: String,
    scheduled_for_ms: i64,
    token: String,
    finished: bool,
}

/// 列表只读投影的领取事实。它不表示进程当前仍存活。
pub(crate) struct CurrentClaim {
    pub state: String,
    pub fingerprint_matches: bool,
    pub has_outcome: bool,
}

pub(crate) struct CurrentClaimLookup {
    pub task_id: String,
    pub scheduled_for_ms: u64,
    pub fingerprint: String,
}

/// 一次只读连接、一次精确键查询；超过 SQLite 默认参数上限时拒绝投影而不猜状态。
pub(crate) fn read_current_claims(
    path: &Path,
    workspace_id: &str,
    lookups: &[CurrentClaimLookup],
) -> Result<Vec<Option<CurrentClaim>>, String> {
    if lookups.is_empty() { return Ok(Vec::new()); }
    if lookups.len() > 400 {
        return Err("当前到期任务过多，执行状态暂不可用；请分批核查".into());
    }
    match std::fs::metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((0..lookups.len()).map(|_| None).collect());
        }
        Err(error) => return Err(format!("定时任务执行库不可读取：{error}")),
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("定时任务执行库只读打开失败：{error}"))?;
    connection.busy_timeout(std::time::Duration::from_millis(250))
        .map_err(|error| error.to_string())?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if version > 1 {
        return Err("定时任务执行库来自较新版本，领取状态暂不可用".into());
    }
    let mut sql = String::from("SELECT task_id,scheduled_for_ms,fingerprint,state,outcome_json \
        FROM scheduled_job_attempts WHERE workspace_id=?1 AND (");
    let mut values = vec![rusqlite::types::Value::Text(workspace_id.to_string())];
    let mut scheduled_for = Vec::with_capacity(lookups.len());
    for (index, lookup) in lookups.iter().enumerate() {
        if index > 0 { sql.push_str(" OR "); }
        sql.push_str(&format!("(task_id=?{} AND scheduled_for_ms=?{})", 2 * index + 2, 2 * index + 3));
        let instant = i64::try_from(lookup.scheduled_for_ms)
            .map_err(|_| "定时时刻超出范围".to_string())?;
        values.push(rusqlite::types::Value::Text(lookup.task_id.clone()));
        values.push(rusqlite::types::Value::Integer(instant));
        scheduled_for.push(instant);
    }
    sql.push(')');
    let mut statement = connection.prepare(&sql)
        .map_err(|error| format!("定时任务领取状态查询失败：{error}"))?;
    let rows = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?, row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?.is_some()))
    }).map_err(|error| format!("定时任务领取状态查询失败：{error}"))?;
    let mut found = HashMap::new();
    for row in rows {
        let (task_id, instant, fingerprint, state, has_outcome) = row
            .map_err(|error| format!("定时任务领取状态读取失败：{error}"))?;
        found.insert((task_id, instant), (fingerprint, state, has_outcome));
    }
    Ok(lookups.iter().zip(scheduled_for).map(|(lookup, instant)| {
        found.get(&(lookup.task_id.clone(), instant)).map(|(prior_fingerprint, state, has_outcome)| CurrentClaim {
            state: state.clone(), fingerprint_matches: prior_fingerprint == &lookup.fingerprint, has_outcome: *has_outcome,
        })
    }).collect())
}
fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(|e| e.to_string())?;
    if version > 1 { return Err("定时任务执行库来自较新版本，未执行任务".into()); }
    connection.execute_batch("CREATE TABLE IF NOT EXISTS scheduled_job_attempts (
        workspace_id TEXT NOT NULL, task_id TEXT NOT NULL, scheduled_for_ms INTEGER NOT NULL,
        fingerprint TEXT NOT NULL, owner_token TEXT NOT NULL, state TEXT NOT NULL,
        started_at_ms INTEGER NOT NULL, outcome_json TEXT,
        PRIMARY KEY(workspace_id, task_id, scheduled_for_ms)); PRAGMA user_version=1;")
        .map_err(|e| e.to_string())?;
    Ok(connection)
}

pub(crate) fn claim(path: &Path, key: JobKey<'_>, fingerprint: &str, now_ms: u64) -> Result<ClaimResult, String> {
    if key.workspace_id.is_empty() || key.task_id.is_empty() || fingerprint.is_empty() {
        return Err("定时任务领取缺少真实工程、任务或版本".into());
    }
    let scheduled_for_ms = i64::try_from(key.scheduled_for_ms).map_err(|_| "定时时刻超出范围")?;
    let started_at_ms = i64::try_from(now_ms).map_err(|_| "领取时刻超出范围")?;
    let mut connection = open(path)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
    let previous: Option<(String, String, i64, Option<String>)> = transaction.query_row(
        "SELECT fingerprint, state, started_at_ms, outcome_json FROM scheduled_job_attempts WHERE workspace_id=?1 AND task_id=?2 AND scheduled_for_ms=?3",
        params![key.workspace_id, key.task_id, scheduled_for_ms], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional().map_err(|e| e.to_string())?;
    if let Some((prior_fingerprint, state, started, outcome)) = previous {
        return Ok(if prior_fingerprint != fingerprint {
            ClaimResult::Blocked("同一触发时刻已领取另一版任务；请核对原执行后重新设置触发时刻".into())
        } else if state == "settled" {
            match outcome {
                Some(outcome_json) => ClaimResult::Settled { outcome_json, started_at_ms: started.max(0) as u64 },
                None => ClaimResult::Blocked("任务已结账但缺少结果，未重复执行".into()),
            }
        } else {
            ClaimResult::Blocked(format!("本次触发已领取（{state}）；执行可能仍在运行或已中断，未重复调用模型或工具"))
        });
    }
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let token = random.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    transaction.execute("INSERT INTO scheduled_job_attempts(workspace_id,task_id,scheduled_for_ms,fingerprint,owner_token,state,started_at_ms) VALUES(?1,?2,?3,?4,?5,'running',?6)",
        params![key.workspace_id,key.task_id,scheduled_for_ms,fingerprint,token,started_at_ms]).map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(ClaimResult::Acquired(JobClaim { path: path.to_owned(), workspace_id: key.workspace_id.into(),
        task_id: key.task_id.into(), scheduled_for_ms, token, finished: false }))
}

impl JobClaim {
    pub(crate) fn finish(mut self, outcome_json: &str) -> Result<(), String> {
        serde_json::from_str::<serde_json::Value>(outcome_json).map_err(|e| e.to_string())?;
        let changed = open(&self.path)?.execute("UPDATE scheduled_job_attempts SET state='settled',outcome_json=?1 WHERE workspace_id=?2 AND task_id=?3 AND scheduled_for_ms=?4 AND owner_token=?5 AND state='running'",
            params![outcome_json,self.workspace_id,self.task_id,self.scheduled_for_ms,self.token]).map_err(|e| e.to_string())?;
        if changed != 1 { return Err("定时任务执行所有权已变化，结果未覆盖原记录".into()); }
        self.finished = true;
        Ok(())
    }
}
impl Drop for JobClaim {
    fn drop(&mut self) {
        if self.finished { return; }
        // 取消/异常不是“没执行过”；留未知并阻止隐式重放。进程崩溃则保留 running，同样阻止重放。
        if let Ok(connection) = open(&self.path) {
            let _ = connection.execute("UPDATE scheduled_job_attempts SET state='outcome_unknown' WHERE workspace_id=?1 AND task_id=?2 AND scheduled_for_ms=?3 AND owner_token=?4 AND state='running'",
                params![self.workspace_id,self.task_id,self.scheduled_for_ms,self.token]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key() -> JobKey<'static> { JobKey { workspace_id: "ws-one", task_id: "task-one", scheduled_for_ms: 100 } }
    #[test]
    fn competing_connections_claim_once_and_restart_reuses_only_durable_result() {
        let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("jobs.sqlite3");
        let ClaimResult::Acquired(first) = claim(&path, key(), "original", 100).unwrap() else { panic!(); };
        assert!(matches!(claim(&path, key(), "original", 101).unwrap(), ClaimResult::Blocked(_)));
        assert!(matches!(claim(&path, key(), "changed", 101).unwrap(), ClaimResult::Blocked(_)));
        first.finish(r#"{"executed":true}"#).unwrap();
        assert!(matches!(claim(&path, key(), "original", 102).unwrap(), ClaimResult::Settled { started_at_ms:100, .. }));
        let ClaimResult::Acquired(other) = claim(&path, JobKey { workspace_id:"ws-two", ..key() }, "original", 103).unwrap() else { panic!(); };
        drop(other);
        assert!(matches!(claim(&path, JobKey { workspace_id:"ws-two", ..key() }, "original", 104).unwrap(), ClaimResult::Blocked(_)));
        assert!(claim(&path, JobKey { workspace_id:"ws-two", scheduled_for_ms:101, ..key() }, "original", 105).is_ok());
    }
    #[test]
    fn list_projection_reads_only_the_exact_occurrence_without_initializing_storage() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("jobs.sqlite3");
        let lookup = |instant, fingerprint: &str| CurrentClaimLookup {
            task_id: "task-one".into(), scheduled_for_ms: instant, fingerprint: fingerprint.into(),
        };
        assert!(read_current_claims(&path, "ws-one", &[lookup(100, "original")]).unwrap()[0].is_none());
        assert!(!path.exists());
        let ClaimResult::Acquired(first) = claim(&path, key(), "original", 100).unwrap() else { panic!(); };
        let mut current = read_current_claims(&path, "ws-one", &[
            lookup(100, "original"), lookup(101, "original"), lookup(100, "changed"),
        ]).unwrap();
        assert!(current[1].is_none());
        assert!(!current[2].as_ref().unwrap().fingerprint_matches);
        let current = current.remove(0).unwrap();
        assert_eq!(current.state, "running");
        assert!(current.fingerprint_matches);
        assert!(!current.has_outcome);
        first.finish(r#"{"executed":true}"#).unwrap();
        let settled = read_current_claims(&path, "ws-one", &[lookup(100, "original")]).unwrap().remove(0).unwrap();
        assert_eq!(settled.state, "settled");
        assert!(settled.has_outcome);
    }
    #[test]
    fn workspace_cannot_change_between_acceptance_and_result_projection() {
        let _config_guard = crate::tests::config_test_guard();
        let pin = pin_workspace().unwrap();
        assert!(begin_workspace_change().is_err());
        drop(pin);
        let change = begin_workspace_change().unwrap();
        assert!(pin_workspace().is_err());
        drop(change);
        assert!(pin_workspace().is_ok());
    }
}
