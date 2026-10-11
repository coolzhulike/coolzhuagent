Let me read the turn scope file and the rest of the grep output.我没有改代码、没有操作桌面、没有跑测试。本轮读源码用了 18 次宿主只读调用：read 11 次、grep 7 次，数量以宿主台账为准。

## 先更正上轮的一个判断

`computer_use_desktop_bridge.rs` 第 127 行的 `fallback_scope` 已经写明：window-canvas 只是可见客户区，包含工具栏，不等于绘画画布。所以契约文字本身没写错。真正的缺口是没有给模型一个明确的绘图区引用，模型自己找错了位置。

## 决策 1：浏览器路由改用结构化显式选择，不再扩关键词

**源码事实**
- `from_current_user` 只按本轮正文的关键词判断（`computer_use_turn_scope.rs:16-18`）。
- 判断结果在 `main.rs:21192` 冻结。
- 只有判为原生时，`chat_run_admission.rs:53` 才绑定面板（`capture_panel_binding`）。
- 否则 `computer_use_executor.rs:2048` 走外部浏览器预检（`BrowserNativeBridge::preflight`）。B 场景返回 `extension_unavailable`，就是这条路径。

**最小改动**
1. 发送请求增加一个可选字段 `browser_backend`，取值 `native_panel`、`external` 或空。由聊天页上一个可见、可撤销的选择项设置，不能因为面板开着就自动设置。
2. `from_current_user` 增加一个接收该字段的版本，在 `main.rs:21192` 和正文一起冻结。
3. 该字段只决定路由，不产生权限。只读、导航等约束仍只从正文推导。面板绑定沿用第 53 行，room 切换沿用 generation 失效机制。
4. 冲突时阻断并说明原因，不猜。例如字段选了 native_panel，正文却写了外部浏览器或 Chrome。
5. 兼容：字段为空时保持现有关键词逻辑，旧关键词保留但不再新增。

**未核实**：前端发送载荷里是否已有类似字段。那次 grep 用了花括号 glob，返回空，结果不可信。

## 决策 2：页面导航与在途 down/up 的竞争

**关闭和导航在源码上的区别**
- 关闭：`invalidate`（`browser_panel.rs:511-533`）先把 generation 加一，再调用 `retire_view`（`native_browser_input.rs:38-44`）。视图立即隐藏，但要等执行门（execution_gate）释放后才真正关闭。所以宿主不会在 Execute 返回前销毁视图，最多等到 3 秒超时。
- 导航：用户导航（`browser_panel.rs:709`）和弹窗改为当前视图导航（第 472 行）都直接调用 `view.navigate`。它们不取执行门，也不延后或撤销。

**源码修改**
- 在这两处导航前尝试获取执行门。拿不到时，最多等到现有 3 秒释放截止。等到后先复核 generation 和 `pending_navigation.revision`，已过期就丢弃这次导航。不放宽 3 秒截止。
- 页面自己用 JS 发起的跳转，宿主拦不住。这种情况只能靠 Execute 之后的资源/文档复核如实上报：up 落到新文档或无法确认时，报 node_changed 或 ReleaseUnknown，并记为失败分支。

**合法、有界的实操**
- 竞争窗口由页面自身行为构造，不依赖主会话的时机，因此不受约 10 秒往返延迟影响。
  - 页面 A 在 pointerdown 处理里直接 `location.assign(B)`，用来测页面自发导航。
  - 或调用 `window.open(B)`，走宿主第 472 行路径。弹窗是否改在同一个面板视图里导航，我没有读到确认，需要实测前先核。
- 点击由模型按用户指令发出，导航由页面触发，主会话不发任何辅助事件。
- 页面 A 记录 down 和 pagehide；页面 B 记录有没有收到无配对的 up，以及之后一次正常点击是否成对。

**通过标准**
- B 页面没有孤立的 up。
- 宿主回执和页面事实一致：只有 up 确实在 A 上送达，才能报 released。
- 鼠标没有卡在按下状态。

**停止条件**：出现 ReleaseUnknown、孤立事件，或者回执和页面事实不符，就停下保存轨迹，不靠重跑把它洗掉。

**说明**：宿主用户导航和 down/up 的竞争，在 10 秒延迟下没法合法命中。修完后只能写"有源码保证，实测未覆盖"。

## Paint：draw_region 最小方案

**新发现**：`canvas_element`（`computer_use_desktop_bridge.rs:897-908`）用名称包含"画布"来匹配。真实 Paint 的组"在画布上使用 画笔 工具"已经能命中。但任何名称带"画布"的控件也会命中，再加上 Document/Image 类型全部算画布，所以范围过宽。Paint 里是否真有其它带"画布"的控件，我没有实测。

**最小实现**
1. 只有同时满足以下条件的元素才输出可选的 `draw_region`：
   - control_type 为 Group；
   - `automation_id == "image"`；
   - 名称包含"画布"；
   - 所属进程或窗口就是当前目标 Paint 窗口；
   - 唯一匹配。
2. `draw_region` 的内容：
   - `{rect, source:"uia", element_reference, automation_id, coordinate_space:"desktop_physical_px"}`；
   - rect 与客户区、截图区（`client_rect`、`screen_rect`）取交集。
3. 匹配不上或匹配多个就不输出。旧的 `canvas_rect` 和旧归一化契约原样保留。
4. 在 `drag_contract` 中写明：有 `draw_region` 时，优先用它的 reference 作为 drag 目标，笔画点相对它的 rect。
5. 第一版不把 `draw_region` 放进 `FrameRef::bind`（第 131 行附近），避免旧帧失配。
6. 不阻断画布外的输入，也不生成任何固定的 Paint 动作。

**baseline 与进展分开**
- 首次观察时记录 `draw_region` 裁剪图的哈希作为本轮 baseline。原图里的旧人物和中间横线都算 baseline 内容。
- 新增结果 = 结束帧与 baseline 在 `draw_region` 内的差异。
- 每步进展 = 当前帧与上一帧的差异。
- 有 requires_new_effect 但没有 `draw_region` 时，标为 `new_effect_verified=unknown`，不能判 goal=true。观察和只读任务照常零动作完成。

## 采纳风险与真实验收门槛

**风险**
- 导航延后会让用户点导航按钮时最多迟滞 3 秒。
- `draw_region` 的识别依赖 Paint 版本的 UIA 结构，换版本就不输出，退回普通观察。
- 显式选择项有可能被误设成默认值。

**真实验收门槛**
- Paint，在已有旧图的画布上"画一条新横线"：人工确认新线在白色画布内、不在工具栏上；宿主 goal 与人工判断一致。
- 零输入且要求新成果的请求不得判成功。
- 只读的存在性问题仍能零动作完成。
- Browser：显式选择 native_panel 后，A、C 两类提示词都命中右栏面板。
- 改完要出新包，在新包上做正式回归，0.2.71 不含这些修复。

**未读或未实测**
- 前端发送载荷。
- `native_browser_host.rs` 的关闭路径。
- 弹窗导航的目标视图标签是否就是面板视图。
- CDP 在导航中途对 mouseReleased 的实际行为。