(function bootstrap(root, factory) {
  const manifest = factory();
  if (typeof module === "object" && module.exports) module.exports = manifest;
  else if (root) root.CoolzhuSevenLetterManifest = manifest;
})(typeof globalThis === "object" ? globalThis : this, function createCoolzhuSevenLetterManifest() {
  "use strict";

  const actorRoot = "assets/p83-coolzhu/actors/";
  const glyphRoot = "assets/p83-coolzhu/glyphs/";
  // alpha>=128 包围框外扩 16px：保留抗锯齿/金色边缘，同时去掉 1254 方板的大块透明余量。
  // 坐标仍是源 PNG 原生像素，播放器会把 glyphRect 等比映射到固定 glyph slot。
  const glyphRects = {
    C: { x: 127, y: 40, width: 1002, height: 1138 },
    O: { x: 82, y: 45, width: 1116, height: 1127 },
    L: { x: 228, y: 58, width: 853, height: 1113 },
    Z: { x: 129, y: 50, width: 1057, height: 1160 },
    H: { x: 139, y: 62, width: 971, height: 1105 },
    U: { x: 158, y: 63, width: 959, height: 1125 },
  };
  const actorTips = {
    C: [[340, 112], [340, 96], [240, 190], [285, 126], [575, 390]],
    O1: [[320, 146], [281, 81], [282, 176], [310, 137], [90, 107]],
    O2: [[423, 268], [290, 74], [279, 132], [340, 96], [424, 184]],
    L: [[391, 74], [305, 145], [359, 211], [392, 93], [589, 308]],
    Z: [[295, 71], [248, 167], [473, 186], [99, 81], [177, 220]],
    H: [[145, 122], [174, 198], [109, 170], [537, 76], [564, 325]],
    U: [[373, 76], [295, 137], [314, 248], [433, 225], [533, 67]],
  };

  function actorFrames(id) {
    return Array.from({ length: 5 }, (_, frameIndex) => ({
      id: `${id}-k${frameIndex}`,
      src: `${actorRoot}${id}-k${frameIndex}.png`,
      width: 640,
      height: 640,
      anchors: {
        // 所有整人帧统一以原生透明图的鞋底基线和 640 画布中心作初始固定锚。
        foot: [320, 576],
        swordTip: actorTips[id][frameIndex],
      },
    }));
  }

  function revealFor(glyph) {
    const paths = {
      C: [
        [[0.82, 0.15], [0.55, 0.08], [0.28, 0.18], [0.14, 0.42], [0.14, 0.70], [0.34, 0.90], [0.66, 0.93], [0.84, 0.82]],
      ],
      O: [[
        [0.50, 0.08], [0.74, 0.13], [0.90, 0.32], [0.94, 0.58], [0.84, 0.82], [0.63, 0.93],
        [0.37, 0.93], [0.16, 0.82], [0.06, 0.58], [0.10, 0.32], [0.26, 0.13], [0.50, 0.08],
      ]],
      L: [
        [[0.18, 0.08], [0.18, 0.92]],
        [[0.18, 0.92], [0.86, 0.92]],
      ],
      Z: [
        [[0.12, 0.14], [0.88, 0.14]],
        [[0.88, 0.14], [0.12, 0.86]],
        [[0.12, 0.86], [0.88, 0.86]],
      ],
      H: [
        [[0.16, 0.10], [0.16, 0.90]],
        [[0.16, 0.50], [0.84, 0.50]],
        [[0.84, 0.10], [0.84, 0.90]],
      ],
      U: [
        [[0.16, 0.10], [0.16, 0.70]],
        [[0.16, 0.70], [0.30, 0.88], [0.50, 0.93], [0.70, 0.88], [0.84, 0.70]],
        [[0.84, 0.70], [0.84, 0.10]],
      ],
    };
    return {
      mode: "path",
      coordinateSpace: "normalized",
      widthUnit: "normalized",
      width: 0.11,
      // 暗玉卷轴上的确定性底层笔芯；原 glyph PNG 随后以 mask 纹理覆盖，不替代玉纹/龙饰。
      coreColor: "#0c915b",
      borderColor: "#e2b85d",
      strokes: paths[glyph],
    };
  }

  // 背景内幅经 1.10 等比 cover 后仍保留两侧玉柱；字位按各自自然宽度重排，避免压柱或互相叠字。
  const slotCenters = [164, 402, 652, 877, 1091, 1319, 1540];
  const GLYPH_TARGET_HEIGHT = 240;
  const GLYPH_TARGET_Y = 320;
  const ACTOR_SLOT_Y = 150;
  // 640 源帧的 foot anchor 为 y=576，370 宽槽使人物约放大 9%；footY=700 时剑尖活动
  // 区域仍贴近 y=320..560 的加大字形，而不是停在画布底部。
  const ACTOR_FOOT_Y = 700;

  // 统一展示高度但按裁剪源的宽高比求宽；归一化笔顺仍在这个目标槽内映射，
  // 因而不会因 C/L/U 的非正方 crop 被横向拉伸，且每个字位中心 x 不变。
  function glyphSlotFor(glyph, index) {
    const sourceRect = glyphRects[glyph];
    const width = GLYPH_TARGET_HEIGHT * sourceRect.width / sourceRect.height;
    const center = slotCenters[index];
    return {
      x: center - width / 2,
      y: GLYPH_TARGET_Y,
      width,
      height: GLYPH_TARGET_HEIGHT,
    };
  }

  function letter(glyph, actorId, index, actorCenterOffset = 0) {
    const glyphSlot = glyphSlotFor(glyph, index);
    const center = slotCenters[index];
    return {
      glyph,
      glyphSource: { src: `${glyphRoot}${glyph}.png`, width: 1254, height: 1254 },
      glyphRect: { ...glyphRects[glyph] },
      slot: {
        glyph: glyphSlot,
        actor: {
          x: center - 185 + actorCenterOffset,
          y: ACTOR_SLOT_Y,
          width: 370,
          height: 590,
          actorX: center + actorCenterOffset,
          footY: ACTOR_FOOT_Y,
        },
      },
      reveal: revealFor(glyph),
      durationMs: 600,
      frames: actorFrames(actorId),
    };
  }

  return Object.freeze({
    version: 1,
    sourcePolicy: "P8.3 new full-person actors and independent glyph PNGs only; no legacy CODE/rig assets",
    canvas: { width: 1680, height: 900 },
    background: {
      color: "#071710",
      source: {
        src: "assets/p83-coolzhu/scroll-shanhe-v1.png",
        width: 1672,
        height: 941,
      },
      fit: "cover",
      zoom: 1.10,
      revealMode: "center-out",
      revealMs: 600,
      edgeColor: "#e2b85d",
    },
    timing: { introMs: 600, frameMs: 120, stepMs: 180, holdMs: 1200 },
    completeEvent: "coolzhu-seven-letter-startup-internal",
    letters: [
      letter("C", "C", 0),
      letter("O", "O1", 1),
      letter("O", "O2", 2),
      letter("L", "L", 3),
      letter("Z", "Z", 4),
      letter("H", "H", 5),
      // U 的人物源帧右侧动作外扩；只左移人物锚点，U 字位中心仍固定在 1540。
      letter("U", "U", 6, -45),
    ],
  });
});
