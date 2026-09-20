param(
  [string]$Manifest = 'config/package-manifest.json',
  [ValidateSet('debug', 'release')]
  [string]$Configuration = 'debug',
  [switch]$SkipBuild,
  [string]$PackageRoot,
  [string]$ReportPath
)

$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path '.').Path
$repoPath = [System.IO.Path]::GetFullPath($repo).TrimEnd('\', '/')
$repoPrefix = $repoPath + [System.IO.Path]::DirectorySeparatorChar
$manifestPath = if ([System.IO.Path]::IsPathRooted($Manifest)) {
  [System.IO.Path]::GetFullPath($Manifest)
} else {
  [System.IO.Path]::GetFullPath((Join-Path $repo $Manifest))
}

if (-not $manifestPath.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "package manifest must stay inside the workspace: $manifestPath"
}
if (-not (Test-Path -LiteralPath $manifestPath)) {
  throw "package manifest not found: $manifestPath"
}

$manifestData = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$packageResourceExcludedPathPattern = '(?i)(^|[/\\])(\.coolzhu|\.git|\.claude|\.superpowers|__pycache__|backups?(?:-[^/\\]+)?|tmp|logs?|sessions?)([/\\]|$)|web-sessions|coolzhu\.toml$|package-report\.json$|(^|[/\\])\.env($|\.)|\.(sqlite3?|db)(-(wal|shm|journal))?$|\.(pyc|pyo|bak|old|orig|rej|pem|key|pfx|p12)$|(^|[/\\])(credentials?|secrets?|token-cache|credential-cache|access-token|refresh-token|session-token|auth-token)(\.[^/\\]+)?$'
$packageRootCandidate = if ($PackageRoot) {
  if ([System.IO.Path]::IsPathRooted($PackageRoot)) { $PackageRoot } else { Join-Path $repo $PackageRoot }
} elseif ($manifestData.package_root) {
  if ([System.IO.Path]::IsPathRooted($manifestData.package_root)) { $manifestData.package_root } else { Join-Path $repo $manifestData.package_root }
} else {
  Join-Path $repo 'package'
}

$resolvedPackagePath = [System.IO.Path]::GetFullPath($packageRootCandidate).TrimEnd('\', '/')
if (
  $resolvedPackagePath.Equals($repoPath, [System.StringComparison]::OrdinalIgnoreCase) -or
  -not $resolvedPackagePath.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)
) {
  throw "package root must be a child of the workspace: $resolvedPackagePath"
}

# 递归删除只允许命中专用 package/ 或 tmp/ staging，避免参数错误清空源码目录。
$defaultPackageRoot = Join-Path $repoPath 'package'
$tmpRoot = Join-Path $repoPath 'tmp'
$tmpPrefix = $tmpRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
$isDefaultPackageRoot = $resolvedPackagePath.Equals(
  [System.IO.Path]::GetFullPath($defaultPackageRoot).TrimEnd('\', '/'),
  [System.StringComparison]::OrdinalIgnoreCase
)
$isTmpStagingRoot = $resolvedPackagePath.StartsWith(
  [System.IO.Path]::GetFullPath($tmpPrefix),
  [System.StringComparison]::OrdinalIgnoreCase
)
if (-not $isDefaultPackageRoot -and -not $isTmpStagingRoot) {
  throw "package root must be workspace\package or a child of workspace\tmp: $resolvedPackagePath"
}

# 删除 staging 前先完成 manifest 路径预检，避免越界输入或 `..\` 输出在复制后才失败。
$packageChildPrefix = $resolvedPackagePath + [System.IO.Path]::DirectorySeparatorChar
$releaseInputs = [System.Collections.Generic.List[string]]::new()
$releaseInputs.Add($manifestPath)
foreach ($artifact in @($manifestData.artifacts)) {
  if ($artifact.source) {
    $releaseInputs.Add(([string]$artifact.source).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration))
  }
  if ($artifact.build -and $artifact.build.working_dir) {
    $releaseInputs.Add(([string]$artifact.build.working_dir).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration))
  }
}
foreach ($resource in @($manifestData.resources)) {
  if ($resource.source) {
    $releaseInputs.Add(([string]$resource.source).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration))
  }
}
foreach ($inputPath in $releaseInputs) {
  $candidate = if ([System.IO.Path]::IsPathRooted($inputPath)) { $inputPath } else { Join-Path $repoPath $inputPath }
  $fullInput = [System.IO.Path]::GetFullPath($candidate).TrimEnd('\', '/')
  if (
    -not $fullInput.Equals($repoPath, [System.StringComparison]::OrdinalIgnoreCase) -and
    -not $fullInput.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)
  ) {
    throw "release input must stay inside the workspace: $fullInput"
  }
}
foreach ($releaseItem in @($manifestData.artifacts) + @($manifestData.resources)) {
  if (-not $releaseItem.target) { continue }
  $targetText = ([string]$releaseItem.target).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration)
  $targetCandidate = if ([System.IO.Path]::IsPathRooted($targetText)) { $targetText } else { Join-Path $resolvedPackagePath $targetText }
  $fullTarget = [System.IO.Path]::GetFullPath($targetCandidate).TrimEnd('\', '/')
  if (-not $fullTarget.StartsWith($packageChildPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "release output must stay inside PackageRoot: $fullTarget"
  }
}

# 每次从空 staging 开始，杜绝 manifest 已删除 artifact、旧脚本和旧资源继续被 WiX 收集。
if (Test-Path -LiteralPath $resolvedPackagePath) {
  Remove-Item -LiteralPath $resolvedPackagePath -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $resolvedPackagePath | Out-Null
$resolvedPackageRoot = (Resolve-Path -LiteralPath $resolvedPackagePath).Path

$backupKeep = if ($manifestData.backup_keep) { [int]$manifestData.backup_keep } else { 10 }
$binRoot = Join-Path $resolvedPackageRoot 'bin'
$backupRoot = Join-Path $repo 'tmp\package-backups'
$logRoot = Join-Path $repo 'tmp/logs'
New-Item -ItemType Directory -Force -Path $binRoot, $backupRoot, $logRoot | Out-Null

function Expand-PackageToken {
  param([string]$Value)
  if ($null -eq $Value) { return $null }
  return $Value.Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration)
}

function Resolve-RepoPath {
  param([string]$Path)
  $expanded = Expand-PackageToken $Path
  if ([System.IO.Path]::IsPathRooted($expanded)) {
    return $expanded
  }
  return Join-Path $repo $expanded
}

function Resolve-PackagePath {
  param([string]$Path)
  $expanded = Expand-PackageToken $Path
  if ([System.IO.Path]::IsPathRooted($expanded)) {
    return $expanded
  }
  return Join-Path $resolvedPackageRoot $expanded
}

function Get-OptionalHash {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) {
    return $null
  }
  return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
}

function ConvertTo-RepoRelativePath {
  param([Parameter(Mandatory = $true)][string]$Path)
  $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  if ($fullPath.Equals($repoPath, [System.StringComparison]::OrdinalIgnoreCase)) {
    return '.'
  }
  if (-not $fullPath.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "release input must stay inside the workspace: $fullPath"
  }
  return $fullPath.Substring($repoPrefix.Length).Replace('\', '/')
}

function ConvertTo-PackageRelativePath {
  param([Parameter(Mandatory = $true)][string]$Path)
  $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  $packagePath = $resolvedPackageRoot.TrimEnd('\', '/')
  $packageChildPrefix = $packagePath + [System.IO.Path]::DirectorySeparatorChar
  if (-not $fullPath.StartsWith($packageChildPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "release output must stay inside PackageRoot: $fullPath"
  }
  return $fullPath.Substring($packageChildPrefix.Length).Replace('\', '/')
}

function Invoke-ArtifactBuild {
  param([object]$Artifact)
  if ($SkipBuild) {
    Write-Host "skip build: $($Artifact.id)"
    return
  }
  if (-not $Artifact.build) {
    Write-Host "no build command: $($Artifact.id)"
    return
  }

  $command = [string]$Artifact.build.command
  $args = @()
  if ($Artifact.build.args) {
    $args = @($Artifact.build.args | ForEach-Object { Expand-PackageToken ([string]$_) })
  }
  if ($Configuration -eq 'release' -and $Artifact.build.append_release_arg -ne $false) {
    if (-not ($args -contains '--release')) {
      $args += '--release'
    }
  }
  $workingDir = if ($Artifact.build.working_dir) { Resolve-RepoPath ([string]$Artifact.build.working_dir) } else { $repo }
  Write-Host "build $($Artifact.id): $command $($args -join ' ')"
  Push-Location $workingDir
  $previousErrorAction = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    & $command @args
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
      throw "build failed for $($Artifact.id) with exit code $exitCode"
    }
  } finally {
    $ErrorActionPreference = $previousErrorAction
    Pop-Location
  }
}

function Backup-ExistingArtifact {
  param(
    [Parameter(Mandatory = $true)][string]$ExistingPath,
    [Parameter(Mandatory = $true)][int]$Keep
  )
  if (-not (Test-Path -LiteralPath $ExistingPath)) {
    return
  }
  $leaf = Split-Path -Leaf $ExistingPath
  $stem = [System.IO.Path]::GetFileNameWithoutExtension($leaf)
  $ext = [System.IO.Path]::GetExtension($leaf)
  $artifactBackupDir = Join-Path $backupRoot $leaf
  New-Item -ItemType Directory -Force -Path $artifactBackupDir | Out-Null
  $timestamp = Get-Date -Format 'yyyyMMdd-HHmmssfff'
  $backupPath = Join-Path $artifactBackupDir "$stem.$timestamp$ext"
  Copy-Item -LiteralPath $ExistingPath -Destination $backupPath -Force
  Write-Host "backup $leaf -> $backupPath"

  $resolvedBackupDir = (Resolve-Path -LiteralPath $artifactBackupDir).Path
  $resolvedBackupRoot = (Resolve-Path -LiteralPath $backupRoot).Path
  if (-not $resolvedBackupDir.StartsWith($resolvedBackupRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refuse to prune outside package backup root: $resolvedBackupDir"
  }

  $pattern = "$stem.*$ext"
  $oldBackups = @(Get-ChildItem -LiteralPath $artifactBackupDir -Filter $pattern -File | Sort-Object LastWriteTimeUtc -Descending | Select-Object -Skip $Keep)
  foreach ($old in $oldBackups) {
    Remove-Item -LiteralPath $old.FullName -Force
    Write-Host "prune old backup $($old.FullName)"
  }
}

function Resolve-WebView2LoaderSource {
  param(
    [Parameter(Mandatory = $true)][object]$Artifact,
    [Parameter(Mandatory = $true)][string]$RequestedSource
  )

  if (Test-Path -LiteralPath $RequestedSource -PathType Leaf) {
    return $RequestedSource
  }

  # Tauri 的 WebView2 依赖有时只落在本次 cargo 构建的 build crate x64 输出中。
  # 仅对这个明确的 artifact 启用回退，并把搜索范围锁在 source 所属 release/build 下。
  if ([string]$Artifact.id -ne 'gui-desktop.webview2-loader') {
    return $RequestedSource
  }

  $releaseRoot = Split-Path -Parent $RequestedSource
  $fallbackRoot = Join-Path $releaseRoot 'build'
  $candidates = @()
  if (Test-Path -LiteralPath $fallbackRoot -PathType Container) {
    $buildCrates = @(Get-ChildItem -LiteralPath $fallbackRoot -Directory -Filter 'webview2-com-sys-*' -Force | Sort-Object FullName)
    foreach ($buildCrate in $buildCrates) {
      $candidatePath = Join-Path $buildCrate.FullName 'out\x64\WebView2Loader.dll'
      if (Test-Path -LiteralPath $candidatePath -PathType Leaf) {
        $candidates += (Get-Item -LiteralPath $candidatePath)
      }
    }
  }

  if ($candidates.Count -eq 1) {
    Write-Host "fallback source $($Artifact.id): $RequestedSource -> $($candidates[0].FullName)"
    return $candidates[0].FullName
  }

  if ($candidates.Count -eq 0) {
    throw "artifact source missing for $($Artifact.id): $RequestedSource; no x64 WebView2Loader.dll candidate under $fallbackRoot"
  }

  $candidateList = ($candidates | ForEach-Object { $_.FullName }) -join '; '
  throw "ambiguous WebView2 loader sources for $($Artifact.id): expected one x64 candidate under $fallbackRoot, found $($candidates.Count): $candidateList"
}

function Publish-Artifact {
  param([object]$Artifact)
  $requestedSource = Resolve-RepoPath ([string]$Artifact.source)
  $source = Resolve-WebView2LoaderSource -Artifact $Artifact -RequestedSource $requestedSource
  $target = Resolve-PackagePath ([string]$Artifact.target)
  $sourceRelative = ConvertTo-RepoRelativePath $source
  $targetRelative = ConvertTo-PackageRelativePath $target
  if (-not (Test-Path -LiteralPath $source)) {
    throw "artifact source missing for $($Artifact.id): $source"
  }
  $targetParent = Split-Path -Parent $target
  New-Item -ItemType Directory -Force -Path $targetParent | Out-Null

  $sourceHash = Get-OptionalHash $source
  $targetHash = Get-OptionalHash $target
  $shouldCopy = $true
  if ($targetHash -and $sourceHash -eq $targetHash) {
    $sourceTime = (Get-Item -LiteralPath $source).LastWriteTimeUtc
    $targetTime = (Get-Item -LiteralPath $target).LastWriteTimeUtc
    $shouldCopy = $sourceTime -gt $targetTime
  }

  if (-not $shouldCopy) {
    Write-Host "unchanged $($Artifact.id): $target"
    return [pscustomobject]@{
      id = $Artifact.id
      source = $sourceRelative
      target = $targetRelative
      copied = $false
      sha256 = $sourceHash
    }
  }

  if (Test-Path -LiteralPath $target) {
    Backup-ExistingArtifact -ExistingPath $target -Keep $backupKeep
  }
  Copy-Item -LiteralPath $source -Destination $target -Force
  Write-Host "publish $($Artifact.id): $source -> $target"
  return [pscustomobject]@{
    id = $Artifact.id
    source = $sourceRelative
    target = $targetRelative
    copied = $true
    sha256 = $sourceHash
  }
}

function Assert-PackageChildPath {
  param([Parameter(Mandatory = $true)][string]$Path)
  $resolvedPath = if (Test-Path -LiteralPath $Path) {
    (Resolve-Path -LiteralPath $Path).Path
  } else {
    $parent = Split-Path -Parent $Path
    $leaf = Split-Path -Leaf $Path
    $resolvedParent = (Resolve-Path -LiteralPath $parent).Path
    Join-Path $resolvedParent $leaf
  }
  $resolvedRoot = (Resolve-Path -LiteralPath $resolvedPackageRoot).Path
  if (-not $resolvedPath.StartsWith($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refuse to modify resource outside package root: $resolvedPath"
  }
}

function Copy-PackageResource {
  param([object]$Resource)
  $source = Resolve-RepoPath ([string]$Resource.source)
  $target = Resolve-PackagePath ([string]$Resource.target)
  [void](ConvertTo-RepoRelativePath $source)
  [void](ConvertTo-PackageRelativePath $target)
  if (-not (Test-Path -LiteralPath $source)) {
    if ($Resource.optional -eq $true) {
      Write-Host "skip optional resource $($Resource.id): $source"
      return
    }
    throw "resource source missing for $($Resource.id): $source"
  }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
  Assert-PackageChildPath -Path $target
  if ((Get-Item -LiteralPath $source).PSIsContainer) {
    if (Test-Path -LiteralPath $target) {
      Remove-Item -LiteralPath $target -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    $resolvedSourceRoot = (Resolve-Path -LiteralPath $source).Path.TrimEnd('\', '/')
    $resolvedSourcePrefix = $resolvedSourceRoot + [System.IO.Path]::DirectorySeparatorChar
    Get-ChildItem -LiteralPath $resolvedSourceRoot -File -Recurse -Force | ForEach-Object {
      $relativePath = $_.FullName.Substring($resolvedSourcePrefix.Length)
      if ($relativePath -notmatch $packageResourceExcludedPathPattern) {
        $destination = Join-Path $target $relativePath
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
        Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
      }
    }
  } else {
    if (Test-Path -LiteralPath $target -PathType Container) {
      Remove-Item -LiteralPath $target -Recurse -Force
    }
    Copy-Item -LiteralPath $source -Destination $target -Force
  }
  Write-Host "resource $($Resource.id): $source -> $target"
}

$results = @()
foreach ($artifact in @($manifestData.artifacts)) {
  Invoke-ArtifactBuild -Artifact $artifact
  $results += Publish-Artifact -Artifact $artifact
}

foreach ($resource in @($manifestData.resources)) {
  Copy-PackageResource -Resource $resource
}

& (Join-Path $repo 'scripts/package-safety.ps1') -Root $resolvedPackageRoot | Out-Null

$report = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString('o')
  configuration = $Configuration
  manifest = ConvertTo-RepoRelativePath $manifestPath
  package_root = ConvertTo-RepoRelativePath $resolvedPackageRoot
  backup_keep = $backupKeep
  artifacts = $results
}

$resolvedReportPath = if ($ReportPath) {
  if ([System.IO.Path]::IsPathRooted($ReportPath)) {
    [System.IO.Path]::GetFullPath($ReportPath)
  } else {
    [System.IO.Path]::GetFullPath((Join-Path $repoPath $ReportPath))
  }
} else {
  $reportStamp = Get-Date -Format 'yyyyMMdd-HHmmssfff'
  Join-Path $repoPath "tmp\package-reports\package-report-$Configuration-$reportStamp.json"
}
$resolvedReportPath = [System.IO.Path]::GetFullPath($resolvedReportPath)
if (-not $resolvedReportPath.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "package report must stay inside the workspace: $resolvedReportPath"
}
$packagePrefix = $resolvedPackageRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
if (
  $resolvedReportPath.Equals($resolvedPackageRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
  $resolvedReportPath.StartsWith($packagePrefix, [System.StringComparison]::OrdinalIgnoreCase)
) {
  throw "package report must not be written inside PackageRoot: $resolvedReportPath"
}
$reportParent = Split-Path -Parent $resolvedReportPath
New-Item -ItemType Directory -Force -Path $reportParent | Out-Null
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $resolvedReportPath -Encoding UTF8
Write-Host "package report: $resolvedReportPath"
