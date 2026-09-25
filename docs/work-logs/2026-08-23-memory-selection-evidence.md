# 2026-08-23 记忆召回选择证据

## 本轮目标

按 Codex Harness 差异引入计划继续推进 P1-1。先补齐上下文预览中“记忆为什么被加载/过滤”的最小可观测闭环，避免只看到最终 bead 数量而无法解释候选、TTL、superseded 和 token 预算裁剪。

## 实现

- `modules/gui-web/packages/web-console/src/main.rs`
  - `select_context_memory_beads` 返回记忆 bead 与 `ContextMemorySelectionEvidence`。
  - 记录召回策略（`semantic`、`keyword` 或 `prompt_fallback`）、原始候选、实际选中 bead、TTL/无效过滤、superseded 过滤、记忆 token 预算跳过及 token 用量。
  - 在最终 system prompt 预算裁剪后同步 `selected_ids` 与 `used_tokens`，确保预览展示的是本轮真正注入的 bead，而不是过滤前的中间结果。
  - `ContextAssembly` 对外序列化该证据，并加入回归断言。
- `modules/gui-web/packages/web-console/src/app.js`
  - 记忆窗口的 Context Preview 展示上述召回策略、候选/选中 ID、过滤原因和 token 预算。

## 实机/API 验证

使用隔离工作区和 `127.0.0.1:8799` 启动刚编译的 web-console，创建测试会话、聊天室和一个手工记忆 bead，再调用 context-preview API。返回结果确认：

- `strategy=keyword`；候选与最终注入均包含同一个 bead。
- `used_tokens=16/token_budget=67918`；TTL、superseded、预算跳过均为空。
- runtime snapshot 同时返回 workspace、room、`workspace-write`、`glm-5.2`、`test` provider 和工具目录 revision。

截图证据：`tmp/context-preview-evidence.png`（本轮最终 UI 预览）。

## 本地验证

- `cargo build -p coolzhu-web-console --offline`：通过。
- `cargo test -p coolzhu-web-console --offline context_builder_includes_relevant_beads_history_and_current_turn -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline web_frontend_memory_window_connects_summary_prompt_context_and_governance -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：829 项通过。
- `cargo test -p coolzhu-web-console --offline`：829 项通过。
- `cargo check -p coolzhu-tool-registry --offline`：通过。
- `cargo test --test module_linkage_smoke --offline`：4 项通过。
- `node --check modules/gui-web/packages/web-console/src/app.js` 与 `git diff --check`：通过。

## 后续优先级

本轮是 P1-1 的最小增量，尚未实现 memory mode 切换、提取/合并后台 job、turn/item 分页、resume/fork/rollback 等生命周期操作；下一轮继续按优先级补齐这些能力，并为每轮保留可复现的 API/UI 截图证据。
