# 内置浏览器明确名称识别修补与实操记录

本记录是0.2.86发布后的源码候选验证，不属于0.2.86安装包。Paint按用户最新要求停止复测；主会话独立实施，未使用子代理、Qwen或受限Opus，也未创建新Devin云端会话。

## 问题与修改

用户明确说“当前右栏内置原生浏览器”时，原有连续短语表没有“内置原生浏览器”，导致错误选择外部扩展路径，报extension_unavailable且零输入。该问题在086正式测试FOCUS2/3中暴露，是Agent路由缺口。

仅在`computer_use_turn_scope.rs`的当前用户短语表增加该名称；复用既有表驱动验证同义句及只读限制。没有从历史、记忆、模型参数猜测目标，也没有扩大桌面权限或改变输入安全流程。

## 真实模型实操

用例`BU-CANDIDATE-087-NATIVE-ALIAS-20261007`通过正常聊天室发起，目标表述只使用此前失败的“当前右栏内置原生浏览器”。跨来源独立进程子页面实际完成一次click、一次输入ALIAS087、一次Enter；三条输入均sent/released。最新页面子输入/提交1/1，父输入/提交0/0，三项原文grounded，外层completed，聊天室显示49.7秒。

原聊天室`room-1791131523339`、Agent `session-1791131217833`、唯一远端`island-kayak`保持。单ACP attempt `11ced4e054ee81fa42be071d83ae928b6a8d923396ade548`，end_turn/drained；内部lane远端为空，两lane锁清。六次规划/验收请求均有对应响应。可信网页事件四条，包含输入及提交，不以聊天自述替代事件和动作回执。

实操图：[操作前](evidence/native-alias-before.jpg)、[操作后](evidence/native-alias-after.jpg)。原始[动作与网页事实](evidence/native-alias-facts.json)、[单会话事实](evidence/single-session-facts.json)、[实际进程与二进制摘要](evidence/candidate-processes.json)可供其他模型设计针对性测试。

## 代码检查与限制

实际offline build通过。完整主控制台检查在输入任务结束后为1396通过、0失败、6项既有忽略，另lib8及native-host1通过。首次全量检查与真实输入任务并行，协调锁Busy使一项启动隔离测试失败；原失败日志保留，待任务排空后再完整运行通过，没有修改生产协调锁或测试来掩盖。见[首次日志](evidence/web-tests-all.log)、[空闲完整结果](evidence/web-tests-all-idle.log)。

此轮不扩大到任意HTML、全部复杂变换、多屏或四项总体完成；正式安装版身份仍以[086交接](../release-0.2.86/change-report-and-test-handoff.md)为准。插件、升级和启动演出其余验收在[当前队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)继续跟踪。
