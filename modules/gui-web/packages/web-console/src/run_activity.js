/* 运行过程只占聊天底部一至五行；持久轨迹由后端历史提供。 */
window.CoolzhuRunActivity = (() => {
  let running = false, thinking = "", tools = [], following = true, bound = false;
  const host = () => document.querySelector('[data-role="chat-run-activity"]');
  function render() {
    const panel = host(), text = document.querySelector('[data-role="chat-activity-stream"]'), toggle = document.querySelector('[data-role="chat-activity-follow"]');
    if (!panel || !text) return;
    if (!bound) {
      bound = true;
      text.addEventListener("wheel", event => { if (event.deltaY < 0) { following = false; render(); } }, {passive:true});
      text.addEventListener("keydown", event => { if (["PageUp","ArrowUp","Home"].includes(event.key)) { following = false; render(); } });
      toggle?.addEventListener("click", () => { following = !following; render(); });
      document.addEventListener("selectionchange", () => { if (running && !window.getSelection()?.toString()) render(); });
    }
    panel.hidden = !running;
    const value = [thinking, ...tools].filter(Boolean).join("\n").slice(-6000);
    const selection = window.getSelection();
    const selecting = selection && !selection.isCollapsed && text.contains(selection.anchorNode);
    // 选择内容时保留当前DOM和滚动位置，松开选择后再显示最新的有界缓冲。
    if (!running || !selecting) {
      if (text.textContent !== value) text.textContent = value;
      text.hidden = !value;
      if (following && !selecting) text.scrollTop = text.scrollHeight;
    }
    if (toggle) {
      toggle.hidden = !value;
      const label = following ? "暂停跟随" : "跟随最新";
      toggle.title = label;
      toggle.setAttribute("aria-label", label);
      toggle.setAttribute("aria-pressed", String(!following));
    }
  }
  return {
    start() { running = true; thinking = ""; tools = []; following = true; render(); },
    thinking(value) { thinking = String(value || "").slice(-6000); render(); },
    tools(rows) { tools = rows.slice(-4).map(value => String(value).slice(-1000)); render(); },
    finish() { running = false; thinking = ""; tools = []; render(); },
  };
})();
