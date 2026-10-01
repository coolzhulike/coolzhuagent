/* 桌面右栏浏览器：网页驻留于无宿主权限的独立 WebView，DOM 只负责位置与导航。 */
window.CoolzhuNativeBrowserPanel = (() => {
  let adapter, current = null, closedTarget = null, serial = Promise.resolve(), generation = 0, frameRequested = false;
  const invoke = () => window.__TAURI__?.core?.invoke;
  const enqueue = operation => { const next = serial.then(operation); serial = next.catch(() => {}); return next; };
  const send = command => invoke()("browser_panel_command", {command});
  async function displayScale(scope) {
    let factor;
    try {
      const nativeWindow = window.__TAURI__?.window?.getCurrentWindow?.();
      if (nativeWindow?.scaleFactor) factor = await nativeWindow.scaleFactor();
    } catch (_) { /* 未授予窗口查询时，仅向可信桌面命令查询比例。 */ }
    if (!Number.isFinite(factor) || factor <= 0) factor = (await send({action:"metrics",scope})).window_scale_factor;
    const ratio = (window.devicePixelRatio || 1) / factor;
    if (!Number.isFinite(ratio) || ratio <= 0) throw new Error("无法确认网页显示比例");
    return ratio;
  }
  function visibleBounds(scale) {
    const frame = adapter.frame();
    if (!frame || !frame.getClientRects().length || !adapter.active()) return null;
    const host = frame.closest(".chat-right-rail");
    if (host) {
      const panel = host.closest(".chat-window-panel");
      const style = getComputedStyle(host);
      // 折叠栏仍有布局矩形；原生子视图不能依据矩形继续覆盖主聊天。
      if (panel?.dataset.rightCollapsed === "true" || host.getAttribute("aria-hidden") === "true"
        || host.inert || host.hasAttribute("inert") || style.visibility !== "visible" || style.display === "none") return null;
    }
    const rect = frame.getBoundingClientRect();
    if (rect.width < 100 || rect.height < 80) return null;
    const overlays = document.querySelectorAll('[role="dialog"],dialog[open],.tool-detail-modal');
    if ([...overlays].some(node => node.getClientRects().length && getComputedStyle(node).visibility !== "hidden")) return null;
    const convert = value => ({x:Math.round(value.x*scale), y:Math.round(value.y*scale), width:Math.round(value.width*scale), height:Math.round(value.height*scale)});
    return {bounds:convert(rect),host_bounds:host ? convert(host.getBoundingClientRect()) : undefined};
  }
  function display(state) {
    if (!current || state.scope !== current.scope) return;
    current.mounted = state.active;
    current.hidden = state.hidden === true;
    if (state.url && !current.needsNavigation) { current.url = state.url; adapter.url(state.url); }
    if (state.error) adapter.status(`网页载入失败：${state.error}`);
    else if (state.reason === "window-resized") scheduleLayout();
    else if (state.active) adapter.status(state.loading ? `正在载入 ${state.url}` : state.title || state.url);
  }
  function scheduleLayout() {
    if (frameRequested || !current) return;
    frameRequested = true;
    requestAnimationFrame(() => { frameRequested = false; void synchronize().catch(error => adapter.status(`浏览器布局更新失败：${error.message}`)); });
  }
  async function synchronize() {
    const target = current; if (!target) return;
    return enqueue(async () => {
      if (current !== target) return;
      const scale = await displayScale(target.scope);
      if (current !== target) return;
      const geometry = visibleBounds(scale);
      if (!geometry) {
        if (target.mounted && !target.hidden) {
          display(await send({action:"hide",scope:target.scope}));
        }
        return;
      }
      const key = JSON.stringify(geometry);
      if (target.mounted && !target.hidden && target.boundsKey === key && !target.needsNavigation) return;
      const navigate = !target.mounted || target.needsNavigation;
      const requestedUrl = target.url;
      const revision = target.navigationRevision;
      const state = await send({action:navigate ? "navigate" : "resize",scope:target.scope,url:requestedUrl,...geometry});
      if (current !== target) return;
      if (revision !== target.navigationRevision) return;
      target.boundsKey = key; target.needsNavigation = false; display(state);
    });
  }
  function init(api) {
    if (adapter) return; adapter = api;
    if (!invoke()) return;
    window.__TAURI__?.event?.listen("browser-panel-state", event => display(event.payload)).catch(error => adapter.status(`浏览器状态监听不可用：${error.message}`));
    const frame = adapter.frame(); if (frame) new ResizeObserver(scheduleLayout).observe(frame);
    new MutationObserver(scheduleLayout).observe(document.body, {subtree:true,childList:true,attributes:true,attributeFilter:["hidden","class","open","style","data-right-collapsed","aria-hidden","inert"]});
    window.addEventListener("resize", scheduleLayout);
    window.addEventListener("beforeunload", () => { if (current) void send({action:"close",scope:current.scope}); });
  }
  async function close() {
    const previous = current; current = null; generation++;
    // 显式关闭销毁原生资源；仅保留同工程、同聊天室再次打开时的地址。
    if (previous) closedTarget = {contextKey:previous.contextKey,url:previous.url};
    if (!previous || !invoke()) return;
    await enqueue(() => send({action:"close",scope:previous.scope}));
  }
  async function suspend() {
    const target = current;
    if (!target || !invoke()) return;
    await enqueue(async () => {
      if (current !== target || !target.mounted || target.hidden) return;
      const state = await send({action:"hide",scope:target.scope});
      if (current === target) display(state);
    });
  }
  function resume() {
    const contextKey = JSON.stringify(adapter.scope());
    if (current?.contextKey === contextKey) {
      scheduleLayout();
    } else if (!current && closedTarget?.contextKey === contextKey) {
      // 只响应打开浏览器的界面操作；新视图重新登记资格，不续发旧输入。
      void navigate(closedTarget.url).catch(error => adapter.status(`浏览器打开失败：${error.message}`));
    }
  }
  async function navigate(url) {
    if (!invoke()) throw new Error("当前窗口不支持桌面浏览器");
    closedTarget = null;
    const contextKey = JSON.stringify(adapter.scope());
    if (current?.contextKey === contextKey) {
      current.url = url;
      current.needsNavigation = true;
      current.navigationRevision++;
      await synchronize();
      return {opened:"nativePanel"};
    }
    const previous = current;
    current = null;
    const requestGeneration = ++generation;
    if (previous) await enqueue(() => send({action:"close",scope:previous.scope}));
    // 首次或跨工程连续导航时，仅最后一次请求能创建新的原生视图。
    if (requestGeneration !== generation) return {opened:"nativePanel"};
    const scope = JSON.stringify({context:adapter.scope(),generation:requestGeneration});
    current = {scope,contextKey,url,mounted:false,hidden:false,boundsKey:null,needsNavigation:true,navigationRevision:1};
    const frame = adapter.frame(); if (frame) frame.src = "about:blank";
    await synchronize();
    return {opened:"nativePanel"};
  }
  async function action(action) {
    const target = current; if (!target) return;
    return enqueue(async () => {
      if (current !== target || !target.mounted) return;
      display(await send({action,scope:target.scope}));
    });
  }
  return {init,navigate,close,suspend,resume,available:() => Boolean(invoke()),back:()=>action("back"),forward:()=>action("forward"),reload:()=>action("reload"),stop:()=>action("stop")};
})();
