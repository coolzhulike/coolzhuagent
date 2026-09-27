# 0.2.21 安装候选构建身份

2026-09-27 使用完整 `scripts/build-msi.ps1 -Version 0.2.21 -Configuration release` 构建，未使用 Skip；脚本退出码 0。过程日志保存在本机忽略目录 `tmp/candidate-0.2.21/build-msi.log`。本记录只证明构建、载荷与安装包身份，不代表安装或原生功能验收。构建期间未操作已安装应用或用户会话数据。

## 安装包与源码

- MSI：`dist/CoolzhuAgent-0.2.21.msi`，244,112,223 字节，SHA-256 `00C98A6FA78CA4A1BCAAE93FAD6AE9E4B95E46D2BDEAC39D896CD6EF181D8AAC`；重算值与安装器报告一致。当前包未签名。
- 源码快照：构建前后均为 `beda1d27d98d4a4f02d1588094afc887ac848885a8ad30095578eb005e377eaa`，覆盖 1,218 个声明范围内文件、200,669,660 字节；逐文件比较新增、删除、修改均为 0。`live_worktree_changed=false`，`build_snapshot_integrity=verified-unchanged-live-tree`。
- 构建输入摘要：`915859596b732b0b9e3ceec6d3ce2385313e8a0836a4d95a9d0150ef0a4ce923`。先前开发态 Web SHA `C2CE91124626F80609758C67494B21BC7831C311133261A83DFB84EC6E9ECFC6` 仅为源码验证身份，不能代替本表中的 release Web 载荷身份。
- 发布报告 ID：`pkg-report-release-20260927-205320598-3c795f92`，内容摘要 `2c81c6da41aa04a9593a4e00dd084f837e7e6cbafb67589f345ae49910c2d060`。四份[归档原始收据](pkg-report-release-20260927-205320598-3c795f92/)均已生成；归档的发布报告、载荷清单和安装器报告与构建原件的文件哈希相同。

## 发布门与载荷

发布报告中的 `declared_source_snapshot_scope`、`source_input_stability`、`declared_roots_present`、`build_input_stability`、`artifact_provenance`、`frozen_inputs` 六项门均为 `pass`，`release_eligible=true`，无发布拒绝原因。

载荷清单记录 854 个文件、319,877,038 字节，摘要 `2eb7651c5c3952bbce13c4f6544bb46bab318b2640c334c4c1315bccb4e69d6e`。逐一重算 854 个文件的长度及 SHA-256 均匹配；包根连同清单自身共 855 个文件。只读 MSI 数据库的 `File` 表为 855 行、`Directory` 表为 75 行，10 个关键文件名均存在。

| 产物 | 包根目标（MSI File 表含同名项） | SHA-256 |
| --- | --- | --- |
| Web 控制台 | `bin/coolzhu-web-console.exe` | `A55CD7ED616728211FCE93F4239ED2FBE128687EBBA09FDB15FB2984A46E521B` |
| 浏览器原生宿主 | `bin/coolzhu-browser-native-host.exe` | `8A130975DF05D39E92892B6A9AB11A1D1DE97E599BE85F31D72134F2BA1DBA8E` |
| Clawbot sidecar | `bin/coolzhu-clawbot-sidecar.exe` | `C73447EB259B1F84F0466AD57807B1B4EB1727863D4AC8B295C5B03C77068798` |
| CLI | `bin/coolzhu-cli.exe` | `AE111A62C205DFDFCFAC1AD10A40CF23E83DB8C7003652553A94C7981FDBF1C3` |
| Computer Use 预检 | `bin/coolzhu-computer-use-check.exe` | `33F092CC12A01E45D343AFE6525A00FF433A45D3503B56D206775063AA29F5E6` |
| Vision smoke | `bin/coolzhu-vision-smoke.exe` | `0CEE7EA52906423441149AD10C0DABB40C9B612D9E7A95284BF19FD0E70E4245` |
| 桌面视觉 | `bin/coolzhu-latest-desktop-vision.exe` | `2B49585D5856A050E571056E7F18D44F4F60197A42119AB0C8E208022E5E9546` |
| Tauri shell | `bin/coolzhu-tauri-shell.exe` | `992B9948811E36E978CF589358870382298FC65C0578AA13F5C5E8ED25E85202` |
| WebView2 loader | `bin/WebView2Loader.dll` | `8427B1FC58EC707813E5C0A51EB5D69397BB333250A7B891BE4D3B123F1E0F1C` |
| 安装启动器 | `COOLZHU-AGENT.exe` | `04BB3B67E21A31E0B8BEC5ED97C4AD847229AB7167F7D91D7F297D78FE82F528` |

10 个目标的包根文件 SHA-256 均与报告一致。[安全扫描报告](CoolzhuAgent-0.2.21-package-safety.json)与 `dist` 原件哈希相同，为 `safe=true`、`findings=[]`，扫描 855 个包根文件；载荷路径和只读 MSI File/Directory 名称中无 `tmp`、会话库、SQLite、私钥或凭据类名称。四份公开收据经高置信凭据标记检查未发现命中。以上检查没有读取或打包用户实际工作区数据库与密钥。
