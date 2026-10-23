/* 统一会话参数页：模型目录只提供提示，用户可直接配置新模型。 */
(() => {
  "use strict";
  const esc = value => String(value ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const option = (value, label) => `<option value="${esc(value)}">${esc(label)}</option>`;
  const triOptions = `${option("", "沿用工程设置")}${option("true", "开启")}${option("false", "关闭")}`;
  const efforts = [["auto", "自动"], ["none", "关闭"], ["minimal", "极简"], ["low", "低"], ["medium", "中"], ["high", "高"], ["xhigh", "超高"], ["max", "最大"]];
  const avatarChoices = [["", "随模型自动选择"], ...[
    ["deepseek", "DeepSeek 鲸鱼娘"], ["chatgpt", "ChatGPT 白龙娘"], ["qwen", "千问 Qwen 娘"],
    ["claude", "Claude 太阳娘"], ["gemini", "Gemini 星光娘"], ["grok", "Grok 星环娘"],
    ["glm", "GLM 智谱娘"], ["generic", "通用助手"],
  ].map(([name, label]) => [`assets/avatars/brand-v1/${name}.png`, label])];
  async function httpRequest(path, body, method, signal, headers) {
    let response;
    try {
      response = await fetch(path, body === undefined ? { signal, ...(headers ? { headers } : {}) } : {
        method: method || "POST", headers: { "Content-Type": "application/json", ...headers }, body: JSON.stringify(body), signal,
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
    let busy = false, dirty = false, initialDraftApplied = false, provider = "custom", access = "api", toastTimer = null;
    let sendingIds = new Set(), avatar = "", effortSaving = false, generationCapabilities = null;
    const providers = [
      ["openrouter", "OpenRouter", "https://openrouter.ai/api/v1", "openai_chat_completions"],
      ["openai", "OpenAI 兼容", "https://api.openai.com/v1", "openai_chat_completions"],
      ["anthropic", "Anthropic", "https://api.anthropic.com/v1", "anthropic_messages"],
      ["opencode", "OpenCode Zen", "https://opencode.ai/zen/v1", "openai_chat_completions"],
      ["huggingface", "Hugging Face", "https://router.huggingface.co/v1", "openai_chat_completions"],
      ["custom", "自定义 / 本地", "http://127.0.0.1:11434/v1", "openai_chat_completions"],
    ];
    let discovery = null, devinCatalog = null, discoverySequence = 0, discoveryController = null, discovering = false;
    let scopePromise = null;
    async function request(path, body, method, signal) {
      if (disposed) throw new Error("配置页已关闭，请重新打开");
      let headers;
      if (options.workspacePath !== undefined) {
        // 只绑定一次；迟到请求不能通过自动刷新标识转而写入新工程。
        scopePromise ||= httpRequest("/api/workspace").then(workspace => {
          if (workspace.workspace !== options.workspacePath || !workspace.workspace_id || !workspace.configuration_scope)
            throw new Error("工程已改变，草稿未提交；请刷新页面后重新打开配置页");
          return { id: workspace.workspace_id, value: workspace.configuration_scope };
        });
        const scope = await scopePromise;
        if (disposed) throw new Error("配置页已关闭，请重新打开");
        if (path === "/api/sessions" || path.startsWith("/api/sessions/"))
          headers = { "X-Coolzhu-Workspace-Id": scope.id, "X-Coolzhu-Configuration-Scope": scope.value };
      }
      return httpRequest(path, body, method, signal, headers);
    }
    const httpProtocols = [["openai_chat_completions", "OpenAI Chat Completions"], ["anthropic_messages", "Anthropic Messages"], ["openai_images_generations", "图像生成"], ["openai_videos", "视频生成"]];
    const protocols = options.backendFilter === "devin_acp" ? [["devin_acp", "Devin · 账号登录"]]
      : options.backendFilter === "llm_http" ? httpProtocols : [...httpProtocols, ["devin_acp", "Devin · 账号登录"], ["openai_subscription", "OpenAI / ChatGPT · 账号登录"]];
    container.classList.add("model-settings");
    container.innerHTML = `
      <div class="ms-intro"><div><span class="ms-eyebrow">MODEL & SESSION</span><h2>模型与会话</h2></div><div class="ms-heading-actions"><button type="button" data-ms-action="new" class="ms-secondary">＋ 新建会话</button><button type="button" data-ms="save" data-ms-action="save" class="ms-primary">保存配置</button></div></div>
      <label class="ms-session-picker">编辑配置的会话<select data-ms="session-select" aria-label="选择要编辑配置的会话"></select><span data-ms="dirty" hidden>● 未保存</span></label>
      <div class="ms-avatar-row"><img data-ms="avatar-preview" alt="会话头像" width="40" height="40"><span>会话头像</span><select data-ms="avatar" aria-label="选择品牌头像">${avatarChoices.map(item=>option(...item)).join("")}</select><label class="ms-upload">上传图片<input data-ms="avatar-upload" type="file" accept="image/png,image/webp"></label><details class="ms-avatar-library"><summary>从 Q 版头像库选择</summary><div class="ms-avatar-grid">${avatarChoices.slice(1).map(([path,label])=>`<button type="button" data-ms-avatar="${esc(path)}" aria-pressed="false"><img src="${esc(path)}" alt="" width="64" height="64"><span>${esc(label)}</span></button>`).join("")}</div></details></div>
      <div class="ms-effort-row"><label for="ms-effort-slider">思考程度<small>对下一次请求生效</small></label><div class="ms-effort-track"><input id="ms-effort-slider" data-ms="effort-slider" type="range" min="0" max="0" value="0" step="1" aria-label="思考程度"><div data-ms="effort-ticks"></div></div><output data-ms="effort-value">自动</output></div>
      <div class="ms-access-tabs" role="tablist" aria-label="接入方式"><button type="button" role="tab" data-ms-access="api">API 接入</button><button type="button" role="tab" data-ms-access="subscription">订阅接入</button></div>
      <div class="ms-access-layout"><nav class="ms-providers" aria-label="提供方"><div data-ms="api-providers">${providers.map(([id,name])=>`<button type="button" data-ms-provider="${id}">${name}</button>`).join("")}</div><div data-ms="subscription-providers" hidden><button type="button" data-ms-provider="devin">Devin</button><button type="button" data-ms-provider="openai_subscription">OpenAI / ChatGPT</button><button type="button" disabled title="后续开放">Anthropic / Claude · 后续</button><button type="button" disabled title="后续开放">Gemini · 后续</button><small>账号授权由官方登录组件完成。本机不保存密码。</small></div></nav>
      <form data-ms="form" autocomplete="off">
        <fieldset data-ms="fields">
          <section class="ms-section"><div class="ms-section-title"><span>01</span><h3>连接</h3></div>
            <div class="ms-grid">
              <label>会话名称<input data-ms="name" required maxlength="32" placeholder="例如：日常助手"></label>
              <label>通信协议<select data-ms="protocol">${protocols.map(item => option(...item)).join("")}</select></label>
              <div class="ms-wide" data-ms="devin-auth" hidden></div><div class="ms-wide ms-account-card" data-ms="openai-auth" hidden><strong>OpenAI / ChatGPT</strong><p data-ms="openai-auth-status" role="status">读取中…</p><div class="ms-heading-actions"><button type="button" data-ms-action="openai-status">刷新状态</button><button type="button" data-ms-action="openai-login">登录</button><button type="button" data-ms-action="openai-cancel" hidden>取消登录</button><button type="button" data-ms-action="openai-logout">退出登录</button></div><a data-ms="openai-login-link" target="_blank" rel="noopener noreferrer" hidden>打开授权页</a><details data-ms="openai-auth-error" hidden><summary>错误详情</summary><pre data-ms="openai-auth-error-detail"></pre></details></div>
              <small class="ms-wide" data-ms="devin-context" hidden>同一聊天室与模型配置继续发送时，会沿用远端会话，应用重启后也可恢复。重置上下文或切换模型后，会开始新的远端会话。</small>
              <div class="ms-wide" data-ms="devin-rebind-row" hidden><button type="button" data-ms-action="rebind-devin" class="ms-secondary" aria-label="重新绑定 Devin 远端会话" title="重新绑定 Devin 远端会话">↻</button><small>如果已经清除了 Devin 远端会话，可重新绑定。保留本地聊天记录；下一轮发送时建立新连接。</small></div>
              <label class="ms-wide" data-ms="devin-review-label" hidden><span><input type="checkbox" data-ms="devin-review">允许读取当前工程用于仓库审查</span><small>仅文件读取、查找与内容搜索；不开放写入、命令、电脑操作或原生 Devin 工具。</small></label>
              <label class="ms-wide" data-ms="devin-computer-label" hidden><span><input type="checkbox" data-ms="devin-computer">允许 Computer Use</span><small>通过宿主操作浏览器与桌面；规划和验收使用所选模型，图片需由 ACP 声明支持。</small></label>
              <fieldset class="ms-wide" data-ms="devin-plugins-label" hidden><legend>允许的插件工具</legend><button type="button" data-ms="refresh-plugins" data-ms-action="refresh-plugins" class="ms-secondary" aria-label="刷新可选插件工具" title="刷新可选插件工具">↻</button><div data-ms="devin-plugin-tools"></div><small data-ms="devin-plugin-hint">仅勾选需要的已启用工具。插件执行需要当前聊天室完全访问；缺少授权时本轮不会延期执行。</small></fieldset>
              <label class="ms-wide">接口地址 · Base URL<input data-ms="base-url" type="url" required spellcheck="false" placeholder="https://api.example.com/v1"></label>
              <div class="ms-wide ms-discovery" aria-label="远程模型发现">
                <div class="ms-discovery-actions"><button type="button" data-ms-action="discover" class="ms-secondary">获取模型</button><label class="ms-checkbox"><input type="checkbox" data-ms="discovery-no-key">此次查询不使用密钥</label></div>
                <p data-ms="discovery-status" class="ms-hint" role="status" aria-live="polite">获取可用模型。</p>
                <div data-ms="discovery-results" class="ms-discovery-results" hidden>
                  <label>筛选模型<input data-ms="discovery-filter" type="search" placeholder="按名称或 ID 筛选"></label>
                  <label>可用模型<select data-ms="discovery-model" aria-label="发现的远程模型"></select></label>
                  <p data-ms="discovery-detail" class="ms-hint"></p>
                  <div class="ms-discovery-actions"><button type="button" data-ms-action="adopt-model" class="ms-secondary">使用此模型</button><button type="button" data-ms-action="adopt-capability" class="ms-secondary">采用图片能力</button></div>
                </div>
              </div>
              <label class="ms-wide">模型 ID<input data-ms="model" required maxlength="1024" spellcheck="false" placeholder="模型 ID"><small>可从目录选择或手填。</small></label>
              <label class="ms-wide">请求路径 · Endpoint <span class="ms-optional">可选</span><input data-ms="endpoint" spellcheck="false" placeholder="留空自动填写"></label>
              <label class="ms-wide">API Key<input data-ms="api-key" type="password" autocomplete="new-password" spellcheck="false" placeholder="本地服务可留空"><small data-ms="key-hint">留空保留已有密钥。</small></label>
              <label>模型用途<select data-ms="model-type"><option value="text">对话 / 代码</option><option value="vision">图像理解</option><option value="multimodal">多模态</option><option value="image">图像生成</option><option value="video">视频</option></select></label>
              <label class="ms-checkbox"><input type="checkbox" data-ms="clear-key">清除该会话保存的密钥</label>
              <label class="ms-wide">图片输入能力<select data-ms="supports-multimodal"><option value="">沿用模型能力</option><option value="true">支持图片直传</option><option value="false">纯文本 · 由视觉 Agent 描述</option></select><small data-ms="multimodal-hint"></small></label>
            </div>
          </section>
          <section class="ms-section" data-ms="generation" hidden><div class="ms-section-title"><span>02</span><h3>生成参数</h3></div>
            <p data-ms="generation-note" class="ms-hint"></p><div class="ms-grid">
              <label>数量<input data-ms="gen-n" type="number" min="1" max="10" step="1" placeholder="模型默认"></label>
              <label>尺寸<input data-ms="gen-size" placeholder="例如 1024x1024" list="ms-gen-size"></label>
              <label>宽高比<input data-ms="gen-aspect_ratio" placeholder="模型默认" list="ms-gen-aspect_ratio"></label>
              <label>分辨率<input data-ms="gen-resolution" placeholder="模型默认" list="ms-gen-resolution"></label>
              <label>质量<input data-ms="gen-quality" placeholder="模型默认" list="ms-gen-quality"></label>
              <label>输出格式<input data-ms="gen-output_format" placeholder="PNG / JPEG / WebP" list="ms-gen-output_format"></label>
              <label>种子<input data-ms="gen-seed" type="number" min="0" max="4294967295" step="1" placeholder="随机"></label>
              <label>时长（秒）<input data-ms="gen-duration" type="number" min="1" max="600" step="1" list="ms-gen-duration" placeholder="模型默认"></label>
              <label>生成音频<select data-ms="gen-generate_audio"><option value="">模型默认</option><option value="true">生成</option><option value="false">关闭</option></select></label>
              <label>回调地址<input data-ms="gen-callback_url" type="url" placeholder="可选 HTTPS 地址"></label>
              <label>宽度<input data-ms="gen-width" type="number" min="64" max="8192" step="1" placeholder="模型默认"></label>
              <label>高度<input data-ms="gen-height" type="number" min="64" max="8192" step="1" placeholder="模型默认"></label>
              <label>帧数<input data-ms="gen-num_frames" type="number" min="1" max="14401" step="1" placeholder="模型默认"></label>
              <label>帧率<input data-ms="gen-frame_rate" type="number" min="1" max="120" step="1" placeholder="模型默认"></label>
            </div><div data-ms="generation-choices"></div><p class="ms-hint">参考图使用聊天图片附件。首末帧、局部编辑蒙版暂未开放；不支持的参数会禁用。生成结果保存为本地附件，可预览；视频进后台任务。</p>
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
              <label data-ms="effort-label" hidden>思考层级<select data-ms="reasoning-effort">${efforts.map(item => option(...item)).join("")}</select></label>
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
      </form></div><p class="ms-toast" data-ms="status" role="status" aria-live="polite" hidden></p>`;
    const el = key => container.querySelector(`[data-ms="${key}"]`);
    const value = key => el(key).value.trim();
    const set = (key, data) => { el(key).value = data ?? ""; };
    const tri = key => value(key) === "" ? null : value(key) === "true";
    const number = key => value(key) === "" ? null : Number(value(key));
    const status = (message, error = false) => {
      clearTimeout(toastTimer); el("status").textContent = message; el("status").hidden = !message;
      el("status").classList.toggle("ms-error", error); el("status").setAttribute?.("role", error ? "alert" : "status");
      if (message && !error) toastTimer = setTimeout(() => { if (!disposed) el("status").hidden = true; }, 3000);
    };
    const dirtyBadge = () => { el("dirty").hidden = !dirty; };
    const devinAuth = window.CoolzhuDevinAuth?.mount(el("devin-auth"), {visible:false});
    container.querySelector('[data-ms-action="openai-logout"]').hidden = true;
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
    const openaiConnection = () => value("protocol") === "openai_subscription";
    const subscriptionConnection = () => devinConnection() || openaiConnection();
    const syncAccess = () => {
      access = subscriptionConnection() ? "subscription" : "api";
      el("api-providers").hidden = access !== "api"; el("subscription-providers").hidden = access !== "subscription";
      container.querySelectorAll("[data-ms-access]").forEach(node => { node.classList.toggle("is-active", node.dataset.msAccess === access); node.setAttribute("aria-selected", String(node.dataset.msAccess === access)); });
      container.querySelectorAll("[data-ms-provider]").forEach(node => node.classList.toggle("is-active", node.dataset.msProvider === provider));
    };
    const syncAvatar = () => {
      // 旧系统头像恢复自动品牌选择；用户上传的头像保持原样。
      if (window.CoolzhuBrandAvatars?.isLegacy(avatar)) avatar = "";
      el("avatar").innerHTML = avatarChoices.map(item=>option(...item)).join("") + (avatar && !avatarChoices.some(([path])=>path===avatar) ? option(avatar, "自定义头像") : "");
      set("avatar", avatar);
      el("avatar-preview").src = window.CoolzhuBrandAvatars?.resolve({avatar,model:value("model"),provider}) || avatar || "assets/avatars/brand-v1/generic.png";
      container.querySelectorAll("[data-ms-avatar]").forEach(node => node.setAttribute("aria-pressed", String(node.dataset.msAvatar === avatar)));
    };
    function syncEffort() {
      const choices = Array.from(el("reasoning-effort").options || []);
      const slider = el("effort-slider"); slider.max = String(Math.max(0, choices.length - 1));
      slider.value = String(Math.max(0, choices.findIndex(item => item.value === value("reasoning-effort"))));
      const connectionChanged = snapshot && (value("model") !== snapshot.session?.model || value("protocol") !== snapshot.protocol || provider !== snapshot.session?.provider);
      slider.disabled = busy || effortSaving || ["image","video"].includes(value("model-type")) || choices.length < 2 || el("reasoning-effort").disabled || !!connectionChanged;
      el("effort-ticks").innerHTML = choices.map(item => `<span>${esc(item.textContent)}</span>`).join("");
      el("effort-value").textContent = choices[Number(slider.value)]?.textContent || "不适用";
      slider.setAttribute?.("aria-valuetext", el("effort-value").textContent);
    }
    async function saveEffort() {
      const choice = el("reasoning-effort").options?.[Number(value("effort-slider"))];
      if (!choice || busy || effortSaving) return;
      const oldEffort = value("reasoning-effort"), oldModel = value("model"), id = selectedId, sequence = loadSequence;
      const patch = {reasoning_effort:choice.value};
      if (devinConnection() && choice.value !== "auto") {
        const model = devinCatalog?.find(item => item.id === oldModel);
        const variant = model && devinCatalog.find(item => item.family === model.family && item.reasoning_effort === choice.value);
        if (!variant) { status("请先刷新账号模型目录，核对可用的思考档位。",true); syncEffort(); return; }
        patch.model = variant.id;
      }
      set("reasoning-effort", choice.value); if (patch.model) set("model", patch.model); syncEffort();
      if (!id) { markDirty(); return; }
      // 其它未保存字段不随滑块提交。
      effortSaving = true; syncEffort();
      try {
        const latest = await request(`/api/sessions/${encodeURIComponent(id)}/model-settings`, {session:patch,parameters:snapshot?.parameters || {},expected_revision:snapshot?.configuration_revision}, "PATCH");
        if (disposed || id !== selectedId || sequence !== loadSequence) return;
        snapshot = latest;
        status("思考程度已更新，下次请求生效。");
      } catch (error) {
        if (!disposed && id === selectedId && sequence === loadSequence) { set("reasoning-effort",oldEffort); set("model",oldModel); status(`更新失败：${error.message}`,true); }
      } finally { effortSaving=false; if (!disposed) syncEffort(); }
    }
    let authPoll = null;
    async function openaiAuth(action = "status") {
      try {
        const result = await request(`/api/backends/openai/${action}`, action === "status" ? undefined : {}, "POST");
        if (disposed) return;
        const account = result.account;
        el("openai-auth-status").textContent = [result.logged_in ? "已登录" : result.active ? "等待授权" : "未登录", account?.email, account?.planType].filter(Boolean).join(" · ");
        el("openai-auth-error").hidden = true;
        container.querySelector('[data-ms-action="openai-login"]').textContent = result.logged_in ? "重新登录" : "登录";
        container.querySelector('[data-ms-action="openai-logout"]').hidden = !result.logged_in;
        let limits = el("openai-auth").querySelector(".ms-account-limits");
        if (result.limits) {
          if (!limits) { limits = document.createElement("details"); limits.className="ms-account-limits"; const title=document.createElement("summary"); title.textContent="账号额度"; limits.append(title,document.createElement("pre")); el("openai-auth").append(limits); }
          limits.querySelector("pre").textContent=JSON.stringify(result.limits,null,2);
        } else limits?.remove();
        const link=el("openai-login-link"); link.hidden=!result.auth_url;
        if (result.auth_url) link.href=result.auth_url;
        container.querySelector('[data-ms-action="openai-cancel"]').hidden=!result.active;
        clearTimeout(authPoll);
        if (result.active) authPoll=setTimeout(()=>void openaiAuth(),2000);
      } catch (error) { if(!disposed) { el("openai-auth-status").textContent="连接失败，请重试"; el("openai-auth-error-detail").textContent=error.message; el("openai-auth-error").hidden=false; } }
    }
    function remoteCandidate() {
      if (subscriptionConnection()) return true;
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
        ? (subscriptionConnection() ? "获取账号可用模型。" : "获取接口可用模型。")
        : "本地模型请手填。";
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
        model.tool_calling === true ? "目录声明支持工具调用" : model.tool_calling === false ? "目录声明不支持工具调用" : "",
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
      const filteredConnector = options.modelFilter && value("base-url").replace(/\/$/, "") === options.initialDraft?.baseUrl;
      const models = (discovery?.models || []).filter(model => (!filteredConnector || options.modelFilter(model)) && `${model.id} ${model.name}`.toLowerCase().includes(query));
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
        const result = await request(subscriptionConnection() ? `/api/backends/${openaiConnection() ? "openai" : "devin"}/models` : "/api/models/discover", subscriptionConnection() ? {} : body, "POST", controller.signal);
        if (disposed || sequence !== discoverySequence) return;
        discovery = result;
        if (subscriptionConnection()) { devinCatalog = result.models || []; updateDynamicFields(); syncEffort(); }
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

    const generationKeys=["n","size","aspect_ratio","resolution","quality","output_format","seed","duration","generate_audio","callback_url","width","height","num_frames","frame_rate"];
    const generationNumbers=new Set(["n","seed","duration","width","height","num_frames","frame_rate"]);
    function syncGeneration() {
      const kind=value("model-type"), media=!subscriptionConnection() && ["image","video"].includes(kind);
      el("generation").hidden=!media;
      for (const key of ["context","output","reasoning-mode","temperature","top-p"]) el(key).closest("section").hidden=media;
      if(!media)return;
      const cap=generationCapabilities;
      const matches=cap?.model===value("model") && cap?.model_type===kind && cap?.base_url===value("base-url");
      const fields=matches?cap.fields:{};
      const agnes=value("model").toLowerCase().startsWith("agnes");
      const agnesVideo25=kind==="video" && ["agnes-video-2.5","agnes-video-2.5-flash"].includes(value("model"));
      // 官方 2.5 协议与旧版帧参数不同；远程目录可进一步限制已适配参数。
      const known=agnesVideo25?{duration:{minimum:4,maximum:12},size:{values:value("model").endsWith("-flash")?["720P"]:["720P","1080P","1K","2K"]},aspect_ratio:{values:["16:9","9:16","1:1","4:3","3:4","21:9"]},seed:{minimum:0,maximum:4294967295}}:{};
      const basic=provider==="openrouter"? ["n","callback_url"] : kind==="image"?["n","size","quality","output_format"]:agnesVideo25?[]:agnes?["width","height","num_frames","frame_rate"]:[];
      const allowed=kind==="image"?["n","size","aspect_ratio","resolution","quality","output_format","seed"]:agnesVideo25?Object.keys(known):["duration","size","aspect_ratio","resolution","generate_audio","seed","callback_url","width","height","num_frames","frame_rate"];
      let choices="";
      for(const key of generationKeys) {
        const input=el(`gen-${key}`),rule=fields?.[key] || known[key];
        input.closest("label").hidden=!allowed.includes(key);
        input.disabled=!allowed.includes(key) || !(rule || basic.includes(key));
        input.title=input.disabled?"当前模型目录未声明此参数，暂不可设置":"";
        if(generationNumbers.has(key)) {input.min=String(rule?.minimum ?? (key==="seed"?0:1));input.max=String(rule?.maximum ?? (key==="n"?10:key==="duration"?600:key==="seed"?4294967295:14401));}
        if(rule?.values) choices+=`<datalist id="ms-gen-${key}">${rule.values.map(v=>option(v,String(v))).join("")}</datalist>`;
      }
      el("generation-choices").innerHTML=choices;
      el("generation-note").textContent=matches || agnesVideo25 ? "留空使用模型默认值。" : provider==="openrouter"?"获取模型目录后可设置参数。":"使用基本参数，以接口支持为准。";
    }
    function generationDraft() {
      if(subscriptionConnection() || !["image","video"].includes(value("model-type")))return null;
      const parameters={};
      for(const key of generationKeys) {const input=el(`gen-${key}`);if(input.disabled || !value(`gen-${key}`))continue;
        parameters[key]=generationNumbers.has(key)?Number(input.value):key==="generate_audio"?input.value==="true":input.value.trim();
      }
      const cap=generationCapabilities;
      const capabilities=cap?.model===value("model")&&cap?.model_type===value("model-type")&&cap?.base_url===value("base-url")?cap:null;
      return {parameters,capabilities};
    }

    function updateDynamicFields() {
      const devin = devinConnection(), subscription = subscriptionConnection();
      syncAccess(); syncAvatar(); el("openai-auth").hidden = !openaiConnection();
      el("devin-review-label").hidden = !devin;
      el("devin-context").hidden = !devin;
      el("devin-rebind-row").hidden = !devin || !selectedId;
      el("devin-computer-label").hidden = !devin;
      el("devin-plugins-label").hidden = !devin;
      if (authVisible !== devin) { authVisible = devin; devinAuth?.setVisible(devin); }
      for (const key of ["base-url", "endpoint", "api-key", "clear-key", "discovery-no-key"]) {
        el(key).disabled = subscription || (key === "clear-key" && !selectedId);
        el(key).closest("label").hidden = subscription;
      }
      el("base-url").required = !subscription;
      syncGeneration();
      for (const key of ["context", "output", "temperature", "top-p", "thinking-budget", "model-type"]) {
        el(key).disabled = subscription;
      }
      if (openaiConnection()) {
        const model = devinCatalog?.find(item => item.id === value("model"));
        const selected = value("reasoning-effort");
        const supported = model?.supported_reasoning_efforts || [];
        const savedEffort = snapshot?.session?.model === value("model") ? snapshot.session.reasoning_effort : "auto";
        el("reasoning-mode").innerHTML = option("auto", "账号模型默认");
        el("reasoning-mode").disabled = true;
        el("reasoning-effort").innerHTML = option("auto","自动") + efforts.filter(([id]) => supported.includes(id) || !model && id === savedEffort && id !== "auto").map(item => option(...item)).join("");
        set("reasoning-effort", supported.includes(selected) || !model && selected === savedEffort ? selected : "auto"); el("reasoning-effort").disabled = !supported.length;
        el("budget-label").hidden = true; el("thinking-budget").required = false;
        el("effective-limit").textContent = "容量与额度由 ChatGPT 账号模型决定。";
        el("reasoning-hint").textContent = "档位以账号模型目录为准。";
        syncEffort(); return;
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
        el("reasoning-hint").textContent = "切换档位会选择对应的模型变体。";
        el("effective-limit").textContent = "上下文与输出容量使用 Devin 模型默认值。";
        el("multimodal-hint").textContent = tri("supports-multimodal") === true
          ? "直接发送图片，需模型支持。"
          : "先由视觉 Agent 描述图片。";
        el("key-hint").textContent = "使用本机 Devin CLI 登录，不使用 HTTP 密钥。";
        syncEffort(); return;
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
        auto: "自动使用模型支持的思考方式。",
        effort: "选择模型支持的档位；自动沿用默认值。",
        thinking: "开启或关闭思考；自动沿用默认值。",
        adaptive: "自适应思考；温度与 Top P 请留空。",
        budget: "预算至少 1024，且小于输出上限；采样参数请留空。",
      };
      const qwenHints = {
        auto: "自动沿用模型默认档位。",
        effort: "可用档位：低、中、超高；其它档位取对应值。",
        thinking: "开启使用超高档；自动沿用模型默认值。",
      };
      el("reasoning-hint").textContent = (qwen38 && qwenHints[mode]) || hints[mode];
      const multimodal = tri("supports-multimodal");
      const imageStrategy = snapshot?.image_input_strategy;
      const savedNative = imageStrategy === "native" || (imageStrategy == null && snapshot?.effective_supports_multimodal === true);
      const savedVision = imageStrategy === "vision-description" || (imageStrategy == null && snapshot?.effective_supports_multimodal === false);
      el("multimodal-hint").textContent = multimodal === true
        ? "直接发送图片，需模型支持。"
        : multimodal === false
          ? "先由视觉 Agent 描述图片。"
          : savedNative ? "当前直接发送图片。" : savedVision ? "当前由视觉 Agent 描述图片。" : "自动识别图片能力。";
      syncEffort();
    }

    function fill(data) {
      snapshot = data;
      const s = data?.session || {}, p = data?.parameters || {};
      generationCapabilities=p.generation?.capabilities || null;
      for(const key of generationKeys) set(`gen-${key}`,p.generation?.parameters?.[key] ?? "");
      provider = s.provider || "custom"; avatar = s.avatar || ""; set("avatar", avatar);
      if (!providers.some(item => item[0] === provider) && !["devin","openai_subscription"].includes(provider)) provider="custom";
      set("name", s.name); set("model", s.model); set("model-type", s.model_type || "text");
      set("protocol", data?.protocol || options.defaultProtocol || "openai_chat_completions");
      set("base-url", data?.base_url || "http://127.0.0.1:11434/v1"); set("endpoint", data?.endpoint);
      set("api-key", ""); el("clear-key").checked = false; el("discovery-no-key").checked = false;
      set("supports-multimodal", p.supports_multimodal == null ? "" : String(p.supports_multimodal));
      el("clear-key").disabled = !selectedId;
      el("api-key").placeholder = selectedId ? "留空保留已有密钥" : options.initialDraft ? "填写该平台 API Key / Token" : "本地服务可留空";
      el("key-hint").textContent = selectedId ? `${s.api_key_status || "未配置"} · 留空保留` : "填写该平台的密钥；本地服务可留空。";
      set("context", p.context_window || ""); set("output", p.max_output_tokens || "");
      el("context").placeholder = data?.default_context_window ? `默认 ${data.default_context_window.toLocaleString()}` : "沿用模型默认值";
      el("output").placeholder = data?.default_max_output_tokens ? `默认 ${data.default_max_output_tokens.toLocaleString()}` : "沿用模型默认值";
      el("effective-limit").textContent = data ? `当前：上下文 ${Number(data.effective_context_window).toLocaleString()} · 输出 ${Number(data.effective_max_output_tokens).toLocaleString()} tokens` : "留空使用模型默认值。";
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
      updateDynamicFields(); resetDiscovery(); dirty = false; dirtyBadge(); syncAvatar();
      status(""); if (openaiConnection()) void openaiAuth();
      options.onSelected?.(selectedId);
    }

    function renderSessionOptions() {
      el("session-select").innerHTML = `${option("", "新会话")}${sessions.map(s => option(s.id, `${s.name} · ${s.model}${sendingIds.has(s.id) ? " · 当前发送对象" : ""}`)).join("")}`;
      set("session-select", selectedId || "");
    }

    async function select(sessionId) {
      if (busy || effortSaving) return;
      if (dirty && !window.confirm("当前配置尚未保存，是否放弃更改并切换？")) { set("session-select",selectedId || ""); return; }
      editSequence++;
      resetDiscovery();
      selectedId = sessionId || null;
      renderSessionOptions();
      options.onSelected?.(selectedId);
      const sequence = ++loadSequence;
      if (!selectedId) {
        fill(null);
        if (options.initialDraft) {
          initialDraftApplied = true;
          set("name", options.initialDraft.name); set("protocol", "openai_chat_completions");
          set("base-url", options.initialDraft.baseUrl); set("model", ""); set("api-key", "");
          updateDynamicFields(); resetDiscovery(); dirty = true;
          status("新连接草稿尚未保存。填写该平台凭据并获取模型，再保存；已有会话和密钥保持独立。");
        }
        el("fields").disabled = false; el("save").disabled = false; return;
      }
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
        if (options.initialDraft && !initialDraftApplied) {
          await select(null);
        } else await select(sessions.some(s => s.id === sessionId) ? sessionId : sessions.some(s => s.id === registry.active_session_id) ? registry.active_session_id : sessions[0]?.id || null);
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
      if (busy || effortSaving || !el("form").reportValidity()) return;
      if (!selectedId && options.initialDraft && !value("api-key")) {
        status("请填写该平台自己的 API Key / Token，再保存新连接。", true); return;
      }
      resetDiscovery();
      const devin = devinConnection(), subscription = subscriptionConnection();
      const session = { name: value("name"), model: value("model"), model_type: subscription ? "text" : value("model-type"), avatar, reasoning_effort: value("reasoning-effort") };
      session.provider = devin ? "devin" : openaiConnection() ? "openai_subscription" : provider;
      const changedProvider = snapshot?.session && snapshot.session.provider !== session.provider;
      if (subscription) session.api_key_ref = "";
      else if (changedProvider || value("api-key") || el("clear-key").checked) session.api_key_ref = el("clear-key").checked ? "" : value("api-key");
      const parameters = {
        ...(snapshot?.parameters || {}),
        generation:generationDraft(),
        protocol: value("protocol"), base_url: value("base-url"), endpoint: value("endpoint") || "",
        context_window: number("context") || 0, max_output_tokens: number("output") || 0,
        temperature: number("temperature"), top_p: number("top-p"),
        supports_multimodal: tri("supports-multimodal"),
        reasoning_mode: value("reasoning-mode"), thinking_budget: value("reasoning-mode") === "budget" ? number("thinking-budget") : null,
      };
      parameters.backend_kind = devin ? "devin_acp" : openaiConnection() ? "openai_subscription" : "llm_http";
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
      if (!subscription && ["image","video"].includes(session.model_type)) Object.assign(parameters,{context_window:0,max_output_tokens:0,temperature:null,top_p:null,reasoning_mode:"auto",thinking_budget:null,enable_llm_tools:false,computer_use_enabled:false,llm_tool_exposure:"whitelist",tool_allowlist:[]});
      if (openaiConnection()) Object.assign(parameters, {base_url:null,endpoint:null,context_window:0,max_output_tokens:0,temperature:null,top_p:null,reasoning_mode:"auto",thinking_budget:null,enable_llm_tools:false,computer_use_enabled:false,llm_tool_exposure:"whitelist",tool_allowlist:[]});
      if (el("clear-key").checked && !window.confirm("确认清除此会话保存的 API Key？")) return;
      busy = true; el("save").disabled = true; el("fields").disabled = true; el("session-select").disabled = true;
      status("正在保存…");
      try {
        const creating=!selectedId;
        const result = await request(creating ? "/api/sessions/model-settings" : `/api/sessions/${encodeURIComponent(selectedId)}/model-settings`, { session:{...session,base_url:parameters.base_url,endpoint:parameters.endpoint}, parameters, expected_revision: snapshot?.configuration_revision });
        if(creating){selectedId=result.session.id;sessions.push(result.session);renderSessionOptions();}
        if (disposed) return;
        fill(result);
        const index = sessions.findIndex(s => s.id === selectedId);
        if (index >= 0) sessions[index] = result.session;
        renderSessionOptions(); status("配置已保存");
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
      dirty = true; editSequence++; dirtyBadge(); options.onChanged?.(selectedId);
    };
    // input 即时保护尚未失焦的草稿，避免列表自动刷新覆盖第一次键入。
    const discoveryInputs = new Set(["base-url", "endpoint", "protocol", "api-key", "clear-key", "discovery-no-key"]);
    const onInput = event => {
      const key = event.target.dataset.ms;
      if (key === "effort-slider") { const choice=el("reasoning-effort").options?.[Number(value("effort-slider"))]; el("effort-value").textContent=choice?.textContent || "自动"; return; }
      if (key === "discovery-filter") { renderCandidates(); return; }
      if (key === "discovery-model" || key === "discovery-no-key") return;
      if (discoveryInputs.has(key)) resetDiscovery();
      if (key === "model") { renderCandidateDetail(); syncAvatar(); if (devinConnection()) { set("reasoning-effort", "auto"); updateDynamicFields(); } }
      if (el("form")?.contains(event.target)) markDirty();
    };
    const onChange = event => {
      if (event.target === el("session-select")) { void select(value("session-select")); return; }
      const key = event.target.dataset.ms;
      if (key === "effort-slider") { void saveEffort(); return; }
      if (key === "avatar") { avatar=value("avatar"); syncAvatar(); markDirty(); return; }
      if (key === "avatar-upload") {
        const file=event.target.files?.[0]; if (!file) return;
        if (!["image/png","image/webp"].includes(file.type) || file.size>2*1024*1024) { status("请选择不超过 2 MB 的 PNG 或 WebP 图片。",true); return; }
        const id=selectedId, sequence=loadSequence;
        const reader=new FileReader(); reader.onload=async()=>{ try { const result=await request("/api/sessions/avatar",{content:reader.result}); if(disposed || id!==selectedId || sequence!==loadSequence)return; avatar=result.avatar; syncAvatar(); markDirty(); } catch(error){status(`上传失败：${error.message}`,true);} }; reader.readAsDataURL(file); return;
      }
      if(key==="model-type" && !subscriptionConnection()) {
        const kind=value("model-type");set("protocol",kind==="image"?"openai_images_generations":kind==="video"?"openai_videos":provider==="anthropic"?"anthropic_messages":"openai_chat_completions");
        set("endpoint","");generationCapabilities=null;for(const name of generationKeys)set(`gen-${name}`,"");resetDiscovery();
      }
      if(key==="protocol" && !subscriptionConnection()) {
        const protocol=value("protocol");if(protocol==="openai_images_generations")set("model-type","image");else if(protocol==="openai_videos")set("model-type","video");else if(["image","video"].includes(value("model-type")))set("model-type","text");
      }
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
      const avatarChoice = event.target.closest("[data-ms-avatar]")?.dataset.msAvatar;
      if (avatarChoice !== undefined) { avatar = avatarChoice; syncAvatar(); markDirty(); return; }
      const action = event.target.closest("[data-ms-action]")?.dataset.msAction;
      const accessTab=event.target.closest("[data-ms-access]")?.dataset.msAccess;
      const selectedProvider=event.target.closest("[data-ms-provider]")?.dataset.msProvider;
      if (accessTab || selectedProvider) {
        const next=selectedProvider || (accessTab === "subscription" ? "devin" : "custom");
        if (next!==provider) {
          provider=next; devinCatalog=null; generationCapabilities=null; set("model-type","text"); set("endpoint",""); for(const name of generationKeys)set(`gen-${name}`,"");
          const profile=providers.find(item=>item[0]===next);
          set("protocol",profile?.[3] || (next === "devin" ? "devin_acp" : "openai_subscription"));
          if(profile) set("base-url",profile[2]);
          set("api-key",""); set("model",""); set("reasoning-effort","auto");
          updateDynamicFields(); resetDiscovery(); markDirty();
          if(openaiConnection()) void openaiAuth();
        }
      }
      if (action === "save") void save(event);
      if (action?.startsWith("openai-")) void openaiAuth(action.slice(7));
      if (action === "new") void select(null);
      if (action === "discover") void discoverModels();
      if (action === "refresh-plugins") void refreshPluginTools();
      if (action === "rebind-devin") void rebindDevin();
      if (action === "adopt-model") {
        const model = selectedCandidate();
        if (model) { set("model", model.id);
          if(!subscriptionConnection() && ["image","video"].includes(value("model-type"))) generationCapabilities={model:model.id,model_type:value("model-type"),base_url:value("base-url"),fields:model.generation_fields || {},fetched_at:Date.now()};
          if (devinConnection()) set("reasoning-effort", "auto"); updateDynamicFields(); if (devinConnection() && model.reasoning_effort) set("reasoning-effort", model.reasoning_effort); renderCandidateDetail(); markDirty(); }
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
      refresh, select, setSending(ids) { sendingIds=new Set(ids); renderSessionOptions(); }, canLeave() { return !dirty || window.confirm("配置尚未保存，确认离开？"); }, newSession: () => select(null), get sessionId() { return selectedId; }, get dirty() { return dirty; },
      destroy() { clearTimeout(toastTimer); clearTimeout(authPoll); disposed = true; loadSequence++; refreshSequence++; discoverySequence++; discoveryController?.abort(); devinAuth?.destroy(); window.removeEventListener?.("devin-account-changed", accountChanged); container.removeEventListener("input", onInput); container.removeEventListener("change", onChange); container.removeEventListener("click", onClick); el("form")?.removeEventListener("submit", save); },
    };
    container.__modelSettings = api;
    void refresh(selectedId);
    return api;
  }
  window.CoolzhuModelSettings = Object.freeze({ mount });
})();
