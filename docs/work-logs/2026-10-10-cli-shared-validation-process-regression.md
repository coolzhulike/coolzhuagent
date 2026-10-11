# 2026-10-10 共享事件校验后真实 CLI 恢复矩阵

当前源码 2dacc8300d7811c903bcb3f34d4c7df680d97c5a，使用最新离线构建的 tmp/cargo-target-integration/debug/coolzhu-cli.exe。核对执行后报告二进制 SHA 与当前文件一致，再归档 docs/testing/2026-10-10-cli-history-restore/shared-validation-process.json。

PS5.1 首次驱动退出 1，因 ErrorActionPreference=Stop 将预期拒绝场景 stderr 转为 NativeCommandError，驱动提前退出；日志 tmp/analysis-cli-history-shared-validation-process.txt。没有把该退出记为产品矩阵失败或通过，也未使用旧结果文件证明本次结果。

PS7 执行同一临时驱动实际退出 0，legacy/completed/failed/interrupted 恢复 0，truncated/duplicate/sequence/future-schema/empty-identity 拒绝 1；9 项会话文件 SHA 均不变。日志 tmp/analysis-cli-history-shared-validation-process-ps7.txt。夹具位于独立 tmp，使用最旧事件目录以验证兼容，不调用真实模型、不启动正式后台。

更新总门禁表顶部复核点和最新回归范围。此证据仅为 debug 二进制与合成历史真实进程恢复，不是已安装 0.2.130 或正式长程验收。全部 32WBS 保持进行。
