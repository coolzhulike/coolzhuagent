use super::UiaError;

/// 显式应用与窗口是同时成立的条件；目标不明确时禁止选第一个窗口。
pub(super) struct WindowTarget {
    application: Option<String>,
    window: Option<String>,
}

impl WindowTarget {
    pub(super) fn from_hints(
        application: Option<&str>,
        window: Option<&str>,
        objective: Option<&str>,
    ) -> Option<Self> {
        let application = normalized_hint(application);
        let window = normalized_hint(window);
        if application.is_some() || window.is_some() {
            return Some(Self {
                application,
                window,
            });
        }
        // 保留没有显式目标的旧记事本提示，不让目标文本覆盖显式参数。
        objective
            .filter(|value| {
                value.to_ascii_lowercase().contains("notepad") || value.contains("记事本")
            })
            .map(|_| Self {
                application: Some("notepad".into()),
                window: None,
            })
    }

    pub(super) fn matches(&self, executable: Option<&str>, title: &str, class: &str) -> bool {
        let title = title.to_ascii_lowercase();
        let class = class.to_ascii_lowercase();
        self.application.as_deref().is_none_or(|application| {
            let executable = executable.map(normalized_path);
            if application.contains('/') {
                // 完整路径必须相同，不能退回只比较同名进程。
                executable.as_deref() == Some(application)
            } else if application.ends_with(".exe") {
                // 进程名不能从聊天内容、窗口标题或类名里推测。
                executable
                    .as_deref()
                    .and_then(|path| path.rsplit('/').next())
                    == Some(application)
            } else {
                executable
                    .as_deref()
                    .and_then(|path| path.rsplit('/').next())
                    .is_some_and(|name| name.strip_suffix(".exe").unwrap_or(name) == application)
                    || title.contains(application)
                    || class.contains(application)
            }
        }) && self
            .window
            .as_deref()
            .is_none_or(|window| title.contains(window) || class.contains(window))
    }
}

fn normalized_hint(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalized_path)
}

fn normalized_path(value: &str) -> String {
    value.replace('\\', "/").to_ascii_lowercase()
}

pub(super) fn unique_window(handles: &[isize]) -> Result<isize, UiaError> {
    match handles {
        [handle] => Ok(*handle),
        [] => Err(UiaError::ElementNotFound),
        _ => Err(UiaError::ElementAmbiguous),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_executable_matches_localized_paint_without_title_hint() {
        let target = WindowTarget::from_hints(Some(" MSPaint.EXE "), None, None).unwrap();
        assert!(target.matches(
            Some(r"C:\Program Files\WindowsApps\PaintApp\mspaint.exe"),
            "无标题 - 画图",
            "ApplicationFrameWindow"
        ));
        assert!(!target.matches(
            Some(r"C:\Coolzhu\coolzhu-tauri-shell.exe"),
            "聊天 mspaint.exe",
            "mspaint.exe"
        ));
        assert!(!target.matches(None, "mspaint.exe", "mspaint.exe"));
    }

    #[test]
    fn explicit_application_and_window_must_both_match() {
        let target = WindowTarget::from_hints(
            Some("mspaint.exe"),
            Some("无标题 - 画图"),
            Some("打开记事本"),
        )
        .unwrap();
        assert!(target.matches(Some("mspaint.exe"), "无标题 - 画图", "Window"));
        assert!(!target.matches(Some("notepad.exe"), "无标题 - 画图", "Window"));
        assert!(!target.matches(Some("mspaint.exe"), "另一个文件 - 画图", "Window"));
    }

    #[test]
    fn full_application_path_does_not_match_a_different_installation() {
        let target =
            WindowTarget::from_hints(Some(r"C:\Apps\Paint\mspaint.exe"), None, None).unwrap();
        assert!(target.matches(Some("c:/apps/paint/MSPAINT.EXE"), "画图", "Window"));
        assert!(!target.matches(Some(r"D:\Apps\Paint\mspaint.exe"), "画图", "Window"));
    }

    #[test]
    fn unspecified_target_keeps_foreground_mode_and_notepad_inference() {
        assert!(WindowTarget::from_hints(Some(" "), None, Some("绘制轮廓")).is_none());
        let target = WindowTarget::from_hints(None, None, Some("在记事本输入")).unwrap();
        assert!(target.matches(Some("notepad.exe"), "无标题", "Window"));
        assert!(!target.matches(Some("mspaint.exe"), "无标题 - 画图", "Window"));
    }

    #[test]
    fn absent_and_ambiguous_targets_are_errors_instead_of_foreground_fallback() {
        assert!(matches!(unique_window(&[]), Err(UiaError::ElementNotFound)));
        assert!(matches!(
            unique_window(&[1, 2]),
            Err(UiaError::ElementAmbiguous)
        ));
        assert_eq!(unique_window(&[2]).unwrap(), 2);
    }
}
