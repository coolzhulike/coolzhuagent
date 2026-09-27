# 《风险决策》评审：源码核对结果与仍需回答的问题

本评审的做法：**用本机源码核对它的每一处关键前提**，把"成立"与"不成立"分开，并列出仍会阻塞 RPR 工单执行的点。
证据口径同 `s0-baseline.md`：源码已观察 / 未验证分开标注。

---

## 一、经源码核对**成立**、可以据此执行（7 项）

| # | 文档主张 | 源码证据 | 结论 |
| --- | --- | --- | --- |
| V-1 | DEC-05 前提："历史参数页已采用空表单保持旧值" | `web-console/src/model_settings.js:186`：`if (value("api-key") \|\| el("clear-key").checked) session.api_key_ref = el("clear-key").checked ? "" : value("api-key");` → 空表单**不触碰** `api_key_ref`（=保持），清除勾选才置 `""`（=清空） | **成立**，且不是"约定"而是现行为 |
| V-2 | 空 ref 的界面状态 | `main.rs:48614` `api_key_configuration_status("")` → `"未配置（能力已启用）"` | **成立**：清空在状态上是可见的，不存在"显示已清空但状态照旧" |
| V-3 | "没有停机窗口只阻塞生产切换，不阻塞 S2.4 开发" | 单写者/outbox/迁移逻辑均可对副本开发，与生产停机无技术耦合 | **成立**：我先前把 D-6 记为"阻塞 S2.4"过保守，采纳该纠正 |
| V-4 | DEC-02："换生命周期所有权，不换端口" | `start_local_gemma()` 确实存在 agent **托管**路径；而用户 bonsai 是**外部**服务 → 两种模式真实共存 | **成立**（但见 B-3 的冲突） |
| V-5 | "关闭工具不能保证本地模型长任务稳定；不解决上下文溢出/生成超时/规划质量" | 与我在 bonsai 上的实测一致：真正的失败原因是 **64K 时 prompt ≈46k、prefill ≈90 tok/s、整轮 >600s 被切断并自动重试**，以及工具回路脱轨；与工具开关无因果 | **成立**，这条是文档最有价值的技术判断之一 |
| V-6 | "测试分组与 0.2.14 的 1175 分母不同，不能相减得新增覆盖率" | 与本仓库 `s0-baseline.md` 的 D-4（当前快照无 MSI 身份）一致 | **成立** |
| V-7 | 对 A-12 的更正采纳（"guard 被多处使用，剩余只是个别手写点"） | `ScopedCurrentDir::enter` 在 `tool-registry/src/lib.rs:3967/4017/4081/4122/4227/4275` 等处被使用 | **成立**，我的更正已被正确吸收 |

## 二、**会阻塞执行**的缺口（必须先定或先做）

| # | 阻塞点 | 源码证据 | 影响 |
| --- | --- | --- | --- |
| **B-1** | **DEC-04 缺"错误如何携带回执"的契约决定** | `computer-use-core/src/contracts.rs:146`：`ComputerUseError { code, message, retryable, retry_owner }` —— **没有回执位**；而 `ComputerUseAdapter::act` 返回 `Result<StepExecution, ComputerUseError>`，`Err` 带不出任何输入事实 | RPR-04（"helper 错误携带回执"）**无法开工**：实现者只能自创第三套类型，正是文档第 19 行自己禁止的。**建议**：给错误加 `receipt: Option<ActionReceipt>`（`#[serde(default)]`，additive，复用 `run_contract::ActionReceipt`），或在 `act` 上引入 `ExecutionOutcome { delivered, receipt, error }`。**需你选一个** |
| **B-2** | **A-2 规定的测试在现有 API 上不可实现** | `windows-process-guard/src/lib.rs`：broker 公开面只有 `acquire` / `owner_epoch` / `is_current` / `release(self)`；`release` **消费** lease，而 lease 由 `TracingAdapter` 持有 → 测试**无法在运行中使某 owner 的 token 失效**（第 218 行要求的那一步） | RPR-02 的 A-2 部分需要**新增 test-only 失效入口**（如 `#[cfg(test)] force_clear_scope(scope)`）或让 epoch 来源可注入。**需你同意在 broker 上加测试接缝** |
| **B-3** | **DEC-02 与现"单活动模型"显存切换直接冲突** | `main.rs` 的 `shutdown_local_model_services_on_exit()` 与 `switch_local_models()` 都按 `local_chat_runtime_config().port` **杀监听进程**释放显存 | 若按 DEC-02 把 bonsai 判为 `external-service`（**不得终止**），则"切到 vision 释放显存"对 8080 **失效**——而这正是 8GB 卡上该切换存在的理由。**需你定**：external 模式下显存由谁释放（要求用户手动 `stop.bat`？还是允许 chat/vision 与外部模型共存？） |
| **B-4** | **A-7 的两件事被合成一个工作包，且 hook 与闸门不在同一路径** | `main.rs` 中**没有** `HookRunner` / `run_hooks` 引用 → web 工具路径**不经**外部 shell hook；hook 在 `core-runtime/conversation` 路径。另外 `runtime_tool_execute` 的 `Deny` 分支在**调用任何 executor 之前**返回（`tool.rs:334`）→ "deny ⇒ 零执行"在该闸门上是**结构性成立**的 | 文档第 468 行"不能单独证明执行器没有先产生副作用"只对**闸门之前的 hook**成立，不是对闸门本身。A-7 应拆为：(a) 审批绑定冻结输入（web + conversation 共用）；(b) hook 不得反转 deny、且不得先于闸门产生副作用（**仅 conversation 路径**）。按现写法会让实现者在不存在的 web hook 上白做工 |
| **B-5** | **DEC-05 的"清空后静默回退环境变量"尚未证实** | `resolve_api_key_ref("")` → `None`（`main.rs:29947`）；空 ref 状态显示"未配置"。但请求时的**凭据解析顺序**（None 之后是否有 env 兜底）我未读透 | 这是 DEC-05 的实现前提。建议 RPR-07 第一步就写一个测试把这个顺序固定下来；**在证实之前不要宣称"清空语义已修复"** |
| **B-6** | **A-8 的跨进程层会牵动打包，文档未计** | 新增"固定原生线程持命名 mutex + helper 生存期监督 + 持久化未清偿释放记录"意味着**新的原生组件与运行目录**，必须进 `config/package-manifest.json` 与 `installer/Product.wxs`，否则装完缺件 | RPR-05 的 8–13 人日**未含**打包/装机验证量 |

## 三、我认为**仍偏乐观**、建议收紧的点

| # | 点 | 理由 |
| --- | --- | --- |
| **R-1** | A-8/RPR-05 的 **8–13 人日偏低** | 计划原文的 S1.3 就是 **7–12 人日**，且当时明确把"跨进程 helper 存活对账、强制接管"列为**后续**；再加 B-6 的打包量。建议按 **12–20 人日**估 |
| **R-2** | "真实 Paint 放行条件"（第 784–791 行 7 条）**缺一条能力级前置** | 我在本机实测：64K 上下文时 agent 组装约 46k prompt、prefill 约 90 tok/s、**整轮超过 agent 的 600s 单次调用硬上限**（`clamp(100, 600_000)`）被切断并自动重试。这与所有权/回执无关，是**能力级阻塞**。Paint 连续实验前必须先定"预算与步数如何匹配 600s 上限"，否则会稳定失败并被误判为模型选点问题 |
| **R-3** | Quarantined **没有退出条件** | 不变量 INV-CU-05 说"重启不能清空 Quarantined"，但文档没给**人工解除**路径。一次无法对账的释放会让该桌面**永久不可用**。需明确"谁能解除、解除要记录什么" |
| **R-4** | EVD-01（0.2.12/0.2.13 提交）**建议直接接受 `provenance_unknown`** | 我已试过一条可行路径（`msiexec /a` 提取包内 `coolzhu-cli.exe` 读内嵌 `GIT_SHA`）：0.2.0 与 0.2.14 **成功**，而这两个**管理安装返回 0 却不落盘**。继续追的成本高于收益，除非能拿到原构建机 |
| **R-5** | "hooks 不得先于闸门产生副作用"这条**在 web 路径上是空的** | 见 B-4：web 路径不跑外部 hook。若按文档把该断言写成 web 侧验收项，会得到"通过"的假证 |

## 四、需要你回答才能开工的问题（4 个）

1. **B-1**：接受"给 `ComputerUseError` 加 `receipt: Option<ActionReceipt>`（additive、复用 `run_contract::ActionReceipt`）"吗？替代方案是改 `act` 的返回类型（影响面更大，涉及 7 处实现与 17 处调用点）。
2. **B-3**：`external-service` 模式下**显存释放由谁负责**？这直接决定 `local_chat_port` 是否还能被 agent 当作"切换时杀进程"的目标。
3. **R-3**：Quarantined 的**人工解除入口**由谁提供、记录在哪？
4. **R-2**：是否同意在 Paint 连续实验**之前**先做一次"预算 / 步数 vs 600s 上限"的匹配设计（并把结果写进放行条件）？

## 五、我建议的执行顺序（与文档一致，仅补两个前置）

RPR-00（源码身份与现有实现核对）→ RPR-01、RPR-02 的**低风险部分** → **先定 B-1、B-2 两个契约决定** → RPR-04 → RPR-03 → RPR-05（含 B-6 打包量）/ RPR-06 → 再进 RPR-07～RPR-16。

**未获这 4 个回答前，我不会开始 RPR-04 / RPR-05 / RPR-07**——它们的实现方式直接取决于上述决定，先做就会写出需要返工的代码。
