# 内置浏览器当前页入口修补与真实候选验收（2026-10-07）

本轮修复用户只指定“当前内置页面”、未填写target.url时，在已有认证页面事实下仍被URL比较拒绝的问题；另补“右栏已加载原生网页”等同义表述误走外部扩展的路由缺口。主会话独立实施，真实请求继续复用SWE-2-medium及唯一island-kayak，没有新建云端测试会话、换模型、使用模型夹具或恢复Paint测试。

## 改动与职责

`native_browser_adapter.rs`保存每个执行实例第一份已加载宿主观察的来源，包含宿主、工程/聊天室/面板资源、URL与文档token；加载状态不绑定，后来切页不覆盖。`native_browser_verification.rs`只在请求URL缺省时使用该来源，明确URL仍须严格匹配，已结算输入/导航继续沿原来源链核验。独立新鲜度观察不会覆盖原来源。

`computer_use_turn_scope.rs`补齐明确指向右栏原生网页的名称，仍不把设置、统计、Paint或Chrome任务误分类。没有新增工具、权限、投递接口或正式前端调试信息，也没有清除安全历史。详见[设计与风险](../../analysis/2026-09-21-integration-review/native-browser-current-page-plan.md)。

## 六轮真实记录

候选后台为实际offline构建，桌面壳为088正式文件，配套身份见[清单](candidate/manifest.json)。这不是088正式交付范围，也不表示新版本已安装通过。原聊天室room-1791131523339、Agent session-1791131217833保持；六轮每轮一个ACP attempt、end_turn、排空完成和绑定锁释放，内部lane远端为空。

| 用例 | 结果与范围 | 截图 |
| --- | --- | --- |
| scroll | “右栏已加载原生网页”未识别，误走外部扩展，观察前blocked、零事件；原失败保留 | [原失败](candidate/candidate-scroll-route-failure.jpg) |
| scroll2 | 名称修补后，省略整个target；子LTR一次right，位置0→195.3333282470703、可信wheel1，父0/0，succeeded、3/3 | [滚动](candidate/candidate-scroll2-after.jpg) |
| click | 省略整个target；按钮一次真实按下/释放/点击，计数0→1，succeeded、1/1 | [点击](candidate/candidate-click-after.jpg) |
| wrong-url | 明确different.html、当前click.html，blocked、完成动作0、事件0，计数维持1 | [明确错误URL](candidate/candidate-wrong-url-after.jpg) |
| stale | 刷新晚于规划回复与真实点击；真实点击1→2已发送，之后刷新回0。不计为时序负例通过 | [晚于输入](candidate/candidate-stale-late-after.jpg) |
| stale2 | 刷新晚于规划回复约239毫秒、早于投递预检；document_changed、not_sent/not_needed、完成动作0、事件0、计数0。只计投递前文档替换拒绝 | [零投递](candidate/candidate-stale2-after.jpg) |

两项缺省URL正向回执的requested_url为null、observation_origin为current_page，bound_initial_url由宿主观察提供，并非模型补URL。明确错误URL没有被当前页来源覆盖。六轮原始请求、实际参数、回执、页面事件与远端绑定见逐项facts及清单，模型思考正文未归档为测试依据。

严格时序须满足请求<变化开始≤变化结束<规划回复；两轮都未满足，见[第一轮时序](candidate/stale-timing-analysis.json)和[第二轮时序](candidate/stale2-timing-analysis.json)。第二轮虽零投递，也不能据此宣称已完成缺省URL的严格规划期间换页验收。当前修补的基础点击/子滚动、明确错误地址及输入前旧文档拒绝已有证据；严格规划窗口仍单独开放，不能外推全部HTML或四项总体完成。

## 编译与回归

实际offline build通过；已有页面验收边界8项、当前轮路由边界8项通过，新增一个必要当前页来源边界用例，并在既有转场检查中补缺省URL断言。完整主控制台回归1397通过、0失败、6忽略；自动化回归不替代上述真实模型截图、输入回执和可信网页事件。

原安全库2个outcome_unknown许可与9个closed记录保持，资源safe且接受新输入，见[只读快照](candidate/candidate-safety-summary.json)。此报告编写时新安装包尚未完成；后续须独立核验冻结源码、正常打包/安装文件、活动进程，再通过相同SWE会话复测，不得用候选截图替代正式安装验收。

## 其它模型设计复测用例时

从正常聊天室入口发送新独立请求，每轮只调用一次computer_use_perform、surface=browser、max_actions=1，缺省URL用例省略整个target。先确认原面板已加载且初始计数明确；不得从历史补URL、换外部浏览器或失败后补发。正向应有可信事件、完整输入释放和当前页面原文；错误URL及旧文档应零投递，不能把blocked写成执行成功。

时序负例另记录规划请求/回复、普通UI换页区间和真实文档事实，排除变化发生在回复或输入之后的记录。对源宿主、工程、聊天室、面板代次、导航版本和文档token的替换分别核验；不能通过清安全库、改权限或伪造模型输出制造通过。启动演出其它模式、插件剩余生命周期、附件/账号边界等继续按[总队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)推进。
