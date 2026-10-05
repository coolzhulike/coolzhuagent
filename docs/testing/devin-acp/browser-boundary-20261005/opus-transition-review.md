Grep output is noisy; reading the adapter head directly.Checking which paths bump navigation_revision (line 700 is within on_navigation/page-load region?).**结论：方向可以采纳，没有明显更小又不漏洞的方案。但有四处必须补上，其中 2、3 两项按你现在的写法会直接出问题。**

我只读了 438f6ec 快照，主工作树里的改动没读到。本轮宿主只读调用共 10 次（grep 4、read 6）。

**源码核实了什么**
- `browser_panel.rs` 只在四处递增 navigation_revision：
  - 宿主 Navigate（192 行）
  - 面板换地址（700 行）
  - 弹窗及其取消（394、435 行）
- 页内 location 跳转只经过 on_navigation（762 行），revision 不变。所以你的"revision 必须一致"条件和 H 的情况对得上。
- 确实如你所说，`execute_authorized` 不检查页面 URL。这个改动只放宽最终观察时的 URL 约束，不扩权限。

**必须补的地方**
1. **证明的是"输入之后文档变了"，不是"跳转由输入引起"。** 用户在面板页内手点链接、页面定时跳转，revision 都不变，宿主区分不出来。最小的补救：第一版只接受同源跳转，跨源仍然按现在的规则拒绝。H 是 localhost，不受影响。这一项必须作为剩余风险写进文档。
2. **必须和 AuthorizedNavigation 共用"起始 URL 链"。** 依据是 `native_browser_adapter.rs` 123 行：Navigate 只在 `old.matches_page(expected)` 成立时才继承 original_url，否则取 `expected.url`。如果单独开一个槽位：
   - "点击跳转 → 再 Navigate"时，起始 URL 会被改写成中间页，最终验收失败；
   - "Navigate → 点击跳转"时，`matches_page` 因为 URL 不同而失败。

   建议：保留 AuthorizedNavigation 的协议不变，新增一个独立结构，但两边通过同一个辅助函数推导 original_url，并互相识别对方作为来源。
3. **源记录要求两个条件之一：** 来源 URL 等于 requested，或者来源本身就是上一个已认证的终点（终点用 document_token 匹配）。否则会出现"从错误页面起跳"的情况。
4. **执行时还不知道终点，要分两段写：**
   - 执行阶段只登记待定来源；
   - 第一次观察到"同资源但 token 不同"的快照时，才把终点 URL 和 token 定下来；
   - 之后每次 execute 都要先清掉旧的待定来源。
   - 适用的动作只有 Click 和 TextInput。这个适配器没有 key 类动作，Scroll 不登记。

**比较小、但我不推荐的替代方案：** 用一个运行级标志"本轮有过已结算输入"就放宽 URL 检查。它不绑定来源文档，验收粒度太粗。

**未读或未测**
- `observe_navigation` 内部和 `document_token` 的生成方式（同 URL 重载时 token 会不会变）。
- `finish` 有没有对第二次 fresh 观察也调用 `observed_page`。fresh 快照必须带上同一份转移证明，否则第二次观察还会被拒。
- 弹窗类跳转会递增 revision，按这个设计会被拒。这是保守行为，需要用真实页面确认。

**真实验收门槛**
1. H 重跑，最终 verified，并且引用了目标页节点里的原文。
2. 跨源跳转仍然被拒。
3. 跳转后主会话在面板换一次地址，不能通过。
4. "点击跳转 → Navigate"的链路能通过。

单元测试只能证明规则写对了，不能代替这四项真实验收。