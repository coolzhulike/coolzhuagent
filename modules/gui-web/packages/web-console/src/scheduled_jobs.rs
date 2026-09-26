//! 定时任务单次触发的持久领取；配置仍负责计划，数据库负责已经领取和发生的事实。
use std::path::{Path, PathBuf};
use rusqlite::{params, Connection, OptionalExtension};

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
