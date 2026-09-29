# 2026-09-30 启动动画原生并发采集：0.2.28

## 范围与前置状态

- 仅使用 `node_repl` + `@oai/sky` 做窗口操作和原生窗口截图；没有使用 Playwright、桌面直连截图或其他输入通道。
- 启动前只读核对 `.coolzhu/web-sessions.sqlite3`：没有运行中的模型或 computer-use run。
- 旧实例 `coolzhu-tauri-shell.exe` PID `31040` 通过 Sky 窗口的 `Alt_L+F4` 正常退出；退出后的 `list_windows()` 不再返回该窗口。
- Paint `无标题 - 画图` 窗口 `34605722` 全程保留，没有调用其状态捕获、激活或任何输入。
- 正式入口只调用一次：`C:/Program Files/CoolzhuAgent/COOLZHU-AGENT.exe`；没有修改配置、localStorage 或动画设置，也没有发送新的 Qwen 请求。

## 并发方案与实际时间线（Asia/Shanghai）

同一 `node_repl` 调用先创建 `list_windows()` 异步轮询，再立即发起唯一一次 `sky.launch_app()`；两者通过 `Promise.allSettled` 汇合。轮询每次只允许一个 `get_window_state` 在途，发现唯一真实 shell 窗口后使用返回的 `{id, app}` 经 `get_window()` 重新绑定，再捕获 `get_window_state({include_screenshot:true})`。

1. `00:22:30.885`：轮询启动，并在同一调用中发起正式入口。
2. `00:22:30.908`：首次 `list_windows()` 返回，未发现目标 shell。
3. `00:22:44.519`：第二次窗口观察才返回，发现唯一真实 shell：窗口 `920328`，应用 `process:C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`，标题 `COOLZHU AGENT 控制台`。
4. `00:22:44.684`：对该真实窗口完成首个 `get_window_state` 截图并保存 B-25。
5. `00:22:44.836`：轮询结束；`launch_app` 同期返回 `rejected`：`launched app did not expose a targetable window: {6D809377-6AF0-444B-8957-A3773F02200E}\\CoolzhuAgent\\COOLZHU-AGENT.exe`。

## 并发结论

轮询配置了 150ms 间隔、9 秒截止和最多 20 次捕获，但实际 `windows_seen` 只有两次：首次为空，随后间隔约 13.6 秒才看到 shell。`launch_app` 没有阻塞 JavaScript 的 `Promise.allSettled` 结构本身，但底层 Sky/窗口通道在启动期间没有让轮询持续取得窗口观察，表现为调用串行阻塞或排队。因此该方案能在启动器完成后捕获真实 shell，但不能保证采集短启动动画首帧。

启动后只读进程核对显示新实例已运行：`coolzhu-tauri-shell.exe` PID `6524`、`coolzhu-web-console.exe` PID `32976`。没有再次启动或关闭新实例。

## 截图证据（B25 起）

- [B-25-0.2.28-startup-concurrent-01.jpg](B-25-0.2.28-startup-concurrent-01.jpg)：由真实 shell 窗口 `920328` 的 `get_window_state` 返回，1443×897，原生截图区域 `originX=198, originY=12, zIndex=0`，174,726 bytes，SHA-256 `B5D905B4E98F12D5168F8D078675EB694ABEB360E7D0F9313AA3F459F7170107`。
- B-25 画面显示的是启动完成后的 COOLZHU 主控制台，内容为此前 Qwen Paint 的 `blocked / verification / invalid_verification` 终态卡片；没有显示可据以判断的启动动画帧，因此不能把本次结果称为“动画捕获成功”。
- 原始采集元数据：[tmp/startup-concurrent-capture-20260930.json](../../../../tmp/startup-concurrent-capture-20260930.json)。

结论：正式入口只启动一次，旧实例已正常退出，Paint 保持不动；真实 shell 截图已捕获，但启动期间窗口查询出现长等待，表现与底层调用排队相符，不能仅据本次时序确认工具内部串行实现。启动器报告未暴露 targetable window，最终只得到启动后的主界面，未捕获到可验证的短启动动画。按要求没有盲目重试。
