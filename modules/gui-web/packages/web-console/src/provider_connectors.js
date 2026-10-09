/* 连接器只提供统一配置入口；生成、工具、上下文与记忆仍由 HTTP 会话运行时负责。 */
(() => {
  "use strict";
  const profiles = Object.freeze({
    "opencode-zen@native": {
      name: "OpenCode Zen", baseUrl: "https://opencode.ai/zen/v1",
      docs: "https://opencode.ai/docs/zen/",
      note: "仅列出2026-10-07核对的免费聊天模型，并与实时目录取交集；供应及条款以官网为准，不会自动替换付费模型。部分免费模型可能用于训练，请先查看官网隐私说明。",
      // 官方端点和价格表精确列表；不把名称含free当作价格证据。
      modelIds: new Set(["big-pickle", "space-bunny-free", "longcat-2.5-preview-free", "exo-free", "fledge-alpha-free", "mimo-v2.6-flash-free", "mimo-v2.5-free", "ling-3.1-flash-free", "ling-3.0-flash-fin-free", "nemotron-3-ultra-free", "nemotron-3.5-lightning-free"]),
    },
    "huggingface-inference@native": {
      name: "Hugging Face Inference", baseUrl: "https://router.huggingface.co/v1",
      docs: "https://huggingface.co/docs/inference-providers/en/pricing",
      note: "使用 Hugging Face Token；普通免费账户目前每月有 $0.10 推理额度，可能调整。可推理模型不代表全部无限免费；账户用量及收费由官网管理。模型 ID 保留 :provider 路由后缀。",
    },
  });
  const editors = new Map();
  function makeEditor(id) {
    const panel = document.querySelector(`[data-provider-connector="${id}"]`);
    if (!panel) return null;
    if (!editors.has(id)) {
      const profile = profiles[id];
      panel.querySelector('[data-role="provider-note"]').textContent = profile.note;
      editors.set(id, window.CoolzhuModelSettings.mount(panel.querySelector('[data-role="provider-editor"]'), {
        workspacePath: document.querySelector('[data-role="overview-workspace-name"]')?.dataset.workspacePath || "",
        backendFilter: "llm_http", initialDraft: profile,
        modelFilter: profile.modelIds ? model => profile.modelIds.has(model.id) && model.is_free !== false : null,
        onSaved: async () => { await loadSessions(); await loadAgents(); },
      }));
    }
    return editors.get(id);
  }
  for (const [id, profile] of Object.entries(profiles)) {
    const panel = document.querySelector(`[data-provider-connector="${id}"]`);
    panel?.addEventListener("toggle", () => { if (panel.open) makeEditor(id); });
    panel?.querySelector('[data-action="provider-docs"]')?.addEventListener("click", async () => {
      openChatToolWindow("browser");
      const address = document.querySelector('[data-role="browser-window-input"]');
      if (address) { address.value = profile.docs; await browserWindowNavigate(); }
    });
  }
  window.addEventListener("coolzhu-provider-configure", event => {
    const id = event.detail?.id;
    if (!profiles[id]) return;
    const panel = document.querySelector(`[data-provider-connector="${id}"]`);
    if (panel) { panel.open = true; makeEditor(id); panel.scrollIntoView({block:"start", behavior:"smooth"}); }
  });
  window.addEventListener("coolzhu-workspace-changed", () => {
    for (const editor of editors.values()) editor?.destroy();
    editors.clear();
    for (const panel of document.querySelectorAll("[data-provider-connector]")) {
      panel.querySelector('[data-role="provider-editor"]').replaceChildren();
      if (panel.open) makeEditor(panel.dataset.providerConnector);
    }
  });
})();
