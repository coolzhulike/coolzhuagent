# Work log 2026-09-25：PKG-01 + PKG-02 WebView2Loader 确定性产物链

范围：`scripts/package-all.ps1`、`scripts/lib/webview2-loader.ps1`（新增）、`scripts/build-msi.ps1`、
`config/package-manifest.json`、`scripts/test-package-*.ps1`、`docs/analysis/2026-09-21-integration-review/packaging-webview2-loader-export-protocol.md`（决策记录）。
未修改任何 `*.rs`，未修改 `config/package-launcher.json`。

## 做了什么

1. **移除"声明源存在就直接用"**：`Resolve-WebView2LoaderSource` 的目录扫描回退整体移除，
   原位置留下 tombstone（调用即报错，防止后续被悄悄恢复）。
2. **新增稳定导出协议**（`scripts/lib/webview2-loader.ps1`）：
   本次 cargo 构建的 `build-script-executed` 记录 → 定位生产者 `out_dir` → 校验允许根/架构/内容 →
   原子导出到 `modules/gui-desktop/target/package-inputs/windows-x64/{profile}/WebView2Loader.dll` → 写导出收据。
3. **manifest 改为只消费稳定导出路径**，并新增 `export` 块（生产者包、架构、构建入口、允许根、收据路径、源码身份文件集合）；
   `gui-desktop.tauri-shell` 的 build 增加 `--message-format=json-render-diagnostics` + `capture`，
   使 Loader 与随包 exe 出自**同一次构建**（没有第二次 cargo 调用）。
4. **两种模式明确化**：正常构建模式要求本次构建记录 + 内容校验；`--no-build` 要求完整收据且与本次源码/配置/版本一致，
   并在任何发布前先 fail-fast 校验。收据不足时指引"执行正常构建"，并明确否定"手工复制 DLL"。
5. **错误输出统一**为 `[类别] artifact/target/profile + detail + candidates + next`，
   并区分 `BUILD-FAILED` 与 `BUILD-DEPENDENCY-UNAVAILABLE`（后者不得记成选源失败）。
6. **收据/报告字段补齐**：package report 增加 `build_context` 与 `exported_artifacts[]`；
   installer report 增加 `staged_exported_artifacts[]`；`build-msi.ps1` 在生成 MSI 前校验 staging 文件与收据一致。
7. **契约测试重写**：`scripts/test-package-webview2-loader.ps1` 覆盖 L01/L02/L03/L05/L06/L07/L08/L10/L11，
   其中"正常构建模式"由**真实 `package-all.ps1`** 跑夹具（合成 cargo 消息），L04 以 `-RunCleanTargetBuild` 开关提供。

## 关键核实（本轮实测）

- `tauri-build 2.5.6` 只在 `target_env == "gnu"` 时把 Loader 复制到 `<target>/<profile>/`；
  本仓库是 msvc ⇒ **声明路径上的 DLL 从来不是本仓库构建产出**（实测：完整构建后该文件 mtime 不变）。
- `--message-format=json-render-diagnostics` 对**未重跑的构建脚本**同样输出 `build-script-executed`
  （缓存回放）⇒ 只能用于"定位本次构建实际采用的产物目录"，不能宣称"本轮重新执行了脚本"；
  收据用 `invoked.timestamp` 与调用时刻比较后单独记录"是否本次真正重跑"。
- `tmp/2026-09-19-agent-fixes/pr-checkout/modules/gui-desktop/target` 是**指向本仓库 target 的 junction**
  （`dir /AL` 实锤）⇒ 允许根校验必须按**真实路径**解析，不能只比字符串前缀。
- `tauri-build` 每次构建重写 `src-tauri/gen/schemas/capabilities.json` ⇒ 源码身份不能用整树 `git status`。

## 实跑结果

| 命令 | 结果 |
| --- | --- |
| `scripts/test-package-webview2-loader.ps1` | pass=23 fail=0 skip=4（L04/L09/L10-runtime/L11-删除审计按理由跳过） |
| `scripts/test-package-manifest.ps1` / `test-package-safety.ps1` / `test-powershell-script-compat.ps1` | PASS |
| `scripts/package-all.ps1 -Manifest <shell+loader 临时清单> -Configuration release` | 成功；staged `bin/WebView2Loader.dll` SHA-256 == 稳定导出 == 收据，无手工复制 |
| `scripts/package-all.ps1 -SkipBuild -Configuration release -PackageRoot tmp/… -ReportPath tmp/…` | 成功；`exported_artifacts[0].validation = existing-receipt-verified` |
| `scripts/package-all.ps1`（全量 release） | 未完成：`coolzhu-computer-use-core` 在本工作树编译失败（既有问题），web-console 构建被正确归类为 `BUILD-FAILED` |

## 未闭环

- L04 完整清洁 target 构建（数十分钟级）未执行，命令已内置。
- L09 清洁 Windows 环境原生窗口启动、L11 删除 DLL 的消费者审计未执行（不触碰活动安装）。
- 安装态"实际加载来源属于预期安装位置"的运行时核对待安装验证阶段完成。
- 全量打包被 `coolzhu-computer-use-core` 编译错误阻塞，修好后需重跑完整 `package-all.ps1` + `build-msi.ps1`。
