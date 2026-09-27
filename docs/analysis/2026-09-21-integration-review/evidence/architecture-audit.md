# 架构差异原文复核记录

审查日期：2026-09-21。只读审查，未修改产品代码、用户运行数据或桌面原文。对象为 `C:/Users/zhupu/Desktop/01-coolzhu-dsh-架构差异与整合决策.md`，已分段通读全部 962 行；逐项证据校验聚焦会改变执行方向的结论，不代表执行了两个产品的全部能力测试。

## 审查口径与来源

下文 A 根目录为 `C:/Users/zhupu/Desktop/coolzhuagent`；B 根目录为 `C:/Users/zhupu/Desktop/dsh`。路径 `A/...:行号`、`B/...:行号` 指本次读取的本地文件。原文行号均为修改前版本。

B 目录真实存在，可读源码及文档，但不是 Git 仓库，无法从本地确认提交、远程来源或是否最新。`B/package.json:2-9` 声明版本 `0.1.6-alpha.2`、license `MIT`、Node `^22.19.0 || >=24.0.0`、pnpm `11.7.0`。`B/LICENSE:1-12` 为 MIT 文本，包含 DeepSeek 2026 版权及保留版权/许可文字的条件。这仅是本地许可声明事实，不能替代所有依赖、资产、外部 Cua Driver 的独立许可/版本审查。

关键文件 SHA-256：

| 文件 | SHA-256 |
|---|---|
| A `modules/gui-web/packages/web-console/src/main.rs` | `0a2d2a41c1d24113ae83a27842d25e3b20bf1298b4492a5ee67a7ac6aedcdf15` |
| A `modules/llm-adapter/packages/llm-adapter/src/request_parameters.rs` | `9cc27ce4e95e377f90bc318a8e50415dd206eef1ed282588d45b9f04fdda7cba` |
| B `package.json` | `a9cc164c4d922d74571e09c04be4e0f85a318685d410bb1c56072258a19a6f1d` |
| B `pnpm-lock.yaml` | `72523ca50bd34e3e21d07c82e1cedfdb91b17e1bcd83006c645e8dc6e0786fe0` |
| B `packages/session/session-persistence-jsonl/src/lease.ts` | `377f24b1446382d703d5f1346f44e15a25368a0e7967e5bd41db700a919d7ad8` |
| B `apps/desktop/scripts/electron-builder-config.mjs` | `b0b56ad40de62b7ee85837047a131aadf3c05379756d3f869717f5e31a1bc4a0` |

## 需要优先修正的事实

| 编号/影响 | 原文位置与问题 | 核实结果与证据 | 对方案的影响 |
|---|---|---|---|
| AR-01 高 | 125 行 D3.5 把 A 崩溃恢复写成“无（SQLite事务）” | A `main.rs:796` 启动调用 `recover_incomplete_runtime_runs`；`:41804-41824` 明确一笔 Immediate 事务处理 runtime orphan、Goal claim、Goal状态与事件，提交后广播；`:68787` 有幂等恢复测试。 | 改为“已有 run/Goal 启动恢复，缺工具副作用结果的完备重建契约”。不应重做为全新零基础恢复模块，更不能恢复时自动重放有副作用动作。 |
| AR-02 高 | 196/214 行将请求参数概括为 3 字段，max_tokens仅静态表 | A `request_parameters.rs:6-12` 还有 reasoning_mode，`:15-39` 有Qwen3.8校验，`:42-114` 显式原生编码；A `main.rs:5154-5183` 会话参数含协议/endpoint/预算/图片/工具等；`:27719-27733` 显式会话输出覆盖优先。 | 统一配置页已上线，下一阶段是归一解析责任、能力校验和迁移，并非再次建设同一页面。Provider枚举仍存在，但手工custom连接无需给每个模型新增厂商枚举。 |
| AR-03 高 | 250/266 行“API key全从env”；B“只存引用” | A `main.rs:29913-29941` 支持会话内字面值或文件引用，前端 `model_settings.js:186` 可保存api_key_ref且空值不覆盖；B credentials-local `src/index.ts:268-284,443-463` 将refs映射值和records写入YAML。 | 两侧都有本地凭据值，不能把B称作仅引用、不含密钥；迁移要保留实际来源/不回显/清空语义，不能假定env是唯一真源。 |
| AR-04 高 | 259 行 B 凭据优先级漏 provider-managed store | B `packages/credentials/credentials-local/src/index.ts:2-24`：继承环境 > `.credentials.yaml` > 调用目录.env > home.env；设置页保存值高于.env。 | 统一配置时必须显式显示来源及只读覆盖，避免UI保存成功却被旧.env覆盖。 |
| AR-05 高 | 328/345/951 行把“Windows无lock文件”推为缺跨进程租约 | B `packages/session/session-persistence-jsonl/src/lease.ts:1-12,36,78-85` 使用Win32命名内核信号量；`win32.ts:151-160` 调CreateSemaphoreW/WaitForSingleObject；无文件不等于无锁。 | 删除“为DSH补Windows租约”的虚假需求。跨进程竞争/进程死亡恢复仍应作为本机验证项，存在实现不等于实测通过。 |
| AR-06 高 | 303 行“Protected规则优先于profile” | A `core-runtime/src/permission_gate.rs:241-268` FullAccess先短路；Protected仅优先于普通workspace自动放行。 | 不能在权限映射中把现有FullAccess描述成仍会被Protected阻断。若计划改变该语义，必须显式列为行为变更。 |
| AR-07 中 | 300/827 行只写A默认WorkspaceAuto | A `main.rs:6332-6334` debug默认dev_open_permissions=true，release默认false；已有显式配置优先。 | 开发测试与正式默认须分别定义，保留用户已授权“调试默认完全访问”，不把debug行为误带入发布默认。 |
| AR-08 中 | 77 行正确区分PluginPermission，但818/786行扩大为requiredPermission完全无强制 | A `tool-registry/src/lib.rs:208-229` 将插件required_permission纳入permission_specs；CLI `main.rs:4242,4259`消费。A web `main.rs:2141-2146` 只查MVP且未知工具默认ReadOnly，路径需要专项核查。 | 应分清manifest权限标签、每工具最低权限、OS隔离三层；不能全称“从不生效”。优先统一所有入口的权限解析，检查未知/动态工具最低权限回退。 |
| AR-09 高 | 529/789/856 行MCP命名规则不同 | A `core-runtime/src/mcp.rs:26-35` 本就生成`mcp__<server>__<tool>`。 | 决策改为server/rawName规范化、长度、碰撞和兼容alias；不是选择两个不同前缀体系。 |
| AR-10 高 | 664 行“DPI/多显示器不处理” | A `uia-resolver/src/windows_impl.rs:27-52` 有线程PMv2及Drop恢复，`:413-442`测试；0.2.14报告已记录150%缩放失配修复。主屏捕获/多屏拓扑缺口仍独立存在。 | 不得推倒已修DPI；下一阶段测100/125/150/200%、跨屏/窗口移动，分别定义坐标空间和图像变换。 |
| AR-11 中 | 545 行把旧DOM三栏当当前可见布局 | A `chat_experience.css:2-4`隐藏左聊天导航/左resizer；`:6-14`顶部横排；0.2.14报告确认快捷轨+右扩展、顶部下拉。 | 布局目标已经部分实现，前端拆分需保持玉石色控件、竹林图层、对比度和下拉行为，不能依据旧DOM恢复三栏。 |
| AR-12 高 | 737 行 B“无MSI/无安装包/无WiX” | B `apps/desktop/package.json:10,22-34` 有installer测试及win/mac打包；`scripts/electron-builder-config.mjs:130,184,190`配置dmg/zip、NSIS、安装配置。 | 可说未见MSI/WiX，不能说没有安装包。是否采用Electron与是否有分发工程是两件事；保持现有MSI可行。 |
| AR-13 中 | 558/932 行空Host apply标成stub缺口 | B `client/ui-renderer/src/index.ts:1-4`明确browser-only，无host行为是设计；原文自身也指出真正registry在client。 | 把它归为职责分层的空宿主入口，不应排进“补完功能”工单。 |
| AR-14 中 | 243 行配置并发“未见写锁” | A `main.rs:6675` mutate_workspace_config；`:6692` 全局Mutex<WorkspaceConfig>。 | 已有进程内序列化，尚需核实原子落盘和多进程写者锁；不能声称没有任何锁。 |
| AR-15 高 | 810-813行权威介质与事件溯源语义绑定，双权威热冷列为平级选项 | B `core/agent-loop/src/invariant.ts:18-58`检查请求与durable推导/请求头一致，无JSONL物理格式要求；A已SQLite事件表和事务恢复。 | 可先在SQLite事务中保留一个权威事件/状态写点与outbox，逻辑事件可追加且可重放；JSONL可做导出/审计副本。禁止无冲突规则的SQLite+日志双写双权威。 |
| AR-16 中 | 690 行称CLI是ConversationRuntime唯一生产端 | A `tool-registry/src/lib.rs:2199-2208` 也构造子Agent runtime；原文106行本身已写两端。 | 要把CLI、子Agent和web全部列入统一回归矩阵，不能只迁移两个表面入口。 |
| AR-17 中 | 699/701行静态#[test]数量与报告实际执行数混为完成度 | A `docs/testing/release-0.2.14-agent-fixes-report.md` 总结1175 pass、0fail、1ignored，web988、adapter119，含集成/脚本等。源码计数不是运行测试总数。 | 记录测试命令、基线、skip原因与实际报告；不能把代码计数当本次全部重新跑过，更不能据此宣称Paint成功。 |
| AR-18 中 | 628/888行全称“无输入安全控制” | A computer-use `controller.rs:92-109,201,541-569` 有host敏感语义分类、风险审批；正式笔画有窗口/坐标/取消/释放约束；但无通用输入内容治理或OS沙箱。 | 改为精确能力范围，避免用字符串grep结果证明安全控制完全不存在。 |

此外：原文标题承诺“不做优劣排序”，但“更严格”“更独立完整”“产品化更好”“后续90%”等是评价或无分母推断。建议去掉比例、改成具体机制与验收边界。`[完整]`宜拆为“代码存在 / 产品接线 / 本机验证 / 发布验证”四列；实验性provider不能因基础包可注册即标端到端完整。

## 原文可保留的核心认识

1. A产品web主循环与CLI/子Agent runtime确实分离；B主循环/服务容器契约值得借鉴。不能直接切到现有CLI runtime，因为其流接口返回Vec而非真正增量，现有web才是已发布行为基准。
2. A原生JS/CSS与业务巨型main.rs存在维护压力；B keyed slot/类型契约/模块生命周期可作为结构参考。React/Vite不是必须一轮采纳的前提，先按边界拆模块/注册面板可以渐进完成。
3. A长期记忆、Goal DAG、UIA/原生输入/浏览器扩展、桌宠是保留资产。B已安装同进程插件不是安全容器，实验Cua Driver provider不是桌面绘画能力已验证。
4. A自动AGENTS.md注入在本次源代码检索中未见；有内联AGENT.md规则字符串，这不是目录级指令发现。补实现要明确信任级、目录作用域、编码/大小/符号链接和取消，不只是把全文塞进system。
5. B可借鉴的可执行契约包括迁移相邻步清单、请求重建不变量、frozen request、单调用身份/生命周期、录制重放、provider/slot注册生命周期。需给A编写自己的适配与回归，不能把291包直接当替换库集。

## 推荐整合路线及门禁

推荐保留Rust/Tauri/现有Windows MSI与SQLite权威，从当前web产品行为提取共享服务。DSH先借鉴契约；需要复用其运行模块时采用明确、可停止的进程适配试点，避免并行两个不互知的Agent主循环。没有代码证据要求先改用Electron、JSONL物理日志或整包导入DSH。

| 阶段 | 交付与依赖 | 完成门禁 | 失败/回退 |
|---|---|---|---|
| S0 基线与来源 | 锁A源码/安装包/用户数据快照，锁B版本+源码/lockfile哈希及来源；更正两份原文并生成状态矩阵 | 文档断言能定位到代码/实测；许可证/依赖清单可审计；公开材料无密钥 | 不引入未锁依赖，不改用户配置 |
| S1 运行契约 | 定义workspace/room/session/turn/step/request/tool/action ID、终态first-wins、取消、错误/timeout、usage来源；新增脱敏fixture | stream/nonstream/CLI/subAgent共享不变量测试；不能把input_sent当目标success，未知副作用不自动重放 | 保留现web入口，新模块可切回原投影 |
| S2 状态与恢复 | 在SQLite权威事务内追加事件/outbox；广播后投影有序重放，幂等客户端；版本相邻迁移、forward-version拒绝 | 故障注入：commit前/后、广播前/后崩溃；无丢事件/重复副作用；旧库可迁移且可备份恢复 | 暂停新增写者，恢复备份；JSONL只有导出/审计角色 |
| S3 主循环抽离 | 先提取请求解析、权限、工具调度、历史投影，再提取turn runner；HTTP/SSE/CLI成为适配层 | 同fixture在流/非流/CLI/子Agent表现一致；原网页9项和工具9项回归；不会丢Qwen/图片/用量能力 | 分模块开关/接口兼容；不一次搬动87k行 |
| S4 配置/凭据 | resolve(request):Spec、协议能力表、运行配置与包清单分离；保留已存key/base_url和空值语义 | saved→resolved→wire矩阵；未知模型手工配置可用；多进程冲突不丢更新；图片原图/Agnes转述不串路 | 保留兼容读，迁移不静默重写endpoint或key |
| S5 CU专项 | 由独立CU差异/失败矩阵细化；原生动作、视觉反馈、任务验收分别计分 | 受控画布可验证落笔；Paint真实任务证据，stale重观察而非放宽身份检查；成本/延迟/预算有实测 | 有界重规划，保持硬取消与释放，未通过如实blocked |
| S6 UI及扩展 | 面板注册与资源独立构建，保留既有视觉和顶部下拉；轨迹/搜索投影、按需详细审计 | 低高度/缩放/键盘/对比度、消息顺序、迟到事件、跨房间测试；美化方案用户审阅后实施 | 老面板适配器可回退；不整轮复制55个UI包 |
| S7 可选DSH适配 | 仅在上述边界稳定后试点一种provider/工具或PTC；显式版本握手、取消/错误映射、调用授权 | 单写者/owner隔离、无孤儿进程、依赖/许可证/安装产物审核、失败可卸载 | 禁用适配恢复Rust执行；实验provider不能默认发布 |

## 提交给GPT6 Pro应明确追问的架构问题

- 是否赞成“SQLite单权威+事务outbox+可重放逻辑事件”，并指出哪些产品用例确实需要换物理日志？请区分必要架构变化与偏好。
- 共享主循环的最小切口、模块所有权、取消/审批/使用量怎样从巨型main.rs迁出，又保持当前多模态、Goal和接力行为？给依赖顺序与回滚条件。
- Paint已有原生输入与截图，仍出现工具栏误选、画布未识别、工具状态误假设、stale拒绝、20s规划超时、外层总结不可靠。哪些可用Agent侧通用能力改进，哪些必须标模型限制？哪些方案只是在放宽错误成功标准？
- 先采用哪些DSH机制可在1-2个小范围迭代内产生收益？列出明确不采纳项，不能默认迁入全部包、同进程第三方插件或alpha provider。

## 关于Paint与完成度的硬边界

以A `docs/testing/release-0.2.14-agent-fixes-report.md` 的R1-R6审计为依据：已有DPI/动作schema/原图/drag/点击flags/空总结等Agent修复；R4/R5有实际拖拽但白画布仍空，R6只有1次实际点击及两次stale拒绝；海绵宝宝端到端未通过。1175项自动化通过不覆盖这个失败目标。下一版方案可以重新研究通用Agent改进，不能把前次“暂不优化模型绘画能力”解释成所有剩余问题已被证明纯模型问题，也不能反向把所有剩余失败都断言Agent缺陷。

原生桌面窗口的安装后启动曾被自动审批阻断；后台/无窗口验证成功。它是前次验证限制，不是本次架构审查新增的产品缺陷或本轮再次复现。
