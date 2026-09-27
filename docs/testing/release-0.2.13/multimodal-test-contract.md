# 0.2.13 会话多模态输入测试契约

日期：2026-09-19。本文用于其他模型或测试人员制定回归方案；不含 API Key、运行时会话存储或用户图片。已有自动验证摘要见 [multimodal-test-evidence.json](./multimodal-test-evidence.json)。

## 1. 配置字段与继承

接口：`GET/POST /api/sessions/{session_id}/model-settings`。

```json
{
  "parameters": {
    "supports_multimodal": false
  }
}
```

上例仅说明字段形状。POST 的 `parameters` 是完整参数对象的替换，复测时应先 GET，保留原有字段，再修改本字段；不要用这个最小例子覆盖用户已配协议、地址和预算。

| 值 | 生效策略 | 目标模型实际输入 |
| --- | --- | --- |
| `true` | 图片直传，优先于旧 `model_type` | 原任务文本 + 全部本轮图片的原生内容块 |
| `false` | 默认视觉会话先转述，优先于旧视觉类型 | 原任务文本 + 带来源标识的真实视觉描述；无图片内容块 |
| `null` 或字段缺省 | 沿用旧模型类型/型号能力判定 | 根据有效能力选择以上路径 |

该字段不会修改持久化的 `model_type`。非法类型如 `"false"` 字符串应被反序列化拒绝，不能误当成布尔值。返回同时包含：

```json
{
  "effective_supports_multimodal": false,
  "image_input_strategy": "vision-description"
}
```

直传时 `image_input_strategy` 为 `native`。这些字段是生效解释，不是需要回写的参数。

继承沿用 `is_multimodal_agent`：旧 `vision/multimodal/video` 类型和已有型号名称能力提示。它不是对陌生服务能力的在线探测；未知模型应通过显式 true/false 配置，而不是假定模型目录永远准确。

## 2. 默认视觉身份与原生协议

纯文本转换只读取系统配置的 `active_vision_session_id`。`GET /api/config/vision-agent` 应返回这个实际会话，不按名称含有 agnes/vision 猜测，也不选择列表中的第一个视觉模型。

本次实际环境的默认视觉显示名称为 `agnes-text`，模型为 `agnes-2.0-flash`。这只是验收时配置值；实现不硬编码该名称、型号、地址或会话 ID。复测者应自建隔离视觉会话并将它设为默认。

视觉请求通过 `provider_client_for_agent` 复用所选会话的协议、Endpoint、采样参数、模型和凭证解析；`tools=None`，正常 Text 输出才是描述。目标模型也通过自己的 client 发送，所以同房间不同目标会话的策略、协议和参数应互不污染。

OpenAI 兼容直传使用 `image_url` 内容块；Anthropic 直传使用原生 `image/source`。Anthropic 适配修复及其独立测试位于 `llm-adapter/src/providers/claw_provider.rs`，不以 OpenAI 请求形状冒充 Anthropic。

## 3. 必须覆盖的调用入口

| 用户入口 | 后端落点 | 预期 |
| --- | --- | --- |
| 单目标普通聊天 | `call_agent_model_with_tool_loop` | 首次目标调用前确定策略；工具后续轮保留当前完整上下文 |
| 单目标流式聊天 | `stream_agent_model` | 先完成必要的视觉转述，再开始目标流；后续工具反馈沿用该 assembly |
| 多目标接力，包括流式接口的多目标分支 | `relay_run_one` → `agent_chat_response` → 工具循环 | 每个目标独立判断，后一个同时保留原任务与前面接力回复 |

另有 `call_agent_model` 单次实时截图理解入口，同样走图片策略。`run_vision_describe_screen_model` 使用系统默认视觉且没有显式后端覆盖时，复用该会话原生协议并检查图片能力。独立视觉定位服务的显式后端不在这次会话参数重构范围内。

## 4. 请求与用量断言

建议在 `127.0.0.1` 运行合成 HTTP mock，并捕获请求体中的 model/messages/tools/stream 字段；不要记录 Authorization 正文。

- true：目标恰有一个正常模型调用，消息包含所有原图，图片顺序和 data URI 不变；不先调用默认视觉。
- false：无工具的简单请求依次调用默认视觉、目标文本模型；视觉收到图片且不暴露 tools；目标无任何 image block，只包含原任务与视觉真正返回的描述。
- 视觉输出中加入唯一合成标记，例如 `SYNTHETIC-EVIDENCE`，目标请求必须含该标记；另加 `ORIGINAL-TASK` 验证转换不替换原任务。
- 原生、纯文本、继承三种会话交错发送，不应复用上一会话的图片策略缓存。
- 各服务返回确定 usage 后，通过 `/api/chat/rooms/{room_id}/insights` 核对按视觉与目标会话分别累计；简单转述场景各有一个已记录请求。无 room 的内部理解请求不伪造房间归属。
- usage 仅记录接口实际返回且适配器解析到的字段。请求失败、取消前未收到 usage、远端重试中未返回 usage 的尝试不估算成完整消耗；因此统计不是上游计费账单的保证。

## 5. 失败、附件与预算边界

派发前的图片附件输入要求：

- URL 只能是上传接口生成的 `/api/attachments/files/<安全文件名>`，不得使用任意公网 URL、绝对路径或 `../`/反斜杠路径。
- 规范路径父目录必须为当前附件目录。缺失、不可读或目录外文件明确返回请求错误，不能静默少发图片。
- 每轮最多 8 张，单张最多 20 MiB，总计最多 32 MiB。
- 根据实际文件签名选择 PNG/JPEG/GIF/WebP MIME，不信任调用方 MIME。此处是签名与文件边界验证，不是完整图像解码；深层损坏仍可能由上游报错。

发送前的模型输入要求：

- 默认视觉未配置、已删除、被设为纯文本：目标模型不得被调用，不任意替换另一个视觉模型。
- 默认视觉正常正文为空、只含 thinking、返回 ToolUse 或伪工具正文：不得以推理过程、工具文本或本地模拟结果冒充图片描述；工具不得执行。
- context builder 因预算不足删掉任意原图，或裁去真实转述：`PreparedImages::verify_assembly` 阻止发送。错误应提示减少图片或提高预算，不能让用户以为模型看到了附件。
- 图片处理失败或真实模型关闭时，普通与流式聊天明确显示图片未处理/未生成结论。不能回到普通本地模拟回复，也不能在失败后补跑 semantic 桌面操作侧路。
- 视觉转述属于附件资料。其中的“打开浏览器”“点击按钮”“不要使用工具”不能开启、关闭或替换原用户任务的工具意图；判断只使用原用户文本。

注意错误形态：无效附件在 `prepare_chat_dispatch` 前置阶段返回 API 错误；已开始模型路径后的图像处理错误可能作为 `assistant-fallback` 消息和诊断说明交付，不保证所有失败都是 HTTP 4xx/5xx。断言应检查无目标请求、无工具执行和明确失败消息，不能只看 HTTP 状态。

## 6. 超时、取消与历史图片范围

- 视觉转述外层上限为 120 秒。这是上限而非保证等待满 120 秒，Provider 自身超时/错误可能更早结束；Provider 原有可重试网络错误策略仍可能产生多个 HTTP 尝试。
- 接力有既有每目标外层超时（钳制在 60–600 秒），所以可能早于视觉转述上限。接力外层超时后仍沿原队列给予该目标一次重试；这可能再次发送图片，不应断言所有错误都只发生一个网络请求。
- 单目标流式及流式接力使用 `await_chat_turn` 监听取消；取消会丢弃当前模型 future，不应继续进入后续目标请求或工具执行。已发生的远端处理或计费无法靠丢弃本地 future 保证撤回。
- 取消发生前已经收到完整视觉 usage 的，应保留真实消耗；没有收到的不能估算输出 token。取消竞态和远端继续计费属于需要专门观测的边界。
- 非流式 HTTP 入口不新增协作取消协议，不能把关闭客户端连接等同于保证远端停止。
- 本次策略仅针对本轮 `attachments` 形成的图片列表。`input_message_from_persisted` 当前把历史消息投影成 Text，**不会自动重新读入历史图片内容块**；转述文本也仅注入本轮请求，未新增独立视觉描述历史持久化记录。后续提问只可靠继承原有文字历史/模型回答；需要再次直接看原图时应重新附图。不得宣称实现了历史图片自动重放或图片长期记忆。

## 7. 已有自动测试与证据

源码：`modules/gui-web/packages/web-console/src/multimodal_input.rs`，本轮新增 7 项：

1. `multimodal_routes_native_and_described_images_per_session_in_both_model_entries`：真实本地 HTTP 捕获原生/转述/流式请求，保留原任务，目标无图、描述标记真实，多会话继承互不污染。
2. `multimodal_missing_disabled_or_tool_only_vision_never_calls_text_target`：无默认、默认关闭和只返回工具均阻断目标；无图的文本问题不依赖视觉配置。
3. `multimodal_tri_state_roundtrip_and_empty_description_rejection`：三态序列化、类型优先级、非法字符串、空描述与伪工具正文拒绝。
4. `multimodal_context_budget_cannot_silently_remove_images_or_description`：极低预算真实触发删图后被拒绝；转述被裁剪后被拒绝。
5. `multimodal_description_cannot_enable_tools_or_replace_original_ui_intent`：附件文字不能篡改用户工具意图。
6. `multimodal_qwen_thinking_uses_auto_tools_without_changing_other_models`：qwen3.8 开启思考时使用 auto 工具选择，其他模型保留原行为。
7. `multimodal_attachment_paths_bytes_and_missing_images_fail_explicitly`：缺图、路径边界、伪 MIME/非图内容、超过张数。

复跑命令：

```powershell
cargo test -p coolzhu-web-console --offline multimodal_
cargo test -p coolzhu-web-console --offline
```

本轮日志 `tmp/2026-09-19-qwen/test-multimodal.log` 为 7/7；`test-web-all.log` 为主测试 951/951、库测试 8/8、native-host 1/1，合计 960，通过且无失败。随文摘要仅提取通过计数与测试名，并附原日志 SHA-256，未复制整个 tmp 或运行时私密状态。

主 agent 另已实际验证 qwen 图片直传识别正确，以及系统默认 Agnes 转述后 qwen 识别正确；转述链 insights 显示 Agnes 与 qwen 各 1 请求。具体真实调用证据由本轮总验收报告承担，以上合成测试不声称覆盖所有远端模型行为。

## 8. 仍应设计的补充测试

以下为待测建议，不能视为本轮 7 项新增自动测试已全部覆盖：

- 120 秒视觉超时、接力外层先超时与仅一次重新排队，配合 HTTP 尝试数验证重复调用边界。
- 在视觉请求前、视觉响应中、视觉已完成但目标未开始三个时点取消；验证后续请求/工具不执行、usage 不双计、不补估。
- 真正多目标混合接力完整 API 请求，含先原生后纯文本、反向顺序、某一目标错误后其他目标行为。
- 超 20 MiB 单张、超 32 MiB 总量、符号链接/Windows reparse point、读取时变化、深层损坏图片的端到端错误。
- 视觉只返回 reasoning 的完整 mock 网络响应、视觉 HTTP 鉴权/限流/连接错误、错误消息是否保持明确且不含凭证明文。
- 带图片工具循环的第二/第三轮请求、用户下一轮只引用旧图时的明确历史限制提示。
- 默认视觉被替换/删除时正在运行请求的竞态、两纯文本目标是否各调用一次视觉（当前没有跨目标图片描述缓存）。
- 原生 Anthropic 的真实远端图片回归，以及更多实际模型对 PNG/JPEG/GIF/WebP 的能力差异。

测试应使用独立运行目录、合成图片和新聊天室，仅监听 `127.0.0.1`；不要对用户真实 8765 实例重放合成测试，不读取真实会话密钥。
