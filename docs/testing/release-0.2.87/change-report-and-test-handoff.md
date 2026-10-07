# 0.2.87 改动报告与针对性测试交接

本版修复当前用户明确说“内置原生浏览器”仍误选外部扩展路径的问题。仅补齐当前轮明确目标的名称表，未扩大权限、从历史猜测目标或改变输入安全策略。Paint依用户要求退出后续测试，保留已有基本能力证据；微信不改不测，Opus暂停，仅主会话实施和验收。

## 交付身份

正常完整发布构建、六项发布门、Windows安装返回0、1150个安装文件长度与SHA256核验均通过。冻结产品源码`a74ed09e8bc8e88944550fe2e3fadc67b6060022`，来源快照`09965223aa6ceb7503064e7529fd6cc7f8b2ebb905bf62b052eb8ad8d2931886`。MSI 276327042字节，SHA256 `cf6cb98d1fd6afd485842bff5d5d1977af8263c088936ae914cc170af54786e8`。未使用SkipBuild，未把候选或旧版结果追认为本次安装版通过。

本地安装包同时位于`C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.87.msi`。实际Program Files主控制台摘要`c64e64d666f383484d3757ead08affc57f4b94a68055a6eccf87823a5feef0a5`，桌面壳摘要`8c54e077183ef9120ae56591759d764a45b3bf463c1989f5a64db94d8462353d`。完整身份、原图、回执和逐文件证据摘要见[安装版证据清单](installed-evidence/manifest.json)。

## 正式版真实浏览器实操

用例`BU-INSTALLED-087-NATIVE-ALIAS-20261007`经正常聊天室发送，只使用此前误路由的“当前右栏内置原生浏览器”表述，目标为跨来源独立进程iframe的子输入。真实SWE-2-medium依次点击、输入INSTALLED087、按Enter；三步均sent，点击和按键released，文字输入不需要单独释放。子输入/提交1/1，父输入/提交0/0，三条标准均引用当前页面对应原文并grounded；外层completed，聊天室显示1分26秒。

[操作前](installed-evidence/installed087-before.jpg)与[操作后](installed-evidence/installed087-after.jpg)均为正式安装版实际窗口截图。四条可信网页事件与[动作事实](installed-evidence/BU-INSTALLED-087-NATIVE-ALIAS-20261007-facts.json)交叉核验，不以模型自述或ACK替代输入事实。

原聊天室`room-1791131523339`、Agent `session-1791131217833`保持。唯一远端仍为`island-kayak`；仅一个ACP attempt `e8e567875eea97dc641fa3392aac0dac5f126d87f8c6f468`，end_turn、进程排空、锁释放，内部lane远端为空。详见[单会话事实](installed-evidence/BU-INSTALLED-087-NATIVE-ALIAS-20261007-single-facts.json)。没有新建多个云端测试会话、改换Qwen或调用受限Opus。

## 检查与正常日常启动

实际offline build通过；空闲全量主控制台检查1396通过、0失败、6既有忽略，另lib8/native-host1通过。首次并行检查因与真实输入任务争用协调锁而失败，原日志和排空后全量重跑均保留在[别名修补记录](../2026-10-07-native-browser-alias/change-report.md)，未修改生产安全流程掩盖。冻结产品源码的[PR检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/37578950498/job/112654028390)和[push检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/37578946048/job/112654015536)均success，文档后续HEAD另核。

测试终态后按PID、路径、精确UTC启动时间与SHA核验并停止自有测试配套，正常桌面入口已恢复0.2.87。自检确认原工程`C:/Users/zhupu/coolzhuagent`和原安全库一致，见[恢复核验](installed-evidence/restored-daily-087-verification.json)、[日常窗口](installed-evidence/restored-daily087.jpg)。日常界面保留用户原Qwen聊天室历史及选择；本次未向Qwen发送新测试。资源safe，历史2个outcome_unknown许可和9个closed隔离项保持，未清库或绕过恢复。

## 其它界面复核及范围

本轮另在别名候选环境复核DSH真实目录4414项、calculator搜索3项、插件详情及固定来源检查，来源`@deepseek-ai/dsh-tool-calculator`、0.0.1/MIT、commit `b2007a13f06bcf75bf07b9d277ee8d434a316490`、22文件。没有新增安装或改变启用状态；安装/卸载/默认停用/启用及真实调用以076/077正式记录为准。

安装087前，正常日常086界面的模型设置载入、真实历史统计和升级检查已实拍。升级检查结束为no_published_release且按钮恢复，符合仓库只有预发布、无正式稳定版的实际状态；未卡在检查中。统计为真实记录200次请求、输入2743131/output163958、缓存1294080，不推算未知项。截图和原始状态见[本轮补验清单](../2026-10-07-native-browser-alias/evidence/manifest.json)。这些是明确版本的界面补验，不冒称087全功能重新执行。

其它模型设计测试时，先核包/进程身份、原远端绑定和页面空计数；只发独立新任务，按当前schema响应同job/request，失败/取消/未知不得重放。分别检查命名路由、真实子输入/提交、父页零输入、逐项原文grounding、单attempt与终态排空；不得把保留预览、页面渲染或旧版用例替代当前模型执行。

本版闭环名称识别修补，不代表所有HTML、复杂变换或四项总体全部完成。多层/裁剪/旋转、同名兄弟、横向RTL、严格按住期间跨URL/面板变化、多屏、插件部分配置/取消/超时边界、Goal/Relay附件及启动演出其它模式仍按[当前队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)分别登记。当前升级产品只有检查及发布页入口，自动下载/安装/重启链尚未交付，不能因本次手动正常安装而计为自动升级通过。
