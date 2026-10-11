**结论：在 100% 缩放、没有双指缩放的前提下，这个修复的语义和职责都对：偏移只加了一次，也没有负偏移漏洞。修复本身还没有经过真实验收。** 本轮宿主调用 7 次（列工具 1、glob 1、grep 1、read 4）。

**已从源码证实**
- **官方语义**：`getNodeForLocation` 把传入的 x/y 乘以缩放系数当作文档坐标，再经 `DocumentToFrame` 扣掉滚动量后做命中。所以命中查询必须传文档坐标。
- **偏移只加一次**：
  - `point()`（target:44-75）已经不再扣 pageX/Y；
  - 偏移只在 `hit_test_point`（:78-90）加一次；
  - 派发用的仍是视口坐标 x/y（:122）。
- **负偏移**：结果不在 `0..=i32::MAX` 范围内就拒绝，只会误拒，不会越界。
- **两个入口都接上了**：`verify` 和 `verify_viewport`（:149）都已接入。backendNode 和 frame 精确比对、两次文档复核、2 秒上限都还在。
- **错误码**：命中失败时报静态错误码 `native_browser_hit_test_failed`（devtools:95），不回显正文。

**剩余局限**
1. **滚动发生在读 metrics 和命中之间**：`verify` 二次复核时只比较 content 四角和 `point()`，而 `point()` 已经不依赖 pageY。对普通元素，滚动会改变四角坐标，能被发现。对 fixed 定位元素，四角坐标不变，靠的是命中改落到别的节点、被精确比对拒掉。这样仍然安全，但如果想让复核和命中输入一致，可以顺手比较两次 metrics 的 pageX/pageY 是否相同。
2. **双指缩放**：`cssVisualViewport.pageX` 已经包含视觉视口的偏移。但 scale≠1 时，视口坐标要先除以 scale 才是 CSS 像素，所以那时结果会错。这一点没有实测。
3. **浏览器缩放≠100%、RTL 页面横向滚动出现负 pageX、小数滚动取整差 0.5px**：这三种情况都没有实测。从代码看，它们多半表现为命中不一致或越界而被拒，属于保守误拒。
4. **原先报 native_observation_failed 的原因**：表单 form2 那次报的是 native_observation_failed，到底是不是 "No node found" 引起的，没有证实。新错误码可以把这种情况区分出来。
5. **iframe 内的控件**：仍然会因为 frameId 不一致被拒，这是现有范围的限制。

**必须通过的实操标准（尚未通过）**
真实 SWE-2 在 pageY>0 时：
- 点击 checkbox 和 submit 都要预检通过，并且是 released；
- 截图确认已勾选并已提交，和宿主 goal 分开记录。

非 100% 缩放另外单独验证，没有实测之前不加通用拒绝。