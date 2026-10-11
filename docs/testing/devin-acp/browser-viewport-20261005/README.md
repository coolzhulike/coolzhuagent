# Browser Use 源码候选：滚动坐标、跨步进度与文本取证

日期：2026-10-05。全部软件动作由真实 Devin `swe-2-medium` 执行；审查使用既有 `claude-opus-5-5-high` 会话。主会话仅准备网址、刷新聊天室、归档截图，未代点网页目标、代绘、替换模型或制造模型夹具。

本目录是**独立源码候选**，不是已安装 0.2.72 的验收。正式版本失败见 `../../release-0.2.72/installed-browser-20261005/README.md`，保留失败、不追认。phase 身份见各 running-identity 文件；二进制与源码摘要分列。安全状态始终使用原 LocalAppData/input-safety，未清库、伪造释放或绕过人工恢复。

## 已修复的 Agent 缺口

- 观察层给可交互候选提供可选 `in_viewport`，500ms 总余量采样，未知不丢节点，不自动滚动。
- 规划层提供同一次调用最近八步动作和回执枚举，避免多步任务忘记已输入/已滚动；不回传正文、旧引用或跨运行事实。
- BoxModel 是视口坐标，旧逻辑二次扣除滚动量导致离屏误判；HitTest 是文档坐标，需要仅在命中查询加当前滚动偏移。实际输入仍视口坐标，保留节点/frame/文档/资源/两次布局校验与截止时间。
- 预算结束保留最后验收事实，并标明只是最后观察，不把预算终态改成成功。
- 完整引文分段误拒：宿主仅在有界范围内拼接选中的 StaticText 原文；允许跨空结构与可由前序 StaticText 原文解释的 ITB，不推断父子关系、不增加文字、不跳未选中的 StaticText、控件或其它有名节点。范围最多12。节点匹配优先于宿主字段，原 URL/标题/视口回落与旧相邻路径保留。
- 中间节点诊断只存角色枚举、字数和子串关系，不保存正文；不增加前端调试卡。

## 真实过程及结论

|任务|候选阶段|结论|
|---|---|---|
|natural|phase1|1次点击、3/3，但引文只为数字0且撞标题；不算完整引文闭环|
|form|phase1|前三步投递，第4步重复输入离屏控件，未通过|
|form2|phase2|不再重复输入；滚动后 checkbox 命中预检仍失败，未通过|
|natural-full|phase2|1次点击，完整引文选ST4/ST8，2/3，non_adjacent_nodes，旧失败保留|
|form3|phase3|5步点击/输入/滚动/勾选/提交，pageY=406，FORM-PASSED-072，1/1、goal=true，联合软件截图通过|
|natural-range|phase3|完整引文ST4/ST8仍误拒，2/3；外层追加索引要求未保证内层传递|
|natural-diagnostic|phase4|只引数字0，3/3，不算完整引文闭环|
|natural-span|phase4|完整引文进入内部标准，ITB7/ST8相邻文本通过3/3；不代表ST4/ST8误拒修好|
|natural-static|phase4|只选ST4/ST8，2/3；确认中间5空generic、6/7前序ST的ITB子串，失败截图保留|
|natural-bridge|phase5|同样ST4/ST8完整引文由bridged_text通过，1次点击、3/3、目标页输入0，联合软件截图通过|
|history2|phase5|三次真实点击依次push/back/forward，最新页面第二阶段、完整操作顺序、3/3通过；after截图左侧仍为上一轮natural-bridge回复，不能混作同轮回复|
|cross-bridge|phase5|1次真实点击，127.0.0.1→localhost目标页，ST4/ST8完整引文由bridged_text核验，3/3、目标页输入0，联合截图通过|

请求、回复、details 和 runtime-receipt 提供精确 run_id、模型、步骤、实际回执和进程排空；不得把 action released、截图变化、单独模型自述或工程测试当目标完成。`natural-static-visible-reply-failed.jpg` 与 `natural-bridge-visible-reply-passed.jpg` 对照同一文本缺口；`form3-visible-reply-passed.jpg` 同时显示勾选、提交结果和当前 SWE-2 真回复。

## 工程验证与范围

Web/Shell 串行离线真实 build 通过；原验收模块6项通过，既有案例补了真实拆分形状、漏选正文拒绝、ITB仅匹配后序ST拒绝。原几何2项、观察1项、预算终态1项通过。日志在 ignored tmp 中，摘要记录 source-manifest；不以工程检查替代真实软件验收。

Opus 已进行 progress、坐标、完整文本、桥接、Paint 逐项进度、窗口歧义和 Paint Y 几何边界七次源码只读审查；原始请求和回复归档。工具数量以 runtime-receipt 台账为准，不采用模型自估。getFullAXTree 的通用文档顺序、换行ITB、缩放、RTL横向偏移等仍有实测局限，不宣称任意 HTML 语义拼接已支持。

官方坐标依据：[Chromium BoxModel](https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/core/inspector/inspector_highlight.cc) 与 [DOMAgent HitTest](https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/core/inspector/inspector_dom_agent.cc)。官方大文件只暂存 tmp，不随报告复制。

本轮 Paint 开始前，主会话通过 Paint 保存旧画布为 paint-legacy-preserved-before-W.png 后新建空白画布，保存操作不是模型绘画。before截图证明目标基线空白；未通过主会话给落点、绘制或擦除来辅助模型任务。

Paint W（run-chat-5fbfd2b033df8d4fa93957a7fa9d39d4170645cf9ecce2a2）154.4秒，真实3笔均released，1/3、goal=false、no_progress。最新原图只见两个重叠矩形，眼睛、笑嘴和两腿未完成。第一笔视觉验收criterion0=true/progress=true，第二笔重复矩形/progress=false，第三笔使用window-canvas并落入窗口左上工具空区。动作执行和图片链可用，不等于完整人物通过。planning只有整体进展，未传逐项criterion结果；是否改善重复绘制交Opus审查，不武断归因纯模型。paint-figure-before.jpg 与 paint-figure-after-failed.jpg 保留本轮空白和失败。

W运行时工具截图实际显示四周绿光与顶部 Coolzhu Agent is using your computer；因保存时窗口已退出，此轮尚无持久文件，不能称泛光截图归档完成。单屏看见效果不代表多屏已过。

新包正式回归、前端停止/关闭恰在down/up微区间、显式Navigate覆盖popup、缩放/多屏、完整Paint仍需继续。普通 popup/SPA/fragment 的正式版通过见正式报告；不能借此宣称全部 Browser Use/Computer Use 验收完成。微信不测。

## Paint X：进度反馈改进后的真实失败与 HUD 证据

W 已另存为 paint-W-preserved-before-X.png 后新建白画布，X 不复用旧笔迹。phase6 的最近视觉判断只含 generation/index/met，同观察世代才用于下一步规划，不跨步累计、不改变输入权限；明确 step 为从0开始的调度计数，不等于用户的子目标顺序。

X（run-chat-6617eebf1ce39879edad061744f350b8c82dc183921d7e3f）185.0秒，真实四笔 sent/path_completed/released；前3笔使用 UIA 画布，第四笔 window-canvas 落工具区，释放后下一次观察报 target_ambiguous。终态 stage=observation、attempts=4、steps_completed=4、goal=false。原模型回复误称只有3笔且第4未执行，保留原回复并由宿主步骤/终态证据更正，不把第四笔重发。最新图可见矩形、一个在矩形上方的圆及一个内部圆，嘴和腿没有，完整人物未通过。

paint-figure-X-after-failed.jpg 为失败软件截图；paint-X-preserved-before-Y.png 为主会话随后另存的原画布，不是模型执行保存。paint-X-computer-use-glow-running.jpg 和 paint-X-computer-use-glow-later.jpg 实拍 Windows 四边泛光及顶部 Coolzhu Agent is using your computer；终态窗口清单和 after 图确认单屏提示已撤除。W 没有持久泛光文件的缺口仍保留，不用 X 文件追认 W 或多屏通过。

Opus 的窗口歧义审查指出同进程可见顶层窗口都参与匹配，工具提示只是推测；未取得失败瞬间窗口类型时不能判为系统误拒。phase7 仅补后端枚举诊断（类型类别、owned、no-activate、tool-window、transparent、topmost、enabled、foreground、面积档位与提示是否存在），不记录标题、路径、句柄或正文，不变更选择与拒绝行为，不显示在前端。首次编译因 IsWindowEnabled 导入位置错误失败，按本地 Windows 0.58 绑定修正模块与 feature 后真实 offline build 通过；旧失败日志不作功能通过依据。

## Paint Y：六笔执行、几何未达标，暂缓纯模型问题

Y（run-chat-0aa7350c8aff33f07540ee05baa349ba4ef4c668400608c6）295.4秒，六笔都 sent/path_completed/released；预算终态 goal=false，最后 generation7 观察2/3；原模型回复误称1/3，保留原文并按宿主终态证据更正。paint-figure-Y-before.jpg 是新白画布，paint-figure-Y-after-failed.jpg 可见矩形、重叠双眼、笑嘴、左腿在身体里、右腿跨底边，不能算完整人物通过。paint-Y-glow-running.jpg 实拍运行中的泛光，终态后无该提示窗口。

同一 UIA 引用的身体 top=.15/bottom=.7，而第一眼 y=.115..205 已跨身体顶边，左腿 y=.593..693 全在底边内。这是模型本次提交的几何参数，与实际原图一致，不用于给下一轮落点。最近合法逐项视觉进度实际进入 generation2..6 的下一次规划请求，均count=3/met=1；本轮窗口歧义诊断未触发，不据此认定 X 的过滤问题已修复。数值/类别后端日志归纳在 phase7-diagnostic-facts.json，原日志在 tmp。

Opus Y 再读 planner/bridge，结论为本轮模型几何规划未达标，未找到必须改的 Agent 契约缺口。采纳用户“纯模型能力问题暂不处理”，保留完整人物失败；不增加绘图专用引擎、自动修正落点或限制现有 fallback。模型提出回显已释放坐标的单次实验只是推断，未实施，不称审查建议已经接线。X 窗口歧义真实原因及 Browser 竞争仍待验证。

## 浏览器加载中拒绝、前端停止与恢复

真实慢响应网页在 1791196957862 发出部分内容，60秒后 1791197017863 写完。loading 时 SWE pending（run-chat-2219b952e90df656368cd59310e1c685cf4a6c55878c6642）被 observation/native_browser_panel_unavailable 拒绝，0 attempts/0 input；截图证明网页存在但仍加载中，错误不能解释成窗口完全没打开。主会话只点击前端停止按钮，操作开始 1791197010983、观测结束1791197011452，均早于服务器完成；不代点网页按钮、不称 down/up 微区间命中，也不凭 TCP 缓冲写完宣称已证明网络断开。

随后主会话地址栏打开 ready.html，新的独立 SWE recovered（run-chat-8dffaa60063286cc806c4179f0a2649dd29cef0775968604）45.2秒、一次真实 click，最新 done.html 与 DONE-20261005、2/2、goal=true。load-recovered-visible-reply-passed.jpg 同时显示本轮 SWE 回复和真实右栏完成页；load-pending-before.jpg/load-pending-stopped.jpg 及 UI时间收据分列。此项是普通加载资格/停止后恢复通过，不外推极短输入区间的取消、关闭或替换竞争。


本轮收尾已确认所有ACP排空，候选外壳正常退出、精确后台和两项自有网页服务退出、临时GUI配置原字节恢复。0.2.72日常正式launcher重新启动自检通过，原日常工程保留；此环境恢复不改变本轮实操均为SWE-2的事实。Paint Y原画布另存paint-Y-preserved.png。具体收尾收据独立记录，不把恢复日常界面当新包回归。
