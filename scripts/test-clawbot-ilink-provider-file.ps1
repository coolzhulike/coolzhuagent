$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$tmpRoot = Join-Path $workspace 'tmp\tests\clawbot-ilink-provider-file'
$mockPath = Join-Path $tmpRoot 'mock-ilink-file-upstream.mjs'
$samplePath = Join-Path $tmpRoot 'Cargo.toml'
$mockStdout = Join-Path $tmpRoot 'mock.stdout.log'
$mockStderr = Join-Path $tmpRoot 'mock.stderr.log'
$providerStdout = Join-Path $tmpRoot 'provider.stdout.log'
$providerStderr = Join-Path $tmpRoot 'provider.stderr.log'
$mockPort = 18792
$providerPort = 18793

if (Test-Path -LiteralPath $tmpRoot) {
  Remove-Item -LiteralPath $tmpRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null
Copy-Item -LiteralPath (Join-Path $workspace 'Cargo.toml') -Destination $samplePath

@'
import http from 'node:http';

let uploadRequest = null;
let uploadBytes = 0;
let sentMessage = null;

function json(res, status, payload, headers = {}) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {'content-type': 'application/json; charset=utf-8', ...headers});
  res.end(body);
}

function readBuffer(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', chunk => chunks.push(chunk));
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}

async function readJson(req) {
  const raw = (await readBuffer(req)).toString('utf8');
  return raw ? JSON.parse(raw) : {};
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1:18792');
  if (req.method === 'GET' && url.pathname === '/test/capture') {
    json(res, 200, {uploadRequest, uploadBytes, sentMessage});
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/msg/notifystart') {
    json(res, 200, {ret: 0});
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/getuploadurl') {
    uploadRequest = await readJson(req);
    json(res, 200, {
      ret: 0,
      upload_full_url: 'http://127.0.0.1:18792/cdn/full-upload?ticket=mock-full-url',
    });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/cdn/full-upload') {
    const encrypted = await readBuffer(req);
    uploadBytes = encrypted.length;
    res.writeHead(200, {'x-encrypted-param': 'mock-download-param'});
    res.end('');
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/sendmessage') {
    sentMessage = await readJson(req);
    json(res, 200, {ret: 0, msg_id: 'file-sent-1'});
    return;
  }
  json(res, 404, {error: 'not found', path: url.pathname});
});

server.listen(18792, '127.0.0.1', () => console.log('mock iLink file upstream listening'));
'@ | Set-Content -LiteralPath $mockPath -Encoding UTF8

function Wait-HttpOk {
  param([string]$Url, [hashtable]$Headers = @{}, [int]$TimeoutSeconds = 15)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  do {
    try {
      Invoke-WebRequest -UseBasicParsing -Uri $Url -Headers $Headers -TimeoutSec 2 | Out-Null
      return
    } catch {
      Start-Sleep -Milliseconds 250
    }
  } while ((Get-Date) -lt $deadline)
  throw "timeout waiting for $Url"
}

$mockProcess = $null
$providerProcess = $null
try {
  $mockProcess = Start-Process -FilePath 'node' -ArgumentList @($mockPath) -PassThru -WindowStyle Hidden -RedirectStandardOutput $mockStdout -RedirectStandardError $mockStderr
  Wait-HttpOk "http://127.0.0.1:$mockPort/test/capture"

  $env:CLAWBOT_ILINK_BIND_PORT = [string]$providerPort
  $env:CLAWBOT_ILINK_BASE_URL = "http://127.0.0.1:$mockPort"
  $env:CLAWBOT_ILINK_CDN_BASE_URL = "http://127.0.0.1:$mockPort/c2c"
  $env:CLAWBOT_ILINK_ACCOUNT_ID = 'wx-stable'
  $env:CLAWBOT_ILINK_BOT_TOKEN = 'mock-bot-token'
  $env:CLAWBOT_ILINK_PROVIDER_TOKEN = 'provider-file-secret'
  $providerProcess = Start-Process -FilePath 'node' -ArgumentList @(Join-Path $PSScriptRoot 'clawbot-ilink-provider.mjs') -WorkingDirectory $workspace -PassThru -WindowStyle Hidden -RedirectStandardOutput $providerStdout -RedirectStandardError $providerStderr
  $providerHeaders = @{ Authorization = 'Bearer provider-file-secret' }
  Wait-HttpOk "http://127.0.0.1:$providerPort/health" -Headers $providerHeaders

  Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/login/refresh" -Method Post -Headers $providerHeaders -ContentType 'application/json' -Body '{"generation":1,"now_ms":1}' | Out-Null
  $sample = Get-Item -LiteralPath $samplePath
  $payload = @{
    account_id = 'wx-stable'
    peer_id = 'peer-file'
    context_token = 'ctx-file'
    source_external_msg_id = 'msg-file-1'
    body = '发送文件：Cargo.toml'
    local_path = $sample.FullName
    display_name = 'Cargo.toml'
    mime = 'text/plain'
    size_bytes = [int64]$sample.Length
    checksum = 'fnv64:test'
  } | ConvertTo-Json -Compress
  $receipt = Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/send_file" -Method Post -Headers $providerHeaders -ContentType 'application/json' -Body $payload
  if ([string]$receipt.provider_message_id -ne 'file-sent-1') {
    throw "unexpected file receipt: $($receipt | ConvertTo-Json -Compress)"
  }

  $capture = Invoke-RestMethod -Uri "http://127.0.0.1:$mockPort/test/capture" -TimeoutSec 5
  $expectedMd5 = (Get-FileHash -LiteralPath $samplePath -Algorithm MD5).Hash.ToLowerInvariant()
  if ([int]$capture.uploadRequest.media_type -ne 3) { throw 'getuploadurl media_type must be FILE(3)' }
  if ([int64]$capture.uploadRequest.rawsize -ne [int64]$sample.Length) { throw 'rawsize mismatch' }
  if ([string]$capture.uploadRequest.rawfilemd5 -ne $expectedMd5) { throw 'rawfilemd5 mismatch' }
  if ([int64]$capture.uploadRequest.filesize % 16 -ne 0) { throw 'ciphertext filesize is not AES block aligned' }
  if ([int64]$capture.uploadBytes -ne [int64]$capture.uploadRequest.filesize) { throw 'uploaded ciphertext length mismatch' }
  $fileItem = $capture.sentMessage.msg.item_list[0]
  if ([int]$fileItem.type -ne 4) { throw 'sendmessage item type must be FILE(4)' }
  if ([string]$fileItem.file_item.file_name -ne 'Cargo.toml') { throw 'file name mismatch' }
  if ([string]$fileItem.file_item.len -ne [string]$sample.Length) { throw 'file length mismatch' }
  if ([string]$fileItem.file_item.media.encrypt_query_param -ne 'mock-download-param') { throw 'download param mismatch' }
  if ([string]::IsNullOrWhiteSpace([string]$fileItem.file_item.media.aes_key)) { throw 'aes_key missing' }
  Write-Output 'PASS clawbot-ilink-provider-file'
} finally {
  foreach ($name in @('CLAWBOT_ILINK_BIND_PORT','CLAWBOT_ILINK_BASE_URL','CLAWBOT_ILINK_CDN_BASE_URL','CLAWBOT_ILINK_ACCOUNT_ID','CLAWBOT_ILINK_BOT_TOKEN','CLAWBOT_ILINK_PROVIDER_TOKEN')) {
    Remove-Item "Env:$name" -ErrorAction SilentlyContinue
  }
  if ($providerProcess -and -not $providerProcess.HasExited) { Stop-Process -Id $providerProcess.Id -Force }
  if ($mockProcess -and -not $mockProcess.HasExited) { Stop-Process -Id $mockProcess.Id -Force }
  if ($providerProcess -and $providerProcess.HasExited -and (Test-Path -LiteralPath $providerStderr)) {
    $stderr = [string](Get-Content -LiteralPath $providerStderr -Raw)
    if (-not [string]::IsNullOrWhiteSpace($stderr)) { Write-Host "provider stderr: $stderr" }
  }
  if ($mockProcess -and $mockProcess.HasExited -and (Test-Path -LiteralPath $mockStderr)) {
    $stderr = [string](Get-Content -LiteralPath $mockStderr -Raw)
    if (-not [string]::IsNullOrWhiteSpace($stderr)) { Write-Host "mock stderr: $stderr" }
  }
}
