import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const serviceWorkerSource = await readFile(
  new URL("../service_worker.js", import.meta.url),
  "utf8",
);
const contentScriptSource = await readFile(
  new URL("../content_script.js", import.meta.url),
  "utf8",
);

const clone = (value) => JSON.parse(JSON.stringify(value));

function eventHook() {
  const listeners = [];
  return {
    addListener(listener) {
      listeners.push(listener);
    },
    emit(...args) {
      for (const listener of [...listeners]) listener(...args);
    },
    first() {
      return listeners[0];
    },
  };
}

async function waitFor(predicate, message) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const value = predicate();
    if (value) return value;
    await new Promise((resolve) => setImmediate(resolve));
  }
  throw new Error(message);
}

function createServiceWorkerState({ storedTabs = {}, tabs = [] } = {}) {
  return {
    stored: {
      coolzhu_owned_tabs_v1: {
        version: 1,
        tabs: clone(storedTabs),
      },
    },
    tabs: new Map(tabs.map((tab) => [tab.id, { ...tab }])),
    nextTabId: 100,
    storageWrites: [],
    removedTabs: [],
    posted: [],
    retryDelays: [],
    injectionFailures: 0,
    receiverFailures: 0,
    injected: 0,
  };
}

function runServiceWorker(state) {
  const onNativeMessage = eventHook();
  const onNativeDisconnect = eventHook();
  const onInstalled = eventHook();
  const onStartup = eventHook();
  const onRemoved = eventHook();
  const onReplaced = eventHook();
  const port = {
    onMessage: onNativeMessage,
    onDisconnect: onNativeDisconnect,
    postMessage(response) {
      state.posted.push(clone(response));
    },
  };

  const tabsApi = {
    async query(query) {
      const tabs = [...state.tabs.values()].map((tab) => ({ ...tab }));
      return query?.active ? tabs.filter((tab) => tab.active) : tabs;
    },
    async get(tabId) {
      const tab = state.tabs.get(Number(tabId));
      if (!tab) throw new Error(`No tab with id: ${tabId}`);
      return { ...tab };
    },
    async create({ url, active }) {
      const id = state.nextTabId;
      state.nextTabId += 1;
      const tab = { id, windowId: 1, url, title: "owned", active };
      state.tabs.set(id, tab);
      return { ...tab };
    },
    async update(tabId, changes) {
      const current = state.tabs.get(Number(tabId));
      if (!current) throw new Error(`No tab with id: ${tabId}`);
      Object.assign(current, changes);
      return { ...current };
    },
    async remove(tabId) {
      const numericId = Number(tabId);
      if (!state.tabs.delete(numericId)) throw new Error(`No tab with id: ${tabId}`);
      state.removedTabs.push(numericId);
      queueMicrotask(() => onRemoved.emit(numericId, { windowId: 1, isWindowClosing: false }));
    },
    async sendMessage(_tabId, request) {
      if (state.receiverFailures > 0) {
        state.receiverFailures -= 1;
        throw new Error("Could not establish connection. Receiving end does not exist.");
      }
      return {
        request_id: request.request_id,
        ok: true,
        document_id: "document-test",
        dom_revision: 1,
        nodes: [],
      };
    },
    async goBack() {},
    async goForward() {},
    onRemoved,
    onReplaced,
  };

  const chrome = {
    runtime: {
      connectNative() {
        return port;
      },
      onInstalled,
      onStartup,
    },
    tabs: tabsApi,
    scripting: {
      async executeScript() {
        state.injected += 1;
        if (state.injectionFailures > 0) {
          state.injectionFailures -= 1;
          throw new Error("Could not establish connection. Receiving end does not exist.");
        }
      },
    },
    storage: {
      session: {
        async setAccessLevel() {},
        async get(key) {
          return { [key]: clone(state.stored[key]) };
        },
        async set(value) {
          Object.assign(state.stored, clone(value));
          state.storageWrites.push(clone(value));
        },
      },
    },
  };

  const context = vm.createContext({
    chrome,
    fetch: async () => ({ ok: true }),
    setTimeout(callback, delayMs) {
      state.retryDelays.push(delayMs);
      queueMicrotask(callback);
      return state.retryDelays.length;
    },
    clearTimeout() {},
    console,
    Promise,
  });
  vm.runInContext(serviceWorkerSource, context, { filename: "service_worker.js" });

  async function request(message) {
    const before = state.posted.length;
    onNativeMessage.first()(clone(message));
    return waitFor(
      () => state.posted.length > before && state.posted.at(-1),
      `native response timed out for ${message.request_id}`,
    );
  }

  return {
    events: { onRemoved, onReplaced, onInstalled, onStartup },
    request,
  };
}

test("owned tab lease survives a service-worker restart and remains owner-bound", async () => {
  const state = createServiceWorkerState({
    tabs: [{ id: 7, windowId: 1, url: "https://example.test/user", active: true }],
  });
  const first = runServiceWorker(state);
  const opened = await first.request({
    request_id: "open-1",
    type: "tab",
    action: {
      action: "open",
      url: "https://example.test/owned",
      owner_token: "task-owner-a",
    },
  });
  assert.equal(opened.ok, true);
  assert.equal(state.stored.coolzhu_owned_tabs_v1.tabs[opened.tab_id], "task-owner-a");

  const restarted = runServiceWorker(state);
  const denied = await restarted.request({
    request_id: "close-wrong-owner",
    type: "tab",
    action: {
      action: "close_owned",
      tab_id: opened.tab_id,
      owner_token: "task-owner-b",
    },
  });
  assert.equal(denied.ok, false);
  assert.equal(denied.error.code, "tab_not_owned");

  const closed = await restarted.request({
    request_id: "close-right-owner",
    type: "tab",
    action: {
      action: "close_owned",
      tab_id: opened.tab_id,
      owner_token: "task-owner-a",
    },
  });
  assert.equal(closed.ok, true);
  await waitFor(
    () => Object.keys(state.stored.coolzhu_owned_tabs_v1.tabs).length === 0,
    "closed tab lease was not removed from session storage",
  );
});

test("tab replacement transfers the lease and tab removal clears it", async () => {
  const state = createServiceWorkerState({
    tabs: [{ id: 8, windowId: 1, url: "https://example.test/user", active: true }],
  });
  const worker = runServiceWorker(state);
  const opened = await worker.request({
    request_id: "open-replaced",
    type: "tab",
    action: {
      action: "open",
      url: "https://example.test/owned",
      owner_token: "task-replaced",
    },
  });
  const oldId = Number(opened.tab_id);
  const newId = oldId + 1000;
  const oldTab = state.tabs.get(oldId);
  state.tabs.delete(oldId);
  state.tabs.set(newId, { ...oldTab, id: newId });
  worker.events.onReplaced.emit(newId, oldId);
  await waitFor(
    () => state.stored.coolzhu_owned_tabs_v1.tabs[String(newId)] === "task-replaced",
    "replacement tab did not inherit the task lease",
  );
  assert.equal(state.stored.coolzhu_owned_tabs_v1.tabs[String(oldId)], undefined);

  state.tabs.delete(newId);
  worker.events.onRemoved.emit(newId, { windowId: 1, isWindowClosing: false });
  await waitFor(
    () => Object.keys(state.stored.coolzhu_owned_tabs_v1.tabs).length === 0,
    "removed tab lease was not cleared",
  );
});

test("content bridge injection uses bounded exponential backoff", async () => {
  const state = createServiceWorkerState({
    tabs: [{ id: 9, windowId: 1, url: "https://example.test/form", active: true }],
  });
  state.receiverFailures = 1;
  state.injectionFailures = 2;
  const worker = runServiceWorker(state);
  const response = await worker.request({
    request_id: "snapshot-with-backoff",
    type: "snapshot",
  });
  assert.equal(response.ok, true);
  assert.equal(state.injected, 3);
  assert.deepEqual(state.retryDelays.filter((delay) => delay > 0), [100, 250]);
});

class FakeEvent {
  constructor(type, init = {}) {
    this.type = type;
    this.bubbles = Boolean(init.bubbles);
    this.cancelable = Boolean(init.cancelable);
    this.defaultPrevented = false;
    Object.assign(this, init);
  }

  preventDefault() {
    if (this.cancelable) this.defaultPrevented = true;
  }
}

class FakeKeyboardEvent extends FakeEvent {}
class FakeInputEvent extends FakeEvent {}
class FakeMouseEvent extends FakeEvent {}

class FakeElement {
  constructor(tagName, attributes = {}) {
    this.tagName = tagName.toUpperCase();
    this.attributes = new Map(Object.entries(attributes));
    this.disabled = false;
    this.isConnected = true;
    this.isContentEditable = false;
    this.innerText = "";
    this.textContent = "";
    this.listeners = new Map();
    this.form = null;
    this.clickCount = 0;
  }

  get id() {
    return this.getAttribute("id") || "";
  }

  getAttribute(name) {
    return this.attributes.has(name) ? this.attributes.get(name) : null;
  }

  setAttribute(name, value) {
    this.attributes.set(name, String(value));
  }

  closest(selector) {
    if (selector === "form") return this.form;
    return null;
  }

  getBoundingClientRect() {
    return { left: 0, top: 0, width: 120, height: 32 };
  }

  addEventListener(type, listener, options = {}) {
    const entries = this.listeners.get(type) || [];
    entries.push({ listener, once: Boolean(options?.once) });
    this.listeners.set(type, entries);
  }

  removeEventListener(type, listener) {
    this.listeners.set(
      type,
      (this.listeners.get(type) || []).filter((entry) => entry.listener !== listener),
    );
  }

  dispatchEvent(event) {
    event.target = this;
    const entries = [...(this.listeners.get(event.type) || [])];
    for (const entry of entries) {
      entry.listener.call(this, event);
      if (entry.once) this.removeEventListener(event.type, entry.listener);
    }
    return !event.defaultPrevented;
  }

  focus() {
    this.focused = true;
  }

  blur() {
    this.focused = false;
  }

  click() {
    this.clickCount += 1;
    this.dispatchEvent(new FakeMouseEvent("click", { bubbles: true, cancelable: true }));
  }

  scrollBy() {}
}

class FakeInputElement extends FakeElement {
  constructor(type = "text", attributes = {}) {
    super("input", attributes);
    this.type = type;
    this.value = "";
    this.checked = false;
    this.selected = false;
  }

  select() {
    this.selectedText = true;
  }
}

class FakeTextAreaElement extends FakeElement {
  constructor() {
    super("textarea");
    this.value = "";
  }

  select() {
    this.selectedText = true;
  }
}

class FakeSelectElement extends FakeElement {
  constructor() {
    super("select");
    this.value = "";
  }
}

class FakeButtonElement extends FakeElement {
  constructor(type = "submit", attributes = {}) {
    super("button", attributes);
    this.type = type;
  }
}

class FakeFormElement extends FakeElement {
  constructor() {
    super("form");
    this.submitCount = 0;
    this.submitter = null;
  }

  requestSubmit(submitter) {
    this.submitCount += 1;
    this.submitter = submitter || null;
    this.dispatchEvent(new FakeEvent("submit", { bubbles: true, cancelable: true }));
  }

  querySelector() {
    return this.submitterCandidate || null;
  }
}

function runContentScript(targets) {
  const runtimeMessage = eventHook();
  const documentElement = new FakeElement("html");
  const body = new FakeElement("body");
  const document = {
    documentElement,
    body,
    title: "fixture",
    querySelectorAll() {
      return targets;
    },
    querySelector() {
      return null;
    },
    getElementById() {
      return null;
    },
    createRange() {
      return { selectNodeContents() {} };
    },
  };
  const window = {
    innerHeight: 800,
    getSelection() {
      return {
        removeAllRanges() {},
        addRange() {},
      };
    },
    scrollBy() {},
  };
  const context = vm.createContext({
    chrome: { runtime: { onMessage: runtimeMessage } },
    document,
    window,
    location: { href: "https://example.test/form" },
    getComputedStyle: () => ({ visibility: "visible", display: "block" }),
    MutationObserver: class {
      constructor(callback) {
        this.callback = callback;
      }

      observe() {}
    },
    HTMLInputElement: FakeInputElement,
    HTMLTextAreaElement: FakeTextAreaElement,
    HTMLSelectElement: FakeSelectElement,
    HTMLButtonElement: FakeButtonElement,
    HTMLFormElement: FakeFormElement,
    KeyboardEvent: FakeKeyboardEvent,
    InputEvent: FakeInputEvent,
    MouseEvent: FakeMouseEvent,
    Event: FakeEvent,
    CSS: { escape: (value) => String(value) },
    crypto: { randomUUID: () => "document-contract-test" },
    console,
  });
  vm.runInContext(contentScriptSource, context, { filename: "content_script.js" });

  function message(request) {
    let response;
    runtimeMessage.first()(clone(request), {}, (value) => {
      response = value;
    });
    return response;
  }

  const snapshot = message({ request_id: "snapshot", type: "snapshot" });
  return { message, snapshot };
}

test("plain Enter uses requestSubmit once and respects a page keydown cancellation", () => {
  const form = new FakeFormElement();
  const submitter = new FakeButtonElement("submit");
  submitter.form = form;
  form.submitterCandidate = submitter;
  const input = new FakeInputElement("text");
  input.form = form;
  const bridge = runContentScript([input, submitter]);

  const submitted = bridge.message({
    request_id: "enter-submit",
    type: "act",
    expected_document_id: bridge.snapshot.document_id,
    action: { action: "key_combination", target: "dom-1", keys: ["enter"] },
  });
  assert.equal(submitted.ok, true);
  assert.equal(form.submitCount, 1);
  assert.equal(form.submitter, submitter);
  assert.match(submitted.evidence, /semantic=form_request_submit/);

  input.addEventListener("keydown", (event) => event.preventDefault());
  const cancelled = bridge.message({
    request_id: "enter-cancelled",
    type: "act",
    expected_document_id: bridge.snapshot.document_id,
    action: { action: "key_combination", target: "dom-1", keys: ["enter"] },
  });
  assert.equal(cancelled.ok, true);
  assert.equal(form.submitCount, 1);
  assert.match(cancelled.evidence, /semantic=page_handled/);
});

test("Enter does not double-submit when the page handled keydown synchronously", () => {
  const form = new FakeFormElement();
  const input = new FakeInputElement("text");
  input.form = form;
  input.addEventListener("keydown", () => form.requestSubmit());
  const bridge = runContentScript([input]);
  const response = bridge.message({
    request_id: "enter-page-handler",
    type: "act",
    expected_document_id: bridge.snapshot.document_id,
    action: { action: "key_combination", target: "dom-1", keys: ["enter"] },
  });
  assert.equal(response.ok, true);
  assert.equal(form.submitCount, 1);
  assert.match(response.evidence, /semantic=page_handled/);
});

test("Enter click fallback is bounded to the explicitly targeted button", () => {
  const button = new FakeButtonElement("button", { role: "button" });
  const unrelated = new FakeButtonElement("button");
  const bridge = runContentScript([button, unrelated]);
  const response = bridge.message({
    request_id: "enter-button",
    type: "act",
    expected_document_id: bridge.snapshot.document_id,
    action: { action: "key_combination", target: "dom-1", keys: ["enter"] },
  });
  assert.equal(response.ok, true);
  assert.equal(button.clickCount, 1);
  assert.equal(unrelated.clickCount, 0);
  assert.match(response.evidence, /semantic=target_click/);
  assert.doesNotMatch(contentScriptSource, /\bform\.submit\s*\(/);
});
