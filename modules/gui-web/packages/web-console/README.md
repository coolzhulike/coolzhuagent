# COOLZHU AGENT Web GUI

这是 COOLZHU AGENT 的本地浏览器控制台原型。当前版本已经从纯静态页面升级为本地 HTTP 服务，前端页面通过 API 接入后端状态、桌面截图和键鼠视觉稳定性测试。

## 启动方式

```powershell
cargo run -p coolzhu-web-console -- --open
```

默认地址：

```text
http://127.0.0.1:8765/
```

如需修改监听地址，编辑工作区 `coolzhu.toml` 的 `[web]` 段（启动时由 `config_web_bind_addr` 读取 `[web].bind_addr`）：

```toml
[web]
bind_addr = "127.0.0.1:8877"
```

## 已接入接口

- `GET /api/state`：返回 GUI 总览、工程目录、模型配置、截图状态、工具状态和稳定性测试摘要。
- `GET /api/web/cards`：返回 Web-GUI 主卡片 API 接入审计，列出 endpoint 与 loading/error/empty 缺口。
- `GET /api/workspace` / `POST /api/workspace`：读取或切换工程目录，返回允许根目录和目录树。
- `POST /api/capture`：采集当前桌面截图，并保存到固定的最新截图路径。
- `GET /api/capture/latest`：返回最新桌面截图的本地路径、文件大小和更新时间。
- `GET /api/capture/latest/image`：返回最新截图缩略图内容。
- `GET /api/stability/matrix`：返回多分辨率锚点映射矩阵。
- `POST /api/stability/run`：执行键鼠视觉操作稳定性 dry-run，不执行真实点击。
- `POST /api/computer-use/closed-loop`：执行视觉操作闭环，包含执行前截图、锚点定位、ROI 区域、可选真实输入、执行后截图和画面变化校验。
- `GET /api/computer-use/capabilities`：分别报告 Browser DOM、UIA、无模型模板、本地/远程视觉和人工确认的观察能力；ShowUI 不可用时不会阻断 UIA/template dry-run。
- `POST /api/vision/locate`：按 `uia → ocr_template → local_vlm → remote_vlm` 顺序定位；`ocr_template` 当前支持颜色按钮模板，无法确定目标时返回 `skipped/unavailable` 并要求人工确认。

### 无 ShowUI 时的 Computer Use 降级

默认 grounding 路由优先使用结构化 UIA（AutomationId、Name、ClassName、ControlType），其次使用无需模型的颜色/几何模板，再尝试本地或远程视觉模型。浏览器任务应优先使用已连接扩展提供的 DOM 引用。真实输入前必须保留最新截图/UIA 证据并经过权限闸门；所有自动定位失败的请求只返回 dry-run 计划，不执行盲目坐标点击。

可在 `coolzhu.toml` 的 `[vision.router]` 中显式调整顺序：

```toml
[vision.router]
pipeline = ["uia", "ocr_template", "local_vlm", "remote_vlm"]

[vision.router.ocr_template]
enabled = true
confidence_floor = 0.72
```

## 实时视觉语音验收

在 `8765` 已启动、UI-DETR 检测服务按配置可达时，可以复跑前端录屏验收：

```powershell
node modules/gui-web/packages/web-console/tools/realtime_e2e_record.js
```

脚本会通过真实前端按钮启动 unified realtime session，使用浏览器 fake mic 触发 `MediaRecorder` 分段，上报一条浏览器侧 partial ASR，发送一轮模型消息，并验证最终回复触发浏览器 TTS 播放。产物默认写入 `output/playwright/`，包括 `.webm` 录屏、运行截图、最终截图、TTS wav 和 JSON 摘要。

实时自动朗读会向 `/api/audio/tts/speak` 请求 `segment=true`。后端仍兼容旧的 `audio_url`，同时返回 `audio_urls` 队列和每段元数据；前端用浏览器音频队列顺序播放，停止实时会话、手动停止或有效 barge-in 会中断当前段并阻止后续段继续播放。

普通 partial ASR 在系统处于 listening 且没有正在输出时会被标记为 `user_turn`，不会误把 `Audio output loop` 改成 `cancelled`。只有 TTS/模型输出正在进行或用户手动停止时，barge-in 才会取消当前输出。

### 真实 STT/TTS 回归

需要验证真实语音链路时，不使用浏览器 fake mic，也不注入 `/api/audio/realtime/partial`。先在固定端口 `8765` 重启 web-console，并确认 `/api/audio/status` 返回 `stt_available=true` 和 `tts_available=true`，然后运行：

```powershell
$env:COOLZHU_E2E_PREFIX="real-stt-tts-regression"
$env:COOLZHU_E2E_HEADLESS="1"
node modules/gui-web/packages/web-console/tools/realtime_real_speech_e2e.js
```

该脚本会用后端 TTS 合成一段 wav，再把同一段音频交给真实 STT 和 realtime final segment STT；随后打开真实前端，选择当前已勾选会话，发送识别文本，等待非工具类助手回复，并只把回复正文交给浏览器 `Audio.play()` 播放。脚本不会使用 `--use-fake-device-for-media-stream`、`--use-file-for-fake-audio-capture`、fake mic fixture、partial ASR 注入或 full-streaming readiness gate 伪验收。

### 真实语音打断小场景

用于验证“模型正在语音回复时，用户新语音打断并切换意图”的小场景：

```powershell
$env:COOLZHU_E2E_PREFIX="real-voice-interrupt-scenario"
$env:COOLZHU_E2E_HEADLESS="1"
node modules/gui-web/packages/web-console/tools/realtime_voice_interrupt_scenario_e2e.js
```

默认验收语义为：先让模型简单介绍自己，再用第二段真实 TTS/STT 语音打断，要求模型介绍 CoolZhu Agent 的功能。脚本通过真实 TTS 合成两段语音、真实 STT 转写、realtime final segment STT、前端真实 `handleRealtimeBargeInDecision` 停止当前 TTS、真实前端 composer 发送新意图，并验证最终回复包含 CoolZhu Agent 的能力描述。当前本地 STT 为英文模型，品牌词 `CoolZhu` 可能被识别成近似音如 `KUOzu`，脚本会记录真实转写并把该近似音归一到 CoolZhu Agent 场景语义。

### 真实桌面视觉语音验收

用于验证“说一下当前桌面上你看到有哪些东西，并语音播报”的小场景：

```powershell
$env:COOLZHU_E2E_PREFIX="real-desktop-vision-voice"
$env:COOLZHU_E2E_HEADLESS="1"
node modules/gui-web/packages/web-console/tools/realtime_desktop_vision_voice_e2e.js
```

该脚本会先调用 `/api/capture` 采集当前桌面，再读取 `/api/capture/latest/image` 并通过 `/api/pet/drop-upload` 上传到标准附件目录；随后在前端选择一个视觉能力会话，把截图作为真实 image 附件随 `/api/chat/send/stream` 投递给模型，等待非工具类助手回复，最后只把最终回复正文交给浏览器 TTS 播放。播报前会把 `CoolZhu`、`Cool Zhu` 或近似音归一为“酷猪”。脚本不使用 fake mic、不使用浏览器假媒体设备、不把 readiness gate 当作通过证据。

当前链路边界：

- 已真实验收：桌面截图、附件上传、图片进入视觉会话、前端流式渲染、最终回复 TTS 播放。
- 已真实验收于独立脚本：后端 TTS、后端 STT、realtime final segment STT、语音打断停止当前 TTS 并切换任务。
- 待硬件/后续验收：物理麦克风实时采集。麦克风设备异常可按硬件问题搁置，但软件入口不可移除。
- 待补齐全实时：provider-native partial ASR、far-end reference AEC、realtime model adapter。当前请求 `full_streaming` 会显式降级为 `half_duplex_guarded`，不做假通过。

## 安全边界

当前稳定性测试默认只做 dry-run：验证桌面图标、任务栏、浏览器、窗口控件、全屏软件表面的锚点到物理坐标映射。真实键鼠注入仍保留安全闸门，后续确认后再接入。

闭环接口默认也不执行真实点击。只有请求体显式传入 `execute: true` 时才会触发鼠标或键盘输入：

```json
{
  "scenario": "desktop-icon-left-click",
  "execute": false,
  "confirm_after": true,
  "roi_radius": 64
}
```
