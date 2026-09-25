# 2026-08-14 安装态完整自检、问题修复与发布验证

## 1. 范围与基线

- 仓库：`https://github.com/coolzhulike/coolzhuagent`
- 同步基线：`origin/main@08bf548419c4a2f5ba6f51b833a90539fbc28245`，同步时 ahead/behind 为 `0/0`。
- GitHub 最新公开安装资产：预发布版 `v0.2.4/CoolzhuAgent-0.2.4.msi`；没有 stable Release，也没有 `v0.2.5` tag/Release。
- `v0.2.4` 发布历史与当前 `main` 没有共同祖先，因此只作为卸载/安装基线；功能回归和最终交付均使用从同步后 `main` 重建的 `0.2.5` MSI。
- 测试设备：Windows 11、1280×672 逻辑工作区、250% 桌面缩放；CPU 为 Ryzen 7 6800U，GPU 为 Radeon 680M，约 3 GiB 显存，总内存约 12.75 GiB。
- 模型会话：`GLM-5.2`（Code Agent/工具调用）与 `agnes`（视觉理解）。报告不记录 API Key、Key reference、前后缀或其它可关联凭据材料。

## 2. 安装生命周期基线

1. 卸载原有 `v0.2.4`：卸载注册消失，`127.0.0.1:8765` 释放，用户会话数据按设计保留。
2. 发现卸载后 `C:\Program Files\CoolzhuAgent\bin` 空目录残留，建立 Issue #47。
3. 从 `main@08bf548` 重建并安装基线 `0.2.5` MSI：安装注册、开始菜单快捷方式、关键文件、launcher self-check 和主控制台启动均通过。
4. 安装后 3 个既有模型会话仍可读取，`GLM-5.2` 为 active，`agnes` 已配置；Browser Bridge 扩展连接成功。
5. 最终修复包还需执行一次真实的“安装 → 启动 → 卸载并确认根目录消失 → 再次安装并启动”闭环，结果和哈希在本文末尾补记。

## 3. 完整功能自检结果

| 能力 | 结果 | 证据/现象 |
| --- | --- | --- |
| 安装、快捷方式、关键文件与启动 | PASS | 安装态 launcher self-check 成功；Web Console 可在 `127.0.0.1:8765` 启动。 |
| BASE/Health/stream HTTP 契约 | PASS | 回环地址、无重定向检查通过；核心 API 和流终态正常。 |
| 模型会话持久化 | PASS | 3/3 会话可读取；`GLM-5.2` active，`agnes` configured。 |
| GLM-5.2 Code Agent 工程能力 | PASS | 在隔离工程中读取源码、运行 PowerShell 预检（3/4）、只修改目标实现、再次执行测试（4/4）；README/package/test 文件哈希未变化。 |
| 工具调用与权限 | PASS | 真实 shell/文件工具产生 terminal 结果；rooms、permissions、tools、MCP 只读枚举通过。 |
| 上下文连续性 | PASS | `GLM-5.2` 在后续轮次准确回忆唯一 marker；产品附加 Context usage footer，不影响语义判定。 |
| Memory | PASS | 测试 bead 的 add/query/prompt/delete 全闭环，清理后剩余 0。 |
| Goal | PASS | create/detail/status/cancel 全闭环，测试 Goal 最终为 cancelled。 |
| Browser Bridge | PASS | health connected；本地 fixture 的 slider、drag、key、enter、tab lifecycle 5/5 terminal。tab lifecycle 返回的外部 URL 是恢复后的原用户标签，不是测试导航目标。 |
| Computer Use 预检/坐标稳定性 | PASS | SendInput ready；5 个分辨率 × 20 个场景，共 100 项 dry-run 检查通过。 |
| agnes 截图理解 | PASS | 正确识别本地 Browser Bridge fixture 的标题、`fixture-ready`、Alpha、拖放区和 ready 状态；未使用工具或 fallback。 |
| 本地 ShowUI grounding/闭环点击 | SKIP-ENV | 本机无 NVIDIA/CUDA/ROCm，显存和环境不满足项目本地 ShowUI Windows 路径；模型目录、venv、服务均不存在，8000/8001/7860 未监听。按授权跳过真实 grounding/CU 闭环。 |
| Audio/Realtime readiness | PASS | 状态与后端契约检查通过。物理麦克风/扬声器的人机播放未执行。 |
| 微信/Clawbot 本地状态 | PASS | 本地配置/状态读取正常；为避免测试脚本访问可配置外部地址，sidecar 外部 health 标记为 NOT-RUN。 |
| CLI help/version/agents/skills | PASS（修复后） | 产品名、版本、构建元数据、配置域说明和 Windows 用户 roots 已统一；CLI 独立配置不读取 GUI 会话或凭据。 |
| 短屏/高 DPI 布局 | FAIL → FIXED | 1280×672、250% 下确认任务、自检、记忆、视觉等复杂卡片存在裁切；新增短视口媒体查询并用 EOF/级联顺序契约保护。最终安装态 DOM 复核待最终包补记。 |

只读安装态 runner 共执行 45 项调用，其中 44 项成功；唯一网络层失败是本地 ShowUI 的 8000 端口连接超时，按硬件/环境结论归类为 `SKIP-ENV`，不是产品测试失败。

## 4. 发现并跟踪的问题

| Issue | 问题 | 修复摘要 |
| --- | --- | --- |
| #35 | CLI 品牌、版本、构建信息、模型配置与 agents/skills 语义和 GUI/安装包不一致 | 统一 `COOLZHU CODE Agent` 元数据；明确 CLI/GUI 配置域隔离；实现 `--model > .claw > built-in`；Windows 支持 `CODEX_HOME/USERPROFILE`；打包时注入并校验 MSI/CLI 版本；随包发布 CLI 文档。 |
| #45 | Windows PowerShell 5.1 无法可靠解析无 BOM 的含中文脚本 | 为 11 个相关脚本补 UTF-8 BOM；新增 WinPS 5.1 parser/兼容性测试并纳入 project delivery。 |
| #46 | 250% 缩放、短工作区下 Web Console 部分控件不可达 | 在 CSS 文件末尾增加短视口降级，允许工具栏换行并为任务、自检、记忆、视觉区提供可达滚动；测试校验媒体查询位于后续覆盖规则之后且靠近 EOF。 |
| #47 | MSI 卸载后残留空的安装根/bin 目录 | 在稳定 Launcher component 上声明 CreateFolder/RemoveFolder；新增 WiX XML/XPath 静态契约；最终以真实安装/卸载验证为准。 |
| #48 | 未安装 ShowUI 时 `local_vlm` 假报 ready，且定位路由可能仍先选中该后端等待超时 | readiness 同时检查服务、`/v1/models` 模型匹配和本地模型资源；兼容 Qwen 模型 ID/目录别名；定位前门禁，未就绪立即 skipped。 |
| #49 | 已有 3/3 ready 会话时 functional diagnostics 仍提示 provider/key 未配置 | health 与 functional 共用持久化会话级 readiness 计数；只返回计数，不返回凭据或引用信息。 |
| #50 | 诊断日志记录 API Key/Key reference 的前缀 | 新版本只记录布尔 `reference_present`，不记录任何内容、派生前后缀或指纹；增加源码级回归。 |
| #51 | Windows `core.autocrlf` 使 14 项前端静态契约批量假失败 | 仅在测试输入侧规范化 CRLF/CR 为 LF；ASR 回归不再依赖本机 ffmpeg 的英文错误文本。 |
| #52 | 文档规定执行 `module_linkage_smoke`，但当前 main 缺少该测试目标 | 恢复根级 4 项模块联接 smoke，覆盖 computer-use 锚点、vision parser、server app 构造和 runtime session。 |
| #53 | 安装后的桌面快捷方式没有显示 CZ 应用图标 | WiX 新增 `CoolzhuApplicationIcon`，开始菜单与桌面快捷方式显式绑定应用 ICO；安装态 `IconLocation` 已指向 MSI 提取的应用图标。 |
| #54 | 桌宠退出时未默认关闭 COOLZHU Agent 及其子进程 | Web Console 的 `pet_exit_closes_console` 默认改为 true；Tauri 桌宠关闭、托盘退出和 `quit_app` 统一按 launcher 注入 PID 校验进程名后回收 web-console 进程树。 |
| #55 | Windows 高 DPI/小尺寸下 CZ 图标资源缩放异常 | Tauri build.rs 显式声明 ICO/PNG 依赖，避免增量构建复用旧 PE 资源；包安全契约校验 16/24/32/48/64/128/256 全尺寸。 |

所有问题均有 GitHub Issue；最终 PR 使用 `Fixes #35 #45 #46 #47 #48 #49 #50 #51 #52 #53 #54 #55` 关联。

## 5. 修复后的回归门槛

最终提交前必须全部满足：

- `cargo test -p coolzhu-web-console`：lib 8/8、native-host 1/1、main 821/821 全部通过；
- `cargo build -p coolzhu-web-console --offline`：LLVM-MinGW/GNU release 通过；
- CLI tests、command-router tests/doc-tests 全部通过；
- computer-use 38/38、vision-service 35/35、module linkage smoke 4/4 通过；
- `scripts/test-powershell-script-compat.ps1`、`test-project-delivery.ps1`、`test-package-safety.ps1`、`test-package-manifest.ps1` 全部通过；
- Browser extension Node 测试 6/6，realtime voice capture、STT tail、TTS text 定向测试通过；
- `git diff --check` 通过，当前修改范围无 rustfmt 差异；
- staged package launcher self-check 通过，MSI 反编译包含 WebView2 loader、CLI 文档和卸载目录契约。

## 6. 安全与数据边界

- 测试报告、Issue 和 PR 不粘贴任何模型 API Key、Key reference 内容或可关联前后缀。
- 新构建阻止后续日志继续写入 Key/Key reference 前缀，但旧版本已经生成的历史 `err.log` 不会被安装/升级自动改写或删除；如需共享历史支持包，应另行清理或脱敏旧日志。
- 卸载验证只检查/移除 MSI 自身拥有的空目录，不递归删除用户数据目录。
- ShowUI 环境不满足时只跳过本地视觉 grounding 与闭环坐标点击；Browser Bridge、UIA/SendInput 预检、agnes 视觉理解仍按矩阵执行。

## 7. 最终安装包与交付闭环

最终代码提交、重建和真实安装生命周期已完成：

- 源码提交：`5d39cd5794b03167c2d28e0e7cc1673390393a48`
- MSI：`dist/CoolzhuAgent-0.2.5-20260821-000714.msi`
- 大小：`194,836,260` bytes
- SHA-256：`C6972BBAE51814E0410FA8033C148F55206C7148E66AFA725186AC9CA6C68B3B`
- staged/installed launcher self-check：`ok=true`；安装态 PID 为 web-console `17368`、Tauri `6128`
- 最终安装/启动：PASS；注册版本 `0.2.5`，CLI、3 个会话和 GLM-5.2 active 状态通过
- 最终卸载根目录清理：PASS；卸载后 `C:\Program Files\CoolzhuAgent` 不存在，注册项和 8765 listener 均为 0
- 再次安装/启动：PASS；安装态快捷方式存在，health、sessions、grounding/locate backend 检查通过
- 桌面快捷方式图标：PASS；`IconLocation` 为 MSI 缓存的 `CoolzhuApplicationIcon`，文件 68,275 bytes，SHA 与应用 ICO 母版一致
- 图标资源：PASS；Tauri PE 含 7 项 `GROUP_ICON` 尺寸，安装包 WiX 反编译确认两个快捷方式均绑定 `CoolzhuApplicationIcon`
- 高 DPI/ShowUI：本机 250% 缩放下只完成资源/契约与安装态验证；无 NVIDIA/CUDA/ShowUI 时按授权跳过真实 grounding/闭环点击
- Pull Request：[#56](https://github.com/coolzhulike/coolzhuagent/pull/56)
