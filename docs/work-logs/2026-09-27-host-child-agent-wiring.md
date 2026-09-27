# S2.5 子 Agent 宿主接线工作记录

日期：2026-09-27。分支：`codex/navigation-update-followup`。

## 已接通的路径

- tool-registry 的 `Agent` 只负责解析子任务、角色和请求工具集合，调用显式传入的 `HostAgentRunner`。无宿主时返回明确的“未执行”；旧的独立线程、独立 `ConversationRuntime`、硬编码 Claude 默认模型和权限策略已从生产源码删除。
- Web 聊天在持久运行接纳时，为每个目标 Agent 冻结已解析的 provider、模型请求参数、可用工具、房间权限、工作区与用量库。流式、普通发送及接力均通过相同的宿主子执行入口。覆盖模型仅可选择此时已解析且可用的配置。
- 子任务同步复用已有模型请求、工具派发、审计和用量事实链。每次真实模型请求只写一次 `chat_usage_events`；归属父 conversation session、room、turn 与对应父工具 call，工具策略仍按目标 Agent 会话检查。多目标接力中，各目标分别使用自己的快照。
- `Agent` 本身在定义和实际派发两处检查接纳快照；子工具再取父工具集、角色、请求集合与冻结权限的交集，不开放递归 Agent 或向用户发消息。后续配置和 scheduled grant 只可收紧，不能扩大原快照。
- 父取消和根截止贯穿子模型等待及工具派发；执行失败或已过截止的答复不能标为完成。子任务不获取新的聊天室回合锁、不切换 UI 聊天室、不留后台任务。
- Goal phase 在原持久库确认 run/claim 后冻结单目标宿主快照、权限、模型与原 db_path；子模型开始前、收到响应后及实际工具派发前均复核原 claim。仅在该子请求存活期间检查阶段停用，失效时取消在途请求。
- 模型请求用量保留原内部 `turn_id`，另写可空的公开 `run_id`。轨迹优先以公开 run 查各次请求；旧行仅在公开 turn 或 `call_id → tool_calls.run_id` 可确定时关联，房间统计仍只按原唯一请求事实聚合。

## 验证与范围

- `cargo test -p coolzhu-web-console --offline host_child_agent::tests`：本地真实 HTTP 模型请求验证两个不同目标 Agent 使用同一父 conversation session；成功结果、父身份、单次用量、请求 call 区分、未配置模型拒绝、只读快照拒绝 Agent 与写工具、预先和在途取消、预先和在途根截止。日志：`tmp/s25-web-test-final.log`。
- `cargo test -p coolzhu-tool-registry --offline agent_`：3 项通过，涵盖显式宿主、无宿主、角色工具约束及必填字段。日志：`tmp/s25-registry-test-2.log`。
- `cargo build -p coolzhu-web-console --offline` 和 `cargo build -p coolzhu-tool-registry --offline` 已通过；最终构建日志分别在 `tmp/s25-web-build-final.log`、`tmp/s25-registry-build-final.log`。
- Goal 在途停用与 claim 更换测试已通过：本地 HTTP 子模型请求开始后停止阶段，子执行被中断；原 claim 丢失后 `write_file` 在真实工具派发入口被拒绝，目标文件未生成。日志：`tmp/s25-goal-test.log`。Goal 首轮实际构建通过：`tmp/s25-goal-web-build.log`。

本次接线覆盖 Web 聊天发送、接力与 Goal phase。默认 CLI `chat`、已绑定 prompt/REPL 均经 `shared_chat::run_async` 投递同一 Web 流式聊天接口；当前 CLI 二进制对隔离当前 Web 与本地假模型的真实端到端验证已通过，脱敏证据见 `docs/testing/release-0.2.17/evidence-history/s25-cli-agent-host-e2e.md`、`s25-cli-agent-host-result.json` 与 `s25-cli-agent-host-events.jsonl`。显式 `--legacy-runtime` 不经过 Web，调用 Agent 时无宿主并明确拒绝。新增 run 关联的最终构建和真实轨迹复测尚在进行，不将首轮证据冒充其验收结果。

补充真实 Goal 工程固定验收：以 SHA-256 `dfd39612cd2d689e6cf4e87de9b0c421682d1be33137d854a4ce8f5d2479d8ce` 的 Web 副本运行合成 Goal 阶段，假模型确认接到请求后暂缓答复，此时工程切换 HTTP 为 409 且仍在原工程；模型答复后阶段返回 200，同一工程切换返回 200。脱敏结果在 `docs/testing/navigation-update-followup/s25-goal-workspace-pin.md` 与 `s25-goal-workspace-pin-result.json`。最终组合构建后的 CLI/usage 回归仍需单独执行。
