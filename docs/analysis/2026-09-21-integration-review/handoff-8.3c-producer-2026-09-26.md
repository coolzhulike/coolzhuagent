# 交接：8.3c 执行者身份生产者 → 许可门接线（2026-09-26）

> **给新会话的自包含交接**。读完这一份即可开工，不需要回看长对话，也不需要重新推导任何设计。
> 台账 `rpr-execution-blockers.md` 的 §B-117…§B-130 是每一条交付的原始记录；本文件只讲**"现在做到哪、下一步怎么走、什么不能做"**。

---

## 1. 目标（一句话）

给 `InputPermit` 的 `executor_instance_id` 接上**真实生产者**，然后才允许把许可门接进
`computer_use_executor.rs:228` 的真实输入路径。

**为什么必须先有生产者**：许可模型里 `executor_instance_id` 承担**安全绑定语义**
（"允许某个具体执行者产生输入"）。今天唯一能填的字符串是**执行尝试身份**，
填进去就是**用执行尝试身份冒充执行者实例身份**——与已裁决的两处同源错误同类：

| 编号 | 冒充关系 | 裁决 |
| --- | --- | --- |
| B-121 | `action_id`（内容身份）冒充 `execution_attempt_id` | 已裁决：引入 `ExecutionAttemptId`，复合键 |
| B-124 | 不存在的 `policy_revision` 冒充政策绑定 | 已裁决：删除该字段 |
| **本次** | **`attempt` 冒充 `executor_instance_id`** | **待做：先补生产者** |

架构原则（三次同源）：**安全绑定字段必须绑定真实生产事实，而不是绑定一个看起来相似的已有字段。**

---

## 2. 当前状态

**分支**：`rd4-input-safety-and-pkg-integrity`（**未推送**）。工作区干净。

**门禁基线**（必须维持或提升，不允许低于此）：

| 目标 | 命令 | 当前 |
| --- | --- | --- |
| web-console | `cargo test -p coolzhu-web-console --offline` | **1215 / 0**（1 ignored＝真实调用评测） |
| core-runtime | `cargo test -p coolzhu-core-runtime --offline` | **337 / 0** |
| 根级守卫 | `cargo test --test module_linkage_smoke --offline` | 8 / 0 |
| 崩溃装置 | `cargo test --test crash_harness_smoke --offline` | 7 / 0 |
| 工具注册 | `cargo check -p coolzhu-tool-registry --offline` | ✅ |
| 工作区 | `cargo build --workspace --offline` | ✅ |

**本会话已交付（由旧到新，短号）**：

```
3f5f05e  8.2b/8.3b 联合持久化：输入安全库 v2→v3 一次相邻迁移 + DB-1..DB-8 全通过
02d6750  宿主 bin 测试接缝 + K1 核心场景（真实跨进程）
e6658ae  8.2c/8.3c 接线方案落盘（锚点已定）
ac0fac3  8.2c 使能步骤：适配器经生产窄口可达（permit_store() / executor_store()）
e0bf823  登记 B-121 契约冲突（action 内容身份 ≠ 执行尝试身份）
4e9c5bd  ExecutionAttemptId 冻结 + 许可复合键 (action_id, execution_attempt_id)
eb07768  §四 四条重复判定按"逻辑尝试键" + B121-T1..T5 全通过
557bffb  登记 B-124 缺口（policy_revision 无真实来源）
04f2c82  删除 policy_revision；gate/epoch 变化改为可分辨拒绝 + B124-T1/T2
6a183a3  8.2c 许可门单元（§七 顺序，可测）
bc78adc  登记 8.3c 生产者设计（缝在哪已核实）
0a669ef  B-126 修复：consume 补全校验 + 可分辨失败 + 带全谓词的条件 UPDATE
bbe7750  进程实例证据 + 实例比对判定（PID 复用可识别）
d025b53  B126-T2/T4/T5 落地
```

**已闭环、不要再动的东西**：

- 六态状态机与转换表、执行者身份契约、`ExecutionAttemptId`、§四 四条重复判定规则；
- 输入安全库 v3 schema（许可表 + 执行者表）、迁移原子化、DB-1…DB-8 验收；
- 许可门单元 `input_permit_gate.rs` 的 §七 顺序（签发／复核／消费）；
- consume 的全谓词条件更新与可分辨失败。

---

## 3. 下一步：8.3c 最小生产者（三步，顺序固定）

### 第 1 步：把**已捕获的** helper 身份送到宿主（**不新增第二条捕获路径**）

- **缝的位置**：`computer-use-core` 的 `NativeInputOutcome`（`input.rs` 约 :1707）。
  它当前只有 `obligation` / `facts` / `fact_anomaly` / `reply_received_at_ms` / 结束确认时刻，
  **没有** pid 与创建时间。
- **取值来源**：复用输入层**已有**的捕获——`input.rs:6392`
  `windows_process_guard::capture_process_identity(self.pid)`，以及同处
  `GrandchildHolder { pid, identity }`（按身份核对后才终止）。
  **不得**新增独立扫描/查询（两处必然漂移）。
- **类型**（裁决 §四 批准的最小身份，已在 core-runtime 定型）：
  `runtime::ProcessInstanceEvidence { pid: u32, creation_time_filetime: u64 }`。
  契约侧**已存在**，含 `is_identifying()`。把它暴露到 `NativeInputOutcome` 上即可
  （建议字段名 `helper_process: Option<ProcessInstanceEvidence>`）。
- **注意事项**：`NativeInputOutcome` 是受控输入族广泛使用的类型 ⇒ **所有构造点都要同步**。
  建议做法：先加字段并**逐个构造点**补齐（编译错误会逐一点名），或先加带 `Default` 的
  `Option` 字段以降低一次性改动量。**改完必须两个 crate 全绿**（见第 2 节基线）。

### 第 1 步的侦察结论（2026-09-26 会话收尾时核实，**不是**推断）

动手前把第 1 步的未知项查完了，结论是**好消息**——比预想的小：

| 事实 | 值 |
| --- | --- |
| 生产构造点 | `input.rs` 的 `run_native_helper`（**函数起点 :3317**）内，`NativeInputOutcome` 在 **:3604** 构造 |
| helper 句柄是否在同一函数作用域 | **是**：约 **:3531** 有 `let mut child = match command.spawn()`，`child` 与 `child.id()` 都在作用域内 |
| 身份捕获能力 | 已有：`windows_process_guard::capture_process_identity(pid)`（同文件 :6392 在用） |
| 身份类型 | `windows_process_guard::ProcessIdentity { pid, creation_time, image_path }`，访问器 `pid()` / `creation_time_filetime()` |
| 契约类型是否可直接用 | **可以**：`computer-use-core` 的 `Cargo.toml` **已依赖 `runtime`** ⇒ 直接填 `runtime::ProcessInstanceEvidence { pid, creation_time_filetime }`，**不需要**新增依赖或本地同义类型 |
| 其它构造点 | 仅 2 处，都在测试里（约 **:6253**、**:6307**）⇒ 各补 `helper_process: None` 即可 |

**⇒ 第 1 步是"函数内局部改动"**：在 spawn 成功之后捕获一次身份 → 在 :3604 填进 outcome →
两个测试构造点补 `None`。不需要穿层、不需要新增依赖。

**唯一仍需小心的一点（下次上手先读这一段）**：`:3531` 的 `match command.spawn()` **不是一个几行的
match**——它的 Ok 分支里包含**整个等待/轮询/收尾循环**（:3548 附近起的 `cancelled()/timeout` 轮询、
协作退出、kill 收尾都在其中）。因此捕获点的正确位置是**spawn 之后、轮询循环之前**的 Ok 分支早期，
而"match 的结束位置"不能用简单的 `};` 锚定。**必须先读清 Ok 分支的结构再插入**，
不要按行号盲插（本会话已有"锚点不唯一/不命中"的数次教训）。

**建议的验证顺序**：`cargo test -p coolzhu-computer-use-core --offline` ＋
`cargo test -p coolzhu-web-console --offline` **都要绿**（加字段是全链改动；
本会话三次教训：`cargo build` 绿 ≠ 测试能编译）。

### 第 2 步：执行器登记，产出**真实** `executor_instance_id`

- **顺序不能反**（裁决 §五）：
  ```
  register_launch_intent  →  record_instance_evidence  →  executor_instance_id
  ```
  **禁止** `permit created → executor later attached`。
- **API 已就绪**：`crate::input_permit_store::ExecutorStore`（经
  `InputSafetyStore::executor_store()` 取用），两个方法加上 `load_executor` 都已实现并通过测试。
- **判定已就绪**：用 `runtime::classify_executor_instance(&registration, Some(evidence))`
  得到**契约已冻结的观测值**，再交给 `disposition_for(...)` 取处置。
  - 同 PID ＋ 不同创建时间 ⇒ `PidReused` ⇒ `DoNotTouchCurrentProcess`；
  - 登记缺创建时间 ⇒ `AccessDeniedOrUnreadable`（**不得**按 PID 认为一致）；
  - 观测不到 ⇒ `DirectHelperExited`（只证明该实例退出）。
- **登记失败就不发许可**（fail-closed）。

### 第 3 步：许可门绑定真实 id，然后才接线

- 把 `input_permit_gate.rs` 里 `IssuedExecutionPermit.executor_instance_id` 由
  **恒 `None`** 改为来自第 2 步的真实 id。
- 接线位置（§七，裁决已确认**不是二选一**）：
  - **运行级闸门**（`computer_use_executor.rs` 约 :1964）：管"这次运行能不能输入"——**已在，不要动**；
  - **动作级许可门**（`computer_use_executor.rs:228`，紧邻既有 `admit_action_origin`）：管"这一次执行尝试有没有资格"——**本次要接的**。
- **DB 操作必须留在 broker 边界之外**：`lease.dispatch_if_current(...)` 内部
  （约 :281 的注释）**明确不允许**出现 DB 查询/长等待。因此顺序是：
  `record_step` → **许可复核 + 消费** → 进入 broker 边界 → 重检守卫 → 原生输入。
- 消费前复核在许可门内已完成（gate/epoch 比对 + 执行者一致性 + 全谓词条件更新）。

---

## 4. 必须补的验收（裁决 §九）

| 用例 | 状态 | 说明 |
| --- | --- | --- |
| B126-T1 真实执行者身份消费通过 | **待做** | 需要第 1–2 步；这是"T1 证明生产者真的产出了身份" |
| B126-T2 伪造 id ⇒ 拒绝 | ✅ 已做（§B-130） | `PermitExecutorMismatch` |
| B126-T3 PID 复用 ⇒ 拒绝 | ✅ 判定机制已做（§B-129） | **待补**：从真实 helper 取到该证据的端到端 |
| B126-T4 缺执行者 ⇒ 不能消费 | ✅ 已做（§B-130） | `permit_executor=None` 如实报告 |
| B126-T5 消费前身份变化 ⇒ 拒绝 | ✅ 已做（§B-130） | 换身份被拒、原身份仍可消费 |

**另外必须带一条"正常路径"用例**：接线后**输入照旧发生**（证明产品没有被中途关掉）。
这是本会话反复强调的——只接限制不做正向证明，等于不知道产品是否还可用。

**变异验证要求**（本会话既有做法，务必延续）：每条新守卫都要证明"改坏就会红"。
注意本会话踩过 **3 次"无操作变异"**（改了文本但行为没变，显示"未捕获"）：
**变异必须验证"确实改变了行为"**。

---

## 5. 禁止事项（裁定，逐条来自裁决）

1. **不得**用 `attempt_identity` 代替 `executor_instance_id`；
2. **不得**用 PID 字符串代替实例身份；
3. **不得**先接输入再补身份；
4. **不得**先产生 permit 再补执行者；
5. **不得**新增独立身份采集器／第二套 PID 扫描（与既有捕获会漂移）；
6. **不得**把 `policy_revision` 加回来（含用 schema 版本或改名后的 gate 冒充）；
7. **不得**在 SQL 层用 `ON CONFLICT(action_id)` 之类的错误抽象；
8. **不得**把 `attempt` 一词同时表示内容指纹／执行尝试／请求重试；
9. **不得**在 `lease.dispatch_if_current` 边界内做 DB 操作；
10. **不得**用占位值／默认值让契约检查通过（宁可缺字段）。

**8.3c 第一阶段范围**：只包含"helper 创建身份捕获 → outcome 暴露 → executor 登记 → permit 绑定 →
consume 校验"。**不包含**：kill 策略改变、Job 生命周期重构、外部进程管理（属 R4 后续）。

**环境边界**：不迁移真实输入安全库、不解除隔离、不启动真实输入、不提权、不安装、不推送。
8.3c 可在**独立测试数据库 + 测试 scope + 测试 helper**上完成验证，
**不需要**把当前真实桌面当实验对象。

---

## 6. 关键文件与函数速查

| 关注点 | 位置 |
| --- | --- |
| 契约（六态／许可／执行者／实例比对） | `modules/core-runtime/packages/core-runtime/src/input_safety.rs` |
| 契约重导出 | 同 crate `lib.rs` 的 `pub use input_safety::{…}` |
| 许可/执行者 SQLite 适配 | `modules/gui-web/packages/web-console/src/input_permit_store.rs` |
| 许可门（§七 顺序） | `modules/gui-web/packages/web-console/src/input_permit_gate.rs` |
| 输入安全库本体（迁移／资源状态／epoch） | `modules/gui-web/packages/web-console/src/input_safety_store.rs` |
| 执行器（输入前准入 :228／派发点 :268） | `modules/gui-web/packages/web-console/src/computer_use_executor.rs` |
| helper 身份捕获（**取值来源**） | `modules/computer-use/packages/computer-use-core/src/input.rs:6392` |
| `NativeInputOutcome`（**要加字段的类型**） | 同文件约 :1707 |
| K1 跨进程接缝 | `modules/gui-web/packages/web-console/src/input_safety_harness.rs` |
| 崩溃装置 | `tests/crash_harness_smoke.rs` |
| 台账（每条交付的原始记录） | `docs/analysis/2026-09-21-integration-review/rpr-execution-blockers.md` §B-117…§B-130 |
| 状态与遗留 | `…/implementation-status-and-change-inventory.md` §7／§8 |

---

## 7. 若中途失败（保持树绿的纪律）

本会话有三次"加必填字段导致**测试**构建失败（`cargo build` 绿、测试红）"的教训：

1. `InputPermit` 加字段后，**两个测试辅助构造点**没同步；
2. 同一 SELECT 删列后**投影索引**没整体下移（串列）；
3. 另一处独立 SELECT 未同步（`no such column`）。

⇒ **规则**：
- 加/删字段是"**全链改动**"：结构体、构造点、SELECT 列表、投影索引、INSERT 列与**占位符个数**、
  测试辅助，一处都不能漏；
- **`cargo build` 绿 ≠ 测试能编译**：必须跑 `cargo test`（含 `--tests`）；
- 若一轮内无法完成，**回到最近一次全绿的提交**，不要留下半改状态；
- 提交信息里如实标注"未完成／不可发布"，并写明下次的入口。

---

## 8. 一句话给下一个会话

**先给 helper 身份开一条缝（复用既有捕获）、再登记执行者、最后才把许可门接到 :228；
全程保持"字段必须有真实生产者"这条原则——这已经是本会话第三次靠它避免埋坑。**
