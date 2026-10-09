# 04整合计划32工作包执行复核

会话配置读取快照候选：正式112真实并发复现参数16384/1024却返回预算8192/512；修补后名称/连接/参数/revision和预算使用同一捕获值，最终六路1136次读取/180次保存零混合，本地容量约束及同EXE重启正常原生实拍通过，offline build0/完整1423/0/6（lib8/native-host1）。模型0/原SWE不变；112不含此候选，待新包复验，完整Service/outbox及严格Browser仍开放。[原失败、最终候选事实与实拍](../../testing/2026-10-08-config-read-snapshot/report.md)。Goal继续。

112 Browser连续复验：只读观察器在登记后4.6152ms捕获阶段，分段等待盲区已消除；正常AX检查/关闭工具动作仍晚10960ms，超过5秒窗口。一次真实SWE/唯一island，sent/released保留、效果未知，父failed/CU blocked/panel_unavailable，单end_turn/drained并解锁；没有observation_stopped，严格项未通过，不重复简单点击碰窗口。模型配置读取快照一致性转为下一实施项。[原事实、时钟与实拍](../../testing/release-0.2.112/browser-continuous/report.md)。整体Goal继续。

112 Browser专项：一次真实SWE点击sent/released/effect_observed，父completed/单end_turn/drained并解锁；网页内部捕获登记后4.24ms的观察阶段，外部分段等待因调用间隙漏过，未做在途关闭。正常收尾关闭不计撤销通过。定位为验收驱动盲区，后续连续只读等待后正常GUI复验；不改产品期限或注入暂停。[原事实与实拍](../../testing/release-0.2.112/browser-pending/report.md)。严格项和整体Goal继续。

112正式增量：配置保存同一会话锁覆盖校验/参数发布/SQLite更新/失败回退，配置锁内捕获旧Option并恢复原缺项。正常release构建/安装0、六门pass、1159文件逐SHA一致，冻结72be2b5两路CI均success；已公开112预发布，四资产服务端摘要/实际标签匹配；正式Program Files EXE真实SQLite两组失败恢复、同revision并发200/409、正常原生GUI错误/重读/保存/同EXE重启四实拍通过。原SWE-2-medium/revision51/唯一island-kayak已恢复，模型0/新云端0。仅cfg(test)阶段期限修补已包含，产品预算不变；跨资源崩溃/完整Service/outbox及Browser严格时序仍开放。[112交接与实拍](../../testing/release-0.2.112/change-report-and-test-handoff.md)。整体Goal继续。

模型配置保存候选：同一会话锁覆盖校验/参数发布/SQLite更新/失败回退；配置锁内捕获旧Option并恢复原缺项。真实SQLite失败两组恢复、同revision并发200/409、正常GUI失败草稿/重读/保存/同EXE重启实拍通过，offline build0/完整1423/0/6（lib8/native-host1）。无模型请求、原SWE不动；正式111不含，待统一出包。跨资源崩溃/Service/outbox仍开放。[原事实与四张实拍](../../testing/2026-10-08-session-config-save-order/report.md)。


111追加实操：正式原生窗口经正常系统菜单缩到903×551，首页、设置内部滚动至保存入口、关闭恢复四张实拍通过；已正常最大化恢复，未保存原配置。本布局测试模型0，仅关闭本机窄低原生布局，不外推DPI/多屏。[原生证据](../../testing/release-0.2.111/native-low-height/report.md)。另一次真实SWE-2/唯一island-kayak Browser点击sent/released，正常关闭距观察登记5792ms，父failed/CU blocked、效果未知、单end_turn/drained并解锁；没有observation_stopped，严格在途撤销继续未通过。[原事实](../../testing/release-0.2.111/browser-pending-close/report.md)。最新测试修补提交3681bd6两路CI均success，独立[原始CI](../../testing/2026-10-08-acp-replay-test-budget/remote-ci/report.md)已归档；未追认111含该修补。整体Goal继续。


111正式增量：会话参数职责拆分与Browser失败回调归因已正常release构建/安装0、六门pass、1159文件逐SHA一致，原生主界面和设置读取实拍通过；冻结源码20facc6两路远端CI均success，已公开111预发布，四资产服务端摘要/标签匹配；原SWE/唯一island保持、模型0/新云端0。严格时序未因此关闭。前一1f59远端历史重放测试失败已保留，仅测试阶段预算修补后本地实际build0/完整1423/0/6通过，新提交远端另核；该测试修补不追认包含111。[交接与实拍](../../testing/release-0.2.111/change-report-and-test-handoff.md)。整体Goal继续。

Browser回调归因候选：已知资源/URL变化优先于失败HRESULT，避免通用观察错误覆盖确定原因；仅调整既有判断顺序，不改输入/权限/预算/重试。独立桌面offline build0、完整76/0/0回归通过，模型0/新云端0；正式110不含，严格down/up与在途资源撤销仍未实机闭环。[事实与边界](../../testing/2026-10-08-browser-read-attribution/report.md)。会话参数拆分将一并出包，Goal继续。

110低高度页面补验：正式服务903×551真实视口的首页、设置及内部滚动到保存实拍通过，79个已布局图片均加载；仅关闭网页布局子项，不替代原生壳/混合DPI/多屏。模型0/新云端0、SWE原绑定不变、临时视口已恢复。[实拍与边界](../../testing/2026-10-08-low-height-ui/report.md)。883e71e两路远端CI均success；参数拆分仍为未出包候选，整体Goal继续。

会话参数拆分候选：DTO/默认值、协议/地址、HTTP参数校验和预算约束移至独立session_model_config模块；五组提取前后独立比对一致。实际offline build0、完整Web1423/0/6既有忽略（另lib8/native-host1）通过；隔离真实服务正常GUI非法值拦截、后端400不改revision、有效保存与同EXE重启逐参数恢复实拍通过，模型0/新云端0/原库不写。正式110不含此候选，未追认SessionConfigService/Runner/outbox总体完成；Browser窄时序仍开放。[候选事实与实拍](../../testing/2026-10-08-session-config-boundary/report.md)。Goal继续。

110重连补验：正式EXE自有后台真实断线，副本新增450条/删除已加载3条/修改1条，原页面无需刷新或重选，自动跨页恢复527条，内容/顺序逐项匹配SQLite且重复0，包含首边界删除。驱动实际退出0，原库不写/模型0/新云端0；[实拍与原事实](../../testing/release-0.2.110/history-reconnect/report.md)。只关闭此组，流式/跨工作区竞争等仍开放。110已公开预发布，四资产服务器长度/SHA和实际tag733d5f3一致，未签名、不标latest，Goal继续。

110正式增量：独立diagnostics文件轮转正常发布/安装0、六门pass、1159文件逐摘要一致，构建源码两路CI成功。正式EXE的Windows进程间占锁启动684ms不阻塞、解除锁后622ms就绪并轮转两类诊断文件，旧代SHA完整保留、正常页面及原生窗口实拍通过；原SWE/唯一island保持，模型0/新云端0。23项诊断、完整1423/0/6既有忽略及联动8通过。只关闭该轮转子项，全局配额/其它生产者/故障/脱敏导出与Browser窄时序等仍开放。[110交接与实拍](../../testing/release-0.2.110/change-report-and-test-handoff.md)。Goal继续。

文件诊断轮转源码候选：diagnostics独立持有8 MiB/三代与64 KiB记录边界，Web面包屑复用；真实候选EXE的进程间OS锁竞争不阻塞启动、解除后两类文件轮转和正常聊天室实拍通过，模型0/新云端0，不修改权威审计。109不包含；正式新包复验待补，全局配额和脱敏导出继续开放。[方案](diagnostic-file-rotation-design-2026-10-08.md)、[候选原事实与实拍](../../testing/2026-10-08-diagnostic-rotation/report.md)。

109存储增量：正式安装EXE的隔离副本实操通过未checkpoint WAL进入产品迁移前备份、独立目录恢复及升级后新写入保全，正常聊天室实拍与独立库核对一致；真实索引名冲突使EXE退出1，版本/数据/结构全逻辑摘要原状回滚，撤销副本冲突后同EXE正常恢复。原运行库只读、模型0、新云端0。首次两次脚本准备失败保留；仅关闭6.2这些子项，未外推断电/磁盘满/全部升级矩阵。见[WAL与恢复原事实、实拍](../../testing/release-0.2.109/wal-recovery/report.md)。109已公开预发布，四资产服务器长度/SHA、实际tag09608d5一致；构建源码两路CI success。Goal继续。

109正式增量：请求登记关联轨迹已实施，正常构建/安装0、六门pass、1159文件逐摘要一致、1423/0/6既有忽略、构建源码两路CI success。原SWE/唯一island两轮均sent/released、单end_turn/drained且无补发。自然导航取得合法新观察（新页零输入）；正常关闭后panel_unavailable、效果未知。两轮均未命中精确等待资源撤销，原事实与[109实拍交接](../../testing/release-0.2.109/change-report-and-test-handoff.md)保留。已纠正“自然导航等同面板资源变化”的测试前提；窄时序与其它总体矩阵仍开放，Goal继续。

108正式增量：观察失败阶段最小轨迹已实施，离线build0/完整1423通过/0失败/6既有忽略，正常构建六门pass、安装0、1159文件逐摘要一致。原SWE/唯一island-kayak一次真实工具sent/released、新页零输入、单end_turn/drained并解锁；释放后230.1ms正常导航得到合法新观察，最终budget_exhausted，未命中底层等待分支。CU observing不能证明底层请求已登记，后续增加请求关联标记作精准复验；原事实与[108实拍交接](../../testing/release-0.2.108/change-report-and-test-handoff.md)保留。总体Goal继续，严格时序及其它矩阵仍开放。

107已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.107)：四资产服务器长度/SHA和实际tag=314671b核验通过；未签名、不标latest。当前正式107原生窗口保持打开，精确资源变化等待分支和总体未完成矩阵继续开放。

107正式补记：正常release构建/安装0、六门pass、1159文件逐摘要一致，冻结314671b，两路远端CI success。四轮真实SWE/唯一island-kayak均单调用/end_turn/drained且无补发；三轮预设窗口未命中，有限网页跳转触发native_browser_observation_stale并完整保留sent/released、效果未知，新页零输入。精确native_browser_resource_changed等待分支和严格按下窗口仍开放，不冒称本版实机闭环。见[107原事实、截图与交接](../../testing/release-0.2.107/change-report-and-test-handoff.md)。其它矩阵和整体Goal继续。

106正式补记：正常release实际子退出0、六门pass、安装0、1159文件逐摘要一致；冻结c9c61fe，确切MSI带时间戳。原SWE-2-medium/唯一island-kayak两轮真实终态已验证input_steps：成功为sent/released/effect_observed/passed，失败为sent/released/效果未知，单调用/end_turn/drained且无补发。第一轮导航晚于终态、负例未命中已保留；第二轮观察超时负例和实拍通过。只关闭最终释放字段缺口；新发现待处理观察资源变化落为通用timeout，原因识别待修，严格按下窗口仍开放。见[106交接、失败事实与实拍](../../testing/release-0.2.106/change-report-and-test-handoff.md)。其它矩阵和整体Goal继续。

105 Browser观察失效补记：一次真实SWE调用、旧页可信输入sent/released，新页在pointerup后约5073.4ms载入且零输入；最终native_browser_observation_stale、无补发、单end_turn/drained并解锁。该轮不属于严格按下期间导航，也未触发native_browser_resource_changed观察入口分支。发现最终回执缺逐步释放字段，已候选补充同一SQLite步骤投影input_steps及7个提前返回值一致性修复；完整Web1423/0/6既有忽略、核心143通过，尚待106正式安装真实复验。见[105专项、截图与原失败](../../testing/release-0.2.105/browser-observation/report.md)。其它总体矩阵仍开放，Goal继续。

105执行中配置更换补记：真实网络GET1后正常更换配置，原连接在约1.845秒内关闭，无响应体/补发；独立持有旧宿主进程句柄Wait258→0，脚本未终止宿主。工具failed、父completed/end_turn/drained、唯一远端解锁，正常恢复revision51。执行已进入，不写成零执行；完整瞬时工具回执未另存，清理以独立句柄证据为准。[105实拍与交接](../../testing/release-0.2.105/change-report-and-test-handoff.md)。105已公开，四资产及tag摘要一致，构建源码两路CI success。其它总体矩阵继续开放。

105正式补记：插件重新配置旧轮拒绝的具体原因传回与独立阶段事件已正式复验通过。真实SWE-2/唯一island-kayak，配置先提交，工具1条failed、GET0，tool.dispatch_rejected准确标after_admission_before_executor/dsh_action_not_live/executed=false；父completed/end_turn/drained，正常恢复revision49。正常release六门、构建/安装0、1159文件逐SHA一致，冻结aaf39cd。见[105交接与实拍](../../testing/release-0.2.105/change-report-and-test-handoff.md)。104缺诊断是历史阶段；其它矩阵仍开放，Goal继续。

104正式补记：冻结6fad837，正常发布链真实子退出0、六门pass、MSI安装0、1159文件逐长度/SHA匹配。真实SWE-2-medium/唯一island-kayak，一次HTTP GET完整44498字节资料，完整工具回执111227字节；独立tool.result_page_read记录107200→111227，模型读到随机尾文、calculator一次完成正确149548450，两个工具completed、父completed/end_turn/drained、无read_file/无新云端会话，约60.3秒。正常原生实拍与原事实见[104交接](../../testing/release-0.2.104/change-report-and-test-handoff.md)。临时白名单/插件已恢复revision45，服务器正常停止。仅关闭本轮ACP无read_file大结果尾文→工具续接；其它Provider、GC/压缩及完整资格矩阵不外推。102原失败、模型字节数误称及首次采证列名错误保留并纠正。Browser严格时序与其它总体矩阵继续开放，Goal保持进行中。下方“分页候选未实机”属于历史阶段，不覆盖本段正式事实。

103正式补记：长任务卡片已正常安装、1159文件逐摘要通过，折叠/展开/全文内部滚动均有实拍，见[103交接](../../testing/release-0.2.103/change-report-and-test-handoff.md)。102大结果真实链路未通过：HTTP完整、磁盘原文逐字节完整，但模型下游截短，尾文不可见，calculator未执行；[失败事实](../../testing/release-0.2.102/large-result-failure/report.md)保留。已实现独立本轮回执分页源码候选，不开放任意文件，offline build与完整1422/0/6既有忽略通过；未出包实机，不算闭环。原Browser严格时序及其它未完成矩阵保持开放。

0.2.102正式增量：冻结95e0140，六门pass、安装返回0、1159文件逐长度/SHA一致，构建源码两路CI success。真实SWE-2-medium/唯一island-kayak定时任务固定原结果房间，后台执行期间当前主聊天室不切换；两工具completed、材料未误判CU、父completed/end_turn/drained，一次领取，正常重启五次并发扫描零重放。插件源码变化真实复验新增独立tool.dispatch_rejected/tool_not_live/before_dispatch/executed=false，零登记/零GET，排空后精确恢复及撤回临时配置。见[102交接、原事实与实拍](../../testing/release-0.2.102/change-report-and-test-handoff.md)。只关闭这些子项，Browser新Target/跨来源commit严格按下窗口、调度未知结果/失败矩阵、插件许可/配置及大结果端到端仍开放。长任务卡片布局候选已修补并offline build通过，未包含102，后续正式实拍待补。下列101及源码候选段落保留历史阶段，不代表当前仍运行101。

101正式DSH源码变化观察：真实SWE submitted后仅改变既有验收插件的一处无语义注释，工具执行登记0、独立网络GET0，父run completed/ACP end_turn/drained/唯一绑定解锁；排空后按SHA恢复，临时白名单及插件启用状态恢复、revision37。具体登记前拒绝未独立持久化，仅有模型转述，因此只关闭零派发与清理观察，不追认完整源码/许可竞争矩阵通过。见[原事实与实拍](../../testing/release-0.2.101/plugin-source-race/report.md)。Browser严格时序及源码候选正式复验继续开放。

101正式原生视图替换试验未命中严格窗口：旧网页trusted down/up/click与步骤sent/released一致，新网页loaded晚于旧up约10.24秒、新页输入0；单次真实SWE/原island正常end_turn/drained/解锁，但后续观察因资源变化停止，CU blocked/父run failed。另发现终态“没有发送输入”错误覆盖已投递事实，源码候选改为观察停止、以步骤回执为准；未安装，不计正式修复通过。新[原事实与实拍](../../testing/release-0.2.101/browser-native-replacement/report.md)保留未命中及模型错误转述。原文投影候选另收敛为ACP只在外桥保存一次、非ACP无实际读取能力证明时完整交回，不默认续读指针；见[更新方案](tool-result-readability-design-2026-10-08.md)。整体Goal继续，严格时序及候选正式复验均开放。

源码候选新增两处修补：ACP大工具结果在未开放read_file时仍保存原文并完整交回；真实SWE源码审查发现材料中的click/CU入口被误判为操作要求，导致工具completed/ok和ACP end_turn/drained却父run failed，已缩窄未执行提醒的材料边界。旧失败事实保留，唯一island-kayak/revision35保持；offline build、完整1418/0/6忽略通过，但修补尚未出包实机，不能关闭验收。见[方案](tool-result-readability-design-2026-10-08.md)及[真实失败、审查与实拍](../../testing/2026-10-08-spill-capability/report.md)。

101正式4.3增量：右栏编辑器read-before-edit冲突拒绝、取消重载保留本地修改及独立同revision并发保存一方200/另一方409已通过。采证脚本失败独立保留，模型0、唯一远端不变；[原回执、正常软件实拍与范围](../../testing/release-0.2.101/file-conflict/report.md)。仅关闭对应文件子项，模型编辑工具/spill/GC/引用图继续开放。

101正式新增1.4/5.4子项：真实LSP服务已启动但未发布期间的房间A→B→A已命中，返回A时旧请求和原进程仍存活，随后409且持有句柄退出，无旧实例发布；模型0、唯一远端不变。[正式原事实与实拍](../../testing/release-0.2.101/lsp-aba/report.md)。跨工程启动pin拒绝变化和正常A→B→A回收/旧句柄隔离另轮已通过；其它完整矩阵继续开放。核对原计划5.4，其通过标准明确“不宣称全屏TUI全支持”；后续核验受控PTY边界，不将全部TUI支持另行扩大为必须交付项。

0.2.101当前正式增量：正常完整构建、六门pass、1159安装文件逐长度/SHA一致，安装返回0、正式原生后台与壳启动；冻结源码54594d9，两路远端CI均success。真实SWE-2-medium/唯一island-kayak完成本页设计审查及单次DSH工具，#683/#684无需刷新出现；统计侧栏自动882→883，明确派发875→876，539条旧投影不变，未知用量保持未知；本轮精确ACP轨迹实拍已通过。[101交接与实拍](../../testing/release-0.2.101/change-report-and-test-handoff.md)。已公开[101预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.101)，四资产服务器长度/SHA及实际标签与构建提交一致；断线大批变动、完整多工作区并发和Browser严格时序仍开放。

ACP统计遗漏已在正式101修补及独立复验：HTTP与权威ACP台账在读取层按精确ID和作用域去重，不增加第二账本；未发送/结果未知/派发未知独立显示，缺时间与token不补造。完整1416/0/6忽略、offline build和真实HTTP/journal逐项核对通过。[设计与审查取舍](acp-usage-read-model-design-2026-10-08.md)；候选881→882与正式882→883分开保留，不追认100含修补。

0.2.99当前增量：正式安装和[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.99)均已完成，冻结源码`23bbb94a0e4c2799f98025e48e41660206287698`；六门通过、1159安装文件逐长度/SHA一致，实际构建提交两路CI成功。工程树额度/旧索引下新文件精确搜索已在正式界面复验，长任务按完成时间重排已正式出包。结果结账但计划未回写的重启窗口现已独立通过，领取/唯一状态消息保持且5路扫描零重复、模型0。另一次真实SWE沿用唯一island-kayak，在DSH函数进入后正常停用，独立Win32宿主句柄Wait258→0，网络断开且无补发；原配置已恢复。见[099交接与实拍](../../testing/release-0.2.99/change-report-and-test-handoff.md)。

099之后候选增量：跨客户端聊天历史自动同步已修补，真实SWE方案/代码附件及计算器综合流程、无需刷新显示新任务和完整回复、保留历史滚动位置已实拍。初次数字分页400及临时候选漏DSH资源的失败均保留、修正后复验，详见[同步候选报告](../../testing/2026-10-08-chat-history-sync/report.md)。尚待新正式包、真实本页流式与断线期间大批新增/删除矩阵；不能追认099包含。Browser新原生Target替换及跨来源commit严格down/up时序、完整许可/会话/运维矩阵仍开放，整体Goal保持进行中。

以下为各轮历史证据，候选/待出包表述以099当前增量为准，未外推整工作包完成。

4.5增量：正式098真实定时器启动补偿已取消Goal、一次持久领取/终态并发扫描，以及结果已结账但配置发布失败的正常扫描投影恢复通过。模型0次，不外推Devin调度；poll固定系统房间与唯一远端约束冲突，未新建云端测试。长任务重排使用旧扫描时间的问题已修补，完整1411/11项专项及offline build通过，仍待统一出包，见[正式证据](../../testing/release-0.2.98/scheduler-restart/report.md)和[候选](../../testing/2026-10-08-scheduler/report.md)。

0.2.98追加正式实测：共享ACP在接纳后扩大权限，旧轮实际read_file仍被冻结授权拒绝，一轮真实SWE/唯一island-kayak完整收尾，见[权限专项](../../testing/release-0.2.98/frozen-expansion/report.md)。LSP真实工具查找、服务已spawn未发布、同房重新激活三种启动窗口均拒绝旧scope，持有Win32进程句柄确认退出，见[竞争专项](../../testing/release-0.2.98/lsp-inflight/report.md)。只缩减1.4/5.4相应子项，不外推DSH全部许可或严格A→B→A。工程树额度及新文件精确搜索修复已经提交a544415、完整控制台1409通过，仍是源码候选，不在098正式包内。

0.2.97增量：文本附件快照/历史投影及UTF-16侧栏预览修复已正式安装独立复验；真实SWE沿用唯一远端，按SKILL调用calculator后完成七步Browser复核流程。四项无效附件零模型派发，构建六门、1159安装文件和两路源码CI均通过，已公开预发布。见[097交接](../../testing/release-0.2.97/change-report-and-test-handoff.md)。仅缩减2.1/3.3/4.1/4.3的相关子项，GUI选择器和其它矩阵仍开放。

2026-10-08增量以[总体快照](acceptance-summary-2026-10-08.md)及[094正式报告](../../testing/release-0.2.94/change-report-and-test-handoff.md)为准：0.2.94复杂仿射9步长程、SKILL/插件/Browser综合流程、函数内deadline与外层预算已正式通过。因此表中0.2/1.5/3.3/3.4相关旧缺口应按具体已通过子项缩减，不能重复宣称全部尚未执行，也不能将部分通过外推整个工作包完成。0.2.95已完整构建安装、1159文件核验通过，新增同进程子文档视口反馈、按下期间正常关闭及右栏内容切换三项正式实测已通过，见[095正式报告](../../testing/release-0.2.95/change-report-and-test-handoff.md)；[候选按下期间关闭](../../testing/2026-10-08-browser-panel-window/report.md)第三轮已命中，前两轮未计通过。其它架构/环境矩阵仍开放。

依据桌面`04-coolzhuagent-整合改进执行计划.md`第4节、当前源码和已归档安装版真实证据。主会话独立复核，Opus暂停，不使用子代理。下表主体为10月7日历史快照，最新子项结论以上述增量和10月8日总体快照为准。下表不是把“文件存在/测试通过”等同整包完成：代码接线、源码候选、正式安装实操分别列证据；缺口继续开放，不编造总体完成百分比。

用户后续要求覆盖原计划中的假模型主验收与完整Paint实验：真实模型用原SWE-2-medium/唯一island-kayak，重点长程、连续工具、Browser Use；Paint不再测试，微信仅保留迁移功能。现有单元回归可执行，但不新增假模型冒充真实功能验收。

| WBS | 当前证据与判断 | 遗留/下一步 |
| --- | --- | --- |
| 0.1 身份与证据 | 093正式源码415、MSI、六门、1154文件摘要和四个发布资产已对应，版本报告逐项区分候选/正式。 | 后续新增源码继续单独冻结出包，不能追认旧包包含。 |
| 0.2 基线 | 既有离线回归和真实模型台账并存；用户要求真实模型，原假模型黄金基线不再作为产品验收方式。 | 连续工具长程路径仍需本轮集中补验，避免重复简单任务。 |
| 0.3 CI与假成功 | 产品PR85两路成功、冻结merge提交415的baseline一条成功；插件市场正常目录/安装能力已有真实证据。 | 正式证据PR86按最终HEAD另核；未实现能力继续明确报错。 |
| 1.1 身份/终态契约 | 运行契约、请求attempt、聊天终态与CU事实已接线。 | 所有入口同一契约的完整矩阵尚未闭环。 |
| 1.2 输入事实/回执 | `computer_use_store.rs`、`fact_log_sqlite.rs`及executor生产事务接线；零投递/已投递与释放分开报告。 | 不能将所有来源、丢回执和迟到路径都视为完整覆盖。 |
| 1.3 broker/lease | 产品输入所有权、隔离与进程身份核验已存在，历史未知记录保留。 | 两实例/崩溃旧helper完整正式矩阵仍缺。 |
| 1.4 权限 | 动态未知工具拒绝、聊天室完全访问与目录权限、插件授权资格均有真实正负证据。 | 各调度/子Agent入口统一最低权限全矩阵未完成；不能以完全访问抹除未知输入事实。 |
| 1.5 取消/预算 | CU基本释放、091插件宿主启动取消正式通过；093真实函数进入后取消正式通过，外层等待预算超时仅候选证据。 | 094正式插件内层deadline/外层等待预算通过，095启用资格撤销及重新启用旧轮隔离通过，099函数停用后Win32宿主退出通过；授权扩张冻结及全部清理/后代树矩阵仍开放。 |
| 2.1 SessionConfig | 统一模型页、revision、协议/地址/密钥和图片三态已接线；本轮连接器复用此边界。 | 参数DTO/协议/地址/校验/预算规则已抽独立模块，候选GUI保存重载通过并已纳入正式111；发布/会话更新仍在主层；同进程保存顺序及SQLite失败回退已在112正式安装版通过两路径失败、并发200/409及原生GUI保存重启实拍，跨资源崩溃/完整Service仍开放。新平台需凭据后验证实际wire。 |
| 2.2 历史/usage | `chat_tool_history.rs`、`chat_insights.rs`及请求尝试账本已拆出；统计页读取正常。 | 迟到/嵌套分账全矩阵和统一服务收敛未完整。 |
| 2.3 工具派发 | registry、权限闸门和插件生命周期已有真实接线。 | `main.rs`重复路径与hook语义尚未全部收敛为ToolDispatchService。 |
| 2.4 单写者/outbox/epoch | SQLite迁移、事务事实与生产后端已存在。 | **跨进程单写者、outbox、会话权威epoch未完整**，不能以SQLite已有表判完成。 |
| 2.5 共享异步TurnRunner | CLI `shared_chat.rs`已走共享HTTP/SSE入口，不是仅本地Vec模拟；Web仍有普通与流式独立循环。 | Web/CLI/Goal/接力统一异步运行器未完成；Devin Goal/接力/子Agent入口仍明确拒绝。 |
| 3.1 Frame/DPI | 原生帧身份、目标映射和浏览器轴缩放已有正式实操。 | 混合DPI、负坐标与真实多屏矩阵缺环境证据。 |
| 3.2 UIA/语义动作 | 正式Browser同/跨进程子控件、输入、Enter、滚动已有证据，桌面基本输入可用。 | 不外推任意Windows控件pattern能力；完整语义能力表未验。 |
| 3.3 有界反馈/验证 | 实际图片与事实grounding、零动作/blocked不冒领，SWE基本CU通过。 | 长程子目标与失败再规划综合任务须集中补验。 |
| 3.4 失配/settle | 正式过期引用、焦点/子文档变化、裁剪与OOP边界均已实测。 | 094正式旋转/斜切跨来源9步长程及透视/父覆盖层零投递负例通过，095按下期间正常关闭/设置替换通过；非目标误拒率及严格新原生Target/跨来源commit、在途撤销继续开放。 |
| 3.5 Paint实验 | 基本输入正式证据保留；用户明确停止完整Paint绘画验收。 | E1–E5/L1/L2整套不再作为本次阻塞；不得宣传未完成绘图优化。 |
| 4.1 前端模块/面板 | 快捷轨、右栏、顶部下拉、内容预览、独立模块已落地；093玉石控件正式原生实拍通过。 | `app.js`仍大；903×551 Web布局正式110通过；正式111原生同尺寸首页、设置内部滚动与关闭恢复实拍通过，混合DPI/多屏仍待补。 |
| 4.2 轨迹/统计/FTS | 搜索/索引、用量、临时过程与独立轨迹已接线，正式页面读取复核通过；097独立容量库10k/100k搜索、排序/分页/定位和11万条FTS冷重建/并发历史读取通过，已留界面实拍。 | 097正式原生UI的真实SWE/空房间统计隔离、切回恢复、异房间消息ID隔离和未知token已通过，见[统计报告](../../testing/2026-10-08-usage-scope/report.md)；重连迟到、多工作区和模型写入竞争全矩阵仍未完整；见[容量报告](../../testing/2026-10-08-search-volume/report.md)。 |
| 4.3 文件/附件 | 文件服务与附件引用、图片校验已有接线；092两入口和重启新图正式通过。 | 101正式编辑器旧版本冲突/本地修改保留及同revision并发保存已通过，[专项](../../testing/release-0.2.101/file-conflict/report.md)；模型编辑工具、spill/GC及所有有效引用保留的完整演练未完成。 |
| 4.4 SKILL/记忆 | 已有技能发现与记忆实现，不能再另起重复存储。 | 压缩保留图像证据、污染记忆与边界全验收仍缺。 |
| 4.5 Goal/调度 | Goal GL-09/10/13已有失败回退、human_ack与有限重试实现；定时任务页正常读取。 | 102正式真实SWE定时固定原结果房间投递、后台执行不切当前房间、重启五次并发扫描零重放通过；共享Runner/durable job收敛、running未知结果与失败回退全矩阵仍开放。 |
| 5.1 凭据 | Windows受保护存储`secret_protection.rs`已存在，配置密钥不回显，新连接器不继承别平台密钥。 | 中途失败/换设备恢复与全来源迁移矩阵未完整。 |
| 5.2 隔离/监督 | 固定来源、独立宿主、子进程监督与权限资格已实现部分；真实插件取消证据存在。 | 独立进程/固定来源不能当OS沙箱证明；完整逃逸/句柄继承矩阵未验。 |
| 5.3 MCP | 当前Web正式宿主只接stdio，其它transport按已有范围裁决延期。 | OAuth/所有transport生命周期不宣称已交付；保持原兼容名与碰撞检查。 |
| 5.4 LSP/PTY | `lsp_host.rs`、`terminal_host.rs`真实模块已接线，非只有按钮。 | 098正常release及1159安装文件SHA通过；真实Unicode、1MiB/17页、侧栏恢复/退出尾部、归属409及重建关闭已正式独立通过。受控宿主异常退出后持有Win32后代句柄确认signaled，重启后旧句柄409/terminal absent；见[098交接](../../testing/release-0.2.98/change-report-and-test-handoff.md)。098真实LSP异常退出、显式重启、旧句柄拒绝、定义/引用恢复和诊断定位、正常close持有句柄退出已补验，[专项](../../testing/release-0.2.98/lsp-lifecycle/report.md)。101正式房间ABA、启动pin拒绝工程切换/重载及完成后跨工程回收/旧句柄隔离已通过，[专项](../../testing/release-0.2.101/lsp-aba/report.md)。其它跨工作区/权限矩阵仍开放；全屏TUI全支持不是原计划5.4交付要求。 |
| 5.5 连接器 | Browser独立Target/nonce/重启已有多版本证据；语音/桌宠领域保留。 | 微信不改不测；其它连接器共享取消身份矩阵尚未全完成。 |
| 6.1 运维/诊断 | 运行轨迹、健康与审计页面存在，调试信息移至轨迹，未默认外发遥测。 | 文件轮转及Windows锁竞争正式110通过；全局配额/其它生产者/故障/完整脱敏导出仍缺。 |
| 6.2 迁移/升级 | 多轮正常MSI安装、自检与原工程恢复通过，当前正式112。 | 正式109已过未checkpoint WAL进入产品备份、独立恢复、新写入保全和迁移索引冲突回滚；断电/磁盘满/全部旧版矩阵仍缺。 |
| 6.3 Windows矩阵 | 093正常完整安装及原生窗口通过；多版同一模型Browser实操留证，不使用headless代替。 | 正式111本机原生903×551及设置滚动通过；多屏/DPI及启动资源失败/减弱动作仍开放。 |
| 6.4 文档/交付 | 版本、源码、发布资产哈希、改动和测试交接已归档；本轮新增两个专项报告。 | 四项总体仍未完成，SBOM/签名策略与所有feature映射须继续核实。 |

S7可选DSH试点不属于32工作包总体完成前提。真实DSH市场及插件已接入不等于完整PTC/自治工作流或Devin子Agent已开放。

本轮源码、正常构建安装、玉石控件/目录及函数取消正式验收和GitHub四资产发布已归档，见[093正式交接](../../testing/release-0.2.93/change-report-and-test-handoff.md)。优先顺序转为长程连续工具与Browser资源变化验收，再处理总队列启动/会话/插件剩余项。OpenCode/HF真实模型验收待用户填各平台凭据，不能偷偷复用Qwen密钥或生成新的Devin云端会话绕开。具体当前正式事实见[验收队列](current-acceptance-queue.md)，原候选沿革见[插件函数报告](../../testing/2026-10-07-plugin-function/report.md)、[视觉与连接器报告](../../testing/2026-10-07-jade-connectors/report.md)。


## 2026-10-08 正式104插件重新配置竞争补记（部分通过）

配置提交明确早于旧工具登记，真实SWE-2/唯一island-kayak网络GET为0，工具1条failed、父轮completed/end_turn/drained。104仅传回409，独立拒绝诊断缺失；候选补具体原因及after_admission_before_executor事件，offline build与DSH专项8通过/1既有跳过，尚未出包复验。正常恢复配置、白名单及停用状态，revision47、源码摘要保持。102的before_dispatch/零登记证据不能代替此场景。[正式104事实与实拍](../../testing/release-0.2.104/plugin-config-race/report.md)。整体Goal继续。
