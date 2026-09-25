use std::env;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

pub mod local_backend;
pub mod locate;
pub use locate::{
    AttemptStatus, BBoxPx, BackendAttempt, BackendId, LocateRequest, LocateResponse, LocateStatus,
    LocateTarget, PointPx, RegionAnchorKind, RegionHint, ScreenInfo, SystemControlId,
};

use api::{InputContentBlock, InputMessage, MessageRequest, OutputContentBlock, ProviderClient};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_CLOUD_VISION_MODEL: &str = "glm-4.6v-flash";
const DEFAULT_LOCAL_VISION_MODEL: &str = "showui-2b";
const DEFAULT_LOCAL_VISION_BASE_URL: &str = "http://127.0.0.1:8000/v1";
const DEFAULT_VISION_MAX_TOKENS: u32 = 1024;
const DEFAULT_LOCAL_VISION_TIMEOUT_SECONDS: u64 = 180;
const LOCAL_VLM_RESOURCE_LAUNCHER_HINT: &str =
    "modules/vision/resources/local-vlm/start-local-vlm.ps1";
pub const POINT_ONLY_GROUNDING_CONFIDENCE: f32 = 0.35;
pub const POINT_ONLY_GROUNDING_REASON: &str =
    "point-only-grounding; bbox and model confidence unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionBackendKind {
    ZhipuGlm46VFlash,
    LocalOpenAiCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionBackendKind {
    UiDetr1,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisionImageSource {
    Path(PathBuf),
    Url(String),
    DataUrl(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionRequest {
    pub prompt: String,
    pub images: Vec<VisionImageSource>,
    pub post_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionResponse {
    pub backend: VisionBackendKind,
    pub model: String,
    pub text: String,
    pub request_id: Option<String>,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RelativeBoundingBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionElementKind {
    Button,
    Icon,
    Text,
    Input,
    Menu,
    Window,
    Checkbox,
    Slider,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionElement {
    pub id: String,
    pub kind: DetectionElementKind,
    pub label: Option<String>,
    pub text: Option<String>,
    pub bbox: RelativeBoundingBox,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectionRequest {
    pub image: VisionImageSource,
    pub min_confidence: f32,
    pub max_elements: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectionFrame {
    pub backend: DetectionBackendKind,
    pub model: String,
    pub elements: Vec<DetectionElement>,
    pub elapsed_ms: u64,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpDetectionBackend {
    base_url: String,
    model: String,
    api_key: Option<String>,
    timeout_seconds: u64,
    kind: DetectionBackendKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionGroundingResult {
    pub point: Option<(f32, f32)>,
    pub bbox: Option<RelativeBoundingBox>,
    pub confidence: Option<f32>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionToolCapability {
    Describe,
    Ocr,
    GroundPoint,
    GroundBbox,
    DetectElements,
    RealtimePerception,
    VisualAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionToolCapabilityStatus {
    Available,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionToolCapabilityInfo {
    pub capability: VisionToolCapability,
    pub status: VisionToolCapabilityStatus,
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionToolGroundingResponse {
    pub capability: VisionToolCapability,
    pub grounding: VisionGroundingResult,
    pub degradation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionToolReservedResponse {
    pub capability: VisionToolCapability,
    pub status: VisionToolCapabilityStatus,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionToolVisualActionPlan {
    pub action: String,
    pub target: String,
    pub grounding: VisionGroundingResult,
    pub execute_allowed: bool,
    pub degradation_reason: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct VisionToolService;

pub const SHOWUI_GROUNDING_PROMPT: &str = "Based on the screenshot of the page, I give a text description and you give its corresponding location. The coordinate represents a clickable location [x, y] for an element, which is a relative coordinate on the screenshot, scaled from 0 to 1. The origin is the top-left of the screenshot: x increases to the right, y increases downward. Return only [x, y].";

#[derive(Debug)]
pub enum VisionError {
    Io(std::io::Error),
    UnsupportedImageFormat { path: PathBuf },
    Api(api::ApiError),
    Http(reqwest::Error),
    HttpStatus { status: u16, body: String },
    InvalidResponse(&'static str),
    Runtime(String),
}

impl Display for VisionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::UnsupportedImageFormat { path } => {
                write!(f, "unsupported image format: {}", path.display())
            }
            Self::Api(error) => write!(f, "{error}"),
            Self::Http(error) => write!(f, "{error}"),
            Self::HttpStatus { status, body } => {
                write!(f, "local vision backend returned HTTP {status}: {body}")
            }
            Self::InvalidResponse(message) => write!(f, "{message}"),
            Self::Runtime(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VisionError {}

impl From<std::io::Error> for VisionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<api::ApiError> for VisionError {
    fn from(value: api::ApiError) -> Self {
        Self::Api(value)
    }
}

impl From<reqwest::Error> for VisionError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

pub trait VisionBackend {
    fn kind(&self) -> VisionBackendKind;
    fn analyze(&self, request: &VisionRequest) -> Result<VisionResponse, VisionError>;
}

pub trait DetectionBackend {
    fn kind(&self) -> DetectionBackendKind;
    fn model(&self) -> &str;
    fn detect(&self, request: &DetectionRequest) -> Result<DetectionFrame, VisionError>;
}

#[must_use]
pub fn default_local_vision_model() -> &'static str {
    DEFAULT_LOCAL_VISION_MODEL
}

#[must_use]
pub fn default_local_vision_base_url() -> &'static str {
    DEFAULT_LOCAL_VISION_BASE_URL
}

fn vision_data_dir() -> Option<PathBuf> {
    env::var("COOLZHU_VISION_DATA_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|root| root.join(".coolzhu").join("vision")))
}

#[must_use]
pub fn default_local_vlm_install_root() -> PathBuf {
    env::var("COOLZHU_LOCAL_VLM_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| vision_data_dir().map(|root| root.join("local-vlm")))
        .unwrap_or_else(|| env::temp_dir().join("coolzhu-local-vlm"))
}

#[must_use]
pub fn local_vlm_health_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let service_root = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    format!("{service_root}/health")
}

#[must_use]
pub fn local_vlm_resource_launcher_hint() -> &'static str {
    LOCAL_VLM_RESOURCE_LAUNCHER_HINT
}

#[must_use]
pub fn default_latest_desktop_capture_dir() -> PathBuf {
    env::var("COOLZHU_DESKTOP_CAPTURE_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| vision_data_dir().map(|root| root.join("desktop-capture")))
        .unwrap_or_else(|| env::temp_dir().join("coolzhu-desktop-capture"))
}

#[must_use]
pub fn default_latest_desktop_capture_path() -> PathBuf {
    default_latest_desktop_capture_dir().join("desktop-latest.png")
}

#[derive(Debug, Clone)]
pub struct ZhipuVisionBackend {
    model: String,
}

impl Default for ZhipuVisionBackend {
    fn default() -> Self {
        Self {
            model: DEFAULT_CLOUD_VISION_MODEL.to_string(),
        }
    }
}

impl ZhipuVisionBackend {
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = api::resolve_model_alias(&model.into());
        self
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl VisionBackend for ZhipuVisionBackend {
    fn kind(&self) -> VisionBackendKind {
        VisionBackendKind::ZhipuGlm46VFlash
    }

    fn analyze(&self, request: &VisionRequest) -> Result<VisionResponse, VisionError> {
        maybe_seed_zhipu_key_from_desktop();
        let content = build_multimodal_content(request)?;
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: DEFAULT_VISION_MAX_TOKENS,
            messages: vec![InputMessage {
                role: "user".to_string(),
                content,
            }],
            system: Some(
                "You are the vision backend for Claw. Respond in concise Chinese unless asked otherwise."
                    .to_string(),
            ),
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
            stream: false,
        };
        let client = ProviderClient::from_model(&self.model)?;
        let runtime = tokio::runtime::Runtime::new().map_err(|error| {
            VisionError::Runtime(format!("failed to create vision runtime: {error}"))
        })?;
        let response = runtime.block_on(client.send_message(&message_request))?;
        let text = extract_text(&response.content);

        Ok(VisionResponse {
            backend: self.kind(),
            model: response.model,
            text,
            request_id: response.request_id,
            total_tokens: response.usage.total_tokens(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOpenAiVisionBackend {
    base_url: String,
    model: String,
    api_key: Option<String>,
    timeout_seconds: u64,
}

impl Default for LocalOpenAiVisionBackend {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_LOCAL_VISION_BASE_URL.to_string(),
            model: DEFAULT_LOCAL_VISION_MODEL.to_string(),
            api_key: None,
            timeout_seconds: DEFAULT_LOCAL_VISION_TIMEOUT_SECONDS,
        }
    }
}

impl LocalOpenAiVisionBackend {
    #[must_use]
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::default().with_base_url(base_url).with_model(model)
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let trimmed = base_url.trim();
        self.base_url = if trimmed.is_empty() {
            DEFAULT_LOCAL_VISION_BASE_URL.to_string()
        } else {
            trimmed.trim_end_matches('/').to_string()
        };
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        let model = model.into();
        let trimmed = model.trim();
        self.model = if trimmed.is_empty() {
            DEFAULT_LOCAL_VISION_MODEL.to_string()
        } else {
            trimmed.to_string()
        };
        self
    }

    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        let trimmed = api_key.trim();
        self.api_key = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self
    }

    #[must_use]
    pub const fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn chat_completions_url(&self) -> String {
        chat_completions_url(&self.base_url)
    }
}

impl VisionBackend for LocalOpenAiVisionBackend {
    fn kind(&self) -> VisionBackendKind {
        VisionBackendKind::LocalOpenAiCompatible
    }

    fn analyze(&self, request: &VisionRequest) -> Result<VisionResponse, VisionError> {
        let payload = build_local_openai_payload(&self.model, request)?;
        let url = self.chat_completions_url();
        let timeout = std::time::Duration::from_secs(self.timeout_seconds.max(5));
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let mut http_request = client.post(url).json(&payload);
        if let Some(api_key) = &self.api_key {
            http_request = http_request.bearer_auth(api_key);
        }

        let runtime = tokio::runtime::Runtime::new().map_err(|error| {
            VisionError::Runtime(format!("failed to create local vision runtime: {error}"))
        })?;
        let response = runtime.block_on(async move {
            let response = http_request.send().await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(VisionError::HttpStatus {
                    status: status.as_u16(),
                    body,
                });
            }
            Ok::<_, VisionError>(body)
        })?;

        parse_local_openai_response(&response, self.kind(), &self.model)
    }
}

impl HttpDetectionBackend {
    #[must_use]
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let model = model.into();
        let trimmed_base_url = base_url.trim().trim_end_matches('/');
        let trimmed_model = model.trim();
        Self {
            base_url: trimmed_base_url.to_string(),
            model: if trimmed_model.is_empty() {
                "UI-DETR-1".to_string()
            } else {
                trimmed_model.to_string()
            },
            api_key: None,
            timeout_seconds: DEFAULT_LOCAL_VISION_TIMEOUT_SECONDS,
            kind: DetectionBackendKind::UiDetr1,
        }
    }

    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        let trimmed = api_key.trim();
        self.api_key = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self
    }

    #[must_use]
    pub const fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    #[must_use]
    pub const fn with_kind(mut self, kind: DetectionBackendKind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn endpoint_url(&self) -> String {
        detection_endpoint_url(&self.base_url)
    }
}

impl DetectionBackend for HttpDetectionBackend {
    fn kind(&self) -> DetectionBackendKind {
        self.kind
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn detect(&self, request: &DetectionRequest) -> Result<DetectionFrame, VisionError> {
        let started = std::time::Instant::now();
        let payload = build_detection_http_payload(&self.model, request)?;
        let url = self.endpoint_url();
        let timeout = std::time::Duration::from_secs(self.timeout_seconds.max(5));
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let mut http_request = client.post(url).json(&payload);
        if let Some(api_key) = &self.api_key {
            http_request = http_request.bearer_auth(api_key);
        }

        let runtime = tokio::runtime::Runtime::new().map_err(|error| {
            VisionError::Runtime(format!("failed to create detection runtime: {error}"))
        })?;
        let body = runtime.block_on(async move {
            let response = http_request.send().await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(VisionError::HttpStatus {
                    status: status.as_u16(),
                    body,
                });
            }
            Ok::<_, VisionError>(body)
        })?;
        let elements =
            parse_detection_elements(&body, request.min_confidence, request.max_elements);

        Ok(DetectionFrame {
            backend: self.kind,
            model: self.model.clone(),
            elements,
            elapsed_ms: started.elapsed().as_millis() as u64,
            source: Some(self.endpoint_url()),
        })
    }
}

pub fn analyze_latest_desktop_with_backend(
    backend: &(impl VisionBackend + ?Sized),
    prompt: &str,
) -> Result<VisionResponse, VisionError> {
    let latest_capture = default_latest_desktop_capture_path();
    if !latest_capture.is_file() {
        return Err(VisionError::Runtime(format!(
            "latest desktop capture not found: {}",
            latest_capture.display()
        )));
    }

    backend.analyze(&VisionRequest {
        prompt: prompt.to_string(),
        images: vec![VisionImageSource::Path(latest_capture)],
        post_prompt: None,
    })
}

impl VisionToolService {
    #[must_use]
    pub fn capabilities(&self) -> Vec<VisionToolCapabilityInfo> {
        vec![
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::Describe,
                status: VisionToolCapabilityStatus::Reserved,
                name: "describe",
                description: "Reserved screen description capability; model calls are not part of this round.",
            },
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::Ocr,
                status: VisionToolCapabilityStatus::Reserved,
                name: "ocr",
                description: "Reserved OCR capability; model calls are not part of this round.",
            },
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::GroundPoint,
                status: VisionToolCapabilityStatus::Available,
                name: "ground_point",
                description: "Parse or request a clickable point for a visual target.",
            },
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::GroundBbox,
                status: VisionToolCapabilityStatus::Available,
                name: "ground_bbox",
                description: "Parse bbox grounding when the backend provides one; point-only results stay explicit.",
            },
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::DetectElements,
                status: VisionToolCapabilityStatus::Available,
                name: "detect_elements",
                description: "Parse UI-DETR-style detection boxes into a current-window element table.",
            },
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::RealtimePerception,
                status: VisionToolCapabilityStatus::Reserved,
                name: "realtime_perception",
                description: "Reserved realtime perception loop for continuous UI-DETR + OCR updates.",
            },
            VisionToolCapabilityInfo {
                capability: VisionToolCapability::VisualAction,
                status: VisionToolCapabilityStatus::Available,
                name: "visual_action",
                description: "Build visual grounding evidence for downstream computer-use dry-run planning.",
            },
        ]
    }

    #[must_use]
    pub fn capability_status(
        &self,
        capability: VisionToolCapability,
    ) -> VisionToolCapabilityStatus {
        self.capabilities()
            .into_iter()
            .find(|info| info.capability == capability)
            .map_or(VisionToolCapabilityStatus::Reserved, |info| info.status)
    }

    #[must_use]
    pub fn describe_reserved(&self) -> VisionToolReservedResponse {
        reserved_capability_response(VisionToolCapability::Describe, "describe")
    }

    #[must_use]
    pub fn ocr_reserved(&self) -> VisionToolReservedResponse {
        reserved_capability_response(VisionToolCapability::Ocr, "ocr")
    }

    #[must_use]
    pub fn ground_point_from_text(&self, text: &str) -> Option<VisionToolGroundingResponse> {
        let grounding = parse_grounding_result(text)?;
        let (grounding, degradation_reason) = apply_grounding_confidence_policy(grounding);
        Some(VisionToolGroundingResponse {
            capability: VisionToolCapability::GroundPoint,
            grounding,
            degradation_reason,
        })
    }

    #[must_use]
    pub fn ground_bbox_from_text(&self, text: &str) -> Option<VisionToolGroundingResponse> {
        let grounding = parse_grounding_result(text)?;
        let (grounding, degradation_reason) = apply_grounding_confidence_policy(grounding);
        Some(VisionToolGroundingResponse {
            capability: VisionToolCapability::GroundBbox,
            grounding,
            degradation_reason,
        })
    }

    #[must_use]
    pub fn visual_action_from_grounding_text(
        &self,
        action: impl Into<String>,
        target: impl Into<String>,
        grounding_text: &str,
    ) -> Option<VisionToolVisualActionPlan> {
        let grounding = parse_grounding_result(grounding_text)?;
        let (grounding, degradation_reason) = apply_grounding_confidence_policy(grounding);
        Some(VisionToolVisualActionPlan {
            action: action.into(),
            target: target.into(),
            grounding,
            execute_allowed: false,
            degradation_reason,
        })
    }
}

#[must_use]
pub fn apply_grounding_confidence_policy(
    mut grounding: VisionGroundingResult,
) -> (VisionGroundingResult, Option<String>) {
    if grounding.point.is_some() && grounding.bbox.is_none() && grounding.confidence.is_none() {
        grounding.confidence = Some(POINT_ONLY_GROUNDING_CONFIDENCE);
        return (grounding, Some(POINT_ONLY_GROUNDING_REASON.to_string()));
    }
    (grounding, None)
}

fn reserved_capability_response(
    capability: VisionToolCapability,
    name: &str,
) -> VisionToolReservedResponse {
    VisionToolReservedResponse {
        capability,
        status: VisionToolCapabilityStatus::Reserved,
        message: format!("{name} capability reserved; model calls are disabled for this round"),
    }
}

#[must_use]
pub fn build_showui_grounding_request(
    target_description: impl Into<String>,
    image: VisionImageSource,
) -> VisionRequest {
    VisionRequest {
        prompt: SHOWUI_GROUNDING_PROMPT.to_string(),
        images: vec![image],
        post_prompt: Some(target_description.into()),
    }
}

#[must_use]
pub fn parse_relative_point(text: &str) -> Option<(f32, f32)> {
    let start = text.find('[')?;
    let end = text[start..].find(']')? + start;
    let coordinates = &text[start + 1..end];
    let mut parts = coordinates.split(',');
    let x = parts.next()?.trim().parse::<f32>().ok()?;
    let y = parts.next()?.trim().parse::<f32>().ok()?;
    if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    Some((x, y))
}

#[must_use]
pub fn parse_grounding_result(text: &str) -> Option<VisionGroundingResult> {
    let json_seen = parse_first_json_value(text).is_some();
    let json_result = parse_grounding_json(text);
    if json_result.is_some() || json_seen {
        return json_result;
    }

    parse_relative_point(text).map(|point| VisionGroundingResult {
        point: Some(point),
        bbox: None,
        confidence: None,
        label: None,
    })
}

#[must_use]
pub fn parse_detection_elements(
    text: &str,
    min_confidence: f32,
    max_elements: usize,
) -> Vec<DetectionElement> {
    parse_detection_elements_with_dimensions(text, min_confidence, max_elements, None)
}

#[must_use]
pub fn parse_detection_elements_with_dimensions(
    text: &str,
    min_confidence: f32,
    max_elements: usize,
    dimensions: Option<(u32, u32)>,
) -> Vec<DetectionElement> {
    if max_elements == 0 {
        return Vec::new();
    }

    let Some(value) = parse_first_json_value(text) else {
        return Vec::new();
    };

    let dimensions = dimensions.or_else(|| parse_detection_dimensions(&value));
    let mut candidates = Vec::new();
    collect_detection_candidates(&value, &mut candidates);
    let min_confidence = min_confidence.clamp(0.0, 1.0);
    let mut elements = candidates
        .into_iter()
        .filter_map(|candidate| {
            parse_detection_element_value(candidate, min_confidence, dimensions)
        })
        .take(max_elements)
        .enumerate()
        .map(|(index, mut element)| {
            element.id = format!("det-{:03}", index + 1);
            element
        })
        .collect::<Vec<_>>();
    if elements.is_empty() {
        elements = parse_detection_embedded_text_elements(
            &value,
            min_confidence,
            max_elements,
            dimensions,
        );
    }
    elements
}

#[must_use]
pub fn relative_bbox_to_pixel(
    bbox: RelativeBoundingBox,
    dimensions: (u32, u32),
) -> Option<(i32, i32, i32, i32)> {
    let normalized = normalize_relative_bbox(bbox)?;
    let (left, top) = relative_point_to_pixel((normalized.x1, normalized.y1), dimensions)?;
    let (right, bottom) = relative_point_to_pixel((normalized.x2, normalized.y2), dimensions)?;
    Some((left, top, right, bottom))
}

#[must_use]
pub fn relative_point_to_pixel(point: (f32, f32), dimensions: (u32, u32)) -> Option<(i32, i32)> {
    let (x, y) = point;
    let (width, height) = dimensions;
    if width == 0 || height == 0 || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }

    let pixel_x = (x * (width.saturating_sub(1)) as f32).round() as i32;
    let pixel_y = (y * (height.saturating_sub(1)) as f32).round() as i32;
    Some((pixel_x, pixel_y))
}

fn parse_grounding_json(text: &str) -> Option<VisionGroundingResult> {
    let value = parse_first_json_value(text)?;
    parse_grounding_value(&value)
}

fn collect_detection_candidates<'a>(value: &'a Value, candidates: &mut Vec<&'a Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_detection_candidates(item, candidates);
            }
        }
        Value::Object(object) => {
            let list_keys = [
                "detections",
                "results",
                "items",
                "elements",
                "boxes",
                "candidates",
                "predictions",
            ];
            let mut found_list = false;
            for key in list_keys {
                if let Some(items) = object.get(key).and_then(Value::as_array) {
                    found_list = true;
                    for item in items {
                        collect_detection_candidates(item, candidates);
                    }
                }
            }
            if object.contains_key("bbox")
                || object.contains_key("box")
                || object.contains_key("rect")
                || object.contains_key("bounds")
                || object.contains_key("xyxy")
                || (!found_list && object.contains_key("label"))
            {
                candidates.push(value);
            }
        }
        _ => {}
    }
}

fn parse_detection_element_value(
    value: &Value,
    min_confidence: f32,
    dimensions: Option<(u32, u32)>,
) -> Option<DetectionElement> {
    let bbox = value
        .get("bbox")
        .or_else(|| value.get("box"))
        .or_else(|| value.get("rect"))
        .or_else(|| value.get("bounds"))
        .or_else(|| value.get("xyxy"))
        .and_then(|bbox| parse_detection_bbox_value(bbox, dimensions))?;
    let confidence = parse_detection_confidence(value);
    if confidence.is_some_and(|score| score < min_confidence) {
        return None;
    }
    let label = parse_detection_string(
        value,
        &["label", "class", "category", "type", "name", "target"],
    );
    let text = parse_detection_string(value, &["text", "ocr", "content", "title", "caption"]);
    let kind = detection_kind_from_text(label.as_deref().or(text.as_deref()));

    Some(DetectionElement {
        id: String::new(),
        kind,
        label,
        text,
        bbox,
        confidence,
    })
}

fn parse_detection_dimensions(value: &Value) -> Option<(u32, u32)> {
    let width = value
        .get("image_width")
        .or_else(|| value.get("width"))
        .or_else(|| value.get("w"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())?;
    let height = value
        .get("image_height")
        .or_else(|| value.get("height"))
        .or_else(|| value.get("h"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())?;
    (width > 0 && height > 0).then_some((width, height))
}

fn parse_detection_embedded_text_elements(
    value: &Value,
    min_confidence: f32,
    max_elements: usize,
    dimensions: Option<(u32, u32)>,
) -> Vec<DetectionElement> {
    let mut embedded = Vec::new();
    collect_detection_embedded_text(value, &mut embedded);
    embedded
        .into_iter()
        .flat_map(|text| {
            parse_detection_elements_with_dimensions(text, min_confidence, max_elements, dimensions)
        })
        .take(max_elements)
        .enumerate()
        .map(|(index, mut element)| {
            element.id = format!("det-{:03}", index + 1);
            element
        })
        .collect()
}

fn collect_detection_embedded_text<'a>(value: &'a Value, embedded: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => {
            if text.contains("bbox") || text.contains("box") || text.contains("detections") {
                embedded.push(text);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_detection_embedded_text(item, embedded);
            }
        }
        Value::Object(object) => {
            for item in object.values() {
                collect_detection_embedded_text(item, embedded);
            }
        }
        _ => {}
    }
}

fn parse_detection_bbox_value(
    value: &Value,
    dimensions: Option<(u32, u32)>,
) -> Option<RelativeBoundingBox> {
    let raw = if let Some(array_bbox) = parse_bbox_value(value) {
        array_bbox
    } else {
        parse_detection_bbox_object(value)?
    };

    normalize_relative_bbox(raw).or_else(|| normalize_absolute_bbox(raw, dimensions))
}

fn parse_detection_bbox_object(value: &Value) -> Option<RelativeBoundingBox> {
    let object = value.as_object()?;
    let x1 = number_from_object(object, &["x1", "xmin", "left", "x"])?;
    let y1 = number_from_object(object, &["y1", "ymin", "top", "y"])?;
    if let (Some(x2), Some(y2)) = (
        number_from_object(object, &["x2", "xmax", "right"]),
        number_from_object(object, &["y2", "ymax", "bottom"]),
    ) {
        return Some(RelativeBoundingBox { x1, y1, x2, y2 });
    }
    let width = number_from_object(object, &["width", "w"])?;
    let height = number_from_object(object, &["height", "h"])?;
    Some(RelativeBoundingBox {
        x1,
        y1,
        x2: x1 + width,
        y2: y1 + height,
    })
}

fn number_from_object(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f32> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_f64))
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
}

fn normalize_absolute_bbox(
    bbox: RelativeBoundingBox,
    dimensions: Option<(u32, u32)>,
) -> Option<RelativeBoundingBox> {
    let (width, height) = dimensions?;
    if width == 0 || height == 0 {
        return None;
    }
    normalize_relative_bbox(RelativeBoundingBox {
        x1: bbox.x1 / width as f32,
        y1: bbox.y1 / height as f32,
        x2: bbox.x2 / width as f32,
        y2: bbox.y2 / height as f32,
    })
}

fn parse_detection_confidence(value: &Value) -> Option<f32> {
    ["confidence", "score", "probability", "prob"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(Value::as_f64))
        .map(|score| score as f32)
        .filter(|score| score.is_finite() && (0.0..=1.0).contains(score))
}

fn parse_detection_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn detection_kind_from_text(value: Option<&str>) -> DetectionElementKind {
    let Some(value) = value else {
        return DetectionElementKind::Unknown;
    };
    let value = value.trim().to_ascii_lowercase();
    if value.contains("button") || value.contains("按钮") {
        DetectionElementKind::Button
    } else if value.contains("input") || value.contains("textbox") || value.contains("输入") {
        DetectionElementKind::Input
    } else if value.contains("menu") || value.contains("菜单") {
        DetectionElementKind::Menu
    } else if value.contains("window") || value.contains("dialog") || value.contains("窗口") {
        DetectionElementKind::Window
    } else if value.contains("checkbox") || value.contains("check_box") || value.contains("复选")
    {
        DetectionElementKind::Checkbox
    } else if value.contains("slider") || value.contains("滑块") {
        DetectionElementKind::Slider
    } else if value.contains("icon") || value.contains("图标") {
        DetectionElementKind::Icon
    } else if value.contains("text") || value.contains("label") || value.contains("文本") {
        DetectionElementKind::Text
    } else {
        DetectionElementKind::Unknown
    }
}

fn parse_grounding_value(value: &Value) -> Option<VisionGroundingResult> {
    if let Some(array) = value.as_array() {
        return parse_array_grounding(array);
    }

    let point = value
        .get("point")
        .or_else(|| value.get("center"))
        .or_else(|| value.get("coordinate"))
        .or_else(|| value.get("coordinates"))
        .and_then(parse_point_value);
    let bbox = value
        .get("bbox")
        .or_else(|| value.get("box"))
        .or_else(|| value.get("region"))
        .and_then(parse_bbox_value)
        .and_then(normalize_relative_bbox);
    let confidence = value
        .get("confidence")
        .or_else(|| value.get("score"))
        .and_then(Value::as_f64)
        .map(|score| score as f32)
        .filter(|score| score.is_finite() && (0.0..=1.0).contains(score));
    let label = value
        .get("label")
        .or_else(|| value.get("target"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string);

    if point.is_none() && bbox.is_none() {
        return parse_candidate_grounding(value);
    }

    Some(VisionGroundingResult {
        point: point.or_else(|| bbox.map(bbox_center)),
        bbox,
        confidence,
        label,
    })
}

fn parse_candidate_grounding(value: &Value) -> Option<VisionGroundingResult> {
    let candidates = value
        .get("candidates")
        .or_else(|| value.get("results"))
        .or_else(|| value.get("detections"))
        .or_else(|| value.get("items"))
        .and_then(Value::as_array)?;

    candidates
        .iter()
        .filter_map(parse_grounding_value)
        .max_by(|left, right| {
            grounding_rank(left)
                .partial_cmp(&grounding_rank(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn grounding_rank(grounding: &VisionGroundingResult) -> f32 {
    grounding.confidence.unwrap_or_else(|| {
        if grounding.bbox.is_some() {
            0.5
        } else if grounding.point.is_some() {
            POINT_ONLY_GROUNDING_CONFIDENCE
        } else {
            0.0
        }
    })
}

fn parse_first_json_value(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }

    for (start_index, start_char) in text.char_indices() {
        if start_char != '{' && start_char != '[' {
            continue;
        }
        let end_char = if start_char == '{' { '}' } else { ']' };
        for (end_index, end_candidate) in text.char_indices().rev() {
            if end_index <= start_index || end_candidate != end_char {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&text[start_index..=end_index]) {
                return Some(value);
            }
        }
    }
    None
}

fn parse_array_grounding(array: &[Value]) -> Option<VisionGroundingResult> {
    match array.len() {
        2 => parse_point_value(&Value::Array(array.to_vec())).map(|point| VisionGroundingResult {
            point: Some(point),
            bbox: None,
            confidence: None,
            label: None,
        }),
        4 => parse_bbox_value(&Value::Array(array.to_vec()))
            .and_then(normalize_relative_bbox)
            .map(|bbox| VisionGroundingResult {
                point: Some(bbox_center(bbox)),
                bbox: Some(bbox),
                confidence: None,
                label: None,
            }),
        _ => None,
    }
}

fn parse_point_value(value: &Value) -> Option<(f32, f32)> {
    let array = value.as_array()?;
    if array.len() < 2 {
        return None;
    }
    let x = array.first()?.as_f64()? as f32;
    let y = array.get(1)?.as_f64()? as f32;
    if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    Some((x, y))
}

fn parse_bbox_value(value: &Value) -> Option<RelativeBoundingBox> {
    let array = value.as_array()?;
    if array.len() < 4 {
        return None;
    }
    let x1 = array.first()?.as_f64()? as f32;
    let y1 = array.get(1)?.as_f64()? as f32;
    let x2 = array.get(2)?.as_f64()? as f32;
    let y2 = array.get(3)?.as_f64()? as f32;
    Some(RelativeBoundingBox { x1, y1, x2, y2 })
}

fn normalize_relative_bbox(bbox: RelativeBoundingBox) -> Option<RelativeBoundingBox> {
    if !bbox.x1.is_finite() || !bbox.y1.is_finite() || !bbox.x2.is_finite() || !bbox.y2.is_finite()
    {
        return None;
    }
    let x1 = bbox.x1.min(bbox.x2);
    let x2 = bbox.x1.max(bbox.x2);
    let y1 = bbox.y1.min(bbox.y2);
    let y2 = bbox.y1.max(bbox.y2);
    if !(0.0..=1.0).contains(&x1)
        || !(0.0..=1.0).contains(&x2)
        || !(0.0..=1.0).contains(&y1)
        || !(0.0..=1.0).contains(&y2)
        || x1 == x2
        || y1 == y2
    {
        return None;
    }
    Some(RelativeBoundingBox { x1, y1, x2, y2 })
}

fn bbox_center(bbox: RelativeBoundingBox) -> (f32, f32) {
    ((bbox.x1 + bbox.x2) / 2.0, (bbox.y1 + bbox.y2) / 2.0)
}

pub fn build_local_openai_payload(
    model: &str,
    request: &VisionRequest,
) -> Result<Value, VisionError> {
    let mut content = vec![serde_json::json!({
        "type": "text",
        "text": request.prompt,
    })];

    for image in &request.images {
        content.push(serde_json::json!({
            "type": "image_url",
            "image_url": {
                "url": image_source_to_data_url(image)?,
                "detail": "high",
            },
        }));
    }
    if let Some(post_prompt) = request
        .post_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        content.push(serde_json::json!({
            "type": "text",
            "text": post_prompt,
        }));
    }

    Ok(serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": content,
            }
        ],
        "max_tokens": DEFAULT_VISION_MAX_TOKENS,
        "stream": false,
    }))
}

fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

#[must_use]
pub fn detection_endpoint_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/detect") || trimmed.ends_with("/detections") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/detect")
    }
}

pub fn build_detection_http_payload(
    model: &str,
    request: &DetectionRequest,
) -> Result<Value, VisionError> {
    Ok(serde_json::json!({
        "model": model,
        "image": image_source_to_data_url(&request.image)?,
        "min_confidence": request.min_confidence,
        "max_elements": request.max_elements,
    }))
}

fn parse_local_openai_response(
    body: &str,
    backend: VisionBackendKind,
    fallback_model: &str,
) -> Result<VisionResponse, VisionError> {
    let value = serde_json::from_str::<Value>(body)
        .map_err(|_| VisionError::InvalidResponse("local vision response is not valid JSON"))?;
    let first_choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or(VisionError::InvalidResponse(
            "local vision response has no choices",
        ))?;
    let content = first_choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(extract_openai_content_text)
        .ok_or(VisionError::InvalidResponse(
            "local vision response has no message content",
        ))?;

    let model = value
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_model)
        .to_string();
    let request_id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let total_tokens = value
        .get("usage")
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())
        .unwrap_or(0);

    Ok(VisionResponse {
        backend,
        model,
        text: content,
        request_id,
        total_tokens,
    })
}

fn extract_openai_content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }

    let text = content
        .as_array()?
        .iter()
        .filter_map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| block.get("content").and_then(Value::as_str))
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

pub fn build_multimodal_content(
    request: &VisionRequest,
) -> Result<Vec<InputContentBlock>, VisionError> {
    let mut content = vec![InputContentBlock::Text {
        text: request.prompt.clone(),
    }];
    for image in &request.images {
        content.push(InputContentBlock::ImageUrl {
            url: image_source_to_data_url(image)?,
            detail: Some("high".to_string()),
        });
    }
    if let Some(post_prompt) = request
        .post_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        content.push(InputContentBlock::Text {
            text: post_prompt.to_string(),
        });
    }
    Ok(content)
}

fn image_source_to_data_url(image: &VisionImageSource) -> Result<String, VisionError> {
    match image {
        VisionImageSource::Path(path) => image_path_to_data_url(path),
        VisionImageSource::Url(url) | VisionImageSource::DataUrl(url) => Ok(url.clone()),
    }
}

fn image_path_to_data_url(path: &Path) -> Result<String, VisionError> {
    let media_type = media_type_for_path(path)?;
    let bytes = fs::read(path)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{media_type};base64,{encoded}"))
}

fn media_type_for_path(path: &Path) -> Result<&'static str, VisionError> {
    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
        return Err(VisionError::UnsupportedImageFormat {
            path: path.to_path_buf(),
        });
    };
    match extension.to_ascii_lowercase().as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        _ => Err(VisionError::UnsupportedImageFormat {
            path: path.to_path_buf(),
        }),
    }
}

fn extract_text(blocks: &[OutputContentBlock]) -> String {
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            OutputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        "模型未返回文本结果".to_string()
    } else {
        text
    }
}

fn maybe_seed_zhipu_key_from_desktop() {
    if has_non_empty_env("ZAI_API_KEY")
        || has_non_empty_env("BIGMODEL_API_KEY")
        || has_non_empty_env("OPENAI_API_KEY")
    {
        return;
    }

    let key_paths = [
        env::var("COOLZHU_API_KEY_PATH").ok().map(PathBuf::from),
        home_dir().map(|h| h.join("api-key.txt")),
    ];
    for path in key_paths.into_iter().flatten() {
        if let Ok(contents) = fs::read_to_string(&path) {
            let api_key = contents.trim();
            if !api_key.is_empty() {
                env::set_var("ZAI_API_KEY", api_key);
                return;
            }
        }
    }
}

fn has_non_empty_env(key: &str) -> bool {
    env::var(key).is_ok_and(|value| !value.trim().is_empty())
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{
        build_local_openai_payload, build_multimodal_content, build_showui_grounding_request,
        chat_completions_url, default_latest_desktop_capture_path, detection_endpoint_url,
        local_vlm_health_url, local_vlm_resource_launcher_hint, parse_detection_elements,
        parse_detection_elements_with_dimensions, parse_grounding_result,
        parse_local_openai_response, parse_relative_point, relative_bbox_to_pixel,
        relative_point_to_pixel, DetectionElementKind, LocalOpenAiVisionBackend,
        RelativeBoundingBox, VisionBackendKind, VisionImageSource, VisionRequest,
        VisionToolCapability, VisionToolCapabilityStatus, VisionToolService, ZhipuVisionBackend,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("vision-{nanos}-{name}"))
    }

    #[test]
    fn builds_multimodal_content_with_high_detail_image_blocks() {
        let content = build_multimodal_content(&VisionRequest {
            prompt: "请描述图片".to_string(),
            images: vec![VisionImageSource::DataUrl(
                "data:image/png;base64,AAA".to_string(),
            )],
            post_prompt: None,
        })
        .expect("multimodal request should build");

        assert_eq!(content.len(), 2);
        assert!(
            matches!(&content[0], api::InputContentBlock::Text { text } if text == "请描述图片")
        );
        assert!(matches!(
            &content[1],
            api::InputContentBlock::ImageUrl { url, detail }
                if url == "data:image/png;base64,AAA" && detail.as_deref() == Some("high")
        ));
    }

    #[test]
    fn converts_local_png_into_data_url() {
        let path = temp_file("pixel.png");
        fs::write(&path, [137, 80, 78, 71]).expect("png bytes should write");
        let content = build_multimodal_content(&VisionRequest {
            prompt: "图里是什么".to_string(),
            images: vec![VisionImageSource::Path(path.clone())],
            post_prompt: None,
        })
        .expect("local png should convert");

        let api::InputContentBlock::ImageUrl { url, .. } = &content[1] else {
            panic!("expected image url block");
        };
        assert!(url.starts_with("data:image/png;base64,"));

        fs::remove_file(path).expect("temp file cleanup");
    }

    #[test]
    fn local_backend_normalizes_chat_completions_url() {
        assert_eq!(
            chat_completions_url("http://127.0.0.1:8000/v1"),
            "http://127.0.0.1:8000/v1/chat/completions"
        );
        assert_eq!(
            LocalOpenAiVisionBackend::new("http://127.0.0.1:1234", "showui-2b")
                .chat_completions_url(),
            "http://127.0.0.1:1234/v1/chat/completions"
        );
    }

    #[test]
    fn builds_local_openai_multimodal_payload() {
        let payload = build_local_openai_payload(
            "showui-2b",
            &VisionRequest {
                prompt: "find the Settings icon".to_string(),
                images: vec![VisionImageSource::DataUrl(
                    "data:image/png;base64,AAA".to_string(),
                )],
                post_prompt: None,
            },
        )
        .expect("payload should build");

        assert_eq!(payload["model"], "showui-2b");
        assert_eq!(payload["messages"][0]["content"][0]["type"], "text");
        assert_eq!(payload["messages"][0]["content"][1]["type"], "image_url");
        assert_eq!(
            payload["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AAA"
        );
    }

    #[test]
    fn parses_local_openai_text_response() {
        let response = parse_local_openai_response(
            r#"{"id":"chatcmpl-local","model":"showui-2b","choices":[{"message":{"content":"按钮在左上角。"}}],"usage":{"total_tokens":17}}"#,
            VisionBackendKind::LocalOpenAiCompatible,
            "fallback-model",
        )
        .expect("response should parse");

        assert_eq!(response.backend, VisionBackendKind::LocalOpenAiCompatible);
        assert_eq!(response.model, "showui-2b");
        assert_eq!(response.text, "按钮在左上角。");
        assert_eq!(response.request_id.as_deref(), Some("chatcmpl-local"));
        assert_eq!(response.total_tokens, 17);
    }

    #[test]
    fn builds_showui_grounding_payload_with_target_after_image() {
        let request = build_showui_grounding_request(
            "the blue Windows Start button",
            VisionImageSource::DataUrl("data:image/png;base64,AAA".to_string()),
        );
        let payload =
            build_local_openai_payload("showui-2b", &request).expect("payload should build");
        let content = payload["messages"][0]["content"]
            .as_array()
            .expect("content should be an array");

        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[2]["type"], "text");
        assert_eq!(content[2]["text"], "the blue Windows Start button");
    }

    #[test]
    fn parses_and_maps_relative_points() {
        assert_eq!(
            parse_relative_point("answer: [0.25, 0.75]"),
            Some((0.25, 0.75))
        );
        assert_eq!(
            relative_point_to_pixel((0.25, 0.75), (1001, 801)),
            Some((250, 600))
        );
        assert_eq!(relative_point_to_pixel((1.2, 0.1), (100, 100)), None);
    }

    #[test]
    fn parses_json_grounding_point() {
        let result =
            parse_grounding_result(r#"{"point":[0.42,0.58],"confidence":0.91,"label":"confirm"}"#)
                .expect("grounding JSON should parse");

        assert_eq!(result.point, Some((0.42, 0.58)));
        assert_eq!(result.confidence, Some(0.91));
        assert_eq!(result.label.as_deref(), Some("confirm"));
    }

    #[test]
    fn parses_json_grounding_bbox_and_center() {
        let result = parse_grounding_result(r#"{"bbox":[0.7,0.8,0.3,0.2],"score":0.8}"#)
            .expect("bbox JSON should parse");

        assert_eq!(
            result.bbox,
            Some(RelativeBoundingBox {
                x1: 0.3,
                y1: 0.2,
                x2: 0.7,
                y2: 0.8,
            })
        );
        assert_eq!(result.point, Some((0.5, 0.5)));
        assert_eq!(
            relative_bbox_to_pixel(result.bbox.expect("bbox"), (1001, 801)),
            Some((300, 160, 700, 640))
        );
    }

    #[test]
    fn parses_grounding_candidates_by_highest_confidence() {
        let result = parse_grounding_result(
            r#"{
                "candidates": [
                    {"point":[0.1,0.2],"confidence":0.42,"label":"wrong"},
                    {"bbox":[0.4,0.3,0.6,0.5],"confidence":0.91,"label":"confirm"}
                ]
            }"#,
        )
        .expect("highest-confidence candidate should parse");

        assert_eq!(result.label.as_deref(), Some("confirm"));
        assert_eq!(result.confidence, Some(0.91));
        assert_eq!(result.point, Some((0.5, 0.4)));
        assert_eq!(
            result.bbox,
            Some(RelativeBoundingBox {
                x1: 0.4,
                y1: 0.3,
                x2: 0.6,
                y2: 0.5,
            })
        );
    }

    #[test]
    fn parses_uidetr_detection_candidates_into_element_table() {
        let elements = parse_detection_elements(
            r#"{
                "detections": [
                    {"bbox":[0.10,0.20,0.30,0.40],"label":"button","text":"OK","score":0.91},
                    {"bbox":[0.50,0.55,0.90,0.75],"class":"text","ocr":"Cancel","confidence":0.64},
                    {"bbox":[1.20,0.00,1.40,0.20],"label":"bad","score":0.99}
                ]
            }"#,
            0.50,
            20,
        );

        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].kind, DetectionElementKind::Button);
        assert_eq!(elements[0].label.as_deref(), Some("button"));
        assert_eq!(elements[0].text.as_deref(), Some("OK"));
        assert_eq!(elements[0].confidence, Some(0.91));
        assert_eq!(
            elements[0].bbox,
            RelativeBoundingBox {
                x1: 0.10,
                y1: 0.20,
                x2: 0.30,
                y2: 0.40,
            }
        );
        assert_eq!(elements[1].kind, DetectionElementKind::Text);
        assert_eq!(elements[1].text.as_deref(), Some("Cancel"));
    }

    #[test]
    fn parses_huggingface_uidetr_absolute_boxes_with_screen_size() {
        let elements = parse_detection_elements_with_dimensions(
            r#"{
                "image_width": 1000,
                "image_height": 800,
                "predictions": [
                    {"box":{"xmin":100,"ymin":160,"xmax":300,"ymax":320},"label":"button","score":0.88},
                    {"box":{"x":400,"y":200,"width":100,"height":80},"label":"icon","score":0.77}
                ]
            }"#,
            0.50,
            10,
            Some((1000, 800)),
        );

        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].kind, DetectionElementKind::Button);
        assert_eq!(
            elements[0].bbox,
            RelativeBoundingBox {
                x1: 0.10,
                y1: 0.20,
                x2: 0.30,
                y2: 0.40,
            }
        );
        assert_eq!(elements[1].kind, DetectionElementKind::Icon);
        assert_eq!(
            elements[1].bbox,
            RelativeBoundingBox {
                x1: 0.40,
                y1: 0.25,
                x2: 0.50,
                y2: 0.35,
            }
        );
    }

    #[test]
    fn detection_backend_url_maps_base_url_to_detect_endpoint() {
        assert_eq!(
            detection_endpoint_url("http://127.0.0.1:7860"),
            "http://127.0.0.1:7860/detect"
        );
        assert_eq!(
            detection_endpoint_url("http://127.0.0.1:7860/v1"),
            "http://127.0.0.1:7860/v1/detect"
        );
        assert_eq!(
            detection_endpoint_url("http://127.0.0.1:7860/detect"),
            "http://127.0.0.1:7860/detect"
        );
    }

    #[test]
    fn grounding_parser_keeps_showui_point_compatibility() {
        let result =
            parse_grounding_result("The clickable location is [0.25, 0.75].").expect("point");

        assert_eq!(result.point, Some((0.25, 0.75)));
        assert!(result.bbox.is_none());
    }

    #[test]
    fn rejects_out_of_range_grounding_json() {
        assert!(parse_grounding_result(r#"{"point":[1.2,0.2]}"#).is_none());
        assert!(parse_grounding_result(r#"{"bbox":[0.2,0.2,0.2,0.5]}"#).is_none());
    }

    #[test]
    fn zhipu_backend_defaults_to_glm_46v_flash() {
        let backend = ZhipuVisionBackend::default();
        assert_eq!(backend.model(), "glm-4.6v-flash");
        assert_eq!(
            ZhipuVisionBackend::default()
                .with_model("glm-vision-free")
                .model(),
            "glm-4.6v-flash"
        );
    }

    #[test]
    fn latest_desktop_capture_path_ends_with_fixed_filename() {
        assert!(default_latest_desktop_capture_path()
            .display()
            .to_string()
            .ends_with("desktop-latest.png"));
    }

    #[test]
    fn local_vlm_health_url_maps_openai_compatible_base_url() {
        assert_eq!(
            local_vlm_health_url("http://127.0.0.1:8001/v1"),
            "http://127.0.0.1:8001/health"
        );
        assert_eq!(
            local_vlm_health_url("http://127.0.0.1:8001/v1/"),
            "http://127.0.0.1:8001/health"
        );
        assert_eq!(
            local_vlm_health_url("http://127.0.0.1:8001"),
            "http://127.0.0.1:8001/health"
        );
    }

    #[test]
    fn local_vlm_resource_launcher_is_present_in_repo() {
        let resource_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("resources")
            .join("local-vlm");
        assert!(resource_dir.join("start-local-vlm.ps1").exists());
        assert!(resource_dir.join("check-local-vlm.ps1").exists());
        assert!(resource_dir
            .join("coolzhu-local-vlm.template.json")
            .exists());
        assert!(local_vlm_resource_launcher_hint().ends_with("start-local-vlm.ps1"));
    }

    #[test]
    fn vision_tool_service_lists_reserved_and_active_capabilities() {
        let service = VisionToolService::default();
        let capabilities = service.capabilities();

        assert_eq!(
            service.capability_status(VisionToolCapability::Describe),
            VisionToolCapabilityStatus::Reserved
        );
        assert_eq!(
            service.capability_status(VisionToolCapability::Ocr),
            VisionToolCapabilityStatus::Reserved
        );
        assert!(capabilities.iter().any(|capability| capability.capability
            == VisionToolCapability::GroundPoint
            && capability.status == VisionToolCapabilityStatus::Available));
        assert!(capabilities.iter().any(|capability| capability.capability
            == VisionToolCapability::VisualAction
            && capability.status == VisionToolCapabilityStatus::Available));
        assert!(capabilities.iter().any(|capability| capability.capability
            == VisionToolCapability::DetectElements
            && capability.status == VisionToolCapabilityStatus::Available));
        assert!(capabilities.iter().any(|capability| capability.capability
            == VisionToolCapability::RealtimePerception
            && capability.status == VisionToolCapabilityStatus::Reserved));
    }

    #[test]
    fn vision_tool_service_degrades_showui_point_only_confidence() {
        let service = VisionToolService::default();
        let result = service
            .ground_point_from_text("[0.25, 0.75]")
            .expect("ShowUI point should parse");

        assert_eq!(result.capability, VisionToolCapability::GroundPoint);
        assert_eq!(result.grounding.point, Some((0.25, 0.75)));
        assert!(result.grounding.bbox.is_none());
        assert_eq!(result.grounding.confidence, Some(0.35));
        assert_eq!(
            result.degradation_reason.as_deref(),
            Some("point-only-grounding; bbox and model confidence unavailable")
        );
    }

    #[test]
    fn vision_tool_service_reserves_describe_and_ocr_without_model_calls() {
        let service = VisionToolService::default();

        let describe = service.describe_reserved();
        let ocr = service.ocr_reserved();

        assert_eq!(describe.capability, VisionToolCapability::Describe);
        assert_eq!(ocr.capability, VisionToolCapability::Ocr);
        assert_eq!(describe.status, VisionToolCapabilityStatus::Reserved);
        assert_eq!(ocr.status, VisionToolCapabilityStatus::Reserved);
        assert!(describe.message.contains("reserved"));
        assert!(ocr.message.contains("reserved"));
    }
}
