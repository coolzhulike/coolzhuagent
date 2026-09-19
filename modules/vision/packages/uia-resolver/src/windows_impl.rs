use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, TreeScope_Subtree, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow,
    SW_RESTORE,
};

use super::{BBoxPx, UiaElementSnapshot, UiaError, UiaWindowSnapshot};

/// GetWindowRect 在 unaware 线程上会返回逻辑坐标，UIA 和截图 helper 则使用物理坐标。
/// guard 仅覆盖同步观察；不能跨线程转移，也不能改变进程内其它 GUI 线程的 DPI 模式。
struct ThreadDpiGuard {
    previous: DPI_AWARENESS_CONTEXT,
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl ThreadDpiGuard {
    fn enter(context: DPI_AWARENESS_CONTEXT) -> Result<Self, UiaError> {
        let previous = unsafe { SetThreadDpiAwarenessContext(context) };
        if previous.0.is_null() {
            return Err(UiaError::QueryError(format!(
                "SetThreadDpiAwarenessContext: {}", windows::core::Error::from_win32()
            )));
        }
        Ok(Self { previous, _thread_bound: std::marker::PhantomData })
    }
}

impl Drop for ThreadDpiGuard {
    fn drop(&mut self) {
        // previous 是同一线程上设置成功时由 Windows 返回的有效上下文。
        let _ = unsafe { SetThreadDpiAwarenessContext(self.previous) };
    }
}

fn with_physical_coordinates<T>(operation: impl FnOnce() -> Result<T, UiaError>) -> Result<T, UiaError> {
    let _dpi = ThreadDpiGuard::enter(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
    operation()
}

fn init_uia() -> Result<IUIAutomation, UiaError> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok();
    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
        .map_err(|error| UiaError::ComInitFailed(format!("CUIAutomation: {error}")))
}

fn bbox(rect: RECT) -> BBoxPx {
    BBoxPx {
        x: rect.left,
        y: rect.top,
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
    }
}

fn text(value: windows::core::Result<windows::core::BSTR>) -> Option<String> {
    value
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
}

fn control_type_name(id: i32) -> String {
    match id {
        50000 => "Button",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50007 => "ListItem",
        50008 => "List",
        50011 => "MenuItem",
        50013 => "RadioButton",
        50014 => "ScrollBar",
        50015 => "Slider",
        50019 => "Tab",
        50020 => "TabItem",
        50021 => "Text",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "Custom",
        50026 => "Group",
        50028 => "DataGrid",
        50029 => "DataItem",
        50030 => "Document",
        50032 => "Window",
        50033 => "Pane",
        50036 => "Table",
        50037 => "TitleBar",
        _ => return format!("ControlType-{id}"),
    }
    .to_string()
}

fn reference_for(
    process_id: u32,
    window_handle: isize,
    index: usize,
    automation_id: Option<&str>,
    name: Option<&str>,
    control_type: &str,
) -> String {
    let mut hasher = DefaultHasher::new();
    process_id.hash(&mut hasher);
    window_handle.hash(&mut hasher);
    index.hash(&mut hasher);
    automation_id.hash(&mut hasher);
    name.hash(&mut hasher);
    control_type.hash(&mut hasher);
    format!("uia-{:016x}", hasher.finish())
}

fn element_snapshot(
    element: &IUIAutomationElement,
    foreground_handle: isize,
    index: usize,
) -> Option<UiaElementSnapshot> {
    let process_id = unsafe { element.CurrentProcessId() }
        .ok()?
        .try_into()
        .ok()?;
    let is_password = unsafe { element.CurrentIsPassword() }
        .ok()
        .is_some_and(|value| value.as_bool());
    let name = if is_password {
        None
    } else {
        text(unsafe { element.CurrentName() })
    };
    let value = if is_password {
        None
    } else {
        unsafe {
            element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|pattern| pattern.CurrentValue())
                .or_else(|_| {
                    element
                        .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                        .and_then(|pattern| pattern.DocumentRange())
                        .and_then(|range| range.GetText(4_096))
                })
        }
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
    };
    let automation_id = text(unsafe { element.CurrentAutomationId() });
    let class_name = text(unsafe { element.CurrentClassName() });
    let control_type = unsafe { element.CurrentControlType() }
        .map(|value| control_type_name(value.0))
        .unwrap_or_else(|_| "Unknown".to_string());
    let bounding_rect = unsafe { element.CurrentBoundingRectangle() }
        .map(bbox)
        .unwrap_or(BBoxPx {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
    let is_offscreen = unsafe { element.CurrentIsOffscreen() }
        .ok()
        .is_none_or(|value| value.as_bool());
    let is_enabled = unsafe { element.CurrentIsEnabled() }
        .ok()
        .is_some_and(|value| value.as_bool());
    let reference = reference_for(
        process_id,
        foreground_handle,
        index,
        automation_id.as_deref(),
        name.as_deref(),
        &control_type,
    );
    Some(UiaElementSnapshot {
        reference,
        process_id,
        native_window_handle: foreground_handle,
        name,
        automation_id,
        class_name,
        control_type,
        value,
        bounding_rect,
        is_offscreen,
        is_enabled,
    })
}

pub(crate) fn focus_window_impl(native_window_handle: isize) -> Result<(), UiaError> {
    let hwnd = HWND(native_window_handle as *mut core::ffi::c_void);
    if !focus_hwnd(hwnd) {
        return Err(UiaError::QueryError(
            "failed to focus the expected foreground window".to_string(),
        ));
    }
    Ok(())
}

fn focus_hwnd(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        if SetForegroundWindow(hwnd).as_bool() || GetForegroundWindow().0 == hwnd.0 {
            return true;
        }

        let current_thread = GetCurrentThreadId();
        let mut target_process_id = 0u32;
        let target_thread = GetWindowThreadProcessId(hwnd, Some(&mut target_process_id));
        let foreground = GetForegroundWindow();
        let mut foreground_process_id = 0u32;
        let foreground_thread =
            GetWindowThreadProcessId(foreground, Some(&mut foreground_process_id));

        let attached_target = target_thread != 0
            && target_thread != current_thread
            && AttachThreadInput(current_thread, target_thread, true).as_bool();
        let attached_foreground = foreground_thread != 0
            && foreground_thread != current_thread
            && foreground_thread != target_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();

        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        let focused = SetForegroundWindow(hwnd).as_bool() || GetForegroundWindow().0 == hwnd.0;

        if attached_foreground {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
        if attached_target {
            let _ = AttachThreadInput(current_thread, target_thread, false);
        }

        focused || GetForegroundWindow().0 == hwnd.0
    }
}

pub(crate) fn focus_window_by_hint_impl(
    application: Option<&str>,
    window: Option<&str>,
    objective: Option<&str>,
) -> Result<Option<isize>, UiaError> {
    let mut hints = Vec::new();
    if let Some(application) = application.map(str::trim).filter(|value| !value.is_empty()) {
        hints.push(application.to_ascii_lowercase());
    }
    if let Some(window) = window.map(str::trim).filter(|value| !value.is_empty()) {
        hints.push(window.to_ascii_lowercase());
    }
    if objective.is_some_and(|value| {
        let lower = value.to_ascii_lowercase();
        lower.contains("notepad") || value.contains("记事本")
    }) {
        hints.push("notepad".to_string());
        hints.push("记事本".to_string());
    }
    hints.sort();
    hints.dedup();
    if hints.is_empty() {
        return Ok(None);
    }

    let mut search = WindowHintSearch { hints, best: None };
    unsafe {
        EnumWindows(
            Some(enum_window_for_hint),
            LPARAM(&mut search as *mut _ as isize),
        )
    }
    .map_err(|error| UiaError::QueryError(format!("EnumWindows: {error}")))?;
    let Some((_, handle)) = search.best else {
        return Ok(None);
    };
    let hwnd = HWND(handle as *mut core::ffi::c_void);
    if let Ok(uia) = init_uia() {
        if let Ok(element) = unsafe { uia.ElementFromHandle(hwnd) } {
            let _ = unsafe { element.SetFocus() };
        }
    }
    focus_window_impl(handle)?;
    Ok(Some(handle))
}

struct WindowHintSearch {
    hints: Vec<String>,
    best: Option<(i32, isize)>,
}

unsafe extern "system" fn enum_window_for_hint(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let search = unsafe { &mut *(lparam.0 as *mut WindowHintSearch) };
    let name = window_text(hwnd);
    let class_name = window_class(hwnd);
    let haystack = format!("{name} {class_name}").to_ascii_lowercase();
    let mut score = 0;
    for hint in &search.hints {
        if hint == "notepad" {
            if haystack.contains("notepad") {
                score += 6;
            }
        } else if hint == "记事本" {
            if name.contains("记事本") {
                score += 6;
            }
        } else if haystack.contains(hint) || name.contains(hint) {
            score += 4;
        }
    }
    if score > 0 && search.best.is_none_or(|(best_score, _)| score > best_score) {
        search.best = Some((score, hwnd.0 as isize));
    }
    BOOL(1)
}

fn window_text(hwnd: HWND) -> String {
    let mut buffer = vec![0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
}

fn window_class(hwnd: HWND) -> String {
    let mut buffer = vec![0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
}

pub(crate) fn snapshot_foreground_window_impl(limit: usize) -> Result<UiaWindowSnapshot, UiaError> {
    with_physical_coordinates(|| snapshot_foreground_window_physical(limit))
}

fn snapshot_foreground_window_physical(limit: usize) -> Result<UiaWindowSnapshot, UiaError> {
    let limit = limit.clamp(1, 2_000);
    let hwnd: HWND = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err(UiaError::ElementNotFound);
    }
    let uia = init_uia()?;
    let root = unsafe { uia.ElementFromHandle(hwnd) }
        .map_err(|error| UiaError::QueryError(format!("ElementFromHandle: {error}")))?;
    let process_id = unsafe { root.CurrentProcessId() }
        .map_err(|error| UiaError::QueryError(format!("CurrentProcessId: {error}")))?
        .try_into()
        .map_err(|_| UiaError::QueryError("invalid foreground process id".to_string()))?;
    let name = text(unsafe { root.CurrentName() });
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|error| UiaError::QueryError(format!("GetWindowRect: {error}")))?;
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let condition = unsafe { uia.CreateTrueCondition() }
        .map_err(|error| UiaError::QueryError(format!("CreateTrueCondition: {error}")))?;
    let array = unsafe { root.FindAll(TreeScope_Subtree, &condition) }
        .map_err(|error| UiaError::QueryError(format!("FindAll: {error}")))?;
    let length = unsafe { array.Length() }
        .unwrap_or(0)
        .max(0)
        .try_into()
        .unwrap_or(0usize)
        .min(limit);
    let foreground_handle = hwnd.0 as isize;
    let mut elements = Vec::with_capacity(length);
    for index in 0..length {
        let Ok(element) = (unsafe { array.GetElement(index as i32) }) else {
            continue;
        };
        if let Some(snapshot) = element_snapshot(&element, foreground_handle, index) {
            elements.push(snapshot);
        }
    }
    Ok(UiaWindowSnapshot {
        process_id,
        native_window_handle: foreground_handle,
        name,
        bounding_rect: bbox(rect),
        dpi,
        elements,
    })
}

#[cfg(test)]
mod dpi_tests {
    use super::*;
    use windows::Win32::UI::HiDpi::{
        AreDpiAwarenessContextsEqual, GetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_UNAWARE,
    };

    fn current_is(context: DPI_AWARENESS_CONTEXT) -> bool {
        unsafe { AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), context).as_bool() }
    }

    #[test]
    fn physical_snapshot_scope_restores_original_thread_context_after_success() {
        let _original = ThreadDpiGuard::enter(DPI_AWARENESS_CONTEXT_UNAWARE).unwrap();
        assert!(current_is(DPI_AWARENESS_CONTEXT_UNAWARE));
        let result = with_physical_coordinates(|| {
            assert!(current_is(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2));
            Ok(144u32)
        }).unwrap();
        assert_eq!(result, 144);
        assert!(current_is(DPI_AWARENESS_CONTEXT_UNAWARE));
    }

    #[test]
    fn physical_snapshot_scope_restores_original_thread_context_after_early_error() {
        let _original = ThreadDpiGuard::enter(DPI_AWARENESS_CONTEXT_UNAWARE).unwrap();
        let result: Result<(), UiaError> = with_physical_coordinates(|| {
            assert!(current_is(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2));
            // 用无效句柄模拟观察阶段提前失败，不查询或操作任何真实 GUI。
            let mut rect = RECT::default();
            unsafe { GetWindowRect(HWND::default(), &mut rect) }
                .map_err(|error| UiaError::QueryError(format!("GetWindowRect: {error}")))?;
            Ok(())
        });
        assert!(result.is_err());
        assert!(current_is(DPI_AWARENESS_CONTEXT_UNAWARE));
    }

    #[test]
    fn rejected_dpi_context_does_not_change_current_thread_context() {
        let _original = ThreadDpiGuard::enter(DPI_AWARENESS_CONTEXT_UNAWARE).unwrap();
        assert!(ThreadDpiGuard::enter(DPI_AWARENESS_CONTEXT::default()).is_err());
        assert!(current_is(DPI_AWARENESS_CONTEXT_UNAWARE));
    }
}
