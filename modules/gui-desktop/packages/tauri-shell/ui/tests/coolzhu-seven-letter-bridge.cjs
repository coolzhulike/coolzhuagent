"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const bridge = require("../coolzhu-seven-letter-bridge.js");
const player = require("../seven-letter-startup-player.js");
const manifest = require("../coolzhu-seven-letter-manifest.js");

const normalizedManifest = player.normalizeManifest(manifest);
assert.equal(normalizedManifest.word, "COOLZHU");
assert.equal(normalizedManifest.letters.length, 7);
assert.ok(normalizedManifest.letters.every((letter) => letter.frames.length === 5));
assert.ok(normalizedManifest.letters.every((letter) => letter.glyphRect.width < 1254 && letter.glyphRect.height < 1254));
assert.ok(normalizedManifest.letters.every((letter) => letter.reveal.coreColor === "#0c915b"));
assert.ok(normalizedManifest.letters.every((letter) => letter.reveal.borderColor === "#e2b85d"));
assert.equal(normalizedManifest.background.source.src, "assets/p83-coolzhu/scroll-shanhe-v1.png");
assert.equal(normalizedManifest.background.revealMode, "center-out");
assert.equal(normalizedManifest.background.revealMs, 600);
assert.equal(normalizedManifest.timing.introMs, 600, "卷轴展开期间不得显示演员");
assert.equal(player.timelineStateAt(normalizedManifest, 0).showActor, false);
assert.equal(player.timelineStateAt(normalizedManifest, 600).showActor, true);
const expectedSlotCenters = [164, 402, 652, 877, 1091, 1319, 1540];
const expectedActorCenters = [164, 402, 652, 877, 1091, 1319, 1495];
normalizedManifest.letters.forEach((letter, index) => {
  const slot = letter.glyphSlot;
  const sourceAspect = letter.glyphRect.width / letter.glyphRect.height;
  const targetAspect = slot.width / slot.height;
  assert.ok(Math.abs(sourceAspect - targetAspect) < 1e-9, `${letter.glyph} 字形槽必须保持源宽高比`);
  assert.ok(Math.abs(slot.x + slot.width / 2 - expectedSlotCenters[index]) < 1e-9, `${letter.glyph} 字位中心必须固定`);
  assert.equal(letter.actorSlot.actorX, expectedActorCenters[index], `${letter.glyph} 人物锚点应使用独立动作落点`);
  assert.equal(slot.y, 320, `${letter.glyph} 字形应垂直居中到 y=320`);
  assert.equal(slot.height, 240, `${letter.glyph} 字形槽高度应统一为 240`);
  assert.equal(letter.actorSlot.footY, 700, `${letter.glyph} 人物支撑脚基线应落在 700`);
  for (const frame of letter.frames) {
    const placement = player.actorPlacement(letter, frame, letter.actorSlot);
    assert.ok(placement.swordTip, `${letter.glyph} ${frame.id} 应有剑尖地标`);
    assert.ok(
      placement.swordTip.y >= slot.y - 100 && placement.swordTip.y <= slot.y + slot.height + 100,
      `${letter.glyph} ${frame.id} 剑尖不得脱离当前字形活动区`
    );
    if (index === 6) {
      assert.ok(placement.x + placement.width <= 1680, `${letter.glyph} ${frame.id} 人物右缘不得越出 1680 画布`);
    }
  }
});
const uiRoot = path.resolve(__dirname, "..");
for (const source of player.sourceList(normalizedManifest)) {
  assert.equal(fs.existsSync(path.join(uiRoot, source)), true, `启动素材缺失: ${source}`);
}
assert.ok(player.sourceList(normalizedManifest).includes("assets/p83-coolzhu/scroll-shanhe-v1.png"));
const launchHtml = fs.readFileSync(path.join(uiRoot, "launch-performance.html"), "utf8");
const launchCss = fs.readFileSync(path.join(uiRoot, "launch-performance.css"), "utf8");
const playerSource = fs.readFileSync(path.join(uiRoot, "seven-letter-startup-player.js"), "utf8");
assert.match(launchHtml, /<script defer src="\.\/seven-letter-startup-player\.js"><\/script>/);
assert.match(launchHtml, /<script defer src="\.\/coolzhu-seven-letter-manifest\.js"><\/script>/);
assert.match(launchHtml, /<script defer src="\.\/coolzhu-seven-letter-bridge\.js"><\/script>/);
assert.doesNotMatch(launchHtml, /<script[^>]+launch-performance\.js[^>]*><\/script>/, "旧十一字模块不得作为启动页面脚本");
assert.match(launchCss, /display:\s*grid/);
assert.match(launchCss, /place-items:\s*center/);
assert.match(launchCss, /width:\s*auto/);
assert.match(launchCss, /height:\s*auto/);
assert.match(launchCss, /max-width:\s*100%/);
assert.match(launchCss, /max-height:\s*100%/);
assert.match(launchCss, /object-fit:\s*contain/);
assert.doesNotMatch(playerSource, /canvas\.style\.width\s*=\s*`\$\{manifest\.canvas\.width\}px`/, "播放器不得写入固定 CSS 宽度");
assert.doesNotMatch(playerSource, /canvas\.style\.height\s*=\s*`\$\{manifest\.canvas\.height\}px`/, "播放器不得写入固定 CSS 高度");

function containSize(width, height) {
  const scale = Math.min(width / 1680, height / 900);
  return [1680 * scale, 900 * scale];
}
const desktopFit = containSize(1440, 900);
assert.ok(Math.abs(desktopFit[0] - 1440) < 1e-9 && Math.abs(desktopFit[1] - 771.4285714285714) < 1e-9, "1440×900 必须等比完整容纳 1680×900");
const minimumFit = containSize(900, 520);
assert.ok(Math.abs(minimumFit[0] - 900) < 1e-9 && Math.abs(minimumFit[1] - 482.14285714285717) < 1e-9, "900×520 最小窗口必须等比完整容纳 1680×900");

function eventTarget() {
  const listeners = new Map();
  return {
    addEventListener(type, callback) { listeners.set(type, callback); },
    removeEventListener(type) { listeners.delete(type); },
    dispatch(type, event = {}) { listeners.get(type)?.({ type, ...event }); },
  };
}

const documentTarget = eventTarget();
const documentEvents = [];
const nativeEvents = [];
const documentRef = {
  ...documentTarget,
  defaultView: null,
  dispatchEvent(event) {
    documentEvents.push(event);
    documentTarget.dispatch(event.type, event);
    return true;
  },
};
const windowRef = {
  CustomEvent: class CustomEvent {
    constructor(type, init) { this.type = type; this.detail = init.detail; }
  },
  __TAURI__: {
    event: {
      emit(type, payload) {
        nativeEvents.push({ type, payload });
        return Promise.resolve();
      },
    },
  },
};
documentRef.defaultView = windowRef;

const canvasContext = {
  save() {}, restore() {}, setTransform() {}, clearRect() {},
};
const canvas = {
  width: 1680,
  height: 900,
  hidden: false,
  style: {},
  getContext() { return canvasContext; },
  setAttribute() {},
};
const skipButton = { hidden: false, disabled: false, style: {}, ...eventTarget() };
const presentationRoot = { hidden: false, style: {} };
let playerOptions = null;
const playerApi = {
  createStartupPlayer(options) {
    playerOptions = options;
    return {
      start() { return true; },
      stop(reason) {
        options.documentRef.dispatchEvent({
          type: options.manifest.completeEvent,
          detail: { reason },
        });
        return true;
      },
    };
  },
};

const instance = bridge.createBridge({
  documentRef,
  windowRef,
  playerApi,
  manifest: { letters: [] },
  canvas,
  skipButton,
  presentationRoot,
}).start();
assert.ok(instance);
assert.equal(playerOptions.manifest.completeEvent, bridge.INTERNAL_EVENT);
skipButton.dispatch("click");

const domCompletion = documentEvents.find((event) => event.type === bridge.COMPLETE_EVENT);
assert.equal(domCompletion.detail.reason, "skipped");
assert.equal(nativeEvents.length, 1, "Tauri 宿主应收到一次完成事件");
assert.equal(nativeEvents[0].type, bridge.COMPLETE_EVENT);
assert.equal(nativeEvents[0].payload.reason, "skipped");
assert.equal(canvas.hidden, true);
assert.equal(presentationRoot.hidden, true);

console.log("coolzhu-seven-letter-bridge contracts: PASS");
