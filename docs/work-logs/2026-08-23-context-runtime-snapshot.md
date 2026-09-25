# 2026-08-23 上下文运行时作用域快照

## 本轮目标

按 Codex Harness 差异引入计划，优先补齐 P0-2：聊天室上下文预览必须能说明本轮实际使用的 workspace、聊天室、权限、模型和工具目录，且作用域改变后不能继续复用旧的上下文身份。

## 实现

- `modules/gui-web/packages/web-console/src/main.rs`
  - `ContextBuildOptions` 增加 `chat_room_id`，上下文预览、流式模型调用和工具循环均将当前聊天室传入装配层。
  - `ContextAssembly` 增加 `runtime_snapshot`，记录 `snapshot_id`、workspace identity、聊天室、权限 profile、模型/provider、工具目录 revision、memory revision 和 history floor。
  - `context_snapshot_id` 将 workspace、聊天室、权限和工具目录 revision 纳入哈希；这些作用域任一变化都会得到新的上下文身份。
  - 工具目录 revision 基于当前工具类别、状态、权限和可执行状态计算；聊天室权限读取失败时安全回退到当前有效权限 profile。
- `modules/gui-web/packages/web-console/src/app.js`
  - memory/context preview 显示 runtime snapshot、workspace、room、permission、model/provider 和 tools revision，便于现场确认前后端是否切换一致。

## 本地验证

- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过；仅保留仓库已有 warning。
- `cargo test -p coolzhu-web-console --offline context_ -- --test-threads=1`：26 项通过。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：829 项通过。
- `cargo test -p coolzhu-web-console --offline`：829 项通过。
- `cargo check -p coolzhu-tool-registry --offline`：通过。
- `cargo test --test module_linkage_smoke --offline`：4 项通过。

## 仍需关注

- 本轮只固定并展示上下文作用域身份，没有改变权限授予策略；写操作仍由既有审批/房间权限链路决定。
- `showUI` 本地模型不可用时继续沿用现有 capability probe 与降级路由，本轮未强行启用本地视觉推理。
- 工具目录 revision 在上下文构建时计算，后续若现场 profile 变化频繁，可再评估缓存策略，但必须保留配置变化可见性。
