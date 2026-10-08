# Browser 输入时序观测方案及审查取舍

目标是补齐严格按下/释放与原生 Target 世代变化、跨来源文档变化的证据，不改变实际输入行为来制造通过。既有 094–101 证据分别保留，页面正常导航成功、释放已确认和严格窗口命中不能混算。

## 当前代码与边界

`native_browser_input::click` 在同一 UI 闭包中向原 controller 顺序入队 mousePressed、mouseReleased，不等 down ACK 后才发 up。双 ACK 决定 Released，入队失败或 3 秒回执超时为 ReleaseUnknown，不重放。`retire_view` 先隐藏旧视图，再等待执行门退出才关闭。这个结构保证尝试向旧 controller 释放，但 controller 可跨文档导航，不能据此证明旧文档接收 up。

`browser_panel` 已有默认关闭的 `COOLZHU_BROWSER_NAV_DIAGNOSTICS` 导航诊断，不记录 URL 全串、query、scope。generation 在 invalidate 和新建视图时递增；同视图导航通常不增 generation。`native_browser_source` 已监听 SourceChanged，新的 IsNewDocument 事件当前只早退。

官方[导航事件说明](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/navigation-events)和[SourceChanged 说明](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.sourcechanged)没有给出可直接替代精确网络 commit 的时点。SourceChanged/加载回调只记其实际事件名，不标为 commit 上下界，不把 ACK 时刻当 DOM 回调时刻。

## 最小实现

复用现有 logger、开关和诊断文件，不新增前端控件、模型 API、桥接命令、授权协议或遥测服务。提供宿主进程内共享单调时钟与序号；序号只表示采样点分配顺序，跨线程采样不等于事件因果。

click 仅在开关开启时建立有界内存记录：attempt ID、原 generation、down/up 入队前后时间、入队返回、各 ACK 实际采样时间和真假、最终结算。点击阶段不写磁盘；结算后一次输出。缺 ACK/超时仍为未知；结算后到达的 ACK 不更改已返回结果。默认关闭时不分配记录、不采时间。

generation 变更在状态锁内采变更前后两个点，锁外写日志；真实变更只能落在这两个采样之间。现有导航日志及 SourceChanged(IsNewDocument) 使用同一时钟，保持既有功能及早退逻辑。字段不含坐标、URL、query、正文、scope、凭据或页面脚本；attempt ID 仅关联本机工具调用。

不添加人工 delay，不改 3 秒等待，不阻塞 up 等 down 回调。诊断仍有内存分配/采时/结算日志的扰动，不能声明风险为零；真实命中必须把这一条件注明。没有足够证据时维持缺口，不能从日志缺失推断事件未发生。

## 模型审查与验收

正式101原有 SWE-2-medium / island-kayak 本页 #683/#684 已审查实际代码，见[审查原记录](../../testing/release-0.2.101/installed-validation/integration-result.json)。接受复用既有诊断、不加模块、区分 ACK 与 DOM、正常关闭重开可遇非必现的建议。拒绝“风险无”、SourceChanged/ContentLoading 可直接当 commit 上下界，以及模型口述 executed=true 作为原始审计。

实施后 offline build 桌面壳；只补少量关键不变式检查，不扩张无效单元测试。真实 SWE 单次点击正常测试页面，独立页面记录 down/up/click/pagehide/新页面事件；主会话通过正常 UI 关闭、重开或导航，截图证明软件状态。比较共享诊断与页面事实，明确命中或未命中；新页面没有旧输入作为独立必要条件。遇 ReleaseUnknown 保留未知并按既有安全流程，不自动重放、不清历史。Paint 免测、微信不动、Opus暂停、不新建云端测试会话。
