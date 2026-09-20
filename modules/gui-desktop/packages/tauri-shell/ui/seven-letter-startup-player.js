(function bootstrap(root, factory) {
  const api = factory(root);

  if (typeof module === "object" && module.exports) {
    module.exports = api;
  } else if (root) {
    root.CoolzhuSevenLetterStartupPlayer = api;
  }
})(typeof globalThis === "object" ? globalThis : this, function createSevenLetterStartupPlayer(root) {
  "use strict";

  const WORD = "COOLZHU";
  const LETTER_COUNT = WORD.length;
  const FRAME_COUNT = 5;
  const DEFAULT_FRAME_MS = 120;
  const DEFAULT_STEP_MS = 180;
  const DEFAULT_HOLD_MS = 1200;
  const DEFAULT_BACKGROUND = "#0b2d24";
  const COMPLETE_EVENT = "coolzhu-seven-letter-startup-complete";

  function finite(value, fallback) {
    return Number.isFinite(Number(value)) ? Number(value) : fallback;
  }

  function positive(value, fallback) {
    const number = finite(value, fallback);
    return number > 0 ? number : fallback;
  }

  function copyPoint(point, fallback = [0, 0]) {
    if (!Array.isArray(point) || point.length < 2) return [...fallback];
    return [finite(point[0], fallback[0]), finite(point[1], fallback[1])];
  }

  function copyRect(rect, fallback = { x: 0, y: 0, width: 1, height: 1 }) {
    if (!rect || typeof rect !== "object") return { ...fallback };
    return {
      x: finite(rect.x, fallback.x),
      y: finite(rect.y, fallback.y),
      width: positive(rect.width, fallback.width),
      height: positive(rect.height, fallback.height),
    };
  }

  function freezeDeep(value) {
    if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
    Object.keys(value).forEach((key) => freezeDeep(value[key]));
    return Object.freeze(value);
  }

  function assetKey(source) {
    if (typeof source !== "string" || !source.trim()) {
      throw new Error("整人帧/字形母版必须提供非空 src");
    }
    return source;
  }

  function normalizeGlyphSource(input) {
    if (typeof input === "string") {
      return { src: assetKey(input), sourceSize: null };
    }
    if (!input || typeof input !== "object") {
      throw new Error("glyphSource 必须是路径字符串或 { src, width, height } 对象");
    }
    const sourceSize = input.sourceSize || input;
    return {
      src: assetKey(input.src),
      sourceSize: {
        width: positive(input.width || sourceSize.width, 1),
        height: positive(input.height || sourceSize.height, 1),
      },
    };
  }

  function normalizeBackground(input) {
    const value = typeof input === "string"
      ? { color: input }
      : input && typeof input === "object"
        ? input
        : {};
    const sourceInput = value.source || value.image || (value.src ? value : null);
    let source = null;
    if (sourceInput) {
      const normalized = normalizeGlyphSource(sourceInput);
      if (!normalized.sourceSize) throw new Error("背景 source 必须提供 width/height 原始尺寸");
      source = normalized;
    }
    return {
      color: typeof value.color === "string" && value.color ? value.color : DEFAULT_BACKGROUND,
      source,
      fit: value.fit === "contain" ? "contain" : "cover",
      zoom: Math.max(1, finite(value.zoom, 1)),
      revealMode: value.revealMode === "center-out" ? "center-out" : "none",
      revealMs: Math.max(0, Math.round(finite(value.revealMs, 0))),
      edgeColor: typeof value.edgeColor === "string" && value.edgeColor ? value.edgeColor : "#d9ad50",
    };
  }

  function normalizeSheetFrame(letter, frame, frameIndex) {
    const sheet = letter.sheet;
    if (!sheet || typeof sheet !== "object") return null;
    const cols = Math.max(1, Math.floor(finite(sheet.cols, 3)));
    const rows = Math.max(1, Math.floor(finite(sheet.rows, 2)));
    const cellWidth = positive(sheet.cellWidth, 1);
    const cellHeight = positive(sheet.cellHeight, 1);
    const cellCount = cols * rows;
    const requestedIndex = frame && frame.index !== undefined
      ? Number(frame.index)
      : frame && frame.cell !== undefined
        ? Number(frame.cell)
        : frameIndex;
    const index = Math.max(0, Math.min(cellCount - 1, Math.floor(finite(requestedIndex, frameIndex))));
    const x = (index % cols) * cellWidth;
    const y = Math.floor(index / cols) * cellHeight;
    return {
      id: frame && frame.id ? String(frame.id) : `${letter.glyph || "?"}-sheet-${frameIndex}`,
      src: assetKey(sheet.src),
      sourceRect: { x, y, width: cellWidth, height: cellHeight },
      anchors: frame && frame.anchors ? frame.anchors : {},
      anchorSpace: frame && frame.anchorSpace ? frame.anchorSpace : "frame",
      frameIndex,
      sheetIndex: index,
    };
  }

  function normalizeFullFrame(letter, frame, frameIndex) {
    if (!frame || typeof frame !== "object") {
      throw new Error(`${letter.glyph || "?"} 第 ${frameIndex + 1} 帧必须是对象`);
    }
    const sourceRect = frame.sourceRect
      ? copyRect(frame.sourceRect)
      : {
          x: 0,
          y: 0,
          width: positive(frame.width, positive(letter.frameWidth, 1)),
          height: positive(frame.height, positive(letter.frameHeight, 1)),
        };
    return {
      id: frame.id ? String(frame.id) : `${letter.glyph || "?"}-frame-${frameIndex}`,
      src: assetKey(frame.src || letter.src),
      sourceRect,
      anchors: frame.anchors && typeof frame.anchors === "object" ? frame.anchors : {},
      anchorSpace: frame.anchorSpace || "frame",
      frameIndex,
    };
  }

  function normalizeAnchor(anchor, sourceRect, fallback) {
    const point = copyPoint(anchor, fallback);
    return [point[0], point[1]];
  }

  function normalizeFrame(letter, frame, frameIndex) {
    const normalized = letter.sheet
      ? normalizeSheetFrame(letter, frame || {}, frameIndex)
      : normalizeFullFrame(letter, frame, frameIndex);
    const sourceRect = normalized.sourceRect;
    const anchors = normalized.anchors || {};
    const anchorSpace = normalized.anchorSpace;
    const convert = (value, fallback) => {
      const point = normalizeAnchor(value, sourceRect, fallback);
      if (anchorSpace === "source") return [point[0] - sourceRect.x, point[1] - sourceRect.y];
      return point;
    };
    const foot = convert(anchors.foot || anchors.feet || anchors.footAnchor, [sourceRect.width / 2, sourceRect.height]);
    const swordTip = anchors.swordTip || anchors.tip;
    return {
      id: normalized.id,
      src: normalized.src,
      sourceRect: sourceRect,
      anchors: {
        foot,
        face: anchors.face ? convert(anchors.face, [sourceRect.width / 2, sourceRect.height * 0.3]) : null,
        swordTip: swordTip ? convert(swordTip, [sourceRect.width / 2, sourceRect.height * 0.35]) : null,
        hilt: anchors.hilt ? convert(anchors.hilt, [sourceRect.width / 2, sourceRect.height * 0.5]) : null,
      },
      frameIndex: frameIndex,
      sheetIndex: normalized.sheetIndex === undefined ? null : normalized.sheetIndex,
    };
  }

  function normalizeReveal(reveal, glyphRect) {
    const value = reveal && typeof reveal === "object" ? reveal : {};
    const mode = value.mode === "path" || value.type === "stroke-path" ? "path" : "sweep";
    const axis = value.axis === "y" ? "y" : "x";
    const coordinateSpace = value.coordinateSpace === "pixels" ? "pixels" : "normalized";
    const widthUnit = value.widthUnit === "normalized"
      || (value.widthUnit === undefined && coordinateSpace === "normalized" && Number(value.width) <= 1)
      ? "normalized"
      : "pixels";
    const strokes = Array.isArray(value.strokes)
      ? value.strokes.map((stroke) => Array.isArray(stroke) ? stroke.map((point) => copyPoint(point)) : [])
      : [];
    return {
      mode,
      axis,
      direction: value.direction === "reverse" ? "reverse" : "forward",
      width: positive(value.width, widthUnit === "normalized" ? 0.12 : Math.max(8, Math.min(glyphRect.width, glyphRect.height) * 0.12)),
      coreWidth: positive(value.coreWidth, widthUnit === "normalized" ? positive(value.width, 0.12) * 0.88 : null),
      borderWidth: positive(value.borderWidth, widthUnit === "normalized" ? positive(value.width, 0.12) * 1.18 : null),
      coreColor: typeof value.coreColor === "string" && value.coreColor ? value.coreColor : "#bfe8d5",
      borderColor: typeof value.borderColor === "string" && value.borderColor ? value.borderColor : "#d9ad50",
      widthUnit,
      strokes,
      coordinateSpace,
    };
  }

  function normalizeActorSlot(slot, index) {
    const value = slot && typeof slot === "object" ? slot : {};
    const actor = value.actor && typeof value.actor === "object" ? value.actor : value;
    const rect = copyRect(actor, { x: 0, y: 0, width: 1, height: 1 });
    const actorX = finite(actor.actorX, actor.x + actor.width / 2);
    const footY = finite(actor.footY, actor.y + actor.height);
    return {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      actorX,
      footY,
      scale: Number.isFinite(Number(actor.scale)) && Number(actor.scale) > 0 ? Number(actor.scale) : null,
      index,
    };
  }

  function normalizeGlyphSlot(letter, index) {
    const slot = letter.slot && typeof letter.slot === "object" ? letter.slot : {};
    const glyph = slot.glyph && typeof slot.glyph === "object" ? slot.glyph : letter.glyphSlot;
    if (!glyph) throw new Error(`${letter.glyph || "?"} 缺少 slot.glyph 固定字位`);
    const rect = copyRect(glyph);
    return { ...rect, index };
  }

  function normalizeManifest(input) {
    if (!input || typeof input !== "object") throw new Error("启动动画 manifest 必须是对象");
    if (input.version !== undefined && Number(input.version) < 1) throw new Error("不支持的启动动画 manifest 版本");
    const letters = Array.isArray(input.letters) ? input.letters : [];
    if (letters.length !== LETTER_COUNT) throw new Error(`启动动画必须恰好有 ${LETTER_COUNT} 个字位`);
    // 每字可以有独立 glyphSource；旧的单张总母版只作为兼容回退，不再是强制输入。
    const glyphMaster = input.glyphMaster ? normalizeGlyphSource(input.glyphMaster) : null;
    const canvasInput = input.canvas && typeof input.canvas === "object" ? input.canvas : {};
    const canvas = {
      width: Math.max(1, Math.round(positive(canvasInput.width, 1400))),
      height: Math.max(1, Math.round(positive(canvasInput.height, 800))),
    };
    const timingInput = input.timing && typeof input.timing === "object" ? input.timing : {};
    const frameMs = Math.max(1, Math.round(positive(timingInput.frameMs, DEFAULT_FRAME_MS)));
    const stepMs = Math.max(0, Math.round(finite(timingInput.stepMs, DEFAULT_STEP_MS)));
    const holdMs = Math.max(0, Math.round(finite(timingInput.holdMs, DEFAULT_HOLD_MS)));
    const introMs = Math.max(0, Math.round(finite(timingInput.introMs, 0)));
    const background = normalizeBackground(input.background);
    const normalizedLetters = letters.map((letter, index) => {
      if (!letter || typeof letter !== "object") throw new Error(`第 ${index + 1} 个字位必须是对象`);
      const glyph = String(letter.glyph || "");
      if (glyph !== WORD[index]) throw new Error(`第 ${index + 1} 个字位应为 ${WORD[index]}，实际为 ${glyph || "空"}`);
      const sourceFrames = Array.isArray(letter.frames) ? letter.frames : [];
      const frames = letter.sheet
        ? (sourceFrames.length ? sourceFrames : Array.from({ length: FRAME_COUNT }, (_, frameIndex) => ({ index: frameIndex })))
            .slice(0, FRAME_COUNT)
            .map((frame, frameIndex) => normalizeFrame(letter, frame, frameIndex))
        : sourceFrames.slice(0, FRAME_COUNT).map((frame, frameIndex) => normalizeFrame(letter, frame, frameIndex));
      if (frames.length !== FRAME_COUNT) throw new Error(`${glyph} 必须提供 ${FRAME_COUNT} 张整人帧或一个至少含 ${FRAME_COUNT} 格的 3x2 源表`);
      const glyphSource = normalizeGlyphSource(letter.glyphSource || glyphMaster);
      const glyphSourceRect = letter.glyphRect
        ? copyRect(letter.glyphRect)
        : glyphSource.sourceSize
          ? { x: 0, y: 0, width: glyphSource.sourceSize.width, height: glyphSource.sourceSize.height }
          : null;
      if (!glyphSourceRect) throw new Error(`${glyph} 缺少 glyphRect；独立 glyphSource 未提供源尺寸`);
      const glyphSlot = normalizeGlyphSlot(letter, index);
      const actorSlot = normalizeActorSlot(letter.slot, index);
      const durationMs = Math.max(1, Math.round(positive(letter.durationMs, frameMs * FRAME_COUNT)));
      return {
        index,
        glyph,
        frames,
        glyphSource,
        glyphRect: glyphSourceRect,
        glyphSlot,
        actorSlot,
        reveal: normalizeReveal(letter.reveal || input.reveal, glyphSourceRect),
        durationMs,
      };
    });
    for (let index = 1; index < normalizedLetters.length; index += 1) {
      if (normalizedLetters[index].glyphSlot.x <= normalizedLetters[index - 1].glyphSlot.x) {
        throw new Error("七个字位必须按 C/O/O/L/Z/H/U 固定横向递增");
      }
    }
    const stages = [];
    let cursor = introMs;
    normalizedLetters.forEach((letter, index) => {
      const startMs = cursor;
      const endMs = startMs + letter.durationMs;
      const stepEndMs = endMs + (index < normalizedLetters.length - 1 ? stepMs : 0);
      stages.push({ index, startMs, endMs, stepEndMs });
      cursor = stepEndMs;
    });
    const normalized = {
      version: Number(input.version || 1),
      word: WORD,
      canvas,
      background,
      glyphMaster,
      letters: normalizedLetters,
      timing: { frameMs, stepMs, holdMs, introMs },
      stages,
      totalDurationMs: cursor + holdMs,
      completeEvent: input.completeEvent || COMPLETE_EVENT,
      sourcePolicy: input.sourcePolicy || "manifest supplied full-person frames only",
    };
    return freezeDeep(normalized);
  }

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }

  function lerp(left, right, progress) {
    return left + (right - left) * progress;
  }

  function smoothStep(progress) {
    const value = clamp(progress, 0, 1);
    return value * value * (3 - 2 * value);
  }

  function interpolateSlot(left, right, progress) {
    return {
      x: lerp(left.x, right.x, progress),
      y: lerp(left.y, right.y, progress),
      width: lerp(left.width, right.width, progress),
      height: lerp(left.height, right.height, progress),
      actorX: lerp(left.actorX, right.actorX, progress),
      footY: lerp(left.footY, right.footY, progress),
      scale: left.scale !== null && right.scale !== null ? lerp(left.scale, right.scale, progress) : null,
    };
  }

  function timelineStateAt(manifestInput, elapsedMs) {
    const manifest = manifestInput && manifestInput.stages ? manifestInput : normalizeManifest(manifestInput);
    const elapsed = Math.max(0, finite(elapsedMs, 0));
    const glyphProgress = manifest.letters.map(() => 0);
    const base = {
      elapsedMs: elapsed,
      phase: "intro",
      currentLetterIndex: 0,
      actorLetterIndex: 0,
      actorFrameIndex: 0,
      actorSlotProgress: 0,
      actorSlot: manifest.letters[0].actorSlot,
      showActor: elapsed >= manifest.timing.introMs,
      glyphProgress,
      visibleGlyphs: [],
      completed: elapsed >= manifest.totalDurationMs,
      finalHold: false,
    };
    if (elapsed < manifest.timing.introMs) return base;
    for (let index = 0; index < manifest.stages.length; index += 1) {
      const stage = manifest.stages[index];
      const letter = manifest.letters[index];
      if (elapsed < stage.startMs) break;
      if (elapsed < stage.endMs) {
        const progress = clamp((elapsed - stage.startMs) / letter.durationMs, 0, 1);
        glyphProgress[index] = progress;
        const frameIndex = Math.min(FRAME_COUNT - 1, Math.floor((elapsed - stage.startMs) / manifest.timing.frameMs));
        return {
          ...base,
          phase: "draw",
          currentLetterIndex: index,
          actorLetterIndex: index,
          actorFrameIndex: frameIndex,
          actorSlotProgress: 0,
          actorSlot: letter.actorSlot,
          showActor: true,
          glyphProgress,
          visibleGlyphs: glyphProgress.map((value, glyphIndex) => value >= 1 ? glyphIndex : -1).filter((glyphIndex) => glyphIndex >= 0),
          completed: false,
        };
      }
      glyphProgress[index] = 1;
      if (index < manifest.letters.length - 1 && elapsed < stage.stepEndMs) {
        const next = manifest.letters[index + 1];
        const linearStepProgress = stage.stepEndMs === stage.endMs ? 1 : clamp((elapsed - stage.endMs) / (stage.stepEndMs - stage.endMs), 0, 1);
        const stepProgress = smoothStep(linearStepProgress);
        const nextLeadFrame = linearStepProgress >= 0.5;
        return {
          ...base,
          phase: "step",
          currentLetterIndex: index,
          nextLetterIndex: index + 1,
          actorLetterIndex: nextLeadFrame ? index + 1 : index,
          actorFrameIndex: nextLeadFrame ? 0 : FRAME_COUNT - 1,
          stepFramePhase: nextLeadFrame ? "next-letter-lead" : "current-letter-settle",
          stepLinearProgress: linearStepProgress,
          actorSlotProgress: stepProgress,
          actorSlot: interpolateSlot(letter.actorSlot, next.actorSlot, stepProgress),
          showActor: true,
          glyphProgress,
          visibleGlyphs: glyphProgress.map((value, glyphIndex) => value >= 1 ? glyphIndex : -1).filter((glyphIndex) => glyphIndex >= 0),
          completed: false,
        };
      }
    }
    glyphProgress.fill(1);
    const lastIndex = manifest.letters.length - 1;
    return {
      ...base,
      phase: "hold",
      currentLetterIndex: lastIndex,
      actorLetterIndex: lastIndex,
      actorFrameIndex: FRAME_COUNT - 1,
      actorSlotProgress: 1,
      actorSlot: manifest.letters[lastIndex].actorSlot,
      showActor: false,
      glyphProgress,
      visibleGlyphs: manifest.letters.map((_, index) => index),
      completed: elapsed >= manifest.totalDurationMs,
      finalHold: elapsed >= manifest.stages[lastIndex].endMs,
    };
  }

  function mapRevealPoint(letter, point) {
    const source = letter.glyphRect;
    const target = letter.glyphSlot;
    if (letter.reveal.coordinateSpace === "pixels") {
      return [
        target.x + (point[0] / source.width) * target.width,
        target.y + (point[1] / source.height) * target.height,
      ];
    }
    return [target.x + point[0] * target.width, target.y + point[1] * target.height];
  }

  function distance(left, right) {
    return Math.hypot(right[0] - left[0], right[1] - left[1]);
  }

  function progressiveStrokes(letter, progress) {
    const strokes = letter.reveal.strokes || [];
    const normalizedStrokes = strokes
      .map((stroke) => Array.isArray(stroke) ? stroke.filter((point) => Array.isArray(point) && point.length >= 2) : [])
      .filter((stroke) => stroke.length > 0);
    if (!normalizedStrokes.length || progress <= 0) return [];
    // 在目标字位坐标中计算弧长，避免源坐标与目标矩形非等比时渐显速度失真。
    const mappedStrokes = normalizedStrokes.map((stroke) => stroke.map((point) => mapRevealPoint(letter, point)));
    const lengths = mappedStrokes.map((stroke) => stroke.slice(1).reduce((sum, point, index) => sum + distance(stroke[index], point), 0));
    const totalLength = lengths.reduce((sum, length) => sum + length, 0);
    if (totalLength <= 0) return mappedStrokes.map((stroke) => [stroke[0]]);
    let remaining = totalLength * clamp(progress, 0, 1);
    return mappedStrokes.map((stroke) => {
      if (remaining <= 0) return [];
      const output = [stroke[0]];
      for (let pointIndex = 1; pointIndex < stroke.length; pointIndex += 1) {
        const start = stroke[pointIndex - 1];
        const end = stroke[pointIndex];
        const segmentLength = distance(start, end);
        if (segmentLength <= 0) continue;
        if (remaining >= segmentLength) {
          output.push(end);
          remaining -= segmentLength;
          continue;
        }
        const segmentProgress = remaining / segmentLength;
        const partial = [
          start[0] + (end[0] - start[0]) * segmentProgress,
          start[1] + (end[1] - start[1]) * segmentProgress,
        ];
        output.push(partial);
        remaining = 0;
        break;
      }
      // 保留空的后续 stroke；调用方可以据此验证笔顺不会跨 stroke 连线。
      return output;
    });
  }

  function brushWidthPx(letter) {
    return strokeWidthPx(letter, letter.reveal.width);
  }

  function strokeWidthPx(letter, width) {
    const rule = letter.reveal;
    const configuredWidth = width === null || width === undefined ? rule.width : width;
    if (rule.widthUnit === "normalized") return Math.max(1, configuredWidth * Math.min(letter.glyphSlot.width, letter.glyphSlot.height));
    const source = letter.glyphRect;
    const scale = ((letter.glyphSlot.width / source.width) + (letter.glyphSlot.height / source.height)) / 2;
    return Math.max(1, configuredWidth * scale);
  }

  function drawStrokePath(ctx, stroke) {
    if (!stroke.length) return;
    ctx.beginPath();
    ctx.moveTo(stroke[0][0], stroke[0][1]);
    for (let index = 1; index < stroke.length; index += 1) ctx.lineTo(stroke[index][0], stroke[index][1]);
    if (stroke.length === 1) ctx.lineTo(stroke[0][0] + 0.01, stroke[0][1]);
    ctx.stroke();
  }

  function drawThickGlyphUnderlay(ctx, letter, progress, strokes) {
    if (!ctx || !strokes.length) return false;
    if (typeof ctx.beginPath !== "function" || typeof ctx.moveTo !== "function"
      || typeof ctx.lineTo !== "function" || typeof ctx.stroke !== "function") return false;
    const rule = letter.reveal;
    const coreWidth = strokeWidthPx(letter, rule.coreWidth || rule.width * 0.88);
    const borderWidth = strokeWidthPx(letter, rule.borderWidth || rule.width * 1.18);
    ctx.save();
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.strokeStyle = rule.borderColor || "#d9ad50";
    ctx.lineWidth = borderWidth;
    strokes.forEach((stroke) => drawStrokePath(ctx, stroke));
    ctx.strokeStyle = rule.coreColor || "#bfe8d5";
    ctx.lineWidth = coreWidth;
    strokes.forEach((stroke) => drawStrokePath(ctx, stroke));
    ctx.restore();
    return true;
  }

  function backgroundProgress(manifest, elapsedMs) {
    const background = manifest.background;
    if (!background || background.revealMode !== "center-out" || background.revealMs <= 0) return 1;
    return smoothStep(clamp(elapsedMs / background.revealMs, 0, 1));
  }

  function backgroundDrawRect(manifest, background) {
    const source = background.source.sourceSize;
    const canvas = manifest.canvas;
    const baseScale = background.fit === "contain"
      ? Math.min(canvas.width / source.width, canvas.height / source.height)
      : Math.max(canvas.width / source.width, canvas.height / source.height);
    const scale = baseScale * background.zoom;
    const width = source.width * scale;
    const height = source.height * scale;
    return {
      x: (canvas.width - width) / 2,
      y: (canvas.height - height) / 2,
      width,
      height,
    };
  }

  function drawSceneBackground(ctx, manifest, assets, elapsedMs) {
    const background = manifest.background;
    const canvas = manifest.canvas;
    ctx.fillStyle = background.color;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    if (!background.source) return { drawn: false, progress: 1 };
    const image = assets && assets[background.source.src];
    if (!image) return { drawn: false, progress: backgroundProgress(manifest, elapsedMs) };
    const progress = backgroundProgress(manifest, elapsedMs);
    if (progress <= 0) return { drawn: false, progress };
    const target = backgroundDrawRect(manifest, background);
    const halfWidth = (canvas.width / 2) * progress;
    ctx.save();
    ctx.beginPath();
    ctx.rect(canvas.width / 2 - halfWidth, 0, halfWidth * 2, canvas.height);
    ctx.clip();
    ctx.drawImage(
      image,
      0,
      0,
      background.source.sourceSize.width,
      background.source.sourceSize.height,
      target.x,
      target.y,
      target.width,
      target.height
    );
    ctx.restore();
    if (progress < 1 && typeof ctx.fillRect === "function") {
      const leftEdge = canvas.width / 2 - halfWidth;
      const rightEdge = canvas.width / 2 + halfWidth;
      ctx.fillStyle = background.edgeColor;
      ctx.fillRect(Math.max(0, leftEdge - 1), 24, 2, Math.max(0, canvas.height - 48));
      ctx.fillRect(Math.max(0, rightEdge - 1), 24, 2, Math.max(0, canvas.height - 48));
    }
    return { drawn: true, progress };
  }

  function revealClipRect(letter, progress) {
    const glyphRect = letter.glyphSlot;
    const rule = letter.reveal;
    const clamped = clamp(progress, 0, 1);
    // path 模式的这个矩形只用于 scratch surface 的尺寸估算，绝不作为字形可见区域。
    if (rule.mode === "path") return { ...glyphRect, pathMask: true };
    if (clamped <= 0) return { x: glyphRect.x, y: glyphRect.y, width: 0, height: 0 };
    if (clamped >= 1) return { ...glyphRect };
    if (rule.axis === "y") {
      const height = glyphRect.height * clamped;
      return rule.direction === "reverse"
        ? { x: glyphRect.x, y: glyphRect.y + glyphRect.height - height, width: glyphRect.width, height }
        : { x: glyphRect.x, y: glyphRect.y, width: glyphRect.width, height };
    }
    const width = glyphRect.width * clamped;
    return rule.direction === "reverse"
      ? { x: glyphRect.x + glyphRect.width - width, y: glyphRect.y, width, height: glyphRect.height }
      : { x: glyphRect.x, y: glyphRect.y, width, height: glyphRect.height };
  }

  function createScratchCanvas(ctx, width, height) {
    if (ctx && typeof ctx.__createScratchCanvas === "function") return ctx.__createScratchCanvas(width, height);
    if (typeof OffscreenCanvas === "function") return new OffscreenCanvas(width, height);
    const ownerDocument = ctx && ctx.canvas && ctx.canvas.ownerDocument;
    if (ownerDocument && typeof ownerDocument.createElement === "function") {
      const canvas = ownerDocument.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      return canvas;
    }
    return null;
  }

  function drawProgressiveStrokeMask(maskCtx, letter, progress, width, height) {
    const strokes = progressiveStrokes(letter, progress);
    maskCtx.clearRect(0, 0, width, height);
    maskCtx.save();
    maskCtx.globalCompositeOperation = "source-over";
    maskCtx.globalAlpha = 1;
    maskCtx.strokeStyle = "#fff";
    maskCtx.lineWidth = brushWidthPx(letter);
    maskCtx.lineCap = "round";
    maskCtx.lineJoin = "round";
    strokes.forEach((stroke) => {
      if (!stroke.length) return;
      maskCtx.beginPath();
      maskCtx.moveTo(stroke[0][0], stroke[0][1]);
      for (let index = 1; index < stroke.length; index += 1) maskCtx.lineTo(stroke[index][0], stroke[index][1]);
      if (stroke.length === 1) maskCtx.lineTo(stroke[0][0] + 0.01, stroke[0][1]);
      maskCtx.stroke();
    });
    maskCtx.restore();
    return strokes;
  }

  function appendCapsulePath(ctx, start, end, radius) {
    const angle = Math.atan2(end[1] - start[1], end[0] - start[0]);
    const normal = angle + Math.PI / 2;
    const leftStart = [start[0] + Math.cos(normal) * radius, start[1] + Math.sin(normal) * radius];
    const leftEnd = [end[0] + Math.cos(normal) * radius, end[1] + Math.sin(normal) * radius];
    const rightEnd = [end[0] - Math.cos(normal) * radius, end[1] - Math.sin(normal) * radius];
    const rightStart = [start[0] - Math.cos(normal) * radius, start[1] - Math.sin(normal) * radius];
    ctx.moveTo(leftStart[0], leftStart[1]);
    ctx.lineTo(leftEnd[0], leftEnd[1]);
    ctx.lineTo(rightEnd[0], rightEnd[1]);
    ctx.lineTo(rightStart[0], rightStart[1]);
    ctx.closePath();
  }

  function drawPathGlyphFallback(ctx, glyphImage, letter, progress) {
    const strokes = progressiveStrokes(letter, progress);
    if (!strokes.length) return false;
    const radius = brushWidthPx(letter) / 2;
    ctx.save();
    ctx.beginPath();
    strokes.forEach((stroke) => {
      if (stroke.length === 1) {
        ctx.arc(stroke[0][0], stroke[0][1], radius, 0, Math.PI * 2);
        return;
      }
      for (let index = 1; index < stroke.length; index += 1) appendCapsulePath(ctx, stroke[index - 1], stroke[index], radius);
      ctx.arc(stroke[0][0], stroke[0][1], radius, 0, Math.PI * 2);
      ctx.arc(stroke[stroke.length - 1][0], stroke[stroke.length - 1][1], radius, 0, Math.PI * 2);
    });
    ctx.clip();
    const source = letter.glyphRect;
    const target = letter.glyphSlot;
    ctx.drawImage(glyphImage, source.x, source.y, source.width, source.height, target.x, target.y, target.width, target.height);
    ctx.restore();
    return true;
  }

  function drawPathGlyphWithMask(ctx, glyphImage, letter, progress) {
    const source = letter.glyphRect;
    const target = letter.glyphSlot;
    const brush = brushWidthPx(letter);
    const width = Math.max(1, Math.ceil(Math.max(ctx.canvas?.width || 0, target.x + target.width + brush * 2)));
    const height = Math.max(1, Math.ceil(Math.max(ctx.canvas?.height || 0, target.y + target.height + brush * 2)));
    const glyphCanvas = createScratchCanvas(ctx, width, height);
    const maskCanvas = createScratchCanvas(ctx, width, height);
    if (!glyphCanvas || !maskCanvas || typeof glyphCanvas.getContext !== "function" || typeof maskCanvas.getContext !== "function") return false;
    const glyphCtx = glyphCanvas.getContext("2d");
    const maskCtx = maskCanvas.getContext("2d");
    if (!glyphCtx || !maskCtx) return false;
    glyphCtx.clearRect(0, 0, width, height);
    glyphCtx.drawImage(glyphImage, source.x, source.y, source.width, source.height, target.x, target.y, target.width, target.height);
    const strokes = drawProgressiveStrokeMask(maskCtx, letter, progress, width, height);
    glyphCtx.save();
    glyphCtx.globalCompositeOperation = "destination-in";
    glyphCtx.drawImage(maskCanvas, 0, 0);
    glyphCtx.restore();
    ctx.drawImage(glyphCanvas, 0, 0);
    return strokes.some((stroke) => stroke.length > 0);
  }

  function drawSweepGlyph(ctx, glyphImage, letter, progress) {
    const source = letter.glyphRect;
    const target = letter.glyphSlot;
    const clip = revealClipRect(letter, progress);
    if (clip.width <= 0 || clip.height <= 0) return false;
    ctx.save();
    ctx.beginPath();
    ctx.rect(clip.x, clip.y, clip.width, clip.height);
    ctx.clip();
    ctx.drawImage(glyphImage, source.x, source.y, source.width, source.height, target.x, target.y, target.width, target.height);
    ctx.restore();
    return true;
  }

  function drawGlyph(ctx, glyphImage, letter, progress) {
    if (!ctx || !glyphImage || progress <= 0) return false;
    if (letter.reveal.mode === "path" && letter.reveal.strokes.length) {
      const strokes = progressiveStrokes(letter, progress);
      drawThickGlyphUnderlay(ctx, letter, progress, strokes);
      if (drawPathGlyphWithMask(ctx, glyphImage, letter, progress)) return true;
      return drawPathGlyphFallback(ctx, glyphImage, letter, progress);
    }
    return drawSweepGlyph(ctx, glyphImage, letter, progress);
  }

  function actorPlacement(letter, frame, actorSlot) {
    const source = frame.sourceRect;
    const scale = actorSlot.scale || Math.min(actorSlot.width / source.width, actorSlot.height / source.height);
    const foot = frame.anchors.foot || [source.width / 2, source.height];
    const x = actorSlot.actorX - foot[0] * scale;
    const y = actorSlot.footY - foot[1] * scale;
    return {
      x,
      y,
      width: source.width * scale,
      height: source.height * scale,
      scale,
      sourceRect: source,
      swordTip: frame.anchors.swordTip ? {
        x: x + frame.anchors.swordTip[0] * scale,
        y: y + frame.anchors.swordTip[1] * scale,
        visible: true,
      } : null,
      hilt: frame.anchors.hilt ? {
        x: x + frame.anchors.hilt[0] * scale,
        y: y + frame.anchors.hilt[1] * scale,
      } : null,
      foot: {
        x: x + foot[0] * scale,
        y: y + foot[1] * scale,
      },
    };
  }

  function drawActor(ctx, image, letter, frame, actorSlot) {
    if (!ctx || !image) return null;
    const placement = actorPlacement(letter, frame, actorSlot);
    const source = placement.sourceRect;
    ctx.drawImage(image, source.x, source.y, source.width, source.height, placement.x, placement.y, placement.width, placement.height);
    return placement;
  }

  function renderScene(ctx, manifestInput, assets, elapsedMs) {
    const manifest = manifestInput && manifestInput.stages ? manifestInput : normalizeManifest(manifestInput);
    const state = timelineStateAt(manifest, elapsedMs);
    if (!ctx) return { state, glyphsDrawn: [], actor: null, swordTip: null };
    ctx.save();
    if (typeof ctx.setTransform === "function") ctx.setTransform(1, 0, 0, 1, 0, 0);
    if (typeof ctx.clearRect === "function") ctx.clearRect(0, 0, manifest.canvas.width, manifest.canvas.height);
    const background = drawSceneBackground(ctx, manifest, assets || {}, elapsedMs);
    const glyphsDrawn = [];
    manifest.letters.forEach((letter, index) => {
      const glyphImage = assets && assets[letter.glyphSource.src];
      if (drawGlyph(ctx, glyphImage, letter, state.glyphProgress[index])) glyphsDrawn.push(index);
    });
    let actor = null;
    if (state.showActor) {
      const letter = manifest.letters[state.actorLetterIndex];
      const frame = letter.frames[state.actorFrameIndex];
      const actorImage = assets && assets[frame.src];
      actor = drawActor(ctx, actorImage, letter, frame, state.actorSlot);
    }
    ctx.restore();
    return {
      state,
      background,
      glyphsDrawn,
      actor,
      swordTip: actor && actor.swordTip ? { ...actor.swordTip } : null,
    };
  }

  function sourceList(manifest) {
    const sources = new Set();
    if (manifest.background && manifest.background.source) sources.add(manifest.background.source.src);
    manifest.letters.forEach((letter) => sources.add(letter.glyphSource.src));
    manifest.letters.forEach((letter) => letter.frames.forEach((frame) => sources.add(frame.src)));
    return [...sources];
  }

  function loadAssets(manifest, windowRef) {
    if (!windowRef || typeof windowRef.Image !== "function") {
      return Promise.reject(new Error("启动动画需要 window.Image 或注入 assets"));
    }
    const requiredSources = new Set();
    manifest.letters.forEach((letter) => {
      requiredSources.add(letter.glyphSource.src);
      letter.frames.forEach((frame) => requiredSources.add(frame.src));
    });
    const optionalBackgroundSource = manifest.background && manifest.background.source
      && !requiredSources.has(manifest.background.source.src)
      ? manifest.background.source.src
      : null;
    const entries = sourceList(manifest).map((source) => new Promise((resolve, reject) => {
      const image = new windowRef.Image();
      image.decoding = "async";
      image.onload = () => resolve([source, image]);
      image.onerror = () => {
        if (source === optionalBackgroundSource) {
          // 背景不是动画必需资源；renderScene 会继续使用 manifest.background.color。
          resolve([source, null]);
          return;
        }
        reject(new Error(`无法加载启动动画素材: ${source}`));
      };
      image.src = source;
    }));
    return Promise.all(entries).then((loaded) => Object.fromEntries(loaded.filter(([, image]) => image)));
  }

  function createStartupPlayer(options = {}) {
    const manifest = normalizeManifest(options.manifest || options);
    const documentRef = options.documentRef || (root && root.document) || null;
    const windowRef = options.windowRef || root || null;
    const canvas = options.canvas || (documentRef && documentRef.getElementById(options.canvasId || "coolzhu-seven-letter-startup-canvas"));
    if (!canvas || typeof canvas.getContext !== "function") throw new Error("找不到七字启动动画 canvas");
    const ctx = canvas.getContext("2d");
    let assets = options.assets || null;
    let active = false;
    let finished = false;
    let rafId = 0;
    let timerId = 0;
    let startedAt = null;
    let lastElapsed = 0;
    let lastScene = null;

    function resize() {
      canvas.width = manifest.canvas.width;
      canvas.height = manifest.canvas.height;
      if (canvas.style) {
        // 保留 1680×900 的内部像素缓冲；显示尺寸交给启动页 CSS 的等比约束，
        // 避免 inline 固定宽度把 1440px 窗口右侧 U 裁掉。
        canvas.style.width = "";
        canvas.style.height = "";
      }
      if (lastScene) lastScene = renderScene(ctx, manifest, assets || {}, lastElapsed);
    }

    function cancelFrame() {
      if (rafId) {
        if (windowRef && typeof windowRef.cancelAnimationFrame === "function") windowRef.cancelAnimationFrame(rafId);
        else if (windowRef && typeof windowRef.clearTimeout === "function") windowRef.clearTimeout(rafId);
        rafId = 0;
      }
      if (timerId && windowRef && typeof windowRef.clearTimeout === "function") {
        windowRef.clearTimeout(timerId);
        timerId = 0;
      }
    }

    function scheduleFrame(callback) {
      if (windowRef && typeof windowRef.requestAnimationFrame === "function") return windowRef.requestAnimationFrame(callback);
      if (windowRef && typeof windowRef.setTimeout === "function") return windowRef.setTimeout(() => callback(Date.now()), 16);
      return 0;
    }

    function renderAt(elapsed) {
      lastElapsed = Math.max(0, finite(elapsed, 0));
      lastScene = renderScene(ctx, manifest, assets || {}, lastElapsed);
      return lastScene;
    }

    function finish(reason, error = null) {
      if (finished) return false;
      finished = true;
      active = false;
      cancelFrame();
      const state = timelineStateAt(manifest, lastElapsed);
      if (reason === "completed" && typeof options.onComplete === "function") options.onComplete(state, lastScene);
      if (error && typeof options.onError === "function") options.onError(error);
      if (documentRef && typeof documentRef.dispatchEvent === "function") {
        let event = null;
        if (windowRef && typeof windowRef.CustomEvent === "function") event = new windowRef.CustomEvent(manifest.completeEvent, { detail: { reason, state, error } });
        else if (typeof CustomEvent === "function") event = new CustomEvent(manifest.completeEvent, { detail: { reason, state, error } });
        if (event) documentRef.dispatchEvent(event);
      }
      return true;
    }

    function tick(timestamp) {
      if (!active || finished) return;
      if (startedAt === null) startedAt = timestamp;
      const elapsed = timestamp - startedAt;
      renderAt(elapsed);
      if (elapsed >= manifest.totalDurationMs) {
        finish("completed");
        return;
      }
      rafId = scheduleFrame(tick);
    }

    function start() {
      if (active || finished) return false;
      active = true;
      resize();
      const ready = assets ? Promise.resolve(assets) : loadAssets(manifest, windowRef);
      ready.then((loaded) => {
        if (!active || finished) return;
        assets = loaded;
        startedAt = null;
        if (options.reducedMotion === true) {
          renderAt(manifest.totalDurationMs);
          const reducedDelay = Math.max(0, finite(options.reducedMotionDelayMs, 300));
          if (windowRef && typeof windowRef.setTimeout === "function") {
            timerId = windowRef.setTimeout(() => finish("reduced-motion"), reducedDelay);
          } else {
            finish("reduced-motion");
          }
          return;
        }
        rafId = scheduleFrame(tick);
      }).catch((error) => finish("resource-error", error));
      return true;
    }

    function stop(reason = "stopped") {
      return finish(reason);
    }

    return Object.freeze({
      manifest,
      start,
      stop,
      finish,
      renderAt,
      getState: () => timelineStateAt(manifest, lastElapsed),
      getLastScene: () => lastScene,
      isActive: () => active,
      isFinished: () => finished,
    });
  }

  return Object.freeze({
    COMPLETE_EVENT,
    FRAME_COUNT,
    LETTER_COUNT,
    WORD,
    actorPlacement,
    createStartupPlayer,
    drawActor,
    drawGlyph,
    loadAssets,
    normalizeManifest,
    renderScene,
    revealClipRect,
    sourceList,
    timelineStateAt,
  });
});
