const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const consoleRoot = path.resolve(__dirname, "..");
const appPath = path.join(consoleRoot, "src", "app.js");
const indexPath = path.join(consoleRoot, "index.html");
const appSource = fs.readFileSync(appPath, "utf8");
const indexSource = fs.readFileSync(indexPath, "utf8");

function extractBalancedBlock(source, startMarker) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `未找到块起点：${startMarker}`);
  const braceStart = source.indexOf("{", start + startMarker.length);
  assert.notEqual(braceStart, -1, `未找到块起点大括号：${startMarker}`);
  let depth = 0;
  for (let index = braceStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`块大括号未闭合：${startMarker}`);
}

const snippet = [
  "let agentRegistry = { agents: [], active_agent_ids: [] };",
  extractBalancedBlock(appSource, "function canSendToAgent(agent)"),
  extractBalancedBlock(appSource, "function mentionCandidateAgents()"),
  extractBalancedBlock(appSource, "function mentionTokenAt(value, cursor)"),
  extractBalancedBlock(appSource, "function mentionMatches(query, limit = 8)"),
].join("\n");

const sandbox = { console };
vm.createContext(sandbox);
vm.runInContext(`${snippet}\nthis.__api = { mentionTokenAt, mentionMatches, mentionCandidateAgents, setRegistry: (a) => { agentRegistry = a; } };`, sandbox);
const api = sandbox.__api;

// ── DOM 契约：弹层挂载点必须存在 ──────────────────────────────────────────
assert.ok(
  indexSource.includes('data-role="mention-popover"'),
  "index.html 缺少 mention-popover 挂载点",
);

// ── mentionCandidateAgents：只放行 selectable+enabled ─────────────────────
api.setRegistry({
  agents: [
    { id: "a1", name: "agnes", selectable: true, enabled: true },
    { id: "a2", name: "swe", selectable: true, enabled: false },
    { id: "a3", name: "devin", selectable: false, enabled: true },
    { id: "a4", name: "glm", selectable: true, enabled: true },
  ],
});
assert.deepEqual(
  api.mentionCandidateAgents().map((agent) => agent.id),
  ["a1", "a4"],
  "候选集必须过滤掉 disabled / non-selectable",
);

// ── mentionTokenAt：@ 仅在串首或空白后生效，query 无空白 ──────────────────
// vm 上下文对象 prototype 不同，逐字段断言而非 deepEqual。
const assertToken = (token, start, query) => {
  assert.equal(token?.start, start);
  assert.equal(token?.query, query);
};
assertToken(api.mentionTokenAt("@ag", 3), 0, "ag");
assertToken(api.mentionTokenAt("hi @sw", 6), 3, "sw");
assertToken(api.mentionTokenAt("@", 1), 0, "");
assert.equal(api.mentionTokenAt("abc@ag", 6), null, "紧贴文字的 @ 不触发");
assert.equal(api.mentionTokenAt("@ag xx", 6), null, "光标在空白后 token 已结束");
assert.equal(api.mentionTokenAt("没有@", 3), null, "中文字符不算空白边界时紧贴 @ 不触发");
assert.equal(
  api.mentionTokenAt(`@${"x".repeat(41)}`, 42),
  null,
  "超长 token 不触发",
);
// 光标落在 @ 之前不触发
assert.equal(api.mentionTokenAt("hello @ag", 5), null);

// ── mentionMatches：精确 > 前缀 > 包含，按分数排序 ────────────────────────
api.setRegistry({
  agents: [
    { id: "a1", name: "agnes", selectable: true, enabled: true },
    { id: "a2", name: "agnes-pro", selectable: true, enabled: true },
    { id: "a3", name: "xxagnes", selectable: true, enabled: true },
    { id: "a4", name: "glm", selectable: true, enabled: true },
  ],
});
assert.deepEqual(
  api.mentionMatches("agnes").map((agent) => agent.id),
  ["a1", "a2", "a3"],
  "精确命中排最前，其次前缀，最后包含",
);
assert.deepEqual(
  api.mentionMatches("").map((agent) => agent.id).sort(),
  ["a1", "a2", "a3", "a4"],
  "空 query 返回全部候选",
);
assert.equal(api.mentionMatches("zzz").length, 0, "无命中返回空");
assert.equal(api.mentionMatches("agnes", 2).length, 2, "limit 截断生效");

// display_name 回退
api.setRegistry({
  agents: [{ id: "d1", display_name: "Devin 助手", selectable: true, enabled: true }],
});
assert.equal(api.mentionMatches("devin")[0]?.id, "d1", "无 name 时应匹配 display_name");

// 中文名称匹配
api.setRegistry({
  agents: [{ id: "c1", name: "武侠助手", selectable: true, enabled: true }],
});
assert.equal(api.mentionMatches("武侠")[0]?.id, "c1", "中文名前缀应命中");

console.log("mention-targets OK");
