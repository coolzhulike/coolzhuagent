// 聊天室任务清单前端契约测试：
// 1) index.html 有按钮/徽标/弹层挂载点；2) 渲染全程 textContent 无 innerHTML；
// 3) app.js 监听 todos-changed 并按 room_id 过滤；4) 徽标合计纯函数行为。
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const consoleRoot = path.resolve(__dirname, "..");
const appSource = fs.readFileSync(path.join(consoleRoot, "src", "app.js"), "utf8");
const indexSource = fs.readFileSync(path.join(consoleRoot, "index.html"), "utf8");
const experienceSource = fs.readFileSync(path.join(consoleRoot, "src", "chat_experience.js"), "utf8");

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

// ── DOM 契约：工具栏按钮、徽标、弹层容器必须存在 ─────────────────────────
assert.ok(indexSource.includes('data-action="chat-todos-open"'), "缺少任务清单打开按钮");
assert.ok(indexSource.includes('data-role="chat-todos-badge"'), "缺少徽标元素");
assert.ok(indexSource.includes('data-role="chat-todos-popover"'), "缺少弹层容器");
assert.ok(indexSource.includes('data-role="chat-todos-summary"'), "缺少汇总行");
assert.ok(indexSource.includes('data-role="chat-todos-list"'), "缺少列表容器");
assert.ok(indexSource.includes('data-action="chat-todos-close"'), "缺少关闭按钮");

// ── XSS 红线：清单相关函数体内禁止 innerHTML / outerHTML / insertAdjacentHTML ──
for (const marker of [
  "function todosAgentName(",
  "function todosTotals(",
  "function todosRender(",
  "function todosRefresh(",
  "function todosSetOpen(",
]) {
  const block = extractBalancedBlock(experienceSource, marker)
    .replace(/\/\/[^\n]*/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.ok(!/innerHTML|outerHTML|insertAdjacentHTML/.test(block), `${marker} 使用了不安全的 HTML 注入`);
  const domFree = new Set(["function todosTotals(", "function todosAgentName(", "function todosRefresh(", "function todosSetOpen("]);
  assert.ok(block.includes("textContent") || domFree.has(marker),
    `${marker} 应使用 textContent`);
}
// 渲染层兜底：整个清单区块不得出现 innerHTML。
const sectionStart = experienceSource.indexOf("房间任务清单");
assert.notEqual(sectionStart, -1, "缺少任务清单区块注释标记");
const section = experienceSource.slice(sectionStart, experienceSource.indexOf("function init(", sectionStart))
  .replace(/\/\/[^\n]*/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
assert.ok(!/innerHTML|insertAdjacentHTML|document\.write/.test(section), "任务清单区块禁止 HTML 注入");

// ── app.js：SSE 监听 todos-changed 且按当前房间过滤 ─────────────────────
assert.ok(appSource.includes('addEventListener("todos-changed"'), "app.js 未监听 todos-changed 事件");
const listenerBlock = extractBalancedBlock(appSource, 'source.addEventListener("todos-changed"');
assert.ok(listenerBlock.includes("payload.room_id") && listenerBlock.includes("activeChatRoomId"),
  "todos-changed 监听必须按 room_id 过滤");
assert.ok(appSource.includes("scheduleTodosRefresh"), "缺少事件防抖刷新");

// ── /clear 兜底刷新 ────────────────────────────────────────────────────
const clearBlock = extractBalancedBlock(appSource, "async function runSlashClear(");
assert.ok(clearBlock.includes("todosRefresh"), "runSlashClear 缺少清单刷新兜底");

// ── vm：徽标合计纯函数 todosTotals ──────────────────────────────────────
const snippet = extractBalancedBlock(experienceSource, "function todosTotals(");
const sandbox = {};
vm.createContext(sandbox);
vm.runInContext(`${snippet}\nthis.totals = todosTotals;`, sandbox);
const assertTotals = (sessions, done, total) => {
  const result = sandbox.totals(sessions);
  assert.equal(result.done, done, `done for ${JSON.stringify(sessions)}`);
  assert.equal(result.total, total, `total for ${JSON.stringify(sessions)}`);
};
assertTotals([
  { session_id: "a", todos: [
    { status: "completed" }, { status: "in_progress" }, { status: "pending" },
  ]},
  { session_id: "b", todos: [{ status: "completed" }, { status: "completed" }] },
], 3, 5);
assertTotals([], 0, 0);
assertTotals(null, 0, 0);
assertTotals([{ session_id: "a" }], 0, 0);

console.log("chat-todos.test.cjs: all assertions passed");
