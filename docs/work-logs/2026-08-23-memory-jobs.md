# 2026-08-23 记忆生命周期作业状态

## 本轮目标

继续 P1-1：把 extraction、edges、consolidation 从同步按钮动作提升为可追踪、可重试的会话级作业，并在记忆窗口展示运行状态。

## 实现

- `modules/gui-web/packages/web-console/src/main.rs`
  - 新增 `memory_jobs` SQLite 侧表（v18），记录作业类型、排队/执行/完成时间、进度、结果数量和错误。
  - 新增 `GET/POST /api/sessions/{session_id}/memory/jobs` 与 `GET /api/sessions/{session_id}/memory/jobs/{job_id}`。
  - 作业先持久化为 `queued`，再异步执行；`queued/running` 请求会复用现有作业，`retry=true` 可重新排队。
  - extraction 从最近会话消息生成幂等的 L2 conversation bead；edges/consolidation 复用现有实现，统一回写 succeeded/failed 状态。
  - 删除会话时清理 `memory_jobs`，新增生命周期持久化单测。
- `modules/gui-web/packages/web-console/index.html`、`src/app.js`
  - 记忆窗口新增 Memory lifecycle 作业区，支持提取、关联图、巩固、刷新、失败重试和执行中轮询。
- `src/styles.css`
  - 修复新增作业区被压缩的问题：记忆面板实际包含筛选、摘要、作业、内容四行，网格显式声明四行，避免按钮/列表不可见。

## 隔离工作区实机/API 验证

在 `tmp/memory-jobs-runtime-20260823`、`127.0.0.1:8801` 启动新编译服务：

1. extraction 返回并完成 `succeeded`，`result_count=1`。
2. edges 与 consolidation 均完成 `succeeded`，`result_count=0`。
3. 重启服务后作业列表仍从 SQLite 返回，确认跨进程持久化。
4. 浏览器打开记忆窗口，三类作业均显示“已完成 · 100%”；点击“提取”后出现“执行中 · 10%”，随后回到完成状态，确认前端启动、轮询和列表刷新链路。
5. 截图证据：`tmp/memory-jobs-ui-evidence-fixed.png`、`tmp/memory-jobs-ui-round-evidence.png`。

## 本地验证

- `cargo fmt --package coolzhu-web-console`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `cargo test -p coolzhu-web-console --offline memory_job_lifecycle_is_persistent_and_retryable -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline web_frontend_memory_window_connects_summary_prompt_context_and_governance -- --test-threads=1`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过；仅保留仓库既有 warning。
- `git diff --check`：通过。

## 后续优先级

继续补 typed compaction item、turns/items 分页读取，再接 resume/fork/rollback 的统一历史协议；每轮保持本地编译、单测、接口和界面截图验证。
