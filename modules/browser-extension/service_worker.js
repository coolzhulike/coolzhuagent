"use strict";

const HOST_NAME = "com.coolzhu.agent.browser_bridge";
const OWNED_TABS_STORAGE_KEY = "coolzhu_owned_tabs_v1";
const CONTENT_BRIDGE_RETRY_DELAYS_MS = [0, 100, 250, 500, 1000, 2000];
let nativePort = null;
let reconnectTimer = null;
let reconnectAttempt = 0;
const ownedTabs = new Map();
let ownedTabsRestorePromise = null;
let ownedTabsMutationQueue = Promise.resolve();

function failure(requestId, code, message) {
  return { request_id: requestId || "missing", ok: false, error: { code, message } };
}

function normalUrl(url) {
  return typeof url === "string" && (url.startsWith("http://") || url.startsWith("https://"));
}

function validOwnerToken(value) {
  return typeof value === "string" && value.length > 0 && value.length <= 512;
}

function serializedOwnedTabs() {
  return Object.fromEntries(
    [...ownedTabs.entries()]
      .sort(([left], [right]) => left - right)
      .map(([tabId, ownerToken]) => [String(tabId), ownerToken]),
  );
}

async function persistOwnedTabs() {
  await chrome.storage.session.set({
    [OWNED_TABS_STORAGE_KEY]: {
      version: 1,
      tabs: serializedOwnedTabs(),
    },
  });
}

async function restoreOwnedTabs() {
  try {
    if (typeof chrome.storage.session.setAccessLevel === "function") {
      await chrome.storage.session.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
    }
    const stored = await chrome.storage.session.get(OWNED_TABS_STORAGE_KEY);
    const payload = stored?.[OWNED_TABS_STORAGE_KEY];
    const records = payload?.version === 1 && payload.tabs && typeof payload.tabs === "object"
      ? payload.tabs
      : {};
    const liveTabs = await chrome.tabs.query({});
    const liveTabIds = new Set(
      liveTabs
        .map((tab) => Number(tab?.id))
        .filter((tabId) => Number.isInteger(tabId)),
    );
    ownedTabs.clear();
    for (const [rawTabId, ownerToken] of Object.entries(records)) {
      const tabId = Number(rawTabId);
      if (Number.isInteger(tabId) && liveTabIds.has(tabId) && validOwnerToken(ownerToken)) {
        ownedTabs.set(tabId, ownerToken);
      }
    }
    if (ownedTabs.size !== Object.keys(records).length) {
      await persistOwnedTabs();
    }
  } catch (_error) {
    ownedTabs.clear();
    throw new Error("owned_tab_state_unavailable");
  }
}

function ensureOwnedTabsRestored() {
  if (!ownedTabsRestorePromise) {
    ownedTabsRestorePromise = restoreOwnedTabs();
  }
  return ownedTabsRestorePromise;
}

function mutateOwnedTabs(mutator) {
  const task = ownedTabsMutationQueue.then(async () => {
    await ensureOwnedTabsRestored();
    const outcome = await mutator();
    if (outcome?.changed) await persistOwnedTabs();
    return outcome?.value;
  });
  ownedTabsMutationQueue = task.catch(() => undefined);
  return task;
}

function wait(delayMs) {
  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

function missingContentReceiver(error) {
  const message = String(error?.message || error || "").toLowerCase();
  return message.includes("receiving end does not exist")
    || message.includes("could not establish connection")
    || message.includes("no tab with id");
}

function transientContentInjectionFailure(error) {
  const message = String(error?.message || error || "").toLowerCase();
  return missingContentReceiver(error)
    || message.includes("frame with id 0 was removed")
    || message.includes("frame was removed")
    || message.includes("document was unloaded");
}

async function sendContentRequest(tabId, request) {
  try {
    const response = await chrome.tabs.sendMessage(tabId, request, { frameId: 0 });
    if (response !== undefined) return response;
  } catch (error) {
    if (!missingContentReceiver(error)) throw error;
  }

  let lastError = new Error("content_bridge_unavailable");
  for (const delayMs of CONTENT_BRIDGE_RETRY_DELAYS_MS) {
    if (delayMs > 0) await wait(delayMs);
    await normalTabById(tabId);
    try {
      await chrome.scripting.executeScript({
        target: { tabId, frameIds: [0] },
        files: ["content_script.js"],
      });
    } catch (error) {
      if (!transientContentInjectionFailure(error)) throw error;
      lastError = error;
      continue;
    }
    try {
      const response = await chrome.tabs.sendMessage(tabId, request, { frameId: 0 });
      if (response !== undefined) return response;
      lastError = new Error("content_bridge_empty_response");
    } catch (error) {
      if (!missingContentReceiver(error)) throw error;
      lastError = error;
    }
  }
  throw new Error(
    missingContentReceiver(lastError) ? "content_bridge_unavailable" : String(lastError?.message || lastError),
  );
}

async function activeNormalTab() {
  const tabs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const tab = tabs.find((candidate) => Number.isInteger(candidate.id) && normalUrl(candidate.url));
  if (!tab) throw new Error("restricted_page");
  return tab;
}

async function normalTabById(tabId) {
  const numericId = Number(tabId);
  if (!Number.isInteger(numericId)) throw new Error("invalid_tab_id");
  let tab;
  try {
    tab = await chrome.tabs.get(numericId);
  } catch (_error) {
    throw new Error("tab_closed");
  }
  if (!normalUrl(tab?.url)) throw new Error("restricted_page");
  return tab;
}

async function requestTab(request) {
  return request?.tab_id ? normalTabById(request.tab_id) : activeNormalTab();
}

async function relayTabAction(request) {
  const requestId = String(request.request_id || "missing");
  const action = request.action || {};
  if (action.action === "open") {
    const url = String(action.url || "");
    const ownerToken = String(action.owner_token || "");
    if (!normalUrl(url)) return failure(requestId, "restricted_url", "only http and https URLs are allowed");
    if (!validOwnerToken(ownerToken)) {
      return failure(requestId, "invalid_owner_token", "owner token is required and must be bounded");
    }
    const tab = await chrome.tabs.create({ url, active: action.activate !== false });
    if (!Number.isInteger(tab?.id)) {
      return failure(requestId, "invalid_tab_response", "opened tab id is missing");
    }
    try {
      await mutateOwnedTabs(() => {
        ownedTabs.set(tab.id, ownerToken);
        return { changed: true };
      });
    } catch (_error) {
      try {
        await chrome.tabs.remove(tab.id);
      } catch (_ignored) {}
      return failure(
        requestId,
        "owned_tab_state_unavailable",
        "opened tab ownership could not be persisted",
      );
    }
    return {
      request_id: requestId,
      ok: true,
      window_id: String(tab.windowId),
      tab_id: String(tab.id),
      url,
      evidence: "owned_tab_opened",
    };
  }
  if (action.action === "activate") {
    const tab = await normalTabById(action.tab_id);
    await chrome.tabs.update(tab.id, { active: true });
    return {
      request_id: requestId,
      ok: true,
      window_id: String(tab.windowId),
      tab_id: String(tab.id),
      url: tab.url,
      evidence: "tab_activated",
    };
  }
  if (action.action === "close_owned") {
    const numericId = Number(action.tab_id);
    const ownerToken = String(action.owner_token || "");
    if (!Number.isInteger(numericId)) return failure(requestId, "invalid_tab_id", "tab id is invalid");
    let closeResult;
    try {
      closeResult = await mutateOwnedTabs(async () => {
        if (!ownerToken || ownedTabs.get(numericId) !== ownerToken) {
          return { changed: false, value: "not_owned" };
        }
        await chrome.tabs.remove(numericId);
        ownedTabs.delete(numericId);
        return { changed: true, value: "closed" };
      });
    } catch (_error) {
      return failure(
        requestId,
        "owned_tab_state_unavailable",
        "owned tab state could not be read or updated",
      );
    }
    if (closeResult !== "closed") {
      return failure(requestId, "tab_not_owned", "only tabs opened by this Computer Use task may be closed");
    }
    return {
      request_id: requestId,
      ok: true,
      tab_id: String(numericId),
      evidence: "owned_tab_closed",
    };
  }
  return failure(requestId, "unsupported_tab_action", "tab action is not allowlisted");
}

async function relay(request) {
  const requestId = String(request.request_id || "missing");
  if (request.type === "ping") {
    return { request_id: requestId, ok: true, browser_id: "chromium" };
  }
  if (request.type === "tab") {
    return relayTabAction(request);
  }
  const tab = await requestTab(request);
  if (request.type === "act" && request.action?.action === "navigate") {
    const url = String(request.action.url || "");
    if (!normalUrl(url)) return failure(requestId, "restricted_url", "only http and https URLs are allowed");
    await chrome.tabs.update(tab.id, { url });
    return { request_id: requestId, ok: true, tab_id: String(tab.id), url, evidence: "navigation_requested" };
  }
  if (request.type === "act" && request.action?.action === "history_back") {
    await chrome.tabs.goBack(tab.id);
    return { request_id: requestId, ok: true, tab_id: String(tab.id), evidence: "history_back_requested" };
  }
  if (request.type === "act" && request.action?.action === "history_forward") {
    await chrome.tabs.goForward(tab.id);
    return { request_id: requestId, ok: true, tab_id: String(tab.id), evidence: "history_forward_requested" };
  }
  const response = await sendContentRequest(tab.id, request);
  return {
    ...response,
    request_id: requestId,
    browser_id: "chromium",
    window_id: String(tab.windowId),
    tab_id: String(tab.id),
    frame_id: "0",
    url: response?.url || tab.url,
    title: response?.title || tab.title,
  };
}

async function deliverResponse(request, response) {
  const requestId = String(request?.request_id || response?.request_id || "missing");
  const replyToken = typeof request?.reply_token === "string" ? request.reply_token : "";
  try {
    nativePort?.postMessage(response);
  } catch (_error) {
    // The local HTTP reply path below is the authoritative fallback when the
    // browser->native stdin pipe is unavailable or silently drops responses.
  }
  if (!replyToken) return;
  try {
    await fetch("http://127.0.0.1:8765/api/computer-use/browser/response", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-coolzhu-browser-reply-token": replyToken,
      },
      body: JSON.stringify({ ...response, request_id: requestId }),
      cache: "no-store",
    });
  } catch (_error) {
    // Broker-side timeout remains the single terminal result if both reply
    // channels fail.
  }
}

async function onNativeMessage(request) {
  const requestId = String(request?.request_id || "missing");
  try {
    const response = await relay(request || {});
    await deliverResponse(request || {}, response);
  } catch (error) {
    const code = String(error?.message || "browser_bridge_failed");
    await deliverResponse(request || {}, failure(requestId, code, `browser bridge failed: ${code}`));
  }
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  const delay = Math.min(30000, 1000 * 2 ** Math.min(reconnectAttempt, 5));
  reconnectAttempt += 1;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectNative();
  }, delay);
}

function connectNative() {
  if (nativePort) return;
  try {
    const port = chrome.runtime.connectNative(HOST_NAME);
    nativePort = port;
    port.onMessage.addListener((message) => void onNativeMessage(message));
    port.onDisconnect.addListener(() => {
      nativePort = null;
      scheduleReconnect();
    });
    reconnectAttempt = 0;
  } catch (_error) {
    nativePort = null;
    scheduleReconnect();
  }
}

chrome.runtime.onInstalled.addListener(connectNative);
chrome.runtime.onStartup.addListener(connectNative);
chrome.tabs.onRemoved.addListener((tabId) => {
  void mutateOwnedTabs(() => ({
    changed: ownedTabs.delete(Number(tabId)),
  })).catch(() => undefined);
});
chrome.tabs.onReplaced.addListener((addedTabId, removedTabId) => {
  void mutateOwnedTabs(() => {
    const removedId = Number(removedTabId);
    const addedId = Number(addedTabId);
    const ownerToken = ownedTabs.get(removedId);
    let changed = ownedTabs.delete(removedId);
    if (validOwnerToken(ownerToken) && Number.isInteger(addedId)) {
      if (ownedTabs.get(addedId) !== ownerToken) {
        ownedTabs.set(addedId, ownerToken);
        changed = true;
      }
    } else if (ownedTabs.delete(addedId)) {
      changed = true;
    }
    return { changed };
  }).catch(() => undefined);
});
void ensureOwnedTabsRestored().catch(() => undefined);
connectNative();
