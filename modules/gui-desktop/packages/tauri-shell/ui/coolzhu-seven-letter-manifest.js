/* 启动素材清单：字形原图与Q版整人帧只提供资源，不再携带废弃的逐字笔顺。 */
(function (root, factory) {
  const manifest = factory();
  if (typeof module === "object" && module.exports) module.exports = manifest;
  else root.CoolzhuSevenLetterManifest = manifest;
})(typeof globalThis === "object" ? globalThis : this, function () {
  "use strict";
  const glyphRects = {
    C: { x: 127, y: 40, width: 1002, height: 1138 },
    O: { x: 82, y: 45, width: 1116, height: 1127 },
    L: { x: 228, y: 58, width: 853, height: 1113 },
    Z: { x: 129, y: 50, width: 1057, height: 1160 },
    H: { x: 139, y: 62, width: 971, height: 1105 },
    U: { x: 158, y: 63, width: 959, height: 1125 },
  };
  return Object.freeze({
    version: 2,
    canvas: { width: 1680, height: 900 },
    background: { source: { src: "assets/p83-coolzhu/scroll-shanhe-v1.png", width: 1672, height: 941 } },
    actor: { frames: Array.from({length:5}, (_,index)=>({src:`assets/p83-coolzhu/actors/C-k${index}.png`,width:640,height:640})) },
    completeEvent: "coolzhu-seven-letter-startup-internal",
    letters: [..."COOLZHU"].map(glyph=>({
      glyph,
      glyphSource: {src:`assets/p83-coolzhu/glyphs/${glyph}.png`,width:1254,height:1254},
      glyphRect: {...glyphRects[glyph]},
    })),
  });
});
