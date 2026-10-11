# 2026-10-10 构建原始输出采集辅助程序（进行中）

新增 windows-process-guard 的 build-capture 二进制候选，复用 ChildProcessJob::spawn_managed 的挂起创建/绑定/恢复边界。两个输出文件 create_new 成功后才启动命令；两线程各 64KiB 缓冲原始字节写盘并 sync_all，通过最多两条结果消息回报。采集失败或根进程结束时 terminate_and_wait 回收本次 Job，等待采集线程 join；返回原构建退出码，采集失败返回 125。根退出后仍运行后代会被回收，不提供后台构建语义。

真实离线 build 实际退出 0；真实 PowerShell 子进程分别向 stdout/stderr 写 4213760 字节（中文 UTF-8 + 大输出），两份原始日志 SHA 与独立期望一致，子进程退出码 7 保留。临时验证驱动实际退出 0；日志 tmp/analysis-build-capture-helper-build-final.txt、tmp/analysis-build-capture-helper-real-test-final.txt。修复轮询错误终态，避免永久重复 try_wait 错误。

候选尚未提交、未接入 package-all、未进入发布 manifest。仍需：增量落盘屏障、既有日志拒绝且不启动命令、写盘错误/磁盘满和两泵收尾、宿主强退子树回收、超时或回收失败的有界退出、参数路径空格与双系统 PowerShell 兼容。不得以大输出正常例宣称完整故障链通过。32WBS 总目标继续进行。

后续真实检查：scripts/test-build-capture.ps1（明确 PS7 驱动）通过增量屏障、既有 stdout、既有 stderr 三项。构建仍等待释放时两管道文件已有字节；已有日志触发退出 125，原 SHA 不变，命令 ready 标记没有创建。路径含空格，ArgumentList 传参。实际驱动退出 0，日志 tmp/analysis-build-capture-incremental-tests-final.txt。第二日志创建失败时可能留下本次空 stdout，仅作为失败诊断，不是成功回执。
新待修：根命令先退出而后代仍活跃时，当前辅助程序会终止后代，仍可能返回根退出码 0；需识别未完成子树并明确拒绝成功，不能把回收造成的输出结束冒充完整构建。故障写盘与回收失败仍可能造成 join 无界，接入整包前必须处理。候选保持未提交、未接入。

后续修复：ChildProcessJob 新增 active_process_count，采集程序在根退出后、终止 Job 前查询活跃后代；非零或查询失败明确返回采集失败。真实 PowerShell 根创建睡眠后代并返回0，辅助程序实际返回125，检查后代PID已不存在或已退出。增量/独占三项回归及双管道大输出退出7均再通过，驱动实际退出0。日志 tmp/analysis-build-capture-descendants-build.txt、descendants-real-test.txt、descendants-large.txt。完整进程守卫回归正在串行执行，终态待确认；仍未接入整包，写盘错误、宿主强退、有界故障收尾和PS5驱动未闭环。

进程守卫完整回归实际退出0，同一会话50831终态0：54通过、0失败、3忽略；二进制与Doc-tests均0测试。忽略不计通过。候选以未接入整包的状态提交供继续开发，先前未提交描述保留其当时时点，发布资格仍未成立。
