const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

// 最小 DOM 桩：只覆盖 open/renderText/按钮监听实际触达的接口，不镜像生产条件。
function el(tag = 'div') {
  return {tag, dataset: {}, className: '', type: '', value: '', placeholder: '', disabled: false,
    hidden: false, tabIndex: 0, title: '', alt: '', src: '', _text: '', children: [], _handlers: {},
    get textContent() { return this._text; },
    set textContent(v) { this._text = String(v); this.children.length = 0; },
    append(...nodes) { nodes.forEach(n => this.children.push(n)); },
    replaceChildren(...nodes) { this.children.length = 0; nodes.forEach(n => this.children.push(n)); },
    addEventListener(type, fn) { this._handlers[type] = fn; },
    click() { this._handlers.click?.({preventDefault() {}, shiftKey: false}); },
    setAttribute() {}, getAttribute() { return null; },
    scrollIntoView() {}, focus() {}, contains() { return false; },
    querySelector() { return null; }, querySelectorAll() { return []; },
    classList: {add() {}, remove() {}, toggle() {}}, style: {setProperty() {}}};
}

const deferred = () => { let resolve, reject; const promise = new Promise((ok, no) => { resolve = ok; reject = no; }); return {promise, resolve, reject}; };
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const pending = [];
const respond = (status, data) => ({status, ok: status >= 200 && status < 300, json: async () => data});
const params = url => new URLSearchParams(url.split('?')[1] || '');
const texts = node => { const out = []; (function walk(n) { out.push(n._text || ''); (n.children || []).forEach(walk); })(node); return out.join(''); };
const button = (node, label) => { let found = null; (function walk(n) { if (n.tag === 'button' && n._text === label) found = n; (n.children || []).forEach(walk); })(node); return found; };

const context = {
  window: {},
  document: {createElement: tag => el(tag), querySelector: () => null, querySelectorAll: () => [], addEventListener() {}, activeElement: null},
  fetch: url => { const d = deferred(); pending.push({url, d}); return d.promise; },
  URLSearchParams, URL, AbortController, navigator: {}, location: {href: 'http://127.0.0.1:8765/'},
  setWuxiaIconOnly() {}, requestAnimationFrame() {},
  setTimeout, clearTimeout, setInterval, clearInterval, console};
vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../src/content_preview.js'), 'utf8'), context);
const preview = context.window.CoolzhuContentPreview;

test('超长行413回退字节分页，下一段/上一段按钮按真实路径翻页', async () => {
  const container = el();
  const metas = [], locations = [];
  const options = {isCurrent: () => true, onResolved: m => metas.push(m), onLocation: l => locations.push(l),
    openBrowser: async () => { throw new Error('不应走浏览器'); }};
  const ref = preview.reference('tmp/long.txt:1', 'long.txt', 'text/plain');
  assert.equal(ref.file, true); assert.equal(ref.kind, 'text'); assert.equal(ref.line, 1);

  const opening = preview.open(container, ref, options);
  await sleep(1);
  assert.equal(pending.length, 1);
  const p1 = params(pending[0].url);
  assert.equal(p1.get('path'), 'tmp/long.txt');
  assert.equal(p1.get('start_line'), '1');
  assert.equal(p1.get('end_line'), '400');
  pending[0].d.resolve(respond(413, {error: '行窗口超过256KiB，请缩小行范围或使用字节分页读取'}));
  await sleep(1);
  assert.equal(pending.length, 2);
  const p2 = params(pending[1].url);
  assert.equal(p2.get('path'), 'tmp/long.txt');
  assert.equal(p2.get('offset'), '0');
  assert.equal(p2.get('limit'), '262144');
  assert.equal(p2.get('start_line'), null);
  assert.equal(p2.get('end_line'), null);
  pending[1].d.resolve(respond(200, {meta: {revision: 'r1', file_size: 300000}, offset: 0, limit: 262144,
    bytes_read: 262142, next_offset: 262142, content: '字节页一·多字节🙂边界'}));
  await opening;

  assert.ok(texts(container).includes('字节页一·多字节🙂边界'));
  assert.ok(texts(container).includes('字节 0–262142 / 共 300000 字节'));
  assert.equal(metas.length, 1);
  assert.deepEqual(locations, [1]);
  const next1 = button(container, '下一段');
  assert.ok(next1);
  assert.equal(button(container, '上一段'), null); // 首页无历史

  next1.click(); // 真实『下一段』监听→open(byteOffset=262142)
  await sleep(1);
  assert.equal(pending.length, 3);
  const p3 = params(pending[2].url);
  assert.equal(p3.get('offset'), '262142');
  assert.equal(p3.get('limit'), '262144');
  assert.equal(p3.get('start_line'), null);
  pending[2].d.resolve(respond(200, {meta: {revision: 'r1', file_size: 300000}, offset: 262142, limit: 262144,
    bytes_read: 37858, next_offset: null, content: '字节页二·尾页'}));
  await sleep(5);
  const page2 = texts(container);
  assert.ok(page2.includes('字节页二·尾页'));
  assert.ok(!page2.includes('字节页一'), '翻页后旧页内容必须被替换');
  const prev2 = button(container, '上一段');
  assert.ok(prev2);
  assert.equal(button(container, '下一段'), null); // 尾页无下一段

  prev2.click(); // 真实『上一段』监听→open(byteOffset=0, byteHistory 弹栈)
  await sleep(1);
  assert.equal(pending.length, 4);
  assert.equal(params(pending[3].url).get('offset'), '0');
  pending[3].d.resolve(respond(200, {meta: {revision: 'r1', file_size: 300000}, offset: 0, limit: 262144,
    bytes_read: 262142, next_offset: 262142, content: '字节页一·多字节🙂边界'}));
  await sleep(5);
  assert.ok(texts(container).includes('字节页一·多字节🙂边界'));
});

test('迟到与失效请求不得污染当前预览', async () => {
  const container = el();
  const options = {isCurrent: () => true, onResolved() {}, onLocation() {},
    openBrowser: async () => { throw new Error('不应走浏览器'); }};
  const refA = preview.reference('tmp/old.txt', 'old.txt', 'text/plain');
  const refB = preview.reference('tmp/new.txt', 'new.txt', 'text/plain');
  const openA = preview.open(container, refA, options);
  await sleep(1);
  const openB = preview.open(container, refB, options); // dispose() 递增序号作废 A
  await sleep(1);
  const idxA = pending.length - 2, idxB = pending.length - 1;
  pending[idxB].d.resolve(respond(200, {meta: {revision: 'r2', file_size: 10}, offset: 0, limit: 10,
    bytes_read: 10, next_offset: null, start_line: 1, end_line: 1, total_lines: 1, content: 'B新页'}));
  await openB;
  assert.ok(texts(container).includes('B新页'));
  pending[idxA].d.resolve(respond(200, {meta: {revision: 'r1', file_size: 10}, offset: 0, limit: 10,
    bytes_read: 10, next_offset: null, start_line: 1, end_line: 1, total_lines: 1, content: 'A迟到旧页'}));
  await openA;
  assert.ok(texts(container).includes('B新页'));
  assert.ok(!texts(container).includes('A迟到旧页'), '旧代响应不得重绘当前预览');

  // isCurrent=false（预览面板已失焦）的响应同样不得落版
  const refC = preview.reference('tmp/third.txt', 'third.txt', 'text/plain');
  const optionsC = {isCurrent: () => false, onResolved() {}, onLocation() {},
    openBrowser: async () => { throw new Error('不应走浏览器'); }};
  const openC = preview.open(container, refC, optionsC);
  await sleep(1);
  pending[pending.length - 1].d.resolve(respond(200, {meta: {revision: 'r3', file_size: 10}, offset: 0, limit: 10,
    bytes_read: 10, next_offset: null, start_line: 1, end_line: 1, total_lines: 1, content: 'C不可见页'}));
  await openC;
  assert.ok(!texts(container).includes('C不可见页'), '失效请求不得渲染');
  assert.ok(texts(container).includes('正在载入…'));
});
