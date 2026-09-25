# Goal 循环状态图化改造 · 方案设计

- 文档编号：`DESIGN-GOAL-LOOP-001`
- 关联问题：`CUR-GOAL-LOOP-001`（Goal `plan→execute→verify` 单向、缺失败回退）
- 日期：2026-07-22
- 状态：草案（待评审）
- 需求拆解见同目录：`2026-07-22-goal-loop-issue-backlog.md`

---

## 1. 背景与问题

coolzhu 的 Goal 模式已具备一套多角色协作雏形（commander / planner / implementer / verifier），
但阶段推进是**硬编码的线性函数链**：

- `next_runnable_goal_phase_id`（`main.rs:34716`）仅返回第一个 `status == "running"` 的 phase，
  **不做任何基于结果的路由决策**；
- `dispatch_ready_goal_phases`（`main.rs:31154`）按 commander review 的 `action == "ready_to_dispatch"`
  逐个派发，派发即把 phase 置 `running`；
- 没有"verifier 不通过 → 回 implementer""implementer 受阻 → 回 planner"的回退边；
- phase 持久层（`goal_phases` 表）**没有** `retry_count` / `verdict` / `block_reason` 字段，
  无法承载"带原因/证据的回退 + 重试上限"。

对标 LangGraph / AutoGen / CrewAI / MetaGPT 后的结论：**不引入外部框架本体**，
而是把 Goal Runner 从"函数链"抽象成一张**显式状态图**（节点=phase，边=路由规则，
共享 State=goal + phase 持久字段）。回退、重试上限、断点续跑、人工介入都成为图上的一等公民。

## 2. 目标 / 非目标

### 目标（本设计覆盖）
- G1（P0）：verifier 判定不通过 → 自动回退到 implementer 重改，携带 reason + evidence。
- G2（P0）：implementer 判定受阻 → 自动回退到 planner 重规划，携带 reason + evidence。
- G3（P0）：回退带**重试上限**，超限则置 `blocked` 并暂停等人工，杜绝死循环。
- G4（P1）：阶段间交接升级为**结构化 handoff 契约**（强类型产物，替代自由文本摘要）。
- G5（P1）：崩溃/中断后可**从任意 phase 断点续跑**（复用 SQLite 状态）。
- G6（P1）：高风险阶段支持 **human-in-the-loop 断点**（复用现有权限系统）。
- G7（P2）：统一 **trace/事件**，回退路径可回看调试。

### 非目标（明确不做）
- 不引入 LangChain / Dify 的重量级依赖栈或 Python 生态绑定。
- 不做可视化流程编辑器（远期 P3，另立设计）。
- 不重写 commander review 的评审语义，只在其结果上增加路由。

## 3. 现状勘察结论（基于真实代码）

| 事项 | 现状 | 位置 |
|---|---|---|
| 下一阶段选择 | 只找 `running`，无路由 | `main.rs:34716` |
| 阶段调度 | commander review → 派发 ready 阶段 | `main.rs:31154` |
| 单阶段执行 | `run_goal_phase_once` | `main.rs:13790` |
| 阶段状态回写 | 仅 `status` + `updated_at` | `update_goal_phase_status_connection` `main.rs:35166` |
| phase 数据结构 | 无 retry / verdict / reason 字段 | `struct GoalPhaseDto` `main.rs:39476` |
| phase 状态计数 | 已含 `failed`（枚举已预留） | `GoalPhaseStatusCounts` `main.rs:39539` |
| 目标级迭代上限 | **已有** `max_iterations` / `current_iteration` | `struct GoalDto` `main.rs:39504` |
| 终止条件载体 | **已有** `completion_condition: JsonValue` | `struct GoalDto` `main.rs:39514` |
| 建表/迁移 | 已到 `user_version = 7`，v6 有加列迁移模板 | `main.rs:33725 / 33786` |
| 全量重写陷阱 | phase 保存走 `DELETE FROM goal_phases` + 全量 `INSERT` | `main.rs:36004 / 36012` |

**可复用的既有资产**：`max_iterations`（全局刹车）、`completion_condition`（终止条件）、
v6 迁移模板、`GoalPhaseStatusCounts.failed`。设计尽量在其上叠加，减少新建。

## 4. 设计总览：Goal Runner = 状态图

```
        ┌─────────┐   plan ok        ┌────────────┐  work ok    ┌───────────┐
  ─────▶│ planner │ ───────────────▶ │ implementer│ ──────────▶ │ verifier  │
        └─────────┘                  └────────────┘             └───────────┘
             ▲                            │                         │  │
             │  blocked(reason,evidence)  │                         │  │ pass
             └────────────────────────────┘                         │  │
             ▲            fail(reason,evidence) 回退重改             │  │
             └──────────────────────────────────────────────────────┘  ▼
                                                                   ┌───────────┐
        retry 超限任一处 ─────────────────────────────────────────▶│  blocked  │──▶ 人工/终止
                                                                   └───────────┘
                                                                        completed（全部 pass）
```

- **节点**：phase（角色 = planner / implementer / verifier / commander）。
- **边**：由"上一节点产出的 verdict + 当前 retry 计数"决定，而非固定顺序。
- **共享 State**：`GoalDto` + phase 的新增持久字段（§6）。

## 5. 状态模型（phase 状态机）

现有状态字符串：`pending / running / completed / failed / paused`（+ `skipped`）。新增/明确语义：

| 状态 | 含义 | 入边 | 出边 |
|---|---|---|---|
| `pending` | 依赖未满足或未轮到 | 初始 / 被回退目标重置 | → `ready` |
| `ready` | 依赖满足、可派发 | 依赖全 `completed` | → `running`（派发） |
| `running` | 已派发、执行中 | 派发 | → `verifying` / `completed` / `failed` / `blocked` |
| `verifying` | 等待 verifier 结论 | implementer 完成 | → `completed`(pass) / `running`(fail 回退) |
| `completed` | 通过 | verdict=pass | 终态 |
| `failed` | 本次失败（可重试） | verdict=fail 且未超限 | → `running`（重试） |
| `blocked` | 受阻/超限，需人工 | 超 `max_retries` 或 implementer 报阻 | 人工 resume → `ready`，或终止 |

> 说明：`verifying` 可先用逻辑态（不落库独立值）实现，最小改动；后续 P1 再显式化。

## 6. 数据模型变更

### 6.1 `GoalPhaseDto` 新增字段（`main.rs:39476`）

```rust
struct GoalPhaseDto {
    // ...现有字段...
    retry_count: u32,              // 已重试次数，默认 0
    max_retries: u32,              // 本阶段重试上限，默认 2
    last_verdict: Option<String>,  // "pass" | "fail" | "blocked"
    last_reason: Option<String>,   // 回退原因（人读）
    last_evidence: Option<JsonValue>, // 证据（日志/diff/失败输出等结构化）
    route_hint: Option<String>,    // 回退目标 phase_id 或角色（可空，空则按默认规则）
}
```

### 6.2 SQLite 迁移 v8（`ALTER ADD COLUMN`，避免重建表触发 FK 级联）

```sql
-- apply_session_migration_v8：仅当 user_version < 8
ALTER TABLE goal_phases ADD COLUMN retry_count    INTEGER NOT NULL DEFAULT 0;
ALTER TABLE goal_phases ADD COLUMN max_retries    INTEGER NOT NULL DEFAULT 2;
ALTER TABLE goal_phases ADD COLUMN last_verdict   TEXT;
ALTER TABLE goal_phases ADD COLUMN last_reason    TEXT;
ALTER TABLE goal_phases ADD COLUMN last_evidence_json TEXT;
ALTER TABLE goal_phases ADD COLUMN route_hint     TEXT;
PRAGMA user_version = 8;
```

> 选 `ALTER ADD COLUMN` 而非 v6 式重建：新字段全部可空或带默认，无需搬数据；
> 且**不触发** `goal_phases` 的 FK 级联，规避 `web-console-save-cascade-trap`。
> 同步更新：`query_goal_phases_connection`（`main.rs:35043`，SELECT 增列）、
> phase 全量重写的 `DELETE + INSERT`（`main.rs:36004/36012`，INSERT 增列）。

### 6.3 全量重写路径必须纳入新列（关键风险点）

phase 保存走 `DELETE FROM goal_phases WHERE goal_id=?` + 逐行 `INSERT`。
新增 6 列**必须**同时改 INSERT 列表与绑定参数，否则重写会把 retry/verdict 清零丢失。

## 7. 路由核心改造

### 7.1 `next_runnable_goal_phase_id` → 结果驱动路由（`main.rs:34716`）

由"找 running"升级为决策函数（签名可保持 `Option<String>`，或返回富结果枚举）：

```
决策顺序：
1. 若存在 running/verifying 阶段 → 返回它（继续等执行/验证结果）。
2. 读最近一次 verifier verdict：
   - pass  → 该 implementer/verifier 阶段置 completed，选下一个依赖满足的 pending。
   - fail  → 目标 implementer 阶段：retry_count+1；
             若 retry_count <= max_retries → 置 running（携带 last_reason/evidence 重跑）；
             否则 → 置 blocked。
3. 若 implementer 报 blocked（受阻回 planner）：
   - 对应 planner 阶段 retry_count+1；未超限 → planner 置 ready（重规划）；超限 → blocked。
4. 全局刹车：若 goal.current_iteration >= goal.max_iterations → 目标 paused/blocked。
5. 无可跑且无未完成 → 判 completion_condition：满足则 goal completed。
```

### 7.2 `dispatch_ready_goal_phases` 增强（`main.rs:31154`）

- 派发前把 phase 的 `last_reason` / `last_evidence` / `retry_count` **注入 intent**
  （现 intent 构造在 `main.rs:31263`），让回退目标 agent 知道"为什么被打回、上次证据是什么"。
- verifier 阶段完成时，解析其结论写回 `last_verdict` / `last_reason` / `last_evidence`
  （新增 `record_phase_verdict_connection`，与 `update_goal_phase_status_connection` 并列）。

### 7.3 verdict 从哪来

verifier 阶段的完成上报需产出结构化结论。最小实现：约定 verifier 完成消息内嵌
`{"verdict":"pass|fail","reason":"...","evidence":{...}}`，由后端解析回写；
解析失败按 `fail` 兜底并记 trace，避免"无结论即静默通过"。

## 8. Handoff 结构化契约（G4 / P1）

现交接是文本摘要（前端 `taskRenderHandoffSummary`）。升级为带 schema 的产物：

```jsonc
{
  "from_phase": "impl-3", "to_phase": "verify-3",
  "verdict": "fail",
  "reason": "单测 3 例失败",
  "evidence": { "logs": "...", "diff_ref": "...", "failing_tests": ["..."] },
  "artifacts": ["path/a.rs"], "retry_count": 1
}
```

落点：扩展 handoff 记录结构 + `insert_chat_handoff_sqlite`；前端 `syncTaskCardFromGoals`
渲染 verdict 徽标与回退原因。

## 9. Checkpoint / HITL / 可观测（P1–P2）

- **Checkpoint（G5）**：状态已在 `goal_phases`，补一个"从指定 phase 恢复"的入口
  （重置其后阶段为 pending，目标 phase 置 ready），并在 `api_run_next_goal_phase` 支持续跑。
- **HITL（G6）**：phase 增 `requires_human_ack: bool`（可先复用 `route_hint`/事件标记），
  命中时置 `blocked` 并发事件，走 `core-runtime` 权限确认后 resume。
- **可观测（G7）**：所有路由决策写 `goal_events`（已存在表，`main.rs:33741`），
  事件类型如 `goal-phase-verdict` / `goal-phase-retry` / `goal-phase-blocked` / `goal-phase-rerouted`。

## 10. 分阶段实施

| 里程碑 | 内容 | 交付 |
|---|---|---|
| **M0** | v8 迁移 + `GoalPhaseDto` 加字段 + 读写/全量重写三处同步 | 编译通过、迁移测试 |
| **M1（P0）** | 7.1/7.2 路由与回退 + 重试上限 + blocked | 闭合 `CUR-GOAL-LOOP-001` |
| **M2（P1）** | 结构化 handoff（G4）+ 断点续跑（G5） | 交接契约、续跑入口 |
| **M3（P1/P2）** | HITL（G6）+ trace 事件（G7）+ 前端徽标 | 可观测、人工介入 |

## 11. 测试策略

- 迁移：新建 db 直达 v8；v7 库升级到 v8（旧行 retry 默认 0）——`web-console-exe-lock-verify`
  用 `cargo test -p coolzhu-web-console --no-run` 验证链接，运行实例会锁 exe。
- 路由单测：构造 goal + phases，模拟 verdict=fail/超限/blocked，断言 `next_runnable_goal_phase_id`
  的转移；改全局态用 `config_test_guard()`（见 `test-serialization-offline`）。
- 回归：`cargo build -p coolzhu-web-console --offline`；`cargo test --test module_linkage_smoke --offline`。

## 12. 风险与回滚

| 风险 | 缓解 |
|---|---|
| 全量重写漏改新列 → retry/verdict 丢失 | §6.3 三处同步清单纳入 PR 检查 |
| FK 级联清空侧表 | 用 `ALTER ADD COLUMN`，不重建表（`web-console-save-cascade-trap`） |
| 回退死循环 | phase `max_retries` + goal `max_iterations` 双层刹车 |
| 托管模型越界改测试掩盖失败 | 每阶段独立 `cargo test` + `git diff` 核查（`goal-host-glm52-overreach`） |
| GLM 经百炼 reasoning 钳制 | 沿用现有 clamp（`glm-bailian-reasoning-clamp`） |
| 运行实例锁 exe → link 报拒绝访问 | 先停旧实例或 `--no-run`（`web-console-workspace-singleton`） |

## 13. 框架特性 → coolzhu 移植映射（溯源）

| 框架特性 | 来源 | 本设计落点 |
|---|---|---|
| 图式条件路由 + 回退 | LangGraph | §7 路由核心（G1/G2/G3） |
| Checkpoint / 续跑 | LangGraph | §9 断点续跑（G5） |
| 可编程终止条件 | AutoGen | 复用 `completion_condition`（§7.1 step5） |
| 结构化 SOP 产物 | MetaGPT | §8 handoff 契约（G4） |
| 角色=role+tools+goal | CrewAI | 复用 `goal_role_configs`（已存在） |
| Human-in-the-loop | LangGraph/AutoGen | §9 HITL（G6） |
| 统一 trace | 通用 | §9 `goal_events`（G7） |
