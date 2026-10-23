//! 运行原始数据保留十二小时；聊天正文与用量事实不参与清理。
use super::*;
pub(super) const WINDOW_MS: u64 = 12 * 60 * 60 * 1000;
const TABLES: &[&str] = &[
    "runtime_runs",
    "runtime_run_events",
    "tool_calls",
    "computer_use_runs",
    "computer_use_steps",
    "computer_use_step_details",
    "computer_use_planner_diagnostics",
    "computer_use_plan_attempts",
    "devin_acp_config_receipts",
    "devin_acp_context_inputs",
    "execution_index",
];

fn exists(c: &Connection, table: &str) -> rusqlite::Result<bool> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )
}
fn rows(c: &Connection, sql: &str) -> rusqlite::Result<Vec<String>> {
    let mut s = c.prepare(sql)?;
    let result = s.query_map([], |r| r.get(0))?.collect();
    result
}

pub(super) fn spawn() {
    tokio::spawn(async {
        // 首屏与恢复不等待扫描；每轮只处理一个有界批次。
        tokio::time::sleep(Duration::from_secs(90)).await;
        loop {
            if let Ok(mut jobs) = video_jobs().lock() {
                let cutoff = unix_timestamp_millis().saturating_sub(WINDOW_MS);
                jobs.retain(|_, job| {
                    !matches!(
                        job.status.as_str(),
                        "completed" | "failed" | "cancelled" | "interrupted"
                    ) || job.updated_at >= cutoff
                });
            }
            if let Ok(pin) = workspace_activity::pin_workspace() {
                let path = default_session_sqlite_path();
                let workspace = active_workspace_path();
                let jobs = read_config(|config| {
                    workspace_data_dir_for(&workspace, config).join("scheduled-jobs.sqlite3")
                });
                let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                    let _pin = pin;
                    let now = unix_timestamp_millis();
                    clean(&path, now, safety_is_resolved()).map_err(|e| e.to_string())?;
                    let c = open_session_connection(&path).map_err(|e| e.to_string())?;
                    let protected = rows(
                        &c,
                        "SELECT id FROM goals WHERE status NOT IN ('completed','cancelled')",
                    )
                    .map_err(|e| e.to_string())?;
                    let deleted = scheduled_jobs::clean_projected(
                        &jobs,
                        now.saturating_sub(WINDOW_MS),
                        &protected,
                    )?;
                    if deleted > 0 {
                        diag_log(&format!(
                            "[RETENTION] scheduled_raw_results={deleted} database={}",
                            jobs.display()
                        ));
                    }
                    Ok(())
                })
                .await;
                if let Ok(Err(error)) = result {
                    diag_log(&format!("[RETENTION] 本批次未清理完成，稍后重试：{error}"));
                }
            }
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    });
}
fn safety_is_resolved() -> bool {
    let Some(root) = input_safety_store::input_safety_state_root() else {
        return false;
    };
    let Ok(c) = Connection::open_with_flags(
        root.join(input_safety_store::INPUT_SAFETY_DB_FILE),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return false;
    };
    c.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM input_safety_incidents WHERE resolved_at_unix_ms IS NULL)",
        [],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

pub(super) fn clean(path: &Path, now: u64, cu_safe: bool) -> rusqlite::Result<usize> {
    let mut c = open_session_connection(path)?;
    c.busy_timeout(Duration::from_millis(500))?;
    chat_insights::ensure_tables(&c)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let cutoff = now.saturating_sub(WINDOW_MS);
    tx.execute_batch("CREATE TEMP TABLE retention_runs(id TEXT PRIMARY KEY); CREATE TEMP TABLE retention_cu(id TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS runtime_retention_files(path TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS runtime_retention_status(id INTEGER PRIMARY KEY CHECK(id=1),last_at INTEGER,deleted_rows INTEGER);")?;
    tx.execute("INSERT INTO retention_runs SELECT id FROM runtime_runs r WHERE state IN ('completed','failed','interrupted','orphaned') AND finished_at<?1
        AND NOT EXISTS(SELECT 1 FROM goals g WHERE g.id=r.goal_id AND g.status NOT IN ('completed','cancelled'))
        AND NOT EXISTS(SELECT 1 FROM runtime_runs live WHERE live.state IN ('accepted','running','stop_requested') AND (live.id=r.id OR (r.goal_id IS NOT NULL AND live.goal_id=r.goal_id)))
        ORDER BY finished_at LIMIT 200",[cutoff])?;
    // 事故或未排空的 CU 与其父运行一起保留。
    if exists(&tx, "computer_use_runs")? {
        tx.execute_batch("DELETE FROM retention_runs WHERE id IN (SELECT r.id FROM runtime_runs r JOIN computer_use_runs cu ON cu.turn_id=r.legacy_turn_id
          WHERE cu.state NOT IN ('succeeded','failed','blocked','cancelled','timed_out')
          OR EXISTS(SELECT 1 FROM computer_use_cleanup_incidents i WHERE i.run_id=cu.call_id )
          OR EXISTS(SELECT 1 FROM computer_use_steps s WHERE s.run_id=cu.call_id AND s.input_release_status='unknown'));")?;
        if !cu_safe {
            tx.execute_batch("DELETE FROM retention_runs WHERE id IN (SELECT r.id FROM runtime_runs r JOIN computer_use_runs cu ON cu.turn_id=r.legacy_turn_id);")?;
        }
        if cu_safe {
            tx.execute("INSERT INTO retention_cu SELECT call_id FROM computer_use_runs cu WHERE updated_at_ms<?1
              AND state IN ('succeeded','failed','blocked','cancelled','timed_out')
              AND NOT EXISTS(SELECT 1 FROM runtime_runs r WHERE r.legacy_turn_id=cu.turn_id AND r.id NOT IN (SELECT id FROM retention_runs))
              AND NOT EXISTS(SELECT 1 FROM computer_use_cleanup_incidents i WHERE i.run_id=cu.call_id)
              AND NOT EXISTS(SELECT 1 FROM computer_use_steps s WHERE s.run_id=cu.call_id AND s.input_release_status='unknown')
              ORDER BY updated_at_ms LIMIT 200",[cutoff])?;
        }
        tx.execute_batch("INSERT OR IGNORE INTO runtime_retention_files SELECT before_evidence_ref FROM computer_use_steps WHERE run_id IN (SELECT id FROM retention_cu) AND before_evidence_ref IS NOT NULL;
          INSERT OR IGNORE INTO runtime_retention_files SELECT after_evidence_ref FROM computer_use_steps WHERE run_id IN (SELECT id FROM retention_cu) AND after_evidence_ref IS NOT NULL;")?;
    }
    let mut deleted = 0;
    if exists(&tx, "devin_acp_attempts")? {
        // 未知/在途协议状态不是完成；有锁或未排空时不删其运行。
        tx.execute_batch("DELETE FROM retention_runs WHERE id IN (SELECT json_extract(scope_json,'$.run_id') FROM devin_acp_attempts WHERE state NOT IN ('terminal','not_sent') OR process_drained=0 OR attempt_id IN (SELECT locked_attempt FROM devin_acp_bindings WHERE locked_attempt IS NOT NULL));")?;
        tx.execute_batch("CREATE TEMP TABLE retention_acp(id TEXT PRIMARY KEY);
          INSERT INTO retention_acp SELECT attempt_id FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id') IN (SELECT id FROM retention_runs);")?;
        // 留下轻量、无正文的统计事实，旧 ACP 不虚构 token。随后可移除原始协议回执。
        tx.execute_batch("INSERT OR IGNORE INTO chat_usage_events(workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,turn_id,run_id,request_kind,logical_request_id,attempt_id,status,dispatched,usage_known_mask,model_snapshot,protocol_snapshot)
          SELECT json_extract(a.scope_json,'$.workspace_id'),json_extract(a.scope_json,'$.room_id'),json_extract(a.scope_json,'$.agent_id'),r.created_at,0,0,0,0,
          json_extract(a.scope_json,'$.turn_id'),r.id,'devin_chat','acp:'||a.attempt_id,'acp:'||a.attempt_id,
          CASE WHEN a.state='not_sent' THEN 'not_sent' WHEN a.protocol_stop='end_turn' THEN 'completed' ELSE 'remote_unknown' END,
          CASE WHEN a.state='not_sent' THEN 0 ELSE 1 END,0,NULL,'devin_acp'
          FROM devin_acp_attempts a JOIN runtime_runs r ON r.id=json_extract(a.scope_json,'$.run_id') WHERE a.attempt_id IN (SELECT id FROM retention_acp);")?;
        if exists(&tx, "devin_acp_context_inputs")? && exists(&tx, "devin_acp_bindings")? {
            // 一个远端会话所有上下文均过期且无执行锁时，移除本地续接绑定。
            // 下一轮从保留的聊天历史重建上下文，避免跨 12h 保留整份旧输入。
            tx.execute_batch("DELETE FROM devin_acp_bindings WHERE locked_attempt IS NULL
              AND remote_session_id IN (SELECT remote_session_id FROM devin_acp_context_inputs WHERE attempt_id IN (SELECT id FROM retention_acp))
              AND NOT EXISTS(SELECT 1 FROM devin_acp_context_inputs c WHERE c.remote_session_id=devin_acp_bindings.remote_session_id AND c.attempt_id NOT IN (SELECT id FROM retention_acp));")?;
        }
        for table in ["devin_acp_config_receipts", "devin_acp_context_inputs"] {
            if exists(&tx, table)? {
                deleted += tx.execute(
                    &format!(
                        "DELETE FROM {table} WHERE attempt_id IN (SELECT id FROM retention_acp)"
                    ),
                    [],
                )?;
            }
        }
        if exists(&tx, "devin_acp_retired_bindings")? && exists(&tx, "devin_acp_context_inputs")? {
            tx.execute_batch("DELETE FROM devin_acp_retired_bindings WHERE remote_session_id NOT IN (SELECT remote_session_id FROM devin_acp_context_inputs)
              AND remote_session_id NOT IN (SELECT remote_session_id FROM devin_acp_bindings WHERE remote_session_id IS NOT NULL);")?;
        }
        deleted += tx.execute(
            "DELETE FROM devin_acp_attempts WHERE attempt_id IN (SELECT id FROM retention_acp)",
            [],
        )?;
    }
    if exists(&tx, "computer_use_runs")? {
        // ACP 的未知执行保护晚于候选选择，必须再次同步其 CU 子记录。
        tx.execute_batch("DELETE FROM retention_cu WHERE id IN (SELECT cu.call_id FROM computer_use_runs cu JOIN runtime_runs r ON r.legacy_turn_id=cu.turn_id WHERE r.id NOT IN (SELECT id FROM retention_runs));")?;
    }
    if exists(&tx, "execution_index")? {
        deleted+=tx.execute("DELETE FROM execution_index WHERE id IN (SELECT e.id FROM execution_index e WHERE e.finished_ms<?1
          AND e.state IN ('completed','failed','interrupted','cancelled') AND e.run_id IS NULL
          AND json_extract(e.spec_json,'$.source')!='goal_phase'
          AND NOT EXISTS(SELECT 1 FROM runtime_runs r WHERE r.id=json_extract(e.spec_json,'$.parent_run_id') AND r.id NOT IN (SELECT id FROM retention_runs))
          ORDER BY e.finished_ms LIMIT 200)",[cutoff])?;
    }
    for (table, column) in [
        ("computer_use_step_details", "run_id"),
        ("computer_use_plan_attempts", "call_id"),
        ("computer_use_action_origin_rejections", "call_id"),
        ("computer_use_planner_diagnostics", "call_id"),
        ("computer_use_steps", "run_id"),
        ("computer_use_runs", "call_id"),
    ] {
        if exists(&tx, table)? {
            deleted += tx.execute(
                &format!("DELETE FROM {table} WHERE {column} IN (SELECT id FROM retention_cu)"),
                [],
            )?;
        }
    }
    for table in ["execution_index", "tool_calls", "runtime_run_events"] {
        if exists(&tx, table)? {
            deleted += tx.execute(
                &format!("DELETE FROM {table} WHERE run_id IN (SELECT id FROM retention_runs)"),
                [],
            )?;
        }
    }
    deleted += tx.execute(
        "DELETE FROM runtime_runs WHERE id IN (SELECT id FROM retention_runs)",
        [],
    )?;
    tx.execute("INSERT INTO runtime_retention_status VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET last_at=excluded.last_at,deleted_rows=excluded.deleted_rows",params![now,deleted])?;
    tx.commit()?;
    clean_files(&c, path)?;
    if deleted > 0 {
        diag_log(&format!(
            "[RETENTION] at={now} cutoff={cutoff} deleted_rows={deleted} database={}",
            path.display()
        ));
    }
    Ok(deleted)
}

pub(super) fn evidence_paths(value: &str) -> Vec<PathBuf> {
    let references =
        serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| vec![value.to_string()]);
    references
        .into_iter()
        .filter_map(|reference| {
            let value = reference
                .strip_prefix("screenshot:")?
                .split_once(":sha256=")?
                .0;
            let path = PathBuf::from(value);
            path.is_absolute().then_some(path)
        })
        .collect()
}
fn clean_files(c: &Connection, db: &Path) -> rusqlite::Result<()> {
    let parent = db.parent().unwrap_or(Path::new("."));
    // 仅删除本数据库同目录下的运行证据；外置共享截图目录不猜测归属。
    let roots = [
        parent.join("screenshots"),
        parent.join("computer-use"),
        parent.join("desktop-capture"),
    ]
    .into_iter()
    .filter_map(|p| p.canonicalize().ok())
    .collect::<Vec<_>>();
    for value in rows(
        c,
        "SELECT path FROM runtime_retention_files ORDER BY rowid LIMIT 200",
    )? {
        let mut retry = false;
        for candidate in evidence_paths(&value) {
            let Ok(path) = candidate.canonicalize() else {
                continue;
            };
            if !roots.iter().any(|root| path.starts_with(root)) || !path.is_file() {
                continue;
            }
            // 文件名包含完整内容摘要；同时保护 JSON 转义路径、同文件的其他 CU 与聊天引用。
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let referenced:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM computer_use_steps WHERE instr(before_evidence_ref,?1)>0 OR instr(after_evidence_ref,?1)>0)
              OR EXISTS(SELECT 1 FROM chat_room_messages WHERE instr(content,?1)>0 OR instr(attachments_json,?1)>0)
              OR EXISTS(SELECT 1 FROM session_messages WHERE instr(content,?1)>0 OR instr(attachments_json,?1)>0)",[name],|r|r.get(0))?;
            if !referenced && std::fs::remove_file(&path).is_err() {
                retry = true;
            }
        }
        // 活动引用将随它自己的运行再次入队；坏路径不能堵住后续批次。
        c.execute(
            "DELETE FROM runtime_retention_files WHERE path=?1",
            [&value],
        )?;
        if retry {
            c.execute(
                "INSERT OR IGNORE INTO runtime_retention_files(path) VALUES(?1)",
                [value],
            )?;
        }
    }
    Ok(())
}

pub(super) async fn status() -> ApiResult<Json<JsonValue>> {
    let path = default_session_sqlite_path();
    let result=tokio::task::spawn_blocking(move || -> rusqlite::Result<JsonValue> {
        let c=open_session_connection(&path)?; let mut tables=Vec::new(); let mut bytes=0_i64;
        for table in TABLES { if !exists(&c,table)? {continue;}
            let count:i64=c.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|r|r.get(0))?;
            let size=c.query_row("SELECT COALESCE(SUM(pgsize),0) FROM dbstat WHERE name=?1",[table],|r|r.get::<_,i64>(0)).ok();
            bytes+=size.unwrap_or(0);tables.push(json!({"table":table,"rows":count,"bytes":size}));
        }
        let (earliest, latest_run):(Option<i64>,Option<i64>)=c.query_row("SELECT MIN(created_at),MAX(created_at) FROM runtime_runs",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let span=latest_run.unwrap_or(0).saturating_sub(earliest.unwrap_or(0)).max(3600000);
        let estimate=bytes as f64*WINDOW_MS as f64/span as f64;
        let warning=(estimate>512.0*1048576.0).then_some("按当前记录速率估算，十二小时运行数据超过 512 MB；请检查重复重试、长时间活动任务或未处理事故。");
        let latest=if exists(&c,"runtime_retention_status")? {c.query_row("SELECT last_at,deleted_rows FROM runtime_retention_status WHERE id=1",[],|r|Ok(json!({"at":r.get::<_,i64>(0)?,"deleted_rows":r.get::<_,i64>(1)?}))).optional()?} else {None};
        Ok(json!({"retention_hours":12,"tables":tables,"database_bytes":std::fs::metadata(path).ok().map(|m|m.len()),"runtime_bytes":bytes,
          "estimated_12h_bytes":estimate,"earliest_record_at":earliest,"growth_warning":warning,"last_cleanup":latest,
          "note":"每 5 分钟分批清理已结束且超过 12 小时的运行原始数据。保护在途任务、未完成 Goal、未解决事故及聊天引用文件；过期的空闲订阅会话在下轮从聊天历史重建。聊天正文、用量长期保留。空闲页由 SQLite 复用，不在聊天时强制压缩。大小为数据库页估算。"}))
    }).await.map_err(|e|api_error(StatusCode::INTERNAL_SERVER_ERROR,&e.to_string()))?.map_err(sqlite_api_error)?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(c: &Connection, id: &str, state: &str, goal: Option<&str>, time: u64) {
        c.execute("INSERT INTO runtime_runs(id,kind,workspace_id,legacy_turn_id,goal_id,state,claim_token,created_at,finished_at) VALUES(?1,'chat_turn','ws',?1,?2,?3,'claim',?4,?4)",params![id,goal,state,time]).unwrap();
    }
    fn cu(c: &Connection, id: &str, turn: &str, time: u64, evidence: &str) {
        c.execute("INSERT INTO computer_use_runs(call_id,turn_id,session_id,idempotency_key,objective_json,surface,state,deadline_ms,created_at_ms,updated_at_ms) VALUES(?1,?2,'s',?1,'{}','desktop','succeeded',1,?3,?3)",params![id,turn,time]).unwrap();
        c.execute("INSERT INTO computer_use_steps(run_id,step_index,observation_generation,action_type,normalized_target,action_fingerprint,status,started_at_ms,before_evidence_ref) VALUES(?1,1,1,'observe','{}','hash','succeeded',1,?2)",params![id,evidence]).unwrap();
    }
    #[test]
    fn retention_removes_old_finished_runtime_but_preserves_chat_live_goals_and_incidents() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let c = open_session_connection(&db).unwrap();
        initialize_session_schema(&c).unwrap();
        let now = WINDOW_MS * 2;
        let old = 1;
        c.execute("INSERT INTO goals(id,workspace_id,chat_room_id,title,status,max_iterations,current_iteration,background,completion_condition_json,created_at,updated_at) VALUES('goal','ws','room','G','running',1,0,0,'{}',1,1)",[]).unwrap();
        for (id, state, goal, time) in [
            ("old", "completed", None, old),
            ("live", "running", None, old),
            ("goal-run", "completed", Some("goal"), old),
            ("recent", "completed", None, now),
            ("incident", "completed", None, old),
        ] {
            run(&c, id, state, goal, time);
        }
        let root = dir.path().join("desktop-capture/computer-use");
        std::fs::create_dir_all(&root).unwrap();
        let image = root.join("old-hash.png");
        std::fs::write(&image, b"image").unwrap();
        let refs = serde_json::to_string(&vec![format!(
            "screenshot:{}:sha256=abc:1x1",
            image.display()
        )])
        .unwrap();
        cu(&c, "cu-old", "old", old, &refs);
        cu(&c, "cu-incident", "incident", old, "[]");
        c.execute("UPDATE computer_use_steps SET input_release_status='unknown' WHERE run_id='cu-incident'",[]).unwrap();
        // No chat row is a cleanup candidate, regardless of age.
        c.execute(
            "INSERT INTO chat_rooms(id,name,created_at,updated_at) VALUES('room','chat',1,1)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES('room','message','user','user','chat','keep me','text','[]',1)",[]).unwrap();
        assert!(clean(&db, now, true).unwrap() > 0);
        assert_eq!(
            rows(&c, "SELECT id FROM runtime_runs ORDER BY id").unwrap(),
            vec!["goal-run", "incident", "live", "recent"]
        );
        assert!(image.exists() == false);
        assert_eq!(
            rows(&c, "SELECT content FROM chat_room_messages").unwrap(),
            vec!["keep me"]
        );
        assert_eq!(
            rows(&c, "SELECT call_id FROM computer_use_runs").unwrap(),
            vec!["cu-incident"]
        );
        assert_eq!(clean(&db, now, true).unwrap(), 0);
    }
    #[test]
    fn evidence_cleanup_keeps_shared_chat_refs_and_outside_files() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let c = open_session_connection(&db).unwrap();
        initialize_session_schema(&c).unwrap();
        let outside = dir.path().join("user.png");
        std::fs::write(&outside, b"keep").unwrap();
        run(&c, "old", "completed", None, 1);
        let refs = serde_json::to_string(&vec![format!(
            "screenshot:{}:sha256=abc:1x1",
            outside.display()
        )])
        .unwrap();
        cu(&c, "cu", "old", 1, &refs);
        clean(&db, WINDOW_MS * 2, false).unwrap();
        assert_eq!(
            rows(&c, "SELECT id FROM runtime_runs").unwrap(),
            vec!["old"]
        );
        clean(&db, WINDOW_MS * 2, true).unwrap();
        assert!(outside.exists());
        assert!(rows(&c, "SELECT path FROM runtime_retention_files")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn acp_expiry_drops_idle_context_and_binding_but_protects_unknown_attempt_and_cu() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite3");
        let c = open_session_connection(&db).unwrap();
        initialize_session_schema(&c).unwrap();
        c.execute_batch("CREATE TABLE devin_acp_attempts(attempt_id TEXT PRIMARY KEY,scope_json TEXT,state TEXT,process_drained INTEGER,protocol_stop TEXT);
          CREATE TABLE devin_acp_bindings(remote_session_id TEXT,locked_attempt TEXT);
          CREATE TABLE devin_acp_context_inputs(attempt_id TEXT,remote_session_id TEXT);
          CREATE TABLE devin_acp_config_receipts(attempt_id TEXT,receipt_json TEXT);
          CREATE TABLE devin_acp_retired_bindings(remote_session_id TEXT);").unwrap();
        for id in ["finished", "uncertain"] {
            run(&c, id, "completed", None, 1);
            cu(&c, &format!("cu-{id}"), id, 1, "[]");
            let scope=json!({"workspace_id":"ws","room_id":"room","agent_id":"agent","turn_id":id,"run_id":id}).to_string();
            c.execute(
                "INSERT INTO devin_acp_attempts VALUES(?1,?2,?3,?4,'end_turn')",
                params![
                    id,
                    scope,
                    if id == "finished" {
                        "terminal"
                    } else {
                        "unknown"
                    },
                    if id == "finished" { 1 } else { 0 }
                ],
            )
            .unwrap();
            c.execute(
                "INSERT INTO devin_acp_bindings VALUES(?1,?2)",
                params![id, if id == "finished" { None } else { Some(id) }],
            )
            .unwrap();
            c.execute("INSERT INTO devin_acp_context_inputs VALUES(?1,?1)", [id])
                .unwrap();
            c.execute(
                "INSERT INTO devin_acp_config_receipts VALUES(?1,'large-raw-body')",
                [id],
            )
            .unwrap();
        }
        clean(&db, WINDOW_MS * 2, true).unwrap();
        assert_eq!(
            rows(&c, "SELECT id FROM runtime_runs").unwrap(),
            vec!["uncertain"]
        );
        assert_eq!(
            rows(&c, "SELECT call_id FROM computer_use_runs").unwrap(),
            vec!["cu-uncertain"]
        );
        assert_eq!(
            rows(&c, "SELECT remote_session_id FROM devin_acp_bindings").unwrap(),
            vec!["uncertain"]
        );
        assert_eq!(
            rows(&c, "SELECT attempt_id FROM devin_acp_context_inputs").unwrap(),
            vec!["uncertain"]
        );
        assert_eq!(
            rows(&c, "SELECT attempt_id FROM devin_acp_config_receipts").unwrap(),
            vec!["uncertain"]
        );
        assert_eq!(
            c.query_row(
                "SELECT usage_known_mask FROM chat_usage_events WHERE attempt_id='acp:finished'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
