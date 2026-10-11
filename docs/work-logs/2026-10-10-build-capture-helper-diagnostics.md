# 2026-10-10 采集辅助程序自身诊断归档

直接原生命令调用相同辅助程序和 Cargo 构建20次均exit0（tmp/analysis-capture-125-repro.json），未复现整包中的125，不能由此宣布问题消失或断言根因。

Invoke-ManagedBuildCapture 新增 helper stderr 固定缓冲 CopyToAsync 独立CreateNew日志，名称含GUID；受管命令原始双管道仍由 Rust helper采集。失败引导异常给出helper诊断路径。首次固定诊断路径导致旧日志用例提前被拒绝，已改独立GUID并重跑。PS5.1/PS7各四项实际通过、进程退出0，含辅助程序125诊断真实落盘。证据 tmp/analysis-capture-diagnostic-ps5.txt、tmp/analysis-capture-diagnostic-ps7.txt。

尚未修复此前两项整包125失败，须使用新诊断再次真实复现。日志写入故障下CopyToAsync完成/宿主生命周期仍需受管终止验证，不能把本次诊断采集视为全故障闭环。package-all生产接入仍未提交，正式验收和32WBS未完成。
