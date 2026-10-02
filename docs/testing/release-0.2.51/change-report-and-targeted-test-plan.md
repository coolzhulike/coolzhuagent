# 0.2.51 改动报告与真实 Browser Use 回归

2026-10-01，主会话独立执行。原 qwen3.8-flash、medium、百炼 Base URL 和已保存密钥不变。测试者只准备初始页面、发送任务，以及在取消/关闭场景操作对应控制按钮，没有代模型点击计数按钮、输入目标文字或绘画。

## 安装身份和本包修正

正常 MSI 安装返回 0，注册唯一版本 0.2.51。正式 Web PID 20296、Shell PID 29356，8765 监听者匹配；10 个关键产物与本包一致。六个发布门、859 份载荷逐项核验和安全扫描通过。[安装收据](installed-native/installed-artifacts.json)。第一次安装路径格式错误，尚未进入安装阶段；改为 Windows 路径后正常安装成功，不把第一次失败算安装通过。

MSI 247008637 字节，SHA256 `9b3a446a3223b9ef5e9c2929e1d18bd033debb28d23e1ee462df58dc232b2387`。源码快照 `688701e023b7279525f80c93306369c446f9ef93b3a580dfc58110d8ae050505`，载荷摘要 `fcf7a5a03bd3f709650405d437eb5e7a5093adeca69fb297bbb0a85747047076`。[原始构建记录](evidence/build-identity/pkg-report-release-20261001-133708784-f320a2db/package-report.json)。出包时的 installed=false 保留为时点事实。

本包补齐当前用户表述“右栏原生浏览器”“右侧原生浏览器”的选路，不从历史、记忆或模型参数改选后端，不扩大桌面或其它侧栏权限。b52ff1b 的远端 PR 检查 36820381814、push 检查 36820377498 均成功。

## 实际结果

| 编号 | 结果 | 真实软件与账本证据 |
| --- | --- | --- |
| BE 点击 | 通过，40.370 秒，次数 0→1 | 原生后端，恰好 1 次 click，sent/released/effect_observed/passed，CU succeeded，2/2。[截图](installed-native/02-click-be-pass.jpg)、[账本](installed-native/BU051-CLICK-BE-facts.json) |
| BF 滚动 | 通过，34.733 秒，实际 page_y 0→289.3333435058594 | 恰好 1 次 scroll，页面回显 289 CSS 像素，源地址不变，CU succeeded，2/2。[截图](installed-native/04-scroll-bf-pass.jpg)、[账本](installed-native/BU051-SCROLL-BF-facts.json) |
| BG 只读 | 失败，20.250 秒，0 动作 | 用户“禁止点击/输入/滚动”未识别为只读；规划器 done，初始交互观察不计成功。[截图](installed-native/05-read-bg-scope-failure.jpg)、[账本](installed-native/BU051-READ-BG-facts.json) |
| BH 只读 | 失败，30.236 秒，0 动作，1/2 | 只读路由正确，宿主读到标题及拆成三节点的滚动文本；正向证据中一项被原文核对拒绝。不能按肉眼读到内容改写工具终态。[截图](installed-native/06-read-bh-split-evidence-rejected.jpg)、[账本](installed-native/BU051-READ-BH-facts.json) |
| BI 只读/取消准备 | 只读通过，28.990 秒，零动作；取消未触发 | 工具完成早于中止操作准备；不能计取消通过。[截图](installed-native/07-read-bi-pass-cancel-not-triggered.jpg)、[账本](installed-native/BU051-CANCEL-BI-facts.json) |
| BJ 中止 | 父轮中断；未覆盖工具验收期间取消 | 停止请求晚于只读工具成功，父 interrupted；已完成的 CU succeeded 原记录保留。[截图](installed-native/09-cancel-bj-parent-stopped.jpg)、[账本](installed-native/BU051-CANCEL-BJ-facts.json) |
| BK 中止准备 | 未触发目标场景 | 工具和父轮均先完成，UI 已恢复发送状态，不能计取消通过。[现场](installed-native/10-cancel-bk-during.jpg)、[账本](installed-native/BU051-CANCEL-BK-facts.json) |
| BL 输入预检期间中止 | 停止行为通过，21.441 秒 | 父 interrupted；待派发 click 返回 native_input_cancelled/not_sent，CU blocked、goal=false、0 动作，停止后计数仍 0。此处不声称 CU 状态是 cancelled，也不声称完成 10 次点击。[稳定现场](installed-native/13-cancel-bl-no-late-action.jpg)、[账本](installed-native/BU051-CANCEL-BL-facts.json) |
| BM 关闭准备 | 失败，23.464 秒；关闭失效场景未覆盖 | 模型 objective 使用“每一步只发送一次普通点击”，宿主将“发送”误判为 external_communication，在关闭之前已 approval_required、0 动作。不能计关闭保护通过。[现场](installed-native/15-close-bm-approval-failure.jpg)、[账本](installed-native/BU051-CLOSE-BM-facts.json) |

050 的真实文字输入、导航通过继续有效，分别见 [050 报告](../release-0.2.50/change-report-and-targeted-test-plan.md)。不能将历史失败覆写成新轮成功。

## 本轮新修正和下一包针对性测试

新源码尚不在 051 包内：补齐只读禁止输入的斜线写法及明确“不作任何输入”表述；允许连续文本节点原文引用；分开意图里的机械输入投递措辞与真正外发意图。安全边界和方案风险见[专项审查](../../analysis/2026-10-01-browser-natural-language-boundary-review.md)。新包必须正常安装后用同一真实 Qwen 复验，不以源码检查代替正式安装验收。

针对性测试需覆盖：斜线只读请求 input_supported=false、0 动作；拆分的真实文本可引用、改写值仍拒绝；“发送一次普通点击”的普通本地点击不误触审批，真正发送控件/外发宾语仍保留审批；执行中关闭后不再派发旧节点；错误源 URL 不操作当前页面。每轮保存时间、父运行、实际请求、CU 终态、动作回执和软件截图。

Browser Use 整体验收、Paint 闭合笔画/简易海绵宝宝和四周泛光及准确顶层提示、DSH 远程插件实际安装运行、四项总体审核仍开放。PR74 保持 Draft；微信不动不测、Devin 搁置、Pro 审核按用户要求暂缓。
