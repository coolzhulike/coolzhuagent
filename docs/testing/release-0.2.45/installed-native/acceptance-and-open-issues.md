# 正式045 Browser Use实操与遗留项

2026-10-01，主会话独立执行，原qwen3.8-flash、medium、百炼地址和已有密钥不变。未使用模型响应夹具、未代模型滚动、未直接写安全数据库。Windows Installer正常安装返回0，完整包及安装身份通过，见[改动及测试设计报告](../change-report-and-targeted-test-plan.md)。

## 已完成

| 场景 | 实际结果 | 证据 |
|---|---|---|
| AL单次向下滚动 | 原房间当前URL独立任务，1步scroll、sent/not_needed/effect_observed/passed；page_y从0变为289.3333435058594，页面顶部显示289；父completed，CU succeeded/goal=true，43.3秒 | [事实](BU045-SCROLL-AL-facts.json)、[动作前](03-scroll-al-before.jpg)、[效果原图](05-scroll-al-observed.jpg)、[最终回复](06-scroll-al-result.jpg) |
| AM只读 | 本轮新原生观察读取标题、实际滚动位置；input_supported=false、attempts/steps/action_count均0；父completed，CU succeeded/goal=true，2/2，25秒；页面仍289 | [事实](BU045-READONLY-AM-facts.json)、[原图](07-readonly-am-result.jpg) |
| 正常重启 | AL、AM结束后标题栏退出并通过正式入口启动，10安装产物重新匹配，Shell30328/Web30716；safe、accepts=true、待恢复/未确认阻断/未确认遗留均0，已接受遗留2，没有新增审批 | [去敏恢复事实](restart-recovery-facts.json)、[安装身份](installed-artifacts.json)、[重启原图](08-normal-restart.jpg) |
| AN交互观察后关闭页面 | 原生S1/S2及初始验证已取得，规划1790823810302完成；1790823810568正常关闭右栏，原计划点击前重新观察未完成，最终blocked/execution/native_observation_timeout/goal=false；0实际输入、0步骤、not_sent/not_needed，无重试；父completed，35.3秒 | [关闭前账本](BU045-RESOURCE-AN-before-close.json)、[终态事实](BU045-RESOURCE-AN-facts.json)、[初始次数0](09-resource-an-before.jpg)、[关闭中原图](10-resource-an-close-during-verifying.jpg)、[结果原图](11-resource-an-result.jpg) |

旧样本57159服务已经停止；先恢复静态HTML服务至58420，核实际页面载入且位置0后才发送AL。连接失败图[保留](02-sample-unavailable-before-restart.jpg)，不算模型运行失败。样本只提供普通HTML，不模拟模型或工具回执。

AL初始验证8.223秒、规划5.647秒，本次未自然超过旧20秒边界。因此本轮证明正式045单次真实滚动链路通过；不能声称本轮实操还验证了超过20秒的等待。较慢23.138秒导致旧044过期的AK失败仍保留，120秒有界缓存另由既有工程回归覆盖。执行票据2秒、身份/AX/命中重检和动作后退役均未放宽。

## 未通过与后续

内置浏览器Type/Nav仍未实现；AN覆盖S1后关闭整个页面，安全拒绝边界通过，不能外推为点击已成功或所有资源变化通过。该次返回通用观察超时，未细分为页面关闭；supervisor.action_count=1为尝试计数，CU行action_count=0和not_sent才是实际派发口径。执行期间换房间/工程、隐藏/最小化及动作已派发后的变化需独立测试。DSH远程插件实际安装运行、Paint闭合轮廓/简易海绵宝宝及四项总体审核仍开放。不能把AL、AM及正常重启外推为完整Browser Use或四项总体验收通过。PR74保持Draft；4002a5b远端两项Web检查SUCCESS，之后新提交需另核。微信不改不测，Devin搁置，Pro复审按用户要求暂缓。

下一步继续设计并接入浏览器输入/导航及失效场景；再用真实Qwen联合验收Paint和Windows四边泛光/顶部提示/结束撤除。
