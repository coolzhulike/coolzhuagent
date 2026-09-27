# 已安装版 Qwen 图片与右栏预览实测（2026-09-27）

对象始终是正式安装的 0.2.17 原生窗口 `COOLZHU AGENT 控制台`，入口为 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，服务为 `127.0.0.1:8765`，实际工作区为 `C:\Users\zhupu\coolzhuagent`。所有消息都发在本轮新建的 `验收-20260927` 聊天室（`room-1790469689319`）；没有修改其他聊天室、模型配置、输入安全记录或已安装程序。

## Qwen 图片：配置、解析与实际回复

本机保存的 `qwen` 会话为 `session-1779459149988`，provider `custom`，模型 `qwen3.8-flash`，类型 `multimodal`，保存的 Base URL 为 `https://dashscope.aliyuncs.com/compatible-mode`、endpoint path 为 `v1`，存在非空密钥引用。本次只核对引用存在性，没有读取或记录密钥。安装版 `GET /api/sessions/session-1779459149988/model-settings` 返回的已解析配置为 `openai_chat_completions`、`v1/chat/completions`，`supports_multimodal=true`，`image_input_strategy=native`。这是配置解析结果，不等同于抓到了实际发往服务商的 HTTP 请求体。

自制 320×240 PNG 样本左侧为蓝色方块、右侧为红色圆形、下方数字 `7`。通过原生文件选择器上传并在发送前核对 Qwen 为唯一发送目标，见 [附件已选](evidence-installed/C-01-qwen-image-attached.jpg)、[发送前](evidence-installed/C-02-qwen-image-before-send.jpg)。只发送了一次“根据附件回答颜色、形状和数字，不调用工具”的请求。Qwen 在约 3.3 秒后正确答出三项，见 [实际回复](evidence-installed/C-03-qwen-image-answer.jpg)。上传附件的 `image/png` 对象与样本逐字节相同，SHA-256 为 `6677bae69deb529de0c94ad8fb61d13679469bebf4e5037f2f31413d71ad29da`，大小 1673 字节；持久化 user message `msg-1790471397046-user` 带该附件，assistant message `msg-1790471397230-0` 为正确答复。用量记录为 `chat/completed`、`dispatched=1`、输入 1765 tokens、输出 107 tokens。脱敏记录在被忽略的 `tmp/2026-09-27-installed-acceptance/qwen-image-live-sanitized.json`。

因此这次安装版真实回合证明了“保存附件 → 解析为多模态能力 → 模型看图并正确回复”的端到端可见行为。**没有捕获适配器实际 wire 请求的脱敏报文**，无法逐字段断言服务商收到的 URL、content part 类型和数量；源码中的 native 路径和正确回复不能代替 wire 证据。本轮也没有更改百炼地址、代理或密钥来抓包。

## 右栏实际画面

同一聊天室里，用原生窗口可见控件逐项打开：

| 对象 | 实测结论与证据 |
| --- | --- |
| 图片 | 点击聊天里的图片按钮后，右栏显示原图、标签和缩放控制，见 [D-01](evidence-installed/D-01-image-right-rail.jpg)。 |
| 文本文件 | 用只读 UTF-8 `预览验收.txt` 附件测试；右栏显示行号、中文和 `𠮷😀`，并提供当前页查找、复制，见 [D-03](evidence-installed/D-03-text-right-rail.jpg)。 |
| 视频 | 用 6 秒 `预览验收.mp4` 附件测试；点击播放器后真实画面推进至 `0:05 / 0:06`，见 [D-04](evidence-installed/D-04-video-right-rail.jpg)。UIA 辅助树期间仍把视频组标为“无法播放媒体”，与实际播放画面不符，应单独核查辅助状态，不应据它否定已播放的画面。 |
| 本地 URL | 聊天消息中的 `http://127.0.0.1:8765/` 在右栏浏览器内嵌打开，见 [D-05](evidence-installed/D-05-url-right-rail.jpg)。 |
| 外部 URL | 在右栏地址栏输入公开、无登录的 `https://example.com` 后，右栏出现 `Example Domain` 页面；见 [D-06](evidence-installed/D-06-external-url-bridge.jpg)。本次为 Tauri 原生右栏子 WebView：UIA 中外站是独立 `窗格 Example Domain`、`文档 Value: https://example.com/`，主右栏占位 iframe 仍为 `about:blank`，右栏状态显示页面标题而非 iframe 分支的“已在主面板内嵌”。只验证了打开和显示，没有操作外站内容或独立窗口。 |

预览用文本、视频上传前见 [D-02](evidence-installed/D-02-file-video-before-send.jpg)。该消息只要求 Qwen 回复“收到”；Qwen 正常回复，没有调用工具。外站宿主的脱敏 UIA 核对记在被忽略的 `tmp/2026-09-27-installed-acceptance/external-url-host-sanitized.json`。右栏“浏览器桥接诊断”只读连接检查显示 `connected=false`、`setup_required=true`；能力探测显示 `extension_unavailable`，即**另一个**浏览器扩展/native host 桥未连接，这不影响上述 Tauri 子 WebView 的浏览结果。没有点击“安全预演”或“独立窗口”，也没有验证浏览器扩展桥接可操作页面。
