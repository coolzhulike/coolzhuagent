//! 右侧网页是独立、无 capability 的 WebView。主界面只传位置与导航命令，
//! 网页的 URL、加载状态由原生回调提供，不维护猜测出来的浏览历史。
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelAction {
    Metrics,
    Navigate,
    Resize,
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
    label: Option<String>,
    reply: PanelReply,
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
        // 开机演出仅使用 capability 限定的事件接口，不需要应用自定义命令。
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

/// 窗口变化时立即销毁，不把旧坐标留到前端下一帧；前端收到事件后用新矩形重建。
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
        state.reply.loading = false;
        state.reply.reason = Some(reason.into());
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
        if let Ok((view, generation)) = view_for_scope(&app, &store, &command.scope) {
            let bounds = bounds.unwrap();
            view.set_bounds(tauri::Rect {
                position: LogicalPosition::new(bounds.x, bounds.y).into(),
                size: LogicalSize::new(bounds.width, bounds.height).into(),
            })
            .map_err(|_| "无法调整网页位置")?;
            // 重复展示同一地址不会刷新网页，也不丢失页面内的未提交内容。
            if view.url().ok().as_ref() != Some(&url) {
                view.navigate(url).map_err(|_| "无法打开网页")?;
            }
            update(&app, generation, |reply| {
                reply.bounds = Some(bounds);
                reply.error = None;
            });
            return current_reply(&store);
        }
        invalidate(&app, "scope-changed");
        let bounds = bounds.unwrap();
        let (generation, label) = {
            let mut state = store.state.lock().map_err(|_| "网页状态不可用")?;
            state.generation = state.generation.wrapping_add(1);
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
                let allowed = validate_url(url.as_str(), console_origin()).is_ok();
                if !allowed {
                    update(&navigation_app, generation, |reply| {
                        reply.error = Some("已阻止不支持的网页跳转".into())
                    });
                }
                allowed
            })
            .on_page_load(move |_view, payload| {
                update(&load_app, generation, |reply| {
                    reply.url = Some(payload.url().to_string());
                    reply.loading = matches!(payload.event(), PageLoadEvent::Started);
                    reply.reason = None;
                    reply.error = None;
                });
            })
            .on_document_title_changed(move |_view, title| {
                update(&title_app, generation, |reply| reply.title = Some(title));
            })
            .on_new_window(move |url, _features| {
                if validate_url(url.as_str(), console_origin()).is_ok() {
                    if let Some(view) = popup_app.get_webview(&popup_label) {
                        let _ = view.navigate(url);
                    }
                } else {
                    update(&popup_app, generation, |reply| {
                        reply.error = Some("已阻止不支持的网页跳转".into())
                    });
                }
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
    match command.action {
        PanelAction::Resize => {
            let bounds = bounds.unwrap();
            view.set_bounds(tauri::Rect {
                position: LogicalPosition::new(bounds.x, bounds.y).into(),
                size: LogicalSize::new(bounds.width, bounds.height).into(),
            })
            .map_err(|_| "无法调整网页位置")?;
            update(&app, generation, |reply| reply.bounds = Some(bounds));
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
    if let Ok(url) = view.url() {
        update(&app, generation, |reply| reply.url = Some(url.to_string()));
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
