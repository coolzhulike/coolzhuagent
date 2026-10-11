**结论：没有发现越权。文档票据和控制票据的隔离，按 diff 看是可靠的。有 1 项必须修，1 项要先核实（成立就必修），另外有 1 条验收归因规则。** 本轮没有调用工具，只看了你给的 diff 和新模块，不是完整审查。

**必须修**

1. **加载期间跳过了全部来源校验（adapter execute）。**
   - `expected.state["loading"]!=true && !confirm(...)` 在加载时连资源身份都不核对，就直接取 `original_url()`。
   - 会出问题的链路：先点击，留下 pending；用户在面板里手动改地址，revision 加 1，页面进入加载；模型再用 nav 引用执行 Navigate。这时会继承旧链上的 original_url，最终观察范围就跨过了一次手动导航。
   - 以前 confirm 会因为 revision 不一致而拒绝。
   - 最小修法：加载时只跳过文档和 URL 的比较，pending 源的 workspace、room、label、generation、navigation_revision 仍然要和这次观察完全一致。location.assign 和 popup 本来就不改 revision，所以不会误拦你已经测过的链路。

**先核实，成立就必修**

2. **`control_snapshot` 要求 `reply.url` 是合法的 http(s) 地址。**
   - 现在心跳和 `observe` 的第一步都依赖它。如果页面已经就绪，但 reply.url 为空或者不是 http(s)，以前 `input_resource` 能拿到资源，现在心跳会报没有资源，observe 会报 `resource_changed`。这是回归，错误码也指错了原因。
   - 要确认就绪状态下 reply.url 一定有值。如果不一定，文档路径要退回到 `input_resource`。

**验收归因规则（必须写进标准）**

3. **加载中只给了 Navigate 一条出路。** 在\"自然导航\"类任务里，模型可能显式 Navigate 到目标地址，把自然跳转变成显式导航来源。
   - 自然导航类的验收，要检查动作列表里没有 Navigate。
   - 提示词可以补一句\"只有 objective 明确给出目的地址时才 navigate\"。这一句可选。

**你点名的三项**

- **加载时的待结算来源**：snapshot 在加载时提前返回，没有固定 pending 的终点，也没有撤销它，等就绪后的第一次观察再固定，这个顺序是对的。问题只在 execute 那边，就是第 1 项。
- **navcache 生命周期**：
  - register 时会清掉控制身份不同的旧引用，最多保留 4 个，有效期 120 秒；
  - 任何一次 Execute 派发后都会调 `retire()` 清空；
  - 控制身份包含 generation、scope、revision、url 和 popup_sequence，关闭面板、切换范围、出现新 popup 时都会失效。
  - 提前出错返回的路径不会清理，但旧引用因为身份比对过不去，不会被借用。
- **文档/控制票据隔离**：
  - `unchanged()` 要求两次验证是同一种类型；`(Click, Navigation)` 落进 `_ => NotDispatched`；
  - 引用过期后 `contains` 返回 false，会改走 verify_document，然后失败，安全；
  - adapter 要求 nav 前缀和 Navigate 同时成立，planner 的 schema 也只允许 Navigate 用 nav；
  - 旧的 DOM Navigate 仍然在 UI 闭包里核对 `input_resource`；
  - 控制路径在同一个闭包里再核对 control（包含 popup_sequence），`begin_native_navigation` 加锁后又核对一次。

**未核实**

文档输入的各个 verify，以及 click 闭包开头的资格检查，是否全都用 `input_resource`。这些代码不在 diff 里，需要确认它们没有被心跳语义的变化带宽。

**可以有界延期**

- 加载期间重定向改写 url 会让 nav 引用失效，模型要重新观察。这样保守，可以接受。
- nav 准备和执行之间不一致时报的是 `native_browser_node_changed`，建议换成导航专用的错误码，方便诊断。
- 加载期间 `visible_progress` 依据的是 pending 的 URL。它只影响进展计数，不影响成功判定，但要写明这一点。
- `PageObservation` 和 `HostState.resource` 的语义变了，桌面壳和 web-console 必须同包发布。

**缩放验收**

- Windows 150% 是系统 DPI，不等于浏览器缩放。现有的通过只覆盖了 DPI，不能算作非 100% 缩放已通过。
- `zoom_hotkeys_enabled(true)` 会对所有用户生效，而命中预检在浏览器缩放下没有验证过。建议只在验收构建里打开，或者正式发版前必须先完成实测。
- 实测方法：主会话按快捷键缩放到 125%，记为辅助事件；SWE-2 在 pageY>0 时点击 checkbox 和 submit。
  - 点中正确节点算通过；
  - 预检拒绝属于保守局限，可以接受；
  - 点错节点就停下，加上\"缩放不等于 1 就拒绝\"再复测。

**down→替换→up 竞争**

仍然是有边界的未覆盖风险。这次慢弹窗加 Navigate 的通过，只证明了加载接管这条路径，不能追认微区间已经覆盖。