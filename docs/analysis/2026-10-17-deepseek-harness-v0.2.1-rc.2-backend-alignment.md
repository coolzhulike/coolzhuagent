# DeepSeek Harness v0.2.1-rc.2 后端更新与 coolzhuagent 对齐建议

截至 2026-10-17，官方最新发布为 `dsh-v0.2.1-rc.2`，仍是预发布版本。发布时间为 2026-10-17 08:16:33 UTC，即洛杉矶时间 10 月 17 日 01:16:33。目标提交为 `687f282e8625c6b8d11ff58211a90524be2e4317`。[官方 rc.2 发布页][release-rc2]

**rc.2 单独的增量很小，功能对齐应重点看整个 0.2.1 系列。** rc.2 接入新的搜索和抓取 API，并适配 Flash 1017；提示词、子代理、工作目录、PTC 和 SDK 的主要更新已经包含在 rc.1 及此前的 alpha 中。官方 rc.1 的累计比较基线是 `v0.2.0-rc.2`。[rc.1 累计说明][release-rc1]

对 coolzhuagent 最有价值的顺序是：先统一提示词和请求状态契约，再补齐模型能力适配、项目指令生命周期和子任务等待，随后接入新搜索 API、升级插件兼容层。现有 Rust 权限检查、执行记录和 Goal 验证门应继续作为基础。

完整源码现已下载到桌面；函数调用链、具体约束边界和测试位置见[源码级对齐报告](D:/coolzhuagent/docs/analysis/2026-10-17-deepseek-harness-source-level-alignment.md)。该报告将 H10 中的最终请求关联提升为 P0，并细化了 required context、Goal 阻塞阈值与已暂停 Goal 的恢复限制。

## 比较范围

| 比较层次 | 基线 | 用途 |
|---|---|---|
| rc.2 单次增量 | `dsh-v0.2.1-rc.1 → dsh-v0.2.1-rc.2` | 区分本次新增与历史能力 |
| 0.2.1 累计变化 | `dsh-v0.2.0-rc.2 → dsh-v0.2.1-rc.2` | 后续功能对齐的主要范围 |
| 当前机制 | 固定到 rc.2 提交的官方文档和实现 | 解释 harness 和约束实际如何工作 |
| coolzhu 现状 | `D:\coolzhuagent`，HEAD `8a13551b037f5f673188802f2c9128984245fab8` 加读取时的未提交修改 | 定位已有基础和待补齐接口 |

coolzhu 结论来自当前工作区的静态源码核对，不代表安装包已通过相同功能验收。本次未执行模型调用、端到端测试或源码升级。

## rc.2 单次新增

### Flash 1017 适配

官方宣布适配 DeepSeek-V4.1-Flash-1017。源码中的临时 `1017` 标识只改变模型选择器展示，保留目录名称和请求 ID：适用官方 `deepseek-official`、`deepseek-account` 路由，模型 ID 仍为 `deepseek-flash`。**不能据此给 coolzhu 硬编码一个 `deepseek-flash-1017` 请求 ID。**[模型标识插件][model-label]

### 独立 Search 和 Fetch API

新增 `web-deepseek-api` 插件，在 Web 能力服务下分别注册 `deepseek-search-api` 与 `deepseek-fetch-api`。默认仍选择 Messages 搜索和本地 HTTP 抓取；开启网页搜索设置中的新 API 开关并保存后，两条路由一起切换，下一次操作生效。[API 插件][web-api]、[Web 能力服务][web-service]

| 项目 | rc.2 行为 | coolzhu 接入时需要保留的语义 |
|---|---|---|
| 地址 | 默认 `https://api.deepseek.com/v1/tools`，再追加 `/search` 或 `/fetch` | 与模型 API 地址独立配置，保留部署路径前缀 |
| 搜索条数 | `count` 默认 10，范围 1–50；调用方更小的 `maxResults` 优先 | 不把条数设置和旧 Messages 的搜索轮数混为一谈 |
| 原始问题 | 默认传递发起会话最近的人类问题，折叠空白后截到 128 字符，字段为 `ori_question` | 暴露 `sendOriginalQuestion` 开关，设置说明明确其发送行为 |
| 凭据 | API Key 或 DeepSeek 账号令牌；一次操作使用同一配置快照 | 不在请求中途混用新地址与旧凭据 |
| 结果 | Search 映射 snippet 和可选日期；Fetch 返回 Markdown | Fetch 的 HTTP 200 是服务层成功，不能当作目标网页的真实状态码；结果不提供最终重定向地址 |
| 失败 | 拒绝 HTTP 重定向；不自动重试或切换提供方 | 保留取消、凭据和端点错误；回退须由宿主明确决定 |
| 模型开销 | 直接检索不再消耗辅助模型请求 token | 检索服务仍可能独立计费，不等于免费 |

旧 Messages 搜索仍可用，但辅助请求明确关闭 thinking，避免生成随后被丢弃的推理内容。[旧搜索实现][messages-search]

### rc.2 没有重写核心提示词引擎

rc.1 到 rc.2 共 9 个提交，含合并和版本发布提交。两版 Git tree 中，`core/system-prompt/src/index.ts`、`core/agent-loop/src/agent.ts`、`context/agent-instructions/src/index.ts` 的 blob 身份相同。因此，下文这些核心机制不能标为“rc.2 首次新增”。[官方版本比较][compare-rc2]

## 0.2.1 系列的主要后端变化

以下“累计变化”以官方 `v0.2.0-rc.2` 为基线；具体当前行为以链接中的固定提交为准。

| 领域 | 累计变化及当前行为 | 对齐价值 |
|---|---|---|
| 提示词与工具动态更新 | pi-ai 根据具体模型目录能力决定是否支持会话中更新 system prompt、增加工具或移除工具；自定义路由不能靠配置自行宣称支持。[能力映射][pi-models] | 多模型切换、热加载插件时保持正确历史和工具集合 |
| 动态环境上下文 | `PromptContext` 增加 `required` 和 `interpolate`；新增 `refreshContext()`，请求接纳前刷新环境事实，保留已绑定的规则、工具和变量。[提示词实现][prompt-source] | 防止工作目录、沙箱状态等在异步准备后过期 |
| 进程环境与恢复 | 实现目录、Web URL 从 system prompt 移到动态上下文；旧会话保留原提示词段落，追加当前值，避免升级本身破坏历史签名。[变更设计][runtime-note] | 重启、换安装目录或随机端口后继续旧会话 |
| 项目指令 | 新增共享 agents 根目录，支持 `DSH_AGENTS_HOME`；移除插件行上的 `dshHome` 配置入口；切换会话工作目录后重建适用指令链。[指令插件][instructions] | 全局、项目、子目录约束能正确继承、更新和撤销 |
| 子代理 | 工具接纳后立即返回 child ID，运行时负责完成通知；子代理角色和工具改由 Agent Preset 组合；旧 `persona`、`toolFilter` 配置须迁移。[委派工具][subagent]、[迁移设计][preset-note] | 分离“成功启动”和“真正完成”，避免父轮结束误杀后台任务 |
| Goal 与等待 | 正在推进的后代任务未结束时不启动新的 Goal 轮次；轮次只在相应消息进入模型历史后计数。[Goal 驱动器][goal-driver] | 避免等待子任务时空转并消耗推进次数 |
| SDK 与工作目录 | TS/Python SDK 增加 resume 和目录读写；自定义 SDK runtime 需实现 `session/wait`，等待根会话及仍在推进的后代任务。[SDK 协议][sdk-protocol]、[TS 客户端][sdk-client] | 恢复原会话身份，建立明确的整体完成语义 |
| PTC 程序化工具调用 | Node 新增可选 `persistent-in-memory` 环境；Python 接入 Session 文件沙箱；删除 `both`，只保留 `native`、`ptc`。[Node PTC][ptc-node]、[Python PTC][ptc-python]、[工具运行时][tools] | 程序变量复用与文件权限分开管理 |
| 压缩 | 极简模式接入读图和自动压缩；Web/桌面支持 `/compact`。当前压缩保留工具调用配对，复用可复用的请求前缀，压缩后的最新提示词由循环重新协调。[压缩实现说明][compaction] | 长会话不丢有效规则，也不让旧规则重新生效 |
| 插件生命周期 | 自定义索引、依赖同步、可替换插件热重载；兼容性优先看 `engines.dsh`；子路径元信息和 invariant 入口有破坏性变化。[插件管理][plugin-manager]、[累计发布说明][release-rc1] | 升级宿主时明确版本、安装、启用和可执行状态 |
| 实验性 Session 状态 | 新增带类型名称的插件记录，回放和 fork 时保留；记录不进入模型上下文。官方把生产调用限制在仓库实验性包，不能直接视为稳定的通用第三方状态 API。[Session 记录][session] | 为插件状态设计版本化、可重建的持久化接口 |
| 其他后端能力 | SSH 独立 helper、网关调用方访问控制、实验性 Worktrees、开发者工具和 Claude Code Mods 兼容层。[SSH][ssh]、[网关][gateway]、[累计发布说明][release-rc1] | 按需求分批采用；SSH 当前要求本地和远端均为 Linux/macOS，不能直接当作 Windows 已支持 |

官方还列出流式请求空闲超时无法退出、关闭时提前释放资源、崩溃后任务误显示完成、会话存储卸载丢尾部记录等修复。对齐时应将这些故障场景纳入回归，而不是仅核对功能名称。[累计发布说明][release-rc1]

## harness 与系统提示词约束

### 稳定规则和动态事实分开

当前 DSH 把模型输入分成三个互相配合的部分：

| 部分 | 内容 | 更新方式 |
|---|---|---|
| System sections | Harness 身份、部署 persona、工具指导、计划规则等 | 按稳定顺序组装；会话级同名贡献可以覆盖全局贡献 |
| Runtime context | 工作目录、沙箱与审批状态、委派深度、团队身份、运行地址等 | 持久化为带来源的 user-role 上下文快照；内容不变时不重复注入 |
| Tool schemas | 当前作用域允许呈现的工具名称、参数和说明 | 与提示词在同一次 assembly 中确定，再由模型适配器按能力投影 |

这些基础分层和 in-history 提示词机制在 0.2.0 中已经存在。0.2.1 的关键增量是**模型能力覆盖扩大、必需上下文不可关闭、请求接纳前刷新，以及进程变化不再污染稳定前缀**。[系统提示词契约][prompt]、[运行时投影][runtime-projection]

`order` 控制拼接顺序，不是安全权限等级。目录更具体的规则也不能覆盖 system、developer 或人类直接指令；`<system-reminder>` 只是内容边界，不会把 user-role 消息提升成 system-role。[指令插件][instructions]

### 动态更新取决于实际模型能力

循环使用本次 `prepareCall()` 返回的能力裁决，而不是只看前一步的模型设置：[Agent 循环][agent-loop]

- 支持 `systemPromptUpdate: in-history` 且请求序列连续：新提示词追加到历史后部；相同内容不产生更新。
- 不支持、进入新请求序列，或工具变化不能被当前路由表达：把有效提示词归并到首个 system 节点，并清空后续旧版本。
- 提示词被清空：清除所有仍有效的 system 节点，不能让更旧版本“复活”。
- 工具变化有独立 `toolUpdate` 能力，区分完整更新与 `addition-only`；不支持的路由使用当前完整工具声明。
- 同一个重试步骤复用已接受的提示词和工具，不重复接纳用户输入；环境事实可以在派发前刷新。

这能保留满足条件的缓存前缀，但不是缓存命中率保证。追加的旧提示词版本仍占上下文，直到归并或压缩处理。[循环实现契约][agent-loop]、[LLM 工具历史投影][llm]

### 组合提示词必须验证

DSH 对组合错误直接失败：多个有效 `complete` section、未解析变量、非法工具排序等不会带着错误配置发给模型。`interpolate: false` 允许路径和生成文档保留字面的花括号。[系统提示词实现][prompt-source]

`complete: true` 的 persona 会成为完整 system prompt，默认身份、其他工具指导和 suffix 都可被移除；工具声明和必需运行环境仍独立存在。因此关键执行限制不能只藏在可被替换的提示词段落中。[Persona 契约][persona]

### 项目指令有可恢复的生命周期

默认顺序为 `$DSH_HOME/AGENTS.md`、`$DSH_AGENTS_HOME/AGENTS.md`，再到项目根至当前目录的 `AGENTS.md` / `CLAUDE.md` 及 local overlay。共享 agents 根默认是 `~/.agents`。内容会按候选组去重，并按预算优先保留更具体的规则。[指令发现与配置][instruction-config]

基线和后续变化写成带来源的持久消息；更新是替换内容，删除有显式撤销通知，resume 会重新核对。默认 base profile 的总预算是 65,536 字节。发现更深目录主要依赖成功的结构化 `read`、`write`、`edit`，并非监听所有外部修改或解析 shell 中的 `cd`。[指令实现][instruction-source]、[渲染实现][instruction-render]

对 coolzhu 的重点是把“文件名被提到”“内容曾读取”“当前范围仍适用”区分开；文件删除、工作区切换、压缩和重启后也要能确定哪些约束仍有效。

### 行为指导与执行限制不能混用

| 约束例子 | DSH 当前实际约束位置 | 对齐判断 |
|---|---|---|
| 计划模式先规划后行动 | `plan:policy` 提示词和审阅流程；**所有工具仍可调用** | 如果 coolzhu 定义计划阶段为只读，应继续由权限层限制，不能只移植文字。[计划模式][plan] |
| 工具只能在授权范围执行 | pre-execute 决策后还有单调拒绝的 guard；底层执行仍受各自策略控制 | 同时检查模型可见工具和实际派发权限。[工具运行时][tools] |
| PTC 模式只能直接调用 `run_code` | 提示词声明，加上调用入口对其他直接调用返回错误 | 模型说明与可执行入口一致。[工具源码][tools-source] |
| 修改文件先读取 | 提示词指导，加上观察记录和原子版本检查 | 仅存在 `expected_version` 参数还不等于所有入口都强制先读。[文件观察策略][fs-policy] |
| Goal 的来源、版本和阻塞阈值 | 活跃调用者、当前轮次、人类来源、goal revision 等运行时检查；自动 Goal 轮次中的模型 blocked 受可配置阈值限制，默认 3，直接人类来源不受该阈值限制 | “目标确实完成”“是否同一阻塞”仍包含模型语义判断；模型工具还单独禁止恢复 durable paused 状态。[Goal 工具][goal-tools] |
| 子代理不能执行提醒操作 | 新 preset 可继承提醒 schema，但执行器拒绝委派调用者 | 目录里有工具不等于本次调用获准。[子代理组合变更][preset-note] |
| 网页内容不改变指令 | 搜索/抓取工具的提示和结果边界 | 属于模型防护指导，不能替代执行器的权限检查。[Web 工具][web-tools] |

### 保留进程状态不能保留临时提权

Node PTC 的常驻环境与普通单次执行使用相同沙箱体系。Session 策略改变会触发取消和环境失效；下一次执行等待相关清理。一次性授权使用单独的临时环境，不继承常驻变量，也不把更高权限留给后续程序。Python 自定义组合还需提供 `sandbox`、`sandboxPolicy` 服务。[PTC 沙箱设计][ptc-sandbox]、[Node PTC][ptc-node]、[Python PTC][ptc-python]

### 子任务结束要有独立的事实依据

子代理工具返回 child ID 表示启动已被接纳，最终结果由完成通知交付。`session/wait` 等待根会话稳定 idle 以及仍在推进的托管后代；驻留但没有运行的子会话不等于待完成任务。Agent Team 的发送成功仅表示目标 Inbox 接受，不证明对方处理完成，也不保证同步落盘。[委派工具][subagent]、[SDK 协议][sdk-protocol]、[团队工具][team-tools]

Goal 驱动器本身没有独立的结果评估器，也没有把 token、金额和时间预算统一映射成 Goal 阻塞状态。coolzhu 已有的确定性验证和根执行预算应保留，不宜为追求同名功能而退化成仅由模型报完成。[Goal 驱动器][goal-driver]

## coolzhuagent 当前对应情况

| 领域 | 当前源码证据 | 对齐判断 |
|---|---|---|
| Web 提示词 | [main.rs:28283](D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28283) 与 `build_agent_system_prompt_with_beads` 拼接身份、工具规则、memory beads、skill guidance；`AGENT_GUIDE_INLINE` 在同处定义 | 已有有效注入；尚不是 DSH 这种带来源、作用域和生命周期的 section/context registry |
| 核心库提示词 | [prompt.rs:88](D:/coolzhuagent/modules/core-runtime/packages/core-runtime/src/prompt.rs:88) 有 builder 和静态/动态分界；[指令发现](D:/coolzhuagent/modules/core-runtime/packages/core-runtime/src/prompt.rs:242) 加载 CLAW 系列文件 | Web 与核心库构造路径不同；先查接线，避免只修改 builder 就认为产品已对齐 |
| 请求协议 | [types.rs:5](D:/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/types.rs:5) 使用独立 `system: Option<String>`；[OpenAI 兼容转换](D:/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/providers/openai_compat.rs:1050) 前置 system 消息 | 已抽查路径未见 DSH 同等的 `systemPromptUpdate/toolUpdate` 和历史工具声明投影；需明确能力契约 |
| 上下文观测 | [main.rs:28841](D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:28841) 有工具目录 revision、上下文 snapshot、memory revision 和权限标记 | 可复用为请求快照基础，继续补来源、有效范围和回放语义 |
| 执行权限 | [tool_dispatch_service.rs:27](D:/coolzhuagent/modules/gui-web/packages/web-console/src/tool_dispatch_service.rs:27) 先校验父身份、取消、预算和宿主预检，再运行 hook 与工具，并持久化收尾 | 已有实质性约束；应做覆盖和绕过回归，无需新建另一套权限核心 |
| 子 Agent | [host_child_agent.rs:55](D:/coolzhuagent/modules/gui-web/packages/web-console/src/host_child_agent.rs:55) 冻结父模型、工具、权限、工作区；[工具说明](D:/coolzhuagent/modules/gui-web/packages/web-console/src/host_child_agent.rs:218) 明确同步子任务且不能递归委派 | 已有权限继承基础；异步 child ID、状态查询、等待及恢复需作为一组能力设计 |
| Goal 验证 | [main.rs:40296](D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:40296) 经过文件/命令等确定性校验；[Goal 提示](D:/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:46976) 要求产物和证据 | 保留验证门和预算，不把 DSH 的模型完成声明当作更强验收 |
| 文件冲突 | [file_ops.rs:197](D:/coolzhuagent/modules/core-runtime/packages/core-runtime/src/file_ops.rs:197) 已有预期版本和原子发布机制 | 基础已存在；进一步验证模型、插件和 shell 路径各自的约束范围 |
| DSH 插件宿主 | [package.json](D:/coolzhuagent/modules/tooling/packages/dsh-plugin-host/package.json) 仍锁定 DSH `0.1.1-rc.2`；[host.mjs:14](D:/coolzhuagent/modules/tooling/packages/dsh-plugin-host/src/host.mjs:14) 只接受 `tools` 服务依赖 | 当前是有限工具宿主，不等于完整 DSH harness 兼容 |
| DSH 执行接线 | [dsh_execution.rs](D:/coolzhuagent/modules/gui-web/packages/web-console/src/dsh_execution.rs:1) 已处理启用快照、资源身份、权限撤销；dispatch 对 DSH 强制真实父轮身份 | 不应继续沿用旧 README 的“尚未接入”描述；接线与安装版验收仍需分别判断 |
| Web 搜索 | [tool-registry lib.rs:879](D:/coolzhuagent/modules/tooling/packages/tool-registry/src/lib.rs:879) 默认 Bing HTML 搜索后端、明确的超时与可配置回退 | 新增 DeepSeek Search/Fetch provider 即可保留现有工具契约，不必替换整个运行时 |

## 建议的对齐任务与验收条件

下面是实现建议，不是对当前缺陷的全部断言。优先级按提示词正确性、恢复能力和升级风险排序。

| 编号 | 优先级 | 任务 | 最小验收条件 |
|---|---|---|---|
| H01 | P0 | 建立统一 PromptAssembly，记录 section/context 的来源、作用域、版本与必需性；让 Web、Goal、CLI 使用同一契约 | 同名覆盖隔离到单会话；缺变量、多个 complete 和无效工具引用在请求前失败；计划/权限说明不会与真实状态矛盾 |
| H02 | P0 | 引入实际 provider/model 的提示词与工具更新能力；未知能力采用保守重建 | 支持/不支持 in-history 两条路线；新增/删除工具；切换模型；清空提示词；压缩后恢复均无旧规则复活和未授权调用 |
| H03 | P0 | 建立项目指令链及可回放的新增、更新、删除记录，兼容已有 CLAW 文件 | 全局→项目→子目录优先级明确；文件撤销、同内容去重、预算裁剪、cwd 切换、resume 均一致；明确链接文件的读取范围 |
| H04 | P0 | 在现有执行闸门上验证规则来源和生命周期，而非增加一份重复权限状态 | 网页、工具结果、记忆和子代理消息不能升级成人类授权；hook 不能绕过宿主拒绝；取消后迟到结果不触发新动作 |
| H05 | P1 | 定义异步子任务 activation、完成通知和 wait；区分 idle、等待、完成与失败 | 启动回执不算完成；父工具结束不杀子任务；等待不扣 Goal 推进轮次；崩溃后未完成任务不展示成功 |
| H06 | P1 | 将工作目录提升为会话持久状态，驱动指令、文件和进程调用 | 两个会话互不影响；切目录不扩大沙箱授权；旧进程保留原 cwd；恢复时目录失效有明确处理 |
| H07 | P1 | 独立升级 DSH 工具宿主与包兼容检查 | 锁定版本和依赖；验证最新官方工具注册、取消、卸载、超时、schema 和回执；不加载依赖未提供服务的插件 |
| H08 | P1 | 增加 DeepSeek Search/Fetch provider | 默认路由明确；开关下一次操作生效；测试账号/Key、路径前缀、原始问题开关、条数上限、取消、错误和回退策略 |
| H09 | P2 | 按真实用例增加持久 PTC 与 Worktree | 常驻变量不越权；一次提权不污染常驻环境；权限收紧完成清理；工作树与会话 cwd、文件授权一致 |
| H10 | P2 | 完善快照与调试视图 | 能回答每轮用了哪版规则、哪些工具、哪份目录/权限状态；日志不把密钥或临时诊断内容塞入模型指令 |

建议先做 H01–H03 的共同数据契约和回归样本，并把 H04 作为现有执行层的守门测试；H07–H08 可以独立交付。保留 SQLite 作为 coolzhu 权威存储完全可行，采用版本化事件和派生视图即可实现上述语义，无需先改成 Cordis 或 JSONL 存储。

## 升级时需要明确处理的兼容性变化

1. **版本跨度**：coolzhu 宿主从 `0.1.1-rc.2` 跨到 `0.2.1-rc.2`，比本报告重点核对的 `0.2.0-rc.2` 基线更早；还需要补查中间版本迁移，不能把本表当作穷尽清单。
2. **invariant**：运行时 invariant 插件及各包 `./invariant` 导出已删除；coolzhu 依赖里仍列有 `dsh-invariants`，要核对并移除旧接口依赖。
3. **工具模式**：`both` 不再支持；当前 coolzhu 宿主采用 `native`，该项本身无须迁移为 PTC。
4. **子代理组合**：新配置用 preset；旧 `persona`、`toolFilter` 会明确报迁移错误；已保存的历史子代理描述仍有兼容恢复语义。[组合迁移][preset-note]
5. **SDK 完成判断**：补 `session/wait`，不能只等父会话第一次 idle；resume 必须恢复原存储与原 ID，失败不能悄悄创建新会话。[SDK 协议][sdk-protocol]
6. **指令根目录**：移除行级 `dshHome`；从进程 `DSH_HOME` 和 `DSH_AGENTS_HOME` 取值。`{{cwd}}` 不再由循环作为内置 prompt variable 提供，应使用必需的目录 context。[系统提示词契约][prompt]
7. **Agent Team 消息**：改为直接目标 Inbox 投递，删除旧 outbox、自动重试与重发去重；不能保留旧的“返回成功即可靠处理完成”假设。[团队工具][team-tools]
8. **凭据优先级**：Web/桌面优先保存凭据；headless 默认使用环境变量和 `.env`，不读取保存的密钥。自动化和交互式调用需要分别核对。[累计发布说明][release-rc1]
9. **第三方插件**：优先检查 `engines.dsh`；子路径显示元信息通过导出提供；安装、选中启用、加载成功、当前可执行要分别记录。[插件管理][plugin-manager]
10. **实验性能力**：Claude Code Mods、Worktrees、开发者组合包和插件状态记录不能直接当作稳定兼容承诺；完整采纳前逐项验证依赖和边界。[累计发布说明][release-rc1]

[release-rc2]: https://github.com/deepseek-ai/deepseek-harness/releases/tag/dsh-v0.2.1-rc.2
[release-rc1]: https://github.com/deepseek-ai/deepseek-harness/releases/tag/dsh-v0.2.1-rc.1
[compare-rc2]: https://github.com/deepseek-ai/deepseek-harness/compare/dsh-v0.2.1-rc.1...dsh-v0.2.1-rc.2
[model-label]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/client/ui-deepseek-model-label-1017/README.md
[web-api]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/web/web-deepseek-api/README.md
[web-service]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/web/web/src/index.ts
[messages-search]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/web/web-search-deepseek/src/provider.ts
[pi-models]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/models.ts
[prompt-source]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/system-prompt/src/index.ts
[prompt]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/system-prompt/README.md
[runtime-note]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/.agents/notes/implemented/bug-fix/2026-10-11-process-specific-prompt-values-in-runtime-context.md
[runtime-projection]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/agent-loop/src/runtime-context.ts
[instructions]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/context/agent-instructions/README.md
[instruction-config]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/context/agent-instructions/src/config.ts
[instruction-source]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/context/agent-instructions/src/index.ts
[instruction-render]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/context/agent-instructions/src/render.ts
[subagent]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/subagent/tool-subagent/README.md
[preset-note]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/.agents/notes/implemented/simplification/2026-10-09-preset-owned-subagent-composition.md
[goal-driver]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/goal/goal-round-driver/README.md
[goal-tools]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/goal/tool-goal/README.md
[sdk-protocol]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/sdk/protocol/README.md
[sdk-client]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/sdk/client/README.md
[ptc-node]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/ptc-runtime/ptc-runtime-node/README.md
[ptc-python]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/experimental/ptc-runtime-python/README.md
[tools]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/tools/README.md
[tools-source]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/tools/src/index.ts
[compaction]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/compaction/compaction-basic/README.md
[plugin-manager]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/plugin-manager/README.md
[session]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/session/README.md
[ssh]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/ssh/ssh/README.md
[gateway]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/api/gateway/README.md
[agent-loop]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/agent-loop/README.md
[llm]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm/README.md
[persona]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/preset/persona/README.md
[plan]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/plan/plan-mode/README.md
[fs-policy]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/fs/fs-observation-policy/README.md
[web-tools]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/web/tool-web/README.md
[ptc-sandbox]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/.agents/notes/implemented/architecture/2026-10-08-persistent-ptc-sandbox.md
[team-tools]: https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/experimental/tool-agent-team/README.md
