# coolzhuagent × DeepSeek Harness × Codex CLI：源码级 Harness 对比与对齐决策

研究日期：2026-10-17（America/Los_Angeles；下载和校验记录使用 UTC）。

**建议采用“选择性对齐”，保留 coolzhu 的执行底座：学习 Codex 的请求级状态绑定、默认执行模式和工具发现，学习 DeepSeek 的提示词组装、上下文来源及生命周期。优先减少错误触发的约束和无收益的模型往返，再考虑更复杂的自治编排。**

目标是让强模型在已授权范围内充分规划和执行，同时把权限、取消、预算、调用去重等可判定规则交给运行时。增加文字禁令、审批模型、规划角色或验证轮次，都不自动等于更可靠；减少这些机制也不自动等于更快或质量更高。本报告给出源码事实、潜在成本和验收办法，未运行三方端到端性能基准。

## 1. 固定版本与已交付源码

| 项目 | 本次基线 | 本地位置与完整性 |
|---|---|---|
| DeepSeek Harness | `dsh-v0.2.1-rc.2`；提交 `687f282e8625c6b8d11ff58211a90524be2e4317`；2026-10-17 发布的预发布版 | [桌面源码](C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2)；16,297 个文件全部对应官方 Git blob：16,294 个逐字节相同，3 个 `.cmd` 为官方声明的 CRLF 转换 |
| Codex CLI | **最新稳定版 `rust-v0.163.0`**；提交 `83086188ae0614bd882818ff1aa23f3194962012`；2026-10-15 发布 | [桌面源码](C:/Users/zhupu/Desktop/codex-cli-0.163.0)；**9,176 个文件逐字节匹配官方 Git blob**，无缺失、无额外文件 |
| coolzhuagent | HEAD `8a13551b037f5f673188802f2c9128984245fab8` **加现有工作区改动** | [本地工程](D:/coolzhuagent)；本报告引用文件的内容哈希见[证据清单](D:/coolzhuagent/docs/analysis/2026-10-17-three-way-harness-source-evidence.json)，不能只用 HEAD 复现 |

Codex 查询时另有 `rust-v0.164.0-alpha.3`（2026-10-17 发布），本次未把 alpha 混入稳定版分析。[Codex 稳定版发布记录](https://github.com/openai/codex/releases/tag/rust-v0.163.0)、[alpha 发布记录](https://github.com/openai/codex/releases/tag/rust-v0.164.0-alpha.3)。DeepSeek 按用户指定 rc.2 固定；累计架构能力与 rc.2 单次新增必须区分，版本增量详见[已有后端版本报告](D:/coolzhuagent/docs/analysis/2026-10-17-deepseek-harness-v0.2.1-rc.2-backend-alignment.md)。

两个上游目录均为完整提交源码包，不含 `.git` 历史或安装后的依赖。本次只下载、读取和写调研文件，未修改三方业务代码、安装依赖或调用付费模型。

| 下载物 | 校验记录 |
|---|---|
| [Codex ZIP](C:/Users/zhupu/Desktop/codex-cli-0.163.0.zip)，23,196,229 字节 | [Codex manifest](C:/Users/zhupu/Desktop/codex-cli-0.163.0.source-manifest.json)；SHA-256 `e70021802baa025b77d917f5490efb33eeaae90ee2f106325864b2fc70e21350` |
| [DeepSeek ZIP](C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2.zip)，46,562,322 字节 | [DeepSeek manifest](C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2.source-manifest.json)；SHA-256 `828bbbebaf507d2b1fc335a608e1c8174e2dd5c5c80ebb64d5146ddc5ecb511f` |

## 2. 三方优劣与适用方向

以下“优势／代价”是针对 coolzhu 的对齐目标，不是模型智力排名，也不是性能实测排名。

| 工程 | 值得学习的优势 | 代价、限制与不宜直接照搬的部分 | 对 coolzhu 的价值 |
|---|---|---|---|
| DeepSeek Harness | 插件式 PromptAssembly；规则、工具、环境事实分开；实际模型能力参与协议投影；指令增改撤销；显式并发资格；子任务和 Goal 生命周期清晰 | TypeScript/Cordis 插件体系迁移成本；很多能力取决于插件和 preset；`complete` persona 可替换大量指导；Plan 文字不是只读沙箱；`always` 模型请求重试可无上限 | **优先借鉴上下文契约和工具执行语义**，通过 Rust 接口实现；不能只升级工具插件包就宣称完成 Harness 对齐 |
| Codex CLI | Rust 请求级 `StepContext`；固定本次模型、工具路由、环境和扩展输入；工具按资格并行；会话审批缓存；默认模式鼓励执行；延迟工具发现；丰富的取消和观测链路 | 运行时和模型目录耦合较深；CLI 配置、模型能力、实验开关会改变路径；Plan 模式刻意强调决策完整；自动审核／Stop hook／Goal 可带来额外轮次；不能照搬旧版某份 GPT 提示词代表当前所有模型 | **优先借鉴请求快照、调度和默认交互策略**；只引入确有收益的可选机制 |
| coolzhuagent | 已有冻结父上下文、真实工具身份、权限预检、根预算与取消；子任务继承权限上限；Goal 文件／命令验证是真实执行门 | Web 与 CLI 两条行为路径；Web system 混入记忆摘要和通用约束；关键字影响工具暴露；重复调用判断不看结果变化；恢复分支追加模型请求；Web 并发与 CLI 串行不一致 | **保留执行底座，统一上层契约**。主要收益点在触发条件、作用域、状态一致性和减少额外模型往返 |

依据：[DSH 请求链][dsh-step]、[Codex StepContext][cx-step]、[coolzhu 派发][cz-dispatch]、[coolzhu 两条路径分析](D:/coolzhuagent/docs/analysis/2026-10-17-deepseek-harness-source-level-alignment.md)。

## 3. Harness 工程差异表

### 3.1 请求、工具和并发

| 维度 | DeepSeek Harness rc.2 | Codex CLI 0.163.0 | coolzhu 当前源码 | 建议与优劣判断 |
|---|---|---|---|---|
| 一次模型请求绑定什么 | pre-step 接受组装结果；prepare 解析实际路由；刷新环境；冻结 request/header/messages。[源码][dsh-step] [冻结][dsh-build] | `StepContext` 捕获不可变 settings、环境、MCP、扩展数据、已加载指令及**本次实际 tool_router**。[源码][cx-step] | Web 先生成 `context_snapshot_id`，之后仍筛工具、删 CU 工具、追加工具策略；因此该 ID 不等于最终请求。[快照][cz-context-snapshot] [请求][cz-final-request] | **P0：统一 FinalRequestSnapshot**。重试、子任务、观测都引用同一版本，减少模型看到能力与实际执行不一致后的修复轮次 |
| 模型能力与协议 | 按具体模型能力决定 system 和工具更新投影；不支持时替换历史有效 system。[投影][dsh-prompt-project] [能力][dsh-model-capability] | 模型消息从本步 `ModelInfo` 渲染；配置可覆盖；增量工具要求模型 `use_responses_lite` 且开关开启。[模型][cx-model-prompt] [开关条件][cx-step] | Provider 适配器转换协议；Web 主体大量通用指导，最终 wire 请求仍在快照之后生成。[适配][cz-wire] | 保留模型原生模板和能力差异；不把“兼容 API”当作完整能力兼容。观测语义请求与实际 wire 摘要两层 |
| 工具发现／工具数 | ToolRuntime 有可见范围、schema 和 `deferLoading` 语义；native/PTC 由配置决定。[工具][dsh-tool-schema] | 有 ToolSearch 与延迟工具入口；旧 `tool_search` 开关已移除，不能看它的旧默认值判断当前无此功能。[声明][cx-toolsearch-flags] [处理器][cx-toolsearch] | Web 无工具意图时返回无 tools；小窗口还按类别和预算裁剪，MCP 至多 4 个；意图包含文本启发式。[判断][cz-tool-intent] [选择][cz-tool-select] | **强模型保留基础操作工具＋发现入口**，显式禁用仍生效。避免用户没用特定动词就失去查证能力；也避免一次塞入所有 schema |
| 同轮工具并行 | `isConcurrencySafe(args)` 精确为 true 才能并行；省略／异常为独占；排程有独占屏障、重新分类和按模型顺序提交。[资格][dsh-concurrency] [调度][dsh-tool-batch] | 注册的 parallel 资格决定读锁并行或写锁独占；进入执行前可取消，并分别记录等待与执行时间。[源码][cx-parallel] | Web 整批进 JoinSet＋信号量，默认上限 4，此层不分类读写；下层仍可能有专用锁／闸门。CLI 工具循环逐个执行。[Web][cz-parallel] [CLI][cz-core-dispatch] | **P1：两端共用并发资格和资源屏障**。独立读取并行；相同文件写入、有依赖的调用顺序执行。不是把所有操作串行，也不是无限扩大并发 |
| 执行权限和提示词关系 | pre-execute／审批后才过 guard；guard 只能拒绝或不干预，不能推翻先前拒绝。[源码][dsh-tool-gate] | 沙箱能力与审批策略分开；Skip、NeedsApproval、Forbidden 分支明确。[源码][cx-approval-gate] | 宿主权限预检在登记和 hook 前；预算、取消和来源身份在真实派发路径检查。[源码][cz-dispatch] | **三方都不是只靠系统提示词保护执行**。保留确定性硬门禁；减少无条件的文字确认要求。此类本地检查不等同于额外 LLM 思考 |
| 审批复用 | 审批由工具／策略层负责；不能由 persona 覆盖真实权限。[gate][dsh-tool-gate] | 指定动作有会话缓存：命令 key 包括环境、规范化命令、cwd、TTY 和权限；只有 ApprovedForSession 才缓存；并非所有工具通用缓存。[key][cx-approval-keys] [缓存][cx-approval-cache] | CLI 已执行 pre-hook 时重新授权；无 hook 时不重复。Web 也在 hook 后复核冻结权限和作用域。[CLI][cz-core-dispatch] [Web][cz-dispatch] | **P1：授权范围与事实版本绑定**。复核保留，事实未变且原授权仍覆盖时复用；hook 改路径／权限／目标则失效。不能直接删除第二道检查 |
| 子任务结果时机 | spawn 返回激活信息；父任务可以继续；管理器跟踪子树、wait 等待生命周期完成。[spawn][dsh-spawn] [子树][dsh-children] | 提供 Agent 控制和状态等待；wait 订阅状态直到完成或边界条件；V2 是另一个可选实现。[wait][cx-agent-wait] [开关][cx-multi-flags] | `HostChildAgent` 在工具 future 中等待子模型完整结果，再返回 completed；已有父预算和取消传播。[执行][cz-child-run] [权限][cz-child] | 当前结构适合简单委派，父模型要与子任务交错推进时需要 **spawn handle＋wait/cancel**。这不代表应给所有任务强制增加子代理 |
| 程序化工具组合 | 有 native/PTC；persistent runtime 为选择项，所有者、策略变化、取消和超时有生命周期处理。[策略][dsh-ptc-policy] | CodeMode 将本步允许的工具绑定到单元执行，支持 yield/wait；该版本内置默认 **关闭**。[执行][cx-code-mode] [默认][cx-code-flags] | DSH 插件宿主只桥接 tools，不带完整 DSH Session/AgentLoop/PTC 状态机。[源码][cz-dsh-host] | **P2**。大量小工具往返时可减少模型回合；简单任务可能反而增加调试成本。需要能力受限的组合执行，不能绕开原工具 gate |

### 3.2 系统提示词、计划与停止条件

| 维度 | DeepSeek Harness rc.2 | Codex CLI 0.163.0 | coolzhu 当前源码 | 建议与优劣判断 |
|---|---|---|---|---|
| 稳定规则与动态事实 | sections、tools、contexts、variables 分开；环境刷新不重跑全部规则组装；`complete` persona 可替换 system sections。[组装][dsh-prompt-assemble] [刷新][dsh-refresh] [persona][dsh-persona] | World State 分区并记录快照／变化；基础指导、模型指导、项目指令等分别编码；实际模板随模型目录和配置决定。[World State][cx-world-state] [模型][cx-model-prompt] | Web 拼 system＋memory beads＋技能，再把滚动摘要拼进同一 system；CLI 使用另一 builder。[记忆][cz-system-memory] [摘要][cz-summary] [CLI][cz-prompt-builder] | **P0：稳定规则、能力指导、项目指令、用户请求、历史证据分层**。有变化才提交有效变化，保证缓存友好；不要假设一定提升缓存命中率，需实测 |
| 历史资料的指令地位 | 项目指令和环境事实有来源，进入 user-role 上下文；文件指导不能覆盖更高层或人类直接指令。[渲染][dsh-instruction-render] | 压缩摘要为 user；记忆提取证据为 user，记忆读取指导为 developer。不是“所有记忆都 user”。[摘要][cx-summary] [记忆][cx-memory] | 摘要和记忆进入 system 字符串，来源边界被弱化。[源码][cz-system-memory] [源码][cz-summary] | 将“如何读取记忆”与“记忆里写了什么”分开。可信授权来自宿主身份；摘要不能创造权限，也不应抹去仍有效的人类任务 |
| 项目指令发现与变更 | 成功结构化文件触达、pre-step/step-end、resume、压缩等生命周期处理增改撤销；不是任意外部文件修改监听。[触达][dsh-instruction-touch] [pre-step][dsh-instruction-prestep] | 支持 AGENTS.override.md／AGENTS.md／fallback 和层级预算；World State 能表示替换／撤销。**管理器会缓存项目内容**，此路径按环境选择／信任变化刷新，不能称为每步自动重读磁盘。[发现][cx-agents-discovery] [缓存][cx-agents-cache] [变化][cx-agents-diff] | core builder 发现 CLAW 系列文件并拼 system；Web 路径不同，尚不能视为统一生命周期。[发现][cz-instruction-discovery] [Web][cz-assembly] | 统一指令注册表、内容 hash、作用域和撤销语义。按触达或明确刷新失效，不每轮递归扫全仓库 |
| 普通执行与 Plan | Plan 插件激活才加入特定规则；计划工具集合保持稳定；真正只读仍需权限实现。[计划][dsh-plan-policy] | Default 鼓励合理假设后执行；Plan 单独强调详细决策和非修改活动，不能把更新任务清单等同于进入 Plan。[Default][cx-default] [Plan][cx-plan] | 通用 guide 要求多步骤先发现上下文；Goal 自主执行要求与历史任务规则并存。[源码][cz-inline-guide] | **普通任务默认执行；高不确定或用户要求时进入规划模式**。保留必要调查，不强制多次写计划、确认、重规划 |
| 用户追加消息如何影响授权 | Goal 依据当前根 Agent、活跃轮次和可信 source 判断；非人类生产者需正确传 source。[来源][dsh-goal-authority] [默认陷阱][dsh-human-source] | 运行中设置与权限更新有捕获版本；动作审批可按会话范围复用；这不能替代应用自己的任务语义管理。[步骤][cx-step] [缓存][cx-approval-cache] | guide 将最终 user 消息当作当前请求，早先任务只在当前再次要求时继续，可能把状态追问／补充误当作重置。[源码][cz-inline-guide] | **P0：显式 TaskScope＋授权记录**。补充／追问更新当前任务；取消／替换才结束。保留跨回合有效授权，同时拒绝摘要、工具、子任务伪造授权 |
| 推理强度 | 请求准备绑定实际模型路由及配置；不能仅凭 Harness 判断自动难度调节已实现。[prepare][dsh-prepare] | effective effort = 当前选择，否则模型默认；Default preset 不强制 effort，Plan preset 为 medium。[选择][cx-effort] [preset][cx-presets] | 默认 auto；适配器里的 Auto 表示遵循提供方默认，不是已实现任务难度自适应。[默认][cz-effort-default] [解析][cz-effort] | 优先保持用户／模型的原生强度。用更少的无收益请求降成本，不用统一降档掩盖编排问题；机械恢复可直接生成事实回执 |
| 普通回合什么时候结束 | 无工具调用即完成；工具可 concludesTurn；Goal 续跑是另层机制。[源码][dsh-step-end] | 没有后续工具／输入时走停止路径；Stop hook 有阻断内容才继续，Goal 是扩展。没有发现“所有普通任务必经 verifier 模型”的通用阶段。[停止][cx-stop] | CLI 无 ToolUse 就结束；Web 有工具轮数上限、文本恢复和监管提示；Goal 阶段另有文件／命令验证。[CLI][cz-core-dispatch] [恢复][cz-recovery] [Goal][cz-goal-verify] | 普通任务以请求完成与必要证据为结束条件。Goal 的强验收按任务启用，验证失败要有具体证据，不因为预算剩余而继续执行 |
| Goal 的自治与完成 | 长期目标指导允许推断创建意图；根身份、paused、goal-round 阈值有运行时约束；blocked 阈值不验证语义上是否同一原因。[身份][dsh-goal-authority] [状态][dsh-goal-update] [阈值][dsh-goal-block] | create_goal 描述要求显式请求；运行时空闲续跑；update 状态检查和预算记账实际执行；“同一阻塞 3 次”主要来自工具描述，不能当成独立语义验证器。[描述][cx-goal-spec] [更新][cx-goal-update] [续跑][cx-goal-runtime] | 配置 Goal 时有 phase、角色路由、重试和 FilesExist／Command 验证；修复或重规划成功后仍回原链，不是所有普通任务都经过这些角色。[路由][cz-goal-stage] [门][cz-goal-verify] | 保留 coolzhu 的确定性验收优势。Goal 用于确需持续推进的任务；不从“做一次小修改”自动扩展成多角色长流程 |

## 4. 容易造成过度思考／执行的具体位置

“直接成本”表示源码分支能确认多了一次等待、模型请求或串行阶段；“潜在成本”表示存在触发风险，发生率和实际延迟要由运行记录确认。

| 位置／条件 | 源码确认的行为 | 成本性质与问题 | 对齐处理 |
|---|---|---|---|
| coolzhu Web 默认 guide | `agent_tool_usage_instruction()` 无条件拼入 CU 约束；即使最终工具表没有 CU，通用指导仍可能在 system 中。[源码][cz-cu-guide] | **潜在**：无关 token、矛盾解释、模型关注无关确认规则；不能据此断言已经执行截图 | 按实际本步能力／任务注入 CU 指导。保留实际 CU 场景的定位和状态验证要求，不把删截图当作通用加速办法 |
| coolzhu 历史任务规则 | 早先消息被表述为非待办，继续历史任务需当前再要求。[源码][cz-inline-guide] | **潜在**：用户一句“进展如何”可能打断原目标、诱发重新确认；这里是提示词风险，不是已测失败率 | 用 TaskScope 表达任务状态、有效授权、最新补充及显式撤销，避免模型反复推测授权是否重置 |
| coolzhu 工具意图启发式 | 工具关闭或无工具意图时直接不给 tools；后续文本恢复仍不给工具。[选择][cz-tool-select] [恢复][cz-recovery] | **潜在**：强模型本可查证却只回答；缺能力再恢复不能补做外部动作 | 强模型模式常驻基本工具和发现入口；显式禁用优先；纯聊天可让模型自行选择不用工具 |
| coolzhu L1 重复判定 | 已经执行完工具后，按工具名＋输入形成整批签名；连续两批一致即提示“无进展”，不比较输出／资源版本。[源码][cz-repeat] | **潜在**：相同参数轮询得到新结果也可能被纠偏；这是提示注入，不是防止第二次副作用的执行去重 | 进度判断增加输出状态、资源版本、预期等待；识别 wait/poll；调用身份去重继续独立用于防止重放 |
| coolzhu 文本恢复分支 | 未暴露工具、伪工具文本、部分 CU 终态或工具轮数耗尽等情况下，发一次 `tools=None` 的模型请求。[分支][cz-recovery-branch] [实现][cz-recovery] [上限][cz-loop-cap] | **直接，条件触发**：增加一次模型往返；这段路径没有独立的低推理强度选择。不是每轮都会触发 | 先按错误分类：可重试网络故障、参数可修复、策略拒绝、已产生效果、纯表达问题。明确失败回执可由宿主直接呈现；仅需要组织最终内容时请求模型 |
| coolzhu Web／CLI 并发差异 | Web 只在批次入口限总数；CLI 逐个 await。[Web][cz-parallel] [CLI][cz-core-dispatch] | **直接**：CLI 独立慢读取会累加；Web 若调用间有依赖，可能产生冲突后修复成本，但需结合工具内部约束判断 | 共享调度分类；先支持独立读取并行，再引入资源冲突键；把等待时间与工具执行时间分开统计 |
| coolzhu Goal 提示与角色 | 阶段 prompt 再附 `AGENT_GUIDE_INLINE`，系统 guide 路径已有同文；基线 Goal skills 也另加载；失败后可切换角色。[prompt][cz-goal-prompt] [路由][cz-goal-stage] | **潜在／配置相关**：重复规则及多角色重述；模型自行跑验证再由 runtime 跑同一验证可能重复，当前未证明每次发生 | 通用指导单一来源；阶段只注入差异、验收合同和已有证据。相同 revision＋环境＋命令的有效结果可复用 |
| coolzhu pre-hook 后再次授权 | CLI 仅在 pre-hook 真运行过时重新授权；Web 仍重新核对真实约束。[源码][cz-core-dispatch] [Web][cz-dispatch] | **条件成本**：CLI 可能再次进入人工批准通道；但 hook 可能改变目标，复核有必要 | 分开“重新验证授权覆盖”与“再次询问”。仅在授权范围／相关事实改变后重新请求批准 |
| Codex Plan 模式 | 明确要求决策完整、较多澄清及非修改探索。[源码][cx-plan] | **模式相关**：把模板作为所有任务默认会放大准备成本 | 普通执行使用 Default 思路；Plan 是用户可见、可退出的选择，不是所有模型调用的隐藏前置阶段 |
| Codex 自动审批与 strict 分支 | 常规 Skip 不审批；`strict_auto_review` 下 Skip 也进入审核。该标志可来自本轮额外权限授权，不能作为 session strict grant 保存。[gate][cx-approval-gate] [grant][cx-strict-grant] | **配置／授权相关**：额外审核可能耗时；不能说“Codex 每步都审核”或“从不审核” | 普通可判定操作走本地策略；额外模型审核必须有清晰触发原因、缓存与观测，不能默认覆盖每个只读调用 |
| Codex Stop hook／请求重试 | Stop hook 带续跑内容可让已准备结束的回合继续；provider 默认 stream retry=5、request retry=4、stream idle timeout=300s，是不同层参数。[停止][cx-stop] [参数][cx-retries] | **条件成本**：扩大尾延迟；不能直接相加或相乘当作任务总尝试数 | 同一失败类别共享总预算；Stop hook 必须给出可执行的新证据，不得无限以“再检查一次”续跑 |
| DSH `always` 重试 | 可选重试插件遇到 always 模式会继续，直到成功、取消或销毁；normal 默认最多 5 次且只匹配瞬态错误。[策略][dsh-retry-policy] [实现][dsh-retry] | **配置相关**：无上限策略不符合本任务“避免无收益反复执行”的目标 | 学习错误分类、退避、可取消，采用有界总预算；不照搬 always |

官方说明也将普通沙箱内执行与需要审核的审批请求区分开；自动审核不会自行扩大沙箱权限。实际代码另有上述 strict 分支，决定行为时要看生效配置与授权状态。[自动审核说明](https://learn.chatgpt.com/docs/sandboxing/auto-review)、[执行边界说明](https://learn.chatgpt.com/docs/agent-approvals-security)。

## 5. 推荐方案：强模型默认自主，硬约束按客观条件触发

### 5.1 三种路径的决策

| 方案 | 收益 | 主要代价 | 建议 |
|---|---|---|---|
| 整体换成 DSH Harness | 快速接近其插件和会话生态 | TypeScript 运行时、日志、UI、权限、Goal 与现有 Rust 底座整合成本大；工具桥接不等于完整迁移 | 本阶段不作为首选 |
| 整体换成 Codex 核心 | 与其 Rust 请求和工具生命周期靠近 | 上游体量大，模型目录、协议、扩展和应用状态耦合；升级维护成本高 | 本阶段不作为首选 |
| **保留 coolzhu 执行底座，提取三方契约统一 Web／CLI** | 保留已经实现的真实权限、预算、取消和验证；能逐项证明减少无效往返 | 需要明确请求快照、来源、调度与测试接口 | **推荐；每阶段可独立开关、比较和回退** |

### 5.2 目标运行方式

| 决策点 | 建议默认 | 什么时候升级处理 |
|---|---|---|
| 是否规划 | 模型在当前请求内形成必要步骤并开始工作；工具清单型计划是可选辅助 | 用户明确要方案，或关键决策不可从上下文判断时进入 Plan／提问 |
| 是否问权限 | 已授权、范围内、可确定性验证的动作直接执行 | 新的外部副作用、越出授权范围、权限／目标发生变化，或产品强制策略要求 |
| 是否使用工具 | 保留本步允许的基本操作和发现能力，由模型决定是否调用 | 用户显式关闭工具、提供方能力不足、权限拒绝时限制 |
| 是否多代理 | 单模型作为一般执行入口 | 可独立并行的子目标有明确收益，且用户／产品策略允许时委派 |
| 是否重试 | 瞬态失败有限重试；同一根任务共享预算；重试前知道是否已产生副作用 | 不可判定的效果先核对执行回执；策略拒绝不换工具绕过 |
| 是否继续验证 | 跑与变更相关的必要检查；保存验证证据 | 新改动、测试失败、环境／依赖变化或未解除的具体疑点才重复／扩大 |
| 推理强度 | 保留用户选择和具体模型默认；不要固定所有任务最高档或最低档 | 只有明确任务需要或用户选择才调节；评估质量与总耗时，不能只看单次请求时间 |
| 是否结束 | 用户要求的结果与必要验收成立即结束 | 明确未完成目标或有新输入才继续；剩余 token／时间不是继续工作的理由 |

以上是**拟议产品策略**，不是声称当前任一工程已经完整实现。硬约束应回答“何种事实触发、在哪执行、如何失效、是否新增模型请求”；不能只增加一段要求模型反复自查的提示词。

### 5.3 分阶段工作单

| 编号／优先级 | 实现范围与借鉴对象 | 直接验收 | 与已有 DSH 对齐报告关系 |
|---|---|---|---|
| T01／P0 | `FinalRequestSnapshot`：实际模型、effort、能力版本、语义消息、工具 schema、规则 revision、TaskScope；Web／CLI 共用。借鉴 Codex StepContext、DSH request freeze | 修改下一步设置不改变已发出请求；执行工具属于本步暴露范围；重试不重复接纳用户输入；可对照最终 wire 摘要 | 承接 H01、H02、H10 |
| T02／P0 | PromptComposer：规则／模型指导／能力指导／项目指令／记忆事实分层；CU 按能力注入；Goal 只注入阶段差异 | 非 CU 请求无 CU 行为段；记忆不能提升权限；相同规则不重复；清空、替换和压缩后不复活旧规则 | 承接 H01、H03、H04 |
| T03／P0 | TaskScope 与可信来源：用户补充、状态追问、取消、替换分开；授权绑定目标／作用域／有效期 | “进度如何”不清空任务；明确取消停止新派发；工具／摘要／子代理不能伪造用户授权；范围扩大重新判断 | 承接 H04；解决当前 guide 的连续性问题 |
| T04／P1 | 强模型工具策略：基础工具＋发现入口；小窗口按能力和预算逐步加载 | 去掉特定动作词仍可合理查证；显式禁用不暴露；找不到工具给真实缺失原因，不进行无工具“补执行” | 承接 H02；新增发现与暴露策略 |
| T05／P1 | 共用调度、带范围的审批缓存、hook 后事实复核；借鉴双方并发资格与 Codex 缓存 key | 独立读取重叠执行；同资源依赖写入有序；权限变化使缓存失效；未改变范围不重复打扰用户 | 保留现有身份／预算／取消，不推倒重写 |
| T06／P1 | 分类恢复与进度检测：输出／资源版本＋调用身份＋有限重试预算；明确终态可直接生成事实回执 | 正常轮询不误判；已执行副作用不重放；相同不可恢复错误不无限新建模型轮次；恢复请求次数可统计 | 新增延迟重点项；不删除现有执行去重 |
| T07／P1 | 验收证据缓存：revision、相关文件、命令、工具链与环境指纹；按需 Goal／角色 | 相同证据有效期内不重复同一全量验证；新改动使相关证据失效；失败不能被模型一句“已完成”覆盖 | 保留 coolzhu FilesExist／Command 验证优势 |
| T08／P2 | 异步子任务 handle、wait/cancel、PTC／CodeMode 试点 | 父任务可继续独立工作；取消传播；生命周期稳定；工具组合不能超出能力上限；只有往返确实降低才推广 | 承接 H05、H06、H09 |

P0 先建立可观察的事实和正确作用域，P1 再减少不必要往返和串行等待，P2 处理复杂任务吞吐。DSH 插件依赖升级可作为独立事项，但它不会自动交付以上契约。

## 6. 如何验证“释放能力且不拖慢执行”

### 6.1 不把模型差异误记为 Harness 收益

优先在 coolzhu 内做同一模型、同一版本、同一 effort、同一工具实现、同一环境与任务的配置 A/B。跨工程对比作为补充：若模型适配、工具实现或权限不同，必须列出差异。静态源码不能给出可信的“快多少百分比”。

| 指标 | 记录口径与用途 |
|---|---|
| 成功率／验收通过率 | 先满足真实任务质量；不能靠早停、漏做验证换速度 |
| 总完成耗时 p50／p95 | 从用户请求接纳到最终可交付；单列成功、失败、人工等待，避免混合分布掩盖问题 |
| 首次有效动作耗时 | 到首个真正推进任务的工具／产物；计划文字不是有效工具动作 |
| 模型请求次数与推理 token | 区分主执行、规划、恢复、审核、压缩、子任务和 Goal 续跑；提供方不返回推理 token 时标为未知 |
| 工具次数与重复效果 | 区分合法轮询、重试、身份重放被拒绝和真正重复副作用 |
| 等待构成 | 模型耗时、工具排队、执行、人工审批、自动审核、验证、退避；并发时间按关键路径分析，不能简单求和 |
| 提示／缓存指标 | 稳定前缀大小、动态片段变化、工具 schema 数量、实际缓存 token；避免把 prompt 长度直接当成推理耗时 |
| 约束触发率 | 无工具拒绝、L1 纠偏、文本恢复、重复批准、验证重跑；每个事件携带触发事实和规则版本 |

### 6.2 最小验收任务集

| 场景 | 验证能力与约束 | 不应出现的额外成本 |
|---|---|---|
| 单文件小改动 | 读取必要上下文、修改、相关检查通过、最终说明 | 无理由进入正式 Plan、多角色 Goal、重复全量测试 |
| 多文件重构 | 模型能自主拆解；独立读取并行；变更相关验证有效 | 每一步都重复确认原授权，或每读一个文件就重新规划 |
| 长进程／轮询 | 同参数返回不同进度算推进；停止等待可取消 | 被“相同输入”纠偏后换无关工具；忙轮询 |
| 工具短暂网络故障 | 有界退避；只重试尚未产生效果的操作 | 所有错误都交给新模型反思；跨层重试无总预算 |
| 已产生副作用后断流 | 执行回执／身份可恢复，先核对实际结果 | 为得到文本答案重放写文件、发请求等副作用 |
| 追加“进展如何／再补一项” | 原任务和授权连续，新增范围单独判断 | 丢原目标、重新要求重复授权、重复已完成步骤 |
| 项目指令变化／删除／压缩 | 新指令正确接纳，旧指令撤销，记忆只作证据 | 每步全仓库扫描，或摘要恢复旧规则／伪造授权 |
| 子任务运行时取消 | 父子预算、取消及时生效；不再接纳后续执行 | 父任务退出后子任务继续花费模型／工具资源 |
| 验证通过后无新变更 | 已有证据可复用，按要求交付结束 | 预算还剩就继续检查，或审核角色要求重新跑相同命令 |

建议先以成功率不下降、未授权执行为零、重复副作用为零作为边界，再比较 p50／p95、无收益模型请求与重复验证的下降。具体数值目标应由基线运行结果决定，本报告不虚构性能提升幅度。

## 7. 可复用的上游测试与研究边界

本次阅读测试定义以提取验收意图，**没有执行这些测试**。

| 测试入口 | 对齐时可复用的性质 |
|---|---|
| [DSH request-freeze][test-freeze] | 请求接纳后不能被后续可变对象污染 |
| [DSH system-prompt projection][test-clear] | 清空规则后不让历史 system 节点复活 |
| [DSH 指令压缩恢复][test-instruction-compact] | 压缩后重建有效指令，保持生命周期语义 |
| [DSH 文件观察策略][test-file-policy] | 未观察的创建不覆盖现有文件；编辑需要有效版本 |
| [DSH persistent runtime][test-ptc] | 状态所属对象和清理边界 |
| [Codex sparse step settings][cx-test-step] | 新设置按顺序生效，旧步骤保留捕获内容 |
| [Codex 指令刷新取消][cx-test-refresh] | 刷新串行化，取消释放等待 |
| [Codex 取消派发观测][cx-test-timing] | 未进入 handler 的调用不能被记录成实际执行工作 |
| [Codex 沙箱策略测试][cx-test-sandbox] | 审批与实际沙箱边界分开，额外权限不能覆盖显式拒绝 |
| [coolzhu 派发 hook／预算测试][cz-hook-tests] | 保留现有先权限后 hook、拒绝时不执行及预算约束 |

Codex 本版本内置 `CodeMode`、`IncrementalTools`、`AsyncCompaction`、`MultiAgentV2` 默认均关闭；`GuardianApproval` 开关默认开启表示功能可用，不代表所有工具都走模型审核。实际 CLI 配置、托管策略、模型目录和应用覆盖仍会影响运行路径。[CodeMode][cx-code-flags]、[增量工具][cx-incremental-flags]、[异步压缩][cx-compact-flags]、[多代理][cx-multi-flags]、[审核功能][cx-guardian-flag]。

源码位置是本次固定快照的行号；coolzhu 仍有用户原有工作区改动，后续修改后行号可能变化。本次 `main.rs` 内容哈希与上一份 DSH 报告不同，相关行为已按本次读取内容核对。引用文件与报告哈希保存在[三方源码证据清单](D:/coolzhuagent/docs/analysis/2026-10-17-three-way-harness-source-evidence.json)。主要限制是未做动态配置枚举、生产请求日志分析和真实模型耗时实验，因此“潜在浪费”不等于已确认性能故障。

<!-- source-references -->

[cx-agent-wait]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/handlers/multi_agents/wait.rs:322
[cx-agents-cache]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/agents_md_manager.rs:89
[cx-agents-diff]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/context/world_state/agents_md.rs:44
[cx-agents-discovery]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/agents_md.rs:58
[cx-approval-cache]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/sandboxing.rs:72
[cx-approval-gate]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/orchestrator.rs:166
[cx-approval-keys]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/approvals.rs:220
[cx-code-flags]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/features/src/lib.rs:1160
[cx-code-mode]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/code_mode/execute_handler.rs:64
[cx-compact-flags]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/features/src/lib.rs:2058
[cx-default]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/collaboration-mode-templates/templates/default.md:11
[cx-effort]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/session/step_settings.rs:85
[cx-goal-runtime]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/ext/goal/src/runtime.rs:436
[cx-goal-spec]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/ext/goal/src/spec.rs:46
[cx-goal-update]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/ext/goal/src/tool.rs:247
[cx-guardian-flag]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/features/src/lib.rs:1830
[cx-incremental-flags]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/features/src/lib.rs:1590
[cx-memory]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/context/memory.rs:14
[cx-model-prompt]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/prompts/src/model_instructions.rs:8
[cx-multi-flags]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/features/src/lib.rs:1470
[cx-parallel]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/parallel.rs:141
[cx-plan]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/collaboration-mode-templates/templates/plan.md:7
[cx-presets]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/models-manager/src/collaboration_mode_presets.rs:6
[cx-retries]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/model-provider-info/src/lib.rs:66
[cx-step]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/session/step_context.rs:23
[cx-stop]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/session/turn.rs:718
[cx-strict-grant]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/session/mod.rs:3353
[cx-summary]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/context/compaction_summary.rs:17
[cx-test-refresh]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/session/step_activation_tests.rs:104
[cx-test-sandbox]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/sandboxing_tests.rs:190
[cx-test-step]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/session/step_activation_tests.rs:508
[cx-test-timing]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/parallel_observation_tests.rs:20
[cx-toolsearch]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/tools/handlers/tool_search.rs:209
[cx-toolsearch-flags]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/features/src/lib.rs:260
[cx-world-state]: C:/Users/zhupu/Desktop/codex-cli-0.163.0/codex-rs/core/src/context/world_state/mod.rs:321
[cz-assembly]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28587
[cz-child]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/host_child_agent.rs:55
[cz-child-run]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/host_child_agent.rs:298
[cz-context-snapshot]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28841
[cz-core-dispatch]: D:/coolzhuagent/modules/core-runtime/packages/core-runtime/src/conversation.rs:323
[cz-cu-guide]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28347
[cz-dispatch]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/tool_dispatch_service.rs:27
[cz-dsh-host]: D:/coolzhuagent/modules/tooling/packages/dsh-plugin-host/src/host.mjs:143
[cz-effort]: D:/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/reasoning.rs:686
[cz-effort-default]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:51002
[cz-final-request]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:33532
[cz-goal-prompt]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:46973
[cz-goal-stage]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/goal_phase_stage.rs:15
[cz-goal-verify]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:40105
[cz-hook-tests]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/dispatch_hook_budget_tests.rs:97
[cz-inline-guide]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28321
[cz-instruction-discovery]: D:/coolzhuagent/modules/core-runtime/packages/core-runtime/src/prompt.rs:242
[cz-loop-cap]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28182
[cz-parallel]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:27687
[cz-prompt-builder]: D:/coolzhuagent/modules/core-runtime/packages/core-runtime/src/prompt.rs:156
[cz-recovery]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:35578
[cz-recovery-branch]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28019
[cz-repeat]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28095
[cz-summary]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28770
[cz-system-memory]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28300
[cz-tool-intent]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:32851
[cz-tool-select]: D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:33632
[cz-wire]: D:/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/providers/openai_compat.rs:1050
[dsh-build]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/src/agent.ts:616
[dsh-children]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/subagent/subagent/src/manager.ts:252
[dsh-concurrency]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/tools/src/index.ts:1311
[dsh-goal-authority]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/goal/tool-goal/src/authority.ts:49
[dsh-goal-block]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/goal/tool-goal/src/index.ts:305
[dsh-goal-update]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/goal/tool-goal/src/index.ts:270
[dsh-human-source]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/goal/tool-goal/src/authority.ts:83
[dsh-instruction-prestep]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/context/agent-instructions/src/index.ts:330
[dsh-instruction-render]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/context/agent-instructions/src/render.ts:16
[dsh-instruction-touch]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/context/agent-instructions/src/index.ts:76
[dsh-model-capability]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/llm/llm-pi-ai/src/models.ts:75
[dsh-persona]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/preset/persona/src/index.ts:60
[dsh-plan-policy]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/bundle/base/cordis.patch.yml:332
[dsh-prepare]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/src/agent.ts:564
[dsh-prompt-assemble]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/system-prompt/src/index.ts:614
[dsh-prompt-project]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/src/runtime-context.ts:88
[dsh-ptc-policy]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/ptc-runtime/ptc-runtime-node/src/policy-processes.ts:21
[dsh-refresh]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/system-prompt/src/index.ts:582
[dsh-retry]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/llm/llm-retry/src/index.ts:194
[dsh-retry-policy]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/llm/llm/src/retry-policy.ts:14
[dsh-spawn]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/subagent/tool-subagent/src/index.ts:337
[dsh-step]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/src/agent.ts:400
[dsh-step-end]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/src/agent.ts:547
[dsh-tool-batch]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/src/tool-calls.ts:60
[dsh-tool-gate]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/tools/src/index.ts:1501
[dsh-tool-schema]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/tools/src/index.ts:1280
[test-clear]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/tests/system-prompt-projection.spec.ts:203
[test-file-policy]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/fs/fs-observation-policy/tests/policy.spec.ts:116
[test-freeze]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/core/agent-loop/tests/request-freeze.spec.ts:46
[test-instruction-compact]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/context/agent-instructions/tests/agent-instructions.spec.ts:4764
[test-ptc]: C:/Users/zhupu/Desktop/deepseek-harness-v0.2.1-rc.2/packages/ptc-runtime/ptc-runtime-node/tests/persistent.spec.ts:180
