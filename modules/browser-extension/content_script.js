"use strict";

if (!globalThis.__coolzhuBrowserBridgeInstalled) {
  globalThis.__coolzhuBrowserBridgeInstalled = true;
  const documentId = globalThis.crypto?.randomUUID?.() || `doc-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  let revision = 1;
  let targetMap = new Map();
  const observer = new MutationObserver(() => { revision += 1; });
  observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });

  const visible = (element) => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== "hidden" && style.display !== "none" && rect.width > 0 && rect.height > 0;
  };

  const labelFor = (element) => {
    const labelledBy = element.getAttribute("aria-labelledby");
    if (labelledBy) {
      return labelledBy.split(/\s+/).map((id) => document.getElementById(id)?.innerText || "").join(" ").trim();
    }
    if (element.id) {
      const label = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      if (label) return label.innerText.trim();
    }
    return element.closest("label")?.innerText?.trim() || "";
  };

  const nodeName = (element) => String(
    element.getAttribute("aria-label")
      || labelFor(element)
      || element.getAttribute("alt")
      || element.getAttribute("title")
      || element.getAttribute("placeholder")
      || element.innerText
      || "",
  ).trim().slice(0, 300);

  const candidates = () => Array.from(document.querySelectorAll(
    "a[href],button,input,textarea,select,option,[role],[contenteditable=true],summary,form,h1,h2,h3,[draggable='true'],[data-drop-target],[dropzone],[ondrop]",
  )).filter(visible).slice(0, 500);

  const centerOf = (element) => {
    const rect = element.getBoundingClientRect();
    return {
      clientX: Math.round(rect.left + rect.width / 2),
      clientY: Math.round(rect.top + rect.height / 2),
    };
  };

  function dispatchPointerMouse(element, type, point) {
    const EventCtor = type.startsWith("pointer") && typeof PointerEvent === "function" ? PointerEvent : MouseEvent;
    element.dispatchEvent(new EventCtor(type, {
      bubbles: true,
      cancelable: true,
      view: window,
      clientX: point.clientX,
      clientY: point.clientY,
      pointerId: 1,
      pointerType: "mouse",
      buttons: type.endsWith("up") ? 0 : 1,
    }));
  }

  function dispatchDragEvent(element, type, point, dataTransfer) {
    let event;
    try {
      event = new DragEvent(type, {
        bubbles: true,
        cancelable: true,
        clientX: point.clientX,
        clientY: point.clientY,
        dataTransfer,
      });
    } catch (_error) {
      event = new Event(type, { bubbles: true, cancelable: true });
      try {
        Object.defineProperty(event, "dataTransfer", { value: dataTransfer });
      } catch (_ignored) {}
    }
    element.dispatchEvent(event);
  }

  function performDrag(source, dropTarget) {
    if (!dropTarget || !dropTarget.isConnected) throw new Error("drop_target_not_found");
    if (!visible(dropTarget)) throw new Error("drop_target_unavailable");
    const sourcePoint = centerOf(source);
    const dropPoint = centerOf(dropTarget);
    const dataTransfer = typeof DataTransfer === "function" ? new DataTransfer() : {
      data: {},
      setData(type, value) { this.data[type] = String(value); },
      getData(type) { return this.data[type] || ""; },
      effectAllowed: "all",
      dropEffect: "move",
    };
    try { dataTransfer.setData("text/plain", source.id || source.getAttribute("aria-label") || source.textContent || ""); } catch (_error) {}
    dispatchPointerMouse(source, "pointerdown", sourcePoint);
    dispatchPointerMouse(source, "mousedown", sourcePoint);
    dispatchDragEvent(source, "dragstart", sourcePoint, dataTransfer);
    dispatchDragEvent(source, "drag", sourcePoint, dataTransfer);
    dispatchDragEvent(dropTarget, "dragenter", dropPoint, dataTransfer);
    dispatchDragEvent(dropTarget, "dragover", dropPoint, dataTransfer);
    dispatchPointerMouse(dropTarget, "mousemove", dropPoint);
    dispatchDragEvent(dropTarget, "drop", dropPoint, dataTransfer);
    dispatchDragEvent(source, "dragend", dropPoint, dataTransfer);
    dispatchPointerMouse(dropTarget, "mouseup", dropPoint);
    dispatchPointerMouse(dropTarget, "pointerup", dropPoint);
  }

  function setSliderValue(target, percent) {
    const bounded = Math.max(0, Math.min(100, Number(percent)));
    if (target instanceof HTMLInputElement && target.type.toLowerCase() === "range") {
      const min = target.min === "" ? 0 : Number(target.min);
      const max = target.max === "" ? 100 : Number(target.max);
      const safeMin = Number.isFinite(min) ? min : 0;
      const safeMax = Number.isFinite(max) && max > safeMin ? max : 100;
      const raw = safeMin + ((safeMax - safeMin) * bounded / 100);
      const step = Number(target.step);
      const value = Number.isFinite(step) && step > 0
        ? safeMin + Math.round((raw - safeMin) / step) * step
        : raw;
      target.focus();
      target.value = String(Math.max(safeMin, Math.min(safeMax, value)));
    } else if ((target.getAttribute("role") || "").toLowerCase() === "slider") {
      const min = Number(target.getAttribute("aria-valuemin") || "0");
      const max = Number(target.getAttribute("aria-valuemax") || "100");
      const value = min + ((max - min) * bounded / 100);
      target.focus();
      target.setAttribute("aria-valuenow", String(Math.round(value)));
    } else {
      throw new Error("target_not_slider");
    }
    const point = centerOf(target);
    dispatchPointerMouse(target, "pointerdown", point);
    dispatchPointerMouse(target, "mousedown", point);
    target.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertReplacementText", data: String(bounded) }));
    target.dispatchEvent(new Event("change", { bubbles: true }));
    dispatchPointerMouse(target, "mouseup", point);
    dispatchPointerMouse(target, "pointerup", point);
  }

  const normalizedKeys = (keys) => Array.isArray(keys)
    ? keys.map((key) => String(key || "").toLowerCase()).filter(Boolean)
    : [];

  function keyInit(keys, key) {
    const lower = String(key || "").toLowerCase();
    const printable = lower.length === 1 ? lower : {
      enter: "Enter",
      escape: "Escape",
      tab: "Tab",
      home: "Home",
      end: "End",
    }[lower] || lower;
    return {
      bubbles: true,
      cancelable: true,
      key: printable,
      code: lower.length === 1 ? `Key${lower.toUpperCase()}` : printable,
      ctrlKey: keys.includes("ctrl"),
      shiftKey: keys.includes("shift"),
      altKey: keys.includes("alt"),
    };
  }

  function selectContentEditable(element) {
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(element);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  function associatedForm(target) {
    if (target instanceof HTMLFormElement) return target;
    if (target.form instanceof HTMLFormElement) return target.form;
    const closest = target.closest?.("form");
    return closest instanceof HTMLFormElement ? closest : null;
  }

  function submitterFor(form, target) {
    if (
      (target instanceof HTMLButtonElement && String(target.type || "submit").toLowerCase() === "submit")
      || (target instanceof HTMLInputElement && ["submit", "image"].includes(target.type.toLowerCase()))
    ) {
      return target;
    }
    return form.querySelector(
      "button[type='submit']:not([disabled]),button:not([type]):not([disabled]),input[type='submit']:not([disabled]),input[type='image']:not([disabled])",
    );
  }

  function performEnterSemanticFallback(target, keys, keydownAccepted, submitObserved) {
    if (!keydownAccepted || submitObserved) return "page_handled";
    if (keys.some((key) => ["ctrl", "shift", "alt"].includes(key))) return "modified_enter";
    if (target instanceof HTMLTextAreaElement || target.isContentEditable) return "multiline_no_submit";

    const form = associatedForm(target);
    const inputType = target instanceof HTMLInputElement ? target.type.toLowerCase() : "";
    const isSubmitControl = (target instanceof HTMLButtonElement
      && String(target.type || "submit").toLowerCase() === "submit")
      || (target instanceof HTMLInputElement && ["submit", "image"].includes(inputType));
    if (isSubmitControl && form) {
      if (typeof form.requestSubmit === "function") {
        form.requestSubmit(target);
        return "form_request_submit";
      }
      target.click();
      return "submitter_click";
    }

    const isExplicitButton = target instanceof HTMLButtonElement
      || (target instanceof HTMLInputElement && ["button", "reset"].includes(inputType))
      || String(target.getAttribute("role") || "").toLowerCase() === "button";
    if (isExplicitButton && typeof target.click === "function") {
      target.click();
      return "target_click";
    }

    const implicitSubmitInputTypes = new Set([
      "text", "search", "tel", "url", "email", "date", "month", "week",
      "time", "datetime-local", "number",
    ]);
    const canImplicitlySubmit = target instanceof HTMLFormElement
      || (target instanceof HTMLInputElement && implicitSubmitInputTypes.has(inputType));
    if (form && canImplicitlySubmit) {
      const submitter = submitterFor(form, target);
      if (typeof form.requestSubmit === "function") {
        form.requestSubmit(submitter || undefined);
        return "form_request_submit";
      }
      if (submitter && typeof submitter.click === "function") {
        submitter.click();
        return "submitter_click";
      }
      return "form_submit_unavailable";
    }

    if (isSubmitControl && typeof target.click === "function") {
      target.click();
      return "target_click";
    }
    return "synthetic_key_only";
  }

  function performKeyCombination(target, rawKeys) {
    const keys = normalizedKeys(rawKeys);
    const allowed = new Set(["ctrl", "shift", "alt", "enter", "escape", "tab", "home", "end", "a", "c", "v", "x", "z", "y"]);
    if (!keys.length || keys.length > 4 || keys.some((key) => !allowed.has(key))) {
      throw new Error("invalid_key_combination");
    }
    target.focus();
    const primary = [...keys].reverse().find((key) => !["ctrl", "shift", "alt"].includes(key)) || keys[keys.length - 1];
    for (const modifier of keys.filter((key) => ["ctrl", "shift", "alt"].includes(key))) {
      target.dispatchEvent(new KeyboardEvent("keydown", keyInit(keys, modifier)));
    }
    const relatedForm = primary === "enter" ? associatedForm(target) : null;
    let submitObserved = false;
    const markSubmit = () => { submitObserved = true; };
    relatedForm?.addEventListener("submit", markSubmit, { capture: true, once: true });
    const keydownAccepted = target.dispatchEvent(new KeyboardEvent("keydown", keyInit(keys, primary)));
    relatedForm?.removeEventListener("submit", markSubmit, { capture: true });
    let semantic = "synthetic_key";
    if (keys.includes("ctrl") && primary === "a") {
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) target.select();
      else if (target.isContentEditable) selectContentEditable(target);
      semantic = "select_all";
    } else if (primary === "enter") {
      semantic = performEnterSemanticFallback(target, keys, keydownAccepted, submitObserved);
    } else if (primary === "escape") {
      target.blur();
      semantic = "blur";
    }
    target.dispatchEvent(new KeyboardEvent("keyup", keyInit(keys, primary)));
    for (const modifier of keys.filter((key) => ["ctrl", "shift", "alt"].includes(key)).reverse()) {
      target.dispatchEvent(new KeyboardEvent("keyup", keyInit(keys, modifier)));
    }
    return semantic;
  }

  function snapshot(requestId) {
    targetMap = new Map();
    const nodes = candidates().map((element, index) => {
      const reference = `dom-${index + 1}`;
      targetMap.set(reference, element);
      const inputType = element instanceof HTMLInputElement ? element.type.toLowerCase() : null;
      const password = inputType === "password";
      const file = inputType === "file";
      const value = password || file ? "[REDACTED]" : ("value" in element ? String(element.value || "").slice(0, 300) : null);
      return {
        reference,
        role: element.getAttribute("role") || null,
        name: nodeName(element) || null,
        label: labelFor(element) || null,
        tag: element.tagName.toLowerCase(),
        input_type: inputType,
        checked: Boolean(element.checked),
        selected: Boolean(element.selected),
        disabled: Boolean(element.disabled),
        value,
        text: String(element.innerText || "").trim().slice(0, 300) || null,
      };
    });
    return {
      request_id: requestId,
      ok: true,
      document_id: documentId,
      url: location.href,
      title: document.title,
      dom_revision: revision,
      nodes,
      evidence: `dom_snapshot:${documentId}:${revision}:nodes=${nodes.length}`,
    };
  }

  const fail = (requestId, code, message) => ({ request_id: requestId, ok: false, error: { code, message } });

  function act(request) {
    const requestId = String(request.request_id || "missing");
    if (request.expected_document_id !== documentId) return fail(requestId, "stale_document", "document identity changed");
    const action = request.action || {};
    const target = targetMap.get(String(action.target || ""));
    if (!target || !target.isConnected) return fail(requestId, "target_not_found", "DOM reference is stale or missing");
    if (!visible(target) || target.disabled) return fail(requestId, "target_unavailable", "DOM target is hidden or disabled");
    const inputType = target instanceof HTMLInputElement ? target.type.toLowerCase() : "";
    if (inputType === "password" || inputType === "file") return fail(requestId, "sensitive_target_blocked", "password and file inputs are blocked");
    let actionEvidence = "";
    switch (action.action) {
      case "click":
        target.click();
        break;
      case "text_input":
        if (!(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target.isContentEditable)) {
          return fail(requestId, "target_not_editable", "target does not accept text");
        }
        target.focus();
        if (target.isContentEditable) target.textContent = String(action.text || "");
        else target.value = String(action.text || "");
        target.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: String(action.text || "") }));
        target.dispatchEvent(new Event("change", { bubbles: true }));
        break;
      case "select":
        if (!(target instanceof HTMLSelectElement)) return fail(requestId, "target_not_select", "target is not a select element");
        target.value = String(action.value || "");
        target.dispatchEvent(new Event("input", { bubbles: true }));
        target.dispatchEvent(new Event("change", { bubbles: true }));
        break;
      case "check":
        if (!(target instanceof HTMLInputElement) || !["checkbox", "radio"].includes(inputType)) return fail(requestId, "target_not_checkable", "target is not checkable");
        target.checked = Boolean(action.checked);
        target.dispatchEvent(new Event("input", { bubbles: true }));
        target.dispatchEvent(new Event("change", { bubbles: true }));
        break;
      case "submit": {
        const form = target instanceof HTMLFormElement ? target : target.closest("form");
        if (!form) return fail(requestId, "target_not_submittable", "target is not associated with a form");
        form.requestSubmit();
        break;
      }
      case "drag": {
        const dropTarget = targetMap.get(String(action.drop_target || ""));
        try {
          performDrag(target, dropTarget);
        } catch (error) {
          return fail(requestId, String(error?.message || "drag_failed"), "drag action failed");
        }
        break;
      }
      case "slider_drag":
        try {
          setSliderValue(target, action.value);
        } catch (error) {
          return fail(requestId, String(error?.message || "slider_drag_failed"), "slider drag action failed");
        }
        break;
      case "key_combination":
        try {
          actionEvidence = `:semantic=${performKeyCombination(target, action.keys)}`;
        } catch (error) {
          return fail(requestId, String(error?.message || "key_combination_failed"), "key combination failed");
        }
        break;
      case "scroll": {
        const amount = Math.max(1, Math.min(5, Number(action.amount || 1)));
        const unit = Math.max(240, window.innerHeight * 0.8) * amount;
        const dx = action.direction === "left" ? -unit : action.direction === "right" ? unit : 0;
        const dy = action.direction === "up" ? -unit : action.direction === "down" ? unit : 0;
        if (target === document.documentElement || target === document.body) window.scrollBy({ left: dx, top: dy, behavior: "instant" });
        else target.scrollBy({ left: dx, top: dy, behavior: "instant" });
        break;
      }
      default:
        return fail(requestId, "unsupported_action", "action is not allowlisted in the content bridge");
    }
    revision += 1;
    return { request_id: requestId, ok: true, document_id: documentId, url: location.href, title: document.title, dom_revision: revision, evidence: `dom_action:${action.action}:${action.target}${actionEvidence}` };
  }

  chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
    try {
      sendResponse(request?.type === "snapshot" ? snapshot(String(request.request_id || "missing")) : act(request || {}));
    } catch (error) {
      sendResponse(fail(String(request?.request_id || "missing"), "content_script_failed", String(error?.message || error)));
    }
    return false;
  });
}
