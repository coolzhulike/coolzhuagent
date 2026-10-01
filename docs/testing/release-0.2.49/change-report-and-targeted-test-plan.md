# 0.2.49 改动报告与真实回归

2026-10-01，主会话独立执行。固定真实 qwen3.8-flash、medium、原百炼 Base URL 和已保存密钥；无模型响应夹具实操，微信不改不测、Devin 搁置，Pro 复审暂停。

## 安装身份与本包改动

正常 MSI 安装日志终态为 0，注册唯一版本 0.2.49。10 个关键产物与本包一致，正式 Web PID 19880 监听 8765，Shell PID 2268。安装身份见 [installed-artifacts.json](installed-native/installed-artifacts.json)。六个发布门全部通过；859 份载荷逐项核验，共 324215782 字节，安全扫描零发现。

源码快照 6066822868c0dd27baa7ffca88fa0ae763fd93efaa31b960f8e125c5583ec8a1；载荷摘要 257e99bb1a51deb7e4dbdd0d25e662746f94cb0846bdea0b626231bde9b7e5a8；MSI 246963581 字节，SHA256 5570100ba791afb9ff0b2a815912e1337fb7a5ab93908403403ae9960e1aeb14。原始记录见 [构建报告](evidence/build-identity/pkg-report-release-20261001-125456147-fcd289e6/package-report.json)。原始 staged-verification 的 installed=false 是出包时事实，安装核验另存，不回写原收据。

本包投影宿主确认的普通编辑节点 focused 布尔状态，步骤反馈附已有白名单 action_kind。没有传递原值、选区或整个 AX properties，也没有自动聚焦、自动填写或放宽执行守卫。

出包前 Web 和 Shell 离线构建通过，Web 1292 通过/0 失败/2 既有忽略，另 lib 8、宿主接线 1 通过；Shell 64 通过。e624462 的 PR 检查 36816989284、push 检查 36816984161 均完成成功。工程结果不能替代软件验收。

## BU049-AZ：失败，工具错误宣称成功

测试者只打开普通静态 HTML 初始空白页并发送任务，没有操作目标输入框。真实父运行 run-chat-d205ce10923b74329d0d7b719ac57b867fe29ebb7ca10c48，turn chat-turn-1790830813339-0；31.5 秒终态 completed。

工具请求包含正确的输入目标 BU049-AZ-竹林。CU 执行 id cu-session-1779459149988-000000000000000218da4f22fd8dde68-tool-46dab90e071e361598afddab687ce167aa7cd54d71b372a5b2fa474885278165：零动作、零规划、零步骤，仅一次初始页面验收请求。模型 met=true，工具随之返回 succeeded/goal=true/1/1，但回执引用的真实节点和软件截图都仍为“当前输入：空”。

结论：输入验收失败，同时发现成功判定缺陷。证据：[原始请求与步骤账本](installed-native/BU049-TEXT-AZ-facts.json)、[测试前空白页](installed-native/01-text-az-before.jpg)、[错误成功回执与实际空白页同屏](installed-native/02-text-az-false-success.jpg)。不把终态 completed、工具 succeeded 或聊天说明当成真实输入通过。

## 后续修正与测试设计依据

修正在 [证据核对审查](../../analysis/2026-10-01-native-browser-verification-grounding-review.md)。正向 evidence 必须引用实际宿主节点、标题、URL 或视口原文；不匹配按未满足处理。原生交互初始观察不能替代本轮实际操作，只读任务仍可零动作完成。源码修正不在已安装 049 包中，需下一包重新实测。

针对性功能测试依次检查：

1. 初始空框，由真实 Qwen 选择 click，再基于新焦点选择 text_input；回显必须为本轮唯一文本，步骤账本需确有 text_input 和实际派发结果。
2. 节点索引合法但证据杜撰时不得成功；同页内容或宿主变更时旧验收仍被新鲜度守卫拒绝。
3. 原生只读标题/页面事实保持零动作能力；明确 URL 导航、点击、滚动在新包各自复验。
4. 浏览器取消、关闭、源地址错误不派发后续动作；结束撤除使用电脑提示。
5. 输入闭环后继续 Paint 闭合笔画、简易海绵宝宝和全桌面泛光提示联合截图。

Browser Use 输入和新包资源变化回归、Paint、DSH 远程插件实际安装运行、四项整体审核仍未通过，PR74 保持 Draft。048 导航通过与042点击、045滚动/只读等历史结果仅代表各自版本。
