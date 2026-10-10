# 0.2.130 候选验收状态

2026-10-10：正常 release 整包与 MSI 构建均实际 exit 0，冻结源码为 35cbcf238d194d7fbaac648359d8bddc9858f811。本次临时整包脚本加载身份库并用原始日期字符串解析报告；内容哈希、发布资格、九入口集合、回执字节和包根 LICENSE 核验均通过，不沿用 0.2.129 的外层脚本失败结果。

报告 ID 为 pkg-report-release-20261010-125305944-75bdd999，内容 SHA-256 为 9333c193df9e4cc6561fec741641fce636696cde644680a774352ac401986780。来源快照为 5c08e47689a2521db000e442481d0cbe5297e93e86288fc81fdbf0981bfb999e，构建输入摘要为 67659a3c4e44821af842e8173b8cb0da16a921d6d036e83abefa1fc153a9de07，载荷摘要为 ee32fa0d560978d4be19485b618d7d0c993e00cc629846aa8ef6a94c37a83e46。

载荷含 1193 个文件、435607616 字节。根 LICENSE 已实际进入包根，SHA-256 与仓库根原文一致，为 ae27c00cdb7ce9ebd247a94a07319624d24ed29093c7d70b11a1368b581c65aa。来源快照包含根及 18 个组件 LICENSE，共 19 项；组件无扩展名文件的 unclassified 标记只用于可见性，不改变快照纳入。不能把第一方原文补齐提升为全部第三方许可证义务完成。

候选 CLI 实际 --version 返回 0.2.130、上述完整源码 SHA、x86_64-pc-windows-msvc、2026-10-10。最终 MSI 为 dist/CoolzhuAgent-0.2.130.msi，287967249 字节，SHA-256 为 6c40fcfffb93c7f2d0973c6fe226739f9bc5be0318b65f09c4e5abb06323c7fa；已独立核对安装报告绑定。WebView2Loader 仍明确标记为已有 Cargo 构建目录输出复用，不称为本次新生产。

归档报告与九入口回执实际核验通过，证据见 evidence/build-identity/pkg-report-release-20261010-125305944-75bdd999/。日志 tmp/analysis-integration-130-package-build.txt、tmp/analysis-integration-130-msi-build.txt 和 tmp/analysis-integration-130-archived-verification.json。构建观察包含缓存与构建期组件，不证明全部实际分发组件、来源签名或许可证义务。

后续正常 MSI 升级实际 exit 0，未自动重启或启动后台。安装目录 1193 个载荷文件逐长度及 SHA 全部一致，唯一清单外文件为清单自身载体 payload-inventory.json。安装根 LICENSE 摘要与候选及仓库根一致；实装 CLI 实际返回 0.2.130 及上述完整源码 SHA。安装结果、逐文件核对及注册版本证据见 evidence/install/。

本版本已安装，未签名、未发布。恢复原验收后台的命令此前被自动审批策略拒绝，正式运行验收未恢复；安装与文件核验不能代替真实运行。外部许可证原文缺项、完整 NOTICE、组件映射、CI/发布关联及其余 32WBS 门禁继续开放。0.2.129 历史原文缺项清单保留，不用本次补丁改写旧包事实。
