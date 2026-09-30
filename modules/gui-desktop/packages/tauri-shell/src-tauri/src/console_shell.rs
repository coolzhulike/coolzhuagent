//! 保留主控制台的外部链接行为，避免 Shell 插件取消浏览器网页的新窗口导航。
use tauri::{
    ipc::Invoke, plugin::Plugin, webview::PageLoadPayload, AppHandle, RunEvent, Url,
    Webview, Window, Wry,
};

pub fn init() -> impl Plugin<Wry> {
    ConsoleShell {
        inner: tauri_plugin_shell::init(),
    }
}

struct ConsoleShell<P> {
    inner: P,
}

fn guarded_script(script: &str, origin: Option<&Url>) -> String {
    // 来源不可用时不安装任何监听器；权限仍由原有 capability 决定。
    let origin_json = serde_json::to_string(&origin.map(|url| url.origin().ascii_serialization()))
        .expect("固定来源可序列化");
    let label_json = serde_json::to_string(super::CONSOLE_LABEL).expect("固定标签可序列化");
    format!(
        "(function(){{if(window.top!==window || window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label!=={label_json} || window.location.origin!=={origin_json}) return;\n{script}\n}})();"
    )
}

impl<P: Plugin<Wry>> Plugin<Wry> for ConsoleShell<P> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn initialize(
        &mut self,
        app: &AppHandle<Wry>,
        config: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.inner.initialize(app, config)
    }

    fn initialization_script(&self) -> Option<String> {
        // 当前 Shell 插件使用主框架脚本。Windows 仍会注入子框架，因此另有 top 检查。
        self.inner.initialization_script().map(|script| {
            guarded_script(&script, super::browser_panel::console_origin())
        })
    }

    fn window_created(&mut self, window: Window<Wry>) {
        self.inner.window_created(window);
    }

    fn webview_created(&mut self, webview: Webview<Wry>) {
        self.inner.webview_created(webview);
    }

    fn on_navigation(&mut self, webview: &Webview<Wry>, url: &Url) -> bool {
        self.inner.on_navigation(webview, url)
    }

    fn on_page_load(&mut self, webview: &Webview<Wry>, payload: &PageLoadPayload<'_>) {
        self.inner.on_page_load(webview, payload);
    }

    fn on_event(&mut self, app: &AppHandle<Wry>, event: &RunEvent) {
        self.inner.on_event(app, event);
    }

    fn extend_api(&mut self, invoke: Invoke<Wry>) -> bool {
        self.inner.extend_api(invoke)
    }
}
