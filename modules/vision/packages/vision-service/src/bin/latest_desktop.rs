use vision::{
    analyze_latest_desktop_with_backend, default_latest_desktop_capture_path,
    LocalOpenAiVisionBackend, VisionBackend, ZhipuVisionBackend,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if prompt.trim().is_empty() {
        eprintln!("Usage: claw-latest-desktop-vision <prompt>");
        std::process::exit(1);
    }

    let backend = configured_backend();
    let response = analyze_latest_desktop_with_backend(backend.as_ref(), &prompt)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "backend": format!("{:?}", response.backend),
            "model": response.model,
            "latest_capture": default_latest_desktop_capture_path(),
            "text": response.text,
            "request_id": response.request_id,
            "total_tokens": response.total_tokens,
        }))?
    );

    Ok(())
}

fn configured_backend() -> Box<dyn VisionBackend> {
    let backend =
        std::env::var("CLAW_VISION_BACKEND").unwrap_or_else(|_| "local-openai".to_string());
    if matches!(
        backend.trim().to_ascii_lowercase().as_str(),
        "zhipu" | "glm" | "cloud" | "remote"
    ) {
        let model = std::env::var("CLAW_CLOUD_VISION_MODEL")
            .or_else(|_| std::env::var("CLAW_VISION_MODEL"))
            .unwrap_or_else(|_| "glm-vision".to_string());
        Box::new(ZhipuVisionBackend::default().with_model(model))
    } else {
        let base_url = std::env::var("CLAW_LOCAL_VISION_BASE_URL")
            .unwrap_or_else(|_| vision::default_local_vision_base_url().to_string());
        let model = std::env::var("CLAW_LOCAL_VISION_MODEL")
            .unwrap_or_else(|_| vision::default_local_vision_model().to_string());
        let api_key = std::env::var("CLAW_LOCAL_VISION_API_KEY").unwrap_or_default();
        Box::new(LocalOpenAiVisionBackend::new(base_url, model).with_api_key(api_key))
    }
}
