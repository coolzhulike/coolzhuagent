// 验证原 Agent 配置入口不会按模型名称推测 Devin 的能力或转发旧密钥。
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname,"../src/app.js"),"utf8");
function between(start,end) {
  const offset = source.indexOf(start), limit = source.indexOf(end,offset+start.length);
  assert(offset >= 0 && limit > offset);
  return source.slice(offset,limit);
}
function form() {
  const elements = new Map();
  for (const role of ["session-api-secret","session-custom-model","session-model-type","session-reasoning-effort","session-name","session-base-url","session-endpoint"]) {
    elements.set(role,{value:"",dataset:{},replaceChildren(...options) { this.options=options; }});
  }
  elements.get("session-custom-model").value="future-vision-image-alias";
  elements.get("session-api-secret").value="old-http-secret";
  elements.get("session-reasoning-effort").value="high";
  elements.get("session-model-type").value="image";
  const context = {
    document: {querySelector(selector) {return elements.get(selector.match(/data-role="([^"]+)"/)?.[1]);},createElement() {return {}; }},
    sessionProviderValue:() => "devin",isCustomProvider:provider => provider === "custom",currentAvatarPathFromForm:() => "",
    renderSessionReasoningHint() {},
    modelCapabilityFor() {throw new Error("Devin 未经协商不应查询 HTTP 模型能力");},
  };
  vm.createContext(context);
  const fragments = [
    between("function sessionModelValueFromForm()", "function updateCustomProviderFields()"),
    between("function renderModelTypeSelect()", "function reasoningOptionDetailsForForm()"),
    between("function updateReasoningEffortOptions(", "function sessionPayloadFromForm()"),
    between("function sessionPayloadFromForm()", "function setSessionForm(")
  ];
  vm.runInContext(fragments.join("\n"),context);
  return {context,elements};
}
test("Devin 的模型名称不会被推断为图片用途或思考层级", () => {
  const h = form();
  h.context.renderModelTypeSelect();
  h.context.updateReasoningEffortOptions("high");
  assert.equal(h.elements.get("session-model-type").value,"text");
  assert.equal(h.elements.get("session-model-type").disabled,true);
  assert.equal(h.elements.get("session-reasoning-effort").value,"auto");
  assert.equal(h.elements.get("session-reasoning-effort").disabled,true);
});
test("原配置入口保存 Devin 不夹带已有 HTTP 密钥与能力覆盖", () => {
  const h = form();
  const payload = h.context.sessionPayloadFromForm();
  assert.equal(payload.provider,"devin");
  assert.equal(payload.model,"future-vision-image-alias");
  assert.equal(payload.model_type,"text");
  assert.equal(payload.reasoning_effort,"auto");
  assert.equal(payload.api_key_ref,undefined);
  assert.equal(payload.base_url,"");
});
