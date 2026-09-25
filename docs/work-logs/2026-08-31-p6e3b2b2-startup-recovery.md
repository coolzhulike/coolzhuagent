# P6-E.3b.2b.2 启动恢复工作日志

日期：2026-08-31

## 实现

- 扩展 `recover_incomplete_runtime_runs`：数据库不存在时保持严格 no-op；启动恢复使用单笔 `TransactionBehavior::Immediate` 事务。
- 初始查询一次性读取全部 `accepted/running/stop_requested` runtime 的 id、state、kind、workspace、Goal/phase、claim token；每条 active run 以 CAS 收敛为 `orphaned`，写入 `process_restart` 与 `finished_at`，每条恰好追加一个 `run.orphaned`。
- `goal_phase` 仅在 Goal workspace/id、phase Goal/id、`active_run_id` 与 claim token 全部精确匹配时清理五列 durable claim；phase 更新带 `status='running'` 与 run/token ownership CAS。
- Goal 状态收敛：cancelled→phase cancelled/清 route；completed、paused→phase pending/manual_reconcile；其它非终态→phase pending/manual_reconcile 且 Goal paused；不覆盖 terminal Goal。
- 匹配 claim 追加既有 `run-interrupted-needs-review`，payload caller 为 `startup-recovery`，并带 reason/process_restart、run/phase/previous_state/goal_status；不输出 claim token/owner/lease/error_json。
- runtime/Goal/phase 任一步失败都回滚整批，commit 后才广播；未增加 loop/chat/cancellation/model 自动恢复。

## 测试与验证

- 新增恢复定向测试覆盖 A–H；恢复相关 8/8，启动 fail-closed 断言 1/1。
- 追加 terminal/non-running phase 残留 matching claim 回归：只孤儿化 runtime、保留 phase/Goal 现场并继续同批其它 active run。
- 默认 crate 全量 914/914；`module_linkage_smoke` 4/4；offline build 通过；`git diff --check` 通过。

## 哈希

- 最终 `main.rs`：`A552792B527D18A5B8E6815E1FE461485A8FD47C0B340308A4644CFC889D882C`
- 最终 `target/debug/coolzhu-web-console.exe`：`44E36C3693F9171D98C5DD592C35359BCBAADF3A209754099B41C26888E7D854`
