# 0.2.96 改动与针对性验收交接

## 本版改变与测试依据

修复通用工具审计投影把失败/超时误写成“未执行”的问题。失败可能发生在网络请求或其它副作用之后，不能由失败状态推导零执行，也不能据此自动重试。

`tool_dispatch_audit` 独立集中生成审计：实际授权来自 permission_gate；成功明确 executed=true，拒绝和只读明确 false，失败/超时保留 null。运行时和交接派发共用该投影；没有修改实际权限、安全隔离或历史记录。原动作层有明确事实的 Computer Use 回执继续保留其布尔事实。同步两条前端静态契约到当前玉石 v3/SVG 资源，保留按钮接线、流式清理及资源存在断言。

源码包含此前094复杂Browser快照分页/仿射进展、095同进程子滚动及统一控件；各独立正式验收证据见[094报告](../release-0.2.94/change-report-and-test-handoff.md)、[095报告](../release-0.2.95/change-report-and-test-handoff.md)。本版不重复简单任务。

## 构建与正式安装

- 冻结参考提交：572139a64bf9eef246cf696dd1ccd53f74b5146e，dirty=false；权威源码快照 eb93df587b80cee62e41eddb5e43b591d6735c6e32bf81eddd693db7ad2c14b7。正常完整release构建，六项发布门禁全部pass，无跳过打包。
- payload 1159文件、432446904字节；MSI 285744647字节，SHA256 a00e4dd2d1a7767f7f2d27f6e8020cdd83ccc78d5a38e6bc46c2b75cc9c22d99。未签名。
- 正常管理员安装返回0，Program Files中1159文件逐长度和SHA一致，实际CLI版本0.2.96。只在两个工作区均无活动任务后，按完整身份停止本轮095配套进程；原安全库及唯一远端绑定保留。
- [发布文件摘要](installed-validation/package-096-verification.json)、[安装摘要](installed-validation/installed-096-verification.json)、[安装收据](installed-validation/install-096-result.json)、[正式进程身份](installed-validation/installed-096-standard-processes.json)。
- [构建报告](evidence/build-identity/pkg-report-release-20261008-104312743-32e5fbcf/package-report.json)、[payload清单](evidence/build-identity/pkg-report-release-20261008-104312743-32e5fbcf/payload-inventory.json)、[归档身份](evidence/build-identity/pkg-report-release-20261008-104312743-32e5fbcf/package-report-files.json)。

## 真实模型专项复验

沿用SWE-2-medium与唯一island-kayak，通过当前聊天室发送任务，不新建云端会话。固定真实DSH net-tools提交与源摘要沿用095记录；正常启用并临时增加net_fetch白名单，受控HTTP服务仅等待真实请求，不替代模型或插件。

marker `PLUGIN-INSTALLED096-AUDIT-UNKNOWN-20261008`，run `run-chat-654b4a4fa1fdad673f5062146807be0c41370c79cfd1d253`：

- GET 2026-10-08T17:46:33.159732Z实际进入后约24.7ms开始正常停用插件；停用返回33.729Z。
- 对端34.347444Z关闭连接，等待约1.188秒，无响应体、无重试补发；单次工具台账failed/cancelled，elapsed_ms=1841，宿主审计allow-auto。
- 正式聊天回复摘录实际收到的审计字段：execute_requested=true、execute_allowed=true、executed=null，并明确报告取消、cleanup_confirmed=true；没有再写成从未执行。父run completed仅表示回复正常结束，不代表抓取成功。
- 单attempt正常end_turn、process_drained=1、绑定解锁，internal远端为空。结束后正常撤回本次工具白名单、插件保持停用、受控服务器正常关闭。

[结构化事实](installed-validation/audit-unknown-result.json)、[正式软件实拍](installed-validation/01-audit-unknown-final.jpg)。完整瞬时工具响应未独立持久化，截图和回复中的字段摘录是模型转述；原始网络事件、宿主审计、真实工具台账、已核验安装代码及针对实际派发入口的回归交叉验证，不伪造完整原始回执，也不外推独立Win32句柄与所有清理阶段均通过。

## 验证与边界

本地完整控制台1404通过/0失败/6既有忽略，最终offline build通过；workspace linkage8项与S0契约2项通过。构建提交572139a两路远端检查37818095486与37818104917均success，见[push检查](installed-validation/ci-push-572139a.json)及[PR检查](installed-validation/ci-pr-572139a.json)；旧图标断言失败记录保留，不跳过用例。

0.2.96已公开为[GitHub预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.96)。四份资产上传后逐一核对服务器端长度与SHA256，公开后再次核验，标签实际指向上述冻结构建提交；见[发布metadata](installed-validation/github-published-metadata.json)、[标签](installed-validation/github-tag.json)及[本地资产摘要](installed-validation/release-assets-local.json)。预发布保持原正式版本渠道策略，不冒称整体正式验收完成。

后续针对性测试应区分授权、派发、执行副作用及业务成功：失败/超时不能当作零执行或允许自动重放；拒绝/只读仍须为false；成功与明确CU动作事实不得因新的可空类型丢失。优先用真实执行中撤销或超时和可信外部事件验证，避免大量镜像单元用例。

095低高度1443×563的后续正式补验已通过，见[布局补验](../2026-10-08-ui-preview/upload-and-browser-progress.md)。096补验[903×897窄窗口](installed-validation/02-narrow-903x897.jpg)、[903×551窄低窗口](installed-validation/03-narrow-low-903x551.jpg)、[扩展栏](installed-validation/04-narrow-low-settings.jpg)及[内部滚动](installed-validation/05-narrow-low-settings-scroll.jpg)：Logo、模型/工程/聊天室切换、完全访问、快捷栏、输入区与底金线可见，配置右栏可内部滚动并正常关闭，窗口已恢复。仅覆盖所列尺寸的基本布局；高DPI、动态审批、新nativeTarget替换、跨来源commit窄时序、启动特殊模式与其余架构工作包仍开放，见[总体快照](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)。本次不宣称四项任务全部完成；Paint免测、微信不动、Opus暂停保持。
