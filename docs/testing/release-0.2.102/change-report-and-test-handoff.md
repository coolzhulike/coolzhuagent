# 0.2.102 正式安装与真实模型验收交接

正常发布链冻结源码 `95e01409fe4736421a0d4a576a64ab8722928061`。六项发布门全部通过，MSI安装返回0，Program Files中1159个文件的长度与SHA逐项一致；正常启动、重启均使用配套正式Web与桌面壳，浏览器时序诊断默认关闭。构建源码两路CI：37859956710、37859951897均success。

## 变更与身份

- 定时poll可固定结果聊天室；删除目标后拒绝，不改投系统聊天室。旧未绑定任务保留原领取序列化。非流式HTTP200仍须真实运行completed才按执行成功推进。
- ACP登记前拒绝新增固定原因码事件，不保存工具参数或动态错误；不改变授权与执行台账。
- 大结果无实际read_file声明时完整返回；材料中的工具函数标识符不再被误判为用户要求桌面操作。浏览器观察停止的终态改为以步骤实际投递/释放回执为准。
- MSI：285789704字节，SHA256 `3190d9e5c8a5b5042eaa15ebf591a7432c15270c93d3032a5fc22b2f992eb0f0`；源码快照 `de9fc79178aebbf977355fe90a1632982433d7f77ff7a700977c28cedb39a541`。
- [产物独立核验](installed-validation/package-102-verification.json)、[安装逐文件核验](installed-validation/installed-102-verification.json)、[安装退出码](installed-validation/install-102-result.json)。完整源码回归1420通过/0失败/6既有忽略，另lib8及原生宿主1通过，见[源码专项](../2026-10-08-scheduler-room-binding/report.md)。

## 真实SWE-2定时任务

只用既有 `SWE-2-medium`、原聊天室及唯一远端 `island-kayak`。一次任务明确绑定原SWE房间，提交两份已安装源码材料，要求一次真实CLI状态和一次真实calculator，不要求CU。创建通过正常产品API；任务页面、后台执行期间切到主聊天室、原房间可见回复均经正常软件UI实拍。API创建不冒称鼠标完成整套任务创建表单。

- 任务 `schedule-1791502908555-6b214e5c8c271753`，后台实际触发；执行期间当前页面保持主聊天室，结果仍落原SWE房间。
- 单运行 `run-chat-0c599740cfb1780a42b803dc371c63308227b749cfc4ffeb` completed；单ACP `87a753709c1595ab61639765c049570a4e6a2c6fe2f60cd2` end_turn、process_drained=1。
- 两工具均completed：rust-analyzer状态可用；calculator `2047*2027=4149269`。没有虚构桌面操作提醒，父运行正常成功。
- 可见消息#691/#692，耗时44.1秒；一次领取settled/executed=true。正常重启后五次并发run-due扫描均ran=[]，模型请求增量0、消息数不变。
- 原权限、安全库、唯一远端及internal空绑定保持，不创建额外云端会话。

证据：[执行中主聊天室](installed-validation/scheduler-background-main-room.jpg)、[原房间最终回复](installed-validation/scheduler-bound-final.jpg)、[重启后实拍](installed-validation/scheduler-after-restart.jpg)、[宿主与最终回复事实](installed-validation/scheduler-integration-result.json)、[并发零重放](installed-validation/scheduler-no-replay-result.json)。未导出模型思考。

## 插件源码变化拒绝独立闭环

正式102复验命中submitted后改变自有验收插件无语义注释。宿主独立记录一次 `tool.dispatch_rejected`，`reason_code=tool_not_live`、`stage=before_dispatch`、`executed=false`；执行登记0、独立服务器GET0。父completed、ACP end_turn/drained，排空后原字节恢复，临时工具撤回、插件停用、唯一绑定解锁。可见#693/#694、26.8秒。详见[事实、实拍与边界](plugin-source-race/report.md)。这不追认101旧轮已有独立事件。

## 保留失败与未完成项

首个17bc355的102产物构建成功，但未安装/发布，被95e0140版本替代；旧producer证据仍保留。最终包装首次因PowerShell将Rust普通warning视作NativeCommandError而退出1；之后包装改为stdout/stderr分文件，发布链完成，但未正确捕获子进程ExitCode（null）。不将null改写为0；六门、逐文件摘要、CLI身份及安装返回0另行独立证实产物通过。[失败日志](installed-validation/build-final-wrapper-failed.log)、[最终包装原结果](installed-validation/build-final-result.json)、[发布链输出](installed-validation/build-final-stdout.log)均保留。

真实定时与纯材料误CU子项已闭环；HTTP200失败实机、删除房间窄竞争、running未知结果/重试、多工作区完整调度矩阵仍待。插件许可/配置竞争、独立后代句柄清理、大工具结果续读端到端、GC引用图仍待。Browser新原生Target及跨来源commit在down/up之间的严格时序未命中，仍开放；本轮没有重试自动审批已拒绝的诊断启动组合。Paint免测、微信不动、Opus暂停，无子代理。

102实拍发现长任务卡片平铺源码遮挡状态和操作，另有前端修补：先显示归属与状态，正文三行预览、按需展开全文，删除改图标。JS语法与offline build（21.70秒）通过，**此修补在95e0140之后，未包含于102，待后续正式包实拍**。本报告不将整体Goal标为完成，102尚未公开GitHub发布。
