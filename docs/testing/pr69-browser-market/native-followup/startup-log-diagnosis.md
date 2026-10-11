# 正式安装版启动动画日志诊断（2026-09-28）

本次仅核对正式安装版进程、诊断日志、安装资源与对应源码；未操作界面、发送模型请求或更改输入安全状态。动画现场播放仍未验收。

## 已证实

- 22:35:38 启动的 `coolzhu-tauri-shell.exe` PID `9212` 来自 `C:\Program Files\CoolzhuAgent\bin\`，命令行只有 `--web-console-pid=32852`。`startup_performance_requested()` 对这类默认启动返回 true；只有显式 `--show-console` 等路径跳过。因此本次不是因启动参数要求跳过动画。控制台 PID `32852` 同样来自正式安装目录。
- 安装目录中存在 `launch-performance.html`、CSS、播放器、清单、桥接脚本以及清单引用的山水、人物和六张字形 PNG，文件非空。演出页、播放器、清单与桥接脚本的 SHA-256 均与当前仓库对应文件一致。Tauri 配置的 `frontendDist` 为 `../ui`，能力配置包含 `launch-performance`。这些事实确认安装文件层面未缺素材，但不能证明 WebView2 当时确实加载了内嵌资源。
- 同次 Web Console 启动日志于 `2026-09-28T14:35:37.706723Z` 报告 `candidates=2, converged=0, settled=2`、`opening=KeptIsolated`、`legacy_unconverged_runs=2`、`human_review_required=2`，当前资源范围 `windows-session-2`。Luna 的安装版 B05 截图也显示输入为 `unknown`、待本人复核 2、遗留 2。旧运行没有因本次重启而完成；Paint 真实物理输入仍不可验收。两条旧运行的证据边界见[历史只读诊断](../../release-0.2.17/installed-input-safety-recovery-diagnosis.md)。

## 日志能说明与不能说明的事

本次 `package-launcher/tauri.stdout.log`、`tauri.stderr.log` 均为 0 字节。默认诊断路径 `C:\Users\zhupu\.coolzhu\logs\coolzhu-tauri-shell.jsonl` 也是旧的 0 字节文件，未留下本次 `startup_performance_window_build_failed` 或 `startup_performance_timeout` 事件。Web Console 的 stdout 仅确认服务已启动和输入隔离，没有动画完成记录。因此**没有找到建窗失败或 15 秒宿主超时的正面证据，也不能用空日志证明动画成功**。

播放器的正常日常时长为 980 毫秒；首次为 4.6 秒。减少动态效果会在约 120 毫秒后结束；图片加载失败或超过 2 秒会以 `resource-error` / `resource-timeout` 结束；桥接还有 7 秒看门狗。首次/日常由 WebView 的 `coolzhu.scroll-startup.seen.v1` 决定，成功播放、跳过或减少动态效果后会置位；读取 storage 失败也回退日常。本轮未读取实际 WebView storage。桥接只把完成原因发成 `launch-performance-complete` 事件，宿主收到后隐藏动画、显示控制台，未持久化原因。因此仅见控制台，无法区分 980 毫秒日常播放、减少动态效果、资源错误、跳过或其他交接路径。

## 最低后续动作

保留“动画未验收”结论。下一次正式安装版原生验收需从同一入口捕获可观察的连续播放证据，记录实际首次/日常模式、动画起止与交接聊天室；若再次只见控制台，再针对当次进程采集动画完成原因或 WebView 资源错误。现有证据不足以指定某个素材或建窗逻辑为故障点，不据此盲修。Paint 继续保持输入隔离，待两条旧运行按独立证据完成复核和正式放行后再发真实模型测试；不能把重启或旧截图当作通过。
