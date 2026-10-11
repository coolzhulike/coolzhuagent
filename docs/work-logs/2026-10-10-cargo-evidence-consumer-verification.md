# MSI 消费端核对 Cargo 回执引用

新增 Assert-PackageCargoBuildEvidence：collected 模式要求非空条目、与报告相同运行身份、唯一产物、明确成功标志、仓库内规范相对路径与运行/产物文件名后缀；拒绝重解析点。一次读取归档字节，核对整数长度和 SHA256。旧报告缺字段返回 unavailable-legacy-report / verified=false；未采集模式也返回 false，不能携带已采集条目。

build-msi 在验证包报告内容哈希之后、产生 MSI 之前调用该检查。installer report 保存字节引用校验状态、条目数和范围限制；发布证据归档前再次核对，通过的原始回执与 installer report 一起传入既有保留/归档入口。没有给旧版本包回填或宣称其拥有这些新证据。

test-cargo-build-evidence 实际 exit 0，19 cases。消费端新增合法引用、旧报告未知、外国运行、重复/空集合、长度值/类型、路径逃逸、成功标志、SHA 错误和归档正文改写负例；日志 tmp/analysis-cargo-evidence-consumer-tests.txt。build-msi PowerShell AST 解析无错误。前轮真实启动器回执归档也实际通过消费端字节核对，结果 tmp/analysis-cargo-evidence-consumer-real.json，SHA256 20330f2ecf16d43ed5804ba91012d644018ec33a7f65190b042e5c1534462f68。

本轮未实际构建 MSI 或更新安装，也不把语法检查/函数测试作为完整发布链验收。归档副本在原始 tmp 清理后的寻址、并发修改边界、完整正式 9 入口回执、metadata 冻结与最终分发组件/许可证原文映射仍需继续验收。字节校验不证明来源签名、组件完整性或许可证义务；32WBS 的正式交付门禁继续开放。
