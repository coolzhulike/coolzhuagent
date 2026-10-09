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
      let recent = [];
      try { recent = JSON.parse(localStorage.getItem("coolzhu.recent-workspaces") || "[]"); } catch { /* 首次使用 */ }
      const paths = [...new Set([current, ...(Array.isArray(recent) ? recent : [])].filter(Boolean))].slice(0, 8);
      const input = document.createElement("input"); input.value = current; input.placeholder = "输入工程目录完整路径"; input.setAttribute("aria-label", "工程目录路径");
      const apply = async path => {
        if (!path.trim()) return;
        await changeEnvironment(async () => {
          try {
            const response = await requestJson("/api/workspace", {method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({path:path.trim()})});
            try { localStorage.setItem("coolzhu.recent-workspaces", JSON.stringify([...new Set([response.workspace,...paths])].slice(0,8))); } catch { /* 存储不可用时仍可切换 */ }
            closePopup(); await refreshWorkspaceBoundState(response.workspace); syncModelSession(activeSessionId, { force: true });
          } catch (error) { errorNode.textContent = error.message; report(error); }
        });
      };
      const errorNode = note(""); errorNode.setAttribute("role", "alert");
      paths.forEach(path => panel.append(button(path, () => {
        // 仅当前已选工程生成的快捷项可免重载；旧弹层或尚未取得真实路径时走原切换流程。
        if (selectedWorkspace && path === selectedWorkspace && !state.switching
          && workspaceKey() === selectedWorkspaceKey
          && qs("overview-workspace-name")?.dataset.workspacePath === selectedWorkspace) {
          closePopup(true);
          return;
        }
        void apply(path);
      })));
      input.addEventListener("keydown", event => { if (event.key === "Enter") { event.preventDefault(); void apply(input.value); } });
      panel.append(input, button("切换目录", () => void apply(input.value)), errorNode);
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
  async function refreshInsights() {
    const room = activeChatRoomId; if (!room) return;
    const scope = scopeSnapshot();
    try {
      const ids = [...(chatMessageList()?.querySelectorAll(".message[data-message-id]") || [])].slice(-500).map(node => node.dataset.messageId);
      const response = await requestJson(`/api/chat/rooms/${encodeURIComponent(room)}/insights?message_ids=${encodeURIComponent(ids.join(","))}`);
      if (!scopeMatches(scope)) return;
      state.timings = response.timings || {}; state.indices = response.indices || {}; annotate();
      const list = qs("chat-usage-list"); if (!list) return;
      const rows = response.usage || [];
      list.replaceChildren();
      const sums = rows.reduce((sum, row) => ({input:sum.input + row.input_tokens, output:sum.output + row.output_tokens, requests:sum.requests + row.requests}), {input:0,output:0,requests:0});
      const knownTotal = (field, sum) => rows.some(row => row[field] != null) ? sum.toLocaleString() : "未知";
      list.append(note(rows.length ? `已知输入 ${knownTotal("input_tokens", sums.input)} · 已知输出 ${knownTotal("output_tokens", sums.output)} token · ${sums.requests} 次请求` : "暂无接口用量记录"));
      for (const row of rows) {
        const name = sessionRegistry.sessions?.find(session => session.id === row.session_id)?.display_name || row.session_id;
        const tokens = value => value == null ? "未知" : Number(value).toLocaleString();
        list.append(note(`${name}：输入 ${tokens(row.input_tokens)} / 输出 ${tokens(row.output_tokens)}；缓存读取 ${tokens(row.cache_read_tokens)} / 写入 ${tokens(row.cache_write_tokens)}`));
        list.append(note(`请求 ${row.requests || 0} · 已知网络尝试 ${row.attempts || 0} · 失败或中断 ${row.failed_attempts || 0} · 进行中 ${row.pending_attempts || 0} · 未发送 ${row.not_sent_attempts || 0} · 结果未知 ${row.unknown_outcome_attempts || 0} · 派发未知 ${row.unknown_dispatch_attempts || 0} · 用量未完整提供 ${row.partial_usage_attempts || 0}${row.legacy_records ? ` · 旧版用量记录 ${row.legacy_records}` : ""}`));
      }
      qs("chat-usage-note").textContent = response.note;
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
        updateLoadOlderButton(); qs("chat-return-latest").hidden = false;
        article = list.querySelector(`[data-message-id="${CSS.escape(id)}"]`);
      }
      annotate(); article?.scrollIntoView({block:"center",behavior:"smooth"}); article?.classList.add("is-located-message");
      setTimeout(() => article?.classList.remove("is-located-message"), 2200);
    } catch (error) { if (scopeMatches(scope) && version === state.locateVersion) report(error); }
    finally {
      if (scopeMatches(scope) && version === state.locateVersion) activateChatHistorySync();
    }
  }
  function openHistory() { openChatToolWindow("history"); void search(); }
  function describeRecipientEditor(targetIds = getSelectedAgentIds()) {
    const note = qs("unified-model-settings")?.querySelector('[data-ms="target-note"]');
    if (!note) return;
    const editor = state.modelSettings;
    const selected = sessionRegistry.sessions?.find(session => session.id === editor?.sessionId);
    const editorName = selected?.display_name || selected?.name || (editor?.sessionId ? "当前会话" : "新会话");
    const targetNames = targetIds.map(id => sessionRegistry.sessions?.find(session => session.id === id)?.display_name || id);
    if (!targetIds.length) note.textContent = `尚未选择发送对象；当前编辑：${editorName}。保存配置不会改变发送对象。`;
    else if (targetIds.length === 1) {
      note.textContent = editor?.dirty && editor?.sessionId !== targetIds[0]
        ? `当前发送给 ${targetNames[0]}；${editorName} 有未保存的草稿，已保留。请先保存或明确切换下方编辑会话。`
        : `当前发送给 ${targetNames[0]}；当前编辑：${editorName}。`;
    } else note.textContent = `当前发送给 ${targetNames.join("、")}；当前只编辑 ${editorName}，保存不会批量修改其他发送对象。可用上方列表选择编辑会话。`;
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
  function init() {
    if (state.initialized) return; state.initialized = true;
    mountExtraControls(); mountModelSettings(activeSessionId); syncModelSession(activeSessionId);
    document.querySelectorAll('[data-action="chat-history-open"]').forEach(node => node.addEventListener("click", openHistory));
    qs("chat-history-query")?.addEventListener("input", () => { clearTimeout(state.searchTimer); state.searchTimer = setTimeout(() => void search(), 180); });
    qs("chat-history-more")?.addEventListener("click", () => void search(true));
    qs("chat-return-latest")?.addEventListener("click", async () => {
      const scope = scopeSnapshot(); state.locateVersion++;
      try { await loadChatRoomMessages(scope.room); if (scopeMatches(scope)) qs("chat-return-latest").hidden = true; }
      catch (error) { if (scopeMatches(scope)) report(error); }
    });
    qs("chat-usage-refresh")?.addEventListener("click", () => void refreshInsights());
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
  return {init,popup,closePopup,intercept,streamEvent,finish,prepareDelta,refreshInsights,annotate,openHistory,search,locate,duration,syncModelSession,recipientTargetsChanged,roomChanged,
    get changingEnvironment() { return state.switching; }};
})();
