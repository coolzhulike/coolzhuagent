# 历史摘要与附件完整性批次交付

2026-10-09最新长程状态（正式仍0.2.124）：验收结构反馈候选八步真实输入投递释放、两个随机订单实提交，已消除结构错误；标准1的两节点引文被模型用分号改写，text_mismatch，仅1/2确认，随后done导致verification_failed；零计算器、零结果分页读取，父failed/end_turn/drained且原唯一island-kayak解锁。[原失败与实拍](../testing/2026-10-09-acp-verdict-feedback/report.md)。已增加同一冻结请求内的引文反馈，与结构反馈共用最多两次拒绝，原预算/取消不变；不替模型修文、不回放输入、不放宽引文规则，最终事实与freshness仍独立核对。新候选offline build0，完整Web回归及独立真实长程正在执行。旧候选成功不追认新版本；批次正式交付、分页正向、严格Browser时序及32矩阵仍开放，Goal active、Paint免测、微信不动、Opus暂停。

2026-10-09长程续接当前状态（正式包仍为0.2.124）：原任务终态交接旧候选真实通过，8动作/16可信输入后实际调用DSH计算器，中文最终回复含编码/随机码/总额4495；这是候选证据，未替代后续代码或正式安装验收。[交接实拍](../testing/2026-10-09-acp-terminal-task-handoff/report.md)。长交接分页候选正常流程随后出现子框裁剪命中拒绝，整轮失败、零计算器、无结果分页读取，保留[原失败](../testing/2026-10-09-acp-terminal-task-paging/report.md)。子框只读命中提示修补后两处输入正常，九步输入完成，但第二处Enter前规划action含未知字段触发invalid_plan，整轮失败；不是预算耗尽，也不是命中拒绝。[当前原失败与实拍](../testing/2026-10-09-browser-visible-point/report.md)。已补同一请求执行前格式反馈，最多两次拒绝、原预算/取消不变、不修写回复/重放动作；新候选offline build0、Web1432/0/6、壳78/0，新的独立真实长程正在验收。严格Browser时序、完整分页续接、批次正式交付及32工作包剩余矩阵保持开放；Goal active，原唯一SWE-2/island-kayak、Paint免测、微信不动、Opus暂停。

2026-10-09正式124：历史用户正文摘要、共享文本/图片发送前SHA和中文错误提示三项批次正式交付。真实SWE-2-medium/原唯一island-kayak读取UTF-8与UTF-16BE长附件尾部随机码，经双跨来源子文档滚动/填写/Enter提交，8动作/16可信事件，附件冻结SHA/编码与原字节匹配，压缩用户摘要恢复。Browser部分通过，但模型遗漏明确要求的DSH计算器且最终混入西班牙语/未报编码，整体综合验收失败；口算3966不计插件调用通过。仅CU一次completed、父协议completed/单end_turn/drained及解锁。下一项修补CU规划限制仅限子阶段和返回父任务提示后真实复验，不写死插件调用。正常构建安装0、6门pass、1159文件逐SHA、冻结源码3f4f95a两路CI success，四资产公开预发布与服务器摘要/实际tag一致。另一历史Agent遗留绑定锁导致全库准备断言失败已记录，本轮会话无锁，无活动任务中断且未清其它锁。Browser严格nativeTarget/commit及在途撤销/HRESULT、完整记忆/多模态/GC及其它32工作包仍开放。[124正式交接、原失败与实拍](../../testing/release-0.2.124/change-report-and-test-handoff.md)。Goal保持active。

Paint免测、微信不动、Opus暂停；未使用子代理。
