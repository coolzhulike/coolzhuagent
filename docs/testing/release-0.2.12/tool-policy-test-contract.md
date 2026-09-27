# P1 工具暴露与失败恢复：发布测试报告片段

> 本文件为源码契约和扩展用例附件。实际已安装 0.2.12 Release；安装后结果以[发布主报告](../../2026-09-19-release-0.2.12-change-and-test-report.md)为准。下文“本次整理未测试”描述附件审阅者的工作范围，不表示主发布任务未测试。建议用例不能全部标为已通过。

核对日期：2026-09-19。范围仅为本轮 P1 源码、问题文档、已有测试与工作记录；本次整理没有修改产品代码、运行测试或打包安装。安装版本、二进制校验及安装后冒烟结果由主报告填写。

## 1. 变更前后与故障定位

| 场景 | 变更前 | 当前实现与预期 |
| --- | --- | --- |
| 开启调试完全访问，同时关闭模型工具 | `dev_open_permissions` 提前返回 true，覆盖工具开关，并把暴露模式强制改成 all | 完全访问只影响权限；工具是否启用和暴露模式独立求值，关闭后请求不带工具 schema，结构化越界调用也在执行前拒绝 |
| 本地小上下文模型生成 HTML/SVG | 上下文不超过 16,384 时只保留 Computer Use，文件任务可能只剩 UI 工具可选 | 小窗口保留常用文件、搜索、命令和语义工具；只有明确 UI 目标及受限的上下文延续可保留 Computer Use |
| dispatch-only 或自定义名单 | Computer Use 在暴露模式分支前无条件加入 | dispatch-only 不加入 Computer Use；自定义名单最终约束包括语义入口、Computer Use 和 handoff 在内的工具集合 |
| 本地与云端会话需要不同工具策略 | 主要受全局配置控制 | 按 session_id 保存工具开关、范围、Computer Use 开关和工具名单，房间能力可以进一步关闭工具或 Computer Use |
| UI 工具失败后模型换入口重试 | UI 终态文案可能直接充当最终回答，原 HTML/文件任务丢失；通用语义入口一起熔断 | 保留 UI 终态限制；拒绝后续 UI 重试，最多一次无工具恢复原任务。语义文件写入独立于 UI 熔断，仍经文件权限链 |
| 模型以正文输出 `<tool_call>` | 作为普通最终答案保存，不执行但静默失败 | 对被识别的正文伪调用触发一次无工具恢复；不把标记转成执行授权；再次失败时明确原任务未完成 |
| 模型已经调用工具后外层补执行 | 存在重复副作用或旧本地语义旁路风险 | 工具循环内部执行并反馈，外层不重复派发已执行请求；旧本地规则兜底也通过会话/房间策略校验 |
| 调试完全访问显示 | 页面可能仍只显示保存的房间权限，造成“未授权”误解 | 权限 API 同时返回保存值和生效值，页面可显示“调试完全访问”，且不改写房间持久化授权 |

根因和历史复现见 `docs/issues/2026-09-19-local-model-tool-call-derailment.md`。该文档顶部为修复补充，第 5 节仍保存变更前代码与分析，不能把其中“dev_open 强制 all”等历史描述当作当前行为。

## 2. 架构与代码定位

主要文件：`modules/gui-web/packages/web-console/src/main.rs`。下面行号为本次只读快照的近似位置，后续请优先按函数名定位。

| 位置/符号 | 职责与测试观察点 |
| --- | --- |
| `SessionModelLimitOverride`，约 5123；`session_model_settings_for` | `session_model_limits[session_id]` 中读取会话参数；None 表示继承；工具字段见下节 |
| `default_dev_open_permissions`，约 6299；`ConfigTool::default` | 缺省值使用 `cfg!(debug_assertions)`；debug 缺省 true，release 缺省 false，显式 true/false 保留 |
| `llm_tools_enabled` / `llm_tools_enabled_for_session`，约 25999 | 全局开关与会话覆盖，不读 dev_open 来重新启用工具 |
| `llm_tool_permission_for_room`，约 32274 | dev_open →完全访问；否则按房间保存权限求值，无房间或读取失败按只读暴露 |
| `llm_tool_definitions_for_session` / `llm_tool_definitions_with_settings`，约 32456 | 房间能力、会话开关、暴露范围、名单和 Computer Use 的组合 |
| `agent_message_request_build_with_system`，约 32044 | 从策略集合再按请求意图/上下文缩减工具，注入实际工具政策；只有实际存在 Computer Use 才能强制选择它 |
| `select_tools_for_request`，约 32134；`messages_allow_computer_use`，约 31468 | 小窗口筛选和 UI 意图判断；保留“继续”及工具反馈的最近目标上下文 |
| `request_tool_policy_instruction` / `tool_call_name_is_exposed`，约 31489 | 请求末尾的具体能力声明；对模型调用核对本次实际 schema，而非只核对全局注册表 |
| `call_agent_model_with_tool_loop`，约 26397 | 非流式反馈环、已执行结果历史、一次无工具恢复、反馈轮数边界 |
| `api_chat_send_stream` 内首轮与反馈轮，约 17368 / 17633 | 首轮 SSE 汇总后及每次反馈响应检查伪调用、越界调用和 UI 终态；恢复后清空待执行调用 |
| `run_model_tool_dispatch_for_session_with_identity`，约 33350 | 执行入口再次校验会话/房间当前能力；UI 语义派发必须实际开放 Computer Use |
| `is_computer_use_tool_family` / `is_computer_use_tool_request`，约 33467 | 区分正式/旧 UI 名称，以及语义文件操作和语义 UI 操作；后者才受 UI 熔断 |
| `model_text_contains_pseudo_tool_call` / `model_text_requires_tool_recovery`，约 33488 | 识别正文开头的 `<tool_call`、`<function_call`，含小写 xml/json 围栏；明确要求原样协议示例时允许显示文本 |
| `call_agent_model_text_recovery` / `unfinished_tool_recovery_answer`，约 33502 | 原历史+原任务+恢复说明；强制 `tools=None`、`tool_choice=None`；空回答/再次伪调用/再次结构化调用转换为未完成说明 |
| `run_tool_intent_message`，约 33149 | 旧本地意图兜底也带 session/room 进入同一校验入口 |
| `call_agent_model`，约 26113 | 单次视觉理解无执行反馈器，固定不带工具；若仍收到调用则恢复一次。不是普通聊天绕过工具循环的路径 |
| `chat_room_permission_status`，约 14785 | 返回保存权限与 effective 权限，debug 状态不覆盖 SQLite 房间记录 |

关联文件：

- `src/tool_loop_coordinator.rs`：调用身份、错误终态映射及相关测试，验证 provider_tool_call_id/session_id/turn_id 不丢失。
- `modules/computer-use/packages/computer-use-core/src/supervisor.rs`：既有预算和终态规则保留；本轮在会话层恢复任务，没有通过关闭 supervisor 解决误调用。
- Provider 适配层继续把原生结构化 tool_calls 转换为工具块；正文标签仍是文本，本轮没有增加“解析正文后直接执行”的路径。

## 3. 配置优先级与接口契约

### 3.1 独立的四层限制

1. **模型工具开关**：会话 `enable_llm_tools=Some(true/false)` 优先于全局 `[model].enable_llm_tools`；None 继承。注意会话 true 可以覆盖全局 false，因此“全局 false”不是所有会话的不可覆盖上限。
2. **房间能力**：存在房间记录且工具能力关闭时，`llm_tool_definitions_for_session` 直接返回 None；房间 Computer Use 关闭会将会话 Computer Use 视为 false。此限制不能通过会话 true 或 dev_open 重开。
3. **可用工具集合**：先按模式和权限生成，再交集自定义名单，最后按请求意图、小窗口规则缩减。模型返回调用必须在该请求的最终集合内。执行时再次按当前会话/房间策略核验，以防请求发出后设置变化。
4. **实际执行权限**：schema 暴露不代表执行成功；registry/语义文件写入还经过 runtime 权限与审计。dev_open 提供完全访问，但不绕过前面工具能力开关。

Computer Use 的额外上限：全局 `[computer_use].enabled` 必须 true，再与会话 `computer_use_enabled`（None 等价“不额外关闭”）、房间能力、模式、名单及 UI 意图共同取交集。会话 true 不能覆盖全局 Computer Use=false。

### 3.2 模式、名单与调试默认

- `all`：暴露注册工具及特别入口，仍不暴露冗余的 `ToolSearch`，仍受会话名单和请求意图限制；实际执行权限继续检查。
- `whitelist`：缺省模式，按当前授予权限筛选注册工具；只读时不暴露需要写入的 registry 工具。语义入口和 handoff 有独立定义，再受自定义名单过滤。
- `dispatch-only`：只可能暴露 `tools_semantic_dispatch`，不携带 Computer Use。自定义名单如果没有这个名字，结果为空。
- `tool_allowlist=None`：不增加自定义名单限制；`[]`：不暴露任何工具；非空列表：与模式产生的工具集合做精确名字交集。未知工具名不会自动注册为工具。
- dev_open 不修改 mode 字符串，但会把有效权限提高为完全访问，因此 **默认 whitelist 的权限推导集合可能随之扩大**。不要据工具数量变多就误判“又强制变成 all”。测试应同时观察 mode、实际工具名与权限。
- debug/release 默认只用于缺省配置；发布包即使是 release，保留了显式 `dev_open_permissions=true` 的既有用户配置仍会完全访问。安装后应分别验证“保留旧配置”和“全新缺省配置”。

语义入口的重要边界：当前自定义名单约束的是模型可调用入口，不是全部内部派生操作。开放 `tools_semantic_dispatch` 后，包含可识别 path/content 的请求仍能映射到 `write_file`，由文件权限链决定能否执行；其余 legacy 语义路由视为 UI，需要 Computer Use 同时开放。不能把“只允许 semantic_dispatch”断言成“完全没有文件写入能力”。

### 3.3 对外接口与字段

- `GET /api/sessions/{session_id}/model-settings`：读取 `parameters` 与协议/地址/有效容量等信息。
- `POST /api/sessions/{session_id}/model-settings`：请求形如 `{"parameters": {...}, "session": {...可选...}}`。`parameters` 中的 P1 字段为 `enable_llm_tools`、`llm_tool_exposure`、`computer_use_enabled`、`tool_allowlist`。该 API 替换当前会话参数对象，测试写入时应先 GET，再保留无关参数后修改目标字段，避免误清采样/容量等设置。
- API 允许工具范围仅为 `all` / `whitelist` / `dispatch-only` 或 null；非法范围拒绝。名单最多 256 项，每个名称非空且不超过 128 字节。内部规范化仍兼容旧配置 `dispatch` / `dispatch_only`，未知全局值保守回落 whitelist。
- `POST /api/chat/send`、`POST /api/chat/send/stream`、`POST /api/chat/send/relay`：分别覆盖非流式、流式与接力入口。基础测试载荷仅需 session_id、chat_room_id、target_agent_ids、text，使用隔离的测试会话/房间。
- `GET /api/chat/rooms/{room_id}/permissions`：原字段 `permission_profile`、`full_access` 为保存值；新增 `dev_open_permissions`、`effective_permission_profile`、`effective_full_access` 表达实际权限。不能用保存值是否 full-access 推断调试时实际权限。

不需要把 API Key、鉴权头、完整用户配置或真实聊天内容写入测试报告。请求证据只保留工具名、脱敏参数、会话/房间测试标识、响应类型及副作用计数。

## 4. 建议正反与边界测试矩阵

以下是建议继续补充/复测的用例，不应全部标成“已有实机通过”。需要副作用时只使用隔离临时目录和受控浏览器页面。关键失败用例至少在非流式和流式各执行一次；接力检查每个目标会话独立策略。

| 编号 | 设置/输入 | 核心断言 |
| --- | --- | --- |
| T01 | dev_open=true；会话工具=false；要求保存 HTML；假模型返回 write_file 结构化调用 | 首请求无 tools/choice；工具未执行；只一次无工具恢复；最终不声称已保存 |
| T02 | dev_open=true；全局工具=false；会话字段为 null，再改 true | null 继承关闭；true 可按会话开启；另一个未覆盖会话仍关闭，配置无串扰 |
| T03 | 会话工具=true；房间工具能力=false | 房间上限生效，无 schema；直接执行入口也拒绝 |
| T04 | 全局 CU=false/房间 CU=false/会话 CU=false，分别组合其余值 true | Computer Use 始终不在集合；semantic UI 和旧本地意图兜底不能绕过 |
| T05 | dispatch-only；名单 null、[]、[write_file]、[tools_semantic_dispatch] | 分别为仅语义、空、空、仅语义；任何组合不附加 CU |
| T06 | whitelist；只读/工作区写入/完全访问三种有效权限 | 注册工具集合与权限相符；显式名单进一步缩小；dev_open 改的是权限而非 mode |
| T07 | all；名单只含 write_file、CU；会话 CU=false | 只剩 write_file；未知名单项不生效；ToolSearch 始终不自动暴露 |
| T08 | 上下文 0、8192、16384、16385；要求真实文件写入 | 16K 边界内仍有合法文件工具；不能退化为只剩 UI；大于边界不应使用小窗口名单过滤 |
| T09 | “只输出 HTML/SVG”与“实际保存 tmp/demo.html”对照 | 两者不应暴露/触发 UI；前者直接文本产出，后者有权限时可调用文件工具并核对真实文件内容 |
| T10 | 明确浏览器任务→继续；浏览器任务→新 HTML 任务；旧工具结果→Do not use tools | 简短继续保留最近明确 UI 目标；新纯生成目标移除 UI；明确禁工具不因历史 ToolResult 重新开放 |
| T11 | 首轮伪 `<tool_call>`，下一轮有效 HTML | 总模型请求数=2；两个请求不带工具；第二轮包含完整原任务；没有文件副作用；最终正文为 HTML |
| T12 | 首轮伪调用，恢复仍伪调用/空回答/结构化调用/HTTP 错误 | 总恢复次数最多1；最终明确未完成并关联原任务；无第三次自动重试、无被拒绝调用副作用 |
| T13 | 原样协议示例、教程引用、普通 HTML；对照非示例任务中的标记 | 示例保持文本且不执行；正常 HTML 不误拒绝；非示例伪协议触发恢复 |
| T14 | 前缀解释文字后才出现标签、普通 ``` 围栏、```XML 大写围栏、多标签、未闭合标签 | 作为检测边界探针记录现行为；当前不是通用 XML/Markdown 解析器，不应预设所有形态都会被识别；任何文本形态都不应获得执行授权 |
| T15 | 第一次 CU 返回 failed/blocked/timed_out/cancelled 后，再请求正式CU/旧别名/semantic UI | 禁止第二次 UI 派发；恢复原任务，不把 runtime 诊断冒充产物；provider调用身份仍可追踪 |
| T16 | CU 失败后，请求可识别 path/content 的 semantic 文件写入 | 不被 UI 熔断误伤；遵守文件权限；若语义文件识别失败按 UI 限制处理；写入结果真实可核对 |
| T17 | 同轮已成功写文件，随后工具未开放/反馈轮数到上限 | 已执行结果进入恢复上下文；超限请求未执行；外层不重复写入；总副作用计数精确 |
| T18 | 同一批次含合法调用和未开放调用 | 请求级发现未开放调用后进入恢复，不能先偷偷执行该批的合法子集；用计数器验证是否发生副作用 |
| T19 | 请求发出后、执行前关闭会话/房间能力 | 执行入口再次校验，拒绝过期能力；不只依赖发送请求时的 schema |
| T20 | 反馈期间/恢复期间取消；慢响应/断连 | 用户取消后不恢复副作用、不追加新的工具调用；已收到的真实 usage 只记一次；未知消耗不估算 |
| T21 | 两个会话接力：一个工具关闭，一个只允许 read_file | 每个请求按目标会话构造，身份/房间/工具集合不串；共享历史不等于共享授权 |
| T22 | 安装版 release，全新配置与显式保留 dev_open=true 对照 | 缺省为 false；显式值保留；权限接口与 UI 显示 effective 值；切回普通权限不改写原房间保存值 |

应同时观察四类证据：发送给假模型的请求（tools/tool_choice/原任务上下文）、最终用户消息（HTML或明确未完成）、真实副作用（文件/调用计数）、执行审计（调用身份及路由）。单看 HTTP 200 或最终文案不足以证明工具未执行/任务完成。

## 5. 已有测试名称与覆盖性质

主要行为测试位于 `main.rs` 的 `tests`：

- `tool_policy_session_overrides_do_not_inherit_dev_open_exposure`：调试完全访问不覆盖会话范围、名单和总开关。
- `tool_policy_small_context_preserves_file_tools_and_ui_intent`：8K 保留文件工具；纯 HTML、明确浏览器、继续、新任务及明确禁工具的意图区分。
- `tool_policy_pseudo_calls_are_detected_without_executing_or_rejecting_quotes`：开头标记、xml 围栏、正常引用/HTML、显式协议示例、semantic UI/file 区分。
- `tool_policy_disabled_model_recovers_once_without_executing_pseudo_calls`：真实本地 HTTP 假模型，非流式两条分支（恢复成功/持续失败），请求恰2次、无工具 schema、原任务保留、临时文件不存在。
- `tool_policy_semantic_and_local_fallback_cannot_bypass_disabled_computer_use`：执行入口与旧意图兜底拒绝绕行，tools-off+dev-open 仍拒绝文件写入。
- `llm_tool_definitions_returns_none_when_disabled`。
- `llm_tool_definitions_default_whitelist_exposes_readonly_set`。
- `llm_tool_definitions_whitelist_follows_room_permission_scope`。
- `llm_tool_definitions_dev_open_defaults_to_all_registry`：名称保留历史措辞，实际覆盖“默认 whitelist 在完全访问权限下涵盖 registry”，不能解读为 dev_open 强制 mode=all。
- `llm_tool_definitions_all_mode_exposes_full_registry_plus_dispatch`。
- `llm_tool_definitions_dispatch_only_mode_excludes_computer_use`。
- `computer_use_retry_guard_covers_formal_and_legacy_tool_names`。
- `chat_room_permission_status_reports_effective_dev_access_without_overwriting_saved_profile`：调试前/中/后，effective 权限和 SQLite 保存值/时间戳不串改。

补充契约与关联回归：

- `agent_message_request_with_context_uses_assembled_messages_and_system`。
- `computer_use_prompt_requires_surface_grounding_and_post_action_verification`。
- `chat_dispatch_routes_semantic_hotkey_to_tool_agent`。
- `non_stream_model_tool_loop_allows_multiple_feedback_rounds`、`stream_model_tool_loop_reuses_full_context_assembly`：这两项主要检查源码结构契约，不等价于真实多轮故障注入测试。
- `semantic_tool_sidecar_does_not_overwrite_pending_model_tool_work`、`non_stream_model_tool_message_preserves_provider_tool_id`：主要为静态接线契约。
- `tool_loop_coordinator::tests::compact_context_keeps_file_tools_alongside_computer_use`、`no_tool_intent_still_hides_compact_tool`、`provider_tool_id_is_preserved_for_computer_use`、`provider_identity_reaches_computer_use_tool_result`、`every_non_success_terminal_status_is_an_error_for_the_model`。

建议的定向运行命令（本报告整理阶段未执行）：

```powershell
cargo test -p coolzhu-web-console --offline tool_policy_ -- --test-threads=1
cargo test -p coolzhu-web-console --offline llm_tool_definitions_ -- --test-threads=1
cargo test -p coolzhu-web-console --offline computer_use_retry_guard_covers_formal_and_legacy_tool_names -- --test-threads=1
cargo test -p coolzhu-web-console --offline tool_loop_coordinator::tests -- --test-threads=1
cargo test -p coolzhu-web-console --offline chat_room_permission_status_reports_effective_dev_access_without_overwriting_saved_profile -- --test-threads=1
```

## 6. 已有验证证据与尚未覆盖的范围

已有工作记录报告：

- P1 最终定向 `tool_policy_` 为 5/5，通过日志 `tmp/analysis-tool-runtime-final-tests.txt`；工具定义6项、重试守卫1项及相关原契约亦通过。
- `docs/work-logs/2026-09-19-architecture-tool-model-chat-delivery.md` 与后续布局交付记录报告 web-console 8项库+1项helper+944项主程序共953项通过，0失败；这属于上一轮代码验证，不替代安装后验证。
- 独立 18765 控制台+18766 假模型的浏览器验收覆盖流式思考、结构化 read_file、工具结果仅状态栏展示、usage计数与消息索引；三次请求合计300输入/60输出与受控模型相符。该 UI 用例证明正常结构化调用链和展示，不代表所有P1故障分支都已进行流式端到端故障注入。
- `coolzhu-tool-registry` 独立离线构建通过；`module_linkage_smoke` 4/4通过。
- 上一轮对 bonsai 的 `127.0.0.1:8080/v1/models` 仅进行3秒只读探测，连接被拒绝。没有启动/重启模型，没有完成真实27B模型复现回归。本次整理没有再次探测，因此不推断其当前运行状态。

真实本地模型复测必须单列：固定原始 pelican HTML/SVG 提示、同一模型文件/服务参数、上下文、max_output、思考预算；比较工具开/关及有无“仅输出HTML”约束。需保存最终HTML并检查实际SVG产物，核对是否误走UI、是否落盘、是否截断和是否发生恢复。原问题变体C（规划耗尽输出预算）仍可能独立存在，本轮工具策略修复不能保证模型绘图质量、提示词服从性、chat template工具协议兼容性或推理速度。

尤其需要补足的覆盖：真实 bonsai 请求；流式伪协议/越界结构化调用故障注入；UI失败后原任务恢复和semantic文件继续；反馈上限后副作用不重复；取消/断连竞态；release安装后显式配置保留与debug默认差异。若测试暴露边界缺陷，应记录失败并归因，不将建议用例直接标记为通过。

参考工作记录：`docs/work-logs/2026-09-19-tool-policy-and-recovery-verification.md`、`docs/work-logs/2026-09-19-architecture-tool-model-chat-delivery.md`、`docs/work-logs/2026-09-19-jade-bamboo-layout-delivery.md`。本片段不包含密钥、真实用户会话正文或用户配置内容。
