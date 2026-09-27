# 0.2.18 改动、验收与针对性测试入口

日期：2026-09-27。这是构建后持续补记的验收索引，不属于候选包冻结时的原始源码快照。当前为已安装候选版，原生窗口验收进行中；不能据此宣称四项任务或 32 个工作包全部完成。

## 本次对象

- 分支 `codex/navigation-update-followup`，延续已合并 PR #67；本次 PR 待创建。
- MSI：`dist/CoolzhuAgent-0.2.18.msi`，SHA256 `33F60F05143FEDF73A34678F7E3A3D06D26D2D194D217EFB3AA8730CBCA6DEB2`，未签名的本机候选包。
- 源码快照 `ab4f5c2cfbe308159a8ea14857943656e74acd2ae47ed5408bd17492326fcea1`；包报告 `pkg-report-release-20260927-131953270-070fadf3`。准确身份见 [构建收据目录](evidence/build-identity/pkg-report-release-20260927-131953270-070fadf3)。源码来自有改动的工作树，不能只用构建基底 Git 提交代表全部内容。
- 已正常从 0.2.17 升级；10/10 关键安装产物匹配。安装版 Web SHA256 `581822BE7B8EAC97A9CC6AF1B64AEA5F5BA2369CE13445682FE0154D71CA1081`，CLI `0EB7B9135C69CE99815A92260454BC5F97FFF3ECA62B94B5FDB343F366F6555F`。
- 停机升级前后 66 个文件、41 个附件、6 个 SQLite 库和 40 张表的脱敏语义清单一致；[首次启动对账](../navigation-update-followup/installed-0.2.18/upgrade-data-acceptance.md)也已完成，旧消息/配置/附件未丢，新增 run_id 列和访问元数据变化如实列明。真实备份及其密钥仅留忽略目录，不进入仓库或 PR。

## 代码职责与测试切入点

下列路径相对仓库根目录。Web 路径前缀为 `modules/gui-web/packages/web-console/src/`。优先做真实窗口操作与有实际副作用的短流程；接口证据用于解释窗口无法证明的执行和归属语义，不为凑数量重复实现细节断言。

| 功能 / 优先级 | 主要代码职责 | 其他模型设计用例时必须观察的事实 |
| --- | --- | --- |
| 聊天过程、最终正文、目标一致性 / P0 | Web `chat_experience.js`、`chat_tool_history.rs`、`chat_run_admission.rs`、`main.rs` | 思考/工具过程运行时只占 1～5 行，完成后隐藏，轨迹保留；仅思考无正文给明确状态、不复制思考；设置编辑对象不改变发送对象；空目标不能发；切房间不串正文。 |
| 模态路由及用量 / P0 | Web `multimodal_input.rs`、`request_usage.rs`、`chat_insights.rs`、`model_settings.js` | 支持图片者收到原图；纯文本目标只收到默认视觉模型的描述；两个真实请求同 run、各记一次、视觉请求仍归真实视觉会话；旧未知归属不猜测回填；保留百炼 URL/Key 引用。 |
| 子 Agent、Goal / P0 | Web `host_child_agent.rs`、`tool_loop_coordinator.rs`、`main.rs` | CLI 经 Web 宿主接纳；父首轮/子请求/工具反馈分别计量；子任务继承权限和取消、不能扩大权限；Goal 运行中切工程拒绝，结束后允许；失效执行权不能继续落盘。 |
| 工具权限与输入隔离 / P0 | `modules/tooling/packages/tool-registry/src/path_effect.rs`，Web `computer_use_store.rs` | cwd 不是不透明命令的文件访问边界；既有完整访问授权正确继承；Paint 历史资源未知时安全拒绝与零输入如实显示，测试人员不得代替本人复核或清理历史绕过门禁。 |
| 数据与定时任务 / P0 | Web `main.rs`、`scheduled_jobs.rs` | 重启不能跨房间复制历史消息；clean 高版本库拒绝前不改字节；已领取但无结果的 occurrence 显示未知/待核对，不自动重复派发；备份包含历史敏感数据，不能当脱敏文件发布。 |
| 更多、右栏、更新 / P1 | Web `workspace_panels.js/.css`、`app.js`、`app_update.rs`、`index.html` | 五主入口、更多四入口、底部更新可达；Esc/外点/焦点返回；文件/图/视频/URL进入对应右栏；低高度可用；真实版本与检查失败/无发行/最新/新版区分。检查不自动安装。 |
| 终端 / P1 | Web `terminal_host.rs`；`modules/gui-web/packages/windows-process-guard/src/conpty.rs` | 中文命令实际生成文件；切目录后状态保持；刷新可续接；Ctrl+C 后可执行新命令；关闭有真实进程回收。回显不能作为执行证据。 |
| 语言服务 / P1 | Web `lsp_host.rs`；`modules/core-runtime/packages/language-service/` | 真实 rust-analyzer 启动、诊断/定义/引用到准确行列；右栏预览可读；停止后进程与句柄状态真实；不能只测静态列表。 |
| MCP / P1 | Web `mcp_host.rs`；`modules/core-runtime/packages/core-runtime/src/mcp_stdio.rs` | 官方 stdio 服务发现工具→模型调用→结果回传→最终回复；工具只属当前工程、权限拒绝零调用、小上下文不全量剔除 MCP。HTTP/SSE/WS 不是已交付 transport。 |
| 插件事务 / P1 | `modules/tooling/packages/plugin-system/src/install_transaction.rs` | 准备/校验/提交分离、跨进程互斥、失败可恢复；安装不等于执行授权。Web 安装接口仍未交付，不能用本地目录列表宣称远端市场安装成功。 |
| 包身份 / P0 | `scripts/lib/build-identity.ps1`、`scripts/lib/webview2-loader.ps1`、打包与保留脚本 | JSON 日期按原字符串回读，日期对象拒绝；PS5/PS7 摘要一致、代次与锁保持；失败构建不更新成功身份；最终 MSI/安装产物必须与本次收据匹配。 |

## 已有证据与待验矩阵

**安装版新失败（已定位，修复中）**：终端启动提示符可见，执行中文输出命令后，辅助功能文本包含输出，但原生整窗只见提示符和大片空白，手动上滚未能读到结果。PTY-01 尚未通过；不能用此前源码页面成功或安装版 HTTP 成功覆盖此失败。证据：[初始终端](../navigation-update-followup/installed-0.2.18/09-terminal-running.png)、[输出可视失败](../navigation-update-followup/installed-0.2.18/10-terminal-output.png)、[上滚现场](../navigation-update-followup/installed-0.2.18/10-terminal-output-top.png)。实际原始输出确认清屏填充被追加为约 23 行空历史，且空轮询仍强制滚底；已按 [主会话与 Pro 窄修审查](../../analysis/2026-09-27-installed-terminal-pro-review.md) 开始修复，完成后另出版本复验，不追改本候选失败身份。

**另一入口失败**：工程选择器已切到隔离 Rust 工程，但项目文件树只有内部面板/URL 初始化能力，没有普通用户入口，LSP-01 因而受阻。已按同一补审在工程下拉新增“浏览当前工程文件…”源码入口，仍待新包验证。此前点击当前工程后的 disabled 已自行恢复，是完整同路径刷新期间的等待，不记为持久锁死；另做当前可信快捷项 no-op 以避免无意义重载。输入框、历史工程和重新加载行为不变。

原生已目视确认的范围还包括：更多展开、更新主动检查得到“尚无正式发布版本”、SKILL/插件本地空目录真实状态、文本附件的中文/emoji/行号、本地 URL 及普通公共 HTTPS 页面在右栏加载。外链图见 [Example Domain 实际显示](../navigation-update-followup/installed-0.2.18/15-https-example.png)，不泛化为所有需登录或禁止嵌入的站点均通过。

**低高度原生失败**：Windows 贴靠将真实窗口缩至 903×551，右栏折叠后原生网页仍残留在右下，覆盖聊天输入与状态栏，见 [遮挡截图](../navigation-update-followup/installed-0.2.18/16-low-height-browser-occlusion.png)。只读源码已确认隐藏宿主仍有矩形，而 native browser 的显隐判断未检查祖先 visibility 和折叠属性；没有事件日志，不断言该子视图是从未销毁还是销毁后被重建。普通 HTTPS 加载通过不能覆盖此响应式失败，正在窄修显隐同步。

- [具体行为变更、失败记录及边界](../navigation-update-followup/change-report.md)。
- [0.2.18 原生逐项实操](../navigation-update-followup/installed-0.2.18/native-acceptance.md)：含三项失败、实际动作边界和完整窗口截图。
- [0.2.18 安装版模型与聊天](installed-model-chat-acceptance.md)：Qwen 文字/已知原图/右栏图片通过；本地仅思考夹具的临时过程、终态隐藏、轨迹保留及切房间隔离通过，主会话已目视复核截图。未捕获百炼 wire，不将协议夹具当云能力。
- [终端源码修复验证](../navigation-update-followup/terminal-projection-fix.md)与[原生网页显隐源码检查](../navigation-update-followup/native-browser-panel-visibility-check.md)：均不是新安装版复验，不能覆盖上述失败。
- [候选安装实操清单](../navigation-update-followup/candidate-install-acceptance.md)：INSTALL/NAV/UPD/UI/TARGET/PTY/LSP/JOB/DATA/MODEL 的操作、预期和不能替代的证据。
- [安装版四项隔离实操](../navigation-update-followup/installed-0.2.18-isolated-e2e.md)：4 次/4 通过/0 原始失败，含实际 Web/CLI 身份；[机读结果](../navigation-update-followup/installed-0.2.18-e2e-sanitized.json)。受控模型用于验证协议和执行，不等于 Qwen 云端能力。
- [打包前组合源码验证](../navigation-update-followup/final-combined-source-verification.md)：按实际开发二进制分别归档，不能替代安装版结果。
- [日期回读打包回归](../navigation-update-followup/package-date-readback-regression.md)：保留首次打包失败及修复后结果，跳过项不算通过。
- [四项任务状态](../../analysis/2026-09-27-four-task-status-and-navigation-addendum.md) 与 [出包前 Pro 审查](../../analysis/2026-09-27-candidate-prepackage-pro-review.md)。最终总体审查仍待原生证据。

原生实操结果应另附整窗截图、动作与实际结果、当次包身份；失败及复验均保留。新视觉方案需用户选择，真实微信缺联调条件，Paint 待本人复核，正式签名发行、完整自动更新、远端插件市场及 MCP 网络 transport 均不得写成此次已通过。

## 安装后的定向代码复核

Sol Max 只读核对了更新来源/状态/版本比较、接纳快照、子 Agent 工具和权限交集/取消/预算、多目标派发、Goal claim 与工程固定、用量 run 关联、MCP 工作区与连接代际/审批重验/stdio。未发现足以证明当前候选实际失败的确定缺陷；这不是所有模块无缺陷的保证，也没有借此重复编译或追加无关测试。主会话另核对了导航/发送目标和更新前端差异，以及安装版视觉与 MCP 的实际结果。

明确保留的兼容边界：旧 NULL run 用量只凭确定 turn/call 证据关联，未知历史继续不关联；无 HostAgentRunner 的遗留 Agent 直调明确拒绝；更新依赖 GitHub 可达；本地受控模型与官方 MCP echo 不能替代所有云供应商及第三方 MCP 服务的实测。
