# 0.2.25 原生安装与浏览器验收记录

- 验收日期：2026-09-28
- 验收对象：安装版 COOLZHU AGENT 0.2.25
- 原生页面：`http://127.0.0.1:8765/`
- 本批范围：安装身份核验、浏览器导航状态 NAV02402
- 模型约束：启动时已显示真实 `qwen(qwen3.8-flash)`；本批浏览器操作未发送模型请求，也未使用模型夹具。

## 安装与产物身份

- MSI：`C:\Users\zhupu\Desktop\coolzhuagent\dist\CoolzhuAgent-0.2.25.msi`
- MSI SHA-256：`3186569C4D2A277DCACCBEE96C00074ABDC8EC2E492E88D8B8A665C037741BFA`
- 安装日志：`C:\Users\zhupu\Desktop\coolzhuagent\tmp\msi-0.2.25-upgrade.log`
- 安装结果：日志记录 `msiexec` 退出码 0。
- 注册表身份：`COOLZHU CODE Agent`，版本 `0.2.25`，发布者 `Coolzhu`。
- 安装产物哈希核对：10/10 与候选包一致，记录见 `C:\Users\zhupu\Desktop\coolzhuagent\tmp\installed-0.2.25-artifact-hash-compare.json`。
- 候选包身份报告：`C:\Users\zhupu\Desktop\coolzhuagent\tmp\package-0.2.25-browser-followup-report.json`。

### 静止数据摘要的口径限制

升级前摘要 `C:\Users\zhupu\Desktop\coolzhuagent\tmp\upgrade-024-pre-025-static-summary.json` 的 `dataRoot` 是 `C:\Users\zhupu\coolzhuagent\.coolzhu`，文件数 61；安装后摘要 `C:\Users\zhupu\Desktop\coolzhuagent\tmp\upgrade-024-pre-025-post-install-summary.json` 的 `dataRoot` 是 `C:\Users\zhupu\Desktop\coolzhuagent\.coolzhu`，文件数 31。两次递归采样根目录不同，因此 61/31 不是同一数据根的前后配对，不能据此判定数据保留或丢失。本报告不作语义不变声明。

## NAV02402 浏览器验收

受控页面：`http://127.0.0.1:18765/complex.html`。页面包含动态 DOM、canvas 和输入字段“保留字段”。首次输入唯一标记 `NAV025-20260928-KEEP-7F3A`，后续聊天室切换场景使用 `NAV025-CHATSWITCH-KEEP-9B2`。

| 场景 | 实际结果 | 证据 |
|---|---|---|
| 打开复杂本地 HTML，填写表单 | 页面标题、动态 DOM、canvas 和非空字段均显示 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-complex-filled.png` |
| “返回”后通过“更多→浏览器”恢复 | 页面 DOM、canvas 和输入标记保留 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-return-more-retained.png` |
| 左侧“首页”后通过“更多→浏览器”恢复 | 输入标记保留 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-home-browser-retained.png` |
| 左侧“设置”后返回，再通过“更多→浏览器”恢复 | 输入标记保留 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-settings-browser-retained.png` |
| 顶栏聊天室弹层打开与关闭 | 弹层显示时网页未覆盖弹层；关闭后页面和输入标记保留 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-topbar-room-modal.png`；`C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-topbar-modal-closed-retained.png` |
| 窗口放大后恢复 | 放大、恢复后页面与输入标记保留 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-window-expanded-retained.png`；`C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-window-restored-retained-clean2.png` |
| 明确关闭浏览器工具后重新打开 | 实际预览清空为 `about:blank`，旧输入标记不存在；地址栏仍显示上次 URL 元数据 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-close-reopen-blank.png` |
| 切换聊天室 | 从 `聊天 09` 切到 `测试环境` 后，浏览器面板、页面 DOM 和输入标记均清除；未发送模型请求。随后已恢复活动聊天室 `聊天 09`，顶栏恢复真实 qwen。切换前受控页面证据见 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02402-chat-switch-before.png` |

切换聊天室后的原生画面含既有会话内容，未保存截图以避免写入私聊数据；清除结果依据原生无障碍树复核：`hasBrowserPanel=false`、`hasPageDom=false`、`hasMarker=false`。当前活动聊天室已恢复为 `聊天 09`，浏览器面板保持关闭。

## 结论

0.2.25 安装身份和 10 个产物哈希核验通过。NAV02402 中返回、首页、设置、聊天室弹层、窗口放大恢复均保留复杂页面状态；明确关闭后重新打开会清空实际预览。切换聊天室后的页面清理为 AX 观察结果，截图验收未完成。静止数据前后摘要因采样根目录不一致，仅作为安装记录，不能作为数据迁移结论。

## NAV02401 重定向与新窗口边界（本批）

本批使用独立受控服务 `127.0.0.1:18766`，服务进程 PID 为 `23508`，未触碰原有 `18765` 服务。所有操作均通过原生右侧内置浏览器完成，未发送模型请求。

| 场景 | 实际结果 | 证据 |
|---|---|---|
| A 页面 `/a` | 标题为“A 页面 · 重定向验收”，地址为 `/a`，加载完成；临时字段填写 `keep-a` | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-a.png`；`C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-a-keep.png` |
| `/back-to-a` | 302 后回到 A，最终地址 `/a`，加载完成；临时字段按真实导航重置 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-back-to-a.png` |
| `/to-c` | 302 后到 C，最终地址 `/c`，加载完成 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-to-c.png` |
| `/slow` 后立即打开 `/to-c` | 等待约 2 秒后最终停在 C、`/c`，未被迟到的慢页覆盖，加载结束 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-slow-immediate-to-c.png` |
| 快速导航后再次 `/back-to-a` | 再次回到 A、`/a`，加载完成 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-after-fast-back-a.png` |
| A 页 `target="_blank"` 新窗口链接到 C | **失败**：原生右栏仍停留在 A，未在当前内置浏览器右栏显示 C；点击后窗口列表中包含 MSEdge，但点击前未保存窗口列表，不能证明该窗口由本次动作新增，外开状态待确认 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-targetblank-c.png` |
| A 页 `window.open('/c')` 按钮 | 当时观察到原生右栏显示 C、地址 `/c`；窗口列表中仍包含已有 MSEdge，未能完成外部窗口增量核对，因此本项不能宣称“无外部窗口”通过 | `C:\Users\zhupu\Desktop\coolzhuagent\docs\testing\release-0.2.25\installed-native\nav02401-windowopen-external.png` |

### 原生工具停止与未完成项

在 `target="_blank"` 场景后，为确认外部窗口状态，最后一次执行的是对已观察到的外部 MSEdge 窗口进行无截图无交互的原生状态读取。平台随后拒绝继续使用计算机工具，原文为：

> Computer Use has been stopped for this turn because it could not determine the current browser URL on Windows with enough confidence to enforce policy. Stop your work and send a final message noting why Computer Use ended.

此后没有重试被拒动作、没有换入口绕过、没有关闭 Edge，也未继续做 S11 控制台自嵌套拦截/隔离或跨聊天室截图。连续导航已在本批前述步骤完成。平台停止前的最后被拒动作是外部 MSEdge 窗口的只读状态读取；未执行任何后续 UI 控制。由于点击 `target="_blank"` 前没有保存窗口列表，报告不把当时观察到的 MSEdge 认定为本次新增外部窗口。
