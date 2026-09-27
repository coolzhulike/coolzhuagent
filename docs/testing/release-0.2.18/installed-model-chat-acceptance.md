# 0.2.18 已安装版模型与聊天实操

2026-09-27 在唯一原生控制台窗口 `919154` 实测。窗口进程为 `C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`，PID `17152`；8765 的 `GET /api/system/runtime-identity` 报告工作区 `C:\Users\zhupu\coolzhuagent`、会话 schema/supported 均为 `27`、build `9eae3aec3058 · 2026-09-27`、`computer_use_store_state=ready`。已安装 Web 可执行文件的 SHA-256 为 `581822BE7B8EAC97A9CC6AF1B64AEA5F5BA2369CE13445682FE0154D71CA1081`。以下截图均为此窗口的原始整窗截图，不能沿用到后续重新打包的版本。

## 真实 Qwen 文本、原图与预览

新建聊天室 `验收-20260927-安装版018`（`room-1790491115506`），发送前顶部和发送按钮均只显示 `qwen(qwen3.8-flash)`。已保存会话 `session-1779459149988` 解析为 `custom` / `qwen3.8-flash`、`openai_chat_completions`、`https://dashscope.aliyuncs.com/compatible-mode`、`v1/chat/completions`、`image_input_strategy=native`、支持多模态；没有读取或更改密钥、地址和模型参数。

- 文本请求收到准确回复“安装版文字通道正常”，界面显示本轮 `3.9 秒`。运行轨迹 `run-chat-84653899a436f29f3b17e47fdff34a11217c9d410f8fc55c` 为 `completed`。[01 原生截图](evidence-installed/01-qwen-text-native.jpg)
- 上传 320×240 原图 `qwen-vision-fixture.png`，SHA-256 `6677BAE69DEB529DE0C94AD8FB61D13679469BEBF4E5037F2F31413D71AD29DA`，只问左侧、右侧和下方数字。Qwen 回复“左侧：蓝色正方形；右侧：红色圆形；下方数字：7”，界面显示本轮 `3.0 秒`；运行轨迹 `run-chat-6850065eb24bac53abd31b56b5d3c9bd014ef6f6f69b502a` 为 `completed`。[02 原生截图](evidence-installed/02-qwen-image-native.jpg)
- 点击该消息附件后，右栏可见同一图片的大图及蓝方块、红圆形、数字 7，回答仍在左栏。[03 原生截图](evidence-installed/03-qwen-image-right-preview-native.jpg)

图片回答和解析设置说明本轮功能可用；未捕获实际发往百炼的脱敏请求体，故不将它们当作 wire 模态块证明。本次 Qwen 是原图直送配置，未强制视觉转述；源码隔离 HTTP 测试的“视觉转述两请求、两账单同 run”不能冒称为这次云请求的用量事实。

## 独立本地协议夹具：仅思考、无最终正文

另建聊天室 `验收-018-仅思考本地夹具`（`room-1790491573611`）和测试会话 `session-1790491524436`，Base URL 仅指本机临时端口 `127.0.0.1:62744/v1`。原生顶栏和发送按钮均确认它是唯一目标，Qwen 未参与这一轮。夹具只接收一条流式 Chat Completions 请求，顺序发五段带唯一标记 `ONLY_REASONING_018_4D41A1A0E0` 的 `reasoning_content`，然后发 `finish_reason=stop` 和 `[DONE]`；全程没有 `content` 或 final。脱敏请求形状与夹具身份存于 `tmp/2026-09-27-installed-acceptance/reasoning-only-018/`，没有记录 Authorization、消息正文或图像。

- 进行中整窗显示“正在思考”和标记的前两行临时过程，输入栏为“中止本轮”。[04 原生截图](evidence-installed/04-only-reasoning-live-native.jpg)
- 终态临时区收起，界面以“模型已结束，但未返回可显示的回答或工具调用”明确提示，没有把唯一思考标记复制成最终正文；耗时 `4.4 秒`。[05 原生截图](evidence-installed/05-only-reasoning-final-native.jpg)
- 右栏“运行轨迹 → 历史思考与原始过程记录”展开后保留全部五行，正文仍没有标记。[06 原生截图](evidence-installed/06-only-reasoning-trace-native.jpg)
- 切回 Qwen 聊天室后只见该房间的文字和图片记录，旧临时过程与本地夹具轨迹未串房间，发送目标也随房间恢复为 Qwen。[07 原生截图](evidence-installed/07-room-isolation-qwen-native.jpg)

夹具轮次 `run-chat-1058865085d82da3c24ef687f785adb80ee41c3a73d94826` 在轨迹里是 `failed`，因为测试故意不给可显示的最终回答；消息存储将提示正文记为 `assistant-error`，五行思考另记为 `reasoning`。这证明的是安装版对该受控协议边界的展示，不是 Qwen 云模型能力，也不代表实际工具调用已在此轮验收。临时夹具服务已停止，保留安装版窗口与用户工作区；没有碰 Paint 输入隔离或人工放行。

本轮没有发现新的模型/聊天出包缺陷。已知 0.2.18 终端中文显示和低高度原生网页遮挡是其它项目的独立缺口，本记录不将 0.2.18 标为最终候选。
