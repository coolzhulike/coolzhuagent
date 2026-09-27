param(
  [string]$Manifest = 'config/package-manifest.json',
  [ValidateSet('debug', 'release')]
  [string]$Configuration = 'debug',
  [switch]$SkipBuild,
  [string]$PackageRoot,
  [string]$ReportPath,
  # 失败诊断落点（默认 <报告目录>/failures/package-report-failure-<config>-<stamp>.json）。
  # 失败运行**不写**成功报告、**不更新** latest 指针，只写这份诊断；因此
  # "拒绝发布"与"输出失败诊断"两件事可以同时成立（第六轮裁决 §三）。
  [string]$FailureReportPath,
  # -------------------------------------------------------------------------
  # RD4-06 准备/冻结分离（第七轮裁决 §3.1）
  #   准备阶段（-Prepare）：解析/更新依赖、生成必要配置 → 展示变化并按既有变更政策确认
  #                        → **冻结完整输入快照**；随后退出，不做正式构建。
  #   正式构建阶段（默认）：消费冻结输入 → 构建/测试/导出/打包 → 任一声明输入改变即拒绝发布。
  #   正式构建阶段用 -FreezeRecordPath 显式消费准备阶段产出的冻结记录；不显式给出时，
  #   本次构建会在自身开始处**自冻结**输入（记为 self-frozen-at-build-start，属于单阶段运行）。
  # -------------------------------------------------------------------------
  [switch]$Prepare,
  [string]$FreezeRecordPath,
  # 准备阶段确认"输入确实发生了变化且我们接受"：未给出时，发现变化即拒绝（不静默放行）。
  [switch]$AcceptPreparationChanges,
  # 准备阶段显式允许解析并**更新**锁定内容（默认只做只读解析核对，不修改 Cargo.lock）。
  [switch]$UpdateDependencies
)


$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path '.').Path
$repoPath = [System.IO.Path]::GetFullPath($repo).TrimEnd('\', '/')
$repoPrefix = $repoPath + [System.IO.Path]::DirectorySeparatorChar

# WebView2Loader 的确定性产物链（选源必须绑定本次构建记录 + 内容身份）。
# 该库不提供任何目录扫描兜底；旧的无来源搜索回退已按第四轮裁决移除。
. (Join-Path $repo 'scripts/lib/webview2-loader.ps1')
# 构建身份与报告治理（三身份分离 / 报告 ID+内容哈希 / 保留规则 / 失败诊断与发布资格）。
. (Join-Path $repo 'scripts/lib/build-identity.ps1')

# ---------------------------------------------------------------------------
# 本次运行 ID 与阶段追踪。
#   * 运行 ID 在任何预检之前确定：失败诊断要能指认"是哪一次运行失败了"；
#   * 阶段名 + 已记录的各阶段退出码进入失败诊断（裁决第 3 条）。
# ---------------------------------------------------------------------------
$script:packageRunId = Get-PackageReportId -Configuration $Configuration -Prefix 'pkg-run'
# 本次运行的起始时刻：用于判定"某个产物文件是不是本次运行构建/重写出来的"（mtime 证据）。
$script:packageRunStartedUtc = (Get-Date).ToUniversalTime()
$script:packageStage = 'preflight'
$script:packageStageDetail = $null
$script:packageStageRecords = [System.Collections.Generic.List[object]]::new()
# 运行阶段（RD4-06）：prepare = 准备/冻结；build = 正式构建（消费冻结输入）。
$script:packagePhase = $(if ($Prepare) { 'prepare' } else { 'build' })

# staging 是否已被本次运行初始化（失败诊断的隔离搬运只允许针对"本次运行产出的暂存物"，
# 预检失败时包根里若有上一轮的内容，那不属于本次运行，不得搬走）。
$script:stagingInitialized = $false
# 运行中收集的上下文（失败诊断要用到，且必须能在失败时安全读取）。
$script:releaseContext = [ordered]@{
  sourceSnapshotScope = $null
  sourceSnapshotPre = $null
  sourceSnapshotPost = $null
  snapshotComparison = $null
  buildInputDescriptorsBaseline = $null
  buildInputComparison = $null
  buildInputUnconfirmedReason = 'baseline-not-computed'
  artifactResults = @()
  exportSummaries = @()
  cargoIdentity = $null
  # RD4-06 准备/冻结分离的状态（供报告/诊断使用）。
  freezeRecord = $null
  freezeDocument = $null
  freezeComparison = $null
  generatedConfigs = @()
  derivedOutputs = @()
  dependencyResolution = $null
  changedPaths = @()
  frozenInputsMode = $(if ($FreezeRecordPath) { 'consumed-preparation-freeze-record' } else { 'self-frozen-at-build-start' })
}

function Set-PackageStage {
  param([string]$Stage, [string]$Detail)
  $script:packageStage = $Stage
  $script:packageStageDetail = $Detail
}

function Add-PackageStageRecord {
  param(
    [Parameter(Mandatory = $true)][string]$Stage,
    [string]$ArtifactId,
    [string]$Command,
    # 真实进程退出码；非进程阶段传 $null ⇒ 记为 not-applicable（不伪造 0）
    [AllowNull()]$ExitCode,
    [Parameter(Mandatory = $true)][string]$Outcome,
    [string]$Detail
  )

  $recorded = $(if ($null -eq $ExitCode) { 'not-applicable' } else { $ExitCode })
  $script:packageStageRecords.Add([pscustomobject]@{
    stage = $Stage
    artifact_id = $ArtifactId
    command = $Command
    exit_code = $recorded
    outcome = $Outcome
    detail = $Detail
  }) | Out-Null
}

$manifestPath = if ([System.IO.Path]::IsPathRooted($Manifest)) {
  [System.IO.Path]::GetFullPath($Manifest)
} else {
  [System.IO.Path]::GetFullPath((Join-Path $repo $Manifest))
}

<#
  报告/诊断里的冻结记录引用：只登记**可外发**的相对路径与摘要。

  函数内部的冻结记录对象带 absolute_path（供脚本内部使用），但报告与诊断不得包含
  开发者绝对路径（package-safety 会直接拒绝），因此进入报告/诊断前必须先投影。
#>
function ConvertTo-PackageReportFreezeReference {
  param([AllowNull()]$Freeze)

  if (-not $Freeze) { return $null }
  return [ordered]@{
    path = [string]$Freeze.path
    content_sha256 = [string]$Freeze.content_sha256
    freeze_id = [string]$Freeze.freeze_id
    reused_existing_record = $Freeze.reused_existing_record
    pointer_path = $(if ($Freeze.pointer_path) { [string]$Freeze.pointer_path } else { $null })
  }
}

# 主体从预检开始就被 try 包住：任何失败都要产出失败诊断（拒绝发布 + 输出诊断）。
# 注意：body 未重新缩进（避免对 900+ 行做纯排版改动），try/catch 与体同列。
try {

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
  if ($artifact.export) {
    foreach ($exportPath in @($artifact.export.receipt, $artifact.export.build_root, $artifact.export.build_entry_manifest, $artifact.export.lockfile)) {
      if ($exportPath) {
        $releaseInputs.Add(([string]$exportPath).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration))
      }
    }
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
# 例外（RD4-06 准备阶段）：`-Prepare` **不得触碰包根**——它只解析依赖、生成配置、冻结输入，
# 不产出任何可发布内容；因此也不允许把上一轮成功打包的包根清空。
$resolvedPackageRoot = $null
$backupKeep = if ($manifestData.backup_keep) { [int]$manifestData.backup_keep } else { 10 }
$binRoot = $null
$backupRoot = Join-Path $repo 'tmp\package-backups'
$logRoot = Join-Path $repo 'tmp/logs'
New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
if (-not $Prepare) {
  if (Test-Path -LiteralPath $resolvedPackagePath) {
    Remove-Item -LiteralPath $resolvedPackagePath -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $resolvedPackagePath | Out-Null
  $resolvedPackageRoot = (Resolve-Path -LiteralPath $resolvedPackagePath).Path
  # 从这里开始，包根里的内容都归本次运行所有 ⇒ 失败时可以把它们整体移入失败隔离区（分区）。
  $script:stagingInitialized = $true
  # 注意：这里的 Detail 不能调用本文件后面才定义的函数（PowerShell 按执行顺序定义函数）。
  Set-PackageStage -Stage 'staging-init' -Detail $resolvedPackageRoot
  Add-PackageStageRecord -Stage 'staging-init' -Outcome 'completed' -Detail ('staging root prepared: {0}' -f $resolvedPackageRoot) | Out-Null
  $binRoot = Join-Path $resolvedPackageRoot 'bin'
  New-Item -ItemType Directory -Force -Path $binRoot, $backupRoot | Out-Null
} else {
  Set-PackageStage -Stage 'staging-init' -Detail 'skipped-for-prepare'
  Add-PackageStageRecord -Stage 'staging-init' -Outcome 'skipped' -Detail '准备阶段不触碰包根（不清理、不创建、不产出任何可发布内容）' | Out-Null
}


# 本次运行的构建证据：cargo 消息流与调用信息，供 Loader 导出阶段核对来源。
$script:artifactBuildMessages = @{}
$script:artifactBuildInputs = @{}
$script:exportReceipts = @{}
$script:exportSummaries = [System.Collections.Generic.List[object]]::new()
# PKG-L07c 固定代次语义：本次运行**自己的**消费 staging（派生输出位置）。
# 每个 export artifact 的产物先复制到这里并复核内容身份，之后打包只从这里取源；
# 报告也只从本次固定消费收据生成，不重新读取共享输出槽位的"最新收据"。
$script:consumerStagingRoot = Join-Path $repoPath ('tmp/package-consume/' + $script:packageRunId)
$script:exportConsumptions = @{}
$releaseVersion = [string]$env:COOLZHU_RELEASE_VERSION

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

function Get-CargoIdentity {
  # 记录构建身份用的 target / host / 版本。CARGO_BUILD_TARGET 优先（与 build-msi.ps1 一致）。
  $identity = [pscustomobject]@{
    build_target = $null
    host_target = $null
    cargo_version = $null
  }
  $configuredTarget = [Environment]::GetEnvironmentVariable('CARGO_BUILD_TARGET', 'Process')
  if (-not [string]::IsNullOrWhiteSpace($configuredTarget)) {
    $identity.build_target = $configuredTarget.Trim()
  }
  $versionOutput = @(& cargo -vV 2>$null)
  if ($LASTEXITCODE -eq 0) {
    $identity.cargo_version = ([string]($versionOutput | Select-Object -First 1)).Trim()
    foreach ($line in $versionOutput) {
      $match = [regex]::Match([string]$line, '^host:\s*(\S+)\s*$')
      if ($match.Success) {
        $identity.host_target = $match.Groups[1].Value
        break
      }
    }
  }
  if (-not $identity.build_target) { $identity.build_target = $identity.host_target }
  return $identity
}

function Invoke-ArtifactBuild {
  param([object]$Artifact)
  Set-PackageStage -Stage 'artifact-build' -Detail ([string]$Artifact.id)
  if ($SkipBuild) {
    Write-Host "skip build: $($Artifact.id)"
    Add-PackageStageRecord -Stage 'artifact-build' -ArtifactId ([string]$Artifact.id) -Outcome 'skipped' -Detail 'no-build 模式：不构建，改用既有二进制/既有导出收据' | Out-Null
    return
  }
  if (-not $Artifact.build) {
    Write-Host "no build command: $($Artifact.id)"
    Add-PackageStageRecord -Stage 'artifact-build' -ArtifactId ([string]$Artifact.id) -Outcome 'no-command' -Detail 'manifest 未声明 build：来源是已存在的文件（其快照关联由产物—快照关联记录确认）' | Out-Null
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
  $capture = [string]$Artifact.build.capture
  $startedUtc = (Get-Date).ToUniversalTime().ToString('o')
  $stderrPath = $null
  $stdoutPath = $null
  Write-Host "build $($Artifact.id): $command $($args -join ' ')"
  Push-Location $workingDir
  $previousErrorAction = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    if ($capture -eq 'cargo-json-messages') {
      # 需要机器可读的构建消息（build-script-executed）来定位实际依赖生产者。
      # 用 .NET Process 直接接管 stdio：stdout 落盘成可核对的原始记录，stderr 构建结束后回显。
      if (-not ($args | Where-Object { $_ -like '--message-format=*' })) {
        throw "artifact $($Artifact.id) declares capture=$capture but its build args do not set --message-format"
      }
      $stamp = Get-Date -Format 'yyyyMMdd-HHmmssfff'
      $stdoutPath = Join-Path $logRoot "$($Artifact.id)-build-messages-$stamp.jsonl"
      $stderrPath = Join-Path $logRoot "$($Artifact.id)-build-stderr-$stamp.log"
      Write-Host "capture build messages: $stdoutPath"
      $argumentText = (@($args) | ForEach-Object { if ([string]$_ -match '\s') { '"' + [string]$_ + '"' } else { [string]$_ } }) -join ' '
      $startInfo = New-Object System.Diagnostics.ProcessStartInfo
      $startInfo.FileName = $command
      $startInfo.Arguments = $argumentText
      $startInfo.WorkingDirectory = $workingDir
      $startInfo.UseShellExecute = $false
      $startInfo.RedirectStandardOutput = $true
      $startInfo.RedirectStandardError = $true
      $process = [System.Diagnostics.Process]::Start($startInfo)
      $stdoutTask = $process.StandardOutput.ReadToEndAsync()
      $stderrText = $process.StandardError.ReadToEnd()
      $process.WaitForExit()
      $stdoutText = [string]$stdoutTask.Result
      $exitCode = $process.ExitCode
      $process.Dispose()
      [System.IO.File]::WriteAllText($stdoutPath, $stdoutText, [System.Text.UTF8Encoding]::new($false))
      [System.IO.File]::WriteAllText($stderrPath, $stderrText, [System.Text.UTF8Encoding]::new($false))
      foreach ($line in @($stderrText -split "`r?`n")) {
        if ($line) { Write-Host $line }
      }
      if ($null -eq $exitCode) {
        throw "[BUILD-FAILED] artifact=$($Artifact.id) target=$($Artifact.target) profile=$Configuration`ndetail: 无法获取构建进程退出码，构建结果不可信`nnext: 检查构建命令与日志 $stdoutPath"
      }
      $script:artifactBuildMessages[[string]$Artifact.id] = @($stdoutText -split "`r?`n" | Where-Object { $_ })
      $script:artifactBuildInputs[[string]$Artifact.id] = [pscustomobject]@{
        command = $command
        args = $args
        working_dir = $workingDir
        started_utc = $startedUtc
        raw_record_path = $stdoutPath
        stderr_path = $stderrPath
        exit_code = $exitCode
      }
    } else {
      & $command @args
      $exitCode = $LASTEXITCODE
    }
    if ($exitCode -ne 0) {
      # 区分"构建真的失败"与"离线依赖缓存缺失"：后者不得被记成 Loader 选源/回退失败。
      $category = 'BUILD-FAILED'
      $remediation = '使用独立、受控的构建目录重建后再打包；不要把全局 cargo clean 或删除共享 target 当作常规修复前置'
      if ($stderrPath -and (Test-Path -LiteralPath $stderrPath)) {
        $stderrText = [string](Get-Content -Raw -LiteralPath $stderrPath -ErrorAction SilentlyContinue)
        if ($stderrText -match 'no matching package|failed to download|attempting to make an HTTP request but --offline|not found in the registry|registry index was not found|no such file or directory.*registry') {
          $category = 'BUILD-DEPENDENCY-UNAVAILABLE'
          $remediation = '这是缺失离线依赖缓存导致的失败，不是 Loader 选源失败。先准备与锁文件一致的离线依赖缓存（vendored / registry cache）再重试；不要改成手工复制 DLL'
        }
      }
      Add-PackageStageRecord -Stage 'artifact-build' -ArtifactId ([string]$Artifact.id) -Command $command -ExitCode $exitCode -Outcome 'failed' -Detail $category | Out-Null
      throw "[$category] artifact=$($Artifact.id) target=$($Artifact.target) profile=$Configuration`ndetail: build exit code $exitCode`nnext: $remediation"
    }
    Add-PackageStageRecord -Stage 'artifact-build' -ArtifactId ([string]$Artifact.id) -Command $command -ExitCode $exitCode -Outcome 'completed' -Detail ('build exit code {0}' -f $exitCode) | Out-Null
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
  # 旧实现（已按第四轮裁决移除，保留同名函数只为让"这里曾经是目录扫描回退"可被搜索到）：
  #   Test-Path 声明源 → 直接用；否则扫 <release>/build/webview2-com-sys-*/out/x64/WebView2Loader.dll。
  # 移除原因：① 声明源只要存在就直接采用，早先手工放入的 DLL 会先于有效构建来源生效；
  #           ② 目录扫描结果没有来源记录，属于"未经绑定的搜索回退"。
  # 现在的契约：manifest.source 指向**受控稳定导出路径**，且必须配有导出记录（见 New-LoaderExportContext）。
  throw 'Resolve-WebView2LoaderSource 已移除：请使用 New-LoaderExportContext / Export-LoaderArtifact / Assert-LoaderExportReceipt'
}

<#
  为声明了 export 的 artifact 组装上下文：稳定导出路径、收据路径、
  构建入口的源码/配置身份、本次构建消息与调用记录。
#>
function New-LoaderExportContext {
  param(
    [Parameter(Mandatory = $true)][object]$Artifact,
    [Parameter(Mandatory = $true)][object]$Export
  )

  $artifactId = [string]$Artifact.id
  $buildEntryId = [string]$Export.build_entry_artifact
  if (-not $buildEntryId) {
    throw "[EXPORT-CONTRACT] artifact=$artifactId target=$($Artifact.target) profile=$Configuration`ndetail: export.build_entry_artifact 缺失，无法绑定构建入口`nnext: 在 manifest 中声明构建入口 artifact id"
  }
  $buildEntryArtifacts = @($manifestData.artifacts | Where-Object { [string]$_.id -eq $buildEntryId })
  if ($buildEntryArtifacts.Count -ne 1) {
    throw "[EXPORT-CONTRACT] artifact=$artifactId target=$($Artifact.target) profile=$Configuration`ndetail: export.build_entry_artifact=$buildEntryId 不能唯一解析（匹配 $($buildEntryArtifacts.Count) 个）`nnext: 修正 manifest 声明"
  }
  $buildEntryArtifact = $buildEntryArtifacts[0]

  # target-dir 从构建入口自己的声明里解析，避免在脚本里出现第二处硬编码。
  $entryArgs = @()
  if ($buildEntryArtifact.build -and $buildEntryArtifact.build.args) {
    $entryArgs = @($buildEntryArtifact.build.args | ForEach-Object { Expand-PackageToken ([string]$_) })
  }
  $cargoTargetDir = $null
  for ($index = 0; $index -lt $entryArgs.Count; $index++) {
    if ($entryArgs[$index] -eq '--target-dir' -and ($index + 1) -lt $entryArgs.Count) {
      $cargoTargetDir = [string]$entryArgs[$index + 1]
      break
    }
  }
  if (-not $cargoTargetDir) { $cargoTargetDir = 'target' }

  $buildEntryManifest = [string]$Export.build_entry_manifest
  $lockfile = [string]$Export.lockfile
  $buildInputs = if ($script:artifactBuildInputs.ContainsKey($buildEntryId)) { $script:artifactBuildInputs[$buildEntryId] } else { $null }
  $buildMessages = if ($script:artifactBuildMessages.ContainsKey($buildEntryId)) { @($script:artifactBuildMessages[$buildEntryId]) } else { @() }
  $workingDir = $(if ($buildInputs) { [string]$buildInputs.working_dir } else { Resolve-RepoPath ([string]$buildEntryArtifact.build.working_dir) })
  if (Test-Path -LiteralPath $workingDir) {
    $resolvedWorkingDir = (Resolve-Path -LiteralPath $workingDir).Path
    if ($resolvedWorkingDir.TrimEnd('\', '/').Equals($repoPath, [System.StringComparison]::OrdinalIgnoreCase)) {
      $workingDir = '.'
    } elseif ($resolvedWorkingDir.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      $workingDir = $resolvedWorkingDir.Substring($repoPrefix.Length).Replace('\', '/')
    }
  }
  $buildEntryManifestHash = Get-OptionalHash (Resolve-RepoPath $buildEntryManifest)
  $lockfileHash = Get-OptionalHash (Resolve-RepoPath $lockfile)

  return @{
    Artifact = $Artifact
    Export = $Export
    ArtifactId = $artifactId
    Profile = $Configuration
    RepoPath = $repoPath
    StablePath = Resolve-RepoPath ([string]$Artifact.source)
    ReceiptPath = Resolve-RepoPath ([string]$Export.receipt)
    BuildEntryArtifact = $buildEntryId
    BuildEntryManifest = $buildEntryManifest
    BuildEntryManifestSha256 = $(if ($buildEntryManifestHash) { $buildEntryManifestHash.ToLowerInvariant() } else { $null })
    LockfileRelative = $lockfile
    LockfileSha256 = $(if ($lockfileHash) { $lockfileHash.ToLowerInvariant() } else { $null })
    CargoTargetDir = $cargoTargetDir
    BuildTarget = [string]$cargoIdentity.build_target
    HostTarget = [string]$cargoIdentity.host_target
    CargoVersion = [string]$cargoIdentity.cargo_version
    ReleaseVersion = $releaseVersion
    BuildCommand = $(if ($buildInputs) { [string]$buildInputs.command } else { [string]$buildEntryArtifact.build.command })
    BuildArgs = $(if ($buildInputs) { @($buildInputs.args) } else { $entryArgs })
    BuildWorkingDir = $workingDir
    BuildStartedUtc = $(if ($buildInputs) { [string]$buildInputs.started_utc } else { $null })
    BuildMessages = $buildMessages
    RawRecordPath = $(if ($buildInputs) { [string]$buildInputs.raw_record_path } else { $null })
    # PKG-L07c 固定代次语义：产出/消费这一代次的运行 ID，以及**本次打包自己的**消费 staging
    # （派生输出位置，见 manifest.release_policy.preparation_phase.derived_outputs）。
    RunId = $script:packageRunId
    ProducerRunId = $script:packageRunId
    ConsumerRunId = $script:packageRunId
    ConsumeStagingDirectory = $script:consumerStagingRoot
  }
}

function Get-ReleaseStatusDocument {
  # 产物释放状态库（关联 / 禁用 / 运行台账）在一次运行内只读一次；本次运行的写入在末尾单独进行。
  if (-not $script:releaseStatusDocument) {
    $script:releaseStatusDocument = (Read-PackageArtifactReleaseStatus -RepoPath $repoPath).document
  }
  return $script:releaseStatusDocument
}

function Get-LoaderConsumption {
  # 本次运行对该 artifact 的**固定消费收据**（唯一来源：本运行自己登记的消费阶段结果）。
  param([string]$ArtifactId)

  if ($script:exportConsumptions.ContainsKey($ArtifactId)) { return $script:exportConsumptions[$ArtifactId] }
  return $null
}

function Register-LoaderConsumption {
  param(
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [Parameter(Mandatory = $true)][object]$Consumption,
    [Parameter(Mandatory = $true)][string]$Mode
  )

  # 硬约束：consumer_run_id 必须就是本次运行；generation 必须非空（"明确代次"）。
  if ([string]$Consumption.consumer_run_id -ne $script:packageRunId) {
    throw ("[CONSUMPTION-OWNER-MISMATCH] artifact={0} consumer_run_id={1} current_run={2}`ndetail: 消费收据不属于本次运行，拒绝用它生成报告`nnext: 不要跨运行复用消费收据" -f $ArtifactId, $Consumption.consumer_run_id, $script:packageRunId)
  }
  if (-not [string]$Consumption.generation) {
    throw ("[CONSUMPTION-NO-GENERATION] artifact={0} mode={1}`ndetail: 消费收据没有明确 generation，无法把包内产物绑定到某个不可变代次`nnext: 重新执行正常构建打包" -f $ArtifactId, $Mode)
  }
  if (-not $Consumption.consumed_content_matches_generation) {
    throw ("[CONSUMPTION-CONTENT-DRIFT] artifact={0} mode={1}`ndetail: 消费 staging 的内容身份与该代次登记的产物身份不一致`nnext: 重试打包；若重复出现，检查共享输出槽位是否被并发写入" -f $ArtifactId, $Mode)
  }
  $script:exportConsumptions[$ArtifactId] = $Consumption
  $script:exportReceipts[$ArtifactId] = $Consumption.receipt
}

<#
  导出物登记项（exported_artifacts[] 的元素）：固定代次语义字段 + **精确消费关联**。

  裁决 §3.2 要求的字段全部在此：slot_key / generation / producer_run_id / artifact_digest /
  receipt_digest / build_input_digest / consumer_run_id；另有本次消费的 staging 内容身份，
  使"报告字段 ↔ 包内实际字节"可被独立核对（测试据此断言一致，而不只是断言字段存在）。
#>
function New-LoaderExportSummary {
  param(
    [Parameter(Mandatory = $true)][object]$Artifact,
    [Parameter(Mandatory = $true)][hashtable]$Context,
    [Parameter(Mandatory = $true)][object]$Consumption,
    [Parameter(Mandatory = $true)][string]$Mode,
    [Parameter(Mandatory = $true)][string]$Validation,
    [Parameter(Mandatory = $true)][object]$Receipt
  )

  $entry = [ordered]@{
    artifact_id = [string]$Artifact.id
    mode = $Mode
    validation = $Validation
    # ---- 固定代次语义（裁决 §3.2 字段名）----
    slot_key = [string]$Consumption.slot_key
    generation = [string]$Consumption.generation
    producer_run_id = [string]$Consumption.producer_run_id
    consumer_run_id = [string]$Consumption.consumer_run_id
    artifact_digest = [string]$Consumption.artifact_digest
    artifact_length = [long]$Consumption.artifact_length
    receipt_digest = [string]$Consumption.receipt_digest
    build_input_digest = [string]$Consumption.build_input_digest
    # ---- 精确消费关联（本次打包消费的到底是哪一代、哪些字节）----
    consumption = [ordered]@{
      kind = [string]$Consumption.kind
      consumer_run_id = [string]$Consumption.consumer_run_id
      generation = [string]$Consumption.generation
      slot_key = [string]$Consumption.slot_key
      slot_dir = [string]$Consumption.slot_dir_relative
      generation_record = [string]$Consumption.generation_record
      pointer_path = [string]$Consumption.pointer_path
      pointer_generation_at_consumption = [string]$Consumption.pointer_generation_at_consumption
      pointer_committed_utc = [string]$Consumption.pointer_committed_utc
      supersedes_generation = [string]$Consumption.supersedes_generation
      staged_path = [string]$Consumption.consumed_staging_path
      staged_sha256 = [string]$Consumption.consumed_staging_sha256
      staged_length = [long]$Consumption.consumed_staging_length
      consumed_content_matches_generation = [bool]$Consumption.consumed_content_matches_generation
      consumption_verified = [bool]$Consumption.consumption_verified
      verification_source = [string]$Consumption.consumption_source
      read_right = [string]$Consumption.read_right
      fixed_at_utc = [string]$Consumption.fixed_at_utc
      report_generation_rule = [string]$Consumption.report_generation_rule
    }
    # ---- 既有证据字段（保持口径不变）----
    receipt = (ConvertTo-RepoRelativePath $Context.ReceiptPath)
    stable_export = $(if ($Mode -eq 'no-build') { ConvertTo-RepoRelativePath $Context.StablePath } else { [string]$Receipt.file_identity.stable_export })
    stable_export_sha256 = [string]$Consumption.artifact_digest
    stable_export_length = [long]$Consumption.artifact_length
    producer_package_id = [string]$Receipt.dependency_identity.package_id
    producer_out_dir = [string]$Receipt.dependency_identity.producer_out_dir
    producer_candidate_count = [int]$Receipt.dependency_identity.producer_candidate_count
    producer_selection_evidence = [string]$Receipt.dependency_identity.producer_selection_evidence
    architecture = [string]$Receipt.file_identity.architecture
    machine = [string]$Receipt.file_identity.machine
    reuse_nature = [string]$Receipt.reuse_nature
    producer_build_script_rerun_in_this_invocation = $Receipt.build_invocation.producer_build_script_rerun_in_this_invocation
    build_identity = $Receipt.build_identity
  }
  if ($Mode -eq 'build') {
    $entry.raw_source = [string]$Receipt.file_identity.raw_source
    $entry.raw_source_sha256 = [string]$Receipt.file_identity.raw_source_sha256
    $entry.raw_source_length = [long]$Receipt.file_identity.raw_source_length
    $entry.raw_source_machine = [string]$Receipt.file_identity.raw_source_machine
  }
  return [pscustomobject]$entry
}

function Resolve-ArtifactSource {
  param([object]$Artifact)

  $requestedSource = Resolve-RepoPath ([string]$Artifact.source)
  if (-not $Artifact.export) {
    return $requestedSource
  }
  # 声明了 export 的产物**只从本次固定消费收据指向的独立 staging 取源**：
  # 报告与打包都不再重新读取共享槽位（否则槽位在打包期间合法更新到新一代次时，
  # 报告引用的代次与包内实际字节可能漂移）。共享槽位的核对已在消费阶段完成。
  $consumption = Get-LoaderConsumption -ArtifactId ([string]$Artifact.id)
  if (-not $consumption) {
    throw ("[CONSUMPTION-MISSING] artifact={0} target={1} profile={2}`ndetail: 本次运行没有该产物的固定消费收据（导出/消费阶段未完成），不允许回退到读取共享输出槽位`nnext: 重新执行一次完整打包（正常构建）；不要手工把 DLL 复制到包目录" -f $Artifact.id, $Artifact.target, $Configuration)
  }
  return [string]$consumption.consumed_staging_absolute
}

function Invoke-ArtifactExport {
  param([object]$Artifact)

  Set-PackageStage -Stage 'artifact-export' -Detail ([string]$Artifact.id)
  $context = New-LoaderExportContext -Artifact $Artifact -Export $Artifact.export
  $artifactId = [string]$Artifact.id
  if ($SkipBuild) {
    # --no-build：按**消费端固定流程**核对当前有效代次，并把产物复制到本次独立 staging。
    # 只接受已有完整构建收据、且与本次声明输入/版本一致的代次；随后报告只引用本次固定收据。
    $consumption = Use-LoaderExportGeneration `
      -Context $context `
      -ConsumerRunId $script:packageRunId `
      -ConsumeStagingDirectory $script:consumerStagingRoot `
      -VerifyIdentity
    # 收据本身只覆盖构建入口子范围（receipt.build_identity.source_snapshot_digest 恒为 null、
    # source_snapshot_digest_available=false），所以"是否匹配**全量**快照"由产物—快照关联记录回答
    # （见包报告的 release_eligibility.artifact_provenance）：关联缺失 = 未能确认，不是通过。
    [void](Assert-PackageArtifactNotRevoked `
      -StatusDocument (Get-ReleaseStatusDocument) `
      -ArtifactId $artifactId `
      -Profile $Configuration `
      -Target ([string]$Artifact.target) `
      -SourceSha256 $consumption.artifact_digest `
      -StableExportSha256 $consumption.artifact_digest `
      -ReceiptPath ([string]$context.ReceiptPath))
    Write-Host ("no-build: generation {0} consumed and verified for {1}" -f $consumption.generation, $artifactId)
    Add-PackageStageRecord -Stage 'artifact-export' -ArtifactId $artifactId -Outcome 'completed' -Detail ('existing-generation-consumed（generation={0}；未登记禁用内容身份）' -f $consumption.generation) | Out-Null
    Register-LoaderConsumption -ArtifactId $artifactId -Consumption $consumption -Mode 'no-build'
    $script:exportSummaries.Add((New-LoaderExportSummary `
      -Artifact $Artifact -Context $context -Consumption $consumption `
      -Mode 'no-build' -Validation 'existing-receipt-verified' `
      -Receipt $consumption.receipt)) | Out-Null
    return
  }

  if ($context.BuildMessages.Count -eq 0) {
    throw (New-LoaderFailureException `
      -Category 'NOT-BUILT' `
      -ArtifactId $artifactId `
      -Target ([string]$Artifact.target) `
      -Profile $Configuration `
      -Candidates @([string]$Export.build_entry_artifact) `
      -Detail "本次运行没有构建入口 $($context.BuildEntryArtifact) 的构建消息，无法确定 Loader 生产者" `
      -Remediation "正常构建打包时会先构建该入口；如果使用了 --no-build，请提供该源码/配置对应的完整构建收据。不要手工复制 DLL 到稳定导出路径")
  }

  # 导出（发布临界区）内**同时**完成本次消费：产物核对/收据提交/当前有效代次指针/独立 staging
  # 全部在同一把排他发布权内完成，因此报告引用的代次与包内字节不可能漂移。
  $receipt = Export-LoaderArtifact -Context $context
  $consumption = $context.Consumption
  if (-not $consumption) {
    throw ("[CONSUMPTION-MISSING] artifact={0} target={1} profile={2}`ndetail: 导出完成但没有得到本次固定消费收据`nnext: 检查 scripts/lib/webview2-loader.ps1 的消费流程；不要绕过消费 staging 直接打包" -f $artifactId, $Artifact.target, $Configuration)
  }
  Register-LoaderConsumption -ArtifactId $artifactId -Consumption $consumption -Mode 'build'
  $script:exportSummaries.Add((New-LoaderExportSummary `
    -Artifact $Artifact -Context $context -Consumption $consumption `
    -Mode 'build' -Validation 'exported-in-this-run' `
    -Receipt $receipt)) | Out-Null
  Add-PackageStageRecord -Stage 'artifact-export' -ArtifactId $artifactId -Outcome 'completed' -Detail ('exported-in-this-run; reuse_nature={0}; generation={1}' -f [string]$receipt.reuse_nature, $consumption.generation) | Out-Null
}

function Publish-Artifact {
  param([object]$Artifact)
  Set-PackageStage -Stage 'artifact-publish' -Detail ([string]$Artifact.id)
  $target = Resolve-PackagePath ([string]$Artifact.target)
  $source = Resolve-ArtifactSource -Artifact $Artifact
  $sourceRelative = ConvertTo-RepoRelativePath $source
  $targetRelative = ConvertTo-PackageRelativePath $target
  if (-not (Test-Path -LiteralPath $source)) {
    throw "[SOURCE-MISSING] artifact=$($Artifact.id) target=$targetRelative profile=$Configuration`ndetail: artifact source missing: $sourceRelative`nnext: 正常构建模式下重新打包；--no-build 模式下请提供匹配的完整构建收据。不要手工把文件放进包目录"
  }
  $targetParent = Split-Path -Parent $target
  New-Item -ItemType Directory -Force -Path $targetParent | Out-Null

  $sourceHash = Get-OptionalHash $source
  if ($SkipBuild) {
    # 裁决第 9 条：--no-build 下，已登记禁用的内容身份一律不得再当来源（在复制之前拒绝）。
    # 导出物按稳定导出内容身份匹配；无导出声明的产物按其声明来源路径 + 内容身份匹配。
    # 这也覆盖"失败报告不得当作来源"：失败运行不产出成功报告，其产物身份被登记为禁用。
    $vetoReceipt = $(if ($Artifact.export) { [string]$Artifact.export.receipt } else { $null })
    [void](Assert-PackageArtifactNotRevoked `
      -StatusDocument (Get-ReleaseStatusDocument) `
      -ArtifactId ([string]$Artifact.id) `
      -Profile $Configuration `
      -Target $targetRelative `
      -SourcePath $sourceRelative `
      -SourceSha256 $sourceHash `
      -StableExportSha256 $sourceHash `
      -ReceiptPath $vetoReceipt)
  }
  $targetHash = Get-OptionalHash $target
  $shouldCopy = $true
  if ($targetHash -and $sourceHash -eq $targetHash) {
    $sourceTime = (Get-Item -LiteralPath $source).LastWriteTimeUtc
    $targetTime = (Get-Item -LiteralPath $target).LastWriteTimeUtc
    $shouldCopy = $sourceTime -gt $targetTime
  }

  $expectedHash = $sourceHash
  if ($Artifact.export) {
    # 导出物：包内目标必须等于**本次固定消费收据**登记的产物身份（artifact_digest）。
    # 注意这里不再读共享槽位的收据：报告与包内字节都绑定到同一份固定消费收据，
    # 槽位此后合法更新到新一代次也不会让两者漂移。
    $consumption = Get-LoaderConsumption -ArtifactId ([string]$Artifact.id)
    if (-not $consumption) {
      throw ("[CONSUMPTION-MISSING] artifact={0} target={1} profile={2}`ndetail: 缺少本次固定消费收据`nnext: 重新执行一次完整打包（正常构建）" -f $Artifact.id, $targetRelative, $Configuration)
    }
    $expectedHash = [string]$consumption.artifact_digest
    if ($sourceHash -ne $expectedHash) {
      throw "[CONTENT-MISMATCH] artifact=$($Artifact.id) target=$targetRelative profile=$Configuration`ndetail: 本次消费 staging 的内容与该代次登记的产物身份不一致（actual=$sourceHash generation=$($consumption.generation) artifact_digest=$expectedHash）`nnext: 重新执行正常构建导出；不要手工替换 staging 或稳定导出文件"
    }
  }

  if (-not $shouldCopy) {
    Write-Host "unchanged $($Artifact.id): $target"
    Add-PackageStageRecord -Stage 'artifact-publish' -ArtifactId ([string]$Artifact.id) -Outcome 'unchanged' -Detail ('target content identical to source: {0}' -f $targetRelative) | Out-Null
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
  # 复制后再核对一次内容身份：复制中改变 / 被并发替换都不允许进入正式包。
  $publishedHash = Get-OptionalHash $target
  if ($publishedHash -ne $expectedHash) {
    throw "[CONTENT-MISMATCH] artifact=$($Artifact.id) target=$targetRelative profile=$Configuration`ndetail: staged 文件与预期内容不一致（staged=$publishedHash expected=$expectedHash）`nnext: 重新执行打包；不要复用被改动的 staging 目录"
  }
  if ($Artifact.export) {
    $expectedMachine = Get-LoaderArchitectureMachine ([string]$Artifact.export.architecture)
    $stagedMachine = Get-LoaderPeMachine $target
    if ($stagedMachine -ne $expectedMachine) {
      throw "[ARCH-MISMATCH] artifact=$($Artifact.id) target=$targetRelative profile=$Configuration`ndetail: staged 文件架构不匹配（expected=$expectedMachine actual=$stagedMachine）`nnext: 用与目标架构一致的构建重新打包"
    }
  }
  Write-Host "publish $($Artifact.id): $source -> $target"
  Add-PackageStageRecord -Stage 'artifact-publish' -ArtifactId ([string]$Artifact.id) -Outcome 'published' -Detail ('staged {0} (sha256={1})' -f $targetRelative, $sourceHash) | Out-Null
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
  Set-PackageStage -Stage 'resource-copy' -Detail ([string]$Resource.id)
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

$cargoIdentity = Get-CargoIdentity
$results = @()

# ---------------------------------------------------------------------------
# 构建身份：**三个分开的身份**，实现在 scripts/lib/build-identity.ps1（唯一实现点）
#   source_snapshot_digest : 本次使用哪份第一方源码/构建资源快照（构建前冻结）
#   build_input_digest     : 该快照配合什么锁文件/工具链/target/profile/features/构建配置
#   payload_digest         : 最终生成与分发了哪些二进制与资源（见报告写入前的暂存包清单）
# VCS 口径（第五轮裁决 B-1）：
#   * 不改变 Git 状态（只读 rev-parse / ls-files / status，不做 init/add/commit/tag）；
#   * source_commit 不作为当前源码权威，可为空；
#   * vcs_reference_commit 只是种子提交参考；当前工作树未被该提交有效覆盖时
#     vcs_state = untracked_snapshot、dirty_against_commit = not_evaluable（绝不写 false）。
# ---------------------------------------------------------------------------
$vcsState = Get-VcsReferenceState -RepoPath $repoPath
$sourceSnapshotScope = Get-SourceSnapshotScope -ManifestData $manifestData
$sourceSnapshotPreBuild = $null
$sourceSnapshotFreeze = $null
if ($sourceSnapshotScope) {
  # 构建前捕获（两遍静默枚举 + 内容哈希）：正式构建的冻结输入快照。
  $sourceSnapshotPreBuild = New-SourceSnapshot -RepoPath $repoPath -Scope $sourceSnapshotScope
  $sourceSnapshotFreeze = Write-SourceSnapshotFreezeRecord -RepoPath $repoPath -Snapshot $sourceSnapshotPreBuild
  Write-Host ("source snapshot: {0} files={1} bytes={2}" -f $sourceSnapshotPreBuild.source_snapshot_digest, $sourceSnapshotPreBuild.file_count, $sourceSnapshotPreBuild.total_bytes)
  Write-Host ("source snapshot freeze record: {0}" -f $sourceSnapshotFreeze.path)
} else {
  # 未声明 scope ⇒ 不计算、也不冒充源码身份（测试夹具不会被强行套上快照语义）。
  Write-Host 'source snapshot: manifest 未声明 source_snapshot，跳过（不冒充源码身份）'
}
Set-PackageStage -Stage 'source-snapshot-pre' -Detail $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.source_snapshot_digest } else { 'not-declared' })
Add-PackageStageRecord -Stage 'source-snapshot-pre' -Outcome $(if ($sourceSnapshotPreBuild) { 'completed' } else { 'skipped' }) -Detail $(if ($sourceSnapshotPreBuild) { 'two-pass quiescent enumeration before build' } else { 'manifest 未声明 source_snapshot：本次不计算源码快照（记为未确认，不冒充稳定）' }) | Out-Null
$sourceSnapshotDigestValue = $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.source_snapshot_digest } else { $null })

# ---------------------------------------------------------------------------
# 构建输入基线（**构建前**计算）：与构建后重算的结果比较，回答"声明的构建输入在本次
# 构建期间是否被改动"。基线必须在任何 artifact 构建之前得到，否则它就只是"构建后
# 又算了一遍"，无法回答这个问题。
# ---------------------------------------------------------------------------
$buildInputDescriptors = Get-BuildInputDescriptors `
  -RepoPath $repoPath `
  -ManifestData $manifestData `
  -ManifestPath $manifestPath `
  -Configuration $Configuration `
  -CargoIdentity $cargoIdentity `
  -SourceSnapshotDigest $(if ($sourceSnapshotDigestValue) { $sourceSnapshotDigestValue } else { 'not-computed' })
$buildInputDigest = Get-BuildInputDigest -DescriptorLines $buildInputDescriptors
$script:releaseContext.buildInputDescriptorsBaseline = $buildInputDescriptors
$script:releaseContext.sourceSnapshotScope = $sourceSnapshotScope
$script:releaseContext.sourceSnapshotPre = $sourceSnapshotPreBuild
$script:releaseContext.cargoIdentity = $cargoIdentity

# ---------------------------------------------------------------------------
# 声明的输入范围（可审查）：哪些文件是构建输入、哪些路径**不属于**构建输入。
# 后者用于裁决第 8 条：已声明不属于构建输入的日志/临时输出变化不得被误判为源码变化。
# ---------------------------------------------------------------------------
$declaredBuildInputFilesList = [System.Collections.Generic.List[string]]::new()
foreach ($path in @($manifestData.'build_inputs'.files)) {
  if ($path) { $declaredBuildInputFilesList.Add((ConvertTo-IdentityPosixPath ((Expand-PackageToken ([string]$path))))) }
}
$declaredBuildInputFilesList.Add('Cargo.lock')
foreach ($artifact in @($manifestData.artifacts)) {
  if (-not $artifact.export) { continue }
  foreach ($path in @($artifact.export.build_entry_manifest, $artifact.export.lockfile)) {
    if ($path) { $declaredBuildInputFilesList.Add((ConvertTo-IdentityPosixPath ((Expand-PackageToken ([string]$path))))) }
  }
  foreach ($path in @($artifact.export.source_identity.files)) {
    if ($path) { $declaredBuildInputFilesList.Add((ConvertTo-IdentityPosixPath ((Expand-PackageToken ([string]$path))))) }
  }
}
$declaredBuildInputFiles = @($declaredBuildInputFilesList | Select-Object -Unique)
$nonBuildInputDeclarations = @(Get-PackageNonBuildInputDeclarations -ManifestData $manifestData)
$nonBuildInputPatterns = @($nonBuildInputDeclarations | ForEach-Object { $_.pattern })

# ---------------------------------------------------------------------------
# RD4-06 准备/冻结分离（裁决 §3.1）
#   派生输出位置：需在构建中产生的中间文件（含本次消费 staging）只能落在这些**已声明**的
#   位置，并记录生成来源。它们不属于构建输入（其身份由 payload_digest / 代次收据承担）。
# ---------------------------------------------------------------------------
$derivedOutputs = [System.Collections.Generic.List[object]]::new()
foreach ($entry in @($manifestData.'preparation'.derived_outputs)) {
  if (-not $entry) { continue }
  $derivedOutputs.Add([pscustomobject]@{
    path = (ConvertTo-IdentityPosixPath ((Expand-PackageToken ([string]$entry.path))))
    generated_by = [string]$entry.generated_by
    reason = [string]$entry.reason
    within_declared_output_root = [bool]($entry.within_declared_output_root -ne $false)
  }) | Out-Null
}
$derivedOutputs.Add([pscustomobject]@{
  path = (ConvertTo-RepoRelativePath $script:consumerStagingRoot)
  generated_by = 'scripts/package-all.ps1 (PKG-L07c consumption staging)'
  reason = '本次打包消费导出产物时的独立 staging：产物先复制到这里并复核内容身份，报告只从本次固定消费收据生成'
  within_declared_output_root = $true
}) | Out-Null
$script:releaseContext.derivedOutputs = @($derivedOutputs)

# 准备阶段声明的"生成式配置步骤"：能在准备阶段生成的配置，生成后**计入冻结输入**。
$preparationSteps = @($manifestData.'preparation'.steps)
$dependencyResolution = $null
$generatedConfigs = [System.Collections.Generic.List[object]]::new()

if ($Prepare) {
  # =========================================================================
  # 准备阶段：解析/更新依赖 → 生成必要配置 → 展示变化并按既有变更政策确认 → 冻结输入快照
  # =========================================================================
  Set-PackageStage -Stage 'prepare-dependency-resolution' -Detail $null
  # 1) 依赖解析/更新：默认只做**只读**解析核对（--locked --offline，等价 --frozen 的只读版），
  #    确认声明输入与锁定内容自洽；确需更新锁定内容时必须显式 -UpdateDependencies。
  $dependencyArgs = [System.Collections.Generic.List[string]]::new()
  $dependencyArgs.Add('metadata'); $dependencyArgs.Add('--format-version'); $dependencyArgs.Add('1')
  if (-not $UpdateDependencies) { $dependencyArgs.Add('--locked') }
  $dependencyArgs.Add('--offline')
  $depStdout = Join-Path $logRoot ('prepare-cargo-metadata-{0}.json' -f (Get-Date -Format 'yyyyMMdd-HHmmssfff'))
  $depStderr = Join-Path $logRoot ('prepare-cargo-metadata-stderr-{0}.log' -f (Get-Date -Format 'yyyyMMdd-HHmmssfff'))
  $depExit = $null
  $depFailure = $null
  Push-Location $repoPath
  try {
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
      & cargo @($dependencyArgs.ToArray()) 1> $depStdout 2> $depStderr
      $depExit = $LASTEXITCODE
    } catch {
      $depFailure = ($_.Exception.Message -split "`r?`n")[0]
    } finally {
      $ErrorActionPreference = $previousErrorAction
    }
  } finally {
    Pop-Location
  }
  $depStderrText = ''
  if (Test-Path -LiteralPath $depStderr -PathType Leaf) { $depStderrText = [string](Get-Content -Raw -LiteralPath $depStderr -ErrorAction SilentlyContinue) }
  $dependencyResolution = [ordered]@{
    command = ('cargo ' + ($dependencyArgs -join ' '))
    mode = $(if ($UpdateDependencies) { 'update-lockfile-explicitly-requested' } else { 'read-only-locked-resolution-check' })
    working_dir = '.'
    exit_code = $depExit
    stdout_log = (ConvertTo-RepoRelativePath $depStdout)
    stderr_log = (ConvertTo-RepoRelativePath $depStderr)
    stderr_summary = (($depStderrText -split "`r?`n" | Where-Object { $_ } | Select-Object -First 5) -join ' | ')
    failure = $depFailure
    lockfile_modified_allowed = [bool]$UpdateDependencies
    note = '准备阶段负责解析/更新依赖；正式构建阶段一律 --locked --offline，锁文件在构建期被改写即失败（不设白名单）'
  }
  $script:releaseContext.dependencyResolution = $dependencyResolution
  if ($depFailure -or ($null -ne $depExit -and $depExit -ne 0)) {
    Add-PackageStageRecord -Stage 'prepare-dependency-resolution' -Command 'cargo' -ExitCode $depExit -Outcome 'failed' -Detail 'DEPENDENCY-RESOLUTION-FAILED' | Out-Null
    throw ("[DEPENDENCY-RESOLUTION-FAILED] configuration={0}`ndetail: 准备阶段的依赖解析未通过（exit={1}）：{2}`nnext: 先让锁定内容与声明依赖自洽（在准备阶段显式使用 -UpdateDependencies 解析后重新冻结），再进入正式构建；不要在正式构建阶段改锁文件" -f $Configuration, $depExit, (($depStderrText -split "`r?`n" | Where-Object { $_ } | Select-Object -First 3) -join ' | '))
  }
  Add-PackageStageRecord -Stage 'prepare-dependency-resolution' -Command 'cargo' -ExitCode $depExit -Outcome 'completed' -Detail ('依赖解析核对通过（mode={0}）' -f $dependencyResolution.mode) | Out-Null

  # 2) 生成必要配置：逐步骤执行，生成物**计入冻结输入**。
  Set-PackageStage -Stage 'prepare-generate-configs' -Detail $null
  foreach ($step in @($preparationSteps)) {
    if (-not $step) { continue }
    $stepId = [string]$step.id
    $stepCommand = [string]$step.command
    $stepArgs = @($step.args | ForEach-Object { Expand-PackageToken ([string]$_) })
    $stepDir = if ($step.working_dir) { Resolve-RepoPath ([string]$step.working_dir) } else { $repoPath }
    if (-not $stepCommand) { throw ("[PREPARE-STEP-INVALID] configuration={0}`ndetail: 准备阶段步骤 {1} 缺少 command`nnext: 修正 manifest 的 preparation.steps" -f $Configuration, $stepId) }
    Write-Host ("prepare step {0}: {1} {2}" -f $stepId, $stepCommand, ($stepArgs -join ' '))
    Push-Location $stepDir
    try {
      $previousErrorAction = $ErrorActionPreference
      $ErrorActionPreference = 'Continue'
      try { & $stepCommand @stepArgs; $stepExit = $LASTEXITCODE } finally { $ErrorActionPreference = $previousErrorAction }
    } finally {
      Pop-Location
    }
    Add-PackageStageRecord -Stage 'prepare-generate-configs' -Command $stepCommand -ExitCode $stepExit -Outcome $(if ($stepExit -eq 0) { 'completed' } else { 'failed' }) -Detail $stepId | Out-Null
    if ($stepExit -ne 0) {
      throw ("[PREPARE-STEP-FAILED] configuration={0}`ndetail: 准备阶段步骤 {1} 退出码 {2}`nnext: 修复该步骤后重新执行准备阶段" -f $Configuration, $stepId, $stepExit)
    }
    foreach ($generated in @($step.generates)) {
      if (-not $generated) { continue }
      $generatedFull = Resolve-RepoPath ([string]$generated)
      if (-not (Test-Path -LiteralPath $generatedFull -PathType Leaf)) {
        throw ("[GENERATED-CONFIG-MISSING] configuration={0}`ndetail: 准备阶段步骤 {1} 声明生成 {2}，但该文件不存在`nnext: 修正生成步骤；**不允许**把未生成的配置当作冻结输入" -f $Configuration, $stepId, $generated)
      }
      $generatedConfigs.Add([pscustomobject]@{
        path = (ConvertTo-IdentityPosixPath (ConvertTo-RepoRelativePath $generatedFull))
        length = [long](Get-Item -LiteralPath $generatedFull -Force).Length
        sha256 = Get-OptionalHash $generatedFull
        generated_by = ('preparation.steps[{0}]' -f $stepId)
      }) | Out-Null
    }
  }
  $script:releaseContext.generatedConfigs = @($generatedConfigs)
  Add-PackageStageRecord -Stage 'prepare-generate-configs' -Outcome 'completed' -Detail ('生成式配置 {0} 个（生成后计入冻结输入）' -f @($generatedConfigs).Count) | Out-Null

  # 3) 展示变化并按既有变更政策确认：与上一次冻结记录逐项比较（前后摘要 + 发生阶段）。
  Set-PackageStage -Stage 'prepare-change-review' -Detail $null
  $previousFreezeDocument = $null
  $previousFreezePointer = Join-Path (Get-PackageInputFreezeRoot -RepoPath $repoPath) ('latest-{0}.json' -f $Configuration)
  if (Test-Path -LiteralPath $previousFreezePointer -PathType Leaf) {
    try {
      $previousPointerDocument = Get-Content -Raw -LiteralPath $previousFreezePointer -Encoding UTF8 | ConvertFrom-Json
      $previousFreezeFull = Resolve-RepoPath ([string]$previousPointerDocument.freeze_record)
      if (Test-Path -LiteralPath $previousFreezeFull -PathType Leaf) {
        $previousFreezeDocument = Get-Content -Raw -LiteralPath $previousFreezeFull -Encoding UTF8 | ConvertFrom-Json
      }
    } catch {
      Write-Warning ('上一次冻结记录无法解析（按"无历史"处理）：{0}' -f (($_.Exception.Message -split "`r?`n")[0]))
    }
  }
  $prepareChanges = @()
  $previousFreezeMatched = ($null -ne $previousFreezeDocument)
  if ($previousFreezeDocument) {
    # 只有"同一份 manifest"的冻结记录才构成这个输入集合的"上一次冻结"：
    # 不同 manifest（不同输入集合）之间比较没有意义，按"无历史"处理并如实记录。
    $currentManifestSha = Get-OptionalHash $manifestPath
    if ([string]$previousFreezeDocument.package_manifest_sha256 -ne [string]$currentManifestSha) {
      $previousFreezeMatched = $false
      Write-Warning ('上一次冻结记录属于另一份 manifest（previous={0} current={1}），按"无历史"处理' -f [string]$previousFreezeDocument.package_manifest_sha256, [string]$currentManifestSha)
      $previousFreezeDocument = $null
    }
  }
  if ($previousFreezeDocument) {
    $prepareComparison = Compare-PackageInputFreeze `
      -Frozen $previousFreezeDocument `
      -CurrentDescriptorLines $buildInputDescriptors `
      -CurrentSourceSnapshotDigest $sourceSnapshotDigestValue `
      -CurrentGeneratedConfigs @($generatedConfigs) `
      -Phase 'prepare'
    $prepareChanges = @($prepareComparison.changed_paths)
  }
  foreach ($entry in @($prepareChanges)) {
    Write-Host ("prepare change: {0} [{1}] phase={2} before={3} after={4}" -f $entry.path, $entry.kind, $entry.phase, $entry.before_summary, $entry.after_summary)
  }
  $script:releaseContext.changedPaths = @($prepareChanges)
  Add-PackageStageRecord -Stage 'prepare-change-review' -Outcome $(if (@($prepareChanges).Count -gt 0 -and -not $AcceptPreparationChanges) { 'failed' } else { 'completed' }) -Detail ('准备阶段变化 {0} 处（previous_freeze_matched={1} changes_accepted={2}）' -f @($prepareChanges).Count, $previousFreezeMatched, [bool]$AcceptPreparationChanges) | Out-Null
  if (@($prepareChanges).Count -gt 0 -and -not $AcceptPreparationChanges) {
    # 变化必须被显式确认：未确认即拒绝（不静默放行、不把变化从清单里移除换取绿色）。
    $changedSummary = (@($prepareChanges) | ForEach-Object { ('{0}({1})' -f $_.path, $_.kind) }) -join ', '
    throw ("[PREPARE-INPUT-CHANGED] configuration={0}`ndetail: 准备阶段观察到 {1} 处声明输入变化，尚未按变更政策确认：{2}`nnext: 核对上面列出的路径与前后摘要；确认接受这些变化后重跑准备阶段并加 -AcceptPreparationChanges，然后重新冻结。不要通过删减输入清单来换取 release_eligible=true" -f $Configuration, @($prepareChanges).Count, $changedSummary)
  }

  # 4) 冻结完整输入快照（内容寻址、一次性写入）+ 更新冻结指针。
  Set-PackageStage -Stage 'prepare-freeze' -Detail $null
  $freezeDocument = New-PackageInputFreezeDocument `
    -Configuration $Configuration `
    -RunId $script:packageRunId `
    -RepoPath $repoPath `
    -ManifestPath $manifestPath `
    -SourceSnapshotDigest $sourceSnapshotDigestValue `
    -BuildInputDigest $buildInputDigest `
    -BuildInputDescriptorLines $buildInputDescriptors `
    -CargoIdentity $cargoIdentity `
    -Preparation ([ordered]@{
      dependency_resolution = $dependencyResolution
      steps = @($preparationSteps | ForEach-Object { [string]$_.id })
      generated_config_count = @($generatedConfigs).Count
    }) `
    -GeneratedConfigs @($generatedConfigs) `
    -DerivedOutputs @($derivedOutputs) `
    -PreviousFreeze @($previousFreezeDocument) `
    -ChangedPaths @($prepareChanges) `
    -ChangesAccepted ([bool]$AcceptPreparationChanges)
  $freezeRecord = Write-PackageInputFreezeRecord -RepoPath $repoPath -Document $freezeDocument -UpdatePointer
  $script:releaseContext.freezeDocument = $freezeDocument
  $script:releaseContext.freezeRecord = $freezeRecord
  Write-Host ("input freeze: {0} (freeze_id={1} inputs={2} generated_configs={3})" -f $freezeRecord.path, $freezeRecord.freeze_id, @($freezeDocument.build_input_descriptor_lines).Count, @($generatedConfigs).Count)
  Add-PackageStageRecord -Stage 'prepare-freeze' -Outcome 'completed' -Detail ('冻结输入快照已写入（freeze_id={0}）；准备阶段到此结束，不产出可发布包' -f $freezeRecord.freeze_id) | Out-Null

  # 准备阶段记录（受同一报告身份治理）：不是发布收据。
  $prepareReportParent = if ($ReportPath) {
    $resolvedPrepareSource = if ([System.IO.Path]::IsPathRooted($ReportPath)) { [System.IO.Path]::GetFullPath($ReportPath) } else { [System.IO.Path]::GetFullPath((Join-Path $repoPath $ReportPath)) }
    Join-Path (Split-Path -Parent $resolvedPrepareSource) $script:PackagePrepareReportSubdir
  } else {
    Get-PackagePrepareReportRoot -RepoPath $repoPath
  }
  New-Item -ItemType Directory -Force -Path $prepareReportParent | Out-Null
  $prepareStamp = Get-Date -Format 'yyyyMMdd-HHmmssfff'
  $prepareReportPath = Join-Path $prepareReportParent ("package-prepare-$Configuration-$prepareStamp.json")
  $prepareReportId = Get-PackageReportId -Configuration $Configuration -Prefix 'pkg-prepare' -Stamp $prepareStamp
  $prepareGates = [System.Collections.Generic.List[object]]::new()
  $prepareGates.Add((New-PackageReleaseGate -Name 'dependency_resolution' -Result 'pass' -Evidence $dependencyResolution.command)) | Out-Null
  $prepareGates.Add((New-PackageReleaseGate -Name 'generated_configs' -Result 'pass' -Evidence ('生成式配置 {0} 个（生成后计入冻结输入）' -f @($generatedConfigs).Count))) | Out-Null
  $prepareGates.Add((New-PackageReleaseGate -Name 'input_changes_confirmed' -Result 'pass' -Evidence ('变化 {0} 处（changes_accepted={1}）' -f @($prepareChanges).Count, [bool]$AcceptPreparationChanges))) | Out-Null
  $prepareGates.Add((New-PackageReleaseGate -Name 'input_snapshot_frozen' -Result 'pass' -Evidence ('freeze_id={0}' -f $freezeRecord.freeze_id))) | Out-Null
  $prepareRecord = New-PackagePreparationRecord `
    -RunId $script:packageRunId -Configuration $Configuration -RepoPath $repoPath -ManifestPath $manifestPath `
    -Outcome 'prepared' -Freeze (ConvertTo-PackageReportFreezeReference $freezeRecord) -FreezeDocument $freezeDocument `
    -DependencyResolution $dependencyResolution -GeneratedConfigs @($generatedConfigs) `
    -ChangedPaths @($prepareChanges) -GateResults @($prepareGates.ToArray()) `
    -Reasons @()
  $prepareReport = Write-GovernedPackageReport -Report $prepareRecord -Path $prepareReportPath -RepoPath $repoPath -ReportId $prepareReportId
  Write-Host ("prepare record: {0} (report_id={1} release_eligible=false)" -f $prepareReport.report_path, $prepareReport.report_id)
  return [pscustomobject]@{
    phase = 'prepare'
    run_id = $script:packageRunId
    freeze_record = $freezeRecord.path
    freeze_id = $freezeRecord.freeze_id
    changed_path_count = @($prepareChanges).Count
    generated_config_count = @($generatedConfigs).Count
    prepare_report = $prepareReport.report_path
    release_eligible = $false
    next = '正式构建阶段：-FreezeRecordPath ' + $freezeRecord.path
  }
}

# ---------------------------------------------------------------------------
# 正式构建阶段消费冻结输入（裁决 §3.1）：显式 -FreezeRecordPath 时逐项核对"冻结输入在构建
# 开始前是否仍一致"；任一声明输入改变即拒绝发布（并给出改变路径、前后摘要与发生阶段）。
# 未显式给出时，本次构建在自身开始处**自冻结**输入（记为 self-frozen-at-build-start），
# 随后仍以"构建前后两次采样逐项一致"证明输入稳定。
# ---------------------------------------------------------------------------
Set-PackageStage -Stage 'build-frozen-input-check' -Detail $null
$freezeConsumption = $null
if ($FreezeRecordPath) {
  $freezeFull = if ([System.IO.Path]::IsPathRooted($FreezeRecordPath)) { $FreezeRecordPath } else { Join-Path $repoPath $FreezeRecordPath }
  $frozenDocument = Read-PackageInputFreezeRecord -Path ([System.IO.Path]::GetFullPath($freezeFull))
  # 准备阶段生成的配置：构建阶段必须**重算其当前摘要**再与冻结值比较（生成后即计入冻结输入），
  # 而不是假定"构建阶段没有生成动作就等于它消失了"。
  $currentGeneratedConfigs = [System.Collections.Generic.List[object]]::new()
  foreach ($generatedEntry in @($frozenDocument.generated_configs)) {
    if (-not $generatedEntry -or -not $generatedEntry.path) { continue }
    $generatedFullPath = Resolve-RepoPath ([string]$generatedEntry.path)
    if (-not (Test-Path -LiteralPath $generatedFullPath -PathType Leaf)) { continue }
    $currentGeneratedConfigs.Add([pscustomobject]@{
      path = [string]$generatedEntry.path
      length = [long](Get-Item -LiteralPath $generatedFullPath -Force).Length
      sha256 = Get-OptionalHash $generatedFullPath
      generated_by = [string]$generatedEntry.generated_by
    }) | Out-Null
  }
  $freezeConsumption = Compare-PackageInputFreeze `
    -Frozen $frozenDocument `
    -CurrentDescriptorLines $buildInputDescriptors `
    -CurrentSourceSnapshotDigest $sourceSnapshotDigestValue `
    -CurrentGeneratedConfigs @($currentGeneratedConfigs) `
    -Phase 'build-preflight'
  $script:releaseContext.freezeDocument = $frozenDocument
  $script:releaseContext.freezeComparison = $freezeConsumption
  $script:releaseContext.frozenInputsMode = 'consumed-preparation-freeze-record'
  foreach ($entry in @($freezeConsumption.changed_paths)) {
    Write-Host ("frozen input changed: {0} [{1}] phase={2} before={3} after={4}" -f $entry.path, $entry.kind, $entry.phase, $entry.before_summary, $entry.after_summary)
  }
  if (-not $freezeConsumption.identical) {
    Add-PackageStageRecord -Stage 'build-frozen-input-check' -Outcome 'failed' -Detail ('冻结输入在正式构建阶段发生变化：{0} 处' -f $freezeConsumption.changed_path_count) | Out-Null
    throw ("[FROZEN-INPUT-CHANGED] configuration={0}`ndetail: 正式构建阶段的声明输入与冻结输入（freeze_id={1}）不一致（{2} 处）：{3}`nnext: 回到准备阶段重新核对并重新冻结（不要通过删减输入清单换取绿色结果）；确认新输入后再用新的冻结记录进入正式构建" -f $Configuration, $frozenDocument.freeze_id, $freezeConsumption.changed_path_count, ((@($freezeConsumption.changed_paths) | ForEach-Object { ('{0}({1}, phase={2}, before={3}, after={4})' -f $_.path, $_.kind, $_.phase, $_.before_summary, $_.after_summary) }) -join '; '))
  }
  Add-PackageStageRecord -Stage 'build-frozen-input-check' -Outcome 'completed' -Detail ('冻结输入在构建开始前一致（freeze_id={0}，{1} 项）' -f $frozenDocument.freeze_id, @($frozenDocument.build_input_descriptor_lines).Count) | Out-Null
} else {
  # 单阶段运行：在自身开始处冻结（构建输入基线即冻结输入），如实标注来源。
  $selfFreezeDocument = New-PackageInputFreezeDocument `
    -Configuration $Configuration `
    -RunId $script:packageRunId `
    -RepoPath $repoPath `
    -ManifestPath $manifestPath `
    -SourceSnapshotDigest $sourceSnapshotDigestValue `
    -BuildInputDigest $buildInputDigest `
    -BuildInputDescriptorLines $buildInputDescriptors `
    -CargoIdentity $cargoIdentity `
    -GeneratedConfigs @($generatedConfigs) `
    -DerivedOutputs @($derivedOutputs) `
    -PreviousFreeze @() -ChangedPaths @() -ChangesAccepted $false
  $selfFreezeRecord = Write-PackageInputFreezeRecord -RepoPath $repoPath -Document $selfFreezeDocument
  $script:releaseContext.freezeDocument = $selfFreezeDocument
  $script:releaseContext.freezeRecord = $selfFreezeRecord
  $script:releaseContext.frozenInputsMode = 'self-frozen-at-build-start'
  Add-PackageStageRecord -Stage 'build-frozen-input-check' -Outcome 'completed' -Detail ('未提供 -FreezeRecordPath：本次构建在自身开始处自冻结输入（freeze_id={0}）；这是单阶段运行，正式发布应走 -Prepare → -FreezeRecordPath' -f $selfFreezeRecord.freeze_id) | Out-Null
}

if ($SkipBuild) {
  # fail fast：--no-build 的收据校验放在任何发布之前，避免产出半成品 staging。
  foreach ($artifact in @($manifestData.artifacts)) {
    if (-not $artifact.export) { continue }
    $preflightContext = New-LoaderExportContext -Artifact $artifact -Export $artifact.export
    [void](Assert-LoaderExportReceipt -Context $preflightContext -VerifyIdentity)
    Write-Host "no-build preflight: export receipt verified for $($artifact.id)"
    Add-PackageStageRecord -Stage 'artifact-export-preflight' -ArtifactId ([string]$artifact.id) -Outcome 'completed' -Detail 'existing-receipt-verified (fail fast，先于任何暂存/复制)' | Out-Null
  }
}
Set-PackageStage -Stage 'artifact-loop' -Detail $null
foreach ($artifact in @($manifestData.artifacts)) {
  Invoke-ArtifactBuild -Artifact $artifact
  if ($artifact.export) {
    # 导出/校验必须在消费该 artifact 之前完成；manifest 顺序保证构建入口已构建过。
    Invoke-ArtifactExport -Artifact $artifact
  }
  $results += Publish-Artifact -Artifact $artifact
}

# ---------------------------------------------------------------------------
# 消费关联一致性硬检查（PKG-L07c 验收项 7）：报告里的代次字段必须与**包内实际字节**一致，
# 不只是"字段存在"。这里逐条核对：
#   exported_artifacts[].artifact_digest == 实际放进包根的产物内容身份（刚复制后重算）
#   exported_artifacts[].consumption.staged_sha256 == artifact_digest（本次独立 staging）
# 任一不符即拒绝发布（不允许"报告写 G1、包里是 G2"这种漂移进入正式包）。
# ---------------------------------------------------------------------------
$consumptionMismatches = [System.Collections.Generic.List[string]]::new()
foreach ($summary in @($script:exportSummaries)) {
  $published = @($results | Where-Object { [string]$_.id -eq [string]$summary.artifact_id })[0]
  if (-not $published) {
    $consumptionMismatches.Add(('artifact={0}: 没有对应的发布结果' -f $summary.artifact_id))
    continue
  }
  $targetFull = Join-Path $resolvedPackageRoot ([string]$published.target).Replace('/', '\')
  $targetIdentity = Get-OptionalHash $targetFull
  if (-not $targetIdentity) {
    $consumptionMismatches.Add(('artifact={0}: 包内目标不存在（{1}）' -f $summary.artifact_id, $published.target))
    continue
  }
  if ($targetIdentity -ne [string]$summary.artifact_digest) {
    $consumptionMismatches.Add(('artifact={0}: 包内字节与该代次 artifact_digest 不一致（package={1} exported_artifacts.artifact_digest={2} generation={3}）' -f $summary.artifact_id, $targetIdentity, $summary.artifact_digest, $summary.generation))
  }
  if ([string]$summary.consumption.staged_sha256 -ne [string]$summary.artifact_digest) {
    $consumptionMismatches.Add(('artifact={0}: 消费 staging 内容身份与 artifact_digest 不一致（staged={1} artifact_digest={2}）' -f $summary.artifact_id, $summary.consumption.staged_sha256, $summary.artifact_digest))
  }
  if ([string]$summary.consumption.consumer_run_id -ne $script:packageRunId) {
    $consumptionMismatches.Add(('artifact={0}: 消费收据的 consumer_run_id 不是本次运行（{1}）' -f $summary.artifact_id, $summary.consumption.consumer_run_id))
  }
}
if ($consumptionMismatches.Count -gt 0) {
  Add-PackageStageRecord -Stage 'consumption-consistency' -Outcome 'failed' -Detail (@($consumptionMismatches) -join '; ') | Out-Null
  throw ("[CONSUMPTION-CONTENT-DRIFT] profile={0}`ndetail: 报告的代次字段与包内实际字节不一致（{1} 条）：{2}`nnext: 不要发布该包；检查是否有并发发布者在写同一输出槽位，或消费 staging 被第三方改写" -f $Configuration, $consumptionMismatches.Count, (@($consumptionMismatches) -join '; '))
}
Add-PackageStageRecord -Stage 'consumption-consistency' -Outcome 'completed' -Detail ('exported_artifacts 的代次字段与包内字节逐条一致（{0} 个导出物）' -f @($script:exportSummaries).Count) | Out-Null


Set-PackageStage -Stage 'resource-copy' -Detail $null
foreach ($resource in @($manifestData.resources)) {
  Copy-PackageResource -Resource $resource
}
Add-PackageStageRecord -Stage 'resource-copy' -Outcome 'completed' -Detail ('resources copied: {0}' -f @($manifestData.resources).Count) | Out-Null

# ---------------------------------------------------------------------------
# 构建后复核：与构建前的冻结快照**逐文件**比较（含新增/删除）。
# 这不是"首尾各哈希一次"：构建后再做一次两遍静默枚举，任何并发编辑都会以
# added/removed/modified 的具体路径在这里 fail-closed，而不是被当成一致。
# ---------------------------------------------------------------------------
$sourceSnapshotVerification = $null
$snapshotDiff = $null
$sourceSnapshotPostBuild = $null
Set-PackageStage -Stage 'post-build-source-verification' -Detail $null
if ($sourceSnapshotScope) {
  $sourceSnapshotPostBuild = New-SourceSnapshot -RepoPath $repoPath -Scope $sourceSnapshotScope
  $snapshotDiff = Compare-SourceSnapshot -Left $sourceSnapshotPreBuild -Right $sourceSnapshotPostBuild
  $sourceSnapshotVerification = [ordered]@{
    checked_at_utc = (Get-Date).ToUniversalTime().ToString('o')
    method = 'two-pass-quiescent-enumeration-before-and-after-build'
    pre_build_digest = $sourceSnapshotPreBuild.source_snapshot_digest
    post_build_digest = $sourceSnapshotPostBuild.source_snapshot_digest
    identical = $snapshotDiff.identical
    added_count = $snapshotDiff.added_count
    removed_count = $snapshotDiff.removed_count
    modified_count = $snapshotDiff.modified_count
    added_paths = @($snapshotDiff.added)
    removed_paths = @($snapshotDiff.removed)
    modified_paths = @($snapshotDiff.modified)
    freeze_record = $sourceSnapshotFreeze.path
    freeze_record_sha256 = $sourceSnapshotFreeze.content_sha256
    evidence_kind = 'observed-between-two-samples'
    evidence_note = '这是"构建前后两次独立采样之间的差异"，属于**已观察变化**，不是完整写入历史。'
  }
  $script:releaseContext.sourceSnapshotPost = $sourceSnapshotPostBuild
  $script:releaseContext.snapshotComparison = $snapshotDiff
  if (-not $snapshotDiff.identical) {
    Add-PackageStageRecord -Stage 'post-build-source-verification' -Outcome 'failed' -Detail ($snapshotDiff.summary_lines -join '; ') | Out-Null
    throw ("[SOURCE-SNAPSHOT-CHANGED] profile=$Configuration`ndetail: 构建期间源码快照发生变化：{0}`nnext: 停止并发编辑活动树（或在冻结/受控不变输入集合上构建）后重跑；不要把首尾各哈希一次的结果当成构建过程一致" -f ($snapshotDiff.summary_lines -join '; '))
  }
  Add-PackageStageRecord -Stage 'post-build-source-verification' -Outcome 'completed' -Detail ('两次采样逐文件一致：{0}' -f $sourceSnapshotPreBuild.source_snapshot_digest) | Out-Null
  Write-Host ("source snapshot verified unchanged: {0}" -f $sourceSnapshotPreBuild.source_snapshot_digest)
} else {
  Add-PackageStageRecord -Stage 'post-build-source-verification' -Outcome 'skipped' -Detail 'manifest 未声明 source_snapshot ⇒ 无法判定输入是否被改动（记为未确认，不写成一致）' | Out-Null
}

# ---------------------------------------------------------------------------
# 声明的构建输入重算与比较（裁决第 7 条）：构建输入在本次构建期间被改动 ⇒ 拒绝发布。
# 这里比较的是**描述符列表**（含每个输入文件的 sha256），所以差异是具体到文件的。
# ---------------------------------------------------------------------------
Set-PackageStage -Stage 'build-input-recheck' -Detail $null
$buildInputComparison = $null
$buildInputUnconfirmedReason = $null
try {
  $buildInputDescriptorsPost = Get-BuildInputDescriptors `
    -RepoPath $repoPath `
    -ManifestData $manifestData `
    -ManifestPath $manifestPath `
    -Configuration $Configuration `
    -CargoIdentity $cargoIdentity `
    -SourceSnapshotDigest $(if ($sourceSnapshotDigestValue) { $sourceSnapshotDigestValue } else { 'not-computed' })
  $buildInputComparison = Compare-BuildInputDescriptors -BaselineLines $buildInputDescriptors -CurrentLines $buildInputDescriptorsPost
} catch {
  # 重算失败 = 无法确认输入是否被改动 ⇒ 明确记"未能确认"，绝不乐观放行。
  $buildInputUnconfirmedReason = ('构建输入重算失败（未能确认）：{0}' -f (($_.Exception.Message -split "`n")[0]))
}
$script:releaseContext.buildInputComparison = $buildInputComparison
$script:releaseContext.buildInputUnconfirmedReason = $(if ($buildInputUnconfirmedReason) { $buildInputUnconfirmedReason } else { 'not-computed' })
if ($buildInputComparison) {
  if (-not $buildInputComparison.identical) {
    Add-PackageStageRecord -Stage 'build-input-recheck' -Outcome 'failed' -Detail ('声明的构建输入发生变化：{0}' -f ($buildInputComparison.changed_descriptor_lines -join '; ')) | Out-Null
    throw ("[BUILD-INPUT-CHANGED] profile=$Configuration`ndetail: 构建期间**声明的构建输入**发生变化（{0} 条描述符差异）：{1}`nnext: 停止并发修改锁文件/构建入口/安装器定义后重跑；本次产物属于混合输入产物，不允许发布" -f @($buildInputComparison.changed_descriptor_lines).Count, ($buildInputComparison.changed_descriptor_lines -join '; '))
  }
  Add-PackageStageRecord -Stage 'build-input-recheck' -Outcome 'completed' -Detail ('构建输入描述符前后一致：{0}' -f $buildInputComparison.baseline_digest) | Out-Null
} else {
  Add-PackageStageRecord -Stage 'build-input-recheck' -Outcome 'not-confirmed' -Detail $buildInputUnconfirmedReason | Out-Null
}

# ---------------------------------------------------------------------------
# 产物来源（裁决第 9 条）：每个产物都要回答"它是不是由对**本次那份快照**做过的、
# 事后通过校验的构建产出的"。
#   * 构建模式：产物由本次运行构建，且构建所用的树已被证明前后字节一致 ⇒ 可确认；
#     声明了 export 的产物还要看收据里"生产者是否在本次调用中真正重跑"。
#   * --no-build：只接受绑定到**本次快照 + 构建输入摘要**的产物—快照关联记录；
#     关联缺失 ⇒ 未能确认（不是通过，也不冒充通过）。
# ---------------------------------------------------------------------------
Set-PackageStage -Stage 'artifact-provenance' -Detail $null
$releaseStatusDocument = Get-ReleaseStatusDocument
# ---------------------------------------------------------------------------
# PR-PKG-02（第八轮裁决 · 来源模型缺口）：**外部构建输入登记**。
#
# 某些随包二进制来自第三方 registry crate 的构建脚本（例：`WebView2Loader.dll` 由
# `webview2-com-sys` 分发）。第一方源码快照**无法**覆盖这类字节，所以既不能假装它来自源码，
# 也不能把它当成"未确认随机文件"长期挂着——正确做法是登记为**外部构建输入**：
# 来源（registry）、crate、**版本**与 **checksum** 全部固定，并与 `Cargo.lock` 里该
# crate@version 的 checksum **逐字比对**（独立来源，不是自证）。
# 只有整条链走通，产物来源才记为 `declared-external-input-verified`。
# ---------------------------------------------------------------------------
function Resolve-ExternalBuildInputVerification {
  param(
    [AllowNull()][string]$PackageId,
    [object]$ManifestData,
    [Parameter(Mandatory = $true)][string]$WorkspacePath
  )
  $result = [ordered]@{
    verified = $false
    reason = $null
    id = $null
    crate = $null
    version = $null
    checksum = $null
    registry_source = $null
    checksum_authority = $null
  }
  if ([string]::IsNullOrWhiteSpace($PackageId)) {
    $result.reason = 'package-id-missing'
    return [pscustomobject]$result
  }
  $match = [regex]::Match($PackageId, '^registry\+(?<src>[^#]+)#(?<crate>[^@]+)@(?<version>.+)$')
  if (-not $match.Success) {
    $result.reason = 'package-id-not-a-registry-crate'
    return [pscustomobject]$result
  }
  $crate = $match.Groups['crate'].Value
  $version = $match.Groups['version'].Value.Trim()
  $registrySource = $match.Groups['src'].Value
  $result.crate = $crate
  $result.version = $version
  $result.registry_source = $registrySource

  $entry = $null
  foreach ($candidate in @($ManifestData.external_build_inputs)) {
    if ($candidate -and ([string]$candidate.source.crate) -eq $crate -and ([string]$candidate.source.version) -eq $version) {
      $entry = $candidate
      break
    }
  }
  if (-not $entry) {
    $result.reason = ('manifest 未登记该外部构建输入：{0}@{1}' -f $crate, $version)
    return [pscustomobject]$result
  }
  $declared = ([string]$entry.source.checksum).Trim().ToLowerInvariant()
  if ([string]::IsNullOrWhiteSpace($declared)) {
    $result.reason = ('登记项缺少 checksum：{0}' -f [string]$entry.id)
    return [pscustomobject]$result
  }
  # 两侧都归一到"去掉可选 registry+ 前缀"的形式再比（产物 id 与 manifest 可能各自带/不带该前缀）。
  $declaredRegistry = (([string]$entry.source.registry_source) -replace '^registry\+', '').Trim()
  $observedRegistry = ($registrySource -replace '^registry\+', '').Trim()
  if (-not [string]::IsNullOrWhiteSpace($declaredRegistry) -and $declaredRegistry -ne $observedRegistry) {
    $result.reason = ('registry source 不符：manifest {0} / 产物 {1}' -f $declaredRegistry, $observedRegistry)
    return [pscustomobject]$result
  }
  $result.id = [string]$entry.id
  $result.checksum = $declared
  $result.checksum_authority = [string]$entry.source.checksum_authority

  # 与 Cargo.lock 逐字比对（独立来源）。
  $lockRel = [string]$entry.source.checksum_lockfile
  if ([string]::IsNullOrWhiteSpace($lockRel)) {
    $result.reason = ('登记项缺少 checksum_lockfile（checksum 必须有独立权威来源）：{0}' -f [string]$entry.id)
    return [pscustomobject]$result
  }
  $lockPath = if ([System.IO.Path]::IsPathRooted($lockRel)) { $lockRel } else { Join-Path $WorkspacePath ($lockRel.Replace('/', '\')) }
  if (-not (Test-Path -LiteralPath $lockPath -PathType Leaf)) {
    $result.reason = ('checksum 权威文件不存在：{0}' -f $lockRel)
    return [pscustomobject]$result
  }
  $lockText = Get-Content -Raw -LiteralPath $lockPath -Encoding UTF8
  $lockMatch = [regex]::Match($lockText, ('(?s)\[\[package\]\]\s*name\s*=\s*"' + [regex]::Escape($crate) + '"\s*version\s*=\s*"' + [regex]::Escape($version) + '"(?<body>.*?)(\n\[\[|\z)'))
  if (-not $lockMatch.Success) {
    $result.reason = ('Cargo.lock 里找不到 {0}@{1}' -f $crate, $version)
    return [pscustomobject]$result
  }
  $checksumMatch = [regex]::Match($lockMatch.Groups['body'].Value, 'checksum\s*=\s*"(?<sum>[0-9a-fA-F]+)"')
  if (-not $checksumMatch.Success) {
    $result.reason = ('Cargo.lock 里 {0}@{1} 没有 checksum' -f $crate, $version)
    return [pscustomobject]$result
  }
  $lockChecksum = $checksumMatch.Groups['sum'].Value.Trim().ToLowerInvariant()
  if ($lockChecksum -ne $declared) {
    $result.reason = ('checksum 不符：manifest {0} / Cargo.lock {1}' -f $declared, $lockChecksum)
    return [pscustomobject]$result
  }
  $result.verified = $true
  return [pscustomobject]$result
}

$artifactProvenance = [System.Collections.Generic.List[object]]::new()
$externalInputsVerified = [System.Collections.Generic.List[object]]::new()
$provenanceUnconfirmed = [System.Collections.Generic.List[string]]::new()
$provenanceLimitations = [System.Collections.Generic.List[string]]::new()
$scopeDeclared = ($null -ne $sourceSnapshotScope)
foreach ($result in @($results)) {
  $artifactRecord = @($manifestData.artifacts | Where-Object { [string]$_.id -eq [string]$result.id })[0]
  $isExport = ($null -ne $artifactRecord.export)
  $exportSummary = @($script:exportSummaries | Where-Object { [string]$_.artifact_id -eq [string]$result.id })[0]
  $stableExportSha = $(if ($exportSummary) { [string]$exportSummary.stable_export_sha256 } else { $null })
  $receiptPath = $(if ($exportSummary) { [string]$exportSummary.receipt } else { $null })
  $rerunValue = $null
  if ($exportSummary -and $exportSummary.PSObject.Properties['producer_build_script_rerun_in_this_invocation']) {
    $rerunValue = $exportSummary.producer_build_script_rerun_in_this_invocation
  }
  $entry = [ordered]@{
    artifact_id = [string]$result.id
    target = [string]$result.target
    source_path = [string]$result.source
    source_sha256 = [string]$result.sha256
    copied = [bool]$result.copied
    has_export = $isExport
    receipt = $receiptPath
    stable_export_sha256 = $stableExportSha
    producer_build_script_rerun = (ConvertTo-IdentityBoolOrUnknown -Value $rerunValue)
    build_mode = $(if ($SkipBuild) { 'no-build' } else { 'build' })
    # 该产物文件是否在本次运行期间被重写（mtime 证据）：只有这种证据才允许清理禁用记录。
    rebuilt_in_this_run = $false
    source_classification = 'undetermined'
    source_in_declared_snapshot_scope = $false
    confirmation = 'not-confirmed'
    confirmation_reason = $null
    association = $null
  }
  $sourceFullPath = Join-Path $repoPath (([string]$result.source).Replace('/', '\'))
  if (Test-Path -LiteralPath $sourceFullPath -PathType Leaf) {
    $sourceWriteTime = (Get-Item -LiteralPath $sourceFullPath -Force).LastWriteTimeUtc
    $entry.rebuilt_in_this_run = ($sourceWriteTime -ge $script:packageRunStartedUtc)
  }
  $sourceClassification = Get-PackagePathClassification `
    -Path ([string]$result.source) `
    -Scope $sourceSnapshotScope `
    -BuildInputFiles $declaredBuildInputFiles `
    -NonBuildInputPatterns $nonBuildInputPatterns
  $entry.source_classification = $sourceClassification.classification
  $entry.source_in_declared_snapshot_scope = ($sourceClassification.classification -eq 'in-declared-source-snapshot-scope')
  if ($SkipBuild) {
    if (-not $scopeDeclared) {
      $entry.confirmation_reason = 'no-declared-source-snapshot-scope：未声明源码快照范围，无法把该产物绑定到任何一份快照'
    } else {
      $association = Get-PackageArtifactAssociation `
        -StatusDocument $releaseStatusDocument `
        -ArtifactId ([string]$result.id) `
        -Profile $Configuration `
        -SourcePath ([string]$result.source) `
        -SourceSha256 ([string]$result.sha256) `
        -StableExportSha256 $stableExportSha `
        -SourceSnapshotDigest $sourceSnapshotDigestValue `
        -BuildInputDigest $buildInputDigest
      if ($association) {
        $entry.confirmation = 'confirmed-by-association'
        $entry.association = [ordered]@{
          run_id = [string]$association.run_id
          report_id = [string]$association.report_id
          recorded_at_utc = [string]$association.recorded_at_utc
          source_snapshot_digest = [string]$association.source_snapshot_digest
          build_input_digest = [string]$association.build_input_digest
        }
      } else {
        $entry.confirmation_reason = 'association-missing-or-stale：没有绑定到本次源码快照摘要与构建输入摘要的关联记录 ⇒ 未能确认该产物来自对该快照做过的、事后通过校验的构建'
      }
    }
  } else {
    if ($artifactRecord.build) {
      $entry.confirmation = 'built-in-this-run-from-verified-unchanged-inputs'
    } else {
      if ($entry.source_in_declared_snapshot_scope) {
        $entry.confirmation = 'declared-source-file-covered-by-verified-snapshot'
      } else {
        $entry.confirmation_reason = ('该产物没有 build 声明，且其来源不落在声明源码快照范围内（classified={0}）⇒ 无法确认它对应本次快照' -f $entry.source_classification)
      }
    }
    if ($isExport -and $entry.producer_build_script_rerun -ne $true) {
      # 生产者构建脚本在本次调用里没有真正重跑：导出物可能是复用既有 cargo 构建目录的字节。
      # PR-PKG-02：**先**尝试"外部构建输入登记"这条正当路径——来源/版本/checksum 与 Cargo.lock
      # 逐字比对通过时，记 `declared-external-input-verified`（不是 not-confirmed，也不是放宽校验）。
      $externalInput = Resolve-ExternalBuildInputVerification `
        -PackageId $(if ($exportSummary) { [string]$exportSummary.producer_package_id } else { $null }) `
        -ManifestData $manifestData `
        -WorkspacePath $repo
      if ($externalInput.verified) {
        $entry.confirmation = 'declared-external-input-verified'
        $entry.external_input_verified = $true
        $entry.external_input = [ordered]@{
          id = $externalInput.id
          crate = $externalInput.crate
          version = $externalInput.version
          checksum = $externalInput.checksum
          registry_source = $externalInput.registry_source
          checksum_authority = $externalInput.checksum_authority
          verified_from = 'manifest-declared-checksum-equals-cargo-lock-checksum'
        }
        $externalInputsVerified.Add([pscustomobject]$entry.external_input) | Out-Null
      } else {
        $entry.confirmation = 'not-confirmed'
        $entry.confirmation_reason = ('producer-build-script-not-rerun-in-this-invocation：导出物可能复用既有 cargo 构建目录的字节，其输入状态未经本次校验；外部输入登记也未通过（{0}）' -f [string]$externalInput.reason)
        $provenanceLimitations.Add(('{0}: 生产者未在本次调用重跑（reuse_nature={1}）；外部输入核验：{2}' -f [string]$result.id, $(if ($exportSummary) { [string]$exportSummary.reuse_nature } else { 'unknown' }), [string]$externalInput.reason)) | Out-Null
      }
    }
  }
  if ($entry.confirmation -eq 'not-confirmed') { $provenanceUnconfirmed.Add([string]$result.id) | Out-Null }
  $artifactProvenance.Add([pscustomobject]$entry) | Out-Null
}

# ---------------------------------------------------------------------------
# 发布资格门禁（裁决第 1、6、7 条）：逐门给出 pass/fail/not-confirmed + 证据；
# 任一门不是 pass ⇒ release_eligible=false（"未能确认"不是通过）。
# ---------------------------------------------------------------------------
$releaseGates = [System.Collections.Generic.List[object]]::new()
$releaseReasons = [System.Collections.Generic.List[string]]::new()
if ($scopeDeclared) {
  $releaseGates.Add((New-PackageReleaseGate -Name 'declared_source_snapshot_scope' -Result 'pass' `
    -Evidence ('declared_by={0}; roots={1}' -f $sourceSnapshotScope.declared_by, @($sourceSnapshotScope.roots).Count))) | Out-Null
} else {
  $releaseGates.Add((New-PackageReleaseGate -Name 'declared_source_snapshot_scope' -Result 'not-confirmed' `
    -Evidence 'manifest 未声明 source_snapshot' `
    -Note '没有声明源码快照范围 ⇒ 无法回答"输入范围是否受影响"（裁决第 7 条：须明确未能确认，不乐观放行）')) | Out-Null
  $releaseReasons.Add('未声明 source_snapshot：无法确认输入范围是否受影响') | Out-Null
}
if ($snapshotDiff) {
  if ($snapshotDiff.identical) {
    $releaseGates.Add((New-PackageReleaseGate -Name 'source_input_stability' -Result 'pass' `
      -Evidence ('pre={0} post={1}；两次采样逐文件一致' -f $sourceSnapshotPreBuild.source_snapshot_digest, $sourceSnapshotPostBuild.source_snapshot_digest))) | Out-Null
  } else {
    $releaseGates.Add((New-PackageReleaseGate -Name 'source_input_stability' -Result 'fail' -Evidence ($snapshotDiff.summary_lines -join '; '))) | Out-Null
    $releaseReasons.Add('构建期间观察到源码快照变化（混合输入产物）') | Out-Null
  }
} else {
  $releaseGates.Add((New-PackageReleaseGate -Name 'source_input_stability' -Result 'not-confirmed' -Evidence 'no-declared-scope')) | Out-Null
  $releaseReasons.Add('源码输入稳定性未能确认') | Out-Null
}
$missingRoots = @()
if ($sourceSnapshotPreBuild) { $missingRoots = @($sourceSnapshotPreBuild.missing_roots) }
if ($missingRoots.Count -eq 0) {
  $releaseGates.Add((New-PackageReleaseGate -Name 'declared_roots_present' -Result 'pass' -Evidence '声明 roots 全部存在（范围可枚举）')) | Out-Null
} else {
  $releaseGates.Add((New-PackageReleaseGate -Name 'declared_roots_present' -Result 'fail' -Evidence ('缺失声明 root：{0}' -f ($missingRoots -join ', ')))) | Out-Null
  $releaseReasons.Add('声明的源码快照 root 缺失 ⇒ 快照范围不完整') | Out-Null
}
if ($buildInputComparison) {
  if ($buildInputComparison.identical) {
    $releaseGates.Add((New-PackageReleaseGate -Name 'build_input_stability' -Result 'pass' `
      -Evidence ('pre={0} post={1}' -f $buildInputComparison.baseline_digest, $buildInputComparison.current_digest))) | Out-Null
  } else {
    $releaseGates.Add((New-PackageReleaseGate -Name 'build_input_stability' -Result 'fail' -Evidence ($buildInputComparison.changed_descriptor_lines -join '; '))) | Out-Null
    $releaseReasons.Add('构建期间观察到声明的构建输入变化（混合输入产物）') | Out-Null
  }
} else {
  $releaseGates.Add((New-PackageReleaseGate -Name 'build_input_stability' -Result 'not-confirmed' -Evidence $buildInputUnconfirmedReason)) | Out-Null
  $releaseReasons.Add('构建输入稳定性未能确认') | Out-Null
}
if ($artifactProvenance.Count -eq 0) {
  $releaseGates.Add((New-PackageReleaseGate -Name 'artifact_provenance' -Result 'pass' -Evidence '没有 artifact 需要确认')) | Out-Null
} elseif ($provenanceUnconfirmed.Count -eq 0) {
  $releaseGates.Add((New-PackageReleaseGate -Name 'artifact_provenance' -Result 'pass' -Evidence ('全部 {0} 个产物的来源可确认' -f $artifactProvenance.Count))) | Out-Null
} else {
  $releaseGates.Add((New-PackageReleaseGate -Name 'artifact_provenance' -Result 'not-confirmed' -Evidence ('未能确认来源的产物：{0}' -f ($provenanceUnconfirmed -join ', ')))) | Out-Null
  $releaseReasons.Add(('产物来源未能确认：{0}' -f ($provenanceUnconfirmed -join ', '))) | Out-Null
}
# 冻结输入门（RD4-06 / 裁决 §3.1）：正式构建阶段只消费冻结输入；显式消费准备阶段冻结记录时
# 逐项核对（不一致已在上面 fail-closed），自冻结时如实标注模式。
$frozenInputsMode = [string]$script:releaseContext.frozenInputsMode
if ($script:releaseContext.freezeComparison) {
  $freezeGateResult = $(if ($script:releaseContext.freezeComparison.identical) { 'pass' } else { 'fail' })
  $releaseGates.Add((New-PackageReleaseGate -Name 'frozen_inputs' -Result $freezeGateResult `
    -Evidence ('freeze_id={0}; mode={1}; changed_paths={2}' -f [string]$script:releaseContext.freezeDocument.freeze_id, $frozenInputsMode, [int]$script:releaseContext.freezeComparison.changed_path_count) `
    -Note '正式构建阶段消费准备阶段冻结记录：任一声明输入改变即拒绝发布')) | Out-Null
  if ($freezeGateResult -ne 'pass') { $releaseReasons.Add('冻结输入在正式构建阶段发生变化') | Out-Null }
} else {
  $releaseGates.Add((New-PackageReleaseGate -Name 'frozen_inputs' -Result 'pass' `
    -Evidence ('mode={0}; freeze_id={1}' -f $frozenInputsMode, [string]$script:releaseContext.freezeDocument.freeze_id) `
    -Note '未提供 -FreezeRecordPath：本次构建在自身开始处自冻结输入（单阶段运行）；稳定性由构建前后两次采样逐项一致证明')) | Out-Null
}

$liveWorktreeChanged = 'not-confirmed'
$buildSnapshotIntegrity = 'not-confirmed:no-declared-source-snapshot-scope'
if ($snapshotDiff) {
  $liveWorktreeChanged = (-not $snapshotDiff.identical)
  $buildSnapshotIntegrity = $(if ($snapshotDiff.identical) { 'verified-unchanged-live-tree' } else { 'not-established:inputs-changed' })
}
$validationSnapshotDigest = $(if ($sourceSnapshotPostBuild) { $sourceSnapshotPostBuild.source_snapshot_digest } else { 'not-computed' })
$releaseEligibility = New-PackageReleaseEligibility `
  -Configuration $Configuration `
  -LiveWorktreeChanged $liveWorktreeChanged `
  -BuildSnapshotIntegrity $buildSnapshotIntegrity `
  -SourceSnapshotDigest $sourceSnapshotDigestValue `
  -BuildInputDigest $buildInputDigest `
  -ValidationSnapshotDigest $validationSnapshotDigest `
  -Gates $releaseGates.ToArray() `
  -ArtifactProvenance $artifactProvenance.ToArray() `
  -ExternalInputs $externalInputsVerified.ToArray() `
  -Reasons $releaseReasons.ToArray() `
  -Limitations $provenanceLimitations.ToArray()
Write-Host ("release eligibility: release_eligible={0} live_worktree_changed={1} build_snapshot_integrity={2}" -f $releaseEligibility.release_eligible, $releaseEligibility.live_worktree_changed, $releaseEligibility.build_snapshot_integrity)
Add-PackageStageRecord -Stage 'artifact-provenance' -Outcome $(if ($releaseEligibility.release_eligible) { 'completed' } else { 'not-eligible' }) -Detail ('release_eligible={0}' -f $releaseEligibility.release_eligible) | Out-Null

# ---------------------------------------------------------------------------
# 载荷身份 + 产物清单。清单写在**暂存包内**（分发的载荷自身携带可核对的载荷身份
# 与报告引用），并在 package-safety 扫描之前落地，使清单自身也进入安全扫描。
# 清单载体自身排除在 payload_digest 之外：否则形成自引用固定点。
# ---------------------------------------------------------------------------
$payloadInventoryRelative = $script:PayloadInventoryCarrierName
$payload = Get-PayloadInventory -Root $resolvedPackageRoot -RepoPath $repoPath -ExcludeRelativePaths @($payloadInventoryRelative)
Write-Host ("payload identity: files={0} bytes={1} digest={2}" -f $payload.file_count, $payload.total_bytes, $payload.digest)

# 报告落点与报告 ID 先定：产物清单要引用报告 ID 与内容哈希。
$reportStamp = Get-Date -Format 'yyyyMMdd-HHmmssfff'
$resolvedReportPath = if ($ReportPath) {
  if ([System.IO.Path]::IsPathRooted($ReportPath)) {
    [System.IO.Path]::GetFullPath($ReportPath)
  } else {
    [System.IO.Path]::GetFullPath((Join-Path $repoPath $ReportPath))
  }
} else {
  # 沿用既有落点（第五轮裁决 B-2）：tmp/package-reports/package-report-<config>-<stamp>.json
  Join-Path $repoPath "$($script:PackageReportRoot -replace '/', '\')\package-report-$Configuration-$reportStamp.json"
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

$reportId = Get-PackageReportId -Configuration $Configuration -Stamp $reportStamp

$repoHead = Get-LoaderRepoHead -RepoPath $repoPath
$report = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString('o')
  # 本次运行 ID：失败诊断与成功报告共用同一概念，便于把"同一次运行的成功/失败"对上。
  run_id = $script:packageRunId
  configuration = $Configuration
  # 运行阶段（RD4-06）：正式构建阶段（build）。准备阶段不写成功报告（见 prepare/ 分区记录）。
  run_phase = $script:packagePhase
  manifest = ConvertTo-RepoRelativePath $manifestPath
  package_root = ConvertTo-RepoRelativePath $resolvedPackageRoot
  backup_keep = $backupKeep
  # 发布资格：成功报告也必须显式回答"能不能发布"，并逐门给出证据。
  # 注意：release_eligible=false 的报告**不是**可发布收据（见 release_eligibility.refusal_note）。
  release_eligible = $releaseEligibility.release_eligible
  release_eligibility = $releaseEligibility
  # 构建身份：哪次构建 / 哪个源码快照 / 是否含未提交修改（见 source_identity_kind 说明）
  build_context = [ordered]@{
    mode = $(if ($SkipBuild) { 'no-build' } else { 'build' })
    package_manifest = ConvertTo-RepoRelativePath $manifestPath
    package_manifest_sha256 = Get-OptionalHash $manifestPath
    build_target = [string]$cargoIdentity.build_target
    host_target = [string]$cargoIdentity.host_target
    cargo_version = [string]$cargoIdentity.cargo_version
    release_version = $releaseVersion
    # B-1 口径：source_commit 不作为当前源码权威（可为空）；HEAD 只作种子参考。
    source_commit = $vcsState.source_commit
    source_commit_available = (-not [string]::IsNullOrEmpty([string]$vcsState.source_commit))
    source_commit_authority = $vcsState.source_commit_authority
    vcs_state = $vcsState.vcs_state
    vcs_reference_commit = $vcsState.vcs_reference_commit
    vcs_reference_commit_kind = $vcsState.vcs_reference_commit_kind
    dirty_against_commit = $vcsState.dirty_against_commit
    dirty_against_commit_note = $vcsState.dirty_against_commit_note
    tracked_index_file_count = $vcsState.tracked_index_file_count
    vcs_evidence = $vcsState.evidence
    tracked_build_entry_files = $repoHead.tracked_build_entry_files
    source_identity_kind = 'declared-file-hash-set'
    source_identity_note = '该声明文件哈希集合只覆盖 tauri 构建入口声明的文件与导入目录，**不是**全量源码快照；全量源码身份是 source_snapshot_digest（见 source_snapshot 块）。tauri-build 会重写 src-tauri/gen/schemas，故不使用整树 git status 作为身份'
    generated_by = 'scripts/package-all.ps1'
  }
  # 三个分开的身份：互不冒充，各自回答一个不同的问题。
  build_identity = [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    source_snapshot_digest = $sourceSnapshotDigestValue
    build_input_digest = $buildInputDigest
    payload_digest = $payload.digest
    identity_questions = [ordered]@{
      source_snapshot_digest = '本次使用了哪份第一方源码和构建资源快照？'
      build_input_digest = '该快照配合什么锁文件、工具链、target、profile、features 和构建配置？'
      payload_digest = '最终生成和分发了哪些二进制与资源？'
    }
    caveat = '历史来源可以未知；当前使用哪些字节必须可回答。三者仍不证明原始作者、历史提交链或跨机器逐字节可复现。'
    payload_digest_scope = ('暂存包根目录下除 {0} 自身之外的全部文件（path|length|sha256 规范排序后 SHA256）' -f $payloadInventoryRelative)
  }
  # 源码快照：使用哪份第一方源码/构建资源 + 边界 + 构建后复核
  source_snapshot = [ordered]@{
    computed = ($null -ne $sourceSnapshotPreBuild)
    digest = $sourceSnapshotDigestValue
    scope_digest = $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.scope_digest } else { $null })
    file_set_digest = $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.file_set_digest } else { $null })
    file_count = $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.file_count } else { 0 })
    total_bytes = $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.total_bytes } else { 0 })
    quiescent_passes = 2
    declared_by = $(if ($sourceSnapshotScope) { $sourceSnapshotScope.declared_by } else { $null })
    roots = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.roots) } else { @() })
    exclude_path_patterns = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.exclude_path_patterns) } else { @() })
    allow_path_patterns = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.allow_path_patterns) } else { @() })
    allow_reparse_points = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.allow_reparse_points) } else { @() })
    external_path_dependencies = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.external_path_dependencies) } else { @() })
    skipped_reparse_points = $(if ($sourceSnapshotPreBuild) { @($sourceSnapshotPreBuild.skipped_reparse_points) } else { @() })
    unclassified_files = $(if ($sourceSnapshotPreBuild) { @($sourceSnapshotPreBuild.unclassified_files) } else { @() })
    missing_roots = $(if ($sourceSnapshotPreBuild) { @($sourceSnapshotPreBuild.missing_roots) } else { @() })
    extension_histogram = $(if ($sourceSnapshotPreBuild) { $sourceSnapshotPreBuild.extension_histogram } else { @{} })
    freeze_record = $(if ($sourceSnapshotFreeze) { $sourceSnapshotFreeze.path } else { $null })
    freeze_record_sha256 = $(if ($sourceSnapshotFreeze) { $sourceSnapshotFreeze.content_sha256 } else { $null })
    freeze_record_reused = $(if ($sourceSnapshotFreeze) { $sourceSnapshotFreeze.reused_existing_record } else { $null })
    post_build_verification = $sourceSnapshotVerification
    validation_snapshot_digest = $validationSnapshotDigest
    validation_snapshot_digest_note = '构建后第二次独立枚举得到的输入摘要（= 校验采样），用于回答"构建后输入是否仍与构建前一致"。'
    note = '纳入判定是声明 roots 下除排除项之外的一切；排除规则同时承担"不混入用户配置/凭据/运行数据库/模型权重/无关 target"的职责。reparse point（junction/symlink）一律登记而不追随。'
  }
  # 构建输入：锁文件（含独立 Tauri 项目自己的 Cargo.lock）/ 工具链 / target / profile / features / 构建配置
  build_inputs = [ordered]@{
    digest = $buildInputDigest
    descriptor_count = @($buildInputDescriptors).Count
    descriptors = @($buildInputDescriptors)
    post_build_recheck = $(if ($buildInputComparison) {
        [ordered]@{
          comparable = $true
          identical = [bool]$buildInputComparison.identical
          baseline_digest = $buildInputComparison.baseline_digest
          current_digest = $buildInputComparison.current_digest
          changed_descriptor_lines = @($buildInputComparison.changed_descriptor_lines)
          evidence_kind = 'observed-between-two-samples'
        }
      } else {
        [ordered]@{ comparable = $false; identical = 'not-confirmed'; unconfirmed_reason = $buildInputUnconfirmedReason }
      })
    note = '独立 Tauri shell 是独立 Cargo 项目：其 build_entry_manifest / lockfile / build.rs / tauri.conf.json 由 artifact.export 自动登记，不只记顶层 workspace 的 lockfile。'
  }
  # 声明的输入范围（裁决第 7、8 条）：范围规则显式、可审查；"不属于构建输入"的声明带理由。
  declared_input_scope = [ordered]@{
    source_snapshot_declared_by = $(if ($sourceSnapshotScope) { $sourceSnapshotScope.declared_by } else { $null })
    source_snapshot_roots = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.roots) } else { @() })
    source_snapshot_exclude_path_patterns = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.exclude_path_patterns) } else { @() })
    source_snapshot_allow_path_patterns = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.allow_path_patterns) } else { @() })
    source_snapshot_external_path_dependencies = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.external_path_dependencies) } else { @() })
    build_input_files = @($declaredBuildInputFiles)
    non_build_input_declarations = @($nonBuildInputDeclarations)
    non_build_input_note = '这些路径上的变化**不得**被误判为源码变化（裁决第 8 条）。每一条都给出 reason，使"为什么不算构建输入"可审查。'
    path_classification_rule = 'Get-PackagePathClassification：先后判 non_build_input_paths、declared build inputs、然后判是否落在声明 roots 内（排除/放行规则按 root 相对路径匹配）；规则不足时分类为 undetermined。'
  }
  # 载荷：暂存包最终文件清单的摘要 + 清单载体
  payload_inventory = [ordered]@{
    path = (ConvertTo-PackageRelativePath (Join-Path $resolvedPackageRoot $payloadInventoryRelative))
    payload_digest = $payload.digest
    file_count = $payload.file_count
    total_bytes = $payload.total_bytes
  }
  # 导出物登记：谁产出、产出到哪、内容身份、复用性质 + 固定代次语义 + 精确消费关联
  # （PKG-L07c 补强授权项；字段见 scripts/package-all.ps1 的 New-LoaderExportSummary）。
  exported_artifacts = @($script:exportSummaries)
  # RD4-06 准备/冻结分离：正式构建阶段消费的冻结输入 + 实际改变的路径（前后摘要 + 发生阶段）。
  preparation_and_freeze = [ordered]@{
    phase = $script:packagePhase
    frozen_inputs_mode = [string]$script:releaseContext.frozenInputsMode
    freeze = (ConvertTo-PackageReportFreezeReference $script:releaseContext.freezeRecord)
    freeze_id = $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.freeze_id } else { $null })
    frozen_input_count = $(if ($script:releaseContext.freezeDocument) { @($script:releaseContext.freezeDocument.build_input_descriptor_lines).Count } else { 0 })
    prepared_source_snapshot_digest = $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.source_snapshot_digest } else { $null })
    prepared_build_input_digest = $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.build_input_digest } else { $null })
    generated_configs = @($script:releaseContext.generatedConfigs)
    generated_config_rule = '准备阶段生成的配置生成后即计入冻结输入（摘要进入 freeze_id）；正式构建阶段改写即拒绝发布'
    derived_outputs = @($script:releaseContext.derivedOutputs)
    dependency_resolution = $script:releaseContext.dependencyResolution
    cargo_lock_semantics = $script:CargoLockSemantics
    frozen_input_check = $script:releaseContext.freezeComparison
    input_set_mutation_policy = '冻结输入清单不得在构建阶段被改写或删减（不允许"发现输入被改写后把它从清单移除"以换取 release_eligible=true）'
  }
  # 实际改变的路径：逐条带前后摘要与**发生阶段**（裁决 §3.1 最后一句）。
  changed_paths = @($script:releaseContext.changedPaths)
  artifacts = $results
}
# 报告身份块与 Write-GovernedPackageReport 共用同一构造点（避免字段取值漂移）。
$report.report_identity = New-PackageReportIdentity `
  -ReportId $reportId `
  -Path $resolvedReportPath `
  -RepoPath $repoPath `
  -GeneratedBy 'scripts/package-all.ps1'
# 报告内容哈希必须在写产物清单之前算出：清单要引用它（清单在载荷内，报告在载荷外，
# 因此这个顺序不构成环）。哈希口径见 Get-NormalizedPackageReportHash：在"序列化再解析"
# 的归一化文档上计算，保证验证方（读文件→重算）能得到同一个值。
$reportContentHash = Get-NormalizedPackageReportHash -Report $report

$payloadInventoryDocument = [ordered]@{
  schema = [int]$script:BuildIdentitySchema
  kind = 'package-payload-inventory'
  generated_at = (Get-Date).ToUniversalTime().ToString('o')
  configuration = $Configuration
  package_root = ConvertTo-RepoRelativePath $resolvedPackageRoot
  payload_digest = $payload.digest
  digest_scope = ('本目录下除 {0} 自身之外的全部文件；path|length|sha256 规范排序后 SHA256' -f $payloadInventoryRelative)
  file_count = $payload.file_count
  total_bytes = $payload.total_bytes
  self_carrier = [ordered]@{
    path = $payloadInventoryRelative
    excluded_from_payload_digest = $true
    reason = '清单载体的内容哈希由报告与 installer report 登记；若把它算进 payload_digest 会形成自引用固定点'
  }
  build_identity = [ordered]@{
    source_snapshot_digest = $sourceSnapshotDigestValue
    build_input_digest = $buildInputDigest
    payload_digest = $payload.digest
  }
  vcs = [ordered]@{
    vcs_state = $vcsState.vcs_state
    vcs_reference_commit = $vcsState.vcs_reference_commit
    vcs_reference_commit_kind = $vcsState.vcs_reference_commit_kind
    source_commit = $vcsState.source_commit
    source_commit_authority = $vcsState.source_commit_authority
    dirty_against_commit = $vcsState.dirty_against_commit
  }
  # 产物清单引用报告：唯一 ID + 内容哈希（报告 → 清单的反向引用是 payload_digest）。
  report_ref = [ordered]@{
    report_id = $reportId
    report_path = ConvertTo-RepoRelativePath $resolvedReportPath
    content_sha256 = $reportContentHash
    content_hash_algorithm = 'sha256'
    content_hash_scope = '报告的全部叶子节点规范排序后取 SHA256；排除 report_identity.content_sha256'
  }
  retention = [ordered]@{
    rule = '被正式发布引用的报告由 scripts/package-report-retention.ps1 保护（索引 tmp/package-reports/retention-index.json）或连同发布证据归档到 docs/testing/release-<version>/evidence/build-identity/'
    prune = '普通临时清理不得删除被索引保护的报告；Prune 只允许删除未被索引的报告'
  }
  files = @($payload.entries | ForEach-Object {
      [ordered]@{ path = $_.path; length = $_.length; sha256 = $_.sha256 }
    })
}
$inventoryPath = Join-Path $resolvedPackageRoot $payloadInventoryRelative
[System.IO.File]::WriteAllText($inventoryPath, ($payloadInventoryDocument | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
$inventoryFileSha256 = Get-OptionalHash $inventoryPath
Write-Host ("payload inventory: {0} (sha256={1})" -f (ConvertTo-PackageRelativePath $inventoryPath), $inventoryFileSha256)

& (Join-Path $repo 'scripts/package-safety.ps1') -Root $resolvedPackageRoot | Out-Null
Add-PackageStageRecord -Stage 'payload-inventory-and-safety-scan' -Outcome 'completed' -Detail ('payload files={0} digest={1}; package-safety 扫描通过' -f $payload.file_count, $payload.digest) | Out-Null

Set-PackageStage -Stage 'report-write' -Detail $reportId
$governed = Write-GovernedPackageReport -Report $report -Path $resolvedReportPath -RepoPath $repoPath -ReportId $reportId
if ($governed.content_sha256 -ne $reportContentHash) {
  throw ((
    '[REPORT-CONTENT-DRIFT] profile={0}' + "`n" +
    'detail: 报告内容哈希在"预算"与"落盘"之间发生变化（precomputed={1} written={2}）' + "`n" +
    'next: 说明报告对象在写盘前被追加了字段；修正 scripts/package-all.ps1，不要沿用不一致的哈希'
  ) -f $Configuration, $reportContentHash, [string]$governed.content_sha256)
}
Write-Host ("package report: {0} (report_id={1} content_sha256={2})" -f $governed.report_path, $governed.report_id, $governed.content_sha256)

# ---------------------------------------------------------------------------
# 产物—快照关联（裁决第 9 条）：只有"已确认来源"的产物才登记关联。
# 关联里带上**本次**源码快照摘要与构建输入摘要，--no-build 只有找到绑定到同一组摘要
# 的记录才可确认"产物来自对该快照做过的、事后通过校验的构建"。
# ---------------------------------------------------------------------------
$associationEntries = [System.Collections.Generic.List[object]]::new()
foreach ($entry in @($artifactProvenance)) {
  if ([string]$entry.confirmation -eq 'not-confirmed') { continue }
  $sourceLength = 0
  $sourceFull = Join-Path $repoPath (([string]$entry.source_path).Replace('/', '\'))
  if (Test-Path -LiteralPath $sourceFull -PathType Leaf) { $sourceLength = [long](Get-Item -LiteralPath $sourceFull -Force).Length }
  $associationEntries.Add([pscustomobject]@{
    artifact_id = [string]$entry.artifact_id
    target = [string]$entry.target
    source_path = [string]$entry.source_path
    source_sha256 = [string]$entry.source_sha256
    source_length = $sourceLength
    has_export = [bool]$entry.has_export
    stable_export_sha256 = [string]$entry.stable_export_sha256
    receipt_path = [string]$entry.receipt
    producer_build_script_rerun = $entry.producer_build_script_rerun
    build_mode = [string]$entry.build_mode
  }) | Out-Null
}
$associationPath = Add-PackageArtifactAssociations `
  -RepoPath $repoPath `
  -Configuration $Configuration `
  -RunId $script:packageRunId `
  -ReportId $governed.report_id `
  -SourceSnapshotDigest $sourceSnapshotDigestValue `
  -BuildInputDigest $buildInputDigest `
  -Entries $associationEntries.ToArray()
if ($associationPath) {
  Write-Host ("artifact-snapshot associations recorded: {0} entries" -f $associationEntries.Count)
}
# 被本次"构建后校验通过"的运行取代的禁用记录：只清理**有证据**的条目
#   * 生产者本次真正重跑；或
#   * 产物文件在本次运行期间被重写（mtime 证据）；或
#   * 产物来源本身就落在本次已验证不变的源码快照范围内（其字节已被快照覆盖）。
# 其余禁用记录保留（未确认不放行）。
$clearedRevocations = @()
if (-not $SkipBuild) {
  $clearableEntries = [System.Collections.Generic.List[object]]::new()
  foreach ($entry in @($artifactProvenance)) {
    if ([string]$entry.confirmation -eq 'not-confirmed') { continue }
    if (
      $entry.producer_build_script_rerun -eq $true -or
      $entry.rebuilt_in_this_run -eq $true -or
      $entry.source_in_declared_snapshot_scope -eq $true
    ) {
      $clearableEntries.Add($entry) | Out-Null
    }
  }
  $clearedRevocations = @(Clear-PackageArtifactRevocations -RepoPath $repoPath -Configuration $Configuration -Entries $clearableEntries.ToArray())
  if ($clearedRevocations.Count -gt 0) {
    Write-Host ("cleared {0} artifact revocation record(s) superseded by this verified build" -f $clearedRevocations.Count)
  }
}

# ---------------------------------------------------------------------------
# latest 指针：build-msi.ps1 与"被正式发布引用的报告"需要可寻址的引用来源。
# 指针同时作为本脚本的最后一个输出对象（调用方可直接捕获）。
# 失败运行**不写**这个文件（见 catch 分支）：因此"最新有效包"不会被更新。
# ---------------------------------------------------------------------------
$pointer = [ordered]@{
  schema = [int]$script:BuildIdentitySchema
  kind = 'package-report-latest-pointer'
  configuration = $Configuration
  generated_at = (Get-Date).ToUniversalTime().ToString('o')
  run_id = $script:packageRunId
  report_id = $governed.report_id
  report_path = $governed.report_path
  report_content_sha256 = $governed.content_sha256
  report_file_sha256 = $governed.file_sha256
  release_eligible = $releaseEligibility.release_eligible
  release_eligibility = [ordered]@{
    release_eligible = $releaseEligibility.release_eligible
    live_worktree_changed = $releaseEligibility.live_worktree_changed
    build_snapshot_integrity = $releaseEligibility.build_snapshot_integrity
    build_input_digest = $buildInputDigest
    validation_snapshot_digest = $validationSnapshotDigest
    reasons = @($releaseEligibility.reasons)
    limitations = @($releaseEligibility.limitations)
  }
  artifact_release_status = [ordered]@{
    store = (ConvertTo-RepoRelativePath (Get-PackageArtifactReleaseStatusPath -RepoPath $repoPath))
    associations_recorded = $associationEntries.Count
    revocations_cleared = @($clearedRevocations).Count
  }
  payload_inventory = [ordered]@{
    path = (ConvertTo-RepoRelativePath $inventoryPath)
    file_sha256 = $inventoryFileSha256
    payload_digest = $payload.digest
    file_count = $payload.file_count
    total_bytes = $payload.total_bytes
  }
  identities = [ordered]@{
    source_snapshot_digest = $sourceSnapshotDigestValue
    build_input_digest = $buildInputDigest
    payload_digest = $payload.digest
    vcs_state = $vcsState.vcs_state
    vcs_reference_commit = $vcsState.vcs_reference_commit
    dirty_against_commit = $vcsState.dirty_against_commit
  }
  source_snapshot_freeze_record = $(if ($sourceSnapshotFreeze) { $sourceSnapshotFreeze.path } else { $null })
  source_snapshot_freeze_record_sha256 = $(if ($sourceSnapshotFreeze) { $sourceSnapshotFreeze.content_sha256 } else { $null })
}
$pointerPath = Join-Path $reportParent "latest-$Configuration.json"
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $pointerPath) | Out-Null
[System.IO.File]::WriteAllText($pointerPath, ($pointer | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
Write-Host ("package report pointer: {0}" -f (ConvertTo-RepoRelativePath $pointerPath))
Add-PackageStageRecord -Stage 'run-status' -Outcome 'completed' -Detail ('latest 指针已更新（仅成功运行更新）：{0}' -f (ConvertTo-RepoRelativePath $pointerPath)) | Out-Null
# 运行状态台账：只回答"最近一次运行成不成、有没有可发布收据"，**不替代** latest 指针。
Write-PackageRunStatus `
  -RepoPath $repoPath `
  -Configuration $Configuration `
  -Outcome 'succeeded' `
  -ReleaseEligible ([bool]$releaseEligibility.release_eligible) `
  -RunId $script:packageRunId `
  -ReportId $governed.report_id `
  -ReportPath $governed.report_path `
  -ReportContentSha256 $governed.content_sha256 `
  -SourceSnapshotDigest $sourceSnapshotDigestValue `
  -BuildInputDigest $buildInputDigest `
  -ValidationSnapshotDigest $validationSnapshotDigest `
  -Stage 'completed' `
  -Note '成功运行：更新了 latest 指针；release_eligible=false 时该报告不是可发布收据（见 release_eligibility.refusal_note）。' | Out-Null
return [pscustomobject]$pointer

} catch {
  # ==========================================================================
  # 失败路径（裁决第 2、3、4、5 条）：
  #   * 拒绝发布：不写成功报告、不更新 latest 指针、不进入签名/安装/分发；
  #   * **必须**输出失败诊断（上一版"连报告一起拒绝"是缺陷）；
  #   * 已产生的临时产物移入失败隔离区（与可发布产物分区、永不自动晋升）。
  # 诊断环节自身失败不得掩盖原始错误：内部全部 try/catch，失败只追加说明。
  # ==========================================================================
  $packageFailure = $_
  $failureStage = $script:packageStage
  $failureStageDetail = $script:packageStageDetail
  $failureMessage = [string]$packageFailure.Exception.Message
  $failureCategory = Get-PackageFailureCategory -Message $failureMessage
  $failureDiagnosticPath = $null
  $failureDiagnosticNote = $null
  try {
    # 失败阶段登记：若该阶段已在失败点自己登记过（stage 相同且 outcome=failed），不重复登记。
    $alreadyRecorded = $false
    if ($script:packageStageRecords.Count -gt 0) {
      $last = $script:packageStageRecords[$script:packageStageRecords.Count - 1]
      if ([string]$last.stage -eq $failureStage -and [string]$last.outcome -eq 'failed') { $alreadyRecorded = $true }
    }
    if (-not $alreadyRecorded) {
      Add-PackageStageRecord -Stage $failureStage -ArtifactId $failureStageDetail -Outcome 'failed' -Detail $failureCategory | Out-Null
    }

    # 上一份成功报告 / latest 指针：失败前后各读一次（证明"没有覆盖、没有更新"）。
    $pointerBeforeFailure = Read-PackageLatestPointerState -RepoPath $repoPath -Configuration $Configuration

    # 隔离：只在"本次运行初始化过 staging"时移动，避免搬走上一轮运行的产物。
    $quarantine = $null
    if ($script:stagingInitialized -and $resolvedPackagePath) {
      $quarantine = Move-PackageStagingToQuarantine -RepoPath $repoPath -PackageRootPath $resolvedPackagePath -RunId $script:packageRunId
    } else {
      $quarantineRootPath = Get-PackageFailureQuarantineRoot -RepoPath $repoPath
      $quarantine = [pscustomobject]@{
        attempted = $false
        moved = $false
        quarantine_root = (Get-IdentityRepoRelativePath -RepoPath $repoPath -Path $quarantineRootPath)
        destination = $null
        destination_relative = $null
        moved_file_count = 0
        files = @()
        skipped_reason = 'staging 尚未初始化：本次运行没有产出暂存产物（包根里若有内容属于上一轮运行，不搬动）'
        auto_promotion = 'never'
      }
    }

    # 观察到的输入变化（**已观察变化**，不是完整写入历史）。
    $observedSourceChanges = $false
    if ($script:releaseContext.snapshotComparison -and -not $script:releaseContext.snapshotComparison.identical) { $observedSourceChanges = $true }
    $observedBuildInputChanges = $false
    if ($script:releaseContext.buildInputComparison -and -not $script:releaseContext.buildInputComparison.identical) { $observedBuildInputChanges = $true }

    # 逐路径分类：使"是否属于构建输入"可审查（裁决第 7、8 条）。
    $observedPathList = [System.Collections.Generic.List[string]]::new()
    if ($script:releaseContext.snapshotComparison) {
      foreach ($path in @($script:releaseContext.snapshotComparison.added)) { $observedPathList.Add([string]$path) }
      foreach ($path in @($script:releaseContext.snapshotComparison.removed)) { $observedPathList.Add([string]$path) }
      foreach ($path in @($script:releaseContext.snapshotComparison.modified)) { $observedPathList.Add([string]$path) }
    }
    $observedClassification = [System.Collections.Generic.List[object]]::new()
    $classifiedPaths = @{}
    $pathsToClassify = [System.Collections.Generic.List[string]]::new()
    foreach ($path in @($observedPathList.ToArray())) { $pathsToClassify.Add([string]$path) | Out-Null }
    # 构建输入变化也要逐路径分类（描述符行形如 input_file=<相对路径>|length=..|sha256=..）：
    # 这样"变化的是不是构建输入"同样可审查（裁决第 7、8 条）。
    if ($script:releaseContext.buildInputComparison) {
      foreach ($line in @($script:releaseContext.buildInputComparison.changed_descriptor_lines)) {
        $match = [regex]::Match([string]$line, '^input_file=([^|]+)')
        if ($match.Success) { $pathsToClassify.Add($match.Groups[1].Value) | Out-Null }
      }
    }
    foreach ($path in @($pathsToClassify.ToArray())) {
      $key = ConvertTo-PackageIdentityPathKey $path
      if ($classifiedPaths.ContainsKey($key)) { continue }
      $classifiedPaths[$key] = $true
      $classified = Get-PackagePathClassification -Path $path -Scope $sourceSnapshotScope -BuildInputFiles $declaredBuildInputFiles -NonBuildInputPatterns $nonBuildInputPatterns
      $observedClassification.Add([pscustomobject]@{
        path = $classified.path
        classification = $classified.classification
        matched_rule = $classified.matched_rule
        declared_root = $classified.declared_root
        matched_non_build_input_pattern = $classified.matched_non_build_input_pattern
      }) | Out-Null
    }

    # 禁用记录：只有"观察到输入变化"的运行才产出混合输入产物 ⇒ 只有这种运行登记禁用。
    $revocationEntries = [System.Collections.Generic.List[object]]::new()
    if ($manifestData -and ($observedSourceChanges -or $observedBuildInputChanges)) {
      foreach ($artifact in @($manifestData.artifacts)) {
        $sourcePath = Resolve-RepoPath ([string]$artifact.source)
        $sourceSha = $null
        if (Test-Path -LiteralPath $sourcePath -PathType Leaf) { $sourceSha = Get-OptionalHash $sourcePath }
        $stableSha = $null
        $receiptPath = $null
        if ($artifact.export) {
          $stablePath = Resolve-RepoPath ([string]$artifact.source)
          if (Test-Path -LiteralPath $stablePath -PathType Leaf) { $stableSha = Get-OptionalHash $stablePath }
          $receiptPath = ConvertTo-RepoRelativePath (Resolve-RepoPath ([string]$artifact.export.receipt))
        }
        if (-not $sourceSha -and -not $stableSha) { continue }
        $revocationEntries.Add([pscustomobject]@{
          artifact_id = [string]$artifact.id
          source_path = (ConvertTo-RepoRelativePath $sourcePath)
          source_sha256 = $sourceSha
          stable_export_sha256 = $stableSha
          receipt_path = $receiptPath
        }) | Out-Null
      }
      if ($revocationEntries.Count -gt 0) {
        [void](Add-PackageArtifactRevocations `
          -RepoPath $repoPath `
          -Configuration $Configuration `
          -RunId $script:packageRunId `
          -Reason ('本次运行观察到输入变化（{0}）⇒ 相关产物属于混合输入产物，不得再被 --no-build 当作来源' -f $failureCategory) `
          -Category $failureCategory `
          -Stage $failureStage `
          -Entries $revocationEntries.ToArray())
      }
    }

    # 失败运行的发布资格：release_eligible 恒为 false，并逐门给出原因/证据。
    $failureGates = [System.Collections.Generic.List[object]]::new()
    $failureReasons = [System.Collections.Generic.List[string]]::new()
    $failureReasons.Add(('运行失败：stage={0} category={1}' -f $failureStage, $failureCategory)) | Out-Null
    $failureGates.Add((New-PackageReleaseGate -Name 'run_completed' -Result 'fail' -Evidence ('stage={0}; category={1}' -f $failureStage, $failureCategory))) | Out-Null
    if ($sourceSnapshotScope) {
      $failureGates.Add((New-PackageReleaseGate -Name 'declared_source_snapshot_scope' -Result 'pass' -Evidence $sourceSnapshotScope.declared_by)) | Out-Null
    } else {
      $failureGates.Add((New-PackageReleaseGate -Name 'declared_source_snapshot_scope' -Result 'not-confirmed' -Evidence 'manifest 未声明 source_snapshot')) | Out-Null
    }
    if ($script:releaseContext.snapshotComparison) {
      $failureGates.Add((New-PackageReleaseGate -Name 'source_input_stability' -Result $(if ($script:releaseContext.snapshotComparison.identical) { 'pass' } else { 'fail' }) -Evidence ($script:releaseContext.snapshotComparison.summary_lines -join '; '))) | Out-Null
    } else {
      $failureGates.Add((New-PackageReleaseGate -Name 'source_input_stability' -Result 'not-confirmed' -Evidence '失败发生在构建后采样之前/未声明范围')) | Out-Null
    }
    if ($script:releaseContext.buildInputComparison) {
      $failureGates.Add((New-PackageReleaseGate -Name 'build_input_stability' -Result $(if ($script:releaseContext.buildInputComparison.identical) { 'pass' } else { 'fail' }) -Evidence ($script:releaseContext.buildInputComparison.changed_descriptor_lines -join '; '))) | Out-Null
    } else {
      $failureGates.Add((New-PackageReleaseGate -Name 'build_input_stability' -Result 'not-confirmed' -Evidence ([string]$script:releaseContext.buildInputUnconfirmedReason))) | Out-Null
    }
    # 冻结输入门（RD4-06）：显式消费准备阶段冻结记录时按逐项核对结果记；否则如实记"自冻结"。
    if ($script:releaseContext.freezeComparison) {
      $failureGates.Add((New-PackageReleaseGate -Name 'frozen_inputs' `
        -Result $(if ($script:releaseContext.freezeComparison.identical) { 'pass' } else { 'fail' }) `
        -Evidence ('freeze_id={0}; mode={1}; changed_paths={2}' -f [string]$script:releaseContext.freezeDocument.freeze_id, [string]$script:releaseContext.frozenInputsMode, [int]$script:releaseContext.freezeComparison.changed_path_count))) | Out-Null
    } else {
      $failureGates.Add((New-PackageReleaseGate -Name 'frozen_inputs' `
        -Result 'not-confirmed' `
        -Evidence ('mode={0}; freeze_id={1}；失败发生在冻结消费之前或未提供 -FreezeRecordPath' -f [string]$script:releaseContext.frozenInputsMode, $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.freeze_id } else { 'not-computed' })))) | Out-Null
    }
    $failureProvenance = [System.Collections.Generic.List[object]]::new()
    foreach ($result in @($results | Where-Object { $_ })) {
      $failureProvenance.Add([pscustomobject]@{
        artifact_id = [string]$result.id
        target = [string]$result.target
        source_path = [string]$result.source
        source_sha256 = [string]$result.sha256
        copied = [bool]$result.copied
        release_eligible = $false
        confirmation = 'not-confirmed'
        confirmation_reason = '运行失败：本次运行没有产出可发布的产物'
      }) | Out-Null
    }
    $failureLiveWorktreeChanged = 'not-confirmed'
    $failureBuildSnapshotIntegrity = 'not-established:failed-run'
    if ($script:releaseContext.snapshotComparison) {
      $failureLiveWorktreeChanged = (-not $script:releaseContext.snapshotComparison.identical)
      $failureBuildSnapshotIntegrity = $(if ($script:releaseContext.snapshotComparison.identical) { 'verified-unchanged-live-tree-but-run-failed' } else { 'not-established:inputs-changed' })
    }
    $failureValidationDigest = $(if ($script:releaseContext.sourceSnapshotPost) { $script:releaseContext.sourceSnapshotPost.source_snapshot_digest } else { 'not-computed' })
    $failureEligibility = New-PackageReleaseEligibility `
      -Configuration $Configuration `
      -LiveWorktreeChanged $failureLiveWorktreeChanged `
      -BuildSnapshotIntegrity $failureBuildSnapshotIntegrity `
      -SourceSnapshotDigest $sourceSnapshotDigestValue `
      -BuildInputDigest $(if ($buildInputDigest) { $buildInputDigest } else { $null }) `
      -ValidationSnapshotDigest $failureValidationDigest `
      -Gates $failureGates.ToArray() `
      -ArtifactProvenance $failureProvenance.ToArray() `
      -Reasons $failureReasons.ToArray() `
      -Limitations @('失败运行不产出可发布收据：本次运行的全部产物只在失败隔离区，永不自动晋升')

    $failureBuildContext = [ordered]@{
      mode = $(if ($SkipBuild) { 'no-build' } else { 'build' })
      package_manifest = (Get-IdentityRepoRelativePath -RepoPath $repoPath -Path $manifestPath)
      package_manifest_sha256 = Get-IdentityFileHash -Path $manifestPath
      build_target = $(if ($cargoIdentity) { [string]$cargoIdentity.build_target } else { $null })
      host_target = $(if ($cargoIdentity) { [string]$cargoIdentity.host_target } else { $null })
      cargo_version = $(if ($cargoIdentity) { [string]$cargoIdentity.cargo_version } else { $null })
      release_version = $releaseVersion
      vcs_state = $(if ($vcsState) { [string]$vcsState.vcs_state } else { 'not-collected' })
      vcs_reference_commit = $(if ($vcsState) { [string]$vcsState.vcs_reference_commit } else { $null })
      source_commit = $(if ($vcsState) { [string]$vcsState.source_commit } else { $null })
      source_commit_authority = $(if ($vcsState) { [string]$vcsState.source_commit_authority } else { 'not-authoritative' })
      dirty_against_commit = $(if ($vcsState) { [string]$vcsState.dirty_against_commit } else { 'not_evaluable' })
      generated_by = 'scripts/package-all.ps1'
    }
    $declaredScopeForDiagnostic = [ordered]@{
      source_snapshot_declared_by = $(if ($sourceSnapshotScope) { $sourceSnapshotScope.declared_by } else { $null })
      source_snapshot_roots = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.roots) } else { @() })
      source_snapshot_exclude_path_patterns = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.exclude_path_patterns) } else { @() })
      source_snapshot_allow_path_patterns = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.allow_path_patterns) } else { @() })
      source_snapshot_external_path_dependencies = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.external_path_dependencies) } else { @() })
      declared_build_input_files = @($declaredBuildInputFiles)
      non_build_input_declarations = @($nonBuildInputDeclarations)
      declared_by_note = '范围声明来自 manifest 的 source_snapshot / build_inputs 与 release_policy；排除与放行规则按 root 相对路径匹配。'
    }
    $scopeConfirmation = [ordered]@{
      confirmed = ($null -ne $sourceSnapshotScope)
      reason = $(if ($sourceSnapshotScope) { 'manifest 声明了 source_snapshot：输入范围可枚举、可比较' } else { 'manifest 未声明 source_snapshot：无法确定输入范围是否受影响（未能确认，不乐观放行）' })
      snapshot_covered_file_count = $(if ($script:releaseContext.sourceSnapshotPre) { [int]$script:releaseContext.sourceSnapshotPre.file_count } else { 0 })
      declared_root_count = $(if ($sourceSnapshotScope) { @($sourceSnapshotScope.roots).Count } else { 0 })
      missing_roots = $(if ($script:releaseContext.sourceSnapshotPre) { @($script:releaseContext.sourceSnapshotPre.missing_roots) } else { @() })
      build_input_recheck = $(if ($script:releaseContext.buildInputComparison) { 'compared' } else { 'not-comparable' })
      unconfirmed_items = @(
        $(if (-not $sourceSnapshotScope) { 'declared_source_snapshot_scope' } else { $null })
        $(if (-not $script:releaseContext.snapshotComparison) { 'source_input_stability' } else { $null })
        $(if (-not $script:releaseContext.buildInputComparison) { 'build_input_stability' } else { $null })
      ) | Where-Object { $_ }
    }

    # ---- 失败诊断落点：与成功报告分开（failures/ 分区），绝不写进包根 ----
    $failureStamp = Get-Date -Format 'yyyyMMdd-HHmmssfff'
    $failureReportParent = $null
    if ($FailureReportPath) {
      $failureReportParent = Split-Path -Parent ([System.IO.Path]::GetFullPath($(if ([System.IO.Path]::IsPathRooted($FailureReportPath)) { $FailureReportPath } else { Join-Path $repoPath $FailureReportPath })))
    } elseif ($resolvedReportPath) {
      $failureReportParent = Join-Path (Split-Path -Parent $resolvedReportPath) $script:PackageFailureReportSubdir
    } elseif ($ReportPath) {
      $resolvedFailureSource = if ([System.IO.Path]::IsPathRooted($ReportPath)) { [System.IO.Path]::GetFullPath($ReportPath) } else { [System.IO.Path]::GetFullPath((Join-Path $repoPath $ReportPath)) }
      $failureReportParent = Join-Path (Split-Path -Parent $resolvedFailureSource) $script:PackageFailureReportSubdir
    } else {
      $failureReportParent = Get-PackageFailureReportRoot -RepoPath $repoPath
    }
    $failureReportPath = Join-Path $failureReportParent ("package-report-failure-$Configuration-$failureStamp.json")
    $failurePathRejected = $null
    if (-not $failureReportPath.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      $failurePathRejected = '失败诊断必须留在工作区内'
    } elseif ($resolvedPackageRoot) {
      $failurePackagePrefix = $resolvedPackageRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
      if ($failureReportPath.StartsWith($failurePackagePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        $failurePathRejected = '失败诊断不得写进 PackageRoot（临时产物与可发布产物必须分区）'
      }
    }
    if ($failurePathRejected) {
      $failureReportPath = Join-Path (Get-PackageFailureReportRoot -RepoPath $repoPath) ("package-report-failure-$Configuration-$failureStamp.json")
      Write-Warning ("失败诊断落点被拒（{0}），改用默认失败分区：{1}" -f $failurePathRejected, $failureReportPath)
    }

    $failureReportId = Get-PackageReportId -Configuration $Configuration -Prefix $script:PackageFailureReportIdPrefix -Stamp $failureStamp
    if ($script:releaseContext.snapshotComparison) {
      $script:releaseContext.snapshotComparison | Add-Member -NotePropertyName classification -NotePropertyValue @($observedClassification.ToArray()) -Force
    }
    # RD4-06：失败诊断要显著列出**实际改变的路径（前后摘要 + 发生阶段）**。
    $failureChangedPaths = @(Get-PackageFailureChangedPathEntries `
      -SnapshotComparison $script:releaseContext.snapshotComparison `
      -SourceSnapshotPre $script:releaseContext.sourceSnapshotPre `
      -SourceSnapshotPost $script:releaseContext.sourceSnapshotPost `
      -BuildInputComparison $script:releaseContext.buildInputComparison `
      -FrozenComparison $script:releaseContext.freezeComparison `
      -PrepareChangedPaths @($script:releaseContext.changedPaths) `
      -FreezeChangedPaths $(if ($script:releaseContext.freezeComparison) { @($script:releaseContext.freezeComparison.changed_paths) } else { @() }))
    $failurePreparationAndFreeze = [ordered]@{
      phase = $script:packagePhase
      frozen_inputs_mode = [string]$script:releaseContext.frozenInputsMode
      freeze = (ConvertTo-PackageReportFreezeReference $script:releaseContext.freezeRecord)
      freeze_id = $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.freeze_id } else { $null })
      prepared_source_snapshot_digest = $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.source_snapshot_digest } else { $null })
      prepared_build_input_digest = $(if ($script:releaseContext.freezeDocument) { [string]$script:releaseContext.freezeDocument.build_input_digest } else { $null })
      generated_configs = @($script:releaseContext.generatedConfigs)
      derived_outputs = @($script:releaseContext.derivedOutputs)
      dependency_resolution = $script:releaseContext.dependencyResolution
      cargo_lock_semantics = $script:CargoLockSemantics
      frozen_input_check = $script:releaseContext.freezeComparison
      input_set_mutation_policy = '冻结输入清单不得在构建阶段被改写或删减（不允许"发现输入被改写后把它从清单移除"以换取 release_eligible=true）'
    }
    $diagnostic = New-PackageFailureDiagnostic `
      -RunId $script:packageRunId `
      -Configuration $Configuration `
      -RepoPath $repoPath `
      -ManifestPath $manifestPath `
      -PackageRootPath $resolvedPackagePath `
      -Failure ([ordered]@{
        stage = $failureStage
        category = $failureCategory
        message = $failureMessage
        detail_lines = @($failureMessage -split "`r?`n")
        exception_type = $(if ($packageFailure.Exception) { $packageFailure.Exception.GetType().FullName } else { 'unknown' })
        remediation_hint = '修复失败原因后重新执行一次正常构建打包；不要把失败诊断或本次隔离区里的产物当作发布来源'
      }) `
      -BuildContext $failureBuildContext `
      -DeclaredInputScope $declaredScopeForDiagnostic `
      -SourceSnapshotPre $script:releaseContext.sourceSnapshotPre `
      -SourceSnapshotPost $script:releaseContext.sourceSnapshotPost `
      -SnapshotComparison $script:releaseContext.snapshotComparison `
      -BuildInputBaseline ([pscustomobject]@{ digest = $buildInputDigest }) `
      -BuildInputCurrent ([pscustomobject]@{ digest = $(if ($script:releaseContext.buildInputComparison) { $script:releaseContext.buildInputComparison.current_digest } else { $null }); unconfirmed_reason = [string]$script:releaseContext.buildInputUnconfirmedReason }) `
      -BuildInputComparison $script:releaseContext.buildInputComparison `
      -InputScopeConfirmation $scopeConfirmation `
      -StageRecords $script:packageStageRecords.ToArray() `
      -Quarantine $quarantine `
      -PreviousPointerState $pointerBeforeFailure `
      -LatestPointerVerification ([ordered]@{
        checked_by = 'scripts/package-all.ps1 (failure path)'
        before_file_sha256 = [string]$pointerBeforeFailure.file_sha256
        pointer_written_by_failure_path = $false
        note = '失败路径没有任何写 latest 指针的代码；写入失败诊断之后会再读一次并核对（结果记录在 latest-run-<config>.json）'
      }) `
      -ArtifactRevocations $revocationEntries.ToArray() `
      -ReleaseEligibility $failureEligibility `
      -ArtifactResults @($results | Where-Object { $_ }) `
      -ExportSummaries @($script:exportSummaries) `
      -RunPhase $script:packagePhase `
      -ChangedPaths $failureChangedPaths `
      -PreparationAndFreeze $failurePreparationAndFreeze
    $governedDiagnostic = Write-GovernedPackageReport -Report $diagnostic -Path $failureReportPath -RepoPath $repoPath -ReportId $failureReportId
    $failureDiagnosticPath = $governedDiagnostic.report_path

    # 写完之后再读一次 latest 指针：核对"失败运行没有更新最新有效包"。
    $pointerAfterFailure = Read-PackageLatestPointerState -RepoPath $repoPath -Configuration $Configuration
    $pointerUntouched = (
      [string]$pointerBeforeFailure.file_sha256 -eq [string]$pointerAfterFailure.file_sha256 -and
      [string]$pointerBeforeFailure.report_id -eq [string]$pointerAfterFailure.report_id)
    if (-not $pointerUntouched) {
      Write-Warning "[LATEST-POINTER-MUTATED] 失败路径观察到 latest 指针发生变化（before=$($pointerBeforeFailure.file_sha256) after=$($pointerAfterFailure.file_sha256)）"
    }

    Write-PackageRunStatus `
      -RepoPath $repoPath `
      -Configuration $Configuration `
      -Outcome 'failed' `
      -ReleaseEligible $false `
      -RunId $script:packageRunId `
      -FailureReportId $governedDiagnostic.report_id `
      -FailureReportPath $governedDiagnostic.report_path `
      -FailureReportContentSha256 $governedDiagnostic.content_sha256 `
      -SourceSnapshotDigest $sourceSnapshotDigestValue `
      -BuildInputDigest $(if ($buildInputDigest) { $buildInputDigest } else { $null }) `
      -ValidationSnapshotDigest $failureValidationDigest `
      -Stage $failureStage `
      -Category $failureCategory `
      -Note ('失败运行：未写成功报告、未更新 latest 指针（指针仍指向 {0}）、未进入签名/安装/分发；产物已在失败隔离区且不自动晋升' -f $(if ($pointerAfterFailure.report_id) { $pointerAfterFailure.report_id } else { '(无上一份成功报告)' })) | Out-Null

    Write-Host ("failure diagnostic: {0} (report_id={1} release_eligible=false)" -f $governedDiagnostic.report_path, $governedDiagnostic.report_id)
    Write-Host ("failure quarantine: {0} (moved={1} files={2})" -f [string]$quarantine.quarantine_root, [bool]$quarantine.moved, [int]$quarantine.moved_file_count)
  } catch {
    # 诊断环节失败不得掩盖原始错误。
    $failureDiagnosticNote = ('失败诊断/隔离环节自身失败：{0}' -f (($_.Exception.Message -split "`r?`n")[0]))
    Write-Warning $failureDiagnosticNote
  }

  $augmentedFailureMessage = $failureMessage
  if ($failureDiagnosticPath) {
    $augmentedFailureMessage = $failureMessage + "`n[FAILURE-DIAGNOSTIC] path=" + $failureDiagnosticPath + " release_eligible=false next: 该文件是失败诊断，不是发布收据；修复失败原因后重新执行正常构建打包"
  } elseif ($failureDiagnosticNote) {
    $augmentedFailureMessage = $failureMessage + "`n[FAILURE-DIAGNOSTIC] 未能写出失败诊断：" + $failureDiagnosticNote
  }
  throw $augmentedFailureMessage
}

