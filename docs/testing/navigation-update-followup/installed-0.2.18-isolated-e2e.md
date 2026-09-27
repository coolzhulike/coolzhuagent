# 0.2.18 安装版隔离端到端验收

验证对象是升级后安装目录中的实际二进制，不是 `target/debug` 下的源码构建产物：

| 二进制 | 安装路径 | SHA256 |
| --- | --- | --- |
| Web | `C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe` | `581822BE7B8EAC97A9CC6AF1B64AEA5F5BA2369CE13445682FE0154D71CA1081` |
| CLI | `C:\Program Files\CoolzhuAgent\bin\coolzhu-cli.exe` | `0EB7B9135C69CE99815A92260454BC5F97FFF3ECA62B94B5FDB343F366F6555F` |

每个脚本启动前核对安装二进制 SHA256，在新的 `tmp/` 工程中设置独立运行目录，Web 与本地假模型仅绑定随机 `127.0.0.1` 端口。Goal 和视觉用例运行安装 Web 的字节相同临时副本。未连接 8765、未操作桌面或真实模型密钥/配置。四项均只运行一次并首轮通过，原始失败为零。

| 场景 | 次数 / 结果 | 安装版实际证据 |
| --- | --- | --- |
| CLI → Web → 子 Agent / 用量 | 1 / 通过 | CLI 退出码 0，父最终回答与子回答均返回；模型请求 3 次；同一运行的 `chat`、`child_agent`、`tool_feedback` 轨迹各一条，子请求用量 11/7 单条。 |
| Goal 阶段工程固定 | 1 / 通过 | 模型运行中切工程返回 HTTP 409，结束后返回 200；阶段始终在原工程执行，结束后可切到新工程。 |
| 视觉转述 → 文本请求用量 | 1 / 通过 | 真实 PNG 仅进入视觉模型，转述仅进入文本模型；`vision_description` 11/7 和 `chat` 5/3 各一条，共享实际 room/run/内部模型 turn，轨迹各一次且请求 attempt 不重复。附加的仅推理流保留推理轨迹，最终错误正文不含推理标记。 |
| MCP stdio 正式接线 | 1 / 通过 | 官方 SDK Everything Server 连接并发现 13 个工具；管理会话 A 发送目标 B 的模型首轮获得工具定义，echo 结果进入第二轮模型请求，最终正文返回。默认模型窗口 64K，请求实际 `max_tokens` 32K。 |

去标识化机读结果见 [installed-0.2.18-e2e-sanitized.json](installed-0.2.18-e2e-sanitized.json)。原始隔离日志保留在 `tmp/installed-0218-{cli-agent,goal-pin,vision-usage,mcp-stdio}-e2e.log`，对应运行目录分别为 `tmp/s25-cli-e2e-b6dec0fd12`、`tmp/s25-goal-pin-703bdb8382`、`tmp/vision-usage-run-e2e-3275bfb0e1`、`tmp/mcp-model-e2e-86adc677d6`；它们包含临时会话标识，未复制进归档。四个脚本均在收尾时停止其 Web 与本地假模型服务。

这些安装版实操与 [打包前源码验证](final-combined-source-verification.md) 分开统计。此前源码 debug Web 的窄修构建 SHA 为 `4AF5A03ACC09CA4F7FBCF6C14AF5E01D60D8C5105A7B7FFF8AC9FD8BC8F9F965`；上述四项结果只归属于本报告所列安装版 SHA。仅推理流的真实收尾走“模型已结束但无回答”错误路径；新增的 assistant fallback 文案分支由打包前定向 Rust 用例覆盖，本次安装版流式请求不单独证明该分支被执行。
