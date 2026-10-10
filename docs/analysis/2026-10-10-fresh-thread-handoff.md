# 剩余任务交接与并行执行计划（2026-10-10）

用户要求停止在长历史会话中累积上下文，改由新的会话执行剩余任务。模块开发可并行，正式软件与真实模型测试统一安排；新增品牌Q版头像替换、子Agent与定时任务统一执行和右栏执行页。本文件是新会话的启动材料，不要求读取全部旧聊天历史。

## 权威基线

- 当前实现仓库：`C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent`。不要把项目默认目录 `C:/Users/zhupu/Desktop/coolzhuagent` 的旧checkout当成最新源码。
- 分支 `codex/browser-affine-longrun-20261007`；交接前HEAD `0bed77a4d20147e2fad3dd18d7327401127339fa`。本文提交后的SHA写入 `tmp/2026-10-10-fresh-thread-handoff/dispatch.json`，各开发会话从该SHA创建独立managed worktree，避免多会话编辑巨型文件和共享Cargo输出。读取AGENTS.md；大文件按范围读取。
- 正式0.2.127已安装并正常启动；Program Files1159文件逐SHA匹配，release构建实际0/301.68秒、安装0、六项发布检查通过。包源码是 `a3ec4620c67e0bf2ae8e05811d5b198675b719a3`，文档HEAD不是包源码。
- [PR88](https://github.com/coolzhulike/coolzhuagent/pull/88)保持open、未合并。[0.2.127预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.127)五资产服务器摘要与本地一致，tag对应冻结源码。未签名，不擅自修改自动更新索引。
- 正式后台8765；当前验收工作区 `tmp/2026-10-04-devin-models/workspace`。实际完整路径以正式进程收据为准：`tmp/2026-10-10-release-127/installed-127-standard-processes.json`，操作前重新核验，不能沿用过期PID。
- 验收Agent `session-1791131217833`、房间 `room-1791131523339`；SWE-2-medium、配置revision57，唯一Devin远端绑定 `island-kayak`。保持同一个远端会话，不新建云端测试会话、不换Qwen、不改用户凭据。Opus暂停，Paint用户免测，微信保留不动不测。
- 真实库 `.coolzhu/web-sessions.sqlite3` 和JSON在上述验收工作区，不是仓库根的样本会话。密钥不输出、不写交接文件。

## 已通过且不应重复验证的基本项

完整舞剑开机动画主体、跳过与重复启动；基础Compute Use输入/Paint旧证据；普通/SVG/Shadow/同进程和跨来源Browser点击/输入/Enter/滚动；复杂仿射九步任务和多轮长附件→跨来源Browser→DSH计算器；图片真实识别和跨重启；DSH远程市场及若干安装/取消/停用生命周期；基础配置/统计/搜索/索引/右栏/低高度布局；终端和LSP若干恢复/隔离；定时任务固定结果房间及重启并发扫描零重放，均已有对应正式证据。

127已正式纠正上传箭头偏左：隐藏字号0的span仍产生6px flex间隙，删除span并标记纯图标，原SVG与玉石皮肤保持。正式地址草稿16.451秒/33次URL更新保持、打开目标GET一次、后退同步通过。旧的“SVG对称所以按钮已居中”结论已纠正，不应再次裁剪图案来掩盖布局问题。

ChatGPT订阅登录原理和最简单真实连通已有10-08报告，连接器不等于完整订阅provider集成，勿重复简单smoke。OpenCode/HF已有模型目录与入口，真正生成/多模态/工具能力不可由目录推断。

## 剩余任务及先后关系

| 工作域 | 尚未闭环 | 交付标准 |
| --- | --- | --- |
| Browser严格时序（P0） | 新原生Target替换、跨来源commit在down/up窗口、观察在途正常UI撤销、HRESULT与资源变化竞争；不得把planning期间变化等同原生窄竞争。127文档负例有一次来源不明BODY keydown，严格零输入断言失败。 | 保留原失败；补键值/目标/时序取证，生产逻辑不人为增加延迟制造命中；明确送达/释放/效果/终态；正常软件截图与真实调用原始事件对应。输入释放后在途观察取消已125通过，不重新列为未实现。 |
| 长程工具续接（P0） | 结果分页有8192→69632等中段缺口；纠正反馈次数、取消迟到、子目标重规划/跨重启恢复未完整。 | 真正连续读取全部分段后再执行后续工具；文件附件、Browser业务、DSH工具组合验收，未知效果不冒成功，不重复简单问答。 |
| 统一执行架构（新增，P1） | 模型会话作为工具、子Agent和定时任务共享运行器；Devin目前在Goal/Relay/子Agent入口明确拒绝；Web普通/流式和CLI/Goal/调度仍未全面统一。 | 先设计共享TurnRunner/ExecutionService和真实provider能力契约，再接入会话工具与调度适配。持久执行/父子关系/取消/预算/统计/结果归属一致；不通过删除限制或手写成功回复假接通。 |
| 新右栏执行页（新增，P1） | 独立展示子Agent和定时任务执行，不仅合并两张旧列表。 | 共用任务执行模型，显示来源、模型/会话、父任务、状态、进展、结果、耗时、真实usage；详情跳轨迹，取消/重试等实际后端动作；定时配置仍可用，后台任务不切当前房间。技术决策由开发审查，视觉取舍给用户看。 |
| 品牌Q版头像（新增，P1） | 所有头像入口替换旧Q版侠客。DeepSeek鲸鱼娘、ChatGPT白色小龙娘、Qwen、Claude等形象。 | 必须使用image_gen逐资产生成，网上流行形象先核实参考；无参考可采用品牌辨识的原创拟人方向并明确说明，不冒官方形象。统一视角/主体尺度/透明背景/安全边距，32/48/64px可读；模型匹配、默认/未知provider回退和旧预设迁移全覆盖，不覆盖用户上传的自定义图；默认头像、列表、发送对象、消息、模型设置、执行页实际显示一致。玉石竹林整体UI不变。先出视觉方案供用户审阅，再替换集成。 |
| 会话/Provider（P1） | 完整配置Service、跨资源崩溃一致性；账号过期/rebind/换模；OpenCode/HF真正生成，多模态/工具；Devin非聊天入口能力。 | 复用受保护凭据和统一参数；不复用别平台Key。不支持能力明确报错；缺凭据则记录具体外部条件，不使用模型夹具冒真实通过。 |
| 文件/附件/记忆（P1/P2） | 超长单行GUI分页、模型编辑工具、spill/GC与有效引用保留；多模态/群发/Agnes回退、PDF/音视频、压缩图片/污染/召回矩阵。 | 已通过1.8MB完整八页/SHA及19775行定位不能代替超长单行。保持内容寻址字节与权限身份，测试长程而非重复小文件读取。 |
| 核心可靠性（P1/P2） | 跨进程单写者/outbox/会话权威epoch，统一ToolDispatch/hooks，broker两实例/崩溃后旧helper，全来源取消/预算/后代树，嵌套usage分账。 | 模块职责明确，小规模可review增量，不搭第二套session/凭据/记忆系统；先正常通路后必要故障测试，不扩大无依据的防护代码。 |
| 运维/交付（P2） | 断电/磁盘满/旧版迁移，完整脱敏/日志配额，混合DPI/负坐标/多屏，启动失败/减弱动作，完整依赖许可证/签名/feature映射。 | 环境未具备的项目明确列出，不能靠表中已有实现判通过；统一批次构建安装、逐文件摘要、正式截图、CI、改动与针对性测试报告，PR审查后由用户决定合并。 |

## 四个会话的职责与隔离

1. **总控与统一验收**：维护唯一Goal和新版剩余清单，收集各模块提交，审查并集成；独占正式测试窗口、真实库、模型绑定、8765服务、正常MSI安装/发布及PR更新。补齐其它未分给专门会话的架构/附件/Provider/运维工作。审查完整32WBS，不只盯Browser。主控制台入口 `main.rs/app.js/index.html/styles.css` 的交叉接线统一由总控集成。
2. **Browser Use与长程续接**：在独立checkout修复Browser/controller/ACP分段模块及可复现证据驱动，不操控正式窗口、不调用唯一云端、不写真实库。输出提交、设计、候选验证说明、联合实测步骤；严格窗口未命中保持开放，不重复短任务碰窗口。
3. **统一执行架构与任务中心**：在独立checkout设计并实现共享Runner、会话工具、子Agent/调度适配、右栏执行页独立模块，兼顾WBS2.3/2.5/4.5。入口接线给总控提供明确patch/说明，避免另一会话并行写巨型入口；不用现有明确拒绝的Devin路径冒支持。不访问原云端和桌面。
4. **品牌Q版头像**：独立checkout读imagegen及必要调研skill；生成可复用正式资产/manifest/匹配模块与视觉预览，全面列出旧头像引用和迁移策略。不改工具/运行器和共享入口；不操作正式窗口。视觉方案展示给用户，方案未审阅前不得将样图算为正式替换验收。

各会话执行自身任务，不再另起子代理。每个开发checkout使用独立Cargo target及tmp目录；不得并发写共享Cargo输出或对运行中的真实验收库执行迁移。只提交自己范围文件，不reset/checkout/stash其它人的修改。完成后在共享交接目录各自独立结果JSON记录worktree/branch/commit/文件/真实验证/遗留项；总控只读收集，按依赖串行集成。

## 联合测试与交付闸门

开发可并行；桌面操作/正式模型调用只能由总控顺序执行。测试时把新任务中心、模型工具调用、调度结果、头像、长附件和Browser/DSH续接组合进同一批综合验收，不是四会话同时抢鼠标或云端绑定。支持的入口复用同一真实SWE会话；新增能力缺契约先实现，不伪造客户端请求绕拒绝。

先offline实际cargo build和变更必需的验证，再统一候选→正常完整release包→正常安装→正式软件实拍→报告与CI。记录freeze源码与包摘要，不一处CSS一包，不混候选/旧包证据。不以HTTP或headless截图替代原生软件验收，也不堆镜像实现的单元测试。

使用Windows computer-use skill并遵守工具真实限制；普通可逆操作无需重复询问，原授权持续。不能绕过Windows安全/认证屏障。停止实例按PID+路径+启动时刻+SHA核验；不按端口杀其它项目。严禁回显API key、清空历史复核/记忆、自动扩张任务权限。

## 新会话必读材料（按需分段，不整段回放历史）

- 根AGENTS.md、Cargo workspace清单及实际涉及模块。
- `docs/analysis/2026-09-21-integration-review/wbs-implementation-audit-2026-10-07.md`：32工作包遗留矩阵。旧表的126基线以本文127事实覆盖。
- `docs/analysis/2026-09-21-integration-review/current-acceptance-queue.md` 与 `acceptance-summary-2026-10-08.md`：只读顶部最新更新和涉及工作域，不按日期倒着重复测试已关闭项。
- `docs/testing/release-0.2.127/change-report-and-test-handoff.md`：最新包、图标、地址及未通过文档负例的正式原图与原始证据。
- Browser：`browser-input-timing-observation-design-2026-10-08.md`、`acp-tool-result-pagination-design-2026-10-08.md`、`tool-result-readability-design-2026-10-08.md`及对应模块。
- 执行整合：`scheduler-room-binding-design-2026-10-08.md`、`agent_session_backend.rs`、`scheduled_jobs.rs/scheduled_execution.rs/scheduled_delivery.rs`及Goal/ToolDispatch实际模块。
- Provider：`chatgpt-subscription-connectivity-research-2026-10-08.md`、统一会话参数/配置服务设计；原Desktop04执行计划作为需求对照，不能替代最新代码事实。
- 头像：`src/app.js`顶部旧头像默认/别名映射和`assets/avatars/`；用户刚确认上传控件已修，不应改回按钮布局。

运行真实身份、服务PID与thread ID由总控重新读取；本文件不是允许使用过期身份或凭据的依据。后续工作在新会话推进，本旧会话只保留交接，不重复启动任务。

## 已创建的新执行会话

四个新会话已创建并核验正在执行，开发起点冻结为 `40d3598471b8ae1ed88ce9c018e5d06f6442fb31`。以下ID用于读取进度，不把并行开发等同已完成正式验收。

| 会话 | thread ID | 共享进度文件 |
| --- | --- | --- |
| 剩余任务总控与联合验收 | `01a12624-bf49-7861-91ae-0060da4507af` | `coordination-status.json` |
| Browser Use与长程工具续接 | `01a12623-6417-7d30-81c2-1616bce510d3` | `browser-result.json` |
| 统一子Agent与定时任务执行中心 | `01a12623-a67d-7c21-b4e1-d4c6383c46e7` | `execution-result.json` |
| 品牌Q版头像设计与替换 | `01a12623-f155-78e2-876d-7ca13dc299e8` | `avatar-result.json` |

共享进度文件位于权威实现仓库的 `tmp/2026-10-10-fresh-thread-handoff/`；`dispatch.json` 已写入开发基线、任务边界和测试所有者。各开发会话使用独立managed worktree，新总控是正式测试和集成的唯一执行者。本旧会话若收到继续或定时唤醒，只读取上述新会话状态，不重复发起旧任务；后续具体需求优先在新总控会话追加。
