> **本轮静态安装实施更新**：P2静态核验/暂存分支已复用现有事务，真实官方包安装默认停用，安装后的新目录通过生产进程桥计算96；固定SDK解析、回执外依赖拒绝与清理已工程核验。P2远程下载与DSH特定提交后中断实操、P3模型快照/真实Qwen、P4正式按钮及固定运行时分发仍未完成。[详细审查](2026-10-01-dsh-static-install-review.md)。下方阶段表保留完整目标。

# DSH 远程插件安装与运行适配实施方案

2026-10-01。主会话根据现有代码与 DSH 官方固定源码审查；GPT-6 Pro 补审按用户决定暂停。本文是实施方案；P1首包及独立宿主/Rust进程桥已在真实官方计算器上工程验证，正式远程安装和模型调用尚未完成。[本次实施审查与边界](2026-10-01-dsh-host-process-implementation-review.md)保留职责、取消、来源修订和进程世代取舍。

## P1 首包实际结果与版本决策

真实候选为 [omdsh-dev/dsh-tool-calculator](https://github.com/omdsh-dev/dsh-tool-calculator/tree/b2007a13f06bcf75bf07b9d277ee8d434a316490)，固定 commit b2007a13f06bcf75bf07b9d277ee8d434a316490，版本0.0.1、private=true。不能用同名 npm 包假定已公开发行，P2 应接确认的固定 Git/source bundle。原始 package、三个可执行 lib 文件、patch 和 MIT 许可证已逐项 Git blob/SHA256 核验，没有重写计算器或造 ctx 服务。

| 真实对象 | 必须服务及配置 | 已验证范围 |
|---|---|---|
| Calculator module | inject=['tools']；没有 Config；注册 calculator，expression 为必填 string，输出 number | 官方 defineTool、真实数学解析器；正常96/非法表达式拒绝 |
| 官方 ToolRuntime 0.1.1-rc.2 | inject=['systemPrompt']；mode='native' | schemas/execute；真正预取消 AbortSignal；停用 UNKNOWN_TOOL |
| 官方 SystemPrompt 0.1.1-rc.2 | 不依赖另一服务；关闭 DSH 身份及运行上下文注入 | 为真实 ToolRuntime 提供注册服务，不接管 Coolzhu 提示词/记忆 |
| Cordis 4.0.1 Context/Fiber | 官方注册与资源生命周期 | 三个子 fiber dispose 后全部 DISPOSED、注册表0、两个服务消失 |

旧0.0.1-rc.1依赖图含公共源无法取得的dsh-type-meta；没有force或假服务跳过。按真实候选锁文件固定DSH SDK 0.1.1-rc.2、Cordis4.0.1、Schemastery3.18.1、Cosmokit1.8.2，17依赖均从公共npm来源安装且不运行安装脚本，保存精确锁文件/SHA512，并独立下载核对dsh-tools tarball完整性。SDK npm版本不冒充下文历史DSH Git commit的完全同一源码。

原始收据、依赖矩阵、真实工具定义、执行结果、取消、释放以及第一探针对根dispose语义的误判均保留于[060 P1证据](../testing/release-0.2.60/dsh-p1/dsh-p1-runtime-receipt.json)。根fiber的dispose实际是restart，不能仅以根仍ACTIVE认定插件未释放；最终按子fiber、注册表和服务消失核验。未启动DSH AgentLoop、未传入模型密钥、未修改正式依赖。P1仅首个独立工具包通过，未知服务仍不兼容；正式安装、Qwen工具暴露/运行及其它类别包均不能由此追认。

## 当前差距与方案结论

当前 `dsh_market.rs / dsh_market.js` 已接入 `awesome-dsh-plugin.com` 社区目录，可搜索、查看详情和检查清单。目录条目仍不能直接安装。该站是社区发现来源，不能标成 DSH 官方发行源。现有 `extension_market / coolzhu-plugin-system` 管理原生 `plugin.json` 插件，`plugin_runtime.rs` 将启用的进程工具暴露给当前真实模型；它不能加载 Node/Cordis 插件或 DSH profile bundle。读取仓库中的同名 JSON、将 npm 包当 Rust 进程工具或仅接一个 MCP 客户端，都不能消除这项差距。

采用一个独立 Node/Cordis 插件宿主，按已验证的服务依赖逐步适配 DSH 插件。Coolzhu 继续拥有会话、模型请求、上下文/记忆、工具权限、取消、预算和轨迹，不启动第二套 DSH Agent 主循环。安装器与运行宿主分开；社区目录只提供候选，安装使用确认的包身份和固定版本。第一阶段只支持经过真实包验证的工具插件，不能宣称兼容整个 DSH 插件生态。

## 官方依据及适用范围

审查固定在 DSH 源码 `639ed015397290b3745d163aafe02ffee4aa3f84`；本机旧 DSH 目录不是 Git 仓库，不能据其旧版本推断当前机制。源码索引、Git blob 与 SHA256 已核验，保存于本轮临时分析目录。

- DSH 的管理服务操作 profile 的包依赖、组合包选择和 Cordis patch，启用选择与加载成功是不同状态；Host 代码在宿主用户进程执行，不受工作区沙箱保护。[官方插件管理说明](https://github.com/deepseek-ai/deepseek-harness/blob/639ed015397290b3745d163aafe02ffee4aa3f84/packages/boot/plugin-manager/README.zh.md)
- 安装 spec 包括 registry、绝对本地路径、Git 和压缩包；相对路径不能以宿主工作目录猜测。[官方 spec 实现](https://github.com/deepseek-ai/deepseek-harness/blob/639ed015397290b3745d163aafe02ffee4aa3f84/packages/boot/plugin-manager/src/install-spec.ts)
- 包管理有独立写锁、进程树清理、清单与锁文件恢复及失败诊断，依赖构建脚本另行批准；已批准脚本的外部副作用不能用清单回滚消除。[官方安装操作](https://github.com/deepseek-ai/deepseek-harness/blob/639ed015397290b3745d163aafe02ffee4aa3f84/packages/boot/plugin-manager/src/operations.ts)
- DSH 工具通过服务注册 schema，执行携带调用身份、Agent 上下文与取消 signal。插件还可能依赖其它 Cordis 服务，注册工具不等于所有插件只依赖工具服务。[官方工具契约](https://github.com/deepseek-ai/deepseek-harness/blob/639ed015397290b3745d163aafe02ffee4aa3f84/packages/core/tools/README.zh.md)、[实现](https://github.com/deepseek-ai/deepseek-harness/blob/639ed015397290b3745d163aafe02ffee4aa3f84/packages/core/tools/src/index.ts)

以下职责和接口为 Coolzhu 拟议方案，不能当成官方已有互操作协议。

## 模块职责和接入点

| 模块 | 单一职责 | 明确边界 |
|---|---|---|
| DSH 目录适配器 | 获取与展示候选来源、npm/spec、描述 | 不启动包代码，不根据分类认定兼容 |
| 远程包安装器 | 解析 spec，检查版本/完整性/服务依赖，暂存依赖、锁定版本，事务提交与回滚 | 不参与聊天，不启用未经加载验证的包 |
| Node/Cordis 宿主 | 固定已支持服务集合，加载确认的包，发布工具清单，执行/取消某个调用，释放插件资源 | 不获得模型密钥，不接管会话/记忆，不启动 Agent 主循环；独立进程不等于 OS 安全沙箱 |
| Rust 插件网关 | 冻结工程、插件版本和宿主世代；校验 IPC，给现有工具注册/执行链提供定义及结果 | 不让插件回执自己放行权限，不补发未知结果的调用 |
| 现有运行时 | 权限、当前聊天室、请求身份、根预算、取消、usage 和轨迹 | 仍是控制状态的唯一写者；不由市场页面维护另一套状态机 |
| 右侧插件页面 | 社区目录、详情、安装进度、已安装/已启用/加载失败三个独立状态 | 图标控件与可访问标签；内部进程、调试日志和技术路径只进轨迹/诊断 |

新能力放独立模块，避免继续堆入巨型 `main.rs`；既有路由只组装服务。现有原生插件及 stdio MCP 运行链保持独立，不能将不支持的 Cordis 服务默默替换成 MCP 或空实现。

## 接口与生命周期

1. 检查阶段返回包名、解析出的精确版本、来源地址与完整性、声明的插件/bundle、DSH peer 范围、必须的 Cordis 服务及依赖脚本列表。只有检查完整、运行宿主确实提供依赖时才能显示“可安装”；未知依赖显示具体不兼容原因。
2. 安装记录 `operation_id / workspace_id / source / resolved_version / integrity / installed_files / phase / error`。一工程同一包只有一个写者。网络查询、下载、包管理和取消各有上界；进程退出后有界排空输出，孙进程未停止不得释放写锁或声称回滚完成。
3. 私有 registry 不回退公共 registry，来源与认证保持一致；安装不能把 API_KEY 写入日志。依赖脚本与版本豁免不可因普通“安装”按钮而默认放行。清单回滚失败或脚本外部效果另记，不能显示为完全恢复。
4. 宿主握手返回协议版本、宿主世代、精确包身份和支持的服务集合。清单只含真实已注册工具，每项有唯一名字、schema、来源和修订；不得从 README 猜工具能力。异名、重复、过长名称或 schema 不兼容时拒绝接入并保留原因。
5. 调用至少携带当前 `run_id / tool_call_id / workspace_id / room_id / deadline / plugin_revision / host_generation`。Rust 先走既有权限和调用登记，Node 再校验世代并执行一次。取消必须传入真实 AbortSignal；忽略取消的插件按有界收尾终止宿主进程树，并将结果记为不确定或失败，不能自动重试。
6. 停用先撤销新调用资格，再取消/排空已有调用，最后 dispose 插件及资源。卸载和重装发生版本变更时，旧工具定义失效。其它工程不继承启用；工程切换不把当前在运行调用静默移到新工程。
7. 工具结果经有界序列化后进入既有模型 ToolResult 和轨迹；原始 stdout 不兼任 IPC。插件不得通过输出请求隐式执行其它工具、切换权限、读取任意模型密钥或复活已取消父轮。

## 逐步实施与停止条件

| 阶段 | 可交付结果 | 进入下一阶段的证据 |
|---|---|---|
| P1 依赖矩阵 | 对候选远程真实包逐项列出 module、inject、Config、工具 schema 与服务依赖，固定包版本和源码身份 | 至少一个公开真实工具包可加载；未知服务明确拒绝，不以模拟包证明兼容 |
| P2 安装事务 | 目录检查→锁定精确包→下载暂存→安装记录；失败/取消/重启恢复有回执 | 正常安装及一个真实失败回滚，文件和依赖无半成品；不启动插件即完成检查 |
| P3 宿主与工具桥 | 支持 P1 所需真实服务，隔离进程加载、清单、一次执行、取消和 dispose | 实际工具定义进入真实 qwen3.8-flash 请求，Qwen 发起并拿到本次真实结果；进程崩溃不拖死控制台 |
| P4 正式右栏 | 安装、启停、卸载、进度与错误入口连到 P2/P3，保持简洁图标样式 | 正式安装版按键实拍，关闭页面不抛失安装结果；停用后新请求不再暴露旧工具 |
| P5 扩大兼容 | 逐服务增加依赖桥，按精确包版本声明已验证支持范围 | 每类至少一个真实远程包与真实模型运行，不能用目录数量作为兼容数量 |

若 P1 发现工具包依赖 DSH AgentLoop 或广泛未提供服务，第一阶段不靠假服务维持加载；选择可独立工具包或实现确切依赖后再放行。需要模型/provider/UI/profile 扩展的包另列，不在第一版“工具兼容”名义下接管 Coolzhu 架构。

## 针对性软件验收

通过标准是安装版实际页面截图、真实远程固定包文件身份、真实 Qwen 的结构化调用与本轮结果，以及停用/取消后的执行事实。首批覆盖：查找与空结果；检查不激活包；正常安装；同包并发；网络失败；取消安装与重启恢复；启用失败；工具重名；真实模型执行；取消执行；停用期间拒绝新调用；跨工程隔离；卸载失败保留状态。优先实操和少量关键生命周期测试，不能写大量镜像单元用例替代这些结果。

当前已通过社区目录读取、搜索与不兼容说明，P1首个真实独立工具包的依赖/加载/执行/预取消/dispose验证通过；P2–P5正式安装、真实Qwen工具桥及市场页面仍未完成。因此“插件市场与DSH一致，能安装并调用远程插件”的需求保持开放，不能由已有本地原生插件或临时P1通过结果追认。
