# 三层跨域 Browser 长程与权限顶栏同步：源码候选验收

本轮新增复杂任务通过，尚不代表正式安装版或严格时序全矩阵通过。正式发布仍为0.2.118。主会话独立执行，真实SWE-2-medium持续复用唯一Devin远端`island-kayak`，没有创建新云端会话；Paint免测、微信不动、Opus暂停。

## 真实长程任务

普通HTTP页面依次嵌套127.0.0.1→localhost→127.0.0.1→localhost三层文档，包含两次水平反射、旋转/斜切、叶文档独立滚动、跨来源翻页和旁路同名控件。随机校验码及回执只由网页显示，未写进模型任务。它不是模型或工具响应夹具，模型没有通过HTTP/脚本代做输入。

真实SWE一次`computer_use_perform`完成10步：叶输入框点击→校验码输入→Enter→三次子滚动→获取回执点击及跨来源翻页→父输入框点击→回执输入→完成整轮。原始宿主终态为`succeeded/goal_achieved=true`，10步均`sent/effect_observed`，需要释放的动作已`released`，无需释放的动作为`not_needed`；root预算已wired。页面24条真实事件交叉证明校验及最终提交accepted，未触碰旁路控件。不能只按模型自述判通过。

父轮`run-chat-17a44e82c831bca95b9ae42792a21bfbe3be389118fb8e1f` completed，CU调用只有1条completed；ACP单attempt terminal/end_turn/process_drained=1，原绑定解锁且internal远端为空。[原始宿主及事件](evidence/scoped-run/verification.json)、[父轮台账](evidence/scoped-run/facts.json)、[完整提交](evidence/scoped-run/submitted.json)、[最终软件实拍](evidence/scoped-run/native-completed.jpg)。SSE只计事件名，未保存隐藏思考。

该任务运行34f2075候选桌面壳SHA `983ecb2ac8858bb9eaf36c889267e0ffb04b2d59a98872aaadbe869534c16841`，后台为118安装EXE SHA `dc3881aa0f0bf1c0f42d362c7324facc46d5915909bd16b70e8fdf914f1b5308`；两者在复制的同bin目录正常启动。不是已安装119，复制目录旧安装清单不用于宣称候选包完整性通过。

## 两次失败及原因纠正

前两次真实提交均观察阶段`extension_unavailable`、actions/input_steps=0；原始父轮分别failed，单attempt正常收尾并解锁。第一轮使用调试壳与安装后台，第二轮已改为同bin目录却仍失败，不能把第一轮环境猜测写成已证实根因。[第一轮事实](evidence/facts.json)、[第二轮事实](evidence/bundle-run/cu-facts.json)保留，不追改为成功。

实际提交脚本漏了前端`native_browser_panel`，正文“右栏已正常打开”也未命中内置浏览器目标描述。`ComputerUseTurnScope::from_submission`因此未选择原生Panel，factory走旧外部浏览器扩展preflight。纠正为页面本身的提交字段`native_browser_panel=true`及明确“当前右栏原生内置浏览器”后通过。没有放宽宿主身份校验、延长2秒/外层5秒或加私有通道。前两次preflight错误报告的预算not_wired不是本次证明的父预算丢失；正确通道原始终态已wired。

## 权限顶栏同步修补

118中从正常权限API保存后，既有页面顶栏仍可能显示旧状态，必须刷新。现在房间历史SSE复用既有一秒轮询及SQLite连接读取已提交的`permission_profile/updated_at`，变更或首次建立基线发送`permission-changed`失效通知；通知不携带授权。前端沿用房间/工程/代数校验及现有权威权限GET，不等历史流空闲，不重派模型，也不新增锁、队列或另一套权限存储。

候选后台SHA `be729850d215b471d7602a40450eca6d6a88a18377a2e2128e2e8f76e2a10f63`与上述壳正常同bin启动。空闲时通过原权限PATCH从full-access保存workspace-write，未刷新/点击页面，顶栏自动显示“目录权限”；正常PATCH恢复原full-access，顶栏自动恢复“完全访问”。[目录权限原图](evidence/permission-sync/directory-auto-updated.jpg)、[恢复原图](evidence/permission-sync/full-access-auto-restored.jpg)、[API事实](evidence/permission-sync/restore-permission.json)、[实际进程身份](evidence/candidate-bundle-processes.json)。原会话模型、参数revision57及唯一远端绑定未改。此关闭外部API权限变化顶栏同步的候选子项；实际聊天在途变更及全作用域UI矩阵不外推。

offline build actual0；历史专项4/0、前端历史专项6/0；完整Web1425/0/6既有忽略、lib8/native-host1，工具注册check0、根模块联动8/0、全部前端22/0。[检查命令和退出码](evidence/permission-sync/checks.json)、[完整回归](evidence/permission-sync/regression.json)。两项新增边界测试仅覆盖未提交/异房间权限隔离和忙时/ABA迟到事件，不代替实机验收。

## 后续验收与复现

正常打开三层页面，将界面当前Browser目标提交给真实模型，按本轮目标一次perform、完整分页规划并正常wait到终态；同时核对可信输入、原始步骤、完成页面和ACP收尾。正式新包必须独立复验本轮两项，不能用候选日志追认118包含修补。

严格新nativeTarget替换、跨来源commit恰在down/up窗口、在途观察撤销及失败HRESULT与资源变化竞争仍开放。普通导航和此10步长程不能替代这些证据。其余32WBS架构、迁移、附件/记忆和Windows矩阵按总体清单继续，Goal保持active；OpenCode/HuggingFace凭据待用户填充，不复用其它平台密钥。

35项原始证据见[摘要清单](manifest.json)。临时驱动及运行日志保留在tmp，不引入正式产品UI。已按路径/SHA/UTC启动ticks核对候选两个进程后正常停止，靶场正常退出actual0，恢复原Program Files正式118入口；[恢复进程收据](evidence/restored-standard118-processes.json)。34f2075两路远端CI均success，[原始结果](evidence/source-34f2075-ci.json)不外推后续权限修补提交。
