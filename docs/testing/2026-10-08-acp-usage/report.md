# ACP统计遗漏修补：源码候选验收

0.2.100之后候选，尚未正式出包。发现旧统计539条与ACP台账不一致，按[读取整合设计](../../analysis/2026-09-21-integration-review/acp-usage-read-model-design-2026-10-08.md)合并并去重；没有补写台账、改权限或新建云端会话。

## 实现和检查

- 统一读取HTTP事件与ACP权威attempt；重复旧投影只贡献同作用域真实token和已有时间，普通HTTP重试沿原逻辑请求聚合。scope冲突不污染其它房间，ACP轨迹只按精确run关联，不借turn猜配。
- 明确派发与未知派发分开；not_sent、unknown、进行中及失败/中断分开。未知token为NULL，统计已知合计只取已知mask。台账无时间的记录仍NULL，不虚构顺序时间。
- 删除未调用record_acp_attempt，不修改ACP发送/恢复状态机。首次编译E0597已修为先收集再返回，原[失败日志](initial-compile-failure.log)保留。
- 三项有/无表、旧库、重试、精确去重、异scope、状态、缺时间/未知token及损坏台账专项通过；完整控制台1416通过/0失败/6既有忽略，另库和子目标8/1通过；offline build通过。[完整日志](full-tests.log)、[专项](targeted-tests.log)、[编译](build.log)。JS语法和git diff --check通过。

## 真实库、真实模型与原生UI

1. 正常停止已核验身份的空闲正式100后台，保持正式100原生壳，启动配套源码候选，使用原SQLite、原SWE-2-medium和唯一island-kayak。固定DSH运行资源从正式100复制；不修改安装目录或白名单。
2. 开始时当前房间总882次请求，其中SWE881次、Opus历史1次（本轮没有调用Opus）；SWE已知派发874、未发送2、结果/派发未知各5、取消9、进行中0，与只读journal逐项一致。[HTTP和只读对照](http-result.json)、[统计实拍](statistics-before.jpg)。旧usage表539条仍保持，不将两表直接相加。
3. 正常原生UI在当前输入框粘贴实际实现并点击发送：任务`ACP-USAGE-CANDIDATE-CODE-REVIEW-20261008`，run `run-chat-3f354c4a156b0bd19dd802ab60d637ac2b6b10b6715d362d`。单次真实DSH计算器completed、原始审计ok/allow-auto，ACP completed/end_turn/drained、唯一绑定解锁。模型所述881+1=882与独立前后计数一致；瞬时响应体未独立持久化，不把模型所述executed=true当原始字段。[实际运行事实](integration-result.json)。
4. 本页流式运行和收尾期间没有CtrlR，完整回复#682、索引/耗时及空闲输入框自动出现；[流式运行实拍](local-stream-running.jpg)、[完成实拍](local-stream-completed.jpg)。SWE请求881→882、已知网络尝试874→875、房间总883；原usage539未变，新增run恰有1条精确ACP请求、tokens和缺失时间仍NULL。[增量及轨迹HTTP](http-after-result.json)、[统计实拍](statistics-after.jpg)、[展开当前轮次实拍](trace-current-request.jpg)。

## 审查取舍与边界

SWE实际源码审查确认去重、跨scope、unknown、时间与旧HTTP关联；其指出partial_usage_attempts包括mask=0。主会话保留兼容字段名：现有页面明确显示“用量未完整提供”，意图就是包含完全未知，不把该数量解释为“部分已知”。台账attempt_id为全局主键，NOT EXISTS按该精确键排除错误旧投影符合权威归属；不按call/run猜配。

本页真实流式和单次新请求增量已在候选关闭。断线期间大批新增/删除、完整多工作区/并发矩阵仍开放；100不含统计修补，正式新包及独立实操需继续。不外推Browser严格时序、免费模型凭据连通或其它WBS完成。

候选验证后已正常恢复正式100后台，实际模型attempt全库935、旧usage539，唯一绑定解锁；没有清除输入安全历史或改授权。后续新包正常完整构建。
