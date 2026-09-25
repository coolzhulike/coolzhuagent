# GUI Web Console 对外接口说明

## 模块职责

`gui-web` 是 COOLZHU AGENT 的 Web 控制台原型，负责主界面、模块状态展示、测试实验室和本地 HTTP API。长期目标是 UI 只做 API client，业务能力继续抽到 core、vision、computer-use、tooling。

## 对外 package 与命令

- `coolzhu-web-console`

## 当前 HTTP API

- `GET /api/state`
- `GET /api/web/cards`
- `GET /api/workspace`
- `POST /api/workspace`
- `GET /api/agents`
- `GET /api/sessions`
- `POST /api/sessions`
- `GET /api/sessions/{session_id}`
- `PUT /api/sessions/{session_id}`
- `DELETE /api/sessions/{session_id}`
- `POST /api/sessions/{session_id}/activate`
- `GET /api/sessions/{session_id}/messages`
- `GET /api/sessions/{session_id}/beads`
- `POST /api/sessions/{session_id}/beads`
- `GET /api/sessions/{session_id}/beads/query`
- `GET /api/sessions/{session_id}/beads/summary`
- `GET /api/sessions/{session_id}/beads/prompt`
- `PUT /api/sessions/{session_id}/beads/{bead_id}`
- `DELETE /api/sessions/{session_id}/beads/{bead_id}`
- `GET /api/chat/rooms`
- `POST /api/chat/rooms`
- `POST /api/chat/rooms/{room_id}/activate`
- `GET /api/chat/rooms/{room_id}/messages`
- `GET /api/chat/rooms/{room_id}/search?q=&offset=0&limit=50`：全历史正文/作者搜索与稳定消息序号，排除思考及内部工具事件；`around={message_id}` 返回定位点前后各 20 条可见消息。
- `GET /api/chat/rooms/{room_id}/insights`：当前工程聊天室的真实接口用量（按模型会话汇总）、消息索引和逐轮耗时。历史未记录用量不估算补齐；流式累计快照每次请求只计一次，缓存独立列出。
- `GET /api/chat/rooms/{room_id}/attachments`
- `GET /api/attachments/index`
- `POST /api/attachments/upload`
- `GET /api/attachments/files/{file_name}`
- `POST /api/chat/send`
- `POST /api/chat/send/stream`
- `GET /api/agents/{agent_id}/diagnostics`
- `GET /api/diagnostics/health`
- `GET /api/audio/status`
- `POST /api/audio/stt/start`
- `POST /api/audio/stt/data`
- `POST /api/audio/stt/stop`
- `POST /api/audio/tts/speak`
- `GET /api/audio/voice-monitor/status`
- `POST /api/audio/voice-monitor/toggle`
- `GET /api/tools/catalog`
- `GET /api/tools/{tool_id}`
- `POST /api/tools/{tool_id}/dry-run`
- `POST /api/tools/dispatch`
- `POST /api/vision/describe-screen`
- `POST /api/vision/find-target`
- `POST /api/computer-use/action-plan`
- `GET /api/external-vision/capabilities`
- `POST /api/capture`
- `GET /api/capture/latest`
- `GET /api/capture/latest/image`
- `GET /api/stability/matrix`
- `POST /api/stability/run`
- `POST /api/computer-use/profile`
- `POST /api/computer-use/closed-loop`
- `POST /api/computer-use/safe-click-test`
- `POST /api/computer-use/safe-context-menu`
- `POST /api/computer-use/safe-drag-select`

## 新增接口契约

- `GET /api/web/cards` 返回 Web-GUI 主卡片的 API 接入审计结果，包含每张卡片的 endpoint 清单、loading/error/empty 状态覆盖位、`ready/gap` 状态和缺口说明。该接口只读，不触发真实输入。
- `GET /api/state` 和 `GET /api/workspace` 均返回 `workspace_id`，格式为 `ws-<16位hex>`。该 ID 由 canonical workspace 路径归一化后生成，用作后续会话、记忆、附件和工具权限隔离的稳定边界键。
- `GET /api/agents` 返回会话即 Agent 的注册表。自定义会话 Agent 会出现在发送下拉框；系统工具执行 Agent 和系统视觉 Agent 只作为内部执行者返回，`selectable=false`。
- `POST /api/chat/send` 接收 `target_agent_ids`、`text`、`selected_message_ids`、`attachments`，返回聊天室消息、任务队列和系统 Agent 调用状态。当前是最小闭环，真实模型回复后续由 `llm-adapter` 接管。
- `POST /api/attachments/upload` 接收 multipart 表单：`file` 为二进制文件，`name`、`kind`、`mime_type`、`source`、`room_id` 为可选元数据。文件保存到 `.coolzhu/attachments`，单文件上限 32 MiB，返回 `attachment` DTO、`preview`、持久化 `file_name` 和字节数。
- `GET /api/attachments/files/{file_name}` 仅服务上传接口生成的安全文件名，返回持久化附件内容与按扩展名推断的 Content-Type；消息发送时前端使用该稳定 URL 写入附件索引。
- `GET /api/diagnostics/health` 返回健康摘要、检查项、路径、Agent provider 诊断和修复建议。`suggestions` 会覆盖 LLM API key/provider/base_url、Web bind 地址备用端口、WebView2 Runtime、桌宠可执行文件等可操作提示，前端总览卡片只展示紧凑摘要。
- `GET /api/external-vision/capabilities` 仅返回摄像头视觉识别与投屏视觉识别的预留能力，不打开设备。
- `POST /api/computer-use/profile` 执行视觉键鼠链路耗时分析，默认 `execute=false`，返回截图、锚点映射、输入注入、后置截图、画面 diff 的分阶段毫秒耗时。

## 接口变更审查点

### 会话图片输入策略（2026-09-19）

- `GET/POST /api/sessions/{session_id}/model-settings` 的 `parameters.supports_multimodal` 是可空布尔值：`true` 按所选协议原生传图，`false` 先交系统明确选定的默认视觉会话转述，再将文字交给目标会话；省略或 `null` 沿用原有模型类型/能力判定，不改写 `model_type`。
- 返回 `effective_supports_multimodal` 和 `image_input_strategy`（`native` / `vision-description`），便于前端显示实际策略。默认视觉身份仅取 `active_vision_session_id`，不按名称猜测；视觉调用复用该会话现有协议、Endpoint、采样与凭证解析，且不暴露工具。
- 普通、流式、接力聊天逐个目标会话应用该策略。视觉 Agent 和目标会话的真实接口用量分别计入当前聊天室；未发生的调用不计数。
- 图片缺失、默认视觉未配置/设为纯文本、视觉空答或工具调用答、图片/视觉描述被上下文预算裁掉，都明确失败，不以模拟图片结论补齐，也不自动转去桌面操作。视觉描述是附件资料，不参与用户工具意图授权。
- 输入图片必须引用上传接口的受控本地地址，当前接受 PNG/JPEG/GIF/WebP 文件签名；每轮最多 8 张，单张最多 20 MiB、总计最多 32 MiB。目录外文件、不可读附件与不支持格式在派发前返回请求错误。

### 会话工具策略与恢复（2026-09-19）

- `tool.dev_open_permissions` 只决定执行授权。调试构建缺省为完全访问，发布构建缺省关闭；配置中的显式值优先。它不会覆盖模型工具开关或工具暴露范围。
- 会话模型参数支持 `enable_llm_tools`、`llm_tool_exposure`（`all` / `whitelist` / `dispatch-only`）、`computer_use_enabled`、`tool_allowlist` 覆盖。聊天室关闭能力仍有优先权，执行入口也核对同一策略。
- `dispatch-only` 只暴露语义调度；白名单统一过滤 Computer Use，普通文件或纯内容生成任务不获得 UI 工具。小上下文会话保留文件、搜索和命令工具，不能只留下 Computer Use。
- UI 工具失败后的再次 UI 请求、正文伪工具调用、工具关闭后的调用请求和工具轮数上限，触发至多一次无工具回答恢复。原始请求及已执行结果保留，未执行调用不会重放；恢复仍失败时明确说明原任务未完成。
- 真实模型回复不再触发旧的会话外 UI/语义补执行旁路。正文中的 `<tool_call>` 不作为授权或可执行协议。

- API 字段变化必须更新前端 `src/app.js` 和根目录集成测试。
- 真实输入 API 必须保持 `execute=false` 默认值。
- 视觉确认类测试必须进入 `tests/manual-visual-confirmation.md`。
- 会话/Agent 接口不得向前端返回明文 API Key，只返回配置状态或 Key 引用名。
- 系统工具执行 Agent 和系统视觉 Agent 不进入普通用户可选 Agent 下拉框，避免误删或误配置。

## 独立验证

```powershell
cargo check -p coolzhu-web-console --offline
cargo test -p coolzhu-web-console --offline
cargo run -p coolzhu-web-console -- --open
```
