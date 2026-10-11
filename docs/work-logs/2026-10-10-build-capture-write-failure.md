# 2026-10-10 构建采集写入故障与收尾期限

生产采集新增收尾时限：Job 回收最多 10 秒，随后管道终态及线程完成最多再等待 20 秒；超时返回明确未确认错误，辅助程序主入口退出125。只在 is_finished 后 join，避免无界 join 冒充收尾。超时不会宣称两泵已完成；该错误路径由辅助进程退出关闭本地资源，完整异常内核I/O终止矩阵仍待实际证明。

新增默认关闭的 build-capture-test-support feature，专用构建在指定管道第一块写入后注入写入失败。默认二进制重新构建且检查不包含故障环境变量名称。故障二进制另存 tmp/build-capture-fault.exe，不用于正常交付。

真实根 PowerShell 启动睡眠后代并持续输出；stdout/stderr 两项注入均在15秒观察时限内返回125，根与后代PID均已退出。驱动退出0，日志 tmp/analysis-build-capture-write-failure-tests.txt。软件注入不是实际磁盘满、设备断开或 sync_all 故障，不能混称。

默认构建增量/独占/后代检查退出0，日志 tmp/analysis-build-capture-bounded-normal.txt。完整进程守卫回归退出0：54通过、0失败、3忽略，工具会话61012终态0，日志 tmp/analysis-build-capture-bounded-job-tests.txt。默认与故障离线 build 均退出0，相关日志 analysis-build-capture-bounded-build.txt、fault-build.txt、default-restored.txt。

尚未接入 package-all、冻结构建输入或发布清单；宿主强退、真实磁盘错误、有界回收失败实测、PS5驱动与正式验收仍未闭环。32WBS 保持进行。
