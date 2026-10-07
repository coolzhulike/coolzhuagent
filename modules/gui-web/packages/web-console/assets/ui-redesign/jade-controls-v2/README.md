# 玉石控件第二版

2026-10-07用户选择“采用当前方向”。使用内置ImageGen生成，非CLI；以下三个原始透明PNG已复制入仓库，保留原始alpha，没有裁剪、拼接或重绘。参考方案为本会话已展示的玉石按钮、细金线图标方案。运行时由`src/jade_controls.css`统一尺寸，既有SVG负责按钮的功能符号，状态使用本目录的新玉佩、灯笼。

| 文件 | 用途 | 消费尺寸 |
| --- | --- | --- |
| `button-skin.png` | 共用玉石按钮框 | 输入栏/更新44px，低高度36px；状态框30px |
| `status-jade.png` | 系统健康状态玉佩 | 21px，保留原状态语义 |
| `status-lantern.png` | 工作状态及异常灯笼 | 21px，保留原显示与警示规则 |

上传、发送、麦克风、更新使用同一框；不为状态图增加无效点击动作。功能按钮保持纯图标，悬停与无障碍名称保留。背景竹林、卷轴和COOLZHU Logo未换图。

生成提示词集合（生产规格归档）：

1. **button-skin.png** — Use case: ui-mockup. Asset type: a single reusable empty jade UI button skin. Reference: approved jade-and-gold wuxia controls. Square 1024 canvas, centered rounded-square button occupying about 90% of canvas, dark emerald polished jade, restrained fine gold outline and small gold corner details, subtle upper highlight and low relief. Empty center, no letters, no symbols, no icons, no text, no scene, no shadow outside the silhouette. Truly transparent background; preserve alpha. Must remain legible when displayed at 44px, match bamboo-and-jade desktop theme.
2. **status-jade.png** — Use case: stylized-concept. Asset type: standalone small status medallion. A circular carved dark emerald jade medallion, centered and occupying about 80% of a square canvas, simple broad bamboo engraving, restrained gold accents, polished jade depth with clear silhouette readable at 22px. No button frame, no text, no letters, no scenery. Truly transparent background; preserve alpha. Match the approved jade UI control direction.
3. **status-lantern.png** — Use case: stylized-concept. Asset type: standalone small status lantern. A compact vermilion red Chinese lantern with fine warm gold ribs, top hanger and short tassel, centered on square canvas, about 80% canvas height and 65% width. Simple readable silhouette at 22px, restrained ornament, dark jade-and-bamboo UI palette. No button frame, no text, no letters, no scenery. Truly transparent background; preserve alpha.

对应ImageGen原始输出：`exec-740a8764-2de2-4a6b-a24e-1668dbfdbe01.png`、`exec-f9d36104-99e3-4030-a177-4cace936496b.png`、`exec-204f4619-81d4-4337-af01-bf148f9ecd2b.png`。原生软件截图及验收范围见[本轮报告](../../../../../../../docs/testing/2026-10-07-jade-connectors/report.md)。
