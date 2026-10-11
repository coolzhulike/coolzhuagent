# 同批工具取消身份与 CLI 宿主审查

新增真实 ConversationRuntime 两工具响应回归：首个审批处理时取消根令牌并返回 Allow，确认审批器仅一次调用、目标执行器零调用、first/second 两个 tool_use_id 分别保留错误结果，且不请求下一轮模型。该回归使用本地模型协议夹具，不冒充外部模型正式验收。

离线 core-runtime 构建实际退出 0；conversation::tests 实际退出 0，17 通过、0 失败、348 过滤。日志 tmp/analysis-conversation-batch-cancel-build.txt 与 tmp/analysis-conversation-batch-cancel-tests.txt。

宿主审查定位到 CLI main.rs 的 run_turn、run_prompt_json 与另一独立 prompt 路径直接调用 ConversationRuntime::run_turn；在 modules/cli 源码检索没有发现 with_execution_control。不能据此证明 CLI 拥有根取消令牌。文本 run_turn 仅成功时 persist_session，错误时直接返回；JSON 路径通过 ? 提前返回，临时 runtime 不回写持久会话。因此取消保留的内存工具事实尚不等于 CLI 错误路径已持久保存，需要进一步实现/审查恢复规则。

主控制台源码未检出直接 ConversationRuntime::run_turn 调用，不能把本轮 Conversation 回归扩称主控制台实际模型循环已验证。现有 web 工具监督器采用另一控制路径。32WBS 全部入口、持久终态与正式运行矩阵仍保持开放，实装 0.2.128 不含这些候选。
