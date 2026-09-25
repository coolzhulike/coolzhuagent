# Weixin ClawBot Channel Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 先把微信 ClawBot 接入做成 coolzhu 可编译、可测试、可被前端/API 发现的通道架构骨架。

**Architecture:** 本阶段不实现真实微信扫码登录和 iLink 长轮询；先新增 `clawbot_channel` 核心模块，定义账号、联系人绑定、命令解析、入站消息预览和状态 DTO。web-console 暴露 `/api/channels/clawbot/*` 预览接口，后续真实 sidecar/native gateway 可以复用这些类型和路由。

**Tech Stack:** Rust 2021、axum、serde、coolzhu-web-console 现有 session/chat room/goal API。

---

### Task 1: ClawBot 通道核心类型与命令解析

**Files:**
- Create: `modules/gui-web/packages/web-console/src/clawbot_channel.rs`
- Modify: `modules/gui-web/packages/web-console/src/main.rs`
- Test: `modules/gui-web/packages/web-console/src/clawbot_channel.rs`

- [x] **Step 1: Write the failing test**

```rust
#[test]
fn parse_room_and_use_commands() {
    assert_eq!(
        parse_clawbot_command("/room 项目A"),
        Some(ClawbotCommand::SelectRoom {
            room: "项目A".to_string()
        })
    );
    assert_eq!(
        parse_clawbot_command("/use GLM5.2"),
        Some(ClawbotCommand::UseSessionOrModel {
            selector: "GLM5.2".to_string()
        })
    );
}

#[test]
fn bind_inbound_text_to_existing_conversation() {
    let state = ClawbotChannelState::default()
        .with_binding(ClawbotConversationBinding {
            account_id: "wx-a".to_string(),
            peer_id: "peer-1".to_string(),
            peer_name: Some("测试联系人".to_string()),
            chat_room_id: Some("room-project-a".to_string()),
            default_session_id: Some("glm-session".to_string()),
            target_agent_ids: vec!["agent-test001".to_string()],
            workspace_id: "default".to_string(),
            last_context_token: Some("ctx-1".to_string()),
            allowlisted: true,
            enabled: true,
        });

    let preview = state.preview_inbound(ClawbotInboundMessage {
        account_id: "wx-a".to_string(),
        peer_id: "peer-1".to_string(),
        peer_name: Some("测试联系人".to_string()),
        context_token: Some("ctx-2".to_string()),
        external_msg_id: "msg-1".to_string(),
        kind: ClawbotMessageKind::Text,
        text: Some("继续上个任务".to_string()),
        media_refs: vec![],
        received_at_ms: 1,
    });

    assert_eq!(preview.action, ClawbotDispatchAction::DispatchToCoolzhu);
    assert_eq!(preview.chat_room_id.as_deref(), Some("room-project-a"));
    assert_eq!(preview.session_id.as_deref(), Some("glm-session"));
    assert_eq!(preview.target_agent_ids, vec!["agent-test001"]);
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p coolzhu-web-console clawbot_channel --offline`

Expected: FAIL because `ClawbotCommand` / `ClawbotChannelState` / parser types are not implemented.

- [x] **Step 3: Write minimal implementation**

Create the data types, parser, default state, binding lookup, allowlist gate, and inbound preview function. Keep it side-effect-free so future sidecar/native gateway can call it safely.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p coolzhu-web-console clawbot_channel --offline`

Expected: PASS.

### Task 2: web-console ClawBot API skeleton

**Files:**
- Modify: `modules/gui-web/packages/web-console/src/main.rs`
- Test: `modules/gui-web/packages/web-console/src/clawbot_channel.rs`

- [x] **Step 1: Write the failing API shape test**

```rust
#[test]
fn default_status_exposes_disabled_text_only_capabilities() {
    let status = ClawbotChannelState::default().status();
    assert_eq!(status.channel, "weixin-clawbot");
    assert_eq!(status.status, ClawbotRuntimeStatus::Disabled);
    assert!(status.capabilities.contains(&"text".to_string()));
    assert!(status.commands.iter().any(|command| command.name == "/room"));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p coolzhu-web-console clawbot_channel --offline`

Expected: FAIL because `status()` and DTO are missing.

- [x] **Step 3: Write minimal implementation**

Expose route handlers:
- `GET /api/channels/clawbot/status`
- `GET /api/channels/clawbot/commands`
- `POST /api/channels/clawbot/inbound/preview`

These handlers use in-memory/default state only in this phase and do not contact WeChat.

- [x] **Step 4: Run test and build**

Run:
- `cargo test -p coolzhu-web-console clawbot_channel --offline`
- `cargo build -p coolzhu-web-console --offline`

Expected: both exit 0.

### Task 3: Documentation and work-log

**Files:**
- Create: `docs/work-logs/2026-07-03-weixin-clawbot-channel-architecture.md`

- [x] **Step 1: Document what is wired**

Record:
- completed code skeleton,
- API routes,
- what is intentionally not implemented,
- next goal phase for real QR login/sidecar.

- [x] **Step 2: Final verification**

Run:
- `cargo test -p coolzhu-web-console clawbot_channel --offline`
- `cargo build -p coolzhu-web-console --offline`

Expected: both exit 0.
