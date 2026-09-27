# 0.2 黄金 fixture 与历史失败覆盖核账

2026-09-27。本记录按 [04 计划的 0.2 退出条件](../../analysis/2026-09-21-integration-review/04-coolzhuagent-整合改进执行计划.md)核账：从现有 Web 行为建立脱敏黄金 fixture、假模型服务及事件回放，涵盖流式/非流式、工具配对、图片路由、空总结、跨轮过滤和文件长任务；**未录制的副作用默认拒绝**。下表区分清单结构校验、生产入口的相邻行为测试、以及用原历史数据重新驱动产品。三者不能互称。

最近可定位的已运行记录：根 `s0_fixture_contract` 两项于 2026-09-27 的 `tmp/2026-09-26-completion/workspace-final-tests.log` 通过；Web 主二进制 1256 项通过、2 ignored（含下述测试），日志 `tmp/vision-usage-reasoning-web-tests.log`，属于 0.2.18 打包前最后一次相关 Web 全套。该轮 Web 源码构建 SHA256 为 `4AF5A03ACC09CA4F7FBCF6C14AF5E01D60D8C5105A7B7FFF8AC9FD8BC8F9F965`，见[组合验证报告](final-combined-source-verification.md)；测试可执行文件本身未单独记录哈希。底层 `computer-use-core` 的相邻回归见前一份 workspace 日志。**这些日志不证明 0.2.19 安装二进制完整重跑了 S0 黄金场景**；另有 [0.2.19 安装版副本事件对照](runtime-event-coverage-audit.md)只验证一个 Agent 请求/轨迹/用量夹具，并非本表六场景的动态回放。

## S0 清单：六个合成场景

源清单为 [`tests/fixtures/s0-golden/manifest.json`](../../../tests/fixtures/s0-golden/manifest.json)，标明 `synthetic-only`、`recorded-only`。[`tests/s0_fixture_contract.rs`](../../../tests/s0_fixture_contract.rs)两项测试检查六个 ID、JSON、无典型秘密字段及工具 call/result 的 `call_id` 配对；它们**只解释/检查清单**，没有执行 Web 产品路径。测试专用 [`s0_fixture_replay.rs`](../../../modules/gui-web/packages/web-console/src/s0_fixture_replay.rs)提供随机本机端口的最小 OpenAI 兼容假模型；其自身测试验证流式、非流式工具响应。`main.rs` 的 `s0_fixture_server_drives_real_web_model_entrypoints_without_tool_side_effects` 接通真实模型请求构造器，但流式一侧只建立连接，两侧请求的 `tools` 都是 `null`，并未派发清单工具。

| fixture ID | 清单意图 | 已跑代码/测试和实际断言 | 未得到的逐场景证据 |
| --- | --- | --- | --- |
| `BASE-STREAM-TOOL-PAIR` | 流式 `read_file` call/result 配对 | 根 `s0_golden_fixture_replays_only_recorded_tool_results` 检查静态配对；`fixture_server_replays_stream_and_nonstream_tool_pair_contracts` 验 SSE 带指定 call ID 与 `[DONE]`；Web `s0_fixture_server_drives_real_web_model_entrypoints_without_tool_side_effects` 验一次流式请求已建立。三项在上述日志通过。 | Web 入口未消费完整工具结果/收尾事件，也未证明产品派发器拒绝额外副作用。 |
| `BASE-NONSTREAM-TOOL-PAIR` | 非流式 `list_directory` 配对 | 根静态配对和假模型服务测试检查 JSON 工具名/ID，均通过；`main.rs` 非流实入口用的是下行图片路由场景。 | 此 ID 尚无从 Web 接纳到工具反馈的逐场景回放。 |
| `BASE-IMAGE-ROUTING` | 区分原图与视觉转述 | 清单 ID/无秘密守卫通过；Web 实入口测试用该 ID 收到**文本**答复并确认非流式、无工具。另有 [0.2.18 安装版隔离视觉用量](installed-0.2.18-isolated-e2e.md)用真实 PNG 验证视觉模型与目标文本模型的两请求边界，但不是该 S0 清单回放。 | S0 场景本身没有附件或路由来源断言，不能以其通过证明原图/转述二选一完整矩阵。 |
| `BASE-EMPTY-SUMMARY` | 空总结不可伪装成完成正文 | 只在根静态清单测试中出现；Web 的 `tool_result_fallback_stream_and_nonstream_report_failed_tool_when_final_answer_empty` 与仅推理兜底测试覆盖**相邻**空最终答复处理，并在 Web 全套通过。 | 未用这个 ID 通过假模型重放完整 Web 终态；空总结、工具失败、仅推理是不同输入，不应合并算通过。 |
| `BASE-CROSS-TURN-FILTER` | 跨轮临时内容不进入新请求 | 只在根静态清单测试中出现；[`chat_tool_history.rs`](../../../modules/gui-web/packages/web-console/src/chat_tool_history.rs)的 `chat_tool_history_filters_audit_before_budget_and_compaction_without_mutating_store` 与 `chat_tool_stream_two_calls_and_next_user_turn_keep_protocol_and_trim_internal_history` 用独立合成历史验证过滤与原生工具协议，两项在 Web 全套通过。 | 没有将本 ID 的录制事件逐条送入 Web 请求并对比输出快照。 |
| `BASE-LONG-FILE-TASK` | 文件长任务只回放录制结果 | 根静态测试确认 `write_file` intent/result 同 call ID；`chat_tool_stream_two_calls_and_next_user_turn_keep_protocol_and_trim_internal_history` 在临时目录**真实**写入并读取约 10 KB 合成文件、核对下一轮无原始代码泄露，在 Web 全套通过。 | 真写临时文件的测试不是此 ID 的 recorded-only 回放；没有证明生产工具派发在未录制动作上默认拒绝。 |

因此，六个 ID 的**清单完整性已测**，假模型服务与部分真实入口**已测**，但“六个场景都按录制事件重放，且产品对清单外副作用默认拒绝”仍未满足。`chat_tool_history.rs` 内嵌的合成消息/工具结果测试是有价值的回归，不是另一个从用户 Web 行为采集的黄金清单。

## Paint 历史失败：事实归档与相邻回归

[`CU03 语料库`](../cu03-eval/corpus-2026-09-19-paint-window-drag/README.md)的 `manifest.json` 共有 8 条：`paint-r1..r6` 六条 `history_derived`、一条 `new_capture` 简化夹具、一条 `synthetic`。六次历史运行均**未达成目标**。一级导出与 8 张实际引用的窗口截图在仓库中，原始 SQLite/截图仍在忽略的 `tmp/`，二级脱敏摘要见 `docs/testing/release-0.2.14/paint-r3..r6.json`。[`computer_use_eval_corpus.rs`](../../../modules/gui-web/packages/web-console/src/computer_use_eval_corpus.rs)七项守卫验证类别、缺失声明、失败终态、截图及迁移文件摘要、不可评分口径和 R3 指纹反例；七项在上述 Web 全套通过。守卫核的是**归档真实性和口径**，不是让旧动作再次在桌面执行。

| 历史样本/真实终态 | 对应代码与已通过的相邻回归 | 历史数据动态回放状态 |
| --- | --- | --- |
| `paint-r1`：`observation / input_failed`，窗口身份/位置/DPI 改变，0 步 | [`computer_use_adapters.rs`](../../../modules/gui-web/packages/web-console/src/computer_use_adapters.rs) `desktop_window_or_dpi_change_is_stale_before_input` 验合成身份/DPI 变化时零动作；底层 `input::stroke::tests::t01_stale_identity_failure_is_deterministic_stale_not_sent_not_needed` 在 2026-09-27 workspace 日志通过。 | 未以 R1 原截图和缺失的窗口几何重新驱动观察阶段；相邻测试预期 `stale_observation`，不能冒称复现 R1 的 `input_failed`。 |
| `paint-r2`：`intent_guard / invalid_tool_input`，缺成功条件，0 步 | [`computer_use_executor.rs`](../../../modules/gui-web/packages/web-console/src/computer_use_executor.rs) `invalid_input_can_be_corrected_without_consuming_the_single_execution_budget` 验缺字段零动作、纠错预算单独计，Web 全套通过。 | 没有用 R2 原始模型请求重放；聊天层另写 `recursive_call_blocked`，语料库采用运行时权威错误码，不合并两个描述。 |
| `paint-r3`：`supervisor / no_progress`，两次 click | [`computer-use-core/supervisor.rs`](../../../modules/computer-use/packages/computer-use-core/src/supervisor.rs) `repeated_action_without_progress_is_blocked` 与 `controller::tests::repeated_no_progress_stops_before_a_third_input` 于 workspace 日志通过；[`input_permit_store.rs`](../../../modules/gui-web/packages/web-console/src/input_permit_store.rs) `b121_t5_paint_r3_repeated_identical_clicks_are_not_duplicates` 和语料库指纹反例守卫于 Web 全套通过。 | 真实两次 click 的记录/截图可校验，但缺完整决策前输入；测试分别验证一般停止规则、不同观察代次下不误判同一请求，不是 R3 原始运行重播。 |
| `paint-r4`：`supervisor / no_progress`，两次 drag | 与 R3 共享生产 `RunBudgetGuard` 的一般无进展停止规则及其上述测试；语料库守卫保留 R4 的两次拖拽与 3 张截图。 | 现有控制器回归用的是合成 click，没有以 R4 的 drag、画布图及当时反馈重放；不能把一般规则称为 R4 通过。 |
| `paint-r5`：`planning / planner_backend_unavailable`，规划器约 20 秒超时 | [`computer_use_planner.rs`](../../../modules/gui-web/packages/web-console/src/computer_use_planner.rs) `zero_budget_returns_insufficient_budget_with_zero_model_requests` 和阶段预算测试在 Web 全套通过；生产错误映射含 `planner_backend_unavailable`。 | 零预算拒绝**不是** R5 的已发出请求后超时；未对原服务故障或旧请求体做同形回放。 |
| `paint-r6`：`execution / stale_observation`，click/click/drag | `desktop_canvas_requires_fresh_image_and_consumes_generation_once` 验图像变化/代次消费时零额外动作；`failure_receipt_facts_are_persisted_instead_of_the_code_heuristic` 验 stale 错误随真实回执记账，均在 Web 全套通过。 | 计划要求的 R6 等价记账 fixture（3 attempts、1 次已确认输入、2 次 stale 拒绝）未联动重放到步骤、run 聚合、API、UI、导出五处；不能由静态旧数据补造回执。 |

`cu03-eval-2026-09-26-simplified-fixture` 是**新采集的合成占位观察**，其 `simplified_fixture_run_is_rescored_without_inventing_missing_context` 在 Web 全套通过，但三项语义指标均无可评分样本；它不是 R4–R6。`synthetic-replay-fixtures` 同样不可算历史执行。六条历史样本共同缺少发给规划器的提示词本体、UIA 元素树、窗口 rect/DPI/裁剪映射、部分原始模型响应与语义行为标签；故这些相邻回归只能防止已知类问题再次被明显误报，不能证明当年的规划输入或桌面副作用可完整再现。

下一步 0.2 真正剩余的最小验收是：从可公开、脱敏的**实际 Web 请求/事件**形成可逐项验证的样本与期望快照，让六类场景中尚只做静态声明的分支真正经过入口；对写文件等副作用在回放器与产品派发边界验证“仅录制动作可执行”；历史 R1–R6 则先按缺失事实判定可回放层级，再对足够的结构事实重放并注明不可重建部分。现有证据足以说明这些缺口，故本次没有改产品、没有抢占 Cargo、没有操作 8765 或原生桌面，也没有以测试数量代替覆盖率。
