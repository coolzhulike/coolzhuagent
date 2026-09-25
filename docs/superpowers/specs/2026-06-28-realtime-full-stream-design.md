# Realtime Full Stream 完整闭环设计

## 目标

把当前名义上的 `full_streaming` 从“具备四个独立 readiness gate、但实际按轮停止录音并等待完整回复后朗读”的半双工流程，补齐为同一语音 turn 内可验证的连续流水线：

```text
持续麦克风采集
  -> partial/final ASR
  -> 单次用户 turn 提交
  -> 模型 message_delta
  -> 增量短句切分
  -> 流式 TTS chunk
  -> 浏览器边收边播
```

## 已确认的现状

- 浏览器已经具备 `MediaRecorder`、浏览器 partial ASR、后端最终 ASR、SSE 模型流和 TTS chunk 播放队列。
- 后端已经维护 `provider_native_partial_asr`、`far_end_reference_aec`、`realtime_model_adapter`、`streaming_tts_output` 四个 gate。
- 当前 `sttFinishDictation()` 在提交实时语音前调用 `stopAudioRealtimeState()`；监听会停止。
- 当前自动 TTS 只在 `message_done` 调用 `maybeAutoSpeakRealtimeReply()`；模型生成期间不会开始朗读。
- 当前探活和 gate 能分别证明组件工作，但不能证明四个组件属于同一 turn 的真实 Full Stream 闭环。
- 现有 `realtime_real_speech_e2e.js` 使用 TTS 合成音频反向喂给 STT，并没有覆盖真人麦克风采集，因此不能作为真实麦克风闭环完成证据。

## 白龙马调研结论与取舍

白龙马当前实现提供了可借鉴的音频数据面：AudioWorklet 采集 16 kHz PCM、WebSocket 云端 ASR、断线音频缓冲、停滞看门狗、两阶段 duck 打断、流式 TTS 适配和输出设备路由。Coolzhu 保留现有会话代际、能力门控、显式降级和审计机制，只吸收这些已经形成清晰边界的能力。

纳入现有架构：

- AudioWorklet PCM 采集，MediaRecorder 仅作兼容降级；
- 云端流式 STT 适配器的 `partial/final/error/capabilities` 契约；
- 打断预缓冲、断线重连缓冲、转写停滞看门狗和采集诊断；
- 两阶段 `duck -> 语音/噪声判定 -> 打断或恢复`；
- 流式 TTS 服务商能力声明及浏览器输出设备选择；
- 设备变化、旧播放器回调和过时代际音频的保护。

明确不照搬：

- 不使用固定 2048 样本、约 128 ms 的 PCM 大块；Coolzhu 使用 20-40 ms 音频帧降低首字延迟；
- 不硬编码本机 WebSocket 端口和云服务商；端点与服务商进入集中配置；
- 音量阈值和回声基线只作为打断抑制，不能标记为真实 far-end reference AEC；
- 不沿用白龙马的凭据保存和宽泛吞异常方式；凭据不得进入日志、测试证据或安装包。

## 核心状态模型

前端增加单一 `RealtimeTurnController` 状态，字段至少包括：

- `turnId`：每次最终 ASR 产生的新 turn 标识。
- `generationId`：当前模型/TTS 输出代次；barge-in 后立即递增，使旧事件失效。
- `messageId`：当前 assistant 流消息。
- `committedText`：已经提交 TTS 的文本。
- `pendingText`：尚未形成稳定短句的 delta。
- `ttsRequests`：按顺序运行的增量 TTS 请求。
- `modelAbortController`：当前模型流取消句柄。
- `listening`、`reasoning`、`speaking`、`interrupted`：可观察状态。

任何异步回调在修改 UI 或播放前都必须校验 `turnId + generationId`，防止 barge-in 后旧音频继续播放。

## 数据流

### 1. 输入侧

- 实时会话启动后显式选择物理输入设备并保持采集。当前测试机优先选择 `麦克风阵列 (Realtek(R) Audio)`，Steam 与 Virtual Desktop 虚拟麦克风不得作为真实麦克风验收设备。
- 首选 AudioWorklet 把单声道音频重采样为 16 kHz Int16 PCM，按 20-40 ms 帧发送到本机 WebSocket；AudioWorklet 不可用时才降级到 MediaRecorder，并把降级写入状态与报告。
- 前端 `VoiceCaptureTransport` 为每帧附加 session、sequence 和 timestamp。后端 `StreamingSttAdapter` 只暴露 `start/push/finish/cancel/capabilities`，具体阿里云、腾讯云、讯飞或火山引擎实现不泄漏到会话层。
- 允许云端流式 STT 为主通道；服务不可用或凭据无效时降级到现有本地最终转写，并明确进入非实时模式。
- 浏览器 SpeechRecognition 只能用于字幕或兼容降级，不能作为 `provider_native_partial_asr` gate 的完成证据。
- 保留约 1.5 秒打断预缓冲；云端连接中断时最多保留 8 秒尚未确认的 PCM，并按 sequence 去重重放。超过上限丢弃最旧帧并报告数据损失。
- 采集看门狗监控块速率、字节率、最大帧间隔、最近 ASR 入站时间和重连次数；持续有声但 3.5 秒无转写事件时重建 STT 会话。
- 最终 ASR 只提交一次用户消息。必须使用去重键避免浏览器 final 与后端 final 同时触发两个模型请求。
- Full Stream 模式不再为了发送消息而调用完整的音频 stop；只旋转当前录音分段或暂停提交，底层采集保持工作。

### 2. 模型侧

- `message_start` 绑定当前 assistant `messageId`。
- 每个 `message_delta` 附着到当前 turn，同时进入短句切分器。
- 切分优先使用中文/英文句末标点；无标点时使用受控长度和最大等待时间，防止首段语音长期不开始。
- `message_done` 只冲刷剩余文本和结束 turn，不再重复朗读完整消息。

### 3. TTS 侧

- 每个已经承诺的短句通过现有 `/api/audio/tts/stream` 提交，后端继续把真实音频 chunk 发布到 realtime SSE。
- 前端 TTS 队列按 turn 和 segment 排序，边收到 chunk 边播放。
- TTS provider 必须声明是否支持真流式、编码格式和取消能力；非流式 provider 可以工作，但不能满足 `streaming_tts_output` gate。
- 播放前通过 `HTMLMediaElement.setSinkId()` 选择真实输出设备；使用 Web Audio 处理链时同时使用 `AudioContext.setSinkId()`。设备拔出或 `devicechange` 后安全回落到系统默认扬声器并给出可见提示。
- 同一短句失败时允许回落到现有分段 `/api/audio/tts/speak`；回落必须在状态和报告中可见，且不能把 gate 标记为完整流式成功。

### 4. Barge-in

TTS 开始后的前 600 ms 作为抑制器预热窗口。之后连续高于动态回声基线的输入先触发 duck；只有持续语音达到阈值才确认打断，冲击噪声或短促回声必须恢复原音量。确认打断后统一执行：

- 中止当前模型 fetch。
- 递增 `generationId`。
- 清空尚未提交和正在等待的 TTS 请求。
- 停止当前音频和 chunk 队列。
- 丢弃旧 turn 的后续 SSE 事件。
- 保持或恢复麦克风监听，创建新的用户 turn。

如果打断后 3.5 秒内始终没有 partial/final ASR，则视为误触发并恢复被暂停的 TTS。该策略只降低误打断概率；只有后端收到当前播放音频的真实 far-end reference 并完成相关性验证，才能把 AEC gate 标记为 ready。

## Readiness 语义

`full_streaming_ready` 不能只依赖历史全局布尔值。完整运行证据必须满足：

- 四个 gate 都来自当前 realtime session。
- 模型 delta 与 TTS chunk 至少一次通过同一个 `turn_id` 关联。
- TTS chunk 确实进入播放队列；仅探针或仅合成成功不能冒充闭环。
- AEC 参考来自当前播放音频，barge-in 后旧参考被清理。
- provider-native ASR 证据必须包含当前会话真实 PCM 上行及对应 partial/final 入站，探针、浏览器 SpeechRecognition 或合成音频均不能满足 gate。
- 输出设备 gate 必须记录目标 sink、首个可播放 chunk、`playing` 事件和当前 generation；仅 TTS HTTP 成功不算完成。

配置或硬件不足时，系统必须显式降级到 `half_duplex_guarded`，展示具体原因，并继续提供现有可用流程。

## 错误处理

- 麦克风权限、设备缺失、ASR/TTS 上游不可用和自动播放策略拒绝分别显示独立错误。
- 单个增量 TTS 失败不应破坏模型文本显示；允许降级朗读或仅文本完成。
- 超时、取消和用户打断使用不同状态，便于报告真实原因。
- SSE 断线后不得重放旧 turn 音频；恢复只允许从新 turn 开始。
- 云端 ASR 重连只重放尚未确认的 PCM；重复 final、乱序 sequence 和超过缓冲窗口分别记录稳定错误码。
- 观测日志仅记录帧数量、字节数、间隔、延迟、provider、设备标签摘要和错误码，不保存原始 PCM、完整敏感转写或凭据。

## 测试与验收

代码测试保持最小、行为导向：

- delta 在 `message_done` 前触发首个 TTS 请求。
- `message_done` 不重复朗读已经提交的文本。
- final ASR 去重，单个用户 turn 只发送一次。
- barge-in 后旧 generation 的 TTS chunk 不播放。
- 缺少 streaming TTS、provider ASR 或当前 turn 证据时正确降级。

真实前端验收：

- 在 COOLZHU 前端显式选择 `麦克风阵列 (Realtek(R) Audio)`，由用户说出“你好 Coolzhu，请用一句话确认实时语音测试成功”。不得用文件上传、虚拟麦克风或 TTS 合成音频替代。
- 观察同一 turn 的 PCM 上行、云端 partial/final、模型 delta、首个 TTS chunk、真实扬声器 `playing` 事件和用户可听确认。
- 在模型朗读中途说出强打断词，确认模型请求与音频都停止，并能继续下一轮。
- 记录首个 ASR、首个模型 delta、首个 TTS chunk、首音播放和总完成时间。
- 主动断开一次云端 STT 连接，确认缓冲重放不产生重复用户 turn；模拟短促噪声，确认只 duck 而不取消模型。
- 若真实 TTS/ASR 服务或硬件不可用，必须记录为未完成，不以探针替代。

完成声明必须同时满足：真人物理麦克风输入、云端原生流式 STT、真实模型回复、流式或明确降级的 TTS、物理扬声器播放和同一 turn 证据关联。任何一段仅由 mock、探针或合成音频证明时，只能报告该组件通过，不能报告端到端闭环完成。
