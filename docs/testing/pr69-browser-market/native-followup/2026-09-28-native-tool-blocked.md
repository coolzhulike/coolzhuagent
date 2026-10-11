# 原生安装入口复测：工具路径误判与额度中断（2026-09-28）

## 结论

本次没有开始动画或 Paint 原生验收。测试代理误用了只提供浏览器接口的 `cua_repl`；其失败不能证明 Windows Computer Use 不可用。主会话已发现 `mcp__node_repl__js`，技能指定的正确入口为 `node_repl` 中导入 `@oai/sky`。下列旧探测记录只说明工具路径选择错误，不属于产品故障或权限拒绝。

主会话要求同一个 GPT-5.6 Luna max 代理改用正确接口继续，但该轮随即因使用额度耗尽失败，未执行新的原生操作。工具返回建议重试时间为 `Oct 4th, 2026 1:34 AM`，未标注时区。本轮未替换测试模型、未修改产品代码或用户安全状态，也没有新截图。

## 工具能力证据

- 首次 `cua.getState()` 返回 `apps: []`，只列出 Codex In-app Browser，且没有标签页。
- `cua` 暴露的键为 `rewriteDocumentation`、`getState`、`browsers`、`getBrowser`、`createBrowserTab`、`getTab`、`listBrowsers`、`listTabs`、`initialize`；`cua.computer` 为 `undefined`。
- 调用 `cua.listApps()` 与 `cua.listWindows()` 均返回 `not a function`。
- 调用 `cua.computer.launch_app(...)` 返回 `Cannot read properties of undefined (reading 'launch_app')`。
- 初始化接口再次刷新后仍为 `apps: []`。

## 安装入口与身份核验

开始菜单唯一命中的快捷方式：

`C:\ProgramData\Microsoft\Windows\Start Menu\Programs\COOLZHU CODE\COOLZHU CODE Agent.lnk`

其目标为 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，快捷方式参数为空，工作目录为 `C:\Program Files\CoolzhuAgent\`。安装配置 `C:\Program Files\CoolzhuAgent\config\package-launcher.json` 的 `web_console.args` 与 `tauri.args` 都为空；按 `packages/app-launcher/src/lib.rs` 的 `build_tauri_args` 逻辑，默认只注入 `--web-console-pid=<pid>`，不会注入 `--show-console`，所以若入口实际创建新实例，源码条件应请求启动演出。

本次只读进程核验未发现名称为 `COOLZHU-AGENT.exe`、`coolzhu-tauri-shell.exe` 或 `coolzhu-web-console.exe` 的进程，因此没有可冒充为本轮候选的运行身份。

安装文件（仅记录身份，不代表已加载）：

| 文件 | 修改时间 | SHA-256 |
| --- | --- | --- |
| `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe` | 2026-09-28 11:59:16 | `AE848F459C589CA8A2659F8FA42A9340626331F8E288EF1A03AD64090B9ED6D4` |
| `C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe` | 2026-09-28 11:59:08 | `D8A28AB561B71B1FD4DAB5E206A178E63DA0EB5F7CC1B0C792A273AF21A2BA6D` |
| `C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe` | 2026-09-28 11:57:24 | `CDC30E489990ECD7C24E756B548AD39C49B6DD45893B56C28ED7D9CA9D224773` |

源码构建候选 `modules/gui-desktop/packages/tauri-shell/src-tauri/target/debug/coolzhu-tauri-shell.exe` 与 `target/debug/coolzhu-web-console.exe` 未被当作安装入口或运行身份；本轮没有启动它们。没有产生原生窗口或截图，因此也没有截图文件可交付。后续需在指定测试模型恢复可执行后，使用正确的 `@oai/sky` 接口从该快捷方式启动并逐阶段保存首次 4.6 秒、日常约 980 毫秒、交接聊天室及 Paint 原始截图。不得清理用户配置来伪造首次启动。
