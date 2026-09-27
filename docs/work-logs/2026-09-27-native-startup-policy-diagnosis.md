# 2026-09-27 原生启动策略拒绝定位与安装版验证

## 历史拒绝定位

本次只针对当前 Codex 任务 `01a0b94c-b964-7390-a446-776f760d90d4` 的本地会话日志定位。日志第 14923 行（2026-09-26 23:38:01 UTC）记录了历史命令：

```powershell
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8767'; $p=Start-Process -FilePath 'C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe' -ArgumentList '--web-console-pid=26032' -WorkingDirectory 'C:\Program Files\CoolzhuAgent' -WindowStyle Normal -PassThru; $p.Id
```

紧接着第 14925 行返回 `exec_command failed: CreateProcess { message: "Rejected(... pwsh.exe -Command ... rejected: blocked by policy)" }`，用时 0.0 秒，没有 PowerShell 或应用程序输出。拒绝发生在 `exec_command` 创建命令进程的外部策略层；记录没有提供具体策略规则名，不能进一步断言触发原因，也没有证据表明项目自身权限门禁执行过。未重试这条命令，未改动 Codex 或 Windows 安全策略，未关闭防护。

## 正式入口验证

开始菜单快捷方式只读核对表明正式入口为 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，无附加参数，工作目录为安装目录。使用同一个 `tools.exec_command` 对此正式入口做一次最小启动验证，命令如下：

```powershell
Start-Process -FilePath 'C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe' -WorkingDirectory 'C:\Program Files\CoolzhuAgent' -WindowStyle Normal -PassThru
```

工具返回 `exit_code=0`、启动器 PID `19648`。只读进程检查确认其启动了 `coolzhu-web-console.exe` PID `20992` 与 `coolzhu-tauri-shell.exe` PID `15892`。Computer Use 返回唯一的 `COOLZHU AGENT 控制台` 原生窗口；将其切到前景后，捕获到无遮挡首页，再点击左侧“统计信息”并立即重新观察，右侧统计面板成功打开。两张实测截图及安装身份记录见 [evidence-installed](../testing/release-0.2.17/evidence-installed/verification.md)。

安装版只读运行时身份：`http://127.0.0.1:8765/`、工作区 `C:\Users\zhupu\coolzhuagent`、`schema/supported=27`、构建 `2c15397688eb · 2026-09-26`、`computer_use_store=ready`、`legacy_unconverged_runs=2`。这次验证确认标准启动路径与一次无副作用原生窗口导航可用；不代表 Paint、原生计算机操作或全部 0.2.17 功能验收完成。未修改产品代码，窗口保留供后续验收。
