import assert from "node:assert/strict";

await import("../src/realtime_voice_capture.js");
await import("../src/realtime_audio_output.js");

const api = globalThis.CoolzhuRealtimeVoiceCapture;
assert.equal(api.frameSamples(16000, 20), 320);
const ring = new api.FrameRingBuffer(3);
ring.push({ sequence: 1 });
ring.push({ sequence: 2 });
ring.push({ sequence: 3 });
ring.push({ sequence: 4 });
assert.deepEqual(ring.drain().map((frame) => frame.sequence), [2, 3, 4]);
assert.equal(api.isPhysicalMicrophoneLabel("麦克风阵列 (Realtek(R) Audio)"), true);
assert.equal(api.isPhysicalMicrophoneLabel("Steam Streaming Microphone"), false);
assert.equal(api.isPhysicalMicrophoneLabel("Virtual Desktop Audio"), false);

let reconnect = new api.ReconnectGuard(2);
assert.deepEqual(reconnect.next(), {
  retry: true,
  reason: "retryable",
  attempt: 1,
  delayMs: 250,
});
assert.equal(reconnect.next().retry, true);
assert.deepEqual(reconnect.next(), {
  retry: false,
  reason: "retry_limit_reached",
  attempt: 3,
});
reconnect.ready();
assert.equal(reconnect.next().attempt, 1);
reconnect = new api.ReconnectGuard(5);
assert.deepEqual(reconnect.next({ retryable: false }), {
  retry: false,
  reason: "non_retryable",
  attempt: 0,
});

const savedGlobals = {
  navigator: Object.getOwnPropertyDescriptor(globalThis, "navigator"),
  MediaRecorder: Object.getOwnPropertyDescriptor(globalThis, "MediaRecorder"),
  AudioContext: Object.getOwnPropertyDescriptor(globalThis, "AudioContext"),
  AudioWorkletNode: Object.getOwnPropertyDescriptor(globalThis, "AudioWorkletNode"),
  WebSocket: Object.getOwnPropertyDescriptor(globalThis, "WebSocket"),
};
let permissionTrackStops = 0;
let captureTrackStops = 0;
const makeStream = (onStop) => {
  const track = {
    stop: onStop,
    getSettings: () => ({}),
  };
  return {
    getTracks: () => [track],
    getAudioTracks: () => [track],
  };
};
class FakeMediaRecorder {
  constructor(stream) {
    this.stream = stream;
    this.state = "inactive";
    this.listeners = new Map();
  }
  addEventListener(kind, listener) {
    this.listeners.set(kind, listener);
  }
  start() {
    this.state = "recording";
  }
  stop() {
    this.listeners.get("dataavailable")?.({
      data: new Blob(["fake-audio"], { type: "audio/webm" }),
    });
    this.state = "inactive";
    this.listeners.get("stop")?.();
  }
}
try {
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: {
      mediaDevices: {
        async getUserMedia(constraints) {
          return constraints.audio === true
            ? makeStream(() => { permissionTrackStops += 1; })
            : makeStream(() => { captureTrackStops += 1; });
        },
        async enumerateDevices() {
          return [{
            kind: "audioinput",
            deviceId: "default",
            groupId: "physical",
            label: "麦克风阵列 (Realtek(R) Audio)",
          }];
        },
      },
    },
  });
  Object.defineProperty(globalThis, "MediaRecorder", {
    configurable: true,
    value: FakeMediaRecorder,
  });
  Object.defineProperty(globalThis, "AudioContext", {
    configurable: true,
    value: undefined,
  });
  Object.defineProperty(globalThis, "AudioWorkletNode", {
    configurable: true,
    value: undefined,
  });
  const finalSegmentFlags = [];
  const fallbackSession = await api.createSession({
    deviceId: "default",
    onSegment(_blob, metadata) {
      finalSegmentFlags.push(metadata.finalSegment);
    },
  });
  assert.equal(fallbackSession.mode, "mediarecorder_degraded");
  assert.equal(permissionTrackStops, 1);
  await fallbackSession.stop();
  assert.equal(captureTrackStops, 1);
  assert.deepEqual(finalSegmentFlags, [false, true]);

  let contextCloseCount = 0;
  class FakeAudioContext {
    constructor() {
      this.audioWorklet = { addModule: async () => {} };
    }
    createMediaStreamSource() {
      return { connect() {}, disconnect() {} };
    }
    async close() {
      contextCloseCount += 1;
    }
  }
  class FakeAudioWorkletNode {
    constructor() {
      this.port = {
        addEventListener() {},
        removeEventListener() {},
        start() {},
      };
    }
    disconnect() {}
  }
  Object.defineProperty(globalThis, "AudioContext", {
    configurable: true,
    value: FakeAudioContext,
  });
  Object.defineProperty(globalThis, "AudioWorkletNode", {
    configurable: true,
    value: FakeAudioWorkletNode,
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: class {
      constructor() {
        throw new Error("websocket_constructor_failed");
      }
    },
  });
  await assert.rejects(
    api.createSession({ deviceId: "default" }),
    /websocket_constructor_failed/,
  );
  assert.equal(contextCloseCount, 1);
  assert.equal(permissionTrackStops, 2);
  assert.equal(captureTrackStops, 2);
} finally {
  for (const [key, descriptor] of Object.entries(savedGlobals)) {
    if (descriptor) {
      Object.defineProperty(globalThis, key, descriptor);
    } else {
      delete globalThis[key];
    }
  }
}

const output = globalThis.CoolzhuRealtimeAudioOutput;
assert.equal(output.isPhysicalOutputLabel("扬声器 (Realtek(R) Audio)"), true);
assert.equal(output.isPhysicalOutputLabel("Virtual Desktop Audio"), false);
assert.equal(output.isPhysicalOutputLabel("立体声混音 (Realtek(R) Audio)"), false);

const defaultSinkAbort = await output.applyOutputSink({
  async setSinkId() {
    throw new DOMException("The operation could not be performed and was aborted", "AbortError");
  },
}, "default");
assert.deepEqual(defaultSinkAbort, {
  applied: false,
  reason: "sink_selection_failed",
  deviceId: "default",
  error: "The operation could not be performed and was aborted",
});

const sinkAttempts = [];
const fallbackSink = await output.applyOutputSink({
  async setSinkId(deviceId) {
    sinkAttempts.push(deviceId);
    if (deviceId !== "default") throw new DOMException("device missing", "NotFoundError");
  },
}, "missing-device");
assert.deepEqual(sinkAttempts, ["missing-device", "default"]);
assert.deepEqual(fallbackSink, {
  applied: true,
  deviceId: "default",
  fellBackToDefault: true,
  reason: "requested_sink_failed",
});

let detector = output.createBargeInDetector({ warmupFrames: 0 });
assert.equal(detector.push(0.8).state, "candidate");
assert.equal(detector.push(0.8).state, "candidate");
assert.equal(detector.push(0.8).state, "ducked");
for (let index = 0; index < 7; index += 1) detector.push(0.8);
assert.equal(detector.state(), "confirmed");

detector = output.createBargeInDetector({ warmupFrames: 0 });
for (let index = 0; index < 3; index += 1) detector.push(0.8);
for (let index = 0; index < 6; index += 1) detector.push(0.01);
assert.equal(detector.state(), "quiet");

console.log("realtime voice capture contracts: PASS");
