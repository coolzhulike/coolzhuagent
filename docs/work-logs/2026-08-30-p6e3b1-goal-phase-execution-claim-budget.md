# P6-E.3b.1 Goal 阶段执行 claim、预算与固定 DB 路径

日期：2026-08-30

## 范围

本轮只实现 Goal phase execution claim、迭代预算原子占用以及固定 SQLite 路径保护；不进入 E3b.2 finalizer/stop/recovery，不调用外部模型，不修改前端、Goal cancel/loop、Queue 或 Approval。

## 实现

- `GoalPhaseRunContext` 仅在内部携带 `run_id`、`claim_token` 和捕获时的 `db_path`，不派生 serde，也不进入 DTO、SSE 或诊断日志。
- `prepare_goal_phase_run_context` 使用单一 `BEGIN IMMEDIATE` 重查 Goal、phase、Goal 聊天室、assigned session 与 active claim。E3a 已提交的 accepted run 只允许一次 accepted→running，并写唯一 `run.started`；legacy running/no-claim 路径在同一事务生成不可猜 run/token、写 accepted/started 与 claim。
- iteration reserve 位于上述事务的最后一个条件 UPDATE，要求 Goal 非 paused/cancelled/completed 且 `current_iteration < max_iterations`，成功只增加一次。
- legacy 预算失败整体回滚 run/claim/事件，再以短事务暂停 Goal 并写预算事件；已提交 accepted claim 的预算失败以相同 run/token 做严格 CAS，回滚到 failed、清理匹配 phase claim、暂停 Goal，并在同一提交中写 `run.failed` 与预算 Goal 事件。CAS 失败保留现场并 fail-closed。
- `run_goal_phase_once` 从入口捕获并传递固定 DB 路径；模型/工具 await 期间不持有 session store 锁；结果写回使用 `record_goal_phase_model_result_at_path`，路径漂移时在旧 record 前返回 409。
- HTTP Goal loop start 与 WeChat `/continue` 同样在 spawn 前捕获并 move 固定 `PathBuf`；后台 loop 只使用该路径，不在 workspace reload 窗口重新读取全局默认 DB。
- 在执行前增加聊天室缺失 fail-closed 检查，避免 run.started 或 iteration 写入后才发现上下文不可用。

## 自检证据

- `cargo test -p coolzhu-web-console --offline goal_phase_execution_ -- --nocapture`：7 项通过。
- `cargo test -p coolzhu-web-console --offline`（路径纠偏后）：主二进制 887/887 通过；lib 8/8、browser native host 1/1、doc-tests 0 项均通过。
- `cargo build -p coolzhu-web-console --offline`（路径纠偏后）：成功。
- `git diff --check`：成功；仅报告共享工作树既有的 CRLF 转换提示。
- `cargo fmt --check --package coolzhu-web-console`：返回非零。未执行整文件格式化；输出是共享工作树中 main.rs 的既有广泛 rustfmt 漂移，同时包含本轮新增测试的可机械格式化提示，生产行为未因该命令改变。

## 产物

- 源码 `modules/gui-web/packages/web-console/src/main.rs` SHA256（路径纠偏后）：`801BB6C57948C7FC053B072A41E4D771676A8D242061DCC616B349FC211A9353`
- 二进制 `target/debug/coolzhu-web-console.exe` SHA256（路径纠偏后）：`5E3FDA59818CB2DA511A47F0BF8F71A291F42F0DAA8A53679D2EDDD1448C08AC`
- 本轮未启动、停止或修改 8765 服务。
