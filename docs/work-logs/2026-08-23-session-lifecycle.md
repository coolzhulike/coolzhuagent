# 2026-08-23 会话恢复、分叉与回滚闭环

## 本轮目标

将 Codex harness 的 thread 生命周期差异补齐到 web-console：会话恢复后可以拿到同一份可重放 history，历史 turn 可以分叉为独立会话，也可以按游标回滚后续消息和自动压缩记忆。

## 实现

- 新增 `POST /api/sessions/{session_id}/resume`：激活持久化会话并返回最近 20 个 turn/item。
- 新增 `POST /api/sessions/{session_id}/fork`：按 `before` turn/item 游标复制会话配置、消息和游标之前的记忆，分支使用独立 session id。
- 新增 `POST /api/sessions/{session_id}/rollback`：按 turn/item 游标保留目标及之前的消息，移除后续消息和后续自动压缩 bead，更新上下文边界。
- Thread history UI 为可回放的已完成 turn 增加“分叉/回滚”操作；会话设置增加“继续/分叉”入口。
- 压缩 turn 不允许直接作为 rollback/fork 的消息截断边界，避免把摘要误当作原始消息。

## 本地验证

- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `cargo fmt --package coolzhu-web-console`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过（仅保留仓库既有 warning）。
- 生命周期单测 `session_resume_fork_and_rollback_round_trip_history_cursor`：通过。
- web-console 全量测试：`833 passed; 0 failed`。
- 隔离运行时（`COOLZHU_RUNTIME_DIR=tmp/memory-jobs-runtime-20260823`，端口 8801）：
  - 两轮 `/api/chat/send` 写入消息成功；
  - `/resume` 返回 4 个历史 turn；
  - `/fork` 按首轮 turn 复制 4 条消息、2 条历史记忆；
  - `/rollback` 移除后续 2 条消息；额外注入的后续压缩 bead 被移除 1 条；
  - 停止并重启 web-console 后仍读取到 2 个会话，源会话和分支各有 3 个历史 turn。

## 证据

- API 结果：`tmp/memory-lifecycle-api-round3-final.json`、`tmp/memory-lifecycle-api-round3-restart.json`。
- 浏览器实机截图：`tmp/session-lifecycle-ui-evidence.png`，显示 Thread history、压缩 turn 以及已完成 turn 的“分叉/回滚”按钮。

## 备注

本轮运行使用独立临时 workspace，不修改用户当前 8765 端口的已安装实例；真实模型凭据未配置时，消息测试按项目既有本地回退路径完成持久化链路验证。
