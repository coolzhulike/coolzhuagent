**结论：这确实是观察接口的缺口，可以按你的方向修。建议做成 `NodeHandle` 上的可选字段。另外有一个坐标语义的前提，必须先用真实页面确认。** 本轮宿主调用 9 次（列工具 1、glob 1、grep 2、read 5）。

**已从源码证实**
- 观察结果给了整页 `viewport`（observation:58-66），提示语也写了"AX 节点可能位于视口外"（adapter:273）。但 `elements` 只带 reference、role、name、focused 四项（adapter:257-259），模型没有任何依据判断哪个节点在视口外。
- 宿主预检 `point()`（target:43-75）用 BoxModel 和 LayoutMetrics 判断，不在视口内就返回 `outside_viewport`，不自动滚动，也不把坐标钳进视口。这部分不用改。

**字段放哪里**
- 放在 `NodeHandle`，加 `#[serde(default, skip_serializing_if="Option::is_none")] in_viewport: Option<bool>`。它和 handle 按索引一一对应，不用像单独列表那样另做一致性校验，不能判定时就是 None，对应 unknown。
- `deny_unknown_fields` 对新增的可选字段向后兼容。风险在于桌面壳和 web-console 版本不一致时，旧读取端遇到新字段会拒收，所以两端必须同包发布。

**实现方式**
- 在 observe 里，只对已登记的非 RootWebArea 候选（最多 128 个）读 BoxModel。
- 复用同一次读到的 LayoutMetrics，直接调用 `target::point()`，结果映射为：
  - Ok → true；
  - `outside_viewport` → false；
  - 其他错误 → unknown。
- 这些读取放在第 67-71 行那次文档复核之前，让现有的身份复核把它们一起覆盖。
- 总耗时设一个上限，比如 500ms。超时后剩下的节点全标 unknown，不过滤离屏节点。
- 观察阶段不做 HitTest，被遮挡等情况仍由预检判定。
- 提示语补一句：false 时先 scroll 再重新观察；这个字段不授予任何输入资格。

**待证实（重要）**
- `point()` 把 BoxModel 的四角坐标减去 `pageX/pageY`，相当于默认它们是文档坐标，单元测试也是按这个假设写的。
- CDP 的 `getBoxModel` 返回的是文档坐标还是视口坐标，我在这里无法核实。已经通过的点击都发生在 pageY=0，区分不出来。
- 如果实际是视口坐标，页面滚动后，在视口内的控件也会被误判为 false 或被预检误拒。复用 `point()` 能保证观察和预检结论一致，但如果有错也会一起错，必须用 pageY>0 的真实场景来验证。
- `reject()` 会把预检拒绝作为可恢复的步骤反馈交还模型，还是直接结束本轮，我没读到。如果是直接结束，即使有了 in_viewport，模型也没有机会改走 scroll。

**性能和竞态**
- 每个候选多一次只读调用。128 个候选时延迟可能明显增加，需要实测，所以要靠总上限控制。
- 观察和执行之间，懒加载、粘性头部、布局抖动都可能让 in_viewport 过时。它只是提示，预检仍是唯一依据。

**执行计划**
1. 先确认 `reject` 的路径。
2. 加字段、复用 `point()`、加耗时上限，补 2 条单元测试：unknown 的形状；pageY>0 时的判定。
3. 在 0.2.73 候选包上用真实 SWE-2 跑同一张表单，通过标准：
   - 模型自主执行 scroll，再在新观察里点到 checkbox；
   - 宿主轨迹记录 pageY>0、预检通过、released；
   - 截图里确认勾选，并与宿主 goal 分开记录。
4. 如果滚动后预检误拒，就停下来先修坐标语义，不放松预检。

history 第三次点击是 push 而不是 forward，这一点同意，暂不归为适配器问题。

**未读：** `reject` 的定义、devtools 里 BoxModel/HitTest 的实际参数、native_browser_nodes 的登记逻辑。