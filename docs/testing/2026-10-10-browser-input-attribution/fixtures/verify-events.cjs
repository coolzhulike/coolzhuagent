const { isDeepStrictEqual } = require('node:util');
const INPUT = new Set(['pointerdown', 'pointerup', 'pointercancel', 'mousedown', 'mouseup',
  'click', 'dblclick', 'keydown', 'keyup', 'beforeinput', 'input', 'wheel']);

// 只校验页面零输入子门禁；运行终态、协议及宿主归因须独立验证。
function verifyEvents(snapshots, received) {
  const reject = reason => { throw new Error(reason); };
  if (!Array.isArray(snapshots) || snapshots.length !== 2 || !Array.isArray(received)) reject('必须有新旧两文档最终快照及服务器记录');
  const expected = new Map();
  const documents = new Set();
  const roles = new Set();
  for (const snapshot of snapshots) {
    if (typeof snapshot.documentId !== 'string' || !snapshot.documentId || documents.has(snapshot.documentId)) reject('文档身份缺失或重复');
    documents.add(snapshot.documentId);
    if (snapshot.overflow !== 0 || snapshot.beaconRejected !== 0) reject('页面溢出、投递失败或计数缺失');
    if (!Array.isArray(snapshot.events) || snapshot.events.length < 2) reject('页面记录不足');
    if (!Number.isSafeInteger(snapshot.totalEvents) || snapshot.totalEvents !== snapshot.events.length) reject('页面总事件计数缺失或快照被截短');
    const role = snapshot.events[0].role;
    if (!['OLD', 'NEW'].includes(role) || roles.has(role)) reject('新旧文档角色不完整');
    roles.add(role);
    if (snapshot.events[0].kind !== 'document-script-start') reject('缺少文档起始事件');
    let previousTime = -Infinity;
    for (const [index, event] of snapshot.events.entries()) {
      if (event.schema !== 2 || event.document_id !== snapshot.documentId || event.role !== role || event.sequence !== index + 1) reject('记录身份或连续序号失配');
      if (!Number.isFinite(event.page_ms) || event.page_ms < previousTime) reject('页面时钟缺失或逆序');
      previousTime = event.page_ms;
      if (typeof event.trusted !== 'boolean' || event.source !== 'unknown') reject('可信标志或来源证据不完整');
      if (INPUT.has(event.kind) && event.trusted) reject(`发现可信输入：${event.kind} key=${event.key ?? '未知'} code=${event.code ?? '未知'}`);
      expected.set(`${event.document_id}:${event.sequence}`, event);
    }
    if (!snapshot.events.some(event => event.kind === 'load')) reject('缺少文档加载事件');
    if (role === 'OLD' && snapshot.events.at(-1).kind !== 'pagehide') reject('旧文档缺少最终 pagehide');
  }
  const seen = new Set();
  for (const record of received) {
    const key = `${record.document_id}:${record.sequence}`;
    const event = expected.get(key);
    if (!event || seen.has(key)) reject('服务器记录重复或不属于两文档快照');
    const projected = Object.fromEntries(Object.keys(event).map(field => [field, record[field]]));
    if (!isDeepStrictEqual(projected, event)) reject('页面与服务器记录内容不一致');
    if (record.received_unix_ns === undefined || record.server_ns === undefined) reject('服务器接收时钟缺失');
    seen.add(key);
  }
  if (seen.size !== expected.size) reject('服务器漏收页面记录');
  return { page_zero_input_gate: true, documents: 2, matched_events: seen.size,
    formal_acceptance: 'not_proven', input_source: 'unknown' };
}
module.exports = { verifyEvents };
if (require.main === module) {
  const fs = require('node:fs');
  if (process.argv.length !== 5) throw new Error('用法：node verify-events.cjs old-snapshot.json new-snapshot.json page-events.jsonl');
  const snapshots = process.argv.slice(2, 4).map(path => JSON.parse(fs.readFileSync(path, 'utf8')));
  const received = fs.readFileSync(process.argv[4], 'utf8').split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  console.log(JSON.stringify(verifyEvents(snapshots, received)));
}
