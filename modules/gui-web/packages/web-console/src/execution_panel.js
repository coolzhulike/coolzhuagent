/* 右栏执行页。只消费共享执行事实，不切换聊天室、不修改定时配置。 */
(() => {
  const labels = { running: "执行中", cancel_requested: "正在请求停止", completed: "已完成", failed: "失败", interrupted: "已停止等待", outcome_unknown: "结果待核查" };
  const node = (tag, text, cls) => {
    const el = document.createElement(tag);
    if (text != null) el.textContent = String(text);
    if (cls) el.className = cls;
    return el;
  };
  function mount(root, options = {}) {
    if (!root) return null;
    root.__executionPanel?.dispose();
    let timer, busy = false, generation = 0, selected = null, disposed = false, scopePromise = null, detailGeneration = 0;
    const controllers = new Set();
    const snapshot = () => ({ generation, project: options.getScope?.() });
    const current = captured => !disposed && captured.generation === generation
      && captured.project?.workspace === options.getScope?.()?.workspace
      && captured.project?.generation === options.getScope?.()?.generation;
    const obsolete = () => new DOMException("执行页工程或请求已改变", "AbortError");
    async function transport(url, captured, init = {}) {
      if (!current(captured)) throw obsolete();
      const controller = new AbortController(); controllers.add(controller);
      try {
        const response = await fetch(url, { ...init, signal: controller.signal });
        const data = await response.json();
        if (!current(captured)) throw obsolete();
        if (!response.ok) throw new Error(data.error?.message || data.error || data.message || `请求失败 (${response.status})`);
        return data;
      } finally { controllers.delete(controller); }
    }
    async function request(url, captured, init = {}) {
      if (!captured.project?.workspace) throw new Error("执行页尚未绑定工程，请等待工程加载");
      if (!scopePromise) {
        const pendingScope = transport("/api/workspace", captured).then(value => {
          if (value.workspace !== captured.project.workspace || !value.workspace_id || !value.configuration_scope)
            throw new Error("执行页工程已改变，请刷新后重新打开");
          return { workspace: value.workspace_id, configuration: value.configuration_scope };
        });
        scopePromise = pendingScope;
        // 只释放本次失败查询；旧工程迟到失败不能清掉新工程缓存。不会重放任何POST。
        void pendingScope.catch(() => { if (scopePromise === pendingScope) scopePromise = null; });
      }
      const scope = await scopePromise;
      if (!current(captured)) throw obsolete();
      return transport(url, captured, { ...init, headers: { "Content-Type": "application/json",
        "X-Coolzhu-Workspace-Id": scope.workspace, "X-Coolzhu-Configuration-Scope": scope.configuration } });
    }
    const header = node("header", null, "execution-heading");
    header.append(node("strong", "执行卷轴"), node("small", "子 Agent · 定时执行"));
    const refresh = node("button", "刷新"); refresh.type = "button"; header.append(refresh);
    const filter = node("select"); filter.setAttribute("aria-label", "执行来源");
    [["", "全部来源"], ["session_tool", "会话工具 / 子 Agent"], ["scheduled", "定时执行"]].forEach(([value, label]) => {
      const option = node("option", label); option.value = value; filter.append(option);
    });
    const status = node("p", "", "execution-notice"); status.setAttribute("role", "status");
    const list = node("div", null, "execution-list");
    const details = node("section", null, "execution-details"); details.hidden = true;
    root.replaceChildren(header, filter, status, list, details);
    const field = (name, value) => { const p = node("p"); p.append(node("b", `${name}：`), node("span", value ?? "未提供")); return p; };
    function detail(item) {
      const captured = snapshot(), detailVersion = ++detailGeneration;
      const detailCurrent = () => current(captured) && detailVersion === detailGeneration && selected === item.spec.id;
      selected = item.spec.id; details.hidden = false;
      const close = node("button", "收起详情"); close.type = "button";
      close.onclick = () => { detailGeneration++; selected = null; details.hidden = true; details.replaceChildren(); };
      details.replaceChildren(close, node("h4", "执行详情"), field("执行 ID", item.spec.id),
        field("父执行", item.spec.parent_execution_id || item.spec.parent_run_id || "根执行"),
        field("房间", item.spec.room_id), field("来源", item.spec.source_id),
        field("截止", new Date(item.spec.deadline_ms).toLocaleString()),
        node("pre", item.result || item.error || "尚无终态结果"));
      const trace = node("button", "查看运行轨迹"); trace.type = "button";
      const run = item.run_id || item.spec.parent_run_id;
      trace.disabled = !run;
      trace.onclick = async () => {
        trace.disabled = true;
        try {
          // 服务端依据本执行冻结的真实run直接寻址，较旧运行无需扫描整个房间。
          const data = await request(`/api/executions/${encodeURIComponent(item.spec.id)}/trace`, captured);
          if (!detailCurrent()) return;
          const found = data.runs?.find(r => r.run_id === run);
          if (!found) throw new Error("本执行的真实运行轨迹尚不可用");
          if (!detailCurrent()) return;
          const pre = node("pre", JSON.stringify(found, null, 2));
          pre.setAttribute("aria-label", "真实运行轨迹"); details.append(pre);
        } catch (error) { if (detailCurrent() && error.name !== "AbortError") status.textContent = error.message; }
        finally { if (detailCurrent()) trace.disabled = false; }
      };
      details.append(trace, node("h4", "模型请求与真实用量"), node("pre", JSON.stringify(item.usage_attempts, null, 2)));
    }
    async function refreshList() {
      if (busy || root.hidden || document.hidden) return;
      busy = true; const captured = snapshot();
      try {
        const data = await request("/api/executions", captured);
        if (!current(captured)) return;
        status.textContent = ""; list.replaceChildren();
        const items = data.executions.filter(item => !filter.value || item.spec.source === filter.value);
        if (!items.length) list.append(node("p", "当前工程暂无此类执行。已结束任务不会自动重放。", "execution-empty"));
        items.forEach(item => {
          const card = node("article", null, "execution-card"); card.dataset.state = item.status;
          const head = node("header");
          // 品牌按本次执行的真实模型解析；只用会话ID取自定义头像，不能用当前活动模型替代历史执行。
          const avatarUrl = options.getAvatar?.(item.spec)
            || window.CoolzhuBrandAvatars?.url(window.CoolzhuBrandAvatars.resolve(item.spec));
          if (avatarUrl) {
            const avatar = node("img", null, "execution-avatar brand-avatar");
            avatar.src = avatarUrl; avatar.alt = ""; avatar.setAttribute("aria-hidden", "true");
            head.append(avatar);
          }
          head.append(node("span", item.spec.source === "scheduled" ? "定时" : "子 Agent", "execution-source"), node("b", labels[item.status] || item.status));
          card.append(head, node("h4", item.spec.summary || "执行任务"),
            field("进展", item.status === "running"
              ? (item.usage_attempt_count ? `已记录 ${item.usage_attempt_count} 次请求；最新状态 ${item.usage_attempts.at(-1).status}` : "已接纳，尚无模型请求回执")
              : labels[item.status] || item.status),
            field("模型 / 会话", `${item.spec.provider} · ${item.spec.model} / ${item.spec.session_id}`),
            field("父任务", item.spec.parent_execution_id || item.spec.parent_run_id || "根执行"),
            field("耗时", item.elapsed_ms == null ? "终止时刻未知" : `${Math.floor(item.elapsed_ms / 1000)} 秒`),
            field("真实 token", `输入 ${item.input_tokens ?? "未知"} · 输出 ${item.output_tokens ?? "未知"}`));
          const preview = item.result || item.error;
          if (preview) card.append(node("p", preview.length > 180 ? `${preview.slice(0, 180)}…` : preview, "execution-result"));
          const actions = node("footer"); const show = node("button", "详情与轨迹"); show.type = "button";
          show.onclick = async () => {
            if (!current(captured)) return;
            const version = ++detailGeneration;
            show.disabled = true;
            try { const value = await request(`/api/executions/${encodeURIComponent(item.spec.id)}`, captured); if (current(captured) && version === detailGeneration) detail(value); }
            catch (error) { if (current(captured) && version === detailGeneration && error.name !== "AbortError") status.textContent = error.message; }
            finally { if (current(captured) && version === detailGeneration) show.disabled = false; }
          };
          actions.append(show);
          if (item.can_cancel) {
            const cancel = node("button", "停止执行"); cancel.type = "button";
            cancel.disabled = item.status === "cancel_requested";
            cancel.onclick = async () => {
              if (!current(captured)) return;
              cancel.disabled = true;
              try { await request(`/api/executions/${encodeURIComponent(item.spec.id)}/cancel`, captured, { method: "POST", body: "{}" }); if (current(captured)) status.textContent = "已向实际执行器请求停止，等待终态回执。"; }
              catch (error) { if (current(captured) && error.name !== "AbortError") { status.textContent = error.message; cancel.disabled = false; } }
            };
            actions.append(cancel);
          }
          card.append(actions); list.append(card);
        });
      } catch (error) { if (current(captured) && error.name !== "AbortError") status.textContent = error.message; }
      finally { if (current(captured)) busy = false; }
    }
    function invalidate() {
      generation++; detailGeneration++; controllers.forEach(controller => controller.abort()); controllers.clear();
      scopePromise = null; busy = false; selected = null; details.hidden = true;
      details.replaceChildren(); list.replaceChildren(); status.textContent = "";
    }
    const workspaceChanged = () => { invalidate(); queueMicrotask(refreshList); };
    window.addEventListener("coolzhu-workspace-changed", workspaceChanged);
    window.addEventListener("coolzhu-project-scope-changed", workspaceChanged);
    refresh.onclick = refreshList; filter.onchange = refreshList;
    const observer = new MutationObserver(() => { if (!root.hidden) refreshList(); });
    observer.observe(root, { attributes: true, attributeFilter: ["hidden"] });
    timer = setInterval(refreshList, 3000); refreshList();
    const api = { refresh: refreshList, dispose() { invalidate(); disposed = true; clearInterval(timer); observer.disconnect(); window.removeEventListener("coolzhu-workspace-changed", workspaceChanged); window.removeEventListener("coolzhu-project-scope-changed", workspaceChanged); root.replaceChildren(); } };
    root.__executionPanel = api;
    return api;
  }
  window.CoolzhuExecutionPanel = { mount };
})();
