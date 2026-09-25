param(
  [string]$BaseUrl = "http://127.0.0.1:8001/v1",
  [string]$Model = "qwen2.5-vl-3b",
  [string]$RunDir = "",
  [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Continue"

function Find-RepoRoot {
  $dir = Split-Path -Parent $PSCommandPath
  while ($dir) {
    if (Test-Path (Join-Path $dir "Cargo.toml")) {
      return $dir
    }
    $parent = Split-Path -Parent $dir
    if ($parent -eq $dir) {
      break
    }
    $dir = $parent
  }
  return (Resolve-Path ".").Path
}

function ConvertTo-HealthUrl {
  param([string]$BaseUrl)
  $trimmed = $BaseUrl.Trim().TrimEnd("/")
  if ($trimmed.EndsWith("/v1")) {
    $trimmed = $trimmed.Substring(0, $trimmed.Length - 3)
  }
  return "$trimmed/health"
}

$repoRoot = Find-RepoRoot
if ([string]::IsNullOrWhiteSpace($RunDir)) {
  $RunDir = Join-Path $repoRoot ("tmp\logs\local-vlm-check-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
New-Item -ItemType Directory -Force -Path $RunDir | Out-Null

$BaseUrl = $BaseUrl.TrimEnd("/")
$healthUrl = ConvertTo-HealthUrl $BaseUrl
$summary = Join-Path $RunDir "local-vlm-check-summary.json"
$result = [ordered]@{
  base_url = $BaseUrl
  health_url = $healthUrl
  model = $Model
  health = "FAIL"
  chat_completions = "FAIL"
  run_dir = $RunDir
}

try {
  $health = Invoke-RestMethod -Method Get -Uri $healthUrl -TimeoutSec 10
  $health | ConvertTo-Json -Depth 12 > (Join-Path $RunDir "health.json")
  $result.health = "PASS"
} catch {
  $_ | Out-String > (Join-Path $RunDir "health-error.log")
}

try {
  $body = @{
    model = $Model
    messages = @(@{ role = "user"; content = "Return exactly OK." })
    max_tokens = 16
  } | ConvertTo-Json -Depth 12
  Invoke-RestMethod -Method Post -Uri "$BaseUrl/chat/completions" -ContentType "application/json" -Body $body -TimeoutSec $TimeoutSeconds |
    ConvertTo-Json -Depth 12 > (Join-Path $RunDir "chat-completions.json")
  $result.chat_completions = "PASS"
} catch {
  $_ | Out-String > (Join-Path $RunDir "chat-completions-error.log")
}

$result | ConvertTo-Json -Depth 8 | Tee-Object -FilePath $summary
