const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const consoleRoot = path.resolve(__dirname, "..");
const appPath = path.join(consoleRoot, "src", "app.js");
const indexPath = path.join(consoleRoot, "index.html");
const stylesPath = path.join(consoleRoot, "src", "styles.css");
const appSource = fs.readFileSync(appPath, "utf8");
const indexSource = fs.readFileSync(indexPath, "utf8");
const stylesSource = fs.readFileSync(stylesPath, "utf8");

function extract(startMarker, endMarker) {
  const start = appSource.indexOf(startMarker);
  assert.notEqual(start, -1, `未找到源片段：${startMarker}`);
  const end = appSource.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `未找到源片段结束：${endMarker}`);
  return appSource.slice(start, end);
}

function extractBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `未找到源片段：${startMarker}`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `未找到源片段结束：${endMarker}`);
  return source.slice(start, end);
}

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

const defsStart = appSource.indexOf("const TOOL_GROUP_DEFS = [");
assert.notEqual(defsStart, -1, "未找到 TOOL_GROUP_DEFS");
const defsEnd = appSource.indexOf("];", defsStart) + 2;
assert.ok(defsEnd > defsStart, "未找到 TOOL_GROUP_DEFS 结束位置");
const toolGroupDefs = appSource.slice(defsStart, defsEnd);
const toolGroupItems = extract("function toolGroupItems(catalog, def, query) {", "\n\n// 详情弹窗");
const flattenToolCatalog = extract("function flattenToolCatalog(catalog) {", "\n\nasync function runToolDryRun");
const renderStatus = extract("function renderToolInventoryCatalogStatus(catalog) {", "\n\nasync function captureDesktop");

class StubElement {
  constructor(name) {
    this.name = name;
    this.textContent = "";
    this.title = "";
    this.classes = new Set();
    this.children = new Map();
    this.classList = {
      toggle: (className, enabled) => {
        if (enabled) this.classes.add(className);
        else this.classes.delete(className);
      },
    };
  }

  querySelector(selector) {
    return this.children.get(selector) || null;
  }
}

function makeDocument() {
  const groups = new Map();
  for (const key of ["cli", "mcp", "skill"]) {
    groups.set(key, new StubElement(`group-${key}`));
  }
  const computeRow = new StubElement("compute-row");
  const computeStatus = new StubElement("compute-status");
  const manageButton = new StubElement("compute-manage");
  computeRow.children.set(".tool-inventory-status", computeStatus);
  computeRow.children.set('[data-action="tool-inventory-manage"]', manageButton);
  const document = {
    querySelector(selector) {
      const match = selector.match(/^\[data-role="tool-inventory-status-(.+)"\]$/);
      if (match) return groups.get(match[1]) || null;
      if (selector === '[data-tool-inventory="compute-use"]') return computeRow;
      throw new Error(`stub 未覆盖选择器：${selector}`);
    },
  };
  return { document, groups, computeStatus, manageButton };
}

const context = { console };
vm.runInNewContext(
  `${toolGroupDefs}\n${toolGroupItems}\n${flattenToolCatalog}\n${renderStatus}\nthis.renderStatus = renderToolInventoryCatalogStatus;`,
  context,
  { filename: appPath },
);

function assertStatus(element, text, online) {
  assert.equal(element.textContent, text);
  assert.equal(element.classes.has("is-online"), online);
  assert.equal(element.classes.has("is-offline"), !online);
}

function runStatusCase(name, catalog, expected) {
  const dom = makeDocument();
  context.document = dom.document;
  context.renderStatus(catalog);
  for (const [key, value] of Object.entries(expected.groups)) {
    assertStatus(dom.groups.get(key), value.text, value.online);
    assert.equal(dom.groups.get(key).title, value.title || "");
  }
  assertStatus(dom.computeStatus, expected.compute.text, expected.compute.online);
  assert.equal(dom.computeStatus.title, expected.compute.title || "");
  assert.equal(dom.manageButton.textContent, "管理");
  return `${name}: PASS`;
}

const noEntries = {
  categories: [],
};
const partialTrue = {
  categories: [
    { id: "core-tools", items: [
      { id: "core.read", executable_now: true },
      { id: "core.write", executable_now: false },
    ] },
    { id: "computer-use", items: [
      { id: "computer.click", executable_now: true },
      { id: "computer.drag", executable_now: false },
    ] },
  ],
};
const allFalse = {
  categories: [
    { id: "core-tools", items: [{ id: "core.read", executable_now: false }, { id: "core.write", executable_now: false }] },
    { id: "vision-tools", items: [{ id: "vision.capture", executable_now: false }] },
    { id: "computer-use", items: [{ id: "computer.click", executable_now: false }, { id: "computer.drag", executable_now: false }] },
    { id: "plugins", items: [{ id: "plugin.demo", executable_now: false }] },
    { id: "skills", items: [{ id: "skill.demo", executable_now: false }] },
  ],
};

const results = [];
results.push(runStatusCase("无条目", noEntries, {
  groups: {
    cli: { text: "无条目", online: false },
    mcp: { text: "无条目", online: false },
    skill: { text: "无条目", online: false },
  },
  compute: { text: "无条目", online: false },
}));
results.push(runStatusCase("部分 executable_now=true", partialTrue, {
  groups: {
    cli: { text: "可执行 2/4", online: true },
    mcp: { text: "无条目", online: false },
    skill: { text: "无条目", online: false },
  },
  compute: { text: "可执行 1/2", online: true },
}));
results.push(runStatusCase("全部 executable_now=false", allFalse, {
  groups: {
    cli: { text: "可执行 0/5", online: false, title: "已收录 5 项，当前不可执行" },
    mcp: { text: "可执行 0/1", online: false, title: "已收录 1 项，当前不可执行" },
    skill: { text: "可执行 0/1", online: false, title: "已收录 1 项，当前不可执行" },
  },
  compute: { text: "可执行 0/2", online: false, title: "已收录 2 项，当前不可执行" },
}));

// 静态契约：只核对对应的 session HTML 容器与事件处理块，不执行 GUI。
const sessionContainer = extractBetween(
  indexSource,
  '<div class="session-select-wrap">',
  "\n                <div class=\"session-agent-grid\">",
);
assert.match(
  sessionContainer,
  /<button type="button" class="session-select-trigger" data-role="session-trigger" aria-controls="session-list" aria-expanded="false" aria-haspopup="listbox">/,
);
assert.match(sessionContainer, /<div id="session-list" class="session-select-dropdown" data-role="session-list" role="listbox"><\/div>/);

const sessionStateSource = extractBalancedBlock(
  appSource,
  "const setSessionListOpen = (open, { focusTrigger = false } = {}) =>",
);
const sessionOptionSource = extractBalancedBlock(
  appSource,
  `document.querySelector('[data-role="session-list"]')?.addEventListener("click", async (event) =>`,
);
const sessionKeydownSource = extractBalancedBlock(
  appSource,
  'document.addEventListener("keydown", (event) =>',
);
assert.match(sessionStateSource, /sessionWrap\?\.classList\.toggle\("open", isOpen\)/);
assert.match(sessionStateSource, /sessionTrigger\?\.setAttribute\("aria-expanded", String\(isOpen\)\)/);
assert.match(sessionOptionSource, /setSessionListOpen\(false, \{ focusTrigger: true \}\)/);
assert.match(sessionKeydownSource, /event\.key === "Escape" && sessionWrap\?\.classList\.contains\("open"\)/);
assert.match(sessionKeydownSource, /setSessionListOpen\(false, \{ focusTrigger: true \}\)/);

// 静态契约：只核对聊天消息列表及其窄容器块，不冒充 GUI 验收。
const messageListSource = extractBalancedBlock(
  stylesSource,
  'body.ui-3d .workbench-window[data-window-theme="communication-bay"] .message-list',
);
const narrowContainerSource = extractBalancedBlock(
  stylesSource,
  "@container chat-message-list (max-width: 560px)",
);
const narrowMessageSource = extractBalancedBlock(
  narrowContainerSource,
  'body.ui-3d .workbench-window[data-window-theme="communication-bay"] .message',
);
const narrowTimeSource = extractBalancedBlock(
  narrowContainerSource,
  'body.ui-3d .workbench-window[data-window-theme="communication-bay"] .message > time',
);
assert.match(messageListSource, /container-type:\s*inline-size;/);
assert.match(messageListSource, /container-name:\s*chat-message-list;/);
assert.match(narrowMessageSource, /grid-template-columns:\s*28px minmax\(0, 1fr\);/);
assert.match(narrowTimeSource, /grid-row:\s*2;/);
results.push("session button/ARIA/Escape + narrow CSS: PASS (静态契约)");

console.log(JSON.stringify({ extracted: true, cases: results, gui: "未执行（静态契约，不冒充 GUI）" }, null, 2));
