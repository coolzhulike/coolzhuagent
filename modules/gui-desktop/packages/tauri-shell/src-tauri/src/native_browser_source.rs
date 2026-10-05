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
