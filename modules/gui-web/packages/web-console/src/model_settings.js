/* 统一会话参数页：模型目录只提供提示，用户可直接配置新模型。 */
(() => {
  "use strict";
  const esc = value => String(value ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const option = (value, label) => `<option value="${esc(value)}">${esc(label)}</option>`;
  const triOptions = `${option("", "沿用工程设置")}${option("true", "开启")}${option("false", "关闭")}`;
  const efforts = [["auto", "自动"], ["none", "关闭"], ["minimal", "极简"], ["low", "低"], ["medium", "中"], ["high", "高"], ["xhigh", "超高"], ["max", "最大"]];
  async function request(path, body, method, signal) {
    let response;
    try {
      response = await fetch(path, body === undefined ? { signal } : {
        method: method || "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body), signal,
      });
    } catch (error) {
      const pathname = new URL(path, window.location.href).pathname;
      throw new Error(error?.name === "AbortError" ? `${pathname} 请求已取消或超时`
        : `${pathname} 未获得 HTTP 响应：${error?.message || "本地服务连接失败"}`);
    }
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(value.error?.message || value.error || value.message || `请求失败 (${response.status})`);
    return value;
  }

  function mount(container, options = {}) {
    if (typeof container === "string") container = document.querySelector(container);
    if (!container) return null;
    container.__modelSettings?.destroy();
    let sessions = [], selectedId = options.sessionId || null, snapshot = null, loadSequence = 0, refreshSequence = 0, editSequence = 0, disposed = false;
    let busy = false, dirty = false;
    let discovery = null, devinCatalog = null, discoverySequence = 0, discoveryController = null, discovering = false;
    const protocols = options.backendFilter === "devin_acp" ? [["devin_acp", "Devin · 账号登录"]]
      : [["openai_chat_completions", "OpenAI Chat Completions"], ["anthropic_messages", "Anthropic Messages"], ["devin_acp", "Devin · 账号登录"]];
    container.classList.add("model-settings");
    container.innerHTML = `
      <div class="ms-intro"><div><span class="ms-eyebrow">MODEL & SESSION</span><h2>模型与会话</h2><p>为当前聊天室选择模型，配置连接、上下文与思考参数。</p></div><button type="button" data-ms-action="new" class="ms-secondary">新建会话</button></div>
      <label class="ms-session-picker">编辑配置的会话<select data-ms="session-select" aria-label="选择要编辑配置的会话"></select></label>
      <p class="ms-hint" data-ms="target-note">选择发送对象后，此处显示当前编辑的模型会话。</p>
      <form data-ms="form" autocomplete="off">
        <fieldset data-ms="fields">
          <section class="ms-section"><div class="ms-section-title"><span>01</span><h3>连接</h3></div>
            <div class="ms-grid">
              <label>会话名称<input data-ms="name" required maxlength="32" placeholder="例如：日常助手"></label>
              <label>通信协议<select data-ms="protocol">${protocols.map(item => option(...item)).join("")}</select></label>
              <div class="ms-wide" data-ms="devin-auth" hidden></div>
              <small class="ms-wide" data-ms="devin-context" hidden>同一聊天室与模型配置继续发送时，会沿用远端会话，应用重启后也可恢复。重置上下文或切换模型后，会开始新的远端会话。</small>
              <div class="ms-wide" data-ms="devin-rebind-row" hidden><button type="button" data-ms-action="rebind-devin" class="ms-secondary" aria-label="重新绑定 Devin 远端会话" title="重新绑定 Devin 远端会话">↻</button><small>如果已经清除了 Devin 远端会话，可重新绑定。保留本地聊天记录；下一轮发送时建立新连接。</small></div>
              <label class="ms-wide" data-ms="devin-review-label" hidden><span><input type="checkbox" data-ms="devin-review">允许读取当前工程用于仓库审查</span><small>仅文件读取、查找与内容搜索；不开放写入、命令、电脑操作或原生 Devin 工具。</small></label>
              <label class="ms-wide" data-ms="devin-computer-label" hidden><span><input type="checkbox" data-ms="devin-computer">允许 Computer Use</span><small>通过宿主操作浏览器与桌面；规划和验收使用所选模型，图片需由 ACP 声明支持。</small></label>
              <fieldset class="ms-wide" data-ms="devin-plugins-label" hidden><legend>允许的插件工具</legend><button type="button" data-ms="refresh-plugins" data-ms-action="refresh-plugins" class="ms-secondary" aria-label="刷新可选插件工具" title="刷新可选插件工具">↻</button><div data-ms="devin-plugin-tools"></div><small data-ms="devin-plugin-hint">仅勾选需要的已启用工具。插件执行需要当前聊天室完全访问；缺少授权时本轮不会延期执行。</small></fieldset>
              <label class="ms-wide">接口地址 · Base URL<input data-ms="base-url" type="url" required spellcheck="false" placeholder="https://api.example.com/v1"></label>
              <div class="ms-wide ms-discovery" aria-label="远程模型发现">
                <div class="ms-discovery-actions"><button type="button" data-ms-action="discover" class="ms-secondary">获取模型</button><label class="ms-checkbox"><input type="checkbox" data-ms="discovery-no-key">此次查询不使用密钥</label></div>
                <p data-ms="discovery-status" class="ms-hint" role="status" aria-live="polite">输入远程地址后可获取模型；本地模型保持手动填写。</p>
                <div data-ms="discovery-results" class="ms-discovery-results" hidden>
                  <label>筛选模型<input data-ms="discovery-filter" type="search" placeholder="按名称或 ID 筛选"></label>
                  <label>可用模型<select data-ms="discovery-model" aria-label="发现的远程模型"></select></label>
                  <p data-ms="discovery-detail" class="ms-hint"></p>
                  <div class="ms-discovery-actions"><button type="button" data-ms-action="adopt-model" class="ms-secondary">使用此模型</button><button type="button" data-ms-action="adopt-capability" class="ms-secondary">采用图片能力</button></div>
                </div>
              </div>
              <label class="ms-wide">模型 ID<input data-ms="model" required maxlength="1024" spellcheck="false" placeholder="填写服务端接受的完整模型 ID"><small>可选择发现的模型，也可手填；发现结果不会自动覆盖配置。</small></label>
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
          <section class="ms-section" hidden><div class="ms-section-title"><span>04</span><h3>工具能力</h3></div>
            <div class="ms-grid">
              <label>模型工具调用<select data-ms="enable-tools">${triOptions}</select></label>
              <label>电脑操作<select data-ms="computer-use">${triOptions}</select></label>
              <label class="ms-wide">整轮会话时限（分钟）<input data-ms="turn-timeout" type="number" min="1" max="1440" step="1" placeholder="默认 15"><small>覆盖本轮规划、工具与回复；与供应商单次请求超时分开。范围 1 分钟–24 小时。</small></label>
              <label class="ms-wide">工具暴露范围<select data-ms="tool-exposure"><option value="">沿用工程设置</option><option value="whitelist">权限允许的工具 / 自定义清单</option><option value="all">全部已注册且权限允许的工具</option><option value="dispatch-only">仅语义调度入口</option></select></label>
              <label class="ms-wide" data-ms="allowlist-label" hidden>允许的工具名称<textarea data-ms="tool-allowlist" rows="3" placeholder="每行一个工具名称；留空沿用权限范围"></textarea></label>
            </div><p class="ms-hint">会话参数决定模型能看到哪些工具；执行仍受当前工程权限约束。</p>
          </section>
        </fieldset>
        <div class="ms-footer"><p data-ms="status" role="status" aria-live="polite">正在读取配置…</p><button type="submit" data-ms="save" class="ms-primary">保存配置</button></div>
      </form>`;
    const controls = {
      new: ["plus", "新建模型会话"], discover: ["search", "获取远程模型"],
      "adopt-model": ["check", "使用所选模型"], "adopt-capability": ["vision", "采用所选模型的图片能力"],
      "rebind-devin": ["refresh", "重新绑定 Devin 远端会话"],
    };
    for (const [action, [icon, label]] of Object.entries(controls)) {
      setWuxiaIconOnly(container.querySelector(`[data-ms-action="${action}"]`), icon, label);
    }
    setWuxiaIconOnly(container.querySelector('[data-ms="save"]'), "save", "保存模型会话配置");
    const el = key => container.querySelector(`[data-ms="${key}"]`);
    const value = key => el(key).value.trim();
    const set = (key, data) => { el(key).value = data ?? ""; };
    const tri = key => value(key) === "" ? null : value(key) === "true";
    const number = key => value(key) === "" ? null : Number(value(key));
    const status = (message, error = false) => { el("status").textContent = message; el("status").classList.toggle("ms-error", error); };
    const devinAuth = window.CoolzhuDevinAuth?.mount(el("devin-auth"), {visible:false});
    let authVisible = false;
    let loadedReviewTools = [];
    const extensionTool = name => /^(?:plugin__(?!devin_)[A-Za-z0-9_-]+|dsh__[A-Za-z0-9_-]+)$/.test(name) && name.length <= 64;
    function renderPluginTools(data, parameters) {
      const selected = new Set(parameters?.enable_llm_tools === true ? (parameters.tool_allowlist || []).filter(extensionTool) : []);
      const tools = new Map((data?.available_plugin_tools || []).map(tool => [tool.name, tool]));
      for (const name of selected) if (!tools.has(name)) tools.set(name, { name, description: "当前未启用；保留已保存的选择，启用后才可调用。" });
      el("devin-plugin-tools").innerHTML = tools.size ? [...tools.values()].map(tool =>
        `<label class="ms-checkbox"><input type="checkbox" data-ms="devin-plugin" value="${esc(tool.name)}" ${selected.has(tool.name) ? "checked" : ""}><span>${esc(tool.description || tool.name)}<small>${esc(tool.name)}</small></span></label>`).join("")
        : "<p class=\"ms-hint\">暂无已启用的插件工具。请先在插件市场安装并启用，再刷新可选工具。</p>";
      el("devin-plugin-hint").textContent = data?.plugin_tools_error ? `插件工具读取失败：${data.plugin_tools_error}`
        : "仅勾选需要的已启用工具。插件执行需要当前聊天室完全访问；缺少授权时本轮不会延期执行。";
    }

    const devinConnection = () => value("protocol") === "devin_acp";
    function remoteCandidate() {
      if (devinConnection()) return true;
      try {
        const url = new URL(value("base-url")), host = url.hostname.toLowerCase().replace(/^\[|\]$/g, "");
        if (!["https:", "http:"].includes(url.protocol)) return false;
        if (host === "localhost" || host.endsWith(".localhost") || host.endsWith(".local") || host === "::1" || host === "::" || /^f[cd]|^fe[89ab]/.test(host) && host.includes(":")) return false;
        const parts = host.split(".").map(Number);
        if (parts.length === 4 && parts.every(Number.isInteger)) {
          const [a, b] = parts;
          if (a === 0 || a === 10 || a === 127 || a >= 224 || a === 192 && b === 168 || a === 172 && b >= 16 && b <= 31 || a === 169 && b === 254 || a === 100 && b >= 64 && b <= 127) return false;
        }
        return host.includes(".") || host.includes(":");
      } catch { return false; }
    }

    function resetDiscovery() {
      discoverySequence++; discoveryController?.abort(); discoveryController = null;
      discovery = null; discovering = false;
      el("discovery-results").hidden = true;
      set("discovery-filter", ""); el("discovery-model").replaceChildren();
      const canDiscover = remoteCandidate();
      container.querySelector('[data-ms-action="discover"]').disabled = !canDiscover;
      el("discovery-status").classList.remove("ms-error");
      el("discovery-status").textContent = canDiscover
        ? (devinConnection() ? "登录后获取账号的完整模型目录，选择模型及思考档位。" : "点击获取远程模型。仅查询目录，不进行对话或图片测试；未保存的更改保持在表单内。")
        : "本地和内网模型保持手动填写；远程发现需要完整的 HTTP(S) 地址。";
    }

    const selectedCandidate = () => discovery?.models?.find(model => model.id === value("discovery-model"));
    const imageLabel = model => model?.image_input === true ? "支持图片输入" : model?.image_input === false ? "目录声明不支持图片输入" : "图片能力未知";

    function renderCandidateDetail() {
      const model = selectedCandidate();
      const source = model?.capability_source === "official_catalog"
        ? `来源：官方资料（核对于 ${model.capability_checked_at}），查询未实测`
        : "来源：服务端目录，未实测";
      const detail = model ? [imageLabel(model), model.image_input == null ? "需手动确认" : source,
        model.cost_summary || model.cost_tier || "",
        model.reasoning_effort ? `思考档位 ${model.reasoning_effort}` : "",
        model.context_window ? `上下文 ${model.context_window.toLocaleString()} tokens` : "",
        model.max_output_tokens ? `输出上限 ${model.max_output_tokens.toLocaleString()} tokens` : ""].filter(Boolean).join(" · ") : "没有匹配的模型，可继续手动填写。";
      el("discovery-detail").textContent = detail;
      if (model?.capability_reference) {
        const reference = document.createElement("a");
        reference.href = model.capability_reference;
        reference.textContent = " · 官方依据";
        reference.target = "_blank"; reference.rel = "noopener noreferrer";
        el("discovery-detail").append(reference);
      }
      container.querySelector('[data-ms-action="adopt-model"]').disabled = !model;
      container.querySelector('[data-ms-action="adopt-capability"]').disabled = !model || model.image_input == null || value("model") !== model.id;
    }

    function renderCandidates() {
      const query = value("discovery-filter").toLowerCase(), previous = value("discovery-model");
      const models = (discovery?.models || []).filter(model => `${model.id} ${model.name}`.toLowerCase().includes(query));
      el("discovery-model").innerHTML = models.map(model => option(model.id, `${model.name === model.id ? model.id : `${model.name} · ${model.id}`} — ${imageLabel(model)}`)).join("");
      if (models.some(model => model.id === previous)) set("discovery-model", previous);
      else if (models.some(model => model.id === value("model"))) set("discovery-model", value("model"));
      renderCandidateDetail();
    }

    async function discoverModels() {
      if (busy || discovering || !remoteCandidate()) return;
      resetDiscovery();
      const sequence = discoverySequence, controller = new AbortController();
      discoveryController = controller; discovering = true;
      container.querySelector('[data-ms-action="discover"]').disabled = true;
      el("discovery-status").textContent = devinConnection() ? "正在读取 Devin CLI 的账号模型目录…" : "正在查询远程模型目录…";
      const noKey = el("discovery-no-key").checked;
      const body = { session_id: selectedId, base_url: value("base-url"), endpoint: value("endpoint"), protocol: value("protocol"), use_saved_key: !noKey && !el("clear-key").checked };
      if (noKey || el("clear-key").checked) body.api_key = "";
      else if (value("api-key")) body.api_key = value("api-key");
      try {
        const result = await request(devinConnection() ? "/api/backends/devin/models" : "/api/models/discover", devinConnection() ? {} : body, "POST", controller.signal);
        if (disposed || sequence !== discoverySequence) return;
        discovery = result;
        if (devinConnection()) { devinCatalog = result.models || []; updateDynamicFields(); }
        el("discovery-results").hidden = !result.models?.length;
        el("discovery-status").textContent = `${result.provider_hint || "远程服务"} · 已获取 ${result.models?.length || 0} 个模型${result.complete === false ? "（部分目录）" : ""}。 ${(result.warnings || []).join(" ")}`;
        renderCandidates();
      } catch (error) {
        if (disposed || sequence !== discoverySequence || error.name === "AbortError") return;
        el("discovery-status").textContent = `获取失败：${error.message}`;
        el("discovery-status").classList.add("ms-error");
      } finally {
        if (!disposed && sequence === discoverySequence) {
          discovering = false; discoveryController = null;
          container.querySelector('[data-ms-action="discover"]').disabled = !remoteCandidate();
        }
      }
    }

    function updateDynamicFields() {
      const devin = devinConnection();
      el("devin-review-label").hidden = !devin;
      el("devin-context").hidden = !devin;
      el("devin-rebind-row").hidden = !devin || !selectedId;
      el("devin-computer-label").hidden = !devin;
      el("devin-plugins-label").hidden = !devin;
      if (authVisible !== devin) { authVisible = devin; devinAuth?.setVisible(devin); }
      for (const key of ["base-url", "endpoint", "api-key", "clear-key", "discovery-no-key"]) {
        el(key).disabled = devin || (key === "clear-key" && !selectedId);
        el(key).closest("label").hidden = devin;
      }
      el("base-url").required = !devin;
      for (const key of ["context", "output", "temperature", "top-p", "thinking-budget", "model-type"]) {
        el(key).disabled = devin;
      }
      if (devin) {
        el("reasoning-mode").innerHTML = option("auto", "模型变体");
        const model = devinCatalog?.find(item => item.id === value("model"));
        const variants = model ? devinCatalog.filter(item => item.family === model.family && item.reasoning_effort) : [];
        const selectedEffort = value("reasoning-effort");
        const savedEffort = snapshot?.session?.model === value("model") ? snapshot.session.reasoning_effort : "auto";
        el("reasoning-effort").innerHTML = option("auto", "所选模型默认") + efforts.filter(([id]) => variants.some(item => item.reasoning_effort === id) || !model && id === savedEffort && id !== "auto").map(item => option(...item)).join("");
        set("reasoning-effort", variants.some(item => item.reasoning_effort === selectedEffort) || !model && selectedEffort === savedEffort ? selectedEffort : "auto");
        el("reasoning-mode").disabled = true; el("reasoning-effort").disabled = !variants.length;
        el("budget-label").hidden = true; el("thinking-budget").required = false;
        el("reasoning-hint").textContent = "思考档位会选择账号目录中对应的精确模型变体；发送前核对 ACP 生效模型。温度等参数使用模型默认值。";
        el("effective-limit").textContent = "上下文与输出容量使用 Devin 模型默认值。";
        el("multimodal-hint").textContent = tri("supports-multimodal") === true
          ? "图片通过 ACP 直传；发送时核对连接声明的图片能力，不支持则明确报错。"
          : "未启用图片直传时，由系统默认视觉 Agent 描述图片；模型目录不代表图片能力已验证。";
        el("key-hint").textContent = "使用本机 Devin CLI 登录，不使用 HTTP 密钥。";
        return;
      }
      el("reasoning-mode").disabled = false; el("reasoning-effort").disabled = false;
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
      set("protocol", data?.protocol || options.defaultProtocol || "openai_chat_completions");
      set("base-url", data?.base_url || "http://127.0.0.1:11434/v1"); set("endpoint", data?.endpoint);
      set("api-key", ""); el("clear-key").checked = false; el("discovery-no-key").checked = false;
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
      const reviewTools = (p.tool_allowlist || []).filter(name => ["read_file", "glob_search", "grep_search"].includes(name));
      el("devin-review").checked = p.enable_llm_tools === true
        && p.llm_tool_exposure === "whitelist" && Array.isArray(p.tool_allowlist)
        && reviewTools.length > 0;
      el("devin-computer").checked = p.enable_llm_tools === true && p.computer_use_enabled === true
        && p.llm_tool_exposure === "whitelist" && (p.tool_allowlist || []).includes("computer_use_perform");
      // 保存其它参数时保持既有只读子集，不悄悄扩大工具范围。
      loadedReviewTools = el("devin-review").checked ? reviewTools : [];
      renderPluginTools(data, p);
      set("computer-use", p.computer_use_enabled == null ? "" : String(p.computer_use_enabled));
      set("turn-timeout", p.turn_timeout_ms == null ? "" : p.turn_timeout_ms / 60000);
      set("tool-exposure", p.llm_tool_exposure); set("tool-allowlist", (p.tool_allowlist || []).join("\n"));
      updateDynamicFields(); resetDiscovery(); dirty = false;
      status(devinConnection() ? "Devin 配置已载入；可选择图片直传、工程只读、Computer Use 和已启用插件工具。其它附件及子 Agent 尚未开放。" : selectedId ? "配置已载入。修改后对下一轮会话生效。" : "填写连接信息后保存为新会话。");
      options.onSelected?.(selectedId);
    }

    function renderSessionOptions() {
      el("session-select").innerHTML = `${option("", "新会话")}${sessions.map(s => option(s.id, `${s.name} · ${s.model}`)).join("")}`;
      set("session-select", selectedId || "");
    }

    async function select(sessionId) {
      if (busy) return;
      editSequence++;
      resetDiscovery();
      selectedId = sessionId || null;
      renderSessionOptions();
      options.onSelected?.(selectedId);
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
      if (disposed || busy || dirty) return;
      const sequence = ++refreshSequence, editAtStart = editSequence;
      try {
        const registry = await request("/api/sessions");
        if (disposed || sequence !== refreshSequence || editAtStart !== editSequence || dirty) return;
        sessions = (registry.sessions || []).filter(s => !options.backendFilter || s.backend_kind === options.backendFilter || (options.backendFilter === "devin_acp" && ["devin", "devin_acp", "devin-acp"].includes(s.provider)));
        await select(sessions.some(s => s.id === sessionId) ? sessionId : sessions.some(s => s.id === registry.active_session_id) ? registry.active_session_id : sessions[0]?.id || null);
      } catch (error) {
        if (!disposed && sequence === refreshSequence && editAtStart === editSequence && !dirty)
          status(`读取会话失败：${error.message}`, true);
      }
    }

    async function refreshPluginTools() {
      if (disposed || busy || !selectedId || !devinConnection()) return;
      const sessionId = selectedId, sequence = loadSequence, button = el("refresh-plugins");
      button.disabled = true;
      try {
        const data = await request(`/api/sessions/${encodeURIComponent(sessionId)}/model-settings`);
        if (disposed || selectedId !== sessionId || loadSequence !== sequence) return;
        // 仅刷新工具目录，保留包括未保存勾选在内的其它模型参数。
        const names = [...container.querySelectorAll('[data-ms="devin-plugin"]:checked')].map(input => input.value);
        renderPluginTools(data, { enable_llm_tools: true, tool_allowlist: names });
      } catch (error) {
        if (!disposed && selectedId === sessionId && loadSequence === sequence)
          el("devin-plugin-hint").textContent = `插件工具读取失败：${error.message}`;
      } finally { if (!disposed) button.disabled = false; }
    }

    async function save(event) {
      event.preventDefault();
      if (busy || !el("form").reportValidity()) return;
      resetDiscovery();
      const devin = devinConnection();
      const session = { name: value("name"), model: value("model"), model_type: devin ? "text" : value("model-type"), reasoning_effort: value("reasoning-effort") };
      if (devin) session.provider = "devin";
      else if (snapshot?.backend_kind === "devin_acp") session.provider = "custom";
      if (!devin && (value("api-key") || el("clear-key").checked)) session.api_key_ref = el("clear-key").checked ? "" : value("api-key");
      const parameters = {
        ...(snapshot?.parameters || {}),
        protocol: value("protocol"), base_url: value("base-url"), endpoint: value("endpoint") || "",
        context_window: number("context") || 0, max_output_tokens: number("output") || 0,
        temperature: number("temperature"), top_p: number("top-p"),
        supports_multimodal: tri("supports-multimodal"),
        reasoning_mode: value("reasoning-mode"), thinking_budget: value("reasoning-mode") === "budget" ? number("thinking-budget") : null,
      };
      parameters.backend_kind = devin ? "devin_acp" : "llm_http";
      const pluginTools = [...container.querySelectorAll('[data-ms="devin-plugin"]:checked')].map(input => input.value);
      if (devin) Object.assign(parameters, {
        base_url: null, endpoint: null, context_window: 0, max_output_tokens: 0,
        temperature: null, top_p: null, thinking_budget: null, reasoning_mode: "auto",
        enable_llm_tools: el("devin-review").checked || el("devin-computer").checked || pluginTools.length > 0,
        computer_use_enabled: el("devin-computer").checked,
        llm_tool_exposure: "whitelist", tool_allowlist: [
          ...(el("devin-review").checked ? (loadedReviewTools.length ? [...loadedReviewTools] : ["read_file", "glob_search", "grep_search"]) : []),
          ...(el("devin-computer").checked ? ["computer_use_perform"] : []),
          ...pluginTools,
        ],
      });
      busy = true; el("save").disabled = true; el("fields").disabled = true; el("session-select").disabled = true;
      status("正在保存…");
      try {
        if (!selectedId) {
          const created = await request("/api/sessions", { ...session, provider: devin ? "devin" : "custom", base_url: parameters.base_url, endpoint: parameters.endpoint });
          selectedId = created.session.id;
          sessions.push(created.session); renderSessionOptions();
          snapshot = await request(`/api/sessions/${encodeURIComponent(selectedId)}/model-settings`);
        }
        const result = await request(`/api/sessions/${encodeURIComponent(selectedId)}/model-settings`, { session, parameters, expected_revision: snapshot?.configuration_revision });
        if (disposed) return;
        fill(result);
        const index = sessions.findIndex(s => s.id === selectedId);
        if (index >= 0) sessions[index] = result.session;
        renderSessionOptions(); status(devin ? "已保存 Devin 配置；所选宿主工具对下一轮会话生效。" : "已保存，对下一轮会话生效。");
        try { await options.onSaved?.(result.session, result); }
        catch (error) { if (!disposed) status(`配置已保存，但界面刷新失败：${error.message}`, true); }
        if (!disposed) container.dispatchEvent(new CustomEvent("model-settings-saved", { bubbles: true, detail: result }));
      } catch (error) { if (!disposed) status(`保存失败：${error.message}`, true); }
      finally {
        busy = false;
        if (!disposed) { el("save").disabled = false; el("fields").disabled = false; el("session-select").disabled = false; }
      }
    }

    async function rebindDevin() {
      if (disposed || busy || !selectedId || !devinConnection()) return;
      if (dirty) { status("请先保存配置，再重新绑定远端会话。", true); return; }
      const sessionId = selectedId, sequence = loadSequence;
      busy = true; el("fields").disabled = true; el("save").disabled = true;
      status("正在核对当前聊天室的远端绑定…");
      try {
        const current = await request(`/api/backends/devin/binding?session_id=${encodeURIComponent(sessionId)}`);
        if (disposed || selectedId !== sessionId || loadSequence !== sequence) return;
        if (!current.bindings?.some(item => item.remote_session_id)) {
          status("尚未绑定远端会话；下一轮发送时创建，此处不会提前创建。"); return;
        }
        await request("/api/backends/devin/binding", {session_id:sessionId,room_id:current.room_id,expected_bindings:current.bindings});
        if (!disposed && selectedId === sessionId && loadSequence === sequence)
          status("已解除旧远端绑定，聊天记录已保留。下一轮发送时建立并复用新会话。");
      } catch (error) { if (!disposed) status(`重新绑定失败：${error.message}`, true); }
      finally { busy = false; if (!disposed) { el("fields").disabled = false; el("save").disabled = false; } }
    }

    const markDirty = () => {
      dirty = true; editSequence++; status("有未保存的更改。"); options.onChanged?.(selectedId);
    };
    // input 即时保护尚未失焦的草稿，避免列表自动刷新覆盖第一次键入。
    const discoveryInputs = new Set(["base-url", "endpoint", "protocol", "api-key", "clear-key", "discovery-no-key"]);
    const onInput = event => {
      const key = event.target.dataset.ms;
      if (key === "discovery-filter") { renderCandidates(); return; }
      if (key === "discovery-model" || key === "discovery-no-key") return;
      if (discoveryInputs.has(key)) resetDiscovery();
      if (key === "model") { renderCandidateDetail(); if (devinConnection()) { set("reasoning-effort", "auto"); updateDynamicFields(); } }
      if (el("form")?.contains(event.target)) markDirty();
    };
    const onChange = event => {
      if (event.target === el("session-select")) { void select(value("session-select")); return; }
      const key = event.target.dataset.ms;
      if (key === "discovery-filter") return;
      if (key === "discovery-model") { renderCandidateDetail(); return; }
      if (discoveryInputs.has(key)) resetDiscovery();
      if (key === "discovery-no-key") return;
      if (devinConnection() && key === "reasoning-effort" && value("reasoning-effort") !== "auto") {
        const current = devinCatalog?.find(item => item.id === value("model"));
        const variant = current && devinCatalog.find(item => item.family === current.family && item.reasoning_effort === value("reasoning-effort"));
        if (variant) { set("model", variant.id); set("discovery-model", variant.id); renderCandidateDetail(); }
      }
      updateDynamicFields(); markDirty();
    };
    const onClick = event => {
      const action = event.target.closest("[data-ms-action]")?.dataset.msAction;
      if (action === "new") void select(null);
      if (action === "discover") void discoverModels();
      if (action === "refresh-plugins") void refreshPluginTools();
      if (action === "rebind-devin") void rebindDevin();
      if (action === "adopt-model") {
        const model = selectedCandidate();
        if (model) { set("model", model.id); if (devinConnection()) set("reasoning-effort", "auto"); updateDynamicFields(); if (devinConnection() && model.reasoning_effort) set("reasoning-effort", model.reasoning_effort); renderCandidateDetail(); markDirty(); }
      }
      if (action === "adopt-capability") {
        const model = selectedCandidate();
        if (model?.image_input != null && value("model") === model.id) {
          set("supports-multimodal", String(model.image_input)); updateDynamicFields(); markDirty();
        }
      }
    };
    el("form").addEventListener("submit", save);
    const accountChanged = () => { devinCatalog = null; if (devinConnection()) { resetDiscovery(); updateDynamicFields(); } };
    window.addEventListener?.("devin-account-changed", accountChanged);
    container.addEventListener("input", onInput); container.addEventListener("change", onChange); container.addEventListener("click", onClick);
    const api = {
      refresh, select, newSession: () => select(null), get sessionId() { return selectedId; }, get dirty() { return dirty; },
      destroy() { disposed = true; loadSequence++; refreshSequence++; discoverySequence++; discoveryController?.abort(); devinAuth?.destroy(); window.removeEventListener?.("devin-account-changed", accountChanged); container.removeEventListener("input", onInput); container.removeEventListener("change", onChange); container.removeEventListener("click", onClick); el("form")?.removeEventListener("submit", save); },
    };
    container.__modelSettings = api;
    void refresh(selectedId);
    return api;
  }
  window.CoolzhuModelSettings = Object.freeze({ mount });
})();
