//! 聊天室任务清单（TodoWrite 宿主执行内核）。
//!
//! 与 tool-registry 的 CLI 文件存储不同：这里把模型写入的清单按
//! `(room_id, session_id)` 落到会话 sqlite，供房间 UI 回读、上下文快照
//! 重建与生命周期清理。校验规则复用 `tools::normalize_todo_write`，
//! 保证宿主与 CLI 单一事实源。
use super::*;

/// v30 迁移：新增 `chat_room_todos` 表。
///
/// 主键 `(room_id, session_id)`、整表 JSON 全量替换、单行原子读写；
/// `room_id` 外键级联覆盖删房间，`session_id` 不加外键——会话集合由
/// `save()` upsert 维护，按"会话消失即清孤儿行"的既有模式显式清理，
/// 避免级联误删（save 路径在 `PRAGMA foreign_keys = OFF` 下删房间/会话）。
pub(crate) fn apply_session_migration_v30(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS chat_room_todos (
            room_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            todos_json TEXT NOT NULL,
            source TEXT NOT NULL,
            revision INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_call_id TEXT,
            PRIMARY KEY (room_id, session_id),
            FOREIGN KEY (room_id) REFERENCES chat_rooms(id) ON DELETE CASCADE
        );
        "#,
    )?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 30 {
        connection.execute_batch("PRAGMA user_version = 30;")?;
    }
    Ok(())
}

/// 落库的单条清单项。`activeForm` 用驼峰与工具 schema/前端保持一致；
/// `priority` 为 ACP plan 映射预留（本 PR 恒 None）。
#[derive(Debug, Serialize)]
pub(crate) struct StoredTodo {
    pub content: String,
    #[serde(rename = "activeForm")]
    pub active_form: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
}

/// TodoWrite 的宿主工具定义：schema 与 registry mvp 版完全一致，
/// 描述用中文写明使用边界与"轻量清单 ≠ Goal 编排"的分工。
pub(super) fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "TodoWrite".to_string(),
        description: Some(
            "更新当前会话的结构化任务清单（整表替换）。仅在多步任务（≥3 个动作、\
             长时程或用户点名要清单）时建立；每次提交完整新清单；开始一项标 \
             in_progress、完成一项立即标 completed；尽量与其它工具同批调用以节省轮次；\
             全部完成后清单自动清空。这是会话内轻量进度清单，不是 Goal 目标编排，\
             完成状态不等于任务已验证。"
                .to_string(),
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string" },
                            "activeForm": { "type": "string" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            }
                        },
                        "required": ["content", "activeForm", "status"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["todos"],
            "additionalProperties": false
        }),
    }
}

/// 宿主侧执行 TodoWrite：校验 → 事务内 upsert/删行 → 广播事件 → 精简中文回执。
pub(super) fn execute(invoke: &ToolInvoke, room_id: Option<&str>) -> Result<String, String> {
    execute_with_path(&default_session_sqlite_path(), invoke, room_id)
}

/// 核心逻辑与库路径解耦，便于测试用临时库。
fn execute_with_path(
    path: &Path,
    invoke: &ToolInvoke,
    room_id: Option<&str>,
) -> Result<String, String> {
    // fail-closed：清单按 (房间, 会话) 归属，两者缺一即拒绝，绝不落到全局状态。
    let room_id = room_id.ok_or("任务清单需要聊天室归属，未写入")?;
    let session_id = invoke
        .session_id
        .as_deref()
        .ok_or("任务清单需要会话归属，未写入")?;
    if invoke.workspace_id != workspace_identity(&active_workspace_path()) {
        return Err("工程已切换，任务清单未更新".into());
    }
    let todos = tools::normalize_todo_write(&invoke.input)?;

    let mut connection = open_session_connection(path).map_err(|error| error.to_string())?;
    initialize_session_schema(&connection).map_err(|error| error.to_string())?;
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    // 房间必须真实存在：显式校验，不依赖外键开关状态。
    let room_exists: i64 = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_rooms WHERE id = ?1)",
            params![room_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if room_exists == 0 {
        return Err(format!("聊天室不存在：{room_id}"));
    }
    let old_revision: i64 = tx
        .query_row(
            "SELECT revision FROM chat_room_todos WHERE room_id = ?1 AND session_id = ?2",
            params![room_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(0);
    let new_revision = old_revision + 1;

    let all_done = todos
        .iter()
        .all(|todo| matches!(todo.status, tools::TodoStatus::Completed));
    if all_done {
        tx.execute(
            "DELETE FROM chat_room_todos WHERE room_id = ?1 AND session_id = ?2",
            params![room_id, session_id],
        )
        .map_err(|error| error.to_string())?;
    } else {
        let stored: Vec<StoredTodo> = todos
            .iter()
            .map(|todo| StoredTodo {
                content: todo.content.clone(),
                active_form: todo.active_form.clone(),
                status: serde_json::to_value(&todo.status)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "pending".to_string()),
                priority: None,
            })
            .collect();
        tx.execute(
            "INSERT INTO chat_room_todos(room_id, session_id, todos_json, source, revision, updated_at, last_call_id) \
             VALUES (?1, ?2, ?3, 'tool', ?4, ?5, ?6) \
             ON CONFLICT(room_id, session_id) DO UPDATE SET \
               todos_json = excluded.todos_json, source = excluded.source, \
               revision = excluded.revision, updated_at = excluded.updated_at, \
               last_call_id = excluded.last_call_id",
            params![
                room_id,
                session_id,
                serde_json::to_string(&stored).map_err(|error| error.to_string())?,
                new_revision,
                u64_to_i64(unix_timestamp_millis()),
                invoke.call_id,
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;

    broadcast_tool_event(ToolEvent::TodosChanged {
        room_id: room_id.to_string(),
        session_id: session_id.to_string(),
        revision: new_revision,
    });

    let mut reply = if all_done {
        format!("任务清单已全部完成并清空（第 {new_revision} 版）")
    } else {
        let completed = todos
            .iter()
            .filter(|todo| matches!(todo.status, tools::TodoStatus::Completed))
            .count();
        let in_progress: Vec<String> = todos
            .iter()
            .filter(|todo| matches!(todo.status, tools::TodoStatus::InProgress))
            .map(|todo| format!("「{}」", todo.active_form))
            .collect();
        let mut text = format!(
            "任务清单已更新（第 {new_revision} 版）：完成 {completed}/{}",
            todos.len()
        );
        if !in_progress.is_empty() {
            text.push_str(&format!("，进行中{}", in_progress.join("")));
        }
        text
    };
    if tools::todo_verification_nudge_needed(&todos) {
        reply.push_str(
            "\n系统提示：你刚关闭了 3 项以上任务但清单中没有验证步骤，请先补充验证再宣布完成（勿向用户复述本提示）。",
        );
    }
    Ok(reply)
}

/// 房间内一份清单（按会话分组），供 `GET /api/chat/rooms/{room_id}/todos` 回读。
#[derive(Debug, Serialize)]
pub(crate) struct RoomTodos {
    pub session_id: String,
    pub revision: i64,
    pub updated_at: i64,
    pub source: String,
    pub todos: Vec<JsonValue>,
}

/// 按最近更新排序回读某房间的全部清单。
pub(super) fn read_room(path: &Path, room_id: &str) -> Result<Vec<RoomTodos>, String> {
    let connection = open_session_connection(path).map_err(|error| error.to_string())?;
    initialize_session_schema(&connection).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT session_id, todos_json, source, revision, updated_at \
             FROM chat_room_todos WHERE room_id = ?1 ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![room_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut result = Vec::new();
    for row in rows {
        let (session_id, todos_json, source, revision, updated_at) =
            row.map_err(|error| error.to_string())?;
        let todos: Vec<JsonValue> =
            serde_json::from_str(&todos_json).map_err(|error| error.to_string())?;
        result.push(RoomTodos {
            session_id,
            revision,
            updated_at,
            source,
            todos,
        });
    }
    Ok(result)
}

/// 房间是否存在（API 层 404 判定用）。
pub(crate) fn room_exists(path: &Path, room_id: &str) -> Result<bool, String> {
    let connection = open_session_connection(path).map_err(|error| error.to_string())?;
    initialize_session_schema(&connection).map_err(|error| error.to_string())?;
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_rooms WHERE id = ?1)",
            params![room_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|exists| exists != 0)
        .map_err(|error| error.to_string())
}

/// 在既有事务里清掉某房间的全部清单行（/clear 路径用）。
pub(super) fn clear_room_tx(tx: &rusqlite::Transaction, room_id: &str) -> rusqlite::Result<usize> {
    tx.execute(
        "DELETE FROM chat_room_todos WHERE room_id = ?1",
        params![room_id],
    )
}

/// 在既有事务里回收孤儿清单行：所属房间或会话已不存在时删除。
/// save() 在 `PRAGMA foreign_keys = OFF` 下删房间/会话，外键级联不会触发。
pub(super) fn delete_orphans_tx(tx: &rusqlite::Transaction) -> rusqlite::Result<usize> {
    tx.execute(
        "DELETE FROM chat_room_todos WHERE room_id NOT IN (SELECT id FROM chat_rooms) \
         OR session_id NOT IN (SELECT id FROM sessions)",
        [],
    )
}

/// D5：请求组装层的任务清单快照。
///
/// 只拼进本轮 `system_prompt`，绝不写进 `visible_user_content` / 消息表，
/// 否则快照会被持久化进聊天记录并污染后续上下文。
pub(super) fn context_block(path: &Path, room_id: &str, session_id: &str) -> Option<String> {
    const MAX_ITEMS: usize = 20;
    const MAX_CHARS: usize = 1200;
    let connection = open_session_connection(path).ok()?;
    initialize_session_schema(&connection).ok()?;
    let row: Option<(String, i64)> = connection
        .query_row(
            "SELECT todos_json, revision FROM chat_room_todos \
             WHERE room_id = ?1 AND session_id = ?2",
            params![room_id, session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .ok()?;
    let (todos_json, revision) = row?;
    let items: Vec<JsonValue> = serde_json::from_str(&todos_json).ok()?;
    if items.is_empty() {
        return None;
    }
    let mut block = format!(
        "\n\n# 当前任务清单（第 {revision} 版，仅供续接参考，勿向用户原样复述）"
    );
    let mut included = 0usize;
    for item in items.iter() {
        if included >= MAX_ITEMS {
            break;
        }
        let status = item["status"].as_str().unwrap_or("pending");
        let content = item["content"].as_str().unwrap_or("");
        let active = item["activeForm"]
            .as_str()
            .or_else(|| item["active_form"].as_str())
            .unwrap_or("");
        let line = match status {
            "completed" => format!("- [x] {content}"),
            "in_progress" => format!("- [~] {content}（{active}）"),
            _ => format!("- [ ] {content}"),
        };
        // 为截断注记预留空间，保证总长不超过 MAX_CHARS。
        if block.chars().count() + 1 + line.chars().count() + 32 > MAX_CHARS {
            break;
        }
        block.push('\n');
        block.push_str(&line);
        included += 1;
    }
    if included == 0 {
        return None;
    }
    let omitted = items.len() - included;
    if omitted > 0 {
        block.push_str(&format!("\n…其余 {omitted} 项省略"));
    }
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db(dir: &Path) -> PathBuf {
        let path = dir.join("web-sessions.sqlite3");
        let connection = open_session_connection(&path).expect("open temp db");
        initialize_session_schema(&connection).expect("init schema");
        for room_id in ["room-a", "room-b"] {
            connection
                .execute(
                    "INSERT INTO chat_rooms(id, name, created_at, updated_at) VALUES (?1, ?1, 1, 1)",
                    params![room_id],
                )
                .expect("insert room");
        }
        for session_id in ["s-1", "s-2"] {
            connection
                .execute(
                    "INSERT INTO sessions(id, name, provider, model, api_key_ref, created_at, updated_at) \
                     VALUES (?1, ?1, 'p', 'm', 'k', 1, 1)",
                    params![session_id],
                )
                .expect("insert session");
        }
        drop(connection);
        path
    }

    fn invoke_for(session_id: Option<&str>, input: JsonValue) -> ToolInvoke {
        ToolInvoke {
            call_id: "call-t".into(),
            tool_name: "TodoWrite".into(),
            input,
            caller: ToolCaller::Llm,
            workspace_id: workspace_identity(&active_workspace_path()),
            session_id: session_id.map(str::to_string),
            user_authorized: false,
            user_confirmed_twice: false,
        }
    }

    fn two_todos(status: &str) -> JsonValue {
        json!({
            "todos": [
                {"content": "甲步骤", "activeForm": "做甲", "status": status},
                {"content": "乙步骤", "activeForm": "做乙", "status": "pending"}
            ]
        })
    }

    #[test]
    fn chat_todos_migration_is_idempotent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("db.sqlite3");
        let connection = open_session_connection(&path).expect("open");
        initialize_session_schema(&connection).expect("first init");
        initialize_session_schema(&connection).expect("second init must be idempotent");
        connection
            .prepare(
                "SELECT room_id,session_id,todos_json,revision FROM chat_room_todos LIMIT 0",
            )
            .expect("table must exist");
    }

    #[test]
    fn chat_todos_execute_fail_closed_without_room_or_session() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());
        let invoke = invoke_for(Some("s-1"), two_todos("in_progress"));
        let error =
            execute_with_path(&path, &invoke, None).expect_err("无房间必须 fail-closed");
        assert!(error.contains("需要聊天室归属"), "{error}");
        let no_session = invoke_for(None, two_todos("in_progress"));
        let error =
            execute_with_path(&path, &no_session, Some("room-a")).expect_err("无会话必须 fail-closed");
        assert!(error.contains("需要会话归属"), "{error}");
    }

    #[test]
    fn chat_todos_execute_rejects_missing_room() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());
        let invoke = invoke_for(Some("s-1"), two_todos("in_progress"));
        let error = execute_with_path(&path, &invoke, Some("room-missing"))
            .expect_err("missing room must fail");
        assert!(error.contains("聊天室不存在"), "错误须点名房间：{error}");
    }

    #[test]
    fn chat_todos_isolation_revision_and_all_done_delete() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());

        let invoke = invoke_for(Some("s-1"), two_todos("in_progress"));
        let first = execute_with_path(&path, &invoke, Some("room-a")).expect("write room-a");
        assert!(first.contains("第 1 版"), "首版文案：{first}");
        let second = execute_with_path(&path, &invoke, Some("room-a")).expect("bump revision");
        assert!(second.contains("第 2 版"), "递增文案：{second}");
        assert!(second.contains("完成 0/2"), "进度文案：{second}");
        assert!(second.contains("「做甲」"), "进行中 activeForm：{second}");

        // 同房间另一会话、另一房间互不影响。
        execute_with_path(&path, &invoke_for(Some("s-2"), two_todos("in_progress")), Some("room-a"))
            .expect("room-a s-2");
        execute_with_path(&path, &invoke_for(Some("s-1"), two_todos("in_progress")), Some("room-b"))
            .expect("room-b s-1");
        let room_a = read_room(&path, "room-a").expect("read room-a");
        assert_eq!(room_a.len(), 2);
        let revisions: std::collections::BTreeMap<_, _> = room_a
            .iter()
            .map(|row| (row.session_id.as_str(), row.revision))
            .collect();
        assert_eq!(revisions["s-1"], 2);
        assert_eq!(revisions["s-2"], 1);
        // 落库项用驼峰 activeForm。
        let s1 = room_a
            .iter()
            .find(|row| row.session_id == "s-1")
            .expect("s-1 row");
        assert!(s1.todos[0].get("activeForm").is_some(), "stored: {:?}", s1.todos);
        assert_eq!(read_room(&path, "room-b").expect("read room-b").len(), 1);

        // 全部完成 → 行删除 + nudge（≥3 项且无验证步骤）。
        let done = json!({
            "todos": [
                {"content": "甲", "activeForm": "做甲", "status": "completed"},
                {"content": "乙", "activeForm": "做乙", "status": "completed"},
                {"content": "丙", "activeForm": "做丙", "status": "completed"}
            ]
        });
        let text = execute_with_path(&path, &invoke_for(Some("s-1"), done), Some("room-a"))
            .expect("all done");
        assert!(text.contains("全部完成并清空（第 3 版）"), "清空文案：{text}");
        assert!(text.contains("没有验证步骤"), "nudge 文案：{text}");
        let room_a = read_room(&path, "room-a").expect("read room-a after clear");
        assert_eq!(room_a.len(), 1, "s-1 行已删除，s-2 保留");
        assert_eq!(room_a[0].session_id, "s-2");

        // 含中文"验证"的完成清单不触发 nudge。
        let verified = json!({
            "todos": [
                {"content": "甲", "activeForm": "做甲", "status": "completed"},
                {"content": "乙", "activeForm": "做乙", "status": "completed"},
                {"content": "运行验证", "activeForm": "跑验证", "status": "completed"}
            ]
        });
        let text =
            execute_with_path(&path, &invoke_for(Some("s-2"), verified), Some("room-a"))
                .expect("done");
        assert!(!text.contains("没有验证步骤"), "验证步骤不应 nudge：{text}");
    }

    #[test]
    fn chat_todos_rejects_over_limit_and_bad_input() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());
        let too_many: Vec<JsonValue> = (0..31)
            .map(|i| json!({"content": format!("步{i}"), "activeForm": "做", "status": "pending"}))
            .collect();
        let invoke = invoke_for(Some("s-1"), json!({"todos": too_many}));
        assert!(execute_with_path(&path, &invoke, Some("room-a")).is_err());
        let bad = invoke_for(Some("s-1"), json!({"todos": []}));
        assert!(execute_with_path(&path, &bad, Some("room-a")).is_err());
    }

    #[test]
    fn chat_todos_context_block_format_and_truncation() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());
        assert!(context_block(&path, "room-a", "s-1").is_none(), "无清单无快照");

        let input = json!({
            "todos": [
                {"content": "已完成项", "activeForm": "收尾", "status": "completed"},
                {"content": "进行项", "activeForm": "正在做", "status": "in_progress"},
                {"content": "待办项", "activeForm": "等开工", "status": "pending"}
            ]
        });
        execute_with_path(&path, &invoke_for(Some("s-1"), input), Some("room-a")).expect("write");
        let block = context_block(&path, "room-a", "s-1").expect("snapshot");
        assert!(block.starts_with("\n\n# 当前任务清单（第 1 版"), "{block}");
        assert!(block.contains("勿向用户原样复述"), "{block}");
        assert!(block.contains("- [x] 已完成项"), "{block}");
        assert!(block.contains("- [~] 进行项（正在做）"), "{block}");
        assert!(block.contains("- [ ] 待办项"), "{block}");
        // 别的会话/房间看不到这份清单。
        assert!(context_block(&path, "room-a", "s-2").is_none());
        assert!(context_block(&path, "room-b", "s-1").is_none());

        // 长清单截断：25 项 × 长 content，总长 ≤1200 且带"其余 k 项省略"。
        let long: Vec<JsonValue> = (0..25)
            .map(|i| json!({
                "content": format!("第{i:02}项{}", "内容".repeat(30)),
                "activeForm": "执行中",
                "status": "pending"
            }))
            .collect();
        execute_with_path(&path, &invoke_for(Some("s-1"), json!({"todos": long})), Some("room-a"))
            .expect("write long");
        let block = context_block(&path, "room-a", "s-1").expect("truncated snapshot");
        assert!(block.chars().count() <= 1200, "长度超限：{}", block.chars().count());
        assert!(block.contains("其余"), "截断注记：{block}");
        assert!(block.contains("省略"), "截断注记：{block}");
    }

    #[test]
    fn chat_todos_clear_and_delete_room_cleanup() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());
        execute_with_path(&path, &invoke_for(Some("s-1"), two_todos("in_progress")), Some("room-a"))
            .expect("write");
        assert_eq!(read_room(&path, "room-a").expect("read").len(), 1);

        // /clear 同事务清理。
        let mut connection = open_session_connection(&path).expect("open");
        {
            let tx = connection.transaction().expect("tx");
            clear_room_tx(&tx, "room-a").expect("clear_room_tx");
            tx.commit().expect("commit");
        }
        assert!(read_room(&path, "room-a").expect("read").is_empty());
        drop(connection);

        // 重新写入后删房间同清（同事务显式删除 + 外键兜底）。
        execute_with_path(&path, &invoke_for(Some("s-1"), two_todos("in_progress")), Some("room-a"))
            .expect("write again");
        let attachment_dir = temp.path().join("attachments");
        std::fs::create_dir_all(&attachment_dir).expect("attachment dir");
        delete_chat_room_sqlite(
            &path,
            "room-a",
            &std::collections::BTreeSet::new(),
            None,
            &attachment_dir,
        )
        .expect("delete room");
        assert!(read_room(&path, "room-a").expect("read").is_empty());
    }

    #[test]
    fn chat_todos_required_permission_is_read_only() {
        assert_eq!(
            required_permission_for_tool("TodoWrite"),
            PermissionMode::ReadOnly,
            "TodoWrite 只写宿主元数据，必须按只读档开放"
        );
    }

    #[test]
    fn chat_todos_readonly_room_executes_through_runtime() {
        // 验收关键：只读授权（无 session grant、房间非 full-access）下 TodoWrite
        // 经 execute_runtime_tool_blocking_with_executor 自动放行且行真实写入。
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let _db = crate::tests::scoped_session_db_env(temp.path());
        let path = setup_db(temp.path());
        // 钉住"宿主执行不再落 CLI 文件"：若误分发到 tools::execute_tool，
        // 这里指向的 CLAW_TODO_STORE 文件就会被写出来。
        let claw_store = temp.path().join("claw-todos.json");
        let _claw_env = crate::test_env::set("CLAW_TODO_STORE", Some(&claw_store));

        let invoke = invoke_for(Some("s-1"), two_todos("in_progress"));
        let exec = RegistryExecutor::for_room(Some("room-a".to_string()));
        let outcome = execute_runtime_tool_blocking_with_executor(
            invoke,
            temp.path().to_path_buf(),
            Some("room-a".to_string()),
            None,
            &exec,
        );
        assert_eq!(outcome.status, ToolOutcomeStatus::Ok, "回执：{outcome:?}");
        assert_eq!(
            outcome.permission_gate.decision,
            PermissionDecision::AllowAuto,
            "只读元数据工具不得弹审批"
        );
        assert_eq!(read_room(&path, "room-a").expect("read").len(), 1);
        assert!(
            !claw_store.exists(),
            "宿主执行不得写 CLI 文件存储：{}",
            claw_store.display()
        );
    }

    #[test]
    fn chat_todos_orphan_cleanup_keeps_live_rows() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = setup_db(temp.path());
        execute_with_path(&path, &invoke_for(Some("s-1"), two_todos("in_progress")), Some("room-a"))
            .expect("s-1");
        execute_with_path(&path, &invoke_for(Some("s-2"), two_todos("in_progress")), Some("room-a"))
            .expect("s-2");

        // 模拟 save()：FK OFF 下删会话行，孤儿清理只回收 s-1，存活行保留。
        let mut connection = open_session_connection(&path).expect("open");
        connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("fk off");
        {
            let tx = connection.transaction().expect("tx");
            tx.execute("DELETE FROM sessions WHERE id = 's-1'", [])
                .expect("drop session row");
            delete_orphans_tx(&tx).expect("orphan cleanup");
            tx.commit().expect("commit");
        }
        let rows = read_room(&path, "room-a").expect("read");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, "s-2");
    }

    fn has_todo_write(defs: &[ToolDefinition]) -> bool {
        defs.iter().any(|definition| definition.name == "TodoWrite")
    }

    #[test]
    fn chat_todos_exposure_matrix() {
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let _db = crate::tests::scoped_session_db_env(temp.path());
        setup_db(temp.path());
        {
            let mut config = workspace_config().lock().expect("config");
            config.model.enable_llm_tools = true;
        }
        let defs_for = |session: &str, room: Option<&str>| {
            llm_tool_definitions_for_session(session, room).unwrap_or_default()
        };
        // 有房间（默认 workspace-write 档；宿主执行元数据与档位无关，只读档同样开放）。
        let defs = defs_for("s-1", Some("room-a"));
        assert!(has_todo_write(&defs), "有房间须暴露");
        // mvp 版不能泄漏：同名条目只能出现一次（宿主定义）。
        assert_eq!(
            defs.iter().filter(|d| d.name == "TodoWrite").count(),
            1,
            "TodoWrite 只能由宿主定义提供"
        );
        // 无房间：不含（执行器 fail-closed，不暴露）。
        assert!(!has_todo_write(&defs_for("s-1", None)), "无房间不得暴露");
        // dispatch-only：不含。
        {
            let mut config = workspace_config().lock().expect("config");
            config
                .session_model_limits
                .entry("s-1".to_string())
                .or_default()
                .llm_tool_exposure = Some("dispatch-only".to_string());
        }
        assert!(
            !has_todo_write(&defs_for("s-1", Some("room-a"))),
            "dispatch-only 不得暴露"
        );
        // 会话 allowlist 不含 TodoWrite：不含。
        {
            let mut config = workspace_config().lock().expect("config");
            let entry = config
                .session_model_limits
                .entry("s-1".to_string())
                .or_default();
            entry.llm_tool_exposure = None;
            entry.tool_allowlist = Some(vec!["read_file".to_string()]);
        }
        assert!(
            !has_todo_write(&defs_for("s-1", Some("room-a"))),
            "allowlist 排除时不得暴露"
        );
    }

    #[test]
    fn chat_todos_policy_clause_only_when_exposed() {
        let defs = vec![definition()];
        let text = request_tool_policy_instruction(Some(&defs));
        assert!(text.contains("TodoWrite 用法"), "{text}");
        let defs = vec![semantic_dispatch_tool_definition()];
        let text = request_tool_policy_instruction(Some(&defs));
        assert!(!text.contains("TodoWrite 用法"), "{text}");
        let text = request_tool_policy_instruction(None);
        assert!(!text.contains("TodoWrite 用法"), "{text}");
    }

    #[test]
    fn chat_todos_context_block_enters_request_not_messages() {
        // D5：快照只进本轮 request.system，不落库、不进消息内容。
        let _guard = crate::tests::config_test_guard();
        let temp = tempfile::tempdir().expect("tempdir");
        let _db = crate::tests::scoped_session_db_env(temp.path());
        let path = setup_db(temp.path());
        {
            let mut config = workspace_config().lock().expect("config");
            config.model.enable_llm_tools = true;
        }
        execute_with_path(&path, &invoke_for(Some("s-1"), two_todos("in_progress")), Some("room-a"))
            .expect("write");

        let mut agent = crate::tests::context_test_agent();
        agent.id = "s-1".to_string();
        let request = agent_message_request_with_context_messages_for_room(
            &agent,
            false,
            "基础系统提示",
            vec![],
            Some("room-a"),
        );
        let system = request.system.clone().unwrap_or_default();
        assert!(system.contains("# 当前任务清单（第 1 版"), "快照须进 system：{system}");
        // ContextAssembly 式的 system_prompt 本体不被污染——块只附加在请求层，
        // 因此 system 里先有原提示再带快照。
        assert!(system.starts_with("基础系统提示"), "{system}");
        assert!(
            request.messages.iter().all(|message| {
                !serde_json::to_string(&message.content)
                    .unwrap_or_default()
                    .contains("当前任务清单")
            }),
            "快照不得进入消息内容"
        );
        // 库里的房间消息不得出现快照文本。
        let connection = open_session_connection(&path).expect("open");
        let persisted: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM chat_room_messages WHERE content LIKE '%当前任务清单%'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(persisted, 0, "快照不得持久化进 chat_room_messages");
        // 无房间时也不附加快照。
        let request = agent_message_request_with_context_messages_for_room(
            &agent,
            false,
            "基础系统提示",
            vec![],
            None,
        );
        let system = request.system.unwrap_or_default();
        assert!(!system.contains("当前任务清单"), "{system}");
    }
}
