# 附加发布证据的归档名称和字节冲突

检查 Cargo 回执随 MSI 归档的路径时，发现既有 retention 实现按 basename 删除旧目标后强制复制：两个来源同名会互相覆盖，附加文件若名为 package-report.json / payload-inventory.json / package-report-files.json，也可能覆盖权威报告载体。

本轮先验证全部附加来源和命名，再写报告/清单副本：来源必须为工作区内已存在文件，拒绝来源及父目录重解析点；保留名称与来源间同名均拒绝。一次读取来源字节并计算身份，已有同名发布副本若字节不同则拒绝，字节相同可重复归档。新增副本以 CreateNew 独占创建、Flush(true)，写后再核对哈希；删除旧副本的分支移除。

专项 test-retention-extra-evidence 实际 exit 0，7 cases：首次归档、相同字节重复、保留名称冲突、两个来源同名、缺失来源、工作区外来源及既有异字节副本。所有拒绝用例核对既有回执及归档报告未改写；日志 tmp/analysis-retention-extra-evidence-final-tests.txt。夹具是独立 tmp 下的治理报告，不代表正式发布。未在当前环境创建重解析点，所以源码防护不能提升为该路径实机负例通过。

主体修补后的完整 package-build-identity 回归 81 cases、0 failed，日志 tmp/analysis-retention-extra-evidence-tests.txt；最终追加来源重解析检查后再次运行完整回归，日志 tmp/analysis-retention-extra-evidence-final-regression.txt，终态另行核对。本轮没有 Rust 生产变更、实际 MSI 生成或安装更新。

最终完整回归已核对实际 exit 0：81 cases、0 failed。上述等待状态为执行过程记录，不能替代该终态。

初始目标中的“原始临时回执清理后，归档副本独立寻址/验证”仍未实现。此次先封住覆盖风险，不宣称该原目标已闭环。整体报告/清单重复归档、断电/磁盘故障与全量正式发布证据仍需继续验收。
