# 105正式Browser观察失效与最终输入回执缺口

正式105、原SWE-2-medium及唯一island-kayak。一次perform、一次真实点击，收到网页click之后才正常点击地址栏打开按钮；不是严格按下期间导航试验。

独立步骤记录为sent/released，旧网页可信pointerup后约5073.4ms新文档loaded，新页输入0。最终CU blocked/native_browser_observation_stale，父run failed；单次ACP end_turn/process_drained=1，唯一绑定解锁。旧观察失效、零补发通过；native_browser_resource_changed观察入口分支没有触发，不计该分支通过。

模型最终回复承认已投递，但称终态回执未给released字段并错误声称导航发生于按下处理期间。终态JSON确实只携带步骤计数与evidence，缺逐步释放字段；时间顺序以独立网页事件及UI动作时间为准，不能采信模型转述。

源码候选在ComputerUseResult增加兼容旧JSON的可选input_steps，由原执行器从同一SQLite运行的既有步骤记录在终态保存前投影。字段仅含步骤索引、动作类型、投递、释放、partial、效果和验收；不带输入正文、节点、网址、图片或未证明的时序。不新增账本/数据库列/权限检查/重放；读取失败为None，已读取且无步骤才是空数组。取消及持久化错误保留已有投影。此修补尚未包含105，待后续正常出包安装及真实复验。

原始[结果](result.json)、[网页事件](events.jsonl)、[正常UI动作时间](ui-navigation-times.json)与[正式实拍](final-reply.jpg)共同交接。思考未读取/保存，SSE只计数。此前101未命中严格窗口事实仍保持；本轮不替代newTarget/跨来源commit严格时序验收。

源码候选回归：第一轮完整控制台1421通过、2失败、6既有忽略；两项失败均暴露提前返回仍返回旧值、落库已携带input_steps的不一致。改为返回原finish_at_version的实际结果，未删断言或改变安全决策。修正后完整控制台1423通过/0失败/6既有忽略，核心143通过/0失败，专项1通过；[摘要](candidate-regression.json)保留初次失败。仍待106正常安装及真实SWE回执复验。
