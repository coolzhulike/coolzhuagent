# Computer Use 对外接口说明

## 模块职责

`computer-use` 负责桌面操作闭环的核心抽象与底层输入注入能力，包括分辨率矩阵、语义锚点、物理坐标映射、鼠标/键盘动作、输入后端预检和独立命令行回归测试。

## 对外 crate

- `coolzhu-computer-use-core`
- workspace alias: `computer_use`

## 稳定接口

- `computer_use::ResolutionCase`
- `computer_use::MouseActionKind`
- `computer_use::UiTargetKind`
- `computer_use::RelativeAnchor`
- `computer_use::InteractionScenario`
- `computer_use::standard_resolution_cases`
- `computer_use::default_regression_scenarios`
- `computer_use::anchor_to_physical_pixel`
- `computer_use::input::preflight_report`
- `computer_use::input::click_point`
- `computer_use::input::mouse_button_action_point`
- `computer_use::input::move_mouse_relative`
- `computer_use::input::scroll_wheel`
- `computer_use::input::type_text`
- `computer_use::input::press_virtual_key`
- `computer_use::input::hold_virtual_key`
- `computer_use::input::send_virtual_key_combo`

## 输入后端策略

- 默认 `CLAW_MOUSE_BACKEND=auto`，本机发现 Interception DLL 时优先使用驱动级 `interception`，否则回退到 `sendinput`。
- 可显式设置 `CLAW_MOUSE_BACKEND=sendinput` 或 `CLAW_MOUSE_BACKEND=interception` 覆盖默认策略。
- Interception DLL 可通过 `CLAW_INTERCEPTION_DLL_PATH` 指定；未指定时会尝试发现 `%USERPROFILE%\.claw\vendor\interception\Interception\Interception\library\x64\interception.dll`。
- 鼠标设备 ID 默认 `11`，键盘设备 ID 默认 `1`，可通过 `CLAW_INTERCEPTION_MOUSE_DEVICE_ID` 和 `CLAW_INTERCEPTION_KEYBOARD_DEVICE_ID` 覆盖。

## 独立验证

```powershell
cargo check -p coolzhu-computer-use-core --offline
cargo test -p coolzhu-computer-use-core --offline
cargo run -p coolzhu-computer-use-core --bin coolzhu-computer-use-check --offline -- preflight
cargo run -p coolzhu-computer-use-core --bin coolzhu-computer-use-check --offline -- click 484 298 1
cargo run -p coolzhu-computer-use-core --bin coolzhu-computer-use-check --offline -- mouse-action right 484 298
```

临时 Aimlab 网格交互测试脚本放在主工作目录 `tmp` 下，仅用于本机调试，不提交 Gerrit：

```powershell
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File .\tmp\run_aimlab_core_input_test.ps1
```

## 接口变更审查点

- 新增鼠标/键盘动作时，需要补充独立 CLI 或 Aimlab 类交互回归测试。
- 修改坐标映射算法时，需要覆盖不同分辨率与 DPI 缩放矩阵。
- 修改输入后端策略时，需要同时验证 `sendinput` 和 `interception` 的失败/成功路径。
