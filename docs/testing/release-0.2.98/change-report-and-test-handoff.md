# 0.2.98 改动与针对性验收交接

## 改动及原因

修复右栏手动终端三项实际问题：PSReadLine吞掉补充平面字符；复开侧栏未恢复已有终端；已退出终端只读取缓存第一页，刷新后缺失输出末尾。仅在应用创建的手动PowerShell启动时卸载该进程内PSReadLine，不改全局模块、用户Profile或Devin登录进程。右栏已有完整命令输入框，保留真实ConPTY、Ctrl+C和子程序控制台。

前端在打开侧栏时恢复真实终端；已连接时保留增量投影，并校验epoch、句柄和cursor，防重叠恢复/轮询覆盖新状态。已退出终端继续分页到空页，运行中的轮询节奏保持原值。后端进程监督与前端投影各自负责生命周期和显示。

## 正式身份与安装

- 冻结提交`cd80d492729b8b989d50a90ed3594b2df0503ee7`，dirty=false；源码快照`86dbceff1607e34c93ef6b2eef26c36502cc34a21e8d7a837c11b064e0b6f4d0`。正常完整release构建，六项发布门禁pass。
- MSI 285765127字节，SHA256 `5644d71775d303ad81dd2e6b1450b1e3407477e9c08628f1b99e7b404123f1a8`；payload1159文件、432523192字节，未签名。
- 正常管理员安装返回0，Program Files全部1159文件长度/SHA一致，CLI版本0.2.98/Git SHA与冻结提交一致。正式配套二进制启动，没有静态资源覆盖或测试浏览器参数。
- [包身份](installed-validation/package-098-verification.json)、[安装核验](installed-validation/installed-098-verification.json)、[安装收据](installed-validation/install-098-result.json)、[异常重启后的进程身份](installed-validation/installed-098-standard-processes.json)。出包摘要中“尚未安装”为当时阶段，不覆盖后续安装事实。
- [生产者报告](evidence/build-identity/pkg-report-release-20261008-123714646-18dd547d/package-report.json)、[payload清单](evidence/build-identity/pkg-report-release-20261008-123714646-18dd547d/payload-inventory.json)。

## 正式实操通过项

全部使用产品真实HTTP终端入口、真实PowerShell/ConPTY及原生桌面截图。未经终端UI输入命令，没有模型响应夹具。本专项是手动终端子系统验收，新增模型请求为0，不宣称验证了模型调用终端链。原SWE聊天室、唯一`island-kayak`绑定及安全历史保持。

1. 字面量`竹林𠮷😀`实际执行和显示完整。18000行输出超过1MiB缓存，返回1048295字节、17页，每页≤80KiB、无替换符，truncated=true，读到FLOOD结束标记。[结果](installed-validation/phase1-result.json)。
2. 同句柄与进程变量恢复、120×40 resize通过。外房间输入及错误句柄读取HTTP409，拒绝标记未执行。侧栏打开恢复真实运行输出，显示截断通知及FLOOD/RESTORED/Unicode尾部。[实拍](installed-validation/terminal-restored-unicode.jpg)。
3. 自有终端执行`[Environment]::Exit(23)`，API closed=true、UI“终端已退出”。23仅请求值，API不返回独立退出码。正常刷新和复开侧栏后，已退出17页缓存仍恢复到尾部。[退出回执](installed-validation/exit-result.json)、[刷新后实拍](installed-validation/terminal-closed-restored-tail.jpg)。
4. start重建不同句柄，旧变量不存在、旧句柄409；正常close新终端后status absent、新句柄409。[重建与关闭](installed-validation/phase3-result.json)。
5. 正式宿主受控异常退出：只对路径/创建时间/SHA已确认的本轮正式后台执行TerminateProcess。预先持有自有真实PowerShell后代的Win32句柄，WaitForSingleObject从258变为0，独立证明Job后代确实退出。重启同一正式后台后terminal absent、旧句柄409，三张模型台账计数和唯一云绑定不变。没有直接修改安全库；未把此声明外推为启动内部零安全写入。[原始结果](installed-validation/crash-restart-result.json)、[采证源](installed-validation/crash-restart.py)、[重启后实拍](installed-validation/terminal-after-host-restart.jpg)。

## 回归及边界

进程监督库54通过/0失败/3既有忽略；额外既有真实ConPTY检查1通过，包含实际Unicode文件输出、Ctrl+C恢复及持有后代句柄确认Job关闭。完整主控制台1407通过/0失败/6既有忽略；随后最终cursor重叠响应守卫经offline build和本版正式分页恢复实测。没有将此前完整回归冒称在最后一行修改后重跑。候选失败和修复前截图见[候选专项](../2026-10-08-terminal-lifecycle/report.md)。

构建提交的[push CI](installed-validation/ci-push-cd80d49.json)与[PR CI](installed-validation/ci-pr-cd80d49.json)均success。跨工作区完整归属矩阵、LSP与全屏TUI仍缺专项；不得把本次通过等同整个5.4工作包完成。

已公开[0.2.98 GitHub预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.98)，四资产服务器端长度/SHA均匹配本地，实际标签绑定构建提交cd80d49。[发布记录](installed-validation/github-published-metadata.json)、[标签核验](installed-validation/github-tag.json)、[本地摘要](installed-validation/release-assets-local.json)。预发布和未签名状态保持，不代表整体已验收。

下一优先项仍是Browser新nativeTarget替换及跨来源commit恰在按下期间的窄时序。已有关闭/切设置和导航发起证据不替代它们。其余会话附件、动态UI、高DPI、调度及架构矩阵按[总体清单](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)继续。ChatGPT订阅最小真实连通已独立通过，不扩充为正式Provider；Paint免测、微信不动、Opus暂停，不使用子代理，整体Goal保持active。
