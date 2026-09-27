$ErrorActionPreference = 'Stop'

# ============================================================================
# 构建身份与报告治理契约测试（RD4-06 / 第五轮裁决 B-1、B-2）
#
# 覆盖的必测项：
#   I01  source_snapshot_digest 覆盖**新增/删除/修改**文件（真实做一次增删实验）
#   I02  排除规则不混入用户配置 / 凭据 / 运行数据库 / 模型权重 / target / gen
#   I03  allow_path_patterns 能把"被排除命名"的目录重新放行
#   I04  真实 scope 覆盖独立 Tauri 项目的构建入口与锁文件、嵌入式资源、安装器定义
#   I05  两遍静默枚举（不是"首尾各哈希一次"）+ 冻结记录 write-once 与冲突检测
#   I06  三个身份并存且互不冒充；build_input_digest 与 source_snapshot_digest 可独立变化
#   I07  收据口径：vcs_state=untracked_snapshot、source_commit 为空、
#        dirty_against_commit 不被写成 false
#   I08  package-all 全流程：两遍快照 + 构建后复核 + 载荷清单 + 报告 ID/内容哈希 + 指针
#   I09  报告内容哈希可从落盘文档重算；被改写后必须被拒
#   I10  产物清单引用报告（唯一 ID + 内容哈希），载荷清单覆盖暂存包全部文件
#   I11  构建期间源码变化必须 fail-closed（用"构建命令改写源码"的夹具确定性复现）
#   I12  保留规则：Protect/Verify/Prune + 归档副本兜底 + 索引损坏时拒绝清理
#
# 夹具全部落在当前工作树 tmp/ 下，不触碰真实 package/ 与真实 package-reports。
# ============================================================================

$workspace = Split-Path -Parent $PSScriptRoot
$workspaceFull = [System.IO.Path]::GetFullPath($workspace).TrimEnd('\', '/')
$tmpRootFull = [System.IO.Path]::GetFullPath((Join-Path $workspaceFull 'tmp')).TrimEnd('\', '/')
$fixtureRoot = [System.IO.Path]::GetFullPath((Join-Path $tmpRootFull 'package-build-identity-contract')).TrimEnd('\', '/')
$tmpPrefix = $tmpRootFull + [System.IO.Path]::DirectorySeparatorChar
if (-not $fixtureRoot.StartsWith($tmpPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "fixture root must stay inside the current worktree tmp directory: $fixtureRoot"
}

. (Join-Path $workspaceFull 'scripts/lib/build-identity.ps1')
$packageScript = Join-Path $workspaceFull 'scripts/package-all.ps1'
$retentionScript = Join-Path $workspaceFull 'scripts/package-report-retention.ps1'

$script:caseResults = [System.Collections.Generic.List[object]]::new()

function Add-CaseResult {
  param([string]$Id, [string]$Status, [string]$Detail)
  $script:caseResults.Add([pscustomobject]@{ id = $Id; status = $Status; detail = $Detail })
  Write-Host ("[{0}] {1}: {2}" -f $Status, $Id, $Detail)
}

function Assert-Case {
  param(
    [string]$Id,
    [bool]$Condition,
    [string]$Detail
  )
  if ($Condition) {
    Add-CaseResult -Id $Id -Status 'PASS' -Detail $Detail
  } else {
    Add-CaseResult -Id $Id -Status 'FAIL' -Detail $Detail
  }
}

function Write-FixtureText {
  param([Parameter(Mandatory = $true)][string]$Path, [AllowEmptyString()][string]$Text = '')
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Write-FixtureBinary {
  param([Parameter(Mandatory = $true)][string]$Path, [int]$Length = 64)
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [System.IO.File]::WriteAllBytes($Path, (New-Object 'byte[]' $Length))
}

function Get-RelFixturePath {
  param([Parameter(Mandatory = $true)][string]$Path)
  $full = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  return $full.Substring($workspaceFull.Length + 1).Replace('\', '/')
}

function Get-SnapshotPathSet {
  param([object]$Snapshot)
  $set = @{}
  foreach ($entry in @($Snapshot.entries)) { $set[[string]$entry.path] = $true }
  return $set
}

<#
  生成一个完整的身份夹具：第一方源码（含 Tauri 独立项目、Cargo.lock、build.rs、
  嵌入式资源、安装器定义、插件 crate）+ 必须被排除的类别 + artifact + 资源 + 清单。
#>
function New-IdentityCase {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [switch]$MutatingBuild
  )

  $caseRoot = Join-Path $fixtureRoot $Name
  if (Test-Path -LiteralPath $caseRoot) { Remove-Item -LiteralPath $caseRoot -Recurse -Force }
  $sourceRoot = Join-Path $caseRoot 'source'
  New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null

  # ---- 第一方源码与构建资源（应全部纳入） ----
  Write-FixtureText -Path (Join-Path $sourceRoot 'Cargo.toml') -Text "[workspace]`nmembers = [`"crate`", `"tauri`"]`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'Cargo.lock') -Text "version = 4`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/Cargo.toml') -Text "[package]`nname = `"fixture-crate`"`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/build.rs') -Text "fn main() {}`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/src/lib.rs') -Text "pub fn answer() -> u32 { 42 }`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/src/embedded.json') -Text '{ "kind": "fixture" }'
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/assets/app-icon.png') -Text 'PNG-PLACEHOLDER'
  Write-FixtureText -Path (Join-Path $sourceRoot 'tauri/Cargo.toml') -Text "[package]`nname = `"fixture-tauri`"`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'tauri/Cargo.lock') -Text "version = 4`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'tauri/build.rs') -Text "fn main() {}`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'tauri/tauri.conf.json') -Text '{ "productName": "fixture" }'
  Write-FixtureText -Path (Join-Path $sourceRoot 'web/app.js') -Text "console.log('fixture');`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'installer/Product.wxs') -Text '<Wix />'
  Write-FixtureText -Path (Join-Path $sourceRoot '.coolzhu/plugins/plugin-a/Cargo.toml') -Text "[package]`nname = `"fixture-plugin`"`n"

  # ---- 必须被排除的类别 ----
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/target/debug/junk.txt') -Text 'build output'
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/src/credentials.json') -Text '{"api_key":"should-never-enter-snapshot"}'
  Write-FixtureBinary -Path (Join-Path $sourceRoot 'crate/nested/weights.gguf') -Length 128
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/.coolzhu/web-sessions.json') -Text '{"session":"runtime-state"}'
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/run/web-sessions.sqlite3') -Text 'runtime-db'
  Write-FixtureText -Path (Join-Path $sourceRoot 'tauri/gen/schemas/capabilities.json') -Text '{"generated":true}'
  Write-FixtureText -Path (Join-Path $sourceRoot 'toolchain-notes/.env') -Text 'SECRET=1'
  # 放行规则用例：名为 tmp 的目录默认被排除，但被 allow_path_patterns 重新放行
  Write-FixtureText -Path (Join-Path $sourceRoot 'allow-case/tmp/keepme.txt') -Text 'allowed-override'

  # ---- 构建输入（声明在 build_inputs，但**不在**源码快照范围内） ----
  Write-FixtureText -Path (Join-Path $caseRoot 'toolchain-note.json') -Text '{"toolchain":"fixture-v1"}'

  # ---- artifact ----
  # 产物来源放到**声明源码快照范围内**：真实发布里产物来自被构建的树，其"来源可确认"
  # 才有意义（否则发布资格会因"产物来源无法确认"而为 false，掩盖本组用例的重点）。
  Write-FixtureText -Path (Join-Path $sourceRoot 'payload/artifact-payload.txt') -Text ('payload-' + $Name)

  $artifact = [ordered]@{
    id = 'fixture.payload'
    source = ((Get-RelFixturePath (Join-Path $sourceRoot 'payload/artifact-payload.txt')))
    target = 'bin/payload.txt'
  }
  if ($MutatingBuild) {
    # 构建命令在**构建期间改写源码**：用于确定性复现"并发编辑"必须被 fail-closed 拦住。
    $mutateScript = Join-Path $caseRoot 'mutate-source.ps1'
    $targetedFile = Join-Path $sourceRoot 'crate/src/lib.rs'
    Write-FixtureText -Path $mutateScript -Text (
      "param([string]`$TargetedFile)`n" +
      "[System.IO.File]::AppendAllText(`$TargetedFile, `"// mutated during build`" + [char]10)`n")
    $artifact.build = [ordered]@{
      command = 'powershell'
      args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $mutateScript, $targetedFile)
      working_dir = '.'
    }
  }

  $sourceRel = Get-RelFixturePath $sourceRoot
  $manifest = [ordered]@{
    package_root = (Get-RelFixturePath (Join-Path $caseRoot 'package'))
    backup_keep = 2
    artifacts = @($artifact)
    resources = @(
      [ordered]@{
        id = 'fixture.installer'
        source = "$sourceRel/installer"
        target = 'docs/installer'
      }
    )
    build_inputs = [ordered]@{
      files = @((Get-RelFixturePath (Join-Path $caseRoot 'toolchain-note.json')))
    }
    source_snapshot = [ordered]@{
      schema = 1
      roots = @(
        "$sourceRel/Cargo.toml"
        "$sourceRel/Cargo.lock"
        "$sourceRel/crate"
        "$sourceRel/tauri"
        "$sourceRel/web"
        "$sourceRel/installer"
        "$sourceRel/payload"
        "$sourceRel/.coolzhu/plugins"
      )
      allow_path_patterns = @()
      extra_exclude_path_patterns = @()
      allow_reparse_points = @()
      external_path_dependencies = @()
    }
  }

  $manifestPath = Join-Path $caseRoot 'manifest.json'
  Write-FixtureText -Path $manifestPath -Text ($manifest | ConvertTo-Json -Depth 12)

  return [pscustomobject]@{
    name = $Name
    caseRoot = $caseRoot
    sourceRoot = $sourceRoot
    manifestPath = $manifestPath
    packagePath = Join-Path $caseRoot 'package'
    reportPath = Join-Path $caseRoot 'report.json'
    pointerPath = Join-Path $caseRoot 'latest-debug.json'
  }
}

<#
  发布资格 / 失败诊断夹具（RD4-06 第六轮 §三）。
  artifact 的 source **落在声明源码快照范围内**（这正是"来源可确认"的前提）；
  另有可选构建命令在构建期间改写"声明了但不在快照范围内"的构建输入文件。
#>
function New-ReleasePolicyCase {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [switch]$MutateBuildInput,
    [switch]$OmitSourceSnapshotScope
  )

  $caseRoot = Join-Path $fixtureRoot $Name
  if (Test-Path -LiteralPath $caseRoot) { Remove-Item -LiteralPath $caseRoot -Recurse -Force }
  $sourceRoot = Join-Path $caseRoot 'source'
  New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null

  Write-FixtureText -Path (Join-Path $sourceRoot 'Cargo.toml') -Text "[workspace]`nmembers = []`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'crate/src/lib.rs') -Text "pub fn answer() -> u32 { 42 }`n"
  Write-FixtureText -Path (Join-Path $sourceRoot 'payload/payload.txt') -Text ("payload-" + $Name + "`n")
  Write-FixtureText -Path (Join-Path $sourceRoot 'payload/second.txt') -Text ("second-" + $Name + "`n")
  Write-FixtureText -Path (Join-Path $caseRoot 'build-inputs/toolchain.json') -Text '{"toolchain":"v1"}'

  $artifact = [ordered]@{
    id = 'rd406.payload'
    source = (Get-RelFixturePath (Join-Path $sourceRoot 'payload/payload.txt'))
    target = 'bin/payload.txt'
  }
  # 第二个 artifact 的作用：让"部分暂存 + 随后失败"可确定性复现（第一个已进暂存区，第二个失败）。
  $secondArtifact = [ordered]@{
    id = 'rd406.payload-two'
    source = (Get-RelFixturePath (Join-Path $sourceRoot 'payload/second.txt'))
    target = 'bin/second.txt'
  }
  if ($MutateBuildInput) {
    $mutateScript = Join-Path $caseRoot 'mutate-build-input.ps1'
    $targeted = Join-Path $caseRoot 'build-inputs/toolchain.json'
    Write-FixtureText -Path $mutateScript -Text (
      "param([string]`$TargetedFile)`n" +
      "[System.IO.File]::WriteAllText(`$TargetedFile, 'toolchain-changed-mid-run')`n")
    $artifact.build = [ordered]@{
      command = 'powershell'
      args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $mutateScript, $targeted)
      working_dir = '.'
    }
  }

  $manifest = [ordered]@{
    package_root = (Get-RelFixturePath (Join-Path $caseRoot 'package'))
    backup_keep = 2
    artifacts = @($artifact, $secondArtifact)
    resources = @()
    build_inputs = [ordered]@{ files = @((Get-RelFixturePath (Join-Path $caseRoot 'build-inputs/toolchain.json'))) }
    source_snapshot = [ordered]@{
      schema = 1
      roots = @((Get-RelFixturePath $sourceRoot))
      allow_path_patterns = @()
      extra_exclude_path_patterns = @()
      allow_reparse_points = @()
      external_path_dependencies = @()
    }
    release_policy = [ordered]@{
      non_build_input_paths = @(
        [ordered]@{ pattern = '(^|/)rd406-fixture-logs(/|$)'; reason = 'declared log output for the contract test' }
      )
    }
  }
  if ($OmitSourceSnapshotScope) { $manifest.Remove('source_snapshot') | Out-Null }

  $manifestPath = Join-Path $caseRoot 'manifest.json'
  Write-FixtureText -Path $manifestPath -Text ($manifest | ConvertTo-Json -Depth 12)
  return [pscustomobject]@{
    name         = $Name
    caseRoot     = $caseRoot
    sourceRoot   = $sourceRoot
    manifestPath = $manifestPath
    packagePath  = Join-Path $caseRoot 'package'
    reportPath   = Join-Path $caseRoot 'report.json'
    pointerPath  = Join-Path $caseRoot 'latest-debug.json'
    failureRoot  = Join-Path $caseRoot 'failures'
  }
}

function Get-ReleasePolicyFailureDiagnostic {
  param([Parameter(Mandatory = $true)][object]$Case)

  $files = @(Get-ChildItem -LiteralPath $Case.failureRoot -Filter '*.json' -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
  if ($files.Count -eq 0) { return $null }
  return (Get-Content -Raw -LiteralPath $files[0].FullName -Encoding UTF8 | ConvertFrom-Json)
}

try {
  if (Test-Path -LiteralPath $fixtureRoot) {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null

  # ======================================================== I01/I05 快照语义 ==
  $basic = New-IdentityCase -Name 'basic'
  $scope = Get-SourceSnapshotScope -ManifestData (Get-Content -Raw -LiteralPath $basic.manifestPath -Encoding UTF8 | ConvertFrom-Json)
  $snapA = New-SourceSnapshot -RepoPath $workspaceFull -Scope $scope

  $included = @(
    "$(Get-RelFixturePath $basic.sourceRoot)/Cargo.toml"
    "$(Get-RelFixturePath $basic.sourceRoot)/Cargo.lock"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/build.rs"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/src/lib.rs"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/src/embedded.json"
    "$(Get-RelFixturePath $basic.sourceRoot)/tauri/Cargo.toml"
    "$(Get-RelFixturePath $basic.sourceRoot)/tauri/Cargo.lock"
    "$(Get-RelFixturePath $basic.sourceRoot)/tauri/build.rs"
    "$(Get-RelFixturePath $basic.sourceRoot)/tauri/tauri.conf.json"
    "$(Get-RelFixturePath $basic.sourceRoot)/web/app.js"
    "$(Get-RelFixturePath $basic.sourceRoot)/installer/Product.wxs"
    "$(Get-RelFixturePath $basic.sourceRoot)/.coolzhu/plugins/plugin-a/Cargo.toml"
  )
  $excluded = @(
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/target/debug/junk.txt"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/src/credentials.json"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/nested/weights.gguf"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/.coolzhu/web-sessions.json"
    "$(Get-RelFixturePath $basic.sourceRoot)/crate/run/web-sessions.sqlite3"
    "$(Get-RelFixturePath $basic.sourceRoot)/tauri/gen/schemas/capabilities.json"
    "$(Get-RelFixturePath $basic.sourceRoot)/toolchain-notes/.env"
  )
  $snapASet = Get-SnapshotPathSet -Snapshot $snapA

  $missingIncluded = @($included | Where-Object { -not $snapASet.ContainsKey($_) })
  Assert-Case -Id 'I01a-snapshot-covers-first-party-source' -Condition ($missingIncluded.Count -eq 0) `
    -Detail ("源码快照覆盖 {0} 个文件（含独立 Tauri 项目的 Cargo.toml/Cargo.lock/build.rs/tauri.conf.json）；缺失：{1}" -f $snapA.file_count, $(if ($missingIncluded.Count -eq 0) { '(none)' } else { $missingIncluded -join ', ' }))

  $presentExcluded = @($excluded | Where-Object { $snapASet.ContainsKey($_) })
  Assert-Case -Id 'I02a-snapshot-excludes-forbidden-categories' -Condition ($presentExcluded.Count -eq 0) `
    -Detail ("用户配置/凭据/运行数据库/模型权重/target/gen 均未进入快照；意外进入：{0}" -f $(if ($presentExcluded.Count -eq 0) { '(none)' } else { $presentExcluded -join ', ' }))

  $snapARerun = New-SourceSnapshot -RepoPath $workspaceFull -Scope $scope
  Assert-Case -Id 'I05a-quiescent-two-pass-reproducible' -Condition ($snapARerun.source_snapshot_digest -eq $snapA.source_snapshot_digest) `
    -Detail ("同一棵不变树上两遍静默枚举得到同一摘要：{0}" -f $snapA.source_snapshot_digest)

  # 两遍枚举是硬约束：单遍枚举必须被拒（否则无法区分"稳定"与"枚举期间被改"）
  $passCountRejected = $false
  try {
    [void](New-SourceSnapshot -RepoPath $workspaceFull -Scope $scope -Passes 1)
  } catch {
    $passCountRejected = ($_.Exception.Message -match 'SOURCE-SNAPSHOT-PASS-COUNT')
  }
  Assert-Case -Id 'I05b-single-pass-refused' -Condition $passCountRejected `
    -Detail 'Passes < 2 被 fail-closed 拒绝，避免"只看一次"的假一致'

  # 冻结记录：write-once；同 digest 复用，内容清单必须仍等于该 digest
  $freezeA = Write-SourceSnapshotFreezeRecord -RepoPath $workspaceFull -Snapshot $snapA
  $freezeARerun = Write-SourceSnapshotFreezeRecord -RepoPath $workspaceFull -Snapshot $snapA
  Assert-Case -Id 'I05c-freeze-record-write-once' -Condition ($freezeARerun.reused_existing_record -eq $true -and $freezeARerun.content_sha256 -eq $freezeA.content_sha256) `
    -Detail ("冻结记录按摘要寻址且不覆盖：{0}" -f $freezeA.path)

  # ---- 增删改实验 ----
  $addedFile = Join-Path $basic.sourceRoot 'crate/src/added_later.rs'
  Write-FixtureText -Path $addedFile -Text "pub fn added() {}`n"
  $snapB = New-SourceSnapshot -RepoPath $workspaceFull -Scope $scope
  $addDiff = Compare-SourceSnapshot -Left $snapA -Right $snapB
  $addedRel = Get-RelFixturePath $addedFile
  Assert-Case -Id 'I01b-added-file-changes-digest' -Condition (
    $snapB.source_snapshot_digest -ne $snapA.source_snapshot_digest -and
    $addDiff.added_count -eq 1 -and
    $addDiff.added -contains $addedRel) `
    -Detail ("新增文件进入快照差异：digest {0} → {1}；added={2}" -f $snapA.source_snapshot_digest.Substring(0, 12), $snapB.source_snapshot_digest.Substring(0, 12), ($addDiff.added -join ', '))

  Remove-Item -LiteralPath (Join-Path $basic.sourceRoot 'web/app.js') -Force
  $snapC = New-SourceSnapshot -RepoPath $workspaceFull -Scope $scope
  $removeDiff = Compare-SourceSnapshot -Left $snapB -Right $snapC
  $removedRel = "$(Get-RelFixturePath $basic.sourceRoot)/web/app.js"
  Assert-Case -Id 'I01c-deleted-file-changes-digest' -Condition (
    $snapC.source_snapshot_digest -ne $snapB.source_snapshot_digest -and
    $removeDiff.removed_count -eq 1 -and
    $removeDiff.removed -contains $removedRel) `
    -Detail ("删除文件进入快照差异：removed={0}" -f ($removeDiff.removed -join ', '))

  Write-FixtureText -Path (Join-Path $basic.sourceRoot 'crate/src/lib.rs') -Text "pub fn answer() -> u32 { 43 }`n"
  $snapD = New-SourceSnapshot -RepoPath $workspaceFull -Scope $scope
  $modifyDiff = Compare-SourceSnapshot -Left $snapC -Right $snapD
  Assert-Case -Id 'I01d-modified-file-changes-digest' -Condition (
    $snapD.source_snapshot_digest -ne $snapC.source_snapshot_digest -and
    $modifyDiff.modified_count -eq 1 -and
    $modifyDiff.modified -contains "$(Get-RelFixturePath $basic.sourceRoot)/crate/src/lib.rs") `
    -Detail ("内容修改进入快照差异：modified={0}" -f ($modifyDiff.modified -join ', '))

  # ==================================================== I03 allow_path_patterns ==
  # 放行规则必须同时匹配到被排除的**目录本身**，否则目录在遍历时就被剪枝、文件走不到。
  $allowScope = [pscustomobject]@{
    declared_by = 'test'
    roots = @("$(Get-RelFixturePath $basic.sourceRoot)/allow-case")
    exclude_path_patterns = @($script:SourceSnapshotDefaultExcludePatterns)
    allow_path_patterns = @('(^|/)tmp(/|$)')
    allow_reparse_points = @()
    known_source_extensions = @($script:SourceSnapshotDefaultKnownExtensions)
    external_path_dependencies = @()
  }
  $allowEntries = Get-SourceSnapshotEntries -RepoPath $workspaceFull -Scope $allowScope
  $allowRel = "$(Get-RelFixturePath $basic.sourceRoot)/allow-case/tmp/keepme.txt"
  Assert-Case -Id 'I03a-allow-pattern-reincludes-excluded-name' -Condition (@($allowEntries.entries | Where-Object { $_.path -eq $allowRel }).Count -eq 1) `
    -Detail ("被排除命名（tmp）经 allow_path_patterns 显式放行后进入快照：{0}" -f $allowRel)

  # ================================== I04 真实 scope 覆盖 Tauri 独立构建入口与锁文件 ==
  $realManifest = Get-Content -Raw -LiteralPath (Join-Path $workspaceFull 'config/package-manifest.json') -Encoding UTF8 | ConvertFrom-Json
  $realScope = Get-SourceSnapshotScope -ManifestData $realManifest
  $realEntries = Get-SourceSnapshotEntries -RepoPath $workspaceFull -Scope $realScope
  $realSet = @{}
  foreach ($entry in @($realEntries.entries)) { $realSet[[string]$entry.path] = $true }

  $realRequired = @(
    'Cargo.lock'
    'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml'
    'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.lock'
    'modules/gui-desktop/packages/tauri-shell/src-tauri/build.rs'
    'modules/gui-desktop/packages/tauri-shell/src-tauri/tauri.conf.json'
    'modules/gui-desktop/packages/tauri-shell/src-tauri/capabilities/default.json'
    'modules/gui-web/packages/web-console/src/main.rs'
    'modules/gui-web/packages/web-console/index.html'
    'modules/computer-use/packages/computer-use-core/src/input_stroke_native.cs'
    'modules/gui-desktop/packages/desktop-console/assets/app-icon-cz-moon-gate-lantern-v1.png'
    'modules/gui-desktop/packages/tauri-shell/ui/assets/pet-theme.json'
    'tests/fixtures/s0-golden/manifest.json'
    'installer/Product.wxs'
    'docs/design-assets/coolzhu-icons-2026-08-27/final/app-icon-cz-moon-gate-lantern-v1.ico'
    '.coolzhu/plugins/coolzhu-tdd-runner/Cargo.toml'
  )
  $realMissing = @($realRequired | Where-Object { -not $realSet.ContainsKey($_) })
  Assert-Case -Id 'I04a-real-scope-covers-build-inputs' -Condition ($realMissing.Count -eq 0) `
    -Detail ("真实 scope 覆盖第一方 Rust/C#/JS、build.rs、编译期嵌入资源、安装器定义与**独立 Tauri 项目的真实构建入口与锁文件**；缺失：{0}" -f $(if ($realMissing.Count -eq 0) { '(none)' } else { $realMissing -join ', ' }))

  $realForbidden = @($realSet.Keys | Where-Object {
      $_ -match '(^|/)(target|dist|node_modules|package|gen)(/|$)|web-sessions|coolzhu\.toml$|\.env($|\.)|\.(sqlite3?|db)$|\.(pem|key|pfx|p12)$|\.(gguf|onnx|safetensors|pth|pt|ckpt)$|(^|/)(credentials?|secrets?|token-cache)(\.[^/]+)?$'
    })
  Assert-Case -Id 'I04b-real-scope-free-of-forbidden-categories' -Condition ($realForbidden.Count -eq 0) `
    -Detail ("真实 scope（{0} 个文件）不含用户配置/凭据/运行数据库/模型权重/无关 target/gen；命中：{1}" -f @($realEntries.entries).Count, $(if ($realForbidden.Count -eq 0) { '(none)' } else { $realForbidden -join ', ' }))

  $runtimeStateExcluded = @($realSet.Keys | Where-Object { $_ -match 'web-console/\.coolzhu/' }).Count -eq 0
  Assert-Case -Id 'I04c-runtime-state-excluded' -Condition $runtimeStateExcluded `
    -Detail 'web-console 下的 .coolzhu 运行态（会话/音频分片）没有进入快照'

  # ========================================================= I06 三身份不冒充 ==
  $cargoIdentity = [pscustomobject]@{
    cargo_version = 'cargo fixture'
    host_target = 'x86_64-pc-windows-msvc'
    build_target = 'x86_64-pc-windows-msvc'
  }
  $identityManifest = Get-Content -Raw -LiteralPath $basic.manifestPath -Encoding UTF8 | ConvertFrom-Json
  $snapIdentity = New-SourceSnapshot -RepoPath $workspaceFull -Scope (Get-SourceSnapshotScope -ManifestData $identityManifest)
  $descriptorsA = Get-BuildInputDescriptors -RepoPath $workspaceFull -ManifestData $identityManifest -ManifestPath $basic.manifestPath -Configuration 'debug' -CargoIdentity $cargoIdentity -SourceSnapshotDigest $snapIdentity.source_snapshot_digest
  $buildInputA = Get-BuildInputDigest -DescriptorLines $descriptorsA

  Write-FixtureText -Path (Join-Path $basic.caseRoot 'toolchain-note.json') -Text '{"toolchain":"fixture-v2"}'
  $snapIdentityUnchanged = New-SourceSnapshot -RepoPath $workspaceFull -Scope (Get-SourceSnapshotScope -ManifestData $identityManifest)
  $descriptorsB = Get-BuildInputDescriptors -RepoPath $workspaceFull -ManifestData $identityManifest -ManifestPath $basic.manifestPath -Configuration 'debug' -CargoIdentity $cargoIdentity -SourceSnapshotDigest $snapIdentityUnchanged.source_snapshot_digest
  $buildInputB = Get-BuildInputDigest -DescriptorLines $descriptorsB

  Assert-Case -Id 'I06a-three-identities-are-distinct' -Condition (
    $snapIdentity.source_snapshot_digest -ne $buildInputA -and
    $snapIdentity.source_snapshot_digest -ne $snapA.file_set_digest) `
    -Detail ("三身份取值互不相同：source_snapshot_digest={0} build_input_digest={1}（file_set_digest 另计：{2}）" -f $snapIdentity.source_snapshot_digest.Substring(0, 12), $buildInputA.Substring(0, 12), $snapA.file_set_digest.Substring(0, 12))

  Assert-Case -Id 'I06b-build-input-independent-of-source-snapshot' -Condition (
    $buildInputB -ne $buildInputA -and
    $snapIdentityUnchanged.source_snapshot_digest -eq $snapIdentity.source_snapshot_digest) `
    -Detail ("只改构建输入（源码快照范围之外的工具链声明）时：build_input_digest 变化而 source_snapshot_digest 不变（{0} → {1}）" -f $buildInputA.Substring(0, 12), $buildInputB.Substring(0, 12))

  # 独立 Tauri 项目是独立 Cargo 项目：用**真实 manifest** 断言它的锁文件/构建入口
  # 被 build_input_digest 显式登记（夹具没有 export 块，测不出这一条）。
  $realDescriptors = Get-BuildInputDescriptors -RepoPath $workspaceFull -ManifestData $realManifest -ManifestPath (Join-Path $workspaceFull 'config/package-manifest.json') -Configuration 'debug' -CargoIdentity $cargoIdentity -SourceSnapshotDigest ('0' * 64)
  $realDeclaredInputs = @($realDescriptors | Where-Object { $_ -like 'input_file=*' })
  $standaloneLockDeclared = @($realDeclaredInputs | Where-Object { $_ -like 'input_file=modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.lock|*' }).Count -eq 1
  $standaloneManifestDeclared = @($realDeclaredInputs | Where-Object { $_ -like 'input_file=modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml|*' }).Count -eq 1
  $workspaceLockDeclared = @($realDeclaredInputs | Where-Object { $_ -like 'input_file=Cargo.lock|*' }).Count -eq 1
  Assert-Case -Id 'I06c-standalone-tauri-lockfile-declared' -Condition ($standaloneLockDeclared -and $standaloneManifestDeclared -and $workspaceLockDeclared) `
    -Detail ("真实 build_input_digest 同时登记顶层 workspace 锁文件与**独立 Tauri 项目自己的** Cargo.lock / Cargo.toml（登记项 {0} 条）" -f $realDeclaredInputs.Count)

  # ============================================================== I07 收据口径 ==
  $vcs = Get-VcsReferenceState -RepoPath $workspaceFull
  $dirtyIsNotFalse = -not ($vcs.dirty_against_commit -is [bool])
  Assert-Case -Id 'I07a-vcs-state-untracked-snapshot' -Condition (
    $vcs.vcs_state -eq 'untracked_snapshot' -and
    $null -eq $vcs.source_commit -and
    $vcs.dirty_against_commit -eq 'not_evaluable' -and
    $dirtyIsNotFalse) `
    -Detail ("vcs_state={0} source_commit={1} dirty_against_commit={2}（未跟踪的变更 **不**写成 dirty=false）vcs_reference_commit={3}" -f $vcs.vcs_state, $(if ($null -eq $vcs.source_commit) { 'null' } else { 'set' }), $vcs.dirty_against_commit, $(if ($vcs.vcs_reference_commit) { $vcs.vcs_reference_commit.Substring(0, 12) } else { '(none)' }))

  # ==================================== I08/I09/I10 package-all 全流程（成功） ==
  $success = New-IdentityCase -Name 'pipeline-ok'
  & $packageScript -Manifest $success.manifestPath -Configuration debug -ReportPath $success.reportPath -PackageRoot $success.packagePath | Out-Null

  Assert-Case -Id 'I08a-report-written' -Condition (Test-Path -LiteralPath $success.reportPath -PathType Leaf) `
    -Detail ("报告落点沿用既有命名：{0}" -f (Get-RelFixturePath $success.reportPath))

  $report = Get-Content -Raw -LiteralPath $success.reportPath -Encoding UTF8 | ConvertFrom-Json
  $reportIdShape = ([string]$report.report_identity.report_id) -match '^pkg-report-debug-[0-9]{8}-[0-9]{9}-[0-9a-f]{8}$'
  Assert-Case -Id 'I08b-report-unique-id' -Condition $reportIdShape `
    -Detail ("报告唯一 ID：{0}" -f $report.report_identity.report_id)

  $hashVerify = $null
  $hashVerifyError = $null
  try {
    $hashVerify = Assert-PackageReportContentHash -Report $report
  } catch {
    $hashVerifyError = $_.Exception.Message
  }
  Assert-Case -Id 'I09a-report-content-hash-verifiable' -Condition ($null -ne $hashVerify -and $hashVerify.content_sha256 -eq [string]$report.report_identity.content_sha256) `
    -Detail ("报告内容哈希可从落盘文档重算：{0}{1}" -f $report.report_identity.content_sha256, $(if ($hashVerifyError) { " (error: $hashVerifyError)" } else { '' }))

  $tamperedPath = Join-Path $success.caseRoot 'report-tampered.json'
  $tamperedText = (Get-Content -Raw -LiteralPath $success.reportPath -Encoding UTF8).Replace('"configuration":  "debug"', '"configuration":  "release"')
  [System.IO.File]::WriteAllText($tamperedPath, $tamperedText, [System.Text.UTF8Encoding]::new($false))
  $tamperRejected = $false
  $tamperMessage = ''
  try {
    [void](Assert-PackageReportContentHash -Report (Get-Content -Raw -LiteralPath $tamperedPath -Encoding UTF8 | ConvertFrom-Json))
  } catch {
    $tamperRejected = ($_.Exception.Message -match 'REPORT-CONTENT-MISMATCH')
    $tamperMessage = ($_.Exception.Message -split "`n")[0]
  }
  Assert-Case -Id 'I09b-tampered-report-rejected' -Condition $tamperRejected `
    -Detail ("被改写的报告必须被拒：{0}" -f $tamperMessage)

  $inventoryPath = Join-Path $success.packagePath 'payload-inventory.json'
  Assert-Case -Id 'I10a-payload-inventory-inside-payload' -Condition (Test-Path -LiteralPath $inventoryPath -PathType Leaf) `
    -Detail '载荷清单写在暂存包内（分发的载荷自身携带可核对的载荷身份与报告引用）'

  $inventory = Get-Content -Raw -LiteralPath $inventoryPath -Encoding UTF8 | ConvertFrom-Json
  Assert-Case -Id 'I10b-inventory-references-report' -Condition (
    [string]$inventory.report_ref.report_id -eq [string]$report.report_identity.report_id -and
    [string]$inventory.report_ref.content_sha256 -eq [string]$report.report_identity.content_sha256) `
    -Detail ("产物清单引用报告的唯一 ID + 内容哈希：{0} / {1}" -f $inventory.report_ref.report_id, $inventory.report_ref.content_sha256)

  Assert-Case -Id 'I10c-identities-coexist-in-artifacts' -Condition (
    [string]$report.build_identity.source_snapshot_digest -eq [string]$inventory.build_identity.source_snapshot_digest -and
    [string]$report.build_identity.build_input_digest -eq [string]$inventory.build_identity.build_input_digest -and
    [string]$report.build_identity.payload_digest -eq [string]$inventory.build_identity.payload_digest -and
    [string]$report.build_identity.payload_digest -eq [string]$inventory.payload_digest -and
    [string]$report.payload_inventory.payload_digest -eq [string]$inventory.payload_digest) `
    -Detail ("报告与清单里三身份并存且一致：source={0} build_input={1} payload={2}" -f ([string]$report.build_identity.source_snapshot_digest).Substring(0, 12), ([string]$report.build_identity.build_input_digest).Substring(0, 12), ([string]$report.build_identity.payload_digest).Substring(0, 12))

  $stagedFiles = @(Get-ChildItem -LiteralPath $success.packagePath -File -Recurse -Force | ForEach-Object {
      $_.FullName.Substring($success.packagePath.Length + 1).Replace('\', '/')
    })
  $inventoryFiles = @($inventory.files | ForEach-Object { [string]$_.path })
  $expectedPayloadFiles = @($stagedFiles | Where-Object { $_ -ne 'payload-inventory.json' } | Sort-Object)
  $actualPayloadFiles = @($inventoryFiles | Sort-Object)
  Assert-Case -Id 'I10d-payload-inventory-covers-staged-files' -Condition (
    $inventoryFiles -notcontains 'payload-inventory.json' -and
    (($expectedPayloadFiles -join '|') -eq ($actualPayloadFiles -join '|'))) `
    -Detail ("载荷清单覆盖暂存包全部文件（{0} 个，排除清单载体自身以免自引用）：{1}" -f $inventoryFiles.Count, ($inventoryFiles -join ', '))

  $pointerExists = Test-Path -LiteralPath $success.pointerPath -PathType Leaf
  $pointer = if ($pointerExists) { Get-Content -Raw -LiteralPath $success.pointerPath -Encoding UTF8 | ConvertFrom-Json } else { $null }
  Assert-Case -Id 'I08c-report-pointer-and-post-build-verification' -Condition (
    $pointerExists -and
    [string]$pointer.report_id -eq [string]$report.report_identity.report_id -and
    [string]$pointer.report_content_sha256 -eq [string]$report.report_identity.content_sha256 -and
    $report.source_snapshot.post_build_verification.identical -eq $true -and
    [string]$report.source_snapshot.post_build_verification.pre_build_digest -eq [string]$report.source_snapshot.digest) `
    -Detail ("latest 指针与构建后复核同时成立：post_build_verification.identical={0} method={1}" -f $report.source_snapshot.post_build_verification.identical, $report.source_snapshot.post_build_verification.method)

  Assert-Case -Id 'I08d-report-vcs-fields' -Condition (
    [string]$report.build_context.vcs_state -eq 'untracked_snapshot' -and
    $null -eq $report.build_context.source_commit -and
    [string]$report.build_context.dirty_against_commit -eq 'not_evaluable') `
    -Detail ("报告里的 VCS 口径：vcs_state={0} source_commit={1} dirty_against_commit={2}" -f $report.build_context.vcs_state, $(if ($null -eq $report.build_context.source_commit) { 'null' } else { 'set' }), $report.build_context.dirty_against_commit)

  # ================================ I11 构建期间源码变化必须 fail-closed ======
  $mutating = New-IdentityCase -Name 'pipeline-source-changed' -MutatingBuild
  $mutateError = ''
  try {
    & $packageScript -Manifest $mutating.manifestPath -Configuration debug -ReportPath $mutating.reportPath -PackageRoot $mutating.packagePath | Out-Null
  } catch {
    $mutateError = $_.Exception.Message
  }
  $mutateRejected = $mutateError -match 'SOURCE-SNAPSHOT-CHANGED'
  Assert-Case -Id 'I11a-build-time-source-change-fails-closed' -Condition $mutateRejected `
    -Detail ("构建命令改写了快照范围内的源码 ⇒ fail-closed：{0}" -f (($mutateError -split "`n")[1]))

  Assert-Case -Id 'I11b-no-report-on-source-change' -Condition (-not (Test-Path -LiteralPath $mutating.reportPath -PathType Leaf)) `
    -Detail '被 fail-closed 拦下的运行不得留下报告（否则会留下"看似有效"的构建身份）'

  $mutateMentionsPath = $mutateError -match 'crate/src/lib\.rs'
  Assert-Case -Id 'I11c-source-change-lists-changed-path' -Condition $mutateMentionsPath `
    -Detail '失败信息必须给出发生变化的具体路径（mod/added/removed），而不是笼统的"哈希不一致"'

  # ============ I13 失败诊断与发布资格分离（RD4-06 / 第六轮裁决 §三）========
  # 裁决要点：维持拒绝发布不稳定输入所生成的包；但**必须**输出失败诊断；
  # 失败后不得覆盖上一份成功报告 / 不得更新"最新有效包" / 不得进入签名安装分发；
  # 临时产物与可发布产物分区且不自动晋升。
  $diagnosticI11 = Get-ReleasePolicyFailureDiagnostic -Case ([pscustomobject]@{ failureRoot = (Join-Path $mutating.caseRoot 'failures') })
  Assert-Case -Id 'I13a-failure-diagnostic-written' -Condition ($null -ne $diagnosticI11) `
    -Detail ("失败也必须产出诊断（落在 <report>/failures/ 分区，不是成功报告落点）：{0}" -f $(if ($diagnosticI11) { $diagnosticI11.report_identity.report_id } else { '(missing)' }))
  if ($diagnosticI11) {
    Assert-Case -Id 'I13b-diagnostic-minimum-fields' -Condition (
      [string]$diagnosticI11.kind -eq 'package-report-failure' -and
      [string]$diagnosticI11.report_status -eq 'failed' -and
      (Test-IdentityFalseValue -Value $diagnosticI11.release_eligible) -and
      [string]$diagnosticI11.run_id -and
      [string]$diagnosticI11.failure.stage -and
      [string]$diagnosticI11.failure.category -and
      @($diagnosticI11.stage_exit_codes).Count -ge 1 -and
      @($diagnosticI11.declared_input_scope.source_snapshot_roots).Count -ge 1 -and
      [string]$diagnosticI11.input_scope_confirmation.confirmed -eq 'True') `
      -Detail ("kind={0} stage={1} category={2} run_id={3} stage_records={4} declared_roots={5}" -f $diagnosticI11.kind, $diagnosticI11.failure.stage, $diagnosticI11.failure.category, $diagnosticI11.run_id, @($diagnosticI11.stage_exit_codes).Count, @($diagnosticI11.declared_input_scope.source_snapshot_roots).Count)
    $observedPaths = @($diagnosticI11.observed_input_changes.source_snapshot.modified_paths)
    Assert-Case -Id 'I13c-observed-changes-not-write-history' -Condition (
      [string]$diagnosticI11.observed_input_changes.evidence_kind -eq 'observed-between-two-samples' -and
      (Test-IdentityFalseValue -Value $diagnosticI11.observed_input_changes.is_complete_write_history) -and
      ($observedPaths -contains "$(Get-RelFixturePath $mutating.sourceRoot)/crate/src/lib.rs")) `
      -Detail ("evidence_kind={0} is_complete_write_history={1} modified={2}" -f $diagnosticI11.observed_input_changes.evidence_kind, $diagnosticI11.observed_input_changes.is_complete_write_history, ($observedPaths -join ','))
    $classified = @($diagnosticI11.observed_input_changes.classification)
    $inScopeEntry = @($classified | Where-Object { $_.path -match 'crate/src/lib\.rs' })[0]
    Assert-Case -Id 'I13d-change-classified-in-declared-scope' -Condition ($inScopeEntry -and [string]$inScopeEntry.classification -eq 'in-declared-source-snapshot-scope') `
      -Detail ("逐路径分类可审查：{0} -> {1}（{2}）" -f $inScopeEntry.path, $inScopeEntry.classification, $inScopeEntry.matched_rule)
    $quarantine = $diagnosticI11.produced_not_released.quarantine
    Assert-Case -Id 'I13e-temporary-products-partitioned' -Condition (
      [string]$diagnosticI11.produced_not_released.partition -eq 'failure-quarantine' -and
      [string]$diagnosticI11.produced_not_released.auto_promotion -eq 'never' -and
      (Test-IdentityFalseValue -Value $diagnosticI11.produced_not_released.release_eligible) -and
      $quarantine.moved -eq $true -and
      [string]$quarantine.destination_relative -and
      ([string]$quarantine.destination_relative) -notmatch 'package$') `
      -Detail ("暂存产物移入失败隔离区（与可发布产物分区、不自动晋升）：{0}（files={1}）" -f $quarantine.destination_relative, $quarantine.moved_file_count)
    Assert-Case -Id 'I13f-diagnostic-says-signing-not-attempted' -Condition ([string]$diagnosticI11.signing_or_distribution -eq 'not-attempted') `
      -Detail '失败运行不进入签名/安装/分发'
    Assert-Case -Id 'I13g-diagnostic-declares-no-overwrite-no-pointer-update' -Condition (
      (Test-IdentityFalseValue -Value $diagnosticI11.previous_success_report.overwritten_by_this_run) -and
      (Test-IdentityFalseValue -Value $diagnosticI11.previous_success_report.pointer_updated_by_this_run) -and
      (Test-IdentityFalseValue -Value $diagnosticI11.latest_valid_package_updated)) `
      -Detail ("失败诊断显式声明：未覆盖上一份成功报告（overwritten={0}）、未更新 latest 指针（pointer_updated={1}）" -f $diagnosticI11.previous_success_report.overwritten_by_this_run, $diagnosticI11.previous_success_report.pointer_updated_by_this_run)
    Assert-Case -Id 'I13h-diagnostic-content-hash-verifiable' -Condition (
      (Assert-PackageReportContentHash -Report $diagnosticI11).verified -eq $true) `
      -Detail ("失败诊断同样受报告身份治理（内容哈希可重算）：{0}" -f $diagnosticI11.report_identity.content_sha256)
    $diagnosticRefused = $false
    try {
      [void](Assert-PackageReportReleaseEligible -Report $diagnosticI11 -Purpose 'contract-test')
    } catch {
      $diagnosticRefused = ($_.Exception.Message -match 'REPORT-NOT-RELEASE-ELIGIBLE')
    }
    Assert-Case -Id 'I13i-failure-diagnostic-not-a-release-receipt' -Condition $diagnosticRefused `
      -Detail '失败诊断必须被发布侧门禁拒绝（不得当成发布收据/发布来源）'
  }

  # ---- 稳定条件：出可发布收据（构建模式 + --no-build 都要能回答"能不能发布"）----
  $releaseStable = New-ReleasePolicyCase -Name 'release-stable'
  & $packageScript -Manifest $releaseStable.manifestPath -Configuration debug -ReportPath $releaseStable.reportPath -PackageRoot $releaseStable.packagePath | Out-Null
  $stableReport = Get-Content -Raw -LiteralPath $releaseStable.reportPath -Encoding UTF8 | ConvertFrom-Json
  $stableGates = @($stableReport.release_eligibility.gates)
  Assert-Case -Id 'I13j-stable-run-is-release-eligible' -Condition (
    $stableReport.release_eligible -eq $true -and
    @($stableGates | Where-Object { $_.result -ne 'pass' }).Count -eq 0) `
    -Detail ("稳定输入 ⇒ 可发布收据：release_eligible={0}（gates: {1}）" -f $stableReport.release_eligible, (@($stableGates | ForEach-Object { $_.gate + '=' + $_.result }) -join ','))
  Assert-Case -Id 'I13k-state-fields-are-separate' -Condition (
    $stableReport.release_eligibility.live_worktree_changed -eq $false -and
    [string]$stableReport.release_eligibility.build_snapshot_integrity -eq 'verified-unchanged-live-tree' -and
    [string]$stableReport.release_eligibility.validation_snapshot_digest -eq [string]$stableReport.source_snapshot.post_build_verification.post_build_digest -and
    [string]$stableReport.release_eligibility.build_input_digest -eq [string]$stableReport.build_identity.build_input_digest -and
    [string]$stableReport.run_id) `
    -Detail ("状态分开表达：live_worktree_changed={0} build_snapshot_integrity={1} validation_snapshot_digest 与 post_build_digest 一致" -f $stableReport.release_eligibility.live_worktree_changed, $stableReport.release_eligibility.build_snapshot_integrity)
  Assert-Case -Id 'I13l-immutable-snapshot-claim-is-honest' -Condition (
    (Test-IdentityFalseValue -Value $stableReport.release_eligibility.immutable_build_snapshot) -and
    ([string]$stableReport.release_eligibility.immutable_build_snapshot_note).Length -gt 10) `
    -Detail '不冒充"从不可变快照构建"：本实现的稳定证据是前后两次采样逐文件一致'
  Assert-Case -Id 'I13m-stable-run-writes-pointer-and-status' -Condition (
    (Test-Path -LiteralPath $releaseStable.pointerPath -PathType Leaf) -and
    [string]$stableReport.report_identity.report_id) `
    -Detail '成功运行才更新 latest 指针'

  # ---- --no-build：必须消费"匹配快照"的关联记录 ----
  & $packageScript -Manifest $releaseStable.manifestPath -Configuration debug -ReportPath $releaseStable.reportPath -PackageRoot $releaseStable.packagePath -SkipBuild | Out-Null
  $noBuildReport = Get-Content -Raw -LiteralPath $releaseStable.reportPath -Encoding UTF8 | ConvertFrom-Json
  $noBuildProvenance = @($noBuildReport.release_eligibility.artifact_provenance)
  Assert-Case -Id 'I13n-no-build-requires-snapshot-association' -Condition (
    $noBuildProvenance.Count -ge 1 -and
    [string]$noBuildProvenance[0].confirmation -eq 'confirmed-by-association' -and
    [string]$noBuildProvenance[0].association.source_snapshot_digest -eq [string]$noBuildReport.release_eligibility.source_snapshot_digest -and
    $noBuildReport.release_eligible -eq $true) `
    -Detail ("--no-build 消费绑定到本次快照摘要的关联记录：artifact={0} confirmation={1} release_eligible={2}" -f $noBuildProvenance[0].artifact_id, $noBuildProvenance[0].confirmation, $noBuildReport.release_eligible)

  # ---- 变化条件（构建期间改写**声明的构建输入**）：拒绝发布 + 失败诊断 ----
  $releaseInputChange = New-ReleasePolicyCase -Name 'release-build-input-change' -MutateBuildInput
  $inputChangeError = ''
  try {
    & $packageScript -Manifest $releaseInputChange.manifestPath -Configuration debug -ReportPath $releaseInputChange.reportPath -PackageRoot $releaseInputChange.packagePath | Out-Null
  } catch {
    $inputChangeError = $_.Exception.Message
  }
  Assert-Case -Id 'I13o-declared-build-input-change-refused' -Condition ($inputChangeError -match 'BUILD-INPUT-CHANGED') `
    -Detail ("构建期间声明的构建输入变化 ⇒ 拒绝发布：{0}" -f (($inputChangeError -split "`n")[0]))
  $inputChangeDiagnostic = Get-ReleasePolicyFailureDiagnostic -Case $releaseInputChange
  Assert-Case -Id 'I13p-diagnostic-records-build-input-change' -Condition (
    $null -ne $inputChangeDiagnostic -and
    @($inputChangeDiagnostic.observed_input_changes.build_inputs.changed_descriptor_lines).Count -ge 1 -and
    @($inputChangeDiagnostic.release_eligibility.gates | Where-Object { $_.gate -eq 'build_input_stability' -and $_.result -eq 'fail' }).Count -eq 1 -and
    (Test-IdentityFalseValue -Value $inputChangeDiagnostic.release_eligible)) `
    -Detail ("失败诊断记录已观察到的构建输入变化：{0} 条描述符差异；build_input_stability=fail" -f $(if ($inputChangeDiagnostic) { @($inputChangeDiagnostic.observed_input_changes.build_inputs.changed_descriptor_lines).Count } else { 0 }))
  $revoked = @($inputChangeDiagnostic.artifact_revocations)
  Assert-Case -Id 'I13q-mixed-input-artifact-is-revoked' -Condition ($revoked.Count -ge 1 -and [string]$revoked[0].artifact_id -eq 'rd406.payload') `
    -Detail ("观察到的输入变化 ⇒ 产物内容身份登记为不可发布来源（revoked={0}）" -f $revoked.Count)
  $revokedNoBuildError = ''
  try {
    & $packageScript -Manifest $releaseInputChange.manifestPath -Configuration debug -ReportPath $releaseInputChange.reportPath -PackageRoot $releaseInputChange.packagePath -SkipBuild | Out-Null
  } catch {
    $revokedNoBuildError = $_.Exception.Message
  }
  Assert-Case -Id 'I13r-no-build-refuses-revoked-artifact' -Condition ($revokedNoBuildError -match 'ARTIFACT-REVOKED') `
    -Detail ("--no-build 不得把混合输入产物当来源：{0}" -f (($revokedNoBuildError -split "`n")[0]))
  Assert-Case -Id 'I13s-no-success-report-on-refused-run' -Condition (-not (Test-Path -LiteralPath $releaseInputChange.reportPath -PathType Leaf)) `
    -Detail '被拒绝的运行不得在成功报告落点留下文件（诊断在 failures/ 分区，收据不发）'

  # ---- 失败运行不得覆盖上一份成功报告 / 不得更新 latest 指针（同目录真实序列）----
  # 基线必须紧贴失败运行之前取：中间的 --no-build 运行会（合法地）更新它们。
  $previousReportHash = Get-IdentityFileHash -Path $releaseStable.reportPath
  $previousPointerHash = Get-IdentityFileHash -Path $releaseStable.pointerPath
  $stableSourcePayload = Join-Path $releaseStable.sourceRoot 'payload/second.txt'
  $payloadBackup = Join-Path $releaseStable.caseRoot 'second.txt.bak'
  Copy-Item -LiteralPath $stableSourcePayload -Destination $payloadBackup -Force
  Remove-Item -LiteralPath $stableSourcePayload -Force
  $missingSourceError = ''
  try {
    & $packageScript -Manifest $releaseStable.manifestPath -Configuration debug -ReportPath $releaseStable.reportPath -PackageRoot $releaseStable.packagePath | Out-Null
  } catch {
    $missingSourceError = $_.Exception.Message
  }
  Assert-Case -Id 'I13t-failed-run-does-not-overwrite-previous-success-report' -Condition (
    ($missingSourceError -match 'SOURCE-MISSING') -and
    ((Get-IdentityFileHash -Path $releaseStable.reportPath) -eq $previousReportHash) -and
    ((Get-IdentityFileHash -Path $releaseStable.pointerPath) -eq $previousPointerHash)) `
    -Detail '失败运行没有改写上一份成功报告、也没有改写 latest 指针引用的报告字节'
  $stableDiagnostic = Get-ReleasePolicyFailureDiagnostic -Case $releaseStable
  Assert-Case -Id 'I13u-failed-run-diagnostic-and-empty-package-root' -Condition (
    $null -ne $stableDiagnostic -and
    (Test-IdentityFalseValue -Value $stableDiagnostic.release_eligible) -and
    $stableDiagnostic.produced_not_released.quarantine.moved -eq $true -and
    @(Get-ChildItem -LiteralPath $releaseStable.packagePath -Recurse -File -Force -ErrorAction SilentlyContinue).Count -eq 0) `
    -Detail '失败运行的暂存产物已移入隔离区，包根不再残留"看起来像可发布包"的内容'
  Copy-Item -LiteralPath $payloadBackup -Destination $stableSourcePayload -Force
  Remove-Item -LiteralPath $payloadBackup -Force

  # ---- 未声明源码快照范围 ⇒ "未能确认"，不乐观放行 ----
  $undeclaredScope = New-ReleasePolicyCase -Name 'release-no-declared-scope' -OmitSourceSnapshotScope
  & $packageScript -Manifest $undeclaredScope.manifestPath -Configuration debug -ReportPath $undeclaredScope.reportPath -PackageRoot $undeclaredScope.packagePath | Out-Null
  $undeclaredReport = Get-Content -Raw -LiteralPath $undeclaredScope.reportPath -Encoding UTF8 | ConvertFrom-Json
  Assert-Case -Id 'I13v-undeclared-scope-is-not-optimistically-eligible' -Condition (
    (Test-IdentityFalseValue -Value $undeclaredReport.release_eligible) -and
    [string]$undeclaredReport.release_eligibility.live_worktree_changed -eq 'not-confirmed' -and
    @($undeclaredReport.release_eligibility.gates | Where-Object { $_.gate -eq 'declared_source_snapshot_scope' -and $_.result -eq 'not-confirmed' }).Count -eq 1) `
    -Detail ("未声明源码快照范围：release_eligible={0} live_worktree_changed={1}（未确认不得写成 false）" -f $undeclaredReport.release_eligible, $undeclaredReport.release_eligibility.live_worktree_changed)

  # ---- 范围规则本身要可审查：声明路径的分类 ----
  . (Join-Path $workspaceFull 'scripts/lib/build-identity.ps1')
  $classificationScope = Get-SourceSnapshotScope -ManifestData (Get-Content -Raw -LiteralPath $releaseStable.manifestPath -Encoding UTF8 | ConvertFrom-Json)
  $nonBuildInputPatterns = @(Get-PackageNonBuildInputDeclarations -ManifestData (Get-Content -Raw -LiteralPath $releaseStable.manifestPath -Encoding UTF8 | ConvertFrom-Json) | ForEach-Object { $_.pattern })
  $declaredBuildInputs = @((Get-RelFixturePath (Join-Path $releaseStable.caseRoot 'build-inputs/toolchain.json')))
  $inScopeClassification = Get-PackagePathClassification -Path ((Get-RelFixturePath $releaseStable.sourceRoot) + '/crate/src/lib.rs') -Scope $classificationScope -BuildInputFiles $declaredBuildInputs -NonBuildInputPatterns $nonBuildInputPatterns
  Assert-Case -Id 'I13w-in-scope-path-classified-correctly' -Condition ([string]$inScopeClassification.classification -eq 'in-declared-source-snapshot-scope') `
    -Detail ("声明 root 内的文件判为 in-declared-source-snapshot-scope（root 相对匹配，不因工作区位于 tmp/ 之下而整体失效）：{0}" -f $inScopeClassification.matched_rule)
  $declaredInputClassification = Get-PackagePathClassification -Path $declaredBuildInputs[0] -Scope $classificationScope -BuildInputFiles $declaredBuildInputs -NonBuildInputPatterns $nonBuildInputPatterns
  Assert-Case -Id 'I13x-declared-build-input-classified-correctly' -Condition ([string]$declaredInputClassification.classification -eq 'declared-build-input') `
    -Detail ("显式声明的构建输入（即使位于 tmp/ 之下）判为 declared-build-input：{0}" -f $declaredInputClassification.matched_rule)
  $logClassification = Get-PackagePathClassification -Path 'tmp/rd406-fixture-logs/runtime.log' -Scope $classificationScope -BuildInputFiles $declaredBuildInputs -NonBuildInputPatterns $nonBuildInputPatterns
  Assert-Case -Id 'I13y-non-build-input-classified-correctly' -Condition ([string]$logClassification.classification -eq 'declared-non-build-input') `
    -Detail ("已声明不属于构建输入的日志路径判为 declared-non-build-input（不误判为源码变化）：{0}" -f $logClassification.matched_rule)
  $outsideClassification = Get-PackagePathClassification -Path 'modules/whatever/src/lib.rs' -Scope $classificationScope -BuildInputFiles $declaredBuildInputs -NonBuildInputPatterns $nonBuildInputPatterns
  Assert-Case -Id 'I13z-outside-scope-classified-correctly' -Condition ([string]$outsideClassification.classification -eq 'outside-declared-scope') `
    -Detail ("不在声明 roots 下的路径判为 outside-declared-scope：{0}" -f $outsideClassification.matched_rule)

  # ==================================================== I12 报告保留规则 ==
  $retentionRoot = Join-Path $success.caseRoot 'retention'
  $retentionArchive = Join-Path $success.caseRoot 'archive'
  New-Item -ItemType Directory -Force -Path $retentionRoot | Out-Null
  $retentionReport = Join-Path $retentionRoot 'package-report-debug-20990101-000000000-00000000.json'
  Copy-Item -LiteralPath $success.reportPath -Destination $retentionReport -Force
  # 一份未被任何发布引用的报告：Prune 应当删掉它。
  # 注意不能用受保护报告的字节副本——那在"文件哈希"层面就是同一个证据。
  $unprotectedReport = Join-Path $retentionRoot 'package-report-debug-20990102-000000000-11111111.json'
  Write-FixtureText -Path $unprotectedReport -Text ('{"kind":"package-report","report_identity":{"schema":1,"report_id":"pkg-report-debug-20990102-000000000-11111111","content_sha256":"' + ('0' * 64) + '"},"note":"not referenced by any release"}')

  $protectResult = & $retentionScript -Action Protect -ReportPath $retentionReport -ReleaseVersion '9.9.9' -ReleaseArtifact 'dist/fixture.msi' -Reason 'contract test' -Archive -ArchiveRoot $retentionArchive -ReportRoot $retentionRoot
  $protectResult = @($protectResult) | Select-Object -Last 1
  Assert-Case -Id 'I12a-protect-records-identity' -Condition (
    [string]$protectResult.report_id -eq [string]$report.report_identity.report_id -and
    [string]$protectResult.report_content_sha256 -eq [string]$report.report_identity.content_sha256) `
    -Detail ("保留索引登记报告唯一 ID + 内容哈希：{0}" -f $protectResult.report_id)

  $archivedReport = Join-Path $retentionArchive 'release-9.9.9/evidence/build-identity'
  $archiveExists = @(Get-ChildItem -LiteralPath $archivedReport -Recurse -File -ErrorAction SilentlyContinue).Count -gt 0
  Assert-Case -Id 'I12b-archive-carries-release-evidence' -Condition $archiveExists `
    -Detail ("证据连同发布归档：{0}" -f (Get-RelFixturePath $archivedReport))

  $verifyOk = $false
  $verifyError = ''
  try {
    [void](& $retentionScript -Action Verify -ReportRoot $retentionRoot)
    $verifyOk = $true
  } catch {
    $verifyError = $_.Exception.Message
  }
  Assert-Case -Id 'I12c-verify-passes' -Condition $verifyOk -Detail ("Verify 通过：{0}" -f $(if ($verifyOk) { 'protected report verified' } else { $verifyError }))

  # Prune 先做：此时受保护报告仍在 tmp 下 —— 必须"删未受保护的、留受保护的"。
  $pruneResult = & $retentionScript -Action Prune -ReportRoot $retentionRoot
  $pruneResult = @($pruneResult) | Select-Object -Last 1
  Assert-Case -Id 'I12e-prune-keeps-protected' -Condition (
    -not (Test-Path -LiteralPath $unprotectedReport -PathType Leaf) -and
    (Test-Path -LiteralPath $retentionReport -PathType Leaf) -and
    @($pruneResult.kept).Count -ge 1 -and
    @($pruneResult.removed) -contains 'package-report-debug-20990102-000000000-11111111.json') `
    -Detail ("Prune 只删除未被索引的报告：kept={0} removed={1}" -f (@($pruneResult.kept) -join '; '), (@($pruneResult.removed) -join '; '))

  # 再删掉 tmp 下的原报告：发布证据归档必须能顶上（普通临时清理不得删除唯一证据）。
  Remove-Item -LiteralPath $retentionReport -Force
  $verifyAfterDelete = $false
  try {
    [void](& $retentionScript -Action Verify -ReportRoot $retentionRoot)
    $verifyAfterDelete = $true
  } catch {
    $verifyAfterDelete = $false
  }
  Assert-Case -Id 'I12d-archive-is-fallback-when-original-cleaned' -Condition $verifyAfterDelete `
    -Detail 'tmp 下的原报告被清理后，Verify 仍能从发布证据归档核对（普通临时清理不得删除唯一证据）'

  $pruneResult = & $retentionScript -Action Prune -ReportRoot $retentionRoot
  $pruneResult = @($pruneResult) | Select-Object -Last 1
  Assert-Case -Id 'I12g-prune-after-cleanup-keeps-nothing-else' -Condition (
    @($pruneResult.removed).Count -eq 0) `
    -Detail ("原报告被清理后 Prune 不再删任何东西（受保护 ID 仍在索引里）：removed={0} kept={1}" -f (@($pruneResult.removed) -join '; '), (@($pruneResult.kept) -join '; '))

  $indexPath = Join-Path $retentionRoot 'retention-index.json'
  [System.IO.File]::WriteAllText($indexPath, '{ this is not json', [System.Text.UTF8Encoding]::new($false))
  $pruneRefused = $false
  $pruneRefusedMessage = ''
  try {
    [void](& $retentionScript -Action Prune -ReportRoot $retentionRoot)
  } catch {
    $pruneRefused = ($_.Exception.Message -match 'RETENTION-INDEX-INVALID')
    $pruneRefusedMessage = ($_.Exception.Message -split "`n")[0]
  }
  Assert-Case -Id 'I12f-prune-refused-on-broken-index' -Condition $pruneRefused `
    -Detail ("索引损坏时拒绝清理（fail-closed）：{0}" -f $pruneRefusedMessage)

  # ---- 失败诊断 / release_eligible=false 的报告不得被 Protect 成发布证据 ----
  $retentionFailuresRoot = Join-Path $success.caseRoot 'retention-failures'
  New-Item -ItemType Directory -Force -Path $retentionFailuresRoot | Out-Null
  $protectDiagnosticError = ''
  if ($stableDiagnostic) {
    $diagnosticCopyPath = Join-Path $retentionFailuresRoot (([System.IO.Path]::GetFileName((Get-ChildItem -LiteralPath $releaseStable.failureRoot -Filter '*.json' -File | Select-Object -First 1).FullName)))
    Copy-Item -LiteralPath (Get-ChildItem -LiteralPath $releaseStable.failureRoot -Filter '*.json' -File | Select-Object -First 1).FullName -Destination $diagnosticCopyPath -Force
    try {
      [void](& $retentionScript -Action Protect -ReportPath $diagnosticCopyPath -ReleaseVersion '9.9.9' -ReleaseArtifact 'dist/fixture.msi' -ReportRoot $retentionFailuresRoot)
    } catch {
      $protectDiagnosticError = $_.Exception.Message
    }
  }
  Assert-Case -Id 'I12h-protect-refuses-failure-diagnostic' -Condition ($protectDiagnosticError -match 'REPORT-NOT-RELEASE-ELIGIBLE') `
    -Detail ("失败诊断不得被登记为发布证据（否则等于给混合输入产物补一张收据）：{0}" -f (($protectDiagnosticError -split "`n")[0]))

  $protectIneligibleError = ''
  $ineligibleReportPath = Join-Path $retentionFailuresRoot 'ineligible-report.json'
  if (Test-Path -LiteralPath $undeclaredScope.reportPath -PathType Leaf) {
    Copy-Item -LiteralPath $undeclaredScope.reportPath -Destination $ineligibleReportPath -Force
    try {
      [void](& $retentionScript -Action Protect -ReportPath $ineligibleReportPath -ReleaseVersion '9.9.9' -ReleaseArtifact 'dist/fixture.msi' -ReportRoot $retentionFailuresRoot)
    } catch {
      $protectIneligibleError = $_.Exception.Message
    }
  }
  Assert-Case -Id 'I12i-protect-refuses-release-ineligible-report' -Condition ($protectIneligibleError -match 'REPORT-NOT-RELEASE-ELIGIBLE') `
    -Detail ("release_eligible=false 的报告不得被登记为发布证据：{0}" -f (($protectIneligibleError -split "`n")[0]))
  # ==========================================================================
  # RD4-06 准备/冻结分离（裁决 §3.1）与"失败报告不得当作发布收据"（§3.3 验收六）
  #
  # 覆盖：
  #   I14a 失败运行后 --no-build 既没有有效发布收据、也拒绝失败运行的产物身份
  #   I14b 失败报告本身被发布侧门禁拒绝，且 --no-build 也只能产出失败诊断
  #   I15a 准备阶段：解析依赖 + 生成配置 + 展示变化 + 冻结输入快照（不构建、不产出包）
  #   I15b 准备阶段不执行 artifact 构建
  #   I15c 正式构建阶段消费冻结记录（gate=frozen_inputs pass，freeze_id 对齐）
  #   I15d 冻结输入在构建阶段改变 ⇒ 拒绝发布 + 诊断列出改变路径/前后摘要/发生阶段
  #   I15e 准备阶段生成的配置计入冻结输入，构建阶段改写即拒绝
  #   I15f 准备阶段发现输入变化必须显式确认（未确认即拒绝，不给"删减清单换绿色"的机会）
  # ==========================================================================

  # ---- I14：失败报告不能被执行 --no-build 当作有效发布收据（验收六）----
  $failForNoBuild = New-ReleasePolicyCase -Name 'no-build-after-failure' -MutateBuildInput
  $failError = ''
  try {
    & $packageScript -Manifest $failForNoBuild.manifestPath -Configuration debug -ReportPath $failForNoBuild.reportPath -PackageRoot $failForNoBuild.packagePath | Out-Null
  } catch {
    $failError = $_.Exception.Message
  }
  $noBuildRetryError = ''
  try {
    & $packageScript -Manifest $failForNoBuild.manifestPath -Configuration debug -ReportPath $failForNoBuild.reportPath -PackageRoot $failForNoBuild.packagePath -SkipBuild | Out-Null
  } catch {
    $noBuildRetryError = $_.Exception.Message
  }
  $failDiagnostics = @(Get-ChildItem -LiteralPath (Join-Path $failForNoBuild.caseRoot 'failures') -Filter 'package-report-failure-*.json' -File -ErrorAction SilentlyContinue)
  $failDiagnosticDoc = $null
  if ($failDiagnostics.Count -gt 0) {
    $failDiagnosticDoc = Get-Content -Raw -LiteralPath $failDiagnostics[0].FullName -Encoding UTF8 | ConvertFrom-Json
  }
  Assert-Case -Id 'I14a-no-build-has-no-valid-release-receipt-after-failure' -Condition (
    ($failError -match 'BUILD-INPUT-CHANGED') -and
    (-not (Test-Path -LiteralPath $failForNoBuild.reportPath -PathType Leaf)) -and
    ($null -ne $failDiagnosticDoc) -and
    [string]$failDiagnosticDoc.kind -eq 'package-report-failure') `
    -Detail '失败运行不产出成功报告（因此 --no-build 没有可消费的有效发布收据），只有失败诊断'
  Assert-Case -Id 'I14b-no-build-refuses-failed-run-artifact' -Condition ($noBuildRetryError -match 'ARTIFACT-REVOKED') `
    -Detail ("--no-build 拒绝失败运行登记为禁用的产物内容身份：{0}" -f (($noBuildRetryError -split "`n")[0]))
  $noBuildRetryDiagnostics = @(Get-ChildItem -LiteralPath (Join-Path $failForNoBuild.caseRoot 'failures') -Filter 'package-report-failure-*.json' -File -ErrorAction SilentlyContinue)
  Assert-Case -Id 'I14c-no-build-failure-also-writes-diagnostic' -Condition ($noBuildRetryDiagnostics.Count -ge 2) `
    -Detail ("被拒绝的 --no-build 运行同样只产出失败诊断（诊断数 {0}：首次失败 {1}）" -f $noBuildRetryDiagnostics.Count, $(if ($failDiagnosticDoc) { $failDiagnosticDoc.report_identity.report_id } else { '(none)' }))

  # ---- I15：准备/冻结分离 ----
  function New-PrepareFreezeCase {
    param(
      [Parameter(Mandatory = $true)][string]$Name,
      [switch]$WithGeneratedConfig,
      [switch]$WithBuildMarker
    )

    $caseRootLocal = Join-Path $fixtureRoot $Name
    if (Test-Path -LiteralPath $caseRootLocal) { Remove-Item -LiteralPath $caseRootLocal -Recurse -Force }
    $sourceRootLocal = Join-Path $caseRootLocal 'source'
    New-Item -ItemType Directory -Force -Path $sourceRootLocal | Out-Null
    Write-FixtureText -Path (Join-Path $sourceRootLocal 'Cargo.toml') -Text "[workspace]`nmembers = []`n"
    Write-FixtureText -Path (Join-Path $sourceRootLocal 'crate/src/lib.rs') -Text "pub fn answer() -> u32 { 42 }`n"
    Write-FixtureText -Path (Join-Path $sourceRootLocal 'payload/payload.txt') -Text ("payload-" + $Name + "`n")
    Write-FixtureText -Path (Join-Path $caseRootLocal 'build-inputs/toolchain.json') -Text '{"toolchain":"v1"}'

    $markerPath = Join-Path $caseRootLocal 'build-ran.marker'
    $artifact = [ordered]@{
      id = 'rd406pf.payload'
      source = (Get-RelFixturePath (Join-Path $sourceRootLocal 'payload/payload.txt'))
      target = 'bin/payload.txt'
    }
    if ($WithBuildMarker) {
      $artifact.build = [ordered]@{
        command = 'powershell'
        args = @('-NoProfile', '-Command', ("New-Item -ItemType File -Force -Path '{0}' | Out-Null" -f $markerPath))
        working_dir = '.'
      }
    }

    $preparation = [ordered]@{ steps = @(); derived_outputs = @() }
    $generatedRelative = $null
    if ($WithGeneratedConfig) {
      $generatedPath = Join-Path $caseRootLocal 'generated/toolchain.json'
      $generatedRelative = (Get-RelFixturePath $generatedPath)
      $generatorScript = Join-Path $caseRootLocal 'generate-config.ps1'
      Write-FixtureText -Path $generatorScript -Text (
        "param([string]`$Target)`n" +
        "[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName(`$Target)) | Out-Null`n" +
        "[System.IO.File]::WriteAllText(`$Target, '{`"toolchain`":`"generated-v1`"}' + [System.Environment]::NewLine)`n")
      $preparation.steps = @([ordered]@{
          id = 'generate-toolchain-config'
          command = 'powershell'
          args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $generatorScript, $generatedPath)
          working_dir = '.'
          generates = @($generatedRelative)
        })
    }
    $preparation.derived_outputs = @([ordered]@{
        path = (Get-RelFixturePath (Join-Path $caseRootLocal 'derived'))
        generated_by = 'contract-test fixture'
        reason = 'declared derived output location for the RD4-06 contract test'
        within_declared_output_root = $true
      })

    $manifestLocal = [ordered]@{
      package_root = (Get-RelFixturePath (Join-Path $caseRootLocal 'package'))
      backup_keep = 2
      artifacts = @($artifact)
      resources = @()
      preparation = $preparation
      build_inputs = [ordered]@{ files = @((Get-RelFixturePath (Join-Path $caseRootLocal 'build-inputs/toolchain.json'))) }
      source_snapshot = [ordered]@{
        schema = 1
        roots = @((Get-RelFixturePath $sourceRootLocal))
        allow_path_patterns = @()
        extra_exclude_path_patterns = @()
        allow_reparse_points = @()
        external_path_dependencies = @()
      }
    }
    $manifestPathLocal = Join-Path $caseRootLocal 'manifest.json'
    Write-FixtureText -Path $manifestPathLocal -Text ($manifestLocal | ConvertTo-Json -Depth 12)
    return [pscustomobject]@{
      name = $Name
      caseRoot = $caseRootLocal
      sourceRoot = $sourceRootLocal
      manifestPath = $manifestPathLocal
      packagePath = Join-Path $caseRootLocal 'package'
      reportPath = Join-Path $caseRootLocal 'report.json'
      markerPath = $markerPath
      generatedPath = $(if ($generatedRelative) { Join-Path $workspaceFull $generatedRelative } else { $null })
      toolchainPath = Join-Path $caseRootLocal 'build-inputs/toolchain.json'
      prepareRoot = Join-Path $caseRootLocal 'prepare'
    }
  }

  function Invoke-PrepareRun {
    param([object]$Case, [switch]$AcceptChanges, [switch]$ExpectFailure)
    $errorText = ''
    try {
      & $packageScript -Manifest $Case.manifestPath -Configuration debug -ReportPath $Case.reportPath -Prepare -AcceptPreparationChanges:$AcceptChanges | Out-Null
    } catch {
      $errorText = $_.Exception.Message
    }
    return $errorText
  }

  function Get-LatestPrepareRecord {
    param([object]$Case)
    $files = @(Get-ChildItem -LiteralPath $Case.prepareRoot -Filter 'package-prepare-debug-*.json' -File -ErrorAction SilentlyContinue | Sort-Object Name -Descending)
    if ($files.Count -eq 0) { return $null }
    return (Get-Content -Raw -LiteralPath $files[0].FullName -Encoding UTF8 | ConvertFrom-Json)
  }

  # ---- 准备阶段：冻结输入快照，且不构建、不产出可发布包 ----
  $prepareCase = New-PrepareFreezeCase -Name 'prepare-freeze-basic' -WithBuildMarker
  $prepareError = Invoke-PrepareRun -Case $prepareCase
  $prepareRecord = Get-LatestPrepareRecord -Case $prepareCase
  $prepareFreezePointer = Join-Path $workspaceFull 'tmp/package-freeze/latest-debug.json'
  $preparePointerDoc = $(if (Test-Path -LiteralPath $prepareFreezePointer -PathType Leaf) { Get-Content -Raw -LiteralPath $prepareFreezePointer -Encoding UTF8 | ConvertFrom-Json } else { $null })
  $prepareFreezeDoc = $null
  if ($prepareRecord -and $prepareRecord.freeze -and $prepareRecord.freeze.path) {
    $prepareFreezeFullPath = Join-Path $workspaceFull ([string]$prepareRecord.freeze.path).Replace('/', '\')
    if (Test-Path -LiteralPath $prepareFreezeFullPath -PathType Leaf) {
      $prepareFreezeDoc = Get-Content -Raw -LiteralPath $prepareFreezeFullPath -Encoding UTF8 | ConvertFrom-Json
    }
  }
  Assert-Case -Id 'I15a-prepare-freezes-inputs' -Condition (
    -not $prepareError -and
    ($null -ne $prepareRecord) -and
    [string]$prepareRecord.kind -eq 'package-preparation-record' -and
    [string]$prepareRecord.report_status -eq 'prepared' -and
    [string]$prepareRecord.run_phase -eq 'prepare' -and
    (Test-IdentityFalseValue -Value $prepareRecord.release_eligible) -and
    ($null -ne $prepareFreezeDoc) -and
    [string]$prepareFreezeDoc.kind -eq 'package-input-freeze' -and
    @($prepareFreezeDoc.build_input_descriptor_lines).Count -gt 0 -and
    ($null -ne $preparePointerDoc) -and
    [string]$preparePointerDoc.freeze_id -eq [string]$prepareFreezeDoc.freeze_id) `
    -Detail ("准备阶段解析依赖 + 冻结输入快照：freeze_id={0} 冻结输入 {1} 项；准备记录不是发布收据（release_eligible=false）" -f $prepareFreezeDoc.freeze_id, @($prepareFreezeDoc.build_input_descriptor_lines).Count)
  Assert-Case -Id 'I15b-prepare-does-not-build' -Condition (
    (-not (Test-Path -LiteralPath $prepareCase.markerPath)) -and
    (-not (Test-Path -LiteralPath $prepareCase.reportPath)) -and
    (-not (Test-Path -LiteralPath (Join-Path $prepareCase.packagePath 'bin'))) ) `
    -Detail '准备阶段不执行 artifact 构建、不写成功报告、不在包根产出任何内容'
  Assert-Case -Id 'I15b2-prepare-records-cargo-lock-semantics' -Condition (
    ($null -ne $prepareRecord.dependency_resolution) -and
    [string]$prepareRecord.dependency_resolution.exit_code -eq '0' -and
    [string]$prepareRecord.dependency_resolution.mode -eq 'read-only-locked-resolution-check' -and
    [string]$prepareRecord.cargo_lock_semantics.build -match '--locked' -and
    [string]$prepareRecord.cargo_lock_semantics.no_whitelist -match '白名单') `
    -Detail ("准备阶段做只读锁定解析核对（{0}, exit={1}）；正式构建锁定语义与『无白名单』声明同时登记" -f $prepareRecord.dependency_resolution.command, $prepareRecord.dependency_resolution.exit_code)

  # ---- 正式构建阶段消费冻结记录 ----
  $frozenBuildError = ''
  try {
    & $packageScript -Manifest $prepareCase.manifestPath -Configuration debug -ReportPath $prepareCase.reportPath -PackageRoot $prepareCase.packagePath -FreezeRecordPath ([string]$prepareRecord.freeze.path) | Out-Null
  } catch {
    $frozenBuildError = $_.Exception.Message
  }
  $frozenBuildReport = $(if (Test-Path -LiteralPath $prepareCase.reportPath -PathType Leaf) { Get-Content -Raw -LiteralPath $prepareCase.reportPath -Encoding UTF8 | ConvertFrom-Json } else { $null })
  $frozenGate = $(if ($frozenBuildReport) { @($frozenBuildReport.release_eligibility.gates | Where-Object { $_.gate -eq 'frozen_inputs' })[0] } else { $null })
  Assert-Case -Id 'I15c-build-consumes-frozen-record' -Condition (
    -not $frozenBuildError -and
    ($null -ne $frozenBuildReport) -and
    [string]$frozenBuildReport.run_phase -eq 'build' -and
    [string]$frozenBuildReport.preparation_and_freeze.frozen_inputs_mode -eq 'consumed-preparation-freeze-record' -and
    [string]$frozenBuildReport.preparation_and_freeze.freeze_id -eq [string]$prepareFreezeDoc.freeze_id -and
    ($null -ne $frozenGate) -and [string]$frozenGate.result -eq 'pass' -and
    (Test-Path -LiteralPath $prepareCase.markerPath) -and
    $frozenBuildReport.release_eligible -eq $true) `
    -Detail ("正式构建阶段消费冻结记录：gate frozen_inputs={0} freeze_id={1}；构建确实执行（marker 存在）" -f $(if ($frozenGate) { $frozenGate.result } else { '(missing)' }), $frozenBuildReport.preparation_and_freeze.freeze_id)

  # ---- 冻结输入在构建阶段改变 ⇒ 拒绝发布 + 诊断列出改变路径/前后摘要/发生阶段 ----
  $frozenChangeCase = New-PrepareFreezeCase -Name 'prepare-freeze-input-change'
  $frozenChangePrepareError = Invoke-PrepareRun -Case $frozenChangeCase
  $frozenChangePrepareRecord = Get-LatestPrepareRecord -Case $frozenChangeCase
  # 冻结之后改写一项**声明的构建输入**
  Write-FixtureText -Path $frozenChangeCase.toolchainPath -Text '{"toolchain":"v2-after-freeze"}'
  $frozenChangeError = ''
  try {
    & $packageScript -Manifest $frozenChangeCase.manifestPath -Configuration debug -ReportPath $frozenChangeCase.reportPath -PackageRoot $frozenChangeCase.packagePath -FreezeRecordPath ([string]$frozenChangePrepareRecord.freeze.path) | Out-Null
  } catch {
    $frozenChangeError = $_.Exception.Message
  }
  $frozenChangeDiagnostic = $null
  $frozenChangeDiagnostics = @(Get-ChildItem -LiteralPath (Join-Path $frozenChangeCase.caseRoot 'failures') -Filter 'package-report-failure-*.json' -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
  if ($frozenChangeDiagnostics.Count -gt 0) {
    $frozenChangeDiagnostic = Get-Content -Raw -LiteralPath $frozenChangeDiagnostics[0].FullName -Encoding UTF8 | ConvertFrom-Json
  }
  $changeEntry = $(if ($frozenChangeDiagnostic) { @($frozenChangeDiagnostic.changed_paths | Where-Object { [string]$_.path -match 'toolchain\.json' })[0] } else { $null })
  Assert-Case -Id 'I15d-frozen-input-change-refused' -Condition (
    -not $frozenChangePrepareError -and
    ($frozenChangeError -match 'FROZEN-INPUT-CHANGED') -and
    (-not (Test-Path -LiteralPath $frozenChangeCase.reportPath -PathType Leaf)) -and
    ($null -ne $frozenChangeDiagnostic)) `
    -Detail ("冻结输入在正式构建阶段改变 ⇒ 拒绝发布且不写成功报告：{0}" -f (($frozenChangeError -split "`n")[0]))
  Assert-Case -Id 'I15d2-diagnostic-lists-changed-path-with-before-after-phase' -Condition (
    ($null -ne $changeEntry) -and
    [string]$changeEntry.phase -eq 'build-preflight' -and
    [string]$changeEntry.before_summary -match 'sha256=' -and
    [string]$changeEntry.after_summary -match 'sha256=' -and
    [string]$changeEntry.before_summary -ne [string]$changeEntry.after_summary) `
    -Detail ("诊断显著列出改变路径 + 前后摘要 + 发生阶段：{0} [{1}] phase={2}" -f $changeEntry.path, $changeEntry.kind, $changeEntry.phase)
  Assert-Case -Id 'I15d3-diagnostic-declares-phase-and-ineligible' -Condition (
    ($null -ne $frozenChangeDiagnostic) -and
    [string]$frozenChangeDiagnostic.run_phase -eq 'build' -and
    (Test-IdentityFalseValue -Value $frozenChangeDiagnostic.release_eligible) -and
    @($frozenChangeDiagnostic.changed_paths).Count -ge 1 -and
    [string]$frozenChangeDiagnostic.preparation_and_freeze.freeze_id -eq [string]$frozenChangePrepareRecord.freeze.freeze_id) `
    -Detail ("诊断给出 run_phase={0} release_eligible=false changed_paths={1}（引用同一冻结记录）" -f $frozenChangeDiagnostic.run_phase, @($frozenChangeDiagnostic.changed_paths).Count)

  # ---- 准备阶段生成的配置计入冻结输入：构建阶段改写即拒绝 ----
  $generatedCase = New-PrepareFreezeCase -Name 'prepare-freeze-generated-config' -WithGeneratedConfig
  $generatedPrepareError = Invoke-PrepareRun -Case $generatedCase
  $generatedPrepareRecord = Get-LatestPrepareRecord -Case $generatedCase
  $generatedFreezeDoc = $null
  if ($generatedPrepareRecord -and $generatedPrepareRecord.freeze.path) {
    $generatedFreezeFull = Join-Path $workspaceFull ([string]$generatedPrepareRecord.freeze.path).Replace('/', '\')
    if (Test-Path -LiteralPath $generatedFreezeFull -PathType Leaf) {
      $generatedFreezeDoc = Get-Content -Raw -LiteralPath $generatedFreezeFull -Encoding UTF8 | ConvertFrom-Json
    }
  }
  $generatedListed = $(if ($generatedFreezeDoc) { @($generatedFreezeDoc.generated_configs | Where-Object { [string]$_.path -match 'generated/toolchain\.json' })[0] } else { $null })
  Assert-Case -Id 'I15e-generated-config-counted-as-frozen-input' -Condition (
    -not $generatedPrepareError -and
    (Test-Path -LiteralPath $generatedCase.generatedPath -PathType Leaf) -and
    ($null -ne $generatedListed) -and
    [string]$generatedListed.sha256 -and
    [string]$generatedListed.generated_by -match 'preparation\.steps' -and
    [string]$generatedPrepareRecord.generated_configs[0].path -match 'generated/toolchain\.json') `
    -Detail ("准备阶段生成配置并计入冻结输入：{0}（sha256={1}，generated_by={2}）" -f $generatedListed.path, ([string]$generatedListed.sha256).Substring(0, 12), $generatedListed.generated_by)
  Write-FixtureText -Path $generatedCase.generatedPath -Text '{"toolchain":"rewritten-after-freeze"}'
  $generatedChangeError = ''
  try {
    & $packageScript -Manifest $generatedCase.manifestPath -Configuration debug -ReportPath $generatedCase.reportPath -PackageRoot $generatedCase.packagePath -FreezeRecordPath ([string]$generatedPrepareRecord.freeze.path) | Out-Null
  } catch {
    $generatedChangeError = $_.Exception.Message
  }
  $generatedDiagnostic = $null
  $generatedDiagnostics = @(Get-ChildItem -LiteralPath (Join-Path $generatedCase.caseRoot 'failures') -Filter 'package-report-failure-*.json' -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
  if ($generatedDiagnostics.Count -gt 0) {
    $generatedDiagnostic = Get-Content -Raw -LiteralPath $generatedDiagnostics[0].FullName -Encoding UTF8 | ConvertFrom-Json
  }
  $generatedChangeEntry = $(if ($generatedDiagnostic) { @($generatedDiagnostic.changed_paths | Where-Object { [string]$_.path -match 'generated/toolchain\.json' })[0] } else { $null })
  Assert-Case -Id 'I15e2-generated-config-rewrite-refused' -Condition (
    ($generatedChangeError -match 'FROZEN-INPUT-CHANGED') -and
    ($null -ne $generatedChangeEntry) -and
    [string]$generatedChangeEntry.classification -eq 'preparation-generated-config' -and
    [string]$generatedChangeEntry.phase -eq 'build-preflight') `
    -Detail ("准备阶段生成的配置在构建阶段被改写 ⇒ 拒绝（分类={0}，阶段={1}）" -f $generatedChangeEntry.classification, $generatedChangeEntry.phase)

  # ---- 准备阶段发现输入变化必须显式确认 ----
  $confirmCase = New-PrepareFreezeCase -Name 'prepare-freeze-confirm'
  $confirmFirstError = Invoke-PrepareRun -Case $confirmCase
  $confirmFirstRecord = Get-LatestPrepareRecord -Case $confirmCase
  # 冻结之后改变声明输入，再跑一次准备阶段：未确认 ⇒ 拒绝
  Write-FixtureText -Path $confirmCase.toolchainPath -Text '{"toolchain":"v3-needs-confirmation"}'
  $confirmRefusedError = Invoke-PrepareRun -Case $confirmCase
  $confirmRefusedDiagnostic = $null
  $confirmDiagnostics = @(Get-ChildItem -LiteralPath (Join-Path $confirmCase.caseRoot 'failures') -Filter 'package-report-failure-*.json' -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
  if ($confirmDiagnostics.Count -gt 0) {
    $confirmRefusedDiagnostic = Get-Content -Raw -LiteralPath $confirmDiagnostics[0].FullName -Encoding UTF8 | ConvertFrom-Json
  }
  $confirmDiagnosticEntry = $(if ($confirmRefusedDiagnostic) { @($confirmRefusedDiagnostic.changed_paths | Where-Object { [string]$_.path -match 'toolchain\.json' })[0] } else { $null })
  Assert-Case -Id 'I15f-prepare-change-requires-explicit-confirmation' -Condition (
    -not $confirmFirstError -and
    ($confirmRefusedError -match 'PREPARE-INPUT-CHANGED') -and
    ($null -ne $confirmRefusedDiagnostic) -and
    [string]$confirmRefusedDiagnostic.run_phase -eq 'prepare' -and
    (Test-IdentityFalseValue -Value $confirmRefusedDiagnostic.release_eligible) -and
    ($null -ne $confirmDiagnosticEntry) -and
    [string]$confirmDiagnosticEntry.phase -eq 'prepare' -and
    [string]$confirmDiagnosticEntry.before_summary -match 'sha256=' -and
    [string]$confirmDiagnosticEntry.after_summary -match 'sha256=') `
    -Detail ("准备阶段发现输入变化但未获确认 ⇒ 拒绝并写出诊断（run_phase=prepare、含前后摘要、release_eligible=false）：{0}" -f (($confirmRefusedError -split "`n")[0]))
  # 显式确认后：冻结成功，且冻结记录登记了"变化已接受"与变化清单
  $confirmAcceptedError = ''
  try {
    & $packageScript -Manifest $confirmCase.manifestPath -Configuration debug -ReportPath $confirmCase.reportPath -Prepare -AcceptPreparationChanges | Out-Null
  } catch {
    $confirmAcceptedError = $_.Exception.Message
  }
  $confirmedRecord = Get-LatestPrepareRecord -Case $confirmCase
  $confirmedFreezeDoc = $null
  if ($confirmedRecord -and $confirmedRecord.freeze.path) {
    $confirmedFreezeFull = Join-Path $workspaceFull ([string]$confirmedRecord.freeze.path).Replace('/', '\')
    if (Test-Path -LiteralPath $confirmedFreezeFull -PathType Leaf) {
      $confirmedFreezeDoc = Get-Content -Raw -LiteralPath $confirmedFreezeFull -Encoding UTF8 | ConvertFrom-Json
    }
  }
  Assert-Case -Id 'I15f2-confirmed-changes-are-recorded-then-frozen' -Condition (
    -not $confirmAcceptedError -and
    ($null -ne $confirmedFreezeDoc) -and
    ([bool]$confirmedFreezeDoc.changes_accepted) -and
    @($confirmedFreezeDoc.changed_paths_at_prepare).Count -ge 1 -and
    [string]$confirmedFreezeDoc.freeze_id -ne [string]$confirmFirstRecord.freeze.freeze_id -and
    [string]$confirmedRecord.freeze.freeze_id -eq [string]$confirmedFreezeDoc.freeze_id) `
    -Detail ("确认后重新冻结：freeze_id 由 {0} 变为 {1}，冻结记录登记 changes_accepted=true 与 {2} 处变化" -f $confirmFirstRecord.freeze.freeze_id, $confirmedFreezeDoc.freeze_id, @($confirmedFreezeDoc.changed_paths_at_prepare).Count)
} finally {
  # 夹具留在 tmp/ 供人工核对；测试脚本本身不删除，避免把"测试失败现场的路径"抹掉。
}

$failures = @($script:caseResults | Where-Object { $_.status -ne 'PASS' })
Write-Host ("cases: {0}, failed: {1}" -f $script:caseResults.Count, $failures.Count)
if ($failures.Count -gt 0) {
  $failures | ForEach-Object { Write-Error ("{0}: {1}" -f $_.id, $_.detail) }
  throw "package build identity contract failed: $($failures.Count) finding(s)"
}
Write-Output "PASS package-build-identity ($($script:caseResults.Count) cases)"
