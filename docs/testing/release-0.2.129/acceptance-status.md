# 0.2.129 候选验收状态

2026-10-10：正常 release 整包已生成，独立报告校验实际 exit 0；MSI 构建实际 exit 0。冻结源码提交为 270b69b01c93301c9d5f52e9dbe01813df6b3aaf。候选包含 1192 个载荷文件、435606384 字节，发布资格为 true；这不是正式运行验收通过声明。

包报告 ID 为 pkg-report-release-20261010-123444663-d73a995a，内容 SHA-256 为 b10b5750a6287ec5560ea9d1e4a02a46b123cc8366502fa4d06907064650d028。来源快照为 6dcc9248cbb35a56537be5ae6e2131727c0b64a874f0d45a45eca5e6e6c272cc，构建输入摘要为 b703533b64f0c66e8486d22d4e8f04a763f59899fd614e054814c25ee2300eaa，载荷摘要为 e9ce02bf12d579344475ae73296cc2a6cbf906de39b1c79467c2d408af7e5ff5。

本次九个 Cargo 入口均采集实际 JSON 回执，唯一最终成功记录、预期入口集合及报告引用字节通过独立校验。MSI 消费端再次核验九条回执，发布归档中的报告与九条回执也通过独立核验。证据位于 evidence/build-identity/pkg-report-release-20261010-123444663-d73a995a/。构建观察包含缓存及构建期组件，不等同于运行时分发组件、完整许可证义务或来源签名。

最终 MSI 为 dist/CoolzhuAgent-0.2.129.msi，287958989 字节，SHA-256 为 c0ef896664a09fdb600399f84190e47176ce7d8674e3c83a0cff8847ef1e1d6a；已独立核对与安装报告一致。候选 CLI 实际 --version 返回 0.2.129、上述完整源码 SHA、x86_64-pc-windows-msvc、2026-10-10。WebView2Loader 来自已有 Cargo 构建目录，报告明确标注复用，不能写成新生产。

外层临时整包脚本在正常出包之后因未加载身份库而后置校验失败，实际 exit 1，日志 tmp/analysis-integration-129-package-build.txt 保留。独立校验首次在 PowerShell 7 默认日期转换下触发原始日期字符串保护；改用与打包一致的 Windows PowerShell 后，报告内容哈希、发布资格、九入口集合和回执字节全部通过，日志 tmp/analysis-integration-129-independent-verification.txt。不能把外层脚本失败记录改成 exit 0。

本版本未安装、未签名、未发布；已安装版仍是此前核验的 0.2.128。恢复原验收后台的命令此前被自动审批策略拒绝，正式运行验收未恢复。Browser 输入归属负例、外部模型长程、取消恢复、多屏及其余 32WBS 门禁继续开放。完整许可证/NOTICE、组件 SBOM、签名和发布关联仍未闭环。

后续许可证证据核查：根工作区与 Tauri 各执行 locked/offline metadata，命令实际 exit 0；构建源码到本次 metadata 核查之间 Cargo.toml、Cargo.lock 及 modules 无变更。该 metadata 是构建后观察，不冒充构建时采集。九份原文导出分别绑定本次实际回执 SHA，独立解码核验 2216 条文件观察的长度与 SHA，未将重复观察相加为组件总数。按精确组件身份摘要去重后为 433 个观察组件，其中 391 个找到本地原文、42 个未找到（18 个本地、24 个外部）；同一身份的原文证据一致。缺项清单见 evidence/licenses/build-observation-gaps.json。没有在父目录补造原文，没有将未找到原文判定为没有许可证；许可证义务与实际分发映射仍未验证。
