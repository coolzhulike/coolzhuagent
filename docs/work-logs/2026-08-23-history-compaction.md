# 2026-08-23 历史 turns/items 与 typed compaction item

## 本轮目标

继续 P1-1：在已有 session messages/beads 兼容存储上提供 Codex harness 风格的 thread → turn → item 历史投影，并把自动上下文压缩摘要从普通 conversation bead 区分为 typed `compaction` item。

## 实现

- `modules/gui-web/packages/web-console/src/main.rs`
  - 新增 `GET /api/sessions/{session_id}/history?limit=&before=`，按用户消息边界聚合 turn，返回稳定 turn cursor、分页 item、`has_more/next_before`。
  - session message 映射为 `user_message`、`assistant_message`、`tool_call`、`tool_result`、`system`；`context:auto-compact` 独立映射为 `compaction` turn/item，不与原始消息混淆。
  - `ContextAssembly` 增加 `compaction_item`，包含摘要 id、来源、消息数、token 数、预览/持久化状态。
  - 自动压缩持久化 bead 的 kind 改为 `compaction`；core-runtime 将其固定归入 L2，并加入价值权重。
  - 新增历史分页、压缩 item 与前端合同断言。
- `modules/gui-web/packages/web-console/index.html`、`src/app.js`、`src/styles.css`
  - 记忆窗口增加 Thread history 区域，显示 turn/item 数、压缩 item 类型及刷新入口。
  - Context Preview 显示 `compaction_item` 状态；历史和作业区域均显式占用独立 grid 行，避免缩放时被压扁。

## 隔离工作区实机/API 验证

在 `tmp/memory-jobs-runtime-20260823`、`127.0.0.1:8801` 启动新编译服务：

1. `GET /api/sessions/mario-demo/history?limit=6` 返回 1 个 completed turn、2 个 message items。
2. 写入一条 `context:auto-compact` bead 后，历史返回独立 `turn-compaction-*`，item `kind=compaction`；`limit=1` + `next_before` 能读取前一页 completed turn，确认游标分页。
3. 浏览器记忆窗口显示 `Thread history`、`2 个 turn · 3 个 item`、`已压缩 · 1 items` 和 compaction 摘要；Kind/Source 过滤器同步出现 `compaction/context:auto-compact`。
4. 截图证据：`tmp/memory-history-ui-evidence.png`；API 证据：`tmp/memory-history-api-evidence.json`。

## 本地验证

- `cargo fmt --package coolzhu-web-console --package coolzhu-core-runtime`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `cargo test -p coolzhu-web-console --offline session_history_exposes_turns_items_and_typed_compaction_cursor -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline web_frontend_memory_window_connects_summary_prompt_context_and_governance -- --test-threads=1`：通过。
- `cargo test -p coolzhu-core-runtime --offline memory -- --test-threads=1`：40 项通过。
- `cargo build -p coolzhu-web-console --offline`：通过；仅保留仓库既有 warning。
- `git diff --check`：通过。

## 后续优先级

继续把 history projection 接入 resume/fork/rollback 的最小兼容动作，并为跨进程恢复、SSE/JSONL 重放补合同 fixtures；不改变旧 `/messages` 接口。
