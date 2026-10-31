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

// SLASH_COMMANDS 是 const 数组，按 ]; 截取。
const commandsStart = appSource.indexOf("const SLASH_COMMANDS = [");
assert.notEqual(commandsStart, -1, "未找到 SLASH_COMMANDS");
const commandsEnd = appSource.indexOf("];", commandsStart);
assert.notEqual(commandsEnd, -1, "SLASH_COMMANDS 未闭合");
const commandsSnippet = appSource.slice(commandsStart, commandsEnd + 2);

const snippet = [
  "let agentRegistry = { agents: [], active_agent_ids: [] };",
  commandsSnippet,
  extractBalancedBlock(appSource, "function canSendToAgent(agent)"),
  extractBalancedBlock(appSource, "function sessionCandidates()"),
  extractBalancedBlock(appSource, "function sessionMatches(query, limit = 8)"),
  extractBalancedBlock(appSource, "function slashParse(value)"),
].join("\n");

const sandbox = { console };
vm.createContext(sandbox);
vm.runInContext(`${snippet}\nthis.__api = { slashParse, sessionMatches, sessionCandidates, setRegistry: (a) => { agentRegistry = a; } };`, sandbox);
const api = sandbox.__api;

// ── DOM 契约：命令联想弹层挂载点必须存在，输入框提示 / 命令 ─────────────────
assert.ok(
  indexSource.includes('data-role="composer-assist-popover"'),
  "index.html 缺少 composer-assist-popover 挂载点",
);
assert.ok(
  indexSource.includes("/session") && indexSource.includes("/clear") && indexSource.includes("/delete"),
  "index.html 输入框提示缺少 /session /clear /delete",
);

// ── slashParse：仅 / 开头才算命令 ────────────────────────────────────────
// vm 上下文对象 prototype 不同，逐字段断言而非 deepEqual。
const assertParse = (value, mode, cmd, arg) => {
  const parsed = api.slashParse(value);
  assert.equal(parsed?.mode, mode, `「${value}」mode`);
  assert.equal(parsed?.cmd, cmd, `「${value}」cmd`);
  assert.equal(parsed?.arg, arg, `「${value}」arg`);
};

assert.equal(api.slashParse("hello world"), null, "普通文本不触发");
assert.equal(api.slashParse("hello /session"), null, "/ 不在串首不触发");
assert.equal(api.slashParse(""), null, "空串不触发");

assertParse("/", "commands", "", "");
assertParse("/s", "commands", "s", "");
assertParse("/se", "commands", "se", "");
assertParse("/c", "commands", "c", "");
assertParse("/d", "commands", "d", "");

// 完整命令名（无需空格）直接进入对应模式
assertParse("/session", "session", "session", "");
assertParse("/session gl", "session", "session", "gl");
assertParse("/session 武侠", "session", "session", "武侠");
assertParse("/clear", "clear", "clear", "");
assertParse("/clear 多余参数", "clear", "clear", "多余参数");
assertParse("/delete", "delete", "delete", "");
assertParse("/delete 17", "delete", "delete", "17");
assertParse("/delete #17", "delete", "delete", "#17");

// 未识别命令 → unknown（Enter 放行为原文发送）
assertParse("/bogus", "unknown", "bogus", "");
assertParse("/bogus x", "unknown", "bogus", "x");
assertParse("/sessionx", "unknown", "sessionx", "", "完整词失配且无空格 → unknown，Enter 放行原文");
assertParse("/z", "unknown", "z", "");

// ── sessionCandidates：只放行 selectable+enabled ─────────────────────────
api.setRegistry({
  agents: [
    { id: "a1", name: "agnes", selectable: true, enabled: true },
    { id: "a2", name: "swe", selectable: true, enabled: false },
    { id: "a3", name: "devin", selectable: false, enabled: true },
    { id: "a4", name: "glm", selectable: true, enabled: true },
  ],
});
assert.deepEqual(
  api.sessionCandidates().map((agent) => agent.id),
  ["a1", "a4"],
  "候选集必须过滤掉 disabled / non-selectable",
);

// ── sessionMatches：精确 > 前缀 > 包含，按分数排序 ────────────────────────
api.setRegistry({
  agents: [
    { id: "a1", name: "agnes", selectable: true, enabled: true },
    { id: "a2", name: "agnes-pro", selectable: true, enabled: true },
    { id: "a3", name: "xxagnes", selectable: true, enabled: true },
    { id: "a4", name: "glm", selectable: true, enabled: true },
  ],
});
assert.deepEqual(
  api.sessionMatches("agnes").map((agent) => agent.id),
  ["a1", "a2", "a3"],
  "精确命中排最前，其次前缀，最后包含",
);
assert.deepEqual(
  api.sessionMatches("").map((agent) => agent.id).sort(),
  ["a1", "a2", "a3", "a4"],
  "空 query 返回全部候选",
);
assert.equal(api.sessionMatches("zzz").length, 0, "无命中返回空");
assert.equal(api.sessionMatches("agnes", 2).length, 2, "limit 截断生效");

// display_name 回退
api.setRegistry({
  agents: [{ id: "d1", display_name: "Devin 助手", selectable: true, enabled: true }],
});
assert.equal(api.sessionMatches("devin")[0]?.id, "d1", "无 name 时应匹配 display_name");

// 中文名称匹配
api.setRegistry({
  agents: [{ id: "c1", name: "武侠助手", selectable: true, enabled: true }],
});
assert.equal(api.sessionMatches("武侠")[0]?.id, "c1", "中文名前缀应命中");

console.log("slash-commands OK");
