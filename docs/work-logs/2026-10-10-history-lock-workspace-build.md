# 2026-10-10 历史锁修改后 workspace 集成构建

核验源码 7e272df818a226d93d50c726521cb82602662c61，工作树开始时干净。cargo build --workspace --offline -j1 实际退出 0，1 分 01 秒；串行工具会话 99018 终态 0。CARGO_BUILD_JOBS=1，目标 tmp/cargo-target-integration；完整日志 tmp/analysis-history-lock-latest-workspace-build.txt。

覆盖当前 workspace 成员，包括主控制台、CLI、运行时、工具注册、桌面相关成员；不将它宣称为所有 feature、独立 Tauri release manifest、正式冻结出包或真实 GUI 验收。主控制台仍报告 151 条警告，日志保留，不冒称无警告。

下一采集实现约束再次从现有代码确认：windows-process-guard 的 ChildProcessJob::spawn_managed 与 spawn_managed_async 使用挂起创建、绑定 Job 后恢复执行。package-all 仍直接 Process.Start，ReadToEndAsync/ReadToEnd，无法仅靠 CopyToAsync 补全子树所有权。新采集需在执行前确立同等边界，并保留日志独占、原始字节、双泵增量可见性、写盘故障停止自有子树和收尾证据，不能事后绑定 Job 宣称无逃逸。该处尚未修改生产采集。

本轮补全公开锁接口后的整 workspace 编译证据，32WBS 正式运行与交付门禁仍未闭环。
