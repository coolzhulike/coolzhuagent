# 第三轮只读审查：剩余入口补读

## 覆盖情况（以宿主回执为准）

- 本轮先调用 `mcp_list_tools`，之后对 coolzhu-agent 共调用 22 次：`read_file` 19 次、`grep_search` 3 次。其中一次 grep 漏设 `output_mode`，只返回了文件名。
- 开头有 6 次调用写错了工具名，宿主直接拒绝，没有执行。
- 17 个指定入口都实际读到了。每个文件读一段 80 行；有 3 个文件按定点追加：`window_target.rs` 前 55 行、`agent-server` 第 81-160 行、`model_settings.js` 第 291-380 行。
- 没有使用原生工具，没有跑测试。这是入口审查，不等于逐行审计全仓。

## 新增已证实风险

1. **ACP 会话里的原生工具没有被关掉，只靠提示词约束。** 本轮我这个 Devin 会话的工具清单里同时有 `exec`、`write`、`edit`、`read`、`webfetch` 和子 Agent 等原生工具，还有 coolzhu-agent MCP。这些我都没用，但它们确实挂着。所以"ACP 内层禁用工具"不能写在提示词里，必须由宿主强制，例如：
   - 在 ACP `session/request_permission` 一律拒绝；
   - 或在 Devin CLI 的权限配置里 deny；
   - 内层会话不挂任何 MCP。
2. **只读工具也会写工作区。** 第二次 grep 的回执提示：原文已写入 `workspace\.coolzhu\tool-results\sha256-….txt`。也就是说只读审查会在工程根目录下产生落盘文件，但这个目录在 `repository` 之外。
3. **权限闸门实际跑在 full-access 配置下。** 所有回执都是 `decision: allow-auto, reason: full-access-profile`。只读能力其实只靠暴露白名单在把关，白名单一旦配错，整个配置就是全权限。另外同样写相对路径，`read_file` 回执是 `workspace_relative: true`，`grep_search` 却是 `false`，元数据不一致。
4. **"记事本"推断可能误命中聊天窗口。** 在 [window_target.rs:23-55](file://repository/modules/vision/packages/uia-resolver/src/window_target.rs) 中，目标文本含"记事本"时会推断出不带 `.exe` 的 `notepad`。这个分支允许用标题或类名包含来匹配。如果真正的记事本没打开，标题带"notepad"的聊天窗口或浏览器页签会被唯一命中。`.exe` 分支有防误命中的测试，这个分支没有。
5. **`conhost.exe` 只凭名字就被当作可清理对象。** 在 [lib.rs:150-178](file://repository/packages/app-launcher/src/lib.rs) 中，健康检查未就绪时，只要占端口进程名是 `llama-server.exe` 或 `conhost.exe`，就判成可清理，没有核对路径或父进程。另外监听者已死时，把它的 PID 当 `parent_pid` 去清理孤儿，存在 PID 复用风险。执行端是否还有校验，本轮没读到。
6. **Devin 只读复选框的回显/保存不对称。** 见 [model_settings.js:296-298](file://repository/modules/gui-web/packages/web-console/src/model_settings.js) 与 [model_settings.js:368-373](file://repository/modules/gui-web/packages/web-console/src/model_settings.js)：
   - 白名单只是三项的子集（比如只有 `grep_search`）时，复选框也显示勾选，一保存就被悄悄扩成三项。
   - `computer_use_enabled` 为 `null` 时复选框显示未勾选，一保存就把工具关掉。
   - Devin 分支强制 `model_type: "text"`、`supports_multimodal: null`，所以目前设置页没有任何途径开启 ACP 图片。

## 入口职责表

| 入口 | 已读到的职责 | 未深入部分 |
|---|---|---|
| agent-server | axum 路由 `/sessions*` 加 SSE；会话存在内存 HashMap，广播容量 64 | 前 160 行没看到鉴权、会话数上限和绑定地址 |
| language-service | 只做 re-export；缺 Python 时测试明确失败而不是跳过 | manager/client 未读 |
| core-runtime memory | bead 的条数上限裁剪、签名归一、排序（置顶→层级→置信度→时间）、只召回 active | 152 行以外未读 |
| tool-registry | `execute_tool` 静态分发；`Agent` 直接返回错误（无宿主 runner）；bash 超时按秒换算成毫秒；读文件禁止混用行范围和字符范围 | 权限执行在上游，未读 |
| plugin-system | 安装事务、拒绝 reparse 链、hooks | 主体约 3000 行未读 |
| command-router | slash 命令表、compact、插件 | 主体未读 |
| compatibility-harness | 通过 `CLAW_CODE_UPSTREAM` 或祖先目录定位上游 TS 并提取清单 | — |
| llm-adapter client | `ProviderClient` 只有 HTTP 变体，没有 ACP；Anthropic 下 `response_format` 明确报错 | 第 80 行以后未读 |
| diagnostics | 全局日志，输出到文件、控制台和 GUI 回调 | 脱敏逻辑未读到 |
| CLI main | 默认模型 `claude-opus-4-6`，不导入 GUI 会话；OAuth 回调端口 4545 | 主体约 5400 行未读 |
| clawbot-sidecar | 默认监听 127.0.0.1:8787，地址可被环境变量覆盖 | 是否校验只允许回环地址未读到 |
| desktop-console | 冒烟测试、视觉测试、自动化测试三个入口 | — |
| vision locate | 后端优先级 UIA→OCR→本地 VLM→远程 VLM；置信度 0.55，超时 15s | — |
| windows-process-guard | 按 SID 核对对端身份，打开句柄后再核对是同一进程实例 | 管道 ACL 的创建未读 |

## ACP 规划服务：最小实施建议

1. **分发位置。** 已确认 llm-adapter 里没有 ACP 变体，所以分发放在 AgentSessionBackend 这一层，不要把 Devin 塞进 `ProviderClient`。
2. **内层会话必须无工具，由宿主强制。** 规划/验图用的内层 ACP 会话不挂任何 MCP，对所有 permission 请求一律拒绝，回复里也不带 handoff。宿主对内层只允许单轮纯文本进出，以此防止递归。原因见风险 1：现在原生工具是挂着的。
3. **复用现有的模型选择、预算、停止和台账。**
   - 透传精确模型 ID（例如 SWE-2 的确切 ID），不做别名映射。
   - 内层超时要严格短于父预算剩余时间，超时后先发 `session/cancel` 再停进程。前两轮都以"Devin CLI 查询超时"结束，说明这条路径必须设计好。
   - 请求台账记录模型 ID、耗时、取消原因。
4. **图片接线以能力协商为准。**
   - 只以 ACP `initialize` 返回的 `promptCapabilities.image` 为准，并把协商结果写进会话快照。
   - `model_settings.js` 不要再硬写 `supports_multimodal: null`，改成只读展示协商结果。
   - 能力为 false 时，验图请求直接拒绝，不降级去猜。
   - 桥接层怎么处理 initialize，本轮没读到，需要先核对。
5. **分两轮任务做。**
   - Browser 一轮。
   - Paint 一轮：必须显式传 `mspaint.exe`，避开风险 4 的标题匹配路径。
   - 暂不做有序任务序列。
6. **顺手修的小问题。** 复选框改成三项全等才判定为勾选；工具结果的落盘目录在只读模式下要么明确告知用户，要么移出工作区。

这些建议基于审查时的快照。主会话之后补上的 glob 越界校验和 ACP 专用工具指南，本轮没有核对。