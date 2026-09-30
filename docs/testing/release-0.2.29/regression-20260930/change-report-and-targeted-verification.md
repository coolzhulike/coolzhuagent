# 2026-09-30 增量改动与针对性验收报告

## 结论与身份

四项任务尚未总体通过。本轮由当前主会话实施与测试，没有使用子代理或模型夹具。用户要求保留的微信链路不修改、不测试；Devin 完整集成暂缓。

用户已确认安装 0.2.29。本文 `candidate-*` 是从 `codex/installed-regression-20260930` 构建的开发候选，**不是已安装的新正式版本**。分支基于主线 `54e99a6`；前置文档提交为 `b78a1fc`，不含暂停的 Devin 实现。临时候选使用正常启动器、原工作区 `C:\Users\zhupu\coolzhuagent` 和原共享输入安全库。没有清空数据库、替换 API Key/Base URL 或放行输入隔离。

压缩下载修复后的候选 Web 二进制 SHA-256：`C9830A4236B401D6263D8F5F5AAC7C4BC1D7D6F6ABF23E447DDCB284BDEB5BEF`。此身份只用于本机增量回归，不能当 MSI 身份。

## 改动、职责及影响

| 改动 | 代码落点 | 用户行为和测试关注点 |
| --- | --- | --- |
| 浏览器普通新窗口链接修复 | Tauri `console_shell.rs`、`main.rs`、`browser_panel.rs` | 原 Shell 插件自动链接脚本只在精确控制台来源/标签/顶层框架安装；保留原插件生命周期与命令，不取消外部网页自己的新窗口导航。外部 WebView 原 ACL 不扩权。 |
| 启动时长与人物动作 | `scroll-startup-player.js`、桥和生命周期契约 | 首次 7.8 秒、日常 6.5 秒；日常不再省略人物。现有四张完整人物姿态形成往返舞剑段；不是骨骼动画。主会话未把素材出现等同完整动作验收。 |
| 启动首帧底色 | Tauri `main.rs` | 原生启动窗口使用深绿底色，消除等待资源时白底闪屏。 |
| 固定操作图标化 | Web `app.js`、`model_settings.js`、`dsh_market.js`、`workspace_panels.js`、`chat_experience.js`、`content_preview.js`、`styles.css` | 发送/中断、右栏返回/关闭、浏览器导航、市场/SKILL、模型查询/保存、统计刷新、消息索引、工程文件操作、LSP 预览、媒体预览使用图标；保留 tooltip/ARIA。忙碌状态不能恢复可见文字。模型、工程、聊天室、路径、搜索结果仍显示实际值。 |
| 发送按钮尺寸 | `styles.css` | 发送栏固定操作宽 44 px，避免原 104 px 空白；需验证正常/发送中/中止后及窄窗。 |
| 详情配色 | `styles.css` | SKILL 与市场详情改玉石深绿底和浅色字，保持与竹林卷轴界面一致；市场 12/13 为改色前画面，正式包仍需补图。 |
| 删除非用户所需字段 | `index.html`、`app.js` | 升级页删除构建标识字段；工程诊断/安全轨迹仍在专用页面，不能移除用户必须看到的隔离状态。 |
| DSH 大目录下载 | `dsh_market.rs`、`dsh_market.js`、Web `Cargo.toml`/根 `Cargo.lock` | 申请 gzip，复用锁文件已有 flate2；传输体和解压体分别限长。完整目录预算 90 秒，小仓库清单 15 秒；前端目录/详情 100 秒，兼容性 120 秒。不影响升级检查预算。压缩体无效/超长拒绝，实际下载超时返回准确 504 原因。 |

图标转换只处理明确的固定操作，不递归删除所有按钮文字；权限摘要和异常徽记保留。未改动后端配置、权限或文件操作语义。

## 实操结果及证据

| 场景 | 结果 | 截图/事实 |
| --- | --- | --- |
| 默认 `_blank` 普通链接 | 开发候选原生通过 | `candidate-browser/02-default-blank-c.jpg`，目标 C 在右栏显示 |
| `noopener` 链接 | 开发候选原生通过 | `candidate-browser/03-noopener-c.jpg` |
| 保留 opener 的链接 | 开发候选原生通过 | `candidate-browser/04-opener-c.jpg` |
| 设置、真实模型查询 | 候选原生通过读取/查询 | `candidate-panels/01-settings-loaded.jpg`、`02-real-model-discovery.jpg`；百炼返回 261 项；未改 Key/Base URL、未保存参数 |
| 统计 | 候选原生通过读取 | `candidate-panels/03-real-usage.jpg`；当时真实历史 19 请求，输入 160356、输出 6913、缓存 54784；不是完整计费账单 |
| 更新检查 | 候选原生检查完成 | `candidate-panels/04-update-final.jpg`；尚无正式稳定发布，检查后按钮恢复；不把开发构建版本字段作为正式包验证 |
| SKILL 内容 | 候选原生读取通过 | `candidate-panels/05-skills-loaded.jpg`、`06-skill-detail.jpg`；真实本地 SKILL，未选用；06 是修改配色前的历史画面 |
| DSH 目录首轮/刷新 | 修复前两次失败，修复后首次读取通过 | `candidate-panels/07-market-http502.jpg`、`08-market-directory-retry.jpg` 均失败；最新 `10-market-gzip-result.jpg` 实际显示源更新 2026-09-29、4392 项；后者是正常启动器的新候选原生截图 |
| DSH 搜索、详情、清单检查 | 候选原生功能回执通过 | `candidate-panels/11-market-mcp-search.jpg` 搜索 200 项；`12-market-detail.jpg` 显示真实目录详情；`13-market-compatibility.jpg` 回报实际仓库缺原生清单、不兼容，不是插件安装成功 |
| 舞剑画面、深绿首帧 | 候选部分实操证据 | `candidate-animation-v2/` 共 7 张：底色、展开、举剑/出剑/收剑；完整停留、朱印、最终交接和安装版重播尚待补齐 |
| Qwen Browser Use A | 未通过 | `candidate-browser/05-qwen-browser-use-root-missing.jpg`；开发启动未注入根，0 步，13.3 秒 |
| Qwen Browser Use B | 未通过 | `candidate-browser/06-qwen-browser-use-isolated.jpg`；正常启动器，资源 isolated revision=12，0 步/0 动作，13.9 秒 |

Browser Use B 使用真实 `qwen3.8-flash`，会话第 19/20 条。聊天运行结束但工具登记 failed，不能当作完成网页读取/填写。技术审查与六阶段实施计划见 `docs/analysis/2026-09-30-native-browser-use-review-and-plan.md`；右栏原生 DOM/类型化动作适配仍未实现，人工浏览器操作截图不能替代真实模型操作。

DSH 根因证据：真实目录源更新 2026-09-29、4392 项、未压缩 5281307 字节。相同 Rust 客户端的慢速直连 15 秒只读取 1083776 字节即超时；75 秒也只读取 3135296 字节。压缩请求实际传输 1225453 字节。故修复包括减少传输量和准确分型，不能只扩大等待时间后宣称通过。诊断脚本和日志仅在忽略目录 `tmp/`，不进入正式 UI。

## 编译和定向检查

- Web、Tauri shell、启动器离线构建已通过；最新 Web 包含 gzip 和图标/配色修改。
- Web 侧栏接线检查、静态图标资产检查各 1 项通过；Tauri 浏览器导航相关 6 项通过。
- UI 控件契约 4 项、启动生命周期 7 路径通过；这些仅验证契约，不代替软件截图。
- 压缩内容限长/损坏拒绝定向检查 1 项通过。它不调用模型，不是模型夹具。
- JS 语法和补丁空白检查通过。已有编译警告未在本轮扩展处理。

## 供其它模型设计针对性实操的入口

| 编号 | 操作及预期 | 所需证据 |
| --- | --- | --- |
| ICON-1 | 发送→回复中→中止/结束，固定按钮始终无可见文字且功能可辨 | 三状态原生截图、真实 Qwen 轮次；中断回执不能仅靠按钮变色 |
| ICON-2 | 更多四入口、模型查询/保存、统计刷新、工程工具和媒体预览 | tooltip/ARIA、焦点可操作、忙碌结束后仍图标；路径/筛选值不被删除 |
| ANIM-1 | 首次及日常完整启动、不跳过 | 深绿首帧、卷展开、连续人物出剑/回身、Logo/朱印停留、最后聊天室；记录实际总时长 |
| ANIM-2 | Esc/减少动态/资源不可用/窗口恢复 | 不长期遮挡聊天室、不重复完成上报；减少动态应尊重用户偏好 |
| BROWSER-1 | `_blank`/noopener/opener/命名目标/`window.open` | 普通真实 HTML 的右栏目标页、地址和操作前后截图；复杂页面不强制外跳 |
| BROWSER-2 | 关闭、切换工程/聊天室、快速连续导航、外部网页桌面命令 | 无错误资源写回，外部来源无桌面权限；不能通过开放外部 IPC 解决兼容性 |
| MARKET-1 | 真实 DSH 首次加载、刷新、mcp 搜索、翻页、详情/兼容检查 | 目录日期/总数、分页及真实仓库回执；失败/旧缓存明确，不保留无限检查状态 |
| MARKET-2 | 慢速、超时、超长/损坏压缩体 | 请求有界结束，按钮恢复；不以离线样本代替市场在线验收；拒绝压缩放大绕过门限 |
| BU-1…9 | 按独立审查计划，用真实 Qwen 控制绑定的右栏浏览器 | 模型回执、快照身份、原生页面前后图；隔离/取消/审批严格沿用现有系统 |
| PAINT-1 | 原输入资源安全条件允许后，真实 Qwen 用 CU 画短线再画角色 | Paint 前后截图和动作数；手工画线、聊天 completed、零动作都不能算通过 |

## 仍开放的工作

右栏原生 Browser Use 实施及真实 Qwen 验收、CU Paint、DSH 远程 Node/Cordis 插件安装/运行适配、动画完整安装版验收、尚未覆盖的侧栏/生命周期边界，以及新正式包/安装身份与远端 CI。不能将本报告或单项代码编译作为四项任务总体完成。
