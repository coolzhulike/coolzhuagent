# 同源子Frame滚动实施与验收方案

0.2.82已正式验证子编辑与Enter。下一缺口位于桌面壳：observation不提供子RootWebArea引用，target::verify_viewport明确要求顶层，wheel也按父viewport计算距离。它是Agent执行接口限制，不是SWE模型能力问题。

本阶段仅补同源同进程子文档的上下滚动，复用DocumentScope、现有一次性输入许可/票据与wheel事件，不建立第二套Frame执行器，不增加正式前端诊断控件。主会话实施与原SWE实操，Opus暂停，不用子Agent。

文档模块继续核Frame/loader/DOM根/owner链。观察模块提供子RootWebArea引用；滚动预检单独负责真实滚动根状态、owner的视口几何和命中Frame。导航仍只允许顶层。父页面滚动规则保持原路径，不能误借子Frame引用导航或点击。

子文档不能直接复用父viewport中心：宿主从最后一级iframe owner的content quad生成中心，并以真实HitTest的Frame和节点归属证明命中选定子文档。部分可见但中心被裁剪/遮挡时明确拒绝，后续按需改进可见区域选择，不能把出界坐标钳制到其它目标。

浏览器wheel可能在子页面边缘继续滚动父页面。固定只读函数读取该Document.scrollingElement的scrollTop、scrollHeight和clientHeight；无余量时零投递，距离不超过本轮子余量。它不写DOM、不设置scrollTop、不自动聚焦，也不给网页/模型提供脚本接口。保留throwOnSideEffect；实际浏览器如拒绝读函数，按原异常事实修正读取方式，不关闭保护。

准备/执行都复核文档、几何、滚动状态与命中，沿用原资源、期限和输入台账。只读状态仅宿主短期使用；最终输入仍由既有wheel路径发送。事件ACK不证明子页面已滚动，真实独立scroll/wheel事件、位置变化和截图才作为效果证据。

依据：[CSSOM View scrollingElement](https://drafts.csswg.org/cssom-view/#dom-document-scrollingelement)规定实际文档滚动根；[DOM.getBoxModel及getNodeForLocation](https://chromedevtools.github.io/devtools-protocol/tot/DOM/)提供宿主几何和命中Frame。协议调用固定在宿主，模型只选择新鲜节点引用。

真实验收依次为子页面向下、父页面单独滚动、规划期间子导航旧滚动引用零投递、子已到底时零投递且父位置不变。严格时序未命中保留诊断，不增加宿主延迟，不补发失败请求。先候选，再正常打包安装复测；0.2.82原包、截图及正常启动事实保持原身份。

横向/RTL、跨来源/跨进程、嵌套滚动容器、变换/局部裁剪、按住跨URL或面板变化及总体队列不能由本轮外推。此方案的限制需要实际验收记录持续补充。
