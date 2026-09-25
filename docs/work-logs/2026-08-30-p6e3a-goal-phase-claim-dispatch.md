# P6-E.3a：Goal phase claim 与 dispatch 工作记录

日期：2026-08-30
范围：只实现 Goal phase 的 schema、不可猜 claim、dispatch CAS 与失败回滚；不实现 E3b 执行/finalizer/stop，不改 Goal 执行循环、result recorder、chat、前端或真实 kill。

## 实现

- 在 v19 之后增加 v20 migration。`goal_phases` 幂等补齐 `active_run_id`、`claim_token`、`claim_owner`、`claimed_at`、`lease_until`，创建 `idx_goal_phases_active_run` 部分索引，并推进 `PRAGMA user_version=20`。已有 phase、runtime run 和 event 数据保持不变。
- 增加内部 `GoalPhaseClaim`。run id、claim token 通过 `getrandom` 生成，随机源失败直接 fail-closed；owner 只保存在内部 SQLite 状态。`GoalPhaseDto` 仅公开 additive 的 `active_run_id`，不泄露 token、owner 或 lease。
- `dispatch_ready_goal_phases` 在 self overlay 或 handoff 之前执行 Immediate 事务：创建 `goal_phase/accepted` run，使用 pending 且无 claim 的 CAS 将 phase 置为 running，写入 `run.accepted` 与既有 `goal-phase-dispatched` 事件；commit 成功后才广播并执行 handoff/overlay。CAS 失败只返回 skipped，不产生 handoff。
- handoff rejected、创建失败或 self overlay/后续 retry 写入失败时，以同一 run/token 的小事务将 accepted run 置为 failed，同时仅在 claim 匹配时把 phase 恢复为 pending 并清空五个 claim 列，写唯一 `run.failed`；事务失败整体回滚，不能清除其他 claim。
- re-plan、resume-from/resume 和手工 complete 遇到 active claim 返回稳定 409；无 claim 时保留原行为。旧 registry-only chat interrupt 测试补齐临时 DB 隔离，生产 fallback 语义未改变。

## 验证

- 定向 `goal_phase_`：15 passed。
- 完整 `cargo test -p coolzhu-web-console --offline`：872 passed、0 failed；doc-tests 0 failed，耗时 41.93 秒。
- `cargo build -p coolzhu-web-console --offline`：成功（仅已有 warning）。
- `git diff --check`：成功；仅有 Git 的 LF/CRLF 提示。
- `cargo fmt --check --package coolzhu-web-console`：非零。`main.rs` 中存在本轮之前及其他并行阶段的大量格式化漂移；本轮未对 1.36MB 巨型文件做整文件格式化，详情见 `tmp/qa-p6e3a-impl-2026-08-30/cargo-fmt-check.log`。

本轮未启动、停止或触碰 8765 服务，未提交或推送。

## 普通 resume 的 active claim 纠偏（2026-08-30）

- `resume_goal_sqlite` 改为 Immediate 事务内读取 Goal、检查 active claim，再决定幂等早退或执行 paused→planning/running/completed；因此 running+active claim 和 paused+active claim 都返回 409，不能绕过 claim。无 claim 且非 paused 仍保持 200 幂等返回。
- paused resume 的状态更新与 `goal-resumed` 事件在同一事务写入，提交后才广播；`resume_goal_from_phase_sqlite` 原本已先检查 active claim，无需改动。
- 新增 running/paused × active/no claim 四类回归测试，并验证 active claim 场景 runtime run、phase、event、handoff 不变。`run_db_secret_columns_not_populated` 不属于本次问题：claim_token/claim_owner 在 DB 内部必须存在，产品未改动，仅公开 DTO/SSE/Goal event 不泄露。

本轮验证：resume 定向测试 9 passed、0 failed；完整 crate 880 passed、0 failed；offline build 成功；`git diff --check` 成功。未启动、停止或触碰 8765 服务，未提交或推送。

## P1 纠偏（2026-08-30）

- 外部 phase dispatch 先安装精确的 goal/phase overlay，再创建 manual handoff；overlay 成功后 delivered 分支没有后续可失败步骤。rejected 分支只移除本 goal/phase 的精确 overlay，然后将匹配 claim 安全回滚；不会删除同一 goal 的其他 phase overlay。
- `create_manual_handoff` 返回错误时保留 accepted run 与 phase claim，记录有界的 `run.dispatch_in_doubt` 事件，不清 claim 放任重派；这是为了覆盖 inbound message 可能已经落库而 handoff insert 失败的边界。self retry context 改为 room/session 同步内存变更后只执行一次 SQLite save，第二步失败也保留 claim 并进入 in-doubt，避免孤儿执行指令可被重派。
- claim 的同一 Immediate 事务现在同时重查 Goal 状态、phase 的 `assigned_role + updated_at` 版本，并拒绝 paused/cancelled/completed Goal；命中后仅把 planning Goal 推进为 running，不覆盖其他 Goal 状态。CAS 失败不产生 run/event/handoff。
- 新增/补强故障与竞态测试：精确 overlay 回收、handoff 部分持久化 in-doubt、self 双写失败保留 claim，以及 Goal 取消和 phase 替换的事务内版本重查。

本轮验证：`goal_phase_` 19 passed；完整 `cargo test -p coolzhu-web-console --offline` 为 876 passed、0 failed；`cargo build -p coolzhu-web-console --offline` 成功；`git diff --check` 成功（仅 LF/CRLF 提示）。未重新格式化巨型 `main.rs`，`cargo fmt --check` 的既有非零结果及原因沿用上节记录。本轮未启动、停止或触碰 8765 服务，未提交或推送。
