# 构建辅助程序诊断写入故障

原 PowerShell 包装器使用 CopyToAsync 落盘辅助程序 stderr，然后先 WaitForExit。目标写入失败会终止复制；若生产者继续输出超过管道容量，退出等待可能一直不返回。

本轮改为固定81920字节缓冲的独立 .NET 任务。首次目标写入异常保留，随后继续读取到 EOF，停止写目标；正常辅助程序退出后，包装器以 BUILD-CAPTURE-DIAGNOSTIC 拒绝回执，不以不完整诊断冒充成功。没有将失败输出无界驻留内存，没有修改 Rust 或现有 MSI。

Windows PowerShell5.1及PowerShell7各实际exit0，五项均通过：正常双管道中文和退出7、错误身份启动前拒绝、旧日志保护125、辅助程序失败诊断归档、真实4MiB stderr 在已关闭目标流的写入故障后仍完成排空并观察生产者exit9及原异常。原始日志分别为 tmp/analysis-diagnostic-drain-ps5.txt 与 tmp/analysis-diagnostic-drain-ps7.txt。测试中10秒超时仅用于测试判定和清理该测试自身生产者。

Add-Type 编译的 .NET 代码在两种宿主实际执行；无 Rust 生产修改，本轮未重新编译已有候选。读取端异常、宿主被终止、无限输出生产者及全局配额仍缺完整生命周期证明；此修补尚未进入0.2.131 MSI，不宣称6.1或32WBS闭环。

后续读取端修补：退出等待改为每100毫秒同时检查诊断任务故障/取消，故障时仅停止本次持有 Process 的辅助程序，并在10秒内确认根退出；未确认则明确拒绝，不伪造回执。正常构建没有增加运行时限。Windows受管Job采用KILL_ON_JOB_CLOSE，但静态代码事实不能代替该故障路径的真实后代回收验收。

PS5.1和PS7最新各六项实际exit0，新增已关闭读取流触发任务故障，真实启动30秒等待的测试生产者被及时停止并确认退出。日志 tmp/analysis-diagnostic-read-failure-ps5.txt 与 tmp/analysis-diagnostic-read-failure-ps7.txt。该注入验证任务故障观察及自身根停止，尚未验证辅助程序持有真实Job时全部后代退出、阻塞读取取消及宿主终止；这些继续开放。现有MSI不包含该源码修改。

后续真实Job验证：scripts/test-managed-capture-tree-failure.ps1 实际启动正常 build-capture.exe，由其启动PowerShell根及60秒等待后代；两者到达屏障后持有对应进程句柄，再注入已关闭源流触发读取任务故障。实际调用生产 Wait-ManagedCaptureDiagnostic 拒绝路径；辅助程序退出、Job内根与后代均在10秒观察窗内退出，在最终清理前完成判定。PS5.1/PS7均实际exit0，日志为 tmp/analysis-capture-tree-read-failure-ps5.txt、ps7.txt，PID及辅助程序摘要归档 docs/testing/managed-capture-diagnostic-failure/tree-ps5-result.json、tree-ps7-result.json。注入针对读取任务故障，并非真实内核匿名管道断裂；不外推阻塞读取取消、宿主突然退出、全局配额或整个6.1完成。

宿主终止实测失败：新增 -HostExit 路径，真实PowerShell宿主调用生产 Invoke-ManagedBuildCapture，受管命令到达屏障后核对Win32_Process父子链，持有采集器、根、后代句柄，再只终止夹具宿主。三个进程各1秒退出观察均失败，未产生成功回执，测试实际exit1。独立Job仅由辅助程序持有，宿主死亡未关闭该Job；主动读取故障回收不能证明宿主死亡回收。失败原始观察归档 docs/testing/managed-capture-diagnostic-failure/host-exit-failure.json，日志 tmp/analysis-capture-host-exit.txt。最终夹具只清理自身持有的进程，清理后的PID观察独立保存host-exit-cleanup.json；不得计作产品通过。仍需实现可靠宿主生命周期绑定，并覆盖启动竞态及宿主死亡前后全部进程树，不得只添加结束时轮询冒充已解决。

后续生产修补：包装器传入宿主PID及StartTime的FILETIME；Rust采集器在日志创建和构建启动前打开查询/同步权限的同一宿主句柄，核对GetProcessTimes创建时间，持续持有到收尾。启动前、采集循环每轮及返回成功前检查宿主退出，失败进入既有Job终止与有界排空路径；PID复用不能替代原宿主。已实际cargo build --locked --offline -j1成功，日志 tmp/analysis-capture-owner-build.txt。完整guard回归54通过、0失败、3忽略，实际exit0，日志 tmp/analysis-capture-owner-guard-tests.txt。

原HostExit反例复验实际exit0，辅助程序、根及后代全部在最终清理前退出，未生成成功回执；新摘要/PID观察归档host-exit-owner-bound-result.json，原失败保留。PS5.1/PS7包装器原六项实际通过；PS5.1另补宿主创建时间不匹配125且日志/命令均未启动，第七项通过，日志 tmp/analysis-capture-owner-identity.txt。新增所有者参数目前仅生产包装器强制提供，旧直接CLI语法仍允许不绑定宿主；其迁移、启动前后竞态矩阵、权限查询失败、全部入口和正式新候选出包仍需完成，不宣称完整生命周期门禁关闭。
