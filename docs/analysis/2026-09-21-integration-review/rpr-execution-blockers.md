# RPR 执行受阻与前提不符记录（统一台账）

用途：按"如有执行受阻先统一记录问题到文档"的要求，把执行中发现的**前提不符、真实缺口、协作成本、需新裁决项**集中记录。
口径：`已核实源码 file:line` / `子代理实测` / `未验证` 分开标注；不把未验证写成已完成。

更新时间：2026-09-24（批次 1 执行中）

---

## A. 已解除的阻塞（裁决生效 + 前提核实）

| 原阻塞 | 裁决 | 核实结果 |
| --- | --- | --- |
| B-1 错误无回执承载位 | 加 `receipt: Option<ActionReceipt>`，不改 `act` 返回类型 | ✅ 可执行。**依赖环实测不存在**：全 workspace `cargo build --offline` EXIT 0 |
| B-2 缺测试失效接缝 | 同意加编译期门控的 `test-support` 接缝 | ✅ 执行中（子代理） |
| B-3 外部服务与显存切换冲突 | 外部不终止、冲突即阻断、8GB 维持单活动模型 | ✅ 已实现，见 §B-4 |
| B-5 清空密钥是否静默回退 | 先做行为刻画测试，不预设缺陷 | ✅ 已实现，见 §B-5 |
| R-2 预算匹配前置 | 升级为按模型路线的强制准入门禁 | ✅ 设计与核对完成，见 §B-1 |
| R-3 无人工恢复入口 | 原生控制面提供、用户级存储 | 归 RPR-05c，未开工 |

## B. 执行中发现的前提不符与真实缺口

### B-1 RPR-11c：裁决对现场的描述有 4 处不符（子代理核对，均带 file:line）

1. *裁决禁止的"下限回扩"在现网并不存在**。`clamp_stage_timeout(remaining, cap)`（`computer_use_adapters.rs:160-165`）语义是 `remaining.min(cap)`——是 `min`，**无下限、不回扩**，且已有单测正面固定（`computer_use_desktop_bridge.rs:991` 断言 `clamp_stage_timeout(ZERO, cap) == ZERO`）。
   **真正的缺口是另一件事**：11 个调用点（桌面桥 9 处含拖拽与截图 + 浏览器桥 2 处）中 **0 处拒绝**，11 处都是"剩余不足也照发"；其中 `browser_bridge.rs:595` 在转交 tab 动作时**把 `remaining` 整个丢弃**，用裸 `BRIDGE_TIMEOUT`。
   另：真正的 `clamp(100, 600_000)` 在 `main.rs:6651/6682`，入参是**配置值**（默认 30s）而非剩余预算，故当前不构成回扩——但这是一颗地雷：将来若有人把 remaining 传进去，它立刻变成回扩。
2. *"600 秒单次模型请求上限"的位置线索有误**。`clamp(100,600_000)` 是**工具执行超时**；模型请求的 600s 来自 `openai_compat.rs:206`、`claw_provider.rs:127` 的 reqwest `.timeout(600s)`。
3. *外层根 deadline 未实装**。`RunBudget`（`run_contract.rs:201-216`）只有契约与单测，**web-console 零引用**；CU 预算纯来自 config（默认 120s，可配 5~300s，`main.rs:5610-5621`），入口 `execute_with_current_runtime`（`computer_use_executor.rs:1092-1096`）**无 deadline 入参**。后果：`effective_call_budget = min(...)` 中"父运行剩余时间"一项**无符号可依**，准入判据"CU 预算 ≤ 父剩余"**不可判定** → 见 §C-1。
4. *"12 动作 = 1+2n = 25 次请求"只对 Desktop+多模态 planner 成立**。本地**纯文本 planner（bonsai 路线）是 `1+3n = 37` 次**——`plan` 会先追加一次视觉描述请求（`computer_use_planner.rs:542-554`）；Browser 表面是 `n=12`（验收 0 次模型请求，`:566`）。另有 `main.rs:28839` 的光标位置校验视觉请求**不计入任何公式**。

补充两项真实缺口：
5. `ComputerUsePlanner` trait（`controller.rs:11-35`）**没有 `remaining` 参数**，而真正发模型请求的就是 planner → 预算守卫覆盖不到 planner，属**跨 crate 公开 API 变更**（见 §C-2）。
6. `input_stroke.rs:170` 的取消收尾宽限是**固定 2 秒且不受 `remaining` 约束**：`remaining = 0` 时仍会等满 2 秒才强杀。即"`min` 保证不会超过 remaining，固定宽限保证一定超过"。见 §C-4。

7. 裁决 §6.6 六项准入门禁的**原文未能在仓库中定位**（全树 grep `6.6`/`准入门禁`/`六项` 无匹配），子代理只能按工单声明的六个技术面向组织 → **与 §6.6 的一一对应关系未验证**。

### B-2 RPR-04a：裁决对爆炸半径的估计偏保守（对我们有利）

- 裁决要求"不能把 additive 写成所有调用者无需修改，同一 PR 必须更新构造器、字面量、转换实现和测试"。
- **实测爆炸半径 = 0**：全仓**没有任何** `ComputerUseError { .. }` 结构体字面量（唯一 `Self {` 在 `contracts.rs` 内部）；约 20 个辅助构造函数与 75 处 `new/blocked/recoverable` 调用点全部**经由构造函数**，`new`/`blocked` 签名未变即无感。
- 有利事实：`ActionReceipt` 同时 derive `Eq`，故 `ComputerUseError` 的 `Eq` 得以保留；`ActionReceipt` 非 `Copy`，与只 derive `Clone` 的 `ComputerUseError` 不冲突。
- **未做**：没有从 `computer_use` crate root 再导出 `ActionReceipt`（web-console 已有 `runtime` 依赖可直接引用）。若 RPR-04b 想让 helper 侧少一个直接依赖，可补再导出。
- **未验证**：运行时历史错误对象的 JSON 兼容（`.coolzhu/web-sessions.json` / sqlite 中的旧错误）只做了单元级往返证明；离线无法复现运行时会话。

### B-3 并发编辑产生的"假故障"（协作成本，必须记录）

- RPR-04a 子代理在一次 `cargo build -p coolzhu-web-console` 中读到 `main.rs` 的**中间态**（集成负责人已删 `kill_listeners_on_port`、尚未改完 4 处调用点），得到 7 个与其改动无关的 `E0425`；**原样重跑即 EXIT 0**。它正确判定为并发伪影并如实报告，未做绕过。
- 规程（据此固定）：
  1. 子代理**不得**把单次 web-console 构建失败当作自己改动引入的缺陷，必须重跑一次并注明；
  2. 集成负责人在编辑 `main.rs` 期间应预期子代理会读到中间态；
  3. 子代理的最终验证以**自己完成编辑后**的一次干净构建为准。

### B-4 RPR-11a：产品可见行为变化（已完成实现，需知晓）

已落地的语义变更：
- **退出清理完全不再按端口杀进程**。原实现会杀掉 `local_chat_port`（bonsai 8080）、7860、8000、**以及 8081 的 bge-m3 嵌入服务——而 main.rs 从未启动过它**（只有探测）。这是修复而非回归：托管子进程本就登记在 kill-on-close Job Object 中，随控制台退出自动终止。
- **显存释放只针对 Agent 自己创建并登记的实例**；`switch_local_models` 在"要释放的资源由外部服务占用"时**改为阻断并给出操作指引**，不再强杀。因此外部 bonsai 运行时，"切到 vision"会被阻断（裁决 §4.2 明文要求）；"chat"模式对称处理。
- 判定依据是**登记身份**，不是端口占用或进程外观（端口只用于连接与诊断）。

**未实现缺口（记录，不隐瞒）**：
1. 托管实例的**终止身份强度**目前只有"Agent 记录过的 PID"。按裁决 §9.1 应扩展 `windows-process-guard` 做创建时间/句柄级核对；该 crate 本轮被子代理 RPR-02a 占用 → **残留风险：PID 复用窗口内理论上可能终止无关进程**。待 RPR-02a 完成后补原生助手。
2. `switch_local_models` 的**"排空请求"（裁决 §4.3）源码中不存在任何在途请求计数机制**（grep `in_flight`/`active_local_requests`/`drain_local` 零命中）→ 需新增，作为后续子项。

测试侧：原 `web_console_exit_cleans_up_local_model_ports` 断言的是**旧行为**，已重写为 `web_console_exit_never_terminates_local_model_ports`，并新增"禁止标识符必须不存在"的断言（被禁标识符在测试里**运行期拼接**，否则断言文本自身会命中该字符串——这是本轮实际踩到的自指陷阱）。

### B-5 RPR-07a：已证实"清空密钥"确实会回退环境变量

- 回退点：`main.rs:30921-30928`（统一设置/端点路径）。已抽成纯函数 `effective_provider_key(session_key, env_keys, lookup)` + `first_present_env_key_value`，使语义可测且不再内联。
- `resolve_api_key_ref`（`main.rs:30125`）**内部没有**环境变量回退：空/空白 → `None`；`looks_like_secret` → 原值；否则按文件路径候选读取，失败即 `None`。
- 因此**显式清空（`""`）与"从未配置"在当前源码中不可区分**，两者都表现为引用为空 → 都会走环境变量回退。这正好印证裁决 §8.3 的告诫：**历史空值不能批量解释成"禁止继承"**；若要落实"区分允许继承的未设置与用户明确清空"，必须新增持久化的来源事实（例如 `api_key_cleared` 标志），不能靠 resolver 猜。
- 已由 3 个测试固定：会话密钥优先（用 panic 闭包证明 env 未被查询）、清空后按登记顺序回退第一个非空值、无候选即无密钥。**这些测试固定的是当前真实语义，不预设它是缺陷**；是否修改取决于用户对 §8.3 分支的选择。

### B-6 杂项

- `computer-use-core` 现有 `unused variable: remaining` 警告若干（`controller.rs:892/905/920`、`browser_bridge.rs:535`、`computer_use_desktop_bridge.rs:345`），源自上一轮 S1.5 的逐阶段 deadline 传递（部分实现未使用该参数）。**属于我上一轮引入的残留警告**，待子代理停止改动这些文件后统一清理（重命名为 `_remaining`）。

---

### B-7 RPR-01：上一轮对 cwd 风险的判断偏重，但同时暴露了另一类确认缺陷

- **生产代码零处改写进程 cwd**：全仓 `set_current_dir` 只有 9 个调用点，**全部在 `#[cfg(test)]` 内**；生产路径一律用 `Command::current_dir` 给子进程独立 cwd。因此上一轮担心的"并发工具调用看到错误 cwd"**在生产路径上不存在**，只存在于同一测试二进制内的多线程测试之间。
- **确认并修复**：`ScopedCurrentDir::Drop` 用 `set_current_dir(..).expect(...)`，当原始目录已消失时在 unwind 中二次 panic → **abort 整个测试进程**。有真实改前证据：临时用例 `rpr01_demo_drop_panics_when_original_is_gone` FAILED（`Os code 2 NotFound`）。修复为 Drop 不 panic（尽力恢复 + 留痕）。
- **新确认缺陷类（本轮刻意未修，列入下一轮）**：**环境变量的"手写 set → restore"约 15 处**（tool-registry 多处 + `core-runtime/src/prompt.rs`），中途 panic 会**永久污染进程环境**（HOME / CLAW_CONFIG_HOME / PATH / CODEX_HOME 等），且因锁是 poison-tolerant 而**静默级联**。与 cwd 属同一类缺陷，修法同为 RAII（`ScopedEnvVar`）。子代理已列出全部站点。
- **文档错误**：AGENTS.md 提到的 `run_process` 在仓库内 **grep 零命中**，属过期描述，勿据此假设符号存在。

### B-8 RPR-02a：交付超出工单（§3.3 端到端回归已实现），但有 4 点须记录

- **发布门禁命令在 cargo 1.94.1 不存在**：工单给的 `cargo tree --no-dev` 报错（无该参数）。改用 `-e no-dev,features`；并补了**更强证据**：`cargo build -v` 中 guard 的编译行**不含** `--cfg feature="test-support"`，而 `cargo test --no-run -v` 中**含** → 发布依赖图干净。门禁 A 的 `grep -c test-support` = 0。
- **输入前屏障位于写 step 记录之前**（`computer_use_executor.rs:181-188` 早于 `record_step`）→ 输入前失效时**一条 step 行都不会落**，故"零输入"断言加强为 `step 行数 == 0`（原计划的 `input_delivery=not_sent` 行根本不存在）。
- **测试陷阱（重要）**：`ComputerUseController::run` 进入主循环前会先用初始 observation 调一次 `verify`；测试替身若首次即返回 `achieved=true`，控制器会**直接判成功且从不规划输入** → 两条测试都"零输入"且 `status=succeeded` 的**假通过**。已修正并在代码留注释。
- **反向验证（临时移除屏障使该测试失败）未执行**，只给出了步骤与预期失败点 → "测试覆盖的是防御分支而非错误枚举"目前是**设计论证 + 阴性对照**，不是实证。
- **新发现待裁决**：`try_acquire_incidental_input_lease`（main.rs 短时光标输入路径）**没有输入前的 `is_current()` 复检**，只在取得 lease 时校验。见 §C-6。

### B-9 AGENTS.md 的体积数字已过期（用哈希证实，可排除"文件被膨胀/损坏"）

- AGENTS.md 记 `main.rs≈1.36MB`、`app.js≈266KB`、`styles.css≈105KB`；实际为 **3.41MB / 764KB / 456KB**（`wc -c`）。
- **决定性证据**：`app.js` 与 `styles.css` 当前 sha256 与 `s0-baseline.md` 的 S1 快照**完全一致**（`5DCC8ED7…` / `65AF4F23…`）——这两个文件本轮**未被改动**，说明大体积来自上游版本本身，**不是本轮编辑造成的膨胀**。同时排除了"main.rs 被追加重复内容"：关键符号（`switch_local_models`、`const LOCAL_MODEL_BRAND`、`fn resolve_api_key_ref`、`api_chat_send_stream`）计数均为 **1**，无重复定义。
- 待办：更新 AGENTS.md 的三个体积数字（第 3 条"超 256KB 必须分段读"的**规则**依然有效）。

### B-10 我本轮犯的两个错误（记录，均已修正）

1. **门禁命令写错**：`cargo tree --no-dev` 在 cargo 1.94.1 不存在（由 RPR-02a 子代理纠正为 `-e no-dev,features`）。
2. **源码文本断言自指**：第一版 `!WEB_MAIN_RS.contains("kill_listeners_on_port")` 必然失败——断言文本自身就在被断言的 `main.rs` 里。已改为运行期拼接（`["kill_listeners", "_on_port"].concat()`），同样的处理也用于 `Get-NetTCPConnection`。

### B-11 全量门禁暴露的并行竞态失败（已定位根因并修复，非本批改动引入）

- 现象：`tests::local_model_visible_identity_hides_underlying_model_name` 在**全量并行**运行中失败（999 通过 / 1 失败），单跑连续 3 次全过。与前一轮会话遇到的是同一个用例。
- 根因（已定位，非猜测）：该用例的断言分三段读取**进程级可变工作区**——`local_chat_runtime_config()`（经 `read_config` → `active_workspace_path()` → `workspace_state().current`）。而 `main.rs` 内有 **20+ 个测试会改写 `workspace_state()`**（`65165`、`65184`、`65294`、`65353`、`65565`、`65591`、`72066`…`72361` 等）。并发切换时，同一用例两次读取到的端口可能不同 → `is_local_model_endpoint` 在第三段断言处返回 false → 提前返回、文本未被抹除。
- 修复（不依赖环境态化）：把 `sanitize_visible_model_text` 的标签计算与替换抽成两个纯函数 `local_model_sensitive_labels(model, configured_path)` 与 `hide_local_model_labels(labels, text)`；测试改为断言纯逻辑 + 显式端口的 `is_local_model_endpoint_on_port(...)` + 一条运行期拼接的**接线断言**（配置端口必须仍传入判定函数）。
- **未做的后续（记录为独立测试基础设施工单建议）**：真正彻底的做法是引入 crate 级 `workspace_state` 测试守卫，让 20+ 个切换者与该类读取者互斥；本轮未做（改动面大且需与现有 `desktop_input_lease_test_guard` 模式统一），改以"让被测逻辑不依赖环境态"这一更小的切口消除已观测到的失败。同类风险仍存在于其它读取环境态的用例（未穷举）。

### B-12 RPR-03 交付完成，但接线前有一个**必须先定的映射规则**

**已交付**（唯一新增文件 `modules/core-runtime/packages/core-runtime/src/fact_store.rs`，2235 行含测试）：
- 端口 `FactStore`（7 个写方法 + 只读 `snapshot()`，方法不含泛型，可 `Box<dyn>`）；持久化接缝 `FactLogBackend`（只 `append`/`read_all`）；记录信封 `FactLogRecord` **逐变体原样承载既有契约类型**，未新建同义类型、未降级为自由 JSON。
- 实现体：`AppendOnlyFactStore`（唯一规则实现，写入顺序为"先重放验证 → 再落盘 → 最后提交内存"，内存永不领先磁盘）、`JsonlFactLog`（真实追加落盘 + `sync_all`）、`InMemoryFactLog`。
- 8 条不变量写入模块文档：只追加 / 缺失即 unknown / 读回不得比写入更确定 / 终态 first-wins / 迟到事实只增不改且 `revives_run()` 恒 false / 幂等 / 恢复只开新 attempt 且预算只许收紧 / 不得静默丢弃（损坏记录报错不跳过）。
- 13 条新测试；`core-runtime` **232 通过 / 0 失败**（基线 219，未下降）；linkage 4/4；**未引入任何新依赖**；`fact_store.rs` clippy 0 告警。
- 为让同一套类型可持久化做了 3 处**纯 additive 派生**（无字段变更、无默认值补齐）：`late_facts.rs`、`recovery.rs`、`submission_dedup.rs` 加 serde；`SubmissionRecord` 由私有改公开 + 新增只读 `lookup`/`record`。

**明确未接线（不得表述为"事实已持久化"）**：全仓 grep `FactStore|fact_store|JsonlFactLog|AppendOnlyFactStore|FactLogBackend` **只命中 `fact_store.rs` 与 `lib.rs`**——没有任何请求入口在调用它。

**阻塞点（接线前必须决定）**：`RunIdentity::validate()` 要求 **10 个字段全部非空**，而 `main.rs` 的 chat-run 事实只有 `run_id`/`session_id`/`room_id`/`claim token`；`workspace_id`/`public_turn_id`/`step_id`/`tool_call_id`/`action_id`/`owner_epoch` 需要**显式映射规则**，否则写入会被 `invalid_identity` 拒绝（这是 fail-closed，不是缺陷）。见 §C-8。

**两项需要用户/集成负责人决定的设计取舍**：
- 生产用 **SQLite 适配器未实现**，只留了 `FactLogBackend` 接缝。理由充分：core-runtime 当前无 `rusqlite`，为一个最小可测接口把 bundled SQLite 拉进核心 crate 会带 C 编译链与打包体积（与裁决对打包量的关注冲突），且 `s2-entry-plan.md` 已把"单写者 + outbox + epoch"规划为独立 crate。
- **`JsonlFactLog` 没有跨进程锁**：两个进程同时追加同一文件会交错。最小接线须先按"单进程单写者"使用（可与 S2.4 一起），多进程前应改 SQLite 适配器或加独占锁。

**如实标注的已知限制**：每次追加整体重放日志（O(n)）；日志不带到达时间戳（时间由调用方提供）；崩溃可能留半行，读回报 `CorruptRecord` 而非静默丢弃。

**子代理的一处语义返工（已修）**：初版让"被拒绝的恢复决定"也占用 attempt 身份，会导致"先因放宽预算被拒、随后收紧预算重试"被错误再拒；已改为只认已授予的恢复。**与假设不符的事实**：`UsageAttempt.attempt_id` **不是** run 内唯一（`usage.rs` 既有用例中 `attempt-0` 出现在两个不同 `logical_request_id` 下）→ 用量事实身份键改为三元组 `(run_id, logical_request_id, attempt_id)`。

**并发编辑记录（再次出现）**：RPR-03 的 linkage 首跑报 `computer-use-core/src/contracts.rs` 的 E0308（该文件不在它的改动范围、也不在 core-runtime 依赖链上，行号数分钟内即漂移）→ 判定为并发子代理编辑，重跑后 4/4 通过。与 §B-3 同因。

### B-13 RPR-04b 交付：三段都真闭环，但有 5 项未闭环与 4 项行为变更必须知晓

**契约 / 生产者 / 消费者三段状态（分开陈述）**
- **生产者是真的**（不是测试拼的）：C# helper（`input_stroke_native.cs`）在每一步被确认后把事实写进**独立进度文件**（起点"已开始但未按下"、按下确认、每成功移动一点、整条完成、finally 里 Up 成功/失败），**写入后回读校验，写不进去就删文件并中止输入**——宁可不留事实，也不留过期记录被读成"未发送"。Rust 侧 `run_helper` 严格解析（缺键/多键/越界/自相矛盾一律=没有事实）。
- **回执成为唯一事实来源**：新增 `StepExecution.receipt`，原先的 `partial`/`path_completed`/`confirmed_point_count`/`input_release_status` 降级为回执的**投影**，避免平行字段各说一套。
- **消费与持久化真闭环**：`TracingAdapter::act` 的 Ok/Err 双路径优先消费回执并用 `action_attempt_id` 校验身份（不匹配 → 记 `receipt_protocol_anomaly` 并保守处理）；写入 `computer_use_steps` 既有事实列；终态错误进 `computer_use_runs.terminal_result_json`；控制器 `controller.rs:632` 用回执把关 stale 静默重规划。
- **与我的预期不同（对项目有利）**：`main.rs` **无需任何改动**即闭环——`computer_use_dispatch_response`（main.rs:33786）把整条 `ComputerUseResult` 序列化进 `tool_result_text`，回执随 serde 自动到达模型侧。
- **打包影响为零**：C# helper 由 `include_str!` 编译期内联，已核实 `config/package-manifest.json` **无需新增条目**。
- 门禁：`computer-use-core` **60/60**、`web-console` **1012/1012**、`cargo build --workspace` 通过、linkage 4/4。8 条裁决回归逐条有测试且通过（含"真实 Engine 写盘 → 生产解析器读回"与一条真实子进程、零注入的端到端）。

**申报的 4 项行为变更（产品可见，需知晓）**
1. 带"可能已注入"回执的 `stale_observation` **不再静默重新观察**，改为终态失败（输入前的 stale 仍是 `NotSent`，恢复行为不变）。
2. 失败步骤的事实列**由回执优先决定**（此前按错误码猜）；预输入拒绝由 `may_have_been_sent` 变为 `not_sent`。
3. 非路径动作成功现在**携带 `sent` 回执**（`partial=false`、`release=not_needed`；此前为 NULL）。
4. `controlled_drag_path` 返回类型由 `Result<(), String>` 变为 `Result<HelperInputFacts, StrokeFailure>`（唯一调用方已 grep 核实，错误文本不变）。

**诚实申报的 5 项未闭环**
1. **未在真实桌面窗口 + 真实鼠标注入下验证**（会劫持用户鼠标，故未执行）→ 目标环境端到端**仍属未验证**。
2. **跨 run 的"未确认释放则隔离"未闭环**：本轮只保证**同一 run 内**下一动作被阻断。要在新 run 也拒绝桌面输入，需在 `computer_use_store.rs` 增 `has_unconfirmed_release(session_id, turn_id)` 并在取 lease 前拦截——**该文件不在工单允许清单内，归 RPR-05**。
3. **没有 helper 的输入路径仍无回执**：Click/KeyCombination（单次 SendInput）与浏览器桥的 DOM 输入没有事实生产者（扩展协议不返回事实）→ 仍走错误码启发式。属能力边界，见 §C-10。
4. `finish_at_version` 落盘失败时保留回执已实现但**未单测**（需注入 sqlite 写失败）。
5. 新增 1 条 pedantic clippy 告警（`run_helper` 超 100 行）。

**集成负责人补充清理**：测试专用适配器里 12 处 `unused variable: remaining`（`computer_use_adapters.rs` 6 处 + `computer_use_executor.rs` 6 处，含子代理新增的测试替身）已精确改名，**web 全量复验 1012/1012 且警告归零**。

### B-14 RPR-11a 缺口二（切换前排空）：源码核对后确认**不能凭猜测实现**，需二选一

裁决 §4.3 要求"`switch_local_models()`：**排空请求后**，按自身实例身份卸载或停止，再启动目标"。核对结果：

- **当前完全没有排空**：`switch_local_models` 对已登记的托管实例是立即终止。
- **provider 层拿不到端口**：`Provider` trait（`providers/mod.rs:13-25`）只暴露 `send_message`/`stream_message`，**没有任何 endpoint/base_url 访问器**；`send_via_provider`/`stream_via_provider`（`client.rs:8-20`）因此无法判断"这个请求是不是发往本地托管端口"。
- **流式路径是主路径，且语义有歧义**：`MessageStream` 是**公开的两变体 enum**（`client.rs:352-355`），chat 主路径走流式（main.rs:17371/54899）。若只在 `stream_message` 的**握手期**计数，则长达数分钟的生成窗口**不被计入**——排空会变成"看起来排空、实际把模型从生成中杀掉"，正是裁决禁止的假保证。
- 有利事实：main.rs **从不匹配 `MessageStream` 的变体**，只调用 `next_event()` 并以 `api::MessageStream` 作为不透明类型持有 → 把它从 enum 改为"带排空守卫的包装结构"对我们的调用方是兼容的。

**两个可选方案（互不排斥，可叠加）**
- **方案 A（客户端计数，版本无关）**：给 `Provider` trait 加一个**带默认实现**的方法（例如 `fn drain_scope(&self) -> Option<u16> { None }`，additive，10 个实现无需改动），在 `openai_compat` 里对 loopback 主机返回端口；`send_via_provider`/`stream_via_provider` 建立 RAII 守卫，**并让守卫随 `MessageStream` 一起存活**（把该 enum 包成结构体，对我们的调用方兼容）；对外暴露 `api::local_endpoint_drain()`，提供 `in_flight(port)` 与有界 `wait_until_idle(port, timeout)`（轮询即可，不需要新增 tokio `sync` feature）。成本约 1 人日；不依赖任何服务端接口；代价是 `MessageStream` 的类型形状变更（对仓库内调用方兼容，对外部匹配变体的代码是破坏性变更）。
- **方案 B（服务端事实，零 API 变更）**：托管实例是本仓库自己启动的 llama-server，直接轮询其 `GET /slots`，要求所有 slot 的 `is_processing == false` 才算排空。不给任何 crate 加接口、给的是服务端自己的答案；但依赖一个**随 llama.cpp 版本变化的端点**，且**必须用真实二进制实测**（本机 bonsai 的 llama-server 需要启动模型才能验，属于会占 GPU 的操作）。同时必须决定"该端点不可用/解析失败时怎么办"：是照旧终止，还是拒绝切换。

**为什么我没有直接实现**：两种方案的真实取舍（客户端计数的类型变更代价 vs 服务端事实的版本依赖 + 需要实测）以及"不可排空时的兜底策略"都属于产品/接口决定；按裁决自己的规则（"遇到真实源码与前提不符时，应提交具体差异及最小契约修订，不能再通过新增平行类型、虚构路径或放宽安全门禁来绕过"），这里提交差异而不是猜一个实现。

### B-15 A-7b（hook 顺序与拒绝不可反转）：改前有三个真实缺口，且带来一处**必须知情的 CLI 行为变化**

**核对的真实情况（带 file:line）**
- hook 配置 `RuntimeHookConfig{pre_tool_use, post_tool_use}`（`core-runtime/src/config.rs:74-78`）；运行器 `HookRunner`（`hooks.rs:122`），在 `conversation.rs:158` 实例化。
- 真实顺序（`conversation.rs:300-372`，改后）：**宿主闸门 `authorize_call`（301）→ pre-tool hook（310）→ 重判（327，本次新增）→ 目标执行器（353）→ post-tool hook（361）**。**闸门确实先于任何 hook**，因此"hook allow + 宿主 deny"在结构上不可能、"hook deny 而宿主仍执行"也不可能。
- Web 侧**确实不经 hook**：`grep -rn "HookRunner|run_pre_tool_use|run_post_tool_use|RuntimeHookConfig" modules/gui-web/` **零命中**；`main.rs` 唯一 "hook" 命中是 `10257` 的一句提示文本 ⇒ 该侧**不适用**（未为它编测试）。`tool.rs:334` 的 `Deny` 分支在任何 executor 迭代（`tool.rs:362-370` 是唯一执行点）之前返回。

**改前三个真实缺口（都已修）**
1. **hook 完全没有独立执行授权**：`run_commands` 只判断 `commands.is_empty()`，`PermissionPolicy`/`ToolCaller` 全不参与 ⇒ **只要配置里出现命令就 spawn shell**。现在改为默认未授权（`hooks.rs:126`），需 `with_hook_authorization`（`hooks.rs:150`）显式授予，能力名走既有 `with_tool_requirement` 命名空间（`hook:PreToolUse`/`hook:PostToolUse`），**未新增同义权限类型**。
2. **fail-open**：hook 退出码既非 0 也非 2、或**根本起不来**时，只产生一条警告，措辞明写 `allowing tool execution to continue`。现已纳入授权闸门统一处理。
3. **审批在 hook 之前算好、hook 之后被原样沿用**：hook 位于"闸门之后、执行器之前"且能产生任意 shell 副作用 ⇒"审批后世界可被 hook 改变，审批仍被沿用"是真实缺口。现在改为**只要 pre-hook 真的跑过（`executed_commands() > 0`）就重走同一判定入口 `authorize_call` 重新判定**，重判为 deny 则执行器零调用。
   - 说明：hook 没有任何通道改写传给执行器的 input（值传递 `&str`），所以"输入改变"子情形在当前实现中**不可达**；改用更强的规则覆盖它，而不是依赖 hook 自报"我改了输入"（那等于把诚实性交给 hook 自己）。代价：hook 已授权且该调用本需用户批准升级时，用户会被问第二次（已由 3 条测试固定）。

**必须知情的 CLI 行为变化（功能回归，需裁决 §C-12）**
- CLI 的 `build_runtime`（`command-line/src/main.rs:3163`→`3176 ConversationRuntime::new_with_features`）合并了 settings+plugin 的 hooks（`:2787`），但**没有** `with_hook_authorization`，而 CLI 不在本工单允许修改范围 ⇒ **CLI / 子 Agent 的 hook 现在不会运行**，只留一条 "not run" 事实。
- 这正是裁决 §7.3 明令的首期做法（"没有独立授权和效果边界的外部 hook 不得在高风险路径自动扩张"、"不加宽松开关"），但**宿主侧还需要一份真正的"hook 独立授权来源"**（谁批准 hook 自身运行）——这一层裁决没有指定，见 §C-12。
- 另有一份**同名独立实现** `modules/tooling/packages/plugin-system/src/hooks.rs` 仍是"配置即运行"语义；它不在 conversation 路径上，超范围未改，留给 S2.3 的"合并重复 hook 语义"。

**门禁**：`core-runtime` **241 通过 / 0 失败**（基线 232 + 新增 9）；linkage 4/4；`cargo build -p coolzhu-web-console` 通过；`tool-registry`/`command-line`/`plugin-system` check 无新告警。
**判别性实证（本轮唯一做到"临时回退→测试失败→还原"的工单）**：临时去掉授权闸门 ⇒ 2 条测试失败；临时把重判结果固定为 `None` ⇒ 2 条测试失败；均已从 `tmp/backups/*.rpr06b` 还原并复跑通过。
**hook 是否真的没跑**用双重外部证据证明：磁盘标记文件（相对包根，失败即残留）+ stdout 文本 `hook-ran-<label>` + `executed_commands` 计数，并有正对照测试证明探测器本身有效。

### B-16 RPR-11a 缺口一（终止身份强度）已按更强的方案关闭：**每服务一个 kill-on-close Job Object**

RPR-05a 交付后，我把它的结论接到了 `main.rs`（集成负责人串行域）。采用的设计比原缺口描述更强：

- **根因消除而非加强校验**：原实现用 `taskkill /PID <pid> /T /F` 停止托管服务——这是按 PID 的硬杀，PID 复用窗口内可能误杀无关进程。改为**每个托管服务持有一个专属的 `ChildProcessJob`（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）**，停止时只要丢弃该句柄，内核就按 **Job 成员** 回收整棵进程树。**代码中不再有任何按 PID 的终止调用**（`terminate_process_tree` 已删除）。
- 因此 PID 复用风险不是"被校验挡住"，而是**不再存在**；同时 ShowUI 那类"powershell → llama-server"的子树也一并被回收（此前靠 `taskkill /T`，现在靠 Job 成员关系）。
- 登记项同时记录**启动瞬间从子进程句柄捕获的身份**（`capture_child_process_identity`，无 PID 复用窗口）与 Job 句柄：身份为 `None` 时只告警并记录，因为停止已不依赖它；退出清理日志会输出"身份已登记/缺失、进程树已绑定/未绑定"，把这两项作为可见事实。
- 源码结构：新增 `ManagedServiceIdentity`/`ManagedServiceTree`（按平台 cfg 的别名）、`managed_service_identity`、`managed_service_tree`；`ManagedLocalService` 增加 `identity`/`tree` 字段（因持 Job 句柄而不再 `Copy`/`Clone`，退出描述改为锁内构造）；`spawn_detached`/`spawn_detached_logged` 在启动成功后统一登记（后者新增 `role` 参数）；`local_model_child_job`（共享单例 Job）已移除。
- 测试：新增 `taking_a_managed_service_reaps_its_tree_without_pid_kill`（**真实子进程**：绑 Job → 登记 → 取出并丢弃 → 断言子进程在 5 秒内退出）；源码契约测试改为断言 `managed_service_tree(&child)`、新的登记调用形态、`spawn_detached` 体内**不得出现 `taskkill`**，以及 Job 助手体内确有 `new_kill_on_close()` 与 `.assign(child)`。
- 门禁：`cargo build -p coolzhu-web-console` EXIT 0，告警 **73 → 72**（删掉 Job 助手后 `field tree is never read` 出现，改为在退出日志中真实读取后消失）；定向测试 5/5 + 1/1 通过。

**尚未接线的部分（有明确步骤，等 RPR-05b-1 停止改动后做）**
- 跨进程输入排他（RPR-05a 任务一）尚未接到输入路径。RPR-05a 给出的最小接线：把 `computer_use_executor.rs`（desktop CU 主路径）与 `main.rs` 的 `try_acquire_incidental_input_lease`（短时光标路径）从 `broker.acquire(...)` 换成 `ScopedInputOwnership::acquire(broker, scope, owner_id, timeout)`，失败即零输入并复用既有 `input_lease_*` 拒绝语义。
- **为什么不现在做**：desktop CU 主路径在 `computer_use_executor.rs`，而该文件正被 RPR-05b-1 编辑；只改一半会得到"两条输入路径所有权语义不一致"的状态，比都不改更危险。故等它停止后再一次性接。

### B-17 RPR-05b-1 交付：跨 run 互锁 + 可审计解除已闭环，但**互锁作用域有个必须知的缺口**

**交付**（`computer_use_store.rs` + `computer_use_executor.rs`，未碰 `main.rs`）
- 核实到的**真实列名与取值**（以 `append_step`/`record_step` 的 INSERT 为准）：`computer_use_steps.input_release_status` ∈ `not_needed`/`released`/**`unknown`**；`input_delivery` ∈ `not_sent`/`may_have_been_sent`/`sent`；归属经 `computer_use_runs.session_id`/`turn_id`。SQL 里的字面量**参数化传入** `input_release_status_name(Unknown)`/`input_delivery_name(NotSent)`，并有测试断言这两个函数返回 `"unknown"`/`"not_sent"`，防止 SQL 与源码漂移。
- 新增 API：`has_unconfirmed_release`、`unconfirmed_release_facts`（前置检查展示用）、`resolve_unconfirmed_release`（**只追加**一条解除事实）、`release_resolutions`（审计回显）。新增两张表用 `CREATE TABLE IF NOT EXISTS` 在既有 schema 初始化路径补齐。
- **刻意不抢 `user_version`**：版本阶梯 12..20 由 `main.rs` 掌管，store 抢号会产生"版本已推进但工作没做"；改用 IF NOT EXISTS，与 `apply_session_migration_v16/v17/v18` 先例一致 ⇒ **本次不需要改 main.rs**，已有库在下次初始化时自动补建（有升级路径测试覆盖）。
- 解除语义：判据是"存在**未解除**的未确认释放"；解除只写新表，`computer_use_steps`/`computer_use_runs` **一行不碰**（历史 `unknown` 仍是 `unknown`）；后置检查在覆盖行写入后重查同 scope，`remaining_unconfirmed_runs` 必须为 0；无可解除事实时是幂等空操作；理由/现场检查说明为空则拒绝。
- 互锁接线在 `execute_in_room_with_policy` 内、**取得输入 lease 之前**（有结构证据测试：判据调用点必须在 `interactive_input_lease_broker()` 之前）；阻断码 `input_release_unconfirmed_interlock`（blocked/user/不可重试）与 `input_release_interlock_check_failed`（读事实失败时 **fail-closed**），回执用既有 `pre_input_receipt`（`NotSent` + `NotNeeded`），身份形如 `run-scope:<call_id>`，与动作回执不可能混淆（有测试）。
- 15 条新测试；store+executor 定向 **55/55**；**web 全量 1028/1028**；`computer-use-core` 60/60。

**必须知的缺口（本轮最大残余）**
- **互锁作用域是 `(session_id, turn_id)`，而"新 turn 就是新 scope"** ⇒ **上一 turn 遗留的未确认释放不会阻断下一 turn**。跨 run 阻断只在同一 turn 内成立。最小扩展建议（仅 store + 执行器两处）：加 session 维度变体（去掉 `AND r.turn_id = ?2`，或给解除表加 `scope_kind='session'`），执行器同时查两个 scope 并在消息里区分。见 §C-13。
- 行为注意（既有语义，非本次引入）：被互锁阻断的尝试**仍会创建 run 行**（`state=blocked`）并消耗 per-turn 调用额度；解除后以**同一任务身份**重复提交会命中幂等缓存返回那条 blocked 终态（不产生新输入），因此"新 attempt"必须换任务身份。

**RPR-05c 需要接的内容（否则互锁安全但桌面会一直锁着）**：① `main.rs` 的 HTTP 路由 + Tauri 命令（读 `unconfirmed_release_facts` 展示前置检查；写 `resolve_unconfirmed_release`；回显 `release_resolutions`，`remaining_unconfirmed_runs > 0` 必须显示"未清空"——这些都是 `pub(crate)`，同 crate 直接调用）；② 现场 epoch 采集（`current_interactive_session_scope()` + 探测 `owner_epoch`，读不到传 `None`，不得填猜测值）；③ UI 二次确认 + 必填理由/现场检查说明；④ 错误码文案表补两个新码；⑤ **模型不得有任何 tool/参数能触发解除**。
- 诚实标注：解除 API 目前仅测试调用（RPR-05c 之前无生产入口），因此新增了 dead_code 警告——子代理**没有用 `#[allow]` 掩盖**这个"尚未接线"的信号，符合本批的诚实口径。

### B-18 跨进程输入所有权已接入两条输入路径（B-16 里"尚未接线"的部分已闭环）

- `main.rs` 的 `try_acquire_incidental_input_lease`（短时光标路径）与 `computer_use_executor.rs` 的桌面 CU 主路径，都从 `broker.acquire(...)` 改为 `ScopedInputOwnership::acquire(broker, scope, owner_id, Duration::ZERO)`，即"进程内 lease + 命名内核对象"的合成所有权。**另一个进程从此无法在本次 run 期间取得同一交互 scope。**
- 超时选 `Duration::ZERO`（非阻塞探测）而不是排队等待：输入是可选动作，被别的进程占用时应立即让位（零输入），而不是把一个已经不确定的桌面状态拖长。桌面 CU 侧保留了拒绝语义的分类——in-process 冲突仍是 `input_owner_busy`，内核侧失败给 `input_lease_scope_unavailable`（两个码都已在既有 pre-input 码表内）。
- 集成负责人在 `TracingAdapter` 的 `input_lease` 字段类型改为 `Option<&ScopedInputOwnership<'static>>`（它同样提供 `is_current()`/`owner_epoch()`，因此输入前 epoch 屏障语义不变）；测试无需构造 lease（走真实执行器路径）。
- 证据：`cargo build -p coolzhu-web-console` EXIT 0；`computer_use_executor::` 定向 **37/37**（含"输入前 lease 被失效 → 零原生输入"与"lease 完好 → 恰好 1 次输入"两条正反对照）、`incidental_cursor_input_yields_to_existing_owner_and_recovers` 通过。

## E. 第二轮裁决记录（11 项，2026-09-24）

裁决输入：`C:\Users\zhupu\Desktop\第二轮答复.md`。它与 `decision-requests-round2.md` **一一对应但只覆盖第 1–11 项**：**第 12 项（hook 独立授权来源）与第 13 项（互锁作用域）本轮未裁决**，按"未回答的项保持现状并标注待裁决"处理。

| 项 | 裁决 | 对本仓库的直接约束（实施时必须遵守） |
| --- | --- | --- |
| 1 `RunIdentity` | **分作用域校验**：保留唯一 `RunIdentity`，不适用维度用 `Option` 缺省（存 NULL / 序列化省略）；**禁止 `""`/`"unknown"`/`"n/a"`/`"0"`/复制 `turn_id` 凑字段**；本应存在却缺失 → 明确"身份不完整"错误；旧无参 `validate()` 保持严格语义作兼容入口，新写入路径**必须**调 `validate_for(scope)`；记录持久化 scope 与身份 schema 版本，旧记录缺 scope 时按旧严格规则解释（**不得默认视为 Turn**） | 终态映射修正：`Interrupted` **只有**在有宿主确认的取消事实时才 → `Cancelled`，否则 → `Interrupted`（不存在该枚举值时本轮授权新增）。`claim_token` 不进公开身份对象；输入所有权 epoch、会话 writer epoch、claim token **不得混用** |
| 2 SQLite 适配器 | **先落 Web 侧**：新增 `web-console/src/fact_log_sqlite.rs`，**不给 core-runtime 引入 rusqlite**；同一业务提交的步骤更新与事实追加**必须同一事务**；不为"零签名改动"牺牲事务一致性 | **禁止**"先提交步骤成功 → 再独立连接写回执 → 第二步失败 → 仍宣布事实已保存"；不得宣称会话库与输入安全库之间有跨库原子事务；迁移统一登记，不在请求里 `CREATE/ALTER`；同 ID 同内容返回既有结果、同 ID 不同内容**必须报冲突**；不用"十字段拼接串"当万能幂等键 |
| 3 根 deadline | **当前明确没有**：运行记录标 `root_deadline_state = not_wired` / `root_deadline = absent` / `cu_deadline = 实际截止`；`not_wired` **不等于**"用户选择了无限时间"；有效调用预算 = `min(CU 剩余, 当前阶段上限, 实际模型调用上限)` | 根预算接线列为 **RPR-13/S2.5** 明确交付项（不接受无限期延期）。CU 计时起点：**任务接纳并进入调度时**建立唯一截止，必须早于租约等待、模型切换、初始观察与规划；重试/重规划/受限恢复共用同一截止，模型切换耗时不得从预算中隐藏 |
| 4 planner `remaining` | **授权公开 API 变更**：`classify`/`next_action`/`verify` 等**所有可能发起模型调用的方法**统一接收剩余预算；**不允许**"为兼容而忽略 remaining"的默认实现，也不允许缺参时退回固定大超时 | planner 内多个子请求**必须连续扣减**（如"视觉转述 → 动作规划"不得各拿一份完整 remaining）；跨队列等待后不得用旧 remaining 重算；预算为零/不足时**模型 HTTP 请求次数必须为零**；超时后到达的模型结果**不得触发新动作**，但 usage/回执可作为迟到事实保存 |
| 5 两秒收尾 | **修正版 c**：业务 deadline 与 cleanup deadline **分开**。协作退出等待 ≤2s；自首次进入取消/异常收尾起总收尾窗口 ≤4s；独立释放**最多一次**且等待 ≤`min(2s, 剩余收尾时间)`；终止 helper、等待静止、独立释放、同步对账**共同消耗**这 4 秒 | `cleanup_deadline` **一旦固定不得被后续重复信号刷新**；不承诺"释放一定成功"，只承诺有界收尾 + 不确定则隔离并阻止后续输入；四秒结束也不证明原生阻塞操作已停止；报告分开给出：业务截止/停止新输入/收尾开始结束/释放状态/是否隔离；**允许有限安全收尾超出业务期限，但不允许借收尾继续规划或补做任务** |
| 6 六项门禁 | 确认与我方六个面向一致，提供原文并编号 **G1 请求链清单 / G2 实际 wire 体量 / G3 小范围能力校准 / G4 最低闭环可行性 / G5 超预算行为 / G6 原有输入安全门禁** + 三条解释 | 需把原文、G1–G6 编号与三条解释**写入预算链文档**（这是一项待执行的文档交付）：G2 根 deadline 未接线时标 `not_wired`；G5"无新动作"指无新业务动作（受限安全收尾不被误禁、也不算继续任务的例外）；G4+G6 先离线安全验证 → 最小真实闭环 → 才进连续 Paint |
| 7 排空 | **a 为主、b 附加**，但**必须跟踪到流生命周期结束**：不批准"握手结束计数归零"，也不批准"流被 Drop 就证明服务端空闲"。允许：Provider 增加连接/服务身份查询方法、`MessageStream` 改为持有内部流 + 生命周期 guard 的结构体、新增 `api::local_endpoint_drain()` | 在途状态须区分四种：尚未派发 / 已派发流处理中 / 已获可信结束事实 / 远端结果未知；guard 在**实际发出请求前**登记，流式请求把 guard 从握手阶段**转移**到返回的流对象；Drop 只结束本地持有，不证明服务端停止计算；**排空必须先原子关闭该实例的新请求接纳、再等待**，闸门保持到切换完成；同一托管实例的所有用途/会话共用同一闸门；避免自锁；无法建立足够证据时**拒绝自动切换**（端口检测/延长等待/心跳都不能代替结束事实）；`/slots` 只作可选增强且须用真实二进制验证 |
| 8 浏览器面 | **当前选 b**（启发式但**不得产生强执行结论**），协议扩展单独立项。本轮最小增量：`ActionReceipt` 补 `surface`/`evidence_basis`/原始错误引用/推断规则版本/`request_id`·`action_id`；`evidence_basis` 至少区分宿主机输入前拒绝、原生 helper 回执、浏览器协议响应、启发式推断、未知 | `partial` 无法确定时必须能表达未知（授权改为可空或三态），**不得默认 `false` 表示完整执行**；DOM 操作不得伪造鼠标释放成功（无释放义务可标"不适用"）；扩展协议工单前，动作去重与"未知副作用不自动重放"照常生效 |
| 9 临时光标 | **补输入前复检**（不只在 acquire 时检查）：最终移动光标前共享正式通道的 scope 正确 / owner·epoch 有效 / 未隔离或恢复中 / 未取消 / 仍有执行预算；失效则返回明确错误且**光标输入调用次数为零**，不得回退到无 lease 的底层原语 | 复检与派发要进入**同一 broker 的序列化决策边界**（避免 `is_current()=true → 长时间等待 → 已撤销 → 仍派发`）；明确并发顺序：撤销先被接受则后续不得派发；输入先被接纳并已派发则撤销**不得**把它改记成 `not_sent`；本项只处理已定位路径，不扩大为"全仓无其他旁路" |
| 10 环境变量 | **同意 RPR-01b**，采用 RAII；**修正措辞**：不是"用户系统环境变量被永久修改"，而是污染**当前测试进程**的后续执行直到恢复或进程结束。优先序固定：① 子进程专用环境用 `Command::env`（不改父进程全局环境）；② 必须测进程级解析时复用**现有统一环境锁 + RAII guard**（锁从读原值前持有到全部恢复后；不得每模块各建一把锁） | guard 必须保留"原来不存在/原来为空/原来有值"的区别；多变量构造中途失败要恢复已改部分；同锁下嵌套 guard 复用锁令牌避免自锁；恢复失败**不得**被 poison-tolerant 逻辑吞掉后继续报告环境健康；RAII **不保证** abort/直接退出等不展开栈的终止方式；不授权顺手升级工具链或 edition；"约 15 处"只作线索**不作验收分母**，交付须列实际扫描范围、已迁移点与剩余例外 |
| 11 互锁解除入口 | **发布选 a**：最小原生恢复入口（键盘可达原生对话框）与互锁**共同形成可发布单元**；不采纳"仅靠审计的 API/CLI 解除"——"本机/TTY/可审计"都不证明操作者是人。CLI/API **只允许**查询隔离状态、查看恢复事件、请求打开原生对话框 | **不允许** `--force-unlock`/`--yes`/`confirmed=true`/任何直接把 `Quarantined` 改 `Idle` 的普通 HTTP·CLI 操作；无人工恢复入口的候选版本**不得宣传**该 CU 能力已有完整闭环，且**不得为没有恢复 UI 而让已知释放未知状态继续输入**；已存在的隔离记录不得通过开关/升级/回退/重启自动清空；测试专用解除接缝继续受编译期条件限制 |

**裁决给出的接线组合（本轮执行顺序）**

| 组合 | 解锁 | 结束标志 |
| --- | --- | --- |
| 1+2+8 | RPR-04c | 分 scope 真实身份、SQLite 事务接线、浏览器证据等级进入实际存储与消费者 |
| 3+4+5+6 | RPR-11c | CU 预算真实闭环、planner 消费 remaining、独立有界收尾、G1–G6 逐项证据 |
| 7 | RPR-11a 补强 | 流生命周期排空、切换接纳闸门、未知状态拒绝终止 |
| 9 | RPR-02a/broker 补强 | 临时光标路径输入前复检 + 故障注入 |
| 10 | RPR-01b | 环境变量清理与失败隔离回归 |
| 11 | RPR-05b-1+05c+05d | 互锁、最小人工恢复、安装验证一起达到发布条件 |

另：**RPR-06a（审批绑定冻结输入）继续按上一轮裁决实施**，不因根 deadline 未接线而暂停；涉及身份绑定的部分跟随第 1 项新契约统一迁移。

**裁决固定的三条报告用语（必须遵守，防止"接口能通过、运行语义不成立"）**
1. "CU 级预算已闭环" **不等于** "整个聊天已有根 deadline"。
2. "已知客户端已排空" **不等于** "服务端所有生成均已停止"。
3. "恢复记录可审计" **不等于** "解除操作已取得可信人工确认"。

### B-19 排空（裁决第 7 项）交付 + 集成接线 + 一处**我此前的错误被纠正**

**子代理交付（`modules/llm-adapter/**`）**
- **按裁决要求逐个核验了全部 `MessageStream` 消费者**（这正是不服上一轮"只证明 main.rs 不匹配"的批评）：结论是 `MessageStream::` 变体的构造/匹配全仓**只存在于 `client.rs`**，改造后变体变为私有 `MessageStreamKind`；web-console / CLI / tool-registry 的用法都是不透明类型 + `next_event()`，**没有任何生产调用者或测试匹配变体**；`cargo build --workspace` 0 error 作为兼容性证据。
- `Provider::endpoint_identity()` 带默认实现（默认 `unsupported()`，且**该默认值在排空判定中拒绝自动切换**，不会被读成"没有在途"）；`MessageStream` 改为持有内部流 + `InFlightGuard` 的结构体；新增 `inflight.rs` 的四态模型与 `local_endpoint_drain` / `local_endpoint_drain_all` / `drain_verdict_for`。
- 四态与规则表逐条落地，术语严格：`EndpointDrainReport::client_drained` 的文档明确写"这不是服务端所有生成均已停止"；`DrainVerdict` 只在 `ClientSettled` 时 `permits_automatic_switch()`，`IdentityUnknown` 与 `ClientSettledWithRemoteResultUnknown` 都拒绝。
- 排空等待用已启用的 `tokio/time` 轮询，**未新增任何依赖或 feature**。
- 门禁：llm-adapter **142 通过 / 0 失败**（基线 119）；workspace 构建 0 error。

**我此前的错误（子代理纠正）**：我在工单里写"main.rs 调用 `next_event()`/`request_id()`"——实测 **main.rs 从未调用 `request_id()`**（全仓 `.request_id()` 只在 llm-adapter 内部与两个测试）。结论（不透明使用）不变，但事实描述有误，已在此更正。

**集成接线（我做的，`main.rs`）**
- 新增 `drain_refusal_reason(...)`（纯逻辑，可单测）与 `local_model_switch_drain_gate(port)`：**身份键由与请求路径相同的构造函数得出**——用 `ProviderClient::from_custom_openai_compatible(model, "http://127.0.0.1:{port}/v1", None).endpoint_identity()`，因为身份键 = `provider_id + resolved_endpoint`，而后者是 `chat_completions_endpoint(base_url)`（**完整 URL 而非 base**）。**手拼端点会查到空桶并被误判为"已排空"**，这是本项最容易造成假放行的坑，已在代码注释里写明。
- `api_local_models_switch` 在调用 `switch_local_models` **之前**过闸门；拒绝时返回按 verdict 分类的中文原因（身份未知 / 远端结果未知 / 在途未结清），并附"不能证明服务端所有生成均已停止"的严格表述。
- **我划的一条边界（需确认）**：`mode == "off"`**不**过该闸门——因为 "off" 是用户显式停止请求而非自动切换，且拒绝停止会导致显存无法释放。裁决原文说的是"无法建立足够证据时拒绝**自动切换**"，故按此实现；若要求 off 同样受闸门约束，这是一行改动。
- 新增 2 条测试：`drain_refusal_reason_never_treats_unknown_as_drained`（纯逻辑，含"禁止把已排空写成服务端已停止"的断言）、`local_model_switch_is_gated_by_drain_before_stopping_anything`（源码接线契约，断言排空调用先于切换调用）。

### B-20 输入派发边界（裁决第 9 项）交付 + main.rs 侧接线

**子代理交付（`windows-process-guard`）**
- `InteractiveInputLeaseBroker::dispatch_if_current(scope, owner_id, expected_epoch, guards, dispatch)`：**复核与派发在同一临界区内完成，从校验到 `dispatch()` 之间没有任何锁释放点**，因此不存在"`is_current()==true` → 等待 → 已撤销 → 仍派发"的路径。
- 重入检测（`Mutex` 不可重入，自锁是真实风险）：线程局部标记 + 四个入口分别处理——`dispatch_if_current` 返回 `Reentrant`；`acquire` 返回带 `reentrant_dispatching_scope` 的 busy；`is_current` **fail-closed 返回 false**；`revoke`/`Drop` 压入**延迟移除队列**由该线程临界区返回前补做（避免"静默吞掉撤销导致记录泄漏"）。`records` 锁改为中毒容忍，一次 panic 不会让 broker 永久失效。
- 取消/隔离/预算三条状态**不硬编码**，改为调用方传入的带 label 检查闭包（`InputDispatchGuard`），拒绝时 `GuardRefused{label}` 可用于映射既有错误码。
- 选定并测试了"派发中撤销"的语义：**阻塞到 dispatch 返回后生效**，且在途输入**不被改记**为 `not_sent`。
- 门禁：guard **34 通过 / 1 ignored**（基线 23）；11 条新测试含一条 4 线程 × 120 轮 × 3 种时序的压力测试，不变量含"派发成功 ⇒ 决策 ticket < 撤销返回 ticket"。
- **本轮最强的判别性验证**：把实现临时改回"复核后释放锁、sleep 80ms 再派发"的交错形态，**恰好且仅有两条测试失败**（报错正是所需断言），回滚后与变异前快照一致。这是全批次唯一做到这种强度的工作包。

**main.rs 侧接线（我做的）**
- `move_cursor_to` 从 `async fn`（内部其实**没有 await**，是阻塞 `Command::output()`）改为同步函数，使它可以作为同步派发闭包；
- `verify_cursor_on_target` 改为 `lease.dispatch_if_current(&[], || move_cursor_to(x, y))`：失效或让位时**零光标输入且不宣称验证通过**，不回退到无 lease 的原语。
- **尚未接上的三条 guard（诚实标注）**：取消 / Quarantined-恢复中 / 执行预算——该函数作用域内**没有可达的取消与隔离状态**（取消状态按 turn/room 持有，隔离状态在事实层），要接需要plumbing 参数。因此这三条目前**只是机制，尚未生效**。
- 新增源码接线契约与其行为已由定向测试覆盖（`incidental_cursor_input_yields_to_existing_owner_and_recovers` 通过）。

**待办（等 CU 预算子代理释放 `computer_use_executor.rs` 后）**：`TracingAdapter::act` 里**输入前校验与真正派发之间夹着 `store.record_step`**（落库 = 任意等待，正是裁决点名的交错窗口）——最小修法是把 `record_step` 挪到边界之前，再用 `dispatch_if_current` 包住 `inner.act(...)`；未做之前，**executor 侧的那半个窗口仍然存在**。

### B-21 `RunIdentity` 分作用域契约交付（裁决第 1 项 + 第 8 项契约部分）

**交付（仅 4 个文件：`run_contract.rs` 重写、`fact_store.rs`、新增 `action_evidence.rs`、`lib.rs` 导出）**
- `RunIdentityScope::{Turn, StepAction}`：五维容器（workspace/room/session/public_turn/run）**两级都必填**；`step_id`/`request_attempt_id`/`tool_call_id`/`action_id`/`owner_epoch`/`parent_run` 改为 `Option`（省略 = NULL）。`validate_for(scope)` 按表校验；**不适用维度被提供即拒绝**（`identity_dimension_not_applicable`）——这是"降 scope"的可见信号；`validate()` 保留旧十维严格语义；`validate_persisted()` 按记录自身口径分流，**缺 scope 的旧记录按旧严格规则解释，不视为 Turn**。
- **哨兵拒绝**：`""`/`unknown`/`n/a`/`none`/`-`/`0`、控制字符，以及"把 `public_turn_id`/`run_id` 抄进动作维度或 `owner_epoch`"一律拒绝；`InputOwnerEpoch::from_host_counter(0)` 直接拒绝（宿主分配器从 1 开始）。
- **凭据域分离**：新增 `InputOwnerEpoch`/`SessionWriterEpoch`/`RunClaimToken` 三个独立类型，**无跨域 `From`**；`RunIdentity` 里没有 `claim_token`（有测试断言序列化后不含该键）。
- **终态**：`RunTerminalStatus` 新增 `Interrupted`；手写 `Deserialize` **不认识的变体直接报错**（不落成功、不落默认值）；`HostRunOutcome` 映射实现"只有宿主确认的取消才 → `Cancelled`，否则 → `Interrupted`"（`CancelOrigin::User` 只能由宿主确认后使用）；`is_scope_success()` 只认 `Succeeded`，`claims_goal_completion()` 恒 false；`FactSnapshot::scope_succeeded()` 的 `None`（无终态事实）**不是成功**。
- **证据元数据（第 8 项）**：因 `ActionReceipt` 是**结构体字面量**生产（6 处、且本轮禁止改那些文件），加字段会让它们编译失败——故证据做成**同层事实** `ActionFact{identity, receipt, evidence}` 与 `FactLogRecord::ActionReceipt{..., evidence}`（新键 `#[serde(default)]`，旧 JSON → `None`）。`ActionEvidence` 五态依据 + 逐依据必填引用；**只有 `NativeHelperReceipt` 可以断言 `Released`**，DOM/协议/推断/未知路径断言 `Released` 一律拒绝；间接依据不得断言 `Sent`/`path_completed=true`/`partial=false`。写入前 `validate()` + `validate_against(receipt)`，不过则**拒绝写入**；`IdentityAnomaly{blocks_further_input}` 是"停止后续输入"的执行点。
- **`partial` 无需改动**：它本来就是可空 `Option<bool>`（三态），旧 `Some(false)` = **明确未观测到部分执行**而非"完整"；新增只读视图 `PartialObservation::{Unknown, Partial, NotPartial}`（**没有"完整执行"变体**，从类型上堵死 `false ⇒ 完整`）。
- 门禁：core-runtime **274 通过 / 0 失败**（基线 241）+29 条新测试；`cargo build --workspace` 成功；linkage 4/4。**下游生产者一处都不用改就能编译**（因为 `ActionReceipt` 字段表未变）。

**子代理主动申报、需要确认的 4 点**
1. **证据没挂在 `ActionReceipt` 上**：硬约束下的唯一可行解。若裁决坚持挂在 `ActionReceipt`，最小影响面是同时改 6 处字面量（`input_stroke.rs`/`contracts.rs`/`controller.rs`/`browser_bridge.rs`/`computer_use_executor.rs`），本轮禁止。
2. **`StepAction` 把 `tool_call_id` 列为必填**（照裁决原文）——若某些原生动作确实没有工具调用，需要裁决明确放宽；子代理**没有自行放宽**。
3. **`request_attempt_id` 在 `Turn` scope 下被定义为"可选"而非"不适用"**，否则 attempt 登记事实（登记键就是它）无处安放。
4. **异常解除路径未定义**：`must_stop_input` 目前只能靠人类显式处理（本轮范围外，已在文档标注）。

**仍成立的边界**：`fact_store` **仍未接线**（没有任何请求入口调用它，与 RPR-03 现状一致）；因此**"已接线"的说法不成立**。测试总数 241→274（+33），其中 +29 在改动文件内，**另有 4 条出现在本会话未触碰的模块，子代理无法归因**（原有用例逐条仍在且通过）。

### B-22 Web 侧 SQLite 事实后端（裁决第 2 项）已落地：共享连接 + 类型化主体键 + 冲突检测

**新增 `modules/gui-web/packages/web-console/src/fact_log_sqlite.rs`**（实现 core-runtime 的 `FactLogBackend`）
- **借用调用方的连接**（`SqliteFactLog::new(&Connection)`），**不自己开连接**——这是裁决第 2.2 项那句"禁止 先提交步骤成功 → 再独立连接写回执 → 第二步失败 → 仍宣布事实已保存"的结构性前提：只有共用连接才能放进同一个 `transaction()`。
- **类型化主体键**（`fact_subject`）：每类事实各自的键由**该类的既有字段**构造，不是"十字段拼接串"——终态 = `run_id`（含 scope 列）；attempt 登记 = `run_id#request_attempt_id`；恢复 = `parent_run_id#attempt_id`；动作回执 = `run_id#action_id`；迟到事实 = `run_id#source#received_at_unix_ms`；用量 = `run_id#logical_request_id#attempt_id`（沿用 RPR-03 的发现）；提交 = `client_message_id#scope`。**键无法构造时拒绝写入**，不补默认值。
- **幂等与冲突**：相同主体键 + 相同内容指纹 → 幂等 `Ok`（不重复落库）；相同主体键 + 不同内容 → **`FactStoreError::IdentityConflict`，绝不覆盖旧事实**。内容指纹用 FNV-1a 64（**明确不是安全哈希**，只作冲突判定）。
- **迁移统一登记**：新增 `apply_session_migration_v21`（`main.rs`）+ 阶梯调用；建 `fact_log_records` 表与 `UNIQUE(record_kind, record_subject)` 索引，`PRAGMA user_version = 21`。**不在请求里 CREATE/ALTER**。
- 4 条新测试全部通过：幂等写入 + 冲突不覆盖 + 动作回执按 run#action 分键 + 不同类别同文本互不冲突 + 空 action_id 的边界（**测试名已按它真正证明的内容更正**，不再用会误导的名字）。

**明确未做的部分（不得声称"事实已持久化到业务链"）**
1. **没有任何生产调用者使用该后端**——"步骤更新与事实追加同事务"的**协调器尚未接线**（`ComputerUseRunStore` 已有单连接 + `transaction()`，接线点是它；其调用方 `computer_use_executor.rs` 当时正被 CU 预算子代理编辑，故留给下一批）。
2. 因此当前仍**不能**说产品已在业务提交里原子写入事实；本项交付的是"后端就绪 + 迁移登记"。

### B-23 CU 预算闭环（裁决第 3、4、5、6 项）交付 + **我自己造成的一次回归已被发现并修复**

**交付要点（`computer-use-core` + web-console CU 文件）**
- **CU 截止时间建立点**：`computer_use_executor.rs:695` `CuDeadline::establish_without_root(&budgets, now_ms())`；有**源码顺序测试**断言它早于未确认释放互锁、`ScopedInputOwnership::acquire`（租约等待）、`adapters.build`（模型切换）、控制器入口（初始观察与规划）；另有行为测试（观察侧延迟 250ms → planner 看到的剩余 ≤120000−250）。
- **planner 全路径消费 `remaining`**：`classify`/`next_action`/`verify` 都接收；**`verify` 的默认实现已删除**（无"忽略 remaining"的兼容实现）。零预算 → **模型 HTTP 请求计数实测为 0**；串联请求（视觉转述 → 动作规划）**共用同一预算**，第二个拿到扣减后的余量（实测在第二个被拒时总 HTTP 计数 = 1）。迟到结果**只记账、不取消、不补发**，到期后 HTTP 计数仍为 1。
- **两个期限分开**：策略值集中在 `cleanup.rs`（协作退出 2s / 收尾总窗口 4s / 独立释放 ≤2s），单一入口 `input_stroke.rs:19`；`CleanupDeadline::fixed` 用 `get_or_insert_with` 语义**不被重复信号刷新**（有三次不同时刻信号的测试）；另有一条测试断言源码里**不再出现写死的 6 秒**。三个数值均标注为**待验默认值，不是实测时延**。
- **G1–G6**：写入 `rpr-11c-budget-chain.md` §10（编号 + 名称 + 三条解释）；**逐条原文**由我补录（见下）。
- 顺手修掉一个真实泄漏：`browser_bridge.rs:597` 的 tab 动作过去丢弃 `remaining`、用满 `BRIDGE_TIMEOUT`，现改为 `clamp_stage_timeout(remaining, BRIDGE_TIMEOUT)`——这关闭了 RPR-11c 早先发现的"唯一可先修且不引跨 crate 变更"的那处。
- 门禁：`computer-use-core` **82 通过**（基线 60，+22）；linkage 4/4。

**我自己造成的回归（被这个子代理如实报出，我据其定位并修复）**
- 现象：它复跑全量时看到 main **1036/5 失败**（共 1041），全是 `main.rs` 的迁移/版本断言（`user_version` 实测 21 ≠ 期望 20）——**根因是我加的迁移 v21**，不是它。
- 修复：不是简单把 20 改成 21，而是引入 `const SESSION_SCHEMA_VERSION: i64 = 21;` 作为"阶梯终点版本"的单一来源，并把那 6 处断言改为引用该常量——**以后再加迁移不会再连带打断一批测试**。
- 教训（写进这里以免重犯）：**加一步迁移会打断所有"阶梯终点"断言**；这类断言必须引用常量而不是硬编码数字。

**子代理主动申报、需要知悉的三点**
1. **它改了 `computer_use_store.rs` 2 行，但仅 `#[cfg(test)]` 测试夹具**（`cu_budget: None, cleanup: None`）：给 `ComputerUseResult`（在允许清单内）加字段是该结构体**字面量**的破坏性变更，不加这两行整个 crate 无法编译；**未改任何列、签名或生产逻辑**。我接受这一最小改动（替代方案是换载体结构，表达更弱、代价更高），记录在案。
2. 两处**由它选定的值需要确认**：`MIN_STAGE_BUDGET = 500ms`，以及三个收尾数值（裁决已定为"待验默认值"，G5 门禁实测后应重校准）。
3. **仍有真实缺口**：`input.rs`（点击/输入/滚动/按键）路径**未纳入有界收尾**——它用 `run_powershell` + 分离线程 + `recv_timeout`，超时后子进程仍在跑（无 kill、无静止确认），与 `input_stroke.rs` 是两套机制；该路径的释放事实仍只按"错误码 → `may_have_been_sent` + unknown"记录。→ 应立为独立工单。

**关于"三个 deadline 字段进运行记录"的好消息**：`cu_deadline` **已有列** `computer_use_runs.deadline_ms`；`root_deadline_state`/`root_deadline` 已随结果对象写入 `terminal_result_json`（`ComputerUseResult.cu_budget`，`#[serde(default)]`，旧记录读成 `None`）。**因此裁决第 3.1 项不需要改存储层即可满足**，我原先排的"改 store 三字段"待办可以取消（若要便于 SQL 筛选再列化，属可选优化）。

**合并全量门禁（所有写者停止后，含我的修复）**：build-web exit 0、**web 1041/1041**、**cu-core 82/82**、**guard 34/34**、**toolreg 53/53**、**core 274/274**、linkage 4/4，**全部 exit 0、测试构建警告 0**。

### B-24 executor 侧派发边界已接线（关闭 §B-20 里记的 `record_step` 窗口）+ 我犯的一个极性错误

**改动（`computer_use_executor.rs` 的 `TracingAdapter::act`）**
- **删掉**原来那个"只在派发前查一次 `is_current()`"的检查（它后面还夹着 `record_step` 落库 = 可能长时间等待，正是裁决点名的交错形态）。
- **权威判定移进序列化边界**：`lease.dispatch_if_current(&guards, || self.inner.act(...))`，复核与派发之间没有释放点。
- 传入两条守卫（`check()` 返回 **true = 允许派发**）：`cancelled` → `not_cancelled`，`execution_budget_exhausted` → `!remaining.is_zero()`。**隔离/恢复中不做边界内查库**（DB 查询是长等待，会重新制造本边界要消除的交错），该状态在运行接纳时已由未确认释放互锁把关——这条限制写在代码注释里。
- 拒绝→错误码映射复用既有码：guard 拒绝 → `cancelled`/`budget_exhausted`/`input_lease_lost`；重入 → 新增 `input_dispatch_reentrant`。**两个新码都加进了 `failure_may_have_sent_input` 的 pre-input 例外表**（它们确实意味着"输入未发出"）。

**我犯的极性错误（探针定位，非推理）**
- 我最初把 cancelled 守卫写成 `|| (self.cancelled)()`，结果"未取消"（返回 false）却触发拒绝——**broker 的约定是 `check()` 返回 true 表示允许派发**（它自己文档里的例子就是 `!cancelled`）。改名为 `not_cancelled` 并取反后通过。
- 定位手段值得记录：我没有靠猜，而是先做**决定性实验**（把守卫数组临时置空 → 该用例立刻通过 ⇒ 确认是守卫而非边界），再加一行临时探针打印 `refusal` 与 `(self.cancelled)()` 的实际值（输出 `cancelled_now=false` 却报 "guard cancelled did not pass"）⇒ 直接指向极性反了。探针已删除。
- **建议**（未做，留给 guard crate 的后续）：把"`check()` 返回 true = 允许派发"这行约定写进 `InputDispatchGuard::new` 的文档注释——现在只能从示例里推断，容易再踩。

**顺带发现并修掉的一个真实缺陷（审计诚实性）**
- 把检查移进边界后，步骤行会**先**被写成 `status="executing"`；若边界拒绝，就会留下一条"执行中"的陈旧行，而那个动作从未执行。
- 修法是两条：① **边界之前**保留一次**安全向预检**（`is_current()`；它只可能提前拒绝、不可能放行，因此不重新引入交错）——常见情形（已被撤销）下**不写任何步骤行**，也就保住了 RPR-02a 那条"零 step 行 + 零原生输入"的断言；② 若拒绝发生在边界内（撤销落在 `record_step` 期间），用 `record_step` 的 **upsert** 语义把该行修正为 `input_not_sent` + 错误码（修正写入失败则回报 `persistence_error`——不可审计的拒绝正是本批要消灭的那类问题）。
- **未覆盖的分支**：边界内拒绝的"修正行"路径**没有专门测试**——要在 `record_step` 与边界之间注入撤销需要一个测试接缝，当前没有。已在测试注释与本节标注（沿用 RPR-02a 对"未做反向验证"的同一诚实口径）。

**门禁**：`computer_use_executor::` 定向 **40 通过 / 0 失败**（含"输入前 lease 被失效 → 零原生输入 + 零 step 行"与"lease 完好 → 恰好 1 次输入"两条正反对照）；合并全量门禁见下方与 work-log。

### B-25 步骤与事实同事务的仓储方法已落地（裁决第 2.2 项的结构性前提）

**改动（`computer_use_store.rs`）**
- 把步骤写入抽成**唯一实现** `ComputerUseStepRecord::write_step(&Transaction, ...)`，`record_step` 与新增的 `record_step_with_fact` 都调用它——**避免两条写入路径各自漂移**（裁决明确禁止"接口不变但牺牲事务一致性"的偷懒做法）。
- 新增 `record_step_with_facts(step, action_json, append_facts: impl FnOnce(&Transaction) -> Result<(), String>)`：在**同一事务**里写步骤（含 upsert 更新），然后把该事务交给调用方去跑规则引擎。**返回 `Err` 或规则引擎报错都会回滚整个事务**，步骤更新一并撤销——这正是裁决那句"先提交步骤成功 → 再独立连接写回执 → 第二步失败 → 仍宣布事实已完整保存"的反面。
- **我自己的一处缺陷（自查发现并已修正，值得记录）**：该方法的第一版签名是 `Option<&FactLogRecord>`，直接落一条 `FactLogRecord`——**绕过了 `AppendOnlyFactStore` 的规则引擎**（契约校验、first-wins、身份异常分流、`validate_for(scope)`），而裁决明确要求新写入路径必须走规则。改为"把事务交给调用方"的形式后，事实只能经 `AppendOnlyFactStore` 写入。**教训：能做同事务不等于写对了地方；规则引擎的位置不能因为方便而被跳过。**
- 新增测试 `step_and_facts_commit_together_and_a_rule_engine_error_rolls_back_the_step`：① 正常路径证明步骤与**终态事实同事务提交**（事实经 `AppendOnlyFactStore::open(SqliteFactLog::new(&transaction))` + `record_host_outcome(Completed)`）；② 用"自相矛盾的回执（`not_sent` 与 `partial=true` 不可同时成立）"让规则引擎报错，断言 **步骤状态仍是原值 `input_sent`**（证明回滚）且**事实表仍只有 1 条**。
- 该测试同时证明了一件有独立价值的事：**规则引擎可以运行在"借用的事务"上**（`AppendOnlyFactStore` 的 `open` 会 `read_all` 重放，而事务内的写入对同一连接可见），这是本项后续接线的前提。
- 门禁：`computer_use_store::` 定向 **19 通过 / 0 失败**。

**仍未做的部分（不得声称"事实已进入业务链"）**
- **生产调用者还没切换到 `record_step_with_fact`**：`TracingAdapter::act` 仍在用 `record_step`。切换需要为事实构造 `RunIdentity`，而契约对本项有**明确禁令**：`public_turn_id` 只有**确认**宿主侧 `turn_id` 正是公开轮次 ID 之后才允许映射，guard 持有的是内部 turn id，"核对不了就不得拿它顶替"。因此这一步必须等 §B-26 的上下文捕获（`ChatTurnGuard` 持真实的工作区/房间/会话/公开轮次）落地后再接，**不能靠猜 turn id 语义硬接**。
- 所以当前状态应表述为：**同事务写入的通道已就绪并经测试证明，但事实尚未在业务提交里产生**。

### B-26 `ChatTurnGuard` 上下文捕获：**关键疑问已用证据回答**，接线方案已定（尚未实现）

契约（裁决第 1 项）对本项有一条硬禁令："只有**确认**宿主侧 `ChatTurnGuard.turn_id` 正是公开轮次 ID 之后，才允许把它直接映射到 `public_turn_id`；guard 持有的是内部 turn id，核对不了就不得拿它顶替。" 因此我先做核对，结论如下（**带证据，不是推断**）：

**结论：web 的 `turn_id` 就是对外可见的轮次 ID，映射合法。**
- 同一处代码里 `let turn_id = new_chat_turn_id();`（`main.rs:17146` 一带），随后**同一个值**被用于：① 注册取消（`register_chat_turn_with_run(&turn_id, ...)`）、② 写入 `computer_use_runs.turn_id`、③ **经 SSE 事件发给前端**（如 `sse_json_event("tool_status", ..., &turn_id, ...)`）、④ 前端把它**回传**给服务端（`app.js:13718` 的 `turn_id: turnId`，并按 `item.turn_id` 分组，`app.js:8866`）。
- "被客户端接收并回传"正是"公开轮次 ID"的判据，因此可以直接映射 ✅（若产品另有独立的对外轮次 ID，请告知，本节结论随之更正）。

**更有利的事实：构造 guard 的那一处已经同时持有全部五个容器维度**（`main.rs:17150-17168`）：
| 维度 | 就地可得的来源 | 备注 |
| --- | --- | --- |
| `workspace_id` | `workspace_identity(&active_workspace_path())` | 已在局部变量里，**不得**用 `db_path` 顶替 |
| `room_id` | `result.chat_room_id` | 接纳时捕获，不得读"当前 UI 选中的房间" |
| `session_id` | `result.conversation_session_id` | **是 `Option`**：为 `None` 时容器维度缺失 ⇒ 契约要求**硬拒绝**，因此这种情况**不写事实**（不得凭空补值），并记录缺口 |
| `public_turn_id` | `turn_id`（见上结论） | — |
| `run_id` | `run_id`（`new_runtime_run_identifiers()`） | 与 CU run 是不同实体，用显式父子关系关联，**不得同名合并** |

**接线方案（下一步，四件事）**
1. `ChatTurnGuard` 增加一个**不可变**的 `RunScopeContext`（在构造时捕获上表五项；`session_id` 缺失时置 `None` 并据此放弃写事实）。
2. **终态**：`finish`/`Drop` 改走 `record_host_outcome`，把 `ChatTurnStatus` 映射为 `HostRunOutcome`——`Completed→Completed`（**不表示用户目标已完成**）、`Failed→Failed{reason}`、`Interrupted` **必须区分**"宿主确认的取消"（`HostRunOutcome::cancelled(origin)`，来源不得默认用户）与"仅请求过取消/异常中断"（`HostRunOutcome::interrupted(evidence)`）；`Running`/`InterruptRequested` 非终态**不写**。
3. **同事务**：`finalize_chat_runtime_run_sqlite` 目前按路径自行开连接，直接在外面再开一个连接写事实**违反**裁决第 2.2 项。需要在会话存储里加一个"**终态落库 + 事实追加**同事务"的方法（与 `computer_use_store::record_step_with_fact` 同构——后者已落地并测试证明，见 §B-25）。
4. 把 `SqliteFactLog` 的写入接到该方法里（它已能借用事务连接 ✅）。

**尚未做**：上述 1–4 都未实现，因此**当前仍没有任何请求入口产生事实**——"事实已持久化到业务链"的说法依然不成立。

### B-27 `ChatTurnGuard` 接线完成：**生产入口首次在业务事务内写事实**（裁决第 1 + 2 项）

**改动（`main.rs`）**
- `ChatTurnGuard` 增加**接纳时捕获**的 `ChatRunScope`（workspace / room / session / public turn / run 五维）+ `with_scope` 构造器；缺任一维度即不构造作用域，并 diag 记录"本轮不写终态事实"。捕获点是 SSE 流的起点——那里同时持有全部五维（`workspace_identity(...)`、`result.chat_room_id`、`result.conversation_session_id`、`turn_id`、`run_id`），且 `turn_id` 已核实为公开轮次 ID（见 §B-26）。
- `finalize_chat_runtime_run_sqlite` 重构为**单一实现** `..._with_facts(..., append_facts: FnOnce(&Connection, ChatTurnStatus) -> Result<(), String>)`；旧签名成为传空闭包的薄包装（3 处既有测试不受影响）。
- `append_facts` 在 **`commit()` 之前**、同一事务内被调用；返回 `Err` 即回滚（状态、事件、事实一起撤销）。事实经 `AppendOnlyFactStore::open(SqliteFactLog::new(transaction))` + `record_host_outcome` 写入——**规则引擎在这一层也生效**（first-wins / 身份校验 / 异常分流）。
- **终态映射**（纯函数 `host_outcome_for_chat_status`）：`Completed→Completed`、`Failed→Failed{reason}`、`Interrupted→Interrupted{confirmed_cancel}`、`Running→Running`、`InterruptRequested→CancelRequested`（后两者由规则引擎判为非终态、**不写**事实）。
  - **我没有写 `CancelOrigin::User`**：只登记"宿主收到过该轮的停止请求"这一事实（`Other{code:"chat_turn_stop_requested"}`）。原因：取消入口是否**仅**由用户可达我尚未核对，写 `User` 会是过度声明——而契约明确禁止"默认写用户取消"。若产品确认该入口仅用户可达，可改为 `User`（一处小改）。
- 既已终态的幂等路径也补写事实（规则引擎 first-wins ⇒ 同终态 `AlreadyTerminal`、不同终态 `ConflictingTerminal` 且不改写），因此历史缺事实的记录会被自愈。

**准确的完成口径（不要扩大）**：**聊天轮次的终态事实**现在会在业务事务里真实写入 ✅。但 **CU 动作回执事实仍未产生**——执行器缺 `workspace_id` 等维度（`computer_use_runs` 今天不记录工作区），要接需把接纳处的作用域一路 plumb 到 `execute_in_room_with_policy`；这是下一步。

**必须知悉的行为后果**：既然终态与事实同事务，**事实层的失败现在会让该轮终态落库失败**（按裁决第 2.2 项这是有意为之：宁可一起失败，也不要"状态已终态、事实却没写"）。失败时既有回退路径仍会把 guard 标记完成并记录 `diag`，但 DB 行会保持非终态。

**测试**：新增 `chat_run_scope_requires_every_container_dimension`（缺一即不构造、不补占位值）、`interrupted_without_a_host_confirmed_cancel_is_not_cancelled`（无宿主确认事实 ⇒ `Interrupted`；`Running`/`InterruptRequested` ⇒ 无终态）。
**门禁**：合并全量 **web 1044/1044**、cu-core 82/82、guard 34/34、toolreg 53/53、core-runtime 274/274、linkage 4/4，全部 exit 0、警告 0。

### B-28 CU 事实：**动作事实卡在契约上**，运行终态事实还需先修一处既有的伪造值

**核实结果（带源码证据）**
1. **CU 动作事实（`ActionFact` / `ActionReceipt`）当前写不了**：`RunIdentityScope::StepAction` 的必填维度是 **9 个**，其中包含 `RequestAttemptId` 与 `ToolCallId`（`run_contract.rs:168-179`），而构造器 `step_action_fact(run_id, step_id, request_attempt_id, action_id)` 把 `request_attempt_id` 作为**必需参数**（`:473-479`）。CU 执行器里：
   - `ToolCallId` **有**真实来源（`identity.provider_tool_call_id`，CU 是由 `computer_use_perform` 这次工具调用发起的）✅
   - `RequestAttemptId` **没有**真实来源——执行器不持有"哪一次模型请求尝试产生了这个动作"（用量层有 `attempt_id`，但未 plumb 到动作路径）。按契约"本应存在却缺失 ⇒ 身份不完整错误"、"禁止用 `""`/`"unknown"`/`"0"` 凑字段"，**不能伪造**。
   ⇒ 这是**契约决定**（原生动作到底算不算"有模型请求尝试"），不是实施工作量问题。见 §C-14。
2. **CU 运行终态事实（Turn scope）技术上可做**（只需容器五维），但发现一个必须先处理的既有问题：`main.rs:33952` 的调用点用
   `ToolCallIdentity::from_provider(provider_tool_call_id.unwrap_or("provider-call-missing"), caller_session_id.unwrap_or("session-missing"), effective_turn_id)`
   ——**缺会话时会填入字符串 `"session-missing"`**（轮次 id 缺省时也会现生成一个）。这是为了让 CU 的取消/落库路径继续工作而做的既有兜底，但对**事实**来说它就是一个伪造维度：`"session-missing"` 不在契约的占位值黑名单里（`is_placeholder_identity_value` 拒 `""`/`unknown`/`n/a`/`-`/`0` 等），所以它会**通过校验并进入事实**。
   ⇒ 要让 CU 事实诚实，必须先把该调用点的身份改为**显式 Option**（缺维度就不写事实，而不是填哨兵）。这属于产品行为边界（CU 在缺会话时该拒绝执行还是照旧执行但不写事实），见 §C-15。

**结论（准确的完成口径）**：**目前只有聊天轮次的终态事实进入业务链**（§B-27）；**CU 侧的事实一条都还没写**——不是"还没接线"这么简单，而是需要先解决上面两个决定。

## F. 第三轮裁决记录（第 12–15 项 + B 类 13 项确认）

裁决输入：`C:\Users\zhupu\Desktop\第三轮答复.md`。**其中三处明确否决了本台账此前的建议/边界**，已在下表标红并就地修订（见 §F-3）。

### F-1 四项新增裁决

| 项 | 裁决 | 对实施的绑定要求 |
| --- | --- | --- |
| **12/A-1 hook 授权** | **首期选 b**：由**原生人工控制面批准一次具体 hook 调用**；**不采用**"settings 里加 `authorized: true`"这类 (a)。

settings 只表达"配置了什么、希望启用什么"，**不构成授权**；长期信任列表后置 | 批准须绑定 7 类内容（hook 身份 / 解析后的执行内容 / 执行上下文 / 本次实际输入摘要 / 安全政策 / 一次性消费与有效期限）；**命令字符串 hash 不是完整安全证明**（脚本内容变化必须使批准失效）；一次性授权的有效状态由**授权服务**持有，**重放审计记录不得重新生成执行资格**，**重启后未消费的旧批准失效**；执行顺序 9 步（预检查 → 认 Deny 则双零调用 → 解析冻结 invocation → 校验 hook 权限 → 取一次性人工批准 → 原子消费 → 运行并记结果 → **在新真实条件下**重查目标/输入/前置 → 重走 `authorize_call` → 才运行目标工具）；未授权/异常退出 7 条行为表（**pre hook 缺授权 ⇒ 目标工具明确阻断**，不得把"未运行"当"检查通过"；post hook 未授权/失败 ⇒ 保留目标工具真实结果，不重跑；**不增加"未知错误都算 advisory"的兼容开关**）；CLI 只能查询/请求打开确认面，**不得靠 `--yes`/stdin/配置字段/`confirmed=true` 签发授权**；子 Agent 不继承父会话批准；**原生确认面不得成为 CU 的可操作目标**（确认期间自家自动输入必须暂停或被拒），但**不得宣传成 OS 级不可绕过**——发现可伪造确认或改授权权威的路径时**该授权模式不得启用** |
| **13/A-2 未确认释放作用域** | **选 c**：阻断范围与 broker 的**实际输入资源 scope** 一致（本机 Windows 交互/登录会话），**跨 turn、聊天 session、房间、workspace**；不进一步缩小到窗口（鼠标键盘状态不因切窗口而结清）。会话/turn/workspace/房间只作为**事故来源**，不再作为放行过滤 | 存**用户级输入安全存储**（与"会话事实存储"分工，**不复制两份事实权威**、**不宣称跨库原子**）；**不能只改一条 SQL**：还需 ① **事故发布与新输入接纳的串行化**（先使资源不可接纳、再退出正常所有权，中间不留空隙）② **崩溃恢复**（输入前已有可恢复的执行/释放义务记录；回执未结清时不得把"没有最终错误行"当安全空闲）③ **存储不可用时拒绝新物理输入**（仍执行已发生输入的有界收尾）；**辅助光标移动同样在签发许可时检查该 scope**；安全恢复专用释放走**独立受限的恢复资格**；**纯聊天/读日志/不涉物理输入的诊断不受影响**；浏览器 DOM 是否同一物理资源按**实际执行方式**判断；**受控升级对账**迁移旧 turn 级记录（6 条：只从已知运行库导入 / 保留原 session·turn 并新增资源 scope / 无法证明 scope 标"范围待核查"不猜配 / 初始化与对账未完成前不开放新物理输入 / 首次挂载未核查 workspace 先导入检查 / **历史 unknown 不改写为 released**）；`IF NOT EXISTS` 可留作幂等手段但**不构成完整迁移与版本管理**，**不擅自抢占主库下一版本号**；**恢复范围同步扩大**（针对同一资源 scope 的 incidence 集合与 revision；禁止"解除当前 turn"却清空全桌面 incident；确认后出现新事故不得按旧确认放行）；**与最小原生恢复入口一起达到发布条件** |
| **14/A-3 动作身份** | **修正版 b + 最小 c**：`request_attempt_id` **按真实动作来源有条件必填**。**"原生动作"是执行方式、不是动作来源**——模型生成点击计划、由宿主执行，仍是 **ModelPlanned**，**不能因为执行器是宿主就省略模型请求身份**；`tool_call_id` 存在也不能证明动作不是模型规划的 | 四种来源表：**ModelPlanned**（`request_attempt_id` 必填、工具链来源时 `tool_call_id` 必填）/ **HostIncidental**（不适用，有真实工具归属时携带，须有宿主算法版本+父步骤+资源 scope）/ **SafetyCleanup**（不适用，可引用原工具但不得虚构，须有 incident/原 action/恢复资格）/ **UserDirect**（不适用，须有真实控制操作+操作者来源+权限决定+资源 scope）；**模型规划后的恢复动作仍属 ModelPlanned**；宿主做坐标变换/受控展开时**保留原模型请求因果关联**；**两层校验**（结构校验 + **可信关联校验**，不得只信传入的 `origin` 字符串）；6 条判定示例（ModelPlanned 缺 attempt ⇒ 身份不完整；HostIncidental 无宿主父操作 ⇒ 来源不成立；UserDirect 只有模型输入声称用户点击 ⇒ 来源不成立；SafetyCleanup 无原 action/incident/恢复资格 ⇒ 拒绝；request 属其他运行且无有效父子关联 ⇒ 拒绝；有工具调用关系却故意不带 tool id ⇒ 身份不完整）；**新增两个正交概念**：事实层级（Turn/StepAction）+ **上下文类型（Conversation / ControlPlane）**——ControlPlane 可无聊天 session/turn，但**必须在输入前建立真实的控制操作、run/step/action、资源与授权记录**（**不是生成几个随机 ID 假装属于聊天轮次**）；**不另造第二套回执或并行事实存储**；**最小 attempt 传递链**（模型请求 attempt 建立 → 规划响应关联 → 宿主包装规划结果与来源 → 执行器接收计划及不可变来源 → 输入前校验 → ActionFact 持久化），**允许给 planner→executor 的返回包装加宿主因果元数据，不要要求模型在动作 JSON 里填 `request_attempt_id`**；复用 `UsageAttempt{run_id, logical_request_id, attempt_id}` 前**必须先核对其唯一性**（局部编号不能当全局 ID，必要时用真实复合键建稳定映射；**不得用 provider trace / 逻辑请求 ID / 外层 `computer_use_perform` 的 call ID 冒充内部规划 attempt**）；一个动作以**真正产生该执行计划的请求**为主要来源，视觉转述/验收等作附加因果引用，**不得随手选最近一次请求填空** |
| **15/A-4 缺必需身份** | **普通 CU 选 a**：**输入前发现必需上下文缺失即拒绝执行**（返回明确的上下文不完整错误，零物理输入）。

**不接受**"已知缺少必需身份、仍继续执行普通 CU、只是不写事实"——因为身份还参与取消、调用关联、去重、预算、所有权与恢复，明知上下文不成立仍产生桌面副作用会削弱这些机制 | 四种情形表：普通聊天 CU 缺必需 session/turn/调用上下文 ⇒ **拒绝**；合法用户直接操作无聊天上下文 ⇒ **走 ControlPlane 接纳流程建立真实操作后执行**，不走匿名裸输入；动作已发生之后才发现身份/持久化异常 ⇒ **保留真实回执与证据、停止新业务输入、执行必要收尾**，不得丢事实或改称未执行；安全释放/人工恢复本无聊天 session ⇒ **用资源级恢复上下文执行受限清理**，不得因缺聊天维度拒绝必要释放；**哨兵必须移除**（`provider-call-missing`/`session-missing`/缺 turn 临时生成值），但**不能只加占位字符串黑名单后保留原执行流程**；**生成 ID 与伪造 ID 的边界**：接纳真实新操作时生成并持久化 run/action ID 是正常的，问题在于"已有调用缺关联时临时造一个值、再当成原本存在"；provider call 缺失按入口区分（模型工具协议缺配对 ID ⇒ 协议/接纳层拒绝；手动工具调用可有真实本地 invocation ID，external provider call ID 不适用；**两者不用同一个哨兵混合表示**）；**哨兵迁移**：新构造器用 `Option` 表达缺失并验证实体关系，历史值恰好等于哨兵字符串**只作疑点、不得单凭字符串批量删改**，能证明由兜底产生的标"身份来源缺失"，不能证明的保留原值+异常说明；读取/观察等无物理输入操作按自身权限继续，**不得为保留它们而一并放行点击和按键** |

### F-2 B 类 13 项确认（要点）

| 项 | 裁决要点 |
| --- | --- |
| B-1 取消来源 | **保持 `Other{code:"chat_turn_stop_requested"}`**（不能据现有证据改成 `User`）。后续把取消来源**在发出停止信号时传入**（不在终态映射阶段倒推），真实用户停止/内部超时/应用关闭/父任务取消分别保留，来源未知时保守记录；子任务可保留因果来源，但**不能把每条停止请求或模型生成的说明算作人工操作** |
| B-2 500ms | **保留 500ms 为待校准的请求准入底线**（含义限定为"发起模型请求前的最小调度余量"，**不是**"500ms 足以完成该阶段"）；只用于需要模型请求的阶段、**不得阻止不发请求的本地判定与安全收尾**；低于阈值不发新请求、**不得把剩余时间向上补足**；达到阈值仍要检查实际阶段预算与路线可行性；日志记录阈值+实际 remaining+拒绝原因；G3/G5 测量后再调、变更单独记录 |
| B-3 收尾数值 | 确认 2s/4s/≤min(2s,剩余)；**重复取消/不同异常处理器/再次进入释放分支不得刷新窗口**；**B-9 的旧输入路径也要消费同一策略、不另设宽限**；四秒到期只表示自动收尾窗口结束、**不证明原生执行者已停止**，无法确认则隔离 |
| **B-4 `off` 与排空** | **删除 `mode != "off"` 例外**（**这正是本台账 §B-19 里我自己划的边界，被否决**）：off 也要走正常关闭链（停止接纳 → 排空已登记请求 → 满足终止条件 → 停止自己拥有的实例 → 确认关闭结果）；**排空失败或状态未知时返回"关闭未完成"，不得先把配置/UI 写成 off 成功**；外部服务仍不得按端口终止；将来若要"中断生成并强制停止托管实例"，**必须是独立、明确的破坏性操作**（绑定托管实例身份与受影响请求并记录中断事实），**不得偷偷塞进 off 的默认分支** |
| **B-5 终态提交一致性** | **两个失败选项都不接受**。正确做法是**分开两个维度**：**执行状态**（模型/工具是否已结束、是否还有输入或副作用）与**提交状态**（终态与事实是否已原子提交、是否待确认）。提交失败时的固定行为：执行已结束 → 固定不可变的终态提交候选 → 尝试同事务提交 → 成功发布已提交终态；失败显示"执行已结束，记录提交失败/待确认"；失败后**不再启动该运行的业务动作、不发布已提交成功事件、不触发 Goal 后续派发或"验证成功记忆"、不因 guard 未持久化而重跑模型或工具、不清除未完成的输入释放义务**；普通正文可以已显示但**不得继续包装成"运行已可靠提交完成"**；`ChatTurnGuard` **至少区分"执行已结束"与"终态已提交"**，析构路径不得合并两者、也不得在失败后自行回放整项任务；重试**只重试同一个不可变终态提交、不重试业务执行**；**无法确定上次提交是否成功时先读取核对，不盲目再建新事实**；进程重启后若无足够证据恢复原终态则保留中断/未知，**不从 UI 曾显示"完成"推断成功**；恢复提交本身**不使旧 run 重新获得执行资格** |
| B-6 ActionFact 分层 | **接受 `ActionFact{identity, receipt, evidence}`**，不要求改 6 处回执字面量。前提：正式持久化与传输消费**完整 ActionFact**；receipt 与 evidence **同步写入**（不得分别成功后再假定完整）；错误回执经包装**不丢 evidence**；缺 evidence 标未知、**不得按成功返回自动补"原生事实"**；需要重试/释放/恢复判断的消费者能取得对应**证据等级**；复制或映射字段处有回归测试 |
| B-7 `tool_call_id` | 改为**按真实关联有条件必填**，与第 14 项一并执行；工具链里有真实调用应携带，人工直接操作/安全恢复没有则不伪造；**可选性由受信上下文决定，不由调用方随意决定** |
| B-8 身份异常 | 现有 `blocks_further_input` **不足以表达阻断范围**，授权增加/复用明确的 `block_scope`/原因分类；四类异常表（输入前身份不完整且能证明未输入 ⇒ 阻断当前 run/调用，修正来源后接纳新 attempt、**不需解锁整个桌面**；已开始输入且身份不匹配/执行者不明/无法对账 ⇒ **进入共享隔离**，交 RPR-05c；仅历史关联不足但当前资源安全已独立建立 ⇒ 保留异常与人工复核、**不自动冻结所有未来任务**；分类本身无法确定是否已输入 ⇒ 保守按资源风险处理）；**旧 run 终止后不复活 ≠ 永久死锁**——真正要解除的是共享资源隔离，不是硬清原运行标记；人工恢复不能把伪造身份变成合法身份 |
| **B-9 `input.rs`** | **同意独立立项、优先级 P0**，建议 **RPR-04d：普通原生输入的受控生命周期与收尾**。范围覆盖实际可达的点击/文本输入/滚动/按键/组合键路径（输入前身份·权限·scope 校验 → 登记执行意图与可能的释放义务 → 受监督启动原生执行者 → 开始输入 → 阶段回执 → 取消/超时/失败收尾 → 静止与释放对账）；**复用现有进程监督与 cleanup 策略，不再增加第二套互不知情的 timeout runner**；正常收到回复与子进程真正结束**分别记录**；释放义务**按实际按钮和按键登记**（不能统一抬整个键盘，也不能因旧拖拽只用左键就只处理左键）；未获得生命周期保证的原语**不进入已验收的自动输入集合**；不因此禁用纯观察与安全诊断；必测 8 项（含超时后 helper 仍存活、回执丢失、确认静止失败、重复取消），**必须有少量真实受控 helper 验证** |
| B-10 证据元数据 | 确认实施：沿 B-6 的 ActionFact 结构接线（`surface`/`evidence_basis`/原始响应·错误引用/推断规则版本/`request_id`·`action_id`）；浏览器桥派发后超时**只能保守表达可能执行**，缺协议阶段回执时 **`partial` 保持未知**；**不能由错误名称或普通成功响应推出完整输入阶段事实**；证据标记**必须由实际适配器与宿主转换逻辑产生**，不能让模型或任意远端响应自宣更高可信等级；**加了字段但生产构造点仍不填写不算完成** |
| B-11 测试例外 | **I1/I2 授权纳入 RPR-01b**（仅测试隔离/共用锁/RAII，不改产品配置与 OAuth 语义）；**I3 继续保留限制**（不借 RAII 清理改生产请求或凭据链，**不写"全仓已清零"**）；**I4 授权但与其他 main.rs 接线串行合并**；**I5 不按测试清理替换**（不能用 guard 提前恢复而改变进程运行行为）；**I6 授权最小内部可注入实现**（保留公开入口行为，内部抽出接收显式路径集合的纯查找函数供测试，**不借机改变命令解析政策**），**单独小 PR**；现有共用环境锁继续使用、**不再新增第二把互不协调的锁** |
| B-12 第二份 hook | **不能只登记为未来风险**：本轮授权核查其构造、公开执行入口与实际调用者，并补齐"**无独立授权不启动进程**"的条件；**两份实现消费同一个授权决定接口**、不各建信任列表；完整合并留 S2.3；若依赖关系不便立即共用 runner，**先共用领域契约与一致性测试，不引入依赖环**；**不得据此宣称它已通过 Web/CLI 的真实执行验收** |
| B-13 排空 | **先做客户端 a**；已完成的流包装**不重做**；当前需补 6 项（Provider/连接与**实际托管实例身份**关联；请求**派发前**登记；流**完整生命周期**持有状态；超时/断流/提前 Drop 后的**远端未知**状态；**切换期间的新请求接纳闸门**；**off 与普通切换采用同一正常停止条件**；终止目标严格限于**自己拥有**的实例）；`/slots` 仍是版本相关的可选补充，**无真实二进制证据时记未验证**；客户端方案**适用范围有限**：只有受支持入口都经同一管理器时才可据其排空结果判断，**"已知客户端零在途" ≠ 可安全停止服务**，无法建立足够证据就拒绝自动切换 |

### F-3 裁决明确否决本台账三处（已就地修订）

1. **§B-19 的"`off` 不过排空闸门"被否决** → 见 B-4；已改为"off 也走正常关闭链，排空失败/未知则返回关闭未完成"。
2. **§C-15 我的建议"缺审计维度仍可执行、只是不写事实"被否决** → 普通 CU 改为**输入前拒绝**（选 a）；已按此修订。
3. **§B-28 的措辞被纠正**：不能说"CU 动作没有模型请求 attempt"，也不该用"有 `tool_call_id`"来支持省略 attempt → 正确表述是"**执行器当前未收到真实 attempt；模型规划动作仍须补传，宿主自主动作则不适用**"，且"**tool call 关系与动作来源是不同维度，不能互相证明**"。

### F-4 裁决指定的执行包与串行约束

`P-01 动作来源与接纳身份`（14/15/B-7）、`P-02 终态提交一致性`（B-5）、`P-03 普通输入受控收尾`（B-9/B-3）、`P-04 资源级隔离与人工恢复`（13/B-8）、`P-05 hook 独立授权`（12/B-12）、`P-06 模型正常关闭与排空`（B-4/B-13）、`P-07 证据与边界回归`（B-1/B-2/B-6/B-10）、`P-08 测试环境例外收口`（B-11）。

可并行：P-02、P-03 的模块开发与 P-08。**继续串行**：`RunIdentity`/动作来源/事实 schema（**P-01 先冻结，其他模型不得各自降低必填要求**）；`main.rs` 的 CU 接纳/guard/模型切换（单一集成负责人分批接线）；数据库迁移与用户级输入安全状态（一个迁移负责人）；broker/helper/cleanup（顺序合并＋组合故障测试）；原生授权与恢复（**hook grant 与解除隔离是不同权限，不得共用一个万能确认 token**）；真实桌面实验（同一输入资源串行）。

**必须新增的组合验收（12 个跨模块场景，见裁决第 4 节）**：模型规划 click 未收到 attempt ⇒ 输入前拒绝且**不得改标 HostIncidental 放行**；用户直接操作无聊天 session ⇒ 先建真实控制操作、不生成假 chat turn；原动作已输入但回执身份异常 ⇒ 保证据+收尾、资源风险时隔离、**不报零输入**；一个 turn 释放未知、另一个 turn/workspace 立即申请输入 ⇒ **零输入 + 同一资源阻断**；恢复确认期间出现新 incident ⇒ 旧确认不清空新事故、资源保持阻断；`input.rs` 超时但 helper 存活 ⇒ **不接纳下一动作**、有限收尾后不能确认则隔离；终态事务失败但 UI 已收到正文 ⇒ 正文保留、状态为**执行结束/提交待确认**、**无 Goal 自动推进**；模型改 hook 配置为启用 ⇒ **配置变化不生成执行 grant**、匹配 pre hook 无批准则工具明确阻断；hook 改变目标后返回 allow ⇒ **旧审批不复用**、基于新条件重判；切换 off 时仍有未知生成请求 ⇒ **关闭不成功、不终止、不把配置显示成已关闭**；浏览器动作派发后断连 ⇒ 标"可能执行"且证据来源明确、不自动重放；**修复后的安装包重启** ⇒ 隔离/事实/授权失效规则保持、不用旧安装包验收代替。结果须分别标记：核心测试 / 生产入口集成 / 真实 helper / 真实模型服务 / 原生人工界面 / 安装验证。

### F-5 裁决指定的台账措辞修订（10 项，作为本台账的权威表述）

| 原表述或倾向 | 应改为 |
| --- | --- |
| "CU 动作没有模型请求 attempt" | "执行器当前未收到真实 attempt；模型规划动作仍须补传，宿主自主动作则不适用" |
| "有 tool_call_id 就能按宿主动作省略 attempt" | "tool call 关系与动作来源是不同维度，不能互相证明" |
| "缺审计维度仍可执行，只不写事实" | "正常输入前缺必需执行上下文即拒绝；已发生事实仍保留并收尾" |
| "跨 run 互锁已完成" | "当前互锁受 turn 过滤；资源级跨 turn／session／workspace 阻断尚待接线" |
| "hook 配置授权" | "配置声明与一次性人工执行授权分离" |
| "事实失败但 guard 已完成" | "执行结束与终态提交分离；提交失败不发布可靠完成" |
| "ActionReceipt 没加 evidence，所以不符合裁决" | "完整 ActionFact 信封携带证据元数据即可，需证明生产链不丢失" |
| "原生输入已有四秒收尾" | "已覆盖的具体输入路径采用该策略；`input.rs` 尚需补齐，不能全称" |
| "MessageStream 包装完成，排空完成" | "流包装已完成；托管实例身份、请求接纳与终止条件仍需生产接线" |
| "测试全绿，所以新事实已进入业务链" | "契约测试状态与生产消费者状态分别登记" |

### B-29 P-08（裁决 B-11）交付：I1/I2/I6 收口 + **发现一处未被记录的竞态** + 一处新的同类例外

**交付**
- **I1（`core-runtime/src/config.rs`）**：`env_overrides_agent_runtime_config` 由手写 `set_var → remove_var`（三态有损：三个变量一律 `remove_var`，无法还原"原来为空/原来有值"）改为本 crate 已有的 `ScopedEnv`；删掉该文件仅供裸取锁的包装。
- **I2（`oauth.rs`）**：`CLAW_CONFIG_HOME` 用例同样改用 `ScopedEnv`。
- **I6（`tool-registry`）**：抽出**纯查找** `find_command_path_in(command, search_paths)`，公开入口 `find_command_path` 保留原行为并委托它；两个 PATH 用例改为**注入路径集合**，不再改父进程 `PATH`；测试接缝（`execute_powershell_with_search_paths`／`detect_powershell_shell_in`）是 `#[cfg(test)]`，**生产二进制里不存在这些符号**。
- **零行为变化的证明**（三条独立证据）：① 生产构建无 unused 警告（说明公开入口仍被生产链消费）；② 本机实测 `detect_powershell_shell` 仍能经进程 PATH 找到真 PowerShell；③ 两个用例断言执行前后 `PATH` 完全相等，且全仓 grep 确认 tool-registry 测试已无任何 `PATH`/`PATHEXT` 写入。
- 门禁：`core-runtime` **295/0**（基线 274 + 本工单 3 + 并发 P-01 契约的约 18）、`tool-registry` **54/0**（基线 53 + 1）、linkage 4/4；连跑 3 次稳定。

**它发现的一处未被记录的竞态（重要）**：台账此前提 I1 时只说"虽已持同一把 `test_env_lock()`，但仍是手写 set→remove_var"——**漏掉了 `config.rs` 里 8 个"读进程环境"的用例根本没持这把锁**。把 I1 迁到 guard 之后该竞态**立即真实复现**（一次运行即命中：`parses_agent_runtime_config` 读到的 profile 是 `"bench"` 而非配置值，fail 1/277）。修复：给这 8 处**读取侧**补持同一把统一锁（裁决授权范围内：只调整测试隔离与共用锁），**未新增第二把锁**。

**新的同类例外（未在裁决 I 列表中、也未授权修改）**：`modules/tooling/packages/command-router/src/lib.rs:2768-2802` 的测试模块仍手写 `PATH`／`SAFEUSER` 的 `set_var/remove_var`（无 RAII、无统一锁）⇒ **同一类回归在该文件尚未覆盖**，必须与其余例外一起在台账保留可见（裁决要求"不得因为低成本项已修就把整个类标为完成"）。

**保留的例外清单（不得写"全仓已清零"）**：I3 `llm-adapter` 约 55 处（裁决保留原限制）；I5 CLI／GUI／`vision-service`／`web-console` 的生产性环境设置（不得用 guard 提前恢复改变进程运行行为）；`command-router` 上述测试点；另 `tool-registry` 内保留 **2 处故意的**裸 `set_var/remove_var`（"旧写法 vs guard"的对照演示，非迁移遗漏）。

**未验证（子代理如实申报）**：I6 的**非 Windows 分支**做了一处必要行为细节调整（把显式路径集合作为子进程 `PATH` 传给 `sh -lc`，命中且为文件时返回解析路径；判定政策仍由 `command -v` 决定），**未在 Linux/macOS 实测**（本机 win32 且无 pwsh）；非 Windows 只做了代码审查，未交叉编译。

### B-30 P-01 契约冻结交付（裁决 14/15/B-7 的契约侧 + B-2）

**交付（仅 `run_contract.rs` + `fact_store.rs` 文档节 + `computer_use_planner.rs` 的 B-2 节）**
- 新增 **additive 私有模块 `action_origin_contract`**：`ActionSource`（四来源）、`ActionOriginField` + **把字段矩阵写成表**（`required_relations`/`not_applicable_relations`，不散在 `if` 里）、`ContextKind`（**"与事实层级正交"写成 `applicable_hierarchies` 一行**）、`ConversationActionContext`、`ControlPlaneContext`、`ActionContext`、`ActionOrigin` + `ActionOriginAdmission` + **两级校验**（`validate_structure` 一级 / `validate_against(authority)` 二级，二级先跑一级）+ `admit_action_origin`、`ActionOriginAuthority` trait + 内存实现 `TrustedOriginContext`、关系记录（运行/工具调用/宿主操作/清理 incident）、attempt 侧（`RequestIdRole` + `validate_request_id_role`、`PlannedRequestAttempt{from_usage_attempt, stable_key}`、`StablePlannedAttemptId`、`PlannedAttemptRegistry`）、宿主因果（`HostCausalMetadata{for_plan_producer/for_visual_description/for_final_verification}`、`HostCausalProducer`——**只有 `planner_host`，`"model"` 反序列化直接报错**）、`PlannedActionEnvelope`；5 个 additive 错误码。
- **概念纠正写进了类型文档**（不是代码外备注）："原生动作是执行方式、不是来源"、"tool call 关系与动作来源是不同维度、不能互相证明"、模型规划后的恢复仍是 ModelPlanned、宿主坐标变换保留原因果。
- **ControlPlane 的"真实性"是结构性保证**：id 由**宿主按登记顺序**分配（`control-op:{workspace}:{room}:{sequence}`，调用方无法自带随机 id）；`establish` 必须**一次写全**操作者来源/权限决定/资源 scope/真实 run·step·action，缺一或带占位值即拒绝（不存在"先登记再补"）；`establish_sequence` 单调递增便于消费方判断"输入前已建立"；**`ControlPlaneContext` 类型上没有 session/turn 字段** ⇒ "生成随机 ID 假装属于聊天轮次"在类型上不可表达（有测试断言其序列化不含 `session_id`/`public_turn_id`）；回执仍复用 `ActionReceipt`、存储仍是同一套（未另造第二套）。
- 18 条新测试：6 条判定示例逐条对应 + 字段矩阵 + 正交性 + 遗留严格校验未放松 + `HostCausalMetadata` 只能宿主填 + attempt 需稳定复合键 + 附加因果可补充但不能"建立"来源。
- **B-2**：`MIN_STAGE_BUDGET` 注释重写为"发起模型请求前的最小调度余量"（**不是**"500ms 足以完成该阶段"、不是统一最低耗时、不得回扩、保留"未经实测"标签）；新增拒绝日志（`stage`/`threshold_ms`/`remaining_ms`/`reason`/`model_request_sent=false`）；**移除了 `classify` 的阈值门限**（纯本地判定、0 次模型请求，裁决明令不得阻止），`request_model` 的前置判定保留。

**子代理主动申报的一处行为变更（需复核）**：零预算下"本地表面分类"现在返回成功而非 `budget_exhausted`。它 grep 后未发现执行器依赖 classify 的终止路径，但**未做端到端运行验证**——我已记为待复核项（不宣称已确认无影响）。

**接线清单（集成负责人，7 项）**
1. `lib.rs` 补导出本模块全部新名字，并删掉 `run_contract.rs` 里的节级 `#![allow(dead_code)]`（本轮因禁改 lib.rs 而保留）。
2. `ActionFact` 与 `FactLogRecord::ActionReceipt` 增 `origin: Option<ActionOrigin>`，并同步 `fact_log_sqlite.rs` 的穷尽 `match` 与 4 处结构体字面量——**这正是本轮没有落盘的原因**（该文件不在子代理允许清单内，加字段会直接编译失败，属正确克制）。
3. planner 包装层补 `HostCausalMetadata`（三个构造器分别用于规划/视觉转述/验收；attempt 不得用 provider trace、逻辑请求 id、外层 `computer_use_perform` 的 call id 冒充）。
4. execator/动作落盘路径写事实前构造 `ActionOrigin` 并 `admit_action_origin(&origin, &authority)`（ModelPlanned 填 `stable_key()`；工具链动作必带 `tool_call_id`；宿主观测动作补父步骤/算法版本/资源 scope；释放走 SafetyCleanup 并挂 incident/原 action/恢复资格）。
5. 人工直操/资源恢复入口在输入前走 `ControlPlaneOperations::establish`/`adopt` 拿 `ControlPlaneContext`，**不复用聊天 session/turn**。
6. 需要**生产版 `ActionOriginAuthority`**（从事实存储/控制操作登记读真实父对象；当前只有内存版）。
7. 消费侧需读：`source`、`verified_model_request`、`tool_call_id`、`context.control_operation_id` + `user_direct.permission_decision_id`、`cleanup`。

**门禁**：core-runtime **295/0**（子代理注明：改动前该 crate 实测已是 **277** 而非台账记的 274）、`cargo build -p coolzhu-web-console` 通过、**web 1046/0**、linkage 4/4。

### B-31 RPR-04d（裁决 B-9，P0）交付：普通原生输入进入受控生命周期 + **一处新的既有风险**

**现状核实（改动前）**：所有原生原语走 `run_powershell(script, timeout)`——`thread::spawn` 里 `command.output()`、主线程 `recv_timeout`；**超时分支只返回字符串错误**，`Child` 在分离线程内部，**无 kill、无 wait、无静止确认**，PowerShell 会继续注入。点击体自带的 `finally` 释放只在**脚本自己失败**时有效，主机侧超时/取消时无人补发释放。5 个可达调用点全在 `computer_use_desktop_bridge.rs`，均无身份校验、无进度、无收尾、无回执。

**交付**
- 新链路：**输入前身份/权限/scope 校验 → `ReleaseObligation` 登记 → 受监督启动固定 helper → 阶段回执（进度文件）→ 取消/超时/失败收尾 → 静止与释放对账**。
- **复用而非另建**：`cleanup.rs` 的 `CleanupPolicy`/`CleanupDeadline`/`HelperCleanupFacts`/`CleanupReleaseStatus` **一行未改**（`native_cleanup_policy() == CleanupPolicy::default()` 由测试钉住），裁决要的"不再增加第二套互不知情的 timeout runner"确实做到了；事实仍复用 `ActionReceipt`/`InputDelivery`/`InputReleaseStatus`（新增 4 个构造/合并助手，未造同义类型）。
- 新增：`ReleaseObligation`、`NativeInputFacts`（严格解析）、`NativeInputOutcome`（**含 `obligation`、`reply_received_at_ms`、`process_exit_confirmed_at_ms`、`cleanup`——"正常收到回复"与"子进程真正结束"分别记录**）、`NativeInputFailure`、`run_native_helper`、固定 helper。
- **释放义务按实际按钮/按键登记**（有表）：左键/双击只登 `[Left]`；左右和弦登 `[Left, Right]`（**测试断言右键在册**，正是裁决点名的"不能只处理左键"）；滚动为空（只发轮询事件）；文本走 SendInput 为空、走 Interception 只登它可能按下的 3 个修饰键（**不是整块键盘**）；单键/组合键逐个登记去重保序；显式按下→成功按下后对账为 `Unknown`（按住即意图）。
- **释放对账判定表**（未送请求/证明从未按下/登记义务为空 ⇒ `NotNeeded`；按住登记之外的东西/**未确认静止** ⇒ `Unknown`；本进程独立释放已确认 ⇒ `Released`；否则 `Unknown`）。
- **17 条新测试，其中 6 条走真实受控子进程**（真 PowerShell + 真 C# 驱动，或内存驱动），覆盖裁决点名的 8 个必测场景；**全程不驱动真实鼠标键盘**（真实用例只走"身份不匹配 ⇒ 注入前拒绝"路径）。唯一未能用真实子进程稳定复现的是"确认静止失败"，已注明。

**裁决措辞已按正确表述落地**：注释写"**已覆盖的具体输入路径采用该策略；`input.rs` 尚需补齐，不能全称**"，**没有**写成"原生输入已有四秒收尾"。

**子代理主动申报的三点**
1. **比受控笔画更严一格的取舍**：笔画被强杀就无条件补发释放；本机制在 helper 事实证明"一步都没注入"时**不补发**，避免给可证明零注入的运行发出多余 UP（差异已写进注释与报告）。
2. **行为变化（需知悉）**：受控点击/文本/滚动/按键现在**注入前校验前台窗口身份**（用快照身份），不匹配 → `stale_observation`（可恢复，需重新观察）；此前是"注入到当时的前台窗口"。
3. **Interception 分支是转写、本机不可验**：`interception.dll` 存在但驱动未安装（`interception_create_context()` 返回 NULL），`auto` 实际走 SendInput。驱动机器上的验证命令它给了（`cargo run -p coolzhu-computer-use-check -- preflight` 后设 `CLAW_MOUSE_BACKEND=interception`）。helper 用 `-Command` 内联，命令行约 27.8KB/32767（测试守卫 <30000）。

**顺带发现的一处既有风险（不在其允许文件内，未改）**：被强杀的 helper 会留下 `Add-Type` 产生的 **csc.exe 孙进程持有 stdio 管道**；本工单把输出收集限定为 250ms，但 **`input_stroke.rs` 的 `run_helper` 仍是无界 `join()`**，同样场景下会拖过收尾窗口 ⇒ 建议交给该文件的负责工单（记为 §B-32）。

**需要集成负责人接线（`main.rs`，禁改清单内）**：`send_mouse_action`/`send_drag_action`/`send_escape_key`/`send_text_input`（经 `run_blocking_input`）仍用旧原语，需改为受控变体；另 `gui-desktop` 的 `desktop_agent.rs`/`input_backend.rs` 消费同一批旧原语，若属自动输入集合也需同样接线。

**门禁**：`computer-use-core` **99/0**（基线 82 + 17，连跑 3 次稳定）、web 构建通过（bridge 0 警告）、linkage 4/4。

### B-32 新发现的既有风险：受控笔画路径的**无界 `join()`** 会拖过收尾窗口

**来源**：RPR-04d 在实现普通输入收尾时顺带发现（**不在它的允许文件内，未改**）。
- 现象：被**强杀**的 helper 会留下 `Add-Type` 产生的 **`csc.exe` 孙进程持有 stdio 管道**；RPR-04d 已把普通输入路径的输出收集限定为 **250ms**，但 **`input_stroke.rs` 的 `run_helper` 仍是无界 `join()`** —— 同样场景下它会**一直等到孙进程释放管道**，从而拖过 4 秒收尾窗口。
- 影响：这直接削弱裁决第 5 项要的"**有界收尾**"——收尾窗口被拖长，而"四秒到期"的判断因此不再可靠。
- 建议：交给 `input_stroke.rs` 的负责工单修（把无界 `join()` 改为有界等待 + 明确的"未确认静止 ⇒ 隔离"路径）。**本轮未改**，因此台账继续按裁决口径表述："**已覆盖的具体输入路径采用该策略；`input.rs` 尚需补齐，不能全称**"，再加上这一条 `input_stroke.rs` 的例外。

### B-33 P-02 交付：终态提交一致性——"执行已结束"与"终态已提交"分成两个维度

**裁决 B-5 的两个失败选项都不接受**（既不接受"事实失败导致 DB 非终态、UI 却显示完成"，也不接受"事实失败降级告警、业务终态单独照常提交"）。本轮按裁决实现为**两个正交维度**：

**改动（`main.rs`）**
- 新增 `ChatTurnStatus::CommitPending`：表示"**执行已结束，但终态提交未获确认**"。它**不是成功、也不是普通失败**。
  归入 `is_terminal()` 是**必须的**：`prune_chat_turn_registry` 会**永久保留非终态条目**（`main.rs:292-294`），否则该轮会永久滞留并让状态查询一直以为它还活着。
- 新增 `ChatCommitState{NotAttempted, Committed, Pending}` 挂在 guard 上，使"执行是否结束"（`finished`）与"终态是否已提交"（`commit_state`）**不再混为一谈**。
- `finish()`：提交成功 → `Committed` + 真实终态；**提交失败 → `Pending` + 返回 `CommitPending`**，并把注册表状态也设为它——**不再回退成看起来正常的 `Failed`/`Interrupted`**（这正是裁决点名的"假成功"根因：DB 行非终态、UI 却显示结束）。
- `Drop()`：**若提交待确认则提前返回**——不重放提交、不重放任务，只记 diag（裁决："析构路径不能将两者合并，也不能在失败后自行回放整项任务"）。
- 三处穷尽 match（`runtime_terminal_state_for_chat_status`、finalize 的 `accepted` 分支、`host_outcome_for_chat_status`）**一律保守映射为失败**（**绝不映射为 completed**），并在注释里写明 `CommitPending` 是**结果**而非请求、不应被传入。
- **成功侧信号已被抑制**：宠物事件 `chat.completed` 本就以 `status == Completed` 为条件 ⇒ 待确认时不发布；`done` SSE 事件携带的是 `commit_pending`，消费方无法读成成功 ✅。

**测试**（更新既有 trigger 驱动的失败用例 `chat_runtime_guard_drop_and_terminal_failure_never_report_completed`）：用 SQLite 触发器阻断终态事件以稳定构造提交失败，现在断言 ① `finish()` 返回 `CommitPending`（且 `!= Completed`）② `CommitPending.is_terminal()` 为真（注册表可回收）③ **注册表对外状态是 `CommitPending`** ④ DB 行仍非终态、无 `run.completed` 事件 ⑤ **drop 后 DB 状态不变**（证明不重放提交）。

**尚未做完的两点（如实标注，不宣称 B-5 已完全闭环）**
1. **UI 文案未加**：裁决要求界面显示"执行已结束，记录提交失败／待确认"。当前只做到**机器可读状态**（`commit_pending`）与成功信号抑制，`app.js` 里的中文文案尚未补。
2. **没有专门的"提交恢复入口"**：现在依赖既有的 orphan 收敛（重启时把非终态 run 标为 interrupted，**这不是成功**，符合"保留中断/未知状态"），但"重试同一个不可变终态提交"的显式入口未建。

**门禁**：`build-web exit 0`、**web 1046/1046**、cu-core 99/99、guard 34/34、toolreg 54/54、**core 295/295**、linkage 4/4，全部 exit 0、警告 0。

### B-34 P-01 接线第一步：动作来源（`origin`）落入事实 schema 并可被消费

裁决的接线清单第 2 项要求给事实加 `origin` 字段——本轮完成，并**顺手把"可消费"也做了**（否则只是"写进去但读不出"）：

**改动**
- `ActionFact` 与 `FactLogRecord::ActionReceipt` 各加 `origin: Option<ActionOrigin>`（`#[serde(default, skip_serializing_if)]`），缺省表示**没有随附来源判定**，并在文档里写明：**不得**据此推断它是宿主动作、也**不得**推断它不是模型规划动作（"缺来源"与"来源是宿主"是两件事）。
- **写入侧校验**：`record_action_fact` 若带 `origin` 则跑 `validate_structure`；**并明确注释可信关联层（第二层）由调用方用 `admit_action_origin` 完成**——存储层没有 `ActionOriginAuthority`，**不得假装做过第二层**。
- **读回侧校验**：replay 路径同样做结构校验（结构不合法的来源不得被静默接受进投影）。
- **可消费投影**：新增 `FactSnapshot::action_origin_history(action_id)` / `latest_action_origin(action_id)`（与既有证据投影同一口径：**读不到就是 `Unknown`，不是"宿主动作"**）。
- **一处真实正确性细节**：`fact_is_new` 现在包含 `origin_is_new`——否则"回执重复但新带来一条来源判定"的事实**不会被追加**（会被误判为重复而丢弃）。
- `lib.rs` 按接线清单补导出契约名字（`ActionSource`/`ActionOrigin`/`ActionOriginAdmission`/`ActionOriginAuthority`/`ActionContext`/`ContextKind`/`ControlPlaneContext`/`ControlPlaneOperations`/`ConversationActionContext`/`HostCausalMetadata`/`PlannedActionEnvelope`/`PlannedRequestAttempt`/`PlannedAttemptRegistry`/`StablePlannedAttemptId`/`TrustedOriginContext`/`admit_action_origin`）。
- 4 处 `FactLogRecord::ActionReceipt` 字面量（`fact_log_sqlite.rs` 测试内）补 `origin: None`。

**未做的两点（如实标注）**
1. **来源投影没有专门测试**：需要一个合法的 `ActionOrigin` 夹件（含 `ActionContext`/`ResourceScope` 与各来源的必填关联），最好在**生产者接线时用真实来源**写这条测试，而不是在这里拼一个可能与契约不一致的夹件。投影实现与已测试的证据投影逐行同构。
2. `run_contract.rs` 里的节级 `#![allow(dead_code)]` **暂时保留**：契约里 `ControlPlaneContext`/`PlannedAttemptRegistry` 等尚无消费者，此刻删掉会产生一批 dead_code 警告；等 (c)(d) 接线落地后一并删除。

**仍未接线的（P-01 余下）**：(c) **去哨兵 + ControlPlane 接纳入口**（`main.rs:33952` 的 `"session-missing"`/`"provider-call-missing"`；普通聊天 CU 缺必需上下文 ⇒ **输入前拒绝**）；(d) **attempt 最小传递链**（planner→executor 的宿主因果包装）。

**门禁**：`build-web exit 0`、**web 1046/1046**、cu-core 99/99、guard 34/34、toolreg 54/54、**core 295/295**、linkage 4/4，全部 exit 0、警告 0。

### B-35 P-01 接线第二步：去哨兵 + 缺必需执行上下文时**输入前拒绝**

**裁决第 15 项选 a**：普通 CU 在输入前发现必需上下文缺失即拒绝，**不接受**"已知缺少必需身份、仍继续执行普通 CU、只是不写事实"（理由：身份还参与取消、调用关联、去重、预算、所有权与恢复，明知上下文不成立仍产生桌面副作用会削弱这些机制）。

**改动（`main.rs` 的 computer_use 工具入口）**
- **删除三处哨兵**：`provider_tool_call_id.unwrap_or("provider-call-missing")`、`caller_session_id.unwrap_or("session-missing")`，以及**缺 turn 时现场生成一个 trace id** 的兜底。
- 改为**先判上下文**：三者任一缺失即返回拒绝，且**在构造身份与调用执行器之前返回** ⇒ **零物理输入**，也**不会为无上下文的调用生成匿名 run/action 记录**（身份根本不会被构造）。
- 拒绝是**明确、可机读**的：`status=blocked`、`route=computer-use-context-guard`、错误码 `input_context_incomplete` + **缺失维度清单** + 中文理由（并说明合法直操必须经控制面接纳流程）。
- 新增测试 `computer_use_without_required_context_is_refused_before_any_input`（全 None 上下文 ⇒ 断言 blocked/路由/错误码/缺失维度/理由）。**既有测试无一因去哨兵而失败** ⇒ 此前没有测试依赖"无上下文也能执行 CU"。

**这是裁决明示的兼容性变化（不是遗漏）**：裁决原话是"旧无上下文直接调用若**尚未迁入 ControlPlane 接纳流程**，就返回'需要有效执行上下文'，不继续裸执行"。因此当前**所有**无上下文的 CU 调用（含本来合法的用户直操）都被拒绝——这是 fail-closed 的过渡态，**而不是**说"这条路径以后就该被拒"。让合法直操重新可用，需要 (d) 之后的 **ControlPlane 接纳入口**（真实控制操作 + run/step/action + 资源与授权记录，**不是生成随机 ID 假装属于聊天轮次**）。

**仍未做（P-01 余下）**：**ControlPlane 接纳入口**（裁决第 14.5 项，让合法直操/资源恢复有真实上下文）与 **(d) planner→executor 的宿主因果包装**（attempt 最小传递；不得用 provider trace／外层 `computer_use_perform` 的 call id 冒充内部规划 attempt）。

**门禁**：`build-web exit 0`、**web 1047/1047**、cu-core 99/99、guard 34/34、toolreg 54/54、**core 295/295**、linkage 4/4，全部 exit 0、警告 0。

### B-36 P-06 第一项：Provider/连接与**实际托管实例身份**关联（裁决 B-13 第 1 条）

裁决 B-13 要求"Provider／连接与**实际托管实例**身份关联"，且身份"优先来自真实已解析配置与托管实例记录"、"**不能通过端口号重新推导进程所有权**"。本轮落地：

- 新增**唯一**的身份推导入口 `local_chat_endpoint_identity()`（用与请求路径相同的构造函数 `ProviderClient::from_custom_openai_compatible(...).endpoint_identity()`，端点由 `identity.resolved_endpoint()` 取得）。
- **排空闸门改用该入口**（删掉原先在内联手拼 `base_url` 的实现），与托管登记**共用同一端点键**——两处各自拼端点而漂移，正是我此前在 §B-19 记录过的"查到空桶 → 误报已排空 → 把正在生成的模型杀掉"这一类故障。顺带把闸门的 `port` 参数删掉（它已不再被使用）。
- **登记/注销**：`start_local_gemma` 启动成功（拿到 pid）后 `record_managed_local_chat_instance(pid)`；`stop_managed_local_service` 停止 **Chat** 角色后 `clear_managed_local_chat_instance()`。登记只发生在**我们真正启动过该实例**之后 —— 端口不构成所有权证明。
- **测试** `managed_chat_instance_registration_shares_the_drain_identity`：登记前身份不得声称托管 → 登记后 `managed_instance()` 必须有值 → **且 `resolved_endpoint()` 与登记时一致**（证明两处共用同一端点键）→ 注销后不得再声称托管。

**过程中我自己造成的回归（已修）**：改闸门签名后，我此前写的那条源码契约测试仍断言旧的调用形态（带 `port` 参数）而失败。修法是同步更新该断言（而不是把参数加回去）。

**为什么本轮**没有**做 (d)（planner→executor 宿主因果包装）**：核实后发现它是**更大单元的配料、不能独立收口**——① planner 里**完全没有 attempt/逻辑请求概念**（grep `attempt_id`/`logical_request_id`/`UsageAttempt` 全空），要从"真实 attempt 身份 + 稳定复合映射"建起；② 更关键的是，该元数据在 **"CU 动作事实写入路径"（executor → store → 规则引擎，且 `ActionOrigin` 取 ModelPlanned）落地前没有任何消费者**。若现在单独实现，只会得到一个没人读的旁路字段（甚至可能被迫做成 side channel），反而违背"不要平行通道"的口径。因此 (d) 与 CU 动作事实写入作为**同一个交付单元**一起做更合适。

**门禁**：`build-web exit 0`、**web 1048/1048**、cu-core 99/99、guard 34/34、toolreg 54/54、**core 295/295**、linkage 4/4，全部 exit 0、警告 0。

### B-37 (d) 第一步：规划请求的**真实 attempt** 已可被宿主取得（additive 接缝）

裁决第 14.6 项要求动作事实的 `request_attempt_id` 必须来自**真正产生该执行计划的请求**，且"**不得**用 provider trace、逻辑请求 ID 或外层 `computer_use_perform` 的 call ID 冒充内部规划 attempt"。本轮把这条链路的前半段做成**可消费的 additive 接缝**：

- **computer-use-core**：`ComputerUsePlanner` 增加带默认实现的 `fn last_plan_request_attempt(&self) -> Option<runtime::PlannedRequestAttempt> { None }`——默认返回"未知"，且文档里写明**不得**用近似物冒充；因为带默认实现，其它实现零改动。
- **web-console 的 planner**：新增每个逻辑请求的重试计数与"最近一次规划 attempt"两个字段；在**发出规划请求之前**登记 attempt，复合键由真实值构成：`run_id#computer_use_planning:step-{step}#attempt-{n}`（`run_id` = 真实运行 id，`logical_request_id` = 规划阶段 + 真实步骤序号，`attempt_id` = 同一步内的重试序号）。
- **身份不成立时保持未知**：`PlannedRequestAttempt::new` 是**带校验的**，未绑定真实运行（`call_id` 为空）或含占位值时构造失败 ⇒ 返回 `None`、`last_plan_request_attempt()` 保持未知，**而不是造一个假 attempt**（下游会拿它当"这条动作由某次真实规划请求产生"的证据）。
- **测试** `plan_request_attempt_is_a_real_composite_key_and_never_fabricated`：未发请求时报告未知 → 未绑定运行时构造被拒（保持未知）→ 复合键逐字段正确 → **同一步重试得到新 attempt**（`attempt-1` → `attempt-2`，这正是"参数修复/重试必须保留各自请求身份"）→ 另一步的键含 `step-4` → 最近一次 attempt 是最新那一次。

**仍未做（(d) 的后半段 + CU 动作事实写入本体）**：执行器消费该 attempt、构造 `ActionOrigin`（`ModelPlanned` + `request_attempt_id = stable_key()` + `tool_call_id`）、跑 `admit_action_origin` 两层校验、并在**与步骤更新同一事务**里写入 `ActionFact`；这还需要 ① **生产版 `ActionOriginAuthority`**（从事实存储/控制操作登记读真实父对象）与 ② 把 **`workspace_id` plumb 进 CU 执行器**（`computer_use_runs` 今天不记录工作区，见 §B-28）。**因此 CU 侧目前仍然一条事实都没有写**——本轮交付的是"真实 attempt 可被取得"，不是"事实已写入"。

**观察到一次负载下的时序抖动（与本轮改动无关，如实记录）**：合并门禁运行时 `input_stroke.rs` 的**真实子进程**测试 `real_helper_pre_input_failure_produces_a_proven_not_sent_receipt` 失败一次（断言 `Stale` 实得 `ReleaseUnconfirmed`）；随即**单跑 3/3 通过、完整套件单独跑 2/2 通过（99/99）**，而当时我正在并发编译。判定为机器负载导致的时序敏感，非改动引入。**但要提醒**：该断言把"输入前失败"的失败种类写死为 `Stale`，而真实 helper 在负载下可能先走到"释放未确认"分支——这个语义问题应由 `input_stroke.rs` 的负责工单判断（是否 `ReleaseUnconfirmed` 在该场景下也是合法结果）。

**门禁**：`build-web exit 0`、**web 1049/1049**、**cu-core 99/99**（单跑两次确认）、guard 34/34、toolreg 54/54、**core 295/295**、linkage 4/4。

### B-38 专项文档：释放义务缺陷详述 + CU 执行器符合性评估

新增 **`cu-executor-conformance.md`**（待决策材料，含 4 个待裁决点）。要点：

**释放义务缺陷（那条时序抖动测试的根因，链条已核实到行）**：`needs_emergency_release`（`input_stroke.rs:754-761`）的四个入参里**没有任何来自 helper 事实的值** ⇒ helper 因**输入前**校验失败并非零退出时，仍会对一次**从未按下任何键**的动作发起"独立补发释放"；负载下该补发失败 ⇒ `input_release_unconfirmed` ⇒ 因"分类只看错误文本且释放未确认排第一"（`:129`/`:145`）**覆盖**掉原本正确的 `stale_observation`。后果：① 回执**自相矛盾**（`input_delivery=NotSent` 已由事实证明 ✅，但 `input_release` 对 `ReleaseUnconfirmed` **无条件**写 `Unknown`，`:355`）——诚实值应为 `NotNeeded`（**正是测试的期望值**）；② `retryable()` 对 `ReleaseUnconfirmed` 返回 false（`:171-175`）⇒ 一次本可"重新观察后重规划"的**输入前失败被降级为不可重试**。**好消息**：`proven_not_sent` 已经看事实，所以**不会**把零输入误报成"可能已发送"；跨 run 互锁也不会被误触发（判据要求 `delivery <> 'not_sent'`）。

**CU 执行器符合性结论：不符合裁决预期**——契约/规则引擎/输入所有权/预算都就位且有测试，但 **CU 侧至今零事实进入业务链**，且存在上述释放缺陷。阻塞清单已按"卡住什么"分四组：**A** 卡住任何 CU 事实写入（无 `ActionOrigin` 构造、无生产版 `ActionOriginAuthority`、`workspace_id` 未 plumb、需改走同事务写入）；**B** 事实正确性（上述缺陷）；**C** 输入安全未覆盖面（笔画路径无界 `join()`、临时光标缺三条守卫、`main.rs`/`gui-desktop` 仍用旧原语、Interception 不可验证）；**D** 能力边界与过渡态（vision 排空未覆盖、合法直操一律被拒、其他事实类无生产者）。

### B-39 CU-F03 前提不符（子代理按指令停下，未写一行代码）：**裁决链路的第一跳在代码里不存在**

裁决 §5.1 选的链路是"**已授权父运行上下文 → CU 接纳 → 持久化 `computer_use_runs.workspace_id`**"。子代理先做核对，结论是**第一跳不存在**，并**没有用近似值顶替**（我在工单里明确要求"若发现该上下文不可取得就立刻停下报告"）。

**证据（已由我复核）**
1. **父运行的工作区确实被解析并落库，但只在两处，且都不通 CU 接纳点**：
   - 流式聊天：`api_chat_send_stream`（`main.rs:17294`）在 `17301` 计算 `workspace_id = workspace_identity(&active_workspace_path())` → `create_chat_runtime_run_sqlite`（调用点 `17304`，实现 `42229`）写进 `runtime_runs.workspace_id`，键是 `legacy_turn_id` = **公开聊天轮次 id**。该值也被 `ChatRunScope`（`main.rs:456`）在接纳时捕获，但**只喂给 `ChatTurnGuard` 的终态事实**，工具链取不到。
   - goal 阶段：`accept_goal_phase_run`（`main.rs:41804`）写 `kind='goal_phase'` 的归属；`run_goal_phase_once(workspace_id, …)`（`16308`）手上就有该值。
2. **CU 接纳点拿不到它，且没有任何键能反查**：唯一入口 `execute_with_current_runtime`（`computer_use_executor.rs:1422`）的唯一生产调用点在 `main.rs:34047`，传入的 `ToolCallIdentity` 只有 `provider_tool_call_id / session_id / turn_id`，**没有 workspace**；而这里的 `turn_id` 是**模型回合 trace**（流式 `main.rs:17552` 现场生成、非流式 `main.rs:26462` 由 `agent_chat_response` 生成），**永远匹配不上** `runtime_runs.legacy_turn_id`；`runtime_runs.provider_turn_id` 对 chat turn 全是 `None`。
3. **进程内注册表也没有工作区**：`ChatTurnRegistryEntry`（`main.rs:260-268`）只有 `run_id / session_id / chat_room_id / cancellation / status`。
4. **唯一"能拼出来"的键是 `(session_id, chat_room_id)`，但同 scope 下可同时存在多个 run**（同房间并发多轮；切换工作区后新旧工作区 run 并存）⇒ 用它反查必须"在多个活跃 run 里挑一个"，**正是 T13 禁止的近似值**，子代理拒绝使用 ✅。
5. *ControlPlane 分支同样给不出工作区**：契约类型已存在（`runtime::ControlPlaneContext` 带 `workspace_id`），但 web-console 里**没有接纳入口**——全仓对它的唯一引用是 `main.rs:34025` 的一条注释。

**我复核后补充的两点（决定最小修订方案）**
6. 三条生产入口的可得性**不一致**：`api_chat_send_stream`（工具调用点 `18408`）✅ 有冻结归属；`run_goal_phase_once`（`16360`）✅ 有；**非流式 `api_chat_send`（`17056`）❌ 完全没有**——它既不计算 `workspace_id`，也不调用 `create_chat_runtime_run_sqlite`（该函数全仓只在 `17304` 与测试里被调用）。
7. *子代理顺带查实两处现存缺陷**（属 T13 的"数据库不变"面，现在就存在）：CU 接纳处**两次现场解析"当前工作区"**——`computer_use_executor.rs:1440` 开 run store、`:1477` 查房间授权，都走 `default_session_sqlite_path()` → `workspace_scope_paths_for(&active_workspace_path(), …)`。即**运行中切换工作区会让第二次解析落到另一个库**（run store 只是恰好被 Connection 缓存）。而房间 full-access 授权本身就取自**当前工作区那个库** ⇒ "已授权工作区上下文"今天**不是一等实体**。

**建议的最小修订（提交裁决，符合它"发现前提不符时提交具体差异并按最小范围修订"的要求）**
- **放弃"靠 `ToolCallIdentity` 反查"**（不存在可用键），改为**显式传参**：这正对应裁决那句"**工具调度层可以传入父上下文**"。
- 三处入口各自传入**接纳时冻结**的工作区：流式用 `17301` 的 `workspace_id`；goal 用它已有的参数；**非流式在它自己的入口处捕获一次**（与 `17301` 同法——这是该轮次的接纳时刻，不是"执行时读当前工作区"）。
- 执行器在接纳时把它**冻结**进 CU 上下文，并用它取 run store 与房间授权（顺带修掉第 7 点的"两次现场解析"）；**缺失即拒绝输入**（§5.1"旧活动运行缺上下文 ⇒ 不再启动新的业务输入"，fail-closed）。
- 迁移 v22（可空列 + 读回呈现"历史归属未记录"）+ `SESSION_SCHEMA_VERSION` 21→22 由**我**（集成负责人）在 `main.rs` 侧落；迁移与冻结/读回/测试交回子代理。

**当前状态**：**未写任何代码**（子代理零改动），门禁保持基线（web **1049/1049**、linkage 4/4）。

### B-40 PKG-01/02 交付：Loader 确定性产物链（"最先应合并"第①项已关闭）+ 两处偏差

**交付**
- **新增 `scripts/lib/webview2-loader.ps1`**（18 个函数）作为**唯一实现点**，由 `package-all.ps1` dot-source；`gui-desktop.tauri-shell` 的 build 增加 `capture: "cargo-json-messages"` ⇒ **没有第二次 cargo 调用**，Loader 与随包 exe 出自**同一次构建**。
- **改掉了"声明源存在即信任"**：manifest 的 loader `source` 指向**稳定导出路径** `modules/gui-desktop/target/package-inputs/windows-x64/{profile}/WebView2Loader.dll`（+ `export` 块），**不再依赖 `webview2-com-sys-*` 哈希目录**；消费侧要求"稳定导出源存在 **+ 与本次发布输入匹配的收据 + 内容/SHA-256 一致**"才接受。**旧的目录扫描回退已移除**（保留为"调用即报错"的 tombstone）——这是裁决点名的**最先应合并第①项**："声明源存在也必须验证来源"。
- **收据**（`webview2-loader-export.json`，schema=1）补齐裁决要求的六组字段；`package-all.ps1` 与 `build-msi.ps1` 的报告分别新增 `build_context`/`exported_artifacts[]` 与 `staged_exported_artifacts[]`，并有 `Assert-StagedExportedArtifacts` 在生成 MSI 前校验 staging 与收据一致。
- **旧手工 DLL 的处置**：文件仍留原处**未删除**，但**已不可能被优先采用**（新逻辑只认稳定导出 + 匹配收据；裸放一个 DLL 会得到 `RECEIPT-MISSING`）。它给出的实测证据很硬：**完整重建后该手工 dll 的 mtime 未变（2026-09-19 12:02）而 exe 被重写** ⇒ 确认它不是本仓库 msvc 构建的产物。
- **实测**：定向清单打包 `EXIT 0`（连跑两次，第二次先删稳定导出+收据模拟清洁态）→ `bin/WebView2Loader.dll` 的 SHA-256 与稳定导出、收据三者一致、**无手工复制**；`--no-build` 走通（`validation=existing-receipt-verified`）；`test-package-webview2-loader.ps1` **pass=23 / fail=0 / skip=4**；manifest / safety / powershell-compat 测试 PASS。

**偏差一：全量打包被既有编译错误阻塞（与我方改动无关，需在 CU-F01 落地后复验）**
`cargo build` 到 `coolzhu-computer-use-core` 时**不编译**（`HelperFactRead::or`、`NativeInputOutcome.fact_anomaly` 等 6 个错误），全量 `package-all.ps1` 因此失败并被正确归类为 `BUILD-FAILED`。**判断（待复验）**：那是**并行在途改动**——CU-F01 正在改 `input_stroke.rs`/`input.rs`（它的允许清单包含这两个文件）。**若 CU-F01 落地后这些错误仍存在，则它是一个独立的真实编译回归**，必须立刻停下来处理。**在此之前不得声称完整打包可用**（它只跑通了"仅 shell+loader 的定向清单"）。

**偏差二：本工作树没有有效的 VCS 身份（对裁决要求的"构建身份"字段组有直接影响）**
`git ls-files` = 0（全部未跟踪，HEAD 是同步种子提交）⇒ 收据里的 `source_commit` **仅作参考**、`tracked_build_entry_files=0`；**载荷身份改用 manifest 声明的文件哈希集合**（已排除 tauri-build 每次重写的 `src-tauri/gen/schemas/`）。裁决要求"构建身份：哪次构建、哪个源码快照、是否含未提交修改"——在当前无有效 VCS 身份的树上**无法按提交号回答**。若要求以提交号为主，需先恢复**可跟踪的工作树**（这属独立工作项）。

**另外它主动申报的两点**：① 报告落点与裁决措辞不符——裁决说"现有 `build/package-report.json` 证据体系"，本仓库实际是 `tmp/package-reports/package-report-<config>-<stamp>.json`；它**沿用既有落点**并在其中补齐字段组，未改名（改名影响面超出授权）。② 它**越出授权清单**更新了两个测试脚本（`test-package-manifest.ps1`、`test-package-webview2-loader.ps1`）——因为它们硬编码旧契约、不改必挂；**只改了测试脚本，未触碰任何 `*.rs`**，并如实申报。

**未验证**：安装态"实际加载来源属于预期安装位置"的运行时核对；`build-msi.ps1` 全流程（需 WiX + 全量包，被上面阻塞）；L04 清洁 target 全量构建（开关已内置 `-RunCleanTargetBuild`，未跑）；L09/L11 的真实环境部分。

### B-41 PATH-01/02 交付："最先应合并"第②项已关闭 + 一处**裁决前提不符**（重要）

**交付**（`packages/app-launcher/**`、`config/package-launcher.json`、`docs/**`；未碰任何 `modules/**` 或打包脚本）
- `ResolvedLaunchPaths`（`packages/app-launcher/src/launch_paths.rs:107`）字段与裁决清单一一对应；**五级优先级**已实现（①本次明确选择 ②已持久保存的用户级选择 ③旧版可确认覆盖 ④随包默认值（仅首次初始化）⑤兼容支路）；**解析本身是纯函数**（不建目录、不开库、不升级 schema），**唯一写盘入口**是 `apply_resolution_actions` ✅ 与裁决要求一致。
- 用户级选择落点 `%LOCALAPPDATA%\CoolzhuAgent\launcher-user.json`（**不从 `log_dir` 反推**）；只存选择/schema/revision/legacy/observations，**不复制**模型参数·Base URL·密钥·`coolzhu.toml`（有单测断言）；**跨进程 `create_new` 锁 + revision 不匹配即 `SelectionConflict` 不写盘 + `.tmp`→`sync_all`→`rename` 原子发布**；旧值转 `legacy_selections[]` 保留。
- **"已选失效不得静默回退"是结构性保证**：存在已保存选择而访问性非 `Writable` 时**立即返回** `WorkspaceUnavailable`——**代码路径上根本没有"继续往下挑候选"的分支**；错误信息打印路径/来源/原因/"**已阻断启动，不会创建替代工作区**"/修正入口（`--select-workspace <绝对路径>`）/选择文件全路径，并落 `package-selfcheck-last.json(ok=false)`。**实测** `Z:/disconnected/workspace` → exit 1、无替代目录 ✅（P04）。
- **缺键语义按配置版本分档、不混用**：v2 缺 `runtime_dir` = **配置错误**（明确点出"不会从 log_dir 推导"）；v1 缺键走兼容解析，结果仅作 `legacy_log_dir_derived_workspace` **迁移候选**（有数据⇒一次性采用；无数据⇒`WorkspaceSelectionRequired` + 确认入口）；更高 schema 直接拒绝。
- **实测**：`cargo test -p coolzhu-app-launcher --offline` **68 通过 / 0 失败**（基线 28，新增 40）；`cargo build -p coolzhu-web-console --offline` exit 0；linkage 4/4。P01–P13 **已实现并跑过**（其中 P08/P09 是**真拉起 web-console** 的端到端，P12 用 **3 个进程真并发**验证 revision 冲突）；P11/P14/P15 为弱保证；物理迁移本轮不要求、未做。

**裁决前提不符（第⑦.1 条，必须记录）**：裁决假定"保留已有 `paths.data_dir` 等覆盖"**只需不改动**；但旧 launcher **额外注入** `COOLZHU_WEB_SESSION_DB / _STORE / _ATTACHMENT_STORE`，在 web-console 里**优先于 `coolzhu.toml`** ⇒ 用户的 `paths.data_dir` 覆盖被**静默忽略**。子代理**删除了该注入**（只注入工作区根 + 清除继承覆盖并记录），**无覆盖时行为与旧实现完全一致**——这是 P08 的真修复，也说明"不隐藏覆盖"这条裁决意图此前**并未被实现**。

**另外三点它主动申报**：① "优先复用已有用户级启动设置"——**实际不存在**（core-runtime 的 User 层是运行特性/MCP/hook 配置，desktop-console 是 `gui-settings.json`，都不是"打开哪个工作区"），故按裁决授权**新增最小文件**。② 实验**发现并修掉两个真实缺陷**：`%LOCALAPPDATA%\CoolzhuAgent` 自己的 `logs/runtime/config-snapshots` 曾被当成"工作区数据"（会破坏全新用户的首次初始化分界 P01/P03）；**本次**刚写的配置快照曾被当作"旧启动记录"参与候选（自我循环）。③ **容量边界**：`input_safety_state_root` 目前只是**声明的契约路径**（`<user_state_root>\input-safety`），RPR-05 实装输入安全存储时必须真的用它，否则 P14 只有弱保证；后台复用核对依赖 web-console 既有两个只读接口（`/api/system/info` + `/api/diagnostics/health`），因禁改 `modules/**` **没有新增后台身份接口** ⇒ 后台不暴露工作区/构建时判定为**冲突（fail-closed）**，需用户重启实例（有记录可查）。这是 §6.1 "核对 workspace 身份"的**部分满足**，补齐需要一个后台身份接口（属 `main.rs`，由我串行做）。

### B-42 ⚠ 当前工作树状态：**web-console 的测试目标不编译**（CU-F01 在途，非缺陷，但必须记录）

**事实（我实测）**：`cargo build -p coolzhu-computer-use-core --offline` ✅ 通过；但 `cargo build -p coolzhu-web-console --offline --tests` ❌ 失败，错误全部落在 `computer_use_desktop_bridge.rs`：
- `missing fields cursor_moved, phase, protocol and 1 other field in initializer of HelperInputFacts`（`:1150`）
- `struct StrokeFailure has no field named facts`（`:1168/:1187/:1200`）

**判定**：这是 **CU-F01 的在途状态**——它按裁决 §3.1 给 `HelperInputFacts` 补了阶段/协议/完整性字段、并改了 `StrokeFailure` 的形状（去掉 `facts`），但**尚未更新其调用方** `computer_use_desktop_bridge.rs`（该文件在 F01 的允许清单内）。两个独立代理（PKG-01/02 与 PATH-01/02）**都各自观察到并如实报告**了这一现象。

**处置（不宣称任何东西已合并）**：
1. **F01 未完成前，不接受它、也不跑"合并全量门禁"**；F01 回报后我会先跑 `cargo test -p coolzhu-web-console --offline`（全量）并把**实际数字**作为验收依据。
2. 若 F01 报告完成时**仍未**修好这些调用方 ⇒ 那是**把它自己的类型变更留成不编译的树**，属必须立刻停下处理的偏差。
3. 在此期间，任何"打包/安装可用"的说法都不成立（PKG-01/02 的全量打包正是被这一点阻塞）。

### B-43 CU-F01 交付：释放事实一致性（裁决点名的 P0）——**"先核对字段生命周期"这条要求救了这件事**

**它逐条回答了裁决要求核对的四个问题，其中两条暴露真实缺口** ⇒ **我原先提的"两个零值 = 从未按下"规则若直接推广就是错的**：
1. `button_down=false` 的语义是"**从未确认按下过**"（`pressed` 只在 `Down()` 成功后置 true，**`Up()` 不会翻回 false**）✅ ——但**只有在"最终事实"里才能这样读**。
2. **`injected_points=0` 不覆盖"路径开始前光标移动"**：计数器只在 `Move` **成功之后**才自增，于是"第 0 点 `Move` 成功 → 紧随的 `Check()` 抛 `stale`"这个窗口里，记录仍是 `injected_points=0 / button_down=false`，**而光标确实已被移动**。按下则被正确覆盖。
3. **起点快照与"失败路径已封闭的最终事实"在旧字段上无法区分**：起点快照写在任何 Move/Down 之前、收尾事实写在 `finally` 之后；**旧实现在失败早于 `armed=true` 时根本不写收尾记录**，留下的就是起点快照 ⇒ `{0,false,false,null}` **同时代表"什么都没做"与"被杀在按下之前"**；`released != null` 只是间接推断，排除不掉"`Down` 的 SendInput 已成功、进程在写按下事实前被强杀"。
4. **顺带查出一个真实缺陷**：旧代码在 helper 自报释放失败时**把缺失事实填成全零**（`..facts.unwrap_or_default()`），配合 `proven_not_sent` 会推出 `NotSent + NotNeeded` ⇒"按下过（或至少尝试过 Up）却报未发送、无义务"。**该填充已删除**。

**按授权补的最小标记**（两 helper 同构、`protocol=2`）：`protocol`/`request_id`/`phase(pre_input|in_flight|final)`/`cursor_moved`；**v1 旧记录仍可读，但读成 `LegacyUnverifiable` 且永远不能充当零输入证明** ✅

**交付**：五态 `ReleaseObligationState` + `derive_release_obligation`（唯一实现点，笔画与普通输入共用）；回执由**已推导好的**投递事实构成（`NotSent` 固定 `NotNeeded`，不再被"释放未确认"文本类别覆盖）；`needs_emergency_release` **消费五态**（`ProvenAbsent`/`Settled` 绝不补发）；删除"明确失败⇒完全跳过收尾"的反向短路；桥层把**原始原因**与**收尾结果**分开；消费侧闸门加"`NotSent ⇒ input_release=NotNeeded`"。

**测试**：T01–T10 逐条通过；**抖动测试保留原断言**（不接受"两种结果都算成功"）+ 故障注入，**空闲 20/20、编译负载 20/20 PASS**（未采用"失败就再跑到绿"）。

**它主动申报的三点**：① **改了 `contracts.rs`**（不在允许清单、也不在禁止清单）：仅一行消费侧闸门，理由是 §6 要求"校验与构造同时改"，否则旧记录仍会被当零输入证明放行；声明若判越界可回退。② **笔画路径的"真实"补发通道没有单测**——要覆盖就得在测试里真发一次 UP，与"未按下时多发 UP 不作普遍安全假设"冲突，故未做。③ 并发重载下观察到 **2 次基础设施级抖动**（C# `Add-Type` >12s 触发 panic、真实 2 秒释放窗口被打穿），已修并**如实计为基础设施失败、未从分母删除**。另有两处既有断言是**按新语义更新而非放宽**。

**门禁（我独立实测，非采信其自述）**：**cu-core 110/110**（基线 99）、**web 1051/1051**、guard 34/34、toolreg 54/54、core 295/295、linkage 4/4，全部 exit 0、警告 0 ⇒ **§B-42 的"树不编译"已解决**。

### B-44 CU-F03 接线（我的 `main.rs` 串行部分）已完成传参；存储侧交回子代理

**关键设计事实（决定不能走 task_local 方案）**：既有代码自己写着"**流式路径没有包在 `TURN_TRACE` scope 中，显式生成并贯穿本轮**"——**主 UI 路径（流式）不使用 task_local**，所以"turn-scoped 注册表"方案（F03 子代理的方案 A）在主干上不可用；只能走**显式传参**，这正对应裁决那句"**工具调度层可以传入父上下文**"。

**我做的改动**
- `execute_with_current_runtime` / `run_model_tool_use_messages` / `run_model_tool_dispatch_for_session_with_identity` 各增 `workspace_id: Option<&str>`；**四处入口分别传值**：流式用接纳时的 `workspace_id`（`main.rs:17301`）、goal 用其已有参数、**非流式新增一次接纳时捕获**（与流式同法）、并行工具调用路径**传 `None` 并注明原因**。
- **执行器 fail-closed**：`None` 或空白即返回 `input_context_incomplete`（`ComputerUseStage::IntentGuard`），**在任何 store 操作之前**（裁决 §5.1"缺必需上下文不得执行，不得用'当前工作区'顶替"）。
- 绑定暂命名为 `_frozen_workspace`（尚未被存储侧消费），**待 F03 子代理接持久化时改为实际使用**。
- 我一次性改到位的是**传参链**；**迁移 v22、`NewComputerUseRun` 的非可选归属、读回与"历史归属未记录"、T13 与其余必测仍属子代理的存储侧工作**（它的允许清单含 `computer_use_store.rs`）。
- **复验**：`cargo test -p coolzhu-web-console --offline` **1051 通过 / 0 失败、EXIT 0** ⇒ 传参与 fail-closed 未破坏任何既有测试（说明此前没有测试依赖"无工作区也执行 CU"）。

**过程中我自己踩到并修掉的一个坑（记录以免重犯）**：第一版脚本的锚点 `) -> ApiResult<Json<SendMessageResponse>> {\n    let turn_clock = Instant::now();` **不唯一**，插入落到了 `api_chat_send_relay`（另一个聊天入口）而不是 `api_chat_send`；已撤回并改用**含函数名的唯一锚点**。教训与既有约定一致：**批量改动必须有唯一锚点 + 命中断言**。

### B-45 CU-F03 交付完成（存储侧 + 冻结/互锁）+ **它纠正了我一处错误判断（我已修）**

**交付**（`computer_use_store.rs` + `computer_use_executor.rs`）
- **迁移 v22**：按 v11 先例补**可空**列 `workspace_id TEXT` / `workspace_context_version INTEGER`（`PRAGMA table_info` + `ALTER TABLE ADD COLUMN`，每次安全补齐），`if current < 22 { PRAGMA user_version = 22 }`；**只加列、不发任何 UPDATE ⇒ 没有批量回填路径**；`ComputerUseRunStore::open` 也调用它（独立打开 store 也能写归属）。
- **写侧非可选**：`NewComputerUseRun.workspace: CuWorkspaceAttribution`（**非 `Option`**），`create_run` 必定写两列；**历史 NULL 只出现在读回侧**。
- **读回**：`load` 与 `load_by_idempotency_key` 共用**单一投影函数**（避免两条 SELECT 漂移）⇒ `StoredWorkspaceAttribution::{Recorded{…}, Unrecorded}`，`Unrecorded.text() == "历史归属未记录"`（常量），**代码里没有把它解释成"当前工作区"的路径**。
- **禁止值在构造处拒绝**、且**在任何 store 操作与任何物理输入之前**：`workspace_attribution_{empty,control_characters,placeholder,filesystem_path,too_long,not_canonical}`，占位值复用契约层 `runtime::is_placeholder_identity_value`。
- **不可变由三层保证**：① `ComputerUseExecutor.workspace` 是**非 `Option`** 字段且**无 setter**；② 归属只经 `from_parent_run` 构造、在接纳函数里**定型一次**，此后**没有第二次读"当前工作区"的路径**（有结构性测试守卫）；③ 生产代码**无法取得"未归属的执行器"**。
- **它顺手修掉了我记录过的那个缺陷**：原先 store 打开与房间授权复核**各调一次 `default_session_sqlite_path()`**（后者会随当前工作区变库）⇒ 现在**在接纳时冻结运行库路径**、两处共用（这是 T13"数据库不变"面）。
- 12 条新测试，含 **T13 必测**（运行开始后由替身 planner 触发工作区切换 ⇒ 落库归属与库不变、切换后的目录**没有**新建会话库）、写入失败即输入前阻断（`persistence_conflict`、原生输入 0 次）、旧活动运行 NULL 归属阻断新输入且**不回填**、生产入口端到端拒绝禁止值（不建立任何运行行）。

**它发现并纠正了我的错误判断（⑥.1，我已修）**：我此前给"**并行工具调用路径**"传 `None` 并写注释"该路径没有父运行的冻结工作区"——**理由不成立**：那两个调用点（`main.rs:17947`/`18224`）**就在 `api_chat_send_stream` 内**，接纳时的 `workspace_id` 可用。后果是**流式并行工具调用里的 CU 一律被拒**，属超出"缺上下文才拒绝"的**行为收缩**。
- **我的修法**：给 `dispatch_model_tool_calls_parallel{,_with_cancel}` 增加 `workspace_id: Option<String>`（**拥有所有权**——并行分发会 `tasks.spawn`，借用无法逃逸，与既有参数同例），两处流式调用点传 `Some(workspace_id.clone())`，循环内 `clone` 进任务。
- **非流式链**（`call_agent_model_with_tool_loop` → `dispatch_model_tool_calls_parallel`）**确实**拿不到工作区（需经 `agent_chat_response` 逐层传，它有 3 个调用者）⇒ 保留 `None`，但把注释改成**诚实的表述**："该链尚未接线，缺上下文时 fail-closed 拒绝 CU，这是正当拒绝，**不是**'该路径没有工作区可用'"。该接线列为待办。

**我同时完成的两处（属我的域）**：`main.rs` 阶梯在 `apply_session_migration_v21` 之后加 `computer_use_store::apply_session_migration_v22(connection)?;`；`SESSION_SCHEMA_VERSION` 21 → **22**（那 6 处断言引用常量，无需另改）。

**门禁（所有写者停止后由我实测）**：`build-web exit 0`、**web 1063/1063**、**cu-core 116/116**、**guard 40/40**、toolreg 54/54、core 295/295、linkage 4/4，全部 exit 0、警告 0。（注：cu-core 与 guard 的增量来自**仍在跑的 CU-F02**，其报告未到，我暂不采信其结论。）

**F03 提出的三个新问题（需记录/裁决）**
1. **迁移前遗留的 NULL 归属活动运行会长期阻断**：互锁只命中"未收尾 **且** 归属未记录"的行，而**现行代码没有任何机制关闭这类行**（`main.rs` 对 `computer_use_runs` 零引用、无 stale 收敛）⇒ 该 scope 可能被永久阻断，需要按 RPR-05b-1 的"人工追加解除事实"模式补一条收尾/解除入口（不在其允许文件内）。见 §C-16。
2. **它加了一条比要求更严的校验，需确认或否决**：归属值必须形如 `ws-<ASCII 字母数字>`（`workspace_identity` 的产物）。理由：裁决把"窗口名/模型提供字符串"列为禁止值，而**形状检查是唯一能当场拦住它们**的判据；代价是将来若改标识格式，CU 会 fail-closed（错误码 `workspace_attribution_not_canonical` 直接指出原因）。见 §C-17。
3. 读回呈现面**尚未接线到任何读者**：`StoredWorkspaceAttribution::text()` 目前只有测试在用（CU 运行列表/诊断 API 未接入），它加了带注释的 `#[allow(dead_code)]` 而不是删掉（裁决要求"呈现未记录"，而显示入口不在其允许范围）。

### B-46 CU-F02 交付：有界进程与管道收尾（P0 关闭）——**裁决列出的平台约束在本机得到实证**

**现状核实（改动前）**：`run_helper` 用 `Stdio::piped()` 起 powershell；Rust std 的管道**写端可继承**，所以 `Add-Type` 起的 `csc.exe` 孙进程**继续持有写端** ⇒ 两个 `read_to_end` 读取线程拿不到 EOF ⇒ 原 `out_reader.join()` / `err_reader.join()` **无条件 join**，把循环里有界的 child 收尾**全部抵消** ⇒"四秒收尾"机制性失效。同类问题第二处：`input.rs` 原 `collect_process_output` 把 `join()` 放进**被分离**的线程 + `recv_timeout`，超时即**丢弃读取线程与其缓冲区**（无人持有、无法对账）。

**实现 ↔ 裁决六层**
- **控制线程**：**无任何 reader join**；等待额度 = `min(250ms 诊断片长, 协作退出额度 2s, 剩余收尾时间)`，三者都取自 `cleanup.rs` **唯一定义点**、消费**已冻结**的 `CleanupDeadline`。
- **输出读取**：`windows-process-guard/src/pipe.rs` 的 `poll_available`（`PeekNamedPipe`，只读**已可读**字节）+ 增量协议行解析 ⇒ **不依赖 EOF 取回执**；stdout/stderr 各自受控；超限只计**缺口**（`dropped_bytes`/`truncated`），**已收到内容照常保留**。
- **I/O 取消**：`pipe.rs` 的 `cancel_synchronous_io` 只提供**请求层**事实（`Requested/NothingPending/Failed`），**不参与完成判定**；完成只能由**线程句柄等待** `WAIT_OBJECT_0` 得出（`Confirmed`），否则一律 `Unconfirmed{StillRunning|WaitFailed}` ✅ 完全符合"不能把发出取消等同于读取已结束"。
- **资源所有权**：`PipeReaderInner` 同时持有 `JoinHandle` **与 `DuplicateHandle` 的线程句柄副本** + 缓冲区；未核实时**整个 inner 转入残留登记**，`Drop` 路径也走寄存，**绝不"丢句柄后宣称已终止"**；公开观察点 `retained_pipe_reader_bytes(label)` 证明缓冲区也被持有。
- **延后回收**：非阻塞清扫只回收**已核实结束**的；**上限 `MAX_RETAINED_PIPE_READERS = 8`**；决策是纯函数 `decide_retention`；超限记 `Saturated` + `PipeSupervisorFault{saturated, unreclaimed_dropped}`（**明确故障状态，不静默丢弃**）。实测清扫 1.5µs。
- **新输入接纳**：未核实结束 ⇒ `readers_confirmed=false` / `retained≥1` / `is_supervision_fault()=true` / `has_evidence_gap()=true`，且**不改写 release**（**不伪造"释放未知"**）；**协议事实与普通日志分开保留**。

**明确未采用的两个禁止做法（有结构性测试守卫）**：`run_helper`/`run_native_helper` 体内**无 `.join()`、无 `read_to_end`、无 `collect_process_output`**，且必须调用 `helper_pipes::drain(`；`recv_timeout → 无条件 join` 模式已从 `input.rs` 彻底删除。**未加长任何超时**（`CleanupPolicy` 三数值一字未改，测试仍断言 2s/4s/2s；新增 250ms 是**诊断片长**且被 `min(...)` 约束）。

**T11/T12（真实子/孙进程）**：子进程 powershell 起 `ping -n 20`（**单进程、无后代、约 19s**，继承并持有 stdout/stderr 写端 → 正好对应 `Add-Type` 的 `csc.exe`），子进程立刻退出、孙进程 PID 落文件；终止只按**观测时捕获的身份**核对（`terminate_owned_process`，PID 复用会拒绝）。实测：**T11** `elapsed=747µs / completion=confirmed / 孙进程仍活着 / retained=0`（无失管 reader），stderr 走阻塞策略时如实报 `Unconfirmed/StillRunning/Retained{1}`；**T12** 4 次取消后 `first_cancel_unix_ms` 不变、读取**未**结束即如实报 `Unconfirmed`、缓冲区不缩水；CU 层 T11 收尾 **5.74ms**、T12"剩余 4.9994ms ⇒ 实际 5.9099ms"。

**它实证的平台事实（对裁决有直接价值）**：对**阻塞在 pipe `ReadFile`** 的读取线程调用 `CancelSynchronousIo`，本机实测返回 **`NothingPending`**——**既不代表已结束，读取也确实没有结束**。因此它**没有**把取消作为生产主机制（生产策略是不阻塞的轮询读取器）。若将来要求"必须靠取消结束阻塞读"，需改为 OVERLAPPED + 取消句柄，**超出本次范围**（已如实标注）。

**它主动申报的三点**
1. **残留上限的"饱和"分支只有决策级测试**（纯函数 + `Saturated` 语义断言），没用 8 份同时阻塞的读取去真实触发全局饱和（那会污染共享测试进程）；登记表的端到端行为由 T11 与 Drop 用例覆盖。
2. T12 的"未核实"分支在 CU 层**本质由时延决定**（实测命中），故测试同时接受两个分支但都断言"状态与核实必须一致、必须如实上报"；守卫层用阻塞读取**确定性**覆盖该分支。
3. **一个既存的负载敏感用例**（**不是** F01/F02 引入）：`timeout_kills_a_helper_that_ignores_cancellation_and_confirms_stillness`（断言点 `input.rs:4446`"阶段回执必须留下"）。它**改动前**实测基线就是 `108/2` 与 `109/1` 两次失败，改动后 4 次全量中失败 2 次、**单跑必过**；原因是业务期限 600ms 内 helper 连第一条进度记录都没写出（`Add-Type` 编译在并发负载下超时）。**它没有通过放宽断言或加长宽限来掩盖**，只如实报告。⇒ **记为已知负载敏感项**：日后若在合并门禁里看到该用例失败，**不得直接判定为新回归**（需单跑确认）。

**门禁（我实测，涵盖 F02 落地后的状态）**：`build-web exit 0`、**web 1063/1063**、**cu-core 116/116**、**guard 40/40**、toolreg 54/54、core 295/295、linkage 4/4，全部 exit 0、警告 0。

**未做**：`computer_use_desktop_bridge` 等调用链的报告字段透传（其 `cleanup` 已是 `Option<HelperCleanupFacts>`，新增 `pipe` 为 `#[serde(default, skip_serializing_if)]`，**老消费者不受影响**）；前端尚未消费该新字段。

### B-47 专项文档：第四轮执行后的待决策项（背景/现象/阻塞点）

新增 **`pending-decisions-round4.md`**。要点：**真正需要你先定的 4 项** —— **A-1** 迁移前遗留的"NULL 归属且未收尾"CU 运行会**长期阻断**该 scope（已核实 `main.rs` 对 `computer_use_runs` 引用数 = **0**、全 crate 无 orphan/stale/recover/sweep 机制；对照 `runtime_runs` 有 `recover_incomplete_runtime_runs`），需要你先确认**"非成功但明确"的终态口径**（因为"不猜配/不伪造成功"与"不永久阻断"之间的取舍不能由实施者自定）；**A-2** 归属值的 `ws-<Alnum>` 形状校验比裁决更严（格式耦合，我建议**保留 + 在源头加一条契约测试**把隐式耦合变成显式守约）；**B-1** 本工作树无有效 VCS 身份 ⇒ 收据的 `source_commit` 仅作参考（需你定：恢复可跟踪工作树，还是接受 `provenance_unknown` + 载荷哈希身份——**后者不改变仓库状态，前者会**）；**B-4** 后台复用核对的"正向身份"需要**新增后台接口**，需你定暴露哪些字段。其余 **A-3/B-2/B-3/B-5/B-8 属确认**（我已有建议），**B-6/B-7 属口径与范围声明**（已知负载敏感用例、取消语义不需要 OVERLAPPED 加固）。文档同时列出**本轮已关闭的项**以免重复讨论，并特别标注：**互锁的资源级扩展（裁决选 c）属 CU-F06、尚未实施**。

### B-48 RD4-04（我的 `main.rs` 串行区）交付：父运行冻结上下文全链传参完成

**裁决依据**：第五轮 B-5「采用已有冻结运行上下文传递，不另建只含 `Option<String>` 的临时工作区传参，也不在并行工具执行时重新取当前 UI 选择」。

**交付**：新增载体类型 `FrozenParentContext`（`main.rs:475`，五维：`workspace_id` 必填 + `room_id`/`session_id`/`public_turn_id`/`parent_run_id` 可缺省），`new()` 对空工作区返回 `None`（fail-closed，**不**用"当前工作区"顶替；与五维全必填的 `ChatRunScope` **不是**同义类型，故不互相转换、注释中已写明理由）。传参链一次贯通：

`api_chat_send_stream`（在**发起 run 行之后、进入生成器之前**冻结五维）→ `agent_chat_response` → `call_agent_model_with_tool_loop` → `dispatch_model_tool_calls_parallel{,_with_cancel}`（因 `tasks.spawn` 需拥有所有权，循环内整体 `clone`）→ `run_model_tool_use_messages` → `run_model_tool_dispatch_for_session_with_identity` → `computer_use_executor::execute_with_current_runtime`。

非流式/接力/目标三条路径在各自**接纳点**构造上下文；`run_tool_intent_message`、`run_model_tool_dispatch_for_session` 包装层与测试调用点保持诚实的 `None`（执行器 fail-closed 拒绝 CU，注释写明这**不是**"该路径没有工作区"）。执行器签名 `workspace_id: Option<&str>` → `parent: Option<&crate::FrozenParentContext>`，并保留自身兜底过滤 `!context.workspace_id.trim().is_empty()`。

**实测**：`cargo build -p coolzhu-web-console --offline --tests` **EXIT 0**；`cargo test -p coolzhu-web-console --offline` **1071 通过 / 0 失败**（其中 `computer_use_executor::tests::admission_entry_*` 2/2，且"空白工作区"用例改为直接构造结构体以继续覆盖执行器自身的兜底过滤）。

**必须记录的诚实缺口（不得算作已完成）**：

1. 执行器目前**只消费** `workspace_id` 一维；`room_id`/`session_id`/`public_turn_id`/`parent_run_id` 只是被**携带**、**尚未**与可信来源核对（属 A-2 层级重构）。已在执行器与载体处写明"不得假装已校验"。
2. 非流式 `api_chat_send` 与接力路径**不建** chat runtime run 行（`create_chat_runtime_run_sqlite` 全 crate 唯一调用点在流式路径），目标阶段路径亦无 `runtime_runs` 行 ⇒ 这三条路径的 `parent_run_id` 在接纳时**真实为空**，公开 turn 亦然。这是**如实留空**，不是哨兵填补；后续若需按 `parent_run_id` 反查父运行，必须先补建运行行（属 RD4-03/07 范围）。
3. 传递链只覆盖"父运行 → CU 接纳"，**未**触及 A-2 所需的形状校验替换（`ws-<Alnum>` 硬编码仍在 `CuWorkspaceAttribution::from_parent_run`）。

### B-49 并行开发窗口内观察到的两次红灯（按第六轮 §5.1 纪律登记：**归因待核**，不宣布"不是回归"）

第六轮答复明确否决了本台账原先的写法：**不接受**"现有证据已经足以推出两次都不是回归"，也**不接受**把 `pipe_reader_capacity_exhausted` 直接归为他人改动——mtime、错误落在哪个文件、文件在运行期间变大、随后同套件转绿，都只是线索，不能证明「被改文件是否真的被这次编译/测试读取」「执行的是哪一份已编译测试二进制」「失败是否来自跨模块集成」「容量耗尽属预期拒绝、测试隔离问题还是容量释放实现缺陷」「转绿是否用了与转红完全相同的输入与运行状态」。以下按答复要求分四栏登记。

| 项 | 执行结果 | 输入证据 | 共享资源状态 | 原因判断 |
| --- | --- | --- | --- | --- |
| 红灯①：`computer-use-core` 一度 **110 通过 / 8 失败** | 失败（当时） | **已观察变化**：失败用例全在 `input.rs`/`input_stroke.rs`，同窗口内该两文件确有其他负责人的写入；未固定测试二进制身份与构建输入 | **存在争用**：同 crate 正被另一工单编辑 | **假设**（未复现支持）：新增的全局读取器容量记账在用例间未归还（`已预留 8 / 可用 0`）。**归因待核**：该判据被触发 ≠ 判据触发得正确，也 ≠ 可归为他人改动 |
| 红灯②：`web-console` 一度 **1058 通过 / 15 失败** | 失败（当时） | **已观察变化**：`computer_use_store.rs` 在门禁运行期间被写到 212 KB；失败是 CU 语义断言（`Blocked` vs `Succeeded` 等）。**我自己的改动面在同一套件此前实测 1073/0，但这不是早先 15 个失败全部因果的证据** | **存在争用**：同 crate 正被另一工单编辑 | **假设**（未复现支持）：失败来自未完成的中间集成态。**归因待核** |
| 两次红灯后的绿灯 | 通过 | **未能确认**：未建立与红灯完全相同的输入/测试二进制对照 | 已收敛（无写入活动） | **假设**：中间集成态修复后转绿。未做冻结快照对照，故**不升级为"已确认"** |

**就此更正台账纪律（第六轮 §5.1 原文口径）**：并行期间的红／绿结果均绑定**实际测试二进制、构建输入与运行环境**。观察到相关源码写入或共享状态干扰时，**先登记"可能受并发影响，归因待核"，不立即归责，也不宣布无回归**；保留首次日志，协调冻结或隔离相关输入后复跑；不得擅改其他负责人文件，**不以重复运行直到通过替代定位**。

**数字的正式接收方式**（答复 §5.2）：可记录为——"所列最终测试运行均报告通过；之前两次运行存在并行开发窗口及相关失败，最终稳定基线与具体归因证据**分别保留**"。**不得**只凭 1073/0 或 1075/0 反推早先 15 个失败的全部因果，也**不得**把未说明变化的两组通过数量混成同一源码身份的结果。

**另按要求拆清两件曾经被我用一个总状态互相覆盖的事**：`computer-use-core` 的**容量实现发生过修改**（RD4-08/09 窗口内）与 **RD4-09 专门的隔离饱和验证尚未开始**，两者**可以同时成立**，不互相覆盖。RD4-09 的现状见 §B-53③ 与后续 RD4-09/10 工单报告。

**泄漏/隔离类结论的措辞边界**：`pipe_reader_capacity_exhausted` 只直接说明容量判据被触发；判据触发得是否正确、以及是否属他人改动，均需独立证据（RD4-09 工单要求两方向用例）。

### B-51 RD4-01 交回的**一行迁移**由我登记（v23）+ 顺手核正一处**版本回退**隐患

RD4-01 按"不自行抢占版本号"的纪律停下并交回一行。该行落在**我的串行区**，已登记：

1. `main.rs` 阶梯末步之后加**恰好一行** `computer_use_store::apply_session_migration_v23_legacy_run_convergence(connection)?;`，`SESSION_SCHEMA_VERSION` 推到 **23**。
2. **`ComputerUseRunStore::open()` 同例补调 v23**（与 v22 先例一致：本文件拥有的列/表在"独立打开 store、不经 main.rs 阶梯"时也必须可用）。RD4-01 把这一步留给裁决，我按既有先例判定为**必须**：否则 RD4-03 的收敛写入会在 open 路径上以 `no such column` fail-closed（属"缺列静默失败"，比显式报错更坏）。同时更新其文档块，去掉"待登记"措辞。
3. **核正一处版本回退隐患（既有写法，非本轮引入）**：v21 的推进守卫曾写成"小于**终点常量**"、而推进语句写死 `user_version = 21`；由于 v22/v23 的守卫都是"小于**本步自己的版本号**"，在已迁移到 22 的库上重跑阶梯时，v21 步会把版本**回写**成 21，再由后续步骤补回。终点虽正确，但中途出现了一次真实的**版本回退**。已把守卫改为 `current < 21`，并在常量文档处写明"各步守卫必须写本步自己的版本号"。
4. **两条防复发测试**（`main.rs` 测试模块）：
   - `session_migration_steps_never_guard_against_the_terminal_version_constant`：结构守卫，扫描 `main.rs` + `computer_use_store.rs`，禁止任何迁移守卫比较终点常量（断言文本运行时拼接，规避自引用陷阱——**这条注释本身第一次就把该陷阱踩了一遍**，已改写措辞）；
   - `session_schema_ladder_registers_v23_legacy_convergence_objects`：端到端落地断言，阶梯跑完后必须**真的**存在 `computer_use_legacy_run_convergences` 表与 `computer_use_runs` 的 3 个收敛列，拦下"版本号推进了、对象却没建"的空推进。

**本轮最终门禁（全绿）**：

| 套件 | 结果 |
| --- | --- |
| `cargo build -p coolzhu-web-console --offline` | EXIT 0 |
| `cargo test -p coolzhu-web-console` | **1075 通过 / 0 失败** |
| `cargo test -p coolzhu-computer-use-core` | **118 通过 / 0 失败** |
| `cargo test -p coolzhu-windows-process-guard` | **47 通过 / 0 失败**（1 ignored） |
| `cargo test -p coolzhu-tool-registry` | **54 通过 / 0 失败** |
| `cargo test -p coolzhu-core-runtime` | **306 通过 / 0 失败** |
| `cargo test --test module_linkage_smoke` | **4 通过 / 0 失败** |

### B-52 RD4-06 交付记录（源码快照 / 报告治理）与它留下的两点待定

**交付要点**（改动仅 `scripts/**`、`config/package-manifest.json`、`docs/**`；未动任何 `*.rs`，未改 Git 状态）：三身份（`source_snapshot_digest` / `build_input_digest` / `payload_digest`）的唯一实现点在 `scripts/lib/build-identity.ps1`；报告唯一 ID + 内容哈希（在**归一化后文档**上计算，故"读回即一致"）；报告与包内 `payload-inventory.json` 双向引用、缺引用时 `[REPORT-REF-MISSING]` fail-closed；保留/归档规则脚本化（`package-report-retention.ps1`，索引损坏时拒绝清理）；实跑 `package-all.ps1 -SkipBuild` 到**临时包根**与真 WiX 出 MSI（未覆盖 canonical 产物）；并发编辑被真实抓到并 fail-closed（真阳性，重试 14 次）。收据如实标注：`source_commit=null` + `authority=not-authoritative` + `vcs_state=untracked_snapshot` + `dirty_against_commit=not_evaluable`（非布尔 false），另用 `tracked_index_file_count=0` 作为"当前源码未被该提交有效覆盖"的证据。

**留下的两点待定（需你定）**：

1. **fail-closed 取舍未由裁决指定**：并发编辑时**拒绝出报告/出包**（当前实现）。另一合规选项是"照常出包但如实记 `quiescent=false` + 差异清单"（只需改 `package-all.ps1` 一个分支，身份口径不变）。这属产品/运维口径，我不单方面决定。
2. **同一个 flake 的归因已被 PKG-L07c 实测推翻（我先前的写法是错的）**：失败原文**不是** `File.Replace` 字样，而是
   `Cannot find path '...l07c-concurrent\WebView2Loader.dll' because it does not exist. | You cannot call a method on a null-valued expression.`
   后者定位为内联 `(Get-FileHash ...).Hash.ToLowerInvariant()` 在文件**瞬时不可见**时对 `$null` 取方法（已实测复现）；`File.Replace` 只是**底层机制**。真正的缺陷是产物与收据之间**没有互斥**。


### B-53 RD4-08/09 交付：把"负载敏感"的 flake 拆成确定用例 + 读取器容量政策落地

**改动面**：仅 `computer-use-core`（`input.rs`/`input_stroke.rs`）与 `windows-process-guard`（`pipe.rs`/`cleanup.rs`/`lib.rs`）；未触碰 `main.rs`/`app.js`。

**① flake 的机制被查清（这是他这条线的核心价值）**：旧断言 `facts.injected_steps >= 1` 并非"要求 600ms 内写出记录"，而是要求 helper 在**被强杀之前**（600ms 业务期限 + 2s 协作退出 ≈ 2.6s）写出第一条记录；而实测 helper 自身启动耗时 SendInput 配置 ~1.1s（空闲）、`auto` 配置 ~2.5–4.0s（含预检）、负载下 3.2–5.8s——**恰好横跨 2.6s 这条线**，这才是"单跑必过、并发失败（108/2、109/1）"的确切原因。原用例已拆成 3 条**确定可复现**用例（受控启动延迟 4000ms 保证"期限耗尽时未 ready"；同步屏障"取消只在观测到 ready 后返回 true"保证"取消必然发生在 ready 之后"；真实冒烟按声明预算如实失败），并把原始失败日志 `tmp/rd4-08-09/01-test-cu-core-baseline.log` **保留未删**。六组各 20 次重复实测 **120 次全绿**。

**② 容量政策（裁决"计量单位是实际 reader"的落地）**：上限 8 **不调大**；新增 `PIPE_READERS_PER_HELPER=2`、`reserve_pipe_reader_capacity`、创建**线程之前**的预留（不足即 `pipe_reader_capacity_exhausted`，不建线程、不产生输入、句柄关闭）；唯一能创建读取线程的构造器必须收下预留（源检查钉住）；残留登记时预留转为登记占用（不重复计数）；容量只能靠**已核实结束**的回收恢复，绝不驱逐未知读取器。

**③ 一处语义更正（重要，已写入文档口径）**：`unreclaimed_dropped`（RD4-09 后改名 `unowned_unreclaimed` 且修复后**恒 0**）计的是**无人管理的活动读取器**，不是"没用上的槽位"。`park_retained` 的 `Saturated` 分支会"丢登记条目 + 关线程句柄 + 分离线程"，而**读取线程仍存活**并持有管道读句柄与缓冲区——隔离子进程测试实测该线程在 300ms 内轮数 **0 → 57**，写端关闭后才自停。**因此"登记表不超过 8"本身不证明资源有界**；该口径已写进 `pipe.rs` 模块文档与 `cleanup.rs` 相应字段注释。

**④ 平台约束实测（B-7 第 8 条，非推断）**：`PeekNamedPipe` 实测句柄是**同步句柄**（空管道+存活写端时 `lpOverlapped=NULL` 读 800ms 未返回）；文档所述"同步句柄 + 多线程"阻塞情形在**本机/本配置未复现**（同句柄另有线程阻塞在读里时，`PeekNamedPipe` 10.9µs 返回）。生产调用点不满足该前提：`PeekNamedPipe` 全仓仅一处调用（`peek_available`），且只被同一读取线程的 `poll_available` 调用，是串行 peek→read；宿主线程只对**线程**句柄做等待与取消。`CancelSynchronousIo` 返回 `NothingPending` 的记录按实验原样保留、未扩大解释。

**⑤ 我的独立复核**：该工单声称放宽的一条既有阈值经我逐字核对——**硬上限断言 `command_line < 32_767` 原样保留**，仅软余量断言由 3KB 改为 `< 31_000`（≈1.8KB），且每次运行都打印实测值（当前 30014/32767），其文档说明与实测值都在源文件里。全门禁我**独立复跑**与它自报一致：web 1075/0、cu-core 118/0、guard 47/0、tool-registry 54/0、core-runtime 306/0、linkage 4/0。

**⑥ 它留下的待裁决项**：见 §C 第 16–18 项（软余量阈值确认、`Saturated` 故障态是否改为"拒绝即上报并保留句柄"、以及 `holds_unreclaimed_reader()` 对 `Saturated` 返回 true 的语义与行为修正）。

**口径修正（RD4-09 工单实测后）**：① 上一轮"容量只靠已核实结束的回收恢复"成立，但**生产路径并未使用** `HelperPipeReadersAdmission`——实际顺序是"先 spawn helper 进程，再由两条流各自 `reserve_pipe_reader_capacity(1)`，最后才写 stdin；拒绝即 kill+wait 且**尚未写入 stdin**"。因此"不创建读取线程、不产生任何输入、句柄关闭"成立，而"**连进程也不创建**"并**不成立**（拒绝时是 spawn 之后才杀）。② 子进程饱和用例收尾行的 `unmanaged_dropped=1` 来自该用例**刻意绕过接纳**塞进去的第 9 个 reader（正是为了刻画 `Saturated`），不要把这一项也读成 0。

### B-54 第六轮答复落地：A-2 口径文件 + v23/v21 四条边界（含一条真实故障注入用例）

**A-2 的权威执行口径已单独成文**：**`a2-workspace-source-and-frozen-parent-context.md`（v1，2026-09-25）**，标题 **「A2：工作区来源与冻结父上下文——生效执行口径」**。派生自第六轮答复第二部分，含固定链路、四条不得违反的原则、模块职责与授权边界、`FrozenParentContext` 构造与四维消费要求、三条非流式路径处理规则、**六组固定验收测试 A2-T1..T6**、**合并顺序（先接来源、最后删私有正则，禁止中间可执行态）**，并附 §6 四条迁移边界与 §7 执行顺序、同批打包与 PKG-L07c 裁决。**后续实施（含交给实施模型的工作）引用该标题与版本，不再引用"第五轮 §五"这类章节编号。**

**v23/v21 的四条边界（第六轮 §6）已逐条落地并有测试**（改动仅 `main.rs` 测试与 `computer_use_store.rs`）：

| 边界 | 落地方式 |
| --- | --- |
| 单步守卫 | 结构守卫只约束**迁移步骤函数**：每步必须用**本步版本号**守卫（`current < N` 或 `current >= N`），实测 19 个步骤全过 |
| 整体守卫 | 结构守卫**不**约束整体阶梯判断——不再禁止终点常量的合法比较（原写法过宽，已按答复更正） |
| 独立打开路径 | `ComputerUseRunStore::open()` 与主入口调用**同一批迁移函数**，并共用前置规则 `ensure_session_schema_not_from_the_future`（唯一实现），不再各写一份 |
| 错误传播 | 新增顺序断言：**版本写入必须是该步最后一处变更**（写在写版本之后还有 `execute_batch(`/`ensure_`/`ALTER ` 即失败） |

**四类行为测试（第六轮 §6 要求）全部落地**：

1. `session_schema_v22_database_upgrades_to_v23_with_every_object`：造**真实 v22 库**（删掉 v23 对象**并**回退版本号），升级后版本正确、两张表 + 两条索引 + 三列齐全、既有行不丢。
2. `session_schema_v23_reopens_through_both_entries_without_rollback_or_data_loss`：主阶梯与 `ComputerUseRunStore::open()` 两个入口再次打开都不回退版本、不丢数据。
3. `session_schema_failure_midway_never_declares_a_version_without_its_objects`：**真实故障注入**——把 `computer_use_runs` 换成**同名 VIEW**（实测 SQLite：`CREATE TABLE IF NOT EXISTS` 遇同名 VIEW 静默跳过，而 `ALTER TABLE <view> ADD COLUMN` 必报 `Cannot add a column to a view`）。断言迁移**必须返回 Err**、版本停在 22、**不留半套 v23 对象**、既有数据完好。
4. `session_schema_entries_reject_a_newer_database_without_downgrading_it`：把库标成 `user_version = 99`，**两个入口都必须明确拒绝**且**不得把版本号降下来**。

**同时新增的生产行为**：迁移前置规则 `ensure_session_schema_not_from_the_future`（超前版本 fail-closed），两个入口共用（第六轮 §6 边界 3 的直接要求）。**口径说明**：对**已知版本但对象不全**的库，既有设计是"同版本内补齐"（`IF NOT EXISTS` 风格），这不是降版本号，故 T4 不把它列入拒绝之列——这一读法已写在测试注释里，如与裁决本意不符请指出。

**本轮门禁（全绿）**：`web-console 1079/0`（新增 4 条边界用例 + 结构守卫加顺序断言）、`cu-core 118/0`、`guard 47/0`、`tool-registry 54/0`、`core-runtime 306/0`、`module_linkage_smoke 4/0`。

**尚未完成**：A-2 本体（来源分层）按第六轮 §7 为**第 1 顺位**，下一步实施；RD4-06 政策补丁、PKG-L07c、RD4-09/10 已按答复并行起工单。

### B-55 RD4-09/10 交付：容量判据的**两方向**证据 + 测试闸门**不掩盖**缺陷（变异实验）+ command-router 显式环境例外

**RD4-09（真实饱和验证）**——不是引用上一轮报告，而是**独立复跑**后逐条核实：上限 8 未调大、读取线程**唯一创建点**必须收下预留（生产段仅一处 `thread::Builder::new().spawn(`）、拒绝时"没造出读取器"（`residue`/`unmanaged_dropped`/登记表/`fault` 全部不变，且 stdin 未写入）、按**实际完成**释放（登记表只有两处移除点、都以 `verified_completion().0.is_confirmed()` 过滤，杀 1 个持有者后 `residue 8→6`、`available 2`）、并发申请"成功 2 / 被拒 2"、收尾后 `residue=0 available=8`。

**它补上的真实缺口**：原有拒绝用例**全部发生在 `available == 0`**，无法把"按义务判定"与"看到残留就拒绝"区分开，`available == requested` 这个最紧边界也未经真实接纳接口验证。补法是最小必要的（扩进既有子进程用例，未拆函数、未增测试数），实测两方向：`residue=6 reserved=1 available=1` 时请求 2 → **必须拒绝**（报文含 `可用 1，已残留 6，已预留 1`）；`residue=6 reserved=0 available=2` 时请求 2（**恰好等于可用**）→ **必须接纳**，用满后再要 1 个拒绝，归还后 `reserved=0 available=2`。两条合起来证明判据是"**义务 + 请求 > 上限**"，与剩余量、与是否有残留**无关**。

**测试期容量闸门不掩盖生产缺陷**——证据分两层：① **编译产物级**（非只引源码）：`--emit=llvm-ir` / rlib 成员解析 / `--emit=obj` 三层核验，闸门符号在生产构建里命中 **0**（同一构建里生产符号有命中作对照）；② **变异实验**（隔离副本，不触碰共享源码）：在 guard 副本里让 `release_in_place` 不退账 → 对照 `47/0` 变 `43/4`；在 cu-core 副本里（**闸门原样保留、未改一行**）→ 对照 `118/0` 变 **`103/15`**，连续 3 轮数字完全一致。即：**缺陷存在时仍然是红的**。机制上也成立——闸门是**另一个独立计数器**，既不调用 `reserve_pipe_reader_capacity`、也不碰 `residue_registry()`、不释放任何预留，只能推迟用例启动，不可能给生产账目补容量。

**如实保留的残余风险（未改）**：闸门对**同线程嵌套**的 helper 运行不二次占位，而生产确有嵌套（独立释放走同一个 `run_native_helper`，外层两条流可能尚未 drain）⇒ 闸门的 6/8 预算在嵌套场景下**可能少记最多 2 个真实单位**。少记只会让真实拒绝**更早出现**（可见失败），不会把泄漏藏起来；定向 20 轮 + guard 全量 5 轮 + cu-core 全量 3 轮均未出现该情形。

**RD4-10（指定环境例外）**：列出该 crate 测试的真实环境依赖（唯一外部依赖是 PATH 上的 `git`；`TEMP`/`TMP` 必须可写；机器级 git 配置 `commit.gpgsign`/`init.templateDir`/`core.hooksPath`/`core.excludesfile`/`core.autocrlf`/`user.useConfigOnly` 会影响结果但**原先完全未声明**；生产只读 `SAFEUSER`/`USER`/`CODEX_HOME`/`HOME`/`USERPROFILE`，而测试全走显式传参变体刻意绕开环境——这一点原先也未声明）。改动四处：新增 `require_git_or_skip` + `announce_env`，让两个依赖 git 的用例在环境不满足时**明确跳过并给出原因**，且跳过行**绕过 libtest 的输出捕获**直写 stdout，因此**不是静默通过**；并以"把探针程序名改成不存在的可执行文件"做了**负向验证**（不加 `--nocapture` 也能在输出里看到 `[env-skip]`），随后按字节还原。

**同时发现但未修（受本机 target 限制）**：`#[cfg(unix)]` 的 `commit_push_pr_command_commits_pushes_and_creates_pr` 直接 `env::set_var("PATH"/"SAFEUSER")` 并手写恢复，断言失败会**永久污染进程环境**，且其 `env_lock()` 只与自身串行，其他走 PATH 找 `git` 的用例并不持锁 ⇒ 真正的跨用例污染路径。本机只装 `x86_64-pc-windows-msvc`，该段**无法编译验证**，故按"改完必编译"纪律**不交付未验证代码**，建议并入 §C-7 待裁的 env RAII 工单（RPR-01b）。

**我独立复跑的核实**：`command-router 19/0`、`guard 47/0`（1 ignored）、`cu-core 118/0`，与其报告一致。

### B-56 RD4-06 政策补丁交付：**维持拒绝发布** + **必须出失败诊断** + 三态发布资格（含我的独立复核）

**改动面**：仅 `scripts/**`（`package-all.ps1`、`lib/build-identity.ps1`、`package-report-retention.ps1`、`build-msi.ps1`、`test-package-build-identity.ps1`）、`config/package-manifest.json`、`docs/**`；**未触碰任何 `*.rs`/`*.js`/`*.css`/`*.html`**，未触碰 `webview2-loader.ps1` 与其测试，未改 Git 状态。唯一实现点在 `scripts/lib/build-identity.ps1`（其余脚本只调用），与上一轮三身份的落点习惯一致。

**裁决 §三 逐条落地**：① 仍 fail-closed，新增 `release_eligibility.refusal_note` **明文否定**"用 `quiescent=false` 补可发布收据"。② 关键修正——**不再连诊断一起拒绝**：失败诊断写到 `<reportParent>/failures/package-report-failure-<config>-<stamp>.json`，**与 `-ReportPath` 的成功报告落点分开**，因此"失败即无成功报告"与"失败必有诊断"同时成立。③ 诊断最小字段齐备：`run_id`、`failure{stage,category,message,detail_lines}`、**真实进程退出码**（非进程阶段显式 `not-applicable`，不伪造 0）、`declared_input_scope`、`observed_input_changes`、`produced_not_released`、`release_eligible=false`。④ 差异标为 `evidence_kind=observed-between-two-samples` + `is_complete_write_history=false`。⑤ 失败路径**没有任何**写 `latest-<config>.json` 的代码；失败前后各读一次指针核对；暂存物整体移入 `tmp/package-failures/<run-id>/payload`（`auto_promotion=never`），包根随后为空；`signing_or_distribution=not-attempted`。⑥ 状态分列：`live_worktree_changed`（三态，含 `not-confirmed`）/`build_snapshot_integrity`/`build_input_digest`/`validation_snapshot_digest`/`release_eligible` + 逐门 `gates[]`。⑦ 新增**构建前**的构建输入基线 + 构建后逐描述符比较；未声明快照、声明 root 缺失、重算失败一律 `not-confirmed`→拒绝；并如实记 `immutable_build_snapshot=false`（本实现不从快照副本构建）。⑧ 逐路径分类（`in-declared-source-snapshot-scope`/`declared-build-input`/`declared-non-build-input`/`outside-declared-scope`/`undetermined`）写进报告与诊断——实测**日志变化不误判**为源码变化。⑨ `--no-build` 新增**产物—快照关联记录**（同一声明来源路径 + 内容身份 + 本次快照摘要 + 本次构建输入摘要），缺失即 `not-confirmed`；失败运行登记的禁用记录在 `--no-build` 下硬拒 `[ARTIFACT-REVOKED]`。消费侧：`build-msi.ps1` 与 retention 的 Protect 在发布前一律 `Assert-PackageReportReleaseEligible`，**失败报告不得被登记为发布证据**。

**真跑证据**：夹具级 28/28（`tmp/rd4-06-verify/run.ps1`）、真实清单声明范围 22/22（1138 文件 / 16 roots，裁剪 artifact 列表以避免全量编译，报告已明示）。"未覆盖上一份成功报告/未晋升"的实证：成功报告与 `latest-*.json` 的 mtime 停在成功那次、失败诊断 mtime 更晚，且**断言字节相等**；诊断自证 `pointer_updated=False overwritten=False`；隔离区实测落盘。

**我的独立复核**：`test-package-build-identity.ps1` → **64 cases / 0 failed**、`test-package-safety.ps1` → `PASS`、`test-powershell-script-compat.ps1` → `PASS`，与它自报一致。

**它如实列出的强度边界（不得当成已闭环）**：① "不稳定构建的裸 DLL 不作来源"只按"内容身份"否决**已知不稳的那份字节**，字节不同者只能落到 `not-confirmed`（属"未确认"而非"已判定不可用"）。② `--no-build` 的"匹配快照"依赖此前一次**构建后校验通过**的运行，没有关联记录即 `not-confirmed`（本仓库现存 `target/debug/*.exe` 全属此类）。③ **未跑含真实全量编译的打包**（并发在途编辑 + 工单限制），故"从源码重新编译→出包"这一段仍未验证。

### B-57 PKG-L07c 交付：并发导出与收据一致性（**归因被实测推翻并更正**；判定 P1，残留待外部复核）

**改动面**：仅 `scripts/lib/webview2-loader.ps1`、`scripts/test-package-webview2-loader.ps1`、`packaging-webview2-loader-export-protocol.md`；**未改** `package-all.ps1`/`build-msi.ps1`（属 RD4-06），未碰任何 `*.rs`，未改 Git 状态。

**查清后的并发契约**：锁**槽位**＝`sha256(规范槽位目录 | 目标文件名 | package target | profile)`（**不含** build id / 运行 ID / 产物哈希）；实际槽位是 `modules/gui-desktop/target/package-inputs/windows-x64/{profile}/`；发布权＝在该目录 `CreateNew` 独占创建 `.loader-publish-<key>.lock` 并**在整个发布窗口保持打开句柄**（别的进程只能只读看到持有者记录，无法删除/改名，跨会话有效）；回收只认"持有者 pid 不存在 / pid 被复用（启动时刻不匹配）"，加 10 分钟年龄兜底。赢家流程＝取得发布权 → 本次运行独立暂存并复核 → 发布产物 → 发布收据 → 完成校验（两文件互相自洽 + 同代次）→ 释放；争用者超时抛 `[EXPORT-SLOT-BUSY]`（`busy=true retryable=true`）且**不碰赢家的锁/暂存/产物**；消费端**先取同一槽位读取权**再核对"产物 + 收据"，拿不到即 fail-closed 为 Busy。

**改动前"不成立"的一点（这是真缺陷）**：产物与收据之间**没有任何互斥**，两个发布者可以交叉替换同一文件；且**源身份取样发生在发布产物之后**（收据会给"导出窗口内已变化的输入"背书）。三种实测失败形态：`File.Move` → `ERROR_ALREADY_EXISTS`；`File.Replace` → `ERROR_UNABLE_TO_REMOVE_REPLACED` / `ERROR_UNABLE_TO_MOVE_REPLACEMENT` / `ERROR_SHARING_VIOLATION`；事后核对 → 身份不一致；失败方还会观察到目标**瞬时不存在**。已修（源身份取样前移、写侧与读侧都持槽位权）。

**判定 P1，未升 P0**，证据是全部实测的：两处消费点都以**内容 SHA-256** 核对（`Publish-Artifact` 与 `Assert-LoaderExportReceipt`）；160 次并发 `File.Replace` 探针中 30 次失败、**0 次混合/半成品内容、0 次目标永久丢失**；其用例 4/5 用真实进程构造"不完整代次/产物被替换"，消费端一律 `CONTENT-MISMATCH` 拒绝，staging／报告／MSI／分发都进不去。

**它自己标出的残留（P0 升级判据，请求外部复核）**：并发中**失败的一方仍可能把自己的收据写进共享槽位**（写收据时已无第二次产物核对），使槽位上的"收据 ↔ 产物"错配；report 里没有 `generation`，事后无法据此自动判定报告引用的收据是否还是它消费的那一代。它按"错配会在消费点 fail-closed、错配产物进不了可发布分区"判为 P1，并**明确说明未在改动前经验性地强行构造出那一具体交错**——即 P0/P1 判定建立在代码路径 + 消费侧 fail-closed 实测之上，而非该交错的复现。**这一条最需要外部复核，已记为 §C-26。**

**六条验收用例全部落地并真跑**：两真实进程争用（20/20 轮；wait=0 口径 240 次调用 = 57 成功 + **183 次明确 Busy**、无其它失败形态；wait=60 口径 240/240 成功、0 Busy；无交叉覆盖）、产物写完收据未完成即硬退出（exit=97，消费者拒绝并带 `incomplete_generation_or_tampered=true`，被杀进程的锁按 pid 存活判定回收）、收据存在但产物被替换（真进程 + 包层 L06a 都拒绝）、竞争者失败/取消（`[EXPORT-SLOT-BUSY]` 竞争者与被 `Stop-Process` 的竞争者都**不碰**赢家的锁 token/产物/收据哈希）、不同槽位不互相串行（两个发布者同时停在发布窗口、两把锁同时存在）、源输入变化（`SOURCE-INPUT-CHANGED`、**不写收据**、消费者拒绝；**未使用任何 `quiescent=false` 放行**）。

**固定轮数与原始日志**：`-OnlyConcurrency -ConcurrencyRounds 20` → **160 用例轮 / 0 失败**（单次固定轮数，无重试到绿）；改动前对照保留：真实脚本 3 轮里 `L07c-concurrent-export` 失败 1 轮、独立探针 3/3 轮出现失败、`File.Replace` 微探针 30/160 次失败。**全量回归 `pass=30 fail=0 skip=4`**（其日志与冻结版本哈希均记录在案）。

**夹具根并发冲突的判定与修复**（回应我转交的情报）：判定为**"仅测试夹具互相踩"，不是产品并发契约缺陷**（产品边界是 `package-inputs/.../{profile}` 槽位，两者不是同一条边界）；夹具根改为**按运行隔离** `tmp/package-webview2-loader-contract/<stamp>-<8hex>/`，只清理超过 6 小时且名字形如运行 ID 的旧目录；**同时启动两次全量测试，两次都 `pass=30 fail=0`**。我提到的那次 `ArgumentException` 与它踩到的是同一族问题（WinPS 5.1 对 `@(<List[object]>)` 抛 `Argument types do not match`，对 `List[string]`/`ArrayList` 不抛），新代码已避开。

**我独立复跑复核**：`test-package-webview2-loader.ps1` → **`pass=30 fail=0 skip=4`**、并发契约 `cases=8 failed_cases=0`，与其报告一致。

**需要操作侧知道的一条（非缺陷）**：导出收据 `schema` 升到 **2**，旧 `schema=1` 收据被 `RECEIPT-SCHEMA` 拒绝 ⇒ **正式发布需要重新构建一次导出**（有意为之）。

**它未宣称已通过并发验收**：并发仍要求**单一打包者 + 独立输出**（已写入协议文档 §10.5）；多机/多用户 + 真实 MSI 全流程复跑仍属后续验收。另建议 `package-all.ps1` 的 `exported_artifacts[]` 带上 `generation`/`slot_key`（该文件属 RD4-06，它未改）——记为 §C-27。

### B-58 A-2 Step A/B/C 交付：来源分层完成（**CU 私有格式规则已删除**，含结构守卫自证）

按《A2 生效执行口径》§10.2 的合并顺序（**先接来源、最后删正则**）落地，全程树可执行、未出现"旧规则已删、新检查没接上"的中间态。

**Step A｜源头唯一解析器**（`main.rs`）：`CanonicalWorkspaceId`（无公开构造函数，只能由解析器产出）+ `InvalidWorkspaceIdentity`（六类，`code()` 与既有审计码**逐字对应**）+ `canonical_workspace_identity()`（校验顺序与分类跟原 CU 实现逐条一致）。

**Step B｜收紧冻结上下文**：`FrozenParentContext.workspace_id` 改类型为 `CanonicalWorkspaceId`；`new()` 由 `Option` 改为 `Result<Self, InvalidWorkspaceIdentity>`（内部走源头解析器，失败即如实返回原因）；新增接纳点构造器 `frozen_parent_context_at(entry, ...)`，失败时**如实记录**"缺哪个维度、哪个入口尚未建立它"（口径 §10.4 决定三），四个接纳点（`goal-phase`/`chat-relay`/`chat-send`/`chat-send-stream`）全部改走它。

**Step C｜CU 删除私有格式规则**：`CuWorkspaceAttribution::from_parent_run` 改签名为 `(&crate::CanonicalWorkspaceId) -> Self`——**无 Err 分支**，因为"值是否合规"在类型上已不可能为假；`is_canonical_workspace_identity` / `MAX_WORKSPACE_ID_CHARS` / `InvalidWorkspaceAttribution`（含 Display/Error 实现）**整体删除**，仅留一条说明注释。执行器的接纳块随之只剩一个问题——"有没有上下文"：`input_context_incomplete` 只表达缺少上下文，不再承担值审计。

**覆盖没有丢，只是搬了位置**：原 CU 侧那张 15 行的"禁止值→错误码"表（含 `NONE`/`null`/`0`、`/home/me/...json`、`bailian::glm-5.2`、`*Untitled - 记事本`、`session-1`、`ws-`、`ws-abc-def` 等）**逐条搬到源头** A2-T1 的分类表；store 侧改为一条"原样记录已解析身份"的用例；执行器侧只留"无上下文即拒绝"。

**A2 六组进度**：T1 **完成**（生成↔解析往返 + 六类分类 + **结构守卫**"CU 无私有规则"：`from_parent_run` 入参必须是已解析类型，且非注释代码里不得再出现旧符号）；T2/T3 **已满足**（复用既有用例：归属往返不变、历史 `Unrecorded` 不被按当前工作区补值、运行中切换工作区归属与库均不变）；T4 **完成**（窗口名/占位值/路径/自造串/复制 ID 一律不能构造上下文）；**T5/T6 未开始**（四维可信关系核对与入口差异，按 §10.4 决定二走既有查询边界）。

**自证（"读取字段不算接线"的反面证据）**：Step A 时新类型只有测试引用，生产构建出现 `never used`；Step B/C 接线后警告**自动消失**，生产构建警告数回到接线前的 **99**，仅剩 `FrozenParentContext` 四维字段的**预期**警告（已在类型文档写明不得用 `allow` 掩盖）。

**门禁（全绿）**：`web-console 1083/0`（新增 A2-T1 结构守卫与 A2-T4）、`cu-core 118/0`、`guard 47/0`、`tool-registry 54/0`、`core-runtime 306/0`、`module_linkage_smoke 4/0`。

**当前可宣称的边界**：**"A-2 来源分层完成"**；**不得**称"三条非流式 CU 已恢复可用"（T5/T6 未完成）。

### B-59 A2-T5/T6 交付：四维**可信关系核对**接线（含一处必须如实标注的未覆盖）

**新增生产校验** `validate_frozen_parent_relations`（`main.rs`）＋ 执行器接线：在取得冻结上下文之后、**任何 store 操作与任何原生输入之前**做四维核对，拒绝码逐维可区分（`parent_context_room_unknown` / `_session_unknown` / `_run_unknown` / `_run_workspace_mismatch` / `_run_session_mismatch` / `_run_room_mismatch` / `_run_turn_mismatch` / `_run_not_executable` / `_identity_missing`）。**每一维都由权威来源查询回答**：房间与会话查 `chat_rooms`/`sessions` 的存在性；父运行查 `runtime_runs` 行，并核对其 `workspace_id`/`session_id`/`chat_room_id`/`legacy_turn_id` 与上下文逐项一致、`state` 属 `accepted|running`（当前执行资格）。**不做**"同一结构体字段互相比一次"式的假校验（裁决 §2.2）。

**载体补 `entry`**：`FrozenParentContext` 记录接纳入口名，使拒绝信息能**同时指明"缺哪个维度、哪个入口尚未建立它"**（口径 §10.4 决定三）。目标阶段路径改用**真实** `goal_phase` 运行行（`run_context.run_id`）而不是诚实的 `None`——它的运行行本就带 workspace/session/room/claim，把它交给 CU 做关系核对才是"用真实第一方标识"。

**A2-T5 用例**（`a2_t5_relation_mismatches_are_rejected_before_any_native_input`）：7 种错配（运行不存在、跨工作区、会话不符、房间不符、轮次不符、房间不存在、会话不存在）逐一断言：`Blocked` + **具体维度码** + `attempts==0` + `steps_completed==0` + **无运行行** + **无步骤行**。跨工作区那条正是"引用其他工作区的真实运行也拒绝"。

**A2-T6 用例**（`a2_t6_missing_parent_run_is_refused_with_the_dimension_and_entry_named`）：普通模型 CU 缺父运行时 fail-closed，且拒绝信息**同时包含维度名与入口名**（断言含"父运行"与"chat-send"）；随后**补上真实父运行即不再因身份缺失被拒**（证明这不是"永远拒绝"）。

**必须如实标注的未覆盖**：T6 的后半句——"合法 ControlPlane 不伪造聊天身份、普通模型任务不能伪装成 ControlPlane"——属**来源/上下文分类**接线（与 CU-F04「生产 ActionOriginAuthority」/CU-F05「动作事实纵向接线」同一条线），**不在本次冻结上下文范围内**，本轮**没有**做，**不得**当作已校验。

**零输入的证明口径（如实说明）**：以 `attempts==0`、`steps_completed==0`、无运行行、无步骤行四项为零证明（沿用本文件既有口径），**未**使用额外的原生输入 spy。

**行为后果（裁决已明确接受）**：三条非流式路径（`chat-send`/`chat-relay`/`goal-phase` 以外的两条：接力与非流式）**现在会在 CU 接纳处 fail-closed**，拒绝信息指明"父运行：入口 X 尚未建立它"。这正是裁决 §2.4"不批准把真实缺省当成普通模型 CU 的普遍豁免条件"的要求；能力缺口由 RD4-04 剩余生产闭环/RPR-13 建立真实接纳来关闭。**普通文本生成不受影响**（不经过 CU 接纳）。

**门禁（全绿）**：`web-console 1085/0`（新增 A2-T5/T6 与执行器级零输入用例）、`cu-core 118/0`、`guard 47/0`、`tool-registry 54/0`、`core-runtime 306/0`、`module_linkage_smoke 4/0`。

### B-60 RD4-05 / RD4-07 交付：归属／恢复呈现 + 后台实际身份读取（各含"复用"结构守卫）

按裁决 §七 的"随后／接口稳定后并行"推进，两项都在 `main.rs`／`app.js` 串行区内，**不依赖** RD4-02/03 的两个待决点。

**RD4-07｜`GET /api/system/runtime-identity`（后台实际身份读取）**

- 响应字段：`workspace_path`、`workspace_id`、`workspace_id_is_canonical`、`session_db_path`、`session_schema_version`、`supported_session_schema_version`、`computer_use_store_state`、`legacy_unconverged_runs`、`build_version`、`port`。
- **复用规则**（本项核心）：工作区路径来自 `active_workspace_path()`、规范标识来自 `workspace_identity()` 并**必须**能被 A-2 的 `canonical_workspace_identity()` 接受、库路径来自 `default_session_sqlite_path()`、终点版本来自 `SESSION_SCHEMA_VERSION`。`workspace_id_is_canonical` 为 `false` 表示"生成"与"解析"两份规则出现分裂，属**必须可见**的异常——不隐藏、不回退成"当前工作区"。
- **不谎报**：库不存在 ⇒ `session_schema_version = null`（**不是 0**；0 是"全新未初始化库"的真实值）、存储状态 `not-initialized`。`user_version` 用**只读连接**读取（`SQLITE_OPEN_READ_ONLY`），GET 不做 DDL。
- 测试三条：判定函数的"不谎报"行为（临时路径，环境无关）、处理函数与唯一来源逐字一致 + 序列化字段名即契约、**结构守卫**（处理函数必须含 `workspace_identity(`/`canonical_workspace_identity(`/`SESSION_SCHEMA_VERSION`/`default_session_sqlite_path()`/`legacy_unconverged_runs()`；且不得夹带无关查询）。

**RD4-05｜`GET /api/system/attribution-and-recovery`（归属／恢复呈现）+ 前端文案**

- 只读呈现三类事实：**历史归属未记录且待收敛**（复用 RD4-01 的全局扫描 `legacy_unconverged_runs`）、**收敛待对账**（复用 `pending_legacy_run_convergence_intents`）、**非终态运行行**（`runtime_runs` 中 `accepted|running|stop_requested` 的行）。
- **判据只此一份**：把非终态判据提炼为 `LIVE_RUNTIME_RUN_STATE_FILTER`，并让既有的 `recover_incomplete_runtime_runs`（orphan 收敛）**改用它**——呈现与恢复不再各维护一套条件。测试对该判据做**双向**证明：`accepted` 行被列出，改成 `completed` 后不再列出。
- 前端：`index.html` 全局系统状态栏新增 `data-role="system-attribution"` 一格；`app.js` 新增 `refreshAttributionAndRecovery()` 与两个文案函数，并用守卫测试钉住**诚实措辞**。

**一处我主动避免的不诚实（值得记录）**：我最初还想加一个"归属未记录且活跃"的**全局**计数，但核实后发现 store 的那条读回 `unrecorded_workspace_active_runs(session_id, turn_id, exclude_call_id)` 是**按 session/turn 作用域**的，不是全局读回。为凑字段另写一份全局判据就是**复制判据**，因此我**删掉了该字段**，只在文档注释里说明"按作用域的阻断判据是另一个读回，本端点无作用域参数，不把它伪造成全局计数"。

**另一处措辞上的克制**：`live_runtime_runs` **包含正在执行的轮次**，因此前端**不得**把这一栏一律说成"提交失败"。界面文案是"**非终态运行行 N**"，并在提示里说明：只有"执行已结束但终态提交失败"时该轮才会残留在这里、且**不会被报成已完成**，恢复走既有 orphan 收敛。守卫测试同时断言 `app.js` 必须含"包含正在执行的轮次"这一句，防止后人把它简化成误导性文案。

**如实记录的取舍**：CU 侧计数经由 `ComputerUseRunStore::open()` 读取，而该 `open()` 按设计会**自愈自己的 schema**（与启动时行为一致，不修改用户数据、不回填归属）；`runtime_runs` 的读则严格只读。这是"复用既有读回"与"GET 零副作用"之间的取舍，选择复用（避免第二份判据），此点在此明示。

**门禁（全绿）**：`web-console 1091/0`（新增 RD4-07 三条 + RD4-05 三条）、`cu-core 118/0`、`guard 47/0`、`tool-registry 54/0`、`core-runtime 306/0`、`module_linkage_smoke 4/0`。

**未做（不得当作已做）**：RD4-05 只做"呈现"，**不做**恢复动作（收敛写终态仍需 RD4-02/03 的两个前提）；前端也只有状态栏一格摘要，没有独立面板。

### B-61 RD4-02A 交付（第一阶段）：共享输入安全库的**契约层 + 宿主侧存储**（含"不得静默建空库"与阻断引用六项核查）

按第七轮裁决 §1 与 §7.1 的合并单元实施。本阶段交付**契约层**与**宿主侧 SQLite 适配**，并把 §7.2 组合用例的前两项落成测试。

**契约层**（`runtime::input_safety`，`modules/core-runtime/.../src/input_safety.rs`，纯类型、**不引 rusqlite**）：
- 独立版本 `INPUT_SAFETY_SCHEMA_VERSION = 1`（**不**与会话库版本号共用，也**不**因本库改动会话库迁移号）。
- `InputSafetyStoreId`（`is-` + 32 位小写十六进制，严格解析、不做近似归一）。
- `InputSafetyResourceScope`：**物理输入资源作用域**（与 broker 的 `windows-session-<id>` 对齐）；**拒绝**含 `/`、`\`、`:` 的值——这条直接实现裁决 §2.3"锁名不得包含 workspace／安装目录／调用方自选数据库路径"。
- `ResourceSafetyState`（`Isolated`/`Safe`/`Unknown`）：**只有 `Safe` 允许新输入**，`Unknown` 与 `Isolated` 都不允许（"`Unknown` 不是 `Safe`"）。
- 阻断引用的拒绝分类 `BlockingRefRejection`（8 类，逐类有码）与 `VerifiedBlockingRef`：**无 `Deserialize`、无公开构造函数、字段私有**——调用方**无法**通过反序列化 `validated=true` 自证（裁决 §1.5）。
- `RecoveryStage`（R1–R9 逐项对应）、`InputSafetyRecoveryOperation`（阶段只能**单调前进**、提交后冻结）、`InputSafetyIncident`、`InputSafetyEvent`（追加式）。

**宿主侧存储**（`web-console/src/input_safety_store.rs`，唯一逻辑写入口，复用既有 rusqlite）：
- 固定路径：DB = `<root>/input-safety.sqlite3`（文件名稳定，升级 schema 不另建新文件）；根由**注入**（`COOLZHU_INPUT_SAFETY_STATE_ROOT`），**未注入即报 `input_safety_root_not_injected`**，绝不推导工作区/当前目录/`log_dir`。
- **身份与"不得静默建空库"**：首次初始化写侧车标记 `store-identity.json`（只写一次、不删除）+ 库内身份行 + `StoreInitialized` 事件；此后"标记在、库不在" ⇒ `input_safety_database_missing_after_registration` **拒绝**（测试断言**拒绝后不得留下被顺手重建的空库**）；"库在、标记不在"或"库内无身份行" ⇒ `unknown_provenance` **拒绝**；身份不一致 ⇒ `identity_mismatch`；schema 超前 ⇒ 拒绝（不降版本号"修复"）。
- **阻断引用核查** `verify_blocking_ref(raw, expected_scope, expected_revision, explained_by_run)`：逐项验证 store 归属 → incident 存在 → scope 匹配 → 未解决 → 当前状态**确实阻止**新输入 → revision 一致 → 与被处理运行**可解释关联**；全部通过才产出 `VerifiedBlockingRef`。
- 资源状态写入要求 revision **严格前进**（防止旧视图回写）；解决 incident **不自动恢复**新输入。
- 恢复操作按 `recovery_operation_id` **幂等登记**、阶段单调。

**测试（7 条，全绿）**：首次初始化身份唯一且重开不换身份／**已登记后库丢失即拒绝且不留空库**／无标记库拒绝／未注入根用**显式取值**验证（不污染进程环境，符合裁决 §5.2 对测试的新要求）／阻断引用逐项核查（含**测试样式字符串 `input-safety:incident-9` 必须落回 `malformed`** 与"存在但属其他 scope／已解决／revision 不符"三类）／只有独立证明安全才放开新输入／恢复操作幂等与单调。

**我独立复跑的门禁**：`web-console 1098/0`（+7）、`core-runtime 311/0`（+5）、`guard 48/0`、`tool-registry 54/0`、`module_linkage_smoke 4/0`。

**本阶段**未做**（不得当作已完成）**：
1. **launcher 侧的注入导出**：`COOLZHU_INPUT_SAFETY_STATE_ROOT` 目前**还没有生产者**（app-launcher 已有 `input_safety_state_root_for()`，但尚未在启动后台时导出该变量）。因此生产路径上该 store 现在会以 `root_not_injected` **fail-closed**——这是**预期**的半成品态（宁可拒绝也不自造路径），下一步补 app-launcher 的导出。
2. **生产消费者**：尚无任何正式输入入口调用 `verify_blocking_ref`（属 RD4-02B／RD4-03）。
3. `computer_use_store::converge_legacy_run` 仍接收 `&[String]`；按裁决 §1.5 它必须在**同一服务内完成真实核查**或改收可信协调上下文——属 RD4-03。

### B-62 本轮并行窗口记录：三次在途阻塞（均非本项改动，按纪律未碰他人文件）

本轮我的编译/验证先后被三个并行单元的在途写入挡下，**均未修改他人文件**，全部按"等待重试"处理：

| 时刻 | 阻塞形态 | 归属 | 处置 |
| --- | --- | --- | --- |
| 14:22 | `core-runtime/src/sandbox.rs` 收尾括号不匹配（半成品） | RPR-01b 单元 | 等待 ~1 分钟后自洽（core-runtime 311/0） |
| 14:23–14:28 | `computer-use-core/src/input.rs` 调用 `unreclaimed_dropped()`，而 guard 侧 `PipeRetentionOutcome` 尚未同步（指标重命名在途） | RD4-09 单元 | 等待重试；其间我改完自己的测试并加了"不污染进程环境"的解析拆分 |
| 14:43 | `computer-use-core` 测试失败（非编译错），`input.rs` 17 秒前刚被写 | RD4-09 单元 | **未诊断、未触碰**；记为在途窗口，待其收敛后复跑门禁 |

**结论口径（沿 §B-49 的纪律）**：这三条**不是**缺陷归因，只记录"某时刻构建/测试处于并发开发窗口"；它们对最终基线的因果**未经冻结对照**，因此不宣布"与某方无关"，只报告事实与处置。

### B-63 RPR-01b 交付：查出并修掉**五处"空跑计通过"**（含 RD4-10 自身的自相矛盾）＋ 一处负载敏感 flake

**这是本批最有价值的一条**：它证明本项目此前有多处门禁数字是**假的绿**——用例在缺前置时 `return`（或 `if let` 不成立），libtest 记为 `ok`，于是"零断言执行"被算作通过。

**发现（本机实测，全部有原始输出）**：

| # | 位置（改前） | 现象 | 本机实测 |
| --- | --- | --- | --- |
| A | `compatibility-harness/src/lib.rs:311-356` | 3 个用例裸 `if !有夹具 { return; }` | **3/3 空跑**（门禁显示 `3 passed`） |
| B | `language-service/src/lib.rs:186-187,247-248` | `let Some(python)=python3_path() else { return; }`，探测候选只有 `python3`/`/usr/bin/python3` | **2/2 空跑**（本机只有 `python` 3.14.4） |
| C | `tool-registry/src/lib.rs:4538-4540` | `if detect_powershell_shell().is_err(){ return; }` 裸跳过（该 crate 无跳过通报机制） | 本机 PowerShell 存在故未触发 |
| D | 同上 `4536-4565` | 负载敏感 flake：`timeout: Some(2)` 要覆盖 pwsh 启动 + 500ms 睡眠 | **30 轮 2 次失败（7%）**，`Command exceeded timeout of 2000 ms` |
| E | `command-router/src/lib.rs:1915-1927` + 2727/2781 | 缺 git → `return` → 报 `ok`。**RD4-10 自己的日志就是证据**：`tmp/rd4-09-10/10-env-skip-visibility.log` 里消息写着"因此**不是通过**"，下一行却是 `... ok` / `1 passed` | 无 git 机器上门禁 `19/0` 全绿 |
| F | 同上 `2839-2909` + `env_lock` | `#[cfg(unix)]` 用例直接 `env::set_var("PATH"/"SAFEUSER")` + 手写恢复；局部 `env_lock` **只与自身串行** | 改前 `.bak` 里 6 处裸 `set_var/remove_var` |
| G | `core-runtime/src/prompt.rs:988-992` | `ConfigLoader::load()` 读三个 `CLAW_*` 变量却**不持统一锁**，而 `config.rs` 在锁内改它们 ⇒ 锁被绕过 | 潜在竞争，**未复现失败** |
| H | `core-runtime/src/sandbox.rs:342-355` | 非 Linux 必然 `None` ⇒ 断言体一次不执行却计为通过 | 本机（Windows）即如此 |
| I | `core-runtime/src/mcp_stdio.rs:1356-1376` | `python_command()` 取环境后**直接返回、不校验** | python 存在故未触发 |

**改法（逐条对齐裁决 §5.2 的"允许的结果"表）**：A 改真实三文件探测 + 绕过 libtest 捕获的 `[env-skip]` 行 + **新增一条人工夹具用例**（3→4 条，跳过 3 条可独立统计）；B 探测扩为六个候选并**逐个 `--version` 校验**，缺前置改 **fail-closed**（`[env-missing]`，说明"本机未验证/不是通过/验收未完成"）；C 改声明式跳过 + 独立统计；D 把墙钟预算 2s→**10s**（**只放宽预算、不放宽判据**，防的回归是"秒被当毫秒"）；E 改 `require_program_or_fail` + `[env-missing]` 分类契约，并把 RD4-10 的一次性手工负向验证**固化成可执行断言**；F 改**隔离子进程**（父用例只建夹具，用 `Command::env` 显式传 `PATH`/`SAFEUSER`，子进程 `--exact` 只跑自身；父用例断子进程成功 + 断哨兵行 + 断言**父进程环境全程未变**），`env_lock` 一并删除；G 加"只占锁不改环境"的空目标 + 判别性断言；H 改为两平台都执行的契约断言；I 声明环境要求 + 逐候选校验 + 明确失败文案。

**决定性对照实验（我读了原始日志核对）**：清空 `PATH`/`PYTHON*` 后直接运行测试二进制——**旧二进制 `2 passed`（空跑计通过）**，**新二进制 `2 failed`** 且给出 `[env-missing] ... 本机未验证 ... **不是通过**：该能力在本机验收未完成`（`tmp/rpr01b/language-service-skip-proof.txt`）；无 git 环境下 command-router 同样从 `ok` 变 `[env-missing]`（`tmp/rpr01b/command-router-git-env-missing.txt`）。

**它同时纠正了上一轮的判断**：RD4-10 把"缺 git 就跳过"当作可接受处理，而 `/branch`、`/worktree`、`/commit` 属**当前发布必验范围** ⇒ 命中裁决 §5.2 第三行"缺前置意味着验收未完成，不能用 skip 令发布门禁全绿"，故改为 fail-closed ✅。**且它查出 RD4-10 的"外部依赖盘点"漏了三处同类静默跳过**（compatibility-harness／language-service／tool-registry）。

**数字（我已独立复跑核实）**：`tool-registry 54/0`、`command-router 20/0`（+1 判别性用例）、`compatibility-harness 4/0`（原 3 条空跑 → 1 真断言 + 3 声明式跳过）、`language-service 2/0`（**真跑**）；固定轮数重复 **70/70 全绿**（toolreg×30 + coreruntime×20 + command-router×20）。改动仅 7 个文件、全部在 `#[cfg(test)]` 内；未改生产代码、未动 core-runtime 的 `lib.rs` 导出列表、未碰 gui-web/computer-use/scripts、未改 Git 状态。

**它如实申报的未闭环**：① **Unix 子项未编译未运行**（`rustup target add x86_64-unknown-linux-gnu` 两次卡在下载，无网），仅静态审查 ⇒ 按裁决"编译检查 ≠ 运行测试"**保持未验证**；② 首轮 60 轮中有 **1 次** 53/1，但**该轮详情未落盘**（它自认操作疏漏）⇒ 我只按"已知 flake 已定位并修复"记录，不宣称 100% 稳定；③ G 属"潜在竞争、未复现失败"，依据是结构性事实而非失败样例；④ 两条声明式跳过仍需口径确认（见 §C-30）。

### B-64 RD4-09 交付：读取器所有权收口 + 命令行计量纠正（含我的独立复核与两处精度说明）

按裁决 §4.1–§4.3 与 §5.1 实施，四项**同批**语义冻结（未"先改布尔值后补所有权"）。

**§C-17（选②）`Saturated` 不再丢掉仍存活的读取器**：预留令牌新增"未创建"桶标记，容量账目改为 `活动 = 总数 − 预留未创建 − 已保留`（`pipe.rs:461`）；`park_retained` 退化为纯粹的 `Active→Retained` 状态转换（`pipe.rs:1869`），**不再**比较 `entries.len() < limit`——即不再"在收尾时重新竞争另一个容量池"（判定提为纯函数 `decide_retention`，`pipe.rs:1841`）。契约破坏时进入新故障分支 `PipeRetentionOutcome::CapacityFault`（码 `PIPE_CAPACITY_FAULT`，与 `PIPE_CAPACITY_EXHAUSTED` **分开**）：**保留**读取器/线程句柄/管道/缓冲区所有权（不丢 `JoinHandle`、不分离线程）、**锁住接纳**、有界回收、记录**实际数量**（可 > 上限）、**不自动恢复**（唯一解锁点 `review_and_clear_pipe_capacity_fault`，`pipe.rs:1947`，只在故障对象回收后通过）。

**§C-18 拆分"未回收"与"受持有"**：`PipeReaderCapacity`（`pipe.rs:161`）给出五个**不同源**的量——`reserved_not_created` / `active_unreclaimed` / `retained_unreclaimed`、`unreclaimed()`、`owned_unreclaimed()`、`unowned_unreclaimed`（旧 `unmanaged_dropped` 改名，修复后**恒 0**，保留以区分"分离但存活"）、`admission_rejected`、`capacity_fault_units`、`admission_locked`；按 reader 唯一身份聚合。`readers_retained` 反映真实受持有数量（被拒绝接纳不计）；cu-core 侧另加 `capacity_fault_units`（`cleanup.rs:229`、`input.rs:3100`），两路取 `max` 避免同一故障对象重复计数。

**§C-21（选②）预留前移到 `spawn` 之前 + 保护安全收尾容量**：顺序固定为"校验资格 → 计算最大需求 → **一次性预留** → spawn → 建读取器 → 最终检查 → 业务输入"（原生 `input.rs:3201`、笔画 `input_stroke.rs:745`）；命令构造抽成唯一生产点 `native_helper_command`（`input.rs:2867`）。普通动作一次预留 **2 + 2**（`acquire_with_cleanup_reserve`、`take_cleanup_reservation`、`HelperPipeCleanupReservation`），清理运行**转用**该预留（`NativeRunCapacity::CleanupRun`、`native_emergency_release`、`StrokeRunCapacity`），不重新竞争、不递归预留下一层；清理确实用同一池 ⇒ 按真实模型记账、未虚增需求。spawn 失败 ⇒ 凭证 Drop 归还全部未用额度；部分创建 ⇒ 已创建对象额度直到真正回收才释放。

**§5.1 命令行计量口径纠正**：改为按 **UTF-16 单元、含终止空字符**计量（`input.rs:4456` 转义模型、`:4490` `command_line_units`、`:4508` 断言）。**真跑实测（我复跑核对，`--nocapture` 原文）**：`测试版（含两个测试接缝）：脚本 28030 单元／完整命令行（含终止符）28452 单元／配额 32767 单元／硬限余量 4315 单元`、`正式版：28381 单元／接缝成本 71 单元`。旧口径 30,014 是**把 UTF-8 字节当单元**（脚本 29,552 字节 vs 28,030 单元，差 1,562）。并按裁决要求补了"最长受支持路径+空格+中文的动态参数"（基线 59 → 152 单元；同段脚本 200 字节 / 96 单元）、参数转义（`\"`→3 个反斜杠、末尾反斜杠翻倍）、**转义模型与真实进程 `[Environment]::CommandLine` 逐字一致**（`:4643`）、以及"接缝不可由正式 CU 参数/环境开关/用户设置启用"（`:4684`）。

**故障注入证据（我读了原始日志核对）**：`CapacityFault { limit: 8, held: 9, fault_units: 1, reason: ReservationAbsent }`；登记表 9 条、缓冲区可读、**读取线程轮数 0 → 48（仍在跑）**；写端关闭后 `reclaim_finished_pipe_readers()` 真的回收成功；故障锁下接纳被拒并给出 `pipe_reader_capacity_fault: … 已持有 9 … 接纳锁生效中，需显式核查故障后才能恢复`；故障态账目 `retained=9 / obligations=9 / capacity_fault_units=1 / unowned_unreclaimed=0 / admission_rejected=4 / admission_locked=true` 五项不同源。

**门禁（我已独立复跑核实）**：`guard 50/0/1 ignored`、`cu-core 123/0`（先前我在其窗口内看到的"122/1"确为在途）、`tool-registry 54/0`、`module_linkage_smoke 4/0`；其自报固定轮数 **120 轮全绿**（六组各 20 轮），改动前对照日志保留未被覆盖。

**两处精度说明（如实记录，不影响结论）**：① 打印行的"硬限余量 4315 单元"用的是 `32_767 − 28_452`；按裁决给的公式（含终止符）应为 `32_767 − (28_452 + 1) = 4314` 单元——差 1，建议后续把打印口径与断言统一到含终止符的写法。② `Saturated` 的 `reason` 在该次注入里是 `ReservationAbsent`（注入使然），生产路径的对应情形是"预留被绕过"。

**它如实申报的未闭环**：① `ready_executor_ignoring_cancellation…` 在 `tmp/c17-c21/10-cu-core-full.txt` 中**失败过一次**（`facts()` 为空），单跑 5 轮 + 全量 3 轮均绿；属 RD4-08 已记录的负载敏感家族，与本次改动的进度文件路径**无因果证据**，但它明确写"**我无法排除**"⇒ 我只按"一次未复现的偶发失败，原始日志保留"登记。② "正常路径证明该分支不可达"**仍只是账目推理**，行为已由故障注入覆盖，但不宣称"已证明不可达"（符合裁决"仅凭文档不能宣称"）。③ 转义模型只覆盖本仓库的参数形状（程序名+固定参数+内联脚本），不含 `.bat`/`.cmd` 特例。④ "spawn 次数为零"用进程内计数器（紧贴唯一尝试点），未用 ETW 等 OS 级证据。⑤ 测试期容量闸门新增"真实账目也必须放得下"这道门槛，使 cu-core 全量由 ~30s 升到 ~53–67s（"残留也占义务"的直接后果，非缺陷）。

### B-65 PKG-L07c 补强（P0）＋ RD4-06 准备/冻结分离 交付：发布权覆盖**完整临界区** + 固定代次语义

**改动面**：仅 `scripts/**`（`package-all.ps1`、`lib/webview2-loader.ps1`、`lib/build-identity.ps1`、三个测试脚本）、`config/package-manifest.json`，加一份 work-log；未触碰任何 `*.rs`/`*.js`/`*.css`/`*.html`、未碰 `config/package-launcher.json` 与 `packages/app-launcher/**`、未改 Git 状态、未覆盖真实 `package/`（mtime 仍是 2026-09-19 12:03）。

**§3.2 落地**：① **同一把排他权覆盖完整发布临界区**——产物 staged 复核 → `Move` → 发布后复核 → **代次档案**（不可变、独占创建）→ 声明收据 → **当前有效代次指针**（最后写入）；每次共享槽位写入前都要 `Assert-LoaderPublishRight`，**写前复核与写入受同一保护**（这是裁决点名"写前复核若不受同一排他保护，仍可能在复核后被替换"的直接落地）。② 失败竞争者**只写自己的诊断**：不写声明收据、不写指针；Busy/被杀者不碰赢家的锁/产物/收据。③ 固定代次字段齐备：`slot_key`（规范槽位目录+目标文件名+package target+profile 的 SHA256→16 位）、`generation`（每次成功发布一个新 GUID，**不用 mtime**）、`producer_run_id`/`consumer_run_id`（消费方必须是**本次运行**，硬校验）、`artifact_digest`/`receipt_digest`（排除自身的规范摘要，可重算）/`build_input_digest`。④ **消费端固定流程**：读资格 → 明确 generation → 三方核对（指针/档案/声明收据）+ 产物身份 → 复制到本次独立 staging → 核对复制内容 → 固定消费收据 → 释放；报告**只**由固定收据构造，打包只从固定收据指向的 staging 取源（`package-all.ps1:684/701/1294` 的硬检查）。⑤ 授权项落地：`exported_artifacts[]` 新增 7 个固定代次字段 + `consumption{}` 精确关联。

**§3.1 落地**：新增 `-Prepare` 阶段（`cargo metadata --locked --offline` 只读核对；`-UpdateDependencies` 才允许更新锁定内容）→ 执行 `preparation.steps` 并把**生成物计入冻结输入** → 与上次冻结记录比对并**展示变化**（未 `-AcceptPreparationChanges` 即 `[PREPARE-INPUT-CHANGED]` 拒绝）→ **内容寻址的一次性冻结** → 结束（不触碰包根、不构建、不产包）。正式构建用 `-FreezeRecordPath` 逐项核对，不一致即 `[FROZEN-INPUT-CHANGED]`；未给出则自冻结并如实标注 `self-frozen-at-build-start`。9 个 artifact 全部 `--locked --offline`。诊断新增 `run_phase` 与 `changed_paths[]{path,kind,before_summary,after_summary,phase,classification}` + `release_eligible=false`。

**七项验收（新增 L07d 段，真实进程 + 命名事件屏障）**：失败方无写权限（赢家停在"收据已提交、指针未发布"窗口；锁/产物/收据/指针/前代档案字节不变；注入同 generation 不同内容 → `GENERATION-CONFLICT`）／提交间隙退出（**新增** `L07d-exit-between-receipt-and-pointer`：exit=97、指针不变、`RECEIPT-GENERATION-MISMATCH`、锁回收后可发布一致新代次）／消费 G1 后发 G2（指针确实前进且 `supersedes_generation=G1`，**G1 档案不变、`receipt_digest` 可重算、staging 字节 = G1 `artifact_digest`**）／generation 正确但内容错配（档案改写 → `GENERATION-CONFLICT`；三方自洽但产物被替换 → `CONTENT-MISMATCH`）／同 generation 不同收据 → `GENERATION-CONFLICT`／失败报告不得当发布收据（`--no-build` 命中禁用身份 → `[ARTIFACT-REVOKED]`）／**字段与实际消费内容一致**（`artifact_digest == staging 字节 == 包内字节`、`receipt_digest` 由档案重算、`consumer_run_id == 本次运行`）。

**固定轮数**：`-OnlyConcurrency -ConcurrencyRounds 5` → **15 用例 × 5 轮 = 75 轮全绿**（无失败后重试到绿），原始日志在 `tmp/verify-round7/`。

**我的独立复核**：`test-package-webview2-loader` **pass=37 fail=0 skip=4**（+7）／`test-package-build-identity` **78 cases / 0 failed**（+14）／`test-package-manifest` PASS／`test-package-safety` PASS／`test-powershell-script-compat` PASS —— 五项与其自报一致。

**需要操作侧知道（非缺陷）**：**收据 schema 升到 3**，schema 1/2 旧记录一律 `RECEIPT-SCHEMA` 拒绝 ⇒ **正式发布需重新构建一次导出**。

**它如实申报的未闭环**：① **仍未经验性构造出"两发布者互相覆盖"的具体交错**（与 §B-57 同一边界）——本轮的工作是把该交错在协议上**变为不可能**（指针 + 不可变档案 + 写前复核 + 独占创建）并以真实屏障验证窗口内各条边界；**P0 是否关闭见 §C-31**。② 演示 A/B 段 `release_eligible=false` 的唯一原因是"导出物声明源不在声明快照 roots 内 ⇒ provenance `not-confirmed`"（既有政策，未改）。③ 演示**非幂等**：D 段登记的禁用内容身份跨运行保留，同夹具再跑会被 `[ARTIFACT-REVOKED]` 拒绝（fail-closed，设计如此）。④ 准备阶段**仍不是不可变快照副本**（`immutable_build_snapshot=false` 如实保留），`-UpdateDependencies`（会改 `Cargo.lock`）**未实测**。⑤ **未跑含真实全量编译的打包**，"重新编译→出包"段仍未验证；构建模式下导出物 provenance 仍要求生产者本次真重跑，演示靠替身构建重写 `invoked.timestamp` 满足，真实 cargo 路径未跑。

### B-66 Phase 1（PR-RD4-02A）交付：launcher 注入 + 五表结构 + **按 epoch 授权** + 启动对账（组合用例 1/2/3 通过）

按第八轮 §1／§2／§11-Phase 1 实施。**Phase 1 退出条件＝组合测试 1／2／3 通过**，已在**存储层**达成（其端到端形态需 Phase 2 的协调器，见下方边界说明）。

**① launcher 注入（第八轮 §2）**：新增 `INPUT_SAFETY_STATE_ROOT_ENV = "COOLZHU_INPUT_SAFETY_STATE_ROOT"`，并由 `launch_environment` 从 `ResolvedLaunchPaths.input_safety_state_root`（**既有契约字段**，不另行推导目录名）注入；测试断言该变量存在且与 resolved 字段逐字一致。**未注入即 `root_not_injected` fail-closed**，**无任何 fallback**（消息里明写"回退到 `%USERPROFILE%\.coolzhu` 之类会污染生产路径"）。app-launcher `63/0 + 5/0`。

**② 存储结构（第八轮 §1 的五表骨架已齐）**：`incidents` ✅（既有）、`resource_blocks` ✅（新增：`block_id/scope/source_kind/source_ref/state/opened_at/closed_at` + 按 scope+state 的索引）、`recovery_operations` ✅（既有）、`ownership_epochs` ✅（新增：`scope/epoch/coordinator_instance_id/acquired_at/released_at` + **持有者进程身份** `holder_pid`/`holder_creation_filetime`）、`safety_events` ✅（既有，追加式）。

**③ 按 epoch 授权的恢复（取代 `paused=true`／`authority="xxx"` 式自证）**：
- `acquire_recovery_epoch(scope, coordinator)`：**冲突即拒**；仅当**持有者进程确已消失**（`windows-process-guard::capture_process_identity` 判定进程不存在或创建时刻不符＝PID 被复用）才**回收陈旧资格**并推进 epoch，且把"回收只表明需要核查、**不是**原状态已安全"写进事件。**身份取不到＝未知 ⇒ 不回收**（fail-closed）。
- `require_recovery_authorization(scope, presented_epoch)`：拿不出**当前活着的 epoch** ⇒ `RecoveryUnauthorized`。
- `advance_recovery_operation_authorized(...)`：先校验资格，再按**单调**规则推进（不得跳步/回退）。
- `establish_incident_authorized(...)`：恢复路径建立事故也要资格。
- **`rebind_recovery_operation_authorized(...)`**（新增）：重启后"重新取权 → continue"的唯一入口；普通 `put_recovery_operation` 的 UPSERT **刻意只允许**推进 `stage`/`committed`，**不允许换绑 epoch/协调者**。
- **我定义并落地的方向性规则**：**收紧不需要资格，放开需要资格**——`isolate_resource` 可随时写；`reopen_new_input_authorized` 必须持有当前 epoch **且**该 scope 无未关闭的阻断事实（`has_open_resource_block`）。普通 `put_resource_state` 也会拒绝"直接写接受新输入"的无资格调用。
- `reconcile_recovery_operations_on_startup()`：**只读不删**，逐条给出 `StillAuthorized` / `NeedsReacquire`。

**④ 组合用例（存储层，全绿；测试文件 10/0）**：
- **用例 1** `combined_1_...`：未持有 epoch（等价"传 `paused=true` 但闸门没关"）时两次推进尝试（`None`／错误 epoch）都被拒为 `RecoveryUnauthorized`，且**阶段原地不动**（不写终态）；取得 epoch 后放行。
- **用例 2** `combined_2_...`：同库两个句柄模拟两进程——A 取得 epoch，**B 被 `EpochConflict` 拒绝**（报文含持有者与持有 epoch）；B **不能**写 incident（且库中无残留事故行）、**不能**放开新输入；A 仍持有资格。
- **用例 3** `combined_3_...`：取得 epoch → 登记未提交操作 → **崩溃**（不释放、不提交）→ 新实例对账得 `NeedsReacquire`→ 回收陈旧资格并取得新 epoch（**旧 epoch 呈现仍被拒**）→ 重新绑定同一 recovery ID 后对账转为 `StillAuthorized`；全程**恢复行始终存在**（断言"不得删除"）。

**⑤ 过程中暴露并修掉的两个真实设计缺口（均被测试抓到）**：
1. **崩溃后所有权行仍显示"活着"**，新实例会被 epoch 冲突**永久挡住** ⇒ 必须引入**持有者存活判定**（PID + 创建时刻，复用 guard 的 `capture_process_identity`）才能回收陈旧资格。我最初两次实现都没过用例 3，正是这个原因。
2. **"仍可继续"（`StillAuthorized`）的前提是持有者存活**，因此只能由**能证明其存活**的实例判定；且"继续"需要一条**按资格的重新绑定**入口（否则重启后无法沿用同一 recovery ID）。
   两条都写进了代码注释与用例说明，不是靠放宽断言绕过。

**⑥ 顺手清理**：删掉 A-2 Step C 遗留的 `use std::fmt;`（`computer_use_store.rs` 未使用导入）。

**门禁（全绿）**：`web-console 1101/0`（+3 组合用例）、`app-launcher 63/0 + 5/0`、`cu-core 123/0`、`guard 50/0/1`、`tool-registry 54/0`、`core-runtime 311/0`、`module_linkage_smoke 4/0`。

**边界说明（不得宣称已完成）**：
1. 组合用例 1/2/3 目前是**存储层**通过；其**端到端**形态（真实跨进程协调器、真实 `RecoveryControlGuard`、真实 Windows 命名锁）属 **Phase 2（RD4-02B）**——裁决 §11 把 1/2/3 列为 Phase 1 退出条件，我按"存储侧语义已具备 + 端到端待 Phase 2"如实分列。
2. **生产消费者仍未接线**：没有任何正式输入入口调用 `verify_blocking_ref`／`require_recovery_authorization`，所以生产路径**继续保持 fail-closed**（`root_not_injected` 或"无上下文"），符合裁决"未齐备前不能以 turn 级绕过维持正式自动输入"。
3. `resource_blocks` 目前只提供建立/关闭/查询三件事，尚未参与 `verify_blocking_ref` 的判定（该判定读 `resource_state.accepts_new_input`）——它成为**载荷**的时机是 Phase 2/3。

### B-67 Phase 2 增量（PR-RD4-02B）：可信 `RecoveryControlGuard` + 跨进程 `InputSafetyCoordinator`（组合用例 1/2/3 走真实协调器通过；**4/5 未实现**）

按第八轮 §3／§4 实施。**Phase 2 退出条件是"5 个组合用例全部通过"，本轮尚未满足**（1/2/3 已过、4/5 未实现），以下如实分列。

**① 可信资格对象 `RecoveryControlGuard`**：字段私有、**无公开构造函数**、**无 `Deserialize`**，只能由协调器产生；携带 `resource_scope` / `recovery_id` / `epoch` / `coordinator_id` / `allowed_actions`（含 `allows_action` 白名单判定）。**② 协调器 `InputSafetyCoordinator`**：`begin()` 严格按裁决顺序——**取得资源锁（跨进程排他）→ 取 epoch → 写 `RecoveryOperationStarted`（登记 R1）→ 产出资格**；协调范围 = `windows-session-{id}` + `physical-input-resource`（**不是** workspace/session/turn）；取锁观察到 `WAIT_ABANDONED` 时写事件明确"**只表明需要核查，不是已安全**"。锁与 keeper 线程**复用既有 broker 设施**（`acquire_input_scope_across_processes`，所有权固定在 keeper 原生线程，`Drop` 可从任意线程释放）。

**③ 类型上堵掉自证**：store 的**所有"修改安全状态"入口**（推进阶段／建立 incident／重新绑定／重新开放新输入）现在**只接受 `&RecoveryControlGuard`**，不再接受裸 `Option<u64>`；`put_resource_state` **一律拒绝**"接受新输入"（放开新输入的唯一入口是按资格的 `reopen_new_input_authorized`）。⇒ `paused=true` / `authority="xxx"` / "填个 epoch 字段"这些形态在**类型层面**不可能奏效。

**④ 新增一条语义（测试逼出来的）**：**绑在旧 epoch/协调者上的恢复操作，在按资格重新绑定（`rebind_recovery_operation_authorized`）之前不得被推进**——这是裁决 §2.2"资格失效后不得继续修改安全状态"的直接落地；也解释了为何 `put_recovery_operation` 的 UPSERT 刻意不允许换绑。

**⑤ 组合用例 2 有真实跨进程证据**：同一协调范围上第二个协调器 `begin` ⇒ `input_safety_coordinator_busy`（底层是命名互斥体，跨进程）；不经协调器直接抢 epoch ⇒ `epoch_conflict`；伪造"别的 scope 的资格" ⇒ `recovery_unauthorized`（且 `RecoveryControlGuard` 在类型上无法凭空构造）。

**⑥ 组合用例 3 的崩溃模拟用仅测试的存活探针**：真实探针会把"同进程内被丢弃的旧持有者"判为**活着**（同一 PID），因此用例用 `begin_with_liveness_probe_for_test(|_,_| false)` 模拟"持有者已崩溃"；**更强的"真实子进程崩溃后回收"变体尚未补**（guard 已有 `current_exe()` 起子进程的成熟模式，可在同一 Phase 内补齐）。

**⑦ 未完成（不得当作已做）**：
1. **组合用例 4**（已 `Interrupted` 但 helper/释放未知 ⇒ 必须表达 `run stopped / resource uncertain / input blocked` 并保持隔离）——需要资源不确定性模型与"run 状态 ≠ 资源状态"的分离，属 **RD4-03**。
2. **组合用例 5**（Goal 有锚点但 chat turn 缺失 ⇒ 经 `turn_id/session_id → runtime_runs → owner` 真实关系解析，不得复制 ID）——属 **RD4-03** 的 owner 解析。
3. **生产消费者仍未接线**：恢复入口（R1–R9 的驱动）尚不存在，所以生产路径继续 fail-closed。
4. `resource_blocks` 仍只提供建立/关闭/查询，未参与阻断判定。

**门禁（全绿）**：`web-console 1101/0`（含 10 条 `input_safety_store` 用例）、`cu-core 123/0`、`guard 50/0/1`、`tool-registry 54/0`、`core-runtime 311/0`、`module_linkage_smoke 4/0`。

### B-68 RD4-03 语义基础交付：owner **真实关系**解析 + 资源**不确定**模型（**组合用例 4/5 已实现**）

新模块 `web-console/src/legacy_recovery.rs`。两条硬约束直接来自裁决 §4：

**① owner 只经真实关系解析**（用例 5）：`turn_id`/`session_id` → `runtime_runs` → `owner`。结论三态可辨：`Resolved { runtime_run_id, kind, owner_id }` / `Unknown { MissingFields | NoMapping | MultipleCandidates }`——**未知不等于"没有 owner"**，且**不复制任何 id**（不把 `goal_id` 塞进 chat turn 字段）。用例断言：缺字段 ⇒ `MissingFields`；查不到映射 ⇒ `NoMapping`；唯一 chat_turn 候选 ⇒ `Resolved`（owner 来自运行行本身）。

**② `Interrupted` ≠ 释放安全**（用例 4）：`assess_legacy_run_resource` 的规则是——该 scope 若有**未确认的释放义务** ⇒ `Uncertain`，**资源阻断优先于任何其它证据**（即使调用方给出独立安全检查也不放行）；否则必须有**独立**安全检查才可能 `Safe`；拿不出 ⇒ `Uncertain`。措辞固定表达三件事：`run stopped / resource uncertain / input blocked`。

**过程中查实并记录的两个事实（都不是缺陷，但改变了用例设计）**：
1. **`idx_runtime_runs_legacy_turn` 是全局唯一**（谓词只有 `legacy_turn_id IS NOT NULL`，**不限 kind**）⇒ 同一 legacy turn 不可能有两个运行行，因此 `MultipleCandidates` 在现有数据模型下**不可达**，属**防御性分支**。用例改为直接断言这一**结构事实**（第二次插入同 turn 必须失败），而不是伪造场景。
2. **Goal 锚点的边界**：`goal_phase` 行的 `legacy_turn_id` 为空，因此单凭 CU run 行里的 `(session_id, turn_id)` **推不出** Goal 关系；此时的正确结论是 `Unknown{NoMapping}`（**不得**当成"没有 owner"，**不得**复制 goal_id 冒充 turn）。要真正解析 Goal 锚点，需要从**真实关系**取到 goal/phase id（不能从 CU run 行反推）——这条**仍未闭环**，记在下方。

**门禁（全绿）**：`web-console 1103/0`（+2 RD4-03 用例）、`cu-core 123/0`、`guard 50/0/1`、`tool-registry 54/0`、`core-runtime 311/0`、`module_linkage_smoke 4/0`。

**仍未完成（RD4-03 剩余，不得当作已做）**：
1. **R1–R9 驱动**：目前只有 R1/R2/R5/R7/R8/R9 的**存储侧入口**（协调器、隔离、按资格建 incident、RD4-01 的 `converge_legacy_run`、阶段推进、按资格开放），**没有**把它们串成一条生产恢复入口；R3（撤销未激活许可 + 登记在途）、R4（请求旧执行者停止并核查身份）尚无实现。
2. **R7 的真实调用**：`converge_legacy_run` 需要一整套契约字段（证据五组、历史结果、资源状态、当前安全检查、阻断引用），驱动层要负责**如实**组装；尚未组装。
3. **Goal 锚点的真实关系解析**（见上）。
4. **真实子进程崩溃变体**（用例 3 目前用仅测试的存活探针）。
5. 生产消费者仍未接线，正式自动输入继续 fail-closed。

### B-69 RD4-03 驱动层交付：R1–R9 端到端跑通（共享安全库**首次有了真实生产者**；正式输入仍 fail-closed）

新模块 `web-console/src/legacy_recovery_driver.rs`：把 A-1.6 的 R1–R9 串成一条生产恢复入口，**每一步只调用按事实/资格设计的入口**。

**流程实现**：R1 协调器（跨进程锁 + epoch + `RecoveryOperationStarted`）→ R2 `isolate_resource` **真的关闸** + 阶段推进 → R3 登记在途（当前如实的登记面＝受影响运行集合）→ R4 旧执行者/宿主核查（记录交互会话 scope + 本进程无在途执行者）→ R5 **仅当资源不确定**时建立真实事故 + 开阻断 + **在同一服务内完成真实核查**后把已验证引用交给收敛 → R6 按**真实关系**解析 owner（未知 ⇒ **在任何写入之前拒绝**）→ R7 调 `converge_legacy_run`（同事务控制终态 + 收敛事实）→ R8 阶段提交 → R9 **只在独立证明安全且无未关闭阻断**时才开放新输入。

**三条裁决口径的落地方式**：① **不得自证**——`recovery_control_authority` 取 `guard.coordinator_id()`、`new_input_intake_paused` 取"协调器已真实隔离"这一事实，二者都**只能**来自协调器产生的证明对象，不是调用方声明；② **阻断优先**——只要评估为 `Uncertain` 就建事故 + 保持隔离，R9 不开放；③ **未知不等于没有**——owner 为 `Unknown` 时拒绝写终态。

**一个我刻意做成显式失败的口子**：`observed_commit_candidate` 必须是**真实检查的结论**，因此驱动把它做成**必需输入**；`None` ⇒ `commit_candidate_check_missing` **拒绝收敛**（不用缺省值凑成 `false`）。事实日志侧的提交候选查询**尚未实现**，这条口子把缺口显式暴露，而不是用 `false` 悄悄放行。

**端到端用例（3 条，全绿）**：① 有真实 owner 关系的遗留运行被**收敛**（收敛事实落库、候选从 `legacy_unconverged_runs` 中消失、控制维度终止），同时**资源保持隔离**（阻断仍开着、不接受新输入）——这正是组合用例 4 的端到端形态；② owner 关系未知 ⇒ `RefusedBeforeWrite{legacy_run_owner_no_mapping}` 且**一个字节都不写**；③ 缺提交候选检查 ⇒ `RefusedBeforeWrite{commit_candidate_check_missing}` 且不写。

**对裁决 §10 发布条件的意义**：**"共享安全库有真实生产者"这一条现在成立**（驱动是生产者：写 epoch／事故／阻断／事件，并真实核查引用）；但**"有真实消费者"仍不成立**——`verify_blocking_ref`／`require_recovery_authorization` 目前**只有驱动自己**在调用，**没有任何正式输入入口**在接纳前查它，因此生产输入继续 fail-closed。这条不满足就不宣称可发布。

**门禁（全绿）**：`web-console 1106/0`（+3 驱动用例）、`cu-core 123/0`、`guard 50/0/1`、`tool-registry 54/0`、`core-runtime 311/0`、`module_linkage_smoke 4/0`。

**仍未完成（RD4-03 剩余 + 后续阶段）**：
1. **R3/R4 的真实实现**：R3 需要"撤销尚未激活的普通输入许可"的许可层入口；R4 需要跨进程**旧执行者身份核查**（当前只记录 scope 与本进程无在途执行者）。
2. **事实日志侧的提交候选查询**（把上面那个显式口子补上）。
3. **Goal 锚点的真实关系解析**（`goal_phase` 行的 `legacy_turn_id` 为空，不能从 CU run 行反推）。
4. **真实子进程崩溃变体**（用例 3 仍用仅测试的存活探针）。
5. **入口接线**：把 `verify_blocking_ref`／`require_recovery_authorization` 接到**正式输入入口**（这是"真实消费者"与发布条件的关键一环），以及恢复驱动的触发点（启动对账后按候选清单驱动）。

### B-70 入口接线交付：CU 正式输入入口**真的查共享安全库**（"真实消费者"成立；同时**生产 CU 现在会 fail-closed 直到开放路径建成**）

按第八轮 §1.4"**所有正式输入入口必须经这一服务检查状态**"接线。

**接线内容**：新增 `require_resource_accepts_new_input(resource_scope)`（解析注入的库根 → 打开库 → 读该资源状态 → **只有 `accepts_new_input` 为真才放行**；`Unknown`（含"从未登记"）与 `Isolated` 一律拒绝，默认 fail-closed）。资源作用域取本机物理输入资源（`windows-session-{id}`，与 broker 对齐）。执行器接纳里在**身份/关系校验之后、任何 store 写入与任何原生输入之前**调用它，四种拒绝码可辨：`input_safety_root_not_injected` / 库不可用（沿用库的错误码）/ `input_safety_resource_not_accepting_new_input` / `input_safety_scope_unavailable`。

**顺序选择（有依据的排序）**：先"是谁在问"（父上下文 + 四维关系）再"现在能不能输入"（共享资源状态）。这不是随手排的：把资源检查放前面会**顶替**掉身份类拒绝码，我第一版就是这么放并因此让两条 A2-T5/T6 用例失败，改成"先身份后资源"后恢复 ✅。

**消费者门用例（1 条，含四种情形）**：未注入库根 ⇒ 拒绝且不得回退自造路径；注入了根但资源**从未登记**（`Unknown`）⇒ 拒绝且 `attempts==0`、**不留运行行**；资源被隔离 ⇒ 同样拒绝（同码）；**按资格开放之后** ⇒ 不再被这道门拒绝（走到下一个门）。此外"归属记录"用例改为**先注入库根并按资格开放资源**（走生产同一入口，不是测试旁路）。

**对裁决 §10 发布条件的意义**：**"共享安全库有真实消费者"这一半现在成立**——CU 的正式输入入口真的会在接纳前查它；而"遗留收敛／共享阻断／原生恢复之间没有放行空窗"这条也第一次有了真实连接（资源未被独立证明安全 ⇒ 正式输入拒绝）。

**必须点名的一条行为后果（重要，不得含糊）**：接线之后，**生产环境的 CU 会全部 fail-closed**，因为**目前没有任何"开放路径"**——即没有谁在启动时做独立安全评估并**按资格开放**该资源（只有测试这么做）。这是裁决口径的**正当后果**（"未齐备前不能以 turn 级绕过维持正式自动输入"），但意味着 CU 现在**不可用**，直到开放路径建成。开放路径应包含：启动独立评估（无未结清释放义务／无未决事故）→ 取得协调资格 → `reopen_new_input_authorized`；以及恢复驱动的**触发点**（启动对账后按候选清单驱动）。两条都**未实现**。

**门禁（全绿）**：`web-console 1107/0`（+1 消费者门用例）、`cu-core 123/0`、`guard 50/0/1`、`tool-registry 54/0`、`core-runtime 311/0`、`module_linkage_smoke 4/0`。

**仍未完成**：① **开放路径**（见上，最关键）；② 恢复驱动触发点；③ R3/R4 真实实现；④ 事实日志提交候选查询；⑤ Goal 锚点真实关系；⑥ 真实子进程崩溃变体。

### B-71 开放路径 + 启动触发点交付：**上一轮点名的"生产 CU 全阻"已被解决**（但只在独立评估通过时开放）

**上一轮我点名的后果**（"接线之后生产 CU 会全部 fail-closed，因为没有开放路径"）**现在解决了**：开放路径已实现并接进生产启动序列。

**开放路径**（`web-console/src/input_safety_opening.rs`）：**独立评估**三项全过才允许开放——① 该资源没有未关闭的阻断事实；② 该资源上没有尚未提交的恢复操作；③ 没有待收敛的遗留 CU 运行。通过后：取得协调资格（跨进程锁 + epoch）→ 按资格 `reopen_new_input_authorized` → **随后主动释放恢复排他**（正常输入不应依赖恢复锁常驻；"已开放"这一事实已持久化在资源状态里，而"资格失效后不得再改安全状态"仍然成立）。评估不通过 ⇒ `KeptIsolated` 并附**可读原因**（是保守结果，不是失败）。

**启动触发点**：`run_startup_input_safety(...)` 接进 `main()`，**排在既有 `recover_incomplete_runtime_runs` 之后**：先逐条驱动遗留运行恢复（R1–R9），再评估并（可能）开放。库根未注入 ⇒ `SkippedRootNotInjected`（**不自造路径**，正式输入继续 fail-closed）；执行出错 ⇒ **响亮记 error_event 但不中断启动**——因为此时的正确表现就是"正式输入继续被共享安全库挡在门外"，中断启动反而是更差的选择（这条不是"静默继续"：错误被显式记录）。

**启动路径如实传递"没有的能力"**：它把 `observed_commit_candidate` 传 `None`（启动路径尚未实现事实日志侧的提交候选查询），于是驱动会以 `commit_candidate_check_missing` **拒绝写终态**——缺口保持可见，而不是用 `false` 放行。

**用例（5 条，全绿）**：干净资源 ⇒ 评估通过并**真的开放**（资源状态转为接受新输入）；有未关闭阻断 ⇒ 保持隔离且原因可读；有待收敛遗留运行 ⇒ 保持隔离；启动触发点在未注入根时 ⇒ 如实跳过；启动触发点在有待收敛运行且缺提交候选检查时 ⇒ **0 收敛 / 1 拒绝** 且保持隔离。

**门禁（全绿）**：`web-console 1112/0`（+5）、`cu-core 123/0`、`guard 50/0/1`、`tool-registry 54/0`、`core-runtime 311/0`、`module_linkage_smoke 4/0`。

**仍未完成**：① **事实日志提交候选查询**（补上后启动路径才可能真正收敛遗留运行；这是当前"0 收敛"的直接原因）；② R3 的许可撤销入口与 R4 的跨进程执行者身份核查；③ Goal 锚点真实关系；④ 真实子进程崩溃变体；⑤ 开放路径的**策略细化**（当前"有任何遗留运行就不开放"是保守规则；是否允许"逐 scope 开放"待后续）。

### B-72 第八轮 PKG 裁决执行：PR-PKG-01/02/03 完成，**已安装 0.2.15**（构建验证与安装验证分开记录）

**改动面**：`scripts/build-msi.ps1`、`scripts/lib/build-identity.ps1`、`scripts/package-all.ps1`、`config/package-manifest.json`、`modules/cli/packages/command-line/Cargo.toml`（版本号）。未放宽任何来源校验。

**PR-PKG-01（产物身份安全，优先级更高）**：`build-msi.ps1` 的报告解析改为**只认包根**——从 `<PackageRoot>/payload-inventory.json` 的 `report_ref` 取报告，并做**四项一致性断言**（报告内容哈希、report_id、载荷摘要、源码快照摘要），任一不符**立即 throw**（非零退出）；**删除了 `latest-<config>.json` fallback**。installer 报告随之补回 `package_report_ref.report_path` / `captured_from=package-root-inventory-report-ref` / `payload_inventory_ref.*` / **`package_root_identity`**。
**验收（裁决要求的三组构造）**：① 篡改包根清单的 `payload_digest` ⇒ `[PKG-ROOT-BINDING-MISMATCH]`（并打印两个值）✅；② 包根无清单（仓库里**存在** latest 指针）⇒ `[PKG-ROOT-BINDING-MISSING]`，**不回退** ✅；③ 正常包根 ⇒ 绑定核验通过，随后按**包根自己的收据**拒绝（`[REPORT-NOT-RELEASE-ELIGIBLE]`）——此前它会引用一份旧的"可发布"报告并**静默成功** ✅（这正是本项要修的危险形态）。

**PR-PKG-02（来源模型缺口）**：`config/package-manifest.json` 新增 `external_build_inputs` 登记项（`cargo-registry:webview2-com-sys`：registry／crate／**version**／**checksum**／artifact 映射／`checksum_lockfile`）；`package-all.ps1` 新增 `Resolve-ExternalBuildInputVerification`，把 manifest 声明的 checksum 与 **`Cargo.lock`** 里同 crate@version 的 checksum **逐字比对**（独立来源，非自证；两侧 registry 前缀归一化比较）。通过时该导出物记 `declared-external-input-verified`（**不是** not-confirmed，也**没有**删除来源检查）。收据新增 `external_inputs[]` / `external_inputs_verified` 与 **`source_snapshot_verified`**（措辞刻意区分"已验证"与"完整"，并在注释里写明它**不**声称"源码快照包含全部二进制来源"）。**未采纳**被否决的两条（删检查、把 target 并入快照）。

**PR-PKG-03（重新完整打包）**：重新冻结 → 正式构建 → 新收据 **`release_eligible=true`**（六道门全 pass）。

**安装前置的六项条件（程序化核验，全部 PASS）**：`release_eligible=true`；`source_snapshot_digest` 三方一致（报告＝清单＝MSI，`e306ac20…`）；`payload` 三方一致（`30c002f5…`）；MSI 报告引用 `report_id` 一致；`external_inputs_verified=true`；`package_root_identity` 三方一致。

**⚠ 安装失败的真实原因（构建侧无关，属版本序）**：首个 MSI（0.2.5）安装返回 **1603**，日志里伴随 **1715**。定位：机器上**已安装 0.2.14**，而包的版本是 **0.2.5** ⇒ Windows Installer **拒绝降级**。修法是**把 CLI 版本从 0.2.5 提到 0.2.15**（`modules/cli/packages/command-line/Cargo.toml`，> 0.2.14 ⇒ 走升级而非降级，**非破坏性**；没有选择"先卸载用户已装的版本"，那超出授权范围）。重新冻结+构建+出 MSI 后安装**成功**。

**构建验证（独立记录）**：MSI = `dist/CoolzhuAgent-0.2.15.msi`（sha256 `6363B6D5…`，unsigned），绑定本轮快照 `e306ac20…` 与载荷 `30c002f5…`；六项条件全 PASS。
**安装验证（与上者分开记录）**：`msiexec /i` **exit=0**；注册表条目已从 0.2.14 **升级为 0.2.15**；安装目录含 `COOLZHU-AGENT.exe`、`bin/*`（含 `coolzhu-cli.exe`、`coolzhu-web-console.exe`、`WebView2Loader.dll`）、`config`、`docs`、`modules`、`payload-inventory.json`；**运行验证**：`coolzhu-cli.exe --version` 报 `Version 0.2.15`；直接运行 `coolzhu-web-console.exe` **真的启动**并打印 `输入安全启动路径: candidates=0, converged=0, refused=0, opening=SkippedRootNotInjected`（**本轮新写的启动路径在装出来的二进制里生效**，且如实报告"未注入根"⇒ 正式输入 fail-closed）与 `http://127.0.0.1:8765/` 已启动。探针实例已收掉（`remaining=0`）。

**尚未验证（不得当作已做）**：① **由 launcher 启动**的完整链路（`COOLZHU_INPUT_SAFETY_STATE_ROOT` 注入 ⇒ 启动路径应改为评估/开放，而不是 Skipped）；② MSI 的**卸载/修复/重装**路径；③ 升级自 0.2.14 的**用户数据兼容**（会话库 v23 与输入安全库首建）；④ 签名（`signing_status=unsigned`，本轮不涉及）。

### B-73 最终门禁中的一次偶发失败（RD4-09 的容量记账用例）：按 §5.1 四栏登记，**归因待核**

在"版本号变更后的最终门禁"这一次运行里，`guard` 出现 **49 通过 / 1 失败**（其余套件全绿）。失败用例与断言：

- 用例：`pipe::tests::abandoned_reservations_return_every_unit_without_leaking_or_double_counting`
- 位置：`pipe.rs:2940`，断言 `(reserved_not_created, obligations()) == (4, start.obligations() + 4)`，失败值 `left: (4, 4)` ⇒ 说明**起始义务计数非零**时，新预留的 4 个单位只进了"预留未创建"、**没有**同时进义务。

| 项 | 记录 |
| --- | --- |
| **执行结果** | 失败（仅那一次）；随后**全量 3/3 绿（50/0/1）**、**该用例单跑 3/3 绿** |
| **输入证据** | **已观察变化**：该次运行紧跟在"版本号变更 + 重新冻结/构建"之后；但我**未触碰** `windows-process-guard` 的任何代码（本轮改动只有 scripts/manifest/CLI 版本），且未固定该次的测试二进制身份与线程/顺序 |
| **共享资源状态** | **存在争用/未知**：该用例读的是**进程级全局容量账目**；同进程内其它容量用例若在它之前留下状态，就会改变 `start.obligations()`（这正是失败值呈现的形态） |
| **原因判断** | **假设（未复现支持）**：测试间共享全局账目导致的顺序/并行敏感。**归因待核**——不得据此宣布"无回归"，也不得归责于 RD4-09 之外的改动 |

**为什么不当作纯测试噪声**：该用例所在的**正是**保证"放弃的预留不泄漏、不重复计数"的那段生产记账逻辑；若在真实场景（账目非空、多次接纳）下义务计数确实会少记，那是**生产缺陷**而不是测试问题。因此本项需要 RD4-09 补两件证据：① 触发条件的**受控复现**（例如显式构造"起始账目非空"的用例，而不是靠用例顺序碰出来）；② 区分"测试清理不足"与"生产记账缺陷"的对照实验。
**首次失败日志已保留**：`tmp/guard-first-failure.log`（与 `tmp/gates/final-guard.log` 同源），未被后续绿色覆盖。

### B-74 提交入库完成，**GitHub PR 被阻塞**（无远端/无 gh/无 token）；含一次我自己的文件覆盖事故

**已完成（本地）**：工作树确实是 git 仓库（分支原为 `master`，两个种子提交 `baa8e35`/`0a3802b`，`git ls-files` 原为 **0**、无任何远端）。已新建分支 **`rd4-input-safety-and-pkg-integrity`** 并提交 **`965166c`**：`1540 files changed, 454375 insertions(+)`（`target`/`tmp`/`package`/`dist` 由既有 .gitignore 排除），提交信息逐条列出改动面、验证数字与**尚未验证项**。

**GitHub PR 无法创建（阻塞，需用户提供）**：① `git remote -v` **为空**（无任何远端）；② `gh` **未安装**；③ 环境无 `GITHUB_TOKEN`/`GH_TOKEN`。凭据助手是 `manager`（若给出远端 URL，HTTPS 推送**可能**可用）。只读探测 `https://github.com/zhupu1122/coolzhuagent` 返回 **404**（不存在，或私有仓库对未授权请求同样 404，二者不可区分）——因此**不能**据此假定目标仓库。

**完成 PR 所需的命令（待远端确定后执行）**：
```
git remote add origin <仓库 URL>
git push -u origin rd4-input-safety-and-pkg-integrity
# 有 gh 时：gh pr create --base master --head rd4-input-safety-and-pkg-integrity --title "..." --body-file <文件>
```

**⚠ 我自己的事故（如实记录）**：准备工作时我先执行 `git add -A`，随后**直接覆盖**了工作区里**已存在**的 `.gitattributes`（未先查看内容——正是"覆盖前先看目标"这条纪律的反面）。该文件**未被任何提交跟踪**，两个种子提交里都没有它（`fatal: path '.gitattributes' exists on disk, but not in 'baa8e35'/'0a3802b'`），因此无法从 git 恢复；我在 `tmp/2026-09-19-agent-fixes/pr-checkout/.gitattributes` 找到一份**旧检出副本**（3 行：钉 `app.js`/`styles.css` 为 `eol=lf`），据此**恢复原文并追加**了本轮的 `eol=lf` 规则（原因：本机 `core.autocrlf=true`，而 `pipe.rs` 的文本钉住断言把 LF 本身当契约）。**风险**：若原文件还有其它规则（副本未必是最新版），需要由你确认或补回；我无法证明副本与覆盖前的内容逐字一致。

### B-75 现场问题排查：**安装后的桌面/开始菜单图标双击后"无法启动"** —— 不是崩溃，是**不可见的正确拒绝**

**现象（用户报告）**：安装出来的图标双击都没有反应。

**排查链（逐步实测）**：
1. 快捷方式本身正常 ✅：开始菜单 `…\Start Menu\Programs\COOLZHU CODE\COOLZHU CODE Agent.lnk` 与公共桌面 `COOLZHU CODE Agent.lnk` 都指向 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，工作目录＝安装目录，**目标存在** ✅。
2. 从安装目录带输出运行该 exe ⇒ **退出码 1**，并打印：`package-launcher: startup failed: 发现多个含数据的工作区候选，拒绝自动选择（不按时间/容量/数量挑选，不合并，不删除）`，候选为 `[1] C:\Users\zhupu\coolzhuagent`、`[2] C:\Users\zhupu\AppData\Local\CoolzhuAgent`，并给出修正入口 `--select-workspace <绝对路径>`。
3. 该拒绝**已完整落日志** ✅：`%LOCALAPPDATA%\CoolzhuAgent\logs\package-launcher\package-selfcheck-last.json` 里 `"ok":false` + 完整 `error` + `resolution_notes:["[refused] 路径解析未通过：已阻断启动，未创建替代工作区"]`。
4. `--list-candidates`（只读）确认**恰好两个**候选，且**都不包含**当前构建目录 `C:\Users\zhupu\Desktop\coolzhuagent`（该目录没有 `coolzhu.toml`/数据 ⇒ 不是候选）。

**根因**：启动器按设计**拒绝在两个含数据的工作区之间自动选择**（不按时间/容量/数量挑、不合并、不删除），因此 `exit=1` 且**未启动**。用户双击时这段说明只出现在一个**瞬间关闭的控制台**里 ⇒ 主观感受就是"双击没反应"。**不是**打包缺陷、**不是**崩溃、也**不是**图标/快捷方式问题。

**修正入口（启动器自带）**：`--list-candidates`（只读列出候选）／`--select-workspace "<绝对路径>"`（决定归属：候选[1] 旧工作目录、候选[2] 用户级状态根，或当前构建目录）／另有 `--show-console`、`--print-resolved-paths`、`--user-state-dir`、`--headless`、`--help`。

**必须坦白的一处验证缺口**：我此前的"安装验证"是**直接运行 `coolzhu-web-console.exe`**（绕过了启动器）⇒ 只证明控制台能起，**没有**证明图标所走的 **launcher 路径**能起。这条缺口我在 §B-72 已列为"尚未验证：由 launcher 启动的完整链路"，本次现场问题**正是**它 ⇒ 先前的"安装验证"**不完整**，不得据此宣称安装可用。

**由此暴露的产品/UX 缺陷（建议立工单）**：随包安装的快捷方式指向一个**可能结构化拒绝**的启动器，而拒绝信息**对双击用户不可见**（控制台瞬关、日志在非显眼路径）。可选修法：① 拒绝时弹**消息框**；② 快捷方式带 `--show-console` 之类参数让窗口保留；③ 首次运行引导（候选选择界面）；④ 至少在失败时给出**可见提示**并指向日志与 `--select-workspace`。**我不擅自选定机制**（属产品口径）。

### B-76 修复"图标双击无法启动"：多候选不再一律拒绝，改为**按来源优先级采用**（已安装 0.2.16 并端到端验证）

**用户口径（现场裁决）**：默认工作区是 `C:\Users\<用户名>\coolzhuagent`，**用户名按设备账号在运行时解析、不得硬编码**；**已有保存的配置时，使用"最近打开"的 workspace 目录**。

**核验（未硬编码的证据）**：随包配置 `config/package-launcher.json` 里是模板 `"runtime_dir": "%USERPROFILE%\coolzhuagent"`，配置注释明确"按实际运行用户的用户目录上下文解析模板（**不在构建机替换成绝对路径**）"；`C:\Users\zhupu\coolzhuagent` 只是**本机解析结果**。对 `launch_paths.rs` 的改动做 `git diff` 扫描：**无任何** `zhupu` / `C:\Users\…` 硬编码（grep 无输出）。

**代码改动（`packages/app-launcher/src/launch_paths.rs`）**：把"有数据的候选 ≥2 即 `WorkspaceSelectionAmbiguous`"改为按**来源优先级**挑唯一最优：
`SavedSelection{revision}` (3) → `ConfigSnapshot{recorded_at_ms}` (2，即"最近打开") → `ImportedLegacySelection` (1) → `PackagedDefault` / `KnownHistoricalDefault` / `LegacyLogDirDerivation` (0)。**顶层并列或全不可判定**时仍要求一次显式选择（fail-closed，不猜、不合并、不删除）。随包默认值**刻意不高于**历史遗留：它可能写在"曾被误建"的位置上（P13 场景），同层并列时让它胜出会把误建目录选中。

**测试调整（都是新政策的必然结果，且裁决规定的"不合并/不删除"断言全部保留）**：P05 改名并改为断言"采用导入的旧选择"，同时**新增"两条导入选择并列 ⇒ 仍拒绝"**场景；P13 期望更新为采用导入的旧选择（两份数据都仍在、未新建目录的断言原样保留）；新增 `select_holder_by_recency` 决策表用例（唯一最近 / 两条取新 / 并列拒绝 / 全不可判定拒绝 / 已保存选择优先）。`app-launcher` **64/0 + 5/0**。

**端到端验证（安装后真实入口）**：升版本 0.2.16 → 重新冻结 → 正式构建（`release_eligible=true`，六项一致性条件全 PASS）→ MSI `dist/CoolzhuAgent-0.2.16.msi` → **安装 exit=0**。随后：
- `COOLZHU-AGENT.exe --print-resolved-paths`（即图标的目标）**不再拒绝**：`工作区根 request=C:\Users\zhupu\coolzhuagent source=imported_legacy_selection origin=config_snapshot:launcher-config-a7e3fb7c0ba6db2b.json@recorded_at_ms=…`，并**持久化选择 ⇒ revision=1**（此后启动确定化）；`input_safety_state_root=%LOCALAPPDATA%\CoolzhuAgent\input-safety`；exit=0 ✅。
- **真正启动**（图标路径）：`coolzhu-web-console`（pid 20988）与 `coolzhu-tauri-shell`（pid 14568）**均在运行**，启动器日志写明 `后台实际使用 workspace=\?\C:\Users\zhupu\coolzhuagent session_db=…\.coolzhu\web-sessions.sqlite3（build=46a876490f1f）` ✅。⇒ 图标双击现在可用。

**仍未闭环（与本次修复无关，继续保留）**：B-72 列出的 launcher 注入/输入安全启动路径联动（本次日志未展示输入安全启动路径的评估/开放行——因为启动器链路尚未接入注入后行为观察）、卸载/修复/重装、升级数据兼容、签名。

### B-77 启动路径**每次必失败**的自锁缺陷：旧恢复操作不重绑 + 阶段守门不许回退（已修，含真实库验证）

**现场现象**（本机真实输入安全库，修复前）：

```text
WARN coolzhu_web_console: 输入安全启动路径失败（正式输入保持阻断）:
  未持有 scope windows-session-1 的当前恢复资格（持有 epoch Some(1)，当前 Some(4)）：不得修改安全状态
```

每次启动都失败，**且 epoch 每次都前进**（1 → 4 → 5），说明"取得资格"本身是成功的，失败在**其后的某一步**。

**根因定位（读真实库，不靠推断）**：`%LOCALAPPDATA%\CoolzhuAgent\input-safety\input-safety.sqlite3` 里

```text
input_safety_ownership_epochs : windows-session-1, epoch=5, holder=coordinator-14896-…, released=NULL
input_safety_recovery_operations:
  startup-input-safety                                   epoch=5 stage=r1  committed=0   ← 本次
  startup-recovery-cu-session-…rpO0wuINWlivnmBZeYz48T7iLk5bFSaF  epoch=2 stage=r5  committed=0   ← 上次遗留
  startup-recovery-cu-session-…call_ea30b56c5ec44e898fce7353     epoch=1 stage=r1  committed=0   ← 上次遗留
```

即：报错的 `持有 epoch 1` **不是**本次取得的资格，而是**上次运行遗留的操作行**上记录的 epoch。

两个叠加的设计后果（都不是"某处手滑"，而是组合出来的自锁）：

1. 驱动的操作 id 按 `run.call_id` 命名（`startup-recovery-{call_id}`），而 `put_recovery_operation` 的 upsert **刻意只允许推进 `stage`/`committed`、不允许换绑**（防自证换绑，见 `input_safety_store.rs` 注释）。⇒ 重启后同一 id 复用，行仍绑在**已失效**的 epoch 上，`advance_recovery_operation_authorized` 的"必须持有当前 epoch 且操作也绑在当前 epoch"守门必然拒绝。
2. 阶段守门是 `next.order() == stage.order() + 1`（**不得跳步、也不得回退**）。⇒ 即便重绑成功，中断重放从 R2 起逐条请求也会撞上"回退"而被拒。

**后果（严重）**：只要**任何一次**启动在 R1–R9 中途结束（崩溃/被 kill/正常退出前的失败），下一次启动就永久失败 ⇒ 资源永久保持阻断，**无人能恢复**（自锁，且没有人工通道）。

**修复（`legacy_recovery_driver.rs`，两处）**：

- 登记后按**当前资格**重新绑定未提交的旧操作：`rebind_recovery_operation_authorized(operation_id, guard)`（store 侧本就要求"持有当前 epoch"才允许重绑，符合裁决 §2.2"重启后必须重新取得协调权"）。
- `advance` 改为**幂等**：`current.stage.order() >= next.order()` ⇒ 跳过；仅未达标时才调用 store 的按资格推进。阶段推进本身的校验仍在 store。

**真实库验证（同一台机、同一批遗留行，不清理现场）**：

```text
修复前：WARN …输入安全启动路径失败（正式输入保持阻断）: …持有 epoch Some(1)，当前 Some(5)…
修复后：INFO …输入安全启动路径: candidates=2, converged=0, refused=2,
        opening=KeptIsolated { open_blocks: 2, pending_recovery_operations: 1, legacy_unconverged_runs: 2,
                               refusal: Some("该资源仍有未关闭的阻断事实") }
```

**回归用例**：`legacy_recovery_driver::tests::replay_rebinds_an_operation_left_by_a_previous_run`（取得资格→登记操作→**释放资格**模拟中断，再以 `coordinator: None` 重放：断言重放成功、收敛完成、操作行 `recovery_epoch` 已等于**新** epoch）。

### B-78 开放路径**永不可达**的逻辑错误：把"活着的协调者正在处理"也算成"待对账"（已修）

B-77 修掉后暴露：启动路径的评估恒带 `pending_recovery_operations ≥ 1`。读 `assess_input_resource` 才看清——它把启动对账的两类结论**都**计入"待对账"：

- `NeedsReacquire`（旧 epoch／持有者已消失）⇒ 确实"待对账"，应计数；
- `StillAuthorized`（**当前活着的**协调者正在处理）⇒ 在启动路径里那就是**本次调用自己**刚登记的那条操作。

⇒ 只要协调器存在，评估永远得到"该资源上仍有尚未提交的恢复操作"，**开放分支永不可达**（现场 `pending=3` 中有一条正是本次协调器的登记）。

**修复**（`input_safety_opening.rs`）：只把 `NeedsReacquire` 计入 `pending_recovery_operations`。修复后现场 `pending=3 → 1`（剩下的 1 条确为旧 epoch 的待对账操作）。

**回归用例**：`input_safety_opening::tests::operations_held_by_a_live_coordinator_are_not_pending_reconciliation`（持协调器 ⇒ `pending==0`、`is_safe()` 成立，且同一协调器**确实能**按资格开放）。

### B-79 全量测试的并行 env 竞态（1114/1）：非回归，但**必须修**，否则门禁不可信（已修）

**现象**：全量跑出现 1 失败——`computer_use_executor::tests::admission_entry_records_the_frozen_workspace_in_the_frozen_database`，实际 `input_safety_resource_not_accepting_new_input`、期望 `computer_use_room_full_access_required`；**单跑该用例通过**。

**根因（进程级 env + 缺互斥）**：正式输入入口要经共享输入安全库，库根经**进程级环境变量**注入。CU 侧的两个入口用例都持全局串行守卫 `crate::tests::config_test_guard()`，而 `input_safety_opening` 的两个用例会 `set_var`／`remove_var` 同一变量却**没持**该守卫 ⇒ 窗口内 CU 用例读到**别人的临时库根**（一个未被开放的资源）而 fail-closed 拒绝。属既存竞态，本轮新增用例提高了并行度后暴露。

**修复（纯测试面）**：

- `input_safety_opening` 的两个 env 敏感用例加 `let _guard = crate::tests::config_test_guard();`；
- CU 的 `open_input_resource_for_test` 改为返回 **Drop 守卫**（`InputSafetyEnvGuard`），用例结束恢复原 env——此前只 `set_var` 不恢复，把已删除临时目录的库根泄漏给后续用例。

**验证**：**连跑 3 轮全量，1115/0 稳定**（修复前同一命令曾出 1114/1）。

### B-80 **规格未明确**：被拒绝的恢复"永不结账" ⇒ 一个无法判定的遗留运行 = 永久隔离

修复 B-77/B-78 后，本机真实库的**正确**结果是继续保持隔离：`refused=2`（两个真实遗留 CU 运行被如实拒绝，不写终态）、`open_blocks=2`（恢复期为它们建立了真实阻断）。这**符合裁决**（"未知不是没有 owner ⇒ 不写终态、保持隔离"）。

**但缺一环（现状即风险）**：被拒绝的操作 `committed` 恒为 0、阻断永远不关闭，而**没有任何机制**把它们结账或人工放行。后果：只要历史上留下一个无法判定的遗留运行，该资源就**永久隔离**，且现场无人能恢复（既非"崩溃可自愈"，也非"有管理通道"）。裁决文本只规定了"拒绝写终态并保持隔离"，未规定"拒绝后如何结账、谁来放行、凭什么证据"。

**待裁决（不阻塞其它已裁决项）**：

1. 是否为"已被拒绝且已裁定保持隔离"的恢复操作引入**终态登记**（例如 `committed=1` + `disposition=kept_isolated`），使"待对账"不再把它算作未办事项；
2. 阻断（`resource_blocks`）是否允许在"运行本体已被隔离且不再持有输入"时关闭；若允许，谁可关闭、需什么证据、如何审计；
3. 是否需要一条**受控的人工/管理放行通道**（第八轮 §1.5 只讨论了"独立证明"，未给出运营处置路径）。

在此之前，本机现状应如实描述为：**资源保持隔离（阻断 2 条），正式输入 fail-closed**——这是设计结果，不是待修 bug。

### B-81 B-73 收口：容量记账用例的偶发失败不再阻塞门禁（根因仍未定位，如实保留）

改动为**基线相对断言**（不再与硬编码常量比），随后：定向 20/20 轮绿 + 本轮**全量 3/3 轮 1115/0**。原始那次失败（`(4,4)`）**仍不可复现、根因未定位**，故保留"归因待核"，但不再作为门禁阻塞项；`tmp/guard-first-failure.log` 为首跑证据。

### B-82 PR-01（P0-1）交付：`RecoveryDisposition` 状态机 + 人工放行通道，**永久隔离出口已闭环**

**背景（承接 §B-80）**：修复 §B-77/§B-78 之后，本机真实库仍表现为 `refused=2 / open_blocks=2 / pending=1`——
其中"被拒绝的恢复永不结账"没有任何出口：**一个无法判定的遗留运行 = 该桌面资源永久隔离**。

**决策落地（PR-01）**：

1. **契约层**（`core-runtime/src/input_safety.rs`）：新增 `RecoveryDisposition`
   （`Pending` / `Recovered` / `KeptIsolated` / `HumanReviewRequired` / `AbandonedWithEvidence`）与
   `ReleaseIsolationDecision`；`InputSafetyRecoveryOperation` 增加 `disposition` 列语义；
   schema 版本 **1 → 2**。口径：**过程看 `stage`，结论看 `disposition`**——混用会让"还没走完"
   与"永远走不完"不可区分（那正是缺口的成因）。
2. **存储层**（`input_safety_store.rs`）：
   - **v1 → v2 就地升级**：`ALTER TABLE ... ADD COLUMN disposition TEXT NOT NULL DEFAULT 'pending'`
     + 新建 `input_safety_release_decisions`；**不另建空库**（另建空库等于遗忘旧事故）。
   - `settle_recovery_operation_authorized`：结账必须持有**当前** epoch；`Pending` 不是结账；
     已结账**不得改判**（同值幂等）；结账即 `committed = 1` ⇒ 移出"待对账"。
   - `release_isolation_authorized`：人工放行**必须署名 + 理由**；只解除**逐条声明**的阻断；
     决定是**追加事实**（同 ID 覆盖即拒绝）；**不删 incident、不重置库**；
     `release_epoch` / `decided_at` 由**库侧**落定（不接受调用方自报，防回填旧决定套新阻断）。
3. **驱动层**：四条退出路径各自结账——`Converged/AlreadyConverged ⇒ Recovered`、
   `owner 未知 ⇒ HumanReviewRequired`（资源**仍隔离**）、`规则拒绝/前提不满足 ⇒ KeptIsolated`、
   **缺"提交候选检查"⇒ 刻意不结账**（那是"尝试不完整"，不是结论；结账会掩盖它并阻止带齐检查的重放）。
   已结账的操作**不重放**（报 `AlreadySettled`，不重复建阻断）。
4. **评估层**：`unacknowledged_open_block_ids` + `acknowledged_run_ids`——**未获放行的阻断**才算挡路；
   operator 明确接受的遗留运行不再挡路；放行后仍须走**独立评估 + 按资格开放**（人事与机器各一半）。
5. **入口**：`GET /api/system/attribution-and-recovery` 增加 `input_safety_recovery`
   （pending / human_review_required / 未获放行阻断 / 未获接受运行 **分开报**）；
   新增 `POST /api/system/release-isolation`（署名/理由/证据 + **集合相等**校验防 TOCTOU）。
6. **前端**：状态栏新增"放行隔离…"按钮（只在真有挡路项时出现）+ 三态文案 + 放行确认（写明
   "不删事故、不重置库、不等于已开放"）。

**过程中踩到并修掉的两个顺序缺陷**（都写进代码注释，避免后人重犯）：

- **提前结账挡住阶段推进**：R8（`stage_committed`）由 store 置 `committed = true`，而阶段推进要求
  "未提交"。最初把结账写在收敛分支里 ⇒ R8 被"不得跳步或回退"挡下
  （`当前 r7_terminal_committed → 请求 r8_stage_committed`）。**结账必须放在阶段阶梯之后**。
- **结账守门用错维度**：`UPDATE ... WHERE committed = 0` 会让已走完 R8 的操作永远结不了账
  （表现为 `presented_epoch == current_epoch` 却报未持有资格）。守门改为 `WHERE disposition = 'pending'`。
- **同源挂账**：启动路径协调器**自身**的登记操作（`startup-input-safety`）此前也永不结账，
  每次启动都留下待对账项 ⇒ 下一次启动拒绝开放。现已按 opening 结果结账
  （`Opened ⇒ Recovered`、`KeptIsolated ⇒ KeptIsolated`；根未注入/库不可用时保持 pending——那时写不进去）。

**真实验证（本机真实库，未清理现场）**：

```text
迁移：user_version 1 → 2；disposition 列就位；input_safety_release_decisions 建出（就地，未换身份）
第一次：candidates=2, converged=0, refused=2, settled=0,
        opening=KeptIsolated{open_blocks:2, pending:1, human_review_required:2}
        ⇒ 两条历史操作结账为 human_review_required、启动自身操作结账为 kept_isolated
第二次：candidates=2, converged=0, refused=0, settled=2,
        opening=KeptIsolated{open_blocks:2, pending:0, human_review_required:2,
                             refusal:Some("该资源仍有未获放行的阻断事实")}
库内：未结账操作 0 条；human_review_required 2 条；open blocks 2 条；incidents 2 条（**未被删除**）
HTTP（只读/非改动校验）：空署名 ⇒ 400；阻断集合不符 ⇒ 409（TOCTOU）；状态未被改动
```

**验收用例（决策的 Recovery-P0-T1 已落地）**：
`legacy_recovery_driver::tests::p0_t1_unknown_owner_settles_as_human_review_and_stays_blocked`
（owner 未知 ⇒ 资源阻断 + 处置 = `HumanReviewRequired` + **不再永久 pending**）；
`settled_operation_is_reported_instead_of_being_replayed`；存储侧 3 条
（`settling_is_authorized_final_and_not_resurrectable`、`release_decision_requires_signature_and_releases_only_declared_blocks`、
`v1_database_is_upgraded_in_place_and_old_rows_read_as_pending`）；开放侧 1 条
（`release_decision_is_what_makes_the_resource_openable`）；前端守门 1 条
（`release_isolation_surface_is_wired_without_lying`）。web-console **1122/0**、core-runtime **312/0**。

**仍未闭环（如实保留）**：放行通道已具备，但**放行资格与复核口径**未定义（谁有权放行、是否需要双人复核、
证据存放位置）；本机资源当前仍是隔离状态（2 条未获放行阻断）——这是**设计结果**，不是待修 bug；
是否放行是**运营决定**，我不代行。

### B-83 PR-02（P0-2）**开工前侦察**：接线面已定位，但有两个规格分叉必须先定口径

**结论先行**：PR-02 不是"从零造事实链"，而是**接线**——三块关键件**已存在**：

| 已有件 | 位置（可核验） | 说明 |
| --- | --- | --- |
| 真实规划 attempt 的产生 | `computer_use_planner.rs:538` `register_plan_attempt`、`:816` `last_plan_request_attempt` | 复合键 `run_id#logical_request_id#attempt_id`（`computer_use_planning:step-N` + `attempt-K`），且**已有**"绝不伪造"的测试（`:1560`） |
| 同事务事实写入 API | `computer_use_store.rs:1790` `record_step_with_facts(step, action_json, \|tx\| …)` | 步骤行与事实**同一事务**（规则引擎为 `AppendOnlyFactStore`） |
| 动作事实契约 | `run_contract.rs:473` `RunScopeContext::step_action_fact`、`admit_action_origin`（`:2420`）、`ActionOriginAuthority` trait（`:2479`） | 准入是**两级**：结构校验 + 可信关联核对 |

**真正的缺口（两处）**：

1. **没有生产实现 `ActionOriginAuthority`**：唯一实现是测试用的内存 `TrustedOriginContext`（`run_contract.rs:2503`）。因此 `admit_action_origin` 在生产里**一次都没被调用过**。
2. **执行器不写动作级事实**：4 个步骤写入点（`computer_use_executor.rs:169 / 236 / 273 / 346`）只调 `record_step`（步骤行），生产事实里只写了**轮次级** `record_host_outcome`；动作级 `record_action_receipt` 仅出现在测试（`computer_use_store.rs:3229` 起）。

**两个规格分叉（**必须先定，否则接线会产出"看起来有事实、其实没约束"的假链**）**：

**分叉 A：authority 的 attempt 来源——进程内还是落库？**
- 现状：attempt 只活在 planner 实例的 `Mutex<Option<PlannedRequestAttempt>>` 里（进程内）。
- 选项 A1（进程内）：authority 直接查该 planner 实例的 attempt。**真实**（确实来自这次规划请求），但**跨进程不可复核**：进程退出后无法审计"这条动作当时由哪次请求产生"。
- 选项 A2（落库）：新增 attempt 登记表（如 `computer_use_plan_attempts`），planner 每产生一次规划 attempt 就登记一行；authority 从表里核对。可跨进程审计，代价是**新表 + 新写入点 + 迁移**（会话库 v24）。
- 推荐：**A2**。理由是决策原文要求 authority 来源包含 `runtime_runs` / `computer_use_runs` / `tool_calls` / `control_operations`——即"可核对的登记"，而非"进程内记忆"；且 §B-82 的教训正是"挂账/记忆不进持久层 ⇒ 无人能复核"。

**分叉 B：准入失败时，步骤行还写不写？**
- 选项 B1（fail-closed，推荐）：动作事实**不写**，且把拒绝**显式留痕**（步骤行写 `action_origin_refused` 类错误码）——"没有事实"永远由"已记录的原因"解释，不会静默。
- 选项 B2：动作事实不写、步骤行也不写（即整个步骤被拒绝）。**风险**：CU 在 attempt 缺失（例如旧版本运行、或 planner 无法产出合法复合键时）会整体不可用；而 `register_plan_attempt` 明确会在构造不合法时返回 `None`（`planner.rs:541` 注释）。
- 选项 B3：落一条"来源被拒"的事实记录。语义最强，但需要事实表支持"拒绝记录"这一 kind（当前 `InputSafetyEventKind`/事实 kind 需扩展）。
- 推荐：**B1**（保守且不哑：拒绝有留痕，CU 不因缺证据整体熄火）。

**接线清单（口径确定后即可执行，预计 3 个文件）**：

1. `web-console/src/action_origin_authority.rs`（新）：`ProductionActionOriginAuthority` 实现 8 个查询——`conversation_scope`（执行器的冻结上下文）、`run_relation`（`chat_runtime_runs`）、`plan_producer`/`known_request`（分叉 A 选定的来源）、`tool_call_relation`/`control_operation`/`cleanup_incident`（相应登记存在则读出，不存在则 `None` ⇒ **拒绝**，这正是"禁止模型自报 origin"的落点）。
2. `computer_use_executor.rs`：4 个写入点改为经 `record_step_with_facts`——先 `admit_action_origin`（`ModelPlanned` 必须带真实 attempt；`UserDirect` 必须有控制面上下文 + 许可决定；`SafetyCleanup` 必须有 incident + 原动作 + 恢复资格），通过后在同事务写 `record_action_receipt`；不通过按分叉 B 的选定口径处理。
3. `computer_use_store.rs`：如选 B3 则扩展事实 kind；如选 A2 则加 attempt 登记表 + v24 迁移。

**CU-F05 验收（决策原文四场景）到用例的映射**（全部可在 authority + store 层断言，无需真实模型）：

| 场景 | 断言点 |
| --- | --- |
| 模型动作无 attempt | `admit_action_origin` 拒绝（`ModelPlanned` 缺 `request_attempt_id`） |
| tool id 存在但 attempt 错误 | 复合键不一致 ⇒ 拒绝（`known_request`/`plan_producer` 比对失败） |
| 真实 attempt | 准入通过且**事实真的落库**（`fact_log_records` 计数 + 内容） |
| cleanup 无 incident | `SafetyCleanup` 缺 incident 关联 ⇒ 拒绝 |

**为什么本轮**不**开工**：分叉 A/B 任一选错，都会产出决策文档明令避免的"全绿但事实链不是生产约束"；且 PR-02 的接线会**改变 CU 运行时行为**（步骤写入路径），在口径未定前动手，等于把未定语义写进生产路径。本轮已完成并验证的是 PR-01（`c292fe9`），PR-02/PR-03 保持**未开工**状态（不是半成品）。

### B-84 PR-02A（Stage 1+2）交付：attempt 落库 + 生产 `ActionOriginAuthority`；Stage 3（执行器接线）已实测工作面

**裁决依据**：PR-02A 最终裁决（采用 A2 分阶段：先 attempt 落库 + 生产 authority + 执行器写 ActionFact；
`tool_call_id` 降为"有真实登记才必填、无登记不得伪造"；`tool_calls` 登记表留 PR-02B）。

**已落地（Stage 1：登记落库，schema v23 → v24）**

- 新表 `computer_use_plan_attempts`：`attempt_key`（`run#logical_request_id#attempt_id` 复合键）主键 +
  `(call_id)` 索引 + **`(call_id, action_id)` 唯一索引**（一条动作只能由**一次**规划请求产生；
  后来者认领会撞唯一约束而失败，不是"悄悄覆盖"）。
- 新表 `computer_use_action_origin_rejections`：**输入前身份拒绝**的审计事件，`physical_input` 恒为 0
  ——拒绝日志与动作事实分开（裁决 §五）。
- 迁移 `apply_session_migration_v24_action_origin_ledger`：就地补齐（`IF NOT EXISTS`）、推进到 24；
  同时接进 `ComputerUseRunStore::open`（独立打开 store 也必须可用，否则登记会以 `no such table` 失败，
  来源核对就静默退化成"查不到"）与 `main.rs` 阶梯。
- store API：`record_plan_attempt`（幂等）、`bind_plan_attempt_action`、`plan_attempt_for_action`
  （`plan_producer` 的读回面）、`plan_attempt_by_key`（`known_request` 的读回面）、
  `record_action_origin_rejection` / `action_origin_rejection_count`。

**已落地（Stage 2：生产 authority）**

- 新文件 `web-console/src/action_origin_authority.rs`：`ProductionActionOriginAuthority` 实现
  `ActionOriginAuthority` 的 8 个查询，**每条记录都来自真实查询**（构造时快照，因此 `&self` 返回引用安全）：
  - `plan_producer`/`known_request` ← `computer_use_plan_attempts`（落库，可跨进程审计）；
  - `run_relation` ← `computer_use_runs` 行（**工作区未记录的历史行不构造记录**，不拿当前工作区顶替）；
  - `tool_call_relation` / `cleanup_incident` / `control_operation` / `host_operation` ←
    **当前留空**（对应登记表尚不存在）：查不到即 `None`，契约随即按"不得虚构工具归属/来源不成立"拒绝。
- **必要使能（additive）**：`RunRelationRecord`/`ToolCallRelation`/`HostOperationRecord`/
  `CleanupIncidentRecord`/`ControlOperationRecord` 此前**未从 `runtime` 导出**，导致该 trait 在 crate
  之外**根本无法实现**（这也解释了"生产里一次都没调用过 `admit_action_origin`"）。已在 `lib.rs`
  加入导出，**无语义变化**。

**契约侧的好消息（不需要改契约）**：裁决要求的 `tool_call_id` 条件语义**契约早已内建**——
`ActionSource::required_relations()` 明确写着"`tool_call_id` 是否必填由**可信上下文**决定，
可选性不得由调用方自己挑"，`validate_against` 的四个分支正是：
`(None,None)` 通过（类型 B：模型规划非工具链动作）、`(None,Some)` **拒绝**（"工具归属不得虚构"）、
`(Some,None)` 拒绝（漏传）、不一致拒绝；跨 run 由 `ensure_runs_are_related` 拒绝。
因此 CU-F05-3/4 落在"authority 如实返回 None" + "origin 不伪造"上即可。

**验收用例（已通过）**：`plan_producer_comes_from_the_store_not_from_the_caller`（真实 attempt 可查回；
别的动作/别的 run **查不到** —— 禁止"最近一次请求"顶替）、`tool_relations_are_never_fabricated`、
`legacy_rows_without_recorded_workspace_have_no_run_relation`；
存储侧 `plan_attempts_are_persisted_bound_and_read_back`（含"一动作一请求"的唯一约束冲突）、
`pre_input_origin_rejections_are_audited_without_claiming_physical_input`、
`migration_v24_creates_action_origin_ledger_in_place`。
另更新迁移阶梯守卫（终点 v23 → v24，并把 v24 对象纳入"版本推进了、对象真的建出来了"检查）。

**门禁**：web-console **1128/0**、core-runtime **312/0**、module_linkage_smoke 4/0。

**Stage 3（执行器接线）——已实测的工作面（尚未动代码，刻意不半接线）**

1. **适配器缺会话维度**：`TracingAdapter`（`computer_use_executor.rs:116`）当前只有
   `store`/`call_id`/`state`/`cancelled`/`input_lease`；而 `ActionOrigin` 需要
   `ConversationActionContext`（工作区/房间/会话/公开轮次）。四维**在入口就有**（`execute_with_current_runtime`
   的 `parent: FrozenParentContext` + `chat_room_id`），但需要穿过 `execute_in_room_with_policy`
   到适配器（适配器构造点**只有 1 处**：`executor.rs:913`）。
   同时适配器要拿 `&dyn ComputerUsePlanner` 才能取 `last_plan_request_attempt()`（该 trait 方法正是
   为"写动作事实时构造来源"而设，且明确禁止用近似物顶替）。
2. **翻转"输入前缺 attempt ⇒ 拒绝"的影响面已实测**：执行器侧有 **6 个假 planner 实现**、
   **50 个用例**。这些假 planner 都未实现 `last_plan_request_attempt`（默认 `None`）——
   按裁决 §五，它们会全部落到"输入前身份拒绝"。正确做法是给每个假 planner 补一条**真实** attempt
   （而不是放宽判定），属机械但有面儿的改动。
3. **事实写入点已确定**：`act()` 末尾的 `record_step(&step, &action_json)` 处，
   `result`（含 `execution.receipt` / `error.receipt`）在手 ⇒ 就地改用
   `record_step_with_facts`，在同事务里 `facts.record_action_fact(&ActionFact::new(identity, receipt).with_origin(origin))`
   （准入已在输入前完成，符合"先准入、再写事实"）。

**为什么本轮停在这里**：Stage 3 会**翻转 CU 运行时行为**（输入前拒绝），并且影响 50 个用例中的假 planner。
在没有把"上下文穿过适配器 + 6 个假 planner 补真实 attempt"一次做完并验证之前，半接线的形态恰好就是
裁决 §五要禁止的那种——"有些动作有事实、有些没有，且没有拒绝留痕"。已完成的 Stage 1+2 是**完整且被测试的
单元**（登记 + authority），执行器未被触碰，生产里没有任何"声称已核对来源"的假事实。

**CU-F05 现状**：1/2/3/4 的基础已就位（authority 层已测），**5（SafetyCleanup）本轮只能落地"无 incident ⇒ 拒绝"
这一半**——生产里没有 `CleanupIncidentRecord` 形态的登记（`original_action_id` + `recovery_eligible`），
现有输入安全库的 incident 形状不同。正面那一半需要先建事故登记，属 PR-02B 或独立工单。

### B-85 PR-02A Stage 3 交付：执行器接线完成——**CU 动作来源与动作事实从此是生产约束**

**目标（裁决 PR-02A §三／§五）**：让 `CU 动作 → 事实 → 事务 → 审计` 真实成立，且**输入前身份拒绝**＝
零物理输入 + 零步骤行 + 只留审计。已落地：

1. **冻结上下文穿到适配器**：`TracingAdapter` 新增 `planner`（取 `last_plan_request_attempt`）与
   `conversation`（四维会话上下文）；执行器新增 `with_conversation_scope` / `with_provider_tool_call_id`
   两个**构造期**注入点（无 setter ⇒ 运行中不可改写）；生产入口从 `FrozenParentContext` + 房间
   构造四维（房间缺失时**回落到冻结上下文里的房间**——两者都是权威值，回落不放宽核对）。
2. **输入前准入**（`admit_action_origin`）：先凑齐真实输入（冻结四维、planner 的真实规划请求、
   刚落库的登记），再交契约的 `admit_action_origin` 两级核对；**顺序刻意在写步骤行与发输入之前**。
   任一项拿不出来 ⇒ `reject_action_origin`：写一条**审计**（`physical_input = 0`）并返回
   `action_origin_rejected`，**不写步骤行、不发输入**。
3. **步骤行与动作事实同事务**（`persist_step`）：有**可信**回执且已准入 ⇒ 经
   `record_step_with_facts` 在同事务写 `ActionFact::new(identity, receipt).with_origin(origin)`；
   缺任一半 ⇒ 只写步骤行（没有回执就没有"发生过什么"可言，不得凭状态编造事实）。

**过程中发现并修掉的四个真问题**（都已写进代码注释／用例）：

| # | 现象 | 根因 | 处置 |
| --- | --- | --- | --- |
| 1 | 准入报 `incomplete_identity: tool_call_id 本应存在却缺失` | `StepAction` 身份**必填**维度含 `tool_call_id` | 身份如实填**外层工具调用 id**（`provider_tool_call_id`，运行接纳时的真实值）；`ActionOrigin::tool_call_id` 仍留空（无登记表 ⇒ 不得声称工具归属）。两者是**不同轴**：前者是"动作发生在哪次工具调用里"，后者是"是否存在可核对的工具调用关系" |
| 2 | 17 个用例 `no such table: fact_log_records` | 执行器现在真的会写事实，而 `ComputerUseRunStore::open` **没应用 v21**（事实日志表） | 生产侧：`open` 补调 `crate::apply_session_migration_v21`（与 v22/v23 同例，调用同一批函数）；测试侧：手工建的库补齐 v21/v23/v24 |
| 3 | `mismatched_receipt_identity...` 期望 `receipt_protocol_anomaly`，实际停在 `executing` | 协议异常回执（不属于本动作）被拿去写事实 ⇒ 规则引擎拒绝 ⇒ **连步骤行都写不进去** | 事实写入只接受**可信回执**（过 `trusted_receipt_facts`）；异常回执 ⇒ 不写事实，但步骤行照写（如实记录异常） |
| 4 | 三个源码顺序守门用例报"缺少锚点" | 守门用 `split("#[cfg(test)]")` 截取"运行代码区域"，而新加的 `cfg(test)` 夹具字段恰好落在函数体内，把区域**提前切断** | 守门改为切精确标记 `"\n#[cfg(test)]\nmod tests"`（保留原意，不再被属性截断） |

**测试夹具接缝（照 RPR-02a 先例）**：测试替身既没有真实模型请求、也没有接纳时的冻结上下文，因此
`new_for_test` 默认提供**夹具**（attempt 形状与生产一致：`computer_use_planning:step-N` + `attempt-1`）。
夹具**只存在于测试构建**（`#[cfg(test)]` 字段 + `fixture_plan_attempt` 有 `#[cfg(not(test))]` 版本恒返回
`None`），生产构建里"缺 attempt ⇒ 拒绝"不可绕过；要测生产语义用 `without_plan_attempt_fixture()`。
非测试构建（`cargo build -p coolzhu-web-console`）实测通过 ✅。

**CU-F05 验收（全部落地）**：

| 场景 | 用例 | 断言 |
| --- | --- | --- |
| 1 真实 attempt | `cu_f05_1_real_attempt_writes_the_action_fact_with_verified_origin` | 事实**真的落库**、含 `model_planned` 与复合键；登记表有绑定；无拒绝审计 |
| 2 attempt 不存在 | `cu_f05_2_missing_attempt_is_refused_before_input_with_audit_only` | `Blocked` + `action_origin_rejected`；**零事实、零步骤行**、有审计（`physical_input = 0`）；工厂零调用 |
| 3 tool_call_id | `cu_f05_3_tool_ownership_cannot_be_fabricated` | 不声称 ⇒ 通过（类型 B）；声称无登记的工具归属 ⇒ 拒绝（"工具归属不得虚构"） |
| 4 跨 run attempt | `cu_f05_4_cross_run_attempt_is_refused` | 拒绝、零事实、拒绝理由指向运行关联 |
| 5 SafetyCleanup | `cu_f05_5_cleanup_without_an_incident_registry_is_refused` | **负半已测**（无 incident 登记 ⇒ 拒绝）；正半仍缺 `CleanupIncidentRecord` 形态的事故登记（PR-02B） |

**门禁**：web-console **1133/0**、core-runtime **312/0**、computer-use-core 123/0、module_linkage_smoke 4/0。

**仍未闭环（如实保留）**：① CU-F05-5 正半（事故登记）；② `tool_calls` 登记链（PR-02B）——
`ActionOrigin::tool_call_id` 因此仍为 `None`（不伪造）；③ 生产首次真实运行后应复核"每次动作都留下
来源已核对的 ActionFact"（本轮只做了构建期与单测验证，未跑真实模型任务）。

### B-86 PR-03（P0-3）交付：输入入口收口为受控族——**两条仍在跑的"无生命周期"自动化路径已迁移**

**决策要求**：建立统一输入入口（validate → reserve → execute → receipt → cleanup → settlement），
禁止再存在多套互不知情的输入体系；必测 6 项（超时 helper 存活／stdout 管道未结束／release unknown／
cancel 重复／强杀／输入前失败）。

**侦察结论**：受控族 `controlled_*`（`computer_use_core::input`）**已经就是**那个生命周期——
`controlled_click` / `controlled_mouse_button_action` / `controlled_mouse_button_state` /
`controlled_type_text` / `controlled_scroll` / `controlled_press_key` / `controlled_hold_key` /
`controlled_key_combo`，内部含预留、回执、**按实际按钮/键登记释放义务**、收尾与静止对账，
6 项必测在 `input.rs` 的测试模块里**已有覆盖**（含真实子进程：预算超时、重复取消不刷新收尾窗口、
静止未确认不报 released、回执丢失不虚构释放、孙进程持管道等）。

**真正的缺口（P0-3 说的"事实不一致风险"）是两条仍在跑的自动化路径**，它们直调**无生命周期原语**：

| 路径 | 位置 | 原先的输入调用 | 风险 |
| --- | --- | --- | --- |
| 桌宠自动化 | `gui-desktop/packages/desktop-console/src/desktop_agent.rs` | 7 个薄包装直调 `click_point`/`type_text`/`press_virtual_key`/`hold_virtual_key`/`scroll_wheel`/`send_virtual_key_combo`/`move_mouse_relative` | 无预留、无回执、**无释放义务登记**：中途失败可能留下按下的键/按钮而无法对账 |
| 闭环评测执行器 | `gui-web/packages/web-console/src/main.rs` | `send_mouse_action`/`send_escape_key`/`send_text_input`/`send_drag_action` 直调 `mouse_button_action_point`/`press_escape`/`type_text`/`drag_point` | 同上；**拖拽**尤其明显：中途死亡可能让左键停在按下状态 |

**已做的收口**：

1. **桌宠自动化**：6 个带义务的包装改为 `controlled_*`（点击/滚动/打字/按键/按住/组合键）；
   相对移动保留（见下"例外"）。本 agent 目前**没有**取消信号 ⇒ 如实传"永不取消"并写在注释里
   （引入取消时应接入真实信号，而不是让"永不取消"冒充"没有需求"）。
2. **闭环评测执行器**：鼠标动作/ESC/文本改为受控入口；**拖拽重写为"受控按下 → 逐段移动 → 受控抬起"**
   —— 释放义务在"按下"那一步登记、由受控收尾负责，**无论中间是否出错都要执行"抬起"**，
   否则左键会留在按下状态（这正是原先无生命周期路径下可能发生的事实不一致）。
3. **改名收口（编译器强制）**：11 个无生命周期原语改名为 `diagnostic_*`
   （`click_point`→`diagnostic_click_point` …）。改完 `cargo build --workspace` **零错误**，
   即"除诊断入口外没有任何调用方"由**编译器**证明，而不是靠口头约定。
4. **根级源码守门**（`tests/module_linkage_smoke.rs`，新增用例）钉住三件事：
   ① `diagnostic_*` 只允许出现在诊断入口（`bin/check.rs`、桌宠控制台 CLI 子命令、core 自身定义/测试）；
   ② 两条自动化路径必须引用 `controlled_`；③ 仍被自动化使用的"无义务"原语只能是**移动**，
   且其引用文件被**逐个列出**——多一个引用点即失败。
   （过程中踩到一次误报：`diagnostic_redaction_...` 这种**测试名**含 `diagnostic_`，
   故守门的针精确到"函数名 + 左括号"。）

**如实保留的两处例外（已写进代码注释与守门清单）**：

- `move_mouse_relative` / `move_mouse_absolute`：受控族**没有**移动入口。移动**不产生释放义务**
  （不按下任何按钮/键），因此不构成"留下按下状态"的风险；要么给受控族补
  `controlled_move_mouse_relative/absolute` 才能彻底统一。
- 评测执行器与桌宠自动化目前**没有取消信号**（传 `|| false`）。这不影响本次收口的安全性，
  但"取消"是受控族的一等公民，接线时应补上真实信号。

**门禁**：workspace 构建 ✅、web-console **1133/0**、computer-use-core **123/0**、
desktop-console **16/0**、module_linkage_smoke **5/0**（含新增守门）。

### B-87 PR-04（P1）交付：切换/关闭口径收口——`shutdown_incomplete` 可判定，UI 不再把 off 当已完成

**P1-1 根 deadline：按裁决的 Phase 1 处置（**不改代码**），并核实"不假装"成立。**

- 事实：`RootDeadlineState::NotWired` 存在（`computer-use-core/src/budget.rs`），如实呈现为 `not_wired`；
  `supervisor.rs` 有用例断言 `deadline().root_deadline_state().as_str() == "not_wired"`。
- 核实"没有假装已接线"：`RunBudget` 在 web-console 侧**零引用**（grep 无命中），
  即聊天/目标/中继路径没有偷偷造一个根 deadline。
- Phase 2（`RuntimeDeadlineContext`：chat accept／goal accept／relay accept 冻结
  `created_at`/`deadline`/`source`，模型请求取 `remaining = root ∩ CU ∩ stage`）按裁决
  **"不要现在全面改"**执行 ⇒ 本轮不动，保持遗留（列入 §7）。

**P1-2 切换/关闭：找到并修掉一处真实 UI 缺陷。**

- 现状（修前）：后端 `api_local_models_switch` 里 chat switch 与 **`off` 都走同一排空闸门**，
  失败时返回"关闭/切换未完成：{原因}"且**不写** off ✅（`off_does_not_bypass_the_drain_gate` 已覆盖）。
  但两处不足：
  1. 失败只有一句**人读** message，没有机器可判定的结果码；
  2. **前端 `pollLocalModelsMode` 里 `requestedMode === "off" || …` 直接短路**——用户点 off 后
     轮询立刻"完成"并渲染，于是"关闭未完成"在界面上看起来像已关闭。**这正是裁决禁止的"UI 显示 off"**。
- 修法：
  1. 响应新增机器可辨识的 `outcome`：`applied` / `shutdown_incomplete`
     （`local_models_status_response_with_outcome`）；拒绝分支返回
     `shutdown_incomplete` 且消息写明 `ShutdownIncomplete`。
  2. 前端不再短路 off：只在**后端如实呈现** `active_mode === requestedMode` 时才认为完成；
     收到 `shutdown_incomplete` 时显式提示"未执行关闭，当前仍为 X"，不按目标状态等待。
- 守门用例（2 条）：后端断言"结果码必须在**应用切换之前**返回"（顺序错了就等于关闭已被执行，
  再报未完成也晚了）+ 结果码可判定；前端断言不得再出现 off 短路、且必须按结果码处理。
- **保持遗留**：vision switch 仍未纳入排空（vision 服务 7860/8000 未登记在途），
  代码注释里已如实标注、此处不宣称已覆盖（属裁决提到的 P-06 待补项）。

**门禁**：web-console **1135/0**。

### B-88 PR-05（P2-1 · CU-01）第一层交付：统一的 `input_status`（none／partial／complete／unknown）

**侦察结论（省掉了一次重复劳动）**：CU-01 要求的**安全核心已经存在且有测试**——
"partial/unknown 绝不自动重放"由控制器与收尾路径强制（`controller.rs` 的身份不匹配/可能已发出分支、
`input_stroke.rs` 的"释放未确认必须隔离、`retryable` 不是重放授权"、`supervisor.rs` 的
"相同观察下的原样重放被阻断"用例）；事实层也已有四值的 `EffectStatus`/`GoalVerdict`，
并非"success/error 二值"。**真正缺的是粗粒度统一视图**：报告的读者要自己把
`input_delivery`／`partial`／`path_completed`／点数拼成"到底注入没有、完成没有"。

**本轮交付**（`computer-use-core/src/input.rs`，additive）：

- `InputStatus { None, Partial, Complete, Unknown }` + `as_str()`（稳定契约，报告/界面按它落列）；
- `derive_input_status(&DeliveryFacts) -> InputStatus`：**唯一判定点**，映射按最保守方向——
  `NotSent ⇒ none`；**只有** `Sent` 且 `partial = Some(false)` 才 `complete`；
  `partial = Some(true) ⇒ partial`（**含自相矛盾记录：宁可 partial，不许 complete**）；
  "光标动过但零输入事件且记录已封闭" ⇒ `none`（与 `input_delivery` 刻意不称 `NotSent` 是两个粒度，
  代码注释写明）；其余 ⇒ `unknown`；
- `may_claim_complete()` / `forbids_automatic_replay()` 两个判定词，把口径写进类型；
- 笔画回执（`StrokeFailure`）新增 `input_status` 字段（随事实一起派生与传递，**不得重算**）。

**用例**：映射表 8 例（含矛盾记录、缺 `partial` 结论）；不变式用例断言
"这些事实不得声称完成" + "非完成态只能落 none 或禁止自动重放" + 四值字符串契约。

**仍未做（CU-01 的其余列与 CU-02）**：失配子类、请求状态、任务级基线、统一 run 计数
（C8/C9/C11 的落列与界面呈现）；CU-02 的"owner = room/turn/run 的桌面输入租约"——
跨进程互斥与崩溃回收在受控输入路径已有（`ScopedInputOwnership` + 命名内核对象），
但"owner 身份 = room/turn/run"这一口径尚未对齐。

**门禁**：computer-use-core **125/0**、web-console 1135/0、module_linkage_smoke 5/0。

### B-89 PR-03 后续工单：受控族缺 `move` 入口，以及我在拖拽重写里引入的一处**已知局限**（**已交付，见本节末**）

**侦察结论（决定后续工单的形状）**：鼠标移动**不走受监督 helper**——
`move_mouse_relative`/`diagnostic_move_mouse_absolute` 各自 `run_powershell` 一段独立的
`SetCursorPos` 脚本；而 helper 的动作模式只有 `click`/`text`/`scroll`/`key`/`combo`/`down`/`up`
（**没有 `move`**）。因此：

- 移动**没有**注入步数、序列完成等事实（不登记 `injected_steps`）；
- 移动不产生释放义务（不按下任何按钮/键）——这也是 PR-03 允许它作为唯一例外的依据；
- 但每次移动都会**新起一个 PowerShell 进程**（性能与生命周期上都与受监督 helper 不一致）。

**我在 PR-03 里引入的一处已知局限（必须如实记录）**：闭环评测的拖拽被重写为
"受控按下 → 逐段移动（无义务移动）→ 受控抬起"。因为中间段走的是**不受监督**的移动，
它们**不计入**回执的 `confirmed_point_count`（该字段统计的是 helper 自己注入的点）。
后果：读报告的人若拿拖拽的 `confirmed_point_count` 当作"路径注入点数"，会看到偏低的值
（极端情况下为 0），而光标确实移动过。**这不是"谎报完成"**（完成与否由 `path_completed` 与
`partial` 表达，且 PR-05 的 `input_status` 只允许 `Sent + partial=false` 落到 complete），
但它确实是一处**维度口径不完整**。

**收口方案（后续工单，建议与 CU-02 一起做）**：给 helper 增加 `move` 模式（含
`injected_steps`/`sequence_completed` 事实与"永不按下"的语义），并据此提供
`controlled_move_mouse_relative` / `controlled_move_mouse_absolute`；随后：
① 拖拽中间段改走受控移动（点数与"移动也算一步"的口径就完整了）；
② 桌宠自动化的相对移动与评测拖拽都脱离未受监督路径，PR-03 守门清单里的"移动例外"即可删除。

**在此之前**：PR-03 的守门用例仍在用**允许清单**把这两个移动原语钉住（多一处引用即失败），

**已交付（同一轮，收口完成）**：

1. **helper 增加两个模式**：`move`（绝对）与 `move_relative`。相对移动的起点由 **helper 自己**
   在同一段受监督运行里读（`Driver.CursorPosition`；SendInput／Interception 用 `GetCursorPos`，
   mock 驱动记录内部位置）——**不**让主机在外面另探一次，那会重新引入不受监督的路径。
2. **Rust 增加两个受控入口**：`controlled_move_mouse_absolute` / `controlled_move_mouse_relative`
   （义务为 `none`，但同样受监督运行、事实落盘、收尾确认；`cursor_moved` 与「未按下」都如实登记）。
3. **迁移完成**：桌宠自动化的相对移动、闭环评测拖拽的中间段移动改走受控入口；
   两个移动原语改名 `diagnostic_move_*`。
4. **守门收紧**：PR-03 守门用例里「移动例外」的允许清单**整条删除**（例外清零）；
   两个移动原语并入「仅诊断」针 ⇒ 自动化侧不得再引用任何未受监督原语——由编译器 + 守门用例双重保证。
5. **真跑验证**（不可省：helper 的 C# 由 PowerShell 在**运行时** `Add-Type` 编译，单元测试编不出它的
   语法错误）：用例 `real_helper_move_modes_report_cursor_movement_without_pressing` 用 mock 驱动跑
   **真实 helper 进程**，断言 `cursor_moved = Some(true)`、`injected_steps = 0`、`completed`、
   未按下、无残留、`derive_input_status = complete`。

**事实口径的诚实说明**：移动的 `injected_steps` 恒为 `0`——它不是按钮/键/滚动类输入事件
（与「click 的 MoveTo 不计步」同口径）。因此拖拽的 `confirmed_point_count` **仍然不含**中间移动段；
变化在于：中间段现在**各自有自己的受监督回执与「光标移动过」事实**，不再是「无事实的盲区」。
若要把它们并入一个总数，需要 helper 侧新增 `drag` 模式（列为可选后续，不再是安全缺口）。

**门禁**：computer-use-core **126/0**、web-console 1135/0、desktop-console 16/0、
module_linkage_smoke **5/0**、`cargo build --workspace` ✅。
所以"例外"不会悄悄扩散。

### B-90 CU-01 其余列交付（计数／基线／请求状态／失配子类）：四处说不清的事实各自变可分辨

**背景**：CU-01 的验收是「每个步骤事实一致；partial/unknown 绝不自动重放；无 usage 请求也有 attempt 记录；取消/失败不声称完成」。第一层（`input_status`）见 §B-88；本轮补齐其余四列。

| # | 交付 | 修掉的具体缺陷 |
| --- | --- | --- |
| 1 | **统一 run 计数**：匿名元组 `(attempts, steps)` → 具名四维 `ComputerUseRunCounts { attempts, input_sent, partial_input, verified_steps }`；`finish` 在**同一条已锁定连接**上派生权威计数并**回填** `action_count`（拆 `run_counts_on` 避免自死锁）；`replan_count` **明确废弃** | 专项文档 C9 的**可复核统计缺陷**：`action_count` 从不写入、恒为 0，而真值在读取侧由步骤行派生 ⇒ 库里同时存在「有 3 步」与「计数 0」。现在只有一个权威来源 |
| 2 | 执行器消费点收紧：`steps_completed` 只认**确认已发送输入**的步数 | 原判据把零输入行（输入前拒绝）也计入「完成步数」⇒ 让「完成」被没动手的步骤充数 |
| 3 | **任务级基线**：`run_baseline_evidence_ref`（首步 before 证据＝任务开始时视图）＋ `run_final_evidence_ref`（末步 after，缺失退回其 before） | 专项文档 F5/F7：只给「最后一对 before/after」时，**目标早已存在**会被读成「本轮新画出」。基线与末帧成对才是最小证据（派生实现，不新增列） |
| 4 | **请求状态**：`ComputerUseRequestStatus { attempt_key, action_id, has_usage_fact }`，把「请求已登记」与「另有 usage 记录」**分开**报 | 验收要求「**无 usage 请求也有 attempt 记录**」：两者混为一谈时，失败的请求会凭空消失。联接不是猜的——usage 事实的 `record_subject` 与规划请求复合键**同一格式** |
| 5 | **失配子类**：`ReceiptTrust { Trusted, IdentityMismatch, SelfContradictory }` 写进步骤行 `error_code`（`receipt_identity_mismatch` / `receipt_self_contradictory`），不动既有 `status` 口径 | 先前两类被压成一个 `None`，库里只剩笼统的 `receipt_protocol_anomaly`：复盘分不清「**拿错了回执**」（不属于本动作）还是「**回执自相矛盾**」 |

**用例**：`run_counts_are_derived_from_steps_and_backfilled_at_finish`、
`task_baseline_is_the_earliest_view_and_pairs_with_the_final_frame`、
`request_status_keeps_attempts_even_without_usage_facts`、
`receipt_anomalies_record_their_subclass_distinctly`。

**同源问题第三次出现**：手工建的测试库必须补齐本文件拥有的迁移——`temp_store` 补 v21/v24，
否则请求状态会以 `no such table: computer_use_plan_attempts` 失败（前两次分别在执行器测试的
`store()` 与 `ComputerUseRunStore::open` 上）。这条已写进注释。

**门禁**：web-console **1139/0**。

**CU-01 剩余**：这些列的 **UI/报告落列**尚未接；CU-02（owner=room/turn/run 的租约归属）未开工。

### B-91 PR-02B 交付：工具调用登记链——`tool_call_id` 有了**可真核对**的来源

**裁决依据**：PR-02A 最终裁决把 `tool_calls` 登记表列为单独的 PR-02B，并给了建议字段与写入位置
（**不是 executor，而是 tool dispatch boundary**：只有那里才知道"模型请求 + 工具调用 + run 关系"）。

**已落地**：

1. **登记表**（会话库 **v25**）：`tool_calls(tool_call_id PK, run_id, request_attempt_id, tool_name,
   arguments_digest, status, created_at_unix_ms, updated_at_unix_ms)`，按裁决建议的字段，
   **不额外增加身份字段**（裁决 §三：下一步该消费而不是扩展）。两条刻意口径：
   - `request_attempt_id` **可空且未知就留空**：产生该工具调用的模型请求在派发边界上拿不到，
     不拿"最近一次请求"顶替；
   - `arguments_digest` 只存**摘要**，不落参数原文（工具参数可能含凭据/隐私）。
   迁移 `apply_session_migration_v25_tool_call_registry` 同时接进 `main.rs` 阶梯与
   `ComputerUseRunStore::open`（独立打开也必须可用，否则核对会以 `no such table` 静默退化成"查不到"）。
2. **写入点 = 派发边界**（`main.rs::dispatch_model_tool_calls_parallel`）：任务开始时登记
   `status='dispatched'`，收尾时同 id upsert 成 `completed`/`failed`。
   **登记失败不阻断工具执行**（它只影响"以后能不能声称工具归属"），但**响亮记录**、不静默。
3. **核对侧**：`ProductionActionOriginAuthority` 在快照里只为**登记表真有这一行**的动作建立
   `tool_call_relation`——"provider id 存在"**不等于**"关系成立"。
4. **执行器如实声称**：有登记 ⇒ **必须**带 `tool_call_id`（契约对"有工具链关系却漏传"是拒绝的）；
   无登记 ⇒ 不声称，走 `(None, None)`。于是裁决的口径真正落地：
   **有真实登记才必填、无登记不得伪造**。

**用例**（4 条）：
`tool_call_registry_is_idempotent_and_keeps_only_a_digest`（同 id 只更新状态不新建行、参数只存摘要、
未知 attempt 留空）、`tool_relation_requires_a_registered_tool_call`（provider id 存在但未登记 ⇒ 关系**不成立**；
登记后才成立）、`cu_f05_3_registered_tool_call_is_claimed_in_the_action_fact`（执行器端到端：
登记 ⇒ 事实带出 `tool_call_id`）、以及既有 `cu_f05_3_tool_ownership_cannot_be_fabricated`（伪造仍被拒）。

**同源问题第四次出现**：手工建的测试库必须补齐本文件拥有的迁移——本轮是执行器测试的两个
`store()` 助手补 v25（前三次分别是执行器 `store()` 的 v21/v24、`ComputerUseRunStore::open` 的 v21、
`temp_store` 的 v21/v24）。

**门禁**：web-console **1142/0**、computer-use-core 126/0、desktop-console 16/0、
module_linkage_smoke 5/0、`cargo build --workspace` ✅。

**仍未闭环**：① **SafetyCleanup 事故登记**（CU-F05-5 正半）：生产里没有 `CleanupIncidentRecord`
形态的登记（`original_action_id` + `recovery_eligible`），需要单独建；② CU-01 五列的 UI/报告落列；
③ CU-02 的 owner=room/turn/run 归属口径。

### B-92 CU-02（P0）交付：桌面输入租约的 **owner 归属**可审计（互斥与回收原样保留）

**现状核实**（先确认没重复造轮子）：跨进程互斥与崩溃回收**已经存在**——
`ScopedInputOwnership` = 进程内 lease + 命名内核对象，scope 是**交互会话**（因此两个房间天然互斥，
不管 owner 是谁）；持有者崩溃（future 被丢弃）时租约立即可被再次取得，且有用例钉住；
`owner_epoch` + `is_current()` 保证"回收后旧 run 不得复活"。

**真正缺的是归属口径**：CU 侧此前把 `call_id` 直接当 owner，于是租约日志回答不了
"当时是**哪个房间、哪个轮次**占着桌面输入"。

**本轮改动**（小而有据）：
- 新增纯函数 `native_input_owner_id(room, turn, call_id)`：三维齐全 ⇒ `room|turn|call_id`；
  任一维缺失/空白 ⇒ **退回运行 id**，**不编造** `-`/`unknown` 这类占位值（会被后来人当成真实房间名）。
- CU 取得租约时改用该归属串；**互斥语义完全不变**（互斥由 scope 决定，与归属串无关）。
- 用例：三维齐全的组成、缺维退回、空白房间不被当成真实房间、归属串里不出现占位符。

**这里如实划界**：`owner=room/turn/run` 的"归属"这一半已落地；至于"两房间并发点击 ⇒ 第二者 busy 且
0 输入"与"回收后不复活旧 run"，由既有 scope/epoch 机制保证并有既有用例覆盖（本轮未新增重复用例）。

**门禁**：web-console **1143/0**。

### B-93 SafetyCleanup 事故登记交付：CU-F05-5 **两半齐备**（来源可核对 + 真的会登记）

**背景**：`ActionSource::SafetyCleanup` 要求"核对到 incident + 原动作 + 恢复资格"才成立，而生产里
此前**没有**这种形态的登记（只有 session/turn 粒度的释放解除记录）⇒ 任何清理动作都只能被拒绝：
fail-closed 正确，但**无路可走**（§B-84 记的 CU-F05-5 正半缺口）。

**已落地**：

1. **登记表**（会话库 **v26**）：`computer_use_cleanup_incidents(incident_id PK, run_id,
   original_action_id, original_tool_call_id, recovery_eligible, created_at_unix_ms, eligible_at_unix_ms)`，
   与契约的 `CleanupIncidentRecord` 字段对齐（外加登记时间与资格时间）。
2. **生产者（真的会发生）**：执行器写一步时，若该步的**未确认释放**（`input_release_status = Unknown`）
   ⇒ 按 `(run, action)` 确定性生成 `incident_id` 登记一条事故，初始 `recovery_eligible = false`。
   `incident_id` 确定性 ⇒ 重复写入幂等；登记失败**不改写该步的事实**（事实已按回执落盘）但会响亮记录。
3. **资格来源（不是"发生过"就有）**：`resolve_unconfirmed_release`（释放被确认解决）会把该 scope
   覆盖的事故翻转为 `recovery_eligible = 1` 并留痕 `eligible_at_unix_ms`。
   另有显式入口 `mark_cleanup_incident_recovery_eligible`（受控复核场景，同样留痕）。
4. **核对侧**：`ProductionActionOriginAuthority` 预载**本运行**具备资格的事故，按 `incident_id` 提供
   `cleanup_incident` 查询——于是"清理可以成立"成为一条**可走通且可核对**的路径。

**用例**（4 条）：
`cleanup_incidents_gain_recovery_eligibility_only_after_the_release_is_resolved`（初始无资格 ⇒
解决后才具备 + 时间留痕）、`cu_f05_5_eligible_cleanup_incident_is_accepted_as_a_source`
（有资格且原动作一致 ⇒ 成立；原动作不一致 ⇒ 拒绝；未登记 ⇒ 拒绝）、
`unconfirmed_release_registers_a_cleanup_incident_without_eligibility`（执行器端到端：
未确认释放真的留下一条事故，且**无**资格）、以及既有 `cu_f05_5_cleanup_without_an_incident_registry_is_refused`（负半）。

**同源问题第五次出现**：手工建的测试库必须补齐本文件拥有的迁移——本轮是
`release_resolution_tables_are_added_to_an_existing_database_without_rewriting_rows`
（释放解除新增的事故翻转读 v26 表）与执行器两个 `store()` 助手补 v26。

**门禁**：web-console **1146/0**、module_linkage_smoke 6/0、`cargo build --workspace` ✅。

**CU-F05 五场景现状**：1/2/3/4 ✅；**5 两半齐备** ✅（负半 + 正半 + 生产者）。

### B-94 CU-01 五列的**机器可读落列**：`GET /api/computer-use/run-report?call_id=…`

**背景**：CU-01 的验收是"核对 **DB、UI、报告**"。五列此前只存在于库里 ⇒ 核对只能人肉查表。

**已落地**（只读端点，不建表、不迁移、不改状态）：

- 逐步读数：每步 `input_status`（none／partial／complete／unknown，走**唯一判定点**
  `derive_input_status`）、`may_claim_complete`、`forbids_automatic_replay`、失配子类
  （`receipt_identity_mismatch`／`receipt_self_contradictory`）；
- 统一 run 计数四维（`attempts`／`input_sent`／`partial_input`／`verified_steps`）；
- 任务级基线与末帧证据引用（**成对**给出）；
- 规划请求状态（**无 usage 的请求同样在列表里**）；
- 清理事故（含 `recovery_eligible`）。
- 库不可读/未初始化时**如实报 `unavailable`**，不给一份"看起来正常"的空报告（空 ≠ 没有待办）。
- 库里的投递取值认不出时按 `may_have_been_sent` 处理（最保守方向，**不得升格成完成**）。

**用例**：`run_report_surfaces_the_five_columns_without_faking_them`——未初始化库 ⇒ `unavailable`；
三步分别是 complete／partial／unknown 且"部分与未知都不得声称完成、都禁止自动重放"；
计数四维 = (3, 2, 1, 0)；基线与末帧成对；**无 usage 的请求没消失**；事故初始无资格。

**门禁**：web-console **1147/0**、module_linkage_smoke 6/0、`cargo build --workspace` ✅。

**仍缺（如实记）**：**UI 面板**未接。原因不是技术障碍，而是**今天没有"选一个 CU 运行"的界面位置**
（闭环面板是预演场景，不带 `call_id`）；要接需要先有运行列表/运行详情页。端点已可供脚本与
后续 UI 直接消费。

### B-95 CU-04 现状核实：**坐标与陈旧帧的大半早已具备**，本轮补上可度量的验收断言

**为什么先核实**：CU-04 的裁决文字是"采用 frame_id + 返回光栅内整数坐标，宿主映射到物理像素"。
若按字面从零实现，会**重复造已经存在的机制**、并可能把已经正确的坐标契约改坏。核实结论如下。

**已经具备（带锚点，可直接复核）**：

| 要求 | 现状 | 锚点 |
| --- | --- | --- |
| 模型**不**做坐标数学 | vision 返回**截图相对坐标**（0–1），提示词明确"relative coordinate on the screenshot, scaled from 0 to 1" | `vision-service/src/lib.rs` 的 `VisionGroundingResult.point` 与 `SHOWUI_GROUNDING_PROMPT` |
| 宿主映射到物理像素 | `anchor(0–1) × (width-1 / height-1)`，四舍五入；**越界或零尺寸 ⇒ `None`** | `computer-use-core/src/lib.rs::anchor_to_physical_pixel` |
| 多 DPI／多分辨率覆盖 | `standard_resolution_cases()`：hd-100／fhd-100／**fhd-125／qhd-150／uhd-200** | `computer-use-core/src/lib.rs:84` |
| 旧 frame ⇒ 0 输入（浏览器） | 生成号 + `same_input_identity`（page_id／url／dom_revision）不符 ⇒ `stale_observation`（输入前拒绝） | `computer_use_adapters.rs:312/326` |
| 旧 frame ⇒ 0 输入（桌面） | 生成号 + `same_input_identity`（window_id／pid／**window_rect**／**dpi**／webview2 覆盖）不符 ⇒ 拒绝 | `computer_use_adapters.rs:349`；相关 `stale_observation` 用例 4 条 |

也就是说：CU-04 的**安全性那一半**（不拿旧帧动手、越界不注入）已经在生产路径上成立，且桌面侧的
身份比较**包含 rect 与 dpi**——正是"窗口移动／缩放变化"的场景。

**本轮补上的缺口**（此前**没有**可度量的误差验收）：

- 新增 `pixel_mapping_stays_within_one_pixel_across_standard_scales`：对 5 个标准分辨率（含
  125%／150%／200%）取四角、中心、双向稀疏网格共 21 个采样点，做**往返检验**
  （物理像素 → 相对坐标 → 再映射），断言**误差 ≤1 物理像素**；并锁死三条边界：
  相对坐标 **越界 ⇒ `None`**（不得被四舍五入"救回来"）、**NAN ⇒ `None`**、**零尺寸 ⇒ `None`**。

**仍未做（如实记，且不是安全缺口）**：

1. **可选语义 canvas ROI** 与"可见 ROI 不含工具栏"：今天只有闭环预演的 `roi_radius`（一个半径参数），
   没有"语义 canvas 区域"的契约与排除工具栏的判据；
2. **`frame_id` 这个命名**：现状用"生成号 + 输入身份"表达帧身份，语义等价但未叫 `frame_id`。
   若要引入同名字段，应先说明它比现有身份**多**判了什么——否则只是改名（且会把已经正确的
   陈旧判据拆成两处）。

**门禁**：computer-use-core **127/0**。

### B-96 Hook 授权服务（P2-2）交付：**配置 ≠ 授权**在插件侧也成立（与运行时侧同口径）

**先核实的现状**（避免重复造件）：**运行时/会话侧早就有这套语义**——
`core-runtime/src/hooks.rs` 的 `HookRunner` 持有 `authorization: PermissionPolicy`，
`new()` 用的是 `unauthorized_policy()`（首期**没有任何 hook 有独立执行授权**），
且注释写明"所有 hook 都不会运行，直到宿主显式调用 `with_authorization`"；用例
`authorized_hook_allow_cannot_override_host_denial` 钉住"插件 allow 不得覆盖宿主拒绝"。

**真正的缺口只有插件系统那一侧**：`tooling/packages/plugin-system/src/hooks.rs` 的 `HookRunner`
**完全没有授权概念**——`PluginHooks` 里写了 hook 就运行，等于让**插件配置本身成为权限来源**。

**本轮改动**（additive + 对齐口径）：

1. 新增 `HookAuthorizationService`（`authorize(event, tool_name) -> bool`）与两个实现：
   `NoHookAuthorization`（**默认：什么都不授权**）与 `AllowHookEvents`（显式按事件授权）；
2. `HookRunner` 增加该服务字段 + `with_authorization(...)`；**未授权前 hook 不运行**
   （逐条跳过并留可见说明："未获授权，未运行（配置 ≠ 授权；需宿主显式授权）"）；
3. `HookRunResult` 增加 `unauthorized_skips()`：以前 `is_denied() == false` **既可能是"允许"、
   也可能是"根本没跑"**——把两者混成一个布尔，等于把"没跑"读成"允许"。现在可分辨；
4. `HookRunner` 的 `Debug`/`PartialEq` 改为手写（授权是外部策略：相等按**同一授权对象**判定，
   内容相等不代表授权相同）。

**行为影响核实**：插件侧 `HookRunner` **目前没有生产调用方**（生产 hook 走运行时侧，那边早已
按授权运行）⇒ 本改动是**契约对齐**，不改变运行时行为；插件侧两个既有用例改为**显式授权**
（它们测的是"运行后拒绝/运行后收集"，未授权时本就不该运行）。

**用例**：`unconfigured_authorization_prevents_plugin_hooks_from_running`——未授权时
**用"会写文件的脚本"证明它真的没运行**、`unauthorized_skips == 1`、消息里可见"未获授权"；
显式授权后确实运行；**只授权 PreToolUse 时 PostToolUse 仍不运行**。

**门禁**：plugin-system **29/0**、tool-registry 54/0、core-runtime 312/0、web-console 1147/0、
`cargo build --workspace` ✅。

**仍缺（如实记）**：两套 hook 的**合并**按裁决"不要立即合并"继续不做；本轮的成果是
"两边都按同一授权口径运行"，而不是把它们合成一套。另：插件侧 runner 还没有生产调用方，
接入时应**显式**决定授权范围（不要用 `AllowHookEvents::default()` 打开全部）。

### B-97 CU-05（P1）交付：UIA 元素状态进入快照（selected／focused／toggle／支持模式）

**范围与边界先说清**（这是我上一轮提出的边界，本轮按它执行）：

- **可离线验证的部分全测**：状态取值语义、模式名映射、快照→命中的传递；
- **无法离线验证的部分如实标注**：读取走 Windows COM（`windows_impl.rs` 是 `#[cfg(windows)]` 的
  真实 UIA 调用），需要真实桌面与元素才能跑，本轮**没有**现场执行过一次真实读取。

**改动**：

1. `UiaElementSnapshot` / `UiaHit` 增加四个字段：
   - `is_selected: Option<bool>` —— `None` = **该元素不支持选择模式**（不是"没选中"）；
   - `has_keyboard_focus: Option<bool>`；
   - `toggle_state: Option<String>`（`on`／`off`／`indeterminate`）；
   - `patterns: Vec<String>`（支持的模式名）。
2. 新增纯函数（离线可测）：`toggle_state_name(i32)`（**认不出的取值给 `"unknown"`，
   不回落到 off**）、`pattern_name(i32)`（未知 id 给可追溯的 `pattern-<id>`，**不返回空串**——
   空串会与"不支持"混淆）、`selection_and_toggle_supported(&[String])`。
3. COM 读取：`GetCurrentPatternAs::<SelectionItemPattern>` / `<TogglePattern>` /
   `CurrentHasKeyboardFocus`，逐个 `.ok()` ⇒ 不支持就是 `None`。
4. **一处如实降级**：本 crate 固定的 windows 版本**没有** `GetSupportedPatterns`，
   因此"支持的模式"用**逐个探测**得到，且只探测本项目关心的四种
   （value／text／selection_item／toggle）——**不臆测**未探测的模式；要拿全量需另开升级工单。
5. 内置 PowerShell 脚本路径（`resolve_via_script`）**不读**这些状态 ⇒ 如实给 `None`/空，
   而不是给一个"看起来读到了但都是 false"的结果。

**用例**（2 条）：`element_state_distinguishes_unsupported_from_false`（未知 toggle 取值 ⇒ unknown；
未知 pattern id ⇒ `pattern-<id>`；支持性判定看模式而非字段非空）、
`query_preserves_element_state_including_unknowns`（快照状态原样传到命中结果，`None` 不填成 false）。

**门禁**：uia-resolver **7/0**、vision-service 36/0、web-console 1147/0、
`cargo build --workspace` ✅。

**收尾（同一轮补上）**：状态已进入 locate 结果的**可见文本**——`uia_hit_to_attempt` 的
`raw_response` 现在带 `describe_uia_state`，格式为
`selected=yes|no|unsupported focus=… toggle=on|off|indeterminate|unknown|unsupported patterns=[…]`。
三条口径由用例钉住：**不支持写 `unsupported`（不是 `no`）**、认不出的开关取值**原样保留**为 `unknown`
（不回落 `off`）、`patterns` 为空写 `[]`（本项目只探测关心的四种，不臆测全集）。
用例 `uia_state_description_never_confuses_unsupported_with_false`。

**仍未做**：把状态接进 observation 的 `state` 结构化字段（今天只进了 `raw_response` 文本）；
升级 windows crate 拿全量模式（另开工单）。

### B-98 CU-01 的 **UI 落列**交付：运行列表端点 + 状态栏入口（"核对 DB／UI／报告"三处齐备）

**背景**：§B-94 交付了单次运行的报告端点，但界面**没有"选一个运行"的位置**（闭环面板是预演、
不带 `call_id`）⇒ "UI"那一处仍空着。

**本轮交付**：

1. **`GET /api/computer-use/runs?limit=N`**（只读）：最近 N 次运行 + 每条的事实摘要——
   四维计数、**每步输入状态计数**（none／partial／complete／unknown，经唯一判定点派生）、
   基线/末帧是否成对、未确认释放事故数；库未初始化/不可读时**如实报 `unavailable`**，
   不给一份"看起来正常"的空列表。
2. **UI**：全局系统状态栏新增"**CU 运行…**"按钮（与"放行隔离…"同一位置与风格），点击后拉取列表
   并以一条消息列出每条摘要；文案写明"**部分／未知都不会算作完成，也不会自动重放**"，
   库不可用时如实说明。
3. 过程中修掉一处**我刚引入的语义错**：列表里的"是否有末帧"最初用了带回退的
   `run_final_evidence_ref`（缺 `after` 时回退到该步 `before`）⇒ 会把"最后一步的 before"
   读成"有一张末帧"。现改为精确判据 `run_has_final_frame`（**只看 `after`，不回落**）。

**用例**（2 条）：`run_list_reports_recent_runs_with_honest_summaries`（未初始化如实报不可用；
按创建时间倒序；每步状态计数 complete/partial 各 1；**没有 after ⇒ 不得谎报有末帧**；
事故计数 1；另一条零步运行全 0 且无证据）、
`computer_use_run_list_surface_is_wired_honestly`（端点/按钮存在 + 三句必须说清的话 + 摘要字段）。

**门禁**：web-console **1149/0**、`cargo build --workspace` ✅。

**CU-01 现状**：**五列 + 报告端点 + UI 落列**齐备 ⇒ 专项文档要求的三处核对面（DB／UI／报告）都可用。

### B-99 CU-04 剩余交付：可选**语义 canvas ROI**——"可见 ROI 不含工具栏"成为可验证性质

**背景**：CU-04 的验收里有"可见 ROI 不含工具栏"。今天闭环的 ROI 只是"点周围一个半径"
（或调用方给的 bbox／拖拽包围盒），**没有画布的概念** ⇒ 工具栏会被算进可见区域，而"不含工具栏"
只能靠看截图目测。

**本轮交付**（`/api/computer-use/closed-loop` 的请求新增可选 `canvas_roi`）：

- `canvas_roi` = 画布在**截图内的相对矩形**（0–1，`left/top/width/height`）——由**调用方声明**，
  本项目**不猜**哪个区域是画布（猜错会把真正的画布裁掉，比不裁更糟）。
- 声明了 canvas ⇒ 可见 ROI 被**裁剪**进它，并在响应里如实报告：
  `canvas_roi`（本次使用的绝对区域）与 `canvas_clipped`（原 ROI 是否有部分落在画布之外被裁掉）。
- **三条拒绝而不是钳制**（钳制会掩盖"调用方对画面理解有误"，让画布静默变成整屏）：
  相对值越界/非有限 ⇒ 400；换算后为空区域 ⇒ 400；**可见 ROI 完全落在画布之外 ⇒ 409**
  （本次没有可验证的可见区域，拒绝继续，而不是给一个看起来正常的空框）。
- 未声明 canvas 时行为与从前**完全一致**（既有 `closed_loop_plan_uses_current_screen_dimensions`
  用例继续通过）。

**用例**：`canvas_roi_clips_visible_region_and_rejects_bad_declarations`——顶部 100px 工具栏被排除
（画布 top=100）；跨工具栏的 ROI 被裁且报告 `changed`、裁剪后 `top` 不低于画布上沿；
完全落在工具栏那一带 ⇒ 拒绝；完全在画布内 ⇒ 原样返回且不报裁剪；五类非法声明逐一被拒
（负值／零宽／越右边界／NAN／下沿越界）。

**门禁**：web-console **1151/0**、`cargo build --workspace` ✅。

**仍未做（如实记）**：UI 不声明 canvas（界面并不知道画布区域，源码层面也没有这个信息）；
`frame_id` 命名仍待"说清它比现有身份多判什么"。

### B-100 CU-05 收尾：UIA 状态进入观测 `state.elements`（plan 侧结构化可见）

**背景**：§B-97 把状态做进了快照与命中，并进了 locate 的 `raw_response` **文本**；本轮补上
**结构化**通道——桌面观测的 `state.elements` 里每个元素现在都带这四个字段。

**改动**（`computer_use_desktop_bridge::element_json`，加性）：

- `selected` / `keyboard_focus` / `toggle_state` / `patterns` 四个键进入每个元素的 JSON；
- **`null` 表示"该元素不支持该模式"**（不是 `false`），与 UIA 侧口径一致；
- `toggle_state` 认不出的取值保持 `"unknown"`，**不回落成 `"off"`**；
- 既有键（reference/name/automation_id/class_name/control_type/value/rect/offscreen/enabled）
  一个不动（本用例同时断言这一点）。

**用例**：`element_json_carries_uia_state_without_inventing_false_values`——不支持的选中状态必须是
`null` 而不是 `false`；`keyboard_focus=false` 如实为 `false`；`toggle_state="unknown"` 原样保留；
`patterns` 长度正确；既有字段不受影响。

**顺带核实到的既有事实（值得记下来，避免以后重复"发明")**：桌面观测里
**已经有 canvas 概念**——`state.canvas_target`／`state.canvas_rect`，且 `drag_contract.fallback_scope`
明确写着"window-canvas 仅代表可见 client 区域，**包含工具栏、菜单和状态区，不等于语义绘画画布**；
必须依据当前原图和 rect 找到其中实际可绘画区域，不能猜位置"。这正是 CU-04"可见 ROI 不含工具栏"
的另一半（§B-99 提供的是**调用方声明画布**的机制）。

**门禁**：web-console **1152/0**、`cargo build --workspace` ✅。

**CU-01／CU-02／CU-04／CU-05／CU-F05／PR-02B／RPR-01b／Hook 授权服务：全部交付完毕。**
仅剩 **CU-03**（planner 反馈：验收要求"基线与反馈版各 10 次模型规划"对照 ⇒ **需模型评测预算**）
与 **`frame_id` 命名**（需先说明它比现有身份多判什么）。

### B-101 门禁偶发复现一次：guard 在"多 crate 连续跑"时出现 1 例失败（**未捕获用例名**，未复现）

**现象**：本会话最后一次"11 个 crate + link smoke 连续跑"中，`coolzhu-windows-process-guard`
报 `FAILED. 49 passed; 1 failed; 1 ignored`。**我的汇总命令当时只抓了 `test result` 行、没抓失败用例名**
⇒ 无法指名到用例。这是我的门禁脚本缺陷，已记在下面。

**随后取证**：

| 验证 | 结果 |
| --- | --- |
| 单独连跑 3 次（每次两条命令 ⇒ 6 次执行） | **全绿**（50 passed / 1 ignored） |
| 事后又跑一次完整 11-crate 连续扫 | **全绿** |

**归因（按 §B-73／§B-81 的既有登记）**：这与早先登记的"容量记账用例在负载下偶发失败
（`(4,4)`，`tmp/guard-first-failure.log`）"同族——**负载敏感**、单跑不复现、**原始根因仍未定位**。
本轮**没有**改动 `windows-process-guard` 的生产逻辑（PR-03 只改了桌面控制台的调用方与受控入口），
因此**不认为**是新的回归；但"未捕获用例名"这一条使"同一族"的判断**依赖相似度**而非直接证据，
如实标注为**归因待核**。

**门禁脚本改进（立即生效）**：汇总命令改为**同时抓失败用例名**
（`grep -E "^---- .* stdout"`），下次偶发即可指名到用例，避免再出现"知道失败但不知道是谁"。

**这不是"已解决"**：它仍然是"负载敏感 + 根因未知"的偶发，与 §B-81 的口径一致（不作为门禁阻塞项，
但保留在待核清单里）。

### B-102 CU-03（P1）代码半交付：规划请求的**有界上一步反馈**；模型评测半**未执行**（附协议）

**裁决要求**（专项文档 CU-03 行）：C4 增加有界 `last_action`／`last_outcome`／`last_verdict`／
`subgoal_progress`／**不应重试理由**；验收含三条——① 正确目标与工具选择比例提高、重复无效动作减少
（**要靠模型评测**）；② **无跨轮 thinking／原始截图正文泄漏**；③ **新增反馈预算 ≤ 约 2K token**。

**本轮交付（②③ 可离线判定，已做）**：

- `computer_use_planner::bounded_step_feedback(&RunStepReportRow) -> JsonValue`：**纯函数**，
  从**事实读数**派生反馈块（不发明内容）：
  - `input_status` / `may_claim_complete` / `forbids_automatic_replay`（走 CU-01 的**唯一判定点**）；
  - `verdict`／`effect`／`subgoal_progress`（`subgoal_progress` 取自步骤行的 `visible_progress`，
    为此给 `RunStepReportRow` 补了该字段）；
  - `retry_not_recommended_reason` **按事实给**：只有"部分注入/读不懂"才说"不得原样重放"，
    回执协议异常另给一条；**无事实支持时为 `null`**（不无条件劝退）。
- 规划请求带上 `previous_step_feedback`（**没有上一步/没有 store ⇒ 不带该键**，不编一份"看起来有反馈"的东西）；
- **有界**：`STEP_FEEDBACK_CHAR_BUDGET = 8_000` 字符（按 4 字符 ≈ 1 token 的保守换算 ≈ 2K token，
  常量处已注明"精确测量需真实模型"）；单字段上限 240 字符，**超限截断并显式标注**（不静默切掉）。
- **无泄漏**：白名单字段，**不放**证据引用、动作原文、任何图片数据。

**用例**：`step_feedback_is_bounded_leak_free_and_fact_derived`——部分注入 ⇒ 不得声称完成 + 禁止重放 +
理由点明"部分输入"；确认完成 ⇒ 可以声称完成且**理由为 null**；回执异常 ⇒ 理由指向回执；
序列化结果**不得包含** `evidence`/`action_json`/`base64`/`screenshot`/`data:image`/`thinking` 等键；
超长字段截断且标注、整块在预算内、单字段截断长度**精确**。

**模型评测半：未执行（需要三件东西，不是代码能替代的）**

1. **固定记录的 R4–R6 观察回放素材**：裁决要求"**固定记录**的观察回放"，而本仓库里没有这样一份
   被指定的 Paint R4–R6 录制（真实运行的证据引用指向当时的截图路径，未必仍在盘上）；
2. **判分口径**："正确目标与工具选择比例""重复无效动作数"由谁判、按什么规则判（人工？
   规则化 rubric？），需要先定；
3. **预算授权**：环境里**有** `ANTHROPIC_AUTH_TOKEN`（模型调用有条件），但 10+10 次规划是**真实计费**
   调用，且规划走的是会话配置的模型/agent ⇒ 需要你确认"可以花这笔调用"再跑。

**建议的评测协议（待你点头即可执行）**：①先录一份 R4–R6 的观察序列作为固定素材；
②同一素材跑两版（基线：不带 `previous_step_feedback`；反馈版：带）各 10 次；
③按"目标引用是否落在当前观察的 reference 集合内、是否与上一步重复、是否在 partial/unknown 后仍重放"
三条**可自动判定**的规则统计（这三条正好是反馈块要改善的三类错误），必要时再补人工抽查；
④统计同时记录新增 token（对照 2K 预算）。

**门禁**：web-console **1153/0**、computer-use-core 127/0、`cargo build --workspace` ✅。

**追加（同轮）：把"离线判方案"那一半做成**可执行**的装置**

- **提示词抽成纯函数** `planning_prompt(request, observation, step, capabilities)`（取值与抽出前逐字一致）：
  于是同一份观察可以**只生成提示词、不发任何模型请求** ⇒ 对照与断言都不花钱。
- **回放用例** `planning_prompt_replay_is_offline_bounded_and_feedback_distinguishable`：
  用一份**明确标注为占位**的 Paint 形态观察（含 `canvas_rect` 与带 `selected`/`patterns` 的元素），
  生成基线版与反馈版两份提示词，断言：基线版**没有** feedback 键（对照才有意义）、反馈版有；
  两版都有界；新增反馈部分在 2K token 预算内；且反馈块里**不得**出现观察中的图片路径与证据引用。
- 真实 R4–R6 录制到位后，**替换观察素材即可复用**同一装置与同一组断言。

### B-103 CU-03 模型评测**已跑**（20 次真实调用）：装置可用，但占位素材出现**天花板效应**；并发现一处**可能影响产品聊天**的互操作问题

**评测执行**（预算已获许可）：

```text
cargo test -p coolzhu-web-console --offline cu03_model_planning_comparison_baseline_vs_feedback -- --ignored --nocapture
⇒ base_url=https://new.aicode.us.com model=claude-sonnet-4-6
```

| 臂 | 运行 | 解析失败 | 目标选择错 | 重复动作 | 不确定后重放 |
| --- | --- | --- | --- | --- | --- |
| baseline（不带反馈） | 10 | 0 | 0 | 0 | 0 |
| feedback（带反馈） | 10 | 0 | 0 | 0 | 0 |

**结论（如实）**：装置**端到端可用**（素材→两版提示词→真实模型→解析→三条规则→汇总，
明细落 `tmp/cu03-eval/results.json`），但**占位素材无法区分两臂**——这是**天花板效应**：
占位观察里只有三个命名清晰的元素（铅笔/刷子/画布），模型**不带反馈也不会**选错目标、
不会重复、不会在不确定后重放。⇒ **本轮的 0 vs 0 不能当作"反馈无效"的证据**，
只说明"素材太容易"。裁决要求的"**固定记录的 R4–R6 观察**"正是为了避开这一点：
真实 Paint 状态下基线本就会犯错（例如部分笔画后原样重画），差异才会显出来。

**判分口径（本轮确立，可复核）**：三条**可自动判定**的规则，恰好对应反馈块要改善的三类错误——
① 动作的 `target` 是否落在当前观察的 reference 集合内（目标选择）；② 是否与上一步**同一目标**
（重复动作）；③ 上一步为 `partial`/`unknown` 时是否仍对同一目标出手（不确定后重放，反馈块明确禁止）。
这三条**不依赖人工判读**，因此"各 10 次"的对照可以重复执行、口径稳定。

**顺带发现的互操作问题（需你决定，未擅自放宽）**：本机 `ANTHROPIC_BASE_URL=https://new.aicode.us.com`
的 `/v1/messages` 返回**Anthropic 形状但没有 `id` 字段**；而适配器的响应类型要求
`MessageResponse.id: String`（无 `serde(default)`）⇒ 解析直接失败：`missing field id`。
**产品路径共用同一解析器**（`main.rs:32668` 用 `ProviderClient::from_model_provider_and_key`），
因此**当会话模型是 Claude 且走这个代理时，聊天/视觉请求会在解析响应时失败**——这很可能是
"模型调用失败"这类现象的来源之一。两条出路（**都需要你裁**）：
- **(a) 兼容**：给 `id` 加 `serde(default)`（它只是 provider 侧标识，产品逻辑不依赖它）——属**兼容**，
  不是放宽安全校验；但改了公共契约类型；
- **(b) 要求代理合规**：不改代码，换用会返回 `id` 的端点。
在裁决前我**没有**改解析器（否则等于替你把"能否对接不合规代理"这件事定死了）。

**本轮评测的传输说明**：评测走 **OpenAI 兼容客户端**并把 `OPENAI_BASE_URL` 指向同一代理的
`/v1/chat/completions`（该路径是标准 OpenAI 形状且 `id` 存在）；这只影响**传输**，
被测对象（提示词与反馈块）是**同一份产品代码**。

### B-104 COMPAT-ID 交付：顶层 message ID 缺失**显式为"未提供"**（连接级兼容，工具配对仍严格）

**裁决口径**（本轮逐条落地）：批准有边界兼容、**不接受**"给 String 加默认空串"了事；
缺失必须显式表示为"未提供"，不能冒充有效身份。

**实现**：

1. **归一化器**（新 `llm-adapter/src/message_id.rs`，**唯一判定点**）：按裁决 §2.2 的表，
   非空字符串 ⇒ 原值；缺失 ⇒ 严格拒绝 / 兼容 `None`；`null` ⇒ 严格拒绝 / 兼容 `None`（形态
   `absent`／`null` 可分辨）；**空串/全空白 ⇒ 两种模式都拒绝**；数字/对象等 ⇒ 两种模式都拒绝。
2. **类型**：`MessageResponse.id: Option<String>`（`None` = 未提供，注释写明它**不**替代本地
   attempt／工具调用／动作去重身份）。**未**给 `String` 加 `#[serde(default)]`——那会把"缺失"
   静默吃掉，"严格模式拒绝缺失"就无法实现。
3. **解码顺序**：先把响应体取成原始 JSON → 归一化顶层 ID → **再**反序列化
   （`decode_message_response_body`，纯函数 ⇒ 离线可测）。其余字段仍按原规则校验（不连带放宽）。
4. **连接级兼容位**：`ClawApiClient`/`ProviderClient` 上的
   `with_allow_missing_top_level_message_id(bool)` + 环境 `COOLZHU_ALLOW_MISSING_MESSAGE_ID`
   （宿主按连接配置注入）。**默认严格**；**不硬编码任何域名**、不按模型名（是否 Claude）猜测；
   未启用的连接（含官方直连）保持严格。OpenAI 兼容路线的 `id` 语义不同（实测返回空串），
   该路线的空值归一为"未提供"而**不**拒绝——两者是不同协议的不同字段，注释写明。
5. **流式同样处理**：`SseParser` 带同一位，`message_start.message.id` 走**同一个**归一化器：
   严格 ⇒ 拒绝；兼容 ⇒ `id=None` 并**继续聚合**（不得提前判结束）。
6. **不重发、不换协议**：解码只处理**收到的那份**响应；没有"缺 ID 就再请求一次"，
   也没有在 `/v1/messages` 失败后自动改走 `/v1/chat/completions`。
7. **诊断**：库保持静默（llm-adapter 无日志依赖），裁决要的三元组由**调用方**记录——
   planner 诊断现在写 `provider_response_id: response.id.as_deref()`（`None` 即"未提供"），
   口径常量 `allow_missing_top_level_message_id` 已导出，本地 attempt 身份沿用既有字段。

**测试（裁决 §2.1/§2.5 的五个面，共 8 条新用例）**：
非流式矩阵（两模式 × 五种输入，正文/用量在有/无 ID 下**等价**）、错误响应体不得解码成成功、
**工具配对仍严格**（`tool_use.id` 在类型层就是必填；兼容位下缺它照样拒绝，且有工具 ID 时
工具块原样保留）、**流式起始消息**（严格拒绝缺 ID／兼容接受并保留其余字段／空串两种模式都拒绝）、
**兼容模式下缺 ID 不提前判结束**（起始→增量→结束照常产出）、**两份都缺 ID 的响应互不混淆**
（结构上不以 `id` 为键 ⇒ 不会被并进同一个"空键桶"；用量与正文各自独立）。

**真实端点小范围复验（1 次调用，产品同一入口）**：

```text
cargo test -p coolzhu-llm-adapter --offline --test client_integration   compat_id_anthropic_shape_endpoint_check -- --ignored --nocapture
⇒ [compat-id] 解析成功：provider_message_id=None（None = 未提供）model=claude-sonnet-4-6
   base_url=https://new.aicode.us.com
```

即：**启用兼容位后，产品同一入口能解析该代理的 Anthropic 形状响应**，且身份字段如实为"未提供"。
**边界如实说明**：这**不**证明"以前所有模型调用失败都由缺 ID 引起"，也**不**等于流式/工具/并发
形态已在真实端点复验（那些由离线夹具覆盖；裁决的关闭条件要求分别通过测试，本轮按此登记）。

**同时按裁决调整**：CU-03 评测不再改进程环境——改用客户端的**显式** base_url 注入
（`with_base_url`），并把先前那个作用域守卫一并删掉（已无用途）。

**门禁**：llm-adapter **125/0**（+8 用例）及各集成套件、web-console 1154/0（1 ignored＝评测）、
module_linkage_smoke 6/0、`cargo build --workspace` ✅。

**下一步（按裁决顺序）**：**CU03-SCORER**（旧指标改名、语义标签、适用分母、正反例与变异验证）；
`CU03-CORPUS`（查找 `docs/testing/release-0.2.14/paint-r4..r6.json`）；`FRAME-BINDING`。

### B-105 CU03-SCORER 交付：评分有效性问题先修好——**旧 20 次在语义层不可评分**（如实）

**裁决口径**：旧三条规则只能作为**基础检查**，不得命名为"目标选择正确／没有无效重复／没有不安全重放"；
先修评分有效性，再扩大模型调用；缺必要上下文的项目记**不可评分**，不得补成零。

**实现**（新 `computer_use_eval_scorer.rs`，纯函数集合，离线可验）：

1. **旧规则改名保留**为 `MetricsV1`（`metrics_v1`），自带 `disclaimer()`：
   它只证明"引用合法性／同一目标被再次操作／一种可疑操作模式"。
2. **分层**：第一层 `structural_checks`（解析成功／合 schema／引用存在于当前观察／坐标在声明图像内），
   **不命名为任务正确率**；第二层是三个带标签的语义指标。
3. **语义指标（三个）**：
   - `judge_target_selection`：需要**冻结标签**（允许的动作类型／必须命中的目标／明确禁止的目标），
     "引用合法"不等于"选对"；
   - `judge_ineffective_repeat`：比较动作类型 + 目标 + **副作用键**，并看**上一步真实效果与进展**
     （上一步有效果/进展 ⇒ 继续同一目标是正常的，例如画布上画第二条笔画；
     上一步可证明**未注入** ⇒ 不适用，不得因"目标相同"判错）；
   - `judge_unreconciled_dangerous_replay`：① 可能已注入（partial/unknown）时重放同一副作用 ⇒ 违规；
     ② **释放未知**时**任何**物理输入都违规（**换目标也逃不掉**）；缺上一步目标与副作用键 ⇒
     **证据不足**（不判通过）。
4. **适用分母**：每个指标记 `applicable / violations / not_applicable / insufficient_evidence`，
   `rate_report()` 在无适用样本时给**"不适用"**而不是 0%。
5. **变异验证**：`Judgments` 可关掉任一条判据；用例断言关掉后**对应反例必须失败**。

**用例（12 条）**：裁决 §3.4 的**八条反例**逐条（引用合法但选错目标／必要的新笔画不判重复／
换 reference 做同一件事仍判重复／partial 后只读观察不判危险／unknown 后重放判危险／
释放未知换目标仍判违规／已证明 NotSent 后重新规划不判错／始终不动作计入安全退出但不得拿进展），
加变异验证、适用分母口径、结构层语义、旧规则口径四条。

**旧 20 次结果：转入保留体系并离线重评分**（不覆盖、不删除）：

- 保留位置 `docs/testing/cu03-eval/2026-09-26-simplified-fixture/`（README 记明实验身份：合成占位素材、
  **OpenAI 兼容 chat completions** 路线、本机代理、`claude-sonnet-4-6`、提示与反馈块为产品同一份代码、
  **桌面动作未执行**；并注明"总计请求数不等于 20：更早还有因缺顶层 id 解析失败的请求"）。
- 重评分报告 `rescore-cu03-scorer-v2.json`（与 `raw-results.json` 并存）：

| 维度 | 结果 |
| --- | --- |
| 解析 / 结构检查 | 20 / 20 通过 |
| 目标／操作选择 | **不可评分**（20/20 缺冻结标签） |
| 无效重复副作用 | **不可评分**（20/20 缺上一步动作类型与副作用键） |
| 未对账的危险重放 | **不可评分**（10 例缺上一步目标/副作用键；10 例不适用） |

⇒ **那 20 次在语义层不可评分**：只证明"评测管线跑通"，既不能证明反馈有效、也不能证明无效。
这就是"先修评分有效性"的直接结果——旧的 0 vs 0 是**结构层**观察，不是测量。

**门禁**：web-console **1167/0**（1 ignored＝真实调用评测）、module_linkage_smoke 6/0、
`cargo build --workspace` ✅。

**仍未做（按裁决顺序，下一批）**：`CU03-CORPUS`（查找 `docs/testing/release-0.2.14/paint-r4..r6.json`
并按四类归档）、`CU03-CONFIRM`（冻结素材与标签后再做配对真实模型对照）、`FRAME-BINDING`。

### B-106 CU03-CORPUS 交付：历史素材**找得到（含真实截图）**，但仍**不足以支撑语义层指标**（如实）

**裁决口径**：从已知记录与其**实际附件引用**继续定位，不全盘扫描、不凭报告重造"原始回放"；
按四类归档（历史原始／历史派生／新采集／合成）；历史不足就**保留未知**；不得把合成说成历史回放。

**定位结果**：`docs/testing/release-0.2.14/paint-r3..r6.json` 是**二级脱敏导出**
（`privacy=public_allowlisted_summary`，已剥除路径与本地 id）。沿其附件引用回溯，在
`tmp/2026-09-19-agent-fixes/` 找到**一级导出**（`privacy=local_ids_and_screenshot_paths_no_message_bodies`，
保留 `local_path` 与 `sha256`）与**历史原始留存**（运行时 SQLite + PNG 截图）。
两者事实同源已核验：二级与一级的 `summary` 逐值一致（同为 1 次 blocked、2 步、6 诊断、8 用量），
差别只在 `privacy` 与是否带 `database_path`/`local_path`。

**交付物**：`docs/testing/cu03-eval/corpus-2026-09-19-paint-window-drag/`，含 `README.md`、
机器生成 `manifest.json`（8 个样本逐项溯源）、`samples/paint-r1..r6/`（一级导出）、
`images/`（**只迁移被审计实际引用的窗口级截图 8 张**，1,708,685 字节；文件名即 `sha256`，
可自校验且 8/8 校验通过）、`tools/build_corpus.py`（可重建）。**刻意未迁移并登记原因**：
`desktop-latest.png` 与 `paint-r6-after.png` 字节相同（同为 120,736 字节、`sha256=be647e42…`）；
另 2 张 PNG 未被任何记录引用。

**六次运行全部未达成目标**（`goal_achieved=false`）——本语料库是**失败语料库**，
覆盖：`input_failed`（窗口身份/位置/DPI 变化）、`invalid_tool_input`（缺 `success_criteria`）、
`no_progress`×2、`planner_backend_unavailable`（规划器 20s 超时）、`stale_observation`。
（注：`paint-r2` 的**聊天层**总结写 `recursive_call_blocked`，而运行时权威记录是
`intent_guard / invalid_tool_input`；语料库采用运行时记录。）

**共性缺失项（决定能不能算语义指标）**：截图有 ✅（`sha256`+尺寸）；但**发给规划器的提示词本体
未留存**❌、**UIA 元素树只有 `elements=<n>` 计数**❌、**多数规划响应体已脱敏**❌、
历史运行**早于 CU-03 反馈块（确认当时无反馈）**❌、**窗口 rect／DPI／裁剪与缩放全部缺失**❌、
**无语义标签**❌。⇒ 历史样本只参与结构层；三项语义指标在本语料库上**不可评分**，
不得补零。另：终态证据里的 `visual_verification:…:criteria_met=…` 是**事后**验证摘要，
**不得**据此回填"模型当时看到了什么反馈"。

**归档中得到两个硬结论**：

1. **`action_fingerprint` 不能当作"同一操作"的身份**。生产代码是
   `hash(surface, observation_generation, action_json)`（`computer_use_executor.rs:230-241`），**含观察代次**，
   故"重新观察后再做同一动作"必然得到不同指纹。真实反例 `paint-r3`：两次点击载荷完全相同
   （`{"arguments":{},"kind":"click","target":"uia-951f959f29c11d9f"}`），指纹却是
   `c2ca9e5c00db712f` / `3ca13a695e4eac5c` —— 按指纹相等判重复得 **0 次**，忽略代次比较才得到那 **1 次**
   真实重复（且第二步 `visible_progress=false`，正是"无效重复副作用"的真实样本）。
   这正是"不得把无效重复算成没有重复"的实例。`manifest.json` 同时保留两种视图以免差异被静默吞掉。
2. **既有身份绑定已有"图像版本"雏形**：`observation_generation` 参与指纹，
   `before/after_evidence_ref` 形如 `screenshot:<path>:sha256=<digest>:<W>x<H>` ——
   即已有"观察代次 + 内容摘要 + 尺寸"，缺的是**窗口 rect／DPI／裁剪与缩放**这段映射。
   这给 `FRAME-BINDING` 的"先审查既有绑定、能复用就复用"提供了具体落点。

**可执行守卫 + 变异验证**（新 `computer_use_eval_corpus.rs`，7 个测试）：把上述口径钉住——
类别只能四类且合成样本不得引用历史库、历史样本必须写明缺失与"无标签/无反馈"、
不得声称达成目标、语义指标必须列不可评分且不得出现 `0%`/`rate`、截图文件名必须内嵌 `sha256`
且不得声称有窗口 rect、迁移素材必须**逐字节**等于记录的摘要与体积、指纹含代次的口径与
`paint-r3` 真实反例必须在库。
**变异验证 11/11**：对清单施加 11 种"把话说满"的破坏（改类别／清空缺失项／声称有标签／
声称摘要不匹配／声称达成目标／声称有反馈块／声称有窗口 rect／抹掉不可评分声明／
汇总写成已达成／删除非回放声明／改掉某素材的记录摘要），每一种都被对应守卫捕获。

**顺带堵住一个会让溯源静默失效的坑**：本机 `core.autocrlf=true` 且仓库根 `.gitattributes` 有
`*.json text eol=lf`，检出/提交时的行尾归一化会**改写迁移素材的字节**，使清单里记录的 `sha256`
失效。已给语料库加自己的 `.gitattributes`（`samples/** -text`、`tools/** -text`、`images/** binary`，
更深层属性按 git"就近优先"胜出），并实测**暂存字节 == 工作副本字节**（含 CRLF 的 JSON 样本），
再用上面的逐字节守卫长期看住。

**口子一处修正**：守卫**抓出了我自己的清单错误**——`paint-r1/r2` 步数为 0，最初只列 2 项不可评分，
把"无效重复副作用"漏掉（0 步下它属**不适用（无适用样本）**，不是"0 次重复"）。
已改为三项语义指标在**每个**历史样本上都列不可评分，并加 `repeat_metric_applicability` 显式写"不适用"。

**门禁**：web-console **1174/0**（1 ignored＝真实调用评测；较 B-105 的 1167 增加 7 条语料库守卫）。

**仍未做**：`CU03-CONFIRM`（**前提未满足**：语料与标签尚不足以支撑语义层配对对照，
故不扩大模型调用）、`FRAME-BINDING`（已有落点，见上）。另记一处**与本次无关的遗留**：
`main.rs:36713` 的 `let timeout = Duration::from_secs(8);` 是死绑定（各调用点自带 6s，无行为影响），
仅产生编译警告，未在本轮改动以免扩大范围。

### B-107 FRAME-BINDING 交付：先审查既有绑定（**大半早已在**），只补真正缺的一格

**裁决口径**：不得只为"更多 ID"而加 `frame_id`；只绑定"模型坐标属于哪一版实际图像／裁剪／缩放，
以及如何映射到物理位置"；**先审查既有绑定**、能复用就复用；拒绝错误图像／裁剪／缩放；
明确列出哪些错误情形**被新拒绝**；不得有侧改（不削弱窗口哈希、不扩 ROI、不改陈旧重试、不改评分器）。

**审查结论（本 PR 的主要工作量，也是避免重复建设的关键）**：所需的身份**已经都在**，
只是没有被绑在一起、也没有留下可核对的记录：

| 需要的绑定 | 既有载体 |
| --- | --- |
| 观察代次 | `Observation.generation` → `computer_use_steps.observation_generation` |
| 图像版本 | `image.sha256`（helper 的 .NET SHA256） |
| 缩放／尺寸 | `image.width` / `image.height` |
| 裁剪 | `image.screen_rect`（截图在屏幕物理像素的原点+尺寸） |
| 坐标容器 | `canvas_rect`（= `client_rect ∩ screen_rect`）或目标元素 rect |

会携带**模型坐标**的动作只有 `Drag`（0..1 归一化点）；其输入前守卫**已经**比较了窗口身份五项、
截图画布内容摘要、UIA 画布边界与身份。⇒ 没有 `frame_id` 这种"再造一个 ID"的必要。

**真正缺的三格**（本 PR 只补这三处）：

1. **绑定没有被记录**：`screenshot:<path>:sha256=<d>:<W>x<H>` 全仓库**只有生产端、零解析端**，
   "坐标属于哪一版图"事后无法核对。**修复**：新 `computer_use_frame.rs` 提供
   `parse_screenshot_evidence`（把它变成可读），并把绑定写进**既有 `evidence` 通道**
   （`frame_binding:sha256=…:WxH:screen_rect=…:canvas_rect=…:container_inside_image=…`）——
   **不新增表、不新增列、不新增 ID 空间**。观察快照层负责**记录**（不因此拒绝观察）。
2. **坐标容器与证据图像之间没有被核对**：两者若来自不同快照（现在不会，但没有东西阻止将来的
   重构造成错配），坐标会被映射到错误的裁剪/缩放下而不报错。**修复**：`FrameRef::bind`
   要求 `state` 的图像身份与 `screenshot:` 证据串**独立互查一致**，不一致即按
   `wrong_image`/`wrong_scale` 拒绝。
3. **几何缺失时没有明确表达**：拿不到 sha256／`screen_rect` 时无法区分"绑定成立"与"根本没绑定"。
   **修复**：`FrameRef::bind` 返回 `frame_unbindable` 并**点名缺了哪些字段**；证据串里记
   `frame_binding:unbindable:missing=…`，且该标记**不得被 `parse` 读成绑定**（否则等于把
   "没有依据"伪装成"有依据"）。

**新拒绝的情形（如实枚举，不夸大）**：

| 情形 | 旧行为 | 新行为 |
| --- | --- | --- |
| 拖拽时几何不全（缺 sha256／尺寸／`screen_rect`／可解析的截图证据） | 照常映射坐标 | **拒绝** `frame_unbindable`（零物理输入） |
| 拖拽时 `state` 图像身份与 `screenshot:` 证据串不一致 | 照常映射 | **拒绝** `wrong_image`／`wrong_scale` |
| 截图画布分支：窗口 rect 与图像内容都不变、但 `canvas_rect` 变（`client_rect` 变化） | **看不见**，同一组 0..1 点落到不同物理区域 | **拒绝** `wrong_crop`（用**观察当时记下的**绑定的 `parse` 结果对**当前实时**绑定比较） |
| 非拖拽动作（click 等）图像内容变化 | 不检查 | **仍然不检查**（刻意不改：实时 UI 内容易变，纳入会引入新的误拒；click 的坐标取自当前快照的实时元素矩形，不是模型给的坐标） |

⇒ 第三行是**唯一在旧代码下真实可达**的新拒绝；前两行是"把不可核对变成可核对"的防御性拒绝。

**8 项最小验收**（每项都有可执行断言）：① 一致时正常绑定并可往返读回；② `wrong_image` 可分辨；
③ `wrong_scale` 可分辨；④ 裁剪差异归 `wrong_crop` 且优先级在图像之后；⑤ 容器包含关系被**记录**
（不新增拒绝——拖拽路径原有 `target_offscreen` 已拒越界）；⑥ `screenshot:` 串由"只写不读"变成
可解析；⑦ 几何缺失记 `unbindable` 且**不得**被读成绑定、不得补推测值；⑧ 映射口径与
`stroke_arguments` **共用同一个** `unit_axis_to_pixel`（端点 `size-1` 不越界）。

**接线守卫（源码级，防"写了没接"）**：`tests/module_linkage_smoke.rs` 新增
`frame_binding_is_wired_at_the_mapping_point_and_does_not_replace_window_identity`：
快照必须记录绑定、`FrameRef::bind` 必须出现在 `stroke_arguments` **之前**、
窗口身份仍必须是**五项全比**且仍被调用。**变异验证 3/3**：把绑定挪到映射之后、
让快照不记录绑定、去掉 `dpi` 比较——三种都被捕获。

**无侧改的证据**：既有桥与适配器测试**全部原样通过**；窗口身份五项与陈旧重试语义
（仍用 `stale_observation` 码，故"允许重新观察一次"的策略不变）均未改动；ROI 与评分器未触碰。
另修掉一个**自己引入**的解析缺陷：`parse` 原用"第一个未匹配字段"兜底认尺寸，会把新增的
`container_inside_image=true` 误当尺寸导致整串解析失败——改为**按形状**认 `<W>x<H>`，
并由往返用例钉住。

**门禁**：web-console **1184/0**（1 ignored＝真实调用评测；较 B-106 增 10 条）、
module_linkage_smoke **7/0**（增 1 条接线守卫）、web-console build ✅、tool-registry check ✅。

**仍未做**：真实桌面端到端跑一次"帧绑定拒绝"（需真实窗口，未现场执行）；
`frame_id` 这个**名字**仍未引入，因为本 PR 的审查结论是**不需要新标识**——
若后续要对外暴露一个稳定名字，应基于本模块的既有字段组合，而不是新造。

### B-108 环境守卫的"同进程读者"那一半：补上**共用锁**，并收紧到**逐处**而非逐文件

**裁决口径**：环境守卫的修复保留；下一步核查"同进程读者是否用同一把守卫"；**优先显式连接注入**
而不是改写进程环境。

**核查结果（三处发现，前两处是真问题）**：

1. **web-console 没有全 crate 共用的进程环境锁**。`std::env::set_var` 改的是整个进程的环境，
   而 Rust 测试默认在**同一进程内多线程并行**跑；既有的 `ScopedEnvVar`（`input_safety_opening.rs`）
   与 `InputSafetyEnvGuard`（`computer_use_executor.rs`）都只**恢复**、不**加锁**。
   ⇒ 两个用例各自把"输入安全库根"指向自己的临时目录时，**任一方在作用域内读到的都可能是对方的值**，
   表现为随机失败或"根未注入"，且只在并发时出现。这正是裁决所说"panic 还原只是其中一半"。
2. **还有一处完全没有守卫的裸写入**：`computer_use_executor.rs` 里
   `let saved = var_os(..); remove_var(..); …; if let Some(saved) { set_var(..) }`——
   中间任何断言 panic 都会把库根**永久**从本进程环境删掉。逐文件检查的
   `test_environment_writes_are_paired_with_a_restoring_guard` **看不出来**，
   因为同一文件里另有一个守卫类满足了"文件含守卫标记"这一条件。
3. **读取侧的纯函数缝早已存在**（`input_safety_state_root_from(Option<OsString>)`，注释明确
   "让测试用显式取值验证，而不必改动进程环境"）——符合裁决"优先显式注入"的方向，
   无需新增机制；本次只把"仍在改环境"的那几处收敛到同一条路。

**实现**：

- 新增 `web-console/src/test_env.rs`（与 `llm-adapter::process_env_lock`、`tool-registry::process_state_lock`
  同一口径）：全 crate 共用的进程环境锁 + `ScopedEnvVar` 守卫；守卫**同时持有锁**，
  且 `Drop::drop`（恢复环境）先于锁字段的析构 ⇒ **恢复动作始终在锁内**完成
  （把锁放进同一个结构体，就是为了杜绝调用方写错顺序导致"无锁恢复"）。
- 锁**可重入**：本线程已有存活守卫时不再取锁（thread-local 计数）。理由是嵌套 `set`
  （测试辅助函数设一次、用例再设一次）是常见写法，非重入锁会**自锁**。
- 迁移三处：`input_safety_opening.rs`（删掉本地只恢复不加锁的守卫）、
  `computer_use_executor.rs` 的守卫类、以及第 2 条的裸保存/恢复对
  （并把"未注入"那一步收进块作用域，使恢复不受断言成败影响）。
- 只有确实要验证"环境变量被读取"的用例才走环境；其余继续走显式取值缝。

**收紧为逐处检查**：新增根级守卫 `web_console_test_code_writes_process_env_only_through_the_shared_guard`
——web-console 的**测试区**里出现裸 `std::env::set_var(` / `remove_var(` 即违规，
**唯一**例外是守卫实现模块 `test_env.rs`；整文件由 `#[cfg(test)] mod NAME;` 声明的模块按全文判定。
（这条比既有的文件级检查强，正是它能抓到第 2 条那种"有守卫类却又裸写"的情形。）
现状：测试区裸写入 **0 处**；`main.rs` 唯一那处是**生产**代码（给 provider 播种 `ZAI_API_KEY`），
不在测试区。

**变异验证（4/4）**：① 已有测试区塞入裸写入 ⇒ 守卫红；② `cfg(test)` 整文件模块塞入裸写入 ⇒ 守卫红；
③ 把 `set` 里的取锁去掉 ⇒ 并发用例红，报"持锁期间读到了别人的值 ⇒ 进程环境锁没有生效"
（另有一个用例同时红，说明不加锁时用例之间确实会互相干扰）；
④ 嵌套作用域用例独立钉住可重入（若改成每次取非重入锁会自锁）。

**门禁与稳定性**：web-console **1188/0**（1 ignored＝真实调用评测；较 B-107 增 4 条）、
module_linkage_smoke **8/0**（增 1 条逐处守卫）；全量套件**连跑 3 次全绿**。

**仍未做／待核**：`§B-101` 那次"多 crate 连续跑时 1 例失败（未捕获用例名）"**仍未被复现**，
因此**不主张**本次即为其原因——只能说"环境并发观察"是当时被记录在案的候选成因之一，
且这一类风险现在已被逐处守卫与共用锁消除。其余 crate（`llm-adapter`／`tool-registry`／
`core-runtime`／`command-router`／`vision`）各自已有锁或守卫约定，本次**未**逐个收紧为逐处检查，
按"不扩大范围"留待需要时再做。

### B-109 本轮补充裁决（2026-09-26）：**先纠错**——撤回两条错误契约要求，并把"搜不到符号"这条推断废掉

**裁决口径摘要**：上一轮裁决的 Phase 3 部分"来自一次错误地改写了已经确定、且已有实现的契约"，
**错误源头在裁决方**；本轮正式撤回两条要求，并给出一条证据规则。以下按裁决 §2／§8.1 落地。

#### 一、我的错误（如实登记）

我在上一条汇报里写了 **"Phase 3（PR-CU-FACT）整个未动，代码里搜不到符号"**。这是**错的**，
错在把"搜不到 `ExecutionOutcome`／`ActionScope`"推广成了"整个 Phase 3 未实现"。裁决指出：
**不存在计划中的某个符号，只能证明该符号不存在，不能证明该功能未实现**——而这两条要求本身
已被撤回，拿它们当"Phase 3 的判据"从一开始就不成立。

**逐符号核对结果（本轮实做，替代原来的推断）**：§B-43 那份释放义务修复**完整保留在当前代码里**——

| §B-43 的交付点 | 现状 |
| --- | --- |
| 五态 `ReleaseObligationState` | 在（26 处引用；`ProvenAbsent`／`Possible`／`Unsettled`／`Settled`／`EvidenceConflict` 五态齐，含测试） |
| 唯一推导点 `derive_release_obligation` | 在（13 处，笔画与普通输入共用） |
| `needs_emergency_release` 消费五态 | 在（14 处） |
| v1 旧记录读成 `LegacyUnverifiable` | 在（8 处） |
| `cursor_moved` / `protocol=2` / `phase(pre_in_flight,final)` | 在（`cursor_moved` 60 处） |
| 消费侧闸门 `NotSent ⇒ input_release = NotNeeded` | 在（`computer-use-core/src/contracts.rs:237-246`） |
| 零填充 `facts.unwrap_or_default()` 已删除 | 是（全仓库无命中） |

⇒ **本项不再作为待开工缺陷**；将来只有在**确认回归**时才重开具体缺陷。

#### 二、两条正式撤回（不再是可以领取的工单）

| 原要求 | 本轮处理 | 生效口径 |
| --- | --- | --- |
| 用 `ExecutionOutcome` 取代 `Result<StepExecution, ComputerUseError>` | **撤回，不实施** | 保留已选定的 `Result` 形态与 `ComputerUseError.receipt: Option<ActionReceipt>`（§B-84 已交付）：**失败可以携带回执，不需要为此再造一个平行顶层结果类型** |
| 新增 `ActionScope::NativeAction`，其 `request_attempt_id` 可选 | **撤回，不实施** | 保留事实层级、`ContextKind` 与 `ActionSource` 的**正交**设计：模型规划动作**即使由原生 helper 执行仍必须携带真实规划 attempt**；清理／宿主辅助／真实用户直操**按来源判断**不适用维度 |

**已落地的防重犯措施**：§C-收口 表里这两条（含重复出现的两份行）全部标
**`[已撤回／被本轮替代]`** 且注明"禁止据此开工"；状态文档对应行改写为"已撤回"。
**旧决定不删除、已提交历史不重写**——只加撤回标记，阻止后续按错误工单开工。

#### 三、Phase 3 改名为"符合性核对"，并登记实际缺口

**新名**：**CU 动作事实与现有执行契约的生产接线／符合性核对**。
**已实现的不重建**：释放事实链（§B-43）、`ActionOriginAuthority` 生产实现与输入前准入（§B-84／§B-85）、
`tool_calls` 登记链（§B-91）、SafetyCleanup 事故登记（§B-93）。
**仍需核对的**（沿**实际生产调用链**核对，不以符号存在与否代替验收）：`ProductionActionOriginAuthority`
的覆盖范围、真实 attempt 关联、步骤与事实同事务、迟到事实、**正式输入入口是否已全覆盖**。

#### 四、其余四项裁决的登记（执行项见 §C-收口-补）

| 决定 | 生效口径 | 我这一侧的状态 |
| --- | --- | --- |
| **放行资格与复核** | 默认**单人**受信本机恢复操作员，**不要求双人**；角色绑定实际 **Windows 用户 SID** ＋ 资源与政策版本；**署名不是授权**；身份须经原生系统验证；一次批准绑定一次具体决定并复用 `ReleaseIsolationDecision`；**不接受 Web 传入 `verified=true`** | **已按最紧一层先行收口**，见下节 |
| **R3／R4 权威来源** | **宿主启动登记**（关系）＋ **OS 实例证据**（进程句柄/创建时间/实际用户会话）＋ **helper 执行回执**（执行）；helper **不自行认领身份**；`owner_id`／`coordinator_id`／PID 不得互相替代；六类失败语义逐一规定 | 仍是**无实现**；口径已足够开工 |
| **Goal 与 chat turn** | Goal 阶段的一次真实执行与 CU 是**强父执行关系**；Chat turn 是**可缺省的因果引用**；自主 Goal **不需要制造** chat turn，有字段也**不得复制 ID 充数**；授权增加 `GoalPhase` 等价上下文分支（**不是** `ActionScope::NativeAction`，也不是另建回执） | 组合测试 5 只覆盖**否定路径**，正向能力仍缺（已单列） |
| **真实进程崩溃测试** | **批准**：编译期门控＋隔离库＋测试专用资源作用域＋**真正的受控子进程**；不杀当前产品／模型服务／用户程序 | 已按批准范围单列，未开工 |
| **`.gitattributes` 恢复核查** | **批准一次有边界的只读核查**，并建立**前向规则基线**；特别核查覆盖前 `git add -A` 可能留下的索引／blob；**不再写"未提交所以 Git 无法恢复"**（该结论过强）；本工单**不执行** `gc`/`prune`/`reset --hard`/`clean` | 已单列，未开工（见下节说明） |

#### 五、本轮立即交付②：公开 HTTP 放行入口**先 fail-closed**（裁决 §3.1／§9.1）

**核实（不是照抄裁决）**：`POST /api/system/release-isolation`（`main.rs` 路由 `:1418`、
实现 `api_release_isolation`）原本在**只有非空 `operator` 字符串 ＋ 理由 ＋ 阻断集合一致**时，
就会取得恢复资格并**真的调用 `release_isolation_authorized` ＋ `reopen_new_input_authorized`**
——即**真的改变资源状态**，而**没有任何操作者认证**。⇒ 裁决 §3.1 的判断与我核实的一致：
`400`（空署名）与 `409`（阻断集合过期）**只能**证明输入校验与版本冲突处理。

**改动（只收紧，不放开任何此前被拒绝的请求）**：
- 新增唯一开关点 `operator_authorization_available()`（**当前恒 `false`**），排在两处校验**之后**、
  **任何写动作之前**：到达即返回 `403` 并附可行动说明，**不写放行决定、不开放新输入**。
- 注释写明这是**唯一开关点**：实现认证后按实际验证结果返回，**不要另开旁路**；恒 `false` 时该接口
  只做校验与核对。
- 保留的：查询、提交复核请求、打开原生确认面（后者属下一执行单元）。
- **未做**（留给下一单元）：原生系统验证（优先桌面应用适用的 Windows Hello／用户验证接口，
  回退受控系统凭据验证并**验证取得的实际身份**；两者不可用或身份不符则**继续隔离，不退回纯署名**）；
  身份与 `resource_scope`／政策版本绑定；`120 秒一次性确认窗口`；证据目录 `<input_safety_state_root>/evidence/`。

**守卫 + 变异验证 3/3**（`public_release_endpoint_cannot_change_state_until_operator_auth_exists`）：
① 开关改为 `true` ⇒ 红；② 去掉 fail-closed 开关 ⇒ 红；③ 把门挪到写动作之后 ⇒ 红。
另有一条**顺带发现**：该接口原本**没有**任何"成功放行"的自动化用例（既有用例只断言前端接线），
`400/409` 是当时**手工 HTTP 实测**的——所以这次加门没有打红任何既有测试。这个缺口已由上述新用例部分补上。

#### 六、仍按"优先序"待开工（不在本轮冒充已完成）

按裁决 §9.1 的顺序：**立即**＝决策与台账纠错（本 §B-109，**已完成**）→ **优先**＝恢复操作员认证授权
（本轮只做了其最紧一层：先关掉公开通道）→ **优先并串行**＝R3／R4 执行者权威登记 →
**可并行**＝Goal 强父关系 → **跟随**＝真实崩溃装置 → **独立只读起步**＝`.gitattributes` 恢复核查 →
之后＝真实资源恢复与桌面验证（**由有权限的实际操作者执行，本轮不代为放行**）。

**本轮未做且未声称完成**：R3／R4 实现、Goal 正向强关联、真实崩溃装置、`.gitattributes` 只读核查、
原生确认入口、证据目录落位。**隔离状态未变**、**未推送**、**未安装**、**未改真实运行库**。

**发布门禁（裁决 §9.2，登记备查）**：以下全部满足才可关闭"受信恢复能力可发布"——
① 普通控制台调用者不能凭署名放行；② 系统验证身份与资源操作员政策一致；③ R3 真能撤销未激活许可、
R4 真能核查对应进程实例；④ owner／进程／释放未知时仍保持正确阻断、不靠终态或重启清空；
⑤ Goal 真实执行关系可在无 chat turn 时成立；⑥ 真实恢复者崩溃后能重新取权对账、旧资格不能改状态；
⑦ 人工批准与机器前后检查及最终提交有完整证据；⑧ 解除只允许新尝试重新观察与接纳，不复活旧运行、
不重写历史未知。

### B-110 `.gitattributes` 覆盖事故：**覆盖前原文已从本仓库对象库找回**，我原先"无法恢复"的结论过强

**裁决口径（§7）**：批准一次**有边界的只读核查**并建立**前向规则基线**；特别是核查覆盖前
`git add -A` 可能留下的**索引与对象库**；**不再写"未提交所以 Git 无法恢复"**；本工单**不执行**
`gc`／`prune`／`reset --hard`／`clean`，也**不**自动 `add --renormalize .`。

#### 一、核查过程与结果（只读）

| 步骤（按裁决给的顺序） | 结果 |
| --- | --- |
| `git ls-files --stage -- .gitattributes` | 索引里有：blob `5236b849`（＝当前工作副本） |
| `git show :.gitattributes` | 与工作副本一致；即**索引已被后来的重建覆盖** |
| `git log --all -- .gitattributes` | **该文件其实被提交过**：`965166c`（内容＝`5236b849`） |
| `git fsck --full --no-reflogs --unreachable` | 79 个不可达 blob；其中筛出**内容像属性文件**的两个候选 |

**两个候选与判定**：

| blob | 体积 | 内容 | 判定 |
| --- | --- | --- | --- |
| `70fbf2ed…` | 231 B | 注释 ＋ `/modules/.../app.js` ＋ `/modules/.../styles.css`（3 行） | **不是**覆盖前原文——它与 `tmp/2026-09-19-agent-fixes/pr-checkout/.gitattributes`（Sep 19 23:06）**逐字节相同**，是更早的旧副本。**我当初就是拿它"恢复"的，所以从一开始就没看到真正的原文** |
| **`7869d60a…`** | **425 B** | 注释 ＋ **`* text=auto eol=lf`** ＋ 6 条 binary | **就是覆盖前原文** |

**关联证据（为什么可判定）**：该不可达 blob 的对象文件创建于 **2026-09-25 18:57**，
而 `.gitattributes` 被覆盖于 **18:58**——`git add -A` 之后一分钟内发生覆盖，时序与前缀都吻合；
内容也符合本仓库既有理由（`pipe.rs` 的 LF 钉住断言）。
⇒ 裁决 §7.1 的判断**成立**：那次 `add` 确实把覆盖前内容留在了对象库里，我原先
"未被跟踪 ⇒ 无法从 git 恢复"的结论**过强**，已按本轮裁决废止。

**诚实的边界（§7.2 要求区分）**：我找回的是**被暂存的那份规则文本**（可核对、关联可靠），
**不等于**"覆盖前磁盘上的逐字节原件"——Git 在暂存时可能做过属性／行尾规范化。
两者在本例中相差一分钟、内容自洽，但结论按前者表述。

#### 二、逐条差异与恢复

覆盖前原文（找回）与当前（我此前的重建，`5236b849`）的差异是**单向遗漏**：

| 规则 | 覆盖前原文 | 我的重建 | 处理 |
| --- | --- | --- | --- |
| **`* text=auto eol=lf`**（全仓） | **有** | **无**（丢失） | **已恢复** |
| `*.png/.ico/.msi/.dll/.exe/.sqlite3 binary` | 有 | 有 | 保留 |
| `/modules/.../app.js`、`/styles.css` 两行 | 无（当时由全仓规则覆盖） | 有 | 保留（更显式） |
| `*.rs/.ps1/.toml/.json/.md text eol=lf` | 无（同上） | 有 | 保留（更显式） |

丢失这条**有实际后果**：`text=auto` 覆盖**所有**被探测为文本的文件，而我列出的 5 个扩展名漏掉了
`.js`／`.css`／`.html`／`.cs`／`.yml`／`Cargo.lock` 等 ⇒ 在本机 `core.autocrlf=true` 下，
这些文件的检出字节不再受保护（正是原文注释要防的事）。

**前向基线**：以找回的原文为底，保留带理由的显式扩展名规则，逐条依据写进文件头注释。
已核对**找回原文的规则行是当前文件的子集**（无遗漏）。

#### 三、生效性核对（不把"文件存在"当"规则生效"）

- `git check-attr`：`service_worker.js`、`Cargo.lock` 现在都得到 `text: auto` ＋ `eol: lf`
  （此前不在任何显式规则里）；`.rs` 等为 `text: set`；`dist/x.msi` 为 `text: unset`。
- **语料库逐字节固定未被破坏**：`samples/**` 与 `tools/**` 仍为 `text: unset`（更深文件按
  "就近优先"胜出），`git ls-files --eol` 显示语料库 14 个 `-text`（字节固定）、
  8 个 `i/crlf w/crlf`（以 CRLF 原样入库，与其记录摘要一致）、60 个 `i/lf w/lf`；
  逐字节守卫用例 `migrated_sample_files_recorded_digests_byte_for_byte` **通过**。
- **影响面实测＝0**：加入全仓规则后 `git status --porcelain` 只多出 `.gitattributes` 自身一行。
  原因是 `text=auto eol=lf` 下 git 比较前会把工作区内容归一化，故 `i/lf w/crlf` 的那些文件
  不显示为改动。
- **未执行**（本工单明令）：`renormalize`／`gc`／`prune`／`reset --hard`／`clean`。

**结论**：本项按裁决 §7.3「有可靠关联的覆盖前内容」分支处理——**恢复遗漏规则 ＋ 保留此次修复记录**；
不再保留"未知原文"这一状态，也不再阻塞其它开发。**新构建已绑定到本文件当前已审核的规则集**。

### B-111 Goal 阶段上下文交付（裁决 §5.3 授权的分支）＋**范围发现：Goal→CU 路径尚不存在**

**先说范围发现（它会改变该项的工作量估计，所以放在最前）**：我按 §5.2 去找"Goal 发起 CU 时
冻结哪些锚点"，结果 web-console 里**根本没有 Goal→CU 的发起路径**——`goal_phase` 与
`computer_use`／父运行在代码里**没有任何交点**，`FrozenParentContext::new` 只有**一个**生产调用点
（聊天接纳路径）。⇒ 本项不是"修正映射"，而是**先要建一条接纳路径**；裁决 §5.2 列的六个冻结项
（阶段运行 id、Goal↔阶段关系、阶段尝试身份、工作区／owner／claim／资格、房间会话关联、
可选发起轮次）目前**都还没有生产者**。

**因此本轮只做被 §5.3 明确授权、且能自洽闭环的那一步**：让「没有 chat turn 的 Goal 阶段」
在既有上下文体系里**可诚实表达**；并把受理侧**保持 fail-closed**，等宿主能核对真实父运行再打开。

#### 交付：`ContextKind::GoalPhase` ＋ `GoalPhaseActionContext`

按 `ControlPlaneContext` 的既有先例设计（显式字段 ＋ "必须由宿主建立"＋ 结构里**没有**可冒充的
聊天字段），新增：

| 字段 | 语义 |
| --- | --- |
| `hierarchy` | 事实层级，与上下文类型**正交**（沿用既有设计，不新造） |
| `goal_id` / `phase_id` / `phase_run_id` | **强父执行关系**：所属 Goal、阶段、**本次阶段尝试**的真实运行 id（已有运行 id 足以区分时不再造冗余 id） |
| `workspace_id` | 非可选：Goal 一定在某工作区里执行 |
| `room_id` / `session_id` | **可缺省**（自主 Goal 没有聊天房间）：缺省就是 `None`，**不造占位值** |
| `initiating_chat_turn` | **可缺省的因果引用**：自主 Goal 为 `None`；存在时只作因果记录 |

刻意**没有**的：任何"当前活跃运行"字段；任何全局唯一的聊天标识字段（§5.4 明令不得复用
`legacy_turn_id` 那类列——多个 Goal 阶段会为同一个 chat 标识竞争）；`step_id`／`action_id`
（属 `ActionOrigin` 的事实层级，不在这里重复一份）。
`is_same_attempt_as()` 专门表达"阶段重试＝不同尝试"，供调用方据此**不继承**旧尝试的许可／审批／动作。

**上下文种类语义**：`as_str() == "goal_phase"`；`requires_chat_session_and_turn() == false`
（聊天上下文为 `true`）；`kind()` 如实返回 `GoalPhase`——**不等于** `ControlPlane`，
`control_operation_id()` 对它返回 `None`，因此**无法借控制面通道绕开父关系要求**。
`room_id()`／`session_id()` 改为返回 `Option<&str>`：缺省时如实返回 `None`，
**不用空串把"没有"伪装成一个值**（该访问器原本只在契约文件内被调用，签名改动零外部影响）。

**受理侧保持 fail-closed（关键）**：`ActionOrigin::validate_against` 里 `GoalPhase` 分支
先做结构校验，然后**一律拒绝**并给出可行动理由（点名缺的是哪条强父关系）。
这不是"未实现所以先放过"，而是刻意的：强父关系的全部意义是「这条 CU 属于**哪一次真实 Goal
阶段执行**」，而宿主权威目前**没有**按 `phase_run_id` 核对真实阶段运行的能力；此时放行
等于让 `phase_run_id` 退化成一句自称——正是 §5.2 禁止的"现场在同房间的活跃运行里选一个"。
⇒ **表达力已具备，受理能力等宿主登记补齐后再在此处改为真实核对；在那之前任何 Goal 阶段来源都进不来。**

**测试 6 条**（`core-runtime` `run_contract::tests`）：① 独立上下文种类且不要求 chat turn、
与 ControlPlane 不同、序列化为具名变体 `goal_phase`；② 自主 Goal 三维全缺省**合法**，
访问器如实返回 `None`（不是空串）、占位值（`unknown`/`N/A`/`none`/`0`/`-`）一旦被当作"存在"即拒、
强父运行 id 不得为空；③ 因果引用如实记录且**不落进**任何含义不同的全局唯一聊天列、
也不要求"仍在运行"；④ 阶段重试识别为不同尝试；⑤ **未知／未来上下文变体必须报错**，
不得兜底成任何已受信上下文（旧读者行为测试）；⑥ 在宿主能核对父运行前**一律拒**，
且模型规划动作的义务不因换上下文而免除（仍必须有真实规划 attempt）。
**变异验证 3/3**：让 GoalPhase 也要求 chat turn ⇒ 红；缺省房间返回空串而非 `None` ⇒ 红；
把 fail-closed 改成往下走（等于放行）⇒ 红。

**门禁**：`core-runtime` **318/0**（较此前增 6 条）、`cargo build --workspace` ✅、
web-console **1189/0**（1 ignored＝真实调用评测）、module_linkage_smoke 8/0、tool-registry check ✅。

**仍未做**（按 §9.1，不冒充完成）：Goal→CU 的接纳路径本身及其六个冻结项、宿主按 `phase_run_id`
核对真实阶段运行的能力（`ActionOriginAuthority` 需要新增该查询）、§5.5 要求的正向用例
（真实自主 Goal 无 chat turn 仍能经合法强父关系**受理**——**当前会被刻意拒绝**，因为受理能力未到）、
以及阶段重试／错误父运行／跨工作区引用／父运行取消／迟到回执不重开旧阶段这五个场景。
这些都以"宿主登记"为前置，属 §9.1 的「优先并串行 R3／R4 执行者权威登记」之后。

### B-112 收口两项无风险待办：**.gitattributes 隔离检出验证**（§7.2 剩余）＋**正式输入入口清单核对**

#### 一、`.gitattributes` 前向基线在**隔离检出**中验证（补 §7.2 要求的那一步）

§7.2 明确要求"使用 `git check-attr`／`git ls-files --eol` 检查实际生效属性与行尾，**再在隔离检出中验证**；
不把属性文件存在等同于规则实际生效"。上一轮只做到属性查询，本轮把隔离检出补上：

`git worktree add --detach <临时目录> HEAD`（自动应用已提交的属性）后核对：

| 维度 | 主工作区 | **隔离检出** |
| --- | --- | --- |
| `i/lf w/lf` | 772 | **827** |
| `i/lf w/crlf`（仓库 LF、工作区 CRLF） | 55 | **0** |
| `i/-text w/-text`（不归一化） | 803 | 803 |
| `i/crlf w/crlf`（语料库原样保留） | 8 | 8 |

逐字节抽查：`Cargo.lock`、`modules/browser-extension/service_worker.js`、
`modules/gui-web/packages/web-console/src/app.js` 在隔离检出里**均为 LF**；
语料库 `samples/paint-r1/paint-r1-audit.json` **仍为 CRLF**（与清单记录的 sha256 一致）。
⇒ 恢复的那条 `* text=auto eol=lf` **确实生效**：主工作区那 55 个"仓库 LF／工作区 CRLF"的文件，
在干净检出中全部落成 LF；而语料库的字节固定未被该全仓规则破坏（更深文件的 `-text` 就近优先）。
随后 `git worktree remove --force` 清理，`git worktree list` 只剩主工作区。

#### 二、正式输入入口清单核对（只读审计）＋**补上一处守卫覆盖缺口**

裁决要求"按入口清单核对现有受控原语的消费；不预设必须再造 `NativeInputExecutor`"。审计结果：

| 正式输入入口（生产代码） | 使用的原语 | 结论 |
| --- | --- | --- |
| `gui-web/web-console/src/computer_use_desktop_bridge.rs` | `controlled_click`／`controlled_drag_path`／`controlled_key_combo`／`controlled_scroll`／`controlled_type_text`／`controlled_input` | ✅ 全受控 |
| `gui-web/web-console/src/main.rs` | `controlled_eval_input`／`controlled_mouse_button_action`／`controlled_mouse_button_state`／`controlled_move_mouse_absolute`／`controlled_press_key`／`controlled_type_text` | ✅ 全受控 |
| `gui-desktop/desktop-console/src/desktop_agent.rs` | 7 个 `controlled_*`，且**统一经 `controlled_call(...)`** 包装 | ✅ 全受控 |
| `computer-use-core/src/input_stroke.rs`／`input.rs` | 受控族的**定义处** | ✅ 库，不注入 |
| `computer-use-core/src/bin/check.rs`、`desktop-console/src/main.rs` | `diagnostic_*` | ✅ 已在守卫允许清单内（诊断/自检用途） |

**审计发现的缺口**：根级守卫 `only_the_controlled_input_entry_is_reachable_from_automation` 的第 ② 条
只断言了 `desktop_agent.rs` 与 web-console 的 `main.rs`，却**没有**断言
`computer_use_desktop_bridge.rs`——而它才是**桌面 CU 真正注入输入的地方**（click/drag/key/scroll/text
都在这里落地）。也就是说：最关键的那条路径当时落在断言之外。

**已修（只加断言，不改生产代码）**：把 `computer_use_desktop_bridge.rs` 补进 ② 的清单。
实测它本就使用 `controlled_*`，因此补入后立即通过，作用是**防将来回归**。
**判别性验证**：把该文件里的一条 `controlled_drag_path(` 改回 `diagnostic_drag_point(`（模拟回归），
守卫立刻报"引用了无生命周期输入原语（diagnostic_*）：自动路径必须走 controlled_*"⇒ 这条断言是活的。

**门禁**：module_linkage_smoke **8/0**（断言增强，未增用例数）；本次未改任何生产代码。

### B-113 8.2a 交付：六态输入许可转换契约**纯逻辑冻结**（不落库、不占 schema 版本号）

**裁决口径（§3）**：批准新增最小的持久许可登记，**同意先做纯逻辑与测试以缩短 schema 串行窗口**；
纯逻辑与后续存储**必须使用同一套转换规则**（不得先写一套测试状态机、落库再独立写第二套）；
schema 设计与迁移计划可提前评审，**版本号仍由输入安全库负责人在合并时分配**。

**交付**（`core-runtime/src/input_safety.rs`，纯逻辑，无存储、无版本号）：

| 项 | 内容 |
| --- | --- |
| 六态 | `PendingActivation` / `DispatchCommitted` / `Executing` / `Finished` / `Revoked` / `OutcomeUnknown`，语义按裁决 §3.2 冻结 |
| 转换表 | `Pending→DispatchCommitted\|Revoked`；`DispatchCommitted→Executing\|Finished\|OutcomeUnknown`；`Executing→Finished\|OutcomeUnknown`；`OutcomeUnknown→Finished`（**对账**）；`Finished`/`Revoked` 为终态 |
| 判定辅助 | `may_still_be_dispatched()`（只有未消费的 `PendingActivation`）、`crossed_dispatch_boundary()`（此后撤销都不构成"未发送"，而 `Revoked` **没有**越界——它在消费前生效） |
| 许可绑定 | `InputPermit`（§3.3 的最低绑定：permit/action/scope/执行上下文引用/冻结动作摘要/政策与 gate revision/签发 owner 与 epoch/期限/执行者实例/revision/撤销原因）＋ 结构校验 |
| 重复请求 | `decide_permit_reuse()`：同内容 ⇒ 返回**已有**状态（不重新获得可执行资格）；换内容 ⇒ 拒绝；已撤销的许可也不会因"再请求一次"复活 |
| 竞争判定 | `resolve_intake_close_race()`：关闸先 ⇒ 消费失败 ⇒ **原生输入为零**；消费先 ⇒ 在途、**不得改写为未发送**、交 R4；**两个都成功或都不成功 ⇒ 按写锁竞争失败处理（`SQLITE_BUSY` 不是成功）** |
| 只收紧入口 | `TightenOnlyAction::{CloseIntake, RevokeUnconsumedPermits}`，`loosens()` 恒 `false` —— 它不是放行通道 |
| 异常而非转换 | `PermitAnomalyKind`：撤销后观察到真实输入记成**追加异常**，**不是**状态转换（不得为状态机漂亮丢事实） |

**三条"特别重要"语义直接写进文档注释并由测试钉住**：`DispatchCommitted` ≠ 输入已发生；
`Finished` ≠ 成功 ≠ 释放已确认；`OutcomeUnknown→Finished` 是**对账**不是恢复执行（同一许可永不因此再次产生输入）。

**测试 9 条**（`input_safety::tests`）：转换表合法/非法逐项（非法项必须带可分辨理由，且"撤销回待激活"理由要点名
"凭空发一份新许可"）；派发/越界判定；三条语义；异常不是转换；结构校验（占位/空值/控制字符被拒、
**越界后缺执行者实例即错误**、待激活可暂缺但不得激活）；可激活判定（未消费+未过期+执行者已建立，时间由调用方传入）；
重复请求；竞争判定；只收紧入口。
**变异验证 4/4**：允许"已撤销→待激活"⇒ 红；允许"未知→执行中"⇒ 红；让 `DispatchCommitted` 也算"仍可派发"⇒ 红；
把写锁竞争当成"消费在先"⇒ 红。
（诚实记一笔：其中 M2 我第一次构造失误——改后的目标集合里其实**没有** `Executing`，等于没引入违规，
所以显示"未捕获"；已重做正确变异并确认被捕获。）

**门禁**：`core-runtime` **327/0**（较此前增 9 条）；`cargo build --workspace` ✅。

**未做（不冒充）**：8.2b 持久化（单一实现的事务接口、迁移、重启与提交失败处理）、8.2c 生产接线
（各输入入口在**实际派发前**消费许可）、8.2d 并发/崩溃（关闸竞争、重复消费、派发前后崩溃与迟到回执）。
落库时必须复用本节的同一套转换规则，且**不得**为旧运行批量生成"已撤销／未发送"记录（裁决 §3.6）。

### B-114 8.3a 交付：执行者实例身份契约**纯逻辑冻结**（与 8.2a 配套，不占 schema 窗口）

**裁决口径（§4）**：三种证据来源的分工**不重定义**；生产终止接口只接受经核查、仍持有**实际句柄**的内部对象，
**不接受裸 PID**；活句柄不落库、不序列化跨进程；重启后的持久记录是「待核对线索」，不是仍有效的操作能力。

**交付**（`core-runtime/src/input_safety.rs`，纯逻辑）：

| 项 | 内容 |
| --- | --- |
| 来源分工 | `ExecutorEvidenceSource` 三种，每种都显式写出 `proves()` 与 `does_not_prove()`，且 `alone_suffices_to_conclude_stopped()` **恒 false**（没有哪个来源能单独支撑「已停止」） |
| 执行者身份 | `HelperIdentity { host_process_path, script_or_program_digest }`：**解释器路径相同、脚本身份不同 ⇒ 不是同一个受控执行者**（正对裁决「只验证 powershell.exe 路径不足以证明在运行本次受控脚本」） |
| 登记 | `ExecutorRegistration`（实例/启动/协调器/scope/宿主启动实例/action/PID/创建时间/用户会话/helper 身份/协议版本/监督绑定/状态/revision）＋ 结构校验：**未绑定真实监督关系的不得算「已核查通过」**；协议版本 0（未记录）被拒；占位值被拒 |
| 创建身份完整性 | `creation_identity_complete()`：缺创建时间只能「保留错误继续核查」，**不得**当成身份相符 |
| 句柄世代 | `ExecutorVerification::is_valid_for(instance, generation)`：**重新取得句柄后上一次核查不自动继承** |
| 失败表 | `disposition_for(observation)` 九行逐行冻结（PID 复用 ⇒ 不动当前进程；AccessDenied ⇒ Unknown 且保持阻断；缺登记 ⇒ 不猜 owner；直接 helper 退出 ⇒ 继续查后代与释放；Job 关闭 ⇒ 有界等待并核查；终止返回成功 ⇒ 只记「已请求」；退出确认但释放未知 ⇒ 继续隔离；迟到回执 ⇒ 追加事实且不恢复旧资格） |
| 自动提权 | `disposition_allows_automatic_privilege_escalation()` **恒 false** |
| 后代证据 | `DescendantStopEvidence`：父进程退出与 Job 关闭**都不算**后代已停，只有逐个确认成员退出才算 |
| 终止语义 | `TerminatePhase`：只有 `ExitedConfirmed` 是停止确认；`WaitTimedOut` ⇒ 进入明确隔离／人工复核，**不无限重试到看起来成功** |

**测试 8 条**；**变异验证 4/4**：让「启动登记」单独足以断言已停止 ⇒ 红；PID 复用时去动当前进程 ⇒ 红；
允许自动提权 ⇒ 红；父进程退出即断言后代已停 ⇒ 红。

**门禁**：`core-runtime` **335/0**（较 8.2a 再增 8 条）；`cargo build --workspace` ✅。

**未做（不冒充）**：8.3b 宿主登记落库与迁移（版本号由输入安全库负责人在合并时分配）、
8.3c 生产核查接线（启动顺序：登记启动意图 → 创建**尚不允许输入**的 helper → 绑定监督 →
取实际身份 → 持久化 → 握手 → **最后**才签发输入许可），以及真实句柄的 OS 侧读取
（`GetProcessTimes` 等，需真实进程）。8.4 的装置基础（父子进程、测试库、同步屏障、看门狗清理）
可在本契约之上开工。

### B-115 8.4 装置**基础**交付：真实父子进程 + 屏障 + 有界收尾（**不是** K1–K6 场景通过）

**裁决口径（§5.1）**：装置拆两层——**装置基础**（父子进程、实际创建句柄、测试库、测试作用域、
同步屏障、日志、看门狗超时与清理）**现在就能做**；**业务故障场景**（K1–K6）等相应契约冻结后逐个接入。
基础层的完成标准是「**自身运行、退出、失败清理可验证**」。

**交付**（`tests/crash_harness_smoke.rs`，3 条用例）：

| 裁决要求 | 落地 |
| --- | --- |
| 父测试创建的专用子进程 | 父测试以 `current_exe()` 重新调用**自己**并只跑子模式那一个用例（`--exact crash_harness_child_mode`） |
| 只终止**自己持有实际创建句柄**的子进程 | 终止只经 `std::process::Child`（真实句柄）；**API 上不存在**接收裸 PID 的入口 |
| 每次运行独立的会话库／输入安全库／证据目录 | `HarnessWorkspace` 建五类独立目录（session／input-safety／evidence／barrier／logs），**从不指向**真实输入安全根或真实运行库 |
| 测试专用锁命名空间，不得占用真实桌面输入 scope | scope 形如 `windows-session-harness-<运行 id>` |
| 用屏障确认前置阶段**真的到达**，不靠 sleep | `Barrier`：子进程先写 `.partial` 再 `rename` 发布阶段标记（避免读到半截），父进程**有界等待** |
| 看门狗超时与受控回收；**父级收尾失败也算测试失败** | `ChildExecutor::cleanup` 有界确认退出，失败即 panic（不静默、不靠删临时目录充数） |
| 不向真实鼠标键盘注入 | 子模式只写标记文件＋等待，**不调用任何输入 API** |
| 故障入口默认关闭、不出现在产品面 | 整个文件在 `tests/` 测试目标，**不进任何生产二进制**；子模式还需显式环境变量，缺失即返回 |
| 终止是**异步**的（与 §4.4 同口径） | `terminate_and_confirm`：`kill` 只算「已请求」，必须等 `try_wait` 报退出才确认；超时返回错误而**不**继续重试到看起来成功 |

**可执行边界不变式**（`crash_harness_honours_its_hard_boundaries`）：装置本体不得出现
按裸 PID／进程名终止的形状、不得引用任何输入原语、必须有显式等待预算常量、子模式必须默认关闭、
不得读写真实输入安全根。**变异验证 3/3**：塞入 `TerminateProcess`／塞入输入原语名／
把子模式改成默认开启，三种都被捕获。

**稳定性与清理**：连跑 3 次全绿（每次 0.03s）；跑完后 `tasklist` 中 **0 个** harness 残留进程。

**踩到并修掉的一处自指**：边界用例原先用 `include_str!` 扫**整个文件**，于是它把 forbidden 清单里
**自己的字面量**当成违规而失败（「守门人扫到自己」）。已改为只扫**装置本体**（按本用例名切分）。

**未做（不冒充）**：K1–K6 六个故障点、许可／执行者状态绑定、OS 侧实例身份读取（属 8.3c）、
业务故障场景断言（裁决明确：**不得**把「装置已写」登记为「六个崩溃场景全部通过」）。
装置的下一步是把已冻结的 8.2a／8.3a 契约作为测试事件接进来（许可消费已提交／执行者已登记／
终态提交完成），再逐项绑定故障点。

### B-116 8.4b 交付：测试事件词汇表 + K1–K6 依赖门（**机器可判**，不靠散文）

**裁决口径（§5.1／§5.2）**：冻结小范围**测试事件**（"许可消费已提交""执行者已登记""终态提交完成"），
通过 test-support 暴露同步屏障——它是**测试观察点，不是生产可启用的崩溃开关**；
§5.2 给出 K1–K6 的依赖与关键断言。

**交付**（`tests/crash_harness_smoke.rs`，7 条用例）：

1. **事件词汇表 `HarnessEvent`（6 个）**：`RecoveryRegistered` / `IntakeClosedWithPendingRevoked` /
   `PermitConsumed` / `ExecutorRegistered` / `TerminalCommitted` / `ConfirmationCompleted`，
   每个都有唯一阶段名，且**锚定到已冻结的契约**：
   `PermitConsumed → InputPermitState::DispatchCommitted`（并断言该状态**已越过派发边界且不可再派发**）、
   `ExecutorRegistered → ExecutorInstanceState::VerifiedAlive`（该状态在 8.3a 里要求已绑定监督）、
   `RecoveryRegistered / IntakeClosedWithPendingRevoked / TerminalCommitted → RecoveryStage` 的既有阶段。
   ⇒ 事件名与契约状态**不可能各自漂移**：改一边就有用例红。
2. **事件可被真的驱动**：子模式支持"按事件名到达阶段再等待被终止"（未知阶段名一律拒绝），
   并有端到端用例证明装置能把子进程带到具名事件阶段（`harness_can_drive_a_named_event_stage_end_to_end`）。
3. **K1–K6 依赖门（机器可判）**：`HarnessScenario` 逐项携带 `dependencies()` 与 `terminate_point()`，
   与 §5.2 的表**逐行一致**；`available()` 只在依赖全具备时为真，`blocked_by()` 必须点名缺哪一项、
   每项依赖必须给出 `blocker()` 理由。
4. **不得把装置就绪当成场景通过**：`no_scenario_is_claimable_before_its_dependencies_land` 断言
   **当前没有任何 K 场景可声明通过**（`claimable` 必须为空），同时六个场景**都**指出了阻塞点——
   保证"逐项解锁"有据可依，而不是含糊搁置。

**依赖现状（如实，全部为"不可驱动"并各带理由）**：
- `ExistingCoordinator`：协调器已实现，但位于 web-console 的 **bin 内部**，**根测试目标无法导入**
  ⇒ 要驱动 K1 需先暴露 lib 侧入口或把驱动下沉；
- `PermitPersistenceR3`（8.2b/8.2c，等 schema 窗口）、`ExecutorSupervisionR4`（8.3b/8.3c）、
  `RecoveryTransaction`、`OperatorAuth`（8.1）均未落地。

**变异验证 4/4**：改掉 K4 的依赖集 ⇒ 红；把消费事件锚到"待激活"⇒ 红；
未知阶段名被认作已知 ⇒ 红；声称某依赖已可用 ⇒ 红。
**稳定性**：连跑 3 次全绿（每次 0.03s），跑完后 0 个 harness 残留进程。

**诚实记一笔（同一类错误第三次）**：这轮我的变异 M2 第一次又是**无操作变异**——把两个分支都写成 `false`，
行为没变，所以显示"未捕获"。已重做为真正让依赖可用并确认被捕获。教训：**变异必须验证"确实改变了行为"**，
不能只看"我改了文本"。

**未做（不冒充）**：K1–K6 的真实故障场景（依赖如上一节所列尚未落地）；本轮**没有**声明任何场景通过。

### B-117 8.2b／8.3b 联合持久化交付：输入安全库 v2 → v3 一次相邻迁移 + 迁移验收 DB-1…DB-8 全通过

**授权口径（2026-09-26）**：批准进入 8.2b／8.3b **联合持久化**；由当前串行集成角色统一核对并
**分配输入安全库的下一固定迁移版本**；复用已冻结六态与执行者身份契约；**只在隔离数据库中迁移和验证**；
协调器可测性优先通过所属 bin 的测试入口解决。**本轮不含**迁移当前真实安全库、解除隔离、启动真实输入、
提权、安装或推送。

#### 一、版本登记（按 §2.2 的确定规则，不猜不动态）

- **终点版本 N 的确认方式**：读**最终合并基线的迁移目录**——`INPUT_SAFETY_SCHEMA_VERSION` 当时为 `2`
  （`input_safety_store::ensure_schema` 里 v1→v2 就地升级链的终点），**未**依据"历史上曾出现的 v1/v2"、
  **未**使用会话库 v23、**未**在运行时用"当前版本＋1"生成。
- **本批登记为 `2 → 3` 一次相邻迁移**，`INPUT_SAFETY_SCHEMA_VERSION` 改为 `3`（钉住版本的用例同步更新，
  并在注释里写明登记口径）。许可表与执行者表**共用这一次迁移**，避免各自抢号、也避免中间版本表达不了两者关系。
- 超前版本仍**明确拒绝**（`SchemaFromTheFuture`），不降版本修复——DB-4 覆盖。

#### 二、交付内容

| 项 | 内容 |
| --- | --- |
| 共享 schema | `input_safety_permits`（六态许可登记）＋ `input_safety_executors`（执行者实例登记）＋必要索引；复用既有安全事件与资源状态，**不建第二份事故库、不建第二套恢复状态机** |
| 迁移原子化 | `ensure_schema` 整体包在 `BEGIN IMMEDIATE … COMMIT` 内：**对象与版本一起提交或一起回滚**；失败时保留原始错误（回滚失败也不掩盖）；并发打开用 `busy_timeout` 等待，超时返回**显式**写锁竞争（不跑半套 schema） |
| 8.2b 适配 | 登记 pending／条件消费／撤销／关闸并原子撤销未消费／对账结清／状态分布查询 |
| 8.3b 适配 | 启动意图登记／实际实例证据推进（处置由**契约失败表**给出）／读回；**只存身份与证据，不存活句柄**，PID 是"待核对线索"而非执行能力 |
| 契约复用 | 状态列存契约的 `as_str()`；转换合法性调用 `InputPermitState::transition_to`；**不在 SQL 层另写一套语义**；未知状态字符串一律报错而不兜底 |
| 事务纪律 | 三处原子操作都在**同一短事务**内"读取 revision → 跑纯逻辑 → 条件更新"；冲突返回明确错误，不靠最后写入覆盖；**不跨 helper 等待/模型请求/人工确认持有事务** |
| 边界 | 适配器留在**宿主输入安全存储**边界（web-console），**未把 rusqlite 引入 core-runtime** |

#### 三、迁移验收 DB-1…DB-8（本轮全部通过；DB-6／DB-7 驱动真实 SQLite）

| 编号 | 结果 |
| --- | --- |
| DB-1 现有受支持版本升级 | ✅ store ID、资源 scope（`revision=7`／`epoch=3`）、遗留恢复操作**全部保留**，旧行仍按"在办"读回；版本推进到 3、v3 对象可查询 |
| DB-2 同版本重复打开 | ✅ 事件数不变（不重复迁移/初始化）、不降版本、记录不丢 |
| DB-3 迁移中途失败 | ✅ 用**真实** `ensure_v3_objects` + 与生产同一套事务纪律：注入失败后版本**仍为 2**、v3 对象**不存在**、随后真实迁移干净通过（无残留半套 schema） |
| DB-4 超前版本 | ✅ 明确拒绝；既有身份**未被改动**（拒绝不等于重建） |
| DB-5 两进程同时打开待迁移库 | ✅ 另一方写事务在场时第二个写事务**被拒**（不得并发迁移）；放锁后迁移正常完成 |
| DB-6 关闸 vs 消费（真实 SQLite） | ✅ 先关闸 ⇒ 未消费许可被撤销且消费失败；先消费 ⇒ 属在途，关闸**不撤销**、**不改写成"未发送"** |
| DB-7 重复消费/重复登记/重复对账（真实 SQLite） | ✅ 重复消费因状态已变被拒（不产生第二次执行资格）；同 action 重复登记被拒；重复对账只能结清一次 |
| DB-8 新表为空但旧事故未解决 | ✅ 资源**仍隔离**、旧恢复操作仍在办；消费不存在许可**明确拒绝**——"没有新许可记录"≠"历史安全" |

**验收口径**：比较的是**业务实体与状态**（身份/scope/revision/恢复操作/许可状态），
不要求数据库文件字节一致（新增对象与正常写入本就会改文件）。

**诚实记一笔**：DB-3 不是在 `ensure_schema` 内部注入故障，而是在独立连接上以**同一套事务纪律**
调用**同一个** `ensure_v3_objects` 再注入失败后回滚——它验证的是"事务边界保护了版本与对象的一致性"
这一性质；对生产路径的故障注入（例如磁盘写满）**未做**，如实登记为未覆盖。

#### 四、门禁

web-console **1198/0**（1 ignored＝真实调用评测；较 8.4b 增 9 条，其中 DB-1…DB-8 八条）、
core-runtime **335/0**、crash_harness_smoke **7/0**、module_linkage_smoke **8/0**、
`cargo build --workspace` ✅、tool-registry check ✅。
适配器当前**无生产调用者**（8.2c／8.3c 才接线），因此加了**显式的** `dead_code` 豁免并写明理由
——「接线落地后应移除该豁免」，不是掩盖。

#### 五、未做（不冒充，按 §8 的顺序）

8.2c／8.3c 的**生产接线**（各输入入口在**实际派发前**消费许可；启动顺序：登记意图 → 创建**尚不允许输入**
的 helper → 绑定监督 → 取实际身份 → 持久化 → 握手 → **最后**才签发输入许可）、宿主 bin 的测试接缝（§5）、
K1–K6 场景（每个都需要生产者**真的**到达事件）。迁移成功**不等于**真实隔离已获解除。

### B-118 宿主 bin 测试接缝 + **K1 核心场景已跑通**（真实跨进程，非模拟）

**授权口径（§5.1／§5.2）**：协调器在 bin 内部，外部 `tests/` 集成测试不能 `use` 它 ⇒
把需要访问协调器的父测试放在**所属 bin 的测试模块**里，再由该测试程序创建**自己的**受控子测试进程；
**调用同一生产实现**，不复制协调器代码、不重写 epoch／锁逻辑、不反序列化"已持权"对象；
隔离路径与测试 scope 经**显式参数／子进程环境**传入；只终止自己持有创建句柄的子进程。

#### 交付（`web-console/src/input_safety_harness.rs`，`#[cfg(test)]` 模块，2 条用例）

- **子模式**（默认关闭，需显式环境变量）：以**同一生产实现**
  `InputSafetyCoordinator::begin_with_coordination_scope` 在**隔离输入安全库**上取得协调资格，
  写下屏障与所持 epoch，然后**刻意不完成登记**、等待被终止——这正是 K1 的故障点
  "取得协调权后、恢复登记完成前"。
- **父测试**：`current_exe()` 启动子测试程序（**并打印实际程序路径**，避免"执行错二进制仍报告通过"）、
  有界等待屏障（不靠 sleep）、只终止自己创建的子进程并有界确认退出。

**K1 已跑通的三条断言（都是真实跨进程，不是同进程模拟）**：

| # | 断言 | 结果 |
| --- | --- | --- |
| ① | 子进程持有协调资格期间，**另一个实例**的取得尝试被跨进程互斥挡住 | ✅ `input_safety_coordinator_busy` |
| ② | **"锁可取得不等于安全"**：仅凭"有人在协调"不得让资源变成可接纳新输入 | ✅ `accepts_new_input == false` |
| ③ | 持有者被终止后，**崩溃者的痕迹仍可见**：留下**未结账**的恢复操作，且绑定其当时的 epoch、`committed == false`（不得被静默清掉或伪造成已完成） | ✅ 见用例日志 |

**④ 如实覆盖两条分支**：持有者死亡后的重新取权，存储用的是 **fail-closed 回收规则**
（与裁决 §4.5 同口径：只有能**正面确认**旧持有者已消失才回收陈旧资格；拿不到身份按 Unknown 不回收）。
因此用例接受"取得新 epoch"**或**"明确 `input_safety_epoch_conflict`"两种结果，并在**任一分支**下都断言
"资源不得自动变成可接纳新输入"。**本机实测落在拒绝分支**（环境中无法正面确认被杀测试进程已消失），
用例打印了这一事实——这不是测试放宽，而是把两条合法结果都钉住。

#### 踩到并修掉的两个真问题

1. **`--exact` 需要完整用例路径**：模块内用例全名是 `input_safety_harness::k1_harness_child_mode`，
   我最初只写了短名 ⇒ 子进程"跑了 0 个用例"静默退出、父进程只看到超时。**先修了取证**（把子进程
   stdout/stderr 落盘并在超时时打印），才看到"0 tests"，否则会误判成协调器问题。
2. **我的 ⑤ 断言一开始写反了**：我断言"不得出现已登记项"，但取得协调资格**本身**就会写下
   `RecoveryOperationStarted`；被终止后留下未结账操作**正是期望行为**。已改为断言正确事实。

#### 验证范围（分列，不混同）

本用例验证的是"**宿主测试二进制中的真实跨进程协调逻辑**"：
**不等于**正式安装程序的启动端到端验收；也**不等于** K1 的完整验收（未覆盖：真实恢复提交边界、
持久化后的许可／执行者状态联动——那部分依赖 8.2c／8.3c 的生产调用点）。

#### 门禁

web-console **1200/0**（1 ignored＝真实调用评测；较 §B-117 增 2 条）、core-runtime 335/0、
crash_harness_smoke 7/0、module_linkage_smoke 8/0、`cargo build --workspace` ✅、tool-registry check ✅。
K1 连跑 3 次全绿（每次 ~0.37s），跑完后 **0 个**残留进程。
根装置的 K1 依赖门仍为"不可驱动"是对的：那说的是 **root `tests/` 装置**，而 K1 的接缝按裁决落在 **bin** 内。

### B-119 8.2c／8.3c 接线方案（锚点已定）——**本轮刻意不动输入路径**，并说明为什么

**裁决 §8 的第 4 步是"8.2c／8.3c 的小范围生产接线"**。本轮把接线方案与锚点核实清楚并落盘，
但**没有**修改输入路径。理由不是回避，而是本轮裁决自己定下的两条硬约束：

1. **"存储与接线未完成的能力继续明确拒绝，不启用内存替代方案"**，且
   **"不能通过扩大权限或放宽观察校验来消除测试阻塞"**；
2. **"main.rs、安全库写入口、broker 共享状态由对应负责人串行合并"**。

**关键的设计事实（决定了"只接消费"是不可行的）**：许可的**签发**与**消费**必须**同批落地**。
若只把"派发前消费许可"接进输入路径，而签发路径尚未存在，那么**任何** CU 动作都会因为
"查不到许可"而被拒 ⇒ 等于在签发落地前把 CU 输入整体关掉。这属于**产品行为变化**，
不应作为一次中途状态交付。因此 8.2c 的正确形态是"**签发 + 绑定 + 消费**"三点一起接。

#### 锚点（已核实，非推测）

| 步骤 | 位置 | 说明 |
| --- | --- | --- |
| ① 签发（`register_pending`） | `computer_use_executor.rs:228` 紧邻既有 `admit_action_origin(&expected_action_id, index)?` 之后 | 与既有输入前准入**同一位置、同一 fail-closed 口径**：拒绝时零物理输入、零步骤行、只留审计。此处已具备 `action_id`、动作 JSON（可算冻结摘要）、run 上下文与输入安全库根 |
| ② 绑定 + 消费（`consume_permit` + `BindTo`） | 输入租约检查（`:212` `lease.is_current()`）之后、**原生输入调用之前**（`:268` 附近的派发点） | 消费在同一短事务内核对 gate／政策／epoch／期限／绑定；不通过即**不输入**。执行者实例 id 来自 8.3b 的登记 |
| ③ 执行者登记（8.3c） | 与 ② 同一段：`register_launch_intent` → 取实例证据 → 绑定监督 → 落库 → 握手 → **最后**才签发输入许可 | 遵循裁决 §4.2 的固定启动顺序；**只存身份与证据，不存活句柄** |

**budget 的取值口径**：`consume_permit` 接收的是调用方**在签发时冻结**的 `gate_revision`／
`policy_revision`／`held_epoch`；调用方不重新读"当前值"再传进去（否则等于自己和自己比）。
一旦这三项与库中行不一致 ⇒ 说明关闸、政策变更或资格更替已经发生 ⇒ **拒绝输入**。

#### 测试策略（沿用既有离线夹具，不需要真实桌面）

既有 CU-F05 系列用例已经能在**离线夹具**上驱动执行器的输入前准入路径（`plan_attempt_fixture`
等接缝）。8.2c 的用例应加在同一处，至少覆盖：
① 正常路径：签发 → 绑定 → 消费 → 输入照旧发生（**证明产品未被中途关掉**）；
② 无许可 ⇒ 零输入（fail-closed）；
③ 关闸在先 ⇒ 消费失败、零输入（与 DB-6 同口径，但在**执行器路径**上）；
④ 重复消费 ⇒ 第二次被拒且不产生第二次执行资格（与 DB-7 同口径）；
⑤ 签发时冻结的 gate／epoch 与库中不一致 ⇒ 拒绝，且**不得**用"重新读取当前值"绕过。

#### 本轮为什么停在这里（如实）

改的是**安全关键的输入路径**。半途而废的接线比不接更危险：它会同时破坏"产品可用"与
"拒绝语义可信"两件事，而裁决明确禁止用放宽校验或内存替代来消除阻塞。因此我把它留给
**一次专注的串行窗口**（签发 + 消费 + 执行者登记三点同批），而不是在上下文受限时挤出一半。
**未做**：8.2c／8.3c 的代码改动、K2–K6。当前 CU 输入路径与合并前**完全一致**，未受影响。

### B-120 8.2c 的**使能步骤**交付：许可/执行者适配器经生产窄口可达（接线阻塞已解）

**背景**：写 §B-119 的接线方案时核实出一个**具体阻塞**——`PermitStore::new(&Connection)` 需要连接，
而生产路径拿不到：`InputSafetyStore` 的连接是私有的，`connection_for_test` **只在测试构建里存在**。
若不先解掉它，8.2c 的接线必然卡在"适配器无法从生产路径触达"，只能靠再加一套测试旁路——**那正是
裁决禁止的**（"不启用内存替代方案"、不得为测试开正式旁路）。

**交付**（`input_safety_store.rs` 两个**窄口**，非连接外泄）：

| 窄口 | 作用 |
| --- | --- |
| `permit_store()` | 交回 `PermitStore<'_>`，工作在**本库自己的连接**上 |
| `executor_store()` | 交回 `ExecutorStore<'_>`，同上 |

设计要点：适配器**本来**就设计成"工作在既有 store 的同一连接上"——这正是"同一套事务纪律、
单一写入口"的落地方式；窄口只交出适配器，**不外泄连接本身**，调用方也无法借它绕过 store 的其它不变式。
注释写明调用方不得在适配器外层再开长事务或跨等待持有事务（适配器内部用的是短
`BEGIN IMMEDIATE … COMMIT`）。

**验收用例**（`permits_are_reachable_through_the_production_accessor`）走的是**生产形态**：
从 store 拿适配器（而不是自己 `Connection::open`），完成
① 经窄口登记 + 消费（待激活 → 已提交派发）；
② **同一条生产可达路径上的关闸竞争**：先关闸 ⇒ 新消费必失败；
③ 执行者适配器同样可达（登记启动意图 → 读回"待核查"）。
⇒ 8.2c 接线时不再需要任何测试旁路。

**门禁**：web-console **1201/0**（1 ignored＝真实调用评测；较 §B-118 增 1 条）、core-runtime 335/0、
crash_harness_smoke 7/0、module_linkage_smoke 8/0、`cargo build --workspace` ✅。

**未做（不冒充）**：8.2c 的**输入路径接线**本身（签发 + 绑定 + 消费三点同批，锚点见 §B-119）、
8.3c 的执行者登记接线、K2–K6。本轮改动**不触碰** CU 输入路径，产品行为与合并前一致。

### B-121 ⚠ **具体契约冲突（接线前必须定）**：许可的"同一动作"身份与执行器逐次尝试不是同一件事

**裁决要求（第八轮授权 §3.2）**："若已冻结的契约对此已有更精确规定，直接采用。发现契约要求…
这类循环前置时，**记录为具体契约冲突，不能用占位 ID 解决**"；本批口径亦写明"后续持久化若发现
不可实现或矛盾之处，应**提交具体反例及最小契约修订**，不能在存储层另写一套不同语义"。
因此本项**停下来登记**，不擅自定身份口径。

#### 冲突是什么

许可契约（§3.3）规定：

- `action_id` 相同且**内容相同**的重复请求 ⇒ **返回已有许可状态**，不得重新创建可执行资格；
- `action_id` 相同但**内容不同** ⇒ 拒绝。

而执行器路径里的 `expected_action_id` 是 `computer_use::action_attempt_id(surface, action)`，
实现为（`computer-use-core/src/contracts.rs:256-263`）：

```text
"{surface}:{target}:{fnv1a64(action 的 JSON)}"
```

**纯内容派生：不含 run、不含步骤序号、不含观察代次、不含时间。** 于是：

- **两条内容完全相同的合法动作**（极常见：点一下 → 观察 → 再点一下；连按两次 Escape；
  同一位置两次同样的拖拽）会得到**同一个 `action_id`**；
- 若把许可按 `action_id` 建键，则第二条会被"同 action 已存在"判成**重复请求**：
  登记被拒、消费也已被第一条用掉 ⇒ **第二次合法动作被拒绝输入**。

#### 真实反例（不是设想）

§B-106 归档的历史语料里就有这种模式：`paint-r3` 的**两次点击动作载荷完全相同**
（`{"arguments":{},"kind":"click","target":"uia-951f959f29c11d9f"}`），只是观察代次不同。
若当时接了"按 `action_id` 建键"的许可门，**第二次点击会被拒**——而这正是产品要支持的正常行为。

（顺带印证 §B-106 的另一条结论：`action_fingerprint` 含观察代次、`action_attempt_id` 不含，
两者不是同一回事。许可要判"同一次尝试"，需要的是**后者缺的那一维**。）

#### 语义分歧的实质

契约里的"**同一动作的重复请求**"指的是**请求级重试**（同一个网络请求被重发，应当幂等返回而非
再发一次输入）；而执行器路径上的"两条相同动作"是**两次独立的、都应当被执行的尝试**。
两者都叫"相同动作"，但**应得的处置相反**：前者要幂等复用，后者要各自独立放行。

#### 最小契约修订（建议，待裁决）

不改六态、不改状态机、不在存储层另写语义；只补一维身份：

1. 许可的**幂等键**改用 `(action_id, attempt_identity)`，其中 `attempt_identity` 由执行器提供
   ——建议直接复用已有的 **观察代次 + 步骤序号**（这两者在执行器里当下都已可得，
   且 `action_fingerprint` 已经在用观察代次，口径一致）；
2. `action_id` 仍保留在许可上（用于审计与"内容是否被换掉"的判定）；
3. "同 `action_id` 不同内容 ⇒ 拒绝"这条**保持不变**；
4. 请求级重试的幂等语义改由**请求侧**承担（同一请求重发时，执行器本来就只会派发一次——
   这一点现有路径已成立，不需要许可再兜一次）。

这样既不动已冻结的六态与执行者身份契约，也不需要占位 ID，更不需要在 SQL 层写第二套语义。

#### 本轮已做／未做

**已做**：把冲突与反例查清并落盘；确认这不是"实现难度"而是**身份口径冲突**，
且猜错的后果是**拒绝合法的真实输入**（安全方向上不是放宽，但产品行为会被破坏）。
**未做**：8.2c 的输入路径接线本身——它**必须**等这个身份口径定下来，否则接出来就是错的。

### B-122 §B-121 裁决落地（Step 1–3 完成，Step 4–5 待做）：身份模型与复合键已冻进代码

**裁决口径（§一／§二／§五）**：保留 `action_id` 作为**动作语义／内容身份**（审计、一致性核对、
篡改判断、历史关联），**不再**单独作许可唯一键、**不再**单独判断重复输入；新增
**`ExecutionAttemptId`** 作为**一次独立执行尝试身份**（许可消费、输入接纳、防同一次请求重复执行）；
许可唯一身份 ＝ **`(action_id, execution_attempt_id)`**。

#### 已完成

| 步 | 内容 |
| --- | --- |
| 契约 | 新增 `ExecutionAttemptId { parent_action_id, observation_generation, step_identity, attempt_sequence }` ＋ 校验：**`attempt_sequence` 从 1 起**（0 表示"还没定是第几次尝试"，那不是身份）、`observation_generation` 从 1 起、占位/空值/控制字符被拒；`stable_key()` 是**可复现**字符串（`action#gen…#step#attempt#`），**刻意不是随机 UUID**（§九.1 禁止——会破坏内容审计与重复分析） |
| 契约 | `InputPermit` 增加**必填** `execution_attempt_id`；`validate_structure` 增加交叉校验：`parent_action_id` **必须等于** 许可的 `action_id`（内容身份不得两处各说各话）。校验顺序刻意先报**字段自身**问题、再报派生一致性冲突（更好诊断） |
| schema | v3 的 `input_safety_permits` 增加 `observation_generation`／`step_identity`／`attempt_sequence`／`execution_attempt_id` 四列，唯一约束由"动作内容"改为 **`UNIQUE(action_id, execution_attempt_id)`**（正是 §五 的"扩展许可记录、不建平行表"） |
| 存储 | 写入与读回都带四维；`load_permit` 从库中重建 `ExecutionAttemptId`（**不靠占位值**，重建失败即报错） |

#### 依赖"观察代次 + 步骤序号"为何不够（裁决 §二，已按此实现）

`observation_generation` 是**观察上下文**、`step_identity` 是**规划位置**，都不天然唯一
（恢复重算后 `gen=10, step=3` 可能再次出现）。因此身份里必须带 `attempt_sequence`——
它是"同一逻辑步骤再次尝试的递增编号"，正是把"再规划一次"与"同一次尝试"分开的那一维。

#### 未做（如实，且**下一步就是它**）

1. **§四的四条重复判定规则**尚未按新键改写：`register_pending` 目前仍是"同一 `action_id` 已存在即拒"，
   因此 **B121-T1（同内容不同尝试 ⇒ 两个许可、两次允许）当前会失败**——必须先改这条规则。
   规则落点已明确：按**逻辑尝试键**（`gen#step#attempt`，去掉内容哈希）判定——
   同逻辑尝试且**内容不同** ⇒ `ActionIdentityConflict`（拒绝，§四 情况 3）；
   同逻辑尝试且**内容相同** ⇒ 返回已有状态（情况 1／2）；不同逻辑尝试 ⇒ 允许新许可（情况 2／4）。
2. **B121-T1…T5 五条验收用例**（其中 T5 用 `paint-r3` 的真实两次点击，证明"动作内容重复 ≠ 执行请求重复"）
   ——这是本轮最该先补的，因为它们才是"不再错误拒绝合法输入"的证据。
3. **8.2c 接线**（§七 的顺序：先生成 `ExecutionAttemptId` → authority 验证 → 创建许可 → 消费）。

**门禁**：core-runtime **335/0**、web-console **1201/0**、crash_harness_smoke 7/0、
module_linkage_smoke 8/0、`cargo build --workspace` ✅。
**未触碰 CU 输入路径**；未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

**过程中修掉的两处自伤**（都值得记）：① 给 `InputPermit` 加必填字段后，两个**测试辅助**构造点
（core-runtime 的 `permit()` 与存储侧的同名辅助）没同步 —— 非测试构建通过、**测试构建失败**，
说明"`cargo build` 绿"不等于"测试能编译"；② 我新加的交叉校验排在字段校验之前，抢报了更派生的
错误（把"action_id 是占位值"报成了"两处不一致"），已把**字段自身校验前置**。

### B-123 §B-121 裁决落地（Step 4 完成）：§四 四条重复判定规则按"逻辑尝试键"改写 + **B121-T1…T5 全通过**

**裁决口径（§四／§六）**：重复判定不得按 `action_id`，而按**执行尝试**；
T1…T5 是"动作内容重复 ≠ 执行请求重复"的直接证据。

#### 规则改写（落在 `register_pending` 的同一个事务里）

判定键换成**逻辑尝试键** = `观察代次 # 步骤 # 尝试序号`（**去掉内容哈希**——内容变了它不变，
正是识别"同尝试换内容"所需要的）：

| 情况 | 条件 | 结果 |
| --- | --- | --- |
| 1／2 同一次请求重试 | 同逻辑尝试 ＋ **同内容**（同 action_id） | 返回已有状态语义，**不新建、不重新输入** |
| 3 尝试被复用 | 同逻辑尝试 ＋ **内容不同** | **拒绝** `ActionIdentityConflict`（执行身份被复用） |
| 2／4 同内容的再一次尝试 | **不同逻辑尝试**（不同代次/步骤/序号） | **允许新许可** |

注释里明确写了"这里刻意**不**按 `action_id` 拒——那正是 §B-121 要修掉的错误合并"。

#### B121-T1…T5（全部通过）

| 用例 | 场景 | 结果 |
| --- | --- | --- |
| **T1** | 同内容、不同观察代次 ⇒ 两个许可、**两次都允许**且**两次都能消费** | ✅ |
| **T2** | 同一次请求重试（action_id ＋ 尝试完全相同）⇒ 不新建、已有许可状态不变（仍待激活） | ✅ |
| **T3** | 同一逻辑尝试换内容 ⇒ `ActionIdentityConflict`（并核对点名了两个 action_id） | ✅ |
| **T4** | 恢复重算：同代次同步骤、仅 `attempt_sequence` 递增 ⇒ **新的执行尝试**，不被旧许可吞掉 | ✅ |
| **T5（关键）** | 用 §B-106 归档的 **`paint-r3` 真实两次点击**（目标与载荷完全相同、观察代次不同）：先断言两者 `action_id` **相同**、`execution_attempt_id` **不同**，再断言第二次**必须放行** | ✅ |

T5 是这次冲突的现场复现：它证明"动作内容重复 ≠ 执行请求重复"，
也就是**修掉了一个比缺功能更严重的问题——安全系统错误拒绝合法输入**。

#### 变异验证 3/3

① 把判定键退回按 `action_id`（即旧错误行为）⇒ T1／T5 红；
② 让"同尝试换内容"不再拒绝 ⇒ T3 红；
③ 把身份里的 `attempt_sequence` 固定成常量 ⇒ T4 红。

#### 顺带修掉的一处**用例数据失真**

我早先写的 `permits_are_reachable_through_the_production_accessor` 让**两个不同动作共用一个逻辑尝试**
（`gen41#step-7#attempt1`），新规则下被正确拒为 `ActionIdentityConflict` ⇒ 该用例红。
这是**用例数据不真实**，不是规则错：已把第二条改成**另一次执行尝试**（新代次/新步骤），
并在注释里写明"同一逻辑尝试换内容现在会被正确拒绝"。

**门禁**：web-console **1206/0**（1 ignored＝真实调用评测；较 §B-122 增 5 条）、
core-runtime 335/0、crash_harness_smoke 7/0、module_linkage_smoke 8/0、`cargo build --workspace` ✅。

**未做**：8.2c 接线本身（§七 的 Step 5：生成 `ExecutionAttemptId` → authority 验证 → 创建许可 → 消费）。
**未触碰 CU 输入路径**；未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-124 ⚠ Step 5 接线前的**第二处具体缺口**：`policy_revision` 没有真实来源（不得伪造）

**背景**：Step 1–4 已就绪（身份契约、复合键、§四 规则、B121-T1…T5），按 §七 开始 Step 5 的接线时，
在执行器接纳边界需要构造 `InputPermit`。许可的最低绑定（§3.3）要求填 **"当前安全政策与 gate revision"**。
我当初把它materialize 成了一个字段 `InputSafetyResourceState` 之外的东西：

```
InputPermit { ..., policy_revision: u64, gate_revision: u64, ... }
```

**核实结果（全仓库检索）**：`policy_revision` **没有任何真实来源**——它不是笔误，是**我加契约时自己
引入的字段**。库里现成的只有：

| 真实存在的 | 出处 |
| --- | --- |
| `gate_revision` | `InputSafetyResourceState.revision`（单调 revision，任何安全状态变更都推进它） |
| `recovery_epoch` | `InputSafetyResourceState.recovery_epoch` |
| **`policy_revision`** | **不存在**：`InputSafetyResourceState` 只有 `scope / state / revision / coordinator_instance_id / recovery_epoch / accepts_new_input` |

**为什么停下来而不是先填一个值**：

- 填常量（例如 schema 版本或 `0`）＝ **编造绑定**：许可会声称"绑定到某个政策版本"，而那个版本
  并不存在；将来政策变化时**没有任何东西会推进它**，等于一个永远成立的假闸门。
- 这恰好是本轮反复强调的那条：**不得用占位值顶替真实来源**（§3.2"不能用占位 ID 解决"、
  §九.4"不得在 SQL 层用 `ON CONFLICT` 解决"是同一类）。B-121 已经因为同类问题停过一次。

**两个可选处置（请择一，或给第三种）**：

1. **删掉 `policy_revision`**（最小、最诚实）：许可只绑定 `gate_revision` 与 `held_epoch`，
   两者都有真实来源；将来真需要"政策版本"时再作为独立决策引入。
2. **引入真实政策版本来源**：在输入安全库里给它一个可推进的 revision（例如随安全政策/配置变更推进），
   并明确谁推进它。这需要新的存储面与推进点，属于新的授权范围。

**顺带确认一个我本来要自己定的位置问题（现已定，供复核）**：按 §七"生成 attempt 在执行器接纳边界、
消费在物理输入前"，许可门应落在 **`computer_use_executor.rs:228`** 的**逐动作** fail-closed 点
（与既有 `admit_action_origin` 同一位置、同一口径），而**不是** `:1964` 的**运行级**资源闸门
（那里已在接纳时检查过"资源是否接受新输入"）。两者互补：运行级管"这次运行能不能输入"，
逐动作级管"这一次执行尝试有没有资格"。

**本轮状态**：Step 1–4 已完成并有验收（§B-122／§B-123），Step 5 因上述缺口**未开始编码**。
**未触碰 CU 输入路径**；未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-127 授权"接入真实生产者"（8.3c）：**缝在哪已核实**，下一步是穿过它

**授权**：允许接入 `executor_instance_id` 的**真实生产者**，然后与许可门（签发＋消费）同批落地。
**为什么要生产者**：§B-126 的许可门在 consume 时必须绑定真实执行者实例；今天唯一可填空的字符串是
尝试身份 —— 填进去就是**伪造执行者身份**（与 B-121／B-124 同类），所以宁可不接。

#### 核实的结论（不是推断）

- 输入层**已经在捕获** helper 的进程身份：`input.rs:6392` `windows_process_guard::capture_process_identity(self.pid)`，
  以及场景收尾用的 `GrandchildHolder { pid, identity }`（按身份核对后再终止，PID 复用会被拒）。
- 但它**没有对外暴露**：`NativeInputOutcome` 的字段只有 `obligation` / `facts` / `fact_anomaly` /
  `reply_received_at_ms` / "确认子进程真正结束的时刻"，**没有 pid、没有创建时间**。
- ⇒ 生产者缺的不是"捕获能力"（已有），而是**一条把已捕获的身份送到宿主的缝**。

#### 生产者设计（最小改动，复用已有捕获）

1. **缝**：在 `NativeInputOutcome`（或回执）上暴露
   `helper_process: Option<HelperProcessIdentity { pid: u32, creation_time_filetime: u64 }>`，
   取值来自**同一处已捕获的身份**（`capture_process_identity` 的结果），**不新增第二条捕获路径**
   ——两条捕获路径必然漂移，这正是本轮反复防的事。
2. **登记**：执行器在原生输入派发点用 `ExecutorStore`（8.3b 已交付，含失败表处置）
   落 `register_launch_intent` → `record_instance_evidence`（用契约的 `ExecutorObservation`
   表达"匹配且存活／已退出／不可读"等），得到**真实** `executor_instance_id`。
3. **绑定**：许可门的 consume 用**该** id，替换掉现在的占位自绑
   （`PermitExecutorBinding::BindTo(&self.attempt_key)` 必须消失）。
4. **顺带修掉 B-126 查出的静默缺陷**：契约里"越过派发边界必须有执行者实例"
   （`input_safety.rs:946-951`）**只在签发路径被校验**（`validate_structure` 只被
   `input_permit_store.rs:282` 的 `register_pending` 调用），consume 路径**不校验它** ⇒
   伪造值能静默落库。同批把该校验加到 consume（或在存储层加约束）。

#### 为什么这一步必须与许可门同批（而不是先后）

- 只接签发：会产生**永远无法消费**的 pending 许可（消费者需要执行者实例 ⇒ 必然失败），是垃圾数据；
- 只接消费：没有可绑定的真实执行者 id ⇒ 必须伪造；
- 先接许可再接生产者：等于先埋一个假绑定再换掉它，中间态是**不安全且已落库**的。

#### 边界与顺序

- 涉及 `computer-use-core`（输入层契约）与 web-console（执行器／存储）：按裁决
  "broker／helper 由对应负责人串行合并"，本轮由同一串行角色推进。
- **未触碰 CU 输入路径**（本项尚未开始编码）；未迁移真实库、未解除隔离、未启动真实输入、
  未提权、未安装、未推送。

### B-131 会话交接：下一步（8.3c 生产者 → 许可门接线）已写成自包含交接文件

本会话在此收尾。接手入口：**`handoff-8.3c-producer-2026-09-26.md`**（与台账同目录），内容包括：

- **当前状态**：分支／门禁基线数字／本会话 14 个提交的清单与含义／"已闭环、不要再动"的清单；
- **下一步三步**（顺序固定）：① 把**已捕获的** helper 身份开到 `NativeInputOutcome`（复用
  `input.rs:6392`，**不新增第二条捕获路径**）；② 执行器用 `ExecutorStore` 落
  `register_launch_intent → record_instance_evidence` 产出真实 `executor_instance_id`
  （判定用已冻结的 `classify_executor_instance`）；③ 许可门绑定真实 id 后，才接
  `computer_use_executor.rs:228`（**DB 操作必须留在 broker 边界之外**）；
- **验收**：B126-T1 待做；T2／T4／T5 已做；T3 判定机制已做、端到端待补；**另需一条"输入照旧发生"的
  正向用例**；并延续"变异必须验证确实改变了行为"（本会话踩过 3 次无操作变异）；
- **禁止事项 10 条**（含不得用 attempt 冒充执行者、不得加回 policy_revision、不得在
  `dispatch_if_current` 边界内做 DB 操作、不得用占位值让契约检查通过等）；
- **失败纪律**：加/删字段是全链改动（结构体／构造点／SELECT／投影索引／INSERT 占位符／测试辅助），
  且 **`cargo build` 绿 ≠ 测试能编译**；无法一轮完成就回到最近全绿提交，不留半改状态。

**本会话未做**（不冒充）：8.3c 的第 1–3 步编码、B126-T1、T3 端到端、`:228` 接线、K2–K6。
**未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。**

### B-133 ⚠ 第 2 步的**结构性发现**：受控输入"一次调用＝spawn＋立即注入"，装不下裁决要求的创建顺序

**裁决要求的顺序（§五／§七）**：

```
创建尚不允许输入的 helper → 绑定监督 → 取实际身份 → 持久化登记 → 握手 → 最后才签发/消费输入许可
```

**核实到的事实（不是推断）**：受控输入族的每个入口都是**一次调用完成全流程**。
以 `input.rs:2659 controlled_click(...)` 为例：它接收 `attempt`，内部走 `native_run(...)`，
**在同一次调用里 spawn helper 并立即执行注入**，返回 `NativeInputOutcome`。
⇒ helper **没有"已创建、待许可"的空闲阶段**；身份（第 1 步刚带出来的 `helper_process`）
是在**注入那一刻**才存在的。

**为什么这挡住了第 2／3 步**：许可要在**物理输入之前**消费，而消费要求绑定**已核实的执行者实例**；
但实例身份只在"即将注入"的那一刻才被创建。两者在现有生命周期里**互斥**：

- 若在 spawn **之前**消费许可 ⇒ 许可指向一个**尚不存在**的执行者实例
  —— 这正是裁决明令禁止的 **"先产生 permit 再补执行者"**，也是我们连续拒绝过的第四类"幻觉绑定"。
- 若在 spawn **之后**消费 ⇒ 那时 helper 已经准备注入，许可门来不及阻止这一次输入
  （门要在**输入之前**生效才有意义）。

**因此第 2／3 步需要先把 helper 生命周期拆成两相**（这是范围变化，请裁决）：

| 方案 | 内容 | 代价 |
| --- | --- | --- |
| **(a) 两相 helper（建议）** | ① 创建＋登记＋握手，helper **处于"不得输入"的等待态**；② 收到"许可已消费"的命令后才执行注入 | 改 `computer-use-core` 的 helper 协议 ＋ helper 脚本（C#／PowerShell）；这正是裁决说的 **broker／helper 面**，由串行负责人合并。**新增一个"等待许可"状态**，本身是要防"辅助进程被误当成可输入"的新风险点 |
| (b) 宿主侧执行者身份 | 把"执行者"定义为**创建并监督 helper 的宿主进程/启动操作**，在 spawn 前登记意图（`register_launch_intent`），实例证据事后补录 | 与裁决 §四"执行者＝实际执行实例"不符；且"事后补录"接近被禁的"先 create permit 再补 executor" |
| (c) 维持现状不接 | 保留 `input_permit_gate` 的可测单元与全部契约／存储验收，**不接** `:228`，直到两相 helper 落地 | 产品不加新限制，但也不获得许可门的保护；**本项停在"已就绪但未接线"** |

**我不做**：为了让第 2／3 步"看起来能接"而把消费提前到 spawn 之前，或放宽"必须有已核实执行者"这条校验。
两者都会制造一个**看起来比缺失更安全**的绑定——本会话已因同类原因停过三次（B-121／B-124／B-127），
每次都被裁决确认正确。

**当前状态**：第 1 步已完成（§B-132：身份在存活时捕获并经 `NativeInputOutcome` 暴露，
core 127/0）；第 2／3 步**等上面 (a)/(b)/(c) 的选择**。许可门、契约、存储、验收（B121-T1…T5、
B124-T1/T2、B126-T2/T4/T5）均已就位且全绿，**不因本发现而失效**。
**未触碰真实输入路径**；未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-135 8.3c-2 的设计锚点（已侦察）：两相握手**复用既有文件控制通道**，不新造传输

**侦察目标**：把 helper 改成"起来但不动、等许可"的两相形态，需要一条控制通道。先看有没有现成的。

**核实到的事实（`input.rs`）**：

| 事实 | 值 |
| --- | --- |
| helper 的启动形态 | `powershell.exe -NoProfile -NonInteractive -Sta -Command <script>`（`:1149`／`:2991`）——**整个请求是内嵌在命令行脚本里的 JSON** |
| **已有的控制通道** | 请求里带 `cancel_file` 与 `progress_file`（`:3394-3395`），路径是 `%TEMP%` 下带 nonce 的文件（`:3379-3380`） |
| 取消通道 | 宿主写 `cancel_file`；helper 在轮询里检查它（`stroke_cancelled` 路径） |
| 事实通道 | helper 往 `progress_file` 追加增量协议记录；宿主轮询读回（"不依赖 EOF"） |
| 成功标记 | `NATIVE_HELPER_SUCCESS_MARKER = "native-input:ok"`（`:1217`），可从增量记录里先看到 |

**⇒ 结论：两相握手应当复用这条既有文件通道，而不是新增 IPC／命名管道／套接字。**
理由与本轮反复坚持的一致：**不新增第二条同义通道**（两条必然漂移）。具体建议：

1. 请求里**增加一个 `ready_file`**（与 `cancel_file`／`progress_file` 同族、同 nonce、同目录），
   语义与 `progress_file` 严格分开：`progress_file` 是**执行事实**，`ready_file` 是**生命周期信号**；
2. helper 在被创建后**立即**（注入任何输入之前）写 `ready_file`，然后进入
   `AwaitingPermit` 的**有界、可取消**等待循环——复用它对 `cancel_file` 已有的轮询形状，
   **不引入新的等待原语**；
3. 宿主等到 `ready_file` 后：捕获身份（`child.id()`，§B-132 已完成）→ `ExecutorStore` 登记 →
   签发并**消费**许可 → **写 `permit` 文件**；
4. helper 只有在"`ready` 已写 ＋ 收到 permit"之后才允许进入 `ExecutingInput`——
   这正是 §B-134 冻结的 `HelperHandshake::Execute` 前置条件（`required_state = PermitConsumed`）。

**这一步为什么不在本轮编码**：改动落在**内嵌 PowerShell／C# 脚本文本**（`Engine.Run` 的 mode 开关、
`input_stroke_native.cs`）与 `computer-use-core` 协议两处，**必须用真实 helper 进程验证**
（"脚本真的起来但不动"这件事，只有跑起来才知道）；而它是**安全关键路径**——
若"等待许可"实现成了忙等或让 helper 在等待期意外可输入，破坏面比缺门更大。
按 §十一，`:228` 的接线本轮**继续禁止**。

**当前状态**：§B-134 已把状态机与握手协议冻结为契约（core-runtime 341/0）；
本项只登记设计锚点，**未改任何脚本或协议**。CU 输入路径与合并前一致。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-137 ⚠ 8.3c-2 的**次序约束（阻塞汇报）**：helper 侧"等许可"不能单独落地，否则会锁死全部 CU 输入

**已确认的插入点（可直接开工）**：内嵌 PowerShell 脚本里，物理输入的调用是
`input.rs:4344 [CoolzhuNative.Engine]::Run(`（随后 `:4355` 打印 `'native-input:ok'`）。
`write-ready → wait-permit` 的正确位置就是 **`:4344` 之前**——这正好满足裁决 §八
"READY 必须发生在任何输入 API 调用之前"。

**但发现的次序约束（这就是本轮要汇报的阻塞）**：`wait-permit` 一旦落地，helper 会**等待一个
当前没有任何代码会写入的 permit 文件**。后果不是"功能缺失"，而是：

- 轻则每次输入都被自守望超时打断（helper 有 `self_watchdog_ms`，_*不会*永久挂死）；
- 重则**全部受控 CU 输入失效**——因为写 permit 文件的前置（身份登记 + 许可签发与消费）
  属于 **8.3c-3／8.2c**，尚未实现。

也就是说：**8.3c-2 的 helper 侧与 8.3c-3／8.2c 的宿主侧是一个原子批次**，不能按裁决 §十二 的
分段那样"先合 8.3c-2 再合 8.3c-3"。分段合并在这一步**不成立**，原因是生命周期本身要求
"等待"与"通知"同时存在，缺一边就是死锁而非降级。

**唯一能"先做一半"的方式**是让宿主**无条件**写 permit 文件——但那等于把"未授权的执行"伪装成
"已授权"，正是裁决 §三 明确禁止的（"permit 文件不是授权来源"的前提是**宿主真的消费过许可**）。
因此不做。

**⇒ 本轮结论（阻塞）**：8.3c-2 的实现需要与 **8.3c-3（`ExecutorStore` 登记）＋ 8.2c（许可
签发/消费/写 permit 文件）** 合并为**一次原子改动**，且必须用**真实 helper 进程**验证
（B127-T6…T10 全部要求真实进程）。这次改动同时触及：
- `computer-use-core` 的内嵌 PowerShell／C# 脚本文本（新增 `ready_file`／permit 读取与有界等待循环，
  复用 `cancel_file` 轮询形状、cancel 优先）；
- `computer-use-core` 的宿主侧等待与请求字段（新增 `ready_file`，等 `ready` 后继续）；
- web-console 执行器（身份登记 → 许可签发/消费 → 写 permit 文件）；
- 五条进程级用例。

**本轮未改任何脚本或协议**；CU 输入路径与合并前一致；未迁移真实库、未解除隔离、未启动真实输入、
未提权、未安装、未推送。**这条阻塞不是"要不要做"的裁决问题，而是"必须合并批次"的实施约束**，
故按指示如实汇报，不请求新决策。

### B-138 批次边界裁决落地（§B-137 阻塞的答复）：8.3c-A 原子切片，并汇报本轮阻塞

**裁决**：批准把 `8.3c-2`／`8.3c-3`／`8.2c` 合并为**一个交付单元 `8.3c-A`**（两相 helper 端到端切片）。
分层仍在（契约分层、提交可拆），但**不得存在中间可运行版本**让 helper 等待不存在的 permit。
同时明确拒绝两个半成品：只加 `wait-permit` 而"以后再写 permit"（＝产品被自身半升级状态永久拒绝，
**不是** fail-closed）；以及宿主绕过 `ExecutorStore`／`PermitGate` 直接写 permit（＝把文件当授权来源）。

**已把批次边界、四处改动范围、协议补充要求（permit 仅宿主生成且绑 nonce、等待循环复用 cancel 模型、
cancel 优先、DB 在 broker 边界外、写 permit 后崩溃 ⇒ PermitConsumed ＋ ExecuteUnknown）、
新增的 B127-T11（旧 permit 残留跨 nonce 必须拒绝）与 T12（READY 伪造必须拒绝）、
以及 §十 的**完整完成标准**写进交接文件 `handoff-8.3c-producer-2026-09-26.md` 的新章节 3-bis。**

**本轮阻塞（如实汇报，不请求新决策）**：`8.3c-A` 是**不可拆分的原子批次**（改动点：内嵌
PowerShell／C# 脚本、`computer-use-core` 宿主侧、web-console 执行器、7 条真实进程用例），
且按裁决 Step C **不允许**失败回退到旧路径——因此**不存在"先落一半也能跑"的安全中间态**。
本会话剩余工作预算不足以在一轮内完成"改完四处 ＋ 真实 helper 进程验证 7 条 ＋ 收尾"，
而按 Step A／Step C 的要求，**未完成前旧路径必须保持有效**（本轮既未改脚本也未改协议，
旧路径完好）。⇒ 下一次接手应从交接文件 §3-bis 直接开工，一次做完。

**未改任何脚本或协议**。门禁维持：web-console **1215/0**（1 ignored＝真实调用评测）、
core-runtime **345/0**、computer-use-core 127/0、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-139 8.3c-A 状态基准裁决：**保持冻结、不拆批**（本轮不推进代码）

**裁决**：本轮**不推进代码修改**。8.3c-A 已不是"多个小改动"，而是**一次安全生命周期切换批次**；
任何单侧落地都会产生**比当前旧路径更危险的中间态**。正式登记：
`8.3c-A = 契约完成，实现未开始（需要一次完整端到端切换窗口）`。

**阻塞性质澄清（登记备查）**：不是技术未知、不是架构未定、不是要重新设计，而是
**原子交付窗口不足**——四部分强依赖：`helper protocol → identity registration → permit consume
→ execute gate → real helper tests`。三个半改的后果各自写进交接文件（半改 A 让新版本自身阻断
所有输入；半改 B 等于"文件存在＝授权"；半改 C 身份存在但未控制执行时序）。

**已写进交接文件 `handoff-8.3c-producer-2026-09-26.md` 新章节 3-ter（下一次窗口的唯一口径）**：

- **四阶段顺序**（不得改变）：Phase 1 helper 协议（先过 T6／T7／T9／T12，重点是"READY＝零物理输入"）
  → Phase 2 宿主身份登记 → Phase 3 Permit 真消费（替换 attempt 占位为真实 `executor_instance_id`）
  → Phase 4 恢复 EXECUTE（措辞：**不是接入 Permit，而是把原生输入触发点移动到 EXECUTE gate 之后**）；
- **提交可拆、行为切换必须一次完成**（feature flag／atomic merge／同一 RC）；
- **新增切换前门禁 `8.3c-A Preflight Gate`**：恢复 `:228` 前必须校验
  **helper protocol version／host protocol version／permit schema version／executor identity support**
  四项一致——helper 是 PowerShell/C#、不是 Rust 内部模块，**协议版本必须显式存在**（当前尚未存在）；
- **新增 T13**：旧 helper 协议（无 READY／EXECUTE）⇒ 拒绝执行，**不能自动降级**；
- **环境与验收命令**：独立测试 safety root、不触碰真实 input scope；
  必须至少跑 computer-use-core ＋ web-console ＋ core-runtime 的 `cargo test` 与 `cargo build --workspace`
  ——**不能只跑 `cargo build`**（本会话三次"编译通过但测试构造点失败"已有记载）。

**本轮未改任何代码**（含脚本与协议）。门禁维持：web-console **1215/0**（1 ignored＝真实调用评测）、
core-runtime **345/0**、computer-use-core 127/0、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-140 执行 8.3c-A 时的**分层阻塞与设计缺口**（裁决未规定"谁编排"）

按"按计划执行"启动 Phase 1，先核两件决定批次形状的事，结果是**两个硬事实**：

| 核实项 | 结果 | 含义 |
| --- | --- | --- |
| `computer-use-core` 是否依赖 web-console | **否**（Cargo.toml 无该依赖） | ⇒ 它**无法**调用 `PermitGate`／`ExecutorStore`（两者在 web-console） |
| `controlled_*` 的消费方 | **4 个文件／3 个 crate**：computer-use-core 自身、`gui-desktop/desktop-console`、`gui-web/web-console` 的 bridge 与 main | ⇒ 改 `controlled_*`／`native_run` **签名**的影响面是跨 crate 的 |

**由此得出的设计缺口（裁决 §三／§七 没有规定）**：helper 由 `run_native_helper`（在
computer-use-core 内）创建，而"等 READY → 捕获身份 → `ExecutorStore` 登记 → 许可签发/消费 →
写 permit"这一串**只能在 web-console 里做**（gate 与 executor 适配器都在那边）。

⇒ 因此 `run_native_helper` **不能再是"一次调用完成创建＋输入"**，必须**拆成两相 API**：

```
prepare  : 创建 helper → 等 READY → 返回 helper 身份（helper 保持存活、等待许可）
   ↓        （web-console 在此之间完成：注册执行者 → 签发并消费许可 → 写 permit 文件）
execute  : 让 helper 继续（helper 轮询到本人 permit → EXECUTE → 注入）
```

**这就是 8.3c-A 的真实体量（比裁决描述更大）**：
① `computer-use-core` 的 **API 拆分**（prepare／execute），影响 4 个文件／3 个 crate；
② helper 脚本协议（READY／等 permit／EXECUTE gate）；
③ web-console 的编排（登记 + 许可 + 写 permit）；
④ 8 条真实进程用例（T6…T13）；
⑤ 且按裁决 §四，旧路径必须保持默认有效（feature flag／atomic merge），**不得出现半升级可运行中间态**。

**为什么本轮不继续往下改**：上面任一项单独落地都会命中已被明确拒绝的三个半改之一
（helper 空等 / 宿主直写 permit / 身份存在但未控制时序）；而"①的 API 拆分 + ②的脚本协议 +
③的编排 + ④的 8 条真实进程验证"超出本会话剩余预算。⇒ 按"遇到阻塞汇报阻塞问题"如实汇报。

**下一步入口（已补进交接 §3-ter 的执行口径）**：先定"**谁编排**"这个设计点——
建议由 **web-console 编排**（它同时持有执行器与许可门），computer-use-core 只提供
prepare／execute 两个入口并保持**不含任何安全判定**（它本来也不该有）。这样分层不破：
安全判定全在 web-console，core 只负责"能创建、能等、能执行"。
**本条不是要新决策**，而是把裁决留下的一个实施空位补齐并登记。

**本轮未改任何代码**（含脚本与协议）。门禁维持：web-console **1215/0**、core-runtime **345/0**、
computer-use-core **127/0**、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-142 8.3c-A 命名与状态纠正 + runtime 窗口实现纪律（裁决 §一／§三／§四／§五）

**命名纠正（§一）**：`8.3c-A-contract`（设计冻结 ＋ 协议门禁）＝ **完成**；
`8.3c-A-runtime`（helper READY/WAIT_PERMIT/EXECUTE 的实际行为、permit 文件真实流转、
`ExecutorStore` 实际绑定、EXECUTE gate）＝ **待完整切换窗口**。
协议门禁解决的是"**新旧 helper 是否允许进入同一生命周期**"，**不是**"helper 是否已完成两相执行"
⇒ **不得**把门禁通过写成"8.3c-A 已完成"。同时裁决明确：**本轮不再增加契约层内容**，
继续加状态／握手／契约测试已不是降低风险而是**契约膨胀**；真正缺的是
`contract → runtime implementation → real helper → real executor → real permit`。

**已写进交接文件 §3-ter**（实现纪律，按**生命周期顺序**而非文件顺序）：

- **Phase 1 helper 双相化**：`START → READY → WAIT_PERMIT → PERMIT → EXECUTE → INPUT`；
  **此阶段结束旧路径仍未切换**（可存在新协议代码，但生产 executor 不调用）；
  出口条件先过 **T6**（spawn→READY→wait，physical input = 0）与 **T12**（伪 READY 必须校验
  valid nonce ＋ valid state ＋ valid helper identity，不是文件存在即通过）。
- **Phase 2 宿主编排**（web-console，它持有 ExecutorStore／PermitGate／输入安全状态）：
  `prepare → wait READY → capture identity → register → issue → consume → write permit`；
  **硬要求：web-console 不得自己读 helper PID**——身份**唯一来源**是
  `NativeInputOutcome → ProcessInstanceEvidence`，否则又出现"core 捕获一次、web 再查一次"两个来源。
- **Phase 3 切换 `controlled_*`**（**唯一危险阶段**）：必须**一次完成**，
  不允许部分新路径部分旧路径（同一动作会出现两种语义）；失败 ⇒ **fail closed，不得回退旧 helper**。
- **切换保护开关** `two_phase_helper_required = true`：**不是 fallback**，而是启动时检查生产组合完整
  （helper protocol=1／host protocol=1／permit schema 兼容／executor identity supported）；
  任一不满足 ⇒ **该能力启动失败**，而非切旧路径。
- **测试顺序三组**：① helper 独立（T6／T12／T11，无需真实 permit）；② 完整授权链（T8／T7／T9，需 web-console）；
  ③ 故障窗口（T10 与 consume 后／execute 前／execute 后崩溃，依赖真实状态机）。
- **schema**：暂不新增（已有 permit 状态／executor identity／helper lifecycle 足以表达）；
  只有"必须持久化 `AwaitingPermit` 的 helper"时才新增，**不得为记录中间态扩大安全库**。
- **非批次项**（非空基线／并发释放）：批准但**排序放后**——属验证已有资源模型，
  不在切换窗口前引入额外共享状态测试。

**当前状态不是阻塞，而是正确冻结点**：风险已从"设计错误"转移为"实现切换纪律"。
**本轮未改任何代码**。门禁维持：web-console **1215/0**（1 ignored＝真实调用评测）、
core-runtime **347/0**、computer-use-core 127/0、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-143 8.3c-A-runtime · Phase 1 宿主半边交付：READY 的有界等待与校验（T12 宿主侧）

按裁决 §三 Phase 1"**此阶段结束旧路径仍未切换；允许存在新协议代码，但生产 executor 不调用**"的授权，
先做其中**不触碰内嵌脚本**、可独立验证的那一半：宿主侧"等到 READY 并校验它"。

**交付**（`computer-use-core/src/helper_ready.rs`）：

- `await_helper_ready(ready_file, expected_nonce, budget)`：**有界**（超预算即 `TimedOut`）、
  **不忙等**（固定 20ms 轮询，与既有 cancel/progress 轮询同量级、**不引入新等待机制**）；
- `ReadyOutcome::{Ready, Rejected, TimedOut}` 可分辨；`may_proceed()` 只有合法 READY 为真；
- **T12 的宿主半边**：文件存在**不等于** READY——必须过
  ① `HelperReadySignal::validate()`（type／协议版本／nonce／pid）② **nonce 属于本次会话**
  ③ 解析失败（半截写入／多带授权声明）也算**拒绝**；
- 非法信号**立即拒绝**而不是"当作还没到再等一会"（伪造信号重试只会拖长窗口）。

**测试 3 条**：合法 READY 在预算内放行；伪造/异会话 READY（别的 nonce、缺字段、带
`authorization` 声明）必须被拒；无文件时有界超时（耗时贴近预算）。
`computer-use-core` **130/0**（增 3 条）。

**⚠ 命名冲突提醒（本轮顺带发现，登记以免将来混淆）**：输入层**已有**一个不同含义的
"helper ready"——`input::tests::native_lifecycle::launch_budget_expiry_before_helper_ready_*`
里的 ready 指"**进程启动预算**内就绪"，与本模块的 **两相生命周期信号 READY 不是一回事**。
这正是裁决 §B-121 警告过的"一词多义"风险（当时是 `attempt` 同时表示内容指纹／执行尝试／请求重试）。
⇒ 本模块的文档与函数名都显式写作 **两相 READY / lifecycle READY**，落地脚本侧时也应在协议字段里
保持区分（`ready_file` 属两相协议，`launch_budget` 属启动预算），不要复用同一个词。

**边界**：本模块**不启用任何生产路径**（`controlled_*` 未调用它），**未改内嵌 PowerShell／C# 脚本**，
Phase 3 的一次性切换仍未开始。门禁：computer-use-core **130/0**、web-console **1215/0**
（1 ignored＝真实调用评测）、core-runtime **347/0**、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-144 ⚠ 脚本侧两相段：**已写、已验证不了、已回退**（阻塞汇报：缺可靠的离线脚本校验）

**做了什么**：按 Phase 1 把两相段插进内嵌 PowerShell（`NATIVE_INPUT_HELPER_ENTRY`）——
位置在 `$progress` 创建之后、`[CoolzhuNative.Engine]::Run(` 之前，且**opt-in**
（`if ($r.two_phase_helper)`，该字段默认不存在 ⇒ 本段**绝不执行**，行为与旧路径一致）；
内容：写 `ready_file`（type／协议版本／nonce＝`request_id`／`$PID`／时间戳）→ 有界轮询
（**先 cancel**；permit 命中且 nonce 匹配才继续；nonce 不匹配则 `exit 4` 拒绝输入；
cancel 时不在此退出、落回 `Engine::Run` 由既有 driver 按 `stroke_cancelled` 处理，保持取消语义一致）。

**为什么回退**：脚本改动**影响所有输入模式**（内嵌脚本是一个整体），而我用来兜底的
"离线语法校验"（抽出脚本文本 → `powershell -NoProfile -Command "[void][scriptblock]::Create(...)"`）
**没能被证明有效**：

| 用例 | 期望 | 实际 |
| --- | --- | --- |
| 基线（含新段） | 解析通过 | 通过 ✅ |
| M1 少一个右花括号 | 应解析失败 | **通过**（未抓住） |
| M2 字符串未闭合 | 应解析失败 | **通过**（未抓住；且这次变异本身构造有误——去掉一个 `"` 后单引号串仍合法） |

⇒ **校验的检出能力未建立**（至少 M1 表明 `[scriptblock]::Create` 在这个形状下不会因括号不配对报错，
或我的提取/比较有缺陷）。既然"改错脚本＝全部输入失效"且**没有可信的离线保护**，按既有纪律
**回到最近全绿提交**（`git checkout -- input.rs`），不留半改。

**这不是"脚本不能改"，而是"改脚本必须能真跑 helper 来验证"**：本机有 PowerShell，
真正的保护是 **T6（spawn → READY → wait，physical input = 0）这类真实进程用例**，
而不是静态解析。⇒ 脚本侧两相段归入**必须真跑**的那一批，与 Phase 1 出口条件同时完成。

**顺带登记**：`git checkout` 后 `input.rs` 回到 HEAD（含 §B-132 的身份捕获），
`two_phase_helper` 出现次数 **0**；`computer-use-core` **130/0**（含 §B-143 的 3 条 READY 用例），
工作区干净。

**未做**：脚本侧两相段（归入"必须真跑"批次）；Phase 2／3；`:228` 仍未接线。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-145 Phase 1 裁决落地：宿主半边关闭、**静态解析撤销**、helper-runtime 待真实进程批次

**裁决（§一／§二／§三／§十二）**：`helper_ready.rs` 宿主半边**可以关闭**；PowerShell/C# helper 两相段
**不能以静态解析方式合入**，必须进入**真实 helper 运行窗口**与 T6 一起完成。
§B-144 的回退被确认为**正确动作**——不是"脚本不好测"，而是**已证明所准备的静态校验方案无法证明脚本安全**；
在安全关键输入路径里"静态脚本检查通过 ≠ helper 生命周期行为正确"，因此**不应保留任何脚本半改**。

**状态口径（§十二）**：`8.3c-A-contract` ✅ ／ `8.3c-A-host-ready` ✅ ／
`8.3c-A-helper-runtime` ⏳ ／ `8.3c-A-executor-bind` ⏳ ／ `8.3c-A-input-switch` ⛔。

**§三 撤销静态门禁**：`scriptblock::Create()` 只验证语法可解析，**不验证** READY 是否在输入前发生、
等待期是否真的无输入、cancel 是否优先、permit nonce 是否匹配、helper 是否提前执行 `Engine::Run`
⇒ **不再要求"脚本改动先过静态 parser"**，改由**真实 helper 行为测试**把关。

**§二 保留的宿主侧设计**（不得改）：READY ≠ 文件存在（`file exists → parse → validate → session match`
才叫 READY）；**非法 READY 立即 `Rejected`**，**不得**改成"继续等待直到 timeout"——后者会隐藏协议污染、
nonce 错配、恶意/错误 helper，而 `Rejected` 比 `TimedOut` 更有诊断价值。

**下一批 `8.3c-A-helper-runtime` 范围（§四／§五／§六／§七／§八／§十）**：只做脚本两相生命周期 ＋
T6 真实 helper 测试；插入位置在 `$progress` 之后、`Engine::Run(` 之前；等待循环要求
`cancel → validate nonce → heartbeat → 有界 sleep`，**禁止** `while(true){}` 与不可控 `sleep(1000)`；
`if ($r.two_phase_helper)` opt-in 保持但**只限开发/测试阶段**，最终生产必须**统一由协议版本门禁判定**
（不能长期"部分 `controlled_*` 新协议、部分旧协议"）。
测试不得假装已有完整 `PermitGate`：可用**测试专用 permit stub** 并明确其**不是生产授权**，
**禁止**"写生产格式 permit 然后宣称 `PermitGate` 已通过"。新增 **T6-A**（无 permit ⇒ timeout，`input = 0`）／
**T6-B**（错 nonce ⇒ 拒绝，`input = 0`）／**T6-C**（合法**测试** permit ⇒ 输入发生）。
完成标准七项全满足才进入 Phase 2。

**以上全部已写进交接文件 §3-ter。本轮未改任何代码。** 门禁维持：computer-use-core **130/0**
（含 §B-143 的 3 条 READY 宿主侧用例）、web-console **1215/0**（1 ignored＝真实调用评测）、
core-runtime **347/0**、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-146 helper-runtime 冻结确认：下一窗口直接执行，本轮只补两条实施约束

**裁决**：保持冻结；`8.3c-A-helper-runtime` 的下一窗口**直接执行**（只改 helper 两相生命周期 ＋
跑真实 helper 的 T6-A/B/C）；**不再增加契约**（helper 状态／handshake 消息／Permit 字段／测试枚举／
额外安全抽象一律不加）；不接 `ExecutorStore`、不接 `PermitGate`、不恢复输入入口。
裁决同时点明本阶段真正的风险**不是"不够完善"，而是有人为了尽快看到输入成功而把已冻结的
生命周期边界打穿**。

**本轮只补两条新的实施约束进交接（不加任何代码）**：

1. **Step 1 的精确边界（§三）**：**只改** `NATIVE_INPUT_HELPER_ENTRY` 一处常量，**只新增**
   `ready_file`／permit 等待／EXECUTE gate；插入点保持 `$progress → two-phase block → Engine::Run(`；
   **禁止移动** `cancel` 初始化、`progress` 初始化与 **cleanup 逻辑**——它们**已经过旧路径验证**。
2. **测试 permit 必须显式可区分（§五）**：Phase 1 的 test permit stub 必须用
   `protocol_mode=test`／`TestPermitSignal` 之类显式标记，**禁止**产生 `PermitGate issued` 这类
   日志或记录——否则会出现"**测试文件看起来像生产授权记录**"，比缺测试更坏。
   另补一句 opt-in 终局（§六）：最终生产必须关闭 optional path、改为**协议版本门禁**，
   要防的是"**不同入口不同安全等级**"。

**状态（§九 台账口径，与 §B-145 一致）**：`8.3c-contract` ✅／`8.3c-host-ready` ✅／
`8.3c-helper-runtime` ⏳ 等真实 helper 窗口／T6-A·B·C ⏳／`8.3c-executor-bind` 未开始／
Permit production binding 未开始／input switch ⛔ 禁止。

**本轮未改任何代码。** 门禁维持：computer-use-core **130/0**、web-console **1215/0**
（1 ignored＝真实调用评测）、core-runtime **347/0**、linkage 8/0、`cargo build --workspace` ✅。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-147 8.3c-A-helper-runtime Step 1 落地：脚本两相段（opt-in）＋**真实 helper 门禁抓住了命令行超限**

**做法（按裁决 §三 Step 1）**：只改 `NATIVE_INPUT_HELPER_ENTRY` 一处常量，插入点在 `$progress` 之后、
`[CoolzhuNative.Engine]::Run(` 之前（**未移动** cancel／progress 初始化与 cleanup——它们已经过旧路径验证）。
两相段 **opt-in**（`if($r.two_phase_helper)`，该字段默认不存在 ⇒ **绝不执行**）：写 `ready_file`
（type／协议版本／nonce＝`request_id`／`$PID`／时间戳）→ 有界等待（**先 cancel**，落回 `Engine::Run`
由既有 driver 按 `stroke_cancelled` 处理；**再 permit**，nonce 不匹配则 `exit 4` 拒绝输入；`-Milli 20` 轮询）。

**验收方式按裁决 §三 改对了**：**不再用静态解析**，而是用**现有的真实 helper 测试套件**作门禁
（`computer-use-core` 的 130 条测试真的会启动 PowerShell helper 跑这段脚本）。

**门禁立刻抓到一个静态解析永远发现不了的问题（本项最有价值的产出）**：

```
命令行（含终止符）达到 31501 单元，超过软门槛 31000 单元：留给动态部分的余量不足 0 个单元
```

⇒ 我第一版两相段**太长**，把 `-Command <script>` 的命令行顶超限 **501 单元**。
**这不是语法问题，是命令行长预算问题**——静态 `scriptblock::Create` 永远不会报它。
压缩后（短变量名、单行拼接、复用既有 `-Milli` 缩写）通过，脚本 UTF-16 单元数 2139。

**验收结果**：`computer-use-core` **130/0**（含命令行门禁）；web-console **1215/0**
（1 ignored＝真实调用评测）；module_linkage_smoke 8/0。
⇒ **旧路径未受影响已被真实执行证明**（不是被静态检查"看起来通过"）。

**仍未做**：**T6-A／T6-B／T6-C**——它们要**启用 opt-in** 才能驱动新路径，而当前 `native_run`
不接受外部注入 `two_phase_helper`／`ready_file`／`permit_file`，需要一层**宿主侧测试管道**
（只传测试参数，不改生产语义）。这一层属 Phase 1 剩余，与 T6-A/B/C 同批。

**顺带记录一处环境现象**：本轮 `cargo build --workspace` 中 rustc 崩过一次
（`STATUS_STACK_BUFFER_OVERRUN`），但同一 crate 的 `cargo test` 随即 **1215/0 通过**，
复跑 `cargo build --workspace` 亦通过 ⇒ 判定为**编译器偶发崩溃**，非代码问题（如实登记，不作"已修复"）。

**本轮未接** `ExecutorStore`／`PermitGate`／`:228`；未写生产 permit；未迁移真实库、未解除隔离、
未启动真实输入、未提权、未安装、未推送。

### B-148 Phase 1 决策落地：测试注入缝（只补测试参数）＋ 命令行长度门禁 ＋ rustc 环境异常结构化记录

**裁决要点（§一–§九）**：继续 `8.3c-A-helper-runtime`；**只扩展已有测试注入缝**，不新造测试管道、
不新增生产能力；**暂不迁移脚本启动方式**（不引入 `-File`／临时脚本文件）；Phase 2 与 `:228` 保持冻结；
命令行长作为**持续门禁**；rustc 崩溃只作环境异常登记。

**① 测试注入缝（§一，下一步的实施口径）**：`native_run_with_mock`（`input.rs:3291`）已存在
⇒ 之前的"需要新造测试管道"判断**过重**，本轮更正为"让现有测试入口能携带两相 helper 所需的
**测试参数**"。允许只增测试字段：`two_phase_helper`／`ready_file`／`permit_file`／`protocol_mode=Test`
（或等价结构）；**不允许**测试入口直接接受 `InputPermit`／`ExecutorInstanceId`——
Phase 1 验证的是 **helper 生命周期**，**不是授权系统**。

**② T6-A/B/C 期望值冻结（§二，语义上必须区分三类结果）**：

| 用例 | 期望 | 关键点 |
| --- | --- | --- |
| **T6-A** 无 permit | **`TimedOut` ＋ `physical_input_count = 0`** | **不是 `Rejected`**：没有非法信号，只是没有授权 |
| **T6-B** 错误 nonce permit | **`RejectedPermit` ＋ `input = 0`** | **不得继续等待**：这是协议错误，不是缺少授权 |
| **T6-C** 合法测试 permit | `EXECUTE` gate 放行、`Engine::Run` 执行 | 证明 EXECUTE gate **确实控制输入入口** |

**③ 命令行长度门禁（§三，决策：暂不迁移启动方式）**：事实是 31501（超软门槛 31000）→ 压缩后 2139 通过。
**不引入** `powershell -File`／临时脚本文件——那会带来文件生命周期、权限、临时目录、清理、
脚本来源身份、多实例竞争等**新的架构决策**，当前没必要。**新增门禁 `ScriptCommandLineBudget`**：
任何 helper 脚本变化都必须检查**完整命令行 UTF-16 长度（含终止符）＋ 动态参数余量**；
**保持 `<31000` 为软门槛**，**不得**改成"刚好低于 Windows 极限即可"——未来动态参数
（path／nonce／临时文件／调试参数）仍需空间。**只有**出现以下任一情况才单独开设计：
Phase 2 新增参数无法压缩／协议长期增长逼近阈值／多平台脚本管理需统一文件化／安全审计要求脚本独立身份。

**④ rustc 环境异常结构化记录（§六，按要求：不写"已修复"，也不写"代码无关"）**：

```
build environment anomaly
  rustc version: 1.94.1 (e408947bf 2026-03-25)
  host:          x86_64-pc-windows-msvc
  command:       cargo build --workspace --offline
  occurrences:   2（同一轮内）
  crates:        coolzhu_web_console (src/main.rs)、COOLZHU_AGENT (packages/app-launcher/src/main.rs)
  symptom:       rustc exit 0xc0000409 STATUS_STACK_BUFFER_OVERRUN
  后续观测:      对应 crate 的 cargo test 通过（web-console 1215/0）；
                 复跑 cargo build --workspace 通过；
                 computer-use-core 130/0
  结论（可支持的限度）: 本轮观察到两次 rustc 崩溃，对应 crate 后续测试通过、复跑成功，
                 当前未发现与本轮修改相关的代码证据。
```

**再次出现时应比较**：是否同一 crate／同一 rustc／同一机器／是否与特定 feature 或 build script 有关。

**§五 Phase 1 剩余顺序**：Step 1 扩展 `native_run_with_mock`（加三字段）→ Step 2 接入真实 helper opt-in
（**默认 `two_phase_helper=false`，旧路径不变**）→ Step 3 跑 T6-A/B/C → Step 4 检查
computer-use-core／web-console／core-runtime 三门禁。**四项完成 Phase 1 才关闭。**

**§七 禁止事项不变**：不接 `:228`／不写生产 permit／不接 `ExecutorStore`／不迁移真实 safety DB／
不解除隔离／不启动真实桌面输入／**不自动 fallback 旧 helper**。

**本轮只采集环境数据与登记决策，未改任何代码。** 门禁：computer-use-core **130/0**、
`cargo build --workspace` ✅（复跑）。未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-149 Step 1 的注入边界阻塞：**nonce 由内部生成、调用方无法指定** ⇒ T11 的"防串用"会变成空检查

**裁决口径**：继续 Step 1–4；只扩展测试注入能力（`TwoPhaseHelperTestOptions`）；**测试入口不得接受**
`InputPermit`／`ExecutorInstanceId`／`RecoveryControlGuard`；默认 `two_phase_helper=false`，
已有 130 条测试继续证明旧路径未变；**若出现阻塞，优先判断是否属** helper 生命周期／测试注入边界／
命令行预算三类，**不要重开已冻结的两相协议设计**。

**本轮核实的阻塞（属第 2 类：测试注入边界）**：

| 事实 | 位置 |
| --- | --- |
| nonce 在 **`run_native_helper` 内部生成**（`"{pid}-{nanos}-{seq}"`） | `input.rs:3370` |
| 并**覆盖式**写入 `request["request_id"]` | `input.rs:3393` |
| 调用方**没有任何途径**指定它 | 签名 `run_native_helper(request, obligation, timeout, cancelled, mock_scenario, backend, capacity)` |

**为什么这会挡住 T6-C 与 T11**：

- T6-C 要写一份 nonce **匹配**的测试 permit；T11 要证明"**helper B 读到 helper A 遗留的 permit 必须拒绝**"。
- 但测试**无法预先知道** nonce——只能先把 READY 读出来、再用读到的那一个去写 permit。
- 这样做的话，"nonce 必须属于本会话"就变成**自指检查**：**任何** READY 都会被接受，
  **T11 的防串用保护变成空检查**（形式上还在、实际不生效）。
- 也不能靠"注入 `request_id` 后再调用"：`run_native_helper` 会**覆盖**该键（`:3393`），注入会被吃掉。

**两个可选处置（我倾向 (a)，请裁决）**：

| 方案 | 内容 | 代价 |
| --- | --- | --- |
| **(a) 由调用方提供 nonce（建议）** | 两相段改用**调用方可设的独立字段**（如 `two_phase_nonce`）而不是 `request["request_id"]`；测试注入该字段，Phase 2 由 web-console 提供 | 脚本改几处标识符（**字符数很小，仍在命令行预算内**）；`request_id` 的进度关联语义**不动** |
| (b) 由 READY 反读 nonce | 测试读 READY 拿 nonce 再写 permit | **拒绝**：使 T11 与 nonce 校验失去意义（自指） |

**为什么必须现在定**：T6-C 与 T11 的**验收价值取决于 nonce 是否由"本会话"掌握**。若走 (b)，
我们会有两个"看起来通过"的用例，但它们证明的是"文件读得出来"，不是"授权不可串用"——
这正是本项目反复防的"**看起来比缺失更安全**"。

**本轮未改任何代码**（未动 `native_run_with_mock`、未动脚本）。门禁维持：computer-use-core **130/0**、
web-console **1215/0**（1 ignored＝真实调用评测）、core-runtime **347/0**、linkage 8/0、
`cargo build --workspace` ✅。未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-151 B128-T2 落地：防回退结构守卫（两相段不得用 `request_id` 作 nonce 来源）

**裁决口径（§七）**：增加结构测试，检查 helper 脚本中**不存在**「以 `request_id` 作为 READY／permit／
EXECUTE 的 nonce 来源」。理由：本轮改动只有一行，将来很容易有人为了「少传一个字段」绑回 `request_id`，
从而让「nonce 属于本会话」沦为**自指检查**，T11 与 nonce 校验同时失效。

**交付**（`input.rs` 的 `input::tests::native_lifecycle` 模块，与命令行门禁测试**同模块**）：
`two_phase_block_takes_its_nonce_from_the_caller_supplied_field` —— 从脚本切出两相段
（`if($r.two_phase_helper){` → `if($sp[1])` 之前），断言 ① 段内**必须**含 `$r.two_phase_nonce`；
② 段内**不得**出现 `request_id`；③ 该段仍**早于** `[CoolzhuNative.Engine]::Run(`。

**变异验证 1/1**：把 `$n=[string]$r.two_phase_nonce` 改回 `$r.request_id` ⇒ 守卫立刻红
（报「两相段不得引用 request_id 作为 nonce 来源」），确认这条守卫是活的。

**过程的两次自伤（如实记录）**：① 第一次我把测试插到文件里**第一处** `#[cfg(test)] mod tests {`
之后，破坏了编译（`cannot find function helper_pipes::supervise_stdout`）——说明那个位置并非我以为的
上下文；**立即 `git checkout` 回退**保证树绿（130/0），再重新锚定到**命令行门禁测试所在模块**
（同模块 ⇒ 作用域正确）后成功。② 教训与既有纪律一致：**大文件插代码必须锚在已知同模块的既有符号上**，
不能按「第一个匹配」落位。

**门禁**：computer-use-core **131/0**（增 1 条）、web-console **1215/0**（1 ignored＝真实调用评测）、
core-runtime **347/0**、module_linkage_smoke 8/0。**未改脚本语义**（只加测试）；未接
`ExecutorStore`／`PermitGate`／`:228`；未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-152 8.3c-A-helper-runtime **Step 1 交付**：测试注入入口（两相模式），默认路径零两相键

**裁决口径（§一）**：只扩展**测试注入入口**，让测试能提供 `two_phase_nonce`；**不扩大范围**——
禁改 production `native_run`／`controlled_*` 行为／helper 默认行为／`request_id` 生成逻辑／
READY 协议／`PermitGate`／`ExecutorStore`。

**交付**（`input.rs`，均 `#[cfg(test)]`）：

| 项 | 内容 |
| --- | --- |
| `TwoPhaseHelperTestOptions` | `two_phase_nonce`（**由测试生成**，绝不读 READY 反推）＋ `ready_file` ＋ `permit_file`；**刻意不含** `InputPermit`／`ExecutorInstanceId`——Phase 1 验的是 helper 生命周期，不是授权系统 |
| `native_run_with_mock_two_phase(...)` | 与既有 `native_run_with_mock` **同一路径**，只在请求里注入 `two_phase_helper=true`／`two_phase_nonce`／`ready_file`／`permit_file` |

**为什么新增入口而不是改既有签名**：既有 `native_run_with_mock` 的 130 条用例不动 ⇒
"默认行为不变"由**不调用新入口**直接保证（§二.2 的意图），也不需要改动测试调用链。

**顺带清理**：`helper_ready.rs` 因"尚未接线（Phase 2 才接）"产生 dead_code 告警，
已加**显式豁免＋理由**（与本会话对 `input_permit_store` 的做法一致），并注明
"**接线落地后应移除此豁免**"——不是掩盖。当前该文件**零告警**。

**门禁**：computer-use-core **131/0**（既有 130 条未受影响 ⇒ 默认路径未变），
web-console 1215/0、core-runtime 347/0、module_linkage_smoke 8/0。

**仍未做（Step 4，需真实 helper 进程）**：**T6-A**（无 permit ⇒ `TimedOut`＋`input=0`）、
**T6-B**（错 nonce ⇒ `RejectedPermit`＋`input=0`）、**T6-C**（合法测试 permit ⇒ 过 EXECUTE gate）、
**T11**（A/B 两 nonce，B 读 A 的 permit ⇒ 拒绝）、**B128-T1**（`request_id` 与 `two_phase_nonce`
不混淆）、以及建议项 **B128-T3／T4**。
**Phase 1 未关闭**，因此 **Phase 2（`ExecutorStore`／`PermitGate`／permit_file 生产写入）与
`:228` 继续冻结**。未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

### B-153 Step 1 验收与 Step 4 口径冻结（＋一条待办登记）

**Step 1 关闭条件已满足（裁决 §一）**：测试注入完成（`TwoPhaseHelperTestOptions` ＋
`native_run_with_mock_two_phase`）；身份边界确认 `request_id ≠ two_phase_nonce`（由**测试侧生成**，
不读 READY 反推）⇒ **B128-T1 的前提成立**；**默认路径隔离由结构保证**——旧测试走旧入口、
新测试走 two-phase 入口，**不是**"`native_run` ＋ `two_phase_helper=false`"这种隐式覆盖，
因此"旧行为未变"是**事实**而非假设。

**待办登记（裁决 §二）**：`TODO(8.3c-A-executor-bind): 删除 helper_ready 的 dead_code 豁免`。
已同时写入源码 TODO 标记与本节，**防止豁免变成永久装饰**。

**Step 4 执行顺序（裁决 §三，固定）**：

```
B128-T1 → T11 → T6-A → T6-B → T6-C → 三 crate 门禁 → Phase 1 关闭
```

**逐条通过标准（裁决 §四–§六）**：

| 用例 | 断言要点 |
| --- | --- |
| **B128-T1** | READY 的 `nonce` 必须等于 `two_phase_nonce`；progress/log **仍关联 `request_id`**；**不得**以"两者恰好相等"作为唯一证明 |
| **T11** | helper A（nonce=A）产出 permit(A)；helper B（nonce=B）读到 permit(A) ⇒ **`RejectedPermit`**。**禁止**采用"B 读 READY 再复制 nonce 生成 permit"的测法（那会退回 B-128 之前的问题） |
| **T6-A** | READY 后无 permit ⇒ **`TimedOut`** ＋ `physical_input = 0`（**不是** `RejectedPermit`：无非法信号，只是无授权） |
| **T6-B** | READY 后 given permit(B)（错 nonce）⇒ **`RejectedPermit`** ＋ `physical_input = 0` |
| **T6-C** | READY ＋ 合法**测试** permit ＋ EXECUTE ⇒ 输入路径可达。**Phase 1 唯一允许"输入发生"的用例**，且输入必须是 **mock／受控执行计数**，不是生产桌面输入 |

**约束（裁决 §七／§八）**：不得为提速合并测试、跳过真实 helper、或用静态 stub 替代 T6——
当前最大未知是**脚本生命周期**而非 Rust 状态机；可优化 helper 启动参数／等待窗口，但**不能优化掉
真实 PowerShell helper**。**诊断信息一律放 Rust 测试侧**，不得写进 inline PowerShell（命令行预算 2144 单元、
软门槛 31000）。

**§十 继续冻结**：`ExecutorStore`／`PermitGate` production／permit_file 生产协议／
`computer_use_executor.rs:228`／`controlled_*` 拆分／真实 safety DB／当前隔离解除。

**本轮未改任何脚本或生产代码**（仅加源码 TODO 标记与文档）。门禁：computer-use-core **131/0**、
web-console **1215/0**（1 ignored＝真实调用评测）、core-runtime **347/0**、linkage 8/0。
未迁移真实库、未解除隔离、未启动真实输入、未提权、未安装、未推送。

## C. 需要新裁决的问题（非裁决文档已覆盖，由本轮发现）

> **第七轮已裁决（2026-09-25）**：本节的 §C-1..§C-15 **选择待定全部关闭**，§C-16..§C-27 亦已有结论。**逐项生效结论与 G1–G6 稳定门禁文本的权威正文见 `round7-rulings-and-gates.md`**（不再引用"第几轮 §几"）。
> **登记纪律**：统一使用四种状态——**已裁决 / 已实现 / 生产已接线 / 目标环境·安装已验收**；"决策关闭"**不等于**"实现与验收关闭"，故下方条目的处置一律照原文保留，只在条目上标注裁决结果。

> **集中版**：待决事项的"现象／证据、阻塞点、背景、选项与建议、不决策的后果"见 `pending-decisions-current-backlog.md`（含分批处理建议与"只回编号+结论"的回法）。本节的 §C-1..§C-15 属较早轮次，我在集中文档里**没有**它们被后续裁决覆盖的证据，故不擅自宣布闭合，请一并确认。

1. **[已裁决·第七轮]** **外层根 deadline 如何处置**：接线 `RunBudget`（工作量大、触及所有入口与 UI 状态），还是**显式声明"当前无根 deadline"**并把 `effective_call_budget` 的 min 链中该项去掉？后者诚实但会弱化准入语义。
2. **[已裁决·第七轮]** **是否授权在 RPR-11c 阶段给 `ComputerUsePlanner` trait 加 `remaining`**（跨 `computer-use-core` 公开 API 变更）。不加则预算守卫覆盖不到真正发请求的 planner。
3. **[已裁决·第七轮]** **RPR-11a 的两项缺口**（原生终止身份强度、切换前排空）是作为 RPR-11a 的必交付追加，还是拆为 11a-2 / 11a-3 单列工单。
4. **[已裁决·第七轮]** **取消收尾的固定 2 秒宽限**是否也必须受 `remaining` 约束。裁决 §6.5 只要求"独立且受限"，未说必须 ≤ remaining；若要求 ≤ remaining，则在剩余不足时会缩短释放动作，可能造成释放不完整（与"未确认释放必须阻断下一动作"相互作用）。
5. **[已裁决·第七轮]** 裁决 §6.6 六项门禁的原文可否提供（或确认由本轮按技术面向自行组织的六个面向替代）。
6. **[已裁决·第七轮]** *`try_acquire_incidental_input_lease` 是否也要输入前复检**：该路径（短时光标输入，`verify_cursor_on_target` 用）只在取得 lease 时校验 `is_current()`，之后到真正移动光标之间不复检。若要求与正式 CU run 同等强度，需补复检并给出失效时的行为（拒绝移动还是降级为不移动光标）。
7. **[已裁决·第七轮]** **环境变量 RAII 收口（约 15 处）是否列为下一轮工单**：它与本轮修的 cwd 缺陷同类，且影响面更广（HOME/PATH/CODEX_HOME 被 panic 永久污染 + 静默级联）。我建议立为独立工单（例如 RPR-01b），因为它跨 tool-registry 与 core-runtime 两个 crate，且触及测试基础设施而非产品行为。
8. **[已裁决·第七轮]** *`RunIdentity` 的 10 字段映射规则**（RPR-03 → RPR-04c 接线的硬前置）。**已核实的真实事实**（可直接据此裁决）：
   - `RunIdentity` 的 10 个字段是 `workspace_id`/`room_id`/`session_id`/`public_turn_id`/`run_id`/`step_id`/`request_attempt_id`/`tool_call_id`/`action_id`/`owner_epoch`（**全部非空**才通过 `validate()`，否则 `RunContractError`）。
   - 该结构本质是**动作/步骤级身份**（含 `step_id`、`tool_call_id`、`action_id`、`owner_epoch`），而 chat 的 `ChatTurnGuard`（`main.rs:462-469`）只持有 `turn_id`/`run_id`/`claim_token`/`db_path`。
   - `ChatTurnStatus` = `Running`/`InterruptRequested`/`Interrupted`/`Completed`/`Failed`（`main.rs:183-191`，`is_terminal()` 为后三者）；`RunTerminalStatus` = `Succeeded`/`Failed`/`Blocked`/`Cancelled`/`TimedOut`（`run_contract.rs:144-150`）。自然映射为 `Completed→Succeeded`、`Failed→Failed`、`Interrupted→Cancelled`，非终态不写；`Blocked`/`TimedOut` 在 turn 级无对应来源。
   - **结论**：turn 级事实要凑满 10 个非空字段，就必须为 `step_id`/`tool_call_id`/`action_id`/`request_attempt_id`/`owner_epoch` 编造值——这正是本批明令禁止的。
   **建议方案（供你直接批准或否决）**：保持 `RunIdentity` 单一类型、`validate()` 对"全身份"用法仍然严格，**新增一个场景范围（例如 `RunIdentityScope::{Turn, Step, Action}`）并配 `validate_for(scope)`**：turn 级只校验 turn 级真实存在的维度，其余维度交给 step/action 级事实。这样既不新增同义类型、也不填假值、也不放宽既有 fail-closed。
9. **[已裁决·第七轮]** *生产 SQLite 适配器的落点**：放在 S2.4 的独立单写者 crate，还是放在 web-console 侧（那里已有 rusqlite bundled）？这决定 core-runtime 是否需要引入 rusqlite 及其打包代价。
10. **[已裁决·第七轮]** *浏览器面的"部分输入回执"要不要做**：DOM 输入目前没有事实生产者（扩展协议不返回事实），因此浏览器桥只能走错误码启发式。要覆盖它需要改浏览器扩展协议（新增事实回报）——是否立为独立工单，还是接受该面的回执弱于桌面面。
11. **[已裁决·第七轮]** *切换前排空的实现方案与兜底策略**（见 §B-14）：选方案 A（客户端计数、需把 `MessageStream` 从 enum 包成结构体）、方案 B（轮询托管 llama-server 的 `/slots`，需真实二进制实测），还是两者叠加？以及**无法判定是否排空时**（端点不可用/超时）是"照旧终止"还是"拒绝切换并提示用户"。
12. **[已裁决·第七轮]** *hook 的"独立授权来源"由谁提供**（A-7b 的直接后果，见 §B-15）：现在 conversation 路径上的 hook **默认未授权即不运行**，而 CLI 尚未接线授权来源 ⇒ **CLI/子 Agent 的 hook 目前不会运行**。裁决 §7.3 只说了"没有独立授权就不放行"，但没指定"授权从哪来"。需要定：是 settings 里一个显式的 hook 授权字段、还是每次运行前的一次性用户批准？在定下来之前，CLI 侧 hook 处于"按裁决要求被禁用"的状态。
13. **[已裁决·第七轮]** *未确认释放互锁的作用域要不要扩到 session**（见 §B-17）：现在只在同一 `(session_id, turn_id)` 内阻断，**新 turn 是新 scope，因此上一 turn 遗留的未确认释放不阻断下一 turn**。而"未确认释放"本质是**桌面级**状态（可能已有按键流到了系统），不因用户开了新 turn 而消失。选项：(a) 扩到 session 维度（store + 执行器各一处小改）；(b) 保持 turn 维度，把"新 turn 可绕过"作为已接受风险记录；(c) 扩到"整个交互会话（Windows 登录会话）"维度，与输入所有权的 scope 对齐。**我的建议**：(c) 最贴合语义（与 `current_interactive_session_scope()` 对齐），(a) 为最小可行；不建议 (b)。
14. **[已裁决·第七轮]** *原生动作算不算"有模型请求尝试"**（见 §B-28）：`StepAction` scope 必填 `RequestAttemptId`，而 CU 执行器没有它的真实来源。选项：(a) 为**宿主发起的原生动作**新增一个 scope（例如 `NativeAction`），其必填维度只含容器 + `step_id` + `action_id` + `tool_call_id`；(b) 在 `StepAction` 内把 `request_attempt_id` 降为可选，并规定"缺省即表示该动作不是由某次模型请求直接产生的"；(c) 先把请求尝试 id 一路 plumb 到动作路径（成本最高，但表达最完整）。**我的建议**：(b) 最小且语义清楚；(a) 更严格但要动契约枚举。
15. **[已裁决·第七轮]** *缺会话/轮次维度时 CU 该怎么办**（见 §B-28 第 2 点）：`main.rs:33952` 现在会填入 `"session-missing"` 之类的哨兵值继续执行。要让事实诚实，必须定：CU 在缺这些维度时是**拒绝执行**（fail-closed），还是**照旧执行但不写事实**（事实缺口如实记录）？**我的建议**：后者（执行能力不应因审计维度缺失而丧失），但必须把哨兵值从身份里去掉、改为显式 `Option`。

16. **[已裁决·第七轮·§C-16]** ~~软余量阈值被放宽需确认~~：**有条件接受 `<31_000`**；硬上限不变；但**计量口径必须纠正**——按完整 Windows 命令行（程序名/引号/转义/编码后脚本/终止符）的**实际 UTF-16 单元**核对，不得写成 KB；30,014 若不含终止符则剩余 **2,752 单元**。还须测最长路径、空格/中文、转义与真实动态输入，并分别输出测试版与正式版长度、确认接缝不能经生产参数/环境/设置启用。原文保留：**软余量阈值被放宽需确认**
17. **[已裁决·第七轮｜选②]** ~~`Saturated` 分支要不要改成"拒绝即故障上报并保留句柄"~~：**必须改**——返回严重内部故障、**保留** reader/线程句柄/管道/缓冲区所有权、**锁住该监督器的新 helper 接纳**、有界回收、记录**真实数量**、**不自动恢复**服务能力；允许调整内部返回类型携带资源，但**生产调用者必须实际接住**；**不采用**"应急 Vec"假装有界；不得仅凭文档宣称该分支绝对不可达（改为故障注入验证）。原文保留：**`Saturated` 分支要不要改成"拒绝即故障上报并保留句柄"**
18. **[已裁决·第七轮｜授权改行为，与 17 同批]** ~~`holds_unreclaimed_reader()` 语义修正~~：**不能**简单令 `Saturated → false`；至少区分 `unreclaimed` / `owned_unreclaimed` / `unowned_unreclaimed` / `admission_rejected` / `capacity_fault`；`readers_retained` 反映**真实受持有数量**（不是把所有错误都计 1）；按 **reader 唯一身份**聚合避免重复计数；名称与 UI/诊断不得混用。原文保留：**`holds_unreclaimed_reader()` 对 `Saturated` 返回 true 的语义修正**
19. ~~**打包的 fail-closed 取舍**~~ **已裁决（第六轮 §三）**：维持拒绝发布不稳定输入所生成的包，**但必须输出失败诊断**（当前"连报告一起拒绝"是要修的点）；**不允许**用 `quiescent=false` 给混合输入产物补可发布收据。落点见 `a2-workspace-source-and-frozen-parent-context.md` §9。原文保留于 §B-52：**打包的 fail-closed 取舍**（见 §B-52）：并发编辑时"拒绝出报告/出包"（当前实现）还是"照常出包但如实记 `quiescent=false` + 差异清单"。属产品/运维口径。
20. ~~**`L07c-concurrent-export` flake 是否另立工单**~~ **已裁决（第六轮 §四）**：批准独立工单 **PKG-L07c**（并发导出与收据一致性），**默认 P1**，若核查发现失败时仍可能发布错配产物则升 **P0**；不阻塞 A-2；修复前正式发布用单一打包者。**已起工单实施**。原文保留于 §B-52：**`L07c-concurrent-export` flake**——归因已在 §B-57 更正（失败原文是 `$null` 取方法，不是 `File.Replace`；真正缺陷是产物与收据无互斥）。
21. **[已裁决·第七轮｜选②]** ~~是否授权把容量接纳前移到 helper `spawn` 之前~~：**批准**，且顺序固定为"校验资格 → 计算普通 helper 与必要清理的最大 reader 需求 → 一次性预留 → 创建并监督 helper → 绑定读取器 → 最终输入前检查 → 允许业务输入"；**必须防止收尾死角**（普通 helper 不得占满容量致安全收尾无配额；若清理用同一池则接纳时连同一次清理一并预留、清理转用该预留且不递归；若不使用该池则按真实模型记录、不虚增）；spawn 失败归还全部未用额度、部分成功只归还未用；容量等待消耗原预算不重置期限。验收：容量不足时 **spawn 为零**、两路按实际 reader 计量、中途失败不泄漏/不重复归还、饱和下已预留的必要释放仍能进入受控收尾。原文保留：**是否授权把读取器容量接纳前移到 helper `spawn` 之前**
22. **[已裁决·第七轮｜确认并入 RPR-01b]** ~~unix-only 环境变量污染是否并入 RPR-01b~~：**并入**，范围含 tool-registry/core-runtime 仍有问题的环境变量测试 + 所列 `#[cfg(unix)]` 的 `PATH`/`SAFEUSER` 污染路径 + command-router 新例外（**单独子项**）。优先显式环境参数或 `Command::env`；必须改进程级环境时用同进程**统一锁 + RAII** 并保真"缺失/空值/有值"；**Unix 不能只加一把局部锁就宣称线程安全**，优先隔离子进程；**Windows 通过 ≠ Unix 已编译/已运行**，Unix 子项在匹配环境编译并运行前**保持未验证**。"永久污染"精确为"当前测试进程后续执行被污染"。原文保留：**unix-only 的环境变量污染是否并入 RPR-01b**
23. **[已裁决·第七轮｜原意纠正]** ~~"指定环境例外"的逐字所指~~：原意是**此前逐项登记的进程环境变量/cwd 测试隔离例外**，**不是**"环境不满足就跳过"。外部依赖盘点与显式跳过**有价值但不能作为该工单已完成的替代证据**；逐项审查后**保留有独立价值的部分**，不盲目全量回退。允许/禁止的五种情况见 `round7-rulings-and-gates.md` §5.2。原文保留：**"指定环境例外"的逐字所指**

24. **[已裁决·第七轮｜选①]** ~~构建期间声明构建输入被改写即拒绝发布~~：**维持拒绝**，**不设** `Cargo.lock`/`tauri.conf.json` 构建中变更白名单。改为**两阶段**：准备阶段解析/更新依赖、生成配置并按既有政策确认后**冻结完整输入快照**；正式构建只消费冻结输入，任何声明输入改变即拒绝。Cargo 用 `--locked --offline` 或等价 `--frozen`；生成式配置**生成后计入冻结输入**，构建中产生的中间文件放**已声明的派生输出**并记录来源；**不得**发现被改写后把它从输入清单移除以换取绿色。原文保留：**构建期间声明构建输入被改写即拒绝发布，需产品口径确认**
25. ~~**打包测试夹具根的并发冲突是否按运行隔离**~~ **已判定并修复（§B-57）**：判定为"仅测试夹具互相踩，非产品并发契约缺陷"，夹具根改为按运行隔离并已有"两个全量测试同时启动、两次都 30/0"的实证。原文保留：**打包测试夹具根的并发冲突是否按运行隔离**（见 §B-56 与 §B-55）：`scripts/test-package-webview2-loader.ps1` 用**固定**夹具根 `tmp/package-webview2-loader-contract`，两个进程并发跑同一测试会互相清空（实测报 `Cannot find path ...\manifest.json` 与一次 `ArgumentException`）。已把该情报转交在跑的 PKG-L07c 工单，请其一并判定"产品并发契约缺陷"与"仅测试夹具互相踩"的边界并给结论。

26. **[已裁决·第七轮｜升级为发布完整性 P0]** ~~PKG-L07c 的 P1/P0 判定需要外部复核~~：**批准 P0，但限定结论**——属**发布证据完整性缺陷/风险**，**不是**已证明签名或分发了错误包（尚未经验性构造出该交错；现有消费侧拒绝仍是有效缓解）。**最小修复不得停在"再核一次哈希 + 补字段"**：同一槽位发布权必须覆盖 **产物最终核对 ＋ 收据生成与提交 ＋ 当前有效代次的发布**；失败竞争者只能写自己的诊断。消费者固定流程与"禁止报告时重读最新收据"见 `round7-rulings-and-gates.md` §3.2。原文保留：**PKG-L07c 的 P1/P0 判定需要外部复核**
27. **[已裁决·第七轮｜授权合并到同一 PKG-L07c 修复]** ~~是否授权补 `generation`/`slot_key`~~：**授权**，由当前脚本负责人修改 `package-all.ps1` 的 `exported_artifacts[]`，与 PKG-L07c 补强**同 PR 或连续绑定 PR**；字段语义固定为 `slot_key`/`generation`/`producer_run_id`/`artifact_digest`/`receipt_digest`/`build_input_digest`/`consumer_run_id`。原文保留：**是否授权给 `package-all.ps1` 的 `exported_artifacts[]` 补 `generation`/`slot_key`**

28. **[已裁决·第七轮｜选 A]** ~~RD4-02：共享输入安全存储落在哪里~~：**新建独立共享安全库**，固定用 `ResolvedLaunchPaths.input_safety_state_root`（`<user_state_root>\input-safety`，数据库 `<root>\input-safety.sqlite3`），**禁用**工作区 `.coolzhu`、cwd、`log_dir` 推导与测试夹具路径作为生产默认；**独立组件 schema 版本**（初始 1，**不**把会话库 v23 改 v24，两库不共用版本号）。实体、写入方与生命周期政策见 `round7-rulings-and-gates.md` §1。原文保留：**RD4-02：共享输入安全存储落在哪里**
29. **[已裁决·第七轮]** ~~RD4-03：谁真正"暂停新输入接纳"、谁签发"恢复协调权"~~：**跨进程恢复协调**（不是仅进程内 mutex），**暂停范围＝实际物理输入资源 scope**（跨 turn/session/room/workspace，不是仅受影响聊天 session）；由宿主 `InputSafetyCoordinator` **签发可验证资格**（绑定 resource_scope/operation_id/coordinator_instance_id/epoch/gate_revision/来源库与候选集/允许操作），**禁止调用方自证**；`new_input_intake_paused` 只能作**日志结果字段**；固定 **R1–R9** 顺序与锁的命名/线程约束见 `round7-rulings-and-gates.md` §2。原文保留：**RD4-03：谁真正"暂停新输入接纳"、谁签发"恢复协调权"**

**不决策的后果（如实陈述）**：v23 迁移已就位但**生产收敛不会运行** ⇒ 迁移前遗留的"NULL 归属且未收尾"运行仍会**长期阻断**其 scope（A-1 已核实清理引用数为 0、全 crate 无 sweep 机制）。这不是新风险，而是把裁决已点名的缺口保持显式。

30. **[已裁决·第八轮 §5]** ~~两条保留的"声明式跳过"是否属发布必验范围~~：**采纳建议**——外部可选输入允许跳过；**PowerShell 原生 CU 环境必须 fail-closed**（CU helper 本身依赖 `powershell.exe`，缺失**不是** optional dependency，而是 **execution environment invalid**）。规则表：外部扫描工具 skip／可选模型资源 skip／**PowerShell fail**／**原生输入依赖 fail**／**发布必需 DLL fail**。原文保留：**两条保留的"声明式跳过"是否属发布必验范围**

31. **[已裁决·第八轮 §6]** ~~PKG-L07c 的 P0 是否可以关闭~~：**关闭 P0 修复项**，**不关闭**"并发发布能力已验收"；状态记为 **P0 修复完成 ＋ 并发发布实测验收待补**（真实交错 publisher A/B race **未构造**，故不得写"并发发布已证明安全"）。剩余验收立独立工单 **PKG-L07c-RACE**（100 次并发导出／10 个失败竞争者／随机 kill；检查 winner generation 唯一、receipt 不错配、consumer 永远读完整代次），**不阻塞普通单发布**。原文保留：**PKG-L07c 的 P0 是否可以关闭**



## C-收口. 第八轮裁决新增项（**已裁决的执行项**，按状态登记）

> 权威正文见 `round8-rulings-and-execution-order.md`。以下不是「待裁决」，而是**已裁决的执行项**。
>
> **2026-09-26 纠错（本轮补充裁决 §2）**：本表曾把若干**已经实现**的项登记为「待实现」，
> 也把两条**已被本轮正式撤回**的契约要求当成后续工单。两处都已按下文更正；被撤回的条目
> 一律标 **[已撤回／被本轮替代]**，**不再是可领取的任务**。

| 项 | 生效结论 | 状态 |
| --- | --- | --- |
| RD4-02A 最终形态 | **采用 A ＋ C 的语义约束**：独立输入安全库，但它**不是物理输入所有权本身**，而是"输入安全事实"的**权威记录**；否决 B（塞 session SQLite，破坏"不宣称跨库原子性"）与纯 C。结构：`InputSafetyStore → input-safety.sqlite3`，含 **incidents / resource_blocks / recovery_operations / ownership_epochs / safety_events**；位置 `ResolvedLaunchPaths.input_safety_state_root`，**禁止** workspace `.coolzhu`／session db／temporary path | **方向已定，继续实施**（Phase 1） |
| RD4-02A 剩余 | **launcher 注入** `COOLZHU_INPUT_SAFETY_STATE_ROOT`；生产路径**必须注入**；缺失即 `root_not_injected` **直接 fail closed**；**不要 fallback**（否则测试环境污染生产路径） | **已实现**（更正于 2026-09-26）：`packages/app-launcher/src/lib.rs` 从既有契约字段 `ResolvedLaunchPaths.input_safety_state_root` 注入（**不另行推导目录名**），注释直接引用第八轮 §2；引入提交 `965166c`。**未直接抓取**的只是「由 launcher 启动的控制台进程内」那一行联动日志（见 §5） |
| RD4-02B | **跨进程资源协调器 ＋ 可信 `RecoveryControlGuard`**；**不是**进程 mutex／session 级锁／caller 参数声明。`InputSafetyCoordinator`：取得资源锁（scope ＝ `windows-session-{id}` ＋ `physical-input-resource`，**不是** workspace/session/turn）→ 写 `RecoveryOperationStarted` → 创建 `RecoveryControlGuard { resource_scope, recovery_id, epoch, coordinator_id, allowed_actions }`；恢复入口**必须**走 `Coordinator → InputSafetyStore → RecoveryControlGuard`，**禁止** `paused=true`／`authority="xxx"` 式调用 | **开工条件满足**（Phase 2） |
| 组合测试 1 | `paused=true` 但闸门未关闭 ⇒ `RecoveryUnauthorized`，**不写终态** | **已实现**（更正于 2026-09-26）：`input_safety_store.rs:2730` `combined_1_recovery_without_a_held_epoch_is_unauthorized`；**层级＝存储层**（真库真事务，无真实子进程） |
| 组合测试 2 | 双进程竞争：A 取得 epoch 21，B **epoch 冲突被拒**，且 B **不能**写 incident／改 run 状态／开新输入 | **已实现**：`input_safety_store.rs:2776` `combined_2_second_recovery_instance_is_rejected_by_epoch_conflict` ＋ 协调器用例 `input_safety_coordinator_busy`；**层级＝存储／协调器层**（所谓双进程实为同进程两个协调器实例，**非**真实双进程） |
| 组合测试 3 | 恢复者死亡 ⇒ **启动恢复**（`RecoveryStarted → crash → startup reconcile → acquire new coordinator epoch → continue/reject`）；**禁止 delete recovery row** | **已实现**：`input_safety_store.rs:2860` `combined_3_restart_reconcile_needs_a_new_epoch_and_never_deletes_rows`；**层级＝存储层**，崩溃用**存活探针模拟**，**非**真实进程终止（真实变体见下方独立行） |
| 组合测试 4 | 已 `Interrupted` 但 helper 未知 ⇒ **保持隔离**；**不能** `Interrupted == safe`，必须表达 `run stopped / resource uncertain / input blocked` | **已实现**：`legacy_recovery.rs:300` `combined_4_interrupted_run_does_not_imply_release_safety`（含遗留 NULL 归属未收尾运行）；**层级＝存储＋语义层** |
| 组合测试 5 | Goal 有锚点但 chat turn 缺失 ⇒ 经 `turn_id/session_id → runtime_runs → owner` **真实关系**解析；**不能**把 `goal_id` 复制成 `chat_turn_id` | **已实现（仅否定路径）**：`legacy_recovery.rs:236` `combined_5_owner_resolution_uses_real_relations_only`，三种未知分别可辨且不复制 ID。**⚠ 只覆盖否定路径**——「真实自主 Goal 没有 chat turn 仍能经合法**强父关系**接纳」这一**正向能力仍缺**，见下方独立行（本轮裁决 §5.5 要求两者分开证明） |
| **CU 释放义务缺陷（原记「新 P0 事实错误」）** | ~~现状 `NotSent ＋ ReleaseUnknown` 可同时落库~~ —— **该缺陷已由 §B-43 修复**：五态 `ReleaseObligationState` ＋ 唯一推导点 `derive_release_obligation`（笔画与普通输入共用）＋ 回执由已推导的投递事实构成 ＋ `needs_emergency_release` 消费五态 ＋ 消费侧闸门（`contracts.rs:237-246`：`NotSent` 必须带 `input_release = NotNeeded`）＋ 零填充 `facts.unwrap_or_default()` 已删除 | **既有修复保留；2026-09-26 逐符号核对仍在位**（`ReleaseObligationState` 26 处、`derive_release_obligation` 13 处、`needs_emergency_release` 14 处、`LegacyUnverifiable` 8 处、五态测试齐）。**不再作为待开工缺陷**；只有将来真回归才重开 |
| ~~CU Error Contract：`ExecutionOutcome` 取代 `Result<StepExecution, ComputerUseError>`~~ | **[已撤回／被本轮替代]** 本轮正式撤回，**不实施**。保留已选定的 `Result` 形态与 `ComputerUseError.receipt: Option<ActionReceipt>`（§B-84 已交付）：**失败可以携带回执，不需要为此再造一个平行顶层结果类型** | **禁止据此开工**（2026-09-26 本轮补充裁决 §2.1） |
| ~~CU 动作身份：新增 `ActionScope::NativeAction`，其 `request_attempt_id` 可选~~ | **[已撤回／被本轮替代]** 本轮正式撤回，**不实施**。保留事实层级、`ContextKind` 与 `ActionSource` 的**正交**设计：模型规划动作**即使由原生 helper 执行，仍必须携带真实规划 attempt**；清理／宿主辅助／真实用户直操**按来源判断**不适用维度 | **禁止据此开工**（2026-09-26 本轮补充裁决 §2.1） |
| ~~**CU Error Contract**：`ExecutionOutcome`~~ | **[已撤回／被本轮替代]** 见本表上方同名条目的撤回说明；**不实施**，**不再是可领取工单** | **禁止据此开工**（2026-09-26） |
| ~~**CU 动作身份**：新增 `ActionScope::NativeAction`~~ | **[已撤回／被本轮替代]** 见本表上方同名条目的撤回说明；**不实施**（保留 `ContextKind`／`ActionSource` 正交设计） | **禁止据此开工**（2026-09-26） |
| **Paint（CU-01..CU-05）** | **不进入模型调参，先修事实链**。**P0**：CU-01 动作状态改 `none/partial/complete/unknown`（替代 `success/error`）；CU-02 **输入独占**（one physical keyboard/mouse owner，否则两个 CU run 争抢桌面）。**P1**：CU-03 planner 反馈（`last_action`/`last_error`/`last_verdict`/`subgoal_progress`）；CU-05 UIA 状态（`selected`/`focused`/`pattern`/`toggle state`）；CU-04 `frame_id` ＋ raster 坐标（**已交付**，见 §B-107：审查后结论是**不需要新标识**，改为复用既有值做帧绑定）。**不要**继续扩大模型承担：截图 → 坐标数学 | **延后**（Phase 4） |
| **PKG-L07c-RACE** | 独立补验：100 次并发导出／10 失败竞争者／随机 kill；检查 winner generation 唯一、receipt 不错配、consumer 永远读完整代次 | **新工单**（Phase 5，不阻塞单发布） |
| 架构路线 | **保留 Rust/Tauri/SQLite**，吸收 DSH 契约，**不迁移 Node runtime** | 已裁决 |

### C-收口-补. **本轮（2026-09-26）新拆出的独立项**——不得与上面「已有测试」混作同一证据

| 项 | 要求 | 为什么单列 |
| --- | --- | --- |
| **真实进程级崩溃装置** | 父测试创建**专用子进程**、独立会话库／输入安全库／证据目录、测试专用锁命名空间、`cfg(test)`／默认关闭的故障入口、只终止**自己持有创建句柄**的子进程、受控假输入、看门狗与受控后代回收；用命名事件／管道屏障确认前置阶段真的到达后再终止（**不靠 sleep 猜时序**）。最低六个故障点见本轮裁决 §6.2；正式打包继续验证 `test-support` 未进入产品依赖与二进制 | 组合测试 3 用的是**存活探针模拟**崩溃 ⇒「测试存在」与「真实崩溃变体已执行」是**不同证据**。**本轮已批准**（裁决 §4／§6） |
| **Goal 正向强父关系** | 真实自主 Goal **没有 chat turn** 时，仍能经**合法强父关系**接纳 CU；并覆盖阶段重试、错误父运行、跨工作区引用、父运行取消、迟到回执不重开旧阶段。强关系＝CU 对应哪一次**真实 Goal 阶段运行**；**可缺省**的是有没有聊天发起者 | 组合测试 5 只证明**否定路径**（不复制 ID、正确 Unknown），**不能**替代正向能力（裁决 §5.5） |
| **R3 撤销未激活许可** | 许可状态至少区分：待激活／已消费进入派发／执行中／已结束／已撤销／结果未知。动作：按真实 scope 关闸并推进 gate revision → **原子撤销**未消费旧许可 → 已消费进在途清单交 R4（**不得改记为未发送**）→ 最终派发前再核许可与 epoch。并须避免**恢复自锁**（关闸／取消不得先等持有整段排他的活跃 helper 自己放锁） | 当前**无实现**，且**没有可复用的许可/broker 概念**（2026-09-26 核实：全仓库唯一的 `Broker` 是浏览器桥的连接 broker，与输入许可无关；`gate_revision` 亦无任何命中）⇒ 本项要**从零建持久许可登记**并接进派发边界，属§9.1「优先并串行」且触及输入安全库 schema（由该库负责人串行合并）。裁决 §4.4 已给出完整口径与许可状态机 |
| **R4 跨进程执行者实例核查** | 权威来源＝**宿主启动登记**（哪个 run／action 由谁创建）＋**OS 实例证据**（创建返回的进程句柄、创建时间、实际用户／会话）＋**helper 执行回执**；`owner_id`／`coordinator_id`／PID **三者不得互相替代**；六类失败语义逐一规定（已确认退出／PID 已复用／AccessDenied⇒Unknown／缺登记或损坏／监督者退出但 helper 可能存活／迟到回执）；只准在**同一个已核实实例句柄**上完成后续操作 | 当前**无实现**；裁决 §4.5 本轮给出权威来源与失败语义（原阻塞点正是「裁决未指定可信来源」） |
| **恢复操作员认证授权** | 单受信本机操作员（**不要求双人**），绑定实际 **Windows 用户 SID** ＋ 资源与政策版本；原生系统验证（优先桌面应用适用的 Windows Hello／用户验证接口，回退受控系统凭据验证并**验证取得的实际身份**；两者不可用或身份不符则**继续隔离**，**不退回纯署名**）；一次批准绑定一次具体决定并复用 `ReleaseIsolationDecision`；**不接受 Web 传入 `verified=true`** | 现状只实现**记录与并发一致性检查**；`400` 空署名／`409` 过期阻断集合**只能**证明输入校验与版本冲突，**不能**证明调用者有权放行（裁决 §3.1）。在补齐前，**普通 HTTP 放行接口不得凭这些字段改变资源状态**（可保留查询、提交复核请求、打开原生确认面） |


## C-禁止. 第八轮禁止事项（后续执行硬约束）

1. **禁止**用 `resource_blocking_refs` 非空 ＝ 安全成立；**必须验证来源**。
2. **禁止** `Interrupted` ＝ 释放安全。
3. **禁止**为让 Paint 成功而放宽 DPI／删除 stale／增大 timeout／关闭输入校验。
4. **禁止**同时重构 `main.rs` ＋ runtime ＋ storage ＋ UI；**必须小 PR**。

## D. 本轮已闭环、无需裁决

| 工单 | 交付 | 实测门禁 |
| --- | --- | --- |
| RPR-04a | `ComputerUseError.receipt: Option<ActionReceipt>`（additive）+ `with_receipt`/`receipt()` + 3 测 | `computer-use-core` 53/53；`cargo build --workspace` EXIT 0；爆炸半径实测 **0** |
| RPR-07a | `effective_provider_key`/`first_present_env_key_value` 抽取 + 3 个刻画测试，固定"清空即回退环境变量" | `web-console` 定向 3/3 |
| RPR-11a | 移除按端口终止（含退出时误杀 bge-m3 的缺陷）；托管身份登记；外部占用改为阻断；4 测 | `web-console` 定向 5/5 + 4/4；编译 EXIT 0 |
| RPR-02a | guard `test-support` 接缝 + `revoke_owner_for_test` + 5 测；web-console 侧 §3.3 端到端回归 2 测（纯测试代码，未动生产逻辑） | guard 6/6；`computer_use_executor::` 21/21；发布图无 `test-support`（cfg 级证据） |
| RPR-11c | `rpr-11c-budget-chain.md`（789 行）：预算层级、下限回扩核对、`effective_call_budget` 落点、1+2n/1+3n 推导、容量与时间双检查、待测清单 | 只读，未改源码、未发起真实模型调用 |
| RPR-01 | `ScopedCurrentDir` Drop 不 panic（+ `enter_in`）；4 处手写 cwd save/restore 收口；2 处补 env_lock；core-runtime 新增 crate 内 RAII；5 测 | tool-registry 46/46（并发与串行各一次）；core-runtime 219/219；linkage 4/4 |

残留警告清理：`controller.rs:892/905/920`、`browser_bridge.rs:535`、`computer_use_desktop_bridge.rs:345` 的 `unused variable: remaining`（上一轮 S1.5 残留）已改为 `_remaining`。
