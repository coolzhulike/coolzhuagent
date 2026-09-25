"use strict";

const assert = require("node:assert/strict");
const player = require("../seven-letter-startup-player.js");

function makeManifest() {
  const letters = player.WORD.split("").map((glyph, letterIndex) => {
    const sourceFrames = Array.from({ length: 5 }, (_, frameIndex) => ({
      id: `${glyph}-${frameIndex}`,
      src: `full-${letterIndex}-${frameIndex}.png`,
      width: 200,
      height: 300,
      anchors: {
        foot: [100, 290],
        swordTip: [20 + frameIndex * 3, 120 - frameIndex * 4],
        hilt: [150, 180],
      },
    }));
    const letter = {
      glyph,
      // 每字独立纯字形源；两个 O 明确共用同一个 src。
      glyphSource: { src: glyph === "O" ? "glyph-O.png" : `glyph-${glyph}.png`, width: 90, height: 80 },
      glyphRect: { x: 0, y: 0, width: 90, height: 80 },
      slot: {
        glyph: { x: 20 + letterIndex * 100, y: 30, width: 90, height: 80 },
        actor: { x: 20 + letterIndex * 100, y: 140, width: 100, height: 300, actorX: 70 + letterIndex * 100, footY: 500 },
      },
      reveal: { mode: "path", coordinateSpace: "normalized", width: 0.14, strokes: [[[0, 0], [0.5, 0.5], [1, 0.2]]] },
    };
    if (glyph === "C") {
      letter.reveal.strokes = [
        [[0.1, 0.1], [0.1, 0.9]],
        [[0.9, 0.1], [0.9, 0.9]],
        [[0.1, 0.5], [0.9, 0.5]],
      ];
    } else if (glyph === "U") {
      letter.reveal.strokes = [
        [[0.1, 0.1], [0.1, 0.75]],
        [[0.1, 0.75], [0.5, 0.9], [0.9, 0.75]],
        [[0.9, 0.75], [0.9, 0.1]],
      ];
    }
    if (letterIndex % 2 === 1) {
      letter.sheet = {
        src: `sheet-${letterIndex}.png`,
        cols: 3,
        rows: 2,
        cellWidth: 220,
        cellHeight: 320,
      };
      letter.frames = sourceFrames.map((frame, frameIndex) => ({
        id: frame.id,
        index: frameIndex,
        anchors: frame.anchors,
      }));
    } else {
      letter.frames = sourceFrames;
    }
    return letter;
  });
  return {
    version: 1,
    sourcePolicy: "test full-person frames and 3x2 source tables",
    canvas: { width: 820, height: 560 },
    background: "#0b2d24",
    timing: { introMs: 0, frameMs: 50, stepMs: 20, holdMs: 100 },
    letters,
  };
}

class FakeContext {
  constructor() {
    this.calls = [];
    this.stack = [];
    this.fillStyle = "";
    this.canvas = { width: 820, height: 560 };
    this.scratchCanvases = [];
    this.__createScratchCanvas = (width, height) => {
      const canvas = new FakeScratchCanvas(width, height);
      this.scratchCanvases.push(canvas);
      return canvas;
    };
  }

  save() { this.stack.push({ fillStyle: this.fillStyle }); this.calls.push({ op: "save" }); }
  restore() { assert.ok(this.stack.length > 0, "canvas 状态必须成对 save/restore"); this.fillStyle = this.stack.pop().fillStyle; this.calls.push({ op: "restore" }); }
  setTransform(...args) { this.calls.push({ op: "setTransform", args }); }
  clearRect(...args) { this.calls.push({ op: "clearRect", args }); }
  fillRect(...args) { this.calls.push({ op: "fillRect", args }); }
  beginPath() { this.calls.push({ op: "beginPath" }); }
  rect(...args) { this.calls.push({ op: "rect", args }); }
  clip() { this.calls.push({ op: "clip" }); }
  closePath() { this.calls.push({ op: "closePath" }); }
  arc(...args) { this.calls.push({ op: "arc", args }); }
  drawImage(...args) { this.calls.push({ op: "drawImage", args }); }
}

class FakeScratchContext {
  constructor() {
    this.calls = [];
    this.stack = [];
    this.globalCompositeOperation = "source-over";
    this.globalAlpha = 1;
    this.strokeStyle = "#000";
    this.lineWidth = 1;
    this.lineCap = "butt";
    this.lineJoin = "miter";
  }

  save() {
    this.stack.push({
      globalCompositeOperation: this.globalCompositeOperation,
      globalAlpha: this.globalAlpha,
      strokeStyle: this.strokeStyle,
      lineWidth: this.lineWidth,
      lineCap: this.lineCap,
      lineJoin: this.lineJoin,
    });
    this.calls.push({ op: "save" });
  }
  restore() { assert.ok(this.stack.length > 0); Object.assign(this, this.stack.pop()); this.calls.push({ op: "restore" }); }
  clearRect(...args) { this.calls.push({ op: "clearRect", args }); }
  drawImage(...args) { this.calls.push({ op: "drawImage", args }); }
  beginPath() { this.calls.push({ op: "beginPath" }); }
  moveTo(x, y) { this.calls.push({ op: "moveTo", x, y }); }
  lineTo(x, y) { this.calls.push({ op: "lineTo", x, y }); }
  stroke() { this.calls.push({ op: "stroke", lineWidth: this.lineWidth, lineCap: this.lineCap, lineJoin: this.lineJoin }); }
}

class FakeScratchCanvas {
  constructor(width, height) {
    this.width = width;
    this.height = height;
    this.context = new FakeScratchContext();
  }
  getContext() { return this.context; }
}

function makeAssets(manifest) {
  const assets = {};
  if (manifest.background && manifest.background.source) {
    assets[manifest.background.source.src] = {
      id: manifest.background.source.src,
      width: manifest.background.source.sourceSize.width,
      height: manifest.background.source.sourceSize.height,
    };
  }
  manifest.letters.forEach((letter) => {
    assets[letter.glyphSource.src] = { id: letter.glyphSource.src, width: letter.glyphSource.sourceSize.width, height: letter.glyphSource.sourceSize.height };
  });
  manifest.letters.forEach((letter) => letter.frames.forEach((frame) => {
    assets[frame.src] = { id: frame.src, width: frame.sourceRect.width, height: frame.sourceRect.height };
  }));
  return assets;
}

const raw = makeManifest();
const normalized = player.normalizeManifest(raw);
assert.equal(normalized.word, "COOLZHU");
assert.equal(normalized.letters.length, 7);
assert.ok(normalized.letters.every((letter) => letter.frames.length === 5));
assert.equal(normalized.letters[1].frames[4].sourceRect.x, 220);
assert.equal(normalized.letters[1].frames[4].sourceRect.y, 320);
assert.deepEqual(normalized.letters.map((letter) => letter.glyph), player.WORD.split(""));
assert.ok(normalized.letters.every((letter, index) => index === 0 || letter.glyphSlot.x > normalized.letters[index - 1].glyphSlot.x));
assert.equal(normalized.glyphMaster, null, "独立 glyphSource 模式不应强迫总字形母版");
assert.equal(normalized.letters[1].glyphSource.src, normalized.letters[2].glyphSource.src, "两个 O 可以共用一个纯字形源");
assert.equal(player.sourceList(normalized).filter((source) => source === "glyph-O.png").length, 1, "共用纯字形源必须去重加载");
assert.equal(normalized.totalDurationMs, 7 * 250 + 6 * 20 + 100);

assert.throws(() => player.normalizeManifest({ ...raw, letters: raw.letters.slice(0, 6) }), /恰好有 7/);
assert.throws(() => player.normalizeManifest({ ...raw, letters: raw.letters.map((letter, index) => index === 1 ? { ...letter, glyph: "X" } : letter) }), /应为 O/);

const assets = makeAssets(normalized);
const firstStage = normalized.stages[0];
const earlyStep = player.timelineStateAt(normalized, firstStage.endMs + 4);
assert.equal(earlyStep.phase, "step");
assert.equal(earlyStep.stepFramePhase, "current-letter-settle");
assert.equal(earlyStep.actorLetterIndex, 0);
assert.equal(earlyStep.actorFrameIndex, 4);
assert.equal(earlyStep.glyphProgress[0], 1);
assert.equal(earlyStep.visibleGlyphs.includes(0), true);
assert.ok(earlyStep.actorSlot.actorX > normalized.letters[0].actorSlot.actorX);
assert.ok(earlyStep.actorSlot.actorX < normalized.letters[1].actorSlot.actorX);
const lateStep = player.timelineStateAt(normalized, firstStage.endMs + 16);
assert.equal(lateStep.stepFramePhase, "next-letter-lead");
assert.equal(lateStep.actorLetterIndex, 1);
assert.equal(lateStep.actorFrameIndex, 0);
assert.ok(earlyStep.actorSlot.actorX < normalized.letters[0].actorSlot.actorX + (normalized.letters[1].actorSlot.actorX - normalized.letters[0].actorSlot.actorX) * 0.2, "步进位置应采用非线性缓入");

const secondStageState = player.timelineStateAt(normalized, normalized.stages[1].startMs + 1);
assert.equal(secondStageState.phase, "draw");
assert.equal(secondStageState.currentLetterIndex, 1);
assert.equal(secondStageState.actorFrameIndex, 0);
assert.deepEqual(secondStageState.visibleGlyphs, [0]);

const almostDone = player.timelineStateAt(normalized, normalized.totalDurationMs - 1);
assert.equal(almostDone.phase, "hold");
assert.equal(almostDone.completed, false);
assert.equal(almostDone.showActor, false, "hold 阶段不应继续显示人物");
assert.deepEqual(almostDone.visibleGlyphs, [0, 1, 2, 3, 4, 5, 6]);
const done = player.timelineStateAt(normalized, normalized.totalDurationMs);
assert.equal(done.completed, true);
assert.equal(done.finalHold, true);

const ctx = new FakeContext();
const midScene = player.renderScene(ctx, normalized, assets, firstStage.startMs + 100);
assert.equal(ctx.stack.length, 0);
assert.ok(midScene.glyphsDrawn.includes(0));
assert.ok(midScene.actor);
assert.ok(midScene.swordTip);
assert.ok(ctx.calls.filter((call) => call.op === "drawImage").length >= 2);

// path reveal 必须在实际宽笔刷 mask 上按 stroke 顺序渐显，不能退化成 path bounding rect。
const strokeCtx = new FakeContext();
const cLetter = normalized.letters[0];
player.drawGlyph(strokeCtx, assets[cLetter.glyphSource.src], cLetter, 1);
const cMask = strokeCtx.scratchCanvases
  .map((canvas) => canvas.context)
  .find((maskContext) => maskContext.calls.some((call) => call.op === "stroke"));
assert.ok(cMask, "C 必须创建宽笔刷 mask context");
assert.equal(cMask.calls.filter((call) => call.op === "stroke").length, 3);
assert.ok(cMask.calls.filter((call) => call.op === "stroke").every((call) => call.lineWidth > 0 && call.lineCap === "round"));
const cStrokeStarts = cMask.calls.filter((call) => call.op === "moveTo");
assert.equal(cStrokeStarts.length, 3);
assert.ok(cStrokeStarts[0].y < cStrokeStarts[0].y + 1 && cStrokeStarts[0].x < cStrokeStarts[1].x, "C 左竖必须先于右竖");
assert.ok(cStrokeStarts[2].y > cStrokeStarts[0].y && cStrokeStarts[2].y < cStrokeStarts[1].y + cLetter.glyphSlot.height, "C 中横必须在两竖之后");
const cLineEnds = cMask.calls.filter((call) => call.op === "lineTo");
assert.deepEqual(cLineEnds.map((call) => [Math.round(call.x), Math.round(call.y)]), [[29, 102], [101, 102], [101, 70]], "完整 C 笔段必须保持一次目标字位映射");
const cStrokeOrder = cMask.calls.filter((call) => ["beginPath", "moveTo", "lineTo", "stroke"].includes(call.op)).map((call) => call.op);
assert.deepEqual(cStrokeOrder.filter((op) => op === "stroke"), ["stroke", "stroke", "stroke"]);
const partialCtx = new FakeContext();
player.drawGlyph(partialCtx, assets[cLetter.glyphSource.src], cLetter, 0.2);
const partialMask = partialCtx.scratchCanvases.map((canvas) => canvas.context).find((maskContext) => maskContext.calls.some((call) => call.op === "stroke"));
assert.equal(partialMask.calls.filter((call) => call.op === "stroke").length, 1, "C 前段渐显不能提前露出后续竖/横");

const uCtx = new FakeContext();
const uLetter = normalized.letters[6];
player.drawGlyph(uCtx, assets[uLetter.glyphSource.src], uLetter, 1);
const uMask = uCtx.scratchCanvases.map((canvas) => canvas.context).find((maskContext) => maskContext.calls.some((call) => call.op === "stroke"));
const uStarts = uMask.calls.filter((call) => call.op === "moveTo");
assert.equal(uMask.calls.filter((call) => call.op === "stroke").length, 3);
assert.ok(Math.abs(uStarts[0].x - uStarts[1].x) < 0.001 && uStarts[1].y > uStarts[0].y && uStarts[2].x > uStarts[1].x, "U 笔顺必须左→底→右");
const finalCtx = new FakeContext();
const finalScene = player.renderScene(finalCtx, normalized, assets, normalized.totalDurationMs);
assert.equal(finalScene.glyphsDrawn.length, 7, "完成后七个字必须全部常驻");
assert.equal(finalScene.actor, null, "最终七字停留只保留字母，不应残留人物");
assert.equal(finalCtx.stack.length, 0);

const placement = player.actorPlacement(normalized.letters[0], normalized.letters[0].frames[0], normalized.letters[0].actorSlot);
assert.equal(placement.foot.y, normalized.letters[0].actorSlot.footY);
assert.equal(placement.swordTip.x, placement.x + 20 * placement.scale);

const backgroundRaw = {
  ...raw,
  background: {
    color: "#071710",
    source: { src: "scroll.png", width: 160, height: 100 },
    fit: "cover",
    zoom: 1.1,
    revealMode: "center-out",
    revealMs: 20,
    edgeColor: "#e2b85d",
  },
  timing: { ...raw.timing, introMs: 20 },
};
const backgroundManifest = player.normalizeManifest(backgroundRaw);
const openingCtx = new FakeContext();
const openingScene = player.renderScene(openingCtx, backgroundManifest, makeAssets(backgroundManifest), 10);
assert.equal(openingScene.state.showActor, false, "卷轴未展开完成前不得显示演员");
assert.ok(openingScene.background.progress > 0 && openingScene.background.progress < 1);
assert.ok(openingCtx.calls.some((call) => call.op === "clip"), "卷轴中心向两侧展开必须使用裁切");
const openedScene = player.renderScene(new FakeContext(), backgroundManifest, makeAssets(backgroundManifest), 20);
assert.equal(openedScene.state.showActor, true);
assert.equal(openedScene.background.progress, 1);

function makeRuntime() {
  const context = new FakeContext();
  const canvas = { width: 0, height: 0, style: {}, getContext: () => context };
  const events = [];
  const rafs = new Map();
  let rafId = 0;
  const documentRef = {
    dispatchEvent(event) { events.push(event); return true; },
    getElementById() { return canvas; },
  };
  const windowRef = {
    CustomEvent: class CustomEvent {
      constructor(type, init) { this.type = type; this.detail = init.detail; }
    },
    requestAnimationFrame(callback) { const id = ++rafId; rafs.set(id, callback); return id; },
    cancelAnimationFrame(id) { rafs.delete(id); },
  };
  return { context, canvas, events, rafs, documentRef, windowRef };
}

(async () => {
  const loadedSources = [];
  class FakeImage {
    set src(value) {
      loadedSources.push(value);
      queueMicrotask(() => this.onload?.());
    }
  }
  const loadedAssets = await player.loadAssets(normalized, { Image: FakeImage });
  assert.equal(Object.keys(loadedAssets).length, player.sourceList(normalized).length);
  assert.equal(loadedSources.filter((source) => source === "glyph-O.png").length, 1, "两个 O 的 glyphSource 只应加载一次");

  class MissingBackgroundImage {
    set src(value) {
      queueMicrotask(() => {
        if (value === backgroundManifest.background.source.src) this.onerror?.();
        else this.onload?.();
      });
    }
  }
  const fallbackAssets = await player.loadAssets(backgroundManifest, { Image: MissingBackgroundImage });
  assert.equal(fallbackAssets[backgroundManifest.background.source.src], undefined, "背景缺失时应省略可选背景 asset");
  assert.ok(fallbackAssets[backgroundManifest.letters[0].glyphSource.src], "背景缺失不得影响字形必需素材");
  const fallbackScene = player.renderScene(new FakeContext(), backgroundManifest, fallbackAssets, backgroundManifest.totalDurationMs);
  assert.equal(fallbackScene.background.drawn, false, "背景缺失时应回退纯色底并继续绘制七字");
  assert.equal(fallbackScene.glyphsDrawn.length, 7, "背景缺失时七字仍应完成");

  const runtime = makeRuntime();
  let completed = 0;
  const instance = player.createStartupPlayer({
    manifest: raw,
    assets,
    canvas: runtime.canvas,
    documentRef: runtime.documentRef,
    windowRef: runtime.windowRef,
    onComplete(state, scene) {
      completed += 1;
      assert.equal(state.completed, true);
      assert.equal(scene.glyphsDrawn.length, 7);
    },
  });
  assert.equal(instance.isActive(), false);
  assert.equal(instance.start(), true);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(runtime.rafs.size, 1);
  const firstCallback = [...runtime.rafs.values()][0];
  runtime.rafs.clear();
  firstCallback(0);
  assert.equal(runtime.rafs.size, 1);
  const finalCallback = [...runtime.rafs.values()][0];
  runtime.rafs.clear();
  finalCallback(normalized.totalDurationMs + 1);
  assert.equal(completed, 1);
  assert.equal(instance.isFinished(), true);
  assert.equal(runtime.events.length, 1);
  assert.equal(runtime.events[0].type, player.COMPLETE_EVENT);
  assert.equal(runtime.events[0].detail.reason, "completed");
  assert.equal(instance.finish("completed"), false, "完成回调必须只触发一次");

  console.log("seven-letter-startup-player contracts: PASS");
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
