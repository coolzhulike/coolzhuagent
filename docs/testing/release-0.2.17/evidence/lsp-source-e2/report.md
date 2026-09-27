# Rust 语言服务源码版 E2 现场验收

日期：2026-09-27。证据等级：**E2，仅隔离源码构建**；不等同本轮最终 MSI 安装版 E3。

## 身份与隔离

- Git HEAD：`9eae3aec3058f77e962702f7fc99aecb66653a76`，未提交的本轮源码改动另见工作区 diff。
- 最终 Web 可执行文件先从 `target/debug/coolzhu-web-console.exe` 复制到忽略区 `tmp/lsp-e2-20260927-095735/coolzhu-web-console-e2-v3.exe`，SHA256：`21083A80F4AB51F5AFF073FC8AB32B3AD8581B467ECFF4D02D135A6AD088C44C`，UTC 修改时间 `2026-09-27T02:50:19.9070700Z`。此前布局修正后的中间二进制 SHA256 为 `9FDB2A3BE09013B1ABA85F47930AFB89CCB92552E5FC24CCC33E9C24E271BC4B`。
- 独立服务绑定 `127.0.0.1:18767`；运行目录 `tmp/lsp-e2-20260927-095735/workspace`，`LOCALAPPDATA` 指向同一隔离目录下的 `localappdata`。只在这里写入用户级 Rust preset 配置。未操作用户正在使用的 `8765` 服务。
- 服务：当前 stable 工具链的官方 `rust-analyzer 1.94.1 (e408947b 2026-03-25)`。测试项目 Cargo.toml 包含独立 `[workspace]`，使它在仓库 `tmp` 下不继承父 workspace。
- 验证后已通过界面关闭语言服务，`GET /api/lsp/status` 返回 `configured=true, running=false, handle=null`；测试浏览器和仅 `18767` 服务已关闭，系统中无残留 `rust-analyzer.exe`。

## 真实界面结果

1. 在工程右栏打开 `src/main.rs`，显式点击“配置并启动 Rust 服务”。打开文件前 `GET /api/lsp/status` 只读且不启动进程；启动后 UI 显示已运行。测试文件中 `let = ;` 是有意放入的语法错误。
2. 真实服务发布 `6:9 Error · expected pattern`。右栏显示“诊断 · 1”，点击该结果后光标坐标变为 `6:9`。[诊断可见](05-diagnostic-fixed-layout.png)、[点击后坐标](10-diagnostic-click-6-9.png)。
3. 在第 4 行的 `answer()` 调用上点击“跳转定义”，得到 `src/main.rs:1:4` 一项；点击“查找引用”，得到第 4 行调用和第 1 行声明两项。[定义可见](06-definition-visible.png)、[引用两项可见](07-references-visible.png)。点击第 1 行引用后坐标变为 `1:4`，[点击后坐标](11-reference-click-1-4.png)。
4. 文件打开后预览占右栏主要高度；“目录”按钮可展开工程树，再收起回到完整文件预览。[目录展开](08-directory-toggle.png)。关闭服务后右栏显示“已配置，需手动启动”，[关闭界面](12-stopped-final.png)，服务状态与之吻合。

初次测试夹具缺少独立 `[workspace]`，`cargo metadata` 明确报“current package believes it's in a workspace when it's not”；当时语言服务虽启动，诊断/定义为空。补齐夹具后才重测并取得以上结果。最早的 [打开前](01-before-start.png)、[启动但布局受挤压](02-started.png)、[定义结果被挤压](03-definition.png)、[引用结果被挤压](04-references.png) 仅保留为失败现场，不计作通过。中间构建的 [关闭状态](09-stopped.png) 后又由最终构建复核。

源码验证：`cargo build -p coolzhu-web-console --offline` 通过，最后一次日志在 `tmp/lsp-e2-final-build.log`；`node --check modules/gui-web/packages/web-console/src/app.js` 通过。`cargo test -p coolzhu-language-service --offline` 为 7/7 通过、1 项显式真实服务测试按常规忽略；随后单独运行 `real_rust_analyzer_initializes_navigates_and_stops -- --ignored` 为 1/1 通过。测试日志分别在 `tmp/lifecycle-e2-20260927b/lsp-test.log` 与 `lsp-real-test.log`。浏览器操作临时日志已移至 `tmp/lsp-e2-20260927-095735/playwright-cli-logs`。
