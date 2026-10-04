Key lines found. Reading the planner request construction (540-780).Now find `provider_client_for_agent` and `dispatch_model_request` and how ACP agents are handled.Core finding confirmed. Now architecture coverage of other modules in parallel.## 第二轮只读审查（全模块架构覆盖，不是逐行全量审查）

**本轮事实：** 共调用宿主工具 61 次（1 次 tools/list，60 次 read/glob/grep），全部返回 ok。比计划的 30–50 次多。其中 9 次 grep 结果被截断，溢出文件我没有回读，相关项已列为待核。没有运行任何测试。下文 `wc/` 指 `repository/modules/gui-web/packages/web-console/src`。

### 一、重点结论：Devin 不能做 computer-use 的规划和验图

挂了工具名字不等于能力接通。现有代码链条如下：
1. 规划器每次发请求都走 `crate::provider_client_for_agent(agent)`（`wc/computer_use_planner.rs:646`）。发送由 `dispatch_model_request` 调 `ProviderClient::send_message` 完成（同文件 931-942）。
2. `provider_client_for_agent` 先执行 `backend.require_http()?`（`wc/main.rs:32397-32401`）。DevinAcp 和 DevinCloud 在这里直接返回 UnsupportedCapability（`wc/agent_session_backend.rs:29-37`）。
3. 规划器取的就是本次调用所在的会话（`with_context` 只接收 identity，计划器文件 566-572；`agent()` 按 session_id 取会话，616-621）。没有另配规划会话的入口。`ProviderClient` 本身也只有 HTTP 实现（`llm-adapter/src/client.rs:30,326,347`）。
4. 即使规划能走通，Devin 分支也会拒绝图片输入，报“Devin 图片输入尚未开放”（`wc/main.rs:32344`），所以验图同样做不了。
5. 桥本身也没有接这个工具：`INITIAL_TOOLS` 和 `REVIEW_TOOLS` 都不含 `computer_use_perform`（`devin_acp/bridge.rs:14-22`），列表按名单过滤（212）。`review_enabled` 要求 `computer_use_enabled == Some(false)`，否则报错（`devin_acp/chat.rs:132-140`）。

**结论：** 在 Devin 会话里，就算把 `computer_use_perform` 加进桥，第一步规划也会以 `planner client:` 错误失败关闭。真实规划和验图目前只能由 HTTP 会话完成。

### 二、风险（有文件/行号证据）

**P0：** 本轮没有确认新的 P0。

**P1**
- **开发开关可绕过规则：** `dev_open_tool_permissions_enabled` 读取 `config.tool.dev_open_permissions`（`wc/main.rs:3608-3616`）。开启后 ACP 桥用 FullAccess 档案，且规则为空（`bridge.rs:217-225`）。非只读模式下 `INITIAL_TOOLS` 包含 bash 和 write，等于一个开关就能拿掉 ACP 的规则约束。本轮每条回执都显示 `full-access-profile`，但看不出来源是这个开关还是当前激活档案。发行默认值待核。
- **同一轮内无法做 Browser→Paint：** 这是设计限制，不是缺陷，但会直接卡住接线。
  - 用户这轮只要提到“右侧浏览器”等词，任何桌面目标或 surface=desktop 都会被拒（`wc/computer_use_turn_scope.rs:15-18,40-53`）。
  - 用户这轮给出的显式 `computer_use_perform` 参数只允许正好一个（同文件 77-78）。之后参数不同的调用会被判为 `request_mismatch`（33-39）。
- **规划会话和发起会话绑死：** 见第一节第 3 条。想用别的会话做 CU 规划，没有受控入口。

**P2**
- **画布判定偏宽：** `canvas_element` 把所有 Document 或 Image 控件都当画布，并硬编码了 `mspaintview`（`wc/computer_use_desktop_bridge.rs:896-907`），有在非画布文档上落笔的风险。笔画参数有边界：2–256 个点、时长不超过 5000ms（909-930）。
- **SKILL 没有版本快照：** `active-skill.json` 只存 `selected_id`（`wc/extension_market.rs:226-233`）。Goal 用 SKILL 时每次现读磁盘、截到 6000 字，不记录哈希（`wc/main.rs:46989-47010`）。桥的注释也写明插件、CU、MCP 需要版本快照验收后才开放（`bridge.rs:216`）。
- **插件激活未见内容哈希：** `dsh_*.rs` 里 grep sha256/digest/snapshot 没有命中。`install_transaction.rs` 未检，待核。
- **launcher 的 `CleanupKnownHolder` 会结束指定 pid：** 见 `packages/app-launcher/src/main.rs:536-541`。“已知持有者”的判定函数没读，待核。`Block` 分支会按工作区、会话库、构建版本、当前用户核对后才决定是否复用（542-552），这一部分合理。

### 三、Browser Use→Paint 最小可执行接线

1. **规划会话：** 在接纳阶段的冻结父上下文里加 `planner_session_id`，由用户或配置指定，模型不能选。`with_context` 从这里取规划会话，并校验它是 HTTP 且支持视觉。诊断里同时记录发起会话和规划会话，保留精确模型 ID 比较。
2. **桥接线：** 只在第 1 步满足后，才把 `computer_use_perform` 放进 ACP 桥的名单。调用转入现有的 `execute_in_room` 路径（`wc/computer_use_executor.rs:2237-2260`），沿用桥已有的父接纳、预算和同库检查（`bridge.rs:196-205`）。同时放宽 `review_enabled` 的互斥条件。
3. **轮内多步：** 把 TurnScope 的单个显式请求改成用户显式声明的有序序列，例如“先 browser:url，再 desktop:mspaint”。每次调用依次匹配下一项，用掉即作废，仍禁止从历史推导。另一个零改动的办法是拆成两轮。
4. **预算：** 统一用 `config.budgets()`（`executor.rs:2247`）加根预算，不给 mspaint 开特权，和主会话第 4 点一致。
5. **数据交接：** 浏览器结果怎样带进 Paint（剪贴板、文件或文本），本轮没有找到现成通道，待核。
6. **画布判定：** 收紧为“窗口身份匹配 + 画布控件”双条件。窗口身份可复用 `window_target` 的 exe 匹配逻辑（`vision/uia-resolver/src/window_target.rs:88-105`，测试里已拒绝伪造标题）。

### 四、其余主题

- **上下文和记忆：** Devin 分支复用了 `build_context_assembly_with_roster`，参数包括 reset floor、聊天室和名册（`wc/main.rs:32345-32348`），只是不带图片。core-runtime 里有 `should_compact` / `compact_session`（`compact.rs:37,89`）和 `summarize_memory_beads`（`memory.rs:152`），但它们是否在组装上下文时被调用，没有追踪，待核。
- **启动动画：** 总时长 6500/7800ms，分为展开（<1400）、舞剑和标志（<5700）、停留（<6700）、交接四段（`tauri-shell/ui/scroll-startup-player.js:43-44,65`）。素材 2s 超时、减少动态 120ms、帧绘制出错立即收尾（172-189），并导出了 `stop`。界面上有没有跳过按钮，待核。
- **图标：** `setWuxiaIconOnly` 只写中文标签到 aria-label 和 title（`wc/app.js:8986-8993`），确实没有暴露内部 ID 或调试信息。
- **右侧浏览器：** 网页跑在独立、无宿主权限的 WebView 里。栏折叠或有弹窗时隐藏，并按 DPI 比例换算坐标（`wc/native_browser_panel.js:1-35`）。浏览器策略（拖拽、组合键、多标签）由 config 读入（`executor.rs:2226-2231`），对应的配置界面没找到，待核。
- **vision 定位：** 优先用 UIA，其次模板匹配；有 min_confidence 阈值和 `dpi_scale` 字段（`vision-service/src/locate.rs:101-106,119,220`）。
- **windows-process-guard：** Job 对象设置 `KILL_ON_JOB_CLOSE`（`lib.rs:25-28`），交互输入租约带 epoch（121-148）。恢复用的命名管道受用户 SID ACL 保护，并在注释里明确“同 SID 不代表人工确认”（`local_control.rs:1-2`）。

### 五、模块覆盖表

| 模块 | 读到的关键入口 | 结论 |
|---|---|---|
| CU 规划器、执行器 | planner 538-738、925-965；executor 2200-2260 | 只支持 HTTP，已确认 |
| devin_acp 桥、chat | bridge 1-25、195-225；chat 100-180 | 工具名单和互斥条件已确认 |
| turn_scope | 1-112 | 单显式请求、浏览器排斥桌面 |
| 桌面 bridge | 870-930 | 画布判定偏宽 |
| llm-adapter | client.rs 入口 grep | 只有 HTTP |
| tooling | plugin-system lib grep、dsh grep | 只读了骨架；command-router 和 compatibility-harness 只定位到文件，没读 |
| vision、uia-resolver | locate grep、window_target 测试 | 架构层面已覆盖 |
| tauri-shell | 文件清单、启动动画 | 原生浏览器各 .rs 文件没读 |
| launcher | main 515-555、config | 已知持有者判定待核 |
| windows-process-guard | lib、local_control grep | 架构层面已覆盖 |
| core-runtime memory | grep 定位 | 和 ACP 的调用链待核 |
| 前端 app.js | 8986-9045、native_browser_panel | 配置界面待核 |
| diagnostics、cli、clawbot-sidecar、agent-server、language-service | 没读 | 未覆盖 |
