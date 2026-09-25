$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$tmpRoot = Join-Path $workspace 'tmp\tests\clawbot-ilink-provider-qr-link'
$testPath = Join-Path $tmpRoot 'qr-link-helper-test.mjs'

if (Test-Path -LiteralPath $tmpRoot) {
  Remove-Item -LiteralPath $tmpRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

@'
import fs from 'node:fs';
import path from 'node:path';

const providerPath = path.join(process.argv[2], 'scripts', 'clawbot-ilink-provider.mjs');
const source = fs.readFileSync(providerPath, 'utf8');
const start = source.indexOf('async function imageUrlToDataUrl');
const end = source.indexOf('async function refreshLogin');
if (start < 0 || end < start) {
  throw new Error('未找到 imageUrlToDataUrl 函数块');
}
const imageUrlToDataUrl = new Function(`${source.slice(start, end)}; return imageUrlToDataUrl;`)();

let fetchCalls = 0;
globalThis.fetch = async () => {
  fetchCalls += 1;
  throw new Error('LiteApp 扫码页 URL 不应由 provider 拉取');
};

const qrUrl = 'https://liteapp.weixin.qq.com/q/mock-login?qrcode=abc123&bot_type=3';
const result = await imageUrlToDataUrl(qrUrl);
if (result !== qrUrl) {
  throw new Error(`LiteApp URL 应原样返回，实际：${result}`);
}
if (fetchCalls !== 0) {
  throw new Error(`LiteApp URL 不应触发 fetch，实际调用：${fetchCalls}`);
}
console.log('PASS clawbot-ilink-provider-qr-link');
'@ | Set-Content -LiteralPath $testPath -Encoding UTF8

node $testPath $workspace
