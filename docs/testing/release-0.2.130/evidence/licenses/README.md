# 130 构建组件许可证后续复核

post-build-observation-gaps.json 使用 130 已归档的九份实际 Cargo JSON 回执，关联 129 已归档 metadata 的精确 package_id；所有回执均唯一成功结束且组件身份可关联。此处明确复用旧 metadata，不声称 130 构建时采集 metadata。许可证文件来自当前本地组件目录观察，不声称安装载荷或法律义务证明。

组件去重 433 个，其中 409 个找到本地许可证相关原文，24 个外部组件未找到原文，第一方缺项 0。相同身份的多入口证据一致；读出的原文逐文件长度和 SHA 校验通过。补齐 18 份第一方 LICENSE 的结果因此获得针对 130 实际构建观察的复核，不修改 129 的历史缺项记录。

导出脚本 tmp/analysis-integration-130-license-review.py 实际退出 0，各入口完整原文观察位于 tmp/analysis-130-license-observation-*.json。此归档只保存缺项和回执/metadata 摘要，完整原文统一归档、实际分发组件映射、NOTICE、义务与签名仍未完成，completeness 保持 incomplete。
