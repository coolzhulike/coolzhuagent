# 文档变化负例的输入归因复验准备

正式 127 原失败：页面只记录可信 keydown 与 BODY，未记录 key/code；不能区分具体键或来源。本目录是下一轮复验素材，不修改 release-0.2.127 中的事件、检查或失败结论。

old.html/new.html 均实际加载 event-probe.js。探针记录每文档唯一 ID、顺序、页面时钟与事件时间、目标/活动元素、焦点/可见性、key/code/location/repeat/修饰键，以及指针/滚轮/inputType。故意不记录输入正文；输入来源始终 unknown，不凭 isTrusted 推断为模型、CDP 或人类。

运行页面需要受控服务器：静态提供三份页面文件；/phase 必须复用只读原运行账本的 planning/action_count=0 判定；/event 保留接收时间及所有字段。当前未实现或启动此服务器，也未发送真实模型请求。node probe-test.cjs 仅模拟事件验证记录字段、丢投递计数、容量计数和正文排除；实际 exit 0，不是浏览器可信输入验收。

正式复验的门禁不放宽：新旧页面可信 pointerdown/up/cancel、mousedown/up、click/dblclick、keydown/keyup、beforeinput/input、wheel 输入均必须为零；focus/load/pagehide 作为生命周期单独记录。须比较页面内 __inputAttribution.events 与服务器收到的 document_id/sequence，overflow 和 beaconRejected 必须为零；生命周期终态收集缺失不能默认通过。还要关联原生 key dispatch、协议请求、runtime attempt 及步骤起止时间。页面事件本身仍不能证明来源。

复验仍要求同一原 SWE-2-medium/唯一 island-kayak、一请求、旧动作 not_sent/not_needed、无额外重规划、原 end_turn/drained 和解锁；严格 nativeTarget/commit/按下抬起/关闭/HRESULT 竞争是另一个尚未闭环范围。后台启动此前被审批拒绝，未绕过。候选待正式复验，Goal active。
