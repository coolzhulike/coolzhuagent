# 0.2.54 改动报告与浏览器回归

2026-10-01，主会话独立实施/测试。真实qwen3.8-flash、medium、原百炼Base URL及已有密钥保持；没有模型夹具，没有代模型点击次数或绘画。微信不动不测，Devin搁置，Pro审核按用户要求暂停。

## 构建与安装

本包新建原生浏览器视图显式show；失败仍撤销资格并清理。Shell离线build通过，既有64项回归通过；Web沿用053的1294项通过/0失败/2既有忽略，另lib8与宿主接线1通过。提交cd6bec8的远端PR检查36828738206已success，push检查当时仍运行，最终状态另核。

正常退出053的Shell15368/Web27476，确认旧进程与8765监听均为空。正常MSI安装exit0；唯一注册054，正式Shell30840/Web22520，8765唯一归属Web22520，10项关键安装摘要一致。MSI247000445字节，SHA256 `805593866db73fb148d7f56348a93073f3aca5c339c8edb48f76b793cba3cc27`；源码快照`2de4f1d92dbf4f3de4621b1429923b99410b0f217bff5a12dd8b4a765c6bd644`；载荷`14946a1d04e2852ab925a19b11e7e9c40760de996efc7875f18533e93c898e1a`，859文件/324494310字节、6门通过、安全扫描0项问题。出包installed=false保留当时时点。[安装身份](installed-native/installed-artifacts.json)，[安装收据](installed-native/normal-install-receipt.json)。

## 真实结果与遗留

1. 首次无地址显示about:blank，是正常初始状态；输入click.html并点击打开可正常显示次数0。[首次加载](installed-native/01-browser-initial-loaded.jpg)。
2. 关闭后通过更多→浏览器重开，地址仍click.html，但持续白屏；未按打开或刷新，验收未通过。[重开白屏](installed-native/02-browser-reopen-blank.jpg)、[再次观察](installed-native/03-browser-reopen-still-blank.jpg)。白色区域是前端about:blank，已销毁原生实例并未重新创建，显式show不是根因修复。
3. BT运行中关闭：为该场景显式打开页面，不能算自动重开通过。父运行1790839593084—1790839655468，62.384秒。监测1790839611107看到第一步dispatching，但实际UI关闭1790839633831，已晚于第二步释放1790839626342；属于验收等待/步骤间关闭，**没有覆盖交付中关闭**。前两步sent/released/effect_observed；第三步1790839641771—1790839641780按native_browser_panel_unavailable、not_sent/not_needed拒绝。终态blocked/goal=false，无自动重开或重试、无新未知隔离。[关闭现场](installed-native/04-close-bt-requested.jpg)、[终态](installed-native/05-close-bt-stopped.jpg)、[账本](installed-native/BU054-CLOSE-BT-facts.json)、[时点](installed-native/close-bt-timing.json)。未声称目标10达成。

## 下一包方案与测试设计

055修正在原生浏览器前端适配器内保留刚关闭的地址与工程/聊天室上下文；明确再次打开且上下文匹配才重新navigate创建新世代，不续发旧票据或输入。详见[根因、职责与风险审查](../../analysis/2026-10-01-browser-reopen-resume-review.md)。

真实验收：初次加载后关闭/重开无需额外打开或刷新即显示；同Qwen新轮点击0→1；跨聊天室不得加载旧地址；交付中关闭须单独核覆盖时点，不以本轮BT替代。此前真实只读、文字输入、导航、滚动及错误源URL拒绝证据继续有效。随后Paint闭合矩形/简易海绵宝宝与全屏提示联验。DSH远程安装运行、开机舞剑动画及四项整体尚未全部完成，PR74保持Draft。
