//! 原生输入原语与**受控原生输入生命周期**。
//!
//! 本模块有两类入口，边界必须分清：
//!
//! 1. **受控入口**（`controlled_*`，见文件下半部分的"受控原生输入生命周期"）：点击、文本、
//!    滚动、按键、组合键、按下/抬起都走"输入前身份／权限／scope 校验 → 登记执行意图与释放
//!    义务 → 受监督地启动受控 helper → 阶段回执 → 取消／超时／失败收尾 → 静止与释放对账"。
//!    收尾策略与受控笔画（`input_stroke`）**共用** [`crate::cleanup`]，但两者是**不同机制**：
//!    各自独立的执行者、事实与验收结论，不能互相继承。"已覆盖的具体输入路径采用该策略；
//!    `input.rs` 尚需补齐，不能全称"原生输入已有四秒收尾"。
//! 2. **未受控原语**（本文件上半部分的 `diagnostic_click_point` / `diagnostic_type_text` / `diagnostic_scroll_wheel` /
//!    `diagnostic_press_virtual_key` / `diagnostic_send_virtual_key_combo` / `diagnostic_drag_point` 等）：保留给交互式与
//!    诊断调用（例如控制台的人工动作、`coolzhu-computer-use-check`）。它们**超时后不终止
//!    子进程、不做静止确认、不登记释放义务**，因此**不得**进入"已验收的自动输入集合"。

use std::env;
use std::io::Write;
use std::path::{Path as FsPath, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::Serialize;

use crate::cleanup::{
    unix_ms, CleanupDeadline, CleanupPolicy, CleanupReleaseStatus, HelperCleanupFacts,
};
use runtime::{ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus};
#[cfg(windows)]
pub(crate) use windows_process_guard::HelperPipeReadersAdmission;
#[cfg(all(windows, test))]
use windows_process_guard::PIPE_READERS_PER_HELPER;

/// 一次"清理运行（独立释放）"转用的读取器容量预留。
///
/// 它本身是 Windows 概念（`windows_process_guard` 的管道读取器容量）；受控输入在
/// 非 Windows 上完全不可用，因此那里退化为 `()`——接口与调用点保持同一形状，
/// 但**不会**在非 Windows 上被当作"有容量可用"来使用。
#[cfg(windows)]
pub(crate) type HelperCleanupReservation = windows_process_guard::HelperPipeCleanupReservation;
/// 见 Windows 版 [`HelperCleanupReservation`]。
#[cfg(not(windows))]
pub(crate) type HelperCleanupReservation = ();

#[path = "input_stroke.rs"]
mod stroke;
pub use stroke::{
    capture_window_image, controlled_drag_path, helper_failure_receipt, helper_success_receipt,
    partial_input_receipt, pre_input_receipt, sent_receipt, validate_stroke, HelperInputFacts,
    StrokeFailure, StrokeFailureKind, StrokeWindow,
};

const ENV_INPUT_BACKEND: &str = "CLAW_MOUSE_BACKEND";
const ENV_INTERCEPTION_DLL_PATH: &str = "CLAW_INTERCEPTION_DLL_PATH";
const ENV_INTERCEPTION_MOUSE_DEVICE_ID: &str = "CLAW_INTERCEPTION_MOUSE_DEVICE_ID";
const ENV_INTERCEPTION_KEYBOARD_DEVICE_ID: &str = "CLAW_INTERCEPTION_KEYBOARD_DEVICE_ID";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 原生输入后端。受控生命周期按它决定文本路径的释放义务（SendInput 不留按键义务，
/// Interception 逐字符按修饰键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputBackend {
    SendInput,
    Interception,
}

impl InputBackend {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SendInput => "sendinput",
            Self::Interception => "interception",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonAction {
    LeftClick,
    RightClick,
    LeftRightChord,
    DoubleClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MousePoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputBackendPreflightReport {
    pub backend: String,
    pub ready: bool,
    pub detail: String,
    pub interception_dll_path: Option<String>,
    pub interception_mouse_device_id: i32,
    pub interception_keyboard_device_id: i32,
}

// ---------------------------------------------------------------------------
// 未受控原语（交互式与诊断专用，**不属于已验收的自动输入集合**）
// ---------------------------------------------------------------------------
//
// 下面这组函数用 `run_powershell` 派线程等待 PowerShell：超时后子进程仍可能继续运行，
// 没有终止与静止确认，也不登记释放义务。自动输入（计算机使用的动作派发）必须走文件下半部分
// 的 `controlled_*`；只有控制台人工动作与 `bin/check.rs` 诊断可以继续用它们。

/// 未受控点击原语（见本文件顶部"未受控原语"说明）。
pub fn diagnostic_click_point(x: i32, y: i32, clicks: u32, timeout: Duration) -> Result<(), String> {
    if clicks > 1 {
        return diagnostic_mouse_button_action_point(x, y, MouseButtonAction::DoubleClick, timeout);
    }
    diagnostic_mouse_button_action_point(x, y, MouseButtonAction::LeftClick, timeout)
}

pub fn diagnostic_mouse_button_action_point(
    x: i32,
    y: i32,
    action: MouseButtonAction,
    timeout: Duration,
) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_mouse_button_action(x, y, action, timeout),
        InputBackend::Interception => interception_mouse_button_action(x, y, action, timeout),
    }
}

pub fn diagnostic_move_mouse_relative(dx: i32, dy: i32, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_move_mouse_relative(dx, dy, timeout),
        InputBackend::Interception => interception_move_mouse_relative(dx, dy, timeout),
    }
}

pub fn diagnostic_move_mouse_absolute(x: i32, y: i32, timeout: Duration) -> Result<(), String> {
    sendinput_move_mouse_absolute(x, y, timeout)
}

pub fn diagnostic_mouse_button_down_point(
    x: i32,
    y: i32,
    button: MouseButton,
    timeout: Duration,
) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_mouse_button_state(x, y, button, true, timeout),
        InputBackend::Interception => interception_mouse_button_state(x, y, button, true, timeout),
    }
}

pub fn diagnostic_mouse_button_up_point(
    x: i32,
    y: i32,
    button: MouseButton,
    timeout: Duration,
) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_mouse_button_state(x, y, button, false, timeout),
        InputBackend::Interception => interception_mouse_button_state(x, y, button, false, timeout),
    }
}

#[must_use]
pub fn drag_path(start: MousePoint, end: MousePoint, segments: u32) -> Vec<MousePoint> {
    if start == end {
        return vec![start];
    }

    let segments = segments.clamp(1, 128);
    let mut points = Vec::with_capacity(usize::try_from(segments).unwrap_or(1) + 1);
    for index in 0..=segments {
        let ratio = f64::from(index) / f64::from(segments);
        let x = f64::from(start.x) + f64::from(end.x - start.x) * ratio;
        let y = f64::from(start.y) + f64::from(end.y - start.y) * ratio;
        let point = MousePoint {
            x: x.round() as i32,
            y: y.round() as i32,
        };
        if points.last().copied() != Some(point) {
            points.push(point);
        }
    }
    points
}

pub fn diagnostic_drag_point(
    start: MousePoint,
    end: MousePoint,
    segments: u32,
    step_delay: Duration,
    timeout: Duration,
) -> Result<(), String> {
    let path = drag_path(start, end, segments);
    match active_backend() {
        InputBackend::SendInput => {
            sendinput_drag_path(&path, MouseButton::Left, step_delay, timeout)
        }
        InputBackend::Interception => {
            interception_drag_path(&path, MouseButton::Left, step_delay, timeout)
        }
    }
}

pub fn diagnostic_press_escape(timeout: Duration) -> Result<(), String> {
    diagnostic_press_virtual_key(0x1B, timeout)
}

pub fn diagnostic_scroll_wheel(delta: i32, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_scroll_wheel(delta, timeout),
        InputBackend::Interception => interception_scroll_wheel(delta, timeout),
    }
}

pub fn diagnostic_type_text(text: &str, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_type_text(text, timeout),
        InputBackend::Interception => interception_type_text(text, timeout),
    }
}

pub fn diagnostic_press_virtual_key(virtual_key: u8, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_press_virtual_key(virtual_key, timeout),
        InputBackend::Interception => interception_press_virtual_key(virtual_key, 70, timeout),
    }
}

pub fn diagnostic_hold_virtual_key(virtual_key: u8, hold_ms: u64, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_hold_virtual_key(virtual_key, hold_ms, timeout),
        InputBackend::Interception => interception_press_virtual_key(virtual_key, hold_ms, timeout),
    }
}

pub fn diagnostic_send_virtual_key_combo(virtual_keys: &[u8], timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_send_virtual_key_combo(virtual_keys, timeout),
        InputBackend::Interception => interception_send_virtual_key_combo(virtual_keys, timeout),
    }
}

#[must_use]
pub fn active_backend_name() -> &'static str {
    match active_backend() {
        InputBackend::SendInput => "sendinput",
        InputBackend::Interception => "interception",
    }
}

#[must_use]
pub fn preflight_report() -> InputBackendPreflightReport {
    let backend = active_backend();
    let dll_path = interception_dll_path();
    let mouse_device_id = interception_mouse_device_id();
    let keyboard_device_id = interception_keyboard_device_id();

    match backend {
        InputBackend::SendInput => InputBackendPreflightReport {
            backend: active_backend_name().to_string(),
            ready: true,
            detail: "当前使用 SendInput / keybd_event 路径。".to_string(),
            interception_dll_path: dll_path.map(|path| path.display().to_string()),
            interception_mouse_device_id: mouse_device_id,
            interception_keyboard_device_id: keyboard_device_id,
        },
        InputBackend::Interception => {
            let detail = match ensure_interception_dll() {
                Ok(dll) => match verify_interception_context(&dll) {
                    Ok(()) => "Interception DLL 已找到，底层上下文创建成功。".to_string(),
                    Err(error) => error,
                },
                Err(error) => error,
            };
            InputBackendPreflightReport {
                backend: active_backend_name().to_string(),
                ready: detail.contains("成功"),
                detail,
                interception_dll_path: dll_path.map(|path| path.display().to_string()),
                interception_mouse_device_id: mouse_device_id,
                interception_keyboard_device_id: keyboard_device_id,
            }
        }
    }
}

fn active_backend() -> InputBackend {
    let configured = env::var(ENV_INPUT_BACKEND)
        .unwrap_or_else(|_| "auto".to_string())
        .trim()
        .to_ascii_lowercase();
    let interception_dll = interception_dll_path();
    let interception_ready = configured == "auto"
        && interception_dll
            .as_ref()
            .is_some_and(|dll| verify_interception_context(dll).is_ok());
    select_backend(
        configured.as_str(),
        interception_dll.is_some(),
        interception_ready,
    )
}

fn select_backend(
    configured: &str,
    interception_dll_present: bool,
    interception_context_ready: bool,
) -> InputBackend {
    match configured {
        "interception" => InputBackend::Interception,
        "auto" if interception_dll_present && interception_context_ready => {
            InputBackend::Interception
        }
        _ => InputBackend::SendInput,
    }
}

fn interception_dll_path() -> Option<PathBuf> {
    if let Ok(path) = env::var(ENV_INTERCEPTION_DLL_PATH) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let candidate = env::var("USERPROFILE")
        .ok()
        .map(PathBuf::from)?
        .join(".claw")
        .join("vendor")
        .join("interception")
        .join("Interception")
        .join("Interception")
        .join("library")
        .join("x64")
        .join("interception.dll");
    candidate.is_file().then_some(candidate)
}

fn interception_mouse_device_id() -> i32 {
    env::var(ENV_INTERCEPTION_MOUSE_DEVICE_ID)
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|value| (11..=20).contains(value))
        .unwrap_or(11)
}

fn interception_keyboard_device_id() -> i32 {
    env::var(ENV_INTERCEPTION_KEYBOARD_DEVICE_ID)
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|value| (1..=10).contains(value))
        .unwrap_or(1)
}

fn sendinput_mouse_button_action(
    x: i32,
    y: i32,
    action: MouseButtonAction,
    timeout: Duration,
) -> Result<(), String> {
    let script = format!(
        "$ErrorActionPreference='Stop'; Add-Type -TypeDefinition @'\n{CLICK_NATIVE}\n'@\n{}",
        sendinput_click_body(x, y, action)
    );
    run_powershell(&script, timeout).map(|_| ())
}

const CLICK_NATIVE: &str = r#"using System;
using System.Runtime.InteropServices;
public struct INPUT { public uint type; public MOUSEINPUT mi; }
public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }
public static class MouseOps {
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern uint SendInput(uint count,INPUT[] inputs,int size);
}"#;

fn sendinput_click_body(x: i32, y: i32, action: MouseButtonAction) -> String {
    format!(
        r#"
$ErrorActionPreference='Stop'
if (-not [MouseOps]::SetCursorPos({x},{y})) {{ throw 'cursor_move_failed: SetCursorPos returned false' }}
Start-Sleep -Milliseconds 160
$script:clickLeftHeld=$false; $script:clickRightHeld=$false
function Send-InputFlag([uint32]$flag) {{
 if($flag -eq 2) {{ $script:clickLeftHeld=$true }}
 if($flag -eq 8) {{ $script:clickRightHeld=$true }}
 # PowerShell 读取嵌套值类型会得到副本，必须完整写回 mi，避免发送 flags=0 的空输入。
 $packet=New-Object INPUT; $packet.type=0; $mousePacket=New-Object MOUSEINPUT; $mousePacket.dwFlags=$flag; $packet.mi=$mousePacket
 $sent=[MouseOps]::SendInput(1,@($packet),[Runtime.InteropServices.Marshal]::SizeOf([type]'INPUT'))
 if($sent -ne 1) {{ throw "send_input_failed: flag=$flag sent=$sent" }}
 if($flag -eq 4) {{ $script:clickLeftHeld=$false }}
 if($flag -eq 16) {{ $script:clickRightHeld=$false }}
}}
$clickFailure=$null; $releaseFailure=$false
try {{ {sequence} }} catch {{ $clickFailure=$_ }}
finally {{
 foreach($releaseFlag in @(4,16)) {{
  $held=if($releaseFlag -eq 4) {{$script:clickLeftHeld}} else {{$script:clickRightHeld}}
  if($held) {{
   $released=$false
   for($retry=0;$retry -lt 3;$retry++) {{ try {{ Send-InputFlag $releaseFlag; $released=$true; break }} catch {{ Start-Sleep -Milliseconds 10 }} }}
   if(-not $released) {{ $releaseFailure=$true }}
  }}
 }}
}}
if($releaseFailure) {{ throw 'mouse_release_failed: click could not release its pressed button' }}
if($null -ne $clickFailure) {{ throw $clickFailure }}
"#,
        sequence = sendinput_mouse_button_sequence(action)
    )
}

fn sendinput_mouse_button_sequence(action: MouseButtonAction) -> &'static str {
    match action {
        MouseButtonAction::LeftClick => {
            "Send-InputFlag 0x0002; Start-Sleep -Milliseconds 45; Send-InputFlag 0x0004; Start-Sleep -Milliseconds 120;"
        }
        MouseButtonAction::RightClick => {
            "Send-InputFlag 0x0008; Start-Sleep -Milliseconds 45; Send-InputFlag 0x0010; Start-Sleep -Milliseconds 120;"
        }
        MouseButtonAction::LeftRightChord => {
            "Send-InputFlag 0x0002; Start-Sleep -Milliseconds 25; Send-InputFlag 0x0008; Start-Sleep -Milliseconds 70; Send-InputFlag 0x0010; Start-Sleep -Milliseconds 25; Send-InputFlag 0x0004; Start-Sleep -Milliseconds 120;"
        }
        MouseButtonAction::DoubleClick => {
            "Send-InputFlag 0x0002; Start-Sleep -Milliseconds 35; Send-InputFlag 0x0004; Start-Sleep -Milliseconds 90; Send-InputFlag 0x0002; Start-Sleep -Milliseconds 35; Send-InputFlag 0x0004; Start-Sleep -Milliseconds 120;"
        }
    }
}

fn sendinput_mouse_button_state(
    x: i32,
    y: i32,
    button: MouseButton,
    is_down: bool,
    timeout: Duration,
) -> Result<(), String> {
    run_powershell(
        &format!(
            "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic struct INPUT {{ public uint type; public MOUSEINPUT mi; }}\npublic struct MOUSEINPUT {{ public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }}\npublic static class MouseOps {{\n    [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y);\n    [DllImport(\"user32.dll\")] public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);\n}}\n'@; \
             Add-Type $signature; \
             [MouseOps]::SetCursorPos({x}, {y}) | Out-Null; \
             Start-Sleep -Milliseconds 80; \
             $input = New-Object INPUT; \
             $input.type = 0; \
             $input.mi = New-Object MOUSEINPUT; \
             $input.mi.dwFlags = {flag}; \
             [void][MouseOps]::SendInput(1, @($input), [System.Runtime.InteropServices.Marshal]::SizeOf([type]'INPUT'));",
            flag = sendinput_mouse_button_state_flag(button, is_down),
        ),
        timeout,
    )
    .map(|_| ())
}

fn sendinput_mouse_button_state_flag(button: MouseButton, is_down: bool) -> u32 {
    match (button, is_down) {
        (MouseButton::Left, true) => 0x0002,
        (MouseButton::Left, false) => 0x0004,
        (MouseButton::Right, true) => 0x0008,
        (MouseButton::Right, false) => 0x0010,
    }
}

fn sendinput_move_mouse_relative(dx: i32, dy: i32, timeout: Duration) -> Result<(), String> {
    run_powershell(
        &format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class MouseOps {{\n    [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y);\n}}\n'@; \
             Add-Type $signature; \
             $pos = [System.Windows.Forms.Cursor]::Position; \
             [MouseOps]::SetCursorPos($pos.X + ({dx}), $pos.Y + ({dy})) | Out-Null;"
        ),
        timeout,
    )
    .map(|_| ())
}

fn sendinput_move_mouse_absolute(x: i32, y: i32, timeout: Duration) -> Result<(), String> {
    run_powershell(
        &format!(
            "$signature = @'\nusing System.Runtime.InteropServices;\npublic static class MouseOps {{\n    [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y);\n}}\n'@; \
             Add-Type $signature; \
             [MouseOps]::SetCursorPos({x}, {y}) | Out-Null;"
        ),
        timeout,
    )
    .map(|_| ())
}

fn sendinput_drag_path(
    path: &[MousePoint],
    button: MouseButton,
    step_delay: Duration,
    timeout: Duration,
) -> Result<(), String> {
    let Some(start) = path.first().copied() else {
        return Err("drag path requires at least one point".to_string());
    };
    let Some(end) = path.last().copied() else {
        return Err("drag path requires an end point".to_string());
    };
    let points = powershell_point_array(path.iter().skip(1).copied());
    let step_delay_ms = step_delay.as_millis().clamp(1, 250);
    run_powershell(
        &format!(
            "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic struct INPUT {{ public uint type; public MOUSEINPUT mi; }}\npublic struct MOUSEINPUT {{ public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }}\npublic static class MouseOps {{\n    [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y);\n    [DllImport(\"user32.dll\")] public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);\n}}\n'@; \
             Add-Type $signature; \
             function Send-InputFlag([uint32]$flag) {{ \
                 $input = New-Object INPUT; \
                 $input.type = 0; \
                 $input.mi = New-Object MOUSEINPUT; \
                 $input.mi.dwFlags = $flag; \
                 [void][MouseOps]::SendInput(1, @($input), [System.Runtime.InteropServices.Marshal]::SizeOf([type]'INPUT')); \
             }} \
             [MouseOps]::SetCursorPos({start_x}, {start_y}) | Out-Null; \
             Start-Sleep -Milliseconds 80; \
             Send-InputFlag {down_flag}; \
             Start-Sleep -Milliseconds 60; \
             $points = @({points}); \
             foreach ($point in $points) {{ \
                 [MouseOps]::SetCursorPos([int]$point[0], [int]$point[1]) | Out-Null; \
                 Start-Sleep -Milliseconds {step_delay_ms}; \
             }} \
             [MouseOps]::SetCursorPos({end_x}, {end_y}) | Out-Null; \
             Start-Sleep -Milliseconds 40; \
             Send-InputFlag {up_flag};",
            start_x = start.x,
            start_y = start.y,
            end_x = end.x,
            end_y = end.y,
            down_flag = sendinput_mouse_button_state_flag(button, true),
            up_flag = sendinput_mouse_button_state_flag(button, false),
        ),
        timeout,
    )
    .map(|_| ())
}

fn sendinput_scroll_wheel(delta: i32, timeout: Duration) -> Result<(), String> {
    run_powershell(
        &format!(
            "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class MouseOps {{\n    [DllImport(\"user32.dll\")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);\n}}\n'@; \
             Add-Type $signature; \
             [MouseOps]::mouse_event(0x0800, 0, 0, [uint32]({delta}), [UIntPtr]::Zero);"
        ),
        timeout,
    )
    .map(|_| ())
}

fn sendinput_type_text(text: &str, timeout: Duration) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    run_powershell(&sendinput_unicode_text_script(text), timeout).map(|_| ())
}

fn sendinput_unicode_text_script(text: &str) -> String {
    let units = text
        .encode_utf16()
        .map(|unit| unit.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "$signature = @'\nusing System;\nusing System.ComponentModel;\nusing System.Runtime.InteropServices;\n[StructLayout(LayoutKind.Sequential)] public struct INPUT {{ public uint type; public INPUTUNION U; }}\n[StructLayout(LayoutKind.Explicit)] public struct INPUTUNION {{ [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; [FieldOffset(0)] public HARDWAREINPUT hi; }}\n[StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {{ public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }}\n[StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {{ public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }}\n[StructLayout(LayoutKind.Sequential)] public struct HARDWAREINPUT {{ public uint uMsg; public ushort wParamL; public ushort wParamH; }}\npublic static class KeyboardOps {{\n    public const uint INPUT_KEYBOARD = 1;\n    public const uint KEYEVENTF_KEYUP = 0x0002;\n    public const uint KEYEVENTF_UNICODE = 0x0004;\n    [DllImport(\"user32.dll\", SetLastError=true)] public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);\n    public static void SendUnicode(ushort scan) {{\n        INPUT down = new INPUT();\n        down.type = INPUT_KEYBOARD;\n        down.U.ki.wScan = scan;\n        down.U.ki.dwFlags = KEYEVENTF_UNICODE;\n        INPUT up = new INPUT();\n        up.type = INPUT_KEYBOARD;\n        up.U.ki.wScan = scan;\n        up.U.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;\n        INPUT[] inputs = new INPUT[] {{ down, up }};\n        uint sent = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));\n        if (sent != inputs.Length) {{ throw new Win32Exception(Marshal.GetLastWin32Error()); }}\n    }}\n}}\n'@; \
         Add-Type $signature; \
         $units = [uint16[]]@({units}); \
         foreach ($unit in $units) {{ \
             [KeyboardOps]::SendUnicode($unit); \
             Start-Sleep -Milliseconds 2; \
         }}"
    )
}

fn sendinput_press_virtual_key(virtual_key: u8, timeout: Duration) -> Result<(), String> {
    sendinput_hold_virtual_key(virtual_key, 70, timeout)
}

fn sendinput_hold_virtual_key(
    virtual_key: u8,
    hold_ms: u64,
    timeout: Duration,
) -> Result<(), String> {
    run_powershell(
        &format!(
            "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class KeyboardOps {{\n    [DllImport(\"user32.dll\")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);\n}}\n'@; \
             Add-Type $signature; \
             [KeyboardOps]::keybd_event([byte]0x{virtual_key:02X}, 0, 0, [UIntPtr]::Zero); \
             Start-Sleep -Milliseconds {hold_ms}; \
             [KeyboardOps]::keybd_event([byte]0x{virtual_key:02X}, 0, 2, [UIntPtr]::Zero);"
        ),
        timeout,
    )
    .map(|_| ())
}

fn sendinput_send_virtual_key_combo(virtual_keys: &[u8], timeout: Duration) -> Result<(), String> {
    if virtual_keys.is_empty() {
        return Err("virtual key combo requires at least one key".to_string());
    }

    let mut down_lines = Vec::new();
    let mut up_lines = Vec::new();
    for key in virtual_keys {
        down_lines.push(format!(
            "[KeyboardOps]::keybd_event([byte]0x{key:02X}, 0, 0, [UIntPtr]::Zero);"
        ));
        up_lines.push(format!(
            "[KeyboardOps]::keybd_event([byte]0x{key:02X}, 0, 2, [UIntPtr]::Zero);"
        ));
    }
    up_lines.reverse();

    run_powershell(
        &format!(
            "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class KeyboardOps {{\n    [DllImport(\"user32.dll\")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);\n}}\n'@; \
             Add-Type $signature; \
             {} \
             Start-Sleep -Milliseconds 80; \
             {}",
            down_lines.join(" "),
            up_lines.join(" ")
        ),
        timeout,
    )
    .map(|_| ())
}

fn interception_mouse_button_action(
    x: i32,
    y: i32,
    action: MouseButtonAction,
    timeout: Duration,
) -> Result<(), String> {
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let script = format!(
        "$env:PATH = '{dll_dir};' + $env:PATH; \
         $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\n[StructLayout(LayoutKind.Sequential)] public struct InterceptionMouseStroke {{ public ushort state; public ushort flags; public short rolling; public int x; public int y; public uint information; }}\npublic static class MouseOps {{ [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y); }}\npublic static class InterceptionNative {{ [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context(); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionMouseStroke[] stroke, uint nstroke); }}\n'@; \
         Add-Type $signature; \
         $context = [InterceptionNative]::interception_create_context(); \
         if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context'; }} \
         try {{ \
             [MouseOps]::SetCursorPos({x}, {y}) | Out-Null; \
             Start-Sleep -Milliseconds 120; \
             function Send-MouseState([uint16]$state) {{ \
                 $stroke = New-Object InterceptionMouseStroke; \
                 $stroke.state = $state; \
                 if ([InterceptionNative]::interception_send($context, {device}, @($stroke), 1) -lt 1) {{ throw \"failed to send interception mouse state $state\"; }} \
             }} \
             {sequence} \
         }} finally {{ \
             [InterceptionNative]::interception_destroy_context($context); \
         }}",
        device = interception_mouse_device_id(),
        sequence = interception_mouse_button_sequence(action),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_mouse_button_sequence(action: MouseButtonAction) -> &'static str {
    match action {
        MouseButtonAction::LeftClick => {
            "Send-MouseState 1; Start-Sleep -Milliseconds 35; Send-MouseState 2; Start-Sleep -Milliseconds 120;"
        }
        MouseButtonAction::RightClick => {
            "Send-MouseState 4; Start-Sleep -Milliseconds 35; Send-MouseState 8; Start-Sleep -Milliseconds 120;"
        }
        MouseButtonAction::LeftRightChord => {
            "Send-MouseState 1; Start-Sleep -Milliseconds 25; Send-MouseState 4; Start-Sleep -Milliseconds 70; Send-MouseState 8; Start-Sleep -Milliseconds 25; Send-MouseState 2; Start-Sleep -Milliseconds 120;"
        }
        MouseButtonAction::DoubleClick => {
            "Send-MouseState 1; Start-Sleep -Milliseconds 35; Send-MouseState 2; Start-Sleep -Milliseconds 90; Send-MouseState 1; Start-Sleep -Milliseconds 35; Send-MouseState 2; Start-Sleep -Milliseconds 120;"
        }
    }
}

fn interception_mouse_button_state(
    x: i32,
    y: i32,
    button: MouseButton,
    is_down: bool,
    timeout: Duration,
) -> Result<(), String> {
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let script = format!(
        "$env:PATH = '{dll_dir};' + $env:PATH; \
         $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\n[StructLayout(LayoutKind.Sequential)] public struct InterceptionMouseStroke {{ public ushort state; public ushort flags; public short rolling; public int x; public int y; public uint information; }}\npublic static class MouseOps {{ [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y); }}\npublic static class InterceptionNative {{ [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context(); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionMouseStroke[] stroke, uint nstroke); }}\n'@; \
         Add-Type $signature; \
         $context = [InterceptionNative]::interception_create_context(); \
         if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context'; }} \
         try {{ \
             [MouseOps]::SetCursorPos({x}, {y}) | Out-Null; \
             Start-Sleep -Milliseconds 80; \
             $stroke = New-Object InterceptionMouseStroke; \
             $stroke.state = {state}; \
             if ([InterceptionNative]::interception_send($context, {device}, @($stroke), 1) -lt 1) {{ throw 'failed to send interception mouse state'; }} \
         }} finally {{ \
             [InterceptionNative]::interception_destroy_context($context); \
         }}",
        device = interception_mouse_device_id(),
        state = interception_mouse_button_state_code(button, is_down),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_mouse_button_state_code(button: MouseButton, is_down: bool) -> u16 {
    match (button, is_down) {
        (MouseButton::Left, true) => 1,
        (MouseButton::Left, false) => 2,
        (MouseButton::Right, true) => 4,
        (MouseButton::Right, false) => 8,
    }
}

fn interception_move_mouse_relative(dx: i32, dy: i32, timeout: Duration) -> Result<(), String> {
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let script = format!(
        "$env:PATH = '{dll_dir};' + $env:PATH; \
         $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\n[StructLayout(LayoutKind.Sequential)] public struct InterceptionMouseStroke {{ public ushort state; public ushort flags; public short rolling; public int x; public int y; public uint information; }}\npublic static class InterceptionNative {{ [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context(); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionMouseStroke[] stroke, uint nstroke); }}\n'@; \
         Add-Type $signature; \
         $context = [InterceptionNative]::interception_create_context(); \
         if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context'; }} \
         try {{ \
             $move = New-Object InterceptionMouseStroke; \
             $move.flags = 0; \
             $move.x = {dx}; \
             $move.y = {dy}; \
             if ([InterceptionNative]::interception_send($context, {device}, @($move), 1) -lt 1) {{ throw 'failed to send interception relative move'; }} \
         }} finally {{ \
             [InterceptionNative]::interception_destroy_context($context); \
         }}",
        device = interception_mouse_device_id(),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_drag_path(
    path: &[MousePoint],
    button: MouseButton,
    step_delay: Duration,
    timeout: Duration,
) -> Result<(), String> {
    let Some(start) = path.first().copied() else {
        return Err("drag path requires at least one point".to_string());
    };
    let Some(end) = path.last().copied() else {
        return Err("drag path requires an end point".to_string());
    };
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let points = powershell_point_array(path.iter().skip(1).copied());
    let step_delay_ms = step_delay.as_millis().clamp(1, 250);
    let script = format!(
        "$env:PATH = '{dll_dir};' + $env:PATH; \
         $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\n[StructLayout(LayoutKind.Sequential)] public struct InterceptionMouseStroke {{ public ushort state; public ushort flags; public short rolling; public int x; public int y; public uint information; }}\npublic static class MouseOps {{ [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int x, int y); }}\npublic static class InterceptionNative {{ [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context(); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionMouseStroke[] stroke, uint nstroke); }}\n'@; \
         Add-Type $signature; \
         $context = [InterceptionNative]::interception_create_context(); \
         if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context'; }} \
         try {{ \
             function Send-MouseState([uint16]$state) {{ \
                 $stroke = New-Object InterceptionMouseStroke; \
                 $stroke.state = $state; \
                 if ([InterceptionNative]::interception_send($context, {device}, @($stroke), 1) -lt 1) {{ throw \"failed to send interception mouse state $state\"; }} \
             }} \
             [MouseOps]::SetCursorPos({start_x}, {start_y}) | Out-Null; \
             Start-Sleep -Milliseconds 80; \
             Send-MouseState {down_state}; \
             Start-Sleep -Milliseconds 60; \
             $points = @({points}); \
             foreach ($point in $points) {{ \
                 [MouseOps]::SetCursorPos([int]$point[0], [int]$point[1]) | Out-Null; \
                 Start-Sleep -Milliseconds {step_delay_ms}; \
             }} \
             [MouseOps]::SetCursorPos({end_x}, {end_y}) | Out-Null; \
             Start-Sleep -Milliseconds 40; \
             Send-MouseState {up_state}; \
         }} finally {{ \
             [InterceptionNative]::interception_destroy_context($context); \
         }}",
        device = interception_mouse_device_id(),
        start_x = start.x,
        start_y = start.y,
        end_x = end.x,
        end_y = end.y,
        down_state = interception_mouse_button_state_code(button, true),
        up_state = interception_mouse_button_state_code(button, false),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_scroll_wheel(delta: i32, timeout: Duration) -> Result<(), String> {
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let script = format!(
        "$env:PATH = '{dll_dir};' + $env:PATH; \
         $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\n[StructLayout(LayoutKind.Sequential)] public struct InterceptionMouseStroke {{ public ushort state; public ushort flags; public short rolling; public int x; public int y; public uint information; }}\npublic static class InterceptionNative {{ [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context(); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionMouseStroke[] stroke, uint nstroke); }}\n'@; \
         Add-Type $signature; \
         $context = [InterceptionNative]::interception_create_context(); \
         if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context'; }} \
         try {{ \
             $wheel = New-Object InterceptionMouseStroke; \
             $wheel.state = 1024; \
             $wheel.rolling = [int16]({delta}); \
             if ([InterceptionNative]::interception_send($context, {device}, @($wheel), 1) -lt 1) {{ throw 'failed to send interception wheel'; }} \
         }} finally {{ \
             [InterceptionNative]::interception_destroy_context($context); \
         }}",
        device = interception_mouse_device_id(),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_type_text(text: &str, timeout: Duration) -> Result<(), String> {
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let escaped = escape_powershell_single_quoted(text);
    let script = format!(
        r#"$env:PATH = '{dll_dir};' + $env:PATH;
$signature = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct InterceptionKeyStroke {{ public ushort code; public ushort state; public uint information; }}
public static class KeyboardOps {{
    [DllImport("user32.dll")] public static extern ushort MapVirtualKeyW(uint uCode, uint uMapType);
    [DllImport("user32.dll")] public static extern short VkKeyScanW(char ch);
}}
public static class InterceptionNative {{
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context();
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context);
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionKeyStroke[] stroke, uint nstroke);
}}
'@
Add-Type $signature
$context = [InterceptionNative]::interception_create_context()
if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context' }}
$device = {device}
$upFlag = 0x01
$e0Flag = 0x02
function Test-ExtendedKey([byte]$vk) {{
    switch ($vk) {{
        0x21 {{ return $true }}
        0x22 {{ return $true }}
        0x23 {{ return $true }}
        0x24 {{ return $true }}
        0x25 {{ return $true }}
        0x26 {{ return $true }}
        0x27 {{ return $true }}
        0x28 {{ return $true }}
        0x2D {{ return $true }}
        0x2E {{ return $true }}
        0x5B {{ return $true }}
        0x5C {{ return $true }}
        default {{ return $false }}
    }}
}}
function Send-KeyStroke([byte]$vk, [bool]$isUp) {{
    $scan = [KeyboardOps]::MapVirtualKeyW($vk, 0)
    if ($scan -eq 0) {{ throw "unable to map virtual key $vk" }}
    $stroke = New-Object InterceptionKeyStroke
    $stroke.code = [uint16]$scan
    $state = if ($isUp) {{ $upFlag }} else {{ 0 }}
    if (Test-ExtendedKey $vk) {{ $state = $state -bor $e0Flag }}
    $stroke.state = [uint16]$state
    if ([InterceptionNative]::interception_send($context, $device, @($stroke), 1) -lt 1) {{
        throw "failed to send key stroke for virtual key $vk"
    }}
}}
try {{
    $text = '{escaped}'
    foreach ($ch in $text.ToCharArray()) {{
        $vkInfo = [KeyboardOps]::VkKeyScanW($ch)
        if ($vkInfo -eq -1) {{ throw "unsupported character: $ch" }}
        $vk = [byte]($vkInfo -band 0xFF)
        $shiftState = (($vkInfo -shr 8) -band 0xFF)
        $modifiers = @()
        if (($shiftState -band 1) -ne 0) {{ $modifiers += 0x10 }}
        if (($shiftState -band 2) -ne 0) {{ $modifiers += 0x11 }}
        if (($shiftState -band 4) -ne 0) {{ $modifiers += 0x12 }}
        foreach ($modifier in $modifiers) {{
            Send-KeyStroke -vk ([byte]$modifier) -isUp $false
            Start-Sleep -Milliseconds 8
        }}
        Send-KeyStroke -vk $vk -isUp $false
        Start-Sleep -Milliseconds 18
        Send-KeyStroke -vk $vk -isUp $true
        [array]::Reverse($modifiers)
        foreach ($modifier in $modifiers) {{
            Start-Sleep -Milliseconds 8
            Send-KeyStroke -vk ([byte]$modifier) -isUp $true
        }}
        Start-Sleep -Milliseconds 24
    }}
}} finally {{
    [InterceptionNative]::interception_destroy_context($context)
}}"#,
        device = interception_keyboard_device_id(),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_press_virtual_key(
    virtual_key: u8,
    hold_ms: u64,
    timeout: Duration,
) -> Result<(), String> {
    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let script = format!(
        r#"$env:PATH = '{dll_dir};' + $env:PATH;
$signature = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct InterceptionKeyStroke {{ public ushort code; public ushort state; public uint information; }}
public static class KeyboardOps {{
    [DllImport("user32.dll")] public static extern ushort MapVirtualKeyW(uint uCode, uint uMapType);
}}
public static class InterceptionNative {{
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context();
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context);
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionKeyStroke[] stroke, uint nstroke);
}}
'@
Add-Type $signature
$context = [InterceptionNative]::interception_create_context()
if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context' }}
$scan = [KeyboardOps]::MapVirtualKeyW({virtual_key}, 0)
if ($scan -eq 0) {{ throw 'failed to map virtual key' }}
$upFlag = 0x01
$e0Flag = 0x02
$stateBase = 0
switch ({virtual_key}) {{
    0x21 {{ $stateBase = $e0Flag }}
    0x22 {{ $stateBase = $e0Flag }}
    0x23 {{ $stateBase = $e0Flag }}
    0x24 {{ $stateBase = $e0Flag }}
    0x25 {{ $stateBase = $e0Flag }}
    0x26 {{ $stateBase = $e0Flag }}
    0x27 {{ $stateBase = $e0Flag }}
    0x28 {{ $stateBase = $e0Flag }}
    0x2D {{ $stateBase = $e0Flag }}
    0x2E {{ $stateBase = $e0Flag }}
    0x5B {{ $stateBase = $e0Flag }}
    0x5C {{ $stateBase = $e0Flag }}
}}
try {{
    $down = New-Object InterceptionKeyStroke
    $down.code = [uint16]$scan
    $down.state = [uint16]$stateBase
    $up = New-Object InterceptionKeyStroke
    $up.code = [uint16]$scan
    $up.state = [uint16]($stateBase -bor $upFlag)
    if ([InterceptionNative]::interception_send($context, {device}, @($down), 1) -lt 1) {{ throw 'failed to send key down' }}
    Start-Sleep -Milliseconds {hold_ms}
    if ([InterceptionNative]::interception_send($context, {device}, @($up), 1) -lt 1) {{ throw 'failed to send key up' }}
}} finally {{
    [InterceptionNative]::interception_destroy_context($context)
}}"#,
        device = interception_keyboard_device_id(),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn interception_send_virtual_key_combo(
    virtual_keys: &[u8],
    timeout: Duration,
) -> Result<(), String> {
    if virtual_keys.is_empty() {
        return Err("virtual key combo requires at least one key".to_string());
    }

    let dll = ensure_interception_dll()?;
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let key_list = virtual_keys
        .iter()
        .map(|key| key.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        r#"$env:PATH = '{dll_dir};' + $env:PATH;
$signature = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct InterceptionKeyStroke {{ public ushort code; public ushort state; public uint information; }}
public static class KeyboardOps {{
    [DllImport("user32.dll")] public static extern ushort MapVirtualKeyW(uint uCode, uint uMapType);
}}
public static class InterceptionNative {{
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context();
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context);
    [DllImport("interception.dll", CallingConvention=CallingConvention.Cdecl)] public static extern int interception_send(IntPtr context, int device, InterceptionKeyStroke[] stroke, uint nstroke);
}}
'@
Add-Type $signature
$context = [InterceptionNative]::interception_create_context()
if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context' }}
$keys = @({key_list})
$upFlag = 0x01
$e0Flag = 0x02
function Test-ExtendedKey([byte]$vk) {{
    switch ($vk) {{
        0x21 {{ return $true }}
        0x22 {{ return $true }}
        0x23 {{ return $true }}
        0x24 {{ return $true }}
        0x25 {{ return $true }}
        0x26 {{ return $true }}
        0x27 {{ return $true }}
        0x28 {{ return $true }}
        0x2D {{ return $true }}
        0x2E {{ return $true }}
        0x5B {{ return $true }}
        0x5C {{ return $true }}
        default {{ return $false }}
    }}
}}
function Send-KeyStroke([byte]$vk, [bool]$isUp) {{
    $scan = [KeyboardOps]::MapVirtualKeyW($vk, 0)
    if ($scan -eq 0) {{ throw "unable to map virtual key $vk" }}
    $stroke = New-Object InterceptionKeyStroke
    $stroke.code = [uint16]$scan
    $state = if ($isUp) {{ $upFlag }} else {{ 0 }}
    if (Test-ExtendedKey $vk) {{ $state = $state -bor $e0Flag }}
    $stroke.state = [uint16]$state
    if ([InterceptionNative]::interception_send($context, {device}, @($stroke), 1) -lt 1) {{
        throw "failed to send combo key stroke for virtual key $vk"
    }}
}}
try {{
    foreach ($vk in $keys) {{
        Send-KeyStroke -vk ([byte]$vk) -isUp $false
        Start-Sleep -Milliseconds 18
    }}
    Start-Sleep -Milliseconds 90
    [array]::Reverse($keys)
    foreach ($vk in $keys) {{
        Send-KeyStroke -vk ([byte]$vk) -isUp $true
        Start-Sleep -Milliseconds 18
    }}
}} finally {{
    [InterceptionNative]::interception_destroy_context($context)
}}"#,
        device = interception_keyboard_device_id(),
    );
    run_powershell(&script, timeout).map(|_| ())
}

fn ensure_interception_dll() -> Result<PathBuf, String> {
    let dll = interception_dll_path().ok_or_else(|| {
        "interception backend requires a configured interception.dll path".to_string()
    })?;
    if !dll.is_file() {
        return Err(format!("interception.dll not found: {}", dll.display()));
    }
    Ok(dll)
}

fn verify_interception_context(dll: &PathBuf) -> Result<(), String> {
    let dll_dir = escape_powershell_single_quoted(
        dll.parent()
            .ok_or_else(|| "interception.dll parent directory not found".to_string())?
            .to_string_lossy()
            .as_ref(),
    );
    let script = format!(
        "$env:PATH = '{dll_dir};' + $env:PATH; \
         $signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class InterceptionNative {{ [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern IntPtr interception_create_context(); [DllImport(\"interception.dll\", CallingConvention=CallingConvention.Cdecl)] public static extern void interception_destroy_context(IntPtr context); }}\n'@; \
         Add-Type $signature; \
         $context = [InterceptionNative]::interception_create_context(); \
         if ($context -eq [IntPtr]::Zero) {{ throw 'failed to create interception context'; }} \
         [InterceptionNative]::interception_destroy_context($context); \
         'OK'"
    );
    match run_powershell(&script, Duration::from_secs(6)) {
        Ok(output) if output.trim().eq_ignore_ascii_case("OK") => Ok(()),
        Ok(output) => Err(format!("Interception 上下文返回异常输出: {output}")),
        Err(error) => Err(format!("Interception 预检失败: {error}")),
    }
}

fn run_powershell(script: &str, timeout: Duration) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    let script = format!(
        "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::UTF8; \
         [Console]::InputEncoding = [System.Text.UTF8Encoding]::UTF8; \
         $dpiSignature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class DpiAwareness {{\n    [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware();\n}}\n'@; \
         Add-Type $dpiSignature; \
         [DpiAwareness]::SetProcessDPIAware() | Out-Null; \
         {script}"
    );
    thread::spawn(move || {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Sta", "-Command", &script]);
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        let result = command
            .output()
            .map_err(|error| format!("failed to launch PowerShell for input backend: {error}"));
        let _ = tx.send(result);
    });

    let output = rx
        .recv_timeout(timeout)
        .map_err(|_| {
            format!(
                "input backend PowerShell timed out after {}s",
                timeout.as_secs()
            )
        })?
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "input backend PowerShell failed: {stdout} {stderr}"
        ))
    }
}

fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn powershell_point_array(points: impl Iterator<Item = MousePoint>) -> String {
    points
        .map(|point| format!("@({}, {})", point.x, point.y))
        .collect::<Vec<_>>()
        .join(",")
}

// ===========================================================================
// 受控原生输入生命周期（RPR-04d）
// ===========================================================================
//
// 被覆盖的可达路径：点击 / 双击 / 右键点击 / 左右和弦、文本输入、滚动、按键、组合键，
// 以及显式的按下与抬起。每一步都按同一条链路走：
//
//   输入前身份／权限／scope 校验
//     → 登记执行意图与本次可能的释放义务（`ReleaseObligation`）
//     → 受监督地启动受控 helper（固定入口，无脚本执行入口）
//     → 开始输入 → 阶段回执（进度文件，逐步汇报）
//     → 取消／超时／失败的收尾（协作退出 ≤2 秒、总窗口 ≤4 秒，截止时间只固定一次）
//     → 静止确认与释放对账（独立释放最多一次，只释放登记过的按钮/按键）
//
// 与受控笔画的关系：**共用 `crate::cleanup` 的同一套收尾策略**，不新增第二套 timeout runner；
// 但执行者、事实与验收结论各自独立——笔画路径的结论不能搬到这条路径上。

/// 修饰键的虚拟键码：Interception 文本路径会逐个按下/抬起它们，因此必须登记为释放义务。
pub const VIRTUAL_KEY_SHIFT: u8 = 0x10;
pub const VIRTUAL_KEY_CONTROL: u8 = 0x11;
pub const VIRTUAL_KEY_ALT: u8 = 0x12;

/// 文本输入的 UTF-16 单元上限（与既有文本路径的上限一致）。
pub const MAX_NATIVE_TEXT_UNITS: usize = 4_000;
/// helper 自报的注入步数上限：超出只能是伪造或损坏。
const MAX_NATIVE_INJECTED_STEPS: u32 = 4_096;
/// helper 成功完成整条序列时打印的标记。
const NATIVE_HELPER_SUCCESS_MARKER: &str = "native-input:ok";
/// 阶段回执的轮询间隔。
const NATIVE_PROGRESS_POLL_MS: u64 = 15;

/// 受控原生输入的收尾策略：与受控笔画**同源**，不复制、不改写任何数值。
///
/// 三个数值的唯一集中定义点是 [`crate::cleanup::CleanupPolicy`]（协作退出 ≤2 秒、
/// 自动收尾总窗口 ≤4 秒、独立释放等待 ≤ `min(2 秒, 剩余收尾时间)`）。
#[must_use]
fn native_cleanup_policy() -> CleanupPolicy {
    CleanupPolicy::default()
}

/// 一次受控原生输入**开始之前**登记的释放义务：按本次实际操作会用到的按钮与按键登记。
///
/// 既不是"统一抬整个键盘"，也不是"只处理左键"：右键点击登记右键、左右和弦登记两个键、
/// 组合键登记组合里的每个键、单键登记那一个键；滚动与 SendInput 文本登记空集合
/// （它们不会留下按住状态），Interception 文本登记它可能按下的三个修饰键。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseObligation {
    buttons: Vec<MouseButton>,
    keys: Vec<u8>,
}

impl ReleaseObligation {
    /// 没有释放义务。
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// 按鼠标键登记（去重，保留顺序）。
    #[must_use]
    pub fn mouse_buttons(buttons: &[MouseButton]) -> Self {
        Self {
            buttons: dedup_buttons(buttons),
            keys: Vec::new(),
        }
    }

    /// 按虚拟键登记（去重，保留顺序）。
    #[must_use]
    pub fn virtual_keys(keys: &[u8]) -> Self {
        Self {
            buttons: Vec::new(),
            keys: dedup_keys(keys),
        }
    }

    /// 鼠标键与虚拟键一起登记。
    #[must_use]
    pub fn mouse_buttons_and_keys(buttons: &[MouseButton], keys: &[u8]) -> Self {
        Self {
            buttons: dedup_buttons(buttons),
            keys: dedup_keys(keys),
        }
    }

    #[must_use]
    pub fn buttons(&self) -> &[MouseButton] {
        &self.buttons
    }

    #[must_use]
    pub fn keys(&self) -> &[u8] {
        &self.keys
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty() && self.keys.is_empty()
    }

    #[must_use]
    pub fn describes_button(&self, button: MouseButton) -> bool {
        self.buttons.contains(&button)
    }

    #[must_use]
    pub fn describes_key(&self, virtual_key: u8) -> bool {
        self.keys.contains(&virtual_key)
    }

    /// 请求 JSON 里的义务表示：helper 的收尾释放与独立释放通道读的就是它。
    #[must_use]
    pub fn request_json(&self) -> serde_json::Value {
        serde_json::json!({
            "buttons": self
                .buttons
                .iter()
                .map(|button| mouse_button_name(*button))
                .collect::<Vec<_>>(),
            "keys": self
                .keys
                .iter()
                .map(|key| u32::from(*key))
                .collect::<Vec<_>>(),
        })
    }

    /// 错误文本/证据用的一行摘要；空义务写 `none`，不留空白歧义。
    #[must_use]
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "none".to_string();
        }
        let buttons = self
            .buttons
            .iter()
            .map(|button| mouse_button_name(*button))
            .collect::<Vec<_>>()
            .join(",");
        let keys = self
            .keys
            .iter()
            .map(|key| format!("0x{key:02X}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("buttons=[{buttons}] keys=[{keys}]")
    }
}

fn dedup_buttons(buttons: &[MouseButton]) -> Vec<MouseButton> {
    let mut collected = Vec::with_capacity(buttons.len());
    for button in buttons {
        if !collected.contains(button) {
            collected.push(*button);
        }
    }
    collected
}

fn dedup_keys(keys: &[u8]) -> Vec<u8> {
    let mut collected = Vec::with_capacity(keys.len());
    for key in keys {
        if !collected.contains(key) {
            collected.push(*key);
        }
    }
    collected
}

#[must_use]
fn mouse_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
    }
}

/// 点击类动作的释放义务：按**实际会用到的按钮**登记（双击=左键、和弦=左右两键）。
#[must_use]
pub fn click_release_obligation(action: MouseButtonAction) -> ReleaseObligation {
    match action {
        MouseButtonAction::LeftClick | MouseButtonAction::DoubleClick => {
            ReleaseObligation::mouse_buttons(&[MouseButton::Left])
        }
        MouseButtonAction::RightClick => ReleaseObligation::mouse_buttons(&[MouseButton::Right]),
        MouseButtonAction::LeftRightChord => {
            ReleaseObligation::mouse_buttons(&[MouseButton::Left, MouseButton::Right])
        }
    }
}

/// 滚轮的释放义务：滚轮只发轮询事件，不按下任何按钮。
#[must_use]
pub fn scroll_release_obligation() -> ReleaseObligation {
    ReleaseObligation::none()
}

/// 文本输入的释放义务：SendInput 的每个单元是一次 down+up 同批注入（不可被取消拆开），
/// 因此不留按键义务；Interception 逐字符按下/抬起修饰键，按它可能按下的三个修饰键登记。
#[must_use]
pub fn text_release_obligation(backend: InputBackend) -> ReleaseObligation {
    match backend {
        InputBackend::SendInput => ReleaseObligation::none(),
        InputBackend::Interception => {
            ReleaseObligation::virtual_keys(&[VIRTUAL_KEY_SHIFT, VIRTUAL_KEY_CONTROL, VIRTUAL_KEY_ALT])
        }
    }
}

/// 按键/组合键的释放义务：就是这次要按下的那些键，逐一登记（不是整块键盘）。
#[must_use]
pub fn key_release_obligation(virtual_keys: &[u8]) -> ReleaseObligation {
    ReleaseObligation::virtual_keys(virtual_keys)
}

/// 显式按下/抬起的释放义务：按下登记该按钮；抬起不新增义务（它是在解除义务）。
#[must_use]
pub fn button_state_release_obligation(button: MouseButton, is_down: bool) -> ReleaseObligation {
    if is_down {
        ReleaseObligation::mouse_buttons(&[button])
    } else {
        ReleaseObligation::none()
    }
}

/// v1 进度记录（没有版本／阶段／身份标记）。仍可读取，但**不能**当证明。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportedV1 {
    injected_steps: u32,
    completed: bool,
    armed_buttons: Vec<String>,
    armed_keys: Vec<u32>,
    pressed: bool,
    #[serde(default)]
    released: Option<bool>,
}

/// v2 进度记录：在 v1 之上补最小必要的版本／身份／阶段／完整性标记。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportedV2 {
    protocol: u32,
    request_id: String,
    phase: String,
    cursor_moved: bool,
    injected_steps: u32,
    completed: bool,
    armed_buttons: Vec<String>,
    armed_keys: Vec<u32>,
    pressed: bool,
    #[serde(default)]
    released: Option<bool>,
}

/// 按钮／虚拟键名字的解码：未知名字一律拒绝（要么伪造、要么版本不符）。
fn decode_armed(
    armed_buttons: &[String],
    armed_keys: &[u32],
) -> Result<(Vec<MouseButton>, Vec<u8>), String> {
    let mut buttons = Vec::new();
    for name in armed_buttons {
        let button = match name.as_str() {
            "left" => MouseButton::Left,
            "right" => MouseButton::Right,
            other => return Err(format!("未知按钮名：{other:?}")),
        };
        if !buttons.contains(&button) {
            buttons.push(button);
        }
    }
    let mut keys = Vec::new();
    for key in armed_keys {
        let key = u8::try_from(*key)
            .ok()
            .filter(|key| *key != 0)
            .ok_or_else(|| format!("不是合法虚拟键：{key}"))?;
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok((buttons, keys))
}

/// 受控原生 helper 自己报告的原生输入事实（来自进度文件，严格解析）。
///
/// 与路径事实（[`HelperInputFacts`]）**分开**：这里没有"路径点"这个概念，只有
/// "已确认的注入步数 / 是否走完 / 是否按住过或仍按住 / 释放结果"。
///
/// CU-F01 补充的最小标记：
/// - `protocol` / `request_id`：这份记录属于哪一版协议、哪一次请求；
/// - `phase` / `cursor_moved`：这条记录是过程快照还是收尾之后的最终事实，以及
///   "光标是否已经移动过"——没有它，`injected_steps == 0` 与"什么都没做"无法区分。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeInputFacts {
    /// 记录声明的协议版本（旧记录 = [`HELPER_FACT_PROTOCOL_V1`]）。
    pub protocol: u32,
    /// 本次请求的身份；旧记录为 `None`，因此无法核对归属。
    pub request_id: Option<String>,
    /// 记录所处的阶段；只有 [`HelperFactPhase::Final`] 才算收尾已封闭。
    pub phase: HelperFactPhase,
    /// 是否确认移动过光标；`None` = 旧记录无从核对。
    pub cursor_moved: Option<bool>,
    /// 已确认完成的注入步数（点击步骤 / 已输入字符 / 按键步骤）；0 = 可证明什么都没注入。
    pub injected_steps: u32,
    /// 本次运行是否走完了整条操作序列。
    pub completed: bool,
    /// 报告时**可能仍处于按下状态**的鼠标键（helper 自己维护的登记集合）。
    pub armed_buttons: Vec<MouseButton>,
    /// 报告时可能仍处于按下状态的虚拟键。
    pub armed_keys: Vec<u8>,
    /// 本次运行中是否**按下过**任何按钮或按键（已经抬起的也算）。
    pub pressed: bool,
    /// `None` = helper 没有确认释放结果。
    pub released: Option<bool>,
}

impl NativeInputFacts {
    /// 读取 helper 进度文件内容。
    ///
    /// 严格解析：缺键、多键、类型不符、越界或自相矛盾都**不是**"零注入"，
    /// 一律返回 [`HelperFactRead::Rejected`] 并带上原因；只有完全符合已知协议
    /// （v2 带标记记录，或 v1 旧记录）才返回可信事实。
    ///
    /// 旧版（v1）记录仍可读取，但它的 `phase` 是 `LegacyUnverifiable`、
    /// `cursor_moved` 是 `None`，因此**永远不能**充当"零输入"证明（版本兼容 +
    /// 异常呈现，而不是把旧记录丢掉或当证据用）。
    #[must_use]
    pub fn read(raw: &[u8], expected_request_id: &str) -> HelperFactRead<Self> {
        let value: serde_json::Value = match serde_json::from_slice(raw) {
            Ok(value) => value,
            Err(error) => {
                return HelperFactRead::Rejected {
                    anomaly: format!("进度记录不是合法 JSON：{error}"),
                }
            }
        };
        match serde_json::from_value::<ReportedV2>(value.clone()) {
            Ok(reported) => Self::from_v2(reported, expected_request_id),
            Err(v2_error) => match serde_json::from_value::<ReportedV1>(value) {
                Ok(reported) => Self::from_v1(reported),
                Err(v1_error) => HelperFactRead::Rejected {
                    anomaly: format!(
                        "进度记录不符合任何已知协议：v2（{v2_error}）；v1（{v1_error}）"
                    ),
                },
            },
        }
    }

    fn from_v1(reported: ReportedV1) -> HelperFactRead<Self> {
        let mut facts = Self {
            protocol: HELPER_FACT_PROTOCOL_V1,
            request_id: None,
            phase: HelperFactPhase::LegacyUnverifiable,
            cursor_moved: None,
            injected_steps: reported.injected_steps,
            completed: reported.completed,
            armed_buttons: Vec::new(),
            armed_keys: Vec::new(),
            pressed: reported.pressed,
            released: reported.released,
        };
        match decode_armed(&reported.armed_buttons, &reported.armed_keys) {
            Ok((buttons, keys)) => {
                facts.armed_buttons = buttons;
                facts.armed_keys = keys;
            }
            Err(anomaly) => return HelperFactRead::Rejected { anomaly },
        }
        match facts.check_integrity() {
            Ok(()) => HelperFactRead::Trusted(facts),
            Err(anomaly) => HelperFactRead::Rejected { anomaly },
        }
    }

    fn from_v2(reported: ReportedV2, expected_request_id: &str) -> HelperFactRead<Self> {
        if reported.protocol != HELPER_FACT_PROTOCOL_V2 {
            return HelperFactRead::Rejected {
                anomaly: format!(
                    "协议版本不符：记录声明 protocol={}，本进程只认 {} 与缺失版本键的 v1",
                    reported.protocol, HELPER_FACT_PROTOCOL_V2
                ),
            };
        }
        if reported.request_id != expected_request_id {
            // 身份不合＝这份事实不属于本动作的这次请求，绝不能被接纳。
            return HelperFactRead::Rejected {
                anomaly: format!(
                    "记录不属于本次请求：request_id={:?}，期望 {:?}",
                    reported.request_id, expected_request_id
                ),
            };
        }
        let Some(phase) = HelperFactPhase::from_marker(&reported.phase) else {
            return HelperFactRead::Rejected {
                anomaly: format!("阶段标记无法识别：phase={:?}", reported.phase),
            };
        };
        let (buttons, keys) = match decode_armed(&reported.armed_buttons, &reported.armed_keys) {
            Ok(decoded) => decoded,
            Err(anomaly) => return HelperFactRead::Rejected { anomaly },
        };
        let facts = Self {
            protocol: reported.protocol,
            request_id: Some(reported.request_id),
            phase,
            cursor_moved: Some(reported.cursor_moved),
            injected_steps: reported.injected_steps,
            completed: reported.completed,
            armed_buttons: buttons,
            armed_keys: keys,
            pressed: reported.pressed,
            released: reported.released,
        };
        if let Err(anomaly) = facts.check_v2_integrity() {
            return HelperFactRead::Rejected { anomaly };
        }
        HelperFactRead::Trusted(facts)
    }

    /// v1／v2 共同的自洽性检查（与旧实现逐条一致）。
    fn check_integrity(&self) -> Result<(), String> {
        if self.injected_steps > MAX_NATIVE_INJECTED_STEPS {
            return Err(format!(
                "已注入步数 {} 超出上限 {MAX_NATIVE_INJECTED_STEPS}",
                self.injected_steps
            ));
        }
        // "已确认全部释放"与"仍有按住"不可能同时成立。
        if self.released == Some(true) && self.still_holding() {
            return Err("已确认释放与仍有按住不可能同时成立".to_string());
        }
        // 没有注入过任何步数就不可能按下过东西。
        if self.injected_steps == 0 && (self.still_holding() || self.pressed) {
            return Err("零注入却报告按下／按住，只能是伪造或损坏".to_string());
        }
        Ok(())
    }

    /// v2 记录额外的阶段／完整性检查：过程快照不得冒充最终事实，反之亦然。
    fn check_v2_integrity(&self) -> Result<(), String> {
        self.check_integrity()?;
        if self.cursor_moved != Some(true) && self.cursor_moved != Some(false) {
            return Err("v2 记录必须声明 cursor_moved".to_string());
        }
        // 起点阶段：任何 Move / Down / 注入都还没发生。
        if self.phase == HelperFactPhase::PreInput
            && (self.injected_steps > 0
                || self.pressed
                || self.still_holding()
                || self.completed
                || self.cursor_moved == Some(true))
        {
            return Err("起点阶段的记录却已经移动光标／注入／按下：阶段与事实自相矛盾"
                .to_string());
        }
        // 注意：原生输入里"注入过步数却没移动光标"是**合法**的（文本、组合键、按键
        // 都不移动光标），因此这里不设"注入 ⇒ 移动过"的交叉约束；那条约束只属于笔画路径
        // （路径点必须在光标移动之后才可能被确认注入）。
        // 最终事实必须给出释放结论；唯一例外是"按设计仍按住"（armed 非空）。
        if self.phase == HelperFactPhase::Final && self.released.is_none() && !self.still_holding() {
            return Err("最终事实却没有释放结论，也不是按设计仍按住".to_string());
        }
        Ok(())
    }

    /// 投影到两条路径共用的释放事实视图。
    #[must_use]
    pub fn release_view(&self) -> HelperFactView<'_> {
        HelperFactView {
            phase: self.phase,
            cursor_moved: self.cursor_moved,
            injected: self.injected_steps,
            ever_pressed: self.pressed,
            still_holding: self.still_holding(),
            sequence_completed: self.completed,
            released: self.released,
            request_id: self.request_id.as_deref(),
        }
    }

    /// 报告时是否可能仍按住按钮或按键。
    #[must_use]
    pub fn still_holding(&self) -> bool {
        !self.armed_buttons.is_empty() || !self.armed_keys.is_empty()
    }

    /// 报告时仍按住、但**不在本次登记义务里**的东西。
    ///
    /// 这种"登记之外的按住"说明登记漏了东西（或 helper 报告了契约外的状态），
    /// 一律按未知处理：独立释放补的是登记集合，补不到它。
    #[must_use]
    pub fn holds_something_unregistered(&self, obligation: &ReleaseObligation) -> bool {
        self.armed_buttons
            .iter()
            .any(|button| !obligation.describes_button(*button))
            || self
                .armed_keys
                .iter()
                .any(|key| !obligation.describes_key(*key))
    }
}

/// 读取 helper 进度文件；不存在或不可读都表示"没有记录"。
fn read_native_facts(path: &FsPath, expected_request_id: &str) -> HelperFactRead<NativeInputFacts> {
    match std::fs::read(path) {
        Ok(raw) => NativeInputFacts::read(&raw, expected_request_id),
        Err(_) => HelperFactRead::Missing,
    }
}

/// 受控原生输入运行中**主机自己能确认的事实**。
///
/// 「正常收到回复」与「子进程真正结束」必须分开记录：超时后 helper 仍在跑时两者会不一致，
/// 而"没有确认静止"绝不能被读成"已经停下"。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeInputOutcome {
    /// 本次运行**登记的释放义务**（输入开始之前登记的那一份，运行期不再改写）。
    pub obligation: ReleaseObligation,
    /// **helper 的实例身份**（8.3c）：pid ＋ 创建时间，在 helper 存活时捕获。
    ///
    /// `None` = 本次没有捕获到身份（例如身份读取失败）——**不得**用别的值顶替。
    /// 消费端据此经 `runtime::classify_executor_instance` 判定"是否同一实例"。
    pub helper_process: Option<runtime::ProcessInstanceEvidence>,
    /// helper 自己报告的原生输入事实；`None` = 没有事实。
    pub facts: Option<NativeInputFacts>,
    /// 进度记录存在但不可信时的原因（协议不符／身份不合／自相矛盾）。
    ///
    /// 它与 `facts = None` 不是一回事：前者是"有记录但读不得"，后者是"没有记录"。
    /// 两者都不允许被默认值抹平成"零注入"。
    pub fact_anomaly: Option<String>,
    /// 正常收到 helper 回复（读到带释放结果的终态记录，或它以成功状态退出并给出完成标记）。
    pub reply_received_at_ms: Option<u64>,
    /// 确认子进程**真正结束**（进程句柄给出结束状态）的时刻；`None` = 未确认静止。
    pub process_exit_confirmed_at_ms: Option<u64>,
    /// 有界收尾事实；`None` = 本次没有进入取消/异常收尾。
    pub cleanup: Option<HelperCleanupFacts>,
}

impl NativeInputOutcome {
    #[must_use]
    pub fn facts(&self) -> Option<&NativeInputFacts> {
        self.facts.as_ref()
    }

    /// 事实读取结果（可信／不可信／缺失三分）。
    #[must_use]
    pub fn fact_read(&self) -> HelperFactRead<&NativeInputFacts> {
        match (&self.facts, &self.fact_anomaly) {
            (Some(facts), _) => HelperFactRead::Trusted(facts),
            (None, Some(anomaly)) => HelperFactRead::Rejected {
                anomaly: anomaly.clone(),
            },
            (None, None) => HelperFactRead::Missing,
        }
    }

    /// 推导释放义务用的共同输入（唯一实现点见 [`derive_release_obligation`]）。
    #[must_use]
    pub fn release_inputs(&self, kind: StrokeFailureKind) -> ReleaseDerivationInputs<'_> {
        ReleaseDerivationInputs {
            kind,
            input_possible: true,
            path_action: false,
            obligation_mechanism_leaves_nothing: self.obligation.is_empty(),
            // 身份不合的记录在读取时已经被判为不可信，能走到这里的记录都属于本动作。
            facts_belong_to_this_action: true,
            facts: self.facts.as_ref().map(NativeInputFacts::release_view),
            fact_anomaly: self.fact_anomaly.as_deref(),
            // 分类为"释放未确认"本身就意味着 helper 自己报告过释放失败（原始原因不看补发结果）。
            helper_reported_release_failure: kind == StrokeFailureKind::ReleaseUnconfirmed,
            stillness_confirmed: self.stillness_confirmed(),
            independent_release_confirmed: self.independent_release_confirmed(),
        }
    }

    #[must_use]
    pub fn cleanup(&self) -> Option<&HelperCleanupFacts> {
        self.cleanup.as_ref()
    }

    /// 是否确认子进程静止。与"收到回复"分开判定。
    #[must_use]
    pub fn stillness_confirmed(&self) -> bool {
        self.process_exit_confirmed_at_ms.is_some()
    }

    /// 是否收到过 helper 的回复。
    #[must_use]
    pub fn reply_received(&self) -> bool {
        self.reply_received_at_ms.is_some()
    }

    /// 本次运行是否可能已经注入过输入。
    ///
    /// 没有事实时按"可能已注入"处理（保守方向），被强杀的执行者永远不能算已证明零注入。
    #[must_use]
    pub fn may_have_injected(&self) -> bool {
        match self.facts.as_ref() {
            None => true,
            Some(facts) => facts.pressed || facts.still_holding() || facts.injected_steps > 0,
        }
    }

    /// 本进程是否按登记义务补发过一次独立释放并确认。
    #[must_use]
    pub fn independent_release_confirmed(&self) -> bool {
        self.cleanup
            .as_ref()
            .is_some_and(|cleanup| cleanup.independent_release_confirmed)
    }

    /// 释放对账：登记义务 + helper 事实 + 本进程的独立释放共同决定。
    ///
    /// 这里按"请求已经送到 helper"取值：输入前失败那条路径登记的义务恒为空，
    /// 判定表会落到 `NotNeeded`，不需要额外区分。
    #[must_use]
    pub fn release_status(&self) -> InputReleaseStatus {
        native_release_status(
            &self.obligation,
            true,
            self.facts.as_ref(),
            self.independent_release_confirmed(),
            self.stillness_confirmed(),
        )
    }

    /// 收尾报告里的释放状态（与回执的释放状态同源，避免两处各说一套）。
    #[must_use]
    pub fn cleanup_release_status(&self) -> CleanupReleaseStatus {
        cleanup_release_status_of(self.release_status())
    }
}

/// 释放对账的旧判定表已被 [`derive_release_obligation`]（CU-F01 五态）取代；
/// 这里保留适配层以保证调用点与既有单测的语义稳定。
/// 释放对账：登记义务 + helper 事实 + 本进程的独立释放共同决定。
///
/// 本函数**只是** [`derive_release_obligation`] 的一个薄适配层：判定规则只有一份，
/// 不允许在这里再写一套。`kind` 传 `Failed`（"非失联"）是安全的：真正把"被强杀"
/// 排除在外的判据是记录必须声明 `phase = final`——被杀的执行者写不出收尾后的最终事实。
#[must_use]
fn native_release_status(
    obligation: &ReleaseObligation,
    input_possible: bool,
    facts: Option<&NativeInputFacts>,
    independent_release_confirmed: bool,
    stillness_confirmed: bool,
) -> InputReleaseStatus {
    derive_release_obligation(&ReleaseDerivationInputs {
        kind: StrokeFailureKind::Failed,
        input_possible,
        path_action: false,
        obligation_mechanism_leaves_nothing: obligation.is_empty(),
        facts_belong_to_this_action: true,
        facts: facts.map(NativeInputFacts::release_view),
        fact_anomaly: None,
        helper_reported_release_failure: false,
        stillness_confirmed,
        independent_release_confirmed,
    })
    .release_status()
}

#[must_use]
fn cleanup_release_status_of(release: InputReleaseStatus) -> CleanupReleaseStatus {
    match release {
        InputReleaseStatus::NotNeeded => CleanupReleaseStatus::NotNeeded,
        InputReleaseStatus::Released => CleanupReleaseStatus::Confirmed,
        InputReleaseStatus::Unknown => CleanupReleaseStatus::Unconfirmed,
    }
}

/// 把两段输入（前一段已确认成功、后一段失败）的释放事实并起来：
/// 只要有一段未确认就不能宣布"已释放"；两段都对账无义务才是 `NotNeeded`。
#[must_use]
pub fn merge_input_release(
    first: InputReleaseStatus,
    second: InputReleaseStatus,
) -> InputReleaseStatus {
    if first == InputReleaseStatus::Unknown || second == InputReleaseStatus::Unknown {
        InputReleaseStatus::Unknown
    } else if first == InputReleaseStatus::Released || second == InputReleaseStatus::Released {
        InputReleaseStatus::Released
    } else {
        InputReleaseStatus::NotNeeded
    }
}

// ---------------------------------------------------------------------------
// CU-F01：释放事实一致性
//
// 受控笔画（`input_stroke`）与受控原生输入（本文件下半部分）是两个**不同机制**，
// 但"这次失败到底有没有注入过、有没有释放义务"的推导必须是同一套：本节的类型与
// `derive_delivery_facts` / `derive_release_obligation` 是**唯一**实现点。
//
// 三条不可动摇的前提：
// 1. 证明必须**属于本动作**、**完整**（覆盖路径开始前的光标移动）且**已封闭**
//    （收尾之后写出的最终事实）。起点快照、缺字段的默认值、当前按钮状态都不算证明。
// 2. 没有事实 ≠ 零输入：读不到记录时一律按最保守方向，绝不把缺失填成零与 false。
// 3. 证据冲突时保留冲突，绝不自动选更乐观的结论。
// ---------------------------------------------------------------------------

/// 进度记录声明的协议版本：只有该键的旧记录按 v1 读取（兼容，但**不能**当作证明）。
pub const HELPER_FACT_PROTOCOL_V1: u32 = 1;
/// 带阶段／完整性／光标移动标记的记录版本（CU-F01 补充的最小标记）。
pub const HELPER_FACT_PROTOCOL_V2: u32 = 2;

/// 进度记录所处的阶段。
///
/// **只有** [`HelperFactPhase::Final`] 才是"收尾（`finally`）已经结束"之后写出的
/// 最终事实；其余阶段都只是过程快照，不能用来证明"从未输入"。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HelperFactPhase {
    /// 起点：helper 已经开始执行，但任何 Move / Down 都还没有发生。
    PreInput,
    /// 已执行过输入动作、收尾还没结束：这条记录随时可能被下一条覆盖。
    InFlight,
    /// 收尾之后写出的最终事实。
    Final,
    /// 旧版记录（没有阶段键）：阶段与"光标是否移动过"都无从核对。
    ///
    /// 也是默认值：默认值必须落在这个"什么都不能证明"的变体上。
    #[default]
    LegacyUnverifiable,
}

impl HelperFactPhase {
    /// 记录是否声明"收尾已经结束"。旧记录永远不算已封闭。
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Final)
    }

    /// 协议文本 → 阶段；无法识别一律读成"无从核对"，不留想象空间。
    #[must_use]
    pub fn from_marker(marker: &str) -> Option<Self> {
        match marker {
            "pre_input" => Some(Self::PreInput),
            "in_flight" => Some(Self::InFlight),
            "final" => Some(Self::Final),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreInput => "pre_input",
            Self::InFlight => "in_flight",
            Self::Final => "final",
            Self::LegacyUnverifiable => "legacy_unverifiable",
        }
    }
}

/// 读取 helper 进度文件的结果。
///
/// **不**把"记录不可信"折叠成"没有记录"：异常必须带着原因上抛，否则上层只能用
/// 默认零值去填，那正是"把缺失读成未发送"的入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperFactRead<T> {
    /// 没有记录（文件不存在 / 不可读）。
    Missing,
    /// 有记录但不可信（缺字段、越界、自相矛盾、协议版本不符、不属于本次请求）。
    Rejected { anomaly: String },
    /// 可信事实。
    Trusted(T),
}

impl<T> HelperFactRead<T> {
    /// 可信事实；其余两种情况都返回 `None`（调用方必须再区分异常与缺失）。
    #[must_use]
    pub fn trusted(&self) -> Option<&T> {
        match self {
            Self::Trusted(facts) => Some(facts),
            _ => None,
        }
    }

    /// 异常文本；没有记录与可信事实都返回 `None`。
    #[must_use]
    pub fn anomaly(&self) -> Option<&str> {
        match self {
            Self::Rejected { anomaly } => Some(anomaly),
            _ => None,
        }
    }

    /// 是否读到了不可信的记录（与"没有记录"不同）。
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }
}

/// helper 事实的共同视图：两条输入路径各自把自己能确认的东西投影到这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperFactView<'a> {
    /// 记录所处的阶段；只有 `Final` 才算收尾已封闭。
    pub phase: HelperFactPhase,
    /// 是否确认移动过光标；`None` = 旧记录无从核对。
    pub cursor_moved: Option<bool>,
    /// 已确认注入的点数（笔画）／步数（原生输入）。
    pub injected: u32,
    /// 本动作是否按下过按钮／按键（已经抬起的也算）。
    pub ever_pressed: bool,
    /// 报告时是否可能仍按住。
    pub still_holding: bool,
    /// 路径／操作序列是否确认走完。
    pub sequence_completed: bool,
    /// helper 确认的释放结果；`None` = 没有确认。
    pub released: Option<bool>,
    /// 记录里声明的请求身份（旧记录没有该字段）。
    pub request_id: Option<&'a str>,
}

impl HelperFactView<'_> {
    /// 记录是否声明"收尾已经结束"。
    #[must_use]
    pub const fn closed(&self) -> bool {
        self.phase.is_final()
    }
}

/// 推导的输入：各路径把自己能确认的事实投影到这里，由**同一处**得出结论。
#[derive(Debug, Clone, Copy)]
pub struct ReleaseDerivationInputs<'a> {
    /// 原始执行分类（只看 helper 报告文本，不看事实）。
    pub kind: StrokeFailureKind,
    /// 请求是否已经送到 helper（`false` = 结构上不可能注入输入）。
    pub input_possible: bool,
    /// 本次动作是否是路径动作（笔画）；非路径动作不写路径字段。
    pub path_action: bool,
    /// 输入开始之前登记的义务是否**在机制上**不可能留下按住状态（滚动、SendInput
    /// 文本、抬起这类操作）。这是正面事实，与"没有事实"完全不同；笔画路径不登记
    /// 义务，因此恒为 `false`。
    pub obligation_mechanism_leaves_nothing: bool,
    /// 记录是否**属于本动作**这次请求（身份核对的结果）。
    pub facts_belong_to_this_action: bool,
    /// 可信事实；`None` = 没有事实或记录不可信（此时看 `fact_anomaly`）。
    pub facts: Option<HelperFactView<'a>>,
    /// 协议／身份／完整性／冲突异常；有值即为"证据冲突"。
    pub fact_anomaly: Option<&'a str>,
    /// helper 自己报告了释放失败（受信报告）。
    pub helper_reported_release_failure: bool,
    /// 执行者已确认静止（进程句柄给出结束状态）。
    pub stillness_confirmed: bool,
    /// 本进程按登记义务补发过一次独立释放并确认。
    pub independent_release_confirmed: bool,
}

/// 投递事实：回执里"输出去向／部分／走完／已注入点数"四个维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryFacts {
    pub input_delivery: InputDelivery,
    pub partial: Option<bool>,
    pub path_completed: Option<bool>,
    pub confirmed_point_count: Option<u32>,
}

impl DeliveryFacts {
    /// 可证明的"未发送"：路径动作同时明确写 0 点、未走完。
    #[must_use]
    const fn proven_not_sent(is_path: bool) -> Self {
        Self {
            input_delivery: InputDelivery::NotSent,
            partial: Some(false),
            path_completed: if is_path { Some(false) } else { None },
            confirmed_point_count: if is_path { Some(0) } else { None },
        }
    }

    /// 未知：既不能写未发送，也不能写完整。
    #[must_use]
    const fn unknown() -> Self {
        Self {
            input_delivery: InputDelivery::MayHaveBeenSent,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
        }
    }
}

/// **CU-01（Paint 事实层 · Layer 1）**：动作输入状态的**粗粒度四值视图**。
///
/// 为什么需要它：报告的读者此前要自己把 `input_delivery`／`partial`／`path_completed`／
/// 点数拼成"到底注入没有、完成没有"。拼错一次就会产出"把未知读成完成"或"把部分读成没发生"。
/// 四值视图把这件事**只判定一次**，并且与 [`DeliveryFacts`]（唯一事实来源）同一口径。
///
/// **安全口径**：`Partial` 与 `Unknown` 都**不得**触发自动重放，`Unknown` 更不得被读成完成——
/// 重放禁令本身由控制器与收尾路径负责（`controller.rs` 的身份不匹配/可能已发出分支、
/// `input_stroke.rs` 的"释放未确认必须隔离"），本视图只保证**不会把事实说成比它更强**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputStatus {
    /// 没有输入事件被确认（结构事实，或"光标动过但 0 点且未按下"的封闭记录）。
    None,
    /// 确认注入过**部分**内容：绝不算完成，且不得据此自动重放。
    Partial,
    /// 确认走完：只有"确认发送且明确不是部分"才允许落到这里。
    Complete,
    /// 读不懂／记录未封闭：既不是完成，也不是"没有发生"；进入对账，禁止自动重放。
    Unknown,
}

impl InputStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Partial => "partial",
            Self::Complete => "complete",
            Self::Unknown => "unknown",
        }
    }

    /// 是否**可以**声称"这一步完成了"。只有 `Complete` 为真——其余三态一律不许。
    #[must_use]
    pub const fn may_claim_complete(self) -> bool {
        matches!(self, Self::Complete)
    }

    /// 是否禁止自动重放（`Partial` / `Unknown`；`None` 表示可安全重试，`Complete` 表示无需重试）。
    #[must_use]
    pub const fn forbids_automatic_replay(self) -> bool {
        matches!(self, Self::Partial | Self::Unknown)
    }
}

/// 由的事实层推导粗粒度输入状态（**唯一判定点**）。
///
/// 映射规则按"最保守方向"排列，任何一条不满足都落到 `Unknown`：
///
/// | 事实 | 视图 | 说明 |
/// | --- | --- | --- |
/// | `input_delivery = NotSent` | `none` | 结构事实：请求没送到或明确 0 点且未走完 |
/// | `Sent` 且 `partial = Some(false)` | `complete` | 唯一允许声称完成的组合 |
/// | `partial = Some(true)` | `partial` | 明确的部分注入（含自相矛盾记录：宁可 partial，不许 complete） |
/// | `MayHaveBeenSent` 且 `partial = Some(false)` 且点数 `0` | `none` | 光标动过但**没有任何输入事件被确认**且记录已封闭；注意 `input_delivery` 刻意不称其为 `NotSent`（光标真的动过），两者是不同粒度 |
/// | 其它 | `unknown` | 记录未封闭/缺字段/读不懂 |
#[must_use]
pub fn derive_input_status(facts: &DeliveryFacts) -> InputStatus {
    if facts.input_delivery == InputDelivery::NotSent {
        return InputStatus::None;
    }
    if facts.input_delivery == InputDelivery::Sent && facts.partial == Some(false) {
        return InputStatus::Complete;
    }
    if facts.partial == Some(true) {
        return InputStatus::Partial;
    }
    if facts.input_delivery == InputDelivery::MayHaveBeenSent
        && facts.partial == Some(false)
        && facts.confirmed_point_count == Some(0)
    {
        return InputStatus::None;
    }
    InputStatus::Unknown
}

/// 释放义务的五态（第四轮裁决第三节 CU-F01 §3）。
///
/// 五态各自对应一个**确定的动作**：不补发 / 不重复补发 / 一次受控收尾 /
/// 先确认静止再收尾否则隔离 / 保留冲突证据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseObligationState {
    /// 已证明不存在：不得启动任何补发。
    ProvenAbsent,
    /// 已产生且已结清：不得重复补发。
    Settled,
    /// 已产生且未结清：按既定安全条件执行**一次**受控收尾。
    Unsettled,
    /// 可能存在（证据不足，但也没有证明没有义务）：先确认旧执行者静止，满足条件
    /// 才收尾，否则隔离。
    Possible,
    /// 证据冲突：保留冲突证据，不得自动选更乐观的结论。
    EvidenceConflict,
}

impl ReleaseObligationState {
    /// 是否已证明没有释放义务。
    #[must_use]
    pub const fn is_proven_absent(self) -> bool {
        matches!(self, Self::ProvenAbsent)
    }

    /// 是否需要一次受控收尾。`ProvenAbsent` / `Settled` 一律不启动。
    #[must_use]
    pub const fn requires_controlled_cleanup(self) -> bool {
        matches!(
            self,
            Self::Unsettled | Self::Possible | Self::EvidenceConflict
        )
    }

    /// 收尾之前是否必须先确认旧执行者静止。
    #[must_use]
    pub const fn requires_stillness_before_cleanup(self) -> bool {
        matches!(self, Self::Possible | Self::EvidenceConflict)
    }

    /// 是否仍带着**未结清**的释放义务：外部行为必须按安全阻断处理。
    #[must_use]
    pub const fn carries_unsettled_duty(self) -> bool {
        !matches!(self, Self::ProvenAbsent | Self::Settled)
    }

    /// 映射到既有的回执释放维度，不新增第三套状态。
    #[must_use]
    pub const fn release_status(self) -> InputReleaseStatus {
        match self {
            Self::ProvenAbsent => InputReleaseStatus::NotNeeded,
            Self::Settled => InputReleaseStatus::Released,
            Self::Unsettled | Self::Possible | Self::EvidenceConflict => {
                InputReleaseStatus::Unknown
            }
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProvenAbsent => "proven_absent",
            Self::Settled => "settled",
            Self::Unsettled => "unsettled",
            Self::Possible => "possible",
            Self::EvidenceConflict => "evidence_conflict",
        }
    }
}

/// 推导**投递事实**（裁决 §2 正推顺序的第三步）。
///
/// "可证明未发送"要求记录**属于本动作**、**已封闭**且覆盖路径开始前的光标移动；
/// 起点快照（未封闭）、旧版记录、缺字段默认值一律不算。
#[must_use]
pub fn derive_delivery_facts(inputs: &ReleaseDerivationInputs<'_>) -> DeliveryFacts {
    // 身份／协议／完整性异常：读不懂就按最保守的方向（可能已发出），且不写任何维度结论。
    if !inputs.facts_belong_to_this_action || inputs.fact_anomaly.is_some() {
        return DeliveryFacts::unknown();
    }
    // 请求没送到 helper：这是结构事实，不是从记录推断出来的。
    if !inputs.input_possible {
        return DeliveryFacts::proven_not_sent(inputs.path_action);
    }
    let Some(facts) = inputs.facts else {
        // 没有事实：维持未知，进入对账；不写路径字段，避免"未知"被读成"完整"。
        return DeliveryFacts::unknown();
    };
    if proven_zero_input(inputs, &facts) {
        return DeliveryFacts::proven_not_sent(inputs.path_action);
    }
    // 光标移动过、但一个采样点都没注入、也没按下：不是"未发送"（光标真的动过），
    // 也不能算"发送了"——没有任何输入事件被确认。
    //
    // 记录**未封闭**（例如只有起点快照）时路径维度一律留未知：起点快照不覆盖它之后
    // 可能发生的事，填上"0 点、未走完"会把"没证据"读成"没执行"。
    if facts.injected == 0 && !facts.sequence_completed && !facts.ever_pressed && !facts.still_holding
    {
        return if facts.closed() {
            DeliveryFacts {
                input_delivery: InputDelivery::MayHaveBeenSent,
                partial: Some(false),
                path_completed: inputs.path_action.then_some(false),
                confirmed_point_count: inputs.path_action.then_some(0),
            }
        } else {
            DeliveryFacts::unknown()
        };
    }
    let path_completed = inputs.path_action.then_some(facts.sequence_completed);
    let confirmed_point_count = inputs.path_action.then_some(facts.injected);
    if facts.sequence_completed {
        return DeliveryFacts {
            input_delivery: InputDelivery::Sent,
            partial: Some(false),
            path_completed,
            confirmed_point_count,
        };
    }
    // 未确认走完：真实注入过的点必须保留，但不许升格成完成。
    if inputs.kind == StrokeFailureKind::HelperLost {
        return DeliveryFacts {
            input_delivery: InputDelivery::MayHaveBeenSent,
            partial: Some(true),
            path_completed,
            confirmed_point_count,
        };
    }
    DeliveryFacts {
        input_delivery: InputDelivery::Sent,
        partial: Some(true),
        path_completed,
        confirmed_point_count,
    }
}

/// 完整的"零输入"证明：收尾已封闭的最终事实 + 没有移动过光标 + 没有按下 + 零注入。
fn proven_zero_input(inputs: &ReleaseDerivationInputs<'_>, facts: &HelperFactView<'_>) -> bool {
    facts.closed()
        && facts.cursor_moved == Some(false)
        && facts.injected == 0
        && !facts.ever_pressed
        && !facts.still_holding
        && !facts.sequence_completed
        // 被强杀的执行者永远不能算"已证明零输入"：它可能没来得及写事实。
        && inputs.kind != StrokeFailureKind::HelperLost
}

/// 推导**释放义务**（裁决 §2 正推顺序的第四步）。
///
/// 判定顺序（自上而下，先命中先返回）：
/// 1. 记录不属于本动作，或协议／完整性异常 → 证据冲突（不得选更乐观结论）；
/// 2. 请求没送到 helper → 已证明不存在；
/// 3. helper 报过释放失败、记录却说"从未按下" → 证据冲突（不选"没有义务"）；
/// 4. 没有可信事实 → 机制上不会留下按住的操作为"已证明不存在"，其余为"可能存在"；
/// 5. 记录显示没有按下 → 只有"已封闭 + 覆盖光标移动 + 零注入"才是已证明不存在，
///    否则为"可能存在"；
/// 6. 按下过／仍按住 → 已确认释放（或本进程一次受控收尾且执行者已静止）才算已结清；
///    仍按住时未确认静止一律未结清；
/// 7. 其余 → 已产生且未结清。
#[must_use]
pub fn derive_release_obligation(inputs: &ReleaseDerivationInputs<'_>) -> ReleaseObligationState {
    use ReleaseObligationState::{EvidenceConflict, Possible, ProvenAbsent, Settled, Unsettled};
    if !inputs.facts_belong_to_this_action || inputs.fact_anomaly.is_some() {
        return EvidenceConflict;
    }
    if !inputs.input_possible {
        return ProvenAbsent;
    }
    if inputs.helper_reported_release_failure
        && inputs
            .facts
            .is_none_or(|facts| !facts.ever_pressed && !facts.still_holding)
    {
        // 受信报告与记录互相矛盾：不选"没有义务"这个更乐观的结论。
        return EvidenceConflict;
    }
    // 机制上不可能留下按住状态（滚动、SendInput 文本、抬起）：这是登记义务给出的
    // **正面事实**，不是"缺少证据"。只有记录也没有按下事实时才成立。
    if inputs.obligation_mechanism_leaves_nothing
        && inputs
            .facts
            .is_none_or(|facts| !facts.ever_pressed && !facts.still_holding)
    {
        return ProvenAbsent;
    }
    // 本进程已经按登记义务补发过一次并确认，且执行者已确认静止：这是一次**已完成**的
    // 受控收尾，义务已结清（被强杀时 helper 的"仍按住"记录正是靠这一步解除的）。
    if inputs.independent_release_confirmed && inputs.stillness_confirmed {
        return Settled;
    }
    let Some(facts) = inputs.facts else {
        // 没有事实：未证明零输入，也未证明没有义务 → 可能存在（被强杀也在此）。
        return Possible;
    };
    if !facts.ever_pressed && !facts.still_holding {
        return if proven_zero_input(inputs, &facts) {
            ProvenAbsent
        } else {
            Possible
        };
    }
    if facts.still_holding {
        // 仍按住：只有"本进程受控收尾已确认 + 执行者已静止"才能算结清。
        return if inputs.independent_release_confirmed && inputs.stillness_confirmed {
            Settled
        } else {
            Unsettled
        };
    }
    if inputs.independent_release_confirmed && inputs.stillness_confirmed {
        return Settled;
    }
    if facts.released == Some(true) {
        return Settled;
    }
    Unsettled
}

/// 收尾之前的推导：本进程还没补发过独立释放。
#[must_use]
pub fn derive_before_cleanup(inputs: &ReleaseDerivationInputs<'_>) -> ReleaseObligationState {
    let mut inputs = *inputs;
    inputs.independent_release_confirmed = false;
    derive_release_obligation(&inputs)
}

/// 收尾之后的推导：把"本进程这一次受控收尾"的结果合并进事实。
#[must_use]
pub fn derive_after_cleanup(
    inputs: &ReleaseDerivationInputs<'_>,
    independent_release_confirmed: bool,
) -> ReleaseObligationState {
    let mut inputs = *inputs;
    inputs.independent_release_confirmed = independent_release_confirmed;
    derive_release_obligation(&inputs)
}

/// 写入之前的回执行校验（裁决 §6）。
///
/// 规则：`NotSent ⇒ partial = false ∧ path_completed = false（若有）∧ 已注入点数为零
/// ∧ 本动作 input_release = NotNeeded`。
///
/// 两点边界必须写清楚：
/// - 结构校验**只能**检查字段之间是否自洽，它**不能**证明字段为真；"零输入"这个事实
///   仍由受信生产者（helper 事实 + 身份/协议/完整性校验）负责。
/// - 该校验只作用于**新写入**：旧记录仍按原样读取与复核，不因为新校验上线而丢弃。
pub fn validate_receipt_for_write(receipt: &ActionReceipt) -> Result<(), String> {
    receipt
        .validate()
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    if receipt.input_delivery != InputDelivery::NotSent {
        return Ok(());
    }
    if receipt.partial != Some(false) {
        return Err(
            "not_sent 必须同时声明 partial=false：未发送与“未观测到部分执行”不能各说一套"
                .to_string(),
        );
    }
    if receipt.path_completed == Some(true) {
        return Err("not_sent 与 path_completed=true 不可同时成立".to_string());
    }
    if receipt.confirmed_point_count.unwrap_or(0) > 0 {
        return Err("not_sent 与已确认注入点数不可同时成立".to_string());
    }
    if receipt.input_release != InputReleaseStatus::NotNeeded {
        return Err(format!(
            "not_sent 却带着未结清的释放义务（input_release={:?}）：零输入证明必须同时证明没有释放义务",
            receipt.input_release
        ));
    }
    Ok(())
}

/// 受控原生输入失败：错误文本 + 是否可能已注入 + 登记的释放义务 + 运行事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeInputFailure {
    pub message: String,
    /// helper 是否已收到请求（`false` = 可证明输入开始之前就失败了）。
    pub input_possible: bool,
    /// 本次尝试登记的释放义务。
    pub obligation: ReleaseObligation,
    pub outcome: NativeInputOutcome,
}

impl NativeInputFailure {
    /// 输入开始之前就被拒绝：没有 helper、没有义务、没有事实。
    fn before_input(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            input_possible: false,
            obligation: ReleaseObligation::none(),
            outcome: NativeInputOutcome::default(),
        }
    }

    /// 请求已经送到 helper 之后失败：可能已注入，事实按 helper 报告为准。
    fn after_input(
        message: impl Into<String>,
        obligation: ReleaseObligation,
        mut outcome: NativeInputOutcome,
    ) -> Self {
        // 运行事实里的义务与本次意图必须是同一份：不允许两处各记一套。
        outcome.obligation = obligation.clone();
        Self {
            message: message.into(),
            input_possible: true,
            obligation,
            outcome,
        }
    }

    /// 附加有界收尾事实，不改动失败分类本身。
    fn with_outcome_cleanup(mut self, cleanup: HelperCleanupFacts) -> Self {
        self.outcome.cleanup = Some(cleanup);
        self
    }

    /// 失败分类复用既有码表（`stroke_cancelled` / `mouse_release_failed` / `helper_lost` …），
    /// 不新造第二套分类器；`stroke_cancelled` 只是既有取消码的名字，不代表笔画路径。
    #[must_use]
    pub fn kind(&self) -> StrokeFailureKind {
        StrokeFailureKind::classify(&self.message)
    }

    #[must_use]
    pub fn facts(&self) -> Option<&NativeInputFacts> {
        self.outcome.facts()
    }

    #[must_use]
    pub fn cleanup(&self) -> Option<&HelperCleanupFacts> {
        self.outcome.cleanup()
    }

    #[must_use]
    pub fn reply_received_at_ms(&self) -> Option<u64> {
        self.outcome.reply_received_at_ms
    }

    #[must_use]
    pub fn process_exit_confirmed_at_ms(&self) -> Option<u64> {
        self.outcome.process_exit_confirmed_at_ms
    }

    /// 是否确认子进程静止；未确认静止不得被当成"已经停下"。
    #[must_use]
    pub fn stillness_confirmed(&self) -> bool {
        self.outcome.stillness_confirmed()
    }

    #[must_use]
    pub fn release_status(&self) -> InputReleaseStatus {
        self.release_state().release_status()
    }

    /// 释放义务的五态（CU-F01）。**唯一**推导点见 [`derive_release_obligation`]。
    #[must_use]
    pub fn release_state(&self) -> ReleaseObligationState {
        derive_release_obligation(&self.release_inputs())
    }

    /// 推导释放义务的共同输入：运行事实 + 本动作"请求是否已送到 helper"。
    #[must_use]
    pub fn release_inputs(&self) -> ReleaseDerivationInputs<'_> {
        let mut inputs = self.outcome.release_inputs(self.kind());
        inputs.input_possible = self.input_possible;
        inputs
    }

    /// 是否必须隔离：释放未确认，或静止未确认。
    #[must_use]
    pub fn must_quarantine(&self) -> bool {
        self.release_status() == InputReleaseStatus::Unknown || !self.stillness_confirmed()
    }
}

impl std::fmt::Display for NativeInputFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let release = match self.release_status() {
            InputReleaseStatus::NotNeeded => "not_needed",
            InputReleaseStatus::Released => "released",
            InputReleaseStatus::Unknown => "unknown",
        };
        write!(
            formatter,
            "{}（义务={} 已注入步数={:?} 释放={release} 静止={}）",
            self.message,
            self.obligation.summary(),
            self.facts().map(|facts| facts.injected_steps),
            self.stillness_confirmed()
        )
    }
}

impl std::error::Error for NativeInputFailure {}

/// 受控原生输入的成功回执：输入已发出，完整性与释放按 helper 事实对账。
#[must_use]
pub fn native_success_receipt(action_id: &str, outcome: &NativeInputOutcome) -> ActionReceipt {
    let receipt = ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery: InputDelivery::Sent,
        // 没有事实就不写"完整/部分"结论：未知不能被读成完整。
        partial: outcome.facts().map(|facts| !facts.completed),
        path_completed: None,
        confirmed_point_count: None,
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release: outcome.release_status(),
    };
    debug_assert!(
        validate_receipt_for_write(&receipt).is_ok(),
        "受控原生输入的成功事实必须能构成可写入的自洽回执：{receipt:?}"
    );
    receipt
}

/// 失败路径的回执：把 helper 自己的事实落成自洽回执。
///
/// - 能证明输入前失败 → 明确的"未发送"；
/// - 已注入但未走完 → 保留部分输入与释放义务；
/// - 走完但释放失败 → 分别记录"走完"与"释放未确认"；
/// - 失联且没有任何事实 → 维持未知（可能已发送 + 释放未知），不推断零输入。
#[must_use]
pub fn native_failure_receipt(action_id: &str, failure: &NativeInputFailure) -> ActionReceipt {
    native_failure_receipt_with(action_id, failure, None)
}

/// 同一次动作里前一段输入**已确认成功**（例如点击已经点到），后一段原生输入失败。
///
/// `prior_release` 是前一段的释放对账结论；两段必须一并计入（`Unknown` 优先）。
#[must_use]
pub fn native_failure_receipt_after_confirmed_input(
    action_id: &str,
    failure: &NativeInputFailure,
    prior_release: InputReleaseStatus,
) -> ActionReceipt {
    native_failure_receipt_with(action_id, failure, Some(prior_release))
}

fn native_failure_receipt_with(
    action_id: &str,
    failure: &NativeInputFailure,
    prior_release: Option<InputReleaseStatus>,
) -> ActionReceipt {
    let inputs = failure.release_inputs();
    // 投递事实与释放义务都来自**同一份**推导：这里不再自己看字段做判断。
    let delivery = derive_delivery_facts(&inputs);
    let release = derive_release_obligation(&inputs);
    let (input_delivery, partial) = match prior_release {
        // 前一段已确认发出：整条动作属于"已知的部分执行"，绝不退回 not_sent。
        Some(_) => (InputDelivery::Sent, Some(true)),
        None => (delivery.input_delivery, delivery.partial),
    };
    let input_release = match prior_release {
        Some(prior) => merge_input_release(prior, release.release_status()),
        None => release.release_status(),
    };
    let receipt = ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery,
        partial,
        path_completed: None,
        confirmed_point_count: None,
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release,
    };
    debug_assert!(
        validate_receipt_for_write(&receipt).is_ok(),
        "受控原生输入的失败事实必须能构成可写入的自洽回执：{receipt:?}"
    );
    receipt
}

/// 一次受控原生运行的参数：身份/scope 与取消信号。
pub struct NativeInputAttempt<'a> {
    /// 期望窗口身份；`Some` 时 helper 会在**注入之前**校验前台窗口，不匹配即拒绝注入。
    pub window: Option<StrokeWindow>,
    /// 宿主取消信号：输入开始前、每个阶段之间、以及收尾阶段都会看它。
    pub cancelled: &'a dyn Fn() -> bool,
}

impl<'a> NativeInputAttempt<'a> {
    /// 没有窗口身份校验的尝试（仅在没有可观察身份时使用）。
    #[must_use]
    pub fn without_identity(cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            window: None,
            cancelled,
        }
    }

    /// 带窗口身份的尝试。
    #[must_use]
    pub fn with_window(window: StrokeWindow, cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            window: Some(window),
            cancelled,
        }
    }
}

/// 受控点击（`clicks > 1` = 双击）；释放义务按实际按钮登记（左键）。
pub fn controlled_click(
    x: i32,
    y: i32,
    clicks: u32,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    if clicks == 0 || clicks > 2 {
        return Err(NativeInputFailure::before_input(
            "invalid_native_clicks: 点击次数只能是 1 或 2",
        ));
    }
    let action = if clicks > 1 {
        MouseButtonAction::DoubleClick
    } else {
        MouseButtonAction::LeftClick
    };
    controlled_mouse_button_action(x, y, action, attempt, timeout)
}

/// 受控鼠标按钮动作（左键 / 右键 / 左右和弦 / 双击）。
pub fn controlled_mouse_button_action(
    x: i32,
    y: i32,
    action: MouseButtonAction,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    let params = serde_json::json!({
        "action": click_action_name(action),
        "x": x,
        "y": y,
    });
    let backend = active_backend();
    native_run(
        "click",
        params,
        click_release_obligation(action),
        attempt,
        timeout,
        backend,
    )
}

/// 受控鼠标按钮状态（显式按下 / 抬起）。
///
/// 按下会登记该按钮的释放义务；成功按下之后按键**仍处于按下状态**，
/// 因此这次运行的释放对账是 `Unknown`（未知即隔离），直到后续的抬起被确认。
pub fn controlled_mouse_button_state(
    x: i32,
    y: i32,
    button: MouseButton,
    is_down: bool,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    let params = serde_json::json!({
        "action": mouse_button_name(button),
        "x": x,
        "y": y,
    });
    let backend = active_backend();
    native_run(
        if is_down { "down" } else { "up" },
        params,
        button_state_release_obligation(button, is_down),
        attempt,
        timeout,
        backend,
    )
}

/// 受控**绝对**移动：只改光标位置，**永不按下**任何按钮/键。
///
/// 释放义务是 `none`（移动不产生义务），但**受监督运行、事实落盘、收尾确认**一样不少：
/// 它证明"光标移动过"（`cursor_moved`）并如实报告未走完/失败。
pub fn controlled_move_mouse_absolute(
    x: i32,
    y: i32,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    native_run(
        "move",
        serde_json::json!({ "x": x, "y": y }),
        ReleaseObligation::none(),
        attempt,
        timeout,
        active_backend(),
    )
}

/// 受控**相对**移动：`dx`/`dy` 相对当前光标位置。
///
/// 位置由 **helper 自己**在同一段受监督运行里读（`CursorPosition`），
/// 而不是主机在外面另探一次——那会重新引入一条不受监督的路径。
pub fn controlled_move_mouse_relative(
    dx: i32,
    dy: i32,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    native_run(
        "move_relative",
        serde_json::json!({ "x": dx, "y": dy }),
        ReleaseObligation::none(),
        attempt,
        timeout,
        active_backend(),
    )
}

/// 受控文本输入：不点击，只输入文本（点击由调用方作为前一段输入）。
pub fn controlled_type_text(
    text: &str,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    validate_native_text(text)?;
    // 文本的释放义务取决于后端（SendInput 无义务、Interception 按修饰键），因此先解析一次。
    let backend = active_backend();
    let obligation = text_release_obligation(backend);
    let params = serde_json::json!({ "text": text });
    native_run("text", params, obligation, attempt, timeout, backend)
}

/// 受控滚动（`delta` 为滚轮量，正负表示方向）。
pub fn controlled_scroll(
    delta: i32,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    if delta == 0 || delta.checked_abs().is_none_or(|value| value > 600) {
        return Err(NativeInputFailure::before_input(
            "invalid_native_scroll: 滚动量必须非零且绝对值不超过 600",
        ));
    }
    let params = serde_json::json!({ "delta": delta });
    let backend = active_backend();
    native_run(
        "scroll",
        params,
        scroll_release_obligation(),
        attempt,
        timeout,
        backend,
    )
}

/// 受控按键（按住 70ms，与既有 `diagnostic_press_virtual_key` 的语义一致）。
pub fn controlled_press_key(
    virtual_key: u8,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    controlled_hold_key(virtual_key, 70, attempt, timeout)
}

/// 受控按键（指定按住时长）；释放义务按**这一个键**登记。
pub fn controlled_hold_key(
    virtual_key: u8,
    hold_ms: u64,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    validate_native_keys(&[virtual_key], false)?;
    if hold_ms == 0 || hold_ms > 2_000 {
        return Err(NativeInputFailure::before_input(
            "invalid_native_hold: 按住时长必须在 1–2000ms 之间",
        ));
    }
    let params = serde_json::json!({ "keys": [u32::from(virtual_key)], "hold_ms": hold_ms });
    let backend = active_backend();
    native_run(
        "key",
        params,
        key_release_obligation(&[virtual_key]),
        attempt,
        timeout,
        backend,
    )
}

/// 受控组合键；释放义务按**组合里的每个键**登记（一个一个按键，不是整块键盘）。
pub fn controlled_key_combo(
    virtual_keys: &[u8],
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    validate_native_keys(virtual_keys, true)?;
    let keys = virtual_keys
        .iter()
        .map(|key| u32::from(*key))
        .collect::<Vec<_>>();
    let params = serde_json::json!({ "keys": keys });
    let backend = active_backend();
    native_run(
        "combo",
        params,
        key_release_obligation(virtual_keys),
        attempt,
        timeout,
        backend,
    )
}

/// 输入前的身份／权限／scope 校验 + 受监督执行。
///
/// 校验失败 = 明确的"输入尚未开始"（`input_possible == false`，回执可写未发送）；
/// 其中"目标点是否真的在桌面可见范围内"由 helper 在注入之前最后复核一次。
fn native_run(
    mode: &str,
    params: serde_json::Value,
    obligation: ReleaseObligation,
    attempt: &NativeInputAttempt,
    timeout: Duration,
    backend: InputBackend,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    validate_native_attempt(attempt, backend)?;
    let request = native_request(mode, params, attempt.window);
    run_native_helper(
        request,
        &obligation,
        timeout,
        attempt.cancelled,
        None,
        backend,
        NativeRunCapacity::OrdinaryAction,
    )
}

/// 输入前的身份／权限／scope 校验。后端由调用方解析一次后传进来（不重复预检）。
fn validate_native_attempt(
    attempt: &NativeInputAttempt,
    backend: InputBackend,
) -> Result<(), NativeInputFailure> {
    if let Some(window) = attempt.window {
        validate_native_window(window).map_err(NativeInputFailure::before_input)?;
    }
    // 权限/scope：后端必须真的可用。Interception 需要 DLL；不可用就宁可不注入。
    match backend {
        InputBackend::SendInput => Ok(()),
        InputBackend::Interception => ensure_interception_dll()
            .map(|_| ())
            .map_err(|error| NativeInputFailure::before_input(format!("interception_backend_unavailable: {error}"))),
    }
}

/// 窗口身份的有效性：与受控笔画同一套判据（handle/pid/dpi 非零、矩形自洽）。
fn validate_native_window(window: StrokeWindow) -> Result<(), String> {
    let valid_rect = |rect: [i32; 4]| {
        rect[2] > 0 && rect[3] > 0 && rect[0].checked_add(rect[2]).is_some() && rect[1].checked_add(rect[3]).is_some()
    };
    if window.handle == 0 || window.process_id == 0 || window.dpi == 0 || !valid_rect(window.rect) {
        return Err("invalid_native_identity: 期望窗口身份或边界无效".to_string());
    }
    Ok(())
}

fn validate_native_text(text: &str) -> Result<(), NativeInputFailure> {
    let units = text.encode_utf16().count();
    if units == 0 || units > MAX_NATIVE_TEXT_UNITS {
        return Err(NativeInputFailure::before_input(format!(
            "invalid_native_text: 文本需要 1–{MAX_NATIVE_TEXT_UNITS} 个 UTF-16 单元"
        )));
    }
    Ok(())
}

fn validate_native_keys(virtual_keys: &[u8], allow_multiple: bool) -> Result<(), NativeInputFailure> {
    let count_ok = if allow_multiple {
        (1..=8).contains(&virtual_keys.len())
    } else {
        virtual_keys.len() == 1
    };
    if !count_ok || virtual_keys.contains(&0) {
        return Err(NativeInputFailure::before_input(
            "invalid_native_keys: 虚拟键数量或取值无效",
        ));
    }
    Ok(())
}

#[must_use]
fn click_action_name(action: MouseButtonAction) -> &'static str {
    match action {
        MouseButtonAction::LeftClick => "left",
        MouseButtonAction::RightClick => "right",
        MouseButtonAction::LeftRightChord => "chord",
        MouseButtonAction::DoubleClick => "double",
    }
}

/// 受控原生输入的请求构造：**生产路径唯一**的请求写入点（键名与 helper 严格对应）。
fn native_request(mode: &str, params: serde_json::Value, window: Option<StrokeWindow>) -> serde_json::Value {
    serde_json::json!({
        "mode": mode,
        "params": params,
        "identity": window.map(|window| serde_json::json!({
            "handle": window.handle,
            "process_id": window.process_id,
            "rect": window.rect,
            "dpi": window.dpi,
        })),
    })
}

/// 固定的受控原生 helper 脚本：一段 C#（本机制自己的驱动/引擎/事实落盘）+ 一段入口
/// PowerShell（只做参数搬运，不做任何脚本执行）。
///
/// 脚本经 `-Command` 内联传递，因此**整条命令行必须留在 Windows 的配额之内**：
/// `CreateProcessW` 的上限是**含终止空字符在内**的 32,767 个 **UTF-16 单元**（口径见
/// `native_helper_stays_within_the_windows_command_line_limit`，它会在超限时直接测试失败，
/// 而不是留到运行时才报"文件名或扩展名太长"；**不得**把单元数读成字节数或"字符数"）。
///
/// 身份判定用本段内的 `NativeIdentity`：判据与诊断措辞和受控笔画的 `IdentityCheck` 一致
/// （各自那一侧都有用例钉住措辞），两份实现不允许出现两种说法。
fn native_helper_script() -> String {
    format!(
        "$ErrorActionPreference='Stop'; $OutputEncoding=[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); \
         Add-Type -TypeDefinition @'\n{}\n'@\n{}",
        NATIVE_INPUT_HELPER_CS, NATIVE_INPUT_HELPER_ENTRY
    )
}

/// 受控原生 helper 的**生产命令构造**（唯一构造点）。
///
/// 命令行的每一个字都必须经过同一条路径：`-Command` 内联脚本是唯一会随 helper 长大的
/// 部分，命令行计量（§5.1）也直接量这条命令——而不是另写一份"估算公式"。
fn native_helper_command(script: &str) -> Command {
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// **尝试**启动受控原生 helper 的次数（含失败尝试）。
///
/// 它是"容量不足时连进程都不创建"这句话的唯一证据来源：一次被容量拒绝的运行，
/// 这个计数**不会**增加（而"先 spawn 再申请容量"的旧顺序一定会增加）。
static NATIVE_HELPER_SPAWN_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

fn note_native_helper_spawn_attempt() {
    NATIVE_HELPER_SPAWN_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
}

/// 已经**尝试**启动受控原生 helper 的次数（只读；测试用来证明"容量不足时进程数为零"）。
#[cfg(test)]
fn test_native_helper_spawn_attempts() -> u64 {
    NATIVE_HELPER_SPAWN_ATTEMPTS.load(Ordering::Acquire)
}

/// 普通动作一次接纳要预留的读取器容量单位数：
/// **本 helper 两条流（stdout/stderr）+ 可能的一次清理（独立释放）**。
///
/// 生产路径的实际需求由 `HelperPipeReadersAdmission::acquire_with_cleanup_reserve()` 一处给出；
/// 这个常量只服务测试期的进程级容量闸门（生产构建不编译闸门，也就不需要它）。
#[cfg(test)]
const NATIVE_RUN_READER_UNITS: usize = PIPE_READERS_PER_HELPER * 2;

/// 一次运行的读取器容量来源（§C-21 选②）。
#[derive(Debug)]
enum NativeRunCapacity {
    /// 普通业务动作：在 `spawn` **之前**一次性预留 `2 + 2` 个单位
    /// （本 helper 两条流 + 可能的一次清理），容量不足时连进程都不创建。
    OrdinaryAction,
    /// 清理运行（独立释放）：**转用**上层已经预留的那份清理容量，
    /// 不再重新竞争普通容量，也**不再**预留下一层清理额度（不递归）。
    CleanupRun(HelperCleanupReservation),
}

/// 是否需要由本进程独立补发一次释放（不依赖 helper 的 `finally`）。
///
/// 四个条件同时成立才补发：受控输入（不是释放通道自身）、登记过义务、helper 没有自己确认过
/// 释放结果（成功或明确报失败）、以及**可能真的按住过东西**。第三、四条来自受控笔画的既有
/// 判定：被强杀、或自己以非零状态退出时 `finally` 可能没跑到。
///
/// `facts_may_hold_something` 由 helper 的事实决定：`None` = 没有事实 → **按可能按住处理**
/// （保守方向）；有事实时看它是否按下过、是否仍按住。反过来，如果事实明确"一步都没注入、
/// 什么都没按下"（例如身份不匹配被拒），这里就**不补发**——不给一个可证明零注入的运行
/// 发出多余的 UP 事件（这点比受控笔画更严：笔画路径只要被强杀就无条件补发）。
/// 是否需要由本进程独立补发一次释放（不依赖 helper 的 `finally`）。
///
/// 判定**消费事实推导出的五态**，不再自己拼字段条件：
/// - 已证明不存在 / 已产生且已结清 → 不补发；
/// - 已产生且未结清 → 在既定安全条件（被强杀 / 以非零状态退出）下补发**一次**；
/// - 可能存在 / 证据冲突 → 先确认旧执行者静止，满足条件才补发，否则隔离；
/// - 释放通道自身（`mode = release`）永不递归补发。
///
/// "helper 自己报过释放失败"不再是跳过补发的理由（那会因为明确失败反而完全放弃收尾）；
/// 它由五态表达为"未结清／证据冲突"，仍然**最多一次**。
fn needs_native_emergency_release(
    mode_controls_input: bool,
    state: ReleaseObligationState,
    stillness_confirmed: bool,
    forced_kill: bool,
    helper_exited_ok: bool,
) -> bool {
    if !mode_controls_input || !state.requires_controlled_cleanup() {
        return false;
    }
    // 只有"可能存在／证据冲突"才要求先确认静止；已有按下事实的未结清义务按既定条件收尾。
    if state.requires_stillness_before_cleanup() && !stillness_confirmed {
        return false;
    }
    forced_kill || !helper_exited_ok
}

/// 是否属于"执行者失联"：被本进程强杀，或非零退出却没给出任何原因。
///
/// 注意非零退出**不等于**失联：helper 主动报错时同样是抛出后非零退出并把原因写进 stderr——
/// 那属于"报告了失败"，应保留原有分类。判定与受控笔画完全一致（`is_helper_lost`）。
fn native_helper_lost(forced_kill: bool, helper_exited_ok: bool, helper_reported_failure: bool) -> bool {
    forced_kill || (!helper_exited_ok && !helper_reported_failure)
}

/// helper 输出管道读取监督器（原生监督器的 CU 侧别名）。
///
/// 它是**读取线程、线程句柄与缓冲区的唯一所有权点**：没有它，任何 "join"/"等读取结束"
/// 都可能被孙进程持有的管道写端拖成无界等待。
#[cfg(windows)]
pub(crate) type HelperPipeReader = windows_process_guard::PipeReaderSupervisor;

pub(crate) use crate::cleanup::{PipeSupervisionFacts, PIPE_DRAIN_DIAGNOSTIC_WAIT_MS};

/// 管道收尾结果：文本 + 管道事实 + 从已收到字节里解析出的协议记录。
#[cfg(windows)]
pub(crate) struct HelperPipeOutput {
    /// stdout 的文本视图。
    pub(crate) stdout: String,
    /// stderr 的文本视图。
    pub(crate) stderr: String,
    /// 管道收尾事实（写入 `HelperCleanupFacts.pipe`）。
    pub(crate) facts: PipeSupervisionFacts,
    /// 已经收到的**完整行**协议记录（不依赖 EOF）。
    pub(crate) stdout_records: Vec<String>,
}

/// 有界地收集一个管道读取线程的结果。
///
/// ## 为什么不再用"分离线程 + recv_timeout"
///
/// 旧实现把 `join()` 放进一个**被分离**的线程里，超时就返回空输出。那既没有取消读取、
/// 也没有核实线程是否结束，还把读取线程与其缓冲区失管（无人再持有、无法对账）。
/// CU-F02 要求：未结束的读取线程、句柄与缓冲区**仍由监督器持有**，
/// 且"发出取消"**不等于**"读取已经结束"。
///
/// 现在读取线程由原生监督器（`windows-process-guard` 的有界管道读取器）持有：
/// 它只看"已经可读"的字节、不发出可能无界阻塞的读取，因此**孙进程是否仍持有管道写端
/// 都不影响它退出**；收尾只做有界等待并核实完成状态。
#[cfg(windows)]
mod helper_pipes {
    use super::{
        complete_lines, CleanupDeadline, CleanupPolicy, HelperPipeOutput, HelperPipeReader,
        PipeSupervisionFacts, PIPE_DRAIN_DIAGNOSTIC_WAIT_MS,
    };
    use std::process::{ChildStderr, ChildStdout};
    use std::time::{Duration, Instant};
    use windows_process_guard::HelperPipeReadersAdmission;

    /// 建立 stdout 读取监督器；失败**不**降级成"读了空输出"。
    ///
    /// 生产路径一律走 [`supervise_stdout_from`]（容量在 `spawn` 之前已经预留）；
    /// 这个不带凭证的版本只服务测试夹具（它自己那两条流没有走接纳凭证）。
    #[cfg(test)]
    pub(crate) fn supervise_stdout(
        label: &str,
        pipe: ChildStdout,
    ) -> Result<HelperPipeReader, String> {
        HelperPipeReader::stdout(label.to_string(), pipe)
            .map_err(|error| format!("helper stdout 监督器建立失败: {error}"))
    }

    /// 建立 stderr 读取监督器（只服务测试夹具，见 [`supervise_stdout`]）。
    #[cfg(test)]
    pub(crate) fn supervise_stderr(
        label: &str,
        pipe: ChildStderr,
    ) -> Result<HelperPipeReader, String> {
        HelperPipeReader::stderr(label.to_string(), pipe)
            .map_err(|error| format!("helper stderr 监督器建立失败: {error}"))
    }

    /// 用**事先取得**的接纳凭证建立 stdout 读取监督器（§C-21 选②）。
    ///
    /// 容量不足时调用方在 `spawn` **之前**就已被拒绝，因此生产路径不再有
    /// "先起进程、再发现没有读取器容量"的顺序。
    pub(crate) fn supervise_stdout_from(
        admission: &mut HelperPipeReadersAdmission,
        label: &str,
        pipe: ChildStdout,
    ) -> Result<HelperPipeReader, String> {
        admission
            .stdout(label.to_string(), pipe)
            .map_err(|error| format!("helper stdout 监督器建立失败: {error}"))
    }

    /// 用**事先取得**的接纳凭证建立 stderr 读取监督器（§C-21 选②）。
    pub(crate) fn supervise_stderr_from(
        admission: &mut HelperPipeReadersAdmission,
        label: &str,
        pipe: ChildStderr,
    ) -> Result<HelperPipeReader, String> {
        admission
            .stderr(label.to_string(), pipe)
            .map_err(|error| format!("helper stderr 监督器建立失败: {error}"))
    }

    /// 管道收尾的等待额度：`min(诊断片长, 协作退出额度, 剩余收尾时间)`。
    ///
    /// 三个数都来自 `cleanup.rs` 的唯一策略定义点：没有收尾窗口时（正常成功路径）
    /// 取"诊断片长与协作退出额度的较小者"——**不建立第二个收尾窗口**。
    fn drain_slice(deadline: Option<CleanupDeadline>, policy: CleanupPolicy) -> Duration {
        let slice = Duration::from_millis(PIPE_DRAIN_DIAGNOSTIC_WAIT_MS)
            .min(policy.cooperative_exit_grace());
        deadline.map_or(slice, |deadline| {
            deadline.remaining_at(Instant::now()).min(slice)
        })
    }

    /// 有界收尾两个管道读取线程，并如实汇总事实。
    ///
    /// 两个读取线程**共同消耗**同一个收尾窗口：第二个只拿第一个用完之后的剩余额度。
    ///
    /// 收尾末尾还会做一次**非阻塞**清扫（[`windows_process_guard::reclaim_finished_pipe_readers`]）：
    /// 回收逻辑不会在一个永不结束的任务上阻塞其他所有任务。
    pub(crate) fn drain(
        out: HelperPipeReader,
        err: HelperPipeReader,
        deadline: Option<CleanupDeadline>,
        policy: CleanupPolicy,
    ) -> HelperPipeOutput {
        let started = Instant::now();
        // 收尾窗口的数值仍只在 `cleanup.rs`：这里只消费它，不另设第二套。
        let out_drain = out.drain(drain_slice(deadline, policy));
        // 第二个读取线程只拿第一个用完之后的剩余额度：两者共同消耗同一个窗口。
        let err_drain = err.drain(drain_slice(deadline, policy));
        let facts = PipeSupervisionFacts {
            stdout_confirmed: out_drain.completion.is_confirmed(),
            stderr_confirmed: err_drain.completion.is_confirmed(),
            // `readers_retained` = **真实受持有数量**（一个 still-held 读取器贡献 1）；
            // 被拒绝接纳（根本没创建）不在这里出现，也不会被计成 1。
            readers_retained: u32::try_from(
                out_drain.retention.holds_unreclaimed_reader() as usize
                    + err_drain.retention.holds_unreclaimed_reader() as usize,
            )
            .unwrap_or(u32::MAX),
            dropped_bytes: out_drain.dropped_bytes.saturating_add(err_drain.dropped_bytes),
            truncated: out_drain.truncated || err_drain.truncated,
            waited_ms: started
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            eof_seen: out_drain.eof_seen && err_drain.eof_seen,
            // 超出正常容量不变量但**仍被持有**的数量（两路取最大是因为两路读的是同一个
            // 进程级登记表的快照，相加会把同一个故障对象数两次）。
            capacity_fault_units: out_drain
                .retention
                .capacity_fault_units()
                .max(err_drain.retention.capacity_fault_units())
                as u64,
        };
        // 协议记录从"已经收到的字节"里解析出来：**不依赖 EOF**，
        // 也不因为收尾未核实就当作"没有回执"。
        let stdout_records = complete_lines(&out_drain.bytes);
        // 延后回收：每次收尾顺手做一次**非阻塞**清扫，把此前已核实结束的残留读取交还给系统。
        // 清扫不等待任何线程，因此永远不会被"还没结束的读取"挡住。
        let _ = windows_process_guard::reclaim_finished_pipe_readers();
        HelperPipeOutput {
            stdout: out_drain.text_lossy(),
            stderr: err_drain.text_lossy(),
            facts,
            stdout_records,
        }
    }

    /// 轮询期间的增量协议事实：只看"**已经收到**"的字节，不等 EOF。
    ///
    /// 用在监督循环里判断"收到回复"——这样回执不依赖 EOF：即使孙进程继续持有管道写端、
    /// 读取线程还没拿到 EOF，已经到达的协议记录也照样被算数。
    pub(crate) fn snapshot_text(reader: &HelperPipeReader) -> String {
        reader
            .snapshot()
            .map_or_else(String::new, |snapshot| snapshot.text_lossy())
    }
}

/// 从"已经收到的字节"里解析**有边界的**协议记录：只取以换行结尾的完整行。
///
/// 末尾那段没有换行的半行**不算**记录（它可能是被截断的半个回执）；
/// 调用方要"已经到达但尚未成行"的文本视图时请直接用已收到的字节，
/// 两种读法都不依赖 EOF——这正是"持续解析有边界的协议记录"与"等 EOF 才行动"的区别。
#[cfg(windows)]
pub(crate) fn complete_lines(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut records = Vec::new();
    let mut rest = text.as_ref();
    while let Some(index) = rest.find('\n') {
        let (line, tail) = rest.split_at(index + 1);
        rest = tail;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if !trimmed.is_empty() {
            records.push(trimmed.to_string());
        }
    }
    records
}

/// 无法确认静止时的失败：执行者**可能仍在运行**，必须按失联 + 释放未确认隔离。
fn native_stillness_failure(
    obligation: &ReleaseObligation,
    outcome: NativeInputOutcome,
) -> NativeInputFailure {
    NativeInputFailure::after_input(
        "helper_lost: 受控原生输入 helper 无法确认静止（执行者可能仍在运行）".to_string(),
        obligation.clone(),
        outcome,
    )
}

/// 测试专用：把一次受控原生运行交给内存驱动（**不驱动真实鼠标键盘**）。
///
/// 生产路径的请求构造器与监督循环完全复用，只有注入驱动被替换：`mock_scenario` 只能由测试
/// 直接构造的请求带来，生产代码里没有任何写入该键的调用点。
#[cfg(test)]
fn native_run_with_mock(
    mode: &str,
    params: serde_json::Value,
    obligation: &ReleaseObligation,
    attempt: &NativeInputAttempt,
    timeout: Duration,
    mock_scenario: &str,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    let backend = active_backend();
    validate_native_attempt(attempt, backend)?;
    let request = native_request(mode, params, attempt.window);
    run_native_helper(
        request,
        obligation,
        timeout,
        attempt.cancelled,
        Some(mock_scenario),
        backend,
        NativeRunCapacity::OrdinaryAction,
    )
}

/// 受监督地运行受控原生 helper。
///
/// 收尾策略与受控笔画**共用** `crate::cleanup`：协作退出等待 `min(2 秒, 剩余收尾时间)`、
/// 总窗口 ≤4 秒、收尾截止时间只在第一次进入取消/异常收尾时固定且**不被重复信号刷新**、
/// 独立释放最多一次且等待 ≤ `min(2 秒, 剩余收尾时间)`。
///
/// `mock_scenario` 只由测试入口传入（生产调用恒为 `None`）：它让 helper 用内存驱动替代真实注入，
/// 从而在**不驱动真实鼠标键盘**的前提下验证生命周期本身。
#[cfg(windows)]
fn run_native_helper(
    request: serde_json::Value,
    obligation: &ReleaseObligation,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
    mock_scenario: Option<&str>,
    backend: InputBackend,
    capacity: NativeRunCapacity,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    if !cfg!(windows) {
        return Err(NativeInputFailure::before_input("受控原生输入仅支持 Windows"));
    }
    // 测试专用（生产构建不编译）：helper 运行会占用读取器容量，因此先过进程级测试闸门，
    // 避免并行用例互相挤爆读取器容量。**普通动作**按"本 helper 两条流 + 一次清理"计。
    #[cfg(test)]
    let _test_pipe_slot = test_pipe_reader_capacity_slot(NATIVE_RUN_READER_UNITS);
    // ------------------------------------------------------------------
    // 接纳顺序（§C-21 选②）：资格校验 → **计算最大 reader 需求** → **一次性预留** →
    // 创建并监督 helper → 创建读取器 → 最终输入前检查 → 允许业务输入。
    //
    // "一次性预留"在 `spawn` **之前**：容量不足时**连进程都不创建**（旧顺序是
    // "先 spawn helper，再逐条流申请容量，拒绝后再 kill"）。
    // 普通动作连"可能的一次清理（独立释放）"所需的容量一起预留，避免收尾死角；
    // 清理运行**转用**上层已经预留的那份，既不重新竞争普通容量，也不递归预留下一层。
    let (mut admission, cleanup_reservation) = match capacity {
        NativeRunCapacity::OrdinaryAction => {
            let mut admission = HelperPipeReadersAdmission::acquire_with_cleanup_reserve()
                .map_err(|denial| {
                    NativeInputFailure::before_input(format!(
                        "受控原生输入 helper 读取器容量不足（未创建进程、未产生输入）: {denial}"
                    ))
                })?;
            // 清理额度**在这里**就从普通动作的凭证里切出来单独持有：
            // 它仍然在账上，只是被专门留给"可能的一次独立释放"。
            let cleanup = admission.take_cleanup_reservation();
            (admission, cleanup)
        }
        // 清理运行本身**不再**预留下一层清理额度（不递归）。
        NativeRunCapacity::CleanupRun(reserved) => (reserved.into_admission(), None),
    };
    let dll = match backend {
        InputBackend::SendInput => None,
        InputBackend::Interception => Some(
            ensure_interception_dll()
                .map_err(|error| NativeInputFailure::before_input(format!("interception_backend_unavailable: {error}")))?,
        ),
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nonce = format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let cancel_file = std::env::temp_dir().join(format!("coolzhu-native-cancel-{nonce}"));
    let progress_file = std::env::temp_dir().join(format!("coolzhu-native-progress-{nonce}.json"));
    let _ = std::fs::remove_file(&progress_file);
    let mode_controls_input = request["mode"] != "release";
    let policy = native_cleanup_policy();
    // helper 自守望 = 业务期限 + 收尾窗口：宿主意外消失时它也必须自己停下。
    let watchdog_ms = timeout
        .as_millis()
        .saturating_add(policy.cleanup_window().as_millis())
        .min(u128::from(u64::MAX)) as u64;
    let mut request = request;
    request["obligation"] = obligation.request_json();
    request["backend"] = serde_json::json!(backend.as_str());
    // 请求身份随请求下发、由 helper 原样回写：进度记录必须能核对"这份事实属于本动作"。
    request["request_id"] = serde_json::json!(nonce);
    request["cancel_file"] = serde_json::json!(cancel_file.to_string_lossy());
    request["progress_file"] = serde_json::json!(progress_file.to_string_lossy());
    request["self_watchdog_ms"] = serde_json::json!(watchdog_ms);
    request["mouse_device_id"] = serde_json::json!(interception_mouse_device_id());
    request["keyboard_device_id"] = serde_json::json!(interception_keyboard_device_id());
    request["mock_scenario"] = serde_json::json!(mock_scenario);

    let script = native_helper_script();
    let mut command = native_helper_command(&script);
    if let Some(dll) = dll.as_ref().and_then(|dll| dll.parent()) {
        // interception.dll 只能从 DLL 搜索路径里解析：在启动之前把它放进子进程 PATH。
        // （环境变量不占命令行长度的额度，因此不影响 §5.1 的命令行计量口径。）
        let path = env::var("PATH").unwrap_or_default();
        command.env("PATH", format!("{};{}", dll.display(), path));
    }
    // 计数点：**尝试**启动执行者。容量不足时这个计数不会增加——"未创建进程"由此可证。
    note_native_helper_spawn_attempt();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            // 进程没建成：未用的额度（含清理预留）随凭证一起归还，不留幽灵占用。
            return Err(NativeInputFailure::before_input(format!(
                "无法启动受控原生输入 helper: {error}"
            )));
        }
    };
    // 8.3c：**在 helper 仍然存活时**捕获它的实例身份——这是唯一捕获点，复用既有能力
    // （`capture_process_identity`），**不新增第二条捕获路径**。
    // 必须在派发前捕获：进程退出后取不到身份，那时再"补一个"就等于编造。
    // pid + 创建时间构成最小身份；同 pid 不同创建时间 ⇒ 不是同一实例（PID 复用可识别）。
    let helper_process = windows_process_guard::capture_process_identity(child.id())
        .ok()
        .map(|identity| runtime::ProcessInstanceEvidence {
            pid: identity.pid(),
            creation_time_filetime: identity.creation_time_filetime(),
        });
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| NativeInputFailure::before_input("受控原生输入 helper stdin 不可用"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| NativeInputFailure::before_input("受控原生输入 helper stdout 不可用"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| NativeInputFailure::before_input("受控原生输入 helper stderr 不可用"))?;
    // 读取线程由原生监督器持有：只看"已经可读"的字节，**不发出无界阻塞的读取**。
    // 被强杀的 helper 可能留下孙进程（如 `Add-Type` 的编译器）继续持有管道写端，
    // 无界 `join` 会把有界收尾拖成无界等待——这里从机制上避开它。
    // 两条流从**凭证**里各带走 1 个单位：读取器的容量由此凭证在 spawn 之前已经预备。
    let out_reader = match admission.stdout("native-helper-stdout", stdout) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeInputFailure::before_input(format!(
                "helper stdout 监督器建立失败: {error}"
            )));
        }
    };
    let err_reader = match admission.stderr("native-helper-stderr", stderr) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            // out_reader 未被消费：它的 Drop 会把未核实的读取转入有上限的残留登记，不失管。
            return Err(NativeInputFailure::before_input(format!(
                "helper stderr 监督器建立失败: {error}"
            )));
        }
    };
    if let Err(error) = stdin.write_all(request.to_string().as_bytes()) {
        // 请求没送到 helper（写入不完整 ⇒ helper 连 JSON 都解析不出）：这是**可证明**的
        // 零输入，因此**不**补发任何多余 UP，收尾事实如实写"无需释放"。
        // 之前的实现无条件补发一次独立释放——那正是"起点/结构事实之外的乐观补发"。
        let killed = child.kill().is_ok();
        let waited = child.wait();
        drop(stdin);
        // 读到多少算多少：协议事实来自进度文件，管道的文本只作普通日志保留。
        let pipes = helper_pipes::drain(out_reader, err_reader, None, policy);
        let _ = std::fs::remove_file(&progress_file);
        let deadline = CleanupDeadline::establish(policy, unix_ms());
        let wait = deadline.independent_release_wait_at(Instant::now());
        let cleanup = HelperCleanupFacts {
            stopped_new_input_at_ms: deadline.started_at_unix_ms(),
            cleanup_started_at_ms: deadline.started_at_unix_ms(),
            cleanup_finished_at_ms: unix_ms(),
            cooperative_exit_waited_ms: 0,
            forced_kill: !(killed && waited.is_ok()),
            independent_release_issued: false,
            independent_release_wait_ms: wait.as_millis().min(u128::from(u64::MAX)) as u64,
            independent_release_confirmed: false,
            independent_release_skipped_window_expired: false,
            release: CleanupReleaseStatus::NotNeeded,
            pipe: Some(pipes.facts),
        };
        return Err(
            NativeInputFailure::before_input(format!("受控原生输入 helper 请求写入失败: {error}"))
                .with_outcome_cleanup(cleanup),
        );
    }
    drop(stdin);
    // 请求已经送到 helper：从这里开始不能再声称"零注入"。
    let started = Instant::now();
    // 第一次进入取消/异常收尾时建立**唯一**的收尾截止时间；之后的取消/超时信号复用它，
    // **不得刷新**（否则收尾窗口可以被续期）。
    let mut cleanup_deadline: Option<CleanupDeadline> = None;
    let mut cancellation_at: Option<Instant> = None;
    let mut cooperative_exit_waited = Duration::ZERO;
    let mut forced_kill = false;
    // 两个结束事实都由循环的每个出口显式赋值：不存在"默认值"这种读法。
    let exit_confirmed;
    let helper_exited_ok;
    let mut fact_read: HelperFactRead<NativeInputFacts> = HelperFactRead::Missing;
    let mut reply_received_at_ms: Option<u64> = None;
    // 轮询期间是否**已经看到**成功标记（增量、不依赖 EOF）。
    let mut marker_seen_during_poll = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_confirmed = true;
                helper_exited_ok = status.success();
                break;
            }
            Ok(None) => {}
            Err(_) => {
                // 状态读不出来：强杀；只有强杀成功且 wait 给出结束状态才算确认静止。
                let killed = child.kill().is_ok();
                forced_kill = true;
                exit_confirmed = killed && child.wait().is_ok();
                helper_exited_ok = false;
                break;
            }
        }
        // 阶段回执：helper 每确认一步都写进度文件；带释放结果的终态记录 = "收到回复"。
        match read_native_facts(&progress_file, &nonce) {
            HelperFactRead::Missing => {}
            HelperFactRead::Rejected { .. } => {
                // 不可信的记录不能被当成事实，也不能被静默丢弃：原因留到收尾后一并呈现。
                fact_read = read_native_facts(&progress_file, &nonce);
            }
            HelperFactRead::Trusted(facts) => {
                if facts.released.is_some() && reply_received_at_ms.is_none() {
                    reply_received_at_ms = Some(unix_ms());
                }
                fact_read = HelperFactRead::Trusted(facts);
            }
        }
        // 增量协议记录：只看"已经收到"的字节即认定收到成功标记（**不依赖 EOF**）。
        // 即使孙进程继续持有管道写端、读取线程还没见到 EOF，已经到达的回执也照样算数。
        if !marker_seen_during_poll
            && helper_pipes::snapshot_text(&out_reader).contains(NATIVE_HELPER_SUCCESS_MARKER)
        {
            marker_seen_during_poll = true;
        }
        if cancellation_at.is_none() && (cancelled() || started.elapsed() >= timeout) {
            // 独立哨兵文件只承载取消信号，不接收任何用户代码或命令。
            let _ = std::fs::write(&cancel_file, b"cancel");
            cancellation_at = Some(Instant::now());
            let fixed = CleanupDeadline::fixed(&mut cleanup_deadline, policy, unix_ms());
            cleanup_deadline = Some(fixed);
        }
        if let (Some(at), Some(deadline)) = (cancellation_at, cleanup_deadline) {
            let now = Instant::now();
            cooperative_exit_waited = now.saturating_duration_since(at);
            // 协作退出等待 = min(2 秒, 剩余收尾时间)：helper 自己退出时上面的 try_wait 会先结束循环。
            if at.elapsed() >= deadline.cooperative_exit_grace_at(now) {
                let killed = child.kill().is_ok();
                forced_kill = true;
                exit_confirmed = killed && child.wait().is_ok();
                helper_exited_ok = false;
                break;
            }
        }
        thread::sleep(Duration::from_millis(NATIVE_PROGRESS_POLL_MS));
    }
    // 有界收尾两个读取线程：**不等待孙进程自然退出**，只做有界等待并核实读取线程结束。
    // 等待额度取 `min(诊断片长, 剩余收尾时间)`，与协作退出／独立释放共同消耗同一个窗口。
    let pipes = helper_pipes::drain(out_reader, err_reader, cleanup_deadline, policy);
    // 事实必须在删除进度文件之前取出；退出后的读回优先，没有就退回轮询期间的最后一次读。
    let final_read = read_native_facts(&progress_file, &nonce);
    let _ = std::fs::remove_file(&progress_file);
    let _ = std::fs::remove_file(&cancel_file);
    let fact_read = match final_read {
        HelperFactRead::Missing => fact_read,
        other => other,
    };
    let facts = fact_read.trusted().cloned();
    let fact_anomaly = fact_read.anomaly().map(str::to_string);
    let stdout_text = pipes.stdout.as_str();
    let stderr_text = pipes.stderr.as_str();
    // 管道收尾事实（Copy，供收尾报告取用）：读取线程是否**核实**结束、缺口在哪。
    let pipe_facts = pipes.facts;
    let helper_reported_failure = !stderr_text.trim().is_empty();
    // "收到回复"的三个来源：轮询期间看到的终态记录，退出后读回的终态记录，
    // 以及"成功退出 + 完成标记"（标记可能来自轮询期间的**增量**记录，不等 EOF）。
    // 没有终态记录（例如被强杀）= 没有收到回复。
    if reply_received_at_ms.is_none() && facts.as_ref().is_some_and(|facts| facts.released.is_some()) {
        reply_received_at_ms = Some(unix_ms());
    }
    if reply_received_at_ms.is_none()
        && helper_exited_ok
        && (stdout_text.contains(NATIVE_HELPER_SUCCESS_MARKER) || marker_seen_during_poll)
    {
        reply_received_at_ms = Some(unix_ms());
    }
    let release_reported_failed = facts
        .as_ref()
        .is_some_and(|facts| facts.released == Some(false))
        || stderr_text.contains("input_release_unconfirmed");
    // 正推顺序（裁决 §2）：先收集原始执行结果与 helper 回执 → 校验身份/协议/完整性 →
    // 推导投递事实 → 推导释放义务 → 有义务时才收尾 → 合并收尾事实 → 生成最终分类。
    //
    // 第一步：**原始执行原因**只由 helper 自己的报告与进程事实决定，
    // 不被后面的补发结果覆盖（补发结果属于"安全收尾"这个独立维度）。
    let cause = NativeInputFailureCause::classify(
        release_reported_failed,
        cancellation_at.is_some(),
        exit_confirmed,
        native_helper_lost(forced_kill, helper_exited_ok, helper_reported_failure),
        helper_exited_ok,
    );
    let cause_kind = cause.kind();
    let mut derivation = NativeInputOutcome {
        obligation: obligation.clone(),
        // 上面在 spawn 成功后捕获的真实身份；捕获不到就是 None（不编造）。
        helper_process: helper_process.clone(),
        facts: facts.clone(),
        fact_anomaly: fact_anomaly.clone(),
        reply_received_at_ms,
        process_exit_confirmed_at_ms: exit_confirmed.then(unix_ms),
        cleanup: None,
    };
    // 第二步：推导释放义务（收尾之前）。
    let before_cleanup = derive_before_cleanup(&derivation.release_inputs(cause_kind));
    // 第三步：有义务或可能有义务时，按既定安全条件执行**一次**受控收尾。
    let release_needed = needs_native_emergency_release(
        mode_controls_input,
        before_cleanup,
        exit_confirmed,
        forced_kill,
        helper_exited_ok,
    );
    if cleanup_deadline.is_none() && (forced_kill || release_needed) {
        // "需收尾错误"同样属于第一次进入收尾：在这里固定收尾截止时间。
        cleanup_deadline = Some(CleanupDeadline::establish(policy, unix_ms()));
    }
    // 独立（补发）释放**最多一次**，等待上限 = min(2 秒, 剩余收尾时间)。
    let mut independent_release_issued = false;
    let mut independent_release_wait = Duration::ZERO;
    let mut independent_release_confirmed = false;
    let mut independent_release_skipped_window_expired = false;
    if release_needed {
        let wait = cleanup_deadline.map_or(policy.independent_release_wait_cap(), |deadline| {
            deadline.independent_release_wait_at(Instant::now())
        });
        if wait.is_zero() {
            // 收尾窗口已到期：不延长、不重试循环，直接按"未确认释放"隔离。
            independent_release_skipped_window_expired = true;
        } else {
            independent_release_issued = true;
            independent_release_wait = wait;
            // 独立释放**转用**接纳时已经预留的清理容量（不重新竞争普通容量）。
            match native_emergency_release(
                obligation,
                wait,
                mock_scenario,
                backend,
                cleanup_reservation,
            ) {
                Ok(()) => independent_release_confirmed = true,
                // 补发失败不改写原始执行原因：未结清的释放义务由收尾事实与五态表达，
                // 外部安全分类由调用链合并（见 `native_failure_error_with_release`）。
                Err(_) => {}
            }
        }
    }
    // 第四步：合并收尾事实，得到**最终**释放义务状态（唯一推导点，不在这里写第二条判定）。
    let release_state = derive_after_cleanup(&derivation.release_inputs(cause_kind), independent_release_confirmed);
    let release = release_state.release_status();
    derivation.cleanup = cleanup_deadline.map(|deadline| HelperCleanupFacts {
        stopped_new_input_at_ms: deadline.started_at_unix_ms(),
        cleanup_started_at_ms: deadline.started_at_unix_ms(),
        cleanup_finished_at_ms: unix_ms(),
        cooperative_exit_waited_ms: cooperative_exit_waited
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        forced_kill,
        independent_release_issued,
        independent_release_wait_ms: independent_release_wait
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        independent_release_confirmed,
        independent_release_skipped_window_expired,
        release: cleanup_release_status_of(release),
        // 管道收尾事实：与协议事实分开记录；未核实结束只记监督/I/O 故障，
        // **不**据此改写释放义务（不伪造"释放未知"）。
        pipe: Some(pipe_facts),
    });
    let outcome = derivation;
    // 第五步：最终分类 —— 安全收尾维度（未结清）与原始原因维度分别保留，不互相覆盖。
    if cause == NativeInputFailureCause::StillRunning {
        return Err(native_stillness_failure(obligation, outcome));
    }
    if let Some(cause) = cause.into_failure(stderr_text.trim(), &obligation) {
        return Err(NativeInputFailure::after_input(cause, obligation.clone(), outcome));
    }
    // 成功路径**不**在这里因为"义务仍未结清"改写结果：`down` 这类动作的意图就是按住，
    // 释放维度由回执与收尾报告如实给出；外部安全分类在调用链里合并。
    Ok(outcome)
}

/// 非 Windows：受控原生输入不可用（不会走到任何进程/管道路径）。
#[cfg(not(windows))]
fn run_native_helper(
    _request: serde_json::Value,
    _obligation: &ReleaseObligation,
    _timeout: Duration,
    _cancelled: &dyn Fn() -> bool,
    _mock_scenario: Option<&str>,
    _backend: InputBackend,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    Err(NativeInputFailure::before_input("受控原生输入仅支持 Windows"))
}

/// 原始执行原因（只看 helper 自己的报告与进程事实，不看本进程的补发结果）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeInputFailureCause {
    /// helper 自己报告释放失败。
    ReleaseReportedFailed,
    /// 已取消或超时。
    Cancelled,
    /// 无法确认静止（执行者可能仍在运行）。
    StillRunning,
    /// 执行者失联。
    HelperLost,
    /// 其它非零退出。
    Failed,
    /// 没有失败。
    None,
}

impl NativeInputFailureCause {
    fn classify(
        release_reported_failed: bool,
        cancelled: bool,
        exit_confirmed: bool,
        helper_lost: bool,
        helper_exited_ok: bool,
    ) -> Self {
        if release_reported_failed {
            Self::ReleaseReportedFailed
        } else if cancelled {
            Self::Cancelled
        } else if !exit_confirmed {
            Self::StillRunning
        } else if helper_lost {
            Self::HelperLost
        } else if !helper_exited_ok {
            Self::Failed
        } else {
            Self::None
        }
    }

    /// 对应的失败分类；`None` = 本次没有失败。
    #[must_use]
    const fn kind(self) -> StrokeFailureKind {
        match self {
            Self::ReleaseReportedFailed => StrokeFailureKind::ReleaseUnconfirmed,
            Self::Cancelled => StrokeFailureKind::Cancelled,
            Self::StillRunning | Self::HelperLost => StrokeFailureKind::HelperLost,
            Self::Failed => StrokeFailureKind::Failed,
            Self::None => StrokeFailureKind::Failed,
        }
    }

    /// 失败文本；没有失败时返回 `None`。
    fn into_failure(self, stderr: &str, obligation: &ReleaseObligation) -> Option<String> {
        match self {
            Self::None => None,
            Self::ReleaseReportedFailed => Some(format!(
                "mouse_release_failed: helper 未能确认释放（义务={}）: {stderr}",
                obligation.summary()
            )),
            Self::Cancelled => Some("stroke_cancelled: 受控原生输入已取消或超时".to_string()),
            Self::StillRunning => Some(
                "helper_lost: 受控原生输入 helper 无法确认静止（执行者可能仍在运行）".to_string(),
            ),
            Self::HelperLost => Some(format!(
                "helper_lost: 受控原生输入 helper 未正常结束且未给出原因（forced_kill={}）",
                stderr.is_empty()
            )),
            Self::Failed => Some(format!("受控原生输入失败: {stderr}")),
        }
    }
}

/// 独立（补发）释放：**最多一次**，等待上限由调用方按 `min(2 秒, 剩余收尾时间)` 给出。
///
/// 只释放**登记过**的按钮与按键（不是统一抬整个键盘，也不是只抬左键），不注入新输入；
/// 这里**不承诺释放成功**：失败会把"未确认释放"如实返回，交上层隔离。
fn native_emergency_release(
    obligation: &ReleaseObligation,
    wait: Duration,
    mock_scenario: Option<&str>,
    backend: InputBackend,
    reserved: Option<HelperCleanupReservation>,
) -> Result<(), String> {
    let request = serde_json::json!({ "mode": "release", "params": {} });
    // 清理运行只允许"转用上层预留"这一条容量来源（`CleanupRun` 分支里没有下一层清理预留）。
    let capacity = match reserved {
        Some(reserved) => NativeRunCapacity::CleanupRun(reserved),
        // 没有可用预留（例如预留已被归还）时，清理运行**不得**去竞争普通容量：
        // 宁可如实报"这次补发拿不到读取器容量"，也不把容量账目搅乱。
        None => return Err("cleanup_reader_capacity_unavailable: 独立释放没有可用的清理预留".to_string()),
    };
    run_native_helper(request, obligation, wait, &|| false, mock_scenario, backend, capacity)
        .map(|_| ())
        .map_err(|failure| failure.message)
}

/// 固定的受控原生 helper：C# 驱动与步骤引擎。
const NATIVE_INPUT_HELPER_CS: &str = r#"namespace CoolzhuNative {
 using System;
 using System.IO;
 using System.Threading;
 using System.Diagnostics;
 using System.Runtime.InteropServices;
 using System.Collections.Generic;
 public interface Driver {
  void Check();
  void MoveTo(int x, int y);
  // 读当前光标位置：相对移动必须先知道"从哪出发"，否则只能由主机在 helper 之外再探一次
  // （那正是我们要消除的未受监督路径）。
  void CursorPosition(out int x, out int y);
  void ButtonDown(string button);
  void ButtonUp(string button);
  void Wheel(int delta);
  void TypeUnit(ushort unit);
  void KeyDown(byte vk);
  void KeyUp(byte vk);
  void Wait(int ms);
  string[] ArmedButtons();
  int[] ArmedKeys();
  bool EverPressed();
  void ReleaseArmed();
  void ReleaseRequested(string[] buttons, int[] keys);
  void Dispose();
 }
 // 输入事实的接收端（CU-F01 v2）：阶段 + 是否移动过光标 + 注入量。
 public interface Progress {
  void Report(string phase, bool cursorMoved, int injectedSteps, bool completed, string[] armedButtons, int[] armedKeys, bool pressed, bool? released);
 }
 // 固定键 JSON + 回读校验；写不进去就删文件并报错（宁可不留事实）。
 public sealed class FileProgress : Progress {
  readonly string path; readonly string requestId;
  public FileProgress(string p, string r) { path = p; requestId = r == null ? "" : r; }
  public void Report(string phase, bool cursorMoved, int injectedSteps, bool completed, string[] armedButtons, int[] armedKeys, bool pressed, bool? released) {
   if (String.IsNullOrEmpty(path)) return;
   var buttons = armedButtons == null ? new string[0] : armedButtons;
   var keys = armedKeys == null ? new int[0] : armedKeys;
   var quoted = new List<string>();
   foreach (var button in buttons) quoted.Add("\"" + button + "\"");
   var keyText = new List<string>();
   foreach (var key in keys) keyText.Add(key.ToString());
   string json = "{\"protocol\":2,\"request_id\":\"" + requestId + "\",\"phase\":\"" + phase + "\""
    + ",\"cursor_moved\":" + (cursorMoved ? "true" : "false")
    + ",\"injected_steps\":" + injectedSteps + ",\"completed\":" + (completed ? "true" : "false")
    + ",\"armed_buttons\":[" + String.Join(",", quoted.ToArray()) + "]"
    + ",\"armed_keys\":[" + String.Join(",", keyText.ToArray()) + "]"
    + ",\"pressed\":" + (pressed ? "true" : "false")
    + ",\"released\":" + (released.HasValue ? (released.Value ? "true" : "false") : "null") + "}";
   try {
    File.WriteAllText(path, json);
    if (File.ReadAllText(path) != json) throw new Exception("progress write was not readable back");
   } catch {
    try { if (File.Exists(path)) File.Delete(path); } catch {}
    throw new Exception("progress_write_failed: 输入事实无法可靠落盘");
   }
  }
 }
 // 与受控笔画的 IdentityCheck 同一套判据与同一份诊断措辞。
 public static class NativeIdentity {
  public static string Difference(long expectedHandle, long actualHandle, uint expectedPid, uint actualPid, int[] expectedRect, int[] actualRect, uint expectedDpi, uint actualDpi, bool pidAvailable, bool rectAvailable) {
   var problems = new List<string>();
   if (expectedHandle != actualHandle) problems.Add("foreground expected=" + expectedHandle + " actual=" + actualHandle);
   if (!pidAvailable || expectedPid != actualPid) problems.Add("pid expected=" + expectedPid + " actual=" + actualPid + " available=" + pidAvailable);
   if (expectedDpi != actualDpi) problems.Add("dpi expected=" + expectedDpi + " actual=" + actualDpi);
   bool same = rectAvailable && expectedRect != null && actualRect != null && expectedRect.Length == 4 && actualRect.Length == 4;
   for (int i = 0; same && i < 4; i++) if (expectedRect[i] != actualRect[i]) same = false;
   if (!same) problems.Add("rect expected=[" + String.Join(",", Array.ConvertAll(expectedRect, v => v.ToString())) + "] actual=[" + String.Join(",", Array.ConvertAll(actualRect, v => v.ToString())) + "] available=" + rectAvailable);
   return String.Join("; ", problems.ToArray());
  }
 }
 public static class Buttons {
  public static string Name(string button) {
   if (button == "left" || button == "right") return button;
   throw new Exception("invalid_native_button: " + button);
  }
 }
 // 公共基类：取消/身份/自守望检查、义务登记、收尾释放。
 public abstract class Checker : Driver, IDisposable {
  [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
  [DllImport("user32.dll")] static extern int GetSystemMetrics(int key);
  readonly IntPtr handle; readonly uint pid, dpi; readonly int[] rect; readonly string cancel; readonly int watchdogMs; readonly bool identity;
  readonly Stopwatch watch = Stopwatch.StartNew();
  readonly List<string> armedButtons = new List<string>();
  readonly List<int> armedKeys = new List<int>();
  bool pressed;
  protected Checker(bool identity, long handle, uint pid, int[] rect, uint dpi, string cancel, int watchdogMs) {
   this.identity = identity; this.handle = new IntPtr(handle); this.pid = pid; this.rect = rect; this.dpi = dpi;
   this.cancel = cancel; this.watchdogMs = watchdogMs;
   if (identity && SetThreadDpiAwarenessContext(new IntPtr(-4)) == IntPtr.Zero) throw new Exception("dpi_context_failed: 无法设置物理像素坐标上下文");
  }
  // 取消一律用既有失败码 stroke_cancelled（不新造分类）；身份不匹配用 stale_observation。
  public virtual void Check() {
   if (!String.IsNullOrEmpty(cancel) && File.Exists(cancel)) throw new Exception("stroke_cancelled: 受控原生输入已被取消");
   if ((GetAsyncKeyState(0x1B) & 0x8000) != 0) throw new Exception("stroke_cancelled: Escape 已按下");
   if (watchdogMs > 0 && watch.ElapsedMilliseconds > watchdogMs) throw new Exception("stroke_cancelled: helper 自守望超时");
   if (!identity) return;
   uint p; RECT r; var foreground = GetForegroundWindow();
   bool pidAvailable = GetWindowThreadProcessId(handle, out p) != 0;
   bool rectAvailable = GetWindowRect(handle, out r);
   uint actualDpi = GetDpiForWindow(handle);
   string mismatch = NativeIdentity.Difference(handle.ToInt64(), foreground.ToInt64(), pid, p, rect,
    new int[] { r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top }, dpi, actualDpi, pidAvailable, rectAvailable);
   if (mismatch.Length > 0) throw new Exception("stale_observation: " + mismatch);
  }
  public void Wait(int ms) { for (int left = ms; left > 0;) { Check(); int slice = Math.Min(left, 10); Thread.Sleep(slice); left -= slice; } Check(); }
  public string[] ArmedButtons() { return armedButtons.ToArray(); }
  public int[] ArmedKeys() { return armedKeys.ToArray(); }
  public bool EverPressed() { return pressed; }
  protected void Armed(string button) { pressed = true; if (!armedButtons.Contains(button)) armedButtons.Add(button); }
  protected void Unarmed(string button) { armedButtons.Remove(button); }
  protected void ArmedKey(int vk) { pressed = true; if (!armedKeys.Contains(vk)) armedKeys.Add(vk); }
  protected void UnarmedKey(int vk) { armedKeys.Remove(vk); }
  // 目标点越界一律在注入之前拒绝，绝不"点了再说"。
  protected void AssertOnDesktop(int x, int y) {
   int left = GetSystemMetrics(76), top = GetSystemMetrics(77), width = GetSystemMetrics(78), height = GetSystemMetrics(79);
   if (x < left || y < top || x >= left + width || y >= top + height) throw new Exception("cursor_move_failed: 目标点不在桌面可见范围内");
  }
  public abstract void MoveTo(int x, int y);
  public abstract void CursorPosition(out int x, out int y);
  public abstract void ButtonDown(string button);
  public abstract void ButtonUp(string button);
  public abstract void Wheel(int delta);
  public abstract void TypeUnit(ushort unit);
  public abstract void KeyDown(byte vk);
  public abstract void KeyUp(byte vk);
  protected abstract void RawButtonUp(string button);
  protected abstract void RawKeyUp(byte vk);
  // 收尾释放：不做取消/身份复核（取消之后仍要生效），只释放实际按下过的东西。
  public void ReleaseArmed() {
   var failed = new List<string>();
   foreach (string button in armedButtons.ToArray()) {
    bool ok = false;
    for (int i = 0; i < 3 && !ok; i++) { try { RawButtonUp(button); Unarmed(button); ok = true; } catch { Thread.Sleep(10); } }
    if (!ok) failed.Add(button);
   }
   foreach (int vk in armedKeys.ToArray()) {
    bool ok = false;
    for (int i = 0; i < 3 && !ok; i++) { try { RawKeyUp((byte)vk); UnarmedKey(vk); ok = true; } catch { Thread.Sleep(10); } }
    if (!ok) failed.Add("vk:" + vk);
   }
   if (failed.Count > 0) throw new Exception("释放失败: " + String.Join(",", failed.ToArray()));
  }
  // 独立释放通道：只释放请求里登记的东西，最多 3 次；失败必须如实报出。
  public virtual void ReleaseRequested(string[] buttons, int[] keys) {
   var failed = new List<string>();
   if (buttons != null) foreach (string button in buttons) Release("button " + button, failed, () => RawButtonUp(Buttons.Name(button)));
   if (keys != null) foreach (int vk in keys) Release("vk " + vk, failed, () => RawKeyUp((byte)vk));
   if (failed.Count > 0) throw new Exception("input_release_unconfirmed: 独立释放未能确认: " + String.Join(",", failed.ToArray()));
  }
  static void Release(string label, List<string> failed, Action action) {
   bool ok = false;
   for (int i = 0; i < 3 && !ok; i++) { try { action(); ok = true; } catch { Thread.Sleep(10); } }
   if (!ok) failed.Add(label);
  }
  public virtual void Dispose() {}
 }
 // SendInput 后端：与既有 sendinput_* 同一套 user32 调用。
 public sealed class SendInputDriver : Checker {
  [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint data, flags, time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Explicit)] struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mouse; [FieldOffset(0)] public KEYBDINPUT key; }
  [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public INPUTUNION u; }
  [StructLayout(LayoutKind.Sequential)] struct CURSORPOINT { public int x, y; }
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern bool GetCursorPos(out CURSORPOINT point);
  [DllImport("user32.dll", SetLastError = true)] static extern uint SendInput(uint n, INPUT[] inputs, int size);
  public SendInputDriver(bool identity, long handle, uint pid, int[] rect, uint dpi, string cancel, int watchdogMs)
   : base(identity, handle, pid, rect, dpi, cancel, watchdogMs) {}
  static void Send(INPUT[] inputs) {
   uint sent = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));
   if (sent != (uint)inputs.Length) throw new Exception("send_input_failed: SendInput sent=" + sent);
  }
  static INPUT Mouse(uint flags, uint data) { var input = new INPUT(); input.type = 0; input.u.mouse.flags = flags; input.u.mouse.data = data; return input; }
  static INPUT Keyboard(uint flags, ushort vk, ushort scan) { var input = new INPUT(); input.type = 1; input.u.key.flags = flags; input.u.key.vk = vk; input.u.key.scan = scan; return input; }
  static uint MouseFlag(string button, bool down) {
   bool left = Buttons.Name(button) == "left";
   if (left) return down ? 0x0002u : 0x0004u;
   return down ? 0x0008u : 0x0010u;
  }
  public override void MoveTo(int x, int y) { Check(); AssertOnDesktop(x, y); if (!SetCursorPos(x, y)) throw new Exception("cursor_move_failed: SetCursorPos 返回 false"); }
  public override void CursorPosition(out int x, out int y) { Check(); CURSORPOINT point; if (!GetCursorPos(out point)) throw new Exception("cursor_position_failed: GetCursorPos 返回 false"); x = point.x; y = point.y; }
  public override void ButtonDown(string button) { Check(); Send(new INPUT[] { Mouse(MouseFlag(button, true), 0) }); Armed(Buttons.Name(button)); }
  public override void ButtonUp(string button) { Check(); Send(new INPUT[] { Mouse(MouseFlag(button, false), 0) }); Unarmed(Buttons.Name(button)); }
  protected override void RawButtonUp(string button) { Send(new INPUT[] { Mouse(MouseFlag(button, false), 0) }); }
  protected override void RawKeyUp(byte vk) { Send(new INPUT[] { Keyboard(0x0002, 0, vk) }); }
  public override void Wheel(int delta) { Check(); Send(new INPUT[] { Mouse(0x0800, (uint)delta) }); }
  // 文本：一个 UTF-16 单元 = 一次 SendInput（down+up 同批），不可被取消拆开。
  public override void TypeUnit(ushort unit) { Check(); Send(new INPUT[] { Keyboard(0x0004, 0, unit), Keyboard(0x0006, 0, unit) }); }
  public override void KeyDown(byte vk) { Check(); Send(new INPUT[] { Keyboard(0, 0, vk) }); ArmedKey(vk); }
  public override void KeyUp(byte vk) { Check(); Send(new INPUT[] { Keyboard(0x0002, 0, vk) }); UnarmedKey(vk); }
 }
 // Interception 后端：状态码/扩展键判定与既有 interception_* 一致。
 public sealed class InterceptionDriver : Checker {
  [StructLayout(LayoutKind.Sequential)] public struct MouseStroke { public ushort state, flags; public short rolling; public int x, y; public uint information; }
  [StructLayout(LayoutKind.Sequential)] public struct KeyStroke { public ushort code, state; public uint information; }
  [DllImport("interception.dll", CallingConvention = CallingConvention.Cdecl)] static extern IntPtr interception_create_context();
  [DllImport("interception.dll", CallingConvention = CallingConvention.Cdecl)] static extern void interception_destroy_context(IntPtr context);
  [DllImport("interception.dll", CallingConvention = CallingConvention.Cdecl)] static extern int interception_send(IntPtr context, int device, MouseStroke[] stroke, uint nstroke);
  [DllImport("interception.dll", CallingConvention = CallingConvention.Cdecl)] static extern int interception_send(IntPtr context, int device, KeyStroke[] stroke, uint nstroke);
  [DllImport("user32.dll")] static extern ushort MapVirtualKeyW(uint uCode, uint uMapType);
  [DllImport("user32.dll")] static extern short VkKeyScanW(char ch);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  readonly IntPtr context; readonly int mouseDevice, keyDevice;
  public InterceptionDriver(uint mouseDevice, uint keyDevice, bool identity, long handle, uint pid, int[] rect, uint dpi, string cancel, int watchdogMs)
   : base(identity, handle, pid, rect, dpi, cancel, watchdogMs) {
   this.mouseDevice = (int)mouseDevice; this.keyDevice = (int)keyDevice;
   context = interception_create_context();
   if (context == IntPtr.Zero) throw new Exception("interception_unavailable: 无法创建 Interception 上下文");
  }
  void SendMouse(ushort state, short rolling) {
   var stroke = new MouseStroke(); stroke.state = state; stroke.rolling = rolling;
   if (interception_send(context, mouseDevice, new MouseStroke[] { stroke }, 1) < 1) throw new Exception("send_input_failed: Interception 鼠标注入失败");
  }
  void SendKey(byte vk, bool up) {
   ushort scan = MapVirtualKeyW(vk, 0);
   if (scan == 0) throw new Exception("send_input_failed: 无法映射虚拟键 " + vk);
   var stroke = new KeyStroke(); stroke.code = scan;
   int state = up ? 0x01 : 0x00;
   if (IsExtended(vk)) state |= 0x02;
   stroke.state = (ushort)state;
   if (interception_send(context, keyDevice, new KeyStroke[] { stroke }, 1) < 1) throw new Exception("send_input_failed: Interception 键盘注入失败");
  }
  static bool IsExtended(int vk) {
   switch (vk) {
    case 0x21: case 0x22: case 0x23: case 0x24: case 0x25: case 0x26: case 0x27: case 0x28:
    case 0x2D: case 0x2E: case 0x5B: case 0x5C: return true;
    default: return false;
   }
  }
  [StructLayout(LayoutKind.Sequential)] struct CURSORPOINT { public int x, y; }
  [DllImport("user32.dll")] static extern bool GetCursorPos(out CURSORPOINT point);
  static ushort ButtonState(string button, bool down) {
   bool left = Buttons.Name(button) == "left";
   if (left) return (ushort)(down ? 1 : 2);
   return (ushort)(down ? 4 : 8);
  }
  public override void MoveTo(int x, int y) { Check(); AssertOnDesktop(x, y); if (!SetCursorPos(x, y)) throw new Exception("cursor_move_failed: SetCursorPos 返回 false"); }
  public override void CursorPosition(out int x, out int y) { Check(); CURSORPOINT point; if (!GetCursorPos(out point)) throw new Exception("cursor_position_failed: GetCursorPos 返回 false"); x = point.x; y = point.y; }
  public override void ButtonDown(string button) { Check(); SendMouse(ButtonState(button, true), 0); Armed(Buttons.Name(button)); }
  public override void ButtonUp(string button) { Check(); SendMouse(ButtonState(button, false), 0); Unarmed(Buttons.Name(button)); }
  protected override void RawButtonUp(string button) { SendMouse(ButtonState(button, false), 0); }
  protected override void RawKeyUp(byte vk) { SendKey(vk, true); }
  public override void Wheel(int delta) { Check(); SendMouse(1024, (short)delta); }
  // 与 interception_type_text 相同：逐字符按下/抬起修饰键，并登记其义务。
  public override void TypeUnit(ushort unit) {
   Check();
   short info = VkKeyScanW((char)unit);
   if (info == -1) throw new Exception("unsupported_character: " + ((char)unit));
   byte vk = (byte)(info & 0xFF);
   int shiftState = (info >> 8) & 0xFF;
   var modifiers = new List<byte>();
   if ((shiftState & 1) != 0) modifiers.Add(0x10);
   if ((shiftState & 2) != 0) modifiers.Add(0x11);
   if ((shiftState & 4) != 0) modifiers.Add(0x12);
   foreach (byte modifier in modifiers) { SendKey(modifier, false); ArmedKey(modifier); Thread.Sleep(8); }
   SendKey(vk, false); Thread.Sleep(18); SendKey(vk, true);
   for (int i = modifiers.Count - 1; i >= 0; i--) { Thread.Sleep(8); SendKey(modifiers[i], true); UnarmedKey(modifiers[i]); }
  }
  public override void KeyDown(byte vk) { Check(); SendKey(vk, false); ArmedKey(vk); }
  public override void KeyUp(byte vk) { Check(); SendKey(vk, true); UnarmedKey(vk); }
  public override void Dispose() { if (context != IntPtr.Zero) interception_destroy_context(context); }
 }
 public static class Drivers {
  public static Driver Create(string backend, string mockScenario, int watchdogMs, string cancelFile,
   uint mouseDevice, uint keyDevice, bool identity, long handle, uint pid, int[] rect, uint dpi) {
   if (!String.IsNullOrEmpty(mockScenario)) return new MockChecks.Mock(mockScenario, cancelFile, watchdogMs);
   if (backend == "sendinput") return new SendInputDriver(identity, handle, pid, rect, dpi, cancelFile, watchdogMs);
   if (backend == "interception") return new InterceptionDriver(mouseDevice, keyDevice, identity, handle, pid, rect, dpi, cancelFile, watchdogMs);
   throw new Exception("invalid_native_backend: " + backend);
  }
 }
 public static class Engine {
  sealed class Step {
   public readonly string Button; public readonly bool Down; public readonly int WaitMs;
   public Step(string button, bool down, int waitMs) { Button = button; Down = down; WaitMs = waitMs; }
  }
  // 步骤序列与既有 sendinput_mouse_button_sequence 一致（勿改）。
  static List<Step> ClickSequence(string action) {
   var steps = new List<Step>();
   if (action == "left" || action == "double") {
    if (action == "double") { steps.Add(new Step("left", true, 35)); steps.Add(new Step("left", false, 90)); }
    steps.Add(new Step("left", true, action == "double" ? 35 : 45));
    steps.Add(new Step("left", false, 120));
    return steps;
   }
   if (action == "right") { steps.Add(new Step("right", true, 45)); steps.Add(new Step("right", false, 120)); return steps; }
   if (action == "chord") {
    steps.Add(new Step("left", true, 25)); steps.Add(new Step("right", true, 70));
    steps.Add(new Step("right", false, 25)); steps.Add(new Step("left", false, 120));
    return steps;
   }
   throw new Exception("invalid_native_click_action: " + action);
  }
  static byte SingleKey(int[] keys) {
   if (keys == null || keys.Length != 1) throw new Exception("invalid_native_key: 需要且只需要一个虚拟键");
   if (keys[0] <= 0 || keys[0] > 255) throw new Exception("invalid_native_key: 虚拟键越界");
   return (byte)keys[0];
  }
  static List<byte> ComboKeys(int[] keys) {
   if (keys == null || keys.Length < 1 || keys.Length > 8) throw new Exception("invalid_native_combo: 组合键需要 1–8 个虚拟键");
   var combo = new List<byte>();
   foreach (int key in keys) {
    if (key <= 0 || key > 255) throw new Exception("invalid_native_combo: 虚拟键越界");
    combo.Add((byte)key);
   }
   return combo;
  }
  // 输入过程中的汇报是硬性的：写不下去就中止，避免留下可能过期的记录。
  static void Report(Progress progress, string phase, bool cursorMoved, int steps, bool completed, string[] buttons, int[] keys, bool pressed, bool? released) {
   if (progress == null) return;
   progress.Report(phase, cursorMoved, steps, completed, buttons, keys, pressed, released);
  }
  // 收尾汇报：事实完整度问题不得覆盖真正的失败分类。
  static void ReportSoft(Progress progress, string phase, bool cursorMoved, int steps, bool completed, string[] buttons, int[] keys, bool pressed, bool? released) {
   if (progress == null) return;
   try { progress.Report(phase, cursorMoved, steps, completed, buttons, keys, pressed, released); } catch {}
  }
  public static void Run(Driver driver, Progress progress, string mode, string action, int x, int y, string text, int delta, int[] keys, int holdMs) {
   if (driver == null) throw new Exception("invalid_native_driver");
   int steps = 0; bool completed = false; bool holdsByDesign = false; bool releaseFailed = false; bool cursorMoved = false; Exception failure = null;
   Action<bool, bool?> reportStep = (done, released) => Report(progress, "in_flight", cursorMoved, steps, done, driver.ArmedButtons(), driver.ArmedKeys(), driver.EverPressed(), released);
   try {
    // 硬性起点事实：写不下去就不开始输入。
    Report(progress, "pre_input", false, 0, false, new string[0], new int[0], false, null);
    switch (mode == null ? "" : mode) {
     case "click": {
      var sequence = ClickSequence(action);
      driver.Check(); driver.MoveTo(x, y); cursorMoved = true;
      // 光标已移动：立刻落盘，否则"移动过但零注入"与"什么都没做"无法区分。
      reportStep(false, null);
      driver.Wait(160);
      foreach (Step step in sequence) {
       driver.Check();
       if (step.Down) driver.ButtonDown(step.Button); else driver.ButtonUp(step.Button);
       steps++; reportStep(false, null);
       driver.Wait(step.WaitMs);
      }
      completed = true;
      break;
     }
     case "text": {
      if (String.IsNullOrEmpty(text) || text.Length > 4000) throw new Exception("invalid_native_text: 文本为空或超过 4000 个 UTF-16 单元");
      for (int i = 0; i < text.Length; i++) {
       driver.Check(); driver.TypeUnit((ushort)text[i]);
       steps++; reportStep(false, null);
       driver.Wait(2);
      }
      completed = true;
      break;
     }
     case "scroll": {
      if (delta == 0 || Math.Abs((long)delta) > 600) throw new Exception("invalid_native_scroll: 滚动量必须非零且不超过 600");
      driver.Check(); driver.Wheel(delta);
      steps++; reportStep(true, null);
      completed = true;
      break;
     }
     case "key": {
      byte vk = SingleKey(keys);
      if (holdMs <= 0 || holdMs > 2000) throw new Exception("invalid_native_hold: 按住时长必须在 1–2000ms");
      driver.Check(); driver.KeyDown(vk);
      steps++; reportStep(false, null);
      driver.Wait(holdMs);
      driver.Check(); driver.KeyUp(vk);
      steps++; reportStep(false, null);
      completed = true;
      break;
     }
     case "combo": {
      var combo = ComboKeys(keys);
      foreach (byte vk in combo) { driver.Check(); driver.KeyDown(vk); steps++; reportStep(false, null); driver.Wait(18); }
      driver.Wait(80);
      combo.Reverse();
      foreach (byte vk in combo) { driver.Check(); driver.KeyUp(vk); steps++; reportStep(false, null); driver.Wait(18); }
      completed = true;
      break;
     }
     case "move":
     case "move_relative": {
      // 移动：只改光标位置，**永不按下**任何按钮/键 ⇒ 本模式没有释放义务。
      // 相对移动必须在**同一段受监督运行里**先读位置：主机在外面再探一次会引入
      // 一条不受监督的路径，也正是本次收口要消除的东西。
      driver.Check();
      int targetX = x; int targetY = y;
      if (mode == "move_relative") { int cx; int cy; driver.CursorPosition(out cx, out cy); targetX = cx + x; targetY = cy + y; }
      driver.MoveTo(targetX, targetY); cursorMoved = true;
      // 光标移动过就必须立即落盘（与 click 同口径）：否则"移动过"与"什么都没做"无法区分。
      // 注意 `steps` 不加：它计的是**按钮/键/滚动**这类输入事件，移动不是（与 click 的 MoveTo 同口径）。
      reportStep(true, null);
      completed = true;
      break;
     }
     case "down":
     case "up": {
      string button = Buttons.Name(action);
      driver.Check();
      if (mode == "down") driver.ButtonDown(button); else driver.ButtonUp(button);
      steps++; reportStep(true, null);
      completed = true;
      // 明确"按住"就是本次运行的意图：没有失败就不在收尾里把它抬起来。
      holdsByDesign = mode == "down";
      break;
     }
     default: throw new Exception("unsupported_native_mode: " + mode);
    }
   } catch (Exception e) { failure = e; }
   finally {
    if (!holdsByDesign) {
     try { driver.ReleaseArmed(); } catch (Exception e) { releaseFailed = true; failure = new Exception("input_release_unconfirmed: 收尾释放未确认: " + e.Message, failure); }
    }
    try { driver.Dispose(); } catch (Exception e) { failure = new Exception("driver_dispose_failed: " + e.Message, failure); }
    bool? released = holdsByDesign ? (bool?)null : (bool?)(!releaseFailed && driver.ArmedButtons().Length == 0 && driver.ArmedKeys().Length == 0);
    // 收尾之后的最终事实：无论是否按下过都要写，否则"从未按下"无法被证明。
    ReportSoft(progress, "final", cursorMoved, steps, completed, driver.ArmedButtons(), driver.ArmedKeys(), driver.EverPressed(), released);
   }
   if (failure != null) throw failure;
  }
 }
 public static class Entry {
  // 独立释放通道：只做释放，不注入新输入，不校验身份、不启用自守望。
  public static void Release(string backend, string mockScenario, uint mouseDevice, uint keyDevice, string[] buttons, int[] keys) {
   Driver driver = Drivers.Create(backend, mockScenario, 0, "", mouseDevice, keyDevice, false, 0, 0, null, 0);
   try { driver.ReleaseRequested(buttons, keys); } finally { try { driver.Dispose(); } catch {} }
  }
 }
 public static class MockChecks {
  // 纯内存驱动：不碰 user32、不注入输入；用于真实 helper 进程下的受控验证。
  public sealed class Mock : Checker {
   readonly string scenario; int units;
   public Mock(string scenario, string cancelFile, int watchdogMs) : base(false, 0, 0, null, 0, cancelFile, watchdogMs) { this.scenario = scenario; }
   // slow_steps 模拟"阻塞且不理会取消"的执行者：只有主机强杀才能停下它。
   public override void Check() { if (scenario == "slow_steps") return; base.Check(); }
   int cursorX; int cursorY;
   public override void MoveTo(int x, int y) {
    if (scenario == "fail_before_press") throw new Exception("send_input_failed: mock 在按下之前失败");
    if (scenario == "fail_move") throw new Exception("cursor_move_failed: mock 移动失败");
    cursorX = x; cursorY = y;
   }
   public override void CursorPosition(out int x, out int y) { x = cursorX; y = cursorY; }
   public override void ButtonDown(string button) {
    if (scenario == "fail_before_press") throw new Exception("send_input_failed: mock 在按下之前失败");
    Armed(Buttons.Name(button));
   }
   public override void ButtonUp(string button) {
    if (scenario == "fail_after_press" || scenario == "fail_release") throw new Exception("send_input_failed: mock 在按下之后失败");
    Unarmed(Buttons.Name(button));
   }
   protected override void RawButtonUp(string button) {
    if (scenario == "fail_release") throw new Exception("release failed");
    Unarmed(Buttons.Name(button));
   }
   protected override void RawKeyUp(byte vk) {
    if (scenario == "fail_release") throw new Exception("release failed");
    UnarmedKey(vk);
   }
   public override void Wheel(int delta) { if (scenario == "fail_wheel") throw new Exception("send_input_failed: mock 滚轮失败"); }
   public override void TypeUnit(ushort unit) {
    units++;
    if (scenario == "partial_text" && units > 2) throw new Exception("send_input_failed: mock 文本中途失败");
    if (scenario == "slow_steps") Thread.Sleep(100);
   }
   public override void KeyDown(byte vk) {
    if (scenario == "combo_mid_fail" && ArmedKeys().Length >= 1) throw new Exception("send_input_failed: mock 组合键中途失败");
    if (scenario == "slow_steps") Thread.Sleep(100);
    ArmedKey(vk);
   }
   public override void KeyUp(byte vk) { UnarmedKey(vk); }
   public override void ReleaseRequested(string[] buttons, int[] keys) {
    if (scenario == "release_fails") throw new Exception("input_release_unconfirmed: mock 独立释放失败");
   }
  }
 }
}
"#;

/// 固定的受控原生 helper：入口 PowerShell（只做参数搬运，不做任何脚本执行）。
const NATIVE_INPUT_HELPER_ENTRY: &str = r#"$r = [Console]::In.ReadToEnd() | ConvertFrom-Json
$sp=([string]$r.mock_scenario).Split('|')
$p = $r.params
$ob = $r.obligation
$mouseDevice = if ($null -ne $r.mouse_device_id) { [uint32]$r.mouse_device_id } else { 11 }
$keyDevice = if ($null -ne $r.keyboard_device_id) { [uint32]$r.keyboard_device_id } else { 1 }
if ($r.mode -eq 'release') {
 $buttons = if ($null -ne $ob.buttons) { [string[]]$ob.buttons } else { @() }
 $keys = if ($null -ne $ob.keys) { [int[]]$ob.keys } else { @() }
 [CoolzhuNative.Entry]::Release([string]$r.backend, $sp[0], $mouseDevice, $keyDevice, $buttons, $keys)
 'released'
 exit
}
$identity = $r.identity
$hasIdentity = $null -ne $identity
$handle = [long]0
$targetProcess = [uint32]0
$rect = $null
$targetDpi = [uint32]0
if ($hasIdentity) {
 $handle = [long]$identity.handle
 $targetProcess = [uint32]$identity.process_id
 $rect = [int[]]$identity.rect
 $targetDpi = [uint32]$identity.dpi
}
$driver = [CoolzhuNative.Drivers]::Create(
 [string]$r.backend,
 $sp[0],
 [int]$r.self_watchdog_ms,
 [string]$r.cancel_file,
 $mouseDevice,
 $keyDevice,
 $hasIdentity,
 $handle,
 $targetProcess,
 $rect,
 $targetDpi)
if($sp[2]){Start-Sleep -Milli ([int]$sp[2])}
$progress = New-Object CoolzhuNative.FileProgress ([string]$r.progress_file, [string]$r.request_id)
# 8.3c-A Phase 1：两相生命周期（opt-in；$r.two_phase_helper 默认不存在 ⇒ 绝不执行）。
if($r.two_phase_helper){$q=[string]$r.ready_file;$m=[string]$r.permit_file;$n=[string]$r.request_id
if($q){[IO.File]::WriteAllText($q,'{"type":"ready","helper_protocol_version":1,"nonce":"'+$n+'","pid":'+$PID+',"timestamp_unix_ms":'+[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()+'}')
$w=1;while($w){if(Test-Path -LiteralPath ([string]$r.cancel_file)){$w=0}elseif($m -and (Test-Path -LiteralPath $m)){if(([IO.File]::ReadAllText($m)) -like ('*"'+$n+'"*')){$w=0}else{exit 4}}else{Start-Sleep -Milli 20}}}}
if($sp[1]){$null=ni $sp[1]}
$keys = if ($null -ne $p.keys) { [int[]]$p.keys } else { $null }
[CoolzhuNative.Engine]::Run(
 $driver,
 $progress,
 [string]$r.mode,
 [string]$p.action,
 [int]$p.x,
 [int]$p.y,
 [string]$p.text,
 [int]$p.delta,
 $keys,
 [int]$p.hold_ms)
'native-input:ok'
"#;

/// **测试专用**：进程级读取器容量闸门。
///
/// 读取器容量是**进程级**的生产语义（活动读取器的预留与已残留读取器共同计入
/// `MAX_RETAINED_PIPE_READERS` 个单位，一个 helper 占两个）。因此测试进程里并行运行的
/// 用例必须自己排队：否则某个用例会因为**别的用例**占满容量而被拒绝，而那个失败与它
/// 要验证的行为毫无关系。生产构建里不存在这段代码（`#[cfg(test)]`）。
#[cfg(test)]
static PIPE_READER_TEST_UNITS: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

/// 闸门凭证：Drop 时归还；同一线程内可重入（helper 运行里还会嵌套一次"独立释放"运行）。
#[cfg(test)]
pub(crate) struct PipeReaderTestSlot {
    units: usize,
}

#[cfg(test)]
thread_local! {
    /// 本线程已经持有的闸门单位数（重入标记：同线程的嵌套 helper 运行不再二次占位）。
    static PIPE_READER_TEST_HELD: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn test_pipe_reader_capacity_slot(units: usize) -> PipeReaderTestSlot {
    // 上限 8 个单位：测试用例合计最多占 6 个，留 2 个给"用例自己留下的、还没被回收的
    // 残留读取器"（**残留现在也占容量义务**，见 `windows-process-guard` 的容量模型）。
    const TEST_UNITS: usize = 6;
    let reentrant = PIPE_READER_TEST_HELD.with(|held| {
        if held.get() > 0 {
            true
        } else {
            held.set(units);
            false
        }
    });
    if reentrant {
        return PipeReaderTestSlot { units: 0 };
    }
    loop {
        // 故障锁生效意味着**容量契约已经被破坏**（有读取器没有预留就要求登记）。
        // 这不是"等一等就好"的状态，因此直接如实失败，绝不用等待把它藏起来。
        if let Some(fault) = windows_process_guard::pipe_supervision_fault() {
            if fault.admission_locked {
                panic!("读取器容量契约被破坏（接纳已锁住）：{fault}");
            }
        }
        let mut in_use = PIPE_READER_TEST_UNITS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // 两道门槛都要过：① 用例之间的测试预算；② **真实账目**也必须放得下本次运行
        // （登记表里的残留读取器同样占容量义务）。少了第二道，失败会落在与被测行为无关
        // 的地方——那正是"测试不掩盖产品缺陷"要求避免的形态。
        let fits_budget = in_use.saturating_add(units) <= TEST_UNITS;
        let fits_ledger = windows_process_guard::pipe_reader_capacity().available() >= units;
        if fits_budget && fits_ledger {
            *in_use += units;
            return PipeReaderTestSlot { units };
        }
        drop(in_use);
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
impl Drop for PipeReaderTestSlot {
    fn drop(&mut self) {
        PIPE_READER_TEST_HELD.with(|held| {
            if held.get() == self.units {
                held.set(0);
            }
        });
        if self.units == 0 {
            return;
        }
        let mut in_use = PIPE_READER_TEST_UNITS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *in_use = in_use.saturating_sub(self.units);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **CU-01**：粗粒度输入状态的映射表（最保守方向）。
    #[test]
    fn input_status_maps_delivery_facts_conservatively() {
        let cases = [
            (
                DeliveryFacts::unknown(),
                InputStatus::Unknown,
                "读不懂/未封闭一律 unknown",
            ),
            (
                DeliveryFacts::proven_not_sent(false),
                InputStatus::None,
                "结构事实：未发送",
            ),
            (
                DeliveryFacts::proven_not_sent(true),
                InputStatus::None,
                "路径动作明确 0 点且未走完",
            ),
            (
                DeliveryFacts {
                    input_delivery: InputDelivery::Sent,
                    partial: Some(false),
                    path_completed: Some(true),
                    confirmed_point_count: Some(12),
                },
                InputStatus::Complete,
                "确认发送且明确不是部分",
            ),
            (
                DeliveryFacts {
                    input_delivery: InputDelivery::MayHaveBeenSent,
                    partial: Some(true),
                    path_completed: Some(false),
                    confirmed_point_count: Some(3),
                },
                InputStatus::Partial,
                "确认注入过部分内容",
            ),
            (
                // 自相矛盾记录（发送 + 部分）：宁可 partial，**不许** complete。
                DeliveryFacts {
                    input_delivery: InputDelivery::Sent,
                    partial: Some(true),
                    path_completed: None,
                    confirmed_point_count: None,
                },
                InputStatus::Partial,
                "矛盾记录不得升格成完成",
            ),
            (
                // 光标动过但没有输入事件被确认，且记录已封闭。
                DeliveryFacts {
                    input_delivery: InputDelivery::MayHaveBeenSent,
                    partial: Some(false),
                    path_completed: Some(false),
                    confirmed_point_count: Some(0),
                },
                InputStatus::None,
                "光标动过但零输入事件：粗粒度属于 none（delivery 仍是'可能已发出'，两者不同粒度）",
            ),
            (
                // 发送了但没有 partial 结论：缺字段 ⇒ 不得声称完成。
                DeliveryFacts {
                    input_delivery: InputDelivery::Sent,
                    partial: None,
                    path_completed: None,
                    confirmed_point_count: None,
                },
                InputStatus::Unknown,
                "缺 partial 结论不得当完成",
            ),
        ];
        for (facts, expected, reason) in cases {
            assert_eq!(derive_input_status(&facts), expected, "{reason}: {facts:?}");
        }
    }

    /// **CU-01 不变式**：只有"确认发送且明确不是部分"才允许声称完成；
    /// partial 与 unknown 一律禁止自动重放。
    #[test]
    fn input_status_never_claims_completion_without_proof() {
        let all = [
            DeliveryFacts::unknown(),
            DeliveryFacts::proven_not_sent(false),
            DeliveryFacts::proven_not_sent(true),
            DeliveryFacts {
                input_delivery: InputDelivery::Sent,
                partial: Some(true),
                path_completed: Some(true),
                confirmed_point_count: Some(5),
            },
            DeliveryFacts {
                input_delivery: InputDelivery::MayHaveBeenSent,
                partial: Some(true),
                path_completed: None,
                confirmed_point_count: None,
            },
        ];
        for facts in all {
            let status = derive_input_status(&facts);
            assert!(
                !status.may_claim_complete(),
                "这些事实不得声称完成：{facts:?} -> {status:?}"
            );
            assert!(
                status.forbids_automatic_replay() || status == InputStatus::None,
                "非完成态只能落 None 或禁止自动重放：{facts:?} -> {status:?}"
            );
        }
        // 唯一的完成组合。
        let complete = DeliveryFacts {
            input_delivery: InputDelivery::Sent,
            partial: Some(false),
            path_completed: Some(true),
            confirmed_point_count: Some(9),
        };
        assert_eq!(derive_input_status(&complete), InputStatus::Complete);
        assert!(derive_input_status(&complete).may_claim_complete());
        assert!(!derive_input_status(&complete).forbids_automatic_replay());
        // 四值的字符串是稳定契约（报告/界面按它落列）。
        assert_eq!(InputStatus::None.as_str(), "none");
        assert_eq!(InputStatus::Partial.as_str(), "partial");
        assert_eq!(InputStatus::Complete.as_str(), "complete");
        assert_eq!(InputStatus::Unknown.as_str(), "unknown");
    }

    use super::{
        drag_path, select_backend, sendinput_unicode_text_script, InputBackend, MousePoint,
    };

    #[test]
    fn auto_backend_falls_back_to_sendinput_when_interception_context_is_unavailable() {
        assert_eq!(select_backend("auto", true, false), InputBackend::SendInput);
        assert_eq!(
            select_backend("interception", true, false),
            InputBackend::Interception
        );
        assert_eq!(
            select_backend("auto", true, true),
            InputBackend::Interception
        );
    }

    #[test]
    fn drag_path_keeps_start_and_end_points() {
        let path = drag_path(MousePoint { x: 10, y: 20 }, MousePoint { x: 30, y: 50 }, 4);

        assert_eq!(path.first(), Some(&MousePoint { x: 10, y: 20 }));
        assert_eq!(path.last(), Some(&MousePoint { x: 30, y: 50 }));
        assert!(path.len() >= 2);
    }

    #[test]
    #[cfg(windows)]
    fn click_mock_reports_native_failures_and_finally_releases() {
        use super::{run_powershell, sendinput_click_body, MouseButtonAction};
        use std::time::Duration;
        // 固定生产 PowerShell 点击体配纯内存 MouseOps，不导入 user32 或发送真实输入。
        let mock = r#"using System; using System.Collections.Generic;
public struct INPUT { public uint type; public MOUSEINPUT mi; }
public struct MOUSEINPUT { public int dx,dy; public uint mouseData,dwFlags,time; public UIntPtr dwExtraInfo; }
public static class MouseOps {
 public static int Mode; public static List<uint> Flags=new List<uint>();
 public static bool SetCursorPos(int x,int y){ return Mode!=1; }
 public static uint SendInput(uint n,INPUT[] p,int size){uint f=p[0].mi.dwFlags;Flags.Add(f);if((Mode==2&&f==2)||(Mode==3&&f==4))return 0;return 1;}
}"#;
        let body = sendinput_click_body(10, 20, MouseButtonAction::LeftClick);
        let script = format!(
            r#"$ErrorActionPreference='Stop'; Add-Type -TypeDefinition @'
{mock}
'@
foreach($mode in @(0,1,2,3)) {{
 [MouseOps]::Mode=$mode; [MouseOps]::Flags.Clear(); $failure=''
 try {{ {body} }} catch {{ $failure=$_.Exception.Message }}
 if($mode -eq 0 -and ($failure -ne '' -or ([MouseOps]::Flags -join ',') -ne '2,4')) {{throw "successful input order: error=$failure flags=$([MouseOps]::Flags -join ',')"}}
 if($mode -eq 1 -and ($failure -notmatch 'cursor_move_failed' -or [MouseOps]::Flags.Count -ne 0)) {{throw 'failed cursor must not click'}}
 if($mode -eq 2 -and ($failure -notmatch 'send_input_failed' -or ([MouseOps]::Flags -join ',') -ne '2,4')) {{throw 'failed down must release and fail'}}
 if($mode -eq 3 -and ($failure -notmatch 'mouse_release_failed' -or [MouseOps]::Flags.Count -lt 4)) {{throw 'failed release must remain failure'}}
}}
'click-native-checks:ok'
"#
        );
        assert_eq!(
            run_powershell(&script, Duration::from_secs(15))
                .unwrap()
                .trim(),
            "click-native-checks:ok"
        );
    }

    #[test]
    fn drag_path_deduplicates_tiny_movements() {
        let path = drag_path(MousePoint { x: 1, y: 1 }, MousePoint { x: 2, y: 2 }, 32);

        assert_eq!(
            path,
            vec![MousePoint { x: 1, y: 1 }, MousePoint { x: 2, y: 2 }]
        );
    }

    #[test]
    fn drag_path_clamps_segment_count() {
        let path = drag_path(MousePoint { x: 0, y: 0 }, MousePoint { x: 200, y: 0 }, 500);

        assert!(path.len() <= 129);
        assert_eq!(path.last(), Some(&MousePoint { x: 200, y: 0 }));
    }

    #[test]
    fn unicode_sendinput_script_uses_utf16_scan_codes_not_sendkeys() {
        let script = sendinput_unicode_text_script("A中😀");

        assert!(script.contains("KEYEVENTF_UNICODE"));
        assert!(script.contains("SendInput"));
        assert!(script.contains("MOUSEINPUT"));
        assert!(script.contains("$units = [uint16[]]@(65,20013,55357,56832);"));
        assert!(!script.contains("SendKeys"));
    }

    /// RPR-04d：受控原生输入生命周期（点击 / 文本 / 滚动 / 按键 / 组合键）。
    mod native_lifecycle {
        use super::super::*;
        use crate::cleanup::CleanupPolicy;
        use runtime::{InputDelivery, InputReleaseStatus};
        use std::time::{Duration, Instant};

        const WINDOW_TIMEOUT: Duration = Duration::from_secs(20);

        fn attempt_without_identity<'a>(cancelled: &'a dyn Fn() -> bool) -> NativeInputAttempt<'a> {
            NativeInputAttempt::without_identity(cancelled)
        }

        /// 释放义务必须**按实际按钮和按键**登记：不能统一抬整个键盘，也不能只处理左键。
        #[test]
        fn registered_release_obligations_follow_the_actual_buttons_and_keys() {
            let chord = click_release_obligation(MouseButtonAction::LeftRightChord);
            assert!(chord.describes_button(MouseButton::Left));
            assert!(
                chord.describes_button(MouseButton::Right),
                "和弦用了右键，就必须登记右键，不能只处理左键"
            );
            assert_eq!(
                click_release_obligation(MouseButtonAction::RightClick).buttons(),
                [MouseButton::Right]
            );
            assert_eq!(
                click_release_obligation(MouseButtonAction::DoubleClick).buttons(),
                [MouseButton::Left]
            );
            // 滚动不留按住状态。
            assert!(scroll_release_obligation().is_empty());
            // 文本：SendInput 每个单元是一次 down+up 同批注入 → 无义务；
            // Interception 逐字符按修饰键 → 只登记它可能按下的三个修饰键。
            assert!(text_release_obligation(InputBackend::SendInput).is_empty());
            assert_eq!(
                text_release_obligation(InputBackend::Interception).keys(),
                [VIRTUAL_KEY_SHIFT, VIRTUAL_KEY_CONTROL, VIRTUAL_KEY_ALT]
            );
            // 组合键：逐个登记组合里的键，而不是整块键盘。
            let combo = key_release_obligation(&[VIRTUAL_KEY_CONTROL, 0x41]);
            assert_eq!(combo.keys(), [VIRTUAL_KEY_CONTROL, 0x41]);
            assert!(!combo.describes_key(0x42), "没按的键不得进入义务登记");
            // 显式按下登记该按钮；抬起是在解除义务，不新增。
            assert!(button_state_release_obligation(MouseButton::Right, true)
                .describes_button(MouseButton::Right));
            assert!(button_state_release_obligation(MouseButton::Right, false).is_empty());
            // 义务的请求表示与摘要都是可核对的单一形式。
            assert_eq!(
                chord.request_json(),
                serde_json::json!({"buttons": ["left", "right"], "keys": []})
            );
            assert_eq!(chord.summary(), "buttons=[left,right] keys=[]");
            assert_eq!(scroll_release_obligation().summary(), "none");
        }

        /// 收尾策略只有一份：受控原生输入直接用 `cleanup.rs` 的定义，不另立数值。
        #[test]
        fn cleanup_policy_is_the_shared_one_and_not_a_second_runner() {
            let policy = native_cleanup_policy();
            assert_eq!(policy, CleanupPolicy::default());
            assert_eq!(policy.cooperative_exit_grace(), Duration::from_secs(2));
            assert_eq!(policy.cleanup_window(), Duration::from_secs(4));
            assert_eq!(policy.independent_release_wait_cap(), Duration::from_secs(2));
        }

        /// `CreateProcessW` 的命令行配额：**含终止空字符在内**共 32,767 个 **UTF-16 单元**。
        ///
        /// 它**不是** UTF-8 字节数，也**不是**"字符数"——中文一个字符只占 1 个单元，
        /// 但在 UTF-8 里占 3 个字节。计量口径必须与内核一致。
        const WINDOWS_COMMAND_LINE_LIMIT_UNITS: usize = 32_767;

        /// 软门槛：命令行**含终止符**不得超过这个单元数（为动态部分留出的最低余量）。
        ///
        /// 口径（第七轮 §5.1）：`32_767 − 31_000 = 1_767` 个 **UTF-16 单元**的最低余量
        /// （约 1.77 千个单元）；**不得**读成"约 1.8KB 字节余量"。
        const COMMAND_LINE_SOFT_LIMIT_UNITS: usize = 31_000;

        /// 两个**测试接缝**在入口脚本里的确切语句（`$sp` 来自 `mock_scenario`）。
        const SEAM_STARTUP_DELAY_STMT: &str = "if($sp[2]){Start-Sleep -Milli ([int]$sp[2])}";
        const SEAM_READY_MARKER_STMT: &str = "if($sp[1]){$null=ni $sp[1]}";

        /// std 在 Windows 上拼接命令行时的**参数转义规则**（逐条对应 std 的 `append_arg`）。
        ///
        /// 之所以要在这里给出模型，是因为需要**在启动之前**就知道单元数；模型的正确性由
        /// [`command_line_model_matches_a_real_process`] 用真实进程的
        /// `[Environment]::CommandLine` 核对（同一 argv，逐字相等）。
        fn append_escaped_arg(line: &mut Vec<u16>, arg: &[u16], force_quotes: bool) {
            let needs_quotes = force_quotes
                || arg.is_empty()
                || arg
                    .iter()
                    .any(|unit| *unit == b' ' as u16 || *unit == b'\t' as u16);
            if !needs_quotes && !arg.iter().any(|unit| *unit == b'"' as u16) {
                line.extend_from_slice(arg);
                return;
            }
            line.push(b'"' as u16);
            let mut backslashes = 0usize;
            for unit in arg {
                if *unit == b'"' as u16 {
                    // 引号前的反斜杠必须**翻倍**，再多一个用来转义引号本身（2n+1）。
                    // 这一条由 `command_line_model_matches_a_real_process` 实测核对：
                    // `\"` 必须是 3 个反斜杠（奇数才能转义引号），`\\"` 必须是 5 个。
                    line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
                    line.push(*unit);
                    backslashes = 0;
                } else if *unit == b'\\' as u16 {
                    backslashes += 1;
                } else {
                    line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
                    backslashes = 0;
                    line.push(*unit);
                }
            }
            // 末尾的反斜杠同样要翻倍，否则会把收尾引号转义掉。
            line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
            line.push(b'"' as u16);
        }

        /// 完整命令行（**含终止空字符**）的 UTF-16 单元数——与 `CreateProcessW` 的计量一致。
        fn command_line_units(command: &Command) -> usize {
            use std::os::windows::ffi::OsStrExt;
            let mut line: Vec<u16> = Vec::new();
            append_escaped_arg(&mut line, &command.get_program().encode_wide().collect::<Vec<_>>(), true);
            for arg in command.get_args() {
                line.push(b' ' as u16);
                append_escaped_arg(&mut line, &arg.encode_wide().collect::<Vec<_>>(), false);
            }
            // 终止空字符：内核把它算在 32,767 之内。
            line.len() + 1
        }

        /// helper 脚本经 `-Command` 内联传递：整条命令行必须留在 Windows 的配额之内。
        ///
        /// 口径（第七轮 §5.1 纠正）：配额是**含终止空字符在内**的 32,767 个 **UTF-16 单元**；
        /// 断言按单元数做，且**分别输出**"测试版（含两个测试接缝）"与"正式版（去掉接缝）"
        /// 两条命令行的长度。
        #[test]
        fn native_helper_stays_within_the_windows_command_line_limit() {
            let script = native_helper_script();
            let command = native_helper_command(&script);
            let units = command_line_units(&command);
            let script_units = script.encode_utf16().count();
            println!(
                "[helper 命令行] 测试版（含两个测试接缝）：脚本 {script_units} 单元／\
                 完整命令行（含终止符）{units} 单元／配额 {WINDOWS_COMMAND_LINE_LIMIT_UNITS} 单元／\
                 硬限余量 {} 单元",
                WINDOWS_COMMAND_LINE_LIMIT_UNITS.saturating_sub(units)
            );
            // 正式版：把两个**只在测试里存在**的接缝语句去掉之后的长度。
            assert!(
                script.contains(SEAM_STARTUP_DELAY_STMT) && script.contains(SEAM_READY_MARKER_STMT),
                "入口脚本必须仍然带有这两个测试接缝（否则下面的对照没有意义）"
            );
            let production_script = script
                .replace(SEAM_STARTUP_DELAY_STMT, "")
                .replace(SEAM_READY_MARKER_STMT, "");
            let production_units = command_line_units(&native_helper_command(&production_script));
            println!(
                "[helper 命令行] 正式版（去掉测试接缝）：完整命令行（含终止符）{production_units} 单元／\
                 两个接缝的成本 = {} 单元",
                units.saturating_sub(production_units)
            );
            assert!(
                units < WINDOWS_COMMAND_LINE_LIMIT_UNITS,
                "命令行（含终止符）达到 {units} 单元，超出 Windows 配额\
                 （{WINDOWS_COMMAND_LINE_LIMIT_UNITS} 个 UTF-16 单元，含终止空字符）：\
                 请把 C# 或入口拆小"
            );
            assert!(
                units < COMMAND_LINE_SOFT_LIMIT_UNITS,
                "命令行（含终止符）达到 {units} 单元，超过软门槛 {COMMAND_LINE_SOFT_LIMIT_UNITS} 单元：\
                 留给动态部分的余量不足 {} 个单元（口径是 UTF-16 单元，不是字节、也不是'约 1.8KB'）",
                COMMAND_LINE_SOFT_LIMIT_UNITS.saturating_sub(units)
            );
            // 硬限的**剩余额度**按单元写出来（含终止符的口径）。
            assert_eq!(
                WINDOWS_COMMAND_LINE_LIMIT_UNITS - units - 1,
                WINDOWS_COMMAND_LINE_LIMIT_UNITS - (units + 1),
                "剩余额度 = 32_767 − (命令行单元数 + 终止符)"
            );
            // 固定入口不允许任何脚本执行通路。
            for forbidden in ["Invoke-Expression", "IEX ", "iex(", "Add-Type -Path", "& $"] {
                assert!(!script.contains(forbidden), "固定 helper 不得含脚本执行入口：{forbidden}");
            }
        }

        /// **动态输入**也必须计量：最长受支持路径、空格／中文、参数转义都按同一口径算。
        ///
        /// 只量"某个脚本常量"是不够的——命令行的其余部分同样会随参数长大，因此这里用
        /// 真实 `Command`（同一个生产构造器）逐项核对单元数，并把每一项都打印出来。
        #[test]
        fn command_line_units_count_real_escaping_and_dynamic_arguments() {

            // ① 单元 vs 字节：中文一个字符 = 1 个 UTF-16 单元 = 3 个 UTF-8 字节。
            let chinese = "中文路径";
            assert_eq!(chinese.encode_utf16().count(), 4);
            assert_eq!(chinese.len(), 12);

            // ② 转义：引号前的反斜杠翻倍、末尾反斜杠翻倍。
            let mut line = Vec::new();
            let one_backslash_then_quote = "a".to_string() + "\\" + "\"" + "b";
            append_escaped_arg(
                &mut line,
                &one_backslash_then_quote.encode_utf16().collect::<Vec<_>>(),
                false,
            );
            assert_eq!(
                String::from_utf16(&line).unwrap(),
                format!("\"a{}\"b\"", "\\".repeat(3)),
                "`a\\\"b` 必须是 3 个反斜杠（2n+1，奇数才能转义引号）"
            );
            let mut trailing = Vec::new();
            // 带空格 ⇒ 必须加引号；此时末尾反斜杠若翻倍不够，就会把收尾引号转义掉。
            let ends_with_backslash = "tail ".to_string() + "\\";
            append_escaped_arg(
                &mut trailing,
                &ends_with_backslash.encode_utf16().collect::<Vec<_>>(),
                false,
            );
            assert_eq!(
                String::from_utf16(&trailing).unwrap(),
                format!("\"tail {}\"", "\\".repeat(2)),
                "末尾反斜杠必须翻倍（2n），否则会把收尾引号转义掉"
            );

            // ③ 实际动态参数：最长受支持路径、空格与中文。只量"某个脚本常量"是不够的，
            //    因此这里把同一段参数**真的**放进生产构造器，逐项核对单元数。
            let long_path = format!(r"C:\{}\_输入 文件.txt", r"很长的目录名\".repeat(8));
            let script_with_long_path =
                format!("$x='{}'; {}", long_path.replace('\'', "''"), "{ Write-Output $x }");
            let dynamic = native_helper_command(&script_with_long_path);
            let units = command_line_units(&dynamic);
            let expected = {
                use std::os::windows::ffi::OsStrExt;
                let mut expected: Vec<u16> = Vec::new();
                append_escaped_arg(
                    &mut expected,
                    &dynamic.get_program().encode_wide().collect::<Vec<_>>(),
                    true,
                );
                for arg in dynamic.get_args() {
                    expected.push(b' ' as u16);
                    append_escaped_arg(&mut expected, &arg.encode_wide().collect::<Vec<_>>(), false);
                }
                expected.len() + 1
            };
            assert_eq!(units, expected, "单元数必须与逐项拼接的结果一致");
            let baseline_units = command_line_units(&native_helper_command("$null"));
            assert!(
                units > baseline_units,
                "动态参数必须真的被算进行长（{units} 必须大于基线 {baseline_units}）"
            );
            // 口径的直接证据：同一段文本的 UTF-8 字节数明显大于 UTF-16 单元数（含中文）。
            assert!(
                script_with_long_path.len() > script_with_long_path.encode_utf16().count(),
                "含中文的文本：字节数必须大于 UTF-16 单元数（两种口径不能混用）"
            );
            println!(
                "[helper 命令行·动态] 基线 {baseline_units} 单元 → 最长受支持路径+空格+中文 {units} 单元                 （增量 {}）；同一段脚本：UTF-8 字节 {} ／ UTF-16 单元 {}",
                units.saturating_sub(baseline_units),
                script_with_long_path.len(),
                script_with_long_path.encode_utf16().count()
            );
            assert!(units < WINDOWS_COMMAND_LINE_LIMIT_UNITS);
        }

        /// 转义模型的**实测核对**：用一条带刁钻参数的真实命令行，逐字比对
        /// `[Environment]::CommandLine`（内核收到的原文）。
        ///
        /// 刁钻点：空格、制表符、中文、`\"`、`\\"`、**末尾反斜杠**。
        #[test]
        #[cfg(windows)]
        fn command_line_model_matches_a_real_process() {
            let probe = "$ErrorActionPreference='Stop'\n\
                 [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)\n\
                 $probe='中文 空格\ttab \"quote\" \\\\\" 双引号结束测试'\n\
                 [Console]::Out.Write([Environment]::CommandLine)\n\
                 # 末尾反斜杠 \\";
            let mut command = native_helper_command(probe);
            let predicted = {
                use std::os::windows::ffi::OsStrExt;
                let mut line: Vec<u16> = Vec::new();
                append_escaped_arg(
                    &mut line,
                    &command.get_program().encode_wide().collect::<Vec<_>>(),
                    true,
                );
                for arg in command.get_args() {
                    line.push(b' ' as u16);
                    append_escaped_arg(&mut line, &arg.encode_wide().collect::<Vec<_>>(), false);
                }
                String::from_utf16(&line).expect("模型拼出的命令行必须是合法 UTF-16")
            };
            let output = command.output().expect("必须能启动 powershell.exe 取回命令行");
            let actual = String::from_utf8_lossy(&output.stdout).to_string();
            assert!(
                !actual.is_empty(),
                "探针没有回显命令行：stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                actual.trim_end_matches(['\r', '\n']),
                predicted,
                "转义模型必须与内核收到的命令行逐字一致（否则所有计量口径都不可信）"
            );
            println!("[helper 命令行·转义模型] 实测一致：{} 单元", predicted.encode_utf16().count());
        }

        /// **测试接缝不可由正式路径启用**（第七轮 §5.1 第 13 条）。
        ///
        /// 两条接缝都只从 `mock_scenario`（请求 JSON）取值，而该键只由**测试构造**的请求写入：
        /// 正式 CU 参数、环境变量、用户设置里都没有它的写入点。
        #[test]
        fn helper_test_seams_cannot_be_enabled_from_production_inputs() {
            let source = include_str!("input.rs");
            let production = source
                .split("#[cfg(test)]\nmod tests {")
                .next()
                .expect("生产代码部分");
            // ① 唯一写入点：把 `mock_scenario` 参数放进请求；它来自函数参数而不是环境/配置。
            assert_eq!(
                production.matches("request[\"mock_scenario\"] = ").count(),
                1,
                "mock_scenario 只允许一个写入点"
            );
            for forbidden in [
                "env::var(\"COOLZHU_MOCK",
                "env::var_os(\"COOLZHU_MOCK",
                "mock_scenario: Some(",
            ] {
                assert!(
                    !production.contains(forbidden),
                    "接缝不得由环境变量或常量启用：{forbidden}"
                );
            }
            // ② 生产入口（`native_run`）必须传 `None`。
            let native_entry = production
                .split("fn native_run(")
                .nth(1)
                .expect("native_run")
                .split("fn validate_native_attempt(")
                .next()
                .expect("native_run 函数体");
            assert!(
                native_entry.contains("None,\n        backend,")
                    || native_entry.contains("None, backend,")
                    || native_entry.contains("None,"),
                "生产入口必须把 mock_scenario 传成 None"
            );
            // ③ 唯一能传 Some 的入口是测试构造的（编译期就不在生产构建里）。
            let mock_entry = production
                .split("fn native_run_with_mock(")
                .nth(1)
                .expect("native_run_with_mock 函数体");
            assert!(
                mock_entry.contains("Some(mock_scenario)"),
                "只有测试入口才会带 mock_scenario"
            );
            assert!(
                production.contains("#[cfg(test)]\nfn native_run_with_mock("),
                "带 mock_scenario 的入口必须是 #[cfg(test)]（生产构建不编译它）"
            );
            // ④ 入口脚本里两条接缝都**只在 `$sp` 非空时**执行，而 `$sp` 只来自 mock_scenario。
            let script = native_helper_script();
            assert!(script.contains(SEAM_STARTUP_DELAY_STMT) && script.contains(SEAM_READY_MARKER_STMT));
            assert!(script.contains("$sp=([string]$r.mock_scenario).Split('|')"));
            // 固定读取点共 6 处：`$sp[0]`（Release 与 Drivers::Create 各一次）
            // + 两个接缝各自的条件与取值（`$sp[2]` 两次、`$sp[1]` 两次）。
            assert_eq!(
                script.matches("$sp[").count(),
                6,
                "`$sp` 只允许在固定的读取点出现（新增取值点必须重新核对本用例）"
            );
            // ⑤ 受控笔画那一侧同样不写 mock_scenario。
            let stroke = include_str!("input_stroke.rs");
            let stroke_production = stroke.split("mod tests {").next().expect("生产代码部分");
            assert_eq!(
                stroke_production.matches("\"mock_scenario\"").count(),
                0,
                "笔画路径的生产请求不得带 mock_scenario（接缝只在测试请求里）"
            );
        }

        /// 身份诊断的措辞必须与受控笔画那一侧一致：同一件事不能有两种说法。
        #[test]
        fn native_identity_diagnostics_use_the_same_wording_as_the_controlled_stroke() {
            let script = native_helper_script();
            let stroke = include_str!("input_stroke_native.cs");
            for field in [
                "foreground expected=",
                "pid expected=",
                "dpi expected=",
                "rect expected=[",
                "available=",
            ] {
                assert!(script.contains(field), "受控原生 helper 缺少身份诊断字段 {field}");
                assert!(stroke.contains(field), "受控笔画缺少身份诊断字段 {field}");
            }
            assert!(script.contains("stale_observation: "));
        }

        /// 独立释放只在**事实推导出的义务状态**要求收尾时才补发，最多一次。
        ///
        /// CU-F01 归一后判定不再自己看字段拼条件（原来的"登记过义务 + helper 没确认
        /// + 可能按住"六参判定已被五态取代）。
        #[test]
        fn independent_release_is_needed_exactly_when_the_derived_state_requires_it() {
            use ReleaseObligationState::{EvidenceConflict, Possible, ProvenAbsent, Settled, Unsettled};
            // 释放通道自身：不递归补发。
            assert!(!needs_native_emergency_release(false, Unsettled, true, true, false));
            // 已证明不存在 / 已结清：不补发。
            assert!(!needs_native_emergency_release(true, ProvenAbsent, true, true, false));
            assert!(!needs_native_emergency_release(true, Settled, true, true, false));
            // 已产生且未结清：被强杀 / 非零退出 → 补发。
            assert!(needs_native_emergency_release(true, Unsettled, true, true, false));
            assert!(needs_native_emergency_release(true, Unsettled, true, false, false));
            // 正常退出：finally 已跑 → 不补发。
            assert!(!needs_native_emergency_release(true, Unsettled, true, false, true));
            // 可能存在 / 证据冲突：先确认执行者静止，否则隔离。
            assert!(needs_native_emergency_release(true, Possible, true, false, false));
            assert!(!needs_native_emergency_release(true, Possible, false, false, false));
            assert!(needs_native_emergency_release(true, EvidenceConflict, true, true, false));
            assert!(!needs_native_emergency_release(true, EvidenceConflict, false, true, false));
        }

        /// 进度文件的解析必须严格：任何缺失/多余/越界/自相矛盾都**不是**"零注入"，
        /// 一律保留异常（CU-F01：读不到 ≠ 零输入）。
        #[test]
        fn native_fact_parsing_is_strict_and_never_guesses() {
            // 没有记录 vs 记录不可信：两者都不是"零注入"。
            for raw in [
                br#"{"injected_steps":1,"completed":false}"#.as_slice(),
                br#"{"injected_steps":1,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":true,"released":null,"extra":1}"#,
                br#"{"injected_steps":1,"completed":false,"armed_buttons":["middle"],"armed_keys":[],"pressed":true,"released":null}"#,
                br#"{"injected_steps":1,"completed":false,"armed_buttons":[],"armed_keys":[0],"pressed":true,"released":null}"#,
                br#"{"injected_steps":2,"completed":true,"armed_buttons":["left"],"armed_keys":[],"pressed":true,"released":true}"#,
                br#"{"injected_steps":0,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":true,"released":null}"#,
                br#"{"injected_steps":4097,"completed":true,"armed_buttons":[],"armed_keys":[],"pressed":true,"released":true}"#,
            ] {
                let read = NativeInputFacts::read(raw, "req-1");
                assert!(
                    read.is_rejected(),
                    "不得把无法核对的字节拼成看起来可信的事实：{}",
                    String::from_utf8_lossy(raw)
                );
                assert!(read.anomaly().is_some_and(|anomaly| !anomaly.is_empty()));
            }
            assert!(matches!(
                NativeInputFacts::read(b"", "req-1"),
                HelperFactRead::Rejected { .. }
            ));
            assert!(NativeInputFacts::read(b"not json", "req-1").is_rejected());

            // v2 记录：协议/身份/阶段都核对过才可信。
            let v2 = br#"{"protocol":2,"request_id":"req-1","phase":"in_flight","cursor_moved":true,"injected_steps":1,"completed":false,"armed_buttons":["left"],"armed_keys":[17],"pressed":true,"released":null}"#;
            let facts = NativeInputFacts::read(v2, "req-1")
                .trusted()
                .cloned()
                .expect("v2 记录必须可信");
            assert_eq!(
                facts,
                NativeInputFacts {
                    protocol: HELPER_FACT_PROTOCOL_V2,
                    request_id: Some("req-1".to_string()),
                    phase: HelperFactPhase::InFlight,
                    cursor_moved: Some(true),
                    injected_steps: 1,
                    completed: false,
                    armed_buttons: vec![MouseButton::Left],
                    armed_keys: vec![17],
                    pressed: true,
                    released: None,
                }
            );
            // 协议版本不符 / 请求身份不合 / 阶段标记不可识别：都不可信。
            for raw in [
                br#"{"protocol":9,"request_id":"req-1","phase":"final","cursor_moved":false,"injected_steps":0,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":false,"released":null}"#.as_slice(),
                br#"{"protocol":2,"request_id":"someone-else","phase":"final","cursor_moved":false,"injected_steps":0,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":false,"released":null}"#,
                br#"{"protocol":2,"request_id":"req-1","phase":"nonsense","cursor_moved":false,"injected_steps":0,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":false,"released":null}"#,
                br#"{"protocol":2,"request_id":"req-1","phase":"pre_input","cursor_moved":true,"injected_steps":0,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":false,"released":null}"#,
            ] {
                assert!(
                    NativeInputFacts::read(raw, "req-1").is_rejected(),
                    "必须保留协议/身份/阶段异常：{}",
                    String::from_utf8_lossy(raw)
                );
            }

            // 旧版（v1）记录仍可读取，但不能充当"零注入"证明。
            let legacy = NativeInputFacts::read(
                br#"{"injected_steps":0,"completed":false,"armed_buttons":[],"armed_keys":[],"pressed":false,"released":null}"#,
                "req-1",
            )
            .trusted()
            .cloned()
            .expect("旧记录必须仍可读取");
            assert_eq!(legacy.protocol, HELPER_FACT_PROTOCOL_V1);
            assert_eq!(legacy.phase, HelperFactPhase::LegacyUnverifiable);
            assert_eq!(legacy.cursor_moved, None, "旧记录无从核对光标是否移动过");
        }

        /// 释放对账的判定表：任何"没有确认"的情形都不能写成已释放。
        #[test]
        fn release_accounting_never_claims_release_without_confirmation() {
            let left = ReleaseObligation::mouse_buttons(&[MouseButton::Left]);
            let holding = NativeInputFacts {
                protocol: HELPER_FACT_PROTOCOL_V2,
                request_id: None,
                phase: HelperFactPhase::InFlight,
                cursor_moved: Some(true),
                injected_steps: 1,
                completed: false,
                armed_buttons: vec![MouseButton::Left],
                armed_keys: Vec::new(),
                pressed: true,
                released: None,
            };
            // 请求没送到 helper：没有义务。
            assert_eq!(
                native_release_status(&left, false, None, false, false),
                InputReleaseStatus::NotNeeded
            );
            // 义务为空：这次操作在机制上不可能留下按住状态。
            assert_eq!(
                native_release_status(&ReleaseObligation::none(), true, None, false, true),
                InputReleaseStatus::NotNeeded
            );
            // 事实显示按住了登记之外的东西（这里是"登记为空却按住左键"）→ 未知，绝不放过。
            assert_eq!(
                native_release_status(&ReleaseObligation::none(), true, Some(&holding), false, true),
                InputReleaseStatus::Unknown
            );
            // 登记了义务但没有确认释放 → 未知。
            assert_eq!(
                native_release_status(&left, true, None, false, true),
                InputReleaseStatus::Unknown
            );
            // 未确认静止 → 未知（执行者可能还在跑）。
            assert_eq!(
                native_release_status(&left, true, None, true, false),
                InputReleaseStatus::Unknown
            );
            // 本进程补发并确认 → 已释放。
            assert_eq!(
                native_release_status(&left, true, None, true, true),
                InputReleaseStatus::Released
            );
            // helper 确认释放 → 已释放。
            let released = NativeInputFacts {
                protocol: HELPER_FACT_PROTOCOL_V2,
                request_id: None,
                phase: HelperFactPhase::Final,
                cursor_moved: Some(true),
                injected_steps: 2,
                completed: true,
                armed_buttons: Vec::new(),
                armed_keys: Vec::new(),
                pressed: true,
                released: Some(true),
            };
            assert_eq!(
                native_release_status(&left, true, Some(&released), false, true),
                InputReleaseStatus::Released
            );
            // 两段输入的释放事实合并：未知优先。
            assert_eq!(
                merge_input_release(InputReleaseStatus::Released, InputReleaseStatus::Unknown),
                InputReleaseStatus::Unknown
            );
            assert_eq!(
                merge_input_release(InputReleaseStatus::NotNeeded, InputReleaseStatus::Released),
                InputReleaseStatus::Released
            );
            assert_eq!(
                merge_input_release(InputReleaseStatus::NotNeeded, InputReleaseStatus::NotNeeded),
                InputReleaseStatus::NotNeeded
            );
        }

        /// 输入前的校验失败：明确的"未发送、无义务"，绝不启动 helper。
        #[test]
        fn pre_input_rejections_declare_not_sent_without_touching_the_desktop() {
            let cancelled = || false;
            let attempt = attempt_without_identity(&cancelled);
            // 空文本 / 超长文本。
            let too_long = "a".repeat(MAX_NATIVE_TEXT_UNITS + 1);
            for text in ["", too_long.as_str()] {
                let failure = controlled_type_text(text, &attempt, WINDOW_TIMEOUT)
                    .expect_err("非法文本必须在输入前拒绝");
                assert!(!failure.input_possible);
                assert!(failure.message.contains("invalid_native_text"));
                assert!(failure.cleanup().is_none());
                let receipt = native_failure_receipt("desktop:text:0", &failure);
                receipt.validate().expect("输入前拒绝的回执必须自洽");
                assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
                assert_eq!(receipt.partial, Some(false));
                assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
            }
            // 非法点击次数 / 非法滚动量 / 非法虚拟键与按住时长。
            assert!(controlled_click(10, 20, 3, &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_scroll(0, &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_scroll(601, &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_hold_key(0, 70, &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_hold_key(0x41, 0, &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_key_combo(&[], &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_key_combo(&[0, 0x41], &attempt, WINDOW_TIMEOUT).is_err());
            assert!(controlled_key_combo(&[1, 2, 3, 4, 5, 6, 7, 8, 9], &attempt, WINDOW_TIMEOUT).is_err());
            // 身份无效（handle/pid/dpi 为 0）：宁可不注入。
            let invalid_identity = NativeInputAttempt::with_window(
                StrokeWindow {
                    handle: 0,
                    process_id: 0,
                    rect: [0, 0, 100, 100],
                    dpi: 0,
                },
                &cancelled,
            );
            let failure = controlled_click(10, 20, 1, &invalid_identity, WINDOW_TIMEOUT)
                .expect_err("无效身份必须在输入前拒绝");
            assert!(!failure.input_possible);
            assert!(failure.message.contains("invalid_native_identity"));
        }

        /// 真实受控 helper（真实 PowerShell + 真实 C# 驱动，**零注入**）：
        /// 身份不匹配时 helper 在移动光标之前就自报失败，进度文件证明"什么都没注入"。
        #[test]
        #[cfg(windows)]
        fn real_helper_refuses_to_inject_when_the_window_identity_does_not_match() {
            let cancelled = || false;
            // handle=1/pid=4 永远不可能是当前前台窗口：Check() 先于任何注入抛出。
            let attempt = NativeInputAttempt::with_window(
                StrokeWindow {
                    handle: 1,
                    process_id: 4,
                    rect: [0, 0, 400, 300],
                    dpi: 96,
                },
                &cancelled,
            );
            let failure = controlled_click(10, 20, 1, &attempt, Duration::from_secs(15))
                .expect_err("身份不匹配必须拒绝注入");
            assert!(failure.input_possible, "请求已经送到 helper");
            assert_eq!(failure.kind(), StrokeFailureKind::Stale);
            assert!(failure.message.contains("stale_observation"));
            // 事实来自 helper 自己：它证明按下尚未发生。
            let facts = failure.facts().expect("真实 helper 必须写出起点事实");
            assert_eq!(facts.injected_steps, 0);
            assert!(!facts.pressed);
            assert!(!facts.still_holding());
            // 回复与静止分别有据可查。
            assert!(failure.reply_received_at_ms().is_some());
            assert!(failure.stillness_confirmed());
            assert!(!failure.must_quarantine());
            let receipt = native_failure_receipt("desktop:click:0", &failure);
            receipt.validate().expect("真实事实构成的回执必须自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
            assert_eq!(receipt.partial, Some(false));
            assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
        }

        /// **§B-89**：受控移动走**同一段受监督运行**，且如实登记"光标移动过、未按下"。
        ///
        /// 为什么必须真跑：helper 的 C# 由 PowerShell 在**运行时** `Add-Type` 编译，
        /// 单元测试编不出它的语法错误；只有真实运行一次才能证明新分支可用。
        /// 用 mock 驱动（不碰 user32、不注入真实输入）⇒ 不会移动用户桌面上的光标。
        #[test]
        #[cfg(windows)]
        fn real_helper_move_modes_report_cursor_movement_without_pressing() {
            let cancelled = || false;
            let attempt = attempt_without_identity(&cancelled);
            // 移动**没有释放义务**：这正是它可以与"按下类动作"共用同一运行器而不需要收尾释放的原因。
            let obligation = ReleaseObligation::none();
            for (mode, params) in [
                ("move", serde_json::json!({"x": 120, "y": 240})),
                ("move_relative", serde_json::json!({"x": 8, "y": -6})),
            ] {
                let outcome = native_run_with_mock(
                    mode,
                    params.clone(),
                    &obligation,
                    &attempt,
                    Duration::from_secs(20),
                    "",
                )
                .unwrap_or_else(|failure| panic!("{mode} 必须成功：{}", failure.message));
                let facts = outcome.facts().expect("helper 必须写出事实");
                assert_eq!(
                    facts.cursor_moved,
                    Some(true),
                    "{mode}：光标移动过必须如实登记"
                );
                assert_eq!(
                    facts.injected_steps, 0,
                    "{mode}：移动不是按钮/键/滚动类输入事件，不得记为注入步数"
                );
                assert!(facts.completed, "{mode}：走完必须如实登记");
                assert!(!facts.pressed, "{mode}：移动**永不按下**");
                assert!(!facts.still_holding(), "{mode}：不得留下按住状态");
                // 收尾的 `released` 是「无残留按下」的确认：移动从不按下，因此这里
                // `Some(true)` 表示"确认没有残留"，与 click 的收尾口径一致——
                // 它**不是**在说"释放了一个我从没按下的按钮"。
                assert_eq!(
                    facts.released,
                    Some(true),
                    "{mode}：收尾必须确认无残留（没有义务≠不确认）"
                );
                // 派生视图：移动**完成**（Sent + 明确非部分），但没有注入点数。
                // 走**生产同一条**派生入口（`release_inputs` + `derive_delivery_facts`）。
                let delivery = derive_delivery_facts(
                    &outcome.release_inputs(StrokeFailureKind::Failed),
                );
                assert_eq!(
                    derive_input_status(&delivery),
                    InputStatus::Complete,
                    "{mode}：移动走完就是 complete（而不是 unknown 或 partial）"
                );
                assert!(
                    !derive_input_status(&delivery).forbids_automatic_replay(),
                    "{mode}：完成的移动不需要重放禁令"
                );
            }
        }

        /// 真实受控 helper：**按下之后失败**。helper 自己登记过左键、确认走不完，
        /// 并在收尾里把它抬起来；回执必须同时保留"部分注入"与"已释放"两个事实。
        #[test]
        #[cfg(windows)]
        fn real_helper_fails_after_pressing_and_still_reports_partial_input_and_release() {
            let cancelled = || false;
            let attempt = attempt_without_identity(&cancelled);
            let obligation = click_release_obligation(MouseButtonAction::LeftClick);
            let failure = native_run_with_mock(
                "click",
                serde_json::json!({"action": "left", "x": 10, "y": 20}),
                &obligation,
                &attempt,
                Duration::from_secs(20),
                "fail_after_press",
            )
            .expect_err("按下之后失败必须报错");
            assert!(failure.input_possible);
            let facts = failure.facts().expect("helper 必须写出事实");
            assert_eq!(facts.injected_steps, 1, "左键按下已确认");
            assert!(facts.pressed, "按下过必须如实记录");
            assert!(!facts.completed);
            assert!(!facts.still_holding());
            assert_eq!(facts.released, Some(true), "收尾释放已确认");
            assert!(failure.stillness_confirmed());
            assert_eq!(failure.release_status(), InputReleaseStatus::Released);
            assert!(!failure.must_quarantine());
            let receipt = native_failure_receipt("desktop:click:0", &failure);
            receipt.validate().expect("自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::Sent);
            assert_eq!(receipt.partial, Some(true));
            assert_eq!(receipt.path_completed, None);
            assert_eq!(receipt.input_release, InputReleaseStatus::Released);
            // CU-F01 §3：helper 已经**自己确认过**释放结果 ⇒ 义务已结清，
            // 不再由主机补发第二次 UP（"已产生且已结清 ⇒ 不重复补发"）。
            assert_eq!(
                failure.release_state(),
                ReleaseObligationState::Settled,
                "helper 已确认释放的义务必须算已结清"
            );
            assert!(
                failure.cleanup().is_none(),
                "没有需要收尾的义务就不该进入收尾：{:?}",
                failure.cleanup()
            );
        }

        /// 真实受控 helper：**部分文本输入**。helper 逐步汇报已输入的字符数，
        /// 生产解析器读回它，回执必须写"部分"而不是"完整"。
        #[test]
        #[cfg(windows)]
        fn real_helper_reports_partial_text_input_facts_the_production_parser_reads() {
            let cancelled = || false;
            let attempt = attempt_without_identity(&cancelled);
            let obligation = text_release_obligation(InputBackend::SendInput);
            assert!(obligation.is_empty(), "SendInput 文本不留下按键义务");
            let failure = native_run_with_mock(
                "text",
                serde_json::json!({"text": "abcdef"}),
                &obligation,
                &attempt,
                Duration::from_secs(20),
                "partial_text",
            )
            .expect_err("文本中途失败必须报错");
            let facts = failure.facts().expect("helper 必须写出事实");
            assert_eq!(facts.injected_steps, 2, "只有 helper 确认过的字符才算数");
            assert!(!facts.completed);
            assert!(!facts.pressed, "SendInput 文本不按下任何按键");
            assert_eq!(facts.released, Some(true));
            assert!(failure.stillness_confirmed());
            let receipt = native_failure_receipt("desktop:text_input:0", &failure);
            receipt.validate().expect("自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::Sent);
            assert_eq!(receipt.partial, Some(true), "部分输入不得升格为完整");
            assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
            assert!(!failure.must_quarantine());
        }

        /// 真实受控 helper：**组合键中途失败**。已经按下过的那一个键必须留在本次运行的
        /// 记录里，收尾释放只针对这些键（不是整块键盘）。
        #[test]
        #[cfg(windows)]
        fn real_helper_combo_failure_releases_exactly_the_keys_it_pressed() {
            let cancelled = || false;
            let attempt = attempt_without_identity(&cancelled);
            let keys = [VIRTUAL_KEY_CONTROL, 0x41];
            let obligation = key_release_obligation(&keys);
            assert_eq!(obligation.keys(), keys);
            let failure = native_run_with_mock(
                "combo",
                serde_json::json!({"keys": [u32::from(VIRTUAL_KEY_CONTROL), 0x41_u32]}),
                &obligation,
                &attempt,
                Duration::from_secs(20),
                "combo_mid_fail",
            )
            .expect_err("组合键中途失败必须报错");
            let facts = failure.facts().expect("helper 必须写出事实");
            assert_eq!(facts.injected_steps, 1, "只按下了第一个键");
            assert!(facts.pressed);
            assert!(!facts.completed);
            assert!(!facts.still_holding(), "收尾把按下的那个键抬起来了");
            assert_eq!(facts.released, Some(true));
            assert_eq!(failure.release_status(), InputReleaseStatus::Released);
            let receipt = native_failure_receipt("desktop:key_combination:0", &failure);
            receipt.validate().expect("自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::Sent);
            assert_eq!(receipt.partial, Some(true));
            assert_eq!(receipt.input_release, InputReleaseStatus::Released);
            // 释放义务覆盖组合里的每个键，而不是"只处理某一个"。
            assert!(obligation.describes_key(VIRTUAL_KEY_CONTROL));
            assert!(obligation.describes_key(0x41));
        }

        // ------------------------------------------------------------------
        // RD4-08 §B-6：把"启动预算"与"已 ready 的取消/静止确认"拆成确定可复现的用例
        // ------------------------------------------------------------------
        //
        // 被拆掉的旧用例是 `timeout_kills_a_helper_that_ignores_cancellation_and_confirms_stillness`。
        // 它把两件互不相干的事放在一个 600ms 业务期限里：
        //
        //  * 它断言 `facts.injected_steps >= 1`（"阶段回执必须留下"），而这要求 helper
        //    在 **600ms 内**完成 `Add-Type` 编译（csc.exe）并写出第一条进度记录；
        //  * 只有满足这个前提，它才真的在验证"执行者确认运行后忽略取消 + 生产收尾协调器有界处置"。
        //
        // 并发负载下前提不成立：helper 连第一条记录都没写出，用例失败在一个它**根本没进入**
        // 的分支上；更糟的是这个失败很容易被误读成"强杀或静止确认不可靠"。
        //
        // 拆成三个用例（口径：不能只看红灯就归因新回归，也不能单跑转绿就说没有回归）：
        //
        // 1. **启动预算用例**：用**受控启动延迟**确定"业务期限耗尽时 helper 尚未 ready"，
        //    验证完整业务期限照常耗尽并触发正确处理；阶段回执缺失的原因是"尚未 ready"
        //    有据可查，**不是**"强杀/静止确认不可靠"，也**不**在 ready 后刷新生产 deadline；
        // 2. **已 ready 用例**：用 **ready 屏障**确认执行者已就绪，再触发取消；用一个
        //    **独立且有限的测试准备期限**兜住"等不到 ready"的情形，并明确本条验证的是
        //    **收尾阶段**（ready 之后的取消/静止确认）；
        // 3. **真实完整 helper 冒烟**：真实 `Add-Type`／启动／执行／结束链在**声明预算**下
        //    的表现，记录全部阶段时间；预算不适配就如实失败，**不能**由前两类用例替代。
        //
        // 三者都不改生产收尾数值（协作退出 ≤2s、总窗口 ≤4s、独立释放 ≤`min(2s,剩余)`），
        // 也不在生产路径上刷新业务期限。测试接缝（受控延迟/ready 屏障/阶段时间线）只经
        // `mock_scenario` 字符串传入，生产请求里没有任何写入该键的调用点。

        // ------------------------------------------------------------------
        // §C-21 验收：**容量不足时 helper spawn 次数为零**（隔离子进程里的真实账目核对）
        // ------------------------------------------------------------------

        /// 父测试驱动的**专用测试子进程**环境变量名（子进程体只在它存在时运行）。
        const CU_CAPACITY_CHILD_ENV: &str = "COOLZHU_CU_READER_CAPACITY_CHILD";
        /// 子进程体的测试名（父进程用 `--exact` 只跑它）。
        const CU_CAPACITY_CHILD_TEST: &str =
            "input::tests::native_lifecycle::cu_reader_capacity_child_process_body";

        /// 子进程体：真实占满 8 个 reader 单位，然后走**生产**运行路径，证明"拒绝发生在
        /// 创建执行者之前"（spawn 尝试次数增量 == 0）。
        ///
        /// 为什么必须独立进程：读取器容量是**进程级**的；在共享测试进程里占满 8 个单位
        /// 会让并行用例一起被拒，那与它们要验证的行为无关。
        #[test]
        #[cfg(windows)]
        fn cu_reader_capacity_child_process_body() {
            if std::env::var_os(CU_CAPACITY_CHILD_ENV).is_none() {
                return;
            }
            use windows_process_guard::{pipe_reader_capacity, HelperPipeReadersAdmission};
            // 本线程**不做测试闸门等待**：本用例要验证的正是"饱和账目下的拒绝"，
            // 闸门的"等真实账目放得下"会与它冲突（那会把拒绝变成挂起）。
            // 闸门是测试专用的独立计数器，这里只是让它在本线程上直接让路。
            let gate_bypass = |run: &mut dyn FnMut()| {
                let previous = PIPE_READER_TEST_HELD.with(|held| held.replace(usize::MAX));
                run();
                PIPE_READER_TEST_HELD.with(|held| held.set(previous));
            };
            let initial = pipe_reader_capacity();
            assert_eq!(
                initial.obligations(),
                0,
                "子进程必须是干净起点：{initial}"
            );
            println!("[cu-capacity-child] 起点 {initial}");

            // 4 个**存活**的真实子进程 × (stdout + stderr) = 8 个 reader = 上限。
            // 这里用的是**生产接纳接口**（凭证 → 两条流），不做任何故障注入。
            let mut holders = Vec::new();
            for index in 0..4 {
                let mut child = Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "Start-Sleep -Seconds 60",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .creation_flags(0x0800_0000)
                    .spawn()
                    .expect("必须能启动 powershell.exe 持有管道写端");
                let mut admission = HelperPipeReadersAdmission::acquire()
                    .unwrap_or_else(|denial| panic!("空账目下必须接纳：{denial}"));
                let out = admission
                    .stdout(format!("cu-capacity-{index}-stdout"), child.stdout.take().expect("stdout"))
                    .expect("stdout 读取器");
                let err = admission
                    .stderr(format!("cu-capacity-{index}-stderr"), child.stderr.take().expect("stderr"))
                    .expect("stderr 读取器");
                // 监督器**保持存活**（这就是"活动未回收"的 8 个单位）。
                holders.push((child, out, err));
            }
            let saturated = pipe_reader_capacity();
            println!("[cu-capacity-child] 占满后 {saturated}");
            assert_eq!(saturated.limit, 8);
            assert_eq!(
                saturated.active_unreclaimed, 8,
                "8 个 reader 必须都记在'活动未回收'里"
            );
            assert_eq!(saturated.available(), 0);
            assert_eq!(saturated.unowned_unreclaimed, 0);
            assert!(!saturated.admission_locked, "这是容量不足，不是契约故障");

            // ① 原生输入路径：容量不足 ⇒ **不创建执行者、不产生输入**。
            let before = test_native_helper_spawn_attempts();
            let attempt = attempt_without_identity(&|| false);
            let mut failure_slot = None;
            gate_bypass(&mut || {
                failure_slot = Some(
                    native_run(
                        "text",
                        serde_json::json!({ "text": "x" }),
                        text_release_obligation(InputBackend::SendInput),
                        &attempt,
                        Duration::from_millis(600),
                        InputBackend::SendInput,
                    )
                    .expect_err("容量饱和时必须拒绝"),
                );
            });
            let failure = failure_slot.expect("必须拿到拒绝结果");
            let after = test_native_helper_spawn_attempts();
            println!(
                "[cu-capacity-child] 原生路径被拒：{}；（helper spawn 尝试增量 {}）",
                failure.message,
                after.saturating_sub(before)
            );
            assert!(
                failure.message.contains("pipe_reader_capacity_exhausted"),
                "拒绝原因必须能被识别为容量不足：{}",
                failure.message
            );
            assert!(!failure.input_possible, "拒绝必须发生在任何输入之前");
            assert_eq!(
                after.saturating_sub(before),
                0,
                "容量不足时 **helper spawn 次数必须为零**（旧顺序是先 spawn 再申请容量）"
            );

            // ② 受控桌面（笔画/截图）路径：同一条秩序（spawn 之前拒绝）。
            let before = test_native_helper_spawn_attempts();
            let mut stroke_slot = None;
            gate_bypass(&mut || {
                stroke_slot = Some(
                    crate::input::capture_window_image(
                        crate::input::StrokeWindow {
                            handle: 1,
                            process_id: 1,
                            rect: [0, 0, 16, 16],
                            dpi: 96,
                        },
                        Duration::from_millis(600),
                    )
                    .expect_err("容量饱和时受控桌面 helper 也必须被拒绝"),
                );
            });
            let stroke = stroke_slot.expect("必须拿到拒绝结果");
            let after = test_native_helper_spawn_attempts();
            println!(
                "[cu-capacity-child] 受控桌面路径被拒：{stroke}；（helper spawn 尝试增量 {}）",
                after.saturating_sub(before)
            );
            assert!(
                stroke.contains("pipe_reader_capacity_exhausted"),
                "拒绝原因必须能被识别为容量不足：{stroke}"
            );
            assert_eq!(
                after.saturating_sub(before),
                0,
                "受控桌面路径同样必须**在创建执行者之前**拒绝"
            );

            // 账目没有被拒绝动作改动。
            let after_denials = pipe_reader_capacity();
            println!("[cu-capacity-child] 两次拒绝之后 {after_denials}");
            assert_eq!(after_denials.active_unreclaimed, 8);
            assert_eq!(after_denials.retained_unreclaimed, 0, "拒绝不得新增登记项");
            assert_eq!(after_denials.unowned_unreclaimed, 0);
            assert_eq!(after_denials.capacity_fault_units, 0);

            // 有界清理：结束全部持有者，等容量按**实际回收**归位（不留孤儿进程）。
            for (mut child, out, err) in holders {
                let _ = child.kill();
                let _ = child.wait();
                drop(out);
                drop(err);
            }
            let started = Instant::now();
            while pipe_reader_capacity().unreclaimed() > 0
                && started.elapsed() < Duration::from_secs(10)
            {
                let _ = windows_process_guard::reclaim_finished_pipe_readers();
                std::thread::sleep(Duration::from_millis(50));
            }
            let final_capacity = pipe_reader_capacity();
            println!("[cu-capacity-child] 清理完成 {final_capacity}");
            assert_eq!(final_capacity.obligations(), 0);
            assert_eq!(final_capacity.unowned_unreclaimed, 0);
            assert_eq!(final_capacity.available(), 8);
            println!("[cu-capacity-child] done");
        }

        /// **父级**：启动专用测试子进程，自己做有界看门与回收（不无界等待）。
        #[test]
        #[cfg(windows)]
        fn cu_reader_capacity_isolation_parent_runs_a_dedicated_child_process() {
            let exe = std::env::current_exe().expect("当前测试可执行文件");
            let mut child = Command::new(&exe)
                .args(["--exact", "--nocapture", CU_CAPACITY_CHILD_TEST])
                .env(CU_CAPACITY_CHILD_ENV, "1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(0x0800_0000)
                .spawn()
                .expect("必须能启动专用测试子进程");
            let (tx, rx) = std::sync::mpsc::channel();
            let watchdog = Duration::from_secs(120);
            std::thread::spawn(move || {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = std::io::Read::read_to_string(&mut pipe, &mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut pipe, &mut stderr);
                }
                let status = child.wait();
                let _ = tx.send((stdout, stderr, status));
            });
            let (stdout, stderr, status) = rx
                .recv_timeout(watchdog)
                .unwrap_or_else(|_| panic!("专用测试子进程未在 {watchdog:?} 内结束"));
            for line in stdout.lines().filter(|line| line.starts_with("[cu-capacity-child]")) {
                println!("[cu-capacity-parent 转发] {line}");
            }
            assert!(
                status.is_ok_and(|status| status.success()),
                "专用测试子进程必须成功退出：stdout={stdout}\nstderr={stderr}"
            );
            for anchor in [
                "[cu-capacity-child] 起点",
                "[cu-capacity-child] 占满后",
                "[cu-capacity-child] 原生路径被拒",
                "[cu-capacity-child] 受控桌面路径被拒",
                "[cu-capacity-child] 两次拒绝之后",
                "[cu-capacity-child] 清理完成",
            ] {
                assert!(
                    stdout.contains(anchor),
                    "子进程输出缺少证据锚点 {anchor:?}：\n{stdout}"
                );
            }
            println!("[cu-capacity-parent] 证据锚点齐全");
        }


        /// 受控启动延迟：写在 helper 写出任何进度记录**之前**。
        ///
        /// 必须大于"业务期限 + 协作退出额度"，这样"helper 在被强杀之前不会 ready"是
        /// **由构造保证**的确定条件，而不是靠时序碰运气。
        const LAUNCH_DELAY_MS: u64 = 4_000;

        /// 已 ready 用例的**测试准备期限**：只是"等不到 ready 就如实失败"的上限。
        ///
        /// 它**不是**被测的业务期限（取消由 ready 屏障后的信号触发，不是由它触发），
        /// 也**不授权**生产系统忽略启动耗时。
        const READY_PREP_DEADLINE: Duration = Duration::from_secs(20);

        /// 真实完整 helper 冒烟的**声明预算**（超了就如实失败）。
        ///
        /// "预检 + 启动"含后端预检自己的一次 PowerShell（`verify_interception_context`
        /// 自身超时 6s），因此这个声明值比 helper 自身的启动预算宽；整批用例并行时
        /// 实测启动阶段可以达到 ~16s（空闲/串行时 ~2.4–2.7s），所以这里的声明值按
        /// "并行最坏"给足。它只用来把"预算不适配"变成**明确失败**，真正的证据是每次
        /// 打印出来的实测阶段时间。
        const SMOKE_STARTUP_BUDGET: Duration = Duration::from_secs(30);
        const SMOKE_EXECUTION_BUDGET: Duration = Duration::from_secs(10);
        const SMOKE_STILLNESS_BUDGET: Duration = Duration::from_secs(10);
        const SMOKE_TOTAL_BUDGET: Duration = Duration::from_secs(45);

        /// 测试接缝：ready 屏障文件的路径（每次调用都唯一）。
        ///
        /// 接缝**只经 `mock_scenario` 字符串传入**（该字段只有测试构造的请求会带）：生产
        /// 请求里没有它，helper 入口脚本里对应的 `$sp` 分支因此整条不执行。
        fn ready_seam_path(tag: &str) -> std::path::PathBuf {
            std::env::temp_dir().join(format!(
                "coolzhu-cu-ready-{}-{}-{tag}",
                std::process::id(),
                unix_ms()
            ))
        }

        /// 测试接缝场景串：`<场景>|<ready 文件>|<executor 启动延迟毫秒>`。
        ///
        /// 延迟发生在 helper **写出 ready 标记之前、任何进度记录之前**：因此
        /// "业务期限耗尽时 helper 尚未 ready"是受控延迟保证的确定条件。
        fn seam_scenario(base: &str, ready_path: &std::path::Path, startup_delay_ms: u64) -> String {
            format!("{base}|{}|{startup_delay_ms}", ready_path.display())
        }

        /// ready 屏障文件的创建时刻（宿主文件系统时钟，同一台机器）。
        fn ready_seam_unix_ms(path: &std::path::Path) -> Option<u64> {
            let modified = std::fs::metadata(path).ok()?.modified().ok()?;
            let since = modified
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .ok()?;
            Some(since.as_millis().min(u128::from(u64::MAX)) as u64)
        }

        /// 当前配置的后端名称（只读环境变量，便于报告里区分"是否含后端预检"）。
        fn configured_backend_label() -> String {
            std::env::var("CLAW_MOUSE_BACKEND").unwrap_or_else(|_| "auto".to_string())
        }

        /// 重复次数（固定次数、保存每次结果；不做"失败就再跑到绿"）。
        fn repeat_count() -> usize {
            std::env::var("RD4_REPEAT")
                .ok()
                .and_then(|value| value.trim().parse::<usize>().ok())
                .filter(|count| *count > 0)
                .unwrap_or(3)
        }

        /// **受控负载**：若干个 PowerShell 反复做 `Add-Type -TypeDefinition`（每次都是新的
        /// 命名空间 ⇒ 真的起一次 csc.exe 编译），与并发负载拖慢 helper 启动时的负载同形状。
        /// 它们到点自行结束；测试结束时再有界地结束它们。
        struct ControlledLoad {
            children: Vec<std::process::Child>,
        }

        impl ControlledLoad {
            /// 按 `RD4_LOAD_PROCS`（默认 0 = 空闲）起负载，存活 `window`。
            fn spawn_from_env(window: Duration) -> Self {
                let count = std::env::var("RD4_LOAD_PROCS")
                    .ok()
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let seconds = window.as_secs().max(1);
                let script = format!(
                    "$end=(Get-Date).AddSeconds({seconds}); $i=0; \
                     while((Get-Date) -lt $end) {{ $i++; $type='namespace CoolzhuRd4Load' + $i + ' {{ public class C {{ public int X; }} }}'; \
                     Add-Type -TypeDefinition $type -ErrorAction SilentlyContinue }}"
                );
                let children = (0..count)
                    .map(|_| {
                        let mut command = Command::new("powershell.exe");
                        command
                            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null());
                        command.creation_flags(CREATE_NO_WINDOW);
                        command.spawn().expect("必须能启动受控负载进程")
                    })
                    .collect::<Vec<_>>();
                Self { children }
            }

            fn processes(&self) -> usize {
                self.children.len()
            }
        }

        impl Drop for ControlledLoad {
            fn drop(&mut self) {
                // 有界回收：负载进程自己到点也会退出，这里只是不等它。
                for child in &mut self.children {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

        /// **启动预算用例**：helper **尚未 ready** 时，完整业务期限仍会耗尽并触发正确处理。
        ///
        /// 确定条件由受控延迟建立：mock 在写出任何进度记录之前先睡 [`LAUNCH_DELAY_MS`]，
        /// 因此"600ms 业务期限到点时 helper 还没有 ready"是构造保证的。
        ///
        /// 断言只用**能证明的事实**：
        /// * ready 标记**从未出现** ⇒ 阶段回执缺失的原因就是"尚未 ready"（不臆测、也不改口径）；
        /// * 取消分类、强制终止、静止确认、协作退出额度都如实记录（有界）；
        /// * 缺失的事实**不被填零**、不伪造释放结论；
        /// * 整条路径**没有**在 ready 之后刷新生产 deadline（本用例根本不经过 ready）。
        #[test]
        #[cfg(windows)]
        fn launch_budget_expiry_before_helper_ready_is_handled_correctly() {
            let repeats = repeat_count();
            let load = ControlledLoad::spawn_from_env(Duration::from_secs(20 + 20 * repeats as u64));
            println!(
                "[启动预算] 重复 {repeats} 次，受控负载进程 {}(RD4_LOAD_PROCS)，后端配置={}",
                load.processes(),
                configured_backend_label()
            );
            let mut rounds = 0usize;
            for round in 1..=repeats {
                let ready_file = ready_seam_path("launch-budget");
                let _ = std::fs::remove_file(&ready_file);
                let scenario = seam_scenario("slow_steps", &ready_file, LAUNCH_DELAY_MS);
                // 取消**只能**由业务期限触发：这里不给任何外部取消信号。
                let not_cancelled = || false;
                let attempt = attempt_without_identity(&not_cancelled);
                let obligation = text_release_obligation(InputBackend::SendInput);
                // 测试专用：闸门等待在计时之前完成，保证下面记录的是真实阶段时间。
                let _test_pipe_slot = test_pipe_reader_capacity_slot(NATIVE_RUN_READER_UNITS);
                let started = Instant::now();
                let failure = native_run_with_mock(
                    "text",
                    serde_json::json!({ "text": "x".repeat(200) }),
                    &obligation,
                    &attempt,
                    Duration::from_millis(600),
                    &scenario,
                )
                .expect_err("业务期限耗尽必须失败");
                let elapsed = started.elapsed();
                assert!(failure.message.contains("stroke_cancelled"));
                assert_eq!(failure.kind(), StrokeFailureKind::Cancelled);
                // ready 从未出现：helper 根本没进入"执行者忽略取消"那一段。
                assert!(
                    !ready_file.exists(),
                    "第 {round} 次：受控延迟期间 helper 不该 ready（构造前提被破坏）"
                );
                assert!(
                    failure.facts().is_none(),
                    "第 {round} 次：未 ready 就不该有阶段回执——缺失原因是'尚未 ready'，\
                     不是'强杀或静止确认不可靠'"
                );
                // 收尾事实：强杀 + 静止确认 + 协作退出额度用满（有界）。
                let cleanup = failure.cleanup().expect("业务期限耗尽必须进入收尾").clone();
                assert!(cleanup.forced_kill, "第 {round} 次：必须强制终止");
                assert!(failure.stillness_confirmed(), "第 {round} 次：必须确认静止");
                assert!(failure.process_exit_confirmed_at_ms().is_some());
                assert!(
                    cleanup.cooperative_exit_waited_ms >= 2_000
                        && cleanup.cooperative_exit_waited_ms <= 2_600,
                    "第 {round} 次：协作退出额度必须是既定的 2 秒：{:?}",
                    cleanup.cooperative_exit_waited_ms
                );
                assert!(
                    !cleanup.independent_release_issued,
                    "第 {round} 次：SendInput 文本没有释放义务"
                );
                // 事实缺失不得被填成"零注入"，也不得凭空写释放结论。
                let receipt = native_failure_receipt("desktop:text_input:0", &failure);
                receipt.validate().expect("回执必须自洽");
                assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
                assert_eq!(receipt.partial, None, "未知不得被写成完整或部分");
                assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
                println!(
                    "[启动预算] 第{round}次：受控启动延迟={LAUNCH_DELAY_MS}ms（ready 未出现）／\
                     业务期限=600ms 耗尽／取消分类={:?}／强制终止={}／静止确认={}／\
                     协作退出={}ms／独立释放={}／总耗时={:?}",
                    failure.kind(),
                    cleanup.forced_kill,
                    failure.stillness_confirmed(),
                    cleanup.cooperative_exit_waited_ms,
                    cleanup.independent_release_issued,
                    elapsed
                );
                rounds += 1;
            }
            assert_eq!(rounds, repeats);
        }

        /// **已 ready 的取消／静止确认用例**：执行者确认运行后忽略取消，生产收尾协调器有界处置。
        ///
        /// 屏障：只有**确认观测到 ready 标记**（`Add-Type` 已完成、执行者已就绪）之后才发出取消信号；
        /// 取消因此一定发生在执行者已经运行的那一段里。ready 一直不出现时用
        /// [`READY_PREP_DEADLINE`] 兜住，并**如实报基础设施/协议失败**（不算通过）。
        ///
        /// 本条验证的是**收尾阶段**：ready 之后的取消、强杀、静止确认与同一个收尾窗口。
        #[test]
        #[cfg(windows)]
        fn ready_executor_ignoring_cancellation_gets_bounded_cleanup_and_stillness_confirmation() {
            let repeats = repeat_count();
            let load = ControlledLoad::spawn_from_env(Duration::from_secs(20 + 20 * repeats as u64));
            println!(
                "[ready 后取消] 重复 {repeats} 次，受控负载进程 {}(RD4_LOAD_PROCS)，后端配置={}",
                load.processes(),
                configured_backend_label()
            );
            use std::cell::Cell;
            for round in 1..=repeats {
                let ready_file = ready_seam_path("ready-cancel");
                let _ = std::fs::remove_file(&ready_file);
                // 本用例不测启动预算：不设启动延迟，只等 ready 屏障。
                let scenario = seam_scenario("slow_steps", &ready_file, 0);
                let ready_seen_at = Cell::new(None::<Instant>);
                let cancel_after_ready = || {
                    if ready_file.exists() {
                        if ready_seen_at.get().is_none() {
                            ready_seen_at.set(Some(Instant::now()));
                        }
                        true
                    } else {
                        false
                    }
                };
                let attempt = attempt_without_identity(&cancel_after_ready);
                let obligation = text_release_obligation(InputBackend::SendInput);
                // 测试专用：闸门等待在计时之前完成，保证下面记录的是真实阶段时间。
                let _test_pipe_slot = test_pipe_reader_capacity_slot(NATIVE_RUN_READER_UNITS);
                let started_unix = unix_ms();
                let started = Instant::now();
                let failure = native_run_with_mock(
                    "text",
                    serde_json::json!({ "text": "x".repeat(200) }),
                    &obligation,
                    &attempt,
                    READY_PREP_DEADLINE,
                    &scenario,
                )
                .expect_err("ready 之后的取消必须失败");
                let elapsed = started.elapsed();
                let ready_observed = ready_seen_at.get();
                assert!(
                    ready_observed.is_some(),
                    "第 {round} 次：helper 未在测试准备期限 {READY_PREP_DEADLINE:?} 内 ready ⇒ \
                     属于基础设施/协议失败（保留在记录里，本条不构成取消路径的结论）"
                );
                let ready_unix = ready_seam_unix_ms(&ready_file);
                assert!(
                    ready_unix.is_some(),
                    "第 {round} 次：既然观测到 ready，就必须能读到屏障文件的创建时刻"
                );
                assert_eq!(failure.kind(), StrokeFailureKind::Cancelled);
                // **阶段回执必须留下**：取消发生在执行者已经运行之后，它当时确实还活着。
                let facts = failure.facts().expect("ready 之后必须留下阶段回执");
                assert!(
                    facts.injected_steps >= 1,
                    "第 {round} 次：ready 之后 helper 仍在推进：{:?}",
                    facts.injected_steps
                );
                assert!(!facts.completed);
                assert_eq!(
                    facts.released, None,
                    "第 {round} 次：被强杀 ⇒ 没有终态释放记录（回执丢失）"
                );
                // 收尾：强杀 + 静止确认 + 同一个收尾窗口（数值仍是既定的那三个）。
                let cleanup = failure.cleanup().expect("取消必须进入收尾").clone();
                assert!(cleanup.forced_kill, "第 {round} 次：必须强制终止");
                assert!(failure.stillness_confirmed(), "第 {round} 次：必须确认静止");
                assert!(failure.process_exit_confirmed_at_ms().is_some());
                assert!(
                    cleanup.cooperative_exit_waited_ms >= 2_000
                        && cleanup.cooperative_exit_waited_ms <= 2_600,
                    "第 {round} 次：协作退出额度必须是既定的 2 秒（执行者不理会取消）：{:?}",
                    cleanup.cooperative_exit_waited_ms
                );
                assert!(
                    !cleanup.independent_release_issued,
                    "第 {round} 次：SendInput 文本没有释放义务"
                );
                // 取消只可能在 ready 之后发出：屏障 + 时间线两处证据。
                let ready_observed_at = ready_observed.expect("上面已断言 ready 被观测到");
                let ready_delay = ready_unix.expect("上面已断言 ready 时刻存在")
                    .saturating_sub(started_unix);
                assert!(
                    ready_observed_at >= started,
                    "第 {round} 次：ready 观测必须发生在本次运行开始之后"
                );
                let receipt = native_failure_receipt("desktop:text_input:0", &failure);
                receipt.validate().expect("回执必须自洽");
                assert_eq!(receipt.input_delivery, InputDelivery::Sent);
                assert_eq!(receipt.partial, Some(true));
                println!(
                    "[ready 后取消] 第{round}次：启动（预检+Add-Type+执行者建立）={ready_delay}ms／\
                     观测到 ready 后才发取消／协作退出={}ms／强杀={}／静止确认={}／\
                     阶段回执 steps={}／总耗时={:?}",
                    cleanup.cooperative_exit_waited_ms,
                    cleanup.forced_kill,
                    failure.stillness_confirmed(),
                    facts.injected_steps,
                    elapsed
                );
            }
        }

        /// **真实完整 helper 冒烟**：真实 `Add-Type`／启动／执行／结束链在**声明预算**下的表现。
        ///
        /// 注入驱动是**内存驱动**（零注入，不碰真实桌面），其余全是真实链路：真实
        /// `powershell.exe`、真实 `Add-Type` 编译、真实进度文件、真实管道收尾。
        ///
        /// 记录全部阶段时间（宿主的 unix 毫秒与 helper 自己写下的时刻在同一台机器上对齐）：
        /// * 预检+启动：测试起点 → helper 自己写下的 ready 时刻；
        /// * 执行：ready → 终态回执（`reply_received_at_ms`）；
        /// * 结束/静止：回执 → `process_exit_confirmed_at_ms`。
        ///
        /// 三个声明预算超了**就如实失败**；本条**不能**由前两类用例替代。
        #[test]
        #[cfg(windows)]
        fn real_helper_full_chain_smoke_records_every_stage_under_the_declared_budget() {
            let repeats = repeat_count();
            let load = ControlledLoad::spawn_from_env(Duration::from_secs(20 + 20 * repeats as u64));
            println!(
                "[真实冒烟] 重复 {repeats} 次，受控负载进程 {}(RD4_LOAD_PROCS)，后端配置={}",
                load.processes(),
                configured_backend_label()
            );
            for round in 1..=repeats {
                let ready_file = ready_seam_path("smoke");
                let _ = std::fs::remove_file(&ready_file);
                let scenario = seam_scenario("slow_steps", &ready_file, 0);
                let not_cancelled = || false;
                let attempt = attempt_without_identity(&not_cancelled);
                let obligation = text_release_obligation(InputBackend::SendInput);
                // 测试专用：闸门等待在计时之前完成，保证下面记录的是真实阶段时间。
                let _test_pipe_slot = test_pipe_reader_capacity_slot(NATIVE_RUN_READER_UNITS);
                let started_unix = unix_ms();
                let started = Instant::now();
                let outcome = native_run_with_mock(
                    "text",
                    serde_json::json!({ "text": "abc" }),
                    &obligation,
                    &attempt,
                    SMOKE_TOTAL_BUDGET,
                    &scenario,
                )
                .expect("真实完整链路必须在声明预算内成功（预算不适配就如实失败）");
                let elapsed = started.elapsed();
                let ready_unix = ready_seam_unix_ms(&ready_file)
                    .expect("真实 helper 必须留下 ready 阶段时刻");
                let reply_unix = outcome
                    .reply_received_at_ms
                    .expect("必须记录终态回执时刻");
                let exit_unix = outcome
                    .process_exit_confirmed_at_ms
                    .expect("必须记录子进程结束（静止）时刻");
                let startup = Duration::from_millis(ready_unix.saturating_sub(started_unix));
                let execution = Duration::from_millis(reply_unix.saturating_sub(ready_unix));
                let stillness = Duration::from_millis(exit_unix.saturating_sub(reply_unix));
                let facts = outcome.facts().expect("真实 helper 必须写出终态事实");
                assert_eq!(facts.injected_steps, 3);
                assert!(facts.completed);
                assert_eq!(facts.released, Some(true));
                assert!(outcome.reply_received(), "必须记录收到回复");
                assert!(outcome.stillness_confirmed(), "必须记录子进程真正结束");
                println!(
                    "[真实冒烟] 第{round}次：预检+启动={startup:?}（预算 {SMOKE_STARTUP_BUDGET:?}）／\
                     执行={execution:?}（预算 {SMOKE_EXECUTION_BUDGET:?}）／\
                     结束静止={stillness:?}（预算 {SMOKE_STILLNESS_BUDGET:?}）／\
                     总耗时={elapsed:?}（预算 {SMOKE_TOTAL_BUDGET:?}）／ready 标记={}",
                    ready_file.exists()
                );
                assert!(
                    startup <= SMOKE_STARTUP_BUDGET,
                    "第 {round} 次：启动阶段 {startup:?} 超出声明预算 {SMOKE_STARTUP_BUDGET:?}"
                );
                assert!(
                    execution <= SMOKE_EXECUTION_BUDGET,
                    "第 {round} 次：执行阶段 {execution:?} 超出声明预算 {SMOKE_EXECUTION_BUDGET:?}"
                );
                assert!(
                    stillness <= SMOKE_STILLNESS_BUDGET,
                    "第 {round} 次：结束/静止阶段 {stillness:?} 超出声明预算 {SMOKE_STILLNESS_BUDGET:?}"
                );
                assert!(
                    elapsed <= SMOKE_TOTAL_BUDGET,
                    "第 {round} 次：整条链路 {elapsed:?} 超出声明预算 {SMOKE_TOTAL_BUDGET:?}"
                );
            }
        }

        /// 真实受控 helper：**重复取消不得刷新收尾窗口**。
        ///
        /// 取消信号在每一轮循环都为真（重复信号），且执行者不理会取消文件：
        /// 收尾截止时间必须只固定一次，协作退出 + 独立释放共用同一个窗口。
        #[test]
        #[cfg(windows)]
        fn repeated_cancellation_never_refreshes_the_fixed_cleanup_window() {
            let cancelled = || true;
            let attempt = attempt_without_identity(&cancelled);
            // 按住 2000ms：取消信号在 ~0ms 就到来，而执行者不理会取消文件，
            // 因此它会一直活到协作退出额度用尽、被主机强杀——同时"那个键仍按住"。
            let keys = [0x41];
            let obligation = key_release_obligation(&keys);
            // 测试专用：闸门等待必须在计时之前完成（下面有绝对耗时上限）。
            let _test_pipe_slot = test_pipe_reader_capacity_slot(NATIVE_RUN_READER_UNITS);
            let started = Instant::now();
            let failure = native_run_with_mock(
                "key",
                serde_json::json!({"keys": [65_u32], "hold_ms": 2000}),
                &obligation,
                &attempt,
                Duration::from_millis(300),
                "slow_steps",
            )
            .expect_err("取消必须失败");
            let elapsed = started.elapsed();
            assert_eq!(failure.kind(), StrokeFailureKind::Cancelled);
            let cleanup = failure.cleanup().expect("取消必须进入收尾").clone();
            assert_eq!(
                cleanup.stopped_new_input_at_ms, cleanup.cleanup_started_at_ms,
                "第一次进入收尾时就固定了截止时间"
            );
            assert!(
                cleanup.cooperative_exit_waited_ms >= 2_000
                    && cleanup.cooperative_exit_waited_ms <= 2_600,
                "重复信号不得把协作退出额度续期：{:?}",
                cleanup.cooperative_exit_waited_ms
            );
            assert!(cleanup.forced_kill);
            assert!(failure.stillness_confirmed());
            // 登记过义务的被强杀运行：只补发一次独立释放，且等待不超过剩余窗口。
            assert!(cleanup.independent_release_issued);
            assert!(cleanup.independent_release_confirmed);
            assert!(
                cleanup.independent_release_wait_ms <= 2_000,
                "独立释放等待上限 = min(2 秒, 剩余收尾时间)"
            );
            assert!(
                cleanup.cooperative_exit_waited_ms + cleanup.independent_release_wait_ms
                    <= native_cleanup_policy().cleanup_window().as_millis() as u64,
                "协作退出与独立释放共用一个收尾窗口"
            );
            assert_eq!(failure.release_status(), InputReleaseStatus::Released);
            // 收尾的有界性同样由上面的窗口数字钉住；这里只核对整体量级没有失控。
            assert!(
                elapsed <= Duration::from_secs(15),
                "收尾必须有界：{elapsed:?}"
            );
            let receipt = native_failure_receipt("desktop:key_combination:0", &failure);
            receipt.validate().expect("自洽");
            assert_eq!(receipt.input_release, InputReleaseStatus::Released);
        }

        /// 真实受控 helper 的成功路径：**收到回复**与**子进程结束**分别记录。
        #[test]
        #[cfg(windows)]
        fn real_helper_success_reports_reply_and_exit_separately() {
            let cancelled = || false;
            let attempt = attempt_without_identity(&cancelled);
            let obligation = text_release_obligation(InputBackend::SendInput);
            let outcome = native_run_with_mock(
                "text",
                serde_json::json!({"text": "abc"}),
                &obligation,
                &attempt,
                Duration::from_secs(20),
                "slow_steps",
            )
            .expect("内存驱动下的正常完成必须成功");
            let facts = outcome.facts().expect("helper 必须写出终态事实");
            assert_eq!(facts.injected_steps, 3);
            assert!(facts.completed);
            assert_eq!(facts.released, Some(true));
            assert!(outcome.reply_received(), "必须记录收到回复");
            assert!(outcome.stillness_confirmed(), "必须记录子进程真正结束");
            assert_eq!(outcome.obligation.summary(), obligation.summary());
            let receipt = native_success_receipt("desktop:text_input:0", &outcome);
            receipt.validate().expect("成功回执必须自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::Sent);
            assert_eq!(receipt.partial, Some(false));
            assert_eq!(receipt.path_completed, None);
            assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
        }

        /// **确认静止失败**：执行者可能仍在运行 → 失联 + 释放未确认 + 必须隔离。
        ///
        /// 这条路径无法用真实子进程稳定复现（需要 kill/wait 自身失败），因此用生产构造函数
        /// 直接验证判定结果；真实子进程侧由"超时后仍存活""重复取消"两条用例覆盖。
        #[test]
        fn unconfirmed_stillness_is_quarantined_and_never_reported_as_released() {
            let obligation = ReleaseObligation::mouse_buttons(&[MouseButton::Left]);
            let outcome = NativeInputOutcome {
                obligation: obligation.clone(),
                // 测试构造：本用例不涉及实例身份（真实捕获在 run_native_helper 的 spawn 之后）。
                helper_process: None,
                facts: Some(NativeInputFacts {
                    protocol: HELPER_FACT_PROTOCOL_V2,
                    request_id: None,
                    phase: HelperFactPhase::InFlight,
                    cursor_moved: Some(true),
                    injected_steps: 1,
                    completed: false,
                    armed_buttons: vec![MouseButton::Left],
                    armed_keys: Vec::new(),
                    pressed: true,
                    released: None,
                }),
                fact_anomaly: None,
                reply_received_at_ms: Some(1_000),
                process_exit_confirmed_at_ms: None,
                cleanup: Some(crate::cleanup::HelperCleanupFacts {
                    pipe: None,
                    stopped_new_input_at_ms: 1_000,
                    cleanup_started_at_ms: 1_000,
                    cleanup_finished_at_ms: 5_000,
                    cooperative_exit_waited_ms: 2_000,
                    forced_kill: true,
                    independent_release_issued: true,
                    independent_release_wait_ms: 1_500,
                    independent_release_confirmed: false,
                    independent_release_skipped_window_expired: false,
                    release: CleanupReleaseStatus::Unconfirmed,
                }),
            };
            let failure = native_stillness_failure(&obligation, outcome);
            assert_eq!(failure.kind(), StrokeFailureKind::HelperLost);
            assert!(!failure.stillness_confirmed());
            assert!(failure.must_quarantine());
            assert_eq!(failure.release_status(), InputReleaseStatus::Unknown);
            assert_eq!(
                failure.outcome.cleanup_release_status(),
                CleanupReleaseStatus::Unconfirmed
            );
            let receipt = native_failure_receipt("desktop:click:0", &failure);
            receipt.validate().expect("自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
            assert_eq!(receipt.partial, Some(true));
            assert_eq!(receipt.path_completed, None);
            assert_eq!(receipt.confirmed_point_count, None);
            assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
        }

        /// **回执丢失**：helper 没留下任何事实时，回执只能写"可能已发送"，
        /// 既不能推断零输入，也不能凭空写释放结论。
        #[test]
        fn missing_helper_receipt_keeps_unknown_delivery_and_no_invented_release() {
            let obligation = ReleaseObligation::mouse_buttons(&[MouseButton::Left]);
            let outcome = NativeInputOutcome {
                obligation: obligation.clone(),
                // 测试构造：本用例不涉及实例身份（真实捕获在 run_native_helper 的 spawn 之后）。
                helper_process: None,
                facts: None,
                fact_anomaly: None,
                reply_received_at_ms: None,
                process_exit_confirmed_at_ms: Some(unix_ms()),
                cleanup: Some(crate::cleanup::HelperCleanupFacts {
                    pipe: None,
                    stopped_new_input_at_ms: 1_000,
                    cleanup_started_at_ms: 1_000,
                    cleanup_finished_at_ms: 3_000,
                    cooperative_exit_waited_ms: 2_000,
                    forced_kill: true,
                    independent_release_issued: false,
                    independent_release_wait_ms: 0,
                    independent_release_confirmed: false,
                    independent_release_skipped_window_expired: true,
                    release: CleanupReleaseStatus::Unconfirmed,
                }),
            };
            let failure = NativeInputFailure::after_input(
                "helper_lost: 没有留下任何事实".to_string(),
                obligation.clone(),
                outcome,
            );
            assert!(!failure.outcome.reply_received());
            assert!(failure.must_quarantine(), "未知释放必须隔离");
            assert_eq!(failure.release_status(), InputReleaseStatus::Unknown);
            let receipt = native_failure_receipt("desktop:click:0", &failure);
            receipt.validate().expect("自洽");
            assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
            assert_eq!(receipt.partial, None, "未知不得被写成完整或部分");
            assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
            // 前一段已确认的输入不能被这条未知事实抹掉，但释放仍然未知。
            let after_prior = native_failure_receipt_after_confirmed_input(
                "desktop:text_input:0",
                &failure,
                InputReleaseStatus::Released,
            );
            after_prior.validate().expect("自洽");
            assert_eq!(after_prior.input_delivery, InputDelivery::Sent);
            assert_eq!(after_prior.partial, Some(true));
            assert_eq!(after_prior.input_release, InputReleaseStatus::Unknown);
        }
        // ------------------------------------------------------------------
        // CU-F02：CU 层的有界管道收尾（真实子进程 + 持有管道的孙进程）
        // ------------------------------------------------------------------
        //
        // 这里的子进程/孙进程由测试自己构造，因为**固定 helper 协议不提供**
        // "起一个孙进程"的入口；而真实事故里的孙进程（`Add-Type` 的 `csc.exe`）
        // 与这里的 `ping.exe` 扮演同一个角色：继续持有 Rust 给子进程的管道写端。
        // 被测的是**生产的 CU 收尾函数** `helper_pipes::drain`——`run_helper` 与
        // `run_native_helper` 调用的就是它、用的就是同一条收尾截止时间。

        /// 子进程打印的协议记录：孙进程持管道时也必须能读到（不依赖 EOF）。
        const PIPE_RECORD_LINE: &str = "coolzhu-cu-record:stdout:ready";

        /// 孙进程 `ping` 的报文数：约 19 秒的寿命，足够覆盖"收尾不得等它"的断言窗口。
        const PIPE_GRANDCHILD_PING_COUNT: &str = "20";

        /// 真实"子进程退出、孙进程继续持有 stdout/stderr"的构造脚本。
        fn pipe_holder_script(pid_file: &std::path::Path) -> String {
            let path = pid_file.to_string_lossy().replace('\\', "/");
            let start_grandchild = format!(
                "Start-Process -FilePath ping.exe -ArgumentList '-n','{PIPE_GRANDCHILD_PING_COUNT}','127.0.0.1' -NoNewWindow -PassThru | ForEach-Object {{ 'coolzhu-grandchild:' + $_.Id; [System.IO.File]::WriteAllText('{path}', [string]$_.Id) }}"
            );
            [
                "$ErrorActionPreference='Stop'".to_string(),
                "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)".to_string(),
                format!("'{PIPE_RECORD_LINE}'"),
                start_grandchild,
                "'coolzhu-child:done'".to_string(),
                "exit 0".to_string(),
            ]
            .join("\n")
        }

        /// 场景收尾：只终止自己观测到的孙进程（按身份核对，PID 复用会被拒绝）。
        struct GrandchildHolder {
            pid: u32,
            identity: windows_process_guard::ProcessIdentity,
        }

        impl GrandchildHolder {
            fn is_alive(&self) -> bool {
                windows_process_guard::capture_process_identity(self.pid).is_ok()
            }
        }

        impl Drop for GrandchildHolder {
            fn drop(&mut self) {
                let _ = windows_process_guard::terminate_owned_process(self.pid, &self.identity);
            }
        }

        struct PipeFixture {
            child: std::process::Child,
            stdout: Option<HelperPipeReader>,
            stderr: Option<HelperPipeReader>,
            stdout_label: String,
            stderr_label: String,
            grandchild: Option<GrandchildHolder>,
            pid_file: std::path::PathBuf,
        }

        impl PipeFixture {
            fn spawn(tag: &str) -> Self {
                use std::process::{Command, Stdio};
                let pid_file = std::env::temp_dir().join(format!(
                    "coolzhu-cu-pipe-{tag}-{}.pid",
                    std::process::id()
                ));
                let _ = std::fs::remove_file(&pid_file);
                let mut command = Command::new("powershell.exe");
                command
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        &pipe_holder_script(&pid_file),
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = command.spawn().expect("必须能启动 powershell.exe 子进程");
                let stdout_label = format!("cu-{tag}-stdout");
                let stderr_label = format!("cu-{tag}-stderr");
                let stdout = helper_pipes::supervise_stdout(
                    &stdout_label,
                    child.stdout.take().expect("stdout 管道"),
                )
                .expect("stdout 监督器");
                let stderr = helper_pipes::supervise_stderr(
                    &stderr_label,
                    child.stderr.take().expect("stderr 管道"),
                )
                .expect("stderr 监督器");
                let started = Instant::now();
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            assert!(status.success(), "子进程必须正常退出: {status:?}");
                            break;
                        }
                        Ok(None) => {
                            assert!(started.elapsed() < WINDOW_TIMEOUT, "子进程退出超时");
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(error) => panic!("try_wait 失败: {error}"),
                    }
                }
                let mut grandchild = None;
                let deadline = Instant::now() + Duration::from_secs(5);
                while Instant::now() < deadline {
                    if let Ok(raw) = std::fs::read_to_string(&pid_file) {
                        if let Ok(pid) = raw.trim().parse::<u32>() {
                            if let Ok(identity) =
                                windows_process_guard::capture_process_identity(pid)
                            {
                                grandchild = Some(GrandchildHolder { pid, identity });
                                break;
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let grandchild = grandchild.expect("必须先观测到孙进程身份，才能只终止自己起的进程");
                // 子进程已退出：给读取线程一点时间做"结尾观察"。
                std::thread::sleep(Duration::from_millis(400));
                Self {
                    child,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    stdout_label,
                    stderr_label,
                    grandchild: Some(grandchild),
                    pid_file,
                }
            }

            fn stdout(&self) -> &HelperPipeReader {
                self.stdout.as_ref().expect("stdout 监督器未被消费")
            }

            fn take_stdout(&mut self) -> HelperPipeReader {
                self.stdout.take().expect("stdout 监督器只被消费一次")
            }

            fn take_stderr(&mut self) -> HelperPipeReader {
                self.stderr.take().expect("stderr 监督器只被消费一次")
            }

            fn grandchild_alive(&self) -> bool {
                self.grandchild.as_ref().is_some_and(GrandchildHolder::is_alive)
            }

            fn terminate_grandchild(&mut self) {
                if let Some(holder) = self.grandchild.take() {
                    drop(holder);
                }
                std::thread::sleep(Duration::from_millis(500));
            }

            /// 有界闭环：等到该标签的残留归零（并行的其它用例也可能顺手清扫登记）。
            fn wait_for_reclaim(&self) {
                let started = Instant::now();
                while retained_residue(&self.stdout_label, &self.stderr_label) > 0
                    && started.elapsed() < Duration::from_secs(3)
                {
                    let _ = windows_process_guard::reclaim_finished_pipe_readers();
                    std::thread::sleep(Duration::from_millis(50));
                }
                assert_eq!(
                    retained_residue(&self.stdout_label, &self.stderr_label),
                    0,
                    "孙进程退出后，未核实的读取必须被回收"
                );
            }
        }

        /// CU-F02：原生输入（`run_native_helper`）的收尾同样**不得**出现无界等待。
        #[test]
        fn f02_native_helper_cleanup_has_no_unbounded_wait_and_uses_the_bounded_pipe_drain() {
            let source = include_str!("input.rs");
            // 边界取**列 0** 的 `#[cfg(test)]`（测试模块）：函数体里现在也有缩进的
            // 测试专用属性，按裸串切会把函数体截断。
            // 用空格拼接：下面的检查只看片段是否出现，不依赖换行。
            let helper: String = source
                .split("fn run_native_helper(")
                .nth(1)
                .expect("run_native_helper")
                .lines()
                .take_while(|line| !line.starts_with("#[cfg(test)]"))
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                !helper.contains(".join()"),
                "不得对读取线程做无界 join（旧的分离线程 + recv_timeout 同样是失管）"
            );
            assert!(
                !helper.contains("read_to_end"),
                "读取必须由原生监督器有界地进行"
            );
            assert!(
                !helper.contains("collect_process_output"),
                "不得再用'分离线程等 join'的收尾方式"
            );
            let drain_call = "helper_pipes::drain(";
            assert!(helper.contains(drain_call), "必须走共用的有界管道收尾入口");
            assert!(
                helper.contains("pipe: Some(pipe_facts)"),
                "管道收尾事实必须写进收尾报告"
            );
        }

        fn retained_residue(stdout_label: &str, stderr_label: &str) -> usize {
            windows_process_guard::retained_pipe_reader_count_labeled(stdout_label)
                + windows_process_guard::retained_pipe_reader_count_labeled(stderr_label)
        }

        impl Drop for PipeFixture {
            fn drop(&mut self) {
                if let Some(holder) = self.grandchild.take() {
                    drop(holder);
                }
                let _ = std::fs::remove_file(&self.pid_file);
                let _ = self.child.wait();
            }
        }

        /// **CU T11**：子进程退出、孙进程继续持有 stdout/stderr ⇒
        /// CU 不等待孙进程自然退出；收尾有界；已收到的回执不丢；无失管 reader。
        #[test]
        #[cfg(windows)]
        fn t11_cu_pipe_cleanup_is_bounded_and_never_waits_for_a_grandchild() {
            // 测试专用：本用例直接建两个读取器，先过进程级测试闸门。
            let _test_pipe_slot = test_pipe_reader_capacity_slot(2);
            let policy = native_cleanup_policy();
            let mut fixture = PipeFixture::spawn("t11");
            // 回执不依赖 EOF：子进程已退出，孙进程仍在持管道。
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut incremental = String::new();
            while Instant::now() < deadline {
                incremental = helper_pipes::snapshot_text(fixture.stdout());
                if incremental.contains(PIPE_RECORD_LINE) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                incremental.contains(PIPE_RECORD_LINE),
                "已经收到的协议记录必须在没有 EOF 的情况下可读：{incremental:?}"
            );
            let before = fixture.stdout().snapshot().expect("监督器仍在");
            assert!(!before.eof_seen, "孙进程持管道：不得看到 EOF");
            assert!(!before.completion.is_confirmed(), "读取线程当时仍在运行");

            // 生产收尾：与业务期限无关的收尾窗口（这里从零开始 = 刚进入收尾）。
            let cleanup_deadline = CleanupDeadline::establish(policy, 0);
            let started = Instant::now();
            let output = helper_pipes::drain(
                fixture.take_stdout(),
                fixture.take_stderr(),
                Some(cleanup_deadline),
                policy,
            );
            let elapsed = started.elapsed();
            println!(
                "[CU T11] 收尾耗时 {elapsed:?} 事实={:?} stdout={:?}",
                output.facts,
                output.stdout.trim()
            );
            assert!(
                elapsed < Duration::from_secs(2),
                "收尾必须有界（孙进程还有约 19 秒才退）：{elapsed:?}"
            );
            // 无失管 reader：两路读取都**核实**结束，没有残留。
            assert!(
                output.facts.readers_confirmed(),
                "两路读取线程都必须核实结束：{:?}",
                output.facts
            );
            assert!(
                !output.facts.is_supervision_fault(),
                "没有未核实的读取时不得报监督/I/O 故障：{:?}",
                output.facts
            );
            assert_eq!(output.facts.readers_retained, 0);
            assert!(
                !output.facts.has_evidence_gap(),
                "没有截断、没有未核实：不得有证据缺口：{:?}",
                output.facts
            );
            assert!(
                output.stdout.contains(PIPE_RECORD_LINE),
                "已经收到的执行回执不得因为收尾而丢失"
            );
            assert!(
                output
                    .stdout_records
                    .iter()
                    .any(|line| line.contains(PIPE_RECORD_LINE)),
                "完整行协议记录必须能被解析出来（不依赖 EOF）"
            );
            // 关键：收尾结束时孙进程**还活着**——CU 没有等它自然退出。
            assert!(
                fixture.grandchild_alive(),
                "CU 不得等待孙进程自然退出：收尾结束时它必须还活着"
            );
        }

        /// **CU T12**：停止/取消只是请求，**不是**读取结束；管道收尾只消费
        /// **已经固定**的收尾窗口（不刷新、不另领第二套额度），并如实上报完成状态。
        #[test]
        #[cfg(windows)]
        fn t12_cu_pipe_cleanup_consumes_the_fixed_window_and_never_claims_completion() {
            // 测试专用：本用例直接建两个读取器，先过进程级测试闸门。
            let _test_pipe_slot = test_pipe_reader_capacity_slot(2);
            let policy = native_cleanup_policy();
            let mut fixture = PipeFixture::spawn("t12");
            let before = fixture.stdout().snapshot().expect("监督器仍在");
            assert!(
                !before.completion.is_confirmed(),
                "孙进程持管道：读取当时仍在运行"
            );
            // 完成状态只能来自核实：再问一次必须与快照一致（非阻塞检查）。
            assert_eq!(
                before.completion.is_confirmed(),
                fixture.stdout().is_confirmed_finished()
            );

            // 收尾窗口已被前面的阶段消耗到只剩几毫秒：管道收尾**只能**用剩下的额度。
            let established_at = Instant::now() - Duration::from_millis(3_995);
            let cleanup_deadline = CleanupDeadline::establish_at(policy, 0, established_at);
            let remaining = cleanup_deadline.remaining_at(Instant::now());
            assert!(
                remaining <= Duration::from_millis(10),
                "构造前提：剩余窗口必须已经很小：{remaining:?}"
            );
            let started = Instant::now();
            let output = helper_pipes::drain(
                fixture.take_stdout(),
                fixture.take_stderr(),
                Some(cleanup_deadline),
                policy,
            );
            let elapsed = started.elapsed();
            println!(
                "[CU T12] 剩余窗口 {remaining:?} ⇒ 实际收尾 {elapsed:?} 事实={:?}",
                output.facts
            );
            assert!(
                output.facts.waited_ms <= 100,
                "管道收尾只能消费剩余窗口，不得另领一个完整诊断片长：{:?}",
                output.facts
            );
            assert!(
                elapsed <= Duration::from_millis(400),
                "窗口很小时收尾必须立刻返回：{elapsed:?}"
            );
            // 未核实就必须如实报"未核实"：既不宣称已结束，也不丢掉已经收到的字节。
            if output.facts.readers_confirmed() {
                println!("[CU T12] 证据：读取在窗口内核实结束 ⇒ 如实报已结束");
                assert!(!output.facts.has_evidence_gap());
                assert_eq!(output.facts.readers_retained, 0);
            } else {
                assert!(
                    output.facts.has_evidence_gap(),
                    "未核实结束必须记成证据缺口：{:?}",
                    output.facts
                );
                assert!(
                    output.facts.is_supervision_fault(),
                    "未核实结束必须记成监督/I/O 故障：{:?}",
                    output.facts
                );
                assert!(
                    output.facts.readers_retained >= 1,
                    "未核实的读取资源必须仍由监督器持有：{:?}",
                    output.facts
                );
                println!("[CU T12] 证据：窗口耗尽后读取未核实 ⇒ 如实上报，未宣称已结束");
            }
            assert!(fixture.grandchild_alive(), "收尾不得等待孙进程");

            // 重复的停止/取消信号不得刷新窗口：截止时间由 `CleanupDeadline::fixed` 固定一次。
            let mut slot = Some(cleanup_deadline);
            let first = CleanupDeadline::fixed(&mut slot, policy, 0);
            let again = CleanupDeadline::fixed(&mut slot, policy, 9_999);
            assert_eq!(
                first.deadline_at(),
                again.deadline_at(),
                "重复信号不得把收尾窗口续期"
            );
            assert_eq!(again.started_at_unix_ms(), 0);

            // 闭环：孙进程退出后由非阻塞清扫回收残留。
            fixture.terminate_grandchild();
            fixture.wait_for_reclaim();
        }
    }
}
