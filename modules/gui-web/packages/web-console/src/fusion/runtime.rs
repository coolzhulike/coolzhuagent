//! Fusion Team 运行时：发送接纳、宿主编排器、角色轮次、房间消息与 HTTP API。
//! 编排由程序驱动：模型只能产出文本与 team_* 工具调用，调度/权限/预算/取消归宿主。
use super::super::*;
use super::*;
use axum::extract::{Path as AxumPath, Query};
use axum::http::StatusCode;
use axum::response::sse::Event;
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone)]
pub(super) struct FusionTeamSettings {
    pub host_web_access: bool,
    pub task_timeout_ms: u64,
    pub max_briefs_per_turn: u32,
    pub max_role_turns: u32,
}

impl FusionTeamSettings {
    fn parse(encoded: &str) -> Self {
        let value = serde_json::from_str::<JsonValue>(encoded).unwrap_or_else(|_| json!({}));
        Self {
            host_web_access: value
                .get("host_web_access")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false),
            task_timeout_ms: value
                .get("task_timeout_ms")
                .and_then(JsonValue::as_u64)
                .unwrap_or(1_800_000)
                .clamp(60_000, 86_400_000),
            max_briefs_per_turn: value
                .get("max_briefs_per_turn")
                .and_then(JsonValue::as_u64)
                .unwrap_or(25)
                .clamp(1, 100) as u32,
            max_role_turns: value
                .get("max_role_turns")
                .and_then(JsonValue::as_u64)
                .unwrap_or(40)
                .clamp(1, 200) as u32,
        }
    }
}

/// 发送前解析的团队成员会话（querier 缺省回落到 executor）。
struct ResolvedTeam {
    team: store::FusionTeamRow,
    settings: FusionTeamSettings,
    host: AgentSessionDto,
    executor: AgentSessionDto,
    querier: AgentSessionDto,
    querier_is_separate: bool,
}

impl ResolvedTeam {
    fn agent_for(&self, role: FusionRole) -> &AgentSessionDto {
        match role {
            FusionRole::Host => &self.host,
            FusionRole::Executor => &self.executor,
            FusionRole::Querier => &self.querier,
        }
    }
}

fn agent_sendable(agent: &AgentSessionDto) -> bool {
    agent.selectable
        && agent.enabled
        && agent_session_backend::AgentSessionBackend::for_provider(&agent.provider)
            == agent_session_backend::AgentSessionBackend::LlmHttp
}

fn find_agent(session_id: &str) -> Option<AgentSessionDto> {
    default_agent_sessions()
        .into_iter()
        .find(|agent| agent.id == session_id)
}

/// 配置残缺时报具体原因；querier 未配置合法（回落 executor）。
fn resolve_team(db_path: &Path, team_id: &str) -> Result<ResolvedTeam, ApiError> {
    let team = store::fusion_team_get(db_path, team_id)?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Fusion Team 不存在或已删除"))?;
    if !team.enabled {
        return Err(api_error(StatusCode::BAD_REQUEST, "Fusion Team 未启用"));
    }
    let settings = FusionTeamSettings::parse(&team.settings_json);
    let lookup = |session_id: Option<&String>, role: FusionRole| -> Result<AgentSessionDto, ApiError> {
        let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) else {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                &format!("Fusion Team 未配置{}", role.display_name()),
            ));
        };
        match find_agent(session_id) {
            Some(agent) if agent_sendable(&agent) => Ok(agent),
            Some(agent) => Err(api_error(
                StatusCode::BAD_REQUEST,
                &format!("Fusion {} 绑定的会话 `{}` 不可发送或不是模型会话", role.display_name(), agent.id),
            )),
            None => Err(api_error(
                StatusCode::BAD_REQUEST,
                &format!("Fusion {} 绑定的会话 `{session_id}` 已不存在", role.display_name()),
            )),
        }
    };
    let host = lookup(team.host_session_id.as_ref(), FusionRole::Host)?;
    let executor = lookup(team.executor_session_id.as_ref(), FusionRole::Executor)?;
    let (querier, querier_is_separate) = match team
        .querier_session_id
        .as_ref()
        .filter(|id| !id.trim().is_empty())
    {
        Some(session_id) => (lookup(Some(session_id), FusionRole::Querier)?, true),
        None => (executor.clone(), false),
    };
    Ok(ResolvedTeam {
        team,
        settings,
        host,
        executor,
        querier,
        querier_is_separate,
    })
}

// ── 活动运行登记表（同房间同团队只允许一个编排器；后续消息排队为主持事件）─────

struct ActiveFusionRun {
    cancellation: Arc<ChatTurnCancellation>,
    pending_events: Mutex<Vec<String>>,
}

static ACTIVE_FUSION_RUNS: OnceLock<Mutex<HashMap<String, Arc<ActiveFusionRun>>>> = OnceLock::new();

fn fusion_runs() -> &'static Mutex<HashMap<String, Arc<ActiveFusionRun>>> {
    ACTIVE_FUSION_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn run_key(room_id: &str, team_id: &str) -> String {
    format!("{room_id}|{team_id}")
}

fn drain_events(run: &ActiveFusionRun) -> Vec<String> {
    run.pending_events
        .lock()
        .map(|mut events| events.split_off(0))
        .unwrap_or_default()
}

/// 已有运行则把新消息投为其主持事件并返回 true；否则返回 false（需新起编排器）。
fn push_event_to_active_run(room_id: &str, team_id: &str, event: String) -> bool {
    let run = fusion_runs()
        .lock()
        .ok()
        .and_then(|runs| runs.get(&run_key(room_id, team_id)).cloned());
    let Some(run) = run else {
        return false;
    };
    if run.cancellation.is_requested() {
        return false;
    }
    let pushed = if let Ok(mut events) = run.pending_events.lock() {
        events.push(event);
        true
    } else {
        false
    };
    pushed
}

// ── 发送接纳 ────────────────────────────────────────────────────────────────

pub(crate) struct FusionPrepared {
    db_path: PathBuf,
    workspace_id: String,
    room_id: String,
    resolved: ResolvedTeam,
    turn_id: String,
    run_id: String,
    claim_token: String,
    cancellation: Arc<ChatTurnCancellation>,
    root_budget: root_execution_budget::RootExecutionBudget,
    parent: FrozenParentContext,
    guard: ChatTurnGuard,
    user_message: ChatMessageDto,
    user_text: String,
    dedup_key: Option<u64>,
}

/// 接纳 fusion 发送：幂等指纹 → 校验 → 持久化用户消息 → 登记 turn/run。
pub(crate) async fn admit(payload: SendMessageRequest) -> Result<FusionPrepared, ApiError> {
    let workspace_pin = Arc::new(
        workspace_activity::pin_workspace().map_err(|message| api_error(StatusCode::CONFLICT, &message))?,
    );
    let workspace_id = workspace_identity(&active_workspace_path());
    if payload
        .expected_workspace_id
        .as_deref()
        .is_some_and(|expected| expected != workspace_id)
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "服务当前工程已变化，未提交本轮；请重新选择工程和会话",
        ));
    }
    let dedup_key = chat_request_fingerprint(&payload);
    if let Some(key) = dedup_key {
        if let Some(reason) = check_chat_request_duplicate(key) {
            return Err(api_error(StatusCode::CONFLICT, &reason));
        }
    }
    match admit_inner(payload, workspace_pin, workspace_id).await {
        Ok(mut prepared) => {
            prepared.dedup_key = dedup_key;
            Ok(prepared)
        }
        Err(error) => {
            if let Some(key) = dedup_key {
                release_chat_request_fingerprint(key);
            }
            Err(error)
        }
    }
}

async fn admit_inner(
    payload: SendMessageRequest,
    workspace_pin: Arc<workspace_activity::WorkspacePin>,
    workspace_id: String,
) -> Result<FusionPrepared, ApiError> {
    let team_id = fusion_team_id_of(&payload.target_agent_ids)?;
    let db_path = default_session_sqlite_path();
    let resolved = resolve_team(&db_path, &team_id)?;
    let room_id = payload
        .chat_room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_CHAT_ROOM_ID)
        .to_string();
    {
        let store = session_store()
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储不可用"))?;
        if !store.state.chat_rooms.iter().any(|room| room.id == room_id) {
            return Err(api_error(StatusCode::NOT_FOUND, "聊天室不存在"));
        }
    }
    let (room_enabled, _, _) = chat_room_capabilities_sqlite(&db_path, &room_id)
        .unwrap_or((true, true, true));
    if !room_enabled {
        return Err(api_error(StatusCode::BAD_REQUEST, "当前聊天室已停用真实模型"));
    }
    let mut attachments = payload.attachments.clone().unwrap_or_default();
    attachment_text::freeze(&mut attachments, &attachment_store_dir(), false)?;
    let user_text = payload.text.trim().to_string();
    if user_text.is_empty() && attachments.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "消息内容不能为空"));
    }
    let mut content = user_text.clone();
    attachment_text::append(&mut content, &attachments);
    let user_message = ChatMessageDto {
        id: format!("msg-{}-user", unix_timestamp_millis()),
        author: String::new(),
        role: "user".to_string(),
        target: resolved.team.name.clone(),
        content: content.clone(),
        kind: if attachments.is_empty() {
            "text".to_string()
        } else {
            "multimedia".to_string()
        },
        attachments,
    };
    persist_chat_dispatch(&room_id, payload.session_id.as_deref(), &[], vec![user_message.clone()])?;

    let turn_id = new_chat_turn_id();
    let (run_id, claim_token) = new_runtime_run_identifiers()
        .map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    create_chat_runtime_run_sqlite(
        &db_path,
        &run_id,
        &claim_token,
        &workspace_id,
        payload.session_id.as_deref(),
        &room_id,
        &turn_id,
    )
    .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("创建运行记录失败：{error}")))?;
    let cancellation = register_chat_turn_with_run(
        &turn_id,
        Some(run_id.clone()),
        payload.session_id.clone(),
        room_id.clone(),
    );
    mark_live_turn_active(&turn_id, &room_id, cancellation.clone());
    let mut guard = ChatTurnGuard::new_runtime(
        turn_id.clone(),
        run_id.clone(),
        claim_token.clone(),
        db_path.clone(),
    );
    guard = guard.with_workspace_pin(workspace_pin);
    if let Some(scope) = chat_run_scope_for(
        &workspace_id,
        &room_id,
        payload.session_id.as_deref(),
        &turn_id,
        &run_id,
    ) {
        guard = guard.with_scope(scope);
    }
    let root_budget = root_execution_budget::RootExecutionBudget::establish(
        resolved.settings.task_timeout_ms,
    );
    let mut parent = frozen_parent_context_at(
        "fusion",
        &workspace_id,
        Some(room_id.as_str()),
        payload.session_id.as_deref(),
        Some(turn_id.as_str()),
        Some(run_id.as_str()),
    )
    .ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法冻结 Fusion 运行上下文"))?;
    // 不携带 host_model_snapshots——角色工具表由 fusion scope 精确控制，快照会绕过过滤。
    parent.root_budget = Some(root_budget.clone());
    parent.runtime_db_path = Some(db_path.clone());

    // 同房间同团队已有编排器在跑：本轮只把新消息投为其主持事件，不起新编排器。
    let _ = push_event_to_active_run(
        &room_id,
        &team_id,
        format!("【用户新消息】{user_text}"),
    );

    Ok(FusionPrepared {
        db_path,
        workspace_id,
        room_id,
        resolved,
        turn_id,
        run_id,
        claim_token,
        cancellation,
        root_budget,
        parent,
        guard,
        user_message,
        user_text,
        dedup_key: None,
    })
}

// ── 非流式入口 ──────────────────────────────────────────────────────────────

pub(crate) async fn send(payload: SendMessageRequest) -> ApiResult<axum::Json<SendMessageResponse>> {
    let mut prepared = admit(payload).await?;
    let user_message = prepared.user_message.clone();
    let target = format!("{}{}", FUSION_TARGET_PREFIX, prepared.resolved.team.id);
    let started = start_chat_runtime_run_sqlite(
        &prepared.db_path,
        &prepared.run_id,
        &prepared.claim_token,
    )
    .unwrap_or(false);
    if !started {
        let _ = prepared.guard.finish(ChatTurnStatus::Failed);
        return Err(api_error(StatusCode::CONFLICT, "Fusion 运行启动冲突"));
    }
    spawn_orchestrator(prepared);
    Ok(axum::Json(SendMessageResponse {
        execution_failed: false,
        runtime: None,
        accepted_agent_ids: vec![target],
        messages: vec![user_message],
        tasks: Vec::new(),
        notes: vec!["Fusion Team 已接管本轮".to_string()],
        request_bindings: Vec::new(),
    }))
}

// ── 流式入口：立即 done，编排器后台运行，角色消息经聊天室 SSE 增量同步到达 ──────

pub(crate) fn stream_events(
    prepared: FusionPrepared,
) -> impl futures_core::Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        let turn_id = prepared.turn_id.clone();
        let run_id = prepared.run_id.clone();
        let room_id = prepared.room_id.clone();
        let target = format!("{}{}", FUSION_TARGET_PREFIX, prepared.resolved.team.id);
        yield Ok(sse_json_event(
            "started",
            &ChatStreamStarted {
                turn_id: turn_id.clone(),
                chat_room_id: room_id.clone(),
                run_id: Some(run_id.clone()),
            },
        ));
        yield Ok(sse_json_event("message", &prepared.user_message));
        let started = start_chat_runtime_run_sqlite(
            &prepared.db_path,
            &prepared.run_id,
            &prepared.claim_token,
        )
        .unwrap_or(false);
        if !started {
            yield Ok(sse_json_event(
                "error",
                &ChatStreamError { message: "Fusion 运行启动冲突".to_string() },
            ));
            yield Ok(sse_json_event(
                "done",
                &ChatStreamDone {
                    turn_id,
                    run_id: Some(run_id),
                    status: ChatTurnStatus::Failed,
                    accepted_agent_ids: Vec::new(),
                    tasks: Vec::new(),
                },
            ));
            return;
        }
        yield Ok(sse_json_event(
            "message",
            &room_message_for(
                &room_id,
                "Fusion Team",
                "fusion-status",
                "Fusion Team 已接管本轮；角色发言与任务进展将陆续到达，可用停止按钮中断。",
            ),
        ));
        spawn_orchestrator(prepared);
        yield Ok(sse_json_event(
            "done",
            &ChatStreamDone {
                turn_id,
                run_id: Some(run_id),
                status: ChatTurnStatus::Completed,
                accepted_agent_ids: vec![target],
                tasks: Vec::new(),
            },
        ));
    }
}

// ── 编排器 ──────────────────────────────────────────────────────────────────

fn spawn_orchestrator(prepared: FusionPrepared) {
    tokio::spawn(async move {
        let mut prepared = prepared;
        let key = run_key(&prepared.room_id, &prepared.resolved.team.id);
        let run = Arc::new(ActiveFusionRun {
            cancellation: prepared.cancellation.clone(),
            pending_events: Mutex::new(Vec::new()),
        });
        let replaced = fusion_runs()
            .lock()
            .ok()
            .and_then(|mut runs| runs.insert(key.clone(), run.clone()));
        if let Some(old) = replaced {
            // 同键旧运行残留：取消其编排器（旧 orchestrator 会在轮次边界退出）。
            old.cancellation.request();
        }
        let cancellation = prepared.cancellation.clone();
        let budget = prepared.root_budget.clone();
        let db_path = prepared.db_path.clone();
        let room_id = prepared.room_id.clone();
        let team_id = prepared.resolved.team.id.clone();
        let dedup_key = prepared.dedup_key;
        let status = match await_chat_turn(
            cancellation.as_ref(),
            root_execution_budget::scope(
                budget,
                CHAT_CANCELLATION.scope(
                    Arc::clone(&cancellation),
                    orchestrate_inner(&prepared, &run),
                ),
            ),
        )
        .await
        {
            Ok(status) => status,
            Err(_) => ChatTurnStatus::Interrupted,
        };
        let _ = store::fusion_brief_cancel_running(&db_path, &room_id, &team_id);
        if let Ok(mut runs) = fusion_runs().lock() {
            if runs.get(&key).map(|existing| Arc::ptr_eq(existing, &run)) == Some(true) {
                runs.remove(&key);
            }
        }
        prepared.guard.finish(status);
        if let Some(key) = dedup_key {
            release_chat_request_fingerprint(key);
        }
    });
}

#[derive(Default)]
struct OrchestrateStats {
    briefs: u32,
    role_turns: u32,
}

static FUSION_MESSAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn room_message_for(_room_id: &str, author: &str, kind: &str, content: &str) -> ChatMessageDto {
    ChatMessageDto {
        id: format!(
            "msg-{}-{}",
            unix_timestamp_millis(),
            FUSION_MESSAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
        author: author.to_string(),
        role: "assistant".to_string(),
        target: "聊天".to_string(),
        content: content.to_string(),
        kind: kind.to_string(),
        attachments: Vec::new(),
    }
}

fn emit(prepared: &FusionPrepared, author: &str, kind: &str, content: &str) {
    emit_room_message(&prepared.room_id, author, kind, content);
}

/// 角色工具执行路径也会用它（team_plan 的计划卡片）。
pub(super) fn emit_room_message(room_id: &str, author: &str, kind: &str, content: &str) {
    let message = room_message_for(room_id, author, kind, content);
    if let Ok(mut store) = session_store().lock() {
        let _ = store.append_chat_room_messages(room_id, vec![message]);
    }
}

fn task_open(prepared: &FusionPrepared) -> Option<store::FusionTaskRow> {
    store::fusion_task_latest_open(
        &prepared.db_path,
        &prepared.room_id,
        &prepared.resolved.team.id,
    )
    .ok()
    .flatten()
}

async fn orchestrate_inner(prepared: &FusionPrepared, run: &Arc<ActiveFusionRun>) -> ChatTurnStatus {
    // 上次进程遗留的 running brief 收敛为 interrupted（不重放）；
    // 受影响 brief 转为恢复事件交给主持（继续/放弃由主持决定，不自动重放）。
    let interrupted = store::fusion_briefs_by_status(
        &prepared.db_path,
        &prepared.room_id,
        &prepared.resolved.team.id,
        "running",
    )
    .unwrap_or_default();
    let swept = store::fusion_briefs_sweep_interrupted(&prepared.db_path).unwrap_or(0);
    let settings = &prepared.resolved.settings;
    let mut stats = OrchestrateStats::default();
    // 首轮主持事件：用户消息（线程里由 run_role_turn 落库）。
    let mut host_events = vec![format!("【用户消息】{}", prepared.user_text)];
    if swept > 0 {
        // 中断 brief 的执行者工作项回 pending；待主持决定后重新派发。
        let _ = store::fusion_work_items_sweep_running(&prepared.db_path);
        for brief in &interrupted {
            host_events.push(format!(
                "【恢复】上次进程中断的 brief {}（{} → {}，kind={}）已标记 interrupted，未重放。\
                 请核查当前实际状态后决定：重新派发执行（新 brief）或放弃该工作项。",
                brief.id, brief.from_role, brief.to_role, brief.kind
            ));
        }
        emit(
            prepared,
            "Fusion Team",
            "fusion-status",
            &format!("检测到 {} 条上次进程中断的任务交接，已标记为 interrupted 等待主持恢复。", interrupted.len().max(swept)),
        );
    }
    loop {
        if prepared.cancellation.is_requested() {
            emit(prepared, "Fusion Team", "fusion-status", "已按用户请求停止。");
            return ChatTurnStatus::Interrupted;
        }
        if prepared.root_budget.is_expired() {
            emit(prepared, "Fusion Team", "fusion-status", "已达到本次运行的时间预算，停止推进。");
            finalize_open_task(prepared, "blocked");
            return ChatTurnStatus::Failed;
        }
        // 合并活动期间新到的用户消息。
        host_events.append(&mut drain_events(run));

        if !host_events.is_empty() {
            let prompt = host_events.join("\n\n");
            host_events.clear();
            if stats.role_turns >= settings.max_role_turns {
                emit(prepared, "Fusion Team", "fusion-status", "角色轮次达到上限，停止推进。");
                finalize_open_task(prepared, "blocked");
                return ChatTurnStatus::Failed;
            }
            stats.role_turns += 1;
            match run_role_turn(prepared, FusionRole::Host, None, &prompt).await {
                Ok(answer) => {
                    let trimmed = answer.trim();
                    if !trimmed.is_empty() {
                        emit(prepared, "主持", "fusion-reply", trimmed);
                    }
                }
                Err(outcome) => {
                    emit(
                        prepared,
                        "主持",
                        "fusion-status",
                        &format!("主持本轮执行失败：{}", outcome.reason),
                    );
                    finalize_open_task(prepared, "failed");
                    return ChatTurnStatus::Failed;
                }
            }
            // 主持发起了决策：任务挂起，等待用户下一次发送。
            if let Some(task) = task_open(prepared) {
                if task.status == "awaiting_user" {
                    return ChatTurnStatus::Completed;
                }
            }
            continue;
        }

        let queued = store::fusion_briefs_queued(
            &prepared.db_path,
            &prepared.room_id,
            &prepared.resolved.team.id,
        )
        .unwrap_or_default();
        if queued.is_empty() {
            // 退出前再收一轮迟到事件，缩小与推送方的竞态窗口。
            host_events.append(&mut drain_events(run));
            if !host_events.is_empty() {
                continue;
            }
            finalize_open_task(prepared, "");
            return ChatTurnStatus::Completed;
        }
        for brief in queued {
            if prepared.cancellation.is_requested() {
                emit(prepared, "Fusion Team", "fusion-status", "已按用户请求停止。");
                return ChatTurnStatus::Interrupted;
            }
            if prepared.root_budget.is_expired() {
                emit(prepared, "Fusion Team", "fusion-status", "已达到本次运行的时间预算，停止推进。");
                finalize_open_task(prepared, "blocked");
                return ChatTurnStatus::Failed;
            }
            if stats.briefs >= settings.max_briefs_per_turn {
                emit(prepared, "Fusion Team", "fusion-status", "本任务交接次数达到上限，停止推进。");
                finalize_open_task(prepared, "blocked");
                return ChatTurnStatus::Failed;
            }
            stats.briefs += 1;
            run_brief(prepared, &brief, &mut stats).await;
            // brief 结果以事件形式回到主持线程（下一轮循环主持可见）。
            if let Ok(Some(finished)) = store::fusion_brief_get(&prepared.db_path, &brief.id) {
                host_events.push(team_tools::render_result_event(&finished));
            }
        }
    }
}

/// 编排器退出时收敛任务状态：全部 done→done，有 blocked→blocked，其余保持。
fn finalize_open_task(prepared: &FusionPrepared, fallback: &str) {
    let Some(task) = task_open(prepared) else {
        return;
    };
    if !fallback.is_empty() {
        let _ = store::fusion_task_update(&prepared.db_path, &task.id, fallback, None, None);
        return;
    }
    if let Ok(items) = store::fusion_work_items_for_task(&prepared.db_path, &task.id) {
        if items.is_empty() {
            return;
        }
        if items.iter().all(|item| item.status == "done") {
            let _ = store::fusion_task_update(&prepared.db_path, &task.id, "done", None, None);
        } else if items.iter().any(|item| item.status == "blocked") {
            let _ = store::fusion_task_update(&prepared.db_path, &task.id, "blocked", None, None);
        }
    }
}

struct RoleTurnFailure {
    reason: String,
}

/// 执行一条 queued brief：房间卡片 → 角色轮次 → 结果卡片。
async fn run_brief(prepared: &FusionPrepared, brief: &store::FusionBriefRow, stats: &mut OrchestrateStats) {
    if !store::fusion_brief_claim(&prepared.db_path, &brief.id, &prepared.run_id)
        .unwrap_or(false)
    {
        return;
    }
    emit(
        prepared,
        "Fusion Team",
        "fusion-brief",
        &team_tools::render_brief_card(brief),
    );
    let Some(role) = FusionRole::parse(&brief.to_role) else {
        let _ = store::fusion_brief_finish(
            &prepared.db_path,
            &brief.id,
            "failed",
            &json!({"status":"failed","summary":"未知目标角色"}).to_string(),
        );
        return;
    };
    if role == FusionRole::Host {
        let _ = store::fusion_brief_finish(
            &prepared.db_path,
            &brief.id,
            "failed",
            &json!({"status":"failed","summary":"brief 不能派发回主持"}).to_string(),
        );
        return;
    }
    if stats.role_turns >= prepared.resolved.settings.max_role_turns {
        let _ = store::fusion_brief_finish(
            &prepared.db_path,
            &brief.id,
            "failed",
            &json!({"status":"failed","summary":"角色轮次达到上限"}).to_string(),
        );
        return;
    }
    stats.role_turns += 1;
    let prompt = team_tools::render_brief_prompt(brief);
    match run_role_turn(prepared, role, Some(brief.id.clone()), &prompt).await {
        Ok(answer) => {
            // 角色未调用 team_report/escalate：按 partial 收敛，答案作摘要。
            let finished = store::fusion_brief_get(&prepared.db_path, &brief.id)
                .ok()
                .flatten()
                .map(|row| !matches!(row.status.as_str(), "queued" | "running"))
                .unwrap_or(false);
            if !finished {
                let summary = answer.trim().chars().take(1200).collect::<String>();
                let _ = store::fusion_brief_finish(
                    &prepared.db_path,
                    &brief.id,
                    "partial",
                    &json!({"status":"partial","summary":summary}).to_string(),
                );
            }
        }
        Err(outcome) => {
            let _ = store::fusion_brief_finish(
                &prepared.db_path,
                &brief.id,
                "failed",
                &json!({"status":"failed","summary":outcome.reason}).to_string(),
            );
        }
    }
    if let Ok(Some(finished)) = store::fusion_brief_get(&prepared.db_path, &brief.id) {
        emit(
            prepared,
            "Fusion Team",
            "fusion-result",
            &team_tools::render_result_card(&finished),
        );
    }
}

/// 角色轮次：角色线程为独立上下文，复用共享模型工具循环与真实父上下文。
async fn run_role_turn(
    prepared: &FusionPrepared,
    role: FusionRole,
    brief_id: Option<String>,
    prompt: &str,
) -> Result<String, RoleTurnFailure> {
    let agent = prepared.resolved.agent_for(role).clone();
    let team_id = prepared.resolved.team.id.clone();
    let thread = store::fusion_role_messages_list(
        &prepared.db_path,
        &prepared.room_id,
        &team_id,
        role.as_str(),
    )
    .map_err(|error| RoleTurnFailure {
        reason: format!("读取角色线程失败：{error:?}"),
    })?;
    let options = context_build_options_for_agent_with_floor_and_room(
        &agent,
        None,
        Some(prepared.room_id.as_str()),
    );
    let mut assembly =
        build_context_assembly_with_roster(&agent, &thread, prompt, &[], options, None);
    assembly
        .system_prompt
        .push_str(&role_prompt(role, &prepared.resolved));
    let scope_value = FusionToolScope {
        team_id: team_id.clone(),
        room_id: prepared.room_id.clone(),
        workspace_id: prepared.workspace_id.clone(),
        db_path: prepared.db_path.clone(),
        role,
        brief_id: brief_id.clone(),
        task_id: task_open(prepared).map(|task| task.id),
        host_web_access: prepared.resolved.settings.host_web_access,
        querier_is_separate: prepared.resolved.querier_is_separate,
        task_title_hint: prepared
            .user_text
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| line.chars().take(160).collect::<String>()),
    };
    let response = scope(
        scope_value,
        run_shared_model_tool_loop(
            &agent,
            prompt,
            assembly,
            Some(prepared.room_id.as_str()),
            Some(&prepared.parent),
            None,
            format!("fusion-{}-{}", prepared.turn_id, role.as_str()),
            None,
            None,
        ),
    )
    .await
    .map_err(|error| RoleTurnFailure {
        reason: format!("{error:?}"),
    })?;
    // 角色线程落库：输入 + 本轮答案（长内容截断，详情经 brief/artifact 按需召回）。
    for (role_name, content) in [
        ("user", prompt.chars().take(4000).collect::<String>()),
        ("assistant", response.answer_text.chars().take(4000).collect::<String>()),
    ] {
        let _ = store::fusion_role_message_append(
            &prepared.db_path,
            &prepared.room_id,
            &team_id,
            role.as_str(),
            &store::FusionRoleMessage {
                role: role_name.to_string(),
                content,
                kind: "text".to_string(),
            },
        );
    }
    if !response.tool_requests.is_empty() {
        diag_log(&format!(
            "[FUSION] role={} turn={} 遗留未执行工具调用 {} 个",
            role.as_str(),
            prepared.turn_id,
            response.tool_requests.len()
        ));
    }
    Ok(response.answer_text)
}

fn role_prompt(role: FusionRole, resolved: &ResolvedTeam) -> String {
    match role {
        FusionRole::Host => {
            let web = if resolved.settings.host_web_access {
                "你可以直接使用 WebSearch/WebFetch 检索。"
            } else {
                "你不能直接联网检索；需要网络信息时用 team_query 派给查询者。"
            };
            format!(
                "\n\n## Fusion Team 角色：主持\n\
                 你是 Fusion Team 的主持与用户接口。职责：判断用户消息是问答型还是任务型。\n\
                 - 问答型：直接回答；需要检索时可读文件或用 team_query 派给查询者。\n\
                 - 任务型：先 team_plan 登记工作项计划，再 team_dispatch 派执行者实施；结果送达后对照验收标准用 team_accept 审核；返工可再派发。\n\
                 规则：你没有写文件和执行命令的权限；{web}\n\
                 不可逆操作、范围扩大或成本显著差异的决策必须 team_ask_user 问用户；其余方案自行决定并向用户说明。\n\
                 回复面向用户，简洁明确；最终答复时给出结论与关键依据。"
            )
        }
        FusionRole::Executor => {
            let query = if resolved.querier_is_separate {
                "可把网络检索/大范围搜索用 team_query 派给查询者；正在修改的代码自己读。"
            } else {
                "直接用只读工具检索。"
            };
            format!(
                "\n\n## Fusion Team 角色：执行者\n\
                 按 brief 实施代码修改并运行验证（测试/构建）。{query}\n\
                 完成后必须调用 team_report（status/summary/evidence/open_issues）；\n\
                 受阻（权限不足、方案不可行、反复失败）用 team_escalate 上报主持，不要无限重试同一做法。"
            )
        }
        FusionRole::Querier => "\n\n## Fusion Team 角色：查询者\n\
             只回答被问的问题，不做实现、不改文件。结论必须给引用（文件:行号 或 URL）与置信度；\n\
             完成后必须调用 team_report 汇报。"
            .to_string(),
    }
}

// ── HTTP API ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct FusionTeamsQuery {
    #[serde(default)]
    room_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FusionTeamUpsertPayload {
    name: Option<String>,
    enabled: Option<bool>,
    host_session_id: Option<String>,
    executor_session_id: Option<String>,
    querier_session_id: Option<String>,
    settings: Option<JsonValue>,
}

fn team_validity(team: &store::FusionTeamRow) -> (bool, Vec<String>) {
    let mut issues = Vec::new();
    let mut check = |session_id: Option<&String>, label: &str, required: bool| {
        let Some(id) = session_id.filter(|id| !id.trim().is_empty()) else {
            if required {
                issues.push(format!("未配置{label}"));
            }
            return;
        };
        match find_agent(id) {
            Some(agent) if agent_sendable(&agent) => {}
            Some(_) => issues.push(format!("{label}绑定的会话 `{id}` 不可用")),
            None => issues.push(format!("{label}绑定的会话 `{id}` 已不存在")),
        }
    };
    check(team.host_session_id.as_ref(), "主持", true);
    check(team.executor_session_id.as_ref(), "执行者", true);
    check(team.querier_session_id.as_ref(), "查询者", false);
    (issues.is_empty(), issues)
}

fn team_json(team: &store::FusionTeamRow) -> JsonValue {
    let (valid, issues) = team_validity(team);
    json!({
        "id": team.id,
        "name": team.name,
        "enabled": team.enabled,
        "target_id": format!("{}{}", FUSION_TARGET_PREFIX, team.id),
        "settings": serde_json::from_str::<JsonValue>(&team.settings_json).unwrap_or_else(|_| json!({})),
        "members": {
            "host": { "session_id": team.host_session_id },
            "executor": { "session_id": team.executor_session_id },
            "querier": {
                "session_id": team.querier_session_id,
                "same_as_executor": team
                    .querier_session_id
                    .as_deref()
                    .map(str::is_empty)
                    .unwrap_or(true),
            },
        },
        "sendable": team.enabled && valid,
        "issues": issues,
    })
}

pub(crate) async fn api_fusion_teams_list() -> ApiResult<axum::Json<JsonValue>> {
    let teams = store::fusion_teams_list(&default_session_sqlite_path())?;
    Ok(axum::Json(json!({
        "teams": teams.iter().map(team_json).collect::<Vec<_>>(),
    })))
}

fn validate_member_session(session_id: &str, label: &str) -> Result<(), ApiError> {
    match find_agent(session_id) {
        Some(agent)
            if agent_session_backend::AgentSessionBackend::for_provider(&agent.provider)
                == agent_session_backend::AgentSessionBackend::LlmHttp => Ok(()),
        Some(_) => Err(api_error(
            StatusCode::BAD_REQUEST,
            &format!("{label}必须绑定模型会话"),
        )),
        None => Err(api_error(
            StatusCode::BAD_REQUEST,
            &format!("{label}绑定的会话 `{session_id}` 不存在"),
        )),
    }
}

fn upsert_team(team_id: &str, payload: &FusionTeamUpsertPayload) -> ApiResult<store::FusionTeamRow> {
    let db_path = default_session_sqlite_path();
    let existing = store::fusion_team_get(&db_path, team_id)?;
    let name = payload
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| existing.as_ref().map(|team| team.name.clone()))
        .unwrap_or_else(|| "Fusion Team".to_string());
    let enabled = payload
        .enabled
        .or_else(|| existing.as_ref().map(|team| team.enabled))
        .unwrap_or(false);
    let host_session_id = payload
        .host_session_id
        .clone()
        .or_else(|| existing.as_ref().and_then(|team| team.host_session_id.clone()))
        .unwrap_or_default();
    let executor_session_id = payload
        .executor_session_id
        .clone()
        .or_else(|| existing.as_ref().and_then(|team| team.executor_session_id.clone()))
        .unwrap_or_default();
    let querier_session_id = match payload.querier_session_id.as_ref() {
        Some(value) => Some(value.trim().to_string()).filter(|value| !value.is_empty()),
        None => existing
            .as_ref()
            .and_then(|team| team.querier_session_id.clone()),
    };
    let settings = payload
        .settings
        .as_ref()
        .map(|value| value.to_string())
        .or_else(|| existing.as_ref().map(|team| team.settings_json.clone()))
        .unwrap_or_else(|| "{}".to_string());
    if enabled && (host_session_id.is_empty() || executor_session_id.is_empty()) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "启用 Fusion Team 前需先配置主持与执行者会话",
        ));
    }
    if !host_session_id.is_empty() {
        validate_member_session(&host_session_id, "主持")?;
    }
    if !executor_session_id.is_empty() {
        validate_member_session(&executor_session_id, "执行者")?;
    }
    if let Some(querier) = querier_session_id.as_deref() {
        validate_member_session(querier, "查询者")?;
    }
    store::fusion_team_upsert(
        &db_path,
        team_id,
        &name,
        enabled,
        &settings,
        &host_session_id,
        &executor_session_id,
        querier_session_id.as_deref(),
    )?;
    store::fusion_team_get(&db_path, team_id)?
        .ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "Fusion Team 保存失败"))
}

pub(crate) async fn api_fusion_team_create(
    axum::Json(payload): axum::Json<FusionTeamUpsertPayload>,
) -> ApiResult<axum::Json<JsonValue>> {
    let team_id = format!("fteam-{}", unix_timestamp_millis());
    let team = upsert_team(&team_id, &payload)?;
    Ok(axum::Json(team_json(&team)))
}

pub(crate) async fn api_fusion_team_update(
    AxumPath(team_id): AxumPath<String>,
    axum::Json(payload): axum::Json<FusionTeamUpsertPayload>,
) -> ApiResult<axum::Json<JsonValue>> {
    if store::fusion_team_get(&default_session_sqlite_path(), &team_id)?.is_none() {
        return Err(api_error(StatusCode::NOT_FOUND, "Fusion Team 不存在"));
    }
    let team = upsert_team(&team_id, &payload)?;
    Ok(axum::Json(team_json(&team)))
}

pub(crate) async fn api_fusion_team_delete(
    AxumPath(team_id): AxumPath<String>,
) -> ApiResult<axum::Json<JsonValue>> {
    let deleted = store::fusion_team_delete(&default_session_sqlite_path(), &team_id)?;
    Ok(axum::Json(json!({"deleted": deleted})))
}

pub(crate) async fn api_fusion_tasks(
    Query(query): Query<FusionTeamsQuery>,
) -> ApiResult<axum::Json<JsonValue>> {
    let db_path = default_session_sqlite_path();
    let room_id = query.room_id.clone().unwrap_or_default();
    let connection = open_session_connection(&db_path).map_err(sqlite_api_error)?;
    initialize_session_schema(&connection).map_err(sqlite_api_error)?;
    let mut statement = connection
        .prepare(
            "SELECT id,team_id,room_id,status,title,created_at,finished_at FROM fusion_tasks
             WHERE (?1='' OR room_id=?1) ORDER BY created_at DESC LIMIT 50",
        )
        .map_err(sqlite_api_error)?;
    let tasks = statement
        .query_map(params![room_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "team_id": row.get::<_, String>(1)?,
                "room_id": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "title": row.get::<_, Option<String>>(4)?,
                "created_at": row.get::<_, i64>(5)?,
                "finished_at": row.get::<_, Option<i64>>(6)?,
            }))
        })
        .map_err(sqlite_api_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_api_error)?;
    drop(statement);
    // 任务面板需要工作项级状态（done/pending/blocked 与重试计数）与待答决策数。
    let tasks: Vec<JsonValue> = tasks
        .into_iter()
        .map(|mut task| {
            let task_id = task["id"].as_str().unwrap_or_default().to_string();
            let items = store::fusion_work_items_for_task(&db_path, &task_id).unwrap_or_default();
            task["work_items"] = json!(items
                .iter()
                .map(|item| json!({
                    "id": item.id,
                    "title": item.title,
                    "status": item.status,
                    "depends_on": serde_json::from_str::<JsonValue>(&item.depends_on_json)
                        .unwrap_or_else(|_| json!([])),
                    "retry_count": item.retry_count,
                    "max_retries": item.max_retries,
                    "last_verdict": item.last_verdict,
                    "last_reason": item.last_reason,
                }))
                .collect::<Vec<_>>());
            task["pending_decisions"] = json!(store::fusion_decisions_pending(&db_path, &task_id)
                .map(|items| items.len())
                .unwrap_or(0));
            task
        })
        .collect();
    Ok(axum::Json(json!({"tasks": tasks})))
}

pub(crate) async fn api_fusion_task_cancel(
    AxumPath(task_id): AxumPath<String>,
) -> ApiResult<axum::Json<JsonValue>> {
    let db_path = default_session_sqlite_path();
    let Some(task) = store::fusion_task_get(&db_path, &task_id)? else {
        return Err(api_error(StatusCode::NOT_FOUND, "任务不存在"));
    };
    let key = run_key(&task.room_id, &task.team_id);
    let run = fusion_runs()
        .lock()
        .ok()
        .and_then(|runs| runs.get(&key).cloned());
    if let Some(run) = run {
        run.cancellation.request();
    }
    let _ = store::fusion_brief_cancel_running(&db_path, &task.room_id, &task.team_id);
    store::fusion_task_update(&db_path, &task_id, "cancelled", None, None)?;
    Ok(axum::Json(json!({"cancelled": true})))
}

#[derive(Deserialize)]
pub(crate) struct FusionDecisionAnswerPayload {
    #[serde(default)]
    option_id: Option<String>,
    #[serde(default)]
    free_text: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

/// 决策卡片回答：写答案 → 任务回 executing → 房间 answered 卡片 →
/// 以合成用户回合重启编排器（点选规则直达；自由文本由主持映射）。
pub(crate) async fn api_fusion_decision_answer(
    AxumPath(decision_id): AxumPath<String>,
    axum::Json(payload): axum::Json<FusionDecisionAnswerPayload>,
) -> ApiResult<axum::Json<JsonValue>> {
    let db_path = default_session_sqlite_path();
    let Some(decision) = store::fusion_decision_get(&db_path, &decision_id)? else {
        return Err(api_error(StatusCode::NOT_FOUND, "决策不存在或已过期"));
    };
    if decision.status != "pending" {
        return Err(api_error(StatusCode::CONFLICT, "该决策已回答"));
    }
    // options_json 形态：{options:[...], allow_free_text}（兼容历史裸数组）。
    let meta: JsonValue =
        serde_json::from_str(&decision.options_json).unwrap_or_else(|_| json!({}));
    let options = match meta.get("options") {
        Some(value) => value.as_array().cloned().unwrap_or_default(),
        None => meta.as_array().cloned().unwrap_or_default(),
    };
    let allow_free_text = meta
        .get("allow_free_text")
        .and_then(JsonValue::as_bool)
        .unwrap_or(true);
    let (label, free_text) = match (
        payload.option_id.as_deref().map(str::trim),
        payload.free_text.as_deref().map(str::trim),
    ) {
        (Some(option_id), _) if !option_id.is_empty() => {
            let label = options
                .iter()
                .find(|entry| {
                    entry.get("id").and_then(JsonValue::as_str) == Some(option_id)
                        || entry.get("label").and_then(JsonValue::as_str) == Some(option_id)
                })
                .and_then(|entry| entry.get("label"))
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            match label {
                Some(label) => (label, None),
                None => return Err(api_error(StatusCode::BAD_REQUEST, "所选选项不存在")),
            }
        }
        (_, Some(text)) if !text.is_empty() => {
            if !allow_free_text && !options.is_empty() {
                return Err(api_error(StatusCode::BAD_REQUEST, "该决策只允许点选"));
            }
            (format!("（自由回答）{text}"), Some(text.to_string()))
        }
        _ => return Err(api_error(StatusCode::BAD_REQUEST, "option_id 或 free_text 必填")),
    };
    let answer_json = json!({
        "option_id": payload.option_id,
        "label": label,
        "free_text": free_text,
    })
    .to_string();
    if !store::fusion_decision_answer(&db_path, &decision_id, &answer_json)? {
        return Err(api_error(StatusCode::CONFLICT, "该决策已回答"));
    }
    let Some(task) = store::fusion_task_get(&db_path, &decision.task_id)? else {
        return Err(api_error(StatusCode::NOT_FOUND, "决策所属任务已不存在"));
    };
    if task.status == "awaiting_user" {
        store::fusion_task_update(&db_path, &decision.task_id, "executing", None, None)?;
    }
    emit_room_message(
        &task.room_id,
        "主持",
        "fusion-decision",
        &format!(
            "已收到你的决定\n{}\n选择：{}\n[fusion-decision]{}",
            decision.question,
            label,
            json!({"id": decision_id, "answered": true, "answer": label})
        ),
    );
    // 恢复推进：合成 fusion 目标的用户回合复用 admit→编排器全链路
    // （决策记录已落库，事件文本提示主持这是规则直达的回答）。
    let resume = SendMessageRequest {
        expected_workspace_id: None,
        native_browser_panel: false,
        session_id: payload.session_id,
        chat_room_id: Some(task.room_id.clone()),
        target_agent_ids: vec![format!("{}{}", FUSION_TARGET_PREFIX, task.team_id)],
        text: format!("【决策回答】对「{}」的决定：{label}。请按此决定继续推进任务。", decision.question),
        selected_message_ids: None,
        attachments: None,
    };
    let prepared = admit(resume).await?;
    let started =
        start_chat_runtime_run_sqlite(&prepared.db_path, &prepared.run_id, &prepared.claim_token)
            .unwrap_or(false);
    if !started {
        let mut prepared = prepared;
        let _ = prepared.guard.finish(ChatTurnStatus::Failed);
        return Err(api_error(StatusCode::CONFLICT, "恢复运行启动冲突，请稍后重试"));
    }
    spawn_orchestrator(prepared);
    Ok(axum::Json(json!({
        "answered": true,
        "decision_id": decision_id,
        "answer": label,
    })))
}

/// 进程启动清扫：遗留 running brief → interrupted、running 工作项 → pending。
/// 在会话 schema 初始化完成后调用一次；不自动重放任何副作用。
pub(crate) fn startup_sweep(db_path: &Path) {
    match store::fusion_briefs_sweep_interrupted(db_path) {
        Ok(count) if count > 0 => {
            diag_log(&format!("fusion 启动清扫：{count} 条 running brief 标记 interrupted"));
            let _ = store::fusion_work_items_sweep_running(db_path);
        }
        _ => {}
    }
}
