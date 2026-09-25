#!/usr/bin/env node
/*
 * Real speech interruption scenario for the realtime voice path.
 *
 * Scenario:
 *   1. User asks the selected model to briefly introduce itself.
 *   2. The assistant reply starts browser TTS playback.
 *   3. A second real TTS->STT utterance interrupts that playback.
 *   4. The frontend stops the current TTS through the existing barge-in handler.
 *   5. The selected model answers the new CoolZhu Agent feature request.
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

const repoRoot = findRepoRoot(__dirname);
const { chromium } = loadPlaywright(repoRoot);

const baseUrl = process.env.COOLZHU_E2E_BASE_URL || "http://127.0.0.1:8765";
const outDir = path.resolve(process.env.COOLZHU_E2E_OUT_DIR || path.join(repoRoot, "output", "playwright"));
const prefix = process.env.COOLZHU_E2E_PREFIX || "realtime-voice-interrupt-scenario";
const headed = process.env.COOLZHU_E2E_HEADLESS !== "1";
const timeoutMs = Number(process.env.COOLZHU_E2E_TIMEOUT_MS || 300000);
const preferredAgentId = process.env.COOLZHU_E2E_AGENT_ID || "";
const firstUtterance = process.env.COOLZHU_INTERRUPT_FIRST_TEXT || "Please briefly introduce yourself in two short English sentences.";
const interruptUtterance = process.env.COOLZHU_INTERRUPT_SECOND_TEXT || "Please introduce CoolZhu Agent features.";

fs.mkdirSync(outDir, { recursive: true });

const videoOut = path.join(outDir, `${prefix}.webm`);
const beforeInterruptShot = path.join(outDir, `${prefix}-before-interrupt.png`);
const finalShot = path.join(outDir, `${prefix}-final.png`);
const resultJson = path.join(outDir, `${prefix}.json`);
const firstPromptWav = path.join(outDir, `${prefix}-first-prompt.wav`);
const interruptPromptWav = path.join(outDir, `${prefix}-interrupt-prompt.wav`);
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

function countHits(text, expectedWords) {
  const words = normalizedWords(text);
  const hits = expectedWords.filter((word) => words.includes(word));
  return { words, hits };
}

function assertTranscriptLooksReal(transcript, expectedWords, minHits = 2) {
  const quality = countHits(transcript, expectedWords);
  if (quality.hits.length < Math.min(minHits, expectedWords.length)) {
    throw new Error(`STT transcript missed expected words. transcript=${JSON.stringify(transcript)} hits=${quality.hits.join(",")}`);
  }
  return quality;
}

function assertCoolzhuReply(text) {
  const lower = String(text || "").toLowerCase();
  const hasAgent = lower.includes("agent") || lower.includes("代理") || lower.includes("智能体");
  const featureTerms = [
    "feature",
    "function",
    "capabil",
    "vision",
    "voice",
    "computer-use",
    "desktop",
    "goal-mode",
    "automate",
    "workflow",
    "orchestrate",
    "multi-agent",
    "功能",
    "能力",
    "视觉",
    "语音",
    "桌面",
    "自动化",
    "工作流",
  ];
  const hasFeature = featureTerms.some((term) => lower.includes(term));
  const hasBrand = lower.includes("coolzhu") || lower.includes("cool zhu") || lower.includes("kuozu") || lower.includes("agent");
  if (!hasAgent || !hasFeature || !hasBrand) {
    throw new Error(`Final reply did not look like CoolZhu Agent feature answer: ${JSON.stringify(text)}`);
  }
  return { hasAgent, hasFeature, hasBrand, featureTermsMatched: featureTerms.filter((term) => lower.includes(term)) };
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
  const sessionId = start.session_id || `audio-rt-interrupt-${Date.now()}`;
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
}

async function stopRealtimeBackends() {
  await jsonFetch("/api/realtime/session/stop", { method: "POST" }).catch(() => null);
  await jsonFetch("/api/audio/realtime/stop", { method: "POST" }).catch(() => null);
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
    throw new Error("No configured chat agent target is available for voice interruption regression.");
  }
  return selected;
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

async function sendFrontendPrompt(page, text, stageName, summary) {
  const before = {
    botCount: await page.locator(assistantReplySelector).count(),
    userCount: await page.locator(".message.user").count(),
  };
  await page.evaluate((prompt) => {
    const input = document.querySelector('[data-role="message-input"]');
    input.value = prompt;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  }, text);
  await page.locator('[data-action="send-message"]').click();
  stage(summary, stageName, { text });
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
    .replace(/^\s*test\d+[^\n]*\n/i, "")
    .replace(/\n\d{1,2}:\d{2}:\d{2}\s*$/m, "")
    .trim();
}

async function startFrontendTtsPlayback(page, text) {
  await page.evaluate((speechText) => {
    window.__coolzhuVoiceInterrupt.firstPlayback = {
      status: "starting",
      startedAt: Date.now(),
      result: null,
      error: null,
    };
    window.ttsSpeakText(speechText, {
      source: "voice-interrupt-first-reply",
      segmented: true,
    })
      .then((result) => {
        window.__coolzhuVoiceInterrupt.firstPlayback.status = "resolved";
        window.__coolzhuVoiceInterrupt.firstPlayback.result = result;
      })
      .catch((error) => {
        window.__coolzhuVoiceInterrupt.firstPlayback.status = "rejected";
        window.__coolzhuVoiceInterrupt.firstPlayback.error = error?.message || String(error);
      });
  }, text);
  await page.waitForFunction(
    () => window.activeTtsPlaybackIsRunning && window.activeTtsPlaybackIsRunning(),
    null,
    { timeout: 120000 }
  );
}

async function dispatchRealSttInterrupt(page, interruptText, speechMs) {
  const response = await page.evaluate(async ({ text, duration }) => {
    const partialResponse = await fetch("/api/audio/realtime/partial", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        text,
        confidence: 0.82,
        is_final: true,
        speech_ms: Math.max(1800, Number(duration || 0)),
        tts_playing: true,
        echo_correlation: 0.03,
        vad_active: true,
        provider: "real_stt_interrupt",
        language: "en-US",
      }),
    });
    const partial = await partialResponse.json();
    if (!partialResponse.ok) {
      throw new Error(partial.error || "real STT interrupt partial failed");
    }
    const handled = window.handleRealtimeBargeInDecision(partial);
    return {
      partial,
      handled,
      activeAfterHandle: window.activeTtsPlaybackIsRunning(),
    };
  }, { text: interruptText, duration: speechMs });
  if (!response.partial?.should_interrupt || !response.handled) {
    throw new Error(`Barge-in did not interrupt playback: ${JSON.stringify(response)}`);
  }
  await page.waitForFunction(
    () => window.activeTtsPlaybackIsRunning && !window.activeTtsPlaybackIsRunning(),
    null,
    { timeout: 10000 }
  );
  return response;
}

async function main() {
  const consoleLines = [];
  const summary = {
    baseUrl,
    videoOut,
    beforeInterruptShot,
    finalShot,
    resultJson,
    firstPromptWav,
    interruptPromptWav,
    firstUtterance,
    interruptUtterance,
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

    const firstTts = await synthesizeSpeech(firstUtterance, firstPromptWav);
    const firstStt = await transcribeAudioPath(firstTts.result.audio_path, "real-interrupt-first-prompt");
    stage(summary, "first_prompt_stt_probe", {
      text: firstStt.text,
      durationMs: firstStt.duration_ms,
      quality: assertTranscriptLooksReal(firstStt.text, ["briefly", "introduce", "yourself", "sentences"], 2),
    });

    const interruptTts = await synthesizeSpeech(interruptUtterance, interruptPromptWav);
    const interruptStt = await transcribeAudioPath(interruptTts.result.audio_path, "real-interrupt-second-prompt");
    const interruptRealtime = await transcribeRealtimeSegment(interruptTts.bytes, interruptTts.result.duration_ms);
    const interruptText = String(interruptRealtime.asr_text || interruptStt.text || "").trim();
    stage(summary, "interrupt_stt_probe", {
      sttText: interruptStt.text,
      realtimeText: interruptRealtime.asr_text,
      realtimeProvider: interruptRealtime.asr_provider,
      durationMs: interruptRealtime.asr_duration_ms,
      quality: assertTranscriptLooksReal(interruptText, ["please", "introduce", "agent", "features"], 2),
    });

    await jsonFetch("/api/realtime/session/start", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        start_vision: false,
        start_audio: true,
        requested_mode: "half_duplex_guarded",
        auto_send_transcript: false,
        auto_tts_reply: false,
        barge_in_policy: {
          enabled: true,
          browser_echo_cancellation: true,
          noise_suppression: true,
          auto_gain_control: true,
        },
      }),
    });
    stage(summary, "realtime_session_started");

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
      window.__coolzhuVoiceInterrupt = { audioPlays: [], audioEvents: [], firstPlayback: null };
      const originalPlay = HTMLMediaElement.prototype.play;
      HTMLMediaElement.prototype.play = function patchedPlay(...args) {
        const record = {
          tag: this.tagName,
          src: this.currentSrc || this.src || "",
          startedAt: Date.now(),
          status: "called",
        };
        window.__coolzhuVoiceInterrupt.audioPlays.push(record);
        const mark = (status) => {
          record.status = status;
          record.updatedAt = Date.now();
          window.__coolzhuVoiceInterrupt.audioEvents.push({ status, src: record.src, at: Date.now() });
        };
        this.addEventListener("pause", () => mark("paused"), { once: true });
        this.addEventListener("ended", () => mark("ended"), { once: true });
        return originalPlay.apply(this, args)
          .then((value) => {
            mark("resolved");
            return value;
          })
          .catch((error) => {
            record.status = "rejected";
            record.error = error?.message || String(error);
            window.__coolzhuVoiceInterrupt.audioEvents.push({ status: "rejected", error: record.error, at: Date.now() });
            throw error;
          });
      };
    });

    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
    await page.locator('[data-role="message-input"]').waitFor({ timeout: 30000 });
    const selectedAgent = await selectFirstConfiguredAgent(page);
    stage(summary, "front_end_agent_selected", { selectedAgent });

    const firstPromptForModel = `${firstStt.text}\nPlease answer in English. Keep it concise so this voice response can be interrupted.`;
    const firstReply = await sendFrontendPrompt(page, firstPromptForModel, "first_prompt_sent", summary);
    const firstReplyForSpeech = spokenReplyText(firstReply);
    if (!firstReplyForSpeech) {
      throw new Error(`First assistant reply had no speakable text: ${JSON.stringify(firstReply)}`);
    }
    stage(summary, "first_reply_received", {
      replyText: firstReply.slice(0, 1000),
      replyForSpeech: firstReplyForSpeech.slice(0, 700),
    });

    await startFrontendTtsPlayback(page, firstReplyForSpeech.slice(0, 700));
    stage(summary, "first_reply_tts_started", {
      active: await page.evaluate(() => window.activeTtsPlaybackIsRunning()),
    });
    await page.screenshot({ path: beforeInterruptShot, fullPage: false });

    const barge = await dispatchRealSttInterrupt(
      page,
      interruptText,
      Math.max(interruptTts.result.duration_ms || 0, interruptStt.duration_ms || 0, interruptRealtime.asr_duration_ms || 0)
    );
    stage(summary, "barge_in_confirmed", barge);

    const interruptPromptForModel = `${interruptText}\nThis was a real STT interruption. Treat KUOzu/Cool Zhu/CoolZhu as CoolZhu Agent and introduce CoolZhu Agent features in two short English sentences.`;
    const coolzhuReply = await sendFrontendPrompt(page, interruptPromptForModel, "interrupt_message_sent", summary);
    const coolzhuReplyForSpeech = spokenReplyText(coolzhuReply);
    const coolzhuQuality = assertCoolzhuReply(coolzhuReplyForSpeech);
    stage(summary, "coolzhu_reply_received", {
      replyText: coolzhuReply.slice(0, 1000),
      replyForSpeech: coolzhuReplyForSpeech.slice(0, 700),
      quality: coolzhuQuality,
    });

    const audioProbe = await page.evaluate(() => window.__coolzhuVoiceInterrupt || {});
    const stoppedOrPaused = audioProbe.audioEvents?.some((event) => event.status === "paused" || event.status === "ended");
    const rejectedByPause = audioProbe.audioPlays?.some((event) => (
      event.status === "rejected" && /interrupted|pause/i.test(String(event.error || ""))
    )) || /interrupted|pause/i.test(String(audioProbe.firstPlayback?.error || ""));
    const activeClearedByHandler = barge.activeAfterHandle === false;
    if (!stoppedOrPaused && !rejectedByPause && !activeClearedByHandler) {
      throw new Error(`No frontend audio pause/end event observed after interruption: ${JSON.stringify(audioProbe)}`);
    }
    stage(summary, "front_end_interrupt_audio_observed", { audioProbe, stoppedOrPaused, rejectedByPause, activeClearedByHandler });

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
    await stopRealtimeBackends();
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
      beforeInterruptShot,
      finalShot,
      resultJson,
      firstPromptWav,
      interruptPromptWav,
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
