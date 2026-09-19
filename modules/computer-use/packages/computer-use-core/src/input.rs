use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::Serialize;

#[path = "input_stroke.rs"]
mod stroke;
pub use stroke::{capture_window_image, controlled_drag_path, validate_stroke, StrokeWindow};

const ENV_INPUT_BACKEND: &str = "CLAW_MOUSE_BACKEND";
const ENV_INTERCEPTION_DLL_PATH: &str = "CLAW_INTERCEPTION_DLL_PATH";
const ENV_INTERCEPTION_MOUSE_DEVICE_ID: &str = "CLAW_INTERCEPTION_MOUSE_DEVICE_ID";
const ENV_INTERCEPTION_KEYBOARD_DEVICE_ID: &str = "CLAW_INTERCEPTION_KEYBOARD_DEVICE_ID";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputBackend {
    SendInput,
    Interception,
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

pub fn click_point(x: i32, y: i32, clicks: u32, timeout: Duration) -> Result<(), String> {
    if clicks > 1 {
        return mouse_button_action_point(x, y, MouseButtonAction::DoubleClick, timeout);
    }
    mouse_button_action_point(x, y, MouseButtonAction::LeftClick, timeout)
}

pub fn mouse_button_action_point(
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

pub fn move_mouse_relative(dx: i32, dy: i32, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_move_mouse_relative(dx, dy, timeout),
        InputBackend::Interception => interception_move_mouse_relative(dx, dy, timeout),
    }
}

pub fn move_mouse_absolute(x: i32, y: i32, timeout: Duration) -> Result<(), String> {
    sendinput_move_mouse_absolute(x, y, timeout)
}

pub fn mouse_button_down_point(
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

pub fn mouse_button_up_point(
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

pub fn drag_point(
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

pub fn press_escape(timeout: Duration) -> Result<(), String> {
    press_virtual_key(0x1B, timeout)
}

pub fn scroll_wheel(delta: i32, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_scroll_wheel(delta, timeout),
        InputBackend::Interception => interception_scroll_wheel(delta, timeout),
    }
}

pub fn type_text(text: &str, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_type_text(text, timeout),
        InputBackend::Interception => interception_type_text(text, timeout),
    }
}

pub fn press_virtual_key(virtual_key: u8, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_press_virtual_key(virtual_key, timeout),
        InputBackend::Interception => interception_press_virtual_key(virtual_key, 70, timeout),
    }
}

pub fn hold_virtual_key(virtual_key: u8, hold_ms: u64, timeout: Duration) -> Result<(), String> {
    match active_backend() {
        InputBackend::SendInput => sendinput_hold_virtual_key(virtual_key, hold_ms, timeout),
        InputBackend::Interception => interception_press_virtual_key(virtual_key, hold_ms, timeout),
    }
}

pub fn send_virtual_key_combo(virtual_keys: &[u8], timeout: Duration) -> Result<(), String> {
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
    let script=format!("$ErrorActionPreference='Stop'; Add-Type -TypeDefinition @'\n{CLICK_NATIVE}\n'@\n{}",sendinput_click_body(x,y,action));
    run_powershell(&script,timeout).map(|_| ())
}

const CLICK_NATIVE:&str=r#"using System;
using System.Runtime.InteropServices;
public struct INPUT { public uint type; public MOUSEINPUT mi; }
public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }
public static class MouseOps {
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern uint SendInput(uint count,INPUT[] inputs,int size);
}"#;

fn sendinput_click_body(x:i32,y:i32,action:MouseButtonAction)->String {
    format!(r#"
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
"#,sequence=sendinput_mouse_button_sequence(action))
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

#[cfg(test)]
mod tests {
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
        use super::{sendinput_click_body,run_powershell,MouseButtonAction};
        use std::time::Duration;
        // 固定生产 PowerShell 点击体配纯内存 MouseOps，不导入 user32 或发送真实输入。
        let mock=r#"using System; using System.Collections.Generic;
public struct INPUT { public uint type; public MOUSEINPUT mi; }
public struct MOUSEINPUT { public int dx,dy; public uint mouseData,dwFlags,time; public UIntPtr dwExtraInfo; }
public static class MouseOps {
 public static int Mode; public static List<uint> Flags=new List<uint>();
 public static bool SetCursorPos(int x,int y){ return Mode!=1; }
 public static uint SendInput(uint n,INPUT[] p,int size){uint f=p[0].mi.dwFlags;Flags.Add(f);if((Mode==2&&f==2)||(Mode==3&&f==4))return 0;return 1;}
}"#;
        let body=sendinput_click_body(10,20,MouseButtonAction::LeftClick);
        let script=format!(r#"$ErrorActionPreference='Stop'; Add-Type -TypeDefinition @'
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
"#);
        assert_eq!(run_powershell(&script,Duration::from_secs(15)).unwrap().trim(),"click-native-checks:ok");
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
}
