# RD4-06：源码快照与构建身份分离 + 报告治理（第五轮裁决 B-1、B-2）

日期：2026-09-25
范围：`scripts/lib/build-identity.ps1`（新增）、`scripts/package-all.ps1`、`scripts/lib/webview2-loader.ps1`、
`scripts/build-msi.ps1`、`scripts/package-report-retention.ps1`（新增）、`config/package-manifest.json`（声明）、
`scripts/test-package-build-identity.ps1`（新增）、`scripts/test-package-manifest.ps1`（扩展）、docs。
**未触碰**：任何 `*.rs`、`config/package-launcher.json`、`packages/app-launcher/**`。

## 做了什么

1. **三个分开的身份**（唯一实现点 `scripts/lib/build-identity.ps1`）
   - `source_snapshot_digest` = `H(scope 描述符 ‖ file_set_digest)`，边界由
     `config/package-manifest.json#source_snapshot` 声明（16 个 roots + 排除/放行/重解析点规则），
     纳入判定是"声明 roots 下除排除项之外的一切"。
   - `build_input_digest` = 描述符行的规范哈希：锁文件（**含独立 Tauri 项目自己的 `Cargo.lock`**）、
     构建入口声明、工具链（cargo/rustc/host/build target）、profile、target-dir、features、构建 args、
     workspace 配置与相关环境变量、package manifest 哈希。
   - `payload_digest` = 暂存包文件清单的规范哈希（排除清单载体自身，避免自引用固定点）。
   - 匹配基准 `match_base=root-relative` 已进入 `scope_digest`；重解析点只登记不追随。
2. **收据口径**：`source_commit=null` + `source_commit_authority=not-authoritative`、
   `vcs_reference_commit`(种子参考) + `vcs_state=untracked_snapshot` + `dirty_against_commit=not_evaluable`
   （绝不写 `false`）；webview2-loader 收据显式标注 `source_identity_is_full_source_snapshot=false`。
   **没有执行任何 `git init`/`add`/`commit`/`tag`**，只做只读探测。
3. **并发编辑**：`New-SourceSnapshot` 两遍静默枚举 + 冻结记录（write-once、按摘要寻址）；
   package-all 在构建后逐文件复核；不一致即 `[SOURCE-SNAPSHOT-CHANGED]`、不写报告。
4. **报告治理**：报告 ID + 可重算内容哈希（在"序列化再解析"的归一化文档上计算）+
   包内 `payload-inventory.json` 反向引用 + `latest-<config>.json` 指针 + installer report 引用；
   新增 `scripts/package-report-retention.ps1`（Verify/Protect/Prune/List，Prune 只删未受保护的、索引损坏时拒绝清理，
   归档到 `docs/testing/release-<version>/evidence/build-identity/`）；build-msi 出包后自动 Protect+Archive。
5. **既有证据保护**：为 `tmp/package-reports/` 里既有的 3 份发布轮次报告就地建立保留条目
   （不搬迁、不改名；版本号与报告的对应未做推断，故未归档到具体版本目录）。

## 实测

- 真实 manifest、`-Configuration release -SkipBuild`、暂存到 `tmp/rd4-06-package-release`（不动真实 `package/`）：
  成功产出报告 `tmp/package-reports/package-report-release-20260925-084033575.json`
  （`report_id=pkg-report-release-20260925-084033575-470d4bcd`，
  `content_sha256=98be85b6bb29116b3d5c1a01b38bc648b3360c4ff536cc249d0f0c6c6c483111`）。
- 并发编辑被真抓到 4 次（`cleanup.rs` / `pipe.rs` / `lib.rs` 枚举期间、`computer_use_store.rs` 构建期间），
  全部以具体路径 fail-closed。
- `scripts/test-package-build-identity.ps1`：36 用例全 PASS（增删改差异、三身份独立变化、
  真实 scope 覆盖与不混入、构建期改写源码 fail-closed、报告/清单/指针、保留规则正负例）。
- 既有测试全绿：`test-package-manifest`、`test-package-webview2-loader`（23 PASS）、
  `test-package-safety`、`test-powershell-script-compat`。
- **MSI 段实测通过**：`build-msi.ps1 -Version 0.2.14 -Configuration release -PackageRoot tmp/rd4-06-package-release -SkipPackageBuild`
  → 真 WiX 出 `dist/CoolzhuAgent-0.2.14-20260925-084930.msi`（未覆盖 canonical 文件），
  installer report 的 `package_report_ref.verified=true` + 三身份 + `msi_payload_sha256` 齐全，
  自动归档到 `docs/testing/release-0.2.14/evidence/build-identity/<report_id>/`（含 installer report）。
  `[REPORT-REF-MISSING]` fail-closed 也在缺指针时被实测触发。
- `package-report-retention.ps1 -Action Verify`：`4 protected report(s) verified`。

## 未闭环

1. **含真实编译**的全量 `package-all` + `build-msi` 仍受并发编辑与 `computer-use-core` 编译问题制约；
   本次 MSI 验证用的是 `-SkipPackageBuild`（消费既有 release 产物与收据）。
2. `dirty_against_commit` 的 `tracked_worktree` 分支未实测（本环境不允许执行 `git init`，见裁决 B-1 第 1 条）。
3. fail-closed 与"照常出包但如实记 `quiescent=false`"之间的取舍未由裁决指定，当前取保守一侧；
   若决策者要求释放"活动树打包"，只需改一个分支，不需改身份口径。
4. `COOLZHU_GIT_SHA` 仍写入种子提交号（报告已标注其"仅参考"语义，但编译进 CLI 的字符串未改）。
5. 既有 flake（非本工单引入）：`test-package-webview2-loader.ps1` 的 `L07c-concurrent-export`
   在 4 次运行中失败 1 次（`File.Replace` 并发替换同一目标），随后连续 3 次全绿；建议另立工单。

## 本次验证留下的产物（供人工核对，非新发布）

- `dist/CoolzhuAgent-0.2.14-20260925-084930.msi`（载荷来自 `tmp/rd4-06-package-release`）
- `dist/CoolzhuAgent-0.2.14-installer-report.json`、`dist/CoolzhuAgent-0.2.14-package-safety.json`
- `docs/testing/release-0.2.14/evidence/build-identity/<report_id>/`（保留索引第 4 条）
- `tmp/package-reports/`（报告 + latest 指针 + 保留索引）、`tmp/source-snapshots/`（冻结记录）、
  `tmp/rd4-06-package-release/`（暂存包，约 300 MB）、`tmp/package-build-identity-contract/`（测试夹具）、
  `tmp/rd4-06-*.txt`（原始输出）

详见 `docs/analysis/2026-09-21-integration-review/build-identity-and-report-governance.md`。
