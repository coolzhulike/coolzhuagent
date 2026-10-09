# Browser严格输入时序可观测方案（尚未实施）
剩余验收：真实nativeTarget代际替换和跨来源文档提交恰好发生在down/up之间，原up仍发旧controller，新资源不接旧动作。既有同步document.replace、beforeunload、普通关闭已过；097新Target已创建但晚于release6.414秒，098导航成功但严格commit未证明，不追认。
现实现click在一个UI闭包获取原controller后顺序排队mousePressed/mouseReleased，不等down回调才排队up；execution_gate持续到结果/3s超时，retire_view立即hide后等待gate再close。页面渲染线程pointerdown可能很慢，COM排队、ACK和DOM事件各不是同一个时间点。
拟新增最小opt-in本地结构化诊断，复用既有COOLZHU_BROWSER_NAV_DIAGNOSTICS开关：同宿主单调微秒+序号，input attempt_id和resource generation关联press/release调用开始、入队返回、ACK真假、超时及结算；记录面板generation撤销/新建、SourceChanged的IsNewDocument、ContentLoading。仅标记宿主看到的事实，不改3s上限、不故意延迟release、不注入script、不接受页面控制、不增加模型可用API、不改变权限或状态机、日志不记URL/query/token/scope/文本。默认关闭、不加前端调试控件，缺ACK保留未知；新文档脚本/HTTP请求时间不冒充commit。
官方WebView2导航说明：NavigationStarting是发起网络请求；SourceChanged也可为fragment；ContentLoading才开始新页面内容加载；不同navigationId可重叠。来源https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/navigation-events 。考虑SourceChanged(new_document)和ContentLoading仍不宜强称精确提交时刻，要求区分保守上下界。
需审核：A是否需要新模块或直接复用既有logger更小；B何种真实记录才能证明down<generation变化/新文档commit<up；C回调顺序与DOM处理不能等同，ACK晚到情况下如何不误判；D是否可以通过正常UI关闭重开与页面2.4秒真实pointerdown处理命中（不修改宿主等待），如果难命中应诚实保留缺口；E只读诊断会不会泄漏或扰动竞态，如何最小化；F源码层已保证up原controller是否足够验收基本能力，严格竞态与基本能力分开列证据。


## native_browser_input.rs
```rust
/// 资格已撤销后隐藏原视图，等待当前执行清理完成；不阻塞 UI 或重新启用资源。
pub(super) fn retire_view(view:tauri::Webview) {
    let _=view.hide();
    tauri::async_runtime::spawn(async move {
        let _execution=execution_gate().lock().await;
        let _=view.close();
    });
}

/// 按下/释放使用同一个原始controller。导航或取消不能截断释放；COM入队不等于回执成功。
async fn click(app: &AppHandle, target: VerifiedTarget, expires_ms: u64) -> (PanelInputOutcome,bool,bool) {
    if now() >= expires_ms || super::browser_panel::input_resource(app).as_ref() != Some(&target.resource) {
        return (PanelInputOutcome::NotDispatched,false,false);
    }
    #[cfg(windows)]
    {
        use webview2_com::{CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler};
        use super::native_browser_devtools::bounded_callback;
        let Some(view) = app.get_webview(&target.resource.label) else { return (PanelInputOutcome::NotDispatched,false,false); };
        let (sender,receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(Mutex::new(Some(sender)));
        let sender_outside = sender.clone();
        let app_ui=app.clone();
        let queued = view.with_webview(move |platform| {
            let finish = |value| { if let Ok(mut sender)=sender.lock() { if let Some(sender)=sender.take() { let _=sender.send(value); } } };
            if now() >= expires_ms || super::browser_panel::input_resource(&app_ui).as_ref() != Some(&target.resource) {
                finish((PanelInputOutcome::NotDispatched,false,false));return;
            }
            let core = match unsafe { platform.controller().CoreWebView2() } {
                Ok(core)=>core,Err(_)=>{finish((PanelInputOutcome::NotDispatched,false,false));return;}
            };
            let phases=std::sync::Arc::new(Mutex::new((None::<bool>,None::<bool>)));
            let make_handler=|pressed:bool| {
                let phases=phases.clone();let sender=sender.clone();
                let handler:ICoreWebView2CallDevToolsProtocolMethodCompletedHandler=bounded_callback::Handler(Box::new(move |status,text| {
                    let confirmed=status.is_ok() && unsafe {bounded_callback::read(text)}.is_ok_and(|raw|
                        serde_json::from_str::<serde_json::Value>(&raw).is_ok_and(|v|v.as_object().is_some_and(|v|v.is_empty())));
                    if let Ok(mut phase)=phases.lock() {
                        let slot=if pressed {&mut phase.0} else {&mut phase.1};
                        if slot.is_none() { *slot=Some(confirmed); }
                        if let (Some(down),Some(up))=*phase { if let Ok(mut sender)=sender.lock() { if let Some(sender)=sender.take() {
                            let _=sender.send((if down && up {PanelInputOutcome::Released} else {PanelInputOutcome::ReleaseUnknown},down,up));
                        } } }
                    }
                    Ok(())
                })).into();handler
            };
            let down_handler=make_handler(true);let up_handler=make_handler(false);
            let release_params=serde_json::json!({"type":"mouseReleased","x":target.x,"y":target.y,"button":"left","buttons":0,"clickCount":1}).to_string();
            let method=CoTaskMemPWSTR::from("Input.dispatchMouseEvent");
            let press_params=serde_json::json!({"type":"mousePressed","x":target.x,"y":target.y,"button":"left","buttons":1,"clickCount":1}).to_string();
            let parameters=CoTaskMemPWSTR::from(press_params.as_str());
            let down_queued=unsafe {core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*parameters.as_ref().as_pcwstr(),&down_handler)}.is_ok();
            // 同一UI闭包按顺序入队两条命令；绝不等待down回调才排队up，丢回调也必须尝试释放。
            let parameters=CoTaskMemPWSTR::from(release_params.as_str());
            let up_queued=unsafe {core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*parameters.as_ref().as_pcwstr(),&up_handler)}.is_ok();
            if !down_queued || !up_queued { finish((PanelInputOutcome::ReleaseUnknown,false,false)); }
        });
        if queued.is_err() {
            // UI闭包是否已经执行无法证明，按未知处理，不能自动重放。
            drop(sender_outside);return (PanelInputOutcome::ReleaseUnknown,false,false);
        }
        tokio::time::timeout(Duration::from_secs(3),receiver).await.ok().and_then(Result::ok)
            .unwrap_or((PanelInputOutcome::ReleaseUnknown,false,false))
    }
    #[cfg(not(windows))]
    { (PanelInputOutcome::NotDispatched,false,false) }
}

```


## native_browser_source.rs
```rust
//! 原生同文档 URL 事件只同步地址显示，不授予输入权限、不注入网页脚本。
use tauri::{AppHandle, Webview};

#[cfg(windows)]
mod platform {
    use super::*;
    use webview2_com::{CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2SourceChangedEventArgs,
        ICoreWebView2SourceChangedEventHandler, ICoreWebView2SourceChangedEventHandler_Impl,
    }};
    use windows::core::{implement, Ref};

    #[implement(ICoreWebView2SourceChangedEventHandler)]
    struct Handler { app: AppHandle, generation: u64, label: String }
    impl ICoreWebView2SourceChangedEventHandler_Impl for Handler_Impl {
        fn Invoke(&self, sender: Ref<'_, ICoreWebView2>, args: Ref<'_, ICoreWebView2SourceChangedEventArgs>) -> windows::core::Result<()> {
            let (Some(sender), Some(args)) = (sender.as_ref(), args.as_ref()) else { return Ok(()); };
            let mut new_document = windows::core::BOOL(0);
            unsafe { args.IsNewDocument(&mut new_document)?; }
            // 新文档的加载/重定向继续由原有加载回调负责。
            if new_document.as_bool() { return Ok(()); }
            let mut raw_source = windows::core::PWSTR::null();
            let read = unsafe { sender.Source(&mut raw_source) };
            let source = CoTaskMemPWSTR::from(raw_source);
            read?;
            if let Ok(url) = tauri::Url::parse(&source.to_string()) {
                super::super::browser_panel::update_same_document_source(&self.app, self.generation, &self.label, &url);
            }
            Ok(())
        }
    }

    pub async fn install(app: &AppHandle, view: &Webview, generation: u64) -> Result<i64, String> {
        let app = app.clone();
        let label = view.label().to_owned();
        let (tx, rx) = tokio::sync::oneshot::channel();
        view.with_webview(move |platform| {
            let result = unsafe { platform.controller().CoreWebView2().and_then(|core| {
                let handler: ICoreWebView2SourceChangedEventHandler = Handler { app, generation, label }.into();
                let mut token = 0;
                core.add_SourceChanged(&handler, &mut token)?;
                Ok(token)
            }) }.map_err(|_| "无法监听网页地址变化".to_string());
            let _ = tx.send(result);
        }).map_err(|_| "网页窗口不可用".to_string())?;
        rx.await.map_err(|_| "网页地址监听已取消".to_string())?
    }

    pub fn remove(view: &Webview, token: i64) {
        // 视图退役即解除 COM 回调，释放其 AppHandle；不保留 view 或 COM 对象。
        let _ = view.with_webview(move |platform| {
            let _ = unsafe { platform.controller().CoreWebView2().and_then(|core| core.remove_SourceChanged(token)) };
        });
    }
}

pub async fn install(app: &AppHandle, view: &Webview, generation: u64) -> Result<Option<i64>, String> {
    #[cfg(windows)] { platform::install(app, view, generation).await.map(Some) }
    #[cfg(not(windows))] { let _ = (app, view, generation); Ok(None) }
}

pub fn remove(view: &Webview, token: Option<i64>) {
    #[cfg(windows)] if let Some(token) = token { platform::remove(view, token); }
    #[cfg(not(windows))] { let _ = (view, token); }
}

```


## browser_panel.rs实际片段
```rust
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
        ("popup_sequence", state.popup_sequence.to_string()),
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
pub fn invalidate(app: &AppHandle, reason: &str) {
    let store = app.state::<PanelStore>();
    let (label, token, reply) = {
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
        (state.label.take(), state.source_changed_token.take(), state.reply.clone())
    };
    if let Some(view) = label.and_then(|label| app.get_webview(&label)) {
        super::native_browser_source::remove(&view, token);
        super::native_browser_input::retire_view(view);
    }
    emit(app, &reply);
}

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
            .zoom_hotkeys_enabled(true)
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
```
