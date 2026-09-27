# S5.4 受控交互终端工作记录

日期：2026-09-27。分支：`codex/navigation-update-followup`。

在现有 `windows-process-guard` 中增加固定 PowerShell 的原生 ConPTY：创建时先暂停子进程、加入关闭即清理的 Job，再恢复执行；输出由独立线程持续排空并保持有界缓存，输入有界，支持调整尺寸、Ctrl+C 与关闭。`PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` 接收 HPCON 值本身；`STARTF_USESTDHANDLES` 使重定向启动的 Web 不把自己的标准句柄传给终端进程。这两个 Win32 参数均经过真实 PowerShell 测试，而非仅依赖编译。

Web 宿主把终端绑定当前工作区、Agent 与聊天室，以不可猜测句柄和代际撤销阻止跨范围续接。每次 API 操作短暂固定工作区；启动和每次输入、调整尺寸均要求当前聊天室完整访问授权并经过既有 PowerShell 权限 gate。授权降级后状态不返回终端文本，输入和输出被拒绝，自有终端仍可中断或关闭。阻塞的创建、读取和回收在专用阻塞任务内完成。

右栏提供运行、中断、关闭、清空与输出续读；刷新后在同一范围内恢复句柄。输出仅写入 `textContent`，有限状态解析跨批次的 VT 控制序列。PowerShell 重绘时的清屏和光标回首页按清屏处理，不将旧屏内容反复追加。此处是受控命令面板，不承诺完整 TUI 兼容。

低层真实测试 `real_powershell_survives_interrupt_and_job_close` 已通过：由文件副作用确认中文环境中的交互执行，15 秒长命令被 Ctrl+C 中断、shell 保持存活、后续命令成功；关闭时已启动且仍存活的后代进程与 shell 一并退出。日志为 `tmp/s54-conpty-powershell-job-tree.log`。

隔离 Web HTTP 验收使用随机本机端口和独立工作区，设置 `dev_open_permissions=false`，记录被测可执行文件 SHA-256：`310f214d3ef55efcd8dc59377ce3ab839f2d4d3e259f49fb1e3e951971504819`。默认工程写入权限拒绝创建；完整访问后中文命令、同范围续读、游标去重和伪造房间拒绝均通过；降为工程写入后输入/输出拒绝且状态/中断响应不泄露输出，自有句柄仍可中断/关闭；恢复授权后 shell 继续工作，换聊天室使旧句柄失效。结果与细节归档在 `docs/testing/navigation-update-followup/s54-controlled-terminal.md`。公开聊天室权限设置接口仅支持 `workspace-write` 与 `full-access`；代码中的 `read-only` 防御分支未由该公开接口实测。

前端补充 `tmp/s54_vt_smoke.cjs`，跨批次清屏、首页、OSC 与退格烟测通过；脚本及日志保留在忽略的 `tmp/`。页面实操使用包含最终终端资源的 SHA-256 `dfd39612cd2d689e6cf4e87de9b0c421682d1be33137d854a4ce8f5d2479d8ce` 可执行文件：中文输出、目录切换和文件副作用、刷新续接、页面中断长命令、后续继续输入与关闭均通过，三张截图与脱敏结果在测试归档。后续并行改动完成后仍需统一回归，不能把此单点构建当整轮冻结包。
