//! 本轮用户明确提出的原生网页边界；不从历史、记忆或模型参数选择后端或创建权限。
use computer_use::{ComputerUseError, ComputerUseRequest, ComputerUseRetryOwner, ComputerUseSurface};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ComputerUseTurnScope {
    native_browser: bool,
    native_browser_read_only: bool,
}

impl ComputerUseTurnScope {
    pub(super) fn from_current_user(text: &str) -> Self {
        let text = text.to_ascii_lowercase();
        let native_browser = ["内置浏览器", "内置网页", "内置页", "右栏浏览器", "右栏网页", "右栏页面",
            "右栏原生浏览器", "右侧原生浏览器", "右侧浏览器", "右侧网页", "右侧扩展栏浏览器", "builtin browser", "built-in browser"]
            .iter().any(|name| text.contains(name));
        let explicit_no_input = ["不得发送输入", "不要发送输入", "不发送输入",
            "不点击、不滚动、不输入", "不得点击、滚动或输入", "禁止点击、滚动或输入", "do not send input"]
            .iter().any(|restriction| text.contains(restriction)) || forbids_all_browser_input(&text);
        Self { native_browser, native_browser_read_only: native_browser && explicit_no_input }
    }

    pub(super) fn native_browser(self) -> bool { self.native_browser }

    pub(super) fn native_browser_read_only(self) -> bool {
        self.native_browser_read_only
    }

    pub(super) fn validate(self, request: &ComputerUseRequest) -> Result<(), ComputerUseError> {
        if !self.native_browser {
            return Ok(());
        }
        let desktop_target = request.target.as_ref().is_some_and(|target|
            target.application.is_some() || target.window.is_some());
        let browser_target = request.target.as_ref().is_some_and(|target|
            target.url.as_ref().is_some_and(|url| !url.trim().is_empty()));
        if desktop_target || request.surface == ComputerUseSurface::Desktop
            || (request.surface == ComputerUseSurface::Auto && !browser_target) {
            return Err(ComputerUseError::blocked(
                if self.native_browser_read_only { "current_turn_readonly_browser_required" } else { "current_turn_native_browser_required" },
                "本轮用户明确选择原生内置浏览器；禁止改为桌面或继续历史任务。请使用 browser surface。",
                ComputerUseRetryOwner::Model,
            ));
        }
        Ok(())
    }
}

/// 三类输入全部被禁止时，顺序与顿号/“或”不改变边界；不把只禁点击误当纯只读。
fn forbids_all_browser_input(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let orders = [["点击", "输入", "滚动"], ["点击", "滚动", "输入"],
        ["滚动", "输入", "点击"], ["滚动", "点击", "输入"],
        ["输入", "点击", "滚动"], ["输入", "滚动", "点击"]];
    ["不得", "禁止", "不要"].iter().any(|prefix| orders.iter().any(|[first, second, third]|
        [format!("{prefix}{first}、{second}、{third}"),
         format!("{prefix}{first}、{second}或{third}")].iter().any(|restriction| compact.contains(restriction))))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(surface: &str, target: serde_json::Value) -> ComputerUseRequest {
        serde_json::from_value(serde_json::json!({"objective":"历史任务", "surface":surface,
            "target":target, "success_criteria":["目标可见"], "constraints":[]})).unwrap()
    }
    #[test]
    fn current_readonly_browser_rejects_historical_paint_before_adapter_creation() {
        let scope = ComputerUseTurnScope::from_current_user("内置浏览器，不点击、不滚动、不输入");
        let error = scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).unwrap_err();
        assert_eq!(error.code, "current_turn_readonly_browser_required");
        assert!(scope.validate(&request("auto", serde_json::json!({"application":"mspaint"}))).is_err());
        assert!(scope.validate(&request("browser", serde_json::json!({"url":"https://example.com/"}))).is_ok());
        assert!(scope.validate(&request("auto", serde_json::json!({"url":"https://example.com/"}))).is_ok());
    }
    #[test]
    fn ordinary_paint_and_browser_actions_are_not_reclassified_by_history_words() {
        for text in ["在Paint画一条线", "在Chrome先观察再点击链接"] {
            let scope = ComputerUseTurnScope::from_current_user(text);
            assert!(!scope.native_browser());
            assert!(!scope.native_browser_read_only());
            assert!(scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).is_ok());
        }
        for text in ["在内置浏览器先观察再点击链接", "内置浏览器不要点击按钮，使用键盘输入", "在右栏浏览器点击下一页"] {
            let scope = ComputerUseTurnScope::from_current_user(text);
            assert!(scope.native_browser());
            assert!(!scope.native_browser_read_only());
            assert!(scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).is_err());
            assert!(scope.validate(&request("browser", serde_json::json!({"url":"https://example.com/"}))).is_ok());
        }
    }
    #[test]
    fn readonly_browser_accepts_the_current_combined_chinese_restriction() {
        for text in ["内置浏览器，不得点击、滚动或输入", "内置浏览器，禁止点击、滚动或输入",
            "内置浏览器，不得点击、输入、滚动", "内置浏览器，不要滚动、输入或点击",
            "内置浏览器，禁止输入、 点击 、滚动"] {
            let scope = ComputerUseTurnScope::from_current_user(text);
            assert!(scope.native_browser_read_only());
            assert!(scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).is_err());
        }
    }
    #[test]
    fn current_panel_page_request_uses_native_backend_without_reclassifying_other_panels() {
        let scope = ComputerUseTurnScope::from_current_user(
            "在当前右栏页面点击增加次数一次，只操作当前内置页，不输入、不滚动、不导航");
        assert!(scope.native_browser());
        assert!(!scope.native_browser_read_only());
        assert!(scope.validate(&request("browser", serde_json::json!({"url":"http://127.0.0.1:57159/click.html"}))).is_ok());
        assert!(scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).is_err());
        // 实操中的同义表述仍明确指向右栏，不能静默改走外部扩展后端。
        for text in ["仅当前右栏原生浏览器，不操作其它软件", "使用当前右侧原生浏览器点击一次"] {
            let scope = ComputerUseTurnScope::from_current_user(text);
            assert!(scope.native_browser());
            assert!(!scope.native_browser_read_only());
            assert!(scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).is_err());
        }
        for text in ["在右栏设置修改模型参数", "在Chrome点击网页，右栏显示统计", "在Paint绘图，右栏显示运行轨迹"] {
            assert!(!ComputerUseTurnScope::from_current_user(text).native_browser());
        }
    }
}
