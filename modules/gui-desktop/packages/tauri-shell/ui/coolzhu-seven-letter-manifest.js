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
    version: 3,
    canvas: { width: 1680, height: 900 },
    background: { source: { src: "assets/p83-coolzhu/scroll-shanhe-v1.png", width: 1672, height: 941 } },
    // 四个动作取自同一透明原图，各帧按真实透明间隙裁样并保留原始长宽比。
    actor: { frames: [
      {src:"assets/p83-coolzhu/actors/swordsman-four-poses-v2.png",sourceRect:{x:32,y:108,width:462,height:544},anchor:{x:212,y:499}},
      {src:"assets/p83-coolzhu/actors/swordsman-four-poses-v2.png",sourceRect:{x:577,y:64,width:444,height:652},anchor:{x:217,y:631}},
      {src:"assets/p83-coolzhu/actors/swordsman-four-poses-v2.png",sourceRect:{x:1098,y:116,width:531,height:527},anchor:{x:297,y:507}},
      {src:"assets/p83-coolzhu/actors/swordsman-four-poses-v2.png",sourceRect:{x:1641,y:204,width:504,height:464},anchor:{x:260,y:416}},
    ] },
    completeEvent: "coolzhu-seven-letter-startup-internal",
    letters: [..."COOLZHU"].map(glyph=>({
      glyph,
      glyphSource: {src:`assets/p83-coolzhu/glyphs/${glyph}.png`,width:1254,height:1254},
      glyphRect: {...glyphRects[glyph]},
    })),
  });
});
