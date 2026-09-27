# PKG-L07c 固定代次语义补强 + RD4-06 准备/冻结分离（第七轮裁决 §3）

- **日期**：2026-09-25
- **权威口径**：`docs/analysis/2026-09-21-integration-review/round7-rulings-and-gates.md` §3.1 / §3.2 / §3.3
- **工单**：两个单元**串行合并**、同一脚本集成负责人：① PKG-L07c 补强（发布完整性 **P0**）；② RD4-06 准备/冻结分离。
- **改动面**：`scripts/**`（`package-all.ps1`、`lib/webview2-loader.ps1`、`lib/build-identity.ps1`、三个测试脚本）、`config/package-manifest.json`、`docs/**`。
  **未触碰**任何 `*.rs` / `*.js` / `*.css` / `*.html`，未改 Git 状态，未改写真实 `package/`（其 mtime 仍为 2026-09-19）。

## 0. 一句话

把"当前有效代次"从"最新收据文件"改成**槽位指针 + 不可变代次档案**，让发布权覆盖**完整发布临界区**（产物最终核对 + 收据生成与提交 + 当前有效代次发布），消费端走**固定流程**（读资格 → 明确 generation → 核对 → 独立 staging → 固定消费收据 → 释放 → 报告只从固定收据生成）；同时把构建拆成**准备阶段（冻结输入）**与**正式构建阶段（只消费冻结输入）**，Cargo 正式入口用 `--locked --offline`，诊断显著列出**实际改变的路径 / 前后摘要 / 发生阶段**与 `release_eligible=false`。

## 1. §3.2 落点（固定代次语义 + 完整发布临界区）

| 要求 | 落点 | 实现方式 |
| --- | --- | --- |
| 发布权覆盖完整临界区 | `scripts/lib/webview2-loader.ps1:1526`（发布临界区注释）、`1620..1700`（`Export-LoaderArtifact` 内部） | 同一把排他发布权内依次完成：产物 staged 复核 + `Move` + 发布后复核 → 代次档案（不可变）→ 声明收据 → **当前有效代次指针**；每次共享槽位写入前都 `Assert-LoaderPublishRight`（写前复核与写入处于同一排他保护下） |
| 当前有效代次的发布 | `webview2-loader.ps1:867` `Publish-LoaderGeneration` | 代次档案目录 `.loader-generations-<slot_key>/<generation>.json`（独占创建、不可变）+ 指针 `.loader-current-<slot_key>.json`（唯一指定当前有效代次，最后一次写入） |
| 失败竞争者只能写自己的诊断 | `webview2-loader.ps1` 的 `Export-LoaderArtifact` 失败路径 | 失败即不写声明收据、不写指针；竞争者 Busy/被杀前不触碰赢家的锁/暂存/产物/收据（既有 `Enter/Exit-LoaderPublishSlot` + 新增断言） |
| `slot_key` | `webview2-loader.ps1:380..415` `Get-LoaderPublishSlot` | 规范槽位目录 + 目标文件名 + package target + profile 的 SHA256 前 16 位；同一物理槽位的竞争者必然一致 |
| `generation` | 同上 + `Publish-LoaderGeneration` | 每次成功发布一个新 GUID（**不用 mtime**）：相同字节的两次发布也有不同代次 |
| `producer_run_id` / `consumer_run_id` | `webview2-loader.ps1` 收据 `publication.producer_run_id` + 消费收据 `consumer_run_id`；`package-all.ps1:615` `New-LoaderExportSummary` | 产出方由调用方（打包运行 ID）提供，否则导出器生成并标注 `producer_run_id_source`；消费方必须是本次运行（`Register-LoaderConsumption` 硬校验） |
| `artifact_digest` / `receipt_digest` / `build_input_digest` | `webview2-loader.ps1:177/199/209` + 收据 `publication` | 产物最终核对后的内容身份；收据自身规范摘要（排除 `publication.receipt_digest` 自身，任何一方可重算）；构建输入身份摘要（artifact 声明 + 构建入口源码/配置 + manifest/lockfile + target/profile/cargo target-dir/release version） |
| 消费端固定流程 | `webview2-loader.ps1:2182` `Use-LoaderExportGeneration` | 取得读取资格 → 读指针取**明确 generation** → 三方核对（指针/代次档案/声明收据）+ 产物内容身份 → 复制到本次打包的**独立 staging** → 核对复制内容 → 固定消费收据 → 释放读取权 |
| 禁止报告生成时读最新收据 | `package-all.ps1:684` `Resolve-ArtifactSource`、`701` `Invoke-ArtifactExport`、`1294` 一致性硬检查 | 打包只从**固定消费收据指向的 staging** 取源；`exported_artifacts[]` 只由固定收据构造；报告阶段不再读共享槽位 |
| 代次收据可追溯保留 | 代次档案目录 | 每个成功发布的代次各自留一份不可变收据 ⇒ 消费 G1 后槽位更新到 G2，G1 报告仍可核对 |
| 失败运行不得进 `exported_artifacts[]` | `package-all.ps1` 失败路径 | 失败诊断只登记 `exported_artifacts_observed`；成功报告的 `exported_artifacts` 只包含本次成功消费的代次 |
| 授权改动 `exported_artifacts[]` | `package-all.ps1:1761` | 新增 `slot_key / generation / producer_run_id / consumer_run_id / artifact_digest / receipt_digest / build_input_digest` + `consumption{}` 精确消费关联 |

## 2. §3.3 验收（7 项 + 真实双进程屏障）与实证

用例全部落在 `scripts/test-package-webview2-loader.ps1` 的 L07d 段，并**接入并发套件的轮循环**（`-OnlyConcurrency -ConcurrencyRounds N` 按固定轮数重复，每轮真实进程 + 命名事件屏障）：

| 验收项 | 用例 | 关键断言 |
| --- | --- | --- |
| ① 失败方不能覆盖赢家收据 | `L07d-loser-has-no-write-access-to-valid-generation` | 赢家停在"收据已提交、指针未发布"窗口（真实进程 + 屏障）；竞争者 `EXPORT-SLOT-BUSY`；赢家的锁/产物/收据/指针/前一有效代次档案字节不变；事后注入"同 generation 不同内容收据" ⇒ `GENERATION-CONFLICT` |
| ② 产物提交与收据提交之间退出 ⇒ 拒绝不完整代次 | `L07c-incomplete-generation-rejected`（产物→收据之间，既存）+ `L07d-exit-between-receipt-and-pointer`（**新增**：收据→指针之间） | exit=97；指针字节不变（失败运行不发布当前有效代次）；消费者 `RECEIPT-GENERATION-MISMATCH`；后续发布者回收锁并发布一致的新代次 |
| ③ 消费 G1 后发布 G2，报告仍精确绑定 G1 | `L07d-consume-g1-then-publish-g2` | 指针确实前进到 G2（`supersedes_generation=G1`）；G1 档案字节不变、`receipt_digest` 可由 G1 档案重算；G1 staging 字节 = G1 `artifact_digest` |
| ④ generation 正确但内容错配仍拒绝 | `L07d-generation-correct-content-mismatch` | 档案被改写 ⇒ `GENERATION-CONFLICT`；三方自洽但产物被替换 ⇒ `CONTENT-MISMATCH`（带 `incomplete_generation_or_tampered=true`） |
| ⑤ 同一 generation 对应不同收据内容判冲突 | `L07d-same-generation-different-receipt-content` | 只改非身份类字段 ⇒ `GENERATION-CONFLICT`，且指针与代次档案保持原样 |
| ⑥ 失败报告不能被 `--no-build` 当作有效发布收据 | `test-package-build-identity.ps1` `I14a/I14b/I14c` + `I13h/I13i/I13r` | 失败运行不产成功报告、只产失败诊断；`--no-build` 命中禁用内容身份 ⇒ `[ARTIFACT-REVOKED]`；`Protect` 拒绝失败诊断 |
| ⑦ 不只断言字段存在，还要核对与实际消费内容一致 | `L07d-consumption-fields-match-consumed-content` + `L07d-package-report-generation-binding` | `artifact_digest == staging 实际字节 == 包内字节`；`receipt_digest` 从档案重算；`build_input_digest` 从收据内嵌身份重算；`slot_key` 按 manifest 重算；`consumer_run_id == 本次运行` |

**真跑结果（全部原始输出在 `tmp/verify-round7/`）**

- `test-package-webview2-loader.ps1 -ConcurrencyRounds 5`（仅并发）：**cases=15 / failed_cases=0，75 个用例轮全绿**（`tmp/verify-round7/loader-concurrency-5rounds.log`，明细 `tmp/package-webview2-loader-concurrency/20260925-064620-59b6c1ff/summary.json`）。
- 全量 loader 契约：`pass=37 fail=0 skip=4`（`tmp/verify-round7/loader-final-3rounds.log`，EXIT=0）。
- `test-package-build-identity.ps1`：**78 cases / 0 failed**（EXIT=0，`tmp/verify-round7/identity-final.log`）。
- `test-package-manifest.ps1`：PASS（EXIT=0）；`test-package-safety.ps1`：PASS（EXIT=0）；`test-powershell-script-compat.ps1`：PASS（EXIT=0）。

**实现过程中的真实失败（保留原始日志，不是"改到绿"）**

- `tmp/verify-round7/loader-concurrency-r1.log`：`[String] does not contain a method named 'ToArray'`（摘要展开把单元素 List 退化成字符串）⇒ 修 `Get-LoaderCanonicalDigest`。
- `tmp/verify-round7/loader-concurrency-r1b.log`：声明收据目录未创建 ⇒ `Move ... Could not find a part of the path`。
- `tmp/verify-round7/identity-rd406-r1.log`：`I15b` 准备阶段误建包根 `bin/`、`I15b2` 字段名不一致（`no_locked` vs `no_whitelist`）、`I15e2` 构建阶段未重算生成式配置摘要 ⇒ 分别修复（准备阶段不再触碰包根；统一字段名；构建阶段重算 frozen 生成物摘要）。
- `tmp/verify-round7/safety-test.log`：报告泄漏绝对路径（`preparation_and_freeze.freeze.absolute_path`）⇒ 新增 `ConvertTo-PackageReportFreezeReference` 投影，并把该函数移到 `try` 之前（早期失败的诊断路径也能用）。
- `tmp/verify-round7/compat-test.log`：`test-package-webview2-loader.ps1`/`test-package-manifest.ps1` 丢失 UTF-8 BOM ⇒ 已恢复（非 ASCII 的 PowerShell 源在 5.1 下必须有 BOM）。

## 3. §3.1 落点（RD4-06 准备/冻结分离）

| 要求 | 落点 | 实现方式 |
| --- | --- | --- |
| 不设构建中变更白名单 | `config/package-manifest.json` `release_policy.preparation_phase.cargo_lock_semantics.no_whitelist` | 政策文本 + manifest 测试断言 |
| 准备阶段：解析/更新依赖 | `package-all.ps1:1011` 起 `-Prepare` 分支 | 默认 `cargo metadata --format-version 1 --locked --offline`（只读核对，`exit_code` 入准备记录）；确需更新锁定内容必须显式 `-UpdateDependencies` |
| 准备阶段：生成必要配置 | `package-all.ps1` `prepare-generate-configs` 段 | 逐步骤执行 `preparation.steps`，`generates[]` 声明生成物；生成物**计入冻结输入**（摘要进 `freeze_id`） |
| 展示变化并按政策确认 | `package-all.ps1` `prepare-change-review` 段 | 与上一次**同一 manifest** 的冻结记录逐项比较（不同 manifest 视为"无历史"）；有变化且未 `-AcceptPreparationChanges` ⇒ `[PREPARE-INPUT-CHANGED]` 拒绝 |
| 冻结完整输入快照 | `build-identity.ps1:2267/2335` | 内容寻址、一次性写入 `tmp/package-freeze/package-input-freeze-<config>-<freeze_id>.json` + 指针 `latest-<config>.json` |
| 正式构建阶段只消费冻结输入 | `package-all.ps1:1211` 起 | `-FreezeRecordPath` 时逐项核对（文件摘要 + 非文件描述符 + 准备阶段生成的配置）；不一致 ⇒ `[FROZEN-INPUT-CHANGED]`；未显式给出则在构建开始处自冻结并如实记为 `self-frozen-at-build-start` |
| Cargo 锁定语义 | `config/package-manifest.json` 9 个 cargo artifact 的 `build.args` | 全部 `--locked` + 既有 `--offline`；manifest 测试断言两者都在 |
| 生成式配置不得事后移出清单 | `build-identity.ps1:2414` `Compare-PackageInputFreeze` + 冻结记录的 `input_set_mutation_policy` | 冻结清单不可变；改写在构建阶段以 `classification=preparation-generated-config` 命中并拒绝 |
| 诊断列出改变路径/前后摘要/发生阶段 | `build-identity.ps1:2239/2563` + `build-identity.ps1:1997..2003` | `changed_paths[] = {path, kind, before_summary, after_summary, phase, classification, evidence_kind}`；`run_phase` 区分 prepare/build；失败诊断 `release_eligible=false` |
| 派生输出位置 | `config/package-manifest.json` `preparation.derived_outputs` + `package-all.ps1:259` | 逐条声明 path/generated_by/reason（含导出槽位、消费 staging、冻结记录、gen/target） |

**RD4-06 实证**：`I15a..I15f2`（身份测试内）覆盖"准备冻结 / 不构建不动包根 / 构建消费冻结记录 / 冻结输入改变拒绝 + 诊断含前后摘要与阶段 / 生成式配置计入冻结输入且改写即拒绝 / 准备阶段变化必须显式确认"。`package-all.ps1` 真实演示（`tmp/verify-round7/verify-package-all-v2.ps1`，输出 `tmp/verify-round7/package-demo/summary.json`）：

- C 段：准备阶段冻结（`freeze_id=df5882…`）→ 输入改变 → `[FROZEN-INPUT-CHANGED]`（`failure_stage=build-frozen-input-check`），成功报告与 latest 指针**字节不变**，诊断含 `phase=build-preflight` + 前后摘要。
- D 段：重新准备（`changes_accepted=true`）→ 构建期间输入改变 → `[BUILD-INPUT-CHANGED]`（`phase=build-input-recheck`），暂存产物 **2 个**移入 `tmp/package-failures/<run>/payload`（`auto_promotion=never`），包根为空，`signing_or_distribution=not-attempted`。
- E 段：稳定输入（无导出物的夹具）⇒ 构建模式与 `--no-build` 都出**可发布收据**（`release_eligible=true`，六门全 pass，含新增 `frozen_inputs`）。

## 4. 未闭环 / 需要操作侧知道

1. **收据 schema 升到 3**：schema 1/2 的旧导出记录一律拒绝 ⇒ 正式发布需要**重新构建一次导出**（有意为之，与 B-57 从 1 升 2 同一口径）。
2. **导出物 provenance 与"声明源是否落在源码快照范围"**：构建模式下导出物要确认来源，除"生产者本次真重跑"外还要求其声明源落在声明快照范围内；演示夹具把稳定导出路径放在快照 roots 之外，因此该产物仍是 `not-confirmed`（既有政策，本次未改）——这也是 A/B 段 `release_eligible=false` 的唯一原因。
3. **`--no-build` 与共享禁用记录**：失败运行登记的禁用内容身份会跨运行保留；同一夹具再次运行时 `--no-build` 会被 `[ARTIFACT-REVOKED]` 拒绝（fail-closed，设计如此）。演示因此不是幂等的：干净首跑 B 段成功（`tmp/verify-round7/package-all-demo-v8.log`），其后各轮被禁用记录拒绝（`package-all-demo-final2.log`）。
4. **未复现"具体交错"这一点仍未变**：本轮**没有**经验性地构造出"两个真实发布者在同一瞬间各自完成产物+收据写入并互相覆盖"的原始交错（与 B-57 同）；本轮做的是：把该交错**在协议上变成不可能**（指针 + 不可变档案 + 写前复核 + 独占创建），并用真实双进程屏障验证窗口内的各条边界。P0 关闭与否请按裁决口径判定。
5. **准备阶段的"冻结输入"仍不是不可变快照副本**：`immutable_build_snapshot=false` 如实保留；稳定性证据依旧是"构建前后两次独立采样逐项一致"。
6. **准备阶段的依赖解析是只读核对**：`-UpdateDependencies` 会允许解析并更新锁定内容（可能改写 `Cargo.lock`），本轮**未在验证中执行过它**，因此"准备阶段真的更新锁文件"这一段未被实测。
7. 未跑含真实全量编译的打包（工单限制 + 并发编辑），"从源码重新编译 → 出包"这段仍未验证。

## 5. 证据索引（`tmp/verify-round7/`）

- `loader-concurrency-5rounds.log`（15 用例 × 5 轮，0 失败）
- `loader-final-3rounds.log`（全量 `pass=37 fail=0 skip=4`）
- `identity-final.log`（78 cases / 0 failed）
- `manifest-test2.log`、`safety-test3.log`、`compat-test3.log`（均 PASS）
- `package-all-demo-final2.log` + `package-demo/summary.json`（A–E 五段演示）
- `package-all-demo-v8.log`（B 段干净首跑）
- 实现期失败日志：`loader-concurrency-r1.log`、`loader-concurrency-r1b.log`、`identity-rd406-r1.log`、`safety-test.log`、`compat-test.log`
