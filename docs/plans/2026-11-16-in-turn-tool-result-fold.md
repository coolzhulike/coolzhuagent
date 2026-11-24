# M6 轮内工具结果折叠 · 设计

日期：2026-11-16 · 状态：已实施（见 §10） · 上游：[`2026-11-10-context-fold-compression-plan.md`](./2026-11-10-context-fold-compression-plan.md) §4 M6

## 0. 问题与实测依据

工具循环（`main.rs:29258`，`for round in 0..=max_tool_feedback_rounds`）每轮把

- `assistant` 消息：`ToolUse`（含完整 `input` 入参）+ `Thinking`
- `user` 消息：`ToolResult`（`dispatch.tool_result_text` 全文）

追加进 `history` 并在下一请求整体重发（`main.rs:29450-29479`）。M0 实测（§0.3）：`tool_feedback` 占计费输入 **~96%**（17.3M tokens），单回合峰值 389K、最多 41 次请求。

两个加剧因素（已对照代码核实）：

1. **派发层 spill 不省上下文**。`tool_dispatch_service.rs:154` 调 `tool_result_spill::for_context(text, db_path, false)` —— `can_read_original` 硬编码 `false`：>8000 字符的结果会存 blob，但返回头 + **完整原文**（`tool_result_spill.rs:13`，"本轮没有可用的原文读取工具，以下保留完整结果"）。即单条大结果在首次进入上下文时没有任何上界。
2. **`ToolUse.input` 无界**。`write_file`/`edit_file`/`bash` 等入参全文随 assistant 消息每轮重发；一次几十 KB 的文件写入会永久占用后续全部轮次。

M6 只解决**稳态增长**：已消费过的旧结果在后续请求中替换为"事实行 + 原文指针"。首次视图（模型第一次看到该结果的那一次请求）不截断——模型必须先见到结果才能行动，这是不可压缩的下界。单轮超大结果造成的峰值只能靠后续模型摘要（M4）或首轮视图截断（见 §6 可选项）缓解，不在本设计内承诺。

## 1. 设计总览

在工具循环内维护折叠状态：每当 `history` 的**轮内追加段**（`base_history.len()` 之后的消息）估算 token 数超过**高水位**，把其中"最旧优先、除最新一轮问答对"的全部 `ToolResult` 正文与超限 `ToolUse.input` 就地替换为折叠行，直到段落降到**低水位**以下。

- **只动轮内段**：`history[in_turn_start..]`；`base_history`（跨轮回装配结果）逐字节不动。跨轮压缩归 M1/生命周期，互不重叠。
- **协议配对不变**：只替换 `ToolResult` 内部 `ToolResultContentBlock::Text.text` 与 `ToolUse.input` 的值；`tool_use_id`、`is_error`、消息顺序、块结构全部保留。Anthropic 校验靠 `tool_use_id` 配对，不读正文。
- **批量滞回保前缀**：高水位触发、一次折到低水位，而不是每轮各折一条——两次折叠之间轮内前缀保持稳定，`tool_feedback` 48.9% 的缓存命中率不被破坏（缓存本来就按前缀命中，折叠点之前的字节变化一次后重新冻结）。
- **幂等**：已折叠块不再二次折叠（前缀标记识别）；后续轮次新变旧的结果在下次越线时才折。

## 2. 触发与水位

估算函数复用 `small_context_mcp_prompt_budget`（`main.rs:35031-35045`）的逐块口径，提取为 `estimate_input_message_tokens(&InputMessage, image_token_estimate) -> u32`：

- `Text`/`Thinking` → `estimate_bead_tokens(text)`
- `ImageUrl` → `policy.image_token_estimate`
- `ToolUse` → `estimate_bead_tokens(name) + estimate_bead_tokens(input.to_string())`
- `ToolResult` → `estimate_bead_tokens(serde_json::to_string(content))`
- 每块 +8、每消息 +8 结构开销

水位只看轮内追加段 `history[in_turn_start..]` 的估算和：

| 配置（`ConfigContextLifecycle` 增项） | 默认 | 说明 |
|---|---|---|
| `in_turn_fold_enabled` | `true` | 总开关；关掉完全恢复原行为 |
| `in_turn_fold_high_tokens` | `32_000` | 轮内段估算超过此值触发一次折叠 |
| `in_turn_fold_low_tokens` | `16_000` | 折叠到低于此值停止（最旧优先） |
| `in_turn_fold_excerpt_chars` | `1_200` | 无读取能力时保留的头部节选字符数 |
| `in_turn_fold_tool_use_input_chars` | `4_000` | `ToolUse.input` 序列化超过此字符数才 stub |

定标依据：M0 单回合峰均 ~30K tokens。32K 高水位意味着普通回合（<10 轮中小结果）完全不触发；只有真正失控的回合付折叠代价。`input_tokens` 计费口径下 32K→16K 的折叠收益是**后续每轮少发 ~16K**，40 轮回合可省数百万输入。

## 3. 折叠单元

对每个 eligible `ToolResult` 块产出单行折叠文本：

```
[工具结果已折叠；模型已在此前请求中读过全文。tool={name} status={completed|failed}
摘要：{dispatch.summary_text}
原文：SHA256={sha256} 字节数={n} 已存 tool-results/{file_name}。{pointer 或 excerpt}]
```

- `name`/`summary_text`/`is_error`：追加结果时在循环状态里记录 `tool_use_id → FoldMeta`（参数修复路径 29373 同样登记，`summary_text` 用修复原因）。
- **有读取能力**（`sent.exposes("read_file")`，`request_binding.rs:59`）：pointer = `read_file({"path":...,"character_offset":0,"max_chars":12000})`，沿用 spill 已有约定，模型已认识该格式。
- **无读取能力**：不放指针（`tool_result_spill` 的铁律：未开放工具不给续读指针），改为 `摘要 + 头部 excerpt_chars 字符节选`。仍然是有界有损，但保留了开头关键信息；原文 blob 依然落盘，后续若轮内开放了读取工具，新折叠行可带指针（已折行不回改）。
- `ToolResultContentBlock::Json`：序列化后按同规则处理；`is_error` 标志保留在块上不动。
- **超大 `ToolUse.input`**：序列化 > `in_turn_fold_tool_use_input_chars` 时替换为 `{"_folded":true,"original_bytes":n,"sha256":"...","note":"工具入参已折叠；执行事实见对应 tool_result"}`。配对不依赖 input 值，安全。`write_file` 类大入参是主要收益点之一。

## 4. 原文落盘

复用 `content_blob_store::put(root, bytes, "txt")`（内容寻址，`sha256-{digest}.txt`，天然去重）。root 解析与 `truncate_tool_result_for_context`（`main.rs:30642`）一致：

```
parent.goal_phase.db_path() 或 parent.runtime_db_path 的父目录 + "tool-results"
```

被折文本是派发层 spill 投影后的版本（>8000 时本身已含 spill 头 + 原文）——不追求溯源到派发前原文，内容寻址保证同内容不重复占盘；blob 保存失败时退化为 `摘要 + excerpt`，不 panic、不丢状态行。

## 5. 实施形态

- 新模块 `src/in_turn_fold.rs`：纯函数 `fold_stale(history: &mut Vec<InputMessage>, base_len, metas, opts, blob_root, can_read) -> FoldReport`，全部逻辑可单测。
- 循环内（`main.rs` 工具循环）三处接线：
  1. `let in_turn_start = base_history.len();` + `let mut fold_metas: HashMap<String, FoldMeta>`（循环外初始化）。
  2. 追加结果处（29368-29376 参数修复、29422-29479 正常派发）登记 meta。
  3. `history = Some(next_history)` 之后、`continue` 之前调 `fold_stale`；输出 `FoldReport{folded_blocks, tokens_before, tokens_after}`，非零时 `diagnostics::info("context","in_turn_fold",…)` + `diag!`。
- `can_read`：本轮 `sent.exposes("read_file")`（该判定针对刚发出的请求；下一轮工具集通常相同，偏差只影响指针措辞，不影响正确性）。

## 6. 可选/排除项

- **首轮视图截断（不在本期）**：把 `can_read_original` 真实值穿透派发层，使首次进入上下文的单结果也有界。需改 `dispatch_model_tool_calls_parallel → tool_dispatch_service` 的签名链；收益是削单轮峰值，与稳态折叠独立，单独立项。
- **模型写轮内摘要**：事实行（`summary_text`）已证明够用于房史投影；升级摘要归 M4。
- **跨轮折叠**：M1，不重叠。
- **Thinking 块**：保留。模型推理链是后续决策依据，且多数 provider 要求原样回传。

## 7. 测试清单（`in_turn_fold.rs` + 循环接线测试）

- 折叠后 `tool_use_id`/`is_error`/消息数/块数不变，仅 text/input 缩小。
- 最新一轮问答对逐字节不变；`base_history` 段逐字节不变。
- 低于高水位不动手；一次折叠后 ≤ 低水位（eligible 足够时）。
- 无 `read_file`：无 `read_file(` 字样，excerpt ≤ `excerpt_chars`；有 `read_file`：含指针且 blob 可经 `runtime::read_file_character_range` 读回原文。
- 二次调用对同一段不再改动（幂等/前缀稳定断言）。
- 参数修复产生的 `is_error` 结果同样可折（meta 已登记）。
- blob 写入失败：退化为摘要 + excerpt，不 panic。
- `ToolUse.input` 超阈值 stub，未超不动。

## 8. 验收指标

- 本地复跑 M0 查询对比：实施后 `tool_feedback` 单请求 `input + cache_write` 均值应显著下降；`cache_read` 命中占比不降（前缀仍 append-only）。
- 诊断事件 `context.in_turn_fold` 出现频率：普通回合为 0，长工具回合每回合 1–3 次。
- 回归：`cargo test -p coolzhu-web-console --offline` 全绿（重点 `tool_loop_coordinator`、`stream_model_tool_loop_reuses_full_context_assembly`、tool_feedback 相关测试）。

## 9. 风险

- **模型中途需要旧结果细节**：有读取工具时可 `read_file` 续读；无读取工具时剩摘要 + 1200 字符节选——与房史只存事实行的现状同级，风险可接受，`in_turn_fold_enabled=false` 一键回退。
- **估算偏差**：`estimate_bead_tokens` 是粗估，水位定宽松（32K/16K），误差不会引起行为抖动。
- **`sent.exposes` 与实际工具集漂移**：只影响指针措辞，不影响配对与正确性。

## 10. 实施记录（2026-11-16）

- 新模块 `src/in_turn_fold.rs`：`fold_stale` 纯函数 + `FoldMeta` / `InTurnFoldOptions` / `FoldReport` + `estimate_input_message_tokens`（与 `small_context_mcp_prompt_budget` 共用，原内联估算已重构为调用它）。
- 配置：`ConfigContextLifecycle` / `ContextLifecyclePolicy` 增加 `in_turn_fold_enabled`（默认 true）、`in_turn_fold_high_tokens`（32K）、`in_turn_fold_low_tokens`（16K）、`in_turn_fold_excerpt_chars`（1200）、`in_turn_fold_tool_use_input_chars`（4000）。
- 接线（`run_shared_model_tool_loop`）：`in_turn_start = base_history.len()`、`fold_metas` 在参数修复与正常派发两处追加时登记；两处 `history = Some(...)` 之后调用 `fold_in_turn_history`（`can_read` 取 `sent.exposes("read_file")`）。非流式 `api_chat_send` 路径无回灌循环，不涉及。
- 辅助：`frozen_tool_results_db_path` / `tool_results_store_root` 从 `truncate_tool_result_for_context` 提取复用，blob 根与 spill 完全一致。
- 观测：`diag!` + `diagnostics::info("context","in_turn_fold",…)`。
- 单测 9 个：配对保持/最新轮与 base 不动/水位不触发/无读取能力节选/指针续读回原文/幂等/缺 meta 仍折/blob 失败降级/ToolUse.input stub/关闭开关 noop。
- 与设计的偏差：折叠行含 `status={completed|failed}`（来自 `is_error`）；`ToolUse.input` stub 同步落 blob 并在 stub 内记 `sha256`。
