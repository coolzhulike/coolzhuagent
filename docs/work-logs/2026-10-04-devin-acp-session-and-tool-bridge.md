# Devin ACP 会话与本地工具桥：第二阶段实施记录

日期：2026-10-04。承接用户“继续”，在 `codex/source-sync-20261002` 本地实施。源码基线仍为 `f73c49b`；保留第一阶段修改和用户指定的正式工作区。

## 已落实的工程

新增 `devin_acp/session.rs`：实现独立 ACP 会话驱动，覆盖 initialize、new/load、真实模型配置回执、prompt、session/update 与 cancel。配置连接和任务连接职责分开，ACP 提示不会进入旧 HTTP 模型工具循环。加载已有会话必须协商 loadSession；不支持时拒绝创建替代会话。加载历史单独标记 replay，不能当作本轮回复或执行证据。

接收器在独立任务中保留半帧，使用容量 32 的队列、1 MiB 帧上限、通知次数上限和累计事件内容上限。取消发生在读到部分 JSON 时不会丢帧；发送过程中取消导致连接失效并保留未知执行，不在半条请求后追加协议消息。文本、思考、远端工具观察、配置、用量分别投影；每条事件绑定宿主 scope 和递增序号。成本未提供时保持空值。远端 tool_call/tool_call_update 即使报 completed，也只作观察，不会生成本地真实工具成功事实。

新增 `devin_acp/journal.rs`：在 SQLite 内持久化工程、房间、Agent、run、turn、attempt、owner epoch、连接 generation、远端绑定、模型回执与执行状态。epoch/generation 由本地事务分配，不取自模型或 CLI 通知。发送前提交 submitted；断线、未知终态或等待被丢弃保留 unknown 和发送锁。取消请求、协议 stopReason、进程排空三种事实分开记录。真实进程排空且本地工具都有确定终态后，才释放 terminal/not_sent 的锁；未知执行和迟到成功不自动解除锁。远端 session 不能跨工程、房间或 Agent 绑定。

台账打开沿用现有会话库版本前置检查，拒绝未来版本；早期本地台账只补可空模型回执列，不改历史状态。模型 requested/effective 在完整返回配置确认后保存，resolved_model 继续未知；回执冻结后不可重复覆盖。独立台账尚未正式挂到应用会话 schema 的发布迁移，这仍是接入聊天路由前的工作。

新增 `devin_acp/bridge.rs`：每个 attempt 独立本机 HTTP MCP 桥，随机端口和随机 bearer 秘密，校验 Host、Origin、协议版本及请求大小。桥不会挂到主控制台公共路由；只有宿主持有桥对象才启动监听。只开放已审核的内置 read_file/write_file/edit_file/glob_search/grep_search/bash 子集，不开放任意路由、资源接口或 CLI 文件/终端回调。

工具调用复用真实 `RegistryExecutor`、`runtime_tool_supervision`、runtime 权限评估、工具登记/去重、审计和根预算。宿主执行编号来自真实 run、完整 scope 与 MCP 请求关联编号，重复请求在副作用前拒绝。权限档位、工具集合、Protected 规则、授权副本、父 claim 与工程在接纳时冻结；blocking worker 执行前重新核对父运行、claim、取消、预算和台账。登记前和 worker 执行前还重新读取当前工具定义及房间权限，运行中关闭工具或降低权限会拒绝后续调用。冻结门与实时门都需要通过，运行中打开开发完全访问不能扩大原轮次权限。

工程 pin 由冻结策略持有，blocking worker 仍存活时不会因 HTTP 等待结束而释放。桥不允许 bash 后台请求或显式沙箱旁路。本轮只打通已获授权的执行与真实拒绝回执；旧 pending 审批队列不携带 ACP scope/epoch，暂不入队，避免旧批准在取消后绕过冻结门。完整审批的继续执行尚待接线，不能声称与其它 provider 已完全一致。

新增 `devin_acp/process.rs`，并在 `coolzhu-windows-process-guard` 增加 Job 进程树终止和 ActiveProcesses 查询。停止意图、句柄 Drop、主进程 wait 都不能独自当作整树排空。原生 Windows 测试实际创建受管 PowerShell 父子进程，再核对 Job ActiveProcesses=0 与主进程回收后才登记排空。测试构造器仅在 cfg(test) 可用；正式 CLI 生成入口继续拒绝启动。

对巨型 main.rs 只加冻结策略的短接线；`runtime_tool_supervision.rs` 在异步调用与 blocking worker 之间传递冻结策略，没有重写既有执行循环。

## 验证事实和限制

最终控制台全量回归：主目标 1341 通过、0 失败、2 原有忽略；其它目标 8 + 1 通过，doc-tests 通过。日志 `tmp/analysis-devin-acp-20261004-regression-installed-cli.log`。此前 1339 项全量日志 `tmp/analysis-devin-acp-20261004-regression-complete.log` 也保留。其中包含 ACP → 本机 HTTP MCP → 真实本地文件写入 → 工具台账 → 文本回复的完整工程测试。ACP 对端是 duplex fixture，写文件的 producer 是真实宿主 executor；这不能当作真实 Devin 账号生成验收。

前端测试 12 通过；既有 UI 契约通过。日志 `tmp/analysis-devin-acp-20261004-js.log`、`tmp/analysis-devin-acp-20261004-ui-contracts.log`。未改动现有 Devin 表单能力门禁，也未宣称完成 GUI 验收。

首次桥测试遇到测试环境初始化的临时 MutexGuard 生命周期导致互相等待，改为每次替换后先释放锁，三项桥测试随后通过。完整链路测试一度出现 fixture 两个读端同名造成的移动错误，已分别命名并通过全量回归。原失败日志保留，不隐去排查过程。

累计非文本事件上限、早期台账兼容、远端跨房间绑定和实时撤销工具均已加入回归并通过。补充 CLI 安装识别后，ACP 专项 37 通过；日志 `tmp/analysis-devin-acp-20261004-installed-cli-tests.log`。

最终离线 build 已通过，日志 `tmp/analysis-devin-acp-20261004-build-installed-cli.log`；调试后端 SHA256 为 `98819CFCC53EB1B06AC1E916692FF53F4C5A6A652E102F492DB0C0D23A30BB91`。Windows 进程守卫 54 通过、3 原有忽略，module_linkage_smoke 8 通过，tool-registry 离线 check 通过；对应日志 `tmp/analysis-devin-acp-20261004-process-guard.log`、`tmp/analysis-devin-acp-20261004-module-linkage.log`、`tmp/analysis-devin-acp-20261004-tool-registry.log`。

新编译后端在独立临时工作区和随机端口再次完成 14 项 API 保存、拒绝、重启验证；日志 `tmp/analysis-devin-acp-20261004-api.log`。CLI 缺失场景通过显式不存在路径验证，未因本机安装自动降级到另一可执行文件。

## 正式使用前仍需完成

用户随后提供 `C:\Users\coolzhu\AppData\Local\Programs\Devin\Devin.exe`。该文件为签名有效的 Devin 桌面入口（3.10.48），在其安装目录找到独立 CLI：`resources\app\extensions\windsurf\devin\bin\devin.exe`。只读 `--version` 确认完整版本 `devin 3000.10.48 (fcf7ba39)`，SHA256 `D8877EBF699499B1D0957A9FDD99CB596013FC3BFEB782B756496BBCF527BB1B`；`acp --help` 确认提供 ACP stdio 子命令。`auth status` 返回未登录，模型查询也以未登录退出；未进行登录、生成或模型请求。诊断只保存固定标记，不回显账号或凭据原文。

已新增本机安装目录的保守发现规则，明确覆盖变量指向桌面入口时直接拒绝；不会把 Electron 的版本当作 CLI 版本。新编译后端实测自动发现内置 CLI，模型查询返回明确“尚未登录”；错误桌面入口实测在启动前拒绝。证据 `tmp/analysis-devin-installed-probe--auto.json`、`tmp/analysis-devin-installed-probe--desktop.json`、`tmp/analysis-devin-auth-catalog.json`。版本固定仅设置在隔离核查进程内，没有修改用户或机器环境变量。

真实账号模型目录格式、MCP HTTP 能力、ACP 模型配置及加载仍未取证。官方仍说明原生 Windows 不提供 OS sandbox；全局配置/MCP、hooks、skills、内建 edit/exec 与子 Agent 是否可约束，必须用此固定 CLI 的真实证据验证。[Windows sandbox](https://docs.devin.ai/cli/sandbox)、[配置文件](https://docs.devin.ai/cli/reference/configuration/config-file)。

因此这次实现的是可独立验证的 P1/P3 底层工程，正式 ACP prompt 未接入原聊天/Goal/子 Agent 入口，`agent_execution_ready=false`。固定 CLI 旁路验证、统一上下文/技能/记忆快照、共享事件落库与 UI 投影、scope 审批、未知态人工对账及 Cloud PR72 适配仍待完成。不能把新增模块或离线测试当作统一 Agent 目标已经交付。

本轮未发真实 Devin 生成或收费请求，未推送、合并 PR、重新打包或替换正式安装版；0.2.63 安装数据与工作区选择未改动。

协议依据：[ACP 会话建立与加载](https://agentclientprotocol.com/protocol/v1/session-setup)、[ACP prompt 与取消](https://agentclientprotocol.com/protocol/v1/prompt-turn)、[模型配置](https://agentclientprotocol.com/protocol/v1/session-config-options)、[MCP HTTP transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)。协议规定与 Devin 固定版本的实际实现分别取证。

## 追加：在插件与模型设置内登录

用户随后要求登录应在插件界面操作，而不是手动运行终端命令。已新增 `devin_acp/auth.rs` 与共用 `devin_auth.js`：插件市场顶部增加“Devin 账号”，Agent 模型设置选择 Devin 时展示同一登录组件。提供“登录 Devin”“取消登录”“刷新状态”，授权进度自动轮询；两个入口同步变化，关闭设置页不自动取消官方授权，也不覆盖尚未保存的模型草稿。

后端新增只读 GET `/api/backends/devin/auth` 和 POST `/auth/login`、`/auth/cancel`。只有本机严格同源 POST 才能启动或取消，登录请求不接收账号、密码、令牌、可执行文件或任意参数。登录由固定官方 `auth login` 浏览器流程负责；输出持续排空但不回传或保存原文。一次只允许一个活动流程，取消必须匹配随机流程编号，迟到的状态或旧取消不能覆盖新流程。等待授权上限五分钟；取消、超时与退出均核对受管 Job 进程树排空，无法确认退出时禁止另开登录。

登录命令正常退出后还需重新查询真实认证状态，不能只凭 exit 0 报“已登录”；未知结果仍未知。取消不等于注销，若授权已在取消前完成，界面按重新核对的账号状态显示。认证接口不改变 ACP 任务门禁，也不触发生成或收费模型请求。

编译通过：`tmp/analysis-devin-auth-build-final.log`。最终调试后端 SHA256 `96C2A163BA2B4A505629E5A304144CD275146AE25033BCEF5E1477DE8B800DBE`。新增授权测试四项通过，覆盖认证原文保密、同源限制、流程身份、真实 Windows 进程退出/取消/超时；全量回归主目标 1345 通过、0 失败、2 原有忽略，其它目标 8 + 1 通过，日志 `tmp/analysis-devin-auth-regression.log`。最后的文案及认证状态解析补充后，ACP 专项 41 通过，日志 `tmp/analysis-devin-auth-final-tests.log`。

前端登录与既有 Devin 表单行为测试共九项通过；语法检查、既有 UI 契约、module_linkage_smoke 八项通过。日志 `tmp/analysis-devin-auth-ui-final.log`、`tmp/analysis-devin-auth-contracts-final.log`、`tmp/analysis-devin-auth-module-linkage.log`。

新后端在独立临时目录、独立端口实际核对本机 CLI 未登录状态。GET 不启动授权；跨站登录 POST 拒绝，令牌输入拒绝，错误流程编号取消拒绝；模型查询错误提示改为界面登录入口。证据 `tmp/analysis-devin-auth-preview.json`、`tmp/analysis-devin-auth-api-final.json`。实际浏览器核对插件页的可用登录按钮及未登录提示，并核对模型设置中切换 Devin 后共享登录按钮可用；未保存临时表单、未点击真实授权。截图 `tmp/devin-auth-plugin-preview.png`。

已保留最终编译版本的本机预览供用户点击登录。预览使用复制的调试二进制与临时会话库，避免占用下次编译产物；没有替换正式安装版和指定工作区数据。真实账号授权完成、目录格式与生成验收仍需用户完成官方登录后继续核对。[官方 CLI 授权接口](https://docs.devin.ai/cli/reference/commands)。
