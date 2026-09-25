# 玉石竹林工作台布局决策

日期：2026-09-19。用户已追加授权实际修改布局，并要求保留玉石图标、武侠竹林背景、加强文字对比度。本轮新增 `modules/gui-web/packages/web-console/src/wuxia_layout.css` 实施整体布局；统一模型页面同步在 `model_settings.css` 调整。早先三套视觉稿用于讨论方向，本轮最终方向为现有玉石武侠主题的收敛与可读性改善，已进入实施，不再等待第五优先级的方案批准。本文区分已落入代码的方向与仍需浏览器核验的子页细节。

## 已确认的约束

- 保留玉石图标、武侠竹林背景和现有头像资产；不要将其替换为通用线框图标或纯色空背景。
- 调整布局、字体、层次与文字对比度；装饰退到背景，正文与可操作控件仍需清晰。
- 左侧保留快捷图标轨道；点击后在右侧扩展栏显示对应功能。
- 顶栏 Agent、工程目录、聊天室横向排列，通过下拉切换环境。
- 思考只在生成期间显示，完成后隐藏；工具结果进入状态区；搜索索引、耗时与用量入口保持可达。
- 简化重复标题和按钮，但不能把现有独立功能变成不可达。功能入口移除前须确认替代入口。

## 本轮已实施的布局方向

- 新主题在 `index.html` 最后加载：styles → chat_experience → model_settings → wuxia_layout，保留前期功能修复。
- 正文 15px / 1.75，常规 UI 14px，标签和元信息 12～13px；主文字使用 `--jade-ink:#edf5ee`，辅文使用 `--jade-muted:#b5cbbd`。
- 消息采用稳定深绿底 `#102b21f7`，用户消息采用低饱和暖绿底 `#2b3020f7`；正文取消光晕、保持段落呼吸空间。
- 顶部 Agent / 工程 / 聊天室横向对齐，低高度压缩外壳高度与间距；左轨仍使用现有玉石 SVG / PNG 图标。
- shell 与聊天背景保留月夜竹林图，保留头像、玉剑水印、玉石状态和其他主题资产；水印与装饰降低视觉权重。
- 工具页保留内部 `.chat-tool-host-heading` 作为唯一标题栏，保留“返回”和“关闭”。`.has-tool-view` 时隐藏外层 rail-heading、tabs 与 footer；回到协作页后重新显示协作标签与动作。

以上描述的是实际代码方向；最终尺寸、滚动与文字对比由真实浏览器验收确认。

## 文字与背景：真实冲突来源

| 位置 | 旧样式来源与问题 | 本轮落地与后续建议 |
| --- | --- | --- |
| 全局字体 | `styles.css:25` 优先雅黑，末尾落到 Consolas / monospace；基础输入继承只含 button/input/select，textarea 不在同一声明 | UI 统一使用系统无衬线字体；仅代码、终端和原始日志使用等宽；显式覆盖 textarea |
| 聊天正文 | 基础 `.message-content` 为 15px / 1.45，但 `styles.css:4362` 的 `.ui-redesign .message p, ul, .message-content` 覆盖成 13px / 1.28 | 正文落地为 15px / 1.75；工具页 14px，标签与元信息 12～13px |
| 消息卡背板 | `styles.css:15715` 中 `.message` 使用 `rgba(80,125,103,.56)`，子 div 另叠 `.33` 透明层；图片与发光装饰透入文本区 | 使用一个稳定的深墨绿高遮盖率背板（建议 .90～.96），子 div 背景透明；保留竹林可见的消息间距与边缘 |
| 阅读底色 | `.chat-atmosphere-bamboo` 在 `12856` 使用双渐变与月夜竹林图，后续 `14458` 维持 `.72` opacity | 保留图源；通过遮罩调亮暗，不把图片删除；文字卡承担对比度而非让全部背景消失 |
| 文字色与层次 | 大量辅助文字为 8～11px；host kicker 在 `16587` 为 8px、按钮在 `16612` 为 9px；多个区域使用 opacity 降低整块内容 | 主文字落地为 `#edf5ee`，辅文 `#b5cbbd`；用明确颜色区分层次，避免对整块文字加低 opacity |
| 字体粗细与光效 | 早期样式频繁使用 font-weight 900/1000，文字与边框均发光 | 正文 400/450，标签 500，标题 600；正文不使用 text-shadow，发光仅保留在图标与当前选中状态 |

选择器应限定真实区域，例如 `body.ui-3d .workbench-window[data-window-theme="communication-bay"] .message-content`。对 `.message p` 和 `.message ul` 同步覆盖，避免块间字号不同。不要对所有 `pre` 设置无条件换行：聊天代码可横向滚动，终端长日志按独立内容规则处理。

新主题规则已放在现有布局规则之后，并与 `model_settings.css` 使用一致的颜色/字体方向。末尾加载仍不能覆盖更高优先级的旧选择器，须检查实际计算样式，不能只比较文件先后。

## 右侧扩展栏：减少套层

真实结构为：`.chat-right-rail-heading` → `.chat-right-tabs` → `.chat-tool-host-heading` → 子页面标题。`renderChatRightRailHeading()` 与 `setChatToolHostMeta()` 都从 `CHAT_TOOL_WINDOW_META` 取得同一功能标题，因此工具视图至少有两次重复命名。

实际选择保留内部 `.chat-tool-host-heading` 及其返回/关闭动作；工具视图隐藏外层 `.chat-right-rail-heading`、`.chat-right-tabs`、`.chat-right-rail-footer`，扩展栏改为纵向 flex，内部宿主使用 `42px minmax(0,1fr)`。这样工具名称只出现一次，同时释放重复页头与标签占用的高度。协作页面继续保留原有页签，工具“返回”恢复此前协作/任务/状态视图。

“返回”与“收起”的行为并不完全相同：前者恢复此前协作页，后者折叠右栏。本轮保留宿主返回按钮，因此隐藏工具页的外层标签不会丢失返回路径；内部关闭按钮保留其原有行为。协作页的关闭和页签保持可达。

| 内容 | 建议 |
| --- | --- |
| `.chat-right-rail-footer` 的任务链/转交/收起 | 已在 `.has-tool-view` 时隐藏；协作视图保留实际任务动作；工具关闭由内部唯一标题栏提供 |
| `.chat-tool-host-heading-copy > span` / `.chat-right-rail-heading-copy > span` | 删除重复装饰小标题，保留一个明确功能名 |
| `.chat-insights-panel > h2` | 在已由右栏标题命名的历史/用量页减少重复标题；搜索标签仍必须存在 |
| 统一模型页 `.ms-eyebrow` / `.ms-intro h2` / 说明 | 收敛重复命名，保留说明字段用途的文本，不再叠多层介绍 |
| 顶栏健康玉石、灯笼、警示 / 底部连接 / 聊天活动状态 | 健康、连接、运行并非同一事实，不盲目合并数据；顶栏采用紧凑健康标记，异常明确显示，当前轮运行文字只在聊天状态条显示 |
| 工程索引状态、操作状态 | 空闲信息可以压缩；执行失败与保存结果仍要显示，不能用 CSS 全部隐藏 |
| 状态页环境信息 | 顶栏已有 Agent/工程/聊天室；状态页优先显示模型、权限、健康与任务，重复环境信息可降为详情 |

右栏的四层容器 `.chat-tool-host-content`、`.workbench-window`、`.window-panel`、各页面 layout 多处同时 `overflow:auto`，且多层 `height/min-height:100%`（`styles.css:16636` 起）。每页应指定一个主滚动层；代码、目录树、日志等明确内容区可独立滚动，外壳不再叠出滚动条。

## 保留的主题资产

所有路径位于 `modules/gui-web/packages/web-console/assets/`。

| 资产 | 作用与处理 |
| --- | --- |
| `icons-wuxia/*.svg` | 左轨与功能按钮现有玉绿金线图标；例如 chat.svg 使用 `#b9dfcf` / `#d7b35a`；保持原图案与颜色 |
| `ui-redesign/three-column/control-icons/*.png` | 专注、面板开合、审批、暂停/继续等玉石控件；保持语义，尺寸按操作密度统一 |
| `avatars/wuxia-v3/*.png` | 当前 Agent 与消息头像；继续保留选择能力 |
| `ui-redesign/bamboo-wuxia-stage-bg-v1.png` | 保留原外层武侠竹林资产；新主题统一使用月夜竹林覆盖 shell 与聊天背景，避免旧蓝色舞台漏出 |
| `ui-redesign/chat-backgrounds/wuxia-bamboo-moonlit-chat-stage-v2.png` | 当前 shell 与中央聊天使用的月夜竹林；保留图源、通过遮罩与稳定内容背板控制可读性 |
| `ui-redesign/p5/chat-watermark-jade-sword-v1.png` | 玉剑水印；可降低透明度与发光，避免穿透文字卡 |
| `ui-redesign/p6/status-jade-ready-v1.png` | 健康状态玉石；与真实状态保持绑定 |
| `ui-redesign/p6/brand-banner-wuxia-v1.png`、banner-layers、`bamboo-banner-wind.js` | 品牌与竹叶层次；顶栏变矮时裁切容器而非换掉资产，尊重减少动画偏好 |
| 灯笼、印章、jade-success-sweep | 属于装饰和结果反馈；保留资产，可减弱常驻动画和重复位置，不能承担唯一状态说明 |

不要批量重写所有 `img` 为同一种颜色滤镜；这会破坏玉石 PNG、头像与截图本身的颜色。

## 子窗口实施优先级与选择器

以下选择器均以 `.chat-tool-host-content >` 作为宿主前缀，避免影响不在右栏中的原始窗口。

| 优先级 | 页面 / 选择器 | 当前问题 | 建议布局 |
| --- | --- | --- | --- |
| P0 | 通用 `.workbench-window.is-active > .window-panel` | 100% 最小高度与多层滚动、边框套边框 | 一层内容背板；主滚动层明确，min-width/min-height 为 0 |
| P0 | `.settings-workbench-window .settings-layout` | 多个设置大组连续堆叠，旧样式规则众多 | 模型页优先；生命周期、权限、语音等用清晰分组；统一间距与输入高度，保留新头像保存 |
| P0 | `.project-workbench-window .project-command-rail > .ide-toolbar` | 尾部 `17706` 强制三列，按钮 24px 高 / 10px 字；窄栏标题压缩 | 常用保存/刷新/新建明确可见，低频动作菜单化；32px 左右控件与 12～13px 标签 |
| P0 | `.project-workbench-window .project-command-rail > .project-tree` | 尾部 `17783` 强制树 122px，不随可用高度增长 | 常规高度使用 `flex:1; min-height:140px`；只让目录树内容滚动 |
| P1 | `.project-workbench-window .project-layout` / `.ide-preview-body` | 38% 文件树 + 预览 + 大纲，下方大纲继续占高 | 宽右栏保留树/预览左右分割；窄栏通过可达标签切换；大纲收为详情；不要盲目把所有区域单列 |
| P1 | `.terminal-workbench-window .terminal-window-command` | `17190` 将命令、超时、运行按钮全部单列；低高度输出被挤掉 | 命令文本主行；超时/运行/清空横排。输出占剩余高度，执行详情默认折叠 |
| P1 | `.browser-workbench-window .browser-window-toolbar` | 前进/后退/打开/刷新/停止/独立窗口并列，在窄栏多次折行 | 导航图标 + 地址主输入 + 打开；停止仅运行时可用，独立窗口次操作；代理/诊断保持 details |
| P1 | `.tasks-workbench-window .task-permission-layout` / `.task-window-section-head` | 任务、配置、诊断在单列长页面混排，多处小字号 | 当前任务与待审批前置；角色配置与诊断可折叠；状态列表文字优先，操作只出现一次 |
| P1 | `.memory-workbench-window .memory-window-filters` / `.memory-window-layout` | 筛选、四个统计、生命周期作业、会话历史位于记忆珠列表之前 | 关键词与必要筛选首行，统计紧凑；作业/历史收为展开详情，记忆珠入口靠前；来源追踪随选择展开 |
| P1 | `.vision-workbench-window .vision-window-layout` / `.vision-command-rail` | `17580` 强制单列，五张操作卡在截图之前 | 截图证据优先；常用采集/描述/定位横排或双列；闭环与诊断折叠，清晰保留执行状态 |
| P2 | `.clawbot-workbench-window .clawbot-window-panel` | 三轨内容被压成长单列，状态与设置混杂 | 连接状态前置，绑定/管理操作分组，原始事件日志收详情 |
| P1 | 历史 `.chat-history-result`、用量 `[data-role="chat-usage-list"]` | 搜索结果是完整文字块，计数/日期层次不明显 | 发言人/时间/索引作为12px元信息，匹配摘要14px；用量分输入/输出/缓存，定义说明保留 |

低高度下优先减少标题、空隙和重复操作，再考虑分栏；不要通过把文字缩到 10px 来容纳所有内容。响应式应参考右栏实际容器宽度，而不是只按全窗口宽度决定：大屏中右栏仍可能很窄。

## 新旧 CSS 叠层核验点

这是对第一版 `wuxia_layout.css` 的只读审阅发现，实施者继续修复，最终以计算样式与截图为准：

- 消息左侧模糊玉绿光点来源于旧 `styles.css:8838` 的 `.chat-workbench-window .message > div::before`，不是 `.message::before`。清除外层伪元素时也需要覆盖内容子 div 的伪元素。
- 非工具协作页的旧 header/tab/footer 规则带 `.workbench-window[data-window-theme="communication-bay"]`，优先级高于新主题的短选择器。应匹配相同作用域，否则外框 38/36px 行高与内部旧 52/42px 元素冲突。
- 旧 `.message.user > div` 与 `.message.thought` 比通用 `.message > div` / `.message` 更具体；应同步覆盖，避免用户消息出现第二层金色背板、系统通知保留蓝灰底。
- 旧 `.chat-atmosphere-bamboo` 的限定作用域 opacity 规则高于新主题短选择器；竹林实际透明度需在浏览器确认。
- 清除蓝色舞台应针对 shell 和残留装饰层，保留新主题月夜竹林背景；不要直接把所有背景设为纯色。
- 新旧多层 `overflow` 与高度约束仍需逐页验收；页面在最后加载不代表所有旧几何规则已经失效。

## 验收建议

- 1366×768、1024×600 与窄窗口检查顶栏三项的标签/当前值/下拉入口均可读。
- 打开设置、工程、终端、任务、历史、用量，正文无横向裁切，唯一主滚动区域明确。
- 保留玉石图标、现有头像、竹林可见背景；消息正文不被水印/竹叶/发光纹理干扰。
- 工程树可随高度扩展；终端命令区不会吞掉大部分输出区；视觉截图和记忆列表不是长页面末端的隐藏功能。
- 缩减标题后仍能通过键盘访问关闭、返回、保存、审批等真实动作；图标提供明确 title / aria-label。
- 长中文、英文路径、代码块、错误信息和进行中状态均检查文字对比与换行；低高度不以过小字号换空间。

本文由只读审阅更新；产品代码由本轮布局实施任务修改。实际实施以 `wuxia_layout.css` / `model_settings.css` 为准，浏览器验收结果见同日实现记录。

## 子窗口浏览器验收附录

2026-09-19 使用独立 Edge 会话 `jade-review`，在本地隔离验收服务 `127.0.0.1:18765` 检查 1280×720 与 1024×600。全程仅切换页面、展开详情、滚动和截图，没有发送聊天、运行工具或保存配置；验收结束已关闭该浏览器会话。

| 页面 | 发现及修复后结果 |
| --- | --- |
| 任务中心 | 低高度下可滚动到模块自检与诊断入口，未发现底部功能永久遮挡；没有触发任何任务或诊断。 |
| 记忆 / 知识 | 修复旧网格将内容区压成 0px 的问题；记忆列表、星图、详情和来源追踪均可滚动到。卡片装饰伪元素不再占正文列，正文可用宽度由 14px 恢复到 362px；最终实测卡片与网格行高均为 76px、间距 6px，七条记录均无行间重叠。 |
| 视觉实验室 | 修复操作卡压缩覆盖与截图区高度不足；截图证据前置，操作卡按自然高度排列。低高度下定位、闭环验证及定位摘要均可达，文字和按钮没有互相覆盖。 |
| 浏览器 | 修复桥接诊断展开后被预览区域覆盖的问题；代理与诊断同时展开时，诊断区实测高度约 264px，输入项、三个诊断按钮和结果说明完整可见，可继续滚动到浏览器预览。 |

上述页面未发现新的阻断性裁切或明显文字对比问题，浏览器控制台没有 JavaScript 错误。本次仅验收页面布局与可达性，不代表视觉执行、浏览器诊断或记忆写入动作已运行验证。

截图保存在 `tmp/2026-09-19-e2e/`：`review-tasks-1280.png`、`review-tasks-1024-bottom.png`、`review-memory-final-1024-list.png`、`review-memory-fixed-1280-source.png`、`review-vision-fixed-1280.png`、`review-vision-fixed-1024-actions.png`、`review-vision-fixed-1024-summary.png`、`review-browser-fixed-1280-diagnostics.png` 和 `review-browser-fixed-1024-diagnostics.png`。前缀相同而没有 `fixed` / `final` 的截图保留修复前状态，勿作为最终效果图。
