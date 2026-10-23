# Harness 方案审查反馈复核

**结论：部分采纳反馈，但“3 个都必须先向用户确认的开工阻断”不成立。** 可复现基线与实例隔离应作为 HA-00 的实际交付条件；模型额度影响真实实验及最终验收，不阻止已授权的确定性工作；授权只能依据人类原始指令核实，不能依据审查文件转述升级。

本页是对 [REVIEW-2026-10-17.md](REVIEW-2026-10-17.md) 的审核意见及修订建议，不改变现行 [PLAN.md](PLAN.md)、[ACCEPTANCE.md](ACCEPTANCE.md)、[state.json](state.json) 的合同或授权状态。本次直接指令是“审核以下反馈”；未启动 HA-00 实施，未运行真实模型实验或改业务代码。

## 1. 三项阻断的裁决

| 反馈 | 裁决 | 处理建议 |
|---|---|---|
| A1 基线过期，须确认当前改动为起点 | **内容变化属实；阻断 HA-01 业务修改，不阻断 HA-00 核对和冻结。** 规划指纹本来不是运行基线，HA00-03 已要求另行冻结可复现内容 | 保留原规划指纹，另存带时间/ID 的实施基线及恢复说明；保留所有既有改动，只收录构建/实验所需内容。若已获实施授权，非破坏性的本地冻结无需再次询问 |
| A2 没有额度就无法 60/60 | **最终验收条件成立；阶段性交付定义不足应补充，但不是逻辑矛盾或全局开工阻断。** PLAN 已明确可先冻结基线、稍后补跑模型实验 | 保留完整 60 条合同及严格 `--complete`。允许交付已验收的确定性改动和未完实验清单；模型部分如实 blocked，不算成功，不缩减合同凑完成率 |
| A3 授权上下文过期 | **在确有直接实施指令的执行会话中成立，是元数据同步工作。** 本会话可见指令尚不足以确认审查中转述的“没有问题就开始实施” | 有直接授权就记录原始消息引用、时间和范围并继续；没有就不能将转述写成已授权。state 是进度记录，不是高于人类指令的授权机关 |

“重新基线前任何条目都不能 in_progress”应删除：冻结基线本身就是 HA-00 的工作。真正开始 HA-00 时应及时记录 owner、in_progress 和下一动作；禁止提前标 accepted，而不是禁止如实记录进行中。否则突然中断后反而不知道谁在做基线。

## 2. 事实纠正

| 事项 | 本次核验 | 对结论的影响 |
|---|---|---|
| main.rs | 规划指纹 3,652,853 B / `90ab33494c3923ea7bdf52b586ea97314a2805f888fdc869494a4d5872b27377`；当前 3,652,979 B / `3d1aa95e28d0eb839279e4d2381ddd20e6a1f160ea6fb9193b08e7ca7e930a4d` | 确有 126 字节长度差及内容变化。不能由长度、哈希推断具体语义或全部 Pro 发现已失效，需按相关函数复核 |
| 文件清单 | baseline 的 `source_files` 共 **27** 项，含 AGENT.md；扣除此文件才是 26 项 | 应写清统计范围，不把它描述为完整源码快照 |
| Git 状态 | baseline 的已跟踪统计为 ` M:119`、`D :50`、` D:3`、`R :37`，共 **209** 项；本次同口径 `-uno` 仍为相同统计 | 反馈漏了 3 项未暂存删除，并把已跟踪项与含未跟踪项的 307 项混比。相同计数不证明内容相同，哈希变化仍需处理 |
| 含未跟踪项的统计 | 本次普通口径 309 项，其中 100 个未跟踪文件/目录条目；展开所有未跟踪文件为 668 项 | 307 是该次审查时的观测，不应成为固定开工条件。快照必须明确纳入必要的未跟踪源码，不纳入所有杂项 |
| 运行实例 | 8765 本次监听 PID **5268**，路径仍为 `C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe`；开始时间 2026-10-17 21:07:34 -07:00 | PID 25112 已是历史信息。启动任何实验前重新核对，不据旧 PID 执行进程操作 |
| 账号验证记录 | [九项工作记录](../../work-logs/2026-10-17-nine-item-implementation.md) L80 起补充 Devin 实际对话通过；L93 起补充用户确认 ChatGPT 登录/目录/真实回复，且记录 0.2.141–0.2.142 | “OAuth、Devin 都仍未验证”引用了过期小节。已有成功记录不证明当前剩余额度、模型可比性或本轮费用授权 |

`check_plan.py` 本次结构检查仍为 `structure_valid: true`，60 条仍 pending，main.rs 出现在 drift 提示；`--self-test` 11/11 通过。它只证明台账约束检查有效，不证明 60 项业务行为，也没有验证授权文本的真实性。

## 3. 实施风险处置

### 基线快照

采纳必须保存可恢复内容的意见。以当前工作区最终内容为输入，包含构建所需源码、测试、清单/锁文件、构建脚本、必要资源和非敏感配置，以及所依赖的新增未跟踪文件；忠实保留删除/重命名结果。不能用 HEAD checkout 代替当前工作区，也不能将所有未跟踪项盲目复制。

排除实际 `.coolzhu` 状态、用户 DB/附件、账号和密钥、日志、tmp、target、安装包及无关产物。需要这些类型材料的测试从脱敏 fixture 或显式测试配置重新生成。优先一份可恢复源码快照加 manifest；已有可靠补丁流程可复用，不要求同时建设两套备份体系。

记录包含/排除规则、来源内容、工具链/依赖条件、恢复步骤。在独立目录恢复并核对必要构建输入，检查快照期间没有相关并发修改；发现变化就重取受影响文件或重新冻结。原规划 baseline 保留为历史证据，新运行基线独立命名，不覆盖原哈希假装一直一致。

### 测试入口并非全都缺失

| 验收领域 | 已找到的入口 | 尚不能据此宣称的保证 |
|---|---|---|
| HA-06 hook 超时/取消、效果保留 | `dispatch_hook_budget_tests.rs::interrupted_hook_case`；`dispatch_hook_budget_post_deadline_preserves_target_effect`；`dispatch_hook_budget_post_cancel_yields_and_preserves_effect` | 不能自动证明断流后重送不产生第二次效果；需补回执丢失/重送路径 |
| 身份拒绝与收尾 | `tool_dispatch_settlement.rs::refused_registration_never_reaches_file_write_or_changes_original_owner`；`tool_invocation_identity.rs::provider_id_is_correlated_without_colliding_across_actual_requests` | 不能推导所有外部服务恰好一次 |
| hook 实链路/隔离 fixture | `dispatch_hook_config_tests.rs::startup_configuration_reaches_real_dispatch`；`main.rs::hook_dispatch_fixture` / `dispatch_hook_override_guard` | 部分预算测试启用测试全开放权限，不能代替 HA-07 的真实审批/撤销验证 |
| HA-08 并行派发 | `main.rs::dispatch_model_tool_calls_parallel_preserves_order` | 当前断言主要检查返回顺序与结果；没有证明执行区间重叠，也没有覆盖全部资源冲突 |

源码位于 [hook 预算测试](../../../modules/gui-web/packages/web-console/src/dispatch_hook_budget_tests.rs)、[hook 启动配置测试](../../../modules/gui-web/packages/web-console/src/dispatch_hook_config_tests.rs)、[收尾测试](../../../modules/gui-web/packages/web-console/src/tool_dispatch_settlement.rs)、[身份测试](../../../modules/gui-web/packages/web-console/src/tool_invocation_identity.rs)、[Web 主入口](../../../modules/gui-web/packages/web-console/src/main.rs)。本次只读取，未执行这些 Rust 测试。

HA-00 应产出“条目 → 现有测试 → 缺失观察点 → 最小新增方式”映射。优先复用已有隔离 fixture、通知和事件记录，再按需要加局部测试屏障，不先立项通用故障注入平台。审批/队列/领取资格的四阶段撤销竞态仍待盘点；不能因为找到部分测试就宣布已有完整覆盖。

### 隔离应提升为启动实验前置条件

这一项比原反馈的“不一定阻断”更重要。[main.rs](../../../modules/gui-web/packages/web-console/src/main.rs) L1323–1329 在启动时调用 `stop_owned_instance`；[instance_restart.rs](../../../packages/app-launcher/src/instance_restart.rs) L24 起会检查监听者及重启凭据，并向符合条件的旧实例发送退出请求。不能以“只是启动第二个进程”推断不会影响日常实例。

源码确认的隔离入口包括：独立配置中的 `[web].bind_addr`、`COOLZHU_RUNTIME_DIR`、`COOLZHU_WEB_SESSION_DB`、`COOLZHU_WEB_SESSION_STORE`、`COOLZHU_USER_STATE_ROOT`、`COOLZHU_LOG_DIR`。还应核对 input-safety/native-recovery、插件/账号状态、附件及其他子进程实际解析的目录；不要假定只换端口/DB 就完成隔离。对本轮不用的 CU/后台调度能力优先关闭，并验证没有用户状态写入。

需要在启动前明确所有路径与空闲端口，启动后只读核对真实配置/进程身份，确认测试持久化路径都落在隔离目录，保留日常实例存活、日常库无测试写入的证据。不得替换安装版或调用日常服务的重启入口。若现有路径无法隔离，阻塞该进程级实验并继续能独立完成的源码盘点，不扩大为所有工作停摆。

### 周期与推进

13 个工作包确实不宜许诺一个会话完成，但也无法仅按条目数断言需要多少天/轮次。HA-00 先给出复用/补测/新增的范围与成本依据，随后按依赖推进。若原始授权已经允许连续实施，HA-00 后做一次证据复核即可，不自动增加用户重新审批；范围出现实质扩张、无法隔离或付费配置未明确时才处理相应缺口。

## 4. 无额度时的交付定义建议

保留原 60 条合同，增加阶段性报告口径即可，不让 `--complete` 带 blocked 通过。

| 交付状态 | 必须成立 | 对外可以表述 | 不可以表述 |
|---|---|---|---|
| 方案就绪 | 方案/台账一致，可接替 | 已完成实施设计 | 功能已验收 |
| 确定性阶段交付 | HA-00 至 HA-10 的 **52 条**逐项 accepted/passed；必要构建和定向测试有效；独立可做的集成/接替证据归档 | 确定性部分已验收；模型实验及整体收口待完成 | 60/60；已证实提速；完整交付 |
| 完整验收 | 13 个工作包、60 条全部有效通过；无未处理执行/效果；`--complete` 成功 | 首轮范围完整验收 | T08 自动获得实施或发布授权 |

额度不足时，只有真实受影响的实验条目 blocked；HA-11 工作包不能 accepted，HA12-04 整体收口保持未通过。交接文档、证据整理和恢复演练仍可做，不应因为 HA-11 blocked 就停止保存交接信息。HA-12 的包级 accepted 依赖 HA-11，不必把它解释为平时不能演练接替；修订文本应明确这一区分。

阶段性交付应包含已验收改动、逐项证据、失败/阻塞原因、未运行实验清单、固定模型配置需求、下一步与回退方式。`--complete` 此时返回非零是正确行为，不需要放宽检查器，也不把 blocked 改成 not_required。

15 组配对约为 **30 次场景运行**，不是最多 30 次模型 API 请求。一个场景可能含多个工具循环/模型请求，计费准备需明确 provider、实际模型身份、effort、工具能力、输入材料和总费用/token/时限上限。现有 ChatGPT 订阅入口仍有工具能力限制，能聊天不表示能覆盖工具 Harness 对照。只在具备可比路径和明确授权时开始真实调用，不将账号历史成功记录当成此次预算批准。

## 5. 文案修订建议

| 位置 | 建议表述/处置 |
|---|---|
| ACCEPTANCE 开头 | “全部验收条目必做。运行时 `not_required` 是被测试结果之一；证明其正确返回可以让 HA09-02 通过，但不得用它豁免任何验收条目。”现有含义不矛盾，补一句即可 |
| START_HERE 开头 | “本计划对应的 Harness 对齐实施尚未开始；九项工作和后续安装版修复属于既有前置改动。”后续读实际状态，不把固定首页描述当实时事实 |
| baseline.json | 明确 `purpose=planning_observation` / 历史统计；实施基线另存。不得原地抹除历史快照差异 |
| state 授权 | “进度以 state 为准，授权以真实用户指令为准；state 只引用授权来源及范围。”不以文档文本生成新授权 |
| state 检查点 | 记录发现 main.rs 漂移及下一步 HA-00；开始实际核对时可如实 in_progress，冻结完成前不 accepted |
| PLAN/EVIDENCE 交付定义 | 增加上表的阶段性交付口径，明确 blocked 不计通过，`--complete` 始终要求全部 60 条 |

建议采纳基线恢复、隔离和阶段性交付补充，纠正统计口径与过期事实，取消重复审批建议。实际实施获授权后，从 HA-00 进行当前内容冻结、调用链核对、测试缺口映射和隔离验证；模型额度准备不应卡住这些工作。
