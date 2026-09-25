// 「满林风动」banner 风场动画（docs/plans/2026-07-10-banner-wind-animation-plan.md）。
// 架构：静态分层素材 + 程序化统一风场（零帧间漂移）。分层素材缺失时自动回退整图模式。
// 与旧 bamboo-leaves.js 的差异：新增分层渲染与全局风场；按方案移除月晕呼吸（drawMoonHalo）。

const leafSources = [
  "./assets/ui-redesign/bamboo-leaves/leaf-01-flat.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-02-curl.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-03-twig.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-04-edge.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-05-spin.png",
];

const layerSources = {
  sky: "./assets/ui-redesign/banner-layers/layer0-sky-moon-mountains.png",
  cloudA: "./assets/ui-redesign/banner-layers/layer1-cloud-a.png",
  cloudB: "./assets/ui-redesign/banner-layers/layer1-cloud-b.png",
  bambooFar: "./assets/ui-redesign/banner-layers/layer2-bamboo-far.png",
  bambooLeft: "./assets/ui-redesign/banner-layers/layer3-bamboo-near-left.png",
  bambooRight: "./assets/ui-redesign/banner-layers/layer3-bamboo-near-right.png",
  logoText: "./assets/ui-redesign/banner-layers/layer4-logo-leaftext.png",
  figure: "./assets/ui-redesign/banner-layers/layer5-figure.png",
};

// 动效手感参数（Phase C 调参集中处；角度单位：弧度）。
const WIND = {
  swayPeriodMs: 5200,        // 全局风相位基础周期
  gustEveryMs: [8000, 15000],
  gustHoldMs: [1200, 2000],
  gustAttack: 3.2,           // 阵风包络进入速率（1/s）
  gustRelease: 0.9,          // 阵风包络衰退速率（1/s）
  farSkew: 0.010,            // L2 远竹 skew 幅度（常态）
  farSkewGust: 0.014,        // 阵风附加
  nearRotate: 0.024,         // L3 近竹枝旋转幅度
  nearRotateGust: 0.03,
  logoSkew: 0.009,           // L4 logo 字 skew 幅度
  logoSkewGust: 0.012,
  cloudASpeed: 1 / 90000,    // 云 A：全宽/90s
  cloudBSpeed: 1 / 55000,
  cloudGustBoost: 0.8,       // 阵风时云加速倍率（用户反馈 2.4 太快，降为温和加速）
  sweepHalfWidth: 96,        // 风拂高光半宽（羽化范围，越大越柔）
  sweepTiltDeg: 16,          // 高光带倾角（斜向掠过更像风）
  sweepAlpha: 0.10,          // 高光常态峰值透明度
  sweepGustAlpha: 0.08,      // 阵风附加透明度
};

const reduceMotionQuery = window.matchMedia?.("(prefers-reduced-motion: reduce)");

if (!reduceMotionQuery?.matches) {
  startBambooWind().catch(() => {});
}

async function startBambooWind() {
  const banner = document.querySelector(".brand-banner");
  const bannerImage = banner?.querySelector(".brand-banner-image");
  if (!banner || !bannerImage) return;

  const [leafImages] = await Promise.all([
    Promise.all(leafSources.map(loadImage)),
    waitForImage(bannerImage),
  ]);

  // 分层素材：全部加载成功才启用分层模式，否则回退整图模式（保持可用）。
  let layers = null;
  try {
    const entries = await Promise.all(
      Object.entries(layerSources).map(async ([key, source]) => [key, await loadImage(source)]),
    );
    layers = Object.fromEntries(entries);
  } catch (_error) {
    layers = null;
  }

  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { alpha: true });
  if (!context) return;

  canvas.className = "bamboo-leaf-rain";
  canvas.setAttribute("aria-hidden", "true");
  Object.assign(canvas.style, {
    position: "absolute",
    inset: "0",
    width: "100%",
    height: "100%",
    pointerEvents: "none",
    zIndex: "2",
  });
  banner.append(canvas);

  const state = {
    canvas,
    context,
    banner,
    bannerImage,
    layers,
    images: leafImages,
    width: 0,
    height: 0,
    dpr: 1,
    rafId: 0,
    lastTime: 0,
    particles: [],
    spawnBank: 0,
    normalTarget: randomInt(5, 8),
    retargetAt: performance.now() + randomBetween(3400, 6200),
    gustUntil: 0,
    nextGustAt: performance.now() + randomBetween(...WIND.gustEveryMs),
    gustLevel: 0, // 平滑阵风包络 0..1
  };

  const resize = () => resizeCanvas(state);
  resize();
  drawScene(state, performance.now());
  bannerImage.style.opacity = "0";

  const observer = typeof ResizeObserver === "function" ? new ResizeObserver(resize) : null;
  if (observer) {
    observer.observe(banner);
  } else {
    window.addEventListener("resize", resize, { passive: true });
  }

  const stop = () => {
    if (state.rafId) {
      cancelAnimationFrame(state.rafId);
      state.rafId = 0;
    }
    state.lastTime = 0;
  };

  const cleanup = () => {
    stop();
    observer?.disconnect();
    window.removeEventListener("resize", resize);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    reduceMotionQuery?.removeEventListener?.("change", onMotionPreferenceChange);
    bannerImage.style.opacity = "";
    canvas.remove();
  };

  const tick = (time) => {
    if (document.hidden) {
      stop();
      return;
    }
    if (!state.lastTime) state.lastTime = time;
    const delta = Math.min((time - state.lastTime) / 1000, 0.05);
    state.lastTime = time;

    updateWind(state, time, delta);
    updateParticles(state, time, delta);
    drawScene(state, time);
    state.rafId = requestAnimationFrame(tick);
  };

  const start = () => {
    if (!state.rafId && !document.hidden) {
      state.rafId = requestAnimationFrame(tick);
    }
  };

  function onVisibilityChange() {
    if (document.hidden) {
      stop();
    } else {
      start();
    }
  }

  function onMotionPreferenceChange(event) {
    if (event.matches) cleanup();
  }

  document.addEventListener("visibilitychange", onVisibilityChange);
  reduceMotionQuery?.addEventListener?.("change", onMotionPreferenceChange);

  seedParticles(state, performance.now());
  start();
}

function loadImage(source) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.decoding = "async";
    image.onload = () => resolve(image);
    image.onerror = reject;
    image.src = source;
  });
}

function waitForImage(image) {
  if (image.complete && image.naturalWidth > 0) return Promise.resolve(image);
  return new Promise((resolve, reject) => {
    image.addEventListener("load", () => resolve(image), { once: true });
    image.addEventListener("error", reject, { once: true });
  });
}

function resizeCanvas(state) {
  const rect = state.canvas.getBoundingClientRect();
  const width = Math.max(1, rect.width);
  const height = Math.max(1, rect.height);
  const dpr = Math.min(window.devicePixelRatio || 1, 2);

  state.width = width;
  state.height = height;
  state.dpr = dpr;
  state.canvas.width = Math.round(width * dpr);
  state.canvas.height = Math.round(height * dpr);
  state.context.setTransform(dpr, 0, 0, dpr, 0, 0);
}

// ---------- 统一风场 ----------

function updateWind(state, now, delta) {
  if (now >= state.nextGustAt) {
    state.gustUntil = now + randomBetween(...WIND.gustHoldMs);
    state.nextGustAt = now + randomBetween(...WIND.gustEveryMs);
  }
  const gustActive = now < state.gustUntil;
  const rate = gustActive ? WIND.gustAttack : WIND.gustRelease;
  const target = gustActive ? 1 : 0;
  state.gustLevel += (target - state.gustLevel) * Math.min(1, rate * delta);

  // 输出到 CSS 变量：供未来 DOM 层（或其它 UI 元素）消费同一风场。
  const sway = windSway(now, 0);
  state.banner.style.setProperty("--wind-sway", sway.toFixed(4));
  state.banner.style.setProperty("--wind-gust", state.gustLevel.toFixed(4));
}

// 全局风相位：不同层通过 phaseOffset 取错峰值，但同源，联动一致。
function windSway(now, phaseOffset) {
  const base = Math.sin((now / WIND.swayPeriodMs) * Math.PI * 2 + phaseOffset);
  const flutter = Math.sin((now / 1730) * Math.PI * 2 + phaseOffset * 1.7) * 0.22;
  return Math.max(-1, Math.min(1, base + flutter));
}

// ---------- 粒子（沿用旧逻辑，gust 接入平滑包络） ----------

function seedParticles(state, now) {
  const count = Math.min(state.normalTarget, 7);
  for (let index = 0; index < count; index += 1) {
    const particle = createParticle(state, now);
    particle.baseX = randomBetween(state.width * 0.78, state.width * 1.04);
    particle.y = randomBetween(-particle.size * 0.4, state.height * 0.24);
    particle.age = randomBetween(0, 4);
    state.particles.push(particle);
  }
}

function updateParticles(state, now, delta) {
  const gustActive = now < state.gustUntil;
  if (!gustActive && now >= state.retargetAt) {
    state.normalTarget = randomInt(5, 8);
    state.retargetAt = now + randomBetween(3400, 6200);
  }

  const targetCount = gustActive ? 12 : state.normalTarget;
  const spawnRate = gustActive ? 3.6 : 1.2;
  state.spawnBank += delta * spawnRate;

  while (state.particles.length < targetCount && state.spawnBank >= 1) {
    state.particles.push(createParticle(state, now));
    state.spawnBank -= 1;
  }

  while (state.particles.length < Math.min(targetCount, gustActive ? 9 : 5)) {
    state.particles.push(createParticle(state, now));
  }

  for (const particle of state.particles) {
    const speedFactor = 1 + state.gustLevel * 1.2;
    const phase = particle.phase + particle.age * particle.swaySpeed;
    const endpointBrake = 0.2 + Math.abs(Math.cos(phase)) * 0.8;

    particle.age += delta;
    particle.baseX -= particle.velocityX * speedFactor * delta;
    particle.y += particle.velocityY * delta;
    particle.rotation += particle.spin * endpointBrake * delta;

    const x = particle.baseX + Math.sin(particle.phase + particle.age * particle.swaySpeed) * particle.swayAmplitude;
    if (!particle.fadeStart && (x < -particle.size * 0.55 || particle.y > state.height * 0.72)) {
      particle.fadeStart = now;
    }
  }

  state.particles = state.particles.filter((particle) => {
    if (!particle.fadeStart) return true;
    return now - particle.fadeStart < 600;
  });
}

// ---------- 场景渲染 ----------

function drawScene(state, now) {
  const { context, width, height } = state;
  context.clearRect(0, 0, width, height);

  if (state.layers) {
    drawLayeredScene(state, now);
  } else {
    // 回退：分层素材缺失时绘制原始整图 + 条带扭曲伪摆动（无月晕呼吸）。
    context.drawImage(state.bannerImage, 0, 0, width, height);
    const gust = state.gustLevel;
    drawWindRegion(state, now, { x: 0, y: 0, width: 0.15, height: 1, amplitude: 1.8 + gust * 2.2, phase: 0.1 });
    drawWindRegion(state, now, { x: 0.91, y: 0, width: 0.09, height: 1, amplitude: 1.5 + gust * 2.4, phase: 1.4 });
    drawWindRegion(state, now, { x: 0.79, y: 0.13, width: 0.13, height: 0.68, amplitude: 0.9 + gust * 1.5, phase: 2.2 });
  }

  drawLogoSweep(state, now);
  drawWindTrails(state, now);
  drawParticles(state, now);
}

function drawLayeredScene(state, now) {
  const { context, width, height, layers } = state;
  const gust = state.gustLevel;

  // L0 天空/月亮/远山：静止。
  context.drawImage(layers.sky, 0, 0, width, height);

  // L1 云：横向循环平移（远云慢、近云快），阵风加速；两份绘制实现无缝绕回。
  drawCloud(state, layers.cloudA, now * WIND.cloudASpeed * (1 + gust * WIND.cloudGustBoost));
  drawCloud(state, layers.cloudB, now * WIND.cloudBSpeed * (1 + gust * WIND.cloudGustBoost) + 0.37);

  // L2 远景竹林：绕底边 skewX 慢摆。
  drawSkewedLayer(state, layers.bambooFar, {
    skew: windSway(now, 0.4) * (WIND.farSkew + gust * WIND.farSkewGust),
    originY: height,
  });

  // L3 近景竹枝：左右簇绕各自枝根旋转，错频。
  drawRotatedLayer(state, layers.bambooLeft, {
    angle: windSway(now, 1.3) * (WIND.nearRotate + gust * WIND.nearRotateGust),
    originX: 0,
    originY: height * 0.1,
  });
  drawRotatedLayer(state, layers.bambooRight, {
    angle: windSway(now, 2.1) * (WIND.nearRotate + gust * WIND.nearRotateGust) * -1,
    originX: width,
    originY: height * 0.08,
  });

  // L4 logo 竹叶字：绕底边中心 skewX 微摆（重点元素，幅度克制以保可读性）。
  drawSkewedLayer(state, layers.logoText, {
    skew: windSway(now, 3.0) * (WIND.logoSkew + gust * WIND.logoSkewGust),
    originY: height * 0.72,
  });

  // L5 人物：静止（保真优先）。
  context.drawImage(layers.figure, 0, 0, width, height);
}

function drawCloud(state, image, progress) {
  const { context, width, height } = state;
  const span = width * 1.3; // 云循环行程（含离场余量）
  const offset = ((progress % 1) + 1) % 1 * span - width * 0.15;
  context.save();
  context.globalAlpha = 0.9;
  context.drawImage(image, -offset, 0, width, height);
  context.drawImage(image, -offset + span, 0, width, height);
  context.restore();
}

function drawSkewedLayer(state, image, { skew, originY }) {
  const { context, width, height } = state;
  context.save();
  context.translate(0, originY);
  context.transform(1, 0, skew, 1, 0, 0);
  context.translate(0, -originY);
  context.drawImage(image, 0, 0, width, height);
  context.restore();
}

function drawRotatedLayer(state, image, { angle, originX, originY }) {
  const { context, width, height } = state;
  context.save();
  context.translate(originX, originY);
  context.rotate(angle);
  context.translate(-originX, -originY);
  context.drawImage(image, 0, 0, width, height);
  context.restore();
}

// 回退模式用：单图分区条带扭曲（伪摆动），来自旧 bamboo-leaves.js 试验版。
function drawWindRegion(state, now, region) {
  const { context, width, height, bannerImage } = state;
  const x = region.x * width;
  const y = region.y * height;
  const regionWidth = region.width * width;
  const regionHeight = region.height * height;
  const sourceScaleX = bannerImage.naturalWidth / width;
  const sourceScaleY = bannerImage.naturalHeight / height;
  const padding = 6;
  const stripHeight = 2;

  context.save();
  context.beginPath();
  context.rect(x, y, regionWidth, regionHeight);
  context.clip();
  context.clearRect(x, y, regionWidth, regionHeight);

  for (let offsetY = 0; offsetY < regionHeight; offsetY += stripHeight) {
    const progress = offsetY / regionHeight;
    const anchorWeight = Math.pow(1 - progress, 1.35);
    const wave = Math.sin(now / 940 + region.phase + progress * 2.6);
    const shift = wave * region.amplitude * (0.18 + anchorWeight * 0.82);
    const drawHeight = Math.min(stripHeight + 0.8, regionHeight - offsetY);
    const sourceX = Math.max(0, (x - padding) * sourceScaleX);
    const sourceWidth = Math.min(
      bannerImage.naturalWidth - sourceX,
      (regionWidth + padding * 2) * sourceScaleX,
    );
    context.drawImage(
      bannerImage,
      sourceX,
      (y + offsetY) * sourceScaleY,
      sourceWidth,
      drawHeight * sourceScaleY,
      x - padding + shift,
      y + offsetY,
      regionWidth + padding * 2,
      drawHeight,
    );
  }
  context.restore();
}

// 风拂高光：斜向柔和亮带掠过 logo 竹叶字（叶面反光）。
// 分层模式下用 logo 层 alpha 作遮罩——高光只落在叶字像素上，消除矩形裁剪的"像素方块感"；
// 强度与阵风联动（常态若隐若现，风起时明显），更接近"风拂过叶面"。
function drawLogoSweep(state, now) {
  const { context, width, height } = state;
  const gust = state.gustLevel;
  const alpha = WIND.sweepAlpha * (0.35 + gust * 0.65) + gust * WIND.sweepGustAlpha;
  if (alpha < 0.01) return;

  const logoLeft = width * 0.2;
  const logoWidth = width * 0.56;
  const sweep = logoLeft + ((now % 7200) / 7200) * (logoWidth + WIND.sweepHalfWidth * 2) - WIND.sweepHalfWidth;
  const tilt = (WIND.sweepTiltDeg * Math.PI) / 180;
  const dx = Math.cos(tilt) * WIND.sweepHalfWidth;
  const dy = Math.sin(tilt) * WIND.sweepHalfWidth;
  const gradient = context.createLinearGradient(sweep - dx, height / 2 - dy, sweep + dx, height / 2 + dy);
  gradient.addColorStop(0, "rgba(224, 255, 216, 0)");
  gradient.addColorStop(0.5, `rgba(224, 255, 216, ${alpha})`);
  gradient.addColorStop(1, "rgba(224, 255, 216, 0)");

  if (state.layers) {
    // 离屏合成：logo 层 alpha ∩ 斜向渐变 → 高光只存在于叶字内部。
    if (!state.sweepCanvas) {
      state.sweepCanvas = document.createElement("canvas");
    }
    const sweepCanvas = state.sweepCanvas;
    if (sweepCanvas.width !== state.canvas.width || sweepCanvas.height !== state.canvas.height) {
      sweepCanvas.width = state.canvas.width;
      sweepCanvas.height = state.canvas.height;
    }
    const sctx = sweepCanvas.getContext("2d");
    sctx.setTransform(state.dpr, 0, 0, state.dpr, 0, 0);
    sctx.clearRect(0, 0, width, height);
    sctx.drawImage(state.layers.logoText, 0, 0, width, height);
    sctx.globalCompositeOperation = "source-in";
    sctx.fillStyle = gradient;
    sctx.fillRect(0, 0, width, height);
    sctx.globalCompositeOperation = "source-over";

    context.save();
    context.globalCompositeOperation = "screen";
    context.drawImage(sweepCanvas, 0, 0, state.canvas.width / state.dpr, state.canvas.height / state.dpr);
    context.restore();
    return;
  }

  // 回退模式（无分层素材）：无 alpha 可用，退化为无裁剪边界的斜向宽羽化淡光。
  context.save();
  context.globalCompositeOperation = "screen";
  context.fillStyle = gradient;
  context.fillRect(logoLeft, height * 0.2, logoWidth, height * 0.62);
  context.restore();
}

function drawWindTrails(state, now) {
  const { context, width, height } = state;
  const gust = state.gustLevel;
  const drift = Math.sin(now / 760) * 8;
  context.save();
  context.lineCap = "round";
  context.lineWidth = 0.7;
  context.strokeStyle = `rgba(142, 211, 177, ${0.08 + gust * 0.08})`;
  for (let index = 0; index < 3; index += 1) {
    const y = height * (0.2 + index * 0.19) + Math.sin(now / 820 + index) * 3;
    context.beginPath();
    context.moveTo(width * (0.67 + index * 0.035), y);
    context.bezierCurveTo(
      width * 0.73 + drift,
      y - 6,
      width * 0.79 + drift,
      y + 5,
      width * 0.86,
      y - 2,
    );
    context.stroke();
  }
  context.restore();
}

function drawParticles(state, now) {
  const { context, width } = state;
  const gustTilt = state.gustLevel * (-6 * Math.PI) / 180;

  for (const particle of state.particles) {
    const phase = particle.phase + particle.age * particle.swaySpeed;
    const x = particle.baseX + Math.sin(phase) * particle.swayAmplitude;
    const fade = particle.fadeStart ? Math.max(0, 1 - (now - particle.fadeStart) / 600) : 1;
    const moonlitAlpha = x < width * 0.22 ? 0.9 : particle.alpha;

    context.save();
    context.globalAlpha = Math.min(1, moonlitAlpha) * fade;
    context.translate(x, particle.y);
    context.rotate(particle.rotation + gustTilt);
    context.drawImage(particle.image, -particle.size / 2, -particle.size / 2, particle.size, particle.size);
    context.restore();
  }
}

function createParticle(state, now) {
  const size = 128 * randomBetween(0.12, 0.28);
  const period = randomBetween(2.4, 4);

  return {
    image: state.images[randomInt(0, state.images.length - 1)],
    baseX: randomBetween(state.width * 0.78, state.width + size * 0.7),
    y: randomBetween(-size, state.height * 0.12),
    size,
    age: 0,
    phase: randomBetween(0, Math.PI * 2),
    swayAmplitude: randomBetween(6, 14),
    swaySpeed: (Math.PI * 2) / period,
    velocityX: randomBetween(8, 20),
    velocityY: randomBetween(6, 14),
    rotation: randomBetween(-Math.PI, Math.PI),
    spin: randomBetween(-0.7, 0.7),
    alpha: randomBetween(0.58, 0.82),
    fadeStart: null,
    bornAt: now,
  };
}

function randomBetween(min, max) {
  return min + Math.random() * (max - min);
}

function randomInt(min, max) {
  return Math.floor(randomBetween(min, max + 1));
}
