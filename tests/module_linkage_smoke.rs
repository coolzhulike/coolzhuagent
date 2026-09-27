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
    //
    // 2026-09-26 入口清单核对（§B-112）补上 `computer_use_desktop_bridge.rs`：
    // 它才是**桌面 CU 真正注入输入的地方**（click/drag/key/scroll/text 全在这里落地），
    // 而原先只断言了同 crate 的 `main.rs`——这等于让最关键的那条路径落在断言之外。
    // 实测它本就使用 `controlled_*`，因此补进清单立即通过，作用是**防将来回归**。
    for automation in [
        "modules/gui-desktop/packages/desktop-console/src/desktop_agent.rs",
        "modules/gui-web/packages/web-console/src/main.rs",
        "modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs",
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

/// **CU-04 帧绑定的接线守卫**：绑定必须真的接在"坐标即将被映射成物理像素"的位置上。
///
/// 纯函数测试只能证明 `FrameRef` 自己算得对，证明不了它被调用。这里用源码级守卫钉住三件事：
/// 观察快照必须**记录**绑定、拖拽路径必须**在映射前**用绑定把门、且窗口物理身份比较
/// **不得**被这层绑定替换掉（五项全比必须还在）。任何一条被删掉，这个守卫都会红。
#[test]
fn frame_binding_is_wired_at_the_mapping_point_and_does_not_replace_window_identity() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bridge = std::fs::read_to_string(
        root.join("modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs"),
    )
    .expect("桌面桥源码必须可读");
    let adapters = std::fs::read_to_string(
        root.join("modules/gui-web/packages/web-console/src/computer_use_adapters.rs"),
    )
    .expect("适配器源码必须可读");

    // 1) 观察快照记录绑定：证据里必须出现绑定标记的构造，且绑定失败时如实记 unbindable。
    assert!(
        bridge.contains("let frame_marker = FrameRef::bind("),
        "快照必须记录帧绑定（否则坐标属于哪一版图仍然无从核对）"
    );
    assert!(
        bridge.contains("map_or_else(|error| error.evidence(), |frame| frame.evidence())"),
        "绑定不成立时必须如实写成 unbindable 证据，而不是悄悄省略这一条"
    );
    assert!(
        bridge.contains("frame_marker,"),
        "绑定证据必须真的进入快照的 evidence 列表"
    );

    // 2) 拖拽路径在映射**之前**把门：FrameRef::bind 必须出现在 stroke_arguments 之前。
    let bind_at = bridge
        .find("let frame = FrameRef::bind(&expected.state, &expected.evidence, rect)")
        .expect("拖拽路径必须在映射前建立绑定");
    let map_at = bridge
        .find("let (points, duration_ms) = stroke_arguments(")
        .expect("拖拽路径必须调用 stroke_arguments");
    assert!(
        bind_at < map_at,
        "帧绑定必须发生在坐标映射之前（否则等于先映射再检查，等于没检查）"
    );

    // 3) 无侧改：桌面窗口物理身份仍是五项全比，且仍被调用。
    //    注意必须取 `impl DesktopSnapshot` 里的那一个——浏览器侧另有同名函数比较的是
    //    页面身份（page_id/url/dom_revision），两者不能混看。
    let impl_at = adapters
        .find("impl DesktopSnapshot {")
        .expect("DesktopSnapshot 实现必须存在");
    let desktop = &adapters[impl_at..];
    let identity = desktop
        .find("fn same_input_identity(&self, other: &Self) -> bool {")
        .expect("桌面身份比较必须存在");
    // 按字符取窗口：源码含中文注释，按字节切片会落在字符中间。
    let body: String = desktop[identity..].chars().take(400).collect();
    for field in [
        "self.window_id == other.window_id",
        "self.process_id == other.process_id",
        "self.window_rect == other.window_rect",
        "self.dpi == other.dpi",
        "self.webview2_overlay == other.webview2_overlay",
    ] {
        assert!(
            body.contains(field),
            "窗口输入身份被削弱了：少了 `{field}`。帧绑定只负责图像/裁剪/缩放，\
             不得取代窗口物理身份"
        );
    }
    assert!(
        adapters.contains("same_input_identity(&current)"),
        "窗口身份比较必须仍然在输入前被调用"
    );
}

/// **RPR-01b 的同进程那一半**：web-console 的**测试代码**里不得出现裸进程环境写入。
///
/// 既有的 `test_environment_writes_are_paired_with_a_restoring_guard` 是**文件级**检查
/// （文件里出现过守卫标记就放行），因此看不出一件事：一个文件里有守卫类，同时**另有一处**
/// 手工 `saved = var_os()` → `remove_var()` → 末尾 `set_var(saved)` 的写入。那种写法一旦中间
/// 断言 panic 就会把变量永久删掉——`computer_use_executor.rs` 里确实有过这样一处。
///
/// 这里把不变式收紧到**逐处**：测试区里出现 `std::env::set_var(` / `std::env::remove_var(`
/// 即视为违规，**唯一**例外是守卫/锁的实现模块 `test_env.rs`。这样"改写进程环境"这件事
/// 在 web-console 里只有一条路可走，而那条路自带锁与恢复。
#[test]
fn web_console_test_code_writes_process_env_only_through_the_shared_guard() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let crate_src = root.join("modules/gui-web/packages/web-console/src");
    // 唯一的例外：守卫与进程环境锁的实现本身。
    let guard_module = "modules/gui-web/packages/web-console/src/test_env.rs";
    // 整文件都是测试代码的模块（由 main.rs 以 `#[cfg(test)] mod NAME;` 声明）。
    let main_rs = std::fs::read_to_string(crate_src.join("main.rs")).expect("main.rs 必须可读");
    let mut all_test_modules = Vec::new();
    for line in main_rs.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("#[cfg(test)] mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                all_test_modules.push(name.to_string());
            }
        }
    }

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&crate_src).expect("src 目录必须可读") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative == guard_module {
            continue;
        }
        let contents = std::fs::read_to_string(&path).expect("源码必须可读");
        let module_name = name.trim_end_matches(".rs");
        // 认定"测试区"：整文件即测试模块时取全文；否则取内联 `#[cfg(test)] mod tests` 之后。
        let test_region = if all_test_modules.iter().any(|item| item == module_name) {
            contents.as_str()
        } else {
            match contents.find("\n#[cfg(test)]\nmod tests") {
                Some(index) => &contents[index..],
                None => "",
            }
        };
        let raw_writes = test_region.matches("std::env::set_var(").count()
            + test_region.matches("std::env::remove_var(").count();
        if raw_writes > 0 {
            offenders.push(format!("{relative}（{raw_writes} 处）"));
        }
    }
    assert!(
        offenders.is_empty(),
        "这些文件的**测试代码**里有裸进程环境写入：{offenders:?}
         请改用 `crate::test_env::set` / `crate::test_env::remove`——它们成对提供\
         「作用域结束（含 panic）恢复」与「全 crate 共用的进程环境锁」。\
         只恢复不加锁时，同进程并行跑的其它用例仍会读到中途值；只有锁没有恢复则会泄漏。"
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
