# 原参考图视觉纠偏：源码与最终调试二进制验收

日期：2026-09-27。参考基准是用户提供并归档的 [`original-scroll-ui-user-reference.png`](../../../design/2026-09-27-ui-concepts/assets/original-scroll-ui-user-reference.png)。本轮新增的字标和竹林图是按该图制作的**参考衍生素材**，不是从原图无损提取的独立图层。

## 实现范围

- `index.html` 顶部品牌的可访问名称和文字改为 `COOLZHU`；原有 Agent、工程目录、聊天室三个按钮及绑定未改。
- `src/scroll_theme.css` 以两侧等宽轨道保证字标对齐窗口中线；窄窗继续单行显示字标和三个真实入口，长名称省略。字标取代左上角小块品牌；外层与聊天阅读区改用竹林底图，阅读区中部保留深色遮罩，左右边缘露出竹叶。
- 产品资产新增 `assets/ui-redesign/scroll-frame/coolzhu-jade-wordmark-v1.png` 与 `original-bamboo-background-v1.png`，沿用原有 `scroll-frame-v1.png` 玉轴。未改 `app.js`、动画 B 或 Rust 资源路由；品牌测试断言由负责 Rust 的代理同步更新。

## 实图与交互

| 证据 | 结果 | 身份 |
| --- | --- | --- |
| [1440×900 正常窗](final-binary-normal-1440x900.png) | 字标居中，竹叶在外沿与聊天两侧可见；消息中心与输入区保持暗色阅读面 | 最终调试二进制 |
| [903×551 低窗](final-binary-low-903x551.png) | 顶栏仍是单行，字标、三入口、输入与底部更新可见 | 最终调试二进制 |
| [Agent 下拉](source-preview-agent-menu-903x551.png)、[工程目录下拉](source-preview-workspace-menu-903x551.png)、[聊天室下拉](source-preview-room-menu-903x551.png) | 三个原按钮真实点击打开 `dialog`，不被顶栏或玉轴裁切；聊天室按 Escape 关闭并恢复焦点 | 按盘源码预览 |
| [更多四入口](source-preview-more-903x551.png) | 浏览器、终端、SKILL、插件市场展开可见，底部升级入口仍可达 | 按盘源码预览 |
| [右栏正常窗](source-preview-right-rail-1440x900.png)、[右栏低窗](source-preview-right-rail-903x551.png) | 点击设置打开真实右栏；正常窗聊天与设置并列，低窗设置覆盖右侧，输入与发送按钮仍可用 | 按盘源码预览 |

按盘预览使用首轮构建副本，SHA-256 为 `0EF736167487C4EEE51DF45D522FB60D5F5B78E5BF3CE9FFA8B943E34FDC28ED`，独立端口 `62572`，`COOLZHU_WEB_STATIC_ROOT` 指向当前 `web-console` 源码目录，因此这些交互图不代表最终打包资源。最终复验使用重新构建的 `coolzhu-web-console.exe` 副本，SHA-256 为 `487CA0B41C9E97912DD5695E88C2E89E8E9E8D14175E12FC7FB38643883CA69A`，独立端口 `55222`，**未设置** `COOLZHU_WEB_STATIC_ROOT`。真实 HTTP 请求对 `/`、`/src/scroll_theme.css`、两张新 PNG 与原卷框 PNG 均返回 200，类型分别为 HTML、CSS 和 PNG。调试构建仍可能经自身的源码候选目录读到资源；这两张图证明最终二进制运行时的页面效果，不单独证明安装包资产完整性。安装态需看对应安装验收记录。

两个服务的工作目录、`USERPROFILE`、`HOME`、`APPDATA`、`LOCALAPPDATA`、`CLAW_CONFIG_HOME`、`COOLZHU_RUNTIME_DIR`、`COOLZHU_LOG_DIR` 均分别指向 `tmp/visual-scroll-live` 与 `tmp/visual-scroll-final`；配置禁用真实 LLM，未使用云模型、伪造回复或用户会话库。正常与低窗终版图显示的是隔离工作区的空聊天室。`tmp/visual-scroll-live/reading-fixture.txt` 只是准备中的本地长文代码文本，**没有注入 DOM、发送或渲染**；本轮不能据此宣称长文与代码卡已验收。

## 隔离遗漏与清理状态

预览启动脚本遗漏隔离 `TMP`、`TEMP`、`TMPDIR`。Web Console 启动时的 `persist_gui_web_url` 于是把最后的临时服务地址 `http://127.0.0.1:55222` 写入共享的 `%TEMP%/coolzhu-gui-web-url.txt`（只读核对到文件最后写入时间 2026-09-27 22:51:57）；桌面壳的 `gui_web_url` 会读此文件。这解释了安装版壳一度打开临时工作区的 URL。脚本设置的 `COOLZHU_DESKTOP_SHELL_MANAGED=1` 使预览 Web 跳过自动拉起桌面壳；脚本没有显式写 launcher 最近工程、HKCU 或公共 recent。共享临时 URL 是已确认的跨环境副作用。

发现后已停止两个预览 Web 父进程，并确认端口 `62572`、`55222` 无监听；未停止或替换默认 `8765` 与安装版进程。更早一次误把 `--help` 交给不解析该参数的调试程序，它尝试默认 `8765`，因端口已占用而退出，并在用户默认日志路径留下启动记录；该次没有成功监听。曾尝试只在文件内容精确等于该预览 URL 时删除共享 hint，命令被自动审批策略以 `blocked by policy` 拒绝，**没有执行删除，也没有改用其他手段绕过**。随后主会话与安装验收代理确认：首轮是直接启动桌面壳读取了残留 hint；通过正式系统入口重新启动后，安装版 Web 回到 `8765`，壳窗口 URL 与原工程恢复，hint 由正常启动自然写回。本轮不再启动预览服务或干预桌面。
