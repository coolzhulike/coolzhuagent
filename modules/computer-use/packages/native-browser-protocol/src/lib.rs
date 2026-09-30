//! 原生浏览器宿主的资源登记契约；独立于 Chrome tab 协议，不携带网页权限。
use serde::{Deserialize, Serialize};

pub const STATE_PATH: &str = "/api/native-browser/host-state";
pub const OBSERVATION_PATH: &str = "/api/native-browser/host-observation";
pub const MAX_OBSERVATION_BYTES: usize = 262_144;
pub const TOKEN_FILE: &str = "native-browser-host-token";
pub const MAX_STATE_BYTES: usize = 4096;
pub const LEASE_MILLIS: u64 = 3000;
pub const ACTIVITY_PATH: &str = "/api/native-computer-use/activity";
pub const ACTIVITY_LEASE_MILLIS: u64 = 1800;
pub const MAX_ACTIVITY_BYTES: usize = 1024;

/// 仅展示当前真实执行活动，不包含模型内容、权限或目标达成声明。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerUseActivityReceipt {
    pub active: bool,
    pub lease_ms: u64,
}

impl ComputerUseActivityReceipt {
    pub fn valid_shape(&self) -> bool {
        if self.active { (1..=ACTIVITY_LEASE_MILLIS).contains(&self.lease_ms) }
        else { self.lease_ms == 0 }
    }
}

pub fn token_filename(port: u16) -> String {
    format!("{TOKEN_FILE}-{port}")
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PanelResource {
    // 仅为待核验的来源声明；后台必须按真实工程与数据库关系解析。
    pub workspace_path: String,
    pub room_id: String,
    pub label: String,
    pub generation: u64,
    pub navigation_revision: u64,
}

impl PanelResource {
    pub fn valid_shape(&self) -> bool {
        !self.workspace_path.is_empty()
            && self.workspace_path.len() <= 2048
            && !self.workspace_path.chars().any(char::is_control)
            && !self.room_id.is_empty()
            && self.room_id.len() <= 200
            && !self.room_id.chars().any(char::is_control)
            && self.generation > 0
            && self.label == format!("browser-panel-{}", self.generation)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostState {
    pub host_id: String,
    pub sequence: u64,
    // 隐藏、关闭、切换环境或加载时为 None，不能沿用旧资源。
    pub resource: Option<PanelResource>,
}

impl HostState {
    pub fn valid_shape(&self) -> bool {
        (16..=96).contains(&self.host_id.len())
            && self.host_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && self.sequence > 0
            && self.resource.as_ref().is_none_or(PanelResource::valid_shape)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostReceipt {
    pub accepted: bool,
    pub resource_registered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<ObservationRequest>,
}

/// 只有后台创建请求；网页、模型不能提供 CDP 方法或脚本。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRequest {
    pub request_id: String,
    pub resource: PanelResource,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedNode {
    pub role: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PageObservation {
    pub url: String,
    pub title: String,
    pub nodes: Vec<ObservedNode>,
    pub truncated: bool,
}

impl PageObservation {
    pub fn valid_shape(&self) -> bool {
        self.url.len() <= 4096 && (self.url.starts_with("http://") || self.url.starts_with("https://"))
            && !self.url.chars().any(char::is_control) && self.title.chars().count() <= 256
            && !self.title.chars().any(char::is_control)
            && self.nodes.len() <= 128 && self.nodes.iter().all(|node|
            node.role.chars().count() <= 64 && node.name.chars().count() <= 256
            && !node.role.chars().any(char::is_control) && !node.name.chars().any(char::is_control))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationReply {
    pub host_id: String,
    pub request_id: String,
    pub resource: PanelResource,
    // 失败仅用宿主定义的错误码，禁止夹带页面原文或未裁剪的 CDP 响应。
    pub observation: Option<PageObservation>,
    pub error: Option<String>,
}
