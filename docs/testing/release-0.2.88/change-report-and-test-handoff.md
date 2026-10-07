# 0.2.88 改动报告与针对性测试交接

本版补齐内置原生浏览器子文档的横向滚动，并修复RTL子文档命中坐标错误。实现与验收由主会话独立完成；真实模型使用原SWE-2-medium/唯一island-kayak，未换Qwen、调用受限Opus或新建多个云端会话。Paint依用户要求不再测试，微信保持原功能。

## 交付身份与实现

最终冻结源码 `6047bb3c7462e106572498ae02028072abbaee8b`，来源快照 `acdd8cce47b0e60985ed7149579820054d1c719f7c08c3e52c868cf36b08cbf2`。正常完整发布构建六门通过；Windows正常安装及1150文件长度/SHA一致。MSI 276339330字节，SHA256 `17478e8a20504df6aa2cbb2c16727ee2796b9d8c26705fa59d6f45896768240c`。包位于 `C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.88.msi`。完整产物身份见[安装核验](installed-horizontal/installed-088-verification.json)。首轮5571c63构建发现能力提示尚未同步，未安装/交付/上传；最终仅以6047bb3重新构建、安装和复验，不能混用首轮摘要。

- `native_browser_scroll.rs`固定只读函数读取scrollWidth/scrollLeft及direction/writingMode。横排LTR区间[0,range]，RTL区间[-range,0]，按物理Left/Right计算剩余距离；复用原wheel和许可，不设置网页滚动属性、不执行模型脚本。
- `native_browser_frame_geometry.rs`归一化OOP局部坐标时，尺寸来自LayoutViewport，pageX/Y来自VisualViewport的真实本frame滚动偏移。旧代码在RTL初始位置0时误用布局原点1409，把子视口中心196算为1605，导致No node found at given location。
- `native_browser_target.rs`接受i32范围内的有符号文档命中坐标，真实输入点仍须处于视口内且通过原文档、节点及父owner归属/遮挡检查。没有增加输入权限、绕过释放或清除安全历史。
- `native_browser_adapter.rs`同步提供给模型的子文档能力提示。正式前端没有新增调试信息；定位用的候选数字诊断已移除。

设计决策及Chromium官方依据见[方案](../../analysis/2026-09-21-integration-review/native-browser-horizontal-scroll-plan.md)。本地实际offline桌面build通过、74项既有及必要数字边界检查通过；这些检查只作回归，不替代下列真实模型实操。

## 正常安装版六轮实操

原聊天室 `room-1791131523339`、Agent `session-1791131217833`保持。通过正常聊天输入发送每轮新请求，明确当前右栏原生浏览器URL、子RootWebArea、max_actions=1，不补发失败任务。父127.0.0.1嵌入localhost独立进程子页；父子页面均记录可信wheel/scroll。每轮只有一个ACP attempt，end_turn、排空和绑定锁释放，内部lane远端为空。

| 独立用例 | 真实页面结果 | 完成动作 | 窗口原图 |
| --- | --- | --- | --- |
| ltr-right | 子位置 195.3333282470703；deltaX 195.5 | 1 | [截图](installed-horizontal/installed-ltr-right-after.jpg) |
| ltr-left | 子位置 0；deltaX -195.33333333333334 | 1 | [截图](installed-horizontal/installed-ltr-left-after.jpg) |
| ltr-left-boundary | 零投递、事件0 | 0 | [截图](installed-horizontal/installed-ltr-left-boundary-after.jpg) |
| rtl-left | 子位置 -390.6666564941406；deltaX -391 | 1 | [截图](installed-horizontal/installed-rtl-left-after.jpg) |
| rtl-right | 子位置 0；deltaX 390.6666666666667 | 1 | [截图](installed-horizontal/installed-rtl-right-after.jpg) |
| rtl-right-boundary | 零投递、事件0 | 0 | [截图](installed-horizontal/installed-rtl-right-boundary-after.jpg) |

四项正向均真实一次wheel、子滚动生效并由当前页面原文验收；父横向位置及父滚轮次数全程0。两项边界按预期blocked，not_sent/not_needed、完成动作0、可信事件0，不能写成正向succeeded。RTL反向返回从负位置开始，覆盖有符号文档坐标命中路径。逐轮请求、回执、原文和事件见[安装证据清单](installed-horizontal/manifest.json)。

## 原失败与候选范围

候选11轮包括5项正向、3项边界、3项保留失败。首个LTR请求遗漏target.url，在观察阶段拒绝、零事件；补URL的新独立请求正向通过。RTL向左原失败与固定数字诊断再次失败均为not_sent、事件0；修补后RTL向左、继续向左、负坐标反向返回及右边界分别验证。不能把原失败追改为成功，早期LTR候选也不能替代最终安装版。见[候选原图与事实](candidate-horizontal/manifest.json)。

RTL-left正式回复曾使用“左滚到底”描述−390.67，但页面总横向范围更大；真实回执只确认一次左滚生效，不能据模型措辞认定已覆盖RTL最左端。本版仅覆盖LTR左端原点及RTL右端原点边界，另一端极限仍须后续单独验收。

“当前内置页面”而无target.url的自然语言流程仍需审视，不因明确URL的六轮通过而宣称闭环。更复杂旋转/斜切/透视owner、严格pointerdown跨URL及按下中面板变化、插件部分配置/取消/超时边界、Goal/Relay附件和启动演出其它模式仍按[当前队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)登记；不外推所有HTML自动化或四项总体完成。自动下载安装/重启仍为单独延期项。

## 其它模型复测设计要点

先核当前安装源码/摘要与活动进程，再确认原聊天室、模型、唯一云端绑定及父子初始计数。按LTR right→left→left边界、RTL left(amount=2)→right(amount=2)→right边界顺序发送六个独立请求，每次仅一个新dom_ref滚动动作，等待终态再下一轮；不要跨失败重放旧引用。

比较网页可信事件、输入回执和窗口截图三者：方向及前后位置应匹配；父页不能受影响；边界必须零新增wheel；模型判成功必须引用本次页面原文，不能靠ACK。改变direction/writingMode、子文档替换或owner遮挡后必须重新观察，不允许原票据继续投递。非horizontal-tb横向仍明确拒绝，不能写成已支持竖排滚动。

验收结束后按完整进程身份停止自有测试配套，正常桌面入口恢复0.2.88、原工程及原安全库。[恢复事实](startup-recovery/restored-daily-088-verification.json)及[正常界面](startup-recovery/restored-daily088.jpg)。日常工程原先选中的Qwen仅恢复显示，本轮六项测试均为SWE-2，未向Qwen发新请求。历史2个outcome_unknown许可与9个closed记录保持，资源safe且接受新输入；未清库或绕过恢复。

## GitHub交付核验

[0.2.88公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.88)四资产均已上传，服务端长度/SHA256与本地相同；实际tag绑定冻结6047源码。见[服务端核验](evidence/github-release-verification.json)。冻结产品两路CI success，后续证据提交的CI独立核验，不用产品来源结果外推当前HEAD。PR #84保持开放，未自动合并。
