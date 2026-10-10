# Cargo 原始构建回执绑定包报告

package-all 在采集 stdout JSON 消息后记录原始文件 SHA256；成功报告生成前，由新 cargo-build-evidence.ps1 再次读取固定字节并核对该哈希。要求唯一最终 success=true 的 build-finished、至少一条具有包/目标身份的 compiler-artifact；布尔字段必须是真布尔值，字符串 true 不接受。

回执通过 CreateNew 独占写入 tmp/package-reports/build-evidence/<run_id>/<artifact_id>.jsonl 并 Flush(true)，失败写入清理本次创建的残片，不能删除或覆盖已存在的归档。归档位于临时报告域，不写源码声明范围；helper 已显式登记为构建输入。成功报告的 cargo_build_evidence 记录原始回执运行、产物、仓库相对路径、长度、哈希、组件记录数及范围限制，字段在报告内容哈希计算前加入。no-build 明确 not-collected-no-build，旧报告不回填。

实际验证：

- test-cargo-build-evidence.ps1 实际 exit 0，8 cases，覆盖归档字节/身份、独占创建、终态后追加、失败/字符串终态、重复终态、组件身份缺失及采集后字节漂移；日志 tmp/analysis-cargo-evidence-archive-tests.txt。
- 完整 package-build-identity 实际 exit 0，81 cases、0 failed。新增模拟消息采集的完整 package-all 用例，校验归档引用进入真实生成的报告并重算内容哈希；不将模拟编译消息当成真实 Cargo 发布。日志 tmp/analysis-cargo-evidence-report-final-tests.txt。
- 用前轮真实启动器 JSON 消息实测 helper，归档 20571 字节、18 条 compiler-artifact，SHA256 20330f2ecf16d43ed5804ba91012d644018ec33a7f65190b042e5c1534462f68；结果 tmp/analysis-cargo-evidence-real-result.json。该验证不是完整正式 release 包运行。

本轮没有 Rust 生产变更、安装更新、共享后台启动或发布动作。报告引用的长期保留/搬迁验证、全量 release 9 个入口的真实回执、metadata 同次冻结、许可证原文与载荷映射、最终 NOTICE 及签名仍未闭环。构建消息包含缓存及 build/proc-macro 组件，不能据此宣布完整运行时分发组件清单。
