# P5 顶栏、聊天对比与安装图标候选

日期：2026-08-27
阶段：P5（实施中）

## 本轮布局决策

- 顶栏采用非对称三槽：品牌约 13%，Agent/工作区为主槽，聊天室/状态为次槽。
- 桌面基线：`clamp(150px, 13vw, 210px) minmax(380px, 1.6fr) minmax(260px, .85fr)`。
- Agent 与工作区入口只连接现有真实配置能力；没有真实下拉时不得伪装成已实现的下拉控件。
- 聊天室名称使用对称轨道实现几何居中，并连接现有会话选择入口。
- 动态状态灯迁入顶栏聊天室槽，保留灯笼呼吸、玉光和朱红异常状态；右栏不重复显示。

## 聊天视觉层级

- 背景：墨青 `#061713`。
- Logo：暖金 `#D6B866`、淡玉 `#A7D7B2`，大尺寸居中，静态透明度约 0.22–0.28。
- 助手卡：烟玉玻璃，玉边 `#78C99A`，最大宽度约 88%。
- 用户卡：乌金茶褐，金边 `#D8B866`，最大宽度约 72%，靠右。
- 工具/推理卡：青墨蓝，灰玉边 `#749B91`，最大宽度约 82%。
- 卡片必须保留背景可见性，但正文仍需清晰；避免外层与内容层叠加后接近完全不透明。

## ImageGen 产物

生成方式：Codex 内置 ImageGen；附件雪碧图仅作为材质和造型参考。

### 顶栏框

- `modules/gui-web/packages/web-console/assets/ui-redesign/p5/topbar-three-plate-scroll-v1.png`
- 提示词核心：透明底、横向三联竹简框、15% 品牌／52% Agent 与工作区／33% 聊天室与状态、墨青与旧金材质、内部无文字和图标。
- 用法：只作为响应式低透明装饰层，真实内容和交互仍由 HTML/CSS 渲染。

### 安装图标候选

目录：`docs/design-assets/coolzhu-icons-2026-08-27/candidates/`

1. `app-icon-a-jade-sword-monogram.png`：竹影剑徽。玉璧负形暗示 C/Z，一柄短剑贯穿；品牌感最强，也可作为聊天背景徽记。
2. `app-icon-b-vermilion-code-seal.png`：朱印竹码。朱红八角印配代码括号；色差最大，小尺寸识别度最佳，当前推荐。
3. `app-icon-c-moon-gate-lantern.png`：月门灯芯。墨玉月门、发光竹叶光标和灯笼；最贴合聊天氛围。
4. `app-icon-d-bamboo-scroll-code.png`：竹简智符。竹简与大号代码括号；开发工具语义最直接。

四份候选均为 1254×1254、32 位 Alpha PNG。提示词共同约束：单一强轮廓、透明外背景、16/32px 可辨识、无桌面标签文字、无场景和水印。

## 安装链约束

用户选定母版后再进入 P5-F：

- 分别导出应用 PNG/ICO 与安装器 PNG/ICO；ICO 必须含 16、24、32、48、64、128、256 七层。
- 同步 Tauri `icon.png` / `icon.ico`、MSI `Product.wxs` 引用资源、启动器 PE 图标及添加/删除程序图标。
- 另做 32/64px 简化托盘图标，并合并当前重复托盘创建链路。
- 不仅检查 Explorer 缓存，还需提取两个 PE 图标并核验 MSI `Icon`/`Shortcut` 表。

## 控件资源真实映射

- `queue-v1.png`：右栏真实任务链入口。
- `steer-now-v1.png`：真实任务转交。
- `pause-v1.png` / `resume-v1.png`：现有 Goal 暂停/继续。
- `restore-chat-v1.png`：现有会话历史回滚。
- `panel-left/right-*`、`focus-layout-v1.png`：聊天室布局控制，主体图提升至约 32px。
- `approve-once`、`approve-rule`、`reject-feedback`：继续用于真实审批。
- `restore-code-v1.png`、`restore-both-v1.png`：后端能力未具备，不挂载到可用控件。

## 已淘汰资产

专用聊天徽记两次生成均把棋盘格烘焙为 RGB，未接入生产资源；工作区中的失败首稿已删除。聊天室改用真透明 A 款玉璧剑徽的版本化副本。

## 本轮实施收口（2026-08-27）

- 顶栏继续采用品牌、Agent/工作区、聊天室/状态三槽；顶栏 Agent 文本与头像跟随真实发送对象选择，工作区末级目录由聊天工作区路径统一同步。
- 顶栏聊天室名称改用三轨网格，文字本身几何居中，chevron 保持独立右侧轨道；状态灯在 DOM 中声明 idle 初始态，并随任务/审批状态更新可访问标签。
- 聊天消息卡补充最小宽度、最大内容宽度、任意断词和代码预换行，保持背景可见且避免长文本造成横向溢出。
- 新增/加强 P5 静态契约：顶栏槽位和交互顺序、唯一状态灯、真实 Agent/工作区/聊天室触发、徽记文本与资源、真实 ImageGen 动作映射、禁止代码/联合恢复假控件。

### 验证结果

- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `git diff --check`：通过（仅有 Git 的 LF/CRLF 提示）。
- `cargo test -p coolzhu-web-console web_frontend_ --offline`：142/142 通过。
- `cargo build -p coolzhu-web-console --offline`：通过；保留仓库既有 Rust 警告。
- `cargo check -p coolzhu-tool-registry --offline`：通过。
- `cargo test --test module_linkage_smoke --offline`：4/4 通过。
- `cargo test -p coolzhu-web-console --offline`：851 通过、1 个既有不相关失败（`web_realtime_session_exposes_tts_health_for_task_card` 仍断言旧英文文案 `Streaming TTS endpoint`，不在本轮 P5 范围）。
- 未启动长期服务或浏览器；未提交 Git。

## P5-F 图标定稿与落地（2026-08-28）

### 定稿母版与派生资源

本轮采用用户选定的“C 月门灯芯＋CZ”透明母版：

`docs/design-assets/coolzhu-icons-2026-08-27/final/app-icon-cz-moon-gate-lantern-v1.png`

母版保持原文件不变（1254×1254、RGBA、透明背景）。`scripts/generate-p5f-app-icon.py` 使用仓库现有的 Pillow 12.3.0 运行时，以 Lanczos 高质量重采样生成 16、24、32、48、64、128、256px PNG，并生成包含这七个 PNG 帧的 Windows ICO；ICO 目录将 256px 帧放在首位，以便 Windows/Tauri 默认读取高分辨率帧。四个角均未出现可见不透明像素；母版左下角原始抗锯齿 alpha 为 1，未被改写。

最终派生资源位于母版 `final/` 目录；Tauri 使用
`modules/gui-desktop/packages/tauri-shell/src-tauri/icons/app-icon-cz-moon-gate-lantern-v1.ico`
及同名 256px PNG，desktop-console 使用
`modules/gui-desktop/packages/desktop-console/assets/app-icon-cz-moon-gate-lantern-v1.png`。

### 实际引用链

- `packages/app-launcher/build.rs` 的 Windows PE 资源编译改为引用最终 ICO，因此 `COOLZHU-AGENT.exe`、桌面快捷方式与开始菜单快捷方式拥有同一应用图标。
- `installer/Product.wxs` 的 `CoolzhuApplicationIcon` 改为唯一应用图标源，桌面/开始菜单快捷方式继续使用它；`ARPPRODUCTICON` 同步指向 `CoolzhuApplicationIcon`，添加/删除程序不再使用独立旧安装器图标。
- `scripts/build-msi.ps1` 和 `scripts/test-package-safety.ps1` 改为校验/传递最终 ICO。
- Tauri bundle 改为版本化 ICO/PNG；移除配置托盘，由现有手工菜单链唯一创建托盘，并使用 `app.default_window_icon()` 作为托盘图标，避免配置托盘与手工托盘重复。
- desktop-console 的 eframe `ViewportBuilder` 改为 include 版本化 256px PNG。

未改聊天室水印、网页布局或业务后端；旧历史文档与生成清单中的历史路径保留为历史记录，活动代码与构建配置不再引用旧图标。

### P5-F 验证记录

- `python scripts/validate-p5f-app-icon.py`：通过；母版角 alpha 为 `(0, 0, 1, 0)`（左下角原始抗锯齿 alpha=1，未修改），派生 PNG 为 16/24/32/48/64/128/256px RGBA，最终 ICO 与 Tauri ICO 均为 7 帧，目录顺序为 256/128/64/48/32/24/16，运行时 256px PNG 与最终派生资源字节一致。
- 离线查看 16/24/32px 派生预览：`CZ` 与月门灯芯轮廓仍可辨；这不替代真实 Windows Shell 的图标缓存/显示验收。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过；`git diff --check`：通过（仅 Git 的换行提示）。
- `cargo test -p coolzhu-web-console web_frontend_ --offline`：143/143 通过；`cargo build -p coolzhu-web-console --offline`：通过，保留仓库既有 Rust 警告。
- `scripts/test-package-safety.ps1`：`PASS package-safety`。
- `cargo build -p coolzhu-app-launcher --offline`：通过；`cargo test -p coolzhu-app-launcher --offline`：26/26 单元测试通过，文档测试 0 项。
- `cargo build -p coolzhu-desktop-console --offline`：通过；`cargo test -p coolzhu-desktop-console --offline`：16/16 通过，保留既有 19 条 desktop-console f32 未来兼容警告。
- `cargo build --manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml --offline`：通过；对应 `cargo test --manifest-path ... --offline` 在测试编译阶段被既有缺失文件 `modules/gui-desktop/packages/tauri-shell/ui/assets/pet-actions/generated-previews-20260617/stabilized-metrics.json` 的 `include_str!` 阻断，未执行测试，与图标改动无关。
- 活动源码/配置扫描：未发现旧应用 ICO、旧 Tauri `icons/icon.*` 或旧 desktop-console 图标引用；历史文档与 `SOURCE-MANIFEST.json` 的历史清单记录未改写。
- 未启动浏览器、桌面程序、长期服务或安装器；仍需测试会话在真实 Windows Shell、Tauri 托盘、desktop-console 窗口、快捷方式/开始菜单及 MSI 添加/删除程序页确认图标缓存与显示效果。

### P5-F 审核校正（2026-08-28）

- 将临时图标生成/验证脚本迁移为 `scripts/generate-p5f-app-icon.py` 与 `scripts/validate-p5f-app-icon.py`；两者均从仓库根目录解析资源路径，并优先导入环境 Pillow、回退到仓库临时运行时。
- 新增 `final/README.md` 与 `final/SOURCE-MANIFEST.json`，记录无参考图 ImageGen 提示词、两次因 24bpp RGB 棋盘格背景而废弃的参考图编辑、母版哈希、七份 PNG、ICO 帧序及所有运行时副本映射。
- 删除三个已无活动引用的旧运行资源：Tauri `src-tauri/icons/icon.ico`、`icon.png` 与 desktop-console `assets/coolzhu-agent-icon.png`；`coolzhu-icons-2026-08-12` 目录仍作为历史资产保留，并在其 README 中标明不属于活动运行链。
- 手工托盘仍是唯一创建链；`build_tray` 使用 `TrayIconEvent::Click` 的左键 `MouseButtonState::Up` 过滤，Enter/Move/Leave/DoubleClick 与右键不会调用 `do_toggle_console`，菜单事件保持不变。
- `scripts/test-package-safety.ps1` 现在解析 `tauri.conf.json` 的 JSON 结构，精确校验 `app.trayIcon` 缺失与 `bundle.icon` 两项顺序/内容，并校验 PNG 实际尺寸、32-bit Alpha、透明角、哈希、ICO 七帧、旧资源不存在及活动代码旧路径负向契约。

### 本轮校正验证记录

- `python scripts/generate-p5f-app-icon.py`：通过，Pillow 12.3.0；母版 SHA256 仍为 `2bfcd2602d8a364235a351dabac84caabc9512e0a17cc62de0a95757ec652fb2`。
- `python scripts/validate-p5f-app-icon.py`：通过；最终 ICO 与 Tauri ICO SHA256 均为 `a8852f903d7f0c5246f9567e59666cc8fef763e0a3e0095a2ce3db26703e4db9`，七帧顺序为 256/128/64/48/32/24/16；final 256px、Tauri PNG、desktop-console PNG SHA256 均为 `890a58bf681bb22009471cf851c0bee6248ee55fb8572e4c6442a9c36f172131`。
- `scripts/test-package-safety.ps1`、manifest JSON 解析、`node --check modules/gui-web/packages/web-console/src/app.js` 与 `git diff --check`：通过；Git 仅报告既有 LF/CRLF 提示。
- `cargo build -p coolzhu-app-launcher --offline` 与 `cargo test -p coolzhu-app-launcher --offline`：通过；26/26 测试通过。
- `cargo build -p coolzhu-desktop-console --offline` 与 `cargo test -p coolzhu-desktop-console --offline`：通过；16/16 测试通过，保留既有 19 条 f32 未来兼容警告。
- `cargo build --manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml --offline`：通过；Tauri `cargo test` 仍仅因既有缺失 `ui/assets/pet-actions/generated-previews-20260617/stabilized-metrics.json` 的 `include_str!` 在测试编译阶段阻断。
- 四个 web 文件的大小与时间戳保持 P5-F 基线：`index.html` 117049 bytes / 2026-08-27 23:23:31.4666766 +08:00，`app.js` 739052 bytes / 2026-08-28 00:32:11.2240251 +08:00，`styles.css` 400098 bytes / 2026-08-28 00:25:25.3258077 +08:00，`main.rs` 2895877 bytes / 2026-08-28 00:32:11.2390726 +08:00。

### P5-G desktop-console PE 图标校正（2026-08-28）

- 新增 `modules/gui-desktop/packages/desktop-console/build.rs`，独立按该 crate 的真实目录层级计算 workspace root，使用 final ICO、Windows `rc.exe`、临时 `.rc/.res` 和 `cargo:rustc-link-arg-bin=coolzhu-desktop-console=...` 将 PE 图标资源嵌入 desktop-console。
- build.rs 在非 Windows 目标安全跳过，同时声明 final ICO、`RC` 与 `WINDOWS_RC` 的 Cargo 重建依赖；资源 ID 为 `COOLZHU_DESKTOP_CONSOLE_APPLICATION_ICON`。
- 保留 `src/main.rs` 中 eframe 的 256px PNG 运行时窗口图标；PE ICO 与运行时 PNG 为互补链路，不改母版及任何派生资源。
- `scripts/test-package-safety.ps1` 增加 desktop-console build.rs 存在性、最终 ICO、资源编译器、目标 bin link arg 契约，并将该 build.rs 纳入活动旧路径负向扫描。
- 只读 PE 资源枚举确认新 `target/debug/coolzhu-desktop-console.exe` 含 `RT_GROUP_ICON` 与 `RT_ICON`；Explorer/桌面视觉显示仍交给真实 Windows 测试会话确认。
- `cargo build -p coolzhu-desktop-console --offline`：通过，触发并编译新增 build.rs；保留既有 19 条 desktop-console f32 未来兼容警告。
- `cargo test -p coolzhu-desktop-console --offline`：通过，16/16 测试通过；同样保留既有 19 条 f32 未来兼容警告。
- `scripts/test-package-safety.ps1` 与 `git diff --check`：通过；只读证据 `tmp/qa-p5g-icon-2026-08-28/pe-resource-enum-after-build.json` 记录 `RT_GROUP_ICON=1`、`RT_ICON=7`、枚举错误码均为 0，PE SHA256 为 `E22AFB7D6BD95F3FADCEBC27A114A856766F78EB158198497FB9FB381C846A29`。
- P5-G 本轮未写入 Launcher/Tauri 构建产物或四个 web 文件；Launcher 与 Tauri debug EXE 的时间戳/哈希保持既有值。共享工作区在本轮开始前已存在 web 文件及相关前端资源的未提交修改，未在本轮回退或覆盖。
