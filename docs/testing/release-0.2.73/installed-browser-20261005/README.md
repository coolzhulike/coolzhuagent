# 0.2.73 已安装版本真实软件回归

日期：2026-10-05。用户安装后核验，Windows 注册版本、CLI 均为 **0.2.73**，实际 Web/Shell 摘要与交付载荷一致，源码提交为 `99adfc616ee441045289572031ac0407c0ae90e9`。本轮无源码修改、无新包；不要把报告提交身份写成安装包的构建身份。

## 环境及证据规则

先通过正式 launcher 核日常启动，再正常关闭日常外壳，在既有独立验收工作区使用 **Program Files 中已安装的 Web/Shell** 运行；不是源码候选。标准端口 8765，保留原输入安全库，不清库、不换空库。见 [二进制核验](installed-binary-verification.json)、[运行身份](running-identity.json)、[启动自检](installed-launch-selfcheck.json)。普通本地 HTML 是真实软件目标，不是模型回包夹具。

全部十轮实操 requested/effective 为 `swe-2-medium`，共 39 项 ACP 请求；方案讨论一轮 `claude-opus-5-5-high`，1 项 ACP 请求。40 项全部 terminal/drained，最终未结算 0。工具台账与逐步输入释放见 [结构化汇总](regression-summary.json)及各轮 `*-facts.json`；阶段采样中全局未结算 1 是另一轮仍在运行，不代表收尾未排空。本轮所有已派发步骤释放状态均无 unknown，输入成功与目标成功分别记录。

## 通过与未覆盖矩阵

| 轮次 | 安装版实际结果 | 证据与界限 |
|---|---|---|
| form3 | 95.2 秒，五步，目标 true；点击、文本、滚动、勾选、提交，pageY=406，1/1 | [同轮回复及软件联合图](form3-visible-reply-passed.jpg)，没有重复成功输入 |
| natural-bridge | 42.5 秒，一次 click，3/3、目标 true | [目标页](natural-bridge-target-passed.jpg)；完整9字“目标页输入事件：0”由 ST4/ST8 的 bridged_text 核验，不是只验数字0 |
| cross-bridge | 37.1 秒，一次 click，3/3、目标 true | [跨源目标页](cross-bridge-target-passed.jpg)，127.0.0.1 → localhost；目标页输入0 |
| history2 | 74.6 秒，三次 click，3/3、目标 true | [最终第二阶段](history2-target-passed.jpg)，顺序“起点→进入第二阶段→历史后退→历史前进” |
| micro | 38.5 秒，一次 click、普通导航目标 true；**竞争未命中** | [目标页](micro-target-not-race.jpg)、[网页事件](micro-page-events.jsonl)：pointerup=1791200978334.5，pagehide=1791200978344.6，卸载晚10.1ms，不能算 down/up 之间页面替换通过 |
| load-pending | 12.4 秒，blocked、目标 false；0 输入 | [加载中截图](load-pending-before.jpg)；资格检查 native_browser_panel_unavailable。页面实际正在加载，模型“面板没打开”不是页面事实；不把拒绝报通过 |
| 前端 Stop | 辅助主会话点停止按钮，结束观测早于服务器写完约17.6秒 | [停止截图](load-pending-stopped.jpg)、[UI时间收据](load-stop-ui-receipt.json)、[服务事件](slow-page-events.jsonl)；服务器最终写成功，不证明 TCP 断开；不是微区间取消 |
| load-recovered | 34.4 秒，一次 click，2/2、目标 true | [恢复后目标页](load-recovered-target-passed.jpg)，新的独立 SWE 任务至 DONE-20261005，不追认加载轮成功 |
| paint-single | 49.4 秒，一笔五点/1200ms、sent/path_completed/released，最新验图2/2、目标 true | [绘制前](paint-single-before.jpg)、[绘制后](paint-single-after-passed.jpg)、[当前轮回复](paint-single-visible-reply.jpg)；新增 W 在左侧空白，右侧旧 Y 人物不是本轮成果 |
| popup-pending | 37.3 秒，一次 click released 后 native_observation_timeout，目标 false | [原失败回复](popup-pending-reply.md)，不补发、不追认。加载中 popup 的模型后续操作链尚未闭环 |
| 前端显式地址覆盖 | 辅助主会话导航到 explicit.html，早于慢 popup 响应完成14.35秒 | [覆盖页](popup-explicit-overrides-pending.jpg)、[UI收据](popup-override-ui-receipt.json)、[服务事件](popup-page-events.jsonl)：旧响应 subsequently disconnected；**不是 SWE 的 typed Navigate 测试** |
| popup-recovery | 38.4 秒，接管页新的 SWE 一次 click，2/2、目标 true | [晚响应结束后的目标页](popup-recovery-after-late-response-passed.jpg)，EXPLICIT-PASSED-073，未被旧 popup 再夺回 |
| 单屏运行提示 | 正式运行截图可见顶部英文及四周泛光，终态撤除 | [完整 HUD](popup-pending-running-hud.jpg)。Paint 运行截图仅顶部下缘，不能将它称完整英文 Paint 联合验收；多屏未验收 |

除 form3 外，多个目标页 after 图左侧还停留在旧 form 回复，只证明本轮软件目标页，不充作同轮回复联合证据。各轮原请求、回复、宿主终态和模型身份独立保存。页面标记 FORM/TARGET-072 是复用 HTML 文案，运行二进制身份是0.2.73。

完整海绵宝宝原 Y 仍失败：六笔 released，最后 generation7 为2/3，goal=false；原回复误写1/3，保留原文和宿主纠正。眼睛跨身体顶边、左腿在身体内是实际模型几何规划问题。此次单笔通过只证明执行及验图链路，不能代替完整人物能力。

## Opus 讨论与后续判断

见 [审查原回复](opus-boundary-reply.md)、[审查身份](opus-boundary-facts.json)。实际只读工具调用为 read_file 2、grep_search 2、glob_search 1，共5项；模型自称工具计数不替代台账。读取的是本轮授权复制的输入、目标、观察源码片段及网页事件，不是整个 Chromium/项目全面审查。

采纳“微区间未命中要保留缺口，不无限刷试”的建议。生产派发将 down/up 连续排队、未知释放立即停止、不补发；本轮事件只证明普通导航在 up 后卸载。仍缺目标页首次 pointermove 的 buttons 与孤立 up/down 证据，不能据此宣布竞争闭环。缩放快捷键未观察到实际比例变化，未发送缩放任务，不算125%通过。

下一轮优先：

1. SWE 的显式 Navigate 覆盖加载中 popup，保留加载原失败。先确认观察资格和 Navigate 如何协作，避免把 timeout 整体放宽。
2. 非100%实际浏览器缩放/DPI与多屏；取得真实缩放值和截图再送模型，不用 CSS zoom 冒充。
3. 输入期间停止/关闭/替换的严格 down→页面切换→up 场景；若无法自然命中，作为有界未覆盖风险提交审核，不称已过。
4. 正式首次模式、减少动态及素材失败。最初恢复只拍到聊天室，0帧收据保留；后续修正采集对象，正常前台启动已捕到卷轴展开、Q版人物两个不同舞剑姿态。日常基本流程通过，不外推首次及异常分支。
5. DSH市场完整安装默认停用→明确启用→真实模型工具调用→停用/卸载；Devin宿主插件能力未开放需单独接线，不暗换模型。其余模型配置/附件/侧栏/终端/媒体/更新及去调试文字全量回归仍待完成，微信不改不测。

## 收尾与复现交接

十轮请求及单独审查均按原请求重建新独立任务；不可重放已有 call_id、不可把旧成果用于新任务验收。真实输入未知或释放未知立即停止，不通过删除记录恢复。网页需重开普通本地服务并替换请求中的实际端口，保持页面语义和测试限制；不直接重复旧地址。

验收外壳正常关闭，只清理已确认的自有后台和四个网页服务，GUI临时地址原字节恢复；Paint保留打开且不覆盖旧Y文件。正式 launcher 恢复原日常工作区，自检通过，健康10正常/1工作区提示/0错误；见 [收尾](cleanup-receipt.json)、[日常自检](daily-restored-selfcheck.json)、[日常健康](daily-restored-health.json)。日常原 Qwen/聊天09仅环境恢复，本轮测试没有改用Qwen。

[manifest.json](manifest.json)包含原文件字节数及 SHA256；不归档数据库、密钥或未经筛选的服务日志。源码/载荷与报告HEAD分别记录，当前报告归属草稿 [PR #80](https://github.com/coolzhulike/coolzhuagent/pull/80)，未转正式评审，未宣布全量验收完成。

## 日常启动舞剑补拍

原0帧收据不覆盖。正常前台正式launcher启动，选中并激活返回的独立“COOLZHU”演出窗口，仅观察采集，不输入跳过、不修改时长、素材或系统减少动态设置。[采集记录](startup-front-capture.json)共5帧：帧0为初始背景、帧1合卷、帧2展开、[帧3腾跃出剑](startup-front-3.jpg)、[帧4回身姿态及完整Logo](startup-front-4.jpg)，可见整人姿态变化。[宿主生命周期](startup-front-lifecycle.json)daily/reduced_motion=false、finished reason=completed、page_elapsed_ms=6972、frame_count=1476，之后console_visible=true，并有[实际聊天室移交图](startup-front-handoff.jpg)及[最终launcher自检](daily-final-selfcheck.json)。宿主帧计数不是录像帧数，未称逐帧流畅度或所有姿态均已实拍。

此项日常基本流程验收通过。首次模式、减少动态、素材失败、多屏/DPI仍待验收。中途未激活的采集误拍到被遮挡窗口下的其它应用，仅存临时目录，不归档、不作为启动成果。
