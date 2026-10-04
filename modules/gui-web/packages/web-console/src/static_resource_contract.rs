//! 生产界面的代码资源必须与宿主同版；开发构建保留明确的静态热更新路径。
use std::path::Path;

const CORE: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../index.html")),
    ("src/app.js", include_bytes!("app.js")),
    ("src/dsh_market.js", include_bytes!("dsh_market.js")),
    ("src/styles.css", include_bytes!("styles.css")),
    ("src/chat_experience.js", include_bytes!("chat_experience.js")),
    ("src/chat_experience.css", include_bytes!("chat_experience.css")),
    ("src/wuxia_layout.css", include_bytes!("wuxia_layout.css")),
    ("src/realtime_voice_capture.js", include_bytes!("realtime_voice_capture.js")),
    ("src/realtime_audio_output.js", include_bytes!("realtime_audio_output.js")),
    ("src/stt_tail_capture.js", include_bytes!("stt_tail_capture.js")),
    ("src/model_settings.js", include_bytes!("model_settings.js")),
    ("src/devin_auth.js", include_bytes!("devin_auth.js")),
    ("src/model_settings.css", include_bytes!("model_settings.css")),
    ("src/workspace_panels.js", include_bytes!("workspace_panels.js")),
    ("src/workspace_panels.css", include_bytes!("workspace_panels.css")),
    ("src/scroll_theme.css", include_bytes!("scroll_theme.css")),
    ("src/content_preview.js", include_bytes!("content_preview.js")),
    ("src/native_browser_panel.js", include_bytes!("native_browser_panel.js")),
    ("src/run_activity.js", include_bytes!("run_activity.js")),
    ("src/video_wait.js", include_bytes!("video_wait.js")),
    ("assets/bamboo-banner-wind.js", include_bytes!("../assets/bamboo-banner-wind.js")),
];

pub(super) fn embedded(relative: &Path) -> Option<&'static [u8]> {
    let path=relative.to_string_lossy().replace('\\',"/");
    CORE.iter().find(|(name,_)| *name==path).map(|(_,bytes)| *bytes)
}

fn mismatch(path: &str) -> String {
    format!("界面资源版本不一致：{path}。请重新安装与当前程序配套的完整版本，再打开界面。为避免混用不同版本，本次未加载控制台。")
}

pub(super) fn validate(relative: &Path, bytes: &[u8]) -> Result<(),String> {
    if !cfg!(debug_assertions) {
        if let Some(expected)=embedded(relative) {
            if expected!=bytes { return Err(mismatch(&relative.to_string_lossy())); }
        }
    }
    Ok(())
}

pub(super) async fn preflight_index() -> Result<(),String> {
    if cfg!(debug_assertions) { return Ok(()); }
    // 与静态服务使用完全相同的首个可读候选；缺文件走同一二进制内嵌兜底。
    for (name,expected) in CORE {
        for candidate in super::static_path_candidates(Path::new(name)) {
            if let Ok(actual)=tokio::fs::read(candidate).await {
                if actual!=*expected { return Err(mismatch(name)); }
                break;
            }
        }
    }
    Ok(())
}
