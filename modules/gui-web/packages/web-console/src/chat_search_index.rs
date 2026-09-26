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
            CREATE TRIGGER IF NOT EXISTS chat_search_insert_v1 AFTER INSERT ON chat_room_messages BEGIN
                INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(new.rowid); END;
            CREATE TRIGGER IF NOT EXISTS chat_search_update_v1 AFTER UPDATE ON chat_room_messages
                WHEN old.rowid IS NOT new.rowid OR old.content IS NOT new.content OR old.author IS NOT new.author OR old.role IS NOT new.role OR old.kind IS NOT new.kind BEGIN
                INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(old.rowid);
                INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(new.rowid); END;
            CREATE TRIGGER IF NOT EXISTS chat_search_delete_v1 AFTER DELETE ON chat_room_messages BEGIN
                INSERT OR IGNORE INTO chat_search_dirty_v1 VALUES(old.rowid); END;
            INSERT OR IGNORE INTO chat_search_dirty_v1 SELECT rowid FROM chat_room_messages;")?;
    }
    Ok(())
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
