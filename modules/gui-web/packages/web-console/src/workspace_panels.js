/* 快捷轨与内容工作区：只管理页面生命周期，业务仍由现有服务负责。 */
window.CoolzhuWorkspacePanels = (() => {
  let api;
  let scopeId = "";
  let traceController = null;
  let traceBefore = null;
  let archiveBefore = null;
  let traceMessageId = null;
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
    const reopen = qs("preview-tabs-reopen"); if (reopen) { reopen.hidden = !tabs.length; setWuxiaIconOnly(reopen, "file", `打开已保留的预览标签 (${tabs.length})`); }
    const selected = tabs.find(tab => tab.id === selectedTab);
    if (selected?.ref.sourceMessageId) {
      const source = element("button", "来源"); source.type = "button"; source.className = "workspace-preview-source";
      setWuxiaIconOnly(source, "chat", "回到引用此内容的聊天消息");
      source.addEventListener("click", () => locateAfterHome(selected.ref.sourceMessageId)); bar.append(source);
    }
    for (const tab of tabs) {
      const item = element("div"); item.className = "workspace-preview-tab";
      const select = element("button", tab.ref.label || tab.ref.locator); select.type = "button"; select.className = "workspace-preview-tab-select";
      select.title = tab.ref.locator; select.setAttribute("role", "tab"); select.setAttribute("aria-selected", String(tab.id === selectedTab));
      select.addEventListener("click", () => void activateTab(tab.id));
      const close = element("button"); close.type = "button"; close.className = "workspace-preview-tab-close";
      setWuxiaIconOnly(close, "stop", `关闭 ${tab.ref.label}`);
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
  function panelOpened() {
    renderTabs();
    if (api.active() === "browser") window.CoolzhuNativeBrowserPanel?.resume();
  }
  function browserLocation(url) {
    if (api.active() !== "browser" || !/^https?:/i.test(url)) return;
    const ref = window.CoolzhuContentPreview?.reference(url, url); if (!ref || ref.file) return;
    let tab = tabs.find(item => item.id === selectedTab);
    // 地址栏导航也要拥有网页标签；保留先前文件标签及其消息来源。
    if (!tab || tab.ref.kind !== "web") {
      const id = tabKey(ref);
      tab = tabs.find(item => item.id === id);
      if (!tab) { tab = {id,ref,authorized:true}; tabs.push(tab); }
    }
    // 网页标签跟随已接纳的实际地址；最初引用的聊天消息仍由来源索引保留。
    tab.ref = {...tab.ref,locator:ref.locator,label:ref.locator,kind:"web"};
    const next = tabKey(tab.ref); tabs = tabs.filter(item => item === tab || item.id !== next); tab.id = next; selectedTab = next;
    saveTabs(); renderTabs();
  }
  function home(options = {}) {
    activationVersion++; api.close({disposeBrowser:options.disposeBrowser === true}); api.collapse(options.preserveScroll !== false); renderTabs();
    qs("message-input")?.focus({preventScroll:true});
  }
  function locateAfterHome(id) {
    const scope = scopeKey();
    home({preserveScroll:false});
    const panel = document.querySelector(".chat-workbench-window .chat-window-panel, .chat-window-panel");
    let finished = false;
    let fallback;
    const settle = () => {
      if (finished) return;
      finished = true;
      panel?.removeEventListener("transitionend", onTransitionEnd);
      clearTimeout(fallback);
      if (scopeKey() === scope) void window.CoolzhuChatExperience?.locate(id);
    };
    const onTransitionEnd = event => {
      if (event.target === panel && event.propertyName === "grid-template-columns") settle();
    };
    panel?.addEventListener("transitionend", onTransitionEnd);
    fallback = setTimeout(settle, 250);
  }
  function initMoreDock() {
    const more = qs("window-dock-more");
    const trigger = more?.querySelector("summary");
    const items = Array.from(more?.querySelectorAll(".window-dock-secondary [data-window-target]") || []);
    if (!more || !trigger || !items.length) return;
    const close = (restoreFocus = false) => {
      if (!more.open) return;
      more.open = false;
      trigger.setAttribute("aria-expanded", "false");
      if (restoreFocus) trigger.focus({preventScroll:true});
    };
    more.addEventListener("toggle", () => trigger.setAttribute("aria-expanded", String(more.open)));
    more.addEventListener("keydown", event => {
      if (event.key === "Escape" && more.open) {
        event.preventDefault(); event.stopPropagation(); close(true); return;
      }
      if (event.target === trigger && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        event.preventDefault();
        more.open = true;
        (event.key === "ArrowDown" ? items[0] : items[items.length - 1]).focus({preventScroll:true});
        return;
      }
      const index = items.indexOf(event.target);
      if (index < 0) return;
      let next = index;
      if (event.key === "ArrowDown") next = (index + 1) % items.length;
      else if (event.key === "ArrowUp") next = (index + items.length - 1) % items.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = items.length - 1;
      else return;
      event.preventDefault();
      items[next].focus({preventScroll:true});
    });
    document.addEventListener("pointerdown", event => {
      if (more.open && !more.contains(event.target)) close();
    }, true);
    document.addEventListener("focusin", event => {
      if (more.open && !more.contains(event.target)) close();
    });
  }
  function navigate(id) {
    const more = qs("window-dock-more");
    if (more) more.open = false;
    if (id === "chat") { home(); return; }
    if (id === "trace") { openTaskTab("trace"); return; }
    api.open(id);
    if (id === "schedules") openTaskTab(taskTab);
    if (id === "clawbot") void api.refreshWechat();
    if (id === "trace") { traceMessageId = null; void loadTrace(); }
  }
  async function openTraceArchive(messageId = null) {
    if (!api) return;
    traceMessageId = typeof messageId === "string" && messageId.trim() ? messageId : null;
    openTaskTab("trace", false);
    await loadTrace();
    if (traceMessageId) return;
    const archive = qs("chat-trace-items")?.querySelector(".chat-trace-archive");
    if (archive) {
      archive.open = true;
      archive.scrollIntoView({block:"start"});
    }
  }
  function beforeClose(id, options = {}) {
    if (id === "browser") {
      const native = window.CoolzhuNativeBrowserPanel;
      void (options.disposeBrowser ? native?.close() : native?.suspend())?.catch(() => {});
      const frame = document.querySelector('[data-role="browser-window-frame"]');
      if (frame) { frame.src = "about:blank"; delete frame.dataset.currentUrl; }
    }
    if (id === "preview") window.CoolzhuContentPreview?.dispose();
    if (id === "trace" || id === "schedules") { traceController?.abort(); traceController = null; taskGeneration++; taskExecution?.dispose(); taskExecution=null; }
  }
  function scopeChanged() {
    if (!api) return;
    const next = scopeKey();
    if (scopeId === next) return;
    // 切换发送模型仍属于同一聊天室；只在工程或聊天室改变时清理浏览器地址。
    const browserContextChanged = scopeId.split("\u0000").slice(0, 2).join("\u0000") !== next.split("\u0000").slice(0, 2).join("\u0000");
    saveTabs(); scopeId = next; activationVersion++;
    window.CoolzhuVideoWait?.scopeChanged();
    void window.CoolzhuNativeBrowserPanel?.close({forgetTarget:browserContextChanged}).catch(() => {});
    if (browserContextChanged) api.resetBrowser?.();
    const browserFrame = document.querySelector('[data-role="browser-window-frame"]');
    if (browserFrame) { browserFrame.src = "about:blank"; delete browserFrame.dataset.currentUrl; }
    window.CoolzhuContentPreview?.dispose(); traceController?.abort(); traceBefore = null; traceMessageId = null;
    qs("chat-trace-items")?.replaceChildren();
    if (["preview", "trace", "browser"].includes(api.active())) home();
    restoreTabs();
    taskGeneration++;
    if (api.active() === "schedules") { openTaskTab(taskTab, taskTab !== "trace"); if (taskTab === "trace" && qs("chat-trace-status")) qs("chat-trace-status").textContent="聊天室已切换，请重新选择轨迹标签读取。"; }
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
  const traceStatus = value => ({completed:"已完成",succeeded:"已完成",running:"运行中",accepted:"已接纳",failed:"失败",interrupted:"已中止",cancelled:"已取消",requested:"待执行",prepared:"待派发",dispatched:"已派发",not_sent:"未发送",refusal:"模型拒绝",max_tokens:"达到输出上限",max_turn_requests:"达到请求上限","not-executed":"未执行",unknown:"状态未知",remote_unknown:"远端状态未知",http_error:"请求失败"})[value] || value || "未知";
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
    const requestedMessage = traceMessageId;
    const items = qs("chat-trace-items"), status = qs("chat-trace-status"); if (!items || !scope.room) return;
    if (!append) {
      traceBefore = null; archiveBefore = null; items.replaceChildren();
      const runs = element("div"); runs.dataset.role = "chat-trace-runs";
      const archive = element("details"); archive.className = "chat-trace-archive";
      archive.append(element("summary","历史思考与原始过程记录（独立存档）"));
      const archiveItems = element("div"); archiveItems.dataset.role = "chat-trace-archive";
      const more = element("button","查看更早原始记录"); more.type = "button"; more.dataset.role = "chat-trace-archive-more"; more.hidden = true;
      setWuxiaIconOnly(more, "chevron", "查看更早原始记录");
      more.addEventListener("click", () => void loadArchive(true,traceController.signal,key).catch(error => { if (error.name !== "AbortError" && scopeKey() === key) status.textContent = error.message; }));
      archive.append(archiveItems,more); items.append(runs,archive);
      if (requestedMessage) archive.hidden = true;
      else void loadArchive(false,signal,key).catch(error => { if (error.name !== "AbortError" && scopeKey() === key) archiveItems.textContent = error.message; });
    }
    status.textContent = "正在读取真实运行轨迹…";
    const query = new URLSearchParams({limit:"30"}); if (traceBefore) query.set("before",traceBefore);
    if (requestedMessage) { query.delete("before"); query.set("message_id",requestedMessage); }
    try {
      const response = await fetch(`/api/chat/rooms/${encodeURIComponent(scope.room)}/trace?${query}`, {signal});
      const data = await response.json(); if (!response.ok) throw new Error(data.error || "运行轨迹读取失败");
      if (scopeKey() !== key || signal.aborted || requestedMessage !== traceMessageId) return;
      if (requestedMessage && (data.requested_message_id !== requestedMessage || data.runs?.length !== 1)) {
        throw new Error("未收到这条回复的唯一运行记录，请刷新后重试");
      }
      const runHost = qs("chat-trace-runs"); if (!runHost) return;
      for (const run of data.runs || []) {
        const row = element("details"); row.className = "chat-trace-entry"; row.dataset.runId = run.run_id;
        const calls = run.calls || []; const names = [...new Set(calls.map(call => call.tool_name))].join("、");
        const elapsed = run.finished_at != null && run.started_at != null ? ` · ${((run.finished_at-run.started_at)/1000).toFixed(1)} 秒` : "";
        row.append(element("summary",`${traceStatus(run.status)}${elapsed} · ${names || "模型会话"} · ${new Date(run.created_at).toLocaleString("zh-CN")}`));
        row.append(element("small",`轮次 ${run.turn_id || "未记录"} · ${run.run_id}`));
        if (run.source_message_id) {
          const locate=element("button","定位本轮提问"); locate.type="button";
          setWuxiaIconOnly(locate, "crosshair", "定位本轮提问");
          locate.addEventListener("click",()=>locateAfterHome(run.source_message_id)); row.append(locate);
        }
        if (run.stop_reason) row.append(element("p",run.stop_reason));
        for (const call of calls) {
          const fact = element("p",`${call.tool_name} · ${traceStatus(call.status)} · ${Math.max(0,(call.updated_at-call.started_at)/1000).toFixed(1)} 秒`);
          fact.title = call.call_id; row.append(fact,element("small",`调用 ${call.call_id}`));
        }
        const tokens = value => value == null ? "未知" : Number(value).toLocaleString();
        for (const request of run.requests || []) row.append(element("p",`模型请求 · ${traceStatus(request.status)} · 输入 ${tokens(request.input_tokens)} / 输出 ${tokens(request.output_tokens)} token`));
        if (requestedMessage) row.open = true;
        runHost.append(row);
        if (requestedMessage) row.scrollIntoView({block:"start"});
      }
      traceBefore = data.next_before; qs("chat-trace-more").hidden = !data.has_more;
      setWuxiaIconOnly(qs("chat-trace-more"), "chevron", "查看更早轮次");
      status.textContent = requestedMessage ? "已定位这条回复的运行记录" : runHost.childElementCount ? `已显示 ${runHost.childElementCount} 个运行轮次 · 工具调用按真实调用 ID 去重` : "暂无结构化运行轮次；历史思考与原始记录仍可在下方查看。";
    } catch (error) { if (error.name !== "AbortError" && scopeKey() === key) status.textContent = `运行轨迹加载失败：${error.message}`; }
  }


  let taskTab = "scheduled", taskGeneration = 0, taskExecution = null;
  async function taskRead(url) {
    const version = taskGeneration, key = scopeKey();
    const response = await fetch(url); const data = await response.json();
    if (version !== taskGeneration || key !== scopeKey()) throw new DOMException("任务页作用域已变更", "AbortError");
    if (!response.ok) throw new Error(data.error?.message || data.error || `读取失败 (${response.status})`);
    return data;
  }
  function initTaskHub(schedules) {
    const host = qs("schedules-panel"); if (!host) return;
    const title=element("h2","任务"); const nav=element("nav"); nav.className="task-hub-tabs"; nav.setAttribute("role","tablist");
    host.append(title,nav);
    for (const [id,label] of [["scheduled","定时任务"],["background","后台任务"],["trace","轨迹"],["cu","CU 记录"]]) {
      const button=element("button",label); button.type="button"; button.dataset.taskTab=id; button.setAttribute("role","tab"); button.setAttribute("aria-controls",`task-pane-${id}`);
      button.addEventListener("click",()=>openTaskTab(id)); nav.append(button);
      const pane=element("section"); pane.dataset.taskPane=id; pane.id=`task-pane-${id}`; pane.setAttribute("role","tabpanel"); pane.hidden=id!==taskTab; host.append(pane);
    }
    const pane=id=>host.querySelector(`[data-task-pane="${id}"]`);
    if (schedules) pane("scheduled").append(schedules);
    const history=element("details"); history.append(element("summary","历史任务消息")); const messages=element("div"); history.append(messages); pane("scheduled").append(history);
    let historyBefore=null;
    const moreHistory=element("button","更早的任务消息");moreHistory.type="button";moreHistory.hidden=true;history.append(moreHistory);
    const readHistory=async(append=false)=>{try {
      const query=new URLSearchParams({limit:"100"});if(append&&historyBefore)query.set("before",historyBefore);
      if(!append)messages.textContent="正在读取…";
      const data=await taskRead(`/api/chat/rooms/room-scheduled-tasks/messages?${query}`);
      if(!append)messages.replaceChildren();
      for(const message of (data.messages||[]).slice().reverse()) messages.append(element("p",`${new Date(message.created_at).toLocaleString()} · ${message.content}`));
      historyBefore=data.next_before;moreHistory.hidden=!data.has_more;
      if(!messages.childElementCount)messages.textContent="没有历史任务消息。";
    }catch(error){if(error.name!=="AbortError")messages.textContent=error.message;}};
    history.addEventListener("toggle",()=>{if(history.open)void readHistory();});
    moreHistory.addEventListener("click",()=>void readHistory(true));
    const trace=document.querySelector('[data-window-id="trace"] .workspace-content-panel'); if(trace) pane("trace").append(trace);
    const storage=element("details"); storage.append(element("summary","运行数据与十二小时保留策略")); const info=element("div"); storage.append(info); host.append(storage);
    storage.addEventListener("toggle",async()=> {if(!storage.open)return; info.textContent="正在估算…";try {
      const data=await taskRead("/api/tasks/storage"); info.replaceChildren(element("p",data.note));
      info.append(element("p",`运行表 ${(data.runtime_bytes/1048576).toFixed(1)} MB · 按当前增长估算十二小时 ${(data.estimated_12h_bytes/1048576).toFixed(1)} MB`));
      info.append(element("p",`最早运行记录：${data.earliest_record_at?new Date(data.earliest_record_at).toLocaleString():"暂无"} · 最近清理：${data.last_cleanup?new Date(data.last_cleanup.at).toLocaleString()+"，删除 "+data.last_cleanup.deleted_rows+" 条":"尚未执行"}`));
      if(data.growth_warning){const warning=element("p",data.growth_warning);warning.setAttribute("role","alert");info.append(warning);}
      for(const row of data.tables||[])info.append(element("p",`${row.table}：${row.rows} 条 · ${row.bytes==null?"大小未知":(row.bytes/1048576).toFixed(2)+" MB"}`));
    } catch(error){if(error.name!=="AbortError")info.textContent=error.message;}});
    pane("cu").append(element("p","打开此标签读取当前工程的 CU 运行。"));
  }
  function openTaskTab(id, refresh=true) {
    taskTab=id; taskGeneration++; api.open("schedules");
    document.querySelectorAll("[data-task-tab]").forEach(button=>{button.setAttribute("aria-selected",String(button.dataset.taskTab===id));});
    document.querySelectorAll("[data-task-pane]").forEach(pane=>{pane.hidden=pane.dataset.taskPane!==id;});
    if (!refresh)return;
    if(id==="scheduled")void api.refreshSchedules();
    if(id==="trace"){traceMessageId=null;void loadTrace();}
    if(id==="background") {
      const root=document.querySelector('[data-task-pane="background"]');
      taskExecution?.dispose(); taskExecution=window.CoolzhuExecutionPanel?.mount(root,{getScope:()=>projectRequestScope(),getAvatar:spec=>avatarUrl(avatarForSession(sessionById(spec.session_id)||{}))});
    } else {taskExecution?.dispose();taskExecution=null;}
    if(id==="cu")void loadCuTasks();
  }
  async function loadCuTasks() {
    const host=document.querySelector('[data-task-pane="cu"]'); if(!host)return;host.replaceChildren();
    const toolbar=element("div");toolbar.className="task-hub-filters";
    const select=(label,options)=>{const box=element("label",label+" ");const control=element("select");for(const [value,text] of options){const option=element("option",text);option.value=value;control.append(option);}box.append(control);toolbar.append(box);return control;};
    const room=select("范围",[["all","当前工程"],["room","当前聊天室"]]);
    const period=select("时间",[["12","最近十二小时"],["24","最近一天"],["all","全部保留记录"]]);
    const state=select("状态",[["","全部"],["running","运行中"],["succeeded","已完成"],["failed","失败"],["blocked","被阻止"],["cancelled","已取消"],["timed_out","超时"]]);
    const refresh=element("button","刷新");refresh.type="button";toolbar.append(refresh);
    const list=element("div"),status=element("p"),more=element("button","更早的 CU 记录");more.type="button";more.hidden=true;
    host.append(toolbar,status,list,more);let before=null,serial=0;
    const read=async(append=false)=>{const request=++serial;status.textContent="正在读取 CU 运行…";more.disabled=true;
      try {
        const query=new URLSearchParams({limit:"50"});if(room.value==="room")query.set("room_id",api.scope().room);
        if(period.value!=="all")query.set("since",String(Date.now()-Number(period.value)*3600000));if(state.value)query.set("state",state.value);
        if(append&&before)query.set("before",before);
        const data=await taskRead(`/api/computer-use/runs?${query}`);if(request!==serial)return;if(data.unavailable)throw new Error(data.unavailable);
        if(!append)list.replaceChildren();
        for(const run of data.runs||[]) {
          const row=element("details");row.append(element("summary",`${traceStatus(run.state)} · ${run.session_id} · ${new Date(run.created_at_ms).toLocaleString()}`));
          row.append(element("p",`尝试 ${run.attempts} · 已发送输入 ${run.input_sent} · 验证步骤 ${run.verified_steps} · 清理事故 ${run.cleanup_incidents}`));
          const report=element("div");row.append(report);let loaded=false;
          row.addEventListener("toggle",async()=>{if(!row.open||loaded)return;report.textContent="读取详情…";try {
            const data=await taskRead(`/api/computer-use/run-report?call_id=${encodeURIComponent(run.call_id)}`);if(data.unavailable)throw new Error(data.unavailable);report.replaceChildren();loaded=true;
            for(const step of data.steps||[]) {
              const block=element("section");block.className="task-cu-step";
              const input=({none:"未发送",partial:"部分发送",complete:"完整发送",unknown:"未知"})[step.input_status]||step.input_status;
              block.append(element("p",`步骤 ${step.step_index} · ${traceStatus(step.status)} · 输入${input}${step.error_code?" · "+step.error_code:""}`));
              if(step.forbids_automatic_replay)block.append(element("p","输入可能已生效，此步骤禁止自动重放。"));
              for(const [phase,label] of [["before","查看操作前截图"],["after","查看操作后截图"]]) {
                const button=element("button",label);button.type="button";block.append(button);
                button.addEventListener("click",()=>{button.disabled=true;const image=element("img");image.alt=label;image.className="task-cu-evidence";image.loading="lazy";
                  image.src=`/api/computer-use/evidence?${new URLSearchParams({call_id:run.call_id,step:String(step.step_index),phase})}`;
                  image.addEventListener("error",()=>{image.remove();block.append(element("p","该步骤没有截图或证据已过期。"));});block.append(image);});
              }
              report.append(block);
            }
            if(!data.steps?.length)report.append(element("p","没有已记录的操作步骤。"));
            for(const incident of data.cleanup_incidents||[])report.append(element("p",`清理事故 ${incident.incident_id} · ${incident.recovery_eligible?"可进入输入释放恢复":"需要检查释放状态"}`));
            const raw=element("details");raw.append(element("summary","原始诊断详情"),element("pre",JSON.stringify(data,null,2)));report.append(raw);
          }catch(error){if(error.name!=="AbortError")report.textContent=error.message;}});list.append(row);
        }
        before=data.runs?.at(-1)?.call_id;more.hidden=(data.runs?.length||0)<50;status.textContent=list.childElementCount?`已显示 ${list.childElementCount} 次运行` :"没有符合筛选条件的 CU 记录。";
      }catch(error){if(error.name!=="AbortError"&&request===serial)status.textContent=`CU 记录读取失败：${error.message}`;}finally{if(request===serial)more.disabled=false;}
    };
    refresh.addEventListener("click",()=>void read());more.addEventListener("click",()=>void read(true));
    for(const control of [room,period,state])control.addEventListener("change",()=>void read());
    await read();
  }

  function init(adapter) {
    if (api) return; api = adapter; scopeId = scopeKey();
    restoreTabs();
    initMoreDock();
    qs("preview-tabs-reopen")?.addEventListener("click", showRestoredTabs);
    const schedules = document.querySelector('[data-role="task-schedule-section"]');
    initTaskHub(schedules);
    document.addEventListener("click", contentClick, true);
    document.querySelector('[data-action="chat-trace-open"]')?.addEventListener("click", () => navigate("trace"));
    qs("chat-trace-refresh")?.addEventListener("click", () => void loadTrace());
    qs("chat-trace-more")?.addEventListener("click", () => void loadTrace(true));
  }
  return {init,navigate,home,beforeClose,scopeChanged,showReference,loadTrace,openTraceArchive,panelOpened,widthChanged,currentWidth,browserLocation};
})();
