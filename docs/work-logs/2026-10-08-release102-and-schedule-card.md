# 正式102与长任务卡片修补

正式102冻结95e0140，正常安装与1159文件逐摘要核验通过。真实SWE单次定时投递原绑定聊天室，两次工具completed，材料函数名不再触发误CU失败；正常重启五次并发扫描零重放。插件源码变化复验取得独立tool.dispatch_rejected/tool_not_live/executed=false，零执行登记/零GET、排空后精确恢复。原唯一island-kayak保持。完整交接与实拍见[102报告](../testing/release-0.2.102/change-report-and-test-handoff.md)。包装子ExitCode=null如实保留，不改写为0。

正式实拍发现定时任务卡片将长源码全文平铺，状态和删除操作被挤出。前端候选先呈现会话显示名、结果房间、计划和执行状态；正文按Unicode字符截240并限制三行，长文本用原生details按需展开，全文独立滚动；删除沿用原行为，改现有图标并保留title/aria。去掉不可用会话的裸内部ID。不修改模型、权限、调度与云端绑定。

node --check通过；cargo build -p coolzhu-web-console --offline通过（21.70秒，原warning保留）。低影响布局不新增镜像单元测试，不重复已通过1420项。此候选尚未包含102，下一正式包正常UI实拍后才验收。Browser严格时序及其它台账保持开放，Goal继续。


## 正式104：真实大回执分页通过

104正式补记：冻结6fad837，正常发布链真实子退出0、六门pass、MSI安装0、1159文件逐长度/SHA匹配。真实SWE-2-medium/唯一island-kayak，一次HTTP GET完整44498字节资料，完整工具回执111227字节；独立tool.result_page_read记录107200→111227，模型读到随机尾文、calculator一次完成正确149548450，两个工具completed、父completed/end_turn/drained、无read_file/无新云端会话，约60.3秒。正常原生实拍与原事实见[104交接](../testing/release-0.2.104/change-report-and-test-handoff.md)。临时白名单/插件已恢复revision45，服务器正常停止。仅关闭本轮ACP无read_file大结果尾文→工具续接；其它Provider、GC/压缩及完整资格矩阵不外推。102原失败、模型字节数误称及首次采证列名错误保留并纠正。Browser严格时序与其它总体矩阵继续开放，Goal保持进行中。下方“分页候选未实机”属于历史阶段，不覆盖本段正式事实。


## 2026-10-08 正式104插件重新配置竞争补记（部分通过）

配置提交明确早于旧工具登记，真实SWE-2/唯一island-kayak网络GET为0，工具1条failed、父轮completed/end_turn/drained。104仅传回409，独立拒绝诊断缺失；候选补具体原因及after_admission_before_executor事件，offline build与DSH专项8通过/1既有跳过，尚未出包复验。正常恢复配置、白名单及停用状态，revision47、源码摘要保持。102的before_dispatch/零登记证据不能代替此场景。[正式104事实与实拍](../testing/release-0.2.104/plugin-config-race/report.md)。整体Goal继续。


105正式补记：插件重新配置旧轮拒绝的具体原因传回与独立阶段事件已正式复验通过。真实SWE-2/唯一island-kayak，配置先提交，工具1条failed、GET0，tool.dispatch_rejected准确标after_admission_before_executor/dsh_action_not_live/executed=false；父completed/end_turn/drained，正常恢复revision49。正常release六门、构建/安装0、1159文件逐SHA一致，冻结aaf39cd。见[105交接与实拍](../testing/release-0.2.105/change-report-and-test-handoff.md)。104缺诊断是历史阶段；其它矩阵仍开放，Goal继续。



105执行中配置更换补记：真实网络GET1后正常更换配置，原连接在约1.845秒内关闭，无响应体/补发；独立持有旧宿主进程句柄Wait258→0，脚本未终止宿主。工具failed、父completed/end_turn/drained、唯一远端解锁，正常恢复revision51。执行已进入，不写成零执行；完整瞬时工具回执未另存，清理以独立句柄证据为准。[105实拍与交接](../testing/release-0.2.105/change-report-and-test-handoff.md)。105已公开，四资产及tag摘要一致，构建源码两路CI success。其它总体矩阵继续开放。


106正式增量：c9c61fe冻结，完整1423Web/143核心、六门、正常MSI安装0及1159文件逐SHA通过。真实SWE原island-kayak两轮保留成功与失败的sent/released；第一轮正常导航晚4818ms，负例未命中保留；第二轮观察timeout且效果未知/无补发，实拍归档。两次构建包装与旧MSI路径核验失败原样记录，采用报告确切带时间戳产物；无伪造报告。两路源码CI已success。后续资源等待原因识别候选已offline build及完整1423/0/6通过，尚待107实机。见[106交接](../testing/release-0.2.106/change-report-and-test-handoff.md)。


## 0.2.107正式安装与浏览器观察边界

107正式补记：正常release构建/安装0、六门pass、1159文件逐摘要一致，冻结314671b，两路远端CI success。四轮真实SWE/唯一island-kayak均单调用/end_turn/drained且无补发；三轮预设窗口未命中，有限网页跳转触发native_browser_observation_stale并完整保留sent/released、效果未知，新页零输入。精确native_browser_resource_changed等待分支和严格按下窗口仍开放，不冒称本版实机闭环。见[107原事实、截图与交接](../../testing/release-0.2.107/change-report-and-test-handoff.md)。其它矩阵和整体Goal继续。


## 0.2.108正式观察阶段复验

108正式增量：观察失败阶段最小轨迹已实施，离线build0/完整1423通过/0失败/6既有忽略，正常构建六门pass、安装0、1159文件逐摘要一致。原SWE/唯一island-kayak一次真实工具sent/released、新页零输入、单end_turn/drained并解锁；释放后230.1ms正常导航得到合法新观察，最终budget_exhausted，未命中底层等待分支。CU observing不能证明底层请求已登记，后续增加请求关联标记作精准复验；原事实与[108实拍交接](../../testing/release-0.2.108/change-report-and-test-handoff.md)保留。总体Goal继续，严格时序及其它矩阵仍开放。
