# 子文档横向滚动实施方案（2026-10-07）

## 当前缺口与决策

0.2.87子文档滚动预检只读取scrollHeight/scrollTop，并显式拒绝Left/Right。原native_browser_input已有deltaX和有界单次wheel，协议也已包含四个方向，因此不新增工具、权限、输入入口或模型专属路径。主会话独立实施；Opus依用户要求暂停，不等待远端审核。

固定只读READ_SCROLL扩展scrollWidth/scrollLeft及真实滚动根的computed direction/writingMode。预检模块计算当前请求方向的剩余距离，继续把wheel_limit交给原投递模块；原文档/节点绑定、owner逐层几何与裁剪命中、二次状态重检、许可及终态规则不变。不得用设置scrollLeft或执行模型脚本代替真实wheel。

横排LTR的横向区间为[0,scrollWidth-clientWidth]，RTL为[-(scrollWidth-clientWidth),0]；Left取当前位置到下界的距离，Right取上界到当前位置的距离。RTL最右端为0、向左为负值，依据[MDN scrollLeft](https://developer.mozilla.org/en-US/docs/Web/API/Element/scrollLeft)和[CSSOM View](https://drafts.csswg.org/cssom-view/#dom-element-scrollleft)。有符号值必须校验有限性和当前区间。纵向已有逻辑保留，不把复杂竖排writingMode的符号域错误宣称为支持。

## 风险与验证

- RTL公式符号错误会拒绝正常向左或把边界输入链给父页：用实际横向LTR/RTL子页双向滚动及原点边界零投递核验，父页也记录真实滚动位置。
- 页面可覆盖JS读取：继续固定函数与throwOnSideEffect，不接受请求传脚本；读取异常沿原错误返回，不回退为猜测距离。
- 滚动根/方向在规划或预检期间变化：完整状态进入已有二次重检和票据匹配，不能复用旧状态发wheel。
- writingMode非horizontal-tb的横向请求先明确拒绝；旋转owner、按住期间导航和面板替换属于其它独立项，不扩大本次改动。
- 先实际offline编译桌面crate、运行必要数字边界检查，再使用原SWE-2-medium/唯一island-kayak在真实原生子页操作截图和事件核验。候选结果与正式安装包分开记录；原0.2.87资产不被替换。

验收证据包括：正式/候选身份、请求与真实模型、被选子RootWebArea、一次wheel回执、子页方向和前后位置、父页保持不变、RTL原点边界not_sent/事件0、单云端attempt排空及恢复原工程。没有对应实操前不得写成正式通过。

## 实操发现的RTL坐标问题及修补

候选LTR向右/向左及左端边界通过，但RTL合法向左被DOM.getNodeForLocation拒绝。保留原失败和独立诊断请求，均零投递，未将其记为预期负例。候选固定只读数字诊断证实：子视口391×284、cssLayoutViewport.pageX=1409，而cssVisualViewport.pageX=0；旧变换把中心196加1409得到1605，浏览器返回No node found at given location。

Chromium [InspectorPageAgent::getLayoutMetrics](https://raw.githubusercontent.com/chromium/chromium/main/third_party/blink/renderer/core/inspector/inspector_page_agent.cc)的LayoutViewport使用VisibleContentRect起点，VisualViewport的pageX/Y使用本frame的GetScrollOffset；[LocalFrameView::DocumentToFrame](https://raw.githubusercontent.com/chromium/chromium/main/third_party/blink/renderer/core/frame/local_frame_view.cc)减的也是真实GetScrollOffset。因此OOP局部metrics应使用LayoutViewport的clientWidth/clientHeight，结合VisualViewport的pageX/pageY，而不能把布局矩形原点当滚动偏移。

修补放在frame_geometry的既有局部metrics归一化内，所有OOP节点操作复用，不在scroll模块重复创建另一套坐标路径。hit_test_point接受有限且i32范围内的有符号文档坐标；鼠标输入坐标仍由已有视口内校验独立限制，并且必须通过原frame/backend节点归属和父owner命中，负数不是输入许可。临时数字诊断已移除，正式界面无新增调试入口。74项桌面检查通过，后续必须以修补候选及正常安装包的真实模型/网页事件/截图确定结果，不用数字检查代替实操。
