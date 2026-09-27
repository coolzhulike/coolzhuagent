# 0.2.13 定向测试交接

先阅读 [总报告](../../2026-09-19-release-0.2.13-qwen-change-and-test-report.md)，再按需要使用：

- [多模态测试契约](multimodal-test-contract.md)：配置字段、优先级、输入/输出、错误边界、未覆盖项。
- [协议验收记录](adapter-protocol-verification.md)：Qwen 参数映射与 Anthropic 原生图片请求。
- [真实长任务问题](../../issues/2026-09-19-qwen-real-test-findings.md)：思考与工具参数混合、历史膨胀、状态聚合。
- [电脑操作问题](../../issues/2026-09-19-qwen-computer-use-test.md)：两轮失败、根因、缺失能力和验收要求。
- [动画成品](evidence/pelican-r5.html) / [浏览器检查](pelican-browser-review.md)。

`evidence/` 保存每次真实测试的 request、events、answer、summary、run、insights，以及安装检查和截图。以 `installed-` 开头的是安装后实际调用；`svg-r1` 至 `svg-r5` 是连续五轮文件任务。`tool_calls` 旧摘要字段只统计一种事件，可能为 0，不能当作未调用工具；实际工具以 tool-result、provider ID 和原生终态交叉核对。`svg-metrics.json` 给出每轮增量，避免把累计用量反复相加。

`scripts/` 中的浏览器验收只打开本地 HTML 或本机参数页，不发模型请求，不修改模型产物。安装参数页脚本阻止非 GET/HEAD 请求。

```powershell
node docs/testing/release-0.2.13/scripts/verify-pelican-artifact.cjs docs/testing/release-0.2.13/evidence/pelican-r5.html tmp/qwen-artifact-qa
node docs/testing/release-0.2.13/scripts/verify-pelican-extended.cjs docs/testing/release-0.2.13/evidence/pelican-r5.html tmp/qwen-artifact-extended-qa
```

`cloud-test-reference/` 是原测试驱动的参考副本，供设计 API 用例，**没有凭据和隔离运行配置**；它不是开箱即用的云端自动回归。准备独立环境后再改路径、端口和会话 ID；不要指向用户现有聊天室，也不要把关闭前端连接当作已经中止底层桌面动作。

`evidence-manifest.json` 记录证据文件 SHA256。总报告和两份 issue 另随交接压缩包提供。所有路径中保留的原运行目录用于追溯，临时目录不存在时可优先使用本目录对应同名证据。
