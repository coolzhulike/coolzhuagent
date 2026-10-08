# 当前验收队列（2026-10-07，0.2.93控件、模型目录及函数取消正式闭环）

当前权威状态：正式安装及公开预发布098；终端生命周期与异常宿主退出正式通过。098真实LSP异常退出/显式重启/旧句柄隔离、诊断定位及独立进程退出证明已补验，见[专项](../../testing/release-0.2.98/lsp-lifecycle/report.md)。跨来源提交新增一轮真实SWE导航/释放通过，但严格down/up提交窗口未获时间证据，[报告](../../testing/2026-10-08-browser-commit/report.md)。根文件被大型子目录额度挤掉及新文件搜索依赖旧索引的新问题正在源码候选修补。以下按日期追加段保留历史，不以旧097/候选段覆盖当前098状态。

2026-10-08终端增量：097之后源码候选修复补充平面字符输入、终端侧栏复开恢复及退出后分页尾部。真实ConPTY容量、归属、恢复、退出重建/关闭与原生UI证据通过，[专项报告](../../testing/2026-10-08-terminal-lifecycle/report.md)。完整控制台1407通过；未打包，不追认097。候选自有进程已清理，原工作区正式097后台恢复；LSP/全屏TUI/宿主崩溃重启及其余矩阵继续开放。此前dd03e52两路CI已success，新源码HEAD另核。

2026-10-08统计增量：097正式原生UI的SWE→主聊天室→SWE统计切换通过，539逻辑请求/538派发/4失败或中断/0进行中与只读真实台账相符，未知token保持未知；异房间消息ID不返回索引/耗时。模型新请求0、原唯一云端绑定不变。见[统计归属报告及原生实拍](../../testing/2026-10-08-usage-scope/report.md)。迟到响应和全部多工作区/并发矩阵仍开放。

2026-10-08追加：当前正式安装及公开预发布为097；构建提交0b4fddc和文档提交e3d802a两路CI均success。097独立容量库10k/100k消息搜索、稳定排序/分页/定位、11万条索引冷重建与并发历史读取通过，实拍见[容量报告](../../testing/2026-10-08-search-volume/report.md)。新原生Target正式实验确认generation 1→3且新页无旧输入，但关闭发生在释放后6.414秒，严格按下期间替换未命中，仍开放，[事实报告](../../testing/2026-10-08-native-target/report.md)。模型所述“期间替换”不符合时间证据，不采纳为通过。

2026-10-08当前状态以[整体验收快照](acceptance-summary-2026-10-08.md)为准：094已正式安装并完成复杂仿射长程、透视/遮挡拒绝、同步替换、HTTP导航精确边界、SKILL/插件/Browser综合流程以及插件内外超时。094尚未远端发布。0.2.95已正式安装、1159文件摘要一致；同进程子文档自身视口反馈、按下期间正常关闭、按下期间右栏切换到设置三项正式真实SWE实测已通过，见[095报告及截图](../../testing/release-0.2.95/change-report-and-test-handoff.md)。新原生Target替换、跨来源commit窄时序及插件许可竞争仍开放。095尚未远端发布。下文093及更旧状态保留历史，不作为当前未通过判定。

最新正式结论：PR85已合入4150fb8，0.2.93正常完整构建六门pass、Windows安装exit 0、1154安装文件逐长度/SHA核验通过，原工程和原安全库正常桌面入口恢复093。统一玉石控件正式实拍通过；OpenCode目录86项/免费聊天交集10项，HF目录448个模型及提供方路由，均有正式UI和产品接口证据。两平台未填凭据、未保存草稿、生成0，真实模型长程尚待验收。原SWE-2-medium/唯一island-kayak完成真实DSH函数进入后正常取消，一次调用、响应前连接关闭、无迟到响应、run与工具收束，测试插件和临时白名单正常撤回。正式内层deadline、外层预算超时及许可冻结窄竞争仍不据此关闭。来源PR85两路成功，冻结merge提交415的baseline一条成功，文档PR另核。见[093正式报告、截图及测试交接](../../testing/release-0.2.93/change-report-and-test-handoff.md)。下方092及候选描述保留为历史阶段事实，以本段为当前状态。


[093预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.93)已公开，四资产服务端及桌面分发长度/SHA核验一致，实际tag=4150fb8；当前日常正式安装093，正式证据由[PR86](https://github.com/coolzhulike/coolzhuagent/pull/86)承接，未自动合入。

092之后源码候选追加：用户已选择采用ImageGen统一玉石控件方向，资源、样式和原生实拍已完成；OpenCode/HF连接器提供插件市场统一配置入口，真实产品目录分别86项/455个模型及路由选项，平台凭据未填、生成及长程未验。真实SWE原唯一island-kayak已完成DSH网络函数进入后取消和外层等待预算超时清理，原取消点击过晚轮次保留。兼容入口`./`及Node内置导入的修补尚未正式出包，因此不追认092包含，也不关闭正式安装验收。PR84已由远端合入，新增源码从最新主分支46fb1b1另行承接，不更新已关闭PR。详见[函数专项](../../testing/2026-10-07-plugin-function/report.md)、[控件/连接器专项](../../testing/2026-10-07-jade-connectors/report.md)、[32工作包矩阵](wbs-implementation-audit-2026-10-07.md)。许可冻结窄竞争及其它原队列保持开放。

本表汇总剩余工作，历史失败与已通过证据以各版本报告为准。当前仅主会话实施与测试，真实模型使用原SWE-2-medium会话；Opus依用户要求暂不使用，微信不改不测。用户最新明确Paint不再测试，已有基本输入证据保留，Paint不再列为后续测试任务。

附件最新正式结论：0.2.92正常完整构建、六门、安装返回0及1150文件核验通过；原SWE-2-medium/唯一island-kayak完成原生流式图片20.6秒和宿主重启后普通图片21.3秒两项正式验收，新随机图形与数字全部正确。每轮一个attempt、end_turn/drained、实际图片1、无工具，正常原工程及安全库恢复092；1400控制台/16前端通过，冻结759两路CI success。候选与正式证据独立，不追认091包含。Goal/Relay、其它附件及纯文本视觉降级本轮复验仍开放。见[092正式报告及原图](../../testing/release-0.2.92/change-report-and-test-handoff.md)及[原候选](../../testing/2026-10-07-devin-attachments/report.md)。

[092预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.92)已公开，四项分发资产的服务端长度/SHA及实际标签759核验一致；当前日常正式安装092。PR84现已合入，下文其“开放”的描述保留为当时阶段事实。

最新插件结论：0.2.91正常安装、六门及1150文件核验通过；原SWE/island-kayak三轮正式实操完成配置变更后旧调用409拒绝且实际派发0、40次有界目录读取并发时新调用真实返回182且仅执行一次、execute模式宿主启动后正常取消并183毫秒内确认退出。完整Web1398/0/6忽略、插件44/0/1忽略、模块联动8通过，冻结a366两路CI success，原工程与安全库正常入口恢复091。090目录并发失败与b6远端失败保留，未追认090整体通过。函数执行中取消/超时及许可冻结窄竞争仍开放。见[091正式报告与截图](../../testing/release-0.2.91/change-report-and-test-handoff.md)及[原失败与候选](../../testing/2026-10-07-plugin-lifecycle/report.md)。

[091预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.91)已公开，四项分发资产服务端长度/SHA及标签a366核验一致；091为前一版交付事实；当前日常已安装092，下面089为更早交付事实。PR84说明已同步，未自动合入。

当前页入口最新正式结论：0.2.89正常安装1150文件及六门通过，原SWE/island-kayak四项实操通过：省略target的当前页点击与跨来源子滚动、明确错误URL零输入、严格规划期间刷新文档后旧引用零投递。候选两轮未命中时序保留，不追改；完整回归1397通过/6忽略。原工程正常桌面入口恢复，冻结5e545两路CI success。见[089正式交接与截图](../../testing/release-0.2.89/change-report-and-test-handoff.md)。[089预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.89)已公开，四资产服务端长度/SHA及实际tag=5e545均核验通过。

最新范围：0.2.86正常安装1150文件和六门核验通过，同一SWE-2/island-kayak完成OOP子输入/Enter、上下滚动、顶部零投递、规划期间焦点变化和子文档替换六项正式实操。两项时序负例均有请求<可信变化<回复及零文字证据。原失败与未命中时序另列，详见[086报告](../../testing/release-0.2.86/change-report-and-test-handoff.md)。本轮发现“内置原生浏览器”别名遗漏导致错误选择外部扩展路径，已独立补齐，087正常安装及真实SWE实操通过（子INSTALLED087、1/1，父0/0，3/3），不追认086包含；三层嵌套的左右同名兄弟按钮和父容器裁剪已在087正式补验通过；规划期间关闭第三轮严格命中、零投递，前两轮晚于回复单列。子横向LTR/RTL已在088正常安装版六轮通过；无URL当前页入口及严格规划期间刷新已在089正式通过；更复杂嵌套/旋转、严格按住跨URL、按下中关闭及面板替换仍开放，不外推全部HTML或四项总体完成。以下历次追加记录为当时阶段事实，以本段及最新版本报告为当前结论。

| 顺序 | 事项 | 当前状态 | 完成依据 |
| --- | --- | --- | --- |
| 1 | Browser普通文字/SVG子元素点击及混合指令路由 | 0.2.78正式安装版真实SWE通过；独立遮挡负例零输入 | 正式包逐文件核验后，同一SWE会话实操截图＋宿主释放回执＋网页事件 |
| 2 | 按下期间整页变化的释放边界 | 同步整页内容替换已在0.2.78正式版通过；跨URL导航严格时序仍开放 | 先区分内容替换与新文档导航，再证明变化发生在pointerdown和pointerup之间；禁止用点击后变化替代 |
| 3 | Browser其它复杂节点与宿主资源变化 | 正式普通独立子控件正/负例、Shadow/iframe父误击拒绝、初始面板关闭负例已通过；开放Shadow子点击与父误击拒绝已在0.2.79正式版通过；验证期间关闭能撤销旧证明已通过；同进程iframe子点击、父误击和规划期间子导航旧引用拒绝已在0.2.81正式版通过；同源同进程Frame编辑、Enter真实提交、父输入和规划期间焦点/文档变化零文字投递已在0.2.82正式版通过；同源同进程子向下/向上、到底零投递、子导航旧滚动引用拒绝及父滚动已在0.2.83正式版通过；跨来源/OOP子按钮、正向轴缩放、父遮挡和规划期间子导航拒绝已在0.2.85正式通过；0.2.86 OOP编辑/单Enter/上下滚动及规划中焦点/子文档变化拒绝正式通过；当前轮别名及三层左右同名兄弟点击、父容器裁剪、规划期间关闭旧引用零投递已在087正式安装版真实SWE通过；088子横向LTR/RTL双向及边界正式通过；无URL当前页及严格规划期间刷新已在089正式通过；更复杂嵌套/变换、按下中关闭及面板替换仍开放 | 实际页面或面板变更截图与对应零投递/释放结果，无失败补发；不把初始关闭或渲染当时序/内部自动化通过 |
| 4 | Compute Use多屏 | 基础Paint输入已有正式通过，多屏仍未补验 | 真实显示器布局、坐标与目标软件实操截图；环境不足时保留缺口，不虚构多屏 |
| 5 | 插件剩余生命周期 | 正式卸载、固定来源重装默认停用及重新启用已通过；091配置变化旧资格拒绝、目录并发新调用及宿主启动后取消通过；093函数执行中取消已正式通过；正式超时及许可冻结竞态仍开放 | 当前真实插件调用与取消/超时/授权事实逐项对应；旧失败请求不能复活，宿主启动取消不替代函数执行中取消 |
| 6 | 会话上下文与附件边界 | SWE原远端续接通过；092两入口图片直传及重启新图正式通过；Goal/Relay附件、账号过期及模型切换尚未补全 | 正常产品入口＋真实模型结果与台账；不注销用户账号制造负例，不调用受限Opus |
| 7 | 升级入口与后续自动升级 | 已采纳第一阶段为检查/发布页；086及087检查正常结束、版本正确；自动下载/安装/重启属于独立后续能力，未交付，不计当前检查失败 | 按既有UPD-01与Pro审查界定范围；完整自动升级若实施，另验官方资产身份、活动任务、安装交接和恢复 |
| 8 | 启动演出其它模式 | 演出及Esc/跳过按钮已有正式实操；091已运行再次启动恢复同一窗口、不重播通过；资源失败、减弱动作及首次边界仍开放 | 实际窗口截图、一次结束/交接事实；失败和兜底不当作完整舞剑通过，未生效的WebView2测试参数不计通过 |
| 9 | 四项任务总体复核 | 未完成 | 当前主会话先按最终代码、安装包及真实证据复核；Opus暂停不影响可执行工作继续推进 |

PR83已合并，当前新增修复与证据由[PR84](https://github.com/coolzhulike/coolzhuagent/pull/84)承接。0.2.77正式插件生命周期见[报告](../../testing/release-0.2.77/change-report-and-test-handoff.md)，0.2.78阶段事实见[本版改动与测试交接](../../testing/release-0.2.78/change-report-and-test-handoff.md)。

[0.2.78预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.78)已公开；四个分发资产服务端长度/SHA和实际标签源码0b已核验，正常日常启动恢复通过。

复杂节点补验、8767事件环境误判排除及正式8765地址同步反证见[078边界补验](../../testing/release-0.2.78/browser-boundary-followup.md)。开放Shadow实际缺口、职责边界、正式真实SWE正/负例及关闭时序见[079报告](../../testing/release-0.2.79/change-report-and-test-handoff.md)。

0.2.79 PR远端检查的SQLite首次并发打开锁冲突已接手：只读版本读取有界重试及阶段诊断，本地1390完整检查和10轮既有并发用例通过；不重放业务事务，不跳过失败。修补不在079包内；080正常安装及1150文件摘要通过，真实SWE正/负例31.4/21.1秒通过，cc6源码两条远端检查success。后续文档HEAD检查另核。见[080交接](../../testing/release-0.2.80/change-report-and-test-handoff.md)。

0.2.80正式证据及正常桌面入口恢复见[080交接](../../testing/release-0.2.80/change-report-and-test-handoff.md)。同进程iframe已按[文档绑定方案](native-browser-iframe-next-plan.md)在0.2.81实现并正式验收；跨进程独立Target和子编辑等仍需实施及实操。

0.2.80已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.80)：实际tag绑定cc6源码，四个上传资产的长度/SHA均匹配，见[服务端核验](../../testing/release-0.2.80/evidence/github-release-verification.json)。080交付事实保留；当前正常桌面入口已恢复到081。PR84待合入审查；同进程iframe已正式通过，跨进程和四项任务整体仍未完成。

0.2.81已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.81)：正常安装1150文件匹配，原SWE三轮25.2/34.0/32.0秒通过，d9产品源码两条远端检查success；四资产及标签源码核验、原工程日常入口恢复通过。见[081报告与原图](../../testing/release-0.2.81/change-report-and-test-handoff.md)。下一步继续Browser未验收边界，不把本轮同进程click外推为全部Browser完成。

0.2.82正式四项已闭环，40份安装版原始证据（含两轮未计通过的诊断）及正常入口恢复见[082报告](../../testing/release-0.2.82/change-report-and-test-handoff.md)。包冻结来源587两路远端检查success；出包后cfg(test)修正61a另核。仅证明同源同进程编辑和页面单键，不外推全部HTML、跨进程或四项总体完成。

082已公开预发布，四资产服务端摘要及实际标签587核验通过；出包后纯测试修正61a两路远端检查均success。当前桌面正常启动为082，下一优先项为同进程子Frame滚动。

0.2.83正常安装五项子滚动实操通过，原SWE续接、真实截图/事件/回执、完整包身份和日常恢复见[083交接](../../testing/release-0.2.83/change-report-and-test-handoff.md)。仅闭环同源同进程上下滚动，跨来源/OOP及其余总体队列仍开放。另复核浏览器保留标签标题与实际URL的状态同步。

0.2.83已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.83)：四个分发资产服务端长度/SHA256与本地匹配，实际tag绑定冻结产品0b；正常日常桌面入口已核验为083、原工程和原安全库。来源CI两路success，后续文档/新修补HEAD另核。公开核验见[服务端记录](../../testing/release-0.2.83/evidence/github-release-verification.json)。

0.2.84已正常安装，原SWE真实单次A→B导航28.1秒、普通后退/前进、关闭后保留不自动载入、显式重开B的页面/地址/标签同步通过；六门及1150文件匹配，正常原工程与安全库恢复。冻结产品82两路CI success。见[084交接](../../testing/release-0.2.84/change-report-and-test-handoff.md)。下一项跨来源/OOP方案已据官方ForSession接口复核，尚未实施；其余总队列保持开放。

0.2.84已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.84)，四资产服务端长度/SHA与本地相符，实际标签绑定冻结82；见[公开核验](../../testing/release-0.2.84/evidence/github-release-verification.json)。当前正常桌面入口为084，下一优先项仍为跨来源/OOP iframe。


0.2.85已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.85)，四资产服务端长度/SHA及实际tag=ae87均已核验，原工程正常桌面入口恢复为085。正式独立进程按钮/缩放及覆盖/规划期子导航已闭环，跨进程编辑/按键/滚动和其它队列继续开放。详见[085交接](../../testing/release-0.2.85/change-report-and-test-handoff.md)。


2026-10-06追加：用户清除Devin云端会话后，本轮通过正常重绑入口仅解除旧绑定，首轮创建唯一`island-kayak`并持续复用；内部规划不再创建云端会话。本地历史、失败和安全记录保留。候选OOP子输入、单Enter、上下滚动及顶部零投递已有真实证据，完整组合首轮的未知字段失败保留，不外推正式安装版。新单会话桥Paint实际W笔画已投递/释放，但模型未收到MCP原图，验收失败；正修复为同一ACP会话串行图片续轮。详见[单会话方案与本轮事实](devin-single-session-next-plan.md)。当前正常配套为候选调试环境；0.2.85是最新已安装发布版本，0.2.86尚未冻结出包。


同一云端上下文的候选Paint原图缺口已在`PAINT4`闭环：一个ACP attempt内串行1张规划原图/2张验收原图，新增一笔V并完成2/2原图验收；内部远端保持空。仅application匹配歧义的PAINT3为零输入失败，明确窗口的PAINT4成功，二者均保留；不外推任意应用窗口定位或正式安装版。显示交接提示修正后主控制台完整1395通过、6忽略，实际build和JS语法检查通过，正在继续候选实操与打包收尾。

后续PAINT5发现重启恢复图片历史被旧纯文本解析拒绝，零输入，原绑定保留。已修复历史图片只作固定类型元数据投影；最新PAINT6重启后续接同一island-kayak，实际新增L且原图验收2/2，外层completed/end_turn/drained，最终回复无交接文字。独立OOP FULL也完成真实click/type/Enter、子输入/提交1/1与父0/0、3/3 grounded；父页面回归真实输入/提交1/1，子FRESH086与1/1不变、4/4 grounded。原失败不追改，候选截图与回执见[086候选交接](../../testing/release-0.2.86/candidate-change-report.md)。正式打包安装及其余总体队列仍开放。

2026-10-07追加：用户明确Paint不用再测，已退出后续验收；原安全历史保持。内置原生浏览器别名已修复，原SWE/island-kayak单attempt完成click/type ALIAS087/Enter，子1/1父0/0、3/3；空闲完整1396通过，原并行争锁失败保留。此为086后源码候选，详见[别名修补与实拍](../../testing/2026-10-07-native-browser-alias/change-report.md)，不追认086包含。插件市场真实DSH目录4414项、calculator搜索3项与详情入口通过；其它队列继续按实际证据推进。


2026-10-07正式补记：087名称回归、六门、安装及1150文件、原SWE唯一island-kayak与日常入口恢复均通过，冻结a74两路CI success。详见[087报告](../../testing/release-0.2.87/change-report-and-test-handoff.md)。日常086设置载入/真实统计/升级正常结束及候选DSH4414目录、calculator3项搜索、详情/22文件来源检查已补实拍；不替代其它版本全部生命周期验证。Paint明确退出后续测试。自动升级完整链仍未实现，不能用本次正常安装追认。

087已公开预发布，四资产服务端摘要及tag=a74核验通过。升级第一阶段的检查/发布页与完整自动升级已按原UPD-01决定区分，不把已延期能力重复作为当前检查故障。

2026-10-07正式边界补验：087同一SWE/island-kayak完成三层左右同名按钮（两轮4/4）、父容器裁剪（not_sent、事件0）、规划期间关闭（严格请求<关闭区间<回复、not_sent、事件0）。两轮关闭晚于回复且实际点击已发送，未计通过，原始时序与失败保留。六轮单attempt end_turn/drained且内部远端为空，安全2未知/9closed保持，正常日常入口恢复。只补真实验收与报告，没有改产品源码或重打包。源码审视明确旋转owner与子文档横向/RTL目前为实现限制，不能外推已支持。见[正式补验与原图](../../testing/release-0.2.87/browser-nested-followup/report.md)。

2026-10-07追加：088最终6047bb3正常完整构建与安装1150文件通过，原SWE/island-kayak六轮子横向LTR/RTL双向及原点边界真实验收闭环；候选无URL失败、RTL原命中失败及独立诊断失败保留。74项桌面检查通过，原工程与安全库正常日常入口恢复。复杂变换、严格按住/面板变化等保持开放，无URL当前页自然语言请求纳入下一审视项。详见[088改动、原图与测试交接](../../testing/release-0.2.88/change-report-and-test-handoff.md)。

088已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.88)：四资产服务端大小/SHA256、实际tag=6047与本地验收包一致，原工程正常入口已恢复088。冻结产品两路CI success；文档HEAD检查另核。另一端横向极限及无URL当前页入口仍开放。见[公开核验](../../testing/release-0.2.88/evidence/github-release-verification.json)。


2026-10-08文本文件增量：0.2.96之后源码候选已补受控UTF-8/UTF-16文本快照、当前预算完整性和历史投影；真实SWE/唯一island-kayak由两份附件推导343和代号，完成7步Browser拒绝→复核→重提，单attempt end_turn/drained且解锁。四项附件负例400、零模型派发。正常点击UTF-16附件发现预览乱码，已修正并重启候选实拍。候选完整控制台1407/0/6忽略、联动8通过；正式安装复验待补。GUI文件选择器被实操工具跨进程目标识别限制，正常上传API不替代GUI通过。见[候选报告与原始实拍](../../testing/2026-10-08-text-attachments/report.md)。PDF/音视频、Goal/Relay附件及其它会话边界保持开放。


## 2026-10-08 0.2.97正式文本附件综合验收闭环

- 发送时受控正文快照/历史投影、UTF-16预览修复冻结0b4fddc；正常完整构建六门pass、管理员安装0、1159文件长度/SHA一致，构建源码两路CI成功。
- 原SWE-2-medium/唯一island-kayak读取新UTF-8 CSV与UTF-16BE备注，使用已选SKILL，仅调用一次真实calculator算得787，再以一次perform完成7步原生Browser，首次业务拒绝后勾复核重提。可信提交false→true、点击released、文本not_needed、CU成功，单attempt/end_turn/drained/绑定解锁。
- 正式聊天室正常加载本轮消息#665/#666及6分4秒耗时；点击UTF-16BE附件在右栏正确显示中文和实际代号。四项非法附件HTTP400，模型attempt927/消息1136前后均未增加。API上传不替代GUI选择器通过；该缺口保留。
- [097交接与实拍](../../testing/release-0.2.97/change-report-and-test-handoff.md)。已公开GitHub预发布四资产，服务器端长度/SHA与本地一致，tag0b4fddc；未签名，不冒称整体完成。
- 下一项优先浏览器新nativeTarget与跨来源commit窄时序，随后其它剩余矩阵；本轮7步文本综合不替代严格时序证明。ChatGPT订阅最小一次连通已通过，不再重复调用；正式订阅Provider不在该最小实验交付内。Goal保持active，Paint免测、微信不动、Opus暂停，不使用子代理。

## 2026-10-08 0.2.98正式终端生命周期闭环

- 正常完整release六门pass、管理员安装0、1159文件长度/SHA一致，cd80d49两路远端CI success。
- Unicode真实执行、1MiB环形缓冲17页、侧栏恢复同句柄与退出尾部、resize、异房间/旧句柄409及重建关闭正式复验通过。
- 自有正式宿主受控异常退出，持有Win32后代句柄从WAIT_TIMEOUT变为signaled，重启后terminal absent/旧句柄409；模型台账与唯一island-kayak绑定不变，新增模型请求0。
- [098交接和原生实拍](../../testing/release-0.2.98/change-report-and-test-handoff.md)。这是手动终端子系统验证，不宣称模型终端链通过；LSP/全屏TUI、多工作区完整矩阵仍开放。
- Browser新nativeTarget及跨来源commit窄时序继续优先，整体Goal保持active。
