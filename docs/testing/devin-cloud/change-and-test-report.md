# Devin 云会话接入：改动与测试交接（2026-09-30）

## 判定与协议依据

首版采用独立云会话插件，不增加模型 Provider。官方 v3 API 创建的是异步 Agent session，与当前 OpenAI/Anthropic 消息、流式 token、模型参数接口不兼容；用户已授权采用插件作为替代方案。

核对资料：[官方 API 概览](https://docs.devin.ai/api-reference/overview)、[v3 OpenAPI](https://docs.devin.ai/v3-openapi.yaml)、[Devin MCP](https://docs.devin.ai/work-with-devin/devin-mcp)、[CLI 模型设置](https://docs.devin.ai/cli/models)。公开创建 schema 没有通用 `model_id`、`temperature`、`reasoning_effort`。CLI 的模型/思考设置不能据此推定云 REST 有相同能力。首版不伪造这些参数；模型选择、思考程度由用户按官方产品实际提供的能力手动核验。

已在用户指定的既有 ChatGPT 审查会话讨论风险，结论支持插件方案，要求工程隔离、占位密钥不联网、避免重复创建、保留真实远端状态。留档为 [审查内容](architecture-review-dom.txt) 与 [审查画面](architecture-review.png)。页面只显示“极高”思考强度，本轮未独立核实该会话的具体模型名称。

## 功能与职责

- 入口：左侧“更多”→“插件市场”→“当前工程原生插件”→“Devin 云会话 · 配置与手动测试”。
- `src/devin_plugin/client.rs` 负责固定官方 HTTPS 地址、认证、请求/响应边界与会话身份验证；不处理本地权限或工程。
- `src/devin_plugin/store.rs` 负责工程内 SQLite 会话绑定和写操作账本。
- `src/devin_plugin/mod.rs` 负责配置加密、配置版本、工程固定、HTTP handler 和现有工具执行器适配；不创建第二套 Agent 运行器。
- `src/devin_plugin.js` 负责界面交互、32 秒等待上限、旧请求丢弃、分页和工程切换清理。密钥不存浏览器。
- 配置位于当前工程 `.coolzhu/devin-plugin.json`；首次展开生成随机占位值，Windows 下复用现有 DPAPI 加密，默认不启用。输入真实 key 后保存，留空保留原值。只显示“占位”或“已配置但未验证”，不回传密钥或密文。
- 组织 ID 必填 `org-…`。Agent 模式默认为“官方默认”，空值不发 `devin_mode`；其余模型参数、仓库、playbook、ACU 设置均省略以沿用服务端默认。Agent 模式不是模型 ID，也不是思考程度。
- API：`GET/POST /api/devin/config`，`POST /api/devin/action`；动作包含 create、send、get、messages、手动 bind。消息按官方 `has_next_page/end_cursor` 分页。
- 工具：就绪后暴露 `plugin__devin_create/send/get/messages`，复用既有聊天工具定义、权限闸门、执行监督和轨迹；工具名保留，外部插件不能冒名覆盖。手动 bind 不交给模型执行。
- POST 之前记录 `unknown`；同一操作编号和内容再次提交，未知结果拒绝重发，已确认结果返回缓存。模型编号包含工程、会话及调用 ID。取消或网络中断后需先到官网核对；刷新页面或修改配置后也不能把再次点击“创建”当作恢复操作。
- 只登记当前工程和组织的 session；切换工程立即清空未保存 key、消息草稿、分页和回执。配置版本改变时旧请求被拒绝。运行中的请求固定工程，不能投影到其它工程。
- HTTP 成功只表示请求返回；界面原样显示官方 `status/status_detail`，不改写为任务完成。保留官方 ACU、结构化结果和 PR 字段，不虚构 token 或费用。
- 不自动创建、轮询、重发、上传本地文件；不接入仅支持 stdio 的现有 MCP Host；云端 Devin 不控制本机 Paint。

## 主会话已完成检查

本轮代码与测试均由主会话完成，没有调用实现或测试子代理。没有使用假模型服务或模型夹具，也没有发送真实 Devin 或 Qwen 请求。

| 检查 | 结果及证据 | 验收边界 |
|---|---|---|
| Web console offline build | 修改后两次编译成功；132 条既有 warning | 新符号、静态内嵌资源与工具接线可编译 |
| JavaScript 语法 | app.js、devin_plugin.js 均通过 | 不替代界面验证 |
| 密钥/默认参数/绑定/账本保护 | 4 项局部保护检查通过，0 失败 | 不是模型会话验收 |
| 界面读取、保存非密钥参数 | 隔离工程 runtime 中保存 enabled/org，重启后重读保留，默认模式仍为空 | 未验证真实组织/密钥 |
| 占位阻断 | 点击创建返回“随机占位…未联网”；[最终截图](03-placeholder-final-layout.png) | 没有云 session 创建 |
| 布局 | 实测发现控件挤压后修为分行布局；[截图](02-config-layout.png) | 当前浏览器尺寸，非安装版验收 |
| 工程切换 | UI 从 runtime 切至 runtime-b，org/enabled/草稿/回执清空；[截图](04-workspace-isolation.png) | 没有真实跨工程云会话 |
| 旧工程写入 | 实际 API 拒绝错误 expected_workspace，返回“工程已切换” | 本地版本/工程约束 |
| 本地保存与前端错误 | runtime-b 配置仍为 placeholder 且 DPAPI 密文；当前页面 error log 为空 | 不宣称真实认证有效 |

界面验证使用本次编译的实际后端、端口 18765、仓库 `tmp/devin-cloud-ui/` 下两个隔离工程。只修改这些测试工程，未修改用户百炼地址、Qwen 密钥、原工程会话或微信功能。测试日志位于 tmp，不能作为长期唯一证据。初版挤压截图 `01-placeholder-ui.png` 只记录缺陷，不能作为最终布局通过依据。

## 交给用户的真实会话检查

安装新候选后，在实际使用工程展开 Devin 配置，输入本人 key 与 org，勾选启用，保留官方默认模式并保存。创建可能消耗 ACU，请只创建一次，再使用同一 session 继续检查。

| 编号 | 操作 | 期望与应截图内容 |
|---|---|---|
| D1 | 配置并保存真实 key/org；刷新 | 无明文回显，凭据状态为已配置但未验证；默认模式保持官方默认 |
| D2 | 创建简单云任务 | 返回真实 session ID/官方 URL；本地会话下拉出现；HTTP 返回不称任务完成 |
| D3 | 手动读取状态 | 与官网同 session 的真实 status/detail 对应；running 不被标成 completed |
| D4 | 给该会话发送后续消息；读取消息与下一页 | 同一 session；官方 messages/items 可读；有后续页才开放下一页 |
| D5 | 在官网实际可用处选择模型/思考程度 | 由用户核验官网是否提供云会话设置；coolzhu 不发送未定义参数；不以 mode 冒充模型 |
| D6 | 工程 A 绑定会话，切 B 后读取/发送 A ID | 不能继承 A 绑定；需要明确手动验证关联，A 的未保存内容已清空 |
| D7 | 错误组织、账户权限不足或请求超时 | 返回准确错误；不能显示成功/完成；未知写操作不自动重发，先到官网核对再手动 bind |
| D8 | 在已有 Qwen 会话明确委派 Devin 任务 | 工具调用进入现有权限闸门和轨迹；占位/未启用时不暴露；读取状态与创建完成严格区分 |

记录版本、工程、操作、session ID（分享前适当脱敏）、官网与 coolzhu 同次状态截图、错误/是否实际创建。不要截图 API key，也不要用假响应补齐未通过项目。D1–D8 当前均未判定真实会话通过，由用户按原要求手动执行。

## 旧任务边界

PR71 已合并且两项远端 CI 通过。0.2.29 用户已安装、摘要核对匹配；同次启动的生命周期日志完整，但原生截图仍只捕获交接后的主界面，开机动画视觉验收未完成。Paint 最近真实 Qwen 测试在动作前因视觉结果不是合法 JSON 失败，步骤为 0，仍未画出海绵宝宝。内置浏览器新窗口链接与 DSH 远程插件实际安装仍有未闭环项；四项整合任务没有总体通过。Devin 的新增局部验证不替代这些验收。
