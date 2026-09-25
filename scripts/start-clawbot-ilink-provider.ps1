param(
  [string]$BindHost = '127.0.0.1',
  [int]$Port = 8790,
  [string]$IlinkBaseUrl = 'https://ilinkai.weixin.qq.com',
  [string]$ProviderToken,
  [string]$AccountId = 'wx-ilink',
  [string]$BotToken,
  [string]$ChannelVersion = '1'
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$provider = Join-Path $PSScriptRoot 'clawbot-ilink-provider.mjs'
if (-not (Test-Path -LiteralPath $provider)) {
  throw "provider script not found: $provider"
}

$env:CLAWBOT_ILINK_BIND_HOST = $BindHost
$env:CLAWBOT_ILINK_BIND_PORT = [string]$Port
$env:CLAWBOT_ILINK_BASE_URL = $IlinkBaseUrl
$env:CLAWBOT_ILINK_ACCOUNT_ID = $AccountId
$env:CLAWBOT_ILINK_CHANNEL_VERSION = $ChannelVersion
if ($ProviderToken) { $env:CLAWBOT_ILINK_PROVIDER_TOKEN = $ProviderToken }
if ($BotToken) { $env:CLAWBOT_ILINK_BOT_TOKEN = $BotToken }

Write-Host "Starting coolzhu iLink provider..."
Write-Host "bind: http://$($BindHost):$Port"
Write-Host "upstream: $IlinkBaseUrl"
Write-Host "provider token: $(if ($ProviderToken) { "SET(len=$($ProviderToken.Length))" } else { '<unset>' })"
Write-Host ''
Write-Host '首次真实登录：保持本窗口运行，然后另开 PowerShell 执行：'
Write-Host "  powershell -ExecutionPolicy Bypass -File scripts/clawbot-real-provider-probe.ps1 -ProviderUrl 'http://$($BindHost):$Port' -ProviderToken '<如果设置了ProviderToken则填同一个值>'"
Write-Host ''

Push-Location $repo
try {
  & node $provider
  exit $LASTEXITCODE
} finally {
  Pop-Location
}
