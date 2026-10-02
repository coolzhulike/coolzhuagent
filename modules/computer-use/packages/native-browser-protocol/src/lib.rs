//! 原生浏览器宿主的资源登记契约；独立于 Chrome tab 协议，不携带网页权限。
use serde::{Deserialize, Serialize};

pub const STATE_PATH: &str = "/api/native-browser/host-state";
pub const OBSERVATION_PATH: &str = "/api/native-browser/host-observation";
pub const INPUT_PATH: &str = "/api/native-browser/host-input";
pub const MAX_INPUT_BYTES: usize = 8192;
pub const MAX_OBSERVATION_BYTES: usize = 262_144;
pub const TOKEN_FILE: &str = "native-browser-host-token";
pub const MAX_STATE_BYTES: usize = 4096;
pub const MAX_HOST_RECEIPT_BYTES: usize = 8192;
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
    /// 独立认证通道的来源声明；服务端仍须按OS进程实例和实际shell路径重新核验。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<HostIdentity>,
    // 隐藏、关闭、切换环境或加载时为 None，不能沿用旧资源。
    pub resource: Option<PanelResource>,
}

impl HostState {
    pub fn valid_shape(&self) -> bool {
        (16..=96).contains(&self.host_id.len())
            && self.host_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && self.sequence > 0
            && self.identity.as_ref().is_none_or(HostIdentity::valid_shape)
            && self.resource.as_ref().is_none_or(PanelResource::valid_shape)
    }
}

/// 长期宿主与短命helper的身份契约分离；不能用PID字符串或当前时间伪造创建身份。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostIdentity {
    pub instance_id: String,
    pub boot_id: String,
    pub pid: u32,
    pub creation_time_filetime: u64,
    pub canonical_executable: String,
}

impl HostIdentity {
    pub fn valid_shape(&self) -> bool {
        opaque_id(&self.instance_id) && opaque_id(&self.boot_id)
            && self.pid > 0 && self.creation_time_filetime > 0
            && !self.canonical_executable.is_empty() && self.canonical_executable.len() <= 2048
            && !self.canonical_executable.chars().any(char::is_control)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostReceipt {
    pub accepted: bool,
    pub resource_registered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<ObservationRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<PanelInputRequest>,
}

/// 两阶段类型化输入。协议没有任意脚本、CDP方法名或外部坐标字段。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PanelClickTarget {
    pub observation_id: String,
    pub document_token: String,
    pub node_id: String,
}
impl PanelClickTarget {
    pub fn valid_shape(&self) -> bool {
        [&self.observation_id,&self.document_token,&self.node_id].into_iter().all(|s| opaque_id(s))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag="phase", rename_all="snake_case", deny_unknown_fields)]
pub enum PanelInputCommand {
    PrepareClick { target: PanelClickTarget },
    ExecuteClick { target: PanelClickTarget, ticket_id: String, permit_id: String, attempt_id: String, executor_instance_id:String, expires_at_unix_ms: u64 },
    PrepareScroll { target: PanelClickTarget, direction: ScrollDirection, amount: u8 },
    ExecuteScroll { target: PanelClickTarget, direction: ScrollDirection, amount: u8, ticket_id: String, permit_id: String, attempt_id: String, executor_instance_id: String, expires_at_unix_ms: u64 },
    PrepareText { target: PanelClickTarget, text: String },
    ExecuteText { target: PanelClickTarget, text: String, ticket_id: String, permit_id: String, attempt_id: String, executor_instance_id: String, expires_at_unix_ms: u64 },
    PrepareNavigate { target: PanelClickTarget, url: String },
    ExecuteNavigate { target: PanelClickTarget, url: String, ticket_id: String, permit_id: String, attempt_id: String, executor_instance_id: String, expires_at_unix_ms: u64 },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum PanelInputKind { #[default] Click, Scroll, Text, Navigate }

pub fn valid_input_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= 2048 && !text.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
}
pub fn valid_navigation_url(url: &str) -> bool {
    !url.is_empty() && url.len() <= 2048 && !url.chars().any(char::is_control)
        && (url.starts_with("https://") || url.starts_with("http://"))
}

/// 导航回执仍结算源动作；新资源只用于后续重新观察，不复用源文档节点。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PanelNavigationReceipt { pub destination: PanelResource, pub url: String }
impl PanelNavigationReceipt {
    pub fn matches_source(&self, source: &PanelResource) -> bool {
        self.destination.valid_shape() && valid_navigation_url(&self.url)
            && self.destination.workspace_path == source.workspace_path && self.destination.room_id == source.room_id
            && self.destination.label == source.label && self.destination.generation == source.generation
            && source.navigation_revision.checked_add(1) == Some(self.destination.navigation_revision)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum ScrollDirection { Up, Down, Left, Right }
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PanelInputRequest {
    pub request_id: String,
    pub resource: PanelResource,
    pub command: PanelInputCommand,
}
impl PanelInputRequest {
    pub fn valid_shape(&self) -> bool {
        opaque_id(&self.request_id) && self.resource.valid_shape() && match &self.command {
            PanelInputCommand::PrepareClick {target} => target.valid_shape(),
            PanelInputCommand::PrepareScroll {target,amount,..} => target.valid_shape() && (1..=5).contains(amount),
            PanelInputCommand::PrepareText {target,text} => target.valid_shape() && valid_input_text(text),
            PanelInputCommand::PrepareNavigate {target,url} => target.valid_shape() && valid_navigation_url(url),
            PanelInputCommand::ExecuteText {text,..} if !valid_input_text(text) => false,
            PanelInputCommand::ExecuteNavigate {url,..} if !valid_navigation_url(url) => false,
            PanelInputCommand::ExecuteScroll {amount,..} if !(1..=5).contains(amount) => false,
            PanelInputCommand::ExecuteClick {target,ticket_id,permit_id,attempt_id,executor_instance_id,expires_at_unix_ms}
            | PanelInputCommand::ExecuteScroll {target,ticket_id,permit_id,attempt_id,executor_instance_id,expires_at_unix_ms,..}
            | PanelInputCommand::ExecuteText {target,ticket_id,permit_id,attempt_id,executor_instance_id,expires_at_unix_ms,..}
            | PanelInputCommand::ExecuteNavigate {target,ticket_id,permit_id,attempt_id,executor_instance_id,expires_at_unix_ms,..} =>
                target.valid_shape() && opaque_id(ticket_id) && opaque_id(executor_instance_id) && *expires_at_unix_ms > 0
                    && [permit_id,attempt_id].into_iter().all(|s| !s.is_empty() && s.len() <= 2048 && !s.chars().any(char::is_control)),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum PanelInputOutcome { Prepared, NotDispatched, Released, ReleaseUnknown, Acknowledged, DispatchUnknown }

/// 认证通道回执是动作事实，不声明网页任务已完成。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PanelInputReply {
    pub host_id: String,
    pub request_id: String,
    pub resource: PanelResource,
    pub outcome: PanelInputOutcome,
    pub ticket_id: Option<String>,
    pub expires_at_unix_ms: Option<u64>,
    pub attempt_id: Option<String>,
    pub executor_instance_id: Option<String>,
    pub down_confirmed: bool,
    pub up_confirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<PanelNavigationReceipt>,
    pub error: Option<String>,
}
impl PanelInputReply {
    pub fn valid_shape(&self) -> bool {
        opaque_id(&self.request_id) && self.resource.valid_shape()
            && self.navigation.as_ref().is_none_or(|nav| self.outcome == PanelInputOutcome::Acknowledged && nav.matches_source(&self.resource))
            && (16..=96).contains(&self.host_id.len())
            && self.host_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && self.ticket_id.as_deref().is_none_or(opaque_id)
            && self.executor_instance_id.as_deref().is_none_or(opaque_id)
            && self.attempt_id.as_ref().is_none_or(|s|!s.is_empty() && s.len()<=2048 && !s.chars().any(char::is_control))
            && self.error.as_ref().is_none_or(|s| s.len() <= 96 && s.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'))
            && match self.outcome {
                PanelInputOutcome::Prepared => self.ticket_id.is_some() && self.expires_at_unix_ms.is_some_and(|v| v > 0)
                    && self.attempt_id.is_none() && self.executor_instance_id.is_none()
                    && !self.down_confirmed && !self.up_confirmed && self.error.is_none(),
                PanelInputOutcome::NotDispatched => !self.down_confirmed && !self.up_confirmed,
                PanelInputOutcome::Released => self.ticket_id.is_some() && self.attempt_id.is_some() && self.executor_instance_id.is_some()
                    && self.down_confirmed && self.up_confirmed && self.error.is_none(),
                // wheel没有按住输入；此回执只证明投递ACK，不伪造按下/释放或滚动目标达成。
                PanelInputOutcome::Acknowledged => self.ticket_id.is_some() && self.attempt_id.is_some() && self.executor_instance_id.is_some()
                    && !self.down_confirmed && !self.up_confirmed && self.error.is_none(),
                PanelInputOutcome::DispatchUnknown => self.ticket_id.is_some() && self.attempt_id.is_some() && self.executor_instance_id.is_some()
                    && !self.down_confirmed && !self.up_confirmed && self.error.is_some(),
                PanelInputOutcome::ReleaseUnknown => true,
            }
    }
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
    /// 宿主采集的CSS视口与滚动偏移；AX节点存在不等于位于可见视口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<PageViewport>,
    /// 文档身份只暴露随机宿主引用；frame/loader/backend DOM ID保留在桌面内存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_token: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_handles: Vec<NodeHandle>,
    /// 仅投影宿主AX确认聚焦的普通编辑节点索引，不携带原值、选区或其它属性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_node_index: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PageViewport {
    pub page_x: f64,
    pub page_y: f64,
    pub width: f64,
    pub height: f64,
}

impl PageViewport {
    pub fn valid_shape(&self) -> bool {
        [self.page_x, self.page_y].into_iter().all(|v| v.is_finite() && v.abs() <= 1e9)
            && [self.width, self.height].into_iter().all(|v| v.is_finite() && v > 0.0 && v <= 32768.0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeHandle { pub index: usize, pub node_id: String }

pub fn opaque_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl PageObservation {
    pub fn valid_shape(&self) -> bool {
        self.url.len() <= 4096 && (self.url.starts_with("http://") || self.url.starts_with("https://"))
            && self.viewport.as_ref().is_none_or(PageViewport::valid_shape)
            && !self.url.chars().any(char::is_control) && self.title.chars().count() <= 256
            && !self.title.chars().any(char::is_control)
            && self.document_token.as_deref().is_none_or(opaque_id)
            && self.node_handles.len() <= self.nodes.len()
            && (self.node_handles.is_empty() || self.document_token.is_some())
            && self.node_handles.iter().all(|handle| handle.index < self.nodes.len() && opaque_id(&handle.node_id))
            && self.node_handles.iter().map(|handle| handle.index).collect::<std::collections::HashSet<_>>().len() == self.node_handles.len()
            && self.node_handles.iter().map(|handle| &handle.node_id).collect::<std::collections::HashSet<_>>().len() == self.node_handles.len()
            && self.focused_node_index.is_none_or(|index| self.nodes.get(index)
                .is_some_and(|node| matches!(node.role.as_str(), "textbox" | "searchbox"))
                && self.node_handles.iter().any(|handle| handle.index == index))
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
