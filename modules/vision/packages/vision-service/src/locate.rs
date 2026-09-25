use serde::{Deserialize, Serialize};

// ── Request ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocateRequest {
    pub target: LocateTarget,
    #[serde(default)]
    pub backends: Option<Vec<BackendId>>,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
    #[serde(default)]
    pub cross_verify: bool,
    #[serde(default)]
    pub region_hint: Option<RegionHint>,
    #[serde(default)]
    pub capture_path: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub diagnostics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LocateTarget {
    #[serde(rename = "natural")]
    Natural { text: String },
    #[serde(rename = "system")]
    System { id: SystemControlId },
    #[serde(rename = "uia")]
    Uia {
        #[serde(default)]
        process_id: Option<u32>,
        #[serde(default)]
        window_name: Option<String>,
        #[serde(default)]
        automation_id: Option<String>,
        #[serde(default)]
        class_name: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        control_type: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SystemControlId {
    StartButton,
    TaskbarSearchBox,
    TaskView,
    Taskbar,
    SystemTray,
    NotificationCenter,
    DesktopPeek,
    TaskbarClock,
    WindowMinimizeButton,
    WindowMaximizeButton,
    WindowCloseButton,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegionHint {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub anchor_kind: Option<RegionAnchorKind>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RegionAnchorKind {
    TaskbarBottom,
    TaskbarLeft,
    TaskbarRight,
    TaskbarTop,
    SystemTray,
    ActiveWindowTitleBar,
    ActiveWindowClientArea,
    DesktopFull,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BackendId {
    #[serde(alias = "ui_automation")]
    Uia,
    #[serde(alias = "ocr_template", alias = "ocr", alias = "template")]
    OcrTemplate,
    #[serde(alias = "local_vlm", alias = "local", alias = "showui")]
    LocalVlm,
    #[serde(alias = "remote_vlm", alias = "remote")]
    RemoteVlm,
}

impl BackendId {
    /// 推荐的无 ShowUI 定位顺序：结构化 UIA 优先，其次是无需模型的模板，
    /// 最后才调用本地/远程视觉模型。
    #[must_use]
    pub const fn recommended_priority(self) -> u8 {
        match self {
            Self::Uia => 10,
            Self::OcrTemplate => 20,
            Self::LocalVlm => 30,
            Self::RemoteVlm => 40,
        }
    }

    #[must_use]
    pub const fn requires_model(self) -> bool {
        matches!(self, Self::LocalVlm | Self::RemoteVlm)
    }
}

fn default_min_confidence() -> f32 {
    0.55
}
fn default_timeout_ms() -> u64 {
    15_000
}

// ── Response ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocateResponse {
    pub status: LocateStatus,
    pub target_description: String,
    pub point: Option<PointPx>,
    pub bbox: Option<BBoxPx>,
    pub confidence: f32,
    pub chosen_backend: Option<BackendId>,
    pub attempts: Vec<BackendAttempt>,
    pub capture_path: String,
    pub screen: ScreenInfo,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub degradation_reason: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LocateStatus {
    Ok,
    LowConfidence,
    NotFound,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendAttempt {
    pub backend: BackendId,
    pub status: AttemptStatus,
    #[serde(default)]
    pub point: Option<PointPx>,
    #[serde(default)]
    pub bbox: Option<BBoxPx>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub raw_response: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
}

impl BackendAttempt {
    pub fn skipped(backend: BackendId, reason: &str) -> Self {
        Self {
            backend,
            status: AttemptStatus::Skipped,
            point: None,
            bbox: None,
            confidence: None,
            raw_response: None,
            model: None,
            base_url: None,
            elapsed_ms: 0,
            error: Some(reason.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AttemptStatus {
    Ok,
    Skipped,
    Failed,
    LowConfidence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PointPx {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BBoxPx {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ScreenInfo {
    pub logical_width: u32,
    pub logical_height: u32,
    pub dpi_scale: f32,
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_request_natural_serde_roundtrip() {
        let json = r#"{"target":{"kind":"natural","text":"Windows start button"},"min_confidence":0.6,"cross_verify":true,"diagnostics":true}"#;
        let req: LocateRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.min_confidence, 0.6);
        assert!(req.cross_verify);
        assert_eq!(req.timeout_ms, 15_000);
        let back = serde_json::to_string(&req).expect("serialize");
        assert!(back.contains("\"natural\""));
    }

    #[test]
    fn locate_request_system_serde() {
        let json = r#"{"target":{"kind":"system","id":"start-button"},"backends":["uia"]}"#;
        let req: LocateRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.backends, Some(vec![BackendId::Uia]));
        assert_eq!(req.min_confidence, 0.55);
        assert!(!req.diagnostics);
    }

    #[test]
    fn locate_request_uia_serde() {
        let json = r#"{"target":{"kind":"uia","process_id":1200,"window_name":"记事本","automation_id":"SearchApp","class_name":"TextBox"},"min_confidence":0.7}"#;
        let req: LocateRequest = serde_json::from_str(json).expect("parse");
        match &req.target {
            LocateTarget::Uia {
                process_id,
                window_name,
                automation_id,
                class_name,
                ..
            } => {
                assert_eq!(*process_id, Some(1200));
                assert_eq!(window_name.as_deref(), Some("记事本"));
                assert_eq!(automation_id.as_deref(), Some("SearchApp"));
                assert_eq!(class_name.as_deref(), Some("TextBox"));
            }
            _ => panic!("expected Uia target"),
        }
    }

    #[test]
    fn locate_response_full_serde_roundtrip() {
        let resp = LocateResponse {
            status: LocateStatus::Ok,
            target_description: "start-button".to_string(),
            point: Some(PointPx { x: 12, y: 1432 }),
            bbox: Some(BBoxPx {
                x: 0,
                y: 1412,
                width: 48,
                height: 48,
            }),
            confidence: 0.99,
            chosen_backend: Some(BackendId::Uia),
            attempts: vec![BackendAttempt {
                backend: BackendId::Uia,
                status: AttemptStatus::Ok,
                point: Some(PointPx { x: 12, y: 1432 }),
                bbox: Some(BBoxPx {
                    x: 0,
                    y: 1412,
                    width: 48,
                    height: 48,
                }),
                confidence: Some(0.99),
                raw_response: None,
                model: Some("windows-uiautomation".to_string()),
                base_url: None,
                elapsed_ms: 18,
                error: None,
            }],
            capture_path: "/tmp/cap.png".to_string(),
            screen: ScreenInfo {
                logical_width: 1707,
                logical_height: 960,
                dpi_scale: 1.5,
            },
            elapsed_ms: 62,
            degradation_reason: None,
            notes: vec!["uia-resolved".to_string()],
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        let back: LocateResponse = serde_json::from_str(&json).expect("parse");
        assert_eq!(back.status, LocateStatus::Ok);
        assert_eq!(back.point.unwrap().x, 12);
    }

    #[test]
    fn locate_response_not_found() {
        let json = r#"{"status":"not-found","target_description":"xyz","confidence":0.0,"attempts":[],"capture_path":"/tmp/x.png","screen":{"logical_width":1920,"logical_height":1080,"dpi_scale":1.0},"elapsed_ms":500}"#;
        let resp: LocateResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.status, LocateStatus::NotFound);
        assert!(resp.point.is_none());
    }

    #[test]
    fn system_control_id_serde() {
        let ids = vec![
            ("\"start-button\"", SystemControlId::StartButton),
            ("\"taskbar\"", SystemControlId::Taskbar),
            ("\"system-tray\"", SystemControlId::SystemTray),
            (
                "\"window-close-button\"",
                SystemControlId::WindowCloseButton,
            ),
        ];
        for (json, expected) in &ids {
            let id: SystemControlId = serde_json::from_str(json).expect(json);
            assert_eq!(id, *expected, "mismatch for {json}");
        }
    }

    #[test]
    fn region_hint_serde() {
        let hint = RegionHint {
            x: 0.0,
            y: 0.9,
            width: 1.0,
            height: 0.1,
            anchor_kind: Some(RegionAnchorKind::TaskbarBottom),
        };
        let json = serde_json::to_string(&hint).expect("serialize");
        assert!(json.contains("taskbar-bottom"));
        let back: RegionHint = serde_json::from_str(&json).expect("parse");
        assert_eq!(back.anchor_kind, Some(RegionAnchorKind::TaskbarBottom));
    }

    #[test]
    fn backend_attempt_skipped_constructor() {
        let a = BackendAttempt::skipped(BackendId::LocalVlm, "model not loaded");
        assert_eq!(a.status, AttemptStatus::Skipped);
        assert_eq!(a.error.as_deref(), Some("model not loaded"));
    }

    #[test]
    fn ocr_template_backend_is_model_free_and_ordered_before_vlm() {
        assert!(!BackendId::OcrTemplate.requires_model());
        assert!(
            BackendId::OcrTemplate.recommended_priority()
                < BackendId::LocalVlm.recommended_priority()
        );
        let json = serde_json::to_string(&BackendId::OcrTemplate).expect("serialize");
        assert_eq!(json, "\"ocr-template\"");
    }
}
