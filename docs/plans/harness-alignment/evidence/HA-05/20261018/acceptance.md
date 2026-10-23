# HA-05逐条验收

当前循环只对宿主确认的同一组连续失败计数，上限为3。相同参数的成功读取、运行中或未知语义不会被断言为“没有进展”。没有增加进展分类模型、审批或新的根预算。

| 条目 | 实际断言 | 原始证据 |
|---|---|---|
| HA05-01 | 同参数真实读取3次，running版本1→completed版本2；L1为0。Agent等待只有根与一个子请求，完成后仅一次反馈。 | ha05-RunningThenDone.json、ha05-wait-Complete.json |
| HA05-02 | 同一bash实际失败3次，错误时间戳变化不续期；模型请求3次，终态tool_repeat_failure_limit，无第四次恢复请求。 | ha05-PermanentFailure.json |
| HA05-03 | 两次实际失败后外部ready文件版本变化，同参数第三次成功；模型请求4次，L1为0。 | ha05-ExternalRecovery.json |
| HA05-04 | 在子请求屏障接纳取消或等原根截止，工具始终1次、请求始终2次，子执行不写completed，根/子deadline一致。 | ha05-wait-Cancel.json、ha05-wait-Deadline.json、audit.json |

上述JSON在runs/web-regression-02；同次实际76项回归全部通过，包含HA01～04原70项、HA05新增5项及ACP等待1项。离线生产构建退出0，命令、环境、开始/结束、输入哈希在各receipt。构建不代替行为验收。

首轮73通过3失败保留在web-regression-01：夹具复用人类输入/房间触发真实重复提交保护。修复为独立场景身份，没有关闭生产去重。诊断日志的隔离目录拒写不作为成功依据；业务回执来自独立SQLite和真实本地HTTP。

这些结果只证明确定性调度及局部失败护栏。未知工具语义沿原反馈上限/根期限约束；不宣称能理解所有任务进展。没有使用真实模型，不证明HA11质量/耗时收益。没有安装、发布或触碰日常8765实例。
