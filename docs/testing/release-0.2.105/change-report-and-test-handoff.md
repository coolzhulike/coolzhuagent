# 正式0.2.105改动与测试交接

本版修复插件重新配置后旧轮拒绝只剩“409 Conflict”的问题：ACP桥保留已有API的具体原因，DSH在工具已登记但执行器尚未启动的拒绝阶段写入现有运行轨迹。正式105已安装，并由真实SWE-2-medium复验通过；104的部分通过与诊断缺口仍[独立保留](../release-0.2.104/plugin-config-race/report.md)。

## 构建与安装身份

冻结源码 `aaf39cd5b52e584e5f0acc012a4e8b8335d65130`，快照 `c261dcfc51959d02c1f65c458443cbe8fd7f29d6ca4c79b458b8fdbf44bc0e7f`。正常release真实子进程退出0、六门pass，正常MSI安装退出0；Program Files内1159个文件逐长度/SHA一致。MSI 285810184字节，SHA256 `51d9ce42b9a81d66537095e94a41ecce4cc0255378b27fc9b606563f9359384e`。正常正式后台与桌面壳启动，Browser诊断默认关闭，原工作区、权限与安全库保持。[安装核验](installed-validation/installed-105-verification.json)、[构建退出码](installed-validation/build-result.json)、[实际进程身份](installed-validation/installed-105-standard-processes.json)。

## 职责与行为变化

- `devin_acp/bridge.rs`只负责将内层已有ErrorResponse的具体错误传回ACP模型，保留HTTP状态。
- `dsh_web.rs`沿用已有运行事件：资格校验失败时写 `tool.dispatch_rejected`，stage为`after_admission_before_executor`，reason_code为`dsh_action_not_live`、executed=false，附同一工具ID/名称。固定字段不记录参数、密钥和动态错误正文；事件保存失败仍明确拒绝并提示保存失败。
- 原审批、资格、工具登记与结账不变，没有第二账本或新闸门。此阶段已有一条工具failed记录，不能写成before_dispatch或零登记；102的源码变化登记前拒绝是另一个场景。

源码offline build通过；既有DSH专项8通过、0失败、1项因独立官方运行资源条件跳过。没有新增模型夹具。[候选构建日志](../release-0.2.104/plugin-config-race/candidate-build.log)、[专项测试日志](../release-0.2.104/plugin-config-race/candidate-dsh-tests.log)。此前完整1422通过/0失败/6既有忽略属于分页基线，不冒称本次又重跑了全部测试。

## 正式真实复验

沿用原房间与唯一远端island-kayak，仅一次SWE-2请求、一次工具尝试，不创建新云端会话。ACP submitted之后，通过正常插件配置接口把空对象改为无副作用验收标签，原22个源码文件摘要不变。

- 配置提交1791506080711.9082ms，工具登记1791506098564ms；工具尚未执行即因旧配置快照失效拒绝。
- 独立运行事件1791506098828ms准确记录上述stage/reason_code/executed=false及同一工具ID。真实HTTP端点GET为0，工具1条failed、无重试补发。
- 最终回复原样包含“DSH原工具快照已失效（停用、配置更新、卸载或重装），未执行”，具体原因确已传至模型，不以模型解释替代独立事件。
- run `run-chat-c76fe3e7dbd0649c4c8f54b49c2bec05c0a2cd5cf932713f` completed；唯一ACP `70c0914c331dd6bb7a6be669b32c6bfbbfa6321fca4fb5a2` end_turn、process_drained=1、绑定解锁。聊天#701/#702、30.2秒，无需刷新显示。
- 排空后通过正常接口恢复空配置、原白名单及插件停用状态，revision49；新合法activation不倒退、源码摘要保持，服务器正常停止。

[软件实拍](plugin-config-race/final-reply.jpg)、[独立核验](plugin-config-race/verification.json)、[原事实及最终回复](plugin-config-race/result.json)、[配置时序](plugin-config-race/config-observation.json)、[真实网络事件](plugin-config-race/network-events.jsonl)、[配置恢复](plugin-config-race/config-restored.json)、[状态恢复](plugin-config-race/restored.json)。

## 执行中的配置更换与旧宿主退出（另一独立阶段）

正式105、原SWE-2/唯一island-kayak再发一次真实请求；自有慢资料端点只是实际HTTP服务，不替代模型或工具。仅在真实net_fetch网络进入后通过正常配置接口更新验收标签。测试脚本事先核对正式node.exe路径/创建时间/SHA并持有实际Win32进程句柄，没有终止该进程。

独立网络GET1，返回响应体前约1.845秒连接关闭，没有迟到响应或重放；原宿主持有句柄WaitForSingleObject由258变0，证明原进程真正退出。宿主审计failed/host_interrupted，工具一条failed；这属于已经进入执行后取消，不能写为executed=false或登记前拒绝。最终回复显示cleanup_confirmed=true，但完整瞬时工具响应未另存，因此清理结论以独立句柄事实为依据，不仅凭模型转述。

run `run-chat-4bca9de730b90a1f15069cb3b6117e808af7d211a39e6dd7` completed；ACP `86da0f8b9b7f07bcb2687de0df8159ead9d8581c93329eb3` end_turn/process_drained=1，绑定解锁；聊天#703/#704、23.2秒，正常界面自动显示。排空后恢复空配置、原白名单/停用状态，revision51，原源码摘要一致，资料服务器正常停止。

[正式软件实拍](plugin-config-inflight/final-reply.jpg)、[独立核验](plugin-config-inflight/verification.json)、[持有句柄原事实](plugin-config-inflight/held-process-result.json)、[真实网络](plugin-config-inflight/slow-events.jsonl)、[精确工具审计](plugin-config-inflight/matching-tool-audit.json)、[恢复事实](plugin-config-inflight/restored.json)。只关闭本次执行中重新配置路径，不外推全部许可撤回、后代进程树或任意Provider。

## 后续测试设计与边界

此子项已闭环。后续应分别验证运行中的许可撤销、其它生命周期阶段和后代句柄、其它Provider及回执读取资格撤回；每项明确区分登记前拒绝、已登记未执行和已执行取消，不使用一份零GET覆盖所有阶段。Browser新原生Target替换及跨来源commit严格down/up窗口仍未命中；总体会话、架构及运维矩阵继续开放，以四台账为准。Paint免测、微信不动、Opus暂停、不使用子代理。ChatGPT订阅已完成一次最小文本连通，不扩为正式Provider。整体Goal继续。

## GitHub交付与检查

[0.2.105公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.105)已上传MSI、installer-report、package-safety和MSI.sha256四资产，服务器端长度/digest与本地一致，实际标签指向aaf39cd。构建源码两路CI 37864933967、37864929432均success。未签名、不标latest、不发布自动更新清单；证据提交不改变构建来源。[服务端元数据](installed-validation/github-published-metadata.json)、[实际标签](installed-validation/github-tag.json)。
