/* 聊天工作台交互：环境下拉、临时思考、工具状态、记录索引与用量。 */
window.CoolzhuChatExperience = (() => {
  const state = { popup: null, trigger: null, moved: [], reasoning: new Map(), tools: new Map(), pendingMessages: new Map(),
    turnStarted: 0, streamScope: null, timer: null, searchVersion: 0, searchOffset: 0, timings: {}, indices: {}, initialized: false,
    room: null, workspace: null, scopeVersion: 0, locateVersion: 0, switching: false, modelWorkspace: null, modelActiveId: null };
  const qs = (role) => document.querySelector(`[data-role="${role}"]`);
  const transientKinds = new Set(["reasoning", "tool-call", "tool-summary", "tool-result", "computer-use", "vision-computer-use"]);
  const duration = (ms) => ms < 60000 ? `${(ms / 1000).toFixed(1)} 秒` : `${Math.floor(ms / 60000)} 分 ${Math.floor(ms / 1000) % 60} 秒`;
  const button = (label, action) => {
    const node = document.createElement("button"); node.type = "button"; node.textContent = label;
    // 列表条目保留名称；固定操作使用图标，避免抹掉工程路径或消息搜索结果。
    const icons = {"关闭":"stop", "切换目录":"check", "浏览当前工程文件…":"folder", "保存当前 Agent 头像":"save"};
    if (icons[label]) setWuxiaIconOnly(node, icons[label], label);
    node.addEventListener("click", action); return node;
  };
  const note = (text) => { const node = document.createElement("p"); node.textContent = text; return node; };
  function clearReport() { qs("chat-feedback")?.remove(); }
  function report(error) {
    // 运行过程收起后，环境切换/定位失败仍须可见，不能写到隐藏的过程标签。
    clearReport();
    const target = document.createElement("div"); target.className = "chat-feedback";
    target.dataset.role = "chat-feedback"; target.setAttribute("role", "alert");
    target.append(note(error?.message || String(error)), button("关闭", clearReport));
    if (state.popup) state.popup.append(target);
    else document.querySelector(".chat-main-column .composer")?.before(target);
  }
  const workspaceKey = () => activeWorkspaceKey || "default";
  const scopeSnapshot = () => ({ room: activeChatRoomId, workspace: workspaceKey(), version: state.scopeVersion });
  const scopeMatches = scope => scope.room === activeChatRoomId && scope.workspace === workspaceKey() && scope.version === state.scopeVersion;
  async function changeEnvironment(action) {
    if (state.switching) return;
    if (state.modelSettings?.canLeave && !state.modelSettings.canLeave()) return;
    saveComposerDraft();
    if (state.avatarSaving) { report("正在保存头像，请稍后切换环境。"); return; }
    if (activeChatAbortController) { report("本轮正在运行，请等待结束或中止后再切换环境。"); return; }
    state.switching = true;
    // 整体锁住选择器和输入，避免两个 activate 或 workspace 请求交错。
    const controls = [...document.querySelectorAll('.environment-popover button, .environment-popover input, .environment-popover select, [data-action="overview-agent-settings"], [data-action="top-chat-room"], [data-role="overview-workspace-name"], [data-action="send-message"], [data-role="message-input"], [data-role="unified-model-settings"] input, [data-role="unified-model-settings"] textarea, [data-role="unified-model-settings"] button, [data-role="unified-model-settings"] select, [data-role="session-extra-controls"] button, [data-role="session-extra-controls"] select')];
    const previous = controls.map(node => [node, node.disabled]);
    const containers = [qs("unified-model-settings"), qs("session-extra-controls")].filter(Boolean);
    const previousInert = containers.map(node => [node, node.inert]);
    containers.forEach(node => { node.inert = true; });
    controls.forEach(node => { node.disabled = true; });
    try { await action(); }
    finally {
      previous.forEach(([node, disabled]) => { node.disabled = disabled; });
      previousInert.forEach(([node, inert]) => { node.inert = inert; });
      state.switching = false;
    }
  }
  function closePopup(focus = false) {
    for (const [node, marker] of state.moved) marker.replaceWith(node);
    state.moved = [];
    state.popup?.remove(); state.popup = null;
    state.trigger?.setAttribute("aria-expanded", "false");
    if (focus) state.trigger?.focus();
    state.trigger = null;
  }
  function moveInto(selector, container) {
    const node = document.querySelector(selector); if (!node) return;
    const marker = document.createComment("环境选择控件原位"); node.before(marker);
    state.moved.push([node, marker]); container.append(node);
  }
  function popup(kind) {
    if (state.switching) return;
    const selectors = { room: '[data-action="top-chat-room"]', agent: '[data-action="overview-agent-settings"]', workspace: '[data-role="overview-workspace-name"]' };
    const trigger = document.querySelector(selectors[kind]);
    if (state.trigger === trigger) { closePopup(true); return; }
    closePopup(); state.trigger = trigger;
    const panel = document.createElement("section"); panel.className = "environment-popover";
    panel.setAttribute("role", "dialog"); panel.setAttribute("aria-label", {room:"切换聊天室",agent:"选择发送对象",workspace:"切换工程目录"}[kind]);
    const head = document.createElement("header"); head.append(note(panel.getAttribute("aria-label")), button("关闭", () => closePopup(true)));
    panel.append(head); document.body.append(panel); state.popup = panel;
    trigger?.setAttribute("aria-expanded", "true"); trigger?.setAttribute("aria-haspopup", "dialog");
    const rect = trigger?.getBoundingClientRect();
    panel.style.left = `${Math.max(8, Math.min(rect?.left || 8, innerWidth - 372))}px`;
    panel.style.top = `${Math.min((rect?.bottom || 60) + 8, innerHeight - 140)}px`;
    if (activeChatAbortController) {
      panel.append(note("本轮正在运行，请等待结束或中止后再切换环境。"));
    } else if (kind === "room") {
      moveInto('[data-sidebar-group="conversation-list"]', panel);
      moveInto('[data-sidebar-group="chat-actions"]', panel);
      // 在旧列表的冒泡处理之前接管选择，复用真正的聊天室切换函数。
      panel.addEventListener("click", event => {
        const option = event.target.closest("[data-room-id]"); if (!option) return;
        event.preventDefault(); event.stopPropagation();
        void changeEnvironment(async () => {
          const previous = activeChatRoomId;
          try { saveComposerDraft(); activeChatRoomId = option.dataset.roomId; await openSelectedChatRoom(); closePopup(); }
          catch (error) { activeChatRoomId = previous; report(error); }
        });
      }, true);
    } else if (kind === "agent") {
      moveInto('[data-sidebar-group="recipient-targets"]', panel);
      const wrap = panel.querySelector(".agent-dropdown-wrap"); wrap?.classList.add("open");
      panel.querySelector('[data-role="agent-trigger"]')?.setAttribute("aria-expanded", "true");
    } else {
      const selectedWorkspace = qs("overview-workspace-name")?.dataset.workspacePath || "";
      const current = selectedWorkspace || qs("chat-workspace-path")?.title || "";
      const selectedWorkspaceKey = workspaceKey();
      const paths = [current].filter(Boolean);
      const recentList=document.createElement("div"); recentList.className="workspace-recents"; panel.append(recentList);
      void requestJson("/api/workspaces").then(data=>{
        if(!panel.isConnected) return;
        if(data.notice) recentList.append(note(data.notice));
        for(const project of data.projects || []) {
          if(!paths.includes(project.path)) paths.push(project.path);
          const row=document.createElement("div"); row.className="workspace-recent-row";
          row.append(button(`${project.current ? "● " : ""}${workspaceDisplayPath(project.path)}`,()=>void apply(project.path)));
          if(project.protected) { const label=note("默认工程 · 保留"); label.className="workspace-reserved-label"; row.append(label); }
          if(!project.current && !project.protected) row.append(button("移除",async()=>{
            if(!window.confirm("从最近工程中移除？工程文件与聊天历史会保留。"))return;
            try { await requestJson("/api/workspaces/remove",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:project.path})}); row.remove(); }
            catch(error){errorNode.textContent=error.message;}
          }));
          if(!project.current && !project.protected && project.available) row.append(button("删除磁盘工程…",async()=>{
            try {
              const prepared=await requestJson("/api/workspaces/recycle/prepare",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:project.path})});
              if(!window.confirm(`删除工程 ${prepared.name}？\n${workspaceDisplayPath(prepared.path)}\n\n${prepared.impact}`))return;
              const name=window.prompt(`再次确认：请输入工程名「${prepared.name}」。取消不会删除文件。`,"");
              if(name===null)return;
              if(name!==prepared.name)throw new Error("工程名称不匹配，未删除文件。");
              const result=await requestJson("/api/workspaces/recycle/confirm",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({token:prepared.token,name,confirmed_twice:true})});
              row.remove();paths.splice(0,paths.length,...paths.filter(path=>path!==project.path));
              try{localStorage.setItem("coolzhu.workspace-recent",JSON.stringify(paths));}catch{}
              errorNode.textContent=result.warning||"工程已移入系统回收站。";
            } catch(error){errorNode.textContent=error.message;}
          }));
          recentList.append(row);
        }
      }).catch(error=>{errorNode.textContent=error.message;});
      const input = document.createElement("input"); input.value = workspaceDisplayPath(current); input.placeholder = "工程完整路径"; input.setAttribute("aria-label", "工程目录路径");
      const apply = async path => {
        if (!path.trim()) return;
        if (selectedWorkspace && workspaceDisplayPath(path) === workspaceDisplayPath(selectedWorkspace) && !state.switching
          && workspaceKey() === selectedWorkspaceKey
          && qs("overview-workspace-name")?.dataset.workspacePath === selectedWorkspace) {
          closePopup(true);
          return;
        }
        await changeEnvironment(async () => {
          try {
            const response = await requestJson("/api/workspace", {method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:path.trim()})});
            try { localStorage.setItem("coolzhu.recent-workspaces", JSON.stringify([...new Set([response.workspace,...paths])].slice(0,8))); } catch { /* 存储不可用时仍可切换 */ }
            closePopup(); await refreshWorkspaceBoundState(response.workspace); syncModelSession(activeSessionId, { force: true });
          } catch (error) { showWorkspaceError(error, path.trim(), "switch"); report(error); }
        });
      };
      const errorNode = note(""); errorNode.setAttribute("role", "alert");
      const parentDir = p => { const t = String(p || "").trim().replace(/[\\/]+$/, ""); const i = Math.max(t.lastIndexOf("\\"), t.lastIndexOf("/")); return i > 2 ? t.slice(0, i) : ""; };
      const showWorkspaceError = (error, grantPath, grantAction) => {
        const message = error?.message || String(error);
        const grantable = /不在允许/.test(message) && grantPath && !/^[A-Za-z]:[\\/]?$/.test(grantPath);
        errorNode.textContent = grantable ? `${message}；也可点击「授权此目录」直接加入允许范围` : message;
        authorizeButton.hidden = !grantable;
        if (grantable) { authorizeButton.dataset.grantPath = grantPath; authorizeButton.dataset.grantAction = grantAction || "switch"; }
      };
      const authorizeButton = button("授权此目录", async () => {
        const grantPath = authorizeButton.dataset.grantPath || "";
        if (!grantPath) return;
        if (!window.confirm(`授权后，本工程的工具可在该目录及其子目录内工作。\n\n授权目录：${workspaceDisplayPath(grantPath)}\n\n是否继续？`)) return;
        authorizeButton.disabled = true;
        try {
          await requestJson("/api/tools/allowed-roots", {method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:grantPath})});
          authorizeButton.hidden = true; errorNode.textContent = "";
          if (authorizeButton.dataset.grantAction === "create") await createProject();
          else await apply(grantPath);
        } catch (error) { showWorkspaceError(error, grantPath, authorizeButton.dataset.grantAction); }
        finally { authorizeButton.disabled = false; }
      });
      authorizeButton.hidden = true;
      const createProject = async () => {
        try {
          if(!input.value.trim())throw new Error("请填写新工程的完整路径。");
          const created=await requestJson("/api/workspaces",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:input.value.trim()})});
          await apply(created.path);
        } catch(error){ showWorkspaceError(error, parentDir(input.value), "create"); }
      };
      input.addEventListener("keydown", event => { if (event.key === "Enter") { event.preventDefault(); void apply(input.value); } });
      panel.append(input, button("切换目录", () => void apply(input.value)), button("新建工程",()=>void createProject()), errorNode, authorizeButton, note("默认工程保留。移除保留文件；删除子工程需二次确认，文件与历史移入回收站。"));
      const browse = button("浏览当前工程文件…", () => { closePopup(); openChatToolWindow("project"); });
      browse.disabled = !selectedWorkspace;
      panel.append(browse);
    }
    panel.querySelector('input, button')?.focus();
  }
  function rememberReasoning(id, value) {
    state.reasoning.set(id, String(value || "").slice(-6000));
    while (state.reasoning.size > 8) state.reasoning.delete(state.reasoning.keys().next().value);
  }
  function activeThinking() {
    window.CoolzhuRunActivity?.thinking([...state.reasoning.values()].filter(Boolean).join("\n"));
  }
  const toolStatusLabels = { requested: "等待执行", running: "执行中", completed: "已完成", failed: "失败", interrupted: "已中止", "not-executed": "未执行", unknown: "状态未确认" };
  const toolTerminalStates = new Set(["completed", "failed", "interrupted", "not-executed", "unknown"]);
  function toolDisplaySummary(value) {
    const text = String(value || "");
    // 兼容旧服务/旧审计：结构化结果或源码只能留在审计，状态栏仅保留文件引用。
    if (!/"[^"\n]+"\s*:|<\/?[a-zA-Z][^>]*>/.test(text)) return text.slice(0, 280);
    const paths = [];
    for (const match of text.matchAll(/"(?:filePath|file_path|path|artifact_path)"\s*:\s*("(?:\\.|[^"\\])*")/g)) {
      try { const path = JSON.parse(match[1]); if (path.length <= 1024 && !/[\x00-\x1f]/.test(path) && !paths.includes(path)) paths.push(path); } catch { /* 结果截断时不猜路径 */ }
      if (paths.length >= 3) break;
    }
    return paths.length ? `相关文件：${paths.join("；")}` : "";
  }
  function renderToolStates() {
    const rows = [...state.tools.values()].filter(value => !toolTerminalStates.has(value.status))
      .map(value => `${value.name} · ${toolStatusLabels[value.status] || value.status}${value.summary ? `：${value.summary}` : ""}`);
    window.CoolzhuRunActivity?.tools(rows);
  }
  function toolStatus(message) {
    const text = String(message.content || message.text || "");
    const structured = !!message.call_id;
    const id = String(message.call_id || text.match(/^tool_call_id:\s*(\S+)/m)?.[1] || message.id || `legacy-${state.tools.size}`).replace(/^tool-result-/, "tool-call-");
    const previous = state.tools.get(id);
    // 新协议已给出可靠状态，旧摘要/结果只是同一调用的审计副本，不能再增加一条或覆盖终态。
    if (!structured && previous?.structured) return;
    const name = String(message.tool_name || text.match(/^tool_name:\s*(\S+)/m)?.[1] || text.match(/^工具 `([^`]+)`/)?.[1] || text.match(/^模型请求工具：([^\s{]+)/)?.[1] || previous?.name || "工具");
    const rawStatus = message.status || text.match(/^status:\s*(\S+)/m)?.[1] || (text.includes(" 失败：") ? "failed" : text.includes(" 未执行：") || text.includes("（预览，未执行真实操作）") ? "not-executed" : text.includes(" 已完成：") ? "completed" : "requested");
    const status = ({ok:"completed", "dry-run-only":"not-executed", rejected:"failed", timeout:"failed", timed_out:"failed", blocked:"failed", cancelled:"interrupted"})[rawStatus] || rawStatus;
    if (previous?.structured && (toolTerminalStates.has(previous.status) || (previous.status === "running" && status === "requested"))) return;
    state.tools.set(id, { name, status, structured, summary: toolDisplaySummary(message.summary || (structured ? "" : text)) });
    renderToolStates();
  }
  function finishTools(status) {
    for (const value of state.tools.values()) {
      if (!["requested", "running"].includes(value.status)) continue;
      value.status = status === "interrupted" ? "interrupted" : status === "failed" ? "failed" : value.status === "requested" ? "not-executed" : "unknown";
      value.summary = value.status === "unknown" ? "未收到工具终态，不能确认执行结果" : "本轮已结束";
    }
    renderToolStates();
  }
  function intercept(message, streaming = false) {
    const kind = String(message?.kind || "").trim().toLowerCase().replaceAll("_", "-");
    if (kind === "reasoning") {
      if (streaming) rememberReasoning(message.id, message.content);
      else state.reasoning.delete(message.id);
      activeThinking(); return true;
    }
    if (transientKinds.has(kind) || String(message?.role || "").trim().toLowerCase() === "tool") { toolStatus(message); return true; }
    if (message?.role === "assistant" || kind === "assistant-reply" || kind === "assistant-fallback") {
      if (streaming && !String(message.content || "").trim()) { state.pendingMessages.set(message.id, message); return true; }
      state.pendingMessages.delete(message.id);
      if (String(message.content || "").trim()) { state.reasoning.clear(); activeThinking(); }
    }
    return false;
  }
  function streamEvent(event, data) {
    if (event === "tool_status") {
      const scope = state.streamScope;
      if (scope?.turn && scopeMatches(scope) && data?.turn_id === scope.turn && data?.chat_room_id === scope.room) toolStatus(data);
      return true;
    }
    if (event === "started") {
      if (data?.chat_room_id && data.chat_room_id !== activeChatRoomId) return true;
      state.streamScope = {...scopeSnapshot(), turn: String(data?.turn_id || "")};
      clearReport(); state.turnStarted = Date.now(); state.reasoning.clear(); state.tools.clear(); activeThinking();
      window.CoolzhuRunActivity?.start(); qs("chat-activity-label").textContent = "正在思考…";
      clearInterval(state.timer);
      state.timer = setInterval(() => { qs("chat-turn-time").textContent = `本轮 ${duration(Date.now() - state.turnStarted)}`; }, 200);
    } else if (event === "message_delta" && data?.kind === "reasoning") {
      if (String(data.delta || "").startsWith("模型请求工具：")) { toolStatus({id:data.id,content:data.delta}); return true; }
      rememberReasoning(data.id, (state.reasoning.get(data.id) || "") + (data.delta || "")); activeThinking(); return true;
    } else if (event === "message_delta") {
      state.reasoning.clear(); activeThinking(); qs("chat-activity-label").textContent = "正在回复…";
    } else if (event === "done" || event === "error") {
      if (data?.turn_id && (!state.streamScope || data.turn_id !== state.streamScope.turn || !scopeMatches(state.streamScope))) return true;
      finishTools(event === "error" ? "failed" : data?.status);
      finish(); qs("chat-activity-label").textContent = event === "error" || data?.status === "failed" ? "本轮未完成" : data?.status === "interrupted" ? "已中止" : "本轮完成";
      void refreshInsights();
      if (chatToolWindowId === "history") void search();
    }
    return false;
  }
  function finish() { window.CoolzhuRunActivity?.finish(); clearInterval(state.timer); state.timer = null; state.streamScope = null; state.reasoning.clear(); state.pendingMessages.clear(); activeThinking(); }
  function roomChanged(roomId) {
    const workspace = workspaceKey();
    if (state.room === roomId && state.workspace === workspace) return;
    state.room = roomId; state.workspace = workspace; state.scopeVersion++; state.locateVersion++;
    clearReport();
    window.CoolzhuWorkspacePanels?.scopeChanged();
    state.searchVersion++; state.searchOffset = 0; clearTimeout(state.searchTimer);
    finish(); state.tools.clear(); state.turnStarted = 0; state.timings = {}; state.indices = {};
    const hide = role => { const node = qs(role); if (node) node.hidden = true; };
    hide("chat-return-latest"); hide("chat-history-more");
    qs("chat-history-results")?.replaceChildren();
    qs("chat-usage-list")?.replaceChildren();
    usageVersion++; if(qs("chat-usage-meter"))qs("chat-usage-meter").textContent="用量加载中";
    for (const role of ["chat-turn-time", "chat-history-count", "chat-usage-note"]) { const node = qs(role); if (node) node.textContent = ""; }
    const label = qs("chat-activity-label"); if (label) label.textContent = "就绪";
    // 即使历史面板已打开，也不能保留上一房间的点击目标。
    if (roomId && chatToolWindowId === "history") void search();
    syncModelSession(activeSessionId);
  }
  function prepareDelta(id, delta) {
    const pending = state.pendingMessages.get(id);
    if (!pending) return false;
    state.pendingMessages.delete(id);
    upsertMessage({...pending, content: delta}, {streaming: true});
    return true;
  }
  function annotate() {
    chatMessageList()?.querySelectorAll(".message").forEach(article => {
      let meta = article.querySelector(".message-run-meta");
      if (!meta) { meta = document.createElement("small"); meta.className = "message-run-meta"; article.querySelector("time")?.append(meta); }
      const id = article.dataset.messageId;
      const parts = [];
      if (state.indices[id]) parts.push(`#${state.indices[id]}`);
      if (state.timings[id] != null) parts.push(`本轮 ${duration(state.timings[id])}`);
      meta.textContent = parts.join(" · ");
    });
  }

  let usageVersion=0;
  const countText=value=>value==null?"未知":Number(value).toLocaleString("zh-CN",{maximumFractionDigits:1});
  async function refreshUsage() {
    const scope=scopeSnapshot(),version=++usageVersion,room=activeChatRoomId; if(!room)return;
    const current=()=>scopeMatches(scope)&&version===usageVersion;
    const meter=qs("chat-usage-meter"),list=qs("chat-usage-list");
    try {
      const filter=qs("usage-scope")?.value||"room",hours=qs("usage-period")?.value||"0";
      const query=new URLSearchParams({hours}); if(filter==="room")query.set("room_id",room);
      const paths=[`/api/usage?${query}`]; if(filter!=="room"||hours!=="0")paths.push(`/api/usage?room_id=${encodeURIComponent(room)}`);
      const results=await Promise.all(paths.map(path=>requestJson(path))); if(!current())return;
      const data=results[0],total=(results[1]||data).summary;
      if(meter) {
        const mode=localStorage.getItem("coolzhu.usage-meter")||"both";meter.hidden=mode==="hidden";
        const token=total?`${countText(total.total_tokens)} token`:"暂无用量";
        const rate=total?.tokens_per_second==null?"平均 — tok/s":`平均 ${countText(total.tokens_per_second)} tok/s`;
        meter.textContent=mode==="total"?token:mode==="rate"?rate:`${token} · ${rate}`;
        meter.title=total?.unknown_requests?`${total.unknown_requests} 次请求用量未完整上报；已知部分 ${countText(total.known_total_tokens)} token` : data.note;
      }
      if(!list)return;list.replaceChildren();
      for(const media of data.media||[]) list.append(note(`工程${media.kind==="image"?"图像":"视频"}生成：${media.jobs} 次 · ${media.units} 个产物 · 时长 ${media.duration_seconds??"—"} 秒 · 厂商已报告费用 ${media.reported_cost??"—"}（${media.known_cost_jobs} 次有费用回执）；与对话 token 分开。`));
      if(!data.summary){list.append(note("所选范围暂无对话用量记录。"));return;}
      const summary=data.summary;
      list.append(note(`总 token ${countText(summary.total_tokens)} · 已知部分 ${countText(summary.known_total_tokens)} · 请求 ${summary.requests} · 失败 ${summary.failed||0} · 未完整上报 ${summary.unknown_requests}`));
      list.append(note(`平均解码速率 ${countText(summary.tokens_per_second)} tok/s · 首 token 均值 ${countText(summary.ttft_ms)} ms · 有效计时 ${summary.timed_requests} 次`));
      const chart=document.createElement("div");chart.className="usage-bars";chart.setAttribute("aria-label","按模型的已知 token 用量");
      const max=Math.max(1,...data.models.map(row=>row.known_total_tokens||0));
      for(const row of data.models) {
        const label=document.createElement("div");label.textContent=`${row.key} · ${countText(row.known_total_tokens)} token · ${row.requests} 次`;
        const bar=document.createElement("span");bar.style.width=`${100*(row.known_total_tokens||0)/max}%`;bar.className="usage-bar";label.append(bar);chart.append(label);
      }list.append(chart);
      const table=document.createElement("table");table.className="usage-table";
      const heading=document.createElement("tr");for(const label of ["会话 / UTC 时段","请求","输入","输出","缓存读","缓存写","平均 tok/s"]){const th=document.createElement("th");th.textContent=label;heading.append(th);}table.append(heading);
      const rows=qs("usage-group")?.value==="time"?data.series:data.sessions;
      for(const row of rows){const tr=document.createElement("tr");const name=sessionRegistry.sessions?.find(item=>item.id===row.key)?.display_name||row.key;for(const value of [name,row.requests,countText(row.input_tokens),countText(row.output_tokens),countText(row.cache_read_tokens),countText(row.cache_write_tokens),countText(row.tokens_per_second)]){const td=document.createElement("td");td.textContent=value;tr.append(td);}table.append(tr);}list.append(table);
      qs("chat-usage-note").textContent=data.note;
    }catch(error){if(current()){if(list)list.textContent=`用量读取失败：${error.message}`;if(meter)meter.textContent="用量读取失败 · 点击重试";}}
  }

  async function refreshInsights() {
    const room = activeChatRoomId; if (!room) return;
    const scope = scopeSnapshot();
    try {
      const ids = [...(chatMessageList()?.querySelectorAll(".message[data-message-id]") || [])].slice(-500).map(node => node.dataset.messageId);
      const response = await requestJson(`/api/chat/rooms/${encodeURIComponent(room)}/insights?message_ids=${encodeURIComponent(ids.join(","))}`);
      if (!scopeMatches(scope)) return;
      state.timings = response.timings || {}; state.indices = response.indices || {}; annotate();
      void refreshUsage();
    } catch (error) { const list = qs("chat-usage-list"); if (scopeMatches(scope) && list) list.textContent = `用量加载失败：${error.message}`; }
  }
  async function search(append = false) {
    const room = activeChatRoomId; if (!room) return;
    const scope = scopeSnapshot();
    const version = ++state.searchVersion;
    if (!append) state.searchOffset = 0;
    const query = qs("chat-history-query")?.value || "";
    const list = qs("chat-history-results");
    if (!append && list) list.textContent = "正在检索完整历史…";
    try {
      const response = await requestJson(`/api/chat/rooms/${encodeURIComponent(room)}/search?q=${encodeURIComponent(query)}&offset=${state.searchOffset}&limit=50`);
      if (version !== state.searchVersion || !scopeMatches(scope)) return;
      if (!append) list.replaceChildren();
      qs("chat-history-count").textContent = `${response.total} 条${query ? "匹配记录" : "消息"}`;
      for (const item of response.items) {
        const entry = button(`#${item.index} · ${String(item.author || "").trim() || "你"} · ${new Date(item.created_at).toLocaleString("zh-CN")}\n${item.snippet}`, () => { if (scopeMatches(scope)) void locate(item.id); });
        entry.className = "chat-history-result"; entry.dataset.locateMessage = item.id; list.append(entry);
      }
      if (!response.items.length && !append) list.append(note("没有匹配的记录。"));
      state.searchOffset += response.items.length;
      qs("chat-history-more").hidden = !response.has_more;
    } catch (error) { if (version === state.searchVersion && scopeMatches(scope)) list.textContent = `搜索失败：${error.message}`; }
  }
  async function locate(id) {
    const room = activeChatRoomId;
    const scope = scopeSnapshot(); const version = ++state.locateVersion;
    chatHistorySync?.stop();
    let article = chatMessageList()?.querySelector(`[data-message-id="${CSS.escape(id)}"]`);
    try {
      if (!article) {
        const response = await requestJson(`/api/chat/rooms/${encodeURIComponent(room)}/search?around=${encodeURIComponent(id)}`);
        if (!scopeMatches(scope) || version !== state.locateVersion) return;
        const list = chatMessageList(); list.replaceChildren();
        for (const [offset, message] of response.messages.entries()) {
          state.indices[message.id] = (response.start_index || Math.max(1, response.position - 20)) + offset;
          upsertMessage(message);
        }
        messagePaging = {roomId:room, hasMore:response.has_older, nextBefore:response.messages[0]?.id};
        updateLoadOlderButton();
        article = list.querySelector(`[data-message-id="${CSS.escape(id)}"]`);
      }
      if (article) qs("chat-return-latest").hidden = false;
      annotate(); article?.scrollIntoView({block:"center",behavior:"smooth"}); article?.classList.add("is-located-message");
      setTimeout(() => article?.classList.remove("is-located-message"), 2200);
    } catch (error) { if (scopeMatches(scope) && version === state.locateVersion) report(error); }
    finally {
      if (scopeMatches(scope) && version === state.locateVersion) activateChatHistorySync();
    }
  }
  function openHistory() { openChatToolWindow("history"); void search(); }
  function describeRecipientEditor(targetIds = getSelectedAgentIds()) {
    state.modelSettings?.setSending?.(targetIds);
  }
  function recipientTargetsChanged(targetIds = getSelectedAgentIds()) {
    if (!state.initialized || !state.modelSettings) return;
    const editor = state.modelSettings;
    const validTargets = targetIds.filter(id => sessionRegistry.sessions?.some(session => session.id === id));
    const preferred = validTargets.length === 1 ? validTargets[0]
      : validTargets.includes(editor.sessionId) ? editor.sessionId : validTargets[0];
    if (preferred && editor.sessionId !== preferred && !editor.dirty) {
      void editor.select(preferred).catch(error => report(error));
    }
    describeRecipientEditor(targetIds);
  }
  function mountModelSettings(id) {
    const workspacePath = qs("overview-workspace-name")?.dataset.workspacePath || "";
    if (!workspacePath) { state.modelSettings?.destroy(); state.modelSettings = null; return; }
    const selectedTargets = getSelectedAgentIds();
    const preferred = selectedTargets.length === 1 ? selectedTargets[0] : selectedTargets.includes(id) ? id : selectedTargets[0] || id;
    state.modelWorkspace = workspaceKey(); state.modelWorkspacePath = workspacePath; state.modelActiveId = id;
    state.modelSettings = window.CoolzhuModelSettings?.mount(qs("unified-model-settings"), {
      workspacePath,
      sessionId: preferred,
      onSelected: () => describeRecipientEditor(),
      onChanged: () => describeRecipientEditor(),
      onSaved: async () => {
        try { await loadSessions(); await loadAgents(); await loadGoalRoles(); }
        catch (error) { report(`参数已保存，但界面刷新失败：${error.message}`); }
      },
      onDeleted: async (_, { deleted } = {}) => {
        if (deleted) document.querySelectorAll(`.message[data-session-id="${CSS.escape(deleted)}"]`).forEach(node => node.remove());
        try { await loadSessions(); await loadAgents(); await loadGoalRoles(); }
        catch (error) { report(`会话已删除，但界面刷新失败：${error.message}`); }
        describeRecipientEditor();
      },
    });
    describeRecipientEditor();
  }
  function syncModelSession(id, { force = false } = {}) {
    const scopeLabel = qs("session-extra-scope");
    const selected = sessionRegistry.sessions?.find(session => session.id === id);
    if (scopeLabel) scopeLabel.textContent = `当前管理的会话：${selected?.display_name || selected?.name || id || "未选择"}；与上方“编辑配置的会话”独立，发送对象以顶栏为准。`;
    const saveAvatar = qs("session-avatar-save"); if (saveAvatar && !state.avatarSaving) saveAvatar.disabled = !id;
    if (!state.initialized) return;
    // 工程隔离优先：销毁旧作用域表单，避免同名会话 ID 或旧草稿跨工程保存。
    if (state.modelWorkspace !== workspaceKey() || state.modelWorkspacePath !== qs("overview-workspace-name")?.dataset.workspacePath || !state.modelSettings) { mountModelSettings(id); return; }
    state.modelActiveId = id;
    const editor = state.modelSettings;
    const container = qs("unified-model-settings");
    if (!force && (editor.dirty || container?.contains(document.activeElement))) { describeRecipientEditor(); return; }
    const selectedTargets = getSelectedAgentIds();
    const preferred = selectedTargets.length === 1 ? selectedTargets[0]
      : selectedTargets.includes(editor.sessionId) ? editor.sessionId : selectedTargets[0] || id;
    void editor.refresh(preferred).then(() => describeRecipientEditor()).catch(error => report(error));
  }
  function mountExtraControls() {
    const container = qs("session-extra-controls"); if (!container) return;
    container.classList.add("session-extra-controls");
    const heading = document.createElement("h2"); heading.textContent = "Agent 外观与视觉路由";
    const scope = note(""); scope.dataset.role = "session-extra-scope";
    container.append(heading, scope);
    const avatar = document.querySelector(".session-agent-grid .avatar-config-field");
    const vision = qs("settings-vision-agent")?.closest("label");
    if (avatar) container.append(avatar);
    const feedback = note(""); feedback.setAttribute("role", "status");
    const save = button("保存当前 Agent 头像", async () => {
      if (state.avatarSaving || state.switching || !activeSessionId) return;
      const sessionId = activeSessionId; const workspace = workspaceKey();
      const avatarPath = currentAvatarPathFromForm();
      state.avatarSaving = true; save.disabled = true; feedback.textContent = "正在保存头像…";
      try {
        await requestJson(`/api/sessions/${encodeURIComponent(sessionId)}`, {
          method: "PATCH", headers: {"Content-Type":"application/json"}, body: JSON.stringify({avatar: avatarPath}),
        });
        if (workspace !== workspaceKey()) return;
        await loadSessions(); await loadAgents(); feedback.textContent = "头像已保存。";
      } catch (error) { if (workspace === workspaceKey()) feedback.textContent = `头像保存失败：${error.message}`; }
      finally { state.avatarSaving = false; save.disabled = !activeSessionId; }
    });
    save.dataset.role = "session-avatar-save"; container.append(save, feedback);
    if (vision) container.append(vision, note("视觉 Agent 的选择即时保存，用于图像理解与视觉任务。"));
  }
  // ── 目录代码图谱：状态 / 更新索引 / 检索注入开关 / 符号检索 ────────────────
  function graphSetOpen(open) {
    const pop = qs("code-graph-popover"), trigger = document.querySelector('[data-action="code-graph-open"]');
    if (!pop) return;
    pop.hidden = !open;
    trigger?.setAttribute("aria-expanded", String(open));
    if (open) { void graphRefreshStatus(); qs("code-graph-query")?.focus(); }
  }
  async function graphRefreshStatus() {
    const status = qs("code-graph-status");
    try {
      const data = await requestJson("/api/code-graph/status");
      if (status) {
        status.textContent = data.indexed
          ? `${data.file_count} 文件 · ${data.symbol_count} 符号 · ${data.edge_count} 引用边${data.built_at ? ` · ${new Date(data.built_at).toLocaleString("zh-CN", { hour12: false })}` : ""}`
          : "尚未建立索引——点「更新索引」扫描当前工作区";
      }
      const inject = qs("code-graph-inject");
      if (inject) inject.checked = Boolean(data.inject_enabled);
    } catch (error) {
      if (status) status.textContent = `状态读取失败：${error.message}`;
    }
  }
  async function graphRebuild() {
    const status = qs("code-graph-status");
    if (status) status.textContent = "正在扫描工作区…";
    try {
      const data = await requestJson("/api/project/symbol-index", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ full: false }) });
      if (status) status.textContent = `完成：${data.file_count} 文件 · ${data.symbol_count} 符号 · ${data.edge_count ?? 0} 引用边 · ${data.elapsed_ms}ms`;
      void graphRefreshStatus();
    } catch (error) {
      if (status) status.textContent = `索引失败：${error.message}`;
    }
  }
  async function graphSetInject(enabled) {
    try {
      await requestJson("/api/code-graph/inject", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ enabled }) });
    } catch (error) { report(error); void graphRefreshStatus(); }
  }
  function graphRenderResults(data) {
    const list = qs("code-graph-results");
    if (!list) return;
    const rows = [];
    for (const hit of data.hits || []) {
      const li = document.createElement("li");
      li.className = "code-graph-hit";
      const main = document.createElement("b");
      main.textContent = `${hit.name}`;
      const loc = document.createElement("span");
      loc.className = "code-graph-loc";
      loc.textContent = ` [${hit.kind}] ${hit.path}:${hit.line}`;
      li.append(main, loc);
      const edges = [];
      if (hit.imports?.length) edges.push(`imports→${hit.imports.join(", ")}`);
      if (hit.imported_by?.length) edges.push(`←${hit.imported_by.join(", ")}`);
      if (edges.length) {
        const rel = document.createElement("small");
        rel.textContent = edges.join("  ");
        li.append(rel);
      }
      rows.push(li);
    }
    for (const file of (data.files || []).slice(0, 4)) {
      const li = document.createElement("li");
      li.className = "code-graph-hit is-file";
      li.textContent = `📄 ${file}`;
      rows.push(li);
    }
    list.replaceChildren(...rows);
    if (!rows.length) {
      const li = document.createElement("li");
      li.className = "code-graph-empty";
      li.textContent = data.indexed === false ? "索引未建立" : "无命中";
      list.append(li);
    }
  }
  async function graphQuery() {
    const query = qs("code-graph-query")?.value.trim() || "";
    const list = qs("code-graph-results");
    if (!query) { list?.replaceChildren(); return; }
    try {
      const data = await requestJson(`/api/code-graph/query?q=${encodeURIComponent(query)}&limit=12`);
      graphRenderResults(data);
    } catch (error) { report(error); }
  }

  // ── 房间任务清单：工具栏徽标合计 + 弹层按会话分组（todos-changed 事件驱动，不轮询）──
  let todosVersion = 0;
  function todosAgentName(sessionId) {
    const agents = Array.isArray(agentRegistry?.agents) ? agentRegistry.agents : [];
    const found = agents.find(agent => agent?.id === sessionId);
    return found?.display_name || found?.name || sessionId || "未知会话";
  }
  // 徽标合计：跨会话汇总「完成数/总数」；纯函数便于契约测试验证。
  function todosTotals(sessions) {
    let done = 0, total = 0;
    for (const session of Array.isArray(sessions) ? sessions : []) {
      const todos = Array.isArray(session?.todos) ? session.todos : [];
      total += todos.length;
      done += todos.filter(todo => todo?.status === "completed").length;
    }
    return { done, total };
  }
  function todosRender(data) {
    const badge = qs("chat-todos-badge"), summary = qs("chat-todos-summary"), list = qs("chat-todos-list");
    const sessions = (Array.isArray(data?.sessions) ? data.sessions : [])
      .filter(session => Array.isArray(session?.todos) && session.todos.length);
    const { done, total } = todosTotals(sessions);
    if (badge) { badge.hidden = total === 0; badge.textContent = total ? ` ${done}/${total}` : ""; }
    if (summary) summary.textContent = total ? `${done}/${total} 完成` : "";
    if (!list) return;
    if (!sessions.length) {
      const empty = document.createElement("p");
      empty.className = "chat-todos-empty";
      empty.textContent = "当前聊天室暂无任务清单。模型处理多步任务时会自动建立。";
      list.replaceChildren(empty);
      return;
    }
    const groups = sessions.map(session => {
      const group = document.createElement("section");
      group.className = "chat-todos-group";
      const head = document.createElement("header");
      const name = document.createElement("b");
      // 全部经 textContent 落 DOM：清单内容来自模型输出，禁止 innerHTML。
      name.textContent = todosAgentName(session.session_id);
      const meta = document.createElement("span");
      meta.className = "chat-todos-meta";
      const time = session.updated_at
        ? new Date(Number(session.updated_at)).toLocaleString("zh-CN", { hour12: false })
        : "";
      meta.textContent = ` 第 ${session.revision} 版${time ? ` · ${time}` : ""}${session.source === "acp-plan" ? " · Devin 计划" : ""}`;
      head.append(name, meta);
      const items = document.createElement("ul");
      items.className = "chat-todos-items";
      for (const todo of session.todos) {
        const status = todo?.status || "pending";
        const li = document.createElement("li");
        li.className = `chat-todos-item is-${status}`;
        const icon = document.createElement("span");
        icon.className = "chat-todos-icon";
        icon.setAttribute("aria-hidden", "true");
        icon.textContent = status === "completed" ? "✓" : status === "in_progress" ? "◐" : "○";
        const text = document.createElement("span");
        text.className = "chat-todos-text";
        text.textContent = String(todo?.content ?? "");
        li.append(icon, text);
        if (status === "in_progress" && todo?.activeForm) {
          const active = document.createElement("small");
          active.className = "chat-todos-active";
          active.textContent = String(todo.activeForm);
          li.append(active);
        }
        if (todo?.priority) {
          const priority = document.createElement("small");
          priority.className = "chat-todos-priority";
          priority.textContent = String(todo.priority);
          li.append(priority);
        }
        items.append(li);
      }
      group.append(head, items);
      return group;
    });
    list.replaceChildren(...groups);
  }
  async function todosRefresh() {
    const scope = scopeSnapshot(); const version = ++todosVersion;
    const room = scope.room;
    if (!room) { todosRender(null); return; }
    const current = () => scopeMatches(scope) && version === todosVersion;
    try {
      const data = await requestJson(`/api/chat/rooms/${encodeURIComponent(room)}/todos`);
      if (!current()) return;
      todosRender(data);
    } catch (error) {
      // 404（房间已删）与其它错误都按空清单收口：徽标/弹层不阻塞聊天主路径。
      if (!current()) return;
      todosRender(null);
    }
  }
  function todosSetOpen(open) {
    const pop = qs("chat-todos-popover"), trigger = document.querySelector('[data-action="chat-todos-open"]');
    if (!pop) return;
    pop.hidden = !open;
    trigger?.setAttribute("aria-expanded", String(open));
    if (open) void todosRefresh();
  }

  function init() {
    if (state.initialized) return; state.initialized = true;
    mountExtraControls(); mountModelSettings(activeSessionId); syncModelSession(activeSessionId);
    document.querySelectorAll('[data-action="chat-history-open"]').forEach(node => node.addEventListener("click", openHistory));
    document.querySelectorAll('[data-action="code-graph-open"]').forEach(node => node.addEventListener("click", () => graphSetOpen(qs("code-graph-popover")?.hidden ?? true)));
    document.querySelectorAll('[data-action="code-graph-close"]').forEach(node => node.addEventListener("click", () => graphSetOpen(false)));
    document.querySelectorAll('[data-action="chat-todos-open"]').forEach(node => node.addEventListener("click", () => todosSetOpen(qs("chat-todos-popover")?.hidden ?? true)));
    document.querySelectorAll('[data-action="chat-todos-close"]').forEach(node => node.addEventListener("click", () => todosSetOpen(false)));
    void todosRefresh();
    document.querySelectorAll('[data-action="code-graph-rebuild"]').forEach(node => node.addEventListener("click", () => void graphRebuild()));
    qs("code-graph-inject")?.addEventListener("change", event => void graphSetInject(event.target.checked));
    qs("code-graph-query")?.addEventListener("input", () => { clearTimeout(state.graphTimer); state.graphTimer = setTimeout(() => void graphQuery(), 220); });
    qs("chat-history-query")?.addEventListener("input", () => { clearTimeout(state.searchTimer); state.searchTimer = setTimeout(() => void search(), 180); });
    qs("chat-history-more")?.addEventListener("click", () => void search(true));
    qs("chat-return-latest")?.addEventListener("click", async () => {
      const scope = scopeSnapshot(); state.locateVersion++;
      try { await loadChatRoomMessages(scope.room); if (scopeMatches(scope)) qs("chat-return-latest").hidden = true; }
      catch (error) { if (scopeMatches(scope)) report(error); }
    });
    qs("chat-usage-refresh")?.addEventListener("click", () => void refreshUsage());
    qs("chat-usage-meter")?.addEventListener("click", () => {openChatToolWindow("usage");void refreshUsage();});
    for(const role of ["usage-scope","usage-period","usage-group"])qs(role)?.addEventListener("change",()=>void refreshUsage());
    const meterMode=qs("usage-meter-mode"); if(meterMode){meterMode.value=localStorage.getItem("coolzhu.usage-meter")||"both";meterMode.addEventListener("change",()=>{localStorage.setItem("coolzhu.usage-meter",meterMode.value);void refreshUsage();});}
    document.addEventListener("pointerdown", event => { if (state.popup && !state.popup.contains(event.target) && !state.trigger?.contains(event.target)) closePopup(); });
    document.addEventListener("keydown", event => {
      if (event.key === "Escape" && state.popup) { event.preventDefault(); closePopup(true); }
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "f") { event.preventDefault(); openHistory(); qs("chat-history-query")?.focus(); }
      if (state.popup && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        const options = [...state.popup.querySelectorAll("button:not([disabled]),input,select")];
        const index = options.indexOf(document.activeElement);
        if (options.length) { event.preventDefault(); options[(index + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length].focus(); }
      }
    });
    window.addEventListener("resize", () => closePopup());
  }
  return {init,popup,closePopup,intercept,streamEvent,finish,prepareDelta,refreshInsights,annotate,openHistory,search,locate,duration,syncModelSession,recipientTargetsChanged,roomChanged,todosRefresh,
    get changingEnvironment() { return state.switching; }};
})();
