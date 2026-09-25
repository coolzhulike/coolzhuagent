# coolzhu Agent 三栏武侠控制台总体方案

日期：2026-08-24
状态：布局与视觉路线已确定；资源包已生成；前后端实施按本文分期推进。

> 后续用户要求与当前实现优先：981px 以上保持三列停靠（P6 已将覆盖式侧栏收窄至 980px）；左侧快捷工具归右栏，不归中央主舞台。旧原型仅作为布局历史参考，当前实现与后续 P7 记录以新要求为准。

## 一、最终路线

采用“左窄—中宽—右中”的非对称三栏，而不是继续沿用上下对称分区：

```text
┌ 会话/项目栏 ┬──────────── 聊天主舞台 ────────────┬ 任务/运行栏 ┐
│ 236 px      │ minmax(0, 1fr)，优先获得全部余量    │ 328 px      │
│ 可收起/缩放 │ 消息、工具卡、审批卡、输入与主操作   │ 可收起/缩放 │
└─────────────┴─────────────────────────────────────┴─────────────┘
```

选择原因：Agent 产品的高频路径是“选会话 → 连续对话/引导 → 查看运行与审批”。横向分工能让聊天滚动、输入框和代码/工具结果保持同一视觉轴；右栏按需出现，不再让低频窗口长期挤占主舞台。上下分区只适合终端或 Diff 的临时展开，保留为主舞台内部抽屉，而不再作为全局骨架。

## 二、窗口职责与控件位置

### 1. 左栏：定位上下文

保留会话/房间创建、切换、搜索、收件人和工作目录入口，重新组织为：

- 顶部：新会话、搜索、折叠按钮。
- 中部：收藏/分组、项目、会话树；父子分叉以树线和 `branch.svg` 表达。
- 底部：当前工作目录、分支/HEAD、权限概览；详细授权移入设置或右栏审批页。
- 列表项悬浮动作：置顶、归档、更多；默认隐藏，键盘聚焦时同样出现。

聊天室顶部左侧保留一个始终可见的“显示左栏”按钮，因此左栏收起后不会失去返回入口。按钮使用真实 `<button>`，动作名建议为 `chat-left-rail-toggle`，并绑定 `aria-controls="chat-left-rail"` 与 `aria-expanded`。

### 2. 中栏：唯一主操作舞台

中栏从上到下固定为：

- 紧凑会话头：左栏开关、会话名、项目/分支、运行状态、右栏开关。
- 消息流：文本、代码、工具、审批、计划阶段均以内联卡片出现，重要事件才同步到右栏。
- 底部输入坞：附件/上下文、输入框、模式选择、发送/停止；输入区永远不被侧栏遮挡。

输入坞把三个行为明确拆开：

- `发送`：仅空闲时开始新 run。
- `立即引导当前运行`：忙碌时必须命中当前 `expected_run_id`，不做自动猜测。
- `加入队列`：持久化、可编辑、可删除和重排。

停止按钮只表示“停止当前运行”；Goal 的另一个按钮叫“暂停后续阶段”。这两个动作不能合并成含糊的“暂停”。

Logo、竹叶文字和武侠人物以半透明图层放入消息流背景：Logo 8%–12%，人物 10%–15%，远景竹林 5%–9%。它们固定在滚动容器视觉底层、不可点击；消息气泡、代码块与输入坞均有独立的深色实底/毛玻璃衬层，保证可读性。

### 3. 右栏：运行与决策

右栏不是常驻大面板，而是四个可切换页签：

1. `活动`：当前 run、Goal 阶段、Agent roster、handoff 和事件时间线。
2. `队列`：待执行输入、拖拽排序、编辑、删除、手动继续。
3. `审批`：命令/文件/网络请求、作用域和允许的决策。
4. `上下文`：选中消息、token 使用、工具结果、Diff/终端抽屉入口。

聊天室顶部右侧放始终可见的“显示右栏”按钮，动作名建议为 `chat-right-rail-toggle`。等待审批、运行失败或人工确认时，右栏在 `auto` 模式自动展开；用户手动关闭后，本轮不强制抢回，只在按钮和灯笼上显示提示。

现有隐藏的 roster 与 handoff drawer 直接归位到右栏；现有前端数据与渲染函数可以复用。手工 handoff 和任务链要使用新的唯一 action key，不能复用现有会被 Map 同名覆盖的 `task-card-chain`。

## 三、收起、缩放与动态扩展

每个工作区独立持久化：

```json
{
  "version": 1,
  "left": { "mode": "open", "width": 236 },
  "right": { "mode": "auto", "width": 328, "tab": "activity" }
}
```

CSS 主轨道：

```css
grid-template-columns:
  var(--chat-left-track)
  minmax(0, 1fr)
  var(--chat-right-track);
```

- 左栏允许 200–320 px，右栏允许 260–420 px；拖拽结束时保存，双击分隔条恢复默认宽度。
- 收起轨道为 `0px`，侧栏同时设置 `inert`、`aria-hidden="true"`、`min-width: 0` 和 `overflow: hidden`；焦点移回对应开关。
- `Escape` 关闭叠层侧栏；左右侧栏在窄屏不能同时覆盖主舞台。
- 分隔条使用 `role="separator"`，支持方向键微调与 Shift+方向键大步调整。
- 切换动画 160–180 ms；首次恢复布局不播放动画；切换时保存消息列表的底部锚点，避免滚动跳动。

响应式规则：

| 视口 | 左栏 | 右栏 | 主舞台 |
|---|---|---|---|
| ≥1440 px | 默认展开 | 默认 `auto` | 始终优先扩展 |
| 981–1439 px | 停靠，可收起 | 右侧叠层，默认关闭 | 不被压缩到不可读 |
| ≤980 px 或高度 ≤760 px | 左侧叠层 | 右侧叠层 | 同时只开一个侧栏 |

提供“专注模式”：一次收起两栏；再次点击恢复各自上次宽度和模式，而不是恢复固定默认值。

## 四、武侠视觉系统

### 1. 视觉层级

- 主材质：墨绿玉石与深竹青；结构边界用旧金，不用大面积亮金。
- 高光：保留现有漆器光泽，但把最亮区域集中在主操作和状态变化，避免全屏同时发光。
- 消息层：用户/Agent 气泡保持低饱和深色，长代码使用接近纯色的底板。
- 背景层：竹林、Logo、人物只做低对比水印，滚动时不视差晃动。

### 2. 状态特色

- 金色灯笼：右栏顶端的全局运行提示。空闲 3.6 s 轻呼吸，运行 1.8 s，等待审批先脉冲两次再常亮；错误变暗并停止呼吸。
- 玉光：run 成功或阶段验证通过时，沿状态条从左向右掠过一次，约 260 ms。
- 朱红印记：批准、拒绝、需人工确认的状态底板；勾、叉与文字由 DOM/SVG 叠加，不烙死在位图里。
- 动态减弱：系统要求减少动态时取消连续位移、旋转和呼吸，只保留静态明暗与文字状态。

### 3. 图标使用

- 16–24 px 高频控件使用 `assets/icons-wuxia/*.svg`；本轮新增 11 个缺口图标。
- 28–48 px 主操作、空状态和引导卡可以使用 ImageGen 透明 PNG。
- 每个图标按钮至少 36×36 px 热区，必须有 Tooltip、`aria-label`、焦点环和禁用原因。
- 禁止仅靠颜色区分运行/审批/错误；文字和形状必须同步变化。

## 五、前端改造点

### `index.html`

- 将 `.chat-window-panel` 改为三个直接子节点：`#chat-left-rail`、主聊天、`#chat-right-rail`。
- 将当前隐藏 roster、handoff drawer 移入右栏，而不是复制 DOM。
- 在主聊天头加入左右栏开关；输入坞加入显式 start/steer/queue 入口。
- Diff、终端、浏览器、任务链继续作为中栏内部抽屉/卡片，不再占全局固定行。

### `styles.css`

- 以 CSS 变量管理三轨宽度、收起和叠层状态，删除最终覆盖为“两列”的末端规则。
- 所有主栏设置 `min-width: 0`；代码块、工具卡和消息内容允许内部横向滚动。
- 新增侧栏遮罩、拖拽分隔条、焦点态、减弱动态和半透明聊天背景层。
- 复用现有竹叶横幅动效，但限定到顶栏；不恢复旧舰桥长按/拖拽切页逻辑。

### `app.js`

- 新增每工作区布局状态、宽度钳制、切换、拖拽与响应式 overlay 状态机。
- 在 active workspace 完成刷新/重置后再恢复布局，避免串用上一个工作区的偏好。
- 复用已有 roster/handoff 的刷新和渲染逻辑，为隐藏功能补可见入口。
- 新增统一 capability 渲染：按钮是否显示、是否可用和禁用原因全部取后端 `actions`，不从状态字符串猜测。
- 为 start/steer/queue 生成稳定 `client_message_id`；Steer 强制携带 `expected_run_id`。

## 六、后端审查结论

当前 coolzhu 的强项是持久化 Goal/phase/event、人工确认、重试证据和路由提示；应继续保留 DAG。主要缺口在“运行层”，不能只通过前端补按钮。

| 能力 | 当前实际情况 | 方案 |
|---|---|---|
| 会话/项目/分组 | session 不含 cwd、项目、父子树；只有全局活动工作区 | 项目事实、用户分组、分叉 lineage 三层分开建模 |
| 多会话并行 | 多目标发送与 Goal run-all 仍顺序 await | 不同 session 可并行；同 session 只允许一个活动 run |
| Queue/Steer | send 请求无 mode、幂等 ID、expected run | 分成 start/queue/steer 三个硬契约 |
| 停止 | Goal 只在阶段间检查；在途模型/工具仍可能产生副作用 | run 绑定 CancellationToken/JoinHandle/子进程，确认 aborted 后才完成 |
| 暂停/恢复 | pause 只是调度状态；session resume 是载入历史 | 文案与 API 拆成“停止当前运行”“暂停/恢复后续阶段”“打开会话” |
| 审批 | pending/grant 多为内存；scope 粗；前端可猜决策 | 请求持久化并绑定 run/tool call，后端下发 `available_decisions` |
| Worktree | session 无独立 cwd/Git/worktree 生命周期 | 第一版只显示 cwd/branch/HEAD；完成 managed API 后才显示创建/删除按钮 |
| 检查点 | rollback 只改会话历史，不还原文件 | 区分 `history` 与 `history_git`，文件恢复必须先预览 |
| Goal 竞态 | phase 无 run_id/version claim，可能重复调用模型 | Immediate 事务 + CAS claim；结果绑定 active_run_id |

### 1. 最小运行模型

新增独立侧表，避免被当前 session 全表重写保存路径误删：

- `session_sections`：用户分组、外观和排序。
- `session_runtime_meta`：parent/fork、workspace、cwd、repo、worktree、branch、HEAD。
- `session_runs`：chat/goal_phase、run 状态、attempt、client message、interrupt 时间与错误。
- `session_inputs`：start/queue/steer、expected run、顺序和应用状态。
- `agent_events`：持久事件序列、run/goal/phase 和 aggregate version。
- `approval_requests` / `approval_grants`：请求、允许决策、matcher、版本和有效期。
- `session_checkpoints`：history cursor；可选 Git HEAD、patch 引用和校验值。

同一 session 只能有一个 `starting|running|waiting_approval|waiting_user|interrupting` run；`client_message_id` 在 session 内唯一。`goal_phases` 增加 `active_run_id/version/started_at/completed_at`，claim 必须用条件 UPDATE，影响行数为 0 时返回 409，不能二次调用模型。

### 2. 最小 API

```text
POST /api/sessions/{id}/inputs
  { client_message_id, mode: start|queue|steer,
    expected_run_id?, text, attachments, selected_message_ids }

GET    /api/sessions/{id}/queue
PATCH  /api/sessions/{id}/queue/{input_id}
DELETE /api/sessions/{id}/queue/{input_id}
POST /api/sessions/{id}/queue/reorder
POST /api/sessions/{id}/queue/start

GET  /api/runs/{run_id}
POST /api/runs/{run_id}/interrupt
GET  /api/events?session_id=...&after_sequence=...

POST /api/dispatches
  { client_dispatch_id, target_session_ids,
    strategy: parallel|relay, busy_policy: reject|queue, input }

POST /api/approvals/{id}/decision
  { decision, scope?, matcher?, expected_version }

GET/POST /api/sessions/{id}/checkpoints
POST /api/checkpoints/{id}/restore/preview
POST /api/checkpoints/{id}/restore
  { restore_files: false|true, expected_git_head }
```

审批 MVP 决策固定为：`approve_once`、`approve_session`、`decline_continue`、`cancel_run`。聊天室/全工作区 full access 留在设置页；命令前缀、文件集合和网络 host 作为后续 matcher。

### 3. 真实停止语义

- Interrupt 必须校验 `session_id + run_id`，置为 `interrupting` 后触发模型、工具和子进程取消。
- 命令审批等待也必须被取消；收到执行器 aborted/terminated 确认后才写 `interrupted`。
- 用户停止后保留队列，但不自动执行下一项，避免“刚停止又继续”。
- 服务重启时非终态 run 转 `interrupted(server_restart)`，pending approval 转 `expired`，队列保留，绝不自动批准或删除 worktree。

### 4. 后端能力下发

每个 session/goal 由后端明确返回：

```text
runtime.state
runtime.active_run_id
runtime.flags = [waiting_approval, waiting_user]
runtime.queued_count
actions.stop.enabled/reason
actions.pause_dispatch.enabled/reason
actions.resume_dispatch.enabled/reason
```

前端只按此能力矩阵展示真实动作。未完成 run 中断前，停止按钮显示“尚未支持安全停止”或隐藏；不能把修改 SQLite 状态包装成已经停止。

## 七、实施顺序

### A. 布局与资源壳层

- 落地三栏 DOM、开关、拖拽、持久化、叠层和专注模式。
- 归位 roster/handoff；接入 Logo/人物水印、灯笼、玉光和朱红印记。
- 只展示现有后端真实支持的操作；新能力先以 disabled + 原因或 feature flag 隐藏。

### B. Run/Event 基础层

- 增加 SQLite migration、run 状态机、事件事务/outbox 与重启恢复。
- 将 chat 与 Goal phase 的一次执行都绑定 run_id；Goal claim 加 CAS。
- 实现真实 interrupt，并覆盖模型、工具、审批等待和子进程。

### C. Queue/Steer/并行派发

- 完成稳定幂等 ID、持久队列 CRUD/重排、expected run 校验。
- 支持不同 session 并行、同 session 单 run；relay 作为显式策略保留。
- 接入右栏队列和输入坞三模式。

### D. 审批与调度语义

- 持久化审批请求/授权，后端下发可用决策与 matcher。
- 把 Goal scheduler pause/resume 与 run interrupt 完全分离。
- 等待审批时让右栏、灯笼和消息内联卡同步显示，但以消息内联卡作为主要决策点。

### E. 执行上下文与检查点

- 先增加 session 级 cwd、Git branch/HEAD 的只读展示。
- managed worktree 完成创建、冲突检查、归属和安全删除后再开放按钮。
- `history_git` 捕获 patch、校验和与恢复预览；默认只恢复聊天历史，文件恢复必须二次确认。

## 八、验收标准

- 左右栏均能用鼠标、键盘和触屏开合；收起后中栏实时扩展，无横向溢出或滚动跳动。
- 刷新、切换工作区和不同视口后，布局偏好不串 workspace，窄屏同时只显示一个叠层侧栏。
- Logo/人物背景不影响消息、代码和输入对比度；减弱动态模式下无连续呼吸或位移。
- roster、handoff、队列、审批均有可发现入口；焦点顺序、Tooltip、`aria-expanded`、Escape 行为完整。
- start/queue/steer 不混用；Steer 的 run 不匹配返回 409；重复 client ID 不会重复执行。
- Stop 能终止在途模型/工具/子进程并最终落为 `interrupted`；停止后队列不自启。
- Goal phase 只允许一个有效 attempt claim；并发请求不能重复调用模型或工具。
- 审批决定绑定请求版本，重启不自动批准；可用作用域由服务端给出。
- Worktree/文件恢复按钮在后端不具备安全生命周期前不出现。
- 前端改动后通过 `cargo build -p coolzhu-web-console --offline`，并补布局状态、API 状态机和 migration 回归测试。

## 九、已生成交付物

- 总体效果图：`assets/ui-redesign/concepts/coolzhu-agent-asymmetric-bamboo-layout-concept-2026-08-24.png`
- 控件与状态资源：`assets/ui-redesign/three-column/`
- 高频矢量控件：`assets/icons-wuxia/` 新增 11 个 SVG
- 资源说明与 ImageGen 提示词：`assets/ui-redesign/three-column/README.md`
