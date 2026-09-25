# 视觉稿生成提示词

日期：2026-09-19。生成方式：内置 image_gen；每套单独生成，随后按检查结果精修。仅用于概念审阅，未应用到产品。示例模型名、消息、时间、用量均为虚构占位。

## A · 浅色冷静工作台

```text
Use case: ui-mockup. Create one high-fidelity desktop application interface concept image, landscape 16:10, approximately 1600x1000. Product name COOLZHU CODE, Chinese UI for an AI coding assistant. Concept A: quiet light workstation. This is a design proposal, not a screenshot of existing software. Straight-on flat UI screenshot, no laptop frame, no perspective, no people, no decorative illustration.
Layout invariants: only a 56 px vertical icon rail at far left, no expanded left chat list. A single compact horizontal top bar 52 px high contains small brand, then three inline dropdown selectors side-by-side: “Agent  开发助手 ⌄”, “工程  coolzhuagent ⌄”, “聊天室  控制台改造 ⌄”. Labels and values are inline, never stacked. Chat occupies center 64% width and a docked right extension panel 31% width. White warm-grey surfaces, very subtle blue-grey boundaries, teal primary accent, calm professional typography, Chinese sans serif similar to Microsoft YaHei, 16 px message type, generous but useful spacing, one consistent thin stroke icon set. No ornamental borders, no gradients, no neon, no oversized title.
Left rail icons: chat, folder, task, terminal, globe, search, usage chart; settings pinned at bottom. Search icon active.
Center chat: user bubble “梳理工具调用链路，给出修复方案。” Completed assistant reply is pure readable prose with a short numbered list, title “已完成架构梳理”, body “统一参数入口，并将工具权限与工具可见性分开管理。” and “1. 明确模型能力与参数范围”, “2. 工具状态独立显示”, “3. 保留聊天主区域的阅读空间”. No exposed internal reasoning or raw tool JSON in completed reply. Under it a tiny quiet metadata row “耗时 18.4 秒 · 3 次调用 · 2,840 tokens”. At bottom above composer one slender horizontal tool status strip with check icon “已完成  读取文件 3/3   查看活动 ›”. Composer about 100 px high with placeholder “继续对话…”, small attachment / microphone icons and teal send arrow. Compact model + “思考：高” chips below input.
Right panel titled “记录搜索” with close x and search field “工具调用”; filter pills “当前聊天室” and “全部记录”; 3 crisp search result rows show timestamp, matched keywords highlighted, and “跳转到消息 ↗”. Bottom secondary section “消息索引” with small numbered turn list. Bottom right a very small usage summary “本轮 2.84K · 本会话 18.6K”.
Microcopy may be simplified to keep Chinese legible; composition is most important. This must look like a coherent polished shipping desktop app designed by a senior product designer.
```

### 精修

```text
Edit this desktop UI concept while preserving all overall geometry, surfaces, text hierarchy, chat content, right search panel, header and composer. Make two precise changes only: 1) Replace the circular assistant avatar currently showing the OpenAI swirl logo beside the reply with a simple teal circle containing the white letter C, so there is no third-party company logo. 2) Make the narrow far-left navigation rail icon-only: remove the visible text labels under the navigation icons (聊天, 文件, 任务, 终端, 网页, 搜索, 用量, 设置), leaving the corresponding icons centered in their existing slots and selected search highlight unchanged. Keep all remaining content unchanged. Clean and professional.
```

## B · 深色高密度工作台

```text
Use case: ui-mockup. Generate one high-fidelity flat desktop app interface concept, landscape 16:10 approximately 1600x1000. Product COOLZHU CODE Chinese AI coding assistant. Concept B: dense dark engineering workspace with exquisite restraint. Dark graphite #171B22 main surface, slightly lighter #202731 right panel, crisp soft white sans serif text, muted blue-grey secondary text, single electric cyan #5CBBD2 accent. Professional systems UI, extremely clean and usable, readable at low height. No computer mockup or perspective, no neon, no artwork, no glowing borders, no other company logos.
Must: far left only a 52 px icon rail, absolutely no left expanded chat navigation. Top 48 px bar, one horizontal line: small “COOLZHU”, “Agent 开发助手 ⌄”, “工程 coolzhuagent ⌄”, “聊天室 模型调试 ⌄”; labels and values on one line. Left rail outline icons for chat, folders, tasks, terminal, browser, search, usage; gear at bottom. Right extension is 34% width, chat 61% width, no unused dead zone.
Main chat shows completed prior short exchange and current running reply. Completed user text: “检查本地模型的工具选择策略。” Prior assistant short completed answer: “已定位工具暴露与权限配置的交叉影响。” metadata “耗时 12.8 秒”. Current run is shown below as a small live “正在思考 · 06.2 秒” block ABOVE any final output, with one muted progress summary “正在核对当前会话参数与工具范围…”. Never show completed hidden chain-of-thought. One slim persistent horizontal tool status strip immediately above composer: “运行中 · 检查模型能力 · 第 2 / 3 项”, cyan spinner, right link “活动 ›”. Composer compact multiline field “补充要求，或引导当前运行…”, attach icon, small model chip “本地代码模型”, “思考 高” and visible square stop button. Running state without a fake completed success.
Right extension panel selected tab “模型参数”, tabs “连接 / 生成 / 工具 / 用量”. Unified model configuration form, no provider table, no vendor list. Show connection fields in two compact columns where appropriate: “配置名称” value “本地代码模型”, “协议” value “OpenAI 兼容 ⌄”; full row “接口地址” value “http://127.0.0.1:8080/v1”; “模型 ID” value “local-code-model”; API key masked dots. Reasoning control “思考强度” with segmented 自动 / 低 / 中 / 高, 高 selected. Numeric fields “最大输出 8192”, “上下文 32768”, “温度 自动”, “超时 120 秒”. Tool section “工具策略” dropdown “文件与终端”, separate “调试权限 完全访问” badge. Collapsed “高级参数  {}” section. Bottom save button “保存配置”, secondary “测试连接”. Small footer “本会话 18.6K tokens · 本轮 2.84K”. Use real crisp thin icon motifs, no random decoration. Chinese copy legibility matters but preserve layout over text density.
```

### 精修

```text
Edit this UI mockup only. Remove the visibly pure-black rectangular/blurred artifact in the empty central chat area between the '正在思考' card and the bottom tool status bar. Replace the entire empty area seamlessly with the same uniform dark graphite #171B22 main chat background. There must be no black blob, no gradient/noise artifact, no extra card, no new text. Keep every other layout, text, control, color, icon and the whole right parameter panel unchanged. Flat clean professional desktop UI screenshot.
```

## C · 温暖轻量工作台

```text
Use case: ui-mockup. One polished high fidelity Chinese desktop AI coding assistant interface concept screenshot, landscape 16:10 around 1600x1000, flat front-on, no device frame. Product COOLZHU CODE. Concept C: warm lightweight studio. Warm ivory background #F7F5EF, white message surface, charcoal ink text, muted olive-green accent #6B8066, soft sand dividing lines. Understated friendly and editorial, but no ornament, no cartoon, no gradients, no 3D effects, no other company's logo. Consistent Chinese sans serif, comfortable 16-17px message text. The design should feel very different from a cold grey engineering dashboard.
Critical structure: leftmost only a narrow 56px icon rail, no expanded left conversation navigator. Single row top bar about 52px: small COOLZHU logo, then three concise horizontal dropdowns “Agent 开发助手 ⌄”, “工程 coolzhuagent ⌄”, “聊天室 界面优化 ⌄”; labels and values inline side-by-side, not vertically stacked. Top right quiet “就绪” green dot and window controls. Center conversational workspace, right extension panel around 360px wide. Far left rail monochrome line icons: chat, folder, checklist, terminal, globe, search, chart; settings icon bottom. Chart icon active in soft olive rounded rectangle.
Center: user bubble “把工具状态、搜索和用量信息整理到合适的位置。” Assistant reply as a clean document without heavy bubble: heading “让对话成为主角”, concise Chinese lines “工具过程收进状态栏，完成后保留结果与耗时。” and 3 small check rows “顶部下拉切换会话环境”, “右侧面板按需展开”, “搜索可定位到每一轮消息”. Below a very subtle “本轮完成 · 24.6 秒 · 4 次工具调用”. No completed reasoning block. No raw JSON or tool outputs as messages.
Right extension panel titled “用量统计”, close icon, small segmented “本轮 / 本会话 / 今日” with 本会话 selected. Large “18,640” and label “累计 tokens”, a minimal stacked horizontal bar labeled 输入 / 输出 / 缓存 / 思考. Four tidy values in 2 by 2 grid: “输入 12,420”, “输出 3,180”, “缓存 2,040”, “思考 1,000”. Small disclosure “按模型报告统计 · 思考已包含于输出时不重复累加” in muted text. Below a tasteful minimalist 7-day small vertical bar chart, label “最近 7 天”. Under it 3 short history rows show “模型调试  8.2K”, “工具调用  6.5K”, “界面优化  3.9K”. Bottom card “上下文 26%” with progress line and “当前 8.5K / 上限 32K”.
Immediately above bottom composer a single slim olive tool strip: “✓ 已完成  写入方案文档 · 4 项调用   查看详情 ›”. Compact spacious composer with “继续说说你的想法…”, paperclip and microphone icons, tiny model dropdown and “思考 自动” chips, prominent but small olive send button. A small search icon in message toolbar is visible. Strong hierarchy, generous horizontal breathing room but no giant vertical empty header. All numbers are clearly mock data; do not add spurious dates.
```

### 精修

```text
Edit only the usage statistics section of this interface mockup. Keep the entire screenshot, layout, typography, colors, other text and controls unchanged. Correct the right-hand token data so the total 18,640 is input plus output without double counting their subsets. In the 2 by 2 numbers area make the labels and values exactly: top left '输入' '14,460'; top right '输出' '4,180'; bottom left '缓存（含于输入）' '2,040'; bottom right '思考（含于输出）' '1,000'. The horizontal total usage bar above should have only two main segments labeled '输入' and '输出', remove the four-item legend to prevent subset double counting. Under the four values use the concise note '总量 = 输入 + 输出；缓存与思考不重复累计'. Everything else unchanged. This is concept mock data, no new claims or ornament.
```

