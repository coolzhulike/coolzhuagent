/* 快捷轨与内容工作区：只管理页面生命周期，业务仍由现有服务负责。 */
window.CoolzhuWorkspacePanels = (() => {
  let api;
  let scopeId = "";
  let traceController = null;
  let traceBefore = null;
  let archiveBefore = null;
  let tabs = [];
  let selectedTab = "";
  let scopedWidth = null;
  let activationVersion = 0;
  const storageKey = "coolzhu.preview-tabs.v1";
  const qs = role => document.querySelector(`[data-role="${role}"]`);
  const traceKinds = new Set(["reasoning", "tool-call", "tool-result", "tool-summary", "computer-use", "vision-computer-use"]);
  const element = (tag, text) => { const node = document.createElement(tag); if (text != null) node.textContent = text; return node; };
  const scopeKey = () => { const scope = api.scope(); return `${scope.workspace}\u0000${scope.room}\u0000${scope.session || ""}`; };
  const tabKey = ref => (ref.file ? "file:" + ref.locator.replaceAll("\\", "/").toLowerCase() : ref.locator);
  function saveTabs() {
    if (!scopeId) return;
    try {
      const saved = JSON.parse(localStorage.getItem(storageKey) || "{}");
      const entries = Object.entries(saved).filter(([key,value]) => key !== scopeId && value && typeof value === "object").sort((a,b) => (b[1].updated || 0) - (a[1].updated || 0)).slice(0,19);
      const valid = tabs.filter(tab => !tab.ref.locator.startsWith("blob:") && (!tab.ref.file || tab.authorized));
      entries.push([scopeId, {updated:Date.now(),selected:selectedTab,width:scopedWidth,tabs:valid.map(tab => ({locator:tab.ref.locator,label:tab.ref.label,line:tab.ref.line,sourceMessageId:tab.ref.sourceMessageId,revision:tab.ref.revision}))}]);
      localStorage.setItem(storageKey, JSON.stringify(Object.fromEntries(entries)));
    } catch (_) { /* 浏览器存储不可用时继续提供本次预览。 */ }
  }
  function restoreTabs() {
    tabs = []; selectedTab = ""; scopedWidth = api.defaultWidth;
    try {
      const saved = JSON.parse(localStorage.getItem(storageKey) || "{}")[scopeId];
      if (Number.isFinite(saved?.width)) scopedWidth = saved.width;
      for (const entry of (Array.isArray(saved?.tabs) ? saved.tabs : []).slice(0,12)) {
        if (typeof entry.locator !== "string" || entry.locator.length > 4096 || entry.locator.startsWith("blob:")) continue;
        const ref = window.CoolzhuContentPreview?.reference(entry.locator, String(entry.label || "").slice(0,120));
        if (!ref) continue;
        if (Number.isSafeInteger(entry.line) && entry.line > 0) ref.line = entry.line;
        if (typeof entry.sourceMessageId === "string" && entry.sourceMessageId.length <= 256) ref.sourceMessageId = entry.sourceMessageId;
        if (typeof entry.revision === "string" && entry.revision.length <= 256) ref.revision = entry.revision;
        const id = tabKey(ref); if (!tabs.some(tab => tab.id === id)) tabs.push({id,ref,authorized:true,restored:true});
      }
      selectedTab = tabs.some(tab => tab.id === saved?.selected) ? saved.selected : tabs[0]?.id || "";
    } catch (_) { /* 损坏的本地状态不阻止打开聊天。 */ }
    api.setWidth?.(scopedWidth); renderTabs();
  }
  function widthChanged(width) { if (Number.isFinite(width)) { scopedWidth = width; saveTabs(); } }
  function currentWidth() { return scopeId === scopeKey() && Number.isFinite(scopedWidth) ? scopedWidth : null; }
  function renderTabs() {
    const bar = qs("preview-tabs"); if (!bar) return;
    bar.replaceChildren();
    bar.hidden = !tabs.length || !["preview", "browser"].includes(api.active());
    const reopen = qs("preview-tabs-reopen"); if (reopen) { reopen.hidden = !tabs.length; reopen.textContent = `预览标签 (${tabs.length})`; }
    const selected = tabs.find(tab => tab.id === selectedTab);
    if (selected?.ref.sourceMessageId) {
      const source = element("button", "来源"); source.type = "button"; source.className = "workspace-preview-source";
      source.title = "回到引用此内容的聊天消息";
      source.addEventListener("click", () => { home(); void window.CoolzhuChatExperience?.locate(selected.ref.sourceMessageId); }); bar.append(source);
    }
    for (const tab of tabs) {
      const item = element("div"); item.className = "workspace-preview-tab";
      const select = element("button", tab.ref.label || tab.ref.locator); select.type = "button"; select.className = "workspace-preview-tab-select";
      select.title = tab.ref.locator; select.setAttribute("role", "tab"); select.setAttribute("aria-selected", String(tab.id === selectedTab));
      select.addEventListener("click", () => void activateTab(tab.id));
      const close = element("button", "×"); close.type = "button"; close.className = "workspace-preview-tab-close"; close.setAttribute("aria-label", `关闭 ${tab.ref.label}`);
      close.addEventListener("click", () => closeTab(tab.id)); item.append(select, close); bar.append(item);
    }
  }
  async function activateTab(id) {
    const tab = tabs.find(item => item.id === id); if (!tab) return;
    selectedTab = id; tab.restored = false; saveTabs();
    const version = ++activationVersion; const key = scopeKey();
    const current = () => key === scopeKey() && version === activationVersion && selectedTab === id;
    if (tab.ref.kind === "web" && !tab.ref.file) {
      window.CoolzhuContentPreview?.dispose();
      await api.openBrowser(tab.ref.locator); renderTabs(); return;
    }
    api.open("preview"); renderTabs();
    await window.CoolzhuContentPreview.open(qs("content-preview"), tab.ref, {
      isCurrent: () => current() && api.active() === "preview",
      onResolved: metadata => { if (current()) { tab.authorized = true; if (metadata?.revision) tab.ref.revision = metadata.revision; saveTabs(); } },
      onLocation: line => { if (current()) { tab.ref.line = line; saveTabs(); } },
      openBrowser: url => api.openBrowser(url),
    });
  }
  function closeTab(id) {
    const index = tabs.findIndex(tab => tab.id === id); if (index < 0) return;
    const wasSelected = selectedTab === id; tabs.splice(index,1);
    if (wasSelected) { activationVersion++; window.CoolzhuContentPreview?.dispose(); void window.CoolzhuNativeBrowserPanel?.close().catch(() => {}); selectedTab = tabs[Math.min(index,tabs.length-1)]?.id || ""; }
    saveTabs(); renderTabs();
    // 关闭标签不代表授权加载相邻网页；保留标签待用户点击。
    if (wasSelected) showRestoredTabs();
  }
  function showRestoredTabs() {
    if (!tabs.length) { home(); return; }
    api.open("preview"); renderTabs();
    const host = qs("content-preview"); if (!host) return;
    host.replaceChildren(element("p", "已保留当前会话的预览标签。点击上方标签重新打开内容；网页和媒体不会自动载入或播放。"));
  }
  function panelOpened() { renderTabs(); }
  function browserLocation(url) {
    if (api.active() !== "browser" || !/^https?:/i.test(url)) return;
    const tab = tabs.find(item => item.id === selectedTab); if (!tab || tab.ref.kind !== "web") return;
    const ref = window.CoolzhuContentPreview?.reference(url, tab.ref.label); if (!ref || ref.file) return;
    tab.ref = {...tab.ref,locator:ref.locator,kind:"web"};
    const next = tabKey(tab.ref); tabs = tabs.filter(item => item === tab || item.id !== next); tab.id = next; selectedTab = next;
    saveTabs(); renderTabs();
  }
  function home() {
    activationVersion++; api.close(); api.collapse(); renderTabs();
    qs("message-input")?.focus({preventScroll:true});
  }
  function navigate(id) {
    if (id === "chat") { home(); return; }
    api.open(id);
    if (id === "schedules") void api.refreshSchedules();
    if (id === "clawbot") void api.refreshWechat();
    if (id === "trace") void loadTrace();
  }
  function beforeClose(id) {
    if (id === "browser") void window.CoolzhuNativeBrowserPanel?.close().catch(() => {});
    if (id === "preview") window.CoolzhuContentPreview?.dispose();
    if (id === "trace") { traceController?.abort(); traceController = null; }
  }
  function scopeChanged() {
    if (!api) return;
    const next = scopeKey();
    if (scopeId === next) return;
    saveTabs(); scopeId = next; activationVersion++;
    window.CoolzhuVideoWait?.scopeChanged();
    void window.CoolzhuNativeBrowserPanel?.close().catch(() => {});
    window.CoolzhuContentPreview?.dispose(); traceController?.abort(); traceBefore = null;
    qs("chat-trace-items")?.replaceChildren();
    if (["preview", "trace", "browser"].includes(api.active())) home();
    restoreTabs();
  }
  async function showReference(ref) {
    if (!ref) return;
    scopeChanged();
    const id = tabKey(ref); let tab = tabs.find(item => item.id === id);
    if (!tab) { if (tabs.length >= 12) tabs.shift(); tab = {id,ref,authorized:!ref.file,restored:false}; tabs.push(tab); }
    else tab.ref = {...tab.ref,...ref};
    await activateTab(id);
  }

  function contentClick(event) {
    if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const target = event.target.closest("[data-content-ref]") || event.target.closest("a,img.attachment-preview,img.rich-media-control,video.attachment-preview,video.rich-media-control");
    if (!target || !target.closest('[data-role="chat-message-list"]')) return;
    let locator = target.dataset.contentRef || target.getAttribute("href") || target.getAttribute("src");
    const mediaMime = target.tagName === "IMG" ? "image/*" : target.tagName === "VIDEO" ? "video/*" : "";
    const ref = window.CoolzhuContentPreview.reference(locator, target.dataset.contentLabel || target.getAttribute("alt") || target.textContent, target.dataset.contentMime || mediaMime);
    if (!ref) return;
    ref.sourceMessageId = target.closest(".message[data-message-id]")?.dataset.messageId;
    if (target.tagName === "VIDEO") target.pause();
    event.preventDefault(); event.stopPropagation(); void showReference(ref);
  }
  const traceStatus = value => ({completed:"已完成",succeeded:"已完成",running:"运行中",accepted:"已接纳",failed:"失败",interrupted:"已中止",cancelled:"已取消",requested:"待执行","not-executed":"未执行",unknown:"状态未知",remote_unknown:"远端状态未知",http_error:"请求失败"})[value] || value || "未知";
  async function loadArchive(append, signal, key) {
    const host = qs("chat-trace-archive"); if (!host) return;
    const scope = api.scope(); const query = new URLSearchParams({limit:"100"}); if (append && archiveBefore) query.set("before",archiveBefore);
    const response = await fetch(`/api/chat/rooms/${encodeURIComponent(scope.room)}/messages?${query}`, {signal});
    const data = await response.json(); if (!response.ok) throw new Error(data.error || "原始记录读取失败");
    if (scopeKey() !== key || signal.aborted) return;
    const messages = (data.messages || []).filter(message => traceKinds.has(String(message.kind || "").replaceAll("_","-")) || message.role === "tool");
    for (const message of messages.reverse()) {
      const row = element("details"); row.className = "chat-trace-entry";
      const kind = String(message.kind || "").replaceAll("_","-");
      const phase = kind === "reasoning" ? "思考过程" : kind === "tool-call" ? "工具请求" : kind === "tool-result" ? "工具结果" : "工具过程";
      row.append(element("summary",`${message.tool_name || message.author || "Agent"} · ${phase} · ${new Date(message.created_at).toLocaleString("zh-CN")}`),element("pre",message.content || "无内容")); host.append(row);
    }
    archiveBefore = data.next_before;
    const more = qs("chat-trace-archive-more"); if (more) more.hidden = !data.has_more;
  }
  async function loadTrace(append = false) {
    traceController?.abort(); traceController = new AbortController();
    const signal = traceController.signal; const scope = api.scope(); const key = scopeKey();
    const items = qs("chat-trace-items"), status = qs("chat-trace-status"); if (!items || !scope.room) return;
    if (!append) {
      traceBefore = null; archiveBefore = null; items.replaceChildren();
      const runs = element("div"); runs.dataset.role = "chat-trace-runs";
      const archive = element("details"); archive.className = "chat-trace-archive";
      archive.append(element("summary","历史思考与原始过程记录（独立存档）"));
      const archiveItems = element("div"); archiveItems.dataset.role = "chat-trace-archive";
      const more = element("button","查看更早原始记录"); more.type = "button"; more.dataset.role = "chat-trace-archive-more"; more.hidden = true;
      more.addEventListener("click", () => void loadArchive(true,traceController.signal,key).catch(error => { if (error.name !== "AbortError" && scopeKey() === key) status.textContent = error.message; }));
      archive.append(archiveItems,more); items.append(runs,archive);
      void loadArchive(false,signal,key).catch(error => { if (error.name !== "AbortError" && scopeKey() === key) archiveItems.textContent = error.message; });
    }
    status.textContent = "正在读取真实运行轨迹…";
    const query = new URLSearchParams({limit:"30"}); if (traceBefore) query.set("before",traceBefore);
    try {
      const response = await fetch(`/api/chat/rooms/${encodeURIComponent(scope.room)}/trace?${query}`, {signal});
      const data = await response.json(); if (!response.ok) throw new Error(data.error || "运行轨迹读取失败");
      if (scopeKey() !== key || signal.aborted) return;
      const runHost = qs("chat-trace-runs"); if (!runHost) return;
      for (const run of data.runs || []) {
        const row = element("details"); row.className = "chat-trace-entry"; row.dataset.runId = run.run_id;
        const calls = run.calls || []; const names = [...new Set(calls.map(call => call.tool_name))].join("、");
        const elapsed = run.finished_at != null && run.started_at != null ? ` · ${((run.finished_at-run.started_at)/1000).toFixed(1)} 秒` : "";
        row.append(element("summary",`${traceStatus(run.status)}${elapsed} · ${names || "模型会话"} · ${new Date(run.created_at).toLocaleString("zh-CN")}`));
        row.append(element("small",`轮次 ${run.turn_id || "未记录"} · ${run.run_id}`));
        if (run.source_message_id) {
          const locate=element("button","定位本轮提问"); locate.type="button";
          locate.addEventListener("click",()=>{ home(); void window.CoolzhuChatExperience?.locate(run.source_message_id); }); row.append(locate);
        }
        if (run.stop_reason) row.append(element("p",run.stop_reason));
        for (const call of calls) {
          const fact = element("p",`${call.tool_name} · ${traceStatus(call.status)} · ${Math.max(0,(call.updated_at-call.started_at)/1000).toFixed(1)} 秒`);
          fact.title = call.call_id; row.append(fact,element("small",`调用 ${call.call_id}`));
        }
        const tokens = value => value == null ? "未知" : Number(value).toLocaleString();
        for (const request of run.requests || []) row.append(element("p",`模型请求 · ${traceStatus(request.status)} · 输入 ${tokens(request.input_tokens)} / 输出 ${tokens(request.output_tokens)} token`));
        runHost.append(row);
      }
      traceBefore = data.next_before; qs("chat-trace-more").hidden = !data.has_more;
      qs("chat-trace-more").textContent = "查看更早轮次";
      status.textContent = runHost.childElementCount ? `已显示 ${runHost.childElementCount} 个运行轮次 · 工具调用按真实调用 ID 去重` : "暂无结构化运行轮次；历史思考与原始记录仍可在下方查看。";
    } catch (error) { if (error.name !== "AbortError" && scopeKey() === key) status.textContent = `运行轨迹加载失败：${error.message}`; }
  }

  function init(adapter) {
    if (api) return; api = adapter; scopeId = scopeKey();
    restoreTabs();
    qs("preview-tabs-reopen")?.addEventListener("click", showRestoredTabs);
    const schedules = document.querySelector('[data-role="task-schedule-section"]');
    const scheduleHost = qs("schedules-panel"); if (schedules && scheduleHost) scheduleHost.append(schedules);
    document.addEventListener("click", contentClick, true);
    document.querySelector('[data-action="chat-trace-open"]')?.addEventListener("click", () => navigate("trace"));
    qs("chat-trace-refresh")?.addEventListener("click", () => void loadTrace());
    qs("chat-trace-more")?.addEventListener("click", () => void loadTrace(true));
  }
  return {init,navigate,home,beforeClose,scopeChanged,showReference,loadTrace,panelOpened,widthChanged,currentWidth,browserLocation};
})();
