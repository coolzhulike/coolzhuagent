# Devin 登录与免费 SWE-2 真实验收

2026-10-04，用户明确授权完成登录、使用免费 SWE-2 做基础会话测试并更新 PR。正式安装版与指定工作区数据未替换；验证在临时工作区进行。

## 登录与目录

真实 CLI：`devin 3000.10.48 (fcf7ba39)`，SHA256 `D8877EBF699499B1D0957A9FDD99CB596013FC3BFEB782B756496BBCF527BB1B`。Devin 桌面入口与其捆绑的原生 CLI 分开识别。

启动登录后停住的根因是 CLI 先显示登录方式选择菜单，需要真实控制台输入；空 stdin 无法确认默认选项。本次改为受 Job 监督的 ConPTY，识别完整且已选中的“Log in with browser”菜单后仅发送一次确认。菜单用光标定位绘制，没有保证换行，因此识别不能依赖行首。原始终端输出、授权回调参数和凭据不传给页面、不写日志。

官方回调页显示 Connected to Devin，随后真实 `auth status`、新编译后端与插件页均确认已登录。已登录状态检查会查询远端账号，五秒上限不足；改为后端二十秒、前端二十五秒有限等待。证明见 [插件页截图](devin-plugin-authenticated-20261004.png)。

真实 `models list --format json` 返回 `families[].variants[]`，共 721 个变体。旧 parser 只接受数组/`models`，本次新增真实分组目录支持，使用 `model_uid` 与 `label`，保留目录提供的 `cost_tier/cost_summary`。SWE-2 High、Medium、Max 均标记 Free。本次仅选 `swe-2-medium`；未启用付费回退或使用其它模型。

## 三轮真实文本测试

使用实际 `ManagedProcess` 测试构造器、`SessionTransport`、`SessionService`、SQLite 台账与真实 CLI ACP 对端；不是协议 fixture。测试构造器仅在 `cfg(test)` 可用，常规回归忽略此测试。必须显式设置 `COOLZHU_DEVIN_REAL_SMOKE=1`，核对固定 CLI 和账号目录的 Free 标记后才发送提示。授权诊断同样需要 `COOLZHU_DEVIN_REAL_AUTH=1`。

临时工程只含合成文本，权限拒绝工具，关闭配置导入、子 Agent 和自动更新，不挂载宿主 MCP；存在全局 MCP 文件时测试直接拒绝，不覆盖用户设置。每轮创建独立受管进程，后续轮次使用 `session/load` 恢复已绑定会话。加载历史单独标记 replay，不参与本轮回复判定。

| 轮次 | 验收 | 实际结果 | 本轮文本块 | 历史事件 |
|---|---|---|---:|---:|
| 1 | 中文问候并记住随机测试标记 | 你好，已记住。 | 5 | 0 |
| 2 | 跨进程恢复后找回前文标记 | 与第一轮标记一致 | 4 | 10 |
| 3 | 基础计算 17 + 25 | 42 | 1 | 13 |

三轮 requested/effective 均为 `swe-2-medium`，stopReason 均为 `end_turn`，Job 进程树均确认排空。resolved_model 保持未知，不能把目录或选项值当底模解析证明。Free 是已认证账号目录的计费档位，本次没有另行查询账单，也没有编造实际扣费数字。脱敏的合成文本回执见 [JSON 报告](real-swe2-basic-conversation-20261004.json)。

首次真实 ACP 配置失败发现 CLI 会在 `session/new` 响应前发送 `session/update`。现在先有界缓存通知，收到 new 回执后逐条核对 sessionId；不匹配时不保存绑定、不发布事件。增加正确归属与错误房间的回归测试后，重试三轮全部通过。

## 验收范围

已验证真实认证、账号模型目录、ACP 配置回执、流式文本、协议终态、跨进程上下文恢复与受管退出。正式主控制台聊天、Goal、子 Agent 任务入口继续关闭，完整上下文/记忆/技能、审批继续执行和 Windows 原生工具旁路隔离尚待接入验收。这份报告不宣称 Devin 已与其它 provider 的全部 Agent 能力一致。

完整编译及回归结果在最终 PR 验证记录中补充。此次没有重新打包安装或合并 PR。
