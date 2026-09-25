# Realtime Full Stream Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make realtime voice operate as a correlated ASR → model delta → streaming TTS pipeline with safe barge-in and explicit half-duplex fallback.

**Architecture:** Keep the current Web Console SSE, audio endpoints, and TTS chunk bridge. Add one browser-side turn controller that owns correlation, incremental text commitment, TTS request ordering, and cancellation; extend backend realtime evidence so readiness represents one active session/turn rather than unrelated historical probes.

**Tech Stack:** Rust/Axum, vanilla JavaScript, browser MediaRecorder/SpeechRecognition/WebAudio, SSE, Cargo tests, Node syntax checks, in-app Browser verification.

---

### Task 1: Preserve the current dirty module and record the baseline

**Files:**
- Backup: `tmp/backups/<timestamp>-full-stream-pre/modules-gui-web/`
- Create: `tmp/backups/<timestamp>-full-stream-pre/manifest.json`

- [ ] **Step 1: Record the current module status and hashes**

Run:

```powershell
git -C modules/gui-web status --short
Get-FileHash modules/gui-web/packages/web-console/src/app.js -Algorithm SHA256
Get-FileHash modules/gui-web/packages/web-console/src/main.rs -Algorithm SHA256
```

Expected: the existing `main.rs` modification is visible and both hashes are recorded before editing.

- [ ] **Step 2: Back up the complete Web Console source directory**

Copy `modules/gui-web/packages/web-console/src`, `index.html`, and the module Cargo manifests into a timestamped directory under `tmp/backups/`; write a JSON manifest containing source path, backup path, size, and SHA-256. Do not prune any existing backup.

- [ ] **Step 3: Run the realtime baseline**

Run:

```powershell
cargo test -p coolzhu-web-console --offline realtime -- --nocapture
node --check modules/gui-web/packages/web-console/src/app.js
```

Expected: capture the current pass/fail count. A baseline failure must be diagnosed before feature edits.

### Task 2: Add a browser-side realtime turn controller

**Files:**
- Modify: `modules/gui-web/packages/web-console/src/app.js:80-110`
- Modify: `modules/gui-web/packages/web-console/src/app.js:8348-8375`
- Test: `modules/gui-web/packages/web-console/src/main.rs:53580-53610`

- [ ] **Step 1: Add a failing wiring test**

Add a focused source-contract test next to the existing realtime frontend tests:

```rust
#[test]
fn realtime_full_stream_wires_delta_commit_and_generation_cancellation() {
    assert!(WEB_APP_JS.contains("createRealtimeTurnController"));
    assert!(WEB_APP_JS.contains("commitRealtimeAssistantDelta"));
    assert!(WEB_APP_JS.contains("flushRealtimeAssistantSpeech"));
    assert!(WEB_APP_JS.contains("invalidateRealtimeGeneration"));
    assert!(WEB_APP_JS.contains("turn_id"));
    assert!(WEB_APP_JS.contains("generation_id"));
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```powershell
cargo test -p coolzhu-web-console --offline realtime_full_stream_wires_delta_commit_and_generation_cancellation -- --nocapture
```

Expected: FAIL because the controller functions do not exist.

- [ ] **Step 3: Implement the controller state**

Add these browser-side primitives near the current realtime globals:

```javascript
function createRealtimeTurnController() {
  return {
    turnId: null,
    generationId: 0,
    messageId: null,
    committedText: "",
    pendingText: "",
    ttsTail: Promise.resolve(),
    finalTranscriptKeys: new Set(),
  };
}

const realtimeTurn = createRealtimeTurnController();

function invalidateRealtimeGeneration(reason = "cancelled") {
  realtimeTurn.generationId += 1;
  realtimeTurn.pendingText = "";
  realtimeTurn.committedText = "";
  realtimeTurn.messageId = null;
  realtimeTurn.ttsTail = Promise.resolve();
  emitLocalRealtimeSessionEvent("full_stream_generation_invalidated", {
    turn_id: realtimeTurn.turnId,
    generation_id: realtimeTurn.generationId,
    reason,
  });
}
```

Use `crypto.randomUUID()` with a timestamp fallback when a new final transcript starts a turn.

- [ ] **Step 4: Run the focused test and syntax check**

Expected: both pass.

### Task 3: Commit model deltas to TTS before message completion

**Files:**
- Modify: `modules/gui-web/packages/web-console/src/app.js:8348-8460`
- Modify: `modules/gui-web/packages/web-console/src/app.js:13105-13295`
- Test: `modules/gui-web/packages/web-console/src/main.rs:53580-53630`

- [ ] **Step 1: Add failing source-contract assertions**

Assert that `message_delta` calls `commitRealtimeAssistantDelta(data)` and `message_done` calls `flushRealtimeAssistantSpeech(data)` before the existing completed-message auto-speak path.

- [ ] **Step 2: Implement bounded phrase extraction**

Use a deterministic helper:

```javascript
function takeRealtimeSpeechSegments(text, { flush = false } = {}) {
  const source = String(text || "");
  const segments = [];
  let cursor = 0;
  const boundary = /[。！？!?；;\n]/g;
  for (let match = boundary.exec(source); match; match = boundary.exec(source)) {
    const end = match.index + match[0].length;
    const segment = source.slice(cursor, end).trim();
    if (segment) segments.push(segment);
    cursor = end;
  }
  let rest = source.slice(cursor);
  if (!flush && rest.length >= 48) {
    const splitAt = Math.max(rest.lastIndexOf("，", 48), rest.lastIndexOf(",", 48), 24);
    segments.push(rest.slice(0, splitAt + 1).trim());
    rest = rest.slice(splitAt + 1);
  }
  if (flush && rest.trim()) {
    segments.push(rest.trim());
    rest = "";
  }
  return { segments: segments.filter(Boolean), rest };
}
```

- [ ] **Step 3: Queue streaming TTS by turn and generation**

`commitRealtimeAssistantDelta` appends `data.delta`, extracts stable segments, and chains `ttsStreamText` calls on `realtimeTurn.ttsTail`. Each request includes `turn_id`, `generation_id`, and a segment index. Before and after awaiting, compare the captured generation to the current generation and silently discard stale work.

- [ ] **Step 4: Prevent duplicate completion speech**

`flushRealtimeAssistantSpeech` flushes only `pendingText`. `maybeAutoSpeakRealtimeReply` must skip the full message when the same `messageId` already committed streamed speech; it may retain the existing whole-message path for half-duplex fallback.

- [ ] **Step 5: Verify**

Run the focused tests, `node --check`, and `cargo test -p coolzhu-web-console --offline realtime`.

Expected: delta wiring passes and the existing segmented fallback tests remain green.

### Task 4: Keep input capture alive and deduplicate final ASR

**Files:**
- Modify: `modules/gui-web/packages/web-console/src/app.js:12775-13020`
- Modify: `modules/gui-web/packages/web-console/src/app.js:13545-13655`
- Test: `modules/gui-web/packages/web-console/src/main.rs:53620-53790`

- [ ] **Step 1: Add a failing contract test**

The test must assert that the full-stream final-transcript branch calls `submitRealtimeFinalTranscript` and does not call `stopAudioRealtimeState({ keepAutoTtsPending })` before `sendMessage()`.

- [ ] **Step 2: Implement transcript deduplication**

Use a key of normalized transcript plus a 2-second bucket:

```javascript
function realtimeFinalTranscriptKey(text, now = Date.now()) {
  return `${String(text || "").trim().replace(/\s+/g, " ").toLowerCase()}|${Math.floor(now / 2000)}`;
}
```

`submitRealtimeFinalTranscript` returns early for an existing key, creates a new `turnId`, resets streamed speech state, fills the composer, and starts one `sendMessage()` without stopping recorder or partial recognition.

- [ ] **Step 3: Preserve half-duplex fallback**

Only use continuous submission when `active_mode === "full_streaming"`. The existing stop/send/resume behavior remains unchanged for `half_duplex_guarded`.

- [ ] **Step 4: Verify**

Run the focused contract test, the realtime suite, and JS syntax check.

### Task 5: Correlate backend readiness evidence to one turn

**Files:**
- Modify: `modules/gui-web/packages/web-console/src/main.rs:230-317`
- Modify: `modules/gui-web/packages/web-console/src/main.rs:1947-2055`
- Modify: `modules/gui-web/packages/web-console/src/main.rs:37735-38130`
- Modify: `modules/gui-web/packages/web-console/src/main.rs:39526-39780`
- Test: `modules/gui-web/packages/web-console/src/main.rs:53960-54730`

- [ ] **Step 1: Add failing behavior tests**

Create tests proving:

```rust
assert!(!risk.full_streaming_ready); // four gates from unrelated turn ids
assert!(risk.full_streaming_ready);  // model delta, TTS chunk and AEC evidence share active session/turn
```

Also assert `source=model_stream_probe` cannot supply correlated production evidence.

- [ ] **Step 2: Extend realtime state**

Add optional active/correlated turn fields to `RealtimeSessionState`:

```rust
active_turn_id: Option<String>,
model_stream_turn_id: Option<String>,
tts_stream_turn_id: Option<String>,
aec_reference_turn_id: Option<String>,
```

Reset them on session start/stop and generation invalidation.

- [ ] **Step 3: Accept correlation fields on TTS stream requests and events**

Extend the TTS stream request DTO with `turn_id`, `generation_id`, and `segment_index`; carry them into emitted `tts_stream_chunk` payloads. Only mark correlated readiness when the session is active and `turn_id` matches the active turn.

- [ ] **Step 4: Tighten risk calculation**

`full_streaming_ready` requires the existing four gates plus matching current `model_stream_turn_id` and `tts_stream_turn_id`. Missing correlation adds a visible readiness note.

- [ ] **Step 5: Run backend tests**

Run:

```powershell
cargo test -p coolzhu-web-console --offline realtime_streaming_risk -- --nocapture
cargo test -p coolzhu-web-console --offline realtime_ -- --nocapture
```

Expected: all realtime tests pass.

### Task 6: Make barge-in cancel every output layer

**Files:**
- Modify: `modules/gui-web/packages/web-console/src/app.js:12618-12665`
- Modify: `modules/gui-web/packages/web-console/src/app.js:13175-13295`
- Test: `modules/gui-web/packages/web-console/src/main.rs:55070-55430`

- [ ] **Step 1: Add a failing barge-in wiring test**

Assert `handleRealtimeBargeInDecision` calls `invalidateRealtimeGeneration("barge-in")`, aborts the active model controller, clears queued chunks, and keeps/restarts input in Full Stream mode.

- [ ] **Step 2: Implement unified cancellation**

Call invalidation before stopping playback. Ensure every TTS queue callback checks the generation. Do not disable `audioRealtimeAutoTtsEnabled` permanently; preserve it for the next turn while the session remains running.

- [ ] **Step 3: Run tests**

Expected: barge-in, realtime, and JS checks pass.

### Task 7: Build and perform real browser/microphone verification

**Files:**
- Create: `docs/work-logs/2026-06-28-realtime-full-stream-verification.md`

- [ ] **Step 1: Build the Web Console**

Run:

```powershell
cargo fmt -p coolzhu-web-console
cargo build -p coolzhu-web-console --offline
cargo test -p coolzhu-web-console --offline realtime -- --nocapture
```

Expected: build succeeds and the realtime suite is green.

- [ ] **Step 2: Start the package and open the local Web Console**

Use the existing package launcher and health endpoint. Record process, port, and health output.

- [ ] **Step 3: Use the in-app Browser for the realtime UI**

Start realtime voice, grant microphone access only if the browser asks within this requested test, speak a two-clause sentence, and record timestamps for final ASR, first model delta, first TTS chunk, and first audio playback.

- [ ] **Step 4: Verify barge-in**

Speak a strong interrupt phrase during playback. Pass only if both model output and audio stop and a new turn can begin without restarting the session.

- [ ] **Step 5: Record limitations honestly**

If microphone, provider-native ASR, streaming TTS, or real model service is unavailable, mark that layer `BLOCKED`; retain code/test evidence but do not claim an end-to-end pass.
