$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$probeScript = Join-Path $PSScriptRoot 'clawbot-real-provider-probe.ps1'
$outputDir = Join-Path $workspace 'tmp\tests\clawbot-real-provider-probe'
$outputPath = Join-Path $outputDir 'probe-report.json'

if (-not (Test-Path -LiteralPath $probeScript)) {
  throw "probe script not found: $probeScript"
}

if (Test-Path -LiteralPath $outputDir) {
  Remove-Item -LiteralPath $outputDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

& $probeScript `
  -MockProvider `
  -ProviderToken 'fixture-token-should-not-leak' `
  -OutputPath $outputPath | Out-Null

if (-not (Test-Path -LiteralPath $outputPath)) {
  throw "probe report was not written: $outputPath"
}

$raw = Get-Content -LiteralPath $outputPath -Raw
if ($raw -like '*fixture-token-should-not-leak*') {
  throw 'probe report leaked provider token'
}

$report = $raw | ConvertFrom-Json
if (-not $report.ok) {
  throw "mock probe did not pass: $raw"
}

$stepNames = @($report.steps | ForEach-Object { [string]$_.name })
foreach ($expected in @('health', 'login_refresh', 'updates')) {
  if ($stepNames -notcontains $expected) {
    throw "probe report missing step: $expected"
  }
}

Write-Output 'PASS clawbot-real-provider-probe'
