# P7-A Release Candidate 实施记录

日期：2026-09-02（Asia/Shanghai）
范围：仅处理发布候选前的状态栏收敛、兼容说明与文档登记；仅做 Haiku 标识兼容修复，不新增强度档位。

## 本轮变更

- 全局底栏收敛为“工作区 / 连接 / 会话”三项；端口与构建号从底栏移入现有“模块自检”诊断健康卡的运行环境元信息，保留既有 `system-port`、`system-build` 更新入口与诊断数据。
- 连接状态沿用既有 `/api/system/info` 刷新节奏：初始/请求中显示“连接中”，成功显示“已连接”，失败显示“未连接”；文字、`aria-label` 与中性/绿色/朱红状态样式同步，未新增轮询或后端接口。
- 补充 Web 静态源契约，约束三项底栏、诊断区端口/构建字段及连接状态实现；更新 P6-E.4h 最终两遍 921/0 与视觉验收记录。
- 新增[供应商模型与 reasoning 兼容性说明](../user-guide/provider-reasoning-compatibility.md)，按 `reasoning_capability_catalog()` 整理 9 个 provider、39 条目录、5 条旧会话 deprecated 与当前新选 34 条，说明 requested/effective、auto 与安全省略边界。
- 在 2026-08-24 总体方案顶部补充当前实现优先、981px 断点及快捷工具归右栏的短注。
- 将内置 Claude Haiku 4.5 目录与 Claw payload 统一到官方 model id `claude-haiku-4-5-20251001`；旧 `claude-haiku-4-5-20251213` 仅作为显式历史 alias，能力接口下发 alias，前端通用索引保留旧会话的 model/reasoning requested 值，出站仍发送官方 ID。

## 实现期检查

- `cargo build -p coolzhu-llm-adapter --offline`：通过（2026-09-02）。
- `cargo build -p coolzhu-web-console --offline`：通过（2026-09-02；保留既有 warnings）。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过（2026-09-02）。
- `git diff --check`：通过（仅报告既有/工作区 LF→CRLF 提示，无 whitespace error）。
- 实现代理未运行正式 `cargo test`、未读取真实密钥；独立测试任务随后执行 P7-B 初轮验收，结果见下节。

## P7-B 初轮验收与后续修复

- Web：921 passed / 0 failed。
- llm-adapter：107 passed / 0 failed，1 ignored（需要真实 vendor 密钥，未因此伪报失败）。
- CLI：1 passed / 0 failed；module linkage：4 passed / 0 failed；Node 语法检查通过。
- 旧 Haiku model id 会话在设置页打开后，`requested=none` 能保持并保存为 `none`，通过旧会话兼容验收；出站 model id 仍由后端 alias 映射到官方 ID。
- 根视觉检查发现两项真实问题：`refreshState()` 成功分支会把稳定性矩阵诊断写成自动聊天气泡；实际嵌套层级下长 Agent 名称按钮 `scrollHeight=44`、`clientHeight=23` 且 overflow 可见，出现两行文字溢出（证据中的 y=46–58 区域）。
- 随后完成最小修复：移除成功连接调试气泡但保留失败提示；将 Agent 按钮、文字与 chevron 的 flex/ellipsis/nowrap/固定尺寸规则合并到当前实际 25px 规则，并补静态源契约。最终使用 workspace/tmp 隔离 target 的 `cargo build -p coolzhu-web-console --offline` 与 Node 检查均通过。
- P7-B 诊断真图进一步发现右栏模块自检头部按钮因继承 column/flex basis 被拉高到约 110px，健康卡状态/计数与端口/构建元信息被同一行挤压而不可读；已仅在 `chat-tool-host-content` 内补 row、常规 36px 按钮与健康卡独立元信息行，长构建值允许换行，未改全局底栏规则，并补少量静态契约。
- 上述诊断布局修复后的离线 web build 与 Node 检查已通过；P7-B2 独立最终复验 Web 922/0，并由根逐张查看最终四张 UI 证据图后确认通过。
- P7-B2 证据边界为 `tmp/qa-p7b2-candidate-test-2026-09-02/` 下的测试日志、browser 几何/交互记录及根审四张最终截图；其中 `runtime/`、`.coolzhu/`、`edge-profile/` 等运行时会话与浏览器配置仅作测试环境，不属于候选提交或视觉结论素材。
- （P7-B2 阶段历史状态）视觉复验现已通过；当时 0.2.9 Release MSI 构建、安装验收及后续 PR 流程仍未完成，本条不代表当前发布状态。

## 后续状态

- （P7-B2 阶段历史状态）候选版本 0.2.9 的 Release 打包、安装与 MSI 独立验收仍待后续任务执行；当时本记录不宣称已打包、已安装或已完成全路线。供应商 reasoning 目录同样未被表述为 9 家真实请求联调完成。
- （P7-C.3 阶段历史状态）0.2.9 Release MSI 已正常升级安装成功；`tmp/qa-p7-installed-2026-09-02/logs/msiexec-upgrade-0.2.8-to-0.2.9-attempt2-2026-09-03.log` 记录 ProductVersion=0.2.9、安装成功或错误状态=0、MainEngineThread returning 0。安装后的 10 个 binary SHA-256 已与 `tmp/package-reports/package-report-release-20260903-011838146.json` 中的候选值逐项匹配。普通消息/SSE 真实 UI 短轮使用隔离 `127.0.0.1:18766` fixture 已通过，相关请求记录为 `tmp/qa-p7-installed-2026-09-02/fixture/mock-llm-requests.jsonl`，真实截图为 `tmp/qa-p7-installed-2026-09-02/screenshots/installed-p7-local-message-sky-2026-09-03.png`。此前用户恢复 0.2.9 后的左右栏收展与任务中心右停靠也通过，稳定图为 `tmp/qa-p7-installed-2026-09-02/screenshots/installed-p7c4-left-collapsed-stable-sky-2026-09-03.png`；该图仅是 0.2.9 历史证据，不作为 0.2.10 最终证据。独立 Browser 小轮因工具不可用未执行。

## P7-C WebView2 发布前自检修正

- 根验收发现标准 EdgeWebView `Application` 根目录下实际使用一层版本目录（例如 `Application\\151...\\msedgewebview2.exe`），旧自检只检查根目录 exe，且以 `exists()` 判断会把目录误报为运行库。
- 已在 `main.rs` 保留 `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` 明确覆盖入口，补充已知 `Application` 根目录的一层版本子目录候选（不做全盘或递归扫描），并将运行库命中条件收紧为真实文件 `is_file()`；加入版本目录存在/缺失与目录不应命中的最小源测试，测试未由本实现代理执行。
- 2026-09-02 独立测试日志 `tmp/qa-p7c1-webview2-test-2026-09-02/logs/cargo-test-web-console-p7c1.log` 已完成 925 passed / 0 failed / exit 0。
- 2026-09-03 C.1 独立运行态证据目录为 `tmp/qa-p7c1-webview2-test-2026-09-03/`：`api/diagnostics-health.json` 中 `desktop.webview2=ok`，命中 `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\152.0.4191.53\msedgewebview2.exe`；`screenshots/module-self-check-1280x720.png`（1280x720，506740 bytes，SHA-256 `02E3EA2B1AA0AC9AD4A9A2B054AAB86FAFAF55131C5498B5584B7B779C02D685`）由根直接 view 复验通过，36px 按钮、端口/构建信息可读且无自动 debug 泡。`logs/p7c1-webview2-evidence-2026-09-03.txt` 已记录 served HTML/JS/CSS 200 且源 hash 一致、owned PID 1280 已不存在、18765 已释放及 8765 只读内核无监听；cleanup 已由独立会话归档并通过。
- 本项修改后的 `cargo build -p coolzhu-web-console --offline` 已通过（exit 0，2026-09-02；保留既有 59 条编译 warning）；构建由实现会话验证，正式测试由独立测试会话完成。
- 原候选 commit `e4371e39955762bb76c661aeabf1bdf77274846e` 已创建；本项自检修正随后以增补 commit 进入打包，构建结果见 P7-C.2。

## P7-C.2 0.2.9 Release MSI 构建

- 增补 source commit 为 `56d29f978a970938ae83eb6ca8a7871457581bbd`（短 SHA：`56d29f9`）；按 `scripts/build-msi.ps1 -Version 0.2.9 -Configuration release` 使用指定 staging 构建成功，Release build target 为 `x86_64-pc-windows-msvc`。
- 0.2.9 MSI 位于 `dist/CoolzhuAgent-0.2.9.msi`，大小 196385227 bytes，SHA-256 为 `BF5718943DBA91537E05A4321B85DC54C5678C741E9BE20F200923CE6A4D41D7`；installer report 为 `dist/CoolzhuAgent-0.2.9-installer-report.json`，package safety report 为 `dist/CoolzhuAgent-0.2.9-package-safety.json`，报告显示 803 files、`safe=true`、`findings=[]`。
- 完整构建日志为 `tmp/p7-release-preflight-2026-09-03/build-msi-0.2.9-release.log`，package report 为 `tmp/package-reports/package-report-release-20260903-011838146.json`；staging 为 `tmp/p7-release-preflight-2026-09-02/staging/package-0.2.9-release`。staged CLI `--version` 已核验 `0.2.9`、source commit 同上、target 同上。
- staged CLI `--version` 报告的 build date 为 `2026-09-02`（UTC；installer report 仅含 `generated_at`，不含 build date 字段）；MSI 当前未签名，`signed=false`、`signing_status=unsigned`。本条仅记录构建阶段本实现会话未安装或启动应用；后续 C.3 attempt 2 的实际安装结果见“后续状态”。

## P7-C.4 Tauri 桌面壳可见性修正

- 普通无参或 `--show-console` 启动默认只显示控制台；明确 `--pet`、`--show-pet`、`--pet-action` 或 `--pet-event` 才显示桌宠，显式同时要求控制台与桌宠时保留两者。桌宠窗口创建后不再隐式 `show()`，避免启动可见性契约被覆盖。
- 原生托盘新增“隐藏桌宠”，仅隐藏桌宠窗口，不退出进程、不终止控制台、不改变控制台可见性；既有“显示桌宠”、拖动、动画和生命周期行为保留。
- `--pet-state` 现作为被动状态更新：仅向已有桌宠发送状态事件，不强制显示/唤起已隐藏桌宠，也不弹出控制台；与显式桌宠意图组合时仍按显式意图显示。该行为通过源契约测试覆盖。
- `pet-theme.json` 增加 `blink` 瞬时信号配置，复用现有 8 张 `idle-{index}.png`；不改现有 `asset_version`、`pet-mini.html` 的闭眼覆盖逻辑或任何 PNG，修复 audio 事件被 Rust 归一化为 idle 的实际链路缺口。
- 首次独立 Tauri 测试编译曾被历史 `generated-previews-20260617/stabilized-metrics.json` 缺失阻断。已确认该原件不在 tracked/可达或悬空 Git 历史、现有项目源及 tmp 交付目录；未运行会删除当前动画 PNG 的稳定化脚本。为让其余测试真实编译执行，仅将测试专用 `include_str!` 改为测试期读取，缺失时报告明确路径；保留 3 个原 metrics 测试及全部断言，不 skip、ignore 或提供默认数据，因此这 3 项仍会如实报告缺 fixture。
- 独立 Tauri 全套基线记录于 `tmp/qa-p7-installed-2026-09-02/logs/cargo-test-tauri-shell-c4-full-2026-09-03.log`：36 passed / 10 failed / 0 ignored。失败分类为 3 个 CRLF 源文本契约（`quit_app`、`report_throne_zone`、diagnostics count）、3 个过时 blink overlay 契约、1 个 audio→blink 数据映射缺口及 3 个缺失历史 metrics fixture；本轮仅分别做测试期换行规范化、按现有 8 帧 overlay/PNG 锚点更新契约及 theme 数据修复，未伪造 metrics 结果。
- 独立修复后 Tauri 全套记录于 `tmp/qa-p7-installed-2026-09-02/logs/cargo-test-tauri-shell-c4-final-2026-09-03.log`：43 passed / 3 failed / 0 ignored，exit 101；3 个失败均为缺失历史 `stabilized-metrics.json` fixture（`pet_martial_frames_keep_character_scale_consistent_with_idle`、`pet_success_frames_do_not_mix_closeup_character_scales`、`pet_wuxia_frames_keep_policy_stable_scale_and_anchors`）。C.4 启动/托盘/被动 state 行为、audio→blink 映射、8 帧 overlay 与实际 PNG 几何契约均通过；测试编译保留两个 test-only dead-code warning，未因此改测或生产代码。
- `cargo fmt --manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml -- --check`：通过；`git diff --check`：通过（仅 LF→CRLF 提示）。`cargo build --manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml --offline`：通过，22.74s；日志为 `tmp/p7-release-preflight-2026-09-03/p7c4-tauri-build-after-blink-fix.log`。当前 `main.rs` SHA-256 为 `1F424ABAFA3179C4BCD0A7516F699AE3A0B2250FD55819B3AEB9F7D2298D5650`，`pet-theme.json` SHA-256 为 `5F177647AD33E472A5EC6921AC33A14DDC50E53A61842A10168B68EF73A35DE3`；全套不宣称绿色，3 个历史 metrics fixture 缺失保持为明确失败。

## P7-C.5 0.2.10 Release MSI 构建

- 已由 source commit `72446dd9fbf0cff0d14f82cb5329835f79f6d432` 执行 `scripts/build-msi.ps1 -Version 0.2.10 -Configuration release`，build-msi exit 0；Release build target 为 `x86_64-pc-windows-msvc`，staging 为 `tmp/p7-release-preflight-2026-09-03/staging/package-0.2.10-release`。
- 0.2.10 MSI 位于 `dist/CoolzhuAgent-0.2.10.msi`，大小 196393420 bytes，SHA-256 为 `22C018A4DCDC8448A694BD34DBF803E50EE7DD8628E0915D4D1F59F619246487`；installer report 为 `dist/CoolzhuAgent-0.2.10-installer-report.json`，package safety report 为 `dist/CoolzhuAgent-0.2.10-package-safety.json`，报告显示 803 files、`safe=true`、`findings=[]`，签名状态为 unsigned。
- 完整构建日志为 `tmp/p7-release-preflight-2026-09-03/build-msi-0.2.10-release.log`，package report 为 `tmp/package-reports/package-report-release-20260903-082404084.json`。staged CLI 纯 stdout 证据为 `tmp/p7-release-preflight-2026-09-03/staged-cli-version-0.2.10.stdout.txt`，核验 `Version=0.2.10`、`Git SHA=72446dd9fbf0cff0d14f82cb5329835f79f6d432`、target 同上、build date=`2026-09-03`（UTC；日期来自 staged CLI，不是 installer report）。
- MSI 只读属性证据为 `tmp/p7-release-preflight-2026-09-03/msi-properties-0.2.10-readonly.txt`：ProductVersion=`0.2.10`，ProductCode=`{B422037C-FCBE-4D18-8A10-0B9FFA3F6EC2}`，UpgradeCode=`{7873714F-87EE-4DFA-8AAB-2A2402B4ABEA}`。P7-C.5 attempt 2 verbose 安装日志 `tmp/qa-p7-installed-0.2.10-2026-09-03/logs/msiexec-upgrade-attempt2-2026-09-03.log`（22:40:03）记录 ProductVersion=`0.2.10`、status=`0`、MainEngineThread returning `0`；安装核验汇总为 `tmp/qa-p7-installed-0.2.10-2026-09-03/logs/p7c5-install-verification-2026-09-03.log`，其中注册表版本为 `0.2.10`，已安装 10 个 binary 的 SHA-256 与 `tmp/package-reports/package-report-release-20260903-082404084.json` 逐项匹配。
- P7-C.5 隔离实装证据目录为 `tmp/qa-p7-installed-0.2.10-2026-09-03/`。默认原生主窗仅启动 1 个 Tauri、无桌宠通过：`screenshots/p7c5-0.2.10-default-main-window-sky-2026-09-04.png`（1267x657，446331 bytes，SHA-256 `2633F394FB3F7B618050B8311F8BC63388F41CE76AD38AF2BCCDD94BFFB3FE7C`）。右栏收起后中央扩展且左栏可用通过：`screenshots/p7c5-layout-left-open-right-collapsed-sky-2026-09-04.png`（507411 bytes，SHA-256 `0C426E18CBE91C27EAE282A742FC4481C4910D6964FEA34469EB1D5269E4FB09`）。左快捷任务中心转右工具且不覆盖中央通过：`screenshots/p7c5-left-task-center-right-tool-sky-2026-09-04.png`（534172 bytes，SHA-256 `131B5BEBFEB4D4AAA0D3EB7D297F9640F1C5ED1BE20D0459BAB2B7FD98DF4FBC`）。
- Custom `127.0.0.1:18766` fixture 的唯一普通消息/SSE 请求通过，`stream=true`、custom model、无外联且结束后无桌宠：`screenshots/p7c5-local-sse-no-pet-task-center-sky-2026-09-04.png`（608449 bytes，SHA-256 `C2F31BD92473F1E6556FA1F2E6CAF7D61B1DE75C95781DD45DF38EFA8437ABC3`）；请求记录 `fixture/mock-llm-requests.jsonl` SHA-256 为 `5BCB592DBC2E9B34E13CD2B833DBB91AFFEEDE2347446DE981DB65BDF764DF9E`，且恰 1 条请求。旧 Haiku UI 保存 `none` 与 API 回读通过：raw `claude-haiku-4-5-20251213` 保留、canonical `claude-haiku-4-5-20251001` 能力 alias、`requested/effective=none`、status=`exact`、Anthropic strategy/protocol 正确、无消息无外联；截图 `screenshots/p7c5-old-haiku-none-settings-sky-2026-09-04.png`（676364 bytes，SHA-256 `55BEE68FED2F91625EF3A3E6F4CDBDA15A8F6189C1A36E98940D4411B79FAEBB`），API 证据 `api/p7c5-old-haiku-none-evidence.json` SHA-256 `B8485C6BFA1E8C1B78DA7E37A4F3BBB6182C1D4E8815193AEC7167048E5AE6BE`。
- 只读桌面快捷方式文件核查发现 `C:\Users\Public\Desktop\COOLZHU CODE Agent.lnk` 的 target 为 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，IconLocation 为 `C:\WINDOWS\Installer\{B422037C-FCBE-4D18-8A10-0B9FFA3F6EC2}\CoolzhuApplicationIcon,0`；缓存 ICO、最终 CZ ICO 与 Tauri ICO 的 SHA-256 均为 `A8852F903D7F0C5246F9567E59666CC8FEF763E0A3E0095A2CE3DB26703E4DB9`。这是快捷方式/图标文件级证据，不替代桌面截图或视觉验收。
- 桌面视觉仍为 `NOT_CAPTURED`：Sky 不暴露 desktop，Snipping 两次只抓到 Codex 且未另存；用户需手动补传真实桌面图。公共 `.lnk` target/IconLocation 与最终 CZ/Tauri ICO hash 的文件证据已通过，但不替代桌面视觉验收。
- Tauri 最终 43 passed / 3 failed / 0 ignored（3 个历史 metrics fixture 缺失）保持已知限制；上述隔离实装五项已通过，桌面视觉证据仍待用户补传。

## P7-C.5 全控件与后端对齐验收（2026-09-05，进行中）

- 真实模型验收使用已配置的 GLM-5.2 与 agnes，不以 Haiku 代替：GLM-5.2 文本短轮已成功，原生证据为 `tmp/qa-p7-installed-0.2.10-2026-09-03/screenshots/p7c5-real-glm-5.2-text-success-sky-2026-09-04.png`；agnes 对自建 QA 聊天室中的截图附件给出了对应界面描述，Web 证据为 `tmp/qa-p7-control-alignment-2026-09-05/01-agnes-web-ui-browser-cua.png`。agnes 返回消息未提供完整执行来源字段，因此仅记录未观察到回退提示，不宣称已由全链路字段证明无回退。本轮恢复后未重复这些已成功请求。
- 修复前控件库存包含 248 个静态原生交互元素、111 组 `data-action`；按真实源码已分为 93 组后端 route/method/handler 核验、12 组纯前端、6 组浏览器/Tauri 宿主分支。这是静态链路分类，不是测试通过数；动态生成控件和分隔线仍在独立补漏。
- 自建 QA 会话/房间的创建、保存、激活、分叉、空历史重置、权限读取等有接口证据；项目文件、搜索、记忆、MCP、音频及视觉 readiness 已做有限检查。详细证据为 `tmp/qa-p7-control-alignment-2026-09-05/07-request-evidence.json`，区分 45 条本轮请求与 14 条此前变更记录。历史未捕获的 HTTP 状态和时刻保持 null，不反推或补造。
- 项目测试曾临时切换到 tmp fixture，随后恢复原工作区、主聊天室与 GLM-5.2；这包含状态变更，不能称纯只读。fixture 没有独立 Git 根，worktree diff 可能解析父仓库，因此不计为隔离 diff PASS。空 QA 房间附件真实为 `total=0/items.length=0`，全局附件索引数量不混入房间结果。
- 当前依赖状态：浏览器桥接未连接，STT 不可用，MCP 服务尚未连接，部分视觉接口为预留/未运行。HTTP 200 和接口存在不代表这些功能已实装通过；未通过自动提权、安装服务或连接第三方来消除依赖限制。
- 主任务只读截图与 DOM 几何确认 1280×720 双侧栏下中央为 418 px，但用户消息正文仅约 98 px、助手正文约 158 px，属于真实窄正文布局问题；截图与审核在 `tmp/qa-p7-control-alignment-2026-09-05/07-supervisor-current-web-layout.png` 和同目录 `07-supervisor-layout-review.md`。另有会话选择器仅 click 的键盘缺口，以及工具目录按条目存在假报可用的静态问题，已交代码会话进行最小修正，尚未在本条记录中宣称修复完成。
- 独立测试会话的浏览器 provider 当前不可用，Windows 原生通道亦不可用；主任务 Web 只读审核图不能冒充独立 GUI 操作、Tauri 或桌面图标截图。全控件验收、修后安装包与最终 PR 仍未闭环。

## P7-C.5 控件修正与候选复验（2026-09-06，进行中）

- 已取消并删除五小时启动定时 `5-p7-c-5`；本轮由用户手动继续，未创建替代定时，未使用新的额度恢复券。
- 实现会话已将会话选择器及选项改为原生 button，补充展开状态 ARIA、Escape 关闭与选中后回焦；局部样式恢复普通、悬停、焦点和选中金色，选中项悬停不再被全局 button 样式覆盖。主任务已审核源码，但键盘实际操作和选中态截图仍待独立 GUI 验收。
- 消息列表新增基于容器宽度的 560px 紧凑布局：头像 28px，正文单独一列，时间移到正文下一行；宽区保留既有三列及金、玉、蓝透明光泽。工具目录状态改为按 `executable_now === true` 计数，空目录、全部不可执行、部分可执行分开显示；“计算资源”改为“电脑操作”，按钮保持“管理”，不再用目录存在假报可执行。
- 最终候选离线构建 exit 0，二进制 SHA-256 为 `187F86CC0A92D674A3233C8E4D72F33DC86066E64E5641B05A114A45F5D0D26E`。独立最终 Web 测试为 8 + 1 + 925 = 934 passed / 0 failed，exit 0；Node 语法检查与临时工具三态执行测试均 exit 0。证据在 `tmp/qa-p7-control-alignment-2026-09-06/12-web-console-build.*`、`13-final-*`，不以早先陈旧 exit 文件替代最终日志。
- 隔离候选 `127.0.0.1:18765` 使用自建样例会话与四条合成消息，真实 LLM 关闭。独立测试保存原始 1280×720 截图 `14-layout-bottom.png`、`15-layout-top.png`，主任务逐张查看确认宽区消息可读与颜色层次保留；两图均为左栏收起、右栏展开，不能替代双栏展开的窄区验收。Rust 围栏目前是普通文本，本轮未将其记为 Markdown 渲染通过。
- 双栏展开顶部/底部及右栏收起的三张补图尚未取得：独立测试续轮时 CUA 返回无浏览器，原生 Sky 通道亦未恢复。候选服务与测试数据保留，未使用其他截图通道绕过限制；主任务请求重新打开候选预览仅返回 queued，不能宣称连接恢复。后续须先补这三张图及真实控件操作，再进入修后打包安装与 PR。
- 新增仓库内可重复测试入口 `modules/gui-web/packages/web-console/tests/ui-control-contracts.cjs`。主任务审核后由独立测试执行 `node modules/gui-web/packages/web-console/tests/ui-control-contracts.cjs`，真实 exit 0、stderr 空；前三项在 DOM stub 上执行真实渲染函数验证工具三态，第四项仅作会话与窄布局的源契约断言，均不冒充 GUI 操作。原始日志为同证据目录的 `19-ui-control-contracts.stdout.log`、`.stderr.log`、`.exit.txt`。
- 独立代码会话补齐14个遗漏动态控件族的源码链路，记录于同证据目录 `20-inventory-gap-mapping.md`，逐项标明前端状态、读取、写入、删除、系统选择器和模型进程依赖。主任务抽核真实路由与 IDE 关闭处理，校正“关闭未保存标签”并非写盘，并将后续用例限制在常用成功路径及必要的取消/恢复。此为 STATIC 映射补充，未改旧库存，不计14项 GUI 通过。

## P7-C.5 三项 Tauri 失败修复（2026-09-06）

- 用户要求修复未通过项。根查明：三项测试依赖 `generated-previews-20260617/stabilized-metrics.json`，而 `scripts/project-delivery.ps1:37` 明确排除该临时预览目录；历史 metrics 与生成 manifest 均缺失，不能从最终 PNG 假造生成时 scale 数值。
- 代码会话 `p7c5_code_r4` 仅修改 Tauri `src/main.rs` 的 `#[cfg(test)]` 模块：移除历史 JSON 依赖，按旧 Python `face_bbox` 算法读取当前 `pet_action_frames` 素材，保留肤色条件、8邻域、边界坐标、候选选择以及原 1.08 / 0.08 / 1.05 比例阈值。未识别到脸部则明确失败；未删除或忽略原三项测试，未放宽当前 PNG 的其他几何断言。
- 生成过程的 uniform_fit/face_scale 部分改为已跟踪生成脚本的限定区段源码契约，明确只证明当前生成策略；当前 PNG 的脸部比例及其他几何属性直接测量。没有声称恢复历史 scale_min/max，也不将源码策略或像素测量冒充 GUI 验收。未改 PNG、主题、交付过滤、生产窗口逻辑或后端业务。
- 主任务审核算法及全部差异后，由原独立测试会话 `p7c5_test_r3` 执行三个原失败用例：每项 1 passed / 0 failed / exit 0。再执行 Tauri 完整套件：**46 passed / 0 failed / 0 ignored，exit 0**，用时1.89秒。此前43/3属于历史基线，本节取代其作为当前源码的最终结果，但不反改历史日志。
- 独立 Python 基线与 Rust `--nocapture` 日志的24帧路径、bbox、宽高完全一致，`matched=24/mismatches=0`；武术帧内比例45/42≈1.07143，与idle中点偏差≈0.02247，成功帧比例1.0。原始证据在 `tmp/qa-p7-control-alignment-2026-09-06/21-current-pet-face-baseline.json`、`24-python-rust-parity.json`、`24-pet_*.log`、`25-tauri-full.log` 与对应退出码文件。
- 测试代码离线 `cargo build --tests`、fmt及差异检查通过；24张基线PNG哈希不变。实现构建和hash证据为同目录 `23-*`；根审核的 Tauri 源文件 SHA-256 为 `A0BD83F354C4B3A15E9F2B230202231D803CAA90B4AD2A4713B903E02205D536`。本轮未重复Web934项或已成功的真实模型请求。
- Computer Use更新至26.901.51231后，独立测试再次检查仍为 CUA `apps: [] / browsers: []`、Sky `Trusted RPC service is not configured: sky`。18765隔离候选服务正常；新GUI截图、会话键盘和金色选中态操作仍受阻，不能称每个控件均已通过。当前修正尚未打包安装，尚未提交/推送PR，五小时定时仍已取消。

## P7-C.6 用户调整发布顺序（2026-09-06）

- 用户明确要求“先打包安装提交PR”。据此先将已审核修正纳入0.2.11 Release候选、安装验证并提交PR；GUI截图与全控件验收缺口继续记录，不再作为创建PR的前置条件，也不因此宣称它们已完成。
- 源码提交仅纳入本轮已审核的Web选择器/消息布局/工具状态修正、可重复控件测试、Tauri测量测试修复及本工作日志。不提交tmp、运行时会话、凭据、安装包或截图；不自动合并PR，不强推分支。
- 后续安装结果、产物hash和PR链接须以实际输出补记。本条仅记录发布顺序变更，不代表安装或PR已完成。

### 0.2.11 实际构建与安装尝试

- 已审核源码提交为 `a00f52428397c651f1c397508490b6934a2d12b9`，执行Release完整打包，build-msi exit0。产物 `dist/CoolzhuAgent-0.2.11.msi` 为196401612 bytes，SHA-256 `A2CAB83F70EBFFAB66069FD50EA6B7FD9B5EB6CC7B56C3AE481149E3F19D6CC4`，803 files、safe=true、findings为空、unsigned；旧0.2.10产物保留。
- 构建证据为 `tmp/p7-release-preflight-2026-09-06/build-msi-0.2.11-release.log`、对应exit文件及 `tmp/package-reports/package-report-release-20260906-121303233.json`。staged CLI为0.2.11、source a00f524、target x86_64-pc-windows-msvc、build date 2026-09-06。
- 独立测试会话核验staging全部10个artifact哈希一致，安装目录无运行中的用户实例后，针对该批准MSI请求一次RunAs升级。用户取消UAC，helper退出1，MSI实际exit_code=null，错误为 `The operation was canceled by the user.`；未启动msiexec，未产生安装日志，不自动重试或绕过确认。
- 取消后只读核对：旧ProductCode `{B422037C-FCBE-4D18-8A10-0B9FFA3F6EC2}` / 0.2.10仍在，新0.2.11条目不存在，已装CLI仍为0.2.10。证据为 `tmp/qa-p7-installed-0.2.11-2026-09-06/01-0.2.11-preflight.log`、`02-msiexec-0.2.11-result.json`、`03-post-cancel-readonly-state.log`。0.2.11安装及安装后验收保持未完成。
- 继续执行用户已授权的PR提交，正文披露安装取消和GUI缺口；这不代表用户再次批准弹出UAC。PR只推送到已认证的coolzhuNo19分叉，再向coolzhulike/coolzhuagent的main提出，不强推、不自动合并。
