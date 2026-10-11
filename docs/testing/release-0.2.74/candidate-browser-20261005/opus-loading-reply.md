**结论：方向可行，最小边界成立。下面 4 处需要修正，并且要先核对 `begin_native_navigation` 是否拒绝 loading 状态。** 本轮没有调用工具，判断只基于你贴出的片段。`begin_native_navigation`、`verify_document`、`input_process`、`host::observe` 的完整实现我都没读。

**片段能证实的事实**
- 只要 `reply.loading` 为真，`visible_panel_resource` 就返回 None。所以加载期间观察直接报 `panel_unavailable`，这就是你说的阻塞点。
- 现在的 Navigate 校验走 `verify_document`，需要文档 token 和节点。加载中没有文档，这条路径必然失败。
- `navigate()` 在 UI 闭包里会再查一次过期，再调 `begin_native_navigation`；`core.Navigate` 出错记为 DispatchUnknown。这几项都应该保留。

**必须修正**
1. **用类型把两种资源分开。** 定义 `ControlResource` 和 `InputResource` 两个 newtype，`input_process` 和各输入的 verify 只接受 `InputResource`。只靠命名约定，迟早会被混用。`begin_native_navigation` 如果内部复用 `visible_panel_resource`，就要拆成一个 control 版本，不能顺手放宽 input 版本。
2. **只读模式要隐藏导航引用。** 片段里只读时 `elements` 为空。`navigation_target` 也必须跟着置空，因为 Navigate 本身就是输入。
3. **加载期间的 InputPending 怎么关。** 加载中拿不到文档，`confirm` 没法比较。规则应是：加载中执行 Navigate 时，把 pending 关掉但不固定为 Observed，original_url 按现有 Click→Navigate 链传递；序列化时不输出 `observed_input_transition`。否则 pending 会被固定到 Navigate 的目标页，被错标成 `after_settled_input`。
4. **先等就绪，再返回 loading。** 片段里已经有 50ms 轮询，就在现有剩余预算内先等 ready，到时仍在加载才返回控制状态。这样模型不会反复观察、白白消耗步数，也不用改任何超时。

**可以接受的竞态**
- 观察到加载之后页面加载完成：revision 不变，导航引用仍然有效，Navigate 会覆盖刚加载好的页面。这是模型显式授权的 URL，可以接受。
- 用户手动导航、关闭面板、换作用域：revision 或 generation 会变，导航引用失效，这是正确的。

**范围建议**：第一版只在加载状态下用 `nav-` 引用，就绪状态的 Navigate 保持现有 `dom-` 路径，避免影响已回归的场景。两种前缀互相借用一律拒绝。

**非 100% 缩放的真实验收**
- 主会话把面板缩放到 125% 和 150%（标为辅助事件），Windows 150% DPI 另跑一轮。
- 测试页记录 pointerdown 的 target id、clientX/Y、devicePixelRatio、`visualViewport.scale`。
- SWE-2 在 pageY>0 时点击 checkbox 和 submit。
- 判定：
  - 点到预期节点，且坐标落在它的 rect 内：通过；
  - 预检保守拒绝：记为已知局限；
  - 点到别的节点：阻塞，加上\"缩放不等于 1 就拒绝\"后复测。

**严格的 down→替换→up 竞争**
- 生产代码在同一个闭包里连续入队，文档级替换要落进约 1ms 的窗口里，只能靠宿主侧插桩，那就不是生产实现了。这项转为有边界的未覆盖风险，写明依据：同闭包入队、退役要等执行门、ReleaseUnknown 不补发。
- 每轮导航回归都要满足：ReleaseUnknown 为 0；新页面第一次 pointermove 的 `buttons` 为 0；没有孤立的 down/up。
- 每次回归预先固定最多 3 次自然尝试，没命中就记\"未覆盖\"，不能算通过。
- 可以合法命中的是另一个场景：页面在 pointerdown 处理里同步替换目标节点，这时 up 一定在替换之后到达。它只能标成\"节点替换\"，不能当作文档替换的竞争已经通过。