//! 搜索派生索引：消息表唯一真值，普通 dirty 队列隔离消息写入与 FTS 能力。
use super::*;

// 冷索引分批提交，单次写锁只覆盖 500 行，不挡住正常会话提交数秒。
pub(super) fn prewarm(connection: &Connection) -> rusqlite::Result<bool> {
    {
        let transaction=connection.unchecked_transaction()?;
        initialize(&transaction)?;
        transaction.commit()?;
    }
    for _ in 0..256 {
        let transaction=connection.unchecked_transaction()?;
        let complete=refresh_batch(&transaction)?;
        transaction.commit()?;
        if complete { return Ok(true); }
        std::thread::yield_now();
    }
    // 超大积压本次仍完整扫描，后续搜索继续派生索引；绝不把部分索引当全库。
    Ok(false)
}

pub(super) fn prepare(connection: &Connection) -> rusqlite::Result<bool> {
    connection.execute_batch("SAVEPOINT chat_search_refresh")?;
    match refresh_batch(connection) {
        Ok(complete) => { connection.execute_batch("RELEASE chat_search_refresh")?; Ok(complete) }
        Err(error) => {
            let _=connection.execute_batch("ROLLBACK TO chat_search_refresh; RELEASE chat_search_refresh");
            Err(error)
        }
    }
}

fn initialize(connection: &Connection) -> rusqlite::Result<()> {
    // 触发器只使用 SQLite 内建普通表操作，不要求所有写连接注册函数或支持 FTS。
    let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='chat_search_fts_v1')", [], |row| row.get(0))?;
    if !exists {
        connection.execute_batch("CREATE VIRTUAL TABLE chat_search_fts_v1 USING fts5(content, author, tokenize='trigram case_sensitive 1');
            CREATE TABLE IF NOT EXISTS chat_search_dirty_v1 (source_rowid INTEGER PRIMARY KEY);
            INSERT OR IGNORE INTO chat_search_dirty_v1 SELECT rowid FROM chat_room_messages;")?;
    }
    install_triggers(connection)
}

// 外层 UPSERT 走到 DO UPDATE 分支时，触发器内的 INSERT OR IGNORE 仍会因唯一键冲突
// 中断整笔写入；改为守卫式 INSERT ... WHERE NOT EXISTS 后两条路径都安全。
// 每次初始化都重建为同一形态，已有旧触发器的库幂等升级，不清空 dirty/FTS/消息。
fn install_triggers(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("DROP TRIGGER IF EXISTS chat_search_insert_v1;
        CREATE TRIGGER chat_search_insert_v1 AFTER INSERT ON chat_room_messages BEGIN
            INSERT INTO chat_search_dirty_v1 SELECT new.rowid
                WHERE NOT EXISTS(SELECT 1 FROM chat_search_dirty_v1 WHERE source_rowid=new.rowid); END;
        DROP TRIGGER IF EXISTS chat_search_update_v1;
        CREATE TRIGGER chat_search_update_v1 AFTER UPDATE ON chat_room_messages
            WHEN old.rowid IS NOT new.rowid OR old.content IS NOT new.content OR old.author IS NOT new.author OR old.role IS NOT new.role OR old.kind IS NOT new.kind BEGIN
            INSERT INTO chat_search_dirty_v1 SELECT old.rowid
                WHERE NOT EXISTS(SELECT 1 FROM chat_search_dirty_v1 WHERE source_rowid=old.rowid);
            INSERT INTO chat_search_dirty_v1 SELECT new.rowid
                WHERE NOT EXISTS(SELECT 1 FROM chat_search_dirty_v1 WHERE source_rowid=new.rowid); END;
        DROP TRIGGER IF EXISTS chat_search_delete_v1;
        CREATE TRIGGER chat_search_delete_v1 AFTER DELETE ON chat_room_messages BEGIN
            INSERT INTO chat_search_dirty_v1 SELECT old.rowid
                WHERE NOT EXISTS(SELECT 1 FROM chat_search_dirty_v1 WHERE source_rowid=old.rowid); END;")
}

fn refresh_batch(connection: &Connection) -> rusqlite::Result<bool> {
    let dirty={
        let mut statement=connection.prepare("SELECT source_rowid FROM chat_search_dirty_v1 LIMIT 500")?;
        let rows=statement.query_map([],|row| row.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if dirty.is_empty() { return Ok(true); }
    let mut read=connection.prepare(&format!("SELECT content,author FROM chat_room_messages WHERE rowid=?1 AND {VISIBLE_SQL}"))?;
    let mut remove=connection.prepare("DELETE FROM chat_search_fts_v1 WHERE rowid=?1")?;
    let mut insert=connection.prepare("INSERT INTO chat_search_fts_v1(rowid,content,author) VALUES(?1,?2,?3)")?;
    let mut clean=connection.prepare("DELETE FROM chat_search_dirty_v1 WHERE source_rowid=?1")?;
    for rowid in dirty {
        remove.execute(params![rowid])?;
        let source=read.query_row(params![rowid],|row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional()?;
        if let Some((content,author))=source { insert.execute(params![rowid,content.to_lowercase(),author.to_lowercase()])?; }
        clean.execute(params![rowid])?;
    }
    connection.query_row("SELECT NOT EXISTS(SELECT 1 FROM chat_search_dirty_v1)",[],|row| row.get(0))
}

pub(super) fn phrase(needle: &str) -> Option<String> {
    // Trigram 不索引不足三字符的搜索，NUL 也不能作为 FTS 查询词。保留完整扫描路径。
    if needle.chars().count()<3 || needle.contains('\0') { return None; }
    Some(format!("\"{}\"", needle.replace('"',"\"\"")))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 预修复版本的库形态：脏队列与 OR-IGNORE 触发器，验证既有库幂等升级。
    const LEGACY_SCHEMA: &str = "CREATE VIRTUAL TABLE chat_search_fts_v1 USING fts5(content, author, tokenize='trigram case_sensitive 1');
        CREATE TABLE chat_search_dirty_v1 (source_rowid INTEGER PRIMARY KEY);
        CREATE TRIGGER chat_search_insert_v1 AFTER INSERT ON chat_room_messages BEGIN
            INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(new.rowid); END;
        CREATE TRIGGER chat_search_update_v1 AFTER UPDATE ON chat_room_messages
            WHEN old.rowid IS NOT new.rowid OR old.content IS NOT new.content OR old.author IS NOT new.author OR old.role IS NOT new.role OR old.kind IS NOT new.kind BEGIN
            INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(old.rowid);
            INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(new.rowid); END;
        CREATE TRIGGER chat_search_delete_v1 AFTER DELETE ON chat_room_messages BEGIN
            INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(old.rowid); END;";

    fn fixture() -> (tempfile::TempDir, Connection) {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("search.sqlite3")).unwrap();
        connection.execute_batch("CREATE TABLE chat_room_messages(room_id TEXT NOT NULL, id TEXT NOT NULL,
            author TEXT NOT NULL DEFAULT '', role TEXT NOT NULL DEFAULT '',
            target TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '',
            kind TEXT NOT NULL DEFAULT '', attachments_json TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(room_id, id));").unwrap();
        (directory, connection)
    }

    // 与消息保存路径一致的 UPSERT 形态：已有行进入 DO UPDATE 分支。
    fn upsert(connection: &Connection, content: &str) -> rusqlite::Result<usize> {
        connection.execute("INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at)
            VALUES('a','m1','助手','assistant','',?1,'assistant-partial','[]',1)
            ON CONFLICT(room_id,id) DO UPDATE SET author=excluded.author, role=excluded.role,
                target=excluded.target, content=excluded.content, kind=excluded.kind,
                attachments_json=excluded.attachments_json, created_at=excluded.created_at",
            params![content])
    }

    fn dirty(connection: &Connection) -> Vec<i64> {
        connection.prepare("SELECT source_rowid FROM chat_search_dirty_v1 ORDER BY source_rowid").unwrap()
            .query_map([], |row| row.get::<_, i64>(0)).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap()
    }

    #[test]
    fn upsert_update_arm_keeps_single_dirty_row() {
        let (_directory, connection) = fixture();
        initialize(&connection).unwrap();
        upsert(&connection, "初稿").unwrap();
        let rowid: i64 = connection.query_row("SELECT rowid FROM chat_room_messages WHERE id='m1'", [], |row| row.get(0)).unwrap();
        assert_eq!(dirty(&connection), vec![rowid]);
        // 连续两次运行中进度保存：脏行已存在时 DO UPDATE 不得再因唯一键冲突回滚。
        upsert(&connection, "二稿").unwrap();
        upsert(&connection, "三稿").unwrap();
        assert_eq!(dirty(&connection), vec![rowid]);
        assert_eq!(connection.query_row::<String, _, _>(
            "SELECT content FROM chat_room_messages WHERE id='m1'", [], |row| row.get(0)).unwrap(), "三稿");
    }

    #[test]
    fn initialize_rebuilds_legacy_or_ignore_triggers() {
        let (_directory, connection) = fixture();
        connection.execute_batch(LEGACY_SCHEMA).unwrap();
        upsert(&connection, "旧一稿").unwrap();
        // 幂等升级：重复 initialize 后 DO UPDATE 不再冲突，dirty/FTS 不被清空。
        initialize(&connection).unwrap();
        initialize(&connection).unwrap();
        upsert(&connection, "新二稿").unwrap();
        upsert(&connection, "新三稿").unwrap();
        let rowid: i64 = connection.query_row("SELECT rowid FROM chat_room_messages WHERE id='m1'", [], |row| row.get(0)).unwrap();
        assert_eq!(dirty(&connection), vec![rowid]);
        // 刷新后 FTS 恰有一行最新内容，不重复、不丢失。
        prepare(&connection).unwrap();
        prepare(&connection).unwrap();
        assert!(dirty(&connection).is_empty());
        assert_eq!(connection.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM chat_search_fts_v1", [], |row| row.get(0)).unwrap(), 1);
        assert_eq!(connection.query_row::<String, _, _>(
            "SELECT content FROM chat_search_fts_v1", [], |row| row.get(0)).unwrap(), "新三稿");
    }

    #[test]
    fn concurrent_progress_upserts_keep_single_dirty_row_and_room_isolation() {
        let (directory, connection) = fixture();
        initialize(&connection).unwrap();
        let db = directory.path().join("search.sqlite3");
        // 两条独立连接同一时刻并发写同一 (room,id) 的运行中 UPSERT；busy_timeout 有界，不重试。
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            for writer in ["并发稿甲", "并发稿乙"] {
                // move 只移本次循环的 writer 与共享 db/barrier 的引用；共享值本身留在外层。
                let db = &db;
                let barrier = &barrier;
                scope.spawn(move || {
                    let writer_connection = Connection::open(db).unwrap();
                    writer_connection.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                    barrier.wait();
                    for pass in 0..8 {
                        upsert(&writer_connection, &format!("{writer}-{pass}")).unwrap();
                    }
                });
            }
            barrier.wait();
        });
        let (rowid, content): (i64, String) = connection.query_row(
            "SELECT rowid, content FROM chat_room_messages WHERE room_id='a' AND id='m1'",
            [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert!(content == "并发稿甲-7" || content == "并发稿乙-7",
            "并发写必须落为其中一方的最终稿，实得 {content}");
        assert_eq!(dirty(&connection), vec![rowid], "并发写不得重复登记同一 rowid");
        // 真实刷新路径：FTS 恰索引最终内容一行，不丢不重。
        prepare(&connection).unwrap();
        prepare(&connection).unwrap();
        assert!(dirty(&connection).is_empty());
        assert_eq!(connection.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM chat_search_fts_v1", [], |row| row.get(0)).unwrap(), 1);
        assert_eq!(connection.query_row::<String, _, _>(
            "SELECT content FROM chat_search_fts_v1", [], |row| row.get(0)).unwrap(), content.to_lowercase());
        // 第二房间同词搜索必须为零命中（房间隔离，非镜像实现）。
        connection.execute(
            "INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at)
             VALUES('b','n1','助手','assistant','','无关内容','assistant-message','[]',2)", []).unwrap();
        prepare(&connection).unwrap();
        let term = phrase("并发稿").unwrap();
        let hits = |room: &str| -> i64 {
            connection.query_row(
                "SELECT COUNT(*) FROM chat_search_fts_v1 JOIN chat_room_messages m
                 ON m.rowid=chat_search_fts_v1.rowid
                 WHERE chat_search_fts_v1 MATCH ?1 AND m.room_id=?2",
                params![term, room], |row| row.get(0)).unwrap()
        };
        assert_eq!(hits("a"), 1);
        assert_eq!(hits("b"), 0);
    }
}
