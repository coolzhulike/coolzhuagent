# coolzhuagent 九项问题与需求分析

- 日期：2026-10-17
- 状态：分析稿 + 已确认决策 D1-D5（未修改任何业务代码；未进入方案设计）
- 代码基线：`HEAD=8a13551b`（2026-10-11 merge）+ 工作树未提交改动；以工作树实际代码为准
- 运行基线：本机 `127.0.0.1:8765` 开发实例，工作区 `D:\coolzhuagent`，`coolzhu.toml` 的 `[paths] data_dir = "D:/tmp/.coolzhu"`
- 用途：为下一步“改什么、先改什么、按什么验收”提供问题事实与需求规格；**本文不给出实现方案**

---

## 0. 结论摘要

1. **启动慢的主因不在前端，而在 bind 之前的“全量重写 + 锁争用”。** `session_store()` 在端口 bind 之前同步执行：全量读取 sessions / rooms 的全部消息 → `ensure_chat_room` / migrate / scrub → **无条件执行一次全量 `save()`**（`DELETE` 后重插 sessions、rooms、messages、beads，每条消息再写两次 `attachment_refs`，并写一份 12.2MB pretty JSON 镜像）。`err.log` 实测 `SESSION-STORE init done ... elapsed_ms=21904`；但同一 SQL 只读实测 1878 条仅 **15.5ms**、2210 条 **15.8ms**，说明 8 秒级读耗时是锁/写/磁盘争用，不是查询复杂度。`save commit` 峰值 **93.5s**，锁日志 21,134 次等待、峰值 29.5s。
2. **库已经明显膨胀，且存在“只写不读、无老化”的表。** 2026-10-12 的 111MB 备份中，`devin_acp_config_receipts` 单表 **4258 行 / 77.2MB**（占全库约 70%），代码里只有 `INSERT`、没有任何读取与清理；`devin_acp_context_inputs` 另有 5.96MB。到 2026-10-17 当前库已 **148.8MB**（5 天 +34%），而聊天正文只有约 9MB，其余是 `runtime_run_events` 9244 行、`tool_calls` 6068 行、ACP 回执 5180 行、FTS5 影子表等旁路数据。`chat_usage_events` 仅 539 行，真正有用的统计事实反而很少。
3. **workspace 不一致失败是“身份校验冲突 + 三套最近记录互不相通”**：启动器发现 8765 上已有实例时，会比较 workspace 规范路径、session DB 绑定、构建版本、实例属主；任一 mismatch 即 `Conflict` 并拒绝复用/继续（`service_identity.rs:338-421`）。`launcher-user.json` 当前 `selection=D:\coolzhuagent`，同时保留 `legacy_selections=C:\Users\zhupu\coolzhuagent`；UI 里切换 workspace 只改内存、不写该文件；`cargo run` 又走 `%USERPROFILE%\coolzhuagent`。三套来源叠加，造成“两套 workspace + 冲突无法启动”。
4. **“工程目录不能新建和删除”是能力缺口。** 顶部“工程目录”弹层只支持输入/切换已存在路径（`chat_experience.js:96-128`，`POST /api/workspace`）；没有创建目录、删除工程、初始化工程的入口或 API。工程窗口里的“新建文件/目录/删除”只作用于当前工程内部文件（`index.html:128-154`），不是工程级操作。
5. **主聊天室“权限未知”是前端状态机的竞态。** `/api/chat/rooms/main-room/permissions` 实际返回有效数据（本机实测 `permission_profile=workspace-write`、`dev_open_permissions=true`）；但 `taskRenderFullAccessStatus` 要求 `projectWorkspaceScope` 非空且 `status.room_id === activeChatRoomId`，并且 `chatPermissionRequestSerial` 只保留最新一次请求。SSE 首轮必发 `permission-changed`（`chat_history_sync.rs:124`）会触发一次刷新；诊断刷新又复用同一 serial（`app.js:5707-5710`），可把在途的权限响应作废，`taskFullAccessStatus` 停在 `loading`，最终显示“状态未知”并禁用选择（`app.js:5549-5602`、`:9383-9399`、`:11308-11311`）。
6. **“删除删除”是 `setBusy` 的 DOM 文本追加 bug，不是 CSS。** 删除按钮的“删除”在 `<span>` 内（`index.html:644`），而 `setBusy` 只找按钮的**直接文本节点**；忙态结束时找不到，就 `append(document.createTextNode("删除"))`，于是变成“删除删除”（`app.js:19606-19628`）。新建/重命名/转交等按钮同病；截图是真实可复现的源码行为。
7. **顶部“警示/健康”并非完全没接后端，而是接错了对象。** “就绪/警示”来自 `agentHealthSnapshot()`，只看可发送 Agent 的 `api_key_status` 文本（`app.js:7460-7473`）；“灯笼”来自任务状态与待审批数（`app.js:12259-12278`）。它们都不读 `/api/diagnostics/health` 的系统健康检查，所以看起来“永远那几个状态”。权限则在旁边的独立下拉里。
8. **定时任务确实强占了一个聊天室。** `room-scheduled-tasks` 是稳定系统室（`main.rs:253-257`），启动时**无条件幂等创建**（`main.rs:38056` → `:41425-41465`），禁止重命名/删除（`:40419`、`:40475`），并出现在 `/api/chat/rooms` 列表里；用户看到的“定时任务”聊天室就是它。后台子 Agent 的运行事实在 `runtime_runs` / `runtime_run_events` / `tool_calls`，但没有一个“定时 + 后台 + 轨迹 + CU”的统一页面。
9. **轨迹/CU 不是没有数据，而是没有统一入口和老化。** 111MB 备份中 `runtime_run_events` 7301 行、`tool_calls` 2208 行、`computer_use_runs` 343 行、`computer_use_steps` 654 行、`computer_use_planner_diagnostics` 1260 行；配置里只有 `computer_use.evidence_retention_days=7`，当前没有轨迹容量/时间窗口，也没有用户可调限制。用户已定：运行类数据保留约 **12h**、不归档（D5）。
10. **设置页的问题是“信息重复 + 入口错位 + 接入模型过窄”。** 当前只有一个可见的 `unified-model-settings`；标题下副标题、下拉下 `target-note`（运行时被改写为“当前发送给…；当前编辑…”长句）、底部常驻状态文本都属于用户点名的多余文本；新建在标题行、保存在页底；Devin/OpenCode/HF 在插件市场而不在设置页；OpenRouter 只有目录提示、没有 provider 身份；图像/视频已有真实执行链，但参数硬编码、协议解析未复用；头像控件被移入 `hidden` 的旧会话区，实际不可见。
11. **统计面板“没有内容”是数据与呈现双重缺口。** 面板 DOM、渲染函数、`/insights` 接口都在，但只渲染 1-3 行文本；当前房间的 `usage` 为空，是因为 539 条 `chat_usage_events` 全属于旧 workspace `ws-23f646a969206cb4`，而当前房间运行在 `ws-8d17063fefe5c23c`；更根本的是 ACP（Devin）回合没有 token usage，HTTP 的 `LedgerObserver` 是唯一 token 写入点。没有首 token 时间，也没有按模型/按时段聚合，图表无从谈起。
12. **token 速率需要先补“分子/分母”事实。** DSH 的口径是整会话 `decodeTokens / (decodeMs/1000)`，`decodeMs = Σ(assistant/message.time − firstTokenTime)`，展示在 composer 的 activity 药丸；coolzhu 现在只有整轮墙钟 `chat_message_timing.elapsed_ms`，既没有首 token 时间，也没有 ACP token，因此只能算“整轮墙钟速率”，会系统性偏低。
13. **SKILL 与插件市场都是“有目录、没闭环”。** `/api/tools/catalog` 已能返回 3 个内置 SKILL（goal-model-reasoning / goal-session-chain / goal-tool-execution），但 SKILL 页没有 create-skill、superpowers、安装/启用/详情闭环；插件市场有 DSH 社区目录与本地插件安装链路，但详情页没有安装按钮/版本兼容/信任确认，远端安装仍不完整。
14. **终端是纯删除项，影响面可控但引用很多。** 左栏 tab（`index.html:119`）、面板（`:950-968`）、`terminal_host.rs`、`app.js` 的 terminal 初始化/快捷键/CSS/测试断言都要一起清；不涉及数据迁移。

---

## 0.1 已确认决策（2026-10-17 用户确认）

| # | 决策 | 对需求/验收的影响 |
|---|---|---|
| D1 | “启动完成”= **见到开机动画**；冷启动可接受 **≤15s** | 验收口径从“端口/窗口”改为“开机动画出现时间”；当前 launcher 在健康就绪前不 spawn Tauri，现状做不到“先见动画”，这是与现状的直接冲突，必须在需求里单列 |
| D2 | **启动使用“最近一次 workspace”**；同端口只允许一个工作区；并发冲突时**关闭已有实例并按最近 workspace 重启**；首次启动默认 `%USERPROFILE%\coolzhuagent`；**不应出现两套 workspace 需要启动选择** | 原“冲突切换到已有实例”按本轮补充修订为“关闭旧实例后重启”；需要定义“最近”的持久化来源、优雅/强制关闭边界、旧实例有运行任务时的处理 |
| D3 | 默认档 **读写 workspace 外目录需要逐次审批**；`dev_open_permissions` 在权限方案完全开发完成后**删除**；auto 待调研市面复杂度后再定是否近期纳入 | 默认档不是“完全禁止外目录”，而是“外目录逐次审批”；`dev_open_permissions` 是过渡态，不进最终模型；auto 暂不排期，仅做调研 |
| D4 | 订阅首批只做 **Devin + OpenAI**；OpenCode Go **先不做** | 设置页“订阅接入”首批厂商收敛为 2 个；OpenCode 只保留 API 接入（Zen key）定位，不做订阅登录 |
| D5 | 轨迹等运行数据按 **运行时长估算大小，保留约 12 小时**；**不需要归档** | 保留策略从“容量上限”改为“时间窗口 12h + 大小估算护栏”；删旧不归档；需明确 12h 覆盖哪些表（轨迹/CU/ACP 回执，不含聊天消息） |

> 仍待确认的问题见第 5 节；以上 5 条已从“待确认”转为“已定”，后续方案必须按此收敛。

---

## 1. 分析范围、方法与证据边界

### 1.1 范围

本次覆盖用户给出的 9 条提示，归并为 14 个问题/需求条目（见第 3 节）。只做：

- 现状调用链与数据流梳理；
- 问题根因定位（代码/日志/数据库/接口证据）；
- 需求规格化（角色、状态、边界、验收标准）；
- 市面 agent 对应能力的参考事实；
- 不确定点的反问清单。

不做：代码修改、构建、安装、UI 实施、方案选型。

### 1.2 方法

| 方法 | 用途 | 说明 |
|---|---|---|
| 静态代码阅读 | 主证据 | `main.rs`（约 3.6MB / 87k 行）、`app.js`（约 882KB）、`index.html`、`model_settings.js`、`chat_experience.js` 等分段 grep/read |
| 运行中接口只读 GET | 校验现状 | `127.0.0.1:8765` 的 `/api/workspace`、`/api/chat/rooms`、`/permissions`、`/insights`、`/trace`、`/api/sessions`、`/api/tools/catalog` |
| SQLite 只读统计 | 容量与增长 | 111MB 冷启动备份 + 552KB 日常库；`dbstat` + `COUNT(*)`，不改库 |
| DSH 产物解包 | 速率口径参考 | `C:\DSH\resources\app.asar` 按 asar header offset 提取 `dsh-client-ui-chat`、`dsh-session-stats`、`dsh-token-meter` |
| 外部资料 | 市场参考 | 官方文档/源码为主；详见 `tmp/analysis-20261017/R1-*.md`、`R2-*.md` |

### 1.3 证据边界（必须先说明）

1. **源码 ≠ 安装版。** 工作树有 197 项未提交改动；用户截图中的“删除删除”“7 个会话”与当前 `/api/chat/rooms` 返回的 4 个房间不一致。凡涉及安装版 UI 的结论，本文标注“需安装版复核”，不把源码推断当安装版事实。
2. **日志没有统一时间戳。** `err.log` 的 `[SESSION-STORE]` 行只有 `elapsed_ms`，没有墙钟时间；只能做阶段耗时对比，不能还原两次启动的绝对时刻。
3. **111MB 备份是 2026-10-12 的冷启动前快照**，不是今天 8765 实例的实时库（今天日常库 552KB、schema 40 表）。容量结论用于说明增长机制，不用于断言今天的启动耗时。
4. **没有做运行时 profiler。** 启动慢的归因基于代码路径 + 日志阶段耗时 + 库结构；CPU/磁盘/Defender 的精确占比未测。
5. **DSH 是打包产物分析。** 结论来自解包后的 client bundle 与包内 README，不是 DSH 源码仓库；但公式与字段可直接核对。

---

## 2. 横向发现（跨条目的共同原因）

### 2.1 启动关键路径：所有重活都在 bind 之前

`main()` 的顺序（`main.rs:1307-1495`）是：

1. 解析 workspace、读配置、拿 session writer 锁（`:1315-1322`）；
2. 恢复 session config journal / native recovery（`:1321-1324`）；
3. 初始化结构化诊断（`:1328-1340`）；
4. 恢复未完成 runtime runs（`:1347-1366`）；
5. 跑 input safety 启动评估（`:1371-1394`）；
6. **`session_store()` 全量加载 + 首次保存**（`:1397`）；
7. 才 `TcpListener::bind`（`:1413`）；
8. 拉起桌宠、scheduler，最后 `axum::serve`（`:1469-1479`）。

也就是说，窗口/端口可用之前，必须先完成“读全库 + 全量重写 + 写 JSON 镜像”。任何一项随历史增长而变慢，都会直接表现为“启动慢”，而不是“页面加载慢”。

### 2.2 数据层：读全量、写全量、历史只增不删

- `load_session_state_from_sqlite` 对每个 session/room 执行无 `LIMIT` 的 `SELECT ... ORDER BY created_at,id`（`main.rs:44862-44892`）。
- `save_session_state_to_sqlite_with_epoch` 先 `DELETE FROM chat_handoffs/memory_beads/chat_rooms/sessions`，再逐条重插（`main.rs:50205-50214`、`:50328-50478`）。
- 每次 `save()` 还写一份 `serde_json::to_string_pretty` 的兼容 JSON（`main.rs:38141-38151`）。
- `enforce_capacity()` 只裁剪内存工作集（`main.rs:38154-38165`），不删除 SQLite 历史；而 `chat_room_messages` 的旧行在 `DELETE FROM chat_rooms`（FK 关闭）后仍保留。因此库会持续增长，启动时又要全量读回来，形成“越用越慢”的结构。
- 备份库中 `devin_acp_config_receipts` 77.2MB、`devin_acp_context_inputs` 5.96MB，均无老化代码；`chat_usage_events` 只有 250KB。增长的主要不是“有用统计”，而是运行副产物。

### 2.3 权限：房间级存储已存在，但“默认/有效/开发开放”三层混在一起

- 存储：`chat_room_permissions(room_id, permission_profile, updated_at)`，取值 `workspace-write` / `full-access`（`main.rs:42143` 附近）。
- 读取：`/api/chat/rooms/{room_id}/permissions` 返回 `permission_profile`、`full_access`、`dev_open_permissions`、`effective_permission_profile`、`effective_full_access`（`main.rs:16879-16896`）。
- 本机实测：`main-room` 的 `permission_profile=workspace-write`，但 `dev_open_permissions=true`，`effective_permission_profile=full-access`、`effective_full_access=true`。**“默认权限”在开发配置下实际是完全访问**，这与用户描述的“默认仅 workspace、不允许 computer use”不一致，是需求必须澄清的核心。
- 执行侧：`context_permission_profile(chat_room_id)` 优先取房间权限，否则取 `active_permission_profile()`（`main.rs:28898-28909`）；但会话模型参数里还有 `computer_use_enabled`、`tool_allowlist`、`llm_tool_exposure`，两者谁覆盖谁需要按工具链逐项核对（见 3.2.5）。
- 前端：权限状态依赖 `projectWorkspaceScope` 与当前房间 id，首屏竞态导致“主聊天室未知”（3.2.4）。

### 2.4 模型接入：provider 是自由字符串，协议/类型枚举有限

- `provider` 是自由字符串；`backend_kind` 由 provider 派生（`llm_http` / `devin_acp` / `devin_cloud`）；设置页非 Devin 新建一律写 `provider="custom"`（`model_settings.js:436-439`、`:465`）。
- HTTP 协议只有 `openai_chat_completions` 与 `anthropic_messages`，另有 `devin_acp`（`model_settings.js:49-51`）。`llm-adapter` 里另有 `OpenAiImagesGenerations` / `OpenAiVideos` 枚举与端点解析，但图片/视频链路没有复用该解析器，而是自行拼 `{base}/{endpoint}`（C 报告证据）。
- `model_type` 有 7 个枚举：`text/vision/multimodal/image/audio/video/embedding`（`model_settings.js:83`）。image/video 有真实执行链（`main.rs:18646-18670`、`:32062-32165`、`:32345-32696`）；audio/embedding 没有按 `model_type` 的分派。
- OpenRouter 在代码里只有目录提示名（`model_discovery.rs:179`），没有 provider 枚举/连接器/模型白名单；这是“支持 OpenRouter 各类模型”需求的最大现状缺口。

### 2.5 前端信息架构：左栏扩展页与聊天室窗口的职责重叠

- 左栏现有入口：首页、定时任务、设置、统计信息、微信连接、更多（浏览器、终端、SKILL、插件市场）、升级更新（`index.html:109-125`）。
- 定时任务既有“左栏扩展页”（`data-window-id="schedules"`），又有一个系统聊天室；轨迹、CU、统计各自是独立窗口；用户要求把它们合并到“定时/后台任务”一个页面，并从聊天窗口移出。
- 聊天顶部工具栏当前是“搜索 / 消息索引 + 思考与运行记录”（`index.html:742`）；用户要求删后者、把位置让给 token 总量与平均速率。

---

## 3. 逐项分析

> 编号约定：`P-*` 为问题（先解释原因），`R-*` 为需求（给规格与验收）。每条都附“待确认”，不在此处做方案选择。

### 3.1 P-01 启动很慢，且 workspace 不一致时会启动失败

#### 3.1.1 现象与证据

| 证据 | 内容 |
|---|---|
| `err.log` | `[SESSION-STORE] init done: sessions=3, rooms=4, elapsed_ms=21904`；`session loaded: id=session-1791131217833, messages=1878, beads=154, elapsed_ms=8156`；`room loaded: id=room-1791131523339, messages=2210, elapsed_ms=7683`；`read done ... elapsed_ms=17167`；`initial save done: elapsed_ms=1799` |
| `err.log`（另一次） | `save sessions inserted: sessions=3, elapsed_ms=29693`；`save rooms inserted: rooms=4, elapsed_ms=46618`；`save commit done: elapsed_ms=48146`；另一处 `save commit done: elapsed_ms=93524` |
| 同一 SQL 只读实测 | 1878 条 15.5ms、2210 条 15.8ms；冷库 1236 条 9.3ms、1568 条 10.5ms → 与日志 8.1s/7.7s 相差约 500 倍，说明瓶颈是争用/写/磁盘，不是查询 |
| 锁诊断 | `%TEMP%\coolzhu-session-lock.log` 21,134 次等待，峰值 `after_wait_ms=29554`、`held_ms=92684`；峰值 `save commit` 93.5s |
| 当前库（2026-10-17） | 148,803,584 B；`session_messages=2035`、`chat_room_messages=2365`、`tool_calls=6068`、`runtime_run_events=9244`、`devin_acp_config_receipts=5180`；聊天正文约 9MB，文件 142MB |
| 111MB 备份（10-12） | `devin_acp_config_receipts` 4258 行 / 77,225,984 B；`devin_acp_context_inputs` 438 行 / 5,963,776 B；`chat_room_messages` 1723 行 / 4,509,696 B；`session_messages` 1393 行 / 4,132,864 B |
| 冷启动收据 | `tmp/installed-cold-start-132/receipt.json`：`workspace_path="\\\\?\\D:\\coolzhuagent"`、`session_db_path="D:/tmp/.coolzhu\\web-sessions.sqlite3"`、`"正式服务工作区或会话库错误"` |
| launcher 健康期限 | HEAD `config/package-launcher.json:6` `health_timeout_secs=30`；现场一次成功冷启动 spawn 08:19:12 → bind 08:19:37 → Tauri 08:19:39（27s），余量约 3s；工作树已改为 120 |
| 配置 | `coolzhu.toml:4`：`data_dir = "D:/tmp/.coolzhu"`（业务数据根在仓库外，且正斜杠导致混合分隔符） |

> **“参考库”是什么**：不是日常 552KB 空库，而是用于启动验收的**冻结数据库副本**，要覆盖真实使用的数据量与旁路表。建议口径：复制当前 `D:/tmp/.coolzhu/web-sessions.sqlite3`（148.8MB、3 sessions/4 rooms、2035 条 session 消息、2365 条 room 消息、5180 条 ACP 回执、9244 条 runtime events），放到与生产相同类型的本地磁盘；同一份副本冷启动 5 次，取 P95。备选是 111MB 的 `tmp/installed-cold-start-133/web-sessions-before.sqlite3`（10-12 快照）或合成库。**不建议用 552KB 日常库**，它测不出锁/写放大/库膨胀问题。

#### 3.1.2 启动时序（代码事实）

**launcher 侧**（`packages/app-launcher/src/lib.rs`）：读 `package-launcher.json` → 五级工作区解析（本次显式选择 > `%LOCALAPPDATA%\CoolzhuAgent\launcher-user.json` 持久选择 > 随包默认 > 旧配置 > 历史快照）→ 预检 → `decide_reuse` 身份判定 → spawn web-console（注入 `COOLZHU_RUNTIME_DIR`，清除 `COOLZHU_WEB_SESSION_DB/_STORE/_ATTACHMENT_STORE`）→ **每 500ms 轮询 `/api/diagnostics/health`，最多 `health_timeout_secs`** → 健康就绪后才 spawn Tauri（窗口）。超时则终止 web-console 并写失败自检（`lib.rs:847-878`）。所以“双击没反应”的 30s/120s 上限由这里决定。

**web-console 侧**：

| 阶段 | 代码位置 | 是否在 bind 前 | 备注 |
|---|---|---|---|
| workspace/config 解析、writer 锁 | `main.rs:1315-1322` | 是 | `active_workspace_path()` + `workspace_scope_paths_for()`；writer 锁是 `try_lock`，冲突立即失败 |
| journal / native recovery | `:1321-1324` | 是 | 失败可能直接阻断 |
| 结构化诊断 | `:1328-1340` | 是 | 文件/轮转初始化 |
| runtime run 恢复 | `:1347-1366` | 是 | 失败 fail-closed |
| input safety 启动评估 | `:1371-1394` | 是 | 失败只警告，不阻断 |
| **SessionStore 全量加载** | `:1397` → `:37755-38100` | 是 | 读全库 + ensure/migrate/scrub |
| **首次全量 save** | `:38093-38099` → `:50165-50478` | 是 | DELETE + 重插 + pretty JSON |
| bind / 监听 | `:1413` | — | 到这里窗口才可用 |
| 桌宠/scheduler/serve | `:1461-1479` | — | 之后才是业务服务 |

#### 3.1.3 根因（按影响排序）

**R1（已证实，结构性主因）：启动即无条件全量写回，写放大 O(总消息数)。**
`try_load_from_paths` 末尾固定 `store.save()`（`main.rs:38093-38099`）。一次 save 会：读全部 handoffs → `PRAGMA foreign_keys=OFF` → `BEGIN IMMEDIATE` → `DELETE FROM chat_handoffs/memory_beads/chat_rooms/sessions` → 逐条重插 2035 条 session 消息 + 2365 条 room 消息（每条消息再执行 2 条 `attachment_refs` 语句，且都是 `tx.execute` 重新 prepare）→ 6 张侧表孤儿 DELETE → `history_persistence::apply` → commit → `committed_state = state.clone()` → 12.2MB `to_string_pretty` + 整文件覆盖写。正常 1.2–2.6s，峰值 46.6 / 67.5 / 84.9 / **93.5s**。启动时 `migrate done 0–6ms`、`scrub 7–27ms`，说明通常没有业务变化，但写回仍然发生。

**R2（已证实，直接放大器）：全局 store 锁被 save 长时间占住，读也排队。**
`TrackedSessionStore` 是单 Mutex（`main.rs:37857-37891`）。`save` 持锁期间，所有 `/api/*` 与首屏请求串行等待；锁日志 21,134 次等待、312 次 ≥1s、峰值 `after_wait_ms=29,554ms`、`held_ms=92,684ms`。同一 SQL 只读 15ms，而启动日志 8.1s/7.7s，说明读耗时主要不是 SQL，而是与写/锁/检查点/磁盘争用叠加。另：W10 runtime 恢复、W11 input safety、W12 SessionStore 各开一次同一个 142MB 库并做 schema 初始化，三次串行。

**R3（已证实，容量放大器）：142MB 文件里只有约 9MB 聊天正文。**
旁路数据：`runtime_run_events` 9244、`tool_calls` 6068、`devin_acp_config_receipts` 5180、FTS5 五张影子表 1498 文档、`computer_use_*` 约 4.3k 行、`fact_log_records` 1496。5 天从 111MB → 148.8MB（+34%）。`devin_acp_config_receipts` 只有 INSERT（`devin_acp/journal.rs:499`），全仓没有 SELECT；`devin_acp_context_inputs` 5.96MB 也无老化。SQLite 只设了 `busy_timeout(5s)`、`foreign_keys=ON`、`journal_mode=WAL`，**没有** `cache_size/mmap_size` 调优，142MB 库走默认约 2MB 页缓存；WAL checkpoint 与写者互斥，进一步放大峰值。

**R4（已证实，时序因素）：所有重活都在 bind 之前串行。**
`recover_incomplete_runtime_runs`、`input_safety`、`session_config_journal`、`SessionStore` 全部同步阻塞；`session_store()` 用 `OnceLock::get_or_init`，初始化期间任何线程调用都会阻塞。当前没有“先 bind 最小健康页、再异步加载会话”的分层，所以慢直接等于“窗口不出现”。

**R5（已证实，慢→失败）：launcher 健康期限 30s 小于现场冷启动 27s 的余量。**
HEAD `config/package-launcher.json:6` 是 30s；现场一次成功冷启动 spawn 08:19:12 → bind 08:19:37 → Tauri 08:19:39，余量约 3s。任何一次 save 峰值（46–93s）都会让 launcher 在健康就绪前终止 web-console，用户看到的是“启动失败”，不是“慢”。工作树已把 30 改为 120，并新增 45s 就绪测试，但这不是启动变快的证明，只是把失败阈值后移。

**R6（已证实，workspace 语义）：同一逻辑位置在不同启动方式下解析出不同路径。**
- `default_workspace_path()` 优先 `COOLZHU_RUNTIME_DIR`，缺失时退到 `%USERPROFILE%\coolzhuagent`（`main.rs:3505-3513`）。launcher 会注入该变量，`cargo run`/旧快捷方式不会 → 两套 workspace、两套 `.coolzhu`、两套库并存。
- `data_dir="D:/tmp/.coolzhu"` 是正斜杠，`join("web-sessions.sqlite3")` 后成 `D:/tmp/.coolzhu\web-sessions.sqlite3`，与规范化 `D:\tmp\.coolzhu\...` 字符串不等；`receipt.json` 与 `identity-observed.json` 正是两种形态。
- HEAD 上 launcher 与 web-console 的 `workspace_id` 摘要算法不同（`hasher.write(bytes)` vs `bytes.hash`），工作树已有未提交修正；这不会让 `decide_reuse` 的路径比较失败，但会污染自检/外部验收。
- `receipt.json` 的“正式服务工作区或会话库错误”在仓库源码与两个已安装 exe 字节串里都搜不到 → 判据来自**未入库的外部验收脚本**，具体比哪一项未确认。

**R7（证据质量缺口）：日志无法把“读/写/锁/磁盘”分开。**
`err.log` 无时间戳；`rolling_file::append` 在 `try_lock` 竞争时整条丢行；`save` 的四个计时点都是累计值（`save schema ready`、`save cleared`、`save sessions inserted`、`save rooms inserted`、`commit` 全从 save 开始计），没有阶段增量；W1-W11、W13-W18 无计时；`web-console.stdout/stderr.log` 均 0B。因此“最近哪次提交导致慢”无法只靠日志归因。

#### 3.1.4 workspace 不一致失败矩阵

| 场景 | 期望 workspace | 实际实例 | 判定 | 用户看到 |
|---|---|---|---|---|
| 正常安装版双击（有持久选择） | `D:\coolzhuagent`（`launcher-user.json` rev 24） | 同 workspace / 同库 / 同构建 | 复用或新启动 | 正常 |
| 首次安装/无持久选择 | `%USERPROFILE%\coolzhuagent` | 随包默认；launcher 首次创建 | 新启动 | 正常（但与旧的 `C:\Users\...` 记录不同） |
| UI 切到 C 后重启 | 最近 workspace = C | `launcher-user.json` 仍是 D | 回到 D | 用户以为“记不住/两套” |
| 旧 `legacy_selections` 仍在 | 当前选择 D | `C:\Users\zhupu\coolzhuagent` 仍是候选 | 候选并列/迁移 | 可能出现选择/冲突 |
| 源码 `cargo run` 与安装版同时开 | 安装版 workspace | 源码实例 `%USERPROFILE%\coolzhuagent`（无 `COOLZHU_RUNTIME_DIR`） | workspace mismatch | 启动失败/冲突提示或抢 8765 |
| 源码配置 `data_dir` 指向仓库外 | `<workspace>/.coolzhu` | `D:/tmp/.coolzhu` | session_db 字符串形态不一致（`D:/tmp/.coolzhu\...` vs `D:\tmp\.coolzhu\...`） | 外部验收脚本判“正式服务工作区或会话库错误” |
| 旧实例未退出、新包版本不同 | 新构建版本 | 旧 build | build mismatch | 拒绝复用，可能启动失败 |
| 端口被非本产品进程占用 | — | 其它进程 | listener_process mismatch | 端口冲突提示 |
| 用户手工设置了 `COOLZHU_WEB_SESSION_DB` | 工作区配置 | 环境变量覆盖（web-console 最高优先） | 启动器只清除自己 spawn 的子进程，`cargo run` 不清 | 需要看诊断日志才知道 |
| 冷启动 27s、HEAD 期限 30s | 健康就绪 ≤30s | SessionStore 21.9s + 其它 | 超时终止 | “双击没反应/启动失败” |
| `workspace_id` 算法不一致 | 同一 `ws-` 值 | HEAD 两侧摘要不同 | 路径比较不受影响，但自检/外部验收可能误判 | 观测历史/验收报告不一致 |

> 关键事实：启动器**不会**为了“能启动”而放宽身份校验，也不会按端口杀未知进程；冲突是 fail-closed 的设计结果。
>
> 用户已定（D2，2026-10-17 修订）：**启动使用“最近一次 workspace”；同端口只允许一个工作区；并发冲突时关闭已有实例并按最近 workspace 重启；首次启动默认 `%USERPROFILE%\coolzhuagent`；不应出现两套 workspace 需要启动选择**。当前 launcher 在 `Conflict` 时既不关闭也不切换，只报冲突；`api_set_workspace` 也不持久化到 `launcher-user.json`。因此“最近 workspace”与“冲突接管”都是新增行为，不是现有能力。详见 3.1.8。
>
> 用户已定（D1）：**启动完成 = 见到开机动画，冷启动 ≤15s**。当前 launcher 要等 `/api/diagnostics/health` 就绪才 spawn Tauri，健康就绪前没有窗口/动画；因此“先见动画”与现状直接冲突，是本条最大的需求变化。

#### 3.1.5 最近改动关联（已证实到提交/文件，未逐提交计时）

| 时间 | 提交/改动 | 与启动的关系 |
|---|---|---|
| 2026-09-25 | `965166c3` 大型集成 | 引入当前 `SESSION-STORE init/initial save` 诊断与保存结构（长期结构问题） |
| 2026-09-30 | `d0ab87dc` 首次并发初始化修复 | 每次开库增加一次只读 preflight + 最多 5s 重试预算；正常约 140–200ms |
| 2026-10-04 | `e1991ee8` 接通 Devin 工程只读审查 | `devin_acp_config_receipts` 开始积累；10-12 备份 74.9MB |
| 2026-10-10 | `172a9be3` 启动恢复前占有 writer + epoch 校验 | 把 writer 锁/journal/epoch 插到启动最前；`try_lock` 使冲突立即失败，新增 fail-fast 点 |
| 2026-10-10 | `7d22dd52` 会话锁诊断 | 引入 `[SESSION-LOCK]`/轮转日志（现 5.5MB/42k 行），证明锁争用；日志 IO 也进入启动路径 |
| 2026-10-10 | launcher 复用/自检四提交（`598adb56`、`bcc3781b`、`a9774084`、`14140eaa`） | 复用判定变成多因子（用户/构建/协议/workspace/DB/启动状态），任一 mismatch 即 Conflict → “启动失败”面变大 |
| 2026-10-11 | `8a13551b` 合并 250 项本地集成 | 工作树随后继续修改到 10-17；包含 runtime run 恢复/input safety 等 bind 前阶段 |
| 工作树未提交 | `health_timeout_secs 30→120`；`launch_paths.rs:1150-1156` 修正 workspace_id 摘要算法 | 前者把失败阈值后移；后者对齐 launcher 与 web-console 的 `ws-` 值（HEAD 上两者算法不同） |
| 数据侧 | 5 天 111MB → 148.8MB（+34%） | 把 O(N) 启动路径推过 30s 门槛；`devin_acp_config_receipts` 92.6MB 是最大单项 |

> 结论：启动慢是“启动即全量写回 + 单锁串行 + 库膨胀”的长期结构问题；workspace 失败是“复用身份校验 + 不同启动方式路径解析不同 + 外部验收脚本字符串比较”的近期显性化问题。两者不是同一个根因。**未发现某一次提交单独把读 SQL 变慢**；8s 读耗时是争用，不是查询回归。

#### 3.1.6 需求规格（R-01 启动）

| 编号 | 需求 | 验收标准（建议） |
|---|---|---|
| R-01-01 | 启动完成 = 见到开机动画 | 在“参考库”上，冷启动从双击到**开机动画出现** P95 **≤15s**（D1）；热启动 ≤8s；动画不得等待 SessionStore/全量历史完成；日志分阶段输出耗时（配置/恢复/读库/首次保存/bind/动画/首屏） |
| R-01-02 | 启动不得因“全量历史”线性阻塞 | 首屏/动画不依赖全量消息反序列化；历史加载有分页/上限；启动日志能区分“读、写、序列化、锁等待、磁盘” |
| R-01-03 | 启动不得无条件全量重写 | 没有状态变化时，启动不产生全表 DELETE/重插；首次保存只在确有迁移/修复时发生；JSON 镜像不得成为关键路径 |
| R-01-04 | workspace 冲突：单端口单工作区 | **同一端口只允许一个工作区**；发现已有健康实例时，默认动作 = **切换到该已有实例**（聚焦其窗口/URL），不再默认启动第二个；冲突面板显示已有实例的 workspace/构建/库，用户可选择“取消”或“显式退出该实例后再启动当前” |
| R-01-05 | 开发配置不得伪装成日常启动 | `data_dir` 在 workspace 外时，启动/诊断显著标识“外部数据根”；安装版双击不得继承开发环境的覆盖变量 |
| R-01-06 | 数据增长可见、可限制 | 设置/诊断页展示库大小、各表 Top N、轨迹/CU/ACP 回执占用；按 12h 运行窗口治理（见 R-09） |
| R-01-07 | 慢与失败分开 | 健康期限必须大于“参考库冷启动 P99 + 安全余量”；超时提示包含“已等待 X 秒、最后阶段、日志路径”，并能“继续等待/重试/退出”，而不是只报失败 |
| R-01-08 | 阶段耗时完整 | 记录 W1-W18 各阶段增量耗时、锁等待、SQLite 读写/提交、JSON 序列化；日志带时间戳且不丢行；能在诊断页导出启动报告 |
| R-01-09 | 身份字段一致 | launcher 与 web-console 的 `workspace_id`、路径规范化、session DB 展示使用同一算法/形态；自检与外部验收不得因分隔符/前缀误判 |
| R-01-10 | 开机动画先于数据就绪 | 双击后先出现可辨识的开机动画/骨架，不得等健康检查通过才创建窗口；动画期间显示阶段（正在读取配置/恢复会话/加载历史），超时可解释 | 双击后尽快出现动画（建议目标 ≤2s，待确认）；动画持续到 UI 可用；动画不是白屏/无响应；失败时动画转为错误页并给出日志入口 |

#### 3.1.7 已定与仍待确认

**已定（D1/D2）**：

1. 启动完成 = 见到开机动画；冷启动 ≤15s。
2. 参考库 = 当前 148.8MB 库的冻结副本；5 次冷启动取 P95。
3. 启动使用“最近一次 workspace”；同端口单工作区；冲突关闭旧实例并按最近 workspace 重启；首次启动默认 `%USERPROFILE%\coolzhuagent`；不做两套 workspace 选择。

**仍待确认**：

4. `data_dir` 配到 workspace 外是否作为正式能力？安装版是否禁止？
5. “关闭旧实例”的边界：优雅退出还是允许强制终止？旧实例有运行任务时默认等待/确认？
6. “最近 workspace”以最后一次成功切换还是最后一次成功启动为准？UI 切换是否立即写记录？
7. 旧 `C:\Users\zhupu\coolzhuagent` 数据如何迁移/清理？是否与 `D:\coolzhuagent` 合并？
8. 安装器是否预创建 `%USERPROFILE%\coolzhuagent`（当前 `Product.wxs` 未创建，首次由 launcher 创建）？

---

#### 3.1.8 启动 workspace 选择逻辑：用户预期 vs 现状

> 用户补充的预期：**启动最近一次的 workspace；并发冲突关闭之前的端口并重启；首次打开默认 `%USERPROFILE%\coolzhuagent`（安装时生成的默认 workspace 目录）；不应出现两套 workspace 需要启动选择，更不应因此冲突无法启动。**

**（1）现状：三套“最近”来源，彼此不通**

| 来源 | 谁写 | 谁读 | 问题 |
|---|---|---|---|
| `%LOCALAPPDATA%\CoolzhuAgent\launcher-user.json` 的 `selection` | launcher 的 `--select-workspace` / 候选采用（`launch_paths.rs:1416-1421`、`:1537-1550`、`:1572-1585`） | launcher 启动时**最高优先**（`:1431-1459`） | 当前 rev 24 指向 `D:\coolzhuagent`；UI 里切换 workspace **不会更新它** |
| `launcher-user.json` 的 `legacy_selections` | 历史迁移 | 候选发现（可能作为 legacy 候选） | 当前文件里仍保留 `C:\Users\zhupu\coolzhuagent`；这就是“两套目录”的残留 |
| UI 的 `workspace_state.current` / `POST /api/workspace` | 前端切换时写内存（`main.rs:3385-3438`） | 只影响当前进程 | **没有持久化到 launcher-user.json**；重启回到 launcher 的旧选择 |
| `default_workspace_path()` | 无 | web-console 直接启动/`cargo run` | `COOLZHU_RUNTIME_DIR` 缺失时退 `%USERPROFILE%\coolzhuagent`（`main.rs:3505-3513`）→ 与 launcher 的 `D:\coolzhuagent` 不同 |
| `data_dir="D:/tmp/.coolzhu"` | 工作区 TOML | 会话库路径 | 即使 workspace 相同，库也在仓库外；路径字符串混用 `/` 与 `\` |

实测 `launcher-user.json`（2026-10-17）：

- `selection.workspace_root = "D:\\coolzhuagent"`（source=`explicit_select`，revision 24）；
- `legacy_selections[0].workspace_root = "C:\\Users\\zhupu\\coolzhuagent"`；
- `observations.last_workspace_root = "\\\\?\\D:\\coolzhuagent"`；
- `observations.last_observed_session_db = "D:/tmp/.coolzhu\\web-sessions.sqlite3"`。

**实际发生什么（串联）**

1. 双击安装版 → launcher 读 `launcher-user.json`，发现 `selection=D:\coolzhuagent` → 直接用 D:\，不会再问“用哪个”。
2. 用户之前若在 UI 里切到 C:\project，`api_set_workspace` 只改内存 → 重启 launcher 仍读 D:\，用户看到“又回到旧目录”。
3. 直接 `cargo run` → 没有 `COOLZHU_RUNTIME_DIR` → `default_workspace_path()` 退 `%USERPROFILE%\coolzhuagent` → 另一套目录/库。
4. 8765 已有 D:\ 实例，再启动 C:\ 实例 → `decide_reuse` 发现 workspace 不同 → `Conflict` → 新实例报错，旧实例继续跑；用户看到“冲突无法启动”。

**（2）为什么会有两套 workspace**

1. **安装默认 vs 历史选择**：安装默认是 `%USERPROFILE%\coolzhuagent`（`config/package-launcher.json:8`），但 `launcher-user.json` 一旦存在旧选择（`D:\coolzhuagent`），启动就优先用它；旧 `C:\Users\...` 仍留在 `legacy_selections` 里。
2. **launcher vs 直接运行**：`cargo run` 不经过 launcher，没有 `COOLZHU_RUNTIME_DIR`，走 `default_workspace_path()` 的 `%USERPROFILE%\coolzhuagent`；安装版走 launcher 的 `D:\coolzhuagent`。两套目录、两套 `.coolzhu`、两套库。
3. **UI 切换不落盘**：用户在某次运行中切到第三个 workspace，`api_set_workspace` 只改内存 `workspace_state`，不写 `launcher-user.json`；下次启动仍回到旧选择，用户感知为“两套/记不住”。
4. **`data_dir` 外置**：`coolzhu.toml` 把业务库放到 `D:/tmp/.coolzhu`，与 workspace 根分离；外部验收若按“库必须在 workspace 内”比较，会把正常配置判成“工作区或会话库错误”。

**（3）端口冲突现状**

- `preflight_web_console_port` 发现监听者后，用 `decide_reuse` 比较 workspace/DB/build/属主；不一致 → `ServiceConflict`（`main.rs:640-675`），只报错，**不关闭旧实例**。
- 只有“已死进程留下的监听”或“已知本地模型占位”才走 `cleanup_stale_listener_processes`（`main.rs:620-633`）；健康超时才 `terminate_process(web_pid)`（`lib.rs:854-862`）。
- 所以用户要的“并发冲突关闭之前的端口重启”目前不存在；现状是 fail-closed，且旧实例可能是另一个 workspace，新实例直接起不来。

**（4）需求规格（R-01 续：单一 workspace 记录 + 冲突接管）**

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-01-11 | 单一“最近 workspace”记录 | 每个 Windows 用户只有一份权威记录（workspace 路径 + workspace_id + 更新时间 + 来源）；首次安装默认 `%USERPROFILE%\coolzhuagent`；UI 切换/launcher 显式选择都原子更新它 | 切换后重启仍打开同一 workspace；不存在“需要启动时二选一”的候选弹窗 |
| R-01-12 | 启动不询问 workspace | 正常启动只读“最近 workspace”；只有记录缺失/路径不可用才进入修复流程；不得静默落到另一个候选 | 双击直接打开最近 workspace；无选择框；日志显示记录来源/时间 |
| R-01-13 | 首次初始化 | 首次启动创建 `%USERPROFILE%\coolzhuagent` 并写入记录；安装器是否预创建需与“安装时生成”对齐 | 全新用户首次启动即打开该目录；目录不存在时创建；权限失败有可读错误 |
| R-01-14 | 端口冲突接管 | 仅当监听者是本产品 `coolzhu-web-console.exe` 且属主为当前用户时，优雅关闭旧实例（请求退出/等待端口释放），再按“最近 workspace”启动；有运行中任务时先提示/确认；非本产品进程不得杀 | 同 workspace 冲突 → 关闭旧实例并重启，不报“身份不一致”；有任务时提示；未知进程 → 只报冲突、不杀 |
| R-01-15 | 开发运行一致 | `cargo run` / 直接启动 web-console 也应使用同一“最近 workspace”记录，或显式 `--workspace`；不得静默退 `%USERPROFILE%` 造成第二套 | 开发运行与安装版打开同一目录；日志显示来源；显式参数优先 |
| R-01-16 | 旧记录迁移 | `legacy_selections` 只作为“无权威记录”时的迁移候选，不参与正常启动；迁移后清理/标记，避免两套并存 | 迁移一次后不再出现旧目录；诊断可见迁移记录 |

**（5）验收场景**

| 场景 | 期望 |
|---|---|
| 全新安装首次启动 | 打开 `%USERPROFILE%\coolzhuagent`，创建目录/记录，无选择框 |
| 在 UI 切到 `D:\projectA` 后重启 | 直接打开 `D:\projectA`，记录已更新 |
| 旧实例（同 workspace）占 8765 | 优雅关闭旧实例 → 启动新实例；用户看到“正在重启”而不是“身份冲突” |
| 旧实例（另一 workspace）占 8765 | 关闭旧实例 → 打开最近 workspace；旧实例有运行任务时先确认 |
| 未知进程占 8765 | 不杀进程；提示端口占用与处理建议 |
| `cargo run` 与安装版 | 使用同一记录；不再出现 `C:\Users\...` 与 `D:\...` 两套 |
| 最近 workspace 路径被删除/无权限 | fail-closed，提示修复/重新选择；不自动落到默认目录 |

**（6）仍待确认**

1. “关闭之前的端口”是**优雅退出**还是允许**强制终止**？旧实例有运行中对话/定时任务/未保存状态时，默认等待还是弹确认？
2. “最近 workspace”以**最后一次成功切换**为准，还是以**最后一次成功启动**为准？UI 切换后立即写，还是下次启动前写？
3. 旧 `C:\Users\zhupu\coolzhuagent` 是迁移合并、保留为只读历史，还是首次升级时提示删除？两套数据是否需要合并？
4. 安装器是否应预创建 `%USERPROFILE%\coolzhuagent`（当前 `Product.wxs` 没有该目录，首次创建由 launcher 完成）？
5. 开发模式是否允许一个显式 `--workspace` 临时覆盖而不写记录？还是也必须更新“最近 workspace”？

---

### 3.2 P-02 ~ P-07 工程目录、聊天室切换、删除文案、主聊天室权限、权限模型、健康图标

#### 3.2.1 P-02 工程目录不能新建和删除

**现状（已证实）**

- 顶部“工程目录”弹层只支持：最近路径列表、手输完整路径、切换目录、打开当前工程文件（`chat_experience.js:96-128`）；没有“新建工程”“删除工程”。
- 后端 `POST /api/workspace` 要求目标目录**已存在**（`main.rs:1606-1607`、`:11593-11612`）；不存在则拒绝，更不会创建。
- 工程窗口内的“新建文件/目录/删除”是**当前工程内部文件树操作**（`index.html:134-137`；`app.js:18656-18769`；`main.rs:8971-9123`），与“工程/工作区”不是一层概念。
- 切换工作区走 `api_set_workspace` → `reload_workspace_scope`：为新工作区完整建一个 `SessionStore`（又一次 load + 全量 initial save），再在全局锁下替换（`main.rs:3385-3438`）。这解释了“新建工程后切换”本身也会慢。

**需求规格（R-02 工程目录管理）**

| 编号 | 需求 | 细节/边界 | 验收 |
|---|---|---|---|
| R-02-01 | 新建工程 | 选择父目录 + 输入工程名；校验名称合法、目标不存在或为空目录、父目录可写；可选项：初始化 `.coolzhu`/`coolzhu.toml`、初始化 git、从模板创建 | 空目录创建成功并自动切换；重名/非法名/无权限/父目录不存在分别有可读错误；不得覆盖非空目录 |
| R-02-02 | 删除工程 | 必须区分“从最近列表移除”和“删除磁盘目录”；磁盘删除需二次确认 + 输入工程名 + 说明影响；优先回收站/可恢复；有运行中会话/定时任务/未完成 Goal 时阻断或要求先停止 | 取消不删除；确认后目标目录与 `.coolzhu` 处理符合选择；失败可回滚/提示；不得删除父目录或 workspace 外路径 |
| R-02-03 | 工程列表与状态 | 显示路径、最近打开、大小、会话数、是否有运行任务；支持置顶/移除；当前工程有明确标识 | 重启后列表保持；移除当前工程有保护；路径去重/规范化 |
| R-02-04 | 切换语义 | 切换工程是“换工作区”，不是换聊天室；切换前保存草稿、停止/确认运行中任务；切换后清空不属于新工程的 UI 状态 | 运行中切换被阻断或显式确认；切换后不串会话/不串库；失败保留原工程 |
| R-02-05 | 安全边界 | 新建/删除只能作用于用户可写目录；禁止 UNC/系统目录/仓库外未授权路径；删除走审计 | 拒绝路径有明确提示；审计记录操作者、时间、路径、结果 |

**待确认**

1. “删除工程”是删磁盘目录，还是只从列表移除？是否要回收站/备份？
2. 新建工程是否需要模板（空工程 / git / 带 `.coolzhu` 配置）？
3. 工程是否允许嵌套/同名不同路径？最近列表上限多少？

#### 3.2.2 P-03 聊天室切换有时候会卡顿

**现状（已证实）**

- 后端 `activate` 每次切换都触发一次全量 `save`：`DELETE` + 重插全库 + 12MB JSON 镜像（`main.rs:40399-40410`、`:38103-38138`、`:50165-50212`）；同类 save 日志 1.5–2.2s，峰值可达数十秒（见 3.1）。
- 前端一次切换串行 6–8 个请求：房间消息、协作状态、权限、诊断偏好、草稿、Agent roster 等（`app.js:15716-15739`、`:11361-11437`、`:9397`）；SSE 还会重连/重放 `permission-changed`（`chat_history_sync.rs:124-126`）。
- 启动时 `loadChatRooms()` 还会再拉一次 roster（`app.js:9397`），与初始化流程重复。
- 卡顿不是单一原因：后端 save 锁 + 前端串行请求 + SSE 重连 + 权限/统计刷新叠加；切换期间没有 skeleton/取消机制，旧响应可能覆盖新房间。

**需求规格（R-03 切换体验）**

| 编号 | 需求 | 验收 |
|---|---|---|
| R-03-01 | 切换即时反馈 | 点击后 ≤300ms 出现目标房间骨架/标题/加载态；不得整页冻结或无反馈 |
| R-03-02 | 切换不触发全量重写 | 只切换活动房间不应重写全部历史；切换期间写操作有界、可取消 |
| R-03-03 | 请求可取消、结果不串房 | 快速连续切换时，旧请求被 abort 或版本号丢弃；消息/权限/统计不得落到新房间 |
| R-03-04 | 消息首屏优先 | 先渲染最近 N 条（如 50 条），更早消息按需加载；不得等全量历史 |
| R-03-05 | 串行改并行/合并 | 权限、统计、诊断等非关键请求并行或延后；roster 只在启动/变更时拉一次 |
| R-03-06 | 失败可恢复 | 超时/失败显示错误 + 重试；保留当前输入草稿；不丢未发送内容 |
| R-03-07 | 可度量 | 记录 switch start / first paint / messages ready / permission ready 四个时间点；P95 目标：骨架 ≤300ms、消息 ≤1.5s、权限 ≤2s |

**待确认**：目标 1.5s 是否可接受？切换时是否允许后台任务继续跑？是否需要在切换前自动停止本轮？

#### 3.2.3 P-04 聊天室删除按键文本错误（“删除删除”）

**根因（已证实）**

- 删除按钮的文案在 `<span>删除</span>` 内（`index.html:644`）。
- `setBusy(button, true/false)` 只查找按钮的**直接文本节点**（`app.js:19606-19628`）：忙态时把直接文本节点替换为“删除中”，但 `<span>` 未被处理；忙态结束找不到原直接文本节点，就 `append(document.createTextNode("删除"))`。于是 `<span>删除</span>` + 新文本节点 = “删除删除”。
- 同类问题存在于“新建/重命名/转交”等把文案放在子元素里的按钮；不是 CSS，也不是安装版特有。
- `setWuxiaIconOnly` 会 `replaceChildren` 掉 `<span>`，但 `initializeIconControls` 的 actions 映射没有 `chat-room-delete`（`app.js:9053-9097`），所以该按钮仍走“图标 + span”形态，触发上述 bug。

**需求规格（R-04 按钮文案）**

| 编号 | 需求 | 验收 |
|---|---|---|
| R-04-01 | 一个按钮只有一份可见文案 | 任意忙/闲状态切换后，`textContent` 中“删除”只出现一次；图标按钮无文字 |
| R-04-02 | 忙态不破坏 DOM | 忙态只改文案/禁用/aria-busy，不追加重复文本节点；结束后恢复原文案 |
| R-04-03 | 覆盖所有同类按钮 | 新建、重命名、删除、转交、保存等使用统一按钮文案机制；加 DOM 断言防回归 |
| R-04-04 | 可访问性 | 忙态 `aria-busy=true`、`aria-label` 说明“删除中”；图标按钮有 title/aria-label |

**待确认**：这些操作按钮最终是“纯图标”还是“图标+文字”？用户要求“不要多余文本”，但删除是危险操作，是否保留文字以防误触？

#### 3.2.4 P-05 主聊天室显示权限未知且无法选择权限

**现状（已证实/高置信）**

- 接口与数据正常：本机 `GET /api/chat/rooms/main-room/permissions` 返回 200，`permission_profile="workspace-write"`、`full_access=false`、`dev_open_permissions=true`、`effective_permission_profile="full-access"`、`updated_at=null`。
- 前端 `taskRenderFullAccessStatus` 的 `known` 需要同时满足：有 `activeChatRoomId`、`projectWorkspaceScope` 非空、`status.room_id===activeChatRoomId`、`!status.error`、`!status.loading`（`app.js:5549-5583`）。
- 首屏 `loadChatRooms()` 在 `/api/state` 设置 `projectWorkspaceScope` 之前就调用 `refreshFullAccessStatus()`（`app.js:9383-9399`、`:8310-8324`），失败后没有“scope 就绪再重试”。
- 另外，`chatPermissionRequestSerial` 是单例：SSE 首轮 `permission-changed`（`chat_history_sync.rs:124-126`）触发刷新、诊断刷新也复用同一 serial（`app.js:5707-5710`），新的请求会把旧响应判为过期；若新请求又把状态置为 `loading` 后失败/被丢弃，UI 会停在 loading → “状态未知”。
- `known=false` 时 `<select>` 被 `disabled=true`（`app.js:5589-5592`），所以“无法选择权限”。

**需求规格（R-05 权限状态机）**

| 编号 | 需求 | 验收 |
|---|---|---|
| R-05-01 | 明确状态 | `未选择房间 / 加载中 / 已就绪 / 读取失败 / 未知` 五态分离；加载中不得显示“状态未知” |
| R-05-02 | 就绪后自动重试 | workspace scope、房间 id、SSE 任一变化后，若仍未知则自动重试一次；失败提供“重试” |
| R-05-03 | 主聊天室可用 | 主聊天室与普通房间同权：数据就绪即可选择权限；不得因初始化顺序永久禁用 |
| R-05-04 | 请求版本隔离 | 权限请求版本与诊断/SSE 刷新解耦；旧响应不得覆盖新状态；同一房间的并发请求合并 |
| R-05-05 | 显示有效权限 | “已保存权限”和“有效权限（含开发开放）”分开显示；`dev_open_permissions=true` 时必须显式提示“当前实际为完全访问” |
| R-05-06 | 失败可解释 | 失败原因（404/网络/解析/超时）可读；禁用控件时给出原因，不静默 |

**待确认**：主聊天室是否允许切到“完全访问”？还是主聊天室固定默认权限，只有普通房间可授权？这影响状态机与默认值。

#### 3.2.5 P-06 访问权限跟随聊天室，而不是某个具体会话模型

**现状（已证实）**

- 存储层是**聊天室级**：`chat_room_permissions(room_id, permission_profile, updated_at)`（`main.rs:42141-42146`、`:42612-42622`）。
- 执行层：`context_permission_profile(chat_room_id)` 优先房间权限，否则全局 `active_permission_profile()`（`main.rs:28898-28909`）；全局 `permission_profile`/`dev_open_permissions` 会覆盖/放大房间结果。
- 会话模型层：`computer_use_enabled`、`tool_allowlist`、`llm_tool_exposure` 只决定“模型能看到哪些工具”（`main.rs:36008-36052`、`:34034`），不是最终文件权限。
- 默认 `workspace-write` 房间的 CU 会被执行器拒绝（`computer_use_executor.rs:2284-2305`），但 **CU 工具仍可能暴露给模型**，表现为“模型尝试后被拒”，而不是“看不到该工具”。
- 当前 `dev_open_permissions=true` 使文件权限全局 FullAccess，但顶栏仍显示“目录权限”（因为 UI 用 `permission_profile` 判断，不读 `effective_permission_profile`）。**“显示默认、实际完全访问”是当前最危险的错位。**
- 工作区外的默认行为是“审批”而非硬拒；用户已确认默认档外目录读写**逐次审批**（D3），方向一致，但需保证“每次”都审批、审批可见、不能一次授权长期放行。

**需求规格（R-06 权限模型）**

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-06-01 | 权限作用域 = 聊天室 | 每个房间独立保存 `permission_profile`；切换房间即切换权限；不得被某个会话模型配置静默覆盖 | 房间 A 完全访问、房间 B 默认，互不影响；重启后恢复 |
| R-06-02 | 三档语义 | **默认**：workspace 内读写 + 允许的工具调用；**workspace 外读/写逐次审批**；不允许 CU。**完全访问**：任意命令/文件 + CU，需双重确认、可撤销、重启失效。**自动审批（待调研）**：指定模型逐调用审查，危险动作仍人工；是否纳入近期待调研 | 三档的可见文案、颜色、确认流程、执行效果一致；不出现“显示默认、实际完全访问” |
| R-06-03 | 默认禁止 CU | 默认档下 `computer_use_perform` 不暴露给模型；即使模型请求也返回明确“需要完全访问”而不是执行后被拒 | 工具目录/模型请求/执行器三层一致；审计可见拒绝原因 |
| R-06-04 | 工作区外逐次审批 | 默认档读/写 workspace 外**每次都要审批**（不是“一次授权永久生效”，也不是硬拒）；审批展示目标路径/操作/调用者；拒绝后模型收到明确原因 | 每次外目录读/写都有审批记录；路径规范化防绕过；UNC/符号链接处理明确；审批超时=拒绝 |
| R-06-05 | 有效权限可见 | UI 同时显示“已保存档位 / 开发开放 / 实际生效”；`dev_open_permissions` 是**过渡开关**，权限方案完全开发完成后删除 | 开发开关开启时顶部/授权详情有醒目提示；关闭后立即回默认；删除后无残留配置/迁移路径 |
| R-06-06 | 与模型工具解耦 | 模型配置只决定“想用什么工具”，房间权限决定“能不能用”；工具目录按房间权限过滤 | 默认房间的模型看不到 CU；完全访问房间才可见；切换房间后工具列表同步 |
| R-06-07 | 审计 | 每次权限变更、完全访问启用/撤销、CU 拒绝、目录外访问尝试都记录操作者/房间/时间/原因 | 审计可查；重启后保留；不记录密钥 |

**auto 复杂度调研（R1 报告，供 D3 决策）**

| 产品 | auto 机制 | 复杂度要点 |
|---|---|---|
| Codex | `approvals_reviewer="auto_review"`，reviewer subagent 只审“本来要 ask”的四类请求；沙箱内不重审 | 审查模型/策略文本/成本/审计；不是单一开关 |
| Claude Code | 6 种模式，`auto` 用分类器；规则分 environment/allow/soft_deny/hard_deny 四层，支持 `$defaults` 增量 | 分类器 + 四层规则 + 意图判定 + 来源可见 + 即时生效 |
| Cursor | allowlist → 沙箱 → LLM 分类器三段式；自然语言 allow/block instructions 只是 steering | 需要沙箱配合；分类器模型不可用则 auto 不可用 |
| DSH | `auto` = danger-full-access + ask + 逐调用 review；子 Agent pin `never` | 审查在派发前；拒绝可上抛；无 allow-always |

共同点：**分类器/审查模型、规则分层、审计、熔断、成本控制、危险动作清单**缺一不可。结论：auto 不是一个 UI 开关，复杂度高；是否近期纳入需单独排期，不建议与本次权限基础改造捆绑。

**已定（D3）**：默认档读/写 workspace 外**逐次审批**；`dev_open_permissions` 在权限方案完全开发完成后删除；auto 待调研市面复杂度后再定。

**仍待确认**

1. “逐次审批”的粒度：一次读/写算一次，还是同一轮同一路径可合并？审批结果是否只在当前轮有效？
2. 谁审批：用户本人、指定模型、还是“用户+模型”双签？审批 UI 在聊天内还是系统弹窗？
3. 删除 `dev_open_permissions` 的迁移：已有配置/房间权限如何转换？是否要一次性提示？
4. 自动审批的“指定模型”是全局/房间/会话级？高风险动作清单谁维护？

#### 3.2.6 P-07 系统警示/健康显示未接后端，两个 icon 改为权限显示

**现状（已证实）**

- 顶部有三个状态节点：`chat-top-status-ready`（就绪）、`chat-top-status-lantern`（灯笼）、`chat-top-status-alert`（警示）（`index.html:91-104`）。
- CSS 里 `lantern` 恒隐藏、`alert` 在 idle 时隐藏，尺寸约 30×30（`wuxia_layout.css:77-78`、`jade_controls.css:24-34`）；用户实际只看到“就绪/警示”或其一。
- 数据源：`agentHealthSnapshot()` 只看可发送 Agent 的 `api_key_status` 文本（`app.js:7460-7473`）；`renderChatTopAlertStatus` 把该 tone 与任务 error 组合（`:7502-7561`）；`lantern` 取任务状态/待审批数（`:12259-12278`）。**都不读 `/api/diagnostics/health`**。
- 真正的健康检查只在诊断窗口用模板拼接调用（`app.js:1761`、`:2112`；`main.rs:80850` 测试被绕过），与顶部图标无关。
- 权限已经有一个独立下拉（`index.html:77-90`），但用户认为它和两个状态 icon 占位重复、且 icon 无实际作用。

**需求规格（R-07 顶部状态）**

| 编号 | 需求 | 验收 |
|---|---|---|
| R-07-01 | 移除无后端依据的状态 icon | 删除“灯笼/警示”两个装饰性节点；保留的“就绪”必须有明确数据源与可点击说明 |
| R-07-02 | 权限 pill 取代其位置 | 在聊天室标题右侧显示权限 pill：`默认权限 / 完全访问 / 自动审批（待调研）/ 未知`；颜色、图标、文案一致 |
| R-07-03 | 点击即改 | 点击 pill 打开权限菜单；选择“完全访问”走双重确认；选择默认立即生效；变更后全端同步 |
| R-07-04 | 有效权限 | pill 显示“实际生效”而非仅保存值；开发开放/外部目录等放大因素用角标/提示 |
| R-07-05 | 健康归位 | 系统健康（诊断/连接/模型服务）放在诊断页或状态详情，不在主聊天栏用无数据的 icon 冒充 |
| R-07-06 | 可访问性 | pill 有 `aria-label`、键盘可达、焦点可见；颜色不是唯一信息载体 |

**待确认**：顶部还要不要保留“就绪”灯？如果保留，它的数据源是否改为 `/api/diagnostics/health` 的总体状态？

---

### 3.3 P-08 ~ P-09 定时任务、后台子 Agent、轨迹与 CU 记录

#### 3.3.1 P-08 定时任务强行占用一个聊天室

**现状（已证实）**

- `SCHEDULED_TASK_SYSTEM_ROOM_ID = "room-scheduled-tasks"`、显示名“定时任务”（`main.rs:253-257`）；**启动时无条件幂等创建**（`main.rs:38056` → `:41425-41465`），重启必复现。
- 系统室禁止重命名（`:40419-40423`）、禁止删除（`:40475-40479`）；它承载两类写入：定时任务状态消息（`:41499-41541`）与 Goal 阶段镜像（`:41469-41495`，唯一调用 `:10689-10697`）。
- 但 poll 型定时任务**必须显式绑定结果房间**（`scheduled_delivery.rs:10-38`、`index.html:1157`）；只有无 `chat_room_id` 的旧任务才回退系统室（`scheduled_delivery.rs:56-63`）。现网唯一任务绑定的是 `room-1791131523339`，系统室只剩 3 条 `scheduled-status`。两库 `kind='goal_phase'` 的 `runtime_runs` 均为 0 行，所以这个室的“运行轨迹”实际上永远空态。
- `/api/chat/rooms` 的 `chat_room_list_response` 直接遍历全部 `state.chat_rooms`（`main.rs:38364-38375`），没有过滤系统室；左栏 `renderChatRoomList` 也全量渲染（`app.js:15669-15713`），计数还包含它。
- 左栏“定时任务”扩展页（`index.html:111`、`data-window-id="schedules"`，`:905`）**不是独立页面**：`index.html:1140` 的 `task-schedule-section` 被 `workspace_panels.js:331-332` append 到该面板；打开时 `openChatToolWindow` 再把整个 window 节点 append 到聊天右栏的 `chat-tool-host-content`（`app.js:11696`），关闭用注释锚点还原。内容=创建表单（会话/类型 poll|goal/目标/结果房间/计划/内容/权限/接力超时）+ 列表。
- 定时任务本体存在 `scheduled-jobs.sqlite3`；后台调度器每 30s 扫描一次，不依赖前端页面是否打开（`main.rs:10541-10554`）；投递/执行在 `scheduled_jobs.rs`、`scheduled_execution.rs`、`scheduled_delivery.rs`。

**需求规格（R-08 定时任务去聊天室化）**

| 编号 | 需求 | 验收 |
|---|---|---|
| R-08-01 | 定时任务不再占会话 | 会话导航不显示系统室；系统室不出现在“新建/重命名/删除”可选范围；已有 `room-scheduled-tasks` 的消息迁移到任务页/轨迹，不丢历史 |
| R-08-02 | 定时任务页完整 | 列表（启用/停用/下次触发/上次结果/绑定 Goal）、创建/编辑/删除、手动运行、查看投递结果、失败原因 |
| R-08-03 | Goal 镜像有归属 | Goal 阶段消息进入“任务/轨迹”而不是某个聊天室；需要能按 Goal/阶段过滤；原系统室历史可追溯 |
| R-08-04 | 后台运行不依赖页面 | 关闭任务页/窗口后定时任务继续；重启后恢复；漏触发有补偿或明确记录 |
| R-08-05 | 权限 | 创建/运行/删除定时任务受房间或工程权限约束；高风险任务（CU、命令）需显式授权 |

**待确认**

1. `room-scheduled-tasks` 是彻底删除，还是保留为“系统内部室”但不在 UI 显示？Goal 阶段镜像是否还需要一个 room_id？
2. 历史系统室里的消息迁移到哪个页面、以什么结构展示？是否需要保留原消息 id？
3. 定时任务是否允许绑定“聊天室作为结果投递目标”？如果不允许，结果只进任务页？

#### 3.3.2 P-09 轨迹/CU/后台子 Agent 没有统一页面，也没有老化

**现状（已证实）**

- 轨迹窗口：`index.html:907-923`（“当前聊天室的运行轨迹” + 系统安全详情 + “CU 运行…”按钮）；接口 `/api/chat/rooms/{room_id}/trace`（`main.rs:1877`、`chat_insights.rs:275-288`），按 room 限定。它只读 `runtime_runs` → `runtime_run_events` → `tool_calls` + `chat_usage_events`；**不展示** event payload、CU 步骤、`execution_index`、`scheduled_job_attempts`、`goal_events`。
- 数据表：`runtime_runs`、`runtime_run_events`、`tool_calls`、`chat_usage_events`、`chat_message_timing`、`computer_use_runs`、`computer_use_steps`、`computer_use_step_details`、`computer_use_plan_attempts`、`computer_use_planner_diagnostics`、`computer_use_cleanup_incidents`、`devin_acp_attempts`、`devin_acp_config_receipts`、`devin_acp_context_inputs`、`execution_index`。
- 当前库/备份的规模：`runtime_run_events` 9244、`tool_calls` 6068、`computer_use_*` 约 4.3k 行、`devin_acp_config_receipts` 5180 行 / **92.6MB（占现网库 69%）**、`devin_acp_context_inputs` 438；CU 五表合计仅约 4.6MB；聊天正文约 9MB，库 142MB。111MB 备份里 receipts 74.9MB（67%），轨迹/CU 并不是主要占用。
- 后台子 Agent：**没有独立的 `runtime_runs` 行**（`runtime_runs.kind` 的 CHECK 只有 `chat_turn/goal_phase/goal_loop`）；子 Agent 只以 `chat_usage_events.request_kind='child_agent'` 挂父 `run_id`（`main.rs:27952`），正文是父回合的 `tool-result`，被 `VISIBLE_SQL` 排除出可见聊天（`chat_insights.rs:61`），只在轨迹存档可见。真正覆盖“子 Agent/定时/Goal”三源的是右栏“执行”页（`execution_panel.js:49-52`，表 `execution_index`），但两库 `execution_index` 均 **0 行**，说明运行产物与源码不一致或该页从未被填充。
- CU 记录：`computer_use_store.rs` 有 runs/steps/attempts/diagnostics；截图/证据落盘；`/api/computer-use/runs` 可列出最近运行。当前只在轨迹窗口的“CU 运行…”按钮里按需查看，不在统一任务页。
- 老化：`session` 的 `max_*` 只裁内存工作集，不裁 SQLite 历史（`main.rs:38154-38165`、`:50532-50560`；`history_persistence.rs:1` 明确“淘汰不是删除”）；铁证：现网单室 2210 行 > `max_room_messages=2000`。`[computer_use] evidence_retention_days=7` 只在测试里被读（`computer_use_store.rs:3945/3972/4001`），**生产零引用**；截图目录 **2638 文件 / 1.06GiB**、`computer-use` 目录 192 文件 / 74.5MB 无人清理；全仓无 `VACUUM`、无任何 `DELETE computer_use_*/runtime_*/devin_acp_config_receipts`。**“轨迹无限保存”是当前事实，不是担心。**

**需求规格（R-09 统一任务/轨迹页 + 老化）**

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-09-01 | 统一页面 | 一个“任务”扩展页，标签：定时任务 / 后台任务 / 轨迹 / CU 记录；共享筛选（工程、聊天室、时间、状态、模型、Agent） | 四类数据都能在同一页进入；不再需要从聊天窗口打开轨迹；深链接可定位单条 run |
| R-09-02 | 后台子 Agent 可见 | 列出父聊天室、子 Agent/会话、任务、开始/结束、状态、工具步骤数、token/耗时；可取消/重试/查看轨迹 | 子 Agent 运行中可见；结束后可查；取消有终态；不串父/子 |
| R-09-03 | 轨迹详情 | 一次 run = 模型轮次 + 工具调用 + CU 步骤 + 结果/错误；展示时间线、输入/输出摘要、耗时、token、截图引用；大内容按需展开 | 能从 run 跳到工具/CU/消息；失败原因可读；敏感内容脱敏 |
| R-09-04 | CU 记录 | 列表 + 详情：动作、目标、结果、截图、基线、事故/清理；按房间/时间过滤；支持导出证据包（可选） | 截图可打开；失败/中断有状态；证据路径可追溯 |
| R-09-05 | 12h 运行时窗口 | **已定（D5）**：轨迹/CU 截图/ACP 回执/后台任务**全部统一固定 12 小时**；按“运行时长 × 数据速率”估算占用并展示；聊天消息历史不在此窗口内 | 超过 12h 的数据被清理；页面显示估算大小/最早记录；清理不影响聊天记录；12h 不可调 |
| R-09-06 | 老化策略 | 按时间窗口删除最旧的非活动记录；活动 run/未完成 Goal/未处理事故不删；**不归档**；删除可审计；容量估算作为护栏（异常增长告警） | 12h 后自动清理；不删活动证据；无归档路径；删除有记录；异常增长有告警 |
| R-09-07 | 回执/上下文治理 | `devin_acp_config_receipts`、`devin_acp_context_inputs` 纳入同一 12h 窗口；只写不读的表要么限制条数，要么改为按需写 | 单表不再无界增长；诊断页能看到 Top 表 |
| R-09-08 | 迁移与兼容 | 老库首次升级时建立基线/索引；不因老化破坏外键/搜索索引；必要时 VACUUM/重建 FTS | 升级后旧轨迹仍可查（在窗口内）；搜索可用；库大小下降 |
| R-09-09 | 分类保留 | 至少区分：聊天消息历史（永久/按消息策略）、`runtime_run_events`、`tool_calls`、CU 证据（截图）、ACP 回执/上下文、FTS 索引；运行类默认 12h，聊天类不走 12h | 每类可单独查看；不会因清轨迹误删聊天记录 |
| R-09-10 | 执行页数据真实 | `execution_index` 空表必须解释/回填；若保留“执行”页，子 Agent/定时/Goal 三源都应有真实行；否则以 `runtime_*` 为真源 | 执行页不再空白；与轨迹页数据一致；重启后仍在 |

**已定（D5）**：轨迹/CU 截图/ACP 回执/后台任务全部统一固定 **12 小时**；不归档；大小按运行时长估算。

> 影响提示：当前 `devin_acp_config_receipts` 92.6MB 是长期累积；12h 窗口覆盖该表后，按现写入速率只会保留最近 12h 的一小部分，库会显著缩小、启动的全量写回也会变轻。

**仍待确认**

1. 清理粒度：逐条、按 run、按天分区？删除时是否要保留“已清理”索引/统计？
2. 被引用的 CU 证据（仍被活动 run/未处理事故引用）是否随 12h 一起删，还是等引用结束后删？
3. 是否允许“关闭自动清理”？建议不允许，否则重新无界增长。
4. 定时任务/后台任务/轨迹/CU 是一个页面的四个 tab，还是“任务”+“轨迹”两个页面？

### 3.4 P-11 设置页与模型接入（含界面规划草案）

> 界面规划草案（供审核，未实施）：
> - [A · API 接入（文本/多模态）](../../design-assets/2026-10-17-settings-redesign/A-settings-api.png)
> - [B · 订阅接入（账号 + ACP）](../../design-assets/2026-10-17-settings-redesign/B-settings-subscription.png)
> - [C · 图像/视频生成](../../design-assets/2026-10-17-settings-redesign/C-settings-generation.png)
> - [D · 扩展栏信息架构](../../design-assets/2026-10-17-settings-redesign/D-extension-ia.png)
> - 可编辑源：[wireframes.html](../../design-assets/2026-10-17-settings-redesign/wireframes.html)

#### 3.4.1 现状（代码事实）

**（1）页面结构**

- 设置窗口只有一个可见 section：`data-role="unified-model-settings"`；旧的会话生命周期、工具、音频 section 全部 `hidden`（`index.html:181-184`、`:272-273`；CSS 注释“设置扩展栏只展示当前会话的模型配置”，`styles.css:17824-17826`）。
- 顶部：标题「模型与会话」+ 副标题「为当前聊天室选择模型，配置连接、上下文与思考参数。」+ 右上「新建会话」+「编辑配置的会话」下拉 + `target-note`（`model_settings.js:54-56`）。
- 底部：sticky `status` + 「保存配置」（`model_settings.js:113`）；状态文本没有 `setTimeout`，载入后常驻「配置已载入。修改后对下一轮会话生效。」（`:353`），保存后常驻「已保存，对下一轮会话生效。」（`:475`）。
- `target-note` 的静态文案会被 `chat_experience.js:330-343` 改写为“当前发送给 X；当前编辑 Y…”长句；它承载“发送对象≠编辑对象”的真实信息，但用户明确认为这是多余文本栏。

**（2）三个“插件”不在设置页**

| 条目 | 位置 | 实际能力 | 数据/凭据 |
|---|---|---|---|
| Devin 账号 | 插件市场 `index.html:981-985` | CLI 登录状态、登录/取消/刷新 | `devin_acp/auth.rs`；缓存 60s |
| Devin 模型与会话 | 插件市场 `index.html:986-989` | 复用同一个 `model_settings`，`backendFilter="devin_acp"`、`defaultProtocol="devin_acp"` | 与设置页同一 `/api/sessions*` |
| Devin 云会话 | 插件市场 `index.html:1015-1032` | 独立云端 API 会话/手动测试 | `devin-plugin.json` 加密 key、独立账本 |
| OpenCode Zen | 插件市场 `index.html:990-995` | 只是前端 profile + `initialDraft`；保存为 `provider="custom"` 的普通 HTTP 会话 | 无独立后端 |
| Hugging Face | 插件市场 `index.html:996-1001` | 同上 | 无独立后端 |

`extension_market.rs:103-111` 把 OpenCode/HF 两条连接器硬编码为 `enabled/installed=true, manageable=false`；`provider_connectors.js:25-30` 只是 `mount` 同一套 `model_settings`。所以“删掉插件、迁移到设置页”本质上是**入口与信息架构迁移**，不是把两套后端合并。

**（3）模型接入数据模型**

- `provider` 是自由字符串；`backend_kind` 由 provider 派生（`llm_http/devin_acp/devin_cloud`）；设置页没有 provider 选择，非 Devin 新建一律 `provider="custom"`（`model_settings.js:436-439`、`:465`）。
- HTTP 协议只有 `openai_chat_completions` / `anthropic_messages`，外加 `devin_acp`（`model_settings.js:49-51`）。
- `model_type` 7 枚举：`text/vision/multimodal/image/audio/video/embedding`（`model_settings.js:83`）；image/video 有真实执行链（`main.rs:18646-18670`、`:32062-32165`、`:32345-32696`），但 URL 自行拼 `{base}/{endpoint}`、参数硬编码（768×1152 / 121 帧 / 24fps），没有复用 `llm-adapter` 的 `OpenAiImagesGenerations` / `OpenAiVideos` 解析器；audio/embedding 没有按 `model_type` 的分派。
- OpenRouter 只有目录提示名（`model_discovery.rs:179`），没有 provider 枚举/连接器/白名单。
- 会话上限 10（`main.rs:40788-40792`）；“每个厂商/模型一个会话”的迁移会受此限制。

**（4）思考档位**

- effort 存 `sessions.reasoning_effort`；`reasoning_mode`/采样/工具参数存 TOML。
- 保存走 `POST /api/sessions/{id}/model-settings`，需要整包 `parameters` + `expected_revision`（`session_config_service.rs:174-252`）；`PATCH /api/sessions/{id}` 可只传 `reasoning_effort`/`avatar`（`main.rs:13045-13054`、`:40922-40997`）。
- 设置页只有表单 submit，没有滑块/即时写；Devin 的“档位”是**精确模型变体 ID**（`devin_acp/discovery.rs:169-177`），与 HTTP 的 effort 映射是两套机制。

**（5）头像**

- 控件存在于 DOM（`index.html:196-203`），但被 `chat_experience.js:396-398` 移到 `#session-extra-controls`，而该容器在 `hidden` 的 `section.session-settings` 内 → 实际不可见。
- `chat_experience.js:400-414` 的保存目标是 `activeSessionId`，不是下拉所选编辑会话；后端只接受 `assets/avatars/*.png|webp`，没有上传端点。
- 头像数据本身可用：`SessionSummaryDto.avatar` + `PATCH /api/sessions/{id}`（`main.rs:52313`、`:40974-40976`、`:50775-50799`）。

#### 3.4.2 需求规格（R-11 设置页）

**R-11-01 标题与操作区**

| 项 | 规格 |
|---|---|
| 标题 | 只保留「模型与会话」；删除「为当前聊天室选择模型…」副标题 |
| 操作 | 「新建会话」与「保存配置」并列放在标题右侧；不再在页底单独放保存 |
| 状态 | 不常驻“配置已载入/已保存”；用 toast + 会话选择器上的未保存圆点表达 |
| 上下文 | 不显示“当前发送给…”长文本；用会话选项中的「发送中」徽标表达 |

**R-11-02 会话选择、头像与思考滑块**

| 项 | 规格 | 边界 |
|---|---|---|
| 会话选择 | 下拉列出“头像 + 名称 + 模型 + 发送中/已保存/未保存” | 会话数 ≤10；切换前有未保存更改需确认 |
| 头像 | 选择器下方独立“会话头像”行：品牌头像库 + 上传 + 预览；保存到**当前编辑会话** | png/webp；建议 ≤2MB、建议 256×256；无头像回退 generic；上传失败不改旧头像 |
| 思考滑块 | 会话选择器正下方；档位 `自动/关闭/低/中/高/最高`（按模型支持裁剪）；拖动即 PATCH，不点保存 | 仅已有会话可即时写；新建草稿先存默认值；失败回滚并提示；Devin 档位映射到精确模型变体；不支持思考的模型显示“不适用” |
| 即时性 | 下一次请求生效，不打断当前流；当前轮结束后按新档位 | 与 Codex/Claude 的“下一请求生效”一致（见 3.4.4） |

**R-11-03 一级分类：API 接入 / 订阅接入**

| tab | 内容 | 说明 |
|---|---|---|
| API 接入 | 提供方列表：OpenRouter、OpenAI 兼容、Anthropic、OpenCode Zen、Hugging Face、自定义/本地；连接字段 + 获取模型 + 能力标签 | 用 Base URL/Key 直连 HTTP；OpenRouter 必须有正式 provider 身份与模型目录 |
| 订阅接入 | **首批：Devin（已实现）+ OpenAI/ChatGPT（ChatGPT OAuth）**；Anthropic/Claude、Gemini 后续；OpenCode Go 不做，OpenCode 仅保留 API 接入（Zen key）。登录状态 + 账号 + 模型目录 + ACP | 登录由厂商官方页面完成；本机只保存登录状态/令牌引用；不保存明文密码；额度/计费只展示厂商返回值 |

**R-11-04 API 接入字段与模型发现**

| 字段/动作 | 规格 | 验收 |
|---|---|---|
| Base URL | 必填；校验 http/https、去尾斜杠；本地地址允许 | 非法 URL 即时错误 |
| API Key | 密码框；留空保留已保存；可清除；不回收显 | 保存后不回显；清除有确认 |
| Endpoint | 可选；留空按协议补齐 | 不重复拼 `/v1` |
| 获取模型 | 调用目录接口；显示来源/时间/是否完整；支持筛选；失败可重试 | 目录不自动覆盖手填值；“采用”才写入 |
| 模型 ID | 必填；支持手填 | 与目录候选并列 |
| 能力标签 | 输入/输出模态、工具调用、思考档位、上下文、输出上限、价格（若目录提供） | 标签注明“目录声明/未实测” |
| 模型类型 | 对话/多模态/图像生成/视频生成/音频/向量（按实际支持裁剪） | 切换类型时表单区块切换，不保留不适用字段 |
| 上下文/输出 | 可留空用默认；显示有效值 | 超范围校验 |

**R-11-05 订阅接入字段与 ACP**

| 项 | 规格 | 验收 |
|---|---|---|
| 厂商列表 | 首批：Devin、OpenAI/ChatGPT（ChatGPT OAuth）；Anthropic/Claude、Gemini 标“后续/未开放”；OpenCode 不在订阅列表。标注“已登录/未登录/不可用” | 未登录不显示假模型；后续厂商不可误导为可用 |
| 登录 | 打开厂商官方登录；轮询状态；可取消；失败可重试 | 不收集账号密码；登录态可刷新/退出 |
| 模型目录 | 只显示账号实际返回的 ID/家族/思考档位/能力/额度说明；支持搜索 | 不手拼不存在的变体；目录变化提示重新核对 |
| ACP | 订阅模型通过 ACP 进入聊天室；发送前核对精确生效 ID | 模型不符/换模失败时停止，不静默降级 |
| 计费/额度 | 展示厂商返回的额度/计划；不做本地计费推算 | 明确“订阅/额度/按量”三种口径 |

**R-11-06 图像/视频生成**

| 项 | 图像 | 视频 |
|---|---|---|
| 协议 | 独立图像生成端点/请求体；不复用 chat completions | 独立异步任务提交 + 轮询/callback |
| 参数 | 尺寸/分辨率、宽高比、质量、数量、输出格式、种子、参考图/局部编辑图 | 时长、分辨率、宽高比、首帧/末帧、参考图、音频、种子、callback |
| 校验 | `n` 上限、尺寸约束（倍数/比例/像素区间）、互斥字段 | 时长/分辨率按模型能力裁剪；不支持字段禁用并说明 |
| 结果 | 进资产/预览，不在聊天里塞 base64；记录 usage/费用 | 异步任务进“后台任务”；结果过期前落盘；失败保留错误 |
| 统计 | 与对话 token 分开统计 | 任务数/时长/费用单独统计 |
| 能力目录 | 用 OpenRouter `input_modalities/output_modalities` 或厂商目录生成控件 | 视频目录字段（durations/resolutions/frame_images）驱动 |

**R-11-07 保存与草稿**

| 项 | 规格 |
|---|---|
| 保存 | 只由「保存配置」触发；校验失败定位到字段；revision 冲突提示“配置已变化，请刷新/对比” |
| 成功 | 一次性 toast「配置已保存」，3s 后消失；不再显示常驻状态文本 |
| 未保存 | 会话选择器右侧小圆点/“未保存”；切换会话/关闭设置/切换工程时确认；不显示长段说明 |
| 新建 | 「新建会话」进入空白草稿；保存前不允许发送；保存后出现在下拉并选中 |
| 失败 | 明确失败字段/原因；保留用户输入；不静默回滚 |

**R-11-08 从插件市场迁移**

| 项 | 规格 | 验收 |
|---|---|---|
| 移除入口 | 插件市场不再显示 Devin 账号/Devin 模型与会话/OpenCode/HF 连接器卡片 | 设置页能完成同样的配置；插件市场只保留 DSH 社区插件与本地插件 |
| 保留数据 | 已保存的 Devin/OpenCode/HF 会话仍能在设置页看到、编辑、发送 | 不丢 key/会话/远端绑定 |
| Devin 云会话 | 若保留，迁到“高级/诊断”或独立页；不与模型会话混在一起 | 不出现两个 Devin 入口 |
| 去重 | 设置页只挂载一个 `model_settings` 实例；插件市场不再挂载同一组件 | 不出现两份表单/两个保存按钮 |

**R-11-09 可访问性与错误**

- 所有控件有 label/aria；滑块键盘可调；toast `role=status`；错误 `role=alert`；颜色不是唯一状态载体。
- 密钥字段 `autocomplete="new-password"`、不回显；日志/诊断脱敏。

#### 3.4.3 界面规划草案说明

- A（API 文本/多模态）：标题只留标题；右上“新建 + 保存”；会话选择器带“发送中”；下方头像行；再下方思考滑块；一级 tab；左侧提供方列表 + 右侧连接/能力/高级参数；保存后 toast。
- B（订阅）：厂商列表 + 账号卡 + 模型目录表 + ACP 接入；不出现“插件市场”入口。
- C（生成）：图像/视频分段；尺寸/时长/参考图/首帧等独立字段；结果进资产/后台任务；与对话 token 分开计费。
- D（扩展栏 IA）：任务页合并定时/后台/轨迹/CU；终端删除；统计、SKILL、插件市场、token 顶部展示各自归位。

> 这些图是**布局与字段草案**，不是最终视觉稿；审核通过后再进入视觉/交互细化与实施。

#### 3.4.4 市场参考（事实，不是方案）

| 主题 | 参考 | 可借鉴事实 | 来源 |
|---|---|---|---|
| API/订阅双通道 | Codex（ChatGPT 登录 vs API key）、Claude Code（订阅 OAuth vs `ANTHROPIC_API_KEY`）、Gemini CLI（Google 登录 vs API key） | 登录态决定能力集与额度；API key 可能优先于订阅导致意外计费，必须显示“当前计费通道” | R1 报告；`https://developers.openai.com/codex/pricing/`、`https://code.claude.com/docs/en/authentication` |
| OpenRouter 模型目录 | OpenRouter `/models`、`/images/models`、`/videos/models` | 模态用 `input_modalities/output_modalities` 数组；图像/视频目录字段完全不同（尺寸描述符、时长/分辨率/首末帧/音频） | `https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties`、`.../images/generate-an-image`、`.../video-generation/list-all-video-generation-models` |
| 思考档位交互 | Codex `Alt+,`/`Alt+.` 微调、`/model` 内调档，默认不持久化；Claude `/effort` 下一请求生效；Gemini `thinkingLevel`/`thinkingBudget` 按代际 | “即时生效（下一请求）”比“必须保存”更主流；档位要按模型能力裁剪，`auto` 表示恢复默认 | `https://github.com/openai/codex/blob/27c05a52/codex-rs/tui/src/chatwidget/reasoning_shortcuts.rs`、`https://code.claude.com/docs/en/model-config`、`https://ai.google.dev/gemini-api/docs/generate-content/thinking` |
| 生成参数 | OpenAI Images 尺寸约束表达式；Veo 4/6/8s、产物 2 天过期；Sora 已下线 | 生成参数是“约束/描述符”，不是固定枚举；结果会过期；不能硬编码单一供应商 | `https://developers.openai.com/api/reference/resources/images/methods/generate/`、`https://ai.google.dev/gemini-api/docs/veo`、`https://developers.openai.com/api/reference/resources/videos/methods/retrieve` |
| 订阅模型目录 | Devin ACP 账号目录、OpenCode Zen `/models` | 只显示账号实际返回的模型；精确模型 ID 与思考变体由目录决定 | `https://docs.devin.ai/cli/models`、`https://opencode.ai/docs/zen/` |

#### 3.4.5 待确认

1. **已定（D4）**：首批只做 Devin + OpenAI/ChatGPT；OpenAI 走 **ChatGPT 账号 OAuth**；OpenCode Go 先不做，OpenCode 只保留 API 接入（Zen key）。
2. **仍待确认**：OAuth 的 scopes/回调/令牌存储、额度展示、与 API key 通道的切换提示；ChatGPT 登录是否同时带来 Codex 云端能力，还是只做模型目录。
3. 图像/视频生成的产物展示：进聊天消息卡片、右侧资产页，还是独立“作品库”？是否需要历史作品管理？
4. 思考滑块是否允许“关闭”（部分模型不能关）？滑块档位以模型目录为准还是统一 6 档？
5. 头像上传是否允许自定义图片？还是只从品牌头像库选择？存储位置/大小/裁剪规则？
6. 删除插件市场三个入口后，Devin 云会话（独立 API）是否保留？保留的话放哪？
7. 设置页是否要保留“高级参数”（温度/Top P/工具暴露/时限）展开区？默认折叠还是隐藏？

---

### 3.5 P-12 统计信息扩展栏没有内容

#### 3.5.1 现状（已证实）

- 面板 DOM 存在：`index.html:928-930`（`统计信息` / `chat-usage-refresh` / `chat-usage-list` / `chat-usage-note`）；左栏 tab 在 `index.html:113`。
- 渲染存在但只有文本：`chat_experience.js:258-280`，按 `usage` 行输出 1-3 行 `<p>`，空时“暂无接口用量记录”；没有 canvas/SVG 图表/图表库/图表样式。
- 接口：`GET /api/chat/rooms/{room_id}/insights`（`main.rs:1876`、`chat_insights.rs:206`），内部走 `request_usage::summary`。
- 当前为空的数据原因：`chat_usage_events` 共 539 行，**全部属于旧 workspace `ws-23f646a969206cb4`**（最后 2026-10-06T05:54Z）；当前房间 10-06 之后的 779 条 `runtime_runs` 属于 `ws-8d17063fefe5c23c`。`summary` 按 room + workspace 过滤 → 当前工作区 0 行。即使取到，539 行的 `usage_known_mask=0`、token 全 0，前端也只会显示“未知”。
- 更根本的数据缺口：当前 provider=devin / model=swe-2-max。`TurnResult` 无 usage（`devin_acp/session.rs:344-351`），`devin_acp_attempts` 只有 `model_json`、无 token/时间戳（`devin_acp/journal.rs:53-58`）；ACP 的 `usage_update` 只是上下文 `used/size`，不是 token。全仓唯一 token 写入点是 HTTP 的 `LedgerObserver`（`request_usage.rs:60-84`）。
- 耗时口径不对：`chat_message_timing.elapsed_ms` 是**整轮墙钟**（`turn_clock` 在派发前起计，同轮多个回复写同一个值；`main.rs:18616`、`:19896`、`chat_insights.rs:33-45`），不是生成耗时；全仓没有首 token 时间/流式 chunk 时间/model/streamed 标记。
- 聚合能力：按房间/会话/轮次/状态可查；按模型（按请求）不可、按小时/天无接口无索引；3 个索引都不含 `created_at`。图表做不出来 = ACP 无 token + 接口只有标量 + 前端无绘图原语。

#### 3.5.2 需求规格（R-12 统计图表）

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-12-01 | 当前聊天室口径 | 默认只统计当前聊天室；可切换到“全工程/指定会话/指定模型” | 页面标题显示当前范围；切换后数据与图例同步 |
| R-12-02 | 时间维度 | 支持今日/近 7 天/近 30 天/自定义；按小时/天聚合 | 时间轴可读；时区明确；空桶显示 0 而不是缺失 |
| R-12-03 | 模型/会话维度 | 按会话模型分组（名称+模型 ID+provider）；可堆叠/并排 | 同模型不同会话可区分；历史模型变化有快照/标注 |
| R-12-04 | Token 指标 | 输入（未缓存）、输出、缓存读、缓存写、推理、总计；请求数、失败/中断数、进行中 | 每项都有定义 tooltip；未知显示“未知”而不是 0 |
| R-12-05 | 图表 | 至少：时间趋势（堆叠柱/线）、按模型占比（环/条）、请求与失败（柱+线）、成本（若价格可得） | 图例可点选；hover 显示精确值；无数据空态；加载骨架 |
| R-12-06 | 刷新 | 手动刷新 + 回合结束/切换房间后自动刷新；请求可取消 | 不闪回旧房间数据；刷新有 loading 状态 |
| R-12-07 | 明细 | 图表点选可下钻到请求/会话/消息；显示时间、模型、输入/输出/缓存、耗时、状态 | 下钻不丢筛选；返回保留状态 |
| R-12-08 | 导出（可选） | CSV/JSON 导出当前筛选；不含密钥/敏感内容 | 导出字段与页面一致；文件名含范围/时间 |
| R-12-09 | 口径说明 | 页面固定说明“来自供应商返回/ACP 台账；未知不等于 0；缓存是明细不重复相加；成本为估算” | 说明可展开；与 DSH 的 `tokenUsage` 四桶口径一致 |

#### 3.5.3 市场参考

- DSH 把 token 拆成 `uncachedInputTokens/outputTokens/cacheReadTokens/cacheWriteTokens` 四个桶，并用 `sessionStats`（turns/steps/llmMs/toolMs/ttft/decode）做时间与速率；UI 是 composer 的两枚药丸 + 点开明细，不是图表（`dsh-token-meter` / `dsh-session-stats` README；`dsh-client-ui-chat/lib/client.js:7603-7638`）。
- Claude Code `/usage` 按模型分组显示 cost/API 时长/墙钟/四类 token；Codex `/status` 显示额度窗口；Copilot 按人发起的 prompt 计 premium request。共同点是“输入/输出/缓存/成本分开，未知不冒充 0”。
- 图表维度（按会话/模型/时段、请求数、失败率、成本）是通用做法；没有产品公开 tokens/sec 字段（R2 报告结论），所以速率需自算（见 3.9）。

#### 3.5.4 待确认

1. 统计范围默认“当前聊天室”还是“当前工程”？用户原话是“当前聊天室的，各会话模型各时段”。
2. 时间粒度默认“按天”还是“按小时”？保留多久的可视化数据（与轨迹老化共用还是独立）？
3. 是否需要成本/价格？价格来自模型目录还是用户手填？
4. ACP/Devin 没有 token 时，图表是显示“未知/不可用”还是用字符估算？估算必须标注。

---

### 3.6 P-10 终端扩展栏与左侧终端快捷栏删除

**现状（已证实）**

- 左栏“更多”里有终端 tab（`index.html:119`）；终端面板在 `index.html:950-968`；`data-window-id="terminal"`。
- 前端：`app.js` 的 terminal 初始化、`terminal-window-*` 动作、`initializeIconControls` 里的 terminal 图标、快捷键/恢复逻辑；`terminal_host.rs` 后端；`styles.css`/`wuxia_layout.css` 的终端样式；`main.rs` 测试断言含 `terminal`。
- 删除终端不涉及数据迁移；但“终端”作为工具能力（模型 tool）与“终端 UI 面板”是两件事，需确认只删 UI 还是也禁工具。

**需求规格（R-10 终端移除）**

| 编号 | 需求 | 验收 |
|---|---|---|
| R-10-01 | 删除入口 | 左栏“更多”不再有终端；`data-window-id="terminal"` 不可达；快捷键/命令面板不再出现 |
| R-10-02 | 删除面板与后端 UI 依赖 | 终端 DOM、JS 初始化、CSS、`terminal_host` 的 UI 路由/恢复逻辑移除或仅保留工具执行所需部分 |
| R-10-03 | 不影响工具执行 | 模型/CLI 的 shell 工具仍可用（若产品保留）；不因删 UI 误删 `bash`/`PowerShell` 工具 |
| R-10-04 | 清理测试/文档 | 删除对应断言/截图/文档入口，避免测试继续要求 terminal 存在 |

**待确认**：只删“终端面板”，还是连“给模型用的 shell 工具”也一起收敛？用户原话是“终端扩展栏删除，左侧的终端快捷栏按键也删除”，倾向只删 UI。

---

### 3.7 P-13 SKILL 扩展栏什么都没有

#### 3.7.1 现状（已证实）

- SKILL 面板：`index.html:970-975`；前端 `app.js:14370` 请求 `/api/extension-market/skills`。
- 后端只扫 `<workspace>/skills` 与 `<workspace>/.coolzhu/skills`（`extension_market.rs:215-218`），要求 `<dir>/SKILL.md` 且恰好两层（`:228-232`）；frontmatter 只取 `name/description`（`main.rs:12892-12922`）。
- 当前工程 `D:\coolzhuagent` 实测返回 3 条 `project:coolzhu-goal-*`；默认工程 `%USERPROFILE%\coolzhuagent` 既无 `skills/` 也无 `.coolzhu/skills` → 在该工程下面板必为 0 条。运行时工程只存内存，无法断定用户当时在哪个工程；也不能排除“有 3 条但不是想要的公共技能”。
- `/api/tools/catalog` 的 skills 分类另走 `skill_source_roots()`，比面板多一个 opencode 根（`main.rs:12798-12843`）；两处共用 frontmatter 解析。
- 缺口：仓库无 create-skill 文件/按钮/接口；`docs/superpowers/` 只有 6 个设计文档，无 SKILL.md；“superpowers”只作为角色提示词字符串（`main.rs:49655-49659`）；不识别 `.claude/.codex/.agents/.opencode`；没有 Skill 工具、没有按需加载。

#### 3.7.2 需求规格（R-13 SKILL）

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-13-01 | 目录可用 | 列表显示名称、描述、来源、启用状态、最近更新；搜索/筛选；空态说明“当前工程没有 SKILL” | 当前工程 3 条可见；空工程不误导为“功能坏了” |
| R-13-02 | 详情 | 打开 SKILL.md 正文、frontmatter、路径、来源、权限/工具依赖、适用模型 | 大文件分段读取；不执行正文 |
| R-13-03 | 启用/停用 | 启用后进入当前会话的 skill 目录；停用后模型不可见；按工程/会话隔离 | 启用后模型可调用；停用后不再出现；不串工程 |
| R-13-04 | 内置 create-skill | 提供“创建 SKILL”向导：名称/描述/触发场景/正文模板/示例；生成 `<ws>/skills/<name>/SKILL.md` | 创建后出现在列表并可启用；非法名称/重复有错误 |
| R-13-05 | Superpowers | 内置/一键安装常用 superpowers 技能集（brainstorming、plans、TDD、debugging、review、verification 等）；显示来源/版本/许可证 | 安装后可见/可启用；离线包可用；不静默联网 |
| R-13-06 | 多根发现 | 支持 `<ws>/skills`、`<ws>/.coolzhu/skills`、`.agents/skills`、用户级目录；优先级明确；只识别一层 `SKILL.md`/`<name>.md` | 多根冲突有优先级；符号链接/嵌套有明确规则 |
| R-13-07 | 安装/来源 | 本地目录、内置包、远程仓库（可选）；安装前显示来源/版本/兼容性；安装后默认停用 | 安装失败可回滚；不覆盖同名；来源不可信提示 |
| R-13-08 | 模型调用 | 模型通过 Skill 工具按需加载正文；目录只注入 name/description；加载结果进轨迹 | 模型能看到目录；调用后正文进上下文；停用即不可用 |
| R-13-09 | 权限 | SKILL 只能读其目录资源；执行脚本需显式授权；与房间权限一致 | 越权读取/执行被拒；审计可见 |

#### 3.7.3 市场参考

- `SKILL.md` + YAML frontmatter 已是跨厂商事实标准；最小可移植子集 `name`（≤64，kebab-case）+ `description`（≤1024），description 决定自动触发（R2 报告）。
- `.agents/skills` 正成为厂商中立共享根；Claude Code/Codex/DSH 都支持多根 + 优先级；DSH 只识别一层、以最近 `.git` 祖先为项目边界，不递归 `**/SKILL.md`（`dsh-skill-filesystem` README）。
- 元数据常驻、正文按需是共识；技能通常不单独建市场，而是打包进插件分发（Claude Code plugin、DSH bundle、zcode 插件）。
- Superpowers（obra/superpowers，MIT）按宿主提供插件市场或 git clone 安装，技能集覆盖 brainstorming→plans→TDD→debugging→review→verification→worktree→parallel agents（R2 报告）。

#### 3.7.4 待确认

1. “常用 create skill 和 superpowers”是**内置随包**、**一键安装**，还是**市场可搜索**？三者体验/维护成本不同。
2. 是否支持 `.claude/.codex/.agents` 兼容根？还是只做 coolzhu 自己的 `skills/`？
3. SKILL 是否需要“市场/远程安装”，还是只做本地目录 + 内置包？
4. SKILL 脚本执行是否允许？如果允许，权限模型如何与房间权限结合？

---

### 3.8 P-14 插件市场点开没有安装按键

#### 3.8.1 现状（已证实）

- 插件市场窗口在 `index.html:978-1013`；DSH 社区目录卡片只有“查看详情与兼容性”（`dsh_market.js:192-204`）。
- 安装按钮只在**详情弹层**里，且被 `setWuxiaIconOnly` 变成无文字图标 + 默认 `disabled=true`（`dsh_market.js:123-126`）；必须先点同样图标化的“检查固定来源”，成功返回 `static_installable:true` 才解禁（`dsh_market.rs:467`）。
- 本地 12 个候选卡反而有安装图标，但排在 DSH 目录之后，用户先看到的是“没有安装按钮”的 DSH 卡。
- DSH 安装链路代码完整：source → install（默认停用）→ enable 走 Node 宿主；但 `GitUrl` 分支 Web 侧不可达（`extension_market.rs:177` 只传本地目录），compatibility 恒 `installable:false`（`dsh_market.rs:611`）。
- 本地原生安装已闭环：install → 事务 → `installed.json` / `settings.enabledPlugins` → `plugin__*` 工具（`plugin_runtime.rs:20-46`）。
- 兼容性：原生 manifest 无平台/架构/宿主版本/依赖/签名；DSH 要求 GitHub 固定 commit、根 `package.json`+`LICENSE`、SDK 锁一致、`services==["tools"]`、工具 1-24（`dsh_source_download.rs:436-452`、`dsh_activation.rs:90-104`）——这是“只能看不能装/装了用不了”的主因。
- 无 501 拦截；`NOT_IMPLEMENTED` 只在发布运维与 MCP 非 stdio 出现。市场实测：DSH 目录 5704 条 / 238 页；插件接口 7 plugins + 12 candidates。

#### 3.8.2 需求规格（R-14 插件安装）

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-14-01 | 详情页安装按钮 | 每个可安装插件在详情主操作区有明确“安装”按钮（文字或图标+文字），不藏在二次弹层 | 打开详情即可看到；禁用时有原因 |
| R-14-02 | 版本/兼容性 | 显示版本、发布日期、兼容的宿主范围、平台/架构、依赖；不兼容给“为什么”与可选版本 | 不兼容不显示可点安装；兼容版本可切换 |
| R-14-03 | 来源/信任 | 显示来源（官方索引/社区/本地）、仓库、commit、哈希；安装前确认“以你的权限运行” | 不可信来源需显式确认；哈希/commit 校验失败拒绝 |
| R-14-04 | 安装状态机 | 未安装/安装中/已安装（停用）/已启用/有更新/安装失败；进度与错误可见；可取消 | 状态与磁盘/运行时一致；重启后保持 |
| R-14-05 | 安装后启用 | 安装默认停用；启用前做兼容性/工具 schema 检查；启用后模型工具列表更新 | 启用即出现 `plugin__*`；停用即消失；失败回滚 |
| R-14-06 | 卸载/更新 | 卸载前停止相关进程/撤销工具资格；更新保留配置；失败可回滚 | 不残留运行中工具；旧版本可恢复 |
| R-14-07 | 本地/远程统一 | 本地候选与 DSH 远程走同一详情/安装/状态 UI；远程 GitUrl 分支必须真正可达或有明确“暂不支持” | 不再出现“本地有按钮、远程没有”的割裂 |
| R-14-08 | 兼容方案 | 原生 manifest 增加平台/架构/宿主版本/依赖/签名（可选）；DSH 按 engines/peer 校验；不兼容给降级/豁免 | 安装前可判定；不兼容不静默安装 |
| R-14-09 | 市场源管理 | 可添加/移除索引源；显示源可信提示；刷新/搜索/分页 | 源失效有错误；不把“列表”当“已安装” |

#### 3.8.3 市场参考

- DSH 插件市场：索引源 URL + 信任提示、列表/详情/版本、兼容性（`engines.dsh` 优先，否则 peer 范围）、安装确认、已安装/有更新、卸载；不兼容版本可“豁免”（R2 报告）。
- 常见“点开没有安装按钮”的原因：市场未添加、来源不可信、哈希/commit 校验失败、版本/平台不匹配、需要登录、只读托管 scope、该表面无安装器、已安装置灰、`defaultEnabled:false`（R2 报告 10 类）。
- VS Code/Obsidian 的共性：详情页主按钮、版本/兼容性、安装后启用/禁用/卸载、失败回滚。

#### 3.8.4 待确认

1. DSH 远程安装是否在近期范围内？如果 GitUrl 不可达，是先做“本地插件市场”，还是补远程下载？
2. 兼容性豁免是否允许用户手动开启？谁承担风险？
3. 插件签名/哈希是否作为强制门禁？现有本地插件没有签名，迁移策略是什么？
4. 安装后“默认停用”是否所有插件一致？是否允许“安装即启用”？

---

### 3.9 P-15 聊天室顶部 token 总量与平均速率（DSH 口径）

#### 3.9.1 现状与 DSH 实现

- 当前聊天顶部工具栏：`index.html:742`，按钮「搜索 / 消息索引」+「思考与运行记录」。用户要求删后者，空出的位置显示当前聊天室总 token 与平均速率。
- 现有数据：`chat_usage_events` 四类 token + `chat_message_timing.elapsed_ms`（整轮墙钟）；没有首 token 时间、没有 ACP token、没有按请求 model。
- DSH 实现（本地 asar 解包）：
  - `tok/s = 整会话 decodeTokens / (decodeMs/1000)`，`decodeMs = Σ(assistant/message.time − firstTokenTime)`；`TTFT = ttftMs/ttftSteps`（`dsh-client-ui-chat/lib/client.js:7494-7496`、`:7363-7369`）。
  - `tokenUsage` 四桶：`uncachedInputTokens/outputTokens/cacheReadTokens/cacheWriteTokens`；`sessionStats`：turns/steps/llmMs/toolMs/ttftMs/ttftSteps/decodeMs/decodeTokens（`dsh-session-stats` README；`dsh-token-meter` README）。
  - 展示：composer dock 两枚药丸——activity = “{轮}轮{步}步 · {tps} tok/s”，usage = “总量 + 缓存命中”；点开才是 TTFT/TPS/模型用时/工具用时/四桶明细（`dsh-client-ui-chat/lib/client.js:7565-7638`）。
  - 口径是**整会话平均**，不是“最近 N 秒”也不是当前轮；`compact` 模式只显示速度/缓存命中。
- coolzhu 当前只能算“整轮墙钟速率”（总 token / 整轮耗时），包含排队、prefill、工具循环，系统性偏低；不能冒充 DSH 的 decode 速率。

#### 3.9.2 需求规格（R-15 顶部 token/速率）

| 编号 | 需求 | 细节 | 验收 |
|---|---|---|---|
| R-15-01 | 删除旧按钮 | 从聊天工具栏移除「思考与运行记录」；轨迹入口迁到任务页/保留深链接；安全徽标迁到权限/诊断处 | 工具栏不再有该按钮；轨迹仍可达；安全状态不丢 |
| R-15-02 | 总量 | 显示当前聊天室“总 token”（建议 = 未缓存输入 + 缓存读 + 缓存写 + 输出）；tooltip 明细 | 与统计页同口径；未知显示“未知”；不把缓存重复相加 |
| R-15-03 | 平均速率 | 显示“平均 tok/s”（建议 decode 口径：Σ输出 token / Σ(完成时间−首 token 时间)）；无法计算时显示“—”并说明 | 不把整轮墙钟速率冒充 decode；有首 token 才显示 |
| R-15-04 | 实时性 | 流式过程中显示“生成中/当前速率”；回合结束更新总 token 与平均速率；切房间重置 | 不频繁抖动；请求失败/中断有状态 |
| R-15-05 | 明细 | 点击打开 popover：输入/输出/缓存/推理、请求数、失败数、首 token 均值、解码耗时、模型/会话分组 | 明细可下钻统计页；口径说明可见 |
| R-15-06 | 状态 | 加载/空/未知/错误四态；空聊天室不显示 0 tok/s；无 usage 的 ACP 显示“用量未上报” | 不伪造数字；错误可重试 |
| R-15-07 | 性能 | 不因统计请求阻塞聊天；增量更新；每轮最多一次聚合 | 打字/流式不卡；切换房间不串数据 |
| R-15-08 | 可配置 | 设置里可选“显示总量/速率/两者/隐藏”；compact 模式只显示速率 | 选择持久化；窄窗不溢出 |

#### 3.9.3 数据缺口（阻断级）

1. **分子**：ACP/Devin 回合没有 token usage；需要 ACP 协议若提供 usage 就读取，若不提供则明确“未上报”，不能用字符估算冒充。
2. **分母**：没有首 token 时间；需要记录 `request_start / first_token / completed` 三个时间点，按 attempt/step 存。
3. **模型维度**：`chat_usage_events` 没有 model/provider 快照；历史模型变化无法还原。
4. **索引**：`created_at` 无索引，按时段聚合会扫表。
5. **聚合接口**：`/insights` 只返回按 session 的标量，没有房间级 total/rate 字段。

#### 3.9.4 市场参考

- DSH 是唯一在本机可核实的“tok/s”展示；口径是 decode-only、整会话平均、composer 药丸 + 明细 popover。
- Claude Code spinner 显示“本轮输出 tokens”，`/usage` 显示会话累计/成本；Codex `/status` 显示额度窗口；Cursor 显示请求/额度。多数产品不直接暴露 tok/s，需要自算；因此 coolzhu 必须明确口径，避免与用户预期不符（R1/R2 报告）。

#### 3.9.5 待确认

1. “平均 token 速率”按 DSH 的 decode 口径，还是按“总 token / 整轮墙钟”？前者更接近模型速度，后者更接近用户等待。
2. 顶部显示“当前聊天室累计”还是“最近一轮/最近 N 秒”？用户原话是“当前聊天室消耗的总 token 数和平均 token 速率”，倾向累计。
3. ACP 不提供 usage 时，是否允许用字符估算并标注“估算”？还是只显示“未上报”？
4. 删除「思考与运行记录」后，安全徽标（待人工复核/输入隔离）放哪？需要单独入口还是并入权限 pill？

---

## 4. 优先级与依赖（决策输入，不是排期）

> 这里的 P0/P1/P2 是“用户可感知影响 × 证据确定性 × 被其它需求依赖的程度”，不是最终排期；最终顺序需用户拍板。

| 条目 | 类型 | 用户影响 | 证据强度 | 依赖 | 建议优先级 |
|---|---|---|---|---|---|
| P-01 启动慢 / workspace 失败 | 问题 | 极高（打不开/等很久/数据错库） | 高（日志/锁/库/代码） | 无；但影响所有其它验证 | P0 |
| P-04 “删除删除” | 问题 | 中（可见错误/不专业） | 高（`setBusy` 代码） | 无 | P0（低成本） |
| P-05 主聊天室权限未知 | 问题 | 高（无法授权/CU 不可用） | 高（接口+竞态） | P-06 权限语义 | P0 |
| P-06 权限模型/默认语义 | 需求 | 极高（安全边界） | 高（存储/执行链） | 产品决策：默认/完全/auto | P0 |
| P-02 工程目录新建/删除 | 需求 | 高（无法组织工程） | 高（缺 API） | 工作区切换/身份 | P1 |
| P-03 聊天室切换卡顿 | 问题 | 高（频繁操作） | 高（全量 save+串行请求） | P-01 数据层 | P1 |
| P-07 健康图标→权限 pill | 需求 | 中（信息架构） | 高（数据源） | P-06 | P1 |
| P-08 定时任务去聊天室 | 需求 | 中（列表污染/概念混乱） | 高（系统室/列表） | 任务页 IA | P1 |
| P-09 任务/轨迹/CU 统一 + 老化 | 需求 | 高（找不到/库膨胀） | 高（表/保留） | P-01 数据层 | P1（老化部分 P0 风险） |
| P-11 设置页/模型接入 | 需求 | 高（核心配置） | 高（UI/协议/生成链） | 产品决策：订阅范围 | P1 |
| P-12 统计图表 | 需求 | 中（看不见用量） | 高（数据为空/ACP 缺口） | P-15 数据采集 | P2（UI 可先做空态） |
| P-15 token 总量/速率 | 需求 | 中高（对标 DSH） | 高（缺首 token/ACP token） | 数据采集 | P2（数据先行） |
| P-13 SKILL | 需求 | 中（能力扩展） | 高（目录/缺口） | 安装/来源策略 | P2 |
| P-14 插件安装 | 需求 | 中（生态） | 高（按钮/兼容链） | 远程下载/兼容策略 | P2 |
| P-10 终端移除 | 需求 | 低（精简） | 高（引用面） | 无 | P2（可随手做） |

**关键依赖链**

1. **P-01 → P-03 / P-09 / P-11 / P-12**：启动/保存的全量重写与锁不解决，切换、任务页、统计刷新都会继续卡；但可以先做“不让慢变成失败”（健康期限/错误提示）与“不让库继续膨胀”（receipts 老化）。
2. **P-06 → P-05 / P-07**：权限语义（默认/完全/auto、dev_open、workspace 外）不定，主聊天室状态机和顶部 pill 无法定稿。
3. **P-15 数据采集 → P-12 图表 / P-09 轨迹费用**：没有 ACP token 与首 token 时间，速率/图表只能显示“未知”；建议先定“ACP 不提供时如何呈现”，再决定是否投入采集。
4. **P-11 订阅范围 → P-13/P-14 的账号/插件边界**：订阅模型是设置页能力还是插件市场能力，会影响插件市场的职责与安装对象。
5. **P-09 老化策略 → P-01 启动**：如果 receipts/轨迹继续无界增长，启动只会更慢；老化是启动治理的一部分，不只是存储清理。

---

## 5. 待确认问题（请用户拍板）

> 下列问题会直接改变需求规格或验收标准；未确认前，相关条目只能做“现状/缺口”分析，不能定稿。

### A. 启动与工作区

**已定**：

1. 启动完成 = **见到开机动画**；冷启动 **≤15s**。
2. 参考库 = 当前 **148.8MB** 库的冻结副本（3 sessions/4 rooms/2035+2365 条消息/5180 条 ACP 回执），5 次冷启动取 P95；不用 552KB 日常库。
3. 启动使用**最近一次 workspace**；同端口只允许一个工作区；并发冲突**关闭旧实例并按最近 workspace 重启**；首次启动默认 `%USERPROFILE%\coolzhuagent`；**不做两套 workspace 启动选择**。

**仍待确认**：

4. **`data_dir` 仓库外**：是正式能力还是仅开发/验收模式？安装版是否禁止？
5. **健康期限**：30s / 60s / 120s？超时后是继续等待（显示进度）还是直接失败？
6. **关闭旧实例的边界**：优雅退出还是允许强制终止？旧实例有运行中对话/定时任务/未保存状态时，默认等待还是弹确认？
7. **“最近 workspace”定义**：以最后一次成功切换为准，还是最后一次成功启动？UI 切换是否立即写记录？
8. **旧 `C:\Users\zhupu\coolzhuagent`**：迁移合并、保留只读历史，还是首次升级时提示删除？
9. **安装器**是否预创建 `%USERPROFILE%\coolzhuagent`？当前 `Product.wxs` 未创建，首次由 launcher 创建。

### B. 权限

**已定**：

7. 默认档读/写 workspace 外 **逐次审批**；CU 默认禁止。
8. `dev_open_permissions` 在权限方案完全开发完成后 **删除**。
9. 自动审批（auto）**先调研市面复杂度**，暂不排期。

**仍待确认**：

10. **主聊天室**：是否允许切“完全访问”？还是固定默认、只有普通房间可授权？
11. **完全访问有效期**：仅本次进程？24h？重启失效？是否需要“撤销”入口？
12. **逐次审批的粒度**：一次读/写算一次，还是同一轮同一路径可合并？审批结果是否只在当前轮有效？谁审批？
13. **删除 `dev_open_permissions` 的迁移**：已有配置/房间权限如何转换？

### C. 设置与模型接入

11. **订阅厂商首批范围（已定）**：只 Devin + OpenAI/ChatGPT；Claude/Gemini 后续；OpenCode Go 先不做。
12. **OpenAI 接入口径（已定）**：ChatGPT 账号 OAuth。仍待确认：OAuth 的 scopes/回调/令牌存储、额度展示、与 API key 通道的切换提示。
13. **图像/视频产物**：进聊天消息卡片、右侧资产页，还是独立“作品库”？是否需要历史作品管理/过期提醒？
14. **思考滑块**：是否允许“关闭”？档位以模型目录为准还是统一 6 档？Devin 变体切换是否也走滑块？
15. **头像**：只允许品牌头像库，还是允许上传自定义？大小/裁剪/存储位置？
16. **Devin 云会话**：保留则放哪？删除则已有云会话数据如何处理？
17. **插件市场三个入口移除后**：OpenCode/HF 的“连接器”是否彻底消失，只留设置页的 API 接入？

### D. 任务、轨迹、老化

18. **`room-scheduled-tasks`**：彻底删除，还是保留为内部室但不在 UI 显示？Goal 镜像还需要 room_id 吗？
19. **历史系统室消息**：迁移到任务页/轨迹的哪个结构？是否保留原消息 id/时间？
20. **任务页 IA**：定时/后台/轨迹/CU 是四个 tab，还是“任务”+“轨迹”两个页面？执行页（execution_panel）是否合并？
21. **12h 窗口（已定）**：轨迹/CU 截图/ACP 回执/后台任务**全部统一 12h**；大小按运行时长估算；不归档。
22. **12h 固定（已定）**：固定 12h，不提供 6/12/24h 可调；是否允许“关闭自动清理”仍待确认（建议不允许，避免再次无界增长）。
23. **清理机制（仍待确认）**：逐条/按 run/按天分区？删除时是否保留“已清理”索引？被引用的 CU 证据是否随 run 一起删？

### E. 统计与速率

24. **统计范围**：当前聊天室 / 当前工程？时间粒度按天/小时？保留多久？
25. **速率口径**：DSH 的 decode 口径（输出 token / 生成时间），还是总 token / 整轮墙钟？顶部显示累计还是最近一轮？
26. **ACP 无 usage**：显示“未上报”，还是允许字符估算并标注“估算”？
27. **成本/价格**：是否展示？价格来自目录还是手填？

### F. SKILL 与插件

28. **create-skill / superpowers**：内置随包、一键安装、还是市场搜索？是否需要远程仓库？
29. **兼容根**：是否支持 `.claude/.codex/.agents/.opencode`？还是只做 `skills/`？
30. **SKILL 脚本执行**：允许吗？权限如何与房间权限结合？
31. **DSH 远程安装**：是否近期范围？GitUrl 不可达时先做本地市场还是补下载？
32. **兼容性豁免/签名**：是否允许用户手动豁免？签名/哈希是否强制？

### G. 终端

33. **只删 UI 还是也禁 shell 工具**？用户原话倾向只删“终端扩展栏 + 左侧快捷栏”。

---

## 6. 证据索引与复现

### 6.1 分析报告（本轮新增，均在 `tmp/analysis-20261017/`）

| 报告 | 内容 |
|---|---|
| `A-startup.md` | 启动时序、慢点、锁、workspace 失败矩阵、最近提交、日志缺口 |
| `B-rooms-permissions.md` | 工程目录、切换、删除文案、主聊天室权限、权限作用域、健康图标 |
| `C-settings-models.md` | 设置页现状、多余文本 T1-T19、三个插件、接入矩阵、媒体链、思考、头像、保存 |
| `D-schedules-trace-terminal.md` | 定时任务房间、schedules 页 DOM 搬运、后台子 Agent、轨迹/CU 表、保留、终端引用 |
| `E-usage-token-rate.md` | 统计面板/DOM/接口、token 记账、ACP 缺口、DSH 公式、按钮迁移、字段缺口 |
| `F-skills-plugins.md` | SKILL 扫描/面板为空、create-skill/superpowers、插件详情按钮、安装链、兼容性 |
| `R1-market-permissions-models.md` | Codex/Claude/Cursor/Devin/Gemini/OpenCode/Copilot/DSH 权限、订阅、OpenRouter、图像/视频、思考档位 |
| `R2-market-skills-plugins-usage.md` | SKILL 生态、Superpowers、插件市场/安装/兼容、token 口径、轨迹留存 |

### 6.2 关键源码位置

| 主题 | 位置 |
|---|---|
| 启动 main / bind 顺序 | `modules/gui-web/packages/web-console/src/main.rs:1307-1495` |
| SessionStore 加载/首存 | `main.rs:37755-38138`、`:38009-38100` |
| 全量 save | `main.rs:50165-50521` |
| 权限接口/存储 | `main.rs:16879-16896`、`:42141-42146`、`:42612-42622`、`:28898-28909` |
| 房间列表/系统室 | `main.rs:253-257`、`:38056`、`:38364-38375`、`:40419-40479` |
| 定时任务投递 | `scheduled_delivery.rs:10-63`、`main.rs:10541-10554` |
| 轨迹接口 | `main.rs:1877`、`chat_insights.rs:275-288` |
| 统计/用量 | `main.rs:1876`、`chat_insights.rs:8-45`、`request_usage.rs:60-184` |
| ACP 无 usage | `devin_acp/session.rs:344-351`、`devin_acp/journal.rs:53-58` |
| 设置页 | `model_settings.js:54-113`、`:353`、`:475`；`chat_experience.js:330-343`、`:396-414` |
| 插件市场 | `index.html:978-1013`、`dsh_market.js:123-204`、`dsh_market.rs:467/611`、`extension_market.rs:103-111/177/215-232` |
| SKILL | `index.html:970-975`、`app.js:14370`、`extension_market.rs:215-232`、`main.rs:12798-12922` |
| 终端 | `index.html:119/950-968`、`app.js:10907-11290/19340-19362`、`terminal_host.rs:367-376` |
| 健康图标 | `index.html:91-104`、`app.js:7460-7561/12259-12278`、`wuxia_layout.css:77-78` |

### 6.3 运行/数据证据

- 本机 `127.0.0.1:8765` 只读 GET：`/api/workspace`（workspace=`D:\coolzhuagent`）、`/api/chat/rooms`（main-room / room-scheduled-tasks / 2 个普通房）、`main-room/permissions`（workspace-write、`dev_open_permissions=true`、effective full-access）、`main-room/insights`（usage=[]）、`main-room/trace`（runs=[]）、`/api/sessions`（3 个会话）。
- `tmp/installed-cold-start-132/receipt.json`、`133`、`identity-observed.json`；`%LOCALAPPDATA%\CoolzhuAgent\logs\package-launcher\package-selfcheck-last.json`。
- `%TEMP%\coolzhu-session-lock.log`（21,134 次等待，峰值 29,554ms；held 92,684ms）。
- SQLite：`tmp/installed-cold-start-133/web-sessions-before.sqlite3`（111MB 备份，表级大小见 3.1/3.3）；当前 `D:/tmp/.coolzhu/web-sessions.sqlite3`（148.8MB）。
- 截图目录：2,638 文件 / 1.06GiB；`computer-use` 192 文件 / 74.5MB。
- DSH 解包：`tmp/analysis-20261017/dsh/`（`dsh-client-ui-chat`、`dsh-session-stats`、`dsh-token-meter` 等）。

### 6.4 界面规划草案

- `docs/design-assets/2026-10-17-settings-redesign/A-settings-api.png`
- `B-settings-subscription.png`
- `C-settings-generation.png`
- `D-extension-ia.png`
- `wireframes.html`（可编辑源）
- `wireframes-full.png`（总览）

### 6.5 明确未确认

1. `receipt.json` 的“正式服务工作区或会话库错误”判据来自未入库的外部验收脚本；具体比较字段未确认。
2. `err.log` 串库（`D:\` 与 `C:\Users\zhupu\`）成因未确认；`rolling_file` 丢行使日志不能当完整审计。
3. `execution_index` 两库均 0 行：是未使用、未回填，还是运行版本与源码不一致，未确认。
4. `goal_phase` 运行行两库均 0：系统室轨迹空态是“没数据”还是“没写入”，未确认。
5. 安装版截图与当前源码的 UI 差异（7 个会话 vs 4 个房间）未逐项对照构建身份。
6. 启动 save 峰值 46–93s 的精确归因（并发写/杀软/磁盘/WAL checkpoint）未用 profiler 证实。
7. ACP 协议是否本身提供 token usage、只是本仓库没读，未确认。
8. 图像/视频生成的真实厂商兼容性/额度未逐模型实测。
9. DSH 的 tok/s 口径是在其打包产物中核对，未在 DSH 源码仓库逐行复核（但字段/公式一致）。
10. 用户截图中的“权限未知”是当前 dev 实例还是安装版实例，未绑定构建身份。

---

## 7. 一句话总结

这 9 条提示里，**真正阻断使用的是 3 件事**：启动被全量写回/锁/库膨胀拖到健康期限之外，workspace 身份在不同启动方式下不一致，权限状态机把“有效完全访问”显示成“目录权限/状态未知”。其余是信息架构与能力闭环问题：定时任务占聊天室、轨迹/CU/统计/后台没有统一页面与老化、设置页信息重复且接入模型过窄、SKILL/插件有目录无安装闭环、token 速率缺少分子分母。

已确认的收敛口径：**开机动画 ≤15s、同端口单工作区且冲突切已有实例、默认档外目录逐次审批（dev_open 最终删除，auto 待调研）、订阅首批 Devin+OpenAI、运行轨迹保留约 12h 且不归档**。下一步应基于这 5 条决策进入“修改方案”设计，而不是再扩大问题范围。
