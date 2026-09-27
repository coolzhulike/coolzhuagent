# 第八轮裁决固化：RD4 后续收口 + 执行顺序与禁止事项

- **版本**：v1（2026-09-25），来源＝第八轮答复（新一轮问题决策／RD4 后续收口版）
- **性质**：本文件是第八轮裁决的**本地权威正文**；实施与工单引用"本文件 §N"。第七轮正文见 `round7-rulings-and-gates.md`。
- **登记纪律**：沿用四种状态（**已裁决／已实现／生产已接线／目标环境·安装已验收**）；"裁决关闭"不等于"实现关闭"。

## 0. 总体结论

当前阶段**不是"继续补测试"**，而是**进入安全事实链闭环阶段**。已完成：RD4-01（父运行冻结上下文链路贯通）、RD4-02A（安全库契约方向确定 + 部分非法引用拒绝已被测试覆盖）、PKG-L07c（收据代次／消费绑定／竞争写入保护已修复）、reader 容量问题已定位（正常路径风险降低）。

**尚未完成的核心阻塞**：① 输入安全事实**没有真正生产化来源**；② 恢复资格**没有可信签发者**；③ CU 动作事实链**仍缺生产接入**；④ Paint 失败链仍有**事实模型缺陷**，不能进入模型优化阶段。

## 1. RD4-02A 最终决策：采用 A ＋ C 的语义约束

**建立独立输入安全库**，同时明确：**它不是物理输入所有权本身，而是"输入安全事实"的权威记录**。

- **否决 B**（塞进 session SQLite）：会把 `session transaction → input safety fact` 变成同事务，虽技术简单，但破坏已冻结的"**不宣称跨库原子性**"语义。
- **否决纯 C**（完全依赖 OS/input lease、不建持久安全事实）：那样没有持久事实可审计。

**结构与位置**：

```
InputSafetyStore → input-safety.sqlite3
    +-- incidents
    +-- resource_blocks
    +-- recovery_operations
    +-- ownership_epochs
    +-- safety_events
```

位置＝`ResolvedLaunchPaths.input_safety_state_root`；**禁止** workspace `.coolzhu`、session db、temporary path。

## 2. RD4-02A 剩余工作（批准）

**launcher 注入**：新增 `COOLZHU_INPUT_SAFETY_STATE_ROOT`。规则：**生产路径必须注入**；没有即 `root_not_injected` **直接 fail closed**；**不要 fallback**（例如 `%USERPROFILE%\.coolzhu`），否则**测试环境污染生产路径**。

## 3. RD4-02B 最终决策：跨进程资源协调器 ＋ 可信 RecoveryControlGuard

**采用**：跨进程资源协调器 + 可信 `RecoveryControlGuard`。**不是**：进程 mutex ❌／session 级锁 ❌／caller 参数声明 ❌。理由：输入所有权的本质是**桌面级资源**；唯一相近的 `local_model_switch_drain_gate` **不是** CU 输入闸门。

新组件 **`InputSafetyCoordinator`**：
1. **获得资源锁**：scope ＝ `windows-session-{id}` ＋ `physical-input-resource`（**不是** workspace／session_id／turn_id）。
2. 写入 `RecoveryOperationStarted`。
3. 创建 `RecoveryControlGuard { resource_scope, recovery_id, epoch, coordinator_id, allowed_actions }`。
4. **恢复入口必须验证**：**禁止** `paused=true`、`authority="xxx"` 这类调用；必须走 `Coordinator → InputSafetyStore → RecoveryControlGuard`。

## 4. 五个未完成组合测试的决策

| # | 场景 | 决策与期望 |
| --- | --- | --- |
| 1 | `paused=true` 但闸门未关闭 | **必须失败**。输入 `{paused:true}` 而 `InputSafetyCoordinator` **未持有 gate epoch** ⇒ `RecoveryUnauthorized`，**不写终态** |
| 2 | 双进程恢复竞争 | **必须测试**。A 取得 epoch 21；B **epoch 冲突被拒**；B **不能**写 incident、**不能**改 run 状态、**不能**开新输入 |
| 3 | 恢复者死亡 | **加入启动恢复**：`RecoveryStarted → process crash → startup reconcile → acquire new coordinator epoch → continue/reject`；**禁止直接 delete recovery row** |
| 4 | 已 `Interrupted` 但 helper 未知 | **保持隔离**。**不能** `Interrupted == safe`；必须表达 `run stopped / resource uncertain / input blocked` |
| 5 | Goal 有锚点但 chat turn 缺失 | **采用真实关系**：`turn_id/session_id → runtime_runs → owner` 解析；**不能**把 `goal_id` 复制成 `chat_turn_id` |

## 5. §C-30 决策：声明式跳过（采纳建议）

外部可选输入允许跳过；**PowerShell 原生 CU 环境必须 fail-closed**。理由：CU helper 本身依赖 `powershell.exe`，缺失**不是** optional dependency，而是 **execution environment invalid**。

| 项 | 结果 |
| --- | --- |
| 外部扫描工具 | skip |
| 可选模型资源 | skip |
| **PowerShell** | **fail** |
| **原生输入依赖** | **fail** |
| **发布必需 DLL** | **fail** |

## 6. §C-31 决策：P0 修复关闭，"并发发布能力已验收"不关闭

**关闭 P0 修复项**；**不关闭**"并发发布能力已验收"。状态记为：**P0 修复完成 ＋ 并发发布实测验收待补**。理由：写收据临界区修复完成、generation 已加入、消费绑定完成，但**真实交错（publisher A / publisher B race）没有构造**，因此**不能**写"并发发布已证明安全"。

**验收剩余 → 新增独立工单 `PKG-L07c-RACE`**（**不阻塞普通单发布**）：100 次并发导出、10 个失败竞争者、随机 kill；检查 **winner generation 唯一**、**receipt 不错配**、**consumer 永远读完整代次**。

## 7. CU 释放义务缺陷（**新的 P0 级事实错误**）

现象：当前 `NotSent` ＋ `ReleaseUnknown` **可以同时落库**，原因是**释放义务判断没有消费 helper fact**。

**批准 (a)**：释放义务**必须基于事实**——若 `button_down=false` ＋ `injected_points=0` ＋ `path_completed=false` 则 `release_needed=false`。
**批准 (b)**：回执**禁止** `input_delivery=NotSent` ＋ `input_release=Unknown`；必须 `input_release=**NotNeeded**`。
**禁止只改测试**——这是**事实模型错误**，不是测试过严。

## 8. CU Error Contract 决策

`ComputerUseError { code, message, retryable }` **无法携带事实回执**，而"错误返回无法携带 input fact"。

**采用 `ExecutionOutcome`**（而不是直接污染 `Error`）：

```text
旧： Result<StepExecution, ComputerUseError>
新： ExecutionOutcome { execution: Option<StepExecution>, receipt: ActionReceipt, error: Option<ComputerUseError> }
```

理由：**错误不是异常**——CU 执行失败**仍可能产生事实**。→ 立新工单（Phase 3）。

## 9. CU 动作身份决策：采用 A（新增 `ActionScope`）

**不要**强迫所有动作都属于 `StepAction`。理由：原生动作没有模型请求（release／safety cleanup／user direct）。

```text
StepAction   → request_attempt_id required
NativeAction → request_attempt_id optional
```

## 10. Paint R1–R6 后续决策：**不进入模型调参，先修事实链**

0.2.14 已修动作 schema／原图／DPI／拖拽／点击 flags，但 Paint 未通过。

**P0**：

- **CU-01 动作状态**：增加 `none / partial / complete / unknown`，**替代**现在的 `success/error`。
- **CU-02 输入独占**：必须 **one physical keyboard/mouse owner**，否则两个 CU run 可以争抢桌面。

**P1**：

- **CU-03 planner 反馈**：增加 `last_action` / `last_error` / `last_verdict` / `subgoal_progress`（当前缺口：规划器没有显式承接上一动作反馈）。
- **CU-05 UIA 状态**：增加 `selected` / `focused` / `pattern` / `toggle state`（当前只是几何定位）。
- **CU-04** `frame_id` ＋ raster 坐标：**批准**。

**不要继续扩大模型承担**：截图 → 坐标数学。

## 11. 最终执行顺序（阶段与退出条件）

| 阶段 | PR | 内容 | 退出条件 |
| --- | --- | --- | --- |
| **Phase 1**（必须先做） | **PR-RD4-02A** | `InputSafetyStore`、launcher root injection、incident schema | **组合测试 1／2／3 通过** |
| **Phase 2** | **PR-RD4-02B** | `InputSafetyCoordinator`、`RecoveryControlGuard`、cross-process lease | **5 个组合测试全部通过** |
| Phase 3 | PR-CU-FACT | `ExecutionOutcome`、`ActionReceipt`、partial input model、release fact | — |
| Phase 4 | PR-CU-PAINT | `frame_id`、planner feedback、UIA state、ROI | — |
| Phase 5 | PKG-L07c-RACE | 独立补验 | — |

## 12. 禁止事项（后续执行硬约束）

1. **禁止**用 `resource_blocking_refs` 非空＝安全成立；**必须验证来源**。
2. **禁止** `Interrupted` ＝ 释放安全。
3. **禁止**为让 Paint 成功而：放宽 DPI／删除 stale／增大 timeout／关闭输入校验。
4. **禁止**同时重构 `main.rs` ＋ runtime ＋ storage ＋ UI；**必须小 PR**。

## 13. 最终台账状态（裁决给定）

| 项 | 状态 |
| --- | --- |
| RD4-02A | 继续实施，方向已定 |
| RD4-02B | 开工条件满足 |
| RD4-03 | 等 02B 后实施 |
| §C-30 | 已裁决 |
| §C-31 | P0 修复关闭，验收独立保留 |
| CU Release bug | **P0 修复** |
| CU Error contract | 新工单 |
| Paint 优化 | **延后**，先事实模型 |
| 架构路线 | **保留 Rust/Tauri/SQLite**，吸收 DSH 契约，**不迁移 Node runtime** |
