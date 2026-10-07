# 0.2.86 改动报告与针对性测试交接

记录日期：2026-10-06（本地时区；构建与实操原始记录跨至UTC 2026-10-07）。本轮主会话独立实施与测试，真实模型SWE-2-medium。Opus暂停，微信不改不测。用户已明确Paint不用再测，保留已有基本功能证据，不以完整人物绘画作为剩余验收。

## 交付身份与范围

0.2.86正常构建、Windows安装返回0、1150个安装文件长度及SHA256全部一致。六项发布门通过，未使用SkipBuild。冻结产品源码`84634500ef085d3937497bae31ddd70ca7f9d419`，来源快照`1962312661aeb834b54e2dc94bc56aafc030f6db94507cce6aafbf678d34de03`。MSI共276310658字节，SHA256 `dc854adc28bf88374f756d93381f0d0c0272f3d831bdc308a7884145c656b679`。

实际Program Files主控制台SHA256 `6a183f8498620cd5ada79e45a1e7334215301e5773944c742fd4375e840ff770`；桌面壳`8c54e077183ef9120ae56591759d764a45b3bf463c1989f5a64db94d8462353d`。身份、进程、原始回执、原图和逐文件证据索引见[正式安装证据清单](installed-evidence/manifest.json)。

本版修复跨来源独立进程iframe能显示但编辑/单键/滚动未正确绑定所属DevTools会话的问题。编辑器解析、焦点和选区读取、对象释放跟随子会话；滚动先核对子页局部命中，再按owner链映射，边界不穿透父页。实际输入仍通过既有WebView与一次性许可流程，模型无脚本或任意CDP执行接口。

Devin原先的内部规划另建云端会话，现由当前聊天模型通过`computer_use_respond`交回同job/request规划与验收。桌面原图通过同ACP连接串行续轮，原attempt、预算、取消链与执行锁保留。补齐图片历史重放的元数据兼容，历史图不冒充当前观察；交接提示留临时过程，最终答案仅保留结果。详见[候选改动、原失败及原图](candidate-change-report.md)。

## 正式安装版真实实操

原聊天室`room-1791131523339`、Agent `session-1791131217833`持续绑定唯一远端`island-kayak`。所有下列用例仅一个ACP attempt，结束为end_turn且进程排空、锁释放；内部lane远端为空，无新增云端测试会话。应用重启后仍续接同一上下文。以下均用正常聊天室发送，经安装版8765右栏原生浏览器执行，网页真实input/submit/wheel事件与宿主动作释放回执交叉核验，未使用模型夹具。

| 用例后缀（统一前缀BU-INSTALLED-086，日期20261006） | 实际结果 | 验收结论 | 截图 |
|---|---|---|---|
| INPUT | 子click/type RELEASE086/Enter三步sent/released；子输入/提交1/1，父0/0；3/3 grounded | 通过，外层completed | [输入前](installed-evidence/installed-input-before.jpg)、[输入后](installed-evidence/installed-input-after.jpg) |
| SCROLL-DOWN | 子scrollTop 0→150、wheel 0→1；父scrollTop/wheel 0/0；2/2 | 通过 | [滚动前](installed-evidence/installed-scroll-before.jpg)、[下滚后](installed-evidence/installed-scroll-down-after.jpg) |
| SCROLL-UP | 子150→0、wheel 1→2；父0/0；2/2 | 通过 | [上滚后](installed-evidence/installed-scroll-up-after.jpg) |
| SCROLL-BOUNDARY | 顶部继续向上，native_browser_scroll_boundary；not_sent，零步骤、零网页事件，父子计数不变 | 预期拒绝通过，业务目标false | [边界](installed-evidence/installed-scroll-boundary-after.jpg) |
| FOCUS6 | 子点击正常释放；第二次文字规划期间切到父输入，native_browser_editor_not_focused；文字not_sent，父子零input | 输入前焦点变化拒绝通过，业务目标false | [切换时](installed-evidence/installed-focus6-switched.jpg)、[终态](installed-evidence/installed-focus6-after.jpg) |
| REPLACE4 | 子点击正常释放；第二次文字规划期间正常按钮替换子页面，native_browser_navigation_changed；文字not_sent，父与新子零input | 旧子文档引用拒绝通过，业务目标false | [替换时](installed-evidence/installed-replace4-switched.jpg)、[终态](installed-evidence/installed-replace4-after.jpg) |

FOCUS6时序：真实规划请求1791351057550 < 可信人工焦点变化1791351058022.7（服务观察1791351058032.2195）< 对应模型回复1791351064041。REPLACE4时序：1791351456321 < 子替换1791351456554.2（服务观察1791351456565.3738）< 回复1791351463160。完整同request、可信事件与零文字回执见两份[timing proof](installed-evidence/BU-INSTALLED-086-FOCUS6-20261006-timing-proof.json)、[子替换证明](installed-evidence/BU-INSTALLED-086-REPLACE4-20261006-timing-proof.json)。模型自述不能代替该时序。

## 诊断、失败与发现的新缺口

以下六轮均保留事实，不计上述六项正式通过：FOCUS与FOCUS5在文字已发送后的验收阶段才人工切焦点，只证明旧验收失效；FOCUS4先因未聚焦而拒绝，人工变化在终态之后；REPLACE3未命中规划窗口，未实际替换，正常输入成功，仅为无变化对照。FOCUS2/3走到外部扩展路径，extension_unavailable、零输入；其根因是Agent当前轮语句匹配未包含“内置原生浏览器”，不能归因于模型能力或浏览器无法输入。

该路由别名缺口仍存在于冻结0.2.86源码。明确使用“右栏原生浏览器”可命中现有正确路径；后续应补齐当前用户明确选择的别名，不能从历史、记忆或模型工具参数推导后端。出包后修补必须独立标识，不能追认本包已经包含。

## 其他模型如何设计针对性测试

先验证实际包/来源/进程和原远端绑定，再生成独立任务；不重放旧失败perform，不为每轮新建远端。用真实127.0.0.1父页与localhost独立子页，空计数启动；正常UI准备和模型执行分别计数。模型只调用一次perform并按当次schema响应；未知结果、取消或拒绝立即停止。读取网页可信事件、动作sent/released、当前观察逐项grounding与聊天室终态；成功不能只依赖ACK、图片变化或聊天自述。

正例分别覆盖子click/type/Enter、上下滚动和父输入回归；负例覆盖边界、规划期间焦点与子文档变化。负例只有在真实变化严格落在该文字规划请求和回复之间、文字not_sent且相关页面零input时才通过。未命中窗口保存诊断，不能修改既有运行结果。重点复核同名兄弟、多层嵌套/裁剪/旋转、横向/RTL、输入前面板关闭，以及严格pointerdown至pointerup期间跨URL或面板替换。这些没有在本版获得完整正式证据，不可由本次通过外推。

单会话回归应核对主lane同一远端、内部lane为空、单attempt/end_turn/drained和锁释放，重启后图片历史恢复兼容；远端删除只走明确重绑入口，保留本地历史与原安全库。插件配置变化、执行中取消/超时/许可竞态，Goal/Relay附件、账号过期与切换、升级完整链、开机演出资源失败/减弱动作等见[总验收队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)。Paint按最新用户要求停止复测，多屏无真实硬件证据。

## 检查、日常恢复与发布

实际离线构建通过；主控制台完整1396通过、0失败、6项既有忽略，另lib8和native-host1通过；ACP针对性61通过、3忽略；桌面壳72通过。JS语法与diff格式检查通过。冻结846源码[push检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/37574489435/job/112640218175)与[PR检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/37574492314/job/112640227395)均success，后续文档HEAD另核。

测试运行全部终态后，仅按PID、路径、精确启动时间和SHA停止自有测试配套。已恢复原工程`C:/Users/zhupu/coolzhuagent`的正常桌面入口，启动自检确认086/846及原安全库；见[恢复核验](installed-evidence/restored-daily-086-verification.json)、[实际窗口](installed-evidence/restored-daily086.jpg)。日常界面恢复用户原有聊天室及模型选择，不代表重新使用Qwen执行本轮测试。安全资源safe，历史2个outcome_unknown许可和9个closed隔离项保留，未重置或删除。

本报告闭环本阶段六项OOP安装版验收，不代表全部HTML、所有Browser Use边界或四项任务总体完成。公开分发资产和标签的实际核验结果将记录在[evidence](evidence/)中。
