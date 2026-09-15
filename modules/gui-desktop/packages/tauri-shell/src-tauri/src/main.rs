#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::sync::{Mutex, OnceLock};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, Url, WebviewWindowBuilder, WindowEvent,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CONSOLE_LABEL: &str = "console";
const PET_LABEL: &str = "pet";
const EXTERNAL_BROWSER_LABEL: &str = "external-browser";
const DEFAULT_GUI_WEB_URL: &str = "http://127.0.0.1:8765";
const PET_WINDOW_SIZE: f64 = 152.0;
const PET_THEME_JSON: &str = include_str!("../../ui/assets/pet-theme.json");
const WEB_CONSOLE_PID_ARG: &str = "--web-console-pid";
const WEB_CONSOLE_PID_ENV: &str = "COOLZHU_WEB_CONSOLE_PID";
const WEB_CONSOLE_PROCESS_NAME: &str = "coolzhu-web-console.exe";
const DIAGNOSTICS_MODULE: &str = "tauri-shell";
static WEB_CONSOLE_PARENT_MONITOR_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static PET_NATIVE_DRAG_RELEASE_WATCHER_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static PET_CROWNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static THRONE_ZONE: OnceLock<Mutex<Option<ThroneZone>>> = OnceLock::new();
static BROWSER_RUNTIME_STATE: OnceLock<Mutex<BrowserRuntimeState>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, Serialize)]
enum BrowserCommand {
    Navigate { url: String },
    Back,
    Forward,
    Reload,
    Stop,
    Focus,
    Close,
}

#[derive(Clone, Debug, Default, Serialize)]
struct BrowserWindowState {
    url: Option<String>,
    title: Option<String>,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    last_error: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct BrowserRuntimeState {
    response: BrowserWindowState,
    history: Vec<String>,
    history_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct ThroneZone {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct PetThroneEvent {
    phase: &'static str,
    x: i32,
    y: i32,
}

#[derive(Debug, Deserialize)]
struct PetTheme {
    #[serde(default)]
    asset_version: String,
    default_state: String,
    states: BTreeMap<String, PetThemeState>,
    #[serde(default)]
    event_map: Vec<PetEventMapping>,
}

#[derive(Debug, Deserialize)]
struct PetThemeState {
    frame_pattern: String,
    frame_count: usize,
    interval_ms: u64,
    priority: i32,
    min_duration_ms: u64,
    #[serde(default)]
    auto_return_ms: Option<u64>,
    message: String,
    #[serde(default)]
    bubble: Option<String>,
    #[serde(default)]
    frame_offsets: Vec<f32>,
    #[serde(default)]
    frame_scales: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct PetEventMapping {
    #[serde(rename = "match")]
    match_kind: String,
    value: String,
    state: String,
    message: String,
}

/// 启动时快速探测本机常见代理端口（Clash-verge/mihomo 默认 7897、Clash 经典 7890 等）。
/// 仅做 TCP 连通性检测（150ms 超时），返回首个可连端口。用于给 WebView2 environment 统一配代理。
fn detect_local_proxy_port() -> Option<u16> {
    for port in [7897u16, 7890, 7891, 2080] {
        if let Ok(addr) = format!("127.0.0.1:{port}").parse::<std::net::SocketAddr>() {
            if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150))
                .is_ok()
            {
                return Some(port);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StartupVisibility {
    show_console: bool,
    show_pet: bool,
}

fn startup_visibility(args: &[String]) -> StartupVisibility {
    let show_console = args.iter().any(|arg| arg == "--show-console");
    let state_update_requested = args
        .iter()
        .any(|arg| arg == "--pet-state" || arg.starts_with("--pet-state="));
    let pet_requested = args.iter().any(|arg| {
        arg == "--pet"
            || arg == "--show-pet"
            || arg == "--pet-action"
            || arg.starts_with("--pet-action=")
            || arg == "--pet-event"
            || arg.starts_with("--pet-event=")
    });
    StartupVisibility {
        show_console: show_console || (!pet_requested && !state_update_requested),
        show_pet: pet_requested,
    }
}

fn main() {
    if let Err(error) = diagnostics::init("coolzhu-tauri-shell") {
        diagnostics::error_event(
            DIAGNOSTICS_MODULE,
            "diagnostics_init_failed",
            "Failed to initialize diagnostics during startup",
            &error,
            &[
                ("phase", "startup".to_string()),
                ("component", "diagnostics".to_string()),
            ],
        );
    }

    // 启动时探测本机代理（Clash/mihomo 等），统一设到整个 WebView2 environment：独立浏览器窗口
    // （抖音等被 fake-ip 劫持的站）走代理即可正常加载；本地回环（控制台/桌宠连 8765）bypass 直连。
    // 用全局 environment 代理而非单窗口 proxy_url，是因为 WebView2 同一 user-data 目录的 environment
    // 选项必须一致，给单个窗口设独立 proxy 会与主窗口冲突、在主线程 build 时卡死（已实测多次）。
    // WebView2 启动参数始终设置：禁用后台 timer 节流与渲染器降级，确保桌宠窗口失焦/被遮挡时
    // setInterval（5 分钟空闲表演检查、1.2s 窗口稳定）与动画 tick 仍可靠运行；有本机代理时再追加 proxy。
    let mut webview2_args = String::from(
        "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows",
    );
    if let Some(port) = detect_local_proxy_port() {
        webview2_args.push_str(&format!(
            " --proxy-server=http://127.0.0.1:{port} --proxy-bypass-list=127.0.0.1;localhost;[::1]"
        ));
    }
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", webview2_args);
    let run_result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            spawn_web_console_parent_monitor_if_requested(app, &args);
            let visibility = startup_visibility(&args);
            if handle_pet_action_args(app, &args) {
                if visibility.show_console {
                    show_console(app);
                }
                return;
            }
            if visibility.show_pet {
                show_pet(app);
                emit_pet_status(app, "success", "桌宠已由 Web 控制台唤起");
            }
            if visibility.show_console {
                show_console(app);
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            toggle_console,
            show_console_command,
            hide_console_command,
            quit_app,
            start_pet_dragging,
            report_throne_zone,
            stabilize_pet_window_command,
            pet_status,
            set_pet_action,
            pet_drop_uploaded,
            browser_window_command,
            open_browser_window
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(error) = build_tray(&handle) {
                diagnostics::error_event(
                    DIAGNOSTICS_MODULE,
                    "tray_build_failed",
                    "Failed to build Tauri tray during startup",
                    error.as_ref(),
                    &[
                        ("phase", "startup".to_string()),
                        ("component", "tray".to_string()),
                    ],
                );
                return Err(error);
            }
            if let Err(error) = build_console_window(&handle) {
                diagnostics::error_event(
                    DIAGNOSTICS_MODULE,
                    "console_window_build_failed",
                    "Failed to build console window during startup",
                    error.as_ref(),
                    &[
                        ("phase", "startup".to_string()),
                        ("component", "console-window".to_string()),
                    ],
                );
                return Err(error);
            }
            if let Err(error) = build_pet_window(&handle) {
                diagnostics::error_event(
                    DIAGNOSTICS_MODULE,
                    "pet_window_build_failed",
                    "Failed to build pet window during startup",
                    error.as_ref(),
                    &[
                        ("phase", "startup".to_string()),
                        ("component", "pet-window".to_string()),
                    ],
                );
                return Err(error);
            }
            let startup_args: Vec<String> = std::env::args().collect();
            spawn_web_console_parent_monitor_if_requested(&handle, &startup_args);
            let handled_pet_action = handle_pet_action_args(&handle, &startup_args);
            let visibility = startup_visibility(&startup_args);
            if visibility.show_console {
                show_console(&handle);
            }
            if visibility.show_pet && !handled_pet_action {
                show_pet(&handle);
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = run_result {
        diagnostics::error_event(
            DIAGNOSTICS_MODULE,
            "tauri_run_failed",
            "Tauri application run returned an error",
            &error,
            &[("phase", "run".to_string())],
        );
        std::process::exit(1);
    }
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let tray_icon = app.default_window_icon().cloned().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Tauri default window icon is missing",
        )
    })?;
    let show_item = MenuItem::with_id(app, "show", "显示控制台", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide", "隐藏控制台", true, None::<&str>)?;
    let pet_item = MenuItem::with_id(app, "pet", "显示桌宠", true, None::<&str>)?;
    let hide_pet_item = MenuItem::with_id(app, "hide-pet", "隐藏桌宠", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 COOLZHU AGENT", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &hide_item,
            &pet_item,
            &hide_pet_item,
            &quit_item,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_console(app),
            "hide" => hide_console(app),
            "pet" => show_pet(app),
            "hide-pet" => hide_pet(app),
            "quit" => quit_application(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                do_toggle_console(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn build_console_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if app.get_webview_window(CONSOLE_LABEL).is_some() {
        return Ok(());
    }

    let console_url = format!("{}/", gui_web_url().trim_end_matches('/'));

    let console = WebviewWindowBuilder::new(
        app,
        CONSOLE_LABEL,
        tauri::WebviewUrl::External(console_url.parse()?),
    )
    .title("COOLZHU AGENT 控制台")
    .inner_size(1440.0, 900.0)
    // Windows 高 DPI 会显著缩小逻辑工作区；创建时以当前显示器工作区（含任务栏）
    // 为边界，并降低最小尺寸，避免 250% 缩放下窗口底部永久落在屏幕外。
    .min_inner_size(900.0, 520.0)
    .center()
    .prevent_overflow_with_margin(tauri::LogicalSize::new(16.0, 16.0))
    .visible(false)
    .focused(true)
    .build()?;

    let app_for_close = app.clone();
    console.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            // 标题栏的关闭按钮必须真的退出。原实现是 prevent_close + hide：窗口消失了，
            // 但 Tauri 进程、托盘和 8765 上的 agent 全部继续驻留，用户看到的就是
            // "点了关闭却没退出"。隐藏到托盘仍可由托盘菜单「隐藏控制台」和托盘左键切换完成，
            // 不再借用窗口关闭语义。
            // 先 prevent_close 再回收：避免窗口关闭触发主循环退出、抢在回收 web-console 之前。
            api.prevent_close();
            quit_application(&app_for_close);
        }
    });

    Ok(())
}

fn build_pet_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if app.get_webview_window(PET_LABEL).is_some() {
        return Ok(());
    }

    let pet_window = WebviewWindowBuilder::new(
        app,
        PET_LABEL,
        tauri::WebviewUrl::App("pet-mini.html".into()),
    )
    .title("")
    .inner_size(PET_WINDOW_SIZE, PET_WINDOW_SIZE)
    .min_inner_size(PET_WINDOW_SIZE, PET_WINDOW_SIZE)
    .max_inner_size(PET_WINDOW_SIZE, PET_WINDOW_SIZE)
    .position(120.0, 120.0)
    .visible(false)
    .focused(false)
    .focusable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    // 文件拖放交给前端 HTML5：WebView2 渲染窗口属于独立子进程，原生 WM_DROPFILES 无法在主进程拦截
    // （跨进程不能子类化窗口过程，实测拖放命中的是 Chrome_RenderWidgetHostHWND 等子进程窗口）；
    // 改由网页层监听 drop 读取文件内容，再经 Tauri command 上传到 web-console。
    .disable_drag_drop_handler()
    .build()?;

    let app_for_close = app.clone();
    let pet_for_events = pet_window.clone();
    let app_for_drop = app.clone();
    pet_window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            quit_application(&app_for_close);
        }
        WindowEvent::Moved(_) | WindowEvent::Focused(_) => {
            repair_pet_window_chrome(&pet_for_events);
        }
        // 文件拖到桌宠图标：收集路径，转交 web-console 移入 workspace 附件目录并加进 composer 附件。
        WindowEvent::DragDrop(drag) => match drag {
            // 拖入悬停时给可见反馈，便于确认后端确实收到了拖放事件。
            tauri::DragDropEvent::Enter { .. } | tauri::DragDropEvent::Over { .. } => {
                emit_pet_status(&app_for_drop, "attention", "松手把文件放给我");
            }
            tauri::DragDropEvent::Drop { paths, .. } => {
                let collected: Vec<String> = paths
                    .iter()
                    .filter_map(|p| p.to_str().map(str::to_string))
                    .collect();
                if collected.is_empty() {
                    emit_pet_status(&app_for_drop, "warning", "没收到文件路径");
                } else {
                    handle_pet_file_drop(app_for_drop.clone(), collected);
                }
            }
            tauri::DragDropEvent::Leave => {
                emit_pet_status(&app_for_drop, "idle", "");
            }
            _ => {}
        },
        _ => {}
    });

    strip_pet_window_chrome(&pet_window);
    normalize_pet_window_bounds(&pet_window);
    stabilize_pet_window(&pet_window);

    Ok(())
}

fn show_console(app: &AppHandle) {
    if app.get_webview_window(CONSOLE_LABEL).is_none() {
        if let Err(error) = build_console_window(app) {
            diagnostics::error_event(
                DIAGNOSTICS_MODULE,
                "console_window_build_failed",
                "Failed to build console window",
                error.as_ref(),
                &[
                    ("phase", "runtime".to_string()),
                    ("component", "console-window".to_string()),
                ],
            );
        }
    }

    if let Some(console) = app.get_webview_window(CONSOLE_LABEL) {
        present_console(&console);
    }
    emit_pet_status(app, "success", "控制台已显示");
}

fn hide_console(app: &AppHandle) {
    if let Some(console) = app.get_webview_window(CONSOLE_LABEL) {
        console.hide().ok();
    }
    emit_pet_status(app, "idle", "控制台已隐藏");
}

fn show_pet(app: &AppHandle) {
    if app.get_webview_window(PET_LABEL).is_none() {
        if let Err(error) = build_pet_window(app) {
            diagnostics::error_event(
                DIAGNOSTICS_MODULE,
                "pet_window_build_failed",
                "Failed to build pet window",
                error.as_ref(),
                &[
                    ("phase", "runtime".to_string()),
                    ("component", "pet-window".to_string()),
                ],
            );
        }
    }

    if let Some(pet) = app.get_webview_window(PET_LABEL) {
        pet.show().ok();
        stabilize_pet_window(&pet);
    }
}

fn hide_pet(app: &AppHandle) {
    if let Some(pet) = app.get_webview_window(PET_LABEL) {
        pet.hide().ok();
    }
}

fn do_toggle_console(app: &AppHandle) -> ConsoleState {
    if app.get_webview_window(CONSOLE_LABEL).is_none() {
        if let Err(error) = build_console_window(app) {
            diagnostics::error_event(
                DIAGNOSTICS_MODULE,
                "console_window_build_failed",
                "Failed to build console window while toggling",
                error.as_ref(),
                &[
                    ("phase", "runtime".to_string()),
                    ("component", "console-window".to_string()),
                ],
            );
        }
    }

    if let Some(console) = app.get_webview_window(CONSOLE_LABEL) {
        let decision = console_toggle_decision(
            console.is_visible().unwrap_or(false),
            console.is_minimized().unwrap_or(false),
            console.is_focused().unwrap_or(false),
        );
        match decision {
            ConsoleToggleDecision::Hide => {
                console.hide().ok();
                emit_pet_status(app, "idle", "控制台已隐藏");
                ConsoleState {
                    visible: false,
                    message: "控制台已隐藏".to_string(),
                }
            }
            ConsoleToggleDecision::Present => {
                present_console(&console);
                emit_pet_status(app, "success", "控制台已显示");
                ConsoleState {
                    visible: true,
                    message: "控制台已显示".to_string(),
                }
            }
        }
    } else {
        emit_pet_status(app, "warning", "控制台窗口创建失败");
        ConsoleState {
            visible: false,
            message: "控制台窗口创建失败".to_string(),
        }
    }
}

#[tauri::command]
fn toggle_console(app: tauri::AppHandle) -> ConsoleState {
    do_toggle_console(&app)
}

#[tauri::command]
fn show_console_command(app: tauri::AppHandle) -> ConsoleState {
    show_console(&app);
    ConsoleState {
        visible: true,
        message: "控制台已显示".to_string(),
    }
}

#[tauri::command]
fn hide_console_command(app: tauri::AppHandle) {
    hide_console(&app);
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    quit_application(&app);
}

/// 退出应用：先回收由安装器 launcher 拉起的 Web Console，再结束 Tauri 进程。
///
/// Tauri 与 web-console 是两个独立进程；只退出自己会留下 8765 监听和后台 agent。
/// PID 由 launcher 注入，且在 Windows 上先核对进程名，防止陈旧/被复用的 PID
/// 指向无关进程后被 taskkill 误伤。
///
/// `app.exit(0)` 只是把 `RequestExit` 投递给事件循环，实测在窗口事件回调里调用时
/// 事件循环并不会因此退出：点标题栏关闭后 web-console 已被回收，但 Tauri 进程和
/// 控制台窗口都还在，用户看到的就是"点了关闭却没退出"。这里再显式结束进程兜底，
/// 让标题栏关闭 / 托盘退出 / 桌宠关闭三条路径都拿到确定的退出结果。
fn quit_application(app: &tauri::AppHandle) {
    terminate_owned_web_console_process();
    app.exit(0);
    std::process::exit(0);
}

fn owned_web_console_pid(args: &[String], current_pid: u32) -> Option<u32> {
    web_console_parent_pid(args).filter(|pid| *pid != current_pid)
}

fn terminate_owned_web_console_process() {
    let args: Vec<String> = std::env::args().collect();
    let Some(pid) = owned_web_console_pid(&args, std::process::id()) else {
        return;
    };

    #[cfg(windows)]
    {
        if !windows_process_name_matches(pid, WEB_CONSOLE_PROCESS_NAME) {
            return;
        }
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        if !matches!(status, Ok(exit) if exit.success()) {
            diagnostics::error_event(
                DIAGNOSTICS_MODULE,
                "web_console_shutdown_failed",
                "Failed to terminate the launcher-owned Web Console",
                &std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("taskkill failed for pid {pid}"),
                ),
                &[("pid", pid.to_string())],
            );
        }
    }

    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
}

#[cfg(windows)]
fn windows_process_name_matches(pid: u32, expected_name: &str) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    let Ok(output) = output else {
        return false;
    };
    let expected = format!("\"{}\"", expected_name.to_ascii_lowercase());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.to_ascii_lowercase().starts_with(&expected))
}

#[tauri::command]
fn start_pet_dragging(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(pet_window) = app.get_webview_window(PET_LABEL) {
        stabilize_pet_window(&pet_window);
        emit_pet_status(&app, "dragging", "移动中");
        watch_native_drag_release_and_restore_idle(app.clone());
        start_native_pet_drag(&pet_window)?;
        stabilize_pet_window(&pet_window);
        Ok(())
    } else {
        Err("Pet window not found".to_string())
    }
}

#[tauri::command]
fn report_throne_zone(
    app: tauri::AppHandle,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    device_pixel_ratio: f64,
) -> Result<(), String> {
    let console = app
        .get_webview_window(CONSOLE_LABEL)
        .ok_or_else(|| "Console window not found".to_string())?;
    let origin = console
        .inner_position()
        .map_err(|error| error.to_string())?;
    let scale = device_pixel_ratio.max(0.5);
    let zone = ThroneZone {
        left: origin.x + (left * scale).round() as i32,
        top: origin.y + (top * scale).round() as i32,
        width: (width.max(1.0) * scale).round() as u32,
        height: (height.max(1.0) * scale).round() as u32,
    };
    let mut guard = THRONE_ZONE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "Throne zone lock poisoned".to_string())?;
    *guard = Some(zone);
    Ok(())
}

fn throne_drop_contains(zone: ThroneZone, x: i32, y: i32) -> bool {
    let right = zone.left.saturating_add(zone.width as i32);
    let bottom = zone.top.saturating_add(zone.height as i32);
    x >= zone.left && x <= right && y >= zone.top && y <= bottom
}

fn throne_snap_position(zone: ThroneZone, pet_width: u32, pet_height: u32) -> (i32, i32) {
    let center_x = zone.left + zone.width as i32 / 2;
    let seat_y = zone.top + (zone.height as f32 * 0.4).round() as i32;
    (
        center_x - pet_width as i32 / 2,
        seat_y - pet_height as i32 / 2,
    )
}

fn handle_pet_drag_release(app: &tauri::AppHandle) {
    let zone = THRONE_ZONE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| *guard);
    let Some(pet) = app.get_webview_window(PET_LABEL) else {
        return;
    };
    let (Ok(position), Ok(size), Some(zone)) = (pet.outer_position(), pet.outer_size(), zone)
    else {
        emit_pet_status(app, "idle", "拖拽已结束");
        return;
    };
    let center_x = position.x + size.width as i32 / 2;
    let center_y = position.y + size.height as i32 / 2;
    if throne_drop_contains(zone, center_x, center_y) {
        let (x, y) = throne_snap_position(zone, size.width, size.height);
        let _ = pet.set_position(PhysicalPosition::new(x, y));
        PET_CROWNED.store(true, std::sync::atomic::Ordering::SeqCst);
        emit_pet_status(app, "crowned", "已登上王座");
        if let Some(console) = app.get_webview_window(CONSOLE_LABEL) {
            let _ = console.emit(
                "pet-throne",
                PetThroneEvent {
                    phase: "seated",
                    x,
                    y,
                },
            );
        }
    } else {
        let was_crowned = PET_CROWNED.swap(false, std::sync::atomic::Ordering::SeqCst);
        emit_pet_status(app, "idle", "拖拽已结束");
        if was_crowned {
            if let Some(console) = app.get_webview_window(CONSOLE_LABEL) {
                let _ = console.emit(
                    "pet-throne",
                    PetThroneEvent {
                        phase: "left",
                        x: position.x,
                        y: position.y,
                    },
                );
            }
        }
    }
}

fn browser_runtime_state() -> &'static Mutex<BrowserRuntimeState> {
    BROWSER_RUNTIME_STATE.get_or_init(|| Mutex::new(BrowserRuntimeState::default()))
}

fn browser_state_snapshot() -> BrowserWindowState {
    browser_runtime_state()
        .lock()
        .expect("browser runtime state lock")
        .response
        .clone()
}

fn update_browser_history_after_navigate(url: &str) -> BrowserWindowState {
    let mut guard = browser_runtime_state()
        .lock()
        .expect("browser runtime state lock");
    let next_index = guard.history_index.map(|index| index + 1).unwrap_or(0);
    if next_index < guard.history.len() {
        guard.history.truncate(next_index);
    }
    guard.history.push(url.to_string());
    guard.history_index = Some(guard.history.len().saturating_sub(1));
    let can_go_back = guard.history_index.unwrap_or(0) > 0;
    let can_go_forward = guard
        .history_index
        .map(|index| index + 1 < guard.history.len())
        .unwrap_or(false);
    guard.response = BrowserWindowState {
        url: Some(url.to_string()),
        title: Some("COOLZHU 浏览器".to_string()),
        loading: false,
        can_go_back,
        can_go_forward,
        last_error: None,
    };
    guard.response.clone()
}

fn update_browser_history_by_delta(delta: isize) -> BrowserWindowState {
    let mut guard = browser_runtime_state()
        .lock()
        .expect("browser runtime state lock");
    if let Some(index) = guard.history_index {
        let candidate = if delta.is_negative() {
            index.saturating_sub(delta.unsigned_abs())
        } else {
            index.saturating_add(delta as usize)
        };
        if candidate < guard.history.len() {
            guard.history_index = Some(candidate);
            guard.response.url = guard.history.get(candidate).cloned();
        }
    }
    let index = guard.history_index.unwrap_or(0);
    guard.response.can_go_back = index > 0;
    guard.response.can_go_forward = index + 1 < guard.history.len();
    guard.response.loading = false;
    guard.response.last_error = None;
    guard.response.clone()
}

fn update_browser_last_error(error: String) -> BrowserWindowState {
    let mut guard = browser_runtime_state()
        .lock()
        .expect("browser runtime state lock");
    guard.response.loading = false;
    guard.response.last_error = Some(error);
    guard.response.clone()
}

fn clear_browser_window_state() -> BrowserWindowState {
    let mut guard = browser_runtime_state()
        .lock()
        .expect("browser runtime state lock");
    guard.response.loading = false;
    guard.response.last_error = None;
    guard.response.clone()
}

fn present_external_browser_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    window.unminimize().map_err(|error| {
        let message = format!("恢复浏览器窗口失败：{error}");
        update_browser_last_error(message.clone());
        message
    })?;
    window.show().map_err(|error| {
        let message = format!("显示浏览器窗口失败：{error}");
        update_browser_last_error(message.clone());
        message
    })?;
    window.set_focus().map_err(|error| {
        let message = format!("聚焦浏览器窗口失败：{error}");
        update_browser_last_error(message.clone());
        message
    })?;
    Ok(())
}

/// 在独立 WebView 窗口（Edge 内核、顶级导航）打开 URL：用于抖音/B站等禁止 iframe 内嵌的多媒体站点。
/// 顶级页面不受 X-Frame-Options/CSP frame-ancestors 限制，故视频能正常播放。复用同一窗口，重复打开则导航换址。
/// 线程模型（多次踩坑总结）：必须 async 命令 + 直接 build()。async 命令在独立线程，build() 把窗口创建
/// 请求交给主线程 event loop 正常处理（命令异步等待结果）——既不阻塞主 event loop（桌宠/窗口不卡死），
/// 又让 WebView2 在主线程创建（不闪退）。绝不能用 run_on_main_thread 包 build：闭包在主线程同步等
/// WebView2 消息泵，而消息泵正被该闭包占用 → 死锁卡死。代理由 main() 在 app 级 environment 统一处理。
#[tauri::command]
async fn browser_window_command(
    app: tauri::AppHandle,
    command: BrowserCommand,
) -> Result<BrowserWindowState, String> {
    match command {
        BrowserCommand::Navigate { url } => {
            let trimmed = url.trim();
            let lower = trimmed.to_ascii_lowercase();
            if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                let error = "仅支持 http(s) URL".to_string();
                update_browser_last_error(error.clone());
                return Err(error);
            }
            let parsed: Url = trimmed.parse().map_err(|e| {
                let error = format!("URL 解析失败：{e}");
                update_browser_last_error(error.clone());
                error
            })?;
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                existing.navigate(parsed).map_err(|e| {
                    let error = format!("导航失败：{e}");
                    update_browser_last_error(error.clone());
                    error
                })?;
                present_external_browser_window(&existing)?;
                return Ok(update_browser_history_after_navigate(trimmed));
            }
            let window = WebviewWindowBuilder::new(
                &app,
                EXTERNAL_BROWSER_LABEL,
                tauri::WebviewUrl::External(parsed),
            )
            .title("COOLZHU 浏览器")
            .inner_size(1200.0, 820.0)
            .min_inner_size(720.0, 480.0)
            .center()
            .prevent_overflow_with_margin(tauri::LogicalSize::new(16.0, 16.0))
            .focused(true)
            .build()
            .map_err(|e| {
                let error = format!("创建浏览器窗口失败：{e}");
                update_browser_last_error(error.clone());
                error
            })?;
            present_external_browser_window(&window)?;
            Ok(update_browser_history_after_navigate(trimmed))
        }
        BrowserCommand::Back => {
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                existing.eval("history.back()").map_err(|e| {
                    let error = format!("后退失败：{e}");
                    update_browser_last_error(error.clone());
                    error
                })?;
                return Ok(update_browser_history_by_delta(-1));
            }
            Ok(browser_state_snapshot())
        }
        BrowserCommand::Forward => {
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                existing.eval("history.forward()").map_err(|e| {
                    let error = format!("前进失败：{e}");
                    update_browser_last_error(error.clone());
                    error
                })?;
                return Ok(update_browser_history_by_delta(1));
            }
            Ok(browser_state_snapshot())
        }
        BrowserCommand::Reload => {
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                existing.eval("location.reload()").map_err(|e| {
                    let error = format!("刷新失败：{e}");
                    update_browser_last_error(error.clone());
                    error
                })?;
            }
            Ok(clear_browser_window_state())
        }
        BrowserCommand::Stop => {
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                existing.eval("window.stop()").map_err(|e| {
                    let error = format!("停止失败：{e}");
                    update_browser_last_error(error.clone());
                    error
                })?;
            }
            Ok(clear_browser_window_state())
        }
        BrowserCommand::Focus => {
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                present_external_browser_window(&existing)?;
            }
            Ok(browser_state_snapshot())
        }
        BrowserCommand::Close => {
            if let Some(existing) = app.get_webview_window(EXTERNAL_BROWSER_LABEL) {
                let _ = existing.close();
            }
            Ok(clear_browser_window_state())
        }
    }
}

#[tauri::command]
async fn open_browser_window(
    app: tauri::AppHandle,
    url: String,
    proxy: Option<String>,
) -> Result<(), String> {
    let _ = proxy;
    browser_window_command(app, BrowserCommand::Navigate { url }).await?;
    Ok(())
}

/// 处理拖到桌宠的文件：先播"搬运"动画，再异步把文件路径 POST 给 web-console
/// （由 web-console 移入 workspace 附件目录并加进 composer 附件），最后按结果播提示动画。
fn handle_pet_file_drop(app: tauri::AppHandle, paths: Vec<String>) {
    emit_pet_status(&app, "carrying", "正在接收拖入的文件…");
    let url = format!("{}/api/pet/drop-files", gui_web_url());
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let result = client
            .post(&url)
            .json(&serde_json::json!({ "paths": paths }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => {
                let moved = resp
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v.get("moved").and_then(|m| m.as_array()).map(|a| a.len()))
                    .unwrap_or(0);
                if moved > 0 {
                    emit_pet_status(
                        &app,
                        "success",
                        &format!("已把 {moved} 个文件放进工作区附件"),
                    );
                } else {
                    emit_pet_status(&app, "warning", "没有可接收的文件");
                }
            }
            Ok(resp) => {
                emit_pet_status(
                    &app,
                    "warning",
                    &format!("接收文件失败：HTTP {}", resp.status()),
                );
            }
            Err(error) => {
                emit_pet_status(&app, "warning", &format!("接收文件失败：{error}"));
            }
        }
    });
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PetUploadFile {
    name: String,
    #[serde(default)]
    mime_type: Option<String>,
    content_base64: String,
}

/// 桌宠 HTML5 拖放上传 command：前端读出拖入文件内容（base64）后调用，
/// 这里经 reqwest 转发给 web-console 的 /api/pet/drop-upload（rust 侧无浏览器跨域问题），
/// 并按结果播放 carrying/success/warning 动画。返回成功登记的文件数。
#[tauri::command]
async fn pet_drop_uploaded(
    app: tauri::AppHandle,
    files: Vec<PetUploadFile>,
) -> Result<usize, String> {
    if files.is_empty() {
        return Ok(0);
    }
    emit_pet_status(&app, "carrying", "正在接收拖入的文件…");
    let url = format!("{}/api/pet/drop-upload", gui_web_url());
    let client = reqwest::Client::new();
    let result = client
        .post(&url)
        .json(&serde_json::json!({ "files": files }))
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await;
    match result {
        Ok(resp) if resp.status().is_success() => {
            let moved = resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("moved").and_then(|m| m.as_array()).map(|a| a.len()))
                .unwrap_or(0);
            if moved > 0 {
                emit_pet_status(
                    &app,
                    "success",
                    &format!("已把 {moved} 个文件放进工作区附件"),
                );
            } else {
                emit_pet_status(&app, "warning", "没有可接收的文件");
            }
            Ok(moved)
        }
        Ok(resp) => {
            let status = resp.status();
            emit_pet_status(&app, "warning", &format!("接收文件失败：HTTP {status}"));
            Err(format!("HTTP {status}"))
        }
        Err(error) => {
            emit_pet_status(&app, "warning", &format!("接收文件失败：{error}"));
            Err(error.to_string())
        }
    }
}

fn watch_native_drag_release_and_restore_idle(app: tauri::AppHandle) {
    let was_active =
        PET_NATIVE_DRAG_RELEASE_WATCHER_ACTIVE.swap(true, std::sync::atomic::Ordering::SeqCst);
    if was_active {
        return;
    }

    std::thread::spawn(move || {
        wait_for_native_left_button_release();
        PET_NATIVE_DRAG_RELEASE_WATCHER_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        handle_pet_drag_release(&app);
    });
}

#[cfg(windows)]
fn wait_for_native_left_button_release() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    let mut saw_pressed = false;
    while std::time::Instant::now() < deadline {
        let pressed = (unsafe { GetAsyncKeyState(VK_LBUTTON as i32) }) < 0;
        if pressed {
            saw_pressed = true;
        } else if saw_pressed {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[cfg(not(windows))]
fn wait_for_native_left_button_release() {
    std::thread::sleep(std::time::Duration::from_millis(900));
}

#[tauri::command]
fn stabilize_pet_window_command(app: tauri::AppHandle) {
    if let Some(pet_window) = app.get_webview_window(PET_LABEL) {
        stabilize_pet_window(&pet_window);
    }
}

#[tauri::command]
fn pet_status() -> PetStatus {
    pet_status_for_state("idle", None)
}

#[tauri::command]
fn set_pet_action(app: tauri::AppHandle, state: String, message: Option<String>) -> PetStatus {
    let status = pet_status_for_state(&state, message.as_deref());
    emit_pet_status_payload(&app, &status);
    status
}

fn emit_pet_status(app: &AppHandle, state: &str, message: &str) {
    let status = pet_status_for_state(state, Some(message));
    emit_pet_status_payload(app, &status);
}

fn emit_pet_status_payload(app: &AppHandle, status: &PetStatus) {
    if let Some(pet) = app.get_webview_window(PET_LABEL) {
        pet.emit("pet-status", status).ok();
    }
}

fn navigate_console_to_current_web_gui(console: &tauri::WebviewWindow) {
    let url = format!("{}/", gui_web_url().trim_end_matches('/'));
    if let Ok(url) = Url::parse(&url) {
        console.navigate(url).ok();
    }
}

fn present_console(console: &tauri::WebviewWindow) {
    navigate_console_to_current_web_gui(console);
    console.unminimize().ok();
    console.show().ok();
    console.set_focus().ok();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsoleToggleDecision {
    Present,
    Hide,
}

fn console_toggle_decision(
    is_visible: bool,
    is_minimized: bool,
    _is_focused: bool,
) -> ConsoleToggleDecision {
    if is_visible && !is_minimized {
        ConsoleToggleDecision::Hide
    } else {
        ConsoleToggleDecision::Present
    }
}

fn gui_web_url() -> String {
    let value = std::fs::read_to_string(std::env::temp_dir().join("coolzhu-gui-web-url.txt"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("COOLZHU_GUI_WEB_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_GUI_WEB_URL.to_string());
    normalize_gui_web_url(&value)
}

fn normalize_gui_web_url(value: &str) -> String {
    let value = value.trim().trim_end_matches('/');
    if let Some(rest) = value.strip_prefix("http://0.0.0.0") {
        return format!("http://127.0.0.1{rest}");
    }
    if let Some(rest) = value.strip_prefix("http://[::]") {
        return format!("http://[::1]{rest}");
    }
    value.to_string()
}

fn pet_theme() -> &'static PetTheme {
    static THEME: OnceLock<PetTheme> = OnceLock::new();
    THEME.get_or_init(|| {
        serde_json::from_str(PET_THEME_JSON)
            .expect("ui/assets/pet-theme.json must be valid desktop pet theme JSON")
    })
}

fn pet_theme_state(state: &str) -> (&'static str, &'static PetThemeState) {
    let theme = pet_theme();
    let normalized = state.trim().to_ascii_lowercase();
    theme
        .states
        .get_key_value(&normalized)
        .or_else(|| theme.states.get_key_value(&theme.default_state))
        .or_else(|| theme.states.iter().next())
        .map(|(key, value)| (key.as_str(), value))
        .expect("pet theme must contain at least one state")
}

fn normalize_pet_state(state: &str) -> String {
    pet_theme_state(state).0.to_string()
}

fn default_pet_state_message(state: &str) -> String {
    pet_theme_state(state).1.message.clone()
}

fn pet_status_for_state(state: &str, message: Option<&str>) -> PetStatus {
    let (normalized, config) = pet_theme_state(state);
    let message = message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&config.message)
        .to_string();
    PetStatus {
        state: normalized.to_string(),
        message,
        console_url: gui_web_url(),
        bubble: config.bubble.clone(),
        frames: pet_action_frames(normalized),
        frame_offsets: pet_action_frame_offsets(normalized),
        frame_scales: pet_action_frame_scales(normalized),
        priority: config.priority,
        interval_ms: config.interval_ms,
        min_duration_ms: config.min_duration_ms,
        auto_return_ms: config.auto_return_ms,
    }
}

fn pet_action_frames(state: &str) -> Vec<String> {
    let (normalized, config) = pet_theme_state(state);
    let frame_count = config.frame_count.max(1);
    let asset_version = pet_theme().asset_version.trim();
    (0..frame_count)
        .map(|index| {
            let path = config
                .frame_pattern
                .replace("{state}", normalized)
                .replace("{index}", &index.to_string());
            if asset_version.is_empty() {
                path
            } else {
                format!("{path}?v={asset_version}")
            }
        })
        .collect()
}

fn pet_action_frame_offsets(state: &str) -> Vec<f32> {
    let (_, config) = pet_theme_state(state);
    let frames = pet_action_frames(state);
    if config.frame_offsets.len() == frames.len() {
        config.frame_offsets.clone()
    } else {
        frames.iter().map(|_| 0.0).collect()
    }
}

fn pet_action_frame_scales(state: &str) -> Vec<f32> {
    let (_, config) = pet_theme_state(state);
    let frames = pet_action_frames(state);
    if config.frame_scales.len() == frames.len() {
        config
            .frame_scales
            .iter()
            .map(|scale| {
                if scale.is_finite() && *scale > 0.0 {
                    *scale
                } else {
                    1.0
                }
            })
            .collect()
    } else {
        frames.iter().map(|_| 1.0).collect()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn pet_bubble_asset(state: &str) -> Option<&'static str> {
    pet_theme_state(state).1.bubble.as_deref()
}

#[cfg_attr(not(test), allow(dead_code))]
fn pet_status_for_event(event_type: &str, message: Option<&str>) -> PetStatus {
    let normalized_event = event_type.trim().to_ascii_lowercase();
    for mapping in &pet_theme().event_map {
        if pet_event_mapping_matches(mapping, &normalized_event) {
            let mapped_message = message
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&mapping.message);
            return pet_status_for_state(&mapping.state, Some(mapped_message));
        }
    }
    pet_status_for_state(&pet_theme().default_state, message)
}

fn pet_event_mapping_matches(mapping: &PetEventMapping, event_type: &str) -> bool {
    match mapping.match_kind.as_str() {
        "contains" => event_type.contains(&mapping.value),
        "equals" => event_type == mapping.value,
        _ => event_type.starts_with(&mapping.value),
    }
}

fn handle_pet_action_args(app: &AppHandle, args: &[String]) -> bool {
    if let Some((state, message)) = pet_action_from_args(args) {
        if startup_visibility(args).show_pet {
            show_pet(app);
        }
        emit_pet_status(app, &state, &message);
        true
    } else {
        false
    }
}

fn spawn_web_console_parent_monitor_if_requested(app: &AppHandle, args: &[String]) {
    let Some(pid) = web_console_parent_pid(args) else {
        return;
    };
    if WEB_CONSOLE_PARENT_MONITOR_STARTED
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if !process_exists(pid) {
            app.exit(0);
            break;
        }
    });
}

fn web_console_parent_pid(args: &[String]) -> Option<u32> {
    web_console_parent_pid_from_args_or_env(args, std::env::var(WEB_CONSOLE_PID_ENV).ok())
}

fn web_console_parent_pid_from_args_or_env(
    args: &[String],
    env_value: Option<String>,
) -> Option<u32> {
    cli_arg_value(args, WEB_CONSOLE_PID_ARG)
        .or(env_value)
        .and_then(|value| parse_parent_pid(&value))
}

fn parse_parent_pid(value: &str) -> Option<u32> {
    let pid = value.trim().parse::<u32>().ok()?;
    (pid > 0).then_some(pid)
}

fn pet_action_from_args(args: &[String]) -> Option<(String, String)> {
    let state = cli_arg_value(args, "--pet-state")?;
    let normalized = normalize_pet_state(&state);
    let message = cli_arg_value(args, "--pet-message")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_pet_state_message(&normalized));
    Some((normalized, message))
}

fn cli_arg_value(args: &[String], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    for (index, arg) in args.iter().enumerate() {
        if let Some(value) = arg.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
        if arg == name {
            return args.get(index + 1).cloned();
        }
    }
    None
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    let Ok(output) = output else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
}

#[cfg(not(windows))]
fn process_exists(pid: u32) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("kill -0 {pid}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn strip_pet_window_chrome<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
        WS_BORDER, WS_CAPTION, WS_DLGFRAME, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME,
        WS_EX_NOACTIVATE, WS_EX_STATICEDGE, WS_EX_TOOLWINDOW, WS_EX_WINDOWEDGE, WS_THICKFRAME,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;

    // Tauri can leave host HWND chrome bits on Windows; strip them after creation.
    unsafe {
        let chrome_mask = (WS_CAPTION | WS_BORDER | WS_DLGFRAME | WS_THICKFRAME) as isize;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !chrome_mask);

        let edge_mask =
            (WS_EX_DLGMODALFRAME | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE | WS_EX_WINDOWEDGE) as isize;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let desired_ex_style = (ex_style & !edge_mask & !(WS_EX_APPWINDOW as isize))
            | (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired_ex_style);

        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOZORDER
                | SWP_NOOWNERZORDER
                | SWP_NOACTIVATE
                | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(not(windows))]
fn strip_pet_window_chrome<R: tauri::Runtime>(_window: &tauri::WebviewWindow<R>) {}

// 原生文件拖放方案已废弃：桌宠是 WebView2，文件拖放命中的渲染窗口（Chrome_RenderWidgetHostHWND /
// Intermediate D3D Window）属于独立子进程，主进程无法子类化其窗口过程拦截 WM_DROPFILES（Windows
// 跨进程限制）。改用前端 HTML5 拖放：网页层监听 drop 读取文件内容，经 pet_drop_uploaded command
// 上传到 web-console（见 build_pet_window 的 disable_drag_drop_handler 与 pet-mini.html 的 drop 监听）。

fn start_native_pet_drag<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[cfg(windows)]
fn hide_visible_helper_windows<R: tauri::Runtime>(pet_window: &tauri::WebviewWindow<R>) {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, RECT},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, ShowWindow,
            SW_HIDE,
        },
    };

    struct CleanupContext {
        process_id: u32,
        pet_hwnd: HWND,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        let context = &mut *(lparam as *mut CleanupContext);
        if hwnd == context.pet_hwnd || IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id != context.process_id {
            return 1;
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let looks_like_leaked_chrome =
            (width <= 360 && height <= 120) || (width <= 120 && height <= 360);
        if looks_like_leaked_chrome {
            ShowWindow(hwnd, SW_HIDE);
        }

        1
    }

    let Ok(hwnd) = pet_window.hwnd() else {
        return;
    };

    let mut context = CleanupContext {
        process_id: std::process::id(),
        pet_hwnd: hwnd.0 as HWND,
    };

    unsafe {
        EnumWindows(
            Some(enum_window),
            &mut context as *mut CleanupContext as LPARAM,
        );
    }
}

#[cfg(not(windows))]
fn hide_visible_helper_windows<R: tauri::Runtime>(_pet_window: &tauri::WebviewWindow<R>) {}

fn stabilize_pet_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    window.set_focusable(false).ok();
    strip_pet_window_chrome(window);
    normalize_pet_window_bounds(window);
    hide_visible_helper_windows(window);
}

fn repair_pet_window_chrome<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    window.set_focusable(false).ok();
    strip_pet_window_chrome(window);
    hide_visible_helper_windows(window);
}

fn normalize_pet_window_bounds<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let size = tauri::Size::Logical(tauri::LogicalSize {
        width: PET_WINDOW_SIZE,
        height: PET_WINDOW_SIZE,
    });
    window.set_size(size).ok();
    window.set_min_size(Some(size)).ok();
    window.set_max_size(Some(size)).ok();
}

#[derive(Debug, Clone, Serialize)]
struct ConsoleState {
    visible: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct PetStatus {
    state: String,
    message: String,
    console_url: String,
    bubble: Option<String>,
    frames: Vec<String>,
    frame_offsets: Vec<f32>,
    frame_scales: Vec<f32>,
    priority: i32,
    interval_ms: u64,
    min_duration_ms: u64,
    auto_return_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::{
        console_toggle_decision, default_pet_state_message, normalize_gui_web_url,
        normalize_pet_state, owned_web_console_pid, pet_action_frame_offsets, pet_action_frames,
        pet_action_from_args, pet_bubble_asset, pet_status_for_event, startup_visibility,
        throne_drop_contains, throne_snap_position, web_console_parent_pid_from_args_or_env,
        ConsoleToggleDecision, ThroneZone, PET_THEME_JSON, PET_WINDOW_SIZE,
    };
    use std::{collections::VecDeque, fs::File, path::PathBuf};

    const MAIN_RS: &str = include_str!("main.rs");
    const CARGO_TOML: &str = include_str!("../Cargo.toml");
    const PET_MINI_HTML: &str = include_str!("../../ui/pet-mini.html");
    const PET_STABILIZER_PY: &str = include_str!(
        "../../ui/assets/pet-actions/generated-sheets-20260617/stabilize_pet_frames.py"
    );
    const PET_FRAME_CANVAS: u32 = 256;
    const PET_FRAME_RENDER_SCALE: f32 = 0.5;
    const PET_FRAME_DEFAULT_CENTER_DRIFT_TOLERANCE: f32 = 24.0;
    const PET_FRAME_DEFAULT_BASELINE_DRIFT_TOLERANCE: u32 = 28;
    const PET_FRAME_VISUAL_DRIFT_TOLERANCE: f32 = 14.0;
    const PET_FRAME_EDGE_CUT_RATIO_TOLERANCE: f32 = 0.35;

    fn normalized_main_source() -> String {
        MAIN_RS.replace("\r\n", "\n").replace('\r', "\n")
    }

    fn normalized_pet_stabilizer_source() -> String {
        PET_STABILIZER_PY.replace("\r\n", "\n").replace('\r', "\n")
    }

    fn bounded_source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start_index = source
            .find(start)
            .unwrap_or_else(|| panic!("未找到源码契约起点: {start}"));
        let section_start = start_index + start.len();
        let end_index = source[section_start..]
            .find(end)
            .unwrap_or_else(|| panic!("未找到源码契约终点: {end}"));
        &source[section_start..section_start + end_index]
    }

    fn assert_stabilizer_source_line(section: &str, expected: &str, description: &str) {
        assert!(
            section.lines().any(|line| line.trim() == expected),
            "{description}源码契约缺少精确语句: {expected}"
        );
    }

    fn assert_pet_stabilizer_policy_contract() {
        let source = normalized_pet_stabilizer_source();
        let policies = bounded_source_section(
            &source,
            "STATE_POLICIES = {",
            "\n}\n\nSTATE_FRAME_SELECTIONS",
        );
        for (state, expected) in [
            (
                "working",
                "\"working\": {\"mode\": \"uniform_fit\", \"anchor\": \"baseline\"},",
            ),
            (
                "sweeping",
                "\"sweeping\": {\"mode\": \"uniform_fit\", \"anchor\": \"baseline\"},",
            ),
            (
                "sword-flight",
                "\"sword-flight\": {\"mode\": \"uniform_fit\", \"anchor\": \"center\", \"center_y\": 132},",
            ),
            (
                "sleeping",
                "\"sleeping\": {\"mode\": \"uniform_fit\", \"anchor\": \"center\", \"center_y\": 134},",
            ),
            (
                "success",
                "\"success\": {\"mode\": \"face_scale\", \"anchor\": \"baseline\"},",
            ),
            (
                "perform_martial",
                "\"perform_martial\": {\"mode\": \"face_scale\", \"anchor\": \"baseline\"},",
            ),
        ] {
            assert_stabilizer_source_line(
                policies,
                expected,
                &format!("{state} 的状态策略"),
            );
        }

        let stabilize =
            bounded_source_section(&source, "def stabilize() -> None:\n", "\n\nif __name__");
        let uniform_fit = bounded_source_section(
            stabilize,
            "        if policy[\"mode\"] == \"uniform_fit\":\n",
            "        elif policy[\"mode\"] == \"face_scale\":\n",
        );
        assert_stabilizer_source_line(
            uniform_fit,
            "scales = [uniform_fit_scale(frames, state)] * len(frames)",
            "uniform_fit 分支的共享 scale",
        );

        let face_scale = bounded_source_section(
            stabilize,
            "        elif policy[\"mode\"] == \"face_scale\":\n",
            "        elif policy[\"mode\"] == \"height_crop\":\n",
        );
        assert_stabilizer_source_line(
            face_scale,
            "scales = [per_frame_face_scale(frame) for frame in frames]",
            "face_scale 分支",
        );
    }

    #[test]
    fn launcher_startup_visibility_respects_console_and_pet_intent() {
        let default = startup_visibility(&["coolzhu-tauri-shell.exe".to_string()]);
        assert!(default.show_console);
        assert!(!default.show_pet);

        let launcher = startup_visibility(&[
            "coolzhu-tauri-shell.exe".to_string(),
            "--show-console".to_string(),
        ]);
        assert!(launcher.show_console);
        assert!(!launcher.show_pet);

        for pet_flag in [
            "--pet",
            "--show-pet",
            "--pet-action=perform_martial",
            "--pet-event=chat.completed",
        ] {
            let pet_only =
                startup_visibility(&["coolzhu-tauri-shell.exe".to_string(), pet_flag.to_string()]);
            assert!(!pet_only.show_console, "{pet_flag} should be pet-only");
            assert!(pet_only.show_pet, "{pet_flag} should show the pet");
        }

        let state_only = startup_visibility(&[
            "coolzhu-tauri-shell.exe".to_string(),
            "--pet-state=sleeping".to_string(),
        ]);
        assert!(!state_only.show_console);
        assert!(!state_only.show_pet);

        let state_with_pet = startup_visibility(&[
            "coolzhu-tauri-shell.exe".to_string(),
            "--pet-state=sleeping".to_string(),
            "--pet".to_string(),
        ]);
        assert!(!state_with_pet.show_console);
        assert!(state_with_pet.show_pet);

        let state_with_console = startup_visibility(&[
            "coolzhu-tauri-shell.exe".to_string(),
            "--pet-state=sleeping".to_string(),
            "--show-console".to_string(),
        ]);
        assert!(state_with_console.show_console);
        assert!(!state_with_console.show_pet);

        let both = startup_visibility(&[
            "coolzhu-tauri-shell.exe".to_string(),
            "--show-console".to_string(),
            "--pet".to_string(),
        ]);
        assert!(both.show_console);
        assert!(both.show_pet);
    }

    #[test]
    fn tray_exposes_safe_pet_hide_action() {
        let production = MAIN_RS
            .split("#[cfg(test)]")
            .next()
            .expect("production source should precede tests");
        assert!(production
            .contains("MenuItem::with_id(app, \"hide-pet\", \"隐藏桌宠\", true, None::<&str>)?"));
        assert!(production.contains("\"hide-pet\" => hide_pet(app)"));

        let action_handler = production
            .split("fn handle_pet_action_args(")
            .nth(1)
            .expect("handle_pet_action_args helper should exist")
            .split("\nfn ")
            .next()
            .expect("handle_pet_action_args helper should have a bounded body");
        assert!(action_handler.contains("if startup_visibility(args).show_pet"));

        let build_pet = production
            .split("fn build_pet_window(")
            .nth(1)
            .expect("build_pet_window helper should exist")
            .split("\nfn ")
            .next()
            .expect("build_pet_window helper should have a bounded body");
        assert!(
            !build_pet.contains("pet_window.show().ok();"),
            "building the pet window must not make it visible without an explicit request"
        );

        let hide_pet = production
            .split("fn hide_pet(")
            .nth(1)
            .expect("hide_pet helper should exist")
            .split("\nfn ")
            .next()
            .expect("hide_pet helper should have a bounded body");
        assert!(hide_pet.contains("pet.hide().ok();"));
        for forbidden in [
            "quit_application",
            "terminate_owned_web_console_process",
            "console.hide()",
        ] {
            assert!(
                !hide_pet.contains(forbidden),
                "hide_pet must not call {forbidden}"
            );
        }
    }

    #[test]
    fn gui_client_url_normalizes_unspecified_listener_hosts() {
        assert_eq!(
            normalize_gui_web_url("http://0.0.0.0:8765/"),
            "http://127.0.0.1:8765"
        );
        assert_eq!(
            normalize_gui_web_url("http://127.0.0.1:8766/"),
            "http://127.0.0.1:8766"
        );
    }

    #[test]
    fn tauri_shell_uses_shared_diagnostics_for_errors() {
        assert!(
            CARGO_TOML.contains(
                "diagnostics = { package = \"coolzhu-diagnostics\", path = \"../../../../diagnostics/packages/diagnostics\" }"
            ),
            "the standalone Tauri workspace should use the shared diagnostics crate by relative path"
        );

        let main_rs = normalized_main_source();
        let main = main_rs.find("fn main()").expect("main should exist");
        let init = main_rs
            .find("diagnostics::init(\"coolzhu-tauri-shell\")")
            .expect("diagnostics should initialize during startup");
        let proxy_probe = main_rs[main..]
            .find("detect_local_proxy_port()")
            .map(|index| main + index)
            .expect("proxy detection should exist");
        assert!(
            main < init && init < proxy_probe,
            "diagnostics should initialize before other startup work"
        );
        assert_eq!(
            main_rs.matches("diagnostics::error_event(\n").count(),
            9,
            "init failure and every Tauri shell error should use diagnostics::error_event"
        );
        for removed_helper in ["log_tauri_shell_error", "escape_json", "unix_millis"] {
            let declaration = format!("fn {removed_helper}(");
            assert!(
                !main_rs.contains(&declaration),
                "{removed_helper} should be removed from the Tauri shell"
            );
        }
    }

    #[test]
    fn external_browser_command_contract_is_explicit_and_safe() {
        let production = MAIN_RS
            .split("#[cfg(test)]")
            .next()
            .expect("production source should precede tests");
        for variant in [
            "Navigate { url: String }",
            "Back",
            "Forward",
            "Reload",
            "Stop",
            "Focus",
            "Close",
        ] {
            assert!(
                production.contains(variant),
                "BrowserCommand should expose {variant}"
            );
        }
        for field in [
            "url: Option<String>",
            "title: Option<String>",
            "loading: bool",
            "can_go_back: bool",
            "can_go_forward: bool",
            "last_error: Option<String>",
        ] {
            assert!(
                production.contains(field),
                "BrowserWindowState should expose {field}"
            );
        }
        assert!(production.contains("async fn browser_window_command"));
        assert!(production.contains("browser_window_command"));
        assert!(production.contains("BrowserCommand::Navigate { url }"));
        assert!(production
            .contains("browser_window_command(app, BrowserCommand::Navigate { url }).await"));
        assert!(production.contains("EXTERNAL_BROWSER_LABEL"));
        assert_eq!(
            production
                .matches("const EXTERNAL_BROWSER_LABEL: &str = \"external-browser\"")
                .count(),
            1,
            "external browser label is part of the no-IPC security boundary"
        );
        assert!(
            !MAIN_RS.contains("\"windows\": [\"console\", \"pet\", \"external-browser\"]"),
            "external browser window must not be added to default IPC capabilities"
        );
    }

    #[test]
    fn pet_mini_window_uses_double_size_wuxia_layout() {
        assert_eq!(PET_WINDOW_SIZE, 152.0);
        assert!(PET_MINI_HTML.contains("width: 152px"));
        assert!(PET_MINI_HTML.contains("height: 152px"));
        assert!(PET_MINI_HTML.contains("width: 128px"));
        assert!(PET_MINI_HTML.contains("height: 128px"));
    }

    #[test]
    fn pet_state_normalization_is_stable() {
        assert_eq!(normalize_pet_state("thinking"), "thinking");
        assert_eq!(normalize_pet_state("unknown"), "idle");
    }

    #[test]
    fn pet_state_frames_cover_core_actions() {
        for state in ["idle", "thinking", "sleeping", "success"] {
            let frames = pet_action_frames(state);
            assert!(!frames.is_empty());
            assert!(frames[0].contains(state));
        }
        assert_eq!(
            pet_action_frames("blink").len(),
            8,
            "blink signal should reuse all eight idle frames for the overlay"
        );
        assert!(PET_THEME_JSON.contains("\"blink\""));
        assert!(
            pet_action_frames("blink")[0].contains("idle-"),
            "blink signal should reuse idle frames instead of a separate animation"
        );
        assert_eq!(pet_action_frames("warning").len(), 7);
    }

    #[test]
    fn pet_frame_urls_are_versioned_to_bypass_webview_cache() {
        for state in ["idle", "success", "perform_martial"] {
            for frame in pet_action_frames(state) {
                assert!(
                    frame.contains("?v="),
                    "{state} frame should carry the pet theme asset version: {frame}"
                );
            }
        }
    }

    #[test]
    fn pet_theme_manifest_declares_migrated_clawd_states() {
        for state in [
            "working",
            "attention",
            "notification",
            "dragging",
            "carrying",
            "juggling",
            "sweeping",
        ] {
            let frames = pet_action_frames(state);
            assert!(!frames.is_empty(), "{state} should have configured frames");
            let expected_token = if state == "dragging" {
                "sword-flight"
            } else {
                state
            };
            assert!(
                frames[0].contains(expected_token),
                "{state} should be data-driven from its own action frames"
            );
        }
    }

    #[test]
    fn pet_theme_declares_wuxia_dragging_and_single_sword_performance_state() {
        let dragging_frames = pet_action_frames("dragging");
        assert_eq!(dragging_frames.len(), 8);
        assert!(
            dragging_frames[0].contains("sword-flight"),
            "dragging should use wuxia sword-flight action frames"
        );

        let martial_frames = pet_action_frames("perform_martial");
        assert_eq!(martial_frames.len(), 8);
        assert!(martial_frames[0].contains("perform_martial"));
        assert!(
            !PET_THEME_JSON.contains("\"perform_dance\""),
            "dancing state should be removed from the wuxia pet theme"
        );
        assert!(
            !PET_THEME_JSON.contains("\"perform_boxing\""),
            "boxing state should be removed from the wuxia pet theme"
        );
    }

    #[test]
    fn pet_event_status_maps_chat_tool_audio_events() {
        let chat = pet_status_for_event("chat.delta", Some("Answering"));
        assert_eq!(chat.state, "perform_martial");
        assert_eq!(chat.message, "Answering");
        assert_eq!(chat.bubble.as_deref(), None);
        assert!(chat.frames[0].contains("perform_martial"));

        let tool = pet_status_for_event("tool.started", None);
        assert_eq!(tool.state, "perform_martial");
        assert_eq!(tool.message, "Tool is running");

        let audio = pet_status_for_event("audio.input", Some("  "));
        assert_eq!(audio.state, "blink");
        assert_eq!(audio.message, "Listening");
        assert_eq!(audio.bubble.as_deref(), None);
    }

    #[test]
    fn pet_event_status_maps_terminal_and_unknown_events() {
        let warning = pet_status_for_event("error", None);
        assert_eq!(warning.state, "warning");
        assert_eq!(
            warning.bubble.as_deref(),
            Some("assets/pet-bubbles/warning.png")
        );
        assert_eq!(warning.frames.len(), 7);

        let success = pet_status_for_event("complete", Some("Done"));
        assert_eq!(success.state, "success");
        assert_eq!(success.message, "Done");

        let unknown = pet_status_for_event("session.idle", None);
        assert_eq!(unknown.state, "idle");
        assert_eq!(unknown.message, default_pet_state_message("idle"));
    }

    #[test]
    fn pet_event_status_maps_goal_and_permission_lifecycle() {
        let goal = pet_status_for_event("goal.phase.running", Some("Planner is working"));
        assert_eq!(goal.state, "working");
        assert_eq!(goal.message, "Planner is working");

        let permission = pet_status_for_event("permission.required", None);
        assert_eq!(permission.state, "attention");
        assert!(permission.message.contains("permission"));

        let notice = pet_status_for_event("notification.agent", Some("New task update"));
        assert_eq!(notice.state, "notification");
        assert_eq!(notice.message, "New task update");
    }

    #[test]
    fn pet_action_args_support_split_and_inline_values() {
        let inline = vec![
            "coolzhu-tauri-shell".to_string(),
            "--pet-state=warning".to_string(),
            "--pet-message=注意测试".to_string(),
        ];
        assert_eq!(
            pet_action_from_args(&inline),
            Some(("warning".to_string(), "注意测试".to_string()))
        );

        let split = vec![
            "coolzhu-tauri-shell".to_string(),
            "--pet-state".to_string(),
            "success".to_string(),
            "--pet-message".to_string(),
            "完成测试".to_string(),
        ];
        assert_eq!(
            pet_action_from_args(&split),
            Some(("success".to_string(), "完成测试".to_string()))
        );
    }

    #[test]
    fn pet_action_args_default_message_and_normalize_state() {
        let args = vec![
            "coolzhu-tauri-shell".to_string(),
            "--pet-state".to_string(),
            "unknown".to_string(),
            "--pet-message".to_string(),
            " ".to_string(),
        ];
        assert_eq!(
            pet_action_from_args(&args),
            Some((
                "idle".to_string(),
                default_pet_state_message("idle").to_string()
            ))
        );
    }

    #[test]
    fn web_console_parent_pid_accepts_args_and_env_fallback() {
        let inline = vec![
            "coolzhu-tauri-shell".to_string(),
            "--web-console-pid=12345".to_string(),
        ];
        assert_eq!(
            web_console_parent_pid_from_args_or_env(&inline, None),
            Some(12345)
        );

        let split = vec![
            "coolzhu-tauri-shell".to_string(),
            "--web-console-pid".to_string(),
            "23456".to_string(),
        ];
        assert_eq!(
            web_console_parent_pid_from_args_or_env(&split, None),
            Some(23456)
        );

        let no_arg = vec!["coolzhu-tauri-shell".to_string(), "--pet".to_string()];
        assert_eq!(
            web_console_parent_pid_from_args_or_env(&no_arg, Some("34567".to_string())),
            Some(34567)
        );
        assert_eq!(
            web_console_parent_pid_from_args_or_env(&no_arg, Some("not-a-pid".to_string())),
            None
        );
    }

    #[test]
    fn pet_exit_only_targets_launcher_owned_web_console_pid() {
        let args = vec![
            "coolzhu-tauri-shell.exe".to_string(),
            "--web-console-pid=12345".to_string(),
        ];
        assert_eq!(owned_web_console_pid(&args, 9999), Some(12345));
        assert_eq!(
            owned_web_console_pid(&args, 12345),
            None,
            "the shell must never taskkill itself when a stale pid equals its own pid"
        );

        let standalone = vec!["coolzhu-tauri-shell.exe".to_string()];
        assert_eq!(owned_web_console_pid(&standalone, 9999), None);
    }

    #[test]
    fn pet_exit_path_reclaims_web_console_before_app_exit() {
        let source = normalized_main_source();
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source should precede tests");
        let quit = production
            .find("fn quit_application(")
            .expect("quit_application should exist");
        let terminate = production[quit..]
            .find("terminate_owned_web_console_process();")
            .map(|offset| quit + offset)
            .expect("quit_application should terminate the owned web console");
        let exit = production[terminate..]
            .find("app.exit(0);")
            .map(|offset| terminate + offset)
            .expect("quit_application should exit the Tauri app");
        assert!(terminate < exit);
        assert!(production.contains("quit_application(&app_for_close)"));
        assert!(production.contains("quit_application(app)"));
        assert!(production.contains("#[tauri::command]\nfn quit_app"));
    }

    /// 标题栏的关闭按钮必须真的退出。历史实现是 `prevent_close` + `hide()`，
    /// 窗口消失但 Tauri 进程、托盘图标和 8765 上的 agent 全部继续驻留，
    /// 用户看到的就是"点了关闭却没退出"。
    #[test]
    fn console_window_close_quits_instead_of_only_hiding() {
        let source = normalized_main_source();
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source should precede tests");
        let build_console = production
            .split("fn build_console_window(")
            .nth(1)
            .expect("build_console_window helper should exist")
            .split("\nfn ")
            .next()
            .expect("build_console_window helper should have a bounded body");

        assert!(
            build_console.contains("api.prevent_close();")
                && build_console.contains("quit_application(&app_for_close);"),
            "closing the console window must reclaim the owned web console and exit the app"
        );
        assert!(
            !build_console.contains(".hide()"),
            "closing the console window must not degrade into hide-only, which leaves the agent running"
        );

        // app.exit 只投递 RequestExit，实测不足以结束进程；退出路径必须有兜底。
        let quit = production
            .find("fn quit_application(")
            .expect("quit_application should exist");
        let request_exit = production[quit..]
            .find("app.exit(0);")
            .map(|offset| quit + offset)
            .expect("quit_application should request a graceful exit");
        let hard_exit = production[quit..]
            .find("std::process::exit(0);")
            .map(|offset| quit + offset)
            .expect("quit_application must guarantee the process actually exits");
        assert!(
            request_exit < hard_exit,
            "the hard exit must be the last step of quit_application"
        );
    }

    #[test]
    fn pet_bubbles_match_status_states() {
        assert_eq!(pet_bubble_asset("idle"), None);
        assert_eq!(pet_bubble_asset("blink"), None);
        assert_eq!(
            pet_bubble_asset("thinking"),
            Some("assets/pet-bubbles/thinking.png")
        );
        assert_eq!(
            pet_bubble_asset("success"),
            Some("assets/pet-bubbles/success.png")
        );
        assert_eq!(pet_bubble_asset("sleeping"), None);
        assert!(default_pet_state_message("warning").contains("注意"));
    }

    #[test]
    fn pet_mini_has_manual_double_click_fallback() {
        assert!(PET_MINI_HTML.contains("handlePetDoubleClick"));
        assert!(PET_MINI_HTML.contains("lastClickAt"));
        assert!(PET_MINI_HTML.contains("mouseup"));
        assert!(PET_MINI_HTML.contains("toggle_console"));
    }

    #[test]
    fn pet_mini_status_events_render_text_bubbles() {
        assert!(PET_MINI_HTML.contains("bubbleText"));
        assert!(PET_MINI_HTML.contains("showBubble"));
        assert!(PET_MINI_HTML.contains("payload.message"));
    }

    #[test]
    fn pet_mini_text_bubble_suppresses_image_bubble() {
        assert!(PET_MINI_HTML.contains("bubbleImage.removeAttribute(\"src\")"));
        assert!(PET_MINI_HTML.contains("bubbleText.textContent = text"));
    }

    #[test]
    fn pet_mini_uses_payload_frames_for_action_state() {
        assert!(PET_MINI_HTML.contains("normalizeFrameList"));
        assert!(PET_MINI_HTML.contains("payload.frames"));
        assert!(PET_MINI_HTML.contains("payload.frame_offsets"));
        assert!(PET_MINI_HTML.contains("payload.frame_scales"));
        assert!(PET_MINI_HTML.contains("transform-origin: center bottom"));
        assert!(PET_MINI_HTML.contains("applyFrame"));
        assert!(PET_MINI_HTML.contains("warning: actionFrames(\"warning\", 7)"));
    }

    #[test]
    fn pet_mini_applies_state_priority_and_auto_return() {
        assert!(PET_MINI_HTML.contains("statePriorities"));
        assert!(PET_MINI_HTML.contains("minDurationMs"));
        assert!(PET_MINI_HTML.contains("autoReturnTimer"));
        assert!(PET_MINI_HTML.contains("pendingStatus"));
    }

    #[test]
    fn pet_mini_triggers_idle_performances_sleep_and_activity_wake() {
        assert!(PET_MINI_HTML.contains("IDLE_PERFORMANCE_MS"));
        assert!(PET_MINI_HTML.contains("IDLE_SLEEP_MS"));
        assert!(PET_MINI_HTML.contains("idlePerformanceStates"));
        assert!(PET_MINI_HTML.contains("recordPetActivity"));
        assert!(PET_MINI_HTML.contains("maybeRunIdleAutonomy"));
        assert!(PET_MINI_HTML.contains("perform_martial"));
        assert!(!PET_MINI_HTML.contains("perform_dance"));
        assert!(!PET_MINI_HTML.contains("perform_boxing"));
        assert!(PET_MINI_HTML.contains("applyStatus({ state: \"sleeping\""));
    }

    #[test]
    fn pet_mini_idle_blink_uses_unified_state_transition() {
        let idle_blink = PET_MINI_HTML
            .find("function scheduleIdleBlink()")
            .expect("idle blink scheduler should exist");
        let idle_autonomy = PET_MINI_HTML
            .find("function maybeRunIdleAutonomy()")
            .expect("idle autonomy helper should follow blink scheduler");
        let scheduler = &PET_MINI_HTML[idle_blink..idle_autonomy];

        assert!(
            scheduler.contains("blinkOverlayUntil = performance.now() + 240;"),
            "automatic idle blink should use the closed-eye overlay window"
        );
        assert!(
            scheduler.contains("applyFrame(frameIndex);"),
            "automatic idle blink should preserve the current idle frame index"
        );
        assert!(
            scheduler.contains("if (state !== \"idle\")"),
            "automatic idle blink should stop when a newer state is active"
        );
        assert!(
            !scheduler.contains("state = \"blink\""),
            "automatic idle blink must not bypass setState by mutating state directly"
        );
        assert!(
            !scheduler.contains("activeFrames = frames.blink"),
            "automatic idle blink must not bypass setState by mutating activeFrames directly"
        );
        assert!(
            !scheduler.contains("frameIndex = 0"),
            "automatic idle blink must not reset the idle frame sequence"
        );
        assert!(
            !scheduler.contains("setState(\"idle\")"),
            "automatic idle blink must not restart the idle state"
        );
    }

    #[test]
    fn pet_mini_previews_dragging_frame_before_native_drag() {
        let drag_state = PET_MINI_HTML
            .find("function beginDragPreview()")
            .expect("dragging preview helper should exist");
        let set_dragging = PET_MINI_HTML
            .find("setState(\"dragging\"")
            .expect("dragging preview should set dragging state");
        let native_drag = PET_MINI_HTML
            .find("await call(\"start_pet_dragging\")")
            .expect("native drag command should still be invoked");

        assert!(
            drag_state < set_dragging && set_dragging < native_drag,
            "dragging frame must be previewed before native drag takes over"
        );
    }

    #[test]
    fn pet_mini_starts_drag_preview_after_drag_threshold_not_mouse_down() {
        let mouse_down = PET_MINI_HTML
            .find("document.body.addEventListener(\"mousedown\"")
            .expect("pet should handle mouse down");
        let mouse_move = PET_MINI_HTML
            .find("document.body.addEventListener(\"mousemove\"")
            .expect("pet should handle mouse move");
        let mouse_down_block = &PET_MINI_HTML[mouse_down..mouse_move];
        assert!(
            !mouse_down_block.contains("beginDragPreview();"),
            "pressing without dragging should not show the flying state"
        );

        let preview = PET_MINI_HTML[mouse_move..]
            .find("beginDragPreview();")
            .map(|index| mouse_move + index)
            .expect("crossing the drag threshold should start a visual drag preview");
        let native_drag = PET_MINI_HTML
            .find("await call(\"start_pet_dragging\")")
            .expect("native drag should still be invoked from move threshold");

        assert!(
            mouse_down < preview && preview < native_drag,
            "drag preview should start after drag threshold before native drag can take over"
        );
    }

    #[test]
    fn pet_mini_blur_does_not_end_preview_during_native_drag() {
        let blur = PET_MINI_HTML
            .find("window.addEventListener(\"blur\", () => {")
            .expect("blur handler should exist");
        let next_blur = PET_MINI_HTML[blur + 1..]
            .find("window.addEventListener(\"blur\"")
            .map(|index| blur + 1 + index)
            .expect("second blur handler should follow drag blur handler");
        let blur_block = &PET_MINI_HTML[blur..next_blur];
        assert!(
            blur_block.contains("const nativeDragInProgress = isNativeDragging"),
            "blur should snapshot native drag state before clearing pointer state"
        );
        assert!(
            blur_block.contains("if (!nativeDragInProgress)"),
            "native drag blur should not clear the dragging preview"
        );
        assert!(
            blur_block.contains("endDragPreview();"),
            "blur should still clear preview when no native drag is active"
        );
    }

    #[test]
    fn pet_mini_does_not_restore_idle_immediately_after_native_drag_starts() {
        let native_drag = PET_MINI_HTML
            .find("await call(\"start_pet_dragging\")")
            .expect("native drag command should be invoked");
        let mouseup = PET_MINI_HTML[native_drag..]
            .find("document.body.addEventListener(\"mouseup\"")
            .map(|index| native_drag + index)
            .expect("mouseup should appear after native drag call");
        let drag_call_block = &PET_MINI_HTML[native_drag..mouseup];

        assert!(
            !drag_call_block.contains("applyStatus({ state: \"idle\""),
            "dragging should not be restored to idle immediately after start_dragging returns"
        );
    }

    #[test]
    fn pet_mini_window_mouseup_finishes_native_drag_preview() {
        let window_mouseup = PET_MINI_HTML
            .find("window.addEventListener(\"mouseup\"")
            .expect("window mouseup should exist");
        let blur = PET_MINI_HTML[window_mouseup..]
            .find("window.addEventListener(\"blur\"")
            .map(|index| window_mouseup + index)
            .expect("blur handler should follow window mouseup");
        let mouseup_block = &PET_MINI_HTML[window_mouseup..blur];

        assert!(
            mouseup_block.contains("finishNativeDrag();"),
            "window mouseup should restore idle after native drag ends"
        );
    }

    #[test]
    fn native_drag_command_emits_dragging_status_before_window_drag() {
        let command = MAIN_RS
            .find("fn start_pet_dragging")
            .expect("drag command should exist");
        let status = MAIN_RS[command..]
            .find("emit_pet_status(&app, \"dragging\"")
            .map(|index| command + index)
            .expect("native drag command should emit dragging status");
        let native_drag = MAIN_RS[command..]
            .find("start_native_pet_drag")
            .map(|index| command + index)
            .expect("native drag command should invoke window drag");

        assert!(
            status < native_drag,
            "dragging status should be emitted before the native drag call"
        );
    }

    #[test]
    fn native_drag_command_schedules_release_idle_watcher() {
        let command = MAIN_RS
            .find("fn start_pet_dragging")
            .expect("drag command should exist");
        let status = MAIN_RS[command..]
            .find("emit_pet_status(&app, \"dragging\"")
            .map(|index| command + index)
            .expect("native drag command should emit dragging status");
        let watcher = MAIN_RS[command..]
            .find("watch_native_drag_release_and_restore_idle")
            .map(|index| command + index)
            .expect("native drag command should schedule a release watcher");
        let native_drag = MAIN_RS[command..]
            .find("start_native_pet_drag")
            .map(|index| command + index)
            .expect("native drag command should invoke window drag");

        assert!(
            status < watcher && watcher < native_drag,
            "release watcher should be armed after entering dragging and before native drag takes over"
        );
    }

    #[test]
    fn native_drag_release_watcher_waits_for_left_button_release_and_restores_idle() {
        assert!(
            MAIN_RS.contains("GetAsyncKeyState"),
            "Windows release watcher should poll the native mouse button state"
        );
        assert!(
            MAIN_RS.contains("VK_LBUTTON"),
            "release watcher should watch the left mouse button specifically"
        );
        assert!(
            MAIN_RS.contains("handle_pet_drag_release(&app)"),
            "release watcher should route through the throne-aware release handler"
        );
    }

    #[test]
    fn throne_drop_bridge_reports_dom_zone_and_handles_drag_release() {
        let zone = ThroneZone {
            left: 100,
            top: 50,
            width: 400,
            height: 200,
        };
        assert!(throne_drop_contains(zone, 300, 150));
        assert!(!throne_drop_contains(zone, 80, 150));
        assert_eq!(throne_snap_position(zone, 76, 76), (262, 92));
        let main_rs = normalized_main_source();
        assert!(main_rs.contains("#[tauri::command]\nfn report_throne_zone"));
        assert!(main_rs.contains("\"pet-throne\""));
    }

    #[test]
    fn pet_theme_and_mini_ui_support_persistent_crowned_state() {
        assert!(PET_THEME_JSON.contains("\"crowned\""));
        assert!(PET_THEME_JSON.contains("assets/pet-actions/crowned-{index}.png"));
        assert!(PET_MINI_HTML.contains("crowned: actionFrames(\"crowned\")"));
        assert!(PET_MINI_HTML.contains("crowned: 5"));
        assert!(PET_MINI_HTML.contains("crowned: 0"));
        assert!(PET_MINI_HTML.contains("let crownedIntroComplete = false"));
        assert!(PET_MINI_HTML.contains("function nextFrameIndex(frameCount)"));
        assert!(PET_MINI_HTML.contains("const settledStart = Math.max(0, frameCount - 2)"));
    }

    #[test]
    fn pet_status_after_native_drag_clears_frontend_drag_latch() {
        assert!(PET_MINI_HTML.contains("if (payload.state !== \"dragging\")"));
        assert!(PET_MINI_HTML.contains("isNativeDragging = false"));
        assert!(PET_MINI_HTML.contains("dragPreviewActive = false"));
    }

    #[test]
    fn console_toggle_presents_until_visible_unminimized_and_focused() {
        assert_eq!(
            console_toggle_decision(false, false, false),
            ConsoleToggleDecision::Present
        );
        assert_eq!(
            console_toggle_decision(true, true, false),
            ConsoleToggleDecision::Present
        );
        assert_eq!(
            console_toggle_decision(true, false, false),
            ConsoleToggleDecision::Hide
        );
        assert_eq!(
            console_toggle_decision(true, false, true),
            ConsoleToggleDecision::Hide
        );
    }

    #[test]
    fn pet_action_frames_share_canvas_and_visual_anchor() {
        for state in [
            "idle", "blink", "thinking", "sleeping", "warning", "success", "crowned",
        ] {
            let frames = pet_action_frames(state);
            let baseline = read_png_alpha_bounds(&frames[0]);
            let center_tolerance = pet_frame_center_drift_tolerance(state);
            let baseline_tolerance = pet_frame_baseline_drift_tolerance(state);
            for frame in frames {
                let bounds = read_png_alpha_bounds(&frame);
                assert_eq!(bounds.width, PET_FRAME_CANVAS, "{frame} canvas width");
                assert_eq!(bounds.height, PET_FRAME_CANVAS, "{frame} canvas height");

                let center_delta = (bounds.primary_center_x - baseline.primary_center_x).abs();
                assert!(
                    center_delta <= center_tolerance,
                    "{frame} center_x {} differs from state baseline {} by {}",
                    bounds.primary_center_x,
                    baseline.primary_center_x,
                    center_delta
                );

                let baseline_delta = bounds.primary_bottom.abs_diff(baseline.primary_bottom);
                assert!(
                    baseline_delta <= baseline_tolerance,
                    "{frame} bottom {} differs from state baseline {} by {}",
                    bounds.primary_bottom,
                    baseline.primary_bottom,
                    baseline_delta
                );
            }
        }
    }

    #[test]
    fn pet_action_frame_offsets_stabilize_visual_centroid() {
        for state in [
            "thinking",
            "working",
            "attention",
            "notification",
            "dragging",
            "carrying",
            "juggling",
            "sweeping",
            "warning",
            "success",
        ] {
            let frames = pet_action_frames(state);
            let offsets = pet_action_frame_offsets(state);
            assert_eq!(offsets.len(), frames.len(), "{state} offset count");

            let mut adjusted = Vec::new();
            for (frame, offset) in frames.iter().zip(offsets.iter()) {
                let bounds = read_png_alpha_bounds(frame);
                adjusted.push(bounds.alpha_centroid_x * PET_FRAME_RENDER_SCALE + offset);
            }

            let min = adjusted
                .iter()
                .fold(f32::INFINITY, |acc, value| acc.min(*value));
            let max = adjusted
                .iter()
                .fold(f32::NEG_INFINITY, |acc, value| acc.max(*value));
            let drift = max - min;
            let tolerance = pet_frame_visual_drift_tolerance(state);
            assert!(
                drift <= tolerance,
                "{state} compensated visual drift {drift} exceeds {tolerance}; adjusted={adjusted:?}"
            );
        }
    }

    fn pet_frame_center_drift_tolerance(state: &str) -> f32 {
        match state {
            "sleeping" | "crowned" => 12.0,
            _ => PET_FRAME_DEFAULT_CENTER_DRIFT_TOLERANCE,
        }
    }

    fn pet_frame_baseline_drift_tolerance(state: &str) -> u32 {
        match state {
            "sleeping" | "crowned" => 8,
            _ => PET_FRAME_DEFAULT_BASELINE_DRIFT_TOLERANCE,
        }
    }

    fn pet_frame_visual_drift_tolerance(state: &str) -> f32 {
        match state {
            "sweeping" | "perform_martial" => 24.0,
            _ => PET_FRAME_VISUAL_DRIFT_TOLERANCE,
        }
    }

    fn pet_frame_primary_height_ratio_tolerance(state: &str) -> f32 {
        match state {
            "success" | "juggling" => 1.30,
            "blink" => 1.02,
            "thinking" | "carrying" | "notification" | "attention" | "warning" => 1.12,
            _ => 1.08,
        }
    }

    #[test]
    fn pet_action_frames_have_no_straight_edge_cuts() {
        for state in [
            "idle",
            "blink",
            "thinking",
            "sleeping",
            "warning",
            "success",
            "crowned",
            "dragging",
            "perform_martial",
        ] {
            for frame in pet_action_frames(state) {
                let bounds = read_png_alpha_bounds(&frame);
                assert!(
                    bounds.min_column_visible_ratio <= PET_FRAME_EDGE_CUT_RATIO_TOLERANCE,
                    "{frame} left edge looks cropped: ratio {} exceeds {}",
                    bounds.min_column_visible_ratio,
                    PET_FRAME_EDGE_CUT_RATIO_TOLERANCE
                );
                assert!(
                    bounds.max_column_visible_ratio <= PET_FRAME_EDGE_CUT_RATIO_TOLERANCE,
                    "{frame} right edge looks cropped: ratio {} exceeds {}",
                    bounds.max_column_visible_ratio,
                    PET_FRAME_EDGE_CUT_RATIO_TOLERANCE
                );
            }
        }
    }

    #[test]
    fn pet_wuxia_frames_keep_policy_stable_scale_and_anchors() {
        // 生成时的 uniform_fit scale 无法从最终 PNG 反推；这里仅锁定已跟踪脚本的
        // 四个状态策略与共享 scale 分支，几何断言仍直接读取当前运行时 PNG。
        assert_pet_stabilizer_policy_contract();

        for state in [
            "idle",
            "blink",
            "thinking",
            "carrying",
            "juggling",
            "notification",
            "attention",
            "warning",
            "success",
            "crowned",
        ] {
            let frames = pet_action_frames(state);
            let heights: Vec<u32> = frames
                .iter()
                .map(|frame| read_png_alpha_bounds(frame).primary_height)
                .collect();
            let min = heights.iter().min().copied().unwrap_or(1);
            let max = heights.iter().max().copied().unwrap_or(1);
            let tolerance = pet_frame_primary_height_ratio_tolerance(state);
            assert!(
                max as f32 / min as f32 <= tolerance,
                "{state} standing-style frames should not visibly jump in size: {heights:?}; tolerance={tolerance}"
            );
        }

        for state in ["dragging", "sleeping"] {
            let frames = pet_action_frames(state);
            let centers: Vec<f32> = frames
                .iter()
                .map(|frame| read_png_alpha_bounds(frame).visible_center_y)
                .collect();
            let min = centers
                .iter()
                .fold(f32::INFINITY, |acc, value| acc.min(*value));
            let max = centers
                .iter()
                .fold(f32::NEG_INFINITY, |acc, value| acc.max(*value));
            assert!(
                max - min <= 2.0,
                "{state} center-anchored frames should not drift vertically: {centers:?}"
            );
        }

        for state in ["working", "sweeping", "perform_martial"] {
            let frames = pet_action_frames(state);
            let bottoms: Vec<u32> = frames
                .iter()
                .map(|frame| read_png_alpha_bounds(frame).visible_bottom)
                .collect();
            let min = bottoms.iter().min().copied().unwrap_or_default();
            let max = bottoms.iter().max().copied().unwrap_or_default();
            assert!(
                max.abs_diff(min) <= 1,
                "{state} baseline-anchored frames should not bob at the feet: {bottoms:?}"
            );
        }
    }

    #[test]
    fn pet_martial_frames_keep_character_scale_consistent_with_idle() {
        // 面部测量只覆盖当前 pet_action_frames 指向的 PNG；不读取、生成或回填历史 metrics。
        assert_pet_stabilizer_policy_contract();
        let martial = pet_face_measurements("perform_martial");
        let idle = pet_face_measurements("idle");
        let martial_min = martial
            .iter()
            .map(|measurement| measurement.height)
            .min()
            .expect("perform_martial 当前 PNG 必须识别到至少一帧面部")
            as f64;
        let martial_max = martial
            .iter()
            .map(|measurement| measurement.height)
            .max()
            .expect("perform_martial 当前 PNG 必须识别到至少一帧面部")
            as f64;
        let idle_min = idle
            .iter()
            .map(|measurement| measurement.height)
            .min()
            .expect("idle 当前 PNG 必须识别到至少一帧面部") as f64;
        let idle_max = idle
            .iter()
            .map(|measurement| measurement.height)
            .max()
            .expect("idle 当前 PNG 必须识别到至少一帧面部") as f64;

        assert!(
            martial_max / martial_min <= 1.08,
            "martial face scale should stay stable: min={martial_min}, max={martial_max}"
        );
        let martial_mid = (martial_min + martial_max) / 2.0;
        let idle_mid = (idle_min + idle_max) / 2.0;
        assert!(
            (martial_mid / idle_mid - 1.0).abs() <= 0.08,
            "martial character scale should match idle: martial={martial_mid}, idle={idle_mid}"
        );
    }

    #[test]
    fn pet_blink_overlay_reuses_idle_character_scale() {
        let idle_frames = pet_action_frames("idle");
        let blink_frames = pet_action_frames("blink");
        assert_eq!(
            blink_frames.len(),
            idle_frames.len(),
            "blink signal should reuse the complete idle frame sequence"
        );
        assert!(
            blink_frames
                .iter()
                .zip(idle_frames.iter())
                .all(|(blink, idle)| blink == idle),
            "blink signal should reuse idle assets instead of a separate animation"
        );

        assert!(PET_MINI_HTML.contains("let idleClosedFrames = actionFrames(\"idle-closed\", 8)"));
        assert!(PET_MINI_HTML
            .contains("if (state === \"idle\" && performance.now() < blinkOverlayUntil)"));
        for (index, (idle, blink)) in idle_frames.iter().zip(blink_frames.iter()).enumerate() {
            let idle_bounds = read_png_alpha_bounds(idle);
            let closed_path = format!("assets/pet-actions/idle-closed-{index}.png");
            let closed_bounds = read_png_alpha_bounds(&closed_path);
            assert_eq!(
                closed_bounds.width, PET_FRAME_CANVAS,
                "closed frame {index} canvas width"
            );
            assert_eq!(
                closed_bounds.height, PET_FRAME_CANVAS,
                "closed frame {index} canvas height"
            );
            assert!(
                (closed_bounds.primary_width as f32 / idle_bounds.primary_width as f32 - 1.0).abs()
                    <= 0.02,
                "closed frame {index} should preserve idle body width: closed={}, idle={}",
                closed_bounds.primary_width,
                idle_bounds.primary_width
            );
            assert!(
                (closed_bounds.primary_height as f32 / idle_bounds.primary_height as f32 - 1.0)
                    .abs()
                    <= 0.02,
                "closed frame {index} should preserve idle body height: closed={}, idle={}",
                closed_bounds.primary_height,
                idle_bounds.primary_height
            );
            assert!(
                closed_bounds
                    .primary_bottom
                    .abs_diff(idle_bounds.primary_bottom)
                    <= 2,
                "closed frame {index} should preserve idle baseline: closed={}, idle={}",
                closed_bounds.primary_bottom,
                idle_bounds.primary_bottom
            );
            assert!(
                (closed_bounds.primary_center_x - idle_bounds.primary_center_x).abs() <= 2.0,
                "closed frame {index} should preserve idle center: closed={}, idle={}",
                closed_bounds.primary_center_x,
                idle_bounds.primary_center_x
            );
            assert_eq!(
                blink, idle,
                "blink signal should point at idle frame {index}"
            );
        }
    }

    #[test]
    fn pet_success_frames_do_not_mix_closeup_character_scales() {
        // 成功帧同样只验证当前 PNG 的 face_bbox；无法识别面部时 helper 会直接失败。
        assert_pet_stabilizer_policy_contract();
        let success = pet_face_measurements("success");
        let min = success
            .iter()
            .map(|measurement| measurement.height)
            .min()
            .expect("success 当前 PNG 必须识别到至少一帧面部") as f64;
        let max = success
            .iter()
            .map(|measurement| measurement.height)
            .max()
            .expect("success 当前 PNG 必须识别到至少一帧面部") as f64;
        assert!(
            max / min <= 1.05,
            "success frames should not jump in size: min={min}, max={max}"
        );
    }

    #[derive(Debug)]
    struct PetFaceMeasurement {
        bbox: (u32, u32, u32, u32),
        width: u32,
        height: u32,
    }

    fn pet_face_measurements(state: &str) -> Vec<PetFaceMeasurement> {
        pet_action_frames(state)
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let bbox = read_png_face_bbox(frame);
                let measurement = PetFaceMeasurement {
                    bbox,
                    width: bbox.2 - bbox.0,
                    height: bbox.3 - bbox.1,
                };
                println!(
                    "pet_face_measurement state={state} frame={index} path={frame} bbox=[{}, {}, {}, {}] face_width={} face_height={}",
                    measurement.bbox.0,
                    measurement.bbox.1,
                    measurement.bbox.2,
                    measurement.bbox.3,
                    measurement.width,
                    measurement.height
                );
                measurement
            })
            .collect()
    }

    fn read_png_face_bbox(frame: &str) -> (u32, u32, u32, u32) {
        let asset_path = frame.split('?').next().unwrap_or(frame);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ui")
            .join(asset_path);
        let file = File::open(&path).unwrap_or_else(|error| {
            panic!("无法打开当前桌宠 PNG {}: {error}", path.display());
        });
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info().unwrap_or_else(|error| {
            panic!("无法读取当前桌宠 PNG {}: {error}", path.display());
        });
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).unwrap_or_else(|error| {
            panic!("无法解码当前桌宠 PNG {}: {error}", path.display());
        });
        assert!(
            info.width > 0 && info.height > 0,
            "当前桌宠 PNG {} 不能为空尺寸",
            path.display()
        );
        assert!(
            matches!(info.bit_depth, png::BitDepth::Eight),
            "当前桌宠 PNG {} 必须是 8-bit，实际为 {:?}",
            path.display(),
            info.bit_depth
        );
        let bytes = &buffer[..info.buffer_size()];
        let channels = match info.color_type {
            png::ColorType::Rgba => 4,
            png::ColorType::Rgb => 3,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Grayscale => 1,
            png::ColorType::Indexed => {
                panic!(
                    "当前桌宠 PNG {} 使用了不支持的 indexed 色彩类型",
                    path.display()
                );
            }
        };
        let pixel_count = (info.width * info.height) as usize;
        let mut skin = vec![false; pixel_count];
        for y in 0..info.height {
            for x in 0..info.width {
                let index = ((y * info.width + x) as usize) * channels;
                let (red, green, blue, alpha) = match info.color_type {
                    png::ColorType::Rgba => (
                        bytes[index],
                        bytes[index + 1],
                        bytes[index + 2],
                        bytes[index + 3],
                    ),
                    png::ColorType::Rgb => (bytes[index], bytes[index + 1], bytes[index + 2], 255),
                    png::ColorType::GrayscaleAlpha => {
                        let gray = bytes[index];
                        (gray, gray, gray, bytes[index + 1])
                    }
                    png::ColorType::Grayscale => {
                        let gray = bytes[index];
                        (gray, gray, gray, 255)
                    }
                    png::ColorType::Indexed => unreachable!("indexed 色彩类型已在上方拒绝"),
                };
                if alpha > 80
                    && red > 145
                    && green > 70
                    && green < 220
                    && blue > 45
                    && f64::from(red) > f64::from(green) * 1.04
                    && f64::from(green) > f64::from(blue) * 1.03
                    && red - blue > 45
                {
                    skin[(y * info.width + x) as usize] = true;
                }
            }
        }

        let mut seen = vec![false; pixel_count];
        let mut candidates = Vec::new();
        for y in 0..info.height {
            for x in 0..info.width {
                let start = (y * info.width + x) as usize;
                if !skin[start] || seen[start] {
                    continue;
                }

                let mut queue = VecDeque::from([(x, y)]);
                seen[start] = true;
                let mut area = 0u32;
                let mut left = x;
                let mut right = x + 1;
                let mut top = y;
                let mut bottom = y + 1;

                while let Some((current_x, current_y)) = queue.pop_front() {
                    area += 1;
                    left = left.min(current_x);
                    right = right.max(current_x + 1);
                    top = top.min(current_y);
                    bottom = bottom.max(current_y + 1);

                    let x_start = current_x.saturating_sub(1);
                    let x_end = current_x.saturating_add(1).min(info.width - 1);
                    let y_start = current_y.saturating_sub(1);
                    let y_end = current_y.saturating_add(1).min(info.height - 1);
                    for next_y in y_start..=y_end {
                        for next_x in x_start..=x_end {
                            if next_x == current_x && next_y == current_y {
                                continue;
                            }
                            let index = (next_y * info.width + next_x) as usize;
                            if skin[index] && !seen[index] {
                                seen[index] = true;
                                queue.push_back((next_x, next_y));
                            }
                        }
                    }
                }

                let face_width = right - left;
                let face_height = bottom - top;
                let aspect = f64::from(face_width) / f64::from(face_height.max(1));
                let center_x = f64::from(left + right) / 2.0;
                if area >= 200
                    && (0.7..=1.4).contains(&aspect)
                    && (20..=120).contains(&face_width)
                    && (20..=120).contains(&face_height)
                    && (center_x - f64::from(info.width) / 2.0).abs()
                        <= f64::from(info.width) * 0.28
                    && f64::from(top) < f64::from(info.height) * 0.65
                {
                    // 元组顺序与 Python max(candidates) 相同，保留同面积时的 tie-break。
                    candidates.push((area, left, top, right, bottom));
                }
            }
        }

        let (_, left, top, right, bottom) = candidates.into_iter().max().unwrap_or_else(|| {
            panic!(
                "当前桌宠 PNG {} 无法识别角色面部（skin_components 无合格候选）",
                path.display()
            )
        });
        (left, top, right, bottom)
    }

    #[derive(Debug)]
    struct AlphaBounds {
        width: u32,
        height: u32,
        visible_center_y: f32,
        visible_bottom: u32,
        primary_width: u32,
        primary_height: u32,
        primary_area: u32,
        primary_center_x: f32,
        alpha_centroid_x: f32,
        primary_bottom: u32,
        min_column_visible_ratio: f32,
        max_column_visible_ratio: f32,
    }

    fn read_png_alpha_bounds(frame: &str) -> AlphaBounds {
        let asset_path = frame.split('?').next().unwrap_or(frame);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ui")
            .join(asset_path);
        let file = File::open(&path).unwrap_or_else(|error| {
            panic!("failed to open {}: {error}", path.display());
        });
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info().unwrap_or_else(|error| {
            panic!("failed to read png info for {}: {error}", path.display());
        });
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).unwrap_or_else(|error| {
            panic!("failed to decode {}: {error}", path.display());
        });
        let bytes = &buffer[..info.buffer_size()];
        let channels = match info.color_type {
            png::ColorType::Rgba => 4,
            png::ColorType::Rgb => 3,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Grayscale => 1,
            png::ColorType::Indexed => {
                panic!("indexed png is not supported for {}", path.display());
            }
        };
        let alpha_index = match info.color_type {
            png::ColorType::Rgba => Some(3),
            png::ColorType::GrayscaleAlpha => Some(1),
            _ => None,
        };

        let mut min_x = info.width;
        let mut min_y = info.height;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut seen = false;
        let mut sum_alpha = 0f32;
        let mut sum_x = 0f32;
        let mut visible = vec![false; (info.width * info.height) as usize];
        let mut min_column_visible = 0u32;
        let mut max_column_visible = 0u32;
        for y in 0..info.height {
            for x in 0..info.width {
                let index = ((y * info.width + x) as usize) * channels;
                let alpha = alpha_index
                    .map(|offset| bytes[index + offset])
                    .unwrap_or(255);
                if alpha > 8 {
                    visible[index / channels] = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    sum_alpha += f32::from(alpha);
                    sum_x += x as f32 * f32::from(alpha);
                    seen = true;
                }
            }
        }

        assert!(seen, "{} has no visible pixels", path.display());
        let (primary_min_x, primary_max_x, primary_min_y, primary_bottom, primary_area) =
            largest_visible_component_bounds(&visible, info.width, info.height);
        let visible_height = max_y - min_y + 1;
        for y in min_y..=max_y {
            for (x, visible_count) in [
                (min_x, &mut min_column_visible),
                (max_x, &mut max_column_visible),
            ] {
                let index = ((y * info.width + x) as usize) * channels;
                let alpha = alpha_index
                    .map(|offset| bytes[index + offset])
                    .unwrap_or(255);
                if alpha > 8 {
                    *visible_count += 1;
                }
            }
        }
        AlphaBounds {
            width: info.width,
            height: info.height,
            visible_center_y: (min_y + max_y) as f32 / 2.0,
            visible_bottom: max_y,
            primary_width: primary_max_x - primary_min_x + 1,
            primary_height: primary_bottom - primary_min_y + 1,
            primary_area,
            primary_center_x: (primary_min_x + primary_max_x) as f32 / 2.0,
            alpha_centroid_x: sum_x / sum_alpha,
            primary_bottom,
            min_column_visible_ratio: min_column_visible as f32 / visible_height as f32,
            max_column_visible_ratio: max_column_visible as f32 / visible_height as f32,
        }
    }

    fn largest_visible_component_bounds(
        visible: &[bool],
        width: u32,
        height: u32,
    ) -> (u32, u32, u32, u32, u32) {
        let mut visited = vec![false; visible.len()];
        let mut largest_area = 0u32;
        let mut largest_bounds = (0u32, 0u32, 0u32, 0u32, 0u32);

        for y in 0..height {
            for x in 0..width {
                let start = (y * width + x) as usize;
                if !visible[start] || visited[start] {
                    continue;
                }

                let mut queue = VecDeque::from([(x, y)]);
                visited[start] = true;
                let mut area = 0u32;
                let mut min_x = x;
                let mut max_x = x;
                let mut min_y = y;
                let mut max_y = y;

                while let Some((cx, cy)) = queue.pop_front() {
                    area += 1;
                    min_x = min_x.min(cx);
                    max_x = max_x.max(cx);
                    min_y = min_y.min(cy);
                    max_y = max_y.max(cy);

                    for (nx, ny) in [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ] {
                        if nx >= width || ny >= height {
                            continue;
                        }
                        let index = (ny * width + nx) as usize;
                        if visible[index] && !visited[index] {
                            visited[index] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }

                if area > largest_area {
                    largest_area = area;
                    largest_bounds = (min_x, max_x, min_y, max_y, area);
                }
            }
        }

        largest_bounds
    }

    fn median_u32(mut values: Vec<u32>) -> f32 {
        values.sort_unstable();
        let len = values.len();
        assert!(len > 0, "median requires at least one value");
        if len % 2 == 0 {
            (values[len / 2 - 1] as f32 + values[len / 2] as f32) / 2.0
        } else {
            values[len / 2] as f32
        }
    }
}
