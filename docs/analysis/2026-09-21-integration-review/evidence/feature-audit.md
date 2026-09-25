# 功能全量清单证据审查

审查日期：2026-09-21。输入：`C:/Users/zhupu/Desktop/02-coolzhu-dsh-功能全量清单.md`（667 行），并交叉核对本地 coolzhuagent、dsh、0.2.14 发布报告。这里只读取源码和既有证据，没有启动产品、请求云模型或执行桌面输入。

## 审查结论与证据等级

清单对包名的覆盖较好，适合作为代码资产目录；不能直接把 `[完整]` 汇总为产品完成率。A 侧遗漏了已经接线的关键产品能力，B 侧有把接口包、实验 provider 和默认产品能力混算的问题。应把每行扩展为“接口存在 / 生产接线 / 测试证据 / 已知边界 / 整合决策”五列。

本次通读了全部 667 行，并将 B 侧所有 291 个 `packages/*/*/package.json` 与清单逐名核对；只发现 `util/time` 的规范包名 `util-time` 使用了目录短名 `time`，并非遗漏了整个包。对架构决策影响最大的断言进行了下述源码抽查。没有对 291 个包逐文件穷尽审计，没有执行 dsh 测试，因此不能替原清单全部 `[完整]` 背书。

证据优先级：生产调用与执行结果 > 定向测试 > 契约及实现源码 > README/注释 > 名称或目录存在。纯静态审查可以确认接口与调用点，不能推出模型端到端成功率。

## 必须修正的事实

| 编号 | 原断言与问题 | 修订结论 | 证据 |
|---|---|---|---|
| F01 | B 侧“291 个 workspace 包”；后面又把 apps/native/vendor 列为非 workspace | 291 是 `packages/*/*` 包层数量。另有 22 个 workspace manifest：vendor 9、native/system 及其平台/入口 6、apps 4、benchmarks 1、website 1、python/sdk-runtime 1，总计 313 个匹配成员，不含仓库根包。保留“B 侧包层清单 291 包”这个准确分母 | `C:/Users/zhupu/Desktop/dsh/pnpm-workspace.yaml:1` 到各 glob；实际 manifest 枚举 |
| F02 | experimental 标 17，vendor 标 6 | 实际分别为 16 与 9。experimental 清单的最后一条是说明，不是第 17 个包 | `C:/Users/zhupu/Desktop/dsh/packages/experimental/README.md:25`，manifest 枚举；vendor 为 cordis、cosmokit、group、hmr、include、loader、logger-console、schemastery、timer |
| F03 | `mvp_tool_specs()` 有 20 项、模型可见工具 20 项 | 注册表静态 spec 实际 19 项。Web 模型工具面还附加 `semantic_dispatch`、`computer_use_perform`、`chat_handoff`，并明确过滤 ToolSearch；会话、房间、权限、暴露模式、allowlist 会改变最终请求工具数，不能用静态 spec 数代表模型可见工具数 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/tooling/packages/tool-registry/src/lib.rs:278`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:32419`、`:32580`、`:32605`、`:32627` |
| F04 | A5“不处理 DPI/多显示器，截图仅主屏” | DPI 结论过期。UIA 同步观察已使用可恢复 PMv2 线程上下文；正式桌面桥按绑定窗口采集原图并以物理像素 rect/DPI/身份校验。多显示器、负坐标、跨屏与混合缩放仍需要专项实测，不能反向宣称全支持 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/vision/packages/uia-resolver/src/windows_impl.rs:26`、`:52`、`:369`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:75`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/input_stroke_native.cs:57`、`:70` |
| F05 | A5“无输入内容安全控制” | 绝对断言错误：宿主收集目标/参数/可见节点文本，识别删除、支付、发送、凭据等敏感语义；文本输入限制 Edit/Document 和 4000 字节；另有动作能力、审批、限额。不能据此宣称具备完整语义安全方案，规则误报/漏报、密码框等仍待专项审查 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/controller.rs:92`、`:201`、`:276`、`:541`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:162` |
| F06 | windows-process-guard 是唯一 unsafe 容器 | 错误。uia-resolver 明确允许 unsafe，直接调用 COM/UIA/Win32，并不只依赖该进程容器。应把不同 native 边界分别审查，不以全 workspace 禁止 unsafe 掩盖例外 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/vision/packages/uia-resolver/Cargo.toml:14`、`:25`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/vision/packages/uia-resolver/src/windows_impl.rs:34`、`:47` |
| F07 | A5 标“完整 + 死代码”且列底层原语，没有区分正式控制通道 | 改为“基础执行已接线，端到端部分验证”。正式控制通道已有结构化规划、图片观察、受控拖拽、取消、证据/状态/用量；Paint 的目标仍未通过，不得因核心单测通过而标产品完整 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_planner.rs:41`、`:524`、`:564`；`C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14-agent-fixes-report.md` |
| F08 | A6 文件面遗漏正在承载产品逻辑的拆出模块 | 补充 chat_tool_history、tool_loop_coordinator、multimodal_input、computer_use_planner/executor/adapters/desktop_bridge/store，以及对应测试。巨型 main 规模原值正确，但“只有巨型单文件”不能抹掉这些已经提取的边界 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:4`；`:9`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/chat_tool_history.rs:18`、`:75` |
| F09 | A 侧 SQLite “表清单”遗漏运行审计实体 | 至少补 metadata、chat_room_diagnostics、computer_use_runs、computer_use_steps、computer_use_step_details、computer_use_planner_diagnostics；迁移中 `goal_phases_v6` 等临时替换表不应算当前业务表 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:39653`、`:39844`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_store.rs:13`、`:45`、`:67`、`:72` |
| F10 | B client-ui-renderer 因 Host apply 空而标 stub | 错误归类。Host 入口注释明确是仅浏览器 renderer；Client 实现安装 slot renderer 并提供 React mount。应记录“Host 无行为是设计”，而非功能未实现 | `C:/Users/zhupu/Desktop/dsh/packages/client/ui-renderer/src/index.ts:1`；`C:/Users/zhupu/Desktop/dsh/packages/client/ui-renderer/src/client/index.ts:92` |
| F11 | B JSONL 的“Windows 无 lock 文件”易被解释成无跨进程锁 | Windows 使用由路径派生的命名内核信号量；无文件足迹不等于无锁。仍需注意名称按登录会话隔离，不能直接保证跨用户/跨登录会话排他 | `C:/Users/zhupu/Desktop/dsh/packages/session/session-persistence-jsonl/src/lease.ts:1`、`:80`；`C:/Users/zhupu/Desktop/dsh/packages/session/session-persistence-jsonl/README.md:160` |
| F12 | A2“新增 provider 需改约10处”混同了新增模型/连接 | 新增 ProviderKind/原生适配分支确实涉及 enum 分发，但已有 `from_session_endpoint` 支持模型未登记时按显式协议构造连接；用户正常换模型 ID、Endpoint 不需要新增 provider。清单必须保留这个已实现能力，不能把统一配置再当待从 dsh 新建 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/client.rs:39`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/request_parameters.rs:1` |
| F13 | 把临时 PR checkout 当污染建议清理 | 这是本轮发布来源及历史验证资产，且包含共享 target 的 junction。应改为扫描排除；真正清理前独立检查路径/链接/未提交数据，不把删除作为架构整合前置 | `C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14-agent-fixes-report.md` 的构建源码与发布追溯；本轮上下文已确认共享构建链接 |

## 原判断可保留但需限定

- A workspace 28 成员、main.rs 87025 行/3470755 字节、app.js 20416 行、styles.css 17809 行，当前文件实测相符；精确行数用包含空行的 splitlines，不能用 PowerShell Measure-Object -Line（会忽略空行）替换。
- Web 主路径没有调用 ConversationRuntime；CLI 的 ApiClient::stream 一次返回 Vec，的确不是逐个异步事件接口。证据：`C:/Users/zhupu/Desktop/coolzhuagent/modules/core-runtime/packages/core-runtime/src/conversation.rs:38`。这说明存在双运行时，不说明应立即把 Web 换成当前 CLI runtime。
- LspManager::new 的当前生产扫描无命中，仅 language-service/src/lib.rs:197、:258 测试构造；可保留“未接线”结论，整合应增加真实生产调用与生命周期测试。
- 插件远程安装尚未完成，甚至当前 install handler 仅检查非空 ID 就返回 installed=true，不能把响应字段当真实安装成功证据。证据：`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:18942`。
- diagnostics/stream 返回静态能力宣告，不能作为真正故障注入测试。证据：`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:18503`。
- dsh computer-use 基础包是独占 provider 注册服务，不实现动作；真实桌面 provider 位于 experimental。证据：`C:/Users/zhupu/Desktop/dsh/packages/computer-use/computer-use/src/index.ts:16`。其“完整”只适用于注册接口，不能推广为默认发行版桌面能力已可用。
- dsh “webhook 完整”“SDK 完整”“LSP 完整”仅是所列契约的实现完备，原文已列无队列/无取消/仅四操作等边界。应分别定义验收范围，不能与用户期望的完整产品功能同义。

## 缺失的产品能力维度与重复计数

| 维度 | A 已有事实 | 原清单应补的边界与整合要求 |
|---|---|---|
| 模型配置、图片路由 | 统一参数页、原图直传/默认视觉转述、思考参数等已在0.2.14发布 | 参数协议能力需与产品保存值/实际wire逐项对齐；图片开关不代表音视频全模态。保留用户百炼 Base URL、已有 Key、qwen3.8-flash；Agnes 仅是当前默认视觉配置名，架构不能写死该名 |
| 消息与工具呈现 | 暂态思考、独立状态、跨轮过滤、旧自动记忆降噪已实现 | 重构须保留 room/turn/call 作用域、终态不能倒退、旧历史不破坏及同轮原生工具配对；不要又造第二套 UI ledger |
| 搜索/定位/用量 | 房间全文子串检索和定位、逐轮耗时、provider已返回用量 | 不是 FTS、不是跨工作区搜索、不是准确账单。`chat_insights.rs:136` 是遍历内存消息；`:163` 房间范围；`:197`说明缺 usage 不计入。dsh 的检索属于可借鉴索引升级，不是“从0增加搜索” |
| Goal 与多 Agent | 主 Web 路径有 phase claim、重试、implementer blocked→planner、目标暂停及升级 | 不能因为 coolzhu-orchestrator 插件未接线就断言主产品没有编排。`main.rs:37675` 已存在失败回退及统一 retry_count，仍需幂等、并发、恢复测试证明闭环边界 |
| 浏览器与桌面 | MV3 owned tab bridge 和正式桌面 bridge 都实现 ComputerUseAdapter | 应独立列正式用户入口、模型工具入口、开发靶场/仅plan接口；`computer.*` 调试目录不能与 `computer_use_perform` 真实工具混为一张能力表 |
| 运行证据与打包 | 0.2.14有安装包源码身份、哈希、安装用户数据保留、1175通过证据 | 测试数不是功能覆盖率；Paint端到端失败、Anthropic在线跳过、原生窗口启动被审批拦截都必须与“通过”并列。dsh尚无本轮同条件实测，不能用包数量替代性能/可靠性比较 |

重复计数主要在“seam接口包+provider实现+tool暴露+UI呈现+bundle装配”被当作五项不同用户功能。例如 dsh computer-use 基础包+2个experimental provider不等于3套产品目标能力；session格式/持久化/投影族也不是19项用户功能。A 的tool-registry、Web动态工具、调试工具目录同理。建议每个用户能力只分配一个 feature_id，各包仅作为实施组成引用。

## 待验证项，禁止从静态清单推出

1. 原文 `[完整]` 每项是否进入目标发布 profile、是否有用户入口、是否打包了实际依赖、生命周期卸载是否可靠。
2. Windows 原生会话写锁的跨登录会话范围；目标整合应明确单用户本机会话边界，不能把命名内核信号量当分布式锁。
3. 统一配置与 dsh 参数注册/凭据引用整合的兼容迁移，尤其空 Key 不覆盖已存 Key、保留精确 Base URL、无效参数不静默忽略、旧历史/旧会话仍可打开。
4. 所有输入通道的授权一致性、敏感字段与密码控件识别；当前关键词规则和输入长度限制不是完整内容安全证明。
5. 中文 Windows 编码、100/125/150/200%缩放、负坐标与跨屏窗口、画布与工具栏识别、输入后稳定等待。既有150%DPI修复只证明该缺陷修复，未覆盖整个矩阵。
6. CU观察新鲜度导致失败究竟是窗口/控件真实变化，还是计划耗时/观察代际粒度造成误拒；不可简单延长时限或移除stale检查，应记录各阶段时间与身份变化。
7. UI与搜索在1万/10万消息、流式重连、切换房间/工程时的性能和作用域；目前子串遍历并不等于索引检索。
8. 异常退出后的工具/Goal/CU终态恢复、同一调用重放幂等、取消传播、未返回usage的可观测性。原包清单没有这些可测契约。

## 逐阶段整合候选与门槛

| 阶段 | 候选工作 | 为什么此时做 | 必须通过的门槛 |
|---|---|---|---|
| P0 基线与测量 | 修正文档标签和分母；建立 feature_id→生产入口→事件→存储→测试；保留0.2.14黄金样本；接最小CI | 先消除误判和重复开发，再评价dsh差异 | 现有1175测试基线可复现；安装版冒烟/失败样本单独报告；文档不再将注册表数量等同模型可见功能；API Key/历史不外泄 |
| P1 CU可靠性实验 | 观察/规划/输入/稳定等待/验收结构化trace；画布区域与工具状态显式表示；局部裁剪+原图映射；有限stale刷新；分层验收 | Paint仍未通过，存在Agent可检验改进假设，不能仅归因模型能力 | 先mock/靶场再Paint；同模型同初态A/B；报告物理输入率、有效画布落笔率、误报成功率、stale拒绝原因、耗时与token；不得提高画质要求或用预制图替代输入任务 |
| P2 保行为提边界 | 从Web提取 ToolLoopService、ConversationProjection、UsageLedger、SessionConfigService；与CLI建立共享契约，暂保HTTP/SSE/SQLite兼容 | 借鉴dsh seam/provider/tool分层，避免一次全栈迁移同时改变产品行为 | 黄金wire/SSE回放无退化；room/turn/call身份保持；工具参数不进入思考，跨轮记忆投影不退化；取消/终态矩阵通过 |
| P3 补高收益缺口 | FTS派生索引、稳定事件查询、LSP真实接线、插件安装结果与真实状态一致、诊断故障注入、日志保留 | 这些能在不替换已工作的UI/运行时前提下改善产品 | 索引可重建且按工作区授权；安装失败不报installed；LSP重启/退出无泄漏；诊断不以常量“ok”冒充测试；大历史性能门槛先测后定 |
| P4 受控扩展 | 评估dsh适配注册、PTC、子agent协议、workflow/SDK、实验CU provider作为可选桥接 | 只有接口契约稳定后再评估跨Rust/TS运行时成本 | 每项有依赖/进程/凭据/取消/持久化所有权；不能同时让两个runtime拥有同一会话；实验provider须打包与真实功能验证，禁止默认启用未经验证组合 |
| P5 UI和发行 | 保留玉石icon、竹林背景及对比度约束；在既有左右布局上优化；升级/回滚/运行证据统一 | 不重建已完成的统一配置和消息体验 | 低高度窗口可用、键盘可达、后台运行中切换保护、搜索定位回放、升级保留配置、签名/发布追溯；用户审批后才实施新视觉方案 |

推荐原则：优先迁移 dsh 的边界设计、可回放测试与声明式装配思想；不能根据291对28的数量差认定应整体重写。实际引入其包代码时另审依赖、许可证和部署成本。本文件只提出整合候选，不授权产品改动。
