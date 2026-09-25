# RD4-06 报告政策补丁：失败诊断与发布资格分离（2026-09-25）

- **权威口径**：`docs/analysis/2026-09-21-integration-review/a2-workspace-source-and-frozen-parent-context.md` 第 9 节第一项。
- **实施记录**：`docs/analysis/2026-09-21-integration-review/build-identity-and-report-governance.md` 附录 A（逐条落点、实测、未闭环）。
- **改动文件**（全部在允许边界内）：
  - `scripts/lib/build-identity.ps1`（新增：失败诊断/发布资格/产物释放状态/路径分类）
  - `scripts/package-all.ps1`（主体包 try/catch；阶段追踪；失败诊断；发布资格；隔离分区；`--no-build` 关联/禁用门禁）
  - `scripts/package-report-retention.ps1`（Protect 拒绝失败诊断与 `release_eligible=false`；Prune 不删诊断）
  - `scripts/build-msi.ps1`（引用 package report 时拒绝非发布资格报告）
  - `config/package-manifest.json`（`release_policy`：显式范围、非构建输入声明、落点）
  - `scripts/test-package-build-identity.ps1`（新增 I13a–I13z / I12h / I12i；夹具产物来源移入声明范围）
- **未触碰**：任何 `*.rs` / `*.js` / `*.css` / `*.html`、`config/package-launcher.json`、`packages/app-launcher/**`、
  `scripts/lib/webview2-loader.ps1`、`scripts/test-package-webview2-loader.ps1`、`docs/analysis/2026-09-21-integration-review/rpr-execution-blockers.md`；
  未改变 Git 状态；未清空共享 `target`。

## 结论

不稳定输入所生成的包**仍然拒绝发布**，但失败时**必须**产出失败诊断（独立落点、独立身份、可重算内容哈希）。
发布资格被拆成可分别回答的门（`live_worktree_changed` / `build_snapshot_integrity` / `build_input_digest` /
`validation_snapshot_digest` / `release_eligible` + 逐门证据），任一门 `not-confirmed` 即不放行。

## 实测摘要（原始输出见 `tmp/rd4-06-verify/`）

| 场景 | 结果 |
| --- | --- |
| 夹具：稳定输入（构建 + `--no-build`） | `release_eligible=true`，`--no-build` 经关联记录确认来源 |
| 夹具：构建期间改写快照内源码 | `[SOURCE-SNAPSHOT-CHANGED]` 拒绝 + 诊断 + 隔离 + 不覆盖成功报告/不更新指针 |
| 夹具：构建期间改写**声明的构建输入** | `[BUILD-INPUT-CHANGED]` 拒绝 + 诊断（含描述符差异）+ 禁用记录 ⇒ 之后 `--no-build` `[ARTIFACT-REVOKED]` |
| 夹具：已声明非构建输入的日志变化 | 不判为源码变化，运行照常成功 |
| 真实清单声明范围（1138 文件 / 16 roots），稳定 | `release_eligible=true`；`--no-build` 关联确认（attempts=1） |
| 真实清单声明范围，构建输入中途变化 | 拒绝 + 诊断 + 隔离 2 文件 + 报告/指针字节不变 + `[ARTIFACT-REVOKED]` |
| 真实清单（未裁剪）`--no-build` | 预检 `[RECEIPT-MISSING]` 拒绝；诊断写在真实 `tmp/package-reports/failures/`；真实 `package/` 未动 |
| `scripts/test-package-build-identity.ps1` | 64 cases / 0 failed |
| `test-package-manifest` / `test-package-safety` / `test-powershell-script-compat` | 全部 exit 0 / PASS |

未跑：含真实全量编译的打包（与并发在途编辑冲突且耗时）。

## 阻塞与如实记录

- `test-package-webview2-loader.ps1`（他人工单 PKG-L07c）在本次运行期间被**并发执行/并发编辑**：
  夹具根 `tmp/package-webview2-loader-contract` 被另一个进程清空，出现 `manifest.json`/`messages.jsonl` 找不到、
  以及 `L07c` 结果收集处的 `ArgumentException`。**不是本工单引入**（本工单未触碰该文件与 loader 库）。
  早前一次未被清空的运行里，L01/L02/L02b/L02c/L03/L05/L05a/L07b 与 L07c 单轮**全部 PASS**。
- 需要产品口径确认：构建输入重算使"构建期间锁文件被更新"从"照常通过"变为 `[BUILD-INPUT-CHANGED]` fail-closed
  （裁决第 7 条的要求，但属行为变化）。
