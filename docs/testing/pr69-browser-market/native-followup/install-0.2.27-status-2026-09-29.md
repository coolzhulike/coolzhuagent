# 0.2.27 安装状态（2026-09-29）

**结论：0.2.27 完整 MSI 尚未安装成功，现有正式安装目录仍是旧版。不要把此次尝试当作新包验收。**

- 候选安装包：[CoolzhuAgent-0.2.27.msi](C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.27.msi)，SHA-256 `3139F0EDFE545C659D40D0F54BF95E503A67F8A6E025AC41543435529FE6EB77`；包内 `bin/coolzhu-web-console.exe` SHA-256 `BDC9CA403A4B33407D9330784E9291C9980AB53912E80071C305D81D34CA9968`。
- 04:34 以完整 MSI 静默升级时，日志 `tmp/upgrade-0.2.27-msiexec-20260929.log` 第 2484 行指出 `MSI_LUA: Elevation prompt disabled for silent installs`；第 2485、2492 行给出 Windows Installer **1730**：“你必须是 Administrator 才能删除此应用程序”；`InstallInitialize` 返回 3，最终 `MainEngineThread is returning 1603`（第 2855、2867 行）。失败发生在 `RemoveExistingProducts` 旧版卸载阶段。
- 当前会话令牌为 Medium Integrity，Administrators 组仅 `deny only`，`IsAdministrator=False`。最初 PowerShell 包装命令的外层退出码 0 和空 `$LASTEXITCODE` 不能作为 MSI 成功证据；应以安装日志的 **1603** 和实际安装文件为准。
- 正式安装目录 `C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe` 仍为 SHA-256 `CDC30E489990ECD7C24E756B548AD39C49B6DD45893B56C28ED7D9CA9D224773`，与新包哈希不符。09:13 核验无 Coolzhu 或 msiexec 进程；未启动应用，也未替换单个文件。
- 停机前用户工作区摘要 `tmp/upgrade-preinstall-runtime-summary-20260929.json` 已修正末尾格式，Node/PowerShell 解析通过；原 59 个文件、90,440,400 字节、42 个附件以及 manifest SHA-256 `ddd6590ee492de7db4612f5d6fb278adc7581c6149d14a4f586b606d9943f1c3` 保持不变。

后续使用正常的管理员安装路径运行**同一个完整 MSI**，不绕过 UAC，不重复无管理员静默安装。安装成功后先核退出码、产品版本与安装目录 web-console 哈希，再由 Luna 在首次启动前核同根工作区摘要，随后进行真实 Qwen/Paint 复测。
