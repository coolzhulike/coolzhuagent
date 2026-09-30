# 0.2.36 当前轮次边界修复及真实回归计划

2026-09-30，主会话独立实现/测试。正式安装仍034；036正常发布链已完成并经原工作区正常启动。035候选G短线及静态泛光通过，J只读网页请求误续接历史Paint。036修复轮次边界；L独立Paint折线发现执行侧拐点丢失，尚未验收通过。

## 问题和实现

J真实请求只读右栏example.com，但模型把历史I绘图作为当前待办，宿主实际发出2次Paint拖动/released，blocked/no_progress，0网页观察。证据见[035真实失败](../release-0.2.35/candidate-joint/browser-j-task-boundary-failure.md)。

1. `main.rs`上下文投影保留简短历史轮次结束标记，替代旧能力误判正文，避免两条user因过滤合并理解。原始回复、轨迹、记忆存储不删，系统策略声明末条user是本轮请求。
2. 新独立`computer_use_turn_scope.rs`仅解析本轮原文明确的“内置浏览器+禁止发送输入”组合。聊天接纳冻结限制到父运行，历史或模型objective不能创建权限。
3. `computer_use_executor.rs`在合法请求校验后、适配器创建前拒绝desktop、桌面target以及无网页target的auto请求，写入intent_guard拒绝/0输入事实。合法browser强制原生只读宿主，全部输入能力仍关闭，不回退Chrome。普通Paint及明确允许输入的网页请求沿原权限链，不由历史文字重分类。

这是局部确定性约束，不能宣称覆盖任意自然语言限制、所有工具或所有网页安全风险。原生类型化输入、目标验证尚未完成，不把只读当完整Browser Use。

## 已完成工程检查

两项先确认真实断言失败，再实施修复：desktop请求曾被准入；旧assistant删除后模型输入只有两条user。新增3项局部回归，无新增模型夹具。离线build退出0；完整Web1274通过/0失败/2忽略（38.68秒），库8通过、静态资源1通过。原Tauri59/0检查未因本轮纯Web修复重跑。7bdb1b6两条远端CI（36701744511/36701739281）成功。不能将工程检查视为软件验收。

正常036发布链6门通过，关键产物10/10、MSI/归档哈希独立匹配，内容扫描safe/0 findings。MSI 246705533字节，SHA256 `972792A0136DE8A7A9ADFEED3E9F9785258494BA1AA1C9F2F15E95BBC65E16F0`；报告 `pkg-report-release-20260930-182338907-2df91118`，源码快照 `a86bbd93bd6de24a822831123e577ae317ec2f475e56a942cbeb085922dbc6f0`，载荷859文件。收据保留原始字节，活动树含继承的029未提交文档，不称干净提交构建。

L新轮真实Qwen只规划当前Paint（没有续接旧头像/网页），三点 `[.449,.542]→[.488,.542]→[.488,.633]`、1500ms。宿主1次drag、3点接纳、path_completed/released，原图却只有斜线，L形未完成；最后视觉JSON无效，blocked/invalid_verification、0 verified。结束后activity=false/lease=0，safe/accepts_new_input=true、未确认阻断0；历史两条已放行记录保留。见[原始截图与失败事实](candidate-joint/paint-l-path-and-verification-failure.md)。新的连续移动修复须037候选验证，036 MSI不含该修复。

## 真实Qwen定向测试

| 编号 | 前置和请求 | 期望证据 | 状态 |
| --- | --- | --- | --- |
| TURN-036-01 | 原房间保留历史I/J，正常候选，当前明确只读内置网页 | 如模型仍发desktop，intent_guard/current_turn_readonly_browser_required，0动作/0输入；不得再续画历史任务 | 待真模型 |
| BU-036-01 | 用户右栏原生example.com可见，真Qwen一次browser只读 | 真实AX资源/工程/聊天室/父运行一致，URL、标题、正文和链接与原始截图一致；不外部抓取替代 | 待真模型 |
| BU-036-02 | 观察期间隐藏/导航或取消 | 旧资源拒绝，无迟到写入和自动重放，既有输入隔离未绕过 | 待实操 |
| CU-036-01 | 新轮明确Paint允许绘图，原真Qwen | 仍可走desktop；实际动作/落笔/释放与原始截图一致，不把短线算头像完成 | L轮走desktop/1次drag并释放；折角丢失及判定JSON失败，未通过 |

所有模型实操使用原qwen3.8-flash/Base URL/API_KEY/medium。微信不改不测，Devin暂缓。DSH远程安装运行与四项总体验收仍开放。
