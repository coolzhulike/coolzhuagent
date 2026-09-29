//! 右侧网页是独立、无 capability 的 WebView。主界面只传位置与导航命令，
//! 网页的 URL、加载状态由原生回调提供，不维护猜测出来的浏览历史。
use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicU64, Ordering}, Mutex, OnceLock};
use tauri::{
    webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder},
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Url, Webview,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct PanelBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelAction {
    Metrics,
    Navigate,
    Resize,
    Hide,
    Close,
    Back,
    Forward,
    Reload,
    Stop,
}

#[derive(Debug, Deserialize)]
pub struct PanelCommand {
    action: PanelAction,
    scope: String,
    url: Option<String>,
    bounds: Option<PanelBounds>,
    host_bounds: Option<PanelBounds>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PanelReply {
    scope: String,
    url: Option<String>,
    title: Option<String>,
    loading: bool,
    active: bool,
    hidden: bool,
    destroyed: bool,
    reason: Option<String>,
    error: Option<String>,
    bounds: Option<PanelBounds>,
    can_go_back: Option<bool>,
    can_go_forward: Option<bool>,
    window_scale_factor: Option<f64>,
}

#[derive(Default)]
struct PanelState {
    generation: u64,
    navigation_revision: u64,
    label: Option<String>,
    reply: PanelReply,
    pending_navigation: Option<PendingNavigation>,
}

struct PendingNavigation {
    revision: u64,
    requested_url: String,
    requested_observed: bool,
    redirect_url: Option<String>,
    from_popup: bool,
}

#[derive(Default)]
pub struct PanelStore {
    gate: tokio::sync::Mutex<()>,
    state: Mutex<PanelState>,
}

// 启动后固定信任来源；不能由之后发生的临时文件变化扩大 IPC 的信任边界。
fn console_origin() -> Option<&'static Url> {
    static ORIGIN: OnceLock<Option<Url>> = OnceLock::new();
    ORIGIN
        .get_or_init(|| {
            Url::parse(&super::gui_web_url())
                .ok()
                .filter(is_loopback_http)
        })
        .as_ref()
}

fn is_loopback_http(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
        && url.username().is_empty()
        && url.password().is_none()
}

pub fn trusted_console_url(url: &Url) -> bool {
    console_origin().is_some_and(|origin| origin.origin() == url.origin())
}

pub fn pin_console_origin() {
    let _ = console_origin();
}

fn trusted_asset_url(url: &Url) -> bool {
    (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"))
        || (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || trusted_console_url(url)
}

pub fn trusted_custom_command(label: &str, url: &Url, command: &str) -> bool {
    match label {
        super::CONSOLE_LABEL if trusted_console_url(url) => matches!(
            command,
            "toggle_console"
                | "show_console_command"
                | "hide_console_command"
                | "quit_app"
                | "start_pet_dragging"
                | "report_throne_zone"
                | "stabilize_pet_window_command"
                | "pet_status"
                | "set_pet_action"
                | "pet_drop_uploaded"
                | "browser_window_command"
                | "open_browser_window"
                | "browser_panel_command"
                | "confirm_recovery"
        ),
        super::PET_LABEL if trusted_asset_url(url) => matches!(
            command,
            "toggle_console"
                | "show_console_command"
                | "hide_console_command"
                | "quit_app"
                | "start_pet_dragging"
                | "stabilize_pet_window_command"
                | "pet_status"
                | "set_pet_action"
                | "pet_drop_uploaded"
        ),
        super::STARTUP_PERFORMANCE_LABEL
            if super::trusted_startup_performance_url(url) =>
        {
            command == "report_startup_performance"
        }
        _ => false,
    }
}

fn validate_url(value: &str, origin: Option<&Url>) -> Result<Url, String> {
    let url = Url::parse(value.trim()).map_err(|_| "网页地址无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("网页仅支持不含账号密码的 HTTP 或 HTTPS 地址".into());
    }
    // 同端口的 loopback 别名也不得把本地控制台装入无权限网页容器。
    if origin.is_some_and(|trusted| {
        trusted.origin() == url.origin()
            || (is_loopback_http(&url)
                && url.port_or_known_default() == trusted.port_or_known_default())
    }) {
        return Err("本地控制台不能在网页预览中打开".into());
    }
    Ok(url)
}

fn clamp_bounds(bounds: PanelBounds, width: f64, height: f64) -> Result<PanelBounds, String> {
    clamp_host_bounds(bounds, None, width, height)
}

fn clamp_host_bounds(bounds: PanelBounds, host: Option<PanelBounds>, width: f64, height: f64) -> Result<PanelBounds, String> {
    if ![
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        width,
        height,
    ]
    .iter()
    .all(|v| v.is_finite())
        || bounds.width <= 0.0
        || bounds.height <= 0.0
        || width < 100.0
        || height <= 49.0
    {
        return Err("网页显示区域无效".into());
    }
    // 只有可信 console 能给出真实右栏容器；窄屏抽屉仍保留快捷轨和顶栏，
    // 且内容矩形必须落在该容器内。旧宿主未声明容器时保留 40% 的保守边界。
    let (left, top, host_right, host_bottom) = if let Some(host) = host {
        if ![host.x,host.y,host.width,host.height].iter().all(|v| v.is_finite())
            || host.width <= 0.0 || host.height <= 0.0 || host.x + host.width < width * 0.75 {
            return Err("网页宿主不是有效的右侧扩展栏".into());
        }
        (host.x.max(58.0), host.y.max(48.0), (host.x + host.width).min(width), (host.y + host.height).min(height))
    } else { (width * 0.4, 48.0, width, height) };
    let x = bounds.x.max(left).min(width);
    let y = bounds.y.max(top).min(height);
    let right = (bounds.x + bounds.width).min(host_right);
    let bottom = (bounds.y + bounds.height).min(host_bottom);
    if right - x < 1.0 || bottom - y < 1.0 {
        return Err("网页显示区域不在右侧扩展栏内".into());
    }
    Ok(PanelBounds {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

fn emit(app: &AppHandle, reply: &PanelReply) {
    let _ = app.emit_to(
        EventTarget::Webview {
            label: super::CONSOLE_LABEL.into(),
        },
        "browser-panel-state",
        reply,
    );
}

// 仅显式开启时写入桌面壳的诊断文件；不记录 URL、查询参数或聊天室 scope。
fn log_navigation_diagnostic(
    app: &AppHandle,
    generation: u64,
    stage: &str,
    outcome: &str,
    url: Option<&Url>,
) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !*ENABLED.get_or_init(|| {
        std::env::var("COOLZHU_BROWSER_NAV_DIAGNOSTICS")
            .is_ok_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true"))
    }) {
        return;
    }
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let store = app.state::<PanelStore>();
    let Ok(state) = store.state.lock() else { return; };
    let pending = state.pending_navigation.as_ref();
    let fields = [
        ("sequence", SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string()),
        ("stage", stage.to_string()),
        ("outcome", outcome.to_string()),
        ("generation", generation.to_string()),
        ("state_generation", state.generation.to_string()),
        ("revision", state.navigation_revision.to_string()),
        ("pending_revision", pending.map_or("none".into(), |value| value.revision.to_string())),
        ("pending_popup", pending.is_some_and(|value| value.from_popup).to_string()),
        ("requested_match", url.zip(pending).is_some_and(|(url, pending)| pending.requested_url == url.as_str()).to_string()),
        ("panel_url_match", url.is_some_and(|url| state.reply.url.as_deref() == Some(url.as_str())).to_string()),
        ("active", state.reply.active.to_string()),
        ("scheme", url.map_or("none", Url::scheme).to_string()),
    ];
    drop(state);
    diagnostics::info("browser_panel", "navigation_diagnostic", "内置浏览器导航事件", &fields);
}

fn validation_diagnostic_outcome(result: &Result<Url, String>, url: &Url) -> &'static str {
    if result.is_ok() { return "allowed"; }
    if !matches!(url.scheme(), "http" | "https") { return "rejected_scheme"; }
    if url.host_str().is_none() { return "rejected_host"; }
    if !url.username().is_empty() || url.password().is_some() { return "rejected_credentials"; }
    "rejected_console_or_other"
}

fn update(app: &AppHandle, generation: u64, change: impl FnOnce(&mut PanelReply)) {
    let store = app.state::<PanelStore>();
    let reply = {
        let Ok(mut state) = store.state.lock() else {
            return;
        };
        if state.generation != generation || !state.reply.active {
            return;
        }
        change(&mut state.reply);
        state.reply.clone()
    };
    emit(app, &reply);
}

fn update_page_load(app: &AppHandle, generation: u64, url: &Url, actual_url: Option<Url>, event: PageLoadEvent) {
    let store = app.state::<PanelStore>();
    let reply = {
        let Ok(mut state) = store.state.lock() else { return; };
        if state.generation != generation || !state.reply.active { return; }
        let page_url = url.to_string();
        let redirected = if let Some(pending) = state.pending_navigation.as_ref() {
            if pending.revision != state.navigation_revision { return; }
            if pending.requested_url != page_url {
                // 旧 A 与“B 重定向回 A”具有相同 URL；只接受本次 B 启动后观察到的跳转。
                let observed_redirect = pending.requested_observed
                    && pending.redirect_url.as_deref() == Some(page_url.as_str())
                    && actual_url.as_ref() == Some(url);
                if !observed_redirect { return; }
                true
            } else { false }
        } else { false };
        // 重定向后的首个有效加载事件应为 ContentLoading。旧 A 的迟到
        // NavigationCompleted 也可能报告 A，不能据此结束 B 的请求。
        if redirected && !matches!(event, PageLoadEvent::Started) { return; }
        if state.pending_navigation.is_some() {
            state.pending_navigation = None;
        }
        if matches!(event, PageLoadEvent::Started) {
            state.reply.url = Some(page_url);
            state.reply.loading = true;
        } else {
            // 旧页面的完成事件不能把连续导航的新地址写回右栏。
            if state.reply.url.as_deref() != Some(page_url.as_str()) { return; }
            state.reply.loading = false;
        }
        state.reply.reason = None;
        state.reply.error = None;
        state.reply.clone()
    };
    emit(app, &reply);
    log_navigation_diagnostic(app, generation, "page_load_applied", "accepted", Some(url));
}

fn observe_navigation(app: &AppHandle, generation: u64, url: &Url) {
    let store = app.state::<PanelStore>();
    let Ok(mut state) = store.state.lock() else { return; };
    if state.generation != generation || !state.reply.active { return; }
    let revision = state.navigation_revision;
    let Some(pending) = state.pending_navigation.as_mut() else { return; };
    if pending.revision != revision { return; }
    let next_url = url.to_string();
    if pending.requested_url == next_url {
        pending.requested_observed = true;
        pending.redirect_url = None;
    } else if pending.requested_observed {
        pending.redirect_url = Some(next_url);
    }
}

fn popup_navigation_current(
    state: &PanelState,
    generation: u64,
    scope: &str,
    label: &str,
    revision: Option<u64>,
) -> bool {
    state.generation == generation
        && state.reply.active
        && state.reply.scope == scope
        && state.label.as_deref() == Some(label)
        && revision.is_none_or(|revision| {
            state.pending_navigation.as_ref().is_some_and(|pending| pending.revision == revision && pending.from_popup)
                && state.navigation_revision == revision
        })
}

fn cancel_popup_navigation(store: &PanelStore, generation: u64, scope: &str) -> Result<(), String> {
    let mut state = store.state.lock().map_err(|_| "网页状态不可用")?;
    if state.generation != generation || !state.reply.active || state.reply.scope != scope {
        return Err("网页已关闭或所属聊天室已切换".into());
    }
    if state.pending_navigation.as_ref().is_some_and(|pending| pending.from_popup) {
        state.navigation_revision = state.navigation_revision.wrapping_add(1);
        // 旧请求已失效；不能留下 revision 不匹配的 pending 阻止后续加载事件收尾。
        state.pending_navigation = None;
    }
    Ok(())
}

fn popup_navigation_error(
    app: &AppHandle,
    generation: u64,
    scope: &str,
    label: &str,
    revision: Option<u64>,
    message: &str,
) {
    let store = app.state::<PanelStore>();
    let reply = {
        let Ok(mut state) = store.state.lock() else { return; };
        if !popup_navigation_current(&state, generation, scope, label, revision) { return; }
        if revision.is_some() {
            state.pending_navigation = None;
            state.reply.loading = false;
        }
        state.reply.error = Some(message.into());
        state.reply.clone()
    };
    emit(app, &reply);
}

fn handle_new_window(app: &AppHandle, generation: u64, scope: &str, label: &str, url: Url) {
    log_navigation_diagnostic(app, generation, "new_window_callback", "received", Some(&url));
    let validation = validate_url(url.as_str(), console_origin());
    if validation.is_err() {
        log_navigation_diagnostic(app, generation, "popup_validation", validation_diagnostic_outcome(&validation, &url), Some(&url));
        popup_navigation_error(app, generation, scope, label, None, "已阻止不支持的网页跳转");
        return;
    }
    let revision = {
        let store = app.state::<PanelStore>();
        let Ok(mut state) = store.state.lock() else { return; };
        if !popup_navigation_current(&state, generation, scope, label, None) { None } else {
            state.navigation_revision = state.navigation_revision.wrapping_add(1);
            let revision = state.navigation_revision;
            state.pending_navigation = Some(PendingNavigation {
                revision,
                requested_url: url.to_string(),
                requested_observed: false,
                redirect_url: None,
                from_popup: true,
            });
            Some(revision)
        }
    };
    let Some(revision) = revision else {
        log_navigation_diagnostic(app, generation, "popup_request", "stale_view", Some(&url));
        return;
    };
    log_navigation_diagnostic(app, generation, "popup_request", "staged", Some(&url));
    let queued_app = app.clone();
    let queued_scope = scope.to_owned();
    let queued_label = label.to_owned();
    // Windows 的新窗口回调在 Wry 工作线程上执行；此处仅投递主线程任务，
    // 不把投递顺序当作 WebView2 的 NewWindowRequested deferral 完成确认。
    if app.run_on_main_thread(move || {
        let store = queued_app.state::<PanelStore>();
        log_navigation_diagnostic(&queued_app, generation, "popup_task", "entered", Some(&url));
        let current = store.state.lock().is_ok_and(|state| {
            popup_navigation_current(&state, generation, &queued_scope, &queued_label, Some(revision))
        });
        if !current {
            log_navigation_diagnostic(&queued_app, generation, "popup_task", "stale", Some(&url));
            return;
        }
        let Some(view) = queued_app.get_webview(&queued_label) else {
            log_navigation_diagnostic(&queued_app, generation, "popup_task", "view_missing", Some(&url));
            popup_navigation_error(&queued_app, generation, &queued_scope, &queued_label, Some(revision), "网页窗口尚未就绪");
            return;
        };
        if view.navigate(url.clone()).is_err() {
            log_navigation_diagnostic(&queued_app, generation, "popup_dispatch", "dispatch_failed", Some(&url));
            popup_navigation_error(&queued_app, generation, &queued_scope, &queued_label, Some(revision), "无法打开新窗口目标网页");
            return;
        }
        // Tauri 返回 Ok 只确认派发，底层 WebView2 load_url 错误不经该返回值传递。
        log_navigation_diagnostic(&queued_app, generation, "popup_dispatch", "dispatched", Some(&url));
        let reply = {
            let Ok(mut state) = store.state.lock() else { return; };
            if !popup_navigation_current(&state, generation, &queued_scope, &queued_label, Some(revision)) { return; }
            state.reply.url = Some(url.to_string());
            state.reply.loading = true;
            state.reply.error = None;
            state.reply.clone()
        };
        emit(&queued_app, &reply);
    }).is_err() {
        log_navigation_diagnostic(app, generation, "popup_task", "schedule_failed", None);
        popup_navigation_error(app, generation, scope, label, Some(revision), "无法调度新窗口目标网页");
    }
}

/// 窗口缩放期间隐藏子视图，待前端用新矩形定位后再显示；保留网页历史与表单。
pub fn suspend(app: &AppHandle, reason: &str) {
    let store = app.state::<PanelStore>();
    let (label, reply) = {
        let Ok(mut state) = store.state.lock() else { return; };
        if !state.reply.active || state.reply.hidden { return; }
        state.reply.hidden = true;
        state.reply.reason = Some(reason.into());
        (state.label.clone(), state.reply.clone())
    };
    if let Some(view) = label.and_then(|label| app.get_webview(&label)) {
        let _ = view.hide();
    }
    emit(app, &reply);
}

/// 真正关闭或切换所属聊天室时销毁隔离视图。
pub fn invalidate(app: &AppHandle, reason: &str) {
    let store = app.state::<PanelStore>();
    let (label, reply) = {
        let Ok(mut state) = store.state.lock() else {
            return;
        };
        if !state.reply.active {
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        state.reply.active = false;
        state.reply.destroyed = true;
        state.reply.hidden = false;
        state.reply.loading = false;
        state.reply.reason = Some(reason.into());
        state.pending_navigation = None;
        (state.label.take(), state.reply.clone())
    };
    if let Some(view) = label.and_then(|label| app.get_webview(&label)) {
        let _ = view.close();
    }
    emit(app, &reply);
}

fn current_reply(store: &PanelStore) -> Result<PanelReply, String> {
    store
        .state
        .lock()
        .map(|state| state.reply.clone())
        .map_err(|_| "网页状态不可用".into())
}

fn view_for_scope(
    app: &AppHandle,
    store: &PanelStore,
    scope: &str,
) -> Result<(Webview, u64), String> {
    let state = store.state.lock().map_err(|_| "网页状态不可用")?;
    if state.reply.scope != scope || !state.reply.active {
        return Err("网页已关闭或所属聊天室已切换".into());
    }
    let view = state
        .label
        .as_ref()
        .and_then(|label| app.get_webview(label))
        .ok_or("网页窗口尚未就绪")?;
    Ok((view, state.generation))
}

async fn native_history_or_stop(view: &Webview, action: &PanelAction) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let operation = match action {
            PanelAction::Back => 0,
            PanelAction::Forward => 1,
            _ => 2,
        };
        view.with_webview(move |platform| {
            // COM 对象只能在 WebView 所属的 UI 线程访问；with_webview 保证这个前提。
            let result = unsafe {
                platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| match operation {
                        0 => core.GoBack(),
                        1 => core.GoForward(),
                        _ => core.Stop(),
                    })
            }
            .map_err(|_| "浏览器未能执行该操作".to_string());
            let _ = tx.send(result);
        })
        .map_err(|_| "网页窗口不可用".to_string())?;
        rx.await.map_err(|_| "浏览器操作已取消".to_string())?
    }
    #[cfg(not(windows))]
    {
        let script = match action {
            PanelAction::Back => "history.back()",
            PanelAction::Forward => "history.forward()",
            _ => "window.stop()",
        };
        view.eval(script).map_err(|_| "网页操作未能发送".into())
    }
}

#[tauri::command]
pub async fn browser_panel_command(
    app: AppHandle,
    webview: Webview,
    store: tauri::State<'_, PanelStore>,
    command: PanelCommand,
) -> Result<PanelReply, String> {
    if webview.label() != super::CONSOLE_LABEL
        || !webview
            .url()
            .ok()
            .is_some_and(|url| trusted_console_url(&url))
    {
        return Err("此页面无权控制网页预览".into());
    }
    if command.scope.is_empty()
        || command.scope.len() > 1024
        || command.scope.chars().any(char::is_control)
    {
        return Err("网页所属聊天室无效".into());
    }
    let _serial = store.gate.lock().await;
    if matches!(command.action, PanelAction::Close) {
        if current_reply(&store)?.scope == command.scope {
            invalidate(&app, "closed");
        }
        return current_reply(&store);
    }
    let console = app
        .get_window(super::CONSOLE_LABEL)
        .ok_or("控制台窗口不可用")?;
    if matches!(command.action, PanelAction::Metrics) {
        let mut reply = current_reply(&store)?;
        reply.window_scale_factor = Some(console.scale_factor().map_err(|_| "无法读取窗口缩放")?);
        return Ok(reply);
    }
    let bounds = if matches!(command.action, PanelAction::Navigate | PanelAction::Resize) {
        let logical = console
            .inner_size()
            .map_err(|_| "无法读取窗口尺寸")?
            .to_logical::<f64>(console.scale_factor().map_err(|_| "无法读取窗口缩放")?);
        Some(clamp_host_bounds(
            command.bounds.ok_or("缺少网页显示区域")?,
            command.host_bounds,
            logical.width,
            logical.height,
        )?)
    } else {
        None
    };

    if matches!(command.action, PanelAction::Navigate) {
        let url = validate_url(
            command.url.as_deref().ok_or("缺少网页地址")?,
            console_origin(),
        )?;
        let command_generation = store.state.lock().map_err(|_| "网页状态不可用")?.generation;
        log_navigation_diagnostic(&app, command_generation, "command_navigate", "requested", Some(&url));
        if let Ok((view, generation)) = view_for_scope(&app, &store, &command.scope) {
            let bounds = bounds.unwrap();
            view.set_bounds(tauri::Rect {
                position: LogicalPosition::new(bounds.x, bounds.y).into(),
                size: LogicalSize::new(bounds.width, bounds.height).into(),
            })
            .map_err(|_| "无法调整网页位置")?;
            // 重复展示同一地址不会刷新网页，也不丢失页面内的未提交内容。
            let pending_other = store.state.lock().map_err(|_| "网页状态不可用")?
                .pending_navigation.as_ref().is_some_and(|pending| pending.requested_url != url.as_str());
            let navigating = view.url().ok().as_ref() != Some(&url) || pending_other;
            let requested_revision = if navigating {
                {
                    let mut state = store.state.lock().map_err(|_| "网页状态不可用")?;
                    state.navigation_revision = state.navigation_revision.wrapping_add(1);
                    state.pending_navigation = Some(PendingNavigation {
                        revision: state.navigation_revision,
                        requested_url: url.to_string(),
                        requested_observed: false, redirect_url: None, from_popup: false,
                    });
                    Some(state.navigation_revision)
                }
            } else { None };
            if navigating { view.navigate(url.clone()).map_err(|_| "无法打开网页")?; }
            view.show().map_err(|_| "无法显示网页")?;
            let reply = {
                let mut state = store.state.lock().map_err(|_| "网页状态不可用")?;
                if state.generation != generation || !state.reply.active {
                    return Err("网页已关闭或所属聊天室已切换".into());
                }
                state.reply.bounds = Some(bounds);
                // 加载事件可能先于 navigate 命令返回；迟到的命令回执不得写回 B。
                if !navigating || state.pending_navigation.as_ref().is_some_and(|pending|
                    Some(pending.revision) == requested_revision) {
                    state.reply.url = Some(url.to_string());
                    if navigating { state.reply.loading = true; }
                }
                state.reply.hidden = false;
                state.reply.error = None;
                state.reply.clone()
            };
            emit(&app, &reply);
            return Ok(reply);
        }
        invalidate(&app, "scope-changed");
        let bounds = bounds.unwrap();
        let popup_scope = command.scope.clone();
        let (generation, label) = {
            let mut state = store.state.lock().map_err(|_| "网页状态不可用")?;
            state.generation = state.generation.wrapping_add(1);
            state.pending_navigation = None;
            let label = format!("browser-panel-{}", state.generation);
            state.label = Some(label.clone());
            state.reply = PanelReply {
                scope: command.scope,
                url: Some(url.to_string()),
                active: true,
                bounds: Some(bounds),
                ..Default::default()
            };
            (state.generation, label)
        };
        let navigation_app = app.clone();
        let load_app = app.clone();
        let title_app = app.clone();
        let popup_app = app.clone();
        let popup_label = label.clone();
        let data = app
            .path()
            .app_local_data_dir()
            .map_err(|_| "无法读取网页数据目录")?
            .join("browser-panel");
        let builder = WebviewBuilder::new(&label, tauri::WebviewUrl::External(url))
            .data_directory(data)
            .disable_drag_drop_handler()
            .devtools(false)
            .on_navigation(move |url| {
                let validation = validate_url(url.as_str(), console_origin());
                let allowed = validation.is_ok();
                log_navigation_diagnostic(&navigation_app, generation, "navigation_starting", validation_diagnostic_outcome(&validation, url), Some(url));
                if allowed {
                    observe_navigation(&navigation_app, generation, url);
                } else {
                    update(&navigation_app, generation, |reply| {
                        reply.error = Some("已阻止不支持的网页跳转".into())
                    });
                }
                allowed
            })
            .on_page_load(move |view, payload| {
                let event = payload.event();
                log_navigation_diagnostic(&load_app, generation, "page_load_observed", if matches!(event, PageLoadEvent::Started) { "started" } else { "finished" }, Some(payload.url()));
                update_page_load(&load_app, generation, payload.url(), view.url().ok(), event);
            })
            .on_document_title_changed(move |_view, title| {
                update(&title_app, generation, |reply| reply.title = Some(title));
            })
            .on_new_window(move |url, _features| {
                handle_new_window(&popup_app, generation, &popup_scope, &popup_label, url);
                NewWindowResponse::Deny
            });
        // add_child 自己切换到 UI 线程；本异步命令不可再套 run_on_main_thread。
        match console.add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width, bounds.height),
        ) {
            Ok(view) => {
                let alive =
                    store.state.lock().map_err(|_| "网页状态不可用")?.generation == generation;
                if !alive {
                    let _ = view.close();
                    return Err("网页创建期间窗口或聊天室已变化".into());
                }
            }
            Err(_) => {
                update(&app, generation, |reply| {
                    reply.error = Some("无法创建网页预览".into())
                });
                invalidate(&app, "creation-failed");
                return Err("无法创建网页预览".into());
            }
        }
        let reply = current_reply(&store)?;
        emit(&app, &reply);
        return Ok(reply);
    }

    let (view, generation) = view_for_scope(&app, &store, &command.scope)?;
    if matches!(command.action, PanelAction::Back | PanelAction::Forward | PanelAction::Reload | PanelAction::Stop) {
        cancel_popup_navigation(&store, generation, &command.scope)?;
    }
    match command.action {
        PanelAction::Resize => {
            let bounds = bounds.unwrap();
            view.set_bounds(tauri::Rect {
                position: LogicalPosition::new(bounds.x, bounds.y).into(),
                size: LogicalSize::new(bounds.width, bounds.height).into(),
            })
            .map_err(|_| "无法调整网页位置")?;
            view.show().map_err(|_| "无法显示网页")?;
            update(&app, generation, |reply| { reply.bounds = Some(bounds); reply.hidden = false; });
        }
        PanelAction::Hide => {
            view.hide().map_err(|_| "无法隐藏网页")?;
            update(&app, generation, |reply| reply.hidden = true);
        }
        PanelAction::Reload => view.reload().map_err(|_| "无法刷新网页")?,
        PanelAction::Back | PanelAction::Forward | PanelAction::Stop => {
            native_history_or_stop(&view, &command.action).await?;
            #[cfg(windows)]
            if matches!(command.action, PanelAction::Stop) {
                update(&app, generation, |reply| reply.loading = false);
            }
        }
        _ => unreachable!(),
    }
    current_reply(&store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_navigation_rejects_privileged_schemes_credentials_and_console_aliases() {
        let origin = Url::parse("http://127.0.0.1:8765/").unwrap();
        for value in [
            "javascript:alert(1)",
            "file:///C:/private",
            "tauri://localhost",
            "https://a:b@example.com",
            "http://localhost:8765/",
            "http://127.0.0.1:8765/api",
        ] {
            assert!(validate_url(value, Some(&origin)).is_err(), "{value}");
        }
        for value in [
            "https://example.com/a",
            "https://accounts.example.net/login",
            "http://127.0.0.1:3000/preview",
        ] {
            assert!(validate_url(value, Some(&origin)).is_ok(), "{value}");
        }
    }

    #[test]
    fn unknown_webview_never_inherits_custom_commands_even_at_trusted_origin() {
        for label in ["browser-panel-1", "external-browser", "console-forged"] {
            assert!(!trusted_custom_command(
                label,
                &Url::parse(super::super::DEFAULT_GUI_WEB_URL).unwrap(),
                "quit_app"
            ));
        }
        assert!(!trusted_custom_command(
            "console",
            &Url::parse("https://example.com").unwrap(),
            "browser_panel_command"
        ));
        assert!(!trusted_custom_command(
            "console",
            &Url::parse(super::super::DEFAULT_GUI_WEB_URL).unwrap(),
            "unregistered_command"
        ));
    }

    #[test]
    fn startup_report_command_requires_exact_performance_webview_and_page() {
        let page = super::super::startup_performance_url().unwrap();
        assert!(trusted_custom_command(
            "launch-performance",
            &page,
            "report_startup_performance"
        ));
        assert!(trusted_custom_command(
            "launch-performance",
            &Url::parse(&format!("{page}?mode=first")).unwrap(),
            "report_startup_performance"
        ));
        assert!(!trusted_custom_command(
            "console",
            &page,
            "report_startup_performance"
        ));
        assert!(!trusted_custom_command(
            "launch-performance",
            &Url::parse("https://example.com/launch-performance.html").unwrap(),
            "report_startup_performance"
        ));
        assert!(!trusted_custom_command(
            "launch-performance",
            &Url::parse(&format!("{page}?mode=first&secret=1")).unwrap(),
            "report_startup_performance"
        ));
        assert!(!trusted_custom_command(
            "launch-performance",
            &page,
            "quit_app"
        ));
    }

    #[test]
    fn bounds_cannot_cover_main_chat_header_or_escape_client_area() {
        let bounds = clamp_bounds(
            PanelBounds {
                x: -500.0,
                y: -20.0,
                width: 2000.0,
                height: 1500.0,
            },
            1000.0,
            700.0,
        )
        .unwrap();
        assert_eq!(
            bounds,
            PanelBounds {
                x: 400.0,
                y: 48.0,
                width: 600.0,
                height: 652.0
            }
        );
        assert!(clamp_bounds(
            PanelBounds {
                x: f64::NAN,
                ..bounds
            },
            1000.0,
            700.0
        )
        .is_err());
        assert!(clamp_bounds(
            PanelBounds {
                x: 1200.0,
                ..bounds
            },
            1000.0,
            700.0
        )
        .is_err());
    }

    #[test]
    fn drawer_bounds_stay_inside_declared_sidebar_and_preserve_quick_rail() {
        let host = PanelBounds { x: 60.0, y: 80.0, width: 820.0, height: 600.0 };
        let actual = clamp_host_bounds(PanelBounds { x: 0.0, y: 0.0, width: 1200.0, height: 900.0 }, Some(host), 900.0, 700.0).unwrap();
        assert_eq!(actual, host);
        assert!(clamp_host_bounds(actual, Some(PanelBounds { width: 100.0, ..host }), 900.0, 700.0).is_err());
    }

    #[test]
    fn capability_grants_only_named_trusted_webviews() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert!(capability.get("windows").is_none());
        assert_eq!(
            capability["webviews"],
            serde_json::json!(["console", "pet", "launch-performance"])
        );
    }
}
