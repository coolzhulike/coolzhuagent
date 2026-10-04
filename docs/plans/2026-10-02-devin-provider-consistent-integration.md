# Devin 在 Agent 内统一使用的实施计划

日期：2026-10-02，实施状态更新于 2026-10-04。后端身份、配置及只读发现已落地；ACP 会话驱动、持久化台账和窄工具桥已在本地实施并验证工程链路。真实 CLI 与原生 Windows 旁路尚未验收，正式 Agent 聊天接线仍关闭。

目标是让用户在现有 Agent 配置中选择 Devin 和实际可用模型，在同一聊天室发送消息、查看进度、审批工具、使用技能与记忆、停止及恢复任务。沿用现有 provider 的使用入口和操作语义，协议差异由后端适配，并如实展示能力差异。

## 1 启动问题先行处置

本机重新构建的 0.2.63 安装成功，已安装 10 个关键产物摘要全部与构建报告一致。13:34 的启动自检报告 `ok=false`：发现两份都有数据的工作区，且没有可判定的用户级选择，于是阻止启动，后台和桌面 PID 均为空。

- `C:\Users\coolzhu\coolzhuagent`：安装配置默认工作区。
- `C:\Users\coolzhu\AppData\Local\CoolzhuAgent`：历史默认工作区。

已通过安装版 `--list-candidates` 复核，两目录均可写；逐一使用 `--workspace ... --print-resolved-paths` 只读解析，退出码均为 0。旧 WebView2 stderr 时间早于此次失败，不作为本次根因。当前直接阻断点是工作区歧义，不是已证实的程序损坏或端口冲突。

恢复步骤：用户确定要继续使用的目录 → 通过安装版 `--select-workspace <绝对路径>` 原子保存用户选择 → 使用正常启动器启动 → 回读健康、自检、版本、工作区与桌面进程。保留两份原数据，不按文件大小或修改时间猜测，不合并或删库。选择存于用户级 `launcher-user.json`，无需重新编译。

实际恢复结果：用户选定 `C:\Users\coolzhu\coolzhuagent` 后已保存 revision=1；正常安装入口退出 0，自检 `ok=true`，后台 PID 18860、桌面 PID 19220，健康接口可访问，桌面存在标题为“COOLZHU AGENT 控制台”的窗口。后台回读工作区与会话库均在选定目录，构建身份 `f73c49b0a24e · 2026-10-02`。未删除或合并另一目录。证据位于 `tmp/analysis-startup-recovery-selfcheck.json`、`tmp/analysis-startup-recovery-health.json` 和本轮启动日志；进程身份只证明启动恢复，不替代全界面验收。

产品改进待办：多候选首次启动时提供原生目录选择窗口，显示来源和含数据事实；取消保持未启动。选择仍调用同一校验与原子发布服务。权限失败或目录不可用应给出明确提示；升级后有有效保存选择则继续沿用。此改进独立于 Devin，避免把启动修复和接入改造混成一个大变更。

## 2 源码和文档基线

| 基线 | 身份 | 用法 |
|---|---|---|
| 当前主线 | `f73c49b0a24ee32aa8e9a82dd2d98712b6943526` | 所有新实现以此为工作基础，保留最新 Browser Use、DSH、权限和运行生命周期 |
| PR #72 | `9572ec533700f9e9aaa5166afcfd9edf4f2ec7d5`，草稿 | 复用其 Cloud 客户端、凭据保护、账本及已有证据；重新适配主线后验证 |
| 用户方案 | `coolzhuagent-Devin-完整接入方案-2026-10-01.docx` | 接入设计参考；其中历史版本、暂缓决定和实施建议不是本轮独立授权 |
| 本机安装包 | 0.2.63，源码 `f73c49b`，2026-10-02 重建 | 当前启动问题的复现基线，不与历史同版本安装包混用 |

[PR #72](https://github.com/coolzhulike/coolzhuagent/pull/72) 目前提供独立 Cloud 会话管理和本地委派入口，尚未进入 ProviderClient 循环，也未完成本地技能、上下文和记忆闭环。不能回退主线到其旧候选或把该草稿的面板验收视为统一 Agent 完成。

实施前比较 PR 与最新主线的三方差异，只移植已审核改动；巨型 `main.rs`、`app.js` 采用小接线点，新职责优先拆入小模块。PR 最终是否合并或另开后续 PR，应依据适配差异处理，不直接自动合并。

## 3 用户可见的一致性标准

| 使用环节 | Devin 的目标行为 | 通过条件 |
|---|---|---|
| 创建和编辑 Agent | 在既有 provider 选择器选择 Devin，使用同一 Agent 记录 | 无须先去插件市场创建独立会话才能聊天 |
| 连接配置 | 同一配置页展示连接状态、认证入口、能力与错误 | 明确 CLI 登录与 Cloud 组织密钥各自用途；秘密不进入会话 DTO |
| 模型选择 | 获取账号实际可用模型，保存请求值和实际生效值 | 请求失败保持旧值；不得用 Cloud mode 充当模型 |
| 思考程度与参数 | 原控件根据真实协商能力显示可选值或不支持 | 不静默吞掉配置，不把其它 provider 的参数直接转发 |
| 普通聊天 | 使用现有输入框、消息列表、附件及任务进度 | 同一聊天室连续多轮、刷新和重启可恢复 |
| 工具和审批 | 使用既有工具卡片、权限与执行台账 | 相同权限配置产生相同允许/拒绝结果，取消后不追加动作 |
| 技能和插件 | 使用现有已授权集合和版本快照 | 启停与卸载即时生效；不额外继承 CLI 全局工具或技能 |
| 上下文与记忆 | 复用工程、聊天室、Agent 的上下文构建与记忆范围 | 无跨房间串读；候选记忆按同一写入规则验证 |
| 停止和恢复 | 现有停止按钮与任务恢复入口 | 显示实际停止状态，未知执行不会被显示成已完成 |
| Goal 与子 Agent | 从相同 Agent 目录选取，并冻结父执行关系 | 一个阶段、一个子任务各自有可追溯执行身份 |
| 费用和诊断 | 同一统计入口，区分 token、ACU 或未知指标 | 没有提供的数值显示未知，不能补成 0 或虚构换算 |

“一致”要求相同的产品入口、所有权和操作结果；不要求所有后端提供相同参数。完整交付必须覆盖上表，不能只做到下拉框出现 Devin。

## 4 协议路线和后端结构

### 4.1 主路线

统一 Agent 使用优先走 Devin CLI ACP。官方支持账号模型目录、CLI 模型选择和 ACP stdio 接口；模型目录及生效身份按当前登录账号和固定 CLI 版本取证，不内置容易过期的型号表。[CLI 命令](https://docs.devin.ai/cli/reference/commands)，[模型说明](https://docs.devin.ai/cli/models)。

Cloud REST 保留为同一应用内的远程委派能力，复用 PR #72。它不能承担“选择模型后直接成为同等本地 Agent”的全部职责；Cloud session 创建接口与本地模型请求的语义不同。Cloud 的 `devin_mode` 留在远程任务设置，避免与模型或 reasoning 混用。[创建接口](https://docs.devin.ai/api-reference/v3/sessions/post-organizations-sessions)。

若希望云 Devin 反向访问本地能力，需要另建可达且认证的桥；不是 ACP 主路线的隐式备选。本轮计划不以公网隧道或 WSL2 偷换原生 Windows 执行环境。

### 4.2 统一接口

在现有 ProviderClient 之外引入应用级 `AgentSessionBackend`。这是建议名称，实施时服从仓库命名。

```text
现有 Agent 配置和聊天室
        ↓
统一会话服务  所有权  上下文  权限  预算  事件投影
        ↓
AgentSessionBackend
        ├─ LlmBackend → 现有 ProviderClient 和本地主循环
        ├─ DevinAcpBackend → 受监督 CLI 进程与 ACP 会话
        └─ DevinCloudBackend → PR72 客户端与后台观察器
        ↓
统一上下文服务和受控工具桥 → 现有真实 executor
```

接口职责：连接检查、模型/参数能力目录、建立/加载指定会话、提交 turn、事件订阅、取消、恢复和状态核查。共享事件包含文本增量、工具请求/更新、权限请求、用量、错误和终态；必须带本地 scope、turn/attempt 和连接 generation。

现有 LLM 路线保留自己的工具循环；ACP 由 Devin 驱动其会话循环，宿主负责桥接和门禁。不得把一个 ACP prompt 同时交给现有 LLM 工具循环，造成双重执行或重复回复。

产品层可展示 `provider=devin`；存储需要独立 `backend_kind`/连接引用和版本化能力，不能只向 ProviderKind 枚举添加一项后继续套用 OpenAI HTTP 路由。扩展后的枚举匹配点必须逐一检查。

### 4.3 模型配置与切换

- 只读模型发现可使用官方 `devin models list --format json`；以 ACP 当前会话实际返回的模型配置和生效值完成确认。明确 catalogue 的发现来源、账号、版本、时间及是否已验证。
- ACP 若返回 `configOptions`，按真实选项发送 `session/set_config_option`，记录返回的完整配置；旧协议仅在固定版本实证支持后适配其模型接口。启动参数只负责新会话的默认请求，不能当成既有会话换模成功。
- 模型别名可能随官方更新解析到新版本。区分 requested alias、实际选项 ID 和可取得的 resolved ID；无法得知确切底模时标记未知。
- 模型、模式、reasoning 分开保存。Devin CLI 的思考能力并不证明 ACP 必然提供同名参数；必须读真实协商结果。未知选项保留诊断，不伪造 none/low/high 档位。
- 运行中换模复用现有在途排空规则。ACP 协议允许修改配置不等于本应用可以绕过冻结 turn 配置；本应用先排空再切换，失败回滚显示值，重启核对 requested/effective。
- Adaptive/Fusion 如账号提供，应作为实际路由选项保留其语义；不要宣称固定底模或让失败任务自动换到更昂贵选项。

配置选项的设置、完整返回值和通知形式以 [ACP 配置协议](https://agentclientprotocol.com/protocol/v1/session-config-options) 为依据；协议存在不代表 Devin 已实现每个可选项。

## 5 工具权限和 Windows 验证条件

Devin CLI 在原生 Windows 上尚不提供官方 OS sandbox，ACP 运行同样受该限制。因此工具桥不自动等于全部动作都经本地审批。[官方 sandbox 说明](https://docs.devin.ai/cli/sandbox)。

P0 必须验证固定 CLI 能否禁用或约束其内建 edit/exec、额外 MCP、全局配置导入、技能脚本、自动子 Agent 和背景进程。拒绝、撤销、超时、无效配置或 hook 崩溃时均不能产生未授权副作用。文档提示和单纯工具通知不能证明动作已被控制。

官方允许按工具和范围配置 deny/ask/allow，可用作验证入口，但不能单凭配置文本就宣布具备 OS 隔离。[权限文档](https://docs.devin.ai/cli/reference/permissions)。若无法可靠关闭旁路，则完整本地变更能力不放行；先保留未满足条件的明确状态，另行设计 Windows 外层隔离，不将此状态称为已完成统一 provider。

工具桥另建窄 stdio MCP server 或经证实可用的 ACP 客户端工具适配层，复用真实 executor。当前 `mcp_host.rs` 是客户端，不能误当 server。每次工具请求由宿主绑定实际 workspace/room/agent/run、父 claim、owner epoch、取消、deadline、权限、工具来源和资源锁；不信任模型提供的身份或“已批准”文字。

工具与技能只提供用户授权、父允许集合和当前实际可用集合的交集。插件停用、MCP 重连或配置换代拒绝旧 generation。内置执行事件和经工具桥执行的本地事件区分来源；只有后者取得本地 executor 的实际回执。

## 6 生命周期和持久数据

复用当前 SQLite 的可恢复事务设计，新增版本化连接、会话绑定、turn attempt、远端 operation、观察水位和未知资源锁。是否复用现有表由设计确认，不机械复制另一套所有权系统。

绑定主键至少覆盖 workspace、room、agent、local session、connection generation 和 remote session。工程切换后旧更新只可写入所属历史记录，不能落入当前聊天。Goal 与 HostChildScope 的真实父关系在提交前冻结并在工具执行前重新核对。

每次用户新意图生成新的 attempt；相同文字连续发送仍是两次意图。fingerprint 只核对内容，不能作操作身份。Cloud 写请求网络前登记状态；unknown 保留资源锁，换 operation ID、刷新、重启或 lease 过期都不能绕过。接受回执、绑定和 settle 使用同一事务或可恢复 journal。

ACP prompt 超时或断连同样可能留下执行；没有停止事实时按未知处置。取消请求已发、协议终态已到、进程及子进程已排空分别记录；只读恢复指定 session，不自动重投 prompt。Cloud 的本地停止等待与远端 terminate/archive 分别显示。

文本增量和远端状态可通过本应用 SSE 推送给现有消息界面；这是宿主事件投影，不冒充 Cloud token SSE。Cloud observer 属于后台服务，面板关闭只关闭订阅。消息按 event ID 去重，游标视为不透明值。

复用 `build_context_assembly_with_roster`、记忆 selection/revision、当前边界和工具目录形成最小冻结上下文。不能把整个会话库或全局记忆导出。记忆候选仍由本地验证后写入；压缩与完成、未知解锁彼此独立。

## 7 分阶段实施与代码落点

| 阶段 | 具体任务和建议落点 | 完成门槛 |
|---|---|---|
| P0 固定能力与最小原型 | 固定主线/PR/CLI/协议版本；新 `devin_acp` 探针；只读认证/模型目录；受控临时工程检验权限、配置、取消与进程排空 | 真实 ACP 模型选择和 requested/effective 可验证；Windows 内建旁路结论明确；未通过则标记阻塞 |
| P1 通用会话后端 | 新 `agent_session_backend.rs`/`agent_session_service.rs`；封装现有 LLM 路线；接线 `api_chat_send`、`api_chat_send_stream`、`stream_agent_model` | 所有已有 provider 行为保持；Devin 接入共用 turn、事件和错误投影；没有双循环 |
| P2 Agent 配置和模型体验 | 扩展 `AgentSessionDto` 的后端/连接字段；统一能力查询；模型选择和保存；`app.js`/`index.html` 小范围接线；CLI 认证状态和模型目录缓存 | 新建/编辑/复制/禁用 Agent，连续聊天、真实换模、失败回滚、刷新及重启全部通过 |
| P3 工具上下文和执行所有权 | 新受控桥、技能 manifest、上下文 exporter；接现有权限/真实 executor；扩展 `host_child_agent.rs` 的后端快照和 `goal_execution_parent.rs` 的继承 | 同一任务可用技能、插件、记忆和审批；工程/房间/父 claim/generation 切换不会串权 |
| P4 Cloud 基础能力复用 | 将 PR72 client/store 接入共用 scope/attempt 服务；迁移旧绑定；修复 pending Map；后台 observer、停止/恢复、ACU 与产物核验 | 保留远程委派入口；真实 Cloud 验收独立完成；unknown 不重发；旧记录不自动归属 |
| P5 安装验收与灰度 | 固定 CLI/桥及许可资源的分发策略；安装前工作区选择检查；schema 迁移和回滚；完整回归与真实账号端到端 | 干净安装和升级均可从正常快捷方式启动；统一使用验收全部闭环后再发布 |

每阶段产出独立、可审查变更和验证记录，不再向 `main.rs` 填整套新后端。PR72 的大文件补丁需先在最新主线上逐段校准后拆分，Cloud 客户端与 ACP 使用同一服务但不同 transport/认证。

关键现有定位已核对：`modules/llm-adapter/.../providers/mod.rs` 的 Provider 和 ProviderKind；`client.rs` 的 ProviderClient；web-console 的 `host_child_agent.rs`、`goal_execution_parent.rs`、`mcp_host.rs` 和上述聊天/上下文函数。第一阶段新增 `agent_session_backend.rs` 与 `devin_acp/{discovery,protocol,transport}.rs`，第二阶段新增 `devin_acp/{session,journal,bridge,process}.rs`。实际边界见第二阶段 work-log。正式聊天室文本入口已接通；完整 scope 审批、工具任务和 Cloud observer 仍待完成，不能把文本会话验收扩张为完整 Agent 工具执行交付。

## 8 验收任务与证据

| 编号 | 场景 | 必须观察到的事实 |
|---|---|---|
| U01 | 原生快捷方式启动 | 正确工作区、0.2.63 或候选版本、健康就绪和桌面窗口；多候选选择取消不建替代数据 |
| U02 | Agent 内选择 Devin | 既有表单保存真实模型、连接和能力；同一聊天入口发出对应 ACP turn |
| U03 | 模型和思考程度 | 列表来自真实账号；生效值由协议回执确认；缺能力明确显示；重启保持 |
| U04 | 连续多轮及同文重发 | 相同 remote session 继续；新意图新 attempt；重复 UI 提交不重复执行 |
| U05 | 工程和聊天室切换 | 历史输出、记忆、审批、绑定和用量不进入另一 scope |
| U06 | 工具和技能 | 至少包含读文件、修改文件、执行检查、授权插件、MCP、技能脚本；各有真实 producer 或拒绝事实 |
| U07 | 取消和换模 | 取消后无新本地动作；进程排空有事实；在途不换配置；失败不静默切 provider |
| U08 | 崩溃和 unknown | 发送中断、落盘失败、断连、刷新、双窗口均无第二次未知写入；未知锁跨重启保留 |
| U09 | Windows 内建旁路 | 原生 edit/exec、全局配置/MCP、子 Agent、hook 失效、后台进程均不突破批准集合 |
| U10 | Goal 和子 Agent | 父取消、预算、工具交集、执行作用域冻结；失效 claim 不能恢复成可执行 |
| U11 | 记忆生命周期 | 最小读、候选写、压缩和恢复保持权限、版本、审计和证据引用 |
| U12 | Cloud 远程任务 | 组织/session 身份正确，accepted/远端结束/本地验收独立；费用和未知控制有回执 |
| U13 | 产物和费用 | 固定 head/base 的产物核验；用户批准后才应用；真实统计或未知，不虚构 token/美元 |
| U14 | 现有 provider 回归 | 代表性 OpenAI/Anthropic/百炼及自定义连接保留配置、流事件、工具、停止与重启语义 |
| U15 | 安装升级与迁移 | 快捷方式与包资源完整；CLI 不存在/登录过期/升级后能力变化可诊断；旧数据库迁移可回滚 |

离线协议与故障注入、安装版联调、真实 Devin 账号任务分别记录，不能相互代替。真实收费任务使用单独批准的账号、工程、上下文范围和预算。每项记录版本、scope、attempt、remote session、expected/actual、实际费用及证据来源。

必要工程检查随改动执行：受影响 crate 的 `cargo build --offline`、web-console 完整测试、module_linkage_smoke，以及既有前端契约和语法检查；后续修改 llm-adapter、Tauri 时分别执行其测试。安全故障用例有意义地覆盖 ownership、unknown、epoch、事务恢复与取消，不只检验源码出现某个字符串。离线协议和隔离 API 验证不能代替真实 Devin 认证、生成或安装版 GUI 验收。

## 9 本轮结果与后续决策

本轮已完成启动恢复与设计，并按用户随后“先在本地进行实施修改”的要求落实第一阶段代码：后端身份和门禁、原 Agent 与模型配置入口、受管 CLI 只读发现、ACP 握手/配置协议基础，以及未就绪任务的接纳前拒绝。能力未知如实返回空值，拒绝未经协商的 HTTP 采样、容量与图片覆盖。已进行编译、完整控制台回归、前端行为测试与隔离 API 保存/重启验证；结果见 `docs/work-logs/2026-10-02-devin-local-provider-foundation.md`。

按用户提供的 Devin 桌面路径找到内置原生 CLI，已核对版本 `devin 3000.10.48 (fcf7ba39)`、文件 SHA256 和 ACP 子命令。2026-10-04 已完成官方浏览器授权、真实认证核查与 721 项账号模型目录适配；SWE-2 三个变体均由目录标记 Free。使用 `swe-2-medium` 的真实 ACP 专项通过中文首轮回复、跨进程恢复前文与基础计算，三轮 requested/effective 一致、end_turn 与进程树排空明确。P0 的 Windows 内建旁路仍未完成完整验收。

已落实独立 ACP 会话驱动、台账、事件、取消和窄工具桥。2026-10-04 根据用户“模型接入聊天室才算完整验收”的要求补齐正式聊天室纯文本接线：使用既有单目标发送入口、共享运行接纳和消息流，从输入框真实发送并完成中文首轮、暗号续聊、页面刷新和后台重启后的计算；六条消息与三轮 ACP 模型/终态/排空回执均可核对。另一聊天室的真实非流式发送完成，远端绑定不同。旧配置只读连接仍不允许 prompt，真实聊天室使用新的文本执行适配器，不通过测试 fixture 或环境开关绕过正式入口。

P1 的聊天室文本路径及 P2 的基本登录、模型选择、连续聊天和持久恢复已完成；完整工具执行、附件、Goal、群发/接力、子 Agent 仍关闭，P0/P3 的全面旁路与审批验收未完成，P4–P5 未完成。工具拒绝采用固定原生 CLI、独立工作目录、受控配置和不挂载工具；真实 read/write/exec 拒绝探测通过，不将其扩张为全面 OS 隔离通过。Devin 不进入 HTTP 工具循环或 HTTP 文本恢复，未知结果仍保留锁，未使用收费模型，未替换正式安装包。

PR72 的原始 Cloud 基础代码和历史保持，统一 observer 与上下文仍待补强。最新结果见 [聊天室真实验收与截图](../testing/devin-acp/chatroom-acceptance-20261004.md)；此前后端专项记录保留于 [原真实登录与 ACP 专项](../testing/devin-acp/real-validation-20261004.md)。

下一步实施按 P0 → P1 → P2 → P3 推进，Cloud 补强按共用底座后展开。产品目标已确定为 Agent 内统一使用，无须再把“是否只做插件面板”交回用户决定；仍需真实确认的事项为 CLI 可控性、账号能力、授权范围与预算。

补充 P2 登录体验：插件市场与 Agent 模型设置共用“登录 Devin”入口，官方浏览器授权由受管本机进程执行，页面展示登录、等待、取消、超时、重试及真实认证状态。Windows 改用 ConPTY 确认官方默认浏览器选项，解决无输入而超时的问题；页面与 CLI 均已核对已登录。密码、令牌与原始授权输出不进入页面或会话 DTO。认证和纯文本真实验收已完成，聊天室真实文本接线和三轮界面验收已完成；完整工具隔离与 Goal/子 Agent 接线仍待实施。

引用文档的历史“暂缓”与“只交方案”是原方案背景；本轮实际范围来自用户最新请求。此文件不替代后续真实任务授权，也不把拟议后端或尚未验证的能力写成已实现。
