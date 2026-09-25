# 构建身份分离与报告治理（RD4-06 / 第五轮裁决 B-1、B-2 执行记录）

用途：记录"源码快照 / 构建输入 / 载荷"三个身份的实际口径、落点、验证方式与未闭环项。
配套：`packaging-webview2-loader-export-protocol.md`（PKG-01/02 导出协议）、
`packaging-loader-and-runtime-dir.md`（运行时目录）、`rpr-execution-blockers.md`（执行台账）。
口径：**已核实源码**标注 `file:line` 或函数名；实测标注来源。

---

## 0. 一句话结论

**历史来源可以未知，当前使用哪些字节必须可回答。**

本工作树**刻意**不进入正常跟踪模式（第五轮裁决 B-1 选 b），因此不伪造 Git 提交身份；
改用**三个分开的、可重算的身份**回答三个不同问题，并且：

- 并发编辑的活动树**不再**被"首尾各哈希一次"伪装成一致——构建前后各做一次**两遍静默枚举**，
  任何新增/删除/内容变化都以具体路径 **fail-closed**；
- 报告有**唯一 ID + 内容哈希**，被**产物清单**引用；被正式发布引用的报告受**保留规则**保护或连同发布证据归档。

---

## 1. 三个身份（互不冒充）

| 身份 | 回答的问题 | 计算方式（唯一实现点：`scripts/lib/build-identity.ps1`） | 落点 |
| --- | --- | --- | --- |
| `source_snapshot_digest` | 本次使用了哪份**第一方源码和构建资源**快照？ | `H( scope 描述符行 ‖ file_set_digest )`；`file_set_digest = H(排序后的 "file=<path>\|<length>\|<sha256>" 行)` | package report `build_identity` / `source_snapshot`；冻结记录 `tmp/source-snapshots/source-snapshot-<digest>.json` |
| `build_input_digest` | 该快照配合什么**锁文件、工具链、target、profile、features 和构建配置**？ | `H(排序后的描述符行)`，描述符由 `Get-BuildInputDescriptors` 产出 | package report `build_identity` / `build_inputs.descriptors`（逐行可审计） |
| `payload_digest` | 最终**生成和分发**了哪些二进制与资源？ | `H(暂存包根目录下除清单载体自身外的全部 "file=<path>\|<length>\|<sha256>" 行)` | 包内 `payload-inventory.json` + package report `build_identity` / `payload_inventory` |

相关函数：

- `Get-SourceSnapshotScope` / `Get-SourceSnapshotEntries` / `New-SourceSnapshot` / `Compare-SourceSnapshot`
- `Write-SourceSnapshotFreezeRecord` / `Get-BuildInputDescriptors` / `Get-BuildInputDigest`
- `Get-PayloadInventory` / `Get-PackageReportId` / `Write-GovernedPackageReport` / `Assert-PackageReportContentHash`
- `Get-VcsReferenceState`

**边界声明** `config/package-manifest.json#source_snapshot`：`roots`（16 项）+ 排除/放行/重解析点规则 +
`external_path_dependencies`。纳入判定是"**声明 roots 下除排除项之外的一切**"（可审计边界），
不是扩展名白名单；已知扩展名清单只用于产出 `unclassified_files` 可见性报告。

`scope_digest` 与 `file_set_digest` 分开登记，`source_snapshot_digest` 同时绑定两者——
**不同边界下的相同文件集不会得到同一个源码身份**。

### 1.1 范围（覆盖 + 不混入）

覆盖：第一方 Rust／C#／JS、构建脚本（`scripts/**`）、`build.rs`、编译期嵌入资源
（`include_str!` / `include_bytes!` 指向的 `.cs`/`.json`/`.html`/`.js`/`.css`/`.png`/`.py`/`Cargo.toml`/`README.md`）、
安装器定义（`installer/Product.wxs` + `docs/design-assets/**` 图标）、路径依赖（全部 workspace 成员 crate，
含 `.coolzhu/plugins/*`），以及**独立 Tauri 项目的真实构建入口与锁文件**
（`modules/gui-desktop/packages/tauri-shell/src-tauri/{Cargo.toml,Cargo.lock,build.rs,tauri.conf.json,capabilities}`）。

不混入（`SourceSnapshotDefaultExcludePatterns`，同时是 `.gitignore` 与 `package-safety` 的分类）：
用户配置、凭据、运行数据库、模型权重、第三方二进制、日志、**无关历史 `target`**、
tauri-build 生成目录 `gen/`、以及 web-console 下的 `.coolzhu` 运行态（会话/音频分片）。
`.coolzhu/plugins/**` 由 `allow_path_patterns` 显式放行（它是 workspace 成员，不是运行态）。

> 术语纠正（裁决 B-1 第 2 条）：`git ls-files` 只列举**索引中的已跟踪文件**，
> 结果为 0 **不能**证明"没有 `.git`"。准确口径写进报告：
> `vcs_state = untracked_snapshot`，`vcs_evidence` 记录 `git rev-parse HEAD` 与 `ls-files` 计数。

### 1.2 匹配基准（`match_base=root-relative`）

排除/放行规则匹配的是**相对所属声明 root 的路径**，不是仓库相对路径；`match_base` 已进入 `scope_digest`。
理由：roots 已声明范围，规则只需描述"范围内部要剔除什么"；否则任何位于被排除命名之下的工作区
（例如仓库内 `tmp/`）会整体失效，也无法用夹具验证。"roots 本身受信"，规则只作用于其内部条目。

### 1.3 重解析点（junction / symlink）

**不追随**：遍历遇到重解析点只登记（`skipped_reparse_points[]` + `handling = not-followed-registered-only`）并跳过。
需要追随时必须在 `allow_reparse_points` 显式声明。实测当前真实 scope 命中 **0** 个重解析点。
外部构建依赖（`tmp/tools/dotnet`、`tmp/tools/wix`、`modules/*/target`）在
`external_path_dependencies` 中**显式登记**（`handling = registered-only-not-followed`），不通过无边界追随 junction 解决。

---

## 2. 收据字段口径（照抄裁决，不自行改写）

| 字段 | 取值 | 说明 |
| --- | --- | --- |
| `source_commit` | `null` | **不作为当前源码权威**，可为空 |
| `source_commit_authority` | `not-authoritative` | 显式标注"提交号不是权威" |
| `vcs_reference_commit` | `baa8e3559f1c…` | 种子提交，**仅参考** |
| `vcs_reference_commit_kind` | `seed-reference-only` | |
| `vcs_state` | `untracked_snapshot` | 索引已跟踪文件数为 0 ⇒ 当前源码没有被该提交有效覆盖 |
| `dirty_against_commit` | `not_evaluable` | **不得**写成 `false`：没有被跟踪的变更无从判定 |
| `dirty_against_commit_note` | 说明文本 | 记录判据（`git status --porcelain` 行数 / 无法判定原因） |
| `tracked_index_file_count` | `0` | `git ls-files` 计数 |
| `source_snapshot_digest` | 实际计算值 | |
| `build_input_digest` | 实际计算值 | |
| `payload_digest` | 实际计算值 | |

实现是**自适应**的：一旦工作树进入正常跟踪模式（索引非空），
`vcs_state` 变为 `tracked_worktree`、`source_commit` 取 HEAD、`dirty_against_commit` 变为真实布尔值。
`Get-VcsReferenceState`（`scripts/lib/build-identity.ps1`）与 `Get-LoaderRepoHead`（`scripts/lib/webview2-loader.ps1`）两侧口径一致。

**本工单不改变 Git 状态**：只读 `rev-parse` / `ls-files` / `status --porcelain`，
没有执行、也没有把 `git init`、`git add`、提交、打标签作为自动动作。

### 2.1 收据里"身份不冒充"的显式标注

webview2-loader 收据的 `source_identity_sha256`（`source_identity_kind = declared-file-hash-set`）
只覆盖**构建入口声明的文件 + 导入目录**，因此收据新增：

```
source_identity_is_full_source_snapshot = false
source_snapshot_digest                  = null
source_snapshot_digest_available        = false
source_snapshot_note                    = 全量源码身份在 package report 的 build_identity 里
```

package report 的 `build_context.source_identity_note` 同样明确："该声明文件哈希集合只覆盖 tauri
构建入口声明的文件与导入目录，**不是**全量源码快照"。这直接回应裁决的提醒：
**manifest 声明的文件哈希是否覆盖全部源码输入尚未被证明**，因此它只作为子范围登记，
不替代源码快照清单。

---

## 3. 并发编辑：如何避免"首尾各哈希一次"的假一致

实现（`New-SourceSnapshot`，`scripts/lib/build-identity.ps1`）：

1. **构建前**：完整枚举 + 逐文件哈希，**连做两遍**，两遍的 `file_set_digest` 必须相同；
   不同即 `[SOURCE-SNAPSHOT-UNSTABLE]`，并给出 added/removed/modified 的具体路径。
   两遍一致后把清单写成**冻结记录** `tmp/source-snapshots/source-snapshot-<digest>.json`（按摘要寻址、
   同摘要复用、**拒绝覆盖**，同名不同内容即 `SOURCE-SNAPSHOT-FREEZE-COLLISION`）。
2. **构建/发布后**：再做一次两遍静默枚举，与构建前快照**逐文件比较**（`Compare-SourceSnapshot`，
   含新增与删除）。不一致即 `[SOURCE-SNAPSHOT-CHANGED]`，**不写报告**（避免留下"看似有效"的构建身份）。
3. `Passes < 2` 被 fail-closed 拒绝，防止退化成单次哈希。

两处窗口（枚举期间、构建期间）都覆盖，因此并行编辑不会被"两个端点一致"掩盖。
**实测证据（本仓库，2026-09-25）**：

```
[SOURCE-SNAPSHOT-UNSTABLE] … modified_paths=modules/computer-use/packages/computer-use-core/src/cleanup.rs
[SOURCE-SNAPSHOT-UNSTABLE] … modified_paths=modules/gui-web/packages/windows-process-guard/src/pipe.rs
[SOURCE-SNAPSHOT-UNSTABLE] … modified_paths=modules/core-runtime/packages/core-runtime/src/lib.rs
[SOURCE-SNAPSHOT-CHANGED]  … modified_paths=modules/gui-web/packages/web-console/src/computer_use_store.rs
```

（这些是**在途并发编辑**（RD4-01 等）被真实抓到的记录，不是构造的负例；确定性负例见
`scripts/test-package-build-identity.ps1` 的 I11：夹具的"构建命令"在构建期间改写源码。）

### 3.1 策略选择（需要知情的取舍）

当前策略是 **fail-closed**：源快照在构建窗口内变化 ⇒ 拒绝出报告、拒绝出正式包。
这与"不得宣称构建过程完全一致"直接对应，与仓库既有 fail-closed 文化一致；
代价是：**在并发编辑的活动树上无法完成正式打包**（这是正确行为，不是缺陷——
应改在冻结/受控不变输入集合上构建，或先停止并发编辑）。

另一个合规选项是"照常出包，但在报告里如实记 `quiescent=false` + 差异清单"，由消费方判断。
**该取舍未在裁决中明确指定**，当前实现选了更保守的一侧；若决策者希望释放"活动树打包"，
只需把 `package-all.ps1` 的 `[SOURCE-SNAPSHOT-CHANGED]` 分支改为"记录不阻断"，无需改动身份口径。

---

## 4. 报告治理（B-2 三项）

### 4.1 落点（沿用既有，未改名、未搬迁）

```
tmp/package-reports/package-report-<config>-<stamp>.json      # package report（沿用既有命名）
tmp/package-reports/latest-<config>.json                      # latest 指针（同目录）
tmp/package-reports/retention-index.json                      # 保留索引
tmp/source-snapshots/source-snapshot-<digest>.json            # 源码快照冻结记录
<package_root>/payload-inventory.json                         # 载荷清单（写进包内，随分发出厂）
docs/testing/release-<version>/evidence/build-identity/<report_id>/…  # 发布证据归档
```

裁决提到的 `build/package-report.json` 是**对本仓库落点的不准确假设**，
本轮**没有**为迎合该措辞搬迁报告或改动既有证据链接（`tmp/package-reports/` 原样保留）。

### 4.2 唯一 ID + 内容哈希

- `report_identity.report_id = pkg-report-<config>-<stamp>-<8hex>`（文件名与 ID 共用同一时间戳）。
- `report_identity.content_sha256` = **规范叶子路径排序后**的 SHA256，范围是整份报告**除
  `report_identity.content_sha256` 自身**（否则是自引用固定点）。
- 关键：哈希是在"**序列化再解析**"的归一化文档上算的（`Get-NormalizedPackageReportHash`）——
  在生产方内存对象上算会让验证方（读文件→重算）得到不同值，`Assert-PackageReportContentHash`
  会当场发现（本轮开发中就被 `[REPORT-CONTENT-DRIFT]` 守卫抓到过一次重复定义）。

### 4.3 引用链（报告 ↔ 清单 ↔ installer report）

- 产物清单（`<package_root>/payload-inventory.json`）引用报告：`report_ref.{report_id, report_path, content_sha256, content_hash_scope}`。
- 报告引用清单：`payload_inventory.{path, payload_digest, file_count, total_bytes}`（不构成环：
  清单载体自身被排除在 `payload_digest` 之外，报告的内容哈希不依赖清单字节）。
- `build-msi.ps1` 的 installer report 新增：`package_report_ref`（ID + 内容哈希 + 文件哈希 +
  报告路径 + `captured_from` + `verified`）、`payload_inventory_ref`（路径 + 文件哈希 + `payload_digest`）、
  `build_identity`（三身份 + MSI 自身 `msi_payload_sha256`）、`vcs`。
  **拿不到报告引用时 fail-closed**（`[REPORT-REF-MISSING]`，不允许生成无法回溯的正式包）。

### 4.4 保留规则（落在哪里）

`scripts/package-report-retention.ps1`（`-Action Verify|Protect|Prune|List`）：

- `Protect`：登记 `report_id` + 内容哈希 + 文件字节哈希 + 载荷清单哈希到
  `tmp/package-reports/retention-index.json`；`-Archive` 时把报告 + 载荷清单 + 其它证据
  （`-ExtraEvidencePath`，例如 installer report）复制到
  `docs/testing/release-<version>/evidence/build-identity/<report_id>/` 并写归档清单。
  治理前的历史报告（无 `report_identity`）**不伪造哈希**，以"文件字节哈希"口径登记并显式标注。
- `Verify`：逐个核对副本存在性、文件字节哈希、以及**内容哈希可重算**；tmp 下原报告被清理时
  自动回退到归档副本。
- `Prune`：**只删除未被索引保护的报告**；索引缺失/损坏 ⇒ **拒绝清理**（fail-closed）；
  即使索引丢失，`docs/testing/release-*/evidence/build-identity/**` 里的归档清单也构成第二重保护。
- `build-msi.ps1` 在写出 MSI 后自动 `-Action Protect -Archive`（把 installer report 一并归档）。
- 已执行：为 `tmp/package-reports/` 里**既有的 3 份**发布轮次报告建立保护条目（就地保护、不搬迁；
  版本号与报告的对应**未做推断**，故未归档到具体版本目录）。
  `Verify` 实测：`PASS package-report-retention verify: 3 protected report(s) verified`。

---

## 5. 实测（本仓库，2026-09-25）

真实 manifest、`-Configuration release -SkipBuild`、暂存到 `tmp/rd4-06-package-release`（不动真实 `package/`）：

```
source snapshot: b9d3250b352ca8d905cf16803884ff9903e53e1cc629e66b56da8de08ff50217 files=1138 bytes=195670373
source snapshot verified unchanged: b9d3250b…
payload identity: files=854 bytes=306694171 digest=3664ceb6…
package report: tmp/package-reports/package-report-release-20260925-084033575.json
              (report_id=pkg-report-release-20260925-084033575-470d4bcd
               content_sha256=98be85b6bb29116b3d5c1a01b38bc648b3360c4ff536cc249d0f0c6c6c483111)
```

三身份实测取值：

```
source_snapshot_digest = b9d3250b352ca8d905cf16803884ff9903e53e1cc629e66b56da8de08ff50217
build_input_digest     = eb193730f1625f025084d269c0911000cf43696d6f66cad425c8324bb18377e1
payload_digest         = 3664ceb6acdcf8e63490a49fbee406de608660ab9cb3f5ad898badfcb56ccc9d
vcs_state              = untracked_snapshot
vcs_reference_commit   = baa8e3559f1c73d7364e34c1f7597c73ed49d601
dirty_against_commit   = not_evaluable
```

真实 scope 扫描结果：1138 个文件、**0** 个"禁止混入"命中、**0** 个重解析点、**0** 个未分类扩展名。
契约测试 `scripts/test-package-build-identity.ps1`：**36 个用例全部 PASS**（含增删改差异实验、
三身份独立变化、收据口径、全流程报告/清单/指针、构建期改写源码 fail-closed、保留规则正负例）。

### 5.1 MSI 段（`build-msi.ps1`）实测通过

`build-msi.ps1 -Version 0.2.14 -Configuration release -PackageRoot tmp/rd4-06-package-release -SkipPackageBuild`：

```
package report referenced: report_id=pkg-report-release-20260925-084033575-470d4bcd content_sha256=98be85b6…
staged export verified: gui-desktop.webview2-loader -> bin/WebView2Loader.dll sha256=8427b1fc… (reused-from-existing-cargo-build-directory)
archive package report evidence: docs/testing/release-0.2.14/evidence/build-identity/pkg-report-release-20260925-084033575-470d4bcd
protected package report: report_id=pkg-report-release-20260925-084033575-470d4bcd
msi = dist/CoolzhuAgent-0.2.14-20260925-084930.msi  sha256=3FC65B58…  wix_version=5.0.2+aa65968c  signed=false
```

installer report 落盘 `dist/CoolzhuAgent-0.2.14-installer-report.json`，其中
`package_report_ref.verified = true`、`captured_from = latest-pointer-in-repo`（本次用 `-SkipPackageBuild`）、
`build_identity.{source_snapshot_digest, build_input_digest, payload_digest, msi_payload_sha256}` 齐全，
`vcs.dirty_against_commit = not_evaluable`。

**fail-closed 也被实测**：不带 `-Configuration release` 时（默认 debug，`tmp/package-reports/latest-debug.json` 不存在）

```
[REPORT-REF-MISSING] profile=debug
detail: 找不到可引用的 package report（既没有本次 package-all 的返回值，也没有 tmp\package-reports\latest-debug.json）
next: 先执行 scripts/package-all.ps1（不要用 -SkipPackageBuild 跳过）再生成 MSI；不要把没有构建身份的证据打包发布
```

归档目录与保留索引：

```
docs/testing/release-0.2.14/evidence/build-identity/pkg-report-release-20260925-084033575-470d4bcd/
  package-report.json / payload-inventory.json / package-report-files.json / CoolzhuAgent-0.2.14-installer-report.json
tmp/package-reports/retention-index.json  → 4 entries（3 份历史发布轮次报告 + 本次报告）
scripts/package-report-retention.ps1 -Action Verify → PASS … 4 protected report(s) verified
```

> 本次用于验证的 MSI 是 `dist/CoolzhuAgent-0.2.14-20260925-084930.msi`（时间戳命名，**未覆盖**
> canonical `dist/CoolzhuAgent-0.2.14.msi`），其载荷来自 `tmp/rd4-06-package-release` 暂存目录，
> 因此它是**验证产物**而非新的正式发布；如需清理，应与归档目录和保留索引条目一并处理。

验证命令：

```powershell
powershell -NoProfile -File scripts/test-package-build-identity.ps1   # 36 cases
powershell -NoProfile -File scripts/test-package-manifest.ps1         # manifest 声明与排除规则
powershell -NoProfile -File scripts/test-package-webview2-loader.ps1  # PKG-01/02 导出协议（23 PASS）
powershell -NoProfile -File scripts/test-package-safety.ps1
powershell -NoProfile -File scripts/test-powershell-script-compat.ps1
powershell -NoProfile -File scripts/package-report-retention.ps1 -Action Verify
```

## 6. 未闭环 / 与裁决前提不符之处

1. **`COOLZHU_GIT_SHA` 语义**：`build-msi.ps1` 仍把它设为 `git rev-parse HEAD`（种子提交）。
   报告里已显式标注它"只是种子参考提交，不是源码权威"（`vcs.note`），但**编译进 CLI 的字符串本身**
   仍是那 40 位提交号——若要彻底消除误导，需要另立工单改动 `build-msi.ps1` + CLI 的版本输出契约。
2. **并发策略的取舍未由裁决指定**：见 §3.1，当前取 fail-closed。
3. **`dirty_against_commit` 的 `tracked_worktree` 分支未实测**：本环境不允许执行
   `git init` / 提交（裁决 B-1 第 1 条），因此该分支只有代码与口径，没有实测证据。
4. **源码清单是否覆盖"全部源码输入"仍是声明 + 边界**：边界是"声明 roots 下除排除项之外的一切"，
   可审计但不等于数学完备——若将来有构建输入落在声明 roots 之外（例如新的外部路径依赖），
   必须同步更新 `source_snapshot.roots` / `external_path_dependencies`，否则它不会进入快照。
   当前 `docs/design-assets`、`tests/fixtures`、`docs/user-guide`、`.coolzhu/plugins` 都已显式列入。
5. **模型权重/第三方二进制不在源码快照内**：它们的身份只在 `payload_digest` 中体现。
   如果将来要把某个权重/工具纳入"源码级身份"，需要显式加入 roots 并放宽排除规则。
6. **含真实编译的全量打包仍未跑通**：`build-msi` 验证用的是 `-SkipPackageBuild`（消费既有 release
   产物与收据）+ 真实 WiX 出 MSI；"从源码重新编译再打包"这一段仍受并发在途编辑与
   `coolzhu-computer-use-core` 编译问题制约（见 `rpr-execution-blockers.md`）。
7. **`scripts/lib/webview2-loader.ps1` 的并发导出存在既有 flake（非本工单引入）**：
   `Move-LoaderFileIntoPlace` 用 `File.Replace`，两个进程同时替换同一目标时会抛出
   "无法将替换文件移到要被替换的文件"。实测：`test-package-webview2-loader.ps1` 的
   `L07c-concurrent-export` 在 4 次运行中失败 1 次、随后连续 3 次全绿（23 PASS）。
   该用例只调用 `Copy-LoaderVerifiedFile`，与本工单改动的代码无关；建议另立工单为该原语加重试/互斥。

---

# 附录 A：失败诊断与发布资格分离（RD4-06 增补，第六轮裁决 §三）

- **权威口径**：`docs/analysis/2026-09-21-integration-review/a2-workspace-source-and-frozen-parent-context.md` 第 9 节第一项
  （「打包政策（第六轮 §三）」）。本附录是该口径的实施记录，引用时仍以上述文件为准。
- **一句话**：维持**拒绝发布**不稳定输入所生成的包；把「拒绝发布」与「输出失败诊断」**分开**——
  成功报告与可发布收据继续不发，失败诊断必须照发。

## A.1 逐条落点

| 裁决要点 | 实现落点 |
| --- | --- |
| ① 维持拒绝发布；不允许用 `quiescent=false` 补可发布收据 | 成功路径仍在 `[SOURCE-SNAPSHOT-CHANGED]` / `[BUILD-INPUT-CHANGED]` 处 fail-closed；`release_eligibility.refusal_note` 显式否定该做法；报告新增 `release_eligible` / `release_eligibility.release_eligible`，`scripts/package-all.ps1` |
| ② 必须输出失败诊断（不拒绝诊断） | `scripts/package-all.ps1` 主体包在 `try/catch` 内；catch 分支写失败诊断到**独立落点** `<reportParent>/failures/package-report-failure-<config>-<stamp>.json`，因此 `-ReportPath` 指向的成功报告落点保持"失败即无报告" |
| ③ 诊断最小字段集 | `New-PackageFailureDiagnostic`（`scripts/lib/build-identity.ps1`）：`run_id` / `failure{stage,category,message,detail_lines}` / `stage_exit_codes[]` / `declared_input_scope` / `observed_input_changes` / `produced_not_released` / `release_eligible=false` |
| ④ 差异标为"已观察变化" | `observed_input_changes.evidence_kind = 'observed-between-two-samples'` + `is_complete_write_history = false` + 说明文字 |
| ⑤ 失败后不得覆盖成功报告 / 不更新最新有效包 / 不进签名安装分发；临时产物分区且不自动晋升 | 失败路径没有任何写 `latest-<config>.json` 的代码；失败前后各读一次指针状态并在 `latest-run-<config>.json` 台账里登记 `latest_valid_package_pointer_updated_by_this_run=false`；`Move-PackageStagingToQuarantine` 把本次运行的暂存产物移入 `tmp/package-failures/<run-id>/payload`（包根随即为空，`auto_promotion='never'`）；`signing_or_distribution='not-attempted'` |
| ⑥ 状态分开表达 | `New-PackageReleaseEligibility`：`live_worktree_changed`（三态）/ `build_snapshot_integrity` / `build_input_digest` / `validation_snapshot_digest` / `release_eligible` + 逐门 `gates[]`（pass/fail/not-confirmed） |
| ⑦ 允许"不可变快照 + 活动树继续编辑"，不允许"直接构建且输入变化" | 本实现**不从快照副本构建**，故如实记 `immutable_build_snapshot=false` 并只在"构建前后两次独立采样逐文件一致"时给 `verified-unchanged-live-tree`；输入范围无法确定时（未声明 `source_snapshot` / 声明 root 缺失 / 构建输入重算失败）一律 `not-confirmed` → `release_eligible=false` |
| ⑧ 已声明不属于构建输入的日志/临时输出变化不得误判为源码变化 | `release_policy.non_build_input_paths`（`config/package-manifest.json`）+ `Get-PackageNonBuildInputDeclarations` / `Get-PackagePathClassification`：逐路径给出 `in-declared-source-snapshot-scope` / `declared-source-snapshot-excluded` / `declared-build-input` / `declared-non-build-input` / `outside-declared-scope`，并把分类结果写进诊断 |
| ⑨ `--no-build` 必须消费匹配快照/配置/产物的有效收据 | 导出物仍走 `Assert-LoaderExportReceipt -VerifyIdentity`；此外新增产物—快照关联记录（`tmp/package-reports/artifact-release-status.json` 的 `associations`）：`--no-build` 只有在找到"绑定到**本次**源码快照摘要 + 构建输入摘要 + 同一声明来源路径/内容身份"的关联时才确认来源；缺失即 `not-confirmed`；失败运行登记的 `revocations` 在 `--no-build` 下硬拒（`[ARTIFACT-REVOKED]`） |

## A.2 消费侧门禁（失败报告不得当发布收据）

`Assert-PackageReportReleaseEligible`（`scripts/lib/build-identity.ps1`）在任何"报告 → 发布"的入口拒绝：

- `scripts/build-msi.ps1`：引用 package report 时（MSI 生成之前）；
- `scripts/package-report-retention.ps1 -Action Protect`：不得把失败诊断或 `release_eligible=false` 的报告登记成发布证据；
- `Produce` 侧：`Prune` 不删除 `failures/` 分区（按名字再兜一次）。

治理前的历史报告没有该字段时**不据此外推**（`evidence='release-eligibility-field-absent'`），既有证据链仍可登记。

## A.3 实测（2026-09-25，本仓库）

- `scripts/test-package-build-identity.ps1`：**64 cases / 0 failed**（新增 I13a–I13z、I12h、I12i；含稳定条件出可发布收据、`--no-build` 关联确认、构建输入中途变化拒绝、失败诊断最小字段、分区、不覆盖/不晋升、未声明范围不乐观放行、逐路径分类）。
- 真实清单（`config/package-manifest.json` 的声明范围，1138 文件 / 16 roots）：活动树稳定 ⇒ `release_eligible=true`；`--no-build` 消费关联 ⇒ `release_eligible=true`；构建期间真实修改一个**声明的构建输入** ⇒ `[BUILD-INPUT-CHANGED]` + 诊断 + 隔离 + 上一份成功报告与 latest 指针字节不变 + 之后 `--no-build` 被 `[ARTIFACT-REVOKED]` 拒绝。
- 未跑"含真实全量编译的打包"（与本轮并发在途编辑冲突且耗时），因此**"从源码重新编译 → 出包"这一段仍未验证**。

## A.4 未闭环 / 与裁决前提不符之处（附录）

1. **"不能把不稳定构建的裸 DLL 当作来源"的实现是内容身份禁用，不是"重放那次构建的完整输入"**：
   `revocations` 按 `artifact_id + profile + 声明来源路径 + 内容身份` 匹配，只能否决"已知不稳"的那份字节；
   字节不同（哪怕同样来自不稳定输入）不会被否决——那种情形只能靠 `not-confirmed`（`release_eligible=false`）拦住，属于"未确认"而非"已判定不可用"。
2. **`--no-build` 的"匹配快照"依赖此前有一次构建后校验通过的成功运行**：没有关联记录时状态是 `not-confirmed`
   ⇒ 报告可出、收据不发。治理前的既有产物（例如本仓库当前 `target/debug/*.exe`）都属这一类。
3. **非导出产物的禁用记录只能靠"本次重写证据"（mtime / 生产者重跑）或"来源落在已验证快照范围内"来清除**：
   若一次干净构建产出**字节完全相同**的产物且没有 mtime/重跑证据，禁用会保留（fail-closed，选择偏保守）。
4. **构建输入重算引入了一个新的硬失败面**：`cargo build` 若在构建期间改写 `Cargo.lock`（或 `tauri.conf.json` 等声明输入），
   现在会 `[BUILD-INPUT-CHANGED]` 拒绝发布。这是裁决第 7 条的直接要求（记录的身份必须对应真正用于构建的输入），
   但确实会让"锁文件在构建时被更新"的合法场景变成 fail-closed——需要产品口径确认。
5. **失败诊断的 `stage_exit_codes` 只对"真实进程阶段"有退出码**（构建阶段），非进程阶段记 `not-applicable`（不伪造 0）。
6. **签名/安装/分发"未进入"是结构性的**（失败路径直接抛错，`build-msi` 拿不到指针即 fail-closed），
   而不是靠"签名前再检查一次"；若将来新增独立发布入口，必须在同一入口调用 `Assert-PackageReportReleaseEligible`。
7. **并发编辑下的 `--no-build` 关联确认需要"两次运行之间活动树不变"**：本仓库同时有多个代理在跑，
   实测出现过一次"两次运行之间快照摘要变化 ⇒ not-confirmed"（如实记录，未放宽规则）。
