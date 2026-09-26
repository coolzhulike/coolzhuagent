import assert from 'node:assert/strict';
import http from 'node:http';
import { once } from 'node:events';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { fetchJsonBounded } from './lib/clawbot-ilink-http.mjs';

const folder = await mkdtemp(join(tmpdir(), 'coolzhu-ilink-delivery-'));
const cursorFile = join(folder, 'cursor.json');
const cursors = [];
const upstream = http.createServer(async (req, res) => {
  if (req.url === '/slow') { res.writeHead(200); res.write('{'); return; }
  if (req.url === '/large') { res.end(JSON.stringify({ text: 'x'.repeat(100) })); return; }
  const chunks = []; for await (const chunk of req) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString() || '{}');
  res.setHeader('content-type', 'application/json');
  if (req.url.endsWith('/getupdates')) {
    const cursor = body.get_updates_buf; cursors.push(cursor);
    const id = cursor === '' ? '1' : cursor === 'c1' ? '2' : null;
    res.end(JSON.stringify({ ret: 0, get_updates_buf: id ? `c${id}` : 'c2', msgs: id ? [{
      message_id: id, from_user_id: 'peer', create_time_ms: 123,
      item_list: [{ type: 1, text_item: { text: `message ${id}` } }],
    }] : [] }));
  } else res.end('{"ret":0}');
});
upstream.listen(0, '127.0.0.1'); await once(upstream, 'listening');
const upstreamUrl = `http://127.0.0.1:${upstream.address().port}`;
const socket = http.createServer(); socket.listen(0, '127.0.0.1'); await once(socket, 'listening');
const port = socket.address().port; await new Promise(resolve => socket.close(resolve));
const url = `http://127.0.0.1:${port}`; let child;
async function start() {
  child = spawn(process.execPath, ['scripts/clawbot-ilink-provider.mjs'], { windowsHide: true, stdio: 'ignore', env: { ...process.env,
    CLAWBOT_ILINK_BASE_URL: upstreamUrl, CLAWBOT_ILINK_BIND_PORT: String(port),
    CLAWBOT_ILINK_ACCOUNT_ID: 'account-test', CLAWBOT_ILINK_BOT_TOKEN: 'test-only-token',
    CLAWBOT_ILINK_PROVIDER_TOKEN: '', CLAWBOT_ILINK_CURSOR_FILE: cursorFile,
  } });
  for (let i = 0; i < 80; i++) {
    try { if ((await fetch(`${url}/health`)).ok) return; } catch {}
    if (child.exitCode !== null) throw new Error('provider startup failed');
    await new Promise(resolve => setTimeout(resolve, 25));
  }
  throw new Error('provider startup timeout');
}
async function stop() { if (child && child.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; } child = null; }
async function poll() { const response = await fetch(`${url}/updates`); assert.equal(response.status, 200); return response.json(); }
async function ack(id, account = 'account-test') { return fetch(`${url}/updates/ack`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ account_id: account, delivery_id: id }) }); }
try {
  await assert.rejects(fetchJsonBounded(`${upstreamUrl}/slow`, {}, 60), error => error.code === 'ETIMEDOUT');
  await assert.rejects(fetchJsonBounded(`${upstreamUrl}/large`, {}, 1000, 16), /大小限制/);
  await start(); const first = await poll(); assert.equal(first.updates[0].message.external_msg_id, '1');
  assert.deepEqual(await poll(), first); assert.equal(cursors.length, 1);
  assert.equal((await ack(first.delivery_id, 'other-account')).status, 409);
  assert.equal((await ack('cross-batch')).ok, false);
  await stop(); await start(); const replay = await poll(); assert.equal(replay.updates[0].message.external_msg_id, '1');
  assert.equal((await ack(replay.delivery_id)).status, 200);
  assert.equal(JSON.parse(await readFile(cursorFile, 'utf8')).cursor, 'c1');
  const second = await poll(); assert.equal(second.updates[0].message.external_msg_id, '2');
  assert.equal((await ack(replay.delivery_id)).status, 200); assert.deepEqual(await poll(), second);
  await stop(); await start(); const resumed = await poll(); assert.equal(resumed.updates[0].message.external_msg_id, '2');
  assert.equal((await ack(second.delivery_id)).ok, false);
  assert.equal((await ack(resumed.delivery_id)).status, 200);
  await stop(); await start(); assert.equal((await poll()).updates.length, 0);
  assert.equal(JSON.parse(await readFile(cursorFile, 'utf8')).cursor, 'c2');
  console.log('PASS: real provider HTTP cursor ack/restart/replay/account binding + bounded body/timeout');
} finally {
  await stop(); upstream.closeAllConnections(); await new Promise(resolve => upstream.close(resolve));
  await rm(folder, { recursive: true, force: true });
}
