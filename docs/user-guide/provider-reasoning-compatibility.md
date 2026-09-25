# 供应商模型与 reasoning 兼容性

更新：2026-09-02
事实来源：`modules/llm-adapter/packages/llm-adapter/src/reasoning.rs` 的 `reasoning_capability_catalog()`。

本文只说明当前内置能力目录，不代表覆盖供应商市场上的全部模型，也不把未做真实请求联调的条目写成“已验证上线”。当前目录固定整理 9 个 provider、39 个 provider/model 条目；其中 5 个 deprecated 只为旧会话兼容保留，新的模型选择器使用其余 34 个条目。

## 先看四类处理

下表按目录条目分组。这里的“原生直发”是 canonical 值可直接放入供应商原生字段；“明确映射”是 resolver 会把请求值转换为供应商协议后再发出。

| 类别 | 条目数 | 当前条目 | wire / 选择行为 |
|---|---:|---|---|
| 原生直发 | 4 | xAI：Grok 4.6、4.5、4.3；智谱：GLM-5.3 | xAI 使用顶层 `reasoning_effort`；GLM-5.3 使用 `thinking.type=enabled` + `reasoning_effort`。Grok 4.5 的旧 `xhigh` 请求兼容映射为 `high`；仅显示目录声明的档位。 |
| 明确映射 | 5 | Anthropic / ClawAPI：Claude Opus 4.6、Sonnet 4.6；DeepSeek：V4 Flash、V4 Pro；智谱：GLM-5.2 | Anthropic 使用 adaptive thinking / `output_config.effort`；DeepSeek V4 的 `medium`、`xhigh` 映射为 `high`；GLM-5.2 的 `minimal`→关闭、`low/medium`→`high`、`xhigh`→`max`。 |
| 仅 `auto` 省略 / 安全关闭 | 25 | Anthropic Haiku 4.5；OpenAI 3 条；智谱旧/基础 6 条；Alibaba Bailian 7 条；Baidu 2 条；ByteDance 2 条；Custom 4 条 | `auto` 不发送 reasoning 覆盖；无已核验可调 effort。目录声明 `none` 的条目只按各自安全关闭语义处理，不能把 `auto` 当成关闭。未知 provider/model 保守只用 `auto` 并省略字段。 |
| 旧会话隐藏 | 5 | xAI：Grok 3、Grok 3 Mini、Grok 2；DeepSeek：`deepseek-chat`、`deepseek-reasoner` | `deprecated=true`，不进入新建模型选择；已有会话仍可读、回显并走兼容 resolver，不扩展新 wire 能力。 |

## 当前 9 个 provider / 39 个目录条目

括号中的数字依次为“目录总数 / 新选择器条数 / 仅旧会话条数”。`auto`、`none` 等是当前 manifest 的 canonical requested 选项，不是把所有供应商都宣称支持这些原生字符串。

| provider_id（显示名） | 条目（目录总数 / 新选 / 旧会话） | 新选择器中的模型与目录选项 |
|---|---:|---|
| `clawapi`（Anthropic / ClawAPI） | 3 / 3 / 0 | Claude Opus 4.6、Claude Sonnet 4.6：`auto/none/low/medium/high/max`；Claude Haiku 4.5：`auto/none`，本切片不伪造 `budget_tokens`。历史会话旧 ID `claude-haiku-4-5-20251213` 只作为显式 alias 兼容，出站 payload 使用官方 ID `claude-haiku-4-5-20251001`，不批量改写持久化 raw。 |
| `openai`（OpenAI） | 3 / 3 / 0 | GPT-4.1、GPT-4.1 Mini、GPT-4o Mini：`auto/none`；按当前 non-reasoning 目录处理，`auto/none` 均省略 reasoning 字段。 |
| `xai`（xAI） | 6 / 3 / 3 | Grok 4.6：`auto/low/medium/high/xhigh`；Grok 4.5：`auto/low/medium/high`；Grok 4.3：`auto/none/low/medium/high`。旧 Grok 3、Grok 3 Mini、Grok 2 只保留旧会话。Grok 4.5 的旧 `xhigh` 请求在 preflight 明确降为 `high`。 |
| `zhipuai`（Zhipu AI） | 8 / 8 / 0 | GLM-5.2：`auto/none/high/max`（有映射）；GLM-5.3：`auto/low/high/max`，thinking 强制开启；GLM-5、GLM-4.7、GLM-4.7 Flash、GLM-4.6、GLM Free：`auto/none`，不宣称可调 effort；GLM-4.6V Flash：仅 `auto`，当前未命中精确关闭 manifest。 |
| `alibaba-bailian`（Alibaba Bailian） | 7 / 7 / 0 | Qwen3.7 Max、Qwen Plus、Qwen Turbo、Qwen Max、GLM-5.2、GLM-5.1、GLM-5：当前只列 `auto`；没有精确协议证据时省略 reasoning 字段。 |
| `baidu`（Baidu Qianfan） | 2 / 2 / 0 | ERNIE 4.5 Turbo 128K、ERNIE X1 Turbo 32K：当前只列 `auto`，保守省略字段。 |
| `bytedance`（ByteDance Ark） | 2 / 2 / 0 | Doubao 1.5 Pro 32K、Doubao 1.5 Lite 32K：当前只列 `auto`，保守省略字段。 |
| `deepseek`（DeepSeek） | 4 / 2 / 2 | DeepSeek V4 Flash、V4 Pro：`auto/none/low/high/max`；`none` 为 `thinking.disabled`，其它明确档位为 `thinking.enabled` + `reasoning_effort`。旧 `deepseek-chat`、`deepseek-reasoner` 只保留 `auto` 和旧会话兼容。 |
| `custom`（Custom OpenAI-compatible） | 4 / 4 / 0 | `custom-model`、`qwen2.5-vl-3b`、`showui`、`llama3.1`：新选择器当前只列 `auto`，未知协议省略字段。已有 Custom 会话的旧 requested（例如 `medium`）保留原值用于兼容回显，resolver 的 effective 可安全回到 `auto`，不会被重建时静默抹掉。 |

合计：3+3+6+8+7+2+2+4+4 = 39；deprecated：3 个旧 Grok + 2 个旧 DeepSeek = 5；新选择器：39-5 = 34。

## requested、effective 与 auto

- `requested` 是用户选择或会话持久化的 canonical 请求值：`auto`、`none`、`minimal`、`low`、`medium`、`high`、`xhigh`、`max`。旧数据读取时会识别旧别名；未知旧值安全回退为 `auto`，但保留 raw 和 legacy fallback 证据。
- `effective` 是当前 provider/model manifest 经过 resolver 后真正采用的语义。原样支持时通常与 requested 相同；明确映射时可能变成另一个档位；不支持时安全回退 `auto` 并省略字段。`downgraded` 仍保留请求可选性，`unsupported` 不应伪装成精确支持。
- `auto` 表示“不主动覆盖供应商默认”，wire 结果是 `Omit`；它不等于 `none`。只有 manifest 明确支持关闭且 resolver 有对应原生协议时，`none` 才表示关闭 thinking。
- Web 前端从 `/api/models/capabilities` 消费 provider/model/reasoning 目录与 preflight 结果，不硬编码供应商矩阵，也不直接拼接 `preflight_wire`。模型目录只列当前代码明确登记的条目。

## 证据边界

本目录是实现事实与保守兼容策略的单一来源；截至 P6/P7 当前记录，没有完成 9 个供应商的真实请求联调，因此未知条目仍标记为 `unknown`，不能据此宣称生产端到端验证已完成。Anthropic 4.6 Opus/Sonnet 的 effort 说明参见[官方 effort 文档](https://platform.claude.com/docs/en/build-with-claude/effort)；DeepSeek V4 的 thinking 兼容说明参见[官方 thinking mode 文档](https://api-docs.deepseek.com/guides/thinking_mode/)。
