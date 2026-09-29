//! 本地绑定与写操作账本。先持久登记 unknown，再发 POST；中断后不自动重发。
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::Path;

fn error(_: rusqlite::Error) -> String {
    "Devin 本地会话账本读写失败".into()
}

pub(super) fn open(root: &Path) -> Result<Connection, String> {
    let directory = root.join(".coolzhu");
    std::fs::create_dir_all(&directory).map_err(|_| "Devin 本地目录不可写")?;
    let db = Connection::open(directory.join("devin-sessions.sqlite3")).map_err(error)?;
    db.busy_timeout(std::time::Duration::from_secs(3))
        .map_err(error)?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS bindings(
        session_id TEXT PRIMARY KEY, org_id TEXT NOT NULL, fact TEXT NOT NULL,
        created_ms INTEGER NOT NULL, source TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS operations(
        id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL, state TEXT NOT NULL,
        result TEXT, created_ms INTEGER NOT NULL);",
    )
    .map_err(error)?;
    Ok(db)
}

pub(super) fn reserve(
    db: &Connection,
    id: &str,
    fingerprint: &str,
) -> Result<Option<Value>, String> {
    let inserted = db
        .execute(
            "INSERT OR IGNORE INTO operations(id,fingerprint,state,created_ms)
        VALUES(?1,?2,'unknown',?3)",
            params![id, fingerprint, now_ms()],
        )
        .map_err(error)?;
    if inserted == 1 {
        return Ok(None);
    }
    let (previous, state, result): (String, String, Option<String>) = db
        .query_row(
            "SELECT fingerprint,state,result FROM operations WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(error)?;
    if previous != fingerprint {
        return Err("操作编号已用于其他内容，未发送 Devin 请求".into());
    }
    if state != "accepted" {
        return Err("此写操作结果未知或失败，已阻止自动重发；请先到 Devin 官网核对".into());
    }
    result
        .and_then(|text| serde_json::from_str(&text).ok())
        .map(Some)
        .ok_or_else(|| "Devin 操作账本结果损坏，未重发".into())
}

pub(super) fn bind(db: &Connection, fact: &Value, org: &str, source: &str) -> Result<(), String> {
    let id = fact["session_id"].as_str().ok_or("会话结果缺少 ID")?;
    db.execute("INSERT INTO bindings(session_id,org_id,fact,created_ms,source) VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(session_id) DO UPDATE SET fact=excluded.fact WHERE bindings.org_id=excluded.org_id",
        params![id,org,fact.to_string(),now_ms(),source]).map_err(error)?;
    bound(db, id, org)
}

pub(super) fn bound(db: &Connection, id: &str, org: &str) -> Result<(), String> {
    let found: Option<String> = db
        .query_row(
            "SELECT org_id FROM bindings WHERE session_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(error)?;
    if found.as_deref() != Some(org) {
        return Err("该 Devin 会话未绑定当前工程与组织，请先手动关联".into());
    }
    Ok(())
}

pub(super) fn settle(db: &Connection, id: &str, result: &Value) -> Result<(), String> {
    db.execute(
        "UPDATE operations SET state='accepted',result=?2 WHERE id=?1",
        params![id, result.to_string()],
    )
    .map_err(error)?;
    Ok(())
}

pub(super) fn list(db: &Connection, org: &str) -> Result<Vec<Value>, String> {
    let mut stmt = db
        .prepare("SELECT fact FROM bindings WHERE org_id=?1 ORDER BY created_ms DESC LIMIT 100")
        .map_err(error)?;
    let rows = stmt
        .query_map([org], |row| row.get::<_, String>(0))
        .map_err(error)?;
    rows.map(|row| {
        serde_json::from_str(&row.map_err(error)?).map_err(|_| "Devin 绑定记录损坏".into())
    })
    .collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_post_and_cross_workspace_never_replay() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let db = open(a.path()).unwrap();
        assert!(reserve(&db, "request-a", "payload-a").unwrap().is_none());
        assert!(reserve(&db, "request-a", "payload-a").is_err());
        assert!(reserve(&db, "request-a", "payload-b").is_err());
        let fact = serde_json::json!({"session_id":"devin-a","status":"running"});
        bind(&db, &fact, "org-a", "manual").unwrap();
        assert!(bound(&db, "devin-a", "org-b").is_err());
        assert!(bound(&open(b.path()).unwrap(), "devin-a", "org-a").is_err());
        settle(&db, "request-a", &fact).unwrap();
        assert_eq!(reserve(&db, "request-a", "payload-a").unwrap(), Some(fact));
    }
}
