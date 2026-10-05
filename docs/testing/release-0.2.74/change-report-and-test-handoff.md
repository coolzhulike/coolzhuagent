# 0.2.74 Browser Use 加载接管改动与测试交接

日期：2026-10-05。本报告区分源码候选实操、安装包构建、安装版复测。**当前已完成源码候选实操；安装版复测尚未完成，不能称整个 Browser Use 全面验收结束。** Compute Use 基础能力按用户新范围沿用 0.2.73 正式 Paint 单笔新增与释放验收，完整人物绘画已退出必做清单，微信不改不测。

## 问题与最终行为

0.2.73 在右栏原生浏览器加载慢页面或慢弹窗时，会失去文档输入资格，同时失去模型用于明确地址导航的观察引用。已释放的点击之后，模型只能超时停止；主会话手动导航能接管，但不能算模型 Browser Use 已闭环。

本轮分离宿主的导航控制资源与就绪网页的文档输入资源。加载中返回明确的 `loading=true` 状态和仅用于 Navigate 的控制引用；不给网页节点、文档凭据、viewport 或成功证据。模型可明确导航到另一个 HTTP(S) 页面，之后重新观察真实文档再点击、输入或验收。没有延长原网页观察超时，没有撤销命中检查，也没有改输入未知立即停止的规则。

## 模块职责与改动映射

| 模块 / 文件 | 职责与改动 | 回归检查点 |
|---|---|---|
| `native-browser-protocol/src/lib.rs` | `PageObservation` 新增加载状态、独立导航引用；校验加载观察为空文档，控制引用不是伪造 AX/DOM | 加载状态不能携带网页节点、文档 token、焦点或 viewport；旧观察默认值兼容 |
| Shell `native_browser_navigation.rs` | 单独存储 `ControlResource` / `VerifiedNavigation`，绑定工程、聊天室、generation、revision、URL、popup sequence；引用120秒、最多4项，动作后失效 | 控制/文档票据不能互借；切换资源、手动导航、出现新popup后旧引用不可复用 |
| Shell `browser_panel.rs`、`native_browser_host.rs` | 可见控制资源在加载中保留；文档 `input_resource` 仍拒加载；显式导航建立新 revision 回执；开启 WebView 原生缩放快捷键 | 正常显示/关闭、导航、popup晚响应、125%缩放；Web和Shell须同包升级 |
| Shell `native_browser_observation.rs` | 页面短暂等待后仍加载则返回控制状态；就绪后才读真实文档；文档读取期间转加载也返回对应状态 | 加载不能借旧页面事实；不能把 pending URL 当已载入网页 |
| Shell `native_browser_input.rs`、`native_browser_edit_input.rs` | `VerifiedOperation::Document/Navigation` 分开验证与执行；UI闭包再核同一控制身份；旧DOM导航仍要求原文档输入资格 | click/text/scroll/keys不能借nav引用；导航回执目的资源使用新revision；down/up仍连续入队并分别确认 |
| Web `native_browser_adapter.rs` | 只读请求不暴露导航引用；只有 Navigate 可用nav；加载保留未结算来源，但仍比较host/工程/聊天室/资源/generation/revision | Opus发现的手动导航串链问题已修；加载不封闭文档终点，就绪后核真实终点 |
| Web `computer_use_planner.rs`、`native_browser_verification.rs` | Navigate schema接受独立nav引用，其它动作只接受文档引用；加载状态不作目标成功判定 | loading目标false；动作投递/释放不能代替目标完成；就绪成功必须由新鲜页面原文核验 |
| Web `devin_acp/process.rs` | 仅测试辅助PowerShell启动等待5→20秒，修复PR远端冷启动超时 | 正式调用期限、3秒进程树排空、2秒主进程回收及真实排空后解锁断言不变 |

路径前缀：协议 `modules/computer-use/packages/`；Shell `modules/gui-desktop/packages/tauri-shell/src-tauri/src/`；Web `modules/gui-web/packages/web-console/src/`。新增控制资源模块仅负责导航，不承接文档命中、模型规划或权限设置，避免混合职责。

## 真实模型及环境

所有 Browser Use 实操 requested/effective 均为 **`swe-2-medium`**，未切换Qwen，没有模型回包夹具。`resolved_model` 为null，只能确认请求与有效配置，不能伪称服务端解析模型ID已有独立回执。方案/实施/最终技术讨论均为 `claude-opus-5-5-high`，通过本项目聊天室真实调用；Opus只审阅提供的代码片段/diff，没有本轮全仓工具审查。

Windows实际150% DPI，只有1个活动显示器。浏览器125%由原生快捷键调整，页面真实DPR从1.5变为1.875，`visualViewport.scale=1`；这是浏览器缩放，不是CSS zoom。表单预置、地址栏预置和缩放是主会话辅助操作，不算模型目标动作。原输入安全库保持不变，完全访问沿用既有授权；一次候选重启漏注入原库时未发模型任务，补回原库连接后再测试。

候选二进制摘要见 [实际二进制](candidate-browser-20261005/final-binary-hashes.json)。本轮源码功能实操的Web为 `B1E9458D…`、Shell为 `12237C72…`，与已安装0.2.73分开；CI测试等待修改在实操后发生，仅 `cfg(test)`，无正式行为变化。新包构建身份另行记录，不把报告HEAD冒充已运行二进制身份。

## 源码候选实操结果

| 轮次 | 真实结果 | 判定与截图 |
|---|---|---|
| popup-recover / loading-recover | 首次实现后3步/2步目标true；加载中模型自主Navigate接管 | 先前候选结果保留，不替代最终来源修复版复测 |
| zoom-form | 104.6秒，前三步投递；第4次计划点击视口外控件，宿主预检 `not_sent`，目标false | 原失败完整保留；不是错点或输入释放未知，不追认成功 |
| zoom-form2 | 148.9秒，click→text→scroll→scroll→click→click，6步，1/1、目标true，pageY=796.27 | [125%前态](candidate-browser-20261005/zoom-form2-before.png)、[提交结果](candidate-browser-20261005/zoom-form2-after.png)；原生输入命中姓名、确认框、提交按钮，未重复输入姓名 |
| replace | 100.4秒，仅click，1/1、目标true | [替换结果](candidate-browser-20261005/replace-after.png)；网页真实down=4812.7、同步replace=4813.2、up=4814.6（同秒179120732xxxx），孤立down/up均0，确实覆盖按住期间节点替换 |
| micro1/2/3 | 52.3/40.2/43.0秒，各仅click，2/2、目标true，无Navigate | 三轮普通自然导航通过；pagehide比up晚8.8/9.4/7.8ms，严格整页竞争**未命中**，停止继续刷试；[第三轮目标页](candidate-browser-20261005/micro3-after.png) |
| popup-final | 88.0秒，click→Navigate→click，3步，2/2、目标true | [旧120秒响应结束后](candidate-browser-20261005/popup-final-after-late-response.png)，旧响应disconnected，接管页仍为EXPLICIT-PASSED-074；全程由模型Navigate接管 |
| loading-final | 74.0秒，Navigate→click，2步，2/2、目标true | [正在加载](candidate-browser-20261005/loading-final-before.png)、[接管后](candidate-browser-20261005/loading-final-after.png)。旧slow响应可能仍写完，不宣称TCP必断 |
| nav-slow-chain | 台账86.5秒，源页Navigate慢页→加载中Navigate明确目标→click，3步，2/2、目标true | [目标页](candidate-browser-20261005/nav-slow-chain-after.png)、[聊天室内同轮回复](candidate-browser-20261005/nav-slow-chain-reply-visible.png)，对应Opus补充的revision链验收 |

各轮 `*-request/response/facts/details/reply`、网页事件、模型身份及逐步投递/释放见 [证据目录](candidate-browser-20261005/) 和 [结构化汇总](candidate-browser-20261005/regression-summary.json)。11轮SWE实操共61项ACP请求，三轮Opus共3项；全部terminal/drained，最终未结算0。已投递步骤没有ReleaseUnknown；首轮缩放第4次预检拒绝没有投递，不能算完成4步。

目标页首次真实pointermove buttons=0以及孤立输入0，是micro1结束后主会话在空白处的辅助移动/单击观测，见 `micro1-auxiliary.json`。明确不计为模型动作或窄整页竞争证据。外部API提交的消息已持久化，原打开窗口仍显示旧历史；正常重开控制台后最新SWE回复可见。多张after图左侧旧失败回复只属于旧历史，不作本轮回复联合证据。

## 审查决定及明确边界

[实施审查](candidate-browser-20261005/opus-implementation-reply.md)提出加载期必须保留资源来源校验，已实现并经最终实操通过。[最终审查](candidate-browser-20261005/opus-final-review-reply.md)同意按“基本功能通过；窄整页竞争及多屏未覆盖”交付，**前提是新安装包上再复测通过**。额外要求的显式Navigate进入慢页→再接管链已在候选通过。

没有为自然竞争人为延迟up，没有fake网页事件，没有放宽视口检查或未知释放停止。加载结束但同控制身份的导航引用仍允许用户已授权的明确Navigate；它不会变成网页输入引用，符合正常显式导航意图，不新增一个因loading结束而禁止导航的限制。加载来源不符时当前会停止，Opus建议的“撤销旧输入来源后只返回加载观察”属于可延期的体验改善；不得借该建议扩大来源链。

仍未覆盖：严格down→整页换文档→up、更多浏览器缩放比例/双指缩放、多屏；硬件只支持单活动屏，不能假验收。现有浏览器能力表明确列出的多标签、拖拽、直接select/check/submit等动作仍未开放，不因普通click完成checkbox/submit而宣称所有动作种类都支持。此前同源/跨源自然导航、历史按钮、普通表单正式0.2.73结果见上一版报告；本轮候选不会覆盖或改写旧失败。

独立诊断注意：候选 `/api/diagnostics/health` 返回 `llm.providers` 0/3 ready及桌宠停用提示，与实际Devin调用成功并存；该候选配置健康显示仍需独立核查，不能把整机健康标为全绿。它没有阻塞本轮输入，但也未在本报告宣布修复。

## 给后续模型的针对性复测要求

1. 安装新包，核注册版本、CLI版本、Web/Shell摘要、launcher自检及实际工作区。先关闭自有候选实例；不能按端口杀未知进程，也不能让0.2.73源码候选冒充新安装版。
2. 在既有SWE验收聊天室、原安全库、完全访问下逐轮建立新任务；禁止重放旧call_id或复用旧成功页面。运行普通HTML服务，替换请求中的实际端口；HTML是测试软件，不是模型夹具。
3. 优先复测popup-final、loading-final、nav-slow-chain，必须从动作台账确认Navigate来自SWE；保留加载前态及晚响应后目标页。加载时节点数0、Root候选0，只能控制Navigate；完成加载之后才允许网页click。
4. 125%表单确认DPR与截图，持续滚动到可见再点，核实际client坐标落在对应控件矩形内，最终pageY>0且FORM-PASSED；视口外预检拒绝不能伪称功能通过。
5. 节点替换检查事件time顺序和宿主released；整页竞争最多三次，未命中就保留未覆盖，不能把普通导航成功写成竞争通过。主会话辅助事件单列。
6. 每轮分别检查目标完成、实际投递、释放、模型身份及所有ACP终态排空；任一输入/释放未知立即停止，不重放、不删安全记录恢复。最终模型回复要能在聊天室看到。
7. 新包必要回归普通点击/文本/滚动、同源/跨源/历史按钮、停止/关闭与输入生命周期。只跑改动相关已有工程检查，不新增大量镜像单元用例。其它侧栏、DSH全流程、开机异常分支等全量任务仍独立待完成，不能由本轮Browser报告代替。

## 工程与发布状态

Web和Shell离线build通过；Web相关已有21项、Shell原生5项/面板8项、planner导航边界1项通过。协议测试命令成功但0项测试，只算编译/命令成功。远端旧HEAD曾有PowerShell辅助启动5秒超时，本轮延长仅测试启动确认并复验，保留远端失败原记录。新HEAD远端检查、安装包构建/摘要以及安装版实操结果随后追加，当前不预填成功。

草稿PR沿用 [PR #80](https://github.com/coolzhulike/coolzhuagent/pull/80)，不合并、不转正式评审。归档不包含数据库、API密钥或未经筛选的服务日志；`manifest.json`按原字节校验，目录内属性禁止Git改写证据换行。
