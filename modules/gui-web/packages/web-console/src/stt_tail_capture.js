(() => {
  "use strict";

  const DEFAULT_TARGET_SAMPLE_RATE = 16000;
  const DEFAULT_MAX_DURATION_MS = 60000;
  const DEFAULT_FRAME_MS = 20;
  const PROCESSOR_NAME = "coolzhu-stt-tail-capture";

  class PcmRingBuffer {
    constructor(capacity) {
      this.capacity = Math.max(1, Math.floor(Number(capacity) || 1));
      this.samples = new Int16Array(this.capacity);
      this.start = 0;
      this.length = 0;
    }

    push(values) {
      for (const value of values) {
        if (this.length < this.capacity) {
          this.samples[(this.start + this.length) % this.capacity] = value;
          this.length += 1;
        } else {
          this.samples[this.start] = value;
          this.start = (this.start + 1) % this.capacity;
        }
      }
    }

    snapshot() {
      const output = new Int16Array(this.length);
      for (let index = 0; index < this.length; index += 1) {
        output[index] = this.samples[(this.start + index) % this.capacity];
      }
      return output;
    }
  }

  function writeAscii(view, offset, value) {
    for (let index = 0; index < value.length; index += 1) {
      view.setUint8(offset + index, value.charCodeAt(index));
    }
  }

  function encodePcm16Wav(samples, sampleRate = DEFAULT_TARGET_SAMPLE_RATE) {
    const pcm = samples instanceof Int16Array ? samples : Int16Array.from(samples || []);
    const rate = Math.max(1, Math.round(Number(sampleRate) || DEFAULT_TARGET_SAMPLE_RATE));
    const dataBytes = pcm.length * 2;
    const bytes = new Uint8Array(44 + dataBytes);
    const view = new DataView(bytes.buffer);

    writeAscii(view, 0, "RIFF");
    view.setUint32(4, 36 + dataBytes, true);
    writeAscii(view, 8, "WAVE");
    writeAscii(view, 12, "fmt ");
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, 1, true);
    view.setUint32(24, rate, true);
    view.setUint32(28, rate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    writeAscii(view, 36, "data");
    view.setUint32(40, dataBytes, true);
    for (let index = 0; index < pcm.length; index += 1) {
      view.setInt16(44 + index * 2, pcm[index], true);
    }
    return bytes;
  }

  function pcm16FromFloat32(samples) {
    const pcm = new Int16Array(samples.length);
    for (let index = 0; index < samples.length; index += 1) {
      const value = Math.max(-1, Math.min(1, samples[index]));
      pcm[index] = value < 0 ? value * 0x8000 : value * 0x7fff;
    }
    return pcm;
  }

  function workletSource() {
    return `
class CoolzhuSttTailCaptureProcessor extends AudioWorkletProcessor {
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
registerProcessor("${PROCESSOR_NAME}", CoolzhuSttTailCaptureProcessor);
`;
  }

  async function createSession({
    stream,
    maxDurationMs = DEFAULT_MAX_DURATION_MS,
    targetSampleRate = DEFAULT_TARGET_SAMPLE_RATE,
  } = {}) {
    const AudioContextClass = globalThis.AudioContext || globalThis.webkitAudioContext;
    if (!stream?.getTracks) {
      throw new Error("microphone_stream_unavailable");
    }
    if (typeof AudioContextClass !== "function" || typeof globalThis.AudioWorkletNode !== "function") {
      stream.getTracks().forEach((track) => track.stop());
      throw new Error("audio_worklet_unavailable");
    }

    const rate = Math.max(8000, Math.round(Number(targetSampleRate) || DEFAULT_TARGET_SAMPLE_RATE));
    const durationMs = Math.max(1000, Math.round(Number(maxDurationMs) || DEFAULT_MAX_DURATION_MS));
    const frameSize = Math.max(1, Math.round(rate * DEFAULT_FRAME_MS / 1000));
    const ring = new PcmRingBuffer(Math.round(rate * durationMs / 1000));
    const context = new AudioContextClass();
    let source = null;
    let node = null;
    let sourceUrl = null;
    let stopped = false;
    let stoppedResult = null;

    try {
      sourceUrl = URL.createObjectURL(new Blob([workletSource()], { type: "application/javascript" }));
      await context.audioWorklet.addModule(sourceUrl);
      URL.revokeObjectURL(sourceUrl);
      sourceUrl = null;
      source = context.createMediaStreamSource(stream);
      node = new AudioWorkletNode(context, PROCESSOR_NAME, {
        numberOfInputs: 1,
        numberOfOutputs: 0,
        channelCount: 1,
        processorOptions: { targetRate: rate, frameSize },
      });
      node.port.onmessage = (event) => ring.push(pcm16FromFloat32(event.data));
      source.connect(node);
      if (context.state === "suspended") {
        await context.resume();
      }
    } catch (error) {
      if (sourceUrl) URL.revokeObjectURL(sourceUrl);
      try { node?.disconnect(); } catch (_) {}
      try { source?.disconnect(); } catch (_) {}
      stream.getTracks().forEach((track) => track.stop());
      await context.close().catch(() => {});
      throw error;
    }

    const session = {
      mode: "audio_worklet_pcm_tail",
      state: "recording",
      sampleRate: rate,
      maxDurationMs: durationMs,
      async stop() {
        if (stopped) return stoppedResult;
        stopped = true;
        node.port.onmessage = null;
        try { node.disconnect(); } catch (_) {}
        try { source.disconnect(); } catch (_) {}
        stream.getTracks().forEach((track) => track.stop());
        await context.close().catch(() => {});
        const samples = ring.snapshot();
        const bytes = encodePcm16Wav(samples, rate);
        stoppedResult = {
          blob: new Blob([bytes], { type: "audio/wav" }),
          durationMs: Math.round(samples.length * 1000 / rate),
          sampleCount: samples.length,
        };
        session.state = "inactive";
        return stoppedResult;
      },
    };
    return session;
  }

  globalThis.CoolzhuSttTailCapture = {
    PcmRingBuffer,
    encodePcm16Wav,
    createSession,
  };
})();
