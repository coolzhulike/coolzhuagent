# 第一优先级：工具暴露与本地模型恢复验证

日期：2026-09-19。

## 结论与修改

问题集中在 Web Console 的模型请求组装及流式/非流式工具反馈环；Provider 适配层负责把结构化 `tool_calls` 转为工具块，正文标签仍作为普通文本处理。Computer Use supervisor 的预算与终态规则保留，由会话层负责恢复原任务。

除原问题报告外，发现 `select_tools_for_request` 在上下文 ≤ 16K 时仅保留 Computer Use，是文件任务误走 UI 的直接诱因。现保留常用文件、搜索、命令工具，并用当前用户目标及简短延续上下文限制 UI 暴露。请求级和执行级分别检查本次实际 schema、会话设置和房间能力。

`dev_open_permissions` 只影响授权，不重新开启工具、不改写 exposure。调试构建新配置默认为完全访问；发布构建默认关闭；已有显式配置保持优先。会话可分别覆盖工具总开关、范围、Computer Use、工具名单。

`dispatch-only` 只暴露语义入口；语义调度参数若可以映射到文件写入，走普通文件权限链，其余 legacy UI 路由必须有 Computer Use 能力。历史本地规则补执行也统一到同一带会话/房间身份的入口，不能绕过关闭设置。

UI 熔断后的再调用、未开放的结构化调用、正文伪工具协议、反馈步数上限，均最多进行一次不携带任何工具 schema 的恢复。保留完整原始请求、之前已执行的工具结果；未执行请求不加入执行队列。恢复仍失败则明确原任务未完成，不把运行时诊断直接冒充任务产物。明确请求协议说明或原样示例的文本不被当作待执行调用。

## 验证证据

- 第一轮 `cargo test -p coolzhu-web-console --offline tool_policy_ -- --test-threads=1`：4/4 通过，日志 `tmp/analysis-tool-runtime-tests.txt`。
- 同一已编译测试程序：工具定义 6/6、Computer Use retry guard 1/1、非流式多轮原契约 1/1 通过。
- 全量回归发现的五项相关旧契约/环境依赖已修复，并逐项复测通过：系统上下文完整性（追加精确策略）、独立聊天室 hotkey fixture、UI 操作约束、紧凑上下文保留文件工具、显式工具授权后的 provider 调用身份传递。没有通过放宽运行时权限修复测试。
- 本地 HTTP 假模型验证工具关闭且完全访问开启时：首轮返回未闭合 `<tool_call>`，第二轮成功返回 HTML；仅请求两次，两个请求都没有工具 schema，文件没有创建，最终回复为 HTML。
- 同一假模型持续返回伪工具调用时：仍只请求两次，最终说明原任务未完成，并保留原始用户任务。
- 新增执行入口回归：关闭 Computer Use 后的语义 UI 路由、本地规则兜底，以及 tools-off + dev-open 的写入请求均在执行前拒绝。补充审查后的 `tool_policy_` 5/5 通过，日志 `tmp/analysis-tool-runtime-final-tests.txt`。
- 单次视觉理解没有工具反馈执行器，现请求固定无工具；聊天无工具回答与工具循环共用恢复路径。每次聊天模型响应及恢复响应记录一次用量；没有聊天室归属的视觉理解不计入“当前聊天室”统计。

## 实机范围

对 `http://127.0.0.1:8080/v1/models` 进行 3 秒只读探测，连接被拒绝；没有启动、重启或修改用户的 bonsai 服务。因此没有宣称真实 27B 模型的 HTML/SVG 产出质量已通过实机回归。真实模型验证需在服务恢复后保持原报告的固定提示、上下文和输出预算，避免把规划耗尽输出预算误判成工具链问题。

所有修改保留 supervisor 预算、权限运行时和执行结果审计。全量构建、跨模块与前端集成验证由主任务汇总。

## 补充：关联模块与统计边界验证

- `cargo build -p coolzhu-tool-registry --offline` 成功；日志 `tmp/analysis-tool-registry-final-build.txt`。
- `cargo test --test module_linkage_smoke --offline` 4/4 通过；日志 `tmp/analysis-module-linkage-final-smoke.txt`。
- 统计审阅补齐独立接力 API、流式接力回复的耗时记录。首轮流式用量改为退出保护：正常完成与取消退出均保存已收到的累计快照，显式完成后不会再次累计，不估算未知输出。
- `cargo test -p coolzhu-web-console --offline chat_insights::tests -- --test-threads=1` 4/4 通过。SQLite 回归确认正常完成与取消各落一条事实记录，重复完成/退出不重复入账；累计快照合并保留 start 输入及 delta 输出，并对重复 delta 去重。日志 `tmp/analysis-chat-insights-final-tests.txt`。
- 明确缓存统计范围为“接口已返回且适配器可解析的字段”；旧 OpenAI 适配器未解析的缓存明细保持 0，不宣称已完成所有供应商的缓存计费统计。
