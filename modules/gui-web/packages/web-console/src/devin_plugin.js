// 云会话独立于模型配置；不保存密钥到浏览器，不自动创建、重发或轮询。
(() => {
  // 聊天室后端复用统一参数页；云任务 API 保持独立，不能冒充可选模型的聊天 Provider。
  const modelsPanel = document.querySelector('[data-role="devin-model-panel"]');
  let modelsEditor = null;
  modelsPanel?.addEventListener("toggle", () => {
    if (modelsPanel.open && !modelsEditor) modelsEditor = window.CoolzhuModelSettings?.mount(
      modelsPanel.querySelector('[data-role="devin-model-settings"]'),
      { defaultProtocol:"devin_acp", backendFilter:"devin_acp", onSaved:async () => { await loadSessions(); await loadAgents(); } },
    );
    else if (modelsPanel.open) void modelsEditor?.refresh();
  });
  window.addEventListener("coolzhu-workspace-changed", () => {
    modelsEditor?.destroy(); modelsEditor = null;
    modelsPanel?.querySelector('[data-role="devin-model-settings"]')?.replaceChildren();
    if (modelsPanel?.open) modelsPanel.dispatchEvent(new Event("toggle"));
  });
  const host = document.querySelector('[data-role="devin-plugin-panel"]');
  if (!host) return;
  const field = name => host.querySelector(`[data-role="devin-${name}"]`);
  const button = name => host.querySelector(`[data-action="devin-${name}"]`);
  const state = { config: null, busy: false, scope: null, serial: 0, pending: new Map(), controller: null, cursor: "", cursorSession: "" };
  const scope = () => typeof activeWorkspaceKey === "string" ? activeWorkspaceKey : "";
  const status = text => { field("status").textContent = text; };
  const setBusy = value => {
    state.busy = value;
    host.querySelectorAll("button, input, select, textarea").forEach(node => { node.disabled = value; });
    button("next").disabled = value || !state.cursor || state.cursorSession !== field("session").value;
  };
  async function request(url, body) {
    const controller = new AbortController(); state.controller = controller;
    const timer = setTimeout(() => controller.abort(), 32000);
    try { return await requestJson(url, body ? { method:"POST", body:JSON.stringify(body), signal:controller.signal } : { signal:controller.signal }); }
    finally { clearTimeout(timer); if (state.controller === controller) state.controller = null; }
  }
  function clearScope() {
    state.controller?.abort(); state.serial++; state.config = null; state.pending.clear();
    state.cursor = ""; state.cursorSession = "";
    state.scope = scope(); field("key").value = ""; field("org").value = ""; field("text").value = "";
    field("bind-id").value = ""; field("result").textContent = ""; field("output").replaceChildren();
    field("session").replaceChildren(new Option("尚未选择", "")); setBusy(false);
  }
  function render(config, selected = field("session").value) {
    state.config = config; state.scope = scope();
    field("enabled").checked = config.enabled; field("org").value = config.org_id;
    field("mode").value = config.mode; field("key").value = "";
    field("key").placeholder = config.credential_state === "placeholder" ? "随机占位值等待本人替换" : "已保存密钥（尚未验证）；留空保留";
    field("session").replaceChildren(new Option("选择云会话", ""));
    for (const session of config.sessions || []) {
      field("session").add(new Option(`${session.title || session.session_id} · ${session.status || "未知"}`, session.session_id));
    }
    if ([...field("session").options].some(option => option.value === selected)) field("session").value = selected;
    status(config.credential_state === "placeholder" ? "随机占位密钥已生成，请由本人填写真实 API Key 与组织 ID。" : "配置已读取；真实密钥有效性与云会话请由本人测试。");
  }
  async function load() {
    if (state.busy) return;
    if (state.scope !== scope()) clearScope();
    const key = scope(), serial = ++state.serial; setBusy(true);
    try { const config = await request("/api/devin/config"); if (key === scope() && serial === state.serial) render(config); }
    catch (error) { if (key === scope() && serial === state.serial) status(`Devin 配置读取失败：${error.message}`); }
    finally { if (serial === state.serial) setBusy(false); }
  }
  field("config-form").addEventListener("submit", async event => {
    event.preventDefault(); if (state.busy || !state.config) return;
    const key = scope(), serial = ++state.serial; setBusy(true);
    try {
      const config = await request("/api/devin/config", { expected_workspace:state.config.workspace_id,
        revision:state.config.revision, enabled:field("enabled").checked, org_id:field("org").value,
        mode:field("mode").value, api_key:field("key").value });
      if (key === scope() && serial === state.serial) { render(config); state.pending.clear(); }
    } catch (error) { if (key === scope() && serial === state.serial) status(`保存失败：${error.message}`); }
    finally { field("key").value = ""; if (serial === state.serial) setBusy(false); }
  });
  function display(value) {
    const container = field("output"); container.replaceChildren();
    const pre = document.createElement("pre"); pre.textContent = JSON.stringify(value, null, 2); container.append(pre);
  }
  async function act(action, after = "") {
    if (state.busy || !state.config) return;
    if (state.scope !== scope()) { clearScope(); await load(); return; }
    const key = scope(), serial = ++state.serial;
    const payload = { expected_workspace:state.config.workspace_id, revision:state.config.revision,
      action, session_id:action === "bind" ? field("bind-id").value.trim() : field("session").value,
      text:["create", "send"].includes(action) ? field("text").value : "", after };
    if (["create", "send"].includes(action)) {
      // 相同内容在未知结果时沿用编号，避免重复点击造成第二次收费创建。
      const fingerprint = JSON.stringify(payload);
      if (!state.pending.has(fingerprint)) state.pending.set(fingerprint, crypto.randomUUID());
      payload.operation_id = state.pending.get(fingerprint);
    }
    setBusy(true); field("result").textContent = "等待 Devin API 返回…";
    try {
      const result = await request("/api/devin/action", payload);
      if (key !== scope() || serial !== state.serial) return;
      display(result);
      if (action === "messages") {
        const page = result.messages;
        state.cursor = page?.has_next_page && typeof page.end_cursor === "string" ? page.end_cursor : "";
        state.cursorSession = payload.session_id;
      }
      field("result").textContent = action === "messages" ? `已读取一页消息${state.cursor ? "，可点击下一页。" : "，已到最后一页。"}` : `请求已返回 · 远端状态 ${result.status || "未知"} / ${result.status_detail || "—"}。`;
      if (result.session_id && action !== "messages") {
        const config = await request("/api/devin/config");
        if (key === scope() && serial === state.serial) render(config, result.session_id);
      }
    } catch (error) {
      if (key === scope() && serial === state.serial) field("result").textContent = `请求未确认成功：${error.message}。写操作不会自动重发；结果未知时请到官网核对。`;
    } finally { if (serial === state.serial) setBusy(false); }
  }
  for (const action of ["create", "send", "get", "messages", "bind"]) button(action).addEventListener("click", () => act(action));
  button("next").addEventListener("click", () => { if (state.cursor && state.cursorSession === field("session").value) act("messages", state.cursor); });
  field("session").addEventListener("change", () => { state.cursor = ""; state.cursorSession = ""; field("output").replaceChildren(); field("result").textContent = ""; setBusy(state.busy); });
  button("refresh").addEventListener("click", load);
  button("open").addEventListener("click", async () => {
    if (state.scope !== scope()) { clearScope(); return; }
    const id = field("session").value;
    if (!/^devin-[A-Za-z0-9_-]+$/.test(id)) return;
    openChatToolWindow("browser");
    const address = document.querySelector('[data-role="browser-window-input"]');
    if (!address) return;
    address.value = `https://app.devin.ai/sessions/${id}`; await browserWindowNavigate();
  });
  host.addEventListener("toggle", () => { if (host.open) load(); });
  window.addEventListener("dsh-market-refresh", () => { if (state.scope !== scope()) { clearScope(); if (host.open) load(); } });
  window.addEventListener("coolzhu-workspace-changed", () => { clearScope(); status("工程已切换，请刷新当前工程配置。"); if (host.open) load(); });
})();
