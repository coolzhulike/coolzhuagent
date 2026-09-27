/* 内容预览只接收显式点击的引用；本地脚本仅在显式选择的隔离预览运行，不获得桌面能力。 */
window.CoolzhuContentPreview = (() => {
  let sequence = 0;
  let controller = null;
  let host = null;
  const textExtensions = /\.(txt|md|markdown|log|json|jsonl|csv|tsv|rs|js|mjs|ts|tsx|jsx|css|html?|xml|yaml|yml|toml|py|sql|sh|ps1|bat|ini|conf|c|h|cpp|hpp|java|vue|svelte)$/i;
  const node = (tag, text) => { const element = document.createElement(tag); if (text != null) element.textContent = text; return element; };
  function classify(locator, mime = "") {
    let path = String(locator).split(/[?#]/)[0];
    if (mime.startsWith("image/") || /\.(png|jpe?g|gif|webp|bmp|svg)$/i.test(path)) return "image";
    if (mime.startsWith("video/") || /\.(mp4|webm|mov|m4v|ogv)$/i.test(path)) return "video";
    if (mime.startsWith("audio/") || /\.(mp3|wav|ogg|oga|m4a|flac|aac|opus)$/i.test(path)) return "audio";
    if (mime.startsWith("text/") || textExtensions.test(path)) return "text";
    if (/\.pdf$/i.test(path)) return "pdf";
    return "web";
  }
  function reference(locator, label, mime) {
    let value = String(locator || "").trim().replace(/^<|>$/g, "");
    if (!value || value.startsWith("#") || /[\u0000-\u001f]/.test(value)) return null;
    let file = /^[a-z]:[\\/]/i.test(value) || !/^[a-z][a-z\d+.-]*:/i.test(value) && !value.startsWith("//");
    if (/^file:/i.test(value)) {
      try { const parsed = new URL(value); if (parsed.hostname) return null; value = decodeURIComponent(parsed.pathname).replace(/^\/([a-z]:)/i, "$1"); file = true; } catch { return null; }
    }
    if (/^\/api\/attachments\/files\//.test(value)) file = false;
    if (!file) {
      try {
        const parsed = new URL(value, location.href);
        if (!["http:", "https:", "blob:"].includes(parsed.protocol)) return null;
        if (parsed.username || parsed.password) return null;
        // blob只接受当前页面创建的同源对象引用。
        if (parsed.protocol === "blob:" && parsed.origin !== location.origin) return null;
        value = parsed.href;
      } catch { return null; }
    }
    const lineMatch = file && value.match(/:(\d+)$/);
    if (lineMatch) value = value.slice(0, -lineMatch[0].length);
    const isolatedDocument = file && /\.(html?|svg)$/i.test(value);
    const kind = isolatedDocument ? "text" : !file && /^https?:/i.test(value) && /\.html?(?:[?#]|$)/i.test(value) ? "web" : classify(value, mime);
    return { locator: value, label: label || value.split(/[\\/]/).at(-1), kind, file, isolatedDocument, line: lineMatch ? Number(lineMatch[1]) : null };
  }
  async function readLimitedText(response) {
    if (!response.ok) throw new Error(`文本加载失败（${response.status}）`);
    const limit = 2 * 1024 * 1024;
    if (Number(response.headers.get("Content-Length")) > limit) throw new Error("文本超过 2 MB，暂不在侧栏完整载入。");
    const reader = response.body.getReader(); const chunks = []; let size = 0;
    try {
      for (;;) {
        const chunk = await reader.read(); if (chunk.done) break;
        size += chunk.value.length;
        if (size > limit) { await reader.cancel(); throw new Error("文本超过 2 MB，暂不在侧栏完整载入。"); }
        chunks.push(chunk.value);
      }
    } finally { reader.releaseLock(); }
    const bytes = new Uint8Array(size); let offset = 0;
    chunks.forEach(chunk => { bytes.set(chunk, offset); offset += chunk.length; });
    return new TextDecoder("utf-8").decode(bytes);
  }
  function isolatedSource(source) {
    const policy = "default-src 'none'; img-src data: blob:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'none'; form-action 'none'; base-uri 'none'; object-src 'none'; frame-src 'none'";
    // 只构造字符串：可信 CSP 在浏览器解析任何文件内容之前生效，避免 DOMParser 的资源请求。
    // 去除声明式跳转和 base；额外的文件内 CSP 保留，只能进一步收紧策略。
    const content = source.replace(/<(?:meta|base)\b(?:[^"'<>]|"[^"]*"|'[^']*')*>/gi, tag => {
      if (/^<base\b/i.test(tag)) return "";
      return /\bhttp-equiv\s*=\s*(?:"\s*refresh\s*"|'\s*refresh\s*'|refresh(?=[\s/>]))/i.test(tag) ? "" : tag;
    });
    return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${policy}"></head><body>${content}</body></html>`;
  }

  function renderText(content, text, options = {}) {
    const toolbar = node("div"); toolbar.className = "content-preview-text-tools";
    const search = node("input"); search.type = "search"; search.placeholder = "查找当前页"; search.setAttribute("aria-label", "查找预览当前页");
    const previous = node("button", "上个"); previous.type = "button";
    const next = node("button", "下个"); next.type = "button";
    const copy = node("button", "复制本页"); copy.type = "button";
    const status = node("small", ""); status.setAttribute("role", "status");
    const pre = node("pre"); pre.className = "content-preview-text has-line-numbers"; pre.tabIndex = 0; pre.setAttribute("aria-label", "文件内容与行号");
    const allLines = String(text || "").split("\n"); const lines = allLines.slice(0,2000); const first = options.startLine || 1;
    const rows = lines.map((line,index) => {
      const row = node("span"); row.className = "content-preview-line"; row.dataset.line = String(first + index);
      const number = node("span", String(first + index)); number.className = "content-preview-line-number"; number.setAttribute("aria-hidden", "true");
      const value = node("span", line || "\u200b"); value.className = "content-preview-line-value"; row.append(number,value); pre.append(row); return row;
    });
    let matches = [], selected = -1;
    const locate = direction => {
      if (!matches.length) return;
      selected = (selected + direction + matches.length) % matches.length;
      rows.forEach(row => row.classList.remove("is-current-match"));
      matches[selected].classList.add("is-current-match"); matches[selected].scrollIntoView({block:"nearest"});
      status.textContent = `${selected + 1} / ${matches.length} 个匹配行`;
    };
    search.addEventListener("input", () => {
      const query = search.value.toLocaleLowerCase(); selected = -1;
      matches = rows.filter((row,index) => { const match = Boolean(query) && lines[index].toLocaleLowerCase().includes(query); row.classList.toggle("is-match", match); row.classList.remove("is-current-match"); return match; });
      previous.disabled = next.disabled = !matches.length; status.textContent = query ? `${matches.length} 个匹配行` : "";
      if (matches.length) locate(1);
    });
    search.addEventListener("keydown", event => { if (event.key === "Enter") { event.preventDefault(); locate(event.shiftKey ? -1 : 1); } });
    previous.addEventListener("click", () => locate(-1)); next.addEventListener("click", () => locate(1)); previous.disabled = next.disabled = true;
    copy.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(String(text || "")); status.textContent = "已复制本页内容"; }
      catch (_) { status.textContent = "剪贴板不可用，可在内容区选中文字后复制。"; }
    });
    toolbar.append(search, previous, next, copy, status);
    if (options.onLine) {
      const jump = node("input"); jump.type = "number"; jump.min = "1"; jump.max = String(options.totalLines || 1); jump.value = String(options.targetLine || first); jump.setAttribute("aria-label", "跳转到文件行号");
      const go = node("button", "跳转行"); go.type = "button";
      const action = () => { const line = Number(jump.value); if (Number.isSafeInteger(line) && line >= 1 && line <= Number(jump.max)) options.onLine(line); };
      go.addEventListener("click", action); jump.addEventListener("keydown", event => { if (event.key === "Enter") { event.preventDefault(); action(); } }); toolbar.append(jump, go);
    }
    content.replaceChildren(toolbar, pre);
    if (allLines.length > lines.length) content.append(node("small", `当前显示前 ${lines.length} 行；复制本页可获取已载入的完整内容。`));
    const target = rows.find(row => Number(row.dataset.line) === options.targetLine);
    if (target) { target.classList.add("is-target-line"); requestAnimationFrame(() => { if (target.isConnected) target.scrollIntoView({block:"nearest"}); }); }
  }
  function attachImageControls(heading, content, image) {
    const toolbar = node("div"); toolbar.className = "content-preview-image-tools";
    const zoomOut = node("button", "−"); const zoomIn = node("button", "+"); const fit = node("button", "适应"); const actual = node("button", "原尺寸");
    const status = node("small", "适应窗口"); status.setAttribute("role", "status"); let scale = null;
    const apply = value => {
      scale = value; content.classList.toggle("is-image-zoomed", scale !== null);
      image.style.width = scale === null ? "" : `${image.naturalWidth * scale}px`; image.style.height = "auto";
      image.style.maxWidth = scale === null ? "" : "none"; image.style.maxHeight = scale === null ? "" : "none";
      status.textContent = scale === null ? "适应窗口" : `${Math.round(scale * 100)}%`;
    };
    const zoom = factor => { if (!image.naturalWidth) return; const current = scale ?? image.getBoundingClientRect().width / image.naturalWidth; apply(Math.min(8, Math.max(.1,current * factor))); };
    zoomOut.setAttribute("aria-label", "缩小图片"); zoomIn.setAttribute("aria-label", "放大图片");
    for (const button of [zoomOut,zoomIn,fit,actual]) button.type = "button";
    zoomOut.addEventListener("click", () => zoom(1/1.25)); zoomIn.addEventListener("click", () => zoom(1.25)); fit.addEventListener("click", () => apply(null)); actual.addEventListener("click", () => { if (image.naturalWidth) apply(1); });
    toolbar.append(zoomOut, zoomIn, fit, actual, status); heading.append(toolbar);
  }
  function attachDocumentPreview(heading, content, ref, current, signal) {
    const toolbar = node("div"); toolbar.className = "content-preview-switch";
    const code = node("button", "源码"); code.type = "button"; code.setAttribute("aria-pressed", "true");
    const preview = node("button", "预览"); preview.type = "button"; preview.setAttribute("aria-pressed", "false");
    const notice = node("small", "隔离预览，脚本仅在预览内运行");
    toolbar.append(code, preview, notice); heading.append(toolbar);
    const sourceNodes = Array.from(content.childNodes); let completeSource = null; let modeVersion = 0;
    const setMode = isPreview => { code.setAttribute("aria-pressed", String(!isPreview)); preview.setAttribute("aria-pressed", String(isPreview)); };
    code.addEventListener("click", () => { modeVersion++; content.replaceChildren(...sourceNodes); setMode(false); preview.disabled = false; });
    preview.addEventListener("click", async () => {
      const version = ++modeVersion; preview.disabled = true;
      try {
        if (completeSource === null) completeSource = await readLimitedText(await fetch(`/api/project/file/content?path=${encodeURIComponent(ref.locator)}`, {signal}));
        if (!current() || version !== modeVersion) return;
        const frame = node("iframe"); frame.title = `${ref.label} · 隔离预览`; frame.className = "content-preview-document is-interactive";
        frame.setAttribute("sandbox", "allow-scripts"); frame.referrerPolicy = "no-referrer";
        frame.setAttribute("allow", "camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'");
        frame.srcdoc = isolatedSource(completeSource); content.replaceChildren(frame); setMode(true);
      } catch (error) {
        if (error.name !== "AbortError" && current() && version === modeVersion) {
          const alert = node("p", error.message); alert.setAttribute("role", "alert"); content.replaceChildren(...sourceNodes, alert); setMode(false);
        }
      } finally { if (current() && version === modeVersion) preview.disabled = false; }
    });
  }
  function dispose() {
    sequence++; controller?.abort(); controller = null;
    host?.querySelectorAll("video,audio").forEach(media => { media.pause(); media.removeAttribute("src"); media.load(); });
    host?.querySelectorAll("iframe").forEach(frame => { frame.src = "about:blank"; });
    host?.replaceChildren();
  }
  async function open(container, ref, options) {
    dispose(); host = container; controller = new AbortController();
    const signal = controller.signal; const version = sequence;
    const current = () => version === sequence && options.isCurrent();
    host.setAttribute("aria-label", `内容预览：${ref.label}`);
    const heading = node("div"); heading.className = "content-preview-meta";
    heading.append(node("strong", ref.label), node("small", ref.locator));
    const content = node("div"); content.className = "content-preview-body";
    host.replaceChildren(heading, content);
    content.append(node("p", "正在载入…"));
    try {
      if (ref.file && !["image", "video", "audio", "pdf"].includes(ref.kind)) {
        if (!["text", "web"].includes(ref.kind)) throw new Error("工作区媒体需要先作为聊天附件上传，或使用已授权的附件引用。");
        const query = new URLSearchParams({path:ref.locator});
        query.set("start_line", String(Math.max(1, (ref.line || 1) - 20)));
        query.set("end_line", String((ref.line || 1) + 399));
        const response = await fetch(`/api/project/file?${query}`, {signal});
        const data = await response.json();
        if (!response.ok) throw new Error(data.error || "文件无法预览，请检查工作区与文件路径。");
        if (!current()) return;
        const changed = ref.revision && data.meta?.revision && ref.revision !== data.meta.revision;
        options.onResolved?.(data.meta); options.onLocation?.(ref.line || data.start_line || 1);
        if (changed) heading.append(node("small", "文件已更新，当前显示磁盘上的最新内容。"));
        renderText(content, data.content, {startLine:data.start_line || 1,targetLine:ref.line,totalLines:data.total_lines,
          onLine: line => void open(container, {...ref,line}, options)});
        heading.append(node("small", `第 ${data.start_line || 1}–${data.end_line || 1} 行 / 共 ${data.total_lines || "未知"} 行 · 只读`));
        if (data.end_line < data.total_lines) {
          const next = node("button", "继续读取后续内容"); next.type = "button";
          next.addEventListener("click", () => void open(container, {...ref,line:data.end_line + 21}, options));
          content.append(next);
        }
        if (ref.isolatedDocument) attachDocumentPreview(heading, content, ref, current, signal);
      } else if (["image", "video", "audio"].includes(ref.kind)) {
        const media = node(ref.kind === "image" ? "img" : ref.kind);
        media.className = `content-preview-media is-${ref.kind}`;
        if (ref.kind === "image") { media.alt = ref.label; media.loading = "eager"; }
        else { media.controls = true; media.preload = "metadata"; if (ref.kind === "video") media.playsInline = true; }
        media.addEventListener("error", () => { if (current()) content.replaceChildren(node("p", "媒体无法播放或载入，请检查格式、链接或服务状态。")); });
        media.addEventListener(ref.kind === "image" ? "load" : "loadedmetadata", () => { if (current()) options.onResolved?.(); }, {once:true});
        media.src = ref.file ? `/api/project/file/content?path=${encodeURIComponent(ref.locator)}` : ref.locator;
        content.replaceChildren(media);
        if (ref.kind === "image") attachImageControls(heading, content, media);
      } else if (ref.file && ref.kind === "pdf") {
        const frame = node("iframe"); frame.title = ref.label; frame.className = "content-preview-document";
        frame.setAttribute("sandbox", "");
        frame.src = `/api/project/file/content?path=${encodeURIComponent(ref.locator)}`;
        content.replaceChildren(frame);
      } else if (ref.kind === "text") {
        const response = await fetch(ref.locator, {signal, credentials:"same-origin"});
        const text = await readLimitedText(response);
        if (!current()) return;
        options.onResolved?.(); renderText(content, text);
      } else {
        // 浏览器宿主统一处理网页；iframe受站点限制时只做真实降级提示。
        await options.openBrowser(ref.locator, ref.label);
      }
    } catch (error) {
      if (error.name !== "AbortError" && current()) { const notice = node("p", error.message); notice.setAttribute("role", "alert"); content.replaceChildren(notice); }
    }
  }
  return {reference, classify, open, dispose};
})();
