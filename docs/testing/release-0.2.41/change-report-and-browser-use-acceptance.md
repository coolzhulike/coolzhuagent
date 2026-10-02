# 0.2.41 原生只读成功终态仲裁与验收交接

2026-10-01，主会话独立实现和测试。用户顺序仍为Browser Use优先、Paint其次；真实模型维持已有qwen3.8-flash、百炼Base URL、API_KEY和思考配置。不使用子代理，不把工程回归当软件实操。

## 具体问题与行为变化

037真实Q/R均失败；R已取得认证native-ax，但固定false验收和事实未回传导致失败。039补只读验收，040补模型判断后同宿主重新采样。两包原始收据保留，均未作为新版本真机验收通过。

040复审提出最终first-wins要求；主会话源码核验确认：原CU行CAS只防覆盖已落CU终态，父运行取消另有Immediate事务。父stop_requested先提交、CU行未终态时，旧实现仍可能写goal=true，父会话最终interrupted。虽然不会继续输入，父子任务状态会矛盾。已向既有审核会话说明实际限制并取得[终态窄审](../release-0.2.40/review/20261001-readonly-terminal-review-dom.txt)，据此补齐本次最小修改。

本轮改变：原生只读成功落库时，复用冻结的父上下文和同一SQLite库，取得BEGIN IMMEDIATE后检查父运行关系与执行状态，再按既有call/state_version/terminal为空的条件提交实际CU终态。父取消先提交则CU取消且goal=false；CU成功先提交则保留当时成功事实，后续取消不能重写它。预算过期归为TimedOut，父上下文或数据库不符归为Blocked；不归为网页内容验收失败，不补发模型调用或第三次AX，不转其他浏览器。

## 模块职责与范围

- `main.rs`：抽出使用已有连接的冻结父关系核验，旧路径继续调用同一实现；房间/会话存在性与父运行工程、房间、会话、轮次、当前状态由真实数据库回答。
- `computer_use_store.rs`：新增受限的事务提交入口；持久层只负责Immediate事务、版本/终态CAS、计数和实际结果写入，判定不得替换call/provider/surface身份。旧提交路径复用原落库函数。
- `computer_use_executor.rs`：生产执行器冻结原生父上下文；原生浏览器成功终态在事务连接内核对实际数据库路径、CU持久deadline、单调根预算、父关系和取消状态。收紧结果保留已有观察、计数和收尾事实；CAS失败返回原终态。

040的宿主AX、独立观察ID、语义节点索引、S1/S2有序投影比较、片段限长和诊断脱敏随包保留。实际WebView2 COM在桌面`with_webview`所属UI线程执行，Web工作线程仅等认证broker；面板观察前与回调后核实际visible/minimized/active/hidden/loading/destroyed。

证据时效点为S2；成功终态的裁决点为同库写事务。没有把UI生命周期、内存取消或单调时钟伪称为跨UI/数据库原子事务。S2后出现的新页面变化不倒改已完成S2事实。所有原生输入能力仍关闭，仍未证明click/type/scroll/navigation。

## 工程验证与构建身份

最终终态源码离线Web build退出0（30.50秒）。新增两项必要的SQLite边界回归均通过：真实父停止入口先提交、成功先提交、预算过期、内存取消、另库/缺父归属及迟到结果；第二连接写入在成功判定事务内被SQLite拒绝、事务完成后可写。完整Web回归1283通过、0失败、2项既有忽略（42.56秒），另lib8/0、宿主静态契约1/0。保留132条既有编译警告。它们不调用模型或软件，不替代截图验收。正常release安装包已完成并独立核验；下表为本次真实构建身份。

安装包：[CoolzhuAgent-0.2.41.msi](C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.41.msi)。大小246762877字节。此前040保留历史，不推荐用其验本补丁。

| 身份 | 实际值 |
|---|---|
| MSI SHA-256 | `ad00c97057aa717300fe4f5fcdbb6d964ee0bd36bc6f3780ab2c88cc3d5612fa` |
| 构建报告ID | `pkg-report-release-20261001-012315274-8e7346e0` |
| 源快照摘要 | `4515395f72a402833179f6566adcec7f12ce704aa523d805f9ed353ae43f104e` |
| 构建输入摘要 | `e30d67e3ccfc954de4b73ec3a84ffeb63f81ca3dadcca151b80f68dd81b4ecac` |
| 载荷摘要 | `a28ea06af7b52908a8be4dfacf18dcf7941e4ce501ca904c84f2baf95ae9bf72` |
| 源参考提交 | `4f7a46fc86f6bb7b43fdeaefb5784987fb3217ae`，dirty工作树；仅种子，源码权威为快照 |
| 构建时源码稳定性 | 构建前后逐文件一致；不是clean commit build |
| 发布门/产物/载荷 | 6/6 pass；10/10源与独立暂存文件摘要一致；859文件 |
| 内容扫描 | safe=true，0发现 |
| 暂存位置 | `tmp/candidate-041-package`；没有覆盖运行中的`package/` |

六份原始JSON与字节保留规则已[归档](evidence/build-identity/pkg-report-release-20261001-012315274-8e7346e0/package-report.json)，包括安装报告、包报告、文件证据、载荷清单、内容扫描和独立暂存核验。原WebView2Loader沿其实际producer输出复用且摘要匹配，不称全部产物重新生成。提交e284355的两项远端Web baseline检查已成功（运行36751433570、36751426381）；后续修改另核最新Checks。

终态窄审的[完整页面文本](../release-0.2.40/review/20261001-readonly-terminal-review-dom.txt)及[最新结论可见的原始截图](../release-0.2.40/review/20261001-readonly-terminal-review-visible.png)已保存；此前同名无visible截图位置未包含最新回复，不能拿它单独核验结论。

用户正常安装并打开控制台后，已核验正式安装版本0.2.41、安装日期20261001、安装目录10/10关键产物与本包摘要匹配。实际进程为安装目录Web12104/Tauri27804，8765由Web12104监听。保持原工程`C:/Users/zhupu/coolzhuagent`、房间`room-1790469689319`及Qwen会话`session-1779459149988`，百炼Base URL、密钥、medium和多模态配置未改。身份收据见[安装核验](installed-native/installed-artifacts.json)。历史blocked by policy未绕过；本轮正常启动没有要求重复安全放行。

## 安装后真模型验收

| 编号 | 前置与操作 | 必须记录的事实与截图 | 状态 |
|---|---|---|---|
| BU041-IDENTITY | 正常安装与启动，原工程/房间/Qwen配置 | 包摘要、10产物、安装目录进程和原生控制台；未改Qwen配置 | 已核验 |
| BU041-READ | 右栏打开普通HTML，真Qwen只读标题/标记/正文，不在提示中给答案 | S轮3/3标准、S1/S2独立ID、实际回复与网页一致、0输入、原图03 | 通过本项；不代表交互通过 |
| BU041-SPA | S1后在同URL改变普通页面文字 | W轮自动时间网页同URL变化，native_browser_observation_stale/goal=false/0输入，原图05 | 动态变化拒绝通过；V人工点击晚于S2，不计覆盖 |
| BU041-ENV | S1后隐藏/最小化/关闭面板或切工程/房间 | X关闭晚于S2，未命中目标窗口；Y无已载入页面即panel_unavailable | S1后资源失效仍待验；不得用Y替代 |
| BU041-CANCEL | 模型判断或重采样时从正常运行界面取消 | Z在verifying时中止，父stop先落库、CU cancelled/goal=false/0输入；迟到Qwen完成未覆盖，原图07 | 本轮取消先提交通过；不代表输入进行中取消通过 |
| BU041-CONFLICT | browser同时给URL和window/application | surface_conflict、0观察/输入，不自动换目标 | 待完成 |
| BU041-INPUT | 只读通过后按已审方案接入类型化click/type/scroll/navigation | 短期节点、派发前资源/权限/取消/输入锁、释放回执、真Qwen前后截图 | 尚未实施 |
| CU041-PAINT | Browser闭环后，真实Qwen在Paint作画 | 闭合轮廓与简易海绵宝宝、释放与视觉判定、四边泛光和英文提示、结束撤除 | 尚未续画 |

工程检查与软件验收分开记录。真实Qwen不合法JSON、模型不执行或资源错误均如实失败；不换模型或用夹具填补。旧L形/人工页面导航/旧037结果不能作为本包通过证据。四项任务未总体完成，DSH远程插件实际安装运行等开放项保留，微信不改不测，Devin暂缓，PR74保持Draft。

本轮逐轮事实、截图及未覆盖原因见[实操记录与开放问题](installed-native/acceptance-and-open-issues.md)。T轮暴露中文三种禁用动作排列未被识别的Agent问题，源码已扩展有限明确措辞、离线Web build通过、3项必要边界测试通过；该补丁尚未安装，不能追认T成功。U/V对“当前内容”选错宿主节点，目标字段回答仍不通过；稳定标题/标记/颜色的S与同URL动态拒绝的W不受该结论替代。
