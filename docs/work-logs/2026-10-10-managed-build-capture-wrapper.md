# 2026-10-10 受管构建采集 PowerShell 入口

新增共享参数组装及 Invoke-ManagedBuildCapture。入口要求调用方提供 SHA256，持有拒绝写入和删除的文件读句柄，启动前后核对辅助程序摘要；调用方仍须从冻结构建证据提供摘要，不能由现有二进制自行计算冒充来源证明。

真实 PS5.1 与 PS7 专项各三项通过（均进程退出码0）：正确摘要下双管道 UTF8 内容完整并保留命令退出码7；错误摘要在启动前拒绝且没有命令或日志副作用；既有日志触发辅助程序125，原字节保留且命令未启动。原共享参数驱动 PS5.1/PS7 各四项也已通过。证据位于 tmp/analysis-managed-capture-wrapper-ps5.txt、tmp/analysis-managed-capture-wrapper-ps7.txt 和 tmp/analysis-managed-capture-shared-ps5.txt、tmp/analysis-managed-capture-shared-ps7.txt。

本轮仅 PowerShell 修改，无 Rust 生产代码变化。尚未接入 package-all.ps1；辅助程序 bootstrap、冻结来源、组件回执和正式整包验收仍未闭环。宿主被终止时的辅助程序生命周期也尚未验证。本提交不证明32WBS或正式交付完成。
