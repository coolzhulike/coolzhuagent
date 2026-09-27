# 0.2.19 运行事件覆盖核账

2026-09-27。本记录核对旧的 `tmp/2026-09-27-current-32-package-gates.md` 门槛表中 0.2、1.1、2.1、2.2、2.3、2.5 六项，并新增一组**已安装 0.2.19 二进制副本的隔离运行**证据。公开的工作包范围见[整合改进执行计划](../../analysis/2026-09-21-integration-review/04-coolzhuagent-整合改进执行计划.md)，当前状态入口见[实施状态与改动清单](../../analysis/2026-09-21-integration-review/implementation-status-and-change-inventory.md)。旧临时门槛表写于 0.2.18 安装后、0.2.19 窄修前；其中已有证据不能因为旧表仍写“待验”而抹去，剩余门槛也不能因为本组受控夹具通过而自动关闭。本记录不是原生桌面截图验收。

| WBS | 已有事实，旧“待验”应如何读 | 本轮之后仍未覆盖 |
| --- | --- | --- |
| 0.2 | 受控模型、迁移和输入契约已有定向用例；后续实际安装运行链也证明部分契约可用。 | 黄金 fixture 覆盖清单，以及历史失败与当前重放结果的逐项映射；总测试通过数不能代替它。 |
| 1.1 | 0.2.18 安装版副本已实际跑通 CLI、Goal、MCP；本轮又在 0.2.19 副本上核对 CLI 和直接 Web 的同一 Agent 夹具。 | 旧字段回放、迟到事实在更多入口之间的对账；本轮没有构造迟到或失败重放。 |
| 2.1 | [0.2.18 原生模型聊天](../release-0.2.18/installed-model-chat-acceptance.md)已跑通 Qwen 文本和原图识别；本地仅思考响应的最终正文与轨迹也分别验过。[0.2.19 功能报告](../release-0.2.19/functional-verification-and-test-design.md)沿用该证据。 | 完整 UI 保存设置到实际云请求的参数矩阵与 Key 来源逐项复验；本地假模型不能证明云端所有参数或视觉协议。 |
| 2.2 | [0.2.18 安装版隔离结果](installed-0.2.18-isolated-e2e.md)有视觉转述和文本同 run 的两次请求用量、CLI 子请求单次用量；本轮 0.2.19 再核对子请求、父首轮、工具反馈各一条及不同 attempt。 | 多轮、迟到、嵌套请求和 UI 轨迹逐项对账；本轮不是视觉请求，也未覆盖迟到事实。 |
| 2.3 | 0.2.18 安装版副本已跑通官方 SDK stdio 服务、模型工具调用与反馈闭环；已有重名/游标定向测试。本轮 Agent 夹具没有调用 MCP。 | 并发取消、hook 重排、动态工具重名的运行时负例，以及非 stdio transport 的产品范围，均不能由本轮 Agent 结果推定。 |
| 2.5 | 0.2.18 已有 CLI→Web→子 Agent 的单入口成功；本轮用同一受控 Agent 夹具对照 0.2.19 **直接 Web 非流式**和**CLI 流式**：模型请求、轨迹用途、用量和 attempt 归属一致。 | 这不是逐条 SSE 事件等价，也未覆盖 Web 浏览器流式入口、真实云提供商、长多轮与迟到响应；两入口的子结果文本投影不同，见下文。 |

本轮先核对安装目录二进制哈希，再把 Web 与 CLI 的**相同字节副本**放入新 `tmp/` 工程。Web SHA256 为 `3BD24917095B44ED856A6087151A0ACB3029E77A6BEC09E47E345179C5AAD042`，CLI 为 `98BAE8DD1573628B1D84B82436AEB4638E45224581172856F11B4A08B4C87590`。本地模型夹具、Web 各绑定随机 `127.0.0.1` 端口，运行目录和 AppData 均隔离；没有连接 8765、读取或输出真实 Key、修改用户配置，也没有操作原生桌面。两个独立房间使用同一 Agent 提示和同一夹具响应；内部 run/turn ID 本来各异，因此只核对各自真实归属，不强求 ID 相同。

| 观测项 | 直接 Web `/api/chat/send` | CLI 聊天流式入口 |
| --- | --- | --- |
| 结果 | `completed`，父最终正文出现 | CLI 退出 0，`done=completed`，父最终正文出现 |
| 模型请求 | 3 次；provider `stream` 依次为 `false,false,false` | 3 次；provider `stream` 依次为 `true,false,false` |
| 轨迹用途 | `chat`、`child_agent`、`tool_feedback` 各一次 | 同左，各一次 |
| 用量 | `chat 5/3`、`child_agent 11/7`、`tool_feedback 6/4`，均 `completed` | 同左 |
| 归属 | 三条均有独立 attempt，且 room 匹配当前房间 | 同左 |
| 返回文本投影 | HTTP `messages` 为 user 与父 `assistant-reply`，未含子结果标记 | CLI SSE 中能看到子结果和父结果标记 |

这里的“三次请求”包括子模型请求和工具反馈后的父模型请求；CLI 仅首个 provider 请求的 `stream=true`，后两次为 `false`。已验证的是持久轨迹/用量和实际请求形状的对应关系，**不是**两种入口逐 SSE 帧一致。CLI 中出现子结果标记只证明本次流式输出有该事件，不能据此说它也成为主聊天的永久 assistant 消息；直接 Web 的 HTTP 返回没有子结果标记，是否需要统一展示属于另一个界面/响应契约问题，本轮没有改产品来迎合夹具。

执行共三轮。前两轮的原始夹具断言错误地要求直接 Web 的 `messages` 必须含子结果标记，因此均在 `nonstream_web` 检查处失败；第二轮 `diagnostic.json` 已显示两入口的请求、轨迹、用量满足上述对照，仅该文本投影断言不成立。修正断言为“两入口父最终结果存在、CLI 流事件含子结果，轨迹/用量一致”后，第三轮退出 0。失败日志分别在 `tmp/runtime-event-compare-019.log`、`tmp/runtime-event-compare-019-second.log`；通过日志在 `tmp/runtime-event-compare-019-third.log`，原始结果在 `tmp/runtime-event-019-40a0705028/result.json`，[脱敏机读结果](runtime-event-coverage-evidence-sanitized.json)仅留请求形状、计数、状态和二进制身份。每轮测试脚本都在 `finally` 中终止并等待自身 Web 进程、关闭自身假模型服务；没有清理或重启用户实例。

本结论只对应上述 **0.2.19 安装二进制副本的隔离运行**。随后针对 LSP 房间授权的源码补丁和开发构建不在该安装身份内，不能用本报告替代其安装验收。
