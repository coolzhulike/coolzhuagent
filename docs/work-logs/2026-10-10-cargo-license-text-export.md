# Cargo 许可证原文证据导出

新增 scripts/export-cargo-license-evidence.py，以已冻结 Cargo metadata 原始字节作为解析输入，记录 metadata SHA256、组件身份摘要、原声明表达式及组件根内可观察的 LICENSE/COPYING/COPYRIGHT/NOTICE 文件。原文字节为 UTF-8 时直接保存，否则使用 base64 无损保存，同时记录长度和 SHA256。不扫描父目录；明确 license_file 缺失、不可读或逃出组件根时登记 unresolved，不复制外部正文。输出临时文件 fsync 后通过独占硬链接发布，拒绝覆盖既有输出。

该工具不解析许可证义务、不替组件选择双许可证、不把 NOTICE 等文件等同于许可证授权，不宣称所有原文已找到。组件是 metadata 解析候选，包含非当前目标及未编入载荷的包；compiled_component_mapping_verified 与 license_obligations_verified 均为 false，completeness 固定 incomplete。它不能用当前源码的解析图反推已安装 0.2.128 的分发清单。

本轮在候选 19e2461 基础上实际执行两份 locked/offline metadata：

- 主 workspace：556 个候选组件，457 个找到本地原文，99 个未找到。导出 tmp/analysis-current-license-text-evidence.json；独立解码核对 815 个原文文件的长度与 SHA256 全部一致，输出 SHA256 为 cfb9bec20d37a7c4b2270a9017b1ee2a014fbb8f72321c6d7439dfb659df9def。
- Tauri 独立 manifest：553 个候选组件，493 个找到本地原文，60 个未找到。导出 tmp/analysis-current-tauri-license-text-evidence.json。两个解析图有重叠，不相加计组件数量。

scripts/test-cargo-license-evidence.py 实际 exit 0，2 通过，验证非 UTF-8 原文字节和声明表达式保持、路径逃逸不复制、既有成果不覆盖及无效输入不发布。日志 tmp/analysis-license-export-tests.txt。本轮只修改 Python 工具/测试与文档，没有 Rust 生产变更。

后续仍须将最终正式构建的实际组件记录关联到载荷，再处理未定位原文、第三方非 Rust 组件、许可证选择与 NOTICE、安装包携带和最终交付绑定。此次原文证据在 tmp 私有中间目录，未把它们作为最终 NOTICE 发布；6.4 门禁继续开放。
