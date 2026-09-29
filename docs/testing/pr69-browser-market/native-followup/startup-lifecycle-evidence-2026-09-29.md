# 2026-09-29 启动演出生命周期证据补充

## 边界与原因

正式入口 `COOLZHU-AGENT.exe` 是无可视窗口的启动器；2026-09-29 的 Sky `launch_app` 等待约 42 秒才返回，此时只能截到主控制台。现有截图不能证明卷轴演出曾在屏幕上播放，也不能证明演出失败。因此动画 B 继续记为“原生画面证据缺失”，不改判为通过。

## 本次代码变更

- 演出页按顺序向宿主报告开始、素材就绪、Canvas 首帧绘制、结束；报告只含首次/日常/恢复模式、减少动态效果标志、有限枚举结束原因、耗时和帧数。
- 原生宿主仅接受 `launch-performance` WebView 从本地演出页发出的专用命令；只允许无查询参数或固定的 `mode=first/daily/restore`，拒绝未知字段、原因和越界数值。不记录任意 URL、查询内容或用户内容。
- 沿用 `coolzhu-tauri-shell.jsonl`，新增 `startup_performance_window_created`、`startup_performance_phase`、`startup_performance_handoff` 等记录。完成报告后记录主控制台是否可见；建窗失败、关闭和 15 秒宿主兜底也记录交接结果。原生上报失败不再静默吞掉，页面报错后仍由现有兜底保障主界面出现。
- 未修改卷轴、玉石竹林画面、首次 4600 毫秒、日常 980 毫秒、减少动态效果 120 毫秒或跳过交互；未增加重播按钮。

## 验证与剩余验收

- `node modules/gui-desktop/packages/tauri-shell/ui/tests/coolzhu-seven-letter-bridge.cjs` 通过：覆盖完整、日常、跳过、减少动态、资源超时、恢复、创建失败和原生命令拒绝。
- `cargo test -p coolzhu-tauri-shell --offline startup_performance`、`cargo test -p coolzhu-tauri-shell --offline startup_report_command` 通过；`cargo build -p coolzhu-tauri-shell --offline` 通过。高刷新率下 1656 帧合法，超过 15000 帧拒绝。
- 本轮没有重启或操作正在运行的安装版，也没有读取或更改 WebView localStorage、会话或用户数据。

诊断日志只能证明演出页执行了绘制回调、宿主接到报告并完成窗口交接；它不能单独证明画面真实显示，也不能证明人物和卷轴动作质量。下一次安装版验收须在触发正式入口之前准备独立的连续画面采集，并与同次启动的诊断时间线对齐。不要等待无窗口的启动器 `launch_app` 返回后才开始截图。
