# Qwen3.8-Flash 官网核实与图片能力设置

核实日期：2026-09-19。职责范围：官网只读核实、统一参数页图片能力 UI；未读取真实密钥、未写实际会话配置、未发送真实模型请求。

## 官网已确认

| 项目 | 确认结果 |
|---|---|
| 精确调用 ID | `qwen3.8-flash`，并非 `qwen3.8-flash-next` 或 `qwen3.8-omni-flash` |
| 模态 | Text / Image / Video 输入，Text 输出 |
| 接口 | OpenAI 与 Anthropic 兼容协议 |
| 工具 | Function Calling 支持；不等于原生完成鼠标点击，实际电脑操作仍需应用工具执行 |
| 上下文 | 1,000,000 tokens |
| 输入上限 | 普通 991,808；思考模式 983,616 |
| 输出上限 | 两种模式均 131,072 |
| 思维链上限 | 262,144 |

以上来自[百炼模型详情](https://help.aliyun.com/zh/model-studio/qwen3-8-flash)。这是 API 托管模型规格，不套用开源 Flash-Next 的容量。

[官方 OpenAI Chat API](https://www.alibabacloud.com/help/en/model-studio/qwen-api-via-openai-chat-completions)确认：

- `reasoning_effort` 原生 `low / medium / xhigh`，默认 `xhigh`；`high/max → xhigh`、`minimal → low`、`none → enable_thinking=false`。
- `reasoning_effort` 与 `thinking_budget` 互斥。显式 low / medium / xhigh 对应预算 4096 / 16384 / 262144；均省略时文档列默认预算 131072。
- `temperature ∈ [0,2)`，`top_p ∈ (0,1]`，推荐只设置其中一个；未找到 Flash 的明确采样默认数值，不套用 Omni 默认值。
- `tool_choice=auto` 可用。Qwen 不支持 `required` 的可靠强制语义，思考模式不支持强制指定函数。`tool_stream=false` 是复杂参数默认，`parallel_tool_calls` 默认 false。
- `max_tokens` 仅限制答案；新接入推荐 `max_completion_tokens`，限制思考与答案之和。
- `preserve_thinking` 对该模型默认 true；历史思考应单独回传 `reasoning_content`，不得拼到正文。缺历史字段不会报错。

北京旧域名的官方示例请求地址仍为 `POST https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions`，见[子业务空间调用说明](https://help.aliyun.com/zh/model-studio/model-calling-in-sub-workspace)。[主 API 页面](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)建议迁移至业务空间专属域名；本轮按用户要求保留现有 base_url 与密钥，未自行迁移地域或域名。

## 对本项目的配置建议（建议不代表已执行）

现有 base_url 为 `https://dashscope.aliyuncs.com/compatible-mode` 时，完整路径需由 endpoint `v1/chat/completions` 拼接；应由主代理确认实际 URL resolver 行为，不能只把 endpoint 写成 `v1`。

建议 `reasoning_mode=effort`、`reasoning_effort=low`，上下文 1000000，测试输出预算 16384 或 32768，采样两项和 thinking_budget 留空，`supports_multimodal=true`。本项目目前输出预算发送为 max_tokens，因此这是答案预算，不能把它解释为完整思考预算。官网最大值与实际测试预算应分开报告。保存时不传 api_key_ref 即可保留既有密钥。

未确认项：用户当前账号对模型的实际权限、配额和旧域名实时可用性；该模型并行工具调用的独立实测；模型在真实图像和电脑任务中的表现。这些只能由后续统一真实测试确认，官网能力声明不能代替验收。

## UI 交付与验证

唯一产品改动：`modules/gui-web/packages/web-console/src/model_settings.js`。复用现有玉石 CSS，无需改样式。

新增“图片输入能力”选择：

| 界面 | parameters.supports_multimodal | 意图 |
|---|---|---|
| 沿用模型能力 | null | 沿用模型用途与能力判断 |
| 支持图片直传 | true | 由目标模型接收图片 |
| 纯文本 · 由视觉 Agent 描述 | false | 视觉 Agent 描述后，将文字交给目标模型 |

保留旧 `session.model_type` 字段。继承状态读取后端 `effective_supports_multimodal` 与 `image_input_strategy`，明确显示的是已保存配置，避免把未保存新模型的能力当成已生效。缺后端有效字段时只提示保存后查看实际策略。本次开关管理图片输入，不声称增加了音频/视频上传链路。

验证：`node --check` 通过。独立无网络浏览器契约测试通过默认继承与有效策略、true/false/null 保存和重新读取、旧 model_type 保留、密钥留空不覆盖、选择即时标记草稿、无页面异常。共 3 次模拟保存，真实模型请求为 0。脚本：`model-settings-multimodal-ui-test.cjs`；结果：`model-settings-multimodal-ui-result.json`。页面 fetch 全部被本地模拟拦截，不调用实际 8765。全应用编译和真实模型测试由主代理收口。

## 资料获取记录

已读 agent-reach 的 SKILL.md、references/search.md 与 references/web.md。按主代理允许，官方 web 搜索/读取为主；Chat API 原网页读取超时后，采用技能中 Jina Reader 回退读取同一官网 URL，保存于 `official-chat-api-jina.md`，摘录 `official-chat-api-excerpts.txt`。仅使用阿里云官方来源，未采用第三方测评推断模型参数。
