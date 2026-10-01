# 0.2.50 改动报告与真实回归

2026-10-01，主会话独立执行。固定真实 qwen3.8-flash、medium、原百炼 Base URL 和已保存密钥。测试者只准备页面并发送任务，没有代模型输入目标文字、导航到目标页或点击计数按钮。微信不改不测，Devin 搁置，Pro 复审按用户要求暂停。

## 安装身份及改动

正常 MSI 安装终态为 0，注册唯一版本 0.2.50，正式 Web PID 19216、Shell PID 21764；10 个关键产物与本包匹配，8765 监听归属正确。六个发布门通过，859 份载荷逐项核验，总计 324461542 字节，安全扫描零发现。[安装收据](installed-native/installed-artifacts.json)。

MSI 246988157 字节，SHA256 c554da84a6b03a75f724ad9981222b8f234701e89848ef0787c40af9bdf59859。源码快照 8c3625921cce2b8af8e1aa4bdb48998dcc7326a7a686bb90948fe37fed60c840，载荷摘要 fa5298e8bc956267cbf1060584cfd303cc49b0cbb7c32ee418cfddf1654ffed1。[原始构建记录](evidence/build-identity/pkg-report-release-20261001-131125215-9fe53e1c/package-report.json)。出包时 installed=false 保留为原始时点事实，安装后另存收据。

本包修正原生页面验收：正向证据必须是被引用宿主节点、标题、URL 或视口事实的实际原文；杜撰证据不计满足。交互任务初始观察仅表示待执行，不能零动作成功。纯只读任务仍允许零动作。具体责任边界与限制见[审查文档](../../analysis/2026-10-01-native-browser-verification-grounding-review.md)。

出包前 Web 离线构建通过，完整 Web 1293 通过/0 失败/2 既有忽略，另 lib 8、宿主接线 1 通过。e5efb8364dca0b505812f047268c189c8407f092 的远端 PR 检查 36818382429、push 检查 36818380288 均成功。工程检查不替代以下软件验收。

## 真实模型与软件验收

| 测试 | 实际结果 | 动作与截图证据 |
| --- | --- | --- |
| BU050-TEXT-BA | 通过，50.066 秒，页面实际回显 BU050-BA-竹林 | 2 次动作：click 聚焦，text_input 输入；输入 sent/effect_observed/passed，CU succeeded，1/1。点击后的验收未满足，输入后的新观察满足，未接纳杜撰证据。[账本](installed-native/BU050-TEXT-BA-facts.json)、[同屏通过图](installed-native/02-text-ba-pass.jpg) |
| BU050-NAV-BB | 通过，34.956 秒，实际网址 next.html，正文标记 BU-NEXT-284 | 1 次 navigate，sent/effect_observed/passed，CU succeeded，2/2。[账本](installed-native/BU050-NAV-BB-facts.json)、[同屏通过图](installed-native/03-nav-bb-pass.jpg) |
| BU050-CLICK-BC | 失败，参数校验阻断，计数保持 0 | 模型沿用历史 57159 端口，且 success_criteria 错为字符串，invalid_tool_input，0 动作。[账本](installed-native/BU050-CLICK-BC-facts.json)、[现场图](installed-native/05-click-bc-rejected.jpg) |
| BU050-CLICK-BD | 失败，错误选择旧扩展后端，0 动作 | 当前用户明确说“右栏原生浏览器”，入口未识别此同义词，实际回执 extension_unavailable。聊天错误复述历史 isolated 状态；实际轨迹显示输入已开放，不能以该聊天复述要求再次放行。[账本](installed-native/BU050-CLICK-BD-facts.json)、[现场图](installed-native/06-click-bd-wrong-backend.jpg) |

049 的零动作错误成功仍按失败保留；本包 BA 实际文字输入通过不能改写历史记录。

## 后续修正与针对性测试设计依据

已在源码补齐“右栏原生浏览器”“右侧原生浏览器”两个明确指向内置面板的用户表述，仍不从模型参数、记忆或历史决定执行后端；不把 Chrome、Paint 或设置面板改为内置浏览器。离线 Web 编译与 4 项既有入口边界测试通过，该修正不在 050 已安装包内，下一包复验。

下一包真实模型测试需检查：同义表述选择原生面板；一次点击将计数 0 变为 1；滚动前后真实位置变化；只读标题/标记零动作；取消和关闭后的旧资源不再派发；错误源 URL 拒绝且页面保持。每项保存真实请求、步骤、终态及软件截图，模型复述与事实不一致时按事实判定。

Browser Use 尚未整体验收，Paint 闭合笔画/简易海绵宝宝与全桌面使用提示、DSH 远程插件实际安装运行、四项总体审核仍开放，PR74 保持 Draft。
