# P6-E.3b.2a Token-aware Goal phase finalizer

日期：2026-08-30（收尾复核：2026-08-31）
范围：只实现 E3b.2a；未进入 E3b.2b，也未启动 8765。

## 实现摘要

- `run_goal_phase_once` 把 captured `db_path`、`run_id`、`claim_token`、`goal_id`、`phase_id` 交给 token-aware finalizer；旧无 token 入口在发现 `active_run_id` 后 fail-closed 返回 409，legacy no-claim fixture 仍保留。
- finalizer 在 captured DB 开 `BEGIN IMMEDIATE`。正常结果先做 phase active pointer 与 runtime scope/token/state ownership CAS；stale token、错 scope、已终态均在消息、verdict、context、memory、handoff 前退出，返回 409 且无业务副作用。
- room/session 消息和 attachment refs 由同一事务 helper 写入，parent 缺失会回滚；pass 同事务写消息、`goal-phase-verdict(pass)`、`phase-completed`、pass evidence、phase completed 与五列 claim 清理、Goal 完成判定、runtime completed 和 deferred events。
- retry、missing artifact、command gate failure、implementer blocked 同事务写消息、verdict/evidence、route/retry、五列 claim 清理、runtime failed 和 Goal events；running retry 清 claim，下一次通过 `prepare_goal_phase_run_context` 的 legacy no-claim branch 生成新 run。
- stop-first：`stop_requested` 先提交时仅收敛为 interrupted、清自有 claim、必要时 Goal paused/manual_reconcile；不产生结果 message/verdict/completed/memory/handoff。结果先提交后 interrupt 返回 `already_finished`，不追加 interrupted/review event。
- completed Goal 的 stop-first 也只清自有 claim：`cancelled` phase 收敛为 `cancelled`，其它 phase（含 completed Goal 下的 running phase）收敛为 `pending/manual_reconcile`，Goal 仍保持 `completed`，不产生结果 message/verdict/memory。
- path drift 只在 captured 旧 DB 以自有 run/token 做 interrupted cleanup；不创建/写入新 DB，不写结果消息、memory；错 token 保留旧库现场。
- commit 后才广播 runtime/Goal events；same-path 只镜像 room/session 内存消息，不做全量 `save()`；成功 pass 后同路径移除内存 goal-task skill overlay，持久 overlay 在完成事务内清理。context lifecycle/auto-memory 仅由成功结果在提交后执行。
- DTO/SSE 公开结构不包含 `claim_token`、`claim_owner`、lease 字段或 `error_json`。

## human-ack 证据

现有 gate 保持不变：`review_goal_commander` 在依赖满足但 `requires_human_ack` 未批准时返回 `awaiting_human_ack`；`dispatch_ready_goal_phases` 先条件写 `human_ack='awaiting'` 并暂停 Goal，approve 后才允许 dispatch。prepare 只把已 accepted 的自有 run 从 accepted CAS 到 running；本轮未新增 cancel/pause/loop/direct-interrupt plumbing。

## 验证

- `cargo test -p coolzhu-web-console --offline goal_phase_`：37 passed。
- token-aware finalizer 集：10 passed（含 completed Goal stop-first、旧入口 active-claim guard 测试）。
- `cargo test -p coolzhu-web-console --offline`：899 passed，doc-tests 0 passed。
- `cargo build -p coolzhu-web-console --offline`：通过。
- `git diff --check`：通过；仅有 Git 的 LF/CRLF 提示，无 whitespace error。

窄回归覆盖：pass 原子（含旧 verdict event、overlay cleanup、公开 DTO）、result-first→interrupt、retry→新 run、implementer blocked→planner/paused→超限 blocked、missing/command failure、stale token 零副作用、旧无 token active claim、stop-first、path drift 正确 token cleanup、path drift 错 token 零变更/新库不创建、事务故障 rollback。

## SHA-256

```text
modules/gui-web/packages/web-console/src/main.rs
6E3BB43DB71472E4595D4D6ECACAA8DA8F2E878CF9FFB36039F6596F1E983810

target/debug/coolzhu-web-console.exe
71A6B9A93274A11CA6E92832DFB6B57641FB13794005AD5E0EF626613B40C2CB
```
