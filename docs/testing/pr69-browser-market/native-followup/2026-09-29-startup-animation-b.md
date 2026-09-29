# 2026-09-29 安装版 0.2.27 启动动画 B 原生验收

## 测试边界

- 正式入口：`C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`。
- 安装身份：web-console 当前文件 SHA-256 `BDC9CA403A4B33407D9330784E9291C9980AB53912E80071C305D81D34CA9968`；shell 当前文件 SHA-256 `05AB3E65C45B3FAEA21E7AA45AEC06BF47D85C3CE1532E00B14AEACC33CA82AB`。
- 启动前通过 Sky 确认安装版 shell/web 窗口不存在；Paint `mspaint.exe` PID 18272 保持打开。
- 启动前控制台没有“当前运行过程”或“中止当前回复”状态；本次没有发送模型请求，也没有改配置、localStorage 或用户数据。

## Skill 接口事实

本版本 computer-use Skill 的 `api.md` 只提供 `get_window_state()` 返回的单帧 `screenshots[].url` data URL，没有录制或视频捕获 API。因而本次按 Skill 支持的方式保存连续单帧；没有使用静态资源或主页截图代替动画。

## 唯一一次正式启动

- Sky 调用时间：`2026-09-29T06:18:28.601Z`（北京时间 `14:18:28.601`）。
- 调用：`sky.launch_app({ app: "C:\\Program Files\\CoolzhuAgent\\COOLZHU-AGENT.exe" })`。
- 原始结果：`launched app did not expose a targetable window: {6D809377-6AF0-444B-8957-A3773F02200E}\\CoolzhuAgent\\COOLZHU-AGENT.exe`。
- 没有再次调用正式入口。随后实际进程出现：
  - web-console PID 33512，启动 `14:18:32.6701319`，路径 `C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe`。
  - tauri-shell PID 33652，启动 `14:18:33.2822708`，路径 `C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`。
- Sky 能获得的第一个 COOLZHU 窗口是 shell `id=107153494`、标题 `COOLZHU AGENT 控制台`；没有获得 launcher/splash 的独立可定位窗口。

## 连续截图证据

- 连续捕获开始：北京时间 `14:19:11.092`；共 18 帧，约 250ms 间隔，最后一帧约 `14:19:17.766`。
- 所有帧都来自同一个真实窗口：`process:C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`、窗口 id `107153494`。
- 文件：`B-21-0.2.27-startup-after-exposure-00.jpg` 至 `B-21-0.2.27-startup-after-exposure-17.jpg`，均为 Sky 返回的 `screens[0]` 原始 JPEG。
- 原始 18 帧保留本机；PR 仅归档核验过的第 00、09、17 帧，避免提交重复主界面图。它们均不能作为动画通过证据。
- 已用 `view_image` 核对第 00、09、17 帧：三帧均显示 COOLZHU 主控制台、qwen 发送对象和验收聊天室，未出现启动动画帧。AX 文本从首帧起也是主控制台 DOM（`COOLZHU CODE 控制台`、当前聊天室、首页等），没有 launcher/loading 状态。
- 连续截图期间没有点击、输入、键盘或鼠标操作；Paint 未操作、未关闭。

## 结论

本轮证明了正式入口在退出旧实例后能启动安装版 web-console/shell，并最终到达主控制台；启动动画 B 没有获得可验收的视觉证据。原因是正式入口调用未暴露可定位的启动窗口，随后只能取得已经进入主控制台的 shell；连续截图开始时距离入口调用约 42 秒。`daily980ms` 或首演出时长本次没有直接测得，不能从主界面帧推断动画已完整播放。因此 B 项应记为“动画证据缺失/未通过验收”，而不是通过。
