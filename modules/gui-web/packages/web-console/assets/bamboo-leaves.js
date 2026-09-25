const leafSources = [
  "./assets/ui-redesign/bamboo-leaves/leaf-01-flat.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-02-curl.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-03-twig.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-04-edge.png",
  "./assets/ui-redesign/bamboo-leaves/leaf-05-spin.png",
];

const reduceMotionQuery = window.matchMedia?.("(prefers-reduced-motion: reduce)");

if (!reduceMotionQuery?.matches) {
  startBambooLeafRain().catch(() => {});
}

async function startBambooLeafRain() {
  const banner = document.querySelector(".brand-banner");
  const bannerImage = banner?.querySelector(".brand-banner-image");
  if (!banner || !bannerImage) return;

  const [images] = await Promise.all([
    Promise.all(leafSources.map(loadLeafImage)),
    waitForImage(bannerImage),
  ]);
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
    bannerImage,
    images,
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
    nextGustAt: performance.now() + randomBetween(8000, 15000),
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

function loadLeafImage(source) {
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
  if (now >= state.nextGustAt) {
    state.gustUntil = now + randomBetween(1200, 2000);
    state.nextGustAt = now + randomBetween(8000, 15000);
  }

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
    const speedFactor = gustActive ? 2.2 : 1;
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

function drawScene(state, now) {
  const { context, width, height, bannerImage } = state;
  const gust = now < state.gustUntil ? 1 : 0;
  context.clearRect(0, 0, width, height);
  context.drawImage(bannerImage, 0, 0, width, height);

  drawWindRegion(state, now, { x: 0, y: 0, width: 0.15, height: 1, amplitude: 1.8 + gust * 2.2, phase: 0.1 });
  drawWindRegion(state, now, { x: 0.91, y: 0, width: 0.09, height: 1, amplitude: 1.5 + gust * 2.4, phase: 1.4 });
  drawWindRegion(state, now, { x: 0.79, y: 0.13, width: 0.13, height: 0.68, amplitude: 0.9 + gust * 1.5, phase: 2.2 });
  drawMoonHalo(state, now, gust);
  drawLogoSweep(state, now, gust);
  drawWindTrails(state, now, gust);
  drawParticles(state, now);
}

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

function drawMoonHalo(state, now, gust) {
  const { context, width, height } = state;
  const pulse = 0.5 + Math.sin(now / 1450) * 0.5;
  const radius = height * (0.22 + pulse * 0.035 + gust * 0.025);
  const glow = context.createRadialGradient(width * 0.132, height * 0.25, 2, width * 0.132, height * 0.25, radius);
  glow.addColorStop(0, `rgba(220, 242, 229, ${0.11 + pulse * 0.05})`);
  glow.addColorStop(0.45, "rgba(149, 201, 190, 0.04)");
  glow.addColorStop(1, "rgba(149, 201, 190, 0)");
  context.save();
  context.globalCompositeOperation = "screen";
  context.fillStyle = glow;
  context.fillRect(width * 0.04, 0, width * 0.2, height * 0.55);
  context.restore();
}

function drawLogoSweep(state, now, gust) {
  const { context, width, height } = state;
  const logoLeft = width * 0.22;
  const logoWidth = width * 0.52;
  const sweep = ((now % 7200) / 7200) * (logoWidth + 90) - 45;
  const gradient = context.createLinearGradient(logoLeft + sweep - 38, 0, logoLeft + sweep + 38, 0);
  gradient.addColorStop(0, "rgba(224, 255, 216, 0)");
  gradient.addColorStop(0.5, `rgba(224, 255, 216, ${0.08 + gust * 0.05})`);
  gradient.addColorStop(1, "rgba(224, 255, 216, 0)");

  context.save();
  context.beginPath();
  context.rect(logoLeft, height * 0.25, logoWidth, height * 0.58);
  context.clip();
  context.globalCompositeOperation = "screen";
  context.fillStyle = gradient;
  context.fillRect(logoLeft, 0, logoWidth, height);
  context.restore();
}

function drawWindTrails(state, now, gust) {
  const { context, width, height } = state;
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
  const gustTilt = now < state.gustUntil ? (-6 * Math.PI) / 180 : 0;

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
