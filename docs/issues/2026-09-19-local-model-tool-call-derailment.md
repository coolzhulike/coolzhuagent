# 本地模型（bonsai）在 agent 下的工具调用脱轨问题

- 日期：2026-09-19
- 模块：`gui-web/web-console`（工具暴露与 Tool-Loop）、`computer-use/computer-use-core`（supervisor 阻断）
- 严重度：高（本地模型无法完成任何"写文件/落地产物"类长任务）
- 状态：2026-09-19 已实现代码修复及回归用例；本地 bonsai 实机复测待在服务可用时执行。
- 本文用途：交给后续模型做深入分析与修改，因此尽量把"已确认事实"与"未验证推断"分开写

---

## 1. 现象

### 本轮修复补充

- 新定位的直接诱因：`select_tools_for_request` 原先在上下文 ≤ 16K 时只保留 `computer_use_perform`，导致文件任务只看到桌面工具；现保留常用文件、搜索、命令工具，按当前任务与延续上下文决定是否暴露 UI 入口。
- `dev_open_permissions` 与模型工具开关/暴露范围解耦；调试构建默认完全访问，已有显式配置优先，发布构建仍默认关闭。
- 新增按会话配置的工具开关、范围、Computer Use 开关和自定义名单；`dispatch-only` 不额外加入 UI 工具，白名单也约束 UI，执行入口再次校验策略。
- 两条会话链路对 UI 终态后的再调用、伪工具正文、关闭工具后的结构化调用和反馈轮数上限，最多执行一次无工具恢复，保留原请求和已执行结果。恢复仍失败时明确报告未完成，不宣称工具已执行。
- 语义调度的文件写入仍经普通权限运行时执行，不再因 UI 熔断一起被禁；正文 `<tool_call>` 永不自动转为执行授权。
- 真实模型回复后不再运行旧的外层强制 UI/语义补执行，避免绕过关闭工具和单轮执行边界。

### 1.1 变体 A：工具开启时（`dev_open_permissions = true`，即仓库默认交付配置）

让本地会话（`coolzhu-local` → bonsai 27B，`http://127.0.0.1:8080/v1`）执行一个**纯生成类任务**：

> Write a single self-contained HTML file containing an inline SVG of a pelican riding a bicycle.
> The SVG must be detailed and recognizable. Output only the HTML.

观察到的结果：

1. 27B **不去直接输出 HTML**，而是发起一个工具调用 `computer_use_perform`
   （一个桌面/浏览器 UI 自动化工具），`objective` 填的就是那句提示词。
2. 该调用被路由到 `computer-use-task-controller`，返回 `status=blocked`，
   原因 `request does not identify a desktop or browser target`。
3. 模型下一轮再要 UI 类工具时，运行时**直接拦截并终止整轮**：
   `[TOOL-LOOP] round 1: blocked cross-tool computer-use retry after terminal error`。
4. **用户可见的最终回复变成一段运行时诊断文案，而不是任务产物**：

```
Computer Use 已在本轮以错误终态结束；运行时已阻止模型改用其它 UI 工具继续调用。
status=blocked, error_code=recursive_call_blocked, retry_owner=user,
last_result=工具 `computer-use-task-controller` 失败：request does not identify a desktop or browser target
```

5. 原始请求（写 HTML）**从头到尾没有得到任何文本回答**，产物为空。

### 1.2 变体 B：工具确实关闭时（`enable_llm_tools = false` + `dev_open_permissions = false`）

日志确认 `[LLM-TOOLS] definitions disabled dev_open=false config_enable=false`、
`build_request ... tools_enabled=false`，即模型没有拿到任何工具 schema。此时：

1. 模型**仍然把工具调用当成文本吐出来**：

```
<tool_call>{"name":"write_file","arguments":{"file_path":"C:/Users/zhupu/Desktop/bonsai/pelican_bicycle.html",
"file_content":"<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\" />\n<title>Pelican Riding a Bicycle</title>
...
```

2. 这段文本**没有被执行**（已确认目标文件不存在），也没有被识别为错误，
   而是**原样成为助手回复**，在 697 字符处截断。
3. 即"写文件"这个意图被静默丢弃：用户既拿不到文件，也拿不到解释。

### 1.3 变体 C：即使关掉工具，提示词不加约束也会失败

在 tools off 的条件下，把提示词换成不加约束的版本，模型会把**整个输出预算花在设计说明上**
（实测 13138 字符全是 `Rear wheel: center (230, 425), radius 85 ...` 这类几何规划），
写完就触顶 `max_tokens`，永远写不到代码。加上"直接输出 HTML、不要规划/解释/工具调用"后才产出 SVG。

> 变体 C 属于**提示词工程**而非代码缺陷，但它是复现本问题时必须固定的变量，
> 否则会把"输出预算被规划吃掉"误判成"上下文不足"。

---

## 2. 复现步骤

环境见第 4 节。三种变体的最小复现：

```bash
# 1) 起 bonsai（-ReasoningBudget 512 与 agent 侧实测调参一致，见 tmp/pelican-tuning-report.md）
powershell -ExecutionPolicy Bypass -File C:\Users\zhupu\Desktop\bonsai\service.ps1 \
  -Action start -Profile ptq-32k -ReasoningBudget 512 -ApiOnly

# 2) 改配置得到变体 A / B
#    变体A: [tool] dev_open_permissions = true   （enable_llm_tools 无效，见 5.1）
#    变体B: [tool] dev_open_permissions = false + [model] enable_llm_tools = false

# 3) 经 agent 发同一句提示词（session 为指向 127.0.0.1:8080 的本地会话）
curl -sS -X POST -H "Content-Type: application/json" \
  --data '{"session_id":"<本地会话id>","target_agent_ids":["<本地会话id>"],
           "text":"Write a single self-contained HTML file containing an inline SVG of a pelican riding a bicycle. Output only the HTML."}' \
  http://127.0.0.1:8765/api/chat/send
```

现成脚本：`C:\Users\zhupu\Desktop\coolzhuagent\tmp\bench_pelican.py`
（`--tools on|off`、`--prompt-style plain|notools|terse`、`--ctx`、`--maxout`、`--reason`），
结果落在 `tmp/pelican-matrix.jsonl`，产物 HTML 落在 `tmp/pelican-*.html`。

---

## 3. 期望

- 一个"把 HTML 写到回答里 / 写进文件"的请求，**不应该**让 27B 去调桌面 UI 自动化工具。
- 即便模型选错工具，**错误的工具调用不应该把整轮任务变成一段错误文案**——
  至少应回落到"以文本形式给出答案"，或明确告知用户"工具不可用，以下是文本结果"。
- 若模型在 tools 关闭时仍输出 `<tool_call>` 文本，运行时应当**明确处理**（执行 / 拒绝并提示 / 剥离后重试），
  而不是把它当作正常助手回复返回。

---

## 4. 环境

- 版本：本地构建自 `coolzhuagent` main（MSI `CoolzhuAgent-0.2.0.msi`，commit `0a3802ba`）
- 工作区：`C:\Users\zhupu\coolzhuagent`（`%USERPROFILE%\coolzhuagent`，生产工作区）
- 本地模型：bonsai（`llama.cpp` 定制构建，ternary PTQ1_0），`ptq-32k` profile，端口 8080
- 硬件：RTX 3070 Ti Laptop 8GB + 16GB RAM，Windows 11 中文
- 相关配置（`coolzhu.toml`）：

```toml
[model]
enable_llm_tools = true
llm_tool_exposure = "all"

[tool]
dev_open_permissions = true

[computer_use]
enabled = true
tool_mode = "task-controller"

[computer_use.controller]
max_calls_per_turn = 2
```

---

## 5. 初步分析

### 5.1 `dev_open_permissions` 让 `enable_llm_tools` 静默失效（已确认）

`modules/gui-web/packages/web-console/src/main.rs:25671`：

```rust
fn llm_tools_enabled() -> bool {
    if dev_open_tool_permissions_enabled() {
        return true;                     // ← 提前返回，[model] enable_llm_tools 被完全忽略
    }
    if read_config(|c| c.model.enable_llm_tools) {
        return true;
    }
    false
}
```

同一处还有 `main.rs:31778` 的 `llm_tool_exposure_mode()`：dev_open 时**强制返回 `"all"`**，
`llm_tool_exposure` 配置值同样被忽略。它的设计意图写在注释里——
"开发开放模式下强制 all，确保模型不会把写文件/执行命令误判成不可用"——
但这个"强制"把开关语义变成了**单向失效**：用户显式设成 false 也不生效。

**实测证据**：把 `enable_llm_tools` 改 false 后，日志仍是
`[TOOL-REG] exposed 21 tools (mode=all, dev_open=true)`、`tools_enabled=true`，
模型照旧调 `computer_use_perform`。只有**同时**把 `dev_open_permissions` 也改 false，工具才真的消失。

> 这一条本身是"配置语义缺陷"，不是架构问题，但它是本问题的**第一触发条件**。

### 5.2 `computer_use_perform` 在任何 exposure 模式下都会暴露（已确认）

`main.rs:31995` `llm_tool_definitions_for_permission()` 的逻辑顺序是：

```rust
let mut defs = vec![semantic_dispatch_tool_definition()];
if read_config(|config| config.computer_use.enabled) {
    defs.push(computer_use_tool_definition());     // ← 无条件加入
}
if mode == "dispatch-only" {
    return Some(defs);                             // dispatch-only 也含 computer_use
}
...
if mode == "whitelist" && !allowlist.contains(spec.name) { continue; }  // 白名单只过滤后面的
```

即：只要 `[computer_use] enabled = true`，`computer_use_perform` 就会出现在
`all` / `dispatch-only` / `whitelist` **全部三种**模式里。
**结论：当前配置体系下，无法做到"给云端模型保留工具、对本地模型不暴露工具"。**

### 5.3 弱模型选错工具（已观察，机理待查）

27B 面对"写文件"选择了 UI 自动化工具而不是 `write_file`。可复现、稳定。
可能原因（**均未验证**）：工具 schema 的描述对 27B 不够可区分；本地模型的 chat template
（bonsai 未启用 `--jinja`，见 6.4）没有正确渲染工具定义；或模型本身的工具选择训练偏向。

### 5.4 一次失败就足以把整轮"烧掉"（已确认）

`modules/computer-use/packages/computer-use-core/src/supervisor.rs:101`：

```rust
pub fn before_new_run(&self) -> BeforeRunDecision {
    if self.circuit_reason.is_some()
        || self.calls_started >= self.budgets.max_calls_per_turn
        || self.failure_count >= self.budgets.max_calls_per_turn
    {
        return BeforeRunDecision::Blocked("recursive_call_blocked".to_string());
    }
    BeforeRunDecision::Proceed
}
```

`max_calls_per_turn = 2`，且 `supervisor.rs:315` 的单测 `second_failed_run_opens_turn_circuit`
证明**累计两次失败即打开本轮熔断**。
熔断一旦打开，`main.rs:26117` 的循环会拦截模型后续的 UI 类工具请求并 `break`：
（流式链路有同样的分支，见 `main.rs:17318`）

```rust
if let Some(last_summary) = computer_use_terminal_failure.as_deref() {
    if tool_requests.iter().any(|(_, name, _)| is_computer_use_tool_family(name)) {
        terminal_supervisor_answer = Some(computer_use_recursive_call_blocked_feedback(last_summary));
        diag!("[TOOL-LOOP] round {round}: blocked cross-tool computer-use retry after terminal error");
        break;                                     // ← 整轮终止
    }
}
```

而被判定为"同一族"的既有 `computer_use_perform`，也有**通用的** `tools_semantic_dispatch`
（`main.rs:32995` `is_computer_use_tool_family()`）——也就是说这条熔断会连带拦掉语义调度出口，
把模型的"最后一次求助"也堵死。

### 5.5 终止路径把"诊断文案"当成了"答案"（已确认）

`main.rs:33005`：

```rust
fn computer_use_recursive_call_blocked_feedback(last_summary: &str) -> String {
    format!("Computer Use 已在本轮以错误终态结束；运行时已阻止模型改用其它 UI 工具继续调用。
             status=blocked, error_code=recursive_call_blocked, retry_owner=user,
             last_result={}", compact_message_snippet(last_summary, 320))
}
```

这段字符串直接作为助手回复返回（实测它就是 `answer`）。**原任务被丢弃，没有任何回落路径**。

### 5.6 文本里的 `<tool_call>` 不被执行也不被拒绝（已确认现象，机理待查）

`main.rs:25882` 的取工具调用逻辑只看**结构化的** block：

```rust
fn model_tool_requests_from_blocks(blocks: &[OutputContentBlock]) -> Vec<(String, String, JsonValue)> {
    blocks.iter().filter_map(|block| match block {
        OutputContentBlock::ToolUse { id, name, input } => Some((id.clone(), name.clone(), input.clone())),
        _ => None,
    }).collect()
}
```

而 `OutputContentBlock::ToolUse` 由 `modules/llm-adapter/packages/llm-adapter/src/providers/openai_compat.rs:732`
**从 API 响应的结构化 `tool_calls` 映射而来**，适配器里**找不到**任何解析文本 `<tool_call>` 标记的代码。
因此变体 B 中那段文本：既没变成 `ToolUse`（不执行），也没被识别（不报错），
直接落进了助手回复正文 → 已确认目标文件确实没有被创建。

---

## 6. 涉及架构改动的情况

按"改动半径"从小到大：

### 6.1 配置语义（小改，但需决策）

`llm_tools_enabled()` / `llm_tool_exposure_mode()` 里的 `dev_open` 提前返回应改为
"dev_open 只提供**默认值**，显式配置优先"，否则 `enable_llm_tools = false` 这种
明确意图无法表达。需要确认 dev_open 的原始设计意图是否包含"不可关闭"。

### 6.2 按 provider / 会话裁剪工具集（**需要一次小重构**）

要做"云端模型有工具、本地模型无工具"，必须把工具集计算从"全局 + 房间权限"改为带上**模型身份**。
现状的签名只吃权限/房间：

```rust
fn llm_tool_definitions() -> Option<Vec<ToolDefinition>>
fn llm_tool_definitions_for_room(chat_room_id: Option<&str>) -> Option<Vec<ToolDefinition>>
fn llm_tool_definitions_for_permission(granted_permission: PermissionMode) -> Option<Vec<ToolDefinition>>
```

需要至少再传入 `provider_kind`（`Custom`/本地端点可由
`is_local_model_endpoint_on_port()` 判定，现成可用）或 `session_id`，
并在 `main.rs:32007` 那段把 `computer_use_tool_definition()` 的加入条件改成可选。
调用点集中在 `main.rs:31659`（`llm_tool_exposure_mode()` 附近）与 `main.rs:32007`，数量不大。

> 这属于**架构选择**而非纯 bug 修复：要不要给弱模型更小的工具集（甚至"仅语义调度 + 只读"），
> 是产品决策。建议后续模型先确认设计意图，再动代码。

### 6.3 失败终态应有回落路径（架构决策）

当前"UI 工具族熔断 → 整轮终止 → 诊断文案即答案"的设计，
对**本地/弱模型**几乎等于"一次误选工具 = 本轮零产出"。
可选方向（需产品决策）：
- 熔断后追加一次**无工具**的生成轮，让模型用文本回答原问题；
- 或把诊断文案与"未完成的原始请求"一起返回，明确提示可重试；
- 至少不要吞掉原请求。

### 6.4 本地服务启动参数不一致（次要线索，**已部分被反向证据削弱**）

`run.ps1:53-61`（bonsai 自带的启动脚本）的 llama-server 参数里**没有 `--jinja`**；
而 agent 自己拉起本地模型时用的参数（`main.rs:3268` `local_gemma_runtime_args`）**包含 `--jinja`**：

```rust
vec![ "--host", ..., "--port", ..., "-ngl", ..., "-c", ..., "-np", ...,
      "--reasoning-budget", ..., "--jinja" ]
```

**反向证据**：变体 A 确实产生了**结构化**的 `computer_use_perform` 调用
（agent 拿到了完整的 `objective` 参数并派发），说明"工具调用解析本身"在没有 `--jinja` 时是工作的。
因此本项**不太可能是根因**，降级为"可选 A/B"：
加 `--jinja` 可能改变工具描述/chat template 的渲染方式，从而影响**模型的工具选择倾向**（对应 5.3），
值得一试但不是首要路径。

### 6.5 文本 `<tool_call>` 的处理策略（架构决策）

是否要在 openai_compat 适配器（或 web-console 的 content 解析）里支持
"模型把工具调用写进正文"的兜底解析？需要权衡：
- 支持 → 弱模型/非 jinja 服务也能落地产物，但会引入一套易误触发的文本协议解析；
- 不支持 → 应至少在检测到 `<tool_call>` 文本时**给出明确错误或剥离重试**，不能静默当答案。

---

## 7. 影响面

- 影响**所有把本地模型用于"落地产物类"任务的场景**（写文件、改代码、生成资源），
  在 `dev_open_permissions = true` 的交付默认配置下**必然触发**。
- 纯问答/聊天不受影响（模型不会选工具）。
- 云端模型不受 5.3 影响，但 5.1/5.2/5.4/5.5 是**全局链路**，改动需回归云端用例。
- 与模型能力相关的调参结论（已单独记录在 `tmp/pelican-tuning-report.md`）：
  关掉工具后，16K/32K/48K 三种上下文都能在 600s 上限内产出 SVG
  （16K 212s / 32K 286s / 48K 530s），说明**瓶颈不在上下文大小**，而在本条链路上。

---

## 8. 给后续模型的建议顺序

1. **先修 6.1 的配置语义**（dev_open 不应无条件覆盖显式配置）——一行级改动，
   能让用户立刻用 `enable_llm_tools = false` 自救，且风险可控。
2. 再决策 6.2/6.3/6.5 三个架构问题（都是产品决策，建议先与需求方确认再动手）。
3. 6.4（`--jinja`）作为可选 A/B，用于解释 5.3 的"工具选择倾向"，不是首要路径。
4. 注意：6.2 的改动虽小，但调用点分散，且这条链路被 `main.rs` 里大量测试
   （如 `81734`、`81018` 附近的断言测试）间接锁定，改签名要同步更新测试。
5. 复现/回归请固定住变体 C 的提示词差异（"terse" vs 默认），
   否则容易把"输出预算被规划吃满"误判为工具链路的回归。

## 9. 附：原始日志证据摘录

```
[LLM-TOOLS] build_request agent=session-1789804322149 provider=Custom
  model=C:\Users\zhupu\Desktop\bonsai\models\Ternary-Bonsai-2-27B-PTQ1_0
  stream=false max_tokens=6144 tools_enabled=true dev_open=true exposure=all tool_count=1 ...

[TOOL-LOOP] round 0: 1 tool calls, dispatching (ToolResult blocks)...
[TOOL-LOOP] n=1, parallel=true, max_concurrency=4, ids=[YaRx...:computer_use_perform]
[TOOL-LOOP] dispatching tool_call: computer_use_perform, id=YaRx...
  input={"objective":"Write a self-contained HTML file containing a detailed inline SVG of a pelican riding a bicycle.",
         "success_criteria":["HTML file is saved to disk...
[TOOL-CHAIN] run_model_tool_dispatch: name=computer_use_perform, ...
[TOOL-LOOP] tool_call OK: computer_use_perform, route=computer-use-task-controller, status=blocked
[TOOL-CHAIN] dispatch detail: route=computer-use-task-controller, status=blocked
语义工具调度：computer-use-task-controller / blocked
[TOOL-LOOP] round 1: blocked cross-tool computer-use retry after terminal error
```

变体 B（工具关闭）时的回复正文：

```
<tool_call>{"name":"write_file","arguments":{"file_path":"C:/Users/zhupu/Desktop/bonsai/pelican_bicycle.html",
"file_content":"<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\" />\n<title>Pelican Riding a Bicycle</title>...
```

（该文件经确认**未创建**。）
