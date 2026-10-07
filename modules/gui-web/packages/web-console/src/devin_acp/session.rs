//! ACP 会话驱动：单一提示循环、有界消息队列、独立取消和逐条身份核对。
//! 聊天路由仅使用经过验证的纯文本配置；工具任务使用独立的能力闸门。
use super::{
    journal::Journal,
    protocol::{self, ExecutionScope, ModelSelection},
    transport::{MAX_FRAME_BYTES, read_frame},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{io, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, AsyncWriteExt},
    sync::mpsc,
    task::JoinHandle,
};

const MAX_NOTIFICATIONS: usize = 16_384;
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
// 入站仍保持 1 MiB；原图多帧提示仅扩大出站图片请求，不影响工具消息或握手。
const MAX_IMAGE_PROMPT_BYTES: usize = 8 * 1024 * 1024;

fn prompt_image_count(value: &Value) -> usize {
    if value["method"] != "session/prompt" { return 0; }
    value.pointer("/params/prompt").and_then(Value::as_array).map_or(0, |blocks| {
        if blocks.iter().any(|block| match block["type"].as_str() {
            Some("text") => block["text"].as_str().is_none_or(|text| text.trim().is_empty()),
            Some("image") => !matches!(block["mimeType"].as_str(), Some("image/png" | "image/jpeg" | "image/webp"))
                || block["data"].as_str().is_none_or(str::is_empty),
            _ => true,
        }) { return 0; }
        blocks.iter().filter(|block| block["type"] == "image").count()
    })
}

fn outgoing_limit(value: &Value) -> usize {
    if prompt_image_count(value) > 0 { MAX_IMAGE_PROMPT_BYTES } else { MAX_FRAME_BYTES }
}

fn validate_outgoing_bytes(value: &Value, bytes: &[u8]) -> io::Result<()> {
    let text_bytes = value.pointer("/params/prompt").and_then(Value::as_array).map_or(0, |blocks|
        blocks.iter().filter_map(|block| block["text"].as_str()).fold(0usize, |total, text| total.saturating_add(text.len())));
    if value["method"] == "session/prompt" && text_bytes >= MAX_FRAME_BYTES {
        return Err(invalid("ACP 提示文本超过本地 1 MiB 限制；未发送。"));
    }
    if bytes.len() >= outgoing_limit(value) {
        return Err(invalid(if prompt_image_count(value) > 0 {
            "ACP 图片提示超过本地 8 MiB 出站限制；未发送。"
        } else { "ACP 请求超过本地 1 MiB 出站限制；未发送。" }));
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

#[derive(Debug, Serialize)]
pub(super) struct Event {
    pub scope: ExecutionScope,
    pub sequence: u64,
    /// load 返回的历史消息不能当成本次回复或本次工具执行。
    pub replay: bool,
    pub update: Update,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Update {
    Text {
        text: String,
        thought: bool,
        message_id: Option<String>,
    },
    UserText {
        text: String,
        message_id: Option<String>,
    },
    /// 远端工具通知仅为观察，真实宿主执行事实由工具桥的台账产生。
    ToolObservation {
        tool_call_id: String,
        status: Option<String>,
    },
    Usage {
        used: u64,
        size: u64,
        cost: Option<Cost>,
    },
    Config {
        options: Value,
    },
    Diagnostic {
        name: String,
    },
}

#[derive(Debug, Serialize)]
pub(super) struct Cost {
    pub amount: f64,
    pub currency: String,
}

fn project(update: &Value) -> io::Result<Update> {
    let name = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("ACP 通知缺少类型。"))?;
    let message_id = update
        .get("messageId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match name {
        "agent_message_chunk" | "agent_thought_chunk" | "user_message_chunk" => {
            let content = update
                .get("content")
                .ok_or_else(|| invalid("ACP 文本通知缺少内容。"))?;
            if content.get("type").and_then(Value::as_str) != Some("text") {
                return Err(invalid("当前 ACP 会话未开放非文本回复。"));
            }
            let text = content
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("ACP 文本内容无效。"))?
                .to_owned();
            if name == "user_message_chunk" {
                Ok(Update::UserText { text, message_id })
            } else {
                Ok(Update::Text {
                    text,
                    thought: name == "agent_thought_chunk",
                    message_id,
                })
            }
        }
        "tool_call" | "tool_call_update" => {
            let tool_call_id = update
                .get("toolCallId")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| invalid("ACP 工具观察缺少关联编号。"))?
                .to_owned();
            let status = update
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Ok(Update::ToolObservation {
                tool_call_id,
                status,
            })
        }
        "usage_update" => {
            let used = update
                .get("used")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("ACP 用量缺少 used。"))?;
            let size = update
                .get("size")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("ACP 用量缺少 size。"))?;
            let cost = match update.get("cost").filter(|v| !v.is_null()) {
                None => None,
                Some(value) => {
                    let amount = value
                        .get("amount")
                        .and_then(Value::as_f64)
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .ok_or_else(|| invalid("ACP 费用数值无效。"))?;
                    let currency = value
                        .get("currency")
                        .and_then(Value::as_str)
                        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()))
                        .ok_or_else(|| invalid("ACP 费用币种无效。"))?
                        .to_owned();
                    Some(Cost { amount, currency })
                }
            };
            Ok(Update::Usage { used, size, cost })
        }
        "config_option_update" => Ok(Update::Config {
            options: update
                .get("configOptions")
                .filter(|v| v.is_array())
                .ok_or_else(|| invalid("ACP 配置通知缺少完整目录。"))?
                .clone(),
        }),
        other => Ok(Update::Diagnostic {
            name: other.to_owned(),
        }),
    }
}

/// reader 独立任务保留半帧；取消 select 不会丢掉已读入的部分 JSON。
pub(super) struct SessionTransport<W> {
    writer: W,
    frames: mpsc::Receiver<io::Result<Value>>,
    reader_task: JoinHandle<()>,
    next_id: u64,
    closed: bool,
}

impl<W: AsyncWrite + Unpin> SessionTransport<W> {
    pub(super) fn new<R: AsyncBufRead + Unpin + Send + 'static>(mut reader: R, writer: W) -> Self {
        let (send, frames) = mpsc::channel(32);
        let reader_task = tokio::spawn(async move {
            loop {
                let frame = read_frame(&mut reader).await;
                let failed = frame.is_err();
                if send.send(frame).await.is_err() || failed {
                    break;
                }
            }
        });
        Self {
            writer,
            frames,
            reader_task,
            next_id: 0,
            closed: false,
        }
    }

    async fn write(&mut self, value: &Value) -> io::Result<()> {
        if self.closed {
            return Err(invalid("ACP 连接已失效。"));
        }
        let mut bytes = serde_json::to_vec(value)?;
        validate_outgoing_bytes(value, &bytes)?;
        bytes.push(b'\n');
        self.writer.write_all(&bytes).await?;
        self.writer.flush().await
    }
    fn id(&mut self) -> io::Result<u64> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| invalid("ACP 请求 ID 已耗尽。"))?;
        Ok(self.next_id)
    }
    fn invalidate(&mut self) {
        self.closed = true;
        self.reader_task.abort();
    }

    /// 握手/加载期间接受历史通知，但所有 client 文件、终端及权限请求均拒绝。
    async fn configure(
        &mut self,
        method: &str,
        params: Value,
        deadline: tokio::time::Instant,
        mut event: impl FnMut(Value) -> io::Result<()>,
    ) -> io::Result<Value> {
        if !matches!(
            method,
            "initialize" | "session/new" | "session/load" | "session/set_config_option"
        ) {
            return Err(invalid("ACP 配置请求不在允许集合中。"));
        }
        let id = self.id()?;
        let result = tokio::time::timeout_at(deadline, async {
            self.write(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .await?;
            let mut count = 0;
            loop {
                let frame = self
                    .frames
                    .recv()
                    .await
                    .ok_or_else(|| invalid("ACP 接收器已关闭。"))??;
                if frame.get("method").is_some() {
                    count += 1;
                    if count > MAX_NOTIFICATIONS {
                        return Err(invalid("ACP 配置通知过多。"));
                    }
                    if frame.get("id").is_some() {
                        self.write(&protocol::denied_client_request(&frame).map_err(invalid)?)
                            .await?;
                    } else {
                        event(frame)?;
                    }
                } else {
                    return response(frame, id);
                }
            }
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "ACP 配置超时。"))
        .and_then(|v| v);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
}
impl<W> Drop for SessionTransport<W> {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

fn response(frame: Value, id: u64) -> io::Result<Value> {
    if frame.get("id").and_then(Value::as_u64) != Some(id)
        || frame.get("method").is_some()
    {
        return Err(invalid("ACP 响应身份不一致，原请求结果未确认。"));
    }
    if let Some(error) = frame.get("error") {
        let message = error["message"].as_str().unwrap_or_default().to_ascii_lowercase();
        let category = if ["quota", "credit", "rate limit"].iter().any(|part| message.contains(part)) {
            "账号额度或速率限制"
        } else if ["not logged", "unauthorized", "authentication"].iter().any(|part| message.contains(part)) {
            "登录状态"
        } else if message.contains("mcp") || message.contains("tool") {
            "远端工具处理"
        } else { "远端请求处理" };
        // 仅登记标准错误码和固定分类；原消息、data 和凭据都不落日志或前端。
        tracing::warn!(error_code = ?error["code"].as_i64(), category, "Devin ACP 请求被远端拒绝");
        return Err(invalid(&format!("Devin {category}失败，本轮未完成；未自动重试。")));
    }
    frame
        .get("result")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| invalid("ACP 响应缺少对象结果。"))
}

pub(super) struct SessionService<W> {
    transport: SessionTransport<W>,
    journal: Journal,
    scope: ExecutionScope,
    remote: String,
    selection: ModelSelection,
    sequence: u64,
    text_bytes: usize,
    deadline: tokio::time::Instant,
    prompt_image: bool,
    image_handoff: Option<Arc<AtomicBool>>,
    awaiting_continuation: bool,
    prompt_round: u16,
}

#[derive(Debug)]
pub(super) struct TurnResult {
    pub stop_reason: String,
    pub model: ModelSelection,
    pub cancel_requested: bool,
    pub image_handoff: bool,
    /// 此服务只观察协议终态，进程监督者另行核对整个进程树。
    pub process_drained: bool,
}

impl<W> Drop for SessionService<W> {
    fn drop(&mut self) {
        if self.awaiting_continuation {
            // 两段提示之间仍持有原执行锁；丢弃连接不能留下可继续派发的 submitted。
            let cancelled = self.journal.request_cancel(&self.scope);
            let unknown = self.journal.transition(&self.scope, &["submitted"], "unknown", None, false);
            if cancelled.is_err() || unknown.is_err() {
                tracing::error!("ACP 原图交接连接丢弃后的未知收尾未确认，会话发送锁保留");
            }
        }
    }
}

/// 调用方丢弃提示 future 也必须停止派发并持久化未知状态，不能让桥继续使用 submitted。
struct SubmittedGuard {
    journal: Journal,
    scope: ExecutionScope,
    cancellation: Arc<crate::ChatTurnCancellation>,
    reader: tokio::task::AbortHandle,
    finished: bool,
}
impl Drop for SubmittedGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.cancellation.request();
        self.reader.abort();
        let cancelled = self.journal.request_cancel(&self.scope);
        let unknown = self
            .journal
            .transition(&self.scope, &["submitted"], "unknown", None, false);
        if cancelled.is_err() || unknown.is_err() {
            tracing::error!("ACP 被丢弃后的未知收尾未完全确认，会话发送锁保留");
        }
    }
}

impl<W: AsyncWrite + Unpin> SessionService<W> {
    pub(super) async fn connect(
        mut transport: SessionTransport<W>,
        journal: Journal,
        scope: ExecutionScope,
        requested: &str,
        mcp_servers: Vec<Value>,
        deadline: tokio::time::Instant,
        mut sink: impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<Self> {
        // prepared 与完整 scope 必须已由接纳事务建立；不凭模型参数创建本地所有权。
        let binding = journal.binding(&scope).map_err(|e| invalid(&e))?;
        if journal.status(&scope).map_err(|e| invalid(&e))?.state != "prepared" {
            return Err(invalid("ACP attempt 不处于配置阶段。"));
        }
        let initialize = transport
            .configure(
                "initialize",
                protocol::initialize_params(),
                deadline,
                |_| Ok(()),
            )
            .await?;
        protocol::validate_initialize(&initialize).map_err(invalid)?;
        let prompt_image = initialize["agentCapabilities"]["promptCapabilities"]["image"] == true;
        journal.save_config_receipt(&scope, "capabilities", &json!({"protocol_version":1,"prompt_image":prompt_image}))
            .map_err(|e| invalid(&e))?;
        if !mcp_servers.is_empty()
            && initialize["agentCapabilities"]["mcpCapabilities"]["http"] != true
        {
            return Err(invalid("ACP 未声明受控 HTTP 工具桥能力。"));
        }
        for server in &mcp_servers {
            let url = server
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("ACP 工具桥缺少 URL。"))?;
            let parsed = reqwest::Url::parse(url).map_err(|_| invalid("ACP 工具桥 URL 无效。"))?;
            if server.get("type").and_then(Value::as_str) != Some("http")
                || parsed.scheme() != "http"
                || parsed.host_str() != Some("127.0.0.1")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(invalid("ACP 仅允许宿主的 IPv4 本机 HTTP 工具桥。"));
            }
        }
        let mut sequence = 0;
        let mut text_bytes = 0;
        let mut pending_new = Vec::new();
        let mut pending_bytes = 0usize;
        let mut params = json!({"cwd":binding.cwd,"mcpServers":mcp_servers});
        let result = if let Some(remote) = binding.remote_session_id.as_deref() {
            if initialize["agentCapabilities"]["loadSession"] != true {
                return Err(invalid("ACP 不支持恢复已有会话，未创建替代会话。"));
            }
            params["sessionId"] = json!(remote);
            transport
                .configure("session/load", params, deadline, |frame| {
                    emit(
                        &scope,
                        remote,
                        true,
                        &mut sequence,
                        &mut text_bytes,
                        frame,
                        &mut sink,
                        None,
                    )
                })
                .await.map_err(|reason| invalid(&format!(
                    "Devin 远端会话恢复失败：{reason}。未创建替代会话、未重发本轮；请核对原登录账号，必要时在设置中重置上下文。"
                )))?
        } else {
            transport
                .configure("session/new", params, deadline, |frame| {
                    // 固定真实 CLI 在 new 响应前发送配置通知；先有界缓存，确认身份前不发布。
                    if frame.get("method").and_then(Value::as_str) == Some("session/update") {
                        pending_bytes = pending_bytes.saturating_add(serde_json::to_vec(&frame)?.len());
                        if pending_new.len() >= 128 || pending_bytes > MAX_TEXT_BYTES {
                            return Err(invalid("ACP 创建会话前的通知超过限制。"));
                        }
                        pending_new.push(frame);
                    }
                    Ok(())
                })
                .await?
        };
        let remote = match binding.remote_session_id {
            Some(id) => id,
            None => result
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty() && v.len() <= 4096)
                .ok_or_else(|| invalid("ACP 新会话缺少有效 ID。"))?
                .into(),
        };
        // 所有缓存通知必须匹配 new 实际回执的 sessionId，完整校验后才落绑定并发布。
        let mut staged = Vec::new();
        for frame in pending_new {
            emit(&scope, &remote, false, &mut sequence, &mut text_bytes, frame,
                &mut |event| { staged.push(event); Ok(()) }, None)?;
        }
        journal
            .save_remote(&scope, &remote)
            .map_err(|e| invalid(&e))?;
        for event in staged { sink(event)?; }
        journal.save_config_receipt(&scope, "initial", &protocol::model_config_receipt(requested, result.get("configOptions")))
            .map_err(|e| invalid(&e))?;
        let options = result
            .get("configOptions")
            .ok_or_else(|| invalid("ACP 会话未提供可确认的模型配置。"))?;
        let set = protocol::set_model_params(&remote, requested, options).map_err(invalid)?;
        let result = transport
            .configure("session/set_config_option", set, deadline, |frame| {
                emit(
                    &scope,
                    &remote,
                    false,
                    &mut sequence,
                    &mut text_bytes,
                    frame,
                    &mut sink,
                    None,
                )
            })
            .await?;
        journal.save_config_receipt(&scope, "selected", &protocol::model_config_receipt(requested, result.get("configOptions")))
            .map_err(|e| invalid(&e))?;
        let selection = protocol::confirmed_model(requested, &result).map_err(invalid)?;
        journal
            .save_model(&scope, &selection)
            .map_err(|e| invalid(&e))?;
        Ok(Self {
            transport,
            journal,
            scope,
            remote,
            selection,
            sequence,
            text_bytes,
            deadline,
            prompt_image,
            image_handoff: None,
            awaiting_continuation: false,
            prompt_round: 0,
        })
    }

    /// 任务只发送一次；超时、失联、消息写入失败均保留 unknown，不能转到旧 LLM 循环。
    pub(super) async fn prompt(
        &mut self,
        text: &str,
        cancellation: Arc<crate::ChatTurnCancellation>,
        cancel_grace: Duration,
        mut sink: impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<TurnResult> {
        self.prompt_content(vec![json!({"type":"text","text":text})], cancellation, cancel_grace, &mut sink).await
    }

    /// 原图用标准 ACP 图片块发送；未声明 image 能力时在提交前拒绝。
    pub(super) async fn prompt_content(
        &mut self, content: Vec<Value>, cancellation: Arc<crate::ChatTurnCancellation>,
        cancel_grace: Duration, mut sink: impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<TurnResult> {
        self.prompt_segment(content, cancellation, cancel_grace, &mut sink, false).await
    }

    pub(super) fn set_image_handoff(&mut self, signal: Arc<AtomicBool>) {
        self.image_handoff = Some(signal);
    }

    /// 只续接刚让出生成的原会话；不重新 claim，也不从 terminal 复活。
    pub(super) async fn continue_with_images(
        &mut self, content: Vec<Value>, cancellation: Arc<crate::ChatTurnCancellation>,
        cancel_grace: Duration, mut sink: impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<TurnResult> {
        if !self.awaiting_continuation || !content.iter().any(|block| block["type"] == "image") {
            return Err(invalid("ACP 没有等待原图续轮；未发送。"));
        }
        self.journal.can_dispatch(&self.scope).map_err(|e|invalid(&e))?;
        self.prompt_segment(content, cancellation, cancel_grace, &mut sink, true).await
    }

    async fn prompt_segment(
        &mut self, content: Vec<Value>, cancellation: Arc<crate::ChatTurnCancellation>,
        cancel_grace: Duration, mut sink: impl FnMut(Event) -> io::Result<()>, continuation: bool,
    ) -> io::Result<TurnResult> {
        if !continuation && self.awaiting_continuation { return Err(invalid("ACP 等待原图续轮，不能提交新任务。")); }
        if content.is_empty() || content.iter().any(|block| match block["type"].as_str() {
            Some("text") => block["text"].as_str().is_none_or(|text| text.trim().is_empty()),
            Some("image") => !self.prompt_image || block["data"].as_str().is_none_or(str::is_empty)
                || !matches!(block["mimeType"].as_str(), Some("image/png" | "image/jpeg" | "image/webp")),
            _ => true,
        }) { return Err(invalid("ACP 提示内容无效或当前连接未声明图片能力；未发送。")); }
        let params = json!({"sessionId":self.remote,"prompt":content});
        let id = self.transport.id()?;
        let envelope = json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":params});
        let encoded = serde_json::to_vec(&envelope)?;
        // 只记录大小/数量，禁止记录截图、提示正文或账号信息。
        let payload_stage = if continuation { format!("image_payload_{}",self.prompt_round) } else { "prompt_payload".into() };
        self.journal.save_config_receipt(&self.scope, &payload_stage, &json!({
            "serialized_bytes":encoded.len(),"image_count":prompt_image_count(&envelope),
            "outgoing_limit_bytes":outgoing_limit(&envelope)
        })).map_err(|e|invalid(&e))?;
        validate_outgoing_bytes(&envelope, &encoded)?;
        self.prompt_round = self.prompt_round.checked_add(1).ok_or_else(||invalid("ACP 提示段计数已耗尽。"))?;
        if cancellation.is_requested() || tokio::time::Instant::now() >= self.deadline {
            self.journal
                .request_cancel(&self.scope)
                .map_err(|e| invalid(&e))?;
            self.journal
                .transition(&self.scope, &[if continuation { "submitted" } else { "prepared" }],
                    if continuation { "unknown" } else { "not_sent" }, None, false)
                .map_err(|e| invalid(&e))?;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ACP 提交前已取消或超时。",
            ));
        }
        if !continuation {
            self.journal.transition(&self.scope, &["prepared"], "submitted", None, true)
                .map_err(|e| invalid(&e))?;
        }
        self.awaiting_continuation = false;
        let mut guard = SubmittedGuard {
            journal: self.journal.clone(),
            scope: self.scope.clone(),
            cancellation: cancellation.clone(),
            reader: self.transport.reader_task.abort_handle(),
            finished: false,
        };
        let outcome = self
            .drive(id, params, cancellation, cancel_grace, &mut sink)
            .await;
        if outcome.is_err() {
            // 错误路径也会终止本地连接和进程，必须留下与 Drop 路径一致的停止意图。
            // 仍保留 unknown；不冒称远端取消确认，排空和工具结账后才能续接。
            self.journal.request_cancel(&self.scope).map_err(|e| invalid(&e))?;
            self.transport.invalidate();
            // 落盘失败返回更明确的错误；不假定状态提交成功。
            self.journal
                .transition(&self.scope, &["submitted"], "unknown", None, false)
                .map_err(|e| invalid(&e))?;
        }
        guard.finished = true;
        outcome
    }

    pub(super) fn set_deadline(&mut self, deadline:tokio::time::Instant) {
        self.deadline = deadline;
    }

    async fn drive(
        &mut self,
        id: u64,
        params: Value,
        cancellation: Arc<crate::ChatTurnCancellation>,
        cancel_grace: Duration,
        sink: &mut impl FnMut(Event) -> io::Result<()>,
    ) -> io::Result<TurnResult> {
        let request = json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":params});
        tokio::select! {
            biased;
            _=cancellation.cancelled()=>{
                self.journal.request_cancel(&self.scope).map_err(|e|invalid(&e))?;
                // 写到半帧时不能再追加 cancel；连接失效，由进程监督者回收。
                return Err(io::Error::new(io::ErrorKind::Interrupted,"ACP 发送中取消，执行结果未知。"));
            },
            _=tokio::time::sleep_until(self.deadline)=>{
                cancellation.request_timeout();
                self.journal.request_cancel(&self.scope).map_err(|e|invalid(&e))?;
                return Err(io::Error::new(io::ErrorKind::TimedOut,"ACP 提交超时，执行结果未知。"));
            },
            result=self.transport.write(&request)=>result?,
        }
        let mut cancelled = false;
        let mut grace_deadline = self.deadline;
        let mut count = 0;
        loop {
            let frame = tokio::select! {
                biased;
                _=cancellation.cancelled(),if !cancelled => {
                    self.cancel(&mut cancelled,&mut grace_deadline,cancel_grace).await?;
                    continue;
                },
                _=tokio::time::sleep_until(self.deadline),if !cancelled => {
                    cancellation.request_timeout();
                    self.cancel(&mut cancelled,&mut grace_deadline,cancel_grace).await?;
                    continue;
                },
                _=tokio::time::sleep_until(grace_deadline),if cancelled => {
                    return Err(io::Error::new(io::ErrorKind::TimedOut,"ACP 未确认取消终态，执行结果未知。"));
                },
                value=self.transport.frames.recv()=>value.ok_or_else(||invalid("ACP 任务断线，执行结果未知。"))??,
            };
            if frame.get("method").is_some() {
                count += 1;
                if count > MAX_NOTIFICATIONS {
                    return Err(invalid("ACP 任务通知超过限制。"));
                }
                if frame.get("id").is_some() {
                    let denied = protocol::denied_client_request(&frame).map_err(invalid)?;
                    tokio::time::timeout_at(
                        if cancelled {
                            grace_deadline
                        } else {
                            self.deadline
                        },
                        self.transport.write(&denied),
                    )
                    .await
                    .map_err(|_| invalid("ACP 拒绝回执写入超时。"))??;
                } else {
                    // 同一宿主连接/epoch 与唯一 remote session 才可投影；没有臆造远端 turn ID。
                    self.journal.binding(&self.scope).map_err(|e| invalid(&e))?;
                    emit(
                        &self.scope,
                        &self.remote,
                        false,
                        &mut self.sequence,
                        &mut self.text_bytes,
                        frame,
                        sink,
                        Some(&self.selection.requested),
                    )?;
                }
            } else {
                let result = response(frame, id)?;
                let reason = result
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .filter(|v| {
                        matches!(
                            *v,
                            "end_turn"
                                | "max_tokens"
                                | "max_turn_requests"
                                | "refusal"
                                | "cancelled"
                        )
                    })
                    .ok_or_else(|| invalid("ACP 未返回已知协议终态。"))?;
                // 取消和 end_turn 可能交叉到达，分别记录，不伪造取消确认。
                let image_handoff = reason == "end_turn" && !cancelled && !cancellation.is_requested()
                    && self.image_handoff.as_ref().is_some_and(|signal| signal.load(Ordering::Acquire));
                if image_handoff {
                    self.journal.can_dispatch(&self.scope).map_err(|e|invalid(&e))?;
                    self.awaiting_continuation = true;
                } else {
                    self.journal.transition(&self.scope, &["submitted"], "terminal", Some(reason), false)
                        .map_err(|e| invalid(&e))?;
                }
                return Ok(TurnResult {
                    stop_reason: reason.into(),
                    model: self.selection.clone(),
                    cancel_requested: cancelled,
                    image_handoff,
                    process_drained: false,
                });
            }
        }
    }

    async fn cancel(
        &mut self,
        cancelled: &mut bool,
        deadline: &mut tokio::time::Instant,
        grace: Duration,
    ) -> io::Result<()> {
        self.journal
            .request_cancel(&self.scope)
            .map_err(|e| invalid(&e))?;
        *cancelled = true;
        *deadline = tokio::time::Instant::now() + grace.min(Duration::from_secs(10));
        tokio::time::timeout_at(*deadline,self.transport.write(&json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":self.remote}})))
            .await.map_err(|_|invalid("ACP 取消通知写入超时。"))?
    }
}

fn emit(
    scope: &ExecutionScope,
    remote: &str,
    replay: bool,
    sequence: &mut u64,
    text_bytes: &mut usize,
    frame: Value,
    sink: &mut impl FnMut(Event) -> io::Result<()>,
    requested: Option<&str>,
) -> io::Result<()> {
    if frame.get("method").and_then(Value::as_str) != Some("session/update") {
        return Ok(());
    }
    if frame["params"]["sessionId"].as_str() != Some(remote) {
        return Err(invalid("ACP 通知来自另一会话。"));
    }
    let source = &frame["params"]["update"];
    let update = if replay && source["content"]["type"] == "image"
        && matches!(source["sessionUpdate"].as_str(), Some("user_message_chunk" | "agent_message_chunk")) {
        // 恢复时的旧图片只记类型，不复制像素或作为本次观察；不能阻断含原图的同一会话续接。
        Update::Diagnostic { name: "replayed_image".into() }
    } else { project(source)? };
    if let Update::Config { options } = &update {
        let (_, effective, _) = protocol::model_config(options).map_err(invalid)?;
        if requested.is_some_and(|value| value != effective) {
            return Err(invalid("ACP 在任务中改变模型，生效身份不再匹配。"));
        }
    }
    // 对所有投影计入总量，不能通过大量工具/配置通知绕过单纯文本上限。
    *text_bytes = text_bytes.saturating_add(serde_json::to_vec(&update)?.len());
    if *text_bytes > MAX_TEXT_BYTES {
        return Err(invalid("ACP 累计事件内容超过限制。"));
    }
    *sequence = sequence
        .checked_add(1)
        .ok_or_else(|| invalid("ACP 事件序号已耗尽。"))?;
    sink(Event {
        scope: scope.clone(),
        sequence: *sequence,
        replay,
        update,
    })
}

#[cfg(test)]
mod tests {
    use super::super::journal::tests::{scope, setup};
    use super::*;
    #[test]
    fn replayed_images_are_metadata_and_never_current_observations() {
        let frame = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"remote",
            "update":{"sessionUpdate":"user_message_chunk","content":{"type":"image","mimeType":"image/png","data":"private-image"}}}});
        let mut events = Vec::new(); let mut sequence = 0; let mut bytes = 0;
        emit(&scope("image-replay"),"remote",true,&mut sequence,&mut bytes,frame.clone(),
            &mut |event|{events.push(event);Ok(())},None).unwrap();
        assert!(events[0].replay);
        assert!(matches!(&events[0].update,Update::Diagnostic { name } if name == "replayed_image"));
        assert!(!serde_json::to_string(&events[0]).unwrap().contains("private-image"));
        assert!(emit(&scope("image-replay"),"other",true,&mut sequence,&mut bytes,frame.clone(),&mut |_|Ok(()),None).is_err());
        assert!(emit(&scope("image-replay"),"remote",false,&mut sequence,&mut bytes,frame,&mut |_|Ok(()),None).is_err());
    }
    #[test]
    fn image_prompt_capacity_does_not_expand_other_frames_or_text() {
        let mut prompt = json!({"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"actual","prompt":[
            {"type":"text","text":"本轮图像比较"},{"type":"image","mimeType":"image/png","data":"A".repeat(MAX_FRAME_BYTES)}]}});
        let encoded = serde_json::to_vec(&prompt).unwrap();
        assert!(encoded.len() > MAX_FRAME_BYTES);
        assert!(validate_outgoing_bytes(&prompt, &encoded).is_ok());
        assert_eq!(prompt_image_count(&prompt), 1);
        assert!(validate_outgoing_bytes(&prompt, &vec![0;MAX_IMAGE_PROMPT_BYTES]).is_err());
        prompt["method"] = json!("session/update");
        assert!(validate_outgoing_bytes(&prompt, &encoded).is_err());
        prompt["method"] = json!("session/prompt");
        prompt["params"]["prompt"][1]["mimeType"] = json!("invalid/type");
        assert!(validate_outgoing_bytes(&prompt, &encoded).is_err());
        prompt["params"]["prompt"][1]["mimeType"] = json!("image/png");
        prompt["params"]["prompt"][0]["text"] = json!("A".repeat(MAX_FRAME_BYTES));
        assert!(validate_outgoing_bytes(&prompt, &serde_json::to_vec(&prompt).unwrap()).is_err());
        assert_eq!(MAX_FRAME_BYTES, 1024 * 1024);
    }
    use tokio::io::{BufReader, DuplexStream, ReadHalf, WriteHalf};
    type Reader = BufReader<ReadHalf<DuplexStream>>;
    type Writer = WriteHalf<DuplexStream>;
    fn pair() -> (SessionTransport<Writer>, Reader, Writer) {
        let (a, b) = tokio::io::duplex(8192);
        let (r, w) = tokio::io::split(a);
        let (sr, sw) = tokio::io::split(b);
        (
            SessionTransport::new(BufReader::new(r), w),
            BufReader::new(sr),
            sw,
        )
    }
    fn config() -> Value {
        json!([{"id":"model","category":"model","type":"select","currentValue":"actual","options":[{"value":"actual"}]}])
    }
    #[tokio::test]
    async fn image_continuation_keeps_one_remote_attempt_and_never_revives_terminal() {
        let (_dir, journal, binding) = setup();
        let claim = journal.claim(scope("image-continuation"), &binding).unwrap();
        let (transport, mut reader, mut writer) = pair();
        let signal = Arc::new(AtomicBool::new(false));
        let peer_signal = signal.clone();
        let peer = tokio::spawn(async move {
            let request = read_frame(&mut reader).await.unwrap();
            reply(&mut writer,&request["id"],json!({"protocolVersion":1,"agentCapabilities":{"promptCapabilities":{"image":true}}})).await;
            let request = read_frame(&mut reader).await.unwrap();
            assert_eq!(request["method"],"session/new");
            reply(&mut writer,&request["id"],json!({"sessionId":"remote","configOptions":config()})).await;
            let request = read_frame(&mut reader).await.unwrap();
            reply(&mut writer,&request["id"],json!({"configOptions":config()})).await;
            let first = read_frame(&mut reader).await.unwrap();
            assert_eq!(first["method"],"session/prompt");
            assert_eq!(first["params"]["sessionId"],"remote");
            peer_signal.store(true,Ordering::Release);
            reply(&mut writer,&first["id"],json!({"stopReason":"end_turn"})).await;
            let second = read_frame(&mut reader).await.unwrap();
            assert_eq!(second["method"],"session/prompt");
            assert_eq!(second["params"]["sessionId"],first["params"]["sessionId"]);
            assert_ne!(second["id"],first["id"]);
            assert_eq!(prompt_image_count(&second),1);
            reply(&mut writer,&second["id"],json!({"stopReason":"end_turn"})).await;
        });
        let mut service = SessionService::connect(transport,journal.clone(),claim.clone(),"actual",vec![],
            tokio::time::Instant::now()+Duration::from_secs(30), |_|Ok(())).await.unwrap();
        service.set_image_handoff(signal.clone());
        let cancellation = Arc::new(crate::ChatTurnCancellation::new());
        let images = vec![json!({"type":"image","mimeType":"image/png","data":"YQ=="})];
        assert!(service.continue_with_images(images.clone(),cancellation.clone(),Duration::from_secs(1),|_|Ok(())).await.is_err());
        let first = service.prompt("原任务",cancellation.clone(),Duration::from_secs(1),|_|Ok(())).await.unwrap();
        assert!(first.image_handoff);
        assert_eq!(journal.status(&claim).unwrap().state,"submitted");
        assert!(journal.claim(scope("other"), &binding).is_err());
        signal.store(false,Ordering::Release);
        let final_turn = service.continue_with_images(images.clone(),cancellation.clone(),Duration::from_secs(1),|_|Ok(())).await.unwrap();
        assert!(!final_turn.image_handoff);
        assert_eq!(journal.status(&claim).unwrap().state,"terminal");
        assert!(service.continue_with_images(images,cancellation,Duration::from_secs(1),|_|Ok(())).await.is_err());
        peer.await.unwrap();
    }
    #[tokio::test]
    async fn new_notifications_are_staged_until_actual_session_identity_is_confirmed() {
        for notification_session in ["remote", "wrong-room"] {
            let (_dir, journal, binding) = setup();
            let claim = journal.claim(scope("new-notification"), &binding).unwrap();
            let (transport, mut reader, mut writer) = pair();
            let peer = tokio::spawn(async move {
                let request = read_frame(&mut reader).await.unwrap();
                reply(&mut writer,&request["id"],json!({"protocolVersion":1,"agentCapabilities":{}})).await;
                let request = read_frame(&mut reader).await.unwrap();
                let mut bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update",
                    "params":{"sessionId":notification_session,"update":{"sessionUpdate":"usage_update","used":0,"size":100}}})).unwrap();
                bytes.push(b'\n');writer.write_all(&bytes).await.unwrap();
                reply(&mut writer,&request["id"],json!({"sessionId":"remote","configOptions":config()})).await;
                if notification_session == "remote" {
                    let request = read_frame(&mut reader).await.unwrap();
                    reply(&mut writer,&request["id"],json!({"configOptions":config()})).await;
                }
            });
            let mut events = vec![];
            let result = SessionService::connect(transport,journal.clone(),claim.clone(),"actual",vec![],
                tokio::time::Instant::now()+Duration::from_secs(3),|e|{events.push(e);Ok(())}).await;
            if notification_session == "remote" {
                assert!(result.is_ok());assert_eq!(events.len(),1);assert_eq!(events[0].scope,claim);
            } else {
                assert!(result.is_err());assert!(events.is_empty());
                assert!(journal.binding(&claim).unwrap().remote_session_id.is_none());
            }
            peer.await.unwrap();
        }
    }
    async fn reply(writer: &mut Writer, id: &Value, result: Value) {
        let mut bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"result":result})).unwrap();
        bytes.push(b'\n');
        writer.write_all(&bytes).await.unwrap();
    }
    async fn handshake(reader: &mut Reader, writer: &mut Writer, load: bool) {
        let request = read_frame(reader).await.unwrap();
        assert_eq!(request["method"], "initialize");
        reply(
            writer,
            &request["id"],
            json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true}}),
        )
        .await;
        let request = read_frame(reader).await.unwrap();
        assert_eq!(
            request["method"],
            if load { "session/load" } else { "session/new" }
        );
        if load {
            assert_eq!(request["params"]["sessionId"], "remote");
            writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"remote\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"history\"}}}}\n").await.unwrap();
        }
        reply(
            writer,
            &request["id"],
            json!({"sessionId":"remote","configOptions":config()}),
        )
        .await;
        let request = read_frame(reader).await.unwrap();
        assert_eq!(request["method"], "session/set_config_option");
        reply(writer, &request["id"], json!({"configOptions":config()})).await;
    }
    #[tokio::test]
    async fn actual_model_text_usage_and_remote_tool_observation_are_distinct() {
        let (_dir, journal, binding) = setup();
        let claim = journal.claim(scope("a1"), &binding).unwrap();
        let (transport, mut reader, mut writer) = pair();
        let peer = tokio::spawn(async move {
            handshake(&mut reader, &mut writer, false).await;
            let prompt = read_frame(&mut reader).await.unwrap();
            assert_eq!(prompt["method"], "session/prompt");
            for update in [
                json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"你好"}}),
                json!({"sessionUpdate":"tool_call","toolCallId":"remote-tool","status":"completed"}),
                json!({"sessionUpdate":"usage_update","used":7,"size":100}),
            ] {
                let mut frame=serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"remote","update":update}})).unwrap();
                frame.push(b'\n');
                writer.write_all(&frame).await.unwrap();
            }
            reply(&mut writer, &prompt["id"], json!({"stopReason":"end_turn"})).await;
        });
        let mut events = vec![];
        let mut service = SessionService::connect(
            transport,
            journal.clone(),
            claim.clone(),
            "actual",
            vec![],
            // 此项验证消息分类，不验证时限；CI并行落盘可能占用数秒。
            tokio::time::Instant::now() + Duration::from_secs(30),
            |e| {
                events.push(e);
                Ok(())
            },
        )
        .await
        .unwrap();
        service.set_deadline(tokio::time::Instant::now() + Duration::from_secs(30));
        let result = service
            .prompt(
                "hi",
                Arc::new(crate::ChatTurnCancellation::new()),
                Duration::from_secs(1),
                |e| {
                    events.push(e);
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert_eq!(result.model.effective.as_deref(), Some("actual"));
        assert!(!result.process_drained);
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].sequence, 3);
        assert!(events.iter().all(|e| e.scope == claim && !e.replay));
        assert!(matches!(&events[1].update, Update::ToolObservation { .. }));
        assert!(matches!(
            &events[2].update,
            Update::Usage { cost: None, .. }
        ));
        assert_eq!(journal.status(&claim).unwrap().state, "terminal");
        assert!(journal.claim(scope("a2"), &binding).is_err());
        peer.await.unwrap();
    }
    #[tokio::test]
    async fn cancellation_does_not_drop_partial_frame_or_claim_process_exit() {
        let (_dir, journal, binding) = setup();
        let claim = journal.claim(scope("a1"), &binding).unwrap();
        let (transport, mut reader, mut writer) = pair();
        let cancellation = Arc::new(crate::ChatTurnCancellation::new());
        let c = cancellation.clone();
        let peer = tokio::spawn(async move {
            handshake(&mut reader, &mut writer, false).await;
            let prompt = read_frame(&mut reader).await.unwrap();
            writer
                .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":")
                .await
                .unwrap();
            c.request();
            let cancel = read_frame(&mut reader).await.unwrap();
            assert_eq!(cancel["method"], "session/cancel");
            assert!(cancel.get("id").is_none());
            writer.write_all(b"{\"sessionId\":\"remote\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"late\"}}}}\n").await.unwrap();
            writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"permission\",\"method\":\"session/request_permission\"}\n").await.unwrap();
            let denied = read_frame(&mut reader).await.unwrap();
            assert_eq!(denied["result"]["outcome"]["outcome"], "cancelled");
            reply(
                &mut writer,
                &prompt["id"],
                json!({"stopReason":"cancelled"}),
            )
            .await;
        });
        let mut service = SessionService::connect(
            transport,
            journal.clone(),
            claim.clone(),
            "actual",
            vec![],
            // 本项验证半帧取消，不验证握手时限；并行 CI 落盘不能抢先触发配置超时。
            tokio::time::Instant::now() + Duration::from_secs(30),
            |_| Ok(()),
        )
        .await
        .unwrap();
        service.set_deadline(tokio::time::Instant::now() + Duration::from_secs(30));
        let result = service
            .prompt("hi", cancellation, Duration::from_secs(10), |_| Ok(()))
            .await
            .unwrap();
        assert!(result.cancel_requested);
        assert_eq!(result.stop_reason, "cancelled");
        assert!(!result.process_drained);
        assert!(journal.can_dispatch(&claim).is_err());
        peer.await.unwrap();
    }
    #[tokio::test]
    async fn disconnect_and_cross_session_updates_are_unknown_and_never_retried() {
        for wrong_session in [false, true] {
            let (_dir, journal, binding) = setup();
            let claim = journal.claim(scope("a1"), &binding).unwrap();
            let (transport, mut reader, mut writer) = pair();
            let peer = tokio::spawn(async move {
                handshake(&mut reader, &mut writer, false).await;
                let request = read_frame(&mut reader).await.unwrap();
                assert_eq!(request["method"], "session/prompt");
                if wrong_session {
                    writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"other\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"wrong\"}}}}\n").await.unwrap();
                }
            });
            let mut service = SessionService::connect(
                transport,
                journal.clone(),
                claim.clone(),
                "actual",
                vec![],
                // 本项检查已发送后的失联，不让慢磁盘上的握手记账耗尽发送期限。
                tokio::time::Instant::now() + Duration::from_secs(30),
                |_| Ok(()),
            )
            .await
            .unwrap();
            assert!(
                service
                    .prompt(
                        "hi",
                        Arc::new(crate::ChatTurnCancellation::new()),
                        Duration::from_millis(50),
                        |_| panic!("不能投影错误会话")
                    )
                    .await
                    .is_err()
            );
            assert_eq!(journal.status(&claim).unwrap().state, "unknown");
            assert!(journal.claim(scope("a2"), &binding).is_err());
            peer.await.unwrap();
        }
    }
    #[tokio::test]
    async fn load_replays_history_separately_and_keeps_remote_binding() {
        let (_dir, journal, binding) = setup();
        let a = journal.claim(scope("a1"), &binding).unwrap();
        journal.save_remote(&a, "remote").unwrap();
        journal
            .transition(&a, &["prepared"], "not_sent", None, false)
            .unwrap();
        journal.record_drained(&a).unwrap();
        let b = journal.claim(scope("a2"), &binding).unwrap();
        let (transport, mut reader, mut writer) = pair();
        let peer = tokio::spawn(async move {
            handshake(&mut reader, &mut writer, true).await;
            let request = read_frame(&mut reader).await.unwrap();
            reply(
                &mut writer,
                &request["id"],
                json!({"stopReason":"end_turn"}),
            )
            .await;
        });
        let mut events = vec![];
        let mut service = SessionService::connect(
            transport,
            journal,
            b,
            "actual",
            vec![],
            tokio::time::Instant::now() + Duration::from_secs(3),
            |e| {
                events.push(e);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].replay);
        service
            .prompt(
                "next",
                Arc::new(crate::ChatTurnCancellation::new()),
                Duration::from_secs(1),
                |_| Ok(()),
            )
            .await
            .unwrap();
        peer.await.unwrap();
    }
    #[test]
    fn aggregate_limit_also_bounds_non_text_events() {
        let mut scope = scope("events");
        scope.owner_epoch = 1;
        scope.generation = 1;
        let mut sequence = 0;
        let mut bytes = 0;
        let mut emitted = 0;
        let frame = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"remote",
            "update":{"sessionUpdate":"tool_call","toolCallId":"x".repeat(600_000),"status":"completed"}}});
        let mut rejected = false;
        for _ in 0..20 {
            if emit(
                &scope,
                "remote",
                false,
                &mut sequence,
                &mut bytes,
                frame.clone(),
                &mut |_| {
                    emitted += 1;
                    Ok(())
                },
                None,
            )
            .is_err()
            {
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        assert!(emitted < 20);
        assert_eq!(sequence, emitted);
    }
    #[test]
    fn malformed_remote_tool_status_cannot_become_a_host_execution() {
        let event = project(
            &json!({"sessionUpdate":"tool_call","toolCallId":"remote","status":"completed"}),
        )
        .unwrap();
        assert!(matches!(event, Update::ToolObservation { .. }));
        assert!(project(&json!({"sessionUpdate":"tool_call","status":"completed"})).is_err());
    }
    #[tokio::test]
    async fn dropped_prompt_wait_persists_unknown_and_revokes_tool_dispatch() {
        let (_dir, journal, binding) = setup();
        let claim = journal.claim(scope("a1"), &binding).unwrap();
        let (transport, mut reader, mut writer) = pair();
        let (submitted_tx, submitted_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let peer = tokio::spawn(async move {
            handshake(&mut reader, &mut writer, false).await;
            let request = read_frame(&mut reader).await.unwrap();
            assert_eq!(request["method"], "session/prompt");
            submitted_tx.send(()).unwrap();
            let _ = release_rx.await;
        });
        let mut service = SessionService::connect(
            transport,
            journal.clone(),
            claim.clone(),
            "actual",
            vec![],
            tokio::time::Instant::now() + Duration::from_secs(30),
            |_| Ok(()),
        )
        .await
        .unwrap();
        let cancellation = Arc::new(crate::ChatTurnCancellation::new());
        // 明确等待对端收到提示后丢弃 future，避免把 80ms 调度竞速当成提交证据。
        let mut prompt = Box::pin(service.prompt(
            "hi", cancellation.clone(), Duration::from_millis(50), |_| Ok(()),
        ));
        tokio::select! {
            result = &mut prompt => panic!("丢弃前提示意外结束：{result:?}"),
            received = submitted_rx => received.unwrap(),
        }
        drop(prompt);
        assert!(cancellation.is_requested());
        let status = journal.status(&claim).unwrap();
        assert_eq!(status.state, "unknown");
        assert!(status.cancel_requested);
        assert!(journal.can_dispatch(&claim).is_err());
        assert!(journal.claim(scope("a2"), &binding).is_err());
        release_tx.send(()).unwrap();
        peer.await.unwrap();
    }
    #[tokio::test]
    async fn unacknowledged_cancel_and_mid_turn_model_change_remain_unknown() {
        for model_change in [false, true] {
            let (_dir, journal, binding) = setup();
            let claim = journal.claim(scope("a1"), &binding).unwrap();
            let (transport, mut reader, mut writer) = pair();
            let cancellation = Arc::new(crate::ChatTurnCancellation::new());
            let c = cancellation.clone();
            let peer = tokio::spawn(async move {
                handshake(&mut reader, &mut writer, false).await;
                let _ = read_frame(&mut reader).await.unwrap();
                if model_change {
                    let changed = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"remote","update":{"sessionUpdate":"config_option_update","configOptions":[{"category":"model","id":"model","type":"select","currentValue":"other","options":[{"value":"other"}]}]}}});
                    let mut bytes = serde_json::to_vec(&changed).unwrap();
                    bytes.push(b'\n');
                    writer.write_all(&bytes).await.unwrap();
                } else {
                    c.request();
                    let cancel = read_frame(&mut reader).await.unwrap();
                    assert_eq!(cancel["method"], "session/cancel");
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            });
            let mut service = SessionService::connect(
                transport,
                journal.clone(),
                claim.clone(),
                "actual",
                vec![],
                tokio::time::Instant::now() + Duration::from_secs(3),
                |_| Ok(()),
            )
            .await
            .unwrap();
            assert!(
                service
                    .prompt("hi", cancellation, Duration::from_millis(50), |_| panic!(
                        "不能投影非请求的模型配置"
                    ))
                    .await
                    .is_err()
            );
            assert_eq!(journal.status(&claim).unwrap().state, "unknown");
            assert!(journal.claim(scope("a2"), &binding).is_err());
            peer.await.unwrap();
        }
    }
    #[test]
    fn unknown_cost_is_not_zero_and_malformed_usage_is_rejected() {
        assert!(project(&json!({"sessionUpdate":"usage_update","used":1})).is_err());
        assert!(project(&json!({"sessionUpdate":"usage_update","used":1,"size":2,"cost":{"amount":-1,"currency":"USD"}})).is_err());
        assert!(matches!(
            project(&json!({"sessionUpdate":"usage_update","used":1,"size":2})).unwrap(),
            Update::Usage { cost: None, .. }
        ));
    }
}
