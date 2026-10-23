# HA-04 实际请求绑定：部分实施记录

状态：**部分实施，HA-04 未验收**。已实现最终请求摘要与 SSE `request_bound` 事件；started 首响应、Devin/订阅路径、撤销屏障和重试/重采样语义尚未补齐。

## 已实现

- 新增 `request_binding.rs`：
  - `RequestBinding { request_id, attempt_id, parent_request_id, turn_id, model, tool_names, tools_digest, parameters_digest, stream }`
  - 在最终 `MessageRequest` 构造、`retain_admitted_mcp_definitions`/`bind_session_tool_catalog` 之后捕获。
  - `tools_digest` 对最终 `request.tools` 做 SHA-256；`parameters_digest` 对 model/max_tokens/reasoning_effort/stream/tool_choice 做 SHA-256。
  - 不包含 system prompt、messages、密钥或原始参数值；`request_id` 由 turn/attempt/parent + 摘要派生。
- `stream_agent_model` 返回 `Option<RequestBinding>`；HTTP 路径在真正 `stream_message(&request)` 前生成绑定。
- 流式循环在收到流后、首个 token 前发送新 SSE 事件 `request_bound`，携带 binding。
- `ChatStreamStarted` 未修改；旧客户端可忽略新事件。
- Devin ACP、OpenAI 订阅路径返回 `None`（当前未绑定），保持原身份/权限/收尾路径不变。

## 已验证

`cargo test -p coolzhu-web-console --offline request_binding`：

- `binding_freezes_tools_and_parameters_without_prompt_or_secret`
  - 同一请求重复捕获一致；工具集合改变后 digest/request_id 改变；
  - 序列化结果不含 `SYSTEM SECRET`、`sk-live`、`secret prompt body`。
- `attempt_and_parent_are_distinct_and_traceable`
  - root/retry 的 request_id、attempt_id 不同；retry 的 parent_request_id 指向 root；
  - 空工具集仍生成 tools_digest。
- `binding_carries_no_authorization_and_revocation_is_live`
  - binding 序列化不含 authorized/permission；
  - 记录 session grant 后 `session_grant_view_for` 为 authorized，清空后立即变为未授权、未二次确认；
  - 说明旧 binding 不携带授权，live gate 仍按当前状态决定。
- `pending_binding_is_taken_once`
  - 非流式入口按 turn_id 记录/取走绑定；第二次取回为空，避免重复归因。

`4 passed; 0 failed`；exit 0。生产 `cargo check -p coolzhu-web-console --offline` exit 0。

非流式 `SendMessageResponse` 新增可选 `request_bindings` 字段；`call_agent_model_with_tool_loop` 在 round 0 最终工具过滤后记录，`api_chat_send` 收口时取回。空列表不序列化，旧客户端兼容。

## 尚未满足的验收

- **HA04-01**：普通入口已通过 `SendMessageResponse.request_bindings` 返回绑定；SSE 仍用 `request_bound` 事件，不在 `started` 首响应。改设置后的旧响应解释尚无两入口对照。
- **HA04-03**：只有 root/retry 的 attempt/parent 单元测试；没有真实传输重试、重新采样、子请求的接纳计数与上限测试。
- **HA04-04**：未构造“请求在途撤销权限/停用工具”的屏障；现有 live gate 仍读当前授权，但没有本工作包的拒绝回执。
- **HA04-05**：Devin ACP、OpenAI 订阅路径没有 binding；CLI/ACP 回归未运行。
- **HA04-02**：已对最终 request 摘要，但未与 adapter 发送点逐字段对照；未记录实际 HTTP 字节。
- `tool_invocation_identity` 的 source_request_key 尚未消费 `RequestBinding.request_id`；执行身份与请求绑定仍是两套。

## 限制

- 没有真实模型请求；`request_bound` 只在流式 HTTP 路径产生。
- 没有把 binding 写入持久运行账本；刷新/重连后无法恢复。
- 不能据此宣称“旧快照不能放行”已满足；需要 HA-04-04 的屏障测试。
