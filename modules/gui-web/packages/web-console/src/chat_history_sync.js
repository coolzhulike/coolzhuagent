/* 聊天历史失效协调器：通知只触发权威读取，不制造消息或重派模型。 */
window.CoolzhuChatHistorySync = (() => {
  const compareKey = (a, b) => {
    const time = Number(a.created_at) - Number(b.created_at);
    if (time) return time;
    const left = Array.from(String(a.id), c => c.codePointAt(0));
    const right = Array.from(String(b.id), c => c.codePointAt(0));
    for (let i = 0; i < Math.min(left.length, right.length); i++) {
      if (left[i] !== right[i]) return left[i] - right[i];
    }
    return left.length - right.length;
  };
  async function readWindow(fetchPage, boundary, signal) {
    let before = null, latest = null, oldest = null;
    const messages = new Map();
    const cursors = new Set();
    do {
      if (signal.aborted) throw new DOMException("已切换聊天室", "AbortError");
      const page = await fetchPage(before, boundary ? 200 : 80, signal);
      latest ||= page;
      oldest = page;
      for (const message of page.messages || []) messages.set(message.id, message);
      const first = page.messages?.[0];
      if (!boundary || !page.has_more || (first && compareKey(first, boundary) <= 0)) break;
      before = page.next_before;
      if (!before || cursors.has(before)) throw new Error("聊天分页游标未推进");
      cursors.add(before);
    } while (true);
    let rows = [...messages.values()].sort(compareKey);
    const all = rows;
    if (boundary) rows = rows.filter(row => compareKey(row, boundary) >= 0);
    // 原窗口唯一消息被删除时回到真实最新页，不保留已删除的空边界。
    if (!rows.length && all.length) rows = (latest.messages || []).slice(-80);
    const hasMore = Boolean(oldest.has_more || all.length > rows.length);
    return {room: latest.room, messages: rows, has_more: hasMore,
      next_before: hasMore ? rows[0]?.id || null : null};
  }
  function create({ sourceFactory, read, apply, busy, missing = () => {}, permissionChanged = () => {} }) {
    let epoch = 0, source = null, controller = null, scope = null;
    let dirty = false, running = false, timer = null, retries = 0;
    function schedule(delay = 120) {
      if (!scope || timer || running || busy()) return;
      timer = setTimeout(() => { timer = null; void flush(); }, delay);
    }
    function invalidate() { dirty = true; retries = 0; schedule(); }
    async function flush() {
      if (!scope || !dirty || running || busy()) return;
      const generation = epoch, captured = scope;
      const request = new AbortController(); controller = request;
      dirty = false; running = true;
      let failed = false;
      try {
        const result = await read(captured, request.signal);
        if (generation !== epoch || request.signal.aborted) return;
        if (busy()) { dirty = true; return; }
        apply(result, captured);
        retries = 0;
      } catch (error) {
        if (generation !== epoch || request.signal.aborted) return;
        dirty = true; failed = true; retries++;
      } finally {
        // 旧请求的 finally 不得覆盖新房间的在途状态（包括A→B→A）。
        if (generation === epoch) {
          running = false; controller = null;
          if (dirty && (!failed || retries <= 3)) schedule(failed ? 2000 * retries : 120);
        }
      }
    }
    function stop() {
      epoch++; source?.close(); source = null; controller?.abort(); controller = null;
      clearTimeout(timer); timer = null; dirty = false; running = false; scope = null; retries = 0;
    }
    function activate(nextScope) {
      stop(); scope = Object.freeze({...nextScope});
      const generation = epoch;
      source = sourceFactory(scope);
      for (const event of ["hello", "history-changed"]) source.addEventListener(event, () => {
        if (generation === epoch) invalidate();
      });
      // 权限影响顶栏展示，不等历史流空闲；旧房间通知不能更新当前房间。
      source.addEventListener("permission-changed", () => {
        if (generation === epoch) permissionChanged(scope);
      });
      source.addEventListener("room-deleted", () => {
        if (generation !== epoch) return;
        stop(); missing();
      });
      // EventSource负责断线重连；每次hello都会权威补齐，不重放业务事件。
    }
    return {activate, stop, invalidate, resume: invalidate};
  }
  return {create, readWindow, compareKey};
})();
