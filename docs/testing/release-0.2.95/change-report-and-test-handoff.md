# 0.2.95 改动与针对性验收交接

## 本版改变

同进程iframe滚动时，中间步骤可能只有滚动位置变化，AX文字不变。原观察路径遗漏子文档自身视口，容易误判无进展。现在观察模块复用滚动模块固定宿主只读查询，读取自身scrollingElement；保留负RTL/小数位置，校验有限宽高，不接收任意页面脚本、不发送输入，不借用父视口。临时CDP对象沿现有释放路径处理，最终文档/资源身份复核保留。

该修复只涉及桌面壳的native_browser_observation/native_browser_scroll职责边界，没有把页面控制混入会话或模型配置模块。源码候选[两步真实SWE滚动](../2026-10-08-browser-sameprocess/report.md)已通过；[按下期间关闭候选](../2026-10-08-browser-panel-window/report.md)属于已有输入释放边界的专项补验，无新增产品测试钩子。

## 产物与安装身份

- 源码参考提交：75b0b6d09df0485677682f4d66ee649d30ba96e5；权威源码快照为4be789096a255a2127313d6eedc6b1b0ce668ccd62ce70054097b1c63f09a379。构建报告记录dirty=true，不能将HEAD冒充全部构建输入；完整声明范围与摘要以报告为准。
- 正常完整release构建，未SkipPackageBuild，六项打包资格门全部pass。1159文件、432446392字节payload；MSI 285748743字节，SHA256为8d23a06cb73274858e78d12865635109a04a542c9f198206fd8b853cb0f5d743。
- 正常管理员MSI安装返回0，Program Files中的1159文件长度及SHA256全部匹配；CLI实际版本0.2.95。原日常和验收库均无活动任务后，按PID/路径/创建时间/摘要仅停止本轮候选配套进程，再正常安装。
- 正式配套后台与桌面壳已启动，沿用原工程、原安全库、SWE-2-medium与唯一island-kayak；没有清安全记录、修改运行结果或新建云端会话。
- [安装摘要](installed-validation/installed-095-verification.json)、[安装收据](installed-validation/install-095-result.json)、[正式进程](installed-validation/installed-095-standard-processes.json)、[启动实拍](installed-validation/01-installed-started.jpg)。本版未签名，尚未上传GitHub。

完整[构建报告](evidence/build-identity/pkg-report-release-20261008-095509863-5ff4f516/package-report.json)及[安装清单](evidence/build-identity/pkg-report-release-20261008-095509863-5ff4f516/payload-inventory.json)可核对具体来源。

## 正式功能复验

本版三项独立真实SWE正式实操已通过：

- 同进程子滚动：两步sent/effect_observed/visible_progress=true，真实位置0→600→898.666687，父视口保持0；CU succeeded/goal_achieved=true，no_progress/replan均0。单远端attempt正常end_turn/drained、绑定解锁。[结果](installed-validation/sameprocess-result.json)、[完成实拍](installed-validation/04-installed-sameprocess-final.jpg)。
- 按下期间正常关闭：pointerdown=1791478998068.2ms，正常UI操作=1791478998177–1791478998269ms，pointerup=1791478999870.9ms，pagehide=1791478999881.1ms；UI操作完整位于按下与释放之间，实际视图结束在释放之后。宿主sent/released，随后native_browser_panel_unavailable明确停止，无重开/补发；CU blocked、业务目标未完成，这是预期负例，不冒领业务成功。[结果](installed-validation/close-result.json)、[UI区间](installed-validation/close-ui-interval.json)、[收尾实拍](installed-validation/06-installed-close-final.jpg)。UI区间不等同内部invalidate精确时间。
- 按下期间切换右栏：通过正常“设置”快捷按钮把浏览器替换为模型配置页，操作区间1791479274400–1791479274491ms，严格位于pointerdown=1791479274299.9ms与pointerup=1791479276102.1ms之间。旧动作sent/released，随后native_browser_panel_unavailable停止、无重开补发，单attempt正常收尾解锁。[结果](installed-validation/sidebar-replace-result.json)、[UI区间](installed-validation/sidebar-replace-ui-interval.json)、[最终实拍](installed-validation/08-installed-sidebar-replace-final.jpg)。这项证明右栏内容切换/原网页隐藏；不冒称创建了新原生浏览器Target或销毁了旧Target。

已在094正式通过的仿射9步长程、SKILL/插件/Browser综合流程、插件内外超时不重复简单测试；见[094交接](../release-0.2.94/change-report-and-test-handoff.md)。

低高度窗口补验尝试未计通过：Windows CU辅助工具两次将窗口相对边框坐标投影到非目标msedgewebview2，均在动作前拒绝；按规范激活/新截图后重试一次仍失败，未继续坐标重放。该工具限制不证明产品布局失败，也不能冒称低高度/DPI矩阵已通过；当前1443×897主尺寸截图仍有效。

## 针对性测试设计依据

1. 同进程子文档中间滚动：文字不变且父视口不动，子位置变化仍应visible_progress=true；到末端出现完成文字才允许目标完成。
2. 读失败/身份变化：不复用父LayoutMetrics或旧引用、不伪造视口；下一动作仍使用新鲜子RootWebArea引用。
3. 数值边界：RTL负横向位置及小数位置合法，非有限数、零宽高等拒绝；已有两项针对性回归通过，不用大量镜像单元用例代替真实实操。
4. 按下期间正常关闭：原已入队动作必须释放，结束资源不可补发或自动重开；业务目标可以blocked，不能把负例安全停止写成业务成功。精确区分UI操作区间与内部销毁时间。
5. 新原生Target替换以及跨来源commit恰在按下期间仍独立开放，不能用右栏内容切换或关闭负例代替。

其余未完成项见[总体快照](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)。本版不是四项整合任务整体完成声明；Paint免测、微信不动、Opus暂停、仅主会话执行的范围保持。
