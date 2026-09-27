# 0.2.12 安装后隔离 API 验收夹具

用途：用已安装的 Web Console 连接合成本地模型，验证工具策略、会话参数、流式展示协议、搜索和真实 usage。不会连接真实模型或操作正式 8765 实例。

可交付文件：`start-isolated.ps1`、`mock-openai.cjs`、`api-regression.cjs`、`verify-recorded-evidence.cjs`、本说明。其余 JSON/SSE/日志为本次运行证据，不是启动依赖。

## 运行

需要 Windows、已安装的 CoolzhuAgent、PowerShell、带内置 fetch 的 Node.js（建议 Node 20+）。每次完整复测请选择一个全新空目录；脚本会新建1个合成会话和4个聊天室，重复复用旧目录可能达到房间数量上限。

在含上述夹具文件的目录中执行，安装路径和端口可以通过参数更改：

```powershell
$acceptanceDir = Join-Path $env:TEMP ('coolzhu-release-acceptance-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
& .\start-isolated.ps1 -RuntimeDirectory $acceptanceDir -Port 18775 -MockPort 18776
$env:RELEASE_ACCEPTANCE_DIR = $acceptanceDir
$env:RELEASE_BASE_URL = 'http://127.0.0.1:18775'
$env:RELEASE_MOCK_URL = 'http://127.0.0.1:18776'
node .\api-regression.cjs
node .\verify-recorded-evidence.cjs
```

启动脚本拒绝8765、相同端口、无效端口及已被占用的验收端口；不会关闭已有服务。已有配置/会话但没有 `.release-acceptance-fixture` 标记的目录也会被拒绝。API脚本只允许 `http://127.0.0.1` 且拒绝8765，核对工作区路径、启动清单和模拟服务 PID，随后才新建数据。

默认安装程序：`C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe`。默认 `COOLZHU_WEB_STATIC_ROOT` 固定到 `C:\Program Files\CoolzhuAgent\modules\gui-web\packages\web-console`；其资源目录用于图标，缺少的HTML/JS/CSS走安装exe内嵌内容，不回退到开发源码。启动参数写入 `launch.json`。

隔离配置显式使用 `dev_open_permissions=true`、`computer_use.enabled=false`；只授予新建测试会话 `read_file`。这测试显式调试完全访问，不验证 release 缺省完全访问是否关闭。脚本创建 `mock-key.txt`，唯一内容是无效的合成凭据 `mock-key`；不读取真实安装工作区的配置或密钥。模拟请求日志只记录匹配该合成凭据的布尔值，不记录鉴权头。

## 自动检查

1. `model-settings` 保存与回读：OpenAI协议、精确Endpoint、32K上下文、2048输出、0.35温度、0.9 Top P、high思考、工具开关与名单。
2. 安装内嵌样式可达；保存房间权限与 effective 调试完全访问字段正确。
3. 流式 `read_file` → 真实runtime读取合成文件 → ToolResult回传 → 最终答案；同时断言真实工具结果成功，不能仅凭mock最终文案判断成功。
4. 思考事件早于正文；2次模型请求usage恰为200输入/40输出；回复耗时已保存；上述采样/思考/输出参数反映到mock收到的真实请求。
5. 完整记录搜索与around定位；搜索/索引不暴露内部工具结果。
6. dev_open开启但会话工具关闭时，发送的schema为空、不执行read_file。
7. 流式正文伪调用→一次无工具恢复成功；非流式持续伪调用→一次恢复后明确未完成。
8. 流式越界结构化write_file→拒绝执行并恢复；三种恢复均只有2次请求、保留原任务、没有创建禁止文件。

`api-results.json` 为PASS/FAIL摘要；`api-regression.log` 可由调用者重定向；各 `*-events.json` 是合成SSE证据；`mock-requests.jsonl`为无凭据的请求元数据；`test-context.json`记录新会话/房间ID；`evidence-verification.json`可记录离线结果复核输出。

服务保持运行供浏览器验证。打开 `launch.json` 中URL，当前激活“Release安装后主验收”及“Release 0.2.12 隔离验收”。输入“模拟工具验收 [mock-tool]：请读取隔离文件并报告标记”可重复查看思考、工具状态和正文。浏览器的新请求会增加usage，比较时以 `read-file-insights.json`（2次）或 `final-insights.json`（本次脚本结束时3次）为固定证据，不能要求页面永远停留在同一个计数。

清理时只处理本次 `launch.json` 中经核对身份的进程；脚本不会自动停止服务或删除数据。本夹具不覆盖真实本地模型质量、真实桌面操作、断连/取消竞态、安装升级迁移或重启持久性；这些应单独验收。
