(() => {
  "use strict";

  const VIRTUAL_OUTPUT_LABELS = [
    "virtual desktop audio",
    "steam streaming speakers",
    "cable input",
    "vb-audio",
    "obs virtual",
    "stereo mix",
    "立体声混音",
  ];

  function isPhysicalOutputLabel(label) {
    const normalized = String(label || "").trim().toLowerCase();
    return Boolean(normalized)
      && !VIRTUAL_OUTPUT_LABELS.some((blocked) => normalized.includes(blocked));
  }

  async function listPhysicalOutputs() {
    if (!globalThis.navigator?.mediaDevices?.enumerateDevices) return [];
    const devices = await navigator.mediaDevices.enumerateDevices();
    return devices
      .filter((device) => device.kind === "audiooutput")
      .filter((device) => device.deviceId === "default" || isPhysicalOutputLabel(device.label))
      .map((device) => ({
        deviceId: device.deviceId,
        groupId: device.groupId,
        label: device.label || (device.deviceId === "default" ? "系统默认扬声器" : "未命名扬声器"),
      }));
  }

  function assertCurrentGeneration(generationId, currentGeneration) {
    if (typeof currentGeneration === "function" && generationId !== currentGeneration()) {
      throw new Error("stale_audio_generation");
    }
  }

  async function applyOutputSink(mediaElement, deviceId, options = {}) {
    assertCurrentGeneration(options.generationId, options.currentGeneration);
    const requestedDeviceId = deviceId || "default";
    if (!mediaElement || typeof mediaElement.setSinkId !== "function") {
      return { applied: false, reason: "media_set_sink_id_unavailable", deviceId: "default" };
    }
    try {
      await mediaElement.setSinkId(requestedDeviceId);
      assertCurrentGeneration(options.generationId, options.currentGeneration);
      return { applied: true, deviceId: requestedDeviceId };
    } catch (error) {
      assertCurrentGeneration(options.generationId, options.currentGeneration);
      if (requestedDeviceId !== "default") {
        try {
          await mediaElement.setSinkId("default");
          assertCurrentGeneration(options.generationId, options.currentGeneration);
          return {
            applied: true,
            deviceId: "default",
            fellBackToDefault: true,
            reason: "requested_sink_failed",
          };
        } catch {
          assertCurrentGeneration(options.generationId, options.currentGeneration);
        }
      }
      return {
        applied: false,
        reason: "sink_selection_failed",
        deviceId: "default",
        error: error?.message || String(error),
      };
    }
  }

  async function applyContextSink(audioContext, deviceId, options = {}) {
    assertCurrentGeneration(options.generationId, options.currentGeneration);
    if (!audioContext || typeof audioContext.setSinkId !== "function") {
      return { applied: false, reason: "context_set_sink_id_unavailable", deviceId: "default" };
    }
    await audioContext.setSinkId(deviceId || "default");
    assertCurrentGeneration(options.generationId, options.currentGeneration);
    return { applied: true, deviceId: deviceId || "default" };
  }

  function createBargeInDetector(options = {}) {
    const warmupFrames = Math.max(0, Number(options.warmupFrames ?? 30));
    const candidateThreshold = Number(options.candidateThreshold ?? 0.12);
    const quietThreshold = Number(options.quietThreshold ?? 0.04);
    const duckFrames = Math.max(1, Number(options.duckFrames ?? 3));
    const confirmFrames = Math.max(duckFrames, Number(options.confirmFrames ?? 10));
    const recoverFrames = Math.max(1, Number(options.recoverFrames ?? 6));
    const falseTriggerMs = Math.max(100, Number(options.falseTriggerMs ?? 1500));
    const resumeMs = Math.max(falseTriggerMs, Number(options.resumeMs ?? 3500));
    const now = typeof options.now === "function" ? options.now : () => Date.now();

    let frameCount = 0;
    let highFrames = 0;
    let lowFrames = 0;
    let currentState = "warming";
    let candidateAt = 0;
    let transcriptObserved = false;

    function transition(next, action = "none") {
      currentState = next;
      return { state: currentState, action };
    }

    return {
      push(level, { transcript = false } = {}) {
        frameCount += 1;
        transcriptObserved = transcriptObserved || transcript;
        if (frameCount <= warmupFrames) return transition("warming");
        const numericLevel = Number(level) || 0;
        if (numericLevel >= candidateThreshold) {
          highFrames += 1;
          lowFrames = 0;
          if (!candidateAt) candidateAt = now();
          if (highFrames >= confirmFrames) return transition("confirmed", "cancel");
          if (highFrames >= duckFrames) return transition("ducked", "duck");
          return transition("candidate");
        }
        if (numericLevel <= quietThreshold) {
          lowFrames += 1;
          highFrames = 0;
          if (lowFrames >= recoverFrames) {
            candidateAt = 0;
            transcriptObserved = false;
            return transition("quiet", "restore");
          }
        }
        if (candidateAt && !transcriptObserved && now() - candidateAt >= resumeMs) {
          candidateAt = 0;
          highFrames = 0;
          lowFrames = 0;
          return transition("quiet", "resume");
        }
        if (candidateAt && !transcriptObserved && now() - candidateAt >= falseTriggerMs) {
          return transition("ducked", "hold");
        }
        return transition(currentState === "warming" ? "quiet" : currentState);
      },
      transcriptObserved() {
        transcriptObserved = true;
      },
      reset() {
        frameCount = 0;
        highFrames = 0;
        lowFrames = 0;
        currentState = "warming";
        candidateAt = 0;
        transcriptObserved = false;
      },
      state() { return currentState; },
    };
  }

  function watchOutputDevice(selectedDeviceId, onChange) {
    if (!globalThis.navigator?.mediaDevices?.addEventListener) return () => {};
    const listener = async () => {
      try {
        const outputs = await listPhysicalOutputs();
        const selected = outputs.find((device) => device.deviceId === selectedDeviceId);
        if (typeof onChange === "function") {
          onChange(selected || outputs.find((device) => device.deviceId === "default") || null, {
            fellBackToDefault: !selected,
          });
        }
      } catch (error) {
        if (typeof onChange === "function") {
          onChange(null, {
            fellBackToDefault: selectedDeviceId !== "default",
            error: error?.message || String(error),
          });
        }
      }
    };
    navigator.mediaDevices.addEventListener("devicechange", listener);
    return () => navigator.mediaDevices.removeEventListener("devicechange", listener);
  }

  globalThis.CoolzhuRealtimeAudioOutput = {
    isPhysicalOutputLabel,
    listPhysicalOutputs,
    applyOutputSink,
    applyContextSink,
    createBargeInDetector,
    watchOutputDevice,
  };
})();
