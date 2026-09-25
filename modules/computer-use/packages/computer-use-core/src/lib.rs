pub mod budget;
pub mod cleanup;
pub mod contracts;
pub mod controller;
pub mod input;
pub mod supervisor;

pub use budget::{CuBudgetFacts, CuDeadline, RootDeadline, RootDeadlineState};
pub use cleanup::{
    CleanupDeadline, CleanupPolicy, CleanupReleaseStatus, CleanupReport, HelperCleanupFacts,
};
pub use contracts::{
    action_attempt_id, ComputerUseAction, ComputerUseActionKind, ComputerUseBudgets,
    ComputerUseCapabilities, ComputerUseError, ComputerUseRequest, ComputerUseResult,
    ComputerUseRetryOwner, ComputerUseRiskClass, ComputerUseRunState, ComputerUseStage,
    ComputerUseSurface, ComputerUseTarget, ComputerUseTerminalStatus, Observation, StepExecution,
    StepInputReleaseStatus, SupervisorSnapshot, Verification,
};
pub use controller::{
    ComputerUseAdapter, ComputerUseApprovalPolicy, ComputerUseClock, ComputerUseController,
    ComputerUseEventSink, ComputerUsePlanner, ComputerUseRunContext, PlannerFuture,
};
pub use supervisor::{
    ActionFingerprint, BeforeRunDecision, RunBudgetGuard, TaskIdempotencyKey,
    TurnComputerUseSupervisor,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolutionCase {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseActionKind {
    LeftClick,
    RightClick,
    LeftRightChord,
    DoubleClick,
    Drag,
    TextInput,
    KeyPress,
    Hotkey,
    VerticalScroll,
    HorizontalScroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTargetKind {
    DesktopIcon,
    TaskbarShortcut,
    BrowserAddressBar,
    BrowserSearchBox,
    BrowserConfirmButton,
    BrowserFormInput,
    BrowserScrollablePage,
    BrowserDragSurface,
    WindowsNotepadEditor,
    WindowsScrollablePane,
    WindowsFileDialogInput,
    WindowsSystemConfirmButton,
    WindowControl,
    FullscreenSurface,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativeAnchor {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InteractionScenario {
    pub name: &'static str,
    pub target: UiTargetKind,
    pub action: MouseActionKind,
    pub anchor: RelativeAnchor,
    pub intent: &'static str,
}

#[must_use]
pub const fn standard_resolution_cases() -> &'static [ResolutionCase] {
    &[
        ResolutionCase {
            name: "hd-100",
            width: 1280,
            height: 720,
            scale_factor: 1.0,
        },
        ResolutionCase {
            name: "fhd-100",
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
        },
        ResolutionCase {
            name: "fhd-125",
            width: 1920,
            height: 1080,
            scale_factor: 1.25,
        },
        ResolutionCase {
            name: "qhd-150",
            width: 2560,
            height: 1440,
            scale_factor: 1.5,
        },
        ResolutionCase {
            name: "uhd-200",
            width: 3840,
            height: 2160,
            scale_factor: 2.0,
        },
    ]
}

#[must_use]
pub const fn default_regression_scenarios() -> &'static [InteractionScenario] {
    &[
        InteractionScenario {
            name: "desktop-icon-left-click",
            target: UiTargetKind::DesktopIcon,
            action: MouseActionKind::LeftClick,
            anchor: RelativeAnchor { x: 0.045, y: 0.16 },
            intent: "open a common desktop shortcut",
        },
        InteractionScenario {
            name: "desktop-icon-context-menu",
            target: UiTargetKind::DesktopIcon,
            action: MouseActionKind::RightClick,
            anchor: RelativeAnchor { x: 0.045, y: 0.16 },
            intent: "open context menu for a desktop shortcut",
        },
        InteractionScenario {
            name: "desktop-icon-double-click",
            target: UiTargetKind::DesktopIcon,
            action: MouseActionKind::DoubleClick,
            anchor: RelativeAnchor { x: 0.045, y: 0.16 },
            intent: "open a desktop shortcut with a double click",
        },
        InteractionScenario {
            name: "taskbar-shortcut-left-click",
            target: UiTargetKind::TaskbarShortcut,
            action: MouseActionKind::LeftClick,
            anchor: RelativeAnchor { x: 0.14, y: 0.965 },
            intent: "open a pinned taskbar app",
        },
        InteractionScenario {
            name: "browser-address-search",
            target: UiTargetKind::BrowserAddressBar,
            action: MouseActionKind::TextInput,
            anchor: RelativeAnchor { x: 0.45, y: 0.075 },
            intent: "type a website or search query in the browser address bar",
        },
        InteractionScenario {
            name: "browser-page-search-icon",
            target: UiTargetKind::BrowserSearchBox,
            action: MouseActionKind::LeftClick,
            anchor: RelativeAnchor { x: 0.52, y: 0.34 },
            intent: "click a page search input or search icon",
        },
        InteractionScenario {
            name: "browser-confirm-button",
            target: UiTargetKind::BrowserConfirmButton,
            action: MouseActionKind::LeftClick,
            anchor: RelativeAnchor { x: 0.64, y: 0.62 },
            intent: "click a browser page confirmation button",
        },
        InteractionScenario {
            name: "browser-form-input",
            target: UiTargetKind::BrowserFormInput,
            action: MouseActionKind::TextInput,
            anchor: RelativeAnchor { x: 0.50, y: 0.46 },
            intent: "type text into a browser page form input",
        },
        InteractionScenario {
            name: "browser-form-submit-key",
            target: UiTargetKind::BrowserFormInput,
            action: MouseActionKind::KeyPress,
            anchor: RelativeAnchor { x: 0.50, y: 0.46 },
            intent: "submit a focused browser form with Enter",
        },
        InteractionScenario {
            name: "browser-address-hotkey",
            target: UiTargetKind::BrowserAddressBar,
            action: MouseActionKind::Hotkey,
            anchor: RelativeAnchor { x: 0.45, y: 0.075 },
            intent: "focus the browser address bar with Ctrl+L",
        },
        InteractionScenario {
            name: "browser-page-scroll",
            target: UiTargetKind::BrowserScrollablePage,
            action: MouseActionKind::VerticalScroll,
            anchor: RelativeAnchor { x: 0.80, y: 0.70 },
            intent: "scroll a browser page and verify new content is visible",
        },
        InteractionScenario {
            name: "browser-pane-horizontal-scroll",
            target: UiTargetKind::BrowserScrollablePage,
            action: MouseActionKind::HorizontalScroll,
            anchor: RelativeAnchor { x: 0.50, y: 0.72 },
            intent: "scroll a browser container horizontally and verify its position changed",
        },
        InteractionScenario {
            name: "browser-drag-control",
            target: UiTargetKind::BrowserDragSurface,
            action: MouseActionKind::Drag,
            anchor: RelativeAnchor { x: 0.50, y: 0.62 },
            intent: "drag a deterministic browser control to a new position",
        },
        InteractionScenario {
            name: "windows-notepad-editor",
            target: UiTargetKind::WindowsNotepadEditor,
            action: MouseActionKind::TextInput,
            anchor: RelativeAnchor { x: 0.50, y: 0.50 },
            intent: "type text into a Windows Notepad editor area",
        },
        InteractionScenario {
            name: "windows-notepad-select-all",
            target: UiTargetKind::WindowsNotepadEditor,
            action: MouseActionKind::Hotkey,
            anchor: RelativeAnchor { x: 0.50, y: 0.50 },
            intent: "select all text in a Windows Notepad editor with Ctrl+A",
        },
        InteractionScenario {
            name: "windows-pane-scroll",
            target: UiTargetKind::WindowsScrollablePane,
            action: MouseActionKind::VerticalScroll,
            anchor: RelativeAnchor { x: 0.82, y: 0.72 },
            intent: "scroll a Windows application pane and verify the viewport changed",
        },
        InteractionScenario {
            name: "windows-file-dialog-path",
            target: UiTargetKind::WindowsFileDialogInput,
            action: MouseActionKind::TextInput,
            anchor: RelativeAnchor { x: 0.50, y: 0.88 },
            intent: "type a file path into a Windows file dialog",
        },
        InteractionScenario {
            name: "windows-system-confirm",
            target: UiTargetKind::WindowsSystemConfirmButton,
            action: MouseActionKind::LeftClick,
            anchor: RelativeAnchor { x: 0.56, y: 0.64 },
            intent: "click a Windows system confirmation button",
        },
        InteractionScenario {
            name: "window-close-control",
            target: UiTargetKind::WindowControl,
            action: MouseActionKind::LeftClick,
            anchor: RelativeAnchor { x: 0.985, y: 0.018 },
            intent: "click the active window close button",
        },
        InteractionScenario {
            name: "fullscreen-surface-chord",
            target: UiTargetKind::FullscreenSurface,
            action: MouseActionKind::LeftRightChord,
            anchor: RelativeAnchor { x: 0.50, y: 0.50 },
            intent: "exercise left/right button chord on a fullscreen surface",
        },
    ]
}

#[must_use]
pub fn anchor_to_physical_pixel(
    anchor: RelativeAnchor,
    resolution: ResolutionCase,
) -> Option<(i32, i32)> {
    if !(0.0..=1.0).contains(&anchor.x)
        || !(0.0..=1.0).contains(&anchor.y)
        || resolution.width == 0
        || resolution.height == 0
    {
        return None;
    }

    let x = (anchor.x * resolution.width.saturating_sub(1) as f32).round() as i32;
    let y = (anchor.y * resolution.height.saturating_sub(1) as f32).round() as i32;
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::{
        anchor_to_physical_pixel, default_regression_scenarios, standard_resolution_cases,
        MouseActionKind, UiTargetKind,
    };

    #[test]
    fn regression_matrix_covers_required_mouse_actions() {
        let scenarios = default_regression_scenarios();
        for action in [
            MouseActionKind::LeftClick,
            MouseActionKind::RightClick,
            MouseActionKind::LeftRightChord,
            MouseActionKind::DoubleClick,
            MouseActionKind::Drag,
            MouseActionKind::TextInput,
            MouseActionKind::KeyPress,
            MouseActionKind::Hotkey,
            MouseActionKind::VerticalScroll,
            MouseActionKind::HorizontalScroll,
        ] {
            assert!(
                scenarios.iter().any(|scenario| scenario.action == action),
                "missing action {action:?}"
            );
        }
    }

    #[test]
    fn regression_matrix_covers_required_targets() {
        let scenarios = default_regression_scenarios();
        for target in [
            UiTargetKind::DesktopIcon,
            UiTargetKind::TaskbarShortcut,
            UiTargetKind::BrowserAddressBar,
            UiTargetKind::BrowserSearchBox,
            UiTargetKind::BrowserConfirmButton,
            UiTargetKind::BrowserFormInput,
            UiTargetKind::BrowserScrollablePage,
            UiTargetKind::BrowserDragSurface,
            UiTargetKind::WindowsNotepadEditor,
            UiTargetKind::WindowsScrollablePane,
            UiTargetKind::WindowsFileDialogInput,
            UiTargetKind::WindowsSystemConfirmButton,
            UiTargetKind::WindowControl,
            UiTargetKind::FullscreenSurface,
        ] {
            assert!(
                scenarios.iter().any(|scenario| scenario.target == target),
                "missing target {target:?}"
            );
        }
    }

    #[test]
    fn browser_matrix_covers_confirm_and_form_input() {
        let scenarios = default_regression_scenarios();
        let confirm = scenarios
            .iter()
            .find(|scenario| scenario.name == "browser-confirm-button")
            .expect("browser confirm scenario");
        assert_eq!(confirm.target, UiTargetKind::BrowserConfirmButton);
        assert_eq!(confirm.action, MouseActionKind::LeftClick);

        let form = scenarios
            .iter()
            .find(|scenario| scenario.name == "browser-form-input")
            .expect("browser form input scenario");
        assert_eq!(form.target, UiTargetKind::BrowserFormInput);
        assert_eq!(form.action, MouseActionKind::TextInput);
    }

    #[test]
    fn windows_app_matrix_covers_editor_dialog_and_confirm() {
        let scenarios = default_regression_scenarios();
        let notepad = scenarios
            .iter()
            .find(|scenario| scenario.name == "windows-notepad-editor")
            .expect("notepad editor scenario");
        assert_eq!(notepad.target, UiTargetKind::WindowsNotepadEditor);
        assert_eq!(notepad.action, MouseActionKind::TextInput);

        let file_dialog = scenarios
            .iter()
            .find(|scenario| scenario.name == "windows-file-dialog-path")
            .expect("file dialog path scenario");
        assert_eq!(file_dialog.target, UiTargetKind::WindowsFileDialogInput);
        assert_eq!(file_dialog.action, MouseActionKind::TextInput);

        let confirm = scenarios
            .iter()
            .find(|scenario| scenario.name == "windows-system-confirm")
            .expect("system confirm scenario");
        assert_eq!(confirm.target, UiTargetKind::WindowsSystemConfirmButton);
        assert_eq!(confirm.action, MouseActionKind::LeftClick);
    }

    #[test]
    fn resolution_matrix_covers_common_dpi_modes() {
        let cases = standard_resolution_cases();
        assert!(cases.iter().any(|case| case.width == 1280));
        assert!(cases
            .iter()
            .any(|case| case.width == 1920 && case.scale_factor == 1.25));
        assert!(cases.iter().any(|case| case.width == 2560));
        assert!(cases.iter().any(|case| case.width == 3840));
    }

    #[test]
    fn maps_relative_anchor_to_each_resolution() {
        for case in standard_resolution_cases() {
            let point = anchor_to_physical_pixel(default_regression_scenarios()[0].anchor, *case)
                .expect("anchor should map");
            assert!(point.0 >= 0 && point.0 < case.width as i32);
            assert!(point.1 >= 0 && point.1 < case.height as i32);
        }
    }
}
