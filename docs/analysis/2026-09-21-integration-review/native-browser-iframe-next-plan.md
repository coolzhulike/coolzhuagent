# 内置浏览器 iframe 文档绑定的下一步实施方案

同进程子文档click已实施，0.2.81正常打包、安装及正式原SWE三项复测均已通过；1150文件摘要、正常入口恢复和产品源码两条远端检查也通过。跨进程子Target及子文档编辑/键盘仍未实施，不能由本轮点击结果外推。以下现状段描述0.2.80修补前的缺口。

## 本轮候选进展

新增native_browser_document小模块负责FrameTree、独立DOM根及owner链；nodes保存私有scope，observation有界逐Frame采样，target按目标子文档核对AX/DOM/几何/命中，input沿用完整身份检查与原释放规则。模型仍只拿随机引用，不新增CDP/script入口或权限系统。整体观察token包含已绑定Frame身份，子Frame导航使原观察失效；这会保守地撤销同轮其它目标引用，后续复杂动态Frame需据真实兼容性事实再判断，不延长原期限掩盖问题。

离线Web与桌面壳build通过，桌面壳既有及一项文档边界回归合计70通过。原SWE-2-medium/veiled-anise五轮记录：子点击36.9秒父0子1且sent/released；父误击31.1秒零输入；首次导航测试漏写右栏原生浏览器，实际走外部扩展路线并在观察前拒绝，保留为测试失败；第二次切换晚于点击释放，验证期间旧证明撤销；第三次规划期间仅子Frame导航，document_changed、steps0、not_sent，父子0，输入前旧引用拒绝通过。截图、真实事件与时间先后见[候选证据](../../testing/release-0.2.81/candidate-iframe/manifest.json)。候选不计正式验收；没有向远端塞模型回复夹具或添加宿主延迟。

## 正式完成范围

正式子按钮点击25.2秒父0子1、sent/released；父误击34.0秒零输入；真实规划期间仅子Frame导航后32.0秒document_changed、steps0/not_sent，旧引用输入前拒绝。原SWE-2-medium / veiled-anise终态排空与续接保持。生产者原始报告、安装包发布和实操截图见[081正式报告](../../testing/release-0.2.81/change-report-and-test-handoff.md)。后两项是预期拒绝，不是模型目标达成。候选失败原记录保留。跨进程与子编辑/键盘/滚动，以及按住期间导航仍未验收。

## 0.2.80历史现状与缺口

native_browser_observation只采顶层Frame的AX；native_browser_dom明确忽略contentDocument。native_browser_nodes的DocumentIdentity包含顶层frame_id/loader_id/backend_root，NodeBinding只有backend节点、角色和名称。执行与命中验证均把目标归于顶层frame。此设计保护已实现的顶层/开放Shadow输入，但无法为iframe内部提供独立可执行引用；不能通过放开frameId条件或把子文档挂进普通children来修复。

官方WebView2 ICoreWebView2_11说明跨来源iframe可能是独立DevTools target，需要附着目标并使用对应session调用；attach须flatten。来源：https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_11 。这证明同进程DOM树方案不能代表所有iframe。先完成实际同进程子文档，再处理跨进程子目标，分别报告支持范围。

## 责任划分与实施顺序

1. 新的小型document_scope模块负责宿主读取的顶层身份、父Frame链、子Frame身份和iframe owner backend节点映射；不接收网页/模型自报身份。沿用既有采样预算，不另建任务调度器或权限系统。
2. native_browser_nodes继续存储本轮引用，NodeBinding增加宿主私有DocumentScope。顶层page token不再被当作子文档身份；同Frame导航必须改变loader/root并使原引用失效。页面里相同名称、相同backend数字也必须带所属scope。
3. native_browser_observation逐一获取支持Frame的AX和对应DOM节点，将实际控件加入有界PageObservation。模型仍只得到不透明引用、节点角色和名字；不提供任意CDP方法、script或frame id执行入口。截断Frame单独无操作引用，不抛弃已核实顶层节点。
4. native_browser_target按binding的scope重新核对完整Frame/owner链、子文档loader/root、AX、节点及几何。HitTest必须命中同一子Frame的独立目标；命中父iframe元素或其它子Frame不能代替按钮。滚动/导航初期继续顶层现状；子Frame动作范围先仅click，后续编辑/键盘另验。
5. native_browser_input的same_target/执行前/释放后比对必须包含目标DocumentScope，不能仅靠顶层DocumentIdentity。子文档变化发生在按下后时继续完成释放，不能把新文档当成原目标成功；关闭/替换面板、取消与未知输入规则沿用。
6. 跨来源独立Target用宿主私有session表，限定实际iframe目标与父链，使用WebView2 ForSession固定读取方法。未关联worker/browser target不提供输入引用。输入坐标和命中模型需在真实WebView2核对，不假设所有session与顶层采用相同坐标。

## 正式真实SWE验收标准

- 同来源子Frame独立button一次真实click，页面子计数1、父0；before/after原图、事件trusted和sent/released回执、同远端终态排空对应。
- 父控件被iframe覆盖：零输入；不同Frame同名button必须只命中指定Frame，不自动改目标。
- 子Frame单独导航或替换：原引用失效，旧点击不补发。明确区分初始失效与输入期间变化，窄时序未命中继续记录开放。
- 跨来源/跨进程Frame另测试，不能由同来源或仅渲染替代。所有实际调用仍是SWE-2-medium / veiled-anise，不用模型回复夹具、Qwen、Opus或子Agent。
- 必须正常构建安装新版本后复测；候选只能记候选。已有普通/SVG/Shadow能力的针对性回归与合理代码检查保留，不扩大为无关单元测试堆积。

风险先处置：父Frame链变化、跨进程target脱附、Frame缩放/滚动/transform、嵌套预算、唯一目标和释放归属。遇到事实不支持的Frame给明确能力缺口；不通过强制外部浏览器、任意JS或延长安全租约掩盖问题。Opus暂停时由当前主会话完成架构审查，不请求额度恢复。
