# 文档变化负例的输入归因复验准备

正式 127 原失败：页面只记录可信 keydown 与 BODY，未记录 key/code；不能区分具体键或来源。本目录是下一轮复验素材，不修改 release-0.2.127 中的事件、检查或失败结论。

old.html/new.html 均实际加载 event-probe.js。探针记录每文档唯一 ID、顺序、页面时钟与事件时间、目标/活动元素、焦点/可见性、key/code/location/repeat/修饰键，以及指针/滚轮/inputType。故意不记录输入正文；输入来源始终 unknown，不凭 isTrusted 推断为模型、CDP 或人类。

运行页面需要受控服务器：server.py 静态提供三份页面文件；/phase 使用只读运行账本的 running/planning/action_count=0 判定；/event 保留接收时间及所有字段。当前未启动此服务器，也未发送真实模型请求。node probe-test.cjs 仅模拟事件验证记录字段、丢投递计数、容量计数和正文排除；实际 exit 0，不是浏览器可信输入验收。

正式复验的门禁不放宽：新旧页面可信 pointerdown/up/cancel、mousedown/up、click/dblclick、keydown/keyup、beforeinput/input、wheel 输入均必须为零；focus/load/pagehide 作为生命周期单独记录。须比较页面内 __inputAttribution.events 与服务器收到的 document_id/sequence，overflow 和 beaconRejected 必须为零；生命周期终态收集缺失不能默认通过。还要关联原生 key dispatch、协议请求、runtime attempt 及步骤起止时间。页面事件本身仍不能证明来源。

复验仍要求同一原 SWE-2-medium/唯一 island-kayak、一请求、旧动作 not_sent/not_needed、无额外重规划、原 end_turn/drained 和解锁；严格 nativeTarget/commit/按下抬起/关闭/HRESULT 竞争是另一个尚未闭环范围。后台启动此前被审批拒绝，未绕过。候选待正式复验，Goal active。

已补 verify-events.cjs 页面子门禁：输入两文档最终快照与服务器页面事件 JSONL，按文档身份/连续序号及逐字段内容对应；重复、漏收、字段篡改、溢出/投递失败、旧文档缺最终 pagehide 或任何可信输入均拒绝。服务器规划触发等非页面事件应保留在原日志中，另行导出页面事件供此工具检查，禁止为通过门禁删掉可信页面事件。

命令为 `node fixtures/verify-events.cjs old-snapshot.json new-snapshot.json page-events.jsonl`。输出只有 page_zero_input_gate，formal_acceptance 固定 not_proven：新文档快照截取时点、原生派发时钟、真实账本及协议终态仍须独立核验。`node fixtures/verify-events-test.cjs` 的上述负例回归实际 exit 0；探针回归再次 exit 0。没有运行正式服务器或真实模型请求。

快照完整性补充：探针单独累计 totalEvents（包括超出保存容量的事件）。校验必须有安全整数 totalEvents，且在 overflow=0 时严格等于 events.length。总计数缺失或不匹配直接拒绝，即使服务器与被截短的数组完全匹配也不能通过。两项新增负例与既有探针/校验回归实际 exit 0。该计数只能核对采集快照范围，不证明快照之后不存在输入，正式采集终止时点仍需账本和原生时钟证据。

服务器准备：命令参数必须明确提供 `--database`、`--output`、`--marker`；输出目录每轮新建，存在则拒绝覆盖。SQLite 使用 mode=ro、显式 BEGIN 同一查询快照及 closing 关闭句柄。来源标记按字面前缀匹配，百分号/下划线不作为通配符；一个来源绑定多运行时拒绝。事件写入 flush/fsync，失败返回 503 并保留 logging_failed 状态。两类时钟以字符串保留纳秒精度。服务器仍未实际启动，HTTP/浏览器接线尚未运行验证。

`python fixtures/server-test.py` 使用独立临时 SQLite 验证字面标记、只读前后文件 SHA、终态/已派发不触发及多运行拒绝，修复后 3 项测试实际 exit 0。首次及复现回归因连接未关闭导致 Windows 文件清理失败，实际 exit 1；完整失败保留 tmp/analysis-browser-attribution-server-first-failure.txt，修复后结果保留 tmp/analysis-browser-attribution-server-tests.txt。未读取或修改正式运行库。
