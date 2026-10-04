//! Devin 配置发现、ACP 会话及受控工具桥。真实 CLI 与原生 Windows 旁路验收前正式接纳保持关闭。
pub(super) mod discovery;
pub(super) mod auth;
pub(super) mod bridge;
mod journal;
mod protocol;
mod process;
mod session;
mod transport;
#[cfg(all(test, windows))]
mod real_smoke;
