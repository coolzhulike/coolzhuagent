# HA12-02 在途命令/未知效果演练（真实事件记录）

时间：2026-10-23 约 04:40–05:15（UTC-7）。

事件：两台隔离 web-console（8766 baseline / 8767 candidate）在 B08 运行期间同时失去 HTTP 响应。

处置顺序（先核对、后动手）：
1. 进程盘点：`Get-CimInstance Win32_Process` 确认服务进程存活（8766→PID 39344、8767→PID 51936）、mock provider（python）存活、另有无关 python/powershell 进程——未盲目终止任何进程。
2. API 探测：`/api/chat/rooms`、根路径 8s 超时无响应，判定为服务端死锁而非慢。
3. 检查 sqlite writer lock（`web-sessions.sqlite3.writer.lock` 0 字节、10/11 遗留——排除为根因）。
4. 检查在途脚本：`run-b08b.ps1` 子进程仍在跑但所有 API 调用挂起 → 判定其为受害者而非原因。
5. 检查后台 `cargo test` 编译占用 → rustc 卡死与构建无关，但构成环境噪声。
6. 仅终止确认僵死的四个目标进程（两服务 + b08b 脚本两进程），无关进程一律不动；服务端重启后经 API 探活确认。

未知效果盘点：b08b 腿 A 的 run1/2 已发出（goal_A 创建、gate fail/repair write_file 均有真实回执）；断点后 goal 状态以 sqlite 为准，重启后从 PhaseInfo 重新读取，不假设、不重跑已确认的效果。

另：rustc 今日全部出现启动期冻结（0 I/O、零 CPU、数分钟后放行）——记录为环境噪声，不因"看起来挂了"盲目 kill 正在推进的实例（通过 CPU/IO 采样区分真死锁与 AV 延迟）。

结论：先核对实际进程与记录、再决定动作；不盲目重跑副作用、不全局杀进程。演练 PASS。
