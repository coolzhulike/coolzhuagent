# 聊天轮次时延：根因、风险评审与落地方案

> 状态：**评估稿（2026-11-03）**，待确认 §10 决策点后实施。
> 范围：web-console（`modules/gui-web/packages/web-console`）会话库读写路径、流式回复路径、自动记忆沉淀；PR #106 逐项复核。
> 证据：`docs/testing/chat-latency-20261103/`（原始输出 + 可复跑脚本）。行号仅作定位，实施以函数名为准。

## 1. 结论摘要

1. **"hi 慢"的主因是存储介质，被代码的连接使用方式放大，与模型、记忆算法、前端框架都无关。**
   - 本机 `D:` 是 **TOSHIBA External USB 3.0 外置机械盘**；`C:` 是 NVMe SSD。真实工作区 `D:\tmp` 的会话库（`.coolzhu/web-sessions.sqlite3`，142MB）就放在 `D:`。
   - 同一个 release 二进制（v0.2.143）、同一份库副本、同一个模型 `qwen3.8-flash`、同一条流式路由、各测 3 次：

     | 指标 | USB 机械盘（D:） | NVMe（C:） |
     |---|---|---|
     | SSE 首字节（用户看到"已开始"） | **42–78s** | **0.14–0.16s** |
     | 首个模型字出现 | 46–89s | 2.3–2.5s |
     | 整轮完成 | 115–204s | 3.0–3.6s |
     | 本地开销（接纳→发出模型请求） | 35–60s | 0.12–0.14s |
     | 模型首 token（发出请求→首 token） | 7–12s | 2.1–2.4s |
     | 生成阶段（首 token→结束，<100 token） | **26–69s** | 0.5–0.9s |

2. **放大器是"每次读写都新开连接"**：`src/*.rs` 中 `open_session_connection(` 约 335 处（含定义和测试），每次都开 2 个连接（只读预检 + 主连接）、跑 PRAGMA 和 9 条 schema 校验，然后用完即关。机械盘上一次打开约 0.4s，SSD 上约 7ms。按 HDD 本地开销（35–60s）与单次代价估算，一轮要打开数十次；B1 会加计数器实测。
3. **流式阶段被同步写盘拖慢**：流式回复期间每 800ms 执行一次全量 `store.save()` 来落盘"进行中"的部分内容。这次落盘在 SSE 生成器里同步执行（占用 tokio 工作线程），同时持有全局会话锁。机械盘上每次 1–3s，等于在读模型流的过程中反复停顿：生成不到 100 个 token 用了 26–69s，SSD 上同样的回复不到 1s。
4. **SSD 上 "hi" 一轮约 3s，其中约 2.3s 是模型首 token**（6.4k 输入 token 的 prefill + 思考）；本地开销约 0.12–0.14s。所以只要不在机械盘上，现有记忆架构和前端加载**都不是瓶颈**，不建议整体替换（见 §6）。
5. **要做的**：把数据目录迁到 SSD（零代码，立刻见效），并修掉三个热路径反模式：连接复用、流式期间不同步落盘、记忆沉淀移出响应路径。做完以后，即使数据留在机械盘上，每轮本地开销的目标也是 ≤1s（§8）。

## 2. 实测证据

环境：Windows，Defender 实时防护开启（两组测试一致）。release 二进制 `target/release/coolzhu-web-console.exe`（2026-11-01 构建，= v0.2.143，不含 PR #106）。会话 `qwen-3.8 flash`、房间 `room-todo-live`、路由 `/api/chat/send/stream`。每次间隔 35s，避开 30s 幂等窗口。

### 2.1 单轮分段（摘自 `bench-base-release*.txt`）

| 轮 | 盘 | accepted→run.started | 用量行 created / started / first_token / finished | SSE 首字节 / 首 delta / 结束 | 本轮 save 次数 |
|---|---|---|---|---|---|
| 1 | HDD | 29.5s | +53.9 / +60.5 / +72.9 / +141.8s | 77.7 / 89.3 / 204.3s | 17 |
| 2 | HDD | 20.4s | +40.8 / +47.0 / +58.0 / +84.3s | 56.6 / 63.4 / 115.5s | 8 |
| 3 | HDD | 14.0s | +30.3 / +35.0 / +41.9 / +67.9s | 42.2 / 46.2 / 122.2s | 8 |
| 1 | SSD | 42ms | +121 / +135 / +2518 / +3426ms | 0.16 / 2.54 / 3.57s | 5 |
| 2 | SSD | 47ms | +110 / +119 / +2282 / +2869ms | 0.14 / 2.30 / 3.01s | 5 |
| 3 | SSD | 46ms | +123 / +130 / +2265 / +2778ms | 0.15 / 2.28 / 3.01s | 5 |

- 单次 save：HDD 1.0–3.0s（其中仅"打开连接 + schema 就绪"就占 0.3–1.3s），SSD 10–16ms。
- HDD 上 save 次数随流式时长增加（17 次 vs 5 次）：存盘越慢 → 流越长 → 800ms 节流点越多 → 存盘次数越多，形成正反馈。
- 启动：HDD 4.2–23.9s 开始监听、21.1–36.7s 初始化完成（两次启动）；SSD 0.5s / 1.0s。
- 回复内容生成完到整轮完成（context_outputs→completed，含最终持久化与记忆沉淀）：HDD 13–27s，SSD 约 60ms。

### 2.2 SQLite 单项操作（`analysis-sqlite-ops.txt`，复刻 `open_session_connection` + `initialize_session_schema`，中位数 ms）

| 盘 | synchronous | 打开+schema | 新开连接后提交 | 复用连接提交 |
|---|---|---|---|---|
| USB HDD | FULL | 389 | 409 | 162 |
| USB HDD | NORMAL | 452 | 244 | **0.1** |
| NVMe | FULL | 6.9 | 2.2 | 0.7 |
| NVMe | NORMAL | 6.6 | 1.5 | 0.1 |

结论：机械盘上真正贵的是**每次新开连接**（冷读 WAL 索引和 schema 页）和 **FULL 级别的 fsync**。复用连接再加上 `NORMAL`，同一块机械盘上单次提交也能降到 0.1ms。

### 2.3 库内容本身不大（`analysis-db-profile.txt`）

消息两张表合计约 9MB / 4.2k 行，记忆 bead 4 条。主房间按索引全量读取 2104 行只要 18ms（Python 侧）。**SQL 本身不慢，慢的是每次读写的连接建立和 fsync。**

### 2.4 前端打开聊天室时调用的接口（`bench-frontend-*.txt`，中位数）

| 接口 | HDD | SSD |
|---|---|---|
| `/api/sessions` | 4542ms | 26ms |
| `/api/chat/rooms/{id}/messages?limit=80`（135KB） | 533ms | 3.8ms |
| `/handoffs` / `/permissions` / `/todos` | 631 / 777 / 1122ms | 15 / 16 / 5ms |
| `/api/chat/rooms`、`/roster`、`/api/pet/state`（纯内存） | ~1ms | ~1ms |
| `/`（130KB 内联前端） | 34ms | 6ms |

前端已经分页（每次 80 条），并通过 `history-events` 做 SSE 增量同步。慢的接口都会开 sqlite 连接，纯内存接口在两块盘上都约 1ms。**前端加载方式不是瓶颈。**

## 3. 根因链

```
单次 I/O 代价（介质）  ×  每轮 I/O 次数（代码）  ×  放大器（同步/持锁/阻塞运行时）
 HDD 0.4s/次打开          几十次打开 + 5–17 次全量 save    流式期间持锁同步 save → 拖慢读模型流
 SSD 7ms/次打开                                           SSE 响应头要等接纳完成才发出
```

| # | 反模式 | 位置 | HDD 影响 | SSD 影响 |
|---|---|---|---|---|
| A | 每次读写新开 2 个连接 + schema 校验，用完即关 | `open_session_connection`（335 处）、`save_session_state_to_sqlite_with_epoch` | 每次 0.4s，一轮几十次 | 每次 7ms |
| B | 流式期间每 800ms 同步全量 `save()`，持全局会话锁，阻塞 tokio 工作线程 | `upsert_room_live_message` → `live_upsert_in_store` | 生成被拖到 26–69s | <50ms |
| C | `prepare_chat_dispatch` + `accept` 在返回 SSE 之前同步执行 | `api_chat_send_stream` | 首字节 42–78s | 0.15s |
| D | 自动记忆沉淀：每条 bead 一次全量 `save()`，持锁，在 HTTP 返回之前执行 | `persist_auto_memory_beads_at_path` → `add_memory_bead` | 计入回复后 13–27s 的收尾 | ~10ms/条 |
| E | 同步 sqlite 调用直接跑在 async 函数里 | 各 handler | 一个慢 I/O 占住一个工作线程，连带其它请求 | 不明显 |
| F | `save()` 每次对全量内存态做 `==` 深比较，成功后再整份 clone | `SessionStore::save_with_config_operation` | 次要 | 当前规模下次要（save 合计 10–16ms） |

## 4. 对此前判断的更正

| 此前说法 | 实测更正 |
|---|---|
| "80–113s 主要是 debug 构建慢，release 会快 10 倍左右" | **错**。release 在同一块 USB 盘上首字节仍要 42–78s。决定性因素是磁盘，不是构建档位。 |
| "测试环境 toml 缺 `[pet]` 段，所以桌宠 spawn 了" | **错**。隔离副本的 toml 和真实 `D:\tmp\coolzhu.toml` 完全相同（哈希一致），都是 `pet.enabled=false`，两边**从未 spawn**。那 13.5s 是我手动冷启动一个放在 USB 盘上的 87MB exe 测出来的，不在产品路径上。 |
| "`HostModelSnapshot::capture` 是接纳块 ~30s 的主体" | **错**。SSD 上整个接纳（accepted→run.started）只要约 40ms。HDD 上的十几秒来自其中的连接打开和写盘。 |
| "<4 字短查询跳过召回能省 5–15s" | SSD 上整段本地开销只有 0.12–0.14s，这项**收益不可测**。而且 3 个汉字的查询（"继续做""改回去""怎么办"）是有意义的，存在漏召回风险（见 §5）。 |

## 5. PR #106 逐项复核

| 改动 | 实测收益 | 风险 | 建议 |
|---|---|---|---|
| `synchronous=NORMAL` | HDD 新开连接后提交 409→244ms；与连接复用叠加后 162→0.1ms | WAL+NORMAL：进程崩溃不丢数据；**整机掉电可能丢最后几个已提交事务，但不会损坏库** | **保留**。在发布说明中写明掉电语义 |
| 接纳两事件合并为一个事务 | 每轮少一次打开+提交（按 §2.2 估算 HDD 约 0.6s） | 低：同一个 run 的事件顺序不变 | 保留 |
| 嵌入共享 Client + 3s 超时 | 端点不可达时快速回退（本机 8081 未监听，本来就是秒拒） | 低：远端慢嵌入服务超过 3s 会回退到 hash | 保留 |
| `<4` 字跳过召回（semantic + structured 两处） | 不可测 | **中**：按字符计数，3 个汉字的有效指令会漏召回 | **撤回**；连接复用后召回的 sqlite 开销自然消失 |
| 桌宠后台线程派发 + 默认关闭 | 你的环境下为 0（配置本来就是关） | 低：未配置 `[pet]` 的新装用户不再自动拉起桌宠 | 待你决定（§10-2）；建议保留默认关，不删功能 |

## 6. 架构评估与市面方案对比

| 层 | 现状 | 市面常见做法 | 判断 |
|---|---|---|---|
| **记忆写入** | 回复之后、HTTP 返回之前，**同步**按规则抽取并写 bead | LangMem：后台"潜意识"抽取（`ReflectionExecutor` 防抖），文档写明热路径写入会"增加可感知延迟"；Mem0 平台：`async_mode` 默认 `true`，add 立即返回 event_id、后台处理；Letta：sleep-time agent 异步整理记忆 | **采用它们的模式，不换系统**：沉淀改为后台写入队列（§7 B3）。换成 Mem0 / Letta 会引入额外的 LLM 抽取调用和外部服务，反而增加成本和时延 |
| **记忆检索** | bead 列表在内存中；`memory_vectors` 存在 sqlite；向量暴力余弦检索 | sqlite-vec / LanceDB（嵌入式 ANN）、Qdrant（服务） | **不换**。当前 4 条 bead，SSD 上召回耗时是毫秒级。数据量到约 5 万向量以上再评估 sqlite-vec |
| **会话持久化** | 全量状态驻留内存，每次 `save()` 与上次提交做 diff 后回写；每次新开连接 | 聊天类应用的常见做法：只追加消息表（一条消息一次 INSERT）+ 长连接 / 连接池 + 单写线程（actor / `tokio-rusqlite` 式）+ 分页读取 | **渐进改造，不换库**：先做连接复用（B1，收益最大、改动集中），再把热路径改为定点写（B4）。SQLite 在 SSD 上单次 save 10–16ms，不是瓶颈 |
| **流式进度** | 进行中的内容每 800ms 落一次库 | 内存态 + SSE 广播；终态才落库（或由后台低频合并写） | 改为内存态，后台合并写（B2） |
| **前端加载** | 分页 80 条，`history-events` SSE 增量同步 | 同类做法 | **不换**。SSD 上所有接口 <30ms |

## 7. 落地方案（分批，每批独立 PR）

### B0 运维：数据目录迁到 SSD（零代码，立刻见效）
- 依据：§2.1，同一个二进制首字节 42–78s → 0.15s。
- 步骤（需要你执行，或明确授权我执行，因为会动真实数据）：
  1. 完全退出 CoolzhuAgent（确认 `coolzhu-web-console` 进程已结束）。
  2. 把 `D:\tmp\.coolzhu` 整个复制到 SSD，例如 `C:\CoolzhuData\tmp-workspace\.coolzhu`（保留原目录作为备份）。
  3. 在 `D:\tmp\coolzhu.toml` 的 `[paths]` 中设置 `data_dir = 'C:\CoolzhuData\tmp-workspace\.coolzhu'`。代码 `resolve_config_path` 支持绝对路径；会话库、附件目录、审计日志都在 data_dir 之下。
  4. 启动后核对：聊天记录完整，日志中 `[SESSION-SQLITE] read start` 显示的是新路径。
- 风险：路径写错会被当成"新库"打开，看起来像空历史（原库不受影响，改回配置即可）。还需要确认 `session.db_path` 没有单独覆盖。
- 附带建议：`target/` 也在 USB 盘上，这是一次 debug 构建要 154 分钟的主要原因。可设置 `CARGO_TARGET_DIR` 指向 SSD。

### B1 连接复用（P0，收益最大）
- 改动：
  - 新增 `session_db` 连接管理：每个库路径一个**长驻写连接**（与 `session_writer` 的进程级写锁绑定），外加一个小型读连接池（2–4 个）。
  - 每个进程、每个路径只做一次 schema 校验（迁移完成后标记）。之后借出连接时只检查 `PRAGMA schema_version`（读一页，命中缓存）。
  - 写连接设置 `cache_size=-16000`（16MB）。
  - 先迁移热路径：`save_session_state_to_sqlite_with_epoch`、`append_runtime_run_events`、run 的建立/启动/终态、用量台账、`chat_todos`、记忆加载（superseded / access / vectors）、`/api/sessions`。
- 验收：每轮打开连接 ≤3 次（新增计数器，写入轮次诊断）；HDD 上单次 save ≤50ms；SSD 上数据不退步。
- 风险与缓解：
  - 长连接残留未结束的语句或事务，会持有 WAL 读快照导致检查点饿死、WAL 膨胀 → 归还连接时断言 `is_autocommit()`，禁止跨 `.await` 持有事务，空闲时执行 `wal_checkpoint(PASSIVE)`。
  - 切换工作区后误用旧库 → 连接按路径分键，切换时整体丢弃。
  - 其他进程（更新版本的实例 / 安装器）修改了 schema → 借出时比对 `schema_version`，变化即重建连接并重新校验；现有"版本超前拒绝打开"的语义保留。
  - 单元测试依赖每次新开连接的隔离性 → 测试中按临时路径分键，自然隔离。

### B2 流式期间不同步落盘（P0）
- 改动：`upsert_room_live_message` 只更新内存中的 live 表，并通过已有的 `history-events` 推送。后台任务每 5s 合并写一次（`spawn_blocking`、不持全局会话锁跨越 I/O），终态 / `CommitPending` 时强制写一次。`history-messages` 和 `messages` 接口读取时合并内存 live 表。
- 必须保留的语义：注释 `CommitPending 不调用本函数：终态未确认时保留唯一可见的部分结果`——终态提交失败时，部分内容仍要可见。由"终态强制写 + 内存 live 表"保证。
- 验收：HDD 上 `first_token→finished` 与 SSD 的差距在 1.5 倍以内；流式期间 save 次数 = 0。
- 风险：进程崩溃会丢失最近 ≤5s 的进行中内容（当前是 ≤800ms）。孤儿 live 行下一轮本来就会被 `sweep_orphan_room_live_messages` 清掉，所以实际可恢复价值很低。

### B3 SSE 先回响应头 + 记忆沉淀移出响应路径（P0）
- 改动：
  - `api_chat_send_stream` 先返回 SSE 并推送 `accepted`，把 `prepare` + `accept` 放进流内的 `spawn_blocking` 执行。
  - 409（幂等重复）/ 404（房间不存在）等改为 SSE `error` 事件，前端按同一条错误路径展示。
  - `persist_auto_memory_beads` 改为回复完成后投递到后台写入队列，**一批 bead 一次事务**。参照 LangMem / Mem0 / Letta 的做法（§6）。
- 验收：HDD 上 SSE 首字节 ≤300ms；`run.completed` 不再包含记忆沉淀耗时。
- 风险：
  - 前端和第三方调用方依赖 HTTP 状态码 → 非流式 `/api/chat/send` 保持不变；流式错误事件要补前端契约测试。
  - bead 晚 1–2 秒可见，下一轮可能还看不到上一轮的 bead → 写入队列按会话串行；队列积压时，下一轮召回前等待（上限 200ms）。
  - 进程退出时队列未写完 → 退出钩子 flush；未写入的 bead 下次启动可由 `memory_jobs` 补抽。

### B4 热路径定点写（P1）
- 改动：追加消息 / 更新消息 / 新增 bead 改为单行 INSERT/UPDATE，不再走全量 diff 的 `save()`。`save()` 只保留给配置类变更。同时去掉每次 save 的全量 `==` 比较和 clone。
- 风险：内存态与库不一致 → 每个变异入口一律"先写库成功、再改内存"，并补一致性回归测试（重启后对比）。
- 触发条件：B1–B3 完成后，如果 HDD 上每轮仍有 >200ms 花在 save 上，再做。

### B5 存储健康提示 + 时延可观测性（P1）
- 启动时探测数据目录：可通过 Windows `IOCTL_STORAGE_QUERY_PROPERTY` 判断 USB / 机械盘，或测一次 fsync 延迟。命中时在 UI 顶部提示"数据目录位于外置/机械盘，聊天会明显变慢"，并给出迁移指引（B0 的步骤）。
- 每轮记录 `local_overhead_ms`、`sqlite_opens`、`store_lock_wait_ms`、`saves` 到 run 事件，前端"耗时"面板展示，作为长期回归指标。

### 不建议做
- 把 SQLite 换成 Postgres / RocksDB / LanceDB：SSD 实测 SQLite 不是瓶颈，迁移风险大，收益没有证据支撑。
- 把记忆层换成 Mem0 / Letta / Zep：需要额外的 LLM 抽取调用或外部服务；只借鉴它们的异步写入模式。
- 重写前端：SSD 上接口都在 30ms 以内，分页和增量同步已经到位。

## 8. 验收指标与基准方法

- 脚本：`docs/testing/chat-latency-20261103/bench-start.ps1`（启动并等待初始化完成）、`bench_hi.py`（流式路由 3 轮）、`bench_frontend_api.py`、`perf_sqlite_ops.py`。每批 PR 都在 **HDD 和 SSD 两份库副本**上各跑一次，前后对比。
- 目标（"hi"，模型 `qwen3.8-flash`）：

| 指标 | 现状 HDD | 现状 SSD | B1–B3 后目标 HDD | 目标 SSD |
|---|---|---|---|---|
| SSE 首字节 | 42–78s | 0.15s | ≤0.3s | ≤0.2s |
| 本地开销（接纳→发出模型请求） | 35–60s | 0.13s | ≤1s | ≤0.15s |
| 生成阶段膨胀（HDD/SSD，同序号轮次比） | 45–76× | 1× | ≤1.5× | 1× |
| 每轮打开连接次数 | 几十次 | 同 | ≤3 | ≤3 |
| 前端 `/api/sessions` | 4.5s | 26ms | ≤100ms | ≤30ms |

- 测量纪律：测前确认没有残留的探针进程持有库文件；同一时间不跑编译；幂等窗口外再发；模型侧耗时（started→first_token）单列，不计入本地开销。

## 9. 风险登记

| ID | 风险 | 批次 | 等级 | 缓解 |
|---|---|---|---|---|
| R1 | NORMAL 下整机掉电丢最后几个事务 | #106 | 低 | 不损坏库；发布说明写明 |
| R2 | 长连接持有读快照导致 WAL 膨胀 | B1 | 中 | 归还时断言 autocommit；空闲时 PASSIVE 检查点；监控 WAL 大小 |
| R3 | 外部进程修改 schema 后长连接失效 | B1 | 中 | 借出时比对 `schema_version`，变化即重建 |
| R4 | 崩溃丢失 ≤5s 的进行中内容 | B2 | 低 | 终态强制写；孤儿行原本就会被清扫 |
| R5 | 流式错误从 HTTP 状态码改为 SSE 事件 | B3 | 中 | 非流式路由不变；补前端契约测试 |
| R6 | bead 延迟可见 / 退出时丢失 | B3 | 低 | 按会话串行；召回前限时等待；退出 flush；`memory_jobs` 补抽 |
| R7 | 定点写导致内存与库不一致 | B4 | 中 | 先写库后改内存；重启一致性回归 |
| R8 | 迁移数据目录时路径配置错误 | B0 | 低 | 保留原目录；改回配置即恢复 |
| R9 | 撤回短查询跳过 | #106 | 低 | 无可测收益，撤回无损 |

## 10. 需要确认的决策点

1. **B0 由谁执行？** 我建议你确认后由我执行（先完整复制，不删原库），或者你按 §7 B0 的步骤自己操作。
2. **桌宠**：你的配置本来就是关，对性能无影响。建议 #106 保留"默认关闭 + 后台派发"、不删功能；如果仍要整体删除，请明确告知。
3. **#106 撤回 `<4` 字跳过召回**：建议撤回（§5）。
4. **实施顺序**：建议 B1 → B2 → B3（各自独立 PR，每个 PR 都附 HDD/SSD 双盘基准），B4 / B5 视 B3 后的实测再定。
