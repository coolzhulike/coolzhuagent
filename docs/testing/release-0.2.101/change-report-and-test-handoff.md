# 0.2.101 改动与验收交接

本版修复 Devin 请求在统计及运行轨迹中遗漏的问题，冻结源码 `54594d96e8f7801ddf606b9b8d7729306594f501`。0.2.100 的跨客户端历史自动同步修补仍保留。本版不代表总体计划或 Browser 严格时序矩阵全部通过。

## 行为与职责

`request_usage.rs` 查询时合并 HTTP 用量与 ACP 权威请求台账；不增加发送钩子或第二份账本。HTTP 中已有精确 `acp:attempt_id` 投影时只计一次，并核对工作区、房间、Agent、run、turn 作用域。异作用域投影不贡献用量。轨迹由同一读取函数查询，ACP 按精确 run 匹配；旧 HTTP 仍保留原 turn/call 回退。

prepared、明确未发送、结果未知、派发未知分别表达。网络尝试只计明确派发；未知不伪造失败或成功。供应商未返回 token 时显示未知，缺时间返回空值，不用 rowid 或当前时间补造。`chat_insights.rs` 不重复实现归并 SQL；两处前端负责中文状态及“用量未完整提供”的展示，不展示内部调试控件。

可据这些行为设计针对性测试：无 ACP 表的旧库、精确重复投影、同 ID 异作用域、prepared/submitted/not_sent/unknown/terminal、拒绝与取消、HTTP 重试、仅部分 token 已知、缺时间、损坏 JSON、同房间多 Agent 和精确 run 轨迹。不要把接口返回未知当作零消耗，也不要把旧投影与 ACP 请求数直接相加。

## 已执行验证

- 三组专项数据库测试通过：旧库/旧用量；六类状态、精确去重、异作用域、缺时间；HTTP 重试及回退、损坏 JSON。完整控制台 1416 通过、0 失败、6 项既有忽略，另 lib 8 项和子目标 1 项通过；offline build、两份 JS 语法检查通过。首次 E0597 编译失败及修正后日志均保留在[候选证据](../2026-10-08-acp-usage/report.md)。
- 正常完整 release 构建，六门 pass、源码快照未变化。源快照 `200b14ea918e28f94fb7f249763e02d16c26ba0e380c5f12d30786da1b228523`，payload `7c5729f4170440b261c6878e027d0288abde0c00272430066e06889f3dcf1e48`。MSI 285773320 字节，SHA256 `e472e0ac9209a7b46234475ed098ebbbdbfddaf93c48ebbdd7ffd93d87ca6f13`，未签名。
- 正常管理员安装返回 0；1159 个 Program Files 文件逐长度/SHA 一致，CLI 显示 0.2.101/54594d9，正式后台及原生壳启动。见[安装核验](installed-validation/installed-101-verification.json)。
- 原生软件本页正常输入实际 Browser 设计与源码审查任务 #683，经唯一 SWE-2-medium / island-kayak 完成单次真实 DSH calculator `882+1`，回复 #684。整轮 completed/end_turn/drained，原远端解锁、internal 未绑定新会话。模型回复无需刷新显示完整，耗时约 73.5 秒：[流式实拍](installed-validation/local-stream-active.jpg)、[完成实拍](installed-validation/local-stream-completed.jpg)。
- 同一统计侧栏未点击刷新，SWE 请求 882→883、明确派发 875→876；9 取消、2 未发送、5 结果未知/派发未知均不变，pending=0。旧 HTTP 投影仍 539 条，不重复相加；另一个 Agent 的 1 条旧请求独立保留。只读 journal 与正常 HTTP 返回逐项一致：[前](installed-validation/statistics-before.jpg)、[后](installed-validation/statistics-after.jpg)、[查询结果](installed-validation/http-after-result.json)。
- 本轮轨迹只有一个精确 ACP 请求，completed/dispatched=true，token 和请求时间仍为空。正常 UI 展开当前 73.5 秒轮次显示“模型请求·已完成”和未知 token：[实拍](installed-validation/trace-current-request.jpg)。查询耗时约统计 57ms、轨迹 34ms，只描述本机本次测量。

原工具审计是 status=ok、permission=allow-auto，并没有 executed 字段；模型所述 executed=true 不作为原始事实。见[完整台账与审计](installed-validation/integration-result.json)。本次单工具用于实际代码方案审查，不以重复问候替代长程验收。

## 审查取舍与剩余缺口

SWE 建议复用既有 opt-in Browser 诊断、保持默认关闭及无前端调试控件；主会话接受这一方向，实施尚在后续源码。click 热路径应先采内存时间戳，结算后再写日志，不能忽略磁盘写入对竞态的扰动。ACK 不等于 DOM 处理时刻，SourceChanged/ContentLoading 也不能据现有官方语义直接证明精确 commit 上下界。模型提出“风险无”及这些上下界的结论未被采纳。

Browser 新原生 Target 替换及跨来源 commit 恰在 down/up 之间仍开放；源码向原 controller 尝试释放不等于严格竞态已命中。本页流式及统计增量已在正式版通过，但断线大批变动、完整跨工程并发、其它 32 工作包矩阵仍按[总体台账](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)继续。Paint 免测，微信不动，Opus 暂停，不使用子代理。

构建提交两路远端检查均 success：[检查一](https://github.com/coolzhulike/coolzhuagent/actions/runs/37848767871)、[检查二](https://github.com/coolzhulike/coolzhuagent/actions/runs/37848762563)。已公开[101预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.101)，四资产服务器长度/SHA及实际标签均与构建提交一致，见[服务器元数据](installed-validation/github-published-metadata.json)。不标 latest、不启用未签名的自动更新清单。
