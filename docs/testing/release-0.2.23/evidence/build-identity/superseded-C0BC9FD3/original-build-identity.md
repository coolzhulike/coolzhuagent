# 0.2.23 首次构建身份（已被后续文案候选替代）

本记录仅对应 MSI SHA-256 `C0BC9FD3F4EC096E976C87F76807B9DE3EFA9C2B1B6BCB886A6E199EE7E1BD4B` 及 Web SHA-256 `F8159F050EE047DC2626DB479D7D1EF00D52396E7D0CE8D1FAB20B677CF25261`。它尚未安装，下面的 S0/权限回放只属于这一代 Web，不追认后续同版本重新打包的产物。四份原始打包收据位于上级目录 `pkg-report-release-20260928-005631335-1fcf2e64/`；本目录另保留本代安全报告与脱敏回放摘要。下文提到的 `dist` 原件、安全扫描及包根核对均指首次构建当时的产物，不指后续同版本报告或当前包根。

2026-09-28 完整执行 `scripts/build-msi.ps1 -Version 0.2.23 -Configuration release`，未使用 `Skip`，退出码 0。过程日志为本机忽略目录 `tmp/candidate-0.2.23/build-msi-release.log`，SHA-256 `828C49B9941BF98108F6EE0734BE98502A337E2D115EC24E3A24AD4013BBCD81`。本记录证明发行编译、包载荷和安装器身份，不代替安装后的原生验收；本次未安装应用或读取用户会话库。

## 安装器与源码

- MSI：`dist/CoolzhuAgent-0.2.23.msi`，246,311,998 字节，SHA-256 `C0BC9FD3F4EC096E976C87F76807B9DE3EFA9C2B1B6BCB886A6E199EE7E1BD4B`；重算值与安装器报告一致。本候选包未签名。
- 构建前后源码快照同为 `9fed9cffa289b62b582f0c8cd4440e85880930f6cf72c80c7c0a8e6d61cee695`，覆盖声明范围内 1,221 个文件、202,944,039 字节。两次独立采样逐文件比较新增、删除、修改均为 0；`live_worktree_changed=false`，`build_snapshot_integrity=verified-unchanged-live-tree`。另对 7 个主要产品/构建脚本文件在打包前后单独重算哈希，7/7 相同，记录留在 `tmp/candidate-0.2.23/{prebuild,postbuild}-source-hashes.json`。
- 构建输入摘要 `2a6ba020042aaac055db94c1e77343cc7b74346c4dec63c163291621e4c34328`。报告中的 Git SHA `9eae50ebd857f95ea44fd542c9a8399a374a385f` 只是工作树的参考种子提交，当前源码身份以上述快照为准。
- 发布报告 ID `pkg-report-release-20260928-005631335-1fcf2e64`，内容摘要 `0cceb63528f95375891bec86ef7c03bd0e60e76188a69eacf6f144df0f1f32d5`。[四份归档收据](../pkg-report-release-20260928-005631335-1fcf2e64/)已生成；其中发布报告、载荷清单、安装器报告与构建原件的文件哈希一致。
- 包内 CLI `--version` 和 MSI `ProductVersion` 均为 `0.2.23`；MSI `ProductName` 为 `COOLZHU CODE Agent`。

## 发布门、载荷与安全

发布报告的 `declared_source_snapshot_scope`、`source_input_stability`、`declared_roots_present`、`build_input_stability`、`artifact_provenance`、`frozen_inputs` 六门均为 `pass`，`release_eligible=true`，无拒绝原因。

载荷清单列出 856 个文件、322,097,875 字节，摘要 `ca42cd4e6682308c233928cf4c63c9b4f9ef17eb25705d0e5b055249ef8dffec`。逐一重算 856 个包内文件的长度与 SHA-256 均匹配；加上清单自身共 857 个包根文件。只读 MSI 数据库的 `File` 表 857 行、`Directory` 表 75 行，10 个关键载荷文件名全部存在。

| 产物 | 包内路径 | 字节 | SHA-256 |
| --- | --- | ---: | --- |
| Web 控制台 | `bin/coolzhu-web-console.exe` | 36,593,152 | `F8159F050EE047DC2626DB479D7D1EF00D52396E7D0CE8D1FAB20B677CF25261` |
| 浏览器原生宿主 | `bin/coolzhu-browser-native-host.exe` | 2,851,328 | `8A130975DF05D39E92892B6A9AB11A1D1DE97E599BE85F31D72134F2BA1DBA8E` |
| Clawbot sidecar | `bin/coolzhu-clawbot-sidecar.exe` | 5,672,448 | `C73447EB259B1F84F0466AD57807B1B4EB1727863D4AC8B295C5B03C77068798` |
| CLI | `bin/coolzhu-cli.exe` | 11,657,728 | `F51C4A97A46FC8DA29B6B193211E0524D54D010942C0C648D1C322BC71064B0F` |
| Computer Use 预检 | `bin/coolzhu-computer-use-check.exe` | 394,240 | `33F092CC12A01E45D343AFE6525A00FF433A45D3503B56D206775063AA29F5E6` |
| Vision smoke | `bin/coolzhu-vision-smoke.exe` | 4,485,632 | `0CEE7EA52906423441149AD10C0DABB40C9B612D9E7A95284BF19FD0E70E4245` |
| 桌面视觉 | `bin/coolzhu-latest-desktop-vision.exe` | 4,467,712 | `2B49585D5856A050E571056E7F18D44F4F60197A42119AB0C8E208022E5E9546` |
| Tauri shell | `bin/coolzhu-tauri-shell.exe` | 85,188,608 | `0ACEB1B8408C9DCAA5BF84BF2C02B4F2BE72E8230080E6BCC12E4B3B9C9FBB62` |
| WebView2 loader | `bin/WebView2Loader.dll` | 160,320 | `8427B1FC58EC707813E5C0A51EB5D69397BB333250A7B891BE4D3B123F1E0F1C` |
| 安装启动器 | `COOLZHU-AGENT.exe` | 998,400 | `470C8A728E5E79FA419443685D35FE89F9E5A51C25E5CC610F1979FB56B378E9` |

[安全扫描报告](CoolzhuAgent-0.2.23-package-safety.json)与 `dist` 原件哈希相同，`safe=true`、`findings=[]`，扫描 857 个包根文件。载荷路径中未发现临时目录、会话库、SQLite、私钥或凭据类名称；四份归档收据的高置信密钥标记扫描命中文件数为 0。只读验证脚本首次运行时尚未复制安全报告到本归档，因归档文件缺失而中止；复制与 `dist` 哈希相同的安全报告后完整重跑通过，最终日志为 `tmp/candidate-0.2.23/verify-release.log`，SHA-256 `CB08D9F65F7B3FAB044E864683D4E0C369BD99CCB3B3BBE6B78097C2F917115C`。

## 同次 release Web 受控回放

以包内 Web（SHA-256 `F8159F050EE047DC2626DB479D7D1EF00D52396E7D0CE8D1FAB20B677CF25261`）在纯临时环境运行 `tests/integration/s0_controlled_replay.py --all`，退出码 0。[脱敏 S0 摘要](s0-release-web-summary.json)记录六类正向和三种白名单拒绝全部达到预期。空摘要场景的预期终态为 `failed`；三项拒绝的预期终态亦为 `failed`，持久化工具调用数均为 0。使用随机本地端口、假模型和假 Key，环境白名单不继承 `COOLZHU_WEB_STATIC_ROOT`，未接触默认 8765、用户库或云端。三项拒绝证明该 fixture 的白名单断言，不代表产品沙箱。

同一 release Web 的真实 HTTP 双工程回放亦退出 0。[脱敏权限摘要](permission-release-web-summary.json)记录：A、B 工程都有固定 `main-room`；切至 B 后旧 A、缺字段、错误路径三种权限 PATCH 均返回 409，B 权限行保持未写；A/B 正确 scope 与完全访问双确认仍可保存。Web 副本、HOME、配置、日志和临时目录均隔离在 `tmp/`，使用随机本地端口并禁用桌面壳。此回放仅证明本次权限端点的工程归属契约，不扩展到诊断偏好端点或已安装桌面的验收。
