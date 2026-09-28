# 0.2.24 安装版原生验收阶段报告

日期：2026-09-28
测试执行模型：GPT-5.6 Luna max
测试窗口：正式安装入口启动的 Tauri 原生窗口；WebView 文档地址为 `http://127.0.0.1:8765/`。
测试模型：已保存的真实 `qwen3.8-flash`；未创建或使用模型夹具。

## 安装与身份

- MSI：`C:\Users\zhupu\Desktop\coolzhuagent\dist\CoolzhuAgent-0.2.24.msi`
- MSI SHA-256：`64D845A18BC07F3C4F0CE974C1164C629D588460AEA439D9BBB20DCED3CAAF8A`
- 包根：`tmp/package-0.2.24-versioned`
- 包报告：`tmp/package-0.2.24-versioned-report.json`
- 源码快照：`28fa27f63f229725e5df5dcd44daa2c1db8c489b403d02d078da4a30ff26ae38`
- 安装后卸载注册表显示 `COOLZHU CODE Agent`、版本 `0.2.24`。
- 正式启动入口 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe` 成功拉起 Tauri 壳和 Web 控制台。
- 安装器通过 Windows 图形安装流程完成；本轮没有捕获 `msiexec` 命令行退出码，不能据此填写数值退出码。注册表版本、正式入口启动和安装后产物核对均成功。
- `tmp/installed-0.2.24-artifact-identity.json`：10 个发布产物，匹配 10，失配 0，缺失 0。

## 升级前后数据

- 升级前快照：`tmp/upgrade-preflight-20260928.json`（2523 个文件条目；内容不在本报告展开）。
- 升级后比较：`tmp/upgrade-data-compare-20260928.json`。
- 缺失文件 0（文件层面）；变化 6 项，均为启动、会话、日志或浏览器桥接 nonce：`web-sessions.json`、`web-sessions.sqlite3`、`input-safety\input-safety.sqlite3`、两个 package-launcher 日志、`runtime\browser-bridge-nonce`。该比较不据此推断数据库语义未变。

## 快捷栏与右栏阶段结果

S01-S06、S09-S10、S15 在新建的空白受控聊天室“聊天 08”内完成留证；S08、S12、S13、S14 使用本轮新建或重建的受控聊天室“聊天 09”，其中 S08/S12 画面仅保留本轮停用插件对照消息。截图背景没有历史消息或私聊内容。模型相关操作均使用已保存的真实 `qwen3.8-flash`。

| 编号 | 实际动作与观察 | 结果 | 新版截图 |
|---|---|---|---|
| S01 | 在新建空白聊天室“聊天 08”以 qwen 目标写入未发送草稿，打开“设置”，再返回聊天室；返回后草稿和 qwen 目标仍在，随后清除了测试草稿。 | 通过 | `installed-native/s01-settings.png`；`installed-native/s01-return-draft.png` |
| S02 | 打开“选择发送对象”；弹窗只显示复选框。实际从 qwen 单选勾到 qwen+agnes-text 多选，再恢复 qwen 单选；未发送消息，草稿在整个过程中保留。 | 通过：复选框结构、单选/多选和未发送草稿保护均已实操验证。 | `installed-native/s02-checkbox-single.png`；`installed-native/s02-checkbox-multi.png`；`installed-native/s02-checkbox-restored.png` |
| S03 | 切换到隔离工程 `tmp\native-acceptance-fixtures-20260928\project`，页面保持空白且无 `Failed to fetch`；打开聊天室选择，列表显示 2 个会话并选择“主聊天室”；随后切回 `C:\Users\zhupu\coolzhuagent` 的聊天08。 | 通过：工程和聊天室切换均完成，原工程、聊天08、qwen 目标已恢复。 | `installed-native/s03-project-dialog.png`；`installed-native/s03-project-switched.png`；`installed-native/s03-room-dialog.png` |
| S04 | 重新打开设置，读取 qwen 配置；面板内容只包含“模型与会话”连接、上下文、思考和采样配置，未出现“工具与审批”、TTS 或 STT；状态为“配置已载入”。此前已在不改 Base URL/API Key 的前提下保存温度 0.2 并关闭重开读回 0.2，随后恢复 0 并保存。本批补充把模型配置临时改为 0.3 但不保存，离开设置再打开仍显示 0.3 和“有未保存的更改”，最后改回 0 并保存。 | 通过：配置读取、可恢复采样参数保存/重读/恢复和未保存模型配置保护均已实操。`s04-parameter-reread-field.png` 是最终恢复后的字段读回，显示值为 **0**，不能用它证明 0.2；0.2 的重开读回来自此前 AX 操作记录。 | `installed-native/s04-settings-loaded.png`；`installed-native/s04-parameter-edited.png`；`installed-native/s04-parameter-saved.png`；`installed-native/s04-parameter-reread.png`；`installed-native/s04-parameter-restored.png`；`installed-native/s04-parameter-reread-field.png`；`installed-native/s04-model-config-dirty.png`；`installed-native/s04-model-config-dirty-reread.png`；`installed-native/s04-model-config-restored.png` |
| S05 | 沿用已保存的百炼地址、协议和密钥，保持“此次查询不使用密钥”未选中，点击“获取模型”。 | 通过：真实请求返回 261 个模型，发现项为 `qwen3.8-flash — 支持图片输入`；未出现“获取失败”或 `Failed to fetch`。 | `installed-native/s05-before-discovery.png`；`installed-native/s05-model-discovery.png` |
| S06 | 本批沿用当前原工程、真实 `qwen3.8-flash` 和空白受控房间：提交受控图片 `tmp/native-acceptance-fixtures-20260928/image/qwen-vision-shapes.png`，要求识别橙色圆形和蓝色三角形；随后让 qwen 调用 `write_file`，在隔离目录 `tmp/qwen-tool-acceptance-20260928/result.json` 写入固定 JSON；最后打开统计信息并刷新。 | 通过本批 S06 图片/工具/统计子项：qwen 实际回复识别出 3 个橙色圆形和 1 个蓝色三角形；AX 中出现真实工具执行结果，且磁盘核验文件长度 48 字节、内容为 `{"source":"real-qwen-tool-acceptance","ok":true}`，证明不是模型口头声称完成；统计显示已知输入 12,117、已知输出 389、3 次请求、失败或中断 0、进行中 0。供应商未提供的用量字段仍按界面显示为未知/不完整。 | `installed-native/s06-image-reply.png`；`installed-native/s06-tool-reply.jpg`；`installed-native/s06-statistics.jpg` |
| S08 | 点击左侧“微信连接”入口，仅读取原生状态；界面显示连接已停用、绑定/接收/待发送/死信计数和能力 `text, image`。点击“刷新状态”后，辅助服务仍显示 `sidecar 不可用`，错误为访问 `http://127.0.0.1:8787/health` 失败，provider 为 `—`；未扫码登录、未修改绑定表单、未向外发送消息，最后关闭面板。 | 通过（范围限定：入口、状态读取和错误收尾均完成）；sidecar 不可用是当前安装版实际保留的健康错误，刷新后未自行伪称恢复。 | `installed-native/s08-wechat-status.png`；`installed-native/s08-wechat-sidecar-health.png`；`installed-native/s08-wechat-status-refresh.png` |
| S09 | 在原生右栏打开本地受控 `http://127.0.0.1:18765/complex.html`；刷新后确认动态 DOM 文本、canvas 和重绘按钮，输入 `S09-状态保留-20260928`，点击重绘并提交本地表单。 | 通过页面显示与脚本执行：AX 树确认“动态 DOM 与 canvas 已初始化”和“本地表单已拦截提交”，页面始终在原生右栏，没有跳转外部浏览器。 | `installed-native/s09-complex-form-canvas.png` |
| S10 | 在浏览器右栏打开顶栏“切换聊天室”弹层后关闭；点击浏览器工具头部“返回”回到右栏协作视图，再从左侧“更多→浏览器”恢复；窗口从 1443×897 放大到 1707×912 再还原；随后在右栏打开真实外站 W3C 首页和 WAI 页面，用后退/前进返回并恢复页面。S10 的表单值 `S10-真实键盘输入-20260928` 是为本场景主动重新输入的测试值。 | 部分通过：顶栏弹层避让、窗口缩放/还原、真实外站在右栏显示及 W3C 首页↔WAI 前进后退均成功，窗口列表未出现因该操作新开的外部浏览器。点击浏览器工具“返回”后，再经“更多→浏览器”恢复，iframe 初始为空；重新点击“打开”导航受控页后 DOM/canvas 可恢复，但此前 S10 表单值未保留，字段为空；该状态丢失已按失败记录。 | `installed-native/s10-topbar-room-modal.png`；`installed-native/s10-topbar-closed-state.png`；`installed-native/s10-right-rail-collapsed.png`；`installed-native/s10-right-rail-recovered.png`；`installed-native/s10-right-rail-recovered-field-reset.png`；`installed-native/s10-window-expanded.png`；`installed-native/s10-window-restored.png`；`installed-native/s10-real-external-w3c.png`；`installed-native/s10-real-external-wai.png`；`installed-native/s10-real-external-back.png`；`installed-native/s10-real-external-forward.png` |
| S12 | 从左侧“更多→终端”打开原生 PowerShell 终端，运行无害命令输出中文标记、当前工程目录和前 5 个目录名；AX 与画面均显示 cwd 为 `C:\Users\zhupu\coolzhuagent`，中文输出可读。随后点击“中断”并“关闭”，终端状态变为“终端已关闭”。未读取凭据、未写文件。 | 通过（当前工程 cwd、中文输出和关闭收尾均实操验证）。 | `installed-native/s12-terminal-cwd-chinese.png`；`installed-native/s12-terminal-closed.png` |
| S13 | 隔离验收工程的 SKILL 页面显示 1 个 SKILL，但该工程默认是 `glm-4.6` 且未配置 qwen，未改其配置或拿其它模型代替；在原工程确认真实 qwen 后，仅新建 `C:\Users\zhupu\coolzhuagent\.coolzhu\skills\native-acceptance-20260928\SKILL.md`，通过右栏 SKILL 列表查看内容并选用。新建受控房“聊天 09”发送“请说明当前可接收的输入类型”，真实 qwen 回复末行包含唯一标记“技能验收：竹影九二八”。停用技能后，因系统最多保留 8 个聊天室，先在 UI 确认删除本轮创建且仅含 3 条测试消息的原“聊天 09”，再新建对照房（显示名仍为“聊天 09”；当前 API 房间 ID 为 `room-1790566542003`，旧房已删除，非凭显示名假设为同一房间），同问的真实 qwen 回复未出现标记。 | 通过（范围限定：原工程专用测试 SKILL；隔离工程未被改动）。选用与停用均通过原生 UI，启用/停用对照来自不同的房间生命周期，未复制凭据或修改 Base URL/API Key。 | `installed-native/s13-skill-list-before.png`；`installed-native/s13-skill-detail.png`；`installed-native/s13-skill-selected.png`；`installed-native/s13-skill-enabled-reply.png`；`installed-native/s13-skill-disabled.png`；`installed-native/s13-skill-disabled-reply.png` |
| S14 | 在原工程新建唯一受控插件目录 `C:\Users\zhupu\coolzhuagent\.coolzhu\plugins\qwen-stdin-echo`，未复制凭据或改现有插件；原 fixture 只有 `manifest.json`，安装版扫描要求 `plugin.json`，因此在该测试目录增加同内容的 `plugin.json` 兼容副本。插件市场显示本地候选后，原生安装使已安装数由 3 变 4，再启用并确认状态为“工具已接入”。在新建完全访问房以真实 qwen 发送仅调用 `plugin__qwen-echo-json` 的请求，回复含 `runtime-executed`、`allow-auto` 和实际 stdin 回显 `qwen-stdin-echo@external / qwen-echo-json / plugin-s14-0928`；运行轨迹显示完成的 `plugin__qwen-echo-json` 轮次。随后通过原生删除确认框清理该启用调用房（确认框显示仅本轮 5 条消息和 1 条衍生记忆），再新建独立的“聊天 09”对照房；停用插件后发送同类请求，真实 qwen 明确说明工具不在当前请求列表，运行轨迹仅有模型会话，没有插件工具调用或回显。最后原生卸载，市场回到 3 个已安装且该目录回到“本地候选·安装”。 | 通过（范围限定：原工程本地受控插件，真实 qwen、真实 stdin 执行和停用拒绝均已留证；未改产品源码、Base URL、API Key 或既有插件状态）。 | `installed-native/s14-plugin-candidate.png`；`installed-native/s14-plugin-installed.png`；`installed-native/s14-plugin-enabled.png`；`installed-native/s14-plugin-qwen-call.png`；`installed-native/s14-plugin-trace.png`；`installed-native/s14-plugin-disabled.png`；`installed-native/s14-plugin-disabled-reply.png`；`installed-native/s14-plugin-disabled-trace.png`；`installed-native/s14-plugin-uninstalled.png`；`installed-native/s14-plugin-uninstalled-candidate.png` |
| S15 | 在空白受控聊天室打开升级更新，点击“检查更新”，等待结果。 | 通过：约 6.5 秒内退出检查状态，显示“尚无正式发布版本 / 官方仓库目前没有正式稳定版发布”，按钮恢复可操作；未卡在“正在检查正式发布版本中”，无 `Failed to fetch`。 | `installed-native/s15-update-before.png`；`installed-native/s15-update-result.png` |

## 第二批受控截图清单

截图为原生 Tauri 窗口画面，默认尺寸为 1443×897；S10 放大证据为 1707×912。S01-S06、S09-S10、S15 背景为新建空白聊天室“聊天 08”，S13 背景为本轮新建的受控聊天室“聊天 09”；文件已在仓库内保存：

- S01：`C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.24\installed-native\s01-settings.png`、`s01-return-draft.png`
- S02：`s02-checkbox-single.png`、`s02-checkbox-multi.png`、`s02-checkbox-restored.png`
- S03：`s03-project-dialog.png`、`s03-project-switched.png`、`s03-room-dialog.png`
- S04：`s04-settings-loaded.png`、`s04-parameter-edited.png`、`s04-parameter-saved.png`、`s04-parameter-reread.png`、`s04-parameter-restored.png`、`s04-parameter-reread-field.png`、`s04-model-config-dirty.png`、`s04-model-config-dirty-reread.png`、`s04-model-config-restored.png`
- S05：`s05-before-discovery.png`、`s05-model-discovery.png`
- S06：`s06-image-reply.png`、`s06-tool-reply.jpg`、`s06-statistics.jpg`
- S08：`s08-wechat-status.png`、`s08-wechat-sidecar-health.png`、`s08-wechat-status-refresh.png`
- S09：`s09-complex-form-canvas.png`
- S10：`s10-topbar-room-modal.png`、`s10-topbar-closed-state.png`、`s10-right-rail-collapsed.png`、`s10-right-rail-recovered.png`、`s10-right-rail-recovered-field-reset.png`、`s10-window-expanded.png`、`s10-window-restored.png`、`s10-real-external-w3c.png`、`s10-real-external-wai.png`、`s10-real-external-back.png`、`s10-real-external-forward.png`
- S12：`s12-terminal-cwd-chinese.png`、`s12-terminal-closed.png`
- S13：`s13-skill-list-before.png`、`s13-skill-detail.png`、`s13-skill-selected.png`、`s13-skill-enabled-reply.png`、`s13-skill-disabled.png`、`s13-skill-disabled-reply.png`
- S13 恢复核对：`s13-skill-restored-empty.png`
- S14：`s14-plugin-candidate.png`、`s14-plugin-installed.png`、`s14-plugin-enabled.png`、`s14-plugin-qwen-call.png`、`s14-plugin-trace.png`、`s14-plugin-disabled.png`、`s14-plugin-disabled-reply.png`、`s14-plugin-disabled-trace.png`、`s14-plugin-uninstalled.png`、`s14-plugin-uninstalled-candidate.png`
- S15：`s15-update-before.png`、`s15-update-result.png`

## 补充浏览器观察

使用 `tmp/native-acceptance-fixtures-20260928/site` 提供的本地受控页面，在原生内置浏览器中打开 `http://127.0.0.1:18765/index.html`，验证了页面脚本、动态内容、同窗口第二页、后退、刷新、停止；S09/S10 另对 `complex.html` 留存了 canvas、表单、右栏折叠恢复和窗口缩放截图。另在右栏打开真实外站 `https://www.w3.org/` 与 `https://www.w3.org/WAI/`，确认页面脚本和复杂内容在右栏显示，后退回首页、前进回 WAI 均成功；窗口列表未出现由该操作新开的外部浏览器。S10 右栏重建后的受控表单值未保留，详见 S10 失败记录。

## 证据与限制

- 第二批已在空白受控聊天室保存原生截图，未包含历史聊天消息。此前第一批针对历史聊天室的操作主要以 AX 树和可见控件记录；本报告只把第二批截图列为新版截图证据。
- 目录 `docs/testing/release-0.2.24/diagnostic-0.2.23/` 下的 `qwen-reply.jpg`、`qwen-statistics.jpg` 是清理自嵌套浏览器后的 **0.2.23 初始诊断**，不是本轮 0.2.24 的通过证据：
  - `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.24\diagnostic-0.2.23\qwen-reply.jpg`
  - `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.24\diagnostic-0.2.23\qwen-statistics.jpg`
- 曾有一次 `sky.set_value` 调用误传参数名 `element`，工具立即返回参数错误；正确参数为 `element_index`。这属于测试工具使用错误，不是产品失败，也未改变界面。
- S06 工具调用产物：`C:\Users\zhupu\Desktop\coolzhuagent\tmp\qwen-tool-acceptance-20260928\result.json`；内容为隔离验收用固定 JSON，未写入用户工程数据或凭据。
- S13 SKILL 测试素材：`C:\Users\zhupu\coolzhuagent\.coolzhu\skills\native-acceptance-20260928\SKILL.md`；这是本轮在原工程新建的专用验收目录，隔离工程因默认 glm 且未配置 qwen 未被改动。启用房删除前的确认框显示仅 3 条本轮测试消息；停用后的新房当前 API ID 为 `room-1790566542003`，两次同名“聊天 09”不能视为同一房间。
- S13 收尾核对：原工程 SKILL 目录当前显示 `qwen-input-contract · 未选用`，截图 `s13-skill-restored-empty.png`；本轮初始选用状态已恢复为空。
- S14 插件测试素材：`C:\Users\zhupu\coolzhuagent\.coolzhu\plugins\qwen-stdin-echo`；这是本轮在原工程新建的唯一受控目录，包含 `manifest.json`、安装版要求的 `plugin.json` 以及 stdin 回显脚本。安装、启用、真实 qwen 工具调用、运行轨迹、停用拒绝和卸载均通过原生 UI 完成；卸载后市场回到 3 个已安装，目录保留为本地候选，不影响既有插件。
- S08 微信仅做入口、状态读取、刷新和关闭；截图中没有联系人 ID 或私聊消息，不代表 sidecar 已恢复。
- 本报告不覆盖 S07、S11、S16；S08/S09/S10/S12/S13/S14 已按本批实际截图记录，其中 S10 的右栏重建后表单状态丢失保留为失败项。S06 图片识别、真实 `write_file` 工具执行和统计刷新、S13 的真实 qwen SKILL 注入/停用对照，以及 S14 的真实插件 stdin 执行/停用拒绝，均已留图。统计中供应商未提供的字段仍显示未知/不完整，不据此推算账单。
