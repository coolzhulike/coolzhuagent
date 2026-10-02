# 0.2.52 改动报告与真实 Browser Use 回归

2026-10-01，主会话独立执行；原 qwen3.8-flash、medium、百炼 Base URL 与保存的密钥保持。只准备初始页面、投递任务和点击明确的关闭按钮，没有代模型点击目标计数、输入验收内容或绘画。

## 安装与变更

正常 MSI 安装返回 0，注册唯一 0.2.52；正式 Web PID 8700、Shell PID 3372，8765 监听者一致。安装 10 份关键产物逐项匹配，6 个发布门、859 份载荷和安全扫描通过。[安装身份](installed-native/installed-artifacts.json)。出包时的 installed=false 保留，不用安装后事实改写构建时点记录。

MSI 247029117 字节，SHA256 `51b9d28ecef0fd81b7bbb253616f3063a3cd3443b35d64e47ce7143bb71f7dd6`；源码快照 `f2218a608e756dd96fa2ac6a4ac5a1980c52510f370d988ceec4d21ecef2b805`，载荷摘要 `b58c0b867a66ee11f57c83bd49573569173b695bddcd98f90f906621376cb6db`。[原始构建记录](evidence/build-identity/pkg-report-release-20261001-141900435-a39295c6/package-report.json)。8e11cf6 的 PR 检查 36823820618 和 push 检查 36823815559 均成功。

本包修正三处 Agent 边界：只读限制支持斜线分隔及“不作任何输入”；连续、相邻的文本角色可按原文组合引用，不能杜撰数值或跨控件拼接；意图中的完整机械输入宾语“发送一次普通点击”等不误判为外发，真正外发意图和目标控件仍按原审批。未改安全库、许可、预算或自动重试。工程验证 Core 140/0、Web 1293/0/2 既有忽略，另 lib 8 与宿主接线 1 通过。见[边界风险审查](../../analysis/2026-10-01-browser-natural-language-boundary-review.md)。

## 真实结果与失败点

| 编号 | 结果 | 软件与账本事实 |
| --- | --- | --- |
| BN 只读/组合原文 | 通过，26.799 秒 | 只读 input_supported=false、0 动作、2/2；宿主文本索引 12/13/14 实际为“实际页面滚动位置：”/“0”/“ CSS像素”，原文组合接受，ungrounded_positive_count=0。[原图](installed-native/02-read-bn-pass.jpg)、[账本](installed-native/BU052-READ-BN-facts.json) |
| BO 机械点击措辞 | 通过，31.907 秒 | 实际 objective 保留“每一步只发送一次普通点击，不批量点击”；没有误触审批，恰好 1 次 sent/released/effect_observed/passed，页面次数 0→1，2/2。[原图](installed-native/04-click-bo-pass.jpg)、[账本](installed-native/BU052-CLICK-BO-facts.json) |
| BP 执行中关闭 | 未通过，88.855 秒 | 关闭时点 1790836278190，父轮仍运行。前两次点击已释放且实际次数 2；第三步 1790836276317 开始，关闭后以 native_input_outcome_unknown、may_have_been_sent/unknown 结束，没有下一步或同轮重试。实际隔离和待复核提醒出现。不能将第三次说成未派发，不能称目标 10 次完成，也不能把停止后无续发当作释放已确认。[执行中原图](installed-native/06-close-bp-during.jpg)、[失败现场](installed-native/07-close-bp-unknown-release.jpg)、[账本](installed-native/BU052-CLOSE-BP-facts.json) |

BP 数据库动作累计为 2，终态 supervisor 记录 3 次尝试；前两次有明确完成回执，第三次未知。归档保留两者，不统一改成成功输入计数。仅从时长不能确定第三次在领取前超时还是销毁期间丢释放确认；代码审查确认两条生命周期缺口均需修补。独立方案见[关闭与清理审查](../../analysis/2026-10-01-browser-close-input-lifecycle-review.md)。

050 的真实文字输入和导航、051 的真实滚动及输入预检中止仍保留为对应版本的通过证据，不覆盖此前失败。052 新修正的只读和机械点击已复验，Browser Use 整体尚未闭环。

## 下一包针对性测试

正常安装下一包后，用相同 Qwen 在多步本地网页任务执行中关闭面板：已完成动作仍可结算，未交付请求须证明 NotDispatched；已交付动作须保留原释放回执，未知不能自动重放。保存关闭时点与步骤起止，确认没有重新打开或后续旧节点输入。随后错误源 URL 必须零输入拒绝且当前网页不变，再进入 Paint 的闭合矩形/简易海绵宝宝及全桌面四周泛光、准确英文顶层提示、结束撤除的联合验收。

Paint、DSH 远程插件实际安装运行、开机动画与四项总体最终验收仍开放；PR74 保持 Draft。微信不动不测、Devin 搁置、Pro 审核暂停。本轮关闭触发的未知输入未手工改安全数据库。
