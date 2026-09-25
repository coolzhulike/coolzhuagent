# 三栏武侠控制台资源包

生成日期：2026-08-24
生成方式：OpenAI 内置 ImageGen；透明通道、切片与固定尺寸由本地脚本做确定性后处理。

## 资源定位

本目录用于“三栏非对称聊天室”方案。运行时小尺寸按钮优先使用 `assets/icons-wuxia/` 中的 SVG；本目录 PNG 用于高辨识度主操作、状态动效、空状态和高分屏展示。

| 文件 | 尺寸 | 用途 |
|---|---:|---|
| `wuxia-controls-atlas-v1.png` | 1280×1280 | 4×4 武侠控件总览与设计基准 |
| `wuxia-controls-atlas-v1.json` | — | 图标名称、切片坐标与文件映射 |
| `control-icons/*.png` | 256×256 | 16 个透明底独立控件图标 |
| `rail-lantern-gold-v1.png` | 256×384 | 右栏运行/等待状态金色灯笼 |
| `jade-success-sweep-v1.png` | 1536×512 | 成功完成时的一次性玉光掠过 |
| `vermilion-seal-blank-v1.png` | 256×256 | 审批、拒绝、确认状态的朱红印记底板 |

## 16 个 ImageGen 控件

从左到右、从上到下：

1. `panel-left-open`、`panel-left-close`、`panel-right-open`、`panel-right-close`
2. `queue`、`steer-now`、`pause`、`resume`
3. `checkpoint`、`restore-code`、`restore-chat`、`restore-both`
4. `approve-once`、`approve-rule`、`reject-feedback`、`focus-layout`

注意：`pause` 的产品文案必须是“暂停后续阶段”；当前后端尚不支持把正在执行的模型/工具冻结后继续。`resume` 对应恢复调度，不应复用为“载入历史会话”。

## 新增 SVG 控件

为 16–24 px 高频控件补充以下矢量资源：

- `plus.svg`、`pin.svg`、`more-vertical.svg`
- `branch.svg`、`worktree.svg`
- `check.svg`、`archive.svg`
- `maximize.svg`、`split-pane.svg`
- `queue.svg`、`context-ring.svg`

它们延续现有 `#b9dfcf` 玉青、`#d7b35a` 金色、`#6f8f83` 青灰体系；无固定宽高、无文字、无外框，可由按钮容器统一控制尺寸和交互状态。

## ImageGen 提示词摘要

### 控件图集

生成透明底、4×4 等距网格的武侠 GUI 控件图集；材质限定为墨绿玉石、竹节、旧金、象牙白和少量朱红；每格一个独立图标，依次表达左右栏开合、队列、立即引导、暂停、恢复、检查点、三类恢复、两类批准、拒绝反馈和专注布局；统一三分之四视角、漆面高光与细金边；禁止文字、数字、Logo、水印和外框。

### 金色灯笼

生成单个透明底金色竹节灯笼，带暖金灯芯、玉珠和短流苏；正面视角、轮廓简洁、中心留出状态光源空间；保留漆器光泽，禁止文字、场景背景和投影地面。

### 玉光掠过

生成透明底横向玉青能量丝带，细雾、竹叶粒子与柔和高光从左向右收束；用于成功状态的一次性 220–360 ms 掠光；禁止文字、徽章和实体背景。

### 朱红印记

生成透明底空心朱红印章底板，略不规则的古印边缘、漆面高光和少量金粉；中心完全留空，供 DOM/SVG 覆盖勾、叉或审批文字；禁止内置字形、Logo 和背景。

## 视觉与动效约束

- 金色灯笼不是纯装饰：空闲 3.6 s 轻呼吸；运行 1.8 s；等待审批先连续两次明显脉冲后转为稳定暖光；错误状态停止呼吸并降低饱和度。
- 玉光只在成功事件边沿触发一次，不循环，建议 260 ms；普通运行过程使用 CSS 渐变，不持续播放位图动画。
- 朱红印记进入时 240 ms 缩放/盖印一次，随后静止；DOM 覆盖层负责语义图形与无障碍文本。
- `prefers-reduced-motion: reduce` 下取消位移、旋转和连续呼吸，只保留静态亮度差。
- 图标按钮必须保留可见焦点环、`aria-label`、Tooltip 与至少 36×36 px 点击热区，不能以图案替代文字状态。

## 聊天背景复用

聊天室半透明背景继续复用已有分层素材：

- `../banner-layers/layer4-logo-leaftext.png`：居中偏上，建议 8%–12% 不透明度。
- `../banner-layers/layer5-figure.png`：右下偏置，建议 10%–15% 不透明度。
- `../bamboo-wuxia-stage-bg-v1.png`：只取低频山雾与竹影，建议 5%–9% 不透明度。

背景层必须位于消息气泡之下并设置 `pointer-events: none`；输入区下方增加实色/毛玻璃遮罩，保证长文本与代码块对比度。

## 使用边界

- 细节丰富的 PNG 不应直接缩到 14 px；小尺寸必须使用 SVG。
- 不再把旧 Mario、飞船舰桥、王座和办公室资源混入本布局。
- 不恢复旧 `bridge3d.js` 的长按/拖拽切页交互；横幅竹叶微动可继续使用已有 `bamboo-banner-wind.js`，但要限制作用域并遵守减弱动态设置。
