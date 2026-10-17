const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const srcDir = path.join(__dirname, "..", "src");
const brandSrc = fs.readFileSync(path.join(srcDir, "brand_avatars.js"), "utf8");
const panelSrc = fs.readFileSync(path.join(srcDir, "execution_panel.js"), "utf8");

// 最小 DOM 桩：仅存 children/dataset/attributes，不做渲染。
class El {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.dataset = {};
    this.attributes = {};
    this._text = "";
  }
  set textContent(value) { this._text = String(value); this.children = []; }
  get textContent() { return this._text + this.children.map((c) => c.textContent || "").join(""); }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = [...nodes]; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
}
const walk = (el, fn) => { for (const child of el.children) { fn(child); walk(child, fn); } };
const findAll = (el, pred) => { const out = []; walk(el, (n) => { if (pred(n)) out.push(n); }); return out; };

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
};
const flush = async () => { for (let i = 0; i < 12; i++) await new Promise((r) => setImmediate(r)); };

// executions：数组 → 每次 /api/executions 都返回该列表；{queue:[...]} → 每次消费一项，{defer:true} 保持挂起由测试手动 resolve。
function buildPanel({ executions }) {
  const listeners = {};
  const fetches = [];
  const queue = executions && executions.queue;
  const context = {
    console, AbortController, DOMException, queueMicrotask, encodeURIComponent, JSON, Date, Math,
    setInterval: () => 1, clearInterval: () => {},
    MutationObserver: class { observe() {} disconnect() {} },
    document: { hidden: false, createElement: (tag) => new El(tag) },
  };
  context.window = context;
  context.window.addEventListener = (type, cb) => { listeners[type] = cb; };
  context.window.removeEventListener = (type) => { delete listeners[type]; };
  let scope = { workspace: "w1", generation: 0 };
  context.fetch = (url, init = {}) => {
    const call = { url, init, ...deferred() };
    fetches.push(call);
    if (url === "/api/workspace") {
      const ws = scope.workspace;
      call.resolve({ ok: true, json: async () => ({ workspace: ws, workspace_id: `id-${ws}`, configuration_scope: "cfg" }) });
    } else if (url === "/api/executions") {
      const body = queue ? queue.shift() : { executions };
      if (!body.defer) call.resolve({ ok: true, json: async () => body });
    }
    return call.promise;
  };
  vm.runInNewContext(brandSrc, context, { filename: "brand_avatars.js" });
  vm.runInNewContext(panelSrc, context, { filename: "execution_panel.js" });
  const BA = context.CoolzhuBrandAvatars;
  const sessions = new Map();
  const sessionById = (id) => sessions.get(id) || null;
  const getAvatarCalls = [];
  const root = new El("div");
  const api = context.CoolzhuExecutionPanel.mount(root, {
    getScope: () => scope,
    // 与 app.js initializeChatRightRailTabs 相同的真实接线：按每张卡的 spec + 其所属会话的头像解析。
    getAvatar: (spec) => {
      getAvatarCalls.push(spec);
      return BA.url(BA.resolve({
        id: spec.session_id, avatar: sessionById(spec.session_id)?.avatar,
        provider: spec.provider, model: spec.model,
      }));
    },
  });
  return { api, root, fetches, sessions, getAvatarCalls, listeners,
    setScope: (workspace) => { scope = { workspace, generation: 0 }; },
    emitWorkspaceChanged: () => listeners["coolzhu-workspace-changed"](),
    list: () => root.children[3] };
}

const item = (id, spec, extra = {}) => ({
  status: "completed", elapsed_ms: 1500, input_tokens: 10, output_tokens: 20,
  spec: { id, source: "session_tool", deadline_ms: 0, room_id: "r1", source_id: "src", ...spec },
  ...extra,
});
const cardAvatars = (list) => findAll(list, (n) => n.tagName === "ARTICLE").map((card) => {
  const img = findAll(card, (n) => n.tagName === "IMG")[0];
  return img ? img.src : null;
});

test("历史执行卡片按各自spec模型与会话头像解析，活动会话切换不改旧卡", async () => {
  const { api, sessions, getAvatarCalls, fetches, list } = buildPanel({ executions: [
    item("exec-opus", { provider: "anthropic", model: "claude-opus-5-5-high", session_id: "s-opus", summary: "Claude旧执行" }),
    item("exec-swe", { provider: "devin", model: "swe-2-max", session_id: "s-devin", summary: "SWE旧执行" }),
    item("exec-unknown", { provider: "mystery", model: "alien-9-ultra", session_id: "s-gone", summary: "未知模型" }),
    item("exec-custom", { provider: "mystery", model: "custom-model", session_id: "s-custom", summary: "自定义头像" }),
  ] });
  sessions.set("s-opus", { id: "s-opus", avatar: "assets/avatars/wuxia-v3/feather-guard.png" });
  sessions.set("s-devin", { id: "s-devin", avatar: "assets/avatars/wuxia-v3/jade-strategist.png" });
  sessions.set("s-custom", { id: "s-custom", avatar: "custom-avatars/hero.png" });
  sessions.set("s-active", { id: "s-active", avatar: "assets/avatars/wuxia-v3/gold-commander.png", model: "glm-4.6" });
  await flush();
  const expected = [
    "./assets/avatars/brand-v1/claude.png",
    "./assets/avatars/brand-v1/generic.png",
    "./assets/avatars/brand-v1/generic.png",
    "./custom-avatars/hero.png",
  ];
  assert.deepEqual(cardAvatars(list()), expected);
  assert.deepEqual(getAvatarCalls.map((s) => s.session_id), ["s-opus", "s-devin", "s-gone", "s-custom"]);
  assert.deepEqual(getAvatarCalls.map((s) => s.model),
    ["claude-opus-5-5-high", "swe-2-max", "alien-9-ultra", "custom-model"]);
  // 活动会话切换为另一模型后强制刷新，旧卡仍按各自spec解析。
  sessions.set("s-active", { id: "s-active", avatar: null, model: "gpt-5" });
  await api.refresh();
  assert.deepEqual(cardAvatars(list()), expected);
  assert.deepEqual(fetches.map((c) => c.url), ["/api/workspace", "/api/executions", "/api/executions"]);
  api.dispose();
});

test("跨workspace迟到的旧响应不重绘当前卡片列表", async () => {
  const panel = buildPanel({ executions: { queue: [
    { defer: true },
    { executions: [item("exec-w2", { provider: "devin", model: "swe-2-max", session_id: "s-w2", summary: "w2执行" })] },
  ] } });
  await flush();
  const w1ExecutionsCall = panel.fetches.find((c) => c.url === "/api/executions");
  assert.equal(w1ExecutionsCall.init.headers["X-Coolzhu-Workspace-Id"], "id-w1");
  // 在途时切到w2：作废w1响应并重新拉取。
  panel.setScope("w2");
  panel.emitWorkspaceChanged();
  await flush();
  const w2ExecutionsCall = panel.fetches.filter((c) => c.url === "/api/executions")[1];
  assert.equal(w2ExecutionsCall.init.headers["X-Coolzhu-Workspace-Id"], "id-w2");
  // w1迟到响应不得污染：最终列表只有w2的一张卡。
  w1ExecutionsCall.resolve({ ok: true, json: async () => ({ executions: [
    item("exec-w1-a", { provider: "anthropic", model: "claude-opus-5-5-high", session_id: "s-w1" }),
    item("exec-w1-b", { provider: "devin", model: "swe-2-max", session_id: "s-w1" }),
  ] }) });
  await flush();
  const cards = findAll(panel.list(), (n) => n.tagName === "ARTICLE");
  assert.equal(cards.length, 1);
  assert.match(cards[0].textContent, /swe-2-max \/ s-w2/);
  assert.equal(cardAvatars(panel.list())[0], "./assets/avatars/brand-v1/generic.png");
  panel.api.dispose();
});
