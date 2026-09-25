use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::Deserialize;
use vision::{
    analyze_latest_desktop_with_backend, build_showui_grounding_request, parse_relative_point,
    relative_point_to_pixel, LocalOpenAiVisionBackend, VisionBackend, VisionImageSource,
    ZhipuVisionBackend,
};

use crate::config::GuiConfig;
use crate::desktop_anchor::{
    anchor_accepts_mouse, parse_anchor_scope, probe_anchor_inventory, resolve_anchor,
    AnchorInventory, AnchorScope, UiAnchor,
};
use crate::desktop_capture::capture_latest_desktop_snapshot_now;
use crate::input_backend::{
    click_point as inject_click_point, hold_virtual_key as inject_hold_virtual_key,
    move_mouse_relative as inject_move_mouse_relative,
    press_virtual_key as inject_press_virtual_key, scroll_wheel as inject_scroll_wheel,
    send_virtual_key_combo as inject_send_virtual_key_combo, type_text as inject_type_text,
};

const MAX_AUTOMATION_STEPS: usize = 6;
const MAX_REALTIME_ASSIST_STEPS: usize = 8;
const DEFAULT_WAIT_MS: u64 = 800;
const WINDOW_LIST_LIMIT: usize = 12;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopAutomationPhase {
    Preparing,
    ProbingWindows,
    Capturing,
    Reasoning,
    Executing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopAutomationMode {
    DesktopSoftware,
    RealtimeGameAssist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopWindowSummary {
    pub process_id: u32,
    pub process_name: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct DesktopAutomationSnapshot {
    pub job_id: u64,
    pub mode: DesktopAutomationMode,
    pub phase: DesktopAutomationPhase,
    pub step: usize,
    pub max_steps: usize,
    pub detail: String,
    pub target_window: Option<String>,
    pub last_action: Option<String>,
    pub visible_windows: Vec<DesktopWindowSummary>,
    pub last_capture_path: Option<PathBuf>,
    pub last_capture_size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DesktopAutomationResult {
    pub summary: String,
    pub mode: DesktopAutomationMode,
    pub model: String,
    pub steps_completed: usize,
    pub target_window: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DesktopAutomationEvent {
    Progress(DesktopAutomationSnapshot),
    Finished(Result<DesktopAutomationResult, String>),
}

#[derive(Debug, Clone)]
struct DesktopAutomationContext {
    job_id: u64,
    prompt: String,
    config: GuiConfig,
    started_at: Instant,
    mode: DesktopAutomationMode,
    step: usize,
    max_steps: usize,
    target_window: Option<String>,
    last_action: Option<String>,
    visible_windows: Vec<DesktopWindowSummary>,
    anchor_inventory: AnchorInventory,
    last_capture_path: Option<PathBuf>,
    last_capture_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct DesktopAutomationDecision {
    status: String,
    summary: Option<String>,
    target_window: Option<String>,
    action: Option<DesktopAutomationAction>,
}

#[derive(Debug, Clone, Deserialize)]
struct DesktopAutomationAction {
    kind: Option<String>,
    anchor_scope: Option<String>,
    x: Option<i32>,
    y: Option<i32>,
    dx: Option<i32>,
    dy: Option<i32>,
    window_title: Option<String>,
    text: Option<String>,
    keys: Option<Vec<String>>,
    key: Option<String>,
    duration_ms: Option<u64>,
    hold_ms: Option<u64>,
    delta: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
struct DesktopItemState {
    found: bool,
    selected: bool,
    focused: bool,
}

pub fn spawn_desktop_automation(
    config: GuiConfig,
    prompt: String,
) -> Receiver<DesktopAutomationEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = run_desktop_automation_job(config, prompt, &tx);
        let _ = tx.send(DesktopAutomationEvent::Finished(result));
    });
    rx
}

fn run_desktop_automation_job(
    config: GuiConfig,
    prompt: String,
    tx: &Sender<DesktopAutomationEvent>,
) -> Result<DesktopAutomationResult, String> {
    config.apply_process_env();
    ensure_live_mode()?;
    let mode = classify_desktop_automation_mode(&prompt);

    let mut ctx = DesktopAutomationContext {
        job_id: unique_job_id(),
        prompt,
        config,
        started_at: Instant::now(),
        mode,
        step: 0,
        max_steps: max_steps_for_mode(mode),
        target_window: None,
        last_action: None,
        visible_windows: Vec::new(),
        anchor_inventory: AnchorInventory::default(),
        last_capture_path: None,
        last_capture_size_bytes: None,
    };

    publish_progress(
        tx,
        &ctx,
        DesktopAutomationPhase::Preparing,
        &format!("桌面代理已接管任务，模式: {}。", mode_label(mode)),
    );

    for step_index in 0..ctx.max_steps {
        ctx.step = step_index + 1;

        publish_progress(
            tx,
            &ctx,
            DesktopAutomationPhase::ProbingWindows,
            "正在探测当前可见窗口。",
        );
        ctx.visible_windows = probe_visible_windows()?;
        ctx.anchor_inventory = if ctx.step == 1 || ctx.target_window.is_some() {
            probe_anchor_inventory(ctx.target_window.as_deref())
                .unwrap_or_else(|_| AnchorInventory::default())
        } else {
            AnchorInventory::default()
        };

        publish_progress(
            tx,
            &ctx,
            DesktopAutomationPhase::Capturing,
            "正在抓取最新桌面截图。",
        );
        let capture = capture_latest_desktop_snapshot_now()?;
        ctx.last_capture_path = Some(capture.path.clone());
        ctx.last_capture_size_bytes = Some(capture.file_size);

        ctx.visible_windows = probe_visible_windows()?;

        publish_progress(
            tx,
            &ctx,
            DesktopAutomationPhase::Reasoning,
            "视觉模型正在规划下一步动作。",
        );
        let decision = maybe_plan_local_showui_click_decision(&ctx, &capture)
            .or_else(|| maybe_plan_local_anchor_decision(&ctx))
            .unwrap_or_else(|| request_next_action(&ctx, &capture))?;

        if let Some(target_window) = decision.target_window.clone() {
            ctx.target_window = Some(target_window);
        }

        match decision.status.trim().to_ascii_lowercase().as_str() {
            "done" | "completed" => {
                let summary = decision
                    .summary
                    .unwrap_or_else(|| "桌面任务已完成。".to_string());
                publish_progress(tx, &ctx, DesktopAutomationPhase::Completed, &summary);
                return Ok(DesktopAutomationResult {
                    summary,
                    mode: ctx.mode,
                    model: ctx.config.vision_model.clone(),
                    steps_completed: ctx.step,
                    target_window: ctx.target_window.clone(),
                });
            }
            "need_user" | "needs_user" => {
                return Err(decision
                    .summary
                    .unwrap_or_else(|| "桌面代理需要人工接管当前步骤。".to_string()));
            }
            _ => {}
        }

        let action = decision
            .action
            .ok_or_else(|| "visual model did not return an action".to_string())?;
        validate_action_for_mode(&ctx, &action)?;
        let action_summary = describe_action(&action);
        ctx.last_action = Some(action_summary.clone());

        publish_progress(
            tx,
            &ctx,
            DesktopAutomationPhase::Executing,
            &format!("正在执行动作: {action_summary}"),
        );
        execute_action(&action, ctx.target_window.as_deref(), &ctx.prompt)?;
        thread::sleep(Duration::from_millis(
            action.duration_ms.unwrap_or(DEFAULT_WAIT_MS).min(5_000),
        ));
    }

    Err(format!(
        "桌面代理在 {} 步内未能完成任务, 请缩小范围后重试.",
        ctx.max_steps
    ))
}
fn request_next_action(
    ctx: &DesktopAutomationContext,
    capture: &crate::desktop_capture::CapturedDesktopSnapshot,
) -> Result<DesktopAutomationDecision, String> {
    if should_use_local_showui(&ctx.config) {
        return Err(
            "local ShowUI is a grounding backend, not a full JSON desktop planner yet; local anchors or simple click grounding must handle this task"
                .to_string(),
        );
    }

    let prompt = build_desktop_automation_prompt(ctx, capture);
    let response = if ctx
        .config
        .vision_backend
        .trim()
        .eq_ignore_ascii_case("local-openai")
    {
        let backend = LocalOpenAiVisionBackend::new(
            &ctx.config.local_vision_base_url,
            &ctx.config.local_vision_model,
        )
        .with_api_key(&ctx.config.local_vision_api_key)
        .with_timeout_seconds(ctx.config.vision_agent_timeout_seconds);
        analyze_latest_desktop_with_backend(&backend, &prompt)
            .map_err(|error| format!("desktop automation local vision request failed: {error}"))?
    } else {
        let backend = ZhipuVisionBackend::default().with_model(&ctx.config.vision_model);
        analyze_latest_desktop_with_backend(&backend, &prompt)
            .map_err(|error| format!("desktop automation cloud vision request failed: {error}"))?
    };
    let json_text =
        extract_json_object(&response.text).unwrap_or_else(|| response.text.trim().to_string());
    serde_json::from_str::<DesktopAutomationDecision>(&json_text).map_err(|error| {
        format!(
            "failed to parse automation decision: {error}; raw={}",
            response.text
        )
    })
}

fn maybe_plan_local_showui_click_decision(
    ctx: &DesktopAutomationContext,
    capture: &crate::desktop_capture::CapturedDesktopSnapshot,
) -> Option<Result<DesktopAutomationDecision, String>> {
    if ctx.mode != DesktopAutomationMode::DesktopSoftware || !should_use_local_showui(&ctx.config) {
        return None;
    }

    if goal_mentions_close(&ctx.prompt) {
        if let Some(window) = ctx
            .visible_windows
            .iter()
            .find(|window| is_known_explorer_window_title(&window.title))
        {
            return Some(resolve_showui_click(
                ctx,
                capture,
                "the close X button on the top-right corner of the File Explorer window titled 此电脑",
                1,
            )
            .map(|mut decision| {
                decision.target_window = Some(window.title.clone());
                decision
            }));
        }
    }

    if ctx
        .last_action
        .as_deref()
        .is_some_and(|value| value.starts_with("type_text("))
        && goal_mentions_this_pc_or_explorer(&ctx.prompt)
    {
        return Some(resolve_showui_click(
            ctx,
            capture,
            "the File Explorer best match result in the Windows search panel",
            1,
        ));
    }

    let (target, clicks) = infer_showui_click_target(&ctx.prompt)?;
    Some(resolve_showui_click(ctx, capture, &target, clicks))
}

fn goal_mentions_this_pc_or_explorer(goal: &str) -> bool {
    let normalized = goal.to_ascii_lowercase();
    goal.contains("此电脑")
        || goal.contains("我的电脑")
        || goal.contains("资源管理器")
        || goal.contains("文件")
        || normalized.contains("this pc")
        || normalized.contains("my computer")
        || normalized.contains("explorer")
        || normalized.contains("file explorer")
}

fn goal_mentions_close(goal: &str) -> bool {
    let normalized = goal.to_ascii_lowercase();
    goal.contains("\u{5173}\u{95ed}")
        || goal.contains("\u{5173}\u{6389}")
        || goal.contains("\u{9000}\u{51fa}")
        || normalized.contains("close")
        || normalized.contains("exit")
}

fn should_use_local_showui(config: &GuiConfig) -> bool {
    config
        .vision_backend
        .trim()
        .eq_ignore_ascii_case("local-openai")
        && config
            .local_vision_model
            .trim()
            .to_ascii_lowercase()
            .contains("showui")
}

fn infer_showui_click_target(prompt: &str) -> Option<(String, u32)> {
    let normalized = prompt.trim().to_ascii_lowercase();
    let clicks = if prompt.contains("双击") || normalized.contains("double click") {
        2
    } else {
        1
    };
    let is_click_prompt = prompt.contains("点击")
        || prompt.contains("点一下")
        || prompt.contains("单击")
        || prompt.contains("双击")
        || normalized.contains("click");
    if !is_click_prompt {
        return None;
    }

    if prompt.contains("开始") || normalized.contains("start") {
        return Some((
            "the blue Windows logo Start button on the bottom taskbar".to_string(),
            clicks,
        ));
    }
    if prompt.contains("搜索") || normalized.contains("search") {
        return Some((
            "the search box labeled 搜索 on the bottom taskbar".to_string(),
            clicks,
        ));
    }
    if prompt.contains("资源管理器")
        || prompt.contains("文件")
        || normalized.contains("file explorer")
        || normalized.contains("explorer")
    {
        return Some((
            "the yellow File Explorer icon on the bottom taskbar".to_string(),
            clicks,
        ));
    }
    if prompt.contains("关闭") || normalized.contains("close") {
        return Some((
            "the close button at the top-right corner of the active window".to_string(),
            clicks,
        ));
    }

    Some((prompt.trim().to_string(), clicks))
}

fn resolve_showui_click(
    ctx: &DesktopAutomationContext,
    capture: &crate::desktop_capture::CapturedDesktopSnapshot,
    target: &str,
    clicks: u32,
) -> Result<DesktopAutomationDecision, String> {
    let backend = LocalOpenAiVisionBackend::new(
        &ctx.config.local_vision_base_url,
        &ctx.config.local_vision_model,
    )
    .with_api_key(&ctx.config.local_vision_api_key)
    .with_timeout_seconds(ctx.config.vision_agent_timeout_seconds);
    let request =
        build_showui_grounding_request(target, VisionImageSource::Path(capture.path.clone()));
    let response = backend
        .analyze(&request)
        .map_err(|error| format!("local ShowUI grounding failed: {error}"))?;
    let point = parse_relative_point(&response.text).ok_or_else(|| {
        format!(
            "local ShowUI did not return a parseable relative coordinate: {}",
            response.text
        )
    })?;
    validate_grounded_point(target, point)?;
    let (x, y) = relative_point_to_pixel(point, capture.source_dimensions).ok_or_else(|| {
        format!(
            "local ShowUI returned an out-of-range coordinate [{:.3}, {:.3}] for screenshot {}x{}",
            point.0, point.1, capture.source_dimensions.0, capture.source_dimensions.1
        )
    })?;

    Ok(DesktopAutomationDecision {
        status: "continue".to_string(),
        summary: Some(format!(
            "local ShowUI grounded target `{target}` to ({x}, {y})"
        )),
        target_window: ctx.target_window.clone(),
        action: Some(DesktopAutomationAction {
            kind: Some(if clicks > 1 {
                "double_click".to_string()
            } else {
                "click".to_string()
            }),
            anchor_scope: None,
            x: Some(x),
            y: Some(y),
            dx: None,
            dy: None,
            window_title: None,
            text: None,
            keys: None,
            key: None,
            duration_ms: Some(DEFAULT_WAIT_MS),
            hold_ms: None,
            delta: None,
        }),
    })
}

fn validate_grounded_point(target: &str, point: (f32, f32)) -> Result<(), String> {
    let normalized = target.to_ascii_lowercase();
    let expects_taskbar = normalized.contains("taskbar")
        || normalized.contains("start button")
        || normalized.contains("search box")
        || normalized.contains("file explorer icon");
    if expects_taskbar && point.1 < 0.80 {
        return Err(format!(
            "local ShowUI grounding rejected [{:.3}, {:.3}]: expected a taskbar target near the bottom of the screenshot",
            point.0, point.1
        ));
    }

    let expects_top_right = normalized.contains("top-right") || normalized.contains("close button");
    if expects_top_right && point.1 > 0.35 {
        return Err(format!(
            "local ShowUI grounding rejected [{:.3}, {:.3}]: expected an upper window control",
            point.0, point.1
        ));
    }

    Ok(())
}

fn build_desktop_automation_prompt(
    ctx: &DesktopAutomationContext,
    capture: &crate::desktop_capture::CapturedDesktopSnapshot,
) -> String {
    match ctx.mode {
        DesktopAutomationMode::DesktopSoftware => build_desktop_software_prompt(ctx, capture),
        DesktopAutomationMode::RealtimeGameAssist => build_realtime_game_prompt(ctx, capture),
    }
}

fn build_desktop_software_prompt(
    ctx: &DesktopAutomationContext,
    capture: &crate::desktop_capture::CapturedDesktopSnapshot,
) -> String {
    let capture_age_ms = capture
        .captured_at
        .elapsed()
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let windows = if ctx.visible_windows.is_empty() {
        "none".to_string()
    } else {
        ctx.visible_windows
            .iter()
            .take(WINDOW_LIST_LIMIT)
            .map(|window| {
                format!(
                    "- [{}] {} | {}",
                    window.process_id, window.process_name, window.title
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let target_hint = ctx.target_window.as_deref().unwrap_or("unknown");
    let last_action = ctx.last_action.as_deref().unwrap_or("none");
    let local_anchors = ctx.anchor_inventory.prompt_block();

    format!(
        "You are the desktop action planner for COOLZHU CODE on Windows.\n\
The user goal is: {goal}\n\
Current step: {step}/{max_steps}\n\
Elapsed seconds: {elapsed}\n\
Current target window hint: {target_hint}\n\
Last action: {last_action}\n\
Visible top-level windows:\n{windows}\n\
Desktop screenshot path: {capture_path}\n\
Desktop screenshot size: {width}x{height}\n\
Desktop screenshot bytes: {bytes}\n\
Desktop screenshot age ms: {capture_age_ms}\n\
Local anchor inventory:\n{local_anchors}\n\
Return strict JSON only, no markdown fences.\n\
JSON schema:\n\
{{\n\
  \"status\": \"continue\" | \"done\" | \"need_user\",\n\
  \"summary\": \"short Chinese summary\",\n\
  \"target_window\": \"best matching window title if known\",\n\
  \"action\": {{\n\
    \"kind\": \"focus_window\" | \"minimize_window\" | \"close_window\" | \"click_anchor\" | \"double_click_anchor\" | \"click\" | \"double_click\" | \"type_text\" | \"key_press\" | \"wait\" | \"scroll\",\n\
    \"anchor_scope\": \"taskbar\" | \"window\" | \"desktop\",\n\
    \"window_title\": \"substring of the target window title when needed\",\n\
    \"x\": 0,\n\
    \"y\": 0,\n\
    \"text\": \"anchor label or text to input when needed\",\n\
    \"key\": \"ENTER\",\n\
    \"duration_ms\": 800,\n\
    \"delta\": -120\n\
  }}\n\
}}\n\
Rules:\n\
- Prefer focus_window before click or type_text when a known target window exists.\n\
- Prefer click_anchor or double_click_anchor whenever the local anchor inventory contains a matching desktop, taskbar, or window target.\n\
- Use absolute desktop coordinates for click and double_click.\n\
- Do not use any shortcut key combinations in this mode.\n\
- Prefer visible mouse operations first: desktop icons, taskbar icons, title-bar buttons, navigation panes and on-screen buttons.\n\
- If the current foreground window blocks the desktop or taskbar, prefer minimize_window on that foreground window first.\n\
- If the taskbar is hidden, click the bottom screen edge first to reveal it.\n\
- To open This PC, prefer a local desktop anchor first, then a local taskbar anchor for File Explorer, then a local window anchor named This PC inside the Explorer window.\n\
- To open 鏂囦欢璧勬簮绠＄悊鍣?or 姝ょ數鑴?prefer double-clicking a visible desktop icon first, then clicking the taskbar 鏂囦欢璧勬簮绠＄悊鍣?icon, or clicking the Start/Search area and then using plain typing plus ENTER.\n\
- To close a window, prefer close_window or clicking the top-right close button instead of a shortcut.\n\
- Use key_press only for single keys such as ENTER, TAB, ESC, arrows, DELETE or BACKSPACE.\n\
- If the task is already complete, return status=done.\n\
- If the screen is blocked by a system dialog or the next step is risky, return status=need_user.\n\
- Only choose one action per response.\n",
        goal = ctx.prompt,
        step = ctx.step,
        max_steps = ctx.max_steps,
        elapsed = ctx.started_at.elapsed().as_secs(),
        target_hint = target_hint,
        last_action = last_action,
        windows = windows,
        capture_path = capture.path.display(),
        width = capture.source_dimensions.0,
        height = capture.source_dimensions.1,
        bytes = capture.file_size,
        capture_age_ms = capture_age_ms,
        local_anchors = local_anchors,
    )
}

fn build_realtime_game_prompt(
    ctx: &DesktopAutomationContext,
    capture: &crate::desktop_capture::CapturedDesktopSnapshot,
) -> String {
    let capture_age_ms = capture
        .captured_at
        .elapsed()
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let windows = if ctx.visible_windows.is_empty() {
        "none".to_string()
    } else {
        ctx.visible_windows
            .iter()
            .take(WINDOW_LIST_LIMIT)
            .map(|window| {
                format!(
                    "- [{}] {} | {}",
                    window.process_id, window.process_name, window.title
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let target_hint = ctx.target_window.as_deref().unwrap_or("unknown");
    let last_action = ctx.last_action.as_deref().unwrap_or("none");

    format!(
        "You are the realtime game assist planner for COOLZHU CODE on Windows.\n\
The user goal is: {goal}\n\
Mode note: this loop is not frame-perfect and must choose only one short, low-risk macro action each turn.\n\
Current step: {step}/{max_steps}\n\
Elapsed seconds: {elapsed}\n\
Current target window hint: {target_hint}\n\
Last action: {last_action}\n\
Visible top-level windows:\n{windows}\n\
Desktop screenshot path: {capture_path}\n\
Desktop screenshot size: {width}x{height}\n\
Desktop screenshot bytes: {bytes}\n\
Desktop screenshot age ms: {capture_age_ms}\n\
Return strict JSON only, no markdown fences.\n\
JSON schema:\n\
{{\n\
  \"status\": \"continue\" | \"done\" | \"need_user\",\n\
  \"summary\": \"short Chinese summary\",\n\
  \"target_window\": \"best matching game window title if known\",\n\
  \"action\": {{\n\
    \"kind\": \"focus_window\" | \"minimize_window\" | \"close_window\" | \"click\" | \"double_click\" | \"move_mouse_relative\" | \"hold_key\" | \"key_press\" | \"wait\",\n\
    \"window_title\": \"substring of the target window title when needed\",\n\
    \"x\": 0,\n\
    \"y\": 0,\n\
    \"dx\": 0,\n\
    \"dy\": 0,\n\
    \"key\": \"W\",\n\
    \"key\": \"W\",\n\
    \"hold_ms\": 200,\n\
    \"duration_ms\": 200\n\
  }}\n\
}}\n\
Rules:\n\
- This mode is for low-frequency assist only, not frame-perfect competitive control.\n\
- Prefer focus_window before any other action when the game window is known.\n\
- Do not use shortcut key combinations in this mode.\n\
- Use hold_key only for short bursts between 80 and 1200 ms.\n\
- Use move_mouse_relative for camera nudges, not large teleports.\n\
- If the scene is too fast, the target is fully occluded, or a precise combo is required, return status=need_user.\n\
- Only choose one action per response.\n",
        goal = ctx.prompt,
        step = ctx.step,
        max_steps = ctx.max_steps,
        elapsed = ctx.started_at.elapsed().as_secs(),
        target_hint = target_hint,
        last_action = last_action,
        windows = windows,
        capture_path = capture.path.display(),
        width = capture.source_dimensions.0,
        height = capture.source_dimensions.1,
        bytes = capture.file_size,
        capture_age_ms = capture_age_ms,
    )
}

fn maybe_plan_local_anchor_decision(
    ctx: &DesktopAutomationContext,
) -> Option<Result<DesktopAutomationDecision, String>> {
    if ctx.mode != DesktopAutomationMode::DesktopSoftware {
        return None;
    }

    let normalized_goal = ctx.prompt.to_ascii_lowercase();
    let wants_this_pc = ctx.prompt.contains("此电脑")
        || ctx.prompt.contains("我的电脑")
        || normalized_goal.contains("this pc")
        || normalized_goal.contains("my computer");
    if !wants_this_pc {
        return None;
    }

    let wants_close = ctx.prompt.contains("关闭")
        || ctx.prompt.contains("关掉")
        || ctx.prompt.contains("退出")
        || normalized_goal.contains("close");

    if wants_close
        && ctx.last_action.as_deref().is_some_and(|value| {
            value.starts_with("click_anchor(taskbar:文件资源管理器")
                || value.starts_with("click_anchor(taskbar:资源管理器")
                || value.starts_with("click_anchor(taskbar:Explorer")
        })
    {
        return Some(Ok(DesktopAutomationDecision {
            status: "continue".to_string(),
            summary: Some("任务栏资源管理器已打开，准备关闭当前前景窗口。".to_string()),
            target_window: ctx.target_window.clone(),
            action: Some(DesktopAutomationAction {
                kind: Some("close_window".to_string()),
                anchor_scope: None,
                x: None,
                y: None,
                dx: None,
                dy: None,
                window_title: None,
                text: None,
                keys: None,
                key: None,
                duration_ms: Some(DEFAULT_WAIT_MS),
                hold_ms: None,
                delta: None,
            }),
        }));
    }

    if wants_close
        && ctx.last_action.as_deref().is_some_and(|value| {
            value.starts_with("close_window")
                || (value.starts_with("click(") && ctx.target_window.is_some())
        })
        && !ctx
            .visible_windows
            .iter()
            .any(|window| is_known_explorer_window_title(&window.title))
    {
        return Some(Ok(done_decision("已完成打开并关闭此电脑窗口.")));
    }

    if let Some(window) = ctx
        .visible_windows
        .iter()
        .find(|window| is_known_explorer_window_title(&window.title))
    {
        if wants_close
            && ctx.last_action.as_deref().is_some_and(|value| {
                value.starts_with("click_anchor(taskbar:文件资源管理器")
                    || value.starts_with("click_anchor(taskbar:资源管理器")
                    || value.starts_with("click_anchor(taskbar:Explorer")
                    || value.starts_with("key_press(ENTER)")
            })
        {
            return Some(Ok(DesktopAutomationDecision {
                status: "continue".to_string(),
                summary: Some("检测到资源管理器窗口已打开，准备关闭。".to_string()),
                target_window: Some(window.title.clone()),
                action: Some(DesktopAutomationAction {
                    kind: Some("close_window".to_string()),
                    anchor_scope: None,
                    x: None,
                    y: None,
                    dx: None,
                    dy: None,
                    window_title: Some(window.title.clone()),
                    text: None,
                    keys: None,
                    key: None,
                    duration_ms: Some(DEFAULT_WAIT_MS),
                    hold_ms: None,
                    delta: None,
                }),
            }));
        }

        if is_known_this_pc_window_title(&window.title) {
            if wants_close {
                return Some(Ok(DesktopAutomationDecision {
                    status: "continue".to_string(),
                    summary: Some("检测到此电脑窗口，准备关闭。".to_string()),
                    target_window: Some(window.title.clone()),
                    action: Some(DesktopAutomationAction {
                        kind: Some("close_window".to_string()),
                        anchor_scope: None,
                        x: None,
                        y: None,
                        dx: None,
                        dy: None,
                        window_title: Some(window.title.clone()),
                        text: None,
                        keys: None,
                        key: None,
                        duration_ms: Some(DEFAULT_WAIT_MS),
                        hold_ms: None,
                        delta: None,
                    }),
                }));
            }
            return Some(Ok(done_decision("已打开此电脑窗口。")));
        }

        if let Some(anchor_label) = resolve_first_anchor_label(
            AnchorScope::Window,
            Some(&window.title),
            &["此电脑", "This PC"],
            false,
        ) {
            return Some(Ok(DesktopAutomationDecision {
                status: "continue".to_string(),
                summary: Some("已打开资源管理器，准备点击窗口内的此电脑锚点。".to_string()),
                target_window: Some(window.title.clone()),
                action: Some(DesktopAutomationAction {
                    kind: Some("click_anchor".to_string()),
                    anchor_scope: Some("window".to_string()),
                    x: None,
                    y: None,
                    dx: None,
                    dy: None,
                    window_title: Some(window.title.clone()),
                    text: Some(anchor_label),
                    keys: None,
                    key: None,
                    duration_ms: Some(DEFAULT_WAIT_MS),
                    hold_ms: None,
                    delta: None,
                }),
            }));
        }
    }

    if ctx.last_action.as_deref().is_some_and(|value| {
        value.starts_with("open_start_menu")
            || value.starts_with("click_anchor(taskbar:开始")
            || value.starts_with("click_anchor(taskbar:Start")
            || value.starts_with("click_anchor(taskbar:unknown")
    }) {
        return Some(Ok(DesktopAutomationDecision {
            status: "continue".to_string(),
            summary: Some("开始菜单已打开，输入 explorer 搜索文件资源管理器。".to_string()),
            target_window: None,
            action: Some(DesktopAutomationAction {
                kind: Some("type_text".to_string()),
                anchor_scope: None,
                x: None,
                y: None,
                dx: None,
                dy: None,
                window_title: None,
                text: Some("explorer".to_string()),
                keys: None,
                key: None,
                duration_ms: Some(600),
                hold_ms: None,
                delta: None,
            }),
        }));
    }

    if ctx
        .last_action
        .as_deref()
        .is_some_and(|value| value.starts_with("type_text("))
    {
        return Some(Ok(DesktopAutomationDecision {
            status: "continue".to_string(),
            summary: Some("搜索词已输入，按 Enter 打开匹配结果。".to_string()),
            target_window: None,
            action: Some(DesktopAutomationAction {
                kind: Some("key_press".to_string()),
                anchor_scope: None,
                x: None,
                y: None,
                dx: None,
                dy: None,
                window_title: None,
                text: None,
                keys: None,
                key: Some("ENTER".to_string()),
                duration_ms: Some(1_200),
                hold_ms: None,
                delta: None,
            }),
        }));
    }

    let this_pc_state = desktop_item_state("此电脑").unwrap_or_default();

    if ctx.step == 1 {
        if this_pc_state.selected || this_pc_state.focused {
            if let Some(anchor_label) = resolve_first_anchor_label(
                AnchorScope::Desktop,
                None,
                &["此电脑", "我的电脑", "This PC", "My Computer"],
                true,
            ) {
                return Some(Ok(DesktopAutomationDecision {
                    status: "continue".to_string(),
                    summary: Some(
                        "检测到桌面上的此电脑图标已被选中，继续单击尝试打开。".to_string(),
                    ),
                    target_window: Some("此电脑".to_string()),
                    action: Some(DesktopAutomationAction {
                        kind: Some("click_anchor".to_string()),
                        anchor_scope: Some("desktop".to_string()),
                        x: None,
                        y: None,
                        dx: None,
                        dy: None,
                        window_title: None,
                        text: Some(anchor_label),
                        keys: None,
                        key: None,
                        duration_ms: Some(DEFAULT_WAIT_MS),
                        hold_ms: None,
                        delta: None,
                    }),
                }));
            }
        }

        if let Some(anchor_label) = resolve_first_anchor_label(
            AnchorScope::Desktop,
            None,
            &["此电脑", "我的电脑", "This PC", "My Computer"],
            true,
        ) {
            return Some(Ok(DesktopAutomationDecision {
                status: "continue".to_string(),
                summary: Some(
                    "本地锚点层已命中桌面的此电脑图标，先单击选中并校验桌面壳层状态。".to_string(),
                ),
                target_window: Some("此电脑".to_string()),
                action: Some(DesktopAutomationAction {
                    kind: Some("click_anchor".to_string()),
                    anchor_scope: Some("desktop".to_string()),
                    x: None,
                    y: None,
                    dx: None,
                    dy: None,
                    window_title: None,
                    text: Some(anchor_label),
                    keys: None,
                    key: None,
                    duration_ms: Some(DEFAULT_WAIT_MS),
                    hold_ms: None,
                    delta: None,
                }),
            }));
        }

        if let Some(anchor_label) = resolve_first_anchor_label(
            AnchorScope::Taskbar,
            None,
            &["文件资源管理器", "资源管理器", "Explorer", "File Explorer"],
            true,
        ) {
            return Some(Ok(DesktopAutomationDecision {
                status: "continue".to_string(),
                summary: Some(
                    "本地锚点层已命中任务栏资源管理器图标，优先走纯鼠标路径。".to_string(),
                ),
                target_window: Some("文件资源管理器".to_string()),
                action: Some(DesktopAutomationAction {
                    kind: Some("click_anchor".to_string()),
                    anchor_scope: Some("taskbar".to_string()),
                    x: None,
                    y: None,
                    dx: None,
                    dy: None,
                    window_title: Some("文件资源管理器".to_string()),
                    text: Some(anchor_label),
                    keys: None,
                    key: None,
                    duration_ms: Some(DEFAULT_WAIT_MS),
                    hold_ms: None,
                    delta: None,
                }),
            }));
        }

        return Some(Ok(DesktopAutomationDecision {
            status: "continue".to_string(),
            summary: Some("未找到桌面或任务栏资源管理器入口，点击开始菜单走搜索路径。".to_string()),
            target_window: None,
            action: Some(DesktopAutomationAction {
                kind: Some("open_start_menu".to_string()),
                anchor_scope: None,
                x: None,
                y: None,
                dx: None,
                dy: None,
                window_title: None,
                text: None,
                keys: None,
                key: None,
                duration_ms: Some(DEFAULT_WAIT_MS),
                hold_ms: None,
                delta: None,
            }),
        }));
    }

    if ctx
        .last_action
        .as_deref()
        .is_some_and(|value| value.starts_with("click_anchor(desktop:"))
        && !ctx
            .visible_windows
            .iter()
            .any(|window| is_known_explorer_window_title(&window.title))
    {
        if let Some(anchor_label) = resolve_first_anchor_label(
            AnchorScope::Desktop,
            None,
            &[
                "\u{6b64}\u{7535}\u{8111}",
                "\u{6211}\u{7684}\u{7535}\u{8111}",
                "This PC",
                "My Computer",
            ],
            true,
        ) {
            return Some(Ok(DesktopAutomationDecision {
                status: "continue".to_string(),
                summary: Some(
                    "desktop icon was selected; clicking it again to open the window".to_string(),
                ),
                target_window: Some("This PC".to_string()),
                action: Some(DesktopAutomationAction {
                    kind: Some("click_anchor".to_string()),
                    anchor_scope: Some("desktop".to_string()),
                    x: None,
                    y: None,
                    dx: None,
                    dy: None,
                    window_title: None,
                    text: Some(anchor_label),
                    keys: None,
                    key: None,
                    duration_ms: Some(1_200),
                    hold_ms: None,
                    delta: None,
                }),
            }));
        }
    }

    if ctx.step == 2 && (this_pc_state.selected || this_pc_state.focused) {
        if let Some(anchor_label) = resolve_first_anchor_label(
            AnchorScope::Desktop,
            None,
            &["此电脑", "我的电脑", "This PC", "My Computer"],
            true,
        ) {
            return Some(Ok(DesktopAutomationDecision {
                status: "continue".to_string(),
                summary: Some("桌面壳层已选中此电脑图标，继续单击尝试打开窗口。".to_string()),
                target_window: Some("此电脑".to_string()),
                action: Some(DesktopAutomationAction {
                    kind: Some("click_anchor".to_string()),
                    anchor_scope: Some("desktop".to_string()),
                    x: None,
                    y: None,
                    dx: None,
                    dy: None,
                    window_title: None,
                    text: Some(anchor_label),
                    keys: None,
                    key: None,
                    duration_ms: Some(DEFAULT_WAIT_MS),
                    hold_ms: None,
                    delta: None,
                }),
            }));
        }
    }

    if !ctx
        .visible_windows
        .iter()
        .any(|window| is_known_explorer_window_title(&window.title))
        && ctx.step >= 3
    {
        return Some(Ok(DesktopAutomationDecision {
            status: "continue".to_string(),
            summary: Some(
                "纯鼠标路径未在前三步拉起目标窗口，切换到本地系统适配器兜底。".to_string(),
            ),
            target_window: Some("此电脑".to_string()),
            action: Some(DesktopAutomationAction {
                kind: Some("open_this_pc".to_string()),
                anchor_scope: None,
                x: None,
                y: None,
                dx: None,
                dy: None,
                window_title: Some("此电脑".to_string()),
                text: None,
                keys: None,
                key: None,
                duration_ms: Some(1_200),
                hold_ms: None,
                delta: None,
            }),
        }));
    }

    None
}

fn done_decision(summary: &str) -> DesktopAutomationDecision {
    DesktopAutomationDecision {
        status: "done".to_string(),
        summary: Some(summary.to_string()),
        target_window: None,
        action: None,
    }
}

#[allow(dead_code)]
fn is_explorer_window_title(title: &str) -> bool {
    title.contains("文件资源管理器")
        || title.contains("此电脑")
        || title.to_ascii_lowercase().contains("explorer")
}

#[allow(dead_code)]
fn is_this_pc_window_title(title: &str) -> bool {
    title.contains("此电脑") || title.to_ascii_lowercase().contains("this pc")
}

fn is_known_explorer_window_title(title: &str) -> bool {
    let normalized = title.to_ascii_lowercase();
    title.contains("\u{6587}\u{4ef6}\u{8d44}\u{6e90}\u{7ba1}\u{7406}\u{5668}")
        || title.contains("\u{8d44}\u{6e90}\u{7ba1}\u{7406}\u{5668}")
        || title.contains("\u{6b64}\u{7535}\u{8111}")
        || normalized.contains("explorer")
        || normalized.contains("file explorer")
        || normalized.contains("this pc")
}

fn is_known_this_pc_window_title(title: &str) -> bool {
    title.contains("\u{6b64}\u{7535}\u{8111}") || title.to_ascii_lowercase().contains("this pc")
}

fn resolve_first_anchor_label(
    scope: AnchorScope,
    window_title: Option<&str>,
    needles: &[&str],
    require_mouse: bool,
) -> Option<String> {
    needles.iter().find_map(|needle| {
        resolve_anchor(scope, needle, window_title)
            .ok()
            .filter(|anchor| {
                if !require_mouse {
                    return true;
                }
                anchor_accepts_mouse(anchor).unwrap_or(false)
            })
            .map(|anchor| anchor.label)
    })
}

fn execute_action(
    action: &DesktopAutomationAction,
    target_window: Option<&str>,
    goal: &str,
) -> Result<(), String> {
    let kind = action
        .kind
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match kind.as_str() {
        "open_this_pc" => Err(
            "open_this_pc system-launch fallback is disabled; use visible mouse/keyboard actions"
                .to_string(),
        ),
        "open_start_menu" => open_start_menu_by_mouse(),
        "focus_window" => {
            let title = action
                .window_title
                .as_deref()
                .or(target_window)
                .ok_or_else(|| "focus_window requires window_title".to_string())?;
            focus_window(title)
        }
        "minimize_window" => click_window_control(
            WindowControl::Minimize,
            action.window_title.as_deref().or(target_window),
        ),
        "close_window" => click_window_control(
            WindowControl::Close,
            action.window_title.as_deref().or(target_window),
        ),
        "click_anchor" => click_anchor(action, target_window, goal, 1),
        "double_click_anchor" => click_anchor(action, target_window, goal, 2),
        "click" => click_at(action, 1),
        "double_click" => click_at(action, 2),
        "type_text" => {
            if let Some(title) = action.window_title.as_deref().or(target_window) {
                let _ = focus_window(title);
            }
            let text = action
                .text
                .as_deref()
                .ok_or_else(|| "type_text requires text".to_string())?;
            type_text(text)
        }
        "key_press" => {
            if let Some(title) = action.window_title.as_deref().or(target_window) {
                let _ = focus_window(title);
            }
            let key = action
                .key
                .as_deref()
                .or_else(|| {
                    action
                        .keys
                        .as_ref()
                        .and_then(|keys| keys.first().map(String::as_str))
                })
                .ok_or_else(|| "key_press requires key".to_string())?;
            press_key(key)
        }
        "hotkey" => {
            if let Some(title) = action.window_title.as_deref().or(target_window) {
                let _ = focus_window(title);
            }
            let keys = action
                .keys
                .as_ref()
                .ok_or_else(|| "hotkey requires keys".to_string())?;
            send_hotkey(keys)
        }
        "hold_key" => {
            if let Some(title) = action.window_title.as_deref().or(target_window) {
                let _ = focus_window(title);
            }
            let key = action
                .key
                .as_deref()
                .or_else(|| {
                    action
                        .keys
                        .as_ref()
                        .and_then(|keys| keys.first().map(String::as_str))
                })
                .ok_or_else(|| "hold_key requires key".to_string())?;
            hold_key(
                key,
                action
                    .hold_ms
                    .or(action.duration_ms)
                    .unwrap_or(DEFAULT_WAIT_MS)
                    .min(2_000),
            )
        }
        "move_mouse_relative" => {
            move_mouse_relative(action.dx.unwrap_or_default(), action.dy.unwrap_or_default())
        }
        "wait" => {
            thread::sleep(Duration::from_millis(
                action.duration_ms.unwrap_or(DEFAULT_WAIT_MS).min(10_000),
            ));
            Ok(())
        }
        "scroll" => {
            if let Some(title) = action.window_title.as_deref().or(target_window) {
                let _ = focus_window(title);
            }
            scroll_wheel(action.delta.unwrap_or(-120))
        }
        other => Err(format!("unsupported desktop automation action: {other}")),
    }
}

fn click_at(action: &DesktopAutomationAction, clicks: u32) -> Result<(), String> {
    let x = action
        .x
        .ok_or_else(|| "click action requires x".to_string())?;
    let y = action
        .y
        .ok_or_else(|| "click action requires y".to_string())?;
    click_point(x, y, clicks)
}

fn click_anchor(
    action: &DesktopAutomationAction,
    target_window: Option<&str>,
    goal: &str,
    clicks: u32,
) -> Result<(), String> {
    let requested_scope =
        parse_anchor_scope(action.anchor_scope.as_deref()).unwrap_or(AnchorScope::Window);
    let window_title = action.window_title.as_deref().or(target_window);
    let scope = normalize_anchor_scope(requested_scope, goal, window_title);
    let label = action
        .text
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| infer_anchor_label(scope, goal, window_title))
        .ok_or_else(|| "anchor action requires text or an inferable goal".to_string())?;

    if scope == AnchorScope::Window {
        if let Some(title) = window_title {
            let _ = focus_window(title);
        }
    }

    let resolved = resolve_anchor(scope, &label, window_title)?;

    if scope == AnchorScope::Desktop {
        return click_desktop_anchor(&resolved, clicks);
    }

    let (x, y) = anchor_interaction_point(&resolved);
    click_point(x, y, clicks)
}

fn anchor_interaction_point(anchor: &UiAnchor) -> (i32, i32) {
    let width = (anchor.right - anchor.left).max(1);
    let height = (anchor.bottom - anchor.top).max(1);
    match anchor.scope {
        AnchorScope::Desktop => {
            let x = anchor.left + width / 2;
            let y = anchor.top + (height / 3).clamp(16, 28);
            (x, y)
        }
        AnchorScope::Taskbar => {
            let x = anchor.left + (width / 3).clamp(10, 24);
            let y = anchor.top + height / 2;
            (x, y)
        }
        AnchorScope::Window => anchor.center(),
    }
}

fn click_desktop_anchor(anchor: &UiAnchor, clicks: u32) -> Result<(), String> {
    let _ = focus_desktop_surface();
    let primary = anchor_interaction_point(anchor);
    let center = anchor.center();
    let lower = (
        primary.0,
        (anchor.top + ((anchor.bottom - anchor.top) * 2 / 3))
            .clamp(anchor.top + 4, anchor.bottom - 4),
    );
    let candidates = [primary, center, lower];

    for (x, y) in candidates {
        click_point(x, y, 1)?;
        thread::sleep(Duration::from_millis(220));
        let state = desktop_item_state(&anchor.label).unwrap_or_default();
        if !state.found {
            continue;
        }
        if state.selected || state.focused {
            if clicks > 1 {
                click_point(x, y, 1)?;
                thread::sleep(Duration::from_millis(320));
            }
            return Ok(());
        }

        let _ = focus_desktop_surface();
        if clicks > 1 {
            click_point(x, y, 1)?;
            thread::sleep(Duration::from_millis(320));
            return Ok(());
        }
    }

    Ok(())
}

fn desktop_item_state(label: &str) -> Result<DesktopItemState, String> {
    let escaped = escape_powershell_single_quoted(label);
    let output = run_powershell(
        &format!(
            r#"
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeDesktop {{
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct LVITEMW {{
    public uint mask;
    public int iItem;
    public int iSubItem;
    public uint state;
    public uint stateMask;
    public IntPtr pszText;
    public int cchTextMax;
    public int iImage;
    public IntPtr lParam;
    public int iIndent;
    public int iGroupId;
    public uint cColumns;
    public IntPtr puColumns;
    public IntPtr piColFmt;
    public int iGroup;
  }}
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr childAfter, string className, string windowTitle);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr hObject);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr VirtualAllocEx(IntPtr hProcess, IntPtr address, UIntPtr size, uint allocType, uint protect);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool VirtualFreeEx(IntPtr hProcess, IntPtr address, UIntPtr size, uint freeType);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadProcessMemory(IntPtr hProcess, IntPtr address, byte[] buffer, int size, out IntPtr read);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool WriteProcessMemory(IntPtr hProcess, IntPtr address, byte[] buffer, int size, out IntPtr written);
}}
'@
$LVM_GETITEMCOUNT = 0x1004
$LVM_GETITEMTEXTW = 0x1073
$LVM_GETITEMSTATE = 0x102C
$LVIF_TEXT = 0x0001
$LVIS_FOCUSED = 0x0001
$LVIS_SELECTED = 0x0002
$PROCESS_ACCESS = 0x0438
$MEM_COMMIT = 0x1000
$MEM_RESERVE = 0x2000
$MEM_RELEASE = 0x8000
$PAGE_READWRITE = 0x04
$targetName = '{escaped}'
$progman = [NativeDesktop]::FindWindow('Progman', 'Program Manager')
if ($progman -eq [IntPtr]::Zero) {{
    [pscustomobject]@{{ found = $false; selected = $false; focused = $false }} | ConvertTo-Json -Compress
    exit 0
}}
$defView = [NativeDesktop]::FindWindowEx($progman, [IntPtr]::Zero, 'SHELLDLL_DefView', $null)
if ($defView -eq [IntPtr]::Zero) {{
    [pscustomobject]@{{ found = $false; selected = $false; focused = $false }} | ConvertTo-Json -Compress
    exit 0
}}
$list = [NativeDesktop]::FindWindowEx($defView, [IntPtr]::Zero, 'SysListView32', 'FolderView')
if ($list -eq [IntPtr]::Zero) {{
    [pscustomobject]@{{ found = $false; selected = $false; focused = $false }} | ConvertTo-Json -Compress
    exit 0
}}
$processId = [uint32]0
[void][NativeDesktop]::GetWindowThreadProcessId($list, [ref]$processId)
$process = [NativeDesktop]::OpenProcess($PROCESS_ACCESS, $false, $processId)
if ($process -eq [IntPtr]::Zero) {{
    [pscustomobject]@{{ found = $false; selected = $false; focused = $false }} | ConvertTo-Json -Compress
    exit 0
}}
$remoteText = [IntPtr]::Zero
$remoteItem = [IntPtr]::Zero
try {{
    $count = [NativeDesktop]::SendMessage($list, $LVM_GETITEMCOUNT, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
    $lvItemSize = [Runtime.InteropServices.Marshal]::SizeOf([type][NativeDesktop+LVITEMW])
    $remoteText = [NativeDesktop]::VirtualAllocEx($process, [IntPtr]::Zero, [UIntPtr]::new(1024), $MEM_COMMIT -bor $MEM_RESERVE, $PAGE_READWRITE)
    $remoteItem = [NativeDesktop]::VirtualAllocEx($process, [IntPtr]::Zero, [UIntPtr]::new([uint64]$lvItemSize), $MEM_COMMIT -bor $MEM_RESERVE, $PAGE_READWRITE)
    if ($remoteText -eq [IntPtr]::Zero -or $remoteItem -eq [IntPtr]::Zero) {{
        [pscustomobject]@{{ found = $false; selected = $false; focused = $false }} | ConvertTo-Json -Compress
        exit 0
    }}
    for ($i = 0; $i -lt $count; $i++) {{
        $lv = New-Object NativeDesktop+LVITEMW
        $lv.mask = $LVIF_TEXT
        $lv.iItem = $i
        $lv.iSubItem = 0
        $lv.pszText = $remoteText
        $lv.cchTextMax = 260
        $lvPtr = [Runtime.InteropServices.Marshal]::AllocHGlobal($lvItemSize)
        try {{
            [Runtime.InteropServices.Marshal]::StructureToPtr($lv, $lvPtr, $false)
            $lvBytes = New-Object byte[] $lvItemSize
            [Runtime.InteropServices.Marshal]::Copy($lvPtr, $lvBytes, 0, $lvItemSize)
            $written = [IntPtr]::Zero
            [void][NativeDesktop]::WriteProcessMemory($process, $remoteItem, $lvBytes, $lvBytes.Length, [ref]$written)
        }} finally {{
            [Runtime.InteropServices.Marshal]::FreeHGlobal($lvPtr)
        }}
        [void][NativeDesktop]::SendMessage($list, $LVM_GETITEMTEXTW, [IntPtr]$i, $remoteItem)
        $textBytes = New-Object byte[] 1024
        $read = [IntPtr]::Zero
        [void][NativeDesktop]::ReadProcessMemory($process, $remoteText, $textBytes, $textBytes.Length, [ref]$read)
        $name = [Text.Encoding]::Unicode.GetString($textBytes).Split([char]0)[0]
        if ([string]::IsNullOrWhiteSpace($name)) {{ continue }}
        if ($name -like \"*$targetName*\") {{
            $state = [NativeDesktop]::SendMessage(
                $list,
                $LVM_GETITEMSTATE,
                [IntPtr]$i,
                [IntPtr]($LVIS_FOCUSED -bor $LVIS_SELECTED)
            ).ToInt32()
            [pscustomobject]@{{
                found = $true
                selected = (($state -band $LVIS_SELECTED) -ne 0)
                focused = (($state -band $LVIS_FOCUSED) -ne 0)
            }} | ConvertTo-Json -Compress
            exit 0
        }}
    }}
    [pscustomobject]@{{ found = $false; selected = $false; focused = $false }} | ConvertTo-Json -Compress
}} finally {{
    if ($remoteText -ne [IntPtr]::Zero) {{ [void][NativeDesktop]::VirtualFreeEx($process, $remoteText, [UIntPtr]::Zero, $MEM_RELEASE) }}
    if ($remoteItem -ne [IntPtr]::Zero) {{ [void][NativeDesktop]::VirtualFreeEx($process, $remoteItem, [UIntPtr]::Zero, $MEM_RELEASE) }}
    if ($process -ne [IntPtr]::Zero) {{ [void][NativeDesktop]::CloseHandle($process) }}
}}
"#,
        ),
        Duration::from_secs(8),
    )?;

    let value: serde_json::Value = serde_json::from_str(&output)
        .map_err(|error| format!("failed to parse desktop item state json: {error}"))?;
    Ok(DesktopItemState {
        found: value
            .get("found")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        selected: value
            .get("selected")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        focused: value
            .get("focused")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

fn normalize_anchor_scope(
    scope: AnchorScope,
    goal: &str,
    target_window: Option<&str>,
) -> AnchorScope {
    if scope != AnchorScope::Desktop {
        return scope;
    }

    let normalized_goal = goal.to_ascii_lowercase();
    let explorer_window = target_window.is_some_and(|title| {
        title.contains("资源管理器")
            || title.contains("文件资源管理器")
            || title.to_ascii_lowercase().contains("explorer")
    });
    let mentions_this_pc = goal.contains("此电脑")
        || goal.contains("我的电脑")
        || normalized_goal.contains("this pc")
        || normalized_goal.contains("my computer");

    if explorer_window && mentions_this_pc {
        AnchorScope::Window
    } else {
        AnchorScope::Desktop
    }
}

fn infer_anchor_label(
    scope: AnchorScope,
    goal: &str,
    target_window: Option<&str>,
) -> Option<String> {
    let normalized_goal = goal.to_ascii_lowercase();
    let mentions_this_pc = goal.contains("此电脑")
        || goal.contains("我的电脑")
        || normalized_goal.contains("this pc")
        || normalized_goal.contains("my computer");
    let mentions_explorer = goal.contains("资源管理器")
        || goal.contains("文件资源管理器")
        || normalized_goal.contains("explorer")
        || target_window.is_some_and(|title| {
            title.contains("资源管理器")
                || title.contains("文件资源管理器")
                || title.to_ascii_lowercase().contains("explorer")
        });

    match scope {
        AnchorScope::Taskbar if mentions_this_pc || mentions_explorer => {
            Some("文件资源管理器".to_string())
        }
        AnchorScope::Desktop if mentions_this_pc => Some("此电脑".to_string()),
        AnchorScope::Window if mentions_this_pc => Some("此电脑".to_string()),
        AnchorScope::Window if mentions_explorer => Some("文件资源管理器".to_string()),
        _ => None,
    }
}
#[allow(dead_code)]
fn try_invoke_anchor(anchor: &UiAnchor) -> Result<bool, String> {
    let label = escape_powershell_single_quoted(&anchor.label);
    let script = match anchor.scope {
        AnchorScope::Taskbar => format!(
            r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$taskbars = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ClassNameProperty,
        'Shell_TrayWnd'
    ))
)
$target = $null
foreach ($taskbar in $taskbars) {{
    try {{
        $buttons = $taskbar.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            (New-Object System.Windows.Automation.PropertyCondition(
                [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                [System.Windows.Automation.ControlType]::Button
            ))
        )
        foreach ($button in $buttons) {{
            if ($button.Current.Name -like '*{label}*') {{
                $target = $button
                break
            }}
        }}
        if ($null -ne $target) {{ break }}
    }} catch {{}}
}}
if ($null -eq $target) {{ 'MISS'; exit 0 }}
$pattern = $null
if ($target.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {{
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
    'OK'
    exit 0
}}
if ($target.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {{
    ([System.Windows.Automation.SelectionItemPattern]$pattern).Select()
    'OK'
    exit 0
}}
if ($target.TryGetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern, [ref]$pattern)) {{
    ([System.Windows.Automation.LegacyIAccessiblePattern]$pattern).DoDefaultAction()
    'OK'
    exit 0
}}
'MISS'
"#
        ),
        AnchorScope::Window => {
            let window_title =
                escape_powershell_single_quoted(anchor.window_title.as_deref().unwrap_or_default());
            format!(
                r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$windows = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Children,
    (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Window
    ))
)
$window = $null
foreach ($candidate in $windows) {{
    try {{
        if ($candidate.Current.Name -like '*{window_title}*') {{
            $window = $candidate
            break
        }}
    }} catch {{}}
}}
if ($null -eq $window) {{ 'MISS'; exit 0 }}
$target = $null
$items = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
foreach ($item in $items) {{
    try {{
        if ($item.Current.Name -like '*{label}*') {{
            $target = $item
            break
        }}
    }} catch {{}}
}}
if ($null -eq $target) {{ 'MISS'; exit 0 }}
$pattern = $null
if ($target.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {{
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
    'OK'
    exit 0
}}
if ($target.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {{
    ([System.Windows.Automation.SelectionItemPattern]$pattern).Select()
    'OK'
    exit 0
}}
if ($target.TryGetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern, [ref]$pattern)) {{
    ([System.Windows.Automation.LegacyIAccessiblePattern]$pattern).DoDefaultAction()
    'OK'
    exit 0
}}
'MISS'
"#
            )
        }
        AnchorScope::Desktop => return Ok(false),
    };

    let output = run_powershell(&script, Duration::from_secs(8))?;
    Ok(output.trim().eq_ignore_ascii_case("OK"))
}

fn click_point(x: i32, y: i32, clicks: u32) -> Result<(), String> {
    inject_click_point(x, y, clicks, Duration::from_secs(6))
}

fn scroll_wheel(delta: i32) -> Result<(), String> {
    inject_scroll_wheel(delta, Duration::from_secs(6))
}

fn focus_window(title: &str) -> Result<(), String> {
    let escaped = escape_powershell_single_quoted(title);
    run_powershell(
        &format!(
            "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class WindowOps {{\n    [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);\n    [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr hWnd);\n}}\n'@; \
             Add-Type $signature; \
             $target = Get-Process | Where-Object {{ $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -like '*{escaped}*' }} | Select-Object -First 1; \
             if ($null -eq $target) {{ throw 'window not found'; }} \
             [WindowOps]::ShowWindowAsync([IntPtr]$target.MainWindowHandle, 9) | Out-Null; \
             Start-Sleep -Milliseconds 150; \
             [WindowOps]::SetForegroundWindow([IntPtr]$target.MainWindowHandle) | Out-Null;"
        ),
        Duration::from_secs(6),
    )
    .map(|_| ())
}

fn focus_desktop_surface() -> Result<(), String> {
    run_powershell(
        "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class DesktopOps {\n    [DllImport(\"user32.dll\")] public static extern IntPtr GetShellWindow();\n    [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);\n    [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr hWnd);\n}\n'@; \
         Add-Type $signature; \
         $shell = [DesktopOps]::GetShellWindow(); \
         if ($shell -eq [IntPtr]::Zero) { throw 'shell window not found'; } \
         [DesktopOps]::ShowWindowAsync($shell, 9) | Out-Null; \
         Start-Sleep -Milliseconds 120; \
         [DesktopOps]::SetForegroundWindow($shell) | Out-Null;",
        Duration::from_secs(6),
    )
    .map(|_| ())
}

#[derive(Clone, Copy)]
enum WindowControl {
    Minimize,
    Close,
}

fn click_window_control(control: WindowControl, title: Option<&str>) -> Result<(), String> {
    let title_filter = title.map(escape_powershell_single_quoted);
    let title_lookup = title_filter
        .as_deref()
        .map(|value| {
            format!(
                "$target = Get-Process | Where-Object {{ $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -like '*{value}*' }} | Select-Object -First 1; \
                 if ($null -ne $target) {{ \
                     $handle = [IntPtr]$target.MainWindowHandle; \
                 }} else {{ \
                     $handle = [WindowOps]::GetForegroundWindow(); \
                     if ($handle -eq [IntPtr]::Zero) {{ throw 'window not found'; }} \
                 }}"
            )
        })
        .unwrap_or_else(|| {
            "$handle = [WindowOps]::GetForegroundWindow(); if ($handle -eq [IntPtr]::Zero) { throw 'foreground window not found'; }".to_string()
        });

    let (offset_x, offset_y, control_names, name_contains) = match control {
        WindowControl::Minimize => (-135, 18, "'最小化','Minimize'", "'最小化','Minimize'"),
        WindowControl::Close => (-35, 18, "'关闭','Close'", "'关闭','Close'"),
    };

    let output = run_powershell(
        &format!(
            "Add-Type -AssemblyName UIAutomationClient; \
             Add-Type -AssemblyName UIAutomationTypes; \
             $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic struct RECT {{ public int Left; public int Top; public int Right; public int Bottom; }}\npublic static class WindowOps {{\n    [DllImport(\"user32.dll\")] public static extern IntPtr GetForegroundWindow();\n    [DllImport(\"user32.dll\")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);\n    [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);\n    [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr hWnd);\n    [DllImport(\"user32.dll\")] public static extern bool BringWindowToTop(IntPtr hWnd);\n    [DllImport(\"user32.dll\")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);\n}}\n'@; \
             Add-Type $signature; \
             {title_lookup} \
             [WindowOps]::ShowWindowAsync($handle, 9) | Out-Null; \
             Start-Sleep -Milliseconds 120; \
             $flags = 0x0001 -bor 0x0002 -bor 0x0040; \
             [WindowOps]::SetWindowPos($handle, [IntPtr]::new(-1), 0, 0, 0, 0, $flags) | Out-Null; \
             [WindowOps]::SetWindowPos($handle, [IntPtr]::new(-2), 0, 0, 0, 0, $flags) | Out-Null; \
             [WindowOps]::BringWindowToTop($handle) | Out-Null; \
             [WindowOps]::SetForegroundWindow($handle) | Out-Null; \
             Start-Sleep -Milliseconds 240; \
             $controlNames = @({control_names}); \
             $nameContains = @({name_contains}); \
             try {{ \
                 $element = [System.Windows.Automation.AutomationElement]::FromHandle($handle); \
                 if ($null -ne $element) {{ \
                     $buttons = $element.FindAll( \
                         [System.Windows.Automation.TreeScope]::Descendants, \
                         (New-Object System.Windows.Automation.PropertyCondition( \
                             [System.Windows.Automation.AutomationElement]::ControlTypeProperty, \
                             [System.Windows.Automation.ControlType]::Button \
                         )) \
                     ); \
                     foreach ($button in $buttons) {{ \
                         $name = $button.Current.Name; \
                         $automationId = $button.Current.AutomationId; \
                         $matches = $false; \
                         if ($controlNames -contains $name -or $controlNames -contains $automationId) {{ $matches = $true; }} \
                         foreach ($needle in $nameContains) {{ \
                             if (-not [string]::IsNullOrWhiteSpace($needle) -and ($name -like \"*$needle*\" -or $automationId -like \"*$needle*\")) {{ $matches = $true; }} \
                         }} \
                         if (-not $matches) {{ continue; }} \
                         $buttonRect = $button.Current.BoundingRectangle; \
                         if ($buttonRect.Right -gt $buttonRect.Left -and $buttonRect.Bottom -gt $buttonRect.Top) {{ \
                             $x = [int](($buttonRect.Left + $buttonRect.Right) / 2); \
                             $y = [int](($buttonRect.Top + $buttonRect.Bottom) / 2); \
                             \"$x,$y\"; \
                             exit 0; \
                         }} \
                     }} \
                 }} \
             }} catch {{}} \
             $rect = New-Object RECT; \
             if (-not [WindowOps]::GetWindowRect($handle, [ref]$rect)) {{ throw 'failed to read window rect'; }} \
             $x = $rect.Right + ({offset_x}); \
             $y = $rect.Top + ({offset_y}); \
             \"$x,$y\""
        ),
        Duration::from_secs(6),
    )?;

    let mut parts = output.trim().split(',');
    let x = parts
        .next()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .ok_or_else(|| format!("failed to parse window control x coordinate: {output}"))?;
    let y = parts
        .next()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .ok_or_else(|| format!("failed to parse window control y coordinate: {output}"))?;
    click_point(x, y, 1)
}

fn open_start_menu_by_mouse() -> Result<(), String> {
    let output = run_powershell(
        r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$root = [System.Windows.Automation.AutomationElement]::RootElement
$controls = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.Condition]::TrueCondition
)
foreach ($control in $controls) {
    try {
        $name = $control.Current.Name
        $automationId = $control.Current.AutomationId
        $rect = $control.Current.BoundingRectangle
        if ($rect.Right -le $rect.Left -or $rect.Bottom -le $rect.Top) { continue }
        if ($rect.Top -lt ($bounds.Bottom - 140)) { continue }
        if ($name -like '*搜索*' -or $name -like '*Search*' -or $automationId -like '*Search*') {
            $x = [int](($rect.Left + $rect.Right) / 2)
            $y = [int](($rect.Top + $rect.Bottom) / 2)
            "$x,$y"
            exit 0
        }
    } catch {}
}
foreach ($control in $controls) {
    try {
        $name = $control.Current.Name
        $automationId = $control.Current.AutomationId
        $rect = $control.Current.BoundingRectangle
        if ($rect.Right -le $rect.Left -or $rect.Bottom -le $rect.Top) { continue }
        if ($rect.Top -lt ($bounds.Bottom - 140)) { continue }
        if ($name -eq '开始' -or $name -eq 'Start' -or $automationId -like '*Start*') {
            if ($rect.Right -gt $rect.Left -and $rect.Bottom -gt $rect.Top) {
                $x = [int](($rect.Left + $rect.Right) / 2)
                $y = [int](($rect.Top + $rect.Bottom) / 2)
                "$x,$y"
                exit 0
            }
        }
    } catch {}
}
$x = [int]($bounds.Left + 160)
$y = [int]($bounds.Bottom - 28)
"$x,$y"
"#,
        Duration::from_secs(6),
    )?;

    let mut parts = output.trim().split(',');
    let x = parts
        .next()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .ok_or_else(|| format!("failed to parse start button x coordinate: {output}"))?;
    let y = parts
        .next()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .ok_or_else(|| format!("failed to parse start button y coordinate: {output}"))?;
    click_point(x, y, 1)
}

fn type_text(text: &str) -> Result<(), String> {
    inject_type_text(text, Duration::from_secs(8))
}

fn press_key(key: &str) -> Result<(), String> {
    let virtual_key =
        virtual_key_code(key).ok_or_else(|| format!("unsupported key_press token: {key}"))?;
    inject_press_virtual_key(virtual_key, Duration::from_secs(6))
}

fn send_hotkey(keys: &[String]) -> Result<(), String> {
    send_key_combo(keys)
}

fn hold_key(key: &str, hold_ms: u64) -> Result<(), String> {
    let virtual_key =
        virtual_key_code(key).ok_or_else(|| format!("unsupported hold_key token: {key}"))?;
    inject_hold_virtual_key(virtual_key, hold_ms, Duration::from_secs(6))
}

fn move_mouse_relative(dx: i32, dy: i32) -> Result<(), String> {
    inject_move_mouse_relative(dx, dy, Duration::from_secs(6))
}

fn send_key_combo(keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Err("hotkey requires keys".to_string());
    }

    let mut modifiers = Vec::new();
    let mut primary = None;

    for key in keys {
        let normalized = key.trim().to_ascii_uppercase();
        match normalized.as_str() {
            "CTRL" | "CONTROL" | "ALT" | "SHIFT" | "WIN" | "WINDOWS" | "LWIN" | "RWIN" => {
                modifiers.push(normalized);
            }
            _ => {
                primary = Some(normalized);
            }
        }
    }

    let primary =
        primary.ok_or_else(|| "hotkey requires at least one non-modifier key".to_string())?;

    let mut ordered = modifiers.clone();
    ordered.push(primary);

    let mut ordered_codes = Vec::new();
    for key in &ordered {
        let code =
            virtual_key_code(key).ok_or_else(|| format!("unsupported hotkey token: {key}"))?;
        ordered_codes.push(code);
    }

    inject_send_virtual_key_combo(&ordered_codes, Duration::from_secs(6))
}

fn probe_visible_windows() -> Result<Vec<DesktopWindowSummary>, String> {
    let output = run_powershell(
        r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class EnumWin {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
"@
$results = New-Object System.Collections.Generic.List[object]
$callback = [EnumWin+EnumWindowsProc]{
    param([IntPtr]$hWnd, [IntPtr]$lParam)
    if (-not [EnumWin]::IsWindowVisible($hWnd)) { return $true }
    $titleBuilder = New-Object System.Text.StringBuilder 512
    [void][EnumWin]::GetWindowText($hWnd, $titleBuilder, $titleBuilder.Capacity)
    $title = $titleBuilder.ToString()
    if ([string]::IsNullOrWhiteSpace($title)) { return $true }
    $processId = [uint32]0
    [void][EnumWin]::GetWindowThreadProcessId($hWnd, [ref]$processId)
    $processName = ''
    try { $processName = (Get-Process -Id ([int]$processId) -ErrorAction Stop).ProcessName } catch {}
    $results.Add([pscustomobject]@{
        Id = [int]$processId
        ProcessName = $processName
        MainWindowTitle = $title
    })
    return $true
}
[void][EnumWin]::EnumWindows($callback, [IntPtr]::Zero)
$results | ConvertTo-Json -Compress
"#,
        Duration::from_secs(6),
    )?;

    if output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let value: serde_json::Value = serde_json::from_str(&output)
        .map_err(|error| format!("failed to parse window probe json: {error}"))?;
    let list = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };

    let mut windows = Vec::new();
    for item in list {
        let process_id = item
            .get("Id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as u32;
        let process_name = item
            .get("ProcessName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let title = item
            .get("MainWindowTitle")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if process_id == 0 || process_name.is_empty() || title.is_empty() {
            continue;
        }
        windows.push(DesktopWindowSummary {
            process_id,
            process_name,
            title,
        });
    }

    Ok(windows)
}

fn publish_progress(
    tx: &Sender<DesktopAutomationEvent>,
    ctx: &DesktopAutomationContext,
    phase: DesktopAutomationPhase,
    detail: &str,
) {
    let snapshot = DesktopAutomationSnapshot {
        job_id: ctx.job_id,
        mode: ctx.mode,
        phase,
        step: ctx.step,
        max_steps: ctx.max_steps,
        detail: detail.to_string(),
        target_window: ctx.target_window.clone(),
        last_action: ctx.last_action.clone(),
        visible_windows: ctx.visible_windows.clone(),
        last_capture_path: ctx.last_capture_path.clone(),
        last_capture_size_bytes: ctx.last_capture_size_bytes,
    };
    let _ = tx.send(DesktopAutomationEvent::Progress(snapshot));
}

fn run_powershell(script: &str, timeout: Duration) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    let script = format!(
        "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::UTF8; \
         [Console]::InputEncoding = [System.Text.UTF8Encoding]::UTF8; \
         $dpiSignature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class DpiAwareness {{\n    [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware();\n}}\n'@; \
         Add-Type $dpiSignature; \
         [DpiAwareness]::SetProcessDPIAware() | Out-Null; \
         {script}"
    );
    thread::spawn(move || {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Sta", "-Command", &script]);
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);

        let result = command.output().map_err(|error| {
            format!("failed to launch PowerShell for desktop automation: {error}")
        });
        let _ = tx.send(result);
    });

    let output = rx
        .recv_timeout(timeout)
        .map_err(|_| {
            format!(
                "desktop automation PowerShell timed out after {}s",
                timeout.as_secs()
            )
        })?
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "desktop automation PowerShell failed: {stdout} {stderr}"
        ))
    }
}

fn describe_action(action: &DesktopAutomationAction) -> String {
    let kind = action
        .kind
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match kind.as_str() {
        "open_this_pc" => "open_this_pc()".to_string(),
        "open_start_menu" => "open_start_menu()".to_string(),
        "focus_window" => format!(
            "focus_window({})",
            action.window_title.as_deref().unwrap_or("unknown")
        ),
        "minimize_window" => format!(
            "minimize_window({})",
            action.window_title.as_deref().unwrap_or("foreground")
        ),
        "close_window" => format!(
            "close_window({})",
            action.window_title.as_deref().unwrap_or("foreground")
        ),
        "click_anchor" => format!(
            "click_anchor({}:{})",
            action.anchor_scope.as_deref().unwrap_or("window"),
            action.text.as_deref().unwrap_or("unknown")
        ),
        "double_click_anchor" => format!(
            "double_click_anchor({}:{})",
            action.anchor_scope.as_deref().unwrap_or("window"),
            action.text.as_deref().unwrap_or("unknown")
        ),
        "click" => format!(
            "click({}, {})",
            action.x.unwrap_or_default(),
            action.y.unwrap_or_default()
        ),
        "double_click" => format!(
            "double_click({}, {})",
            action.x.unwrap_or_default(),
            action.y.unwrap_or_default()
        ),
        "type_text" => "type_text(...)".to_string(),
        "key_press" => format!(
            "key_press({})",
            action
                .key
                .clone()
                .or_else(|| action.keys.as_ref().and_then(|keys| keys.first().cloned()))
                .unwrap_or_else(|| "unknown".to_string())
        ),
        "hotkey" => format!(
            "hotkey({})",
            action.keys.clone().unwrap_or_default().join("+")
        ),
        "hold_key" => format!(
            "hold_key({}, {}ms)",
            action
                .key
                .clone()
                .or_else(|| action.keys.as_ref().and_then(|keys| keys.first().cloned()))
                .unwrap_or_else(|| "unknown".to_string()),
            action
                .hold_ms
                .or(action.duration_ms)
                .unwrap_or(DEFAULT_WAIT_MS)
        ),
        "move_mouse_relative" => format!(
            "move_mouse_relative({}, {})",
            action.dx.unwrap_or_default(),
            action.dy.unwrap_or_default()
        ),
        "wait" => format!("wait({}ms)", action.duration_ms.unwrap_or(DEFAULT_WAIT_MS)),
        "scroll" => format!("scroll({})", action.delta.unwrap_or(-120)),
        other => other.to_string(),
    }
}

fn validate_action_for_mode(
    ctx: &DesktopAutomationContext,
    action: &DesktopAutomationAction,
) -> Result<(), String> {
    let kind = action
        .kind
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match ctx.mode {
        DesktopAutomationMode::DesktopSoftware => {
            if kind == "hotkey" {
                Err(
                    "desktop software mode forbids hotkey actions; use mouse or key_press"
                        .to_string(),
                )
            } else {
                Ok(())
            }
        }
        DesktopAutomationMode::RealtimeGameAssist => {
            if kind == "hotkey" {
                Err(
                    "realtime assist mode forbids hotkey actions; use hold_key or key_press"
                        .to_string(),
                )
            } else {
                Ok(())
            }
        }
    }
}

fn virtual_key_code(key: &str) -> Option<u8> {
    match key.trim().to_ascii_uppercase().as_str() {
        "A" => Some(0x41),
        "B" => Some(0x42),
        "C" => Some(0x43),
        "D" => Some(0x44),
        "E" => Some(0x45),
        "F" => Some(0x46),
        "G" => Some(0x47),
        "H" => Some(0x48),
        "I" => Some(0x49),
        "J" => Some(0x4A),
        "K" => Some(0x4B),
        "L" => Some(0x4C),
        "M" => Some(0x4D),
        "N" => Some(0x4E),
        "O" => Some(0x4F),
        "P" => Some(0x50),
        "Q" => Some(0x51),
        "R" => Some(0x52),
        "S" => Some(0x53),
        "T" => Some(0x54),
        "U" => Some(0x55),
        "V" => Some(0x56),
        "W" => Some(0x57),
        "X" => Some(0x58),
        "Y" => Some(0x59),
        "Z" => Some(0x5A),
        "0" => Some(0x30),
        "1" => Some(0x31),
        "2" => Some(0x32),
        "3" => Some(0x33),
        "4" => Some(0x34),
        "5" => Some(0x35),
        "6" => Some(0x36),
        "7" => Some(0x37),
        "8" => Some(0x38),
        "9" => Some(0x39),
        "SPACE" => Some(0x20),
        "SHIFT" => Some(0x10),
        "CTRL" | "CONTROL" => Some(0x11),
        "ALT" => Some(0x12),
        "WIN" | "WINDOWS" | "LWIN" => Some(0x5B),
        "RWIN" => Some(0x5C),
        "TAB" => Some(0x09),
        "ENTER" => Some(0x0D),
        "BACKSPACE" => Some(0x08),
        "DELETE" => Some(0x2E),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PGUP" | "PAGEUP" => Some(0x21),
        "PGDN" | "PAGEDOWN" => Some(0x22),
        "ESC" | "ESCAPE" => Some(0x1B),
        "UP" => Some(0x26),
        "DOWN" => Some(0x28),
        "LEFT" => Some(0x25),
        "RIGHT" => Some(0x27),
        "F1" => Some(0x70),
        "F2" => Some(0x71),
        "F3" => Some(0x72),
        "F4" => Some(0x73),
        "F5" => Some(0x74),
        "F6" => Some(0x75),
        "F7" => Some(0x76),
        "F8" => Some(0x77),
        "F9" => Some(0x78),
        "F10" => Some(0x79),
        "F11" => Some(0x7A),
        "F12" => Some(0x7B),
        _ => None,
    }
}

fn extract_json_object(input: &str) -> Option<String> {
    let start = input.find('{')?;
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in input[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(input[start..start + index + 1].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

fn ensure_live_mode() -> Result<(), String> {
    let has_key = ["ZAI_API_KEY", "BIGMODEL_API_KEY", "OPENAI_API_KEY"]
        .iter()
        .any(|key| env::var(key).is_ok_and(|value| !value.trim().is_empty()));
    if has_key {
        Ok(())
    } else {
        Err("桌面代理需要可用的在线视觉模型 API Key.".to_string())
    }
}

fn unique_job_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(1)
}

fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

pub fn is_desktop_automation_prompt(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    let action_keywords = [
        "打开",
        "启动",
        "点击",
        "双击",
        "输入",
        "键入",
        "填写",
        "操作",
        "帮我在桌面",
        "帮我打开",
        "帮我点击",
        "帮我输入",
        "帮我完成",
        "切换到",
        "关闭",
        "选择",
        "勾选",
        "提交",
    ];

    action_keywords
        .iter()
        .any(|keyword| prompt.contains(keyword))
        || is_realtime_game_prompt(prompt)
        || normalized.contains("click ")
        || normalized.contains("double click")
        || normalized.contains("type ")
        || normalized.contains("open ")
        || normalized.contains("launch ")
        || normalized.contains("fill in ")
        || normalized.contains("press ")
}

pub fn is_realtime_game_prompt(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    let game_keywords = [
        "游戏", "战斗", "视角", "镜头", "角色", "前进", "后退", "冲刺", "跳跃", "技能", "释放",
        "躲避", "实时", "高频", "game", "combat", "camera", "aim", "move", "strafe", "jump",
        "sprint",
    ];

    game_keywords
        .iter()
        .any(|keyword| prompt.contains(keyword) || normalized.contains(keyword))
}

pub fn classify_desktop_automation_mode(prompt: &str) -> DesktopAutomationMode {
    if is_realtime_game_prompt(prompt) {
        DesktopAutomationMode::RealtimeGameAssist
    } else {
        DesktopAutomationMode::DesktopSoftware
    }
}

pub fn mode_label(mode: DesktopAutomationMode) -> &'static str {
    match mode {
        DesktopAutomationMode::DesktopSoftware => "桌面软件",
        DesktopAutomationMode::RealtimeGameAssist => "实时辅助",
    }
}

pub fn phase_label(phase: DesktopAutomationPhase) -> &'static str {
    match phase {
        DesktopAutomationPhase::Preparing => "准备中",
        DesktopAutomationPhase::ProbingWindows => "窗口探测",
        DesktopAutomationPhase::Capturing => "截图中",
        DesktopAutomationPhase::Reasoning => "视觉规划",
        DesktopAutomationPhase::Executing => "动作执行",
        DesktopAutomationPhase::Completed => "已完成",
        DesktopAutomationPhase::Failed => "失败",
    }
}

fn max_steps_for_mode(mode: DesktopAutomationMode) -> usize {
    match mode {
        DesktopAutomationMode::DesktopSoftware => MAX_AUTOMATION_STEPS,
        DesktopAutomationMode::RealtimeGameAssist => MAX_REALTIME_ASSIST_STEPS,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_desktop_automation_mode, extract_json_object, is_desktop_automation_prompt,
        is_realtime_game_prompt, virtual_key_code, DesktopAutomationMode,
    };

    #[test]
    fn detects_desktop_automation_keywords() {
        assert!(is_desktop_automation_prompt("帮我打开微信并点击联系人"));
        assert!(is_desktop_automation_prompt("click the green button"));
        assert!(!is_desktop_automation_prompt("看一下当前桌面"));
    }

    #[test]
    fn detects_realtime_game_keywords() {
        assert!(is_realtime_game_prompt("在游戏里按住W向前移动"));
        assert!(is_realtime_game_prompt("move the camera to the left"));
        assert_eq!(
            classify_desktop_automation_mode("在游戏里按住W向前移动"),
            DesktopAutomationMode::RealtimeGameAssist
        );
    }

    #[test]
    fn extracts_first_json_object_from_model_output() {
        let raw = "```json\n{\"status\":\"done\",\"summary\":\"ok\"}\n```";
        assert_eq!(
            extract_json_object(raw).expect("json should extract"),
            "{\"status\":\"done\",\"summary\":\"ok\"}"
        );
    }

    #[test]
    fn maps_common_virtual_keys() {
        assert_eq!(virtual_key_code("W"), Some(0x57));
        assert_eq!(virtual_key_code("SPACE"), Some(0x20));
        assert_eq!(virtual_key_code("WIN"), Some(0x5B));
        assert_eq!(virtual_key_code("F4"), Some(0x73));
        assert_eq!(virtual_key_code("unknown"), None);
    }
}
