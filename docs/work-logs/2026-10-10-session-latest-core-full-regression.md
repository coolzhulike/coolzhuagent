# 2026-10-10 最新 Session 源码完整运行时回归

核验源码 23a4a5a80a7259a2872b3e3422dde075470b6ed3，包含 8b49b3e 的同目录暂存会话保存及后续 CLI/回执修改。串行工具会话 39932 最终实际退出 0。

cargo build -p coolzhu-core-runtime --offline -j1 退出 0；cargo test -p coolzhu-core-runtime --offline -j1 退出 0。库测试 375 通过、0 失败、1 忽略；两个 DSH 集成测试各 1 忽略；Doc-tests 0。日志 tmp/analysis-session-latest-core-full-build.txt 与 tmp/analysis-session-latest-core-full-tests.txt。复用独立 tmp/cargo-target-integration，CARGO_BUILD_JOBS=1，未启动并行 Cargo。

未执行：官方 MCP SDK roundtrip 需要 MCP_OFFICIAL_SERVER_ENTRY；DSH deadline 需要已核验固定资源；官方 DSH deadline 需要既有官方来源/回执与独立输出目录。三项均不算通过。现有 unused import/dead code 警告未作为通过所消除的问题。

本轮补齐 Session 修改后完整 core-runtime 回归证据，不证明跨进程 CAS、Windows 断电持久性、崩溃消息/事件事务、真实模型取消或已安装 0.2.130 恢复。32WBS 继续进行。
