# 内置 Browser Use 技术审查与实施计划

日期：2026-09-30。执行者：当前主会话；按用户最新要求，不使用子代理实施或测试。

## 结论与范围

2026-09-30 后续实施：原生宿主资源登记已经进入代码。新增独立 `native-browser-protocol` 契约和两端 `native_browser_host` 模块，后台按真实工程路径和只读 SQLite 房间关系解析身份；宿主仅登记可见、完成加载的面板。独立随机凭据按服务端口隔离、750ms 心跳和3秒租约，重放序列/另一活宿主拒绝；测试凭据只写 tmp，不轮换真实宿主令牌。没有新增网页 IPC 或任意 JS 接口。此为第1阶段的资源/登记部分，**请求派发、DOM快照、类型化输入、父运行与取消接线尚未完成**，不能称已实现模型 Browser Use。代码跟进不在已经归档的0.2.32 MSI内。

3秒租约仅判断连接存活，不能代替输入派发时核验。后续输入仍须在宿主所在线程即时确认可见性、聊天室/工程、instance generation 和 document revision，并核对冻结父运行。租约存在不授予输入权限，也不能以心跳空档为旧动作继续执行的理由。

右栏 WebView2 的人工浏览能力与模型 Browser Use 是不同验收项。当前模型链路的 `BrowserNativeBridge` 连接 Chrome 扩展及 Native Messaging Host，尚没有右栏 WebView2 的 DOM 快照/类型化动作适配。右栏新窗口链接修复不能代替模型操作验收。

本计划仅补这项缺口，不建立第二套 Agent 循环，不自动切换到 Chrome，不扩大外部网页的 Tauri 或 Shell 权限。微信保持原功能且不测试。Devin 依用户要求暂缓，依据另见 `../testing/devin-cloud/official-integration-decision-2026-09-30.md`。

## 代码事实

- `modules/gui-web/packages/web-console/src/browser_bridge.rs`：已有请求 broker、超时、请求 ID 和 reply token 匹配。
- `browser_bridge_protocol.rs`：已有有限的 Snapshot/Act/Tab 协议、document ID 和 BrowserAction 类型；当前 tab ID 是 Chrome 数字标识。不能直接把 WebView label 当该标识传入。
- `computer_use_executor.rs`：browser surface 仍使用现有 BrowserNativeBridge。
- `modules/gui-desktop/packages/tauri-shell/src-tauri/src/browser_panel.rs`：负责右栏原生浏览器的布局、导航、工程作用域和实例生命周期。
- 前端 `native_browser_panel.js`：处理浏览器布局和导航，不提供模型 DOM 操作接口。

## 既有聊天审核会话的意见

审查来源：[整合审查执行计划](https://chatgpt.com/c/6ab013c6-8dc4-83ea-a220-a33c9940783f)，本轮“Browser Use 窄审结论”。页面显示“极高”；本次未独立核实模型的完整版本名称，不据此证明已由特定 GPT-6 Pro 版本验收。

审核同意独立 `native_browser_use` 适配模块，经现有宿主 broker 和权限/资源检查后，在宿主内部使用 WebView2 COM/CDP。仅复用协议语义，不把 Chrome 扩展冒充右栏控制器。至少绑定 run、聊天室、工程、浏览器资源、实例 generation 和 document revision。页面 DOM 只作观察，不作授权。危险提交保留现有审批。

主会话补充：审核建议的“丢弃迟到回调”只能阻止结果写回，不能撤销已经发出的 COM 输入。因此不能只在完成回调检查 generation，然后声称取消绝无迟到动作。动作派发前必须再次验证身份/取消/作用域；已经派发的动作必须登记为 in-flight，等待执行结果或记录结果未知，不能在新资源上重放。

## 实施顺序及模块职责

1. **资源绑定与请求运输**：在现有 broker 增加显式宿主后端类型和资源绑定，保留 Chrome 后端。复用现有请求/响应匹配和有界超时；不得用页面可调用的 IPC 来连接控制通道。认证材料只由宿主读取，不写入前端或日志。
2. **只读快照**：独立原生适配器获取可见 DOM 的 role/name/text 与有限可操作节点。为节点分配与 document revision 绑定的短期 ID。长度、节点数和响应体有界；不提取密码值。跨来源 iframe 无法安全观察时明确标记，不承诺任意页面全部可操作。
3. **类型化动作**：先接 click/type/scroll/navigation。模型只能提供观察所得节点 ID 和结构化参数，不能提交任意 JavaScript、CSS selector 或 CDP method。宿主可以使用固定内部实现，但该能力不得开放给网页或模型。
4. **生命周期与取消**：关闭/隐藏面板、切换聊天室或工程、导航、超时、运行取消、权限撤回，使旧资源/节点失效。导航与 DOM revision 必须在输入派发前重新核验。串行派发，登记 in-flight 状态；超时未知不能被当作“没有动作”后自动重试。
5. **现有工具接线**：继续使用当前 computer-use 的规划、审批、时间预算、错误归属和轨迹，不增加独立模型运行器。用户明确选用内置浏览器时，只选择绑定的右栏资源；缺连接就返回准确错误，不悄悄转 Chrome。
6. **正式包回归**：完成相关离线编译与安全检查后，使用正常启动器、原有工作区、真实 qwen3.8-flash 实操并截图。不能用 Sky 手动完成网页步骤代替模型动作。

## 必须通过的真实验收

| 编号 | 操作 | 通过证据 |
| --- | --- | --- |
| BU-1 | 正常启动器启动 | 输入安全根由现有启动器注入，恢复归属有效；不手工绕过门禁 |
| BU-2 | Qwen 读取右栏普通 HTML | 工具快照的资源/标题与原生截图一致 |
| BU-3 | 输入唯一标记并点击、滚动 | Qwen 工具回执、后续快照和原生页面状态一致 |
| BU-4 | SPA 更新与重新导航 | 新 document/node ID 生效，旧 ID 拒绝 |
| BU-5 | 观察后人为导航 | 旧动作拒绝，不误点新页面 |
| BU-6 | 动作期间取消/超时 | 不再派发后续动作；已派发动作准确标记结果，不重放 |
| BU-7 | 危险提交 | 进入现有审批，无批准时不提交 |
| BU-8 | 切换聊天室/工程及隐藏面板 | 旧资源失效，没有跨环境输入 |
| BU-9 | 外部网页尝试桌面命令 | 被现有 ACL 拒绝；不新增网页可调用的 IPC 或调试端口 |

测试网站可以使用普通本地 HTML 样本；**模型调用必须真实 Qwen**，不得使用模型夹具。报告同时保存模型会话轮次、工具结构化回执、动作前后截图和候选/安装包身份。直到这些步骤通过，状态保持“未验收”。

## 当前实测失败点

`BROWSE-USE-20260930-A`：真实 Qwen 会话第 17/18 条，13.3 秒，工具在 `intent_guard` 返回 `input_safety_root_not_injected`，0 步，未通过。该次开发进程绕过启动器，因此不能归因于右栏桥已执行失败。后续改用产品相同 bin/config 布局的临时候选，经原启动器启动；仍保留安全库已有隔离事实，不能清库或自动放行。

本计划是已完成的技术方案审查记录，**不是已经实现原生适配器的证明**。

`BROWSE-USE-20260930-B`：改用正常启动器启动临时候选，真实 Qwen 第 19/20 条，09:55:05—09:55:18，13.9 秒。工具在 `intent_guard` 返回 `input_safety_resource_not_accepting_new_input`，资源 `isolated revision=12`，0 步、0 动作、`goal_achieved=false`、`retry_owner=user`。启动器根注入已经生效，但现有隔离事实仍然有效；不能清库或由测试者自行放行。模型没有完成读标题、输入或点击。

轨迹运行 `run-chat-d44a62a920648aab11cde6dd3c06e0a18d08d5154da7920f` 的聊天轮为 completed；其真实工具登记 `tool-b62dd8935cb549ac1647e89b3e37072e5c4dadacb6d8b09db472276dd393a3cb` 为 failed。**聊天轮结束不等于浏览器任务成功**。实操截图：`docs/testing/release-0.2.29/regression-20260930/candidate-browser/06-qwen-browser-use-isolated.jpg`。

## 实施前代码复核补充

0.2.31 构建后的只读复核发现，不能直接把现有 Chrome socket 改接 WebView2：

- `BrowserBridgeBroker.connection` 当前是单个连接；`handle_native_socket` 的 Hello 只有 nonce，连接建立后会覆盖前一个 sender。原生面板必须拥有独立后端登记和绑定，不能与 Chrome 互相抢连接；类型化请求需明确后端，选择失败不能自动回退另一浏览器。
- 前端 `native_browser_panel.js` 的 scope 来自 `{context:{workspace:activeWorkspaceKey,room:activeChatRoomId},generation}`，只用于展示生命周期。CU 使用经 SQLite 权威关系核对的父运行 `workspace_id` 和 `room_id`。前端工作区键不能直接充当 CU 的冻结工作区身份，字符串相同不等于已核验。
- `ProductionAdapterFactory` 在每次生产执行接纳时创建，可携带已经核对的父运行上下文和取消信号；不需要为了原生浏览器修改所有测试适配器的 `build(surface)` 接口。宿主登记的面板环境必须经后台权威映射后与该上下文匹配，模型和外部网页均不能提供授权来源。
- 当前 Chrome 协议 `valid_tab_id` 仅接受最多 32 个数字；原生 label `browser-panel-N` 不是该协议的合法 tab。原生后端须有明确资源类型及代次，不改宽该校验以混用两个资源命名空间。
- `browser_panel_command`、`with_webview` 已分别负责主控制台来源校验和 COM 所在线程。原生工具适配模块应从宿主内部调用能力，不能新增给外部网页的 Tauri command，也不能向模型开放任意 JS/CDP。

这些是实施约束及未完成接口，不能以文档存在当作完成第 1 阶段。后续实现顺序仍为资源/运输、只读快照、类型化动作、取消生命周期、工具接线、真实截图验收；不得先开放输入再补身份绑定。
