# Computer Use 独立复核：coolzhuagent、ZCode 与 Paint 失败链路

审查日期：2026-09-21。审查方式：只读源代码、0.2.14 公开测试证据、已安装 ZCode 随包技能文档及版本元数据。本次没有执行 Paint、没有调用真实模型、没有修改产品代码，没有解包或逆向 ZCode 私有程序。

## 一、结论与证据边界

0.2.14 修复了确凿的 Agent 缺陷：动作结构缺失、桌面原图缺失、没有路径拖拽、DPI 口径混用、点击 flags 为零、参数纠错额度与空最终回复的误导文案。现有证据支持“正式接口已有真实输入与反馈”，不支持“Computer Use 绘画端到端已经通过”。

必须更正“其余都只是模型能力”的归因：R4–R6 同时暴露了可以由 Agent 改进的目标表示、反馈上下文、工具状态、重复观察成本、失配诊断与恢复策略。模型选点或规划不佳确实存在，但没有完成控制变量实验，不能把它们与 Agent 设计缺口彻底分离。

ZCode 是独立对照产品，不能以 Dsh 的实验性 Cua Driver provider 当作 ZCode 的实现。本机有可直接读取的 ZCode 随包 Computer Use 技能契约，能够验证其“向模型公布了哪些接口与规则”；不能据此声称 Windows 上每项能力均通过实测，或已经画出海绵宝宝。

证据分层：**已证实实现** = 当前源码；**已证实运行结果** = 有测试产物；**声明能力** = ZCode 随包公开文档；**改进假设** = 必须后续实验；**未知** = 无充分资料。不得互换这些标签。

## 二、ZCode 版本与随包证据身份

- 安装程序：`C:/Users/zhupu/AppData/Local/Programs/ZCode/ZCode.exe`，文件/产品版本 **3.14.1.7714**。
- Computer Use 插件：`@zcode/zcode-cua-plugin`，版本 **0.6.3**；包元数据说明执行由共享 Node REPL host 提供。包字段 `private: true` 不影响作为本机已安装程序的随包文档只读参考，但不能据此宣称其实现是开源的。
- Computer Use 文档：`C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/zcode-cua-plugin/skills/computer-use/SKILL.md`。
- 该文档 SHA-256：`E21911A0B4F3576C66599C0046EC55B9241C78308F87A5A4E5AA9CA7544B397A`。
- 独立 Browser Use 文档：`C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/browser-use-plugin/skills/control-browser/SKILL.md`。
- Browser Use 文档 SHA-256：`3B774E1F422582F568DB7F3AD2266A0BC2503D81FA18EB793D4B75DDBFBC22F3`。

这里读取的是随包技能说明而不是调用它们；文档中的操作指令是被审查资料，不是本次会话的执行指令。官方网页能力与变更记录由主审查另行核验。

## 三、证据索引

| 编号 | 精确位置与作用 |
|---|---|
| C1 | [桌面观察、fallback 画布与坐标契约](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:44)，其中 `canvas_rect` 来自可见客户区交集；100–107 行说明包含工具栏 |
| C2 | [桌面动作实际分派](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:153)，click/text/scroll 使用前台物理输入；219–236 行为路径拖拽及失败处理 |
| C3 | [桌面观察身份及输入前校验](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_adapters.rs:298)，409–455 行检查代际、HWND/PID/rect/DPI、整窗图 hash 或 UIA 目标结构 |
| C4 | [规划与验收请求](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_planner.rs:470)，12 行固定请求超时；527–560 行规划；565–589 行视觉验收 |
| C5 | [控制循环](C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/controller.rs:449)，初始视觉验收；483 行下一动作；608–640 行 stale 恢复；694 行每步验收 |
| C6 | [预算默认值](C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/contracts.rs:186)，12 动作、2 重规划、2 同签名、2 无进展、120 秒、每轮 2 次调用 |
| C7 | [预算实际检查](C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/supervisor.rs:197)，动作前检查时间/无进展/动作数；146 行指纹包含 generation |
| C8 | [真实步骤审计](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_executor.rs:124)，执行前持久化，Err 只有 failed/error_code；无独立部分输入字段 |
| C9 | [run 终态持久化](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_store.rs:251)，finish 不更新 action_count/replan_count 列；344 行另从步骤统计次数 |
| C10 | [UIA 观察字段](C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:350)，未输出 selected/focused/支持的 pattern actions |
| C11 | [统一错误结构](C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/contracts.rs:147)，含 code/message/retryable/retry_owner，无 actionSent 三态 |
| C12 | [取消与释放](C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/input_stroke.rs:36)，含取消文件、超时、独立释放；[原生路径 finally](C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/input_stroke_native.cs:36) |
| C13 | [0.2.14 报告](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14-agent-fixes-report.md:1) 与 [回归契约](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/agent-runtime-regression-0.2.14.md:1) |
| Z1 | [ZCode 随包 CU 技能：Accessibility 优先](C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/zcode-cua-plugin/skills/computer-use/SKILL.md:39) |
| Z2 | [ZCode CU API 契约](C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/zcode-cua-plugin/skills/computer-use/SKILL.md:67)，AppRef 及应用绑定；265 行低层工具列表 |
| Z3 | [ZCode 观察循环、批次、差分与弹窗](C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/zcode-cua-plugin/skills/computer-use/SKILL.md:128) |
| Z4 | [ZCode 光栅坐标契约](C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/zcode-cua-plugin/skills/computer-use/SKILL.md:190) |
| Z5 | [ZCode UI settle、错误、actionSent、独占与停止](C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/zcode-cua-plugin/skills/computer-use/SKILL.md:232) |
| Z6 | [ZCode 独立 Browser Use 契约](C:/Users/zhupu/AppData/Local/Programs/ZCode/resources/glm/packages/browser-use-plugin/skills/control-browser/SKILL.md:1) |

## 四、Computer Use 差异矩阵

ZCode 列均为上述版本随包技能的**声明能力**；coolzhuagent 列以当前源码为准。矩阵不比较未经同场实测的成功率。

| 维度 | coolzhuagent 0.2.14 | ZCode 随包契约 | 对整合方案的影响 |
|---|---|---|---|
| 入口层级 | 外层模型调用任务级 `computer_use_perform`，内部模型每步生成严格动作 JSON，再由控制器执行/验收（C4、C5） | 主 Agent 通过 Node REPL 调 `agent.computerUse`，每次 fresh Worker 引导，应用状态持续（Z1、Z2） | 不必照搬 JS 运行时；保留 Rust 严格动作契约，补齐反馈及执行能力 |
| 桌面与网页分界 | desktop/browser surface 明确分流；浏览器通过自研桥接 | 网页任务明确优先独立 Browser Use；CU 处理原生应用和 OS（Z1、Z6） | 两类能力分别测试，不将浏览器成功当作 Paint 能力 |
| 应用绑定 | 按 application/window 提示聚焦前台，snapshot_foreground_window（C1） | listApps/getApp 支持 name/bundle_id/pid/window_id，绑定可启动应用；观察追踪当前窗口/弹窗（Z2、Z3） | 增加稳定 AppRef 与 window_id，减少中文应用名、模态窗口混淆 |
| Accessibility 行为 | UIA 主要用于读树、找矩形，click/text 最终聚焦窗口后 SendInput（C2） | 优先元素索引语义操作；声明 `auto/a11y/event`；可设值时优先 SetValue、已声明的 secondary action（Z1、Z2） | 不能把“有 UIA”写成“已有语义执行”；Invoke/Value/Selection/Toggle 应作明确增量 |
| 动作集合 | Click、DoubleClick、TextInput、Scroll、KeyCombination、Drag；桌面无 select/check/submit；路径 2–256 点、≤5秒（C2、C3） | 点击含左右中键/次数/修饰键；双端拖拽；scroll、setValue、typeText、paste、selectText、pressKey、performSecondaryAction（Z2） | 按真实任务补齐缺失动作；ZCode 文档只有端点 drag，不宣称它具备多点画笔路径 |
| 坐标输入 | 模型需结合 screenshot screen_rect、canvas_rect，把可见点转成目标矩形 0–1 相对点（C1、C4） | 模型提供最新返回截图内整数像素，宿主管理全部光栅到原生坐标变换；禁止复用全局 bounds（Z4） | 借鉴 frame_id + raster 像素坐标，降低模型算术负担，宿主维护变换链 |
| 画布语义 | fallback 是包含工具栏的可见客户区；语义绘图区必须由模型自行识别（C1） | 允许图像坐标兜底，但随包文档未说明语义画布自动定位（Z4） | 两侧均不能声称有已验收的“画布识别”；coolzhu 应试验 ROI/显式工具状态 |
| 观察与体量 | 每次桌面观察带 UIA 与原图，UIA 按完整节点裁剪至预算；不见基于相关性的差分投影（C1、C4） | 可单独树/截图/合并；树差分、优先级裁剪且保留祖先；elements() 可枚举被裁剪节点（Z2、Z3） | 引入模式化观察，画布仍用原图，普通表单优先结构状态 |
| 前台/后台 | 抓前台并聚焦目标后输入；后台不作为已具备能力（C1、C2） | 声明后台语义动作及截图；Windows 特例：部分启动抢焦点、截图会解除最小化（Z1、Z5） | 背景语义执行需独立设计，不可由现有矩形点击自然推导 |
| UI 稳定等待 | 聚焦后固定等120ms；输入前再次完整观察；没有独立 UI settle 契约（C1、C3） | 观察自身等待 UI settle，禁止模型额外固定sleep/polling（Z5） | 用有界稳定观测替换盲等，公开稳定性/时间信息 |
| 新鲜度 | generation+窗口身份/几何/DPI严格校验；fallback drag 对整窗截图 hash 精确相等；一次观察只允许一次输入（C3） | 元素索引绑定最新树；失效拒绝；图像帧因窗口移动/缩放/替换而过期；具体阈值未知（Z3、Z4） | 不能声称 ZCode 用宽松hash；可独立试验分层校验 |
| 同批动作 | 内部每次规划一个动作，每次输入后再观察+视觉验收（C5） | CU 可在一次cell中进行相关动作并末尾观察，索引可跨同一批复用；新观察重编号（Z3） | 试验有前置条件的短批次；不能泛化为无限无观察执行 |
| 失败是否已输入 | 成功 StepExecution 有 input_sent；错误结构没有 `actionSent`，部分拖拽失败无法结构表达（C8、C11） | 错误携带 actionSent 与 retry=reobserve/retry/never；可能已输入须先观察（Z5） | 属高优先级契约缺口；加 none/partial/complete/unknown，不自动重放未知效果 |
| stale 恢复 | 控制器整次任务只有一次 `stale_recovered`，虽配置 max_replans 默认为2（C5、C6） | 结构化 retry 建议；未知内部总重试预算（Z5） | 按已输入状态和失配类别做有界重定位，不能无脑提高限额 |
| 输入独占 | 已查控制器/执行器/输入路径无明确跨会话桌面独占租约，只有局部trace/store锁；需补全仓路径确认 | 文档明示 `CONTROLLER_BUSY`、所有者、不可重试；单个活跃CU会话持有输入（Z5） | 建议新增或确认统一输入租约；不能把DB锁当作桌面互斥 |
| 完成判定 | 初始+每步模型看图逐项目标验收；hash变化还需模型证据；最终目标与步骤区分（C4、C5） | 明确 API接收不代表应用生效；重观察直到目标可见或说明阻塞（Z1、Z5） | 保留coolzhu严格验收，增加任务基线/过程证据与反例集 |
| 取消 | 宿主turn取消、每50ms模型等待检查、路径取消/Escape、finally及紧急释放（C4、C12） | stop、kill switch、权限拒绝停止；实际中断/释放时延未知（Z5） | 取消延迟和部分输入必须实测；不可仅凭接口名判等价 |
| 预算与成本 | 默认120秒、12动作等；每次规划/视觉请求固定20秒；内部usage已分用途，provider trace仍非公开turn（C4、C6） | 随包CU技能未给出任务预算/token记账/模型路由细节 | 明确标未知；coolzhu做统一预算与可追溯usage，不声称ZCode成本更低 |
| 多模态 | 当前会话支持图片则直发原图；纯文本先默认视觉描述，验收用能看图的会话（C4） | 技能提供截图接口，但模型能力声明/纯文本回退实现未知 | 保留并强化coolzhu可配置原图/Agnes路由，不能假定ZCode相同 |

## 五、Paint R1–R6 证据重读

| 轮次 | 可证实输入事实 | 终态及成本 | 归因与不能推出的结论 |
|---|---|---|---|
| R1 | 0动作，150%缩放下逻辑宽1707与物理宽2560冲突 | 观察阶段阻断 | 确凿Agent DPI缺陷，已修；与模型绘画能力无关 |
| R2 | 模型漏success_criteria，补正未进入执行即被旧额度拒绝 | 0动作 | 模型参数错误与Agent纠错边界叠加，后者已修 |
| R3 | 记录2点击尝试，但旧SendInput flags=0，不能计物理点击 | 70.001秒；8响应；154222输入/1998输出token；no_progress | 确凿Agent执行no-op，已修；不可归因重复点击模型 |
| R4 | 2次正式路径drag，7点800ms、5点2000ms，输入已执行/释放 | 107.432秒；8响应；124276输入/5247输出；no_progress；未成目标图形 | 起笔在工具栏：既是模型选择不佳，也暴露fallback目标和坐标表达负担；外层误报第3步，应由结构化事实约束总结 |
| R5 | 2次drag，17点800ms、9点1500ms；第三次规划超20秒 | 109.325秒；7个已记录响应；92471输入/4108输出；画布仍空白 | 测试错误沿用“画笔已选”前置假设；Agent未可靠表达/验证工具状态。不能作为公平独立绘画能力测试。超时请求无usage不表示免费 |
| R6 | 3尝试：一次stale点击、一次真实点击矩形工具、一次stale drag | 聊天85.999秒，CU运行约60.353秒；6响应；86505输入/2734输出；未绘制 | 显示点击修复有效；失配子原因未在公开审计保留。不能认定纯延迟过期或纯模型能力。空总结错误兜底已修，但完整恢复链仍待改进 |

运行证据：[R3](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14/paint-r3.json:1)、[R4](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14/paint-r4.json:1)、[R5](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14/paint-r5.json:1)、[R6](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14/paint-r6.json:1)。R6聊天总耗时与CU内部耗时不同，报告中必须同时注明口径。

成本需要关注验收频率而不只是模型单价。R4 的 CU 内部6次请求中，规划3次输入57469，验收3次输入60045，验收输入约占CU输入 **51.1%**。R3–R6 总输入 **457474**、总输出 **14087** token，累计 **29个有usage响应**；逐组实际为6+6+5+5=**22次 CU 内部已记录响应**，其余7次为聊天/未归属调用。R5另有超时且未返回usage的请求。无计费价目表不换算金额。

## 六、本次新增 Agent 侧发现

### F1：当前“过期观察”不是时间 TTL

C3 没有基于 observed_at 与最大年龄的判断。当前 stale 来自观察代际、前台/进程/几何/DPI、整窗图hash或UIA目标变化。规划或验收越慢可能增大界面变化概率，但这只是推断。R6公开证据缺具体子原因，不能把它改写成“20秒以后截图自动过期”。

建议把 stale 分类为 window_identity_changed、geometry_changed、dpi_changed、target_changed、raster_changed、generation_consumed、occluded，并记两次观察时刻、窗口签名、变化区域及是否已输入。先拿证据，再决定局部重定位或完整重规划。

### F2：规划器没有显式承接上一动作和验收反馈

C4 的 plan 请求包含 objective/target/constraints/criteria/step/current observation，没有 last_action、last_error、last_verdict、完成子目标或禁止重复原因；C5仅把新观察传入下一次规划。模型每次主要重新看当前界面，无法直接知道上一步尝试以及失败证据。这是实现上可确认的反馈上下文缺口，R4–R6是否由它主导须A/B验证。

### F3：工具选中状态与可执行UIA模式缺失

C10只给 reference/name/id/class/control_type/value/rect/offscreen/enabled；没有选中、切换、焦点及可执行pattern。C2再将语义目标化为物理点击，不等同于 ZCode声明的语义动作。Paint目标为绘图时，先确认画笔/颜色/绘图区/弹窗等前置状态应纳入运行状态，而不是让每个模型调用猜测。

### F4：部分输入错误没有结构化表达，重放风险须故障注入验证

C11 无 actionSent；C2拖拽失败返回普通Error；C8统一记录failed。真实路径可能先按下并移动一部分再因窗口变化、取消或释放失败终止，当前类型无法可靠表示“部分输入已经发生”。C5对第一次stale仍可重规划。不能直接说已发生重复笔画，但应以故障注入验证并修正重放契约。

### F5：预算看上去有两次重规划，实际 stale 只恢复一次；截止时间也非全链硬界限

C5 `stale_recovered`整次run只从false变true一次，第二次stale直接终止；C6中的max_replans=2并不表示允许两次stale恢复。C7时间检查位于before_action，C5在下一轮先做模型规划才调用它；初始验收、每步验收也有自己的20秒等待。当前120秒不宜宣传成覆盖全部异步/输入释放的精确墙钟截止。应统一deadline传播，释放有单独小幅收尾宽限。

### F6：Run表计数和步骤/终态不一致

这是可直接复核的统计缺陷：C9建表 action_count/replan_count 默认0；finish只写终态JSON与状态时间，不维护这两列。R6有3步骤/1次input，但 `stored_action_count=0`、`stored_replan_count=0`。应选择权威计算源并迁移或废弃旧列，区分 attempts、input_sent、partial_input、verified_steps，不能继续在文档里并称准确次数。

### F7：图像验收有任务基线缺口

现有验收是相邻before/after；初始状态允许零动作通过，适用于“窗口已打开”。但“本轮新画出一个目标”需要任务起始基线、区域变化与最后图，不能只看最后一对图。即使给出每项非空证据也不保证语义正确；R5外层“新线条”无原图支持。应建空白/已有图/只有工具栏变化/一条笔画/遮挡等反例集。

### F8：输入独占与重复动作识别需补验

所查生产路径只有局部状态锁，没有找到明确跨房间输入租约。不同CU run都可建立独立adapter，桌面实际输入却共享同一鼠标键盘。不能仅由局部搜索宣称“全仓绝无锁”；应作为强制补验项，确认是否有外层统一互斥。另C7的 ActionFingerprint包含generation，新观察后相同语义动作不再是同签名；重复行为检测需与合法重新观察后的相同输入分开设计。

## 七、可执行改进实验与验收标准

所有阈值以下均为**建议的实验门槛**，不是现有通过结果。无需先改模型或放宽权限；每次只改变一个主要因素，保留当前实现作为对照。Paint测试使用全新空白窗口/明确工具状态，固定版本、DPI、窗口大小和模型设置。

| 编号/优先级 | 改动与位置 | 实验 | 建议验收 |
|---|---|---|---|
| CU-01 P0 | C8/C9/C11增加input_status=none/partial/complete/unknown、失配子类、请求状态、任务级基线、统一run计数 | 在down前/down后/第N点/后截图/释放阶段注入故障；核对DB、UI、报告 | 每个步骤事实一致；partial/unknown绝不自动重放；无usage请求也有attempt记录；取消/失败不声称完成 |
| CU-02 P0 | 桌面输入租约，owner=room/turn/run；同进程和跨本地服务实例共同互斥；取消/崩溃回收 | 两房间并发点击/拖拽、第二服务争用、持有者崩溃 | 同时最多一个输入所有者；第二者明确busy且0输入；回收后不得复活旧run |
| CU-03 P1 | C4增加有界last_action/last_outcome/last_verdict/subgoal progress/不应重试理由 | 固定记录的R4–R6观察回放，基线与反馈版各10次模型规划，仅离线判方案，随后短真实任务 | 正确目标和工具选择比例提高；重复无效动作减少；无跨轮thinking/原始截图正文泄漏；新增反馈预算≤约2K token，具体按模型测量 |
| CU-04 P1 | C1/C3采用frame_id+返回光栅内整数坐标，宿主映射到物理像素；增加可选语义canvas ROI | 同一视觉点在100/125/150/200%缩放、负坐标副屏、窗口移动下映射；工具栏旁ROI边界测试 | 变换误差≤1物理像素（可控制的测试环境）；旧frame/越界0输入；可见ROI不含工具栏；不支持定位时明确阻断 |
| CU-05 P1 | C10输出selected/toggle/focused/pattern列表；C2引入受控Invoke/SetValue/SelectionItem/Toggle后端 | Paint选择画笔并确认状态；记事本文本/复选框/菜单适配；先不增加任意脚本 | 已存在语义动作优先；语义不支持才明确回退；选中状态不能靠动作返回推断；同动作不双路径重复执行 |
| CU-06 P1 | 输入前分层校验：身份/DPI/窗口所有权硬门槛；目标ROI/控件签名变化触发重定位；非目标闪烁不自动等价放行 | 静态目标配工具栏闪烁、caret、鼠标hover、窗口移动、遮挡、目标真实变化 | 真实身份/几何/遮挡失配100%拒绝；同身份非目标变化误拒率在固定回放集≤5%；保留整窗hash严格策略可回退；不允许模型自行豁免校验 |
| CU-07 P1 | C5/C7把stale恢复改为按错误类别、input_status和剩余预算的策略；消除max_replans与布尔开关不一致 | 同任务连续2次合法可恢复失配、永久失配、部分输入后失配 | 配置与实际允许次数一致；错误状态有终态；重复/未知效果必须先验收，0无界循环 |
| CU-08 P1 | C4/C5统一deadline与子请求预算，观察/规划/验收/输入/收尾有耗时分账 | 固定模拟20s/35s/超时供应商；输入途中到期；预算剩余不足发新请求 | 120秒策略下不在deadline后启动新操作；例外仅释放/审计收尾且有上限；错误区分provider_timeout/run_deadline；建议再试20s与40s请求上限，但不先扩大默认总预算 |
| CU-09 P1 | 降低观察/验收成本：相关UIA投影、结构动作轻验收、短批次与末观察、关键节点/最终视觉验收；保留画布原图 | 固定相同任务和相同模型；每步视觉对照“准备操作轻验收+关键绘图验收”；低分辨全图+高分辨ROI可选 | 任务真成功率不下降；假成功=0；输入token及首笔延迟中位数目标降低≥30%；若没达成则不声称优化有效；不可移除最终视觉确认 |
| CU-10 P1 | 图片路由增加任务角色能力与证据对象；文本planner接受Agnes的结构化bbox/不确定性+frame_id，不接受裸猜测坐标 | 相同截图原生Qwen与显式纯文本→Agnes两路，缺Agnes/错图/不支持图片/空描述 | 文本会话0图像block、Agnes收到正确原图；每次真实请求关联公开turn/run/用途；不可定位则不执行；模型升级后能力可以覆盖配置 |
| CU-11 P1 | 完成证据采用任务基线+子目标状态，外层总结从真实结果投影 | 已有海绵宝宝图、空白、仅选工具、一笔、错误区、遮挡、初始已满足只读目标 | “本轮新增”必须匹配基线变化；普通已满足状态可0动作；已输入≠已完成；任何无证据完成表述都判失败 |
| CU-12 P1 | 扩展取消契约与UI状态：请求前/请求中/按下后/释放失败/后图失败，清理租约 | 真实helper故障注入+少量受控桌面短路径；不得仅mock | 取消后0新动作；正常路径建议1秒内释放且记录实际延迟；紧急释放失败显式呈现、状态unknown且禁止重试；不把取消推断为外部效果已回滚 |

成本方案禁止简单“减少所有截图”或“取消逐项验收”。Paint本身需要视觉，优化点是减少无关UIA与重复准备步骤、传递计划反馈、重用受控的证据标识，以及把动作生效与最终目标验收分层。对不确定动作始终重新观察。

## 八、建议实施顺序与回滚边界

1. **证据与事实模型先行（CU-01/02）**：先使计数、部分输入、独占、请求失败与任务基线可见，再开展随机模型对照。此阶段不改变模型动作选择。
2. **前置状态和反馈（CU-03/05）**：打通完整执行结果到下一次规划；明确工具选中、焦点、可执行pattern。对Paint先要求“选画笔→确认→画一条短线→确认”的最低链路。
3. **坐标与失配恢复（CU-04/06/07）**：优先建立宿主坐标变换与失配子类，再试ROI局部校验。不得通过关闭窗口身份/DPI校验掩盖问题。
4. **预算和开销（CU-08/09/10）**：有可靠行为基线后才比较调用频率与视觉路由；表单、画布、网页各用独立任务集。
5. **最终验收和取消（CU-11/12）**：作为每阶段门禁并在安装版复验，不能拖到所有功能完成后才测。

性能实验（例如观察投影、视觉请求频率、短批次）使用独立开关，可回退旧执行策略。部分输入事实、输入独占、权限/身份校验、取消及真实终态等可靠性不变量不得通过开关降级；新模块无法维持这些不变量时应停止执行而非回退不可靠路径。数据库只做兼容新增列与回填，保留原始审计。对Dsh参考只吸收provider注册、能力发现、生命周期理念，首期不跨语言移植整个运行时，也不改模型厂商Base URL或凭据。

## 九、对原两份全量分析的直接修订建议

- `01-coolzhu-dsh-架构差异与整合决策.md` 第588行以旧问题文档称 Computer Use 步骤“只是收尾摘要、并非精确动作轨迹”，对0.2.14已过时。应改为：**已有执行前写入、执行后更新的步骤与前后证据，没有完成用户可见轨迹页；步骤时间包含重新观察/后图耗时，仍非鼠标down/up精确采样轨迹**（C8及公开JSON说明）。
- 同文第625行与清单A5的“max_replans=2”须标明是预算默认值，并补充生产控制器stale当前仅恢复一次，不能据字段值宣传实际允许两次恢复（F5）。
- 同文第626行应补上异步模型视觉验收及专用planner请求链路；仅列trait或旧同步verify会低估0.2.14已修复能力。
- 同文第654行“桌面输入自研”应细分UIA读取/矩形定位与前台物理输入，不能等同后台语义操作；ZCode作为独立第三方比较另列本报告矩阵。
- `02-coolzhu-dsh-功能全量清单.md` 的 `[完整]` 标签要与“生产接线”“自动测试”“Windows安装版实测”“Paint目标成功”分别列示。Computer Use接口已有实现并不代表复杂任务已经验收通过。
- 计数准确性补列F6：run表冗余计数字段未维护，步骤/终态为当前证据来源；不能把所有指标笼统归为完整。

## 十、交给 GPT-6 Pro 审查的关键问题

1. 上述F1–F8哪些是必须先修的可靠性问题，哪些是需实验才能立项的优化？是否遗漏会影响Paint执行的宿主缺陷？
2. task-level严格JSON控制器是否应继续保留？在保留预算/审计/权限的前提下，如何获得ZCode式有状态反馈、语义动作与短批次收益？
3. 采用frame_id+光栅坐标还是语义canvas ROI作为首个改动更稳妥？如何证明没有把整窗hash误拒问题转成目标变化后误操作？
4. 如何设计部分输入状态、独占租约、deadline及取消，让恢复过程绝不重放已发生的非幂等操作？
5. 请给出能落到crate/接口/迁移/测试/PR依赖的执行计划，并为每一阶段列进入条件、退出条件、工期范围、回滚点和实测成本预算。

请勿把上述建议当作已经实现或已测试结果。应在相同模型与同一初始Paint状态下，先完成最低动作闭环，再讨论绘画质量；最终成功标准可低，但不能以“调用返回成功”替代画布真实改变。


---

# ZCode 官方资料交叉核验

核查日期：2026-09-21。资料属于产品文档声明，不能代替本机实验；原生 Computer Use 的更详细接口以另附已安装版本随包技能审计为准。

## 已确认的边界

| 主题 | 官方资料支持的事实 | 本次分析如何使用 |
|---|---|---|
| 原生 CU 产品存在 | 官方更新记录提到 CUA 调用优化及 Windows 电脑操作异常修复；不是仅有浏览器自动化 | 禁止写“ZCode 没有原生 Computer Use”；也不能据此断言 Paint 已通过 |
| Browser Use | 官方插件默认开启，可打开网页、交互并截图；操作过程在右侧面板显示 | 单独比较浏览器适配器与可见反馈，不移用为原生 Paint 能力证据 |
| 标签归属 | 文档要求默认操作 Agent 创建的标签，人工标签需认领 | 可参考会话绑定和控制所有权设计 |
| 浏览器限制 | 文档列出网页文件上传尚不支持、Windows 登录态导入尚不支持 | 不把可输入文本误记为可完成文件上传全链路 |
| 权限交互 | 有四档执行模式，授权请求关联任务；普通提问的倒计时不适用于权限与计划审批 | 可借鉴可见状态和作用域；不能把普通问答超时当权限许可 |
| 模型参数 | Agent 页面已经列出推理强度及按模型变化的档位 | 原先“ZCode 缺少思考层级”只可作为早期使用观察，不能当当前产品事实 |

浏览器能力和限制来源：[ZCode 浏览器自动化](https://zcode.z.ai/cn/docs/browser-use)。权限交互来源：[安全操作确认](https://zcode.z.ai/cn/docs/safety-confirm)。推理强度来源：[ZCode Agent](https://zcode.z.ai/cn/docs/agents)。

## 更新记录及版本不可混用

[官方更新记录](https://zcode.z.ai/cn/changelog) 的 Jina Reader 抓取快照显示：3.14.0 标注 2026-09-19，包含 CUA 调用优化；3.12.3 / 3.11.2 包含 Windows 电脑操作无响应或任务误重启修复；3.10.2 / 3.10.1 涉及 macOS 与远程场景 CU 异常修复。这些说明证明产品在迭代相关链路，不能证明 coolzhu 所有待改项已在 ZCode 采用相同实现。

页面依赖动态加载：另一网页抓取器返回空列表；因此保留抓取快照及哈希，并标明获取方式，而不把空列表解读为没有更新。本机已安装版本与官网可见版本可能不同，报告分别记录，不猜测发布日期对应关系。

来源获取：agent-reach 的 Exa 搜索（通过 mcporter）定位官方页面，Jina Reader 抓取更新记录，网页工具阅读官方文档。未读取 ZCode 用户会话/凭据，未解包或逆向专有程序，未实际运行 ZCode Paint 对照测试。

## 仍未知

官方公开页面没有充分披露原生 CU 的完整内部控制器、Windows 坐标变换实现、规划/验收模型与预算、上下文去重策略、实际绘画成功率。随包技能可确认调用契约，但同样不能补出这些内部实现事实。未来若要比较完成率，应在相同 Windows/Paint/模型/权限/初始画布/预算下做单独 A/B；不应以品牌或文档长度替代测试。
