//! 阶段身份与run.accepted同事务保存；提交不能根据后来改变的角色/路由重解释旧结果。
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StagePin {
    pub(super) original_role: String,
    pub(super) route_hint: Option<String>,
    pub(super) retry_count: u32,
    pub(super) last_verdict: Option<String>,
    pub(super) target_session_id: String,
}

impl StagePin {
    pub(super) fn stage(&self) -> super::goal_phase_stage::PhaseStage {
        super::goal_phase_stage::PhaseStage::resolve(&self.original_role, self.route_hint.as_deref(),
            self.retry_count, self.last_verdict.as_deref() == Some("fail"))
    }

    pub(super) fn capture(phase: &super::GoalPhaseDto, target_session_id: &str) -> Self {
        Self {
            original_role: phase.assigned_role.clone(),
            route_hint: phase.route_hint.clone(),
            retry_count: phase.retry_count,
            last_verdict: phase.last_verdict.clone(),
            target_session_id: target_session_id.to_string(),
        }
    }

    pub(super) fn matches(&self, phase: &super::GoalPhaseDto, target_session_id: &str) -> bool {
        self == &Self::capture(phase, target_session_id)
    }

    pub(super) fn matches_stored(&self, connection: &Connection, goal: &str, phase: &str,
        target_session_id: &str) -> rusqlite::Result<bool> {
        let current = connection.query_row(
            "SELECT assigned_role,route_hint,retry_count,last_verdict FROM goal_phases WHERE goal_id=?1 AND id=?2",
            params![goal, phase], |row| Ok(Self {
                original_role: row.get(0)?, route_hint: row.get(1)?,
                retry_count: row.get(2)?, last_verdict: row.get(3)?,
                target_session_id: target_session_id.to_string(),
            }))?;
        Ok(self == &current)
    }

    pub(super) fn load(connection: &Connection, run_id: &str) -> rusqlite::Result<Self> {
        // 只读取接纳事件，最多两条用于检出重复，不扫描整个运行轨迹。
        let mut statement = connection.prepare(
            "SELECT payload_json FROM runtime_run_events WHERE run_id=?1 AND event_type='run.accepted' ORDER BY id LIMIT 2",
        )?;
        let payloads = statement.query_map(params![run_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if payloads.len() != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let payload: serde_json::Value = serde_json::from_str(&payloads[0])
            .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
        let pin = payload.get("goal_phase_pin").cloned()
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        serde_json::from_value(pin).map_err(|_| rusqlite::Error::QueryReturnedNoRows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_stage_requires_every_frozen_field_and_valid_retry_budget() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE goal_phases(goal_id TEXT,id TEXT,assigned_role TEXT,route_hint TEXT,retry_count INTEGER,last_verdict TEXT);
            INSERT INTO goal_phases VALUES('goal','phase','verifier','implementer',2,'fail')").unwrap();
        let pin = StagePin { original_role: "verifier".into(), route_hint: Some("implementer".into()),
            retry_count: 2, last_verdict: Some("fail".into()), target_session_id: "repair-session".into() };
        assert!(pin.matches_stored(&connection,"goal","phase","repair-session").unwrap());
        assert!(!pin.matches_stored(&connection,"goal","phase","other-session").unwrap());
        assert!(pin.matches_stored(&connection,"other-goal","phase","repair-session").is_err());
        for mutation in ["assigned_role='implementer'", "route_hint='planner'", "retry_count=3", "last_verdict='pass'"] {
            // 每次回滚，确保每一个字段独立变化就足以失去原资格。
            connection.execute_batch("BEGIN").unwrap();
            connection.execute(&format!("UPDATE goal_phases SET {mutation}"),[]).unwrap();
            assert!(!pin.matches_stored(&connection,"goal","phase","repair-session").unwrap(),"{mutation}");
            connection.execute_batch("ROLLBACK").unwrap();
        }
        for retry in [-1_i64, i64::from(u32::MAX)+1] {
            connection.execute("UPDATE goal_phases SET retry_count=?1",params![retry]).unwrap();
            assert!(pin.matches_stored(&connection,"goal","phase","repair-session").is_err(),
                "非法持久预算不得截断或饱和后取得执行资格");
        }
    }

    #[test]
    fn missing_duplicate_or_corrupted_stage_identity_is_rejected() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE runtime_run_events(id INTEGER PRIMARY KEY,run_id TEXT,event_type TEXT,payload_json TEXT)").unwrap();
        let pin = StagePin { original_role:"implementer".into(), route_hint:Some("planner".into()),
            retry_count:1,last_verdict:Some("fail".into()),target_session_id:"shared-session".into() };
        assert!(StagePin::load(&connection,"run").is_err());
        let payload = serde_json::json!({"goal_phase_pin":pin}).to_string();
        connection.execute("INSERT INTO runtime_run_events VALUES(1,'run','run.accepted',?1)",params![payload]).unwrap();
        assert_eq!(StagePin::load(&connection,"run").unwrap(), pin);
        connection.execute("INSERT INTO runtime_run_events VALUES(2,'run','run.accepted','{}')",[]).unwrap();
        assert!(StagePin::load(&connection,"run").is_err());
        connection.execute("DELETE FROM runtime_run_events WHERE id=2",[]).unwrap();
        connection.execute("UPDATE runtime_run_events SET payload_json='{}'",[]).unwrap();
        assert!(StagePin::load(&connection,"run").is_err(),"历史缺少阶段身份不得默认拥有执行资格");
        connection.execute("UPDATE runtime_run_events SET payload_json='broken json'",[]).unwrap();
        assert!(StagePin::load(&connection,"run").is_err());
    }
}
