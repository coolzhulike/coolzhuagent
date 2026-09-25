# qwen3.8-flash 真实会话与 Computer Use 定向测试计划

日期：2026-09-19。性质：源码只读审计和待执行流程；本文件作者没有发送模型请求、操作桌面或读取密钥明文。以执行时实际二进制版本为准，若其他代理后续补齐能力，须重新核对本文列出的限制。

## 结论与当前可测范围

1. SVG 鹈鹕骑自行车 2D 动画可通过正常聊天流式 API 测试。建议先作为纯内容生成禁用所有工具，再以多轮修改验证长程上下文、输出完整性、耗时与 usage。不要将模型没有执行桌面工具视为该任务失败。
2. 正式桌面入口是 `computer_use_perform`，任务级工具由宿主负责观察、规划、输入、验证。当前桌面只支持 UIA 控件中心点击、双击、文本输入、滚动、少数组合键；**不支持拖拽/自由路径笔画/启动应用，也不接截图给桌面 planner**。直接要求 Paint 手绘海绵宝宝，可以测真实模型是否调用入口及真实动作是否发生，但目前不能预设完整绘画必定成功。
3. 当前证据应分为“调用发生”“输入发生”“画布改变”“目标完成”四级。工具终态失败也是真实验收结果。不得把预先写入 SVG/PNG、图像生成服务结果、脚本绘图、剪贴板粘贴图片或导入图片计为手绘通过。
4. `dev_open_permissions=true` 并不等于正式 ComputerUse 的房间授权。正式 executor 另要求保存的房间档位为 `full-access`。默认视觉 Agent 与正式 planner 不是同一条调用路径。

## 源码对应点与限制

| 范围 | 当前真实契约 | 代码位置 |
|---|---|---|
| 工具 schema | `objective`、`surface`（auto/desktop/browser）、`target`（application/window/url/element）、非空 `success_criteria`、`constraints`；无 coordinates/code 字段 | `main.rs:32340 computer_use_tool_definition` |
| 工具暴露 | 全局及会话 enable_llm_tools；全局及会话 computer_use_enabled；room capabilities；exposure 不能为 dispatch-only；显式 allowlist 必须含正式工具名 | `main.rs:32456–32502` |
| 正式执行 | `computer_use_executor::execute_with_current_runtime`，只认 task-controller；以发起 session 构造 planner | `computer_use_executor.rs:721` |
| 房间权限 | `room_permission_grant_view_for_path` 只认 SQLite 房间 `full-access`；dev_open 不在此函数中替代已保存授权 | `main.rs:2319`、`computer_use_executor.rs:760` |
| 桌面观察 | UIA 前台窗口/控件，记录 HWND/PID/窗口矩形/DPI；尝试聚焦已经存在的目标窗口 | `computer_use_desktop_bridge.rs:32` |
| 桌面输入 | 目标 UIA 元素须 enabled 且非 offscreen；click/double_click；text_input 仅 Edit/Document 且 <=4000 字节；scroll 1–5；key_combination 仅小型白名单 | `computer_use_desktop_bridge.rs:89–186` |
| 按键限制 | ctrl/shift/alt/enter/escape/tab/home/end/a/c/l/v/x/y/z；没有 Windows 键、R、S、F4 等，不能依赖 Win+R 启动 Paint 或 Ctrl+S 保存 | `computer_use_desktop_bridge.rs:346` |
| 拖拽限制 | Desktop adapter drag=false，planner desktop allowlist 无 drag；bridge无实现。浏览器 drag 是 DOM 源元素→drop_target，不等于任意画布笔画 | `computer_use_adapters.rs:359`、`computer_use_planner.rs:106` |
| planner | 使用发起聊天会话的模型；观察 JSON 截至 8192 字符；最多1024输出 tokens；20秒调用超时；禁止坐标、JS、shell，只允许最新 UIA/DOM 引用 | `computer_use_planner.rs:12–25,353–423` |
| 桌面验证 | 比较前后 UIA 状态是否变化，以后态字符串匹配 success_criteria；没有像素级绘画识别 | `computer_use_desktop_bridge.rs:194–218` |
| 防错窗口 | 动作前重查 HWND/PID/DPI/矩形；变化则拒绝；WebView2覆盖会触发冲突限制 | `computer_use_adapters.rs:377–437` |
| 终态 | failed/blocked/timed_out/cancelled 后该轮不得换旧工具名重试；结果 route=`computer-use-task-controller` | `main.rs:26719`、`33449` |
| 步骤落库 | `computer_use_runs`存真实终态；`computer_use_steps`当前仅事后摘要，action_type固定controller_action，before/after相同，并非精确动作轨迹 | `computer_use_store.rs:9`、`computer_use_executor.rs:490` |

API能力报告 `GET /api/computer-use/capabilities` 是预检，不代表绘画可用。desktop_available 主要基于平台能力，不能代替 Paint/当前前台/输入后端实际检查。

## 隔离环境

由主任务执行者新建 `tmp/2026-09-19-qwen/runtime-<timestamp>`；单独端口例如18795，启动前必须拒绝已占用端口，不能关闭已有实例。不要复用正式8765、既有18775/18776或其他验收端口。此处只是建议端口，执行时以空闲检查为准。

将工作目录、配置与以下变量均指向新隔离目录：

```powershell
$env:COOLZHU_RUNTIME_DIR = $runtime
$env:COOLZHU_WEB_SESSION_STORE = Join-Path $runtime '.coolzhu/web-sessions.json'
$env:COOLZHU_WEB_SESSION_DB = Join-Path $runtime '.coolzhu/web-sessions.sqlite3'
$env:COOLZHU_WEB_ATTACHMENT_STORE = Join-Path $runtime '.coolzhu/attachments'
$env:COOLZHU_VISION_DATA_DIR = Join-Path $runtime '.coolzhu/vision'
$env:COOLZHU_DESKTOP_CAPTURE_DIR = Join-Path $runtime 'evidence/captures'
$env:COOLZHU_WEB_STATIC_ROOT = 'C:\Program Files\CoolzhuAgent\modules\gui-web\packages\web-console'
```

**截图目录必须额外隔离**：vision 默认截图路径独立于会话 runtime；未设置时可能写入用户 HOME 下 `.coolzhu/vision/desktop-capture/desktop-latest.png`。每次拿到 latest/image 后立刻另存带顺序号的证据，latest会被覆盖。

最小隔离配置（真实endpoint、凭据引用由授权主任务在内存复制/配置，日志不得打印明文；不得为了测试扩大正式会话权限）：

```toml
[web]
bind_addr = "127.0.0.1:18795"
[model]
enable_real_llm = true
enable_llm_tools = true
llm_tool_exposure = "all"
[tool]
dev_open_permissions = true
[tool.execution]
max_feedback_rounds = 8
[computer_use]
enabled = true
tool_mode = "task-controller"
[computer_use.controller]
max_actions = 12
max_replans = 2
max_same_signature = 2
max_no_progress_steps = 2
timeout_seconds = 120
max_calls_per_turn = 1
[computer_use.desktop]
enabled = true
[pet]
enabled = false
```

`max_feedback_rounds` 的确切层级是 `[tool.execution]`（`ConfigTool.execution`）；当前默认是40，测试建议显式缩小，不能让失败持续消耗请求。ComputerUse配置确切硬上限：max_actions50、replans5、same_signature3、no_progress3、timeout300秒、calls_per_turn2。

记录启动EXE路径/SHA、PID/创建时间、绑定端口和配置摘要。结束只处理核对身份的测试进程；桌面本身无法通过端口实现隔离，应使用明确新建的 Paint 窗口和独立桌面时间段，避免并发桌面操作者。

## API 准备与默认视觉 Agent

所有请求发往隔离 `$BASE`。先 `GET /api/workspace` 比对路径，`GET /api/sessions`只保留会话id/name/provider/model/model_type等允许字段，不写出原始含凭据响应。精确验证模型字段为用户要求的 `qwen3.8-flash`，不要静默替换相似名称。如果服务端不接受该模型，记录原始模型名与脱敏错误，由主任务处理。

```http
POST /api/sessions
Content-Type: application/json

{"name":"Qwen真实长程SVG验收","provider":"<原会话provider>","model":"qwen3.8-flash","model_type":"<实际能力类型>","base_url":"<原会话endpoint根路径>","api_key_ref":"<授权引用，日志脱敏>"}

POST /api/chat/rooms
{"name":"Qwen-SVG-长程隔离验收"}

POST /api/sessions/<session_id>/activate
{}

POST /api/chat/rooms/<room_id>/activate
{}
```

为SVG与Paint创建不同session/room，避免工具任务历史污染纯内容生成，也避免默认视觉角色排除会话的问题。

视觉配置入口：`GET /api/config/vision-agent`，`POST /api/config/vision-agent {"agent_id":"<vision-session-id>"}`。选择保存于 session store 的 `active_vision_session_id`（SQLite metadata及JSON字段），不是 provider 配置表。只接受 `vision/multimodal/video` 类型。清空为 `{"agent_id":""}`。

正式 ComputerUse planner 使用发起会话，**不会因为设置默认视觉Agent自动收到图像**。`POST /api/vision/realtime/understand {"question":"只描述当前Paint画布中可见线条和颜色，不操作桌面","max_elements":12}`才会调用被选中的视觉会话并将新截图作为图像输入。该接口本身会发生真实模型调用，应单独计费/记录。总览可回退展示第一个视觉会话，但该接口没有 active_vision_session_id 时会400；不能以总览名称代替GET配置核实。

此外，默认视觉 Agent 被排除在部分群发/目标候选路径外。不要随意把同一 qwen 测试会话选成默认视觉角色后又要求其作为普通聊天投递目标；用独立视觉会话。

## 测试A：长程SVG动画（正常聊天，6轮上限）

设置实际可用 context_window/max_output_tokens，建议已知支持时32768/8192；不要宣称此值适用于所有qwen部署。设置纯内容生成禁止工具：

```http
POST /api/sessions/<svg_session>/model-settings
{"parameters":{"enable_llm_tools":false,"computer_use_enabled":false,"tool_allowlist":[],"context_window":32768,"max_output_tokens":8192}}

PATCH /api/chat/rooms/<svg_room>/diagnostics
{"real_llm_enabled":true,"llm_tools_enabled":false,"computer_use_enabled":false}

POST /api/chat/send/stream
{"session_id":"<svg_session>","target_agent_ids":["<svg_session>"],"chat_room_id":"<svg_room>","text":"请直接输出一个完整可独立运行的HTML文件，内含SVG实现鹈鹕骑自行车的2D循环动画。要求：鹈鹕长喙和喉囊可辨认；双轮与辐条转动；脚踏板与腿部周期同步；身体有轻微起伏；有地面与移动背景；无外部网络资源。不要调用任何工具、不要输出工具调用协议。完整代码使用一个html代码块，确保闭合；末尾在HTML注释中写 TEST_PELICAN_QWEN_R1。"}
```

后续轮次同一session/room串行发送，前一轮必须终态：R2修正脚踏与轮子旋转关系；R3添加播放/暂停与速度控件；R4缩放适配低高度窗口和减少动态选项；R5回顾前四轮需求并补漏；R6最终给出完整代码并保留所有功能。每轮指令要求本轮标记，避免将早期成品当最终响应。总预算6轮，每轮HTTP上限180秒，总20分钟；每轮只可因明确传输失败重试一次且先确认旧run终态，禁止盲目重发。

测试脚本仅将模型响应中的HTML原样写入隔离证据目录供浏览器查看，不替模型补代码。保存原始SSE、按时间事件、正文原文、提取代码、SHA及差异。浏览器可在禁外网的测试页中静态/视觉验收：DOM中有SVG；无外部URL加载；两时刻截图不同；暂停后动画静止；速度控件改变周期；720/600高度可用；代码无截断。长程最低验收是“连续6轮正确引用前文并交付完整可运行结果”，不是累计字数。

任何 `<tool_call>`裸协议、假称读写文件、无工具却生成 tool事件、回复被think残留吞掉均记录为失败。一次恢复成功也要单独记为“触发恢复后成功”，不能隐藏首轮退轨。

## 测试B：Paint Computer Use（先能力动作，再绘画尝试）

### 授权及工具范围

新建Paint专用session/room。UI实际动作前必须显式保存房间full-access：

```http
PATCH /api/chat/rooms/<paint_room>/permissions
{"permission_profile":"full-access","risk_acknowledged":true,"confirmed_twice":true}

PATCH /api/chat/rooms/<paint_room>/diagnostics
{"real_llm_enabled":true,"llm_tools_enabled":true,"computer_use_enabled":true}

POST /api/sessions/<paint_session>/model-settings
{"parameters":{"enable_llm_tools":true,"llm_tool_exposure":"all","computer_use_enabled":true,"tool_allowlist":["computer_use_perform"]}}
```

`GET`上述配置与 `/permissions`验证**保存档位**为full-access，并保留capabilities报告。只暴露正式 ComputerUse，防模型改用bash/write_file/imagegen生成图片绕过目标。

### Paint启动与窗口范围

当前产品没有专门LaunchApplication API，正式工具也没有Windows键。执行者可将“启动一个新的空白Paint窗口”明确作为**夹具准备动作**，记录新的进程与窗口，不计入“模型启动应用成功”。若用户要求模型连启动也完成，当前版本应据实报该步骤不可完成，不能暗中代做再宣称全链路通过。

启动后校验实际窗口名/PID，设置不含用户文档的新画布。不要关闭已有Paint；不要利用既有未保存图像。target.window 填写当前可见新窗口标题。虽然字段名为 application，当前 `focus_window_by_hint` 实际只按窗口标题/class匹配，不检查可执行文件名；只填 `mspaint` 可能匹配不到。可同时记录 application=mspaint，但必须提供真实window标题并核验前台PID。产品只聚焦已有窗口，不会启动它。

前后证据由 `POST /api/capture`（无body）→保存返回metadata→`GET /api/capture/latest/image`获取PNG。Capture只截图，不发模型请求。不要把截图结果当已经进入planner上下文。

### 第1轮：小型真实动作验收（最多1次工具调用）

```http
POST /api/chat/send/stream
{"session_id":"<paint_session>","target_agent_ids":["<paint_session>"],"chat_room_id":"<paint_room>","text":"请调用 computer_use_perform，surface=desktop。目标是刚创建的空白Paint窗口，application=mspaint，window=<实测新窗口标题>。先观察UIA控件，选择铅笔或画笔工具并选择黄色；只操作此窗口。success_criteria: Paint中铅笔或画笔处于选中状态，黄色处于当前颜色。constraints: 只使用正式computer_use_perform，不运行脚本/命令，不生成或导入图片，不使用剪贴板粘贴图片；失败后报告准确终态和原因，不换工具重试。"}
```

此轮可判定模型是否真实给出正式tool_use，宿主是否出现 sendinput证据。若文字标准在UIA中不可见、目标定位失败或planner超时，完整保留失败；不要放宽成“Paint窗口存在即成功”。模型省略调用时宿主可能由显式工具意图生成forced请求，必须根据日志 `formal computer-use ... forcing`区分“模型主动调用”和“宿主兜底”，两者不能混报。

### 第2轮：手绘尝试（最多1次工具调用）

提示用户目标“通过画笔/形状控件画一个海绵宝宝：黄色方形身体、两只眼睛、嘴、棕色短裤和四肢，不要求画得像”。约束：仅当前Paint空白画布，通过鼠标/键盘作画；严禁脚本、绘图代码、图片生成/导入/粘贴；无法拖动必须报告能力限制。

现版本预期可能 blocked/failed/unsupported_action/target_not_found/no_progress。必须保留此真实结果；任务动作测试可以完成为“尝试已执行并暴露产品缺口”，不能把最终文字声明当绘画完成。若后续修复加入笔画：每条笔画须绑定新鲜截图+目标窗口，按下→移动路径→释放、动作后截图，验证画布像素改变；接口具备这些证据后才改为成功验收。

建议本轮HTTP360秒、控制器120秒（最多可设300秒）、最多12动作；整个Paint测试最多3轮，总15分钟。不要为了同一失败在新消息里反复绕过单轮终态保护。若模型异常改变窗口/启动外部操作，调用run interrupt并停止本次实例，保持已有用户程序不动。

## 运行证据、停止与审计SQL

保存SSE每帧的event/data/收到时间，至少保留start内run_id/turn_id、reasoning、tool-call、tool-result、message_done以及error/done。对于ToolResult，解析内部JSON保留`status/surface/stage/error.code/retry_owner/goal_achieved/steps_completed/evidence`；成功须有`route=computer-use-task-controller`且真实输入证据，而非仅assistant回复。

```http
GET /api/runs/<run_id>
GET /api/runs/<run_id>/events?after=0
GET /api/chat/rooms/<room_id>/insights
POST /api/runs/<run_id>/interrupt
{"reason":"隔离验收达到轮次/时间预算"}
```

events端点是SSE，设置明确读取上限并在终态收尾。不要把断开前端HTTP当作已经中止底层输入；状态终态与桌面静止需要另外核实。

只读打开隔离SQLite（URI mode=ro 或现有只读库），不要复制正在写入的db而漏掉WAL：

```sql
SELECT call_id, provider_tool_call_id, turn_id, session_id, chat_room_id,
       surface, state, action_count, replan_count, no_progress_count,
       terminal_result_json, created_at_ms, updated_at_ms
FROM computer_use_runs ORDER BY created_at_ms;
SELECT run_id, step_index, action_type, normalized_target, status, error_code,
       before_evidence_ref, after_evidence_ref, visible_progress,
       started_at_ms, completed_at_ms
FROM computer_use_steps ORDER BY run_id, step_index;
```

步骤表当前只是摘要，`controller_action`不得当原始动作名。真实`terminal_result.evidence`中的`uia_snapshot:...`和`sendinput:Click/...`、前后PNG、执行器日志一起证明动作。当前planner请求usage未在此审计中确认进入chat_insights总数，不能仅凭UI统计推断全部规划请求成本；需与实际提供方usage/代理日志核对且脱敏。

## 附件边界与给 tool_runtime 的建议

已将下列风险发送tool_runtime；主任务正在修改，验收应以修改后版本复核：

1. 当前图片编码路径仅接受kind=image，但用`url.rsplit('/').next()`直接join附件目录，未复用严格本地URL解析。Windows反斜杠/绝对路径、非受控URL需拒绝；防越界读取，并考虑符号链接/重解析点。不得用真实密钥文件做攻击测试，用隔离哨兵文本即可。
2. 上传默认32MiB加multipart开销，但后续读取没有独立图片限额，伪造附件引用可绕过上传路径。单图字节/像素/图数/合计预算应限制，损坏图片、MIME与内容不符应有明确失败反馈。
3. 未找到图片当前静默跳过，可能让用户误以为模型已经看图。响应需显示实际接入数量和失败原因。图片预算不足时已有`image.omitted_budget`诊断，但不能只写日志且给用户“看过”印象。
4. 普通HTML/SVG/text附件当前不会自动作为文本进入模型。要测试上传后修改SVG，应实现受限UTF-8文本提取、字符/字节上限、截断标记、文件边界，并把附件当用户数据，不能提权成system指令。若暂未实现，第一阶段直接通过正文提供SVG，不伪称附件已被模型读取。
5. SVG按image/svg+xml转dataURI并不代表上游模型支持；建议以文本处理或明确转换策略，不能未经说明把SVG当PNG。远程URL附件不应主动抓取任意URL，避免网络/凭据泄漏和测试不可复现。
6. OpenAI兼容与Anthropic原生各自协议必须检查真实请求体中是否出现有效图像内容；不要只测内部InputContentBlock类型或UI预览。本计划后续将独立审查Anthropic原生图片协议，附件本地处理由tool_runtime负责。

## 输出与判定

建议最终证据目录含：`launch-redacted.json`、`config-redacted.json`、`capabilities.json`、`session-settings-redacted.json`、每轮SSE与run状态、model-request计数（无鉴权头）、SVG原文/最终HTML/预览截图、Paint前后PNG、computer_use_runs/steps JSON、insights快照、总结。

报告分别给出：真实模型身份与请求成功；SVG六轮结果；ComputerUse模型调用或宿主兜底；实际输入数量与证据；绘画完成/失败；附件真实进入模型的类型和数量；尚未覆盖项。质量不作高门槛，但证据真实性不可降低。
