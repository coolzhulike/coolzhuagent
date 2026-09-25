//! 本 crate 对输入后端的**唯一**出口（PR-03：自动化输入一律走受控族）。
//!
//! 这里只保留两件东西：
//! - `diagnostic_click_point`：**诊断**子命令 `--input-backend-click-test` 用（无生命周期保证，
//!   仅供本机自检，不进入自动化输入集合）；
//! - `preflight_report`：只读后端预检。
//!
//! 自动化输入的入口是 `computer_use::input::controlled_*`（见 `desktop_agent.rs`），
//! 不再经本模块转发。
pub use computer_use::input::{diagnostic_click_point, preflight_report};
