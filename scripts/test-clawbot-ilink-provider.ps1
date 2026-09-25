$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$tmpRoot = Join-Path $workspace 'tmp\tests\clawbot-ilink-provider'
$mockPath = Join-Path $tmpRoot 'mock-ilink-upstream.mjs'
$reportPath = Join-Path $tmpRoot 'probe-report.json'
$mockStdout = Join-Path $tmpRoot 'mock.stdout.log'
$mockStderr = Join-Path $tmpRoot 'mock.stderr.log'
$providerStdout = Join-Path $tmpRoot 'provider.stdout.log'
$providerStderr = Join-Path $tmpRoot 'provider.stderr.log'
$mockPort = 18790
$providerPort = 18791

if (Test-Path -LiteralPath $tmpRoot) {
  Remove-Item -LiteralPath $tmpRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

@'
import http from 'node:http';

let startCount = 0;
let stopCount = 0;

function send(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {'content-type': 'application/json; charset=utf-8'});
  res.end(body);
}

function read(req) {
  return new Promise(resolve => {
    const chunks = [];
    req.on('data', chunk => chunks.push(chunk));
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8');
      resolve(raw ? JSON.parse(raw) : {});
    });
  });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1:18790');
  if (req.method === 'GET' && url.pathname === '/test/capture') {
    send(res, 200, { startCount, stopCount });
    return;
  }
  if (req.method === 'GET' && url.pathname === '/ilink/bot/get_bot_qrcode') {
    send(res, 200, {
      qrcode: 'mock-qr',
      qrcode_img_content: 'data:image/png;base64,bW9jay1xcg=='
    });
    return;
  }
  if (req.method === 'GET' && url.pathname === '/ilink/bot/get_qrcode_status') {
    send(res, 200, {
      status: 'confirmed',
      bot_token: 'mock-bot-token',
      ilink_bot_id: 'mock-bot',
      ilink_user_id: 'mock-user',
      baseurl: 'http://127.0.0.1:18790'
    });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/getupdates') {
    const body = await read(req);
    if (startCount < 1) {
      send(res, 409, { ret: 1, errmsg: 'session not started' });
      return;
    }
    if (req.headers['ilink-app-id'] !== 'bot'
        || req.headers['ilink-app-clientversion'] !== '132102'
        || body?.base_info?.channel_version !== '2.4.6'
        || body?.base_info?.bot_agent !== 'CoolzhuAgent/0.2.0') {
      send(res, 400, { ret: 1, errmsg: 'official protocol headers/base_info missing' });
      return;
    }
    send(res, 200, {
      ret: 0,
      get_updates_buf: 'cursor-1',
      msgs: [{
        from_user_id: 'peer-http',
        from_user_name: '测试联系人',
        client_id: 'msg-1',
        context_token: 'ctx-1',
        item_list: [{ type: 1, text_item: { text: '来自 mock iLink' } }]
      }]
    });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/msg/notifystart') {
    const body = await read(req);
    if (req.headers['ilink-app-id'] !== 'bot'
        || req.headers['ilink-app-clientversion'] !== '132102'
        || body?.base_info?.channel_version !== '2.4.6'
        || body?.base_info?.bot_agent !== 'CoolzhuAgent/0.2.0') {
      send(res, 400, { ret: 1, errmsg: 'official start headers/base_info missing' });
      return;
    }
    startCount += 1;
    send(res, 200, { ret: 0 });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/msg/notifystop') {
    stopCount += 1;
    send(res, 200, { ret: 0 });
    return;
  }
  if (req.method === 'POST' && url.pathname === '/ilink/bot/sendmessage') {
    const body = await read(req);
    if (!body.msg || body.msg.context_token !== 'ctx-1') {
      send(res, 400, { ret: 1, errmsg: 'missing context' });
      return;
    }
    send(res, 200, { ret: 0, msg_id: 'sent-1' });
    return;
  }
  send(res, 404, { error: 'not found' });
});

server.listen(18790, '127.0.0.1', () => {
  console.log('mock iLink upstream listening');
});
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
  Wait-HttpOk "http://127.0.0.1:$mockPort/ilink/bot/get_bot_qrcode?bot_type=3"

  $env:CLAWBOT_ILINK_BIND_PORT = [string]$providerPort
  $env:CLAWBOT_ILINK_BASE_URL = "http://127.0.0.1:$mockPort"
  $env:CLAWBOT_ILINK_ACCOUNT_ID = 'wx-stable'
  $env:CLAWBOT_ILINK_PROVIDER_TOKEN = 'provider-secret-should-not-leak'
  $providerProcess = Start-Process -FilePath 'node' -ArgumentList @(Join-Path $PSScriptRoot 'clawbot-ilink-provider.mjs') -WorkingDirectory $workspace -PassThru -WindowStyle Hidden -RedirectStandardOutput $providerStdout -RedirectStandardError $providerStderr
  Wait-HttpOk "http://127.0.0.1:$providerPort/health" -Headers @{ Authorization = 'Bearer provider-secret-should-not-leak' }

  & (Join-Path $PSScriptRoot 'clawbot-real-provider-probe.ps1') `
    -ProviderUrl "http://127.0.0.1:$providerPort" `
    -ProviderToken 'provider-secret-should-not-leak' `
    -AccountId 'wx-stable' `
    -PeerId 'peer-http' `
    -Text 'coolzhu probe' `
    -UseBuiltExe `
    -OutputPath $reportPath | Out-Null

  $raw = Get-Content -LiteralPath $reportPath -Raw
  if ($raw -like '*provider-secret-should-not-leak*') {
    throw 'probe report leaked provider token'
  }
  $report = $raw | ConvertFrom-Json
  if (-not $report.ok) {
    throw "iLink provider probe failed: $raw"
  }
  if ([string]$report.send_provider_message_id -ne 'sent-1') {
    throw "unexpected send receipt: $($report.send_provider_message_id)"
  }
  $updates = Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/updates?since_ms=1" -Headers @{ Authorization = 'Bearer provider-secret-should-not-leak' } -TimeoutSec 5
  if ([string]$updates.updates[0].message.account_id -ne 'wx-stable') {
    throw "provider inbound account_id must remain the configured stable alias; got $($updates.updates[0].message.account_id)"
  }
  Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/login/logout" -Method Post -Headers @{ Authorization = 'Bearer provider-secret-should-not-leak' } -ContentType 'application/json' -Body '{}' | Out-Null
  $capture = Invoke-RestMethod -Uri "http://127.0.0.1:$mockPort/test/capture" -TimeoutSec 5
  if ([int]$capture.startCount -ne 1 -or [int]$capture.stopCount -ne 1) {
    throw "expected one notify start/stop, got start=$($capture.startCount) stop=$($capture.stopCount)"
  }
  Write-Output 'PASS clawbot-ilink-provider'
} finally {
  if ($providerProcess -and $providerProcess.HasExited -and (Test-Path -LiteralPath $providerStderr)) {
    $stderr = Get-Content -LiteralPath $providerStderr -Raw
    if ($stderr.Trim()) { Write-Host "provider stderr: $stderr" }
  }
  if ($mockProcess -and $mockProcess.HasExited -and (Test-Path -LiteralPath $mockStderr)) {
    $stderr = Get-Content -LiteralPath $mockStderr -Raw
    if ($stderr.Trim()) { Write-Host "mock stderr: $stderr" }
  }
  Remove-Item Env:CLAWBOT_ILINK_BIND_PORT -ErrorAction SilentlyContinue
  Remove-Item Env:CLAWBOT_ILINK_BASE_URL -ErrorAction SilentlyContinue
  Remove-Item Env:CLAWBOT_ILINK_ACCOUNT_ID -ErrorAction SilentlyContinue
  Remove-Item Env:CLAWBOT_ILINK_PROVIDER_TOKEN -ErrorAction SilentlyContinue
  if ($providerProcess -and -not $providerProcess.HasExited) { Stop-Process -Id $providerProcess.Id -Force }
  if ($mockProcess -and -not $mockProcess.HasExited) { Stop-Process -Id $mockProcess.Id -Force }
}
