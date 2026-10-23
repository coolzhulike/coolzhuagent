const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const srcDir = path.join(__dirname, "..", "src");
const syncSrc = fs.readFileSync(path.join(srcDir, "chat_history_sync.js"), "utf8");
const expSrc = fs.readFileSync(path.join(srcDir, "chat_experience.js"), "utf8");

// ---- 最小 DOM 桩：支持 简单复合选择器 tag/.cls/[data-x="y"] ----
const camel = (s) => s.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
function matchesSel(el, sel) {
  let i = 0;
  const tag = sel.match(/^[a-zA-Z][\w-]*/);
  if (tag) { if (el.tagName !== tag[0].toUpperCase()) return false; i = tag[0].length; }
  const re = /\.([\w-]+)|\[([\w-]+)(?:="([^"]*)")?\]/g; re.lastIndex = i;
  let m;
  while ((m = re.exec(sel))) {
    if (m.index !== i) return false; i = re.lastIndex;
    if (m[1]) { if (!el.classList.contains(m[1])) return false; }
    else {
      const key = m[2].startsWith('data-') ? camel(m[2].slice(5)) : m[2];
      const v = el.dataset ? el.dataset[key] : el.attributes[m[2]];
      if (v === undefined || (m[3] !== undefined && String(v) !== m[3])) return false;
    }
  }
  return i === sel.length;
}
class El {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = []; this.dataset = {}; this.attributes = {}; this._text = "";
    const s = new Set(); this.classList = {
      add: (...c) => c.forEach((x) => s.add(x)), remove: (...c) => c.forEach((x) => s.delete(x)),
      contains: (x) => s.has(x) };
  }
  set className(v) { this.classList = { _set: new Set(String(v).split(/\s+/).filter(Boolean)),
    add: (...c) => c.forEach((x) => this.classList._set.add(x)),
    remove: (...c) => c.forEach((x) => this.classList._set.delete(x)),
    contains: (x) => this.classList._set.has(x) }; }
  get className() { return [...this.classList._set].join(" "); }
  set textContent(v) { this._text = String(v); this.children = []; }
  get textContent() { return this._text + this.children.map((c) => c.textContent || "").join(""); }
  append(...n) { for (const child of n) child.parentNode = this; this.children.push(...n); }
  after(...n) {
    if (!this.parentNode) return;
    for (const child of n) child.parentNode = this.parentNode;
    this.parentNode.children.splice(this.parentNode.children.indexOf(this) + 1, 0, ...n);
  }
  get childElementCount() { return this.children.length; }
  addEventListener(name, listener) { ((this.listeners ||= {})[name] ||= []).push(listener); }
  removeEventListener(name, listener) { this.listeners[name] = (this.listeners[name] || []).filter(x => x !== listener); }
  async click() { await Promise.all((this.listeners?.click || []).map(listener => listener({target:this,button:0}))); }
  focus() {}
  before(...n) {}
  remove() {}
  replaceChildren(...n) { this.children = [...n]; }
  setAttribute(k, v) { this.attributes[k] = String(v); }
  scrollIntoView(opts) { this.scrollCalls = (this.scrollCalls || 0) + 1; this.scrollOpts = opts; }
  matches(sel) { return matchesSel(this, sel); }
  querySelectorAll(sel) { const out = []; const w = (e) => { for (const c of e.children) { if (c.matches(sel)) out.push(c); w(c); } }; w(this); return out; }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function makeFixture({ locatedInDom }) {
  const els = new Map();
  const reg = (role, tag = "div") => { const el = new El(tag); el.dataset.role = role; els.set(role, el); return el; };
  const list = reg("chat-message-list");
  const returnLatest = reg("chat-return-latest", "button"); returnLatest.hidden = true;
  reg("chat-load-older", "button");
  const msg = (id) => { const a = new El("article"); a.classList.add("message"); a.dataset.messageId = id;
    a.append(Object.assign(new El("time"), {})); return a; };
  const target = msg("msg-target"); const latest = msg("msg-new");
  list.append(target, latest);
  if (!locatedInDom) list.replaceChildren(latest);

  const reported = [];
  const composer = new El("div"); composer.before = (n) => reported.push(n.textContent);
  const context = {
    console, JSON, Date, Math, Promise, URLSearchParams, encodeURIComponent, queueMicrotask,
    setTimeout, clearTimeout, setImmediate, CSS: { escape: (s) => String(s).replace(/([^a-zA-Z0-9_-])/g, "\\$1") },
    DOMException, AbortController, location: { origin: "http://x" },
    document: { hidden: false, createElement: (t) => new El(t),
      querySelector: (sel) => { if (sel === ".chat-main-column .composer") return composer;
        const m = sel.match(/^\[data-role="([^"]+)"\]$/); return m ? els.get(m[1]) || null : null; },
      querySelectorAll: () => [], addEventListener: () => {},
      body: new El("body") },
    requestAnimationFrame: (f) => setTimeout(f, 0),
  };
  context.window = context;
  context.window.addEventListener = () => {}; context.window.removeEventListener = () => {};

  // app.js 等价接线（适配器按 DOM 事实判定 busy）：chat-return-latest 可见=浏览历史=暂停同步
  const calls = { apply: 0, read: 0, esOpen: 0 };
  class FakeES {
    constructor(url) { this.url = url; this.listeners = {}; this.readyState = 0; calls.esOpen++;
      queueMicrotask(() => (this.listeners.hello || []).forEach((f) => f())); }
    addEventListener(t, f) { (this.listeners[t] ||= []).push(f); }
    close() { this.readyState = 2; }
  }
  context.EventSource = FakeES;
  const latestResponse = { room: { name: "r" }, messages: [{ id: "msg-new", created_at: 200 }], has_more: false, next_before: null };
  context.activateChatHistorySync = () => {
    context.chatHistorySync?.stop();
    context.chatHistorySync = context.CoolzhuChatHistorySync.create({
      sourceFactory: () => new FakeES("/api/.../history-events"),
      busy: () => Boolean(context.document.querySelector('[data-role="chat-return-latest"]')?.hidden === false),
      read: async () => { calls.read++; return latestResponse; },
      apply: () => { calls.apply++; list.scrollTop = 999999; }, // 模拟 reconcile+锚点恢复拉回最新
      missing: () => {}, permissionChanged: () => {},
    });
    context.chatHistorySync.activate({ room: context.activeChatRoomId, workspace: "w1", workspacePath: "w1", projectGeneration: 0 });
  };
  context.chatMessageList = () => list;
  context.upsertMessage = (m) => { const a = msg(m.id); list.append(a); return a; };
  context.updateLoadOlderButton = () => {};
  context.setWuxiaIconOnly = () => {};
  context.activeChatRoomId = "room-1"; context.activeWorkspaceKey = "w1"; context.projectWorkspaceGeneration = 0;
  context.sessionRegistry = { sessions: [] }; context.activeSessionId = null;
  context.requestJson = async (url) => {
    if (url.includes("/search?")) return { messages: [{ id: "msg-target", created_at: 100 }, { id: "msg-new", created_at: 200 }], has_older: false, position: 1, start_index: 1 };
    throw new Error(`unexpected ${url}`);
  };
  context.refreshInsights = () => {};

  vm.runInNewContext(syncSrc, context, { filename: "chat_history_sync.js" });
  vm.runInNewContext(expSrc, context, { filename: "chat_experience.js" });
  return { context, calls, list, target, latest, returnLatest, reported };
}

test("定位已在DOM中的源消息：标记离开最新窗口，旧同步不得拉回", async () => {
  const { context, calls, returnLatest, target, reported } = makeFixture({ locatedInDom: true });
  context.chatHistorySync = { stop() {} }; // 定位前同步器在跑
  await context.CoolzhuChatExperience.locate("msg-target");
  assert.equal(reported.length, 0, `定位不应报错：${reported.join("|")}`);
  assert.equal(target.scrollCalls, 1, "源消息应被 scrollIntoView");
  assert.equal(returnLatest.hidden, false, "已离开最新窗口须显示回到最新按钮");
  await sleep(300); // 等 hello→invalidate→schedule(120ms)→flush 窗口
  assert.equal(calls.apply, 0, "同步不得在历史定位后应用最新窗口拉回滚动");
  assert.ok(target.classList.contains("is-located-message"), "目标消息应有定位高亮标记");
});

test("定位不在DOM中的源消息：抓取around窗口、标记离开最新、旧同步不覆盖", async () => {
  const { context, calls, returnLatest, list, reported } = makeFixture({ locatedInDom: false });
  context.chatHistorySync = { stop() {} };
  await context.CoolzhuChatExperience.locate("msg-target");
  assert.equal(reported.length, 0, `定位不应报错：${reported.join("|")}`);
  const t = list.querySelector('[data-message-id="msg-target"]');
  assert.ok(t, "around抓取后目标消息应在列表");
  assert.equal(t.scrollCalls, 1);
  assert.equal(returnLatest.hidden, false);
  await sleep(300);
  assert.equal(calls.apply, 0);
});

function makeTraceFixture(respond) {
  const els = new Map();
  for (const role of ["chat-trace-items", "chat-trace-status", "chat-trace-more", "message-input"]) {
    const node = new El("div"); node.dataset.role = role; els.set(role, node);
  }
  const scope = {workspace:"workspace-a",room:"room-a",session:"agent-a"};
  const calls = [];
  let active = "chat";
  const context = {
    console, JSON, Date, Math, Promise, URLSearchParams, encodeURIComponent, AbortController,
    setTimeout, clearTimeout, setWuxiaIconOnly() {},
    localStorage: {getItem:()=>null,setItem(){}},
    document: {
      createElement:tag=>new El(tag), addEventListener(){},
      querySelectorAll:()=>[],
      querySelector(selector) {
        const role = selector.match(/^\[data-role="([^"]+)"\]$/)?.[1];
        if (role && els.has(role)) return els.get(role);
        for (const root of els.values()) {
          const found = root.querySelector(selector); if (found) return found;
        }
        return null;
      },
    },
    fetch: async (url, options) => {
      calls.push(String(url));
      const body = await respond(String(url), options);
      return {ok:true,json:async()=>body};
    },
  };
  context.window = context;
  vm.runInNewContext(fs.readFileSync(path.join(srcDir,"workspace_panels.js"),"utf8"),context);
  context.CoolzhuWorkspacePanels.init({
    scope:()=>({...scope}), active:()=>active, open:id=>{active=id;}, close:()=>{active="chat";},
    collapse(){}, defaultWidth:480,
  });
  // 使用真实app.js的消息按钮接线，避免适配器桩替代需要验证的messageId传递。
  const app = fs.readFileSync(path.join(srcDir,"app.js"),"utf8");
  vm.runInNewContext(app.slice(app.indexOf("function attachMessageTraceLink("),app.indexOf("\nfunction appendMessageText(")),context);
  const button = id => {
    const article = new El("article"); article.dataset.messageId = id;
    const attachments = new El("div"); attachments.classList.add("attachments"); article.append(attachments);
    context.attachMessageTraceLink(article,"assistant","assistant-reply",false);
    return article.querySelector('[data-role="message-trace-link"]');
  };
  return {context,scope,calls,button,items:els.get("chat-trace-items"),status:els.get("chat-trace-status")};
}

const singleRun = (message, run) => ({
  requested_message_id:message, runs:[{run_id:run,status:"completed",created_at:1,calls:[],requests:[]}],
  has_more:false,next_before:null,
});

test("历史回复按钮传真实消息ID，并展开对应的旧运行而非最新列表", async () => {
  const fixture = makeTraceFixture(async url => {
    assert.equal(new URL(url,"http://local").searchParams.get("message_id"),"reply-old");
    return singleRun("reply-old","run-old");
  });
  await fixture.button("reply-old").click();
  assert.equal(fixture.calls.length,1,"精确定位不读取无关历史归档");
  const rows = fixture.items.querySelectorAll(".chat-trace-entry");
  assert.equal(rows.length,1);
  assert.equal(rows[0].dataset.runId,"run-old");
  assert.equal(rows[0].open,true);
  assert.equal(rows[0].scrollCalls,1);
  assert.match(fixture.status.textContent,/已定位这条回复/);
});

test("旧服务忽略消息定位参数时拒绝展示未确认的运行", async () => {
  const fixture = makeTraceFixture(async () => ({runs:[{run_id:"unrelated-latest",calls:[]}],has_more:false}));
  await fixture.button("reply-old").click();
  assert.equal(fixture.items.querySelectorAll(".chat-trace-entry").length,0);
  assert.match(fixture.status.textContent,/未收到这条回复的唯一运行记录/);
});

test("切换房间后迟到的回复定位结果不能写回新作用域", async () => {
  let resolve;
  const fixture = makeTraceFixture(() => new Promise(done => {resolve=done;}));
  const pending = fixture.button("reply-old").click();
  fixture.scope.room = "room-b";
  fixture.context.CoolzhuWorkspacePanels.scopeChanged();
  resolve(singleRun("reply-old","old-room-run"));
  await pending;
  assert.equal(fixture.items.childElementCount,0);
});

test("快速定位另一条回复时，较早请求的迟到结果不能覆盖", async () => {
  let resolveOld;
  const fixture = makeTraceFixture(url => {
    const message = new URL(url,"http://local").searchParams.get("message_id");
    return message === "reply-a" ? new Promise(done => {resolveOld=done;}) : singleRun(message,"run-b");
  });
  const older = fixture.button("reply-a").click();
  await fixture.button("reply-b").click();
  resolveOld(singleRun("reply-a","run-a"));
  await older;
  const rows = fixture.items.querySelectorAll(".chat-trace-entry");
  assert.equal(rows.length,1);
  assert.equal(rows[0].dataset.runId,"run-b");
});

test("跨工程切换后同房间同消息ID的迟到结果不得写回", async () => {
  let resolve;
  const fixture = makeTraceFixture(() => new Promise(done => {resolve=done;}));
  const pending = fixture.button("reply-same").click();
  fixture.scope.workspace = "workspace-b";
  fixture.context.CoolzhuWorkspacePanels.scopeChanged();
  resolve(singleRun("reply-same","workspace-a-run"));
  await pending;
  assert.equal(fixture.items.childElementCount,0);
});

test("会话A切B再回A时，首个请求即使作用域相同也已经失效", async () => {
  let resolveOld;
  let count = 0;
  const fixture = makeTraceFixture(() => ++count === 1
    ? new Promise(done => {resolveOld=done;}) : singleRun("reply-same","fresh-run"));
  const pending = fixture.button("reply-same").click();
  fixture.scope.session = "agent-b";
  fixture.context.CoolzhuWorkspacePanels.scopeChanged();
  fixture.scope.session = "agent-a";
  fixture.context.CoolzhuWorkspacePanels.scopeChanged();
  await fixture.button("reply-same").click();
  resolveOld(singleRun("reply-same","stale-run"));
  await pending;
  const rows = fixture.items.querySelectorAll(".chat-trace-entry");
  assert.equal(rows.length,1);
  assert.equal(rows[0].dataset.runId,"fresh-run");
});

test("逐回复刷新迟到工具终态时保持运行唯一且不重复工具", async () => {
  let complete = false;
  const fixture = makeTraceFixture(async () => {
    const result = singleRun("reply-same","owned-run");
    result.runs[0].calls = [{call_id:"call-same",tool_name:"read_file",status:complete?"completed":"running",started_at:1,updated_at:1001}];
    return result;
  });
  await fixture.button("reply-same").click();
  complete = true;
  await fixture.button("reply-same").click();
  const rows = fixture.items.querySelectorAll(".chat-trace-entry");
  assert.equal(rows.length,1);
  assert.equal(rows[0].dataset.runId,"owned-run");
  const facts = rows[0].querySelectorAll("p").filter(p=>p.textContent.startsWith("read_file ·"));
  assert.equal(facts.length,1);
  assert.match(facts[0].textContent,/已完成 · 1\.0 秒/);
});
