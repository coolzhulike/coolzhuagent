# 0.2.101 原生视图替换窗口与观察错误文案

## 结论

严格“新原生视图在旧动作 down/up 之间出现”未命中，保持未验收。真实网页 down→up 为 2402.4605ms，新网页 loaded 比旧 pointerup 晚 10236.3466ms；不能把正常关闭重开成功等同于严格时序通过。

发现独立产品缺陷：动作步骤明确 `sent/released`，旧网页也记录 trusted pointerdown、pointerup、click，但后续 verification 观察因资源变化停止时，终态摘要写成“本次没有发送输入”。真实 SWE 回复因此错误转述 not_sent。源码候选已将此观察错误改为“观察无法继续；输入投递与释放状态以步骤回执为准”，不改变授权、投递、释放、资源隔离、重试或终态。候选 offline build 与23项现有原生浏览器回归通过；后续原文投影收敛后再次offline build、完整控制台1418通过/0失败/6既有忽略（另lib8/宿主1）通过，见[最终编译](candidate-build-final.log)与[最终回归](candidate-tests-final.log)。没有新增单元用例，尚未安装，不能宣称正式修复通过。

## 正式实操

- 正式后台/桌面壳均为已安装0.2.101；没有切换诊断候选或更改输入时序。
- 正常快捷栏打开右栏浏览器，加载独立临时服务器旧网页；原 SWE-2-medium、原房间、唯一 island-kayak，configuration_revision=35/原白名单保持。
- 一次真实 `computer_use_perform(surface=browser,max_actions=1)`，点击旧网页“替换窗口验收”。旧页面真实按下处理2400ms，仅用于暴露事件窗口，无产品测试钩子。
- 验收端通过正常“关闭当前工具”，再点击本轮聊天链接打开 localhost 新网页；正常界面输入区间记录于 [ui-action-times.json](ui-action-times.json)。未改变宿主调度或人为补发模型动作。
- 原动作 `input_sent_observed/sent/released`，后续观察/验证因 frozen resource 变化停止；CU blocked/verification、父run failed。SWE end_turn/process_drained=1、原绑定解锁、internal远端NULL；ACP仅增加1次。
- 新网页没有 pointerdown/up/click/input/keydown。其 GET/loaded 是服务器及网页事实，不能充作 WebView 精确 commit 或 native Target 身份。
- 真实模型错误转述、失败状态及所有原记录保持，不手改历史结账为成功。

## 证据与限制

[结构化原事实及可见最终回复](result.json)、[网页事件同服务器时钟](events.jsonl)、[本轮真实请求](submitted-request.json)、[运行中新视图与电脑使用提示实拍](new-view-during-run.jpg)、[正常聊天最终回复实拍](101-final-visible-reply.jpg)。不同page时钟不直接比较；server_ns为同一进程的接收时刻，尚不证明跨线程精确因果。

原采证脚本第一次把UI消息索引688误当数据库字符串消息ID，查询None失败；改为本轮marker定位user创建时间，并只读取随后 `assistant-reply`。第一次错误保留在[采证错误说明](collector-errors.txt)，没有读取或归档模型reasoning。

两份脚本快照供审计，执行时位于仓库 `tmp/2026-10-08-browser-native-replacement/`；勿从docs路径直接重放。没有为获得通过重复发送或重新创建云端会话。严格新原生Target、跨来源精确commit及候选观察错误正式复验仍待闭环。

此前自动审批拒绝“停止正式壳并启动诊断候选”的组合，未给具体原因；本轮没有重试该操作，只操作既有未修改的正式软件。
