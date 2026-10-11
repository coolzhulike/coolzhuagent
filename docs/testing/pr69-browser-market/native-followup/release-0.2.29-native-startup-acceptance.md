# 0.2.29 安装版原生启动验收（2026-09-30）

## 测试边界与前置核对

- 安装身份由主会话独立核对：web-console SHA-256 `0CDAA81CAA9E4C3F5BD09BC3095E57A4F1A24445C0614DF955F090BAE4EA33AA`；tauri-shell SHA-256 `A00A3F1A3E9E800E6C6A115161EA546FE068B76FBC5BF63F86F47EE30E8EBD7A`。
- 启动前没有运行中的 Coolzhu 进程；既有诊断日志 `C:\Users\zhupu\.coolzhu\logs\coolzhu-tauri-shell.jsonl` 为 0 字节，安装目录下没有可复用的 launcher stdout/stderr 日志，因此没有现成的 0.2.29 生命周期证据可接回。
- 启动前只读确认没有当前活跃模型进程；未发送 Qwen 请求、未改配置或 localStorage。
- Paint `无标题 - 画图` 保持打开且全程未操作；微信未测试。

## 唯一正式启动与原生采集

使用 `node_repl` + `@oai/sky` 在同一调用中先建立窗口轮询，再调用一次正式入口：

`sky.launch_app({ app: "C:/Program Files/CoolzhuAgent/COOLZHU-AGENT.exe" })`

- 调用开始：`2026-09-30 00:39:41.590 +08:00`。
- `launch_app` 原始返回：`launched app did not expose a targetable window: {6D809377-6AF0-444B-8957-A3773F02200E}\\CoolzhuAgent\\COOLZHU-AGENT.exe`。
- 轮询随后发现真实 shell 窗口 `id=14027790`，应用 `process:C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`，标题 `COOLZHU AGENT 控制台`；单次捕获在途，连续调用 `get_window_state({include_screenshot:true})` 取得 8 帧。
- 首帧截图返回：`2026-09-30 00:39:55.695 +08:00`；采集结束：`00:39:59.212 +08:00`。
- 启动后实际进程：web-console PID `12248`（约 `00:39:46` 启动），tauri-shell PID `29764`（约 `00:39:47` 启动）。没有再次启动或关闭新实例。

## 同次启动诊断日志（证明生命周期，不替代画面）

日志文件：`C:\Users\zhupu\.coolzhu\logs\coolzhu-tauri-shell.jsonl`，本次启动后大小 `1880` bytes。以下字段均来自本次启动追加的 JSONL 记录：

| 时间（+08:00） | 事件/字段 | 真实值 |
|---|---|---|
| 00:39:47.917 | `startup_performance_window_created` | `pid=29764` |
| 00:39:48.125 | `phase=started` | `mode=daily`, `reduced_motion=false`, `page_elapsed_ms=0`, `host_elapsed_ms=537`, `frame_count=0`, `performance_visible=true` |
| 00:39:48.138 | `phase=assets_ready` | `mode=daily`, `page_elapsed_ms=175`, `host_elapsed_ms=549`, `frame_count=0`, `performance_visible=true` |
| 00:39:48.421 | `phase=first_frame` | `page_elapsed_ms=468`, `host_elapsed_ms=832`, `frame_count=1`, `performance_visible=true` |
| 00:39:49.397 | `phase=finished` | `reason=completed`, `page_elapsed_ms=1444`, `host_elapsed_ms=1808`, `frame_count=185`, `performance_visible=true` |
| 00:39:49.444 | `startup_performance_handoff` | `route=reported-finish`, `console_visible=true` |

这些日志证明演出窗口生命周期报告了开始、素材就绪、Canvas 首帧、正常完成和主界面交接；它们不能单独证明动画像素实际显示。

## 原生画面证据（B26 起）

8 帧均来自同一个真实 shell 窗口 `14027790`，尺寸 `1443×897`，原生截图区域 `originX=198, originY=12, zIndex=0`。逐帧 SHA-256 如下：

| 截图 | SHA-256 |
|---|---|
| [B-26-0.2.29-startup-native-01.jpg](B-26-0.2.29-startup-native-01.jpg) | `51E8AB96DB42C6132A051CA38A4EAA2B429ED172208F037444973FDDB0AC7433` |
| [B-27-0.2.29-startup-native-02.jpg](B-27-0.2.29-startup-native-02.jpg) | `9F2025C6C5133514EE1D3A24068BEDEE4D9719CDAC21F2F01F807E811CDF3EE4` |
| [B-28-0.2.29-startup-native-03.jpg](B-28-0.2.29-startup-native-03.jpg) | `8FBC6C3EAB1710AA4188A03CA425B4BC2A6FBD56382DD33D6A41D834AE877F93` |
| [B-29-0.2.29-startup-native-04.jpg](B-29-0.2.29-startup-native-04.jpg) | `77A168334471DB01A6F922CCBBF2BFFBD981272959E385E2C8806EB04C91574B` |
| [B-30-0.2.29-startup-native-05.jpg](B-30-0.2.29-startup-native-05.jpg) | `327B28C12ACD46CA87E2E11AC119F8CFB26300052FA6F4C9D1B4FE01E0D913C0` |
| [B-31-0.2.29-startup-native-06.jpg](B-31-0.2.29-startup-native-06.jpg) | `AA1EF52F569A52F05EE9E8C1F88FE99D2F7F82678A06C2F8ED954347C66A8B65` |
| [B-32-0.2.29-startup-native-07.jpg](B-32-0.2.29-startup-native-07.jpg) | `4023466FA6603447D77B76D7EE4C18BE26D1CB42323F3383C563B651B8532A43` |
| [B-33-0.2.29-startup-native-08.jpg](B-33-0.2.29-startup-native-08.jpg) | `91F6DAC1FF430B4B42C29E25740EA2586C0129DBA911871A21690D22F379686E` |

对 B-26 与 B-33 的画面检查显示，两帧都是启动交接后的 COOLZHU 主控制台，未出现启动卷轴、人物或 launcher/loading 画面；其余帧来自同一连续窗口观察，也不能据此宣称动画像素已被捕获。日志证据与画面证据保持分开。

原始轮询与截图元数据保存在 [tmp/startup-0.2.29-20260930.json](../../../../tmp/startup-0.2.29-20260930.json)。

## 结论

0.2.29 本次唯一正式启动的生命周期日志完整：`started → assets_ready → first_frame → finished(completed) → handoff(reported-finish)`，日常模式、非减少动态效果、185 帧及交接字段均有真实记录。原生截图只捕获到启动后的主控制台，未捕获可验收的动画画面。因此本轮可以判定“启动演出生命周期诊断完整、主界面交接成功”，但不能判定“启动动画视觉验收通过”；没有再次启动、没有操作 Paint、没有发送模型请求。
