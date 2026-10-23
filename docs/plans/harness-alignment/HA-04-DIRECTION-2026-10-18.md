# HA-04 继续实施决策

日期：2026-10-18（America/Los_Angeles）。本次确定后续实施方向，保留 60 条验收合同。台账仍为 HA-00 至 HA-03 accepted、19/60，HA-04 未验收；本次没有业务修改或新增行为测试结果。

**先让响应解释与工具派发真正消费原发送请求，再验证发送内容、身份、撤销和兼容性。保留 `started` 的运行接纳含义，以独立 `request_bound` 事件报告模型请求绑定。** 该选择对应 Pro 原文发现 2 和 T01，不要求为了事件名称重排整个 SSE 启动流程。

## 源码核验与方向修正

下表行号仅用于定位本次工作区；接替时按符号重新定位。`W` 指 `modules/gui-web/packages/web-console/src/`，`A` 指 `modules/llm-adapter/packages/llm-adapter/src/`。

| 反馈或现状 | 核验结果 | 决策 |
|---|---|---|
| 必须在 `started` 内带绑定 | `W/main.rs::api_chat_send_stream` 约 19039 行的 `started` 表示运行已接纳。HA04-01 要求按原发送集合解释响应，没有指定事件名称；Pro 原文发现 2 指的是进入共享循环的首个模型响应 | 保留早期接纳事件。HTTP 请求在实际发送前确定绑定，进入响应处理时携带它；若向客户端报告，`request_bound` 先于该请求的内容/工具事件 |
| 当前 SSE 已发 `request_bound` | `stream_agent_model` 返回绑定，约 19288 行只发出事件；约 19550 行给共享循环传的是 `(initial, usage_known)`，没有原请求绑定 | 事件只是观测，不能作为 HA04-01 已满足的证明；优先补齐后端的数据传递和消费 |
| 共享循环校验工具暴露面 | `run_shared_model_tool_loop` 约 27940–28060 行先重新构造请求，再消费 `initial_response`，随后用重构请求的 `request.tools` 校验 | 首响应必须使用实际发送时的工具集合/能力对象。当前是明确的数据链缺口；尚不能据此宣称已经复现越权派发 |
| 最终 `MessageRequest` 的参数摘要已经完整 | `W/request_binding.rs` 只摘要 model/max_tokens/reasoning_effort/stream/tool_choice；`provider_client_for_agent` 另设 temperature/top_p/reasoning_mode/thinking_budget。`A/providers/openai_compat.rs::send_request` 还会做 provider 投影、参数应用及 response_format 应用 | HA04-02 与 HA04-01 一起解决：以实际序列化后的工具和有效参数为对照，不把配置输入或早期对象摘要当成最终 wire 摘要 |
| retry 的 request/attempt 单测足够 | `W/request_usage.rs` 已生成 logical_request_id，并用 observer 的 attempt_no 写入真实 attempt；`A/request_observer.rs` 已区分 SDK 调用生命周期与 HTTP 重试 | 复用这条真实身份链。当前 binding 的手写 attempt 和 `source_request_key` 应明确关联到它，不再造第三套互不相连的计数 |
| ACP/订阅返回 `None` 就必须补 HTTP binding | 两条路径走独立协议，未构造 HTTP `MessageRequest`。HA04-05 要求受影响共享接口回归及能力边界保持真实 | 按各协议的请求/尝试、权限与收尾机制验证；`None` 既不是通过证据，也不自动代表需要移植 HTTP 对象 |
| HA-05 被 HA-04 依赖卡住 | PLAN 与 state 均为 HA-05 → HA-00；HA-09 也只依赖 HA-00 | 优先收敛 HA-04。若出现实际环境阻塞，可单执行者切换 HA-05 或 HA-09，写清检查点；HA-06/07 及其依赖仍遵守原图 |

来源：[Pro 审核原文](../../analysis/2026-10-17-gpt6-pro-harness-review-original.md)、[验收合同](ACCEPTANCE.md)、[当前部分实施记录](evidence/HA-04/request-binding.md)。部分实施记录保留为历史证据，其中把 `started` 当成必须载体的解释由本决策澄清，原测试回执和哈希不改写。

## 实施顺序及逐项完成标准

以下步骤细化已有 HA04-01 至 HA04-05，不新增验收编号，也不以源码阅读代替行为证据。

| 顺序 | 实施内容 | 对应验收 | 完成时必须交付的证据 |
|---|---|---|---|
| 1 | 通过返回值/已有上下文传递原发送请求的最小绑定与能力引用；SSE 首响应进入共享循环后直接消费它；普通入口沿同一语义处理。后续实际请求各自绑定，未发送的重构对象不登记为已发送请求 | HA04-01 | 普通与 SSE 两入口均执行“服务端收到请求 → 暂停响应 → 修改下一步工具设置 → 返回旧响应”。旧响应只能引用原发送集合；事后新增工具不能追溯成为可调用项。保存实际请求、绑定、响应解释与工具效果计数；另证明下一实际请求按届时配置及已有能力上限构造 |
| 2 | 摘要与 adapter 最终序列化内容对齐，复用已有参数转换；区分 tools 的协议表示和宿主调用映射 | HA04-02 | 本地 HTTP 服务记录真正收到的测试请求体，逐字段对照工具/schema、tool_choice、模型及实际发送的生成/推理参数，覆盖 provider 会删除、改名、转换的字段。普通与 SSE 均覆盖；正文和凭据不用作持久摘要，测试仅用虚构输入 |
| 3 | 关联既有 logical_request_id、实际 attempt 和来源请求；区分传输重试、重新采样及子请求 | HA04-03 | 真实 adapter 走本地 HTTP 429/503→成功及耗尽路径；记录同一逻辑请求的多个 attempt、原始人类输入接纳次数为 1。另用真实调用入口触发有限重新采样/子请求，证明新请求身份、父关联、能力上限及继承的根预算有效，重复处理不会重放工具效果 |
| 4 | 用请求在途屏障验证已有执行门，按失败结果修复必要接缝 | HA04-04 | 记录“请求已收到 → 撤销/停用提交成功 → 释放模型工具响应 → 宿主拒绝”的可排序事件；受测动作尚未开始时，工具进入计数和副作用计数均为 0，并有宿主拒绝回执。分别覆盖权限撤销与工具停用，另设未撤销的可执行对照 |
| 5 | 回归 CLI、Devin ACP、订阅路径受影响接口，保持协议自身身份与能力边界 | HA04-05 | 每条路径提供定向测试或真实不支持响应；验证身份关联、取消/权限及收尾没有退化。订阅不支持原生工具的路径仍明确拒绝，不能仅凭 `None`、HTTP 测试通过或未报错关闭该条 |

第 1、2 步是同一个小批改动的两种证明，应共享必要的测试夹具和构建结果，不按编号重复编译。没有必要把所有目标 agent 的最终请求提前构造：接力请求可能依赖前一 agent 的响应。

### 请求与身份的最小实现边界

优先复用最终 `MessageRequest`、已冻结能力对象和现有请求观察器，通过类型化返回值传递关联。用于响应解释的是原工具集合及必要映射；一个 SHA-256 字符串不能代替该集合。摘要用于核对和追踪，不授予权限。

`request_usage` 的 logical_request_id 在同一次 SDK 请求的 HTTP 重试中保持稳定，attempt 使用真实发送次数。重新采样即使请求体相同，也必须具有独立逻辑身份并关联来源；摘要相同不等于同一次请求。子请求保留父关联和能力上限，不复制或重置根预算。将 `tool_invocation_identity::source_request_key` 与这条链建立可核对的关联，保留既有执行去重语义，不直接更换旧执行 ID 后重放动作。

当前 `request_binding::PENDING` 是按 turn_id 保存一条记录的全局映射，当前共享循环也只在 round 0 记录。优先让必要结果随请求/响应流转；不要扩张此映射为新的全局权限或执行状态库。需要保留的观测关联可挂接现有运行/用量记录，不为刷新界面另起持久化框架。

最终参数摘要应来自实际应用协议转换和覆盖后的有效字段；配置存在但未发送的字段不能冒充 wire 参数。HTTP 两类协议按受影响范围回归。每个实际请求做必要的摘要和关联即可，不对每个 token 重算、重新查询配置或调用额外审核模型。

### 测试接缝与隔离

已有可复用入口包括 `A/request_observer.rs::RequestObservation`、`W/request_usage.rs::LedgerObserver`，以及 `modules/llm-adapter/packages/llm-adapter/tests/client_integration.rs` 的 `retries_retryable_failures_before_succeeding`、`surfaces_retry_exhaustion_for_persistent_retryable_errors` 和本地 `TcpListener` 夹具。原 adapter 测试单独通过还不能证明 Web 输入接纳只发生一次，需在入口到 adapter 的组合路径补计数。

屏障用明确通知/确认事件控制顺序，不用固定 sleep 猜测请求已在途。沿 HA-00 已保存的隔离方案使用临时 workspace、独立 DB/state/log 与动态空闲端口；不操作 8765 的日常实例。HA-04 的确定性协议测试可由本地模拟 provider 驱动真实 adapter/共享循环/宿主执行门，不依赖付费模型；它们不代替 HA-11 的真实模型质量和性能对照。

## 接替检查点

`state.json` 只更新方向说明及下一动作，保持现有条目状态、证据引用和授权范围。下一执行者先读取本页、state 和 HA-04 合同，核实上一会话已停止及没有在途命令，再从第 1 步继续。

每完成一个可验证改动，记录修改文件、对应源码版本、实际测试名/计数/退出码、已观察断言和下一动作；受影响 crate 按 AGENT.md 执行离线构建。失败或资源中断保持未验收。相关原始输出复制到 `evidence/HA-04/` 后再登记通过，不能把 `tmp/` 当成已通过条目的唯一长期证据。

自动推进 12 轮结束属于该次执行的停止条件，不表示 HA-04 的业务依赖无法满足。用户或平台发起接替后，新实施会话按自身可用额度承接现有进度；不能把接替当作重置未完业务运行的根预算、执行身份或效果记录的理由，也不自行提高自动轮数。HA-04 的五条全部取得有效证据后才能 accepted；独立的 HA-05/09 不需要等待它。最终 `--complete` 仍必须满足 13 包、60 条和原有收口条件。
