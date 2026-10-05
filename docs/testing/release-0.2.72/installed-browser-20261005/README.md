# 0.2.72 已安装载荷 Browser Use 实操验收（2026-10-05）

## 身份与范围

Windows 安装记录、CLI 与载荷版本均为 0.2.72，Git SHA f3bb374c8e9be93fbe0837b86a730080cd87a679。日常 launcher 启动自检通过（副本 installed-launch-selfcheck.json）。本次实操使用已安装的 Web 与 Tauri 二进制，在既有独立验收工作区运行；不是 launcher 全链启动，所以不验收 launcher 原生恢复控制面。

真实执行模型固定 Devin SWE-2 `swe-2-medium`，会话 session-1791131217833、聊天室 room-1791131523339，既有“完全访问”授权。Opus 两次只读审查使用 `claude-opus-5-5-high`。没有模型夹具，没有清理输入安全库，没有代模型点击目标网页或填写表单。

本地 pages.py 仅提供常规 HTML 和 isTrusted 事件记录，不产生输入。主会话在地址栏准备源页，模型从当前网页自主执行。截图是实际桌面控制台，未编辑或美化。部分 API 发送的测试截图左侧仍显示前一轮回复，右侧是该轮实际目标页；不得把两者当作同一轮联合回执。popup-visible-reply.jpg 与 form-after-failed.jpg 显示对应真实回复。

## 逐项结果

|项目|真实结果|收据与截图|结论|
|---|---|---|---|
|自然跳转|1 次 click sent/released；TARGET-072、目标地址与输入计数 0 可见。内部模型三项 met=true，但宿主最终 budget_exhausted、goal=false；旧终态丢失最后验收明细。|natural-details.json、natural-after-failed.jpg|未通过，不能凭截图追认宿主成功。|
|window.open|1 次 click 后新页在内置浏览器显示；宿主 3/3、goal=true、freshness_confirmed=true，目标页输入 0。|popup-details.json、popup-visible-reply.jpg|通过普通弹出页面路径；不覆盖在途手动替换竞态。|
|SPA pushState|1 次 click；实际地址 spa-target.html，页面 SPA-TARGET-072，宿主 2/2、goal=true。|spa-details.json、spa-after.jpg|通过同文档地址/内容同步。|
|锚点|1 次 click；地址含 #destination，FRAGMENT-TARGET-072 可见，page_y=312.67；宿主 2/2、goal=true。|fragment-details.json、fragment-after.jpg|通过普通锚点导航与自然滚动。|
|历史后退/前进|3 次实际点击。页面轨迹“起点→进入第二阶段→历史后退→进入第二阶段”，第三次重复 push，没有 forward。终态 budget_exhausted、goal=false。|history-details.json、history-after-failed.jpg|未通过。缺少当时候选名称对应证据，暂不归因到模型或适配器映射。|
|文本、滚动与表单|真实前端发任务，click 聚焦、text_input 成功，输入框显示 SWE2-072。第三步直接 click 离屏目标，预检 not_sent、native_browser_target_outside_viewport；无 scroll，未提交。|form-response.json（只读数据库取证）、form-details.json、form-after-failed.jpg|文本输入已实操；滚动/勾选/提交整体未通过。|
|跨源自然跳转|1 次 click 后 localhost 目标页可见，输入计数 0。内部模型三项 met=true；宿主 budget_exhausted、goal=false。|cross-details.json、cross-after-failed.jpg|未通过，需诊断宿主拒绝哪条正向引文。|

form-running-glow.jpg 同时显示真实输入与“Coolzhu Agent is using your computer”提示。该截图证明本轮提示出现，不等于多屏泛光全部验收。

## 已证实缺口与下一步

1. 旧终态在预算耗尽时丢失最后一次验收事实，外层回复自行称“0/3”。不采用这个推断计数。新源码保留最后观察报告并明确它不是新鲜成功证明。
2. AX 包含离屏控件，但给规划模型的候选只有 reference、role、name、focused，无几何/可见提示。Opus 读取源码确认观察接口缺口；最小方案是在 NodeHandle 加可选 in_viewport，复用固定只读 BoxModel 与既有 point，采样总余量 500ms，失败/未知保留 null，不删候选、不自动滚动、不改变输入预检。桌面壳和 Web 必须同包发布。
3. 自然/跨源未过可能是 AX 文本拆分匹配，现有数据不足以确定。新增逐项宿主 grounding 原因、索引、角色枚举、字数；通用诊断不含正文/引文。先真实取证再修改拼接规则。
4. point 的滚动坐标语义仍需 page_y>0 的真实点击确认，不能靠 page_y=0 的成功点击推断正确。

两次 Opus 原始审查保存在 opus-review.md、opus-viewport-review.md。源码修复/工程测试不属于本目录的正式 0.2.72 验收结果，应在独立候选报告里记录，不能覆盖这些失败。

## 仍未完整验收

前端停止/关闭/替换恰逢 down/up 微区间；显式导航与 popup 竞态双向覆盖；加载/取消中的页面来源同步；缩放与多屏；历史前进；表单滚动与提交；自然/跨源宿主目标确认；Paint 完整人物任务。微信保留原功能且不测。

runtime-receipt.json 保存实际父运行、CU 动作、诊断、ACP 排空与数值 payload 收据；evidence-manifest.json 保存文件摘要。没有保存 API_KEY 或完整配置。原始事件时间证明本轮 popup 在 up 回执之后 pagehide，不代表微区间替换测试。
