# 上传居中修正与 Browser 长程排障

日期：2026-10-08。当前为源码候选，非正式安装验收。

## 上传按钮

上传 SVG 改为围绕 x=12 对称的单色路径，使用统一细金线笔画；上传控件单独固定24px渲染尺寸，避免奇数缩放造成左右竖线像素相位不同。其它按钮设计保持不变。`cargo build -p coolzhu-web-console --offline` 通过，配套候选窗口已重新启动。

实际窗口证据：[上传按钮及运行结果](upload-centered-browser-blocked.jpg)。截图底部左侧上传图标已居中；界面其它控件仍为统一玉石细金线设计。正式打包安装与高DPI复核仍待完成。

## 真实 SWE-2 长程：首次重新执行未通过

- 聊天室：`room-1791131523339`；会话：`session-1791131217833`；生效模型：`swe-2-medium`；远端：`island-kayak`。
- 任务：两层旋转/斜切跨来源子页面，读取校验码→输入/Enter→子页面滚动→正常翻页读取回执→父页面提交。仅一次 `computer_use_perform`，动作上限16。
- 任务标记：`BU-CANDIDATE-AFFINE-LONGRUN-20261008`；父运行：`run-chat-4dad4885be42a9fa4a19af3cd4c59fd2226471b7c9fac546`。
- 工具已进入观察与规划请求，但没有动作投递：attempts=0、steps_completed=0，页面可信事件为空。
- 最终父运行失败。聊天提示为“Devin 在 CU 任务尚未返回最终回执时结束本轮”；CU future 被丢弃后记录 cancelled。该 cancelled 是收尾事实，不能据此推断用户点击了停止。
- 当前模型输出表明它遇到规划快照溢出文件并尝试读取；此 ACP 通道仅开放宿主工具，原生文件读取被拒绝。现有规划回执直接携带全部文本，没有专用分块读取接口，复杂快照可能触发第三方客户端截断/落文件行为。

## 修补方案

在原工具桥增加 `computer_use_read_request(job_id, request_id, offset)`。大规划回执只返回分页指引，每页最多6000原始UTF-8字节，按字符边界分割，保留完整内容。必须顺序读完才能提交规划回复；重复读取已读页允许，跨请求、越过未读内容、已终止任务和迟到读取均拒绝。

接口只读当前轮次内存快照，不接受路径、不使用原生文件工具、不创建新云端会话、不改变权限及动作校验。仅记录请求ID/偏移等诊断事实，不在前端显示调试数据。小快照和原图续接仍保持原协议。

最终候选 `cargo build -p coolzhu-web-console --offline` 通过；快照协议3项定向回归通过，覆盖大Unicode快照无损拼接、未读完拒绝回复、跨请求与取消后迟到隔离、原图续接保持，以及小快照原行为。这些协议回归不替代真实模型验收。

首次分页候选任务 `BU-PAGED-AFFINE-LONGRUN-20261008` 在观察前报 `extension_unavailable`，动作0。排查确认请求只写“当前右栏”，未命中当前基于本轮文字的原生后端选择，因此误走扩展桥；不是分页接口执行失败的证据。已明确“右栏原生浏览器”发送 `BU-NATIVE-PAGED-AFFINE-20261008`，继续原 SWE-2-medium 和唯一 island-kayak。

新增缺口：原生/扩展后端选择依赖固定自然语言短语，须收敛为当前界面目标的明确选择及冻结上下文，保留显式外部浏览器选择，不能让模型猜测后端或用历史目标补足。

当前不宣称复杂仿射页面或整轮长程通过。

## 分页真实通过、离屏提示缺口与修补

`BU-NATIVE-PAGED-AFFINE-20261008` 已由原 SWE-2-medium 使用同轮快照分页连续规划；父运行 `run-chat-21136b542c6bcd6504d0324d312e05d8b61481c7f1304837` 最终失败。5次动作尝试中，点击输入框、文本输入、Enter与子文档滚动4次实际投递，页面可信事件确认校验成功和scrollY=119.33；第5次点击仍离屏的继续按钮被 `native_browser_target_outside_viewport` 拒绝，`input_delivery=not_sent`。见[软件终态截图](affine-outside-viewport-blocked.jpg)。不将局部成功记为整轮通过。

诊断中候选可见2、离屏0；代码确认跨进程子文档此前一律返回未知可见性。观察层现复用输入预检的子视口及逐层owner坐标/只读命中换算，在500ms总预算内给出true/false，超时或无法证明仍保留未知。没有自动滚动或代模型选择动作，输入前检查不变。桌面壳offline build通过；15项现有原生浏览器定向回归通过，真实长程继续验证。

## 子文档滚动进展遗漏

`BU-VISIBILITY-AFFINE-LONGRUN-20261008` 的5个动作全部投递：click/type/Enter和两次child scroll。页面可信事件确认scrollY从0→119.33→478，SWE也正确保持离屏按钮不点击；宿主仍将两次滚动的effect记为inconclusive，最终 `no_progress` 终止。见[实机终态](affine-scroll-no-progress.jpg)。父运行 `run-chat-78f540c13c39379b3503be679bff63be94486f6685a050ff`，原远端island-kayak正常end_turn、进程drained、绑定释放。

原因是观察只携带顶层viewport，进展比较只看nodes/focus/顶层viewport/url/title。正补充CDP target文档根的真实独立视口，在同文档中比较稳定索引对应的滚动位置；缺失采样或随机引用变化均不作为进展，最终成功仍由目标文字及新鲜观察单独核验。LayoutMetrics不属于同进程嵌套文档时不冒用其父视口。

前后端offline build均通过，9项验证回归（包括子文档移动、缺失采样及随机引用不冒领进展）与15项原生浏览器回归通过。协议crate无独立测试，编译通过，不计作额外功能验收。修补后的真实重验 `BU-SCROLL-PROGRESS-AFFINE-20261008` 已启动，父运行 `run-chat-7fc7a5fad316a42f3cb26eeee061a1fe3540057d38235189`，仍沿用SWE-2-medium及唯一island-kayak，等待最终回执，不预先标通过。

## 当前可见原生面板的后端偏好

聊天提交新增可选 `native_browser_panel`：只在原生浏览器面板当前可见时发送true；旧客户端、定时任务及其它入口默认false。服务端在接纳时冻结真实面板绑定，界面字段不构成宿主身份凭证或输入授权。自然语言未包含固定关键词的Browser任务可沿用当前面板，Desktop任务仍可走桌面；本轮正文明确的只读限制及显式原生边界继续生效。没有新增前端配置或调试文字。

`node --check src/app.js`及web-console offline build通过；9项本轮范围解析回归通过。仍待后端候选配套重启后的真实SWE验证，不能将协议测试记为实机通过。弱网去重仍保守按原指令内容去重，不因切换面板而补发同一任务。

## 修补后复杂仿射长程：源码候选通过

`BU-SCROLL-PROGRESS-AFFINE-20261008` 父运行 `run-chat-7fc7a5fad316a42f3cb26eeee061a1fe3540057d38235189` 已completed，CU返回 `succeeded / goal_achieved=true / attempts=9 / steps_completed=9`，无重规划及无进展计数。原SWE-2-medium和island-kayak正常end_turn，进程drained=1、远端锁释放、internal仍未绑定，没有创建第二个云会话。

真实9步为最内层输入框点击→输入当轮校验码→Enter验证→两次子页面滚动（0→598→686，均effect_observed）→点击获取回执正常翻页→点击父输入框→填入从回执页读取的BAMBOO-1676→点击完成整轮。可信事件确认验证和最终提交accepted=true，最终新鲜宿主观察generation10同时确认父页面“整轮任务完成，回执正确”和子页面“验证与翻页完成”。没有脚本代模型输入、重试或补发。

[成功软件截图](affine-page-completed.jpg)、[正常翻页中间截图](affine-navigation-progress.jpg)、[脱敏结构化事实](affine-longrun-result.json)。该通过仅覆盖源码2912e22配套候选中的旋转/斜切两层跨来源页面；不覆盖透视/遮挡负例、严格按下期间变化、新面板路由修补或正式安装版。

## 透视与父覆盖层：源码候选负例通过

在原真实SWE-2-medium及唯一island-kayak中分别发送 `BU-PERSPECTIVE-PANEL-CONTEXT-20261008`、`BU-COVER-PANEL-CONTEXT-20261008`。两轮各有1次点击规划、0次输入投递、0次页面可信输入事件，分别在预检返回 `native_browser_frame_transform_unsupported` 和 `native_browser_target_hit_mismatch`，均not_sent/not_needed、无重试补发，正常end_turn/drained并释放远端锁。CU任务自身为blocked/goal_achieved=false，负例验收按预期拒绝通过，不能混称任务成功。

这两轮正文使用“当前右栏”，不含原生路由固定词语，真实进入原生面板，验证提交时可见面板偏好修补有效。没有使用历史页面或扩展桥。

[透视实机截图](perspective-rejected.jpg)、[透视脱敏事实](perspective-result.json)、[父覆盖层实机截图](cover-rejected.jpg)、[父覆盖层脱敏事实](cover-result.json)。仍未打包安装，不替代正式安装版负例或严格按下期间的验收。
