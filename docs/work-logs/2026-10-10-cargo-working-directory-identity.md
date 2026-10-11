# 2026-10-10 实际构建工作目录配置身份

检查正式manifest九个构建入口，当前全部working_dir=仓库根。Get-BuildInputDescriptors现额外登记每个构建命令与展开后的绝对工作目录，Cargo入口从该实际目录向上登记配置身份，防止未来嵌套构建目录配置创建/修改未被冻结比较发现。--manifest-path仍不等于切换工作目录。

PS5.1/PS7专项真实exit0，通过嵌套Cargo配置创建导致描述符变化、工作目录变化导致输入身份变化、配置正文canary不泄漏；此前祖先/用户配置场景也通过。证据 tmp/analysis-cargo-nested-config-ps5.txt、tmp/analysis-cargo-nested-config-ps7.txt。完整构建身份契约句柄29054终态exit0，81通过0失败，日志 tmp/analysis-cargo-nested-identity-regression.txt。本轮纯PowerShell修改。

仍需配置include/链接、工具链实际身份、不可变输入与正式打包验收；本改动不宣称全部Cargo解析来源被证明。32WBS与正式交付仍未闭环。
