use serde::Serialize;

const STAGE_B_REASON: &str = "等待阶段 B 接入真实运行数据";
const STAGE_C_REASON: &str = "等待阶段 C 接入文件、审批或媒体闭环";
const ILINK_DIRECT_ONLY_REASON: &str = "官方 iLink Provider 当前仅支持私聊，未开放群聊能力";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WechatCommandCategory {
    Help,
    Identity,
    Room,
    Session,
    Model,
    Agent,
    Workspace,
    Task,
    Intervention,
    File,
    Artifact,
    Permission,
    Diagnostic,
    Notification,
}

impl WechatCommandCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Help => "帮助",
            Self::Identity => "身份与状态",
            Self::Room => "聊天室",
            Self::Session => "会话",
            Self::Model => "模型",
            Self::Agent => "Agent",
            Self::Workspace => "工作区",
            Self::Task => "任务",
            Self::Intervention => "运行中干预",
            Self::File => "文件",
            Self::Artifact => "产物",
            Self::Permission => "权限",
            Self::Diagnostic => "诊断",
            Self::Notification => "通知",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WechatCapability {
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "tools.read")]
    ToolsRead,
    #[serde(rename = "tools.write")]
    ToolsWrite,
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "files.write")]
    FilesWrite,
    #[serde(rename = "tasks.control")]
    TasksControl,
    #[serde(rename = "approvals.resolve")]
    ApprovalsResolve,
    #[serde(rename = "bindings.admin")]
    BindingsAdmin,
}

impl WechatCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Status => "status",
            Self::ToolsRead => "tools.read",
            Self::ToolsWrite => "tools.write",
            Self::FilesRead => "files.read",
            Self::FilesWrite => "files.write",
            Self::TasksControl => "tasks.control",
            Self::ApprovalsResolve => "approvals.resolve",
            Self::BindingsAdmin => "bindings.admin",
        }
    }

    pub const fn minimum_role_label(self) -> &'static str {
        match self {
            Self::Chat | Self::Status => "聊天成员",
            Self::ToolsRead | Self::ToolsWrite | Self::FilesRead | Self::FilesWrite => "操作员",
            Self::TasksControl | Self::ApprovalsResolve | Self::BindingsAdmin => "操作管理员",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum WechatCommandAvailability {
    Available,
    FeatureGated(&'static str),
}

impl WechatCommandAvailability {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Available => "可用",
            Self::FeatureGated(_) => "当前不可用",
        }
    }

    pub const fn disabled_reason(self) -> Option<&'static str> {
        match self {
            Self::Available => None,
            Self::FeatureGated(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WechatCommandSpec {
    pub id: &'static str,
    pub path: &'static [&'static str],
    pub category: WechatCommandCategory,
    pub syntax: &'static str,
    pub summary: &'static str,
    pub capability: WechatCapability,
    pub parameters: &'static str,
    pub examples: &'static [&'static str],
    pub terminal_states: &'static [&'static str],
    pub availability: WechatCommandAvailability,
}

macro_rules! command {
    ($id:literal, [$($path:literal),+], $category:ident, $syntax:literal, $summary:literal, $capability:ident, $parameters:literal, [$($example:literal),+], $availability:expr) => {
        WechatCommandSpec {
            id: $id,
            path: &[$($path),+],
            category: WechatCommandCategory::$category,
            syntax: $syntax,
            summary: $summary,
            capability: WechatCapability::$capability,
            parameters: $parameters,
            examples: &[$($example),+],
            terminal_states: &["成功", "拒绝", "当前不可用", "失败"],
            availability: $availability,
        }
    };
}

#[rustfmt::skip]
static COMMAND_REGISTRY: &[WechatCommandSpec] = &[
    command!("help", ["help"], Help, "/help [主题]", "显示完整命令目录或单项帮助", Status, "主题：可选命令名，不含或包含 / 均可", ["/help", "/help file get"], WechatCommandAvailability::Available),
    command!("commands", ["commands"], Help, "/commands", "显示完整命令目录（/help 别名）", Status, "无", ["/commands"], WechatCommandAvailability::Available),
    command!("status", ["status"], Identity, "/status [detail]", "查看微信连接与当前路由状态", Status, "detail：可选，显示完整诊断", ["/status", "/status detail"], WechatCommandAvailability::Available),
    command!("ping", ["ping"], Identity, "/ping", "检查微信到 coolzhu 的往返链路", Status, "无", ["/ping"], WechatCommandAvailability::Available),
    command!("whoami", ["whoami"], Identity, "/whoami", "查看当前私聊联系人身份与绑定授权", Status, "无", ["/whoami"], WechatCommandAvailability::Available),
    command!("rooms", ["rooms"], Room, "/rooms [关键词]", "列出可绑定的 coolzhu 聊天室", Status, "关键词：可选名称过滤", ["/rooms", "/rooms 项目"], WechatCommandAvailability::Available),
    command!("room.select", ["room"], Room, "/room <编号或ID>", "选择当前微信会话对应的聊天室", BindingsAdmin, "编号或ID：来自最近一次 /rooms", ["/room 1", "/room main-room"], WechatCommandAvailability::Available),
    command!("room.current", ["room", "current"], Room, "/room current", "查看当前聊天室绑定", Status, "无", ["/room current"], WechatCommandAvailability::Available),
    command!("group.current", ["group", "current"], Room, "/group current", "查看当前群绑定与同步状态", Status, "仅群聊可用", ["/group current"], WechatCommandAvailability::FeatureGated(ILINK_DIRECT_ONLY_REASON)),
    command!("group.detach", ["group", "detach"], Room, "/group detach", "确认已移出并清理当前群", BindingsAdmin, "仅操作管理员可用", ["/group detach"], WechatCommandAvailability::FeatureGated(ILINK_DIRECT_ONLY_REASON)),
    command!("sessions", ["sessions"], Session, "/sessions", "列出当前聊天室可用模型会话", Status, "无", ["/sessions"], WechatCommandAvailability::Available),
    command!("use", ["use"], Session, "/use <编号|会话ID|唯一会话名>", "选择已有模型会话", BindingsAdmin, "编号来自最近一次 /sessions；也可使用稳定会话 ID 或唯一会话名；不接受模型名", ["/use 2", "/use session-1781738898772"], WechatCommandAvailability::Available),
    command!("new", ["new"], Session, "/new <编号|会话ID|唯一会话名>", "兼容命令：等同 /use，不创建新会话", BindingsAdmin, "必须选择已有会话；模型名不能作为会话标识", ["/new 2", "/new session-1781738898772"], WechatCommandAvailability::Available),
    command!("compact", ["compact"], Session, "/compact", "压缩当前模型会话上下文", TasksControl, "无", ["/compact"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
    command!("models", ["models"], Model, "/models [provider]", "列出可用模型及 Provider", Status, "provider：可选 Provider 过滤", ["/models", "/models bailian"], WechatCommandAvailability::Available),
    command!("model.select", ["model"], Model, "/model <编号或名称>", "切换当前会话模型", BindingsAdmin, "编号或名称：来自最近一次 /models", ["/model 1", "/model glm-5.2"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
    command!("model.status", ["model", "status"], Model, "/model status", "查看当前模型及调用能力", Status, "无", ["/model status"], WechatCommandAvailability::Available),
    command!("targets", ["targets"], Agent, "/targets", "列出可接收消息的目标 Agent", Status, "无", ["/targets"], WechatCommandAvailability::Available),
    command!("target.select", ["target"], Agent, "/target <编号或ID>", "选择一个或多个目标 Agent", BindingsAdmin, "编号或ID：逗号或空格分隔", ["/target 1", "/target agent-a,agent-b"], WechatCommandAvailability::Available),
    command!("target.clear", ["target", "clear"], Agent, "/target clear", "清空显式目标 Agent", BindingsAdmin, "无", ["/target clear"], WechatCommandAvailability::Available),
    command!("workspace.current", ["workspace"], Workspace, "/workspace", "查看当前工作区", Status, "无", ["/workspace"], WechatCommandAvailability::Available),
    command!("workspace.select", ["workspace"], Workspace, "/workspace <编号>", "选择当前绑定工作区", BindingsAdmin, "编号：来自工作区目录", ["/workspace 1"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
    command!("cwd", ["cwd"], Workspace, "/cwd", "查看授权工作目录", Status, "无", ["/cwd"], WechatCommandAvailability::Available),
    command!("tasks", ["tasks"], Task, "/tasks", "列出当前会话任务", Status, "无", ["/tasks"], WechatCommandAvailability::Available),
    command!("task", ["task"], Task, "/task <ID>", "查看任务详情、进度和产物", Status, "ID：任务 ID 或最近一次 /tasks 编号", ["/task task-001", "/task 1"], WechatCommandAvailability::Available),
    command!("continue", ["continue"], Task, "/continue <ID>", "继续指定任务", TasksControl, "ID：任务 ID 或最近一次 /tasks 编号", ["/continue task-001", "/continue 1"], WechatCommandAvailability::Available),
    command!("stop", ["stop"], Task, "/stop [ID]", "停止当前任务或模型运行", TasksControl, "ID：可选；默认停止当前聊天室最近的活动任务", ["/stop", "/stop 1"], WechatCommandAvailability::Available),
    command!("steer", ["steer"], Intervention, "/steer <补充指令>", "向运行中的任务追加约束", TasksControl, "补充指令：不可为空", ["/steer 只修改文档，不改源码"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
    command!("queue.status", ["queue", "status"], Intervention, "/queue status", "查看消息与任务队列", Status, "无", ["/queue status"], WechatCommandAvailability::Available),
    command!("file.list", ["file", "list"], File, "/file list [路径]", "列出授权路径内文件", FilesRead, "路径：可选，默认当前工作目录", ["/file list", "/file list docs"], WechatCommandAvailability::Available),
    command!("file.info", ["file", "info"], File, "/file info <路径>", "查看文件元数据和校验值", FilesRead, "路径：授权根内相对路径", ["/file info README.md"], WechatCommandAvailability::Available),
    command!("file.get", ["file", "get"], File, "/file get <路径>", "读取文件并回传到微信", FilesRead, "路径：授权根内相对路径", ["/file get docs/report.md"], WechatCommandAvailability::Available),
    command!("file.put", ["file", "put"], File, "/file put <路径>", "把微信附件写入目标路径", FilesWrite, "路径：目标相对路径；同消息或下一条消息需含附件", ["/file put incoming/report.docx"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("file.write", ["file", "write"], File, "/file write <路径>", "把命令下一行文本写入目标文件", FilesWrite, "路径：目标相对路径；正文位于下一行", ["/file write notes/todo.md\n完成微信验收"], WechatCommandAvailability::Available),
    command!("artifact.latest", ["artifact", "latest"], Artifact, "/artifact latest", "回传当前会话最近任务产物", FilesRead, "无", ["/artifact latest"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("artifact.get", ["artifact"], Artifact, "/artifact <任务或文件>", "选择并回传登记产物", FilesRead, "任务或文件：任务 ID、产物编号或产物 ID", ["/artifact task-001", "/artifact 2"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("diff", ["diff"], Artifact, "/diff [任务ID]", "查看任务代码差异摘要", ToolsRead, "任务ID：可选，默认当前任务", ["/diff", "/diff task-001"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("tests", ["tests"], Artifact, "/tests [任务ID]", "查看任务测试结果", ToolsRead, "任务ID：可选，默认当前任务", ["/tests", "/tests task-001"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("permissions", ["permissions"], Permission, "/permissions", "查看当前私聊联系人能力与聊天室授权交集", Status, "无", ["/permissions"], WechatCommandAvailability::Available),
    command!("members", ["members"], Permission, "/members", "查看当前群已识别成员与角色", Status, "仅群聊可用", ["/members"], WechatCommandAvailability::FeatureGated(ILINK_DIRECT_ONLY_REASON)),
    command!("member.set", ["member"], Permission, "/member <成员ID> operator|chat|none", "设置已识别成员角色", BindingsAdmin, "成员ID与角色均必填", ["/member member-1 operator"], WechatCommandAvailability::FeatureGated(ILINK_DIRECT_ONLY_REASON)),
    command!("approvals", ["approvals"], Permission, "/approvals", "列出当前待审批请求", Status, "无", ["/approvals"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("approve", ["approve"], Permission, "/approve <ID> once|always", "批准一次或建立受限持续授权", ApprovalsResolve, "ID：审批 ID；模式：once 或 always", ["/approve approval-1 once"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("deny", ["deny"], Permission, "/deny <ID>", "拒绝待审批请求", ApprovalsResolve, "ID：审批 ID", ["/deny approval-1"], WechatCommandAvailability::FeatureGated(STAGE_C_REASON)),
    command!("errors", ["errors"], Diagnostic, "/errors", "查看最近结构化错误", Status, "无", ["/errors"], WechatCommandAvailability::Available),
    command!("logs", ["logs"], Diagnostic, "/logs [条数]", "查看脱敏通道日志", Status, "条数：可选，受上限约束", ["/logs", "/logs 20"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
    command!("reconnect", ["reconnect"], Diagnostic, "/reconnect", "重连 Sidecar 与 Provider", BindingsAdmin, "无", ["/reconnect"], WechatCommandAvailability::Available),
    command!("notify.set", ["notify"], Notification, "/notify on|off", "开启或关闭任务通知", BindingsAdmin, "状态：on 或 off", ["/notify on", "/notify off"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
    command!("notify.status", ["notify", "status"], Notification, "/notify status", "查看任务通知设置", Status, "无", ["/notify status"], WechatCommandAvailability::FeatureGated(STAGE_B_REASON)),
];

pub fn command_registry() -> &'static [WechatCommandSpec] {
    COMMAND_REGISTRY
}

pub fn command_spec_by_id(id: &str) -> Option<&'static WechatCommandSpec> {
    COMMAND_REGISTRY.iter().find(|command| command.id == id)
}

fn command_arguments_after_path(input: &str, path_len: usize) -> String {
    let mut completed_tokens = 0_usize;
    let mut in_token = false;
    let mut path_end = input.len();
    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() {
            if in_token {
                completed_tokens += 1;
                in_token = false;
                if completed_tokens == path_len {
                    path_end = index;
                    break;
                }
            }
        } else {
            in_token = true;
        }
    }
    if completed_tokens < path_len && in_token {
        completed_tokens += 1;
        path_end = input.len();
    }
    if completed_tokens < path_len {
        return String::new();
    }
    input[path_end..]
        .trim_start_matches(char::is_whitespace)
        .to_string()
}

pub fn match_command_spec(input: &str) -> Option<(&'static WechatCommandSpec, String)> {
    let normalized = input.trim().strip_prefix('/')?.trim();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    COMMAND_REGISTRY
        .iter()
        .filter(|command| {
            tokens.len() >= command.path.len()
                && command
                    .path
                    .iter()
                    .zip(tokens.iter())
                    .all(|(expected, actual)| expected.eq_ignore_ascii_case(actual))
        })
        .max_by_key(|command| {
            let has_arguments = tokens.len() > command.path.len();
            let argument_shape_match = match command.id {
                "workspace.current" => !has_arguments,
                "workspace.select" => has_arguments,
                _ => true,
            };
            (command.path.len(), argument_shape_match)
        })
        .map(|command| {
            let arguments = command_arguments_after_path(normalized, command.path.len());
            (command, arguments)
        })
}

fn command_catalog_entry(command: &WechatCommandSpec) -> String {
    let mut entry = format!(
        "{} — {}\n  所需能力：{} · 最小角色：{} · 状态：{}",
        command.syntax,
        command.summary,
        command.capability.as_str(),
        command.capability.minimum_role_label(),
        command.availability.label()
    );
    if let Some(reason) = command.availability.disabled_reason() {
        entry.push_str("（");
        entry.push_str(reason);
        entry.push('）');
    }
    entry
}

pub fn render_help_pages(registry: &[WechatCommandSpec], max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(120);
    let mut blocks = Vec::new();
    let mut previous_category = None;
    for command in registry {
        let category = command.category;
        let mut block = String::new();
        if previous_category != Some(category) {
            block.push_str(&format!("【{}】\n", category.label()));
            previous_category = Some(category);
        }
        block.push_str(&command_catalog_entry(command));
        blocks.push(block);
    }

    let mut bodies = Vec::<String>::new();
    for block in blocks {
        let needs_new_page = bodies
            .last()
            .is_some_and(|body| body.chars().count() + block.chars().count() + 2 > max_chars);
        if bodies.is_empty() || needs_new_page {
            bodies.push(block);
        } else if let Some(body) = bodies.last_mut() {
            body.push_str("\n\n");
            body.push_str(&block);
        }
    }
    let total = bodies.len().max(1);
    if bodies.is_empty() {
        bodies.push("当前构建没有注册微信命令。".to_string());
    }
    bodies
        .into_iter()
        .enumerate()
        .map(|(index, body)| format!("微信连接命令 第 {}/{} 页\n\n{}", index + 1, total, body))
        .collect()
}

pub fn render_command_help(topic: &str) -> Option<String> {
    let normalized = topic
        .trim()
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .replace('.', " ");
    let command = COMMAND_REGISTRY.iter().find(|command| {
        command.id.replace('.', " ") == normalized
            || (command.id.ends_with(".select")
                && command.id.trim_end_matches(".select") == normalized)
    })?;
    let examples = command
        .examples
        .iter()
        .map(|example| format!("  {example}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut detail = format!(
        "{}\n{}\n所需能力：{}\n最小角色：{}\n参数：{}\n示例：\n{}\n可能终态：{}\n状态：{}",
        command.syntax,
        command.summary,
        command.capability.as_str(),
        command.capability.minimum_role_label(),
        command.parameters,
        examples,
        command.terminal_states.join("、"),
        command.availability.label()
    );
    if let Some(reason) = command.availability.disabled_reason() {
        detail.push_str("\n禁用原因：");
        detail.push_str(reason);
    }
    Some(detail)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const EXPECTED_COMMAND_IDS: &[&str] = &[
        "help",
        "commands",
        "status",
        "ping",
        "whoami",
        "rooms",
        "room.select",
        "room.current",
        "group.current",
        "group.detach",
        "sessions",
        "use",
        "new",
        "compact",
        "models",
        "model.select",
        "model.status",
        "targets",
        "target.select",
        "target.clear",
        "workspace.current",
        "workspace.select",
        "cwd",
        "tasks",
        "task",
        "continue",
        "stop",
        "steer",
        "queue.status",
        "file.list",
        "file.info",
        "file.get",
        "file.put",
        "file.write",
        "artifact.latest",
        "artifact.get",
        "diff",
        "tests",
        "permissions",
        "members",
        "member.set",
        "approvals",
        "approve",
        "deny",
        "errors",
        "logs",
        "reconnect",
        "notify.set",
        "notify.status",
    ];

    #[test]
    fn registry_contains_every_acceptance_command_with_complete_metadata() {
        let registry = command_registry();
        let actual = registry
            .iter()
            .map(|command| command.id)
            .collect::<BTreeSet<_>>();
        let expected = EXPECTED_COMMAND_IDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();

        assert_eq!(actual, expected);
        assert_eq!(actual.len(), registry.len(), "命令 ID 必须唯一");
        for command in registry {
            assert!(!command.syntax.trim().is_empty(), "{} 缺少语法", command.id);
            assert!(
                !command.summary.trim().is_empty(),
                "{} 缺少说明",
                command.id
            );
            assert!(
                !command.capability.as_str().is_empty(),
                "{} 缺少能力",
                command.id
            );
            assert!(
                !command.category.label().is_empty(),
                "{} 缺少分类",
                command.id
            );
        }
    }

    #[test]
    fn full_help_pages_reconstruct_the_registry_without_omission() {
        let pages = render_help_pages(command_registry(), 420);

        assert!(pages.len() > 1, "较小消息上限应触发分页");
        for (index, page) in pages.iter().enumerate() {
            assert!(page.starts_with(&format!("微信连接命令 第 {}/{} 页", index + 1, pages.len())));
        }
        let merged = pages.join("\n");
        for command in command_registry() {
            assert!(
                merged.contains(command.syntax),
                "帮助目录遗漏 {}：{}",
                command.id,
                command.syntax
            );
            assert!(merged.contains(command.capability.as_str()));
            if let WechatCommandAvailability::FeatureGated(reason) = command.availability {
                assert!(
                    merged.contains(reason),
                    "禁用命令 {} 未显示原因",
                    command.id
                );
            }
        }
    }

    #[test]
    fn command_help_reports_parameters_permission_examples_and_availability() {
        let detail = render_command_help("file get").expect("file get help");

        assert!(detail.contains("/file get <路径>"));
        assert!(detail.contains("所需能力：files.read"));
        assert!(detail.contains("参数："));
        assert!(detail.contains("示例："));
        assert!(detail.contains("可能终态："));
    }

    #[test]
    fn stage_b_control_commands_are_advertised_only_after_real_runtime_wiring() {
        for command_id in ["new", "continue", "stop", "reconnect"] {
            let spec = command_spec_by_id(command_id).expect("命令必须存在于统一注册表");
            assert_eq!(
                spec.availability,
                WechatCommandAvailability::Available,
                "{command_id} 必须接通真实执行路径后才能在 /help 中标记可用"
            );
        }
    }

    #[test]
    fn help_lists_group_member_commands_and_roles() {
        let help = render_help_pages(command_registry(), 10_000).join("\n");
        for syntax in [
            "/members",
            "/member <成员ID> operator|chat|none",
            "/group current",
            "/group detach",
        ] {
            assert!(help.contains(syntax), "帮助目录缺少 {syntax}");
        }
        for role in ["聊天成员", "操作员", "操作管理员"] {
            assert!(help.contains(role), "帮助目录缺少最小角色说明：{role}");
        }
    }

    #[test]
    fn official_ilink_group_commands_are_visible_but_feature_gated() {
        for command_id in ["group.current", "group.detach", "members", "member.set"] {
            let command = command_spec_by_id(command_id).expect("群聊命令应保留兼容注册信息");
            assert!(
                matches!(command.availability, WechatCommandAvailability::FeatureGated(reason) if reason.contains("官方 iLink") && reason.contains("私聊")),
                "{command_id} 不应在仅支持私聊的官方 iLink Provider 中显示为可用"
            );
        }
    }

    #[test]
    fn file_write_command_preserves_multiline_body() {
        let (_, arguments) = match_command_spec("/file write notes/todo.md\n第一行\n第二行")
            .expect("应识别 file write 命令");

        assert_eq!(arguments, "notes/todo.md\n第一行\n第二行");
    }
}
