const assert = require('node:assert/strict');
const { install } = require('./event-probe.js');
const listeners = new Map();
const beacons = [];
let accept = true;
const state = install({
  window: { addEventListener: (kind, callback) => listeners.set(kind, callback) },
  document: { body: { dataset: { role: 'NEW' } }, activeElement: { tagName: 'BODY' },
    hasFocus: () => true, visibilityState: 'visible' },
  navigator: { sendBeacon: (_, blob) => { beacons.push(blob); return accept; } },
  performance: { timeOrigin: 1000, now: () => 25 },
  crypto: { randomUUID: () => 'synthetic-document-id' }, Blob,
});
listeners.get('keydown')({ type: 'keydown', isTrusted: true, timeStamp: 20,
  target: { tagName: 'BODY' }, key: 'Enter', code: 'Enter', location: 0,
  repeat: false, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false,
  data: 'synthetic-text-must-not-be-recorded' });
const input = state.events[1];
assert.equal(input.key, 'Enter');
assert.equal(input.code, 'Enter');
assert.equal(input.target.tag, 'BODY');
assert.equal(input.active_element.tag, 'BODY');
assert.equal(input.document_id, state.events[0].document_id);
assert.equal(input.sequence, 2);
assert.equal(input.page_ms, 1025);
assert.equal(input.event_timestamp_ms, 20);
assert.equal(input.source, 'unknown');
assert.equal(input.trusted, true);
assert(!JSON.stringify(input).includes('synthetic-text-must-not-be-recorded'));
accept = false;
listeners.get('keyup')({ type: 'keyup', isTrusted: true });
assert.equal(state.beaconRejected, 1);
for (let i = 0; i < 10000; i++) listeners.get('click')({ type: 'click', isTrusted: false });
assert.equal(state.events.length, 10000);
assert.equal(state.overflow, 3);
console.log('验收探针字段、来源未知、无正文、丢投递及容量计数回归通过；没有真实浏览器输入证据');
