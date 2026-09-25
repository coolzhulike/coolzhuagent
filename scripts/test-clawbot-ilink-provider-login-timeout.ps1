$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$tmpRoot = Join-Path $workspace 'tmp\tests\clawbot-ilink-provider-login-timeout'
$mockPath = Join-Path $tmpRoot 'mock-ilink-login-timeout.mjs'
$mockStdout = Join-Path $tmpRoot 'mock.stdout.log'
$mockStderr = Join-Path $tmpRoot 'mock.stderr.log'
$providerStdout = Join-Path $tmpRoot 'provider.stdout.log'
$providerStderr = Join-Path $tmpRoot 'provider.stderr.log'
$mockPort = 18794
$providerPort = 18795

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
  const url = new URL(req.url, 'http://127.0.0.1:18794');
  if (req.method === 'GET' && url.pathname === '/ilink/bot/get_bot_qrcode') {
    await wait(1200);
    send(res, 200, { qrcode: 'mock-qr', qrcode_img_content: 'data:image/png;base64,bW9jay1xcg==' });
    return;
  }
  send(res, 404, { error: 'not found' });
});

server.listen(18794, '127.0.0.1', () => {
  console.log('mock login-timeout iLink upstream listening');
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
  $env:CLAWBOT_ILINK_REQUEST_TIMEOUT_MS = '300'
  $providerProcess = Start-Process -FilePath 'node' -ArgumentList @(Join-Path $PSScriptRoot 'clawbot-ilink-provider.mjs') -WorkingDirectory $workspace -PassThru -WindowStyle Hidden -RedirectStandardOutput $providerStdout -RedirectStandardError $providerStderr
  Wait-HttpOk "http://127.0.0.1:$providerPort/health"

  $watch = [System.Diagnostics.Stopwatch]::StartNew()
  $failedFast = $false
  try {
    Invoke-RestMethod -Uri "http://127.0.0.1:$providerPort/login/refresh" -Method Post -ContentType 'application/json' -Body '{"generation":1,"now_ms":1}' -TimeoutSec 5 | Out-Null
  } catch {
    $failedFast = $true
  }
  $watch.Stop()

  if (-not $failedFast) {
    throw '登录上游超时时 provider 应返回错误，而不是伪造二维码成功'
  }
  if ($watch.ElapsedMilliseconds -gt 900) {
    throw "provider login refresh should fail before sidecar timeout; elapsed=$($watch.ElapsedMilliseconds)ms"
  }
  Write-Output 'PASS clawbot-ilink-provider-login-timeout'
} finally {
  Remove-Item Env:CLAWBOT_ILINK_BIND_PORT -ErrorAction SilentlyContinue
  Remove-Item Env:CLAWBOT_ILINK_BASE_URL -ErrorAction SilentlyContinue
  Remove-Item Env:CLAWBOT_ILINK_REQUEST_TIMEOUT_MS -ErrorAction SilentlyContinue
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
