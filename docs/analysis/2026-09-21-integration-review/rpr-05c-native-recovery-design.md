# RPR-05c 接线设计：最小原生恢复入口（裁决第 11 项，发布选 a）

用途：把裁决第 11 项落成可实施的设计，供集成负责人（`main.rs` 路由）与桌面壳（Tauri 对话框）分别接线。
状态：**设计，未实施**。已实现的是 RPR-05b-1 的互锁与解除事实层（见台账 §B-17）。

## 1. 裁决对发布形态的硬约束（不可协商）

- 互锁与**最小原生恢复入口**共同构成可发布单元。不要求完整恢复页面或视觉美化，只需**键盘可达的原生对话框**，包含：当前隔离事件与原因；旧 helper 静止、释放义务等**检查结果**；明确的**恢复确认**与**不能恢复时的原因**；由**同一输入安全服务**执行恢复请求。
- CLI/API **只允许**：查询隔离状态、查看恢复事件、请求打开原生恢复对话框。
- CLI/API **不允许**：`--force-unlock`、`--yes`、`confirmed=true`、或任何直接把 `Quarantined` 改为 `Idle` 的普通 HTTP/CLI 操作。
- **"本机 / TTY / 可审计"都不自动证明操作者是人**——因此不允许以"仅写审计日志"的 CLI/API 解除作为发布形态。
- 不改变当前聊天布局、统一参数页与视觉方案。
- 报告用语：**"恢复记录可审计" ≠ "解除操作已取得可信人工确认"**。

## 2. 已有可复用件（RPR-05b-1 交付）

| 能力 | 位置 | 说明 |
| --- | --- | --- |
| 判据 | `computer_use_store.rs::has_unconfirmed_release` | 存在**未解除**的未确认释放 |
| 前置检查展示 | `unconfirmed_release_facts(session_id, turn_id)` | run 清单、step 数、时间范围 |
| 解除（只追加） | `resolve_unconfirmed_release(&ReleaseResolutionRequest)` | 含 operator_source / operator_id / reason / operator_check_note / 旧·新 epoch / 覆盖 run·step 计数 |
| 审计回显 | `release_resolutions(session_id, turn_id)` | 前置/后置检查、时间、决定 |
| 表 | `computer_use_release_resolutions`、`computer_use_release_resolution_runs` | `IF NOT EXISTS` 建表，**升级/重启不清空** |

未交付：一次确认凭据、原生对话框、`main.rs` 路由、桌面壳与服务之间的调用通道、错误码文案。

## 3. 设计：一次性确认凭据（本设计的关键，也是最容易被做假的地方）

**目标**：让"模型可触发的路径"在物理上拿不到解除能力，同时明确本保证的**边界**（与裁决第 5.2 节一致：只保证产品正式入口的授权边界，不宣称能抵御同用户、完全访问权限下任意程序对产品的篡改）。

**机制**：
1. 每次启动时，**package launcher**（`packages/app-launcher`，已有 `web_console_runtime_environment` 之类的环境注入点）生成一个**每次启动唯一的随机密钥** `recovery_secret`，同时交给 web-console 与 Tauri 桌面壳（同一父进程分发）。
2. web-console 只持有它用于**核验**；Tauri 壳只在用户于原生对话框上按键确认后，用它计算
   `proof = HMAC(recovery_secret, incident_id ‖ expected_revision ‖ recovery_attempt_id ‖ nonce)`。
3. 恢复请求必须携带 `incident_id`、`expected_revision`、`recovery_attempt_id`、`nonce` 与 `proof`。服务端核验：作用域正确、revision 匹配、`nonce`/`recovery_attempt_id` **一次性消费**（持久化）、且 `proof` 正确。
4. **模型可达的路径（聊天工具、Goal、子 Agent、MCP、普通聊天 HTTP、CLI）都没有 `recovery_secret`**，因此无法构造合法 `proof`——这就是"仅审计日志"与"可信人工确认"的分界。

**必须同时声明的边界（写进公开文档与代码注释，不得省略）**：
- 同一用户、完全访问权限下的任意程序**仍可能**读取该密钥或直接改写数据库；本机制不防这一类攻击。
- 密钥**不得**写入日志、不得回显给任何 UI、不得进入聊天历史或工具结果。
- 若以后要做更强保证（例如 OS 级隔离的 IPC 或凭据保护），另立工单；**不得**在实现里悄悄放宽。

**备选（若 launcher 注入不便）**：由 Tauri 壳启动时通过其自有私有通道（命名管道/回环套接字 + 首帧握手）把密钥交给 web-console。选择理由与代价必须写进实施报告；**不允许**退化方案（例如写明文文件或允许"本机来源即信任"）。

## 4. 状态机与检查项（沿用上一轮 §5.4，不回退）

```
Quarantined
  ↓ 本机用户发起，取得唯一恢复所有权
Recovering
  ├─ 检查或收尾失败 → Quarantined
  └─ 条件满足并持久化 → Idle（新 epoch）
```

解除必须**同时**满足（逐条在对话框里展示，未通过项必须显示原因，**不得**提供"忽略所有检查继续"的按钮）：

| 条件 | 要求 |
| --- | --- |
| 旧输入者已停止 | 能确认旧 helper 已退出或已无法继续产生输入；**不能只看 PID 不存在或心跳过期** |
| 恢复作用域正确 | 针对同一交互资源，不是其它桌面或其它事件 |
| 释放义务已处理 | 必要时执行**受控、限次**的释放专用动作，并取得足以支持当前输入状态的证据 |
| 没有已知人工冲突 | 用户完成恢复确认，并知道后续任务将重新取得控制 |
| 恢复记录已提交 | 前后检查、人工决定、新 epoch 已持久化 |
| 旧任务不复活 | 旧 run 保持原终态；后续只能**新尝试、重新观察、重新授权** |

## 5. 接线清单（按归属拆开）

**「集成负责人 / `main.rs`」**
1. `GET /api/computer-use/input-safety/status` → 隔离状态 + `unconfirmed_release_facts` 的前置检查（只读）。
2. `GET /api/computer-use/input-safety/resolutions` → `release_resolutions` 审计回显（只读）。
3. `POST /api/computer-use/input-safety/recovery/request-dialog` → **只**通知桌面壳打开对话框（不解除任何东西；受 CSRF/来源校验约束）。
4. `POST /api/computer-use/input-safety/recovery/confirm` → 携带 §3 的 `proof`，核验后调用 `resolve_unconfirmed_release(...)`；`operator_source` 固定为"原生控制面"，`operator_id` 取本机用户标识；`input_owner_epoch_before` 由探测取得（读不到就传 `None`，**不得填猜测值**）；`input_owner_epoch_after` 为新 epoch。
5. 绝不新增 `force_unlock` / `confirmed` 之类参数；普通聊天/工具路径**不得**引用上述 confirm 路由。
6. 错误码文案表补：`input_release_unconfirmed_interlock`（retry_owner=user）、`input_release_interlock_check_failed`（system）。

**「桌面壳 / Tauri（`modules/gui-desktop`）」**
7. 键盘可达的恢复对话框：展示事件与原因、逐项检查结果、不可恢复时的原因；确认键触发 §3 的 `proof` 计算与第 4 项调用。
8. 只在用户按键后计算并发送 `proof`；**不缓存、不回显、不落盘**密钥。

**「launcher（`packages/app-launcher`）」**
9. 每次启动生成并分发 `recovery_secret`（同一父进程给两个子进程），不写入日志。

## 6. 发布与验收（裁决第 11.3 / 11.4 项）

- 互锁代码**可以先合并并完成内部测试**；但**无人工恢复入口的候选版本不得宣传该 CU 能力已有完整运行闭环**。
- **不得**为了没有恢复 UI 而关闭互锁、或以"关闭互锁"维持自动输入继续工作。
- **已存在的隔离记录不得**通过开关、升级、回退或重启自动清空。
- 测试专用解除接缝继续受**编译期**条件限制，不得变成正式恢复接口。
- 无恢复入口的**内部候选**只允许只读、诊断或明确受限的测试范围。

验收用例（缺一不可）：
1. 普通 Web 请求（不带 `proof`）**不能**解除；
2. 脚本化 CLI（含重复 `--yes`/`confirmed=true` 形态的尝试）**不能**解除；
3. **重放**同一 `nonce`/`recovery_attempt_id` **不能**二次解除（一次性消费）；
4. 真实原生确认（正确 `proof` + 未过期 revision）**可以**完成合法恢复；
5. 检查失败或审计提交失败 → **继续隔离**；
6. 跨实例（另一个 web-console 进程/另一个工作区）**不能**绕过；
7. 解除后旧 run **不复活**，历史 `unknown` **不被改写**；
8. 重启后互锁仍然生效（持久化事实）。

## 7. 与本设计相关的未决项

- 台账 §C-13（互锁作用域是否扩到 session/登录会话）：本设计对作用域**不敏感**（状态与解除都按 scope 走），但作用域一旦扩大，对话框展示与一次性凭据的 `expected_revision` 也要跟着扩大——建议与 §C-13 一并决定。
- 台账 §C-12（hook 独立授权来源）与本设计无关，但同属"人工确认边界"类问题，建议一并裁决以统一口径。
