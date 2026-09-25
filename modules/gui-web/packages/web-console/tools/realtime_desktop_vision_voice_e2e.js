#!/usr/bin/env node
/*
 * Real desktop vision + voice acceptance scenario.
 *
 * Scenario:
 *   1. Capture the current desktop through the backend capture API.
 *   2. Upload that PNG into the normal attachment store.
 *   3. Select a vision-capable configured chat session.
 *   4. Send "说一下当前桌面上你看到有哪些东西" with the screenshot attachment
 *      through the real chat stream endpoint and render it in the frontend.
 *   5. Play the final model answer through the real browser TTS path.
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
const prefix = process.env.COOLZHU_E2E_PREFIX || "realtime-desktop-vision-voice";
const headed = process.env.COOLZHU_E2E_HEADLESS !== "1";
const timeoutMs = Number(process.env.COOLZHU_E2E_TIMEOUT_MS || 360000);
const desktopVisionPrompt = process.env.COOLZHU_DESKTOP_VISION_PROMPT
  || "说一下当前桌面上你看到有哪些东西。请基于附件截图用中文简短回答，提到主要窗口、卡片、按钮或文字。CoolZhu 播报时读作“酷猪”。";

fs.mkdirSync(outDir, { recursive: true });

const videoOut = path.join(outDir, `${prefix}.webm`);
const finalShot = path.join(outDir, `${prefix}-final.png`);
const captureOut = path.join(outDir, `${prefix}-desktop-capture.png`);
const resultJson = path.join(outDir, `${prefix}.json`);
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

async function bytesFetch(url) {
  const response = await fetch(resolveUrl(url));
  if (!response.ok) {
    throw new Error(`${url} fetch failed with ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

function base64Data(bytes) {
  return Buffer.from(bytes).toString("base64");
}

function coolzhuPronunciationText(text) {
  return String(text || "")
    .replace(/Cool\s*Zhu/gi, "酷猪")
    .replace(/CoolZhu/gi, "酷猪")
    .replace(/KUOzu/gi, "酷猪")
    .replace(/coolzhu/gi, "酷猪");
}

function normalizeText(text) {
  return String(text || "").toLowerCase();
}

function assertDesktopDescription(text) {
  const value = normalizeText(text);
  const desktopTerms = [
    "桌面",
    "屏幕",
    "窗口",
    "界面",
    "聊天",
    "任务",
    "卡片",
    "按钮",
    "浏览器",
    "终端",
    "agent",
    "coolzhu",
    "酷猪",
  ];
  const refusalTerms = [
    "无法查看",
    "看不到",
    "不能查看",
    "没有图片",
    "无法访问附件",
    "i cannot see",
    "can't see",
  ];
  const hits = desktopTerms.filter((term) => value.includes(term.toLowerCase()));
  const refusals = refusalTerms.filter((term) => value.includes(term.toLowerCase()));
  if (refusals.length || hits.length < 2 || String(text || "").trim().length < 12) {
    throw new Error(`Desktop vision reply did not look grounded in the screenshot. hits=${hits.join(",")} refusals=${refusals.join(",")} text=${JSON.stringify(text)}`);
  }
  return { hits, refusals };
}

function spokenReplyText(text) {
  return String(text || "")
    .replace(/\n---\nRemote context usage:[\s\S]*$/m, "")
    .replace(/^\s*test\d+[^\n]*\n/i, "")
    .replace(/\n\d{1,2}:\d{2}:\d{2}\s*$/m, "")
    .trim();
}

async function captureDesktop(summary) {
  const metadata = await jsonFetch("/api/capture", { method: "POST" });
  const bytes = await bytesFetch("/api/capture/latest/image");
  if (bytes.length < 1024) {
    throw new Error(`Desktop capture image is unexpectedly small: ${bytes.length} bytes`);
  }
  fs.writeFileSync(captureOut, bytes);
  stage(summary, "desktop_capture_created", {
    metadata,
    captureOut,
    bytes: bytes.length,
  });
  return { metadata, bytes };
}

async function uploadDesktopCapture(bytes, summary) {
  const upload = await jsonFetch("/api/pet/drop-upload", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      files: [{
        name: `${prefix}-desktop-capture.png`,
        mime_type: "image/png",
        content_base64: base64Data(bytes),
      }],
    }),
  });
  const attachment = upload.attachments?.find((item) => item.kind === "image") || upload.attachments?.[0];
  if (!attachment?.url || attachment.kind !== "image") {
    throw new Error(`Desktop capture upload did not return an image attachment: ${JSON.stringify(upload)}`);
  }
  stage(summary, "desktop_capture_uploaded", {
    attachment,
    moved: upload.moved,
    skipped: upload.skipped,
  });
  return attachment;
}

function isVisionCapableAgent(agent) {
  const text = [
    agent?.id,
    agent?.name,
    agent?.display_name,
    agent?.model,
    agent?.model_type,
    agent?.provider,
  ].filter(Boolean).join(" ").toLowerCase();
  return Boolean(agent?.selectable && agent?.enabled)
    && (
      ["vision", "multimodal", "video"].includes(String(agent.model_type || "").toLowerCase())
      || /视觉|vision|vl|glm-4\.6v|qwen.*vl|gpt-4o|omni|multimodal/.test(text)
    );
}

async function selectVisionCapableAgent(page) {
  await page.waitForFunction(
    () => document.querySelectorAll('[data-role="agent-targets"] input[type="checkbox"]').length > 0,
    null,
    { timeout: 30000 }
  );
  const registry = await jsonFetch("/api/agents");
  const candidates = (registry.agents || []).filter(isVisionCapableAgent);
  if (!candidates.length) {
    throw new Error(`No enabled/selectable vision-capable agent is configured. agents=${JSON.stringify((registry.agents || []).map((agent) => ({
      id: agent.id,
      name: agent.name,
      display_name: agent.display_name,
      model: agent.model,
      model_type: agent.model_type,
      selectable: agent.selectable,
      enabled: agent.enabled,
    })))}`);
  }
  const selectedAgent = candidates[0];
  const selected = await page.evaluate((agentId) => {
    const setter = (typeof setSingleAgentTarget === "function")
      ? setSingleAgentTarget
      : window.setSingleAgentTarget;
    if (typeof setter === "function") {
      setter(agentId);
    } else {
      const host = document.querySelector('[data-role="agent-targets"]');
      const boxes = Array.from(host?.querySelectorAll('input[type="checkbox"]') || []);
      boxes.forEach((box) => {
        box.checked = box.value === agentId;
        box.dispatchEvent(new Event("change", { bubbles: true }));
      });
    }
    const target = Array.from(document.querySelectorAll('[data-role="agent-targets"] input[type="checkbox"]'))
      .find((box) => box.value === agentId);
    return {
      value: target?.value || "",
      checked: Boolean(target?.checked),
      label: target?.closest("label")?.innerText?.trim() || target?.value || "",
      triggerText: document.querySelector('[data-role="agent-trigger"]')?.innerText?.trim() || "",
      hasSetSingleAgentTarget: typeof setter === "function",
    };
  }, selectedAgent.id);
  if (!selected.checked) {
    throw new Error(`Failed to select vision-capable agent ${selectedAgent.id}: ${JSON.stringify(selected)}`);
  }
  return { selectedAgent, selected, candidates };
}

async function frontendSessionState(page) {
  return await page.evaluate(async () => {
    const sessionsResponse = await fetch("/api/sessions");
    const sessions = await sessionsResponse.json();
    const roomsResponse = await fetch("/api/chat/rooms");
    const rooms = await roomsResponse.json();
    return {
      activeSessionId: (typeof activeSessionId !== "undefined" && activeSessionId)
        ? activeSessionId
        : sessions.active_session_id || sessions.sessions?.[0]?.id || null,
      activeChatRoomId: (typeof activeChatRoomId !== "undefined" && activeChatRoomId)
        ? activeChatRoomId
        : rooms.active_room_id || rooms.rooms?.[0]?.id || "main-room",
    };
  });
}

async function sendDesktopVisionPrompt(page, payload) {
  return await page.evaluate(async (requestPayload) => {
    const parseFrame = (typeof parseSseFrame === "function") ? parseSseFrame : window.parseSseFrame;
    const handleEvent = (typeof handleChatStreamEvent === "function") ? handleChatStreamEvent : window.handleChatStreamEvent;
    if (typeof parseFrame !== "function" || typeof handleEvent !== "function") {
      throw new Error("Frontend stream parser/handler is not available.");
    }
    const response = await fetch("/api/chat/send/stream", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestPayload),
    });
    if (!response.ok || !response.body) {
      const text = await response.text();
      throw new Error(`/api/chat/send/stream failed with ${response.status}: ${text}`);
    }
    const decoder = new TextDecoder();
    const reader = response.body.getReader();
    let buffer = "";
    let donePayload = null;
    let eventCount = 0;
    while (true) {
      const { value, done } = await reader.read();
      if (done) {
        break;
      }
      buffer += decoder.decode(value, { stream: true });
      const frames = buffer.split(/\r?\n\r?\n/);
      buffer = frames.pop() ?? "";
      for (const frame of frames) {
        const event = parseFrame(frame);
        if (!event) {
          continue;
        }
        eventCount += 1;
        const result = handleEvent(event);
        if (event.event === "done") {
          donePayload = result;
        }
      }
    }
    buffer += decoder.decode();
    if (buffer.trim()) {
      const event = parseFrame(buffer);
      if (event) {
        eventCount += 1;
        const result = handleEvent(event);
        if (event.event === "done") {
          donePayload = result;
        }
      }
    }
    if (donePayload?.tasks && typeof renderTaskList === "function") {
      renderTaskList(donePayload.tasks);
    }
    if (typeof refreshGoals === "function") {
      await refreshGoals();
    }
    if (typeof autoStartReadyGoalLoops === "function") {
      await autoStartReadyGoalLoops();
    }
    if (typeof refreshActiveBeads === "function") {
      await refreshActiveBeads();
    }
    if (typeof refreshChatCollaboration === "function") {
      await refreshChatCollaboration(requestPayload.chat_room_id);
    }
    return { donePayload, eventCount };
  }, payload);
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

async function speakDesktopDescription(page, text) {
  return await page.evaluate(async (speechText) => {
    const speak = (typeof ttsSpeakText === "function") ? ttsSpeakText : window.ttsSpeakText;
    window.__coolzhuDesktopVisionVoice.playback = {
      status: "starting",
      text: speechText,
      startedAt: Date.now(),
      result: null,
      error: null,
    };
    try {
      let result = null;
      if (typeof speak === "function") {
        result = await speak(speechText, {
          source: "desktop-vision-voice",
          segmented: true,
        });
      } else {
        const response = await fetch("/api/audio/tts/speak", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ text: speechText, auto_play: false, segment: true }),
        });
        result = await response.json();
        if (!response.ok) {
          throw new Error(result.error || "desktop description TTS request failed");
        }
        const url = Array.isArray(result.audio_urls) && result.audio_urls.length
          ? result.audio_urls[0]
          : result.audio_url;
        if (!url) {
          throw new Error("desktop description TTS returned no audio URL");
        }
        const audio = new Audio(url);
        await audio.play();
      }
      window.__coolzhuDesktopVisionVoice.playback.status = "resolved";
      window.__coolzhuDesktopVisionVoice.playback.result = result;
      return { result, audioProbe: window.__coolzhuDesktopVisionVoice };
    } catch (error) {
      window.__coolzhuDesktopVisionVoice.playback.status = "rejected";
      window.__coolzhuDesktopVisionVoice.playback.error = error?.message || String(error);
      throw error;
    }
  }, coolzhuPronunciationText(text).slice(0, 900));
}

async function main() {
  const consoleLines = [];
  const summary = {
    baseUrl,
    videoOut,
    finalShot,
    captureOut,
    resultJson,
    desktopVisionPrompt,
    startedAt: new Date().toISOString(),
    stages: [],
    consoleLines,
    chainStatus: {},
  };

  let browser = null;
  let context = null;
  let page = null;
  let video = null;

  try {
    const audioStatus = await jsonFetch("/api/audio/status");
    if (!audioStatus.tts_available) {
      throw new Error(`Real TTS unavailable: ${JSON.stringify(audioStatus)}`);
    }
    summary.chainStatus.stt_backend_available = Boolean(audioStatus.stt_available);
    summary.chainStatus.tts_backend_available = Boolean(audioStatus.tts_available);
    summary.chainStatus.physical_microphone_verified = false;
    summary.chainStatus.physical_microphone_note = "This scenario verifies voice playback; physical microphone capture remains hardware-dependent.";
    stage(summary, "audio_status_ready", { audioStatus });

    const capture = await captureDesktop(summary);
    const attachment = await uploadDesktopCapture(capture.bytes, summary);

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
      window.__coolzhuDesktopVisionVoice = { audioPlays: [], audioErrors: [], playback: null };
      const originalPlay = HTMLMediaElement.prototype.play;
      HTMLMediaElement.prototype.play = function patchedPlay(...args) {
        const record = {
          tag: this.tagName,
          src: this.currentSrc || this.src || "",
          startedAt: Date.now(),
          status: "called",
        };
        window.__coolzhuDesktopVisionVoice.audioPlays.push(record);
        return originalPlay.apply(this, args)
          .then((value) => {
            record.status = "resolved";
            record.updatedAt = Date.now();
            return value;
          })
          .catch((error) => {
            record.status = "rejected";
            record.error = error?.message || String(error);
            record.updatedAt = Date.now();
            window.__coolzhuDesktopVisionVoice.audioErrors.push(record);
            throw error;
          });
      };
    });

    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
    await page.locator('[data-role="message-input"]').waitFor({ timeout: 30000 });
    stage(summary, "page_loaded");

    const selection = await selectVisionCapableAgent(page);
    stage(summary, "front_end_vision_agent_selected", {
      selectedAgent: selection.selectedAgent,
      selected: selection.selected,
      candidateCount: selection.candidates.length,
    });

    const state = await frontendSessionState(page);
    const beforeBotCount = await page.locator(assistantReplySelector).count();
    const payload = {
      session_id: state.activeSessionId,
      chat_room_id: state.activeChatRoomId,
      target_agent_ids: [selection.selectedAgent.id],
      text: desktopVisionPrompt,
      selected_message_ids: [],
      attachments: [attachment],
    };
    const streamResult = await sendDesktopVisionPrompt(page, payload);
    stage(summary, "desktop_vision_prompt_sent", {
      sessionId: state.activeSessionId,
      chatRoomId: state.activeChatRoomId,
      targetAgentId: selection.selectedAgent.id,
      attachment,
      streamResult,
    });

    const replyText = await waitForAssistantReply(page, beforeBotCount);
    const replyForSpeech = spokenReplyText(replyText);
    if (!replyForSpeech) {
      throw new Error(`Desktop vision reply had no speakable text: ${JSON.stringify(replyText)}`);
    }
    const quality = assertDesktopDescription(replyForSpeech);
    stage(summary, "desktop_description_received", {
      replyText: replyText.slice(0, 1500),
      replyForSpeech: replyForSpeech.slice(0, 900),
      quality,
    });

    const tts = await speakDesktopDescription(page, replyForSpeech);
    const audioProbe = await page.evaluate(() => window.__coolzhuDesktopVisionVoice || {});
    const playbackResolved = audioProbe.audioPlays?.some((item) => item.status === "resolved")
      || audioProbe.playback?.status === "resolved";
    if (!playbackResolved) {
      throw new Error(`Desktop description TTS playback was not observed: ${JSON.stringify(audioProbe)}`);
    }
    stage(summary, "desktop_description_tts_observed", {
      tts,
      audioProbe,
      spokenText: coolzhuPronunciationText(replyForSpeech).slice(0, 900),
    });

    await page.screenshot({ path: finalShot, fullPage: false });
    summary.chainStatus.desktop_capture = "ok";
    summary.chainStatus.desktop_attachment_upload = "ok";
    summary.chainStatus.vision_model_reply = "ok";
    summary.chainStatus.tts_playback = "ok";
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
      finalShot,
      captureOut,
      resultJson,
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
