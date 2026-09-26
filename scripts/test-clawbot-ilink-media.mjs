import assert from 'node:assert/strict';
import http from 'node:http';
import { createCipheriv, createHash } from 'node:crypto';
import { receiveIlinkImages } from './lib/clawbot-ilink-media.mjs';

const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a7x8AAAAASUVORK5CYII=', 'base64');
const key = Buffer.from('0123456789abcdef');
const cipher = createCipheriv('aes-128-ecb', key, null);
const encrypted = Buffer.concat([cipher.update(png), cipher.final()]);
let requests = 0;
const server = http.createServer((req, res) => {
  requests++;
  if (req.url.includes('large')) { res.writeHead(200, { 'content-length': 99_000_000 }); res.end(); return; }
  res.writeHead(200, { 'content-type': 'application/octet-stream' }); res.end(encrypted);
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}/c2c`;
const input = (aesKey, aeskey) => ({ item_list: [{ type: 2, image_item: { aeskey, media: { encrypt_query_param: 'fixture', aes_key: aesKey } } }] });
try {
  for (const message of [input(key.toString('base64')), input(Buffer.from(key.toString('hex')).toString('base64')), input('wrong-fallback', key.toString('hex'))]) {
    const [actual] = await receiveIlinkImages(message, { cdnBaseUrl: base });
    assert.equal(actual.media_id, `sha256:${createHash('sha256').update(png).digest('hex')}`);
    assert.deepEqual(Buffer.from(actual.content_base64, 'base64'), png);
    assert.equal(actual.mime_type, 'image/png');
  }
  const hostile = input(key.toString('base64')); hostile.item_list[0].image_item.media.full_url = 'http://127.0.0.1:1/private';
  const before = requests;
  assert.ok((await receiveIlinkImages(hostile, { cdnBaseUrl: base }))[0].error);
  assert.ok((await receiveIlinkImages(input(key.toString('base64')), { cdnBaseUrl: base, maxTotalBytes: 0 }))[0].error);
  assert.equal(requests, before, '越权URL和耗尽预算必须在发出请求前拒绝');
  const oversize = input(key.toString('base64')); oversize.item_list[0].image_item.media.encrypt_query_param = 'large';
  assert.ok((await receiveIlinkImages(oversize, { cdnBaseUrl: base }))[0].error);
  assert.ok((await receiveIlinkImages(input(Buffer.alloc(16).toString('base64')), { cdnBaseUrl: base }))[0].error);
  console.log('PASS: real HTTP encrypted image + 3 key encodings + origin/size/budget/decrypt failures');
} finally { await new Promise((resolve) => server.close(resolve)); }
