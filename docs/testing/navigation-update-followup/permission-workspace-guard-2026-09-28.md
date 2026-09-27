# 聊天室权限提交的工程归属守卫（2026-09-28）

## 缺口与修复

默认会话库随工程隔离，但每个工程的默认聊天室都使用固定 ID `main-room`。原 `PATCH /api/chat/rooms/{room_id}/permissions` 在处理时取得工程 pin，再按**当时**的聊天室列表和会话库写入。pin 可以阻止处理中途切换工程，却不能证明请求是在当前工程发起；A 工程发出的权限请求若在切至 B 后才进入处理器，B 的 `main-room` 也存在，可能把 A 的授权意图写入 B。

权限请求现新增可反序列化的 `expected_workspace` 字段。处理器取得 pin 后、读取聊天室或写库之前，调用已有的 `bound_project_mutation_root` 核对客户端声明与当前工程路径；缺字段和错误路径均返回 HTTP 409。该字段仅用于核对，不选择会话库。原有完全访问双重确认、权限值校验、审计与 pin 生命周期不变。三个只测试权限值校验的现有请求构造补了字段。

前端四个权限 PATCH（主权限保存、诊断弹窗内的权限保存、授权、撤销）均发送操作发起时捕获的 `roomId` 和 `scope.workspace`；诊断弹窗固定打开时的聊天室和工程，保存前及迟到响应均复核。权限读取失败时顶栏、任务状态及右栏显示“状态未知”，禁止基于默认档位误保存；未选房间显示“未选择”。调试开放权限的有效状态说明仍单独显示。顶栏的单套权限选择器随 `details` 弹层在 Esc、外点、切房与保存成功时关闭。

本轮服务端归属校验仅覆盖上述**权限 PATCH**。诊断偏好自己的 PATCH 沿用原端点契约，前端已固定其发起时房间并挡住已知的切换后保存；不把它表述成具有相同的服务端工程归属守卫。

## 验证状态

源码冻结后，`cargo build -p coolzhu-web-console --offline` 成功（41.33 秒）。同一 debug Web `target/debug/coolzhu-web-console.exe` 的 SHA-256 为 `E2CFA090FE371EF34B8FCE147ECDA134BC4C608AFC8F78CA1E6A5226A648A28C`，大小 59,687,936 字节；构建日志位于 `tmp/candidate-0.2.23/debug-build.log`，SHA-256 为 `BEC776328A3F1CBAB9F5BE7D8369E600132C9C36F9414AE42C70F1B37FB332A6`。`node --check` 与 `git diff --check` 均通过。

两条定向 Rust 测试分别通过：`goal_command::tests::gate_exit_and_expired_root_use_the_same_managed_path` 为 `1 passed`、用时 0.52 秒；`tests::chat_room_full_access_requires_both_risk_confirmations` 为 `1 passed`。日志分别在 `tmp/candidate-0.2.23/goal-targeted-test.log`（SHA-256 `39A919101AF13A635ACF2FB970CABDEB7407E023EFB51561A1E00FBA9529017E`）与 `tmp/candidate-0.2.23/permission-targeted-test.log`（SHA-256 `6E1D095D9DCE4D854EE729C4D165ABFC3770D0BEEAE404FFF2D1542417EAA32F`）。

同一 debug Web 的真实 HTTP 双工程回放通过。Web 副本、HOME、配置、日志、TMP/TEMP/TMPDIR 与 A/B 工程均在 `tmp/`，随机 127.0.0.1 端口，设 managed 标记并禁用桌面壳。A、B 都有固定 `main-room`：A 的正确工程字段加双确认可保存；切至 B 后，旧 A、缺字段、错误路径三个权限 PATCH 均为 HTTP 409，逐次核对 B SQLite 权限行仍为空；B 的正确工程字段加双确认可保存，切回 A 原记录保留。脱敏结果为 `tmp/candidate-0.2.23/permission-scope-c80630ef9ab7/result.json`（SHA-256 `EFBCF7D1FC98C7CC7EC7436A24FEBAE29D78E5357EB578274DB4BF271F3791DD`），控制台日志 SHA-256 为 `9FB3A5CEBEC003293A77ADFD23607DC3F7B4A9E3F3F741D8E11F4EBCC683BC4C`。首次运行仅临时驱动重复创建 HOME 目录报 `FileExistsError`；改为允许已存在目录后复跑全过，原失败日志仍留 `tmp/`，不计作产品缺陷。

随后视觉代理仅将低高度卷轴顶部间距从 8px 调整为 18px，并离线重编译，当时 debug Web SHA-256 为 `D387B7D83009D3BEC30F07F53E2C61F4D47A88E636A9BAB093B07E91DB9F2C96`。上文两条定向测试和首次 HTTP 回放是调整前的 debug 身份，不追认为该次 debug 的重新执行结果。

第一次完整 `0.2.23` release 打包成功，包内 Web SHA-256 `F8159F050EE047DC2626DB479D7D1EF00D52396E7D0CE8D1FAB20B677CF25261`。该同次 release Web 的第二次真实 HTTP 双工程回放退出码 0，旧 A、缺字段、错路径均为 409 且 B 未写入，正确 scope 与双确认仍成功；[该候选脱敏结果](../release-0.2.23/evidence/build-identity/superseded-C0BC9FD3/permission-release-web-summary.json)和[历史身份](../release-0.2.23/evidence/build-identity/superseded-C0BC9FD3/original-build-identity.md)已归档。`tmp/candidate-0.2.23/permission-release-web.log` SHA-256 为 `0D2595EC644E529B02AE6FFBE2F0A724F2ACDBE4677F0D7432F3C1D69A44F501`。该包未安装，后因用户要求简短权限摘要统一显示“完全访问”而被替代。

文案窄改后 `node --check`、`git diff --check` 和 `cargo build -p coolzhu-web-console --offline` 再次通过，debug Web SHA-256 `D7632EDB3C778A237FBD81BE5666C2566F5049E389A8FE704A718DD671FE68D0`；构建日志 `tmp/candidate-0.2.23/debug-build-full-access-copy.log` SHA-256 `E2431E7C88CF8497129C5F3959725CAA7477A3E1C621FCE29AB9B84C0C7DAE92`。同版本完整 release 重新打包，最终候选包内 Web SHA-256 `D18E0CE4F1442683597550AD9D7CF913EEB21FA9DF51ED65171C398E1EF83EAC`。对此同次 Web 再跑真实 HTTP 双工程回放，退出码 0，三种错误 scope 仍为 409 且 B 不受影响，正确 scope 与双确认成功；[最终脱敏结果](../release-0.2.23/evidence/build-identity/permission-release-web-summary.json)与[最终发行身份](../release-0.2.23/evidence/build-identity/README.md)已归档。`tmp/candidate-0.2.23/permission-release-web-copyfix.log` SHA-256 `B3E3A64A001BFC98B4CF4718AC66A623D0E6F88BED3A6B65C9420C761185B1B8`。回放脚本保留在被忽略的 `tmp/permission-workspace-replay.py`，不进入产品提交；原生安装后的权限交互仍由独立验收记录确认。
