# web-console UI 检视与调整执行方案（2026-07-07）

> 交付对象：codex（通过 codex CLI / Claude Code codex 插件派发执行）。
> 检视方式：index.html（1184 行）全文通读 + app.js/main.rs 交叉核实 + 编译运行后逐窗口截图与 boundingBox 量化（1600×900 视口）。
> 本文档 = 检视结论 + 设计方案 + 分阶段任务书（含边界与验证标准）。

## 一、检视结论（问题清单，均已核实）

### 全局
| 编号 | 问题 | 证据 |
| --- | --- | --- |
| G1 | 底部状态栏是硬编码假信息：「系统状态：正常运行 / 版本：v1.0.0 / 环境：开发环境」，占满宽 40px 高；`global-system-bar`/`system-dot` 在 app.js 中零引用，永不更新 | index.html:1150-1154；app.js grep 无匹配 |
| G2 | 王座猫（throne-cat）死代码：`throne-cat.js` 帧循环持续运行，但 styles.css 中 `.throne-cat { display: none }`，永远不可见 | styles.css:3258-3260；index.html:53-55、1181 |
| G3 | 多轮改版遗留钩子：`data-round3-layout` / `data-round3-card` / `data-round3-legacy-layout` 等属性散落各窗口 | index.html 多处 |

### 顶部区（三段 416 / 768 / 416，高 108px）
| 编号 | 问题 |
| --- | --- |
| T1 | Agent 总览卡头部放着「ShowUI 服务启动/停止」开关——ShowUI 是视觉后端服务，与"总览"无关，且与视觉实验窗口职责重叠 |
| T2 | 任务卡片 416×108 内塞 8 组信息（任务名/进度/摘要/4 状态/成功率/待办清单/工具审批/计划），信息密度过载、无主次 |

### 聊天室（默认窗口）
| 编号 | 问题 |
| --- | --- |
| C1 | 左栏「快捷筛选」组（全部/置顶/未读/@我）**没有任何 JS 逻辑**（`chat-filter` 在 app.js 零匹配），是纯摆设，占 57px |
| C2 | 授权配置放在"任务授权"窗口，但 Full access 的生效范围就是**当前聊天室**（app.js:4215 显示 `On · room`，随聊天室保存恢复）——控件与作用对象分离 |
| C3 | 左栏五组（会话操作 116 / 频道模型 243 / 会话选择 125 / 快捷筛选 57 / 接收者 69）恰好塞满 642px，无主次、无弹性 |

### 工程目录
| 编号 | 问题 |
| --- | --- |
| P1 | **双搜索框并存**：`ide-omni-search`（支持 `@函数` `:行号`，被挤到 151px 宽，连自己的占位文案都显示不下）+ 旧 `project-search`（整行 358px，仅过滤文件名）——功能重叠 |
| P2 | 死控件：`ide-mode-toggle-legacy` 按钮、`project-diff-drawer` 抽屉均 `hidden` 且被新 Diff 模式取代 |
| P3 | 未选文件时右侧 1170×643 预览区整片空白，只有一行英文提示 |
| P4 | 已有 IDE 底子（多文件标签 + LRU 驱逐、符号搜索 `/api/project/symbols`、行跳转/高亮、目录树 401 项、拖拽分栏），但缺市面 IDE 常用能力：当前文件**大纲/符号列表**、**面包屑路径**、树节点**右键定位/复制路径**、空态**最近文件** |

### 设置
| 编号 | 问题 |
| --- | --- |
| S1 | TTS/STT 列暴露纯开发调试控件：「模型流探活」「流式TTS探活」按钮、整套 barge-in 打断评估面板（场景下拉+样例文本"等一下 不对"+评估按钮+结果 pre）——普通使用永远用不到 |
| S2 | 工具详情（legacy 区）内的 Semantic dispatch dry-run + scenario matrix 同属调试工具，与"工具与审批"的配置定位不符 |

### 微信连接
| 编号 | 问题 |
| --- | --- |
| W1 | 右栏命令列表卡高 **2306px**（24 条命令逐条纵排），把「命令试跑」推到 y=2514、「事件预览」推到 y=2911——在 633px 视口里位于 3 屏之外，等于不存在 |
| W2 | 「命令试跑（Dry apply）」是调试工具，却与业务卡片平铺 |
| W3 | 主控制台纵向堆 1431px（管理员 154 + 私聊文件/任务 460 + 绑定表单 172 + 绑定列表），超可视区 2.3 倍，Bridge Map 主题（绑定）反而被淹没 |

### 任务授权
| 编号 | 问题 |
| --- | --- |
| A1 | 窗口混装 4 类职责：Goal 角色 / 定时任务 / 授权配置 / 模块自检+Self update。授权迁出（→聊天室）后需要重组 |

### 记忆知识 / 视觉实验 / 浏览器 / 终端
布局均衡（三列/纵排无溢出、无死功能），本轮**不动**；视觉实验仅接收 ShowUI 开关迁入。

## 二、设计原则

1. **一窗一主焦点**：每个窗口有且只有一个视觉权重最高的主区（面积最大、对比最强），辅助功能收进折叠卡/抽屉/弹层，不与主区平铺抢戏。
2. **控件跟着作用对象走**：授权作用于聊天室→进聊天室；ShowUI 服务于视觉→进视觉实验。
3. **调试能力统一收口**：探活/dry-run/评估类工具一律收进「任务中心→诊断」区，业务窗口零调试控件。
4. **死功能删除而非隐藏**：无后端/无逻辑的前端元素直接删（快捷筛选、假状态栏、legacy diff）。
5. **密度节奏**：卡片内信息 ≤3 层（标题/主值/辅注），超出的进 hover 或次级页；列表类必须限高+内部滚动。

## 三、分窗口调整方案

### 3.1 全局
- **G1**：`global-system-bar` 改为真实状态：`工作区路径 · 监听端口 · 构建版本(git short hash + 构建日期) · 活跃会话数`。后端在 main.rs 增加 `GET /api/system/info`（编译期 `env!` 注入版本，运行时取 workspace/端口/会话数）；若不愿动后端，则整条删除（二选一，倾向前者）。
- **G2**：移除 index.html 对 `throne-cat.js` 的引用与 `.throne-cat` DOM/CSS（素材 PNG 保留在仓库）；banner 动画由竹叶方案接替（见第四节）。
- **G3**：删除 `data-round3-*` 属性（纯清理，不改布局行为；grep 确认 app.js/styles.css 无选择器依赖后再删）。

### 3.2 顶部区
- **T1**：ShowUI 服务开关（`data-role="showui-service-control"`）从 Agent 总览头部迁到**视觉实验窗口**操作列顶部（Capture 卡上方独立小条）。总览头部右侧改放当前工作区名（点击复制路径）。
- **T2**：任务卡片瘦身为三层：`任务名 + 进度条`（主）、`4 状态点计数`（辅）、`工具审批徽标`（仅有待审批时显示，高亮）。摘要、成功率、todo 清单、计划数移入「任务链」弹层顶部（点击既有任务链按钮可见）。

### 3.3 聊天室
- **C1**：删除 `chat-quick-filters` 整组（HTML+CSS）。
- **C2**：左栏底部新增「会话授权」组（替代快捷筛选的位置）：
  - 一行状态徽标：`默认 / 外部目录 N / Full access ON`（颜色区分，Full access ON 用警示色）；
  - 一个开关按钮：Full access 开启/撤销（复用 app.js 既有 `enableFullAccessGrant` / revoke 逻辑与确认弹窗）;
  - 一个「管理…」链接按钮：打开弹层（dialog）承载原「workspace 外目录」授权卡的完整表单与列表；
  - 原任务窗口 `task-config-column`（授权配置列）整列移除，相关 DOM 绑定（`authorization-selected-room`、`allowed-root-*`、`full-access-*`、`tasks.fullAccessStatus`）全部指向新位置，**JS 逻辑不重写，只换挂载点**。
- **C3**：左栏主次重排：`会话选择`置顶（主，触发器加大）→ `频道/模型`（flex:1 弹性占剩余高，内部滚动）→ `当前接收者` → `会话授权`（新增）→ `会话操作`收纳为一行图标按钮（新建/重命名/删除/任务链/转交/更早/删除所选 → 图标+tooltip，两个"删除"类放溢出菜单）。

### 3.4 工程目录
- **P1**：删除旧 `project-search` 输入框；`ide-omni-search` 占满工具行剩余宽度（≥260px），占位文案保持「搜文件 @函数 :行号」。
- **P2**：删除 `ide-mode-toggle-legacy`、`project-diff-drawer` 及关联 CSS/JS 引用。
- **P3**：预览空态改为「最近打开」：列出 ide tab 历史（复用 tab 状态存储）最多 8 条 + 快捷键提示（`Ctrl+P 搜索文件`、`@ 搜符号`）。
- **P4**（IDE 补足，市面 IDE 对齐）：
  1. **大纲侧条**：预览面板右侧 200px 可折叠「大纲」，列当前文件符号（函数/结构体/impl），点击跳行。数据：`api_project_symbols` 增加可选 `path=` 过滤参数（main.rs 内该 handler 已读 symbols.json，按 file 字段过滤即可）；
  2. **面包屑**：`project-file-meta` 行升级为可点击路径面包屑，点击某级在目录树中展开定位；
  3. **树节点操作**：右键菜单（或悬停 ⋯）：在树中定位/复制相对路径/复制绝对路径/在 Diff 中打开；
  4. 预览内 `Ctrl+点击` 标识符：取词→调 symbols 查询→唯一命中直接跳转，多命中弹 omni 结果列表。

### 3.5 设置
- **S1**：删除 barge-in 评估面板（`realtime-barge-panel` + 结果 pre）；「模型流探活」「流式TTS探活」两按钮迁到任务中心·诊断区（见 3.7）。TTS/STT 列保留：状态行、音色选择、朗读回复、实时开始/停止、开始/停止听写。
- **S2**：Semantic dispatch dry-run 卡 + scenario matrix 迁到任务中心·诊断区；工具清单行「Semantic Dispatch Plan」保留但"计划配置"按钮跳转到诊断区对应卡。工具调用审计保留在工具详情内。

### 3.6 微信连接
- **W1**：命令列表改**双列紧凑网格**（命令名等宽字体 + 简述小字同格），整卡限高 320px 内部滚动，卡头加「N 条命令」计数。
- **W2**：「命令试跑」+「事件预览」合并为一张折叠卡 `<details>`「调试与事件」，默认收起。
- **W3**：主控制台改**分段视图**（segmented control 三段）：`绑定管理`（默认，绑定表单+绑定列表——Bridge Map 主区）/ `私聊操作`（文件读写+任务管理）/ `管理员`（认领面板+回传开关）。每段内容互斥显示，消除 1431px 纵向堆叠。

### 3.7 任务授权 → 任务中心
- **A1**：授权列迁出后重组为两列：
  - 左列「Goal 与计划」（保持现有：角色分配 / Goal 运行配置 details / 定时任务）；
  - 右列「诊断」＝ 原模块自检 + 修复建议 + Self update，**追加**迁入的：模型流探活、流式TTS探活（S1）、Semantic dispatch dry-run + scenario matrix（S2），各自成小卡，默认折叠。
- **A2**：dock 标签「任务授权」改名「任务中心」（button title 与 span 文案同步改）。

## 四、Logo 区竹叶动画——「晚风竹雨」

**现状**：banner 为静态 PNG（770×116：夜竹林 + 左上月亮 + 竹叶拼成的 COOLZHU CODE + 右侧持剑少女背影，画面已有少量静止飘叶）；王座猫动画被 CSS 隐藏（本方案将其移除，见 G2）。

**意境**：夜里一阵晚风穿过竹林，竹叶擦着月光落下，掠过 logo 字面，消散在画面左下。

### 4.1 素材（由 codex image gen 生成，共 4 张 + 1 张备用）
统一规格：**透明背景 PNG，128×128**，风格与 `assets/ui-redesign/bamboo-leaf-banner-v1.png` 一致——夜色冷调水墨动漫风、深青绿叶身（#2f4a35 至 #4a6b4a 区间）、受月光的一侧有细窄的银白高光边（#cfe0d8），轮廓干净无杂色，笔触有轻微水墨晕染感。生成 prompt 模板：

> A single bamboo leaf sprite for a game UI particle effect, transparent background, dark ink-wash anime style, deep cool green (#3a5a40) leaf lit by pale moonlight with a thin silver rim-light on one edge, clean silhouette, slight ink-wash texture, 128x128, centered, no ground, no other objects. Pose: {姿态}

四种姿态（{姿态} 依次替换）：
1. `flat leaf seen from the side, gently curved`（平展侧视微弯）→ `leaf-01-flat.png`
2. `curled leaf, tip twisted, foreshortened`（卷曲透视）→ `leaf-02-curl.png`
3. `small twig with two leaves`（双叶小枝）→ `leaf-03-twig.png`
4. `narrow leaf almost edge-on, thin dark silhouette`（近侧影窄条）→ `leaf-04-edge.png`
5. 备用：`leaf spinning, motion-blurred hint`（带动势模糊）→ `leaf-05-spin.png`（若前四张动画后单调再用）

存放：`modules/gui-web/packages/web-console/assets/ui-redesign/bamboo-leaves/`。
验收：透明通道干净（无白边/杂点）、色调与 banner 同框不突兀（叠图目测）。

### 4.2 动画实现（纯前端，新文件 `assets/bamboo-leaves.js`）
- **载体**：`.brand-banner` 内叠加绝对定位 `<canvas>`（尺寸随容器，`pointer-events:none`，z-index 在 banner 图之上）。
- **粒子模型**（同屏上限 12，常态 5~8）：
  - 出生：banner 右侧 78%~100% 宽度带 + 顶缘（对应画面竹丛），随机选 4 素材之一，初始缩放 0.12~0.28（显示 15~36px）、随机初始旋转；
  - 运动：水平速度向左 8~20px/s + 垂直下落 6~14px/s；横向叠加正弦摆（振幅 6~14px，周期 2.4~4s）；旋转角速度与摆动相位耦合（摆到端点转得慢，模拟迎风翻叶）；
  - **阵风事件**：每 8~15s 随机触发一次，持续 1.2~2s：存量粒子水平速度 ×2.2、新生成率 ×3，并整体加一个 6° 左倾——「一阵风来」的节奏感；
  - 月光带（x<22% 区域）：粒子 alpha 提到 0.9 并轻微提亮（globalCompositeOperation 不变，仅换预先调亮的帧或 filter:brightness——实现从简，允许只调 alpha）；
  - 消亡：越过左缘 / 落到 72% 高度以下渐隐（0.6s alpha→0）。
- **性能与可达性**：`requestAnimationFrame` 驱动；`document.hidden` 时暂停；`prefers-reduced-motion: reduce` 时不启动动画（保持静态 banner）；素材预加载失败则整体静默退出（不报错、不影响其他功能）。
- **接入**：index.html 中以 `<script type="module" src="./assets/bamboo-leaves.js">` 替换原 throne-cat.js 引用；main.rs 若对 assets 目录按文件名路由需确认新文件与子目录可被服务（现有 assets 服务机制照走）。

## 五、执行阶段划分（codex 任务书）

> **通用硬约束**（每阶段适用）：
> 1. 仅允许改动 `modules/gui-web/packages/web-console/` 下的 `index.html`、`src/app.js`、`src/styles.css`、`assets/**`；Phase 1 的 G1 与 Phase 5 的大纲 API 额外允许改 `src/main.rs` 中明确点名的 handler；**不得**动其他 crate、不得改测试断言来通过验证、不得新增第三方依赖。
> 2. 中文 Windows 环境，文件一律 UTF-8 无 BOM。
> 3. 每阶段完成后必须执行并通过：
>    `cargo build -p coolzhu-web-console --offline --target-dir modules/gui-web/target`
>    `cargo test -p coolzhu-web-console --offline --target-dir modules/gui-web/target`
>    （编译前确认无运行中的 coolzhu-web-console.exe 实例锁住产物）
> 4. 每阶段独立 git commit（中文提交信息，格式 `ui: <阶段摘要>`），改动范围外的文件出现在 diff 中即视为越界，需回退。
> 5. app.js/styles.css 体积大（266KB/105KB），编辑前先精确定位上下文，禁止凭记忆写符号名。

- **Phase 0｜竹叶素材生成**（codex image gen）：按 4.1 生成 5 张 PNG 入库。验收：文件存在、透明通道正确、与 banner 叠图色调协调。
- **Phase 1｜冗余清理**：G1（真实状态栏 + `/api/system/info`）、G2（移除 throne-cat 引用）、G3、C1、P1、P2。验收：grep 确认被删元素的 data-role/class 在三个前端文件中零残留；页面九窗口功能无回归。
- **Phase 2｜授权迁移 + 任务中心**：C2、C3、A1、A2。验收：聊天室左栏可开关 Full access 且状态与后端一致（刷新后保持）；任务中心两列布局；原授权列零残留。
- **Phase 3｜设置瘦身 + 诊断收口**：S1、S2 及其在任务中心·诊断区的落位。验收：设置窗口无任何"探活/评估/dry-run"字样；诊断区新卡可用。
- **Phase 4｜微信连接重排**：W1、W2、W3。验收：1600×900 下右栏三卡全部首屏可见；主控制台无纵向翻页即可完成"看绑定列表"主任务。
- **Phase 5｜IDE 功能补足**：P3、P4（含 main.rs `api_project_symbols` 增加 `path` 过滤参数）。验收：打开 .rs 文件出现大纲并可跳转；面包屑定位正常；空态显示最近文件。
- **Phase 6｜竹叶动画**：按 4.2 实现（依赖 Phase 0）。验收：动画流畅无掉帧感、CPU 占用无异常；`prefers-reduced-motion` 与后台标签页暂停生效；关闭动画文件加载失败时页面无报错。
- **Phase 7｜顶部区调整**：T1、T2。验收：ShowUI 开关在视觉实验窗口可用（启停状态同步）；任务卡片三层结构，审批徽标仅在有待审批时出现。

阶段顺序即依赖顺序（Phase 0 与 Phase 1~5 可并行，Phase 6 依赖 Phase 0）。单阶段失败不阻塞后续独立阶段，但需在 commit 信息中注明遗留。
