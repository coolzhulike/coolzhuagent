# 第一方许可证原文与打包冻结

0.2.129 实际构建观察的原文缺项包括 18 个本地组件；它们的 Cargo metadata 均声明 MIT，仓库根 LICENSE 为 MIT、署名 coolzhu agent contributors。本轮将根 LICENSE 的原始字节复制到这 18 个组件根，创建前拒绝覆盖既有文件，创建后逐一比较字节摘要。不通过父目录搜索给外部组件补造许可证。

config/package-manifest.json 新增必需资源 documentation.first-party-license，将根 LICENSE 复制到包根 LICENSE；根 LICENSE 同时进入 build_inputs.files 与 source_snapshot.roots，后续构建若改变它须按原冻结门禁拒绝发布。各组件原文由已声明 modules/packages 根覆盖。

验证：最终 test-package-manifest.ps1 实际 exit 0；根工作区 17 个受影响 crate 的 cargo build --offline -j1 实际 exit 0，日志 tmp/analysis-first-party-license-build.txt，3m17s，保留既有警告；独立 Tauri cargo build --offline -j1 --release 实际 exit 0，日志 tmp/analysis-first-party-license-tauri-build.txt。原文导出复查中 web-console 的 211 个观察组件均找到本地原文，Tauri 的 304 个观察组件中 281 个找到。复查使用旧构建回执定位组件、读取修补后的原文，不能冒充新候选正式构建证据。

该变更未进入 0.2.129，不能改写它的历史 42 项缺失清单。下一候选还须正常出包、核对包根 LICENSE 字节及新回执。外部组件原文缺项、完整 NOTICE、实际分发映射、许可证义务、签名与正式运行验收继续开放。
