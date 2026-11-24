# 上下文增量折叠压缩（Context Fold）· 方案计划

- 文档编号：`PLAN-CONTEXT-FOLD-001`
- 日期：2026-11-10
- 状态：草案 v3（风险评审完成，v2 遗留的未核实项已补读代码；本轮只出计划，不含实现）
- 外部参考：[billion-context](https://github.com/ranxianglei/billion-context)（acp-kernel）；论文 `paper/model-driven-incremental-hierarchical-compression-*.md`（MIT，v0.2）
- 修订记录：v1 为初稿；v2 逐条对照代码核实假设，发现 3 个阻断级、4 个高风险问题（§1），据此改写设计（§3）与里程碑（§4）。v1 中被推翻的结论见 §1 各条"v1 原文"。v3 补读工具循环、Devin ACP / OpenAI 订阅、会话回滚与 Anthropic 请求体构造：R10 / R11 由推断改为已核实，新增 R13（floor 的五个写入点）与 §3.5 的轮内压力分支，M0 改为对已有 `chat_usage_events` 的查询（附录 A），新增 M6。v3.1：M0 已按附录 A 在实际运行库执行（§0.3），R10、R2 由实测数据证实，里程碑优先级据此调整（§4）。

## 0. 背景

### 0.1 billion-context 方法摘要

BC 是挂在 agent 与模型 API 之间的压缩代理，注入 `compress` / `decompress` / `search_context` / `acp_status` 四个工具由模型自主调用。以下数字均为其论文/README 自述，本仓库未复现：

| 机制 | 要点 | 其自述数据 |
|---|---|---|
| 增量折叠 | 只折叠自上次压缩以来的新增区间；摘要块生成后冻结；最近 ~50K 工作区保持原文 | 单块中位 185 tokens；常驻 ~70× |
| 前缀缓存友好 | 历史 append-only，折叠不动前缀 | 活跃节奏下 94.2% 输入走缓存；账单省 31–54% |
| 可逆 | 每块可单独解压 | re-fetch 调用 −63% |
| 层级化 | tier1→2→3，每条内容每层只折一次 | tier-2 仅触及 60/1711 块 |
| 对照结论 | 固定节奏折叠在重复负载上比模型自选便宜 37%，phase 负载上反之 | 说明"模型自主 compress"不是前置条件 |

注意 BC 压缩的主体是**工具输出**（论文称折叠内容"dominated by high-volume, low-density tool outputs"）。这一点对本仓库的适用性有直接影响，见 §1 R10。

### 0.2 coolzhuagent 现状（已对照代码核实）

**路径 B — web-console，主路径。** 实际有两层，v1 只分析了第一层：

1. **装配期预算裁剪**（`build_context_assembly_with_roster`，`main.rs:29899`）：newest→oldest 选入 `history_token_budget`，溢出进 `dropped_history`，`summarize_dropped_history`（`chat_tool_history.rs:123`）采样 ≤12 行 × 50 字符注入 system_prompt 末尾。
2. **回合后生命周期硬重置**（`apply_context_lifecycle_after_turn`，`main.rs:39615`）：`auto_compact_reached` 时写一条 L2 bead（最近 12 条消息、合计 ≤1600 字符），并执行 `session.context_reset_at = now`。该值即下一轮的 `history_floor_millis`，装配时 floor 之前的消息直接 `continue`（`main.rs:30017`），**既不进历史也不进 `dropped_history`**。

第 2 层才是长会话的实际压缩行为：一次性、全量、不可见地切断历史，只留一条走召回排序的 bead（投影后 confidence ≤0.4，`chat_tool_history.rs:156`，不保证被选中）。

触发条件（`main.rs:33083-33092`）：`percent ≥ auto_compact_percent`（默认 80）**或** `source==Estimated && assembly.truncated`。即在拿不到远端 usage 的 provider 上，装配只要丢过一条历史，本轮结束就硬重置。

预算实际取值（`main.rs:30573-30604`，`7574-7606`）：`history_token_budget = usable_context × 70%`，`usable = 模型窗口 − 输出预留 − 1024`。128K 窗口模型的历史预算在数万 tokens 量级；`ContextBuildOptions::default()` 的 3000 只在模型窗口 ≤8000 时生效。

跨轮历史只携带投影后的内容：`project()` 剔除 `reasoning / tool-call / tool-result`，工具只留"工具执行事实"一行（`chat_tool_history.rs:185-211`）。

**路径 A — `core-runtime/compact.rs::compact_session`。** 仅 CLI `ConversationRuntime` 使用。启发式单条摘要；`merge_compact_summaries` 二次压缩丢弃旧 timeline。

### 0.3 M0 实测基线（2026-11-15，已执行）

数据源：`C:\Users\zhupu\Desktop\AI_temp\.coolzhu\web-sessions.sqlite3`（launcher `last_workspace_root`，v0.2.147 安装后的实际运行库）。165 条 `chat_usage_events`、35 回合；协议分布 openai_chat_completions×160 + devin_acp×5；模型 swe-2×120 + agnes-3.0-flash×32。**样本小、以近期联调为主，趋势结论应定期复核。**

| 指标 | 实测 |
|---|---|
| `tool_feedback` 请求 | 124 次；**17.3M 总输入**（8.86M fresh + 8.46M cache_read），命中率 48.9% |
| `chat` 首请求（跨轮上下文） | 28 次；180K 总输入，命中率 **16.7%** |
| **轮内占总计费输入** | **≈96%**；跨轮 ≈4% |
| 回合内增长 | 9/26 回合 peak > 2× 首请求；avg 5.8 请求/回合，最多 41 次；峰值单请求 **389K** tokens |
| 协议命中 | Anthropic 路径样本不足（仅 5 条 devin_acp）；OpenAI 兼容如上 |

读法：

1. **R10 证实**：计费输入的绝对大头在轮内工具循环。跨轮历史确实很瘦（chat 首请求 avg ~5.4K），但本样本多为短任务。
2. **轮内未命中大头是新鲜工具结果**（每轮新追加的 tool_result 天然不在缓存里），不是前缀失效——所以 M6 的收益上限来自"少发工具结果原文"，而不是提高命中率。轮内已是 append-only。
3. **跨轮 16.7% 命中证实 R2 的代价真实存在**：滑动窗口 + bead 召回变化 + 上轮 user 边界改写，每轮首请求基本全量重计。绝对量小但 100% 可避免。
4. 未观察到 `cache_write_tokens`（全 0），说明该端点只报读不报写；Anthropic 原生协议接入前的基线缺数据。

## 1. 风险评审

严重度：**阻断** = 按 v1 实施会无效或出错；**高** = 必须在设计内解决；**中** = 需明确边界。

### R1（阻断）v1 没有处理生命周期硬重置，折叠会被它直接抹掉

- v1 原文："不改 `auto_compact_percent` 生命周期 bead 的既有语义（fold 块与其并存）"。
- 事实：见 §0.2 第 2 层。折叠块覆盖的区间在重置后落到 floor 之前；且 v1 的"尾段溢出走滚动摘要"会置 `truncated=true`，在 Estimated 来源下**每次溢出都立刻触发重置**。两者并存时折叠机制基本不会生效。
- 结论：折叠必须**取代**自动重置，而不是并存。`fold_enabled` 的会话上，`auto_compact_reached` 改为触发折叠；硬重置只保留为兜底（§3.5）。

### R2（阻断）"前缀缓存稳定"的前提在现有装配下不成立

v1 把缓存命中作为最大收益，但有三处独立的前缀破坏点，v1 的设计一处也没消除：

1. **滑动窗口**：历史超预算后，每轮新增内容都把最旧的入选消息挤出去，`messages[0]` 逐轮变化。v1 的"< gate 走滚动摘要"恰好让窗口在两次折叠之间持续滑动。
2. **system prompt 内的按查询召回**：`select_context_memory_beads` 以本轮 `current_user_text` 为查询（`main.rs:30369`），选中的 bead 渲染在 system prompt 中段（`main.rs:29605`），位于 v1 计划的块注入点**之前**。用户每换一个话题，其后全部内容的缓存都失效。
3. **Anthropic 协议路径没有缓存断点**：`llm-adapter` 全目录没有 `cache_control`（仅解析 usage 回执）。Anthropic Messages 的 prompt cache 需要显式断点，`MessageRequest.system` 的类型是 `Option<String>`（`types.rs:10`），Anthropic 请求体由 `serde_json::to_value(request)` 直接得到（`claw_provider.rs:555`），没有可挂断点的块结构。该路径当前命中率应为 0，与前缀是否稳定无关；要启用需把 system 改为块数组，涉及 `llm-adapter` 的公共类型和两个协议的序列化。OpenAI 兼容路径为自动前缀缓存，只受 1、2 影响。

- 实测（§0.3）：跨轮 `chat` 请求命中率 16.7%，失效代价确实存在；`tool_feedback` 48.9%，其未命中主要是新鲜工具结果而非前缀失效。Anthropic 原生协议样本不足。
- 结论：(a) 折叠改为高/低水位滞回，两次折叠之间窗口起点固定（§3.3）；(b) bead 位置与 `cache_control` 单列里程碑 M3；(c) 验收指标用真实的 `cache_read_input_tokens` 占比（`Usage` 已有该字段），v1 的 `fold_prefix_hash` 降为辅助诊断。
- 未消除项：上一轮 user 消息在成为历史后会被加上 `HISTORICAL_USER_BOUNDARY` 前缀、去掉本轮边界块，因此缓存最多延伸到上一轮 user 消息之前。影响仅一轮的尾部，接受。

### R3（阻断）块按 room 存储是错的

- v1 原文："room 级 `fold_blocks`"。
- 事实：
  - floor、预算、模型窗口都是**会话（agent）级**；同一房间多个 agent（群发接力、roster）各自装配，水位不可能共用。
  - 传给装配函数的历史不一定是房间消息：Fusion 角色线程传的是 `fusion_role_messages_list` 的结果，但 `chat_room_id` 仍填房间 id（`fusion/runtime.rs:784-799`）；`host_child_agent.rs:679`、`computer_use_planner.rs:661`、`multimodal_input.rs` 传空历史。按 room id 取块会把房间的折叠块注入角色线程。
- 结论：块的键为 `(history_scope, session_id)`；`ContextBuildOptions` 新增显式的 `fold_scope: Option<FoldScope>`，默认 `None`。首期只有普通聊天的 HTTP 路径传 `Room(room_id)`，其余调用点保持 `None`，行为不变。

### R4（高）在装配函数里落库会引入副作用

- v1 原文：§2.3 第 3 步"持久化块、推进 watermark……回到第 2 步重选"。
- 事实：`build_context_assembly*` 是同步纯函数，36 处调用，包括 GET 预览接口（`main.rs:14502`）和 20 余个单测。在其中落库意味着预览请求会改写会话状态；函数内部还会经 `session_context_reset_floor` 取 `session_store` 锁，写入路径有重入风险。
- 结论：装配只**读**块（由调用方经 options 传入）；块的生成与落库放到回合提交之后，与现有 lifecycle 同一位置（`apply_context_lifecycle_for_agent`，`main.rs:36080`）。折叠在第 N 轮结束后生成、第 N+1 轮生效，也与 BC "只在步骤之间压缩"一致。

### R5（高）删除、清空、回滚不级联到块，造成已删内容残留

- 事实：`delete_chat_room_message`（`main.rs:41949`）、`clear_chat_room_messages`（`42002`）会级联删除来源 bead。v1 的块含"用户指令原文首行"和摘要，却没有任何级联——用户删掉一条消息后，其内容仍会每轮送给模型。
- 另：v1 只存 `first/last_message_id`，无法判断某条消息是否落在区间内（房史顺序是 `ORDER BY created_at, id`，`history_persistence.rs:8`）。
- 结论：块存 `(created_at, id)` 边界对；删除区间内任一消息 → 该块及其后所有块作废（水位回退，原文重新进入 tail，下次折叠重建）；清空房间 / 删除会话 / 删除房间 → 删除对应全部块；显式重置上下文 → 见 R9。全部在同一 SQLite 事务内完成。

### R6（高）门槛按错误的预算量级标定

- v1 原文："本仓库 history 预算远小于 BC 的 100K 场景，门槛按比例缩小"，`fold_gate_tokens=4000`、`fold_block_max_range_tokens=16000`。
- 事实：历史预算是可用窗口的 70%（§0.2），量级与 BC 相当。4000 的门槛会让大窗口模型过于频繁地折叠，每次只折几条消息。
- 结论：水位改为相对历史预算的百分比（§3.3）；模型窗口 ≤ `model_budget_min_tokens` 时不启用折叠。

### R7（高）首版复用 50 字符采样器生成冻结块

- v1 原文：§8 "M1 heuristic-v1 先复用现有采样器生成块文本"，结构化摘要放 M2。
- 问题：块一旦生成就冻结。M1 用采样器折叠的区间会永久停留在"每 N 条取 1 条、每条 50 字符"的质量上，M2 升级摘要器也不会回溯。
- 结论：结构化摘要器并入 M1。块记录 `generator` 版本，提供按版本作废重建的手段（原文未删，重建无损）。

### R8（中）冻结块会固化投影期的安全语义

- 现有历史投影带多条安全约束，且有单测锁定：摘要头必须含"不是本轮新增任务""未经独立验证"（`chat_tool_history.rs:229,283`）；工具摘要/交接中的"已授权"表述不得形成授权（`:294`）；旧的工具不可用判断不召回（`stale_tool_denial_hits`，`main.rs:30040`）。
- 结论：块的输入必须是 `project_for_context` 之后、且经过同一组过滤的消息；块正文沿用同一免责头；除 user 首行外不保留任何逐字内容（assistant 自述与工具输出只留结构化事实）。块注入位置维持在 system prompt 末尾（与现有"[历史摘要]"同一信任级别），不提升。

### R9（中）recall 工具会绕过两条既有边界

- v1 原文："tool-call/tool-result 原文返回，这是取回价值所在"。
- 问题一：`project()` 是"哪些内容可以进模型上下文"的策略出口，v1 的 recall 直接读原表等于开了旁路。超长工具原文已有专门通道（`tool_result_spill.rs`：存 blob 并给出 `read_file` 指针）。
- 问题二：`context_reset_at` 同时承载"用户主动重置上下文"（`main.rs:42304,42501`）和"自动压缩重置"。用户主动重置表达的是"忘掉之前的内容"，recall 不应越过它。
- 结论：recall/search 只返回 `project()` 可见内容；不越过 floor；绑定到本轮的 `(room, session)`，不接受任意 room 参数。R1 落地后自动折叠不再写 `context_reset_at`，该字段回归"仅用户主动重置"的单一语义。

### R10（高，已核实并经实测证实）轮内工具循环是另一个增长源，且会误触发硬重置

- 事实（`run_shared_model_tool_loop`，`main.rs:29223-29480`）：每个工具反馈轮把一条 assistant `tool_use` 和一条 user `tool_result` 追加到 `history`，下一轮整体重发；所读范围内没有按预算裁剪的逻辑。轮数上限 `max_feedback_rounds` 默认 40（钳制 1–64）。唯一的约束是单条工具结果的投影上限（`tool_dispatch_service.rs:154` → `tool_result_spill`：超过 8000 字符时保留 6000 字符节选并给出原文指针）。
- 这部分内容不跨轮：回合结束后房史只留"工具执行事实"一行。所以**跨轮历史本来就瘦，BC 所针对的大体量工具输出在本仓库位于轮内**。
- 与硬重置的耦合：生命周期用的是本回合**最大**的一次远端 usage（`remember_largest_remote_usage`，`main.rs:33142`），即包含轮内全部工具结果的那次请求。长工具链回合可以单凭轮内增长越过 80%，随后触发硬重置——被重置掉的跨轮历史并不是占用的来源，而下一轮的起始上下文本来也不含这些工具结果。这种情况下的重置只有损失没有收益。
- 轮内前缀是 append-only 的，OpenAI 兼容路径在轮内已经能吃到自动前缀缓存；轮内的问题是窗口上限，不是缓存。
- 结论：(a) 触发条件必须区分"跨轮 tail 超水位"与"轮内压力"，后者不折叠也不重置（§3.5）。(b) 轮内折叠是独立课题，约束不同（保持 `tool_use` / `tool_result` 配对、不破坏轮内前缀），列为 M6。(c) **实测证实**（§0.3）：轮内占计费输入约 96%，是绝对主因；M6 优先级上调至与 M1 并列第一梯队。

### R11（中，已核实）折叠对 ACP 与订阅后端无意义，首期排除

- Devin ACP（`devin_acp/context.rs:122-335`）：远端会话自己持有历史，宿主每轮只发增量（未同步或被更正的消息 + 本轮消息）；system prompt 摘要未变时只发一句"沿用上一轮"。宿主侧折叠不会缩短远端已有的历史。另有硬约束 `assembly.messages.len() == selected_ids.len() + 1`（`:223`）：把块作为消息注入会让 ACP 回合直接失败，这也是 §3.2 把块放在 system prompt 而不是消息序列里的理由之一。
- OpenAI 订阅（`openai_subscription.rs:420-425`）：把 `assembly.messages` 整体序列化成一段文本提示，纯文本、无工具。
- 结论：两者 `fold_scope=None`。首期仅 `AgentSessionBackend::require_http` 通过的普通聊天路径。

### R12（低）其它

- **并发**：同一房间可能有多个 agent 并行结束回合。块以 `(scope, session_id, seq)` 唯一约束，落库时校验"当前末块 seq == 生成时读到的 seq"，不符则放弃本次折叠（下轮重试）。
- **预览与真实路径的历史来源不同**：预览用内存里按容量裁剪过的 `room.messages`（`main.rs:14491`），真实路径读全量房史。折叠后两者都从水位之后取 tail，差异反而缩小；预览需同样传入块。
- **路径 A 的 merge 修复**与折叠无依赖关系，独立成一个小 PR，不占里程碑。
- **配置项**：v1 七个开关过多，收敛为三个（§3.6）。

### R13（中，已核实）`context_reset_at` 有五个写入点，逐个加级联容易漏

- 写入点：新建会话（`main.rs:42304`，=now，含义是"加入之后的消息才可见"）、Goal 角色会话创建（`42369`）、`reset_session`（`42501`，用户清空）、`rollback_session`（`42693`，= 回滚边界，是一个过去的时刻）、生命周期自动重置（`39648`）。
- `rollback_session` 只删 `session_messages`（`HistoryEdit::AfterSessionCursor`），不动房间消息，但改了 floor 并按时间裁剪 bead。
- 结论：不在各写入点分别处理。块记录生成时的 `floor`，读路径只采用 `block.floor == 当前 floor` 的块，其余视为作废并在下次写路径清理。现有和将来的 floor 变更都自动使块失效。

## 2. 目标与非目标

- **G1**：`fold_enabled` 的会话上，用增量折叠取代"80% 硬重置"：历史分段折叠，每段只折一次，块冻结。
- **G2**：单块 150–300 tokens，保留文件路径、错误事实、结论、用户指令首行。
- **G3**：折叠可逆：原文不删，模型可经只读工具取回（受 R9 边界约束）。
- **G4**：删除/清空/回滚与块严格一致，不残留已删内容。
- **G5**：`fold_enabled=false` 时输出与现状逐字节一致。
- **G6（有条件）**：提高 provider 缓存命中。仅在 M0 数据支持时进入 M3。

非目标：模型自主 `compress` 与消息 refs（列为远期）；轮内工具循环压缩（视 M0 结论另行立项）；ACP / OpenAI 订阅 / Fusion 角色线程（首期不启用）；memory bead 召回策略本身。

## 3. 设计

### 3.1 数据模型

```rust
enum FoldScope { Room(String) }            // 首期仅此一种；Fusion 角色线程等后续再加

struct FoldBlock {
    scope: FoldScope,
    session_id: String,
    seq: u32,                              // 同 (scope, session) 内递增，决定注入顺序
    first_key: (u64, String),              // (created_at, message_id)，与房史排序一致
    last_key:  (u64, String),              // 末块的 last_key 即水位
    range_message_count: u32,
    range_token_estimate: u32,
    summary: String,                       // 冻结正文
    token_count: u32,
    tier: u8,                              // 1；tier-2 聚合留到 M5
    generator: String,                     // "heuristic-v2" | "model:<id>"
    floor: u64,                            // 生成时的 context_reset_at；与当前值不等即作废（R13）
    created_at: u64,
}
```

存储：新表 `context_fold_blocks`，主键 `(scope_kind, scope_id, session_id, seq)`，沿用现有 schema 迁移方式。只写 SQLite；是否进入 legacy JSON 导出见 §6 开放问题 3。

否决的替代方案：复用 memory bead。bead 走查询召回排序，无法保证固定顺序与必选，且投影会改写 kind/confidence。

### 3.2 读路径（装配，保持纯函数）

- `ContextBuildOptions` 增加 `fold_scope: Option<FoldScope>` 与 `fold_blocks: Vec<FoldBlock>`，由调用方在装配前读出并传入。默认空，所有未改动的调用点行为不变。
- 只采用 `floor` 与当前 `history_floor_millis` 相等的块（R13）。有块时：水位 = 末块 `last_key`；`room_history` 中 `key ≤ 水位` 的消息跳过（不进入选、不进 `dropped_history`）；其余按现有逻辑处理。
- 块渲染为一段，追加在 system prompt 末尾、现有"[历史摘要]"的位置之前，带与现有摘要相同的免责头。块的 token 计入 `system_tokens`。
- 水位之后仍溢出（单轮内容特别大时会发生）：沿用现有丢弃 + 滚动摘要，作为兜底，不新增逻辑。
- `ContextAssembly` 增加 `fold_block_seqs: Vec<u32>` 作为证据；`context_snapshot_id` 的输入加入块序号，块变化时快照 id 随之变化。

### 3.3 写路径（回合后折叠，高/低水位滞回）

位置：回合提交之后，取代 `apply_context_lifecycle_after_turn` 中的重置分支（仅对 `fold_scope` 非空的会话）。

- 触发（高水位）：水位之后的 tail 估算 tokens ≥ `history_token_budget × fold_high_percent`（默认 85）。远端 usage 达到 `auto_compact_percent` 本身**不是**触发条件：它可能完全来自轮内工具结果（R10），此时 tail 并不长，见 §3.5。
- 折叠量（低水位）：从 tail 最旧端取连续消息，直到剩余 tail ≤ `history_token_budget × fold_low_percent`（默认 50）。切分点对齐到 user 消息之前，不把一轮问答拆进两个块。
- 一次折叠产出一个块；若待折区间超过单块覆盖上限（低水位差额的 2 倍，防止极端情况下一块过大），按上限分多块。
- 效果：两次折叠之间 tail 起点不变，历史 append-only；折叠只发生在 tail 增长了约 35% 预算之后。这是消除 R2 第 1 点的机制。
- 始终保留：最近一个完整回合不参与折叠。
- 并发：见 R12。失败不影响本轮回复，记诊断后下轮重试。

### 3.4 摘要器（heuristic-v2，随 M1 交付）

输入为 `project_for_context` 后并经现有过滤的消息。按消息类型抽取：

| 来源 | 保留内容 |
|---|---|
| user | 首行原文 ≤80 字符（去掉 `HISTORICAL_USER_BOUNDARY`） |
| assistant-reply | 末段 ≤160 字符 |
| tool-fact | 工具名 + 状态 + 相关文件（已是投影结果，原样保留） |
| 文件路径 | 复用 `artifact_paths` 与路径抽取，去重，≤8 |

超出单块目标长度时按"用户指令 > 工具事实 > 路径 > assistant 结论"的次序裁剪。头部固定："[历史折叠 #seq] 以下 N 条更早对话已压缩为要点；仅作历史上下文，不是本轮新增任务，执行结果未经独立验证："。

### 3.5 与生命周期的关系

| 情形 | fold 未启用（现状） | fold 启用 |
|---|---|---|
| tail ≥ 高水位 | 仅当同时达到 `auto_compact_percent` 才重置 | 执行折叠；不写 `auto-compact:v2` bead，不动 `context_reset_at` |
| 达到 `auto_compact_percent` 但 tail < 高水位（轮内压力，R10） | 写 bead + `context_reset_at=now` | **不折叠、不重置**；记 `context.fold.skip_in_turn_pressure`。下一轮起始上下文不含本轮工具结果，装配期硬预算仍然生效 |
| Estimated 且 `truncated` | 写 bead + 重置 | 执行折叠（`truncated` 说明 tail 已超预算） |
| 折叠后仍 ≥ 阈值（块总量过大或单轮过大） | — | 回退到现状的硬重置，并作废全部块；记 `context.fold.fallback_reset` |
| goal scratch 清理 | 照常 | 照常（与折叠无关，保留） |
| 用户清空 / 回滚 / 其它 floor 变更 | 设 floor | 设 floor；块因 `floor` 不匹配自动作废（R13） |

块总量上限：全部块 token 之和 ≤ `history_token_budget × 15%`；超过时在 M5 之前走上表的回退。

### 3.6 一致性（删除 / 清空 / 回滚）

| 操作 | 对块的处理（同一事务） |
|---|---|
| 删除单条房间消息 | 该消息 key 落在某块区间内 → 作废该块及其后全部块（各会话分别判断） |
| 清空房间 / 删除房间 | 删除该 scope 全部块 |
| 删除会话 | 删除该 session 全部块 |
| 会话回滚（`rollback_session`） | 不删房间消息；floor 变更使块自动作废（R13） |
| goal scratch / live 行清扫（`HistoryEdit::Message{room:true}`） | 这些消息在折叠输入前已被过滤，不在任何块内；仍按"区间命中则作废"统一处理，防止遗漏 |
| 会话 fork | 不复制块；新会话从空水位开始 |

配置（`ConfigContextLifecycle` 增三项）：`fold_enabled`（默认 false）、`fold_high_percent`（85）、`fold_low_percent`（50）。

### 3.7 取回工具（M2）

- `search_context {query, max_results?}`：在本会话的块摘要与水位前的可见投影上做关键词匹配，返回 `seq`、消息 id、片段。
- `recall_history {block_seq? | around_message_id?, max_tokens?}`：返回对应区间的投影后原文，总量与单条均有上限。
- 边界（R9）：只读；作用域固定为本轮的 `(room, session)`；不越过 `history_floor_millis`；只返回 `project()` 可见内容。
- 注册为只读工具，遵循会话既有的工具策略。

## 4. 里程碑

| 里程碑 | 内容 | 准入 / 退出条件 |
|---|---|---|
| ~~**M0 基线测量**~~（已完成，见 §0.3） | `chat_usage_events` 按请求记录 `input / cache_read / cache_write`、`request_kind`、`turn_id`、`protocol_snapshot`、`model_snapshot`；附录 A 查询已执行 | 结论：**轮内占计费输入 ~96%，是绝对主因**。优先级据此定为：M6 轮内折叠 ≻ M1 跨轮折叠 ≻ M3 前缀稳定化；§3.5 轮内压力误重置分支建议尽早独立落地（小改动、直接止血） |
| ~~**M1 折叠核心**~~（已实施 2026-11-17，`fold_enabled` 默认 false 灰度） | §3.1–3.6：表、读路径、回合后滞回折叠、heuristic-v2、取代硬重置、删除级联、三项配置。仅 HTTP 后端 + Room scope | 准入：M0 支持。退出：§5 单测与 E2E 通过；`fold_enabled=false` 快照一致。实施记录见 §9 |
| **M2 取回工具** | §3.7 | 准入：M1 灰度无回退异常 |
| **M3 前缀稳定化** | (a) Anthropic 路径加 `cache_control` 断点（需把 `MessageRequest.system` 改为块结构，见 R2）；(b) 按查询召回的 bead 移出 system prompt 中段（候选：移到本轮 user 消息前的独立块）。(b) 改变模型看到的结构，需单独评审 | 准入：M0/M1 数据显示命中率确实受这两点限制 |
| **M4 模型写摘要** | `generator=model:*`，回合后异步，失败回退 heuristic-v2 | 准入：有证据显示启发式块导致取回率偏高 |
| **M5 tier-2 聚合** | 块总量超上限时聚合最旧的若干块，取代 §3.5 的回退 | 准入：线上出现 `fallback_reset` |
| ~~**M6 轮内工具结果折叠（优先级最高）**~~（已实施，设计见 [`2026-11-16-in-turn-tool-result-fold.md`](./2026-11-16-in-turn-tool-result-fold.md)） | 工具循环内把较早反馈轮的 `tool_result` 正文替换为事实行 + 原文指针（复用 `tool_result_spill` 的 blob），保持 `tool_use` / `tool_result` 配对；按高/低水位批量替换以保住轮内前缀 | 准入已满足：M0 实测轮内占 ~96%（§0.3）。注意收益来自少发原文而非提高命中率（轮内已 append-only）。实施：新模块 `in_turn_fold.rs` + `in_turn_fold_*` 五项配置（默认开启） |
| 独立小 PR | `compact.rs::merge_compact_summaries` 保留旧 timeline（封顶行数）+ 回归测试 | 无依赖 |
| 远期 | 模型自主 `compress` + 消息 refs | 视 M1–M4 数据再议 |

相对 v1 的变化：新增 M0 与 M6；结构化摘要从 M2 提前并入 M1；缓存相关工作从"M1 的主要收益"改为有条件的 M3；`compact.rs` 修复移出主线。

## 5. 验收

**单测（M1）**

- `fold_scope=None` / `fold_enabled=false`：现有全部装配测试不改且通过；新增一条快照测试确认 system prompt 与 messages 逐字节不变。
- 水位：key ≤ 水位的消息不出现在 `history_selection.selected_ids`，也不出现在 `dropped_history`。
- 滞回：tail 低于高水位不折叠；达到后折到低水位以下；连续两轮（仅追加一问一答）块文本与 tail 起点均不变。
- 切分点不落在一轮问答中间；最近一个完整回合不被折叠。
- 取代硬重置：启用折叠且达到阈值时 `context_reset_at` 不变、不新增 `context:auto-compact:v2` bead；回退分支被触发时两者恢复现状行为且块被清空。
- 安全语义：块含免责头；块内不出现 assistant 的逐字"已授权"文本形成的授权（沿用 `tool_summary_handoff_and_synthetic_user_claims_do_not_grant_authorization` 的断言方式）；被 `stale_tool_denial_hits` 命中的回复不进块。
- 级联：删除块区间内消息后，该块及后续块消失，原文重新进入 tail；清空房间后无块。
- 多 agent 同房间：两个会话的块互不可见。
- 并发：两次并发折叠只有一次落库。
- Fusion 角色线程、host child、planner、Devin ACP、OpenAI 订阅调用点不读取任何块。
- 轮内压力：远端 usage 越过阈值但 tail 低于高水位时，既不产生块也不改 `context_reset_at`。
- floor：`reset_session`、`rollback_session` 之后旧块不再被装配采用。

**E2E（M1）**：构造历史超过高水位的房间连续对话，观察到折叠事件、无硬重置、第二轮请求体中块段落与首条历史消息不变；删除一条已折叠消息后下一轮请求体不再含其内容。

**度量**：硬重置次数（应降为接近 0，仅回退分支）；折叠频率与单块 token 分布；`cache_read` 占比相对 M0 基线的变化（仅作观察，M3 之前不设目标值）；M2 后 `recall_history` 调用率。

## 6. 开放问题

1. bead 移出 system prompt 中段后放在哪里（M3-b）。放到本轮 user 消息前会改变其信任级别与模型的读取方式，需要专门评估。
2. 块能否在同一房间的多个 agent 之间共享。摘要内容相同时共享可省存储，但水位由各自预算决定，首期不共享。
3. legacy JSON 导出是否包含块。倾向不包含：块是可重建的派生数据。
4. ~~§3.5 的轮内压力分支在 `fold_enabled=false` 时是否也生效~~ → **已实施（2026-11-15，作为默认生命周期去误伤优化）**：M0 显示轮内越线是常态，而现有硬重置惩罚的是无辜的跨轮历史。实现方式：`ContextAssembly` / `ContextUsageSnapshot` 新增 `history_overflowed`（语义严格等于"旧历史因 history 预算被丢弃"，即 `!dropped_history.is_empty()`，不含 user 正文/图片截断）；`apply_context_lifecycle_after_turn` 在 `auto_compact_reached && !history_overflowed` 时记 `context.compact.skip_in_turn_pressure` 诊断后直接返回，不写 bead、不改 `context_reset_at`。该行为与 `fold_enabled` 无关，对所有会话生效；回退手段为还原本改动。回归测试：`context_lifecycle_skips_reset_when_history_did_not_overflow`；原有 `context_lifecycle_auto_compact_adds_memory_floor_and_removes_goal_scratch` 以 `history_overflowed: true` 保持原 compact 路径。
5. M6 与 BC 的做法更接近（工具输出、模型可见的区间引用）。M0 之后是否直接评估采用 BC 的消息 refs + 模型自选区间，而不是自研启发式。

## 7. 核实范围

已读代码：`build_context_assembly_with_roster` 历史选择与摘要注入段、`context_build_options_for_agent_with_floor_and_room`、`apply_context_lifecycle_after_turn` 及其触发条件、`chat_tool_history.rs` 全文、`history_persistence.rs` 全文、房间消息删除与清空入口、`rollback_session`、`reset_session`、`context_reset_at` 全部写入点、`fusion/runtime.rs` 的角色轮次装配、`run_shared_model_tool_loop` 的请求构造与历史追加段、`devin_acp/context.rs::prepare`、`openai_subscription.rs` 的消息装配、`build_anthropic_messages_request` 开头与 `MessageRequest.system` 类型、`request_usage.rs` 的 `chat_usage_events` 写入与迁移、`compact.rs` 全文。

仍未核实：

- `run_shared_model_tool_loop` 读了 29223–29480 行，其后只检索了含 `history` / `budget` 等关键词的行，没有逐行读完。"循环内无预算裁剪"是基于这一范围得出的。
- `fork_session` 只看了函数开头，未确认它是否复制 `context_reset_at` 与 bead。
- BC 的全部数据为其自述，未复现。
- M0 样本仅 165 行、35 回合，且以近期联调为主（fusion E2E 占多）；`chat_usage_events` 各字段在各 provider 下的语义（尤其 `input_tokens` 是否已扣除缓存部分）未逐一对照源端文档，A1 的 hit_ratio 以"总输入 = fresh + 缓存读"口径计算。

## 附录 A：M0 查询（只读）

对运行库以 `mode=ro` 打开执行。`usage_known_mask & 1` 表示 input 已知。

```sql
-- A1 各协议 / 请求类型的输入构成与缓存命中占比
SELECT protocol_snapshot, request_kind, COUNT(*) AS n,
       SUM(input_tokens) AS fresh_in,
       SUM(cache_read_tokens) AS cache_read,
       SUM(cache_write_tokens) AS cache_write,
       ROUND(1.0 * SUM(cache_read_tokens)
             / NULLIF(SUM(input_tokens + cache_read_tokens + cache_write_tokens), 0), 3) AS hit_ratio
FROM chat_usage_events
WHERE usage_known_mask & 1 != 0
GROUP BY 1, 2
ORDER BY fresh_in DESC;

-- A2 每回合：首请求（跨轮上下文）与回合内峰值请求的总输入
WITH r AS (
  SELECT turn_id, request_kind,
         input_tokens + cache_read_tokens + cache_write_tokens AS total_in
  FROM chat_usage_events
  WHERE turn_id IS NOT NULL AND usage_known_mask & 1 != 0
    AND request_kind IN ('chat', 'tool_feedback')
), t AS (
  SELECT turn_id,
         MAX(CASE WHEN request_kind = 'chat' THEN total_in END) AS first_in,
         MAX(total_in) AS peak_in,
         COUNT(*) AS requests
  FROM r GROUP BY turn_id
)
SELECT COUNT(*) AS turns, AVG(requests) AS avg_requests,
       AVG(first_in) AS avg_first_in, AVG(peak_in) AS avg_peak_in,
       SUM(peak_in > 2 * first_in) AS turns_peak_over_2x_first
FROM t WHERE first_in IS NOT NULL;
```

读法：

- A1 的 `hit_ratio` 在 Anthropic 协议上预期为 0（R2）。OpenAI 兼容协议上，`chat` 行反映跨轮前缀稳定性，`tool_feedback` 行反映轮内前缀稳定性，两者之差大致是 R2 第 1、2 点的代价。
- A2 的 `avg_first_in` 与 `avg_peak_in` 之比说明跨轮上下文与轮内增长各占多少。判断"越线回合里首请求是否已越线"还需要各模型的窗口大小，表里没有，按 `model_snapshot` 导出明细后对照模型表计算。
- 两条查询的列名来自 `request_usage.rs` 的写入语句；`input_tokens` 在各协议下是否已扣除缓存部分未逐一确认，解读 A1 前先抽几行核对。

## 9. 实施记录

### M1 跨轮 FoldBlock 核心（2026-11-17）

**代码**：新模块 `modules/gui-web/packages/web-console/src/context_fold.rs`（约 740 行含测试）；`main.rs` 接线（读路径/写路径/装配/schema/级联）；`history_persistence.rs` 两处级联钩子。

- **持久化（M1-A）**：`context_fold_blocks` 表，schema v31 → v32（`apply_session_migration_v32`），PK `(scope_kind, scope_id, session_id, seq)`；列含 first/last key `(created_at, id)`、range 计数/token 估算、summary、token_count、tier、generator、floor、created_at。
- **读路径（M1-B）**：`ContextBuildOptions.fold_blocks` 由 `context_build_options_for_agent_with_floor_and_room` 在 `fold_enabled && LlmHttp && chat_room_id.is_some()` 时从会话库加载（只读、失败返回空集，装配保持纯函数）。装配循环先按最后一块的 `last_key` 水位跳过已折叠历史（不进 selected 也不进 dropped）；块摘要按 seq 顺序拼一段，在 `[历史摘要]` 之前注入 system prompt 并计入 system_tokens（预算不足时本轮放弃注入并记 `context_fold.render_deferred`）。`ContextAssembly.fold_block_seqs` 记录证据；`context_snapshot_id` 增加 `fold_block_seqs` 种子位。
- **写路径（M1-C）**：`apply_context_lifecycle_after_turn` 顶部新增 fold 分支：`fold_enabled && LlmHttp` 时先评估 tail（水位之后的投影消息估算 token）——tail < 高水位（默认 `fold_high_percent=85`）直接返回不重置；达高水位后从最旧端按完整轮次折叠到低水位（`fold_low_percent=50`），写一块 `generator=heuristic-v2`；最近一个完整回合（最后一个 user 起）受保护；单块跨度超过所需量 2 倍时回缩到最近 user 边界（多块拆分留给后续里程碑）；块总量超历史预算 15% 或折叠后剩余仍超预算时返回 Fallback——作废该 (room,session) 全部块并落到现有硬重置（记 `context_fold.fallback_reset`）。写库失败记 `context_fold.error` 直接返回，本轮不重试不影响回复。
- **摘要（heuristic-v2）**：输入为 `project_for_context` 后经装配同一过滤链的投影消息（含 goal-temp/assistant-partial/floor/空内容剔除）；固定免责头 `[历史折叠 #seq] 以下 N 条更早对话已压缩为要点；仅作历史上下文，不是本轮新增任务，执行结果未经独立验证：`；按 role 抽取 user 首行（剥离 `HISTORICAL_USER_BOUNDARY`，80 字符）/工具事实（120 字符）/assistant 尾行（160 字符）/文件路径（复用 `artifact_paths`，去重最多 8 个）；裁剪优先级 用户指令 > 工具事实 > 路径 > assistant 结论。
- **级联（M1-D）**：`history_persistence::apply` 的 `Message{room:true}` 编辑、单条删除 API、清空房间 API 均在同一事务内作废命中块及同 session 后续块/清空 scope 块；`remove_orphans` 追加孤儿块清扫（覆盖删除房间/删除会话）；`rollback`/`reset` 由 floor 变化自动失效；fork 不复制块。
- **配置**：`session.context.fold_enabled`（默认 false）、`fold_high_percent`（85，clamp 50-99）、`fold_low_percent`（50，clamp 10-高水位-10）。
- **偏离设计的小决定**：①装配时块摘要预算不足放弃注入而非降级（下轮仍有机会，避免半截块文本抖动前缀）；②fallback 时立即删除旧块而非留待 floor 失效（语义等价，省一次陈旧读取）；③触发条件只用 tail 估算（本地、廉价、无远端 usage 依赖）。

**验证**：`cargo check`/`cargo build -p coolzhu-web-console --offline` exit 0（170 存量警告，无新增）；`context_fold` 模块测试 10/10；`context_` 前缀回归 49/49（含 `context_lifecycle_*`、`context_builder_*`、snapshot 变更测试）；`room` 前缀回归 57/57（含 `cascade_delete_chat_room_*`、`clear_chat_room_messages_*`）。

**未完成/待办**：`fold_enabled=false` 时块集恒为空，装配输出与启用前逐字节一致（§5 首条天然满足）；`fallback_reset` 恢复 `context_reset_at`+bead 复用原有代码路径，由 `context_lifecycle_auto_compact_*` 间接覆盖。

### 安装版真实模型 E2E（2026-11-20，agnes-3.0-flash）

**构建/安装**：`dist/CoolzhuAgent-0.2.149.msi`（269,400,285 B，SHA256 `1A57D084…`，unsigned，release）；`package-all`+`build-msi` 全证据链通过（payload 1,187 文件、包报告 `pkg-report-release-20261120-052621373-599514ee`、source snapshot `d5a11604…` 构建期不变、vcs 记录 dirty_against_commit=true 即未提交 WIP）。`msiexec /qn` 首次非提权失败（1730 需管理员删旧版），`Start-Process -Verb RunAs` 提权后安装成功，注册表 0.2.149，安装目录二进制时间戳与本次构建一致。

**运行配置**：运行工作区 `C:\Users\zhupu\Desktop\AI_temp`（`/api/workspace` 热切换），`[session.context] fold_enabled = true`；agnes-3.0-flash 走 `https://apihub.agnes-ai.com/v1`（custom/OpenAI 兼容，DPAPI 凭据），窗口按默认表取 64K。启动后会话库自动迁移 v31→v32（`schema-backups/` 留有迁移前备份）。

**过程**：agnes 首轮真实请求 12,686 input（history loaded=126，远端 usage 确认窗口 64K）；投影 tail 明显低于 fold 高水位（约 31.8K），**NoTrigger 行为正确**。随后在房间 `room-1793631023416` 注入 6 条约 21KB 的"事实记录"用户消息（含 6 个可验证事实点 + 填充段），每轮真实模型往返。

**结果（全部实测观测，非推断）**：

- 每轮结束后生命周期评估 tail，达标即写一块：共生成 6 个块（seq 1–6，generator=`heuristic-v2`），最大块覆盖 132 条消息/26,611 token → 544 token 摘要，其余每块 3 条/约 22K → 77–90 token。**增量折叠按设计工作**：水位单调前进，每轮只折最旧完整回合。
- 输入 token 实测曲线：24,543 → 36,513（块生效前爬升）→ **30,051 / 30,178 / 30,305 稳定平台**（折叠后不再随历史线性增长）→ 召回轮 **18,630**（history loaded 仅 2 条原始消息，其余全部走块摘要）。对比不折叠的预期轨迹（45K+），本轮实测下降约 48%；并首次观测到 `cache_read_tokens=4096`。
- **保真度**：召回轮要求逐条复述 6 个事实点——**全部正确**（前 5 个事实仅存于块摘要、原文已在水位下；第 6 个在 tail 中）。
- `context_reset_at` 全程未变（保持 1793199383455）；无 `auto-compact:v2` bead、无硬重置、无 `fallback_reset`。
- 块摘要内容质量：免责头正确，用户指令行保留了"请记住…= 值"的关键事实结构，助手结论行捕获"已记录 N"。

**未覆盖**：M6 轮内折叠未实测（本序列无工具调用；agnes 该会话未暴露工具链路）；`fallback_reset`、删除级联、多 agent 同房间隔离未在真实环境触发（单测覆盖）；`fold_enabled=false` 逐字节等价未在真实环境对比（由"块集恒空"的构造保证）。
