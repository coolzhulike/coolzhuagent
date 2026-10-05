//! Devin 配置、持久 ACP 会话与受控宿主工具桥。聊天室复用工程只读、Computer Use 和插件。
pub(super) mod discovery;
pub(super) mod auth;
pub(super) mod bridge;
pub(super) mod host_tools;
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
