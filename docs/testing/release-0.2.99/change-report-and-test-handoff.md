# 0.2.99 改动与验收交接

本版包含工程浏览与定时重排两项修补，不宣称四项整合任务已全部验收。正式包冻结源码 `23bbb94a0e4c2799f98025e48e41660206287698`，后续证据提交不改变该构建来源。

## 可据此设计测试的行为变更

1. 工程树先为当前目录条目分配额度，再用剩余额度预展开子目录。原先一个巨大子目录可能用尽400项，让后面的根文件消失；现在根文件保留。深度、总量及工程根边界保持原规则。目录本身超过额度的分页能力不在本修补中。
2. 输入完整相对路径搜索文件时，复用工程根路径解析直接查盘，精确项置顶并去重。已有索引不会导致刚新建的已知路径文件无法打开；模糊搜索仍使用原索引，并未声称实现全量增量索引。
3. 定时任务执行及结果持久化后，使用完成时的当前时间计算下一触发点，保留原周期锚点并跳过积压周期。120秒触发、周期60秒、310秒完成时，下次为360秒，原先为已过期的180秒。跨日daily/weekly同样以完成时刻重排；一次任务仍终结。

职责：工程文件发现沿用项目路径解析和原索引；`advance_scheduled_task`只重排配置，领取、权限、结果持久化仍由调度路径负责。未改变调度目的房间、模型配置或Devin绑定。

## 构建及安装

- 正常完整release构建及六项门禁通过，源码快照构建期间未变化。MSI未签名。
- MSI：`dist/CoolzhuAgent-0.2.99.msi`，285752839字节，SHA256 `08b42a75b3c4f1854f0cf32d473a3b9dccc6f38b8ce869d64d3ccfd7ed334657`。
- 源码快照：`f7e62ebbec54b18619377554dc9d58a9e0d1068c66c8eae13470c4bf16d75214`；payload摘要：`921bc53d5933e8556c0062dafc8ac5835f4a7b6a8ca85a19e9e675d9e41a14fa`。
- 正常管理员MSI安装返回0，1159个实际Program Files文件逐长度/SHA一致，CLI版本099/源码23bbb94，正式Web和原生壳启动。核验收据在[installed-validation](installed-validation/installed-099-verification.json)，六门原始产物在[evidence](evidence/build-identity/pkg-report-release-20261008-133344410-57756a52/package-report.json)。
- 本地调度专项11项、完整控制台1411通过/0失败/6既有忽略，另库/子目标8/1通过；offline build通过。原始日志见[源码修补报告](../2026-10-08-scheduler/report.md)。实际构建提交23bbb94的远端检查37839844733、37839835923均success。

[0.2.99公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.99)已上传MSI、installer-report、package-safety及MSI.sha256四份资产；服务器端长度/SHA与本地一致，实际标签指向23bbb94，未标latest。原始元数据见[发布收据](installed-validation/github-published-metadata.json)。

## 正式软件实操

### 新文件及目录额度

在原验收工作区保留真实旧`files.json`，创建自有中文名Rust文件。正式HTTP确认根目录文件可见、精确路径首项正确，索引SHA保持不变。正常UI输入完整相对路径，点击实际搜索结果后右栏显示新文件正文：[搜索截图](installed-validation/new-file-search.jpg)、[打开截图](installed-validation/new-file-opened.jpg)。[原始结果](installed-validation/project-browser-result.json)含接口与索引身份。仅清理本轮自有文件，未删除工作区或重建索引。

### 结果已结账、计划未回写的重启恢复

正常API创建本轮Goal并立即取消，再创建一次计划。临时以只读共享Win32文件句柄占用原配置文件，令真实配置发布失败：原始run-due返回`projection_pending`，领取表`settled`，只读计划投影`recorded`。未修改ACL、配置字节或安全历史。

持有并核对正式Web进程路径/创建时间/SHA后进行受控宿主退出，释放本轮文件句柄，恢复同一个正式后台。启动扫描只恢复配置投影为`completed`，领取记录和唯一一条状态消息前后完全相同；5路并发run-due均为空。见[真实原始结果](installed-validation/scheduler-projection-result.json)、[重启后正式侧栏截图](installed-validation/scheduler-projection-restart.jpg)。只删除本轮计划，取消Goal与领取记录保留。

以上专项模型调用0，SWE-2-medium、原聊天室、唯一`island-kayak`/internal NULL保持，attempt930/usage539不变。没有采用模型夹具，也没有把无模型演练称为模型长程任务通过。

### 真实插件取消后的独立进程退出证明

在同一正式版、原SWE/唯一远端中另执行一次真实DSH `net_fetch`。真实网络函数进入后，通过正常产品接口停用本轮测试插件；测试脚本事先持有实际正式`dsh-runtime/node/node.exe`子进程的Win32句柄，并核对路径、创建时间及二进制SHA，`WaitForSingleObject`由258（仍运行）变为0（已退出）。测试脚本没有终止该宿主。原网络连接约1.199秒后在返回响应前断开，无响应体或补发；宿主审计cancelled/cleanup_confirmed=true，单工具台账failed，真实ACP正常end_turn/drained/解绑锁。

见[专项及独立句柄事实](plugin-held-process/report.md)、[正式聊天室实拍](installed-validation/plugin-held-process-completed.jpg)。本轮只增加一个真实请求，结束后新增工具白名单撤回、测试插件保持原停用状态，原计算器不变。该证据只关闭函数进入后停用路径的独立退出确认，未外推所有清理阶段。

## 仍需后续验收

- 调度真实模型派发、running中断后的未知结果处理和完整多工作区矩阵仍开放。本次重启投影恢复与098正常扫描恢复为独立证据。
- 长任务跨周期修补已正式出包，确定性代码回归通过；尚无真实模型任务跨周期调度证据。poll固定系统房间与唯一Devin绑定约束冲突，未新建云端会话绕过。
- Browser新原生Target替换与跨来源commit的严格down/up窄时序仍开放；既有长程、输入/释放、关闭和右栏切换通过结论保持，不扩大。
- 跨客户端发起消息及后台重启后，原页面未自动同步最新历史；刷新后#673/#674可见。持久化正确，自动同步仍待修补，见[专项发现](plugin-held-process/report.md)。
- 其它余项以[总体队列](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)为准。Paint免测、微信不动、Opus暂停，继续由主会话独立推进Goal。
