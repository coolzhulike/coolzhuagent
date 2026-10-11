# 诊断生产者容量边界审查

以下是独立集成工作区源码审查，不能替代正式运行验收或声称全局磁盘配额已完成。

| 实际入口 | 当前容量路径 | 判断 |
| --- | --- | --- |
| diagnostics::FileOutput | rolling_file，8 MiB 活动文件及三代，64 KiB 单记录 | 单文件边界已有实现；文件数量与目录总量无全局约束 |
| Web diag_log / err.log | append_diagnostic_line，脱敏后轮转 | 已接入同一单文件边界 |
| Web log_session_lock_line | 本轮由直接追加改为 append_diagnostic_line | 候选接线；正式安装版尚不包含 |
| 启动器 Web/Tauri stdout/stderr | File::create 后直接传入 Stdio | 每次启动截断，单次长程仍无界；诊断模块无法控制子进程持有的句柄 |
| core-runtime managed_process::read_pipe | 候选双管道分块读取，每根 8 MiB 上限；超限停止子树并返回错误，收齐线程 | 已有超限失败保护；完整内容留存/引用和全局容量治理尚缺，不能当作完整输出交付 |
| Web spawn_detached_logged / local-gemma.log | 追加文件并复制 stdout/stderr 句柄 | 跨启动及长程均可能增长；不能直接给该文件做并发改名轮转 |
| Web attach_safe_window_logs | 每次 File::create 后传入 Stdio | 单次输出无界，文件数量未统一治理 |
| tool-audit / computer-use-audit / SQLite | 领域事实留存 | 权威审计，禁止混入普通诊断淘汰器 |

会话锁诊断修补只改变诊断写入：原路径、原消息、忽略诊断失败的业务行为保持；新增脱敏、单记录上限、文件轮转和 try_lock 竞争立即返回。现有超大历史文件仍完整轮转保留，不能称目录硬配额已经满足。

后续全局配额需要管理明确登记的诊断生产者集合与目录，不能扫描工作区删除所有 log/jsonl，更不能依据文件后缀清理权威审计。子进程输出必须先通过受监督的有界采集路径，处理管道持续排空、取消/退出、断流、磁盘故障及 Windows 持有句柄。跨目录总量、生产者注册并发、保留优先级、历史导出脱敏仍未闭环。

在候选 598adb56067c8b9e1e755c6c14c6130da6c859f1 复查：packages/app-launcher/src/main.rs 的 main 在 Started 或 ForwardedExisting 后立即返回成功退出，因此只增加启动器后台采集线程不能覆盖长期子进程生命周期。需要持续存活的监督者或生产者自身控制输出；不能将短命线程实现计为长期日志有界。managed_process::read_pipe 的容量治理则需明确超限结果和完整内容保存/引用协议，并核查调用者如何处理采集失败，避免将 Hook 输出丢失转换成放行。

检索中间证据：tmp/analysis-diagnostic-producer-search.txt。本审查不操作真实模型、云端实例、微信或现有进程。

候选验证：cargo build -p coolzhu-web-console --offline -j1 实际 exit 0，耗时 1 分 03 秒；cargo test -p coolzhu-diagnostics --offline -j1 实际 exit 0，27 passed/0 failed，覆盖脱敏、整条记录轮转、超长记录以及独立 OS 锁竞争。日志分别为 tmp/analysis-session-lock-diagnostic-build.txt 与 tmp/analysis-session-lock-diagnostic-tests.txt。这些检查证明共享接口及生产编译接线，不证明真实会话等待时的正式运行或完整全局配额。
