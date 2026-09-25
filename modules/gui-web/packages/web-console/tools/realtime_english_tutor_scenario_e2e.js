#!/usr/bin/env node
/*
 * Real speech English tutor acceptance scenario.
 *
 * It validates the software path without fake audio gates:
 *   1. TTS synthesizes a learner request.
 *   2. STT transcribes the synthesized audio.
 *   3. The frontend sends the transcript to a text model session.
 *   4. The model asks an English tutoring question.
 *   5. TTS plays the teacher response.
 *   6. A second real TTS->STT learner answer is sent for correction.
 *   7. The final correction is synthesized and played.
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
  throw new Error(`Playwright is not available:\n${errors.join("\n")}`);
}

const repoRoot = findRepoRoot(__dirname);
const { chromium } = loadPlaywright(repoRoot);

const baseUrl = process.env.COOLZHU_E2E_BASE_URL || "http://127.0.0.1:8765";
const outDir = path.resolve(process.env.COOLZHU_E2E_OUT_DIR || path.join(repoRoot, "output", "playwright"));
const prefix = process.env.COOLZHU_E2E_PREFIX || "realtime-english-tutor-scenario";
const preferredAgentId = process.env.COOLZHU_E2E_AGENT_ID || "";
const headed = process.env.COOLZHU_E2E_HEADLESS !== "1";
const timeoutMs = Number(process.env.COOLZHU_E2E_TIMEOUT_MS || 300000);
const tutorRequest = process.env.COOLZHU_TUTOR_REQUEST
  || "Please be my English teacher. Ask me one easy question about daily life.";
const learnerAnswer = process.env.COOLZHU_TUTOR_ANSWER || "I go to school yesterday.";

fs.mkdirSync(outDir, { recursive: true });

const videoOut = path.join(outDir, `${prefix}.webm`);
const firstShot = path.join(outDir, `${prefix}-teacher-question.png`);
const finalShot = path.join(outDir, `${prefix}-final.png`);
const resultJson = path.join(outDir, `${prefix}.json`);
const requestWav = path.join(outDir, `${prefix}-request.wav`);
const answerWav = path.join(outDir, `${prefix}-answer.wav`);
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

async function synthesizeSpeech(text, outputPath) {
  const result = await jsonFetch("/api/audio/tts/speak", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      text,
      source: "english-tutor-e2e",
      output_path: outputPath,
      auto_play: false,
      segment: false,
    }),
  });
  if (!result.audio_path || !result.audio_url) {
    throw new Error(`TTS did not return an audio file: ${JSON.stringify(result)}`);
  }
  return result;
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

async function selectTextAgent(page) {
  await page.waitForFunction(
    () => document.querySelectorAll('[data-role="agent-targets"] input[type="checkbox"]').length > 0,
    null,
    { timeout: 30000 },
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
    const current = boxes.find((box) => box.checked && !box.disabled && textAgentIds.includes(String(box.value || "")));
    const textTarget = boxes.find((box) => !box.disabled && textAgentIds.includes(String(box.value || "")));
    const target = preferredTarget || current || textTarget || boxes.find((box) => !box.disabled);
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
    throw new Error("No text agent target is available for English tutor regression.");
  }
  return selected;
}

async function waitForAssistantReply(page, initialCount) {
  await page.waitForFunction(
    ({ count, selector }) => document.querySelectorAll(selector).length > count,
    { count: initialCount, selector: assistantReplySelector },
    { timeout: timeoutMs },
  );
  await page.waitForTimeout(1200);
  return await page.evaluate((selector) => {
    const messages = Array.from(document.querySelectorAll(selector));
    const last = messages[messages.length - 1];
    return last ? last.innerText.trim() : "";
  }, assistantReplySelector);
}

async function sendPrompt(page, text) {
  const before = await page.locator(assistantReplySelector).count();
  await page.evaluate((prompt) => {
    const input = document.querySelector('[data-role="message-input"]');
    input.value = prompt;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  }, text);
  await page.locator('[data-action="send-message"]').click();
  return await waitForAssistantReply(page, before);
}

function spokenReplyText(text) {
  return String(text || "")
    .replace(/\n---\nRemote context usage:[\s\S]*$/m, "")
    .replace(/\n\d{1,2}:\d{2}:\d{2}\s*$/m, "")
    .trim();
}

function ttsSafeEnglish(text) {
  const cleaned = spokenReplyText(text)
    .split(/\r?\n/)
    .filter((line) => !/^\s*(test\d+|session-|agnes-|glm|qwen|remote context usage)/i.test(line))
    .join(" ")
    .replace(/[^\x20-\x7E]+/g, " ")
    .replace(/\bI'd\b/gi, "I would")
    .replace(/\bI'm\b/gi, "I am")
    .replace(/\bdon't\b/gi, "do not")
    .replace(/\bcan't\b/gi, "cannot")
    .replace(/\bhere's\b/gi, "Here is")
    .replace(/["'`:]/g, "")
    .replace(/\s+-\s+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  const sentences = cleaned
    .split(/(?<=[.!?])\s+/)
    .filter((sentence) => /[a-z]/i.test(sentence));
  return (sentences.join(" ") || cleaned || "English tutoring response is ready.").slice(0, 420);
}

function assertTeacherQuestion(text) {
  const value = String(text || "").toLowerCase();
  if (!value.includes("?") && !value.includes("question") && !value.includes("ask")) {
    throw new Error(`Teacher reply did not ask a tutoring question: ${JSON.stringify(text)}`);
  }
}

function assertCorrection(text) {
  const value = String(text || "").toLowerCase();
  const hasCorrection = value.includes("went") || value.includes("correct") || value.includes("grammar") || value.includes("should be");
  const mentionsOriginalTime = value.includes("yesterday") || value.includes("past");
  if (!hasCorrection || !mentionsOriginalTime) {
    throw new Error(`Tutor correction did not look grounded in the learner answer: ${JSON.stringify(text)}`);
  }
}

async function playTtsInBrowser(page, text, source) {
  const speakable = ttsSafeEnglish(text);
  const tts = await jsonFetch("/api/audio/tts/speak", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text: speakable, source, auto_play: false, segment: false }),
  });
  await page.evaluate(async (audioUrl) => {
    const audio = new Audio(audioUrl);
    await audio.play();
    await new Promise((resolve) => {
      audio.addEventListener("ended", resolve, { once: true });
      window.setTimeout(resolve, 12000);
    });
    audio.pause();
  }, tts.audio_url);
  return { ...tts, speakable };
}

async function main() {
  const summary = {
    baseUrl,
    videoOut,
    firstShot,
    finalShot,
    resultJson,
    requestWav,
    answerWav,
    tutorRequest,
    learnerAnswer,
    startedAt: new Date().toISOString(),
    stages: [],
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

    const requestTts = await synthesizeSpeech(tutorRequest, requestWav);
    const requestStt = await transcribeAudioPath(requestTts.audio_path, "english-tutor-request");
    stage(summary, "learner_request_stt", { text: requestStt.text, durationMs: requestStt.duration_ms });

    browser = await chromium.launch({ headless: !headed, args: ["--autoplay-policy=no-user-gesture-required"] });
    context = await browser.newContext({
      viewport: { width: 1707, height: 960 },
      recordVideo: { dir: outDir, size: { width: 1707, height: 960 } },
    });
    page = await context.newPage();
    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
    await page.locator('[data-role="message-input"]').waitFor({ timeout: 30000 });
    const selectedAgent = await selectTextAgent(page);
    stage(summary, "front_end_agent_selected", { selectedAgent });

    const questionPrompt = `${requestStt.text}\nAct as a friendly English tutor. Reply only in plain English. Ask exactly one simple question. Do not include markdown, Chinese, model names, or context notes.`;
    const teacherReply = await sendPrompt(page, questionPrompt);
    const teacherSpeech = spokenReplyText(teacherReply);
    assertTeacherQuestion(teacherSpeech);
    stage(summary, "teacher_question_received", {
      replyText: teacherReply.slice(0, 1000),
      speechText: teacherSpeech.slice(0, 600),
    });
    const teacherTts = await playTtsInBrowser(page, teacherSpeech.slice(0, 600), "english-tutor-question");
    stage(summary, "teacher_question_received_and_played", {
      replyText: teacherReply.slice(0, 1000),
      speechText: teacherSpeech.slice(0, 600),
      speakableText: teacherTts.speakable,
      audioUrl: teacherTts.audio_url,
    });
    await page.screenshot({ path: firstShot, fullPage: false });

    const answerTts = await synthesizeSpeech(learnerAnswer, answerWav);
    const answerStt = await transcribeAudioPath(answerTts.audio_path, "english-tutor-answer");
    stage(summary, "learner_answer_stt", { text: answerStt.text, durationMs: answerStt.duration_ms });

    const correctionPrompt = `Student answer: "${answerStt.text}". Reply only in plain English. Correct the grammar briefly, explain why, and give one improved sentence. Do not include markdown, Chinese, model names, or context notes.`;
    const correctionReply = await sendPrompt(page, correctionPrompt);
    const correctionSpeech = spokenReplyText(correctionReply);
    assertCorrection(correctionSpeech);
    const correctionTts = await playTtsInBrowser(page, correctionSpeech.slice(0, 700), "english-tutor-correction");
    stage(summary, "correction_received_and_played", {
      replyText: correctionReply.slice(0, 1000),
      speechText: correctionSpeech.slice(0, 700),
      speakableText: correctionTts.speakable,
      audioUrl: correctionTts.audio_url,
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
      await context.close().catch(() => null);
    }
    if (browser) {
      await browser.close().catch(() => null);
    }
    if (video) {
      const saved = await video.path().catch(() => null);
      if (saved && saved !== videoOut) {
        fs.copyFileSync(saved, videoOut);
      }
    }
    console.log(JSON.stringify({
      ok: summary.ok,
      error: summary.error || null,
      videoOut,
      firstShot,
      finalShot,
      resultJson,
      requestWav,
      answerWav,
    }, null, 2));
    if (!summary.ok) {
      process.exitCode = 1;
    }
  }
}

main();
