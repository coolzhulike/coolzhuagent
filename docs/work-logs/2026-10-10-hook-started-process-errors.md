# Hook 已启动进程的结果回收失败

运行时原先把 managed_process 所有非 Interrupted 错误解释为 Hook 启动失败并警告放行，但该错误也可能发生在进程启动后的 stdin 请求交付、输出读取或等待阶段。此时没有完整协议结果，不能据此继续工具派发。

候选将成功 spawn 后的错误包装为携带原始 source 和 ErrorKind 的 StartedProcessError，并提供类型检查接口；Hook 对该类错误拒绝派发。真正 spawn 失败仍沿用既有警告策略，取消/截止仍拒绝。不改变正常退出码 0/2/其它的协议。其它调用者继续收到 io::Error，不将失败结果转换为成功。

真实 Windows 用例：子进程立即退出而请求为 8 MiB，管道交付失败明确记录已启动；不存在的可执行文件则明确未启动。直接 Hook 执行同一不完整请求时得到 Deny，不能 Warn/Allow。该用例独立设置 stdin 负载与短环境变量，避免 Windows 环境块过大使测试实际停在 spawn 前。

生产构建实际 exit 0，日志 tmp/analysis-hook-started-error-build.txt。第一轮完整回归 365 通过/0 失败，随后加入 Hook 用例后的完整回归 365 通过/1 失败/1 忽略；失败为 prompt::tests::load_system_prompt_reads_claw_files_and_config 清理临时目录时 Windows code 32 文件占用，日志 tmp/analysis-hook-started-error-final-tests.txt 保留。串行完整复跑正在执行，未结束前不计通过。

串行完整复跑实际 exit 0：366 通过、0 失败，库 1 项 MCP 与两个 DSH 集成测试忽略，共 3 项忽略不计通过。日志 tmp/analysis-hook-started-error-serial-tests.txt。并发文件占用失败仍保留，串行通过不等于该测试并发稳定性已解决。

此改动没有实现输出容量限制、完整内容 spill、诊断全局配额或正式外部模型验收。容量限制必须继续基于完整结果/显式超限契约实现，不能静默截断。本轮不修改实际用户配置或共享后台进程。
