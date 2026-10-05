//! Devin 配置、持久 ACP 会话与受控宿主工具桥。聊天室开放文本、工程只读与 Computer Use。
pub(super) mod discovery;
pub(super) mod auth;
pub(super) mod bridge;
mod journal;
mod protocol;
mod process;
mod session;
mod transport;
mod tool_wait;
pub(crate) mod context;
pub(super) mod chat;
pub(super) mod internal;
#[cfg(all(test, windows))]
mod real_smoke;
