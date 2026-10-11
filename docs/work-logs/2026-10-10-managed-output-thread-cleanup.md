# 同步进程异常后的管道线程收尾

候选在 f795965 的启动后错误分类基础上继续修复采集生命周期：原先 stdout join/读取错误会立即返回，stderr 与 stdin 的 JoinHandle 随之被丢弃；等待进程时的错误也会跳过线程回收。这不符合返回前收齐本次执行资源的要求。

现在等待过程先保存 Result；错误时先尝试关闭所属子树，再分别 join stdout、stderr、stdin，全部 join 后才传播等待、读取或写入错误。正常完成与已确认取消/超时的协议保持，取消造成的 stdin BrokenPipe 仍不能抹掉中止事实。启动后错误保留同一分类，Hook 继续拒绝派发。

新增故障注入覆盖 stdout 返回读取错误及读取线程 panic 两种情况，另一根管道延迟完成；返回错误时确认其完成标记已经设置。真实双管道大输出、stdin 交付失败、根取消与子孙进程截止用例仍由完整运行时回归验证。该注入不是实机磁盘故障或 OS kill 失败证明。

生产构建实际 exit 0：tmp/analysis-managed-reader-join-build.txt。完整串行回归实际 exit 0，367 通过、0 失败；库 MCP 及两项 DSH 集成测试共 3 项忽略，不计通过。日志 tmp/analysis-managed-reader-join-tests.txt。本修补不限制输出容量，不实现目录配额；OS 回收失败、非 Windows 子树隔离以及全部宿主正式验收仍未闭环。前轮并发 prompt 临时目录占用失败也继续保留，未凭串行通过声称已修复。
