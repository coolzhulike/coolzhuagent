# 0.2.23 安装候选构建身份

2026-09-28 文案窄改后完整执行 `scripts/build-msi.ps1 -Version 0.2.23 -Configuration release`，未使用 `Skip`，退出码 0。过程日志为本机忽略目录 `tmp/candidate-0.2.23/build-msi-release-copyfix.log`，SHA-256 `6558978E2F266AE875D2B69EDDB9C181D8DDC4BF5901F4BD06F8CB2BF11ED20A`。本记录指向**带时间戳文件名的新候选**；同版本先前 MSI `C0BC9FD3…` 尚未安装，已被文案修正版替代，其[独立历史收据](superseded-C0BC9FD3/original-build-identity.md)和回放保留。本文证明发行编译、包载荷和安装器身份，不代替安装后的原生验收；本次打包未读取用户会话库。

## 安装器与源码

- MSI：`dist/CoolzhuAgent-0.2.23-20260928-010934.msi`，246,295,614 字节，SHA-256 `0BE92F1A549A8677A95742C20890E059DA54ABA18801BCC47880697EFC9EAB65`；重算值与安装器报告一致。本候选包未签名。旧的无时间戳 `dist/CoolzhuAgent-0.2.23.msi` 仍是被替代候选，不应混用。
- 构建前后源码快照同为 `b910ca61d3d01a16c3c358072e824199777409e274835877ea12556f3f434e09`，覆盖声明范围内 1,221 个文件、202,943,979 字节。两次独立采样逐文件比较新增、删除、修改均为 0；`live_worktree_changed=false`，`build_snapshot_integrity=verified-unchanged-live-tree`。另对 7 个主要产品/构建脚本文件在打包前后单独重算哈希，7/7 相同，记录留在 `tmp/candidate-0.2.23/{prebuild,postbuild}-copyfix-source-hashes.json`。
- 构建输入摘要 `01bae9cfa5c02b3950c6f62ca5ef8d4c0d7d4e7bbfdfa94f9304bbafd8cc0514`。报告中的 Git SHA `9eae50ebd857f95ea44fd542c9a8399a374a385f` 只是工作树的参考种子提交，当前源码身份以上述快照为准。
- 发布报告 ID `pkg-report-release-20260928-010933327-26497e00`，内容摘要 `2bc9df3d6cdad689f7a7bf1dec68e95c1c946741a0f6c37eca929f60b44adc2f`。[四份归档收据](pkg-report-release-20260928-010933327-26497e00/)已生成；其中发布报告、载荷清单、安装器报告与构建原件的文件哈希一致。
- 本目录的 `.gitattributes` 对 `*.json` 禁用文本换行转换，确保 Git 归档保留构建收据的原始字节；本次 14 份 JSON 的索引对象与本地归档逐份同长同 SHA-256，两代报告引用的文件哈希亦匹配。更早版本的既有归档是否受换行规范化影响，需另行核对，本记录不追认其字节身份。
- 包内 CLI `--version` 和 MSI `ProductVersion` 均为 `0.2.23`；MSI `ProductName` 为 `COOLZHU CODE Agent`。

## 发布门、载荷与安全

发布报告的 `declared_source_snapshot_scope`、`source_input_stability`、`declared_roots_present`、`build_input_stability`、`artifact_provenance`、`frozen_inputs` 六门均为 `pass`，`release_eligible=true`，无拒绝原因。

载荷清单列出 856 个文件、322,097,875 字节，摘要 `aff835a600cd175187f1e0afa487efeb1b0826c6fb47f1b15a30358ad3941a5a`。逐一重算 856 个包内文件的长度与 SHA-256 均匹配；加上清单自身共 857 个包根文件。只读 MSI 数据库的 `File` 表 857 行、`Directory` 表 75 行，10 个关键载荷文件名全部存在。

| 产物 | 包内路径 | 字节 | SHA-256 |
| --- | --- | ---: | --- |
| Web 控制台 | `bin/coolzhu-web-console.exe` | 36,593,152 | `D18E0CE4F1442683597550AD9D7CF913EEB21FA9DF51ED65171C398E1EF83EAC` |
| 浏览器原生宿主 | `bin/coolzhu-browser-native-host.exe` | 2,851,328 | `8A130975DF05D39E92892B6A9AB11A1D1DE97E599BE85F31D72134F2BA1DBA8E` |
| Clawbot sidecar | `bin/coolzhu-clawbot-sidecar.exe` | 5,672,448 | `C73447EB259B1F84F0466AD57807B1B4EB1727863D4AC8B295C5B03C77068798` |
| CLI | `bin/coolzhu-cli.exe` | 11,657,728 | `759018CC749EC4C7F5DF2335A3F0DACB07EFC3B9E6DB5F258EEC5C2694BFCA74` |
| Computer Use 预检 | `bin/coolzhu-computer-use-check.exe` | 394,240 | `33F092CC12A01E45D343AFE6525A00FF433A45D3503B56D206775063AA29F5E6` |
| Vision smoke | `bin/coolzhu-vision-smoke.exe` | 4,485,632 | `0CEE7EA52906423441149AD10C0DABB40C9B612D9E7A95284BF19FD0E70E4245` |
| 桌面视觉 | `bin/coolzhu-latest-desktop-vision.exe` | 4,467,712 | `2B49585D5856A050E571056E7F18D44F4F60197A42119AB0C8E208022E5E9546` |
| Tauri shell | `bin/coolzhu-tauri-shell.exe` | 85,188,608 | `0ACEB1B8408C9DCAA5BF84BF2C02B4F2BE72E8230080E6BCC12E4B3B9C9FBB62` |
| WebView2 loader | `bin/WebView2Loader.dll` | 160,320 | `8427B1FC58EC707813E5C0A51EB5D69397BB333250A7B891BE4D3B123F1E0F1C` |
| 安装启动器 | `COOLZHU-AGENT.exe` | 998,400 | `887C0442B8B1B7B3CA7B12B6D695B68446C34447617C63714903E7917321E12F` |

[安全扫描报告](CoolzhuAgent-0.2.23-package-safety.json)与 `dist` 原件哈希相同，`safe=true`、`findings=[]`，扫描 857 个包根文件。载荷路径中未发现临时目录、会话库、SQLite、私钥或凭据类名称；四份归档收据的高置信密钥标记扫描命中文件数为 0。只读完整复核日志为 `tmp/candidate-0.2.23/verify-release-copyfix.log`，SHA-256 `84911CE1E2E817B82A108AE925489110276F24CB2F7095CE71B62F461C9E8E33`。

## 同次 release Web 受控回放

以新包内 Web（SHA-256 `D18E0CE4F1442683597550AD9D7CF913EEB21FA9DF51ED65171C398E1EF83EAC`）在纯临时环境重新运行 `tests/integration/s0_controlled_replay.py --all`，退出码 0。[脱敏 S0 摘要](s0-release-web-summary.json)记录六类正向和三种白名单拒绝全部达到预期。空摘要场景的预期终态为 `failed`；三项拒绝的预期终态亦为 `failed`，持久化工具调用数均为 0。使用随机本地端口、假模型和假 Key，环境白名单不继承 `COOLZHU_WEB_STATIC_ROOT`，未接触默认 8765、用户库或云端。三项拒绝证明该 fixture 的白名单断言，不代表产品沙箱。

同一 release Web 的真实 HTTP 双工程回放亦退出 0。[脱敏权限摘要](permission-release-web-summary.json)记录：A、B 工程都有固定 `main-room`；切至 B 后旧 A、缺字段、错误路径三种权限 PATCH 均返回 409，B 权限行保持未写；A/B 正确 scope 与完全访问双确认仍可保存。Web 副本、HOME、配置、日志和临时目录均隔离在 `tmp/`，使用随机本地端口并禁用桌面壳。此回放仅证明本次权限端点的工程归属契约，不扩展到诊断偏好端点或已安装桌面的验收。
