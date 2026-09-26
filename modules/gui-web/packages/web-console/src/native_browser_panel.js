/* 桌面右栏浏览器：网页驻留于无宿主权限的独立 WebView，DOM 只负责位置与导航。 */
window.CoolzhuNativeBrowserPanel = (() => {
  let adapter, current = null, serial = Promise.resolve(), generation = 0, frameRequested = false;
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
    const rect = frame.getBoundingClientRect();
    if (rect.width < 100 || rect.height < 80) return null;
    const overlays = document.querySelectorAll('[role="dialog"],dialog[open],.tool-detail-modal');
    if ([...overlays].some(node => node.getClientRects().length && getComputedStyle(node).visibility !== "hidden")) return null;
    const convert = value => ({x:Math.round(value.x*scale), y:Math.round(value.y*scale), width:Math.round(value.width*scale), height:Math.round(value.height*scale)});
    const host = frame.closest(".chat-right-rail");
    return {bounds:convert(rect),host_bounds:host ? convert(host.getBoundingClientRect()) : undefined};
  }
  function display(state) {
    if (!current || state.scope !== current.scope) return;
    current.mounted = state.active;
    if (state.url) { current.url = state.url; adapter.url(state.url); }
    if (state.error) adapter.status(`网页载入失败：${state.error}`);
    else if (state.reason === "window-resized") { current.mounted = false; scheduleLayout(); }
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
        if (target.mounted) { await send({action:"close",scope:target.scope}); target.mounted = false; }
        return;
      }
      const key = JSON.stringify(geometry);
      if (target.mounted && target.boundsKey === key) return;
      const state = await send({action:target.mounted ? "resize" : "navigate",scope:target.scope,url:target.url,...geometry});
      if (current !== target) return;
      target.boundsKey = key; display(state);
    });
  }
  function init(api) {
    if (adapter) return; adapter = api;
    if (!invoke()) return;
    window.__TAURI__?.event?.listen("browser-panel-state", event => display(event.payload)).catch(error => adapter.status(`浏览器状态监听不可用：${error.message}`));
    const frame = adapter.frame(); if (frame) new ResizeObserver(scheduleLayout).observe(frame);
    new MutationObserver(scheduleLayout).observe(document.body, {subtree:true,childList:true,attributes:true,attributeFilter:["hidden","class","open","style"]});
    window.addEventListener("resize", scheduleLayout);
    window.addEventListener("beforeunload", () => { if (current) void send({action:"close",scope:current.scope}); });
  }
  async function close() {
    const previous = current; current = null; generation++;
    if (!previous || !invoke()) return;
    await enqueue(() => send({action:"close",scope:previous.scope}));
  }
  async function navigate(url) {
    if (!invoke()) throw new Error("当前窗口不支持桌面浏览器");
    await close();
    const scope = JSON.stringify({context:adapter.scope(),generation:++generation});
    current = {scope,url,mounted:false,boundsKey:null};
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
  return {init,navigate,close,available:() => Boolean(invoke()),back:()=>action("back"),forward:()=>action("forward"),reload:()=>action("reload"),stop:()=>action("stop")};
})();
