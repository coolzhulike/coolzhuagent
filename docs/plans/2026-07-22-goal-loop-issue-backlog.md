# Goal 循环状态图化 · 需求管理 & GitHub Issue 蓝本

- 关联设计：`2026-07-22-goal-loop-state-graph-design.md`（`DESIGN-GOAL-LOOP-001`）
- 目标仓库：https://github.com/coolzhulike/coolzhuagent
- 用途：本文件即"落到 GitHub 的 issue 清单源"。每个 `GL-xx` 对应一个 Issue，可直接复制或用脚本批量创建。
- 日期：2026-07-22（交付记录补充于 2026-07-25）

---

## 交付记录（2026-07-25）

GL-01~15 已全部实现并提 PR。**下表以实际落地为准**——本文档正文写于设计阶段，
其中的迁移版本号与 `main.rs` 行号已经漂移，阅读时以代码和本表为准。

| 任务 | Issue | PR | 与原计划的出入 |
|---|---|---|---|
| GL-01 SQLite 迁移 + goal_phases 增列 | #2 | #3 | 落地为 **v13** 而非文中的 v8（写文时库在 v7，实际已到 v12） |
| GL-02 GoalPhaseDto 增字段 + 读路径 | #4 | #5 | — |
| GL-03 全量重写路径纳入新列 | #6 | #7 | 另修：WAL 下 deferred 事务改 Immediate；phase id 按 trim 归一 |
| GL-04 verdict 回写 | #8 | #9 | verdict 由调用方显式给出——`complete_goal_phase` 也是手工完成入口，不能凭"走到完成"推断 pass |
| GL-05 结果驱动路由 / GL-06 回退边+重试上限 | #10 / #11 | #12 | 合并交付（拆开 GL-05 近乎空转）。统一到 `retry_count`，取代原有的"事件条数≥3"阈值 |
| GL-07 回退原因注入 intent / GL-08 双层刹车 | #13 / #14 | #15 | `current_iteration` 此前从未递增，刹车形同虚设；改为**模型调用前原子占用**。补 `/budget` 接口（否则耗尽后无恢复路径） |
| GL-09 路由回退单测 | #16 | #17 | — |
| GL-11 前端 verdict 徽标 / GL-12 断点续跑 | #18 / #19 | #20 | — |
| GL-10 结构化 handoff 契约 | #21 | #22 | contract 改 `#[serde(skip)]`，只由服务端构造，不接受 HTTP 伪造 |
| GL-13 HITL 断点 / GL-14 路由 trace | #23 / #24 | #25 | ack 状态机用条件 UPDATE 落到 SQL 的 WHERE，保证 `awaiting → approved/rejected` 单次转移 |
| GL-15 工程：模板 + 工作流 + 文档合入 | — | 本 PR | `gh` 未安装，改用 REST API + 凭证管理器，见 `docs/github-issue-workflow.md` |

**schema 版本**：v13（GL-01 回退路由列）→ v14（GL-10 handoff 契约）→ v15（GL-13 人工确认）。

**验证节奏**：每轮 `实现 → codex 审查 → 按反馈修 → 本地启动验证 → PR`。
改变行为的任务要求 codex **亲自拉起 app 做场景实测**——真实并发/边界请求复现出了多个
纯静态审查发现不了的问题（如并发 approve/reject 双双成功、ack 可凭空批准并解除任意暂停）。

---

## 0. 需求管理体系（建议）

### 0.1 Label 体系
| 类别 | 标签 | 说明 |
|---|---|---|
| 类型 | `type:feat` `type:refactor` `type:migration` `type:test` `type:docs` `type:chore` | 变更性质 |
| 领域 | `area:goal-loop` `area:web-console` `area:sqlite` `area:frontend` | 影响模块 |
| 优先级 | `P0` `P1` `P2` `P3` | P0=闭合核心缺口 |
| 状态 | `blocked` `good-first-issue` | 辅助筛选 |

### 0.2 Milestone
| Milestone | 目标 | 对应设计里程碑 |
|---|---|---|
| `M0 数据模型` | phase 字段 + v8 迁移就绪 | M0 |
| `M1 路由回退` | 闭合 `CUR-GOAL-LOOP-001` | M1（P0） |
| `M2 交接续跑` | 结构化 handoff + 断点续跑 | M2 |
| `M3 HITL可观测` | 人工介入 + trace | M3 |

### 0.3 Issue 模板（建议放 `.github/ISSUE_TEMPLATE/`）
- `task.md`：标题 / 背景 / 方案要点 / **验收标准(AC)** / 依赖 / 影响文件 / 估时
- `bug.md`：现象 / 复现 / 期望 / 环境 / 证据
- 统一约定：正文中文；引用设计文档编号；AC 用可勾选清单。

### 0.4 Epic（父 Issue）
> **EPIC：Goal 循环状态图化，闭合 CUR-GOAL-LOOP-001**
> 将 Goal Runner 从线性函数链重构为显式状态图，支持 verifier→implementer、implementer→planner
> 的带证据回退与重试上限。子任务：GL-01 ~ GL-15。设计见 `DESIGN-GOAL-LOOP-001`。

---

## 1. Issue 清单

> 估时：S≈0.5d，M≈1d，L≈2d。依赖列写前置 `GL-xx`。

### M0 · 数据模型（P0 前置）

#### GL-01　SQLite 迁移 v8：goal_phases 增列
- **type:migration / area:sqlite / P0**
- 背景：phase 缺 retry/verdict/reason/evidence 字段，无法承载回退。
- 方案：新增 `apply_session_migration_v8`，`ALTER TABLE goal_phases ADD COLUMN` 增 6 列
  （`retry_count`、`max_retries`、`last_verdict`、`last_reason`、`last_evidence_json`、`route_hint`），
  `PRAGMA user_version = 8`。**不重建表**（避免 FK 级联，见 save-cascade 陷阱）。
- AC：
  - [ ] 全新库初始化后 `user_version = 8`，6 列存在。
  - [ ] v7 旧库升级后旧行 `retry_count=0 / max_retries=2`，不丢数据。
  - [ ] `cargo test -p coolzhu-web-console --no-run` 通过（运行实例锁 exe，用 --no-run 验证）。
- 影响文件：`main.rs:33786` 迁移区、迁移调度处。
- 依赖：无。估时：M。

#### GL-02　GoalPhaseDto 增字段 + 读路径同步
- **type:feat / area:web-console / P0**
- 方案：`struct GoalPhaseDto`（`main.rs:39476`）加 6 字段；`query_goal_phases_connection`
  （`main.rs:35043`）SELECT 增列并反序列化。
- AC：
  - [ ] 读取 phase 返回新字段，旧库缺列时取默认值不 panic。
  - [ ] 序列化给前端的 JSON 含新字段。
- 依赖：GL-01。估时：S。

#### GL-03　phase 全量重写路径纳入新列（防丢失）⚠️
- **type:refactor / area:sqlite / P0**
- 背景：phase 保存走 `DELETE FROM goal_phases` + 全量 `INSERT`（`main.rs:36004/36012`），
  漏改会在每次 save 时清零 retry/verdict。
- AC：
  - [ ] `INSERT` 列表与绑定参数含全部 6 新列。
  - [ ] 回归：改一次 goal 后重读，retry_count/last_verdict 不被清空（单测覆盖）。
- 依赖：GL-01、GL-02。估时：S。

### M1 · 路由与回退（P0，闭合核心缺口）

#### GL-04　verifier 结论解析 + verdict 回写
- **type:feat / area:goal-loop / P0**
- 方案：verifier 阶段完成上报解析 `{verdict,reason,evidence}`，新增
  `record_phase_verdict_connection`（与 `update_goal_phase_status_connection` `main.rs:35166` 并列）
  写回 `last_verdict/last_reason/last_evidence_json`。解析失败按 `fail` 兜底并记事件。
- AC：
  - [ ] verifier 通过 → phase `last_verdict=pass`；不通过 → `fail` + reason/evidence 落库。
  - [ ] 无结论/解析失败 → 记 `fail` 且写 `goal-phase-verdict` 事件，绝不静默 pass。
- 依赖：GL-02、GL-03。估时：M。

#### GL-05　next_runnable_goal_phase_id 改为结果驱动路由
- **type:refactor / area:goal-loop / P0**
- 方案：由"找 running"（`main.rs:34716`）升级为决策：running/verifying 优先返回；
  否则依据最近 verdict 选择前进 / 回退目标 phase（详见设计 §7.1）。
- AC：
  - [ ] verdict=pass → 选下一个依赖满足的 pending。
  - [ ] verdict=fail → 目标 implementer 阶段返回并进入重试分支（交 GL-06）。
  - [ ] 单测覆盖三条主路径（见 GL-09）。
- 依赖：GL-04。估时：L。

#### GL-06　回退边 + 重试上限 + blocked 状态
- **type:feat / area:goal-loop / P0**
- 方案：verifier fail → implementer `retry_count+1`，未超 `max_retries` 置 `running` 重跑，
  超限置 `blocked`；implementer 报阻 → planner 重规划，同样受重试上限约束。
- AC：
  - [ ] fail 且未超限 → 目标阶段回 running，`retry_count` 递增。
  - [ ] 超 `max_retries` → phase `blocked` + `goal-phase-blocked` 事件 + goal 暂停。
  - [ ] implementer→planner 回退路径同样生效。
- 依赖：GL-05。估时：L。

#### GL-07　dispatch intent 注入回退原因/证据
- **type:feat / area:goal-loop / P1**
- 方案：`dispatch_ready_goal_phases`（`main.rs:31154`）构造 intent（`main.rs:31263`）时，
  把 `last_reason/last_evidence/retry_count` 注入，让被打回的 agent 知道上次为何失败。
- AC：
  - [ ] 回退后派发的 intent 含"上次失败原因 + 证据 + 第几次重试"。
- 依赖：GL-06。估时：S。

#### GL-08　双层刹车 + 终止条件判定
- **type:feat / area:goal-loop / P0**
- 方案：路由中接入 `goal.current_iteration >= goal.max_iterations`（全局刹车，字段已存在）；
  无可跑阶段时按 `goal.completion_condition`（已存在）判定 goal 是否 `completed`。
- AC：
  - [ ] 达 `max_iterations` → goal `paused/blocked`，不再派发。
  - [ ] 全部 phase completed 且 completion_condition 满足 → goal `completed`。
- 依赖：GL-05。估时：M。

#### GL-09　路由回退单元测试
- **type:test / area:goal-loop / P0**
- AC：
  - [ ] 覆盖 pass 前进、fail 重试、超限 blocked、implementer→planner、达迭代上限 5 条路径。
  - [ ] 改全局态用 `config_test_guard()`；离线 `--offline` 可跑。
- 依赖：GL-05、GL-06、GL-08。估时：M。

### M2 · 结构化交接与续跑（P1）

#### GL-10　结构化 handoff 契约
- **type:feat / area:goal-loop / P1**
- 方案：交接从文本摘要升级为带 schema 的产物（from/to/verdict/reason/evidence/artifacts/retry_count），
  扩展 handoff 记录 + `insert_chat_handoff_sqlite`。
- AC：[ ] 回退交接产出结构化 handoff 并落库；[ ] 向后兼容旧文本摘要展示。
- 依赖：GL-06。估时：M。

#### GL-11　前端 verdict 徽标 / 回退原因渲染
- **type:feat / area:frontend / P1**
- 方案：`syncTaskCardFromGoals` / `taskRenderHandoffSummary`（`app.js`）渲染 verdict 徽标、
  retry 次数、回退原因。**改前端后必须 `cargo build -p coolzhu-web-console`**（资源编译期内联）。
- AC：[ ] 任务卡展示 pass/fail/blocked 徽标与重试计数；[ ] 回退阶段显示原因。
- 依赖：GL-10。估时：M。

#### GL-12　断点续跑入口
- **type:feat / area:goal-loop / P1**
- 方案：新增"从指定 phase 恢复"（重置其后阶段为 pending、目标置 ready），
  `api_run_next_goal_phase`（`main.rs:13475`）支持续跑。
- AC：[ ] 指定 phase 恢复后仅重跑该及后续阶段；[ ] 崩溃重启后可续跑不重头。
- 依赖：GL-06。估时：M。

### M3 · 人工介入与可观测（P1/P2）

#### GL-13　Human-in-the-loop 断点
- **type:feat / area:goal-loop / P1**
- 方案：phase 增 `requires_human_ack`，命中置 `blocked` 发事件，经 core-runtime 权限确认后 resume。
- AC：[ ] 标记阶段派发前暂停等确认；[ ] 确认后继续，拒绝则终止并记事件。
- 依赖：GL-06。估时：M。

#### GL-14　路由 trace 事件补齐
- **type:feat / area:goal-loop / P2**
- 方案：路由决策写 `goal_events`（表已存在 `main.rs:33741`）：`goal-phase-verdict/retry/blocked/rerouted`。
- AC：[ ] 每次前进/回退/阻塞均有结构化事件，可在事件流回看。
- 依赖：GL-06。估时：S。

#### GL-15　工程：gh 工具链 + issue 模板 + 设计评审落库
- **type:chore / area:web-console / P2**
- 背景：当前环境 `gh` 未安装，无法命令行建 issue。
- AC：
  - [ ] 记录 `gh` 安装与认证步骤（或改用 REST API 的替代路径）。
  - [ ] `.github/ISSUE_TEMPLATE/` 建 task/bug 模板。
  - [ ] 本设计与 backlog 合入 `main`。
- 依赖：无。估时：S。

---

## 2. 依赖拓扑（排期参考）

```
GL-01 ─▶ GL-02 ─▶ GL-03 ─▶ GL-04 ─▶ GL-05 ─▶ GL-06 ─┬─▶ GL-07
                                          └▶ GL-08 ─┘   ├─▶ GL-10 ─▶ GL-11
                                          GL-05,06,08 ─▶ GL-09    ├─▶ GL-12
                                                                  ├─▶ GL-13
                                                                  └─▶ GL-14
GL-15（独立，可随时并行）
```

**关键路径（P0，闭合 CUR-GOAL-LOOP-001）**：GL-01 → 02 → 03 → 04 → 05 → 06（+08+09）。
先交付这条即可上线核心回退能力，M2/M3 增量迭代。

## 3. 落地到 GitHub 的方式（三选一，待确认）

1. **脚本批量建**（推荐）：装 `gh` 并 `gh auth login` 后，跑 `tmp/create-goal-loop-issues.sh`
   一键创建 labels + milestones + 15 个 issues（脚本随本文件附带）。
2. **我代为建**：确认后由我在本机执行 `gh`（需先安装+认证；创建属写外部系统，逐步确认）。
3. **网页手动**：以本文件为源，逐条复制到 GitHub New Issue。

> 创建 issue = 向外部公开系统写入，需你明确同意后再执行；本轮仅产出蓝本与脚本，不擅自创建。
