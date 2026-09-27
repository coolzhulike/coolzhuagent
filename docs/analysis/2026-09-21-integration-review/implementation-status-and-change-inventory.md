# 实施计划完成情况与改动清单（历史正文与 2026-09-27 接手入口）

> **当前入口（2026-09-27，022 安装及本轮原生增量完成）**：最新原稿的居中 COOLZHU、左右下拉与竹林已进入 022；完整 release 六门、856 载荷及源码首尾快照通过，正常升级退出 0，关键安装文件 10/10 匹配。正式入口连接 8765 安装 Web；正常/903×551、三下拉、更多及活动网页加载/关闭/缩小隐藏/放大恢复已有原生操作与十张受控图。运行后 66 持久文件无增删、41 附件及配置未变，正常活动字段差异单列。模型类型与图片失败修复通过 3 项定向测试及 11 个真实 Web 场景，S0 在 debug 与同次 release Web 通过。[Draft PR #68](https://github.com/coolzhulike/coolzhuagent/pull/68) 已创建（首批 `f1d80e3`）；首轮远端中文输出失败由测试驱动 `d38a2a5` 修复，cp1252 模拟及两条远端 S0 已通过，最终状态见 PR Checks。以 [022 验收矩阵](../../testing/release-0.2.22/functional-verification-and-test-design.md)与 [四项状态](../2026-09-27-four-task-status-and-navigation-addendum.md) 为准。021 LSP/视频保持原身份；预览临时地址干扰及快照边车误计数已记录并纠正。Pro 当前不可选、Paint 本人复核及其他未覆盖范围未关闭，不宣布四项全部完成。下方旧版“当前/最新”均为历史快照。

> **最新验收入口（2026-09-27，构建后补记）**：0.2.18 已正常升级，10/10 关键安装产物身份一致；停机升级前后数据语义清单一致。CLI/子 Agent、Goal 工程固定、视觉转述用量、MCP stdio 四项安装版隔离实操均首轮通过。新增原生窗口实操正在进行，四项任务及 32 个工作包尚未总体验收。以 [0.2.18 总索引](../../testing/release-0.2.18/functional-verification-and-test-design.md) 和 [四项最新状态](../2026-09-27-four-task-status-and-navigation-addendum.md) 为准。下方带“本轮末”“当前口径”的旧段落也是历史时点，不能覆盖本入口。

> **后续接手入口（2026-09-27）**：PR #67 已合并，0.2.17 标准原生启动及五入口/三下拉已有截图；Qwen 安装版图片识别已实际通过。Paint 正式工具调用到达输入门禁，但因两条缺历史归属的输入任务而被拒绝，零输入，功能未通过。新增更多/更新、子 Agent 宿主接线、插件事务和 LSP/PTY 修复在 `codex/navigation-update-followup` 继续。当前状态以 [四项任务增补](../2026-09-27-four-task-status-and-navigation-addendum.md)、[安装版实操](../../testing/release-0.2.17/installed-navigation-and-paint.md) 及 [恢复诊断](../../testing/release-0.2.17/installed-input-safety-recovery-diagnosis.md) 为准。下文保留历史阶段记录，不能把旧“无远端/PR 未提交/无原生图”当作当前事实。

- **状态口径**（沿用第七轮裁决 §八）：**已裁决 / 已实现 / 生产已接线 / 目标环境·安装已验收**。四者不等价，"已实现"不等于"已接线"或"已验收"。
- **范围**：第七、八两轮裁决的实施；三次现场问题修复（图标无法启动、默认工作区口径确认、**启动路径自锁**）；一次测试基础设施的并发竞态修复；**PR-01（P0-1）恢复处置闭环**（决策：`RecoveryDisposition` 状态机 + 人工放行通道）。
- **验证基线**（本轮末实跑）：web-console **1135/0**、computer-use-core 123/0、windows-process-guard 50/0（1 ignored，另见 §7）、tool-registry 54/0、core-runtime **312/0**、module_linkage_smoke 4/0、app-launcher 64/0 + 5/0。
- **本文件所有"锚点"都可用 §6 的命令核验**，不是凭记忆写的。

> **2026-09-27 当前口径**：本文件下文保留 2026-09-26 各阶段记录，尤其 §9.1–9.5 的“Step 4 尚未完成”和 1215/132 项门禁数字已是历史快照，不能再当当前状态。最新逐项矩阵、实操证据、0.2.17 打包与安装结果见 [0.2.17 功能核查与针对性测试设计](../../testing/release-0.2.17/functional-verification-and-test-design.md)；代码在 [Draft PR #67](https://github.com/coolzhulike/coolzhuagent/pull/67)。当前工作区回归 `2367 passed / 0 failed / 4 ignored`，CU core `139/0`；两相 helper T1/T11/T6-A/B/C 已通过。0.2.17 MSI 已生成并升级安装，10/10 已安装二进制哈希一致；原生安装版窗口与 Paint 实操截图未获得，S0–S6 不得标作全通过。

## 1. 实施计划完成情况

| 单元（裁决编号） | 状态 | 关键落点 | 证据 | 尚未完成的部分 |
| --- | --- | --- | --- | --- |
| Phase 1 · RD4-02A 共享输入安全库 | **已实现**（契约＋宿主存储） | `core-runtime/src/input_safety.rs`、`web-console/src/input_safety_store.rs` | 契约用例 5 条；存储用例 10 条（含身份/空库/阻断核查/五表） | launcher 注入已加；**由 launcher 启动的控制台进程**内的联动未直接抓日志（见 §5） |
| Phase 1 · 组合用例 1／2／3 | **已实现**（存储层；1/3 走协调器） | `input_safety_store.rs`（epoch／对账）、`legacy_recovery_driver.rs` | `combined_1/2/3_*` 三条用例 | 端到端"真实双进程 + 真实崩溃"变体 |
| Phase 2 · RD4-02B 协调器与资格 | **部分**（协调器＋可信 guard＋跨进程 Busy 已实现） | `InputSafetyCoordinator`、`RecoveryControlGuard`（同文件） | 用例 2：第二协调器 `input_safety_coordinator_busy`；类型级堵自证 | **R3**（撤销未激活许可）与 **R4**（跨进程执行者身份核查）未实现 |
| RD4-03 · 遗留收敛（语义＋驱动） | **已实现**（R1–R9 端到端 + 中断重放） | `legacy_recovery.rs`、`legacy_recovery_driver.rs` | 驱动用例 **4** 条：真收敛 + 保持隔离 + 两条拒绝路径 + **重放重绑**；真实库实跑（§B-77） | 未跑含真实全量编译的出包后收敛演练 |
| 入口接线（共享库成为"真实消费者"） | **生产已接线** | `computer_use_executor.rs` 的资源门 + 四维关系核对 | `input_admission_goes_through_the_shared_safety_store`（四种情形） | 其他正式输入入口（非 CU 通道）尚未逐一核对 |
| 开放路径 + 启动触发点 | **生产已接线 + 现场已观察** | `input_safety_opening.rs` + `main.rs` 启动序列 | 用例 **6** 条；**本机真实库实跑**：`candidates=2, converged=0, refused=2, opening=KeptIsolated{open_blocks:2, pending:1, legacy_unconverged_runs:2}`（不再是 `SkippedRootNotInjected`，也不再失败） | 现场保持隔离的原因见 §7（规格未明确项） |
| PKG-PR-01 包根强绑定 | **已实现** | `scripts/build-msi.ps1`（`PKG-ROOT-BINDING-*`） | 三组验收：篡改⇒MISMATCH、无清单⇒MISSING（不回退 latest）、正常⇒绑定通过后按包根收据拒绝 | — |
| PKG-PR-02 外部输入登记 | **已实现** | `config/package-manifest.json`、`scripts/package-all.ps1`、`scripts/lib/build-identity.ps1` | checksum 与 `Cargo.lock` 逐字比对；收据新增三字段 | 其它第三方二进制若出现，需逐项登记 |
| PKG-PR-03 重建与安装 | **目标环境·安装已验收（含卸载／修复／重装）** | `dist/CoolzhuAgent-0.2.16.msi` | 六项一致性条件全 PASS；`msiexec` 安装／修复／卸载 exit=0；重装后 CLI 报 `0.2.16`；用户数据与工作区数据保留 | **签名**（包为 unsigned） |
| 现场修复①：图标无法启动 | **目标环境·安装已验收** | `packages/app-launcher/src/launch_paths.rs`（`select_holder_by_recency`） | 安装后 `--print-resolved-paths` 采用 `C:\Users\zhupu\coolzhuagent` 并持久化；控制台与桌面壳均运行 | 拒绝信息对双击用户**不可见**这一 UX 缺陷未修（待定机制） |
| 现场修复②：默认工作区按设备账号解析 | **已核实（非缺陷）** | 随包 `config/package-launcher.json` 的 `"%USERPROFILE%\coolzhuagent"` | 模板形式；改动 diff 内**无**硬编码用户名 | — |
| 现场修复③：启动路径**自锁**（本轮） | **已实现 + 现场验证** | `legacy_recovery_driver.rs`（重绑／幂等推进）、`input_safety_opening.rs`（待对账口径） | §B-77／§B-78；2 条新回归用例；真实库实跑不再失败 | — |
| **PR-01（P0-1）恢复处置闭环** | **已实现 + 生产已接线 + 现场验证** | 契约 `input_safety.rs`（`RecoveryDisposition`／`ReleaseIsolationDecision`，schema v2）；存储 `input_safety_store.rs`（就地迁移／结账／放行）；驱动／评估／入口／前端 | §B-82；**Recovery-P0-T1** 落地；真实库两遍验证（`pending 1 → 0`、`settled=2`、`human_review_required=2`）；HTTP 守门 400/409 实测 | 放行资格与复核口径（§7） |

## 2. 前端修改点

前端＝`modules/gui-web/packages/web-console/{index.html, src/app.js}`。**`styles.css` 本轮未改**（沿用既有样式）。

| # | 文件 | 锚点 | 改动 | 目的 | 验证 |
| --- | --- | --- | --- | --- | --- |
| F1 | `index.html` | `data-role="system-attribution"` | 全局系统状态栏新增"归属/恢复"一格 | 让"历史归属未记录／待收敛／非终态运行行"在界面上可见 | 该 `data-role` 在 index.html 出现 1 处 |
| F2 | `src/app.js` | `refreshAttributionAndRecovery()` | 新增拉取 `/api/system/attribution-and-recovery` 的函数，并在 `refreshSystemInfo()` 中调用；错误分支把该格置 `—` | 把后端只读呈现接进既有系统信息刷新节奏 | app.js 中 3 个函数名合计出现 6 处 |
| F3 | `src/app.js` | `describeAttributionAndRecovery()` | 三态文案：`归属存储未初始化`／`归属存储不可读`／`历史归属未记录待收敛 N · 收敛待对账 M · 非终态运行行 K` | **不谎报**：未初始化/不可读与"没有待收敛"是不同事实 | 同上 |
| F4 | `src/app.js` | `attributionDetailText()` | 提示语说明"**非终态运行行包含正在执行的轮次**；只有执行已结束但终态提交失败时才残留，且**不会被报成已完成**" | 防止后人把该栏简化成误导性文案 | 守卫用例断言必须含这句 |
| F5 | （守卫用例） | `attribution_surface_frontend_is_wired_with_honest_wording` | 断言 app.js 含端点与诚实措辞、index.html 含该 `data-role` | 前端接线与文案的回归守门 | 该用例通过 |
| **F6** | `index.html`＋`src/app.js` | `data-role="release-isolation"`、`describeInputSafetyRecovery`、`releaseInputIsolation` | 状态栏"放行隔离…"按钮（**只在真有挡路项时出现**）＋三态文案（待对账／待人工复核／未获放行阻断）＋放行确认（写明不删事故、不重置库、不等于已开放） | PR-01：把"在等谁"如实摊开，并给出受控的人工出口 | `release_isolation_surface_is_wired_without_lying`（断言端点/按钮/三态文案/署名与理由） |
| **F7** | `src/app.js` | `syncReleaseIsolationButton`、`inputSafetyNeedsRelease` | 只有"未获放行阻断／未获接受遗留运行"才提示人工介入（纯 `pending` 不算） | 避免把"还在办"误报成"要人管" | 同上 |

> 本轮**没有**新的前端改动点（后端修复不影响前端契约；`/api/system/*` 两个端点的响应结构未变）。

## 3. 后端修改点

| # | 模块／crate | 文件 | 改动 | 目的 | 验证 |
| --- | --- | --- | --- | --- | --- |
| B1 | core-runtime | `src/input_safety.rs`（新增）、`src/lib.rs`（导出） | 共享输入安全库的**领域契约**：`INPUT_SAFETY_SCHEMA_VERSION=1`、StoreId/ResourceScope/ResourceState/Incident/RecoveryStage/`VerifiedBlockingRef`（无 `Deserialize`、无公开构造） | 格式规则只剩源头一份；阻断引用不可自证 | 契约用例 5 条；`lib.rs` 命中 1 处 |
| B2 | web-console | `src/main.rs` | `CanonicalWorkspaceId` + `canonical_workspace_identity`（源头唯一解析）＋ `validate_frozen_parent_relations`（四维可信关系核对） | A-2 来源分层：CU 在类型上拿不到未解析值 | `main.rs` 三符号合计命中 **28** 处；`FrozenParentContext` **17** 处 |
| B3 | web-console | `src/main.rs` | **迁移 v23** 登记＋`SESSION_SCHEMA_VERSION=23`＋单步守卫修正（每步只比较本步号）＋4 条边界行为测试 | 遗留收敛列/表落地；杜绝"版本回退"隐患 | 迁移用例 6 条 |
| B4 | web-console | `src/main.rs` | 新增 `GET /api/system/runtime-identity`（复用唯一来源；只读、不建表、不谎报 0） | 机器可读的"后端实际身份" | 用例 3 条（含复用结构守卫） |
| B5 | web-console | `src/main.rs` | 新增 `GET /api/system/attribution-and-recovery`（只读）＋启动序列接入 `run_startup_input_safety` | 归属/恢复呈现；启动先驱动遗留收敛再评估开放 | `run_startup_input_safety` 命中 1 处；安装后日志可见 |
| B6 | web-console | `src/input_safety_store.rs`（新增） | 宿主侧 SQLite 适配：**七表**（identity/resource_state/incidents/resource_blocks/recovery_operations/ownership_epochs/events）、身份与侧车标记、**不得静默建空库**、`verify_blocking_ref` 六项核查、按 epoch 授权、启动对账、**按资格重绑** | §1 裁决的全部实体与"未注入即拒绝" | 存储用例 10 条 + 组合用例 3 条 |
| B7 | web-console | `src/legacy_recovery.rs`（新增） | owner **真实关系**解析（未知≠没有 owner）＋资源**不确定性**评估（未确认释放优先）＋事实日志提交候选查询 | 组合用例 4／5 | 用例 3 条 |
| B8 | web-console | `src/legacy_recovery_driver.rs`（新增） | **R1–R9** 驱动：协调器（可复用）→隔离→登记→执行者核查→真实事故/阻断/已验证引用→owner→同事务终态→阶段提交→按资格开放 | 生产遗留收敛入口 | 用例 4 条（含真收敛与重放） |
| B9 | web-console | `src/input_safety_opening.rs`（新增） | 启动**独立评估**（无未决阻断/无待对账/无待收敛）→ 协调资格（**全路径复用同一协调器**）→ 按资格 reopen → 尽力释放 | 解决"接线后生产 CU 全阻" | 用例 6 条 |
| B10 | web-console | `src/computer_use_executor.rs` | 接入共享库资源门（`require_resource_accepts_new_input` + `physical_input_resource_scope`）＋保留四维关系核对 | 正式输入入口成为**真实消费者** | 该文件命中 5 处；消费者门用例 4 情形 |
| B11 | windows-process-guard | `src/pipe.rs` | `Saturated` 改为 `CapacityFault`（保留所有权/锁接纳/记录真实数量/不自动恢复）＋五指标拆分 | 不再"丢登记+关句柄+分离线程" | 命中 37 处；故障注入用例；门禁 50/0 |
| B12 | computer-use-core | `src/input.rs`（＋笔画路径） | 容量**预留前移到 `spawn` 之前**＋为安全收尾预留＋命令行按 **UTF-16 单元含终止符**计量 | 消除可预期拒绝仍先建进程；计量纠正 | 命中 6 处；门禁 123/0 |
| B13 | app-launcher | `src/launch_paths.rs` | **多候选按来源优先级采用**（已保存选择 → 最近记录 → 导入旧选择 → 默认/历史遗留）；并列或全不可判定才要求显式选择 | 现场口径：有已保存配置就用"最近打开"的 workspace | `select_holder_by_recency` 命中 7 处；app-launcher 64/0 |
| B14 | app-launcher | `src/lib.rs` | 新增并注入 `COOLZHU_INPUT_SAFETY_STATE_ROOT`（取 `ResolvedLaunchPaths.input_safety_state_root`） | 生产路径必须注入；缺失即 fail-closed | 命中 3 处；用例断言注入值与契约字段一致 |
| B15 | cli | `modules/cli/packages/command-line/Cargo.toml` | 版本 `0.2.5 → 0.2.16` | 高于机器已装版本，走**升级**而非降级 | 文件第 3 行；MSI 版本校验与暂存 CLI 一致 |
| **B16** | web-console | `src/legacy_recovery_driver.rs` | 复用协调器后，对**已存在且未提交**的旧操作按当前资格 `rebind_recovery_operation_authorized` | 修"持有 epoch 1、当前 5"⇒启动路径每次必失败的自锁（§B-77） | 新用例 `replay_rebinds_an_operation_left_by_a_previous_run`；真实库实跑 |
| **B17** | web-console | `src/legacy_recovery_driver.rs` | `advance` 改**幂等**：`stage.order() >= next.order()` 即跳过 | 中断重放不再被"不得跳步或回退"守门判成回退（§B-77 第 2 因） | 同上（重放用例覆盖 r1/r5 两种遗留 stage） |
| **B18** | web-console | `src/input_safety_opening.rs` | 启动对账只把 `NeedsReacquire` 计入 `pending_recovery_operations`（`StillAuthorized` 是活着协调者的在办事项） | 修"开放分支永不可达"（§B-78）；现场 `pending 3 → 1` | 新用例 `operations_held_by_a_live_coordinator_are_not_pending_reconciliation` |
| **B19** | （测试基础设施） | `input_safety_opening.rs`、`computer_use_executor.rs` | opening 两个 env 敏感用例加全局串行守卫；CU 的 `open_input_resource_for_test` 改返回 **Drop 守卫**（恢复原 env） | 消除进程级 env 竞态与泄漏（§B-79） | **连跑 3 轮全量 1115/0** |
| **B20** | core-runtime | `src/input_safety.rs`、`src/lib.rs` | `RecoveryDisposition`（五值，含 `is_terminal`/`needs_human`/`parse`）、`ReleaseIsolationDecision`、操作结构体加 `disposition`；schema **v1 → v2** | 过程（stage）与结论（disposition）分离；未知值一律按 `Pending`（不谎报已结账） | 契约用例 6 条；core-runtime 312/0 |
| **B21** | web-console | `src/input_safety_store.rs` | **v1 → v2 就地迁移**（`ALTER TABLE` 加列 + 新建 `input_safety_release_decisions`）；`settle_recovery_operation_authorized`；`release_isolation_authorized`；`unacknowledged_open_block_ids`／`acknowledged_run_ids`／`unsettled_*`／`human_review_required_operations`；`put` 不得复活已结账 | 给出"永久隔离"的出口：结账是终态且一次性；放行必须署名/理由且逐条留痕 | 存储用例 3 条（含就地升级与"不删事故"断言） |
| **B22** | web-console | `src/legacy_recovery_driver.rs` | 四条退出路径各自结账；已结账不重放（`AlreadySettled`）；**结账放在阶段阶梯之后** | owner 未知 ⇒ `HumanReviewRequired`（资源仍隔离），不再永久挂账 | **Recovery-P0-T1** + 不重放用例 |
| **B23** | web-console | `src/input_safety_opening.rs` | 未获放行的阻断才算挡路；被接受的遗留运行不再挡路；**启动路径自身操作也结账**；`StartupInputSafetyReport` 增加 `settled` 与 `acknowledged_runs`／`human_review_required` | 放行后开放可达；杜绝"每次启动留下一条待对账"的同源挂账 | 开放侧用例 1 条 + 真实库两遍验证 |
| **B24** | web-console | `src/main.rs` | 归属端点增加 `input_safety_recovery`；新增 `POST /api/system/release-isolation`（署名/理由校验 + **集合相等**防 TOCTOU + 库侧落定 epoch/时刻） | 机器可读的处置呈现与受控放行入口 | 前端守门用例；HTTP 实测 400/409；状态未被改动 |

## 4. 打包、安装与基础设施

| # | 文件 | 锚点 | 改动 | 目的 | 验证 |
| --- | --- | --- | --- | --- | --- |
| P1 | `scripts/build-msi.ps1` | `PKG-ROOT-BINDING-*`（6 处） | 报告解析**只认包根**：四项一致性断言（内容哈希／report_id／载荷摘要／快照摘要），不符即 throw；**删除 latest 指针 fallback** | 修掉"报告 B 代 + 载荷 A 代静默成功" | 三组构造验收 |
| P2 | `scripts/package-all.ps1` | `Resolve-ExternalBuildInputVerification`（2 处） | 外部构建输入核验：manifest checksum 与 `Cargo.lock` 逐字比对 | 来源缺口按"登记外部输入"解决，**不放宽**校验 | 单独探针实测 `verified:true` |
| P3 | `scripts/lib/build-identity.ps1` | `external_inputs_verified`（2 处） | 收据新增 `external_inputs[]`／`external_inputs_verified`／`source_snapshot_verified` | 不误导为"源码快照含全部二进制来源" | 报告实测三字段 |
| P4 | `config/package-manifest.json` | `external_build_inputs`（1 处） | 登记 `cargo-registry:webview2-com-sys`（registry／crate／version／checksum／artifact 映射／权威文件） | 第三方 registry 产物有据可查 | 报告 `external_inputs` 实测 |
| P5 | `.gitattributes` | `eol=lf` 规则 | **恢复既有原文**（钉 app.js/styles.css）并追加 `*.rs`/`*.ps1`/`*.json`/`*.md`/`*.toml` 为 LF ＋二进制标记 | 本机 `core.autocrlf=true`，而 `pipe.rs` 的文本钉住断言把 LF 当契约 | 可见追加段（**含一次覆盖事故说明，见 §7**） |
| P6 | 产物 | `dist/CoolzhuAgent-0.2.15.msi` → `0.2.16.msi` | 两次构建均 `release_eligible=true`、六项一致性条件全 PASS | 可发布包与安装 | `msiexec exit=0`；注册表 0.2.15→0.2.16 |
| **P7** | 安装生命周期（本轮实跑） | `msiexec` 修复／卸载／重装 | 修复 exit=0；卸载 exit=0 且**用户数据与工作区数据保留**；重装 exit=0，CLI 报 `0.2.16`，输入安全库完好 | 证明安装可重复、可升级、非破坏性 | `msiexec` 退出码 + 目录比对 |

## 5. 未完成 / 未验证（**不得当作已完成**）

| 项 | 现状 | 出处 |
| --- | --- | --- |
| RD4-03 剩余 | R3（撤销未激活许可）、R4（跨进程执行者身份核查）、Goal 锚点真实关系、真实子进程崩溃变体 | §B-69 |
| P1-1 根 deadline（Phase 2） | 按裁决 **Phase 1 保持"未接线"并核实不假装**：`RootDeadlineState::NotWired` 如实呈现、`RunBudget` 在 web-console **零引用**（§B-87）。Phase 2 的 `RuntimeDeadlineContext`（chat/goal/relay accept 冻结 + `remaining = root ∩ CU ∩ stage`）**按裁决暂不做** | §B-87 |
| P1-2 切换/关闭口径 | **已实现**（PR-04）：`outcome` 结果码（`applied`／`shutdown_incomplete`）+ 拒绝在应用之前返回 + 前端不再把 off 当已完成（2 条守门用例）。**vision switch 仍未纳入排空**（vision 服务未登记在途，代码注释已如实标注） | §B-87 |
| P1-3 PKG-L07c-RACE | 按裁决**不重新打开**（P0 修复已完成，并发能力待验收） | 决策 §P1-3 |
| PR-05（P2-1 · CU-01） | **五列已实现**：`input_status` 四值（§B-88）＋统一 run 计数（`finish` 回填，修掉 C9 的「有 3 步却存 0」）＋任务级基线＋请求状态（无 usage 仍保留 attempt）＋失配子类（§B-90）。**未做**：这些列的 UI/报告落列；CU-02 的 owner=room/turn/run 归属口径 | §B-88／§B-90 |
| **PR-02B**（`tool_calls` 登记链） | **已实现 + 生产已接线**（§B-91）：v25 登记表、派发边界写入（dispatched → completed/failed）、authority 只认**登记表真有**的关系、执行器"有登记必声称/无登记不声称"；4 条用例含端到端 | §B-91 |
| ~~PR-02B 剩余：SafetyCleanup 事故登记~~ | **已闭环**（§B-93）：v26 登记表 + 执行器真实生产者（未确认释放 ⇒ 登记）+ 资格由解决路径给出 + authority 按 incident_id 核对；CU-F05-5 **两半齐备** | §B-93 |
| **CU-02**（桌面输入租约归属） | **归属口径已实现**（§B-92）：owner 串 = `room\|turn\|call_id`（缺维退回运行 id，不编造占位符）；跨进程互斥/崩溃回收/epoch 防复活**原有机制保留** | §B-92 |
| CU-01 五列落列 | **报告面 + UI 面均已实现**（§B-94／§B-98）：`GET /api/computer-use/run-report`（单次）与 `GET /api/computer-use/runs`（列表）+ 状态栏「CU 运行…」入口；不可读时如实报 `unavailable` | §B-94／§B-98 |
| **CU-04**（坐标与陈旧帧） | **安全半已具备 + 本轮补齐度量验收**（§B-95）：vision 返回 0–1 相对坐标、宿主映射含越界拒绝、四档缩放用例、两表面陈旧帧守卫（桌面含 rect/dpi）；新增**跨缩放往返误差 ≤1px** 断言 | §B-95 |
| **Hook 授权服务（P2-2）** | **已实现（插件侧对齐）**（§B-96）：`HookAuthorizationService` + 默认什么都不授权 + `unauthorized_skips` 可分辨"没跑"与"允许"；运行时侧原已具备该语义 | §B-96 |
| **CU-05（UIA 状态）** | **已实现（含可见面）**（§B-97）：快照/命中四字段（`None` = 不支持，不是 false）+ 纯映射 + 状态进入 locate 的 `raw_response`（`unsupported`/`unknown` 如实写）；离线用例齐；**真实 UIA 读取未现场执行**；模式为**逐个探测**。**未做**：接进 observation 的结构化 `state`；升级 windows crate 拿全量模式 | §B-97 |
| **CU-04 剩余（canvas ROI）** | **已实现**（§B-99）：闭环请求可选 `canvas_roi`（调用方声明的相对画布）⇒ 可见 ROI 被裁进画布并如实报告 `canvas_clipped`；越界/空区域/完全落画布外分别 400/400/409（**不钳制**）。**未做**：UI 不声明画布；帧绑定的命名结论见下行（§B-107） | §B-99 |
| CU-05 结构化通道 | **已实现**（§B-100）：UIA 状态进入观测 `state.elements`（`selected`／`keyboard_focus`／`toggle_state`／`patterns`），`null` = 不支持（不是 false） | §B-100 |
| **CU03-SCORER** | **已实现**（§B-105）：旧规则改名 `metrics_v1` + 两层判定 + 三个带标签语义指标 + 适用分母（"不适用"≠0%）+ 八条反例 + 变异验证（12 条用例）。**旧 20 次已转入保留体系并离线重评分 ⇒ 语义层不可评分**（缺标签/缺上一步事实），与"管线跑通"这一结论一致 | §B-105 |
| **CU-03 planner 反馈** | **代码半已实现**（§B-102）；装置与评分器已就绪（§B-103／§B-105）。**收益结论仍待** CU03-CONFIRM（冻结标签后配对对照）——其前提未满足，见下行 | §B-102／§B-105 |
| **CU03-CORPUS** | **已实现**（§B-106）：历史素材已定位并归档到 `docs/testing/cu03-eval/corpus-2026-09-19-paint-window-drag/`（四类分类 + 8 样本逐项溯源 + 8 张真实截图且摘要 8/8 自校验 + 7 条可执行守卫 + **变异验证 11/11**）。**历史素材仍不足以支撑语义层**：提示词本体/UIA 元素树/几何变换/语义标签全缺 ⇒ 三项语义指标**不可评分**，不补零。**副产物两硬结论**：`action_fingerprint` 含观察代次（不能当"同一操作"身份，`paint-r3` 为真实反例）；既有绑定已有"观察代次+摘要+尺寸"雏形，缺窗口 rect/DPI/裁剪缩放 ⇒ 给 FRAME-BINDING 落点 | §B-106 |
| **COMPAT-ID（缺顶层 message ID）** | **已实现**（§B-104）：`id: Option<String>` + 唯一归一化器（缺失/null ⇒ 严格拒绝／兼容"未提供"；**空串与错误类型两种模式都拒绝**）+ 连接级兼容位（默认严格、不硬编码域名、不按模型名猜测）+ 流式同源处理与"不提前判结束" + 工具配对仍严格 + **不重发、不换协议**；五个面 8 条用例；**真实端点小范围复验通过**（产品同一入口，`provider_message_id=None`） | §B-104 |
| **FRAME-BINDING（原 `frame_id` 命名）** | **已交付（§B-107）；结论是「不需要新标识」**：审查发现观察代次／图像摘要／尺寸／`screen_rect`／`canvas_rect` **都已存在**，只缺「绑在一起并可核对」。故新增 `computer_use_frame.rs`（`FrameRef` 复用既有值、`parse_screenshot_evidence` 把只写不读的证据串变成可读、`frame_binding:` 写进既有 evidence 通道），**不新增表/列/ID 空间**。拖拽映射**前**把门（`frame_unbindable`／`wrong_image`／`wrong_scale`），截图画布分支补上此前看不见的 `wrong_crop`；click 的图像内容变化**刻意仍不检查**（避免误拒）。窗口身份五项与原陈旧重试语义无侧改；接线由根级源码守卫 + 变异验证 3/3 钉住 | §B-107 |
| **RPR-01b 的"同进程读者"那一半** | **已交付（§B-108）**：web-console 原来只有「恢复」没有「锁」——进程环境是全局的而测试并行跑，两个用例各改库根时**都可能读到对方的值**；另有一处**完全没有守卫**的裸保存/恢复对（panic 即永久删变量），而逐文件检查看不出来。新增 `web-console/src/test_env.rs`（全 crate 共用锁 + 守卫**同时持锁**、可重入、恢复在锁内），迁移三处；根级守卫**收紧为逐处**（测试区出现裸 `set_var`/`remove_var` 即红，唯一例外是守卫模块），现状测试区裸写入 0 处。变异验证 4/4；全量套件连跑 3 次全绿。**§B-101 仍未复现，故不主张这就是它的原因** | §B-108 |
| ~~受控族缺 `move` 入口~~ | **已闭环（§B-89）**：helper 增加 `move`／`move_relative` 模式（相对移动由 helper 自己读位置）、`controlled_move_mouse_absolute/relative` 上线、两处调用迁移完成、移动原语改名 `diagnostic_*`、守门**例外清零**；真跑用例（mock 驱动 + 真实 helper 进程）通过 | §B-89 |
| 真实崩溃变体 | 只在**真实库**上观察到"半途中断后重放"（已修并验证）；"恢复者进程被杀"的跨进程变体未构造 | §B-77 |
| 由 launcher 启动的控制台进程内**输入安全联动** | 直接注入 env 运行已观察（走评估/保持隔离）；launcher→控制台链路的该行日志**未直接抓取**（进程由 launcher 派生） | §B-76 |
| **签名** | 包为 `unsigned`；本轮不涉及 | §B-72 |
| ~~Phase 3 原表~~ **已撤回** | **本行原列的两条契约要求已由 2026-09-26 补充裁决正式撤回，不再是工单**：`ExecutionOutcome` 取代 `Result<StepExecution, ComputerUseError>`（撤回，保留既有 `Result` ＋ `ComputerUseError.receipt`）、新增 `ActionScope::NativeAction`（撤回，保留 `ContextKind`／`ActionSource` 正交设计） | §C-收口（撤回标记） |
| CU 释放义务（原记「新 P0」） | **既有修复保留**（§B-43）：五态 `ReleaseObligationState` ＋ 唯一推导点 `derive_release_obligation` ＋ 消费侧闸门 `NotSent ⇒ NotNeeded`（`contracts.rs:237-246`）＋ 零填充已删除。**2026-09-26 逐符号核对仍在位**；不再作为待开工缺陷，只有真回归才重开 | §B-43、§C-收口 |
| **Goal 正向强父关系** | **① 表达力已交付**（§B-111）：`ContextKind::GoalPhase` ＋ `GoalPhaseActionContext`（强父：goal/phase/阶段运行 id、工作区；可缺省：房间/会话/发起轮次），`is_same_attempt_as` 表达阶段重试、未知上下文变体必须报错、受理侧**刻意 fail-closed**。**② 范围发现**：Goal→CU 的**接纳路径本身尚不存在**（`goal_phase` 与 CU 无任何交点），六个冻结项都还没有生产者；正向受理用例以「宿主按 `phase_run_id` 核对真实阶段运行」为前置 | §B-111 |
| **本轮新拆出的独立项**（不得与既有测试混作同一证据） | ① **真实进程级崩溃装置**（已批准，隔离装置；组合测试 3 目前只到存活探针模拟）② **Goal 正向强父关系**（组合测试 5 只覆盖否定路径）③ **R3 撤销未激活许可**（无实现，本轮给出许可状态机与恢复自锁口径）④ **R4 跨进程执行者实例核查**（无实现，本轮给出权威来源＝宿主启动登记＋OS 实例证据＋helper 回执）⑤ **恢复操作员认证授权**（现状只有记录与并发一致性检查；**补齐前普通 HTTP 放行接口不得凭署名改变资源状态**） | 2026-09-26 补充裁决 §2–§6 |
| Phase 4／5 剩余 | Paint 事实链剩余（CU03-CONFIRM 前提未满足、真实帧绑定拒绝需真实桌面、CU-05 现场 UIA 读取）、`PKG-L07c-RACE`、PowerShell fail-closed（已改，仅登记） | §B-102～§B-108、§C-收口-补 |
| **GitHub PR** | 未提交：仓库**无远端**、`gh` **未安装**、环境**无 token**；工作保留在本地分支 `rd4-input-safety-and-pkg-integrity` | §B-74 |

## 6. 复现与核验命令（本文件"锚点"的验证方式）

```bash
# 前端
grep -c "system-attribution" modules/gui-web/packages/web-console/index.html
grep -c "refreshAttributionAndRecovery\|describeAttributionAndRecovery\|attributionDetailText" modules/gui-web/packages/web-console/src/app.js
# 后端
grep -c "FrozenParentContext\|validate_frozen_parent_relations\|canonical_workspace_identity" modules/gui-web/packages/web-console/src/main.rs
grep -c "require_resource_accepts_new_input\|physical_input_resource_scope" modules/gui-web/packages/web-console/src/computer_use_executor.rs
grep -c "rebind_recovery_operation_authorized" modules/gui-web/packages/web-console/src/legacy_recovery_driver.rs   # 本轮：应为 1
grep -c "CapacityFault\|PIPE_CAPACITY_FAULT" modules/gui-web/packages/windows-process-guard/src/pipe.rs
grep -c "select_holder_by_recency" packages/app-launcher/src/launch_paths.rs
# 打包
grep -c "PKG-ROOT-BINDING" scripts/build-msi.ps1
grep -c "external_build_inputs" config/package-manifest.json
# 门禁（全量 + 本轮新增回归用例）
cargo test -p coolzhu-web-console --offline                                  # 期望 1115/0
cargo test -p coolzhu-web-console --offline replay_rebinds_an_operation      # §B-77
cargo test -p coolzhu-web-console --offline operations_held_by_a_live        # §B-78
# 现场观察（真实库、真实资源；期望"不再失败"）
COOLZHU_INPUT_SAFETY_STATE_ROOT="$LOCALAPPDATA/CoolzhuAgent/input-safety" \
  ./target/debug/coolzhu-web-console.exe 2>&1 | grep "输入安全启动路径"
```

## 7. 遗留项与原因（技术路线疑难 / 规格不清，**按现状如实保留**）

| 遗留项 | 现状（可观测） | 为什么遗留（原因） | 影响面 | 需要什么才能推进 |
| --- | --- | --- | --- | --- |
| ~~被拒绝的恢复操作永不结账 ⇒ 永久隔离~~ | **已闭环（PR-01）**：终端处置 + 人工放行通道；真实库 `pending 1 → 0`、`settled=2`、`human_review_required=2`、资源仍隔离（**设计结果**） | 原缺口是"规格未明确"，本轮按决策落地 | — | 无需再裁决（结账/放行机制已具备） |
| **放行资格与复核口径未定义**（PR-01 伴随项） | 通道已具备（署名/理由/证据 + TOCTOU 校验）；但"谁有权放行、是否需要双人复核、证据放哪里"**没有规定** | 决策只要求"人工确认 ⇒ 新增 `ReleaseIsolationDecision`"，未定义运营权限模型 | 放行是**安全决定**：任何能访问本地控制台的人都能署名放行 | 一次口径确认：放行者身份来源（本机账号？）／是否双人／证据留存位置 |
| **P0-2：CU 动作事实进业务链** | **已实现 + 生产已接线**（PR-02A Stage 1+2+3，§B-84／§B-85）：attempt 落库（v24 登记表 + 一动作一请求约束）、生产 `ProductionActionOriginAuthority`、执行器**输入前准入** + **步骤行与动作事实同事务** | 裁决口径：attempt 取落库；输入前身份拒绝＝零物理输入 + 零步骤行 + 只留审计；`tool_call_id` 有登记才必填、无登记不得伪造 | — | **CU-F05-5 正半**（SafetyCleanup 事故登记）与 **PR-02B**（`tool_calls` 登记链）仍待做 |
| **P0-3：普通输入路径统一生命周期** | **已实现 + 生产已接线**（PR-03，§B-86）：两条仍在跑的自动化路径（桌宠自动化、闭环评测执行器）迁到受控族 `controlled_*`；11 个无生命周期原语改名 `diagnostic_*`（**编译器**证明只剩诊断在用）；根级源码守门钉住例外 | 决策要求「禁止多套并存」 ⇒ 收口方式是「编译器 + 守门用例」，不是口头约定；6 项必测在受控族已覆盖（含真实子进程） | — | ① 受控族缺移动入口（相对/绝对移动仍是唯一例外，**无释放义务**）；② 两条路径暂无取消信号（传 `\|\| false`，已注明） |

| **R3：撤销未激活许可 / R4：跨进程执行者身份核查** | 未实现 | R4 需要**跨进程**执行者身份来源（当前只有进程内身份）；裁决未指定可信来源与失败语义 | 恢复期无法证明"旧执行者确实已停" | 指定跨进程执行者身份的权威来源（如 helper 侧登记 + 存活核对）与其 fail-closed 语义 |
| **Goal 锚点的"真实关系"** | 驱动按 `session_id/turn_id → runtime_runs → owner` 解析；Goal 侧关系未纳入 | 现状无法从设计上明确 Goal 锚点与 chat turn 的**必然**关系（可能本来就不一一对应） | 涉及 CU 归属与审计的完整性 | 明确 Goal↔turn 关系的规格（是可缺省的弱引用，还是必须存在的强关系） |
| **真实子进程崩溃变体** | 只覆盖"半途中断后重放"（本轮修好并实测） | 需要能**受控杀死**恢复者进程的跨进程实验装置（而非仅单进程模拟） | 崩溃恢复的证据强度 | 允许引入进程级故障注入装置（或提供可重复的现场复现步骤） |
| **签名** | 包 `unsigned` | 无代码签名证书与时间戳服务（环境不具备） | 安装时会出现来源警告 | 证书与签名流水线 |
| **guard 容量记账用例的原始偶发失败**（§B-81） | 基线相对断言后：定向 20/20 + 全量 3/3 轮绿；原始 `(4,4)` 不可复现 | 未定位到根因（怀疑负载敏感下的测试清理时序） | 门禁可信度 | 若再现，用受控负载 + 日志复现；否则保留"归因待核" |
| ~~**`.gitattributes` 覆盖事故**~~ | **已闭环（§B-110）**：覆盖前原文已从**本仓库对象库**找回（不可达 blob `7869d60a`，对象创建 18:57 / 覆盖 18:58），并按 §7.3「有可靠关联」分支恢复遗漏规则 `* text=auto eol=lf`；已核对原文规则行是当前文件的**子集**、`git check-attr`／`--eol` 实际生效、影响面 0、语料库字节固定未破。**前向基线已建立**（规则逐条依据写在文件头） | 原判断「未被跟踪 ⇒ 无法从 git 恢复」**过强且已被本轮裁决废止**：那次 `git add -A` 确实把覆盖前内容留在了对象库；我当初据以「恢复」的 `pr-checkout` 旧副本只有 3 行，**不是**覆盖前原文 | — | 无需再裁决（已按 §7.3 有依据分支处理；未执行 renormalize/gc/prune/reset/clean） |
| **PowerShell fail-closed（tool-registry）** | `require_powershell_or_fail` 已改（缺 powershell 即失败，不再跳过） | 已完成；列入本表仅为完成度透明 | — | — |

---

## 8. 高风险／复杂待办：范围、风险与前置（2026-09-26）

> 本节只登记**未做**的项，并把「为什么不能顺手做掉」写清楚。凡本轮已完成的都在 §1–§5 有交付记录；
> 本节**不是**待裁决清单——裁决都已给出（§C-收口 与 `round8-*`），这里是**执行前的范围与风险说明**。
>
- 排序用裁决 §9.1 的口径。每条给：**真实范围**（动手前核实过的事实）、**风险／复杂点**、**阻塞点**、**前置**。

> **接手入口**：8.3c 生产者 → 许可门接线的完整交接（当前状态、逐步锚点、禁止事项、验收入口、失败纪律）见 `handoff-8.3c-producer-2026-09-26.md`——本会话在此处收尾，下一步由新会话按该文件执行。
>
> **2026-09-26 裁决调整了两项阻塞判断**（本节已同步）：① 「未授权解除当前隔离」**不等于**只能测拒绝路径——隔离测试库可验授权/恢复/重新接纳的**成功路径**，也可由实际操作员完成**不产生放行资格**的原生身份验证；② 「崩溃装置依赖 R3／R4」**不等于**整个装置都要等——进程监督、测试存储、同步屏障与清理框架可先建设，依赖具体许可状态与执行者身份的故障场景随后接入。

### 8.1 恢复操作员认证授权（§9.1「优先」）——**已只做最紧一层，其余待真实环境**

- **已完成**：公开 HTTP 放行入口 fail-closed（`operator_authorization_available()` 恒 `false`，
  排在两处校验之后、任何写动作之前；守卫 + 变异 3/3，见 §B-109）。**"任意控制台访问者可署名放行"已关掉。**
- **真实范围（未做部分）**：原生系统验证（优先桌面应用适用的 Windows Hello／用户验证接口，
  回退受控系统凭据验证并**验证取得的实际身份**）；身份与 **Windows 用户 SID** ＋ `resource_scope` ＋
  政策版本绑定；`ReleaseIsolationDecision` 补齐关联字段；**120 秒一次性确认窗口**；
  证据目录 `<input_safety_state_root>/evidence/`；确认期间暂停产品自身自动输入。
- **风险／复杂点**：① 属**安全决定**，实现错了比不做更危险；② `IUserConsentVerifierInterop` 有
  最低系统版本要求，**必须检查适用范围**，且**不得**因引入它静默抬高产品最低 Windows 版本或
  升级全部 windows crate；③ 凭据对话框"返回成功"**不等于**身份已验证——认证是独立一环；
  ④ 敏感缓冲区需及时清理，且不得进入 Web／日志／模型上下文／普通数据库。
- **阻塞点（已按 2026-09-26 裁决 §2.1 改写）**：~~未授权解除隔离，所以只能验拒绝路径~~ → **当前真实隔离的放行验收未获授权；但认证成功、策略允许和隔离测试库的恢复提交可以分别验证**。
  这不是放宽闸门，而是把三件事拆开：**证明当前是谁** → **判断此人是否有权批准这一次恢复** → **在条件仍成立时提交真实放行决定**；任何前一步成功都**不能自动调用**下一步。
  由此可推进的：模拟认证器 + 真实策略代码（隔离测试程序，覆盖通过/拒绝/过期/身份错误/重复消费）、**隔离安全库的正向恢复**（合成 incident，走真实恢复事务解除测试事件）、操作员发起的**原生"仅验证身份"**（只用于验证报告，不产生当前真实事件的放行资格）。
  仍不能据此宣称：已经获准解除当前真实隔离。
  隔离测试必须**独立 store_id／scope／证据目录／进程配置**（不能只是给真实库换个文件名）；测试确认结果不得被生产接口消费，生产恢复凭据也不得被测试程序反序列化复用。
- **前置**：真实桌面环境 ＋ 有权限的实际操作者；以及一份明确的适用范围判定（哪些 Windows 版本的哪个接口）。

### 8.2 R3 撤销未激活许可（§9.1「优先并串行」）——**需从零建持久许可登记**

- **真实范围（已核实）**：**不存在可复用的许可／broker 概念**——全仓库唯一的 `Broker` 是浏览器桥的
  连接 broker，与输入许可无关；`gate_revision` 无任何命中（见 `§C-收口-补` 与 `e2cf7cf`）。
  要做的是：六态许可登记（待激活／已消费进入派发／执行中／已结束／已撤销／结果未知）＋
  关闸并推进 gate revision ＋ **原子撤销**未消费旧许可 ＋ 已消费进在途清单交 R4 ＋
  最终派发前再核许可与 epoch。
- **风险／复杂点**：① 触及**输入安全库 schema**——按裁决该库由负责人**串行合并**，不能与其他
  改动并行落库；② 许可"消费"与"关闸"的**先后顺序**必须可判定（关闸在先则不得激活；许可先消费
  则属在途，**不得事后宣称零输入**）；③ 必须避免**恢复自锁**：活跃 helper 若持有整段输入排他，
  关闸／取消不得先等它自己放锁，需要"只收紧"的准备权限（且该权限**不得**变成放行通道）。
- **已交付 8.2a（纯逻辑，2026-09-26，§B-113）**：六态 `InputPermitState` + 冻结转换表 + 许可最低绑定 `InputPermit`（含「激活前必须建立执行者实例」）+ 重复请求处置 + 关闸/消费竞争判定 + 只收紧入口。**未落库、未分配 schema 版本号**（版本号由该库负责人在合并时分配）。
- **阻塞点**：schema 变更的串行窗口；以及 R4 的就绪（在途清单需要 R4 才能核查，否则只是把
  未决状态搬到另一个表）。
- **前置**：与输入安全库负责人约定 schema 变更次序；先把"许可状态机"的纯逻辑与测试做完，
  再落库（这样大部分工作可离线完成，不占用 schema 窗口）。

### 8.3 R4 跨进程执行者实例核查（§9.1「优先并串行」）——**需宿主启动登记 + OS 实例证据**

- **真实范围**：权威来源＝**宿主启动登记**（哪个 run／action 由谁创建）＋**OS 实例证据**
  （创建返回的进程句柄、创建时间、实际用户／会话）＋**helper 执行回执**（执行过什么）。
  要新增执行者登记（`executor_instance_id`／`launch_operation_id`／`coordinator_instance_id`／
  PID ＋创建身份／用户会话关联／协议版本／监督绑定状态／执行状态与最后回执），并实现六类失败语义。
- **风险／复杂点**：① `owner_id`／`coordinator_id`／PID **三者不得互相替代**——历史上被写坏过
  （"能表示成字符串就互相顶替"），是复发高危点；② `AccessDenied`／无法取得创建身份必须是
  **Unknown**，不得当"不存在"，也**不得为继续流程自动提权**；③ 只准在**同一个已核实实例句柄**上
  完成后续操作，不得"先核对一个 PID、再按该 PID 重新打开另一个实例去终止"（TOCTOU 式误杀）；
  ④ 活句柄**不能**序列化后当跨进程能力；⑤ Job 关闭只约束真实成员，**不能**因父进程退出就宣称
  所有后代已停。
- **阻塞点**：需要新的持久登记（同 R3 的 schema 约束）；且"启动顺序"要改（登记启动意图 →
  创建**尚不允许输入**的 helper → 绑定监督 → 取实际身份 → 持久化 → 协议握手 → **最后**才签发输入许可）。
- **已交付 8.3a（纯逻辑，2026-09-26，§B-114）**：三种证据来源分工（各自写明能/不能证明什么，**没有**任何来源可单独断言已停止）、`HelperIdentity`（解释器路径不足以证明本次受控脚本）、`ExecutorRegistration`（未绑定监督不得算已核查通过）、创建身份完整性、**句柄世代**（重新取得句柄后上一次核查不继承）、失败表九行逐行冻结、自动提权恒不允许、后代证据（父退出与 Job 关闭都不算）、终止阶段（只有已确认退出算停止确认，超时进人工复核）。**未落库、未分配版本号**。
- **前置**：R3 就绪 ＋ schema 窗口 ＋ 与 helper 协议负责人的串行合并。

### 8.4 真实进程级崩溃装置（§9.1「跟随 R3／R4」）——**依赖 8.2／8.3 的状态**

- **真实范围**：父测试创建**专用子进程**、每次运行独立会话库／输入安全库／证据目录、
  测试专用锁命名空间、`cfg(test)`／默认关闭的故障入口、只终止**自己持有创建句柄**的子进程、
  受控假输入、看门狗与受控后代回收；用命名事件／管道屏障确认前置阶段**真的到达**后再终止。
  最低六个故障点（取恢复排他后登记前／阻断已提交会话终态未提交／终态已提交安全操作未结账／
  协调者死亡但测试执行者仍存活／迟到请求或回执／恢复时另一进程竞争）。
- **风险／复杂点**：① 装置本身要**不碰真实产品、模型服务、用户程序**；② 不得主要靠 `sleep` 猜时序；
  ③ 父级收尾失败**也算测试失败**，不能删掉临时目录就称资源已回收；④ 故障点里"许可撤销／执行者
  登记"的部分**引用 8.2／8.3 的状态**，所以不能先做。
【已更新】裁决 §5.1 明确「装置基础现在就可开工」，**已完成 8.4a**（§B-115，`tests/crash_harness_smoke.rs`）：真实父子进程（`current_exe()` 子模式）、只经真实句柄终止（无裸 PID 入口）、五类独立测试目录、测试专用 scope、文件屏障（先写 `.partial` 再 rename）、有界收尾（失败即测试失败）、终止异步语义（kill 只算已请求）、可执行边界不变式（变异 3/3）、连跑 3 次全绿且无残留进程。**已交付 8.4b（§B-116）**：测试事件词汇表 6 个，**锚定到已冻结契约**（消费 ↔ `DispatchCommitted` 且断言已越界不可再派发、登记 ↔ `VerifiedAlive`、恢复类 ↔ 既有 `RecoveryStage`）；事件可被真的驱动（端到端用例）；K1–K6 依赖门**机器可判**（与 §5.2 表逐行一致、必须点名阻塞理由），并断言**当前不得声明任何 K 场景通过**。依赖现状全部不可驱动且各带理由：协调器在 web-console **bin 内部**、根测试目标无法导入 ⇒ K1 需先暴露 lib 入口或下沉驱动。**未做**：K1–K6 真实故障场景（依赖未落地）。
- **阻塞点**：8.2／8.3 就绪。**可以先行**的部分：进程创建/终止与屏障机制本身（与产品状态无关），
  但价值有限，故按裁决顺序排在其后。
- **前置**：8.2／8.3 落地后一次性做，避免装置建成即返工。

### 8.5 Goal→CU 接纳路径与正向强父关系（§9.1「可并行」）——**表达力已交付，受理能力缺宿主登记**

- **已完成**：`ContextKind::GoalPhase` ＋ `GoalPhaseActionContext`（§B-111），受理侧**刻意 fail-closed**。
- **真实范围（未做部分）**：① **Goal→CU 的接纳路径本身不存在**（`goal_phase` 与 CU 零交点，
  `FrozenParentContext::new` 只有一个生产调用点）⇒ 要新建这条路径，并冻结 §5.2 的六项
  （阶段运行 id、Goal↔阶段关系、阶段尝试身份、工作区／owner／claim／资格、房间会话关联、
  可选发起轮次）；② `ActionOriginAuthority` 需新增"按 `phase_run_id` 查真实阶段运行"的能力，
  生产实现要从 `goal_phases` 读真实关系；③ §5.5 的正向用例与五个场景（阶段重试／错误父运行／
  跨工作区引用／父运行取消／迟到回执不重开旧阶段）。
- **风险／复杂点**：① 触及**权限契约**（trait ＋ 生产实现）与 `main.rs`（接纳路径），均属串行合并范围；
  ② 最容易犯的错是"为了凑字段而复制 ID"或"在同房间活跃运行里挑一个"——裁决明令禁止；
  ③ 不得把 Goal 自治任务标成 `UserDirect`／`ControlPlane` 来逃避父关系要求。
- **阻塞点**：②的 host 查询能力是①③的前置；且**正向用例在受理打开前必然失败**（当前刻意拒绝），
  所以测试要在同一批里"打开受理 + 加正向用例"。
- **前置**：权限契约的串行窗口；随后可一次性完成（表达力已就位，不用返工）。

### 8.6 需要真实桌面、且**本轮未授权解除隔离**的项

| 项 | 为什么必须真实环境 | 现在为什么不做 |
| --- | --- | --- |
| **CU-05 现场 UIA 读取** | 逐个 pattern 的读取能力只能对真实控件验证（离线只能验映射） | 需真实桌面；裁决要求"缺 feature 可最小补齐"，**不**把升级整个 windows crate 全量模式设为必需 |
| **真实帧绑定拒绝的端到端** | `frame_unbindable`／`wrong_crop` 等只在真实窗口＋真实截图下触发 | 同上：本轮不授权为它解除隔离让路（§B-107 已离线钉住 8 项验收） |
| **CU-04 UI 声明画布** | "有可靠 ROI 就提供来源"——可靠性要靠真实界面判断 | 裁决明确**不要求** UI 随便声明一个白色区域；不可靠就继续"不确定" |
| **launcher→控制台输入安全联动日志** | 那行日志由 launcher 派生的进程产生 | 需实际以 launcher 启动真实产品；**不得**用手动注入环境变量替代（§B-76） |
| **vision 服务排空** | 需要在途请求的真实登记与实例级接纳 | 属"不按端口认领服务"的实例级改造，与 8.3 的实例核算同源 |

### 8.7 其余（已是"如实保留"，不是待办）

| 项 | 现状 |
| --- | --- |
| **PKG-L07c-RACE** | 原工单继续；**不阻塞单发布**。注意裁决口径：**100 次等固定轮数不是单独的安全证明**，须有可控故障点与具体不变量（100 次并发导出／10 个失败竞争者／随机 kill；winner generation 唯一、receipt 不错配、consumer 永远读完整代次）。装置较重，且需避免与其它打包改动并行 |
| **签名** | 包为 `unsigned`；缺证书与时间戳服务（环境不具备）⇒ **资源前置**，非技术问题。本轮不伪造证书、不安装签名插件、不使用凭据 |
| **§B-101／§B-81 偶发失败** | 均**未复现**（§B-101 未捕获用例名；§B-81 的 `(4,4)` 不可复现），保持"**归因待核**"，不改写成"已定位"。§8.3 允许的主动工作是：固定失败时的源码／测试二进制身份、为可疑共享状态建确定性场景、增加不改业务行为的结构化事件与状态快照。**已做**：环境并发观察类已被共用锁与逐处守卫消除（§B-108，变异 4/4）；**未做**：非空基线／并发释放的确定性场景（不确定能否复现，需专门一轮，且**不得**因加相对断言转绿就声称原因已解决） |

---

**本轮提交（分支 `rd4-input-safety-and-pkg-integrity`，**未推送**）**：`965166c`（输入安全事实链＋PKG 完整性＋安装 0.2.15）、`3f07b28`（台账）、`a1224f3`（§B-75 图标根因）、`46a8764`（启动器修复＋测试）、`04756db`（§B-76 修复验证）、`4bd93da`（协调器复用）、**本轮头提交**（§B-77/78/79 修复＋回归用例＋本文档与台账更新，用 `git log -1 --format=%h` 取当前值）。

---

## 9. 总体进度（**每轮执行推进后更新**）

最后更新：2026-09-26（两相 helper 与许可门主线，第 N 轮之后）

### 9.1 8.3c-A 两相 helper 与许可门：阶段进度

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| `8.3c-A-contract` | 六态许可、`ExecutionAttemptId`、两相生命周期、握手协议、ready/permit 文件协议、五条异常语义、协议版本与切换前门禁 | ✅ |
| `8.3c-A-host-ready` | `await_helper_ready`（有界等待；**文件存在 ≠ READY**；非本次会话 nonce 拒绝；非法信号**立即 `Rejected`** 不转 `Timeout`） | ✅ |
| `8.3c-A-helper-runtime` | 脚本两相段（**opt-in**，默认不执行）＋ nonce 来源改为**调用方**提供的 `two_phase_nonce`；**B128-T2 防回退结构守卫**（两相段不得用 `request_id` 作 nonce 来源，变异 1/1） | 🟢 **Step 1/2/3 ＋ T2 守卫 ＋ T6-A 通过**（真实 helper 行为验证：READY 后等待期零物理输入，变异 1/1）；**Step 4 剩余** T6-B／T6-C／T11／B128-T1 |
| `8.3c-A-executor-bind` | `ExecutorStore` 登记 ＋ `PermitGate` 真绑定 ＋ permit 通知 | ⏳ 未开始（裁决冻结） |
| `8.3c-A-input-switch` | `controlled_*` 一次性切换／`computer_use_executor.rs:228` | ⛔ **禁止** |

### 9.2 本会话累计交付（可按 §B 编号核对）

- 8.2b／8.3b **联合持久化**：输入安全库 `v2 → v3` 一次相邻迁移 ＋ DB-1…DB-8 全通过（§B-117）
- 宿主 bin **测试接缝** ＋ **K1 跨进程核心**跑通（真实进程，非模拟）（§B-118）
- 8.2c **使能步骤**：许可/执行者适配器经生产窄口可达（§B-120）
- **B-121 裁决落地**：`ExecutionAttemptId` ＋ 复合键 `(action_id, execution_attempt_id)` ＋ §四 四条重复判定 ＋ B121-T1…T5（§B-122／§B-123）
- **B-124 裁决落地**：删除 `policy_revision` ＋ gate/epoch 变化改为**可分辨拒绝** ＋ B124-T1/T2（§B-125）
- **B-126 修复**：consume 补全校验（全谓词条件更新、可分辨失败）＋ B126-T2/T4/T5（§B-128／§B-130）
- **实例比对判定**：PID 复用可识别（同 pid 不同创建时间 ⇒ `PidReused` ⇒ 不动当前进程）（§B-129）
- **两相生命周期与握手契约**、**ready/permit 协议与五条异常语义**、**协议版本与切换前门禁**（§B-134／§B-136／§B-141）
- 8.4 **崩溃装置基础** ＋ 事件词汇与 K1–K6 依赖门（§B-115／§B-116）
- **8.3c-1**：helper 身份在**存活时**捕获并经 `NativeInputOutcome` 暴露（§B-132）
- **8.3c-A host-ready**：`await_helper_ready`（§B-143）
- **helper-runtime Step 2/3**：`two_phase_nonce`（§B-150）
- **B128-T2 防回退守卫**：结构断言「两相段不得用 request_id 作 nonce 来源」（§B-151）
- **Step 1 测试注入**：`TwoPhaseHelperTestOptions` ＋ `native_run_with_mock_two_phase`（只加测试入口，默认路径零两相键；`helper_ready` 加显式 dead_code 豁免并注明接线后移除）（§B-152）

### 9.3 门禁基线（**每轮必须不低于此**）

| 目标 | 结果 |
| --- | --- |
| `cargo test -p coolzhu-computer-use-core` | **132 / 0**（含命令行长度门禁 `ScriptCommandLineBudget < 31000` 与 B128-T2 防回退守卫） |
| `cargo test -p coolzhu-web-console` | **1215 / 0**（1 ignored＝真实调用评测） |
| `cargo test -p coolzhu-core-runtime` | **347 / 0** |
| `cargo test --test module_linkage_smoke` | 8 / 0 |
| `cargo test --test crash_harness_smoke` | 7 / 0 |
| `cargo build --workspace --offline` | ✅（注意：本轮出现过 rustc 偶发崩溃，单次失败需复跑确认） |

### 9.4 下一步（唯一入口，无待决策项）

**Step 4**（Step 1 已完成）：按**固定顺序**跑真实 helper 测试并断言——
**B128-T1**（`request_id ≠ two_phase_nonce`，READY 用后者、progress 仍用前者）→ **T11**（A/B 两会话 nonce，B 读 A 的 permit ⇒ `RejectedPermit`＋`input=0`）→ **T6-A**（无 permit ⇒ `TimedOut`＋`input=0`）→ **T6-B**（错 nonce ⇒ `RejectedPermit`＋`input=0`）→ **T6-C**（合法测试 permit ⇒ **只有它允许输入发生**，用 mock 驱动计数）→ 三 crate 门禁 → **Phase 1 关闭**。

约束：不得为提速而合并/跳过真实 helper 或用静态 stub 替代；T6-C 的输入必须是 mock／受控执行计数，**不是**生产桌面输入；**诊断信息放 Rust 测试侧，不得写进 inline PowerShell**（命令行预算）。
插入点、期望结果、命令行预算约束、禁止事项见
`handoff-8.3c-producer-2026-09-26.md` **§3-ter**。

### 9.5 当前阻塞（每轮置顶复述）

1. **Step 1 尚未实现**：`native_run_with_mock` 仍需能携带 `two_phase_nonce`（有界改动，**无需新决策**）；
2. **命令行预算几乎无余量**：脚本 2144 单元，但**任何新增（连注释都算）都可能突破 31000 软门槛**——
   本会话已两次因此被门禁拦下；`-File` 迁移**已被裁决推迟**，后续 Phase 2 若继续增长需单独决策；
3. **分层约束**：`computer-use-core` **不依赖** web-console ⇒ 无法调用 `PermitGate`／`ExecutorStore`，
   Phase 2 必须做**两相 API 拆分**（改动半径 4 文件／3 crate），由 web-console 编排；
4. **需真实环境／资源**：§8.6 五项、操作员认证的原生验证、签名（证书）、`PKG-L07c-RACE`；
5. **环境异常**：rustc 偶发 `STATUS_STACK_BUFFER_OVERRUN`（已按"环境异常"记录，见 §B-148）。
6. **Step 4 的观测手段已有**（已核实）：mock 路径回传 `facts.injected_points`，"物理输入 = 0" 可直接断言，**无需新增管道**；写法配方见 handoff §3-ter 的「Step 4 测试写法配方」。
