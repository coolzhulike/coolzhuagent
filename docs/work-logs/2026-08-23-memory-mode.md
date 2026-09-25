# 2026-08-23 会话级 memory mode

## 本轮目标

继续 P1-1 记忆生命周期：为每个模型会话增加可持久化的 `memory_mode`（`enabled`、`disabled`、`polluted`），让用户能够在记忆窗口暂停自动召回或隔离疑似污染记忆，并让上下文快照显式反映该边界。

## 实现

- `modules/gui-web/packages/web-console/src/main.rs`
  - 新增 `memory_settings` SQLite 侧表和 v17 schema 初始化；旧会话缺省为 `enabled`，删除会话时清理孤儿设置。
  - 新增 `PATCH /api/sessions/{session_id}/memory-mode`，校验三种模式、更新会话时间并返回当前模式。
  - 会话摘要返回 `memory_mode`；`ContextRuntimeSnapshot` 和 snapshot hash 纳入该值，切换模式不会复用旧上下文身份。
  - `disabled`/`polluted` 阻止自动 memory recall，不删除 bead，选择证据标记为 `disabled_blocked`/`polluted_blocked`，token 用量为 0，并写入 diagnostics。
- `modules/gui-web/packages/web-console/index.html`
  - 记忆窗口新增 Memory mode 下拉框：启用自动召回、停用自动召回、污染隔离。
- `modules/gui-web/packages/web-console/src/app.js`
  - 读取会话模式、调用 PATCH 接口、失败回滚选择框，并在 Context Preview 中展示 `memory_mode`。

## 实机/API 验证

在隔离工作区 `tmp/memory-mode-workspace` 的 `127.0.0.1:8800` 启动刚编译的服务：

1. `enabled` 初始预览返回 `strategy=keyword`，选中测试 bead，快照为 `ctx-f56420cbf8e6467f`。
2. PATCH 为 `disabled` 后，预览返回 `runtime_snapshot.memory_mode=disabled`、`strategy=disabled_blocked`、`memory_beads=0`、`selected_ids=[]`，快照变为 `ctx-8925b8d7b95b667d`。
3. 停止并重启隔离服务后，`GET /api/sessions` 仍返回该会话 `memory_mode=disabled`，确认 SQLite 侧表跨进程持久化。
4. 浏览器 UI 的 Memory mode 选择框显示“停用自动召回”，Context Preview 同步显示 `disabled_blocked`、`memory_tokens=0` 和 `memory_mode=disabled`。

截图证据：`tmp/memory-mode-disabled-evidence.png`。

## 本地验证

- `cargo fmt --package coolzhu-web-console`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过；仅保留仓库已有 warning。
- `cargo test -p coolzhu-web-console --offline context_memory_mode_blocks_auto_recall_with_explicit_evidence -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline context_snapshot_id_changes_when_runtime_scope_changes -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline web_frontend_memory_window_connects_summary_prompt_context_and_governance -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：830 项通过。
- `cargo test -p coolzhu-web-console --offline`：830 项通过。
- `cargo check -p coolzhu-tool-registry --offline`：通过。
- `cargo test --test module_linkage_smoke --offline`：4 项通过。
- `git diff --check`：通过。

## 后续优先级

`memory_mode` 已具备最小可用边界；下一轮继续补 extraction/consolidation job 状态、typed compaction item 及 turns/items 分页读取，再接 resume/fork/rollback 的统一历史协议。
