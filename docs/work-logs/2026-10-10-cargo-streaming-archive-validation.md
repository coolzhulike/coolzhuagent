# Cargo 回执逐条归档验证

Save-PackageCargoBuildEvidence 移除 ReadAllBytes/完整解码/split/全记录数组。以只读共享句柄计算 SHA，逐条读取并核对最终成功终态、组件身份与计数，再从同一句柄以 81920 字节缓冲复制到独占归档并同步。Windows 该句柄拒绝外部写入/删除，避免哈希与复制之间切换源文件。

读取器在 C# 中逐字符处理行，单行最多 4 Mi 字符；超限明确失败，不截断为有效记录。逐条 JSON 严格 UTF-8 解码，保留 UTF-8 BOM 兼容、布尔 success 约束、终态后记录拒绝及原字节摘要关联。只保留当前记录与计数，不保留全部构建观察对象。

实际 PS7 和 PS5.1 均执行回执测试 19 项通过、归档消费测试 11 项通过。额外 PS7 合成 10000 条 compiler-artifact 记录验证完整计数、长度和原字节摘要一致，并验证超长行拒绝且不发布归档。日志 tmp/analysis-cargo-stream-archive-tests-{7,5}.txt、tmp/analysis-cargo-stream-consumer-tests-{7,5}.txt、tmp/analysis-cargo-stream-large-test-result.txt。合成记录不是真实构建观察，不计正式整包验收。

这是归档生产端修补；Assert-PackageCargoBuildEvidence 消费端、package-all 的进程采集与 Loader 消费仍有整体读取，6.1/6.4 继续开放。单行上限对异常巨大合法记录的适配未完成，实际新整包及源文件外部竞争矩阵仍需验证。安装版 0.2.130 保持，32WBS 未完成。
