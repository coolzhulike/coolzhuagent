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



## C-收口. 第八轮裁决新增项（**已裁决，待实现／待接线**，按阶段推进）

> 权威正文见 `round8-rulings-and-execution-order.md`。以下不是"待裁决"，而是**已裁决的执行项**，按状态登记。

| 项 | 生效结论 | 状态 |
| --- | --- | --- |
| RD4-02A 最终形态 | **采用 A ＋ C 的语义约束**：独立输入安全库，但它**不是物理输入所有权本身**，而是"输入安全事实"的**权威记录**；否决 B（塞 session SQLite，破坏"不宣称跨库原子性"）与纯 C。结构：`InputSafetyStore → input-safety.sqlite3`，含 **incidents / resource_blocks / recovery_operations / ownership_epochs / safety_events**；位置 `ResolvedLaunchPaths.input_safety_state_root`，**禁止** workspace `.coolzhu`／session db／temporary path | **方向已定，继续实施**（Phase 1） |
| RD4-02A 剩余 | **launcher 注入** `COOLZHU_INPUT_SAFETY_STATE_ROOT`；生产路径**必须注入**；缺失即 `root_not_injected` **直接 fail closed**；**不要 fallback**（否则测试环境污染生产路径） | **已批准，待实现** |
| RD4-02B | **跨进程资源协调器 ＋ 可信 `RecoveryControlGuard`**；**不是**进程 mutex／session 级锁／caller 参数声明。`InputSafetyCoordinator`：取得资源锁（scope ＝ `windows-session-{id}` ＋ `physical-input-resource`，**不是** workspace/session/turn）→ 写 `RecoveryOperationStarted` → 创建 `RecoveryControlGuard { resource_scope, recovery_id, epoch, coordinator_id, allowed_actions }`；恢复入口**必须**走 `Coordinator → InputSafetyStore → RecoveryControlGuard`，**禁止** `paused=true`／`authority="xxx"` 式调用 | **开工条件满足**（Phase 2） |
| 组合测试 1 | `paused=true` 但闸门未关闭 ⇒ `RecoveryUnauthorized`，**不写终态** | 待实现（Phase 1 退出条件） |
| 组合测试 2 | 双进程竞争：A 取得 epoch 21，B **epoch 冲突被拒**，且 B **不能**写 incident／改 run 状态／开新输入 | 待实现（Phase 1 退出条件） |
| 组合测试 3 | 恢复者死亡 ⇒ **启动恢复**（`RecoveryStarted → crash → startup reconcile → acquire new coordinator epoch → continue/reject`）；**禁止 delete recovery row** | 待实现（Phase 1 退出条件） |
| 组合测试 4 | 已 `Interrupted` 但 helper 未知 ⇒ **保持隔离**；**不能** `Interrupted == safe`，必须表达 `run stopped / resource uncertain / input blocked` | 待实现（Phase 2） |
| 组合测试 5 | Goal 有锚点但 chat turn 缺失 ⇒ 经 `turn_id/session_id → runtime_runs → owner` **真实关系**解析；**不能**把 `goal_id` 复制成 `chat_turn_id` | 待实现（Phase 2） |
| **CU 释放义务缺陷（新 P0 事实错误）** | 现状 `NotSent ＋ ReleaseUnknown` **可同时落库**，根因是**释放义务判断没有消费 helper fact**。批准 **(a)** 义务必须基于事实（`button_down=false` ＋ `injected_points=0` ＋ `path_completed=false` ⇒ `release_needed=false`）；**(b)** 回执**禁止** `NotSent ＋ ReleaseUnknown`，必须 `input_release=**NotNeeded**`；**禁止只改测试**（属事实模型错误，不是测试过严） | **P0 修复待实现**（Phase 3 PR-CU-FACT） |
| **CU Error Contract** | 采用 **`ExecutionOutcome { execution: Option<StepExecution>, receipt: ActionReceipt, error: Option<ComputerUseError> }`** 取代 `Result<StepExecution, ComputerUseError>`；理由：**错误不是异常**，CU 执行失败**仍可能产生事实** | **新工单**（Phase 3） |
| **CU 动作身份** | **采用 A：新增 `ActionScope`**，不强迫所有动作属于 `StepAction`；`StepAction.request_attempt_id` **required**，`NativeAction.request_attempt_id` **optional**（原生动作没有模型请求：release／safety cleanup／user direct） | 已裁决，待实现 |
| **Paint（CU-01..CU-05）** | **不进入模型调参，先修事实链**。**P0**：CU-01 动作状态改 `none/partial/complete/unknown`（替代 `success/error`）；CU-02 **输入独占**（one physical keyboard/mouse owner，否则两个 CU run 争抢桌面）。**P1**：CU-03 planner 反馈（`last_action`/`last_error`/`last_verdict`/`subgoal_progress`）；CU-05 UIA 状态（`selected`/`focused`/`pattern`/`toggle state`）；CU-04 `frame_id` ＋ raster 坐标（**批准**）。**不要**继续扩大模型承担：截图 → 坐标数学 | **延后**（Phase 4） |
| **PKG-L07c-RACE** | 独立补验：100 次并发导出／10 失败竞争者／随机 kill；检查 winner generation 唯一、receipt 不错配、consumer 永远读完整代次 | **新工单**（Phase 5，不阻塞单发布） |
| 架构路线 | **保留 Rust/Tauri/SQLite**，吸收 DSH 契约，**不迁移 Node runtime** | 已裁决 |

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
