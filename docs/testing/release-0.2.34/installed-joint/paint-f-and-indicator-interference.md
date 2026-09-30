# 正式0.2.34真实Qwen Paint及泛光干扰定位

日期：2026-09-30。当前主会话独立测试实现，不使用子代理。微信不改不测，Devin暂缓。

## 安装、入口与安全事实

用户确认安装后，正式目录`C:/Program Files/CoolzhuAgent`实际CLI输出0.2.34、参考提交be4969ac7e775c05ef9324416d118836660dfbad；10/10关键产物与034绑定报告一致。正常启动器启动后端PID10916与桌面壳PID4820，均来自正式目录；工作区仍为`C:/Users/zhupu/coolzhuagent`，SQLite schema27。没有改用户模型/Base URL/密钥/思考参数。

输入资源`windows-session-1`为safe、accepts_new_input=true、pending_recovery_operations=0、unacknowledged_open_blocks=0、unacknowledged_legacy_runs=0；历史human_review_required=2保留。测试后仍safe、无新增待办。此为后端状态核对，不能代替审批徽记的安装界面截图验收。

## F轮真实执行

任务标记`INSTALLED-034-PAINT-20260930-F`；原qwen3.8-flash自行生成结构化CU调用。任务限定选择黑色画笔、在已有Paint白画布中拖约100像素短线，不保存、不使用其它工具、不改权限；失败不由外层模型重发。

- 聊天轮：`chat-turn-1790758732202-0`；父运行：`run-chat-70b1ac9a298bd023def511972278343a879e90ac45be3f0d`。
- CU：`cu-session-1779459149988-000000000000000218da0d944b4ab32c-tool-69fd8b423ad1cc6b6665567b4cd5590424d99384a595126d04f5b88a4aecabbb`。
- 原生截图2560×1152，窗口hwnd-15c08ae，UIA172→174元素。第0步真实点击`uia-99c039b87cb951c3`，input_delivery=sent、input_release_status=released；截图确认选中画笔。
- 后续两次拖线规划由模型生成，归一化点`[[0.48,0.591],[0.519,0.591]]`，内部控制器一次重规划。第1/2步都在输入前返回stale_observation，input_delivery=not_sent、input_release_status=not_needed。
- 汇总attempts=3、input_sent=1、partial_input=0、verified_steps=0、cleanup_incidents=0；终态failed、goal_achieved=false。白画布仍无黑线，Paint未验收通过。

原始截图逐字节归档：[动作前](01-paint-before.png)、[实际选中画笔后](02-paint-brush-selected.png)、[拖线前被拒绝](03-paint-pre-drag-rejected.png)。像素统计分别使用选笔后的规划依据帧与执行前现场图，不将渲染图或模型描述当证据。

5次实际CU模型请求都正常completed：初始验收8.210秒、首规划7.933秒、点击后验收7.009秒、第一次拖线规划17.509秒、重规划23.677秒。最后一次超过20秒却正常进入输入前校验，证明034已不被旧20秒上限截断；不以此宣称绘制成功。请求台账与规划/步骤事实见 [本轮精简事实](paint-f-facts.json)。

## 拖线失败的agent侧根因

输入前守卫仍严格校验窗口/进程/DPI/边界、规划帧绑定，以及完整客户端操作容器的像素。模型目标是window-canvas fallback，该容器包含可见客户端，不只是白色绘画区。

两组“模型规划依据帧→实际输入前重新采集帧”独立像素比对：第一组客户端76,696像素变化（左38,353、右38,343）；第二组38,360像素变化（左19,728、右18,632）。客户端内部与白画布区域变化均为0；变化全部位于左右屏幕边光。未编辑截图、未掩掉像素，统计只用于定位，见 [像素统计](indicator-interference.json)。

源码`computer-use-indicator.css`在未要求减弱动态时启用了2.8秒循环breathe动画；提示层进入真实桌面截图，导致边缘像素变化，完整容器校验遂正确报告变化。原设计要求静态提示，此动画造成agent自身展示与输入前守卫冲突，不能归因于Qwen不会拖线。

最小修复：撤销呼吸动画，保留静态玉石边光与顶部指定文字，显隐仍由真实执行租约决定。不降低帧守卫、不忽略边缘、不排除提示层的截图、不新增输入权限/重试、不改变模型参数。新修复尚未包含于当前正式034；下一候选需真实模型联合复测。

## 完整桌面截图问题

结束后原`/api/capture`只取得1707×960图，当前物理主屏2560×1440，属PowerShell未初始化DPI上下文导致左上角裁剪。源码最小修复在首次读取Forms屏幕边界前建立per-monitor-v2线程DPI上下文，结束恢复；实际从源码提取的截图脚本退出0，取得2560×1440真截图。没有拼接、缩放或补造屏幕边光。

F轮窗口截图可见局部玉石边光；完整顶部文字/四边、静态提示与实际落笔联合验证仍开放。结束后活动只读接口active=false、lease_ms=0；实际输入释放另据第0步released，不能将光效撤销替代输入释放。

## 工程与后续验收

034源后续仅测试/文档提交f3e3f8d两项远端CI36692651035、36692646247均success。新增静态提示候选：Tauri离线build退出0、59项通过（3.13秒）；Web首次新增脚本括号未按Rust format转义而编译失败，修正后build退出0（1分17秒）；真实源码截图脚本退出0且实际尺寸符合物理主屏。完整Web1271通过/0失败/2忽略（51.09秒），工程检查不代替正式软件验收。

下一真实验收需正常启动器与原Qwen：看到选笔/黑色、拖出短线的前后截图、sent/path_completed/released回执、完整四边及准确顶部文字、收尾撤除。此后扩展Paint手动画图能力，Browser Use继续按先真实原生只读、再类型化输入的已审查方案推进；DSH远程插件实际安装运行及四项整体任务仍未通过。
