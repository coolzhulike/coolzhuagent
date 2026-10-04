//! Devin 配置、ACP 会话和受控工具桥。聊天室开放固定 CLI 的纯文本配置，工具任务仍关闭。
pub(super) mod discovery;
pub(super) mod auth;
pub(super) mod bridge;
mod journal;
mod protocol;
mod process;
mod session;
mod transport;
pub(super) mod chat;
#[cfg(all(test, windows))]
mod real_smoke;
