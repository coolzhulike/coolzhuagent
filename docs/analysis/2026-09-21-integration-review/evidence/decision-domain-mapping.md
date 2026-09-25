## 12项架构决策与23个领域的实施映射（独立建议稿）

本节根据01/02原文、本轮源码审计及 `implementation-contracts.md` 编制，供最终04计划合并。不是GPT6 Pro结论转述，也不表示用户已批准架构改动。现状指本次本地源码与0.2.14报告；“已存在”不等于所有路径已实测。

阶段约定：**P0基线**；**P1 Computer Use专项**；**P2共享运行契约**；**P3持久化接线**；**P4受控扩展**；**P5体验与发行**。P1先在现有边界内修正可复现问题，不等待整体迁移；其稳定契约在P2/P3接入共享执行和事件存储。持续集成、回归与可回滚交付贯穿各阶段。

模块简称均指 `C:/Users/zhupu/Desktop/coolzhuagent` 下源码：web=`modules/gui-web/packages/web-console`，core=`modules/core-runtime/packages/core-runtime`，adapter=`modules/llm-adapter/packages/llm-adapter`，tools=`modules/tooling/packages/tool-registry`，plugins=`modules/tooling/packages/plugin-system`，CU=`modules/computer-use/packages/computer-use-core`，UIA=`modules/vision/packages/uia-resolver`。新接口/字段均为拟议设计，实际拆分包名和数据库版本待实施时确定。

### 12项决策：权威归属与不可混淆边界

| 决策 | 建议结论与权威 | 边界、阶段及必要产物 |
|---|---|---|
| D01 权威状态 | 保留SQLite单权威、单写者；同事务写状态变化与事件/outbox，再广播 | P2定义每字段由状态表还是事件推导拥有，P3落库；JSONL只作审计/导出，不能双向覆盖。旧历史为有来源基线，不编造事件与usage。 |
| D02 扩展宿主 | 保留Rust核心与现有进程外工具；借鉴注册/撤销生命周期 | P4第三方代码不默认进核心进程；UI扩展和工具扩展分别授权。DSH可选适配必须版本握手、可取消和可卸载，不导入全部291包。 |
| D03 前端 | 保留Tauri壳，原生JS逐步模块化、面板注册及独立资源构建 | P5先保留现交互/视觉再拆分；React只在独立试点有收益后决策，不作为共享运行时前提。玉石icon、竹林、对比度为产品约束；整体美化仍需审阅。 |
| D04 权限 | 一个执行期解析器输出resolved权限；暴露、审批、OS隔离分别建模 | P2映射旧5档/3档/房间权限，保留显式用户配置与debug默认完全访问、release默认显式授权。现FullAccess先于Protected，不偷偷改变。plan是协作状态。 |
| D05 进程沙箱 | 保留现有输入身份/取消边界；OS进程隔离作为独立能力 | P4先验证Windows限定场景，不能称已具完整沙箱。隔离不可用应显式失败；VM不是containment，不能无声回退全权。CU桌面租约不能替代文件/网络沙箱。 |
| D06 配置/凭据 | typed `coolzhu.toml`拥有业务配置，包清单拥有发行清单；统一resolve流程 | P2/P3保留模型ID、Base URL、key来源与空输入保留语义。凭据值不进事件/前端回显；不新增业务环境变量。Agnes按默认视觉会话引用解析，不硬编码名字。 |
| D07 版本迁移 | schema、事件格式、CLI会话格式各自有版本和兼容映射 | P3借鉴相邻迁移清单/前向版本拒绝；不强求版本数字相同。迁移先副本，回退需匹配数据快照，不能仅换exe。 |
| D08 编排 | 保留Goal DAG、角色隔离、阶段验收与结构化交接 | P2统一owner/取消/预算，P4再试脚本workflow；DAG与脚本不是二选一。父子会话/并发深度受总预算，实验Teams不默认启用。 |
| D09 记忆 | 保留beads及语义检索，记忆与会话事件分域、写入来源明确 | P3共用历史投影，保留手工记忆；不自动召回旧原始思考/不明审计。压缩结果与长期事实分开标记，可删除/重建边界明确。 |
| D10 MCP | 保留兼容入口与认证，统一工具身份和生命周期 | P4两侧前缀本已相同，核对规范化/长度/碰撞/alias；transport逐项验收，不因enum存在就宣称全可用。OAuth存储与刷新依凭据契约。 |
| D11 工具模型面 | 保留显式暴露策略；一次真实调用一个稳定ID、一个执行所有者 | P2静态19个spec不等于模型工具总数；终态/历史/usage投影分别处理。PTC至P4可选试点，内部每调用仍过统一闸门，不作通用逃生口。 |
| D12 自动化入口 | web、CLI、子Agent共享turn契约，传输适配不拥有业务真值 | P2先统一现入口，P4评估typed SDK/ACP。必须定义取消、逐请求结果、审批和错误映射；不把现内存agent-server stub直接发布为SDK。 |

### 23个领域：保留、改造、借鉴与延后

| 领域 | 当前可确认状态 | 处置与阶段 | 模块落点 | 验收门禁 |
|---|---|---|---|---|
| 01 启动与组合 | launcher启动web/Tauri，有health与归属判定 | 保留；P0锁基线，P5完善生命周期 | `packages/app-launcher`、包清单、Tauri | 端口占用/旧进程/依赖缺失可解释；退出无孤儿进程；原生窗口实测不能由后台health替代 |
| 02 插件机制 | 进程外manifest；部分内置插件未接线，市场安装桩 | 改造/借鉴P4；未接线逐项保留或退役 | plugins、tools、web插件API | 注册/重名/卸载/取消与权限真实生效；installed=true必须有实际产物证据 |
| 03 主循环 | web与CLI/子Agent两套实现；CLI非增量流 | 改造P2，按已发布web行为抽取 | web工具循环、core会话、CLI、tools子Agent | 同fixture覆盖stream/nonstream/CLI/子Agent；一个turn仅一个副作用写者；迟到结果不复活终态 |
| 04 事件与传输 | SSE、事件表及轮询并存；类型手写 | 借鉴P2、接线P3 | core契约、web路由/SSE、前端消费 | `(stream_id,seq)`续读/去重/缺口补读；提交后广播；断线不重跑动作 |
| 05 工具系统 | 静态19spec，动态模型工具面按策略生成 | 改造P2；PTC延后P4 | tools、web执行闸门/历史投影 | 分片不进思考；一次调用一次计数；动态未知工具权限不降为宽松默认；发送/观察/验收分维 |
| 06 模型适配 | 统一会话参数、图片直传/视觉中转已存在 | 保留/改造P2 | adapter、web参数与multimodal_input | 保存→resolved→wire矩阵；未登记模型可配置；Qwen字段互斥、取消/超时、图片路由和已知usage正确 |
| 07 设置配置 | 生效TOML与旧配置入口并存，进程内有锁 | 改造P2/P3 | core config、web WorkspaceConfig、model_settings | 显式覆盖优先；保存冲突不丢更新；原子落盘/多进程竞争；不重写用户Base URL |
| 08 凭据 | web字面值/文件引用、适配器env；OAuth明文 | 改造P3，借鉴独立凭据接口 | adapter、web密钥解析、core OAuth | 来源优先级明确；留空/清空不同；前端、日志、导出不泄漏；刷新竞争与失败恢复 |
| 09 权限审批 | 多套档位；无统一OS沙箱，已有CU敏感/身份控制 | 改造P2，沙箱试点P4 | core权限、web执行、CU控制器 | 权限映射矩阵；Protected/FullAccess次序；每入口同判定；未知工具和隔离失败不能绕过 |
| 10 存储迁移 | SQLite权威、CLI JSON；有run/Goal启动恢复 | 保留/改造P3 | web存储、core格式、CU store | 崩溃注入和幂等恢复；单写者；前向版本拒绝；不确定副作用不重放；备份可恢复 |
| 11 上下文压缩 | 有预算、v2压缩bead与污染过滤；自动目录指令注入未见 | 保留/借鉴P2/P3 | web上下文、chat_tool_history、core | 模型可见内容可追溯；预算/溢出边界；AGENTS作用域/信任/编码；旧原始思考不重新注入 |
| 12 长期记忆 | beads、取代/衰减、hash及可选真实embedding | 保留、解耦P3 | core memory/semantic、web记忆表 | 手工记忆保留；成功证据来源可查；按会话/任务隔离；压缩不伪装事实；向量模型变更可处理 |
| 13 文件附件大输出 | 附件表/目录、预览；缺通用spill与读取新鲜度契约 | 借鉴P3 | tools文件操作、web附件/预览、core文件契约 | read-before-edit冲突不覆盖；大输出全文可取且有界预览；图片变体与原图来源；引用GC不误删 |
| 14 编排定时触发 | Goal DAG、交接、日/周调度；无通用jobs/webhook；orchestrator未接线 | 保留/改造P2/P3；扩展P4 | web Goal/调度、core共享owner | 依赖/重试/人工验收/取消；全局并发与深度；重启对账；调度权限不意外提升；新webhook需幂等 |
| 15 Skills | CLI/web两套发现，Goal baseline注入；引用文本为主 | 改造/借鉴P2/P4 | command-router、web skill发现、skills | 去重/优先级一致；大小/资源路径有界；用户/模型调用控制；加载文本不暗示已执行脚本 |
| 16 MCP | 多transport/认证类型与工具注册存在 | 保留/改造P4 | core mcp/stdio/oauth、tools | 连接/重连/退出/超时；名称碰撞；资源权限；令牌刷新；实测与仅类型声明分开 |
| 17 UI宿主页面 | 快捷轨+右扩展、顶部下拉、玉石竹林已存在；无统一slot | 保留/改造P5 | web JS/CSS/HTML、Tauri、pet | 低高度/缩放/键盘/对比度；下拉切环境不打开错误面板；移除无效icon；视觉稿先审阅 |
| 18 诊断统计轨迹 | token/耗时/搜索已有，完整轨迹/日志UI缺；stream自检桩 | 改造P2/P3/P5 | chat_insights、CU store、diagnostics、前端 | 已知token与unknown分开；请求用途/TTFT按实证；轨迹可重建；日志限量脱敏；自检不能硬报成功 |
| 19 设备与连接器 | CU/UIA/视觉/浏览器桥、ClawBot、语音存在；Paint未通过 | CU优先P1，其余保留/分批改造；SSH/Office/LSP试点P4 | CU/UIA/vision、desktop/browser桥、ClawBot/audio | 独占桌面、坐标变换、真实落笔、取消释放、stale重观察；其余功能各自验收，不由Paint或注册接口推断 |
| 20 CLI/SDK入口 | CLI可用但异步行为不同；agent-server未接线 | 改造P2，SDK/ACP试点P4 | CLI、共享turn服务、API适配 | 多入口同语义；流增量/取消/逐请求结果；SDK不得暴露未授权原始审计；兼容旧CLI参数 |
| 21 测试门禁 | 已有单测/模拟HTTP/前端契约；无统一录制回放及CI链 | 借鉴P0起贯穿 | tests、脚本、CI、脱敏fixtures | 源码/环境/命令/结果可追溯；录制→无key重放；故障矩阵；新版本重跑受影响项，不借1175旧结果背书 |
| 22 打包分发 | A已有MSI；B有NSIS/macOS工程，非无安装器 | 保留/改造P0/P5 | package-manifest、package/build-msi/safety | commit/MSI/二进制hash一致；安装前后用户数据核验；真实窗口+前端；签名状态如实；迁移回退匹配数据 |
| 23 文档自述 | 多份规则/计划与实现有漂移，现已补审计 | 改造P0贯穿 | docs、INTERFACE、发布报告、许可清单 | 每功能区分代码/接线/实测/发布；变更更新契约；过期计划不当事实；来源与依赖许可可审计 |

### 防遗漏与先后约束

原文23领域均有去向，但不表示全部应马上开发。以下资产必须进入回归清单：Goal阶段人工验收、会话接力/结构化交接、记忆/embedding、桌宠拖放与事件、ClawBot群成员权限与outbox、实时语音、浏览器扩展身份与owned tabs、CLI技能发现。它们不能因聚焦聊天/CU而被静默移除。

以下是明确延后项，不能在总计划中冒充已吸收：完整插件市场、实验Teams、全量DSH UI迁移、通用PTC、跨平台桌面、SSH、完整Office工作流、全量国际化与遥测外发。LSP已有未接线资产，须先做生产接线试点再决定复用；无需求的遥测不因DSH具备就开启。

P1不得靠放松窗口身份、跨进程独占、取消释放或未知副作用不重放来改善成功率。P2抽共享循环前先固定P1行为契约；P3不能同时写两套真值；P4不能替换生产主循环产生第二执行者；P5的图形美化和功能迁移应分批交付，每批保留可核对结果与回退条件。所有阶段失败都只回退可替换策略，不关闭诚实终态、权限与输入保护边界。
