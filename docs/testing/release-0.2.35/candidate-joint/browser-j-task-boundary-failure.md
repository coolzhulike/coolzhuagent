# 035候选：浏览器只读请求误续接历史Paint

2026-09-30，真实qwen3.8-flash、原会话与工作区、正常候选启动器。用户提供的[原生右栏截图](07-user-native-browser-visible.png)证明网页实际可见，不能据此声称模型已经观察或操作网页。

J轮本轮原文明确只读当前内置example.com，不点击、不滚动、不输入、不切应用。模型却把上一轮I绘图请求视为同轮待办，调用desktop/Paint；宿主准入没有落实本轮只读边界，实际发送了两次拖动。两个步骤均released，终态blocked/no_progress、goal_achieved=false、verified=0。浏览器观察次数0。后图仅保留此前短线及少量墨迹，没有完整海绵宝宝。

- [J轮动作前Paint原图](05-unexpected-paint-before-browser-j.png)
- [J轮动作后Paint原图](06-unexpected-paint-after-browser-j.png)
- [父轮次、真实步骤、终态与图像来源/哈希](browser-j-task-boundary-failure.json)

定位到两个agent侧缺陷：历史投影过滤含多次失败关键词的assistant回复时，删除了轮次结束边界，前后user请求在模型输入中相邻；此外CU准入只检查模型自填目标及既有权限，没有冻结本轮明确的原生只读限制。这不是单纯Paint绘图质量问题。

后续源码修复：历史失败内容仍不召回，但保留简短结束标记，原始审计不变；系统策略指明末条user才是当前请求。聊天接纳从本轮原文冻结明确的“内置浏览器+禁止输入”组合，CU准入在适配器创建前拒绝desktop及桌面target，写入0输入拒绝事实；合法browser由冻结边界强制原生只读适配，不由模型objective扩大后端权限。普通Paint和允许输入的浏览器请求不被该局部规则重分类。

新修复不在035 MSI中，须后续候选正常构建与真实Qwen复测；单元检查不能替代实际网页观察。原生类型化输入仍未实现，完整Browser Use仍未验收。
