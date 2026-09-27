# 后续 PR 范围与证据口径审计

日期：2026-09-27。基线为 `origin/main` 的 `9eae3ae`（已合并 PR #67），工作分支为 `codex/navigation-update-followup`。本审计只核对工作树、正式报告和构建收据；未暂存、提交、推送或操作原生窗口。下方先保留 0.2.20 时点的审计事实，末节补入 0.2.21 已构建后的候选范围；正式暂存前仍须再取一次文件清单。

## 应纳入的实现文件

本次检查开始时有 45 个已跟踪文件修改；不要只暂存 `git diff`，否则会遗漏下面 **8 个未跟踪的核心生产文件**：

| 新文件 | 对应实现 |
| --- | --- |
| `modules/gui-web/packages/web-console/src/app_update.rs` | 更新检查与状态 |
| `modules/gui-web/packages/web-console/src/host_child_agent.rs` | 宿主子 Agent |
| `modules/gui-web/packages/web-console/src/lsp_host.rs` | LSP 房间授权、运行归属与生命周期 |
| `modules/gui-web/packages/web-console/src/mcp_default_servers.json` | MCP 默认服务配置 |
| `modules/gui-web/packages/web-console/src/mcp_host.rs` | MCP 宿主及权限接线 |
| `modules/gui-web/packages/web-console/src/terminal_host.rs` | 受控终端宿主 |
| `modules/gui-web/packages/windows-process-guard/src/conpty.rs` | Windows ConPTY 底座 |
| `modules/tooling/packages/plugin-system/src/install_transaction.rs` | 插件安装事务 |

视觉实施期间又出现两项未跟踪的产品素材：`modules/gui-desktop/packages/tauri-shell/ui/assets/p83-coolzhu/actors/swordsman-four-poses-v2.png`、`modules/gui-web/packages/web-console/assets/ui-redesign/scroll-frame/scroll-frame-v1.png`。它们若被最终源码引用，也必须与新视觉源码一并纳入及重新出包；本审计写入时视觉实现仍在变化，不将这两张素材追认到 0.2.20。

已跟踪的实现须连同依赖一并纳入：`Cargo.lock`；core-runtime 的 `mcp_stdio.rs`/`lib.rs`；language-service 的 `Cargo.toml`、`client.rs`、`lib.rs`、`manager.rs`；web-console 的 `Cargo.toml`、`index.html`、`app.js`、`main.rs`、`chat_experience.js`、`model_settings.js`、`native_browser_panel.js`、`scheduled_jobs.rs`、`multimodal_input.rs`、`request_usage.rs`、`chat_insights.rs`、`chat_run_admission.rs`、`chat_tool_history.rs`、`computer_use_store.rs`、`tool_dispatch_settlement.rs`、`tool_loop_coordinator.rs` 及相关布局样式；windows-process-guard、command-router、plugin-system、tool-registry 的对应已改文件。尤其 `path_effect.rs` 的不透明命令权限修复不能因只关注终端而遗漏。

打包身份治理还依赖已改的 `scripts/build-msi.ps1`、`scripts/package-all.ps1`、`scripts/package-report-retention.ps1`、`scripts/lib/build-identity.ps1`、`scripts/lib/webview2-loader.ps1` 和两份对应测试脚本。新增视觉源码涉及 `modules/gui-web/packages/web-console/src/scroll_theme.css`、`modules/gui-desktop/packages/tauri-shell/ui/scroll-startup-player.js`；其素材与设计决定在 `docs/design/2026-09-27-ui-concepts/`，当前由视觉实现者继续核对，不能用 0.2.20 包验收追认后来修改。

## 应纳入的正式说明与证据

- 进度入口 `docs/testing/navigation-update-followup/change-report.md`，以及 LSP 房间授权、工程写入与前端 scope、终端、MCP/Goal/CLI、更多/更新的对应验收记录和**经过挑选的**截图/脱敏 JSON。新增分析、工作日志可按相应实现一起纳入，避免代码没有设计边界或验证出处。
- `docs/testing/release-0.2.19/functional-verification-and-test-design.md` 与 `docs/testing/navigation-update-followup/installed-0.2.19/native-acceptance-incremental.md` 保留 0.2.19 成功、失败和未测事实；不能把这批图片改名为 0.2.20。
- `docs/testing/release-0.2.20/functional-verification-and-test-design.md`、`installed-upgrade-and-static-data.md` 与 `evidence/build-identity/` 的正式归档保留 0.2.20 构建、安装和静止数据事实。原生截图应由实际操作者补写包身份和时序后才作为 0.2.20 行为证据。`installed-upgrade-and-static-data.md` 写于物理 Escape 停止时；其“当时尚无截图”不能当作之后仍无截图的最新结论。
- 0.2.20 发布快照为 `61cb4d8c5f6f6105fb5c60f23a4fbe66bdab4b0a6ce1963ab5b7c439ed84d631`，MSI SHA-256 为 `3BA0D9B0893123FEF9A4694BD567399029A74BE55275D8040212BDCE180F6542`，安装 Web 为 `6AD5CB4B595A3BBF582D3BEECACC964718A2648C344BA769695864BC7BA6861F`。源码/隔离 HTTP、0.2.19 原生和 0.2.20 静止安装是三层不同证据。

## 不得误纳入的内容与隐私边界

- 根目录 `.playwright-cli/` 当前未忽略且有页面快照/截图，**不得用 `git add -A` 顺带提交**；该目录是过程产物，不是正式挑选的证据。`tmp/`、`dist/`、`package/` 已被忽略，内含真实升级备份、会话库、摘要密钥、安装器和构建日志，均不应强制加入 PR。原用户 `.coolzhu` 运行数据也不属于实现或验收夹具。
- 对 `docs/analysis`、`docs/testing`、`docs/work-logs`、`docs/design` 的文本执行高置信凭据形态只读扫描，未找到私钥头、Bearer、URL token 或长凭据赋值；产品源码命中的两处测试样例均为未改的基线文件。正常本机路径、报告哈希和已标注夹具值本身不是密钥。截图和逐事件记录的独立复核结论见下节；不能据此整目录暂存。
- 已归档的 build-identity JSON 是可核对的包报告；忽略目录中的原始 `pre/post` 快照、数据库、附件和完整运行日志不能因为报告引用了本地路径就进入提交。

## 0.2.20 时点的剩余验收条件

1. **视觉源码与 0.2.20 身份已经分叉。** 在 0.2.20 构建后首次逐文件重算 1216 个快照文件时，`scroll_theme.css` 与 `scroll-startup-player.js` 已变化，其他当时快照内文件未变；后续素材仍在编辑。若本 PR 纳入视觉，应以新源码重新构建并给新包/原生证据，不能把 0.2.20 安装成功当作它的通过结果。
2. **0.2.20 的 LSP 原生复验已发现确定失败。** 既有隔离 HTTP 正反例、Web 串行全量测试和 10/10 安装身份通过，但原生窗口在切到隔离工程后启动 LSP 返回 409。前端把编码并截断的作用域 key 当成真实工程路径发送；后端拒绝是正确保护。后续窄修须进入新包，再从正常界面验证启动、诊断、定义、引用、停止与撤权/切房，不能用 0.2.20 静止安装或旧包 403 截图宣称通过。
3. **Paint 真实画线仍被已有输入安全隔离阻断。** 旧任务缺独立历史安全证据，须由本人走正式复核；本轮没有执行物理输入，不能在 PR 中写为绘图通过。真实微信 provider、混合 DPI、远端插件安装、非 stdio MCP 和完整自动安装更新也只可按各自已交付边界说明，不合并判作完成。
4. **提交前的证据卫生**：排除 `.playwright-cli/` 和所有私有运行态；核对新视觉文件及用户工作区截图；更新入口报告的最新版本/截图时序。完成这些前，当前工作树不适合直接整体暂存。

这份审计不阻止创建明确标注未覆盖范围的代码评审，但不能以测试总数、构建成功或静止数据一致宣称四项用户目标全部完成。

## 提交证据卫生复核

六份本轮新增的 Pro/设计审查文档已按原始字节备份到忽略目录 `tmp/pre-pr-private-provenance/`；公开副本只移除了 7 个私人 ChatGPT 会话跳转地址，保留会话标题、日期、审查内容和必要轮次标识。备份哈希与原件复制时一致。未处理的更早基线文档不在本轮变更清单内。公开文档中的私人原图链接已改为“私有现场原图留在本机，未纳入 PR”的文字说明，原图没有修改或删除。

对原定 72 张待审安装版截图逐张目视核对：46 张只有受控验收聊天、测试夹具、产品页面或正常本机路径，且被正式报告引用，列为 `include_test_evidence`；11 张显示原用户既有聊天正文、配置同框，或微信账户绑定状态，列为 `exclude_private`；15 张没有明显私人内容，但正式报告未引用，列为 `exclude_unreferenced`。审查期间新增的 0.2.20 两图另作单独核对：隔离 Rust 文件初始画面未被报告引用，排除；LSP 409 图为报告明确记录的**失败**画面，可纳入失败证据，绝不可称作修复通过。逐文件判定在忽略目录 `tmp/pre-pr-image-review.json`；候选路径在 `tmp/pre-pr-candidate-list.md`。这些文件均尚未暂存。

两份逐事件记录 `evidence-history/results-v5.json` 与 `s25-cli-agent-host-events.jsonl` 已对照其隔离 HTTP / 本地假模型报告核实：分别只记录合成历史量测和 CLI 子 Agent 夹具事件；字段与全部字符串检查未见凭据字段、私人会话 URL、外部地址或用户目录路径，可列测试证据。未把它们当作安装版行为证明。

未采用的 C/D 界面概念图和被 V2 替代的 V1 人物图保留为**设计探索归档**，不进入产品资源包；三图分别约 1.5、1.4、1.2 MB。`README.md` 与素材提示词仍引用这些原图，所检查的设计文档相对图片链接均有对应文件。这是 0.2.20 时点记录；设计图不证明运行效果。

## 0.2.21 构建后候选范围刷新

0.2.21 已用完整 release 流程构建，源码快照前后同为 `beda1d27d98d4a4f02d1588094afc887ac848885a8ad30095578eb005e377eaa`，六项发布门通过，MSI SHA-256 为 `00C98A6FA78CA4A1BCAAE93FAD6AE9E4B95E46D2BDEAC39D896CD6EF181D8AAC`。公开[构建身份收据](../release-0.2.21/evidence/build-identity/README.md)与安全报告已归档；MSI 本体仍在被忽略的 `dist/`，不作为 PR 文件。构建成功不代表 0.2.21 已安装或原生功能通过。

当前精确候选清单在本机忽略目录 `tmp/pre-pr-candidate-list.md` 与 `tmp/pre-pr-candidate-manifest.json`，由 Git 修改/未跟踪路径重新生成，尚未执行暂存。清单按决策分组：生产实现及构建脚本 55 项，正式文档/收据 105 项，已选视觉来源设计图 3 项，未采用探索图 3 项，受控测试图片/连续录屏/事件证据 131 项；另有原用户历史界面原图 12 项、未被报告引用过程图 14 项、`.playwright-cli` 过程文件 32 项明确排除。0.2.20 安装报告已按物理 Escape 中止与用户再次授权的时序补齐，列为正式报告候选。

生产实现清单已明确包含 8 个未跟踪 Rust/JSON 核心模块、V2 人物与卷框两份新增产品素材、桌面播放器/manifest、Web `scroll_theme.css`，以及 `app.js` 中 LSP 从 `projectRequestScope().workspace` 提交真实路径的窄修。视觉[源码实操报告与安全媒体](visual-b-scroll-v2/report.md)作为**源码版**证据纳入；连续 WebM、五张稳定页面图和定点启动帧均带原始阶段说明，不冒充安装版播放。`docs/testing/release-0.2.21/functional-verification-and-test-design.md` 与构建身份收据纳入；0.2.20 原生 LSP 409 保留为前版失败，不可改称 0.2.21 通过。

对本轮新增公开 README、视觉报告及新入口文档的 Markdown 相对链接做了只读解析：42 个文件链接、4 个候选目录链接全部存在且指向清单中的候选内容；检查结果在忽略目录 `tmp/pre-pr-link-audit.json`。公开文本无本轮私人 ChatGPT 会话跳转链接命中。原用户工程的 0.2.20 首屏虽然被后续报告提及，但原图仍留本机，报告已改为不可点击的私有证据说明；隔离 Rust 工程的手动启动入口图和受控视频文件选择框图因报告新增引用才纳入。系统对话框图片仅证明当时界面，工具拒绝事实来自原始回执。其它原用户既有聊天或微信账户同框截图继续排除。

新增的 `docs/analysis/2026-09-27-s0-controlled-replay-review.md` 作为受控回放**范围与测试设计**纳入。其六类运行原型仍由另一操作者在 `tmp/` 执行，旧 manifest 的非流式 `list_directory` 不属于当前真实聊天工具，故原样失败不得改称已完成回放；随后 `glob_search` 新受控录制也应按最终报告独立审阅，不凭设计文档宣称六类全通过。

截至此节，需等待 0.2.21 正常升级与原生增量验收，尤其真实 LSP 启动/诊断/跳转/停止、B 启动交接、903×551 卷框与原生浏览器边界、用户数据保留。Paint 仍因既有输入安全隔离未实际画线；真实微信 provider、远端市场安装、非 stdio MCP、混合 DPI 和自动下载安装更新各有未验证范围。PR 草稿只陈述最终实现和对应证据，不宣称四项任务全部完成。
