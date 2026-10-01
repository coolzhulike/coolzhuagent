# DSH 静态安装与依赖来源绑定实施审查

2026-10-01，主会话独立实施。承接[远程插件方案](2026-10-01-dsh-remote-plugin-runtime-plan.md)与[宿主进程桥审查](2026-10-01-dsh-host-process-implementation-review.md)。GPT-6 Pro 补审按用户决定暂停。本轮仍是工程实施，未宣称正式远程市场或真实 Qwen 调用通过。

## 先识别的架构风险与取舍

1. 插件源文件摘要正确，不代表 Node 最终导入的是宿主固定 SDK。插件安装到另一目录后，默认解析可能向上找到工作区的同名依赖。新增 `source_imports.mjs` 使用[Node官方同步模块钩子](https://nodejs.org/api/module.html#moduleregisterhooksoptions)，将首批支持的 SDK 从宿主目录解析，包内导入限定为回执文件并使用刚核验的原始字节。未知依赖明确不兼容。当前工程实测 Node24.15.0；正式分发仍须固定运行时及全部依赖文件，不能把钩子、独立进程或来源摘要称作操作系统沙箱。
2. `PluginKind` 是来源分类，不能新增一个 DSH kind 混合运行类型。保留 External，新增独立的 `DshPackage / DshSourceReceipt` 类型并记录在登记清单中；完整 scoped npm 名与固定 Git 修订保留。登记 ID 使用包名 SHA256 的前24字符，避免 npm 名中的斜线进入事务路径；页面名称读取完整 npm 名，不显示摘要 ID。
3. 读取 package.json 不代表已注册工具。静态安装不执行入口、npm、prepack 或生命周期脚本，也不造进程工具命令和工具 schema。安装始终停用；普通启用入口和浮动更新明确拒绝。在宿主启用流程接入前，外部设置中的启用位也不会使 DSH 包成为已加载工具。
4. 不能在 Web 页面复制另一套安装事务。`install_dsh` 复用 PluginManager 的同一跨进程写锁、同卷 stage/swap、registry/settings 日志和重启恢复。只复制回执文件，生成宿主登记清单；原生安装仍走原有复制校验分支。DSH 安装拒绝覆盖属于其它运行类型或仓库来源的同 ID 登记。

## 实际实施范围

| 文件/模块 | 负责的行为 |
|---|---|
| plugin-system/dsh_package.rs | 固定 GitHub 来源与40位commit、包名/版本/入口、SDK锁摘要、逐文件摘要、路径/链接/大小核验；仅复制核验文件，生成默认停用登记 |
| plugin-system/install_transaction.rs | 增加已准备静态包分支，复用原有目录交换和失败恢复，不启动下载或插件进程 |
| plugin-system/lib.rs | 保存 DSH 运行描述，静态发现核验，普通安装/启用/更新不隐式绕过专用流程 |
| dsh-plugin-host/source_imports.mjs | 固定 SDK 导入，拒绝回执外依赖，加载/收尾失败均撤销钩子 |
| extension_market/plugin_entry | 已安装 DSH 包显示真实 npm 名，仍按实际加载状态呈现 |
| 两个既有/新增工程 example | 对同一个真实官方包分别验证静态安装与生产 Rust/Node 桥；不含模型夹具 |

真实首包仍为 `omdsh-dev/dsh-tool-calculator`，固定 commit `b2007a13f06bcf75bf07b9d277ee8d434a316490`，`@deepseek-ai/dsh-tool-calculator@0.0.1`。6个来源文件按既有原始 Git blob/SHA256 身份复制；安装目录只增加 Coolzhu 自己的登记，不运行 package.json 的构建脚本。

## 已核验事实与限制

- 静态安装工程探针5组通过：登记身份与默认停用；无假原生工具/加载状态；拒绝未接入的启用和普通更新；摘要失败时旧包、登记和设置原字节不变；无残留暂存操作。
- 从实际静态安装后的新目录，经过同一生产 Rust/Node 进程桥取得真实 calculator schema并算出96；冻结修订变化拒绝。真实包文件保持完整，但删去回执中的 `lib/evaluate.js` 时，新宿主进程拒绝未核验导入并确认进程树清理。根预取消仍在进程启动前拒绝。
- 宿主库8组真实官方插件核验通过：执行、参数失败、预取消、调用身份/世代、并发、来源校验期间停用及官方fiber/service释放。
- 现有插件系统全量37通过/0失败，包含共享安装事务的交换失败、registry/settings失败及重启恢复；命令路由插件报告1通过/19过滤。这些是工程检查，不能替代正式页面截图。
- DSH特定探针的摘要失败发生在提交前，不冒称完成了新的 DSH 提交后失败或强杀恢复实操；共享事务既有回归单独记录。P2完整远程安装验收仍未完成。

原始工程回执见[063 dsh-host目录](../testing/release-0.2.63/dsh-host)，构建、回归及探针日志见[工程日志目录](../testing/release-0.2.63/evidence/engineering)。后续编译/回归与远端结果按实际完成时点更新，不沿用旧提交通过结果。

## 下一步接入约束

远程下载层须把社区条目解析为明确仓库和固定修订，核对原始来源身份、许可及全部文件，再调用静态安装入口；网页不能自行拼接 shell 安装命令。正式 Node/SDK 随包分发且验摘要，不能依赖全局 Node 或未核验 node_modules。DSH 启用须先真实加载并保存来源/配置/工具定义快照；只有已启用快照进入模型定义，不能在列目录时隐式执行插件。

工具执行须冻结模型本轮看到的快照、实际工程/聊天室/父轮/调用身份，经过原有权限和根控制后再次核验当前启用资格与完整工具定义。停用须撤资格并取消/收尾相关进程；卸载和升级不能保留旧模型快照资格。真实 qwen3.8-flash 调用与正式右栏安装/启停/卸载截图完成前，P3/P4和整项插件市场需求均保持开放。0.2.63 MSI 不含本轮新增代码。
