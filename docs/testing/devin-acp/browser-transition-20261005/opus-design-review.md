Let me focus on native_browser_verification.rs and adapter.**结论：方向成立，没有发现越权。但有三处必须改正，另外 Keys 的情况要先说清楚。** 本轮宿主只读调用共 8 次（read 2、grep 6），以宿主台账为准。我没有改代码，也没有运行测试。

**关于 Keys 的纠正**
- 我没法核对自己上轮的原话。但在 438f6ec 快照里，原生适配器 `native_browser_adapter.rs:42` 只放行 Click、Scroll、TextInput、Navigate 四种动作，`native-browser-protocol/src/lib.rs:142` 的 `PanelInputKind` 里也没有 Key。
- 包含 home/end/tab/enter/escape 的白名单在外部扩展 `browser-extension/content_script.js:248`。我在 `repository/modules` 下 grep rawKeyDown 没有命中。
- 所以快照里原生面板不支持 Keys。如果 fd8315e 已经接入原生 Keys，那和"适配器只改了停止分类"的说法对不上，需要你核对。
- 不管哪种情况，设计上都应该假定 Enter 会自然导航：只要原生 Keys 存在，就必须进入 pending 来源清单。

**必须修正的三处**
1. **pending 的生命周期要封闭。** 每个新动作执行前，先拿 expected 快照和 pending 比较：
   - 文档已经变了：先固定为 Observed，再派生这次动作的来源；
   - 没变：丢弃 pending。
   
   Scroll 不产生 pending，但也走这个比较。每次输入最多认一次跳转。已固定的 Observed 之后如果文档又变了，`matches_page` 必须失败，不能重新固定。这样才不会把后面别的动作或计时器引起的跳转算到早先那次点击头上。
2. **序列化要兼容。** Explicit 继续用 `authorized_navigation` 这个键，形状不变。verification:28 用的是 deny_unknown_fields，`adapter.rs:175` 是写入点。Observed 另起一个键，比如 `input_transition`。Pending 只留在桥内部，永远不进快照。要确认新键不会影响录制帧和实时帧的相等比较（`computer_use_adapters.rs` 里那段我没读）。
3. **只认 http(s) 文档。** 错误页（chrome-error）、about:blank、data:、blob: 都会让 token 和 URL 一起变，必须排除。

**需要实测确认的前提**
"revision 未变"这个条件，取决于页面自己调 `location.assign` 时宿主不递增 navigation_revision。browser_panel 的导航事件处理我这轮没读。H 场景的原始记录里应该能看到跳转前后的 revision，用它来确认。如果实际会递增，这套设计在 H 上会直接失败；不能为此放宽到"revision 允许 +1"，否则手动改地址也会被放进来。

**跨源：同意不要求同源**
同源本来就不是安全边界，输入授权也是独立检查的。真正剩下的风险是：最终判断改在任意目标页上做，恶意页面可以显示和成功条件一模一样的文字。这个风险在原来的 requested 页上同样存在，所以不加域名硬拦，只做两件事：
- 验收摘要和轨迹里写明最终 URL，以及来源为 `after_settled_input`；
- 如果成功条件里写了目标 URL 或域名，由裁判对照 `page.url` 判断，这本来就能被 `grounded_positive` 引用。

**释放和来源要分开**
- Click 产生 pending 必须是 Released；Keys 必须有 keyUp 回执。
- Unknown 或 NotDispatched 不产生 pending，这一点你已经写了。
- 来源证明和终态 goal=true 都不能当作接口已释放的证据，释放只看输入回执。

**未读、未实测：** browser_panel 对页面自发导航的 revision 处理、`FrameRef` 的录制/实时比较、fd8315e 的实际改动。真实通过标准是：
- H 重跑后 verification 通过，轨迹里显示来源为 after-settled-input；
- 起始页不对的场景仍然被拒；
- 手动改地址的场景仍然被拒。