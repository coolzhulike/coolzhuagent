(function bootstrap(root, factory) {
  const api = factory(root);
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  } else if (root) {
    root.CoolzhuSevenLetterStartupBridge = api;
    api.autoStart();
  }
})(typeof globalThis === "object" ? globalThis : this, function createStartupBridge(root) {
  "use strict";

  const INTERNAL_EVENT = "coolzhu-seven-letter-startup-internal";
  const COMPLETE_EVENT = "launch-performance-complete";

  function createBridge(options = {}) {
    const documentRef = options.documentRef || (root && root.document) || null;
    const windowRef = options.windowRef || (documentRef && documentRef.defaultView) || root || null;
    const playerApi = options.playerApi || (root && root.CoolzhuScrollStartupPlayer) || null;
    const manifest = options.manifest || (root && root.CoolzhuSevenLetterManifest) || null;
    const canvas = options.canvas || (documentRef && documentRef.getElementById(options.canvasId || "launch-performance-canvas"));
    const skipButton = options.skipButton || (documentRef && documentRef.getElementById(options.skipId || "launch-performance-skip"));
    const presentationRoot = options.presentationRoot || (documentRef && documentRef.getElementById(options.rootId || "launch-performance"));
    let instance = null;
    let started = false;
    let finished = false;
    let listenersInstalled = false;
    let watchdogId = 0;
    let startedAt = null;
    let nativeReports = Promise.resolve();
    let mode = options.mode || "";
    if (!mode) {
      try { mode = new URLSearchParams(windowRef.location?.search || "").get("mode") || ""; } catch (_) { /* 测试或受限宿主 */ }
    }
    if (!["first", "daily", "restore"].includes(mode)) {
      try { mode = windowRef.localStorage?.getItem("coolzhu.scroll-startup.seen.v1") ? "daily" : "first"; }
      catch (_) { mode = "daily"; }
    }

    function clearPresentation() {
      if (canvas && typeof canvas.getContext === "function") {
        const context = canvas.getContext("2d");
        if (context) {
          if (typeof context.save === "function") context.save();
          if (typeof context.setTransform === "function") context.setTransform(1, 0, 0, 1, 0, 0);
          if (typeof context.clearRect === "function") context.clearRect(0, 0, Math.max(1, canvas.width || 1), Math.max(1, canvas.height || 1));
          if (typeof context.restore === "function") context.restore();
        }
      }
      if (canvas) {
        canvas.hidden = true;
        if (canvas.style) canvas.style.display = "none";
        if (typeof canvas.setAttribute === "function") canvas.setAttribute("aria-hidden", "true");
      }
      if (skipButton) {
        skipButton.hidden = true;
        skipButton.disabled = true;
        if (skipButton.style) skipButton.style.display = "none";
      }
      if (presentationRoot) {
        presentationRoot.hidden = true;
        if (presentationRoot.style) presentationRoot.style.display = "none";
      }
    }

    const reducedMotion = options.reducedMotion !== undefined
      ? options.reducedMotion === true
      : Boolean(windowRef?.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches);
    const now = () => typeof windowRef?.performance?.now === "function" ? windowRef.performance.now() : Date.now();

    function reportNative(phase, reason, state) {
      const core = windowRef?.__TAURI__?.core;
      if (typeof core?.invoke !== "function") {
        windowRef?.console?.error?.("启动演出无法上报宿主：Tauri invoke 不可用");
        return;
      }
      const report = {
        phase, mode, reducedMotion,
        elapsedMs: Math.max(0, Math.round(now() - startedAt)),
        frameCount: state?.frameCount || 0,
        reason: reason || null,
      };
      // 保持原生诊断顺序；任一上报失败时明确报错，宿主的 15 秒兜底仍会接管。
      nativeReports = nativeReports.then(() => core.invoke("report_startup_performance", { report }))
        .catch(error => { windowRef?.console?.error?.("启动演出上报宿主失败", error); });
    }

    function dispatchCompletion(reason) {
      const detail = Object.freeze({ reason, consoleVisible: false });
      if (documentRef && typeof documentRef.dispatchEvent === "function") {
        let event = null;
        if (windowRef && typeof windowRef.CustomEvent === "function") event = new windowRef.CustomEvent(COMPLETE_EVENT, { detail });
        else if (typeof CustomEvent === "function") event = new CustomEvent(COMPLETE_EVENT, { detail });
        if (event) documentRef.dispatchEvent(event);
      }
    }

    function removeListeners() {
      if (!listenersInstalled) return;
      listenersInstalled = false;
      documentRef?.removeEventListener?.("keydown", onKeyDown);
      documentRef?.removeEventListener?.("pointerdown", onPointerDown);
      documentRef?.removeEventListener?.(INTERNAL_EVENT, onPlayerFinished);
      skipButton?.removeEventListener?.("click", onSkipClick);
    }

    function finalize(reason, state) {
      if (finished) return false;
      finished = true;
      windowRef?.clearTimeout?.(watchdogId);
      if (["completed", "skipped", "reduced-motion"].includes(reason)) {
        try { windowRef.localStorage?.setItem("coolzhu.scroll-startup.seen.v1", "1"); } catch (_) { /* 存储失败不影响启动 */ }
      }
      removeListeners();
      clearPresentation();
      reportNative("finished", reason, state || instance?.getState?.());
      dispatchCompletion(reason);
      return true;
    }

    function requestFinish(reason) {
      if (finished) return false;
      if (instance && typeof instance.stop === "function") {
        const stopped = instance.stop(reason);
        if (!stopped) finalize(reason);
        return true;
      }
      return finalize(reason);
    }

    function onPlayerFinished(event) {
      const reason = event && event.detail && event.detail.reason ? event.detail.reason : "resource-error";
      finalize(reason, event?.detail?.state);
    }

    function onSkipClick() {
      requestFinish("skipped");
    }

    function onKeyDown(event) {
      if (event && event.key === "Escape") {
        event.preventDefault?.();
        requestFinish("skipped");
      }
    }

    function onPointerDown(event) {
      const target = event && event.target;
      if (target === skipButton || target?.closest?.("[data-skip-launch]")) return;
      requestFinish("skipped");
    }

    function installListeners() {
      if (listenersInstalled) return;
      listenersInstalled = true;
      documentRef?.addEventListener?.(INTERNAL_EVENT, onPlayerFinished);
      documentRef?.addEventListener?.("keydown", onKeyDown);
      documentRef?.addEventListener?.("pointerdown", onPointerDown);
      skipButton?.addEventListener?.("click", onSkipClick);
    }

    function start() {
      if (started || finished) return instance;
      started = true;
      startedAt = now();
      reportNative("started");
      if (mode === "restore") { finalize("restored"); return null; }
      if (!playerApi || typeof playerApi.createStartupPlayer !== "function" || !manifest || !canvas) {
        finalize("resource-error");
        return null;
      }
      installListeners();
      watchdogId = windowRef?.setTimeout?.(() => requestFinish("presentation-timeout"), 11000) || 0;
      try {
        instance = playerApi.createStartupPlayer({
          manifest: { ...manifest, completeEvent: INTERNAL_EVENT },
          documentRef,
          windowRef,
          canvas,
          assets: options.assets,
          mode,
          reducedMotion,
          reducedMotionDelayMs: 120,
          onAssetsReady: () => reportNative("assets_ready"),
          onFirstFrame: () => reportNative("first_frame", null, instance?.getState?.()),
        });
        instance.start();
      } catch (error) {
        finalize("resource-error");
      }
      return instance;
    }

    return Object.freeze({
      manifest,
      start,
      finish: requestFinish,
      getInstance: () => instance,
      isFinished: () => finished,
    });
  }

  function autoStart() {
    const documentRef = root && root.document;
    if (!documentRef) return null;
    const run = () => createBridge().start();
    if (documentRef.readyState === "loading") {
      documentRef.addEventListener("DOMContentLoaded", run, { once: true });
      return null;
    }
    return run();
  }

  return Object.freeze({ COMPLETE_EVENT, INTERNAL_EVENT, autoStart, createBridge });
});
