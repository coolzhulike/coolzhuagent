# ChatGPT 订阅登录接入调研与最小连通实验

日期：2026-10-08。范围仅为官方机制核实、与第三方方案比较及一次无工具会话连通实验；没有把实验脚本作为正式 Provider 发布。

## 结论与架构选择

存在官方适用于开源、本地部署应用的接入路径：Sign in with ChatGPT 的 ChatGPT plan usage。Coolzhu Agent 可以使用自身身份动态注册，获得用户授权后调用符合条件的 Responses API 请求。它与 API Key 计费分开，也不会读取用户在 ChatGPT 中的聊天记录或记忆。商业或远程托管产品的适用条件需要另行确认，不能直接套用本地开源路径。[官方概览](https://developers.openai.com/siwc/token-sharing-open-source)

建议后续正式集成直接扩展当前模型适配器的认证和 Responses 流式协议，让 Coolzhu 继续拥有上下文、SKILL、插件和本地工具执行循环。另起 Codex app-server 循环会扩大本次需求范围，暂不采用。保留本地能力的判断来自协议与现有架构分析，尚未通过本次无工具实验验证。[官方推理协议](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)

## 第三方实际方案

| 产品 | 一手资料确认的机制 | 对 Coolzhu 的参考价值 |
| --- | --- | --- |
| Devin | 用户连接符合条件的 ChatGPT 订阅；对应 OpenAI 模型消耗订阅额度，其它模型仍走 Devin 额度。文章没有公开完整认证实现。 | 参考账号连接与额度展示；不可推断其商业客户端注册可直接复用。 |
| OpenCode | 文档提供 ChatGPT Plus/Pro 登录。所查 dev 分支 `plugin/openai/codex.ts` 使用 PKCE、令牌刷新、固定客户端 ID，并把推理请求转向 Codex 后端端点。 | 参考登录生命周期与 SSE 转换；本项目采用新的官方独立注册及公共 Responses 路径，不复制第三方客户端身份。 |

来源：[Devin 官方说明](https://devin.ai/blog/sign-in-with-chatgpt)、[OpenCode Provider 文档](https://opencode.ai/docs/providers)、[OpenCode 认证适配源码](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/plugin/openai/codex.ts)。源码观察属于 2026-10-08 的 dev 分支快照，后续可能变化。

## 官方最小链路

1. 本地安装生成并保存稳定、不含个人身份的 host ID；首次登录使用动态注册入口和应用名称 Coolzhu Agent。
2. 启动 loopback 回调监听，再发起带 PKCE S256、state、nonce 的浏览器授权。使用回调发放的客户端 ID 交换令牌；验证 ID Token 签名、issuer、audience、期限及 nonce，并确认订阅调用 scope。
3. 使用 OAuth access token 查询模型目录；只使用服务端列出的可选模型。
4. 发送一条 `store:false`、`stream:true` 的 Responses 请求；收到 `response.completed` 且文本匹配实验标记才算成功。仅 HTTP 200 不构成通过。

来源：[官方注册与登录](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)、[模型及推理](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)。

## 与现有配置的差异

- 订阅授权不是在 API Key 框填入随机字符串或现有 Codex 凭据。正式集成应有独立账号状态和凭据存储；本次实验令牌仅在内存使用，不保存刷新令牌。
- 不能直接沿用所有 Chat Completions 参数。当前预览对采样参数、输出长度、系统消息和工具声明有额外限制；需要专用参数映射与显式能力声明。
- 托管工具与本地工具不同。Coolzhu 的 CU、Browser、SKILL 与插件需经自身工具协议适配后另验，不能因文本连通即宣称全部可用。
- 不自动转付费 API，也不接管 ChatGPT 的原聊天上下文。

来源：[官方预览限制](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations)。

## 本次实验状态

隔离脚本位于被忽略的 `tmp/2026-10-08-chatgpt-research/siwc-connectivity.py`。已完成语法检查、真实本地监听启动及授权入口准备。脚本不读取现有 Codex 登录文件、不记录回调 code/token，不向仓库写入凭据；只准备一次无工具文本推理。

首次记录为等待授权、推理0次。随后用户完成页面授权，独立 Coolzhu Agent 身份校验及订阅权限确认通过，令牌交换和模型目录均返回200。目录包含7个可用模型；使用 `gpt-6.1-sol` 仅发送1次无工具请求，收到 `response.completed` 和完全匹配的 `COOLZHU-CONNECTED-ba33bf6c`，用量为输入27、输出16、合计43 tokens。**最简单真实会话连通通过**，没有持久化凭据。脱敏证据见[实验结果](../../testing/2026-10-08-ui-preview/chatgpt-subscription-connectivity-result.json)。实验结束后不继续发送请求；这不构成正式Provider或工具链验收。

## 后续验收与交付边界

本需求只要求最简单连通：完成一次明确授权、模型目录成功、单条推理 `response.completed` 和预期文本一致即可。未授权、拒绝、额度不足、协议拒绝分别记录，不能用其它客户端凭据或私有端点绕过。正式 Provider UI、持久凭据刷新、工具与多模态属于后续独立实现范围。
