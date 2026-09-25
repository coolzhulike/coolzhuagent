use serde::{Deserialize, Serialize};

use crate::wechat_command::WechatCapability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WechatMemberPreset {
    None,
    ChatMember,
    Collaborator,
    Operator,
    Administrator,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatCapabilitySet {
    pub chat: bool,
    pub status: bool,
    pub tools_read: bool,
    pub tools_write: bool,
    pub files_read: bool,
    pub files_write: bool,
    pub tasks_control: bool,
    pub approvals_resolve: bool,
    pub bindings_admin: bool,
}

impl WechatCapabilitySet {
    pub const fn for_preset(preset: WechatMemberPreset) -> Self {
        match preset {
            WechatMemberPreset::None => Self {
                chat: false,
                status: false,
                tools_read: false,
                tools_write: false,
                files_read: false,
                files_write: false,
                tasks_control: false,
                approvals_resolve: false,
                bindings_admin: false,
            },
            WechatMemberPreset::ChatMember => Self {
                chat: true,
                status: true,
                tools_read: false,
                tools_write: false,
                files_read: false,
                files_write: false,
                tasks_control: false,
                approvals_resolve: false,
                bindings_admin: false,
            },
            WechatMemberPreset::Collaborator => Self {
                chat: true,
                status: true,
                tools_read: true,
                tools_write: false,
                files_read: true,
                files_write: false,
                tasks_control: false,
                approvals_resolve: false,
                bindings_admin: false,
            },
            WechatMemberPreset::Operator => Self {
                chat: true,
                status: true,
                tools_read: true,
                tools_write: true,
                files_read: true,
                files_write: true,
                tasks_control: false,
                approvals_resolve: false,
                bindings_admin: false,
            },
            WechatMemberPreset::Administrator => Self {
                chat: true,
                status: true,
                tools_read: true,
                tools_write: true,
                files_read: true,
                files_write: true,
                tasks_control: true,
                approvals_resolve: true,
                bindings_admin: true,
            },
        }
    }

    pub const fn allows(self, capability: WechatCapability) -> bool {
        match capability {
            WechatCapability::Chat => self.chat,
            WechatCapability::Status => self.status,
            WechatCapability::ToolsRead => self.tools_read,
            WechatCapability::ToolsWrite => self.tools_write,
            WechatCapability::FilesRead => self.files_read,
            WechatCapability::FilesWrite => self.files_write,
            WechatCapability::TasksControl => self.tasks_control,
            WechatCapability::ApprovalsResolve => self.approvals_resolve,
            WechatCapability::BindingsAdmin => self.bindings_admin,
        }
    }

    /// 群成员只允许三种公开角色。旧版或篡改后的高权限预设必须按无权限处理，
    /// 操作管理员能力只由独立的管理员认领记录授予。
    pub const fn for_group_member_role(preset: WechatMemberPreset) -> Self {
        match preset {
            WechatMemberPreset::None
            | WechatMemberPreset::ChatMember
            | WechatMemberPreset::Operator => Self::for_preset(preset),
            WechatMemberPreset::Collaborator | WechatMemberPreset::Administrator => {
                Self::for_preset(WechatMemberPreset::None)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WechatMemberGrant {
    pub account_id: String,
    pub group_id: String,
    pub member_id: String,
    pub member_name: Option<String>,
    pub preset: WechatMemberPreset,
    pub capabilities: WechatCapabilitySet,
    pub enabled: bool,
    pub created_by: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default)]
    pub first_seen_at_ms: u64,
    #[serde(default)]
    pub last_seen_at_ms: u64,
    #[serde(default)]
    pub source: String,
}

impl WechatMemberGrant {
    pub fn from_preset(
        account_id: impl Into<String>,
        group_id: impl Into<String>,
        member_id: impl Into<String>,
        member_name: Option<String>,
        preset: WechatMemberPreset,
        now_ms: u64,
    ) -> Self {
        Self {
            account_id: account_id.into(),
            group_id: group_id.into(),
            member_id: member_id.into(),
            member_name,
            preset,
            capabilities: WechatCapabilitySet::for_preset(preset),
            enabled: true,
            created_by: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            first_seen_at_ms: now_ms,
            last_seen_at_ms: now_ms,
            source: "manual".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WechatRoomPermissionScope {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

#[derive(Debug)]
pub struct WechatAuthorizationContext<'a> {
    pub account_online: bool,
    pub binding_enabled: bool,
    pub is_group: bool,
    pub mentioned_bot: bool,
    pub management_command_without_mention: bool,
    pub is_operation_administrator: bool,
    pub is_wechat_group_admin: bool,
    pub account_id: &'a str,
    pub group_id: &'a str,
    pub member_id: &'a str,
    pub required: WechatCapability,
    pub room_scope: WechatRoomPermissionScope,
    pub grant: Option<&'a WechatMemberGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum WechatAuthorizationDecision {
    Allow {
        requires_approval: bool,
    },
    Deny {
        code: &'static str,
        reason: &'static str,
    },
}

pub fn evaluate_wechat_authorization(
    input: &WechatAuthorizationContext<'_>,
) -> WechatAuthorizationDecision {
    if !input.account_online {
        return WechatAuthorizationDecision::Deny {
            code: "account_offline",
            reason: "微信连接账号当前不在线",
        };
    }
    if !input.binding_enabled {
        return WechatAuthorizationDecision::Deny {
            code: "binding_disabled",
            reason: "当前微信群绑定未启用",
        };
    }
    if input.is_group {
        if !input.mentioned_bot && !input.management_command_without_mention {
            return WechatAuthorizationDecision::Deny {
                code: "bot_not_mentioned",
                reason: "群消息没有明确 @当前微信机器人",
            };
        }
        let capabilities = if input.is_operation_administrator {
            WechatCapabilitySet::for_preset(WechatMemberPreset::Administrator)
        } else {
            let Some(grant) = input.grant else {
                return WechatAuthorizationDecision::Deny {
                    code: "member_not_allowlisted",
                    reason: "群成员不在微信连接白名单中",
                };
            };
            let identity_matches = grant.enabled
                && grant.account_id == input.account_id
                && grant.group_id == input.group_id
                && grant.member_id == input.member_id;
            if !identity_matches {
                return WechatAuthorizationDecision::Deny {
                    code: "member_not_allowlisted",
                    reason: "群成员不在微信连接白名单中",
                };
            }
            WechatCapabilitySet::for_group_member_role(grant.preset)
        };
        if !capabilities.allows(input.required) {
            return WechatAuthorizationDecision::Deny {
                code: "capability_denied",
                reason: "群成员未获得命令所需能力",
            };
        }
    }

    let is_write = matches!(
        input.required,
        WechatCapability::ToolsWrite | WechatCapability::FilesWrite
    );
    if is_write {
        return match input.room_scope {
            WechatRoomPermissionScope::ReadOnly => WechatAuthorizationDecision::Deny {
                code: "room_permission_denied",
                reason: "聊天室授权不允许写入或写工具",
            },
            WechatRoomPermissionScope::WorkspaceWrite => WechatAuthorizationDecision::Allow {
                requires_approval: true,
            },
            WechatRoomPermissionScope::FullAccess => WechatAuthorizationDecision::Allow {
                requires_approval: false,
            },
        };
    }

    WechatAuthorizationDecision::Allow {
        requires_approval: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wechat_command::WechatCapability;

    fn context(required: WechatCapability) -> WechatAuthorizationContext<'static> {
        WechatAuthorizationContext {
            account_online: true,
            binding_enabled: true,
            is_group: true,
            mentioned_bot: true,
            management_command_without_mention: false,
            is_operation_administrator: false,
            is_wechat_group_admin: false,
            account_id: "wx-main",
            group_id: "group-1",
            member_id: "member-1",
            required,
            room_scope: WechatRoomPermissionScope::WorkspaceWrite,
            grant: None,
        }
    }

    fn grant(preset: WechatMemberPreset) -> WechatMemberGrant {
        WechatMemberGrant::from_preset(
            "wx-main",
            "group-1",
            "member-1",
            Some("测试成员".to_string()),
            preset,
            100,
        )
    }

    #[test]
    fn group_member_is_denied_by_default_even_when_wechat_group_admin() {
        let mut input = context(WechatCapability::Chat);
        input.is_wechat_group_admin = true;

        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Deny {
                code: "member_not_allowlisted",
                reason: "群成员不在微信连接白名单中",
            }
        );
    }

    #[test]
    fn allowlisted_group_member_must_mention_the_bot() {
        let member = grant(WechatMemberPreset::ChatMember);
        let mut input = context(WechatCapability::Chat);
        input.mentioned_bot = false;
        input.grant = Some(&member);

        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Deny {
                code: "bot_not_mentioned",
                reason: "群消息没有明确 @当前微信机器人",
            }
        );
    }

    #[test]
    fn chat_member_cannot_write_even_in_full_access_room() {
        let member = grant(WechatMemberPreset::ChatMember);
        let mut input = context(WechatCapability::FilesWrite);
        input.room_scope = WechatRoomPermissionScope::FullAccess;
        input.grant = Some(&member);

        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Deny {
                code: "capability_denied",
                reason: "群成员未获得命令所需能力",
            }
        );
    }

    #[test]
    fn operator_write_requires_approval_in_normal_room_but_not_full_access() {
        let member = grant(WechatMemberPreset::Operator);
        let mut input = context(WechatCapability::FilesWrite);
        input.grant = Some(&member);

        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Allow {
                requires_approval: true,
            }
        );

        input.room_scope = WechatRoomPermissionScope::FullAccess;
        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Allow {
                requires_approval: false,
            }
        );
    }

    #[test]
    fn operator_cannot_control_tasks_or_approvals() {
        let capabilities = WechatCapabilitySet::for_preset(WechatMemberPreset::Operator);

        assert!(capabilities.chat);
        assert!(capabilities.status);
        assert!(capabilities.tools_read);
        assert!(capabilities.tools_write);
        assert!(capabilities.files_read);
        assert!(capabilities.files_write);
        assert!(!capabilities.tasks_control);
        assert!(!capabilities.approvals_resolve);
        assert!(!capabilities.bindings_admin);
    }

    #[test]
    fn newly_observed_member_none_preset_has_no_capabilities() {
        assert_eq!(
            WechatCapabilitySet::for_preset(WechatMemberPreset::None),
            WechatCapabilitySet::default()
        );
    }

    #[test]
    fn operation_administrator_uses_admin_caps_but_still_obeys_room_scope() {
        let mut input = context(WechatCapability::TasksControl);
        input.is_operation_administrator = true;

        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Allow {
                requires_approval: false
            }
        );

        input.required = WechatCapability::FilesWrite;
        input.room_scope = WechatRoomPermissionScope::ReadOnly;
        assert!(matches!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Deny {
                code: "room_permission_denied",
                ..
            }
        ));
    }

    #[test]
    fn stored_capability_blob_cannot_escalate_an_ordinary_member() {
        let mut member = grant(WechatMemberPreset::ChatMember);
        member.capabilities = WechatCapabilitySet::for_preset(WechatMemberPreset::Administrator);
        let mut input = context(WechatCapability::TasksControl);
        input.grant = Some(&member);

        assert_eq!(
            evaluate_wechat_authorization(&input),
            WechatAuthorizationDecision::Deny {
                code: "capability_denied",
                reason: "群成员未获得命令所需能力",
            }
        );
    }
}
