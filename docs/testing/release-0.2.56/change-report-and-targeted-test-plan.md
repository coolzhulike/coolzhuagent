# 0.2.56 改动报告与真实软件回归

2026-10-01，主会话独立实施和测试。使用既有真实 qwen3.8-flash、medium、原百炼 Base URL 和 API_KEY；无模型夹具，无人工代模型点击或绘画。微信不改不测，Devin 搁置，Pro 补审按用户决定暂缓。

## 发布及安装身份

正常 MSI 安装退出码 0，唯一注册版本 0.2.56，10 项关键安装产物均匹配发布摘要；正式 Shell PID 4344、Web PID 21076，8765 唯一监听为正式 Web。MSI 247033213 字节，SHA256 `53afb24d66a310df703384db7186e413405568903a87dc47a3413e6400d4302f`。报告 `pkg-report-release-20261001-160441263-56ac56ca`；源快照 `da96d31e00d4012e49c7af5ee4a7a7e2bcae89c3df4f58027cebe04e9c56fcfe`；载荷 `ef5f0277c795f386d512338aae8cfc8761d421acfec3e8ca4383da40b74c4c54`，859 文件、324538854 字节。6 项发布门通过，安全扫描 0 项发现。原始身份记录在 evidence/build-identity，安装核验及退出码在 installed-native。

离线 Web build 通过；完整工程检查 1296 通过、0 失败、2 项既有忽略，另 lib 8、宿主接线 1 通过。工程检查不替代软件和真实模型验收。

## 最终修改及职责

1. 原生浏览器适配器保留刚关闭的地址及工程/聊天室键，用户明确在同一上下文重开时重新导航创建新世代；跨上下文不加载原页面。关闭仍销毁视图及旧资格，不恢复旧任务、节点或输入票据。实现见 native_browser_panel.js。
2. 用户明确写出单个完整 computer_use_perform 参数时，从原始本轮消息冻结契约，经 PreparedChatDispatch、父运行传给执行器。模型提交与本轮契约不符，观察和输入前拒绝，错误 current_turn_computer_use_request_mismatch、retry_owner=none，不改参数、不补发。边界在既有验证条件归一化之前检查。
3. 契约只覆盖明确完整参数对象；普通自然语言和多个比较示例维持原路径。不能外推为解决所有动作次数或规划问题。设计、风险和模块责任见两份 2026-10-01 专项审查文档。

## 真实回归结果

| 场景 | 实际结果 | 证据 |
|---|---|---|
| 同聊天室浏览器关闭重开 | 无需再次按打开/刷新，自动显示原网页、次数 0 | 01-browser-before-reopen.jpg、02-browser-auto-reopened.jpg |
| BU056-CURRENT-BW 本轮单击 | 同一保留历史的验收聊天室，新参数正确；真实点击 1 次，次数 0→1，两个成功条件均通过；父 completed、CU succeeded | BU056-CURRENT-BW-facts.json、03-bw-single-click-completed.jpg |
| 跨聊天室重开 | 切到主聊天室后打开浏览器，原网页未加载；URL 输入及旧状态文字仍保留，属于显示遗留，不等于跨房间页面执行 | 10-browser-other-room-no-old-page.jpg |
| CU056-PAINT-BX 五点闭合笔画 | 1 次真实 drag，1350ms、5 点、sent/released、partial=false、path_completed=true；原图实际四条边闭合，CU succeeded、goal=true | CU056-PAINT-BX-facts.json、07-paint-bx-native-verification.png、06-paint-fresh-desktop-after-bx.png |
| 全 Windows 使用提示 | BX 运行中全桌面四边玉石泛光及准确顶部英文可见；结束后 activity=false，独立全屏图无提示 | 08-paint-bx-desktop-active.png、paint-bx-activity-during.json、activity-after-bx.json、06-paint-fresh-desktop-after-bx.png |
| 安装版日常开机动画 | 同步采样实际展卷、Q版出剑及回身姿态、人物退场、Logo/朱印停留、控制台交接；日常时间线结束 page_elapsed=6876ms | startup-daily/frame-01..08.png、startup-lifecycle.json、original-image-index.json |
| CU056-PAINT-BY 简化海绵宝宝 | 两条短笔画实际发出、路径完成并释放；眼睛/笑嘴/两腿未达成，后续规划共享预算余量 10180ms 到期，CU blocked、goal=false；未续发 | CU056-PAINT-BY-facts.json、09-paint-by-desktop-after.png |

BW 耗时 40.328 秒，BX 耗时 74.458 秒，BY 耗时 138.723 秒（父轮包含首请求及工具结束后的回复）。BY 的 CU 共用 120 秒预算，没有因规划重置；终态 action_count=2，与 sent 的两个步骤一致。运行中的该列尚未回填，不能将中途 0 当零输入。BY 说明本例模型选点/规划和现有任务预算未达目标，不说明没有绘图接口，也不能仅增加超时就宣称绘图完成。

## 日常开机动画实拍范围

正常从已安装 COOLZHU-AGENT.exe 启动；只读全桌面采样与一次双击同时开始，避免先等待采样工具返回而错过演出。共采集 24 张，前 8 张保留原始字节；01 闭卷，02 展开山水，03/04 Q版出剑，05 回身姿态，06/07 人物已退场且 Logo/朱印停留，08 正式控制台。非动画预览页、非源码资源拼图，未修改启动 storage 强制首次。图像采样约每 0.9 秒，不能据此声称四个姿态每帧均已审阅；首次、减少动态效果、资源失败仍需各自实操。此前 Sky 四张截图未覆盖人物，仍保留，不能用其遗漏证明安装版没有人物。

第三次正常启动 Shell PID18636。宿主记录 daily/reduced_motion=false，assets_ready、first_frame、completed 和 console_visible=true；page_elapsed=6876ms、host_elapsed=7228ms。日志仅作时序旁证，实际画面以原始全桌面截图为准。重启后运行身份另存 post-startup-installed-artifacts.json，不覆盖最初安装核验。

## 截图差异与结论修正

Sky 窗口截图 05-paint-after-bx.jpg 只显示横线，一度据此作出“矩形失败”的判断。后续核验发现：模型收到的原生验收图、绘制后的原生图，以及本轮重新发起 /api/capture 得到的独立全桌面图，均清楚显示完整矩形。没有再次输入或修画，仍保留矛盾窗口截图；以新独立全桌面采样和原始验收图确认 BX 闭合通过。未把窗口截图差异强行归因为执行器故障，亦未修改视觉验收或放宽标准。原图复制、摘要及字节数见 paint-bx-original-evidence-index.json。

## 接手模型的针对性测试设计

- 同一上下文初次打开、关闭、重开必须无需额外导航；跨工程/聊天室应无旧网页、旧节点和可执行资格。URL/状态展示也应避免误导；本版只完成跨聊天室无页面实操。
- 保留旧多次点击历史后，提供本轮单次完整契约；核对模型提交、实际次数、释放、终态及 usage。若提交旧目标，应在观察前零输入拒绝。普通自然语言不在完整等值契约覆盖范围，需另设计测试。
- 同一真实 Paint 窗口进行一次多点闭合拖动；核原图、5 个确认点、路径完成和释放，不以 released 代图形达成。截图矛盾时先用独立全桌面采样核实，禁止手工补画后认领模型成功。
- 多笔画绘制分别核规划点、实际图、每步费用与预算；预算到期和迟到模型响应不得产生新动作。BY 失败记录保留，可作定向诊断基线；不能因父回复正常 completed 就把 CU blocked 说成完成。
- 全屏提示应只覆盖真实有输入资格的 CU，运行时主屏准确英文和四边可见，终止后撤除；只读不亮、多屏、进行中取消须各自实操覆盖，不能外推本次单屏成功。

仍开放：交付期间浏览器关闭（054 只覆盖步骤间）、跨工程展示、简易海绵宝宝目标、首次启动/减少动态效果/资源失败分支及舞剑四姿态逐帧覆盖、DSH 远程插件实际安装运行及四项总体审核。PR #74 维持 Draft。源码和真实证据先归档，不以工程检查追认这些未完成项。
