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

## 3-bis. **批次边界（2026-09-26 已裁决：必须原子落地）**

**这不是"实现顺序偏好"，而是生命周期契约决定的硬依赖**：helper 进入 `AwaitingPermit` 后，
**必须存在对应的宿主授权闭环**，否则该状态只是一个必然失败路径。

⇒ 把原 `8.3c-2`（helper 等待态）＋ `8.3c-3`（执行者登记）＋ `8.2c`（许可签发/消费）
**合并为一个交付单元 `8.3c-A`：两相 helper 端到端切片**。

**仍然分层**：契约保持分层、提交可以拆分；但**不得存在一个中间可运行版本**，
让 helper 等待一个永远不存在的 permit。

### 两个**已被裁决拒绝**的半成品（不要重犯）

| 方案 | 为什么拒 |
| --- | --- |
| ❌ 先加 `wait-permit`，"以后再写 permit" | 生产行为变成"全部 CU 输入 → READY → 等待不存在的文件 → timeout"。**这不是 fail-closed，是产品功能被自身半升级状态永久拒绝** |
| ❌ helper READY 后由宿主**直接**写 permit | 绕过了 `ExecutorStore` ＋ `PermitGate`，等于把 **permit 文件当成授权来源**，违反已冻结模型 |

### 实施顺序（§三）

1. **Step A：先保持旧路径有效**——改动期间 `NativeInputExecutor → 旧 helper flow` 继续工作；
2. **Step B：一次性切换新协议**——
   `spawn → READY → capture ProcessIdentity → ExecutorStore.register → PermitGate.issue
   → PermitGate.consume → write EXECUTE → helper input`；
3. **Step C：失败回退策略**——新协议失败 ⇒ `AbortBeforeInput` ＋ cleanup；
   **禁止**"新协议失败 ⇒ 自动切回旧 helper 继续输入"（那等于安全路径失败时反而走未授权路径）。

### 四处改动（§七，均需真实进程验证）

| # | 位置 | 允许 | 禁止 |
| --- | --- | --- | --- |
| 1 | `computer-use-core` **helper 脚本**（内嵌 PowerShell／C#） | READY 写入；permit 等待；EXECUTE gate | 自己判断安全策略；自己生成 executor identity；自己创建 permit |
| 2 | `computer-use-core` **宿主侧** | `ready_file`／`permit_file` 参数；handshake 状态 | — |
| 3 | **web-console executor** | 完整链 `identity → ExecutorStore → PermitGate → permit file` | — |
| 4 | **测试** | B127-T6…T12，全部**真实 helper 进程** | 用 mock 冒充进程级验收 |

### 协议补充要求（§四／§五／§六／§九）

- `permit_file` **只能由宿主授权流程生成**，helper **只读**；且**必须绑定当前 helper 的 nonce**
  （否则**旧 permit 文件残留**会让 helper B 读到 helper A 的授权）。
- `AwaitingPermit` 循环：**复用已有 `cancel_file` 模型**——先查 cancel，再查 permit（校验 nonce），
  写 heartbeat，**有界**间隔；**禁止**裸 `while(true){sleep}`；
  期间允许 heartbeat／status／handshake／cancel polling，**禁止** mouse／keyboard／clipboard／window 变更。
- **cancel 优先**（本批最重要的测试之一）：`READY → cancel → permit` 必须 `AbortBeforeInput`，
  **不得** `PermitConsumed`——cancel 意味着"该执行尝试已不应继续获得物理输入资格"。
- **DB 操作仍留在 broker 边界之外**：`ExecutorStore` ＋ `PermitGate` 先完成事务，**再**写 `permit_file`。
- 写 `permit_file` 后、helper 收到前宿主崩溃 ⇒ 状态 `PermitConsumed` ＋ `ExecuteUnknown`；
  **不得**重新写 permit、**不得**重新执行。

### 新增两条协议级测试（§八）

- **B127-T11 旧 permit 残留**：helper A 退出后留下 `permit_file`，helper B 启动 ⇒ **B 拒绝**
  （nonce 不匹配）。
- **B127-T12 READY 伪造**：宿主提前写 `ready_file` 而 helper 未真正进入 `AwaitingPermit` ⇒ **拒绝**。
  READY **不是单纯文件存在**，至少校验 nonce、helper identity、当前握手阶段。

### 8.3c-A 完成标准（§十）——**全部满足才允许恢复 `:228`**

- **Helper**：READY 在任何输入 API 前；`AwaitingPermit` 无物理输入；只有 EXECUTE 触发输入；
  cancel 优先；nonce 防串用。
- **Identity**：helper 存活时捕获；`ExecutorStore` 登记；permit 绑定**真实** `executor_instance_id`。
- **Permit**：issue 前有 attempt；consume 前复核 gate／epoch／executor；stale 拒绝；不自动重试。
- **Failure**：等待期间崩溃／消费后执行前崩溃／EXECUTE 后崩溃，三者都进入正确未知状态。

## 3-ter. **下一次工作窗口的唯一执行口径（2026-09-26 已裁决：保持冻结、不拆批）**

**当前状态（正式登记）**：

| 项 | 状态 |
| --- | --- |
| 8.3c-1 helper identity exposure | ✅ 已完成（§B-132） |
| 8.3c-2 helper 两相协议 | **契约冻结，实现未开始**（§B-134／§B-136） |
| 8.3c-3 executor registration | 等原子批次 |
| 8.2c permit consume integration | 等原子批次 |
| `computer_use_executor.rs:228` | **保持禁止接线** |
| B127-T6…T12（+T13） | 等真实 helper 实现 |
| 当前输入路径 | **未变化**（旧路径有效） |
| 真实隔离状态 | **未变化** |

**本条阻塞的性质**：不是技术未知、不是架构未定、不是要重新设计，而是**原子交付窗口不足**。
四部分强依赖：`helper protocol → identity registration → permit consume → execute gate → real helper tests`。
三个半改各自会造成的后果（登记备查，避免下次为省事而拆批）：

- **半改 A（只让 helper 等 permit）** ⇒ 宿主不生产 permit ⇒ **新版本自身阻断所有输入能力**（不是 fail-closed）；
- **半改 B（宿主直接写 permit）** ⇒ 绕开 `ExecutorStore`／`PermitGate`／gate revision／epoch 检查 ⇒ **文件存在＝授权**；
- **半改 C（只接 executor，不接 helper）** ⇒ 身份存在但**未控制执行时序**。

### **先定「谁编排」（裁决留下的实施空位，2026-09-26 核实后补齐）**

两个硬事实决定了批次形状：

- `computer-use-core` **不依赖** web-console ⇒ 它**无法**调用 `PermitGate`／`ExecutorStore`（都在 web-console）；
- `controlled_*` 的消费方是 **4 个文件／3 个 crate**（core 自身、desktop-console、web-console 的 bridge 与 main），
  ⇒ 改签名的跨度是跨 crate 的。

⇒ `run_native_helper` **必须拆成两相 API**，且**由 web-console 编排**：

```
prepare  : 创建 helper → 等 READY → 返回 helper 身份（helper 存活并等待许可）
   ↓        web-console 在中间完成：ExecutorStore 登记 → PermitGate.issue → consume → 写 permit 文件
execute  : 让 helper 继续（它轮询到本人 nonce 的 permit → EXECUTE → 注入）
```

**分层不破**：安全判定全部留在 web-console；`computer-use-core` 只提供「能创建、能等、能执行」
两个入口，**不含任何安全判定**（它本来也不该有）。

### **命名纠正（2026-09-26 裁决 §一）：`8.3c-A-contract` ≠ 已完成**

| 名称 | 含义 | 状态 |
| --- | --- | --- |
| **`8.3c-A-contract`** | 设计冻结 ＋ 协议门禁 | ✅ **完成** |
| **`8.3c-A-runtime`** | helper READY/WAIT_PERMIT/EXECUTE 的实际行为、permit 文件真实流转、`ExecutorStore` 实际绑定、EXECUTE gate | **待完整切换窗口** |

协议门禁解决的是「**新旧 helper 是否允许进入同一个生命周期**」，**不是**「helper 是否已完成两相执行」。
⇒ **不得**把"门禁通过"写成"8.3c-A 已完成"。

### **阶段状态（2026-09-26 裁决 §十二，权威口径）**

| 阶段 | 状态 |
| --- | --- |
| `8.3c-A-contract`（设计冻结 ＋ 协议门禁） | ✅ |
| `8.3c-A-host-ready`（宿主侧 READY 等待与校验） | ✅ |
| `8.3c-A-helper-runtime`（脚本侧两相生命周期 ＋ T6 真实 helper 测试） | ⏳ **下一批** |
| `8.3c-A-executor-bind`（`ExecutorStore` ＋ `PermitGate`） | ⏳ |
| `8.3c-A-input-switch`（`controlled_*` 一次性切换） | ⛔ **禁止** |

### **静态脚本解析不再作为安全门禁（裁决 §三 撤销）**

`[scriptblock]::Create()` 只验证"PowerShell 语法能否解析"，**不验证**：READY 是否在输入前发生、
等待期间是否真的没有输入、cancel 是否优先、permit nonce 是否匹配、helper 是否提前执行
`Engine::Run`。⇒ **脚本改动必须由真实 helper 行为测试把关**，不再要求"先过静态 parser"。
（§B-144 的回退与此一致：当时正是因为没有可信的静态保护才回退。）

### **下一窗口 Step 1 的精确边界（裁决 §三，新增约束）**

**只改** `NATIVE_INPUT_HELPER_ENTRY` 这一处常量；**只新增** `ready_file`、permit 等待、EXECUTE gate。

- **插入点保持**：`$progress` → **two-phase block** → `[CoolzhuNative.Engine]::Run(`；
- **禁止移动**：`cancel` 初始化、`progress` 初始化、**cleanup 逻辑**——
  这些**已经过旧路径验证**，挪动它们等于把已验证的行为重新置于风险中。

### **测试 permit 必须显式可区分（裁决 §五，新增约束）**

Phase 1 可以造 test permit stub，但它必须**一眼可辨**：用 `protocol_mode=test` 或
`TestPermitSignal` 之类的显式标记。

**禁止**让它产生类似 `PermitGate issued` 的日志或记录——否则将来会出现
"**测试文件看起来像生产授权记录**"，那是比缺测试更坏的结果（会让人误以为授权链已成立）。

### **opt-in 的终局（裁决 §六，补一句）**

`two_phase_helper` 只限开发/测试阶段；**最终生产必须关闭 optional path、改为协议版本门禁**
（`if helper protocol < required ⇒ reject`）。理由是它要防的不只是"新旧混跑"，而是
**"不同入口不同安全等级"**——那正是最难发现的形态。

### **Phase 1 Helper Side 范围（§四）**

只做：**PowerShell/C# helper 生命周期改造 ＋ T6 真实 helper 测试**。
**暂不接**：`ExecutorStore`、`PermitGate`、`computer_use_executor.rs:228`。

- **插入位置**（已批准）：`$progress` 创建之后、`[CoolzhuNative.Engine]::Run(` **之前**
  ——满足"READY 必须早于任何输入 API"。
- **脚本状态要求（§六）**：`initialize → create files → write READY → wait permit`；
  等待循环必须 `if cancel_file: abort` → `if permit_file: validate nonce: break` → `heartbeat` →
  **有界** `sleep`。**禁止** `while(true){}`、**禁止**不可控 `sleep(1000)`。
- **opt-in `if ($r.two_phase_helper)` 保持**（§九）：协议切换期不得影响旧路径测试／非 CU 流程／其他输入族；
  但**只能存在于开发/测试阶段**——最终生产不能长期"部分 `controlled_*` 新协议、部分旧协议"，
  **最终必须统一由协议版本门禁判定**。

### **Phase 1 测试（§七／§八）**

Phase 1 **不能假装已有完整 `PermitGate`**：T6 不测 `ExecutorStore`／`PermitGate`，只测
"helper 能否进入**等待授权但不可输入**状态"。测试可用**测试专用 permit stub**，
但必须明确**它不是生产授权**；**禁止**"写生产格式 permit 然后宣称 `PermitGate` 已通过"。

| 用例 | 流程 | 断言 |
| --- | --- | --- |
| **T6-A** | spawn → READY → 等待 → timeout | `physical_input_count == 0` |
| **T6-B** | READY → permit nonce mismatch | 拒绝，`input = 0` |
| **T6-C** | READY → valid **test** permit → EXECUTE | 输入发生 |

（另有既有 T6／T7／T9／T11／T12 与 T13 的清单见前文；本阶段聚焦 T6-A/B/C。）

### **Phase 1 完成标准（§十）**

`READY 在 Engine::Run 前`／`AwaitingPermit 无输入`／`cancel 优先`／`错 nonce 拒绝`／
`无 permit 超时退出`／`合法 permit 执行`／`旧路径未受影响`——**全部满足**才进入 Phase 2。

### 继续保持不做（§十一）

不改 `:228`；不写**生产** permit 文件；不接 `ExecutorStore`；不迁移真实 safety DB；不解除隔离；
**不用静态脚本检查代替真实 helper 测试**。

### 实现纪律（§三／§四／§五，按生命周期顺序而非文件顺序）

**Phase 1 · helper 双相化**：让 helper 真的支持
`START → READY → WAIT_PERMIT → PERMIT → EXECUTE → INPUT`。
**此阶段结束时旧路径仍未切换**（允许存在新协议代码，但生产 executor 不调用）。
出口条件：先过 **T6**（`spawn → READY → wait`，`physical input = 0`）与 **T12**
（伪 READY：不是 `ready_file exists` 就算，必须 **valid nonce ＋ valid state ＋ valid helper identity**）。

**Phase 2 · 宿主编排**：由 **web-console** 接入（它持有 `ExecutorStore`／`PermitGate`／输入安全状态）：
`prepare helper → wait READY → capture identity → ExecutorStore.register → PermitGate.issue
→ PermitGate.consume → write permit`。
**额外硬要求**：**web-console 不得自己读 helper 的 PID**——身份**唯一来源**是
`NativeInputOutcome → ProcessInstanceEvidence`。否则会重新产生"core 捕获一次、web 再查一次"的
**两个身份来源**（本轮已多次防这类漂移）。

**Phase 3 · 切换 `controlled_*` 调用**（**唯一危险阶段**）：`controlled_click → prepare → authorize
→ execute`。**必须一次完成**；不允许"部分 `controlled_*` 走新路径、部分走旧路径"
（否则同一动作会出现"有 permit / 无 permit"两种语义）；新协议失败 ⇒ **fail closed**，
**不得**回退旧 helper。

### 切换保护开关（§四）

增加 `two_phase_helper_required = true`：**它不是 fallback**，而是**启动时检查生产组合是否完整**
（`helper protocol = 1`、`host protocol = 1`、`permit schema` 兼容、`executor identity supported`）。
任何一项不满足 ⇒ **该能力启动失败**，**不是**切回旧路径。

### 真实测试顺序（§五，分三组）

1. **helper 独立**（无需真实 permit）：**T6** READY 无输入、**T12** READY 伪造、**T11** 旧 permit nonce。
2. **完整授权链**（需 web-console）：identity register ＋ permit issue ＋ permit consume ⇒ **T8** 正常执行、
   **T7** cancel 优先、**T9** nonce 错误。
3. **故障窗口**（依赖真实状态机）：**T10** READY 后崩溃、permit consumed 后 crash、execute 前 crash、
   execute 后 crash。

### schema（§六）

**暂不新增**——已有 permit 状态、executor identity、helper lifecycle 足以表达。
**只有**实现中发现"必须持久化 `AwaitingPermit` 的 helper"时才新增；**不得为记录中间态扩大安全库**。

### 非批次项（§七）

非空基线／并发释放的确定性场景：**批准但排序放后**——它属"验证已有资源模型"，
不解除当前输入阻塞；**不要在切换窗口之前引入额外共享状态测试**。

### 下一次窗口必须按此顺序（不得改变）

**Phase 1 · helper 协议落地**：`spawn → READY → await permit → EXECUTE → input`。
先通过 **B127-T6／T7／T9／T12**；重点是证明 **READY 状态＝零物理输入**。

**Phase 2 · 宿主身份登记**：`READY → capture ProcessIdentity → ExecutorStore → executor_instance_id`。
验证 PID ＋ creation_time、**PID 复用**、identity mismatch。

**Phase 3 · Permit 真消费**：把当前的 attempt 占位替换为**真实 `executor_instance_id`**；
`ExecutorStore → PermitGate.issue → PermitGate.consume → permit_file`。

**Phase 4 · 恢复 EXECUTE**：最后才动 `computer_use_executor.rs:228`。
注意措辞：**不是"接入 Permit"，而是"把原生输入触发点移动到 EXECUTE gate 之后"**。

### 提交与切换的关系（§四）

- **Git 提交可以拆**（helper protocol／executor identity／permit integration／tests 分开提交没问题）；
- **产品行为切换必须一次完成**——不得出现"helper 已等 permit 而 executor 未生成 permit"的生产分支；
- 落地方式：**feature flag / atomic merge**，或所有提交进入**同一个 release candidate**。

### 切换前门禁（§五，新增要求：`8.3c-A Preflight Gate`）

恢复 `:228` **之前**必须校验四项**一致**：**helper protocol version ＋ host protocol version ＋
permit schema version ＋ executor identity support**。理由：helper 是 **PowerShell/C#**、不是 Rust
内部模块，**协议版本必须显式存在**；否则"旧 helper 接新宿主／新 helper 接旧宿主"都会发生。
⇒ 实现时需新增/确认这四项版本的可交换字段与比对函数（当前**尚未存在**，见 T13）。

### 测试清单（§六）

必测 **T6**（READY 等待无输入）、**T7**（cancel 优先于 permit）、**T8**（完整链输入发生）、
**T9**（错误 nonce）、**T10**（READY 后崩溃）、**T11**（旧 permit 文件残留）、**T12**（伪 READY）、
**T13**（**旧 helper 协议被拒绝**：没有 READY／EXECUTE 的旧版本 helper ⇒ 拒绝执行，
**不能自动降级**）。

### 环境与验收命令（§八）

文件范围预计：`computer-use-core`（helper script／protocol structs／native input adapter）、
`web-console`（executor／permit integration／ExecutorStore）、`tests`（helper process fixtures）。
环境需要：可运行的 PowerShell helper、临时目录、**独立测试 safety root**、**不触碰当前真实 input scope**。

验收**至少**跑：

```powershell
cargo test  -p coolzhu-computer-use-core --offline
cargo test  -p coolzhu-web-console --offline
cargo test  -p coolzhu-core-runtime --offline
cargo build --workspace --offline
```

**不能只跑 `cargo build`**——本会话已三次出现"编译通过但测试构造点失败"
（§B-122／§B-125／§B-132 均有记载）。

### 继续保持不做（§七）

不接 `:228`；不改真实输入 backend；不迁移真实 safety DB；不放行 incident；不做真实桌面测试；
**不允许旧 helper fallback**。

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
