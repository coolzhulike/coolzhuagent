/* 统一会话参数页：模型目录只提供提示，用户可直接配置新模型。 */
(() => {
  "use strict";
  const esc = value => String(value ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const option = (value, label) => `<option value="${esc(value)}">${esc(label)}</option>`;
  const triOptions = `${option("", "沿用工程设置")}${option("true", "开启")}${option("false", "关闭")}`;
  const efforts = [["auto", "自动"], ["none", "关闭"], ["minimal", "极简"], ["low", "低"], ["medium", "中"], ["high", "高"], ["xhigh", "超高"], ["max", "最大"]];
  async function request(path, body, method) {
    const response = await fetch(path, body === undefined ? {} : {
      method: method || "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
    });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(value.error?.message || value.error || value.message || `请求失败 (${response.status})`);
    return value;
  }

  function mount(container, options = {}) {
    if (typeof container === "string") container = document.querySelector(container);
    if (!container) return null;
    container.__modelSettings?.destroy();
    let sessions = [], selectedId = options.sessionId || null, snapshot = null, loadSequence = 0, disposed = false;
    let busy = false, dirty = false;
    container.classList.add("model-settings");
    container.innerHTML = `
      <div class="ms-intro"><div><span class="ms-eyebrow">MODEL & SESSION</span><h2>模型与会话</h2><p>连接、思考、上下文和工具，在一个页面中配置。</p></div><button type="button" data-ms-action="new" class="ms-secondary">新建会话</button></div>
      <label class="ms-session-picker">编辑会话<select data-ms="session-select" aria-label="选择要配置的会话"></select></label>
      <form data-ms="form" autocomplete="off">
        <fieldset data-ms="fields">
          <section class="ms-section"><div class="ms-section-title"><span>01</span><h3>连接</h3></div>
            <div class="ms-grid">
              <label>会话名称<input data-ms="name" required maxlength="32" placeholder="例如：日常助手"></label>
              <label>通信协议<select data-ms="protocol"><option value="openai_chat_completions">OpenAI Chat Completions</option><option value="anthropic_messages">Anthropic Messages</option></select></label>
              <label class="ms-wide">模型 ID<input data-ms="model" required maxlength="1024" spellcheck="false" placeholder="填写服务端接受的完整模型 ID"><small>支持直接填写新模型及本地模型路径，无需修改供应商配置表。</small></label>
              <label class="ms-wide">接口地址 · Base URL<input data-ms="base-url" type="url" required spellcheck="false" placeholder="https://api.example.com/v1"></label>
              <label class="ms-wide">请求路径 · Endpoint <span class="ms-optional">可选</span><input data-ms="endpoint" spellcheck="false" placeholder="留空按协议补齐，也可填写完整请求地址"></label>
              <label class="ms-wide">API Key<input data-ms="api-key" type="password" autocomplete="new-password" spellcheck="false" placeholder="本地服务可留空"><small data-ms="key-hint">密钥不会回显；已有密钥在留空保存时保留。</small></label>
              <label>模型用途<select data-ms="model-type"><option value="text">对话 / 代码</option><option value="vision">图像理解</option><option value="multimodal">多模态</option><option value="image">图像生成</option><option value="audio">音频</option><option value="video">视频</option><option value="embedding">文本向量</option></select></label>
              <label class="ms-checkbox"><input type="checkbox" data-ms="clear-key">清除该会话保存的密钥</label>
              <label class="ms-wide">图片输入能力<select data-ms="supports-multimodal"><option value="">沿用模型能力</option><option value="true">支持图片直传</option><option value="false">纯文本 · 由视觉 Agent 描述</option></select><small data-ms="multimodal-hint"></small></label>
            </div>
          </section>
          <section class="ms-section"><div class="ms-section-title"><span>02</span><h3>上下文与输出</h3></div>
            <div class="ms-grid">
              <label>上下文容量 · tokens<input data-ms="context" type="number" min="1" max="4000000" step="1" placeholder="沿用模型默认值"></label>
              <label>最大输出 · tokens<input data-ms="output" type="number" min="1" max="1000000" step="1" placeholder="沿用模型默认值"></label>
            </div><p class="ms-hint" data-ms="effective-limit">留空使用模型默认值；本地模型也受服务启动容量限制。</p>
          </section>
          <section class="ms-section"><div class="ms-section-title"><span>03</span><h3>思考与采样</h3></div>
            <div class="ms-grid">
              <label>思考参数模式<select data-ms="reasoning-mode"></select></label>
              <label data-ms="effort-label">思考层级<select data-ms="reasoning-effort">${efforts.map(item => option(...item)).join("")}</select></label>
              <label class="ms-wide" data-ms="budget-label" hidden>思考预算 · tokens<input data-ms="thinking-budget" type="number" min="1024" max="999999" step="1" placeholder="例如 4096"></label>
              <label>温度 · Temperature<input data-ms="temperature" type="number" min="0" max="2" step="0.01" placeholder="模型默认"></label>
              <label>采样范围 · Top P<input data-ms="top-p" type="number" min="0.01" max="1" step="0.01" placeholder="模型默认"></label>
            </div><p class="ms-hint" data-ms="reasoning-hint"></p>
          </section>
          <section class="ms-section"><div class="ms-section-title"><span>04</span><h3>工具能力</h3></div>
            <div class="ms-grid">
              <label>模型工具调用<select data-ms="enable-tools">${triOptions}</select></label>
              <label>电脑操作<select data-ms="computer-use">${triOptions}</select></label>
              <label class="ms-wide">工具暴露范围<select data-ms="tool-exposure"><option value="">沿用工程设置</option><option value="whitelist">权限允许的工具 / 自定义清单</option><option value="all">全部已注册且权限允许的工具</option><option value="dispatch-only">仅语义调度入口</option></select></label>
              <label class="ms-wide" data-ms="allowlist-label" hidden>允许的工具名称<textarea data-ms="tool-allowlist" rows="3" placeholder="每行一个工具名称；留空沿用权限范围"></textarea></label>
            </div><p class="ms-hint">会话参数决定模型能看到哪些工具；执行仍受当前工程权限约束。</p>
          </section>
        </fieldset>
        <div class="ms-footer"><p data-ms="status" role="status" aria-live="polite">正在读取配置…</p><button type="submit" data-ms="save" class="ms-primary">保存配置</button></div>
      </form>`;
    const el = key => container.querySelector(`[data-ms="${key}"]`);
    const value = key => el(key).value.trim();
    const set = (key, data) => { el(key).value = data ?? ""; };
    const tri = key => value(key) === "" ? null : value(key) === "true";
    const number = key => value(key) === "" ? null : Number(value(key));
    const status = (message, error = false) => { el("status").textContent = message; el("status").classList.toggle("ms-error", error); };

    function updateDynamicFields() {
      const anthropic = value("protocol") === "anthropic_messages";
      // 与适配器的已核验模型清单一致；未知后缀、Omni 和 Anthropic 不套用此映射。
      const qwen38 = !anthropic && ["qwen3.8-flash", "qwen3.8-max", "qwen3.8-max-0902", "qwen3.8-2.4t-a95b", "qwen3.8-27b"].includes(value("model").toLowerCase());
      const current = value("reasoning-mode") || "auto";
      const modes = anthropic
        ? [["auto", "自动识别模型能力"], ["adaptive", "Adaptive 自适应思考"], ["budget", "指定思考预算"]]
        : [["auto", "自动识别模型能力"], ["effort", "Reasoning Effort 层级"], ["thinking", "Thinking 开关"]];
      el("reasoning-mode").innerHTML = modes.map(item => option(...item)).join("");
      set("reasoning-mode", modes.some(item => item[0] === current) ? current : "auto");
      const mode = value("reasoning-mode");
      const effort = value("reasoning-effort");
      const available = mode === "thinking" ? ["auto", "none", "high"]
        : mode === "adaptive" ? ["auto", "none", "low", "medium", "high", "max"]
          : mode === "budget" ? ["none", "high"] : efforts.map(item => item[0]);
      el("reasoning-effort").innerHTML = efforts.filter(item => available.includes(item[0])).map(([id, label]) => option(id, (mode === "thinking" || mode === "budget") && id === "high" ? "开启" : label)).join("");
      set("reasoning-effort", available.includes(effort) ? effort : (mode === "budget" ? "high" : "auto"));
      el("budget-label").hidden = mode !== "budget";
      el("thinking-budget").required = mode === "budget" && value("reasoning-effort") !== "none";
      el("temperature").max = anthropic ? "1" : "2";
      el("allowlist-label").hidden = value("tool-exposure") !== "whitelist";
      const hints = {
        auto: "沿用能力目录的参数映射；新模型或自定义端点可选择明确的参数模式。",
        effort: "将所选层级直接发送为 reasoning_effort；请使用当前模型实际支持的层级。自动表示不发送该字段。",
        thinking: "用 thinking.type 控制开启 / 关闭思考；自动表示沿用服务端默认。",
        adaptive: "使用 Anthropic 原生 adaptive thinking 与 output_config.effort；开启时将温度与 Top P 留空。",
        budget: "使用 Anthropic 原生 budget_tokens；预算至少 1024 且小于实际输出预算。开启时将采样参数留空。",
      };
      const qwenHints = {
        auto: "按 Qwen3.8 原生映射发送所选思考层级；自动沿用服务端默认，关闭发送 enable_thinking=false。",
        effort: "Qwen3.8 原生支持 low / medium / xhigh；minimal 映射为 low，high / max 映射为 xhigh。关闭发送 enable_thinking=false；自动沿用服务端默认。",
        thinking: "Qwen3.8 使用 enable_thinking 开关；关闭发送 false，开启使用 xhigh。此模式下“自动”也开启思考，但不指定层级。",
      };
      el("reasoning-hint").textContent = (qwen38 && qwenHints[mode]) || hints[mode];
      const multimodal = tri("supports-multimodal");
      const imageStrategy = snapshot?.image_input_strategy;
      const savedNative = imageStrategy === "native" || (imageStrategy == null && snapshot?.effective_supports_multimodal === true);
      const savedVision = imageStrategy === "vision-description" || (imageStrategy == null && snapshot?.effective_supports_multimodal === false);
      el("multimodal-hint").textContent = multimodal === true
        ? "图片直接发送给此模型；请确认所用模型及接口支持图片输入。"
        : multimodal === false
          ? "由视觉 Agent 先描述图片，再把文字交给此模型；缺少可用视觉 Agent 时会提示错误。"
          : `沿用模型用途和已识别能力。${savedNative ? "已保存配置当前使用图片直传。" : savedVision ? "已保存配置当前由视觉 Agent 描述图片。" : "保存后显示实际采用的处理方式。"}`;
    }

    function fill(data) {
      snapshot = data;
      const s = data?.session || {}, p = data?.parameters || {};
      set("name", s.name); set("model", s.model); set("model-type", s.model_type || "text");
      set("protocol", data?.protocol || "openai_chat_completions");
      set("base-url", data?.base_url || "http://127.0.0.1:11434/v1"); set("endpoint", data?.endpoint);
      set("api-key", ""); el("clear-key").checked = false;
      set("supports-multimodal", p.supports_multimodal == null ? "" : String(p.supports_multimodal));
      el("clear-key").disabled = !selectedId;
      el("api-key").placeholder = selectedId ? "留空保留已有密钥" : "本地服务可留空";
      el("key-hint").textContent = selectedId ? `密钥状态：${s.api_key_status || "未配置"}。留空不会覆盖已有密钥。` : "密钥不会回显；本地服务可留空。";
      set("context", p.context_window || ""); set("output", p.max_output_tokens || "");
      el("context").placeholder = data?.default_context_window ? `默认 ${data.default_context_window.toLocaleString()}` : "沿用模型默认值";
      el("output").placeholder = data?.default_max_output_tokens ? `默认 ${data.default_max_output_tokens.toLocaleString()}` : "沿用模型默认值";
      el("effective-limit").textContent = data ? `当前生效：上下文 ${Number(data.effective_context_window).toLocaleString()} · 本轮最大输出 ${Number(data.effective_max_output_tokens).toLocaleString()} tokens。输出受上下文预留与本地服务容量限制。` : "留空使用模型默认值；本地模型也受服务启动容量限制。";
      updateDynamicFields(); set("reasoning-mode", p.reasoning_mode || "auto");
      el("reasoning-effort").innerHTML = efforts.map(item => option(...item)).join("");
      set("reasoning-effort", s.reasoning_effort || "auto");
      set("thinking-budget", p.thinking_budget); set("temperature", p.temperature); set("top-p", p.top_p);
      set("enable-tools", p.enable_llm_tools == null ? "" : String(p.enable_llm_tools));
      set("computer-use", p.computer_use_enabled == null ? "" : String(p.computer_use_enabled));
      set("tool-exposure", p.llm_tool_exposure); set("tool-allowlist", (p.tool_allowlist || []).join("\n"));
      updateDynamicFields(); dirty = false;
      status(selectedId ? "配置已载入。修改后对下一轮会话生效。" : "填写连接信息后保存为新会话。");
    }

    function renderSessionOptions() {
      el("session-select").innerHTML = `${option("", "新会话")}${sessions.map(s => option(s.id, `${s.name} · ${s.model}`)).join("")}`;
      set("session-select", selectedId || "");
    }

    async function select(sessionId) {
      if (busy) return;
      selectedId = sessionId || null;
      renderSessionOptions();
      const sequence = ++loadSequence;
      if (!selectedId) { fill(null); el("fields").disabled = false; el("save").disabled = false; return; }
      el("fields").disabled = true; el("save").disabled = true;
      status("正在读取配置…");
      let loaded = false;
      try {
        const data = await request(`/api/sessions/${encodeURIComponent(selectedId)}/model-settings`);
        if (disposed || sequence !== loadSequence) return;
        fill(data);
        loaded = true;
      } catch (error) {
        if (sequence === loadSequence && !disposed) status(`读取失败：${error.message}`, true);
      } finally {
        if (sequence === loadSequence && !disposed) { el("fields").disabled = !loaded; el("save").disabled = !loaded; }
      }
    }

    async function refresh(sessionId = selectedId) {
      if (disposed) return;
      try {
        const registry = await request("/api/sessions");
        if (disposed) return;
        sessions = registry.sessions || [];
        await select(sessionId || registry.active_session_id || sessions[0]?.id || null);
      } catch (error) { if (!disposed) status(`读取会话失败：${error.message}`, true); }
    }

    async function save(event) {
      event.preventDefault();
      if (busy || !el("form").reportValidity()) return;
      const session = { name: value("name"), model: value("model"), model_type: value("model-type"), reasoning_effort: value("reasoning-effort") };
      if (value("api-key") || el("clear-key").checked) session.api_key_ref = el("clear-key").checked ? "" : value("api-key");
      const parameters = {
        protocol: value("protocol"), base_url: value("base-url"), endpoint: value("endpoint") || "",
        context_window: number("context") || 0, max_output_tokens: number("output") || 0,
        temperature: number("temperature"), top_p: number("top-p"),
        supports_multimodal: tri("supports-multimodal"),
        reasoning_mode: value("reasoning-mode"), thinking_budget: value("reasoning-mode") === "budget" ? number("thinking-budget") : null,
        enable_llm_tools: tri("enable-tools"), computer_use_enabled: tri("computer-use"),
        llm_tool_exposure: value("tool-exposure") || null,
        tool_allowlist: value("tool-exposure") === "whitelist" && value("tool-allowlist")
          ? [...new Set(value("tool-allowlist").split(/[\n,，]+/).map(s => s.trim()).filter(Boolean))] : null,
      };
      busy = true; el("save").disabled = true; el("fields").disabled = true; el("session-select").disabled = true;
      status("正在保存…");
      try {
        if (!selectedId) {
          const created = await request("/api/sessions", { ...session, provider: "custom", base_url: parameters.base_url, endpoint: parameters.endpoint });
          selectedId = created.session.id;
          sessions.push(created.session); renderSessionOptions();
        }
        const result = await request(`/api/sessions/${encodeURIComponent(selectedId)}/model-settings`, { session, parameters });
        if (disposed) return;
        fill(result);
        const index = sessions.findIndex(s => s.id === selectedId);
        if (index >= 0) sessions[index] = result.session;
        renderSessionOptions(); status("已保存，对下一轮会话生效。");
        try { await options.onSaved?.(result.session, result); }
        catch (error) { if (!disposed) status(`配置已保存，但界面刷新失败：${error.message}`, true); }
        if (!disposed) container.dispatchEvent(new CustomEvent("model-settings-saved", { bubbles: true, detail: result }));
      } catch (error) { if (!disposed) status(`保存失败：${error.message}`, true); }
      finally {
        busy = false;
        if (!disposed) { el("save").disabled = false; el("fields").disabled = false; el("session-select").disabled = false; }
      }
    }

    const markDirty = () => {
      dirty = true; status("有未保存的更改。"); options.onChanged?.(selectedId);
    };
    // input 即时保护尚未失焦的草稿，避免列表自动刷新覆盖第一次键入。
    const onInput = event => { if (el("form")?.contains(event.target)) markDirty(); };
    const onChange = event => {
      if (event.target === el("session-select")) { void select(value("session-select")); return; }
      updateDynamicFields(); markDirty();
    };
    const onClick = event => { if (event.target.closest('[data-ms-action="new"]')) void select(null); };
    el("form").addEventListener("submit", save);
    container.addEventListener("input", onInput); container.addEventListener("change", onChange); container.addEventListener("click", onClick);
    const api = {
      refresh, select, newSession: () => select(null), get sessionId() { return selectedId; }, get dirty() { return dirty; },
      destroy() { disposed = true; loadSequence++; container.removeEventListener("input", onInput); container.removeEventListener("change", onChange); container.removeEventListener("click", onClick); el("form")?.removeEventListener("submit", save); },
    };
    container.__modelSettings = api;
    void refresh(selectedId);
    return api;
  }
  window.CoolzhuModelSettings = Object.freeze({ mount });
})();
