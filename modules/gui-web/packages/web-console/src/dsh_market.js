// DSH 目录只做远程发现；所有目录字符串作为文本显示，不执行来源里的安装命令。
(() => {
  const host = document.querySelector('[data-window-id="plugin-market"] .dsh-market');
  if (!host) return;

  const search = host.querySelector('[data-role="dsh-market-search"]');
  const category = host.querySelector('[data-role="dsh-market-category"]');
  const status = host.querySelector('[data-role="dsh-market-status"]');
  const list = host.querySelector('[data-role="dsh-market-list"]');
  const pageLabel = host.querySelector('[data-role="dsh-market-page"]');
  const prev = host.querySelector('[data-action="dsh-market-prev"]');
  const next = host.querySelector('[data-action="dsh-market-next"]');
  const searchButton = host.querySelector('[data-action="dsh-market-search"]');
  const state = { page: 1, pages: 1, serial: 0, loaded: false, loading: false,
    workspaceKey: null, abort: null, suppressOpen: false };

  function workspaceKey() {
    return typeof activeWorkspaceKey === "string" ? activeWorkspaceKey : "";
  }

  async function readJson(url, options = {}, timeoutMs = 20000) {
    const controller = new AbortController();
    const upstream = options.signal;
    const cancel = () => controller.abort();
    if (upstream?.aborted) cancel();
    else upstream?.addEventListener("abort", cancel, { once: true });
    const timeout = setTimeout(cancel, timeoutMs);
    try { return await requestJson(url, { ...options, signal: controller.signal }); }
    finally { clearTimeout(timeout); upstream?.removeEventListener("abort", cancel); }
  }

  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    node.textContent = text == null ? "" : String(text);
    return node;
  }

  function safeGitHubRepository(value, owner) {
    try {
      const url = new URL(value);
      const parts = url.pathname.split("/").filter(Boolean);
      if (url.protocol !== "https:" || url.hostname !== "github.com" || url.port ||
          url.search || url.hash || parts.length !== 2 || parts[0] !== owner ||
          !parts.every(part => /^[A-Za-z0-9._-]{1,100}$/.test(part))) return null;
      return url.href;
    } catch { return null; }
  }

  function showDetail(item, detail) {
    document.querySelector('[data-role="dsh-market-detail"]')?.dispatchEvent(new Event("dsh-market-close"));
    const overlay = element("div", "", "tool-detail-modal dsh-market-modal");
    overlay.dataset.role = "dsh-market-detail";
    overlay.dataset.workspaceKey = workspaceKey();
    const sourceAbort = new AbortController();
    const closeDetail = () => { sourceAbort.abort(); overlay.remove(); };
    overlay.addEventListener("dsh-market-close", closeDetail);
    const card = element("div", "", "tool-detail-card dsh-market-detail-card");
    const heading = element("header", "");
    heading.append(element("strong", item.name || item.id));
    const close = element("button", "关闭");
    setWuxiaIconOnly(close, "stop", "关闭插件详情");
    close.type = "button";
    close.addEventListener("click", closeDetail);
    heading.append(close);
    card.append(heading);
    card.append(element("p", `${item.owner || "作者未知"} · ${item.version || "版本未提供"} · ${(item.categories || []).join("、") || "未分类"}`));
    card.append(element("p", detail.description_zh || detail.description_en || item.description || "暂无说明。"));
    card.append(element("p", `仓库：${detail.repository || "未提供"}`));
    const repository = safeGitHubRepository(detail.repository, item.owner);
    if (repository) {
      const open = element("button", "在内置浏览器打开仓库");
      setWuxiaIconOnly(open, "browser", "在内置浏览器打开仓库");
      open.type = "button";
      open.addEventListener("click", async () => {
        open.disabled = true;
        try {
          closeDetail();
          openChatToolWindow("browser");
          const input = document.querySelector('[data-role="browser-window-input"]');
          if (!input) throw new Error("浏览器地址栏不可用");
          input.value = repository;
          await browserWindowNavigate();
        } catch (error) { status.textContent = `仓库打开失败：${error.message}`; }
        finally { open.disabled = false; }
      });
      card.append(open);
    }
    card.append(element("p", `npm 包：${detail.npm || "未提供"}`));
    card.append(element("p", "兼容状态：尚未核验。DSH 的 Node/Cordis 插件不能直接作为 COOLZHU 原生插件运行。", "dsh-market-compatibility"));
    const compatibility = card.querySelector(".dsh-market-compatibility");
    const check = element("button", "检查原生清单");
    setWuxiaIconOnly(check, "check", "检查原生清单兼容性");
    check.type = "button";
    check.addEventListener("click", async () => {
      const key = workspaceKey();
      if (overlay.dataset.workspaceKey !== key) {
        closeDetail();
        status.textContent = "工程已切换，请刷新目录后重新打开详情。";
        return;
      }
      check.disabled = true;
      compatibility.textContent = "正在检查仓库清单…";
      try {
        const result = await readJson("/api/extension-market/dsh/compatibility", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ expected_workspace: detail.workspace_id, id: item.id })
        }, 120000);
        if (key !== workspaceKey() || result.workspace_id !== detail.workspace_id) throw new Error("工程已切换，请重新打开详情");
        const label = result.status === "incompatible" ? "当前仓库不兼容" :
          result.status === "manifest_found_unverified" ? "发现原生清单，仍待完整校验" : "兼容性未知";
        compatibility.textContent = `${label}：${result.reason || "没有更多信息"}`;
      } catch (error) {
        compatibility.textContent = `检查失败：${error.message}`;
      } finally { check.disabled = false; }
    });
    card.append(check);
    const sourceCheck = element("button", "检查固定来源");
    setWuxiaIconOnly(sourceCheck, "check", "检查 DSH 固定来源");
    sourceCheck.type = "button";
    const sourceStatus = element("p", "", "dsh-market-source-status");
    let confirmedSource = null;
    const install = element("button", "安装为停用状态");
    setWuxiaIconOnly(install, "package-crate", "安装固定来源包（默认停用）");
    install.type = "button";
    install.disabled = true;
    install.addEventListener("click", async () => {
      const source = confirmedSource;
      const key = workspaceKey();
      if (!source || overlay.dataset.workspaceKey !== key) return;
      install.disabled = true;
      sourceCheck.disabled = true;
      sourceStatus.textContent = "正在核对刚才确认的固定来源并安装为停用状态…";
      try {
        // 提交后关闭页面不自动重发；安装结果由持久目录核对，不跟随详情面板abort。
        const result = await readJson("/api/extension-market/dsh/install", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ expected_workspace: source.workspace_id, id: item.id,
            commit: source.commit, expected_source_sha256: source.source_sha256 })
        }, 150000);
        confirmedSource = null;
        if (overlay.isConnected && key === workspaceKey()) {
          sourceStatus.textContent = result.installed === true
            ? `已安装 ${result.name} ${result.version}，本次安装默认停用。${result.cleanup_confirmed ? "" : "来源暂存清理未确认，请查看诊断。"}请刷新本地插件目录核对当前状态；模型接线尚待验收。`
            : "安装结果未确认，请刷新本地插件目录核对。";
        }
      } catch (error) {
        confirmedSource = null;
        if (overlay.isConnected && key === workspaceKey()) sourceStatus.textContent = `安装未正常完成：${error.message}。请刷新本地插件目录核对结果，不要自动重发。`;
      } finally { sourceCheck.disabled = false; }
    });
    sourceStatus.setAttribute("role", "status");
    sourceCheck.addEventListener("click", async () => {
      const key = workspaceKey();
      if (overlay.dataset.workspaceKey !== key) {
        closeDetail();
        status.textContent = "工程已切换，请刷新目录后重新打开详情。";
        return;
      }
      sourceCheck.disabled = true;
      confirmedSource = null;
      install.disabled = true;
      sourceStatus.textContent = "正在核验固定提交、包名与完整源码…";
      try {
        const result = await readJson("/api/extension-market/dsh/source", {
          method: "POST", headers: { "Content-Type": "application/json" },
          signal: sourceAbort.signal,
          body: JSON.stringify({ expected_workspace: detail.workspace_id, id: item.id })
        }, 125000);
        if (!overlay.isConnected || key !== workspaceKey() || result.workspace_id !== detail.workspace_id) return;
        if (result.status !== "source_verified") throw new Error("固定源码身份未确认");
        confirmedSource = result;
        install.disabled = result.static_installable !== true;
        sourceStatus.textContent = `${result.name} · ${result.version} · ${result.license} · 修订 ${result.commit} · ${result.source_file_count} 个文件。${result.reason}`;
      } catch (error) {
        if (overlay.isConnected && key === workspaceKey()) sourceStatus.textContent = `固定来源检查失败：${error.message}`;
      } finally { sourceCheck.disabled = false; }
    });
    card.append(sourceCheck, install, sourceStatus);
    overlay.append(card);
    overlay.addEventListener("click", event => { if (event.target === overlay) closeDetail(); });
    document.body.append(overlay);
  }

  function renderItem(item, response, key) {
    const card = element("article", "", "quick-catalog-item dsh-market-item");
    card.append(element("strong", item.name || "未命名插件"));
    const meta = [item.owner || "作者未知", item.version || "版本未提供",
      (item.categories || []).join("、") || "未分类", item.deprecated ? "目录标记已弃用" : "兼容性待核验"];
    card.append(element("small", meta.join(" · ")));
    card.append(element("p", item.description || "暂无说明。"));
    const detailButton = element("button", "查看详情与兼容性");
    setWuxiaIconOnly(detailButton, "file", "查看详情与兼容性");
    detailButton.type = "button";
    detailButton.addEventListener("click", async () => {
      detailButton.disabled = true;
      try {
        const detail = await readJson(`/api/extension-market/dsh/detail?id=${encodeURIComponent(item.id)}`, {}, 100000);
        if (key !== workspaceKey() || detail.workspace_id !== response.workspace_id) throw new Error("工程已切换，请刷新目录");
        showDetail(item, detail);
      } catch (error) { status.textContent = `详情读取失败：${error.message}`; }
      finally { detailButton.disabled = false; }
    });
    card.append(detailButton);
    return card;
  }

  function updateCategories(categories, selected) {
    category.replaceChildren(new Option("全部分类", ""));
    for (const entry of categories || []) {
      if (typeof entry.id !== "string" || typeof entry.name !== "string") continue;
      category.append(new Option(entry.name, entry.id));
    }
    category.value = selected;
  }

  async function load(force = false) {
    const serial = ++state.serial;
    const key = workspaceKey();
    state.workspaceKey = key;
    state.abort?.abort();
    const controller = new AbortController();
    state.abort = controller;
    state.loading = true;
    searchButton.disabled = true;
    prev.disabled = true;
    next.disabled = true;
    status.textContent = force ? "正在刷新远程目录…" : "正在读取远程目录…";
    const selected = category.value;
    const params = new URLSearchParams({ q: search.value.trim(), category: selected, page: String(state.page) });
    if (force) params.set("refresh", "true");
    try {
      // 后端完整目录下载最多 90 秒；前端先超时会使真实目录永远无法显示。
      const response = await readJson(`/api/extension-market/dsh?${params}`, { signal: controller.signal }, 100000);
      if (serial !== state.serial || key !== workspaceKey()) return;
      updateCategories(response.categories, selected);
      state.page = response.page || 1;
      state.pages = response.page_count || 1;
      state.loaded = true;
      list.replaceChildren(...(response.items || []).map(item => renderItem(item, response, key)));
      if (!response.items?.length) list.append(element("p", "当前筛选条件没有结果。"));
      const prefix = response.stale ? "远程刷新失败，显示上次读取的目录" : "目录已读取";
      status.textContent = `${prefix} · 源更新于 ${response.updated || "未知日期"} · 找到 ${response.total || 0} 项。${response.warning || ""}`;
      pageLabel.textContent = `第 ${state.page} / ${state.pages} 页`;
      prev.disabled = state.page <= 1;
      next.disabled = state.page >= state.pages;
    } catch (error) {
      if (serial !== state.serial || key !== workspaceKey()) return;
      status.textContent = state.loaded ? `远程目录读取失败；以下为上次显示的结果：${error.message}` : `远程目录读取失败：${error.message}`;
      if (!state.loaded) {
        list.replaceChildren();
        pageLabel.textContent = "—";
      }
    } finally {
      if (serial === state.serial) {
        state.loading = false;
        searchButton.disabled = false;
        prev.disabled = !state.loaded || state.page <= 1;
        next.disabled = !state.loaded || state.page >= state.pages;
        state.abort = null;
      }
    }
  }

  function openMarket() {
    if (state.suppressOpen || (state.loading && state.workspaceKey === workspaceKey())) return;
    if (!state.loaded || state.workspaceKey !== workspaceKey()) {
      state.page = 1;
      state.loaded = false;
      list.replaceChildren();
      load(false);
    }
  }

  searchButton.addEventListener("click", () => { state.page = 1; load(false); });
  search.addEventListener("keydown", event => {
    if (event.key === "Enter") { event.preventDefault(); state.page = 1; load(false); }
  });
  category.addEventListener("change", () => { state.page = 1; load(false); });
  prev.addEventListener("click", () => { if (state.page > 1) { state.page--; load(false); } });
  next.addEventListener("click", () => { if (state.page < state.pages) { state.page++; load(false); } });
  document.querySelectorAll('[data-window-target="plugin-market"]').forEach(button =>
    button.addEventListener("click", openMarket));
  document.querySelector('[data-action="plugin-market-refresh"]')?.addEventListener("click", () => {
    state.suppressOpen = true;
    setTimeout(() => { state.suppressOpen = false; }, 0);
    load(true);
  }, { capture: true });
  window.addEventListener("dsh-market-refresh", () => {
    const modal = document.querySelector('[data-role="dsh-market-detail"]');
    if (modal && modal.dataset.workspaceKey !== workspaceKey()) modal.dispatchEvent(new Event("dsh-market-close"));
    openMarket();
  });
  prev.disabled = true;
  next.disabled = true;
})();
