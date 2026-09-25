use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::de::DeserializeOwned;
use serde::Deserialize;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorScope {
    Taskbar,
    Window,
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAnchor {
    pub scope: AnchorScope,
    pub label: String,
    pub role: String,
    pub window_title: Option<String>,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl UiAnchor {
    #[must_use]
    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let (x, y) = self.center();
        match &self.window_title {
            Some(window_title) if !window_title.trim().is_empty() => {
                format!(
                    "- [{}] {} @ ({x}, {y}) in {}",
                    self.role, self.label, window_title
                )
            }
            _ => format!("- [{}] {} @ ({x}, {y})", self.role, self.label),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PointHitInfo {
    pub class_name: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnchorInventory {
    pub taskbar: Vec<UiAnchor>,
    pub window: Vec<UiAnchor>,
    pub desktop: Vec<UiAnchor>,
}

impl AnchorInventory {
    #[must_use]
    pub fn prompt_block(&self) -> String {
        let taskbar = if self.taskbar.is_empty() {
            "- none".to_string()
        } else {
            self.taskbar
                .iter()
                .take(8)
                .map(UiAnchor::summary)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let window = if self.window.is_empty() {
            "- none".to_string()
        } else {
            self.window
                .iter()
                .take(12)
                .map(UiAnchor::summary)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let desktop = if self.desktop.is_empty() {
            "- none".to_string()
        } else {
            self.desktop
                .iter()
                .take(10)
                .map(UiAnchor::summary)
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            "Precise local taskbar anchors:\n{taskbar}\n\
Precise local window anchors:\n{window}\n\
Precise local desktop anchors:\n{desktop}"
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawAnchor {
    name: String,
    role: String,
    #[serde(default)]
    window: Option<String>,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

pub fn probe_anchor_inventory(target_window: Option<&str>) -> Result<AnchorInventory, String> {
    let mut taskbar = probe_taskbar_anchors()?;
    taskbar.retain(|anchor| anchor_accepts_mouse(anchor).unwrap_or(false));
    let mut desktop = probe_desktop_anchors()?;
    desktop.retain(|anchor| anchor_accepts_mouse(anchor).unwrap_or(false));

    Ok(AnchorInventory {
        taskbar,
        window: probe_window_anchors(target_window)?,
        desktop,
    })
}

pub fn resolve_anchor(
    scope: AnchorScope,
    label: &str,
    target_window: Option<&str>,
) -> Result<UiAnchor, String> {
    let mut anchors = match scope {
        AnchorScope::Taskbar => probe_taskbar_anchors()?,
        AnchorScope::Window => probe_window_anchors(target_window)?,
        AnchorScope::Desktop => probe_desktop_anchors()?,
    };

    if matches!(scope, AnchorScope::Taskbar | AnchorScope::Desktop) {
        anchors.retain(|anchor| anchor_accepts_mouse(anchor).unwrap_or(false));
    }

    find_best_anchor(&anchors, label, target_window).ok_or_else(|| {
        let scope_name = match scope {
            AnchorScope::Taskbar => "taskbar",
            AnchorScope::Window => "window",
            AnchorScope::Desktop => "desktop",
        };
        format!(
            "{scope_name} anchor not found for: {label}; available={}",
            anchors
                .iter()
                .take(16)
                .map(|anchor| anchor.label.as_str())
                .collect::<Vec<_>>()
                .join(" | ")
        )
    })
}

pub fn anchor_accepts_mouse(anchor: &UiAnchor) -> Result<bool, String> {
    let (x, y) = anchor.center();
    let hit = probe_window_at_point(x, y)?;
    match anchor.scope {
        AnchorScope::Taskbar => {
            return Ok(hit.class_name.eq_ignore_ascii_case("Shell_TrayWnd")
                || hit.class_name.contains("Shell")
                || hit.class_name.contains("Tray"));
        }
        AnchorScope::Window => {
            if hit
                .class_name
                .eq_ignore_ascii_case("LockScreenBackstopFrame")
                || hit.class_name.contains("CoreWindow")
                || hit.title.contains("閿佸睆")
            {
                return Ok(false);
            }
        }
        AnchorScope::Desktop => {
            return Ok(hit.class_name.eq_ignore_ascii_case("SysListView32")
                || hit.class_name.eq_ignore_ascii_case("SHELLDLL_DefView")
                || hit.class_name.eq_ignore_ascii_case("Progman")
                || hit.class_name.eq_ignore_ascii_case("WorkerW"));
        }
    }

    Ok(!hit.class_name.trim().is_empty())
}

#[must_use]
pub fn parse_anchor_scope(value: Option<&str>) -> Option<AnchorScope> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "taskbar" => Some(AnchorScope::Taskbar),
        "window" => Some(AnchorScope::Window),
        "desktop" => Some(AnchorScope::Desktop),
        _ => None,
    }
}

fn probe_taskbar_anchors() -> Result<Vec<UiAnchor>, String> {
    let script = r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$taskbars = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ClassNameProperty,
        'Shell_TrayWnd'
    ))
)
$results = @()
foreach ($taskbar in $taskbars) {
    try {
        $buttons = $taskbar.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            (New-Object System.Windows.Automation.PropertyCondition(
                [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                [System.Windows.Automation.ControlType]::Button
            ))
        )
        foreach ($button in $buttons) {
            $name = $button.Current.Name
            $rect = $button.Current.BoundingRectangle
            if ([string]::IsNullOrWhiteSpace($name)) { continue }
            if ($rect.Right -le $rect.Left -or $rect.Bottom -le $rect.Top) { continue }
            $results += [pscustomobject]@{
                name = $name
                role = 'taskbar_button'
                window = $null
                left = [int]$rect.Left
                top = [int]$rect.Top
                right = [int]$rect.Right
                bottom = [int]$rect.Bottom
            }
        }
    } catch {}
}
$results | Select-Object -First 20 | ConvertTo-Json -Compress
"#;

    let raw = run_powershell(script, Duration::from_secs(8))?;
    parse_anchor_output(&raw, AnchorScope::Taskbar)
}

fn probe_window_anchors(target_window: Option<&str>) -> Result<Vec<UiAnchor>, String> {
    let title = target_window.unwrap_or_default().replace('\'', "''");
    let script = format!(
        r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$signature = @'
using System;
using System.Runtime.InteropServices;
public static class WinOps {{
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
}}
'@
Add-Type $signature
$root = [System.Windows.Automation.AutomationElement]::RootElement
$window = $null
$windows = $root.FindAll(
    [System.Windows.Automation.TreeScope]::Children,
    (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Window
    ))
)
if (-not [string]::IsNullOrWhiteSpace('{title}')) {{
    foreach ($candidate in $windows) {{
        try {{
            if ($candidate.Current.Name -like '*{title}*') {{
                $window = $candidate
                break
            }}
        }} catch {{}}
    }}
}}
if ($null -eq $window) {{
    $foreground = [int][WinOps]::GetForegroundWindow()
    foreach ($candidate in $windows) {{
        try {{
            if ($candidate.Current.NativeWindowHandle -eq $foreground) {{
                $window = $candidate
                break
            }}
        }} catch {{}}
    }}
}}
if ($null -eq $window) {{
    @() | ConvertTo-Json -Compress
    exit 0
}}
$controlTypes = @(
    [System.Windows.Automation.ControlType]::Button,
    [System.Windows.Automation.ControlType]::TreeItem,
    [System.Windows.Automation.ControlType]::MenuItem,
    [System.Windows.Automation.ControlType]::ListItem,
    [System.Windows.Automation.ControlType]::TabItem,
    [System.Windows.Automation.ControlType]::SplitButton,
    [System.Windows.Automation.ControlType]::RadioButton,
    [System.Windows.Automation.ControlType]::Edit,
    [System.Windows.Automation.ControlType]::Hyperlink,
    [System.Windows.Automation.ControlType]::Text
)
$results = @()
foreach ($controlType in $controlTypes) {{
    $items = $window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            $controlType
        ))
    )
    foreach ($item in $items) {{
        try {{
            $name = $item.Current.Name
            $rect = $item.Current.BoundingRectangle
            if ([string]::IsNullOrWhiteSpace($name)) {{ continue }}
            if ($rect.Right -le $rect.Left -or $rect.Bottom -le $rect.Top) {{ continue }}
            $results += [pscustomobject]@{{
                name = $name
                role = $item.Current.ControlType.ProgrammaticName
                window = $window.Current.Name
                left = [int]$rect.Left
                top = [int]$rect.Top
                right = [int]$rect.Right
                bottom = [int]$rect.Bottom
            }}
        }} catch {{}}
    }}
}}
$results | Select-Object -First 40 | ConvertTo-Json -Compress
"#
    );

    let raw = run_powershell(&script, Duration::from_secs(10))?;
    parse_anchor_output(&raw, AnchorScope::Window)
}

fn probe_desktop_anchors() -> Result<Vec<UiAnchor>, String> {
    let script = r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NativeDesktop {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct LVITEMW {
    public uint mask;
    public int iItem;
    public int iSubItem;
    public uint state;
    public uint stateMask;
    public IntPtr pszText;
    public int cchTextMax;
    public int iImage;
    public IntPtr lParam;
    public int iIndent;
    public int iGroupId;
    public uint cColumns;
    public IntPtr puColumns;
    public IntPtr piColFmt;
    public int iGroup;
  }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr childAfter, string className, string windowTitle);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr hObject);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr VirtualAllocEx(IntPtr hProcess, IntPtr address, UIntPtr size, uint allocType, uint protect);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool VirtualFreeEx(IntPtr hProcess, IntPtr address, UIntPtr size, uint freeType);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadProcessMemory(IntPtr hProcess, IntPtr address, byte[] buffer, int size, out IntPtr read);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool WriteProcessMemory(IntPtr hProcess, IntPtr address, byte[] buffer, int size, out IntPtr written);
}
"@
$LVM_GETITEMCOUNT = 0x1004
$LVM_GETITEMRECT = 0x100E
$LVM_GETITEMTEXTW = 0x1073
$LVIR_ICON = 1
$LVIF_TEXT = 0x0001
$PROCESS_ACCESS = 0x0438
$MEM_COMMIT = 0x1000
$MEM_RESERVE = 0x2000
$MEM_RELEASE = 0x8000
$PAGE_READWRITE = 0x04
$progman = [NativeDesktop]::FindWindow('Progman', 'Program Manager')
if ($progman -eq [IntPtr]::Zero) {
    @() | ConvertTo-Json -Compress
    exit 0
}
$defView = [NativeDesktop]::FindWindowEx($progman, [IntPtr]::Zero, 'SHELLDLL_DefView', $null)
if ($defView -eq [IntPtr]::Zero) {
    @() | ConvertTo-Json -Compress
    exit 0
}
$list = [NativeDesktop]::FindWindowEx($defView, [IntPtr]::Zero, 'SysListView32', 'FolderView')
if ($list -eq [IntPtr]::Zero) {
    @() | ConvertTo-Json -Compress
    exit 0
}
$processId = [uint32]0
[void][NativeDesktop]::GetWindowThreadProcessId($list, [ref]$processId)
$process = [NativeDesktop]::OpenProcess($PROCESS_ACCESS, $false, $processId)
if ($process -eq [IntPtr]::Zero) {
    @() | ConvertTo-Json -Compress
    exit 0
}
$remoteText = [IntPtr]::Zero
$remoteItem = [IntPtr]::Zero
$remoteRect = [IntPtr]::Zero
try {
    $count = [NativeDesktop]::SendMessage($list, $LVM_GETITEMCOUNT, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
    $lvItemSize = [Runtime.InteropServices.Marshal]::SizeOf([type][NativeDesktop+LVITEMW])
    $rectSize = [Runtime.InteropServices.Marshal]::SizeOf([type][NativeDesktop+RECT])
    $remoteText = [NativeDesktop]::VirtualAllocEx($process, [IntPtr]::Zero, [UIntPtr]::new(1024), $MEM_COMMIT -bor $MEM_RESERVE, $PAGE_READWRITE)
    $remoteItem = [NativeDesktop]::VirtualAllocEx($process, [IntPtr]::Zero, [UIntPtr]::new([uint64]$lvItemSize), $MEM_COMMIT -bor $MEM_RESERVE, $PAGE_READWRITE)
    $remoteRect = [NativeDesktop]::VirtualAllocEx($process, [IntPtr]::Zero, [UIntPtr]::new([uint64]$rectSize), $MEM_COMMIT -bor $MEM_RESERVE, $PAGE_READWRITE)
    if ($remoteText -eq [IntPtr]::Zero -or $remoteItem -eq [IntPtr]::Zero -or $remoteRect -eq [IntPtr]::Zero) {
        @() | ConvertTo-Json -Compress
        exit 0
    }
    $results = @()
    for ($i = 0; $i -lt $count; $i++) {
        $lv = New-Object NativeDesktop+LVITEMW
        $lv.mask = $LVIF_TEXT
        $lv.iItem = $i
        $lv.iSubItem = 0
        $lv.pszText = $remoteText
        $lv.cchTextMax = 260
        $lvPtr = [Runtime.InteropServices.Marshal]::AllocHGlobal($lvItemSize)
        try {
            [Runtime.InteropServices.Marshal]::StructureToPtr($lv, $lvPtr, $false)
            $lvBytes = New-Object byte[] $lvItemSize
            [Runtime.InteropServices.Marshal]::Copy($lvPtr, $lvBytes, 0, $lvItemSize)
            $written = [IntPtr]::Zero
            [void][NativeDesktop]::WriteProcessMemory($process, $remoteItem, $lvBytes, $lvBytes.Length, [ref]$written)
        } finally {
            [Runtime.InteropServices.Marshal]::FreeHGlobal($lvPtr)
        }

        [void][NativeDesktop]::SendMessage($list, $LVM_GETITEMTEXTW, [IntPtr]$i, $remoteItem)
        $textBytes = New-Object byte[] 1024
        $read = [IntPtr]::Zero
        [void][NativeDesktop]::ReadProcessMemory($process, $remoteText, $textBytes, $textBytes.Length, [ref]$read)
        $name = [Text.Encoding]::Unicode.GetString($textBytes).Split([char]0)[0]
        if ([string]::IsNullOrWhiteSpace($name)) { continue }

        $rect = New-Object NativeDesktop+RECT
        $rect.Left = $LVIR_ICON
        $rectPtr = [Runtime.InteropServices.Marshal]::AllocHGlobal($rectSize)
        try {
            [Runtime.InteropServices.Marshal]::StructureToPtr($rect, $rectPtr, $false)
            $rectBytes = New-Object byte[] $rectSize
            [Runtime.InteropServices.Marshal]::Copy($rectPtr, $rectBytes, 0, $rectSize)
            $written = [IntPtr]::Zero
            [void][NativeDesktop]::WriteProcessMemory($process, $remoteRect, $rectBytes, $rectBytes.Length, [ref]$written)
        } finally {
            [Runtime.InteropServices.Marshal]::FreeHGlobal($rectPtr)
        }

        $ok = [NativeDesktop]::SendMessage($list, $LVM_GETITEMRECT, [IntPtr]$i, $remoteRect)
        if ($ok -eq [IntPtr]::Zero) { continue }
        $rectOut = New-Object byte[] $rectSize
        $read = [IntPtr]::Zero
        [void][NativeDesktop]::ReadProcessMemory($process, $remoteRect, $rectOut, $rectOut.Length, [ref]$read)
        $outPtr = [Runtime.InteropServices.Marshal]::AllocHGlobal($rectSize)
        try {
            [Runtime.InteropServices.Marshal]::Copy($rectOut, 0, $outPtr, $rectSize)
            $bounds = [Runtime.InteropServices.Marshal]::PtrToStructure($outPtr, [type][NativeDesktop+RECT])
        } finally {
            [Runtime.InteropServices.Marshal]::FreeHGlobal($outPtr)
        }

        $topLeft = New-Object NativeDesktop+POINT
        $topLeft.X = $bounds.Left
        $topLeft.Y = $bounds.Top
        $bottomRight = New-Object NativeDesktop+POINT
        $bottomRight.X = $bounds.Right
        $bottomRight.Y = $bounds.Bottom
        [void][NativeDesktop]::ClientToScreen($list, [ref]$topLeft)
        [void][NativeDesktop]::ClientToScreen($list, [ref]$bottomRight)

        $results += [pscustomobject]@{
            name = $name
            role = 'desktop_icon'
            window = $null
            left = [int]$topLeft.X
            top = [int]$topLeft.Y
            right = [int]$bottomRight.X
            bottom = [int]$bottomRight.Y
        }
    }

    $results | Select-Object -First 24 | ConvertTo-Json -Compress
} finally {
    if ($remoteText -ne [IntPtr]::Zero) { [void][NativeDesktop]::VirtualFreeEx($process, $remoteText, [UIntPtr]::Zero, $MEM_RELEASE) }
    if ($remoteItem -ne [IntPtr]::Zero) { [void][NativeDesktop]::VirtualFreeEx($process, $remoteItem, [UIntPtr]::Zero, $MEM_RELEASE) }
    if ($remoteRect -ne [IntPtr]::Zero) { [void][NativeDesktop]::VirtualFreeEx($process, $remoteRect, [UIntPtr]::Zero, $MEM_RELEASE) }
    if ($process -ne [IntPtr]::Zero) { [void][NativeDesktop]::CloseHandle($process) }
}
"#;

    let raw = run_powershell(script, Duration::from_secs(8))?;
    parse_anchor_output(&raw, AnchorScope::Desktop)
}

fn probe_window_at_point(x: i32, y: i32) -> Result<PointHitInfo, String> {
    let script = format!(
        r#"
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public struct POINT {{ public int X; public int Y; }}
public static class HitProbe {{
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT pt);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder className, int maxCount);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
}}
'@
$pt = New-Object POINT
$pt.X = {x}
$pt.Y = {y}
$hwnd = [HitProbe]::WindowFromPoint($pt)
$className = New-Object System.Text.StringBuilder 256
$title = New-Object System.Text.StringBuilder 512
[void][HitProbe]::GetClassName($hwnd, $className, $className.Capacity)
[void][HitProbe]::GetWindowText($hwnd, $title, $title.Capacity)
[pscustomobject]@{{ class_name = $className.ToString(); title = $title.ToString() }} | ConvertTo-Json -Compress
"#
    );

    let raw = run_powershell(&script, Duration::from_secs(4))?;
    let mut list = parse_json_list::<PointHitInfo>(&raw)?;
    Ok(list.pop().unwrap_or(PointHitInfo {
        class_name: String::new(),
        title: String::new(),
    }))
}

fn parse_anchor_output(raw: &str, scope: AnchorScope) -> Result<Vec<UiAnchor>, String> {
    let anchors = parse_json_list::<RawAnchor>(raw)?;
    Ok(anchors
        .into_iter()
        .filter(|anchor| anchor.right > anchor.left && anchor.bottom > anchor.top)
        .map(|anchor| UiAnchor {
            scope,
            label: anchor.name.trim().to_string(),
            role: anchor.role.trim().to_string(),
            window_title: anchor.window.and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            left: anchor.left,
            top: anchor.top,
            right: anchor.right,
            bottom: anchor.bottom,
        })
        .collect())
}

fn parse_json_list<T>(raw: &str) -> Result<Vec<T>, String>
where
    T: DeserializeOwned,
{
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| format!("failed to parse anchor json: {error}"))?;
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| {
                serde_json::from_value(item)
                    .map_err(|error| format!("failed to decode anchor row: {error}"))
            })
            .collect(),
        other => Ok(vec![serde_json::from_value(other).map_err(|error| {
            format!("failed to decode anchor payload: {error}")
        })?]),
    }
}

fn find_best_anchor(
    anchors: &[UiAnchor],
    label: &str,
    target_window: Option<&str>,
) -> Option<UiAnchor> {
    let variants = label_variants(label);
    let target_window = target_window.map(normalize_label);

    anchors
        .iter()
        .filter_map(|anchor| {
            let anchor_label = normalize_label(&anchor.label);
            let match_score = variants.iter().find_map(|variant| {
                if anchor_label == *variant {
                    Some(0_u8)
                } else if anchor_label.contains(variant) || variant.contains(&anchor_label) {
                    Some(1_u8)
                } else {
                    None
                }
            })?;
            let window_penalty = match (&target_window, &anchor.window_title) {
                (Some(target), Some(window_title))
                    if normalize_label(window_title).contains(target) =>
                {
                    0_u8
                }
                (Some(_), _) => 1_u8,
                (None, _) => 0_u8,
            };
            Some((
                window_penalty,
                match_score,
                anchor_role_priority(&anchor.role),
                anchor_area(anchor),
                anchor.clone(),
            ))
        })
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
                .then(left.3.cmp(&right.3))
        })
        .map(|candidate| candidate.4)
}

fn anchor_role_priority(role: &str) -> u8 {
    let normalized = normalize_label(role);
    if normalized.contains("button")
        || normalized.contains("treeitem")
        || normalized.contains("listitem")
        || normalized.contains("menuitem")
        || normalized.contains("tabitem")
        || normalized.contains("splitbutton")
        || normalized.contains("radiobutton")
        || normalized.contains("taskbarbutton")
    {
        0
    } else if normalized.contains("edit") || normalized.contains("hyperlink") {
        1
    } else {
        2
    }
}

fn anchor_area(anchor: &UiAnchor) -> i64 {
    i64::from(anchor.right - anchor.left) * i64::from(anchor.bottom - anchor.top)
}

fn label_variants(label: &str) -> Vec<String> {
    let normalized = normalize_label(label);
    let mut variants = vec![normalized.clone()];

    if contains_any(
        &normalized,
        &["此电脑", "我的电脑", "thispc", "this pc", "computer"],
    ) {
        variants.extend(
            ["此电脑", "我的电脑", "thispc", "this pc", "电脑"]
                .into_iter()
                .map(normalize_label),
        );
    }

    if contains_any(
        &normalized,
        &["文件资源管理器", "资源管理器", "explorer", "file explorer"],
    ) {
        variants.extend(
            ["文件资源管理器", "资源管理器", "explorer", "file explorer"]
                .into_iter()
                .map(normalize_label),
        );
    }

    if contains_any(&normalized, &["关闭", "close"]) {
        variants.extend(
            ["关闭", "close", "关闭标签页", "关闭按钮"]
                .into_iter()
                .map(normalize_label),
        );
    }

    variants.sort();
    variants.dedup();
    variants
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|needle| haystack.contains(&normalize_label(needle)))
}

fn normalize_label(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            !ch.is_whitespace()
                && !matches!(ch, '-' | '_' | ':' | ',' | '，' | '"' | '\'' | '“' | '”')
        })
        .flat_map(char::to_lowercase)
        .collect::<String>()
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
            .map_err(|error| format!("failed to run anchor probe PowerShell: {error}"));
        let _ = tx.send(result);
    });

    let output = rx
        .recv_timeout(timeout)
        .map_err(|_| format!("anchor probe timed out after {}s", timeout.as_secs()))?
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("anchor probe PowerShell failed: {stdout} {stderr}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{find_best_anchor, label_variants, normalize_label, AnchorScope, UiAnchor};

    #[test]
    fn normalizes_labels_for_fuzzy_matching() {
        assert_eq!(normalize_label("This PC"), "thispc");
        assert_eq!(
            normalize_label("文件资源管理器 - 已固定"),
            "文件资源管理器已固定"
        );
    }

    #[test]
    fn expands_common_anchor_variants() {
        let variants = label_variants("打开我的电脑");
        assert!(variants.contains(&"此电脑".to_string()));
        assert!(variants.contains(&"thispc".to_string()));
    }

    #[test]
    fn prefers_exact_anchor_match() {
        let anchors = vec![
            UiAnchor {
                scope: AnchorScope::Window,
                label: "刷新“此电脑”(F5)".to_string(),
                role: "ControlType.Button".to_string(),
                window_title: Some("此电脑 - 文件资源管理器".to_string()),
                left: 0,
                top: 0,
                right: 40,
                bottom: 40,
            },
            UiAnchor {
                scope: AnchorScope::Window,
                label: "此电脑".to_string(),
                role: "ControlType.TabItem".to_string(),
                window_title: Some("此电脑 - 文件资源管理器".to_string()),
                left: 0,
                top: 0,
                right: 200,
                bottom: 40,
            },
        ];

        let matched =
            find_best_anchor(&anchors, "此电脑", Some("文件资源管理器")).expect("expected anchor");
        assert_eq!(matched.label, "此电脑");
    }
}
