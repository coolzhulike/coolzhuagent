/* 仅用于受控验收页面；记录输入身份，不推断宿主/CDP来源，不记录输入正文。 */
(function (root) {
  function install(env) {
    const { window, document, navigator, performance, crypto } = env;
    const documentId = crypto.randomUUID();
    let sequence = 0;
    const state = { documentId, totalEvents: 0, events: [], overflow: 0, beaconRejected: 0 };
    const target = node => node ? { tag: node.tagName || null, id: node.id || null } : null;
    function record(event) {
      state.totalEvents = ++sequence;
      const item = {
        schema: 2, document_id: documentId, sequence,
        kind: event.type, role: document.body?.dataset.role || null,
        time_origin_ms: performance.timeOrigin,
        page_ms: performance.timeOrigin + performance.now(),
        event_timestamp_ms: event.timeStamp ?? null,
        trusted: event.isTrusted === true,
        target: target(event.target), active_element: target(document.activeElement),
        focused: document.hasFocus(), visibility: document.visibilityState,
        key: event.key ?? null, code: event.code ?? null,
        location: event.location ?? null, repeat: event.repeat ?? null,
        ctrl: event.ctrlKey ?? null, alt: event.altKey ?? null,
        shift: event.shiftKey ?? null, meta: event.metaKey ?? null,
        button: event.button ?? null, buttons: event.buttons ?? null,
        pointer_id: event.pointerId ?? null, pointer_type: event.pointerType ?? null,
        client_x: event.clientX ?? null, client_y: event.clientY ?? null,
        input_type: event.inputType ?? null, composing: event.isComposing ?? null,
        delta_x: event.deltaX ?? null, delta_y: event.deltaY ?? null,
        source: "unknown",
      };
      if (state.events.length < 10000) state.events.push(item);
      else state.overflow++;
      if (!navigator.sendBeacon('/event', new env.Blob([JSON.stringify(item)], { type: 'application/json' }))) {
        state.beaconRejected++;
      }
    }
    for (const kind of ['pointerdown', 'pointerup', 'pointercancel', 'mousedown', 'mouseup',
      'click', 'dblclick', 'keydown', 'keyup', 'beforeinput', 'input', 'wheel',
      'focusin', 'focusout', 'pagehide', 'load', 'visibilitychange']) {
      window.addEventListener(kind, record, true);
    }
    window.__inputAttribution = state;
    record({ type: 'document-script-start', isTrusted: false });
    return state;
  }
  if (typeof module !== 'undefined' && module.exports) module.exports = { install };
  else install(root);
})(typeof window !== 'undefined' ? window : globalThis);
