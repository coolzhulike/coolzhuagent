use computer_use::{
    anchor_to_physical_pixel, default_regression_scenarios, standard_resolution_cases,
};
use runtime::{ConversationMessage, Session};

#[test]
fn computer_use_anchor_matrix_is_available_from_root_workspace() {
    let scenario = &default_regression_scenarios()[0];
    let resolution = standard_resolution_cases()
        .iter()
        .find(|case| case.name == "fhd-100")
        .copied()
        .expect("fhd-100 resolution should exist");

    let point = anchor_to_physical_pixel(scenario.anchor, resolution)
        .expect("anchor should map to physical point");

    assert_eq!(point, (86, 173));
}

#[test]
fn vision_grounding_parser_is_available_from_root_workspace() {
    assert_eq!(
        vision::parse_relative_point("target center: [0.25, 0.75]"),
        Some((0.25, 0.75))
    );
}

#[test]
fn server_app_can_be_constructed_from_root_workspace() {
    let _app = server::app(server::AppState::default());
}

#[test]
fn runtime_session_message_type_is_available_from_root_workspace() {
    let mut session = Session::new();
    session
        .messages
        .push(ConversationMessage::user_text("hello coolzhu"));

    assert_eq!(session.messages.len(), 1);
}

/// **PR-03（P0-3）**：唯一输入入口是受控族 `controlled_*`。
///
/// 背景：普通输入曾经有三条互不知情的路径（无生命周期原语 / 桌宠自动化 / 闭环评测各自直调）。
/// 无生命周期原语没有预留、回执、释放义务登记与收尾对账，中途失败可能在桌面上留下按下的
/// 键或按钮而**没有任何事实可对账**。本用例把"只有一个入口"钉成**源码级**约束：
///
/// 1. `diagnostic_*`（无生命周期原语）只允许出现在**诊断**入口：core 的 `bin/check.rs`、
///    桌宠控制台的 CLI 子命令、以及 core 自身的定义与测试；
/// 2. 自动输入路径必须引用 `controlled_`；
/// 3. 仍被自动路径使用的"无义务"原语只允许是**移动**（不按下按钮/键，因此不产生释放义务），
///    且它们的引用文件被**逐个列出**——多一个引用点就失败。
#[test]
fn only_the_controlled_input_entry_is_reachable_from_automation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources = collect_rust_sources(&root.join("modules"));

    // ① 无生命周期原语只允许出现在诊断入口与它的定义处。
    let diagnostic_allowlist = [
        "modules/computer-use/packages/computer-use-core/src/input.rs",
        "modules/computer-use/packages/computer-use-core/src/bin/check.rs",
        "modules/gui-desktop/packages/desktop-console/src/main.rs",
    ];
    // 针是"函数名 + 左括号"：`diagnostic_redaction_...` 这种**测试名**不该被当成引用
    // （实测踩到过），因此必须精确到调用/定义形状。
    let diagnostic_needles = [
        "diagnostic_click_point(",
        "diagnostic_mouse_button_action_point(",
        "diagnostic_mouse_button_down_point(",
        "diagnostic_mouse_button_up_point(",
        "diagnostic_drag_point(",
        "diagnostic_press_escape(",
        "diagnostic_scroll_wheel(",
        "diagnostic_type_text(",
        "diagnostic_press_virtual_key(",
        "diagnostic_hold_virtual_key(",
        "diagnostic_send_virtual_key_combo(",
        // §B-89 之后：移动也有了受控入口，因此这两个原语同样只剩诊断/自检在用。
        "diagnostic_move_mouse_relative(",
        "diagnostic_move_mouse_absolute(",
    ];
    for (relative, contents) in &sources {
        if !diagnostic_needles.iter().any(|needle| contents.contains(needle)) {
            continue;
        }
        assert!(
            diagnostic_allowlist.contains(&relative.as_str()),
            "{relative} 引用了无生命周期输入原语（diagnostic_*）：自动路径必须走 controlled_*，\
             诊断用途请显式列进本用例的允许清单并说明原因"
        );
    }

    // ② 自动输入路径必须引用受控入口。
    for automation in [
        "modules/gui-desktop/packages/desktop-console/src/desktop_agent.rs",
        "modules/gui-web/packages/web-console/src/main.rs",
    ] {
        let contents = &sources
            .iter()
            .find(|(relative, _)| relative == automation)
            .unwrap_or_else(|| panic!("找不到自动输入路径源码：{automation}"))
            .1;
        assert!(
            contents.contains("controlled_"),
            "{automation} 必须经受控入口（controlled_*）注入输入"
        );
    }

    // ③ **例外已清零**（§B-89）：移动也有受控入口，所以自动化侧不得再引用任何
    // 未受监督原语——上一条规则（①）已经把整族 `diagnostic_*` 限制在诊断入口。
}

/// **RPR-01b**：测试里的进程环境写入必须**成对**——要么有恢复式守卫，要么就是守卫自身。
///
/// 不变式（低误报、可执行）：**同一个文件**里出现 `std::env::set_var(` / `std::env::remove_var(`
/// 时，该文件必须同时定义/使用一个恢复式守卫（名字里含 `ScopedEnv` / `EnvVarGuard` /
/// `InputSafetyEnvGuard` 之一）。生产侧的合法写入（把配置注入进程环境、给子进程准备 WebView2
/// 变量）在允许清单里逐个列出并注明原因。
#[test]
fn test_environment_writes_are_paired_with_a_restoring_guard() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // 生产代码里的合法写入：把配置应用到进程环境（不是测试污染，因此不受本不变式约束）。
    let production_allowlist = [
        "modules/gui-web/packages/web-console/src/main.rs",
        "modules/gui-desktop/packages/desktop-console/src/config.rs",
        "modules/gui-desktop/packages/tauri-shell/src-tauri/src/main.rs",
    ];
    let guard_markers = ["ScopedEnv", "EnvVarGuard", "InputSafetyEnvGuard"];
    let mut offenders = Vec::new();
    for (relative, contents) in collect_rust_sources(&root.join("modules")) {
        let writes_env = contents.contains("std::env::set_var(")
            || contents.contains("std::env::remove_var(");
        if !writes_env || production_allowlist.contains(&relative.as_str()) {
            continue;
        }
        if !guard_markers.iter().any(|marker| contents.contains(marker)) {
            offenders.push(relative);
        }
    }
    assert!(
        offenders.is_empty(),
        "这些文件写了进程环境却没有恢复式守卫（RPR-01b）：{offenders:?}
         请用作用域守卫（进入时记原值、Drop 时恢复），否则用例之间会互相看到对方留下的变量"
    );
}

/// 递归收集仓库源码（只读文本，用于上面的源码级守门）。
fn collect_rust_sources(root: &std::path::Path) -> Vec<(String, String)> {
    let mut collected = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 跳过构建产物与依赖缓存：只看仓库源码。
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| matches!(name, "target" | "node_modules" | ".git"))
                {
                    continue;
                }
                stack.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                let Ok(contents) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let relative = path
                    .strip_prefix(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                collected.push((relative, contents));
            }
        }
    }
    collected
}
