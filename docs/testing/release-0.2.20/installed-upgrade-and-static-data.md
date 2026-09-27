# 0.2.20 候选包安装与静止数据核对

日期：2026-09-27。本记录对应本机从已安装 0.2.19 升级至 **未签名、未正式发布**的 0.2.20 候选包；不代表原生功能验收通过。构建身份见 [封包证据](evidence/build-identity/)；0.2.19 的安装版增量实操另见 [前版记录](../navigation-update-followup/installed-0.2.19/native-acceptance-incremental.md)。

安装前通过界面切回原用户工程，正常关闭原生应用；进程及 8765 监听均已退出。随后以本轮私有备份脚本执行全新 `pre` 快照，先复核待安装 MSI 哈希与旧版注册身份，再用 Windows 正常管理员授权的可见 `/passive /norestart` MSI 升级。安装进程退出码为 0，MSI 日志中的服务端及客户端 `MainEngineThread` 均返回 0。未沿用前次非提升静默安装失败路径，也未删除旧程序或用户目录。

| 身份项 | 实测结果 |
| --- | --- |
| MSI SHA-256 | `3BA0D9B0893123FEF9A4694BD567399029A74BE55275D8040212BDCE180F6542`，与封包报告一致 |
| 升级前注册版本 | `0.2.19` |
| 升级后注册版本 | `0.2.20` |
| 安装产物 | 封包报告所列 10 个关键文件全部存在，逐项 SHA-256 为 10/10 匹配 |
| 安装 Web SHA-256 | `6AD5CB4B595A3BBF582D3BEECACC964718A2648C344BA769695864BC7BA6861F` |
| 安装 CLI SHA-256 | `2F757A50A75BFA4ACF03B43963230D9B2F5793B2C481079B3F6A95F2B6F35989` |

升级前快照和**安装后、首次启动前**快照均有 66 个文件、41 个附件（49,153,852 字节）及 6 个 SQLite 数据库。工作区数据、用户数据、输入安全状态与配置四组来源的文件清单摘要相同：`c7fa9af50cd11319d9105cddd509b9e3b1ef82a4557b0e5638d6625077fa88d9`；6 个数据库的白名单表行数及带盐语义摘要、配置摘要亦逐项相同。SQLite 采用一致性备份；活动 WAL/SHM 未当作独立内容复制。私有快照和摘要密钥留在忽略的 `tmp/candidate-0.2.20/upgrade-20260927-candidate-020/`，可能含历史用户数据与敏感凭据，不进入公开提交；跨用户或跨设备恢复仍受同用户 DPAPI 条件限制。

可复核的本地收据：`tmp/candidate-0.2.20/identity-summary.md`、`msi-upgrade-process.json`、`msi-upgrade-elevated.log`、`installed-identity.json`，以及该私有快照目录中的 `pre/inventory.json` 与 `post/inventory.json`。这些收据证明**静止安装与数据保留**，不能替代首次启动后的语义核对。

正式安装入口 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe` 已发起启动，启动命令与进程号记于 `tmp/candidate-0.2.20/native-start.json`。随后的原生窗口枚举时，Computer Use 明确返回“用户按物理 Escape 停止”；该轮随即停止所有桌面操作。**截至那次中止时**尚无 0.2.20 原生窗口截图；后续恢复与实测见下文。不得用 0.2.19 的 `403` 失败、0.2.20 源码/隔离 HTTP 成功或本报告的安装身份代替原生结论。

用户随后明确要求“继续”。2026-09-27 约 21:00（北京时间）恢复原生操作时，先重新观察已安装窗口，没有重装；安装 Web 二进制 SHA-256 仍为 `6AD5CB4B595A3BBF582D3BEECACC964718A2648C344BA769695864BC7BA6861F`。原用户工程和既有 0.2.18 Qwen 测试消息、图片、耗时在原生首屏可见；它们是历史消息，不能算 0.2.20 新模型调用。该首屏原图含用户既有会话，留在本机，**不纳入公开 PR 证据**。

首次启动后的私有 `post-runtime` 一致性快照与启动前 `post` 比较：仍为 66 个文件、41 个附件，配置相同；消息、房间、附件等业务表未新增跨房间镜像。活动数据库的字节摘要并非全部相同：`web-sessions.sqlite3` 的 `memory_access` 仍为 85 行，其中 2 行的最近访问时间及计数更新；`input-safety.sqlite3` 的事件从 87 增至 88，ownership epoch 为 2 行但摘要更新。它们按本次运行副作用分别记录，不把全库说成静止不变，也不删除或自动放行新增的输入安全事件。脱敏结果在私有 `tmp/candidate-0.2.20/runtime-diff-summary.json`；原始库与快照不公开。

原生工程文件入口已打开隔离 Rust 工程的 `src/main.rs`，[手动启动入口](installed-native/01-rust-file-manual-start.png)可见；点击后得到 [HTTP 409 工作区已切换](installed-native/02-lsp-start-409-after-workspace-switch.png)。按提示刷新一次后停止重试。源码核对定位为前端将经过编码及截断的草稿工程键作为 `expected_workspace` 提交，而后端要求真实工程路径；0.2.20 因此未启动 Rust 服务，诊断、定义、引用、停止与切换失效均不能判通过。后续候选包需重新走完整原生链。

聊天视频只完成正常 UI 打开“附件”的 Windows [文件选择框现场](installed-native/03-video-picker-target-blocked.png)。Computer Use 将选择框点击点判为非目标 WebView 子进程，重新聚焦、刷新后仍拒绝；`list_windows()` 未列出可单独绑定的该对话框。已用 Escape 取消，未选入视频、未发送或形成可点击的视频消息。截图只证明对话框现场，具体工具拒绝来自本轮工具回执；不能据此断言产品不接收视频。最后已通过界面切回原用户工程，测试聊天室没有未保存草稿。
