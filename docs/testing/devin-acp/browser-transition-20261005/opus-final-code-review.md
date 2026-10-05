Now read session.rs 18-60, 215-240, 510-545 and planner 550-585, 830-875.**结论：这一轮可以提交草稿 PR。代码里没找到依据确凿、必须先修的越权或泄露问题，只有 1 处建议小修。** 草稿 PR 不等于整体验收完成。本轮做了约 27 次宿主只读调用（read 14、grep 13），以宿主台账为准；没有改文件，也没有运行测试。

**1. 多步基线（读的是 `computer_use_visual_baseline.rs` 全文，planner 548–585、800–875 行）**
- **不保存旧 UIA 授权：成立。** `Start` 里只有 PNG data_url、generation，以及窗口/进程/rect/尺寸/DPI 这些几何信息（7–11、28–36 行），没有 elements 或引用，也不参与 FrameRef。
- **基线固定在第一次动作后的验证：成立。** 需要"本轮新增"的初始单帧会在 825 行提前返回，到不了 `comparison`，所以基线取自第一次动作后验证时的 before 帧。
- **按 job 隔离：只核实了一部分。** `visual_baseline` 是 planner 实例的字段，每个实例默认为空（569、580 行）。但我 grep `new`/`with_context`，命中的全是测试，生产代码在哪里构造 planner 没找到。所以"跨 job 不复用"、"分段等待时不重建 planner"，代码上未核实；U 实测的 generation=1 只能作为旁证。
- **保守但不阻断：** `canvas_rect` 被算进几何比较。画布区域一变就会判为不可比，新增结果判 false，但不阻断观察和动作。

**2. ACP 出站上限（session.rs 16–60、216–230、513–552 行，journal 364–373 行）**
- **8MiB 只给合法的图片提示：成立。** 只有 `session/prompt` 且每个块都合法（png/jpeg/webp、data 非空）时才放宽到 8MiB；其它情况都是 1MiB，提示文本单独限 1MiB。
- **提交前检查和 write 同规则：成立。** `prompt_content` 在 claim 之前、`write` 在写出之前，调用的是同一个 `validate_outgoing_bytes`。
- **台账只记数字：成立。** journal 只接受 3 个数字键。
- **入站限制：** 读帧继续用 `read_frame`/`MAX_FRAME_BYTES`，没有改动。
- **未核实 1：** `write` 自己不检查图片能力协商，这一步只在 `prompt_content` 里做。如果还有别的地方直接用 `write` 发带图片的 prompt，会绕过能力检查。我没看到这样的调用方，但没有全量核实。
- **未核实 2：** 超限后 `?` 直接返回，`prepared→not_sent` 这一步状态转换由调用方完成，这一段我没读。

**3. 自然导航与 popup**
- 适配器 75–102 行的封闭逻辑，以及验证器 29–34、132–134 行的标签，都已经改用校验后的条件。没有发现新的越权。
- popup_sequence 和 revision 的拆分只读到了 407–409、465–474 行，没发现耦合问题，但没有全读。

**4. SourceChanged 退役注销：成立**
- `invalidate` 在同一把锁内一起取出 label 和 token，先 `remove` 再 `retire_view`（551–562 行）。
- 创建时如果视图已失效，就先 `remove` 再 `close`（833–844 行）。
- 回调里有 generation、label、active 三项过滤（380 行），注销之后迟到的回调也会被丢掉。
- 注册失败时只记日志、置 None，不会让网页整体不可用。这一点与此前说的"注册失败返回创建错误"不同，但更合理。

**建议小修（有源码依据，但没观测到实际事故）**
- `update_same_document_source`（browser_panel.rs:387 行）每次都把 `loading` 设成 false。
- 如果新文档正在加载、又没有登记 pending，旧文档发出的一次同文档 URL 变化会提前把 loading 清掉。
- `visible_panel_resource`（592 行）正是靠 loading 拒绝观察资格的，这样会过早重新开放观察资格。
- 建议只在命中显式 pending 时清 loading，其余情况只同步 url。

**剩余实操缺口**
- 完整人物 V 还在跑，结果待定。
- Browser 微区间替换，以及显式导航覆盖 popup，都没有验收。
- SPA 在加载期间改 URL 的场景没有实测。
- 需要出新包，并做正式版总回归。