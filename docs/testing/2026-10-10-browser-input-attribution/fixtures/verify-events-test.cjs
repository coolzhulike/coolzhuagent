const assert = require('node:assert/strict');
const { verifyEvents } = require('./verify-events.cjs');
const event = (id, role, kind, sequence) => ({ schema: 2, document_id: id, role, kind,
  sequence, page_ms: sequence, trusted: false, source: 'unknown', key: null, code: null });
const snapshots = [
  { documentId: 'old', totalEvents: 3, overflow: 0, beaconRejected: 0, events: [
    event('old', 'OLD', 'document-script-start', 1), event('old', 'OLD', 'load', 2), event('old', 'OLD', 'pagehide', 3)] },
  { documentId: 'new', totalEvents: 2, overflow: 0, beaconRejected: 0, events: [
    event('new', 'NEW', 'document-script-start', 1), event('new', 'NEW', 'load', 2)] },
];
const receivedFor = pages => pages.flatMap(page => page.events.map(item => ({ ...item, received_unix_ns: '123', server_ns: '456' })));
assert.equal(verifyEvents(snapshots, receivedFor(snapshots)).formal_acceptance, 'not_proven');
const change = edit => { const pages = structuredClone(snapshots); edit(pages); return pages; };
const trusted = change(pages => pages[1].events.push({ ...event('new', 'NEW', 'keydown', 3), trusted: true, key: 'Enter', code: 'Enter' }));
trusted[1].totalEvents = 3;
assert.throws(() => verifyEvents(trusted, receivedFor(trusted)), /可信输入/);
assert.throws(() => verifyEvents(snapshots, receivedFor(snapshots).slice(1)), /漏收/);
assert.throws(() => verifyEvents(snapshots, [...receivedFor(snapshots), receivedFor(snapshots)[0]]), /重复/);
const tampered = receivedFor(snapshots); tampered[0].role = 'NEW';
assert.throws(() => verifyEvents(snapshots, tampered), /不一致/);
for (const field of ['overflow', 'beaconRejected']) {
  const pages = change(items => items[0][field] = 1);
  assert.throws(() => verifyEvents(pages, receivedFor(pages)), /溢出/);
}
const gap = change(pages => pages[0].events[1].sequence = 4);
assert.throws(() => verifyEvents(gap, receivedFor(gap)), /连续序号/);
const unclosed = change(pages => pages[0].events.pop());
unclosed[0].totalEvents = 2;
assert.throws(() => verifyEvents(unclosed, receivedFor(unclosed)), /pagehide/);
const truncated = change(pages => pages[1].totalEvents = 3);
assert.throws(() => verifyEvents(truncated, receivedFor(truncated)), /截短/);
const missingCounter = change(pages => delete pages[1].totalEvents);
assert.throws(() => verifyEvents(missingCounter, receivedFor(missingCounter)), /计数缺失/);
assert.throws(() => verifyEvents([], []), /两文档/);
console.log('页面零输入校验：可信输入、漏收、重复、篡改、容量异常、序号缺口及未关闭均拒绝；非正式实机证据');
