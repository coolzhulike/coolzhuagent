//! Fusion Team 会话库持久化：schema v31 迁移与全部 CRUD。
//! 设计见 docs/plans/2026-11-09-fusion-team-design.md §9；只追加不删除，与 Goal 表无耦合。
use super::super::*;
use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use std::path::Path;

pub(crate) fn apply_session_migration_v31(connection: &Connection) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS fusion_teams (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 0,
            settings_json TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        );
        CREATE TABLE IF NOT EXISTS fusion_team_members (
            team_id TEXT NOT NULL,
            role TEXT NOT NULL,
            session_id TEXT,
            settings_json TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (team_id, role)
        );
        CREATE TABLE IF NOT EXISTS fusion_tasks (
            id TEXT PRIMARY KEY,
            team_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            workspace_id TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'task',
            status TEXT NOT NULL,
            title TEXT,
            originating_message_id TEXT,
            classification TEXT,
            plan_json TEXT NOT NULL DEFAULT '[]',
            budget_json TEXT NOT NULL DEFAULT '{}',
            usage_json TEXT NOT NULL DEFAULT '{}',
            mandate_json TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            finished_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_fusion_tasks_room ON fusion_tasks(room_id, team_id, status);
        CREATE TABLE IF NOT EXISTS fusion_work_items (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            title TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            depends_on_json TEXT NOT NULL DEFAULT '[]',
            acceptance_json TEXT NOT NULL DEFAULT '[]',
            verification_json TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 2,
            last_verdict TEXT,
            last_reason TEXT,
            last_evidence_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_fusion_work_items_task ON fusion_work_items(task_id, status);
        CREATE TABLE IF NOT EXISTS fusion_briefs (
            id TEXT PRIMARY KEY,
            task_id TEXT,
            work_item_id TEXT,
            room_id TEXT NOT NULL,
            team_id TEXT NOT NULL,
            from_role TEXT NOT NULL,
            to_role TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'queued',
            run_id TEXT,
            result_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            finished_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_fusion_briefs_room ON fusion_briefs(room_id, team_id, status);
        CREATE TABLE IF NOT EXISTS fusion_role_messages (
            room_id TEXT NOT NULL,
            team_id TEXT NOT NULL,
            role TEXT NOT NULL,
            seq INTEGER NOT NULL,
            message_json TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (room_id, team_id, role, seq)
        );
        CREATE TABLE IF NOT EXISTS fusion_decisions (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            question TEXT NOT NULL,
            options_json TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'pending',
            answer_json TEXT,
            created_at INTEGER NOT NULL,
            answered_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_fusion_decisions_task ON fusion_decisions(task_id, status);
        CREATE TABLE IF NOT EXISTS fusion_artifacts (
            id TEXT PRIMARY KEY,
            task_id TEXT,
            kind TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        "#,
    )?;
    if current < 31 {
        connection.execute_batch("PRAGMA user_version = 31;")?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(super) struct FusionTeamRow {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub settings_json: String,
    pub host_session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub querier_session_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub(super) struct FusionTaskRow {
    pub id: String,
    pub team_id: String,
    pub room_id: String,
    pub status: String,
    pub title: Option<String>,
    pub classification: Option<String>,
    pub plan_json: String,
}

#[derive(Debug, Clone)]
pub(super) struct FusionWorkItemRow {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub depends_on_json: String,
    pub acceptance_json: String,
    pub verification_json: Option<String>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub last_verdict: Option<String>,
    pub last_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct FusionBriefRow {
    pub id: String,
    pub task_id: Option<String>,
    pub work_item_id: Option<String>,
    pub room_id: String,
    pub team_id: String,
    pub from_role: String,
    pub to_role: String,
    pub kind: String,
    pub payload_json: String,
    pub status: String,
    pub result_json: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct FusionRoleMessage {
    pub role: String,
    pub content: String,
    pub kind: String,
}

fn connection_for(path: &Path) -> ApiResult<session_db::PooledConnection> {
    let connection = open_session_connection(path).map_err(sqlite_api_error)?;
    initialize_session_schema(&connection).map_err(sqlite_api_error)?;
    Ok(connection)
}

fn team_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FusionTeamRow> {
    Ok(FusionTeamRow {
        id: row.get(0)?,
        name: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
        settings_json: row.get(3)?,
        host_session_id: row.get(4)?,
        executor_session_id: row.get(5)?,
        querier_session_id: row.get(6)?,
        created_at: row.get::<_, i64>(7)?.max(0) as u64,
        updated_at: row.get::<_, i64>(8)?.max(0) as u64,
    })
}

const TEAM_SELECT: &str = "SELECT t.id, t.name, t.enabled, t.settings_json,
    (SELECT session_id FROM fusion_team_members WHERE team_id=t.id AND role='host'),
    (SELECT session_id FROM fusion_team_members WHERE team_id=t.id AND role='executor'),
    (SELECT session_id FROM fusion_team_members WHERE team_id=t.id AND role='querier'),
    t.created_at, t.updated_at FROM fusion_teams t WHERE t.deleted_at IS NULL";

pub(super) fn fusion_teams_list(path: &Path) -> ApiResult<Vec<FusionTeamRow>> {
    let connection = connection_for(path)?;
    let mut statement = connection
        .prepare(&format!("{TEAM_SELECT} ORDER BY t.created_at, t.id"))
        .map_err(sqlite_api_error)?;
    let rows = statement
        .query_map([], team_from_row)
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    Ok(rows)
}

pub(super) fn fusion_team_get(path: &Path, team_id: &str) -> ApiResult<Option<FusionTeamRow>> {
    let connection = connection_for(path)?;
    connection
        .query_row(&format!("{TEAM_SELECT} AND t.id=?1"), params![team_id], team_from_row)
        .optional()
        .map_err(sqlite_api_error)
}

pub(super) fn fusion_team_upsert(
    path: &Path,
    team_id: &str,
    name: &str,
    enabled: bool,
    settings_json: &str,
    host_session_id: &str,
    executor_session_id: &str,
    querier_session_id: Option<&str>,
) -> ApiResult<()> {
    let mut connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sqlite_api_error)?;
    tx.execute(
        "INSERT INTO fusion_teams(id,name,enabled,settings_json,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5,?5)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, enabled=excluded.enabled,
             settings_json=excluded.settings_json, updated_at=excluded.updated_at",
        params![team_id, name, i64::from(enabled), settings_json, now],
    )
    .map_err(sqlite_api_error)?;
    tx.execute(
        "DELETE FROM fusion_team_members WHERE team_id=?1",
        params![team_id],
    )
    .map_err(sqlite_api_error)?;
    for (role, session_id) in [
        ("host", Some(host_session_id)),
        ("executor", Some(executor_session_id)),
        ("querier", querier_session_id),
    ] {
        tx.execute(
            "INSERT INTO fusion_team_members(team_id,role,session_id) VALUES(?1,?2,?3)",
            params![team_id, role, session_id],
        )
        .map_err(sqlite_api_error)?;
    }
    tx.commit().map_err(sqlite_api_error)?;
    Ok(())
}

pub(super) fn fusion_team_delete(path: &Path, team_id: &str) -> ApiResult<bool> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let changed = connection
        .execute(
            "UPDATE fusion_teams SET deleted_at=?2, enabled=0, updated_at=?2 WHERE id=?1 AND deleted_at IS NULL",
            params![team_id, now],
        )
        .map_err(sqlite_api_error)?;
    Ok(changed > 0)
}

pub(super) fn fusion_task_insert(
    path: &Path,
    task_id: &str,
    team_id: &str,
    room_id: &str,
    workspace_id: &str,
    title: Option<&str>,
    originating_message_id: Option<&str>,
    mandate_json: &str,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    connection
        .execute(
            "INSERT INTO fusion_tasks(id,team_id,room_id,workspace_id,kind,status,title,
                 originating_message_id,mandate_json,created_at,updated_at)
             VALUES(?1,?2,?3,?4,'task','planning',?5,?6,?7,?8,?8)",
            params![task_id, team_id, room_id, workspace_id, title, originating_message_id, mandate_json, now],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

pub(super) fn fusion_task_get(path: &Path, task_id: &str) -> ApiResult<Option<FusionTaskRow>> {
    let connection = connection_for(path)?;
    connection
        .query_row(
            "SELECT id,team_id,room_id,status,title,classification,plan_json FROM fusion_tasks WHERE id=?1",
            params![task_id],
            |row| {
                Ok(FusionTaskRow {
                    id: row.get(0)?,
                    team_id: row.get(1)?,
                    room_id: row.get(2)?,
                    status: row.get(3)?,
                    title: row.get(4)?,
                    classification: row.get(5)?,
                    plan_json: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(sqlite_api_error)
}

pub(super) fn fusion_task_update(
    path: &Path,
    task_id: &str,
    status: &str,
    plan_json: Option<&str>,
    classification: Option<&str>,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let finished = matches!(status, "done" | "blocked" | "cancelled" | "failed");
    connection
        .execute(
            "UPDATE fusion_tasks SET status=?2,
                 plan_json=COALESCE(?4, plan_json),
                 classification=COALESCE(?5, classification),
                 updated_at=?3,
                 finished_at=CASE WHEN ?6 THEN COALESCE(finished_at, ?3) ELSE finished_at END
             WHERE id=?1",
            params![task_id, status, now, plan_json, classification, i64::from(finished)],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

/// 最近一次仍处于打开状态的任务（供 brief 归属与 amend 识别）；没有则返回 None。
pub(super) fn fusion_task_latest_open(
    path: &Path,
    room_id: &str,
    team_id: &str,
) -> ApiResult<Option<FusionTaskRow>> {
    let connection = connection_for(path)?;
    connection
        .query_row(
            "SELECT id,team_id,room_id,status,title,classification,plan_json FROM fusion_tasks
             WHERE room_id=?1 AND team_id=?2 AND finished_at IS NULL
             ORDER BY created_at DESC, id DESC LIMIT 1",
            params![room_id, team_id],
            |row| {
                Ok(FusionTaskRow {
                    id: row.get(0)?,
                    team_id: row.get(1)?,
                    room_id: row.get(2)?,
                    status: row.get(3)?,
                    title: row.get(4)?,
                    classification: row.get(5)?,
                    plan_json: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(sqlite_api_error)
}

pub(super) fn fusion_work_items_replace(
    path: &Path,
    task_id: &str,
    items: &[JsonValue],
) -> ApiResult<Vec<String>> {
    let mut connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sqlite_api_error)?;
    tx.execute(
        "DELETE FROM fusion_work_items WHERE task_id=?1 AND status IN ('pending','blocked')",
        params![task_id],
    )
    .map_err(sqlite_api_error)?;
    let mut ids = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("wi-{}", index + 1));
        let title = item
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .take(200)
            .collect::<String>();
        if title.is_empty() {
            continue;
        }
        let depends_on = item
            .get("depends_on")
            .cloned()
            .unwrap_or_else(|| json!([]))
            .to_string();
        let acceptance = item
            .get("acceptance_criteria")
            .cloned()
            .unwrap_or_else(|| json!([]))
            .to_string();
        let verification = item.get("verification").map(|value| value.to_string());
        let max_retries = item
            .get("max_retries")
            .and_then(JsonValue::as_i64)
            .unwrap_or(2)
            .clamp(0, 10);
        tx.execute(
            "INSERT INTO fusion_work_items(id,task_id,title,status,depends_on_json,acceptance_json,
                 verification_json,max_retries,created_at,updated_at)
             VALUES(?1,?2,?3,'pending',?4,?5,?6,?7,?8,?8)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, depends_on_json=excluded.depends_on_json,
                 acceptance_json=excluded.acceptance_json, verification_json=excluded.verification_json,
                 max_retries=excluded.max_retries, updated_at=excluded.updated_at",
            params![id, task_id, title, depends_on, acceptance, verification, max_retries, now],
        )
        .map_err(sqlite_api_error)?;
        ids.push(id);
    }
    tx.commit().map_err(sqlite_api_error)?;
    Ok(ids)
}

pub(super) fn fusion_work_items_for_task(path: &Path, task_id: &str) -> ApiResult<Vec<FusionWorkItemRow>> {
    let connection = connection_for(path)?;
    let mut statement = connection
        .prepare(
            "SELECT id,task_id,title,status,depends_on_json,acceptance_json,verification_json,retry_count,max_retries,
                 last_verdict,last_reason FROM fusion_work_items WHERE task_id=?1 ORDER BY created_at, id",
        )
        .map_err(sqlite_api_error)?;
    let rows = statement
        .query_map(params![task_id], |row| {
            Ok(FusionWorkItemRow {
                id: row.get(0)?,
                task_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                depends_on_json: row.get(4)?,
                acceptance_json: row.get(5)?,
                verification_json: row.get(6)?,
                retry_count: row.get(7)?,
                max_retries: row.get(8)?,
                last_verdict: row.get(9)?,
                last_reason: row.get(10)?,
            })
        })
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    Ok(rows)
}

/// 返回 (新重试次数, 是否超出上限)。
pub(super) fn fusion_work_item_verdict(
    path: &Path,
    work_item_id: &str,
    verdict: &str,
    reason: Option<&str>,
    evidence_json: Option<&str>,
) -> ApiResult<Option<(i64, bool)>> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let next_status = match verdict {
        "accept" => "done",
        _ => "rework",
    };
    let changed = connection
        .execute(
            "UPDATE fusion_work_items SET status=?2, last_verdict=?2, last_reason=?3,
                 last_evidence_json=?4, retry_count=CASE WHEN ?2='rework' THEN retry_count+1 ELSE retry_count END,
                 updated_at=?5 WHERE id=?1",
            params![work_item_id, next_status, reason, evidence_json, now],
        )
        .map_err(sqlite_api_error)?;
    if changed == 0 {
        return Ok(None);
    }
    let (retry_count, max_retries): (i64, i64) = connection
        .query_row(
            "SELECT retry_count, max_retries FROM fusion_work_items WHERE id=?1",
            params![work_item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sqlite_api_error)?;
    let exceeded = next_status == "rework" && retry_count > max_retries;
    if exceeded {
        connection
            .execute(
                "UPDATE fusion_work_items SET status='blocked', updated_at=?2 WHERE id=?1",
                params![work_item_id, now],
            )
            .map_err(sqlite_api_error)?;
    }
    Ok(Some((retry_count, exceeded)))
}

pub(super) fn fusion_work_item_set_status(
    path: &Path,
    work_item_id: &str,
    status: &str,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    connection
        .execute(
            "UPDATE fusion_work_items SET status=?2, updated_at=?3 WHERE id=?1",
            params![work_item_id, status, u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

pub(super) fn fusion_brief_insert(
    path: &Path,
    brief: &FusionBriefRow,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    connection
        .execute(
            "INSERT INTO fusion_briefs(id,task_id,work_item_id,room_id,team_id,from_role,to_role,kind,
                 payload_json,status,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'queued',?10,?10)",
            params![
                brief.id, brief.task_id, brief.work_item_id, brief.room_id, brief.team_id,
                brief.from_role, brief.to_role, brief.kind, brief.payload_json, now
            ],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

pub(super) fn fusion_brief_get(path: &Path, brief_id: &str) -> ApiResult<Option<FusionBriefRow>> {
    let connection = connection_for(path)?;
    connection
        .query_row(
            "SELECT id,task_id,work_item_id,room_id,team_id,from_role,to_role,kind,payload_json,status,result_json
             FROM fusion_briefs WHERE id=?1",
            params![brief_id],
            brief_from_row,
        )
        .optional()
        .map_err(sqlite_api_error)
}

fn brief_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FusionBriefRow> {
    Ok(FusionBriefRow {
        id: row.get(0)?,
        task_id: row.get(1)?,
        work_item_id: row.get(2)?,
        room_id: row.get(3)?,
        team_id: row.get(4)?,
        from_role: row.get(5)?,
        to_role: row.get(6)?,
        kind: row.get(7)?,
        payload_json: row.get(8)?,
        status: row.get(9)?,
        result_json: row.get(10)?,
    })
}

pub(super) fn fusion_briefs_queued(
    path: &Path,
    room_id: &str,
    team_id: &str,
) -> ApiResult<Vec<FusionBriefRow>> {
    let connection = connection_for(path)?;
    let mut statement = connection
        .prepare(
            "SELECT id,task_id,work_item_id,room_id,team_id,from_role,to_role,kind,payload_json,status,result_json
             FROM fusion_briefs WHERE room_id=?1 AND team_id=?2 AND status='queued'
             ORDER BY created_at, id",
        )
        .map_err(sqlite_api_error)?;
    let rows = statement
        .query_map(params![room_id, team_id], brief_from_row)
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    Ok(rows)
}

/// 按状态取本房间本团队的 brief（重启恢复事件源）。
pub(super) fn fusion_briefs_by_status(
    path: &Path,
    room_id: &str,
    team_id: &str,
    status: &str,
) -> ApiResult<Vec<FusionBriefRow>> {
    let connection = connection_for(path)?;
    let mut statement = connection
        .prepare(
            "SELECT id,task_id,work_item_id,room_id,team_id,from_role,to_role,kind,payload_json,status,result_json
             FROM fusion_briefs WHERE room_id=?1 AND team_id=?2 AND status=?3
             ORDER BY created_at, id",
        )
        .map_err(sqlite_api_error)?;
    let rows = statement
        .query_map(params![room_id, team_id, status], brief_from_row)
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    Ok(rows)
}

/// 上次进程遗留的 running brief 收敛为 interrupted（不重放，由主持重新决定）。
pub(super) fn fusion_briefs_sweep_interrupted(path: &Path) -> ApiResult<usize> {
    let connection = connection_for(path)?;
    let changed = connection
        .execute(
            "UPDATE fusion_briefs SET status='interrupted', updated_at=?1, finished_at=?1 WHERE status='running'",
            params![u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(changed)
}

/// brief 开始执行：queued→running 的 CAS；失败返回 false（已被取消或并发抢占）。
pub(super) fn fusion_brief_claim(path: &Path, brief_id: &str, run_id: &str) -> ApiResult<bool> {
    let connection = connection_for(path)?;
    let changed = connection
        .execute(
            "UPDATE fusion_briefs SET status='running', run_id=?2, updated_at=?3
             WHERE id=?1 AND status='queued'",
            params![brief_id, run_id, u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(changed > 0)
}

pub(super) fn fusion_brief_finish(
    path: &Path,
    brief_id: &str,
    status: &str,
    result_json: &str,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    connection
        .execute(
            "UPDATE fusion_briefs SET status=?2, result_json=?3, updated_at=?4, finished_at=?4 WHERE id=?1",
            params![brief_id, status, result_json, now],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

/// 执行中 brief 被停止（取消/中断）：保持 running→cancelled 收敛。
pub(super) fn fusion_brief_cancel_running(path: &Path, room_id: &str, team_id: &str) -> ApiResult<usize> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let changed = connection
        .execute(
            "UPDATE fusion_briefs SET status='cancelled', updated_at=?3, finished_at=?3
             WHERE room_id=?1 AND team_id=?2 AND status IN ('queued','running')",
            params![room_id, team_id, now],
        )
        .map_err(sqlite_api_error)?;
    Ok(changed)
}

pub(super) fn fusion_role_message_append(
    path: &Path,
    room_id: &str,
    team_id: &str,
    role: &str,
    message: &FusionRoleMessage,
) -> ApiResult<i64> {
    let connection = connection_for(path)?;
    let now = u64_to_i64(unix_timestamp_millis());
    let seq: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(seq),0)+1 FROM fusion_role_messages WHERE room_id=?1 AND team_id=?2 AND role=?3",
            params![room_id, team_id, role],
            |row| row.get(0),
        )
        .map_err(sqlite_api_error)?;
    connection
        .execute(
            "INSERT INTO fusion_role_messages(room_id,team_id,role,seq,message_json,created_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![room_id, team_id, role, seq, serde_json::to_string(message).unwrap_or_else(|_| "{}".into()), now],
        )
        .map_err(sqlite_api_error)?;
    Ok(seq)
}

/// 角色线程回放：按 seq 还原为模型历史（role=user/assistant 的持久消息）。
pub(super) fn fusion_role_messages_list(
    path: &Path,
    room_id: &str,
    team_id: &str,
    role: &str,
) -> ApiResult<Vec<PersistedChatMessage>> {
    let connection = connection_for(path)?;
    let mut statement = connection
        .prepare(
            "SELECT message_json, created_at FROM fusion_role_messages
             WHERE room_id=?1 AND team_id=?2 AND role=?3 ORDER BY seq",
        )
        .map_err(sqlite_api_error)?;
    let rows = statement
        .query_map(params![room_id, team_id, role], |row| {
            let encoded: String = row.get(0)?;
            let created_at: i64 = row.get(1)?;
            Ok((encoded, created_at))
        })
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    let mut messages = Vec::with_capacity(rows.len());
    for (encoded, created_at) in rows {
        let parsed = serde_json::from_str::<FusionRoleMessage>(&encoded).unwrap_or(FusionRoleMessage {
            role: "user".to_string(),
            content: encoded.clone(),
            kind: "text".to_string(),
        });
        messages.push(PersistedChatMessage {
            id: String::new(),
            author: String::new(),
            role: parsed.role,
            target: String::new(),
            content: parsed.content,
            kind: parsed.kind,
            attachments: Vec::new(),
            created_at: created_at.max(0) as u64,
        });
    }
    Ok(messages)
}

pub(super) fn fusion_decision_insert(
    path: &Path,
    decision_id: &str,
    task_id: &str,
    question: &str,
    options_json: &str,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    connection
        .execute(
            "INSERT INTO fusion_decisions(id,task_id,question,options_json,status,created_at)
             VALUES(?1,?2,?3,?4,'pending',?5)",
            params![decision_id, task_id, question, options_json, u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

pub(super) fn fusion_decisions_pending(path: &Path, task_id: &str) -> ApiResult<Vec<(String, String)>> {
    let connection = connection_for(path)?;
    let mut statement = connection
        .prepare("SELECT id, question FROM fusion_decisions WHERE task_id=?1 AND status='pending' ORDER BY created_at")
        .map_err(sqlite_api_error)?;
    let rows = statement
        .query_map(params![task_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    Ok(rows)
}

#[derive(Debug, Clone)]
pub(super) struct FusionDecisionRow {
    pub id: String,
    pub task_id: String,
    pub question: String,
    pub options_json: String,
    pub status: String,
    pub answer_json: Option<String>,
}

pub(super) fn fusion_decision_get(path: &Path, decision_id: &str) -> ApiResult<Option<FusionDecisionRow>> {
    let connection = connection_for(path)?;
    connection
        .query_row(
            "SELECT id,task_id,question,options_json,status,answer_json FROM fusion_decisions WHERE id=?1",
            params![decision_id],
            |row| {
                Ok(FusionDecisionRow {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    question: row.get(2)?,
                    options_json: row.get(3)?,
                    status: row.get(4)?,
                    answer_json: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(sqlite_api_error)
}

/// 用户回答决策：pending→answered 的 CAS；返回 false 表示已回答/已取消（不可重复回答）。
pub(super) fn fusion_decision_answer(
    path: &Path,
    decision_id: &str,
    answer_json: &str,
) -> ApiResult<bool> {
    let connection = connection_for(path)?;
    let changed = connection
        .execute(
            "UPDATE fusion_decisions SET status='answered', answer_json=?2, answered_at=?3
             WHERE id=?1 AND status='pending'",
            params![decision_id, answer_json, u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(changed > 0)
}

/// 重启清扫：执行者 running 工作项随 brief interrupted 回到 pending（不重放，由主持重派）。
pub(super) fn fusion_work_items_sweep_running(path: &Path) -> ApiResult<usize> {
    let connection = connection_for(path)?;
    let changed = connection
        .execute(
            "UPDATE fusion_work_items SET status='pending', updated_at=?1 WHERE status='running'",
            params![u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(changed)
}

/// 单条工作项（depends_on 校验用）。
pub(super) fn fusion_work_item_get(path: &Path, work_item_id: &str) -> ApiResult<Option<FusionWorkItemRow>> {
    let connection = connection_for(path)?;
    connection
        .query_row(
            "SELECT id,task_id,title,status,depends_on_json,acceptance_json,verification_json,retry_count,max_retries,
                 last_verdict,last_reason FROM fusion_work_items WHERE id=?1",
            params![work_item_id],
            |row| {
                Ok(FusionWorkItemRow {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    depends_on_json: row.get(4)?,
                    acceptance_json: row.get(5)?,
                    verification_json: row.get(6)?,
                    retry_count: row.get(7)?,
                    max_retries: row.get(8)?,
                    last_verdict: row.get(9)?,
                    last_reason: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(sqlite_api_error)
}

pub(super) fn fusion_artifact_insert(
    path: &Path,
    artifact_id: &str,
    task_id: Option<&str>,
    kind: &str,
    content: &str,
) -> ApiResult<()> {
    let connection = connection_for(path)?;
    connection
        .execute(
            "INSERT INTO fusion_artifacts(id,task_id,kind,content,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![artifact_id, task_id, kind, content, u64_to_i64(unix_timestamp_millis())],
        )
        .map_err(sqlite_api_error)?;
    Ok(())
}

pub(super) fn fusion_artifact_get(path: &Path, artifact_id: &str) -> ApiResult<Option<(String, String)>> {
    let connection = connection_for(path)?;
    connection
        .query_row(
            "SELECT kind, content FROM fusion_artifacts WHERE id=?1",
            params![artifact_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(sqlite_api_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "coolzhu-fusion-store-{tag}-{}-{}.sqlite3",
            std::process::id(),
            unix_timestamp_millis()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn fusion_team_upsert_get_list_delete_roundtrip() {
        let path = temp_db("team");
        fusion_team_upsert(
            &path, "t1", "Fusion Team", true, r#"{"host_web_access":false}"#,
            "sess-host", "sess-exec", None,
        )
        .expect("首次 upsert");
        let team = fusion_team_get(&path, "t1").expect("get").expect("team");
        assert_eq!(team.name, "Fusion Team");
        assert!(team.enabled);
        assert_eq!(team.host_session_id.as_deref(), Some("sess-host"));
        assert_eq!(team.executor_session_id.as_deref(), Some("sess-exec"));
        assert_eq!(team.querier_session_id, None);
        assert_eq!(fusion_teams_list(&path).expect("list").len(), 1);

        // 更新：换查询者、改名、关闭。
        fusion_team_upsert(
            &path, "t1", "改名", false, r#"{"host_web_access":true}"#,
            "sess-host", "sess-exec", Some("sess-query"),
        )
        .expect("二次 upsert");
        let team = fusion_team_get(&path, "t1").expect("get2").expect("team2");
        assert_eq!(team.name, "改名");
        assert!(!team.enabled);
        assert_eq!(team.querier_session_id.as_deref(), Some("sess-query"));

        assert!(fusion_team_delete(&path, "t1").expect("delete"));
        assert!(fusion_team_get(&path, "t1").expect("get3").is_none());
        assert!(fusion_teams_list(&path).expect("list2").is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fusion_task_work_items_and_verdict_flow() {
        let path = temp_db("task");
        fusion_task_insert(&path, "task-1", "t1", "room-1", "ws", Some("任务"), None, "{}")
            .expect("insert task");
        let open = fusion_task_latest_open(&path, "room-1", "t1")
            .expect("latest")
            .expect("open task");
        assert_eq!(open.id, "task-1");
        assert_eq!(open.status, "planning");

        let ids = fusion_work_items_replace(
            &path,
            "task-1",
            &[
                serde_json::json!({"title":"实现 A","acceptance_criteria":["A 存在"]}),
                serde_json::json!({"id":"wi-x","title":"实现 B","depends_on":["wi-1"]}),
            ],
        )
        .expect("replace work items");
        assert_eq!(ids.len(), 2);
        let items = fusion_work_items_for_task(&path, "task-1").expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].status, "pending");
        assert_eq!(items[1].id, "wi-x");

        let (retries, exceeded) =
            fusion_work_item_verdict(&path, &items[0].id, "rework", Some("缺文件"), None)
                .expect("verdict")
                .expect("work item 存在");
        assert_eq!(retries, 1);
        assert!(!exceeded);
        let items_after = fusion_work_items_for_task(&path, "task-1").expect("items2");
        assert_eq!(items_after[0].status, "rework");
        // 连续返工顶到 max_retries=2：第 3 次 rework 置 blocked。
        fusion_work_item_verdict(&path, &items_after[0].id, "rework", Some("仍缺"), None)
            .expect("verdict2");
        let (retries, exceeded) =
            fusion_work_item_verdict(&path, &items_after[0].id, "rework", Some("还缺"), None)
                .expect("verdict3")
                .expect("work item");
        assert_eq!(retries, 3);
        assert!(exceeded);
        assert_eq!(
            fusion_work_item_get(&path, &items_after[0].id)
                .expect("get")
                .expect("item")
                .status,
            "blocked"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fusion_brief_lifecycle_and_qa_path() {
        let path = temp_db("brief");
        // 问答型：task_id 为空也允许入队（设计 §6.2）。
        let qa = FusionBriefRow {
            id: "b-qa".to_string(),
            task_id: None,
            work_item_id: None,
            room_id: "room-1".to_string(),
            team_id: "t1".to_string(),
            from_role: "host".to_string(),
            to_role: "querier".to_string(),
            kind: "query".to_string(),
            payload_json: r#"{"question":"x"}"#.to_string(),
            status: String::new(),
            result_json: None,
        };
        fusion_brief_insert(&path, &qa).expect("insert qa brief");
        let queued = fusion_briefs_queued(&path, "room-1", "t1").expect("queued");
        assert_eq!(queued.len(), 1);
        assert!(queued[0].task_id.is_none());

        assert!(fusion_brief_claim(&path, "b-qa", "run-1").expect("claim"));
        // 已 running 的不能二次 claim。
        assert!(!fusion_brief_claim(&path, "b-qa", "run-2").expect("claim2"));
        fusion_brief_finish(&path, "b-qa", "done", r#"{"summary":"ok"}"#).expect("finish");
        let done = fusion_brief_get(&path, "b-qa").expect("get").expect("brief");
        assert_eq!(done.status, "done");

        // running 残留由 sweep 收敛为 interrupted。
        let running = FusionBriefRow {
            id: "b-run".to_string(),
            task_id: None,
            work_item_id: None,
            room_id: "room-1".to_string(),
            team_id: "t1".to_string(),
            from_role: "host".to_string(),
            to_role: "executor".to_string(),
            kind: "work".to_string(),
            payload_json: "{}".to_string(),
            status: String::new(),
            result_json: None,
        };
        fusion_brief_insert(&path, &running).expect("insert running brief");
        assert!(fusion_brief_claim(&path, "b-run", "run-3").expect("claim3"));
        assert_eq!(fusion_briefs_sweep_interrupted(&path).expect("sweep"), 1);
        assert_eq!(
            fusion_brief_get(&path, "b-run").expect("get2").expect("brief2").status,
            "interrupted"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fusion_decision_answer_is_pending_cas() {
        let path = temp_db("decision");
        fusion_task_insert(&path, "task-1", "t1", "room-1", "ws", None, None, "{}")
            .expect("insert task");
        fusion_decision_insert(
            &path,
            "dec-1",
            "task-1",
            "选哪个方案？",
            r#"{"options":[{"id":"a","label":"方案 A"}],"allow_free_text":true}"#,
        )
        .expect("insert decision");
        let decision = fusion_decision_get(&path, "dec-1")
            .expect("get")
            .expect("decision");
        assert_eq!(decision.status, "pending");
        assert_eq!(decision.task_id, "task-1");
        assert!(decision.answer_json.is_none());
        // pending→answered 只允许一次；重复回答/篡改被 CAS 拒绝。
        assert!(fusion_decision_answer(&path, "dec-1", r#"{"label":"方案 A"}"#).expect("answer"));
        assert!(!fusion_decision_answer(&path, "dec-1", r#"{"label":"篡改"}"#).expect("answer2"));
        let answered = fusion_decision_get(&path, "dec-1")
            .expect("get2")
            .expect("answered");
        assert_eq!(answered.status, "answered");
        assert!(answered.answer_json.unwrap().contains("方案 A"));
        assert!(fusion_decisions_pending(&path, "task-1")
            .expect("pending")
            .is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fusion_work_items_sweep_returns_running_to_pending() {
        let path = temp_db("sweep");
        fusion_task_insert(&path, "task-1", "t1", "room-1", "ws", None, None, "{}")
            .expect("insert task");
        let ids = fusion_work_items_replace(
            &path,
            "task-1",
            &[
                serde_json::json!({"id":"wi-a","title":"A","depends_on":["wi-b"]}),
                serde_json::json!({"id":"wi-b","title":"B"}),
            ],
        )
        .expect("replace");
        assert_eq!(ids.len(), 2);
        // depends_on 落库可读回（调度门的数据源）。
        let item = fusion_work_item_get(&path, "wi-a").expect("get").expect("item");
        assert_eq!(item.depends_on_json, r#"["wi-b"]"#);
        fusion_work_item_set_status(&path, "wi-a", "running").expect("set running");
        fusion_work_item_set_status(&path, "wi-b", "done").expect("set done");
        assert_eq!(fusion_work_items_sweep_running(&path).expect("sweep"), 1);
        let item = fusion_work_item_get(&path, "wi-a").expect("get2").expect("item2");
        assert_eq!(item.status, "pending");
        // done 不受影响。
        assert_eq!(
            fusion_work_item_get(&path, "wi-b").expect("get3").expect("item3").status,
            "done"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fusion_briefs_by_status_scopes_room_and_team() {
        let path = temp_db("bystatus");
        for (id, room, team) in [
            ("b-1", "room-1", "t1"),
            ("b-2", "room-1", "t2"),
            ("b-3", "room-2", "t1"),
        ] {
            fusion_brief_insert(
                &path,
                &FusionBriefRow {
                    id: id.to_string(),
                    task_id: None,
                    work_item_id: None,
                    room_id: room.to_string(),
                    team_id: team.to_string(),
                    from_role: "host".to_string(),
                    to_role: "executor".to_string(),
                    kind: "work".to_string(),
                    payload_json: "{}".to_string(),
                    status: String::new(),
                    result_json: None,
                },
            )
            .expect("insert");
            fusion_brief_claim(&path, id, "run-x").expect("claim");
        }
        let running = fusion_briefs_by_status(&path, "room-1", "t1", "running").expect("running");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "b-1");
        let _ = std::fs::remove_file(&path);
    }
}
