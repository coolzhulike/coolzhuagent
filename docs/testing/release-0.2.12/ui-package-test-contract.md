# 2026-09-19 P3/P5 导航布局与安装资源定向测试报告

> 本文件为源码契约与建议测试附件。发布已完成，12项内嵌字节、427项外置assets和15项HTTP资源已通过安装后核验；详见[发布主报告](../../2026-09-19-release-0.2.12-change-and-test-report.md)。下文“发布待补”等保留审阅时的检查清单语义，不代表当前发布仍未完成。

用途：供接手模型验证本轮界面改动与安装包是否一致。此报告只读审查源码、已有验收证据并生成资源指纹；没有执行打包、安装、卸载或修改产品代码。安装后的结果应由实际发布任务补入，不能将开发环境通过写成安装版通过。

仓库：`C:\Users\zhupu\Desktop\coolzhuagent`。以下代码路径相对此仓库。阅读大文件采用分段输出。审查时 `index.html` 122689 字节，`app.js` 765343 字节，`styles.css` 455926 字节，`main.rs` 3365760 字节。

## 一、当前实现与资源加载契约

### 1. 网页启动顺序

`modules/gui-web/packages/web-console/index.html:8` 起，CSS 必须依次加载：

1. `src/styles.css`：历史完整主题与页面几何规则。
2. `src/chat_experience.css`：隐藏旧左聊天栏、环境下拉、思考/工具状态区、记录/用量面板。
3. `src/model_settings.css`：统一模型参数表单与响应式规则。
4. `src/wuxia_layout.css`：最后加载，实施玉石竹林整体布局与具体子页纠偏。

头部脚本均为 `defer`，顺序为 `chat_experience.js` → `model_settings.js` → 三个语音辅助脚本 → `app.js`。前两者先定义全局模块，`app.js` 再初始化、绑定真实业务状态。尾部 `assets/bamboo-banner-wind.js` 为 `type="module"`，属于外置资产。

可直接观察：`window.CoolzhuChatExperience` 与 `window.CoolzhuModelSettings` 存在；旧 Provider 表单不再作为可见主配置入口；四张样式表都成功响应；最后一张必须是 `wuxia_layout.css`。

### 2. 静态资源实际优先级

`src/main.rs:24875` 的 `serve_static_path` 先读磁盘，全部未命中才调用 `embedded_static_file`。不是始终只返回内嵌前端。

- 设置非空 `COOLZHU_WEB_STATIC_ROOT`：只在该目录查磁盘，然后回退内嵌；不会再查安装根、当前目录或源码目录。
- 未设置时依次查：运行 EXE 的 `bin` 父目录下 `modules/gui-web/packages/web-console` → 当前工作目录同一路径 → 编译时 `CARGO_MANIFEST_DIR` 指向的源码目录 → 内嵌。
- `index.html`、`app.js`、旧 CSS、`chat_experience.js/css`、`model_settings.js/css`、`wuxia_layout.css`、语音辅助脚本和浏览器验证夹具已有 `include_bytes!` 覆盖。
- 玉石 SVG/PNG、头像、竹林 PNG、尾部竹叶脚本没有统一内嵌兜底，需由 `assets` 发布。
- HTML/CSS/JS 返回 UTF-8 Content-Type；静态成功响应使用 `Cache-Control: no-cache, no-store, must-revalidate`。

项目旧说明中“所有静态资源只在编译期内嵌”的简化描述不能作为本次发布验证依据。重新构建仍然必要，它更新内嵌副本；安装目录里旧文件的优先级仍高于新内嵌副本。

**新增页面覆盖结论：已覆盖全部本轮新增页面所需的 HTML/JS/CSS，不存在已发现的新增页面内嵌遗漏。** 搜索/用量及设置宿主都在内嵌 `index.html` 中；环境下拉/状态/索引在内嵌 `chat_experience.js/css`，统一参数表单在内嵌 `model_settings.js/css`，整体美化在内嵌 `wuxia_layout.css`，业务调用入口在内嵌 `app.js`。两个新增JS无额外模块 import；新增CSS没有额外字体包，`wuxia_layout.css` 的两处背景URL都指向已存在的月夜竹林PNG。它们引用的图标、头像和竹林仍走外置assets，不能称“所有引用资源也全部内嵌”。

审查时已经只读核实 `C:\Program Files\CoolzhuAgent\modules\gui-web\packages\web-console` 仅含 `assets` 与 `tools`，没有旧 `index.html` 或 `src`。所以“旧磁盘启动文件覆盖”是升级回归检查项，**不是本机当前已复现缺陷**。主发布任务正在构建 Release 0.2.12，决定保持现有manifest发布结构；本报告不要求为本次发布另行复制源码目录。

## 二、P3 导航实际入口与可观察预期

| 编号 | 入口 / 精确选择器 | 操作与预期 | 应记录的证据 |
| --- | --- | --- | --- |
| NAV-01 | `.workbench-quick-rail`、`.chat-left-rail`、`.chat-rail-resizer-left`、`.chat-left-rail-toggle` | 只有左图标轨道；旧展开聊天栏、左分隔条与左栏开合按钮不可见。恢复旧 localStorage 布局后也不能重新出现左栏。 | 左栏计算样式 `display:none`；`.chat-window-panel` 的 `data-left-state=closed`；刷新前后截图。 |
| NAV-02 | `[data-window-target="project|tasks|terminal|browser|settings"]` | 左轨固定入口依次将相应真实窗口移入 `[data-role="chat-tool-host-content"]`。中央聊天持续存在；不是复制一套失去事件绑定的占位窗口。 | `.workbench-stage` 与宿主中对应 `[data-window-id]` 总数仍为 1；`data-active-window=chat`、`data-active-tool` 与目标相符。 |
| NAV-03 | `[data-role="window-dock-more"]`、`[data-window-target="clawbot|memory|vision"]` | “更多”可展开，三个页面均能打开；打开工具后更多菜单关闭。 | 页头名称正确、目标节点唯一、无不可见遮挡层。 |
| NAV-04 | `[data-window-target="history"]`、`[data-action="chat-history-open"]`、`[data-window-target="usage"]` | 搜索/索引和 Token 用量在右扩展栏打开。Ctrl/Meta+Shift+F 打开记录并聚焦 `[data-role="chat-history-query"]`。 | 入口实际点击、搜索框焦点、工具标题和数据。 |
| NAV-05 | `.chat-right-rail.has-tool-view` | 工具态只显示内部 `.chat-tool-host-heading`：功能名、返回、关闭。外层 `.chat-right-rail-heading`、`.chat-right-tabs`、`.chat-right-rail-footer` 隐藏。 | 三个外层容器 `display:none`；内部标题、两个按钮可见。 |
| NAV-06 | `[data-action="chat-tool-back"]` / `[data-action="chat-tool-close"]` | 返回恢复此前协作页签并把焦点交回页签/开栏按钮；关闭把焦点交回消息输入。两者均还原打开工具前的扩展栏/专注布局快照。不可把“关闭”一概断言为必须隐藏右栏。 | 原页签/布局恢复，工具节点移回原位，焦点目标正确。 |
| NAV-07 | `[data-action="chat-right-rail-toggle"]`、`[data-action="chat-focus-layout"]` | 开合扩展栏和专注模式仍工作。工具打开时隐藏右栏会关闭工具并丢弃旧布局快照；再次打开没有重复标题或孤立节点。 | `aria-expanded`、`aria-pressed`、右栏可见性与消息滚动锚点。 |
| NAV-08 | `[data-action="overview-agent-settings"]` | 顶栏“当前 Agent”打开 `.environment-popover[aria-label="切换 Agent"]`，不改变当前右工具页。下拉含当前会话列表、协作多选对象和“配置模型参数…”入口。 | `aria-expanded=true`，右宿主节点仍是原工具；关闭下拉后迁移控件归位。 |
| NAV-09 | `[data-role="overview-workspace-name"]` | 打开工程下拉，含当前路径/最近路径和手工路径输入。路径历史最多八项。选择或回车执行真正 `/api/workspace` 切换。 | 标题、工程数据与模型设置同步；失败显示错误；不要只验证下拉打开。 |
| NAV-10 | `[data-action="top-chat-room"]` | 打开聊天室下拉，复用聊天列表/创建等现有动作，点击 `[data-room-id]` 调用真实切换。不可打开右工具栏代替选择。 | 当前聊天室标题、正文、索引与统计属于同一房间；原房间输入草稿保留。 |
| NAV-11 | `.environment-popover` | 外部点击、关闭按钮、Escape、窗口 resize 会关闭；Escape/关闭按钮恢复触发按钮焦点；上下箭头移动可用控件焦点。 | 焦点与 `aria-expanded`；反复打开关闭后节点不丢失、不复制。 |
| NAV-12 | `[data-role="chat-environment-settings"]`、`[data-role="session-extra-controls"]` | 旧高级权限/环境配置迁入设置；头像和视觉 Agent 仍可达。头像有独立保存当前 Agent 动作，不调用旧参数保存。 | 高级配置、头像选择、视觉选择均有入口；头像请求应只 PATCH `avatar`。 |

实现参考：`app.js:3794` 强制左栏关闭；`app.js:3878` 三个顶栏 handler；`app.js:10490` 打开工具及 DOM 原位标记；`app.js:10584` 关闭/恢复；`chat_experience.js:18` 环境切换锁；`chat_experience.js:50` 下拉；`chat_experience.js:273` 模型编辑器同步；`chat_experience.js:292` 外观控件。

### 环境切换竞态与草稿专项

以下变更型测试使用隔离工程、测试会话及模拟模型，避免把真实会话当测试夹具：

- 快速连续选择两次 Agent/聊天室/工程：第一个操作期间整体锁住环境入口、发送框、参数与头像容器，不允许交错激活；结束后恢复原 disabled/inert 状态。
- 模型正在运行时尝试切换：下拉显示等待结束/中止提示，不发新的切换请求。
- 在右侧编辑另一个会话的模型参数草稿，后台普通 `loadSessions` 刷新不能把编辑器强制切回当前 Agent，也不能清掉 dirty 草稿；工程变化应销毁旧工程编辑器，避免同名会话 ID 跨工程保存。
- 历史页打开时切聊天室：旧临时思考、工具状态、耗时、搜索结果、定位目标和 Token 显示立刻清空；自动请求新房间搜索/统计，旧慢响应不能覆盖新房间。
- 头像保存目标是当前顶栏 Agent，独立于参数页“编辑会话”；网络请求仅 `{avatar: ...}`，不能顺带覆盖协议/模型参数。

## 三、P5 布局、配色与尺寸边界

用户约束：保留玉石 SVG/PNG、人物头像和武侠竹林；增强文字对比；顶栏 Agent/工程/聊天室横向排列。不是替换成通用扁平主题。

| 范围 | 计算样式 / 可观察预期 | 重点边界 |
| --- | --- | --- |
| 主壳 `.shell.ui-redesign` | `display:grid`，高度 `100dvh`；常规行高 `52px minmax(0,1fr) 26px`；背景月夜竹林+深绿遮罩，不漏出旧蓝色舞台。 | 1280×720、1024×600；额外 1440×900、760×600。 |
| 顶栏 `.layout-top-region` | 环境标签和值在一条横线；当前值允许省略但触发按钮可读、可聚焦。 | 1180px 及以下隐藏品牌/头像/顶栏状态；760px 及以下隐藏环境标签；测试 1181/1180、761/760。 |
| 右栏覆盖模式 | `CHAT_LAYOUT_NARROW_MEDIA=(max-width:980px)`，小于等于该值打开工具使用右覆盖栏。 | 981/980px；覆盖时可关闭/返回，中央输入不被隐藏层拦截。 |
| 低高度 | 小于等于680px，主壳46px顶部+24px底部，图标按钮34px，输入区62px。 | 681/680px、1024×600；不能靠缩成10px字来塞页面。 |
| 消息正文 `.message-content` | 15px/1.75，主字 `#edf5ee`；消息背板近不透明深绿 `#102b21f7`；辅助字约12px；代码13px/1.6。 | 中文长句、长路径、链接、代码、用户消息；不要让 `.message > div::before` 旧模糊光点重新出现。 |
| 图标/装饰 | 左轨保留玉绿金线；消息头像32px；水印/灯笼减弱且不压文字。 | 不对所有 img 统一滤镜，否则头像/截图与玉石PNG会失真。 |
| 参数页 `.model-settings` | 输入14px、辅助字12px；`model-parameters` 容器>=520px时双列；<=340px时顶部/保存行换行。 | 扩展栏约608px与320px；<=680px高保存区改静态流，表单末项与保存按钮可达。 |
| 工程 `.project-layout` | 控件≥32px/12px；目录树获取剩余空间；预览和大纲可滚动。 | `extension` 容器<=600px时上下分区；测试599/600/601px及长文件名。 |
| 终端 `.terminal-window-command` | 命令独占主行，超时/运行/清空横排；输出13px/1.65。 | 1024×600，输入、超时与按钮均不横向裁切；本报告不要求运行命令。 |
| 任务 `.task-window-panel` | 所有配置、计划任务、模块诊断均可沿页面滚动到。 | 低高度底部不能永久遮挡；展开运行配置/监督后重复检查。 |
| 记忆 `.memory-window-panel` | 自然纵向布局；统计之后先显示记忆，再到维护作业/历史；`.memory-window-layout` 高度必须>0。 | `.memory-bead-list`300px；卡片与grid-auto-rows都至少76px、gap6px；文本列与删除列分离；星图260px；详情/来源可达。 |
| 视觉 `.vision-window-layout` | 截图证据前置、实际预览220px；操作双列；定位/闭环跨两列；卡片按内容自然增高。 | 五张操作卡不重叠；定位文本框、闭环按钮、底部摘要均可滚动到；不触发真实输入。 |
| 浏览器 `.browser-window-panel` | toolbar、代理details、桥接details、预览按自然流排列；预览min-height280px。 | 同时展开两个details，桥接输入/三个按钮/结果说明不能被iframe覆盖。 |

### 样式优先级易回归点

- `wuxia_layout.css` 最后加载只解决同等优先级冲突，旧主题存在大量更具体选择器和 `!important`。验证 computed style，而非只搜索新 CSS 是否存在。
- 顶栏列定义在 `chat_experience.css` 与 `wuxia_layout.css` 都有 `!important`，不同阈值1050/650与1180/760可能同时匹配；本轮后加载规则应生效。
- 协作页 header/tab/footer 旧规则带 `.workbench-window[data-window-theme="communication-bay"]`，简短覆盖容易失效；工具态应匹配 `.has-tool-view`。
- `.workbench-window.is-active > .window-panel` 的旧定高及 grid 行分配曾将记忆核心区压成0、将视觉卡压扁、将浏览器诊断当成预览行。三页新修复采用同等宿主/活动态限定、flex自然文档流。
- 记忆 `.memory-bead-item::before` 原本参与grid抢占主文本列；不能只调卡片最小高度。必须同步伪元素、grid-auto-rows、正文列与删除列。
- 模型输入控件有专用高优先级覆盖；不要用通用 button/input 背景覆盖其玉石保存按钮与焦点颜色。

## 四、打包链路与当前发布风险

### 已读实际链路

1. `scripts/build-msi.ps1` 默认配置是 **debug**。未指定 `-SkipPackageBuild` 时设置发布版本/日期/Git SHA/target环境后调用 `package-all.ps1`；**RD4-06 起**还会捕获该次 package report 指针（`report_id` + 内容哈希 + 载荷清单），把 `package_report_ref` / `payload_inventory_ref` / `build_identity`（三身份）/ `vcs` 写进 installer report，并自动 `Protect -Archive` 归档发布证据；**拿不到报告引用时 fail-closed**（`[REPORT-REF-MISSING]`，不能用 `-SkipPackageBuild` 绕过）。
2. `scripts/package-all.ps1` 读取 `config/package-manifest.json`，预检路径后清空专用 staging，按artifact顺序构建、复制并记录SHA256，再复制resources；最后执行package-safety。`-SkipBuild` 会直接使用既有二进制。**RD4-06 起**还会：构建前采集两遍静默枚举的源码快照（`source_snapshot_digest`，冻结记录写 `tmp/source-snapshots/`）、构建后逐文件复核（变化即 `[SOURCE-SNAPSHOT-CHANGED]` fail-closed）、计算 `build_input_digest`、写包内 `payload-inventory.json`（含 `payload_digest` 与报告引用）、并给报告加 `report_identity`（唯一 ID + 可重算内容哈希）。详见 `docs/analysis/2026-09-21-integration-review/build-identity-and-report-governance.md`。
3. web-console manifest构建使用 `--target-dir modules/gui-web/target`，来源为 `modules/gui-web/target/{profile}/coolzhu-web-console.exe`。此前工作区验证的 `target/debug/coolzhu-web-console.exe` 不是这个manifest输入路径；只构建根target后跳过打包构建，可能复制旧EXE。
4. manifest审查时只含 `gui-web.assets` → `modules/gui-web/packages/web-console/assets`，并标记 `optional:true`；没有独立的web `index.html`/`src`资源条目。完整交付因此依赖EXE内嵌启动文件与外置assets。
5. `installer/Product.wxs` 将staging递归收集到 `Program Files\CoolzhuAgent`；不会自动从源码追加遗漏资源。MSI图标、桌面/开始菜单快捷方式来自此文件。运行期会话/附件/密钥类文件被打包扫描及WiX排除。
6. build-msi校验staged CLI `--version`与请求版本相同，构建临时MSI后以SHA校验发布，产出installer-report。若同版本MSI已存在，文件名带时间戳；报告中 `msi` 才是本次文件，不能假定固定名就是最新。
7. 启动器从安装路径读取 `config/package-launcher.json`，web-console位于`bin`，运行目录为`%USERPROFILE%\coolzhuagent`，显式设置运行态数据路径。Tauri主窗加载外部本地HTTP `WebviewUrl::External`，不是用`tauri-shell/ui`替代web控制台。

### 需要安装验收明确排除的风险

| 风险 | 依据与后果 | 必测条件 |
| --- | --- | --- |
| 旧磁盘UI覆盖新内嵌UI | 静态优先读盘；旧版安装或人工热修保留的index/src优先于新EXE。 | 列出安装根中的index/src；HTTP实际字节与本次构建输入比SHA，不只看新文件修改日期。 |
| 源码目录掩盖包遗漏 | 未设置static-root时，安装EXE仍可能命中编译机源码目录。 | 在不依赖源码的静态根/干净机器执行资源校验；仅在仓库机器打开成功不算自包含通过。 |
| 只有新CSS或只有新JS | 逐文件磁盘/内嵌混用会形成跨版本组合，影响DOM迁移、参数挂载和布局。 | index、全部关联JS/CSS的hash作为同一组核验；重新打开窗口，确认模块存在与真实交互。 |
| assets可被静默跳过 | manifest把assets列为optional，且图标/背景/竹叶脚本无通用内嵌。 | 发布前强制检查玉石SVG/PNG、头像、月夜竹林和竹叶脚本文件及HTTP200，检查图片naturalWidth>0。 |
| 编译目录/配置选错 | 根target与modules/gui-web/target不同；脚本默认debug；skip flags会复用旧产物。 | 在发布报告记录configuration、实际EXE路径、SHA与包内/安装后EXE SHA，确保来源一致。 |
| 旧进程仍展示旧资源 | 覆盖磁盘不替换已运行EXE内嵌内容；端口健康不能单独证明版本。 | 记录8765监听PID和完整ExecutablePath，确认是本次安装路径的新进程；读取其HTTP资源。启动器有活进程占用拦截，不应据健康接口推断安装已生效。 |
| 现有静态测试覆盖不全 | embedded核心测试目前列表含wuxia但未逐项列chat_experience/model_settings；manifest测试不要求UI assets/index/src。 | 补充发布检查覆盖这些真实URL，不将PASS package-manifest等同于UI完整性通过。 |

## 五、资源发布检查步骤（供实际发布模型执行）

本次基准已保存 `tmp/analysis-release-ui-source-hashes.json`，覆盖全部十二项内嵌静态条目和三个代表性外置资产。如审查后仍改UI，应先重新生成基准，再构建；不要使用过时hash。

1. 从installer-report读取本次MSI、configuration、SHA；与实际安装用文件比对。读取对应package-report的web-console artifact来源/目标/SHA，确认没有误用根target旧文件。
2. 在staging、MSI解包或安装目录分别检查 `bin/coolzhu-web-console.exe`、`modules/gui-web/packages/web-console/assets`。根据当前设计，index/src磁盘缺失本身不必判失败，因为有内嵌兜底；关键是HTTP实际文件正确且无旧磁盘副本覆盖。
3. 确认安装进程PID、监听端口、EXE完整路径；检查其继承的 `COOLZHU_WEB_STATIC_ROOT` 是否指向旧验收目录。不得只看页面标题或“后端已连接”。
4. 对 `/`、`/src/app.js`、`/src/styles.css`、`/src/chat_experience.js`、`/src/chat_experience.css`、`/src/model_settings.js`、`/src/model_settings.css`、`/src/wuxia_layout.css`、三项语音脚本逐项GET：HTTP200、正确UTF-8类型、非空，二进制SHA与构建输入相同；同时核对no-store响应头。
5. 核对至少 `assets/icons-wuxia/chat.svg`、`assets/icons-wuxia/settings.svg`、`assets/icons-wuxia/search.svg`、`assets/icons-wuxia/model-scroll.svg`、`assets/avatars/wuxia-v3/bamboo-swordsman.png`、`assets/ui-redesign/three-column/control-icons/focus-layout-v1.png`、`assets/ui-redesign/chat-backgrounds/wuxia-bamboo-moonlit-chat-stage-v2.png`、`assets/bamboo-banner-wind.js`。再从Network筛查全部404/失败图片；代表性抽查不能替代全页面检查。
6. **内嵌自包含专项**：由主发布任务启动隔离进程，显式static-root指向仅有已打包assets的隔离目录；此模式不读取源码，index/src只能走内嵌。重复第4/5步；干净浏览器打开设置/顶栏下拉/记忆/视觉/浏览器，验证无ReferenceError和无缺样式。此专项不得修改真实运行配置或移动/删除源码。
7. **旧文件覆盖专项**：先只读记录安装根是否已有index/src，并对已存在文件做SHA比对；出现旧hash时视为发布未通过，由发布任务处理残留归属。不要把清浏览器缓存当作修复磁盘优先级问题。
8. 用新安装版在1280×720、1024×600重复NAV-01至NAV-12及四个子窗口回归；760×600检查覆盖栏；参数320/608px检查容器响应式。保存最终安装路径/PID、URL、窗口尺寸、截图和断言结果。

建议报告输出字段：测试ID、构建版本、EXE SHA、HTTP资源SHA、窗口/栏宽、动作、预期、实际、截图、通过/失败、失败选择器。遇到开发机与安装机不同结果时优先核查实际资源来源。

## 六、已具备的验证证据与尚未验证部分

### 本子代理亲自验证

- `scripts/test-package-manifest.ps1` 本次只读执行输出 `PASS package-manifest`。该测试侧重native-host、sidecar、WebView2Loader、浏览器扩展、STT模型与文档条目，不覆盖上述全部UI发布契约。
- 独立Edge会话`jade-review`对18765隔离开发服务完成1280×720、1024×600的任务/记忆/视觉/浏览器布局复测，已关闭会话；没有发送聊天、运行工具或保存配置。
- 记忆最终数据：七行均76px、gap6px、`hasOverlap=false`；正文可用362px。截图 `tmp/2026-09-19-e2e/review-memory-final-1024-list.png`。
- 记忆详情/来源可达：`review-memory-fixed-1280-detail.png`、`review-memory-fixed-1280-source.png`；其中detail截图早于16px标题微调，不作为最终字号证明。
- 视觉：`review-vision-fixed-1280.png`、`review-vision-fixed-1024-actions.png`、`review-vision-fixed-1024-summary.png`；浏览器：`review-browser-fixed-1280-diagnostics.png`、`review-browser-fixed-1024-diagnostics.png`；任务：`review-tasks-1280.png`、`review-tasks-1024-bottom.png`。均位于上述e2e目录。
- 浏览器0个JavaScript错误；原有iframe sandbox提示仍存在。没有`fixed`/`final`的同名系列截图多为修复前状态，勿混用。
- 前一轮临时VM行为脚本 `tmp/analysis-chat-experience-scope-test.cjs` 已通过作用域/草稿相关断言；它是模块级测试，不代表真实网络竞态与安装包通过。

### 主代理已记录的验证（此审查未重复运行）

`docs/work-logs/2026-09-19-jade-bamboo-layout-delivery.md`记录：web-console 8项库测试、1项helper、944项主程序测试通过，共953项；离线build成功。1440×900、1280×720、1024×600、760×600验证；参数栏约608px双列、320px单列；真实点击过顶栏下拉、设置/工程/终端/搜索/用量；隔离`READ_FILE_OK_20260919`搜索定位得到2条匹配。主要文字配色对比已计算，不能扩展声称所有像素组合均合规。

### 发布任务待补

- 本次实际MSI文件名、版本、哈希、安装成功结果与安装后进程来源。
- 安装后HTTP资源哈希、外置assets完整性、无源码fallback的自包含结果。
- 旧安装升级残留index/src的检查结果。
- 安装版完整导航、跨工程/会话竞态与参数草稿专项结果。此前开发服务截图不能替代安装版验证。

补充当前已知：安装前目录无旧index/src已确认；安装后仍应以实际HTTP资源hash与安装EXE hash完成闭环。953项既有测试通过能支撑源码行为，但不能代替未读取源码目录时的纯内嵌HTTP专项。
