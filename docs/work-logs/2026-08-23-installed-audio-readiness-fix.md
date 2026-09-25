# 2026-08-23 安装包音频能力回归与修复

## 范围

重新安装 `CoolzhuAgent 0.2.5` 后，对已安装版本进行完整能力自检，并针对音频能力中发现的“空 WAV 误报成功”建立 issue、完成本地修复和回归验证。已安装的桌面进程保持运行，未在测试中强制终止。

## 已安装版本基线

- `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`：构建 `d0d85d68e742 · local`。
- Web 控制台：`http://127.0.0.1:8765/`，系统信息、模型会话、工具目录和前端加载正常。
- GLM-5.2 文本流：推理卡片、增量输出和只读工具调用正常。
- agnes 文本流：正常；按上传接口提交截图后，视觉模型正确识别控制台截图。
- ShowUI / Browser Use：本机未检测到本地 ShowUI 服务和浏览器扩展，按约定跳过真实执行，仅完成能力探测和 dry-run。
- 已安装包没有随包落地的 `models/` 音频模型；Windows 仅有英文 `Microsoft Zira Desktop` 语音，未安装语音识别器。

## 发现与 issue

已提交 [Issue #59](https://github.com/coolzhulike/coolzhuagent/issues/59)：

> 安装包缺少可选音频模型时，中文 TTS 返回空 WAV 且 readiness 误报可用

基线现象：`/api/audio/status` 报告 STT/TTS 可用；中文 TTS HTTP 200，但下载到的 WAV 只有 46 字节，RIFF/WAVE 的 `data` chunk 为 0；英文 TTS 可生成有效采样数据。该结果会让前端误以为中文语音可用。

原始回归材料：

- `tmp/logs/installed-tts-selfcheck-1787469115968.wav`：46 字节空 WAV。
- `tmp/logs/installed-tts-english-20260823.wav`：有效英文 WAV。
- `tmp/logs/installed-audio-selfcheck-20260823-151156.json`：实时音频 start/status/stop 回归。
- `tmp/logs/installed-capability-dry-runs-20260823-151124.json`：视觉、鼠标、闭环、浏览器探测 dry-run。
- `tmp/logs/installed-glm-readonly-stream-20260823-150613.sse`、`tmp/logs/installed-agnes-text-stream-20260823-150637.sse`、`tmp/logs/installed-agnes-image-uploaded-stream-20260823-150825.sse`：模型与视觉流式回归。

## 修复内容

`modules/gui-web/packages/web-console/src/audio.rs`：

1. 相对模型路径按开发目录和安装包 `bin/models` 目录解析，并在 STT/TTS readiness 中要求模型实际存在。
2. Piper 和 Windows 原生 TTS 输出统一校验 RIFF/WAVE 的 `data` chunk，拒绝空采样、截断 chunk 和缺少 data chunk 的文件。
3. Windows 原生 STT/TTS 探测改为检查实际安装的 recognizer/voice 数量，不再只检查程序集能否加载。
4. 音频状态仅返回实际存在的模型路径；缺少可选模型时，声音列表不再标记为可用。
5. 增加空 WAV 拒绝和有效 PCM WAV 接受的单元测试。

## 修复后实机回归

使用源码构建的 `coolzhu-web-console` 在隔离 workspace、`127.0.0.1:8777` 上运行：

- `/api/audio/status`：`stt_available=false`、`tts_available=true`、`stt_model=null`、`tts_model=null`，与机器实际安装情况一致。
- 中文 TTS：HTTP 500，返回 `No TTS backend available`，不再返回 200 空 WAV；日志同时记录 Piper 模型缺失和原生 TTS 空采样原因。
- 中文 TTS（错误信息增强后的再次回归）：HTTP 500，返回 `No TTS backend available: Piper: ...; Windows TTS: Native TTS output invalid: WAV data chunk 为空，没有可播放采样`，不再返回 200 空 WAV。
- 英文 TTS：HTTP 200，下载 WAV 101440 字节，其中 `data` chunk 为 101394 字节，包含有效采样数据；文件为 `tmp/logs/source-tts-english-8777-v2.wav`。
- `cargo test -p coolzhu-web-console --offline`：813 个测试通过（新增 2 个 WAV 校验测试）。
- `cargo build -p coolzhu-web-console --offline`：通过。
- 源码运行结果：`tmp/logs/source-audio-readiness-fix-8777-20260823-152914.json`。

## 修复版 release 包回归

- MSI：`dist/CoolzhuAgent-0.2.5-20260823-155056.msi`，180147712 字节。
- SHA-256：`14CE55AC1E42B8AED3E777408E75AF05FC3734BA128DFB2A00F1B068A472CCE`。
- WiX：`5.0.2+aa65968c`；包安全检查通过；未签名（`signed=false`）。
- 包内 `bin/coolzhu-web-console.exe` 构建版本：`0b4ebe3c5117 · local`。
- 未直接覆盖 Program Files 安装，以免中断用户当前会话；包内二进制在隔离 workspace、`127.0.0.1:8777` 启动成功，控制台标题为 `COOLZHU CODE 控制台`，DOM 页面加载正常。
- 包内 `/api/computer-use/capabilities` 正确将 ShowUI 和 Browser Use 标为 skipped；`vision.capture_desktop`、`computer.left_click`、closed-loop 均返回 `execute=false / executed=false` 的安全 dry-run。
- 包内英文 TTS WAV：`tmp/logs/package-tts-english-8777-20260823.wav`，119734 字节，`data` chunk 119688 字节；中文 TTS 返回 HTTP 500 和具体空采样原因。
- 包内运行结果：`tmp/logs/package-selfcheck-8777-20260823.json`。

## 后续

修复提交到独立分支并创建 PR。当前 Program Files 中仍运行用户刚安装的旧构建；新修复构建需在 PR 验证后重新打包，再按用户授权执行安装升级。
