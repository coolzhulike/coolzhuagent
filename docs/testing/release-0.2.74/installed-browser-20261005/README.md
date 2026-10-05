# 0.2.74 已安装版本 Browser Use 回归

日期：2026-10-05。标准MSI安装返回0，Windows注册版本与CLI均为0.2.74；Program Files中的Web/Shell摘要与交付载荷一致，源码为 `e52332d841523a57f8e27e25d8fe7399de88ee4d`。正式launcher在原日常工作区自检通过后，正常关闭日常外壳，在既有独立验收工作区运行**已安装二进制**。原输入安全库、原SWE验收聊天室及完全访问授权保留；没有清库或替换成源码候选。

## 实际结果

| 项目 | 宿主事实 | 原始软件截图 |
|---|---|---|
| 慢popup加载中接管 | 66.4秒，click→Navigate→click，3步，2/2、goal=true；旧120秒响应disconnected后目标未被夺回 | [源页](popup-final-before.png)、[晚响应后](popup-final-after-late-response.png) |
| 初始慢页面接管 | 61.5秒，Navigate→click，2步，2/2、goal=true；加载中不给网页输入资格 | [真实加载中](loading-final-before.png)、[接管成功](loading-final-after.png) |
| 两段Navigate资源链 | 53.8秒，源页Navigate慢页→新loading引用Navigate目标→click，3步，2/2、goal=true；没有跳过中间慢页 | [源页](nav-slow-chain-before.png)、[目标页](nav-slow-chain-after.png) |
| 原生125%缩放表单 | 113.4秒，click→text→scroll→click→click，5步，1/1、goal=true；DPR=1.875、scale=1、pageY=796.27，姓名仅输入一次 | [缩放前态](zoom-form2-before.png)、[真实操作](zoom-form2-running.png)、[提交成功](zoom-form2-after.png) |
| 鼠标按下时节点替换 | 42.0秒，仅1次click，1/1、goal=true；down→节点替换→up，孤立down/up均0 | [替换前](replace-before.png)、[替换后](replace-after.png) |

上述耗时来自运行台账，HTTP往返计时略长，二者不混用。五轮共31项真实Devin ACP请求，requested/effective全部 `swe-2-medium`，全部terminal/drained，最终未结算0；14项动作均实际sent/released，没有未知释放或重发。`resolved_model=null`，不宣称另有服务端解析模型证明。普通HTML服务不是模型夹具，模型自己的Navigate、click、文本和滚动均由宿主真实派发；主会话仅预置地址、缩放及空白处焦点，不代完成目标动作。

节点替换真实事件time：down=1791210787081.3，replace=1791210787081.8，up=1791210787082.7002。替换确实发生在释放前。网页事件按客户端time排序，JSONL服务器收包行序可能不同。125%只适用于表单；后续导航将比例复位到默认，节点替换为DPR1.5，不能写全部测试125%。表单空白处的辅助焦点点击单独计入预置，不属于模型五步。

每轮request/response/reply、运行facts、输入details、[结构化汇总](regression-summary.json)和[网页事件汇总](page-event-summary.json)独立归档。after截图左侧仍为此前源码候选旧回复，只证明本轮右栏软件目标，不当作同轮回复联合证据。外部API提交内容已持久化；正常重开控制台后，最新SWE模型回复在聊天室可见，见[回复可见性](final-reply-visible.png)。`nav-slow-chain-during.png`采样时已经进入明确目标，不能称它捕获了中间slow加载前态；中间加载依据真实观察、导航回执和网页事件核验。

## 结论与未覆盖项

本轮加载期自主接管阻塞、两段导航资源链及125%表单验收缺口已闭环，Browser Use基本执行链路通过。此前正式0.2.73的普通表单、同源/跨源、历史按钮、普通Stop后新任务及Compute Use基础Paint结果沿用上一版原始记录，不改写为0.2.74重新执行。

源码候选三次严格整页切换竞争均未命中（pagehide晚于up），已按Opus有界尝试决定停止刷试，本轮不追加人为延迟释放。多屏受单活动显示器限制仍未覆盖；更多缩放比例、微区间停止/关闭和未开放的浏览器动作种类仍不作全量声明。这些兼容性/竞争边界明确保留，不以普通导航或节点替换替代整页竞争证据。

独立诊断问题仍保留：验收配置的provider健康计数0/3与真实Devin调用成功并存；启动日志另有历史 `kept_isolated` 操作不得改判为recovered的结账提示，日志明确不影响opening结果，本轮31项请求均正常结束。没有删记录或把它宣布修好；日志提示不添加到正式前端调试栏。它们不属于本轮已关闭的浏览器输入阻塞，后续独立核查。

## 复现与收尾

用原始HTML启动新服务，替换各request中的端口，在相同已安装二进制和模型会话中建立**新任务**，不能重放旧call_id或复用旧成功页面。先核原安全库和实际聊天室权限；加载引用只用于Navigate，就绪后重新观察文档才能操作控件。任何输入或释放未知立即停止，不通过清库恢复。

原始截图按字节归档，`manifest.json`列长度与SHA256；不包含数据库或API密钥。最终正常关闭验收窗口、退出自有验收后台和三个测试服务，恢复原GUI地址文件及正式launcher日常工作区；见[收尾收据](cleanup-receipt.json)与[日常启动自检](daily-restored-selfcheck.json)。自检ok=true，日常Web/Shell为新版Program Files二进制。原日常Qwen选择只属环境恢复，不用于本轮验收。本报告与安装包源码身份分开，后续文档提交不会改变本包构建提交。
