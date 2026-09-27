# 0.2.18 已安装版原生窗口实操

日期：2026-09-27。测试对象是本机正常升级安装后的 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe` 入口及其桌面窗口，不是源码开发页。MSI SHA256 `33F60F05143FEDF73A34678F7E3A3D06D26D2D194D217EFB3AA8730CBCA6DEB2`；安装 Web SHA256 `581822BE7B8EAC97A9CC6AF1B64AEA5F5BA2369CE13445682FE0154D71CA1081`。安装身份与数据保存另见 [升级对账](upgrade-data-acceptance.md)。界面“正式发行版”仅表示 release 构建渠道；这个候选包尚未签名或正式发布。

| 项目 | 本次实际操作与证据 | 结果 |
| --- | --- | --- |
| 主界面与更多 | [全窗口启动](00-start.png)可见原有五个主入口、更多及底部升级更新；[展开更多](01-more-open.png)可见浏览器、终端、SKILL、插件市场四项。随后逐一点击[首页](22-home-nav.png)、[定时任务](23-schedules-nav.png)、[设置](24-settings-nav.png)、[统计信息](25-statistics-nav.png)、微信连接（私有现场原图 26-wechat-nav.png 留在本机，未纳入 PR），均打开对应区域；微信卡显示“已停用”，没有执行连接。 | 五个既有入口通过所测可达路径；微信连通未验 |
| 消息索引与定位 | 在当前测试房间“验收-20260927-安装版018”搜索“安装版文字通道正常”，[索引显示两项](20-search-two-matches.png)，分别对应本房间用户 #1 与助手 #2；点击助手 #2 后[聊天定位到该消息](21-search-result-located.png)，消息卡可见“本轮 3.9 秒”。未跨房间搜索或发起新模型请求。 | 通过所测房间内搜索与定位路径 |
| 更新 | [版本状态](03-update-status.png)显示当前 0.2.18；手动点击检查后的[实际结果](04-update-checked.png)为官方仓库暂无正式稳定版，没有假称“已最新版”，没有自动下载、安装或重启。 | 通过本机无正式版分支 |
| 浏览器、SKILL、插件目录 | [浏览器入口](05-browser-settled.png)可用；[本地 SKILL](06-skills.png)显示实际目录状态；[插件目录](07-market-settled.png)如实显示目录未接远端/不能在线安装。通过地址框打开 [https://example.com/](15-https-example.png)，普通 HTTPS 页在右侧原生预览中可见。未验证需登录或限制嵌入的网站。 | 通过所测路径 |
| 文件和 URL 右栏 | 原有聊天中的[本地 URL](11-url-preview-loaded.png)实际加载；[文本附件](12-file-preview-loaded.png)的中文、emoji 和行号可见。 | 通过所测样本 |
| 终端 | [空闲](08-terminal-idle.png)和[启动 PowerShell](09-terminal-running.png)可达。隔离诊断命令经终端 API 发送后，原始 ConPTY 输出含中文结果，但[画面](10-terminal-output.png)只剩 `>>`，有效内容被约 23 行清屏填充及周期性滚底推出视口；上滚也被随后轮询拉回。此轮未从界面输入并点击运行验证文件副作用、中断和刷新。 | **失败，0.2.18 保留失败身份** |
| 工程文件→LSP | 通过界面切到隔离 Rust 工程，[工程已切换](14-lsp-workspace-switched.png)；但旧包没有从正常界面打开已有“项目文件”右栏的入口，无法实际点文件再启动 LSP。不得用隐藏 `?window=project` 路由代替用户路径。 | **失败，用户入口缺失；LSP 原生端到端未验** |
| 原生低高度 | Windows 系统贴靠得到真实 903×551 窗口，打开外部浏览器预览后收窄，右栏折叠但 WebView2 内容仍留在右下，[覆盖聊天和输入](16-low-height-browser-occlusion.png)；等待后未消失。放大后右栏与关闭按钮恢复，点击“关闭当前工具”再收窄则遮挡消失。 | **失败，窄窗口浏览器显隐不同步** |
| 工程恢复与重启 | 通过正常界面切回原用户工程；原聊天仍可见。关闭应用后 8765 无监听；经正式 `COOLZHU-AGENT.exe` 入口重启，原工程仍可见：[重启截图](19-reopened-user-workspace.png)。单独启动内部 `tauri-shell.exe` 不会拉起 Web 服务，不能作为安装入口验收。 | 通过所测路径 |

工程下拉中点击与当前路径相同的快捷项时出现过较长刷新，随后恢复；[当时截图](13-workspace-switch-stuck.png)并非永久锁定证明。首启恢复与数据对账已在模型验收产生新聊天之前完成，具体数字见相邻报告。

终端、工程文件入口和低高度原生浏览器三项已提交窄修，需在新的候选包重新安装后，按实际界面重测并保留 0.2.18 失败证据。其他未覆盖项包括启动动画原生首次演出/跳过、实际 Windows 混合 DPI、终端完整交互、LSP 点击诊断/定义/引用；它们不能由这批截图补认通过。模型与新聊天的安装版实操由[独立报告](../../release-0.2.18/installed-model-chat-acceptance.md)记录。
