# 2026-10-10 构建 stderr 分类逐条读取

生产修改：Test-CargoDependencyUnavailableLog 使用严格 UTF-8、4Mi 字符单行上限逐条读取 stderr，沿用既有依赖缺失模式。即使先命中模式，也继续检查尾部；损坏编码或超长行明确抛错，不把不完整日志冒充完整分类。package-all 不再以 Get-Content -Raw 分类构建失败。

PS5.1/7 专项脚本各验证 5 个分类和 2 个损坏尾部拒绝，真实退出 0。现有 test-cargo-build-evidence.ps1 在 PS5.1/7 均实际退出 0，日志 tmp/analysis-stderr-change-evidence-regression*.txt。整包 Loader 契约回归已启动，日志 tmp/analysis-stderr-stream-package-regression.txt，终态待确认。

本轮未修改 Rust。进程 stdio 仍 ReadToEndAsync/ReadToEnd，完整采集字符串、增量原始日志、磁盘错误停止自有子树与双泵收尾仍未实现；不得将分类修补解释为全链内存边界通过。全部 32WBS 正式验收与交付保持未闭环。

补充专项覆盖：逐行生成 12000 条约 1KiB 诊断，依赖错误在最后一行；分类函数缺失路径拒绝。PS5.1/7 各实际退出 0，当前专项共 9 项，日志 tmp/analysis-stderr-stream-focused-ps5.txt、tmp/analysis-stderr-stream-focused-ps7.txt。该大日志是合成夹具，不是实际 Cargo 运行或峰值内存测量。调用端仍仅对存在 stderr 路径分类，此检查不证明整包入口缺失日志门禁。

整包 Loader 契约回归终态实际退出 0，39 通过、0 失败、4 未执行。4 项仍是 Runtime 分发、DLL 删除、全新 target 构建、清洁 Windows 原生窗口，未视为通过。生产 stderr 分类异常会阻止本轮成功，不证明 stdout/stderr 流式采集已完成。
