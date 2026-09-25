# 2026-08-11/12 安装包与常用能力实测

## 范围

- 发布包：GitHub Release `v0.2.4`（pre-release）
- 源码基线：`9c949ff2f62d1b2d507b9ee63a32ffd7596f5e7f`
- 系统：Windows 11
- 文本模型：GLM-5.2（阿里百炼）
- 视觉模型：agnes（`agnes-2.0-flash`）
- 重点：安装/启动、Code Agent 工程能力、工具与命令行、Computer Use、Browser Use
- 改进 PR：[#37](https://github.com/coolzhulike/coolzhuagent/pull/37)

本文不记录 API Key、Token 或凭据后缀。

## 安装与启动

| 项目 | 结果 | 证据 |
| --- | --- | --- |
| Release 资产完整性 | PASS | v0.2.4 共 8 个资产，下载大小与 GitHub API 一致 |
| MSI SHA256 | PASS | `31A7BEFF28AB3486A61EFBD448B588BCC03AD233D78355ED0A90B3B00A89F7A0` |
| MSI 签名 | WARN | 开发自签名证书，不受本机信任；安装时由用户人工确认 UAC |
| MSI 安装 | PASS | `msiexec` exit 0；详细日志：`tmp/install-v0.2.4-msiexec.log` |
| 启动器自检 | PASS | `ok=true` |
| Web 控制台 | PASS | `http://127.0.0.1:8765/` 返回 200，Tauri 主窗口可见 |
| 会话健康 | PASS | GLM-5.2 与 agnes 均为 ready；视觉 Agent 已选 agnes |

## Code Agent 工程能力

### 场景

在活动工作区内创建隔离的嵌套 Git 工程 `e2e-workspace-20260811`：

- `src/normalize-config.js` 含已知缺陷；
- `test/normalize-config.test.js` 共 4 个用例；
- 基线 `npm.cmd test` 为 3 passed / 1 failed；
- 要求 Agent 只改实现，不改测试、不提交 commit，并在完成后运行测试与检查 diff。

### 首轮结果（安装包 v0.2.4，默认 workspace-write 聊天室）

状态：**FAIL**。

- GLM-5.2 正确调用 `glob_search` 与 4 次 `read_file`，能准确指出字符串 trim、空值回退、tags 去空/去重等缺陷。
- 模型可见工具只有 12 个，只读工具为主，不包含 `write_file`、`edit_file`、`PowerShell` 或 `bash`。
- 模型误用 `computer_use_perform` 尝试执行 `node --test`，被运行时以 `computer_use_room_full_access_required` 阻止。
- 回合结束后嵌套工程 `git status` 仍干净，测试仍为 3 passed / 1 failed。

关键日志：

```text
[TOOL-REG] exposed 12 tools (mode=whitelist, dev_open=false): tools_semantic_dispatch,computer_use_perform,chat_handoff,read_file,glob_search,grep_search,WebFetch,WebSearch,Skill,Sleep,SendUserMessage,StructuredOutput
[TOOL-LOOP] computer_use_perform -> blocked
error_code=computer_use_room_full_access_required
```

根因：`whitelist` 固定只保留 `PermissionMode::ReadOnly`，构造模型请求时没有使用已经传入工具执行循环的 `chat_room_id`。因此聊天室显示 `workspace-write`，但模型永远看不到工作区写工具。

关联 issue：[#33](https://github.com/coolzhulike/coolzhuagent/issues/33)。

### 修复版回归（本地源码 binary，workspace-write 聊天室）

状态：**PARTIAL PASS**。

- 模型可见工具从 12 个增加到 17 个，新增 `write_file`、`edit_file`、`TodoWrite`、`NotebookEdit` 与 `Config`；日志明确记录 `room_permission=workspace-write`。
- GLM-5.2 实际读取 README、实现与测试，并通过 `write_file` 只修改 `src/normalize-config.js`。
- 独立复核 `git diff` 只含该实现文件；`npm.cmd test` 从 3 passed / 1 failed 变为 4 passed / 0 failed。
- workspace-write 按权限设计不暴露 `PowerShell` / `bash`，所以模型不能自行运行命令；命令行能力需要 Full Access 回归。

回归提示中的 `Do not use computer_use_perform for shell commands or file editing` 又暴露了否定意图误判：修复前首轮请求日志为 `tool_choice=computer_use_perform`，模型仍尝试把 Computer Use 当作 shell。关联 issue：[#36](https://github.com/coolzhulike/coolzhuagent/issues/36)。修复否定词识别后，同一提示在新聊天室的真实日志为 `tool_choice=auto`，只调用 `read_file`，未强制 Computer Use。

### Full Access 真实工程回归

状态：**PASS**。

- 聊天室权限已切换为 `full-access`，并记录 `risk_acknowledged=true`、`confirmed_twice=true`。
- GLM-5.2 可见 21 个工具，包含 `read_file`、`write_file`、`edit_file`、`PowerShell`、`bash`、`computer_use_perform` 等；日志为 `room_permission=danger-full-access`、`tool_count=21`、`tool_choice=auto`。
- Agent 先用 PowerShell 跑基线测试，再并行读取实现/测试，调用 `write_file` 修复实现，最后再次运行 PowerShell 测试并检查 diff。
- 独立复核：4/4 tests passed，`git diff --check` PASS，只有 `src/normalize-config.js` 被修改。
- 完成验证后通过补丁恢复嵌套 fixture 到基线，避免留下测试生成改动。

## 命令行

安装包内 `bin/coolzhu-cli.exe` 可启动，但一致性测试失败：

- `--version` 显示 `Claw Code`、版本 `0.2.0`、Build date `2026-03-31`，与 MSI `v0.2.4` 不一致；
- `--help` 显示 `Claw Code CLI v0.2.0`；
- `agents` / `skills` 返回空，未读取 GUI 中已配置的 GLM-5.2、agnes 与技能；
- one-shot prompt 要求 `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY`；
- `/status`、`/version` 只能在交互模式内使用，直接执行会 exit 1。

关联 issue：[#35](https://github.com/coolzhulike/coolzhuagent/issues/35)。

补充环境现象：PowerShell ExecutionPolicy 会拦截 `npm.ps1`，改用 `npm.cmd test` 可正常运行。这是宿主环境差异，不判定为 CoolzhuAgent 缺陷。

## Browser / Browser Bridge

### 内置 Browser

状态：**部分 PASS**。

- 能在主面板 iframe 中打开受控 localhost 页面；
- 点击、下拉选择、复选框与 range 滑块均产生可见状态变化；
- 外部测试控制接口对 iframe 文本 `fill` 出现焦点/剪贴板错误，拖放方法未提供；这两项不作为 CoolzhuAgent 产品能力结论。

### Browser Bridge

状态：**PARTIAL PASS / 扩展加载待即时确认**。

Health 返回：

```json
{
  "connected": false,
  "setup_required": true
}
```

用户授权后执行 `scripts/setup-browser-bridge.ps1`：直接运行被本机 PowerShell ExecutionPolicy 阻止，使用 `-ExecutionPolicy Bypass` 对同一仓库脚本重试后成功。已注册当前用户 Chrome/Edge Native Messaging Host：

- Host：`C:\Program Files\CoolzhuAgent\bin\coolzhu-browser-native-host.exe`
- 扩展 ID：`akpgmkdkaofanikngahmfbhddpppicfi`
- Chrome/Edge manifest 均位于 `%LOCALAPPDATA%\CoolzhuAgent\browser-native-host`
- HKCU Chrome/Edge 注册表项均指向对应 manifest

注册后 Health 仍为 `connected=false`、`setup_required=true`，符合“扩展尚未加载”的预期。浏览器扩展安装属于需要在实际点击前再次确认的动作，故真实 Bridge 自测仍待完成。

同时发现独立缺陷：诊断面板默认指向 `http://127.0.0.1:8877/tests/fixtures/computer-use-browser.html`，但安装后没有 8877 服务，源码与安装目录也没有该 fixture。关联 issue：[#34](https://github.com/coolzhulike/coolzhuagent/issues/34)。

修复分支新增了自带测试页，并先用临时 localhost 服务验证：

- 页面返回 200；
- DOM 含 `fixture-ready`、文本框、Alpha/Beta 选项、range、`drag-me`、`drop-here`；
- range 可从 20 改到 80；
- Ctrl+A 出现 `key=ctrl+a`；
- Enter 出现 `key=enter choice=alpha`。

随后用修复版产品 binary 复核：`/tests/fixtures/computer-use-browser.html` 由 8765 服务自身返回 200，且诊断面板默认 URL 动态变为 `http://127.0.0.1:8765/tests/fixtures/computer-use-browser.html`。

拖拽和真实 tab lifecycle 仍必须在 Browser Bridge 连接后运行产品自测，不能用外部脚本代替。

受控测试页截图证据：[`evidence/browser-bridge-fixture-2026-08-12.png`](evidence/browser-bridge-fixture-2026-08-12.png)。

## Computer Use / 视觉

- workspace-write 聊天室中的 Computer Use 负向权限门：**PASS**。在无 Full Access 时没有执行真实输入，返回单一明确的 `computer_use_room_full_access_required` 终态。
- Full Access 桌面真实输入首轮：**FAIL**。`computer_use_perform` 返回 `succeeded`，但 Notepad 实际写入 objective 的整段尾部，而不是 success criteria 中的精确 marker；关联 issue：[#38](https://github.com/coolzhulike/coolzhuagent/issues/38)。
- 根因是宽泛的 `type ` 文本解析早于 success criteria marker 解析。修复后用新 marker `COOLZHU-CU-FIXED-20260812` 真实重试，独立 UIA/截图观察只出现该 marker：**PASS**。
- agnes 首轮视觉描述准确，但耗时 116.4 秒，且在明确 `Do not use tools` 时仍生成 `computer_use_perform`，错误把 base64 图片当 browser URL；调用被阻止。
- 首层工具意图修复后，模型本身不再请求工具、耗时 16.2 秒，但旧语义旁路仍追加 `computer.drag_select` dry-run 摘要。
- 第二层旁路门控修复后，同一附件最终回归耗时 13.5 秒：描述准确，无模型工具请求、无 `computer.drag_select`、无工具结果摘要，仅保留正常模型任务和视觉附件任务：**PASS**。关联 issue：[#39](https://github.com/coolzhulike/coolzhuagent/issues/39)。

## 本轮修复

1. 构造聊天模型请求时传递 `chat_room_id`，让 `whitelist` 按聊天室权限档位生成工具清单：
   - 无聊天室：ReadOnly；
   - workspace-write：ReadOnly + WorkspaceWrite；
   - full-access：完整 MVP 工具集。
2. 保留现有运行时权限、路径边界和审批门，不把“模型可见”当作“允许执行”。
3. 强化 `computer_use_perform` 描述，禁止模型把它用于 shell、代码执行或文件编辑。
4. 新增并嵌入 `tests/fixtures/computer-use-browser.html`，由 `window.location.origin` 动态生成诊断 URL。
5. 识别 `do not use`、`不要使用`、`禁止调用` 等对 Computer Use 正式入口的否定式提及，避免强制 `tool_choice`。
6. 增加工具权限档位、否定意图、嵌入静态资源和前端接线回归断言。
7. 桌面精确文本输入优先解析 success criteria 中的受边界约束 marker，避免把整段 objective 尾部输入控件。
8. 新增整轮“明确禁止工具”统一门控，覆盖 `Do not use (any) tools` 和常见中文表达，并同时阻断模型工具暴露、Computer Use 强制调用和旧语义工具旁路。

## 源码验证

- `cargo fmt -p coolzhu-web-console -- --check`：PASS。
- `cargo build -p coolzhu-web-console --target x86_64-pc-windows-gnu --offline`：PASS；本机没有 Visual Studio Build Tools，无法执行 MSVC 链接验证，改用隔离的官方 Rust GNU toolchain + llvm-mingw。
- 定向回归：工具权限清单、流式上下文复用、嵌入测试页、前端接线、9 个意图门控测试全部 PASS。
- 全量测试（加入最后一项否定意图修复之前）：8 个 library 测试和 1 个 native-host 测试全部 PASS；主 binary 789/804 PASS、15 FAIL。其中 1 个本次源码精确空格断言已修复并单测 PASS，其余 14 个失败均在同提交的干净基线复现（CSS/JS 源码快照和本机音频解码环境）。干净基线总计 795 PASS / 17 FAIL。
- `node --check src/app.js`、`git diff --check`：PASS。
- 修复版真实 GLM-5.2 Code Agent：workspace-write 文件读取/写入与 Full Access PowerShell 命令调用均 PASS，独立执行测试 4/4 PASS。
- 修复版 Browser Bridge 测试页：产品自身返回 200，诊断 URL 接线 PASS；真实 Bridge 仍未连接。
- Full Access 真实 GLM-5.2 Code Agent：PowerShell 前后测试、文件工具、diff 检查全部 PASS。
- `deterministic_desktop_text_input_prefers_exact_success_marker_over_objective_prose`：PASS，并完成真实 Notepad 回归。
- `explicit_no_tools_vision_report_does_not_enable_or_force_computer_use`：PASS，并完成 agnes 附件回归。
- 最新 GNU debug binary 构建 PASS；健康检查 8 ok / 3 warn / 0 error。warn 为本机视觉 capture、桌宠路径与 WebView2 探测提示，不影响本轮 API/外壳启动。

## 高 DPI / 短工作区界面排查与修复

当前设备的系统桌面缩放为 250%（`AppliedDPI=240`），物理分辨率约为 3200×1800，但应用可使用的逻辑工作区只有 1280×672。排查结论：

- Tauri 控制台窗口原先固定为 1440×900，最小尺寸为 1180×760，并固定放在 `(48, 48)`；最小高度已经比逻辑工作区高 88px，是窗口底部超出屏幕的直接根因。
- 外部浏览器窗口同样使用固定尺寸与位置，没有按显示器工作区限幅。
- Web Console 没有短高度断点；工程栏 `.ide-toolbar` 禁止换行，而父容器隐藏溢出，导致窄侧栏内的按钮或搜索框会被实际裁切。
- 设置页较早定义的两列降级规则被后续更高优先级的 `body.ui-3d .settings-layout` 三列规则覆盖。
- 未发现 DOM `zoom`、全局 `scale` 或 `devicePixelRatio` 二次放大；DPR 只用于 WebGL canvas。聊天、任务、视觉和设置内部的部分滚动是显式设计，但在短工作区中过密。

修复内容：

- 控制台与外部浏览器窗口改为居中创建、降低合理最小尺寸，并使用 Tauri `prevent_overflow_with_margin` 在创建时按当前显示器工作区（扣除任务栏）限制尺寸。
- 工程工具栏允许换行，搜索框占独立整行；设置页在 1200px 以下可靠降为两列，音频卡片跨两列。
- 新增 760px 短高度断点，压缩 Dock、窗口标签、系统栏和视觉操作卡片的固定装饰占比。
- 新增静态回归断言，防止关键短屏规则被后续样式覆盖。

实际浏览器回归在 250% 缩放下得到 `innerWidth=1028`、`innerHeight=554`、`devicePixelRatio=1.5`：工程栏 9 个控件全部在 240px 侧栏边界内，工具栏 `scrollWidth == clientWidth`；设置页为两列、音频卡片跨整行，`body.scrollWidth == clientWidth` 且 `body.scrollHeight == clientHeight`。

关联 issue：[#40](https://github.com/coolzhulike/coolzhuagent/issues/40)。

## 安装器与应用图标设计稿

使用内置 ImageGen 生成两套与 Web Console 深海军蓝、金黄、青蓝像素 HUD 风格一致的透明图标：

- 安装器：大写 `CZ` + 向下部署到终端托盘的符号；
- 安装后的应用：大写 `CZ` + 终端提示符与信号节点。

两套 PNG 均为 1254×1254 RGBA，四角透明、未检出绿色色边；ICO 均包含 16/24/32/48/64/128/256px。32px 缩略图下仍可辨识 `CZ` 与两种用途。应用图标已同步到 Tauri `icons/icon.png` / `icons/icon.ico`，安装器图标已通过 WiX `ARPPRODUCTICON` 接入。完整提示词、去背参数和核验数据见 [`../../design-assets/coolzhu-icons-2026-08-12/README.md`](../../design-assets/coolzhu-icons-2026-08-12/README.md)。

## 本轮新增验证与环境限制

- Browser Extension 契约检查：6/6 PASS；Rust Browser Bridge 定向测试：18/18 PASS。
- `cargo build -p coolzhu-web-console --target x86_64-pc-windows-gnu --target-dir target\\gnu-validation --offline -j1`：PASS，并以新 binary 重启服务。
- 新增 CSS 规则在真实 250% 缩放浏览器中完成运行态验证。
- 使用 Rust GNU toolchain 与 llvm-mingw 完成全 workspace release 构建和 Tauri release 链接；Tauri 壳首次完整构建 14 分 18 秒，接入新图标后的增量构建 2 分 46 秒，均 PASS。
- `git diff --check`：PASS。

## 扩展加载后的 Browser Bridge 闭环

扩展加载后 `/api/computer-use/browser/health` 返回 `connected=true`、`setup_required=false`。首次产品动态测试结果：

- `slider`、`tab_lifecycle`：PASS；
- `drag`：plan 阶段失败，`drop target was not found`；
- `key`（Ctrl+A）、`enter`：动作分别以 `semantic=select_all`、`semantic=page_handled` 成功执行，但 verify 阶段无法看到结果。

根因是扩展快照只收集可交互或显式语义节点；fixture 的拖放目标没有 `data-drop-target`，结果 `<output>` 只有 `aria-live` 而没有 `role`。补上 `data-drop-target="true"` 与 `role="status"`，并增加内嵌 fixture 静态回归后，`slider / drag / key / enter / tab_lifecycle` 五项均进入 `terminal` 且 `ok=true`。关联 issue：[#42](https://github.com/coolzhulike/coolzhuagent/issues/42)。

## Release、MSI 与安装态验证

- workspace release 的 8 个可执行文件与 Tauri release 均通过 GNU/llvm-mingw 完整链接。
- Tauri 高 DPI 壳运行态边界为 `(10, 12)`、`1267×657`，右下角 `(1277, 669)`，完整落在 1280×672 逻辑工作区内；底部状态栏和右侧控件均可见。
- `scripts/build-msi.ps1` 原先在只有仓库本地 .NET SDK 时仍直接启动 WiX apphost，因找不到 `hostfxr.dll` 失败；现改为本地 `dotnet.exe wix.dll` 调用，并增加 PowerShell 解析及契约回归。关联 issue：[#43](https://github.com/coolzhulike/coolzhuagent/issues/43)。
- 初版 0.2.5 MSI 行政安装解包后，后端能启动但 Tauri 报缺少 `WebView2Loader.dll`。根因是发布清单只复制 Tauri EXE；已把该 DLL 作为强制 artifact 发布到 `bin/`，并增加清单回归。关联 issue：[#44](https://github.com/coolzhulike/coolzhuagent/issues/44)。
- 修复后 MSI 行政安装解包 PASS：756 个文件、`WebView2Loader.dll` 1 个（160320 字节）。从该最终文件树启动后，后端健康检查 8 ok / 3 warn / 0 error，桌面壳完整显示，Browser Bridge 已连接，五项动态自测再次 5/5 PASS。
- 生成包：`dist/CoolzhuAgent-0.2.5.msi`，195810816 字节；WiX `5.0.2+aa65968c`；SHA-256 `F9EF1BD30E6F6CB0CF3D00C20D9722CD8AD854EA9CDD46526012A3C9A7DE8E7A`；包安全扫描 756 文件、0 findings、`safe=true`。
- 仓库当前没有代码签名证书，Authenticode 状态为 `NotSigned`，安装器报告明确记录 `signed=false` / `signing_status=unsigned`。

系统已有 0.2.4 为 per-machine 安装。无管理员令牌的静默覆盖升级返回 MSI 1730/1603；已确认是 Windows 管理员权限门而非包内容错误。当前任务会话无法操作 UAC 安全桌面，因此采用 MSI 行政安装解包 + 最终文件树启动完成安装态内容与运行验证；真实覆盖升级仍需用户在 UAC 提示中点击“是”。

## 最终新增验证

- Browser Extension 契约：6/6 PASS。
- Browser Bridge Rust 定向测试：18/18 PASS；fixture 新增语义回归 PASS。
- Browser Bridge 动态测试：修复前 2/5，修复后源码 release 5/5，重建 MSI 最终文件树 5/5。
- 包清单测试、包安全测试、PowerShell build-msi 语法与本地 runtime 契约：PASS。
- MSI 行政安装：exit 0；启动链、Web Console、Tauri、Bridge：PASS。
