# P6-E.3b.2b.1 Goal 停止链路

日期：2026-08-31
范围：统一 Goal 停止链路；未进入 startup recovery、Queue/Approval、provider/tool kill、模型供应商/thinking。

## 实现摘要

- 新增事务内批量 Goal-phase stop CAS：按 workspace+goal+kind/state 只允许 accepted/running → stop_requested；每个实际命中 run 在同一事务追加一次 `run.stop_requested`，重复调用幂等，事件不含 claim token、owner、lease、error_json。
- cancel/pause 将 Goal 状态、mutation event、active goal-phase stop request/event 放入同一 `BEGIN IMMEDIATE`；commit 后才广播 runtime/Goal event，并在 commit 后触发 workspace-scoped loop AtomicBool。终态重复 mutation 不重复写 Goal mutation event，但仍扫描异常残留 active run。
- goal-loop stop 先持久化 active phase stop，再按 workspace+goal 比对并触发内存 loop；不创建 goal_loop runtime row。clawbot stop 复用 pause commit 后信号，不再提前设置内存 stop。
- direct run interrupt 在 stop CAS commit 后读取 run 的 workspace/goal，仅唤醒对应 Goal loop；chat run 仍走原 ChatTurn cancellation 路径。后台 loop 将 stop/stale conflict 视为停止，不重试 stop finalizer 409。
- prepare 在模型入口前收敛自有 accepted+stop_requested 且尚未 started 的 run：以 token/scope 清理 phase claim，cancelled Goal → phase cancelled，其它 → pending/manual_reconcile；写唯一 `run.interrupted` 与 `run-interrupted-needs-review`，commit 后广播并返回 409。running stop_requested 继续由结果 finalizer 处理。
- assignment 的 `assigned_session_id` 是 role 配置派生字段而非物理列；prepare 在同一事务 fresh DTO 上补 session scope 校验，错误 session 保留 accepted claim 现场。

## 回归

- running cancel/pause + 迟到 finalizer：无结果消息，分别收敛 cancelled / pending+manual_reconcile。
- accepted cancel/pause 在 prepare 前收敛；claim 五列清理且不进入模型；wrong-session 保留现场。
- loop stop 持久化、幂等无重复 stop event、跨 workspace 同 goal id 不误唤醒、无 goal_loop runtime row。
- direct Goal interrupt 精确唤醒，chat interrupt 仍触发 ChatTurn cancellation。
- stop event append 故障时 Goal/run/event 全部 rollback；公开 event/status 不泄漏 ownership 字段。

## 验证记录

- `cargo check -p coolzhu-web-console --offline`：通过。
- 新增 Goal stop 定向测试：通过（A/B/C/D/E/F/G 与 scope mismatch）。
- `cargo test -p coolzhu-web-console --offline`：907 passed，0 failed。
- `cargo build -p coolzhu-web-console --offline`：通过；`cargo test --test module_linkage_smoke --offline`：4 passed，0 failed。
- `git diff --check`：通过；完整证据与最终 hash 见 `tmp/qa-p6e3b2b1-impl-2026-08-31/README.md`。

## 并发竞态校正

- direct Goal interrupt 测试与 ChatTurn terminal-failure 测试补持同一 `config_test_guard`，避免清空进程级 `chat_turn_registry` 时与其它测试并行竞争。
- 全部 `clear_chat_turn_registry_for_test` 调用已逐函数核对，均处于 guard 作用域；未改生产代码，未使用单线程测试规避问题。
- 校正后默认并行 full crate 连续两轮均为 907 passed、0 failed；linkage 4 passed、0 failed；offline build 与 diff check 通过。
- 校正后 hash：`main.rs`=`BA402B9422AC821F94888A6F5E6C2C570093B6DCD559B0E1898F0CD0DE42F9C3`，exe=`2E47D3BC58E4EA1B15413AA2BE62C8CAD5BFE156691568C071A23585566EFCB0`。
