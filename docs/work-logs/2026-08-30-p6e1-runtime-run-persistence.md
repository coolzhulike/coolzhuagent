# P6-E.1 runtime run 持久化实现记录

日期：2026-08-30

## 本阶段范围

本阶段只处理 runtime run 的后端持久化与查询/中断契约，不接入 chat stream，不修改现有 chat interrupt、goal phases/Goal 执行或前端，也不实现真实进程 kill。

## 实现摘要

- 将 session SQLite schema 正式推进到 `user_version=19`，增加 `runtime_runs` 与 append-only `runtime_run_events`，并建立 runtime run 查询、legacy turn、active goal phase/loop、事件游标索引。迁移使用 `IF NOT EXISTS`，可修复缺失的 v19 表/索引且保持既有业务行。
- 增加 runtime run DTO、SQLite helper 与独立 broadcast bus。事件写入 helper 分为事务内 deferred append 与提交后广播两层，只有事务成功 commit 后才向 SSE 订阅者广播。
- 进程启动时在 scheduler/bind/serve 前恢复已有数据库中 `accepted`、`running`、`stop_requested` 行为 `orphaned`，每行只追加一次 `run.orphaned`；数据库不存在时 no-op，已有数据库恢复或事件追加失败则 fail-closed。
- 新增 `GET /api/runs/{run_id}`、`GET /api/runs/{run_id}/events?after=` 与 `POST /api/runs/{run_id}/interrupt`。SSE 支持 query/header cursor、真实事件 numeric `id`、`Last-Event-ID` 与 broadcast `Lagged` 后补库；interrupt 使用 `BEGIN IMMEDIATE` + CAS，并对重复请求保持幂等且只产生一个 `run.stop_requested`。

## 验证记录

- `cargo check -p coolzhu-web-console --offline`：通过。
- `cargo test -p coolzhu-web-console runtime_run_ --offline`：6 passed，0 failed。
- 收口修正两个陈旧静态契约后，`chat_stream_started_and_interrupted_done_include_turn_id` 与 `web_realtime_session_exposes_tts_health_for_task_card` 定向测试均通过。
- `cargo test -p coolzhu-web-console --offline`（收口复跑）：861 passed，0 failed；P6-E.1 新增的 6 个测试均通过。
- `cargo build -p coolzhu-web-console --offline`：通过。
- `git diff --check`：通过；仅报告现有工作树文件的 LF→CRLF 提示。
- `cargo fmt --check --package coolzhu-web-console`：返回非零并输出大量已有 `main.rs` 格式差异。本阶段没有运行会改写整文件的 formatter，因此没有产生非 P6-E.1 格式化漂移；新增代码按局部上下文保持格式。

## 边界与运行环境

未启动或停止 8765 服务；只确认孤立测试端口 18778/PID 16300 不存在。未提交、未推送。浏览器视觉验收不属于本阶段。
