//! M1 跨轮增量折叠：FoldBlock 持久化、折叠水位、heuristic-v2 摘要与回合后折叠决策。
//!
//! 只写摘要块，原始 chat_room_messages 不删除；块冻结后不变，下一轮装配按水位跳过
//! 已折叠区间，保住跨轮前缀。floor 记录生成时的 context_reset_at，reset/rollback 后
//! 旧块自动失效。首期仅 HTTP/web-console Room 作用域（装配入口按 backend 门禁）。

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

use crate::{
    attachment_text, chat_tool_history, estimate_bead_tokens, estimate_message_tokens,
    is_goal_temporary_persisted_message, strip_context_usage_footer, unix_timestamp_millis,
    PersistedChatMessage,
};

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub(crate) const SCOPE_KIND_ROOM: &str = "room";
const GENERATOR_HEURISTIC_V2: &str = "heuristic-v2";
/// 单块摘要目标规模（约 150–300 tokens 的字符上界）。
const SUMMARY_CHAR_BUDGET: usize = 1400;
/// 块摘要注入 system prompt 的最大 token 占比（相对历史预算）。
const BLOCK_TOTAL_PERCENT: u32 = 15;

/// 一个冻结的折叠块：一段摘要 + 其覆盖的连续消息区间（含两端）。
#[derive(Debug, Clone)]
pub(crate) struct FoldBlock {
    pub(crate) seq: u32,
    pub(crate) first_key: (u64, String),
    pub(crate) last_key: (u64, String),
    pub(crate) range_message_count: u32,
    pub(crate) range_token_estimate: u32,
    pub(crate) summary: String,
    pub(crate) token_count: u32,
    pub(crate) floor: u64,
    pub(crate) created_at: u64,
}

pub(crate) fn apply_session_migration_v32(connection: &Connection) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS context_fold_blocks (
             scope_kind          TEXT NOT NULL,
             scope_id            TEXT NOT NULL,
             session_id          TEXT NOT NULL,
             seq                 INTEGER NOT NULL,
             first_created_at    INTEGER NOT NULL,
             first_id            TEXT NOT NULL,
             last_created_at     INTEGER NOT NULL,
             last_id             TEXT NOT NULL,
             range_message_count INTEGER NOT NULL,
             range_token_estimate INTEGER NOT NULL,
             summary             TEXT NOT NULL,
             token_count         INTEGER NOT NULL,
             tier                INTEGER NOT NULL DEFAULT 1,
             generator           TEXT NOT NULL,
             floor               INTEGER NOT NULL DEFAULT 0,
             created_at          INTEGER NOT NULL,
             PRIMARY KEY (scope_kind, scope_id, session_id, seq)
         );
         CREATE INDEX IF NOT EXISTS idx_context_fold_blocks_scope
             ON context_fold_blocks(scope_kind, scope_id, session_id, floor);",
    )?;
    if current < 32 {
        connection.execute_batch("PRAGMA user_version = 32;")?;
    }
    Ok(())
}

fn block_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FoldBlock> {
    Ok(FoldBlock {
        seq: row.get::<_, i64>(0)? as u32,
        first_key: (crate::i64_to_u64(row.get::<_, i64>(1)?), row.get::<_, String>(2)?),
        last_key: (crate::i64_to_u64(row.get::<_, i64>(3)?), row.get::<_, String>(4)?),
        range_message_count: row.get::<_, i64>(5)? as u32,
        range_token_estimate: row.get::<_, i64>(6)? as u32,
        summary: row.get(7)?,
        token_count: row.get::<_, i64>(8)? as u32,
        floor: crate::i64_to_u64(row.get::<_, i64>(9)?),
        created_at: crate::i64_to_u64(row.get::<_, i64>(10)?),
    })
}

pub(crate) fn insert_block(
    connection: &Connection,
    room_id: &str,
    session_id: &str,
    block: &FoldBlock,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO context_fold_blocks(
             scope_kind, scope_id, session_id, seq,
             first_created_at, first_id, last_created_at, last_id,
             range_message_count, range_token_estimate, summary, token_count,
             tier, generator, floor, created_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,1,?13,?14,?15)",
        params![
            SCOPE_KIND_ROOM,
            room_id,
            session_id,
            block.seq as i64,
            crate::u64_to_i64(block.first_key.0),
            block.first_key.1,
            crate::u64_to_i64(block.last_key.0),
            block.last_key.1,
            block.range_message_count as i64,
            block.range_token_estimate as i64,
            block.summary,
            block.token_count as i64,
            GENERATOR_HEURISTIC_V2,
            crate::u64_to_i64(block.floor),
            crate::u64_to_i64(block.created_at),
        ],
    )?;
    Ok(())
}

/// 加载某 (room, session) 在当前 floor 下有效的全部块（按 seq 正序）。
/// floor 传生成时的 context_reset_at 原值（0 表示无 floor）。
pub(crate) fn load_blocks(
    connection: &Connection,
    room_id: &str,
    session_id: &str,
    floor: u64,
) -> rusqlite::Result<Vec<FoldBlock>> {
    let mut statement = connection.prepare(
        "SELECT seq, first_created_at, first_id, last_created_at, last_id,
                range_message_count, range_token_estimate, summary, token_count, floor, created_at
         FROM context_fold_blocks
         WHERE scope_kind=?1 AND scope_id=?2 AND session_id=?3 AND floor=?4
         ORDER BY seq",
    )?;
    let rows = statement
        .query_map(
            params![SCOPE_KIND_ROOM, room_id, session_id, crate::u64_to_i64(floor)],
            block_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 装配读路径的容错入口：库打不开/查询失败一律返回空块集，绝不阻断本轮装配。
pub(crate) fn load_blocks_from_path(
    path: &Path,
    room_id: &str,
    session_id: &str,
    floor: u64,
) -> Vec<FoldBlock> {
    let Ok(connection) = crate::open_session_connection(path) else {
        return Vec::new();
    };
    load_blocks(&connection, room_id, session_id, floor).unwrap_or_default()
}

/// 折叠水位：最后一个块的 last_key；其之前（含）的消息不再进入模型上下文。
pub(crate) fn watermark(blocks: &[FoldBlock]) -> Option<(u64, String)> {
    blocks.last().map(|block| block.last_key.clone())
}

/// 消息是否落在水位内（含端点）。与房间排序键 (created_at, id) 一致。
pub(crate) fn covered_by_watermark(message: &PersistedChatMessage, mark: &(u64, String)) -> bool {
    message.created_at < mark.0 || (message.created_at == mark.0 && message.id <= mark.1)
}

/// 块摘要按 seq 顺序拼成一段，注入 system prompt（每块自带免责头）。
pub(crate) fn render_blocks(blocks: &[FoldBlock]) -> String {
    blocks
        .iter()
        .map(|block| block.summary.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn total_token_count(blocks: &[FoldBlock]) -> u32 {
    blocks.iter().map(|block| block.token_count).sum()
}

/// 删除某 room 作用域的全部块（清空房间 / 删除房间）。
pub(crate) fn delete_scope(connection: &Connection, room_id: &str) -> rusqlite::Result<usize> {
    connection.execute(
        "DELETE FROM context_fold_blocks WHERE scope_kind=?1 AND scope_id=?2",
        params![SCOPE_KIND_ROOM, room_id],
    )
}

/// 删除某会话的全部块（删除会话 / fallback 重置）。
pub(crate) fn delete_for_session(
    connection: &Connection,
    room_id: &str,
    session_id: &str,
) -> rusqlite::Result<usize> {
    connection.execute(
        "DELETE FROM context_fold_blocks WHERE scope_kind=?1 AND scope_id=?2 AND session_id=?3",
        params![SCOPE_KIND_ROOM, room_id, session_id],
    )
}

/// 孤儿块清扫：房间或会话已不存在时一并删除（remove_orphans 同事务）。
pub(crate) fn delete_orphans(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<usize> {
    tx.execute(
        "DELETE FROM context_fold_blocks
         WHERE (scope_kind='room' AND scope_id NOT IN (SELECT id FROM chat_rooms))
            OR session_id NOT IN (SELECT id FROM sessions)",
        [],
    )
}

/// 删除范围内命中消息所在的块及其后全部块（同 session、同 scope 的 seq >= 命中块）。
/// 命中判定：first_key <= (created_at, message_id) <= last_key。
pub(crate) fn invalidate_covering_message(
    tx: &rusqlite::Transaction<'_>,
    room_id: &str,
    message_id: &str,
) -> rusqlite::Result<()> {
    let key = tx
        .query_row(
            "SELECT created_at FROM chat_room_messages WHERE room_id=?1 AND id=?2",
            params![room_id, message_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(created_at) = key else {
        return Ok(());
    };
    // 找覆盖该 key 的块（同一房间可能多个 session 各有块链，逐 session 作废）。
    let hits: Vec<(String, i64)> = {
        let mut statement = tx.prepare(
            "SELECT session_id, seq FROM context_fold_blocks
             WHERE scope_kind=?1 AND scope_id=?2
               AND (first_created_at < ?3 OR (first_created_at = ?3 AND first_id <= ?4))
               AND (last_created_at > ?3 OR (last_created_at = ?3 AND last_id >= ?4))",
        )?;
        let rows = statement
            .query_map(params![SCOPE_KIND_ROOM, room_id, created_at, message_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (session_id, seq) in hits {
        tx.execute(
            "DELETE FROM context_fold_blocks
             WHERE scope_kind=?1 AND scope_id=?2 AND session_id=?3 AND seq>=?4",
            params![SCOPE_KIND_ROOM, room_id, session_id, seq],
        )?;
    }
    Ok(())
}

fn next_seq(connection: &Connection, room_id: &str, session_id: &str) -> rusqlite::Result<u32> {
    let seq: i64 = connection.query_row(
        "SELECT COALESCE(MAX(seq),0)+1 FROM context_fold_blocks
         WHERE scope_kind=?1 AND scope_id=?2 AND session_id=?3",
        params![SCOPE_KIND_ROOM, room_id, session_id],
        |row| row.get(0),
    )?;
    Ok(seq as u32)
}

/// 折叠输入投影：与装配历史循环同一过滤链（floor/goal-temp/assistant-partial/
/// project/strip/append/非空），只多一个"水位之后"条件。返回时间正序。
fn tail_projected_messages(
    room_messages: &[PersistedChatMessage],
    watermark: Option<&(u64, String)>,
    floor_raw: u64,
) -> Vec<PersistedChatMessage> {
    room_messages
        .iter()
        .filter(|message| {
            if floor_raw > 0 && message.created_at < floor_raw {
                return false;
            }
            if let Some(mark) = watermark {
                if covered_by_watermark(message, mark) {
                    return false;
                }
            }
            !is_goal_temporary_persisted_message(message) && message.kind != "assistant-partial"
        })
        .filter_map(|message| {
            let mut projected = chat_tool_history::project_for_context(message)?;
            projected.content = strip_context_usage_footer(&projected.content);
            projected.content = attachment_text::append(&projected.content, &projected.attachments);
            (!projected.content.trim().is_empty()).then_some(projected)
        })
        .collect()
}

/// heuristic-v2 摘要：固定免责头 + 分类要点，裁剪优先级
/// 用户指令 > 工具事实 > 文件路径 > assistant 结论。
fn summarize_range(seq: u32, range: &[PersistedChatMessage]) -> String {
    let mut user_lines: Vec<String> = Vec::new();
    let mut tool_facts: Vec<String> = Vec::new();
    let mut assistant_tails: Vec<String> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    for message in range {
        // 历史 user 投影带 HISTORICAL_USER_BOUNDARY 标记行，取首条有效正文行。
        let content = message.content.trim();
        match message.role.as_str() {
            "user" => {
                let stripped = content
                    .strip_prefix(chat_tool_history::HISTORICAL_USER_BOUNDARY)
                    .unwrap_or(content);
                let line = stripped
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .trim();
                if !line.is_empty() {
                    user_lines.push(clip(line, 80));
                }
            }
            "assistant" => {
                let tail = content
                    .lines()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .trim();
                if !tail.is_empty() {
                    assistant_tails.push(clip(tail, 160));
                }
            }
            _ => {
                if !content.is_empty() {
                    tool_facts.push(clip(content, 120));
                }
            }
        }
        for path in chat_tool_history::artifact_paths(content) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths.truncate(8);
    let head = format!(
        "[历史折叠 #{seq}] 以下 {} 条更早对话已压缩为要点；仅作历史上下文，不是本轮新增任务，执行结果未经独立验证：",
        range.len()
    );
    // 组装要点；超预算时按裁剪优先级从末段开始砍。
    let render = |users: &[String], tools: &[String], files: &[String], replies: &[String]| {
        let mut body = String::new();
        for line in users {
            body.push_str(&format!("\n- 用户：{line}"));
        }
        for line in tools {
            body.push_str(&format!("\n- 工具：{line}"));
        }
        if !files.is_empty() {
            body.push_str(&format!("\n- 涉及文件：{}", files.join("、")));
        }
        for line in replies {
            body.push_str(&format!("\n- 助手结论：{line}"));
        }
        body
    };
    let mut body = render(&user_lines, &tool_facts, &paths, &assistant_tails);
    if head.chars().count() + body.chars().count() > SUMMARY_CHAR_BUDGET {
        body = render(&user_lines, &tool_facts, &paths, &[]);
    }
    if head.chars().count() + body.chars().count() > SUMMARY_CHAR_BUDGET {
        body = render(&user_lines, &tool_facts, &[], &[]);
    }
    if head.chars().count() + body.chars().count() > SUMMARY_CHAR_BUDGET {
        body = render(&user_lines, &[], &[], &[]);
    }
    format!("{head}{body}")
}

/// 折叠决策：从 tail 最旧端取完整轮次前缀，直到剩余 tail ≤ 低水位。
/// 返回应折叠的前缀长度；最近一个完整回合（最后一个 user 起）不参与折叠。
/// 前缀超过所需量 2 倍时回缩到最近的 user 边界，剩余超量留给下一轮。
fn decide_fold_prefix(
    tail: &[PersistedChatMessage],
    tail_tokens: u32,
    low_target: u32,
) -> Option<usize> {
    if tail_tokens <= low_target || tail.is_empty() {
        return None;
    }
    // 最近一个完整回合：最后一个 user 及其后全部消息受保护。
    let protect_from = tail.iter().rposition(|message| message.role == "user")?;
    if protect_from == 0 {
        return None;
    }
    let excess = tail_tokens.saturating_sub(low_target);
    let cap = excess.saturating_mul(2).max(1);
    let mut folded = 0_u32;
    let mut end = 0_usize;
    for (index, message) in tail.iter().enumerate().take(protect_from) {
        if folded >= excess {
            break;
        }
        folded = folded.saturating_add(estimate_message_tokens(message));
        end = index + 1;
    }
    // 边界对齐：不能在 user/assistant/工具轮次中间切断，end 后一条必须是 user 或已到保护边界。
    while end < protect_from && tail[end].role != "user" {
        folded = folded.saturating_add(estimate_message_tokens(&tail[end]));
        end += 1;
    }
    // 上限回缩：区间过大时从尾部退回 user 边界，避免单块跨度过长（多块拆分留给后续里程碑）。
    while end > 1 && folded > cap {
        end -= 1;
        folded = folded.saturating_sub(estimate_message_tokens(&tail[end]));
        while end > 1 && tail[end].role != "user" {
            end -= 1;
            folded = folded.saturating_sub(estimate_message_tokens(&tail[end]));
        }
    }
    // 回缩后仍找不到完整轮次边界的切点：宁可不折，也不切在轮次中间。
    if end == 0 || (end < protect_from && tail[end].role != "user") {
        return None;
    }
    Some(end)
}

/// 回合后折叠写路径的判定结果。
pub(crate) enum FoldOutcome {
    /// tail 未达高水位或无可折叠区间：不动。
    NoTrigger,
    /// 已写入一个 FoldBlock。
    Folded { seq: u32, count: usize },
    /// tail 已超阈值但无法安全折叠（单轮过大/块总量超限/折叠后仍满）：
    /// 调用方应回退到现有硬重置路径。
    Fallback,
}

/// 回合后折叠：tail ≥ 高水位时从最旧端折叠到低水位，写一个块。
/// 失败只记诊断返回 NoTrigger/Fallback，不影响本轮回复。
pub(crate) fn fold_after_turn(
    connection: &Connection,
    room_id: &str,
    session_id: &str,
    room_messages: &[PersistedChatMessage],
    floor_raw: u64,
    history_token_budget: u32,
    high_percent: u32,
    low_percent: u32,
) -> rusqlite::Result<FoldOutcome> {
    let blocks = load_blocks(connection, room_id, session_id, floor_raw)?;
    let mark = watermark(&blocks);
    let tail = tail_projected_messages(room_messages, mark.as_ref(), floor_raw);
    let tail_tokens: u32 = tail.iter().map(estimate_message_tokens).sum();
    let high_target = history_token_budget.saturating_mul(high_percent) / 100;
    let low_target = history_token_budget.saturating_mul(low_percent) / 100;
    diagnostics::info(
        "context",
        "context_fold.evaluate",
        "跨轮折叠评估",
        &[
            ("tail_tokens", tail_tokens.to_string()),
            ("high_target", high_target.to_string()),
            ("blocks", blocks.len().to_string()),
        ],
    );
    if tail_tokens < high_target {
        return Ok(FoldOutcome::NoTrigger);
    }
    let Some(prefix_len) = decide_fold_prefix(&tail, tail_tokens, low_target) else {
        // 高水位已过但没有可折叠的完整轮次（单轮过大）：回退硬重置。
        return Ok(FoldOutcome::Fallback);
    };
    let range = &tail[..prefix_len];
    let range_tokens: u32 = range.iter().map(estimate_message_tokens).sum();
    let seq = next_seq(connection, room_id, session_id)?;
    let summary = summarize_range(seq, range);
    let token_count = estimate_bead_tokens(&summary);
    // 块总量上限：超过历史预算 15% 说明层级聚合（M5）缺位，回退硬重置并作废块。
    if total_token_count(&blocks).saturating_add(token_count)
        > history_token_budget.saturating_mul(BLOCK_TOTAL_PERCENT) / 100
    {
        diagnostics::info(
            "context",
            "context_fold.block_cap",
            "折叠块总量超出预算占比，回退硬重置",
            &[("token_count", token_count.to_string())],
        );
        return Ok(FoldOutcome::Fallback);
    }
    let remaining = tail_tokens.saturating_sub(range_tokens);
    if remaining >= history_token_budget {
        // 折叠后仍溢出（保护段本身超预算）：回退硬重置。
        return Ok(FoldOutcome::Fallback);
    }
    let block = FoldBlock {
        seq,
        first_key: (range[0].created_at, range[0].id.clone()),
        last_key: (
            range[range.len() - 1].created_at,
            range[range.len() - 1].id.clone(),
        ),
        range_message_count: range.len() as u32,
        range_token_estimate: range_tokens,
        summary,
        token_count,
        floor: floor_raw,
        created_at: unix_timestamp_millis(),
    };
    insert_block(connection, room_id, session_id, &block)?;
    diagnostics::info(
        "context",
        "context_fold.folded",
        "跨轮历史已折叠为摘要块",
        &[
            ("seq", seq.to_string()),
            ("messages", range.len().to_string()),
            ("range_tokens", range_tokens.to_string()),
            ("tail_tokens", tail_tokens.to_string()),
            ("summary_tokens", token_count.to_string()),
        ],
    );
    crate::diag_log(&format!(
        "[CONTEXT-FOLD] folded session={session_id} room={room_id} seq={seq} messages={} range_tokens={range_tokens} tail={tail_tokens}->{remaining}",
        range.len()
    ));
    Ok(FoldOutcome::Folded {
        seq,
        count: range.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PersistedChatMessage;

    fn message(id: &str, role: &str, kind: &str, content: &str, created_at: u64) -> PersistedChatMessage {
        PersistedChatMessage {
            id: id.to_string(),
            author: role.to_string(),
            role: role.to_string(),
            target: String::new(),
            content: content.to_string(),
            kind: kind.to_string(),
            attachments: Vec::new(),
            created_at,
        }
    }

    fn fixture_db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE sessions(id TEXT PRIMARY KEY);
                 CREATE TABLE chat_rooms(id TEXT PRIMARY KEY);
                 CREATE TABLE chat_room_messages(room_id TEXT, id TEXT, created_at INTEGER);
                 INSERT INTO sessions VALUES('s1');
                 INSERT INTO chat_rooms VALUES('r1');",
            )
            .unwrap();
        apply_session_migration_v32(&connection).unwrap();
        connection
    }

    fn block(seq: u32, last: u64, tokens: u32) -> FoldBlock {
        FoldBlock {
            seq,
            first_key: (1, "m1".into()),
            last_key: (last, format!("m{last}")),
            range_message_count: 2,
            range_token_estimate: tokens * 10,
            summary: format!("[历史折叠 #{seq}] 摘要"),
            token_count: tokens,
            floor: 0,
            created_at: 0,
        }
    }

    #[test]
    fn migration_creates_table_and_advances_version() {
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v32(&connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 32);
        connection
            .prepare("SELECT seq,summary,floor FROM context_fold_blocks LIMIT 0")
            .unwrap();
    }

    #[test]
    fn insert_load_roundtrip_and_floor_filter() {
        let connection = fixture_db();
        let mut folded = block(1, 10, 300);
        insert_block(&connection, "r1", "s1", &folded).unwrap();
        folded.seq = 2;
        folded.floor = 7;
        insert_block(&connection, "r1", "s1", &folded).unwrap();
        let floor0 = load_blocks(&connection, "r1", "s1", 0).unwrap();
        assert_eq!(floor0.len(), 1);
        assert_eq!(floor0[0].seq, 1);
        assert_eq!(load_blocks(&connection, "r1", "s1", 7).unwrap().len(), 1);
        assert!(load_blocks(&connection, "r1", "s1", 99).unwrap().is_empty());
        assert!(load_blocks(&connection, "r1", "other", 0).unwrap().is_empty());
    }

    #[test]
    fn watermark_covers_boundary_key() {
        let blocks = vec![block(1, 10, 100), block(2, 20, 100)];
        let mark = watermark(&blocks).unwrap();
        assert_eq!(mark.0, 20);
        assert!(covered_by_watermark(&message("m5", "user", "chat", "x", 5), &mark));
        assert!(covered_by_watermark(&message("m20", "user", "chat", "x", 20), &mark));
        assert!(!covered_by_watermark(&message("m21", "user", "chat", "x", 21), &mark));
        // created_at 相同时按 id 比较
        assert!(!covered_by_watermark(&message("m21", "user", "chat", "x", 20), &mark));
        assert!(watermark(&[]).is_none());
    }

    #[test]
    fn tail_projection_mirrors_assembly_filters() {
        let mark = (10u64, "m10".to_string());
        let messages = vec![
            message("m5", "user", "chat", "旧消息", 5),
            message("msg-goal-temp", "user", "chat", "临时", 11),
            message("m12", "assistant", "assistant-partial", "瞬时", 12),
            message("m13", "user", "chat", "水位后用户", 13),
            message("m14", "assistant", "chat", "", 14),
            message("m15", "assistant", "chat", "回答", 15),
        ];
        let tail = tail_projected_messages(&messages, Some(&mark), 0);
        // m5 在水位内；m11 goal 临时；m12 partial；m14 投影后为空
        let ids: Vec<&str> = tail.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["m13", "m15"]);
        // floor 过滤
        assert!(tail_projected_messages(&messages, None, 14)
            .iter()
            .all(|m| m.created_at >= 14));
    }

    #[test]
    fn decide_fold_prefix_respects_turn_boundaries_and_protects_latest() {
        // user/assistant 成对；最后一轮受保护不可折。
        let tail: Vec<_> = (0..8)
            .map(|i| {
                if i % 2 == 0 {
                    message(&format!("u{i}"), "user", "chat", &"问".repeat(100), i as u64 + 1)
                } else {
                    message(&format!("a{i}"), "assistant", "chat", &"答".repeat(100), i as u64 + 1)
                }
            })
            .collect();
        let tail_tokens: u32 = tail.iter().map(estimate_message_tokens).sum();
        let low = tail_tokens / 2;
        let end = decide_fold_prefix(&tail, tail_tokens, low).unwrap();
        // end 边界必须是 user 开始（下一轮起点）且不含最后一个 user(6)
        assert!(end >= 2 && end <= 6);
        assert_eq!(tail[end].role, "user");
        // 全部折完也不可：保护段必须留下
        assert!(decide_fold_prefix(&tail[..2], tail_tokens / 4, 1).is_none());
    }

    #[test]
    fn fold_after_turn_writes_block_and_reports_outcome() {
        let connection = fixture_db();
        let mut messages = Vec::new();
        // 每条 ~400 tokens，tail ≈ 2.4K，贴近真实预算量级。
        for i in 0..6u64 {
            messages.push(message(&format!("m{i}"), if i % 2 == 0 { "user" } else { "assistant" }, "chat", &"内容".repeat(800), i + 1));
        }
        let tokens: u32 = messages.iter().map(estimate_message_tokens).sum();
        let budget = tokens;
        let outcome = fold_after_turn(&connection, "r1", "s1", &messages, 0, budget, 85, 50).unwrap();
        match outcome {
            FoldOutcome::Folded { seq, count } => {
                assert_eq!(seq, 1);
                assert!(count >= 2);
            }
            _ => panic!("expected folded, got fallback/no-trigger"),
        }
        let blocks = load_blocks(&connection, "r1", "s1", 0).unwrap();
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].summary.starts_with("[历史折叠 #1]"));
        // 第二轮：水位前进，tail 变短不再触发。
        let outcome2 = fold_after_turn(&connection, "r1", "s1", &messages, 0, budget, 85, 50).unwrap();
        assert!(matches!(outcome2, FoldOutcome::NoTrigger | FoldOutcome::Folded { .. }));
    }

    #[test]
    fn fold_after_turn_fallbacks_when_only_one_turn_is_foldable() {
        let connection = fixture_db();
        // 只有一轮（user+assistant），保护段 = 整个 tail → 无法折叠 → Fallback
        let messages = vec![
            message("u", "user", "chat", &"问".repeat(2000), 1),
            message("a", "assistant", "chat", &"答".repeat(2000), 2),
        ];
        let tokens: u32 = messages.iter().map(estimate_message_tokens).sum();
        let outcome =
            fold_after_turn(&connection, "r1", "s1", &messages, 0, tokens / 2, 80, 40).unwrap();
        assert!(matches!(outcome, FoldOutcome::Fallback));
    }

    #[test]
    fn invalidate_covering_message_drops_block_and_later_seqs() {
        let mut connection = fixture_db();
        connection
            .execute_batch("INSERT INTO chat_room_messages VALUES('r1','m5',5)")
            .unwrap();
        insert_block(&connection, "r1", "s1", &block(1, 10, 100)).unwrap();
        let mut b2 = block(2, 20, 100);
        b2.first_key = (11, "m11".into());
        insert_block(&connection, "r1", "s1", &b2).unwrap();
        let tx = connection.transaction().unwrap();
        invalidate_covering_message(&tx, "r1", "m5").unwrap();
        tx.commit().unwrap();
        assert!(load_blocks(&connection, "r1", "s1", 0).unwrap().is_empty());
        // 区间外的消息不影响
        insert_block(&connection, "r1", "s1", &block(1, 10, 100)).unwrap();
        connection
            .execute_batch("INSERT INTO chat_room_messages VALUES('r1','m99',99)")
            .unwrap();
        let tx = connection.transaction().unwrap();
        invalidate_covering_message(&tx, "r1", "m99").unwrap();
        tx.commit().unwrap();
        assert_eq!(load_blocks(&connection, "r1", "s1", 0).unwrap().len(), 1);
    }

    #[test]
    fn delete_scope_and_orphans_clean_blocks() {
        let mut connection = fixture_db();
        insert_block(&connection, "r1", "s1", &block(1, 10, 100)).unwrap();
        assert_eq!(delete_scope(&connection, "r1").unwrap(), 1);
        insert_block(&connection, "r1", "s1", &block(1, 10, 100)).unwrap();
        // 删掉房间 → 孤儿清扫
        connection.execute_batch("DELETE FROM chat_rooms").unwrap();
        let tx = connection.transaction().unwrap();
        assert_eq!(delete_orphans(&tx).unwrap(), 1);
        tx.commit().unwrap();
    }

    #[test]
    fn summarize_range_keeps_user_lines_and_disclaimer() {
        let range = vec![
            message("u1", "user", "chat", "把 /tmp/a.rs 改成异步", 1),
            message("t1", "assistant", "tool-result", "工具 read_file 读取 /tmp/a.rs 成功", 2),
            message("a1", "assistant", "chat", "已修改完成。\n最后结论行", 3),
        ];
        let summary = summarize_range(3, &range);
        assert!(summary.contains("[历史折叠 #3]"));
        assert!(summary.contains("仅作历史上下文"));
        assert!(summary.contains("用户："));
        assert!(summary.contains("助手结论："));
        assert!(summary.chars().count() <= SUMMARY_CHAR_BUDGET + 64);
    }
}
