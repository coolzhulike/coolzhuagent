// 离线契约验证：聚合渠道归属、两代迁移和自定义值保存，不代表正式 UI 实测。
const assert = require("node:assert/strict");
const avatars = require("../src/brand_avatars.js");
const preset = (key) => `assets/avatars/brand-v1/${key}.png`;
for (const old of avatars.legacyPaths) {
  assert.equal(avatars.resolve({ avatar: old, provider: "devin", model: "anthropic/claude-opus-4-6" }), preset("claude"));
  assert.equal(avatars.resolve({ avatar: `./${old}`, provider: "alibaba-bailian", model: "deepseek-v3.2" }), preset("deepseek"));
}
for (const custom of ["assets/avatars/my-upload.png", "assets/avatars/uploads/user-1.webp",
  "data:image/png;base64,AAAA", "blob:https://localhost/example", "https://example.invalid/avatar.png"]) {
  assert.equal(avatars.resolve({ avatar: custom, model: "gpt-5" }), custom);
  assert.equal(avatars.migrate(custom, { model: "qwen3-coder" }), custom);
}
assert.equal(avatars.resolve({ provider: "devin", model: "SWE-2-medium" }), preset("generic"));
assert.equal(avatars.resolve({ provider: "openai", model: "google/gemini-2.5-pro" }), preset("gemini"));
assert.equal(avatars.resolve({ provider: "custom", model: "Qwen/Qwen3-Coder" }), preset("qwen"));
assert.equal(avatars.resolve({ provider: "custom", model: "gpt-5" }), preset("chatgpt"));
assert.equal(avatars.resolve({ provider: "xai", model: "grok-4" }), preset("grok"));
assert.equal(avatars.resolve({ provider: "智谱 AI", model: "glm-4.6" }), preset("glm"));
assert.equal(avatars.resolve({ provider: "OpenAI" }), preset("chatgpt"));
assert.equal(avatars.resolve({ provider: "custom", model: "my-unrecognized-model" }), preset("generic"));
assert.equal(avatars.resolve({ avatar: preset("deepseek"), model: "claude-sonnet-4-6" }), preset("deepseek"));
assert.equal(avatars.url("data:image/png;base64,AAAA"), "data:image/png;base64,AAAA");
console.log("品牌匹配、22条旧预设迁移与自定义保留契约通过；正式UI待联合验证。");
