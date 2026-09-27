# Qwen3.8-Flash：Paint Computer Use 真实测试未完成

日期：2026-09-19。状态：已复现并定位，本文只记录问题和后续验收要求，未修改产品代码。测试由主任务在隔离运行目录执行；本报告作者只读取证据、源码及隔离数据库的 `computer_use_runs` / `computer_use_steps` 两张表，没有读取会话密钥或操作桌面。

## 结果

**正式工具暴露及调用已经发生；Paint 动作与手绘均未完成。** 两轮均在内部 planner 的动作 JSON 解析阶段以 `invalid_plan` 终止，输入动作数与完成步骤数均为 0。两次不同任务复现同一个错误，没有进行第三次盲重试；它们不是 20 秒 planner 超时，不应把降低思考层级当作已证实的修复。

| 轮次 | 实际请求 | 整轮耗时 | 正式 CU 终态 | UIA 观察证据 | 输入动作 / 步骤 |
| --- | --- | ---: | --- | --- | --- |
| R1 | 在已打开的画图窗口选铅笔/画笔和黄色 | 21.514 秒 | blocked / planning / invalid_plan | 同一 HWND，两次快照，各 170 个元素 | 0 / 0 |
| R2 | 在空白画布手绘粗略海绵宝宝，禁止脚本/生成/导入/粘贴图片 | 43.444 秒 | blocked / planning / invalid_plan | 同一 HWND，两次快照，各 175 个元素 | 0 / 0 |

两轮准确错误均为：

```text
invalid type: string "click", expected struct PlannerAction at line 1 column 17
```

`goal_achieved=false`、`attempts=0`、`steps_completed=0`、`supervisor.action_count=0`、`replan_count=0`；`retryable=false`、`retry_owner=model`。数据库中两条 run 均为 `blocked`，各自步骤查询均为 0 行。主任务已查看 R2 后截图，画布仍为空白，未产生手绘成果。

这里的“0 动作”指 controller 没有执行输入步骤；观察路径本身会尝试聚焦目标窗口，不能据此声称桌面绝无窗口聚焦变化。Paint 空白窗口由宿主预先准备，不能计为模型成功启动了 Paint。

外层 `chat_turn` 两轮均为 `completed`，含义是模型收到工具错误后完成了说明。它不代表 Computer Use 成功。SSE 中工具包装层为 `status: failed`，原生 CU 结构为 `status: blocked`，三者层级不同。模型正文承认失败和未绘画，未改用脚本替代。

## 复现环境与证据

- 本轮执行文件：`tmp/2026-09-19-qwen/staging/bin/coolzhu-web-console.exe`，主任务打包版本 0.2.13。
- 启动记录：`docs/testing/release-0.2.13/evidence/launch-staging.json`，PID 3340，端口 18795；SHA-256 `9CC3F8DDE92C440F2C9C631EABB9FDD6EB3651C1D82D8D04C53627537FF47BCC`。`evidence/launch-debug.json` 是更早 debug 阶段的记录，不应拿它说明本次 Paint 二进制。
- 目标窗口：`无标题 - 画图`，宿主夹具记录 PID 25508；运行证据窗口 `hwnd-13f0682`。
- 模型：qwen3.8-flash，沿用主任务已验证的 medium 配置；正式允许工具仅 `computer_use_perform`。
- 聊天室：`room-1789828631771`；会话：`session-1779459149988`。
- R1 provider 工具 ID：`call_ff93b6818c9844e5b7cc764c`。
- R2 provider 工具 ID：`call_2a99d45df6144b5689d5aa74`。

原始证据位于 `tmp/2026-09-19-qwen/evidence/`：

- [R1 事件](../testing/release-0.2.13/evidence/paint-r1-events.json)、[R1 摘要](../testing/release-0.2.13/evidence/paint-r1-summary.json)、[R1 后截图](../testing/release-0.2.13/evidence/paint-r1-after.png)。
- [R2 事件](../testing/release-0.2.13/evidence/paint-r2-events.json)、[R2 摘要](../testing/release-0.2.13/evidence/paint-r2-summary.json)、[R2 后截图](../testing/release-0.2.13/evidence/paint-r2-after.png)。
- [数据库与事件只读交叉核验](../testing/release-0.2.13/evidence/paint-controller-readonly-audit.json)：保留两条原生终态、provider ID、步骤结果、事件 SHA；核验脚本为 `tmp/2026-09-19-qwen/audit-paint-controller.py`。
- [最终用量统计](../testing/release-0.2.13/evidence/paint-final-insights.json)、[能力报告](../testing/release-0.2.13/evidence/computer-use-capabilities.json)、[运行与请求计划](../testing/release-0.2.13/computer-use-test-plan.md)。

测试摘要中的 `tool_calls: 0` 只按某一种事件类型计数，**不能作为未调用正式工具的证据**。本轮实际有 `kind=tool-summary`、`kind=tool-result`、正式 route，以及数据库中两个真实 provider 工具 ID。后续测试器应按这些证据交叉计数，避免把动作数、工具次数、模型请求数混为一谈。

## QWEN-CU-001：动作协议没有完整传给 planner（P1）

代码：`modules/gui-web/packages/web-console/src/computer_use_planner.rs:15–23,31–57,383–418`。

解析器要求严格对象：`PlannerResponse { done: bool, summary?: string, action?: PlannerAction }`，其中 `PlannerAction` 必须含 `kind`、`target`，可含对象 `arguments`，且两层均 `deny_unknown_fields`。例如下面是**应发送给模型的契约示例，不是本轮原始返回**：

```json
{"done":false,"action":{"kind":"click","target":"uia-<当前观察中的引用>","arguments":{}}}
```

然而实际 system prompt 只有“返回一个 JSON”“选择一种允许动作”和浏览器参数提示，唯一完整 JSON 示例是 `{"done":true,"summary":"blocked: target_not_found"}`。它没有展示 `done=false` / `action` / `kind` / `target` / `arguments` 的嵌套结构，也没有给出桌面允许的动作名清单和参数要求。user prompt 仅包含 surface、step、objective、success_criteria、observation_generation 和截断的 observation，再要求“Return the next action JSON”。

因此当前模型需要猜一个代码中严格约定、提示中却没有定义的结构。两轮错误证明 parser 收到了字符串形式的 `action="click"`，在反序列化 `PlannerAction` 时失败，尚未进入 target grounding、策略检查或输入执行。**可以确认协议说明缺失和失败位置；不能根据错误字符串还原完整原始 JSON，也不能把整段模型返回凭空补成某个示例。**

附带契约问题：

- 此内部请求复用 `main.rs:32095` 的通用 `agent_message_request_build_with_system`。该 builder 在约 32130 行追加 `request_tool_policy_instruction(None)`，其内容要求“直接用最终答案完成原始任务；若需外部操作，说明尚未执行并提供文本”。它没有定义动作结构，且与内部 planner 只产出下一动作的职责存在潜在冲突。未证明它是此次字符串动作的唯一诱因。
- `request.constraints` 和 `request.target` 没有作为独立字段进入 planner 的 user prompt；当前 adapter capabilities 也未注入。上层约束若未重复写进 objective，就可能在内部规划层丢失。宿主底层仍有动作白名单，不能因此宣称所有限制均失效。
- observation 通过 `.chars().take(8192)` 截断序列化 JSON，可能截断对象或移除后面的有效控件；170/175 个元素被枚举不等于每个元素都完整送到模型。本轮没有保存实际 planner prompt，无法证明黄色/画笔引用是否在截断范围内。
- planner 固定 1024 输出 token、20 秒 timeout；本轮错误发生在正常返回之后的结构解析，不能据此认定为生成被截断或超时。

后续修复建议：由统一动作定义生成每个 surface 的完整 JSON schema/示例与参数白名单；分开 planner 专用请求策略和聊天答复策略；显式传入 constraints、target、当前 capabilities；按完整控件条目裁剪观察并标记被裁剪内容。保留严格解析和 grounding，不能为了让此案例通过而无条件接受任意字符串动作、坐标或脚本。

回归应覆盖：真实构造的 planner 请求确实带完整 schema；本地模拟模型返回合法 desktop click 可进入 execute；`{"action":"click"}`、未知字段、错误 surface、过期引用等仍可解释拒绝；R1 一次调用确实选中画笔和黄色并有独立状态证据。现有约 1150 行的 parser 单测自行提供正确 JSON，只能证明解析器，不能证明模型收到了对应契约。

## QWEN-CU-002：桌面缺少 Paint 笔画动作（P1 能力缺口）

代码：`computer_use_adapters.rs:354–373`、`computer_use_planner.rs:122–131`、`computer_use_desktop_bridge.rs:89–189`。

正式 desktop capabilities 明确 `drag=false`、`slider_drag=false`；parser 仅允许 click、double_click、text_input、scroll、key_combination；bridge 只向 UIA 控件中心发点击、有限按键、文本或滚动。没有自由路径笔画、画布内相对点或拖拽动作。浏览器的 DOM 拖放也不能当成 Paint 画布笔画能力。当前正式工具不会启动新的应用，只能聚焦已经存在的目标窗口。

即使先修好 QWEN-CU-001，也不能宣称已经支持手绘海绵宝宝。本轮运行在解析前失败，**运行时尚未走到笔画能力验证**；缺少 drag 是另一个由源码证实的缺口。不能直接采信 R2 模型正文“这不是拖拽能力缺口”的概括，它只说明本次最先遇到的失败是 JSON 解析。

后续如果补画布操作，应设计明确的动作范围、窗口和画布坐标系、缩放/DPI 绑定、按下→移动→释放的完整笔画及取消时释放机制；每步留真实动作证据和前后画布图。不得通过写 SVG/PNG、图像生成、导入或粘贴图片替代用户要求的手绘测试。验收需分别报告“工具调用发生、输入发生、画布改变、目标完成”。

## QWEN-CU-003：正式 planner 没有截图视觉输入，绘画结果也不做像素验证（P1 能力缺口）

代码：`computer_use_desktop_bridge.rs:32–85,194–218,286`、`computer_use_planner.rs:367–418`。

desktop snapshot 生成的是 UIA 窗口和控件 JSON；planner 用发起聊天会话构造单条 `InputMessage::user_text`，没有图像 content block，也没有调用默认视觉 Agent。设置了多模态能力或默认视觉 Agent，不会自动把截图接入这条链路。外层任务摘要“触发内视觉工具侧路”等文字不能替代 planner 请求实际含图的证据。

verify 比较前后 UIA 状态与 evidence，并在 UIA 序列化字符串里匹配成功条件。它不能判断画布像素中是否已有黄色身体、两眼和嘴。窗口或控件文字变化也不等于绘画取得进展。

后续验收应以内部 planner 实际发送的图片 content block、绑定该观察的图片摘要/尺寸、最新图像反馈为准；规划与验证要明确区分文字/UIA状态和像素语义。截图采集API单独可用，不能证明它已集成到正式 controller。

## QWEN-CU-004：planner 请求用量没有记入聊天室统计（P2）

代码：`computer_use_planner.rs:407–418`、`chat_insights.rs:19–73`。

planner 直接 `send_message` 后只取 `response.content`，忽略返回的 `response.usage`，也没有把 room/turn/call 身份交给统一用量记录入口。聊天室现有 `record_usage` 要求 room ID；当前 planner struct 仅持有 session ID。

本轮 insights 在 R1 后为 2 条用量记录，R2 后累计 4 条：输入 13366、输出 1722、缓存字段均 0。这是已保存记录，**不是整条 Computer Use 模型链的总成本**。两轮都完成了一次内部模型返回并进入 JSON 解析，因而每轮至少还有一次 planner 逻辑请求未计入；按主聊天首轮请求＋工具反馈请求＋planner请求计算，两轮至少为 6 次逻辑模型调用。不能根据现存数据估算漏掉的 token、远端计费金额或实际 HTTP 重试次数。

后续应把 planner 以及其他嵌套模型调用纳入统一 usage 事实表，关联 session、room、turn、CU call、调用类别，并在解析失败时仍记录已收到的 usage。以本地已知 usage 返回的失败计划验证“计入一次、不漏、不重复”；缺失 usage 应显示不可得而非伪造 token。

## QWEN-CU-005：没有内部原始计划诊断记录（P2）

当前内部响应在 `answer_text` 后直接解析；解析器只把 serde 错误写入 `ComputerUseError`。SSE、CU runs/steps 表及当前 planner 源码都没有保留内部原始响应、模型响应 ID、原始 prompt 或该观察完整内容。现存证据能定位 `action` 为字符串以及行列位置，不能恢复其它字段或验证当时给模型的全部控件。

CU 的 `provider_tool_call_id` 是外层模型调用正式工具的 ID，不能冒充内部 planner 的 provider response ID。`computer_use_steps` 目前是收尾时生成的摘要，`action_type=controller_action`、前后 evidence 相同、时间在收尾生成；即使未来有步骤行，也不能把它当作精确输入轨迹。

后续可添加限长、脱敏的 planner 诊断记录，关联请求/响应摘要、观察 generation、schema版本、模型、耗时、结束原因与usage；失败时保留有限原始**动作 JSON 正文**供定位，不保留密钥，不要求保存完整思考过程。每个执行步骤则记录真实 kind、target、开始/结束时间和独立前后证据。当前两次失败已经足以先修明确的契约缺口，不需要为取原文继续盲发真实模型请求。

## 2026-09-19 修复进展与验证边界

以上 R1/R2 是 0.2.13 已交付版本的真实失败记录，保留用于回归对照。本节描述后续工作区修复，不把本地模拟测试当作真实 Paint 成功。

| 编号 | 已实现行为 | 自动验证 |
| --- | --- | --- |
| CU-001 | 专用 planner builder 不追加聊天答复策略；按当前 surface 和实际 capabilities 提供完整动作 JSON schema、示例与参数白名单；显式发送 target、constraints、criteria；观察按完整条目缩减，始终生成合法 JSON | 本地 HTTP 捕获实际请求，检查 schema/约束/无工具定义；合法动作通过，字符串 action、错 surface、未知字段和过期引用继续拒绝 |
| CU-002 | 正式 desktop drag 支持 2–256 个画布内相对点；绑定最新 UIA 画布或显式 window-canvas 引用，核验窗口、DPI、generation 与图片 hash；受控原生输入确保释放检查 | 底层输入 Engine 的纯内存 driver 与桥测试覆盖顺序、越界、过期观察、取消与释放失败；未调用真实桌面 |
| CU-003 | 最新原尺寸 PNG 进入多模态 planner；纯文本会话由已配置默认视觉 Agent 真实看图后转述；执行后异步图像验收逐项判断目标，UIA/路径/时间变化不能冒充像素变化或任务完成 | HTTP mock 检查两条真实装配路径；验收检查原图、所有条件与证据；图片不变或条件不全不得完成 |
| CU-004 | planning、visual_description、verification 都记录返回的真实 usage，关联 room、turn、CU call 与类别；解析失败也记录已消耗响应 | HTTP mock 返回已知用量，检查 3 条记录及关联字段；统计迁移重入保留事实和新增关联 |
| CU-005 | 新增限长脱敏动作诊断与逐步真实轨迹；记录内部响应 ID、观察 generation、真实动作/目标/时间及前后证据；原输入正文、截图 data URL、完整思考不写入这些诊断；解析异常仅输出类别/行列 | 失败 action 字符串仍可定位；密钥、图像及文本字段被隐藏；真实输入前后轨迹有独立证据；元数据变化不直接置 visible_progress |

宿主取消通过真实聊天轮次取消对象传到 planner、下一次输入检查和长笔画 helper；controller future 被丢弃时，将未结束的持久化 run 收敛为 cancelled，不永久停留 running。单目标流式与接力模式使用 provider trace 的精确取消关联；非流式入口原本不暴露相同中止协议，没有按聊天室猜测另一轮的取消对象。仅有 Escape 测试不能替代此链路验证。

本地验证日志位于 `tmp/2026-09-19-agent-fixes/`：`cu-tests.log` **77/77**、`core-tests.log` **41/41**、`insights-tests.log` **5/5**。这些结果包含真实本地 HTTP 协议装配和 SQLite 事实检查，但没有向付费模型发送请求，也没有操作实际 Paint。主任务负责最终统一构建与真实复测。

仍需以新包实测核验：实际模型是否持续返回合法动作、真实画布笔画是否发生、视觉判断是否符合截图、长路径期间宿主取消是否停止后续输入，以及每个请求的用量和轨迹是否与最终结果对应。视觉判断仍受模型能力影响；图片变化仅是必要证据，不保证画面质量或目标完成。测试须分别报告“正式工具调用、原生输入、画布改变、逐项目标确认”，不能仅依据外层 chat_turn completed。

## 后续真实复测发现：窗口 DPI 坐标不一致

工作区修复后的首次真实 Paint 测试在 observation 阶段失败，动作数仍为 0：截图 helper 将 `stale_observation` 包在 `input_failed` 中返回。该轮没有进入 planner，不能判为动作 schema 回归失败。事件在 `tmp/2026-09-19-agent-fixes/evidence/paint-r1-events.json`，与前文已交付 0.2.13 的旧 R1 是不同测试。

只读原地探针确认 Paint PID 25508、HWND 20907650、前台 HWND 和 DPI144 全部一致，但 unaware 线程 `GetWindowRect` 返回 `[0,41,1707,768]`，PMv2 线程与 UIA 返回 `[0,61,2560,1152]`。web-console 的进程 awareness 为0；resolver 窗口矩形未经 DPI 统一，而 C# helper 切到了 PMv2 后严格比较物理矩形，导致未移动的窗口也被拒绝。证据为 `tmp/2026-09-19-agent-fixes/dpi-readonly-probe.json`；核验没有聚焦、移动、输入或截图动作。

现已在 resolver 同步 snapshot 作用域中增加线程 PMv2 RAII guard，退出时恢复原上下文，不改变进程或其它 GUI 线程，不放宽窗口身份校验。正常返回、提前失败与非法 context 三项原生线程恢复测试通过；uia-resolver 总计5/5，offline build通过。根因、焦点链排除依据与修复说明见 `tmp/2026-09-19-agent-fixes/paint-dpi-root-cause.md`。真实 Paint 再验收仍由主任务在统一构建后执行。

## 后续最小契约修复

真实 R2 的外层模型漏发必填 success_criteria，参数校验拒绝后，原看护器仍计入实际 CU 调用次数，导致随后补齐参数的请求被封锁。现将已证明尚无观察/输入的参数拒绝纳入独立纠错额度：最多两次错误提交可继续纠正，第三次仍错即终止本轮纠错；真实观察/规划/执行的次数限制保持不变。缺失字段不会由宿主猜测补齐，provider call ID 重放仍保持幂等。新增回归与原 executor 测试共16/16通过，日志为 `tmp/2026-09-19-agent-fixes/cu-input-correction-tests.log`。

R4 证明正式 fallback drag 已能发送原生路径，但模型把整个 client 上方工具栏当成白色画布起点。现明确请求契约：window-canvas/desktop.canvas_rect 表示整个可见 client，包含工具栏；模型需依据原图和两个矩形定位内部真实绘图区。没有加入 Paint 固定坐标或代绘脚本。该次选点失误与“已发生原生输入”须分别保留，不能据工具 input_sent 或图像变化直接宣称完成绘画。最终真实结果由主任务汇总。
