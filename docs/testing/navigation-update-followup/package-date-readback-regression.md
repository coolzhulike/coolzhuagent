# 0.2.18 候选包首次失败与脚本修复记录

## 首次打包事实

- `scripts/build-msi.ps1 -Version 0.2.18 -Configuration release` 的首轮编译已完成，`artifact-export` 阶段失败，错误为 `The script failed due to call depth overflow`。
- 首轮日志：`tmp/candidate-0.2.18/build-msi.log`。失败诊断：`tmp/package-reports/failures/package-report-failure-release-20260927-122154020.json`，其中 `release_eligible=false`。失败暂存已隔离；没有生成或安装 MSI，原 0.2.17 安装与用户数据未动。
- 实际导出器堆栈：`tmp/candidate-0.2.18/diagnose-loader.log`。`Publish-LoaderGeneration` 回读刚写出的收据后，`Get-LoaderReceiptDigest` 递归进入 `Get-LoaderDigestLeafLines`。PowerShell 7.6.5 默认把 ISO 时间 JSON 字符串转为 `DateTime`，`DateTime.Date` 又返回 `DateTime`，导致无界属性递归。

## 最小修复

- `scripts/lib/webview2-loader.ps1` 与 `scripts/lib/build-identity.ps1` 增加能力检测式 JSON 读取：有 `ConvertFrom-Json -DateKind String` 时保留日期原始字符串；Windows PowerShell 5.1 无该参数，沿用默认字符串读取。摘要函数遇 `DateTime`/`DateTimeOffset` 明确拒绝，避免递归或静默重新格式化。
- `scripts/package-all.ps1`、`scripts/build-msi.ps1`、`scripts/package-report-retention.ps1` 中涉及收据、报告、冻结记录和指针的回读统一走对应 helper。原 JSON 字面量、规范摘要、代次、锁和收据校验规则未变。
- 实际导出链用首轮 Cargo 构建消息在 `tmp/` 独立槽位重放，`tmp/candidate-0.2.18/diagnose-loader-after2.log` 显示导出、发布、回读、消费完整成功；此重放不是正式包。

## 回归与独立夹具问题

- 现有 `scripts/test-package-webview2-loader.ps1` 新增两条 L00：两种等价时刻但不同 ISO 字面量写入→回读摘要各自一致且身份不同；DateTime/DateTimeOffset 明确拒绝。`scripts/test-package-build-identity.ps1` 同样新增两条 I00。
- 首次 PS7 构建身份测试的旧 I07/I08 夹具把 VCS 固定当成 `untracked_snapshot`，与当前 `tracked_worktree` 不符；旧 I09 篡改夹具固定匹配两个空格，PS7 JSON 实际一个空格，因此没改到文件。现已分别校验两种合法 VCS 字段形状/报告一致性，并按 JSON 空白匹配首个目标字段。这三项是测试夹具适配，不是日期产品修复。
- 首次 PS7 Loader 并发测试的旧启动器固定起 Windows PowerShell 5.1 子进程。真实 worker 诊断见 `tmp/package-webview2-loader-concurrency/diagnostic-68f3a039/copy-race-wait60/worker-0-stdout.log`：暂存文件 `exists=True`，实际为 `Get-FileHash` 命令不可见，库的宽捕获将其折为 `staged copy disappeared`。前置代次未建立时下游消费负例不能算有效安全回归。测试启动器现改为与父测试相同的 PowerShell 宿主；隔离同槽 6 次调用全成功，无残片。产品文件哈希校验未弱化。

## 已完成验证

- PowerShell 7.6.5：构建身份完整契约 80/80；Loader 完整契约 39 通过、0 失败、4 项环境/范围性跳过。见 `test-package-build-identity-pwsh-rerun.log` 与 `test-package-webview2-loader-pwsh-rerun.log`。
- Windows PowerShell 5.1.26100.9549：构建身份完整契约 80/80。见 `test-package-build-identity-winps.log`。
- Windows PowerShell 5.1.26100.9549：Loader 并发/代次专跑 15/15；真实双进程与命名屏障均成立。见 `test-package-webview2-loader-winps-concurrency.log` 和 `tmp/package-webview2-loader-concurrency/20260927-050611-3788a84b/summary.json`。
- 两个宿主均已核实际收据与既有报告的日期字符串、摘要回读一致及显式日期对象拒绝：`verify-json-date-readback-pwsh.log`、`verify-json-date-readback-winps.log`。
- `scripts/test-powershell-script-compat.ps1` 与这 7 个脚本的 `git diff --check` 均通过。
- 完整 0.2.18 重打包仍暂停，等视觉用量/only-reasoning 产品窄修通过、报告收口并统一冻结输入后才重启。首轮失败证据原样保留。

PS7 Loader 完整契约的 4 项 skip 与结论边界：`L10-runtime-distribution-change`（本轮没有改变 WebView2 Runtime 分发，缺失/不适配须在实际目标环境检查）；`L11-dll-removal-audit`（本轮不删除 DLL，移除决策需另做发布组合和加载路径审计）；`L04-clean-target-offline-deps`（未另建完全空白 target 来重新下载/离线编译数百 crate）；`L09-clean-windows-native-window`（尚未在无历史开发产物的清洁 Windows 环境安装 MSI 并启动原生窗）。这些跳过项不能外推为目标机兼容性或干净环境安装通过。后续在已有机器的 0.2.18 候选升级验收也不能冒充 L09 的清洁环境结论。
