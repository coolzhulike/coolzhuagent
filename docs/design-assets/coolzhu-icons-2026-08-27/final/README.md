# P5-F 应用图标定稿

本目录保存 COOLZHU CODE / CoolzhuAgent 的最终“C 月门灯芯＋CZ”应用图标及其 Windows 运行派生资源。母版由内置 ImageGen 生成，本轮只用仓库正式脚本做确定性的高质量缩放、ICO 打包和校验；母版文件不覆盖、不重绘。

## 母版与生成记录

- 母版：`app-icon-cz-moon-gate-lantern-v1.png`
- 相对仓库路径：`docs/design-assets/coolzhu-icons-2026-08-27/final/app-icon-cz-moon-gate-lantern-v1.png`
- 规格：1254×1254、RGBA、透明背景；SHA256 见 `SOURCE-MANIFEST.json`。
- 生成模式：内置 ImageGen；无参考图最终重生成。
- 成功输出 ID：exec-1275c8c1-c6a2-413b-877a-8c7569eb34a7。ImageGen 默认生成目录仅作来源记录，生产引用仍是本仓库母版。
- Pillow：12.3.0；缩放滤镜：Lanczos（`Image.Resampling.LANCZOS`）。

最终成功调用的原始提示词如下：

~~~text
Use case: stylized-concept
Asset type: Windows desktop application icon master artwork
Primary request: Create a polished wuxia-themed application icon derived from the Moon Gate Lantern concept, with the exact uppercase monogram "CZ" as the central brand mark.
Scene/backdrop: No scene and no background. Output a genuinely transparent RGBA canvas; all pixels outside the icon silhouette must have alpha 0.
Subject: A compact square dark-emerald bamboo-scroll frame surrounding a circular moon gate. Inside the moon gate is one luminous jade bamboo-leaf sweep and a small warm-gold hanging lantern. Center the exact letters C and Z side by side over the jade light.
Style/medium: High-end painted game UI icon, emerald jade, dark bamboo green, warm antique gold, ivory-jade letter faces, glossy enamel, carved metal ornament, restrained magical glow, crisp production finish.
Composition/framing: Square 1:1 icon, front-facing and geometrically centered, strong compact silhouette, generous transparent outer margin. The CZ monogram occupies about 42% of the inner circle width. Use very bold broad strokes, ivory-jade faces, clean warm-gold rims, and a dark emerald shadow. The letters must remain unmistakably readable at 16–32px. Keep the lantern visible at the lower-left of the inner circle and the jade leaf behind the letters.
Text (verbatim): "CZ"
Constraints: Render exactly one uppercase C followed by one uppercase Z. Genuine alpha transparency only; no checkerboard, no white/black/gray background, no square background plate, no wallpaper, no outer drop shadow, no extra letters, words, numbers, symbols, watermark, mockup, or app-window frame. Preserve a refined bamboo-wuxia identity and the green/gold/ivory color harmony.
Avoid: C resembling G, Z resembling 2, thin strokes, overly ornate calligraphy, tiny letters, cluttered leaves, photographic background, fake transparency grid.
~~~

此前两次带参考图编辑均未采用、未入库：输出被判定为 24bpp RGB，且把棋盘格背景烘焙进图像，不满足透明 RGBA 母版要求。

## 派生资源

正式生成器生成七份 PNG，并以 `256, 128, 64, 48, 32, 24, 16` 的目录顺序打包为 Windows ICO；每份 PNG 均保留 32-bit Alpha。每个文件的尺寸和 SHA256 以机器可读的 `SOURCE-MANIFEST.json` 为准。

正式脚本从仓库根目录运行：

```powershell
python scripts/generate-p5f-app-icon.py
python scripts/validate-p5f-app-icon.py
```

脚本优先使用环境中的 Pillow；仓库已有本地 Pillow 运行时可作为回退。干净环境需要先安装 Pillow。

## 实际运行引用链

- `packages/app-launcher/build.rs`：Windows PE 资源编译引用 final ICO；桌面快捷方式和开始菜单快捷方式执行同一启动器。
- `installer/Product.wxs`：`CoolzhuApplicationIcon` 同时服务安装器快捷方式和 `ARPPRODUCTICON`，添加/删除程序不再使用独立旧安装器图标。
- `scripts/build-msi.ps1`：WiX 构建参数使用 final ICO。
- `modules/gui-desktop/packages/tauri-shell/src-tauri/tauri.conf.json`：`bundle.icon` 精确使用版本化 ICO/PNG；不配置重复的 `app.trayIcon`。
- `modules/gui-desktop/packages/tauri-shell/src-tauri/src/main.rs`：唯一手工托盘使用 Tauri 默认窗口图标；托盘仅在左键 Click 的 Up 事件切换控制台，菜单事件保持可用。
- `modules/gui-desktop/packages/desktop-console/build.rs`：独立将 final ICO 编译为 desktop-console PE 的 RT_GROUP_ICON/RT_ICON 资源。
- `modules/gui-desktop/packages/desktop-console/src/main.rs`：eframe 窗口继续使用版本化 256px PNG；它与 PE ICO 是互补链路。

运行时副本与 final 资源的字节映射、哈希和 ICO 帧顺序详见 `SOURCE-MANIFEST.json`。本轮不改聊天室水印、网页布局或前端四个既有 P5 文件。
