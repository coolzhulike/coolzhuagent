# 工具结果可读性与CU材料误判专项（源码候选）

正式101保持运行。此轮发现并修补两处代码问题，尚未打包安装、未做修补后的真实大结果或正常审查终态复验，不能标记这两项正式验收通过。[方案和取舍](../../analysis/2026-09-21-integration-review/tool-result-readability-design-2026-10-08.md)。

## 当前源码变更

1. 超8000字符工具结果继续保存完整内容寻址原文；ACP本轮未开放可用read_file时完整交回，不再只节选6000字符并提示未声明工具。DSH内层和ACP外层均处理，没有扩大白名单/权限，没有新增模型可用API。
2. 未执行CU提醒只读取本轮正文的非代码材料，剔除已有附件分界后的内容和Markdown围栏；动作与CU入口须在同一句正文，保留全局否定与说明前缀。仅影响提醒和失败结账，不修改执行闸门或旧失败记录。

## 正式101真实审查发现

任务标记`SPILL-CAPABILITY-REVIEW-101-20261008`，同一SWE-2-medium、同一聊天室和唯一island-kayak；revision35原白名单不变。原[请求材料](review-prompt.txt)与[发送脚本](send-review.py)公开便于后续设计用例，勿直接重放从而重复收费/创建重复记录。

- 仅一次`plugin__cli_anything_status({"command":"rust-analyzer"})`；宿主台账completed、审计ok/allow-auto，elapsed621ms。不是工具失败。
- ACP一次请求正常end_turn、process_drained=1，唯一远端解锁，internal仍无远端绑定。
- 正常界面#685/#686无需刷新出现，回复约43.5秒；但收尾因源代码中的click/入口名错误添加未执行CU提醒，父run实际failed。此次为发现缺陷的失败证据，不是源码修补已通过实机证明。
- 原[独立台账/审计/最终消息](review-result.json)、[可见回复](visible-reply.json)和[正式软件实拍](formal-review-reply.jpg)保留；未读取或另存模型思考。真实请求数936→937、聊天室存储行数1166→1169（含过程记录的总行数，不是可见消息条数），不新建云会话。
- 首次采证错误假定completed和只读查询字段错误保留在[说明](collector-failures.txt)及原[脚本](collect.py)。纠正查询后采集真实failed，不将脚本失败掩盖或改写成成功。

模型审查认为ACP最小方案可行，同时指出其它Provider续读、容量和引用回收的缺口。主会话保留这些边界；“阻断缺陷无”只是模型对提供片段的意见，不是总体完成证明。

## 代码验证

最终`cargo build -p coolzhu-web-console --offline`成功；完整控制台1418通过/0失败/6既有忽略，lib8及宿主子目标1通过。[编译](build-final.log)、[检查](tests-final.log)。只新增两个必要确定性回归，覆盖Unicode完整原文/保存失败回退，以及代码/附件动作不变成CU任务、真实明确操作仍有未执行提醒。没有新增模型夹具，也不把已有本地测试等同真实模型验收。

上一提交ec45bc7的PR/push两路远端CI均success（37853210888、37853203232）；不能外推当前新修补的远端检查。

## 后续验收边界

- 新正式版本实际大结果：冻结不含read_file的ACP工具列表，真实工具输出>8000字符；核磁盘原文SHA、尾文完整进入工具回执、不出现未声明续读请求、正常最终回答及协议排空。支持读取的会话另验Unicode分页、当前权限/撤销拒绝，不自动扩权。
- 源码审查任务：包含动作函数和CU入口的代码/文本附件、仅合法状态工具；核没有虚假未执行CU提醒且父run正常完成。另对真正要求CU但没派发的任务保留失败提示。只使用原唯一SWE远端。
- 完整结果仍可能超过模型窗口；其它HTTP Provider实际声明、完整引用图/GC、Goal/Relay附件仍开放，不宣称无限容量或自动回收。
- Browser严格新Target与跨来源commit窗口仍开放。此前切换诊断候选壳被自动审批blocked by policy拒绝、未给原因、未执行，本轮未绕过。Paint免测、微信不动、Opus暂停、主会话独立执行，Goal继续。
