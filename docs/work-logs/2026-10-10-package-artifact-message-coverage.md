# 正式 manifest 的 Cargo 构建消息覆盖

原当前 config/package-manifest.json 只有 Tauri shell 配置 cargo-json-messages，用于 WebView2 Loader 生产者定位。其它 8 个 Cargo 分发入口不保存同类机器可读回执，无法从各入口实际构建记录整理组件证据。

本轮为当前正式 manifest 的全部 9 个 Cargo 入口配置唯一 --message-format=json-render-diagnostics 与 capture=cargo-json-messages，沿用 package-all 现有捕获支路。覆盖 Web console、Browser native host、clawbot sidecar、CLI、Computer Use preflight、两个 vision 入口、Tauri shell、安装启动器。没有改历史候选 manifest、产物来源、目标路径、发布版本或已安装包。

test-package-manifest.ps1 新增遍历校验，缺捕获设置、缺格式参数、重复或错误格式均拒绝。初次未配置 manifest 时校验如实失败，日志 tmp/analysis-all-artifact-capture-manifest-tests.txt；配置完成后实际通过，日志 tmp/analysis-all-artifact-capture-manifest-final-tests.txt。构建身份/冻结/载荷/报告治理回归实际 exit 0，80 cases、0 failed，日志 tmp/analysis-artifact-capture-build-identity-tests.txt。

启动器真实 locked/offline -j1 JSON 生产构建实际 exit 0，耗时 1.55 秒：tmp/analysis-launcher-capture-build-messages.jsonl 及对应 stderr.log。许可证工具消费该成功终态与当前 metadata，导出 13 个构建观察组件，11 个找到原文，completeness 仍 incomplete；输出 tmp/analysis-launcher-built-license-evidence.json。该命令使用独立候选 debug target，不是全量 release 打包或安装。

仍需在最终正式构建中收齐每个入口的实际消息，并将原始回执哈希、归档和组件原文绑定到同一包报告。现有 package-all 的通用 artifactBuildInputs 主要由 Loader 导出消费，不能仅凭扩大 manifest 配置就声称全部回执已在成功报告归档，也不能将 build/proc-macro/缓存记录当成运行时分发组件。完整组件映射、非 Rust 依赖、最终 NOTICE 与正式安装验收继续开放。
