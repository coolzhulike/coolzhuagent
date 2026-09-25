param(
  [ValidateSet("qwen", "showui")]
  [string]$Profile = "qwen",
  [string]$InstallRoot = "",
  [string]$RunDir = "",
  [switch]$Background,
  [switch]$WaitReady,
  [switch]$EagerLoad,
  [int]$TimeoutSeconds = 300
)

$ErrorActionPreference = "Stop"

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

function Test-LocalVlmHealth {
  param([string]$HealthUrl)
  try {
    $response = Invoke-RestMethod -Method Get -Uri $HealthUrl -TimeoutSec 5
    return $response
  } catch {
    return $null
  }
}

$repoRoot = Find-RepoRoot
if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
  if (-not [string]::IsNullOrWhiteSpace($env:COOLZHU_LOCAL_VLM_ROOT)) {
    $InstallRoot = $env:COOLZHU_LOCAL_VLM_ROOT
  } else {
    $InstallRoot = Join-Path $env:USERPROFILE ".claw\local-vlm"
  }
}
$InstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)

if ([string]::IsNullOrWhiteSpace($RunDir)) {
  $RunDir = Join-Path $repoRoot ("tmp\logs\local-vlm-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
New-Item -ItemType Directory -Force -Path $RunDir | Out-Null

if ($Profile -eq "qwen") {
  $launcher = Join-Path $InstallRoot "start-qwen-vl-server.ps1"
  $baseUrl = "http://127.0.0.1:8001/v1"
  $model = "qwen2.5-vl-3b"
  $logPath = Join-Path $RunDir "qwen-vl-server.log"
} else {
  $launcher = Join-Path $InstallRoot "start-showui-server.ps1"
  $baseUrl = "http://127.0.0.1:8000/v1"
  $model = "showui-2b"
  $logPath = Join-Path $RunDir "showui-server.log"
}
$healthUrl = ConvertTo-HealthUrl $baseUrl
$summaryPath = Join-Path $RunDir "local-vlm-start-summary.json"

if (-not (Test-Path $InstallRoot)) {
  throw "Local VLM install root not found: $InstallRoot"
}
if (-not (Test-Path $launcher)) {
  throw "Local VLM launcher not found: $launcher"
}

$health = Test-LocalVlmHealth $healthUrl
if ($health) {
  @{
    status = "already-running"
    profile = $Profile
    install_root = $InstallRoot
    base_url = $baseUrl
    health_url = $healthUrl
    model = $model
    run_dir = $RunDir
    health = $health
  } | ConvertTo-Json -Depth 8 | Tee-Object -FilePath $summaryPath
  exit 0
}

if ($Background) {
  $argsList = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", $launcher,
    "-LogPath", $logPath
  )
  if ($EagerLoad) {
    $argsList += "-EagerLoad"
  }
  $process = Start-Process -FilePath "powershell" -ArgumentList $argsList -WindowStyle Hidden -PassThru

  $ready = $false
  $lastHealth = $null
  if ($WaitReady) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
      Start-Sleep -Seconds 5
      $lastHealth = Test-LocalVlmHealth $healthUrl
      if ($lastHealth) {
        $ready = $true
        break
      }
      if ($process.HasExited) {
        break
      }
    }
  }

  @{
    status = if ($ready) { "ready" } elseif ($process.HasExited) { "exited-before-ready" } else { "started" }
    profile = $Profile
    pid = $process.Id
    install_root = $InstallRoot
    launcher = $launcher
    base_url = $baseUrl
    health_url = $healthUrl
    model = $model
    run_dir = $RunDir
    log_path = $logPath
    ready = $ready
    health = $lastHealth
  } | ConvertTo-Json -Depth 8 | Tee-Object -FilePath $summaryPath
  exit 0
}

$launcherArgs = @{
  LogPath = $logPath
}
if ($EagerLoad) {
  & $launcher @launcherArgs -EagerLoad
} else {
  & $launcher @launcherArgs
}
