#!/usr/bin/env node
/*
 * Real backend speech regression for the realtime voice path.
 *
 * This script intentionally does not use browser media simulation. It validates:
 *   1. backend TTS synthesis,
 *   2. backend STT transcription of that synthesized audio,
 *   3. realtime final audio segment STT,
 *   4. real frontend message send with the transcript,
 *   5. backend TTS synthesis of the model reply and browser playback.
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
    "Playwright is not available. Install it with npm or reuse tmp/playwright-capture/node_modules. Attempts:\n"
    + errors.join("\n")
  );
}

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  console.log(`Usage: node realtime_real_speech_e2e.js [--help]

Runs the legacy synthesized-audio realtime regression against COOLZHU_E2E_BASE_URL.
This script validates backend TTS -> backend STT -> model -> browser TTS playback;
it does not capture a physical microphone and is not physical full-stream evidence.`);
  process.exit(0);
}

const repoRoot = findRepoRoot(__dirname);
const { chromium } = loadPlaywright(repoRoot);

const baseUrl = process.env.COOLZHU_E2E_BASE_URL || "http://127.0.0.1:8765";
const outDir = path.resolve(process.env.COOLZHU_E2E_OUT_DIR || path.join(repoRoot, "output", "playwright"));
const prefix = process.env.COOLZHU_E2E_PREFIX || "realtime-real-stt-tts-regression";
const headed = process.env.COOLZHU_E2E_HEADLESS !== "1";
const timeoutMs = Number(process.env.COOLZHU_E2E_TIMEOUT_MS || 240000);
const probePhrase = process.env.COOLZHU_REAL_SPEECH_TEXT || "hello coolzhu realtime voice regression test";
const preferredAgentId = process.env.COOLZHU_E2E_AGENT_ID || "";

fs.mkdirSync(outDir, { recursive: true });

const videoOut = path.join(outDir, `${prefix}.webm`);
const runningShot = path.join(outDir, `${prefix}-running.png`);
const finalShot = path.join(outDir, `${prefix}-final.png`);
const resultJson = path.join(outDir, `${prefix}.json`);
const sourceTtsWav = path.join(outDir, `${prefix}-source-tts.wav`);
const replyTtsWav = path.join(outDir, `${prefix}-reply-tts.wav`);
const assistantReplySelector = ".message.bot:not(.tool)";

function stage(summary, name, extra = {}) {
  summary.stages.push({ name, at: new Date().toISOString(), ...extra });
}

function resolveUrl(url) {
  return new URL(url, baseUrl).toString();
}

async function jsonFetch(url, options = {}) {
  const response = await fetch(resolveUrl(url), options);
  const text = await response.text();
  let data = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch (_) {
    data = { raw: text };
  }
  if (!response.ok) {
    throw new Error(`${url} failed with ${response.status}: ${text}`);
  }
  return data;
}

async function audioBytesFromUrl(url) {
  const response = await fetch(resolveUrl(url));
  if (!response.ok) {
    throw new Error(`${url} audio fetch failed with ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

function base64Data(bytes) {
  return Buffer.from(bytes).toString("base64");
}

function normalizedWords(text) {
  return String(text || "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
}

function assertTranscriptLooksReal(transcript, expectedWords) {
  const words = normalizedWords(transcript);
  const hits = expectedWords.filter((word) => words.includes(word));
  if (hits.length < Math.min(4, expectedWords.length)) {
    throw new Error(`STT transcript did not contain enough expected words. transcript=${JSON.stringify(transcript)} hits=${hits.join(",")}`);
  }
  return { words, hits };
}

async function synthesizeSpeech(text, outputPath) {
  const result = await jsonFetch("/api/audio/tts/speak", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text, auto_play: false, segment: false }),
  });
  if (!result.audio_url || !result.audio_path) {
    throw new Error(`TTS response missing audio_url/audio_path: ${JSON.stringify(result)}`);
  }
  const bytes = await audioBytesFromUrl(result.audio_url);
  fs.writeFileSync(outputPath, bytes);
  return { result, bytes };
}

async function transcribeAudioPath(audioPath, sessionId) {
  const result = await jsonFetch("/api/audio/stt/stop", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ session_id: sessionId, audio_path: audioPath }),
  });
  if (!String(result.text || "").trim()) {
    throw new Error(`STT returned empty text: ${JSON.stringify(result)}`);
  }
  return result;
}

async function transcribeRealtimeSegment(bytes, durationMs) {
  const start = await jsonFetch("/api/audio/realtime/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ auto_send_transcript: false, auto_tts_reply: false }),
  });
  const sessionId = start.session_id || `audio-rt-real-${Date.now()}`;
  try {
    const result = await jsonFetch("/api/audio/realtime/segment", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        session_id: sessionId,
        mime_type: "audio/wav",
        bytes: bytes.length,
        duration_ms: Math.max(500, Number(durationMs || 0)),
        final_segment: true,
        audio_base64: base64Data(bytes),
      }),
    });
    if (!result.asr_attempted || !result.asr_transcript_received || !String(result.asr_text || "").trim()) {
      throw new Error(`Realtime final segment STT did not return transcript: ${JSON.stringify(result)}`);
    }
    return result;
  } finally {
    await jsonFetch("/api/audio/realtime/stop", { method: "POST" }).catch(() => null);
  }
}

async function waitForAssistantReply(page, initialCount) {
  await page.waitForFunction(
    ({ count, selector }) => document.querySelectorAll(selector).length > count,
    { count: initialCount, selector: assistantReplySelector },
    { timeout: timeoutMs }
  );
  await page.waitForTimeout(1200);
  return await page.evaluate((selector) => {
    const messages = Array.from(document.querySelectorAll(selector));
    const last = messages[messages.length - 1];
    return last ? last.innerText.trim() : "";
  }, assistantReplySelector);
}

async function selectFirstConfiguredAgent(page) {
  await page.waitForFunction(
    () => document.querySelectorAll('[data-role="agent-targets"] input[type="checkbox"]').length > 0,
    null,
    { timeout: 30000 }
  );
  const selected = await page.evaluate(async ({ preferredAgentId }) => {
    const host = document.querySelector('[data-role="agent-targets"]');
    const boxes = Array.from(host?.querySelectorAll('input[type="checkbox"]') || []);
    let textAgentIds = [];
    try {
      const response = await fetch("/api/agents");
      const data = await response.json();
      textAgentIds = (data.agents || [])
        .filter((agent) => agent.enabled && agent.selectable && agent.model_type === "text")
        .map((agent) => agent.id);
    } catch (_) {
      textAgentIds = [];
    }
    const preferredTarget = preferredAgentId
      ? boxes.find((box) => !box.disabled && box.value === preferredAgentId)
      : null;
    const current = boxes.find((box) => (
      box.checked
      && !box.disabled
      && textAgentIds.includes(String(box.value || ""))
    ));
    const textTarget = boxes.find((box) => !box.disabled && textAgentIds.includes(String(box.value || "")));
    const sessionTarget = boxes.find((box) => !box.disabled && String(box.value || "").startsWith("session-"));
    const target = preferredTarget || current || textTarget || sessionTarget || boxes.find((box) => !box.disabled);
    if (!target) {
      return null;
    }
    if (preferredAgentId && target.value !== preferredAgentId) {
      return { error: `Preferred agent ${preferredAgentId} is not available in the chat target list.` };
    }
    boxes.forEach((box) => {
      box.checked = box === target;
      box.dispatchEvent(new Event("change", { bubbles: true }));
    });
    const trigger = document.querySelector('[data-role="agent-trigger"]');
    return {
      value: target.value || "",
      label: target.closest("label")?.innerText?.trim() || target.value || "",
      triggerText: trigger?.innerText?.trim() || "",
    };
  }, { preferredAgentId });
  if (selected?.error) {
    throw new Error(selected.error);
  }
  if (!selected?.value) {
    throw new Error("No configured chat agent target is available for frontend regression.");
  }
  return selected;
}

async function waitForAssistantReplyAfterUserTurn(page, before) {
  await page.waitForFunction(
    ({ userCount }) => document.querySelectorAll(".message.user").length > userCount,
    { userCount: before.userCount },
    { timeout: 15000 }
  );
  await page.waitForFunction(
    () => {
      const input = document.querySelector('[data-role="message-input"]');
      return input && input.value.trim() === "";
    },
    null,
    { timeout: 15000 }
  );
  return await waitForAssistantReply(page, before.botCount);
}

function spokenReplyText(text) {
  return String(text || "")
    .replace(/\n---\nRemote context usage:[\s\S]*$/m, "")
    .trim();
}

async function main() {
  const consoleLines = [];
  const summary = {
    baseUrl,
    runningShot,
    finalShot,
    videoOut,
    resultJson,
    sourceTtsWav,
    replyTtsWav,
    probePhrase,
    startedAt: new Date().toISOString(),
    stages: [],
    consoleLines,
  };

  let browser = null;
  let context = null;
  let page = null;
  let video = null;

  try {
    const audioStatus = await jsonFetch("/api/audio/status");
    if (!audioStatus.stt_available || !audioStatus.tts_available) {
      throw new Error(`Real STT/TTS unavailable: ${JSON.stringify(audioStatus)}`);
    }
    stage(summary, "audio_status_ready", { audioStatus });

    const ttsProbe = await synthesizeSpeech(probePhrase, sourceTtsWav);
    stage(summary, "tts_backend_probe", {
      audioPath: ttsProbe.result.audio_path,
      audioUrl: ttsProbe.result.audio_url,
      durationMs: ttsProbe.result.duration_ms,
      bytes: ttsProbe.bytes.length,
    });

    const sttProbe = await transcribeAudioPath(ttsProbe.result.audio_path, "real-stt-tts-regression");
    const sttQuality = assertTranscriptLooksReal(sttProbe.text, ["hello", "real", "time", "voice", "regression", "test"]);
    stage(summary, "stt_backend_probe", {
      text: sttProbe.text,
      confidence: sttProbe.confidence,
      durationMs: sttProbe.duration_ms,
      quality: sttQuality,
    });

    const realtimeSegment = await transcribeRealtimeSegment(ttsProbe.bytes, ttsProbe.result.duration_ms);
    const realtimeQuality = assertTranscriptLooksReal(realtimeSegment.asr_text, ["hello", "real", "time", "voice", "regression", "test"]);
    stage(summary, "realtime_segment_stt_probe", {
      asrText: realtimeSegment.asr_text,
      asrProvider: realtimeSegment.asr_provider,
      asrDurationMs: realtimeSegment.asr_duration_ms,
      quality: realtimeQuality,
    });

    browser = await chromium.launch({
      headless: !headed,
      args: ["--autoplay-policy=no-user-gesture-required"],
    });
    context = await browser.newContext({
      viewport: { width: 1707, height: 960 },
      recordVideo: { dir: outDir, size: { width: 1707, height: 960 } },
    });
    page = await context.newPage();
    page.on("console", (msg) => consoleLines.push(`${msg.type()}: ${msg.text()}`));
    page.on("pageerror", (error) => consoleLines.push(`pageerror: ${error.message}`));
    await page.addInitScript(() => {
      window.__coolzhuRealSpeech = { audioPlays: [], audioErrors: [] };
      const originalPlay = HTMLMediaElement.prototype.play;
      HTMLMediaElement.prototype.play = function patchedPlay(...args) {
        const record = {
          tag: this.tagName,
          src: this.currentSrc || this.src || "",
          startedAt: Date.now(),
          status: "called",
        };
        window.__coolzhuRealSpeech.audioPlays.push(record);
        return originalPlay.apply(this, args)
          .then((value) => {
            record.status = "resolved";
            return value;
          })
          .catch((error) => {
            record.status = "rejected";
            record.error = error?.message || String(error);
            window.__coolzhuRealSpeech.audioErrors.push(record);
            throw error;
          });
      };
    });

    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
    await page.locator('[data-role="message-input"]').waitFor({ timeout: 30000 });
    stage(summary, "page_loaded");

    const selectedAgent = await selectFirstConfiguredAgent(page);
    stage(summary, "front_end_agent_selected", { selectedAgent });

    const beforeTurn = {
      botCount: await page.locator(assistantReplySelector).count(),
      userCount: await page.locator(".message.user").count(),
    };
    const frontEndPrompt = `${sttProbe.text}\nPlease reply in one short English sentence confirming the real STT and TTS regression test.`;
    await page.evaluate((text) => {
      const input = document.querySelector('[data-role="message-input"]');
      input.value = text;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.focus();
    }, frontEndPrompt);
    await page.screenshot({ path: runningShot, fullPage: false });
    await page.locator('[data-action="send-message"]').click();
    stage(summary, "front_end_message_sent", { frontEndPrompt });

    const replyText = await waitForAssistantReplyAfterUserTurn(page, beforeTurn);
    if (!replyText) {
      throw new Error("Frontend did not render assistant reply text.");
    }
    const replyForSpeech = spokenReplyText(replyText);
    if (!replyForSpeech) {
      throw new Error(`Assistant reply had no speakable text after cleanup: ${JSON.stringify(replyText)}`);
    }
    stage(summary, "front_end_reply_received", {
      replyText: replyText.slice(0, 1000),
      replyForSpeech: replyForSpeech.slice(0, 700),
    });

    const replyTts = await page.evaluate(async (text) => {
      const response = await fetch("/api/audio/tts/speak", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ text, auto_play: false, segment: true }),
      });
      const result = await response.json();
      if (!response.ok) {
        throw new Error(result.error || "reply TTS request failed");
      }
      const url = Array.isArray(result.audio_urls) && result.audio_urls.length
        ? result.audio_urls[0]
        : result.audio_url;
      if (!url) {
        throw new Error("reply TTS returned no audio URL");
      }
      const audio = new Audio(url);
      await audio.play();
      return { result, url };
    }, replyForSpeech.slice(0, 700));
    const replyBytes = await audioBytesFromUrl(replyTts.url);
    fs.writeFileSync(replyTtsWav, replyBytes);
    const audioProbe = await page.evaluate(() => window.__coolzhuRealSpeech || {});
    if (!audioProbe.audioPlays?.some((item) => item.status === "resolved")) {
      throw new Error(`Browser did not resolve reply audio playback: ${JSON.stringify(audioProbe)}`);
    }
    stage(summary, "front_end_reply_tts_observed", {
      replyTtsUrl: replyTts.url,
      replyTtsDurationMs: replyTts.result.duration_ms,
      replyTtsBytes: replyBytes.length,
      audioProbe,
    });

    await page.screenshot({ path: finalShot, fullPage: false });
    summary.finishedAt = new Date().toISOString();
    summary.ok = true;
  } catch (error) {
    summary.ok = false;
    summary.error = error?.stack || error?.message || String(error);
    if (page) {
      try {
        await page.screenshot({ path: finalShot, fullPage: false });
      } catch (_) {}
    }
  } finally {
    if (page) {
      video = page.video();
    }
    fs.writeFileSync(resultJson, JSON.stringify(summary, null, 2), "utf8");
    if (context) {
      await context.close();
    }
    if (browser) {
      await browser.close();
    }
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
      sourceTtsWav,
      replyTtsWav,
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
