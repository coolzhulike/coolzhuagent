use vision::locate::{BBoxPx, SystemControlId};

fn decode_console_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let (cow, _, had_errors) = encoding_rs::GB18030.decode(bytes);
    if had_errors {
        String::from_utf8_lossy(bytes).to_string()
    } else {
        cow.to_string()
    }
}

#[cfg(windows)]
mod windows_impl;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiaQuery {
    pub process_id: Option<u32>,
    pub window_name: Option<String>,
    pub element_name: Option<String>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub control_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiaElementSnapshot {
    pub reference: String,
    pub process_id: u32,
    pub native_window_handle: isize,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub control_type: String,
    pub value: Option<String>,
    pub bounding_rect: BBoxPx,
    pub is_offscreen: bool,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiaWindowSnapshot {
    pub process_id: u32,
    pub native_window_handle: isize,
    pub name: Option<String>,
    pub bounding_rect: BBoxPx,
    pub dpi: u32,
    pub elements: Vec<UiaElementSnapshot>,
}

#[derive(Debug, Clone)]
pub struct UiaHit {
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub name: Option<String>,
    pub control_type: String,
    pub bounding_rect: BBoxPx,
    pub is_offscreen: bool,
    pub is_enabled: bool,
    pub confidence: f32,
}

#[derive(Debug)]
pub enum UiaError {
    UnsupportedPlatform,
    ComInitFailed(String),
    ElementNotFound,
    ElementAmbiguous,
    QueryError(String),
}

impl std::fmt::Display for UiaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(f, "UIA only supported on Windows"),
            Self::ComInitFailed(e) => write!(f, "UIA init: {e}"),
            Self::ElementNotFound => write!(f, "element not found"),
            Self::ElementAmbiguous => write!(f, "element query is ambiguous"),
            Self::QueryError(e) => write!(f, "query: {e}"),
        }
    }
}

pub fn snapshot_foreground_window(limit: usize) -> Result<UiaWindowSnapshot, UiaError> {
    #[cfg(windows)]
    {
        windows_impl::snapshot_foreground_window_impl(limit)
    }
    #[cfg(not(windows))]
    {
        let _ = limit;
        Err(UiaError::UnsupportedPlatform)
    }
}

pub fn focus_window(native_window_handle: isize) -> Result<(), UiaError> {
    #[cfg(windows)]
    {
        windows_impl::focus_window_impl(native_window_handle)
    }
    #[cfg(not(windows))]
    {
        let _ = native_window_handle;
        Err(UiaError::UnsupportedPlatform)
    }
}

pub fn focus_window_by_hint(
    application: Option<&str>,
    window: Option<&str>,
    objective: Option<&str>,
) -> Result<Option<isize>, UiaError> {
    #[cfg(windows)]
    {
        windows_impl::focus_window_by_hint_impl(application, window, objective)
    }
    #[cfg(not(windows))]
    {
        let _ = (application, window, objective);
        Err(UiaError::UnsupportedPlatform)
    }
}

pub fn resolve_query(
    snapshot: &UiaWindowSnapshot,
    query: &UiaQuery,
) -> Result<UiaElementSnapshot, UiaError> {
    if query
        .process_id
        .is_some_and(|process_id| process_id != snapshot.process_id)
        || query.window_name.as_ref().is_some_and(|name| {
            snapshot
                .name
                .as_deref()
                .is_none_or(|actual| !actual.contains(name))
        })
    {
        return Err(UiaError::ElementNotFound);
    }
    let mut matches = snapshot.elements.iter().filter(|element| {
        element.is_enabled
            && !element.is_offscreen
            && element.bounding_rect.width > 0
            && element.bounding_rect.height > 0
            && query.element_name.as_ref().is_none_or(|name| {
                element
                    .name
                    .as_deref()
                    .is_some_and(|value| value.contains(name))
            })
            && query.automation_id.as_ref().is_none_or(|automation_id| {
                element.automation_id.as_deref() == Some(automation_id.as_str())
            })
            && query.class_name.as_ref().is_none_or(|class_name| {
                element
                    .class_name
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(class_name))
            })
            && query
                .control_type
                .as_ref()
                .is_none_or(|control_type| element.control_type.eq_ignore_ascii_case(control_type))
    });
    let first = matches.next().cloned().ok_or(UiaError::ElementNotFound)?;
    if matches.next().is_some() {
        return Err(UiaError::ElementAmbiguous);
    }
    Ok(first)
}

pub fn resolve_system_control(id: SystemControlId) -> Result<UiaHit, UiaError> {
    #[cfg(windows)]
    {
        match id {
            SystemControlId::StartButton => resolve_via_script(id),
            _ => Err(UiaError::QueryError(format!(
                "{:?} not yet implemented",
                id
            ))),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Err(UiaError::UnsupportedPlatform)
    }
}

fn resolve_via_script(id: SystemControlId) -> Result<UiaHit, UiaError> {
    let script_body = match id {
        SystemControlId::StartButton => {
            r#"Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$u = [System.Windows.Automation.AutomationElement]::RootElement
$scope = [System.Windows.Automation.TreeScope]::Descendants
$tb = $u.FindFirst($scope, (New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Shell_TrayWnd')))
if (!$tb) { Write-Output 'NF:taskbar'; exit 1 }
$sb = $tb.FindFirst($scope, (New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'StartButton')))
if (!$sb) {
    $sb = $tb.FindFirst($scope, (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Start')))
}
if (!$sb) { Write-Output 'NF:start'; exit 1 }
$r = $sb.Current.BoundingRectangle
$oid = $sb.Current.AutomationId; $cn = $sb.Current.ClassName; $nm = $sb.Current.Name
$off = $(if($sb.Current.IsOffscreen){'1'}else{'0'})
$ena = $(if($sb.Current.IsEnabled){'1'}else{'0'})
$lx = [int]$r.Left; $ty = [int]$r.Top; $wd = [int]$r.Width; $ht = [int]$r.Height
$msg = "$oid|$cn|$nm|$off|$ena|$lx|$ty|$wd|$ht"
Write-Output "OK|$msg"
"#
        }
        _ => return Err(UiaError::QueryError(format!("no script for {:?}", id))),
    };

    let tmp = std::env::temp_dir().join("uia-locate.ps1");
    std::fs::write(&tmp, script_body).map_err(|e| UiaError::ComInitFailed(e.to_string()))?;
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&tmp)
        .output()
        .map_err(|e| UiaError::ComInitFailed(e.to_string()))?;

    let text = decode_console_output(&output.stdout).trim().to_string();
    if !output.status.success() && text.is_empty() {
        let stderr = decode_console_output(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("powershell exited with {}", output.status)
        } else {
            format!("powershell exited with {}: {stderr}", output.status)
        };
        return Err(UiaError::QueryError(detail));
    }
    if text.starts_with("NF") {
        return Err(UiaError::ElementNotFound);
    }
    if text.starts_with("OK|") {
        let parts: Vec<&str> = text[3..].splitn(9, '|').collect();
        if parts.len() >= 9 {
            let a_id = parts[0].to_string();
            let cls = parts[1].to_string();
            let nm = parts[2].to_string();
            let offscreen = parts[3] == "1";
            let enabled = parts[4] == "1";
            let x = parts[5].parse::<i32>().unwrap_or(0);
            let y = parts[6].parse::<i32>().unwrap_or(0);
            let w = parts[7].parse::<i32>().unwrap_or(0);
            let h = parts[8].parse::<i32>().unwrap_or(0);

            return Ok(UiaHit {
                automation_id: Some(a_id),
                class_name: Some(cls),
                name: Some(nm),
                control_type: "Button".to_string(),
                bounding_rect: BBoxPx {
                    x,
                    y,
                    width: w,
                    height: h,
                },
                is_offscreen: offscreen,
                is_enabled: enabled,
                confidence: if offscreen { 0.0 } else { 0.99 },
            });
        }
    }
    Err(UiaError::QueryError(format!("unexpected output: {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_snapshot() -> UiaWindowSnapshot {
        UiaWindowSnapshot {
            process_id: 1200,
            native_window_handle: 99,
            name: Some("无标题 - 记事本".into()),
            bounding_rect: BBoxPx {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            dpi: 120,
            elements: vec![UiaElementSnapshot {
                reference: "uia-1".into(),
                process_id: 1200,
                native_window_handle: 99,
                name: Some("文本编辑器".into()),
                automation_id: Some("TextEditor".into()),
                class_name: Some("RichEditD2DPT".into()),
                control_type: "Document".into(),
                value: Some("".into()),
                bounding_rect: BBoxPx {
                    x: 10,
                    y: 50,
                    width: 780,
                    height: 540,
                },
                is_offscreen: false,
                is_enabled: true,
            }],
        }
    }

    #[test]
    fn query_returns_the_unique_enabled_visible_element() {
        let query = UiaQuery {
            process_id: Some(1200),
            window_name: Some("无标题 - 记事本".into()),
            element_name: Some("文本编辑器".into()),
            automation_id: None,
            class_name: Some("RichEditD2DPT".into()),
            control_type: Some("Document".into()),
        };
        let element = resolve_query(&query_snapshot(), &query).unwrap();
        assert_eq!(element.reference, "uia-1");
    }

    #[test]
    fn resolve_start_button() {
        match resolve_system_control(SystemControlId::StartButton) {
            Ok(hit) => {
                eprintln!("StartButton: {:?}", hit.bounding_rect);
                assert!(hit.confidence > 0.5);
                assert!(hit.bounding_rect.width > 0);
            }
            Err(UiaError::UnsupportedPlatform) => {}
            Err(e) => panic!("unexpected: {e}"),
        }
    }
}
