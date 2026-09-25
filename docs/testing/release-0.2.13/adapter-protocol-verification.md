# Qwen3.8 与 Anthropic 图片协议验收记录

日期：2026-09-19。此子任务只修改 llm-adapter；没有调用真实模型、读取密钥明文、操作桌面或修改用户配置。

## 改动与实际协议

修改文件均位于 `modules/llm-adapter/packages/llm-adapter/src/`：

- `providers/claw_provider.rs`：Anthropic 发送前将通用 `InputContentBlock::ImageUrl` 翻译为原生 `image/source`。data URI 变为 base64/source；HTTP(S) 地址变为 url/source；不传 OpenAI 的 `detail`。发送前拒绝空来源、不支持的 MIME、非 HTTP(S) URL；保留文本、工具调用与工具结果结构。
- `reasoning.rs`：新增 QwenThinking 策略及原生 Qwen 字段编码。
- `request_parameters.rs`：Qwen 显式模式校验、原生思考参数覆盖与互斥处理。
- `providers/openai_compat.rs`：真实发送前执行上述模型参数校验。

Qwen 检测对模型名 trim、ASCII 小写后精确匹配 `qwen3.8-flash`、`qwen3.8-max`、`qwen3.8-max-0902`、`qwen3.8-2.4t-a95b`、`qwen3.8-27b`，并要求 OpenAI 兼容协议。未知后缀、路径前缀、Omni 与 Anthropic 协议不继承此规则。未迁移 endpoint，未更改默认模型。

| 用户选择 | 发出的原生字段 |
| --- | --- |
| auto 模式且层级 auto | 省略思考字段，保持服务端默认 |
| 层级 none | `enable_thinking: false`；关闭优先于模式与预算 |
| low / medium / xhigh | `enable_thinking: true` 与对应 `reasoning_effort` |
| minimal | 映射为 low，并公开说明兼容映射 |
| high / max | 映射为 xhigh，并公开说明兼容映射 |
| thinking 模式且层级 auto | `enable_thinking: true`，省略层级 |
| budget 模式 | `enable_thinking: true` 与显式 `thinking_budget`，不发层级 |
| 不支持的 adaptive 等模式 | 发送前返回可解释配置错误；none 仍优先关闭 |

budget 在适配器接受 0–262144；本轮前端 OpenAI 参数页只有 auto / effort / thinking，未新增 budget 入口。独立配置模式选择可生效，未把标准 `thinking` 对象误发给 Qwen。通用其他模型和 Anthropic 编码有回归断言。

## 验证证据

- `cargo test -p coolzhu-llm-adapter --offline`：119 项通过（100 库测试、19 集成测试），0 失败，1 个需真实鉴权的 live 测试 ignored。日志：`adapter-full-test.log`。
- `cargo build -p coolzhu-llm-adapter --offline`：exit 0。日志：`adapter-build.log`。
- 新增 5 个 Qwen 测试覆盖所有层级、显式模式、预算互斥、关闭优先、非法模式/预算、其他模型隔离。
- 新增 3 个 Anthropic 测试覆盖协议转换、非法来源在联网前失败，以及本地 HTTP 接收端对 stream / non-stream 实际请求体的断言。没有真实模型网络请求。
- 只读确认 web-console `main.rs` 的 `reasoning_strategy_id` 已由主任务接入 `QwenThinking => "qwen_thinking"`；最终 web-console / release 验证由主任务执行。

## 已知边界

- Anthropic 来源校验验证结构、URL 协议、MIME 名与非空数据，不负责解码 base64 或验证图片像素；本地附件读取/真实格式/大小边界由主任务的附件模块负责。远程 URL 的远端内容不在此本地校验中。
- `InputContentBlock::Thinking` 已可独立编码为 assistant `reasoning_content`，现有 `assistant_tool_history_preserves_reasoning_content` 测试通过。web-console 同一用户轮的工具链会保留 Thinking；跨用户轮/重启的持久化历史恢复仍把持久化消息统一转为 Text，不能宣称跨轮独立思考字段已完整保留。该缺口已报告主任务，本子任务未扩大修改范围。
- 未显式写入 `preserve_thinking`，依赖官方默认。真实模型任务的结果和耗时由主任务另行记录，不等同于这些本地协议测试。

## 官方依据

- 阿里云 OpenAI 兼容 Chat Completions：<https://www.alibabacloud.com/help/en/model-studio/qwen-api-via-openai-chat-completions>。本地已取原文 `official-chat-api-jina.md` 与 `official-chat-api-excerpts.txt`。
- Qwen3.8-Flash：<https://help.aliyun.com/zh/model-studio/qwen3-8-flash>。
- Anthropic 视觉协议：<https://platform.claude.com/docs/en/build-with-claude/vision>。本地已取原文 `anthropic-vision-docs.txt`。
