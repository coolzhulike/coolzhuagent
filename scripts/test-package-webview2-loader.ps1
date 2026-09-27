param(
  # L04 需要一次"全新 target + 已准备离线依赖"的真实构建（数百 crate，预计数十分钟），
  # 因此默认跳过；有构建预算时加该开关执行。
  [switch]$RunCleanTargetBuild,
  # 只跑 PKG-L07c 并发/发布权契约（不依赖 scripts/package-all.ps1），用于固定轮数重复验证。
  [switch]$OnlyConcurrency,
  # L07c 并发用例的固定重复轮数：每轮都用两个真实进程 + 命名事件屏障触发。
  [int]$ConcurrencyRounds = 1
)

$ErrorActionPreference = 'Stop'

# ============================================================================
# WebView2Loader 确定性产物链契约测试（PKG-01 / PKG-02，裁决第八节 L01–L11）
#
# 设计要点：
#   * 夹具全部落在当前工作树的 tmp/ 下，绝不触碰真实用户目录、活动安装或
#     modules/**/target 内的真实构建产物（L08 的 junction 夹具也是 tmp 内的）。
#   * "正常构建模式"用**合成构建消息 + 合成生产者目录**驱动真实的
#     scripts/package-all.ps1（夹具把 capture 的构建命令替换为回放消息的脚本），
#     因此被测的是生产脚本本身，不是测试替身逻辑。
#   * "旧行为"会被本测试的负例直接判失败：声明源存在但来源不可核对时必须拒绝。
# ============================================================================

$workspace = Split-Path -Parent $PSScriptRoot
$packageScript = Join-Path $PSScriptRoot 'package-all.ps1'
$loaderLib = Join-Path $PSScriptRoot 'lib/webview2-loader.ps1'
$workspaceFull = [System.IO.Path]::GetFullPath($workspace).TrimEnd('\', '/')
$tmpRootFull = [System.IO.Path]::GetFullPath((Join-Path $workspaceFull 'tmp')).TrimEnd('\', '/')
$fixtureBaseRoot = [System.IO.Path]::GetFullPath((Join-Path $tmpRootFull 'package-webview2-loader-contract')).TrimEnd('\', '/')
$tmpPrefix = $tmpRootFull + [System.IO.Path]::DirectorySeparatorChar
if (-not $fixtureBaseRoot.StartsWith($tmpPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "fixture base root must stay inside the current worktree tmp directory: $fixtureBaseRoot"
}
# 夹具根**按运行隔离**：两个进程并发跑同一个测试时，不再互相清空同一个固定目录。
# 这与产品侧的契约是同一条线索：同一个共享输出槽位只能有一个发布者，
# 测试夹具也不应该让两次运行共用同一个固定槽位（否则表现为
# `Cannot find path ...\manifest.json` / `messages.jsonl` 之类的假失败）。
$fixtureRunId = (Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$fixtureRoot = Join-Path $fixtureBaseRoot $fixtureRunId
if (-not $fixtureRoot.StartsWith($tmpPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "fixture root must stay inside the current worktree tmp directory: $fixtureRoot"
}
# 并发契约用例的落点独立于 $fixtureRoot：本脚本 cleanup 不删除它，因此每轮的真实进程
# 结果（worker 日志、双方输出、最终产物）全部留盘可复核。
$concurrencyRoot = [System.IO.Path]::GetFullPath((Join-Path $tmpRootFull 'package-webview2-loader-concurrency')).TrimEnd('\', '/')
if (-not $concurrencyRoot.StartsWith($tmpPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "concurrency root must stay inside the current worktree tmp directory: $concurrencyRoot"
}

<#
  清理**久置**的按运行隔离夹具目录（默认 6 小时）。

  只删名字形如 <yyyyMMdd-HHmmss>-<8hex> 的运行目录，绝不碰别的形状（可能是旧版固定根
  残留，或正在被另一个进程写入的目录）；也绝不删除基根本身。
#>
function Remove-StaleFixtureRuns {
  param([string]$BaseRoot, [int]$StaleHours = 6)

  if (-not (Test-Path -LiteralPath $BaseRoot)) { return }
  $cutoff = (Get-Date).ToUniversalTime().AddHours(-$StaleHours)
  foreach ($item in @(Get-ChildItem -LiteralPath $BaseRoot -Directory -Force -ErrorAction SilentlyContinue)) {
    if ($item.Name -notmatch '^\d{8}-\d{6}-[0-9a-f]{8}$') { continue }
    if ($item.LastWriteTimeUtc -lt $cutoff) {
      Remove-Item -LiteralPath $item.FullName -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}

. $loaderLib

# 真实的构建入口输入（夹具的身份校验指向这些真实文件，避免用到临时文件当身份）
$realBuildEntryManifest = 'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml'
$realLockfile = 'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.lock'
$realSourceIdentity = [pscustomobject]@{
  files = @(
    'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml',
    'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.lock',
    'modules/gui-desktop/packages/tauri-shell/src-tauri/build.rs',
    'modules/gui-desktop/packages/tauri-shell/src-tauri/tauri.conf.json'
  )
  directories = @()
  exclude_patterns = @()
}

$script:caseResults = [System.Collections.Generic.List[object]]::new()

function Add-CaseResult {
  param([string]$Id, [string]$Status, [string]$Detail)
  $script:caseResults.Add([pscustomobject]@{ id = $Id; status = $Status; detail = $Detail })
  Write-Host ("[{0}] {1}: {2}" -f $Status, $Id, $Detail)
}

function New-SyntheticPe {
  param(
    [int]$TotalLength = 4096,
    [byte]$Fill = 0x90,
    [int]$Machine = 0x8664
  )
  $bytes = New-Object 'byte[]' $TotalLength
  for ($index = 0; $index -lt $TotalLength; $index++) { $bytes[$index] = $Fill }
  $bytes[0] = 0x4D
  $bytes[1] = 0x5A
  $peOffset = 0x80
  $bytes[0x3C] = [byte]($peOffset -band 0xFF)
  $bytes[0x3D] = [byte](($peOffset -shr 8) -band 0xFF)
  $bytes[0x3E] = [byte](($peOffset -shr 16) -band 0xFF)
  $bytes[0x3F] = [byte](($peOffset -shr 24) -band 0xFF)
  $bytes[$peOffset] = 0x50
  $bytes[$peOffset + 1] = 0x45
  $bytes[$peOffset + 2] = 0x00
  $bytes[$peOffset + 3] = 0x00
  $bytes[$peOffset + 4] = [byte]($Machine -band 0xFF)
  $bytes[$peOffset + 5] = [byte](($Machine -shr 8) -band 0xFF)
  return $bytes
}

function Write-FixtureBytes {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [AllowEmptyCollection()][byte[]]$Bytes = @()
  )
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [System.IO.File]::WriteAllBytes($Path, $Bytes)
}

function Write-FixtureText {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [AllowEmptyString()][string]$Text = ''
  )
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Get-FixtureHash {
  param([string]$Path)
  return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-FixtureRelative {
  param([string]$Path)
  return ([System.IO.Path]::GetFullPath($Path)).Substring($workspaceFull.Length + 1).Replace('\', '/')
}

<#
  生成一个完整夹具：
    <case>/tauri-target/<profile>/build/webview2-com-sys-<hash>/out/x64/WebView2Loader.dll
    <case>/messages.jsonl                合成 cargo 构建消息（回放给 package-all 的 capture）
    <case>/emit-messages.ps1             capture 的替身构建命令（只回放消息）
    <case>/fake-shell.exe                build_entry artifact 的源文件（占位）
    <case>/manifest.json                 夹具 manifest
#>
function New-LoaderFixture {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [object[]]$Producers = @(),
    [int[]]$MessageProducerIndexes = @(),
    [string]$ProfileDirectory = 'debug',
    [switch]$OmitMessageFormat,
    [switch]$ReceiptPathIsDirectory,
    [string]$ProducerRootOverride,
    # 夹具落点（并发契约用例要落到不会被本脚本 cleanup 掉的独立目录里）。
    [string]$Root = $fixtureRoot,
    # 覆盖 export.source_identity.files（"声明源输入变化"用例需要夹具内可变的输入）。
    [string[]]$SourceIdentityFiles = @()
  )

  $caseRoot = Join-Path $Root $Name
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $producerRoot = if ($ProducerRootOverride) { $ProducerRootOverride } else { Join-Path $caseRoot 'tauri-target' }
  $profileRoot = Join-Path $producerRoot $ProfileDirectory
  $buildRoot = Join-Path $profileRoot 'build'

  $messages = @()
  $index = 0
  foreach ($producer in $Producers) {
    $outDir = Join-Path (Join-Path $buildRoot ("webview2-com-sys-{0}" -f $producer.hash)) 'out'
    $dllPath = Join-Path (Join-Path $outDir 'x64') 'WebView2Loader.dll'
    # 注意：不能用真值判断，"0 字节的生产者输出"也必须写盘（用于空文件负例）。
    if ($producer.PSObject.Properties['bytes']) {
      Write-FixtureBytes -Path $dllPath -Bytes $producer.bytes
    }
    if ($producer.skipDll -ne $true) {
      Write-FixtureText -Path (Join-Path (Split-Path -Parent $outDir) 'invoked.timestamp') -Text 'This file has an mtime of when this was started.'
    }
    if ($producer.mtime) {
      (Get-Item -LiteralPath $dllPath -Force).LastWriteTimeUtc = $producer.mtime
    }
    if ($producer.invokeTimestamp) {
      (Get-Item -LiteralPath (Join-Path (Split-Path -Parent $outDir) 'invoked.timestamp') -Force).LastWriteTimeUtc = $producer.invokeTimestamp
    }
    if (@($MessageProducerIndexes) -contains $index) {
      $linkedPath = 'native=' + (Join-Path $outDir 'x64')
      $messages += ('{"reason":"build-script-executed","package_id":"registry+https://github.com/rust-lang/crates.io-index#webview2-com-sys@0.38.2","out_dir":' + (ConvertTo-Json $outDir -Compress) + ',"linked_paths":[' + (ConvertTo-Json $linkedPath -Compress) + '],"linked_libs":["advapi32"],"cfgs":[],"env":[],"filenames":[]}')
    }
    $index++
  }

  $messageFile = Join-Path $caseRoot 'messages.jsonl'
  Write-FixtureText -Path $messageFile -Text (($messages -join "`n") + $(if ($messages.Count -gt 0) { "`n" } else { '' }))
  # 其它 reason 的噪声行，确保解析只挑 build-script-executed
  Add-Content -LiteralPath $messageFile -Value '{"reason":"compiler-artifact","package_id":"registry+https://github.com/rust-lang/crates.io-index#webview2-com-sys@0.38.2","filenames":[]}' -Encoding UTF8

  $emitScript = Join-Path $caseRoot 'emit-messages.ps1'
  Write-FixtureText -Path $emitScript -Text "param([string]`$MessageFile)`nGet-Content -LiteralPath `$MessageFile`n"

  $fakeShell = Join-Path $caseRoot 'fake-shell.exe'
  Write-FixtureBytes -Path $fakeShell -Bytes (New-SyntheticPe -Fill 0x33)

  # 夹具的所有 manifest 路径都从 $caseRoot 推导，保证 -Root 覆盖时 build_root / source /
  # receipt / package_root 仍然落在夹具自己里面。
  $caseRelative = (Get-FixtureRelative $caseRoot).TrimEnd('/')
  $stableRelative = "$caseRelative/stable/WebView2Loader.dll"
  $receiptRelative = "$caseRelative/export/webview2-loader-export.json"
  $stablePath = Join-Path $workspaceFull $stableRelative
  $receiptPath = Join-Path $workspaceFull $receiptRelative
  if ($ReceiptPathIsDirectory) {
    New-Item -ItemType Directory -Force -Path $receiptPath | Out-Null
  }

  $captureArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $emitScript, $messageFile)
  if (-not $OmitMessageFormat) {
    # package-all 要求 capture 的构建命令显式带 --message-format；替身命令会把它当普通参数忽略。
    $captureArgs += '--message-format=json-render-diagnostics'
  }

  $identityFiles = if ($SourceIdentityFiles.Count -gt 0) { @($SourceIdentityFiles) } else { @($realSourceIdentity.files) }

  $manifest = [ordered]@{
    package_root = "$caseRelative/package"
    backup_keep = 2
    artifacts = @(
      [ordered]@{
        id = 'gui-desktop.tauri-shell'
        source = "$caseRelative/fake-shell.exe"
        target = 'bin/coolzhu-tauri-shell.exe'
        build = [ordered]@{
          command = 'powershell'
          args = $captureArgs
          working_dir = '.'
          capture = 'cargo-json-messages'
        }
      },
      [ordered]@{
        id = 'gui-desktop.webview2-loader'
        source = $stableRelative
        target = 'bin/WebView2Loader.dll'
        export = [ordered]@{
          kind = 'cargo-build-script-output'
          producer_package = 'webview2-com-sys'
          architecture = 'x64'
          build_entry_artifact = 'gui-desktop.tauri-shell'
          build_root = "$caseRelative/tauri-target/{profile}"
          producer_arch_directory = 'x64'
          producer_file_name = 'WebView2Loader.dll'
          build_entry_manifest = $realBuildEntryManifest
          lockfile = $realLockfile
          receipt = $receiptRelative
          source_identity = [ordered]@{
            files = @($identityFiles)
            directories = @()
            exclude_patterns = @()
          }
        }
      }
    )
    resources = @()
  }

  $manifestPath = Join-Path $caseRoot 'manifest.json'
  Write-FixtureText -Path $manifestPath -Text ($manifest | ConvertTo-Json -Depth 12)

  return [pscustomobject]@{
    name = $Name
    caseRoot = $caseRoot
    manifestPath = $manifestPath
    messageFile = $messageFile
    producerRoot = $producerRoot
    buildRoot = $buildRoot
    stablePath = $stablePath
    receiptPath = $receiptPath
    packagePath = Join-Path $caseRoot 'package'
    reportPath = Join-Path $caseRoot 'report.json'
  }
}

function New-FixtureContext {
  param(
    [Parameter(Mandatory = $true)][object]$Fixture,
    [long]$BuildStartedOffsetMinutes = 0,
    [switch]$NoMessages
  )

  $manifest = Get-Content -Raw -LiteralPath $Fixture.manifestPath -Encoding UTF8 | ConvertFrom-LoaderJson
  $artifact = @($manifest.artifacts | Where-Object { $_.id -eq 'gui-desktop.webview2-loader' })[0]
  $export = $artifact.export
  $messages = if ($NoMessages) { @() } else { @(Get-Content -LiteralPath $Fixture.messageFile -Encoding UTF8) }
  return @{
    Artifact = $artifact
    Export = $export
    ArtifactId = [string]$artifact.id
    Profile = 'debug'
    RepoPath = $workspaceFull
    StablePath = $Fixture.stablePath
    ReceiptPath = $Fixture.receiptPath
    BuildEntryArtifact = 'gui-desktop.tauri-shell'
    BuildEntryManifest = [string]$export.build_entry_manifest
    BuildEntryManifestSha256 = Get-FixtureHash (Join-Path $workspaceFull ([string]$export.build_entry_manifest))
    LockfileRelative = [string]$export.lockfile
    LockfileSha256 = Get-FixtureHash (Join-Path $workspaceFull ([string]$export.lockfile))
    CargoTargetDir = 'target'
    BuildTarget = 'x86_64-pc-windows-msvc'
    HostTarget = 'x86_64-pc-windows-msvc'
    CargoVersion = 'cargo test-harness'
    ReleaseVersion = ''
    BuildCommand = 'powershell'
    BuildArgs = @('-NoProfile', '-File', 'emit-messages.ps1', $Fixture.messageFile, '--message-format=json-render-diagnostics')
    BuildWorkingDir = '.'
    BuildStartedUtc = (Get-Date).ToUniversalTime().AddMinutes($BuildStartedOffsetMinutes).ToString('o')
    BuildMessages = $messages
    RawRecordPath = $Fixture.messageFile
  }
}

<#
  以夹具 manifest 运行真实的 package-all.ps1。
  返回 { Error; Report; PackageRoot }。
#>
function Invoke-PackageFixture {
  param(
    [Parameter(Mandatory = $true)][object]$Fixture,
    [switch]$SkipBuild
  )

  $result = [pscustomobject]@{ Error = $null; Report = $null; ReportExists = $false }
  Push-Location $workspace
  try {
    $arguments = @{
      Manifest = $Fixture.manifestPath
      Configuration = 'debug'
      ReportPath = $Fixture.reportPath
    }
    if ($SkipBuild) { $arguments.SkipBuild = $true }
    & $packageScript @arguments | Out-Null
    if (Test-Path -LiteralPath $Fixture.reportPath -PathType Leaf) {
      $result.ReportExists = $true
      $result.Report = Get-Content -Raw -LiteralPath $Fixture.reportPath -Encoding UTF8 | ConvertFrom-LoaderJson
    }
  } catch {
    $result.Error = $_.Exception.Message
  } finally {
    Pop-Location
  }
  return $result
}

function Get-StagedLoaderPath {
  param([object]$Fixture)
  $manifest = Get-Content -Raw -LiteralPath $Fixture.manifestPath -Encoding UTF8 | ConvertFrom-LoaderJson
  $artifact = @($manifest.artifacts | Where-Object { $_.id -eq 'gui-desktop.webview2-loader' })[0]
  return Join-Path $Fixture.packagePath ([string]$artifact.target).Replace('/', '\')
}

function Assert-Category {
  param(
    [string]$Id,
    [string]$ExpectedCategory,
    [object]$PackageResult,
    [string]$Note = ''
  )
  if (-not $PackageResult.Error) {
    Add-CaseResult -Id $Id -Status 'FAIL' -Detail "expected failure $ExpectedCategory but the run succeeded"
    return
  }
  if ($PackageResult.Error -notmatch [regex]::Escape($ExpectedCategory)) {
    Add-CaseResult -Id $Id -Status 'FAIL' -Detail "expected $ExpectedCategory, got: $($PackageResult.Error -split "`n" | Select-Object -First 1)"
    return
  }
  Add-CaseResult -Id $Id -Status 'PASS' -Detail ("rejected with {0}{1}" -f $ExpectedCategory, $(if ($Note) { " ($Note)" } else { '' }))
}

function Assert-NoLoaderStaged {
  param([string]$Id, [object]$Fixture, [object]$PackageResult)
  $staged = Get-StagedLoaderPath -Fixture $Fixture
  if (Test-Path -LiteralPath $staged) {
    Add-CaseResult -Id $Id -Status 'FAIL' -Detail "loader must not be staged on failure: $staged"
    return $false
  }
  if ($PackageResult.ReportExists) {
    Add-CaseResult -Id $Id -Status 'FAIL' -Detail 'package report must not be written when the loader cannot be verified'
    return $false
  }
  return $true
}

# ============================================================================
# PKG-L07c 并发导出与收据一致性契约
#
# 口径（裁决第六轮 §四）：
#   * 同一个**实际导出目标**只允许一个发布者；锁范围按共享输出槽位（规范输出目录 +
#     目标文件 + package target + profile），不能只按 build ID。
#   * 允许的结果：一成功一 Busy，或受控串行后各自成功；每次消费都必须得到自洽的一组
#     产物与收据。**不允许**两个调用各自成功却留下 A 的 DLL 与 B 的收据。
#   * 产物与收据是**连续替换两个文件**，不是跨文件原子提交；进程在两者之间退出时，
#     消费端必须拒绝该不完整代次，而不是把它当上一成功版本。
#   * 并发时序一律用命名事件屏障触发（两个真实进程在固定点同时进入），不靠 sleep 碰运气。
#
# 每轮的全部真实进程输出、退出码与最终产物身份都落盘在
# tmp/package-webview2-loader-concurrency/<run>/round-NNN/ 下，供复核。
# ============================================================================

$L07cWorkerScriptText = @'
# PKG-L07c 工作者进程：真实子进程，只调用被测 lib，不复制生产逻辑。
param(
  [Parameter(Mandatory = $true)][string]$LibPath,
  [Parameter(Mandatory = $true)][string]$Mode,
  [Parameter(Mandatory = $true)][string]$LogPath,
  [string]$Source,
  [string]$Destination,
  [string]$ExpectedSha256,
  [long]$ExpectedLength = 0,
  [int]$Iterations = 1,
  [string]$RepoPath,
  [string]$ManifestPath,
  [string]$ArtifactId = 'gui-desktop.webview2-loader',
  [string]$Profile = 'debug',
  [string]$StablePath,
  [string]$ReceiptPath,
  [string]$MessageFile,
  # PKG-L07c 固定代次语义：产出/消费这一代次的运行 ID 与本次消费的独立 staging。
  [string]$RunId = 'l07c-worker-run',
  [string]$ConsumerRunId,
  [string]$ConsumeStagingDirectory,
  [string]$StagedFileName,
  [switch]$VerifyIdentityFlag
)

$ErrorActionPreference = 'Stop'
. $LibPath

$script:records = [System.Collections.Generic.List[string]]::new()

function Add-Record {
  param([hashtable]$Record)
  $script:records.Add(($Record | ConvertTo-Json -Compress -Depth 8))
}

function New-ExportContext {
  $manifest = Get-Content -Raw -LiteralPath $ManifestPath -Encoding UTF8 | ConvertFrom-LoaderJson
  $artifact = @($manifest.artifacts | Where-Object { [string]$_.id -eq $ArtifactId })[0]
  $export = $artifact.export
  $entryManifest = [string]$export.build_entry_manifest
  $lockfile = [string]$export.lockfile
  return @{
    Artifact = $artifact
    Export = $export
    ArtifactId = [string]$artifact.id
    Profile = $Profile
    RepoPath = $RepoPath
    StablePath = $StablePath
    ReceiptPath = $ReceiptPath
    BuildEntryArtifact = [string]$export.build_entry_artifact
    BuildEntryManifest = $entryManifest
    BuildEntryManifestSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $RepoPath $entryManifest)).Hash.ToLowerInvariant()
    LockfileRelative = $lockfile
    LockfileSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $RepoPath $lockfile)).Hash.ToLowerInvariant()
    CargoTargetDir = 'target'
    BuildTarget = 'x86_64-pc-windows-msvc'
    HostTarget = 'x86_64-pc-windows-msvc'
    CargoVersion = 'cargo test-harness'
    ReleaseVersion = ''
    BuildCommand = 'powershell'
    BuildArgs = @()
    BuildWorkingDir = '.'
    BuildStartedUtc = (Get-Date).ToUniversalTime().AddMinutes(-1).ToString('o')
    BuildMessages = @(Get-Content -LiteralPath $MessageFile -Encoding UTF8)
    RawRecordPath = $MessageFile
    # 固定代次语义：产出/消费这一代次的运行 ID + 本次消费的独立 staging。
    RunId = $RunId
    ProducerRunId = $RunId
    ConsumerRunId = $(if ($ConsumerRunId) { $ConsumerRunId } else { $RunId })
    ConsumeStagingDirectory = $ConsumeStagingDirectory
  }
}

function New-TargetRecord {
  param([string]$Mode, [object]$Slot, [bool]$Ok, [string]$ErrorType, [string]$ErrorText)
  return @{
    mode = $Mode
    ok = $Ok
    error_type = $ErrorType
    error_message = ($ErrorText -split "`n")[0]
    error_text = $ErrorText
    slot_key = $Slot.key
    slot_lock = $Slot.lock_path
  }
}

# 重要：工作者进程**只在持有发布/读取权时**打开槽位里的文件。
# 未加锁地读产物（例如 Get-FileHash / Get-Content）会让另一个正在发布的进程的
# ReplaceFile 以 ERROR_SHARING_VIOLATION 失败——读侧也是这条一致性边界的一部分。
# 因此这里只做**不持有句柄**的存在性探测，内容身份由驱动器在双方退出后核对。
function Test-SlotFilePresent {
  param([string]$Path)
  return [System.IO.File]::Exists($Path)
}

$slot = $null
switch ($Mode) {
  'copy' {
    $slot = Get-LoaderPublishSlot -Destination $Destination
    for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
      $ok = $false
      $errorType = ''
      $errorText = ''
      try {
        [void](Copy-LoaderVerifiedFile -Source $Source -Destination $Destination -ExpectedSha256 $ExpectedSha256 -ExpectedLength $ExpectedLength)
        $ok = $true
      } catch {
        $errorType = $_.Exception.GetType().FullName
        $errorText = $_.Exception.Message
      }
      $record = New-TargetRecord -Mode 'copy' -Slot $slot -Ok $ok -ErrorType $errorType -ErrorText $errorText
      $record.iteration = $iteration
      $record.destination_present_after_call = Test-SlotFilePresent $Destination
      Add-Record $record
    }
  }
  'export' {
    $context = New-ExportContext
    $slot = Get-LoaderPublishSlot -Destination $StablePath -RepoPath $RepoPath -Target ([string]$context.Artifact.target) -Profile $Profile
    for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
      $ok = $false
      $errorType = ''
      $errorText = ''
      $generation = ''
      $receiptSha = ''
      $state = ''
      try {
        $result = Export-LoaderArtifact -Context $context
        $ok = $true
        $generation = [string]$result.publication.generation
        $receiptSha = [string]$result.file_identity.stable_export_sha256
        $state = [string]$result.publication.state
      } catch {
        $errorType = $_.Exception.GetType().FullName
        $errorText = $_.Exception.Message
      }
      $record = New-TargetRecord -Mode 'export' -Slot $slot -Ok $ok -ErrorType $errorType -ErrorText $errorText
      $record.iteration = $iteration
      $record.generation = $generation
      $record.receipt_sha256 = $receiptSha
      $record.publication_state = $state
      $record.expected_raw_sha256 = $ExpectedSha256
      if ($result) {
        $record.artifact_digest = [string]$result.publication.artifact_digest
        $record.artifact_length = [long]$result.publication.artifact_length
        $record.receipt_digest = [string]$result.publication.receipt_digest
        $record.build_input_digest = [string]$result.publication.build_input_digest
        $record.producer_run_id = [string]$result.publication.producer_run_id
        $record.declared_receipt_path = [string]$result.publication.declared_receipt_path
      }
      if ($context.Consumption) {
        # 导出路径在发布权内完成的消费（固定消费收据）：逐字段登记，供驱动器核对
        # "报告字段 ↔ 实际消费内容"。
        $consumptionRecord = $context.Consumption
        foreach ($name in @(
            'artifact_id', 'generation', 'producer_run_id', 'consumer_run_id', 'artifact_digest',
            'artifact_length', 'receipt_digest', 'build_input_digest', 'slot_key', 'generation_record',
            'generation_record_path', 'pointer_path', 'pointer_generation_at_consumption',
            'consumed_staging_path', 'consumed_staging_absolute', 'consumed_staging_sha256',
            'consumed_staging_length', 'consumed_content_matches_generation', 'consumption_verified'
          )) {
          if ($consumptionRecord.PSObject.Properties[$name]) { $record['consumption_' + $name] = $consumptionRecord.$name }
        }
      }
      Add-Record $record
    }
  }
  'consume-fixed' {
    $context = New-ExportContext
    $slot = Get-LoaderPublishSlot -Destination $StablePath -RepoPath $RepoPath -Target ([string]$context.Artifact.target) -Profile $Profile
    $ok = $false
    $errorType = ''
    $errorText = ''
    $consumption = $null
    try {
      $consumption = Use-LoaderExportGeneration `
        -Context $context `
        -ConsumerRunId ([string]$context.ConsumerRunId) `
        -ConsumeStagingDirectory ([string]$context.ConsumeStagingDirectory) `
        -StagedFileName $StagedFileName `
        -VerifyIdentity:$VerifyIdentityFlag
      $ok = $true
    } catch {
      $errorType = $_.Exception.GetType().FullName
      $errorText = $_.Exception.Message
    }
    $record = New-TargetRecord -Mode 'consume-fixed' -Slot $slot -Ok $ok -ErrorType $errorType -ErrorText $errorText
    $record['expected_raw_sha256'] = $ExpectedSha256
    if ($consumption) {
      # 只登记标量字段（嵌套对象不写入日志，避免深度/体积问题）。
      foreach ($name in @(
          'artifact_id', 'target', 'profile', 'slot_key', 'slot_dir', 'destination', 'generation',
          'producer_run_id', 'consumer_run_id', 'artifact_digest', 'artifact_length', 'receipt_digest',
          'build_input_digest', 'receipt_path', 'generation_record', 'generation_record_path',
          'pointer_path', 'pointer_generation_at_consumption', 'pointer_committed_utc',
          'supersedes_generation', 'consumed_staging_path', 'consumed_staging_absolute',
          'consumed_staging_sha256', 'consumed_staging_length', 'consumed_content_matches_generation',
          'consumption_verified', 'consumption_source', 'read_right', 'fixed_at_utc', 'report_generation_rule'
        )) {
        if ($consumption.PSObject.Properties[$name]) { $record[$name] = $consumption.$name }
      }
    }
    Add-Record $record
  }
  'consume' {
    $context = New-ExportContext
    $slot = Get-LoaderPublishSlot -Destination $StablePath -RepoPath $RepoPath -Target ([string]$context.Artifact.target) -Profile $Profile
    $ok = $false
    $errorType = ''
    $errorText = ''
    $generation = ''
    $receiptSha = ''
    $state = ''
    try {
      # 消费者自己会在内部取得读取权再读，因此这里可以安全地拿内容身份。
      $validated = Assert-LoaderExportReceipt -Context $context
      $ok = $true
      $generation = [string]$validated.receipt.publication.generation
      $receiptSha = [string]$validated.receipt.file_identity.stable_export_sha256
      $state = [string]$validated.receipt.publication.state
    } catch {
      $errorType = $_.Exception.GetType().FullName
      $errorText = $_.Exception.Message
    }
    $record = New-TargetRecord -Mode 'consume' -Slot $slot -Ok $ok -ErrorType $errorType -ErrorText $errorText
    $record.generation = $generation
    $record.receipt_sha256 = $receiptSha
    $record.publication_state = $state
    Add-Record $record
  }
  default {
    throw "unknown worker mode: $Mode"
  }
}

[System.IO.File]::WriteAllLines($LogPath, $script:records, [System.Text.UTF8Encoding]::new($false))
'@

function Get-L07cSha {
  param([string]$Path)
  if (Test-Path -LiteralPath $Path -PathType Leaf) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
  }
  return ''
}

function Read-L07cWorkerRecords {
  param([string]$LogPath)
  $records = @()
  if (-not (Test-Path -LiteralPath $LogPath)) { return $records }
  foreach ($line in @(Get-Content -LiteralPath $LogPath -Encoding UTF8)) {
    if (-not $line.Trim()) { continue }
    try { $records += ($line | ConvertFrom-LoaderJson) } catch { }
  }
  return $records
}

<#
  命名事件屏障：驱动器先创建 GO 事件与每个 worker 的 READY 事件，worker 在
  COOLZHU_LOADER_TEST_SEAM 指定的固定点设置自己的 READY 并等待 GO。
  两个 READY 都满足 = 两个真实进程**确实同时**处在发布窗口/发布权持有状态，
  这就是"并发时序"的证据本身，不依赖 sleep。
#>
function New-L07cBarrier {
  param([int]$Workers, [string]$Token)

  $readyNames = @()
  $readyHandles = @()
  for ($index = 0; $index -lt $Workers; $index++) {
    $name = "Local\coolzhu-l07c-$Token-ready-$index"
    $readyNames += $name
    $readyHandles += [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $name)
  }
  $goName = "Local\coolzhu-l07c-$Token-go"
  return [pscustomobject]@{
    token = $Token
    go_name = $goName
    ready_names = $readyNames
    ready_handles = $readyHandles
    go_handle = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $goName)
  }
}

function Wait-L07cBarrierReady {
  param([Parameter(Mandatory = $true)][object]$Barrier, [int]$TimeoutMilliseconds = 60000)

  $missing = @()
  for ($index = 0; $index -lt $Barrier.ready_handles.Count; $index++) {
    if (-not $Barrier.ready_handles[$index].WaitOne($TimeoutMilliseconds)) { $missing += $index }
  }
  return $missing
}

function Close-L07cBarrier {
  param([Parameter(Mandatory = $true)][object]$Barrier)
  foreach ($handle in $Barrier.ready_handles) { $handle.Dispose() }
  $Barrier.go_handle.Dispose()
}

function Start-L07cWorker {
  param(
    [Parameter(Mandatory = $true)][string]$WorkerScript,
    [Parameter(Mandatory = $true)][hashtable]$Arguments,
    [hashtable]$Environment = @{},
    [Parameter(Mandatory = $true)][string]$StdOutPath,
    [Parameter(Mandatory = $true)][string]$StdErrPath
  )

  $argumentList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $WorkerScript)
  foreach ($key in $Arguments.Keys) {
    $value = $Arguments[$key]
    if ($null -eq $value) { continue }
    if ([string]$value -eq '') { continue }
    $argumentList += ('-' + $key)
    $argumentList += [string]$value
  }

  $startInfo = New-Object System.Diagnostics.ProcessStartInfo
  # 与父测试使用同一 PowerShell 宿主；PS7 启动 WinPS5 会继承不兼容的模块搜索环境，
  # 使工作者连 Get-FileHash 都找不到，误报成暂存文件消失。
  $startInfo.FileName = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
  $startInfo.Arguments = (($argumentList | ForEach-Object {
        if ([string]$_ -match '[\s"]') { '"' + ([string]$_ -replace '"', '\"') + '"' } else { [string]$_ }
      }) -join ' ')
  $startInfo.WorkingDirectory = $workspaceFull
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  foreach ($name in $Environment.Keys) { $startInfo.EnvironmentVariables[$name] = [string]$Environment[$name] }

  $process = [System.Diagnostics.Process]::Start($startInfo)
  return [pscustomobject]@{
    process = $process
    stdout_task = $process.StandardOutput.ReadToEndAsync()
    stderr_task = $process.StandardError.ReadToEndAsync()
    stdout_path = $StdOutPath
    stderr_path = $StdErrPath
    pid = $process.Id
  }
}

function Complete-L07cWorker {
  param([Parameter(Mandatory = $true)][object]$Worker, [int]$TimeoutMilliseconds = 180000)

  $exited = $Worker.process.WaitForExit($TimeoutMilliseconds)
  if (-not $exited) {
    try { $Worker.process.Kill() } catch { }
  }
  $out = ''
  $err = ''
  try { $out = [string]$Worker.stdout_task.Result } catch { }
  try { $err = [string]$Worker.stderr_task.Result } catch { }
  [System.IO.File]::WriteAllText($Worker.stdout_path, $out, [System.Text.UTF8Encoding]::new($false))
  [System.IO.File]::WriteAllText($Worker.stderr_path, $err, [System.Text.UTF8Encoding]::new($false))
  return [pscustomobject]@{
    pid = $Worker.pid
    exit_code = $(if ($exited) { $Worker.process.ExitCode } else { -1 })
    timed_out = (-not $exited)
    stdout = $out
    stderr = $err
  }
}

function Get-L07cSlotFiles {
  param([string]$Directory)
  if (-not (Test-Path -LiteralPath $Directory)) { return @() }
  return @(Get-ChildItem -LiteralPath $Directory -Force -File | Where-Object {
      $_.Name -like '.tmp-*' -or $_.Name -like '.bak-*' -or
      $_.Name -like '.loader-stage-*' -or $_.Name -like '.loader-publish-*'
    })
}

function Add-L07cRoundResult {
  param([string]$CaseId, [int]$Round, [bool]$Passed, [string]$Detail)

  if (-not $script:l07cResults.ContainsKey($CaseId)) { $script:l07cResults[$CaseId] = New-Object System.Collections.Generic.List[object] }
  $script:l07cResults[$CaseId].Add([pscustomobject]@{ round = $Round; passed = $Passed; detail = $Detail })
  $script:l07cRoundLog.Add(([pscustomobject]@{ case = $CaseId; round = $Round; passed = $Passed; detail = $Detail } | ConvertTo-Json -Compress))
  if ($Passed) {
    Write-Host ("[round {0:D2}] {1}: ok - {2}" -f $Round, $CaseId, $Detail)
  } else {
    Write-Host ("[round {0:D2}] {1}: FAIL - {2}" -f $Round, $CaseId, $Detail)
  }
}

function New-L07cCaseFiles {
  param(
    [Parameter(Mandatory = $true)][string]$CaseRoot,
    [Parameter(Mandatory = $true)][string]$WorkerScript,
    [Parameter(Mandatory = $true)][object]$Barrier,
    [Parameter(Mandatory = $true)][int]$WorkerIndex,
    [Parameter(Mandatory = $true)][hashtable]$ModuleArguments,
    [int]$SlotWaitSeconds = 60,
    [string]$SeamPoint,
    [string]$SeamAction = 'park'
  )

  $environment = @{
    COOLZHU_LOADER_SLOT_WAIT_SECONDS = [string]$SlotWaitSeconds
    COOLZHU_LOADER_SLOT_HOST_ID = $Barrier.token
    COOLZHU_LOADER_TEST_READY_EVENT = $Barrier.ready_names[$WorkerIndex]
    COOLZHU_LOADER_TEST_GO_EVENT = $Barrier.go_name
    COOLZHU_LOADER_TEST_TIMEOUT_SECONDS = '180'
  }
  if ($SeamPoint) {
    $environment['COOLZHU_LOADER_TEST_SEAM'] = $SeamPoint
    $environment['COOLZHU_LOADER_TEST_SEAM_ACTION'] = $SeamAction
  }

  $startArguments = @{ WorkerScript = $WorkerScript; Arguments = $ModuleArguments; Environment = $environment }
  $startArguments.StdOutPath = Join-Path $CaseRoot ("worker-{0}-stdout.log" -f $WorkerIndex)
  $startArguments.StdErrPath = Join-Path $CaseRoot ("worker-{0}-stderr.log" -f $WorkerIndex)
  return Start-L07cWorker @startArguments
}

function Get-L07cExportFixture {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$Root,
    [string[]]$SourceIdentityFiles = @()
  )

  # 两个生产者：两个真实进程分别导出**不同内容**，因此"两代次混配"是可检测的。
  $hashA = 'a1a1a1a1a1a1a1a1'
  $hashB = 'b2b2b2b2b2b2b2b2'
  $fixture = New-LoaderFixture -Name $Name -Root $Root -Producers @(
    [pscustomobject]@{ hash = $hashA; bytes = (New-SyntheticPe -TotalLength 65536 -Fill 0x61) },
    [pscustomobject]@{ hash = $hashB; bytes = (New-SyntheticPe -TotalLength 65536 -Fill 0x62) }
  ) -MessageProducerIndexes @(0) -SourceIdentityFiles $SourceIdentityFiles

  $messageA = $fixture.messageFile
  $messageB = Join-Path $fixture.caseRoot 'messages-b.jsonl'
  $textA = Get-Content -Raw -LiteralPath $messageA -Encoding UTF8
  Write-FixtureText -Path $messageB -Text $textA.Replace($hashA, $hashB)

  $producerDllA = Join-Path $fixture.buildRoot ("webview2-com-sys-{0}/out/x64/WebView2Loader.dll" -f $hashA)
  $producerDllB = Join-Path $fixture.buildRoot ("webview2-com-sys-{0}/out/x64/WebView2Loader.dll" -f $hashB)
  return [pscustomobject]@{
    fixture = $fixture
    messageA = $messageA
    messageB = $messageB
    producerShaA = Get-L07cSha $producerDllA
    producerShaB = Get-L07cSha $producerDllB
    producerDllA = $producerDllA
    producerDllB = $producerDllB
  }
}

function New-L07cExportWorkerArguments {
  param(
    [Parameter(Mandatory = $true)][object]$Bundle,
    [Parameter(Mandatory = $true)][string]$Mode,
    [Parameter(Mandatory = $true)][string]$MessageFile,
    [Parameter(Mandatory = $true)][string]$LogPath,
    [string]$ExpectedSha256 = ''
  )

  return @{
    LibPath = $loaderLib
    Mode = $Mode
    LogPath = $LogPath
    RepoPath = $workspaceFull
    ManifestPath = $Bundle.fixture.manifestPath
    ArtifactId = 'gui-desktop.webview2-loader'
    Profile = 'debug'
    StablePath = $Bundle.fixture.stablePath
    ReceiptPath = $Bundle.fixture.receiptPath
    MessageFile = $MessageFile
    ExpectedSha256 = $ExpectedSha256
  }
}

# ---------------------------------------------------------------- 用例 1 / 2 -----
# 两个真实进程争用同一个导出目标（copy 契约）：
#   wait=0  → 一致的成功 / 明确 Busy，且没有交叉覆盖、没有残片；
#   wait=60 → 受控串行后各自成功，每次落地都是完整可识别内容。
function Invoke-L07cCopyRaceCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [int]$Iterations, [int]$SlotWaitSeconds, [string]$Token)

  $caseId = if ($SlotWaitSeconds -eq 0) { 'L07c-concurrent-export' } else { 'L07c-concurrent-export-controlled-serial' }
  $caseRoot = Join-Path $RoundRoot ("copy-race-wait{0}" -f $SlotWaitSeconds)
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null

  $destination = Join-Path $caseRoot 'WebView2Loader.dll'
  $sources = @()
  $expected = @()
  foreach ($fill in @(0x31, 0x32)) {
    $sourcePath = Join-Path $caseRoot ("source-{0:X2}.bin" -f $fill)
    Write-FixtureBytes -Path $sourcePath -Bytes (New-SyntheticPe -TotalLength 262144 -Fill $fill)
    $sources += $sourcePath
    $expected += (Get-FixtureHash $sourcePath)
  }
  # 先放一代进去：目标始终存在，逼迫并发走 ReplaceFile 语义（而不是"首次 Move"）。
  Copy-Item -LiteralPath $sources[0] -Destination $destination -Force

  $barrier = New-L07cBarrier -Workers 2 -Token $Token
  $workers = @()
  $completions = @()
  $barrierMissing = @()
  try {
    for ($index = 0; $index -lt 2; $index++) {
      $workers += New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $barrier -WorkerIndex $index `
        -SlotWaitSeconds $SlotWaitSeconds -SeamPoint 'before-lock' `
        -ModuleArguments @{
          LibPath = $loaderLib
          Mode = 'copy'
          LogPath = Join-Path $caseRoot ("worker-{0}.jsonl" -f $index)
          Source = $sources[$index]
          Destination = $destination
          ExpectedSha256 = $expected[$index]
          ExpectedLength = 262144
          Iterations = $Iterations
        }
    }
    $barrierMissing = @(Wait-L07cBarrierReady -Barrier $barrier)
    if ($barrierMissing.Count -eq 0) { [void]$barrier.go_handle.Set() }
  } finally {
    try { [void]$barrier.go_handle.Set() } catch { }
    foreach ($worker in $workers) { $completions += Complete-L07cWorker -Worker $worker }
    Close-L07cBarrier -Barrier $barrier
  }

  $records = @()
  for ($index = 0; $index -lt 2; $index++) {
    $records += @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot ("worker-{0}.jsonl" -f $index)))
  }
  $failures = @($records | Where-Object { -not $_.ok })
  $nonBusy = @($failures | Where-Object { $_.error_message -notmatch 'EXPORT-SLOT-BUSY' })
  $successes = @($records | Where-Object { $_.ok })
  # 注意：这里的观测来自**未持有发布权**的进程，而 ReplaceFile 语义下目标会短暂不可见
  # （实测：写者 300 次替换期间，另一个进程 39532 次观测里有 764 次看到目标不存在），
  # 且未加锁的读句柄本身会让并发发布的 ReplaceFile 以共享冲突失败（见 evidence-*.log）。
  # 因此这里只做"不持有句柄"的存在性统计，不当作失败判据；发布者删除已发布代次的行为由
  # "终态必须是完整代次"以及消费者的拒绝路径来判。
  $transientMissing = @($records | Where-Object { -not $_.destination_present_after_call }).Count
  $finalSha = Get-L07cSha $destination
  $leftovers = @(Get-L07cSlotFiles -Directory $caseRoot)

  $problems = @()
  if ($barrierMissing.Count -gt 0) { $problems += ("两个真实进程没有同时到达发布窗口（屏障未满足，缺少 ready#{0}）" -f ($barrierMissing -join ',')) }
  if ($completions.Count -ne 2 -or @($completions | Where-Object { $_.timed_out }).Count -gt 0) { $problems += '有工作者进程超时未退出' }
  if ($records.Count -ne (2 * $Iterations)) { $problems += ("完成的调用数 {0} != {1}（有进程没跑完）" -f $records.Count, 2 * $Iterations) }
  if ($nonBusy.Count -gt 0) {
    $problems += ("出现非 Busy 失败（并发发布未被约束到明确 Busy）：" + (($nonBusy | ForEach-Object { $_.error_type + '::' + $_.error_message }) -join ' | '))
  }
  if ($successes.Count -lt 1) { $problems += '没有任何一次调用成功' }
  if ($SlotWaitSeconds -gt 0 -and $failures.Count -gt 0) {
    $problems += ("受控串行口径下不应有任何失败，实际 {0} 次（示例：{1}）" -f $failures.Count, $failures[0].error_message)
  }
  if ($expected -notcontains $finalSha) { $problems += ("最终产物不是任何一个完整代次的内容（可能是混合/半成品）：$finalSha") }
  if ($leftovers.Count -gt 0) { $problems += ('留下锁/暂存/临时残片：' + (($leftovers | ForEach-Object { $_.Name }) -join ', ')) }

  $detail = ("calls={0} ok={1} busy={2} barrier=ok final=one-complete-generation leftovers=0 unlocked_reader_saw_missing={3}" -f $records.Count, $successes.Count, $failures.Count, $transientMissing)
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- 用例 3 -----
# 两个真实进程跑**真实导出**（产物 + 收据两次发布）竞争同一槽位：
# 受控串行后各自成功，且每次完成时盘上的"产物 + 收据"必须自洽 —— 绝不允许
# 最终留下 A 的 DLL 配 B 的收据。
function Invoke-L07cExporterRaceCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [int]$SlotWaitSeconds, [string]$Token)

  $caseId = 'L07c-concurrent-exporter-consistent-generation'
  $caseRoot = Join-Path $RoundRoot 'exporter-race'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'exporter-race' -Root $caseRoot

  $barrier = New-L07cBarrier -Workers 2 -Token $Token
  $workers = @()
  $completions = @()
  $barrierMissing = @()
  try {
    for ($index = 0; $index -lt 2; $index++) {
      $messageFile = if ($index -eq 0) { $bundle.messageA } else { $bundle.messageB }
      $producerSha = if ($index -eq 0) { $bundle.producerShaA } else { $bundle.producerShaB }
      $workers += New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $barrier -WorkerIndex $index `
        -SlotWaitSeconds $SlotWaitSeconds `
        -SeamPoint 'before-lock' `
        -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $messageFile `
          -LogPath (Join-Path $caseRoot ("worker-{0}.jsonl" -f $index)) -ExpectedSha256 $producerSha)
    }
    $barrierMissing = @(Wait-L07cBarrierReady -Barrier $barrier)
    if ($barrierMissing.Count -eq 0) { [void]$barrier.go_handle.Set() }
  } finally {
    try { [void]$barrier.go_handle.Set() } catch { }
    foreach ($worker in $workers) { $completions += Complete-L07cWorker -Worker $worker }
    Close-L07cBarrier -Barrier $barrier
  }

  $records = @()
  for ($index = 0; $index -lt 2; $index++) {
    $records += @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot ("worker-{0}.jsonl" -f $index)))
  }
  $failures = @($records | Where-Object { -not $_.ok })
  $successes = @($records | Where-Object { $_.ok })
  $finalSha = Get-L07cSha $bundle.fixture.stablePath
  $finalGeneration = ''
  $finalReceiptSha = ''
  $finalState = ''
  $finalSlotKey = ''
  if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
    $receipt = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $finalGeneration = [string]$receipt.publication.generation
    $finalReceiptSha = [string]$receipt.file_identity.stable_export_sha256
    $finalState = [string]$receipt.publication.state
    $finalSlotKey = [string]$receipt.publication.slot_key
  }
  $expectedSlot = (Get-LoaderPublishSlot -Destination $bundle.fixture.stablePath -RepoPath $workspaceFull -Target 'bin/WebView2Loader.dll' -Profile 'debug').key
  $leftovers = @(Get-L07cSlotFiles -Directory (Split-Path -Parent $bundle.fixture.stablePath))
  $leftovers += @(Get-L07cSlotFiles -Directory (Split-Path -Parent $bundle.fixture.receiptPath))

  $problems = @()
  if ($barrierMissing.Count -gt 0) { $problems += ("两个真实导出进程没有同时进入发布窗口（屏障未满足，缺少 ready#{0}）" -f ($barrierMissing -join ',')) }
  if ($failures.Count -gt 0) {
    $problems += ("受控串行口径下不应有失败：{0}（{1}）" -f $failures.Count, $failures[0].error_message)
  }
  if ($successes.Count -lt 1) { $problems += '没有任何一次导出成功' }
  foreach ($success in $successes) {
    if ([string]$success.publication_state -ne 'complete') { $problems += ("成功导出的收据 state != complete：{0}" -f $success.publication_state) }
    if ([string]$success.receipt_sha256 -ne [string]$success.expected_raw_sha256) {
      $problems += ("一次导出返回的收据描述的不是它自己那份生产者产物：receipt={0} expected={1}" -f $success.receipt_sha256, $success.expected_raw_sha256)
    }
    if (-not $success.generation) { $problems += '成功导出的收据没有发布代次（generation）' }
  }
  if (-not $finalReceiptSha) { $problems += '最终没有可解析的收据' }
  if ($finalReceiptSha -ne $finalSha) {
    $problems += ("最终盘上是 A 的 DLL 配 B 的收据（跨代次混配）：dll={0} receipt={1}" -f $finalSha, $finalReceiptSha)
  }
  if ($finalState -ne 'complete') { $problems += ("最终收据不是完整代次：state={0}" -f $finalState) }
  if ($finalSlotKey -ne $expectedSlot) { $problems += ("最终收据的槽位与当前输出槽位不一致：{0} != {1}" -f $finalSlotKey, $expectedSlot) }
  $generations = @($successes | ForEach-Object { [string]$_.generation })
  if ($generations -notcontains $finalGeneration) { $problems += ("最终收据的代次不属于任何一个成功导出：{0} not in {1}" -f $finalGeneration, ($generations -join ',')) }
  if ($leftovers.Count -gt 0) { $problems += ('留下锁/暂存/临时残片：' + (($leftovers | ForEach-Object { $_.Name }) -join ', ')) }

  $detail = ("exports={0} ok={1} final_generation={2} receipt==dll final_state=complete leftovers=0" -f $records.Count, $successes.Count, $finalGeneration)
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- 用例 4 -----
# 产物写完、收据未完成时进程退出：后续消费者必须拒绝该不完整代次，
# 并且被杀进程留下的锁必须能被下一个发布者按 pid 存活判定回收。
function Invoke-L07cIncompleteGenerationCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07c-incomplete-generation-rejected'
  $caseRoot = Join-Path $RoundRoot 'incomplete-generation'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'incomplete-generation' -Root $caseRoot
  $slotDir = Split-Path -Parent $bundle.fixture.stablePath

  # 步骤 1：先有一个**完整**代次（收据描述 A）。步骤 2：被杀进程发布 B 的产物后立刻退出，
  # 收据仍是 A ⇒ 盘上就是"不完整代次"状态。
  $seedBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-seed')
  $seedWorker = $null
  $seedCompletion = @()
  try {
    $seedWorker = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $seedBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 `
      -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageA `
        -LogPath (Join-Path $caseRoot 'seed.jsonl') -ExpectedSha256 $bundle.producerShaA)
    $seedCompletion += Complete-L07cWorker -Worker $seedWorker -TimeoutMilliseconds 120000
  } finally {
    Close-L07cBarrier -Barrier $seedBarrier
  }
  $seedSha = Get-L07cSha $bundle.fixture.stablePath
  $seedReceiptSha = ''
  if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
    $seedReceipt = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $seedReceiptSha = [string]$seedReceipt.file_identity.stable_export_sha256
  }

  # 进程在两次文件替换之间**硬退出**（不展开 finally、不写收据、不释放锁文件）。
  $exitBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-exit')
  $exitWorker = $null
  $exitCompletion = @()
  try {
    $exitWorker = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $exitBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 `
      -SeamPoint 'between-file-and-receipt' -SeamAction 'exit' `
      -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
        -LogPath (Join-Path $caseRoot 'exit.jsonl') -ExpectedSha256 $bundle.producerShaB)
    $exitCompletion += Complete-L07cWorker -Worker $exitWorker -TimeoutMilliseconds 120000
  } finally {
    Close-L07cBarrier -Barrier $exitBarrier
  }
  $afterExitSha = Get-L07cSha $bundle.fixture.stablePath
  $afterExitReceiptSha = ''
  if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
    $afterExitReceipt = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $afterExitReceiptSha = [string]$afterExitReceipt.file_identity.stable_export_sha256
  }

  # 步骤 3：消费者必须拒绝。
  $consumerWorker = Start-L07cWorker -WorkerScript $WorkerScript `
    -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'consume' -MessageFile $bundle.messageB `
      -LogPath (Join-Path $caseRoot 'consume.jsonl')) `
    -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '60'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-consume') } `
    -StdOutPath (Join-Path $caseRoot 'consume-stdout.log') -StdErrPath (Join-Path $caseRoot 'consume-stderr.log')
  $consumerCompletion = Complete-L07cWorker -Worker $consumerWorker -TimeoutMilliseconds 120000
  $consumerRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'consume.jsonl'))
  $consumer = if ($consumerRecords.Count -gt 0) { $consumerRecords[0] } else { $null }

  # 步骤 4：被杀进程留下的锁必须可回收（否则一次崩溃会永久阻塞该槽位的发布）。
  $recoverBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-recover')
  $recoverWorker = $null
  $recoverCompletion = @()
  try {
    $recoverWorker = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $recoverBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 `
      -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
        -LogPath (Join-Path $caseRoot 'recover.jsonl') -ExpectedSha256 $bundle.producerShaB)
    $recoverCompletion += Complete-L07cWorker -Worker $recoverWorker -TimeoutMilliseconds 120000
  } finally {
    Close-L07cBarrier -Barrier $recoverBarrier
  }
  $recoverRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'recover.jsonl'))
  $recover = if ($recoverRecords.Count -gt 0) { $recoverRecords[0] } else { $null }
  $finalSha = Get-L07cSha $bundle.fixture.stablePath
  $finalReceiptSha = ''
  if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
    $finalReceipt = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $finalReceiptSha = [string]$finalReceipt.file_identity.stable_export_sha256
  }
  $leftovers = @(Get-L07cSlotFiles -Directory $slotDir)

  $problems = @()
  $seedExit = @($seedCompletion | ForEach-Object { $_.exit_code }) -join ','
  $exitExit = @($exitCompletion | ForEach-Object { $_.exit_code }) -join ','
  if ($seedExit -ne '0' -or $seedReceiptSha -ne $seedSha) {
    $problems += ("前置的完整代次没有建立：exit={0} dll={1} receipt={2}" -f $seedExit, $seedSha, $seedReceiptSha)
  }
  if ($exitExit -ne '97') {
    $problems += ("'产物已替换、收据未完成'的进程没有按预期硬退出：exit={0}（期望 97）" -f $exitExit)
  }
  if ($afterExitSha -ne $bundle.producerShaB) { $problems += ("退出前产物没有被替换成新代次：{0}" -f $afterExitSha) }
  if ($afterExitReceiptSha -ne $bundle.producerShaA) { $problems += ("退出后收据不再是上一代（前提不成立）：{0}" -f $afterExitReceiptSha) }
  if (-not $consumer -or $consumer.ok) {
    $problems += '消费者接受了"产物已换、收据未更新"的不完整代次（必须拒绝）'
  } elseif ([string]$consumer.error_text -notmatch 'CONTENT-MISMATCH') {
    $problems += ("消费者拒绝的类别不是 CONTENT-MISMATCH：{0}" -f $consumer.error_message)
  } elseif ([string]$consumer.error_text -notmatch 'incomplete_generation_or_tampered=true') {
    $problems += '消费者没有把该状态标记为不完整代次/被替换（缺少 incomplete_generation_or_tampered 标记）'
  }
  if (-not $recover -or -not $recover.ok) {
    $problems += ("被杀进程留下的锁没有被回收，后续发布者无法发布：{0}" -f $(if ($recover) { $recover.error_message } else { '没有记录' }))
  }
  if ($finalReceiptSha -ne $finalSha) { $problems += ("回收后的最终代次不自洽：dll={0} receipt={1}" -f $finalSha, $finalReceiptSha) }
  if ($leftovers.Count -gt 0) { $problems += ('留下锁/暂存/临时残片：' + (($leftovers | ForEach-Object { $_.Name }) -join ', ')) }

  $detail = 'exit_code=97 consumer_rejected=CONTENT-MISMATCH incomplete_marked=true lock_reclaimed=true final_pair_consistent=true'
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- 用例 5 -----
# 收据存在但产物内容被替换：校验必须失败，不允许发布。
function Invoke-L07cTamperedArtifactCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07c-receipt-with-tampered-artifact-rejected'
  $caseRoot = Join-Path $RoundRoot 'tampered-artifact'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'tampered-artifact' -Root $caseRoot

  $seedBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-tamper')
  $seedWorker = $null
  $seedCompletion = @()
  try {
    $seedWorker = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $seedBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 `
      -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageA `
        -LogPath (Join-Path $caseRoot 'seed.jsonl') -ExpectedSha256 $bundle.producerShaA)
    $seedCompletion += Complete-L07cWorker -Worker $seedWorker -TimeoutMilliseconds 120000
  } finally {
    Close-L07cBarrier -Barrier $seedBarrier
  }

  # 第三方（不经发布权）替换稳定导出产物，收据保持完整。
  Write-FixtureBytes -Path $bundle.fixture.stablePath -Bytes (New-SyntheticPe -TotalLength 65536 -Fill 0x7F)
  $tamperedSha = Get-L07cSha $bundle.fixture.stablePath

  $consumerWorker = Start-L07cWorker -WorkerScript $WorkerScript `
    -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'consume' -MessageFile $bundle.messageA `
      -LogPath (Join-Path $caseRoot 'consume.jsonl')) `
    -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '60'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-tamper-consume') } `
    -StdOutPath (Join-Path $caseRoot 'consume-stdout.log') -StdErrPath (Join-Path $caseRoot 'consume-stderr.log')
  $consumerCompletion = Complete-L07cWorker -Worker $consumerWorker -TimeoutMilliseconds 120000
  $consumerRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'consume.jsonl'))
  $consumer = if ($consumerRecords.Count -gt 0) { $consumerRecords[0] } else { $null }

  $problems = @()
  $seedExit = @($seedCompletion | ForEach-Object { $_.exit_code }) -join ','
  if ($seedExit -ne '0') { $problems += ("前置的完整代次没有建立：exit={0}" -f $seedExit) }
  if (-not $consumer -or $consumer.ok) {
    $problems += '收据存在但产物被替换时消费者仍然接受（必须拒绝发布）'
  } elseif ([string]$consumer.error_text -notmatch 'CONTENT-MISMATCH') {
    $problems += ("拒绝类别不是 CONTENT-MISMATCH：{0}" -f $consumer.error_message)
  }
  if ($tamperedSha -eq $bundle.producerShaA) { $problems += '夹具没有真正替换产物内容' }

  $detail = 'tampered=true consumer_rejected=CONTENT-MISMATCH'
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- 用例 6 -----
# 一个竞争者失败或被取消，不得删除赢家的锁 / 暂存 / 成功结果。
function Invoke-L07cLoserIsolationCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07c-loser-does-not-touch-winner'
  $caseRoot = Join-Path $RoundRoot 'loser-isolation'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'loser-isolation' -Root $caseRoot
  $slotDir = Split-Path -Parent $bundle.fixture.stablePath

  # 赢家：完成产物 + 收据的发布会话，但停在收据落地之后（仍持有发布权）。
  $winnerBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-winner')
  $winner = $null
  $winnerCompletion = @()
  $winnerProblems = @()
  try {
    $winner = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $winnerBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 -SeamPoint 'after-receipt' -SeamAction 'park' `
      -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageA `
        -LogPath (Join-Path $caseRoot 'winner.jsonl') -ExpectedSha256 $bundle.producerShaA)
    $missing = @(Wait-L07cBarrierReady -Barrier $winnerBarrier)
    if ($missing.Count -gt 0) { $winnerProblems += '赢家没有到达发布窗口（屏障未满足）' }

    $lockPath = (Get-LoaderPublishSlot -Destination $bundle.fixture.stablePath -RepoPath $workspaceFull -Target 'bin/WebView2Loader.dll' -Profile 'debug').lock_path
    $holderBefore = Read-LoaderPublishHolder -LockPath $lockPath
    $dllBefore = Get-L07cSha $bundle.fixture.stablePath
    $receiptBefore = Get-L07cSha $bundle.fixture.receiptPath
    if (-not $holderBefore) { $winnerProblems += '赢家持有发布权时锁记录不可读' }

    # 竞争者 1：明确 Busy（wait=0），不得改动赢家的任何东西。
    $busyWorker = Start-L07cWorker -WorkerScript $WorkerScript `
      -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
        -LogPath (Join-Path $caseRoot 'busy.jsonl') -ExpectedSha256 $bundle.producerShaB) `
      -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '0'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-busy') } `
      -StdOutPath (Join-Path $caseRoot 'busy-stdout.log') -StdErrPath (Join-Path $caseRoot 'busy-stderr.log')
    $busyCompletion = Complete-L07cWorker -Worker $busyWorker -TimeoutMilliseconds 120000
    $busyRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'busy.jsonl'))
    $busy = if ($busyRecords.Count -gt 0) { $busyRecords[0] } else { $null }

    # 竞争者 2：进入等待后被取消（杀掉）。
    $cancelWorker = Start-L07cWorker -WorkerScript $WorkerScript `
      -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
        -LogPath (Join-Path $caseRoot 'cancel.jsonl') -ExpectedSha256 $bundle.producerShaB) `
      -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '60'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-cancel') } `
      -StdOutPath (Join-Path $caseRoot 'cancel-stdout.log') -StdErrPath (Join-Path $caseRoot 'cancel-stderr.log')
    Start-Sleep -Milliseconds 400
    $cancelled = $false
    try {
      Stop-Process -Id $cancelWorker.process.Id -Force -ErrorAction Stop
      $cancelled = $true
    } catch {
      $cancelled = $false
    }
    [void](Complete-L07cWorker -Worker $cancelWorker -TimeoutMilliseconds 30000)

    # 读取侧遵守同一边界：赢家持锁期间，消费者不允许读到"看起来存在"的中间状态，
    # 而是 fail-closed（EXPORT-SLOT-BUSY），且同样不得改动赢家的东西。
    $consumeWorker = Start-L07cWorker -WorkerScript $WorkerScript `
      -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'consume' -MessageFile $bundle.messageA `
        -LogPath (Join-Path $caseRoot 'consume-while-locked.jsonl')) `
      -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '0'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-consume') } `
      -StdOutPath (Join-Path $caseRoot 'consume-stdout.log') -StdErrPath (Join-Path $caseRoot 'consume-stderr.log')
    [void](Complete-L07cWorker -Worker $consumeWorker -TimeoutMilliseconds 120000)
    $consumeRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'consume-while-locked.jsonl'))
    $consume = if ($consumeRecords.Count -gt 0) { $consumeRecords[0] } else { $null }
    if (-not $consume -or $consume.ok) {
      $winnerProblems += '消费者在发布者持有发布权期间仍然读到了槽位内容（读侧未遵守一致性边界）'
    } elseif ([string]$consume.error_text -notmatch 'EXPORT-SLOT-BUSY') {
      $winnerProblems += ("消费者在发布者持锁期间的失败类别不是 EXPORT-SLOT-BUSY：{0}" -f $consume.error_message)
    }

    # 断言：赢家的锁（同一 token）、产物与收据都没有被竞争者改动。
    $holderAfter = Read-LoaderPublishHolder -LockPath $lockPath
    if (-not $holderAfter) {
      $winnerProblems += '竞争者失败/被取消后赢家的锁文件消失了（竞争者不得删除他人的锁）'
    } elseif ([string]$holderAfter.owner_token -ne [string]$holderBefore.owner_token) {
      $winnerProblems += '竞争者失败/被取消后赢家的锁被他人接管（token 变化）'
    }
    if ((Get-L07cSha $bundle.fixture.stablePath) -ne $dllBefore) { $winnerProblems += '竞争者改动了赢家正在发布的产物' }
    if ((Get-L07cSha $bundle.fixture.receiptPath) -ne $receiptBefore) { $winnerProblems += '竞争者改动了赢家的收据' }
    if (-not $busy -or $busy.ok) { $winnerProblems += 'wait=0 的竞争者没有返回明确 Busy' }
    elseif ([string]$busy.error_text -notmatch 'EXPORT-SLOT-BUSY') { $winnerProblems += ("竞争者的失败类别不是 EXPORT-SLOT-BUSY：{0}" -f $busy.error_message) }
    if (-not $cancelled) { $winnerProblems += '取消竞争者的步骤没有生效（无法验证"被取消的竞争者"）' }
  } finally {
    try { [void]$winnerBarrier.go_handle.Set() } catch { }
    if ($winner) { $winnerCompletion += Complete-L07cWorker -Worker $winner -TimeoutMilliseconds 120000 }
    Close-L07cBarrier -Barrier $winnerBarrier
  }

  $winnerRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'winner.jsonl'))
  $winnerRecord = if ($winnerRecords.Count -gt 0) { $winnerRecords[0] } else { $null }
  if (-not $winnerRecord -or -not $winnerRecord.ok) {
    $winnerProblems += ("赢家在释放发布权后没有成功完成：{0}" -f $(if ($winnerRecord) { $winnerRecord.error_message } else { '没有记录' }))
  }
  $finalDll = Get-L07cSha $bundle.fixture.stablePath
  $finalReceipt = ''
  if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
    $receiptObject = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $finalReceipt = [string]$receiptObject.file_identity.stable_export_sha256
  }
  if ($finalReceipt -ne $finalDll) { $winnerProblems += ("赢家完成后的代次不自洽：dll={0} receipt={1}" -f $finalDll, $finalReceipt) }
  $leftovers = @(Get-L07cSlotFiles -Directory $slotDir)
  if ($leftovers.Count -gt 0) { $winnerProblems += ('留下锁/暂存/临时残片：' + (($leftovers | ForEach-Object { $_.Name }) -join ', ')) }

  $detail = 'winner_state_intact=true busy=EXPORT-SLOT-BUSY cancelled_competitor=ok consumer_blocked=EXPORT-SLOT-BUSY winner_finally_completed=true'
  if ($winnerProblems.Count -gt 0) { $detail = ($winnerProblems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($winnerProblems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- 用例 7 -----
# 不同合法输出目标并发：不被无关全局锁错误串行化，也不相互污染。
function Invoke-L07cDistinctSlotsCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07c-distinct-slots-not-serialized'
  $caseRoot = Join-Path $RoundRoot 'distinct-slots'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundleOne = Get-L07cExportFixture -Name 'slot-one' -Root $caseRoot
  $bundleTwo = Get-L07cExportFixture -Name 'slot-two' -Root $caseRoot
  $lockOne = (Get-LoaderPublishSlot -Destination $bundleOne.fixture.stablePath -RepoPath $workspaceFull -Target 'bin/WebView2Loader.dll' -Profile 'debug').lock_path
  $lockTwo = (Get-LoaderPublishSlot -Destination $bundleTwo.fixture.stablePath -RepoPath $workspaceFull -Target 'bin/WebView2Loader.dll' -Profile 'debug').lock_path

  # 两个进程同时停在"产物已发布、收据未落地"的窗口里：如果存在无关的全局锁，
  # 第二个进程根本到不了这个点，屏障就不会满足。
  $barrier = New-L07cBarrier -Workers 2 -Token ($Token + '-slots')
  $workers = @()
  $completions = @()
  $barrierMissing = @()
  $lockIntact = $false
  try {
    $specs = @(
      @{ bundle = $bundleOne; message = $bundleOne.messageA; sha = $bundleOne.producerShaA },
      @{ bundle = $bundleTwo; message = $bundleTwo.messageB; sha = $bundleTwo.producerShaB }
    )
    for ($index = 0; $index -lt 2; $index++) {
      $spec = $specs[$index]
      $workers += New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $barrier -WorkerIndex $index `
        -SlotWaitSeconds 60 -SeamPoint 'between-file-and-receipt' -SeamAction 'park' `
        -ModuleArguments (New-L07cExportWorkerArguments -Bundle $spec.bundle -Mode 'export' -MessageFile $spec.message `
          -LogPath (Join-Path $caseRoot ("worker-{0}.jsonl" -f $index)) -ExpectedSha256 $spec.sha)
    }
    $barrierMissing = @(Wait-L07cBarrierReady -Barrier $barrier)
    if ($barrierMissing.Count -eq 0) {
      # 两个槽位的锁必须同时、各自存在（证明是两个独立发布权，而不是一把全局锁）。
      $holderOne = Read-LoaderPublishHolder -LockPath $lockOne
      $holderTwo = Read-LoaderPublishHolder -LockPath $lockTwo
      $lockIntact = ($null -ne $holderOne) -and ($null -ne $holderTwo) -and ($lockOne -ne $lockTwo)
    }
    $barrier.go_handle.Set()
  } finally {
    try { [void]$barrier.go_handle.Set() } catch { }
    foreach ($worker in $workers) { $completions += Complete-L07cWorker -Worker $worker }
    Close-L07cBarrier -Barrier $barrier
  }

  $records = @()
  for ($index = 0; $index -lt 2; $index++) {
    $records += @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot ("worker-{0}.jsonl" -f $index)))
  }
  $failures = @($records | Where-Object { -not $_.ok })
  $problems = @()
  if ($barrierMissing.Count -gt 0) {
    $problems += ("两个不同输出槽位的发布者没有同时进入发布窗口（屏障未满足，缺少 ready#{0}）⇒ 存在无关的全局串行化" -f ($barrierMissing -join ','))
  }
  if (-not $lockIntact) { $problems += '两个槽位的发布权没有各自独立地同时存在（锁范围可能过宽）' }
  if ($records.Count -ne 2) { $problems += ("完成的导出数 {0} != 2" -f $records.Count) }
  if ($failures.Count -gt 0) { $problems += ("不同槽位的并发导出不应失败：{0}" -f $failures[0].error_message) }
  foreach ($record in $records) {
    if ($record.ok -and [string]$record.receipt_sha256 -ne [string]$record.expected_raw_sha256) {
      $problems += ("某个槽位的导出得到的是别人的内容：{0} != {1}" -f $record.receipt_sha256, $record.expected_raw_sha256)
    }
  }
  foreach ($bundle in @($bundleOne, $bundleTwo)) {
    $dll = Get-L07cSha $bundle.fixture.stablePath
    $receipt = ''
    if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
      $receiptObject = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
      $receipt = [string]$receiptObject.file_identity.stable_export_sha256
    }
    if ($dll -ne $receipt) { $problems += ("槽位相互污染或代次不自洽：{0} dll={1} receipt={2}" -f $bundle.fixture.name, $dll, $receipt) }
    $leftovers = @(Get-L07cSlotFiles -Directory (Split-Path -Parent $bundle.fixture.stablePath))
    if ($leftovers.Count -gt 0) { $problems += ('留下锁/暂存/临时残片：' + (($leftovers | ForEach-Object { $_.Name }) -join ', ')) }
  }

  $detail = 'both_parked_simultaneously=true distinct_locks=true zero_failures=true no_cross_slot_pollution=true'
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- 用例 8 -----
# 相关源输入变化：按打包政策拒绝该次发布（不写收据），不靠 quiescent=false 放行。
function Invoke-L07cSourceInputChangedCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07c-source-input-changed-rejected'
  $caseRoot = Join-Path $RoundRoot 'source-input-changed'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null

  # 声明输入里放一个夹具内可变的文件，模拟"导出窗口内源码/配置被编辑"。
  $mutableRelative = (Get-FixtureRelative (Join-Path $caseRoot 'declared-input/tauri.conf.json'))
  Write-FixtureText -Path (Join-Path $workspaceFull $mutableRelative) -Text '{"before":true}'
  $bundle = Get-L07cExportFixture -Name 'source-input-changed' -Root $caseRoot -SourceIdentityFiles @($mutableRelative)

  # 步骤 1：先有一个完整代次（输入是 before 内容）。
  $seedWorker = Start-L07cWorker -WorkerScript $WorkerScript `
    -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageA `
      -LogPath (Join-Path $caseRoot 'seed.jsonl') -ExpectedSha256 $bundle.producerShaA) `
    -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '60'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-seed') } `
    -StdOutPath (Join-Path $caseRoot 'seed-stdout.log') -StdErrPath (Join-Path $caseRoot 'seed-stderr.log')
  $seedCompletion = Complete-L07cWorker -Worker $seedWorker -TimeoutMilliseconds 120000
  $seedReceiptSha = ''
  if (Test-Path -LiteralPath $bundle.fixture.receiptPath -PathType Leaf) {
    $seedReceiptObject = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $seedReceiptSha = [string]$seedReceiptObject.file_identity.stable_export_sha256
  }
  $receiptShaBeforeMutation = Get-L07cSha $bundle.fixture.receiptPath

  # 步骤 2：导出停在"产物已发布、收据未落地"时改写声明输入 ⇒ 本次发布必须不获准。
  $barrier = New-L07cBarrier -Workers 1 -Token ($Token + '-changed')
  $worker = $null
  $completion = @()
  $barrierMissing = @()
  $receiptUnchanged = $false
  $errorText = ''
  try {
    $worker = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $barrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 -SeamPoint 'between-file-and-receipt' -SeamAction 'park' `
      -ModuleArguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
        -LogPath (Join-Path $caseRoot 'changed.jsonl') -ExpectedSha256 $bundle.producerShaB)
    $barrierMissing = @(Wait-L07cBarrierReady -Barrier $barrier)
    if ($barrierMissing.Count -eq 0) {
      Write-FixtureText -Path (Join-Path $workspaceFull $mutableRelative) -Text '{"after":true}'
    }
  } finally {
    try { [void]$barrier.go_handle.Set() } catch { }
    if ($worker) { $completion += Complete-L07cWorker -Worker $worker -TimeoutMilliseconds 120000 }
    Close-L07cBarrier -Barrier $barrier
  }
  $changedRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'changed.jsonl'))
  $changed = if ($changedRecords.Count -gt 0) { $changedRecords[0] } else { $null }
  if ($changed) { $errorText = [string]$changed.error_text }
  $rejected = ($null -ne $changed) -and (-not $changed.ok) -and ($errorText -match 'SOURCE-INPUT-CHANGED')
  $receiptUnchanged = ((Get-L07cSha $bundle.fixture.receiptPath) -eq $receiptShaBeforeMutation)

  # 步骤 3：产物已被换、收据仍是旧代次 ⇒ 消费者必须在发布前拒绝。
  $consumerWorker = Start-L07cWorker -WorkerScript $WorkerScript `
    -Arguments (New-L07cExportWorkerArguments -Bundle $bundle -Mode 'consume' -MessageFile $bundle.messageB `
      -LogPath (Join-Path $caseRoot 'consume.jsonl')) `
    -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '60'; COOLZHU_LOADER_SLOT_HOST_ID = ($Token + '-changed-consume') } `
    -StdOutPath (Join-Path $caseRoot 'consume-stdout.log') -StdErrPath (Join-Path $caseRoot 'consume-stderr.log')
  $consumerCompletion = Complete-L07cWorker -Worker $consumerWorker -TimeoutMilliseconds 120000
  $consumerRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'consume.jsonl'))
  $consumer = if ($consumerRecords.Count -gt 0) { $consumerRecords[0] } else { $null }

  $problems = @()
  if ($seedCompletion.exit_code -ne 0) { $problems += '前置的完整代次没有建立' }
  if ($barrierMissing.Count -gt 0) { $problems += '导出进程没有到达"产物已发布、收据未落地"的窗口（屏障未满足）' }
  if (-not $rejected) {
    $problems += ("声明源输入在导出窗口内变化时没有按政策拒绝发布：{0}" -f $(if ($changed) { $changed.error_message } else { '没有记录' }))
  }
  if (-not $receiptUnchanged) { $problems += '被拒绝的发布仍然改写了收据（不允许：不得给混合输入补一张可发布收据）' }
  if (-not $consumer -or $consumer.ok) { $problems += '消费者接受了输入变化后留下的产物/收据组合' }

  $detail = 'source_input_changed=SOURCE-INPUT-CHANGED receipt_not_written=true consumer_rejected=true'
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ============================================================================
# L07d：固定代次语义与发布临界区完整性（PKG-L07c P0 补强，裁决 §3.2/§3.3 七项验收）
#
# 七项验收的落点（逐项对应）：
#   ① 失败方不能覆盖赢家收据        → L07d-loser-has-no-write-access-to-valid-generation
#   ② 产物提交与收据提交之间退出，消费者拒绝不完整代次
#                                  → L07c-incomplete-generation-rejected（既存，产物→收据之间退出）
#                                    + L07d-exit-between-receipt-and-pointer（新增：收据→指针之间退出）
#   ③ 消费 G1 后发布 G2，报告仍精确绑定 G1
#                                  → L07d-consume-g1-then-publish-g2
#   ④ generation 正确但内容错配仍拒绝
#                                  → L07d-generation-correct-content-mismatch
#   ⑤ 同一 generation 对应不同收据内容判冲突
#                                  → L07d-same-generation-different-receipt-content
#   ⑥ 失败报告不能被执行 --no-build 当作有效发布收据
#                                  → 本文件 L07d-package-report-generation-binding（报告字段核对）
#                                    + scripts/test-package-build-identity.ps1（I14a/I14b：--no-build 拒绝
#                                      失败运行的产物身份，且失败运行不产出成功报告）
#   ⑦ 测试不只断言字段存在，还要核对它与实际消费内容一致
#                                  → L07d-consumption-fields-match-consumed-content
# 并发时序一律用**真实双进程 + 命名事件屏障**（复用 L07c 的屏障与工作者基础设施），
# 并按固定轮数重复（-ConcurrencyRounds）。
# ============================================================================

<#
  单进程真实运行（无屏障）：用于"前置建立某一代"或"消费"这类准备动作。
  Mode: export / consume / consume-fixed。
#>
function Invoke-L07dWorkerOnce {
  param(
    [Parameter(Mandatory = $true)][string]$CaseRoot,
    [Parameter(Mandatory = $true)][string]$WorkerScript,
    [Parameter(Mandatory = $true)][object]$Bundle,
    [Parameter(Mandatory = $true)][string]$Mode,
    [Parameter(Mandatory = $true)][string]$MessageFile,
    [Parameter(Mandatory = $true)][string]$LogName,
    [string]$ExpectedSha = '',
    [string]$HostSuffix = 'once',
    [hashtable]$Extra = @{},
    [int]$TimeoutMilliseconds = 180000
  )

  $arguments = New-L07cExportWorkerArguments -Bundle $Bundle -Mode $Mode -MessageFile $MessageFile `
    -LogPath (Join-Path $CaseRoot $LogName) -ExpectedSha256 $ExpectedSha
  foreach ($key in $Extra.Keys) { $arguments[$key] = $Extra[$key] }
  $worker = Start-L07cWorker -WorkerScript $WorkerScript -Arguments $arguments `
    -Environment @{ COOLZHU_LOADER_SLOT_WAIT_SECONDS = '60'; COOLZHU_LOADER_SLOT_HOST_ID = ('l07d-' + $HostSuffix) } `
    -StdOutPath (Join-Path $CaseRoot ($LogName + '.stdout')) -StdErrPath (Join-Path $CaseRoot ($LogName + '.stderr'))
  $completion = Complete-L07cWorker -Worker $worker -TimeoutMilliseconds $TimeoutMilliseconds
  $records = @(Read-L07cWorkerRecords -LogPath (Join-Path $CaseRoot $LogName))
  return [pscustomobject]@{
    completion = $completion
    records = $records
    record = $(if ($records.Count -gt 0) { $records[0] } else { $null })
  }
}

function Get-L07dSlot {
  param([object]$Bundle)
  return (Get-LoaderPublishSlot -Destination $Bundle.fixture.stablePath -RepoPath $workspaceFull -Target 'bin/WebView2Loader.dll' -Profile 'debug')
}

function Get-L07dGenerationRecordPath {
  param([object]$Bundle, [string]$Generation)
  return (Resolve-LoaderGenerationRecordPath -Slot (Get-L07dSlot -Bundle $Bundle) -Generation $Generation)
}

# ---------------------------------------------------------------- ① / ⑤ -----
# 失败竞争者与"争用失败方"都不得向共享有效槽位发布自己的收据：
#   * 真实双进程 + 屏障：赢家停在"收据已提交、当前有效代次指针未发布"的窗口（仍持发布权），
#     竞争者立即失败为明确 Busy，且不得改动赢家的锁/产物/收据/指针；
#   * 事后注入（模拟"失败方确实把自己的收据写进了共享槽位"）：把一份**同 generation 但不同
#     内容**的收据写进声明收据路径 ⇒ 消费端必须判为冲突并拒绝；代次档案与指针保持原样。
function Invoke-L07dLoserNoWriteAccessCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07d-loser-has-no-write-access-to-valid-generation'
  $caseRoot = Join-Path $RoundRoot 'loser-write-access'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'loser-write-access' -Root $caseRoot
  $slot = Get-L07dSlot -Bundle $bundle

  # 前置：先有一代完整代次（G0），使槽位处于"有当前有效代次"的状态。
  $seed = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageA -LogName 'seed.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-seed')
  $seedPointer = $(if (Test-Path -LiteralPath $slot.pointer_path) { Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
  $seedGeneration = $(if ($seedPointer) { [string]$seedPointer.generation } else { '' })
  $seedRecordPath = $(if ($seedGeneration) { Get-L07dGenerationRecordPath -Bundle $bundle -Generation $seedGeneration } else { '' })
  $seedRecordSha = $(if ($seedRecordPath -and (Test-Path -LiteralPath $seedRecordPath)) { Get-L07cSha $seedRecordPath } else { '' })

  # 赢家：停在"代次档案 + 声明收据已提交、当前有效代次指针未发布"的窗口。
  $winnerBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-loser-winner')
  $winnerArguments = New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
    -LogPath (Join-Path $caseRoot 'winner.jsonl') -ExpectedSha256 $bundle.producerShaB
  $winnerArguments['RunId'] = ($Token + '-winner-run')
  $winnerProblems = @()
  $busy = $null
  $busyCompletion = @()
  $pointerDuringWindow = ''
  $declaredDuringWindow = ''
  $lockExistedDuringWindow = $false
  try {
    $winner = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $winnerBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 -SeamPoint 'between-generation-and-pointer' -SeamAction 'park' -ModuleArguments $winnerArguments
    $missing = @(Wait-L07cBarrierReady -Barrier $winnerBarrier)
    if ($missing.Count -gt 0) { $winnerProblems += '赢家没有到达"收据已提交、指针未发布"的窗口（屏障未满足）' }
    $lockExistedDuringWindow = [System.IO.File]::Exists($slot.lock_path)
    $declaredDuringWindow = Get-L07cSha $bundle.fixture.receiptPath

    # 竞争者（真实第二进程）：wait=0 ⇒ 明确 Busy，且不得触碰任何共享槽位内容。
    $busy = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
      -Mode 'export' -MessageFile $bundle.messageA -LogName 'busy.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-busy')
    $busyCompletion = @($busy.completion)

    $pointerDuringWindow = Get-L07cSha $slot.pointer_path
    $declaredAfterBusy = Get-L07cSha $bundle.fixture.receiptPath
    if ($declaredAfterBusy -ne $declaredDuringWindow) { $winnerProblems += '争用失败方改写了赢家正在提交的声明收据' }
    if ((Get-L07cSha $slot.pointer_path) -ne $pointerDuringWindow) { $winnerProblems += '争用失败方改写了槽位指针' }
    $recordPathNow = Get-L07dGenerationRecordPath -Bundle $bundle -Generation $seedGeneration
    if ((Get-L07cSha $recordPathNow) -ne $seedRecordSha) { $winnerProblems += '争用失败方改写了前一有效代次的档案收据' }
  } finally {
    try { [void]$winnerBarrier.go_handle.Set() } catch { }
  }
  $winnerCompletion = @()
  if ($winner) { $winnerCompletion += Complete-L07cWorker -Worker $winner -TimeoutMilliseconds 120000 }
  Close-L07cBarrier -Barrier $winnerBarrier

  $winnerRecords = @(Read-L07cWorkerRecords -LogPath (Join-Path $caseRoot 'winner.jsonl'))
  $winnerRecord = $(if ($winnerRecords.Count -gt 0) { $winnerRecords[0] } else { $null })
  $winnerGeneration = $(if ($winnerRecord) { [string]$winnerRecord.generation } else { '' })

  # 失败竞争者的失败形态与写入资格
  if (-not $busy -or -not $busy.record) {
    $winnerProblems += '竞争者没有记录（未能观察到明确 Busy）'
  } else {
    if ($busy.record.ok) { $winnerProblems += '赢家持锁期间竞争者仍然成功（互斥失效）' }
    elseif ([string]$busy.record.error_text -notmatch 'EXPORT-SLOT-BUSY') {
      $winnerProblems += ('竞争者的失败类别不是 EXPORT-SLOT-BUSY：{0}' -f $busy.record.error_message)
    }
  }
  foreach ($completion in @($busyCompletion)) {
    if ($completion.exit_code -ne 0) { $winnerProblems += ('竞争者进程退出码 {0}（应为 0：Busy 是受控失败）' -f $completion.exit_code) }
  }
  if (-not $winnerRecord -or -not $winnerRecord.ok) { $winnerProblems += '赢家在释放发布权后没有成功完成' }
  if (-not $seedGeneration) { $winnerProblems += '前置代次没有建立（槽位没有当前有效代次指针）' }
  if ($winnerGeneration -and $winnerGeneration -eq $seedGeneration) { $winnerProblems += '新一代次与上一代次相同（generation 必须每次发布都不同）' }

  # 事后注入：模拟"失败方把自己的收据写进了共享有效槽位"（同 generation 不同内容）。
  $forgedRejected = $false
  $forgedCategory = ''
  $archiveShaAfterForge = ''
  if ($winnerGeneration) {
    $liveReceipt = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
    # 只改一个"非身份类"字段：generation 保持正确，但收据内容已变。
    $liveReceipt.dependency_identity.producer_selection_evidence = 'forged-by-a-failed-competitor'
    Write-FixtureText -Path $bundle.fixture.receiptPath -Text ($liveReceipt | ConvertTo-Json -Depth 20)
    $consume = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
      -Mode 'consume' -MessageFile $bundle.messageB -LogName 'consume-forged.jsonl' -HostSuffix ($Token + '-forged')
    if ($consume.record -and -not $consume.record.ok) {
      $forgedRejected = $true
      if ([string]$consume.record.error_text -match '\[([A-Z][A-Z0-9\-]+)\]') { $forgedCategory = $Matches[1] }
    }
    $archiveRecordPath = Get-L07dGenerationRecordPath -Bundle $bundle -Generation $winnerGeneration
    $archiveShaAfterForge = Get-L07cSha $archiveRecordPath
    if ($forgedCategory -ne 'GENERATION-CONFLICT') {
      $winnerProblems += ("同 generation 不同收据内容的注入没有被判为冲突（类别={0}）" -f $(if ($forgedCategory) { $forgedCategory } else { '(接受或未分类)' }))
    }
    if (-not $forgedRejected) { $winnerProblems += '失败方的伪造收据被消费端接受了' }
    # 代次档案仍应是"赢家那一代"的原内容（伪造没有写进档案）。
    $winnerRecordSha = $(if ($winnerRecord) { [string]$winnerRecord.receipt_digest } else { '' })
    $archiveReceipt = $(if (Test-Path -LiteralPath $archiveRecordPath) { Get-Content -Raw -LiteralPath $archiveRecordPath -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
    if ($archiveReceipt) {
      $recomputed = Get-LoaderReceiptDigest -Receipt $archiveReceipt
      if ($winnerRecordSha -and $recomputed -ne $winnerRecordSha) {
        $winnerProblems += '赢家代次的档案收据内容被改写（代次档案必须不可变）'
      }
    }
  }

  $detail = ("loser=EXPORT-SLOT-BUSY no_write=true forged_receipt_rejected={0} archive_immutable=true lock_existed={1}" -f $forgedCategory, $lockExistedDuringWindow)
  if ($winnerProblems.Count -gt 0) { $detail = ($winnerProblems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($winnerProblems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- ② -----
# 进程在"声明收据已提交、当前有效代次指针未发布"之间**硬退出**：
#   * 指针必须仍指向上一代次（失败运行没有发布自己的当前有效代次）；
#   * 消费者必须拒绝（收据与指针不一致 ⇒ 不完整代次）；
#   * 后续发布者必须能回收（按 pid 存活判定）并发布出一致的下一代。
function Invoke-L07dExitBetweenReceiptAndPointerCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07d-exit-between-receipt-and-pointer'
  $caseRoot = Join-Path $RoundRoot 'exit-between-receipt-and-pointer'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'exit-between-receipt-and-pointer' -Root $caseRoot
  $slot = Get-L07dSlot -Bundle $bundle

  $seed = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageA -LogName 'seed.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-seed')
  $seedPointer = $(if (Test-Path -LiteralPath $slot.pointer_path) { Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
  $seedGeneration = $(if ($seedPointer) { [string]$seedPointer.generation } else { '' })
  $seedPointerSha = Get-L07cSha $slot.pointer_path
  $seedDllSha = Get-L07cSha $bundle.fixture.stablePath

  # 硬退出（exit=97）：不展开 finally、不释放锁文件、不写指针。
  $exitBarrier = New-L07cBarrier -Workers 1 -Token ($Token + '-exit-receipt')
  $exitArguments = New-L07cExportWorkerArguments -Bundle $bundle -Mode 'export' -MessageFile $bundle.messageB `
    -LogPath (Join-Path $caseRoot 'exit.jsonl') -ExpectedSha256 $bundle.producerShaB
  $exitArguments['RunId'] = ($Token + '-exit-run')
  $exitCompletion = $null
  try {
    $exitWorker = New-L07cCaseFiles -CaseRoot $caseRoot -WorkerScript $WorkerScript -Barrier $exitBarrier -WorkerIndex 0 `
      -SlotWaitSeconds 60 -SeamPoint 'between-generation-and-pointer' -SeamAction 'exit' -ModuleArguments $exitArguments
    $exitCompletion = Complete-L07cWorker -Worker $exitWorker -TimeoutMilliseconds 120000
  } finally {
    Close-L07cBarrier -Barrier $exitBarrier
  }

  $problems = @()
  if ($seedGeneration -eq '') { $problems += '前置代次没有建立' }
  if ($exitCompletion.exit_code -ne 97) { $problems += ('被杀进程的退出码不是 97：{0}' -f $exitCompletion.exit_code) }
  # 失败运行不得发布自己的"当前有效代次"
  if ((Get-L07cSha $slot.pointer_path) -ne $seedPointerSha) {
    $problems += '进程在收据与指针之间退出后，槽位指针被改动了（失败运行不得发布当前有效代次）'
  }
  # 盘上：产物是 B 的字节、声明收据是新一代、指针仍是上一代 ⇒ 这就是"不完整代次"
  if ((Get-L07cSha $bundle.fixture.stablePath) -ne $bundle.producerShaB) { $problems += '夹具前提不成立：产物不是被杀进程那一代的字节' }
  $declaredReceipt = $(if (Test-Path -LiteralPath $bundle.fixture.receiptPath) { Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
  if (-not $declaredReceipt -or [string]$declaredReceipt.publication.generation -eq $seedGeneration) {
    $problems += '夹具前提不成立：声明收据没有进入新一代'
  }

  # 消费者必须拒绝，且失败类别必须指明"收据与指针不一致"这一类不完整代次。
  $consume = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'consume' -MessageFile $bundle.messageB -LogName 'consume.jsonl' -HostSuffix ($Token + '-consume')
  $consumeCategory = ''
  if (-not $consume.record -or $consume.record.ok) {
    $problems += '消费者接受了"收据已提交、指针未发布"的不完整代次'
  } else {
    if ([string]$consume.record.error_text -match '\[([A-Z][A-Z0-9\-]+)\]') { $consumeCategory = $Matches[1] }
    if ($consumeCategory -ne 'RECEIPT-GENERATION-MISMATCH') {
      $problems += ("不完整代次的拒绝类别不是 RECEIPT-GENERATION-MISMATCH：{0}" -f $consumeCategory)
    }
  }

  # 后续发布者必须回收被杀进程留下的锁，并发布出一致的下一代。
  $recovery = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageB -LogName 'recovery.jsonl' -ExpectedSha $bundle.producerShaB -HostSuffix ($Token + '-recovery')
  if (-not $recovery.record -or -not $recovery.record.ok) {
    $problems += ('被杀进程留下的锁没有被回收（后续发布者无法发布）：{0}' -f $(if ($recovery.record) { $recovery.record.error_message } else { '没有记录' }))
  } else {
    $finalPointer = Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson
    if ([string]$finalPointer.generation -ne [string]$recovery.record.generation) {
      $problems += '回收后的指针没有指向新发布的有效代次'
    }
    if ([string]$finalPointer.artifact_digest -ne $bundle.producerShaB) {
      $problems += '回收后的指针登记的产物身份与实际产物不符'
    }
    $consumeAfter = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
      -Mode 'consume' -MessageFile $bundle.messageB -LogName 'consume-after.jsonl' -HostSuffix ($Token + '-consume-after')
    if (-not $consumeAfter.record -or -not $consumeAfter.record.ok) {
      $problems += '回收后的代次仍然无法被消费'
    }
  }

  $detail = ("exit=97 pointer_unchanged=true consumer={0} lock_reclaimed=true final_generation_consistent=true" -f $consumeCategory)
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- ③ -----
# 消费 G1 之后槽位合法更新到 G2：G1 的报告依据（固定消费收据 + 本次独立 staging）仍然有效，
# 且**不依赖指针停在 G1**（断言指针确实已指向 G2）。
function Invoke-L07dConsumeG1ThenPublishG2Case {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07d-consume-g1-then-publish-g2'
  $caseRoot = Join-Path $RoundRoot 'consume-g1-then-g2'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'consume-g1-then-g2' -Root $caseRoot
  $slot = Get-L07dSlot -Bundle $bundle
  $stagingRoot = Join-Path $caseRoot 'consume-staging'

  $problems = @()
  # G1：导出 A（同一发布权内完成消费，固定消费收据）。
  $runIdG1 = $Token + '-run-g1'
  $g1 = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageA -LogName 'g1.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-g1') `
    -Extra @{ RunId = $runIdG1; ConsumerRunId = $runIdG1; ConsumeStagingDirectory = (Join-Path $stagingRoot $runIdG1) }
  if (-not $g1.record -or -not $g1.record.ok) {
    $problems += ('G1 导出失败：{0}' -f $(if ($g1.record) { $g1.record.error_message } else { '没有记录' }))
  }
  $g1Generation = $(if ($g1.record) { [string]$g1.record.generation } else { '' })
  $g1RecordPath = $(if ($g1Generation) { Get-L07dGenerationRecordPath -Bundle $bundle -Generation $g1Generation } else { '' })
  $g1RecordSha = $(if ($g1RecordPath -and (Test-Path -LiteralPath $g1RecordPath)) { Get-L07cSha $g1RecordPath } else { '' })
  $g1Staged = $(if ($g1.record) { [string]$g1.record.consumption_consumed_staging_absolute } else { '' })
  $g1PointerSha = Get-L07cSha $slot.pointer_path

  # G2：导出 B（槽位合法更新到新一代次）。
  $g2 = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageB -LogName 'g2.jsonl' -ExpectedSha $bundle.producerShaB -HostSuffix ($Token + '-g2')
  $finalPointer = $(if (Test-Path -LiteralPath $slot.pointer_path) { Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
  $finalGeneration = $(if ($finalPointer) { [string]$finalPointer.generation } else { '' })

  if (-not $g2.record -or -not $g2.record.ok) {
    $problems += 'G2 导出失败（无法构造"槽位合法更新到新一代次"的场景）'
  }
  if ($finalGeneration -eq $g1Generation -or $finalGeneration -eq '') {
    $problems += '槽位没有合法更新到新的 generation（前提不成立）'
  }
  if ((Get-L07cSha $slot.pointer_path) -eq $g1PointerSha) { $problems += '指针字节没有变化（前提不成立）' }
  if ([string]$finalPointer.supersedes_generation -ne $g1Generation) {
    $problems += '新指针没有登记它取代的上一代（supersedes_generation 缺失或不符）'
  }

  # G1 的报告依据必须仍然有效：从**保存下来的 G1 收据 + 本次独立 staging + 不可变代次档案**重建。
  if ($g1RecordPath -and (Test-Path -LiteralPath $g1RecordPath)) {
    if ((Get-L07cSha $g1RecordPath) -ne $g1RecordSha) { $problems += 'G1 的代次档案在槽位更新到 G2 之后被改写了' }
    $g1Archive = Get-Content -Raw -LiteralPath $g1RecordPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $g1ReceiptDigest = Get-LoaderReceiptDigest -Receipt $g1Archive
    if ([string]$g1.record.consumption_receipt_digest -ne $g1ReceiptDigest) {
      $problems += 'G1 消费收据的 receipt_digest 无法从 G1 代次档案重算（报告字段与实际收据不一致）'
    }
    if ([string]$g1Archive.publication.generation -ne $g1Generation) { $problems += 'G1 代次档案的 generation 与消费收据不一致' }
    if ([string]$g1Archive.publication.artifact_digest -ne $bundle.producerShaA) {
      $problems += 'G1 代次档案登记的产物身份不是 A 的字节'
    }
  } else {
    $problems += 'G1 的代次收据没有保留下来（代次收据必须可追溯保留）'
  }
  if (-not $g1Staged -or -not (Test-Path -LiteralPath $g1Staged)) {
    $problems += 'G1 的本次独立 staging 不存在（报告将无法独立核对它消费的字节）'
  } elseif ((Get-L07cSha $g1Staged) -ne $bundle.producerShaA) {
    $problems += 'G1 的 staging 字节不是 A（消费关联与实际消费内容不一致）'
  } elseif ((Get-L07cSha $g1Staged) -ne [string]$g1.record.consumption_artifact_digest) {
    $problems += 'G1 消费收据的 artifact_digest 与 staging 实际字节不一致'
  }
  if ([string]$g1.record.consumption_generation -ne $g1Generation) { $problems += 'G1 消费收据的 generation 不是 G1' }
  if ([string]$g1.record.consumption_pointer_generation_at_consumption -ne $g1Generation) { $problems += 'G1 消费时指针的 generation 与消费收据不一致' }
  if ([string]$g1.record.consumption_artifact_digest -ne $bundle.producerShaA) { $problems += 'G1 消费收据的 artifact_digest 不是 A 的字节' }

  # 当前消费视图确实已前进到 G2（证明 G1 报告的有效性不是因为指针停着不动）。
  $nowConsume = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'consume' -MessageFile $bundle.messageB -LogName 'consume-now.jsonl' -HostSuffix ($Token + '-consume-now')
  if (-not $nowConsume.record -or -not $nowConsume.record.ok) {
    $problems += '更新到 G2 之后消费者无法消费当前有效代次'
  } elseif ([string]$nowConsume.record.generation -ne $finalGeneration) {
    $problems += ("当前消费视图没有指向 G2：{0} != {1}" -f $nowConsume.record.generation, $finalGeneration)
  }

  $detail = ("g1={0} g2={1} g1_archive_retained=true g1_report_fields_recomputable=true current_view=g2" -f $g1Generation, $finalGeneration)
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- ④ -----
# generation 正确但内容错配仍拒绝：
#   (a) 代次档案被改写（generation 不变，artifact_digest 换成别的字节）⇒ 与指针登记的
#       receipt_digest 不一致 ⇒ 判为冲突/内容错配；
#   (b) 收据与指针、档案三方自洽（generation 正确），但盘上产物被替换 ⇒ CONTENT-MISMATCH。
function Invoke-L07dGenerationCorrectContentMismatchCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07d-generation-correct-content-mismatch'
  $caseRoot = Join-Path $RoundRoot 'generation-content-mismatch'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'generation-content-mismatch' -Root $caseRoot
  $slot = Get-L07dSlot -Bundle $bundle

  $seed = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageA -LogName 'seed.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-seed')
  $problems = @()
  $generation = $(if ($seed.record) { [string]$seed.record.generation } else { '' })
  if (-not $generation -or -not $seed.record.ok) { $problems += '前置代次没有建立' }
  $recordPath = $(if ($generation) { Get-L07dGenerationRecordPath -Bundle $bundle -Generation $generation } else { '' })
  $recordShaBefore = $(if ($recordPath) { Get-L07cSha $recordPath } else { '' })
  $categoryA = ''
  if ($recordPath -and (Test-Path -LiteralPath $recordPath)) {
    # (a) 改写代次档案：generation 保持不变，产物摘要换成 B 的字节。
    $archive = Get-Content -Raw -LiteralPath $recordPath -Encoding UTF8 | ConvertFrom-LoaderJson
    $archive.publication.artifact_digest = $bundle.producerShaB
    Write-FixtureText -Path $recordPath -Text ($archive | ConvertTo-Json -Depth 20)
    $consumeA = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
      -Mode 'consume' -MessageFile $bundle.messageA -LogName 'consume-archive-forged.jsonl' -HostSuffix ($Token + '-a')
    if (-not $consumeA.record -or $consumeA.record.ok) {
      $problems += '代次档案被改写（generation 正确、内容错配）时消费者仍然接受'
    } elseif ([string]$consumeA.record.error_text -match '\[([A-Z][A-Z0-9\-]+)\]') {
      $categoryA = $Matches[1]
      if ($categoryA -notin @('GENERATION-CONFLICT', 'CONTENT-MISMATCH')) {
        $problems += ("内容错配的拒绝类别不可辨认：{0}" -f $categoryA)
      }
    } else {
      $problems += '内容错配的拒绝信息里没有类别标记'
    }
    if ((Get-L07cSha $recordPath) -eq $recordShaBefore) { $problems += '夹具前提不成立：代次档案没有被改写' }
    # 还原档案，保证后续 (b) 的前提是"三方自洽、generation 正确"
    $archive.publication.artifact_digest = $bundle.producerShaA
    Write-FixtureText -Path $recordPath -Text ($archive | ConvertTo-Json -Depth 20)
    if ((Get-L07cSha $recordPath) -ne $recordShaBefore) { $problems += '夹具还原代次档案失败' }
  } else {
    $problems += '代次档案不存在（代次收据必须可追溯保留）'
  }

  # (b) 三方（指针/档案/声明收据）自洽、generation 正确，但产物被替换。
  Write-FixtureBytes -Path $bundle.fixture.stablePath -Bytes (New-SyntheticPe -TotalLength 65536 -Fill 0x6E)
  $tamperedSha = Get-L07cSha $bundle.fixture.stablePath
  $consumeB = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'consume' -MessageFile $bundle.messageA -LogName 'consume-artifact-forged.jsonl' -HostSuffix ($Token + '-b')
  $categoryB = ''
  if (-not $consumeB.record -or $consumeB.record.ok) {
    $problems += 'generation 正确但产物内容错配时消费者仍然接受'
  } else {
    if ([string]$consumeB.record.error_text -match '\[([A-Z][A-Z0-9\-]+)\]') { $categoryB = $Matches[1] }
    if ($categoryB -ne 'CONTENT-MISMATCH') { $problems += ("产物错配的拒绝类别不是 CONTENT-MISMATCH：{0}" -f $categoryB) }
    if ([string]$consumeB.record.error_text -notmatch 'incomplete_generation_or_tampered=true') {
      $problems += '不完整代次/被替换的标记缺失'
    }
  }
  if ($tamperedSha -eq $bundle.producerShaA) { $problems += '夹具没有真正替换产物内容' }

  $detail = ("archive_forgery={0} artifact_mismatch={1} generation={2}" -f $categoryA, $categoryB, $generation)
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- ⑤ -----
# 同一 generation 对应不同收据内容 ⇒ 判冲突（且不得覆盖代次档案）。
# 这里改的是**非身份类**字段，因此 generation、artifact_digest、身份哈希全部"正确"：
# 唯一不成立的正是"同一代次只能有一份收据内容"。
function Invoke-L07dSameGenerationDifferentReceiptCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07d-same-generation-different-receipt-content'
  $caseRoot = Join-Path $RoundRoot 'same-generation-different-receipt'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'same-generation-different-receipt' -Root $caseRoot
  $slot = Get-L07dSlot -Bundle $bundle

  $seed = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageA -LogName 'seed.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-seed')
  $problems = @()
  $generation = $(if ($seed.record) { [string]$seed.record.generation } else { '' })
  $originalReceiptDigest = $(if ($seed.record) { [string]$seed.record.receipt_digest } else { '' })
  if (-not $generation -or -not $seed.record.ok) { $problems += '前置代次没有建立' }

  # 往共享有效槽位写一份"同 generation、不同内容"的收据（模拟失败竞争者的收据）。
  $declared = Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
  $declared.dependency_identity.producer_selection_evidence = 'same-generation-different-receipt-body'
  Write-FixtureText -Path $bundle.fixture.receiptPath -Text ($declared | ConvertTo-Json -Depth 20)
  $forgedReceiptDigest = Get-LoaderReceiptDigest -Receipt (Get-Content -Raw -LiteralPath $bundle.fixture.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson)

  $category = ''
  $consume = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'consume' -MessageFile $bundle.messageA -LogName 'consume.jsonl' -HostSuffix ($Token + '-consume')
  if (-not $consume.record -or $consume.record.ok) {
    $problems += '同 generation 不同收据内容被接受了（必须判冲突）'
  } else {
    if ([string]$consume.record.error_text -match '\[([A-Z][A-Z0-9\-]+)\]') { $category = $Matches[1] }
    if ($category -ne 'GENERATION-CONFLICT') { $problems += ("拒绝类别不是 GENERATION-CONFLICT：{0}" -f $category) }
    if ([string]$consume.record.error_text -notmatch [regex]::Escape($generation)) { $problems += '拒绝信息没有指出涉及的 generation' }
  }
  if ($forgedReceiptDigest -eq $originalReceiptDigest) { $problems += '夹具前提不成立：伪造收据的内容摘要与原收据相同' }
  # 指针与代次档案必须保持原样（伪造者不得改写不可变记录）。
  $pointer = Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson
  if ([string]$pointer.receipt_digest -ne $originalReceiptDigest) { $problems += '槽位指针登记的 receipt_digest 被改写了' }
  $recordPath = Get-L07dGenerationRecordPath -Bundle $bundle -Generation $generation
  $archive = Get-Content -Raw -LiteralPath $recordPath -Encoding UTF8 | ConvertFrom-LoaderJson
  if ((Get-LoaderReceiptDigest -Receipt $archive) -ne $originalReceiptDigest) { $problems += '代次档案被改写（必须不可变）' }

  $detail = ("generation={0} forged_receipt_digest={1} conflict=GENERATION-CONFLICT archive_unchanged=true" -f $generation, $forgedReceiptDigest.Substring(0, 12))
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- ⑦ -----
# 不只断言字段存在：**核对每个代次字段与实际消费内容一致**（可由第三方重算）。
function Invoke-L07dConsumptionFieldsMatchContentCase {
  param([string]$RoundRoot, [string]$WorkerScript, [int]$Round, [string]$Token)

  $caseId = 'L07d-consumption-fields-match-consumed-content'
  $caseRoot = Join-Path $RoundRoot 'consumption-fields'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $bundle = Get-L07cExportFixture -Name 'consumption-fields' -Root $caseRoot
  $slot = Get-L07dSlot -Bundle $bundle
  $runId = $Token + '-run'
  $stagingDirectory = Join-Path $caseRoot ('staging/' + $runId)

  $problems = @()
  # 前置：先有一代完整代次（否则消费阶段没有可消费的当前有效代次）。
  $seed = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'export' -MessageFile $bundle.messageA -LogName 'seed.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-seed')
  if (-not $seed.record -or -not $seed.record.ok) { $problems += '前置代次没有建立' }
  # 走"消费端固定流程"（独立进程），而不是导出返回值的副本。
  $consume = Invoke-L07dWorkerOnce -CaseRoot $caseRoot -WorkerScript $WorkerScript -Bundle $bundle `
    -Mode 'consume-fixed' -MessageFile $bundle.messageA -LogName 'consume-fixed.jsonl' -ExpectedSha $bundle.producerShaA -HostSuffix ($Token + '-cf') `
    -Extra @{ RunId = $runId; ConsumerRunId = $runId; ConsumeStagingDirectory = $stagingDirectory }
  if (-not $consume.record -or -not $consume.record.ok) {
    $problems += ('固定消费流程失败：{0}' -f $(if ($consume.record) { $consume.record.error_message } else { '没有记录' }))
  } else {
    $record = $consume.record
    $pointer = Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson
    $generation = [string]$record.generation
    $recordPath = $(if ($generation) { Get-L07dGenerationRecordPath -Bundle $bundle -Generation $generation } else { '' })
    $archive = $(if ($recordPath -and (Test-Path -LiteralPath $recordPath)) { Get-Content -Raw -LiteralPath $recordPath -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
    $stagedPath = [string]$record.consumed_staging_absolute

    if (-not $generation) { $problems += '消费收据没有明确 generation' }
    if ([string]$pointer.generation -ne $generation) { $problems += '消费收据的 generation 与槽位指针不一致' }
    if ([string]$record.pointer_generation_at_consumption -ne $generation) { $problems += '消费时指针 generation 与消费收据不一致' }
    if ([string]$record.slot_key -ne $slot.key) { $problems += '消费收据的 slot_key 与按 manifest 重算的槽位不一致' }
    if ([string]$record.consumer_run_id -ne $runId) { $problems += '消费收据的 consumer_run_id 不是本次消费运行' }
    if ([long]$record.artifact_length -ne (Get-Item -LiteralPath $bundle.fixture.stablePath).Length) {
      $problems += '消费收据的 artifact_length 与实际产物长度不一致'
    }
    if ([string]$record.artifact_digest -ne $bundle.producerShaA) { $problems += '消费收据的 artifact_digest 不是产物实际字节' }
    if ([string]$record.consumed_staging_sha256 -ne [string]$record.artifact_digest) {
      $problems += '消费 staging 的内容身份与 artifact_digest 不一致'
    }
    if (-not (Test-Path -LiteralPath $stagedPath)) {
      $problems += '消费 staging 文件不存在'
    } elseif ((Get-L07cSha $stagedPath) -ne [string]$record.artifact_digest) {
      $problems += '消费 staging 的实际字节与消费收据登记的 artifact_digest 不一致'
    }
    $stagingFull = [System.IO.Path]::GetFullPath($stagingDirectory).TrimEnd('\', '/')
    if ($stagingFull.StartsWith($slot.slot_dir.TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)) {
      $problems += '消费 staging 落在共享槽位内（必须是本次打包的独立 staging）'
    }
    if ($archive) {
      if ((Get-LoaderReceiptDigest -Receipt $archive) -ne [string]$record.receipt_digest) {
        $problems += '消费收据的 receipt_digest 无法从代次档案重算'
      }
      if ([string]$archive.publication.generation -ne $generation) { $problems += '代次档案的 generation 与消费收据不一致' }
      if ([string]$archive.publication.artifact_digest -ne [string]$record.artifact_digest) {
        $problems += '代次档案的 artifact_digest 与消费收据不一致'
      }
      if ([string]$record.producer_run_id -ne [string]$archive.publication.producer_run_id) {
        $problems += '消费收据的 producer_run_id 与产出方登记的运行 ID 不一致'
      }
      # build_input_digest 可由收据自身的构建输入身份字段重算
      $recomputedBuildInput = Get-LoaderBuildInputDigest `
        -ArtifactContractSha256 ([string]$archive.artifact_contract_sha256) `
        -SourceIdentitySha256 ([string]$archive.build_identity.source_identity_sha256) `
        -SourceIdentityFileCount ([int]$archive.build_identity.source_identity_file_count) `
        -LockfileSha256 ([string]$archive.build_identity.lockfile_sha256) `
        -BuildEntryManifestSha256 ([string]$archive.build_identity.build_entry_manifest_sha256) `
        -BuildTarget ([string]$archive.build_identity.build_target) `
        -CargoTargetDir ([string]$archive.build_identity.cargo_target_dir) `
        -Profile ([string]$archive.build_identity.profile) `
        -ReleaseVersion ([string]$archive.build_identity.release_version) `
        -ProducerPackage ([string]$archive.dependency_identity.producer_package)
      if ([string]$record.build_input_digest -ne $recomputedBuildInput) {
        $problems += '消费收据的 build_input_digest 无法从代次档案的构建输入身份重算'
      }
      if ([string]$archive.build_identity.build_input_digest -ne [string]$record.build_input_digest) {
        $problems += '收据内嵌的 build_input_digest 与消费关联不一致'
      }
    } else {
      $problems += '代次档案不存在（无法核对消费收据）'
    }
    if ([bool]$record.consumption_verified -ne $true) { $problems += 'consumption_verified 不为 true' }
    if ([bool]$record.consumed_content_matches_generation -ne $true) { $problems += 'consumed_content_matches_generation 不为 true' }
  }
  # 槽位里不得留下本次消费的残片（消费只在独立 staging 写文件）。
  $leftovers = @(Get-L07cSlotFiles -Directory $slot.slot_dir)
  if ($leftovers.Count -gt 0) { $problems += ('槽位留下残片：' + (($leftovers | ForEach-Object { $_.Name }) -join ', ')) }

  $detail = 'fields_recomputed=true artifact_digest==staging_sha256 receipt_digest==archive_digest build_input_digest==recomputed consumer_run_id=run slot_key==manifest-slot'
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

# ---------------------------------------------------------------- ⑥/⑦ -----
# 端到端：真实 package-all 打包（构建模式）里 exported_artifacts[] 的代次字段必须与
# **包内实际字节**一致，且消费关联指向本次运行（报告只从固定消费收据生成）。
function Invoke-L07dPackageReportGenerationBindingCase {
  param([string]$RoundRoot, [int]$Round)

  $caseId = 'L07d-package-report-generation-binding'
  $caseRoot = Join-Path $RoundRoot 'package-report-binding'
  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $fixture = New-LoaderFixture -Name 'package-report-binding' -Root $caseRoot -Producers @(
    [pscustomobject]@{ hash = 'c1c1c1c1c1c1c1c1'; bytes = (New-SyntheticPe -TotalLength 49152 -Fill 0x41) }
  ) -MessageProducerIndexes @(0)

  $problems = @()
  $result = Invoke-PackageFixture -Fixture $fixture
  if ($result.Error) {
    $problems += ('package-all 构建模式运行失败：{0}' -f (($result.Error -split "`n")[0]))
  } elseif (-not $result.ReportExists) {
    $problems += 'package-all 没有写出报告'
  } else {
    $entries = @($result.Report.exported_artifacts)
    if ($entries.Count -lt 1) {
      $problems += '报告没有 exported_artifacts 条目'
    } else {
      $entry = $entries[0]
      $stagedLoader = Get-StagedLoaderPath -Fixture $fixture
      $stagedSha = Get-L07cSha $stagedLoader
      $slot = Get-LoaderPublishSlot -Destination $fixture.stablePath -RepoPath $workspaceFull -Target 'bin/WebView2Loader.dll' -Profile 'debug'
      $pointer = $(if (Test-Path -LiteralPath $slot.pointer_path) { Get-Content -Raw -LiteralPath $slot.pointer_path -Encoding UTF8 | ConvertFrom-LoaderJson } else { $null })
      # 字段 → 实际消费内容的一致性（逐条重算）
      if ([string]$entry.artifact_digest -ne $stagedSha) {
        $problems += ('报告 artifact_digest 与包内实际字节不一致：{0} != {1}' -f $entry.artifact_digest, $stagedSha)
      }
      if ([string]$entry.consumption.staged_sha256 -ne [string]$entry.artifact_digest) {
        $problems += '消费关联的 staged_sha256 与 artifact_digest 不一致'
      }
      if (-not $pointer) { $problems += '槽位指针不存在（无法核对报告引用的代次）' }
      if ([string]$entry.generation -ne [string]$pointer.generation) {
        $problems += ('报告 generation 与槽位当前有效代次不一致：{0} != {1}' -f $entry.generation, $pointer.generation)
      }
      if ([string]$entry.slot_key -ne $slot.key) { $problems += '报告 slot_key 与按 manifest 重算的槽位不一致' }
      if ([string]$entry.consumer_run_id -ne [string]$result.Report.run_id) {
        $problems += '报告 consumer_run_id 不是本次运行'
      }
      if ([string]$entry.producer_run_id -ne [string]$result.Report.run_id) {
        $problems += '报告 producer_run_id 不是本次运行（包内产物由本次运行产出）'
      }
      $entryReceipt = $(if ($entry.receipt) { Join-Path $workspaceFull ([string]$entry.receipt).Replace('/', '\') } else { '' })
      $archivePath = $(if ($entry.generation) { Resolve-LoaderGenerationRecordPath -Slot $slot -Generation ([string]$entry.generation) } else { '' })
      if ($archivePath -and (Test-Path -LiteralPath $archivePath)) {
        $archive = Get-Content -Raw -LiteralPath $archivePath -Encoding UTF8 | ConvertFrom-LoaderJson
        $archiveDigest = Get-LoaderReceiptDigest -Receipt $archive
        if ([string]$entry.receipt_digest -ne $archiveDigest) {
          $problems += '报告 receipt_digest 无法从代次档案重算'
        }
        if ([string]$entry.build_input_digest -ne [string]$archive.publication.build_input_digest) {
          $problems += '报告 build_input_digest 与代次收据不一致'
        }
        if ([string]$pointer.receipt_digest -ne $archiveDigest) {
          $problems += '槽位指针的 receipt_digest 与代次档案不一致'
        }
      } else {
        $problems += '代次档案不存在（报告引用的代次收据必须可追溯保留）'
      }
      # 独立 staging 必须存在且等于包内字节；且 staging 不在共享槽位内。
      $stagedConsumePath = [string]$entry.consumption.staged_path
      if ($stagedConsumePath) {
        $stagedConsumeFull = Join-Path $workspaceFull $stagedConsumePath.Replace('/', '\')
        if (-not (Test-Path -LiteralPath $stagedConsumeFull)) {
          $problems += '报告登记的消费 staging 不存在'
        } elseif ((Get-L07cSha $stagedConsumeFull) -ne $stagedSha) {
          $problems += '消费 staging 字节与包内字节不一致'
        }
      } else {
        $problems += '报告没有登记消费 staging 路径'
      }
      foreach ($field in @('slot_key', 'generation', 'producer_run_id', 'consumer_run_id', 'artifact_digest', 'receipt_digest', 'build_input_digest')) {
        if (-not $entry.PSObject.Properties[$field] -or -not [string]$entry.$field) {
          $problems += ('导出物登记缺少字段 {0}' -f $field)
        }
      }
    }
  }

  $detail = 'artifact_digest==package_bytes generation==pointer generation_digests_recomputable consumer_run_id==run_id'
  if ($problems.Count -gt 0) { $detail = ($problems -join '; ') }
  Add-L07cRoundResult -CaseId $caseId -Round $Round -Passed ($problems.Count -eq 0) -Detail $detail
}

function Invoke-L07cConcurrencySuite {
  param([int]$Rounds = 1, [int]$Iterations = 6)

  $runId = (Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
  $runRoot = Join-Path $concurrencyRoot $runId
  New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
  $workerScript = Join-Path $runRoot 'l07c-worker.ps1'
  Write-FixtureText -Path $workerScript -Text $L07cWorkerScriptText

  $script:l07cResults = @{}
  $script:l07cRoundLog = New-Object System.Collections.Generic.List[string]
  Write-Host ''
  Write-Host ("L07c 并发契约：run={0} rounds={1} iterations={2}（全部真实进程 + 命名事件屏障）" -f $runRoot, $Rounds, $Iterations)

  for ($round = 1; $round -le $Rounds; $round++) {
    $roundRoot = Join-Path $runRoot ("round-{0:D3}" -f $round)
    New-Item -ItemType Directory -Force -Path $roundRoot | Out-Null
    Write-Host ("--- L07c round {0}/{1} ---" -f $round, $Rounds)

    Invoke-L07cCopyRaceCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Iterations $Iterations -SlotWaitSeconds 0 -Token ("$runId-r$round-busy")
    Invoke-L07cCopyRaceCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Iterations $Iterations -SlotWaitSeconds 60 -Token ("$runId-r$round-serial")
    Invoke-L07cExporterRaceCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -SlotWaitSeconds 60 -Token ("$runId-r$round-exporter")
    Invoke-L07cIncompleteGenerationCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-incomplete")
    Invoke-L07cTamperedArtifactCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-tamper")
    Invoke-L07cLoserIsolationCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-loser")
    Invoke-L07cDistinctSlotsCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-slots")
    Invoke-L07cSourceInputChangedCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-src")

    # PKG-L07c P0 补强（裁决 §3.2/§3.3 七项验收）：固定代次语义 + 完整发布临界区。
    Invoke-L07dLoserNoWriteAccessCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-l07d-loser")
    Invoke-L07dExitBetweenReceiptAndPointerCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-l07d-exit")
    Invoke-L07dConsumeG1ThenPublishG2Case -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-l07d-g1g2")
    Invoke-L07dGenerationCorrectContentMismatchCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-l07d-mismatch")
    Invoke-L07dSameGenerationDifferentReceiptCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-l07d-samegen")
    Invoke-L07dConsumptionFieldsMatchContentCase -RoundRoot $roundRoot -WorkerScript $workerScript -Round $round -Token ("$runId-r$round-l07d-fields")
    Invoke-L07dPackageReportGenerationBindingCase -RoundRoot $roundRoot -Round $round
  }

  [System.IO.File]::WriteAllLines((Join-Path $runRoot 'round-results.jsonl'), $script:l07cRoundLog, [System.Text.UTF8Encoding]::new($false))

  $caseSummaries = @()
  $aggregateFailed = 0
  foreach ($caseId in ($script:l07cResults.Keys | Sort-Object)) {
    # 注意：Windows PowerShell 5.1 对 @(<List[object]>) 会抛 "Argument types do not match"，
    # 所以这里不套数组子表达式，直接用 List 自身的 .Count / 管道过滤。
    $rows = $script:l07cResults[$caseId]
    $failed = @($rows | Where-Object { -not $_.passed })
    if ($failed.Count -eq 0) {
      Add-CaseResult -Id $caseId -Status 'PASS' -Detail ("{0} 轮全部通过（每轮两个真实进程 + 屏障触发）" -f $rows.Count)
    } else {
      $aggregateFailed++
      Add-CaseResult -Id $caseId -Status 'FAIL' -Detail ("{0}/{1} 轮失败（失败轮：{2}）；首例：{3}" -f $failed.Count, $rows.Count, (($failed | ForEach-Object { $_.round }) -join ','), $failed[0].detail)
    }
    $caseSummaries += [pscustomobject]@{
      case = $caseId
      rounds = $rows.Count
      passed = ($rows.Count - $failed.Count)
      failed = $failed.Count
      failures = @($failed | ForEach-Object { [pscustomobject]@{ round = $_.round; detail = $_.detail } })
    }
  }
  $summary = [ordered]@{
    run_root = $runRoot
    rounds = $Rounds
    iterations_per_process = $Iterations
    trigger = 'named-event-barrier-real-processes'
    cases = $caseSummaries
  }
  [System.IO.File]::WriteAllText((Join-Path $runRoot 'summary.json'), ($summary | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
  Write-Host ("L07c 并发契约结果汇总：{0}（cases={1} failed_cases={2}）" -f (Join-Path $runRoot 'summary.json'), $caseSummaries.Count, $aggregateFailed)
  return $runRoot
}

try {
  if (Test-Path -LiteralPath $fixtureRoot) {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null
  Remove-StaleFixtureRuns -BaseRoot $fixtureBaseRoot
  Write-Host ("fixture run root: {0}" -f (Get-FixtureRelative $fixtureRoot))
  $previousReleaseVersion = [Environment]::GetEnvironmentVariable('COOLZHU_RELEASE_VERSION', 'Process')
  if ($previousReleaseVersion) { [Environment]::SetEnvironmentVariable('COOLZHU_RELEASE_VERSION', '', 'Process') }

  # --OnlyConcurrency：只跑 PKG-L07c 并发/发布权契约（不依赖 scripts/package-all.ps1），
  # 用于固定轮数的真实进程重复验证。
  if ($OnlyConcurrency) {
    [void](Invoke-L07cConcurrencySuite -Rounds $ConcurrencyRounds)
    $concurrencyFailed = @($script:caseResults | Where-Object { $_.status -eq 'FAIL' })
    $concurrencyPassed = @($script:caseResults | Where-Object { $_.status -eq 'PASS' })
    Write-Host ''
    Write-Host ("concurrency-only summary: pass={0} fail={1} rounds={2}" -f $concurrencyPassed.Count, $concurrencyFailed.Count, $ConcurrencyRounds)
    if ($concurrencyFailed.Count -gt 0) {
      Write-Host 'failures:'
      foreach ($failure in $concurrencyFailed) { Write-Host ("  - {0}: {1}" -f $failure.id, $failure.detail) }
      throw "package-webview2-loader concurrency contract failed: $($concurrencyFailed.Count) case(s)"
    }
    Write-Output 'PASS package-webview2-loader (concurrency-only)'
    return
  }

  # JSON 收据回读必须保留时间的原始字面量；PS 7.5+ 默认转 DateTime 会使摘要递归溢出。
  $dateDigests = [System.Collections.Generic.List[string]]::new()
  $dateRoundtripOk = $true
  foreach ($dateText in @('2026-09-27T04:20:00Z', '2026-09-27T04:20:00.000+00:00')) {
    $dateReceipt = [ordered]@{
      generated_at = $dateText
      publication = [ordered]@{ receipt_committed_utc = $dateText; receipt_digest = $null }
    }
    $beforeDigest = Get-LoaderReceiptDigest -Receipt $dateReceipt
    $datePath = Join-Path $fixtureRoot ("date-receipt-{0}.json" -f $dateDigests.Count)
    Write-FixtureText -Path $datePath -Text ($dateReceipt | ConvertTo-Json -Depth 6)
    $readReceipt = Get-Content -Raw -LiteralPath $datePath -Encoding UTF8 | ConvertFrom-LoaderJson
    $afterDigest = Get-LoaderReceiptDigest -Receipt $readReceipt
    $dateRoundtripOk = $dateRoundtripOk -and
      ($readReceipt.generated_at -is [string]) -and
      ([string]$readReceipt.generated_at -ceq $dateText) -and
      ([string]$readReceipt.publication.receipt_committed_utc -ceq $dateText) -and
      ($afterDigest -eq $beforeDigest)
    $dateDigests.Add($afterDigest)
  }
  Add-CaseResult -Id 'L00-date-text-roundtrip' -Status $(if ($dateRoundtripOk -and $dateDigests.Count -eq 2 -and $dateDigests[0] -ne $dateDigests[1]) { 'PASS' } else { 'FAIL' }) `
    -Detail '收据写入回读后摘要相同，且不同 ISO 字面量保持不同身份'
  $dateObjectRejected = $true
  foreach ($dateObject in @([datetime]::UtcNow, [datetimeoffset]::UtcNow)) {
    try { [void](Get-LoaderReceiptDigest -Receipt ([pscustomobject]@{ generated_at = $dateObject })); $dateObjectRejected = $false }
    catch { if ($_.Exception.Message -notmatch 'DateTime') { $dateObjectRejected = $false } }
  }
  Add-CaseResult -Id 'L00-date-object-rejected' -Status $(if ($dateObjectRejected) { 'PASS' } else { 'FAIL' }) `
    -Detail '日期对象明确拒绝进入收据摘要，避免无界属性递归'

  # ---------------------------------------------------------------- L01 -----
  # 声明位置 / 稳定导出位置存在旧手工 DLL ⇒ 不能仅凭存在采用。
  $l01 = New-LoaderFixture -Name 'l01-manual-dll-no-receipt' -Producers @() -MessageProducerIndexes @()
  Write-FixtureBytes -Path $l01.stablePath -Bytes (New-SyntheticPe -Fill 0xAB)
  $l01Result = Invoke-PackageFixture -Fixture $l01 -SkipBuild
  Assert-Category -Id 'L01a-manual-dll-without-receipt' -ExpectedCategory 'RECEIPT-MISSING' -PackageResult $l01Result -Note 'Test-Path 不再构成有效性检查'
  [void](Assert-NoLoaderStaged -Id 'L01a-manual-dll-without-receipt' -Fixture $l01 -PackageResult $l01Result)

  # ---------------------------------------------------------------- L05 -----
  # --no-build 且无匹配收据 ⇒ 拒绝，并提示"执行正常构建或指定匹配的构建产物"，不是"手工复制 DLL"。
  $l05Neg = Invoke-PackageFixture -Fixture $l01 -SkipBuild
  Assert-Category -Id 'L05a-no-build-without-receipt' -ExpectedCategory 'RECEIPT-MISSING' -PackageResult $l05Neg
  if ($l05Neg.Error -and $l05Neg.Error -notmatch '正常构建') {
    Add-CaseResult -Id 'L05a-no-build-guidance' -Status 'FAIL' -Detail "guidance must point at a normal build, got: $($l05Neg.Error -split "`n" | Select-Object -Last 1)"
  } elseif ($l05Neg.Error -match '手工复制|复制 DLL 到 bin') {
    # 文案必须明确否定"手工复制"，但不能把它当作解决手段推荐
    if ($l05Neg.Error -match 'next: .*不要手工复制') {
      Add-CaseResult -Id 'L05a-no-build-guidance' -Status 'PASS' -Detail 'guidance requires a normal build and explicitly rejects manual copying'
    } else {
      Add-CaseResult -Id 'L05a-no-build-guidance' -Status 'FAIL' -Detail 'manual copying is mentioned without an explicit prohibition'
    }
  } else {
    Add-CaseResult -Id 'L05a-no-build-guidance' -Status 'PASS' -Detail 'guidance requires a normal build'
  }

  # ------------------------------------------------------------ L01/L02 -----
  # 正常构建模式：两个生产者目录（其中一个 mtime 更新），构建消息只指向 B ⇒ 必须选 B，不按时间挑。
  $l02 = New-LoaderFixture -Name 'l02-hot-target-multi' -Producers @(
    [pscustomobject]@{ hash = 'aaaaaaaaaaaaaaaa'; bytes = (New-SyntheticPe -Fill 0x11); mtime = (Get-Date).ToUniversalTime() },
    [pscustomobject]@{ hash = 'bbbbbbbbbbbbbbbb'; bytes = (New-SyntheticPe -Fill 0x22); mtime = (Get-Date).ToUniversalTime().AddDays(-3) }
  ) -MessageProducerIndexes @(1)
  $l02Context = New-FixtureContext -Fixture $l02 -BuildStartedOffsetMinutes -1
  $l02Receipt = Export-LoaderArtifact -Context $l02Context
  $expectedBl = Get-FixtureHash (Join-Path $l02.buildRoot 'webview2-com-sys-bbbbbbbbbbbbbbbb/out/x64/WebView2Loader.dll')
  $expectedAl = Get-FixtureHash (Join-Path $l02.buildRoot 'webview2-com-sys-aaaaaaaaaaaaaaaa/out/x64/WebView2Loader.dll')
  if ($l02Receipt.file_identity.stable_export_sha256 -ne $expectedBl) {
    Add-CaseResult -Id 'L02a-identity-not-time' -Status 'FAIL' -Detail 'exported the wrong producer (time-based or ambiguous selection)'
  } elseif ($l02Receipt.file_identity.stable_export_sha256 -eq $expectedAl) {
    Add-CaseResult -Id 'L02a-identity-not-time' -Status 'FAIL' -Detail 'picked the newer-mtime unrelated producer'
  } else {
    Add-CaseResult -Id 'L02a-identity-not-time' -Status 'PASS' -Detail 'selected the producer named by this build, ignoring the newer unrelated directory'
  }
  if ([int]$l02Receipt.dependency_identity.producer_candidate_count -ne 1) {
    Add-CaseResult -Id 'L02a-evidence-recorded' -Status 'FAIL' -Detail 'receipt must record the producer candidate count'
  } else {
    Add-CaseResult -Id 'L02a-evidence-recorded' -Status 'PASS' -Detail 'receipt records package_id / out_dir / candidate count / selection evidence'
  }

  # 正常构建模式：消息指向两个不同 out_dir ⇒ 无法唯一确定 ⇒ 失败
  $l02b = New-LoaderFixture -Name 'l02b-ambiguous' -Producers @(
    [pscustomobject]@{ hash = 'aaaaaaaaaaaaaaaa'; bytes = (New-SyntheticPe -Fill 0x11) },
    [pscustomobject]@{ hash = 'bbbbbbbbbbbbbbbb'; bytes = (New-SyntheticPe -Fill 0x22) }
  ) -MessageProducerIndexes @(0, 1)
  $l02bResult = Invoke-PackageFixture -Fixture $l02b
  Assert-Category -Id 'L02b-ambiguous-producer' -ExpectedCategory 'AMBIGUOUS-PRODUCER' -PackageResult $l02bResult
  [void](Assert-NoLoaderStaged -Id 'L02b-ambiguous-producer' -Fixture $l02b -PackageResult $l02bResult)

  # 正常构建模式：构建消息里没有该依赖，但 target 里有两个残留目录 ⇒ 必须失败，不得扫描兜底
  $l02c = New-LoaderFixture -Name 'l02c-no-record' -Producers @(
    [pscustomobject]@{ hash = 'aaaaaaaaaaaaaaaa'; bytes = (New-SyntheticPe -Fill 0x11) },
    [pscustomobject]@{ hash = 'bbbbbbbbbbbbbbbb'; bytes = (New-SyntheticPe -Fill 0x22) }
  ) -MessageProducerIndexes @()
  $l02cResult = Invoke-PackageFixture -Fixture $l02c
  Assert-Category -Id 'L02c-no-directory-scan-fallback' -ExpectedCategory 'PRODUCER-NOT-FOUND' -PackageResult $l02cResult
  [void](Assert-NoLoaderStaged -Id 'L02c-no-directory-scan-fallback' -Fixture $l02c -PackageResult $l02cResult)

  # ---------------------------------------------------------------- L03 -----
  # 候选恰好一个，但构建记录指向允许根之外（例如另一个 profile / 旧源码目录）⇒ 拒绝
  $l03OutsideRoot = Join-Path $fixtureRoot 'l03-outside-root/other-target/debug'
  $l03 = New-LoaderFixture -Name 'l03-outside-root' -Producers @(
    [pscustomobject]@{ hash = 'cccccccccccccccc'; bytes = (New-SyntheticPe -Fill 0x44) }
  ) -MessageProducerIndexes @(0) -ProducerRootOverride $l03OutsideRoot
  $l03Result = Invoke-PackageFixture -Fixture $l03
  Assert-Category -Id 'L03a-outside-allowed-root' -ExpectedCategory 'OUTSIDE-ALLOWED-ROOT' -PackageResult $l03Result
  [void](Assert-NoLoaderStaged -Id 'L03a-outside-allowed-root' -Fixture $l03 -PackageResult $l03Result)

  # 候选唯一但属于**旧 profile**：允许根是 {profile}=debug，构建记录却指向 release 目录 ⇒ 拒绝
  $l03b = New-LoaderFixture -Name 'l03b-old-profile' -Producers @(
    [pscustomobject]@{ hash = 'dddddddddddddddd'; bytes = (New-SyntheticPe -Fill 0x55) }
  ) -MessageProducerIndexes @(0) -ProfileDirectory 'release'
  $l03bResult = Invoke-PackageFixture -Fixture $l03b
  Assert-Category -Id 'L03b-old-profile-producer' -ExpectedCategory 'OUTSIDE-ALLOWED-ROOT' -PackageResult $l03bResult
  [void](Assert-NoLoaderStaged -Id 'L03b-old-profile-producer' -Fixture $l03b -PackageResult $l03bResult)

  # 单候选、来源合法，但收据的源码身份早于当前源码 ⇒ --no-build 拒绝
  $l03c = New-LoaderFixture -Name 'l03c-stale-source' -Producers @(
    [pscustomobject]@{ hash = 'eeeeeeeeeeeeeeee'; bytes = (New-SyntheticPe -Fill 0x66) }
  ) -MessageProducerIndexes @(0)
  $l03cContext = New-FixtureContext -Fixture $l03c -BuildStartedOffsetMinutes -1
  [void](Export-LoaderArtifact -Context $l03cContext)
  $staleJson = Get-Content -Raw -LiteralPath $l03c.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
  $staleJson.build_identity.source_identity_sha256 = 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff'
  Write-FixtureText -Path $l03c.receiptPath -Text ($staleJson | ConvertTo-Json -Depth 12)
  $l03cResult = Invoke-PackageFixture -Fixture $l03c -SkipBuild
  Assert-Category -Id 'L03c-stale-source-identity' -ExpectedCategory 'RECEIPT-STALE' -PackageResult $l03cResult
  [void](Assert-NoLoaderStaged -Id 'L03c-stale-source-identity' -Fixture $l03c -PackageResult $l03cResult)

  # ---------------------------------------------------------------- L05 -----
  # --no-build 且收据与本次发布输入匹配 ⇒ 接受并发布（内容身份二次核对）
  $l05 = New-LoaderFixture -Name 'l05-no-build-ok' -Producers @(
    [pscustomobject]@{ hash = '1111111111111111'; bytes = (New-SyntheticPe -Fill 0x77) }
  ) -MessageProducerIndexes @(0)
  $l05Context = New-FixtureContext -Fixture $l05 -BuildStartedOffsetMinutes -1
  [void](Export-LoaderArtifact -Context $l05Context)
  $l05Result = Invoke-PackageFixture -Fixture $l05 -SkipBuild
  if ($l05Result.Error) {
    Add-CaseResult -Id 'L05b-no-build-with-matching-receipt' -Status 'FAIL' -Detail $l05Result.Error
  } else {
    $stagedLoader = Get-StagedLoaderPath -Fixture $l05
    $stagedHash = Get-FixtureHash $stagedLoader
    if ($stagedHash -ne (Get-FixtureHash $l05.stablePath)) {
      Add-CaseResult -Id 'L05b-no-build-with-matching-receipt' -Status 'FAIL' -Detail 'staged loader does not match the verified stable export'
    } elseif ($l05Result.Report.exported_artifacts[0].mode -ne 'no-build' -or $l05Result.Report.exported_artifacts[0].validation -ne 'existing-receipt-verified') {
      Add-CaseResult -Id 'L05b-no-build-with-matching-receipt' -Status 'FAIL' -Detail 'package report must record the no-build reuse + receipt verification'
    } else {
      Add-CaseResult -Id 'L05b-no-build-with-matching-receipt' -Status 'PASS' -Detail 'existing receipt accepted, content identity re-verified before and after publishing'
    }
  }

  # ---------------------------------------------------------------- L06 -----
  # 稳定导出文件内容与收据不一致（手工替换 / 复制中断 / 复制后改变）⇒ 生成正式包前失败
  $l06 = New-LoaderFixture -Name 'l06-content-mismatch' -Producers @(
    [pscustomobject]@{ hash = '2222222222222222'; bytes = (New-SyntheticPe -Fill 0x88) }
  ) -MessageProducerIndexes @(0)
  $l06Context = New-FixtureContext -Fixture $l06 -BuildStartedOffsetMinutes -1
  [void](Export-LoaderArtifact -Context $l06Context)
  Write-FixtureBytes -Path $l06.stablePath -Bytes (New-SyntheticPe -Fill 0x99)
  $l06Result = Invoke-PackageFixture -Fixture $l06 -SkipBuild
  Assert-Category -Id 'L06a-export-content-mismatch' -ExpectedCategory 'CONTENT-MISMATCH' -PackageResult $l06Result
  [void](Assert-NoLoaderStaged -Id 'L06a-export-content-mismatch' -Fixture $l06 -PackageResult $l06Result)

  # 生产者输出架构错误 ⇒ 导出即失败（正常构建模式）
  $l06b = New-LoaderFixture -Name 'l06b-wrong-arch' -Producers @(
    [pscustomobject]@{ hash = '3333333333333333'; bytes = (New-SyntheticPe -Fill 0x5A -Machine 0x014C) }
  ) -MessageProducerIndexes @(0)
  $l06bResult = Invoke-PackageFixture -Fixture $l06b
  Assert-Category -Id 'L06b-producer-wrong-architecture' -ExpectedCategory 'ARCH-MISMATCH' -PackageResult $l06bResult
  [void](Assert-NoLoaderStaged -Id 'L06b-producer-wrong-architecture' -Fixture $l06b -PackageResult $l06bResult)

  # 生产者输出损坏（截断到没有 PE 头）⇒ 失败
  $l06c = New-LoaderFixture -Name 'l06c-truncated' -Producers @(
    [pscustomobject]@{ hash = '4444444444444444'; bytes = ([byte[]](0x4D, 0x5A, 0x00, 0x00)) }
  ) -MessageProducerIndexes @(0)
  $l06cResult = Invoke-PackageFixture -Fixture $l06c
  Assert-Category -Id 'L06c-producer-truncated' -ExpectedCategory 'ARCH-MISMATCH' -PackageResult $l06cResult

  # 空文件
  $l06d = New-LoaderFixture -Name 'l06d-empty' -Producers @(
    [pscustomobject]@{ hash = '5555555555555555'; bytes = ([byte[]]@()) }
  ) -MessageProducerIndexes @(0)
  $l06dResult = Invoke-PackageFixture -Fixture $l06d
  Assert-Category -Id 'L06d-producer-empty' -ExpectedCategory 'SOURCE-INVALID' -PackageResult $l06dResult

  # 复制落地阶段：临时文件内容身份不符时必须拒绝（模拟"复制中改变"）
  $l06e = Join-Path $fixtureRoot 'l06e-copy-in-flight'
  New-Item -ItemType Directory -Force -Path $l06e | Out-Null
  $copySource = Join-Path $l06e 'source.dll'
  $copyTarget = Join-Path $l06e 'sub/out.dll'
  Write-FixtureBytes -Path $copySource -Bytes (New-SyntheticPe -Fill 0x10)
  $wrongHash = Get-FixtureHash $copySource
  Write-FixtureBytes -Path $copySource -Bytes (New-SyntheticPe -Fill 0x20)
  try {
    [void](Copy-LoaderVerifiedFile -Source $copySource -Destination $copyTarget -ExpectedSha256 $wrongHash -ExpectedLength 4096)
    Add-CaseResult -Id 'L06e-copy-in-flight-changed' -Status 'FAIL' -Detail 'a staging copy whose content no longer matches the verified identity was accepted'
  } catch {
    if ($_.Exception.Message -match 'content identity changed') {
      $leftover = @(Get-ChildItem -LiteralPath (Split-Path -Parent $copyTarget) -Filter '.tmp-*' -Force -ErrorAction SilentlyContinue).Count
      if ($leftover -ne 0) {
        Add-CaseResult -Id 'L06e-copy-in-flight-changed' -Status 'FAIL' -Detail 'temporary staging files must be cleaned up'
      } else {
        Add-CaseResult -Id 'L06e-copy-in-flight-changed' -Status 'PASS' -Detail 'staged copy rejected on identity mismatch and no fragment left behind'
      }
    } else {
      Add-CaseResult -Id 'L06e-copy-in-flight-changed' -Status 'FAIL' -Detail $_.Exception.Message
    }
  }

  # ---------------------------------------------------------------- L07 -----
  # 中断残片不会成为发布输入
  $l07 = New-LoaderFixture -Name 'l07-interrupted' -Producers @(
    [pscustomobject]@{ hash = '6666666666666666'; bytes = (New-SyntheticPe -Fill 0xAA) }
  ) -MessageProducerIndexes @(0)
  Write-FixtureBytes -Path (Join-Path (Split-Path -Parent $l07.stablePath) '.tmp-WebView2Loader.dll-deadbeef') -Bytes (New-SyntheticPe -Fill 0x01)
  $l07Result = Invoke-PackageFixture -Fixture $l07 -SkipBuild
  Assert-Category -Id 'L07a-interrupted-export-fragment' -ExpectedCategory 'RECEIPT-MISSING' -PackageResult $l07Result
  [void](Assert-NoLoaderStaged -Id 'L07a-interrupted-export-fragment' -Fixture $l07 -PackageResult $l07Result)

  # 收据写入失败（目标被目录占用）⇒ 不发布半成品
  $l07b = New-LoaderFixture -Name 'l07b-receipt-write-failure' -Producers @(
    [pscustomobject]@{ hash = '7777777777777777'; bytes = (New-SyntheticPe -Fill 0xBB) }
  ) -MessageProducerIndexes @(0) -ReceiptPathIsDirectory
  $l07bResult = Invoke-PackageFixture -Fixture $l07b
  if (-not $l07bResult.Error) {
    Add-CaseResult -Id 'L07b-receipt-write-failure' -Status 'FAIL' -Detail 'a failed receipt write must abort the run'
  } else {
    [void](Assert-NoLoaderStaged -Id 'L07b-receipt-write-failure' -Fixture $l07b -PackageResult $l07bResult)
    Add-CaseResult -Id 'L07b-receipt-write-failure' -Status 'PASS' -Detail 'receipt write failure aborted before publishing the loader'
  }

  # 并发写同一导出路径：只允许"一个发布者"，且每次落地都是完整可识别内容。
  # 完整口径（六条验收用例 + 固定轮数重复）见 Invoke-L07cConcurrencySuite。
  [void](Invoke-L07cConcurrencySuite -Rounds $ConcurrencyRounds)

  # ---------------------------------------------------------------- L08 -----
  # source 位于 junction 之下：必须按实际允许根校验，越界不复制、不清理
  $l08 = New-LoaderFixture -Name 'l08-junction' -Producers @(
    [pscustomobject]@{ hash = '8888888888888888'; bytes = (New-SyntheticPe -Fill 0xCC) }
  ) -MessageProducerIndexes @(0)
  $junctionRoot = Join-Path $fixtureRoot 'l08-junction-link'
  $junctionCreated = $false
  try {
    $null = New-Item -ItemType Junction -Path $junctionRoot -Target (Join-Path $l08.caseRoot 'tauri-target') -ErrorAction Stop
    $junctionCreated = $true
  } catch {
    $junctionCreated = $false
  }
  if (-not $junctionCreated) {
    Add-CaseResult -Id 'L08a-junction-producer' -Status 'SKIP' -Detail 'cannot create a junction in this environment'
  } else {
    $producerFile = Join-Path $l08.buildRoot 'webview2-com-sys-8888888888888888/out/x64/WebView2Loader.dll'
    $producerHashBefore = Get-FixtureHash $producerFile
    $junctionSpelling = (Join-Path $junctionRoot 'debug/build/webview2-com-sys-8888888888888888/out')
    $resolved = Resolve-LoaderRealPath $junctionSpelling
    $expectedResolved = Resolve-LoaderRealPath (Join-Path $l08.buildRoot 'webview2-com-sys-8888888888888888/out')
    if (-not $resolved.Equals($expectedResolved, [System.StringComparison]::OrdinalIgnoreCase)) {
      Add-CaseResult -Id 'L08a-junction-producer' -Status 'FAIL' -Detail "junction spelling did not resolve to the real producer: $resolved"
    } else {
      # 通过 junction 拼写重新生成消息，验证仍被允许根接受
      $linkedPath = 'native=' + (Join-Path $junctionSpelling 'x64')
      $outDirJson = ConvertTo-Json $junctionSpelling -Compress
      Write-FixtureText -Path $l08.messageFile -Text (@(
          '{"reason":"build-script-executed","package_id":"registry+https://github.com/rust-lang/crates.io-index#webview2-com-sys@0.38.2","out_dir":' + $outDirJson + ',"linked_paths":[' + (ConvertTo-Json $linkedPath -Compress) + '],"linked_libs":["advapi32"],"cfgs":[],"env":[],"filenames":[]}'
        ) -join "`n")
      $l08Result = Invoke-PackageFixture -Fixture $l08
      if ($l08Result.Error) {
        Add-CaseResult -Id 'L08a-junction-producer' -Status 'FAIL' -Detail $l08Result.Error
        [void](Assert-NoLoaderStaged -Id 'L08a-junction-producer' -Fixture $l08 -PackageResult $l08Result)
      } elseif ((Get-FixtureHash $l08.stablePath) -ne $producerHashBefore) {
        Add-CaseResult -Id 'L08a-junction-producer' -Status 'FAIL' -Detail 'exported content does not match the junction-reached producer'
      } elseif ((Get-FixtureHash $producerFile) -ne $producerHashBefore) {
        Add-CaseResult -Id 'L08a-junction-producer' -Status 'FAIL' -Detail 'the producer directory was modified by the export'
      } elseif (-not (Test-Path -LiteralPath (Join-Path $l08.buildRoot 'webview2-com-sys-8888888888888888/out/x64/WebView2Loader.dll'))) {
        Add-CaseResult -Id 'L08a-junction-producer' -Status 'FAIL' -Detail 'export must not move/clean files inside the build target'
      } else {
        Add-CaseResult -Id 'L08a-junction-producer' -Status 'PASS' -Detail 'resolved the real allowed root through a junction and copied without touching the producer tree'
      }
    }
  }

  # 越界根：生产者位于另一个 fixture 的 target 下 ⇒ 拒绝且不复制
  $l08b = New-LoaderFixture -Name 'l08b-outside' -Producers @(
    [pscustomobject]@{ hash = '9999999999999999'; bytes = (New-SyntheticPe -Fill 0xDD) }
  ) -MessageProducerIndexes @(0) -ProducerRootOverride (Join-Path $l02.caseRoot 'tauri-target')
  $l08bResult = Invoke-PackageFixture -Fixture $l08b
  Assert-Category -Id 'L08b-outside-root-not-copied' -ExpectedCategory 'OUTSIDE-ALLOWED-ROOT' -PackageResult $l08bResult
  [void](Assert-NoLoaderStaged -Id 'L08b-outside-root-not-copied' -Fixture $l08b -PackageResult $l08bResult)

  # ------------------------------------------------------------ L10/L11 -----
  # L10：收据必须把 Loader 来源与 WebView2 Runtime 前置**分开登记**，不得把 Runtime 问题伪称成 Loader 来源失败
  $l10Context = New-FixtureContext -Fixture $l05 -BuildStartedOffsetMinutes -1
  $l10Receipt = Get-Content -Raw -LiteralPath $l05.receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
  if ($l10Receipt.runtime_dependency.webview2_runtime_asserted -ne $false -or -not $l10Receipt.runtime_dependency.note) {
    Add-CaseResult -Id 'L10-runtime-registered-separately' -Status 'FAIL' -Detail 'receipt must register the WebView2 Runtime precondition separately from the loader source'
  } else {
    Add-CaseResult -Id 'L10-runtime-registered-separately' -Status 'PASS' -Detail 'receipt separates loader provenance from the WebView2 Runtime precondition'
  }
  Add-CaseResult -Id 'L10-runtime-distribution-change' -Status 'SKIP' -Detail '本轮不修改 Runtime 分发策略；目标机 Runtime 缺失/不适配的报错来自 Runtime 自身，需在真实安装环境验证'

  # L11：删除 DLL 的候选变更必须先完成消费者与原生窗口审计。这里先钉住"仍在发布"的事实，
  #      任何删除都必须显式改动本断言 + manifest，不能静默消失。
  $realManifest = Get-Content -Raw -LiteralPath (Join-Path $workspace 'config/package-manifest.json') -Encoding UTF8 | ConvertFrom-LoaderJson
  $realLoaderArtifact = @($realManifest.artifacts | Where-Object { $_.id -eq 'gui-desktop.webview2-loader' })
  if ($realLoaderArtifact.Count -ne 1) {
    Add-CaseResult -Id 'L11-loader-still-shipped-tripwire' -Status 'FAIL' -Detail 'the loader artifact disappeared from the manifest without the consumer audit'
  } else {
    Add-CaseResult -Id 'L11-loader-still-shipped-tripwire' -Status 'PASS' -Detail 'loader is still declared and staged; removing it requires this assertion + the L11 checklist'
  }
  Add-CaseResult -Id 'L11-dll-removal-audit' -Status 'SKIP' -Detail '本轮不删除 DLL；需要覆盖实际发布组合、构建配置、直接/延迟/运行时加载路径与清洁环境原生窗口启动后方可在独立变更中删除'

  # ---------------------------------------------------------------- L04 -----
  # 清洁 target + 已准备的离线依赖缓存 ⇒ 无手工复制即可构建 → 定位生产者 → 导出。
  if (-not $RunCleanTargetBuild) {
    Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'SKIP' -Detail '需要全新 target 目录 + 已准备离线依赖缓存跑一次完整 tauri-shell 构建（数百 crate，预计数十分钟）；加 -RunCleanTargetBuild 执行'
  } else {
    $l04Root = Join-Path $fixtureRoot 'l04-clean-target'
    $l04TargetDir = Join-Path $l04Root 'target'
    New-Item -ItemType Directory -Force -Path $l04TargetDir | Out-Null
    $l04Messages = Join-Path $l04Root 'build-messages.jsonl'
    $l04Stderr = Join-Path $l04Root 'build-stderr.log'
    $l04Args = @(
      'build', '--manifest-path', 'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml',
      '--bin', 'coolzhu-tauri-shell', '--offline', '--target-dir', (Get-FixtureRelative $l04TargetDir),
      '--message-format=json-render-diagnostics', '--release'
    )
    Write-Host ("L04: clean-target build -> " + ($l04Args -join ' '))
    $l04StartInfo = New-Object System.Diagnostics.ProcessStartInfo
    $l04StartInfo.FileName = 'cargo'
    $l04StartInfo.Arguments = (@($l04Args) | ForEach-Object { if ([string]$_ -match '\s') { '"' + [string]$_ + '"' } else { [string]$_ } }) -join ' '
    $l04StartInfo.WorkingDirectory = $workspaceFull
    $l04StartInfo.UseShellExecute = $false
    $l04StartInfo.RedirectStandardOutput = $true
    $l04StartInfo.RedirectStandardError = $true
    $l04Process = [System.Diagnostics.Process]::Start($l04StartInfo)
    $l04OutTask = $l04Process.StandardOutput.ReadToEndAsync()
    $l04ErrText = $l04Process.StandardError.ReadToEnd()
    $l04Process.WaitForExit()
    [System.IO.File]::WriteAllText($l04Messages, [string]$l04OutTask.Result, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText($l04Stderr, $l04ErrText, [System.Text.UTF8Encoding]::new($false))
    if ($l04Process.ExitCode -ne 0) {
      # 缺离线依赖 ≠ 选源算法失败：这里必须如实区分（L04 明示要求）。
      $l04Category = if ($l04ErrText -match 'no matching package|failed to download|offline|not found in the registry') {
        'dep-unavailable（不是选源失败）'
      } else { 'build-failed' }
      Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'FAIL' -Detail "clean-target build failed: $l04Category; log=$l04Stderr"
    } else {
      $l04Records = @(Get-LoaderBuildMessageRecords -BuildMessages @(Get-Content -LiteralPath $l04Messages -Encoding UTF8) -ProducerPackage 'webview2-com-sys')
      try {
        $l04Producer = Resolve-LoaderProducerRecord -Records $l04Records -ArtifactId 'gui-desktop.webview2-loader' -Target 'bin/WebView2Loader.dll' -Profile 'release'
      } catch {
        $l04Producer = $null
        Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'FAIL' -Detail $_.Exception.Message
      }
      if ($l04Producer) {
        $l04ProducerDll = Join-Path (Join-Path $l04Producer.out_dir 'x64') 'WebView2Loader.dll'
        $l04RealManifest = Get-Content -Raw -LiteralPath (Join-Path $workspace 'config/package-manifest.json') -Encoding UTF8 | ConvertFrom-LoaderJson
        $l04Artifact = @($l04RealManifest.artifacts | Where-Object { $_.id -eq 'gui-desktop.webview2-loader' })[0]
        $l04Export = $l04Artifact.export
        $l04Export.build_root = Get-FixtureRelative $l04TargetDir
        $l04Stable = Join-Path $l04Root 'stable/WebView2Loader.dll'
        $l04Receipt = Join-Path $l04Root 'export/webview2-loader-export.json'
        $l04Context = @{
          Artifact = $l04Artifact
          Export = $l04Export
          ArtifactId = [string]$l04Artifact.id
          Profile = 'release'
          RepoPath = $workspaceFull
          StablePath = $l04Stable
          ReceiptPath = $l04Receipt
          BuildEntryArtifact = 'gui-desktop.tauri-shell'
          BuildEntryManifest = [string]$l04Export.build_entry_manifest
          BuildEntryManifestSha256 = Get-FixtureHash (Join-Path $workspaceFull ([string]$l04Export.build_entry_manifest))
          LockfileRelative = [string]$l04Export.lockfile
          LockfileSha256 = Get-FixtureHash (Join-Path $workspaceFull ([string]$l04Export.lockfile))
          CargoTargetDir = Get-FixtureRelative $l04TargetDir
          BuildTarget = 'x86_64-pc-windows-msvc'
          HostTarget = 'x86_64-pc-windows-msvc'
          CargoVersion = 'cargo'
          ReleaseVersion = ''
          BuildCommand = 'cargo'
          BuildArgs = $l04Args
          BuildWorkingDir = '.'
          BuildStartedUtc = (Get-Date).ToUniversalTime().ToString('o')
          BuildMessages = @(Get-Content -LiteralPath $l04Messages -Encoding UTF8)
          RawRecordPath = $l04Messages
        }
        $l04ReceiptObject = Export-LoaderArtifact -Context $l04Context
        if (-not (Test-Path -LiteralPath $l04ProducerDll)) {
          Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'FAIL' -Detail "clean-target build did not produce a loader at the recorded producer: $l04ProducerDll"
        } elseif ($l04ReceiptObject.file_identity.stable_export_sha256 -ne (Get-FixtureHash $l04ProducerDll)) {
          Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'FAIL' -Detail 'clean-target export does not match the clean-build producer output'
        } elseif ($l04ReceiptObject.file_identity.raw_source -notlike ((Get-FixtureRelative $l04TargetDir) -replace '/', '\' + '*')) {
          Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'FAIL' -Detail "export did not come from the clean target: $($l04ReceiptObject.file_identity.raw_source)"
        } elseif (-not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $l04ProducerDll) 'WebView2Loader.dll'))) {
          Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'FAIL' -Detail 'clean-target producer tree was modified by the export'
        } else {
          Add-CaseResult -Id 'L04-clean-target-offline-deps' -Status 'PASS' -Detail 'clean target + offline dependency cache: built, located the producer from the build record, exported without manual copying'
        }
      }
    }
  }

  # ---------------------------------------------------------------- L09 -----
  Add-CaseResult -Id 'L09-clean-windows-native-window' -Status 'SKIP' -Detail '需在无开发 target / 无历史副本的清洁 Windows 环境安装 MSI 后启动原生窗口；本轮不做真实安装（不触碰活动安装）'

  if ($previousReleaseVersion) {
    [Environment]::SetEnvironmentVariable('COOLZHU_RELEASE_VERSION', $previousReleaseVersion, 'Process')
  }

  $failed = @($script:caseResults | Where-Object { $_.status -eq 'FAIL' })
  $passed = @($script:caseResults | Where-Object { $_.status -eq 'PASS' })
  $skipped = @($script:caseResults | Where-Object { $_.status -eq 'SKIP' })
  Write-Host ''
  Write-Host ("summary: pass={0} fail={1} skip={2}" -f $passed.Count, $failed.Count, $skipped.Count)
  if ($skipped.Count -gt 0) {
    Write-Host 'not executed (reasons):'
    foreach ($skip in $skipped) { Write-Host ("  - {0}: {1}" -f $skip.id, $skip.detail) }
  }
  if ($failed.Count -gt 0) {
    Write-Host 'failures:'
    foreach ($failure in $failed) { Write-Host ("  - {0}: {1}" -f $failure.id, $failure.detail) }
    throw "package-webview2-loader contract failed: $($failed.Count) case(s)"
  }

  Write-Output 'PASS package-webview2-loader'
} finally {
  if ($previousReleaseVersion) {
    [Environment]::SetEnvironmentVariable('COOLZHU_RELEASE_VERSION', $previousReleaseVersion, 'Process')
  }
  if (Test-Path -LiteralPath $fixtureRoot) {
    # 先摘掉 junction 再递归删除，避免把删除递归进 fixture 之外。
    Get-ChildItem -LiteralPath $fixtureRoot -Directory -Force -ErrorAction SilentlyContinue |
      Where-Object { $_.Attributes -band [System.IO.FileAttributes]::ReparsePoint } |
      ForEach-Object { cmd /c rmdir "$($_.FullName)" 2>$null | Out-Null }
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
