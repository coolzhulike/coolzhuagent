# 实施计划完成情况与改动清单（截至 2026-09-25 本轮末）

- **状态口径**（沿用第七轮裁决 §八）：**已裁决 / 已实现 / 生产已接线 / 目标环境·安装已验收**。四者不等价，"已实现"不等于"已接线"或"已验收"。
- **范围**：第七、八两轮裁决的实施，加两次现场问题修复（图标无法启动、工作区默认值口径确认）。
- **验证基线**（本轮末实跑）：web-console **1113/0**、computer-use-core 123/0、windows-process-guard 50/0（1 ignored，另见 §5 的偶发）、tool-registry 54/0、core-runtime 311/0、module_linkage_smoke 4/0、app-launcher 64/0 + 5/0。
- **本文件所有"锚点"都可用 §6 的命令核验**，不是凭记忆写的。

## 1. 实施计划完成情况

| 单元（裁决编号） | 状态 | 关键落点 | 证据 | 尚未完成的部分 |
| --- | --- | --- | --- | --- |
| Phase 1 · RD4-02A 共享输入安全库 | **已实现**（契约＋宿主存储） | `core-runtime/src/input_safety.rs`、`web-console/src/input_safety_store.rs` | 契约测试 5 条；存储用例 10 条（含身份/空库/阻断核查/五表） | launcher 注入已加，但**联动行为未观察**（见 §5） |
| Phase 1 · 组合用例 1／2／3 | **已实现**（存储层；1/3 走协调器） | `input_safety_store.rs`（epoch／对账）、`legacy_recovery_driver.rs` | `combined_1/2/3_*` 三条用例 | 端到端"真实双进程 + 真实崩溃"变体 |
| Phase 2 · RD4-02B 协调器与资格 | **部分**（协调器＋可信 guard＋跨进程 Busy 已实现） | `InputSafetyCoordinator`、`RecoveryControlGuard`（同文件） | 用例 2：第二协调器 `input_safety_coordinator_busy`；类型级堵自证 | **R3**（撤销未激活许可）与 **R4**（跨进程执行者身份核查）未实现 |
| RD4-03 · 遗留收敛（语义＋驱动） | **已实现**（R1–R9 端到端） | `legacy_recovery.rs`（owner/资源不确定性）、`legacy_recovery_driver.rs` | 驱动用例 3 条：真收敛 + 保持隔离 + 两条拒绝路径 | 事实日志提交候选查询已接；**未跑含真实全量编译的出包后收敛演练** |
| 入口接线（共享库成为"真实消费者"） | **生产已接线** | `computer_use_executor.rs` 的资源门 + 四维关系核对 | `input_admission_goes_through_the_shared_safety_store`（四种情形） | 其他正式输入入口（非 CU 通道）尚未逐一核对 |
| 开放路径 + 启动触发点 | **生产已接线** | `input_safety_opening.rs` + `main.rs` 启动序列 | 用例 5 条；**安装后日志**：`输入安全启动路径: … opening=SkippedRootNotInjected` | 由 launcher 注入根后应改为"评估／开放"——**未观察** |
| PKG-PR-01 包根强绑定 | **已实现** | `scripts/build-msi.ps1`（`PKG-ROOT-BINDING-*`） | 三组验收：篡改⇒MISMATCH、无清单⇒MISSING（不回退 latest）、正常⇒绑定通过后按包根收据拒绝 | — |
| PKG-PR-02 外部输入登记 | **已实现** | `config/package-manifest.json`（`external_build_inputs`）、`scripts/package-all.ps1`（`Resolve-ExternalBuildInputVerification`）、`scripts/lib/build-identity.ps1`（`external_inputs_verified`） | checksum 与 `Cargo.lock` 逐字比对；收据新增三字段 | 其它第三方二进制若出现，需逐项登记 |
| PKG-PR-03 重建与安装 | **目标环境·安装已验收** | `dist/CoolzhuAgent-0.2.16.msi` | 六项一致性条件全 PASS；`msiexec exit=0`；注册表升级到 0.2.16 | 卸载／修复／重装；签名 |
| 现场修复①：图标无法启动 | **目标环境·安装已验收** | `packages/app-launcher/src/launch_paths.rs`（`select_holder_by_recency`） | 安装后 `--print-resolved-paths` 采用 `C:\Users\zhupu\coolzhuagent` 并持久化；控制台 pid 20988 + 桌面壳 pid 14568 均在运行 | 拒绝信息对双击用户**不可见**这一 UX 缺陷未修（待你定机制） |
| 现场口径②：默认工作区按设备账号解析 | **已核实（非缺陷）** | 随包 `config/package-launcher.json` 的 `"%USERPROFILE%\coolzhuagent"` | 模板形式；改动 diff 内**无**硬编码用户名 | — |

## 2. 前端修改点

前端＝`modules/gui-web/packages/web-console/{index.html, src/app.js}`。**`styles.css` 本轮未改**（沿用既有样式）。

| # | 文件 | 锚点 | 改动 | 目的 | 验证 |
| --- | --- | --- | --- | --- | --- |
| F1 | `index.html` | `data-role="system-attribution"` | 全局系统状态栏新增"归属/恢复"一格 | 让"历史归属未记录／待收敛／非终态运行行"在界面上可见 | 该 `data-role` 在 index.html 出现 1 处 |
| F2 | `src/app.js` | `refreshAttributionAndRecovery()` | 新增拉取 `/api/system/attribution-and-recovery` 的函数，并在 `refreshSystemInfo()` 中调用；错误分支把该格置 `—` | 把后端只读呈现接进既有系统信息刷新节奏 | app.js 中 3 个函数名合计出现 6 处 |
| F3 | `src/app.js` | `describeAttributionAndRecovery()` | 三态文案：`归属存储未初始化`／`归属存储不可读`／`历史归属未记录待收敛 N · 收敛待对账 M · 非终态运行行 K` | **不谎报**：未初始化/不可读与"没有待收敛"是不同事实 | 同上 |
| F4 | `src/app.js` | `attributionDetailText()` | 提示语说明"**非终态运行行包含正在执行的轮次**；只有执行已结束但终态提交失败时才残留，且**不会被报成已完成**" | 防止后人把该栏简化成误导性文案 | 守卫用例断言必须含这句 |
| F5 | （守卫用例） | `attribution_surface_frontend_is_wired_with_honest_wording` | 断言 app.js 含端点与诚实措辞、index.html 含该 `data-role` | 前端接线与文案的回归守门 | 该用例通过 |

## 3. 后端修改点

| # | 模块／crate | 文件 | 改动 | 目的 | 验证 |
| --- | --- | --- | --- | --- | --- |
| B1 | core-runtime | `src/input_safety.rs`（新增）、`src/lib.rs`（导出） | 共享输入安全库的**领域契约**：`INPUT_SAFETY_SCHEMA_VERSION=1`、StoreId/ResourceScope/ResourceState/Incident/RecoveryStage/`VerifiedBlockingRef`（无 `Deserialize`、无公开构造） | 格式规则只剩源头一份；阻断引用不可自证 | 契约用例 5 条；`lib.rs` 命中 1 处 |
| B2 | web-console | `src/main.rs` | `CanonicalWorkspaceId` + `canonical_workspace_identity`（源头唯一解析）＋ `validate_frozen_parent_relations`（四维可信关系核对） | A-2 来源分层：CU 在类型上拿不到未解析值 | 该文件命中 11 处；`FrozenParentContext` 17 处 |
| B3 | web-console | `src/main.rs` | **迁移 v23** 登记＋`SESSION_SCHEMA_VERSION=23`＋单步守卫修正（每步只比较本步号）＋4 条边界行为测试 | 遗留收敛列/表落地；杜绝"版本回退"隐患 | 迁移用例 6 条 |
| B4 | web-console | `src/main.rs` | 新增 `GET /api/system/runtime-identity`（复用唯一来源；只读、不建表、不谎报 0） | 机器可读的"后端实际身份" | 用例 3 条（含复用结构守卫） |
| B5 | web-console | `src/main.rs` | 新增 `GET /api/system/attribution-and-recovery`（只读）＋启动序列接入 `run_startup_input_safety` | 归属/恢复呈现；启动先驱动遗留收敛再评估开放 | `run_startup_input_safety` 命中 1 处；安装后日志可见 |
| B6 | web-console | `src/input_safety_store.rs`（新增） | 宿主侧 SQLite 适配：五表（identity/resource_state/incidents/resource_blocks/ownership_epochs/events）、身份与侧车标记、**不得静默建空库**、`verify_blocking_ref` 六项核查、按 epoch 授权、启动对账 | §1 裁决的全部实体与"未注入即拒绝" | 存储用例 10 条 |
| B7 | web-console | `src/legacy_recovery.rs`（新增） | owner **真实关系**解析（未知≠没有 owner）＋资源**不确定性**评估（未确认释放优先）＋事实日志提交候选查询 | 组合用例 4／5 | 用例 3 条 |
| B8 | web-console | `src/legacy_recovery_driver.rs`（新增） | **R1–R9** 驱动：协调器→隔离→登记→执行者核查→真实事故/阻断/已验证引用→owner→同事务终态→阶段提交→按资格开放 | 生产遗留收敛入口 | 用例 3 条（含真收敛） |
| B9 | web-console | `src/input_safety_opening.rs`（新增） | 启动**独立评估**（无未决阻断/无待对账/无待收敛）→ 协调资格 → 按资格 reopen → 释放排他 | 解决"接线后生产 CU 全阻" | 用例 5 条 |
| B10 | web-console | `src/computer_use_executor.rs` | 接入共享库资源门（`require_resource_accepts_new_input` + `physical_input_resource_scope`）＋保留四维关系核对 | 正式输入入口成为**真实消费者** | 该文件命中 5 处；消费者门用例 4 情形 |
| B11 | windows-process-guard | `src/pipe.rs` | `Saturated` 改为 `CapacityFault`（保留所有权/锁接纳/记录真实数量/不自动恢复）＋五指标拆分（`unreclaimed`/`owned_unreclaimed`/`unowned_unreclaimed`/`admission_rejected`/`capacity_fault`） | 不再"丢登记+关句柄+分离线程" | 命中 37 处；故障注入用例；门禁 50/0 |
| B12 | computer-use-core | `src/input.rs`（＋笔画路径） | 容量**预留前移到 `spawn` 之前**＋为安全收尾预留（`acquire_with_cleanup_reserve`／`CleanupRun`）＋命令行按 **UTF-16 单元含终止符**计量 | 消除可预期拒绝仍先建进程；计量纠正 | 命中 6 处；门禁 123/0 |
| B13 | app-launcher | `src/launch_paths.rs` | **多候选按来源优先级采用**（已保存选择 → 最近记录 `ConfigSnapshot.recorded_at_ms` → 导入旧选择 → 安装默认/历史遗留）；并列或全不可判定才要求显式选择 | 现场口径：有已保存配置就用"最近打开"的 workspace | `select_holder_by_recency` 命中 7 处；app-launcher 64/0 |
| B14 | app-launcher | `src/lib.rs` | 新增并注入 `COOLZHU_INPUT_SAFETY_STATE_ROOT`（取 `ResolvedLaunchPaths.input_safety_state_root`） | 生产路径必须注入；缺失即 fail-closed | 命中 3 处；用例断言注入值与契约字段一致 |
| B15 | cli | `modules/cli/packages/command-line/Cargo.toml` | 版本 `0.2.5 → 0.2.16` | 高于机器已装版本，走**升级**而非降级 | 文件第 3 行；MSI 版本校验与暂存 CLI 一致 |

## 4. 打包、安装与基础设施

| # | 文件 | 锚点 | 改动 | 目的 | 验证 |
| --- | --- | --- | --- | --- | --- |
| P1 | `scripts/build-msi.ps1` | `PKG-ROOT-BINDING-*`（6 处） | 报告解析**只认包根**：从 `payload-inventory.json` 的 `report_ref` 取报告，四项一致性断言（内容哈希／report_id／载荷摘要／快照摘要），不符即 throw；**删除 latest 指针 fallback**；引回 `package_report_ref`/`payload_inventory_ref`/`package_root_identity` | 修掉"报告 B 代 + 载荷 A 代静默成功" | 三组构造验收（§1 PKG-PR-01 行） |
| P2 | `scripts/package-all.ps1` | `Resolve-ExternalBuildInputVerification`（2 处） | 外部构建输入核验：manifest 声明的 checksum 与 `Cargo.lock` 同 crate@version checksum 逐字比对（两侧 registry 前缀归一化） | 来源缺口按"登记外部输入"解决，**不放宽**校验 | 单独探针实测 `verified:true` |
| P3 | `scripts/lib/build-identity.ps1` | `external_inputs_verified`（2 处） | 收据新增 `external_inputs[]`／`external_inputs_verified`／`source_snapshot_verified`（措辞区分"已验证"与"完整"） | 不误导为"源码快照含全部二进制来源" | 报告实测三字段 |
| P4 | `config/package-manifest.json` | `external_build_inputs`（1 处） | 登记 `cargo-registry:webview2-com-sys`：registry／crate／version／checksum／artifact 映射／checksum 权威文件 | 第三方 registry 产物有据可查 | 报告 `external_inputs` 实测 |
| P5 | `.gitattributes` | `eol=lf` 规则 | **恢复既有原文**（钉 app.js/styles.css）并追加 `*.rs`/`*.ps1`/`*.json`/`*.md`/`*.toml` 为 LF ＋二进制标记 | 本机 `core.autocrlf=true`，而 `pipe.rs` 的文本钉住断言把 LF 当契约 | `git diff .gitattributes` 可见追加段（**含一次覆盖事故的说明，见 §5**） |
| P6 | 产物 | `dist/CoolzhuAgent-0.2.15.msi` → `0.2.16.msi` | 两次构建均 `release_eligible=true`、六项一致性条件全 PASS | 可发布包与安装 | `msiexec exit=0`；注册表 0.2.15→0.2.16 |

## 5. 未完成 / 未验证（**不得当作已完成**）

| 项 | 现状 | 出处 |
| --- | --- | --- |
| guard 容量记账用例**一次偶发失败**（`abandoned_reservations…`，失败值 `(4,4)`） | 全量 3/3 绿、单跑 3/3 绿，**无法复现**；按四栏登记"归因待核"；首跑日志 `tmp/guard-first-failure.log`。**不能当纯测试噪声**：那段正是保证"放弃的预留不泄漏/不重复计数"的生产逻辑 | §B-73 |
| launcher 链路与输入安全启动路径的**联动观察** | 安装后测试显示 `opening=SkippedRootNotInjected`（因我直接运行）；由 launcher（已注入根）启动后会怎样**未观察** | §B-72、§B-76 |
| MSI **卸载／修复／重装** | 未测 | §B-72 |
| 从 0.2.15 升级到 0.2.16 的**用户数据兼容**（会话库 v23、输入安全库首建） | 未测 | §B-72 |
| **签名** | 包为 `unsigned`；本轮不涉及 | §B-72 |
| RD4-03 剩余 | R3（撤销未激活许可）、R4（跨进程执行者身份核查）、Goal 锚点真实关系、真实子进程崩溃变体 | §B-69 |
| Phase 3／4／5 | CU 释放义务事实修复（**新 P0**）、`ExecutionOutcome`、`ActionScope`、Paint 事实链（CU-01/02 P0、CU-03/04/05 P1）、PKG-L07c-RACE、PowerShell fail-closed | §B-63、第八轮 §7–§11 |
| **GitHub PR** | 未提交：仓库**无远端**、`gh` **未安装**、环境**无 token**；本地已有分支 `rd4-input-safety-and-pkg-integrity` 与 5 个提交 | §B-74 |
| `.gitattributes` **覆盖事故** | 我未先查看即覆盖了已存在的该文件；原文未跟踪、无法从 git 恢复，已用 `tmp/2026-09-19-agent-fixes/pr-checkout/` 的旧副本恢复并追加。**旧副本未必是最新版**，若有其它规则需你确认 | §B-74 |

## 6. 复现与核验命令（本文件"锚点"的验证方式）

```bash
# 前端
grep -c "system-attribution" modules/gui-web/packages/web-console/index.html
grep -c "refreshAttributionAndRecovery\|describeAttributionAndRecovery\|attributionDetailText" modules/gui-web/packages/web-console/src/app.js
# 后端
grep -c "FrozenParentContext\|validate_frozen_parent_relations\|canonical_workspace_identity" modules/gui-web/packages/web-console/src/main.rs
grep -c "require_resource_accepts_new_input\|physical_input_resource_scope" modules/gui-web/packages/web-console/src/computer_use_executor.rs
grep -c "CapacityFault\|PIPE_CAPACITY_FAULT" modules/gui-web/packages/windows-process-guard/src/pipe.rs
grep -c "select_holder_by_recency" packages/app-launcher/src/launch_paths.rs
# 打包
grep -c "PKG-ROOT-BINDING" scripts/build-msi.ps1
grep -c "external_build_inputs" config/package-manifest.json
# 门禁
bash tmp/gates/final-gate.sh
```

**本轮提交（分支 `rd4-input-safety-and-pkg-integrity`）**：`965166c`（输入安全事实链＋PKG 完整性＋安装 0.2.15）、`3f07b28`（台账）、`a1224f3`（§B-75 图标根因）、`46a8764`（启动器修复＋测试）、`04756db`（§B-76 修复验证）。**未推送**。
