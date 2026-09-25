$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$tmpRoot = Join-Path $workspace 'tmp\tests\clawbot-ilink-provider-long-poll'
$mockPath = Join-Path $tmpRoot 'mock-ilink-long-poll.mjs'
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

@'
import http from 'node:http';

function send(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {'content-type': 'application/json; charset=utf-8'});
  res.end(body);
}

const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1:18792');
  if (req.method === 'GET' && url.pathname === '/ilink/bot/get_bot_qrcode') {
    send(res, 200, { qrcode: 'mock-qr', qrcode_img_content: 'data:image/png;base64,bW9jay1xcg==' });
    return;
  }
  if (req.method === 'GET' && url.pathname === '/ilink/bot/get_qrcode_status') {
    send(res, 200, {
      status: 'confirmed',
      bot_token: 'mock-bot-token',
      ilink_bot_id: 'mock-bot',
      ilink_user_id: 'mock-user',
      baseurl: 'http://127.0.0.1:18792'
    });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/msg/notifystart') {
    send(res, 200, { ret: 0 });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/getupdates') {
    await wait(9000);
    send(res, 200, {
      ret: 0,
      get_updates_buf: 'cursor-after-long-poll',
      longpolling_timeout_ms: 35000,
      msgs: [{
        from_user_id: 'peer-delayed',
        client_id: 'msg-delayed',
        context_token: 'ctx-delayed',
        item_list: [{ type: 1, text_item: { text: 'delayed message' } }]
      }]
    });
    return;
  }
  send(res, 404, { error: 'not found' });
});

server.listen(18792, '127.0.0.1', () => {
  console.log('mock long-poll iLink upstream listening');
});
'@ | Set-Content -LiteralPath $mockPath -Encoding UTF8

function Wait-HttpOk {
  param([string]$Url, [int]$TimeoutSeconds = 15)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  do {
    try {
      Invoke-WebRequest -UseBasicParsing -Uri $Url -TimeoutSec 2 | Out-Null
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
  Wait-HttpOk "http://127.0.0.1:$mockPort/ilink/bot/get_bot_qrcode?bot_type=3"

  $env:CLAWBOT_ILINK_BIND_PORT = [string]$providerPort
  $env:CLAWBOT_ILINK_BASE_URL = "http://127.0.0.1:$mockPort"
  $providerProcess = Start-Process -FilePath 'node' -ArgumentList @(Join-Path $PSScriptRoot 'clawbot-ilink-provider.mjs') -WorkingDirectory $workspace -PassThru -WindowStyle Hidden -RedirectStandardOutput $providerStdout -RedirectStandardError $providerStderr
  Wait-HttpOk "http://127.0.0.1:$providerPort/health"

  Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/login/refresh" -Method Post -ContentType 'application/json' -Body '{"generation":1,"now_ms":1}' | Out-Null
  Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/login/refresh" -Method Post -ContentType 'application/json' -Body '{"generation":2,"now_ms":2}' | Out-Null

  $watch = [System.Diagnostics.Stopwatch]::StartNew()
  $updates = Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/updates?since_ms=3" -Method Get -TimeoutSec 45
  $watch.Stop()

  if ($watch.ElapsedMilliseconds -lt 8500 -or $watch.ElapsedMilliseconds -gt 20000) {
    throw "provider should preserve the official long poll instead of aborting at 8s; elapsed=$($watch.ElapsedMilliseconds)ms"
  }
  if (@($updates.updates).Count -ne 1 -or [string]$updates.updates[0].message.external_msg_id -ne 'msg-delayed') {
    throw "official long poll message was not delivered after 9s"
  }
  Write-Output 'PASS clawbot-ilink-provider-long-poll'
} finally {
  Remove-Item Env:CLAWBOT_ILINK_BIND_PORT -ErrorAction SilentlyContinue
  Remove-Item Env:CLAWBOT_ILINK_BASE_URL -ErrorAction SilentlyContinue
  if ($providerProcess -and $providerProcess.HasExited -and (Test-Path -LiteralPath $providerStderr)) {
    $stderr = Get-Content -LiteralPath $providerStderr -Raw
    if ($stderr.Trim()) { Write-Host "provider stderr: $stderr" }
  }
  if ($mockProcess -and $mockProcess.HasExited -and (Test-Path -LiteralPath $mockStderr)) {
    $stderr = Get-Content -LiteralPath $mockStderr -Raw
    if ($stderr.Trim()) { Write-Host "mock stderr: $stderr" }
  }
  if ($providerProcess -and -not $providerProcess.HasExited) { Stop-Process -Id $providerProcess.Id -Force }
  if ($mockProcess -and -not $mockProcess.HasExited) { Stop-Process -Id $mockProcess.Id -Force }
}
