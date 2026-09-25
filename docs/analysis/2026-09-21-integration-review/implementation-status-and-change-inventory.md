# 实施计划完成情况与改动清单（截至 2026-09-25 本轮末）

- **状态口径**（沿用第七轮裁决 §八）：**已裁决 / 已实现 / 生产已接线 / 目标环境·安装已验收**。四者不等价，"已实现"不等于"已接线"或"已验收"。
- **范围**：第七、八两轮裁决的实施；三次现场问题修复（图标无法启动、默认工作区口径确认、**启动路径自锁**）；一次测试基础设施的并发竞态修复；**PR-01（P0-1）恢复处置闭环**（决策：`RecoveryDisposition` 状态机 + 人工放行通道）。
- **验证基线**（本轮末实跑）：web-console **1133/0**、computer-use-core 123/0、windows-process-guard 50/0（1 ignored，另见 §7）、tool-registry 54/0、core-runtime **312/0**、module_linkage_smoke 4/0、app-launcher 64/0 + 5/0。
- **本文件所有"锚点"都可用 §6 的命令核验**，不是凭记忆写的。

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
| P1/P2（决策已排期） | PR-04 运行预算 root deadline（chat/goal/relay accept 冻结）；PR-05 Paint 事实层；PKG-L07c-RACE 并发验收；Hook 授权服务；RPR-01b 剩余裸 `set_var` | 决策文档 §P1/§P2 |
| 真实崩溃变体 | 只在**真实库**上观察到"半途中断后重放"（已修并验证）；"恢复者进程被杀"的跨进程变体未构造 | §B-77 |
| 由 launcher 启动的控制台进程内**输入安全联动** | 直接注入 env 运行已观察（走评估/保持隔离）；launcher→控制台链路的该行日志**未直接抓取**（进程由 launcher 派生） | §B-76 |
| **签名** | 包为 `unsigned`；本轮不涉及 | §B-72 |
| Phase 3／4／5 | CU 释放义务事实修复（**新 P0**）、`ExecutionOutcome`、`ActionScope`、Paint 事实链（CU-01/02 P0、CU-03/04/05 P1）、PKG-L07c-RACE、PowerShell fail-closed | §B-63、第八轮 §7–§11 |
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
| **`.gitattributes` 覆盖事故** | 已恢复原文并追加规则 | 我未先查看目标即覆盖；该文件未被跟踪、无法从 git 恢复，只能用 `tmp/2026-09-19-agent-fixes/pr-checkout/` 的旧副本 | 若有其它规则未包含，会丢失 | 你确认是否有其它规则需要补回 |
| **PowerShell fail-closed（tool-registry）** | `require_powershell_or_fail` 已改（缺 powershell 即失败，不再跳过） | 已完成；列入本表仅为完成度透明 | — | — |

---

**本轮提交（分支 `rd4-input-safety-and-pkg-integrity`，**未推送**）**：`965166c`（输入安全事实链＋PKG 完整性＋安装 0.2.15）、`3f07b28`（台账）、`a1224f3`（§B-75 图标根因）、`46a8764`（启动器修复＋测试）、`04756db`（§B-76 修复验证）、`4bd93da`（协调器复用）、**本轮头提交**（§B-77/78/79 修复＋回归用例＋本文档与台账更新，用 `git log -1 --format=%h` 取当前值）。
