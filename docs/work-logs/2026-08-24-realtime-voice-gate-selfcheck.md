# 2026-08-24 流式语音会话门槛自检

## 验证范围

检查 realtime session 的 ASR → 模型增量 → TTS、AEC/打断和半双工降级状态是否与实际能力一致；本机没有 DashScope STT 凭据、realtime model adapter 和 streaming TTS endpoint，因此不发送外部模型请求。

## 结果

- `/api/audio/status`：`stt_available=false`、`tts_available=true`，TTS 模型路径为空，状态没有伪报本地 STT 可用。
- 请求 `POST /api/realtime/session/start`，参数 `requested_mode=full_streaming`、`start_vision=false`、`start_audio=false`：返回 `active_mode=half_duplex_guarded`、`full_streaming_ready=false`，并给出明确 `mode_downgrade_reason`。
- 四个 full-streaming gate 均保持未就绪：provider-native partial ASR、far-end reference AEC、realtime model adapter、streaming TTS output。
- 任务链 UI 显示 `0/4 full-streaming gates ready` 与 `mode=half_duplex_guarded`；停止会话后确认运行状态回到 false。
- 由于没有 STT 凭据与 provider adapter，本轮未伪造 PCM 或云端流式 token，按能力门槛执行安全降级。

## 本地验证

- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：835 passed，0 failed。
- `cargo build -p coolzhu-web-console --offline`：通过。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- 隔离实例 `127.0.0.1:8801` 启停成功，停止后端口与进程均已清理。

## 证据

- API 门槛与启停结果：`tmp/realtime-streaming-gate-evidence-round11.json`
- 任务链界面（0/4 gate、半双工降级）：`tmp/realtime-streaming-gates-ui-evidence-round11.png`
- 聊天室/语音入口状态：`tmp/realtime-voice-ui-evidence-round11.png`

本轮未发现需要新增 issue 的行为；现有实现已按缺失能力降级并保留原因、门槛和风险等级。
