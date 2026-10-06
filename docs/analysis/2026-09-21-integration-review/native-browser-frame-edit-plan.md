# 同源子Frame编辑与键盘实施方案

0.2.81正式安装版已验收同进程Frame独立click。编辑/键盘仍有显式顶层限制，观察也不给子textbox/searchbox/combobox引用。这是Agent能力缺口，不能归为SWE绘制能力不足。本轮由主会话实施与真实SWE验收；不调用受限Opus或子Agent。

## 方案与风险判断

复用现有DocumentScope、节点引用、文档/几何/命中预检以及原输入许可、期限和释放台账，不另建会话或Frame执行系统。观察增加同进程子编辑控件引用，子RootWebArea仍不提供滚动/导航操作。输入仍使用固定Input.insertText与既有页面单键down/up，不写DOM值，不自动聚焦。

仅检查子文档activeElement不够：焦点必须沿实际父Frame链到达目标，不能仅凭子文档的局部状态推定。初始固定函数遍历Window.frameElement焦点链，但真实浏览器的throwOnSideEffect明确拒绝（诊断候选EDIT4）。最终使用浏览器内建Document.hasFocus()核整个Frame链的实际焦点，再核目标是其document.activeElement；宿主DocumentScope仍核Frame/loader/root/owner身份。网页/模型不能提供脚本或Frame身份。最后读回调再次核焦点链及原编辑状态才派发，不在失败后补发。父焦点变化、子导航/替换和目标失配应零输入拒绝。

层数上限沿用既有四层。编辑不再跨Window读取frameElement或传递不同执行世界的对象。先实操同源、同进程编辑/键盘；跨来源和独立进程仍须分别验收，不能由同源结果外推。保留文本长度、类型、选区、只读/禁用/口令等既有限制，不改变正式UI。

官方依据：[Document.activeElement](https://developer.mozilla.org/en-US/docs/Web/API/Document/activeElement)说明iframe内焦点在外层文档表现为iframe元素；[Runtime.callFunctionOn](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#method-callFunctionOn)支持宿主固定参数与throwOnSideEffect。本轮只读取既有节点对象，不使用执行任意页面脚本的公共接口。

## 实施与验收

1. observation提供子编辑引用和唯一focused_node_index；editor/key移除顶层硬限制，固定读函数核实际文档焦点与控件状态，最终派发复用同一读函数。
2. 离线构建及现有桌面壳针对性检查；不增加镜像实现的测试堆积或模型回复夹具。
3. 原SWE-2-medium / veiled-anise实际同源子输入框点击、中文/ASCII文本与Enter提交；独立网页记录trusted输入/键盘事件，子结果正确、父未写入，模型会话正常终态排空，前后原生截图。
4. 子Frame更换文档及父焦点迁走分别验证旧请求零投递，不把网页最后文字或模型自述代替派发/释放事实。严格时序未命中保持缺口，不添加宿主延迟。
5. 新产品必须正常打包安装后复测；候选和正式分别归档。0.2.81包及既有通过证据保持原身份。

跨进程Frame、子滚动、严格按住跨URL导航/面板关闭及其它总体队列仍开放，不能由本轮编辑结果外推。

## 候选闭环，正式安装待复测

最终原SWE-2/veiled-anise四项候选已通过：三步子click/text/Enter一次真实提交、顶层编辑、真实规划期间父焦点迁走零文字投递、真实规划期间仅子导航旧text引用零投递。此前五轮失败与每次候选身份分别保留；不把输入ACK当表单成功。70项桌面壳、1390项Web既有回归通过。正式0.2.82打包安装及相同四项复测待完成，见[改动报告与原始截图](../../testing/release-0.2.82/change-report-and-test-handoff.md)。

## 深层表单采样修补

首轮真实SWE请求BU-CANDIDATE-082-FRAME-EDIT-20261006在39.8秒结束，target_not_found，steps0、网页零输入。DOM.getDocument原深度8漏掉iframe内form/p/label/input，AX虽可见，控件未获得操作引用；不能归为模型能力问题。固定采样深度改16，仍保留节点与响应预算，不使用无限深度；childNodeCount大于实际采得children时报告truncated，不能声称观察完整。深度预算之外的网页仍需后续按需子树设计，不由此推定兼容全部HTML。

## 只读焦点检查的实现调整

EDIT2点击已释放，随后文本零投递，editor_unavailable；独立诊断EDIT4明确返回focus_read_side_effect，不能把它记成授权不足。保持throwOnSideEffect=true，最终用Document.hasFocus()替代跨Window遍历。Chromium document.idl明确标注Affects=Nothing；Document::hasFocus转给FocusController::IsDocumentFocused，后者核页面active/focused及实际focused_frame是否是该document的后代。此调整复用浏览器自身焦点职责，不关闭副作用保护、不自动聚焦、不给模型提供脚本。

来源：[Document IDL](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/dom/document.idl)、[FocusController实现](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/page/focus_controller.cc)。实际候选及正式结果仍以截图和原始回执补齐。

## Enter默认动作修补

EDIT5已真实完成click、text和Enter down/up，steps3、子文本竹林-82/父空，但子提交0；1分36秒budget_exhausted，目标未达成。原rawKeyDown不携带字符阶段，浏览器未触发表单隐式提交。Enter改为Input.dispatchKeyEvent keyDown并附text/unmodifiedText回车，仍只派发一组按下/释放；其它页面键沿用rawKeyDown。不点击提交按钮代验、不write DOM或伪造submit事件；须由真实SWE和trusted网页submit事实验证。协议依据：[Input.dispatchKeyEvent](https://chromedevtools.github.io/devtools-protocol/tot/Input/#method-dispatchKeyEvent)。
