# 0.2.80 会话库并发打开修补与测试交接

## 问题与实现

PR84的683源码push检查成功，但同份源码PR检查在首次并发打开真实SQLite会话库时出现DatabaseBusy，1389通过、1失败、6忽略。原用例每轮6线程同时首次打开、共8轮，不是模型回复夹具。原远端失败、旧实现本地成功及修补后结果均保存，不能用重跑掩盖失败。

现有WAL模式切换已有显式BUSY处理，但前置只读PRAGMA user_version可能直接BUSY。[SQLite官方busy handler说明](https://www.sqlite.org/c3ref/busy_handler.html)表明注册等待器并不保证每次锁冲突都调用；[WAL说明](https://www.sqlite.org/wal.html)也保留BUSY情形。computer_use_store只对这一条无副作用读取重试，沿用5秒总期限，恢复原5秒busy timeout；事务内直接读取，不重放业务事务或迁移。错误保留SQLite主码、扩展码并补失败阶段。版本超前拒绝、备份和迁移行为保持。

原日志没有确切失败阶段，旧实现本地同一用例成功，因此本轮是按官方锁语义补齐真实缺口，不能宣称精确重现原CI那一次根因。后续阶段诊断用于实际定位，未消除的失败须继续处理，不能放宽期限、删除用例或静默吞错。

## 已完成验证与待交付

离线cargo build通过；既有Web完整回归1390通过、0失败、6忽略，83.59秒。既有并发首次打开用例另有限重复10次，每次8轮×6线程，均通过。只读原始日志及字节SHA见[CI证据](ci-lock-fix/manifest.json)。这些代码检查不替代用户要求的真实SWE软件截图验收。

0.2.79的正式Shadow子点击、父误击拒绝和验证期间关闭撤销证明见[079报告](../release-0.2.79/change-report-and-test-handoff.md)，其MSI不包含本次会话库修补。0.2.80须正常发布链构建、安装文件逐一核验，沿用原SWE-2-medium / veiled-anise正式入口实操；安装摘要、原图、每轮耗时与终态事实随后补充。当前尚未计为080正式安装通过或新HEAD远端检查通过。

四项任务总体未完成；iframe内部自动化、严格新文档down/up、输入前/按住关闭替换面板及其它项目仍见[当前队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)。Paint仅基础输入已正式通过，微信不改不测，Opus暂停；不用子Agent、Qwen或模型回复夹具。
