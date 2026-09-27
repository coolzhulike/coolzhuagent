# 已安装版原生启动与窗口验证

验证日期：2026-09-27（北京时间）。本记录对应本机已安装的 0.2.17，保留原始截图，不代表其他功能已完成验收。

## 安装与运行身份

- 开始菜单快捷方式：`C:\ProgramData\Microsoft\Windows\Start Menu\Programs\COOLZHU CODE\COOLZHU CODE Agent.lnk`。
- 快捷方式目标：`C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`；无附加参数；工作目录为 `C:\Program Files\CoolzhuAgent\`；正常窗口模式。
- 实际启动命令：`Start-Process -FilePath 'C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe' -WorkingDirectory 'C:\Program Files\CoolzhuAgent' -WindowStyle Normal -PassThru`。
- 启动命令退出码为 0，返回启动器 PID `19648`。随后进程清单显示子进程 `coolzhu-web-console.exe` PID `20992` 与 `coolzhu-tauri-shell.exe` PID `15892`，两者父 PID 均为 `19648`。启动器结束后子进程继续运行。
- 只读运行时身份检查：服务地址 `http://127.0.0.1:8765/`，实际工作区 `C:\Users\zhupu\coolzhuagent`，`schema/supported=27`，构建 `2c15397688eb · 2026-09-26`，`computer_use_store=ready`，`legacy_unconverged_runs=2`。该信息来自主会话对安装版运行时接口的只读检查。

## 原生窗口操作

Computer Use 的 `sky.list_apps` 返回唯一运行窗口：进程 `C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`，窗口标题 `COOLZHU AGENT 控制台`，窗口 ID `6292966`。切到该窗口前景后，`sky.get_window_state` 捕获到无遮挡的控制台画面与 Web 内容区域。随后按观察到的左侧“统计信息”入口点击一次，立即重新捕获窗口；右侧出现“统计信息”面板，左侧对应图标高亮。

- 首页，导航前（私有现场原图 01-native-console-before.jpg 留在本机，未纳入 PR）
- 统计信息，导航后（私有现场原图 02-native-usage-open.jpg 留在本机，未纳入 PR）

两张原始截图均为 Computer Use 返回的 `1443×897` JPEG。操作仅涉及前景切换与无副作用页面导航；未发送消息、修改配置、处理权限提示或触碰 Codex 窗口。原生窗口保留，供后续验收。
