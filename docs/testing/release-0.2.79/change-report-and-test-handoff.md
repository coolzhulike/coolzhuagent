# 0.2.79 开放Shadow Root点击修复与针对性测试交接

## 问题与最终实现

0.2.78正式默认入口的真实SWE #329/#330中，开放Shadow Root内部按钮可见，但没有独立操作引用，最终verification_failed、attempts0，父子次数均0。这是Agent侧观察能力缺口。[原正式失败及其它边界报告](../release-0.2.78/browser-boundary-followup.md)保留，不能由后续候选成功追认为旧版通过。

宿主固定只读DOM.getDocument节点采样启用pierce；一个独立的小模块native_browser_dom负责筛选普通children和明确标记为open的Shadow Root，仍不进入contentDocument、封闭/UA Shadow Root或伪元素。开放根内AX控件按实际backendNodeId绑定原顶层文档身份；执行前继续核对文档、角色/名字、交互状态、几何、命中点、面板资源和期限。目标归属核验进入Shadow Root时切断父控件的继承资格，必须独立选择内部控件自身的本轮新引用。

这次仅关闭开放Shadow控件点击缺口，不宣称Shadow文本编辑、键盘、封闭根或iframe内部自动化已通过。采样仍有深度、2048节点和响应长度上限；截断或无法确认的目标不提供部分许可。没有增加正式UI调试控件，也没有放宽安全隔离、同目录EXE身份、用户权限或输入释放规则。

协议字段按[Chromium官方DOM定义](https://github.com/ChromeDevTools/devtools-protocol/blob/master/json/browser_protocol.json)核对：pierce会同时展开iframe及Shadow Root，因此宿主必须另行筛选允许绑定的同文档开放根，不能把读取能力视为跨文档输入许可。

## 候选真实模型验收

原聊天室room-1791131523339、Agent session-1791131217833、Devin远端veiled-anise保持。requested/effective=swe-2-medium，resolved_model=null原样保留。不用Qwen、Opus、子Agent或模型回复夹具。候选在默认8765运行同目录新构建后台/桌面壳，沿用原工程与真实安全库。

| 独立任务 | 消息／耗时 | 实际证据与结论 |
| --- | --- | --- |
| 明确选择Shadow子按钮自身 | #331/#332，54.8秒 | 一次click、steps1、sent/released、freshness_confirmed；父0、Shadow子1；pointerdown/up/click均trusted，子监听shadow-click=1。正例通过 |
| 指定父按钮，中心被Shadow子控件覆盖 | #333/#334，33.3秒 | hit_mismatch、not_sent/not_needed、steps0；父0、子0、当轮输入事件0。预期拒绝通过，模型目标未达成且不重试/补发 |

顶层document事件target在Shadow边界重定向为shadow-host；不能误报为直接操作了宿主。子监听、独立计数、宿主绑定与实际动作回执共同确认子按钮点击。[原图、台账、源码文件摘要与进程配对](candidate-shadow/manifest.json)。每段ACP均terminal/end_turn并排空。

![开放Shadow子按钮真实点击通过](candidate-shadow/shadow-candidate-child-after.jpg)

![父按钮误击边界维持零输入](candidate-shadow/shadow-candidate-parent-after.jpg)

桌面壳离线cargo build与既有69项检查通过；新增检查限定开放根引用、重复/超预算拒绝和独立目标归属。它们用于代码边界回归，不替代真实模型软件截图。Web业务没有在本次Shadow修复中改动。

## 正式安装验收与后续测试

0.2.79已由正常发布链构建，六个发布门均通过；Windows安装退出0，Program Files中1150个文件的长度和SHA256与发布包逐一匹配。包源码683c17e74ca98cf85c3e67142e68a7c04071da5d，源码快照62944ec1b3885c9c0b237fa6d312e22fae7121ad7899c9e7f0b064f71947eb09。MSI为276327042字节，SHA256=845bf1c6f4c9376c643047cff3abaab1bfc6651a4c61837ab2c95899bf36b0cf。

正式默认8765、原SWE-2-medium / veiled-anise完成以下两轮，均有截图、真实网页事件和ACP终态排空证据：

| 独立任务 | 消息／耗时 | 正式结果 |
| --- | --- | --- |
| Shadow子按钮自身引用 | #337/#338，23.4秒 | 一次真实点击，sent/released，父0、子1；pointerdown/up/click均trusted，独立shadow-click=1，正例通过 |
| Shadow子控件覆盖父中心 | #339/#340，18.6秒 | native_browser_target_hit_mismatch，not_sent/not_needed，steps0，父0子0，当轮网页输入事件0，预期拒绝通过 |

[正式安装身份、逐文件摘要、原图及台账](installed-shadow/manifest.json)。顶层事件target重定向为shadow-host，内部独立监听及子次数共同确认实际目标，不能仅凭顶层target判断。

![正式版Shadow子控件一次点击](installed-shadow/installed-079-shadow-child-after.jpg)

![正式版父按钮误击拒绝，父子均0](installed-shadow/installed-079-shadow-parent-after.jpg)

关闭面板尝试单独保留：[时序证据及原图](boundary-gaps/manifest.json)。#341/#342（21.7秒）点击与正常核验先完成，实际关闭太晚；#343/#344（21.6秒）关闭发生在点击已发送且释放后的验证期间，最终native_browser_panel_unavailable、goal=false，右栏保持关闭，无自动重开或补发。这证明验证期间关闭能撤销旧证明；不能计为输入前关闭或按住期间关闭通过。候选#335/#336的观察阶段resource_changed也保留，实际未关闭，具体触发原因尚未定位，不用正式成功覆盖失败。

PR83已合并，新增代码由[PR84](https://github.com/coolzhulike/coolzhuagent/pull/84)承接。683源码push检查成功，但PR检查在真实SQLite首次并发打开用例出现DatabaseBusy（1389通过、1失败、6忽略）。后续只读版本读取的有界重试修补已通过1390项完整回归和10轮既有并发用例；该修补不在0.2.79 MSI中，待0.2.80正常发布链与正式安装验证。原日志未标明确切失败阶段，本地旧实现未重现，不能宣称已精确重现那一轮根因。[CI修补与测试交接](../release-0.2.80/change-report-and-test-handoff.md)。

后续优先处理iframe内部自动化的独立文档身份方案，保留跨URL新文档严格down/up时序和输入前/按下中面板关闭或替换缺口。不增加人为延时或测试钩子制造通过。普通span/SVG、嵌套交互正/负例、普通覆盖层拒绝、同步整页内容替换释放、正常跨URL导航和初始关闭负例已有正式通过，不能替代窄时序。四项任务总体仍未完成；Paint按用户缩减范围基础输入已正式通过，微信不改不测，Opus暂停。
