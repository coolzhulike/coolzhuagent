# 整体验收状态快照（2026-10-08）

2026-10-10地址草稿增量：正式126原生实操复现网页同文档URL更新覆盖地址框输入；候选按视图scope保留草稿、显式导航/关闭/浏览器动作清除。真实键入后8974ms/18次网页更新仍保持，Enter目标GET一次及目标标题实拍通过，后退地址同步；Web offline build实际0、verify0，模型0/新云会话0。候选按身份停止、正式126恢复，原服务正常退出；新正式包复验、严格Browser时序及32WBS继续。上一e4b2a78两路CI success。[原失败和候选实拍](../../testing/2026-10-10-browser-address-draft/report.md)。Goal active。

2026-10-10文档切换增量：正式126原SWE-2/唯一island-kayak/revision57真实planning中自然导航，同名新按钮未误点，旧click以stale_observation/not_sent拦截；原终态预算覆盖缺陷保留。controller候选保留执行失败和receipt、无预算不再额外观察，独立真实复验正确回复，两个文档输入0，父failed为预期负例/end_turn/drained/解锁。core/Web offline build0、原core143/0。候选与正式实拍独立归档，已恢复正式126；新正式包交付、原生commit/down-up/关闭/HRESULT竞态及32WBS继续。[报告与原图](../../testing/2026-10-10-browser-document-change/report.md)。Goal active。

2026-10-10正式126：记忆JSON资料边界、大文本同句柄有界只读预览及文件级SBOM合并交付。冻结3ca0890e/快照642ff169，正常release构建353.2秒/actual0、安装0、六门pass、Program Files1159文件/432190904字节逐SHA一致，两路CI38051919724/38051916797 success。原SWE-2-medium/唯一island-kayak/revision57真实长程318.8秒（#806/#807）：实际污染L1选中/L4排除，两UTF-8/BOM UTF-16BE附件尾部新订单、八Browser动作/16可信输入、宿主2/2后DSH一次，总额3096正确；父completed/end_turn/drained并解锁。三次结果分页有8192→69632缺口，不冒完整连续读取；click/Enter未知效果保持。自有三条记忆撤回、原102保留，自有网页正常停止。1.8MB/19775行SBOM正式八页原字节重组与尾窗一致，正常文件入口及`:19775`末尾实拍通过。仅关闭这两候选的正式交付缺口；超长单行GUI、严格native竞态、完整4.4/6.4及32WBS继续，Goal active、Paint免测/微信不动/Opus暂停/无子代理。[交接及原图](../../testing/release-0.2.126/change-report-and-test-handoff.md)。 已公开GitHub预发布五资产，服务器摘要及实际tag3ca0890e一致；未签名、不修改自动更新索引。

2026-10-10正式125严格取消增量：真实SWE-2-medium/唯一island-kayak/revision57，#805/父060ea336，一次可信click已sent/released后关联观察实际登记；登记后22.466ms正常停止HTTP200，停止CAS后同请求673ms以native_observation_cancelled/reply_parent_changed收尾，未采纳迟到回包，effect/goal未知保持。父interrupted/30.9秒、CUblocked非成功、仅一工具一动作、ACPcancelled/drained/解锁、SSEdone一次，原生轨迹“已中止/模型请求已取消”实拍通过。仅输入释放后观察在途取消通过；晚UI关闭原失败、导航替换/原生窄竞态及32WBS仍开放，Goal active。[原始时序与实拍](../../testing/release-0.2.125/browser-pending-cancel/report.md)。

2026-10-10大文本预览候选：针对125拒绝完整1.8MiB SBOM，将读取提取为同句柄有界快照，64MiB文本可只读分页，单页/编辑256KiB、行窗801行保持。真实SQLite副本八页重组逐字节/SHA一致、19775总行及尾窗一致，BOM空行、超限413/旧版本409均通过；原生1443×897正常文件入口打开及`:19775`尾部定位实拍通过。offline build0、Web1433/0/6及lib8/nativehost1、联动8/0，OOM及丢失收据原失败保留。候选按身份停止，正式125及原配置/运行数量恢复保持；新模型/云会话0。仅源码候选，正式新包、超长单行GUI翻页、完整4.3/严格Browser时序及32工作包继续，Goal active。[完整原字节与实拍](../../testing/2026-10-09-large-text-preview/report.md)。

2026-10-09交付清单增量：文件级CycloneDX1.6 SBOM独立导出候选通过；PS5.1/7导出0、官方Schema及语义一致、Program Files1159文件/432195512字节逐SHA一致，篡改副本无输出和重复输出保字节的负例均退出1。完整库依赖/许可证/签名不计完成，正式125四资产/源快照不改。原生右栏拒绝1.8MiB真实JSON（1MiB上限），完整文件预览未通过，新增4.3大文本分页缺口继续。记忆边界1d3ba64两路CI success。模型调用0/新云会话0，Goal active，Paint免测/微信不动/Opus暂停/无子代理。[原始产物、负例与失败实拍](../../testing/2026-10-09-package-file-sbom/report.md)。

2026-10-09记忆资料边界候选：核心渲染器统一JSON资料投影与当前指令/授权边界，不改变召回、预算、工具或权限。原唯一SWE-2-medium/island-kayak、revision57真实长程708.1秒（可见#803/#804），实际完整任务前检选中两条L1污染资料、排除L4；八Browser动作/16可信输入、两滚动/文本效果、宿主2/2及freshness，随后DSH一次，UTF-8/BOM UTF-16BE冻结附件与中文总额4230一致，父completed/单end_turn/drained并解锁。五次分页读取有12288→70000缺口，不冒完整连续读取通过；零格式/引文拒绝也不补算纠正分支。offline构建及最终排版后构建0，核心356/0/1、Web1433/0/6（lib8/native-host1）、联动8/0。自有三条记忆按身份撤回、102条原记忆保留，自有服务正常0，当前Program Files正式125窗口已恢复；候选待批次出包。41f93ebe文档HEAD两路CI success，后续候选HEAD另验。仅关闭有限旧事实/错误角色资料场景，完整4.4、多模态/GC、严格Browser时序及其余32矩阵继续；Goal active、Paint免测/微信不动/Opus暂停/无子代理。[候选原生实拍与完整事实](../../testing/2026-10-09-memory-data-boundary/report.md)。

2026-10-09当前正式125：CU子阶段限制与冻结父任务交接、子框只读可见点提示、实际地址对应预览标签、同请求有界格式/引文反馈已正常交付。真实SWE-2-medium/原唯一island-kayak读取650行尾部随机码UTF-8/BOM UTF-16BE附件，双跨来源滚动/点击/填码/Enter八动作、16可信输入，两滚动/文本效果及宿主2/2/freshness通过，随后实际DSH计算器一次、中文总额1656，#797/#798（8分31秒）；两工具各一次completed，单end_turn/drained及解锁。同请求拒绝一次input_dispatched=false，本轮最终只有八步输入；拒绝正文未记录，不擅自归类。正式本轮分页读取0，候选两页证据不混算。构建/安装0、六门pass、1159文件逐SHA，冻结e8373cb两路CI success，四资产公开预发布及服务器摘要/实际tag一致。自然导航严格检查失败（资源未撤销，且模型误判目标），正常关闭驱动晚17.677秒未执行；严格nativeTarget/commit、在途撤销/HRESULT以及其它32工作包剩余矩阵仍开放，不重复简单任务碰窗口。[125正式交接与正负实拍](../../testing/release-0.2.125/change-report-and-test-handoff.md)。Goal active；Paint免测、微信不动、Opus暂停、无子代理。

2026-10-09最新长程状态（正式仍0.2.124）：同请求引文反馈候选真实SWE-2-medium/原唯一island-kayak通过综合长程。两实际长附件UTF-8/BOM UTF-16BE尾部随机码，双跨来源子文档滚动/点击/填写/Enter八步全部投递释放，16可信输入事件；两滚动与Text效果独立确认，Enter即时效果未知保留。宿主2/2及freshness通过；完整CU分段回执续读两页后实际DSH计算器一次，最终中文含两编码/码/算式/总额2666，可见#795/#796（12分4秒），父completed/单end_turn/drained并解锁。[候选原生实拍与事实](../../testing/2026-10-09-acp-grounding-feedback/report.md)。本轮零格式/引文拒绝，不冒模型纠正分支实测；原各轮失败保留。offline build0、Web1433/0/6，壳相同29dba及既有78/0。仅计候选，下一批0.2.125正常构建/安装/独立长程复验仍待完成；严格Browser时序及32矩阵仍开放，Goal active、Paint免测、微信不动、Opus暂停。

2026-10-09长程续接当前状态（正式包仍为0.2.124）：原任务终态交接旧候选真实通过，8动作/16可信输入后实际调用DSH计算器，中文最终回复含编码/随机码/总额4495；这是候选证据，未替代后续代码或正式安装验收。[交接实拍](../../testing/2026-10-09-acp-terminal-task-handoff/report.md)。长交接分页候选正常流程随后出现子框裁剪命中拒绝，整轮失败、零计算器、无结果分页读取，保留[原失败](../../testing/2026-10-09-acp-terminal-task-paging/report.md)。子框只读命中提示修补后两处输入正常，九步输入完成，但第二处Enter前规划action含未知字段触发invalid_plan，整轮失败；不是预算耗尽，也不是命中拒绝。[当前原失败与实拍](../../testing/2026-10-09-browser-visible-point/report.md)。已补同一请求执行前格式反馈，最多两次拒绝、原预算/取消不变、不修写回复/重放动作；新候选offline build0、Web1432/0/6、壳78/0，新的独立真实长程正在验收。严格Browser时序、完整分页续接、批次正式交付及32工作包剩余矩阵保持开放；Goal active，原唯一SWE-2/island-kayak、Paint免测、微信不动、Opus暂停。

2026-10-09正式124：历史用户正文摘要、共享文本/图片发送前SHA和中文错误提示三项批次正式交付。真实SWE-2-medium/原唯一island-kayak读取UTF-8与UTF-16BE长附件尾部随机码，经双跨来源子文档滚动/填写/Enter提交，8动作/16可信事件，附件冻结SHA/编码与原字节匹配，压缩用户摘要恢复。Browser部分通过，但模型遗漏明确要求的DSH计算器且最终混入西班牙语/未报编码，整体综合验收失败；口算3966不计插件调用通过。仅CU一次completed、父协议completed/单end_turn/drained及解锁。下一项修补CU规划限制仅限子阶段和返回父任务提示后真实复验，不写死插件调用。正常构建安装0、6门pass、1159文件逐SHA、冻结源码3f4f95a两路CI success，四资产公开预发布与服务器摘要/实际tag一致。另一历史Agent遗留绑定锁导致全库准备断言失败已记录，本轮会话无锁，无活动任务中断且未清其它锁。Browser严格nativeTarget/commit及在途撤销/HRESULT、完整记忆/多模态/GC及其它32工作包仍开放。[124正式交接、原失败与实拍](../../testing/release-0.2.124/change-report-and-test-handoff.md)。Goal保持active。

2026-10-09错误提示候选：统一requestJson移除用户界面的HTTP英文前缀、API路径与原始fetch错误；状态/body保留供程序处理。offline build0、语法0、既有前端22/0；真实库隔离副本的正常原生上传冲突实拍显示中文原因，无HTTP，模型0/云端0，原123界面已恢复。本候选和摘要/附件校验一起待批次正式交付，不提升为整体错误矩阵完成。[实拍及交接](../../testing/2026-10-09-ui-error/report.md)。Goal active。

2026-10-09附件完整性候选：正式123发送前未验证内容寻址身份，已在共享对象模块增加已读字节SHA校验，文本冻结及图片编码复用。两真实EXE/同真实库副本的文本与图片十二次HTTP检查确认损坏对象由旧403变候选400，原始/恢复仍403模型关闭；运行0/ACP0/云端0/原库写0。正常原生上传冲突实拍及正式123恢复实拍通过，区别于发送入口HTTP证据。最终build0、Web1431/0/6；首次同字节图片负例失效及回归1430/1/6保留。候选尚未打包，真实长程续接、多模态正向/GC/Browser严格项和其它WBS继续；Goal active。[完整证据与边界](../../testing/2026-10-09-attachment-integrity/report.md)。

2026-10-09继续验收：正式123两次连续观测驱动均未命中严格在途撤销。第一轮捕获登记后7.2ms但跨node调用导致exec context错误且未关闭；第二轮登记后68ms取得新AX，正常关闭晚14216ms，无stopped事件，sent/released保留而效果未知，failed/单end_turn/drained/唯一island解锁。原失败与实拍保留，不计严格通过。[完整时间及实拍](../../testing/release-0.2.123/browser-pending/report.md)。另真实长会话压缩439条时7个用户摘要被宿主边界占满，已提取历史投影模块并修补；两真实EXE/同真实SQLite副本恢复7正文、历史选取与硬预算保持，build0、Web1430/0/6、模块8/0、registry check0。候选界面正常打开并恢复正式123原生壳，未改原运行库、模型0/新云端0；摘要正文未通过GUI展示或真实模型连通复验，候选待下一批正式包，不冒记忆总体完成。[压缩候选与边界](../../testing/2026-10-09-history-summary/report.md)。其它32工作包与Browser严格项继续，Goal active。

2026-10-09正式123：本步效果归因及滚动文档身份已正常出包并独立复验。真实SWE-2-medium/唯一island-kayak新双来源订单8动作/16可信事件，两个子文档滚动与Text效果确认、Enter未知不冒领，真实提交及宿主2/2、后续DSH总额3514，回复#msg-1791569329337-0，两工具各一次completed、父completed/单end_turn/drained并解锁。独立行情负例一次CU两次无效click均sent/released/inconclusive、no_progress2停止、零第三次，目标未完成/root failed保留为预期负例。正常构建安装0、6门pass、1159安装文件逐SHA、源码407126d两路CI success、四资产公开预发布及服务器摘要/实际tag一致。严格nativeTarget/commit及在途撤销/HRESULT竞争、脚本自发滚动完整因果和其它32WBS继续开放。[123正式交接与实拍](../../testing/release-0.2.123/change-report-and-test-handoff.md)。Goal保持active。

2026-10-09滚动文档身份候选：原NodeCache按文档范围保存稳定随机身份，前后视口不再依赖AX序号。真实SWE-2/唯一island双子文档长程8动作、2次滚动effect_observed、两项随机码提交，宿主2/2后DSH总额5438；3步效果保持未知，根completed、单end_turn/drained并解锁。双端build0、Web1429/0/6、壳78/0、联动8/0。只计候选，下一批正式包与独立复验、严格原生窄时序及其它32WBS仍开放。[候选实拍与交接](../../testing/2026-10-09-browser-document-scroll/report.md)。


2026-10-09本步效果候选：宿主按原UTF-16选区读回文本改变，当前规划动作绑定，AX序号漂移不算焦点变化。真实SWE-2/唯一island综合订单11动作、三次文本effect_observed、3步inconclusive、宿主2/2及DSH总额9019；无效按钮/100ms行情反例两次sent/released/inconclusive后no_progress停止且零第三次输入，两轮end_turn/drained/解锁。offline双端build0、Web1429/0/6、壳77/0、联动8/0；第一候选业务完成但归因验收exit1保留。只计候选，正式新包与严格原生竞态及其余32WBS继续。[候选事实与原图](../../testing/2026-10-09-browser-action-progress/report.md)。Goal active。


2026-10-09正式122：多行边界引文误拒及停止保留部分验收已出包。真实SWE-2-medium/唯一island-kayak独立三阶段跨来源动态订单11动作/27可信输入，宿主2/2，随后DSH计算器总额10578，两工具各一次completed；回复#760/5分58秒，父completed/单end_turn/drained并解锁。正常构建/安装actual0、六门pass、1159文件逐SHA，冻结8edfa3a两路CI success，四资产公开预发布且服务器摘要/实际tag一致。诊断仅类型/字段数量实测覆盖两root入口。121原失败保留；中间行情进展误归因、严格原生竞态及其它32WBS继续，不冒整体完成。[122正式交接与实拍](../../testing/release-0.2.122/change-report-and-test-handoff.md)。Goal保持active。

2026-10-09多行回执候选：保留正式121的11动作/网页完成却引文误拒原失败；修补仅允许节点边界精确空格/LF，内部文字、数字、顺序及新鲜度保持，规划停止保留最后部分验收且仍失败。真实SWE-2-medium/唯一island-kayak新订单综合长程11动作/27可信事件、宿主2/2，CU与DSH计算器各一次completed，总额6470，回复#758/9分37秒，单end_turn/drained/解锁。offline build0、core143/0、Web1428/0/6、模块8/0；两实际root入口诊断仅类型/字段数量。只计源码候选，不替代新包正式独立复验；中间进展、严格原生竞态及其余32WBS继续。[候选报告与实拍](../../testing/2026-10-09-browser-multiline/report.md)。Goal继续。

2026-10-09正式121：五入口参数诊断投影已出包；正常构建/安装0、六门/1159文件与af272e7两路CI success。本轮跨来源动态订单11动作/27可信事件、三项网页完成，但三个相邻回执引文65字/原节点63字导致text_mismatch，宿主1/2、CU blocked及父failed，计算器未调用，单end_turn/drained/原唯一island解锁。root诊断仅类型/数量实测通过，不冒整体长程通过或其它入口覆盖；候选多行边界拼接与停止保留部分验收修补待实操。[121原失败及实拍](../../testing/release-0.2.121/change-report-and-test-handoff.md)。Goal继续。

2026-10-09后续诊断候选：五个工具参数诊断入口统一为根JSON类型/数量投影，不再复制键名、正文、路径或嵌套凭据，真实调用参数与事实链不变。offline build0、Web1427/0/6；首次回归和正式120真实CU输入所有权冲突exit101保留，任务结束后实际重跑0。120不包含此候选，下一有实际工具输入的长程需补正式诊断验证；历史日志、其它生产者/导出/全局配额及完整6.1仍开放。[检查及边界](../../testing/2026-10-09-tool-diagnostic-redaction/report.md)。

2026-10-09正式120增量：动态非目标行情每100ms刷新，真实SWE-2-medium/唯一island-kayak单perform完成三步订单11动作、27可信输入事件，三步验证/最终提交均通过，逐步sent/effect_observed及释放正常；回复#754，10分13秒，单end_turn/drained并解锁。局部成功证据新鲜度与ToolDispatchService协调提取已正式覆盖。构建/安装actual0、六门pass、1159文件逐SHA，冻结ed18c1c两路CI success，四资产公开预发布且服务器摘要/实际tag一致。原119失败保留；中间进展判据、严格原生竞态、完整2.3及32WBS继续，后续诊断脱敏候选不包含本包。[120正式交接与实拍](../../testing/release-0.2.120/change-report-and-test-handoff.md)。Goal保持active。

2026-10-09动态页面候选增量：正式119误将无关行情刷新判为旧观察，第一次点击后failed，原失败与实拍保留。修补将新鲜度限定于文档/资源/导航/焦点/加载及实际阳性证据，保留2秒内部/5秒外层上限；真实SWE-2-medium/唯一island-kayak一次perform完成11步三阶段订单、27条可信输入事件，全部sent/effect_observed且释放正常，回复#752/单end_turn/drained/解锁。该调用同时覆盖新ToolDispatchService原体提取入口。offline build0、Web1426/0/6、模块8/0；首次LLVM OOM exit101原日志保留。仅候选通过，正式新包独立复验待办；中间进展判据、严格输入竞态、完整2.3和32工作包继续开放。[原失败、候选事实与实拍](../../testing/2026-10-09-browser-live-repaint/report.md)。Goal保持active。

2026-10-09正式119增量：正常构建及安装actual0、六门pass、1159文件逐长度/SHA一致；源码da7e178两路CI success，119四资产已公开预发布且服务器摘要/实际标签核验一致。新页面真实SWE-2-medium/唯一island-kayak单perform完成三层跨域双反射/旋转/斜切长程，9步sent/effect_observed、22条可信事件、最终提交accepted，单end_turn/drained并解锁。正常API权限保存后顶栏无需刷新自动目录/完全访问切换实拍通过，已恢复原full-access/revision57。只关闭这两项正式子验收；读取收尾资源复核源码已包含119，严格nativeTarget/commit down-up及在途撤销/HRESULT竞争仍无完整证据。其余32工作包继续，Paint免测/微信不动/Opus暂停，Goal保持active。[119交接及实拍](../../testing/release-0.2.119/change-report-and-test-handoff.md)。

2026-10-09候选增量：三层跨域、双水平反射及旋转/斜切长程由真实SWE-2/唯一island-kayak一次perform完成10步，均sent/effect_observed、24条可信页面事件及最终完成原图，单attempt正常end_turn/drained/解锁。两次扩展通道失败原样保留，纠正测试提交漏native_browser_panel后通过。权限顶栏失效通知修补已在正常API切目录/恢复完全访问中不刷新自动显示；offline build0、Web1425/0/6、前端22/0、模块8/0。正式118不含这两项候选，待新包独立复验；严格时序和32WBS总体继续开放。[完整事实与实拍](../../testing/2026-10-09-browser-three-layer/report.md)。Goal保持active。

Browser读取收尾源码候选：资源/URL复核统一用于UI队列失败、callback及等待消费结束，失效优先资源变化，保留2秒/外层5秒，无重试/新锁/私有入口。独立壳offline build0、现有76/0；当前正式118尚不含，没有新实机证明，严格Browser仍开放。[候选与检查](../../testing/2026-10-09-browser-read-settlement/report.md)。Goal继续。

118正式增量：ACP插件不可续审批收尾已出包复验，真实SWE-2/原唯一island一次调用，接纳目录权限后扩大完全访问，仍按冻结权限拒绝，HTTP0、tool failed、terminal/end_turn/drained/解锁，无新云端；旧unknown保留、旧待审批在候选正常续发收束。构建/安装0、六门pass、1159文件逐SHA，Web1424/0/6、前端22/0，原参数revision57恢复。[118正式交接与实拍](../../testing/release-0.2.118/change-report-and-test-handoff.md)。外部API权限变化顶栏实时同步、非ACP审批实操及严格Browser仍开放；Goal继续。

ACP插件审批收尾候选：正式117真实冻结权限拒绝但遗留awaiting_approval/远端锁，已复现并修复；同一SWE-2/唯一island候选正常续发收束旧记录，新调用failed、terminal/end_turn/drained/解锁、HTTP0，无新云端。offline build0、Web1424/0/6、前端22/0，原失败及实拍保留。候选不冒称正式安装通过，新包待复验；严格Browser仍开放。[专项交接](../../testing/2026-10-09-acp-plugin-approval/report.md)。Goal继续。

117正式增量：旧配置页迟到请求作用域已出包复验。正常构建/安装0、六门pass、1159文件逐SHA一致，冻结d094c7a两路CI success；正式EXE的16组跨工程/ABA/重载/重启/错误释放/无头兼容真实HTTP与存储检查，正常原生B/A参数、保存0.45/revision12和同EXE重启四实拍通过。模型0、新云端0，原SWE/revision51/唯一island恢复。只关闭配置页此子项，完整领域及严格Browser仍开放。[117正式交接](../../testing/release-0.2.117/change-report-and-test-handoff.md)。Goal继续。

配置迟到请求候选：正式116已复现同ID同revision旧A草稿错写B；复用WorkspacePin与配置加载标识，统一页从完整工程路径绑定作用域。最终offline build0、既有Web1423/0/6与前端22/0，16组真实API（跨工程、ABA、重载、重启、400/409释放、无头兼容）及原生B/A参数、保存0.45、同EXE重启实拍通过。首候选GUI绑定失败保留，最终才计通过；模型0、原SWE/revision51/唯一island已恢复。正式116不含，待新包；完整领域迁移和严格Browser仍开放。[候选事实与实拍](../../testing/2026-10-09-session-config-late/report.md)。Goal继续。

116 Browser补验：纯截图驱动在登记后149ms取回新鲜画面，正常关闭仍晚9502ms；普通sent/released/effect_observed、父completed/单end_turn/drained/唯一绑定解锁，但stopped事件0，严格撤销仍未命中。不修改五秒期限或注入暂停，不继续同类简单点击碰窗口。[原事实与实拍](../../testing/release-0.2.116/browser-snapshot/report.md)。Goal继续。

116正式增量：模型配置操作和工程选择复用WorkspacePin，覆盖完整响应；修复正式115跨工程参数混读。正常release构建/安装0、六门pass、1159文件逐SHA一致，冻结aa72c25两路CI success；正式EXE忙期180切换409，真实交错77成功切换/75有效读取零混合，SQLite保存窗口409、另一工程不变、400/409后释放、正常原生A/B参数及同EXE重启三实拍通过。首观察器共享占用失败保留；模型0、原SWE/revision51/唯一island恢复。迟到请求/ABA及完整领域迁移和严格Browser仍开放。[116正式交接与实拍](../../testing/release-0.2.116/change-report-and-test-handoff.md)。Goal继续。

模型配置工程作用域候选：正式115真实复现2041读/180切换的跨工程混合；候选复用WorkspacePin覆盖读写和完整响应派生。实际build0、完整Web1423/0/6（另lib8/native-host1）；交错80次成功切换/77有效读取零混合，真实SQLite保存窗口切换409、另一工程不变、400/409退出释放pin，正常原生A/B参数和同EXE重启实拍通过。模型0、原SWE/revision51/唯一island恢复；115不含，迟到请求/ABA和完整领域迁移仍开放。[原失败、候选与实拍](../../testing/2026-10-09-session-config-scope/report.md)。Goal继续。

115补验：原生文件选择器经正常文件名框确认同时选入UTF-8/UTF-16BE两文件，名称/96B/186B与文件一致；取消保留选择、同已核身份正式壳重启恢复空草稿，模型0。只关闭GUI多文件选择/取消子项。[实拍与边界](../../testing/release-0.2.115/native-picker/report.md)。Browser连续观察新驱动仍未命中：登记后6.55ms捕获，正常关闭晚16721ms且父轮已完成1750ms；单SWE工具sent/released/effect_observed、end_turn/drained/解锁，但无observation_stopped，严格项未通过，不重复简单点击。[完整时间与实拍](../../testing/release-0.2.115/browser-continuous/report.md)。Goal继续。

115正式增量：四个模型参数入口同步块提取至SessionConfigService，沿用TrackedSessionStore诊断/原锁序/发布及失败恢复。实际正常构建及安装0、六门pass、1159项安装摘要一致，同源f6f8729两路CI success。正式EXE旧容量六路1337读/180保存、统一配置六路1150读/180保存均零混合，真实SQLite两组失败恢复与同revision200/409、正常原生设置与同EXE重启实拍通过；原SWE/revision51/唯一island已恢复，四隔离库模型轮次0。第一次构建外层等待未捕获子退出，直接等待重做才计0，原事实保留。115四资产已公开预发布并核实际标签/SHA；只关闭Web服务提取子项，完整领域/工作区作用域/outbox及严格Browser继续开放。[115交接与实拍](../../testing/release-0.2.115/change-report-and-test-handoff.md)。Goal继续。

会话配置服务候选：四个模型配置入口的同步块提取至SessionConfigService，沿用TrackedSessionStore/配置发布/原锁序，无第二缓存或队列。实际build0、完整Web1423/0/6既有忽略及lib8/native-host1通过；旧容量六路1314读/180保存、统一参数六路1145读/180保存均零混合，真实SQLite两组失败恢复和同revision200/409通过。正常原生设置及同EXE重启实拍通过；四个隔离库模型轮次0，原SWE/revision51/唯一island已恢复。两次准备失败原事实保留。114不含候选，完整Service/工作区作用域/outbox/严格Browser仍开放。[候选原事实与实拍](../../testing/2026-10-08-session-config-service/report.md)。Goal继续。

114正式增量：旧容量GET/POST复用单次捕获/已发布配置的本地服务容量，修复113旧GET32768/统一8192及清零POST1000000/65536/统一8192/4096的不一致。正式EXE六路1313次读取/180次保存零混合，本地约束/采样保留、正常原生预算与同EXE重启实拍通过。正常release构建/安装0、六门pass、1159文件逐SHA一致，冻结95c2704两路CI success；已公开114预发布、四资产服务端摘要/实际标签一致。原SWE-2-medium/revision51/唯一island-kayak已恢复且解锁，模型0/新云端0。完整Service/outbox、跨工作区/崩溃和Browser严格时序继续开放。[114交接与实拍](../../testing/release-0.2.114/change-report-and-test-handoff.md)。Goal继续。

113正式增量：配置读取名称/参数/revision/连接/局部容量与预算使用同一捕获快照，正式EXE六路1149次读取/180次保存零混合，正常原生预算及同EXE重启实拍通过。正常release构建/安装0、六门pass、1159文件逐SHA一致，冻结09fa80c两路CI均success；已公开113预发布，四资产服务端摘要/实际标签一致。原SWE-2-medium/revision51/唯一island-kayak已恢复且解锁，模型0/新云端0。只关闭本项同进程读取快照，完整Service/outbox、跨工作区/崩溃和严格Browser仍开放。[113交接与实拍](../../testing/release-0.2.113/change-report-and-test-handoff.md)。Goal继续。

旧容量接口候选：正式113未应用本地容量，旧GET32768而统一页8192，清零POST1000000/65536而统一页8192/4096，原失败已复现。候选旧GET/POST使用同一已捕获/已发布配置并复用本地规则，六路1309次读取/180保存零混合，正常旧API保存后原生统一页预算/采样实拍通过，offline build0/完整1423/0/6（lib8/native-host1）。模型0/原SWE不变；113不含，待新包复验，整体Goal继续。[原失败、候选与截图](../../testing/2026-10-08-legacy-model-limit/report.md)。

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

101正式文件子项：右栏编辑器的旧版本保存被明确拒绝，取消重载后本地修改保留、外部磁盘内容不被覆盖；独立同版本并发保存一方200/另一方409、最终字节与获胜revision一致。采证脚本两次错误独立保留，另轮完整通过回执和正常软件实拍见[文件冲突专项](../../testing/release-0.2.101/file-conflict/report.md)。仅关闭编辑器冲突与同版本并发保存子项；spill/GC/引用图及模型编辑工具矩阵仍开放，模型调用0、唯一远端不变。

101正式新增：真实 rust-analyzer 已启动但尚未发布期间，聊天室 A→B→A 的严格窗口已命中；返回 A 后旧请求仍在途、持有进程句柄仍存活，随后409拒绝、同一进程句柄确认退出，最终没有发布旧实例。两轮模型调用0、唯一远端绑定不变；首轮未命中独立保留。[正式ABA证据与实拍](../../testing/release-0.2.101/lsp-aba/report.md)。仅关闭房间ABA子项，跨工程启动期间切换/重载409和启动完成后的正常A→B→A服务回收、旧句柄409已另轮正式通过，见同报告。原执行计划5.4明确“不宣称全屏TUI全支持”，终端兼容边界按此范围核验，不新增全部TUI支持的交付承诺。

0.2.101当前正式增量：正常完整构建、六门pass、1159安装文件逐长度/SHA一致，安装返回0、正式原生后台与壳启动；冻结源码54594d9，两路远端CI均success。真实SWE-2-medium/唯一island-kayak完成本页设计审查及单次DSH工具，#683/#684无需刷新出现；统计侧栏自动882→883，明确派发875→876，539条旧投影不变，未知用量保持未知；本轮精确ACP轨迹实拍已通过。[101交接与实拍](../../testing/release-0.2.101/change-report-and-test-handoff.md)。已公开[101预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.101)，四资产服务器长度/SHA及实际标签与构建提交一致；断线大批变动、完整多工作区并发和Browser严格时序仍开放。

ACP统计遗漏已在正式101修补及独立复验：HTTP与权威ACP台账在读取层按精确ID和作用域去重，不增加第二账本；未发送/结果未知/派发未知独立显示，缺时间与token不补造。完整1416/0/6忽略、offline build和真实HTTP/journal逐项核对通过。[设计与审查取舍](acp-usage-read-model-design-2026-10-08.md)；候选881→882与正式882→883分开保留，不追认100含修补。

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
| P0 | Browser 严格时序与变换负例 | 透视和父覆盖层负例已在配套源码候选由真实SWE验证，分别以变换不支持及命中不符明确拒绝，零投递；094正式安装版的透视与父覆盖层负例也已通过，零投递且可信事件为空。严格按下期间同步文档替换及同源history地址变化已094正式通过：原动作released，新文档没有click/input。跨来源HTTP导航已正式实测通过：navigation-started与beforeunload发生在pointerup前，新文档加载发生在释放后，新文档无click/input/key；不能宣称覆盖commit恰在按下期间。095正式按下期间正常关闭及右栏切换为设置的窄时序均已通过：原动作释放，后续观察停止且不补发。新原生Target替换与跨来源commit窄时序仍开放，不混算。确切在途观察撤销、失败HRESULT与资源变化同时发生仍缺正式证据；111正常关闭晚于登记5792ms，未命中底层等待分支，已保留原失败，不计通过。规划期间变化已有通过证据，不能替代按下期间验收。 |
| P0 | 连续工具综合任务 | 0.2.94正式安装版综合任务已通过：选中SKILL、单次真实DSH计算器、5步Browser实际业务反馈继续规划，可信拒绝→配送确认→结算完成，CU成功、远端正常收尾并解锁。完整瞬时插件响应未单独持久化，已保留completed/ok台账与实际页面815交叉证据。更广的文件产物/跨重启恢复仍按后续工作包，不外推本次通过。沿用原 SWE-2-medium 与唯一 Devin 绑定，不重复简单问答，不创建多个云会话。 |
| P1 | 插件超时与许可竞争 | 0.2.94正式函数内层deadline和外层等待预算已补真实SWE/真实网络证据：约1.506秒及29.948秒断开、无响应体或补发，远端正常收尾。内层业务ok:false但宿主正常返回completed，不能误称成功抓取；外层宿主audit=timeout/台账failed。095真实函数GET进入后约24ms正常停用插件，约1.183秒断开等待连接且无响应体、无补发，单远端正常收尾，见[撤销验收](../../testing/2026-10-08-plugin-revocation/report.md)。此仅覆盖启用资格撤销，prepared及submitted阶段停用/重新启用后旧轮拒绝已在095正式补验通过，见[重新启用隔离](../../testing/2026-10-08-plugin-reactivation/report.md)；冻结授权扩张等窄竞争仍缺。099已补函数进入后停用的独立Win32宿主退出证明；所有清理阶段及后代树矩阵仍未完整。函数进入后取消已经正式通过，不重复列为未通过。 |
| P1 | 会话与附件边界 | 文本快照和历史投影、UTF-16预览修复已在097正式独立通过：UTF-8 CSV+UTF-16BE备注→实际calculator787→7步Browser反馈复核，四项HTTP400负例模型零派发，正式聊天室消息和中文预览实拍。见[097报告](../../testing/release-0.2.97/change-report-and-test-handoff.md)；候选失败与修复记录独立保留。115原生GUI多文件选择/取消已通过，见本页最新补验；API上传未被用作替代。账号过期、换模/rebind、PDF/音视频、Goal/Relay附件、纯文本转默认视觉Agnes、图片预算/不支持/群发等边界未完整。Devin Goal/Relay/子Agent入口仍明确拒绝，不能仅安排测试后宣称支持。 |
| P1 | 免费模型连接器 | OpenCode/HF 的插件入口及真实模型目录已通过正式页面验证；实际生成请求为0。填写各平台独立凭据后，仍需真实生成、多模态、SKILL/插件和 Browser 长程验收。不得复用其它平台 Key 或把目录声明当实际能力。 |
| P1 | 新图标正式交付 | 0.2.94已正式打包安装，49 SVG与3 PNG实际HTTP资源与安装文件全部一致；0.2.96已正常安装并公开预发布，上传居中修正包含。095的1443×563、096的903×897与903×551基本布局及可滚动配置右栏实拍通过。主窗口、长程开始/停止状态与工具泛光已有证据；全部动态跟随/暂停、审批状态及高DPI矩阵仍需补验。 |
| P2 | 启动特殊模式 | 资源加载失败、减弱动作模式及首次/重启边界未全验。完整舞剑本体、跳过与已运行重复启动不再重复作为未完成项。 |
| P2 | 搜索、统计与调度 | 097正式后端独立容量库的10k/100k搜索、分页/排序/定位、11万条FTS冷重建及并发历史读取已通过，正式页面搜索和第50000条定位实拍已归档，[容量报告](../../testing/2026-10-08-search-volume/report.md)。097正式原生UI的SWE→主聊天室→SWE统计切换、真实539/538请求归属、异房间消息ID不返回索引/耗时及未知token显示已通过，[统计报告](../../testing/2026-10-08-usage-scope/report.md)。跨客户端历史自动同步已在100正式修补、外部新增/回复自动出现通过；101本页流式结束及统计自动增量/精确ACP轨迹通过。110正式后台断线期间副本新增450/删3/改1，原页面跨三页自动恢复527条，内容/顺序一致且重复0已通过；流式/跨工作区等其余重连矩阵仍缺；多工作区/并发写入全矩阵以及running未知结果和失败回退综合验收仍缺；102真实模型定时固定房间投递与重启并发零重放已正式通过。098停机到期及099未回写投影重启恢复已分别通过，不重复列为全未通过。已有 Goal 有限重试/回退实现不等于完整持久调度闭环。 |
| P2 | 文件、记忆和终端生命周期 | 098正式终端Unicode、1MiB/17页、同句柄恢复、resize、异房间/旧句柄409、退出重建与关闭已通过；受控宿主异常退出后持有Win32后代句柄确认signaled，重启后absent/旧句柄409也已通过，[098报告](../../testing/release-0.2.98/change-report-and-test-handoff.md)。098正式LSP异常退出/显式重启/旧句柄拒绝及诊断定位现也已补验，[专项实拍](../../testing/release-0.2.98/lsp-lifecycle/report.md)；101正式房间ABA、启动pin拒绝工程切换/重载及完成后跨工程回收/旧句柄隔离均通过，[专项](../../testing/release-0.2.101/lsp-aba/report.md)；全屏TUI全支持不在原5.4交付范围。跨工作区其余完整矩阵、文件编辑器冲突及同revision并发保存已正式101通过，[专项](../../testing/release-0.2.101/file-conflict/report.md)；模型编辑工具、spill/GC/引用保留及压缩图片证据/记忆污染仍缺。 |
| P2 | 架构整合与运维矩阵 | 共享异步 TurnRunner、跨进程单写者/outbox/会话权威 epoch 尚未完整；SessionConfig参数规则已正式111拆模块，112同进程保存顺序和SQLite失败回退正式通过；完整Service/ToolDispatch职责及重复hooks仍需收敛。109未checkpoint WAL备份/独立恢复/升级后新写入及索引冲突回滚、110诊断文件轮转和OS占锁非阻塞启动已通过；凭据迁移全矩阵、升级断电/磁盘满、全局配额/其它生产者/脱敏故障、两实例/旧helper崩溃、多屏混合DPI/负坐标、权限多入口与句柄继承/隔离证据未全。SBOM/签名策略和feature映射仍待核实。 |

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


## 2026-10-08 正式104插件重新配置竞争补记（部分通过）

配置提交明确早于旧工具登记，真实SWE-2/唯一island-kayak网络GET为0，工具1条failed、父轮completed/end_turn/drained。104仅传回409，独立拒绝诊断缺失；候选补具体原因及after_admission_before_executor事件，offline build与DSH专项8通过/1既有跳过，尚未出包复验。正常恢复配置、白名单及停用状态，revision47、源码摘要保持。102的before_dispatch/零登记证据不能代替此场景。[正式104事实与实拍](../../testing/release-0.2.104/plugin-config-race/report.md)。整体Goal继续。
