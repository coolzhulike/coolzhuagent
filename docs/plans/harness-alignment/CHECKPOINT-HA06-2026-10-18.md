# HA06恢复与效果：实时接替检查点

**已收口：HA00～HA06 accepted，33/60。** `web-regression-03`实际92/92；`build-01`于2026-10-18 10:52:25 UTC退出0，构建输入未变。15组HA06回执、完整失败/成功日志、实现差异及逐条验收在`evidence/HA-06/20261018/`，当时台账保存在`accepted-state.json`。下面的在途描述均为过程历史，禁止据此重启已结束命令。后续工作当前状态以`state.json`为准。

从`state.json`读取当前状态。本页记录HA06初次接入，前包HA00～05曾于本轮实际验收为28/60，完整当时台账在`evidence/HA-05/20261018/accepted-state.json`。共享源码再次变化后，相关旧条目暂为needs_recheck，不能靠刷新旧证据哈希恢复通过。

## 已应用，尚未验收

- main.rs：本次未开放工具、CU失败终态、反馈上限改为宿主事实回执，不调用额外文本恢复；正文协议错误只一次无工具修复且保留修复成功/失败状态。
- tool_recovery.rs：只按本次已发送schema明确声明的必要字段/基础类型发现派发前参数错误。整批未派发时最多一次参数修复，沿原历史/父请求/根预算；不验证工具正文中的授权声明。
- ToolDispatchService：真实pre/post-hook错误、取消/期限和终态存储失败向同一次调用返回宿主停止标记，防止共享循环继续自动追加尝试；取消持久状态保持outcome_unknown。
- ha06_tests.rs：本地实际HTTP/真实工具/SQLite触发器及真实hook进程；包含终态/修复、重复身份换参数换工具、消费端在效果后断连、模型流未完成、post-hook及存储故障。尚无通过结论。

改前备份在`tmp/ha06-20261018-before/`。`tmp/ha06_apply.py`已经应用，**不要重复运行**；临时草稿不是当前源码，今后的修复必须编辑`modules/gui-web/packages/web-console/src/`实际文件。`ha06_apply.py`第一次仅因中文冒号锚点不符停止，未写业务源码；修正锚点后的应用成功。

## 在途命令及下一步

首轮`web-regression-01`编译退出101，未运行测试：新模块误引用crate::api、旧host_child_agent测试未解构新的修复结果tuple、测试从Json移出字段。均已修复，原日志保留。第二轮`web-regression-02`工具session **41398**、launcher PID **17376**；日志/真实命令/开始结束/退出码/输入哈希在`tmp/ha06-20261018-runs/web-regression-02/`。`tmp/ha06_regression.py`将原76项回归与HA06、settlement、hook预算测试合并，两个仅作为子进程入口的probe不计入父测试执行数。实际匹配数仍待确认。

1. 核对receipt和实际进程后接读，禁止盲目重启、全局终止或在编译期间改源码。
2. 已补一次修复边界：参数修复之后再返回正文协议错误，不再调用文本修复；修正参数后第一次实际执行失败，直接停止。repair_dispatch_pending只覆盖正在等待的修复结果，成功后清除，不把后续正常任务一概当修复失败。新场景ParamProtocol、ParamExecutionFailure、ParamTimeout仍待第二轮验证。host_child_agent额外变化只有tuple测试fixture，原文件已备份。
3. 原HA04的Add场景现在应只有原请求，工具未开放终态不再发无工具恢复；这属于HA06消除冗余的预期改变。新的协议修复场景必须证明新请求ID/父链，不能丢掉HA04身份验收覆盖。历史HA04报告保持原样，新增报告解释差异。
4. 验证实际断流、参数修复目的计数、单次人类接纳、真实效果/进程回收和未知状态，必要时按失败修复，保留所有失败回执。
5. 实际测试通过后完成Web生产offline build（可延续原三crate构建缓存，jobs=1、dev debug=0）；保存每次真实日志、事件、输入哈希及差异，人工审查HA06五条后才登记。HA01～05相关回归通过后新增证据恢复当前引用，不改历史对象。

第二轮实际92/92通过，输入未变。随后修复了JoinSet异常只记录日志并flatten掉缺失回执的问题：保留原调用关联并返回worker_outcome_unknown，停止追加请求。JoinLostReceipt用真实工具先产生一次效果、随后cfg(test)缝隙令承载任务panic；预期原持久completed回执不被改写，而消费端不得虚称已完成或重新采样。还补了每场景人类接纳计数=1，以及JSON Schema整数允许1.0的边界。

**当前第三轮：web-regression-03，session 24591、launcher PID 14036**。实际开始时间/日志见`tmp/ha06-20261018-runs/web-regression-03/receipt.json`，尚无最终结果。第二轮不能用于关闭第三轮输入的新改动。后续归档/验收需将JoinLostReceipt纳入第15组事实；其预期panic属于故障注入，仍必须确认整个测试最终0失败。

## 工具与边界

最新进度：第三轮`web-regression-03`实际92/92、0失败，输入未变。生产构建`build-01`在途，session **66895**、cargo PID **12052**，日志在`tmp/ha06-20261018-runs/build-01/`。不要把前一轮日志当作本次构建完成。验收脚本已补齐第15组JoinLostReceipt及每组单次人类接纳；仍需原始回执人工核对及实际build退出0后才能执行。

后续依赖选择：PLAN第4节明确允许HA09独立提前。完成HA06之后，先推进HA09的类型化验证状态及结果消费；HA07/08仍pending，不宣称审批竞态或并发资源调度已通过。`tmp/ha09_goal_verification.rs`只是尚未接入/编译的草稿，不属于业务源码，也不算实现或证据。

`tmp/ha06_run.py <新标签> <命令...>`保存真实回执；`tmp/ha06_running.py`更新在途；`tmp/ha06_regression.py <新标签>`启动合并回归。`tmp/ha06_archive.py`和`tmp/ha06_accept.py <通过的运行标签>`已准备但尚未执行，后者需要按新增JoinSet故障补齐审查映射；不要误跑HA04/HA05归档覆盖历史，也不要绕过回执断言。

HA07、08、09、10未因此开始验收。HA11缺固定provider/model/effort及费用授权，未调用真实模型。使用隔离临时workspace/SQLite/随机本地端口，未触碰日常8765、未安装或发布。最近账户额度快照为89%已用，属账户窗口历史读数，接替需重新核对。当前没有未知的用户业务效果；故障注入的未知状态只属于隔离夹具。
