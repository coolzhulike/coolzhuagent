# Devin 官方能力核对与暂缓决定（2026-09-30）

用户要求接入会话能完整使用 coolzhuagent 的 SKILL、插件、工程/聊天室上下文和记忆管理；若无完整方案，暂缓该插件开发，继续既有回归。主会话核对官方资料，未使用子代理实施或测试。

## 官方已提供的能力

| 路线 | 官方能力 | 本项目仍需自行适配的部分 |
|---|---|---|
| Cloud v3 Sessions REST API | 创建、发送消息、查询远端会话 | 不是本地模型工具调用循环；不会自动获得本地 SKILL、插件和记忆 |
| Cloud custom MCP | Devin 可以连接自定义 MCP；stdio 在云环境运行，HTTP 服务须云环境可达 | 本机服务可达性、作用域、工具执行权限、上下文/记忆桥接 |
| 本地 Devin CLI ACP | stdio JSON-RPC、模型选择、认证、会话交互；官方演示 Zed 等宿主 | coolzhu 的 ACP 客户端、生命周期、取消/断连、聊天室/工程绑定 |
| CLI MCP / SKILL | 本地 stdio/HTTP MCP；支持 `.agents/skills` 与 SKILL.md | 本项目技能目录/授权语义、插件格式与工具名、执行回执映射 |
| CLI hooks | 上下文注入、工具阻断/改写、开始/结束/压缩事件 | 本项目记忆读写与压缩策略、失败关闭、权限与轨迹一致性 |

官方资料：

- [Cloud MCP](https://docs.devin.ai/work-with-devin/mcp)
- [CLI ACP / Commands](https://docs.devin.ai/cli/reference/commands)
- [Zed ACP 集成](https://docs.devin.ai/cli/acp/zed)
- [CLI MCP 配置](https://docs.devin.ai/cli/extensibility/mcp/configuration)
- [CLI SKILL](https://docs.devin.ai/cli/extensibility/skills/overview)
- [Hooks](https://docs.devin.ai/cli/extensibility/hooks/overview) 与 [生命周期](https://docs.devin.ai/cli/extensibility/hooks/lifecycle-hooks)
- [CLI 权限](https://docs.devin.ai/cli/reference/permissions)

## 判定

官方提供了 **ACP + 本地 MCP + hooks 的二次开发基础路线**，不能断言 Devin 完全不支持外部工具或技能。官方资料未给出可直接继承 coolzhuagent 本地运行时语义的完整适配方案。当前 REST 云会话插件也没有实现完整集成。

潜在路线必须先解决两套 Agent 的执行归属：Devin 内建文件/命令工具不能绕过本地权限与轨迹；工程切换不能沿用旧聊天室/记忆作用域；压缩、停止、断连和重试必须保持上下文与记忆一致。Hooks 的普通非零退出仅记录错误，不阻断动作，不能直接当作本项目失败关闭的权限门禁。CLI ACP 的认证来源也不等同 Cloud v3 的服务 API Key；不能承诺填原云 API Key 即可完成 CLI 接入。以上是基于官方协议边界对本项目作出的架构判断，并非官方宣称无法实现。

按用户条件暂缓开发。PR #72 保持独立草稿；含 Devin 的 0.2.30 仅为内部构建候选，不交付、不安装、不并入既有回归。保留代码和已有证据以便将来重新评估，后续回归分支从不含 Devin 的主线继续。

## 恢复开发的条件

先形成 ACP 客户端与本地 MCP 桥接的独立设计，明确技能授权、插件兼容、权限归属和记忆生命周期；真实验证跨工程隔离、停止/恢复、压缩及内建工具访问。未满足这些条件前，不标记为完整 Provider，也不以云会话创建成功替代完整 Agent 验收。
