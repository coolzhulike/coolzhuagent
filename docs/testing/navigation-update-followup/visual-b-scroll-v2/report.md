# B 启动与单主卷框：源码版视觉验收

2026-09-27。本记录在 **0.2.21 打包前** 用独立浏览器和私有服务检查源码视觉；不等同于已安装版的原生 WebView2、混合 DPI 或启动交接验收。Web 控制台由离线开发构建的独立副本运行，SHA256 `C2CE91124626F80609758C67494B21BC7831C311133261A83DFB84EC6E9ECFC6`；启动页由当前仓库 `tauri-shell/ui` 静态源码在另一私有端口提供。两个服务均未连接 8765。

## 实际播放与页面

- [首次启动连续播放录像](startup-b-v2-continuous.webm)：Playwright 浏览器在 `?mode=first` 下启动真实播放器，从导航开始录制到约 5.2 秒后停止；录制包含导航前短暂空白。播放结束时启动层已隐藏，首次演出标记写入本次隔离浏览器的 localStorage。该 WebM 是**连续实际播放录屏**，不是定点图片合成。
- [1670 毫秒](startup-b-v2-1670ms.png)只出现腾空一帧，无前版双头；[2090 毫秒](startup-b-v2-2090ms.png)是出剑姿态；[2500 毫秒](startup-b-v2-2500ms.png)处于人物退场段；[2840 毫秒](startup-b-v2-2840ms.png)人物已退出、字标与“酷朱”朱印留场。这些定点图由同一真实播放器加载实际资源后调用 `renderAt(time)` 渲染，补充录像的可定位时间点。四帧各用原透明图的不等宽 `sourceRect` 和独立鞋底锚点，保持人物比例；不声称骨骼动画。
- [1440×900 正常页面](ui-scroll-v2-normal-1440x900.png)、[长文代码布局](ui-scroll-v2-long-1440x900.png)、[展开右栏](ui-scroll-v2-right-1440x900.png)、[903×551 稳定页面](ui-scroll-v2-low-stable-903x551.png)、[右栏打开后缩至 903×551 的稳定页面](ui-scroll-v2-right-collapse-stable-903x551.png)来自上述独立 Web 副本。长文与代码是**只为排版检查注入的示例内容**，不是用户消息。浏览器 `devicePixelRatio=1`，无原生 WebView2 面板。

首次 903×551 即刻截图（包括开右栏后立即缩窗）遇到了尚未完成的页面恢复/布局过渡，因此均未作为通过证据。稳定等待后顶部显示隔离工程及主 Agent；右栏打开后缩窗，聊天 `x=70..895`、输入区 `x=71..894`，右栏移至视口外 `x≈907`，页面无横向溢出，发送与输入仍在可见范围。没有残留约 114 像素的右栏占位。

## 样式与资源核对

单主卷框仅从透明边框素材采样四边和角部，中心不填充；正常窗口外缘 `12px 30px`，低高度收至 `6px 8px`，不接收指针。页面长文、代码、输入框和右栏均未被装饰遮盖。生成图原件与生产拷贝 SHA256 完全一致：卷框 `7277D593D6C691FAB3C454C18E8FF0F7ECE3C51D03EB37D85C7BDDDF7167BEE8`，四姿态 V2 `15B96ADE3029B7B90F05C2A94D8BB93351B3BC622CC9E8635942825D30D33DBB`；V1 姿态未接入生产。

1440×900 页面实际 computed style：正文 `rgb(237,241,232)`、`16px/27.2px`（1.7 行高）；消息标题 `rgb(184,230,200)`、`13px/19.5px`；代码 `rgb(229,242,232)`、`13px/20.8px`，代码底色 `rgb(6,24,18)`；输入文字 `rgb(237,241,232)`、`15px`，输入底色 `rgb(6,27,20)`；发送按钮文字 `rgb(234,244,232)`、`14px`。正文与控件均位于深墨绿阅读面。此处记录浏览器实际值，不以设计目标代替实测。

## 构建与边界

实际执行了以下命令，`cargo build` 最终日志在 `tmp/scroll-b-v2-web-build-final.log`，最后开发构建哈希如首段：

```powershell
cargo build -p coolzhu-web-console --offline
node --check modules/gui-desktop/packages/tauri-shell/ui/scroll-startup-player.js
node --check modules/gui-desktop/packages/tauri-shell/ui/coolzhu-seven-letter-manifest.js
node --check modules/gui-web/packages/web-console/src/app.js
node modules/gui-desktop/packages/tauri-shell/ui/tests/coolzhu-seven-letter-bridge.cjs
node tmp/scroll-b-v2-lsp-frontend-contract.cjs
```

现有 bridge 检查的完整、日常、Esc、减少动态、资源超时、恢复和创建失败七类生命周期全部通过。私有页面由 Playwright CLI 独立命名会话控制；连续录像实际顺序为 `open about:blank`、`resize 1440 800`、`video-start startup-b-v2-continuous.webm`、`goto http://127.0.0.1:58488/launch-performance.html?mode=first`、等待 5.2 秒、`video-stop`。定点帧和页面截图分别运行 `tmp/scroll-b-v2-e2-20260927/capture_startup_v2.js`、`capture_ui_v2.js`，低窗通过另一次稳定等待复拍。两个私有服务和三个 Playwright 命名会话已按进程身份关闭。

同轮还修正了 LSP 启动请求：`expected_workspace` 提交已确认的实际工程路径，布局用编码/截断 key 只用于迟到响应守卫。定向脚本直接运行前端 `startLspPreview` 函数，空格、中文、超过布局 key 180 字符的长路径、缺工程四种提交体/拒绝情形 4/4 通过；这是前端函数契约检查，**不是原生 LSP 服务启动验收**。

原生首次启动播放与恢复交接、不同 Windows DPI、原生右栏及输入操作，须在 0.2.21 候选安装后单列验收。本记录不追认到先前 0.2.20 已安装包。
