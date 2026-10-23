# HA-03 明确工具能力配置：实施与测试记录

状态：动作关键词硬门已移除；显式关闭、allowlist、权限、schema 预算仍保留。真实模型行为留给 HA-11。

## 代码改动

- `select_tools_for_request` 删除 `has_tool_intent` 参数；只要 `tools_enabled` 且用户未明确说“不要使用任何工具”，就按显式配置/allowlist/权限暴露基础工具与 `tools_semantic_dispatch` 发现入口。
- 新增 `messages_explicitly_disable_all_tools`；`messages_have_tool_intent` / `text_has_tool_action_intent` 降为测试用分类器，不再作为生产暴露硬门。
- 小窗口策略仍保留：常用文件/搜索/命令工具 + 有界 MCP schema 预算。
- `llm_tool_exposure` 继续作为显式策略：`whitelist`（默认/未知值）、`all`、`dispatch-only`；大小写归一。
- 未新增模型名推断；未知模型名仍走 `whitelist`，不自动提权。

## 验收对应

### HA03-01 基础工具/发现入口

`pure_qa_gets_base_tools_without_keyword_gate_and_explicit_no_tools_stays_closed`：

- 纯问答 `介绍一下 Agnes Image 2.1 Flash 的配置` 的 `messages_have_tool_intent` 为 false；
- 但 `select_tools_for_request` 仍返回 `tools_semantic_dispatch` + `write_file`；
- 说明动作关键词不再是暴露硬门。

### HA03-02 显式禁用/allowlist

同一测试：

- 用户明确 `不要使用任何工具` → `messages_explicitly_disable_all_tools=true`，`select_tools_for_request` 返回 `None`；
- `tools_enabled=false` 也返回 `None`；
- 既有 `tool_policy_semantic_and_local_fallback_cannot_bypass_disabled_computer_use`（HA-01 日志）证明禁用 CU/工具仍被宿主执行门拒绝。

### HA03-03 工具缺失/不支持

- 同一测试：`select_tools_for_request(None, ...)` 返回 `None`；`tool_call_name_is_exposed("not_a_real_tool", ...)` 为 false。
- `tool_policy_disabled_model_recovers_once_without_executing_pseudo_calls`：模型 tools 关闭时捕获 2 次请求，均无 tools；伪调用不创建文件，只允许一次无工具恢复。
- 既有 `tool_policy_pseudo_calls_are_detected_without_executing_or_rejecting_quotes` 覆盖伪调用不被当成执行。
- 限制：未构造真实 provider 不支持分支；该分支在 HA-11 真实模型/Provider 对比中检查。

### HA03-04 小窗口/默认/未知模型预算

- `tool_policy_small_context_preserves_file_tools_and_ui_intent`：小窗口仍保留 file/UI 工具，显式关闭时无工具。
- `tool_policy_small_context_mcp_is_bounded_and_keeps_used_name`：MCP schema 受 `small_context_mcp_prompt_budget` 限制；预算 0 返回空。
- 同一测试：`normalize_llm_tool_exposure("some-unknown-model") == "whitelist"`，未知名字不提权。

### HA03-05 发现结果不得扩大授权

- `unadmitted_discovery_tools_are_removed_before_winning_exposure`：`mcp__evil__shell` 在无父绑定时被 `retain_admitted_plugin_definitions` 删除，`read_file` 保留；随后 `tool_call_name_is_exposed` 对 `mcp__evil__shell` 返回 false。
- `tool_call_name_is_exposed` 对未知工具返回 false；`select_tools_for_request` 只在已有 `tools` 定义里裁剪，不凭发现结果新增工具。
- 源码路径：`llm_tool_definitions_for_session` 的 MCP 定义先走 `mcp_host::model_tool_definitions`/`discovered_model_tool_permission`，最终仍由 allowlist/权限/`retain_admitted_plugin_definitions` 过滤；发现只影响可见集合。
- 限制：未连接真实恶意发现服务；这是纯函数/集合断言，真实模型行为留给 HA-11。

## 测试记录

| 测试 | 结果 | 日志 |
|---|---|---|
| `pure_qa_gets_base_tools_without_keyword_gate_and_explicit_no_tools_stays_closed` | 1 passed | `tmp/ha03-test-base-tools-2.log` |
| `tool_policy_small_context`（2 tests） | 2 passed | `tmp/ha03-test-small-context.log` |
| `no_keyword_intent_keeps_tools_but_explicit_disable_hides_them` | 1 passed | `tmp/ha03-test-tool-loop.log` |
| `unadmitted_discovery_tools_are_removed_before_winning_exposure` | 1 passed | `tmp/ha03-test-discovery.log` |
| `tool_policy_disabled_model_recovers_once_without_executing_pseudo_calls` | 1 passed | `tmp/ha03-test-disabled-model.log` |

合计 6 个定向测试通过（base 1 + small 2 + loop 1 + discovery 1 + disabled 1）。  
生产编译：`cargo check -p coolzhu-web-console --offline` exit 0（`tmp/ha03-check.log`）。

## 限制

- 没有真实模型调用；HA03-01 的“受控响应可完成隔离查证”按 PLAN 留到 HA-11。
- 没有对 provider 不支持、发现服务恶意返回做动态注入；当前为单元/源码审计证据。
- 小窗口 MCP 预算只验证边界，不代表实际 token 计费精确。
