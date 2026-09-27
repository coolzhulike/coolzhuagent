# 0.2.17 功能核查与针对性测试设计

日期：2026-09-27。范围：`04-coolzhuagent-整合改进执行计划.md` 的 S0–S6，以及后续追加的模型发现、开机动画、五入口与右侧内容工作区。本文把**已经观察到的结果**与**仍需真实环境验证的项目**分开；任何“待测”均不能计为通过。目标读者是接手设计功能测试用例的模型或测试人员。

后续状态更新：PR #67 已于 2026-09-27 00:06:20 UTC 合并（`9eae3aec3058f77e962702f7fc99aecb66653a76`），送审 HEAD `1ead69e` 的两条 CI 均成功。下文 Draft/待评审描述是打包交付时状态；PR 合并不改变本文所有“待测”判定，也不等于候选包正式发布。新增“更多”和“升级更新”要求与四项现状见 [2026-09-27 增补](../../analysis/2026-09-27-four-task-status-and-navigation-addendum.md)。

原生启动增量验收（2026-09-27）：正常安装入口 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe` 启动成功。GPT-6 Sol Max 通过 Computer Use 激活唯一安装版窗口并点击统计入口，右栏显示“统计信息”；主会话已查看截图。启动画面（私有现场原图 01-native-console-before.jpg 留在本机，未纳入 PR）、统计右栏（私有现场原图 02-native-usage-open.jpg 留在本机，未纳入 PR）。运行实例为 8765，实际工作区 `C:\Users\zhupu\coolzhuagent`，schema/支持版本均为 27。原有 `blocked by policy` 发生在外部工具审批创建进程前；本次没有改安全策略，恢复的是标准启动与观察点击流程。下文“没有安装版原生窗口证据”的历史表述已由这两张图补充，但 Paint、低高度/DPI、模型与其余完整验收仍待执行。

安装版模型与预览增量验收：Qwen 原图识别真实回合正确回答颜色、形状及数字，附件字节与用量有对应记录；右栏 UTF-8 文本、图片缩放、视频播放至 0:05、localhost 网址及外站 Tauri 原生子 WebView 均有实操图。详细证据与限制见 [安装版 Qwen 图片和右栏报告](installed-qwen-image-and-right-rail.md)。这更新了下文 MOD-02/UI-03 及相关 WBS 的安装版待测项；并未取得实际 wire 报文、浏览器扩展自动化、完整长任务或新版新增入口的验收结果。

后续分支源码增量：Rust 语言服务已用真实 rust-analyzer 在隔离浏览器逐项点击诊断、定义、引用与关闭，见 [LSP E2 报告](evidence/lsp-source-e2/report.md)；DATA/JOB 在独立目录、端口与合成数据上完成迁移恢复和中断重启现场，见 [生命周期 E2 报告](evidence/lifecycle-source-e2/report.md)。两者均绑定各自源码构建 SHA256，**不继承 0.2.17 已安装 MSI 的 E3 身份**。

## 1. 环境、版本和证据等级

- 源码基线：实施前 HEAD `2c15397688eb`，0.2.17 代码与证据已提交至 `codex/integration-0.2.17` 并推送 Draft PR #67。MSI 于提交前从确定的源码快照构建，包根收据和源码快照 digest 是包与源码的关联依据；后续文档提交不改变该安装包。`coolzhu-web-console` 编译期内联前端资源，因此页面验收使用的是重新构建后的 `target/debug/coolzhu-web-console.exe`，不是直接打开源码文件。
- 隔离运行实例：`http://127.0.0.1:8766/`，运行目录 `tmp/2026-09-26-completion/ui-retest/runtime`，会话 schema 27；不使用用户原有 8765 服务和真实会话数据库。隔离实例没有云模型 API Key。
- E1 源码/自动化：`cargo test --workspace --offline`：**2367 passed / 0 failed / 4 ignored**，69 个套件；`cargo build -p coolzhu-web-console --offline` 通过。`computer-use-core` 并发套件 **139/0**，Web 套件 **1242/0/1 ignored**。启动动画 Node 用例、微信投递/媒体 Node 用例均通过。原始运行日志保存在忽略目录 `tmp/2026-09-26-completion/`，不是发布包组成部分。
- E2 源码构建的软件实操：此前隔离 `8765` 的原生桌面窗口截图及浏览器实操图保存在 `evidence-pre-release/`。本轮重新构建的 `8766` 浏览器实例也逐项打开五入口、顶部三下拉、文字/图片/视频/网址右栏、消息搜索与轨迹。截图只绑定其拍摄时的源码构建，不能移作最终 MSI 证据。
- E3 **0.2.17 已安装，原生导航已部分验收，Paint 未通过**。MSI 升级退出 0，注册表和 CLI 都显示 0.2.17，安装目录 10/10 个声明二进制与包根 SHA256 一致。早期 `8767` 隔离启动曾遭外部审批拒绝；后经标准注册入口启动成功，在真实工作区的 `8765` 完成五入口、三个下拉和 Qwen→Paint 请求实操。Paint 被产品输入安全门拒绝，零输入；详见 [导航与 Paint 记录](installed-navigation-and-paint.md)。不能把历史外部审批阻断继续当成当前原因，也不能把拒绝截图当绘制成功。

证据判定：E1 证明被覆盖的代码行为；E2 证明拍摄时源码构建的原生/浏览器软件可见行为；最终安装版原生窗口、Paint 等必须另有 E3 截图与实际回执，才满足用户指定的最终验收标准。

安装包身份：`dist/CoolzhuAgent-0.2.17.msi`，SHA256 `35584E5291D9C86CE244F9B6F63C632DA29E8519306E9A577B145D2ADB87B0EB`；包根 digest `ee6cf8adf268908df97798cc17b3b1c2a3cec52941e9ab012cded61e9cb13ef6`；源码快照 digest `fc2da23ade641f33c8d6172b13fce37b1e2579437985f6e64102eeab689affbe`。打包脚本已将包报告归档到 `evidence/build-identity/`。安装日志 `tmp/2026-09-26-completion/msiexec-install-0.2.17.log` 属本机忽略区，不随 PR 分发。代码与本报告已提交 [Draft PR #67](https://github.com/coolzhulike/coolzhuagent/pull/67)，待完成原生现场验收后才能转为正式评审。

## 2. 关键改动及可设计用例的契约

| 功能 ID | 入口与涉及模块 | 应有行为 / 反例 | 本轮证据与结论 |
| --- | --- | --- | --- |
| MOD-01 | 设置→统一模型参数页；`model_settings.js`、`model_discovery.rs` | 保存协议、Base URL、模型 ID、图片能力、思考层级、工具能力。模型发现只读取远端列表，保留用户输入的 Base URL；能力未知时保持未知，不按模型名臆断；草稿发现不得自动覆盖已保存配置。拒绝本机/私网重定向、URL 内凭据与非法 endpoint。 | E1 发现地址、能力来源、未知及本地拒绝用例通过；E2 表单和获取模型入口可见。真实百炼端点发现待测。 |
| MOD-02 | Qwen 会话与附件；`multimodal_input.rs`、`llm-adapter` | `qwen3.8-flash` 保留百炼 Base URL 与现有 Key 来源。明确支持图片时按协议原图发送；明确纯文本时先由默认视觉 Agent 转述；未知能力不得默默改写为支持。需核对实际 wire 请求、usage 和错误信息。 | E1 路由测试通过；`8765` 隔离构建中 Qwen 以已配置 Key 回答 17×23=391，直接识别测试图并生成 10,908 字节 `pelican-bike.html`，在右栏预览可见 SVG 且暂停按钮变“继续”。Agnes 转述后回 Qwen 成功；之前一次 `max_tokens exceeds 65536` 错误已在配置中改为 32768，仍需在 0.2.17 安装版复验。见图 06/07/10/14/16/17。 |
| MOD-03 | 聊天发送/流式工具；`main.rs`、`tool_loop_coordinator.rs` | 只有模型正式 `tool_requests` 经登记和授权后才派发。用户文本写“调用 computer_use.perform”或模型普通文本假称调用，不能触发工具；流式/非流式一致，错误/重试不重放副作用。 | E1 守门和全工作区通过；E2 无 Key 请求显示模型失败，`/api/computer-use/runs` 发送前后均空。仍需有 Key 假模型协议实测。 |
| CU-01 | Computer Use 两相 helper；`computer-use-core/input.rs`、Web permit/store | READY 前零输入；无 permit、错 nonce、超时零输入；有效 permit 才由本次 helper 执行；按事实记录 `not_sent/may_have_been_sent/sent`、partial、释放和后图。并发房间/实例不可争同一输入所有权。 | E1 真实 helper 的 B128-T1、T11、T6-A/B/C 和并发 139/0 通过。E3 真实 Paint 短线、取消与截图仍待测。 |
| CU-02 | Paint 诊断；`computer_use_planner.rs`、`desktop_bridge`、UIA | 空白画布短线与矩形分别验收；选工具、移光标、旧画布都不能冒充新绘制。失败需归类为工具接口、窗口身份、坐标、视觉观察、模型选点或超时；不能简单判为模型能力。 | E3 已实测：Qwen 正式调用 `computer_use_perform`，在 intent_guard 被 `input_safety_resource_not_accepting_new_input` 拒绝，零输入、无新 CU run；前后空白图 B-02/B-05。功能未通过；原因是历史恢复门禁，不能归为模型能力。见 [恢复诊断](installed-input-safety-recovery-diagnosis.md)。 |
| UI-01 | 左快捷轨；`workspace_panels.js/css` | 0.2.17 的“首页、定时任务、设置、统计信息、微信连接”五入口。后四项打开对应右栏；首页回到当前聊天室。不复活旧左侧展开聊天导航。更多/更新属后续代码，不在此包。 | E2 与 E3 五入口已逐项点击，安装版截图 A-01 至 A-05。微信不可用状态可见，真实连接未通过。 |
| UI-02 | 顶栏当前 Agent / 工程目录 / 聊天室 | 点击在原位下拉切换环境，不把三项误送到右栏；在低窗口高度保持左右可用。 | E3 三个下拉已打开（A-06 至 A-08）。发现发送目标与设置编辑对象展示不一致，后续需明确同步语义；低高度缩放尝试未成功，不能声称已测，也不能据此认定窗口不支持调整。 |
| UI-03 | 聊天中的文字、图片、视频、网址；`content_preview.js`、`content_delivery.rs` | 点击在右栏标签显示，文字 UTF-8 不截断中文/扩展字符，图片可看/缩放，视频能按 Range 播放，URL 用受限浏览器面板。目录越界、无效媒体和 Range 异常必须拒绝。 | E2 `预览验收.txt/png/mp4` 和 `https://example.com` 分别打开；文字 `𠮷😀` 正常；视频片段 `206 video/mp4 bytes 0-1023/797886`；原生 Tauri 子 WebView 待测。 |
| UI-04 | 临时思考/工具动态、运行轨迹；`run_activity.js`、`chat_experience.js` | 进行中最多 1–5 行浅色滚动过程；完成后正文只留回复，思考和工具详情在轨迹；真实 tool_call_id 去重，轮次耗时可见。 | E1 显示与去重用例通过；E2 Qwen 长任务图 09 记录进行中、图 10 完成后过程区收起且耗时 1 分 23 秒；图 12 是旧原始轨迹档。新版结构化轨迹与取消迟到仍需安装版截图。 |
| UI-05 | 消息搜索/定位与统计；`chat_search_index.rs`、`chat_insights.rs` | 关键词定位对应消息；跨聊天室隔离；用量未知时不得显示假零或编造 token。 | E2 搜索“预览验收”匹配 1 条，索引总数 4；历史 10k/100k 的真实 API 性能、并发发送与重建已通过并归档（§5 的 4.2）。E3 统计右栏已打开；安装版历史定位仍待测。 |
| UI-06 | 玉轴竹林开机动画；`scroll-startup-player.js`、Tauri shell | 初次播放、日常快速、Esc 跳过、减少动态、资源超时、恢复窗口、窗口创建失败分别不阻塞服务；“酷朱”印章、Q版人物与字标需真实画面/节奏确认。 | E1 Node 生命周期用例通过；E3 原生不同 DPI/低高度视觉截图待测。 |
| DATA-01 | SQLite schema 27；`schema_upgrade.rs`、`credential_store.rs` | WAL 一致性备份、事务迁移、前向版本拒绝；密钥不回显且旧配置迁移。模拟中断不写半迁移状态，旧 exe 不碰高版本库。 | 新源码 E2 合成 v26→v27 实际备份、恢复语义一致，3 会话/中文消息/附件/完成运行保留；干净 v99 库拒绝且 DB/WAL/SHM 状态未变。旧假明文留于忠实迁移前备份，活动库与兼容 JSON 未检出。异常 hot journal、真实旧 MSI 与跨设备秘密仍未验。见[生命周期现场](evidence/lifecycle-source-e2/report.md)。 |
| JOB-01 | 定时任务/Goal；`scheduled_jobs.rs`、`goal_execution_parent.rs` | 单次授权、重启去重、角色离线/暂停恢复、预算连续，不能重复派发或临时全局提权泄漏。 | 新源码 E2 两次真实假模型收件：一次 settled，一次在 running 时终止隔离服务；重启同 occurrence 返回 unknown、无第三次调用。右栏原先误显“1 项待执行”，修复后独立浏览器实见“0 项可执行 · 1 项待核对”和领取结果未知。证明防重派与提示可见，不证明自动恢复或恰好一次副作用。见[生命周期现场](evidence/lifecycle-source-e2/report.md)。 |
| WX-01 | 微信连接；`clawbot_*`、`scripts/lib/clawbot-ilink-*.mjs` | 连接状态真实反映 sidecar/provider；媒体、重复投递与群权限按消息身份处理。不可用时不显示“已连接”。 | E1 投递和媒体脚本通过；E2 sidecar 不可用提示诚实。真实微信 provider 未配置，端到端待测。 |
| PKG-01 | `scripts/package-all.ps1`、`scripts/build-msi.ps1` | 同一源码快照构建目标；CLI/MSI 版本 0.2.17 一致；包根/收据/哈希绑定。升级后原用户数据保留；安装和卸载不误删工作区。 | 首次打包因 Tauri 独立 Cargo.lock 不自洽而拒绝，离线解析并严格重建后正式包通过：`release_eligible=true`，MSI SHA256 `35584E5291D9C86CE244F9B6F63C632DA29E8519306E9A577B145D2ADB87B0EB`，240,077,406 字节，未签名。`msiexec` 升级 0.2.16→0.2.17 exit 0；注册表/CLI 0.2.17，10/10 已安装二进制哈希一致；未做用户数据前后逐文件比对。后续分支 DATA 源码 E2 不能替代新 MSI 安装前后对照。 |

## 3. 建议的实操测试顺序与记录格式

每条用例记录：`feature_id / 构建哈希或 MSI SHA256 / Windows 版本与 DPI / workspace、room 和模型 ID（密钥脱敏）/ 初始状态截图 / 操作与参数 / 结果截图 / API 回执和 run_id / 实际 usage / 通过、失败或未测 / 失败归因与复现条件`。工具与画图用例必须附**动作前后**窗口截图，标注画布区域、鼠标/工具前置状态、输入发送及释放；只有“工具调用成功”文本不算画成。

1. 在隔离工作区安装并打开 0.2.17 原生窗口，截取首屏、五入口、三下拉和低高度布局；分别测试开机动画正常、Esc、减少动态和资源失败。
2. 用已有百炼 Key 测 `qwen3.8-flash` 文本与图片：核对实际请求的 endpoint、模型 ID、模态结构和 usage；纯文本模型再测默认 Agnes 转述。Key 不可用时记录“未测”，不得把密钥错误算模型缺陷。
3. 用假模型服务同时覆盖流式与非流式：普通文本伪工具、正式 tool_calls、缺 id、重试和中断；每轮比较工具登记、轨迹与实际副作用次数。
4. 在干净 Paint 画布先做短线，再做矩形，分别验证 READY/permit、所有权、坐标、画布变化、取消和恢复；随后才运行海绵宝宝长任务。至少截输入前、工具选中、输入后、失败/取消终态。
5. 聊天中打开安全工作区文字、图片、视频、网址；测试中文分页、无效 Range、越界路径与媒体缺失。搜索多房间历史并比较索引与消息数量。
6. 重启应用验证定时任务无双派发、聊天室/Goal 状态与 token 统计保留。升级前后成套备份 SQLite、配置和秘密引用；前向库由旧程序拒绝而非改写。

此前实操的代表性证据（全部是**0.2.17 MSI 之前**的源码构建）：[文字预览](evidence-pre-release/01-text-preview.png)、[图片预览](evidence-pre-release/02-image-preview.png)、[视频播放](evidence-pre-release/03-video-playback.png)、[原生右栏网页](evidence-pre-release/04-native-browser-example.png)、[Qwen 文字](evidence-pre-release/06-qwen-basic-chat.png)、[Qwen 图片](evidence-pre-release/07-qwen-image-direct.png)、[长任务进行中](evidence-pre-release/09-long-task-live-status.png)、[长任务完成与用量](evidence-pre-release/10-qwen-long-task-complete-and-usage.png)、[搜索定位](evidence-pre-release/11-search-message-location.png)、[轨迹存档](evidence-pre-release/12-trajectory-archive.png)、[Agnes 转述](evidence-pre-release/14-vision-fallback-route.png)、[鹈鹕动画暂停](evidence-pre-release/17-browser-pelican-paused.png)、[顶部聊天室下拉](evidence-pre-release/18-browser-room-dropdown-jade.png)、[模型发现能力来源](evidence-pre-release/19-browser-model-discovery-official-capability.png)。同目录还保留失败/过渡截图；不能把它们当作成功证据。

## 4. 发布判定

0.2.17 的 E1 基线全绿，E2 证明隔离源码构建中真实 Qwen 文本/图片/长任务及指定 UI 行为，包装与升级安装身份也已通过。E3 已有原生导航截图，**Paint 实际输入、真实微信连接、旧数据升级恢复仍未取得通过证据**。本包为部分现场验收的候选版本；不能声称 S0–S6 的 32 包全部完工。后续分支新增代码需独立编译、实操和打包，不能继承本包全部验证结论。

## 5. 04 计划的 32 包审查映射

下表“代码”表示本轮能定位到实现或测试入口，**不等于工作包完整验收**。计划中明确要求现场实验、跨进程/跨入口对照或发布证据的项目，必须逐项补足，不能被全工作区自动化的绿色结果替代。

| WBS | 代码核查结果 | 功能/退出门槛的剩余验证 |
| --- | --- | --- |
| 0.1 | 包根收据、构建身份和本报告 feature_id 可对应；MSI 哈希与 10 个安装二进制已核对；代码及证据已提交 Draft PR #67 | 打包时 HEAD 是 `2c15397` 种子引用，源码权威为 snapshot digest；新安装版原生截图仍需补 |
| 0.2 | Web/LLM/CU 有假模型、契约和输入故障测试 | 黄金 fixture 覆盖率及全量历史回放未逐条审计 |
| 0.3 | 离线工作区门禁通过；插件/诊断路径有状态测试 | CI 环境、真实插件不可用声明与诊断探测现场核对 |
| 1.1 | `run_contract.rs` 定义身份/预算/事实契约 | 多入口迟到事实与兼容旧字段的数据库回放 |
| 1.2 | CU executor/store/helper、请求尝试及输入事实测试通过 | 在本版 Paint 截图与 R6 等价样本五层核账 |
| 1.3 | permit、输入所有权及两相 helper 并发测试通过 | 双原生实例与旧 helper 残留现场故障注入 |
| 1.4 | `tool-registry`、Web 权限和未正式调用拒绝已接入测试 | 插件/MCP/CLI/子 Agent 的 debug/release 权限真值表实操 |
| 1.5 | 取消、释放及 deadline 相关自动化通过 | 真正进行中长笔画的取消、释放未知隔离截图 |
| 2.1 | 统一模型参数页、配置发布和远端发现模块可定位 | 保存→解析→实际 wire 的百炼 Key/图片/思考层级对照 |
| 2.2 | `request_usage.rs`、`chat_insights.rs` 与历史投影可定位 | 有真实 usage 的多轮、迟到和嵌套调用对账 |
| 2.3 | 工具派发收尾与调用身份模块可定位 | 并发取消、hook 重排及重名动态工具现场路径 |
| 2.4 | schema 27 事务迁移与备份测试通过 | 会话按 epoch 切权威、广播前后崩溃与旧写者隔离 |
| 2.5 | CLI `shared_chat`/`stream_facts` 与 Web 主路径均已变动 | 流式/非流式/CLI/子 Agent 同 fixture 的逐事件等价；不能仅凭文件存在宣布完成 |
| 3.1 | `computer_use_adapters`/desktop bridge 坐标测试可定位 | 100–200% DPI、负坐标、混合屏真实画布误差 |
| 3.2 | UIA 与物理后备的能力代码可定位 | Paint/记事本控件能力表及语义超时不双击 |
| 3.3 | planner、任务基线与结果消费模块可定位 | 空白/已有图/仅工具栏变化/一笔的现场反例 |
| 3.4 | 失配、settle、重复检测测试可定位 | 固定真失配集及非目标变化误拒样本量和区间 |
| 3.5 | 历史 R1–R6 留有失败分类 | E1–E5、L1/L2 本版对照实验未完成；绝不可填成功率 |
| 4.1 | 左轨/右栏分模块、竹林主题和启动资源测试通过；后续源码分支 LSP 文件预览窄栏布局与目录折叠已在 E2 点击验证，见 [LSP 现场](evidence/lsp-source-e2/report.md)。1000×560/430 浏览器视口下更多、更新和插件面板可达；启动卷轴跳过/减少动效事件与截图已归档，见 [低高度与启动现场](evidence/navigation-source-e2/report.md) | 新增入口、低高度、启动交接仍需新 MSI 原生窗口验证；浏览器视口不等于真实 Windows 缩放与混合屏 DPI |
| 4.2 | 轨迹、搜索索引、token 显示有测试与 E2 观察；隔离实际后端已完成 10k/100k 历史保存、搜索、并行发送、重启及删派生索引重建，见 [历史检索证据](evidence-history/history-retention-and-search-report.md) 和 [v5 原始数值](evidence-history/results-v5.json)；后续 DATA 样本只证明少量中文历史迁移后仍在 | 安装版 GUI 历史定位、断线迟到、真实流式思考收起；API 性能与迁移小样本不能替代 GUI 验收 |
| 4.3 | `conditional_file`、spill、blob/content delivery 可定位 | 文件冲突、GC 引用保全与大媒体异常流实操 |
| 4.4 | skills/记忆/beads 仍有入口与投影 | 目录指令、污染记忆与压缩图像证据的跨版本回放 |
| 4.5 | Goal、调度与根预算模块可定位 | 跨重启、角色离线、人工验收和无双派发 |
| 5.1 | Windows DPAPI/密钥引用和旧配置迁移测试通过；新源码 DATA E2 中活动库/兼容 JSON 未检出旧假明文，迁移前备份仍忠实保留，见 [生命周期现场](evidence/lifecycle-source-e2/report.md) | 换设备重新配置、真实旧库/WAL 与已删除页残留、实际连接不漂移 |
| 5.2 | 进程监督、沙箱及低权限门禁代码可定位 | 第三方插件/MCP 逃逸、句柄继承和隔离不可用现场试验 |
| 5.3 | OAuth store、MCP stdio 与别名代码可定位 | 四 transport、刷新竞争、断线重连的真实服务测试 |
| 5.4 | 后续源码分支 Rust LSP 宿主与右栏已用真实 rust-analyzer 完成诊断、定义、引用点击定位和停止 E2，见 [LSP 现场](evidence/lsp-source-e2/report.md) | 目前只支持可信用户级 Rust preset；PTY owner/输出/重连由另一验收负责，LSP/PTY 最终新 MSI E3 未验 |
| 5.5 | 浏览器/微信/语音/桌宠边界模块与 Node 回归可定位 | 真实微信 provider、语音取消、桌宠拖放及浏览器原生隔离 |
| 6.1 | 诊断与运行轨迹有代码和 E2 显示 | 健康故障注入、日志轮转/配额、脱敏导出 |
| 6.2 | schema27 WAL 备份、前向拒绝测试通过；后续源码 E2 合成 v26 库真实迁移备份、恢复及干净 v99 库字节不变有记录，见 [生命周期现场](evidence/lifecycle-source-e2/report.md) | 旧 MSI→新 MSI 安装前后逐项对照、异常 WAL/hot journal、配置与跨设备秘密成套恢复、用户真实数据差异 |
| 6.3 | 隔离源码构建中原生/浏览器 E2 截图及 Qwen 云模型试验已归档；0.2.17 安装和已安装后端启动通过 | 0.2.17 原生窗口、Paint、低高度及取消截图缺失 |
| 6.4 | 本报告、自动归档的包身份和发布脚本可追溯代码/测试；0.2.17 MSI 与安装日志已生成，Draft PR #67 已开 | 签名证书/签名和原生现场证据仍待补；PR 保持 Draft |
