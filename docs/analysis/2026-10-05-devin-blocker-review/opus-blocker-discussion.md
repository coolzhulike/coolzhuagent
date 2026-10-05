本轮宿主读调用共 11 次（read 5、grep 6，其中 2 次 grep 因 glob 写法返回空，1 次 grep 只返回文件名），以宿主台账为准。我没有改代码，也没有操作电脑。

## 1. 决策确认与修正

- **Browser 优先，Paint 其次**：确认，撤回上轮 E 中先做 B/C 的顺序。复用慢 HTML 真实页面可以，不改生产延迟。
- **ReleaseUnknown 记为未通过或故障分支**：确认。晚于 up 才关闭，只能证明这一次的顺序，不能说竞争在结构上不可能。
- **坐标分层兼容方案**：同意（风险见第 3 节）。
- **requires_new_effect 字段和本轮 baseline**：同意。本轮 baseline 只用来证明"有新增结果"；上一帧继续用来判断每一步有没有进展。两者不能混用。
- **window 歧义、Devin 能力边界、DSH 用已授权的 Qwen 验收**：同意，不需要修正。

## 2. Browser：哪些竞争能插入，源码依据

**进不来的（有源码依据）**
- 点击：在 `native_browser_input.rs:213-222` 里，同一个 UI 闭包会先把 mousePressed、再把 mouseReleased 两条命令依次排队，排 up 之前不等 down 的回调。所以宿主 UI 线程上的其它操作插不进这两次入队之间。
- 按键：`native_browser_key_input.rs:82-87` 对 rawKeyDown/keyUp 是同样的结构。
- 关闭视图：`browser_panel.rs:513-531` 的 invalidate 走 `retire_view`（`native_browser_input.rs:39-44`）。它会先隐藏视图，然后等执行门 `execution_gate` 释放后才真正关闭。Execute 分支在第 92 行通过 `try_lock` 拿着这个门，直到拿到结果或 3 秒超时（第 229 行附近）。所以 up 回执到来前，视图不会被关闭。
- `browser_panel.rs:797` 有一处直接关闭视图，但对象是创建时 generation 已经变了、还没登记的新视图，不是输入目标。

**能插入的（源码上可见，需要实测）**
- 隐藏不等执行门（第 40 行）。CDP 事件可能会投递到已经隐藏的视图上。
- 资源资格只在闭包开头检查一次（第 189 行附近）。CDP 命令是异步处理的，命令已入队、页面还没处理完的这段时间里，页面可能自己跳转或被替换，事件就会落到新文档上。
- 3 秒超时得到 ReleaseUnknown 后，执行门释放，视图随即被关闭，up 是否送达无法确认。

**未核实**
- `native_browser_host.rs`：grep 没有命中关闭/替换相关的词，它真正的路径没读。
- `main.rs:1270` 的 `existing.close()` 没读。
- 用户自己在面板里发起的导航是否经过执行门，没有核实。

**最小验收方法**
- 页面用 localStorage 记录 pointerdown/up、keydown/up 的时间戳、document id、visibilityState，以及 pagehide 事件。
- 在 pointerdown 处理函数里让页面自己忙等约 800ms，把 down 到 up 之间的窗口拉宽。这是页面自身的行为，不是生产延迟。
- 三个场景各做 20 次左右：取消、关闭面板、替换页面。
- 通过标准：同一文档里 down 和 up 必须成对出现，否则就是 NotDispatched 或 node_changed；不能有孤立的 down；新文档里不能收到无对应 down 的 up。
- 停止条件：出现 ReleaseUnknown 或孤立 down，立即停下保存轨迹，不靠重跑把它洗掉。

## 3. Paint：最小实现与验收

**已证实的 Agent 设计缺陷（不是换算 bug）**
- `computer_use_desktop_bridge.rs:124` 告诉模型笔画坐标"相对 canvas_rect"，第 126 行又把归一化坐标描述为相对目标 rect。
- 但 canvas_rect 实际是窗口客户区：按 y = 106 + 0.16×1096 ≈ 281 推算，落点正好在功能区附近，和截图一致。
- 模型被这个名字误导是可以修的；修好后模型能否画准，属于模型能力，尚未验证。

**最小实现**
1. 保留 canvas_rect 的名字和语义不动，在说明文字里标明它是 client_region，可能包含工具栏。
2. 只有当真实 Paint 的 UIA 元素身份能确认画布时，才输出可选的 draw_region。canvas_element 只要是 Document 或 Image 就会匹配，这不算认证。Paint 画布实际的 ClassName/AutomationId 我没核实，需要先采一次真实 UIA。
3. draw_region 第一版不要放进 `FrameRef::bind`（第 131 行）和 `computer_use_adapters.rs:420` 的 recorded/live 相等比较。否则识别结果稍有抖动，旧帧就会大量失配。如果要放进去，必须带新的显式版本号。
4. 近白矩形只能当提示，不当权威，第一版我倾向于不上。
5. 识别不出画布时，保持现在的普通观察能力，不拒绝画布外的输入。

**真实验收**
- 在新空白画布和已有旧笔迹的画布上，各做一次"画一条新横线"。
- 通过标准：人工看截图确认新线在白色画布内、没有点到功能区，并且宿主的 goal 和人工判断一致。
- 停止条件：采不到可信的 UIA 画布身份，就只交付改标签这一项，并记录原因。

**C 项补充风险**
截图前后差异会混入非绘图变化，比如键位提示消失、悬停高亮。
- 有 draw_region 时，只在这个区域里比较差异。
- 没有 draw_region 时，工具结果标为 `new_effect_verified=false` 这类未证实状态，不判成功。
- 外层模型的回复不重写。

## 4. 会阻碍收尾的问题

**确定问题**
- canvas_rect 的提示文字误导模型（bridge:124/126）。
- Paint 修复不在 0.2.71 里，需要出新包。

**未确认项**
- 隐藏期间和命令入队后、页面处理前的页面替换，实际会不会造成事件错投。
- `native_browser_host.rs` 和 `main.rs:1270` 的关闭路径。
- 真实 Paint 画布的 UIA 身份。
- 新包上的正式回归，以及 DSH 市场"安装→启用→收到工具结果→停用/卸载"整个闭环的实拍。