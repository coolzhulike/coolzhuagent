# 决策记录：WebView2Loader 的确定性产物链与复用/异常策略（PKG-01 + PKG-02）

状态：已实施（本轮第四轮裁决第三节）；取代 `packaging-loader-and-runtime-dir.md` 第 1 节的旧选源描述。
相关裁决：`packaging-loader-and-runtime-dir.md` 顶部的第四轮裁决更正表（"最先应合并的两项"之①）。

本文件按裁决要求固定回答八个问题。

---

## 0. 一句话结论

`bin/WebView2Loader.dll` 不再来自"声明路径存在就直接用"或"扫目录挑一个"，
而是来自**本次构建记录点名的依赖产物**，经**架构/内容校验**后导出到**受控稳定路径**，
由 manifest 消费，并留下**可核对的导出收据**；收据与本次源码/配置/版本不一致时**拒绝发布**。

---

## 1. 为什么存在（问题与证据）

### 1.1 症状史

- `config/package-manifest.json` 曾声明 `source = modules/gui-desktop/target/tauri-shell/{profile}/WebView2Loader.dll`。
- 该路径**不是本仓库 msvc 构建的产物**：`tauri-build 2.5.6`（`src/lib.rs`，`target_env == "gnu"` 分支）
  只在 **gnu** 目标下把 `webview2-com-sys` 的 Loader 复制到 `<target>/<profile>/WebView2Loader.dll`；
  msvc 目标只做 `static_vcruntime`。本仓库构建目标是 `x86_64-pc-windows-msvc`。
- 实测证据（本轮，2026-09-25）：完整跑过一次 tauri-shell release 构建后，
  `modules/gui-desktop/target/tauri-shell/release/WebView2Loader.dll` 的 mtime 仍停在 **2026-09-19 12:02**，
  而 `coolzhu-tauri-shell.exe` 被重写为本次构建时间 ⇒ **该文件不是本次构建产出**。
- 它的内容与 `webview2-com-sys-55b44b0ddeeacab2/out/x64/WebView2Loader.dll` 完全相同（同为 `8427b1fc…`），
  即它是**早先手工从构建输出复制到声明路径**的副本。

### 1.2 旧逻辑为什么危险

旧 `Resolve-WebView2LoaderSource` 的第一分支是：

```
if (Test-Path 声明源) { return 声明源 }      # ← 只做存在性检查
否则 扫 <release>/build/webview2-com-sys-*/out/x64/WebView2Loader.dll
     候选=1 用；候选=0 报错；候选>1 报"ambiguous"
```

- `Test-Path` **不是有效性检查**：任何手工放进声明路径的 DLL 都会**先于有效构建来源**被采用，
  且不会被任何架构/来源校验拦住。
- 目录扫描结果**没有来源记录**，属于"未经绑定的搜索回退"。
- 多版本残留时按"候选数"判定，操作者只看到 ambiguous，缺可操作指引。

### 1.3 真正的事实（本轮核实）

`webview2-com-sys 0.38.2` 的 `build.rs` 只把三套架构的 DLL/LIB 复制到 `OUT_DIR/<arch>/`，
并通过 `cargo:rustc-link-search=native=<OUT_DIR>/x64` 告诉 rustc；
`cargo build --message-format=json-render-diagnostics` 会输出
`build-script-executed` 消息，内含 `package_id` 与 **`out_dir`**，
这就是"**本次构建实际采用的依赖产物目录**"的权威记录。

---

## 2. 适用于哪些入口 / 目标 / 版本

| 维度 | 范围 |
| --- | --- |
| 入口 | `scripts/package-all.ps1`（唯一打包入口）、`scripts/build-msi.ps1`（staging→MSI 校验） |
| artifact | `gui-desktop.webview2-loader` → `bin/WebView2Loader.dll`（本轮只此一个 export artifact） |
| 构建入口 | `gui-desktop.tauri-shell`（`modules/gui-desktop/packages/tauri-shell/src-tauri`，**独立 Cargo 项目**，有自己的 `Cargo.lock` 与 `[workspace]`） |
| 目标 | `x86_64-pc-windows-msvc`（`architecture = x64`）；其它架构需在 manifest 里显式改 `architecture` 与 `producer_arch_directory` |
| profile | `debug` / `release`，由 `-Configuration` 控制，落在路径 `{profile}` 上 |
| 依赖版本 | 不锁版本号；身份由 `package_id`（含版本）与构建入口 `Cargo.lock` 哈希共同固定 |
| 不适用 | WebView2 **Runtime** 的分发与前置检查（本轮明确不改，见 §6） |

---

## 3. 输入来源与优先级（稳定导出协议）

### 3.1 生产者 → 稳定导出 → manifest → MSI

```
cargo build（gui-desktop.tauri-shell，本次构建，capture json 消息）
   │  build-script-executed(package_id=…#webview2-com-sys@0.38.2, out_dir=…)
   ▼
生产者文件  <out_dir>/x64/WebView2Loader.dll        ← 唯一允许的原始来源
   │  校验：out_dir 在允许根内（按真实路径解析 junction）、PE machine=AMD64、长度>0、SHA-256
   ▼
稳定导出    modules/gui-desktop/target/package-inputs/windows-x64/{profile}/WebView2Loader.dll
   │       （临时文件 → 内容复核 → 原子替换）
   ▼
导出收据    modules/gui-desktop/target/package-inputs/windows-x64/{profile}/webview2-loader-export.json
   ▼
manifest    "source": "…/package-inputs/windows-x64/{profile}/WebView2Loader.dll"（**只消费稳定导出**）
   ▼
staging     package/bin/WebView2Loader.dll（复制后再复核 SHA-256 + 架构）
   ▼
MSI         installer/Product.wxs 的 <Files Include="$(PackageRoot)\**">
```

**manifest 不再依赖 `webview2-com-sys-*` 这类动态哈希目录**；稳定导出目录是仓库内新建的
受控目录（此前不存在同义目录，因此这是唯一一套，没有第二套并行）。

### 3.2 manifest 新增声明（`export` 块）

```json
"export": {
  "kind": "cargo-build-script-output",
  "producer_package": "webview2-com-sys",
  "architecture": "x64",
  "build_entry_artifact": "gui-desktop.tauri-shell",
  "build_root": "modules/gui-desktop/target/tauri-shell/{profile}",
  "producer_arch_directory": "x64",
  "producer_file_name": "WebView2Loader.dll",
  "build_entry_manifest": "…/src-tauri/Cargo.toml",
  "lockfile": "…/src-tauri/Cargo.lock",
  "receipt": "…/package-inputs/windows-x64/{profile}/webview2-loader-export.json",
  "source_identity": { "files": [...], "directories": [...], "exclude_patterns": [...] }
}
```

`gui-desktop.tauri-shell` 的 build 增加 `--message-format=json-render-diagnostics` 与
`"capture": "cargo-json-messages"`；**没有新增第二次 cargo 调用**——Loader 与随包 exe 出自同一次构建。

### 3.3 收据字段（`schema = 2`）

> `schema = 2`（PKG-L07c，2026-09-25）新增 `publication` 组：**发布代次与发布权**。
> `schema = 1` 是单发布者协议之前的记录，缺少槽位/代次绑定，已被 `RECEIPT-SCHEMA` 明确拒绝，
> 必须重新构建导出（不允许把它当作可发布凭据）。

| 组 | 字段（要点） |
| --- | --- |
| 构建身份 | `source_identity_kind` / `source_identity_sha256`（声明文件哈希集合）/ `source_identity_files` / `source_identity_sampling` / `source_identity_before_publish_sha256` / `source_commit`(+`_available`) / `tracked_build_entry_files` / `build_entry_manifest`(+sha)/ `lockfile`(+sha)/ `cargo_target_dir` / `profile` / `build_target` / `host_target` / `cargo_version` / `release_version` |
| 构建入口 | `build_invocation.command/args/working_dir/message_format/invoked_at` / `raw_message_record`(+sha) / **`producer_build_script_rerun_in_this_invocation`** |
| 依赖身份 | `producer_package` / `package_id` / `producer_out_dir`(+`_repo_relative`) / `producer_candidate_count` / `producer_candidates` / `producer_selection_evidence` |
| 文件身份 | `raw_source`(+len/sha/machine) / `stable_export`(+len/sha) / `package_target` / `architecture` / `machine` |
| 复用性质 | `reuse_nature` = `producer-output-regenerated-in-this-invocation` \| `producer-output-reused-from-existing-cargo-build-directory` |
| 打包关联 | `package_association.package_target/profile/consumed_by/msi_input` |
| **发布代次（schema 2）** | `publication.protocol` / `slot_key` / `slot_identity` / `slot_dir` / `destination` / `generation` / `state = complete` / `file_published_utc` / `receipt_committed_utc` / `publish_right.{mechanism, lock_file, owner_pid, owner_token, owner_host, acquired_utc, reclaimed_stale_lock}` / `cross_file_atomicity = none: continuous-replace-of-two-files` / `incomplete_generation_rule` / `consumer_rule` |
| Runtime（分开登记） | `runtime_dependency.webview2_runtime_asserted = false` + 说明文字 |

**关于"是否本轮重新生成"**：`build-script-executed` 对 fresh 单元会**返回缓存输出**
（本轮已实证：cargo 在一次构建脚本**未重跑**的构建里照旧输出了该消息，build 目录 mtime 未变）。
因此收据**不把它当"重新执行"**，而是另外记录
`producer_build_script_invoked_timestamp_utc` 并与本次调用时刻比较，得到 `reuse_nature`。

**关于源码身份**：`tauri-build` 每次构建都会重写 `src-tauri/gen/schemas/capabilities.json`（实测 mtime 被刷新），
所以身份**不使用整树 `git status`**，而是 manifest 显式声明的文件 + 导入目录的哈希集合
（`source_identity_kind = declared-file-hash-set`，排除 `gen/`）。
另注：本工作树的 HEAD 是 sync 后的种子提交，`tracked_build_entry_files = 0`，
因此 `source_commit` 只作参考，**载荷身份是文件哈希集合**——这一点在收据里显式可见。

> **该哈希集合不是全量源码快照**（RD4-06 / 第五轮裁决 B-1 补正）：它只覆盖该构建入口声明的文件与导入目录。
> 收据因此显式登记 `source_identity_is_full_source_snapshot = false`、
> `source_snapshot_digest = null`（未计算，不冒充），并指向 package report 的 `build_identity`：
> 全量源码身份是 `source_snapshot_digest`，构建输入身份是 `build_input_digest`，载荷身份是 `payload_digest`。
> 定义与落点见 `build-identity-and-report-governance.md`。

### 3.4 附带补齐的报告字段

`package-all.ps1` 的 package report 新增 `build_context`（mode/manifest sha/build target/host/cargo 版本/
release version/source commit/source_identity_kind）与 `exported_artifacts[]`（收据路径、稳定导出路径与 SHA-256、
生产者 package_id / out_dir / 候选数 / 选择证据、架构、`reuse_nature`、`validation`）。
`build-msi.ps1` 的 installer report 新增 `staged_exported_artifacts[]`（staging 文件的 SHA-256/长度/架构 + 收据来源）。

RD4-06 追加（同上决策记录）：

| 组 | 字段要点 |
| --- | --- |
| 三身份 | `build_identity.{source_snapshot_digest, build_input_digest, payload_digest}` + `identity_questions` + `caveat` + `payload_digest_scope` |
| 源码快照 | `source_snapshot.{digest, scope_digest, file_set_digest, file_count, total_bytes, roots, exclude_path_patterns, allow_path_patterns, external_path_dependencies, skipped_reparse_points, unclassified_files, freeze_record(+sha), post_build_verification}` |
| 构建输入 | `build_inputs.{digest, descriptor_count, descriptors[]}`（含独立 Tauri 项目自己的 `Cargo.lock`/`Cargo.toml`/`build.rs`/`tauri.conf.json` 与逐 artifact 的 target-dir/features/args） |
| 载荷 | `payload_inventory.{path, payload_digest, file_count, total_bytes}` + 包内 `payload-inventory.json`（引用报告 ID + 内容哈希） |
| 报告身份 | `report_identity.{report_id, content_sha256, content_hash_scope}`；报告落点与保留规则见 §9.3 |
| VCS 口径 | `build_context.{source_commit=null, source_commit_authority, vcs_state=untracked_snapshot, vcs_reference_commit(_kind), dirty_against_commit=not_evaluable, tracked_index_file_count, vcs_evidence}` |
| installer report | `package_report_ref.{report_id, content_sha256, report_file_sha256, captured_from, verified}`、`payload_inventory_ref.*`、`build_identity.*`（含 `msi_payload_sha256`）、`vcs.*`；拿不到报告引用时 `[REPORT-REF-MISSING]` fail-closed |

---

## 4. 失败时如何处理（两种模式与拒绝条件）

### 4.1 正常构建模式（默认）

对 export artifact 的执行顺序：**先构建入口 → 从构建消息定位生产者 → 校验 → 导出 → 写收据 → 再发布**。

| 情况 | 结果 |
| --- | --- |
| 本次构建消息里没有 `webview2-com-sys` 的 `build-script-executed` | `PRODUCER-NOT-FOUND`（**不做目录扫描兜底**） |
| 出现多个不同 `out_dir` | `AMBIGUOUS-PRODUCER`（**不按时间排序挑选**） |
| `linked_paths` 与 `out_dir` 不一致 | `PRODUCER-EVIDENCE-INCONSISTENT` |
| 生产者目录解析后不在 `build_root/{profile}` 允许根内 | `OUTSIDE-ALLOWED-ROOT`（不复制、不清理） |
| 生产者目录里没有 Loader / 空文件 | `SOURCE-MISSING` / `SOURCE-INVALID` |
| PE 架构 ≠ manifest 声明（含截断、非 PE） | `ARCH-MISMATCH` |
| 复制过程中内容改变 | `CONTENT-MISMATCH`（临时文件被丢弃，不留残片） |
| 收据写入失败 | 直接失败，绝不继续发布该 artifact |
| 该输出槽位已有别的发布者（等待超时） | `EXPORT-SLOT-BUSY`（busy=true retryable=true；**不修改赢家的锁/产物**） |
| 导出窗口内声明源输入发生变化 | `SOURCE-INPUT-CHANGED`（**不写收据**，该次发布不获准；不靠任何 `quiescent=false` 放行） |
| 自己持有的锁被第三方删除/接管 | `PUBLISH-RIGHT-LOST`（中止发布，不继续写产物） |
| 发布后内容与校验身份不符（有第三方改写槽位） | `PUBLISH-INCONSISTENT` |
| cargo 构建失败 | `BUILD-FAILED`；若 stderr 命中离线依赖缺失特征则为 `BUILD-DEPENDENCY-UNAVAILABLE`（**明确说明这不是 Loader 选源失败**） |

### 4.2 `--no-build`

接受条件（全部满足才接受并发布）：
收据存在且 schema/artifact/target/profile/**slot_key** 一致、`publication.state = complete`；稳定导出文件存在且长度+SHA-256 与收据一致；
PE 架构一致；`artifact_contract_sha256`（source/target/build/export 归一化哈希）一致；
`source_identity_sha256` 一致；`lockfile`/`build_entry_manifest`/`build_target`/`cargo_target_dir` 一致；
若本次带 `COOLZHU_RELEASE_VERSION` 则必须与收据一致；
读取期间必须持有该槽位的读取权（见 §10）。
不满足 → 对应 `RECEIPT-MISSING` / `RECEIPT-INVALID` / `RECEIPT-SCHEMA` / `RECEIPT-MISMATCH` / `RECEIPT-STALE` / `RECEIPT-INCOMPLETE` / `RECEIPT-SLOT-MISMATCH` / `CONTENT-MISMATCH` / `ARCH-MISMATCH` / `PROFILE-MISMATCH` / `EXPORT-SLOT-BUSY`。

- **清洁 target 与空依赖缓存不是同一测试**：`--no-build` 只要求"已有完整构建收据 + 与本次源码/配置/版本一致"；
  不要求生产者目录仍在。缺依赖导致的*构建*失败会被单独归类为 `BUILD-DEPENDENCY-UNAVAILABLE`。
- 收据不足时的指引固定为"**执行正常构建，或指定匹配的构建产物**"，
  并**明确否定**"手工复制 DLL"（`next: … 不要手工复制 DLL 到 bin 或稳定导出路径`）。
- `--no-build` 的校验在任何发布之前先做一轮 fail-fast，避免留下半成品 staging。

### 4.3 错误文本形态（统一）

```
[<类别>] artifact=<id> target=<包内目标> profile=<配置>
detail: <说明>
candidates:
  - <候选路径/记录路径>
next: <操作指引，优先"使用独立、受控的构建目录重建">
```

不默认建议全局 `cargo clean`、也不建议删除共享 target（历史共享 target 存在 junction 记录，见 §7）。

---

## 5. 会不会改变既有用户数据位置

不会。本变更是**打包期**行为：

- 只改写入点 `modules/gui-desktop/target/package-inputs/…`（构建产物目录，`.gitignore` 的 `**/target/`）与
  `tmp/logs/*-build-messages-*.jsonl`（临时证据）。
- 不触碰 `%USERPROFILE%\coolzhuagent`、`%LOCALAPPDATA%\CoolzhuAgent`、
  `config/package-launcher.json`（另有工单）或任何安装态数据。
- 旧手工 DLL（`modules/gui-desktop/target/tauri-shell/release/WebView2Loader.dll`）**保持原地不动**
  （裁决要求：不删除），只是**不再被任何正式路径消费**。

---

## 6. Runtime 与 Loader 分开登记

本轮**不修改 WebView2 Runtime 的分发策略**。收据中显式登记
`runtime_dependency.webview2_runtime_asserted = false` 并写明"本收据只证明 Loader 文件来源"，
以避免"Loader 修好了"被误读成"目标机已有合适 Runtime"。
目标机缺 Runtime / Runtime 不适配的报错来自 Runtime 自身，需在真实安装环境验证（L10）。

---

## 7. 自动回归与安装证据

### 7.1 自动回归

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-package-webview2-loader.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-package-manifest.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-package-safety.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-powershell-script-compat.ps1
```

`test-package-webview2-loader.ps1` 覆盖 L01/L02/L03/L05/L06/L07/L08/L10/L11，
其中"正常构建模式"是**让真实 `package-all.ps1` 跑一遍**（夹具把 capture 的构建命令替换为回放合成
cargo 消息的脚本），因此被测对象是生产脚本本身。

`-OnlyConcurrency -ConcurrencyRounds N` 只跑 PKG-L07c 并发/发布权契约（不依赖 `package-all.ps1`），
每轮都用**两个真实子进程 + 命名事件屏障**触发并发，并给出八条用例的逐轮结论 +
`tmp/package-webview2-loader-concurrency/<run>/` 下的真实进程日志与 `summary.json`。八条用例见 §10.4。

### 7.2 真实构建证据（本轮实跑）

| 命令 | 结果 |
| --- | --- |
| `cargo build --manifest-path …/tauri-shell/src-tauri/Cargo.toml --bin coolzhu-tauri-shell --offline --target-dir modules/gui-desktop/target/tauri-shell --release --message-format=json-render-diagnostics` | 成功；证明 fresh 构建脚本同样输出 `build-script-executed`（缓存回放） |
| `scripts/package-all.ps1 -Manifest <仅 shell+loader 的临时清单> -Configuration release` | 成功：定位生产者 → 导出 → 发布；`bin/WebView2Loader.dll` SHA-256 == 稳定导出 == 收据值，**全程无手工复制** |
| `scripts/package-all.ps1 -SkipBuild -Configuration release -PackageRoot tmp/… -ReportPath tmp/…` | 成功：收据通过身份校验后被消费（`exported_artifacts[0].validation = existing-receipt-verified`） |
| `scripts/package-all.ps1`（全量清单，release） | **未完成**：`coolzhu-computer-use-core` 在本工作树不编译（与本次改动无关的既有错误），`gui-web.web-console` 构建失败并被正确归类为 `BUILD-FAILED` |

### 7.3 安装证据

本轮**未做**真实安装与原生窗口启动（不触碰活动安装；需清洁 Windows 环境，L09）。

---

## 8. 何时可以删除兼容逻辑 / 由谁维护

### 8.1 可供删除的对象与条件

| 对象 | 删除条件 |
| --- | --- |
| `Resolve-WebView2LoaderSource` 的 tombstone 函数 | 其目录扫描实现**已在本轮移除**；tombstone 只用于让"这里曾经是搜索回退"可被搜索。待确认无人再引用后即可整体删除 |
| 稳定导出目录中的旧收据 | `cargo clean` 或换 profile 后自然失效（`RECEIPT-MISSING`），无需人工清理 |
| **`bin/WebView2Loader.dll` 本身** | **不满足**：只有证明**全部相关消费者采用静态 Loader**（覆盖实际发布组合、构建配置、直接/延迟/运行时加载路径、清洁环境原生窗口启动）才允许在**独立变更**中删除。"只知道 Tauri 大版本 / 某目录没文件 / 某 EXE 普通导入表里没看到" ⇒ **证据不足，不删除** |
| 本轮新增的 export 校验 | **不应删除**：删除的是"未经绑定的搜索回退"，不是"缺文件就报错"的安全边界 |

### 8.2 维护责任

- 打包脚本与 manifest 声明：`modules/gui-desktop`（tauri shell）与 `scripts/` 打包链路的负责人。
- 每次改动默认值或回退规则，必须同步在本文件（或同级决策记录）追加条目。

---

## 9. 未闭环 / 需后续工单

1. **L04**：清洁 target + 离线依赖缓存的整套构建未在本轮时间盒内执行（命令已内置在
   `test-package-webview2-loader.ps1 -RunCleanTargetBuild`）。
2. **L09 / L11**：清洁 Windows 环境的原生窗口启动、以及删除 DLL 前的消费者审计（含实际加载来源核对），
   需要在真实安装环境执行，并需先确认"哪些消费者确实动态加载 Loader"。
3. **报告落点命名（RD4-06 已闭环，第五轮裁决 B-2）**：裁决提到"现有 `build/package-report.json` 证据体系"，
   该措辞是**对本仓库落点的不准确假设**。本仓库实际落点（且**保持不改名、不搬迁**）是
   `tmp/package-reports/package-report-<config>-<stamp>.json`（或 `-ReportPath` 指定），
   同目录还有 `latest-<config>.json` 指针与 `retention-index.json` 保留索引。
   落点、报告 ID/内容哈希、产物清单引用、保留与归档规则统一记录在
   `build-identity-and-report-governance.md`，并由 `scripts/package-report-retention.ps1` 落实。
4. **全量打包未跑通**：`coolzhu-computer-use-core` 编译失败阻塞 `gui-web.web-console`；
   修好后应重跑一次完整 `package-all.ps1` + `build-msi.ps1` 并归档 installer report。
   注（RD4-06）：`-SkipBuild`（消费既有 release 收据与产物）的真实全流程已经跑通并产出带三身份的报告；
   仍未执行的是**含真实构建 + MSI 生成**的那一段。
5. **运行时加载来源核对**：本地 DLL 搜索目录/path 未被本流程扩大（明确禁止），
   但"实际加载来源属于预期安装位置"仍需在安装验证阶段用真实进程核对。

---

## 10. 单一发布者协议与并发导出一致性（PKG-L07c）

本节是第六轮裁决 §四的落实记录。**取代**此前"并发失败只是既有 flake"的说法：
并发契约被实测查清并已修复，下面是实际语义与仍然成立的边界。

### 10.1 修复前的事实（实测，非推断）

两个真实进程并发对**同一个导出目标**发布时（改动前的实现，无发布权互斥）：

| 观测 | 形态 |
| --- | --- |
| 目标不存在时两侧同时进入 | `File.Move` → `ERROR_ALREADY_EXISTS`（`Cannot create a file when that file already exists.`） |
| 目标已存在时并发替换 | `File.Replace` → `ERROR_UNABLE_TO_REMOVE_REPLACED`（`Unable to remove the file to be replaced.`）/ `ERROR_UNABLE_TO_MOVE_REPLACEMENT` / `ERROR_SHARING_VIOLATION` |
| 事后核对 | `published file identity mismatch`（失败方发现目标已是别人的内容） |
| 目标可见性 | 失败方可能观察到目标**暂时不存在**（`Cannot find path … because it does not exist.`） |

出处：`tmp/l07c-repro/baseline/test-run-before-*.log`（真实测试脚本，`L07c-concurrent-export`）
与 `tmp/l07c-repro/out-replace/`（160 次并发 `File.Replace`：30 次失败、三种 Win32 错误文本、目标未丢）。
**160 次替换没有出现"混合/半成品内容"，也没有出现目标永久丢失**——这一点决定了本次判定（见 §10.5）。

### 10.2 协议（锁范围 = 实际共享输出槽位）

```
取得目标发布权 → 本次运行独立暂存并核对 → 发布产物 → 发布收据 → 校验完成 → 释放发布权
```

- **槽位** `slot_key = sha256(规范槽位目录 | 目标文件名 | package target | profile)`。
  **不含** build id / 运行 ID / 产物哈希：两个不同 build id 仍会覆盖同一个文件，按 build id 加锁等于没加锁。
  槽位目录用真实路径解析（junction 折叠）。
- **发布权** = 在槽位目录里以 `CreateNew` 独占创建 `.loader-publish-<slot_key>.lock`，
  并在整个发布窗口**保持打开的句柄**（`FileShare::Read`）。因此在自己释放前，别的进程既不能删除、
  也不能改名这个锁文件（跨会话有效），但可以只读地看到持有者记录（pid/token/host/purpose/artifact）。
- **争用**：只有"持有者 pid 不存在"或"pid 已被复用（进程启动时刻与记录不匹配）"才判定死锁并回收；
  回收失败仍受等待上限约束，不会无限重试。超时 → `EXPORT-SLOT-BUSY`（明确 Busy，`retryable=true`），
  **不触碰赢家的锁/暂存/产物**。
- **释放**：只删除 token 与自己一致的锁文件；竞争者失败或被取消不会删除他人的锁。
- **暂存**：本次运行唯一命名的 `.loader-stage-*` 文件（先复制并复核内容身份，再原子替换）。
  久置残片的清理只在**持有发布权时**进行。
- **消费端同一边界**：`Assert-LoaderExportReceipt` 先取得同一槽位的读取权，再核对"产物 + 收据"；
  拿不到读取权时 fail-closed（`EXPORT-SLOT-BUSY`），而不是读一个"看起来存在"的中间状态。
- **读侧为什么必须持锁（实测，不只是"为了看到一致的一对"）**：
  1. `ReplaceFile` 语义下目标会**短暂不存在**（写者 300 次替换期间，另一个进程 39532 次观测里有
     764 次看到目标不存在），未加锁的读者会读到"没有产物"；
  2. 未加锁的读句柄（例如 `Get-FileHash` / `Get-Content`）会让**正在发布的进程**的 `File.Replace`
     以 `ERROR_SHARING_VIOLATION`（`The process cannot access the file because it is being used by
     another process.`）失败 —— 读侧不守边界会造成发布方**假失败**。
     实测出处：`tmp/l07c-repro/evidence-unlocked-reader-breaks-publisher.log`
     （3 轮里 2 次，均为"持有发布权的发布者被未加锁读者打断"）。
  因此：**任何读取稳定导出产物/收据的代码都必须先取得该槽位的读取权**，
  而不是"只读所以安全"。

### 10.3 明确不声称的事情（边界）

- 产物与收据是**连续替换两个文件，不是跨文件原子提交**。`publication.cross_file_atomicity =
  none: continuous-replace-of-two-files` 写在收据里。
- 可依赖的不变量是：**收据内嵌产物内容身份 + 代次**，因此"产物已换、收据未更新"（进程在两者之间退出）
  与"收据在、产物被替换"这两种状态都会被消费端的内容核对拒绝，并且**不得当作上一成功版本**。
- 单发布者协议只约束**同一个输出槽位**。两个打包进程若共用同一个槽位，仍然会轮流刷新该槽位：
  每次完成时的"产物 + 收据"都自洽，但**后完成者会让先前报告的 `receipt` 路径指向新代次**。
  因此并发打包必须使用**独立输出**（见 §10.5）。

### 10.4 契约测试（八条用例，每轮真跑）

`scripts/test-package-webview2-loader.ps1 -OnlyConcurrency -ConcurrencyRounds N`：

| 用例 | 断言 |
| --- | --- |
| `L07c-concurrent-export` | 两个真实进程争用同一目标（wait=0）：所有失败都是 `EXPORT-SLOT-BUSY`、至少一次成功、终态是**某个完整代次**、无残片、目标从未"消失" |
| `L07c-concurrent-export-controlled-serial` | 同上但 wait=60：受控串行，零失败，每次落地都是完整可识别内容 |
| `L07c-concurrent-exporter-consistent-generation` | 两个真实进程跑**真实导出**（各自不同的生产者内容）：完成时"产物 + 收据"自洽，终态收据的 `generation`/`sha` 与盘上产物一致（**不允许 A 的 DLL 配 B 的收据**） |
| `L07c-incomplete-generation-rejected` | 进程在"产物已换、收据未落地"时**硬退出**（exit 97）：消费者以 `CONTENT-MISMATCH` + `incomplete_generation_or_tampered=true` 拒绝；被杀进程留下的锁能被回收 |
| `L07c-receipt-with-tampered-artifact-rejected` | 收据在、产物被第三方替换：消费者 `CONTENT-MISMATCH`，不允许发布 |
| `L07c-loser-does-not-touch-winner` | 赢家持锁期间，竞争者 ① 返回明确 Busy、② 被 `Stop-Process` 取消：赢家的锁 token / 产物 / 收据均未被改动，赢家随后正常完成 |
| `L07c-distinct-slots-not-serialized` | 两个**不同**输出槽位的发布者同时停在发布窗口（屏障两个分支都到齐 + 两把锁同时存在）⇒ 没有被无关全局锁串行化；两个槽位互不污染 |
| `L07c-source-input-changed-rejected` | 导出窗口内改写声明的源输入：`SOURCE-INPUT-CHANGED`、**不写收据**、消费者拒绝留下的组合（不靠 `quiescent=false` 放行） |

时序全部由命名事件屏障触发（两个真实进程在固定点同时进入），不靠 sleep 碰运气。
每轮结果与真实进程 stdout/stderr、退出码、最终产物身份落盘在
`tmp/package-webview2-loader-concurrency/<run>/round-NNN/`，汇总在 `summary.json` / `round-results.jsonl`。

测试专用注入点（默认完全惰性，只在显式设置下列环境变量时生效）：
`COOLZHU_LOADER_TEST_SEAM` / `_ACTION`（`park` \| `throw` \| `exit`）/ `_READY_EVENT` / `_GO_EVENT` /
`_TIMEOUT_SECONDS`；**生产运行不得设置**，触发时会 `Write-Warning` 进日志。

### 10.5 正式发布政策（在并发导出能力被正式验收之前）

1. **单一打包者 + 独立输出**：同一时间只允许一个打包进程写给定输出槽位；
   并发的打包运行必须使用独立 `--target-dir` / 独立输出目录（不得共用同一个 `package-inputs` 槽位）。
2. 发布前必须核对**产物内容与收据**：`Assert-LoaderExportReceipt`（含 `slot_key` / `publication.state` /
   内容 SHA-256 / 架构）通过才允许进入 staging 与 MSI。
3. **不得宣称已通过并发导出能力验收**：本次修复消除了并发覆盖与跨代次混配，
   但正式验收仍需在多机/多用户、真实 MSI 全流程下按 §10.4 重跑并归档。
4. 旧 `schema = 1` 收据一律按 `RECEIPT-SCHEMA` 拒绝：正式发布必须重新构建导出。

### 10.6 本次判定与未闭环

- **判定：P1（打包完整性门禁）**，未升级 P0。证据：发布路径的两处消费点（`package-all.ps1` 的
  `Publish-Artifact` 与 `Assert-LoaderExportReceipt`）都以**内容 SHA-256** 核对"稳定导出 vs 收据"，
  并发造成的错配会 fail-closed（`CONTENT-MISMATCH`），没有把错配 DLL/收据送进 staging、MSI 或报告；
  160 次并发替换中也未出现混合/半成品内容。
- 未闭环：① 并发打包共用同一槽位时，后完成者会让先前报告的 `receipt` 路径指向新代次——
  报告里没有登记 `publication.generation`，无法据此自动判定"报告引用的收据还是不是它消费的那一代"。
  建议 `package-all.ps1` 在 `exported_artifacts[]` 里带上 `generation` / `slot_key`（该文件属 RD4-06 工单，本次未改）。
  ② `--no-build` 消费端在槽位目录不存在时会先创建该目录以便取读取权；
  这是派生输出目录，不影响仓库状态，但如需"绝不创建"语义需另行裁决。
  ③ `package-all.ps1` 的 `Publish-Artifact` 在 `Assert-LoaderExportReceipt` 返回后（已释放读取权）
  自行哈希稳定导出文件做二次核对：这一步**未持读取权**，因此（a）可能读到 `ReplaceFile` 的短暂窗口、
  （b）其读句柄可能让另一个并发发布者的 `File.Replace` 假失败。两者都是 fail-closed（`CONTENT-MISMATCH`
  或发布被拒），不会放出错配产物；但若要消除假失败，应把这一步也放进同一槽位的读取权内。
  该文件属 RD4-06 工单，本次未改，仅记录。

### 10.7 测试侧：夹具根按运行隔离（与产品并发契约的区分）

RD4-06 情报（其台账 §B-56）指出：`test-package-webview2-loader.ps1` 使用**固定**夹具根
`tmp/package-webview2-loader-contract`，两个进程并发跑同一测试会互相清空，报
`Cannot find path …\manifest.json`，并在结果收集处出现过一次 `ArgumentException`。本次判定与处置：

- **性质区分**：这是**测试夹具互相踩**，不是产品并发契约缺陷。产品侧的共享输出槽位是
  `…/package-inputs/windows-x64/{profile}`（产物 + 收据），已有单发布者协议保护；
  夹具根只是测试脚手架，两者不是同一条边界，不能用同一条结论覆盖。
- **`ArgumentException` 的成因**：与本次在结果收集处实测到的同一族问题一致 ——
  Windows PowerShell 5.1 对 `@(<List[object]>)` 会抛
  `ArgumentException: Argument types do not match`（对 `List[string]` / `ArrayList` 不会）。
  本测试新的结果收集已避开该写法（改用管道过滤 / 直接取 `.Count`）。
- **处置（测试文件属本工单）**：夹具根改为**按运行隔离**
  `tmp/package-webview2-loader-contract/<yyyyMMdd-HHmmss>-<8hex>/`，
  且只清理"名字形如运行 ID 且超过 6 小时"的旧运行目录（绝不删基根本身、绝不删其它形状的目录，
  避免踩到并发运行的目录）。并发契约用例本来就落在独立根
  `tmp/package-webview2-loader-concurrency/<run>/`。
- **验证**：同时启动两次**全量**本测试，两次都是 `pass=30 fail=0 skip=4`，
  各自夹具根不同（`…020946-6c533ad0` / `…020947-708e19cf`），
  日志：`tmp/l07c-repro/after/concurrent-full-run-A.log` / `-B.log`。
