//! 远端会话的宿主增量上下文；消息归属复用真实聊天运行事件，不按作者名称猜测。
use super::{chat::digest, journal::Journal, protocol::ExecutionScope};
use crate::ContextAssembly;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

#[derive(Clone, Deserialize, Serialize)]
struct MessageStamp {
    id: String,
    digest: String,
    #[serde(default)]
    source: bool,
    #[serde(default)]
    deleted: bool,
}

pub(super) struct PreparedContext {
    pub prompt: String,
    pub images: Vec<Value>,
    stamps: Vec<MessageStamp>,
    system_digest: String,
    continued: bool,
    history_count: usize,
    delta_count: usize,
}

fn stamp(message: &crate::PersistedChatMessage, source: bool) -> MessageStamp {
    MessageStamp {
        id: message.id.clone(),
        source,
        deleted: false,
        digest: digest(
            serde_json::to_string(&if message.attachments.is_empty() {
                json!([&message.role, &message.kind, &message.content])
            } else {
                json!([&message.role, &message.kind, &message.content, &message.attachments])
            })
                .expect("消息摘要可以编码")
                .as_bytes(),
        ),
    }
}

fn ensure(connection: &rusqlite::Connection) -> Result<(), String> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS devin_acp_context_inputs (
        attempt_id TEXT PRIMARY KEY, remote_session_id TEXT NOT NULL,
        stamps_json TEXT NOT NULL, system_digest TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS idx_devin_context_remote ON devin_acp_context_inputs(remote_session_id);")
        .map_err(|error| format!("Devin 增量上下文表不可用：{error}"))
}

/// 宿主先持久化真实回复，再登记消息 ID 与摘要。无需依赖模型自报或显示名称。
pub(crate) fn record_outputs(
    path: &Path,
    run_id: &str,
    room_id: &str,
    agent_id: &str,
    ids: &[String],
) -> rusqlite::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let connection = rusqlite::Connection::open(path)?;
    let mut query = connection.prepare(
        "SELECT id,author,role,target,content,kind,attachments_json,created_at
        FROM chat_room_messages WHERE room_id=?1 AND id IN (SELECT value FROM json_each(?2))",
    )?;
    let messages = query
        .query_map(
            params![room_id, serde_json::to_string(ids).unwrap()],
            crate::persisted_message_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let stamps = messages
        .iter()
        .map(|message| stamp(message, false))
        .collect::<Vec<_>>();
    crate::append_runtime_run_event(
        path,
        run_id,
        "chat.context_outputs",
        json!({"agent_id":agent_id,"messages":stamps}),
    )
    .map(|_| ())
}

/// 使用回复生成时已经确定的 Agent 身份，不按显示作者归属；记录失败不改写真实执行终态。
pub(crate) fn record_replies(
    parent: Option<&crate::FrozenParentContext>,
    room_id: &str,
    replies: &[crate::AutoHandoffCandidate],
) {
    let Some((path, run_id)) = parent.and_then(|parent| {
        Some((
            parent.runtime_db_path.as_deref()?,
            parent.parent_run_id.as_deref()?,
        ))
    }) else {
        return;
    };
    let mut owned = BTreeMap::<&str, Vec<String>>::new();
    for reply in replies {
        owned
            .entry(&reply.from_agent_id)
            .or_default()
            .push(reply.assistant_message_id.clone());
    }
    for (agent, ids) in owned {
        if let Err(error) = record_outputs(path, run_id, room_id, agent, &ids) {
            crate::diag_log(&format!("[CHAT-CONTEXT] 回复归属记录未确认：{error}"));
        }
    }
}

pub(super) fn prepare(
    journal: &Journal,
    scope: &ExecutionScope,
    assembly: &ContextAssembly,
    instructions: &str,
) -> Result<PreparedContext, String> {
    let binding = journal.binding(scope)?;
    let connection = journal.connection()?;
    ensure(&connection)?;
    let mut known = BTreeMap::<String, String>::new();
    let mut uncertain = Vec::new();
    let mut previous_system = None;
    if let Some(remote) = binding.remote_session_id.as_deref() {
        let mut query = connection.prepare("SELECT i.stamps_json,i.system_digest,a.state,a.protocol_stop,
            a.process_drained,a.scope_json FROM devin_acp_context_inputs i
            JOIN devin_acp_attempts a ON a.attempt_id=i.attempt_id WHERE i.remote_session_id=?1 ORDER BY a.rowid")
            .map_err(|error| format!("Devin 发送记录读取失败：{error}"))?;
        let rows = query
            .query_map([remote], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|error| format!("Devin 发送记录查询失败：{error}"))?;
        for row in rows {
            let (encoded, system, state, stop, drained, source) =
                row.map_err(|error| error.to_string())?;
            let completed = state == "terminal" && stop.as_deref() == Some("end_turn") && drained;
            if state == "prepared" || state == "not_sent" {
                continue;
            }
            let previous: ExecutionScope =
                serde_json::from_str(&source).map_err(|_| "Devin 历史来源损坏")?;
            if (
                previous.workspace_id.as_str(),
                previous.room_id.as_str(),
                previous.agent_id.as_str(),
                previous.lane.as_str(),
            ) != (
                scope.workspace_id.as_str(),
                scope.room_id.as_str(),
                scope.agent_id.as_str(),
                scope.lane.as_str(),
            ) {
                return Err("Devin 远端上下文归属不一致，未发送本轮".into());
            }
            let stamps: Vec<MessageStamp> =
                serde_json::from_str(&encoded).map_err(|_| "Devin 消息游标损坏")?;
            for item in stamps {
                // 不确定轮次只抑制旧用户指令重放；不冒称其它历史已确认导入。
                if completed || item.source {
                    if item.deleted {
                        known.remove(&item.id);
                    } else {
                        known.insert(item.id, item.digest);
                    }
                }
            }
            if !completed {
                uncertain.push(previous.turn_id);
                continue;
            }
            previous_system = Some(system);
            let mut replies = connection
                .prepare(
                    "SELECT payload_json FROM runtime_run_events
                WHERE run_id=?1 AND event_type='chat.context_outputs' ORDER BY rowid",
                )
                .map_err(|error| error.to_string())?;
            for reply in replies
                .query_map([&previous.run_id], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?
            {
                let value: Value = serde_json::from_str(&reply.map_err(|error| error.to_string())?)
                    .map_err(|_| "Devin 宿主回复归属损坏")?;
                // 同一次宿主运行可包含其它发送对象的回复；它们仍需作为新增历史导入。
                if value["agent_id"].as_str() != Some(scope.agent_id.as_str()) {
                    continue;
                }
                let stamps: Vec<MessageStamp> = serde_json::from_value(value["messages"].clone())
                    .map_err(|_| "Devin 宿主回复摘要损坏")?;
                for item in stamps {
                    known.insert(item.id, item.digest);
                }
            }
        }
    }
    let mut sources = connection.prepare("SELECT refs.value FROM runtime_run_events e,
        json_each(e.payload_json,'$.message_ids') refs WHERE e.run_id=?1 AND e.event_type='chat.source_messages'")
        .map_err(|error| error.to_string())?;
    let source_ids = sources
        .query_map([&scope.run_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(|error| error.to_string())?;
    let selected = &assembly.history_selection.selected_ids;
    if assembly.messages.len() != selected.len() + 1 {
        return Err("Devin 历史消息与上下文投影不一致".into());
    }
    let mut ids = selected.clone();
    ids.extend(source_ids.iter().cloned());
    // 删除与更正直接核对消息库，不能把预算裁剪或不投影消息当作已删除。
    ids.extend(known.keys().cloned());
    let mut query = connection
        .prepare(
            "SELECT id,author,role,target,content,kind,attachments_json,created_at
        FROM chat_room_messages WHERE room_id=?1 AND id IN (SELECT value FROM json_each(?2))",
        )
        .map_err(|error| error.to_string())?;
    let messages = query
        .query_map(
            params![scope.room_id, serde_json::to_string(&ids).unwrap()],
            crate::persisted_message_from_row,
        )
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let persisted_stamps = messages
        .iter()
        .map(|message| stamp(message, source_ids.contains(&message.id)))
        .collect::<Vec<_>>();
    let actual = persisted_stamps
        .iter()
        .map(|item| (&item.id, &item.digest))
        .collect::<BTreeMap<_, _>>();
    let mut delta = Vec::<Value>::new();
    let corrected = known
        .iter()
        .filter(|(id, old)| {
            actual
                .get(id)
                .is_some_and(|current| current.as_str() != old.as_str())
        })
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for (index, id) in selected.iter().enumerate() {
        let current = actual
            .get(id)
            .ok_or("Devin 上下文消息已被删除，请重新发送")?;
        if known.get(id).is_some_and(|old| old == *current) {
            continue;
        }
        // 保留宿主 ID，后续更正/撤回说明才能准确指向远端已接收的历史。
        let author = messages
            .iter()
            .find(|message| &message.id == id)
            .map(|message| &message.author);
        delta
            .push(json!({"host_message_id":id,"author":author,"message":assembly.messages[index]}));
    }
    let (current, images) = super::image_input::split(
        assembly.messages.last().ok_or("Devin 当前用户消息缺失")?)?;
    delta.push(json!({"host_message_ids":&source_ids, "message":current}));
    let removed = known
        .keys()
        .filter(|id| !actual.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    let selected_ids = selected.iter().cloned().collect::<HashSet<_>>();
    let mut stamps = persisted_stamps
        .iter()
        .filter(|item| item.source || selected_ids.contains(&item.id))
        .cloned()
        .collect::<Vec<_>>();
    // 只在本轮协议完成并排空后确认撤回；未知发送下次仍需通知，不冒称已经同步。
    // 预算外更正只撤回旧版本；后续被选中时仍须真正注入新正文，不能冒称已读。
    stamps.extend(
        removed
            .iter()
            .chain(
                corrected
                    .iter()
                    .filter(|id| !selected_ids.contains(*id) && !source_ids.contains(*id)),
            )
            .map(|id| MessageStamp {
                id: id.clone(),
                digest: String::new(),
                source: false,
                deleted: true,
            }),
    );
    let system_digest = digest(assembly.system_prompt.as_bytes());
    let system = if previous_system.as_deref() == Some(system_digest.as_str()) {
        "本轮沿用上一轮宿主系统说明与选定记忆。".to_string()
    } else {
        format!(
            "以下宿主系统说明与选定记忆替代此前版本；撤回的内容不再适用：\n{}",
            assembly.system_prompt
        )
    };
    let notice = format!("历史更正（这些消息的旧内容作废）：{}。已删除消息（不得再据此执行）：{}。\n此前未完成回合：{}；旧任务已经结束，禁止重放或继续执行，只有本轮最后一条用户消息是新指令。远端历史无法物理删除，需彻底清除时应重置上下文。",
        serde_json::to_string(&corrected).unwrap(),serde_json::to_string(&removed).unwrap(),
        serde_json::to_string(&uncertain.iter().rev().take(8).collect::<Vec<_>>()).unwrap());
    let continued = binding.remote_session_id.is_some();
    let prompt = format!("{instructions}\n\n{system}\n\n{notice}\n\n{}（历史仅作参考，只回答最后一条用户消息）：\n{}",
        if continued { "本轮宿主新增或更正消息；远端已有历史不重复注入" } else { "初次宿主消息快照" },
        serde_json::to_string(&delta).map_err(|_| "Devin 上下文编码失败")?);
    if prompt.len() > 768 * 1024 {
        return Err("本轮文本上下文超过 ACP 消息限额，请缩短输入或重置上下文".into());
    }
    Ok(PreparedContext {
        prompt,
        images,
        stamps,
        system_digest,
        continued,
        history_count: selected.len(),
        delta_count: delta.len().saturating_sub(1),
    })
}

impl PreparedContext {
    /// 配置握手确认远端 ID 后、发送提示之前保存关联；终态和排空事实决定是否已导入。
    pub(super) fn record(&self, journal: &Journal, scope: &ExecutionScope) -> Result<(), String> {
        let mut connection = journal.connection()?;
        ensure(&connection)?;
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        Journal::check(&tx, scope)?;
        let remote: String = tx
            .query_row(
                "SELECT remote_session_id FROM devin_acp_bindings WHERE locked_attempt=?1",
                [&scope.attempt_id],
                |row| row.get(0),
            )
            .map_err(|_| "Devin 远端身份尚未确认")?;
        tx.execute("INSERT INTO devin_acp_context_inputs(attempt_id,remote_session_id,stamps_json,system_digest)
            VALUES(?1,?2,?3,?4)",params![scope.attempt_id,remote,serde_json::to_string(&self.stamps).unwrap(),self.system_digest])
            .map_err(|error| format!("Devin 增量上下文关联保存失败：{error}"))?;
        tx.commit().map_err(|error| error.to_string())?;
        crate::append_runtime_run_event(
            journal.path(),
            &scope.run_id,
            "devin.context_delta",
            json!({
            "continued":self.continued,"host_history_messages":self.history_count,
            "delta_history_messages":self.delta_count,"prompt_bytes":self.prompt.len(),"image_count":self.images.len()}),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }
}
