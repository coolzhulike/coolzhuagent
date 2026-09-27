# S5.4 受控终端验证证据

2026-09-27，在 Windows 开发机对当前分支源码进行隔离验证。以下服务均使用 `tmp/` 下独立工作区与随机本机端口；未请求或停止用户正在使用的 8765 服务。测试帐号、Agent、聊天室和命令均由验证脚本临时生成。

## 原生终端进程

`cargo test -p coolzhu-windows-process-guard --offline real_powershell_survives_interrupt_and_job_close -- --ignored --nocapture` 通过（`tmp/s54-conpty-powershell-job-tree.log`）。测试不是只匹配终端回显：它先检查命令实际生成文件，再确认 15 秒长任务已开始；Ctrl+C 后 5 秒内出现新提示符、长任务的后续文件未生成、下一条命令生成新文件且 shell 未退出。测试还打开实际后代进程 handle，验证关闭前仍运行，关闭 Job 后 shell 与该后代均在期限内退出。该低层测试运行时未采集测试可执行文件哈希，不能用后续编译产物回填。

## 真实 HTTP 契约

`cargo build -p coolzhu-web-console --offline` 通过（`tmp/s54-terminal-web-build-4.log`）。随后运行 `python tmp/s54_terminal_e2e.py`，被测 Web 可执行文件 SHA-256 为 `310f214d3ef55efcd8dc59377ce3ab839f2d4d3e259f49fb1e3e951971504819`，结果见 [脱敏 JSON](s54-controlled-terminal-http-result.json)。隔离配置的 `dev_open_permissions=false`。

默认工程写入权限下创建终端返回 403；完整访问授权后可以创建并在同一 Agent/聊天室续接，中文命令真实写入文件，输出游标不重复，伪造聊天室无法读取。把公开权限从完整访问降到工程写入后，新输入与输出均返回 403，状态及中断响应不含旧输出，但自有终端仍可 Ctrl+C 和关闭；长命令确实停止，恢复完整访问后 shell 可继续执行。关闭后状态无活动句柄，切换聊天室撤销旧句柄。公开权限设置 API 不接受 `read-only`，因此本轮没有宣称通过只读权限配置的 HTTP 测试。

## 页面

内联资源更新后的 Web 构建通过；隔离页面实操使用的可执行文件 SHA-256 为 `dfd39612cd2d689e6cf4e87de9b0c421682d1be33137d854a4ce8f5d2479d8ce`，见 [脱敏结果](s54-controlled-terminal-ui-result.json)。Playwright 独立浏览器在临时端口操作右栏，没有触碰用户 8765 服务。页面控制台无错误。

[运行截图](s54-terminal-running.png)显示终端状态、中文输出与 `目录=deep`；脚本检查终端命令在切换后的 `deep` 目录真实生成 `ui.txt`。输入框很窄且自动滚至长命令末尾，截图只显示命令的后半段；同期 `tmp/s54-terminal-ui-final-4.txt` 快照保留完整原始输入，并无另加“已执行”标注。刷新页面后，终端状态和此前输出在同一范围恢复。

第一次尝试用 15 秒长命令时，页面点击耗时使该命令在中断前自然结束，因此**不把它计作中断证据**。随后另用 60 秒长命令：先确认 `started-ui2.txt` 存在且 `late-ui2.txt` 不存在，页面点击“中断”后长命令后续文件始终未出现；下一条页面命令创建 `resumed-ui.txt`，且输出出现“中断后继续”，见[中断与继续截图](s54-terminal-interrupt-resume.png)。[关闭截图](s54-terminal-closed.png)显示“终端已关闭”。测试浏览器及隔离服务随后正常关闭。

页面按约定支持 PowerShell 文本交互及有限的 VT 清屏/光标回首页处理；窄栏中的长路径及命令会被 PowerShell 自身折行，`>>` 是其屏幕文本，本轮没有承诺完整 TUI 屏幕模拟。安装版最终验收可用短命令与整个应用窗口重新取景。
