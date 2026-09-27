# B 动作与卷框生产素材提示词

## 用户原稿纠偏：顶部字标与竹林底图

2026-09-27 使用内置 `image_gen`，以用户原始设计图 [原稿](assets/original-scroll-ui-user-reference.png) 为输入生成两个独立图层。两项均为参考衍生素材，不是原图无损抠取；接入后的真实窗口截图才是视觉验收依据。动画 B 和既有卷轴装饰保留。

- [金玉字标](assets/coolzhu-jade-wordmark-v1.png)：2103×748，透明背景；SHA256 `E3887921DA8BACFA15421592F37B5F87506815FDDADBEDB08640FA8F87CB2C6B`。生成输出 `exec-6b9a13a7-7222-45d8-b101-6ffc350a9a13.png`。实际上下透明留白较大，前端应按可见字形安排局部显示区域，避免整图缩进矮容器后字形过小。
- [竹林背景](assets/original-bamboo-background-v1.png)：1672×941，不透明背景；SHA256 `4CC52FD6154024269B45801C35541CDE2CD8E87C336F43A08C8919BDCBE7A7FF`。生成输出 `exec-50f51bf5-0a57-4c17-9aa3-99d2aadcd9a7.png`。仅用作底图，界面控件、字标、卷轴由各自图层提供。

字标原始提示词（`transparent_background=true`）：

```text
Use case: background-extraction. Edit target: supplied original COOLZHU app UI. Extract and faithfully recreate ONLY its top-center logo as a production transparent PNG, wide horizontal and tightly framed. Exact seven-letter uppercase text COOLZHU (C O O L Z H U), original slender carved antique-gold outlines with dark emerald jade inlay, and restrained symmetric gold flourish each side. Match original, do not redesign or use block type, NO CODE suffix. Remove all application UI, bamboo, backdrop, scroll rollers, buttons, frames, and all other text. Genuine transparent background, no fake checkerboard. Crisp detail recognizable at 240px wide and 35px high. Small even transparent margins, logo fills most width.
```

背景原始提示词（`transparent_background=false`）：

```text
Use case: precise-object-edit, clean background extraction. Edit target: exact supplied original COOLZHU app design. Produce clean 16:9 production wallpaper: remove ALL UI, all text including logo, icons, buttons, top bar, rail lines, borders, chat cards/messages, composer, and both jade/gold scroll rollers. Preserve/restore ONLY its original near-black dark emerald bamboo background: elegant realistic bamboo leaves along top and outer left/right sides and bottom corners, subdued layered foliage, subtly mottled ink-jade empty center. Keep reference muted jade/antique olive colors, leaf arrangement, wuxia atmosphere and very dark lighting. No moon, mountains, lake, figures, throne, new objects, ornaments or typography. Central 75 percent quiet and dark enough for light text; branches mostly perimeter, with visible leaves along inner side edges. This is a background layer behind actual HTML controls, not a UI mockup. No panels, transparent holes, fake frames, washed out center, no scroll rollers.
```

2026-09-27，内置 image_gen，两项均要求透明背景，未使用 CLI。生成图需经 alpha 与真实运行验证后才能作为生产资产；源文件保留，采用版本化目标路径。

## 卷框

来源：`exec-d275e2cf-f9d0-4d4e-b9ef-01c4c0b93321.png`，参考仓库无字山河卷轴。原始生成提示词：

Production UI decorative asset, transparent PNG, landscape 3:2. Reference image is style reference for green jade scroll rollers and antique gold craftsmanship, not a background to copy. Create ONLY a fine rectangular scroll surround: two slim dark emerald jade cylindrical rollers at left and right, small restrained gold endcaps, narrow aged-gold horizontal mounting lines at top/bottom, small jade ornament centered on each horizontal line. Front view perfectly rectilinear. All center and all exterior space fully transparent alpha, no landscape, no text, no logo, no character, no grid/checkerboard. Frame occupies outer 5% on each side, horizontal edging outer 3%, inner 88% width by 88% height fully empty transparent. Roller diameter consistent, actual dimensional jade highlights and subtle carved bamboo cloud grain. Compact daily workbench version of reference; elegant restrained gold, not massive dragon heads. Keep continuous long straight repeatable edges suitable nine-slice CSS border-image, opaque decorative edges with antialiased alpha and no inner shadow extending into content. Symmetrical rounded endcaps all fully within canvas. Deliver a usable isolated texture asset, not a UI mockup.

## 人物四姿态

来源：`exec-595d38b1-783d-4c4f-a71e-353fbda77439.png`，参考原 `actors/C-k2.png`。原始生成提示词：

Production 2D animation sprite sheet on genuinely transparent alpha, landscape 4:1 with EXACTLY FOUR equal square cells side by side, no borders or labels. Reference is character identity/style source: preserve same chibi Chinese wuxia swordsman, dark brown high ponytail with pale tie, amber eyes, white long tunic and cape, gold dragon shoulder armor, black trousers, silver/gold boots, red sword blade with gold hilt and red tassel. Same character, face, proportions, lighting, costume and weapon in all four cells, 3/4 facing screen right. Full body including ponytail and sword fully contained in each cell with 8% safety margin; no overlapping cells. Cell1 deep crouched compression/anticipation, feet grounded, sword prepared behind. Cell2 rising airborne with both feet visibly off ground, knees tucking, torso higher, sword arcs overhead in preparation. Cell3 apex forward airborne diagonal sword cut, one leg tucked one extended, torso rotated, cape follows. Cell4 low landing with one planted foot and bent knee, sword finishing low to right, cape settling. Each pose is SINGLE solid character, clean silhouette; no ghosted duplicate limbs, no motion blur, no light trails, no ground/platform/background. A red blade is okay but no large glow halo. Hand-painted polished RPG chibi sprite aesthetic matching reference, not photorealistic. This sheet will be sampled as four pose crops at runtime, so ensure each subject occupies similar scale in an exact uniform four-cell grid. No logo/no words/no scenery.

生成输出人物图实际为 2172×724，四格按实际尺寸采样，不能假定正方格。它补充腾空关键姿态，仍不是完整逐帧动作序列。最终运行资产路径与校验结果由实现报告记录。

生成原图已保存在仓库设计目录：[卷框](assets/scroll-frame-v1.png)、[四姿态](assets/swordsman-four-poses-v1.png)。保留原 alpha，未作程序抠图；这两个路径为设计源资产，接入生产前仍需检查透明度与采样边界。

## 四姿态第二版：修正分格与剑尖

V1 经 Sol 像素检查发现格间跨界及最后剑尖抵住图像末列，不能直接四等分使用。内置 image_gen 以 V1 为编辑输入生成 [V2](assets/swordsman-four-poses-v2.png)，源文件 `exec-fa9d01c5-317d-4f6f-b6a5-1dfec7e38e9a.png`。实际裁格仍需按透明间隙确定，不能假设生成器严格满足等分指令。提示词：

Correct this production animation sprite sheet. Preserve EXACTLY the same character design, four poses, costume, sword, art style, and left-to-right order. Change ONLY spacing and scale so every pose including its entire sword, hair, cape is fully contained in its own isolated cell. Output wide 3:1 canvas divided into FOUR equal-width INVISIBLE cells. Shrink each complete character+weapon silhouette to at most 70% of its cell width and 72% of canvas height. Each cell MUST have at least 15% of cell width entirely transparent on both left and right, including sword glow. No pixel from one pose enters another cell. Use four evenly spaced centers at 12.5%,37.5%,62.5%,87.5% of canvas width. All four characters same anatomical scale. COMPLETE the final pose's currently clipped sword tip using reference identity; all four full sword blades must be visible. No subject touches any outer image edge. True transparent alpha throughout background and generous inter-cell gutters, no black/checkered background, no floor, no labels, no cell outlines, no shadows, no ghost limbs, no trails. Four poses remain crouch anticipation, knees-tucked rising leap with sword overhead, airborne forward sword strike, low grounded landing. This is a clean game sprite atlas, NOT a poster. Preserve the same dark brown ponytail chibi swordsman in white cape and gold dragon armor from image1. Production priority is safe transparent margins.
