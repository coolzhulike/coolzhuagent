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
    const playerApi = options.playerApi || (root && root.CoolzhuSevenLetterStartupPlayer) || null;
    const manifest = options.manifest || (root && root.CoolzhuSevenLetterManifest) || null;
    const canvas = options.canvas || (documentRef && documentRef.getElementById(options.canvasId || "launch-performance-canvas"));
    const skipButton = options.skipButton || (documentRef && documentRef.getElementById(options.skipId || "launch-performance-skip"));
    const presentationRoot = options.presentationRoot || (documentRef && documentRef.getElementById(options.rootId || "launch-performance"));
    let instance = null;
    let started = false;
    let finished = false;
    let listenersInstalled = false;

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

    function dispatchCompletion(reason) {
      const detail = Object.freeze({ reason, consoleVisible: false });
      if (documentRef && typeof documentRef.dispatchEvent === "function") {
        let event = null;
        if (windowRef && typeof windowRef.CustomEvent === "function") event = new windowRef.CustomEvent(COMPLETE_EVENT, { detail });
        else if (typeof CustomEvent === "function") event = new CustomEvent(COMPLETE_EVENT, { detail });
        if (event) documentRef.dispatchEvent(event);
      }
      // Tauri 窗口需要把同一个完成契约送到 Rust 宿主；普通浏览器/Node 环境没有该 API 时只保留 DOM 事件。
      const tauriEvent = windowRef && windowRef.__TAURI__ && windowRef.__TAURI__.event;
      if (tauriEvent && typeof tauriEvent.emit === "function") {
        try {
          const pending = tauriEvent.emit(COMPLETE_EVENT, detail);
          if (pending && typeof pending.catch === "function") pending.catch(() => {});
        } catch (_) {
          // 独立浏览器预览或权限尚未就绪时不阻断启动动画收尾。
        }
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

    function finalize(reason) {
      if (finished) return false;
      finished = true;
      removeListeners();
      clearPresentation();
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
      finalize(reason);
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
      if (!playerApi || typeof playerApi.createStartupPlayer !== "function" || !manifest || !canvas) {
        finalize("resource-error");
        return null;
      }
      installListeners();
      const reducedMotion = options.reducedMotion !== undefined
        ? options.reducedMotion === true
        : Boolean(windowRef?.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches);
      try {
        instance = playerApi.createStartupPlayer({
          manifest: { ...manifest, completeEvent: INTERNAL_EVENT },
          documentRef,
          windowRef,
          canvas,
          assets: options.assets,
          reducedMotion,
          reducedMotionDelayMs: 300,
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
