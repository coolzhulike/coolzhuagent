# HA06逐条验收

本次实际回归92项通过、0失败，生产offline build退出0，执行输入未变。命令、实际UTC、退出码和输入哈希在runs对应receipt；审查提取的15组事实在audit.json。

| 条目 | 实际结果与证据 |
|---|---|
| HA06-01 | MissingTool只有1次请求、0工具/效果；Cap只有上限内一次工具和2次请求，无额外文本恢复。PostDenied/SettlementFailed各只有1次请求。宿主给出事实阻塞及原因，CU终态分支同步消除额外恢复。 |
| HA06-02 | ParamFixed：3请求/1实际工具，只有1次parameter_repair；ParamInvalid、ParamProtocol、ParamExecutionFailure均在2请求结束，没有第二次修复。ProtocolFixed/Invalid仅一次text_recovery，无工具；失败保留未完成。ParamTimeout在原8秒预算内停止，无工具/追加请求。 |
| HA06-03 | stream-cut在工具参数/完成标记未齐时结束HTTP流，工具登记和效果0；同次HA04重试场景证明效果前429/503安全重试保持原logical ID且最终只执行一次。identity故意丢弃实际回执仍不能重放；PostDenied实际效果1后失败且不再采样；drop-after-effect在后置进程屏障丢弃消费者，效果1、持久unknown、原身份重放拒绝。JoinLostReceipt在真实效果及持久收尾后令子任务异常退出，消费者报告worker_outcome_unknown、无追加请求；已知completed记录保留，不伪造成功响应。 |
| HA06-04 | identity实际首次bash追加1次；同来源/同provider编号重送、改参数、换write_file均拒绝且原状态不变；另一来源重用provider编号正常追加第二次。原HA04不同真实请求/父链回归仍通过。 |
| HA06-05 | 真实pre/post-hook取消/到期测试复跑，目标效果前0/后1、下一hook不启动、进程回收；drop-after-effect保留cancelled_outcome_unknown且真实后置进程已退出，无迟到写入；SettlementFailed用SQLite触发器拒绝completed更新，效果1、终态unknown、不假成功/不再请求。 |

HA04的Add场景现在直接报告未开放工具，不再额外无工具采样，这是HA06的预期变化；HA04关于新采样ID/父链的当前证据由Param/Protocol修复的实际请求与父链断言接续。原HA04、HA05报告不改写。参数修复仍只有一个人类接纳路径，沿原历史、父ID和live gate，不新增根预算；本次schema检查只覆盖显式必要字段/基础类型，不声称通用语义验证。

首轮编译失败保留；其他失败和最终修正原因按各run日志及检查点查阅。隔离诊断目录拒写不是业务成功证据。hook进程仅运行测试程序和临时文件；未知效果保持unknown，不等同撤销实际效果。没有真实模型、安装/发布或日常实例测试；HA11质量/性能仍待真实配对。
