# CU 执行器符合性评估 + 释放义务缺陷详述

> ## ⚠ 第四轮裁决对本文件表述的正式更正（覆盖下文旧表述）
>
> 裁决第四节明确："这是**释放义务推导与错误投影的真实缺陷**，必须修复，**不能通过接受两种结果关闭**；核心错误不是'超时太短'，而是一个本来不应存在的释放操作被放进了错误处理链，它自己的失败又改变了原动作的事实和恢复资格。"
>
> | 本文件的原表述 | 正式修订（以下为准） |
> | --- | --- |
> | "该测试是负载下的时序抖动" | "**释放义务推导缺陷**，被额外清理失败暴露；具体单次超时经过未独立复验" |
> | "未按下时多发 UP 是无害的"（我用它解释为何平时不暴露） | "该测试中**通常未暴露问题**，**不作为共享输入资源上的普遍安全假设**" |
> | "同一物理情形必须永远返回同一事实" | "**相同、完整、可信证据**必须确定地产生同一事实；证据缺失时允许明确未知，**但不允许自相矛盾**" |
> | "CU 没有任何事实" | "**新 CU 事实链尚无生产写入**；旧步骤审计和已有回执机制另列" |
> | "有界收尾已经实现" | "**策略已集中**；笔画无界 `join()` 与未迁移入口仍使产品范围承诺不成立" |
> | "普通输入路径已经改好" | "**新原语已实现并有测试**；四个 Web 入口及 GUI 调用点**仍待迁移**" |
> | "请求 attempt 已有，所以身份齐全" | "planner **能提供真实键**；执行器与生产 authority **尚需消费和核对**" |
> | "模型排空完成" | "**已覆盖的客户端路径**已实现；**vision 未登记**，不能据此停止全部相关服务" |
>
> 另：裁决同时固定了"**已出现错误分类；代码允许产生不一致回执并错误收紧恢复资格。未证明这次事故触发了跨 run 误隔离，也未证明已发生真实鼠标卡住。**"——这是本次事故的准确描述口径。
>
> 本文档下方内容中与上表冲突的表述，**一律以上表为准**。

用途：回答两个问题——① 那条出现时序抖动的测试**到底是什么问题**、来龙去脉如何；② **当前 CU 执行器是否符合裁决的预期**，若不符合，阻塞点是什么。
配套：`rpr-execution-blockers.md`（执行台账）、`decision-background.md`（决策背景）、`rpr-05c-native-recovery-design.md`。

口径：带 `file:line` 的是**已核对源码**；实测标注"实测"；判断标注"判断"。本文件同时是**待决策材料**，末尾给出需要裁决的点。

---

# 第一部分：释放义务缺陷（那条测试的根因）

## 1. 现象

合并门禁运行时，`computer_use-core` 的真实子进程测试
`input::stroke::tests::real_helper_pre_input_failure_produces_a_proven_not_sent_receipt` 失败一次：
断言 `Stale`，实得 `ReleaseUnconfirmed`（`input_stroke.rs:942`）。
随后 **单跑 3/3 通过、完整套件单独跑 2/2 通过（99/99）**；失败发生时我正在并发编译（机器有负载）。

## 2. 这条测试在测什么

它给 `controlled_drag_path` 传一个**伪窗口身份**（handle 1 / pid 4 都不是当前前台窗口），并特意**不驱动真实输入**：helper 的 `Native.Check()` 会在任何 `Move`/`Down` **之前**因身份不匹配抛错（测试注释 `input_stroke.rs:914-918` 明确写了这一点）。测试期望：
1. helper 自报失败且写出起点事实：`injected_points=0`、`button_down=false`、`path_completed=false`（`:944-946`）；
2. 回执因此是**已证明未发送**：`NotSent` + `partial=false` + `path_completed=false` + `points=0`（`:950-953`）；
3. 并且 **`input_release = NotNeeded`**（`:954`）——**什么都没按，就不存在释放义务**；
4. 失败种类是 `Stale`（可恢复，允许控制器重新观察后重规划）。

## 3. 根因链（逐段带证据）

**① 释放义务的判定不看事实。** `needs_emergency_release(is_stroke, helper_reported_release_failure, forced_kill, helper_exited_ok)`（`input_stroke.rs:754-761`）：

```rust
is_stroke && !helper_reported_release_failure && (forced_kill || !helper_exited_ok)
```

四个入参里**没有任何来自 helper 事实的值**。本场景里 helper 因**输入前**校验失败而 `throw` → 以**非零状态**退出 → `helper_exited_ok = false` ⇒ `release_needed = true`。
**即：对一次从未按下任何键的动作，仍然会发起一次"独立补发释放"。** 它的注释（`:749-753`）也印证了这一取向："被强杀、或自己以非零状态退出时 `finally` 可能没跑到，左键可能仍按下"——这个推理在"helper 可能已经按下"的假设下成立，但**当 helper 的事实已经证明从未按下时就不再成立**。

**② 那次多余的补发释放本身可能失败。** 负载下它的等待可能到期/失败，于是产生 `release_failure = input_release_unconfirmed`（`:642-650`，注释写"连补发都失败：释放事实无法确认，必须让上层按未确认释放处理并隔离"）。

**③ 分类只看错误文本，且"释放未确认"排第一。** `StrokeFailureKind::classify` 的文档注释就是"**分类只看错误文本，不看事实**"（`:129`），判定顺序里 `mouse_release_failed`/`input_release_unconfirmed` **最先**匹配（`:145-146`）⇒ **它覆盖掉原本正确的 `stale_observation`**。

**④ 后果 A：回执自相矛盾。** `helper_failure_receipt`（`:307-357`）中：
- `input_delivery` 由 `proven_not_sent` 决定，而 `proven_not_sent` **确实看了事实**（`!button_down && injected_points == 0 && !path_completed`，`:312-318`）⇒ 投递事实是**正确**的 `NotSent` ✅
- 但 `input_release` 对 `ReleaseUnconfirmed` **无条件写 `Unknown`**（`:355`，注释"按下过按键时一律 unknown"——**问题在于它没有检查"是否真的按下过"**）

于是同一条回执同时说"**输入未发送**"（已由事实证明）与"**释放未知**"。而"未发送"在语义上蕴含"没有释放义务"，诚实值应为 `NotNeeded`——**这正是测试断言的期望值**。这条矛盾能通过 `receipt.validate()`（它只校验 NotSent 与 partial/path_completed/points 的关系），因此会**落库**。

**⑤ 后果 B：一次可恢复的失败被降级成不可重试。** `StrokeFailureKind::retryable()` 对 `ReleaseUnconfirmed` 返回 **false**（`:171-175`），码为 `mouse_release_failed`（`:162`）；桥层 `classify_stroke_failure` 直接委托它（`computer_use_desktop_bridge.rs:1005-1008`）。因此**输入前失败**（本应 `stale_observation` 可重试、允许重新观察后重规划）会被判成**不可重试、按释放未确认处理**。

**⑥ 为什么平时不暴露。** 正常情况下那次多余的补发释放会**成功**，于是 `release_failure` 不产生，原始 `stale_observation` 分类被保留。
⇒ 结果**依赖时序**：**相同、完整、可信的证据本应确定地产生同一事实**；这里的缺陷是**证据足够却仍可自相矛盾**。**注意口径**：这不等于"同一物理情形永远返回同一事实"——证据缺失时允许明确未知，**但不允许自相矛盾**（裁决明确更正）。

## 4. 这会带来什么真实影响（不要夸大，也不要缩小）

- **不会**造成"把零输入误报成可能已发送"：`proven_not_sent` 已经看事实，投递事实仍正确 ✅（我一度担心这一点，核对后排除）。
- **会**产生一条**自相矛盾的回执**（NotSent + release Unknown）落库；下游读 `input_release` 的消费者会看到一个**不存在的释放义务**。
- **会**把一次本可恢复的输入前失败**降级为不可重试**，并把终止分类推向"需要人工处理"的方向。
- **不会**触发跨 run 互锁的误阻断：互锁判据是 `input_release_status='unknown' AND input_delivery<>'not_sent'`（`computer_use_store.rs`），而投递是 `not_sent` ⇒ 不会误判（这一条我核对过，是个好消息）。

## 5. 修法（建议，属 `input_stroke.rs` 的负责工单）

- **(a) 让释放义务判定消费事实**：`needs_emergency_release` 增加 helper 事实参数；当事实证明"**从未按下**"（`button_down == false` 且 `injected_points == 0`；helper 的起点事实本就区分"已开始但未按下"）时返回 `false` ⇒ 不补发 ⇒ 分类保持 `stale_observation`、释放为 `NotNeeded`。
- **(b) 消除自相矛盾的回执**：`helper_failure_receipt` 里 `input_release` 不应无条件 `Unknown`——当 `proven_not_sent` 成立时应为 `NotNeeded`（覆盖 `ReleaseUnconfirmed`）。**这条即使不做 (a) 也应当做**，因为它与"事实必须自洽"直接冲突。
- **(c) 不建议**只改测试断言（例如接受任一种组合）。那是**掩盖**：同一物理情形允许两种事实，本身就违背"事实层必须确定"的要求。若 (a)(b) 落地后判定 `ReleaseUnconfirmed` 在该场景**合法**（例如确有按下），再改断言才是有依据的。
- **(d) 参照物**：RPR-04d 在**新的**普通输入路径里已经采用更严的规则——"helper 事实证明'一步都没注入'时**不补发**"。也就是说**同一仓库里已经存在正确做法**，笔画路径应对齐它。

## 6. 我为什么没有直接改

三个理由：① 该文件不在我本轮的允许范围（我说过"不越界改别人的文件"）；② 修法 (a) 会改变**真实输入路径**的释放行为（少发一次 UP），这属于安全相关的行为变更，应由该工单的所有者按裁决口径确认；③ 我单方面改断言会掩盖一个真实的自相矛盾。因此我把它**记录并上报**，而不是顺手改掉。

---

# 第二部分：CU 执行器符合性评估

## 结论

**不符合裁决的预期。** 契约、规则引擎、事实 schema、输入所有权与预算机制都已就位并被测试覆盖，但**CU 侧至今没有一条事实进入业务链**，且释放义务判定存在上述缺陷。下面是逐项对齐表与阻塞清单。

## 2.1 裁决要求 vs 当前状态

| 裁决要求 | 当前状态 | 差距 |
| --- | --- | --- |
| 动作事实携带来源（14 项） | **schema 已就位**（`ActionFact.origin` + 读回投影 + 契约导出，§B-34） | **无生产者**：没有代码构造 `ActionOrigin` |
| `request_attempt_id` 来自真实规划请求 | **可取得**（planner 暴露真实复合键，§B-37） | **无消费者**：执行器不读它 |
| 两层校验（结构 + 可信关联） | 结构层在存储层强制；`admit_action_origin` 与 authority trait 已实现 | **无生产版 `ActionOriginAuthority`**（只有内存版） |
| 释放义务按**实际按钮/按键**登记 | 新输入路径（RPR-04d）✅；**笔画路径缺"从未按下即无义务"** | 见第一部分（缺陷） |
| 有界收尾（2s/4s/≤2s） | 策略集中且被复用 ✅ | **`input_stroke.rs` 的 `run_helper` 用无界 `join()`**（§B-32）⇒ "四秒有界"在孙进程持管道时不成立 |
| 输入前缺必需上下文即拒绝（15 项） | ✅ 已实现并有测试（§B-35） | 合法直操被一并拒绝，等 ControlPlane |
| 合法直操走 ControlPlane | 契约已就位（`ControlPlaneOperations` 等） | **接纳入口未建** |
| 普通输入进入受控生命周期（B-9） | 新路径 ✅（RPR-04d，17 测 6 真实子进程） | `main.rs` 的四个发送函数与 `gui-desktop` **仍用旧原语** |
| 排空（B-13） | 客户端方案 + 流包装 + 切换闸门 ✅ | **vision 服务（7860/8000）未纳入在途登记**；托管实例身份已关联 ✅ |
| 证据元数据（B-10） | 契约 + 存储校验 ✅ | **适配器/宿主尚未产出 `evidence_basis`** |
| 未确认释放互锁（13） | 同 turn 内 ✅ | **作用域未扩到资源级**（裁决选 c，待实现） |

## 2.2 阻塞清单（按"卡住什么"排序）

**A. 卡住"CU 侧写入任何事实"**
1. `ActionOrigin` 的构造与 `admit_action_origin` 未接入执行器（(d) 后半段）。
2. **生产版 `ActionOriginAuthority` 不存在**（现在只有 `TrustedOriginContext` 内存版）——没有它，可信关联层无法运行。
3. **`workspace_id` 未 plumb 进 CU 执行器**：`computer_use_runs` 今天不记录工作区，而 StepAction 事实要求容器五维齐全；不补就无法构造合法身份（§B-28）。
4. `ActionFact` 落盘需要"步骤 + 事实同事务"的调用改造（机制已就绪：`record_step_with_facts`，§B-25）。

**B. 事实正确性（已观测到的缺陷）**
5. 第 3 节的释放义务缺陷 ⇒ 可产出**自相矛盾的回执**并把可恢复失败降级为不可重试。

**C. 输入安全的未覆盖面**
6. 笔画路径的无界 `join()`（§B-32）⇒ 有界收尾不成立。
7. 临时光标路径缺 cancelled / quarantined / budget 三条守卫（§B-20，只接了 lease 边界）。
8. `main.rs` 四个发送函数 + `gui-desktop` 的 `input_backend.rs` 仍用旧原语 ⇒ **它们还没进入"已验收的自动输入集合"**。
9. Interception 分支在本机不可验证（驱动未安装；`interception_create_context()` 返回 NULL）。

**D. 能力边界与过渡态**
10. 排空未覆盖 vision 服务（客户端方案适用范围有限，裁决已明确"无法建立足够证据就拒绝自动切换"）。
11. 合法用户直操目前一律被拒（ControlPlane 接纳入口未建）——**这是裁决明示的过渡态**，但必须补齐才能恢复能力。
12. 事实链的其他类（CU 运行终态、用量、迟到事实、提交幂等、恢复身份）同样**无生产者**（聊天轮次终态是唯一已接线的）。

## 2.3 要收口需要做什么（建议顺序）

| 顺序 | 交付 | 关闭条件 |
| --- | --- | --- |
| 1 | 释放义务判定消费事实（第 3 节 (a)+(b)） | 零输入场景**确定**报 `Stale` + `NotNeeded`；不存在 NotSent+Unknown 的回执 |
| 2 | `workspace_id` plumb 进 CU 执行器 + 生产版 `ActionOriginAuthority` | 能构造合法 StepAction 身份并通过两层校验 |
| 3 | 执行器写 `ActionFact`（`ModelPlanned` + 真实 attempt + 同事务） | 真实 CU 运行能在库里查到带来源的动作事实，且来源可核对 |
| 4 | `main.rs`/`gui-desktop` 切换到受控输入原语 | 旧原语不再被自动输入集合内的调用者使用 |
| 5 | 笔画路径的有界 `join()` | 孙进程持管道时收尾仍在窗口内结束（或明确隔离） |
| 6 | ControlPlane 接纳入口 | 合法直操可在真实上下文下执行（不再一律拒绝） |

---

# 需要裁决的点

1. **第 3 节 (a) 是否批准**：让释放义务判定消费"从未按下"的事实（会**减少**一次多余的 UP 补发）。这是安全相关的行为变更，需按裁决口径确认。
2. **第 3 节 (b) 是否批准**：`input_release` 在"已证明未发送"时写 `NotNeeded` 而不是 `Unknown`——我判断这条**无争议**（消除自相矛盾），但仍按流程列出。
3. **顺序 1 与顺序 3 是否可并行**：两者都在 CU 路径上但文件不同（`input_stroke.rs` vs 执行器/存储），若允许并行可缩短收口时间；但按裁决"同一输入资源串行、broker/helper/cleanup 顺序合并"的约束，我倾向**顺序 1 先做**。
4. **`workspace_id` 用什么值**：CU 运行今天不记录工作区。选项：(a) 在 CU 接纳处捕获并记录进 `computer_use_runs`（更严格、要动存储层）；(b) 在工具调度处取当前工作区传入（成本低，语义上接近但不如 (a) 严格）。**我的建议**：(a)，因为事实一旦写入就不可改，而 (b) 在"运行中切换工作区"的边界上会有歧义。
