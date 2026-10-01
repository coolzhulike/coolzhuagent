# 0.2.43 原生浏览器滚动与路由修复：变更和定向验收

2026-10-01，主会话独立实施，不使用子代理或模型夹具。优先完成 Browser Use，再测试 Paint。原真实 qwen3.8-flash、百炼 Base URL、已有密钥及 medium 不变。本文区分源码、工程检查、构建与安装版实操；没有通过的项继续保留。

## 已确认的问题

正式042已核验安装身份。真实Qwen完成AB单次Click（次数0→1、Released、goal=true）、AC只读、AE在S1后关闭/S2拒旧面板，以及AI派发前正常取消（父interrupted、CU cancelled/goal=false、0动作）。原图和账本见[042实际验收](../release-0.2.42/installed-native/acceptance-and-open-issues.md)。

AH本轮有真实CU，但原文“当前右栏页面/当前内置页”未匹配原生后端的固定词组，进入旧Chrome扩展接口后返回extension_unavailable。这是Agent选路缺口，不归因于模型能力。另有AG目标判断JSON无效；继续拒绝输入，不通过猜测恢复判断。

## 本次行为变化及职责

| 改动 | 最终行为 | 职责位置 |
|---|---|---|
| 当前任务识别 | 当前原文明确“内置页/内置网页/右栏页面/右栏网页”等同义表述时选原生面板；不以裸“右栏”、历史Paint/Chrome或网页内容选择后端 | computer_use_turn_scope；接纳时冻结范围 |
| 有界Scroll | 模型仅表达本次RootWebArea随机引用、方向和1至5的滚动量；该引用代表观察范围，宿主按真实CSS视口生成坐标及有限wheel，不点击RootWebArea | native-browser-protocol、native_browser_adapter/target/input |
| 动作前复核 | prepare及execute均核同面板/文档/原节点、顶层frame普通DOM、视口及当前命中；票据两秒、一次消费、单in-flight；被替换/遮挡/过期时拒绝，不降级桌面坐标 | 桌面target/input；认证传输 |
| 授权及结算 | 复用真实宿主进程身份、输入所有权、现有SafetyStore许可和父会话库claim；Scroll独立input_kind防止回执混用。wheel成功为Acknowledged、sent/not_needed；没有按住输入，不伪造mouseReleased | native_panel_authorization、persistent_panel_executor |
| 不明投递 | 丢回调或错误为DispatchUnknown、may_have_been_sent/not_needed；仍进入既有隔离，禁止自动重放。无按住输入不等于已知投递结果 | 宿主typed回执；既有安全层 |
| 动作后观察 | 输入后废止全部旧节点；重新观察与逐标准判断决定goal，ACK不等于实际滚动、加载完成或目标达成 | nodes、既有CU控制器/验收 |
| 可验证页面位置 | 宿主固定读取CSS viewport的page_x/page_y/width/height，发送有限数值；AX节点可能在视口外，节点存在不能证明已滚到。S1/S2验收期间视口变化拒绝旧判断 | observation、protocol、adapter、verification |
| 判断输出说明 | 给真实验收模型补正确JSON形状示例，要求全部criteria indices、实际证据及无代码围栏；示例不能替代本轮事实，解析失败继续拒绝输入 | computer_use_planner |

没有新增Agent循环、权限系统或任意CDP/脚本接口。原短命helper的READY/Job/退出契约保留；已有安全库v4和会话库v28的绑定JSON/状态TEXT足够，未新增迁移。旧click绑定的input_kind缺省为Click，不把旧回执解释成wheel。

## 技术审查及边界

既有[技术审查会话](https://chatgpt.com/c/6ab013c6-8dc4-83ea-a220-a33c9940783f)的补充回复确认：Scroll可先实施；WebView实例generation和同实例navigation_revision的两层身份满足要求；wheel必须由宿主生成坐标、废止旧观察，回执只描述动作事实。Type下一步需执行前真实焦点、可编辑性及selection复核；Navigation最后实施，请求受理与新文档完成分别记录。[审查结论截图](../release-0.2.42/type-scroll-navigation-followup-review.png)是方案证据，不是软件通过。页面仅显示“极高”，未核完整模型版本，不宣称GPT6 Pro总体审核通过。

当前只支持Click与顶层viewport内有界wheel；没有自动寻找嵌套滚动容器，不宣称复杂页面均可操作。iframe/shadow、文字输入、导航、提交及其它输入能力仍不开放。网页显示能力和模型操作能力分别验收。

## 工程、构建和安装状态

离线Web build通过（1分19秒，133条既有warning），Shell build通过（31.60秒，1条既有warning）。完整Web回归1290通过、0失败、2项既有忽略（44.62秒），另lib8通过/宿主静态1通过；Shell64通过、0失败；模块接线8通过、0失败；tool-registry离线check通过。正常发布包尚未生成，实际运行仍为正式042。不能将工程检查计为Scroll软件通过。

## 可交给其它模型设计测试的矩阵

共同前置：正式新包安装身份/10关键产物/进程路径核验；原Qwen配置不变；原工程及验收房间；右栏网页已显示并加载；输入资源safe。只用普通实际HTML/SPA页面和真实模型；页面不是模型响应夹具。主会话只准备页面、发送任务及执行中止/资源变化回归，不代Qwen点击或滚动目标。

| 用例 | 实际操作 | 判断及证据要求 | 状态 |
|---|---|---|---|
| BU043-ROUTE | 重发AH同义原文任务，仅指当前内置页 | 原生AX观察、PersistentNativePanel绑定，不能出现旧extension_unavailable；若模型没有结构化调用则不计通过 | 待安装实测 |
| BU043-SCROLL | Qwen观察普通长页，从page_y=0向下有界滚动 | 真实Scroll step、方向/量、ticket/attempt/executor；ACK和not_needed；新观察page_y增加、旧节点失效；目标逐项判断；前后原图 | 待安装实测 |
| BU043-NOOP | 在无法继续滚动的页面执行有界wheel | ACK只能证明投递；没有位置/页面事实变化就不能把“滚动完成”或goal当真 | 待安装实测 |
| BU043-READONLY | 明确禁止全部输入，只读新页面 | cap0/动作0，不能因为Scroll新增而开放输入；页面位置正确，原图/账本 | 待安装实测 |
| BU043-CANCEL | 模型已进入verifying/规划后正常中止，在claim前提交 | 父interrupted及stop时点、CU cancelled/goal=false/0派发；无迟到成功写回；结束提示撤除 | 待安装实测；042 AI已通过自身范围 |
| BU043-RESOURCE | 观察后正常关闭右栏或切换房间/工程 | 有真实早于派发的资源变化时序；旧票据不得输入新目标。过晚尝试不计通过 | 待安装实测 |
| BU043-ACK-BOUNDARY | ACK不能代Click释放，错attempt不能结算，验收期间viewport变化不能复用旧判断 | 必要工程边界验证；不伪造生产回执或把工程检查当软件验收 | 工程检查通过，不替代安装版实测 |
| BU043-TYPE/NAV | 后续阶段实现真实文字输入、导航 | 焦点/selection、敏感字段、文档revision；每项动作回执及新观察/截图独立核验 | 尚未实现 |
| CU043-PAINT | Browser闭环后真实Qwen闭合轮廓及简易海绵宝宝 | 真实连续路径、释放回执、前后Paint原图、四边泛光/准确英文运行提示及结束撤除；主会话不代画 | 尚未续测 |

DSH远程插件实际安装运行、交互中资源变化及四项总体审核仍开放。微信保留不改不测，Devin搁置；PR74继续Draft。
