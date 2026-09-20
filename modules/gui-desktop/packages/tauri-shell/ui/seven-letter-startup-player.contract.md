# 七字整人帧启动动画播放器合同

`seven-letter-startup-player.js` 是独立渲染模块，本身不会自动启动；生产启动页由 `launch-performance.html` 依次加载它、manifest 和 `coolzhu-seven-letter-bridge.js` 后创建实例。它只消费 manifest、固定字形母版和整人帧资源，不依赖旧十一字模块，人物不会被拆成部件。

启动页资源路径相对于 Tauri `frontendDist`（`ui/`）解析：七组五帧整人图、六个独立字形 PNG 和卷轴背景均由 manifest 声明。整人图与字形是必需资源；卷轴背景加载失败时退回 manifest 的纯色背景，不能阻断七字演出。

## 最小 manifest

```js
const manifest = {
  version: 1,
  canvas: { width: 1400, height: 800 },
  background: "#0b2d24",
  timing: { introMs: 0, frameMs: 120, stepMs: 180, holdMs: 1200 },
  letters: [
    {
      glyph: "C",
      // 每字独立纯字形源；两个 O 可以把 glyphSource.src 设为同一路径。
      glyphSource: { src: "assets/glyph-C.png", width: 180, height: 240 },
      // glyphRect 是 glyphSource 中的源裁片；slot.glyph 是画布中的固定横向字位。
      glyphRect: { x: 0, y: 0, width: 180, height: 240 },
      slot: {
        glyph: { x: 30, y: 260, width: 180, height: 240 },
        actor: { x: 0, y: 20, width: 260, height: 500, actorX: 130, footY: 720 },
      },
      reveal: {
        mode: "path",
        coordinateSpace: "normalized",
        width: 0.12,
        strokes: [[[0.1, 0.5], [0.4, 0.1], [0.9, 0.2]]],
      },
      frames: [
        {
          id: "C-k0",
          src: "assets/C-k0.png",
          width: 1536,
          height: 1024,
          anchors: {
            foot: [760, 1000],
            swordTip: [220, 460],
            hilt: [890, 650],
          },
        },
        // 继续提供 C-k1 ... C-k4。
      ],
    },
    // 依次为 O、O、L、Z、H、U；每项同样必须有 5 帧。
  ],
};
```

也可以把某个字的 `frames` 换成 3×2 源表：

```js
{
  glyph: "O",
  sheet: { src: "assets/O-sheet.png", cols: 3, rows: 2, cellWidth: 512, cellHeight: 512 },
  frames: [
    { index: 0, anchors: { foot: [256, 500], swordTip: [80, 220] } },
    { index: 1, anchors: { foot: [256, 500], swordTip: [100, 180] } },
    { index: 2, anchors: { foot: [256, 500], swordTip: [120, 150] } },
    { index: 3, anchors: { foot: [256, 500], swordTip: [150, 180] } },
    { index: 4, anchors: { foot: [256, 500], swordTip: [180, 220] } },
  ],
}
```

如果旧素材确实是一张总字形母版，也可以在 manifest 顶层提供 `glyphMaster`，未填写 `glyphSource` 的字会回退到它；但独立 `glyphSource` 优先，播放器不强迫单张总母版。`anchors` 默认以整人帧或源表单格左上角为原点；需要使用整张源板坐标时设置 `anchorSpace: "source"`。`swordTip` 只作为可观测地标返回，不会被播放器拿来反向平移或缩放人物。

`reveal.mode: "path"` 会为每条笔顺建立独立的 Canvas 宽笔刷 mask，按 stroke 数组顺序和折线弧长渐显；不会用整条 path 的 bounding box 提前露出未到笔画。`widthUnit: "normalized"` 时宽度按字位短边归一化，否则按源字形像素缩放。

## 接入方式

```js
const api = window.CoolzhuSevenLetterStartupPlayer;
const instance = api.createStartupPlayer({
  manifest,
  canvas: document.getElementById("my-startup-canvas"),
  onComplete(finalState, finalScene) {
    // 七字已完整显示并进入 hold；在这里切换到后续界面。
  },
});
instance.start();
```

播放器会按 `C/O/O/L/Z/H/U` 的固定 `slot.glyph` 横向位置推进：已完成字形持续绘制，当前字形按 `reveal` 进度显示，角色在字位之间按 `stepMs` 过渡；最后一个字完成后隐藏人物，仅保持完整 COOLZHU `holdMs`，只调用一次 `onComplete`，并派发 `coolzhu-seven-letter-startup-complete` 事件。

Node 合同测试：

```text
node modules/gui-desktop/packages/tauri-shell/ui/tests/seven-letter-startup-player.cjs
```
