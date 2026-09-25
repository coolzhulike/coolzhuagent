# 发布复测交接：P2 统一模型参数与 P4 消息、搜索、耗时、用量

> 本文件为源码契约和扩展用例附件。实际已安装 0.2.12 Release；安装后结果见[发布主报告](../../2026-09-19-release-0.2.12-change-and-test-report.md)。第1节18765/18766属于前轮历史证据；本轮可复用夹具与18775/18776启动方式见[README](README.md)。下文未重测说明仅指附件审阅范围。

核对日期：2026-09-19。本文依据当前工作区源码及本轮已交付工作记录，只读审查；本次审查没有修改产品文件、运行写 API、重新启动模型或重新执行测试。文末将此前测试证据与尚未验证的场景分开。安装包是否包含这些源码，必须由打包主报告的构建时间、哈希及安装后验收确认。

仓库根目录：`C:\Users\zhupu\Desktop\coolzhuagent`。下文路径均相对于此目录；`{sid}` 是会话 ID，`{rid}` 是聊天室 ID，二者不可互换。使用 URL 编码后的 ID。接口主体使用 `Content-Type: application/json`。

## 1. 复测环境与最小路径

优先在独立运行目录测试，避免改写用户已有会话和密钥。已有隔离样例为 `tmp/2026-09-19-e2e/`：Web Console 端口 18765、模拟模型端口 18766，`coolzhu.toml` 中 `[web].bind_addr = "127.0.0.1:18765"`。此前验收完成后服务已关闭；不要把旧 PID 或旧截图当成当前安装版证据。主程序静态资源编译期内嵌，源码变更后必须重新构建并运行对应新二进制。

已有模拟模型脚本 `tmp/2026-09-19-e2e/mock-openai.cjs` 可用于确定性基线：Node 运行后只监听 localhost，不访问外网；启动时会重写其同目录 `mock-note.txt`，追加 `mock-requests.jsonl`。支持 `POST /v1/chat/completions`、`GET /v1/models`、`GET /health`。脚本日志记录请求模型、工具名称、参数和序号，不记录鉴权或聊天正文。注意它只模拟 OpenAI 协议，不能据此声称 Anthropic UI 端到端测试通过。

控制台基础查询：`GET /api/sessions`、`GET /api/chat/rooms`，从响应取得真实 ID；不存在的会话和聊天室应返回 404。新建测试会话可以通过统一参数页，或 `POST /api/sessions`。新建会话沿用原 API 的激活行为；在参数页的“编辑会话”选择另一个已有会话，仅切换编辑对象，不激活顶部当前 Agent。

下例是模拟模型的统一参数 POST 示例，必须将 `{sid}` 替换成隔离会话：

```http
POST /api/sessions/{sid}/model-settings
Content-Type: application/json

{
  "session": {
    "name": "发布验收模型",
    "model": "mock-acceptance-2026",
    "model_type": "text",
    "reasoning_effort": "high",
    "api_key_ref": "sk-mock-local-only"
  },
  "parameters": {
    "protocol": "openai_chat_completions",
    "base_url": "http://127.0.0.1:18766/v1",
    "endpoint": "",
    "context_window": 32768,
    "max_output_tokens": 4096,
    "temperature": 0.4,
    "top_p": 0.8,
    "reasoning_mode": "effort",
    "thinking_budget": null,
    "enable_llm_tools": true,
    "computer_use_enabled": false,
    "llm_tool_exposure": "whitelist",
    "tool_allowlist": ["read_file"]
  }
}
```

聊天实际接口均是 POST：`/api/chat/send`（JSON）、`/api/chat/send/stream`（SSE）、`/api/chat/send/relay`（接力 JSON）。最小请求示例：

```json
{"session_id":"{sid}","chat_room_id":"{rid}","target_agent_ids":["{sid}"],"text":"请验证流式回复显示顺序"}
```

请求还可带 `selected_message_ids`、`attachments`；它们不是本次定向测试的必要字段。流式 `started` 返回 `turn_id` 和可选 `run_id`；`message_delta` 为 `{id,kind,delta}`；终态 `done` 带 `turn_id`、可选 `run_id`、`status`、`accepted_agent_ids`、`tasks`。终态包括 `completed`、`interrupted`、`failed`。要验证中止，使用 UI 停止按钮或现有中止 API，不用“关闭浏览器”替代显式中止验收。

## 2. P2 API 契约与字段

源码入口：`modules/gui-web/packages/web-console/src/main.rs` 中 `SessionModelLimitOverride`、`SessionModelSettingsUpdateRequest`、`validate_session_model_settings`、`api_get_session_model_settings`、`api_set_session_model_settings`、`provider_client_for_agent`；前端 `src/model_settings.js`；实际协议体由 `modules/llm-adapter/packages/llm-adapter/src/request_parameters.rs` 应用。

| 方法与路径 | 语义与响应 |
|---|---|
| `GET /api/sessions/{sid}/model-settings` | 返回 `session` 摘要、推导后的 `protocol/base_url/endpoint`、`default_context_window/default_max_output_tokens`、`effective_context_window/effective_max_output_tokens`、保存的 `parameters`。没有原始密钥字段。 |
| `POST /api/sessions/{sid}/model-settings` | 主体 `{parameters: {...}, session?: {...}}`。**parameters 必须存在且整组替换该会话的参数项，不是部分 PATCH。** 应先 GET，再修改完整 parameters。成功响应与 GET 同形。 |
| `GET /api/sessions/{sid}/model-limit` | 旧容量接口，返回 `session_id/model/context_window/max_output_tokens/default_*/overridden`。旧接口的容量结果不要等同于新接口受本地服务与预留约束后的最终输出值。 |
| `POST /api/sessions/{sid}/model-limit` | 仅更新 `context_window/max_output_tokens`，保留新协议、采样、工具等字段；缺省容量按 0 处理；超过上限会截到 4,000,000 / 1,000,000。与新接口越界返回 400 的行为不同。 |
| `PATCH /api/sessions/{sid}` | 原会话身份更新接口，仍存在；新参数页通过 model-settings 的 session 子对象调用相同更新逻辑。 |

`parameters` 字段：

| 字段 | 类型、值与边界 |
|---|---|
| `context_window` | `u32`，缺省 0；0 沿用模型默认；新接口最大 4,000,000。 |
| `max_output_tokens` | `u32`，缺省 0；0 沿用模型默认；新接口最大 1,000,000。显式非零时不能大于校验得到的上下文。 |
| `protocol` | 可空字符串枚举：`openai_chat_completions`、`anthropic_messages`；空值沿用旧 provider 推导。UI 始终保存明确协议。空字符串不是合法协议。 |
| `base_url` | 可空字符串；完整 HTTP/HTTPS URL，必须有 host，不能含 URL 用户名/密码；为空/空白回退会话 URL 或模型目录默认。显式 parameters 的地址不受旧会话 URL 256 字符截断规则限制。 |
| `endpoint` | 可空字符串；空/空白使用协议默认路径；支持相对路径、完整 HTTP/HTTPS 地址，绝对 URL 同样禁止 username/password。相对路径不做完整 URL 语义校验。 |
| `temperature` | 可空有限浮点数；OpenAI 0–2，Anthropic 0–1，包含端点。空值不主动覆盖旧请求参数。 |
| `top_p` | 可空有限浮点数；`0 < top_p <= 1`。UI 最小值为 0.01，但 API 可以接收大于 0 的更小正数。 |
| `reasoning_mode` | 可空；`auto`、`effort`、`thinking`、`budget`、`adaptive`。空值当 auto；协议组合见下一节。 |
| `thinking_budget` | 可空 `u32`；只有 Anthropic budget 模式实际发出预算。开启时至少 1024，严格小于本轮实际输出预算。 |
| `enable_llm_tools` | `bool` 或 null；true/false 显式开关，null 沿用工程设置。开启不表示越过执行权限。 |
| `computer_use_enabled` | `bool` 或 null；同上，用于电脑操作能力。 |
| `llm_tool_exposure` | null 或 `all`、`whitelist`、`dispatch-only`。`none`/空字符串不是合法枚举；禁用工具使用 enable_llm_tools=false。 |
| `tool_allowlist` | null 或字符串数组；最多 256 项，名称 trim 后非空，单项不超过 128 **UTF-8 字节**。后端校验本身不规范化/去重；UI 按换行、英文/中文逗号拆分、trim、去重，仅 whitelist 且有文本才提交列表。 |

`session` 子对象均为可选字段：`name/provider/model/model_type/avatar/base_url/endpoint/reasoning_effort/api_key_ref`。统一参数页主要提交 name、model、model_type、reasoning_effort，密钥有新内容或明确清除才提交；编辑旧会话不会主动改 provider。

- `name` 非空时 trim 并截取前 32 Unicode 字符；空白不会清空已有名称。
- `model` trim 后最长 1024 Unicode 字符；空白经旧 normalize 逻辑回退 `glm-4.6`；UI required 阻止空值，但直接 API 不是同样的错误行为。超过 1024 的直接 API 输入会截断。不要把 64 字符视为当前上限。
- `model_type` 合法值：`text/vision/audio/video/image/embedding/multimodal`；非法值在更新中被忽略。此字段是用途标签，不代表统一页面已实现所有音频、视频、向量协议调用；本轮新参数路由只覆盖两个聊天协议。
- `reasoning_effort` 支持 `auto/none/minimal/low/medium/high/xhigh/max`；建议测试使用规范小写。字段错误返回 400，不降级为 medium。
- `provider` 底层仍保留，最长 32 字符。会话级 base_url/endpoint 旧字段只有 custom provider 更新，其余 provider 会清空旧字段；统一页面的 parameters 独立覆盖不受此限制。
- 成功校验后先写工程配置，再更新会话；会话更新错误会尝试回滚前一个参数项。这不是单一数据库跨文件事务，不能把尚未测试的磁盘失败路径宣称为完全原子。
- 类型错误、非法 JSON、负整数等可能在 Axum 反序列化阶段直接失败，和业务校验的 400 分开记录；测试应保留真实 HTTP 状态及响应体，不预设它们都走业务错误文案。

## 3. 思考编码、地址和容量的实际效果

| 协议 / mode | 实际请求字段 |
|---|---|
| 任意 / auto 或未设置 | 沿用旧模型能力目录映射；未知模型不保证主动发送思考字段。temperature/top_p 如显式设置仍生效。 |
| OpenAI / effort | effort != auto 时发送 `reasoning_effort: "所选值"`，含 none；auto 不发送。 |
| OpenAI / thinking | effort=none 发 `thinking:{type:"disabled"}`；其它非 auto 发 enabled；auto 不发送。UI 仅显示 auto/none/high。 |
| Anthropic / budget | effort=none 发 disabled；否则发 `thinking:{type:"enabled",budget_tokens:N}`。UI 开启值为 high。 |
| Anthropic / adaptive | effort=none 发 disabled；low/medium/high/max 发 `thinking:{type:"adaptive"}` 和 `output_config:{effort:"..."}`；auto 不发送显式思考字段。 |

显式非 auto 模式会先移除旧目录推导的 reasoning_effort、thinking、output_config，避免同时发两种编码。显式模式可以传给未知模型，但服务端是否接受特定层级由真实模型决定；参数确实到达服务端和模型确实支持它是两项测试。

Anthropic 不接受 effort/thinking 模式；OpenAI 不接受 budget/adaptive 模式。Anthropic temperature 与 top_p 不能同时设置；budget/adaptive 开启思考时采样必须留空；auto 被目录识别为 Anthropic adaptive 思考时也限制采样。adaptive 仅允许 auto/none/low/medium/high/max。预算关闭（none）不要求 1024；开启预算必须 `< request_max_tokens_for_limit(...)`。

`EndpointResolver` 支持默认协议路径、相对显式路径和完整显式地址。Base=`http://127.0.0.1:18766/v1`、空 Endpoint → `/v1/chat/completions` 或 `/v1/messages`。完整 Endpoint 原样使用而非重复追加 `/chat/completions`；相对路径拼到 Base 后面，开头 `/` 不表示 URL origin 根路径覆盖。对 base 已含 `/v1` 与 endpoint `v1/...` 有重复去除处理。含 query、尾部斜线、混合大小写协议的 URL 建议作为边界实测，不能单凭保存 200 判断最终路由正确。

最终请求输出受模型默认/会话覆盖、本地运行时上限、上下文预留共同影响。`request_max_tokens_for_limit` 不超过模型输出上限且不超过约上下文一半（极小窗口最低 1）；因此填写 max_output=8192、context=8192 并不代表实际请求 max_tokens=8192。本地上限适用 custom 且命中配置的本地模型端口，不是所有 localhost 服务。读取新 GET 的 `effective_*`，并检查模拟服务记录的真实 max_tokens。预算校验用模型/会话容量计算，而后续本地运行时可能更小：必须追加“本地端口上限小于已填预算”的真实发包测试，不能只看保存校验。

## 4. 迁移、密钥与编辑器行为

- 原会话身份仍在 SessionStore / SQLite，原 `coolzhu.toml` 的 `[session_model_limits.<sid>]` 扩展可选字段；只有 context_window/max_output_tokens 的旧配置能继续反序列化，新字段默认 None。没有删除底层 provider 适配和目录。
- parameters 协议未指定时继续旧路径；明确指定时通过 `ProviderClient::from_session_endpoint(...).with_request_parameters(...)` 进入原生协议；流式和非流式共享该客户端参数入口。
- 密钥输入框每次 load/save 成功后清空，不回填原值。UI 留空保存时**省略 session.api_key_ref**，保留已有密钥；勾选清除则发送空串；同时输入新 key 又勾清除时清除优先。
- 直接 API 与 UI 不同：`api_key_ref` 缺失或 null 保留；显式 `""` 清除；非空 trim 后截取前 4096 Unicode 字符。
- 后端已有字段支持字面密钥或文件引用：`sk-`/`sk_`/`dashscope-` 前缀，以及满足现有长度/路径启发式的字符串当密钥；短的任意字符串可能被当成文件路径。测试假密钥请使用 sk- 前缀。
- session GET 摘要只有 `api_key_status`，通常显示状态/尾四位或引用名，没有完整 api_key_ref。旧引用语义和本地持久化方式保留，并非新引入加密密钥保险库；不要声称数据库中完全不含凭据。
- 显式协议客户端若会话 key 解析为空，会尝试旧 provider 目录列出的环境变量。因此“清除该会话保存的密钥”不等于清除系统环境变量，清除后仍可能成功鉴权。测试需隔离已知环境变量。
- 草稿在 input 事件即时标记 dirty；普通会话列表刷新不能覆盖正在编辑/聚焦的表单，也不会把编辑另一会话拉回当前 Agent。切工程销毁旧编辑器、防同名 ID 跨工程沿用草稿。读取有序号失效保护；未成功加载时保存禁用。
- 保存 await onSaved；后续 UI 刷新失败提示“配置已保存，但界面刷新失败”，不是假报保存失败。新会话是先 POST 创建会话再 POST 参数；第二步失败可能留下已创建会话，尚无整体事务回滚承诺。主动切换“编辑会话”会载入新对象，并无未保存确认弹窗承诺。

## 5. P4 搜索、索引与 around 契约

源码：`src/chat_insights.rs` 与 `src/chat_experience.js`，主消息渲染过滤在 `src/app.js` 的 `shouldRenderCompletedMessage`。

| GET 路径 | 行为 |
|---|---|
| `/api/chat/rooms/{rid}/search?q=词&offset=0&limit=50` | 在当前存储载入的房间全部可见历史中检索，不限当前 DOM 或当前消息分页。trim 查询并大小写不敏感地匹配正文 content 或 author 子串；不是正则、SQL LIKE、全文语义检索。空 q 返回全部可见消息。 |
| `/api/chat/rooms/{rid}/search?around={message_id}` | around 优先于 q/offset/limit；定位已存在的可见消息，返回前后各最多 20 条，最多共 41 条，按历史顺序返回。 |
| `/api/chat/rooms/{rid}/insights` | 同时返回 indices、timings、usage，定义见下一节。 |
| `/api/chat/rooms/{rid}/messages?limit=...&before=...` | 旧原始分页仍返回内部消息，UI 统一过滤；不要因原始 API 含工具/思考就判新搜索过滤失败。limit 被约束到 1–200；before 是原始 message ID。 |

search 默认 limit=50，约束到 1–100；limit=0 取 1，>100 取 100。offset 为非负整数；负值/非数字是解析错误，超出总数返回空 items。结果从最新到最旧：

```json
{
  "items": [{"id":"message-id","index":2,"author":"助手","role":"assistant","created_at":1234567890000,"snippet":"正文摘录"}],
  "total": 1,
  "message_count": 2,
  "has_more": false
}
```

`total` 是命中数，`message_count` 是全部可见数；`index` 是消息在全部可见历史中的 **1 起始位次**，不是命中排名/数据库 ID/聊天轮数。新消息追加后已有位次保持；删除/裁剪旧消息后位次可重排，所以“稳定索引”不等于永久不变的编号。检索依据已加载的持久房间历史，受现有历史保留/容量策略约束，不恢复已删除数据。

kind 先 trim、转 ASCII 小写、下划线转连字符，然后排除 `reasoning/tool-call/tool-summary/tool-result/computer-use/vision-computer-use/goal-phase`；role trim 后不分大小写等于 tool 也排除。其余消息可见，不仅限 user/assistant-reply。snippet 去除旧 context usage 尾注后取紧凑 200 字符摘要；**匹配本身使用原始 content**，所以被摘要隐藏的旧尾注也可能命中，这不是已实现的“只搜最终可见文本”。

around 成功：`{messages:[原始消息对象...],found:true,position:位次,total:可见数,has_older:boolean,has_newer:boolean}`。原始消息字段含 id/author/role/target/content/kind/attachments/created_at，时间戳是毫秒。目标已删除、属于其他房间、或属于被过滤的内部消息：HTTP 404，文案“消息已删除或不属于当前聊天室”。内部 helper 的 found:false 不会作为正常 200 暴露。

UI 搜索输入延迟 180ms，每次取 50；点结果如果目标不在 DOM，加载 around 替换当前显示，再高亮约 2.2 秒；保留“加载更早”和“返回最新”。结果页在新轮 done/error 后自动刷新。切房间/工程增加作用域版本，旧搜索/定位/用量响应不得覆盖新环境。

## 6. P4 思考、工具、耗时与 usage 定义

思考为临时区域 `chat-live-reasoning`，仅生成阶段显示，最多展示最近 6000 字符；正文增量到来后清空。空 assistant message_start 暂存，首段真正正文到来才创建气泡，避免“回复空卡在前、思考在后”。done/error/中止/切环境清理临时思考。非流式最终 reasoning 不作为完成聊天气泡渲染。历史正文、搜索/索引、TTS 均过滤内部思考/工具消息；不是删除底层原始记录。

工具活动在独立 `chat-tool-results` 状态区，按 ID 更新，最多保留 30 条摘要，单条摘要最多约 280 字符。tool-call、tool-summary、tool-result、computer-use、vision-computer-use、下划线旧别名和 role=tool 都应拦截，不渲染为普通聊天消息。工具活动条数是 UI 事件/摘要条数，不能拿来等同模型请求数或工具执行次数。

`GET /api/chat/rooms/{rid}/insights` 返回：

```json
{
  "room_id":"room-id",
  "usage":[{"session_id":"agent-id","requests":2,"input_tokens":200,"output_tokens":40,"cache_read_tokens":0,"cache_write_tokens":0}],
  "timings":{"assistant-message-id":6500},
  "indices":{"user-message-id":1,"assistant-message-id":2},
  "source":"provider_reported",
  "note":"从此版本开始累计接口已返回且适配器可解析的用量；无 usage 的请求与此前历史不计入。缓存字段单列，不重复相加；未提供或未解析的缓存明细显示 0。"
}
```

- `indices` 与 search 使用相同可见过滤，map key 是消息 ID。
- `timings` 单位毫秒，由后端 Instant 测整轮处理，包括模型、工具等，不是 tokens/s、网络 TTFT 或单工具耗时。普通/接力/流式出口将整轮耗时写给 assistant-reply/assistant-fallback 消息；多 Agent 接力中多个回复可能共享整轮总耗时，不能解释为各 Agent 独立耗时。没有已收集回复 ID 的中断/失败不保证新增时间条目。前端运行中另用 started 后 Date.now 每 200ms 刷新，因此实时秒数与持久值可能略不同。
- SQLite 新增 `chat_usage_events`：workspace_id、room_id、session_id、created_at、输入/输出/缓存 token。查询按当前 workspace_id + room_id 过滤，再按 session_id 聚合。`requests`=非零、已解析 usage 事实行数，不是所有 HTTP 尝试数/成功轮数/气泡数。工具循环会产生多次模型请求；恢复重试如返回用量也按各次请求记录。
- 输入/输出均为适配器解析的 provider usage，不拿历史长度或 context usage 尾注估算。全部四字段为 0 的请求不落事实表；无 room_id 的内部调用不记入房间统计。没有 usage 的成功回复不增加请求数；取消时已经收到的非零快照仍可计数。缓存字段单列，不再次加进输入输出，未提供/未解析显示 0，不能据此断言实际没有缓存。
- 流式快照字段分别取最大值而非相加；同一请求 finish 使用 take()，Drop 只保存尚未 finish 的快照，避免完成/取消双计。该合并语义要求适配器提供累计快照，不保证支持任意“纯增量 token 计数”私有协议。
- SQLite `chat_message_timing` 为 message_id 主键、room_id、elapsed_ms，用 INSERT OR REPLACE。两表不参与旧会话快照重写，因此刷新/重启/普通保存应保留。timings 的 SQL 只按 room_id 查询，没有 workspace_id 列，依赖既有 DB 作用域和消息 ID 唯一性；若配置共享数据库且不同工程 room ID 相同，应额外复测，不先宣称此组合已完全隔离。
- 统计是历史请求事实；消息删除、会话删除、旧消息裁剪是否同步清理这些新增表，不在当前实现的明确保证内。indices 取当前历史，timings/usage 可能仍保留历史事实；不要擅自要求它们都等于当前消息集合。

## 7. 建议正向、反向和边界测试

所有写入都用专用隔离会话/房间。修改前保存完整 GET；测试后恢复参数；不要在报告打印真实 key。对“服务端参数生效”使用记录了请求体的 mock 或服务端审计，不能仅以页面保存成功作为证据。

| 编号 | 操作 | 预期/证据 |
|---|---|---|
| M01 | GET 旧会话，保存仅 context/output 的旧 TOML 后启动 | 无反序列化失败；新字段为空、旧参数与 provider 仍可用。 |
| M02 | 参数页填写从未登记的模型 ID、超过 64 的完整本地路径，保存后重新 GET/重启 | 1024 内保留完整模型，实际请求 model 不被 64 截断；>1024 直接 API 检查截断行为。 |
| M03 | OpenAI effort=xhigh、temperature=.4、top_p=.8；分别流式/非流式 | 发包含这三字段；旧 thinking/output_config 不并存；effort=auto 时不发显式 reasoning_effort。 |
| M04 | OpenAI thinking 的 auto/none/high；Anthropic adaptive auto/none/low/high/max；budget 2048 | 分别核对上表原生 JSON；两个协议同时覆盖流式和非流式。真实服务仅选择其支持值。 |
| M05 | OpenAI+budget、Anthropic+effort、未知 protocol、tool exposure=none | 新 API 400，原配置不变。 |
| M06 | temperature=-.01/0/2/2.01；Anthropic 1/1.01；top_p=0/.0001/1/1.01 | 分别按 API 边界拒绝/接受；区分 UI min=.01 与 API >0。Anthropic temp 与 top_p 双设拒绝。 |
| M07 | 开启 Anthropic 思考且填采样；budget=1023/1024/实际输出-1/实际输出 | 非法组合400；合法边界到达请求体；output=0 默认值与小本地 runtime 容量都要覆盖。 |
| M08 | context=0、max_output=0；容量上限与上限+1；output>context | 新 API 上限+1及显式output>ctx拒绝；旧 model-limit 上限截断且不丢新字段；effective_max_output 与抓包一致。 |
| M09 | Base HTTP/HTTPS、本地无密钥、file://、URL用户名密码；Endpoint 空、相对、完整精确路径 | 非HTTP/带凭据绝对URL拒绝；检查最终路径只有一份协议后缀；不得只以 GET 回显判定。 |
| M10 | 新假key、留空保存、null、显式空串、勾清除、清除+新key、>4096字符 | UI留空保留，API空串清除，清除优先；GET不回完整key；环境变量回退另测；日志不泄漏引用原文。 |
| M11 | 允许清单 256/257 项、空名、129字节、重复名、含空格名；开关 true/false/null | 合法边界和坏值校验；UI规范化去重；实际模型可见工具和执行策略由P1联合验收。 |
| M12 | 输入首字不失焦后刷新列表；编辑非当前Agent；快速切换读取；切不同工程同名sid | 草稿不被普通刷新覆盖；编辑对象不被拉回；旧响应失效；跨工程参数重新加载，不能带旧草稿保存。 |
| M13 | 模拟保存成功后列表刷新失败；新建第二步参数保存失败；磁盘写入失败 | 区分已保存但UI刷新失败；记录新建残留会话/回滚边界；不声称未经故障注入验证的原子性。 |
| C01 | mock普通流式输入“请验证流式回复显示顺序” | 前约5秒显示临时思考；首段正文前无空助手卡；正文开始思考收起；done后只有正文；usage增加100/20、requests+1。 |
| C02 | 输入“请运行模拟工具，读取隔离验收文件” | mock发read_file后正文含READ_FILE_OK_20260919；正文无工具气泡，状态区有摘要；一轮两次模型请求，usage增加200/40、requests+2。 |
| C03 | 非流式、接力、多工具、失败、显式中止、断流；先reasoning后content与只有content | 不残留思考/空卡；多Agent耗时按整轮定义；收到的usage单记，无usage不估算；中止前只有输入usage则输出仍0。 |
| C04 | 构造>100条可见消息及隐藏reasoning/tool别名，混合中英文与%/_；按正文/author搜索 | 全历史检索，最新在前；%和_按普通字符；隐藏事件不命中/不占号；总数和可见数分清。 |
| C05 | offset=0/50/超总数；limit=0/1/100/101；负数/非数字 | 正常分页不重复；clamp正确；非法类型有明确HTTP错误；has_more与匹配数一致。 |
| C06 | around首条/中间/尾条/不存在/已删/其它房间/隐藏tool，附q及limit | 成功最多41条、目标前后各20、position正确；around覆盖其余查询；不可见目标404。 |
| C07 | 搜索结果目标不在DOM，点击定位，再加载更早、返回最新；删除较早消息 | 定位载入上下文、高亮、无横向布局破坏；删除后序号重排但按ID仍能定位未删目标。 |
| C08 | 搜索面板打开时完成新回复；请求未返时切房间/工程 | 结果自动更新；旧请求不能覆盖新作用域；旧计时、工具摘要、定位状态清空。 |
| C09 | usage多次重复累计快照、只有输入起始usage、cache-only、全0、完全无usage | 分别取max；完成后Drop不双计；已收到非零取消请求入账；全0与无usage不增requests；缓存单列。 |
| C10 | 刷新/重启/会话保存后GET insights；切房间/工程；共享DB同名rid（如实际支持） | 已有事实持久；常规作用域隔离；记录共享DB/timings隔离实际结果，不把推断写成通过。 |

推荐测试产物：每项记录“二进制绝对路径/哈希、运行目录、模型服务和协议、测试输入、HTTP状态、去密钥请求体、SSE关键事件顺序、insights前后差值、截图、是否通过”。真实跨模型测试要分别标注 API兼容/思考参数/结构化工具/usage四项，不以一条普通聊天成功代表全部通过。

## 8. 已有自动测试名与可复跑命令

以下名称在当前源码中存在。命令在仓库根目录执行；本次只审查，没有重跑。

```powershell
cargo test -p coolzhu-web-console --offline unified_model_settings_tests
cargo test -p coolzhu-web-console --offline chat_insights::tests
cargo test -p coolzhu-web-console --offline web_frontend_hides_completed_reasoning_and_routes_tools_to_status
cargo test -p coolzhu-web-console --offline api_key_diagnostics_never_log_reference_content
cargo test -p coolzhu-llm-adapter --offline request_parameters::tests
cargo test -p coolzhu-llm-adapter --offline unified_session_parameters_reach_both_native_endpoints
node tmp/analysis-chat-experience-scope-test.cjs
```

`unified_model_settings_tests` 四项：

- `unified_settings_preserve_legacy_limit_documents`
- `unified_settings_reject_invalid_sampling_and_mixed_protocols`
- `unified_settings_keep_long_local_model_paths_and_reject_non_http_connections`
- `unified_settings_respect_local_service_cap_and_smaller_user_budget`

`chat_insights::tests` 五项：

- `searches_entire_history_with_stable_indices_and_hides_internal_events`
- `tool_results_and_legacy_aliases_are_absent_from_search_and_message_indices`
- `usage_tables_survive_reinitialization`
- `stream_usage_merges_partial_cumulative_snapshots_without_double_counting`
- `stream_usage_recorder_records_completion_and_cancelled_scope_once_each`

适配器 `request_parameters::tests` 三项：

- `explicit_parameters_support_unregistered_models_and_keep_wire_separate`
- `anthropic_budget_uses_native_thinking_field`
- `unspecified_parameters_preserve_legacy_payload`

适配器 `client::tests::unified_session_parameters_reach_both_native_endpoints` 使用真实 localhost TCP 接收非流式 HTTP，覆盖未知模型名、精确 `/custom/exact-endpoint`、OpenAI Bearer 与 Anthropic x-api-key、OpenAI effort/xhigh+采样与 Anthropic budget。此测试不是浏览器交互测试，也不覆盖真实云端。

其他既有可关联测试：`strict_reasoning_validation_returns_bad_request_without_medium_fallback`、`legacy_reasoning_keeps_raw_storage_and_exposes_resolution`、`context_usage_footer_reports_model_window_percentage`、`persisted_context_usage_footer_is_removed_before_the_next_model_turn`、`context_usage_footer_prefers_remote_input_tokens_when_available`。这些只支持各自断言，不作为实际token消费回填证据。

前端 scope 脚本是临时 Node DOM 模拟测试，覆盖草稿、跨工程隔离、环境串行锁及异常解锁、切房间清理、历史刷新与旧响应隔离、头像/视觉控件迁移及头像独立 PATCH；不是完整浏览器引擎。

## 9. 已验证与未验证清单

已存在的可核验证据：

- `tmp/analysis-full-web-tests-final-20260919.txt`：8库+1helper+944主程序，合计953通过、0失败；包含上面四项统一参数、五项chat_insights、前端过滤静态契约和密钥日志测试。
- `tmp/analysis-unified-model-settings-tests-20260919.txt`：四项参数测试通过。`tmp/analysis-chat-insights-final-tests.txt`是较早四项版本，不应拿它替代最终五项证据。
- `docs/work-logs/2026-09-19-unified-model-settings.md` 记载适配器92单测+19集成通过，1项需真实密钥/网络测试忽略；包含两个原生协议localhost实际请求。当前报告未重新执行适配器测试。
- `docs/work-logs/2026-09-19-architecture-tool-model-chat-delivery.md` 记载 Edge/Playwright 在18765/18766隔离验证：先思考后正文；read_file工具只进状态区；普通100/20与工具两请求用量一致；刷新/重启保留耗时用量；READ_FILE_OK搜索定位；房间/工程切换清理；搜索面板实时更新。原始资料位于 `tmp/2026-09-19-e2e/`，包括mock日志和playwright归档。
- `docs/work-logs/2026-09-19-jade-bamboo-layout-delivery.md` 记载1440×900、1280×720、1024×600、760×600布局、约608px双列和320px单列参数栏验收；新主题未改变请求/统计语义。正式截图 `docs/design-assets/2026-09-19-jade-bamboo/`。

未验证或不能由现有证据推出：

- 原先 bonsai `127.0.0.1:8080` 未运行，不能声称已通过该真实本地模型的思考、工具循环与恢复。
- 真实云端 OpenAI/Anthropic/其它兼容模型的全面端到端矩阵、实际服务端支持的各思考层级、费用及缓存明细未完成。
- 本轮审查没有重新打包/安装/启动控制台，安装版回归由主代理另行确认。此前工作记录明确当时未覆盖 Program Files 旧安装版；其描述不是当前新安装状态的证明。
- 保存失败/磁盘满跨存储回滚、新建会话第二步失败、跨工程共享同一SQLite且同名room、极端容量和URL、所有usage私有字段等边界尚无完整实测证据。
- 既有大量前端测试是源码契约检查，不等于浏览器交互；mock成功不等于真实模型能力。以上建议用例未逐项执行，执行模型应逐条标记通过/失败/阻塞及原因。
