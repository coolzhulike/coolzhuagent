// 验证现有模型表单的连接切换、凭据隔离、保存和查询失败行为。
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");

class Element {
  constructor(dataset = {}) {
    this.dataset = dataset; this.value = ""; this.checked = false;
    this.hidden = false; this.disabled = false; this.required = false;
    this.events = new Map(); this.children = []; this.label = { hidden: false };
    this.classList = { add() {}, remove() {}, toggle() {} };
  }
  set innerHTML(html) {
    this.html = html;
    const values = [...html.matchAll(/<option value="([^"]*)"/g)].map(match => match[1]);
    if (values.length) this.value = values[0];
  }
  get innerHTML() { return this.html; }
  addEventListener(name, listener) { this.events.set(name, listener); }
  removeEventListener(name) { this.events.delete(name); }
  replaceChildren() { this.children = []; this.value = ""; }
  append(node) { this.children.push(node); if (!this.value) this.value = node.value; }
  closest(selector) { return selector === "label" ? this.label : this; }
  contains() { return true; }
  reportValidity() { return true; }
  dispatchEvent() {}
}

function harness(protocol = "devin_acp") {
  const elements = new Map(), actions = new Map(), calls = [];
  const container = new Element();
  Object.defineProperty(container, "innerHTML", {set(html) {
    for (const match of html.matchAll(/data-ms="([^"]+)"/g)) elements.set(match[1], new Element({ ms: match[1] }));
    for (const match of html.matchAll(/data-ms-action="([^"]+)"/g)) actions.set(match[1], new Element({ msAction: match[1] }));
    for (const match of html.matchAll(/<select data-ms="([^"]+)"[^>]*>([\s\S]*?)<\/select>/g)) elements.get(match[1]).innerHTML = match[2];
  }});
  container.querySelector = selector => {
    const key = selector.match(/data-ms="([^"]+)"/);
    const action = selector.match(/data-ms-action="([^"]+)"/);
    return key ? elements.get(key[1]) : action ? actions.get(action[1]) : null;
  };
  let snapshot = {
    configuration_revision: 7, backend_kind: protocol === "devin_acp" ? "devin_acp" : "llm_http",
    protocol, base_url: protocol === "devin_acp" ? "" : "https://api.example.com/v1",
    session: {id: "s1",name: "测试会话",model: "saved-model",model_type: "text",provider: protocol === "devin_acp" ? "devin" : "custom",reasoning_effort: "auto"},
    parameters: {protocol, backend_kind: protocol === "devin_acp" ? "devin_acp" : "llm_http"},
  };
  let discoveryError = false;
  const fetch = async (url, options = {}) => {
    const body = options.body ? JSON.parse(options.body) : undefined;
    calls.push({url,body});
    if (url.endsWith("/models") || url.endsWith("/discover")) {
      if (discoveryError) return {ok:false,status:502,json:async () => ({error:"CLI 查询失败"})};
      return {ok:true,json:async () => ({provider_hint:"Devin",models:[{id:"discovered-model",name:"账号模型",image_input:null}],complete:true,warnings:["任务执行尚未就绪"]})};
    }
    if (url === "/api/sessions") return {ok:true,json:async () => ({sessions:[snapshot.session],active_session_id:"s1"})};
    if (body) snapshot = {...snapshot, protocol:body.parameters.protocol,backend_kind:body.parameters.backend_kind,parameters:body.parameters,session:{...snapshot.session,...body.session}};
    return {ok:true,json:async () => snapshot};
  };
  const window = {location:{href:"http://127.0.0.1:8765/"}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname,"../src/model_settings.js"),"utf8"), {
    window,document:{createElement:() => new Element()},fetch,URL,AbortController,CustomEvent:class {},setWuxiaIconOnly() {},
  });
  const api = window.CoolzhuModelSettings.mount(container);
  return {
    elements,actions,calls,api,
    failDiscovery:() => {discoveryError = true;},
    change(key,value) { elements.get(key).value = value; container.events.get("change")({target:elements.get(key)}); },
    click(action) { container.events.get("click")({target:actions.get(action)}); },
    save() { return elements.get("form").events.get("submit")({preventDefault() {}}); },
  };
}
const settle = async () => { for (let i=0;i<5;i++) await new Promise(resolve => setImmediate(resolve)); };

test("Devin 查询不发送隐藏的 HTTP 密钥，失败保留模型草稿", async () => {
  const h = harness(); await settle();
  assert.equal(h.elements.get("base-url").required,false);
  assert.equal(h.elements.get("api-key").disabled,true);
  h.elements.get("api-key").value = "hidden-secret";
  h.click("discover"); await settle();
  const query = h.calls.find(call => call.url === "/api/backends/devin/models");
  assert.deepEqual(query.body,{});
  assert.equal(h.elements.get("model").value,"saved-model");
  h.click("adopt-model");
  assert.equal(h.elements.get("model").value,"discovered-model");
  h.failDiscovery(); h.click("discover"); await settle();
  assert.equal(h.elements.get("model").value,"discovered-model");
  assert.match(h.elements.get("discovery-status").textContent,/失败/);
  h.api.destroy();
});

test("HTTP 切换至 Devin 保存时清理不适用覆盖，保留版本和会话身份", async () => {
  const h = harness("openai_chat_completions"); await settle();
  h.elements.get("api-key").value = "do-not-send";
  h.elements.get("temperature").value = "0.4";
  h.elements.get("context").value = "8192";
  h.change("protocol","devin_acp");
  await h.save();
  const saved = h.calls.find(call => call.url === "/api/sessions/s1/model-settings" && call.body);
  assert.equal(saved.body.expected_revision,7);
  assert.equal(saved.body.session.provider,"devin");
  assert.equal(saved.body.session.api_key_ref,undefined);
  assert.equal(saved.body.parameters.backend_kind,"devin_acp");
  assert.equal(saved.body.parameters.temperature,null);
  assert.equal(saved.body.parameters.context_window,0);
  assert.equal(saved.body.parameters.base_url,null);
  assert.match(h.elements.get("status").textContent,/尚未就绪/);
  h.change("protocol","openai_chat_completions");
  assert.equal(h.elements.get("base-url").required,true);
  assert.equal(h.elements.get("temperature").disabled,false);
  assert.equal(h.elements.get("reasoning-mode").disabled,false);
  h.api.destroy();
});

test("原有 HTTP 模型发现仍走原目录接口和一次性密钥", async () => {
  const h = harness("openai_chat_completions"); await settle();
  h.elements.get("api-key").value = "http-key";
  h.click("discover"); await settle();
  const query = h.calls.find(call => call.url === "/api/models/discover");
  assert.equal(query.body.api_key,"http-key");
  assert.equal(query.body.protocol,"openai_chat_completions");
  assert.equal(query.body.base_url,"https://api.example.com/v1");
  h.api.destroy();
});
