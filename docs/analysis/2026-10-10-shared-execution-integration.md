# 共享执行服务与右栏执行页：模块交付和接线

基线：`40d3598471b8ae1ed88ce9c018e5d06f6442fb31`。本会话只开发独立模块，不访问正式服务、真实库或云端，不安装/发布。所有软件真实验收待总控统一执行。

## 实施前代码事实与风险

- `host_child_agent.rs` 原有 HostAgentRunner 已复用父模型快照、受保护 ProviderClient、权限交集、工具派发、根预算和 request_usage；原随机 child ID 只在回复中出现，没有持久执行索引。模型覆盖已经接受精确会话 ID，也兼容唯一模型名。不能复制第二套会话或凭据。
- `scheduled_jobs.rs` 已按工程/任务/到期时刻持久领取；重启后的 running/unknown 不重放。`scheduled_delivery.rs` 原 poll 已走 `api_chat_send`，按显式房间投递，旧配置才使用原系统房间；新的执行页不能把配置列表当执行事实。
- `agent_session_backend.rs` 中 Devin ACP 内部规划有冻结父运行和工具桥契约，但这不等于子 Agent、Goal 或 Relay 可以使用。Cloud 也没有共享远程委派。保留所有既有拒绝。
- 取消子 Agent 若复用父 token，会连带停止整个聊天室。因此子执行使用本地 token，父取消/预算向下传播；工具派发优先使用子执行 token。请求停止后可能已有远端请求或物理效果，不能以停止等待声称撤销效果。
- 用量不能取模型配置容量或 context 估算，不能把未知当零；同一父运行的子调用按真实 call_id 分账，定时 poll 从实际聊天 run 读取。

## 已落地模块

`execution_service.rs` 的 ExecutionService 持久保存执行来源、模型和会话、父运行/父执行、工具调用、时间、结果和状态。表 `execution_index` 位于原 session SQLite 内；它是关联索引，不复制会话、记忆、凭据、模型请求台账或调度计划。执行 ID 写入成功后才开始请求；重复 ID 拒绝重放，future Drop 保留 outcome_unknown。

TurnRunner 负责共享生命周期，不 spawn 新作用域、不写新的模型循环。HostAgentRunner 用它执行原工具循环；定时 poll 用它执行原聊天入口。聊天接纳 hook 将真实 run 及实际取消 token 绑定到执行索引。结果仍落原固定房间，后台不改 active room。

子执行继承冻结父房间历史和模型自己的既有 memory beads；模型覆盖使用已有会话配置和凭据，不读取另一个房间的历史。父调用、根预算和权限仍来自原 FrozenParentContext。结果返回包含 executionId，并继续有 parentRunId/parentToolCallId 等既有字段。

Provider 契约按入口区分：HTTP 子执行须已解析出原 ProviderClient 和父模型快照；HTTP poll 走原聊天能力检查；ACP 只允许原已实现 poll 聊天入口，子执行明确拒绝；Cloud 两种入口都明确拒绝。这是代码入口支持，**没有新增任何 provider 的真实模型验收结论**。OpenCode/HF/ChatGPT订阅的能力不能由 HTTP 分类推断。

`execution_panel.js/css` 独立页面统一读取两类执行索引，可筛来源、看模型/会话/父任务/状态/耗时/结果/真实 token，向存活 worker 请求停止，查看对应原房间的真实运行轨迹和请求账目。动态内容用 textContent。沿用玉石竹林和卷轴卡片。原定时配置不变；无真实执行时展示空状态，不补造历史运行。静态资源已登记在原生产资源契约内。

## 总控最小入口接线

本会话在独立 checkout 临时接线用于编译，提交不修改四个大入口。总控 cherry-pick 模块提交后应用以下精确入口变更：

1. `src/main.rs` 原 `mod scheduled_execution;` 前增加 `mod execution_service;`。
2. Router 原 `/api/runs/{run_id}/interrupt` 附近增加：

```rust
.route("/api/executions", get(execution_service::api_list))
.route("/api/executions/{id}", get(execution_service::api_detail))
.route("/api/executions/{id}/trace", get(execution_service::api_trace))
.route("/api/executions/{id}/cancel", post(execution_service::api_cancel))
```

3. `index.html` 的样式引用增加 `./src/execution_panel.css`，app.js 执行前引入 `./src/execution_panel.js`（defer）。右栏 tablist 增加：

```html
<button id="chat-right-tab-executions" class="chat-right-tab" type="button" role="tab" data-right-tab="executions" aria-selected="false" aria-controls="chat-right-panel-executions" tabindex="-1"><img src="./assets/icons-wuxia/tasks.svg" alt="" aria-hidden="true" /><span>执行</span></button>
```

在 `data-role="chat-right-panels"` 内增加：

```html
<section id="chat-right-panel-executions" class="chat-right-panel execution-panel" data-right-panel="executions" role="tabpanel" aria-labelledby="chat-right-tab-executions" tabindex="0" hidden></section>
```

4. `app.js` 第222行 `CHAT_RIGHT_RAIL_TABS` 增加 `executions`。第11811行 `initializeChatRightRailTabs()` 在已绑定判定之后调用一次：

```js
window.CoolzhuExecutionPanel?.mount(document.querySelector('#chat-right-panel-executions'), { getScope: () => projectRequestScope() });
```

现有 setChatRightRailTab 根据 data-right-tab/panel 执行显隐和键盘操作，无需新建另一套 tab 控制。第11483行 `CHAT_RIGHT_RAIL_HEADING_META` 增加 `executions: Object.freeze({ kicker: "执行卷轴", title: "子 Agent 与定时执行" })`。保留原协作、任务、状态和工具页。行号以冻结基线为准，集成后需重新核对。

模型作为工具的调用沿用原 Agent schema，例如 `{"description":"审查本次改动","prompt":"…","model":"接纳时已解析的会话ID","allowed_tools":["read_file"]}`。无需新增绕过权限的匿名 HTTP 模型工具。

完整后端入口补丁为同目录 `2026-10-10-shared-execution-main-wiring.patch`，其中还在已有四处 `retain_admitted_mcp_definitions(&mut request, parent...)` 之后调用 `host_child_agent::bind_session_tool_catalog`。它仅修改已开放的Agent定义，列出接纳时真实解析成功的模型会话ID，不新增或提升工具权限。列表返回结果预览和最近8个attempt，详情API返回完整结果及全部真实账目，token汇总始终基于所有本执行attempt。

## 尚未闭环的边界（不要把本模块算作完整 WBS 通过）

- 目前共享索引覆盖 **Agent 会话工具 + scheduled poll**。Goal 型定时推进仍归原阶段运行器，尚未接入本执行页；Web普通/SSE、CLI、Goal、Relay完整模型循环收敛也未完成。总控应在 Goal 实际阶段接纳点用真实 phase agent、room、run 建索引，不能从定时配置的 target_session_id 猜阶段模型。没有 runnable phase 的检查不应伪造一次模型执行。
- 不开放 Devin 子 Agent/Goal/Relay，不把删除 require_http 当作适配。若要开放，须扩展冻结快照的会话后端契约和真实工具目录，再统一测试唯一远端会话。
- 本模块只有真实取消动作；没有“重试”按钮。未知结果不能重放，子执行原父 claim 已结束后也不能重建被冻结授权。后续显式重试需新接纳、新 execution ID 和 retry_of，原请求保留，不能简单再次发旧 payload。
- 列表当前最近200条，详情原房间轨迹最多20页；长期历史需后续加稳定游标。没有伪造百分比；状态/请求账目和轨迹是实际进度。
- 子执行 token 统计为该委派 call_id 下真实模型请求；更深的 CU 规划等其它 call_id 账目可见父轨迹，但尚未实现整个后代树累计。不要称为完整嵌套分账。
- 跨进程执行所有权、恢复后重新取得控制权、单写者/outbox、跨资源事务仍是原未闭环工作。本索引不取代 runtime_runs、tool_calls 或 scheduled_job_attempts 的权威事实。

## 联合真实测试步骤（只由总控执行）

1. 整合模块及入口后 offline build，统一候选/正常完整包流程。确认新 tab 在窄栏、低高度下可读，原定时配置仍可用。
2. 在实际允许 Agent 工具且有有效 provider 的父运行，委派到已保存的模型会话 ID；核原 credentials/参数/记忆和父房间历史、真实工具请求和返回、executionId 持久化、父子关联。唯一 SWE 当前不支持子 Agent；不得为该步骤新建 Devin 云端或冒称已通过，外部条件由总控安排。
3. 原 SWE-2-medium/revision57/唯一island-kayak 原结果房间做一次真实 poll，核同一执行页来源、run绑定、实际结果/usage、后台不切房间。既有已完成任务重启零重放证据不重复列为新成果。
4. 对存活子执行和 poll 分别请求停止；核 UI先 cancel_requested，实际 token/持久 run 中断及终态，父取消向下、子取消不误停父；迟到请求 usage 保留但不得成为新的动作来源。未知效果保留待核查。
5. 重启后已完成任务只读展示；丢弃/未知执行不自动重放；无控制句柄的记录不能显示可取消。核 list/轨迹结果归属固定工程和房间。

具体离线验证结果与提交 SHA 见共享 `execution-result.json`；本文件不是正式软件验收报告。

## 总控集成审查补充与修改所有权（初次记录，实施结论见后文）

已核对总控 `docs/analysis/2026-10-10-execution-integration-review.md`。以下三项是入口接线前的必修问题，当前冻结候选尚未解决，不能仅凭专项通过交付为可集成版本：

1. execution_index 建表必须进入原统一 schema 版本阶梯。生产 open/list/detail/cancel/状态写入不得隐式 migrate；隔离测试显式初始化。统一 schema 阶梯及大入口接线由总控持有，本模块 execution_service.rs 的 open/test 初始化改动须先明确所有权。
2. 执行页冻结 workspace/config scope，服务端校验对应请求头。列表、详情、轨迹、取消、成功及错误等所有异步返回均检查工程 generation；旧响应全部失效。不能在新工程对同名执行 ID 发起旧工程取消。
3. 监听已有 coolzhu-workspace-changed 事件；切工程与 dispose 中断或废弃全部在途请求，清空选择、详情、轨迹和取消状态。重新加载新工程列表前不得保留旧详情。

模块文件 execution_service.rs、execution_panel.js 的修正所有权目前待总控确认；本会话仅更新文档及 execution-result.json，不改冻结源码，不修改总控工作区。若总控接手，上述文件仅由总控修改；若回交模块开发者，先通过共享结果文件通知撤销旧源码摘要复验，再修正并交付完整新来源清单。app.js/main.rs/schema 阶梯大入口仍归总控。总控审查文档提出要求本身不能作为“已接手实现”的证据。

真实重试必须使用新持久接纳身份，保留原来源拒绝重放；当前拆分测试来源不改变此契约。完整共享执行、正式模型与原生软件验收仍由总控统一完成。

补充实施状态：总控通过共享 coordination-status.json 明确 execution_service.rs / execution_panel.js 修正归本会话，总控尚未复制接管；独占 Cargo 槽位已分配。本会话已修正三项要求，并作废旧来源摘要：open 不再迁移，提供 apply_session_migration_v29；列表/详情/取消/按执行身份的轨迹 API 强制携带 X-Coolzhu-Workspace-Id 和 X-Coolzhu-Configuration-Scope，复用原 configuration_request_pin 校验和工程 pin。执行页 getScope 必须由 app.js 传入 projectRequestScope；仅以一次 /api/workspace 绑定对应原工程路径的配置加载身份，不自动把旧请求刷新成新工程身份；全异步返回校验工程 generation。切工程/dispose 立即 abort 并废弃在途请求，清空选择/详情/轨迹/取消状态。前端模拟响应故意忽略 abort，迟到详情、取消错误及轨迹仍不能污染新工程。

统一迁移接线（完整精确变更已在 main-wiring.patch）：在 apply_session_schema_steps 的原 v28 后调用 execution_service::apply_session_migration_v29；SESSION_SCHEMA_VERSION 从28升29；initialize_session_schema 的结构验证增加 execution_index 的 id/workspace_id/spec_json/state/run_id。生产模块不绕过 schema_upgrade::run，版本发布归总控。独立模块测试显式初始化原schema，不让查询建表。总控若在集成中先分配其它版本，须同时调整迁移名和版本，不可撞号。

修正后离线构建/专项以最新 execution-result.json 为准。前端模拟验证属于迟到响应回归，不等于浏览器、窄栏、真实模型或正式软件验证。

身份查询失败缓存修正：仅清除当前失败Promise；旧generation迟到失败不清新工程Promise，允许刷新/下次只读列表轮询重读身份。取消等POST不自动重放。最终修正的offline build和21项Rust专项全部通过，前端异步模拟通过；正式集成及联合真实验收仍待总控。
