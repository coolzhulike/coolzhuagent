# 安装版 Paint 阻断与恢复补审

时间：2026-09-27。GPT-6 Pro 会话“整合审查执行计划”，审查轮次 `a33c7019-c14a-4b4f-b229-aa3958d298eb`。

## 已证实事实

Qwen 3.8 Flash 已经发出正式 `computer_use_perform`。安装版在 intent_guard 返回 `input_safety_resource_not_accepting_new_input`，物理输入资源 state=unknown、revision=1；实际输入 0，没有新 CU run。用户实际工作区存在两个待收敛 legacy run。Paint 操作前后均为空白。完整身份与截图见 [安装版实操报告](../testing/release-0.2.17/installed-navigation-and-paint.md)。

这与之前外部 exec_command 的 blocked by policy 不同，也不能归因为模型不会生成工具调用。

## 采纳的恢复边界

- 先查遗留运行身份、已登记的执行者/进程实例/作业/释放事实，再按现有协调锁、epoch 和门禁流程收敛；不能由新的输入请求自行宣布安全。
- 有独立证据才能自动收敛。不能仅凭 PID 不存在、端口关闭、程序新安装或窗口消失认定旧输入已结束。
- 如果旧数据缺足够身份或释放事实，应形成具体的人工复核清单；不能自行填写 operator/reason 冒充用户接受风险。
- 不删除历史、不新建空安全库、不跳过 intent_guard、不用另一个输入通道完成画线冒充项目成功。
- 恢复后仍须重新执行完整 admission、permit 和执行者身份核验。
- UI 应呈现真实原因、遗留条目和恢复下一步；不能只有 unknown 错误，也不能把人工风险接受按钮当自动恢复开关。

## 主会话校正与验收口径

Pro 将此次结果称为有效安全拒绝，这是合理的局部判定；但它建议不记录“Paint 失败”容易掩盖用户目标。本项目报告同时保留两条：**输入门禁按状态拒绝，未发生物理输入；Paint 画线功能验收未通过**。没有实际执行不能判断后续规划、坐标或绘图质量是否合格。

当前仍在只读诊断，尚未实施恢复，更未宣布 Paint 通过。后续决定须绑定两个实际遗留运行的证据，不以此方案文档代替现场检查。

## 作用域与重启证据补充

后续 Pro 审查轮次：`54d22544-5132-49b7-8eef-4ee6660c6cc6`、`30a300fe-f61d-4de8-804c-2d4958ab209b`。完整现场诊断见 [恢复证据清单](../testing/release-0.2.17/installed-input-safety-recovery-diagnosis.md)。

- 当前进程在 Windows Session 2。两个历史 recovery operation 在 Session 1，但原始 CU run 的 workspace/resource scope 未记录，不能把恢复操作的作用域当成原始输入作用域。
- 当前 API 混合呈现当前 scope 的阻断数量和全局 legacy/human-review 数量。这是诊断显示需要改善之处，不能据此将全局历史项直接过滤掉。
- `resource_state` 缺行返回 Unknown 是保守默认行为；直接改成 Idle 不是修复。
- 本机 LastBootUpTime 为 2026-09-25 03:17:05（+08），两个历史 run 的末次记录均早于此。但单个时间字段不等于已核验完整重启代际，也不能补造旧动作成功/失败记录。

Microsoft 官方说明区分快速启动保存内核状态与 Restart 的完整启动周期；SendInput 文档也明确该调用自身不会重置当前按键状态。参考：[Windows 快速启动与重启](https://learn.microsoft.com/en-us/troubleshoot/windows-client/setup-upgrade-and-drivers/fast-startup-causes-system-hibernation-shutdown-fail)、[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)。这些文档不能单独证明本机两个历史 run 已完成恢复。

主会话保留两项边界：历史动作结果不明应继续如实保留，当前执行者/输入释放安全应由独立证据评估，不能混成一项永久无法满足的证明要求；但在现有正式恢复策略下，没有完整证据或真实操作员确认前，代理不执行放行。暂不为此次测试新增一套恢复框架。
