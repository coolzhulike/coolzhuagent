#!/usr/bin/env node
/*
 * Repeatable frontend acceptance recorder for the guarded realtime loop.
 *
 * It drives the real web-console UI on the fixed local port 8765, starts the
 * unified realtime session, verifies live UI-DETR status plus browser
 * MediaRecorder input, sends one model turn through the composer, observes
 * browser TTS playback, and writes video/screenshot/json artifacts.
 */
const fs = require("fs");
const path = require("path");

function findRepoRoot(startDir) {
  let dir = startDir;
  while (dir && dir !== path.dirname(dir)) {
    if (fs.existsSync(path.join(dir, "Cargo.toml")) && fs.existsSync(path.join(dir, "modules"))) {
      return dir;
    }
    dir = path.dirname(dir);
  }
  return path.resolve(startDir, "..", "..", "..", "..");
}

function loadPlaywright(repoRoot) {
  const candidates = [
    "playwright",
    path.join(repoRoot, "node_modules", "playwright"),
    path.join(repoRoot, "tmp", "playwright-capture", "node_modules", "playwright"),
  ];
  const errors = [];
  for (const candidate of candidates) {
    try {
      return require(candidate);
    } catch (error) {
      errors.push(`${candidate}: ${error.message}`);
    }
  }
  throw new Error(
    "Playwright is not available. Install it with npm in a repo-local dependency directory, "
    + "or reuse tmp/playwright-capture/node_modules. Attempts:\n"
    + errors.join("\n")
  );
}

const repoRoot = findRepoRoot(__dirname);
const { chromium } = loadPlaywright(repoRoot);

const baseUrl = process.env.COOLZHU_E2E_BASE_URL || "http://127.0.0.1:8765";
const outDir = path.resolve(process.env.COOLZHU_E2E_OUT_DIR || path.join(repoRoot, "output", "playwright"));
const prefix = process.env.COOLZHU_E2E_PREFIX || "realtime-vision-voice-model-tts";
const headed = process.env.COOLZHU_E2E_HEADLESS !== "1";
const timeoutMs = Number(process.env.COOLZHU_E2E_TIMEOUT_MS || 180000);
const voiceText = process.env.COOLZHU_E2E_VOICE_TEXT
  || "实时视觉语音验收：请用一句话回复，说明你已收到本次实时视觉和语音测试。";

fs.mkdirSync(outDir, { recursive: true });

const videoOut = path.join(outDir, `${prefix}.webm`);
const runningShot = path.join(outDir, `${prefix}-running.png`);
const finalShot = path.join(outDir, `${prefix}-final.png`);
const resultJson = path.join(outDir, `${prefix}.json`);
const fakeAudio = path.join(outDir, `${prefix}-fake-mic.wav`);
const ttsWav = path.join(outDir, `${prefix}.wav`);

function writeSineWaveWav(filePath, seconds = 45, sampleRate = 16000, toneSeconds = 3) {
  const samples = Math.floor(seconds * sampleRate);
  const toneSamples = Math.floor(toneSeconds * sampleRate);
  const channels = 1;
  const bitsPerSample = 16;
  const byteRate = sampleRate * channels * bitsPerSample / 8;
  const blockAlign = channels * bitsPerSample / 8;
  const dataSize = samples * blockAlign;
  const buffer = Buffer.alloc(44 + dataSize);
  buffer.write("RIFF", 0);
  buffer.writeUInt32LE(36 + dataSize, 4);
  buffer.write("WAVE", 8);
  buffer.write("fmt ", 12);
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(channels, 22);
  buffer.writeUInt32LE(sampleRate, 24);
  buffer.writeUInt32LE(byteRate, 28);
  buffer.writeUInt16LE(blockAlign, 32);
  buffer.writeUInt16LE(bitsPerSample, 34);
  buffer.write("data", 36);
  buffer.writeUInt32LE(dataSize, 40);
  for (let i = 0; i < samples; i += 1) {
    if (i >= toneSamples) {
      buffer.writeInt16LE(0, 44 + i * 2);
      continue;
    }
    const t = i / sampleRate;
    const fade = sampleRate * 0.2;
    const envelope = Math.min(1, i / fade, (toneSamples - i) / fade);
    const sample = Math.round(Math.sin(2 * Math.PI * 440 * t) * 0.26 * envelope * 32767);
    buffer.writeInt16LE(sample, 44 + i * 2);
  }
  fs.writeFileSync(filePath, buffer);
}

async function fetchJson(page, url, options) {
  return await page.evaluate(async ({ url, options }) => {
    const resp = await fetch(url, options || {});
    const text = await resp.text();
    let data = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch (_) {
      data = { raw: text };
    }
    return { ok: resp.ok, status: resp.status, data };
  }, { url, options });
}

async function waitForStatus(page, predicate, timeout, label) {
  const start = Date.now();
  let last = null;
  while (Date.now() - start < timeout) {
    const response = await fetchJson(page, "/api/realtime/session/status");
    last = response.data;
    if (response.ok && predicate(last)) {
      return last;
    }
    await page.waitForTimeout(800);
  }
  throw new Error(`${label} timed out; last=${JSON.stringify(last)}`);
}

function stage(summary, name, extra = {}) {
  summary.stages.push({ name, at: new Date().toISOString(), ...extra });
}

async function main() {
  writeSineWaveWav(fakeAudio);
  const consoleLines = [];
  const browser = await chromium.launch({
    headless: !headed,
    args: [
      "--use-fake-ui-for-media-stream",
      "--use-fake-device-for-media-stream",
      `--use-file-for-fake-audio-capture=${fakeAudio}`,
      "--autoplay-policy=no-user-gesture-required",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1707, height: 960 },
    recordVideo: { dir: outDir, size: { width: 1707, height: 960 } },
    permissions: ["microphone"],
  });
  await context.grantPermissions(["microphone"], { origin: baseUrl });
  const page = await context.newPage();
  page.on("console", (msg) => consoleLines.push(`${msg.type()}: ${msg.text()}`));
  page.on("pageerror", (error) => consoleLines.push(`pageerror: ${error.message}`));
  await page.addInitScript(() => {
    window.__coolzhuE2E = { audioPlays: [], audioErrors: [] };
    const originalPlay = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = function patchedPlay(...args) {
      const record = {
        tag: this.tagName,
        src: this.currentSrc || this.src || "",
        startedAt: Date.now(),
        status: "called",
      };
      window.__coolzhuE2E.audioPlays.push(record);
      return originalPlay.apply(this, args)
        .then((value) => {
          record.status = "resolved";
          return value;
        })
        .catch((error) => {
          record.status = "rejected";
          record.error = error?.message || String(error);
          window.__coolzhuE2E.audioErrors.push(record);
          throw error;
        });
    };
  });

  let video = null;
  const summary = {
    baseUrl,
    runningShot,
    finalShot,
    videoOut,
    resultJson,
    fakeAudio,
    ttsWav,
    startedAt: new Date().toISOString(),
    stages: [],
    consoleLines,
  };

  try {
    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
    await page.locator('[data-role="message-input"]').waitFor({ timeout: 30000 });
    stage(summary, "page_loaded");

    await page.evaluate(() => document.querySelector('[data-action="realtime-session-start"]')?.click());
    const started = await waitForStatus(
      page,
      (s) => Boolean(s?.running && s?.audio_realtime_running && s?.detection_service_reachable),
      45000,
      "realtime session start"
    );
    stage(summary, "realtime_started", { status: started });

    const live = await waitForStatus(
      page,
      (s) => Number(s?.vision_frames_processed || 0) >= 2 && Number(s?.audio_segments_received || 0) >= 1,
      60000,
      "vision frames and microphone segments"
    );
    stage(summary, "vision_audio_live", { status: live });
    await page.screenshot({ path: runningShot, fullPage: false });

    const partial = await fetchJson(page, "/api/audio/realtime/partial", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        text: voiceText,
        confidence: 0.92,
        is_final: true,
        provider: "browser_speech_recognition",
        language: "zh-CN",
        speech_ms: 2300,
        tts_playing: false,
        vad_active: true,
      }),
    });
    stage(summary, "partial_asr_injected_from_browser", { response: partial });

    await page.evaluate((text) => {
      const input = document.querySelector('[data-role="message-input"]');
      input.value = text;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.focus();
    }, voiceText);
    await page.locator('[data-action="send-message"]').click();
    stage(summary, "message_sent_from_frontend");

    await page.waitForFunction(
      () => (window.__coolzhuE2E?.audioPlays?.length || 0) > 0,
      { timeout: timeoutMs }
    );
    await page.waitForTimeout(2500);
    const afterReply = await fetchJson(page, "/api/realtime/session/status");
    const audioProbe = await page.evaluate(() => window.__coolzhuE2E || {});
    stage(summary, "tts_play_observed", { status: afterReply.data, audioProbe });

    const firstAudio = audioProbe.audioPlays?.[0]?.src;
    if (firstAudio) {
      const audioResp = await fetch(firstAudio).catch(() => null);
      if (audioResp?.ok) {
        fs.writeFileSync(ttsWav, Buffer.from(await audioResp.arrayBuffer()));
      }
    }

    await page.evaluate(() => document.querySelector('[data-action="task-card-chain"]')?.click());
    await page.waitForTimeout(1000);
    await page.screenshot({ path: finalShot, fullPage: false });

    await page.evaluate(() => document.querySelector('[data-action="realtime-session-stop"]')?.click());
    const stopped = await waitForStatus(page, (s) => !s?.running, 30000, "realtime session stop");
    stage(summary, "realtime_stopped", { status: stopped });
    summary.finishedAt = new Date().toISOString();
    summary.ok = true;
  } catch (error) {
    summary.ok = false;
    summary.error = error?.stack || error?.message || String(error);
    try {
      await page.screenshot({ path: finalShot, fullPage: false });
    } catch (_) {}
    try {
      const status = await fetchJson(page, "/api/realtime/session/status");
      summary.failureStatus = status.data;
    } catch (_) {}
  } finally {
    video = page.video();
    fs.writeFileSync(resultJson, JSON.stringify(summary, null, 2), "utf8");
    await context.close();
    await browser.close();
    if (video) {
      const rawPath = await video.path();
      fs.copyFileSync(rawPath, videoOut);
    }
    fs.writeFileSync(resultJson, JSON.stringify(summary, null, 2), "utf8");
    console.log(JSON.stringify({
      ok: summary.ok,
      error: summary.error || null,
      videoOut,
      runningShot,
      finalShot,
      resultJson,
      ttsWav,
    }, null, 2));
    if (!summary.ok) {
      process.exitCode = 1;
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
