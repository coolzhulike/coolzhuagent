/* 插件页与模型设置共用 Devin 官方授权；不收集账号或令牌。 */
(() => {
  "use strict";
  function mount(container, options = {}) {
    if (!container) return null;
    container.__devinAuth?.destroy();
    container.innerHTML = `
      <p data-devin-auth="status" role="status" aria-live="polite">正在检查 Devin 登录状态…</p>
      <p data-devin-auth="detail" class="ms-hint">在官方网页完成授权后，此处会自动更新。登录不会创建任务。</p>
      <div class="ms-discovery-actions">
        <button type="button" data-devin-auth="login" class="ms-secondary" data-icon-only="true" aria-label="登录 Devin" title="登录 Devin"><img class="wuxia-icon-only" src="./assets/icons-wuxia/link.svg" alt="" aria-hidden="true" /></button>
        <button type="button" data-devin-auth="cancel" class="ms-secondary" data-icon-only="true" aria-label="取消登录" title="取消登录" hidden><img class="wuxia-icon-only" src="./assets/icons-wuxia/stop.svg" alt="" aria-hidden="true" /></button>
        <button type="button" data-devin-auth="refresh" class="ms-secondary" data-icon-only="true" aria-label="刷新 Devin 登录状态" title="刷新 Devin 登录状态"><img class="wuxia-icon-only" src="./assets/icons-wuxia/refresh.svg" alt="" aria-hidden="true" /></button>
      </div>`;
    const el = key => container.querySelector(`[data-devin-auth="${key}"]`);
    let disposed = false, visible = options.visible !== false, busy = false, sequence = 0;
    let timer = null, controller = null, snapshot = null, previousAuthentication = null;
    function stopPoll() { if (timer != null) clearTimeout(timer); timer = null; }
    function controls() {
      el("login").disabled = busy || !snapshot?.cli_available || snapshot?.login?.active === true || snapshot?.authentication === "authenticated";
      el("cancel").hidden = snapshot?.login?.active !== true;
      el("cancel").disabled = busy || ["cancelling", "cleanup_failed"].includes(snapshot?.login?.phase);
      el("refresh").disabled = busy;
    }
    function render(value) {
      snapshot = value;
      const labels = {authenticated:"Devin 已登录",unauthenticated:"Devin 尚未登录",unavailable:"未找到 Devin 登录组件",unknown:"Devin 登录状态尚未确认",check_failed:"暂时无法核对 Devin 登录状态"};
      el("status").textContent = value.login?.active
        ? (value.login.phase === "cancelling" ? "正在取消 Devin 登录…" : value.login.phase === "cleanup_failed" ? value.login.message : "正在登录 Devin…")
        : value.message || labels[value.authentication] || labels.unknown;
      el("detail").textContent = value.login?.message || "在官方网页完成授权后，此处会自动更新。登录不会创建任务。";
      controls(); stopPoll();
      if (visible && value.login?.active && value.login.phase !== "cleanup_failed") timer = setTimeout(() => { void refresh(); }, 2000);
      if (value.authentication === "authenticated" && previousAuthentication !== "authenticated") options.onAuthenticated?.();
      if (previousAuthentication != null && value.authentication !== previousAuthentication) {
        window.dispatchEvent(new CustomEvent("devin-account-changed", {detail:container}));
      }
      previousAuthentication = value.authentication;
    }
    async function send(path, body) {
      stopPoll(); controller?.abort(); controller = new AbortController();
      const requestController = controller;
      const current = ++sequence, signal = requestController.signal;
      const requestTimeout = setTimeout(() => requestController.abort(), 25000);
      busy = true; controls();
      try {
        const response = await fetch(path, body === undefined ? {signal} : {
          method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify(body), signal,
        });
        const result = await response.json();
        if (disposed || current !== sequence || !visible) return;
        if (!response.ok) throw new Error(result.error?.message || result.error || "Devin 登录请求失败，请重试。");
        render(result);
        if (body !== undefined) window.dispatchEvent(new CustomEvent("devin-account-changed", {detail:container}));
      } catch (error) {
        if (disposed || current !== sequence || !visible) return;
        el("status").textContent = error?.name === "AbortError" ? "登录状态查询超时，请刷新重试。" : error?.message || "Devin 登录服务暂时不可用。";
        // 传输失败不能证明后台授权已停止；按钮保持最后确认的流程身份。
        if (snapshot?.login?.active) timer = setTimeout(() => { void refresh(); }, 3000);
      } finally {
        clearTimeout(requestTimeout);
        if (!disposed && current === sequence) { busy = false; controls(); }
      }
    }
    const refresh = () => visible && !disposed ? send("/api/backends/devin/auth") : Promise.resolve();
    const login = () => send("/api/backends/devin/auth/login", {});
    const cancel = () => snapshot?.login?.attempt_id ? send("/api/backends/devin/auth/cancel", {attempt_id:snapshot.login.attempt_id}) : refresh();
    const peerRefresh = event => { if (event.detail !== container && visible && !busy) void refresh(); };
    window.addEventListener("devin-account-changed", peerRefresh);
    el("login").addEventListener("click", login);
    el("cancel").addEventListener("click", cancel);
    el("refresh").addEventListener("click", refresh);
    const api = {
      refresh,
      setVisible(value) {
        visible = !!value; container.hidden = !visible;
        if (visible) void refresh();
        else { stopPoll(); sequence++; controller?.abort(); busy = false; }
      },
      destroy() {
        disposed = true; sequence++; stopPoll(); controller?.abort();
        el("login").removeEventListener("click", login); el("cancel").removeEventListener("click", cancel); el("refresh").removeEventListener("click", refresh);
        window.removeEventListener("devin-account-changed", peerRefresh);
        // 关闭页面只停止页面轮询，不代替用户取消本机登录。
      },
    };
    container.__devinAuth = api;
    container.hidden = !visible; controls();
    if (visible) void refresh();
    return api;
  }
  window.CoolzhuDevinAuth = Object.freeze({mount});
  function mountPluginPanel() {
    const panel = document.querySelector('[data-role="devin-account-panel"]');
    if (panel) mount(panel);
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mountPluginPanel, {once:true});
  else mountPluginPanel();
})();
