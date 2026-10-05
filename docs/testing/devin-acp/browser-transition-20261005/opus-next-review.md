这一轮我没有调用宿主读取，下面的判断都基于你给的说明。WebView2 事件语义按官方文档理解，没有在实机上核实。

**1. SourceChanged 只同步非新文档的地址：方向成立，补四处**
- **被跳过的地址变化要补回。** 有新文档导航待处理时，你跳过同文档变化，这没问题。但如果那次新文档导航失败、被取消或被拦截，跳过的 pushState 地址就永远丢了。建议在导航完成且失败或取消的分支里，从 `webview.Source` 重新同步一次。
- **回调里不要保留强引用。** 回调只持弱引用，并核对 generation、label、active，防止视图退役后回调还在执行，或者形成 webview→handler→registry→webview 的引用环。视图退役时调用 `remove_SourceChanged`。
- **防止地址回声。** 发给前端的必须是"只更新显示"的事件。前端地址栏收到后不能再触发导航请求，否则会递增 revision，同时破坏 SPA 的来源证明。
- **只影响显示。** `reply.url` 只用于展示。观察和来源证明继续读页面实时的 URL 和 document_token，不能改成读这个缓存。

需要实测确认：
- 手动地址栏里只改 #fragment、以及后退/前进命中同一文档时，是否触发 NavigationStarting。如果不触发，现有的待处理状态可能一直清不掉。
- 注册失败时直接让面板创建失败也可以。因为它只影响显示，降级成"记日志 + 地址不同步"更符合减少过度防护的方向。这个由你决定。

**2. 补"输入"作宾语：可以，属于与 H 同类的 Agent 误拦**
- 条件：必须紧跟在"发送"之后、整个宾语完全匹配、后面接现有的句尾或分隔符边界，不能做包含匹配。
- 反例照旧拦截：发送输入给他人、发送输入框内容、发送输入，发布消息。
- 实际的节点名和 action arguments 仍按原文分类。
- 只补"输入"这一个词，不加"输入内容""输入信息"等变体。
- 回归测试直接用 O 的原句。单元测试只证明规则正确，要 O2 实测通过才算通过。

**3. window.open 打开到同一右栏：先实测再修**
- 预期的失败点：revision 变了，按现有逻辑只撤掉输入来源证明、读页照常，验证器退回 requested URL 规则后判 blocked，而不是读页失败。实测失败点不一样的话，以实测为准。
- 失败后的最小绑定方案：
  - Shell 只记录事实，不理解输入：在 `handle_new_window` 里记下 `{from_revision, opener_document_token, request_uri, IsUserInitiated}`，并通过 observe 暴露出来。
  - 适配器做判定：同时满足以下全部条件才固定 Observed——
    - pending 的 revision 等于 from_revision；
    - 来源 document_token 等于 opener 的 token；
    - IsUserInitiated 为 true；
    - revision 只比原来多 1；
    - 发生在下一个动作封闭 pending 之前。
  - 每个 pending 只认一次。之后 revision 再变，或页面再次跳转，都撤掉来源证明。
- 这个证明只扩大最终只读观察的范围。新页上的每一步输入仍走现有的节点、document 和权限检查。
- 已知的局限：IsUserInitiated 区分不了"模型点击"和"用户同时手动点击"，所以标签只写 `after_settled_input`，不声称因果。非用户手势（例如计时器）触发的 popup 一律拒绝。