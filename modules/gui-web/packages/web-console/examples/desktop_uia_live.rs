//! 独立编译真实 UIA 关键验收，复用生产模块，避免为单个桌面控件重建全部控制台测试。
//! 仅在显式传入 COOLZHU_UIA_FIXTURE 且启用 ignored 用例时产生桌面输入。
#[path = "../src/computer_use_frame.rs"]
mod computer_use_frame;
#[path = "../src/computer_use_adapters.rs"]
mod computer_use_adapters;
#[path = "../src/computer_use_desktop_bridge.rs"]
mod computer_use_desktop_bridge;

fn main() {}
