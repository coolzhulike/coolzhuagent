use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use computer_use::{
    ComputerUseResult, ComputerUseRunState, ComputerUseSurface, ComputerUseTerminalStatus,
};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn apply_session_migration_v11(connection: &Connection) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS computer_use_runs (
            call_id TEXT PRIMARY KEY,
            provider_tool_call_id TEXT,
            turn_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            chat_room_id TEXT,
            idempotency_key TEXT NOT NULL,
            objective_json TEXT NOT NULL,
            surface TEXT NOT NULL,
            state TEXT NOT NULL,
            state_version INTEGER NOT NULL DEFAULT 0,
            risk_class TEXT NOT NULL DEFAULT 'observe',
            approval_state TEXT NOT NULL DEFAULT 'not_required',
            approval_deadline_ms INTEGER,
            action_count INTEGER NOT NULL DEFAULT 0,
            replan_count INTEGER NOT NULL DEFAULT 0,
            no_progress_count INTEGER NOT NULL DEFAULT 0,
            current_observation_generation INTEGER NOT NULL DEFAULT 0,
            deadline_ms INTEGER NOT NULL,
            terminal_result_json TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_session_turn
            ON computer_use_runs(session_id, turn_id);
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_turn_idempotency
            ON computer_use_runs(session_id, turn_id, idempotency_key, updated_at_ms);
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_state
            ON computer_use_runs(state);
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_updated
            ON computer_use_runs(updated_at_ms);

        CREATE TABLE IF NOT EXISTS computer_use_steps (
            run_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            observation_generation INTEGER NOT NULL,
            action_type TEXT NOT NULL,
            normalized_target TEXT NOT NULL,
            action_fingerprint TEXT NOT NULL,
            status TEXT NOT NULL,
            error_code TEXT,
            before_evidence_ref TEXT,
            after_evidence_ref TEXT,
            visible_progress INTEGER NOT NULL DEFAULT 0,
            started_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            PRIMARY KEY (run_id, step_index),
            FOREIGN KEY (run_id) REFERENCES computer_use_runs(call_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_steps_run
            ON computer_use_steps(run_id, step_index);
        CREATE INDEX IF NOT EXISTS idx_computer_use_steps_status
            ON computer_use_steps(status);

        CREATE TABLE IF NOT EXISTS computer_use_step_details (
            run_id TEXT NOT NULL, step_index INTEGER NOT NULL, action_json TEXT NOT NULL,
            PRIMARY KEY(run_id, step_index),
            FOREIGN KEY(run_id) REFERENCES computer_use_runs(call_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS computer_use_planner_diagnostics (
            id INTEGER PRIMARY KEY AUTOINCREMENT, call_id TEXT NOT NULL,
            turn_id TEXT NOT NULL, room_id TEXT, session_id TEXT NOT NULL,
            request_kind TEXT NOT NULL, observation_generation INTEGER NOT NULL,
            model TEXT NOT NULL, provider_response_id TEXT,
            response_json TEXT NOT NULL, error_code TEXT,
            started_at_ms INTEGER NOT NULL, completed_at_ms INTEGER NOT NULL,
            FOREIGN KEY(call_id) REFERENCES computer_use_runs(call_id) ON DELETE CASCADE
        );
        "#,
    )?;
    if current < 11 {
        connection.execute_batch("PRAGMA user_version = 11;")?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewComputerUseRun {
    pub call_id: String,
    pub provider_tool_call_id: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    pub chat_room_id: Option<String>,
    pub idempotency_key: String,
    pub objective_json: String,
    pub surface: ComputerUseSurface,
    pub deadline_ms: u64,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredComputerUseRun {
    pub call_id: String,
    pub provider_tool_call_id: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    pub chat_room_id: Option<String>,
    pub state: ComputerUseRunState,
    pub state_version: u64,
    pub surface: ComputerUseSurface,
    pub terminal_result: Option<ComputerUseResult>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComputerUseStepRecord {
    pub run_id: String,
    pub step_index: usize,
    pub observation_generation: u64,
    pub action_type: String,
    pub normalized_target: String,
    pub action_fingerprint: String,
    pub status: String,
    pub error_code: Option<String>,
    pub before_evidence_ref: Option<String>,
    pub after_evidence_ref: Option<String>,
    pub visible_progress: bool,
    pub started_at_ms: u64,
    pub completed_at_ms: Option<u64>,
}

pub(crate) struct PlannerDiagnostic<'a> {
    pub call_id: &'a str, pub turn_id: &'a str, pub room_id: Option<&'a str>,
    pub session_id: &'a str, pub request_kind: &'a str, pub observation_generation: u64,
    pub model: &'a str, pub provider_response_id: Option<&'a str>,
    pub response_json: &'a str, pub error_code: Option<&'a str>,
    pub started_at_ms: u64, pub completed_at_ms: u64,
}

/// 诊断保留动作结构和几何，不记录输入正文、URL、未知字符串或模型思考。
pub(crate) fn sanitized_action_json(raw: &str) -> String {
    use serde_json::{json, Value};
    fn clean(value: &Value, key: &str) -> Value {
        match value {
            Value::Object(object) => {
                let allowed = ["done", "action", "kind", "target", "arguments", "points", "duration_ms",
                    "x", "y", "button", "keys", "direction", "amount", "drop_target", "value", "checked", "activate", "tab_id"];
                let mut result = serde_json::Map::new();
                for (name, value) in object {
                    if allowed.contains(&name.as_str()) { result.insert(name.clone(), clean(value, name)); }
                    else { result.insert("redacted_fields".into(), Value::Bool(true)); }
                }
                Value::Object(result)
            }
            Value::Array(values) => Value::Array(values.iter().take(256).map(|value| clean(value, key)).collect()),
            Value::String(text) => {
                let safe = match key {
                    "kind" | "action" => ["click","double_click","text_input","scroll","key_combination","drag","slider_drag","navigate","select","check","submit","history_back","history_forward","open_tab","close_tab","activate_tab"].contains(&text.as_str()),
                    "target" | "drop_target" => text.len() <= 128 && (
                        ((text.starts_with("uia-") || text.starts_with("dom-") || text == "browser-tabs") && text.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
                        || text.strip_prefix("window-canvas:").is_some_and(|handle| !handle.is_empty() && handle.chars().all(|ch| ch.is_ascii_hexdigit()))
                    ),
                    "direction" => ["up","down","left","right"].contains(&text.as_str()),
                    "button" => ["left","right"].contains(&text.as_str()),
                    "keys" => ["ctrl","shift","alt","enter","escape","tab","home","end","a","c","l","v","x","y","z"].contains(&text.as_str()),
                    "tab_id" => text.len() <= 32 && text.chars().all(|ch| ch.is_ascii_digit()),
                    _ => false,
                };
                if safe { value.clone() } else { Value::String("[redacted]".into()) }
            }
            _ => value.clone(),
        }
    }
    if raw.len() > 16 * 1024 { return json!({"omitted":"response_too_large","bytes":raw.len()}).to_string(); }
    let result = serde_json::from_str::<Value>(raw).map(|value| clean(&value, "")).unwrap_or_else(|_| json!({"omitted":"invalid_json","bytes":raw.len()})).to_string();
    if result.len() > 16 * 1024 { json!({"omitted":"sanitized_response_too_large","bytes":raw.len()}).to_string() } else { result }
}

pub(crate) struct ComputerUseRunStore {
    connection: Mutex<Connection>,
}

impl ComputerUseRunStore {
    pub(crate) fn from_connection(connection: Connection) -> Self {
        Self {
            connection: Mutex::new(connection),
        }
    }

    pub(crate) fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
        )?;
        apply_session_migration_v11(&connection)?;
        Ok(Self::from_connection(connection))
    }

    pub(crate) fn create_run(&self, run: &NewComputerUseRun) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let changed = connection.execute(
            r#"
            INSERT OR IGNORE INTO computer_use_runs(
                call_id, provider_tool_call_id, turn_id, session_id, chat_room_id,
                idempotency_key, objective_json, surface, state, state_version,
                deadline_ms, created_at_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'requested', 0, ?9, ?10, ?10)
            "#,
            params![
                run.call_id,
                run.provider_tool_call_id,
                run.turn_id,
                run.session_id,
                run.chat_room_id,
                run.idempotency_key,
                run.objective_json,
                surface_name(run.surface),
                run.deadline_ms,
                run.created_at_ms,
            ],
        )?;
        Ok(changed == 1)
    }

    pub(crate) fn transition(
        &self,
        call_id: &str,
        expected_version: u64,
        state: ComputerUseRunState,
        updated_at_ms: u64,
    ) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let changed = connection.execute(
            r#"
            UPDATE computer_use_runs
            SET state = ?1, state_version = state_version + 1, updated_at_ms = ?2
            WHERE call_id = ?3 AND state_version = ?4 AND terminal_result_json IS NULL
            "#,
            params![
                run_state_name(state),
                updated_at_ms,
                call_id,
                expected_version
            ],
        )?;
        Ok(changed == 1)
    }

    pub(crate) fn finish(
        &self,
        call_id: &str,
        expected_version: u64,
        result: &ComputerUseResult,
    ) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let result_json = serde_json::to_string(result)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let changed = connection.execute(
            r#"
            UPDATE computer_use_runs
            SET state = ?1, state_version = state_version + 1,
                terminal_result_json = ?2, updated_at_ms = ?3
            WHERE call_id = ?4 AND state_version = ?5 AND terminal_result_json IS NULL
            "#,
            params![
                run_state_name(terminal_run_state(result.status)),
                result_json,
                now_ms(),
                call_id,
                expected_version,
            ],
        )?;
        Ok(changed == 1)
    }

    pub(crate) fn append_step(&self, step: &ComputerUseStepRecord) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let changed = connection.execute(
            r#"
            INSERT OR IGNORE INTO computer_use_steps(
                run_id, step_index, observation_generation, action_type, normalized_target,
                action_fingerprint, status, error_code, before_evidence_ref, after_evidence_ref,
                visible_progress, started_at_ms, completed_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                step.run_id,
                step.step_index,
                step.observation_generation,
                step.action_type,
                step.normalized_target,
                step.action_fingerprint,
                step.status,
                step.error_code,
                step.before_evidence_ref,
                step.after_evidence_ref,
                step.visible_progress,
                step.started_at_ms,
                step.completed_at_ms,
            ],
        )?;
        Ok(changed == 1)
    }

    /// 实际输入前写 executing，输入/观察后更新同一步；不事后伪造动作与时间。
    pub(crate) fn record_step(&self, step: &ComputerUseStepRecord, action_json: &str) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("computer-use store lock");
        let transaction = connection.transaction()?;
        transaction.execute("INSERT INTO computer_use_steps
            (run_id,step_index,observation_generation,action_type,normalized_target,action_fingerprint,status,error_code,before_evidence_ref,after_evidence_ref,visible_progress,started_at_ms,completed_at_ms)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
            ON CONFLICT(run_id,step_index) DO UPDATE SET status=excluded.status,error_code=excluded.error_code,
            after_evidence_ref=excluded.after_evidence_ref,visible_progress=excluded.visible_progress,completed_at_ms=excluded.completed_at_ms",
            params![step.run_id,step.step_index,step.observation_generation,step.action_type,step.normalized_target,
                step.action_fingerprint,step.status,step.error_code,step.before_evidence_ref,step.after_evidence_ref,
                step.visible_progress,step.started_at_ms,step.completed_at_ms])?;
        transaction.execute("INSERT OR IGNORE INTO computer_use_step_details(run_id,step_index,action_json) VALUES(?1,?2,?3)",
            params![step.run_id,step.step_index,action_json])?;
        transaction.commit()
    }

    pub(crate) fn record_planner_diagnostic(&self, value: &PlannerDiagnostic<'_>) -> rusqlite::Result<()> {
        self.connection.lock().expect("computer-use store lock").execute(
            "INSERT INTO computer_use_planner_diagnostics(call_id,turn_id,room_id,session_id,request_kind,observation_generation,
             model,provider_response_id,response_json,error_code,started_at_ms,completed_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![value.call_id,value.turn_id,value.room_id,value.session_id,value.request_kind,value.observation_generation,
                value.model,value.provider_response_id,value.response_json,value.error_code,value.started_at_ms,value.completed_at_ms])?;
        Ok(())
    }

    pub(crate) fn record_step_verification(&self, call_id: &str, before_generation: u64, verification: &computer_use::Verification) -> rusqlite::Result<()> {
        self.connection.lock().expect("computer-use store lock").execute(
            "UPDATE computer_use_steps SET visible_progress=?1,status=?2,after_evidence_ref=?3
             WHERE run_id=?4 AND step_index=(SELECT MAX(step_index) FROM computer_use_steps WHERE run_id=?4 AND observation_generation=?5)
             AND status='input_sent_observed'",
            params![verification.visible_progress,if verification.achieved {"verified"} else {"completed_unverified"},
                serde_json::to_string(&verification.evidence).unwrap_or_default(),call_id,before_generation])?;
        Ok(())
    }

    pub(crate) fn action_counts(&self, call_id: &str) -> rusqlite::Result<(usize, usize)> {
        self.connection.lock().expect("computer-use store lock").query_row(
            "SELECT COUNT(*),COALESCE(SUM(CASE WHEN status IN ('input_sent','input_sent_observed','verified','completed_unverified','observation_failed') THEN 1 ELSE 0 END),0) FROM computer_use_steps WHERE run_id=?1",
            [call_id], |row| Ok((row.get(0)?,row.get(1)?)))
    }

    pub(crate) fn load(&self, call_id: &str) -> rusqlite::Result<Option<StoredComputerUseRun>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        connection
            .query_row(
                r#"
                SELECT call_id, provider_tool_call_id, session_id, turn_id, chat_room_id, state,
                       state_version, surface, terminal_result_json, updated_at_ms
                FROM computer_use_runs WHERE call_id = ?1
                "#,
                [call_id],
                |row| {
                    let state: String = row.get(5)?;
                    let surface: String = row.get(7)?;
                    let terminal_json: Option<String> = row.get(8)?;
                    Ok(StoredComputerUseRun {
                        call_id: row.get(0)?,
                        provider_tool_call_id: row.get(1)?,
                        session_id: row.get(2)?,
                        turn_id: row.get(3)?,
                        chat_room_id: row.get(4)?,
                        state: parse_run_state(&state)?,
                        state_version: row.get(6)?,
                        surface: parse_surface(&surface)?,
                        terminal_result: terminal_json
                            .map(|json| parse_json_column(&json))
                            .transpose()?,
                        updated_at_ms: row.get(9)?,
                    })
                },
            )
            .optional()
    }

    pub(crate) fn load_by_idempotency_key(
        &self,
        session_id: &str,
        turn_id: &str,
        idempotency_key: &str,
    ) -> rusqlite::Result<Option<StoredComputerUseRun>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        connection
            .query_row(
                r#"
                SELECT call_id, provider_tool_call_id, session_id, turn_id, chat_room_id, state,
                       state_version, surface, terminal_result_json, updated_at_ms
                FROM computer_use_runs
                WHERE session_id = ?1 AND turn_id = ?2 AND idempotency_key = ?3
                ORDER BY updated_at_ms DESC, created_at_ms DESC
                LIMIT 1
                "#,
                params![session_id, turn_id, idempotency_key],
                |row| {
                    let state: String = row.get(5)?;
                    let surface: String = row.get(7)?;
                    let terminal_json: Option<String> = row.get(8)?;
                    Ok(StoredComputerUseRun {
                        call_id: row.get(0)?,
                        provider_tool_call_id: row.get(1)?,
                        session_id: row.get(2)?,
                        turn_id: row.get(3)?,
                        chat_room_id: row.get(4)?,
                        state: parse_run_state(&state)?,
                        state_version: row.get(6)?,
                        surface: parse_surface(&surface)?,
                        terminal_result: terminal_json
                            .map(|json| parse_json_column(&json))
                            .transpose()?,
                        updated_at_ms: row.get(9)?,
                    })
                },
            )
            .optional()
    }

    pub(crate) fn count_runs_for_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> rusqlite::Result<usize> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM computer_use_runs WHERE session_id = ?1 AND turn_id = ?2",
            params![session_id, turn_id],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as usize)
    }

    /// 只有确定在参数校验阶段、尚无输入的拒绝才使用独立纠错额度。
    /// 观察/规划/执行失败和运行中的调用仍消耗原执行额度，不能藉改参数无限重入。
    pub(crate) fn turn_budget_counts(&self, session_id: &str, turn_id: &str) -> rusqlite::Result<(usize, usize)> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement = connection.prepare("SELECT terminal_result_json FROM computer_use_runs WHERE session_id=?1 AND turn_id=?2")?;
        let rows = statement.query_map(params![session_id,turn_id], |row| row.get::<_,Option<String>>(0))?;
        let (mut execution,mut invalid) = (0,0);
        for row in rows {
            let result: Option<ComputerUseResult> = row?.map(|value| parse_json_column(&value)).transpose()?;
            let is_invalid = result.as_ref().is_some_and(|result| result.stage == computer_use::ComputerUseStage::IntentGuard
                && result.attempts == 0 && result.steps_completed == 0
                && result.error.as_ref().is_some_and(|error| matches!(error.code.as_str(), "invalid_tool_input"|"invalid_objective"|"invalid_success_criteria"|"input_correction_budget_exhausted")));
            if is_invalid { invalid += 1; } else { execution += 1; }
        }
        Ok((execution,invalid))
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn terminal_run_state(status: ComputerUseTerminalStatus) -> ComputerUseRunState {
    match status {
        ComputerUseTerminalStatus::Succeeded => ComputerUseRunState::Succeeded,
        ComputerUseTerminalStatus::Failed => ComputerUseRunState::Failed,
        ComputerUseTerminalStatus::Blocked => ComputerUseRunState::Blocked,
        ComputerUseTerminalStatus::Cancelled => ComputerUseRunState::Cancelled,
        ComputerUseTerminalStatus::TimedOut => ComputerUseRunState::TimedOut,
    }
}

fn run_state_name(state: ComputerUseRunState) -> &'static str {
    match state {
        ComputerUseRunState::Requested => "requested",
        ComputerUseRunState::Classified => "classified",
        ComputerUseRunState::Observing => "observing",
        ComputerUseRunState::Planning => "planning",
        ComputerUseRunState::PolicyCheck => "policy_check",
        ComputerUseRunState::AwaitingApproval => "awaiting_approval",
        ComputerUseRunState::Executing => "executing",
        ComputerUseRunState::Verifying => "verifying",
        ComputerUseRunState::Succeeded => "succeeded",
        ComputerUseRunState::Failed => "failed",
        ComputerUseRunState::Blocked => "blocked",
        ComputerUseRunState::Cancelled => "cancelled",
        ComputerUseRunState::TimedOut => "timed_out",
    }
}

fn surface_name(surface: ComputerUseSurface) -> &'static str {
    surface.as_str()
}

fn parse_run_state(value: &str) -> rusqlite::Result<ComputerUseRunState> {
    parse_json_column(&format!("\"{value}\""))
}

fn parse_surface(value: &str) -> rusqlite::Result<ComputerUseSurface> {
    parse_json_column(&format!("\"{value}\""))
}

fn parse_json_column<T: serde::de::DeserializeOwned>(value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

#[cfg(test)]
mod tests {
    use computer_use::{
        ComputerUseError, ComputerUseResult, ComputerUseRetryOwner, ComputerUseStage,
        ComputerUseSurface, ComputerUseTerminalStatus, SupervisorSnapshot,
    };
    use rusqlite::Connection;
    use serde::Deserialize;

    use super::*;
    use crate::ConfigComputerUse;

    fn failed_result(code: &str) -> ComputerUseResult {
        ComputerUseResult {
            call_id: "cu-1".into(),
            provider_tool_call_id: Some("toolu-1".into()),
            status: ComputerUseTerminalStatus::Failed,
            stage: ComputerUseStage::Verification,
            goal_achieved: false,
            surface: ComputerUseSurface::Browser,
            summary: format!("failed: {code}"),
            error: Some(ComputerUseError::blocked(
                code,
                "failed",
                ComputerUseRetryOwner::None,
            )),
            attempts: 1,
            steps_completed: 1,
            evidence: vec!["after.png".into()],
            supervisor: SupervisorSnapshot::default(),
        }
    }

    fn run() -> NewComputerUseRun {
        NewComputerUseRun {
            call_id: "cu-1".into(),
            provider_tool_call_id: Some("toolu-1".into()),
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
            chat_room_id: Some("room-1".into()),
            idempotency_key: "stable-key".into(),
            objective_json: r#"{"objective":"submit"}"#.into(),
            surface: ComputerUseSurface::Browser,
            deadline_ms: 121_000,
            created_at_ms: 1_000,
        }
    }

    fn temp_store() -> ComputerUseRunStore {
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();
        ComputerUseRunStore::from_connection(connection)
    }

    #[test]
    fn diagnostic_redaction_keeps_invalid_action_shape_without_credentials_images_or_text() {
        let raw=r#"{"done":false,"action":"click","api_key":"SECRET","reasoning":"FULL-THOUGHT","image":{"data_url":"data:image/png;base64,IMAGE"},"arguments":{"text":"PRIVATE-TYPING","points":[[0,0],[1,1]]}}"#;
        let redacted=sanitized_action_json(raw);
        assert!(redacted.contains("\"action\":\"click\""));
        for secret in ["SECRET","FULL-THOUGHT","IMAGE","PRIVATE-TYPING","data:image"] { assert!(!redacted.contains(secret)); }
        assert!(serde_json::from_str::<serde_json::Value>(&redacted).is_ok());
        let invalid=sanitized_action_json("not-json PASSWORD");
        assert!(!invalid.contains("PASSWORD"));
        assert!(sanitized_action_json(&"x".repeat(20_000)).len()<100);
        assert!(sanitized_action_json(r#"{"target":"window-canvas:1234"}"#).contains("window-canvas:1234"));
        assert!(sanitized_action_json(r#"{"target":"window-canvas:abc123"}"#).contains("window-canvas:abc123"));
        assert!(!sanitized_action_json(r#"{"target":"window-canvas:SECRET"}"#).contains("SECRET"));
    }

    #[test]
    fn config_defaults_match_the_controller_safety_contract() {
        let config = ConfigComputerUse::default();
        let budgets = config.budgets();

        assert!(config.enabled);
        assert_eq!(config.tool_mode, "task-controller");
        assert_eq!(config.approval_ttl_seconds, 180);
        assert_eq!(config.evidence_retention_days, 7);
        assert_eq!(budgets.max_actions, 12);
        assert_eq!(budgets.max_replans, 2);
        assert_eq!(budgets.max_same_signature, 2);
        assert_eq!(budgets.max_no_progress_steps, 2);
        assert_eq!(budgets.timeout_ms, 120_000);
        assert_eq!(budgets.max_calls_per_turn, 2);
        assert!(config.desktop.require_window_identity);
        assert!(config.desktop.block_webview2_surface_conflict);
        assert!(config.browser.allow_drag);
        assert!(config.browser.allow_key_combinations);
        assert!(config.browser.allow_multiple_tabs);
    }

    #[test]
    fn toml_overrides_are_loaded_and_unsafe_limits_are_clamped() {
        #[derive(Deserialize)]
        struct Wrapper {
            computer_use: ConfigComputerUse,
        }

        let wrapper: Wrapper = toml::from_str(
            r#"
                [computer_use]
                enabled = false
                tool_mode = "shadow"
                approval_ttl_seconds = 30
                evidence_retention_days = 2

                [computer_use.controller]
                max_actions = 0
                max_replans = 99
                max_same_signature = 0
                max_no_progress_steps = 99
                timeout_seconds = 9999
                max_calls_per_turn = 99

                [computer_use.desktop]
                enabled = false
                require_window_identity = false
                block_webview2_surface_conflict = false

                [computer_use.browser]
                enabled = true
                allow_drag = true
                allow_key_combinations = true
                allow_multiple_tabs = true
            "#,
        )
        .unwrap();
        let config = wrapper.computer_use;
        let budgets = config.budgets();

        assert!(!config.enabled);
        assert_eq!(config.tool_mode, "shadow");
        assert_eq!(config.approval_ttl_seconds, 30);
        assert_eq!(config.evidence_retention_days, 2);
        assert_eq!(budgets.max_actions, 1);
        assert_eq!(budgets.max_replans, 5);
        assert_eq!(budgets.max_same_signature, 1);
        assert_eq!(budgets.max_no_progress_steps, 3);
        assert_eq!(budgets.timeout_ms, 300_000);
        assert_eq!(budgets.max_calls_per_turn, 2);
        assert!(!config.desktop.enabled);
        assert!(config.browser.allow_drag);
    }

    #[test]
    fn migration_v11_creates_run_step_tables_and_indexes() {
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 11);
        for table in ["computer_use_runs", "computer_use_steps"] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing table {table}");
        }
        for index in [
            "idx_computer_use_runs_session_turn",
            "idx_computer_use_runs_turn_idempotency",
            "idx_computer_use_runs_state",
            "idx_computer_use_runs_updated",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing index {index}");
        }
    }

    #[test]
    fn session_schema_initialization_applies_migration_v11() {
        let connection = Connection::open_in_memory().unwrap();
        crate::initialize_session_schema(&connection).unwrap();

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 11);
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='computer_use_runs')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn transition_rejects_an_outdated_state_version() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());

        assert!(store
            .transition(
                "cu-1",
                0,
                computer_use::ComputerUseRunState::Observing,
                1_001
            )
            .unwrap());
        assert!(!store
            .transition(
                "cu-1",
                0,
                computer_use::ComputerUseRunState::Planning,
                1_002
            )
            .unwrap());

        let stored = store.load("cu-1").unwrap().unwrap();
        assert_eq!(stored.state, computer_use::ComputerUseRunState::Observing);
        assert_eq!(stored.state_version, 1);
        assert_eq!(stored.chat_room_id.as_deref(), Some("room-1"));
    }

    #[test]
    fn turn_run_count_supports_recursive_call_watchdog() {
        let store = temp_store();
        assert_eq!(store.count_runs_for_turn("session-1", "turn-1").unwrap(), 0);
        assert!(store.create_run(&run()).unwrap());
        assert_eq!(store.count_runs_for_turn("session-1", "turn-1").unwrap(), 1);
        assert_eq!(
            store
                .count_runs_for_turn("session-1", "turn-other")
                .unwrap(),
            0
        );
    }

    #[test]
    fn load_by_idempotency_key_returns_existing_terminal_run_for_same_turn_task() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());
        assert!(store
            .finish("cu-1", 0, &failed_result("target_not_found"))
            .unwrap());

        let existing = store
            .load_by_idempotency_key("session-1", "turn-1", "stable-key")
            .unwrap()
            .expect("same task should be found by idempotency key");

        assert_eq!(existing.call_id, "cu-1");
        assert_eq!(
            existing.terminal_result.unwrap().error.unwrap().code,
            "target_not_found"
        );
    }

    #[test]
    fn terminal_result_is_written_once() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());
        assert!(store.finish("cu-1", 0, &failed_result("x")).unwrap());
        assert!(!store.finish("cu-1", 0, &failed_result("y")).unwrap());

        let stored = store.load("cu-1").unwrap().unwrap();
        assert_eq!(stored.state_version, 1);
        assert_eq!(stored.state, computer_use::ComputerUseRunState::Failed);
        assert_eq!(stored.terminal_result.unwrap().error.unwrap().code, "x");
    }
}
