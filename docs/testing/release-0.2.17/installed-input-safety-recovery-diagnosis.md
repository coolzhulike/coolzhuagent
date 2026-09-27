# 已安装版 Paint 请求的输入安全阻断诊断（2026-09-27）

本记录只读检查已安装 0.2.17 的运行服务、用户会话库和用户级输入安全库。没有调用恢复 challenge、放行或开放接口，没有改数据库、清理旧运行、重试模型或修改安全策略。测试窗口仍由正式安装入口运行。Paint 的真实模型工具调用被门禁拒绝，画线功能验收未通过；门禁在现有事实下拒绝新输入符合其保守规则。

## 当前事实与直接原因

- 正式安装版 Web Console PID `20992` 与 Tauri Shell PID `15892` 均在 Windows Session ID `2`。`GET /api/system/attribution-and-recovery` 报告当前资源范围 `windows-session-2`，会话库 ready，`legacy_unconverged_runs=2`，`pending_convergence_intents=0`，`live_runtime_runs=[]`。
- 当前用户级输入安全库 `C:\Users\zhupu\AppData\Local\CoolzhuAgent\input-safety\input-safety.sqlite3` 对 `windows-session-2` **没有** `input_safety_resource_state` 行；只有本次启动获得、随后释放的一条协调 epoch `1`。`InputSafetyStore::resource_state()` 缺行返回 `InputSafetyResourceState::initial()`，其值明确是 `Unknown`、revision `1`、不接受新输入。因此 Qwen 工具回执中的 `state=unknown revision=1` 可直接对应当前持久化状态，而不是 Paint 或模型的结果。
- 正式启动日志于 `2026-09-27T00:15:50.504955Z` 记录：`candidates=2, converged=0, refused=0, settled=2`，`opening=KeptIsolated`；评估中当前 scope 开启阻断 `0`、待对账恢复 `0`、未获接受的遗留运行 `2`、人工复核 `2`，拒绝理由为“仍有待收敛的遗留 CU 运行”。这解释了为何新会话未建立 `Safe` 状态。
- 两条人工复核操作、事故和两个未关闭阻断都存储在**历史** `windows-session-1`；该 scope 的资源状态为 `isolated`。API 的 `human_review_required_operations()` 与会话库遗留运行数是全局读回，而 `unacknowledged_open_block_ids(scope)` 是按当前 scope 读回。不能把全局 `human_review_required=2` 或历史 scope-1 的两个 block 当成当前 scope-2 的直接阻断。当前 scope-2 的直接拒绝项是两条**没有历史资源范围记录**的旧 CU 运行仍未收敛。

## 两条旧运行的只读核对

| 运行 | 持久化事实 | 恢复结论 |
| --- | --- | --- |
| `cu-session-1781738898772-000000000000000118bdb1027b2478e0-call_ea30b56c5ec44e898fce7353` | `observing`，state version `33`，`workspace_id=NULL`，无终态、无收敛 ID、无步骤、无终态控制事实；按 `(session_id, legacy_turn_id)` 查不到 `runtime_runs` owner 映射；当前记录中未确认释放步骤数 `0` | 历史恢复操作 `r5_incident_established`，已结账为 `human_review_required`；不能把缺失 owner 解释成“没有 owner” |
| `cu-session-1789804322149-000000000000000118d6a9a0fcc1349c-rpO0wuINWlivnmBZeYz48T7iLk5bFSaF` | `planning`，state version `5`；其余缺失项同上 | 同样为 `r5_incident_established` / `human_review_required` |

用户级输入安全库现有 `input_safety_executors=0`、`input_safety_permits=0`、`input_safety_release_decisions=0`。当前按进程名可见的 Coolzhu 进程只有本次安装版的 Web Console 与 Tauri Shell。**没有记录到步骤或旧执行者身份，并不等于已独立证明历史物理输入从未发出或按键已释放**；两条旧运行来自更早版本，历史范围也未记录，不能凭当前数据库空行自签安全结论。

系统只读报告的最近启动时间为 `2026-09-25 03:17:05 +08:00`；两条旧运行最后更新分别为 `2026-06-30 07:28:48.200 +08:00` 与 `2026-09-19 16:18:24.402 +08:00`，均早于此次系统启动。这能排除**在该次开机之前已存在的进程实例**仍在本次系统中运行；但旧记录没有 PID、进程创建时间、job 或按键释放凭据，不能据此证明所有历史输入结果或释放状态。产品结构化日志中按两条完整 call ID 查无记录，也没有补出旧执行者身份。

## 源码判据与恢复边界

- `input_safety_opening.rs` 的 `run_startup_input_safety()` 全局取 `legacy_unconverged_runs()` 作为候选；两条对应恢复操作已在历史 scope 结账，驱动返回 `AlreadySettled`。`assess_input_resource()` 对尚未获人工接受的遗留运行仍作全局计数，故当前 scope-2 拒绝开放。
- 历史恢复驱动明确以 `LegacyResourceScope::Unrecorded` 标记旧运行的**历史**资源范围。恢复操作、事故、阻断位于 scope-1，只说明当时的**当前恢复**范围，并不能证明旧运行仅影响 scope-1。因此不能仅按历史恢复操作 scope 过滤掉当前 scope-2 的两个未知遗留运行；这会把“未记录归属”误作“已证明不相关”。
- 现有 `GET /api/system/attribution-and-recovery` 可只读展示全局遗留运行和恢复台账。原生恢复流程是 `POST /api/system/recovery-challenge` → **本次安装启动登记的 Tauri 窗口**检查并确认 → 一次性证明 → `POST /api/system/release-isolation` → 持有恢复协调权、原子复核阻断及原生执行者/许可状态 → 再独立评估开放。普通浏览器或调用方自报署名不能替代原生确认。当前没有足以自动认定两条历史运行安全的独立 process/job/释放事实，故本轮未发起该流程。

## 可执行的后续处置

1. 对两条完整 call ID 分别做人工复核：核实原会话/运行来源及真实 owner 关系、当时的执行者进程或 job 身份、是否曾发出物理输入、是否存在未确认按键释放，并记录证据引用。能从独立事实证明安全时，走现有有协调资格的收敛与开放路径；不能证明时，明确列出剩余未知风险，由有权限的操作者决定是否经**原生确认**逐条接受。不能由测试代理代签或直接修改 DB。
2. 若进行正式人工接受，当前 scope-2 的恢复页面应展示两条 `unacknowledged_run_ids`、当前 `unacknowledged_block_ids=[]`，在实际核对后提交准确集合及理由和证据。服务端仍须复核快照、实际执行者退出、在途许可状态和独立开放评估；结果只以 `opened` / `kept_isolated` 回执与新资源状态为准。历史 scope-1 的两个开启阻断仍保留独立处置需求，不应由 scope-2 的决定静默清除。
3. 产品接线改进应在只读诊断中**分别呈现**当前 scope 的资源状态/阻断/执行者/许可、全局历史范围未记录的遗留运行、历史 scope 的人工复核数；不要把这些混成一个“当前作用域待处理”数字。若将来能持久化可信的历史 Windows 会话范围，评估可以仅排除**确证属于其他范围**的旧运行；`Unrecorded` 仍保持跨范围保守阻断。涉及主服务路由的改动应与当前其他开发任务协调；本轮未改产品代码。

只读原始依据保存于被 Git 忽略的 `tmp/2026-09-27-policy-diagnosis/`：`live-attribution-sanitized.json`、`legacy-runs-sanitized.json`、`input-safety-sanitized.json`、`installed-startup-input-safety.txt`。截图与模型回执见 [安装版导航与 Paint 实测](installed-navigation-and-paint.md)。

## 用户可在已安装原生窗口审阅的正式恢复入口

本轮只点击底栏“放行隔离…”查看首层提示，然后在理由仍为空时点“取消”。首层提示原文为“恢复理由（写清已核查的依据；下一步由桌面原生窗口确认）”。取消后再次只读查询：当前 `windows-session-2`、未收敛遗留 `2`、待对账恢复 `0`、当前范围未获放行阻断 `0`、最近放行记录仍为空；没有提交 challenge 或放行 POST。

如用户自己完成两条旧运行的证据复核，并决定处理剩余未知风险，正式界面步骤是：在**本次完整安装启动的 Coolzhu 原生控制台**底栏点“放行隔离…”；写下真实恢复理由及已核查依据；网页随后创建一次 challenge，Tauri 弹出标题“Coolzhu Agent · 恢复桌面操作”的 Windows 原生警告框，逐项显示当前 Windows SID、范围、理由、具体阻断、两条要接受的遗留运行、未结许可和执行进程事实。原生框默认选择“否”；只有用户核对信息并亲自点“是”，才会由桌面进程提交一次性证明，再交后台机器复核。它**不是**密码或 Windows Hello 身份重认证；“是”表示接受仍未知的历史执行结果，而不把它伪造为成功。若不接受，就点“否”或取消并保持隔离。提交后仍以回执 `opened` 或 `kept_isolated` 和重新查询的资源状态判断能否重做 Paint 验收。

本次需要用户核对的两条具体记录为：

| 遗留运行 | 已知事实与仍未知之处 |
| --- | --- |
| `cu-session-1781738898772-000000000000000118bdb1027b2478e0-call_ea30b56c5ec44e898fce7353` | `observing` v33；最后更新于 2026-06-30 07:28:48 +08。无终态、步骤、可信旧 owner、进程身份或释放凭据；实际历史输入结果未知。 |
| `cu-session-1789804322149-000000000000000118d6a9a0fcc1349c-rpO0wuINWlivnmBZeYz48T7iLk5bFSaF` | `planning` v5；最后更新于 2026-09-19 16:18:24 +08。缺失项同上；实际历史输入结果未知。 |

以上流程由已安装版 UI 首层提示及 `app.js` 的 `releaseInputIsolation()`、Tauri `recovery_confirmation.rs` 的原生确认实现核对；本轮没有进入第二层原生警告框，因为创建 challenge 已是恢复操作，且不应由验收代理替用户接受风险。
