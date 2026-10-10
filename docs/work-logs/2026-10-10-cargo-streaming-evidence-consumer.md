# Cargo 回执消费端逐条验证

Assert-PackageCargoBuildEvidence 不再 ReadAllBytes 原始 JSONL；只读句柄上流式 SHA、长度核对后复用 Get-CargoStreamSummary，逐条验证最终成功终态、组件身份，并核对报告声明 compiler_artifact_records 的整数类型与实际计数。未改变路径、重解析点、运行/产物身份及旧报告未验证门禁。

PS7/PS5.1 回执测试均 21 项通过，新增错误计数与字符串计数拒绝；两种 PowerShell 的独立发布归档消费测试均 11 项通过。实际读取 0.2.130 九份归档回执，使用原报告身份与显式归档映射，两种 PowerShell 均逐条验证通过。旧归档字节未修改，不计新构建或正式安装验收。

日志 tmp/analysis-cargo-stream-consumer-v2-tests-{7,5}.txt、tmp/analysis-cargo-stream-archive-consumer-v2-{7,5}.txt、tmp/analysis-stream-check-130-{7,5}.txt。真实兼容驱动 tmp/analysis-stream-check-130.ps1 与各脚本均退出 0。

整包进程采集、Loader 消费与归档报告 JSON 仍有整体读取；采集磁盘错误/停止子树、全局配额及正式新包矩阵继续开放。此实现只关闭回执生产/消费重复驻留的子项，不将 6.1/6.4 或全部 32WBS 标为完成。
