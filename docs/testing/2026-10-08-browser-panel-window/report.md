# 原生面板按下期间关闭：真实候选时序

仅第三轮命中严格时序。前两轮完整保留，不因CU或模型自述成功而计作关闭验收通过。三轮均沿用原SWE-2-medium与唯一island-kayak，各单次perform、一次点击，无重试补发，远端正常end_turn/drained、绑定解锁。

## 第三轮：边界通过，业务目标未完成

真实受控网页的pointerdown处理持续1800ms，期间通过已观察的正常“关闭当前工具”按钮关闭右栏；未使用产品测试钩子、伪造输入或修改安全记录。

- 可信pointerdown：1791478590035.2ms。
- 正常UI关闭操作开始/返回：1791478590242–1791478590337ms，区间严格位于真实按下及释放之间；[操作收据](close3-ui-interval.json)、[当时软件截图](03-close-third-round.jpg)。该区间是UI操作时间，不冒称原生内部invalidate的精确时间。
- 可信pointerup：1791478591837.6ms；原页click：1791478591838.3ms；pagehide：1791478591844.8ms，视图实际销毁在释放之后。
- 宿主步骤sent/released，随后观察返回native_browser_panel_unavailable，CU blocked/goal_achieved=false，父run failed；没有未知释放、自动重开或补发。这是关闭后安全停止的预期负例，不称业务点击目标成功。
- [第三轮结果](third-close-during-press.json)记录真实事件、宿主动作与远端收尾；完整状态与截图交叉验证。证明范围为正常UI关闭请求发生在按下期间、原动作释放后资源结束，未直接观测Windows底层内部销毁调用的时间。

## 前两轮未计通过

1. [第一轮](first-no-close.json)：等待窗口结束后才发生点击，测试端未关闭。CU正常点击成功；模型却把提示中的预期关闭当成既成事实。[实拍](01-close-window-not-hit.jpg)显示右栏仍打开，不能据此宣称关闭通过。
2. [第二轮](second-close-after-release.json)：正常关闭在1791478408811–1791478408915ms，晚于pointerup约7.1秒。[收据](close-ui-interval.json)、[实拍](02-close-second-round.jpg)。最终验证因面板不可用停止，证明验证阶段关闭，不覆盖按下期间。

## 后续

该项为源码候选；正式0.2.95独立复验仍待完成。替换整个面板及跨来源导航commit恰在按下期间仍开放，不混算。没有为延长窄时序而超过宿主3秒输入回执期限，也没有引入生产测试后门。

[真实网页服务器](server.py)复现时请复制到tmp新目录运行，不能向此证据目录追加事件。
