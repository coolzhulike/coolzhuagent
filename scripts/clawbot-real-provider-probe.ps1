param(
  [string]$ProviderUrl,
  [string]$ProviderToken,
  [string]$AccountId,
  [string]$PeerId,
  [string]$Text,
  [string]$OutputPath,
  [switch]$MockProvider,
  [switch]$UseBuiltExe,
  [switch]$AllowFailedProbe
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$defaultOutput = Join-Path $repo 'tmp\clawbot-real-provider-probe\probe-report.json'
$resolvedOutput = if ($OutputPath) {
  if ([System.IO.Path]::IsPathRooted($OutputPath)) { $OutputPath } else { Join-Path $repo $OutputPath }
} else {
  $defaultOutput
}
$outputDir = Split-Path -Parent $resolvedOutput
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

function Set-ProbeEnv {
  param([string]$Name, [string]$Value)
  if ($null -ne $Value -and $Value.Trim().Length -gt 0) {
    [Environment]::SetEnvironmentVariable($Name, $Value, 'Process')
  }
}

function Clear-ProbeEnv {
  param([string]$Name)
  [Environment]::SetEnvironmentVariable($Name, $null, 'Process')
}

function Mask-Secret {
  param([string]$Value)
  if (-not $Value) { return '<unset>' }
  return "SET(len=$($Value.Length))"
}

function Extract-FirstJsonObject {
  param([string]$Text)
  $start = $Text.IndexOf('{')
  if ($start -lt 0) {
    return $null
  }
  $depth = 0
  $inString = $false
  $escape = $false
  for ($i = $start; $i -lt $Text.Length; $i++) {
    $ch = $Text[$i]
    if ($escape) {
      $escape = $false
      continue
    }
    if ($ch -eq '\') {
      if ($inString) { $escape = $true }
      continue
    }
    if ($ch -eq '"') {
      $inString = -not $inString
      continue
    }
    if ($inString) {
      continue
    }
    if ($ch -eq '{') {
      $depth++
    } elseif ($ch -eq '}') {
      $depth--
      if ($depth -eq 0) {
        return $Text.Substring($start, $i - $start + 1)
      }
    }
  }
  return $null
}

$previous = @{}
foreach ($name in @(
  'COOLZHU_CLAWBOT_PROVIDER_KIND',
  'COOLZHU_CLAWBOT_PROVIDER_URL',
  'COOLZHU_CLAWBOT_PROVIDER_TOKEN',
  'COOLZHU_CLAWBOT_ACCOUNT_ID',
  'COOLZHU_CLAWBOT_PROBE_PEER_ID',
  'COOLZHU_CLAWBOT_PROBE_TEXT',
  'COOLZHU_CLAWBOT_MOCK_CONFIGURED',
  'COOLZHU_CLAWBOT_MOCK_ONLINE',
  'COOLZHU_CLAWBOT_MOCK_INBOUND_TEXT'
)) {
  $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}

try {
  if ($MockProvider) {
    Set-ProbeEnv 'COOLZHU_CLAWBOT_PROVIDER_KIND' 'mock'
    Set-ProbeEnv 'COOLZHU_CLAWBOT_MOCK_CONFIGURED' 'true'
    Set-ProbeEnv 'COOLZHU_CLAWBOT_MOCK_ONLINE' 'true'
    Set-ProbeEnv 'COOLZHU_CLAWBOT_MOCK_INBOUND_TEXT' 'mock probe inbound'
  } else {
    Set-ProbeEnv 'COOLZHU_CLAWBOT_PROVIDER_KIND' 'http'
    Set-ProbeEnv 'COOLZHU_CLAWBOT_PROVIDER_URL' $ProviderUrl
  }

  Set-ProbeEnv 'COOLZHU_CLAWBOT_PROVIDER_TOKEN' $ProviderToken
  Set-ProbeEnv 'COOLZHU_CLAWBOT_ACCOUNT_ID' $AccountId
  Set-ProbeEnv 'COOLZHU_CLAWBOT_PROBE_PEER_ID' $PeerId
  Set-ProbeEnv 'COOLZHU_CLAWBOT_PROBE_TEXT' $Text

  if (-not $MockProvider -and -not [Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_PROVIDER_URL', 'Process')) {
    throw '真实 ClawBot provider 测试需要 -ProviderUrl 或 COOLZHU_CLAWBOT_PROVIDER_URL'
  }

  Write-Host "ClawBot provider kind: $([Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_PROVIDER_KIND', 'Process'))"
  Write-Host "ClawBot provider url: $([Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_PROVIDER_URL', 'Process'))"
  Write-Host "ClawBot provider token: $(Mask-Secret ([Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_PROVIDER_TOKEN', 'Process')))"
  Write-Host "ClawBot account id: $([Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_ACCOUNT_ID', 'Process'))"
  Write-Host "ClawBot probe peer id: $([Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_PROBE_PEER_ID', 'Process'))"
  Write-Host "ClawBot probe text: $(Mask-Secret ([Environment]::GetEnvironmentVariable('COOLZHU_CLAWBOT_PROBE_TEXT', 'Process')))"

  Push-Location $repo
  try {
    $probeOutput = $null
    $exitCode = 0
    $previousErrorActionPreference = $ErrorActionPreference
    if ($UseBuiltExe) {
      $exe = Join-Path $repo 'target\debug\coolzhu-clawbot-sidecar.exe'
      if (-not (Test-Path -LiteralPath $exe)) {
        $exe = Join-Path $repo 'modules\gui-web\target\debug\coolzhu-clawbot-sidecar.exe'
      }
      if (-not (Test-Path -LiteralPath $exe)) {
        throw '未找到已构建的 coolzhu-clawbot-sidecar.exe，请先构建或去掉 -UseBuiltExe'
      }
      $ErrorActionPreference = 'Continue'
      $probeOutput = & $exe --probe-provider 2>&1
      $exitCode = $LASTEXITCODE
    } else {
      $ErrorActionPreference = 'Continue'
      $probeOutput = & cargo run -p coolzhu-clawbot-sidecar --offline -- --probe-provider 2>&1
      $exitCode = $LASTEXITCODE
    }
    $ErrorActionPreference = $previousErrorActionPreference
  } finally {
    $ErrorActionPreference = $previousErrorActionPreference
    Pop-Location
  }

  $probeText = ($probeOutput | ForEach-Object { [string]$_ }) -join [Environment]::NewLine
  $jsonText = Extract-FirstJsonObject $probeText
  if (-not $jsonText) {
    throw "probe output did not contain JSON. exit=$exitCode output=$probeText"
  }
  Set-Content -LiteralPath $resolvedOutput -Value $jsonText -Encoding UTF8

  if ($ProviderToken -and ((Get-Content -LiteralPath $resolvedOutput -Raw) -like "*$ProviderToken*")) {
    throw 'probe report leaked provider token'
  }

  $report = Get-Content -LiteralPath $resolvedOutput -Raw | ConvertFrom-Json
  Write-Host "probe report: $resolvedOutput"
  Write-Host "probe ok: $($report.ok)"
  foreach ($step in @($report.steps)) {
    $status = if ($step.ok) { 'ok' } else { 'fail' }
    $stepErrorText = if ($step.error) { " error=$($step.error)" } else { '' }
    Write-Host "step $($step.name): $status$stepErrorText"
  }

  if ($exitCode -ne 0 -and -not $AllowFailedProbe) {
    throw "probe failed with exit code $exitCode; report=$resolvedOutput"
  }
} finally {
  foreach ($entry in $previous.GetEnumerator()) {
    if ($null -eq $entry.Value) {
      Clear-ProbeEnv $entry.Key
    } else {
      Set-ProbeEnv $entry.Key $entry.Value
    }
  }
}
