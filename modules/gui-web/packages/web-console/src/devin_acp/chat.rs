//! 聊天室真实会话。续接远端上下文，复用宿主消息流与显式选择的宿主工具。
use super::{
    journal::{Binding, Journal},
    process::ManagedProcess,
    protocol::ExecutionScope,
    session::{Event, SessionService, Update},
};
use crate::{AgentSessionDto, ChatTurnCancellation, ContextAssembly, FrozenParentContext};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;

pub(super) const CLI_VERSION: &str = "devin 3000.10.48 (fcf7ba39)";
const CLI_SHA256: &str = "d8877ebf699499b1d0957a9fdd99cb596013fc3bfeb782b756496bbcf527bb1b";

pub(super) fn error(message: impl Into<String>) -> api::ApiError {
    api::ApiError::UnsupportedCapability {
        capability: message.into(),
    }
}

pub(crate) fn valid_model_id(model: &str) -> bool {
    !model.trim().is_empty() && model.len() <= 1024 && !model.chars().any(char::is_control)
}

pub(crate) fn reasoning_resolution(model: &str, requested: &str) -> crate::ReasoningResolutionDto {
    let effort = super::discovery::model_effort(model).unwrap_or("auto");
    let mut supported_options = vec![crate::ReasoningOptionDto {
        value: "auto".into(), label: "所选模型默认".into(), note: None,
    }];
    if effort != "auto" {
        supported_options.push(crate::ReasoningOptionDto {
            value: effort.into(), label: effort.into(),
            note: Some("此精确模型变体的档位；其它档位需重新获取账号模型目录。".into()),
        });
    }
    crate::ReasoningResolutionDto {
        requested: requested.into(),
        effective: effort.into(),
        status: "pending".into(),
        strategy: "acp_model_variant".into(),
        protocol: "devin_acp".into(),
        reason: "思考档位随精确模型变体选择；发送前由 ACP 核对生效 ID，其它参数沿用 CLI 默认。".into(),
        supported_options,
    }
}

/// HTTP 与 ACP 共用现有聊天室事件消费；完成事实仍由各自协议证明。
pub(crate) enum ModelStream {
    Http(api::MessageStream),
    Devin(TextStream),
}
impl ModelStream {
    pub(crate) async fn next_event(&mut self) -> Result<Option<api::StreamEvent>, api::ApiError> {
        match self {
            Self::Http(stream) => stream.next_event().await,
            Self::Devin(stream) => stream.next().await,
        }
    }
    pub(crate) fn protocol_completed(&self) -> bool {
        match self {
            Self::Http(stream) => {
                stream.inflight_status().terminal_fact
                    == Some(api::TerminationFact::ProtocolCompletion)
            }
            Self::Devin(stream) => stream.completed,
        }
    }
}

enum Item {
    Delta(api::StreamEvent),
    Finished,
    Failed(String),
}
pub(crate) struct TextStream {
    events: mpsc::Receiver<Item>,
    cancellation: Arc<ChatTurnCancellation>,
    completed: bool,
    ended: bool,
}
impl TextStream {
    async fn next(&mut self) -> Result<Option<api::StreamEvent>, api::ApiError> {
        if self.ended {
            return Ok(None);
        }
        match self.events.recv().await {
            Some(Item::Delta(delta)) => Ok(Some(delta)),
            Some(Item::Finished) => {
                self.completed = true;
                self.ended = true;
                Ok(None)
            }
            Some(Item::Failed(reason)) => {
                self.ended = true;
                Err(error(reason))
            }
            None => {
                self.ended = true;
                Err(error("Devin 连接结束但没有完成回执；未重发本轮。"))
            }
        }
    }
}
impl Drop for TextStream {
    fn drop(&mut self) {
        // 关闭网页、停止或根时限结束只提出取消；独立任务仍负责 ACP 回执与进程树排空。
        if !self.ended {
            self.cancellation.request();
        }
    }
}

pub(super) fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn controlled_config() -> Value {
    json!({"version":1,"auto_update":false,"subagents_enabled":false,"notify":"never",
        "read_config_from":{"agents_standard":false,"cursor":false,"windsurf":false,
            "claude":false,"copilot":false,"opencode":false,"zed":false},
        "hooks":{},"permissions":{"allow":[],"ask":[],
            "deny":["read","write","edit","exec","grep","glob","fetch","mcp","mcp__*"]}})
}

fn review_enabled(settings: &crate::SessionModelLimitOverride) -> Result<bool, String> {
    if settings.enable_llm_tools != Some(true) { return Ok(false); }
    if settings.llm_tool_exposure.as_deref() != Some("whitelist")
        || !settings.tool_allowlist.as_ref().is_some_and(|tools| !tools.is_empty()
            && tools.iter().all(|name| super::bridge::REVIEW_TOOLS.contains(&name.as_str())
                || name == "computer_use_perform" && settings.computer_use_enabled == Some(true))) {
        return Err("Devin 仅开放显式选择的工程只读工具与 Computer Use；其它工具尚未就绪。".into());
    }
    Ok(true)
}

fn absent_or_empty(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir()
        && std::fs::read_dir(path)
            .map_err(|_| "无法核对 Devin 扩展目录。")?
            .next()
            .is_none()
    {
        return Ok(());
    }
    Err("检测到全局 Devin MCP、Hooks 或插件；文本会话未启动，请先完成隔离适配。".into())
}

/// --config 只替换用户配置，MCP 和系统扩展另有来源；不能假定空 hooks 会覆盖它们。
pub(super) fn check_ambient_extensions() -> Result<(), String> {
    let user = PathBuf::from(std::env::var_os("APPDATA").ok_or("无法定位 Devin 用户配置。")?)
        .join("devin");
    let system =
        PathBuf::from(std::env::var_os("PROGRAMDATA").ok_or("无法定位系统配置。")?).join("Devin");
    for root in [&user, &system] {
        for name in [
            "mcp_config.json",
            "hooks",
            "plugins",
            "managed-settings.json",
        ] {
            absent_or_empty(&root.join(name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
fn prepare_directory(key: &str) -> Result<(PathBuf, PathBuf), String> {
    prepare_directory_with_tools(key, false)
}

pub(super) fn prepare_directory_with_tools(key: &str, review: bool) -> Result<(PathBuf, PathBuf), String> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("无法定位会话缓存。")?)
        .join("CoolzhuAgent/devin-text-v1")
        .join(key);
    if !base.is_absolute() {
        return Err("Devin 会话缓存必须为绝对路径。".into());
    }
    let cwd = base.join("workspace");
    std::fs::create_dir_all(cwd.join(".devin")).map_err(|_| "无法创建 Devin 文本会话目录。")?;
    // 项目根标记阻止 CLI 向上发现用户工程的配置；这里不包含用户工程文件。
    std::fs::create_dir_all(cwd.join(".git")).map_err(|_| "无法固定 Devin 文本项目根。")?;
    let config = base.join("config.json");
    let mut value = controlled_config();
    if review {
        // 仅此受控服务提供三项只读工具；全局 MCP、Hooks 仍在启动前排除。
        value["permissions"] = json!({"allow":["mcp__coolzhu-agent__*"],"ask":[],
            "deny":["read","write","edit","exec","grep","glob","fetch"]});
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| "文本配置编码失败。")?;
    std::fs::write(&config, &bytes).map_err(|_| "文本配置写入失败。")?;
    let project = json!({"permissions":value["permissions"],"read_config_from":value["read_config_from"],"hooks":{}});
    for name in ["config.json", "config.local.json"] {
        std::fs::write(
            cwd.join(".devin").join(name),
            serde_json::to_vec(&project).unwrap(),
        )
        .map_err(|_| "文本项目配置写入失败。")?;
    }
    for name in ["mcp_config.json", "mcp_config.local.json"] {
        std::fs::write(cwd.join(".devin").join(name), br#"{"mcpServers":{}}"#)
            .map_err(|_| "文本 MCP 配置写入失败。")?;
    }
    Ok((cwd, config))
}

#[cfg(test)]
fn publish(event: Event, tx: &mpsc::Sender<Item>) -> io::Result<()> {
    publish_with_tools(event, tx, false)
}

fn publish_with_tools(event: Event, tx: &mpsc::Sender<Item>, review: bool) -> io::Result<()> {
    if event.replay {
        return Ok(());
    }
    match event.update {
        Update::Text { text, thought, .. } => {
            let delta = if thought {
                api::ContentBlockDelta::ThinkingDelta { thinking: text }
            } else {
                api::ContentBlockDelta::TextDelta { text }
            };
            tx.try_send(Item::Delta(api::StreamEvent::ContentBlockDelta(
                api::ContentBlockDeltaEvent {
                    index: u32::from(thought),
                    delta,
                },
            )))
            .map_err(|_| io::Error::other("聊天室已停止接收或消息队列超限。"))
        }
        // 文本会话绝不将远端工具自述转成宿主工具请求。
        Update::ToolObservation { .. } if review => Ok(()),
        Update::ToolObservation { .. } => Err(io::Error::other(
            "文本会话出现工具调用；已停止，未执行宿主工具。",
        )),
        _ => Ok(()),
    }
}

/// 前端取消后不再投影增量，但独立 worker 必须继续读取协议终态并排空进程。
/// 正常运行时的队列溢出仍是错误，不能用关闭消费者掩盖消息丢失。
fn publish_to_consumer(
    event: Event,
    tx: &mpsc::Sender<Item>,
    review: bool,
    cancellation: &ChatTurnCancellation,
) -> io::Result<()> {
    if cancellation.is_requested() && tx.is_closed() {
        return Ok(());
    }
    publish_with_tools(event, tx, review)
}

/// 聊天与内部请求共享实际 CLI 身份核对；模型资格由随后 ACP 配置回执确认。
pub(super) async fn checked_binary(model: &str) -> Result<PathBuf, api::ApiError> {
    if model.trim().is_empty() { return Err(error("模型 ID 不能为空。")); }
    let binary = super::discovery::binary().map_err(error)?;
    if tokio::fs::metadata(&binary)
        .await
        .map_err(|_| error("无法读取 CLI 身份。"))?
        .len()
        > 512 * 1024 * 1024
    {
        return Err(error("CLI 文件超过已验证大小范围。"));
    }
    let cli_bytes = tokio::fs::read(&binary)
        .await
        .map_err(|_| error("无法核对固定 CLI 文件。"))?;
    if digest(&cli_bytes) != CLI_SHA256 {
        return Err(error("Devin CLI 与已验证文件不同，需要重新验证后接入。"));
    }
    drop(cli_bytes);
    let sandbox = tempfile::tempdir().map_err(|_| error("无法准备模型核对目录。"))?;
    let version = super::transport::run_readonly(
        &binary,
        &["--version"],
        sandbox.path(),
        Duration::from_secs(5),
    )
    .await
    .map_err(error)?;
    if std::str::from_utf8(&version).ok().map(str::trim) != Some(CLI_VERSION) {
        return Err(error("Devin CLI 版本未通过文本会话验收。"));
    }
    // 获取完整目录只用于设置页发现模型。每次生成重复联网取目录会阻塞聊天和
    // CU 内部规划；SessionService 仍必须从远端配置选项选择精确 ID 并确认生效，
    // 不存在或不能选择的模型在发送提示之前失败，不接受别名或自动换模。
    Ok(binary)
}

pub(crate) async fn start(
    agent: &AgentSessionDto,
    assembly: &ContextAssembly,
    parent: Option<&FrozenParentContext>,
) -> Result<ModelStream, api::ApiError> {
    if !cfg!(windows) {
        return Err(error("Devin 文本会话目前只验证了 Windows。"));
    }
    let parent = parent.ok_or_else(|| error("缺少聊天室运行身份，未启动 Devin。"))?;
    if !matches!(parent.entry, "chat-send" | "chat-send-stream") || parent.goal_phase.is_some() {
        return Err(error(
            "Devin 当前开放聊天室文本会话；Goal、接力与子 Agent 尚未就绪。",
        ));
    }
    let room = parent
        .room_id
        .as_deref()
        .ok_or_else(|| error("缺少聊天室身份。"))?;
    let turn = parent
        .public_turn_id
        .as_deref()
        .ok_or_else(|| error("缺少聊天回合身份。"))?;
    let run = parent
        .parent_run_id
        .as_deref()
        .ok_or_else(|| error("缺少持久运行身份。"))?;
    let db = parent
        .runtime_db_path
        .as_deref()
        .ok_or_else(|| error("缺少冻结消息库路径。"))?;
    crate::validate_frozen_parent_relations(db, parent)
        .map_err(|reason| error(format!("聊天室运行身份核对失败：{reason:?}")))?;
    let room_enabled = crate::chat_room_capabilities_sqlite(db, room)
        .map_err(|_| error("聊天室能力不可用。"))?
        .0;
    if !crate::real_llm_enabled() || !room_enabled {
        return Err(error(
            "请先启用真实模型和当前聊天室的模型调用；未生成模拟回复。",
        ));
    }
    crate::agent_session_backend::AgentSessionBackend::DevinAcp
        .validate(crate::session_model_settings_for(&agent.id).backend_kind)
        .map_err(error)?;
    let cancellation = crate::CHAT_CANCELLATION
        .try_with(Arc::clone)
        .map_err(|_| error("缺少本轮停止控制，未启动 Devin。"))?;
    if cancellation.is_requested() {
        return Err(error("本轮已停止，未启动 Devin。"));
    }
    if !valid_model_id(&agent.model) {
        return Err(error("请从 Devin 账号目录选择有效的模型 ID。"));
    }
    let settings = crate::session_model_settings_for(&agent.id);
    let review = review_enabled(&settings).map_err(error)?;
    let computer = settings.computer_use_enabled == Some(true);
    check_ambient_extensions().map_err(error)?;
    let binary = checked_binary(&agent.model).await?;
    let reset = crate::session_context_reset_floor(&agent.id);
    let key = digest(
        &serde_json::to_vec(&(parent.workspace_id.as_str(), room, &agent.id, reset)).unwrap(),
    );
    let instructions = if review {
        format!("你在 coolzhuagent 聊天室中工作。只使用 coolzhu-agent MCP 服务实际声明的工具。工程根目录为 {}。读取源码必须分段；电脑/浏览器操作只通过 computer_use_perform 提交。若回执为 running，必须用 computer_use_wait 等待同一 job_id 直到最终回执；等待不算重复提交，不重新调用 perform，不在运行中结束本轮。使用用户指定的目标、限制和成功标准，不用原生工具替代，不修改或切换默认规划模型。不允许原生文件工具、命令、写入、联网或子 Agent。工具执行事实以宿主回执为准，不把工具自述当作执行或成功证据。", parent.workspace_id.as_str())
    } else {
        "你在 coolzhuagent 聊天室中进行纯文本会话。文件、命令、联网工具、MCP 和子 Agent 均不可用；不要尝试调用工具，不要声称执行了操作。".into()
    };
    if cancellation.is_requested() {
        return Err(error("本轮已停止，未提交 Devin 提示。"));
    }
    let (cwd, config) = prepare_directory_with_tools(&key, review).map_err(error)?;
    // 消息、运行台账和 ACP 绑定使用同一冻结数据库；上下文重置使用独立绑定域，旧记录仍保留。
    let journal = Journal::open(db).map_err(error)?;
    let scope = ExecutionScope {
        workspace_id: parent.workspace_id.as_str().into(),
        room_id: room.into(),
        agent_id: agent.id.clone(),
        lane: String::new(),
        run_id: run.into(),
        turn_id: turn.into(),
        attempt_id: crate::random_hex_identifier(24, "ACP 回合").map_err(error)?,
        owner_epoch: 0,
        generation: 0,
    };
    let binding = Binding {
        remote_session_id: None,
        cwd: cwd.to_string_lossy().into(),
        cli_identity: CLI_VERSION.into(),
        // 同一上下文和精确模型保持远端会话；重置/切换模型时明确换域，不重复注入历史。
        context_digest: format!("host-incremental-v3:{reset:?}:{}", agent.model),
    };
    journal
        .rotate_idle_context(&scope, &binding)
        .map_err(error)?;
    let claim = journal.claim(scope, &binding).map_err(error)?;
    let context = match super::context::prepare(&journal, &claim, assembly, &instructions) {
        Ok(context) => context,
        Err(reason) => {
            journal.transition(&claim, &["prepared"], "not_sent", None, false).map_err(error)?;
            journal.record_drained(&claim).map_err(error)?;
            return Err(error(reason));
        }
    };
    let remaining = parent
        .root_budget
        .as_ref()
        .map_or(Duration::from_secs(900), |budget| budget.remaining());
    let deadline = tokio::time::Instant::now() + remaining;
    let model = agent.model.clone();
    let (tx, events) = mpsc::channel(256);
    let token = cancellation.clone();
    let review_parent = if review {
        let mut frozen = parent.clone();
        frozen.session_backend_model = Some(agent.clone());
        Some(frozen)
    } else { None };
    tokio::spawn(async move {
        let outcome = execute(
            &binary,
            &cwd,
            &config,
            &model,
            journal.clone(),
            claim.clone(),
            &context,
            deadline,
            token,
            &tx,
            review_parent,
            computer,
        )
        .await;
        let item = match outcome {
            Ok(()) => Item::Finished,
            Err(reason) => Item::Failed(reason),
        };
        let _ = tx.send(item).await;
    });
    Ok(ModelStream::Devin(TextStream {
        events,
        cancellation,
        completed: false,
        ended: false,
    }))
}

async fn execute(
    binary: &Path,
    cwd: &Path,
    config: &Path,
    model: &str,
    journal: Journal,
    claim: ExecutionScope,
    context: &super::context::PreparedContext,
    deadline: tokio::time::Instant,
    cancellation: Arc<ChatTurnCancellation>,
    tx: &mpsc::Sender<Item>,
    review_parent: Option<FrozenParentContext>,
    computer: bool,
) -> Result<(), String> {
    let spawned =
        ManagedProcess::spawn_text_cli(binary, cwd, config, model, journal.clone(), claim.clone())
            .await;
    let (mut process, transport) = match spawned {
        Ok(value) => value,
        Err(reason) => {
            journal.transition(&claim, &["prepared"], "not_sent", None, false)?;
            journal.record_drained(&claim)?;
            return Err(reason);
        }
    };
    let outcome = async {
        let bridge = if let Some(parent) = review_parent {
            let bridge = if computer {
                super::bridge::ToolBridge::capture(parent, claim.clone(), journal.clone(), cancellation.clone())?
            } else {
                super::bridge::ToolBridge::capture_review(parent, claim.clone(), journal.clone(), cancellation.clone())?
            };
            Some(bridge.start().await?)
        } else { None };
        let review = bridge.is_some();
        let connect = SessionService::connect(
            transport,
            journal.clone(),
            claim.clone(),
            model,
            bridge.as_ref().map(|bridge| vec![bridge.config()]).unwrap_or_default(),
            deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
            |event| publish_to_consumer(event, tx, review, &cancellation),
        );
        let mut session = tokio::select! { biased;
            _=cancellation.cancelled()=>return Err("Devin 配置期间已停止，未发送本轮。".into()),
            result=connect=>result.map_err(|e|e.to_string())?,
        };
        // 配置阶段上限与生成总时限分开，避免每轮只剩 30 秒。
        session.set_deadline(deadline);
        context.record(&journal, &claim)?;
        let consumer_cancellation = cancellation.clone();
        let result = session
            .prompt(&context.prompt, cancellation, Duration::from_secs(5), |event| {
                publish_to_consumer(event, tx, review, &consumer_cancellation)
            })
            .await
            .map_err(|e| e.to_string())?;
        if result.stop_reason != "end_turn" {
            return Err(format!(
                "Devin 本轮结束：{}；未自动重试。",
                result.stop_reason
            ));
        }
        if result.model.effective.as_deref() != Some(model) {
            return Err("Devin 生效模型未确认。".into());
        }
        if bridge.as_ref().is_some_and(|bridge|bridge.has_pending()) {
            return Err("Devin 在 CU 任务尚未返回最终回执时结束本轮；已撤销待执行动作，不能记为完成。".into());
        }
        Ok(())
    }
    .await;
    let final_state = journal.status(&claim).and_then(|status| {
        if status.state == "prepared" {
            journal.transition(&claim, &["prepared"], "not_sent", None, false)
        } else {
            Ok(())
        }
    });
    let drained = process.drain().await;
    final_state?;
    drained?;
    outcome
}

pub(crate) async fn response(
    agent: &AgentSessionDto,
    prompt: &str,
    history: &[crate::PersistedChatMessage],
    roster: Option<&crate::ChatRosterResponse>,
    room: Option<&str>,
    parent: Option<&FrozenParentContext>,
) -> crate::AgentModelResponse {
    let turn = parent
        .and_then(|p| p.public_turn_id.clone())
        .unwrap_or_default();
    let mut response = crate::AgentModelResponse {
        execution_failed: true,
        answer_text: String::new(),
        reasoning_text: String::new(),
        tool_requests: vec![],
        tool_write_executed: false,
        model_tool_calls_executed: true,
        turn_id: turn.clone(),
        used_real_model: false,
        diagnostic_note: None,
        context_footer: None,
        context_usage: None,
    };
    let outcome = async {
        let (mut stream, _) = crate::stream_agent_model(
            agent,
            prompt,
            &[],
            history,
            roster,
            &turn,
            room,
            parent.and_then(|p| p.parent_run_id.as_deref()),
            parent,
        )
        .await?;
        while let Some(event) = stream.next_event().await? {
            if let api::StreamEvent::ContentBlockDelta(event) = event {
                match event.delta {
                    api::ContentBlockDelta::TextDelta { text } => {
                        response.answer_text.push_str(&text)
                    }
                    api::ContentBlockDelta::ThinkingDelta { thinking } => {
                        response.reasoning_text.push_str(&thinking)
                    }
                    _ => {}
                }
            }
        }
        if !stream.protocol_completed() || response.answer_text.trim().is_empty() {
            return Err(error("Devin 未返回完整可见回复。"));
        }
        Ok::<_, api::ApiError>(())
    }
    .await;
    match outcome {
        Ok(()) => {
            response.execution_failed = false;
            response.used_real_model = true;
        }
        Err(reason) => {
            response.diagnostic_note = Some(reason.to_string());
            response
                .answer_text
                .push_str(&format!("\n\n本轮未完成：{reason}"));
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "需用户授权、真实已登录 CLI 和免费 SWE-2，仅验证合成文件的工具拒绝"]
    async fn real_text_mode_denies_native_file_and_command_tools() {
        assert_eq!(
            std::env::var("COOLZHU_DEVIN_REAL_SMOKE").as_deref(),
            Ok("1")
        );
        check_ambient_extensions().unwrap();
        let binary = super::super::discovery::binary().unwrap();
        assert_eq!(digest(&std::fs::read(&binary).unwrap()), CLI_SHA256);
        let key = digest(
            crate::random_hex_identifier(24, "原生拒绝测试")
                .unwrap()
                .as_bytes(),
        );
        let (cwd, config) = prepare_directory(&key).unwrap();
        let catalog = super::super::transport::run_readonly(
            &binary,
            &["models", "list", "--format", "json"],
            &cwd,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert!(super::super::discovery::catalog_contains(
            &serde_json::from_slice::<Value>(&catalog).unwrap(),
            "swe-2-medium"
        ));
        let root = cwd.parent().unwrap();
        let secret = crate::random_hex_identifier(16, "合成内容").unwrap();
        let sentinel = root.join("synthetic-read-test.txt");
        let write = root.join("synthetic-write-test.txt");
        let exec = root.join("synthetic-exec-test.txt");
        std::fs::write(&sentinel, &secret).unwrap();
        let journal = Journal::open(&root.join("probe.sqlite3")).unwrap();
        let binding = Binding {
            remote_session_id: None,
            cwd: cwd.to_string_lossy().into(),
            cli_identity: CLI_VERSION.into(),
            context_digest: "native-denial-probe".into(),
        };
        let claim = journal
            .claim(
                super::super::journal::tests::scope("native-denial"),
                &binding,
            )
            .unwrap();
        let (mut process, transport) = ManagedProcess::spawn_text_cli(
            &binary,
            &cwd,
            &config,
            "swe-2-medium",
            journal.clone(),
            claim.clone(),
        )
        .await
        .unwrap();
        let mut events = vec![];
        let mut service = SessionService::connect(
            transport,
            journal.clone(),
            claim.clone(),
            "swe-2-medium",
            vec![],
            tokio::time::Instant::now() + Duration::from_secs(90),
            |event| {
                events.push(event);
                Ok(())
            },
        )
        .await
        .unwrap();
        let prompt=format!("这是工具权限拒绝验收，文件全部是合成测试数据。请实际尝试 read 读取 {}；尝试 write 向 {} 写入 TEST；尝试 exec 用 cmd /c echo TEST 写入 {}。每项最多尝试一次，不找替代工具，不发网络请求。被拒绝后停止该项，最后只回复：权限受限。",
            sentinel.display(),write.display(),exec.display());
        let outcome = service
            .prompt(
                &prompt,
                Arc::new(ChatTurnCancellation::new()),
                Duration::from_secs(5),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .await;
        process.drain().await.unwrap();
        assert_eq!(outcome.unwrap().stop_reason, "end_turn");
        assert!(!write.exists() && !exec.exists());
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), secret);
        let reply: String = events
            .iter()
            .filter_map(|event| match &event.update {
                Update::Text {
                    text,
                    thought: false,
                    ..
                } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!reply.contains(&secret));
        let observations: Vec<_> = events
            .iter()
            .filter_map(|event| match &event.update {
                Update::ToolObservation {
                    tool_call_id,
                    status,
                } => Some((tool_call_id, status)),
                _ => None,
            })
            .collect();
        assert!(!observations
            .iter()
            .any(|(_, status)| status.as_deref() == Some("completed")));
        assert!(
            observations
                .iter()
                .any(|(_, status)| status.as_deref() == Some("failed")),
            "没有实际拒绝通知，不能宣布工具拒绝验证通过"
        );
        let report = json!({"cli_version":CLI_VERSION,"model":"swe-2-medium","cost_tier":"Free",
            "read_content_disclosed":false,"native_write_created":false,"native_exec_created":false,
            "tool_notifications":observations.len(),"failed_notifications":observations.iter().filter(|(_,status)|status.as_deref()==Some("failed")).count(),
            "process_drained":journal.status(&claim).unwrap().process_drained,"reply":reply});
        let out = std::env::current_dir().unwrap().join("tmp");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            out.join("devin-text-native-denial-report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn only_actual_catalog_models_are_admitted_without_price_or_family_allowlist() {
        let catalog = json!({"families":[{"variants":[{"model_uid":"swe-2-medium","cost_tier":"Free"},
            {"model_uid":"swe-2-high","cost_tier":"Paid"},{"model_uid":"paid-model","cost_tier":"Free"}]}]});
        assert!(super::super::discovery::catalog_contains(&catalog, "swe-2-medium"));
        assert!(super::super::discovery::catalog_contains(&catalog, "swe-2-high"));
        assert!(super::super::discovery::catalog_contains(&catalog, "paid-model"));
        assert!(!super::super::discovery::catalog_contains(&catalog, "missing-model"));
        assert!(!super::super::discovery::catalog_contains(&json!({}), "swe-2-medium"));
    }
    #[tokio::test]
    async fn replay_never_becomes_new_reply_and_eof_never_means_completion() {
        let (tx, rx) = mpsc::channel(4);
        let scope = super::super::journal::tests::scope("projection");
        publish(
            Event {
                scope: scope.clone(),
                sequence: 1,
                replay: true,
                update: Update::Text {
                    text: "旧消息".into(),
                    thought: false,
                    message_id: None,
                },
            },
            &tx,
        )
        .unwrap();
        publish(
            Event {
                scope,
                sequence: 2,
                replay: false,
                update: Update::Text {
                    text: "新消息".into(),
                    thought: false,
                    message_id: None,
                },
            },
            &tx,
        )
        .unwrap();
        drop(tx);
        let mut stream = TextStream {
            events: rx,
            cancellation: Arc::new(ChatTurnCancellation::new()),
            completed: false,
            ended: false,
        };
        assert!(matches!(
            stream.next().await.unwrap(),
            Some(api::StreamEvent::ContentBlockDelta(_))
        ));
        assert!(stream.next().await.is_err());
        assert!(!stream.completed);
    }
    #[tokio::test]
    async fn completion_requires_drained_receipt_and_drop_requests_cancellation() {
        let (tx, rx) = mpsc::channel(4);
        let token = Arc::new(ChatTurnCancellation::new());
        let mut stream = TextStream {
            events: rx,
            cancellation: token.clone(),
            completed: false,
            ended: false,
        };
        tx.send(Item::Finished).await.unwrap();
        assert!(!stream.completed);
        assert!(stream.next().await.unwrap().is_none());
        drop(stream);
        assert!(!token.is_requested());
        let (tx, rx) = mpsc::channel(4);
        let mut failed = TextStream {
            events: rx,
            cancellation: token.clone(),
            completed: false,
            ended: false,
        };
        tx.send(Item::Failed("服务已失败且排空".into()))
            .await
            .unwrap();
        assert!(failed.next().await.is_err());
        drop(failed);
        assert!(!token.is_requested(), "确定失败不得被 Drop 改成用户中断");
        let (_tx, rx) = mpsc::channel(4);
        drop(TextStream {
            events: rx,
            cancellation: token.clone(),
            completed: false,
            ended: false,
        });
        assert!(token.is_requested());
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let event = || Event {
            scope: super::super::journal::tests::scope("consumer-stopped"),
            sequence: 1,
            replay: false,
            update: Update::Text {
                text: "取消后的迟到增量".into(),
                thought: false,
                message_id: None,
            },
        };
        assert!(publish_to_consumer(event(), &tx, false, &token).is_ok());
        assert!(publish_to_consumer(event(), &tx, false, &ChatTurnCancellation::new()).is_err());
    }
    #[test]
    fn reset_preserves_old_binding_and_cannot_unlock_unknown() {
        let (_dir, journal, mut binding) = super::super::journal::tests::setup();
        let old = journal
            .claim(
                super::super::journal::tests::scope("before-reset"),
                &binding,
            )
            .unwrap();
        journal.save_remote(&old, "old-remote").unwrap();
        journal
            .transition(&old, &["prepared"], "not_sent", None, false)
            .unwrap();
        journal.record_drained(&old).unwrap();
        binding.context_digest = "text-v1:Some(123)".into();
        let requested = super::super::journal::tests::scope("after-reset");
        journal.rotate_idle_context(&requested, &binding).unwrap();
        let new = journal.claim(requested, &binding).unwrap();
        assert!(journal.binding(&new).unwrap().remote_session_id.is_none());
        assert!(journal.save_remote(&new, "old-remote").is_err());
        journal.save_remote(&new, "new-remote").unwrap();
        journal
            .transition(&new, &["prepared"], "unknown", None, false)
            .unwrap();
        journal.record_drained(&new).unwrap();
        binding.context_digest = "text-v1:Some(456)".into();
        assert!(journal.rotate_idle_context(&new, &binding).is_err());
        assert_eq!(journal.status(&new).unwrap().state, "unknown");
        // 用户主动取消且监督者已确认退出后，新用户回合可建立独立连接。
        // 旧未知事实保留，旧远端和旧工具派发身份仍不可复用。
        journal.request_cancel(&new).unwrap();
        journal.rotate_idle_context(&new, &binding).unwrap();
        assert_eq!(journal.status(&new).unwrap().state, "unknown");
        assert!(journal.can_dispatch(&new).is_err());
        let fresh = journal.claim(super::super::journal::tests::scope("after-cancel"), &binding).unwrap();
        assert!(journal.save_remote(&fresh, "new-remote").is_err());
        journal.save_remote(&fresh, "fresh-remote").unwrap();
    }
    #[test]
    fn tool_observation_and_ambient_extensions_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        absent_or_empty(&dir.path().join("missing")).unwrap();
        std::fs::write(dir.path().join("mcp_config.json"), "{}").unwrap();
        assert!(absent_or_empty(&dir.path().join("mcp_config.json")).is_err());
        let (tx, _) = mpsc::channel(4);
        assert!(publish(
            Event {
                scope: super::super::journal::tests::scope("tool"),
                sequence: 1,
                replay: false,
                update: Update::ToolObservation {
                    tool_call_id: "tool-1".into(),
                    status: Some("completed".into())
                }
            },
            &tx
        )
        .is_err());
        let config = controlled_config();
        for tool in [
            "read", "write", "edit", "exec", "grep", "glob", "fetch", "mcp__*",
        ] {
            assert!(config["permissions"]["deny"]
                .as_array()
                .unwrap()
                .contains(&json!(tool)));
        }
    }
}
