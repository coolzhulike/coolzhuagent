# CoolzhuAgent 图标设计稿（2026-08-12）

本目录仅保存 2026-08-12 的历史图标设计母版与旧派生资源，不再是活动运行图标链；旧 PNG/ICO 保留用于历史追溯。当前应用安装、启动器、Tauri、desktop-console、托盘与 WiX 图标统一见 `docs/design-assets/coolzhu-icons-2026-08-27/final/` 及其 manifest。

## 风格依据

- Web Console 主色：深海军蓝 `#071622` / `#102535`、金黄 `#FFC526`、暖橙 `#FF9F16`、青蓝 `#1FB9FF`、米白 `#F6F7EF`。
- 视觉语言：16-bit 像素游戏 HUD、深色终端面板、金色阶梯描边、青蓝信号节点、强轮廓。
- 安装器图标以“向下部署到终端托盘”区分；应用图标以“终端提示符与信号网络”区分。

## 产物

| 用途 | 透明 PNG | ICO | 尺寸 |
| --- | --- | --- | --- |
| 安装包 / 安装器 | `coolzhu-installer-icon.png` | `coolzhu-installer-icon.ico` | PNG：1254×1254 RGBA；ICO：16、24、32、48、64、128、256 px |
| 安装后的应用 | `coolzhu-application-icon.png` | `coolzhu-application-icon.ico` | PNG：1254×1254 RGBA；ICO：16、24、32、48、64、128、256 px |

## 生成与后处理

- 生成方式：内置 `image_gen`，未使用 CLI fallback。
- 透明方式：先生成纯色键背景，再使用 `remove_chroma_key.py --auto-key border --soft-matte --transparent-threshold 12 --opaque-threshold 220 --despill` 去背。
- 安装器 PNG：主体边界 `(178, 163, 1076, 1077)`；四角 alpha 均为 0；部分透明像素 3340；未检出绿色色边。
- 应用 PNG：主体边界 `(189, 170, 1065, 1077)`；四角 alpha 均为 0；部分透明像素 1957；未检出绿色色边。
- 视觉检查：两款均仅含精确大写 `CZ`，无水印和额外文字；32px 缩略图下缩写与两种用途仍可辨识。

## 最终提示词：安装器图标

```text
Use case: logo-brand
Asset type: Windows installer icon, square master icon
Primary request: Create an original compact pixel-art badge for the CoolzhuAgent installer. A bold, highly legible uppercase monogram "CZ" is the central and largest element. Integrate a minimal installer/deployment cue: one small downward arrow entering a thin terminal tray beneath the monogram. Surround the badge with only three tiny cyan signal nodes connected by short circuit-like segments.
Scene/backdrop: perfectly flat solid #00ff00 chroma-key background for local background removal.
Subject: one centered rounded-square deep navy terminal badge with a thick stepped golden-yellow pixel border; exact text "CZ" in large blocky geometric capitals, gold-to-warm-yellow face with dark navy inset shadow; small cyan signal accents; compact download-to-terminal motif.
Style/medium: polished 16-bit pixel-art game HUD icon, consistent with a dark technology console, simple vector-friendly geometry, crisp hard pixel steps, strong silhouette.
Composition/framing: centered single icon, square 1:1, generous uniform padding, front-facing, no perspective mockup, all important details kept inside the central 75%.
Color palette: deep navy #071622 and #102535, golden yellow #FFC526, warm orange #FF9F16, cyan #1FB9FF, off-white #F6F7EF; never use green in the subject.
Text (verbatim): "CZ". Render exactly two uppercase Latin letters C then Z, no punctuation, no spacing, no other letters or words.
Constraints: The letters "CZ" must remain instantly readable at 32px; use a thick high-contrast outline; one icon only; compact symmetric silhouette; no tiny decorative clutter. Background must be exactly one uniform #00ff00 color with no shadows, gradients, texture, reflections, floor plane, or lighting variation. Subject fully separated from the background with crisp edges. No cast shadow, contact shadow, reflection, watermark, brand name, slogan, extra text, question marks, robot character, or photorealism.
Avoid: misspelled letters, duplicated characters, ornate serif typography, thin strokes, green inside the icon, 3D product mockup, realistic lighting.
```

## 最终提示词：应用图标

```text
Use case: logo-brand
Asset type: installed CoolzhuAgent Windows application icon, square master icon
Primary request: Create an original compact pixel-art application badge for CoolzhuAgent. A bold, highly legible uppercase monogram "CZ" is the central and largest element. Integrate an unmistakable minimal code-agent console cue: a small terminal prompt glyph made only from a cyan chevron and underscore beneath the monogram, plus four tiny cyan signal nodes connected around the badge like a restrained neural/circuit network.
Scene/backdrop: perfectly flat solid #00ff00 chroma-key background for local background removal.
Subject: one centered rounded-square deep navy terminal badge with a thick stepped golden-yellow pixel border; exact text "CZ" in large blocky geometric capitals, warm off-white face with golden-yellow edge and dark navy inset shadow; cyan terminal prompt and signal nodes.
Style/medium: polished 16-bit pixel-art game HUD icon matching a dark technology console, clean compact geometry, crisp hard pixel steps, strong silhouette.
Composition/framing: centered single icon, square 1:1, generous uniform padding, front-facing, no perspective mockup, all important details inside central 75%.
Color palette: deep navy #071622 and #102535, golden yellow #FFC526, warm orange #FF9F16, cyan #1FB9FF, off-white #F6F7EF; never use green in the subject.
Text (verbatim): "CZ". Render exactly two uppercase Latin letters C then Z, no punctuation, no spacing, no other letters or words. The terminal prompt is a pictographic chevron and short underscore, not text.
Constraints: The letters "CZ" must remain instantly readable at 32px; strong high-contrast thick outline; distinct from an installer icon by emphasizing the active terminal/signal-network identity, not download or package symbols; one icon only; compact balanced silhouette; no tiny clutter. Background must be exactly one uniform #00ff00 color with no shadows, gradients, texture, reflections, floor plane, or lighting variation. Subject fully separated from background with crisp edges. No cast shadow, contact shadow, reflection, watermark, brand name, slogan, extra text, question marks, robot character, download arrow, installer tray, or photorealism.
Avoid: misspelled letters, duplicated characters, ornate serif typography, thin strokes, green inside the icon, 3D product mockup, realistic lighting.
```
