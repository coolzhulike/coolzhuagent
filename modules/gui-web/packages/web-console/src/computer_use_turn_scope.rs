//! 本轮用户明确提出的原生网页边界；不从历史、记忆或模型参数选择后端或创建权限。
use computer_use::{ComputerUseError, ComputerUseRequest, ComputerUseRetryOwner, ComputerUseSurface};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub(super) struct ComputerUseTurnScope {
    native_browser: bool,
    native_browser_read_only: bool,
    explicit_request: Option<Arc<ComputerUseRequest>>,
}

impl ComputerUseTurnScope {
    pub(super) fn from_current_user(text: &str) -> Self {
        let explicit_request = explicit_request_from_current_user(text).map(Arc::new);
        let text = text.to_ascii_lowercase();
        let native_browser = ["内置浏览器", "内置网页", "内置页", "右栏浏览器", "右栏网页", "右栏页面", "右栏表单", "右侧表单",
            "右栏原生浏览器", "右侧原生浏览器", "右侧浏览器", "右侧网页", "右侧扩展栏浏览器", "builtin browser", "built-in browser"]
            .iter().any(|name| text.contains(name));
        // 导航也是浏览器交互。允许导航但禁止其它三类输入，不能被降成只读任务。
        let allows_navigation = ["允许导航", "只允许一次导航", "仅允许一次导航", "仅执行导航", "只执行导航"]
            .iter().any(|intent| text.contains(intent))
            && !["不允许导航", "不得导航", "禁止导航", "不要导航", "不导航"]
                .iter().any(|restriction| text.contains(restriction));
        let explicit_no_input = ["不得发送输入", "不要发送输入", "不发送输入", "不发送任何输入", "不作任何输入", "do not send input"]
            .iter().any(|restriction| text.contains(restriction))
            || (!allows_navigation && (["不点击、不滚动、不输入", "不得点击、滚动或输入", "禁止点击、滚动或输入"]
                .iter().any(|restriction| text.contains(restriction)) || forbids_all_browser_input(&text)));
        Self { native_browser, native_browser_read_only: native_browser && explicit_no_input, explicit_request }
    }

    pub(super) fn native_browser(&self) -> bool { self.native_browser }

    pub(super) fn native_browser_read_only(&self) -> bool {
        self.native_browser_read_only
    }

    pub(super) fn validate(&self, request: &ComputerUseRequest) -> Result<(), ComputerUseError> {
        // 用户明确给出本轮参数时，模型无权替换成旧轮目标。拒绝后不改写参数、不派发补偿动作。
        if self.explicit_request.as_deref().is_some_and(|expected| expected != request) {
            return Err(ComputerUseError::blocked(
                "current_turn_computer_use_request_mismatch",
                "本轮用户已明确给出 computer_use_perform 参数；模型调用与本轮目标、对象、成功条件或约束不一致，未执行任何操作。",
                ComputerUseRetryOwner::None,
            ));
        }
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

/// 仅识别紧跟工具名与明确参数标签的合法协议对象；不从历史或普通自然语言推导执行契约。
fn explicit_request_from_current_user(text: &str) -> Option<ComputerUseRequest> {
    let lower = text.to_ascii_lowercase();
    let mut requests = Vec::new();
    for (offset, name) in ["computer_use_perform", "computer_use.perform"].iter()
        .flat_map(|name| lower.match_indices(name)) {
        let tail = &text[offset + name.len()..];
        let Some(start) = tail.find('{') else { continue; };
        let label = &tail[..start];
        if label.chars().count() > 80 || !["参数为", "参数如下", "参数：", "参数:", "arguments:", "parameters:"]
            .iter().any(|marker| label.to_ascii_lowercase().contains(marker)) {
            continue;
        }
        // 流式反序列化只消费一个完整对象，允许其后仍有本轮说明，也正确处理字符串内的括号。
        if let Some(Ok(request)) = serde_json::Deserializer::from_str(&tail[start..])
            .into_iter::<ComputerUseRequest>().next() {
            if request.validate().is_ok() { requests.push(request); }
        }
    }
    // 多个合法对象可能是比较示例，不能任意选一个变成授权；保持自然语言原路径。
    (requests.len() == 1).then(|| requests.remove(0))
}

/// 三类输入全部被禁止时，顺序与分隔符不改变边界；不把只禁点击误当纯只读。
fn forbids_all_browser_input(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace())
        .map(|c| if c == '/' || c == '／' { '、' } else { c }).collect();
    // 日常表述常逐项重复否定词；仍要求三类输入都明确禁止，不能把部分限制当只读。
    if ["点击", "输入", "滚动"].iter().all(|action|
        ["不", "不得", "禁止", "不要"].iter().any(|prefix|
            compact.contains(&format!("{prefix}{action}")))) {
        return true;
    }
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
    fn explicit_current_request_rejects_old_goal_even_on_the_same_browser() {
        let mut current = request("browser", serde_json::json!({"url":"https://example.com/"}));
        current.max_actions = Some(1);
        let scope = ComputerUseTurnScope::from_current_user(&format!(
            "在内置浏览器调用 computer_use_perform，参数为{}。仅处理本轮。", serde_json::to_string(&current).unwrap()));
        assert!(scope.validate(&current).is_ok());
        let mut old = current.clone();
        old.objective = "继续点击到10次".into();
        let error = scope.validate(&old).unwrap_err();
        assert_eq!(error.code, "current_turn_computer_use_request_mismatch");
        assert_eq!(error.retry_owner, ComputerUseRetryOwner::None);
        old = current.clone();
        old.constraints.push("移除本轮停止边界".into());
        assert!(scope.validate(&old).is_err());
        old = current.clone();
        old.max_actions = Some(2);
        assert!(scope.validate(&old).is_err());
        old.max_actions = None;
        assert!(scope.validate(&old).is_err());
    }
    #[test]
    fn ordinary_json_and_natural_language_do_not_create_an_explicit_contract() {
        let value = serde_json::to_string(&request("browser", serde_json::json!({"url":"https://example.com/"}))).unwrap();
        for text in [format!("请解释这个JSON {value}"), format!("computer_use_perform 的结果是{value}"),
            format!("比较 computer_use_perform 参数为{value} 与 computer_use_perform 参数为{value}")] {
            assert!(explicit_request_from_current_user(&text).is_none());
        }
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
        for text in ["在内置浏览器先观察再点击链接", "内置浏览器不要点击按钮，使用键盘输入", "在右栏浏览器点击下一页",
            "右栏原生浏览器禁止点击/滚动，允许输入"] {
            let scope = ComputerUseTurnScope::from_current_user(text);
            assert!(scope.native_browser());
            assert!(!scope.native_browser_read_only());
            assert!(scope.validate(&request("desktop", serde_json::json!({"application":"mspaint"}))).is_err());
            assert!(scope.validate(&request("browser", serde_json::json!({"url":"https://example.com/"}))).is_ok());
        }
    }

    #[test]
    fn navigation_permission_is_not_lost_when_other_inputs_are_forbidden() {
        let scope = ComputerUseTurnScope::from_current_user(
            "当前右栏原生浏览器，只允许一次导航，不点击计数、不输入、不滚动");
        assert!(scope.native_browser());
        assert!(!scope.native_browser_read_only());
        // 绝对禁止发送输入仍优先，不能由允许导航抵消。
        let scope = ComputerUseTurnScope::from_current_user(
            "当前右栏原生浏览器，不发送任何输入，只允许一次导航");
        assert!(scope.native_browser_read_only());
        let scope = ComputerUseTurnScope::from_current_user(
            "当前右栏原生浏览器，不允许导航，不点击、不滚动、不输入");
        assert!(scope.native_browser_read_only());
        let scope = ComputerUseTurnScope::from_current_user(
            "当前右栏原生浏览器，只允许一次导航，不点击、不滚动、不输入");
        assert!(!scope.native_browser_read_only());
    }
    #[test]
    fn readonly_browser_accepts_the_current_combined_chinese_restriction() {
        for text in ["内置浏览器，不得点击、滚动或输入", "内置浏览器，禁止点击、滚动或输入",
            "内置浏览器，不得点击、输入、滚动", "内置浏览器，不要滚动、输入或点击",
            "内置浏览器，禁止输入、 点击 、滚动", "右栏原生浏览器禁止点击/输入/滚动/导航/提交",
            "右侧原生浏览器禁止输入／滚动／点击", "右栏原生浏览器，不发送任何输入",
            "右栏原生浏览器，不作任何输入", "只读当前右栏网页，不点击、不输入、不滚动、不导航",
            "内置浏览器，不要点击；禁止输入；不得滚动"] {
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
        for text in ["仅当前右栏原生浏览器，不操作其它软件", "使用当前右侧原生浏览器点击一次", "在右栏表单勾选准备完成然后提交", "在右侧表单选择行程"] {
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
