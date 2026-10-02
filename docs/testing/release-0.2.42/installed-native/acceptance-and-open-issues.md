# 正式0.2.42真实Qwen Browser Use验收

2026-10-01，主会话独立执行；不使用子代理或模型夹具。原qwen3.8-flash、medium、百炼Base URL及既有密钥不变，原工程/验收聊天室、完全访问保持。安装10/10产物和正式Web/Tauri进程已核验，见[安装收据](installed-artifacts.json)。普通57159网页仅本地按钮与JS计数；主会话准备页面、发送任务、执行关闭/中止回归，不代模型点击目标。

| 轮次 | 实际结果 | 原始证据 | 判断 |
|---|---|---|---|
| AB单次点击 | 真实Qwen结构化调用CU/browser；1次Click/1 step，计数0→1；input sent/released、native_dispatch released，effect_observed、goal passed；CU succeeded/goal=true，父completed | [动作前](01-click-ab-before.jpg)、[运行中](02-click-ab-running.jpg)、[次数1](03-click-ab-after-count-one.jpg)、[最终回复](04-click-ab-final.jpg)、[真实账本](BU042-CLICK-AB-facts.json) | 单次点击通过，整轮50.2秒；不是完整Browser Use验收 |
| AC只读 | 准确读标题“单次点击验收”、按钮“增加次数”、当前次数1；cap0/动作0；CU succeeded/goal=true，父completed | [最终截图](05-readonly-ac-final.jpg)、[账本](BU042-READONLY-AC-facts.json) | 新版只读通过，21.5秒；没有误续历史Paint |
| AD关闭 | 关闭动作1790817136709；模型核验完成1790817133168，关闭晚3541ms | [关闭尝试](06-resource-ad-close-during-verifying.jpg)、[结果](07-resource-ad-final-late.jpg)、[账本](BU042-RESOURCE-AD-facts.json) | 时点过晚，不计资源失效通过；不能用截图文件名推导状态 |
| AE关闭 | S1进入verifying1790817240329；判断1790817240347开始；主会话正常关闭1790817254799；模型完成1790817256276，关闭早1477ms；S2拒旧面板native_browser_panel_unavailable，CU blocked/goal=false/动作0，父completed | [页面已关闭](08-resource-ae-close.jpg)、[拒绝结果](09-resource-ae-final-rejected.jpg)、[账本](BU042-RESOURCE-AE-facts.json) | 只读S1→S2关闭边界通过，28.5秒；交互派发中释放/换工程或房间未据此验收 |
| AF取消尝试 | 本轮没有结构化CU、无输入；模型引用历史cancelled_by_user，父failed/stop_requested_at=null。持久助手消息已加“本轮尚未执行…未经本轮工具回执验证”的既有提示 | [模型回复局部](10-cancel-af-no-tool.jpg)、[账本](BU042-CANCEL-AF-facts.json) | 不计真实取消通过；图片局部不替代本轮账本 |
| AG取消尝试 | 实际CU判断1790817480214→1790817487211，因invalid criteria JSON先blocked/动作0；正常中止1790817491151、父stop1790817491285，父interrupted。stop比模型判断失败晚3940ms | [中止截图](11-cancel-ag-stop.jpg)、[账本](BU042-CANCEL-AG-facts.json) | 不计claim前父取消先提交通过；原CU blocked终态保持。截图有其它项目Godot错误窗遮挡，本轮未操作/关闭它，不推断Coolzhu导致 |


| AH取消尝试 | 实际CU被送往旧Chrome扩展路径，extension_unavailable、动作0；父completed，未中止。当前用户原文“当前右栏页面/当前内置页”未匹配原生后端固定关键词 | [账本](BU042-CANCEL-AH-facts.json) | Agent路由缺口，不能计取消通过；已补当前原文识别及Chrome/Paint/其它右栏负向边界，尚不在042安装包 |
| AI真实取消 | verifying在1790818369370进入；正常中止1790818381855，父stop1790818381967提交、父interrupted；CU1790818381993 cancelled/goal=false，0动作/0step；真实模型请求已派发后取消，无迟到成功写回 | [中止原图](12-cancel-ai-stop.jpg)、[结束原图](13-cancel-ai-final.jpg)、[账本](BU042-CANCEL-AI-facts.json) | 新版派发前取消通过；页面计数0，结束运行提示撤除。未宣称输入派发后的取消/释放通过 |

AB运行图可见准确顶部提示“Coolzhu Agent is using your computer”，结束图中已撤除。本轮为应用窗口截图，不证明全桌面四边或多屏；既有全屏证据保留原版本范围。

模型判断JSON无效继续拒绝输入，不猜测修复回复或当作目标成功。后续可改善明确示例/已有结构化输出配置，但必须另做真实回归。只读目标达成、动作可信回执和父终态分开落账，模型声称成功或取消均不能覆盖真实状态。

尚未完成：交互中资源变化、文字输入/滚动/导航、Paint闭合路径和简易海绵宝宝联合泛光验收、DSH远程插件真实安装运行、四项总体审核。下一阶段技术审查已返回：先Scroll、再Type、最后Navigation；同WebView保持generation、导航递增revision并废止旧文档/节点。Scroll已接线源码并离线编译，未打包安装实测；审查不是软件通过；PR74保持Draft。微信不改不测，Devin搁置。
