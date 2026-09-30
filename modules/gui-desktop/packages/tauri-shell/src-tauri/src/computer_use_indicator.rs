//! CU展示器：透明、不可聚焦、鼠标穿透；不持有输入、审批或页面控制权限。
use std::{sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use native_browser_protocol::ComputerUseActivityReceipt;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindowBuilder};

const PREFIX: &str = "cu-indicator-";
#[derive(Clone, PartialEq, Eq)]
struct Layout { x:i32, y:i32, width:u32, height:u32, scale:u64, primary:bool }
#[derive(Default)]
struct DisplayState { until:Option<Instant>, revision:u64, layouts:Vec<Layout> }
fn state() -> &'static Mutex<DisplayState> {
    static STATE: OnceLock<Mutex<DisplayState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(DisplayState::default()))
}

pub(super) fn start_watchdog(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let expired = state().lock().ok().is_some_and(|state|
                state.until.is_some_and(|until| until <= Instant::now()));
            if expired { refresh(&app, None); }
        }
    });
}

pub(super) fn refresh(app: &AppHandle, receipt: Option<ComputerUseActivityReceipt>) {
    let revision = {
        let Ok(mut state) = state().lock() else { return; };
        let until = receipt.filter(ComputerUseActivityReceipt::valid_shape)
            .filter(|receipt| receipt.active)
            .map(|receipt| Instant::now() + Duration::from_millis(receipt.lease_ms));
        if state.until.is_none() && until.is_none() { return; }
        state.until = until;
        state.revision = state.revision.wrapping_add(1);
        state.revision
    };
    let app = app.clone();
    let target = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !current(revision) { return; }
        let active = state().lock().ok().is_some_and(|state|
            state.until.is_some_and(|until| until > Instant::now()));
        if !active || render(&target, revision).is_err() { hide(&target); }
    });
}

fn current(revision: u64) -> bool {
    state().lock().ok().is_some_and(|state| state.revision == revision)
}

fn hide(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(PREFIX) { let _ = window.hide(); }
    }
}

fn render(app: &AppHandle, revision: u64) -> Result<(), Box<dyn std::error::Error>> {
    let monitors = app.available_monitors()?;
    let primary = app.primary_monitor()?.ok_or("无法确定主显示器")?;
    if monitors.is_empty() { return Err("没有可用显示器".into()); }
    let mut layouts: Vec<_> = monitors.iter().map(|monitor| Layout {
        x:monitor.position().x, y:monitor.position().y,
        width:monitor.size().width, height:monitor.size().height,
        scale:monitor.scale_factor().to_bits(), primary:monitor.position() == primary.position(),
    }).collect();
    layouts.sort_by_key(|layout| (layout.x, layout.y));
    let changed = state().lock().map_err(|_| "展示状态不可读")?.layouts != layouts;
    if changed {
        for (label, window) in app.webview_windows() {
            if label.starts_with(PREFIX) { window.destroy()?; }
        }
        // 建WebView可能跨越一次心跳；记录布局避免下一刷新反复销毁半初始化窗口。
        state().lock().map_err(|_| "展示状态不可读")?.layouts = layouts.clone();
    }
    for (index, layout) in layouts.iter().enumerate() {
        let label = format!("{PREFIX}{index}");
        let window = if let Some(window) = app.get_webview_window(&label) { window } else {
            let asset = if layout.primary {"computer-use-indicator.html"} else {"computer-use-edges.html"};
            // 与开机演出一样明确使用打包资源协议，避免devUrl指向8765而找不到静态页。
            #[cfg(any(windows, target_os = "android"))]
            let url = tauri::Url::parse(&format!("http://tauri.localhost/{asset}"))?;
            #[cfg(not(any(windows, target_os = "android")))]
            let url = tauri::Url::parse(&format!("tauri://localhost/{asset}"))?;
            let window = WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::CustomProtocol(url))
                .title("").visible(false).focused(false).focusable(false)
                .decorations(false).transparent(true).shadow(false).always_on_top(true)
                .skip_taskbar(true).resizable(false).disable_drag_drop_handler()
                .on_navigation(|url| (url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost"))
                    && matches!(url.path(), "/computer-use-indicator.html" | "/computer-use-edges.html"))
                .build()?;
            window
        };
        // 穿透失败时保持隐藏；已有但初始化失败的窗口也须重新验证，不能绕过设置。
        window.set_ignore_cursor_events(true)?;
        window.set_position(PhysicalPosition::new(layout.x, layout.y))?;
        window.set_size(PhysicalSize::new(layout.width, layout.height))?;
        // 主线程建窗可能排队较久，显示前再核本次刷新及展示租约。
        if !current(revision) || !state().lock().map_err(|_| "展示状态不可读")?
            .until.is_some_and(|until| until > Instant::now()) { return Ok(()); }
        if !window.is_visible()? { window.show()?; }
    }
    if current(revision) { state().lock().map_err(|_| "展示状态不可读")?.layouts = layouts; }
    Ok(())
}
