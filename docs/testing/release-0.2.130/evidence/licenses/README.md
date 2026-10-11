# 130 构建组件许可证后续复核

post-build-observation-gaps.json 使用 130 已归档的九份实际 Cargo JSON 回执，关联 129 已归档 metadata 的精确 package_id；所有回执均唯一成功结束且组件身份可关联。此处明确复用旧 metadata，不声称 130 构建时采集 metadata。许可证文件来自当前本地组件目录观察，不声称安装载荷或法律义务证明。

组件去重 433 个，其中 409 个找到本地许可证相关原文，24 个外部组件未找到原文，第一方缺项 0。相同身份的多入口证据一致；读出的原文逐文件长度和 SHA 校验通过。补齐 18 份第一方 LICENSE 的结果因此获得针对 130 实际构建观察的复核，不修改 129 的历史缺项记录。

导出脚本 tmp/analysis-integration-130-license-review.py 实际退出 0，各入口完整原文观察位于 tmp/analysis-130-license-observation-*.json。后续 post-build-license-text-observations.json 已统一去重归档原文：433 个组件、694 份组件内文件，保留九入口观察与回执/metadata 摘要。归档大小 4179994 字节，SHA-256 为 e64dc2ffbf0e3cda4826f6ba2c67d1b3f55ee52ab3cbf3250d590deeae55107e。

归档脚本 tmp/analysis-archive-130-license-observations.py 实际退出 0，并从最终归档重新解码 UTF-8/base64 原文、逐文件复核长度和 SHA；不依赖临时导出或 Cargo 源目录完成该重读检查。相同组件跨入口原文证据一致，归档 JSON 不转换换行。实际分发组件映射、NOTICE、义务与签名仍未完成，completeness 保持 incomplete。
