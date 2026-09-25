(() => {
  "use strict";

  const TARGET_SAMPLE_RATE = 16000;
  const DEFAULT_FRAME_MS = 20;
  const DEFAULT_RECONNECT_BUFFER_MS = 8000;
  const DEFAULT_BARGE_IN_BUFFER_MS = 1500;
  const DEFAULT_MAX_RECONNECT_ATTEMPTS = 5;
  const VIRTUAL_MICROPHONE_LABELS = [
    "steam streaming microphone",
    "virtual desktop audio",
    "cable output",
    "vb-audio",
    "obs virtual",
    "stereo mix",
    "立体声混音",
  ];

  const frameSamples = (sampleRate, frameMs) =>
    Math.max(1, Math.round(sampleRate * frameMs / 1000));

  class FrameRingBuffer {
    constructor(limit) {
      this.limit = Math.max(1, Number(limit) || 1);
      this.frames = [];
      this.dropped = 0;
    }

    push(frame) {
      this.frames.push(frame);
      while (this.frames.length > this.limit) {
        this.frames.shift();
        this.dropped += 1;
      }
    }

    drain() {
      const frames = this.frames;
      this.frames = [];
      return frames;
    }

    snapshot() {
      return this.frames.slice();
    }
  }

  function isPhysicalMicrophoneLabel(label) {
    const normalized = String(label || "").trim().toLowerCase();
    return Boolean(normalized)
      && !VIRTUAL_MICROPHONE_LABELS.some((blocked) => normalized.includes(blocked));
  }

  function stopStreamTracks(stream) {
    stream?.getTracks?.().forEach((track) => {
      try {
        track.stop();
      } catch {
        // 轨道可能已由浏览器回收。
      }
    });
  }

  async function listMicrophones() {
    if (!globalThis.navigator?.mediaDevices?.enumerateDevices) return [];
    const devices = await navigator.mediaDevices.enumerateDevices();
    return devices
      .filter((device) => device.kind === "audioinput")
      .filter((device) =>
        device.deviceId === "default"
        || !String(device.label || "").trim()
        || isPhysicalMicrophoneLabel(device.label))
      .map((device) => ({
        deviceId: device.deviceId,
        groupId: device.groupId,
        label: device.label || (device.deviceId === "default" ? "系统默认麦克风" : "未命名麦克风"),
      }));
  }

  class ReconnectGuard {
    constructor(maxAttempts = DEFAULT_MAX_RECONNECT_ATTEMPTS) {
      this.maxAttempts = Math.max(0, Number(maxAttempts) || 0);
      this.consecutiveAttempts = 0;
    }

    ready() {
      this.consecutiveAttempts = 0;
    }

    next({ retryable = true } = {}) {
      if (retryable === false) {
        return { retry: false, reason: "non_retryable", attempt: this.consecutiveAttempts };
      }
      this.consecutiveAttempts += 1;
      if (this.consecutiveAttempts > this.maxAttempts) {
        return {
          retry: false,
          reason: "retry_limit_reached",
          attempt: this.consecutiveAttempts,
        };
      }
      return {
        retry: true,
        reason: "retryable",
        attempt: this.consecutiveAttempts,
        delayMs: Math.min(2000, 250 * this.consecutiveAttempts),
      };
    }
  }

  function pcm16FromFloat32(samples) {
    const pcm = new Int16Array(samples.length);
    for (let index = 0; index < samples.length; index += 1) {
      const value = Math.max(-1, Math.min(1, samples[index]));
      pcm[index] = value < 0 ? value * 0x8000 : value * 0x7fff;
    }
    return Array.from(pcm);
  }

  function websocketUrl(pathname) {
    const base = new URL(pathname, globalThis.location?.href || "http://127.0.0.1:8765");
    base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
    return base.toString();
  }

  function workletSource() {
    return `
class CoolzhuPcmCaptureProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const processorOptions = options.processorOptions || {};
    this.targetRate = processorOptions.targetRate || 16000;
    this.frameSize = processorOptions.frameSize || 320;
    this.pending = [];
    this.phase = 0;
  }

  process(inputs) {
    const channel = inputs[0] && inputs[0][0];
    if (!channel || !channel.length) return true;
    const ratio = sampleRate / this.targetRate;
    while (this.phase < channel.length) {
      const left = Math.floor(this.phase);
      const right = Math.min(channel.length - 1, left + 1);
      const fraction = this.phase - left;
      this.pending.push(channel[left] + (channel[right] - channel[left]) * fraction);
      this.phase += ratio;
    }
    this.phase -= channel.length;
    while (this.pending.length >= this.frameSize) {
      const frame = new Float32Array(this.pending.splice(0, this.frameSize));
      this.port.postMessage(frame, [frame.buffer]);
    }
    return true;
  }
}
registerProcessor("coolzhu-pcm-capture", CoolzhuPcmCaptureProcessor);
`;
  }

  async function selectMicrophone(requestedDeviceId, requirePhysical) {
    const permissionStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    let microphones = [];
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      microphones = devices.filter((device) => device.kind === "audioinput");
    } finally {
      stopStreamTracks(permissionStream);
    }
    const requested = requestedDeviceId
      ? microphones.find((device) => device.deviceId === requestedDeviceId)
      : null;
    const selected = requested
      || microphones.find((device) => device.deviceId === "default")
      || microphones.find((device) => isPhysicalMicrophoneLabel(device.label))
      || microphones[0];
    if (!selected) throw new Error("physical_microphone_not_found");
    if (requirePhysical && !isPhysicalMicrophoneLabel(selected.label)) {
      throw new Error(`virtual_microphone_rejected:${selected.label || selected.deviceId}`);
    }
    return selected;
  }

  async function createMediaRecorderFallback(options, selected, stats) {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { deviceId: { exact: selected.deviceId } },
    });
    if (typeof MediaRecorder !== "function") {
      stopStreamTracks(stream);
      throw new Error("audio_worklet_and_mediarecorder_unavailable");
    }
    const recorder = new MediaRecorder(stream);
    const recordedChunks = [];
    stats.mode = "mediarecorder_degraded";
    recorder.addEventListener("dataavailable", (event) => {
      if (event.data?.size) {
        recordedChunks.push(event.data);
        if (typeof options.onSegment === "function") {
          options.onSegment(event.data, {
            finalSegment: false,
            mode: "mediarecorder_degraded",
          });
        }
      }
    });
    recorder.start(250);
    return {
      mode: "mediarecorder_degraded",
      selectedDevice: { deviceId: selected.deviceId, label: selected.label },
      stats,
      async stop() {
        if (recorder.state !== "inactive") {
          await new Promise((resolve) => {
            const timeout = setTimeout(resolve, 1000);
            recorder.addEventListener("stop", () => {
              clearTimeout(timeout);
              resolve();
            }, { once: true });
            try {
              recorder.stop();
            } catch {
              clearTimeout(timeout);
              resolve();
            }
          });
        }
        if (recordedChunks.length && typeof options.onSegment === "function") {
          await options.onSegment(
            new Blob(recordedChunks, { type: recorder.mimeType || recordedChunks[0]?.type }),
            { finalSegment: true, mode: "mediarecorder_degraded" },
          );
        }
        stopStreamTracks(stream);
      },
      bargeInFrames() { return []; },
    };
  }

  async function createSession(options = {}) {
    if (!globalThis.navigator?.mediaDevices?.getUserMedia) {
      throw new Error("media_devices_unavailable");
    }
    const sessionId = String(options.sessionId || crypto.randomUUID());
    const frameMs = Math.max(20, Math.min(40, Number(options.frameMs) || DEFAULT_FRAME_MS));
    const reconnectBufferMs = Math.max(
      frameMs,
      Number(options.reconnectBufferMs) || DEFAULT_RECONNECT_BUFFER_MS,
    );
    const selected = await selectMicrophone(options.deviceId, options.requirePhysical !== false);
    const stats = {
      mode: "audio_worklet_pcm",
      sessionId,
      selectedInputLabel: selected.label,
      frames: 0,
      bytes: 0,
      droppedFrames: 0,
      reconnects: 0,
      maxFrameGapMs: 0,
      lastFrameAtMs: 0,
    };

    if (typeof AudioContext !== "function" || typeof AudioWorkletNode !== "function") {
      return createMediaRecorderFallback(options, selected, stats);
    }

    const stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        deviceId: { exact: selected.deviceId },
        channelCount: 1,
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    const trackSettings = stream.getAudioTracks()[0]?.getSettings?.() || {};
    stats.echoCancellation = trackSettings.echoCancellation === true;
    stats.noiseSuppression = trackSettings.noiseSuppression === true;
    stats.autoGainControl = trackSettings.autoGainControl === true;
    const context = new AudioContext();
    const sourceUrl = URL.createObjectURL(new Blob([workletSource()], { type: "application/javascript" }));
    try {
      await context.audioWorklet.addModule(sourceUrl);
    } catch (error) {
      stopStreamTracks(stream);
      await context.close().catch(() => {});
      throw error;
    } finally {
      URL.revokeObjectURL(sourceUrl);
    }

    const reconnectBuffer = new FrameRingBuffer(Math.ceil(reconnectBufferMs / frameMs));
    const bargeInBuffer = new FrameRingBuffer(Math.ceil(DEFAULT_BARGE_IN_BUFFER_MS / frameMs));
    let source;
    let node;
    try {
      source = context.createMediaStreamSource(stream);
      node = new AudioWorkletNode(context, "coolzhu-pcm-capture", {
        numberOfInputs: 1,
        numberOfOutputs: 0,
        channelCount: 1,
        processorOptions: {
          targetRate: TARGET_SAMPLE_RATE,
          frameSize: frameSamples(TARGET_SAMPLE_RATE, frameMs),
        },
      });
      source.connect(node);
    } catch (error) {
      try {
        source?.disconnect();
      } catch {
        // 初始化未完成时允许静默清理。
      }
      stopStreamTracks(stream);
      await context.close().catch(() => {});
      throw error;
    }

    let stopped = false;
    let transportStopped = false;
    let degradedRecorder = null;
    let degradedRecordedChunks = [];
    let sequence = 0;
    let socket = null;
    let reconnectTimer = null;
    let lastStreamError = null;
    const endpoint = options.endpoint || "/api/audio/realtime/stream";
    const reconnectGuard = new ReconnectGuard(
      options.maxReconnectAttempts ?? DEFAULT_MAX_RECONNECT_ATTEMPTS,
    );
    const controller = {
      mode: "audio_worklet_pcm",
      sessionId,
      selectedDevice: { deviceId: selected.deviceId, label: selected.label },
      stats,
      bargeInFrames() { return bargeInBuffer.snapshot(); },
      stop: async () => {},
    };

    const emitStats = () => {
      stats.droppedFrames = reconnectBuffer.dropped;
      if (typeof options.onStats === "function") options.onStats({ ...stats });
    };

    const sendFrame = (frame) => {
      if (socket?.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ type: "pcm_frame", frame }));
      } else {
        reconnectBuffer.push(frame);
      }
    };

    const stopPcmTransport = async ({ notifyProvider = false } = {}) => {
      if (transportStopped) return;
      transportStopped = true;
      clearTimeout(reconnectTimer);
      if (notifyProvider && socket?.readyState === WebSocket.OPEN) {
        try {
          socket.send(JSON.stringify({ type: "stop", session_id: sessionId }));
        } catch {
          // WebSocket 可能在发送前已关闭。
        }
      }
      try {
        socket?.close();
      } catch {
        // ignore close failures
      }
      socket = null;
      try {
        node?.port?.removeEventListener("message", handleWorkletMessage);
        node?.disconnect();
      } catch {
        // ignore disconnected worklet
      }
      try {
        source?.disconnect();
      } catch {
        // ignore disconnected source
      }
      await context.close().catch(() => {});
    };

    const degradeToMediaRecorder = async (reason) => {
      if (stopped || degradedRecorder) return;
      await stopPcmTransport();
      if (stopped) {
        stopStreamTracks(stream);
        return;
      }
      if (typeof MediaRecorder !== "function") {
        stopStreamTracks(stream);
        if (typeof options.onFatal === "function") {
          options.onFatal({ ...reason, code: reason?.code || "mediarecorder_unavailable" });
        }
        return;
      }
      stats.mode = "mediarecorder_degraded";
      controller.mode = "mediarecorder_degraded";
      try {
        degradedRecorder = new MediaRecorder(stream);
        degradedRecorder.addEventListener("dataavailable", (event) => {
          if (event.data?.size) {
            degradedRecordedChunks.push(event.data);
            if (typeof options.onSegment === "function") {
              options.onSegment(event.data, {
                finalSegment: false,
                mode: "mediarecorder_degraded",
              });
            }
          }
        });
        degradedRecorder.start(250);
      } catch (error) {
        degradedRecorder = null;
        stopStreamTracks(stream);
        if (typeof options.onFatal === "function") {
          options.onFatal({
            ...reason,
            code: "mediarecorder_degrade_failed",
            message: error?.message || String(error),
            retryable: false,
          });
        }
        return;
      }
      emitStats();
      if (typeof options.onDegraded === "function") {
        options.onDegraded({
          ...reason,
          mode: "mediarecorder_degraded",
          selectedDevice: controller.selectedDevice,
        });
      }
    };

    const openCircuit = (reason) => {
      if (stopped || transportStopped) return;
      lastStreamError = reason;
      void degradeToMediaRecorder(reason);
    };

    const connect = () => {
      if (stopped || transportStopped) return;
      socket = new WebSocket(websocketUrl(endpoint));
      socket.addEventListener("open", () => {
        if (stopped || transportStopped) return;
        socket.send(JSON.stringify({
          type: "start",
          session_id: sessionId,
          sample_rate_hz: TARGET_SAMPLE_RATE,
          channels: 1,
          frame_ms: frameMs,
        }));
        for (const frame of reconnectBuffer.drain()) sendFrame(frame);
        emitStats();
      });
      socket.addEventListener("message", (event) => {
        try {
          const message = JSON.parse(String(event.data));
          if (message?.type === "ready") {
            reconnectGuard.ready();
            lastStreamError = null;
          } else if (message?.type === "error") {
            lastStreamError = message;
            if (message.retryable === false) {
              openCircuit({
                ...message,
                reason: "non_retryable",
              });
            }
          }
          if (typeof options.onEvent === "function") options.onEvent(message);
        } catch (error) {
          if (typeof options.onEvent === "function") {
            options.onEvent({ type: "error", code: "invalid_stream_event", message: error.message });
          }
        }
      });
      socket.addEventListener("close", () => {
        if (stopped || transportStopped) return;
        stats.reconnects += 1;
        emitStats();
        const decision = reconnectGuard.next({
          retryable: lastStreamError?.retryable !== false,
        });
        if (!decision.retry) {
          openCircuit({
            type: "error",
            code: lastStreamError?.code || decision.reason,
            message: lastStreamError?.message || "实时语音流已达到重试上限",
            retryable: false,
            reason: decision.reason,
            attempt: decision.attempt,
          });
          return;
        }
        clearTimeout(reconnectTimer);
        reconnectTimer = setTimeout(connect, decision.delayMs);
      });
      socket.addEventListener("error", () => {
        try {
          socket?.close();
        } catch {
          // 关闭竞争不应制造新的未捕获异常。
        }
      });
    };

    function handleWorkletMessage(event) {
      if (stopped || transportStopped) return;
      const now = Date.now();
      if (stats.lastFrameAtMs) {
        stats.maxFrameGapMs = Math.max(stats.maxFrameGapMs, now - stats.lastFrameAtMs);
      }
      stats.lastFrameAtMs = now;
      let sumSquares = 0;
      for (const sample of event.data) sumSquares += sample * sample;
      const rms = Math.sqrt(sumSquares / Math.max(1, event.data.length));
      if (typeof options.onAudioLevel === "function") options.onAudioLevel(rms);
      const samples = pcm16FromFloat32(event.data);
      const frame = {
        session_id: sessionId,
        sequence: sequence += 1,
        captured_at_ms: now,
        sample_rate_hz: TARGET_SAMPLE_RATE,
        channels: 1,
        samples,
      };
      stats.frames += 1;
      stats.bytes += samples.length * 2;
      bargeInBuffer.push(frame);
      sendFrame(frame);
      emitStats();
    }
    node.port.addEventListener("message", handleWorkletMessage);
    node.port.start();
    try {
      connect();
    } catch (error) {
      transportStopped = true;
      try {
        node.port.removeEventListener("message", handleWorkletMessage);
        node.disconnect();
        source.disconnect();
      } catch {
        // 初始化失败时尽力释放已创建的 WebAudio 节点。
      }
      await context.close().catch(() => {});
      stopStreamTracks(stream);
      throw error;
    }

    controller.stop = async () => {
      if (stopped) return;
      stopped = true;
      if (degradedRecorder?.state !== "inactive") {
        await new Promise((resolve) => {
          const timeout = setTimeout(resolve, 1000);
          degradedRecorder.addEventListener("stop", () => {
            clearTimeout(timeout);
            resolve();
          }, { once: true });
          try {
            degradedRecorder.stop();
          } catch {
            clearTimeout(timeout);
            resolve();
          }
        });
      }
      if (degradedRecordedChunks.length && typeof options.onSegment === "function") {
        await options.onSegment(
          new Blob(degradedRecordedChunks, {
            type: degradedRecorder?.mimeType || degradedRecordedChunks[0]?.type,
          }),
          { finalSegment: true, mode: "mediarecorder_degraded" },
        );
      }
      degradedRecordedChunks = [];
      degradedRecorder = null;
      if (!transportStopped) {
        await stopPcmTransport({ notifyProvider: true });
      } else {
        clearTimeout(reconnectTimer);
        try {
          socket?.close();
        } catch {
          // ignore close failures
        }
      }
      stopStreamTracks(stream);
    };
    return controller;
  }

  globalThis.CoolzhuRealtimeVoiceCapture = {
    frameSamples,
    FrameRingBuffer,
    ReconnectGuard,
    isPhysicalMicrophoneLabel,
    listMicrophones,
    createSession,
  };
})();
