# 0.2.81 同进程iframe点击与测试交接

## 问题与职责

0.2.80的内置浏览器可以渲染iframe，但只给顶层Frame和开放Shadow中的控件提供操作引用，无法直接点击子文档控件。本轮增加宿主读取的Frame/独立文档根/owner链绑定；模型仍只使用不透明节点引用。文档识别在native_browser_document，缓存绑定在nodes，采样在observation，预检在target，原生输入与释放继续由input负责；不新增权限系统，不将子文档混入普通父节点子树。

初期支持同进程子Frame中的button/link/checkbox/radio click；子编辑、键盘、滚动和导航另行验收。独立进程Frame没有本session的contentDocument时只显示可读范围和truncated事实，不伪造可执行引用；WebView2多Target/session另实施。顶层/子文档导航改变Frame loader、DOM根或owner链时，原观察与引用失效；命中父iframe、其它子Frame或独立子控件不能代替原目标。点击按下后的释放规则及既有2秒执行票据不变。

## 候选结果

Web/桌面壳离线build通过；桌面壳70项代码回归通过，没有新增模型回复夹具。全部实际任务由原SWE-2-medium / veiled-anise执行，消息及回复保留原聊天室。候选证据29份文件包含三种通过、一次错误路线失败和一次未命中输入前窗口的尝试，全部原样归档。

| 场景 | 实际结果 | 验收含义 |
| --- | --- | --- |
| 子文档按钮一次点击 | #349/#350，36.9秒，父0子1，sent/released，3个trusted输入事件 | 候选正例通过 |
| 被Frame覆盖的父控件 | #351/#352，31.1秒，hit_mismatch，steps0、not_sent，网页0输入 | 候选预期拒绝通过，模型目标未达成 |
| 首轮子导航 | #353/#354，11.2秒，extension_unavailable、attempts0 | 测试漏写右栏原生浏览器，走外部扩展路线；不计通过，不删除原失败 |
| 点击后切换子页面 | #355/#356，37.3秒，steps1，verification/observation_stale | 仅验证期间撤销通过，不计输入前竞争 |
| 规划期间切换子页面 | #357/#358，35.3秒，document_changed、steps0、not_sent，父子0 | 候选旧子引用输入前拒绝通过 |

[原图、原请求、真实事件、过程与回执](candidate-iframe/manifest.json)。时间先后来自独立网页事件与实际规划诊断；切换由正常UI按钮执行，未添加宿主延迟，不将模型声称成功当作验收。

## 正式交付与执行者要求

正常0.2.81构建、Windows安装、逐文件核验及正式SWE复测待完成；当前不能把候选结果记为已安装版通过。下一执行者应核实际Program Files源码/版本/全部载荷长度SHA，保持原SWE远端，不替换Qwen或受限Opus，分别实操子点击、父误击、规划期间子导航，并收集前后截图、输入释放/零投递、真实事件、ACP终态排空。

本轮未修改正式前端显示，不添加调试信息、配置开关或多余文字。微信不动；Paint基础能力按缩减要求已有正式通过。跨进程iframe、严格跨URL按住导航、关闭替换面板窄窗口、多屏、插件配置/取消/超时/许可竞态、Goal/Relay附件、完整自动升级、启动其它模式及总体审查仍见[队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)。仅主会话实施与测试。
