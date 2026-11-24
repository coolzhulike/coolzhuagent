# Fusion Team P1 — 可靠性补齐（决策卡片 / 验收门 / 依赖 / 重启恢复）

日期：2026-11-10
前置：`docs/work-logs/2026-11-09-fusion-team-p0.md`（P0 提交 `992b950b`）
设计：`docs/plans/2026-11-09-fusion-team-design.md`（v3）

## 范围

P0 遗留的 P1 项全部落地：fusion_tasks.title、确定性验收门、重试上限 blocked、
持久决策卡片 + 回答 API、重启恢复（interrupted）、工作项依赖、前端决策卡片与任务面板。

## 实现

### 1. fusion_tasks.title（P1-1）

- `FusionToolScope` 新增 `task_title_hint`（用户原始消息首行截 160 字）。
- `team_plan`：显式 `title` 优先，缺省回退 hint——之前主持不传 title 时任务标题为空。

### 2. 确定性验收门（P1-2，设计 §6.3）

- 新建 `src/fusion/verification.rs`：`GateOutcome::{NotRequired,Passed,Failed,Invalid}`。
  - `FilesExist`：路径限定工作区内（最深存在祖先 canonicalize 后前缀比较；
    Windows `\\?\` 前缀经祖先归一处理，缺失文件不误伤）；文件为空/非文件记缺失。
  - `Command`：逐条执行，复用 `goal_command::run_receipt`（真实宿主回执、
    超时/取消/输入摘要同实现）；首个失败即停，回执数组入证据。
  - 合同非法/未知类型 → `Invalid`。
- `team_accept` 接前置门：`verdict=accept` 且工作项有 `verification_json` 时先跑门：
  - 失败 → 自动落 `rework`（retry+1，证据入 `last_evidence_json`），主持不能"口头通过"；
  - `Invalid` → 拒绝并要求 `team_plan` 修正合同；
  - 通过 → evidence 记 `gate: passed`。

### 3. 重试上限 blocked（P1-3）

- `fusion_work_item_verdict` 的 retry+1/超限→blocked 沿用 P0；
- 超限后除工具回执提示外，房间补 `fusion-status`「重试达上限，已 blocked」，
  引导主持 `team_ask_user` 发决策卡片（不自动无限重试）。

### 4. 持久决策卡片（P1-4）

- `team_ask_user`：`fusion_decisions` 落 `pending`（options_json 形态
  `{options, allow_free_text}`，兼容裸数组）→ 任务 `awaiting_user` →
  房间 `fusion-decision` 消息（人读文本 + `[fusion-decision]{json}` 机读行）。
- 新增 store：`fusion_decision_get` / `fusion_decision_answer`（pending→answered CAS，
  重复回答被拒）/ `fusion_briefs_by_status` / `fusion_work_item_get` /
  `fusion_work_items_sweep_running`。
- 新增 API `POST /api/fusion/decisions/{id}/answer`：
  `{option_id}` 或 `{free_text}`；点选校验选项存在，自由文本受 `allow_free_text` 门控；
  成功后任务回 `executing` + 房间 answered 卡片；随后以合成 fusion 目标用户回合
  复用 `admit → spawn_orchestrator` 恢复推进（决策已落库，事件文本提示主持这是
  规则直达的回答）。pending 决策跨重启保留（库内持久化 + 卡片可再答）。

### 5. 重启恢复（P1-5）

- `startup_sweep(db_path)`：进程启动时（`recover_incomplete_runtime_runs` 后）
  running brief → `interrupted`，running 工作项 → `pending`；不重放任何副作用。
- 编排器启动时同义清扫，且把受影响 brief 转为【恢复】主持事件
  （主持决定重新派发或放弃），房间补 `fusion-status` 提示。

### 6. 工作项依赖（P1-6）

- `FusionWorkItemRow` 增 `depends_on_json`（两处 SELECT 补列）。
- `team_dispatch` 关联 `work_item_id` 时宿主强制校验：依赖未全 done → 拒绝派发
  并列出未完成/已失败依赖；工作项 blocked → 拒绝并要求先调整计划。

### 7. 前端

- `fusion-decision` 卡片：`renderChatMessageText` 拦截 kind → 解析
  `[fusion-decision]` 机读行 → 渲染问题 + 选项按钮 + 可选自由文本输入；
  点击/提交 → `POST /api/fusion/decisions/{id}/answer` → 卡片锁为 answered；
  同 id 的 answered 回执消息到达时同步锁定（跨刷新一致，`fusionDecisionCards` Map）。
- 任务面板：`GET /api/fusion/tasks?room_id=` 扩展返回 `work_items`（含
  status/depends_on/retry/verdict）与 `pending_decisions`；任务窗口列表追加
  Fusion 条目（⚡ 前缀、状态中文、blocked/待决策标记、未完成工作项摘要）；
  房间切换与任务窗口刷新时拉取。
- TTS：`fusion-decision` 未入黑名单（可朗读/通知），brief/result/status/plan 保持静默。

## 验证

- `cargo test -p coolzhu-web-console --offline fusion::`：12/12 通过
  （新增：决策 CAS、工作项清扫、按状态取 brief、FilesExist/Command 门×2、依赖读回）。
- 全量回归见提交信息。
- 真实 e2e：见提交信息。

## 已知边界

- 决策回答恢复路径复用合成用户回合：房间会多出一条「【决策回答】…」user 消息
  （与 answered 卡片并存，作为可读记录）。
- 工作项依赖只拦派发入口；若依赖在派发后失败，已 running brief 不会被级联取消
  （由主持在结果事件里处理）。
