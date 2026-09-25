param(
  # 保留规则：已被正式发布引用的报告**受保护**或**连同发布证据归档**，
  # 普通临时清理（Prune）不得删除唯一证据。
  [Parameter(Mandatory = $true)]
  [ValidateSet('Verify', 'Protect', 'Prune', 'List')]
  [string]$Action,
  [string]$ReportPath,
  [string]$ReleaseArtifact,
  [string]$ReleaseVersion,
  [string]$Reason,
  [switch]$Archive,
  # 与发布报告一起归档的其它证据（例如 installer report）：路径必须在工作区内。
  [string[]]$ExtraEvidencePath = @(),
  # 归档根目录（默认 docs/testing；测试可指向 tmp 下的夹具目录，避免污染真实发布证据）。
  [string]$ArchiveRoot,
  # 报告目录与保留索引落点（默认沿用既有落点 tmp/package-reports；可用于隔离的验证环境）。
  [string]$ReportRoot,
  [string]$IndexPath,
  [string]$Workspace
)

$ErrorActionPreference = 'Stop'

if (-not $Workspace) {
  $Workspace = Split-Path -Parent $PSScriptRoot
}
$workspacePath = [System.IO.Path]::GetFullPath($Workspace).TrimEnd('\', '/')
$workspacePrefix = $workspacePath + [System.IO.Path]::DirectorySeparatorChar

. (Join-Path $workspacePath 'scripts/lib/build-identity.ps1')

# 报告目录沿用既有落点（第五轮裁决 B-2）：不迁移、不改名既有证据。
$reportRoot = if ($ReportRoot) {
  if ([System.IO.Path]::IsPathRooted($ReportRoot)) { [System.IO.Path]::GetFullPath($ReportRoot) } else { [System.IO.Path]::GetFullPath((Join-Path $workspacePath $ReportRoot)) }
} else {
  Join-Path $workspacePath ($script:PackageReportRoot -replace '/', '\')
}
$reportRoot = $reportRoot.TrimEnd('\', '/')
if (-not $reportRoot.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw (New-IdentityFailureException -Category 'RETENTION-ARG-INVALID' `
    -Detail "ReportRoot 必须留在工作区内：$reportRoot" `
    -Remediation '报告与保留索引必须留在工作区内，避免证据散落到工作区外')
}
$indexPath = if ($IndexPath) {
  if ([System.IO.Path]::IsPathRooted($IndexPath)) { [System.IO.Path]::GetFullPath($IndexPath) } else { [System.IO.Path]::GetFullPath((Join-Path $workspacePath $IndexPath)) }
} else {
  Join-Path $reportRoot 'retention-index.json'
}
$archiveRoot = if ($ArchiveRoot) {
  if ([System.IO.Path]::IsPathRooted($ArchiveRoot)) { [System.IO.Path]::GetFullPath($ArchiveRoot) } else { [System.IO.Path]::GetFullPath((Join-Path $workspacePath $ArchiveRoot)) }
} else {
  Join-Path $workspacePath 'docs\testing'
}
$archiveRoot = $archiveRoot.TrimEnd('\', '/')
if (-not $archiveRoot.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw (New-IdentityFailureException -Category 'RETENTION-ARG-INVALID' `
    -Detail "ArchiveRoot 必须留在工作区内：$archiveRoot" `
    -Remediation '不要把发布证据归档到工作区外')
}

function ConvertTo-WorkspaceRelative {
  param([Parameter(Mandatory = $true)][string]$Path)

  $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  if (-not $fullPath.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "path must stay inside the workspace: $fullPath"
  }
  return $fullPath.Substring($workspacePrefix.Length).Replace('\', '/')
}

function Read-RetentionIndex {
  param([switch]$AllowMissing)

  $index = [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    kind = 'package-report-retention-index'
    generated_by = 'scripts/package-report-retention.ps1'
    updated_at = $null
    entries = @()
  }
  if (-not (Test-Path -LiteralPath $indexPath -PathType Leaf)) {
    if ($AllowMissing) { return $index }
    throw (New-IdentityFailureException -Category 'RETENTION-INDEX-MISSING' `
      -Detail "保留索引不存在：$indexPath" `
      -Remediation '先执行 -Action Protect 登记被正式发布引用的报告，或人工确认没有任何发布证据后再清理')
  }
  try {
    $parsed = Get-Content -Raw -LiteralPath $indexPath -Encoding UTF8 | ConvertFrom-Json
  } catch {
    throw (New-IdentityFailureException -Category 'RETENTION-INDEX-INVALID' `
      -Detail "保留索引无法解析：$indexPath（$($_.Exception.Message)）" `
      -Remediation '不要删除索引；先修复或从 docs/testing/release-*/evidence/build-identity 归档恢复')
  }
  $index.entries = @($parsed.entries)
  $index.updated_at = [string]$parsed.updated_at
  return $index
}

function Write-RetentionIndex {
  param([Parameter(Mandatory = $true)]$Index)

  New-Item -ItemType Directory -Force -Path $reportRoot | Out-Null
  $Index.updated_at = (Get-Date).ToUniversalTime().ToString('o')
  # 原子替换：索引是"证据不被误删"的唯一链，不允许出现半成品。
  $temp = Join-Path $reportRoot ('.tmp-retention-index-' + [guid]::NewGuid().ToString('N'))
  [System.IO.File]::WriteAllText($temp, ($Index | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
  if (Test-Path -LiteralPath $indexPath) {
    $backup = Join-Path $reportRoot ('.bak-retention-index-' + [guid]::NewGuid().ToString('N'))
    try {
      [System.IO.File]::Replace($temp, $indexPath, $backup, $true)
    } finally {
      if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue }
    }
  } else {
    [System.IO.File]::Move($temp, $indexPath)
  }
}

<#
  归档扫描：即使保留索引丢失，只要发布证据归档还在 docs/testing/release-*/evidence/build-identity/，
  对应报告就必须被视为受保护（避免"索引没了就随便删"）。
#>
function Get-ArchivedReportIdentitySet {
  $identities = @{}
  if (-not (Test-Path -LiteralPath $archiveRoot -PathType Container)) { return $identities }
  foreach ($manifest in @(Get-ChildItem -Path (Join-Path $archiveRoot 'release-*\evidence\build-identity\*\package-report-files.json') -File -ErrorAction SilentlyContinue)) {
    try {
      $parsed = Get-Content -Raw -LiteralPath $manifest.FullName -Encoding UTF8 | ConvertFrom-Json
    } catch {
      continue
    }
    $reportId = [string]$parsed.report_id
    if (-not $reportId) { continue }
    $identities[$reportId] = [pscustomobject]@{
      report_id = $reportId
      archive_manifest = ConvertTo-WorkspaceRelative $manifest.FullName
      report_file_sha256 = [string]$parsed.report_file_sha256
    }
  }
  return $identities
}

function Get-ProtectedReportIdSet {
  $protected = @{}
  $index = Read-RetentionIndex -AllowMissing
  foreach ($entry in @($index.entries)) {
    if ($entry.report_id) { $protected[[string]$entry.report_id] = 'retention-index' }
    if ($entry.report_file_sha256) { $protected["file:$([string]$entry.report_file_sha256)"] = 'retention-index' }
  }
  foreach ($key in (Get-ArchivedReportIdentitySet).Keys) {
    if (-not $protected.ContainsKey($key)) { $protected[$key] = 'release-evidence-archive' }
  }
  return $protected
}

function Invoke-RetentionVerify {
  $index = Read-RetentionIndex
  $problems = [System.Collections.Generic.List[string]]::new()
  $verified = 0
  foreach ($entry in @($index.entries)) {
    $reportId = [string]$entry.report_id
    $candidates = [System.Collections.Generic.List[string]]::new()
    if ($entry.report_path) { $candidates.Add((Join-Path $workspacePath (([string]$entry.report_path).Replace('/', '\')))) }
    if ($entry.archived_report_path) { $candidates.Add((Join-Path $workspacePath (([string]$entry.archived_report_path).Replace('/', '\')))) }

    $found = $null
    foreach ($candidate in $candidates) {
      if (Test-Path -LiteralPath $candidate -PathType Leaf) { $found = $candidate; break }
    }
    if (-not $found) {
      $problems.Add(("report_id={0} 的全部副本都找不到（候选：{1}）" -f $reportId, ($candidates -join '; ')))
      continue
    }

    $fileHash = Get-IdentityFileHash -Path $found
    $recordedFileHashes = @([string]$entry.report_file_sha256, [string]$entry.archived_report_file_sha256) | Where-Object { $_ }
    if ($recordedFileHashes.Count -gt 0 -and $recordedFileHashes -notcontains $fileHash) {
      $problems.Add(("report_id={0} 的文件字节哈希与索引不一致（actual={1} recorded={2}）" -f $reportId, $fileHash, ($recordedFileHashes -join ',')))
      continue
    }

    if ($entry.report_content_sha256) {
      try {
        $report = Get-Content -Raw -LiteralPath $found -Encoding UTF8 | ConvertFrom-Json
        [void](Assert-PackageReportContentHash -Report $report)
      } catch {
        $problems.Add(("report_id={0} 的内容哈希无法重算：{1}" -f $reportId, ($_.Exception.Message -split "`n")[0]))
        continue
      }
    }
    $verified++
  }

  if ($problems.Count -gt 0) {
    throw (New-IdentityFailureException -Category 'RETENTION-VERIFY-FAILED' `
      -Detail (($problems | ForEach-Object { "- $_" }) -join '; ') `
      -Remediation '不要删除其余证据；先恢复缺失副本（从 docs/testing/release-*/evidence/build-identity 归档取回）')
  }
  Write-Host ("PASS package-report-retention verify: {0} protected report(s) verified" -f $verified)
  return [pscustomobject]@{ action = 'Verify'; verified = $verified; index = (ConvertTo-WorkspaceRelative $indexPath) }
}

function Invoke-RetentionProtect {
  if (-not $ReportPath) {
    throw (New-IdentityFailureException -Category 'RETENTION-ARG-MISSING' `
      -Detail 'Protect 需要 -ReportPath' `
      -Remediation '传入被正式发布引用的 package report 路径')
  }
  $fullReportPath = if ([System.IO.Path]::IsPathRooted($ReportPath)) { [System.IO.Path]::GetFullPath($ReportPath) } else { [System.IO.Path]::GetFullPath((Join-Path $workspacePath $ReportPath)) }
  if (-not (Test-Path -LiteralPath $fullReportPath -PathType Leaf)) {
    throw (New-IdentityFailureException -Category 'RETENTION-REPORT-MISSING' `
      -Detail "要保护的报告不存在：$fullReportPath" `
      -Remediation '确认报告已由 scripts/package-all.ps1 生成')
  }

  $report = Get-Content -Raw -LiteralPath $fullReportPath -Encoding UTF8 | ConvertFrom-Json
  # 裁决第 2、9 条：**失败诊断**与 release_eligible=false 的报告不是可发布收据，
  # 不得被 Protect 成"发布证据"（否则等于用失败报告给混合输入产物补一张收据）。
  # 治理前的历史报告没有该字段 ⇒ 不据此外推（保持既有证据链可登记）。
  [void](Assert-PackageReportReleaseEligible -Report $report -ReportPath $fullReportPath -Purpose 'retention-protect')
  $reportId = [string]$report.report_identity.report_id
  $contentHash = [string]$report.report_identity.content_sha256
  $contentHashScope = 'canonical sorted leaf paths; report_identity.content_sha256 excluded'
  if (-not $reportId) {
    # 治理前的历史报告：没有 report_identity，但仍然是既有证据链的一环。
    # 不伪造内容哈希，只登记文件字节哈希并显式标注口径。
    $reportId = 'pre-governance-report-' + (Get-IdentityFileHash -Path $fullReportPath).Substring(0, 16)
    $contentHash = $null
    $contentHashScope = 'file-bytes-only (pre-governance report without report_identity)'
    Write-Warning "report has no report_identity; registering as pre-governance evidence with file-bytes-only identity: $fullReportPath"
  } elseif ($contentHash) {
    [void](Assert-PackageReportContentHash -Report $report)
  }

  $reportPathRelative = ConvertTo-WorkspaceRelative $fullReportPath
  $reportFileHash = Get-IdentityFileHash -Path $fullReportPath
  $inventoryPath = [string]$report.payload_inventory.path
  $inventoryFileHash = $null
  $inventoryRelative = $null
  if ($inventoryPath) {
    $candidate = Join-Path $workspacePath (([string]$report.package_root).Replace('/', '\'))
    $candidate = Join-Path $candidate ($inventoryPath.Replace('/', '\'))
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      $inventoryRelative = ConvertTo-WorkspaceRelative $candidate
      $inventoryFileHash = Get-IdentityFileHash -Path $candidate
    }
  }

  $archivedReportPath = $null
  $archivedReportFileHash = $null
  if ($Archive) {
    if (-not $ReleaseVersion) {
      throw (New-IdentityFailureException -Category 'RETENTION-ARG-MISSING' `
        -Detail 'Archive 需要 -ReleaseVersion（归档到 docs/testing/release-<version>/evidence/build-identity/）' `
        -Remediation '发布流程里传入 -ReleaseVersion，例如 build-msi.ps1 -Version')
    }
    if ($ReleaseVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
      throw (New-IdentityFailureException -Category 'RETENTION-ARG-INVALID' `
        -Detail "ReleaseVersion 必须是三段数字版本号，收到：$ReleaseVersion" `
        -Remediation '使用与 MSI 一致的版本号')
    }
    $archiveDir = Join-Path $archiveRoot ("release-$ReleaseVersion\evidence\build-identity\$reportId")
    New-Item -ItemType Directory -Force -Path $archiveDir | Out-Null
    $archivedReportFile = Join-Path $archiveDir 'package-report.json'
    Copy-Item -LiteralPath $fullReportPath -Destination $archivedReportFile -Force
    $archivedReportPath = ConvertTo-WorkspaceRelative $archivedReportFile
    $archivedReportFileHash = Get-IdentityFileHash -Path $archivedReportFile

    $archivedInventoryFile = $null
    $archivedInventoryHash = $null
    if ($inventoryRelative) {
      $archivedInventoryFile = Join-Path $archiveDir 'payload-inventory.json'
      Copy-Item -LiteralPath (Join-Path $workspacePath ($inventoryRelative.Replace('/', '\'))) -Destination $archivedInventoryFile -Force
      $archivedInventoryHash = Get-IdentityFileHash -Path $archivedInventoryFile
    }

    $extraEvidence = [System.Collections.Generic.List[object]]::new()
    foreach ($extra in @($ExtraEvidencePath)) {
      if (-not $extra) { continue }
      $extraFull = if ([System.IO.Path]::IsPathRooted($extra)) { [System.IO.Path]::GetFullPath($extra) } else { [System.IO.Path]::GetFullPath((Join-Path $workspacePath $extra)) }
      if (-not (Test-Path -LiteralPath $extraFull -PathType Leaf)) {
        throw (New-IdentityFailureException -Category 'RETENTION-EVIDENCE-MISSING' `
          -Detail "要一并归档的证据不存在：$extraFull" `
          -Remediation '先生成该证据（例如 installer report）再归档')
      }
      $extraName = [System.IO.Path]::GetFileName($extraFull)
      $extraDestination = Join-Path $archiveDir $extraName
      if (Test-Path -LiteralPath $extraDestination) { Remove-Item -LiteralPath $extraDestination -Force }
      Copy-Item -LiteralPath $extraFull -Destination $extraDestination -Force
      $extraEvidence.Add([ordered]@{
        source_path = ConvertTo-WorkspaceRelative $extraFull
        archived_path = ConvertTo-WorkspaceRelative $extraDestination
        file_sha256 = Get-IdentityFileHash -Path $extraDestination
      }) | Out-Null
    }

    $archiveManifest = [ordered]@{
      schema = [int]$script:BuildIdentitySchema
      kind = 'package-report-archive'
      archived_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      report_id = $reportId
      report_content_sha256 = $contentHash
      report_content_hash_scope = $contentHashScope
      report_file_sha256 = $reportFileHash
      original_report_path = $reportPathRelative
      archived_report_path = $archivedReportPath
      payload_inventory_original_path = $inventoryRelative
      payload_inventory_file_sha256 = $inventoryFileHash
      archived_payload_inventory_path = $(if ($archivedInventoryFile) { ConvertTo-WorkspaceRelative $archivedInventoryFile } else { $null })
      archived_payload_inventory_file_sha256 = $archivedInventoryHash
      release_version = $ReleaseVersion
      release_artifact = $ReleaseArtifact
      reason = $(if ($Reason) { $Reason } else { 'referenced by a formal release' })
      extra_evidence = @($extraEvidence)
      rule = '该归档是发布证据：普通临时清理不得删除；tmp/ 下的原报告即使被清理，报本身份仍可从本归档核对'
    }
    $archiveManifestPath = Join-Path $archiveDir 'package-report-files.json'
    [System.IO.File]::WriteAllText($archiveManifestPath, ($archiveManifest | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
    Write-Host ("archive package report evidence: {0}" -f (ConvertTo-WorkspaceRelative $archiveDir))
  }

  $index = Read-RetentionIndex -AllowMissing
  $entries = [System.Collections.Generic.List[object]]::new()
  $replaced = $false
  foreach ($entry in @($index.entries)) {
    if ([string]$entry.report_id -eq $reportId) {
      $entries.Add([pscustomobject]@{
        report_id = $reportId
        report_path = $reportPathRelative
        report_content_sha256 = $contentHash
        report_content_hash_scope = $contentHashScope
        report_file_sha256 = $reportFileHash
        payload_inventory_path = $inventoryRelative
        payload_inventory_file_sha256 = $inventoryFileHash
        payload_digest = [string]$report.build_identity.payload_digest
        source_snapshot_digest = [string]$report.build_identity.source_snapshot_digest
        build_input_digest = [string]$report.build_identity.build_input_digest
        release_version = $ReleaseVersion
        release_artifact = $ReleaseArtifact
        archived_report_path = $archivedReportPath
        archived_report_file_sha256 = $archivedReportFileHash
        protected_at_utc = (Get-Date).ToUniversalTime().ToString('o')
        reason = $(if ($Reason) { $Reason } else { 'referenced by a formal release' })
      }) | Out-Null
      $replaced = $true
      continue
    }
    $entries.Add($entry)
  }
  if (-not $replaced) {
    $entries.Add([pscustomobject]@{
      report_id = $reportId
      report_path = $reportPathRelative
      report_content_sha256 = $contentHash
      report_content_hash_scope = $contentHashScope
      report_file_sha256 = $reportFileHash
      payload_inventory_path = $inventoryRelative
      payload_inventory_file_sha256 = $inventoryFileHash
      payload_digest = [string]$report.build_identity.payload_digest
      source_snapshot_digest = [string]$report.build_identity.source_snapshot_digest
      build_input_digest = [string]$report.build_identity.build_input_digest
      release_version = $ReleaseVersion
      release_artifact = $ReleaseArtifact
      archived_report_path = $archivedReportPath
      archived_report_file_sha256 = $archivedReportFileHash
      protected_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      reason = $(if ($Reason) { $Reason } else { 'referenced by a formal release' })
    }) | Out-Null
  }
  $index.entries = @($entries)
  Write-RetentionIndex -Index $index
  Write-Host ("protected package report: report_id={0} content_sha256={1}" -f $reportId, $contentHash)
  return [pscustomobject]@{
    action = 'Protect'
    report_id = $reportId
    report_path = $reportPathRelative
    report_content_sha256 = $contentHash
    archived_report_path = $archivedReportPath
    index = (ConvertTo-WorkspaceRelative $indexPath)
  }
}

function Invoke-RetentionPrune {
  # fail-closed：索引缺失/损坏时**拒绝清理**，而不是删除可能是唯一证据的报告。
  $index = Read-RetentionIndex
  $protected = Get-ProtectedReportIdSet
  if (-not (Test-Path -LiteralPath $reportRoot -PathType Container)) {
    Write-Host 'PASS package-report-retention prune: report root does not exist'
    return [pscustomobject]@{ action = 'Prune'; removed = 0; kept_protected = 0 }
  }

  $removed = [System.Collections.Generic.List[string]]::new()
  $kept = [System.Collections.Generic.List[string]]::new()
  foreach ($file in @(Get-ChildItem -LiteralPath $reportRoot -File -Filter '*.json' -Force)) {
    if ($file.Name -eq 'retention-index.json') { continue }
    if ($file.Name -like 'latest-*.json') { continue }
    # 失败诊断不是发布证据，但也不该被"普通临时清理"顺手删掉：它是"为什么没发布"的唯一记录。
    # （失败诊断本体在 <reportRoot>/failures/ 分区，本处按名字再兜一次，避免有人把它挪到根目录。）
    if ($file.Name -like 'package-report-failure-*') {
      $kept.Add(("{0} (失败诊断：诊断证据，不由清理动作删除)" -f $file.Name))
      continue
    }
    $parsed = $null
    try {
      $parsed = Get-Content -Raw -LiteralPath $file.FullName -Encoding UTF8 | ConvertFrom-Json
    } catch {
      $kept.Add(("{0} (无法解析，停手不删)" -f $file.Name))
      continue
    }
    $fileHash = Get-IdentityFileHash -Path $file.FullName
    $reportId = [string]$parsed.report_identity.report_id
    $isProtected = ($reportId -and $protected.ContainsKey($reportId)) -or $protected.ContainsKey("file:$fileHash")
    if ($isProtected) {
      $kept.Add(("{0} (受保护：{1})" -f $file.Name, $protected[$reportId]))
      continue
    }
    Remove-Item -LiteralPath $file.FullName -Force
    $removed.Add($file.Name)
  }
  Write-Host ("PASS package-report-retention prune: removed={0} kept={1}" -f $removed.Count, $kept.Count)
  return [pscustomobject]@{
    action = 'Prune'
    removed = @($removed)
    kept = @($kept)
    index = (ConvertTo-WorkspaceRelative $indexPath)
  }
}

switch ($Action) {
  'Verify' { Invoke-RetentionVerify }
  'Protect' { Invoke-RetentionProtect }
  'Prune' { Invoke-RetentionPrune }
  'List' {
    $index = Read-RetentionIndex -AllowMissing
    $index.entries | ConvertTo-Json -Depth 6
  }
}
