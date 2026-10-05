# 总体推进审查任务（待真实 Opus High 会话发送）

你是 coolzhuagent 项目的技术审查者。请用中文给出可以直接实施的推进方案。此轮为文本审查，没有文件或执行工具；只能根据提供的事实和后续源码摘录判断，明确列出证据缺口，不声称已阅读全仓库或完成软件测试。不要执行修改。本任务与“过度防护审查”使用独立聊天室。

## 用户目标与优先级

1. 优先闭环内置浏览器的 Browser Use，随后闭环 Paint 的 Computer Use。真实模型采用登录 Devin 账号提供的模型，此审查指定 Opus 5.5 High；后续执行模型需用户选择，不偷换 Qwen 或夹具。
2. 插件页支持与其它服务商一致的模型会话编辑，目录识别全部账号可用模型、思考档位手选、其它参数用默认。
3. 代码职责清晰，减少无效保护与重复审批，但保留未知输入释放时的安全恢复；不能伪造安全事实来“通过”。
4. 四项整合任务最终以正式安装软件截图与真实运行事实验收。微信仅保留原功能，不测试。UI卷轴外观、金玉Logo、竹林背景、图标按钮、方案B Q版舞剑开机动画及足够停留时间仍需最终联测。

## 当前源码与证据

源码同步到 origin/main 49ba289，已含 PR72 Devin ACP 接线及 PR77 两项 Browser/CU 修复；本轮扩展账号目录模型与统一编辑器。从独立工作区实施，没有覆盖用户原工作区未提交内容。

历史真实 Qwen 0.2.67 验证通过：右栏点击、输入、滚动、导航；正式运行取消接口命中 pointerdown 与 pointerup 之间，释放收尾完成。前端停止按钮另只证明规划阶段取消。关闭与页面替换两次时序均晚于原输入释放，不能算竞争边界通过。

Browser 只读失败：当前用户明确“不点击、不输入、不滚动”，范围解析漏识别重复否定，进入交互规划后 verification_failed，实际零输入。PR77 修复逐项禁止识别，待新版真实复测。原生适配位于 web-console 的 native_browser_adapter.rs，动作执行进入共享 CU 核心；页面身份、原资源、输入回执、单调截止时间在原轮绑定。

Paint 实际失败有三个不同类型：第一次 window-canvas 包含工具栏，落笔区域不对；第二次实际画布三笔均完整投递与释放，但图形位置和结构错误，第三笔后只剩 22.639 秒验图预算而超时；第三次白画布执行前 stale_observation 拒绝旧帧，零动作。Paint 执行接口已存在，不能统一归因缺接口或全是模型能力。

另发现窗口聚焦无条件 ShowWindow(SW_RESTORE) 改变最大化布局；PR77 改为仅 IsIconic 时恢复。需验证最大化稳定、画布坐标正确、完整简化海绵宝宝、Windows 四边泛光及英文顶层“Coolzhu Agent is using your computer”，结束后全部撤除。

Devin：历史实际 models 目录为 families[].variants[]，曾返回721个变体，历史 SWE-2 文本测试完成；本轮当前 CLI 返回未登录，历史结果不能外推为当前目录或 Opus 测试通过。本轮已移除 SWE 免费白名单，逐轮校验账号精确目录与 ACP 模型配置；思考档位切到真实模型变体，其它参数不覆盖。

正式 Devin chat.rs 是纯文本入口，复用宿主 ContextAssembly/记忆选择，但禁止远端原生工具、文件、terminal 和 MCP。bridge.rs 有冻结权限与工具台账原型，INITIAL_TOOLS 仅 read/write/edit/glob/grep/bash 六种，未接入正式文本入口，也不包含 computer_use_perform。故目前不能通过 Devin 聊天执行 Browser/Paint。需提供接通现有工具链的最小架构方案，而非重新实现所有保护或把远端工具自述当成本机执行。

更完整历史依据：docs/testing/release-0.2.67/change-report-and-browser-cu-acceptance.md、release-0.2.68/browser-cu-regression-plan.md、docs/testing/devin-acp/real-validation-20261004.md。历史报告中 Qwen 执行计划已被用户后续 Devin 要求覆盖，旧报告保持事实不改写。

## 请求的交付

按“阻塞事实→最小修复→职责归属→依赖→真实验收标准”给出步骤与优先级。尤其评估：ACP/MCP 怎样进入现有父轮预算、权限、取消、插件、SKILL 与 CU 调度；是否需要一个 AgentSession 执行服务隔离 HTTP/ACP；Devin 文本上下文与持久远端历史是否重复增长；模型默认上下文未知时本地组装容量如何处理。

输出每步风险、失败回退、原软件截图要求、机器回执要求、不能用单元测试代替的验收项。区分能立即实施的修复与必须真实协议实验才能决策的部分，不为方便提出全部关闭保护或无边界延长预算。
