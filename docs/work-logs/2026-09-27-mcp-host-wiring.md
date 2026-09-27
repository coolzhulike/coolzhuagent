# 2026-09-27 MCP stdio 宿主与模型工具闭环

本轮源码构建接通了 MCP stdio 服务的连接、发现、工具调用、权限审批和模型工具往返。它是隔离源码构建的验证，不属于用户当前已安装的 0.2.17 程序验收；原安装版窗口与用户运行库未重启、未清理。

## 改动与边界

- `core-runtime/mcp_stdio.rs` 使用 MCP stdio 的逐行 JSON-RPC，完成 `initialize` 后发送 `notifications/initialized`。连接绑定工作目录，受管进程按超时、取消或断线回收；工具分页有页数与总数上限，归一化重名和重复 cursor 会拒绝，不能留下半份工具索引。底层旧 Content-Length 方法仅作为单独低层接口保留，默认 MCP 请求不走它。
- Web 宿主按工作区和服务配置连接，只接 `enabled` 的 `stdio` 服务；每个连接有代际与串行请求锁，断线或重连后旧模型调用、旧审批记录不能落到新进程。手动 UI 调用使用当前会话与聊天室权限上下文，模型调用使用聊天接纳时冻结的真实目标 Agent、父运行、取消与 Goal 身份。已发现的 MCP 工具按既有 allowlist 加入模型定义；运行时仍以 `DangerFullAccess` 走统一门禁。
- 小上下文（≤16,384）原有筛选会删光 MCP 工具。现在仅把已发现、已由上游 allowlist 筛过且连接仍有效的定义作为候选，优先保留本轮已经调用的名称，再按名称排序；最多 4 个、完整序列化定义最多 4096 字节、估算最多 1024 token，并且不能超过实际剩余 prompt 容量。余量扣除了输出、安全预留、system、文本与图片、已选内置工具 schema。候选被裁剪时留下数量和预算诊断；未知 `mcp__` 前缀不能借此进入列表。
- 当前运行宿主只实现 stdio；配置中的 SSE/HTTP/WebSocket/OAuth 是兼容读写字段，并未接通对应执行栈，UI 会标为不支持。core 的资源 list/read 低层方法已存在，本轮 Web/模型工具闭环不把资源方法冒称为已上线。

## 验证

本地无账号的官方 `@modelcontextprotocol/server-everything` **2026.1.26** 经标准 stdio 协议实际连接。core 层官方服务互通和拒绝路径、Web 生产 dispatcher 的双目标 A/B 与子 Agent/旧审批回放定向测试均通过。后者以构造的父模型上下文调用真实官方进程，并非真实模型请求。

独立 HTTP 端到端另起隔离工作区、本地假 OpenAI 协议模型和官方 SDK 进程。Web `/chat/send` 接纳管理会话 A 向发送目标 B 发起请求；首轮返回正式 `tool_calls` 调用 `mcp__official__echo`，Web 执行后把 `Echo: web-chat-e2e` 放进第二轮模型请求，最终正文 `MCP-WEB-CHAT-OK` 返回客户端。该夹具在隔离目录启用开发权限，只证明接线/往返；审批拒绝与旧回放由上述独立门禁测试证明，不把低层 echo 或开发权限的往返说成真实用户授权通过。无外部云模型调用。

| 生效 context / 输出上限 | 请求 `max_tokens` | 首轮 MCP / 总工具 | 反馈与正文 | 证据 |
| --- | ---: | ---: | --- | --- |
| 自定义模型默认 64000 / 64000 | 32000 | 13 / 33 | 均通过 | [默认模型容量](../testing/release-0.2.17/evidence/mcp-source-e2/custom-default-64000-64000.json) |
| 显式 65536 / 2048 | 2048 | 13 / 33 | 均通过 | [大窗口](../testing/release-0.2.17/evidence/mcp-source-e2/large-window-65536-2048.json) |
| 显式 8192 / 2048 | 2048 | 4 / 14 | 均通过 | [小窗口](../testing/release-0.2.17/evidence/mcp-source-e2/small-window-8192-2048.json) |

首次测试的无动作措辞受原有工具意图门控，首轮定义为空；之后动作措辞在旧小窗口硬白名单下仍没有 MCP 定义，原始脚本报错为“**接纳请求未提供已发现 MCP 工具定义**”，反馈未进入模型第二轮。[修复前结果](../testing/release-0.2.17/evidence/mcp-source-e2/before-fix-no-mcp-definitions.json) 保留了 `first_tool_names=[]`、`tool_count=0` 的事实。此后才添加受实际 prompt 余量约束的小窗口候选，不能把先前失败抹成通过。

已运行 Web 离线构建、Web 小窗口预算单测、Node 前端语法与差异检查。隔离服务的 `web_sha256`、会话身份、两轮请求形状和结果写在上述 JSON；测试脚本与完整诊断日志保留在 `tmp/mcp_model_e2e.py` 和 `tmp/mcp-model-e2e-*.log`，不会接触用户工作区。后续正式打包仍需在合并其它并行源码改动后统一构建与验收。
