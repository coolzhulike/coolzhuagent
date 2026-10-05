**结论：实现基本在范围内，没有发现越权，也没有把释放回执当成功。但有一处误拦需要决定是否修，还有一处标签不准。**

我实际读了 `repository-20261005-transition/` 下的 `native_browser_adapter.rs`（323 行全读）和 `native_browser_verification.rs` 的 16–140 行。本轮宿主只读调用 11 次（glob 2、grep 2、read 7）。

**核对属实的部分**
- **Keys**：确实支持，而且和 Click 一样要拿到 Released 才登记 pending（adapter:118、144、197–200、217）。Text 和 Scroll 拿到 Acknowledged、释放状态为 NotNeeded 即可，其中 Scroll 不登记 pending。Unknown 和 NotDispatched 在 217 行之前就返回错误，不会产生 pending。
- **原始 URL 来源链**：没有历史来源时取 `expected.url`（157 行），起始页不对的场景会派生出错误的 original，验证器 32 行会拒绝。Navigate 和 Click 两种先后顺序都会经过 `confirm(expected,true)` 把原始 URL 传下去（150–157、216–220）。
- **pending 封闭**：每个新动作执行前都会封闭旧 pending，包括 Scroll。页面没变时固定在原页面（79–84），之后再变就拒绝，单元测试覆盖了这一点。
- **freshness**：fresh 观察用的是新建的桥，不带任何来源状态。`ensure_fresh` 比较了 13 个身份字段，并在 95 行单独比较 `document_token`，不依赖模型的说法。

**问题**
1. **误拦（新引入的回归）**：snapshot 只要 `confirm` 失败，就直接报 `navigation_changed`，阻断后续所有观察（adapter:280）。以前 Click 不登记来源，snapshot 从来不会因为这个失败。现在 Click、Text、Keys 之后出现第二次 URL 或文档变化，整个观察就停了。会触发的情况有：
   - SPA 先 pushState，路由守卫再 replaceState；
   - JS 多跳跳转；
   - 观察刚好抓到中间页；
   - 搜索框防抖改写查询参数。
   
   建议：Input 两种来源匹配失败时，只撤掉来源证明（置 None 或设一个未认证状态），照常返回观察，验证器自然退回 requested URL 规则。Navigation 保持原来的硬拒绝。这样只是拒绝认证，不会扩大权限，也保留了普通观察能力。
2. **标签不准**：页面没变时，annotate 也会输出 `observed_input_transition`（96–98 行，加上封闭后 url 和 token 与源相同的情况）。而 verification:129 只看这个键在不在就标成 `after_settled_input`，没有使用 32 行的校验结果。结果是没有发生跳转、甚至认证没通过时，摘要也会写成输入后跳转。建议只在 url 或 token 与源不同时才输出这个键，并且用已校验的布尔值来决定标签。
3. **测试覆盖窄**：唯一的单元测试没有覆盖以下几种：
   - SPA 同文档、只改 URL；
   - revision 变化后拒绝；
   - 页面未变时的标注；
   - 第二跳跳转。

**未读、未测**
- `native_browser_host::observe` 在页面自发跳转时会不会递增 revision。I/J 的实测说明那两种场景下不会，但其它跳转类型没有核实。
- 控制器怎样调用 `finish` 和 `ensure_fresh`。
- 多跳跳转和 SPA 二次改写的真实场景，没有实测。