# 整体验收状态快照（2026-10-08）

0.2.100当前正式增量：正常完整构建、六门pass、1159安装文件逐长度/SHA一致，已安装并公开[预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.100)。冻结源码2816e3e，两路远端CI均success。原生控制台无需刷新自动显示外部真实SWE新任务#679和完整回复#680，单次计算器completed/审计ok，唯一island-kayak正常收尾；见[100交接与实拍](../../testing/release-0.2.100/change-report-and-test-handoff.md)。正式本页流式/断线大批新增删除矩阵仍开放。

新发现并修补中：ACP台账新请求未同步进入旧usage投影，原097统计与539条旧投影一致不代表完整请求统计。读取层统一按精确acp:attempt_id去重；完整1416/0/6忽略、offline build及真实既有库HTTP核对通过，当前房间合计882，未知token保持未知。该源码候选本页真实SWE流式、881→882请求增量与统计/轨迹实拍已通过，原usage539不变，唯一island正常收尾；正式100已恢复，尚待新正式包独立复验，见[候选报告](../../testing/2026-10-08-acp-usage/report.md)，不追认100含统计修补。见[设计与审查取舍](acp-usage-read-model-design-2026-10-08.md)。

0.2.99当前增量：正式安装和[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.99)均已完成，冻结源码`23bbb94a0e4c2799f98025e48e41660206287698`；六门通过、1159安装文件逐长度/SHA一致，实际构建提交两路CI成功。工程树额度/旧索引下新文件精确搜索已在正式界面复验，长任务按完成时间重排已正式出包。结果结账但计划未回写的重启窗口现已独立通过，领取/唯一状态消息保持且5路扫描零重复、模型0。另一次真实SWE沿用唯一island-kayak，在DSH函数进入后正常停用，独立Win32宿主句柄Wait258→0，网络断开且无补发；原配置已恢复。见[099交接与实拍](../../testing/release-0.2.99/change-report-and-test-handoff.md)。

099之后候选增量：跨客户端聊天历史自动同步已修补，真实SWE方案/代码附件及计算器综合流程、无需刷新显示新任务和完整回复、保留历史滚动位置已实拍。初次数字分页400及临时候选漏DSH资源的失败均保留、修正后复验，详见[同步候选报告](../../testing/2026-10-08-chat-history-sync/report.md)。尚待新正式包、真实本页流式与断线期间大批新增/删除矩阵；不能追认099包含。Browser新原生Target替换及跨来源commit严格down/up时序、完整许可/会话/运维矩阵仍开放，整体Goal保持进行中。

前轮098追加（历史事实）：正式098真实定时器的停机到期已取消Goal收尾、结果已结账但计划发布失败后的正常扫描恢复通过，领取各1次且模型0次，见[专项](../../testing/release-0.2.98/scheduler-restart/report.md)。不外推真实模型调度或重启后的未回写投影。发现长任务用开始时间重排导致下轮已过期，已修补当前时间基准，11项调度/完整1411及offline build通过，和工程浏览修补一起仍待统一出包；a544415两路远端CI均success。

本快照回应“打开新的界面修改、汇总整体未完成验收项”。以正式安装实操记录和当前源码为依据，不把源码实现、编译通过或页面截图等同于功能完整验收。四项整合任务总体仍未完成。

前轮正式安装及公开预发布为 **0.2.98**，冻结源码 `cd80d492729b8b989d50a90ed3594b2df0503ee7`。1159安装文件逐长度/SHA一致、六门构建通过、构建提交两路CI成功；终端Unicode/容量/恢复及宿主异常退出独立句柄验证通过，见[098交接](../../testing/release-0.2.98/change-report-and-test-handoff.md)。文本附件→真实SKILL/计算器插件→七步原生Browser反馈复核和中文预览在097正式通过，见[097报告](../../testing/release-0.2.97/change-report-and-test-handoff.md)。下列旧版段落保留各轮事实。

## 本次打开的界面

- 已正常启动配套候选后台与桌面壳，原生控制台置于前台；最初展示SKILL页，随后切换Browser进行真实长程测试。
- 图标源码提交为 `968e10b`；49 个 SVG、3 个新透明 PNG 属于本轮统一玉石细金线方案。资源核验及14类页面截图见[图标报告](../../testing/2026-10-07-icon-audit/report.md)。
- 最初展示的是源码候选界面；当前已正常完整构建并安装0.2.96，源码参考 `572139a64bf9eef246cf696dd1ccd53f74b5146e`，权威源码快照 `eb93df587b80cee62e41eddb5e43b591d6735c6e32bf81eddd693db7ad2c14b7`，1159文件长度/SHA一致。095新增同进程子滚动、按下期间关闭及右栏切换三项正式实测通过；096新增失败审计修复正式复验及903×551窄低布局实拍。0.2.96已公开GitHub预发布，四份资产服务器端摘要与本地一致，标签指向实际构建提交；详见[096交接](../../testing/release-0.2.96/change-report-and-test-handoff.md)。094/095未单独发布。
- 原生宿主配套启动后已执行多轮真实SWE长程，宿主连接可用。修补大快照分页、离屏提示和子滚动进展后，复杂仿射9步整轮已由真实SWE完成，宿主succeeded/goal_achieved=true；见本日实机记录。该候选通过已在0.2.94正式安装版独立复验通过，首轮预算超时失败仍保留。
- 保留原 SWE-2-medium、原聊天室与唯一远端绑定；没有新建 Devin 云会话、切换模型或更改权限。

本次实际软件画面：[新版控件预览](../../testing/2026-10-08-ui-preview/new-controls.jpg)。

## 已有通过证据

1. **正式交付**：0.2.93 正常完整构建六门通过，Windows 正常安装成功，1154 个安装文件逐长度/SHA一致，四份公开分发资产一致。PR86 的文档证据已合入当前本地 main；旧交接文档中的“未自动合入”描述保留当时历史，不作为当前未合入判断。
2. **基础 Browser Use**：普通/SVG/Shadow、同进程 iframe、跨来源 OOPIF 的点击、输入、Enter、滚动及 LTR/RTL 边界已有真实 SWE 正负证据；过期引用、规划中焦点/子文档变化、父裁剪拒绝和错误目标零投递也有正式证据。这不代表复杂变换与全部时序边界通过。
3. **插件市场与生命周期**：真实 DSH 目录、搜索、详情及安装生命周期已有证据；0.2.91 宿主启动后取消通过，0.2.93 函数真实进入后取消通过，单次调用、无重试/响应体/迟到写回。
4. **图片与界面基本链路**：0.2.92 SWE 流式和普通入口两次真实图片识别通过，其中一次跨重启；统一模型页、右栏、搜索/索引、统计读取已有接线及正式页面复核。升级检查能正常结束；没有正式发布版本时显示“暂无正式版”符合当前预发布状态。
5. **启动及基本桌面操作**：完整舞剑演出、Esc/跳过、已运行重复启动已有证据；基本 Compute Use/Paint 输入已有证据。不能把它们重新列为完全未实现。

## 未完成项与推进顺序

| 优先级 | 项目 | 当前缺口与通过标准 |
| --- | --- | --- |
| P0 | Browser 复杂页面长程 | 旋转/斜切两层跨来源 OOPIF 的读取→输入/Enter→子滚动→正常导航→回父填回执整轮已在源码候选通过：9步全部投递并effect_observed，最终宿主succeeded/goal_achieved=true，原SWE和唯一island-kayak正常收尾。0.2.94正式安装版独立重试也已通过：9/9输入与释放、父子完成页面、可信提交、正常end_turn和绑定解锁全部留证；同进程独立子滚动事实已在095正式两步复验通过，第一步AX文字未变仍确认进展，父视口保持0。 |
| P0 | Browser 严格时序与变换负例 | 透视和父覆盖层负例已在配套源码候选由真实SWE验证，分别以变换不支持及命中不符明确拒绝，零投递；094正式安装版的透视与父覆盖层负例也已通过，零投递且可信事件为空。严格按下期间同步文档替换及同源history地址变化已094正式通过：原动作released，新文档没有click/input。跨来源HTTP导航已正式实测通过：navigation-started与beforeunload发生在pointerup前，新文档加载发生在释放后，新文档无click/input/key；不能宣称覆盖commit恰在按下期间。095正式按下期间正常关闭及右栏切换为设置的窄时序均已通过：原动作释放，后续观察停止且不补发。新原生Target替换与跨来源commit窄时序仍开放，不混算。规划期间变化已有通过证据，不能替代按下期间验收。 |
| P0 | 连续工具综合任务 | 0.2.94正式安装版综合任务已通过：选中SKILL、单次真实DSH计算器、5步Browser实际业务反馈继续规划，可信拒绝→配送确认→结算完成，CU成功、远端正常收尾并解锁。完整瞬时插件响应未单独持久化，已保留completed/ok台账与实际页面815交叉证据。更广的文件产物/跨重启恢复仍按后续工作包，不外推本次通过。沿用原 SWE-2-medium 与唯一 Devin 绑定，不重复简单问答，不创建多个云会话。 |
| P1 | 插件超时与许可竞争 | 0.2.94正式函数内层deadline和外层等待预算已补真实SWE/真实网络证据：约1.506秒及29.948秒断开、无响应体或补发，远端正常收尾。内层业务ok:false但宿主正常返回completed，不能误称成功抓取；外层宿主audit=timeout/台账failed。095真实函数GET进入后约24ms正常停用插件，约1.183秒断开等待连接且无响应体、无补发，单远端正常收尾，见[撤销验收](../../testing/2026-10-08-plugin-revocation/report.md)。此仅覆盖启用资格撤销，prepared及submitted阶段停用/重新启用后旧轮拒绝已在095正式补验通过，见[重新启用隔离](../../testing/2026-10-08-plugin-reactivation/report.md)；冻结授权扩张等窄竞争仍缺。099已补函数进入后停用的独立Win32宿主退出证明；所有清理阶段及后代树矩阵仍未完整。函数进入后取消已经正式通过，不重复列为未通过。 |
| P1 | 会话与附件边界 | 文本快照和历史投影、UTF-16预览修复已在097正式独立通过：UTF-8 CSV+UTF-16BE备注→实际calculator787→7步Browser反馈复核，四项HTTP400负例模型零派发，正式聊天室消息和中文预览实拍。见[097报告](../../testing/release-0.2.97/change-report-and-test-handoff.md)；候选失败与修复记录独立保留。GUI文件选择器未通过，API上传不替代其验收。账号过期、换模/rebind、PDF/音视频、Goal/Relay附件、纯文本转默认视觉Agnes、图片预算/不支持/群发等边界未完整。Devin Goal/Relay/子Agent入口仍明确拒绝，不能仅安排测试后宣称支持。 |
| P1 | 免费模型连接器 | OpenCode/HF 的插件入口及真实模型目录已通过正式页面验证；实际生成请求为0。填写各平台独立凭据后，仍需真实生成、多模态、SKILL/插件和 Browser 长程验收。不得复用其它平台 Key 或把目录声明当实际能力。 |
| P1 | 新图标正式交付 | 0.2.94已正式打包安装，49 SVG与3 PNG实际HTTP资源与安装文件全部一致；0.2.96已正常安装并公开预发布，上传居中修正包含。095的1443×563、096的903×897与903×551基本布局及可滚动配置右栏实拍通过。主窗口、长程开始/停止状态与工具泛光已有证据；全部动态跟随/暂停、审批状态及高DPI矩阵仍需补验。 |
| P2 | 启动特殊模式 | 资源加载失败、减弱动作模式及首次/重启边界未全验。完整舞剑本体、跳过与已运行重复启动不再重复作为未完成项。 |
| P2 | 搜索、统计与调度 | 097正式后端独立容量库的10k/100k搜索、分页/排序/定位、11万条FTS冷重建及并发历史读取已通过，正式页面搜索和第50000条定位实拍已归档，[容量报告](../../testing/2026-10-08-search-volume/report.md)。097正式原生UI的SWE→主聊天室→SWE统计切换、真实539/538请求归属、异房间消息ID不返回索引/耗时及未知token显示已通过，[统计报告](../../testing/2026-10-08-usage-scope/report.md)。跨客户端/重连历史自动同步已发现实际缺口，优先修补；多工作区/并发写入全矩阵以及真实模型定时触发、running未知结果和失败回退综合验收仍缺。098停机到期及099未回写投影重启恢复已分别通过，不重复列为全未通过。已有 Goal 有限重试/回退实现不等于完整持久调度闭环。 |
| P2 | 文件、记忆和终端生命周期 | 098正式终端Unicode、1MiB/17页、同句柄恢复、resize、异房间/旧句柄409、退出重建与关闭已通过；受控宿主异常退出后持有Win32后代句柄确认signaled，重启后absent/旧句柄409也已通过，[098报告](../../testing/release-0.2.98/change-report-and-test-handoff.md)。098正式LSP异常退出/显式重启/旧句柄拒绝及诊断定位现也已补验，[专项实拍](../../testing/release-0.2.98/lsp-lifecycle/report.md)；跨工作区完整矩阵、LSP启动进行中切换/全屏TUI、文件spill/GC/编辑冲突/引用保留及压缩图片证据/记忆污染仍缺。 |
| P2 | 架构整合与运维矩阵 | 共享异步 TurnRunner、跨进程单写者/outbox/会话权威 epoch 尚未完整；SessionConfig/ToolDispatch职责及重复hooks仍需收敛。凭据迁移恢复、升级失败与WAL备份/新写入保全、日志轮转/磁盘配额/脱敏导出、两实例/旧helper崩溃、多屏混合DPI/负坐标、权限多入口与句柄继承/隔离证据未全。SBOM/签名策略和feature映射仍待核实。 |

## 不作为当前阻塞的事项

- Paint 完整人物绘制按用户决定停止验收，保留基本 Compute Use 证据。
- 微信保留迁移功能不动、不测；Opus 额度受限暂停使用；主会话独立执行，不使用子代理。
- 全自动下载安装重启升级按既有第一阶段范围决定延期，当前只做版本检查与发布入口。
- MCP 非 stdio transport/OAuth、可选 DSH/PTC 试点按既有范围延期，不将延期能力宣传为已交付。

## 依据与后续交接

- [32工作包逐项复核](wbs-implementation-audit-2026-10-07.md)
- [当前验收队列及历史证据](current-acceptance-queue.md)
- [0.2.93正式交接](../../testing/release-0.2.93/change-report-and-test-handoff.md)
- [全图标检查报告](../../testing/2026-10-07-icon-audit/report.md)

下一轮继续新原生Target替换、跨来源commit窄时序与插件/会话剩余项。095同进程滚动、按下期间关闭及右栏内容切换已正式通过。SKILL/插件/Browser综合任务及插件内外超时已有094正式通过证据，不重复简单任务。每项区分源码候选与正式安装版，保留真实模型执行事实及实际软件截图；不以编译或静态资源通过替代模型实操。

## 本日继续推进补记

- 终端生命周期候选修复已纳入098正常完整release包并正式独立复验：Unicode、1MiB/17页、侧栏恢复、退出尾部、句柄隔离及重建关闭通过；正式受控宿主异常退出后，持有的后代Win32句柄确认signaled，重启后旧句柄409/终端absent，模型台账和唯一绑定不变。见[098交接与实拍](../../testing/release-0.2.98/change-report-and-test-handoff.md)。源码提交cd80d49两路CI success，1159安装文件逐SHA通过。原[候选专项](../../testing/2026-10-08-terminal-lifecycle/report.md)保留当时阶段与失败，不追认097含修复；LSP、全屏TUI及多工作区完整矩阵仍开放。

- 上传图标已修正光学居中与24px渲染，并纳入0.2.94正式安装版；安装资源与HTTP字节核验、当前尺寸实拍通过，全部DPI矩阵仍待补。见[本日图标与Browser记录](../../testing/2026-10-08-ui-preview/upload-and-browser-progress.md)。
- 长程重新执行已到真实规划请求，但第三方大回执溢出文件与原生文件禁用发生冲突；新增当前工具桥只读快照分页，保留完整规划内容、原图续接和动作校验。编译及3项协议回归通过；真实复杂9步长程已通过0.2.94正式复验。
- “当前右栏”误走扩展桥已修为提交时可见面板偏好，9项范围回归及两轮真实原生负例通过，已通过0.2.94正式安装复验。
- ChatGPT订阅登录调研确认官方独立注册与Responses接入路径；最小无工具真实实验已通过用户授权、7模型目录和单次 `gpt-6.1-sol` 完成回复，43 tokens，令牌未落盘；不代表正式Provider或工具链已集成。见[调研与实验边界](chatgpt-subscription-connectivity-research-2026-10-08.md)。
- 已建立持续Goal，按本汇总与32工作包继续，未将总体标为完成；Paint免测、微信不动、Opus暂停和不使用子代理保持不变。
- 0.2.94正常安装与身份核验完成，原SWE/唯一island-kayak已提交复杂仿射正式复验，未人工代做网页步骤；进度和实际画面见[094交接](../../testing/release-0.2.94/change-report-and-test-handoff.md)。

- 094正式复杂首轮8/9失败已归档：最后预检距离600秒CU截止仅394ms，最后点击明确未投递，不人工补点。旧轮正常结束后，仅将既有controller超时600调整900秒，正常重启，提交独立新轮继续验收；不修改已结束运行事实或权限。

- 094正式预算匹配新轮已completed，CU succeeded/goal_achieved=true，9/9输入且effect_observed、需要释放的动作均released，no_progress/replan均0；父子完成条件同时出现、可信最终提交accepted=true，唯一island-kayak正常end_turn/drained/解锁。首轮超时失败仍独立保留。

- 094同步替换正式实测：replacement-finished比真实pointerdown晚2.0ms，新文档pointerup比pointerdown晚10.5ms，目标HTML；新文档没有click/input/补发。CU succeeded、1步、released，唯一远端正常收尾。跨来源HTTP导航随后独立实测通过，导航发起及beforeunload在释放前、新文档加载在释放后，精确边界见094交接；不冒称commit发生于按下期间。

- 094综合流程正式通过：calculator单次completed/ok，CU单次perform、5/5输入及effect_observed，先触发业务配送反馈再确认重提，最终BAMBOO-ORDER-094/815/包邮；按SKILL三行回复。见[综合结果](../../testing/release-0.2.94/installed-validation/integration-result.json)与[最终实拍](../../testing/release-0.2.94/installed-validation/15-installed-integration-completed.jpg)。

- 094真实插件超时补验完成：[内层结果](../../testing/release-0.2.94/installed-validation/plugin-inner-deadline-result.json)、[外层结果](../../testing/release-0.2.94/installed-validation/plugin-outer-budget-result.json)及对应实拍已归档；只撤销本次新增工具白名单并停用测试net-tools，原计算器和唯一Devin绑定保持。独立Win32句柄/全部清理矩阵仍未完整。

- 同进程子文档自身视口缺失已修复，源码候选真实SWE单次perform两步滚动通过，第一步文字不变仍visible_progress=true，最终目标完成，no_progress/replan均0；原唯一远端正常收尾。见[候选报告、结构化结果与实拍](../../testing/2026-10-08-browser-sameprocess/report.md)。随后纳入095正式包并独立复验通过，不追认094包含该修复。

- 0.2.95已正常完整构建、六门pass、安装返回0、1159文件长度/SHA一致，正式配套启动，并已在同一SWE/island-kayak上独立完成下列三项正式复验。见[095交接](../../testing/release-0.2.95/change-report-and-test-handoff.md)。候选按下期间关闭第三轮命中：正常UI操作在pointerdown后207ms开始且释放前结束，原动作released，销毁在释放之后，后续观察明确停止且不补发；两轮未命中及模型误述均独立保留，[精确时序报告](../../testing/2026-10-08-browser-panel-window/report.md)。候选证据与下列正式复验独立归档；新原生Target替换及navigation commit窄时序仍不混算。

- 095正式三项新增验收闭环：子滚动两步visible_progress；按下期间关闭原动作released/资源终止；按下期间设置快捷按钮替换右栏，旧网页隐藏后观察停止且不补发。三轮均真实SWE/唯一island-kayak正常end_turn/drained/解锁，见[正式报告与全部截图](../../testing/release-0.2.95/change-report-and-test-handoff.md)。右栏内容替换不等同新原生Target替换。初次低高度拖拽未命中仍保留；后续经正常窗口菜单完成1443×563正式布局实拍，顶栏、权限、输入框、可滚动设置右栏及关闭后聊天扩展通过，高DPI仍开放。

- 095执行中插件停用专项已正式通过，见[真实函数撤销](../../testing/2026-10-08-plugin-revocation/report.md)。发现通用审计投影把失败误写为executed=false，后续源码已修正并完成1404项完整控制台回归及offline build；本项不追认095含修正。

- PR87两路远端检查发现旧图标静态断言未随资源升级；已更新为当前v3/SVG资源并保留交互断言，本地完整1404通过。最终HEAD远端结果与新审计投影正式复验仍待核验。

- 095正式插件重新启用补验：prepared阶段声明层拒绝、台账0；submitted阶段旧快照派发409、失败台账1；两轮真实HTTP0、唯一island-kayak正常收尾。仅关闭两阶段旧轮隔离缺口，不替代完整权限竞争矩阵。临时工具白名单与插件均已撤回。

- 096正常完整构建、六门pass、安装返回0、1159文件长度/SHA一致。真实GET后约25ms正常停用插件，连接约1.188秒后关闭无响应体；正式聊天正确摘录execute_requested=true、execute_allowed=true、executed=null，单次failed/cancelled，唯一SWE/island正常收尾并解锁。仅本轮新增白名单撤回，插件停用。见[096交接与实拍](../../testing/release-0.2.96/change-report-and-test-handoff.md)。完整瞬时回执仍未独立持久化，不将模型转述当原始回执。

- 096构建提交572139a两路远端CI均success。已上传并公开[0.2.96预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.96)，四份资产服务器端长度/SHA、实际标签提交核验一致，保留未签名及未整体验收的边界。903×551窄低窗口基本布局、配置侧栏内部滚动与正常关闭已补验，高DPI及动态审批仍开放。


## 本轮新增边界证据

- 098跨来源HTTP提交新增实测：真实SWE单click sent/released/effect_observed、新文档已加载且无旧输入；DOM没有捕获pointerup，无法证明commit恰在down/up之间，严格项保持开放。[事件、截图与结论](../../testing/2026-10-08-browser-commit/report.md)。不改变产品释放时序来制造通过。
- 098真实LSP异常退出、旧请求502无隐式重启、新句柄恢复、旧句柄409、正常关闭持有句柄signaled及右栏诊断定位实拍已完成。启动进行中切换和其它矩阵仍开放。工程大目录可见性及新文件精确路径搜索发现新问题，候选修补验证中，不追认098包含。

- 工程目录预算及新文件完整路径搜索候选修补已通过真实HTTP/正常UI打开，完整1409/0/6忽略、offline build通过；已恢复正式098，正式包复验待后续统一出包。[源码候选报告](../../testing/2026-10-08-project-browser/report.md)。

- 098正式真实LSP查找阶段/已启动阶段切换房间与工作区重载隔离、同房重新激活的世代拒绝及持有句柄退出已补验；严格A→B→A返回后仍在启动窗口未命中，保留开放。[竞争事实](../../testing/release-0.2.98/lsp-inflight/report.md)。

- 098正式共享ACP冻结授权扩大专项通过：submitted后恢复完全访问，8.818秒后旧轮单次read_file仍dry-run-only/require-approval；原SWE/唯一远端正常收尾，调试开关、工具和权限恢复。[实拍与审计](../../testing/release-0.2.98/frozen-expansion/report.md)。只关闭此共享授权路径，不外推全部DSH许可竞争。
