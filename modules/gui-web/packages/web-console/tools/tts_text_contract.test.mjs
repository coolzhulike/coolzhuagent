import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile(new URL("../src/app.js", import.meta.url), "utf8");

function extractFunction(sourceText, name) {
  const marker = `function ${name}`;
  const start = sourceText.indexOf(marker);
  assert.notEqual(start, -1, `${name} should be defined`);
  const paramsStart = sourceText.indexOf("(", start);
  assert.notEqual(paramsStart, -1, `${name} should have parameters`);
  let paramsDepth = 0;
  let paramsEnd = -1;
  for (let index = paramsStart; index < sourceText.length; index += 1) {
    if (sourceText[index] === "(") paramsDepth += 1;
    if (sourceText[index] === ")") paramsDepth -= 1;
    if (paramsDepth === 0) {
      paramsEnd = index;
      break;
    }
  }
  const braceStart = sourceText.indexOf("{", paramsEnd);
  assert.notEqual(braceStart, -1, `${name} should have a function body`);
  let depth = 0;
  for (let index = braceStart; index < sourceText.length; index += 1) {
    const char = sourceText[index];
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth === 0) {
      return sourceText.slice(start, index + 1);
    }
  }
  assert.fail(`${name} body should be balanced`);
}

const splitSource = extractFunction(source, "splitAssistantSpeechTextForTts");
const sanitizeSource = extractFunction(source, "sanitizeAssistantMessageTextForTts");
const { splitAssistantSpeechTextForTts, sanitizeAssistantMessageTextForTts } = new Function(
  `${splitSource}; ${sanitizeSource}; return {
    splitAssistantSpeechTextForTts,
    sanitizeAssistantMessageTextForTts,
  };`,
)();

const replyWithDiagnostics = [
  "库珠是我——一个融合武侠美学与现代工程效率的 AI Agent，专注、沉浸、可持续地陪你把每个想法落地成真。",
  "",
  "---",
  "Remote context usage: 7.0% (69930/1000000 input tokens; source=remote). Local estimate: 56332 tokens; local prompt budget: 56332/848976 tokens; history truncated: false.",
].join("\n");

assert.equal(
  sanitizeAssistantMessageTextForTts(replyWithDiagnostics),
  "库珠是我——一个融合武侠美学与现代工程效率的 AI Agent，专注、沉浸、可持续地陪你把每个想法落地成真。",
);

assert.equal(
  sanitizeAssistantMessageTextForTts("正文\nRemote context usage: 1.0%"),
  "正文",
);

assert.equal(
  sanitizeAssistantMessageTextForTts("正文\n---\nContext usage: 2.0%"),
  "正文",
);

assert.deepEqual(
  splitAssistantSpeechTextForTts("第一句。\n第二句。\nRemote context usage: 3.0%"),
  { text: "第一句。\n第二句。", diagnosticsStarted: true },
);

assert.deepEqual(
  splitAssistantSpeechTextForTts("第一句。\nRemote cont"),
  { text: "第一句。\nRemote cont", diagnosticsStarted: false },
);

const segmentSource = extractFunction(source, "takeRealtimeSpeechSegments");
const takeRealtimeSpeechSegments = new Function(
  `${segmentSource}; return takeRealtimeSpeechSegments;`,
)();
const heldSeparator = takeRealtimeSpeechSegments("第一句。\n---\n");
assert.deepEqual(heldSeparator, { segments: ["第一句。"], rest: "\n---\n" });
assert.deepEqual(
  splitAssistantSpeechTextForTts(heldSeparator.rest + "Remote context usage: 4.0%"),
  { text: "", diagnosticsStarted: true },
);

assert.equal(
  sanitizeAssistantMessageTextForTts("  只有正文  "),
  "只有正文",
);

const pcmEligibilitySource = extractFunction(source, "realtimeProviderNativePcmEligible");
const realtimeProviderNativePcmEligible = new Function(
  `${pcmEligibilitySource}; return realtimeProviderNativePcmEligible;`,
)();
assert.equal(realtimeProviderNativePcmEligible({
  running: true,
  active_mode: "full_streaming",
}), true);
assert.equal(realtimeProviderNativePcmEligible({
  running: true,
  requested_mode: "full_streaming",
  active_mode: "half_duplex_guarded",
  streaming_risk: {
    stt_transport: "segmented_mediarecorder",
    readiness_gates: [{ id: "provider_native_partial_asr", ready: false }],
  },
}), false);
assert.equal(realtimeProviderNativePcmEligible({
  running: true,
  active_mode: "half_duplex_guarded",
  streaming_risk: {
    stt_transport: "provider_native_streaming_asr",
    readiness_gates: [{ id: "provider_native_partial_asr", ready: true }],
  },
}), true);
assert.equal(realtimeProviderNativePcmEligible({
  running: false,
  active_mode: "full_streaming",
}), false);

const queuedSpeechSource = extractFunction(source, "queueRealtimeSpeechSegment");
const drainSpeechSource = extractFunction(source, "drainRealtimeTtsChunkQueue");
const invalidateSource = extractFunction(source, "invalidateRealtimeGeneration");
assert.equal(queuedSpeechSource.includes("streamedMessageIds.add"), false);
assert.equal(drainSpeechSource.includes("streamedMessageIds.add"), true);
assert.equal(invalidateSource.includes("streamingCancelledMessageIds.clear"), false);

const providerEventSource = extractFunction(source, "handleRealtimeVoiceStreamEvent");
assert.equal(providerEventSource.includes("reportAudioRealtimePartial("), false);
assert.equal(providerEventSource.includes("submitRealtimeFinalTranscript("), true);

const audioStartSource = extractFunction(source, "audioRealtimeStart");
assert.equal(audioStartSource.includes("realtimeProviderNativePcmEligible"), true);
assert.equal(audioStartSource.includes("rollbackRealtimeAudioStart"), true);

const indexSource = await readFile(new URL("../index.html", import.meta.url), "utf8");
assert.match(indexSource, /data-role="audio-input-device"/);
assert.match(indexSource, /data-role="audio-output-device"/);
assert.match(source, /localStorage\.setItem\("audioInputDeviceId"/);
assert.match(source, /localStorage\.setItem\("audioOutputDeviceId"/);

console.log("tts text contracts: PASS");
