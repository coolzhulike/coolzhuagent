# 2026-10-10 Loader 原始回执逐条消费（进行中）

本轮在独立集成工作区继续推进打包回执内存缺口。Loader 支持从原始 JSONL 逐条读取，正常整包构建不再保留完整消息行数组；旧数组接口仅保留给没有原始路径的调用。提供路径却无法读取时直接失败，不静默回退。首行兼容 UTF-8 BOM。

首次完整契约测试实际退出 1，10 项失败：整包入口仍以旧消息数组为空判定 NOT-BUILT。已改为同时检查原始回执路径，保留首次失败日志 tmp/analysis-loader-stream-contract-tests.txt。修复后完整测试仍在运行，日志 tmp/analysis-loader-stream-contract-tests-repair.txt；尚不能计为通过。

针对已归档 0.2.130 真实 Tauri JSONL，PS5.1 新旧接口提取结果一致（1 个生产者），实际退出 0。另验证加 BOM 后一致、缺失原始文件明确拒绝且不回退数组，实际退出 0。此证据不是新版正式打包验收。

剩余：等待完整契约测试终态并处理失败；进程采集仍使用 ReadToEndAsync 保存整体字符串，尚未完成流式 stdout/stderr、写盘失败停止自有子进程树及双管道收尾。清洁目标构建、清洁 Windows 原生窗口与全部 32WBS 正式门禁仍未闭环。

后续核对：修复入口判断后的完整测试实际退出 1，剩 2 项失败，日志 analysis-loader-stream-contract-tests-repair.txt。两项都是旧成功夹具不满足已提交的完整回执要求：组件缺 target、junction 回执缺组件及 build-finished。已补齐合成夹具，不降低生产校验。再次完整测试运行中，日志 tmp/analysis-loader-stream-contract-tests-complete-fixtures.txt。

新增 scripts/test-loader-raw-record-stream.ps1，覆盖无 BOM、有 BOM、缺失文件、非法 UTF-8、超过 4Mi 字符单行，共 5 项；PS5.1 和 PS7 均输出全部通过。失败例同时传入有效旧数组，验证没有静默回退。

再次完整测试实际退出 1：38 通过、1 失败、4 未执行。报告代次绑定已通过；junction 重写首行后无尾换行，新增组件记录连接到首行，形成非法 JSON。已在追加前补换行并再次运行同一完整套件，日志 tmp/analysis-loader-stream-contract-tests-fixture-newline.txt；终态仍待确认。

当前重跑并发 copy 检查记录 unlocked_reader_saw_missing=2。现有用例明确仅保证持发布权消费和最终完整代次，未加锁存在性观察不作为失败判据（测试源码约 948 行说明）；不能将该项通过解释为未加锁读者从未看到路径缺失。此边界保留，不改验收口径。

最终本轮完整契约测试实际退出 0：39 通过、0 失败、4 未执行。日志 tmp/analysis-loader-stream-contract-tests-fixture-newline.txt。4 未执行仍为 Runtime 分发变更、DLL 删除审计、全新 target 离线构建、清洁 Windows 原生窗口，均不计通过。本轮仅 PowerShell 与测试/文档修改，没有 Rust 修改，不触发 Rust 重新编译；此结果不证明新版正式安装或全部 32WBS 完成。
