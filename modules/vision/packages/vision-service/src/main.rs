use std::path::PathBuf;

use vision::{
    build_showui_grounding_request, parse_relative_point, LocalOpenAiVisionBackend, VisionBackend,
    VisionImageSource, VisionRequest, ZhipuVisionBackend,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let showui_ground = args.first().is_some_and(|value| value == "--showui-ground");
    if showui_ground {
        args.remove(0);
    }
    if args.len() < 2 {
        eprintln!("Usage: claw-vision-smoke [--showui-ground] <image-path> <prompt>");
        std::process::exit(1);
    }

    let image_path = PathBuf::from(&args[0]);
    let prompt = args[1..].join(" ");
    let backend = configured_backend();
    let request = if showui_ground {
        build_showui_grounding_request(prompt, VisionImageSource::Path(image_path))
    } else {
        VisionRequest {
            prompt,
            images: vec![VisionImageSource::Path(image_path)],
            post_prompt: None,
        }
    };
    let response = backend.analyze(&request)?;
    let relative_point = showui_ground
        .then(|| parse_relative_point(&response.text))
        .flatten();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "backend": format!("{:?}", response.backend),
            "model": response.model,
            "text": response.text,
            "relative_point": relative_point,
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
