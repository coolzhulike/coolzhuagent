# 原始临时引用缺失后的 Cargo 归档验证

新增 Assert-ArchivedPackageCargoBuildEvidence：从发布归档清单读取报告副本，核对原始文件 SHA256、重算规范内容哈希并匹配 report_id；每个报告回执引用必须在 extra_evidence 中唯一找到相同 source_path 和 SHA256 的映射。副本路径必须位于该归档目录和当前仓库内，拒绝路径逃逸及重解析点；随后沿原报告的运行/产物身份、长度和哈希核对归档回执字节，不改写报告引用或内容哈希。

保留工具 Verify 已接入此路径：对于 collected 报告，已登记发布归档时使用归档清单验证并匹配保留索引报告身份；未归档时仍核对原始引用。失败会进入原 Verify 的问题集合并拒绝通过，而不是只看到报告 JSON 尚在就宣布其附属回执完整。

test-archived-cargo-build-evidence 实际 exit 0，11 cases：通过现有 Protect 真实生成夹具归档后，将本测试独占的原始报告和回执移到另一个 tmp 文件名保留；原路径验证如实失败，独立归档及保留工具 Verify 通过。另验证外国报告身份、报告文件哈希错误、缺失/重复映射、映射哈希错误、路径逃逸及副本正文改写，最后正文改写也使保留工具 Verify 拒绝。日志 tmp/analysis-archived-cargo-final-tests.txt。

既有回执测试 19 cases 通过，日志 tmp/analysis-archived-cargo-existing-tests.txt。完整 package-build-identity 最终实际 exit 0，81 cases、0 failed；日志 tmp/analysis-archived-cargo-report-regression.txt。本轮没有 Rust 生产变更、真实 MSI 构建、用户证据清理或安装更新。

这证明候选实现和治理夹具中的独立归档寻址/字节验证，不代表来源签名、全量分发组件或许可证完整性。最终正式 release 的全部 9 个入口回执、原文与载荷映射、清理/恢复正式演练及整包安装验收仍需取得真实证据，32WBS 不完成。
