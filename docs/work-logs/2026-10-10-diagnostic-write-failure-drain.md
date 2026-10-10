# 构建辅助程序诊断写入故障

原 PowerShell 包装器使用 CopyToAsync 落盘辅助程序 stderr，然后先 WaitForExit。目标写入失败会终止复制；若生产者继续输出超过管道容量，退出等待可能一直不返回。

本轮改为固定81920字节缓冲的独立 .NET 任务。首次目标写入异常保留，随后继续读取到 EOF，停止写目标；正常辅助程序退出后，包装器以 BUILD-CAPTURE-DIAGNOSTIC 拒绝回执，不以不完整诊断冒充成功。没有将失败输出无界驻留内存，没有修改 Rust 或现有 MSI。

Windows PowerShell5.1及PowerShell7各实际exit0，五项均通过：正常双管道中文和退出7、错误身份启动前拒绝、旧日志保护125、辅助程序失败诊断归档、真实4MiB stderr 在已关闭目标流的写入故障后仍完成排空并观察生产者exit9及原异常。原始日志分别为 tmp/analysis-diagnostic-drain-ps5.txt 与 tmp/analysis-diagnostic-drain-ps7.txt。测试中10秒超时仅用于测试判定和清理该测试自身生产者。

Add-Type 编译的 .NET 代码在两种宿主实际执行；无 Rust 生产修改，本轮未重新编译已有候选。读取端异常、宿主被终止、无限输出生产者及全局配额仍缺完整生命周期证明；此修补尚未进入0.2.131 MSI，不宣称6.1或32WBS闭环。

后续读取端修补：退出等待改为每100毫秒同时检查诊断任务故障/取消，故障时仅停止本次持有 Process 的辅助程序，并在10秒内确认根退出；未确认则明确拒绝，不伪造回执。正常构建没有增加运行时限。Windows受管Job采用KILL_ON_JOB_CLOSE，但静态代码事实不能代替该故障路径的真实后代回收验收。

PS5.1和PS7最新各六项实际exit0，新增已关闭读取流触发任务故障，真实启动30秒等待的测试生产者被及时停止并确认退出。日志 tmp/analysis-diagnostic-read-failure-ps5.txt 与 tmp/analysis-diagnostic-read-failure-ps7.txt。该注入验证任务故障观察及自身根停止，尚未验证辅助程序持有真实Job时全部后代退出、阻塞读取取消及宿主终止；这些继续开放。现有MSI不包含该源码修改。
