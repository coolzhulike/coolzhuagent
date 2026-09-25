# 2026-08-24 流式回合与上下文快照关联

## 本轮目标

继续补齐与 Codex Harness 对比后确定的上下文可追溯能力：让流式模型调用、工具循环、历史回放和 AgentEvent JSONL 使用同一个真实运行时回合 ID，并保持上下文快照 ID 可回放。

## 修改内容

- `ContextAssembly` 增加可选 `turn_id`，由真实运行时 trace 提供；`context-preview` 等没有真实回合的预览调用保持空值。
- `api_chat_send_stream` 在每轮流式请求入口生成 `stream_turn_id`，贯穿流式模型调用、两处并行工具调度和模型工具回退路径，不再误用第一条用户消息 ID。
- `stream_agent_model` 将真实回合 ID 写回上下文装配，使流式请求的 footer 与非流式请求格式一致。
- 上下文 footer 增加 `Context turn`；历史 DTO 与 AgentEvent envelope 同时暴露 `context_turn_id` 和已有的 `context_snapshot_id`。
- 快照/回合标记解析改为在整行中定位标记，兼容同一行内连续记录 `Context turn` 与 `Context snapshot` 的新 footer 格式。

## 本地验证

- `cargo build -p coolzhu-web-console --offline`：通过。
- `cargo test -p coolzhu-web-console stream_model_tool_loop_reuses_full_context_assembly --offline -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console session_agent_events_jsonl_is_stable_and_typed --offline -- --test-threads=1`：通过。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：835 passed，0 failed。
- `node --check modules/gui-web/packages/web-console/src/app.js` 与 `git diff --check`：通过。
- 使用独立运行目录和 `127.0.0.1:8801` 启动最新构建，调用 `GET /api/sessions/mario-demo/context-preview` 验证 `history_source=chat_room.messages`、候选 15、载入 11、排除 4、快照 `ctx-906b0f8739a452ea`、记忆 revision `mem-e4bb9ce2b99f0a9a`；运行后确认 8801 端口和进程已退出。

## 证据

- 全量测试日志：`tmp/round9-web-tests.log`
- 上下文快照 API：`tmp/turn-snapshot-api-evidence-round9.json`
- 记忆/上下文界面：`tmp/turn-snapshot-ui-evidence-round9.png`
- 上下文预览界面：`tmp/turn-context-preview-ui-evidence-round9.png`

## 备注

本轮隔离目录使用的模型配置为 `智谱 AI / glm-4.6`，运行 fixture 未提供 API Key，因此没有声称完成真实云端流式 token 测试；真实 provider 流式链路由定向静态约束测试和构建验证覆盖。
