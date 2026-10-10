# 2026-10-10 Cargo外部配置身份

Get-BuildInputDescriptors现登记仓库根向上至盘根的.cargo/config及config.toml，以及实际CARGO_HOME（未设置则UserProfile/.cargo）中的两种配置。登记缺失状态和完整字节SHA，不保存配置正文。创建/删除也改变冻结描述符，避免仅登记存在文件导致配置新增漏检。两个配置都登记，保守绑定被Cargo优先级遮蔽的文件。

PS5.1/PS7专项真实exit0：祖先配置创建、修改、删除身份变化；用户配置登记，canary正文不泄漏。日志 tmp/analysis-cargo-config-identity-ps5.txt、tmp/analysis-cargo-config-identity-ps7.txt。首次专项因误用只读HOME变量失败，已更名fixtureCargoHome后重跑，不计首次为通过。

完整构建身份契约实际句柄23241终态exit0，81通过0失败，日志 tmp/analysis-cargo-config-build-identity-regression.txt。本轮纯PowerShell修改，无Rust生产变化。

仍不证明immutable输入，存在链接/并发改写及两次采样之间变化边界；配置include、不同构建工作目录的更深层配置和其他环境覆盖需继续核验。正式交付与32WBS未闭环。
