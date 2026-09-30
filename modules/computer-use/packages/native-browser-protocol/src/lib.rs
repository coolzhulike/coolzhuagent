//! 原生浏览器宿主的资源登记契约；独立于 Chrome tab 协议，不携带网页权限。
use serde::{Deserialize, Serialize};

pub const STATE_PATH: &str = "/api/native-browser/host-state";
pub const TOKEN_FILE: &str = "native-browser-host-token";
pub const MAX_STATE_BYTES: usize = 4096;
pub const LEASE_MILLIS: u64 = 3000;

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
}
