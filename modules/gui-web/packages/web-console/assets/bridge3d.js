// ===========================================================================
// COOLZHU 窗口滑动切换引擎（bridge3d.js v2）
// 修复 v1 BUG：MutationObserver 监听 is-active 会被 app.js 周期性 class 重写反复触发，
// 导致窗口不停翻转。v2 改为「用户动作驱动」：仅在点击页眉标签 / 长按拖动释放时
// 播一次滑动动画（translateX 滑入 + 轻透视，无大角度翻转）。
// 飞船控制室 Three.js 场景已回退；后续按 docs/plans/spaceship-bridge-assets-prompts.md
// 生成图片组件后再重新组装。
// ===========================================================================
(() => {
  const stage = document.querySelector(".workbench-stage");
  const tabs = Array.from(document.querySelectorAll(".window-dock [data-window-target]"));
  if (!stage || !tabs.length) return;

  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function activeIndex() {
    return tabs.findIndex((tab) => tab.classList.contains("is-active"));
  }
  function activeWindow() {
    return stage.querySelector(".workbench-window.is-active");
  }
  function tabLabel(tab) {
    return tab?.querySelector("span")?.textContent || tab?.title || "";
  }
  function neighbor(offset) {
    const index = activeIndex();
    const base = index < 0 ? 0 : index;
    return tabs[(base + offset + tabs.length) % tabs.length];
  }

  // ---- 一次性滑动进场：仅由用户动作显式调用，不观察 class（杜绝反复触发） ----
  function playSlideIn(direction) {
    if (reduced) return;
    requestAnimationFrame(() => {
      const win = activeWindow();
      if (!win || win.dataset.slideAnimating === "1") return;
      win.dataset.slideAnimating = "1";
      const cls = direction < 0 ? "win-slide-from-left" : "win-slide-from-right";
      win.classList.remove("win-slide-from-left", "win-slide-from-right");
      void win.offsetWidth;
      win.classList.add(cls);
      const clear = () => {
        win.classList.remove("win-slide-from-left", "win-slide-from-right");
        delete win.dataset.slideAnimating;
      };
      win.addEventListener("animationend", clear, { once: true });
      setTimeout(clear, 800); // 兜底
    });
  }

  // 点击页眉标签：按标签顺序差决定滑入方向（点左边的窗口 → 从左滑入）。
  let lastActiveIndex = activeIndex();
  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => {
      const direction = lastActiveIndex >= 0 && index < lastActiveIndex ? -1 : 1;
      lastActiveIndex = index;
      playSlideIn(direction);
    });
  });

  // ---- 长按拖动切换（左键按住 320ms 后水平拖动，释放切换相邻窗口） ----
  const HOLD_MS = 320;
  const CANCEL_MOVE = 10;
  const TRIGGER_DX = 90;
  let holdTimer = null;
  let holdStart = null;
  let grabbing = false;
  let grabDx = 0;

  const hint = document.createElement("div");
  hint.id = "win3dHint";
  hint.innerHTML = '<span class="hint-prev"></span><span class="hint-cur"></span><span class="hint-next"></span>';
  document.body.appendChild(hint);

  function interactiveTarget(target) {
    return target.closest(
      "input, textarea, select, button, a, [contenteditable], iframe, video, audio, canvas, .window-tab, [data-action], [role='tab'], li, label"
    );
  }

  function beginGrab(x) {
    grabbing = true;
    grabDx = 0;
    stage.classList.add("win3d-grab");
    document.body.classList.add("win3d-grabbing");
    hint.querySelector(".hint-prev").textContent = "◀ " + tabLabel(neighbor(-1));
    hint.querySelector(".hint-cur").textContent = tabLabel(tabs[activeIndex()] || tabs[0]);
    hint.querySelector(".hint-next").textContent = tabLabel(neighbor(1)) + " ▶";
    hint.classList.add("is-visible");
    holdStart = { x };
  }

  function applyGrab(dx) {
    grabDx = dx;
    const win = activeWindow();
    if (!win) return;
    const clamped = Math.max(-300, Math.min(300, dx));
    // 滑动语义：主要是水平位移 + 极轻透视（无翻转感）。
    const rot = clamped * 0.016;
    win.style.transform = `perspective(1600px) translateX(${clamped * 0.72}px) rotateY(${rot}deg)`;
    win.style.opacity = String(1 - Math.abs(clamped) / 900);
    hint.classList.toggle("lean-left", dx < -30);
    hint.classList.toggle("lean-right", dx > 30);
  }

  function endGrab(commit) {
    const win = activeWindow();
    stage.classList.remove("win3d-grab");
    document.body.classList.remove("win3d-grabbing");
    hint.classList.remove("is-visible", "lean-left", "lean-right");
    if (win) {
      win.style.transition = "transform 220ms cubic-bezier(.22,1,.36,1), opacity 220ms ease";
      win.style.transform = "";
      win.style.opacity = "";
      setTimeout(() => { if (win) win.style.transition = ""; }, 260);
    }
    if (commit && Math.abs(grabDx) >= TRIGGER_DX) {
      // 向左拖 = 下一个窗口（内容左移让位），向右拖 = 上一个。
      neighbor(grabDx < 0 ? 1 : -1)?.click();
    }
    grabbing = false;
    holdStart = null;
    grabDx = 0;
  }

  stage.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || interactiveTarget(event.target)) return;
    holdStart = { x: event.clientX, y: event.clientY };
    holdTimer = setTimeout(() => beginGrab(holdStart.x), HOLD_MS);
  });

  window.addEventListener("mousemove", (event) => {
    if (grabbing) {
      event.preventDefault();
      applyGrab(event.clientX - holdStart.x);
      return;
    }
    if (holdTimer && holdStart) {
      const moved = Math.hypot(event.clientX - holdStart.x, event.clientY - holdStart.y);
      if (moved > CANCEL_MOVE) {
        clearTimeout(holdTimer);
        holdTimer = null;
        holdStart = null;
      }
    }
  }, { passive: false });

  window.addEventListener("mouseup", () => {
    if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
    if (grabbing) endGrab(true);
    else holdStart = null;
  });
  window.addEventListener("blur", () => {
    if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
    if (grabbing) endGrab(false);
  });
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && grabbing) endGrab(false);
  });
})();
