# 0.2.108 改动与真实测试交接

当前正式安装0.2.108，冻结源码`c146f31f72f1c83ed57fd6cdde5e2d62fa05e14d`，源码快照`155d81a5ec223ca9758ca5cb072eb5cdedff2a657759a64de777a9f86688e7b1`。正常构建实际子退出0、六门pass、MSI安装0、Program Files内1159文件逐长度/SHA匹配。MSI SHA256 `6badfa009f8a9f4f6a54461e5966835faf0c94f4f98b2119b640a525aa342d18`。未公开发布108，不标latest或签名。构建源码两路远端CI均success，原回执见installed-validation/source-ci.json；后续候选提交另核。

## 实现

已登记的原生观察失败时，在父运行原有轨迹追加browser.observation_stopped，区分等待资源变化、回包校验与宿主错误等固定阶段。先释放pending请求，再写事件；不持登记锁写库，不改变输入时序或重试策略，不记录正文、URL或凭据，不向正式聊天界面增加调试控件。离线build0、完整控制台1423通过/0失败/6既有忽略、关联8项通过。没有新增模型夹具。

## 正式一次真实SWE事实

原SWE-2-medium/revision51/唯一island-kayak，仅一次perform/max_actions1。测试网页通过只读SQLite确认输入sent/released且CU进入observing后，正常导航一次；新网页载入比up晚230.1ms，新页零输入，无补发。最终合法读取新页，目标未满足而budget_exhausted，effect_observed/goalfailed；单end_turn/drained、绑定解锁。网页服务正常退出0，活动模型运行0。

本轮**未命中**底层观察等待资源更换，browser.observation_stopped事件0条，不能把CU observing状态当成底层pending请求已登记。原事实、请求与[正式实拍](observation-phase-unhit/final-reply.jpg)保留；没有改预设为通过。为精确触发后续复验，下一候选应在底层请求登记点补同一request_id的固定轨迹标记，测试据该标记触发正常网页导航，不再按CU高层状态调整延时。

## 剩余验收

waiting_resource_changed实机分支、新原生Target/跨来源commit恰好在down/up之间、其它插件/附件/调度/DPI及架构运维矩阵继续开放。Paint免测、微信不动、Opus暂停，不新建云端会话、不使用子代理。本版没有新增授权或清改安全历史。108源码及当前正式软件不能冒称已含下一候选的请求登记标记。
