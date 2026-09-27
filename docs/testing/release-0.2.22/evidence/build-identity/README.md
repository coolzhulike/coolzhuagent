# 0.2.22 安装候选构建身份

2026-09-27 使用完整 `scripts/build-msi.ps1 -Version 0.2.22 -Configuration release` 构建，未使用 `Skip`；脚本退出码 0。过程日志在本机忽略目录 `tmp/candidate-0.2.22/build-msi.log`。本记录证明构建、载荷、安装包的身份与发布门结果，不代表安装或原生功能验收。构建期间未操作已安装应用或用户会话数据。

## 安装包与源码

- MSI：`dist/CoolzhuAgent-0.2.22.msi`，246,303,806 字节，SHA-256 `33FBFAC321CE8CE11A89AE883D938A78D8D8C8508CB6ABEEAF587C5EA10022D5`；重算值与安装器报告一致。当前包未签名。
- 源码快照：构建前后均为 `4669762e542b491ebb13578df9492076fd52ee3500ac88d9f529e911392514d0`，覆盖 1,221 个声明范围内文件、202,929,437 字节；逐文件比较新增、删除、修改均为 0。`live_worktree_changed=false`，`build_snapshot_integrity=verified-unchanged-live-tree`。
- 构建输入摘要：`333a2e8cdf1a23e9bf3d8467b9b08dcf935a94dd3347c639c4f3376d631278c5`。开发态 Web SHA `487CA0B41C9E97912DD5695E88C2E89E8E9E8D14175E12FC7FB38643883CA69A` 仅为源码验证身份，不能代替下表的 release Web 载荷身份。
- 发布报告 ID：`pkg-report-release-20260927-225125812-4522a133`，内容摘要 `e10e2504da3c2642d10828346da4599e973dcb94e80a52f6531e3301eee7ce4b`。四份[归档原始收据](pkg-report-release-20260927-225125812-4522a133/)均已生成；归档的发布报告、载荷清单和安装器报告与构建原件的文件哈希相同。

## 发布门与载荷

发布报告中的 `declared_source_snapshot_scope`、`source_input_stability`、`declared_roots_present`、`build_input_stability`、`artifact_provenance`、`frozen_inputs` 六项门均为 `pass`，`release_eligible=true`，无发布拒绝原因。

载荷清单记录 856 个文件、322,070,227 字节，摘要 `ae2285e8f2a98db66e24984b31f41d82167c63dffb705c11b9f468e856a6c7b3`。逐一重算 856 个文件的长度及 SHA-256 均匹配；包根连同清单自身共 857 个文件。只读 MSI 数据库的 `File` 表为 857 行、`Directory` 表为 75 行，10 个关键文件名均存在。

| 产物 | 包根目标（MSI File 表含同名项） | SHA-256 |
| --- | --- | --- |
| Web 控制台 | `bin/coolzhu-web-console.exe` | `F9C08D440CA00DD78855A1B759C3FD3586031A9340A52859AA5B1EB4DA8A384A` |
| 浏览器原生宿主 | `bin/coolzhu-browser-native-host.exe` | `8A130975DF05D39E92892B6A9AB11A1D1DE97E599BE85F31D72134F2BA1DBA8E` |
| Clawbot sidecar | `bin/coolzhu-clawbot-sidecar.exe` | `C73447EB259B1F84F0466AD57807B1B4EB1727863D4AC8B295C5B03C77068798` |
| CLI | `bin/coolzhu-cli.exe` | `167ED58038D075CA72BB69C8F7C3C612CD2613619A000E44AD5A547881EA9E50` |
| Computer Use 预检 | `bin/coolzhu-computer-use-check.exe` | `33F092CC12A01E45D343AFE6525A00FF433A45D3503B56D206775063AA29F5E6` |
| Vision smoke | `bin/coolzhu-vision-smoke.exe` | `0CEE7EA52906423441149AD10C0DABB40C9B612D9E7A95284BF19FD0E70E4245` |
| 桌面视觉 | `bin/coolzhu-latest-desktop-vision.exe` | `2B49585D5856A050E571056E7F18D44F4F60197A42119AB0C8E208022E5E9546` |
| Tauri shell | `bin/coolzhu-tauri-shell.exe` | `0ACEB1B8408C9DCAA5BF84BF2C02B4F2BE72E8230080E6BCC12E4B3B9C9FBB62` |
| WebView2 loader | `bin/WebView2Loader.dll` | `8427B1FC58EC707813E5C0A51EB5D69397BB333250A7B891BE4D3B123F1E0F1C` |
| 安装启动器 | `COOLZHU-AGENT.exe` | `F7C5D120FDFC97D8A88CB51B408F81E70703028BE254297BCF9CEAB50A79AB46` |

10 个目标的包根文件 SHA-256 均与报告一致。[安全扫描报告](CoolzhuAgent-0.2.22-package-safety.json)与 `dist` 原件哈希相同，为 `safe=true`、`findings=[]`，扫描 857 个包根文件；载荷路径和只读 MSI File/Directory 名称中无 `tmp`、会话库、SQLite、私钥或凭据类名称。四份公开收据经高置信凭据标记检查未发现命中。以上检查没有读取或打包用户实际工作区数据库与密钥。

## 同次发行 Web 受控回放

以包内 `bin/coolzhu-web-console.exe`（SHA-256 `F9C08D440CA00DD78855A1B759C3FD3586031A9340A52859AA5B1EB4DA8A384A`）在纯临时环境执行 `tests/integration/s0_controlled_replay.py --all`，退出码 0。[脱敏结果摘要](s0-release-web-summary.json)记录六类正向场景与三种白名单拒绝均达到各自预期；其中空摘要场景预期终态为 `failed`，三种拒绝预期终态为 `failed` 且持久化工具调用数为 0。使用随机本地端口、假模型和假 Key；`COOLZHU_WEB_STATIC_ROOT` 未继承，也未接触默认 8765、用户库或云端。此回放验证发行 Web 与新 CI 驱动的兼容性，不代表已安装桌面的验收，也不把 fixture 白名单当作产品沙箱。
