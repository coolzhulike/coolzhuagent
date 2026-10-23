const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

// 最小 DOM 桩：只覆盖 roomChanged/search/locate 真实路径触达的接口。
function el(tag = 'div') {
  const node = {tag, dataset: {}, hidden: false, className: '', children: [], _text: '', _handlers: {},
    get textContent() { return this._text; },
    set textContent(v) { this._text = String(v); this.children.length = 0; },
    append(...nodes) { nodes.forEach(n => this.children.push(n)); },
    replaceChildren(...nodes) { this.children.length = 0; this._text = ''; nodes.forEach(n => this.children.push(n)); },
    addEventListener(type, fn) { this._handlers[type] = fn; },
    click() { this._handlers.click?.(); },
    remove() {}, before() {}, focus() {}, scrollIntoView() {},
    contains() { return false; }, querySelector() { return null; }, querySelectorAll() { return []; },
    classList: {add() {}, remove() {}}};
  return node;
}

const elements = {};
for (const role of ['chat-history-query','chat-history-results','chat-history-count','chat-history-more',
  'chat-return-latest','chat-usage-list','chat-turn-time','chat-usage-note','chat-activity-label']) {
  elements[role] = el();
}
elements['chat-history-query'].value = '针';
const document = {
  querySelector: selector => {
    const match = selector.match(/^\[data-role="([^"]+)"\]$/);
    return match ? elements[match[1]] || null : null;
  },
  createElement: tag => el(tag), querySelectorAll: () => [], addEventListener() {}, activeElement: null};

const calls = [];
const deferred = () => { let resolve, reject; const promise = new Promise((ok, no) => { resolve = ok; reject = no; }); return {promise, resolve, reject}; };
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

let activated = 0;
const context = {
  window: {addEventListener() {}}, document,
  requestJson: url => { const wait = deferred(); calls.push({url, wait}); return wait.promise; },
  CSS: {escape: value => String(value)},
  activeChatRoomId: 'a', activeWorkspaceKey: 'w', activeChatAbortController: null,
  chatToolWindowId: 'history', activeSessionId: null, chatHistorySync: null,
  chatMessageList: () => null, updateLoadOlderButton() {}, upsertMessage() {},
  loadChatRoomMessages: async () => {}, activateChatHistorySync() { activated++; }, openChatToolWindow() {},
  sessionRegistry: {sessions: []}, setWuxiaIconOnly() {}, getSelectedAgentIds: () => [],
  setTimeout, clearTimeout, setInterval, clearInterval, AbortController, DOMException};
vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../src/chat_experience.js'), 'utf8'), context);
const experience = context.window.CoolzhuChatExperience;
const results = elements['chat-history-results'];
const ids = node => node.children.map(child => child.dataset.locateMessage);
const item = (id, index, snippet) => ({id, index, author: '助手', role: 'assistant', created_at: 1, snippet});
// 真实页面里房间切换由外层脚本维护 activeChatRoomId 再调 roomChanged；夹具按同一顺序驱动。
const switchRoom = id => { context.activeChatRoomId = id; experience.roomChanged(id); };

test('A→B→A 迟到搜索与旧错误不污染当前结果，过期入口不越作用域', async () => {
  switchRoom('a'); // 历史面板打开时切换自动发起搜索 R1
  calls[0].wait.resolve({total: 1, has_more: false, items: [item('a1', 1, '旧A片段')]});
  await sleep(5);
  assert.deepEqual(ids(results), ['a1']);
  assert.equal(elements['chat-history-count'].textContent, '1 条匹配记录');
  assert.equal(elements['chat-history-more'].hidden, true);
  const staleEntry = results.children[0];

  switchRoom('b'); // R2 在途
  switchRoom('a'); // R3 在途，scopeVersion 再进一代
  calls[1].wait.reject(new Error('B房间迟到失败'));
  await sleep(5);
  assert.equal(results.children.length, 0);
  assert.equal(results.textContent, '正在检索完整历史…'); // 旧错误不得写入列表

  calls[2].wait.resolve({total: 2, has_more: true, items: [item('a2', 2, '新A-2'), item('a3', 3, '新A-3')]});
  await sleep(5);
  assert.deepEqual(ids(results), ['a2', 'a3']);
  assert.equal(elements['chat-history-count'].textContent, '2 条匹配记录');
  assert.equal(elements['chat-history-more'].hidden, false);

  const before = calls.length;
  staleEntry.click(); // 过期作用域的条目不得发起定位请求
  assert.equal(calls.length, before);

  void experience.search(); // 同房间旧搜索 R4
  void experience.search(); // 新搜索 R5 取代版本号
  calls[3].wait.resolve({total: 9, has_more: false, items: [item('stale', 1, '迟到结果')]});
  await sleep(5);
  assert.equal(results.children.length, 0); // 被取代的旧搜索不得渲染条目
  assert.equal(elements['chat-history-count'].textContent, '2 条匹配记录'); // 也不得改写计数
  calls[4].wait.resolve({total: 1, has_more: false, items: [item('a4', 4, '最终')]});
  await sleep(5);
  assert.deepEqual(ids(results), ['a4']);

  void experience.locate('a4'); // R6 定位在途
  switchRoom('b'); // 离开作用域并触发 R7
  calls[5].wait.resolve({found: true, position: 1, start_index: 1, total: 1, has_older: false, has_newer: false, messages: [item('a4', 4, '迟到定位')]});
  await sleep(5);
  assert.equal(activated, 0); // 迟到 locate 不得重启历史同步
  calls[6].wait.resolve({total: 1, has_more: false, items: [item('b1', 1, 'B结果')]});
  await sleep(5);
  assert.deepEqual(ids(results), ['b1']);
});

test('工程键切换同样作废在途搜索作用域', async () => {
  context.activeWorkspaceKey = 'w2';
  switchRoom('a'); // 工作区变化即新作用域，自动搜索 R8
  context.activeWorkspaceKey = 'w3';
  experience.roomChanged('a'); // R9
  calls[7].wait.resolve({total: 1, has_more: false, items: [item('w2item', 1, '旧工程结果')]});
  await sleep(5);
  assert.equal(results.textContent, '正在检索完整历史…');
  calls[8].wait.resolve({total: 1, has_more: false, items: [item('w3item', 1, '新工程结果')]});
  await sleep(5);
  assert.deepEqual(ids(results), ['w3item']);
});
