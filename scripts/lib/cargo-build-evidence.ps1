# 构建回执独占归档；原始字节身份进入包报告，不以临时日志路径代替内容绑定。
if (-not ('CoolzhuBoundedJsonLine' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
public static class CoolzhuBoundedJsonLine {
  public static string Read(StreamReader reader) {
    var line = new StringBuilder();
    int ch;
    while ((ch = reader.Read()) != -1) {
      if (ch == 10) return line.ToString().TrimEnd('\r');
      if (line.Length >= 4 * 1024 * 1024) throw new InvalidDataException("Cargo JSON 单行超过4Mi字符上限，未截断为有效记录");
      line.Append((char)ch);
    }
    return line.Length == 0 ? null : line.ToString().TrimEnd('\r');
  }
}
'@
}

function Get-CargoStreamSummary {
  param([IO.Stream]$Source)
  $Source.Position = 0
  $reader = [IO.StreamReader]::new($Source, [Text.UTF8Encoding]::new($false, $true), $false, 4096, $true)
  $finished = 0; $artifacts = 0; $last = $null; $firstLine = $true
  try {
    while ($null -ne ($line = [CoolzhuBoundedJsonLine]::Read($reader))) {
      if ($firstLine) { $line = $line.TrimStart([char]0xfeff); $firstLine = $false }
      if (-not $line.Trim()) { continue }
      $record = ConvertFrom-IdentityJson -Json $line
      if ($finished -gt 0) { throw '最终构建终态后仍有记录' }
      if ($record.reason -eq 'build-finished') {
        if ($record.success -isnot [bool] -or -not $record.success) { throw '构建终态未成功' }
        $finished++
      }
      if ($record.reason -eq 'compiler-artifact') {
        if ([string]::IsNullOrWhiteSpace([string]$record.package_id) -or -not $record.target) { throw '构建组件记录缺少包或目标身份' }
        $artifacts++
      }
      $last = $record.reason
    }
  } finally { $reader.Dispose() }
  if ($finished -ne 1 -or $last -ne 'build-finished' -or $artifacts -eq 0) { throw '构建回执缺少唯一最终成功终态或实际组件记录' }
  return $artifacts
}

function Save-PackageCargoBuildEvidence {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$ArchiveRoot,
    [Parameter(Mandatory = $true)][string]$RunId,
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [Parameter(Mandatory = $true)][string]$RecordPath,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256
  )
  foreach ($name in @($RunId, $ArtifactId)) {
    if ($name -cnotmatch '^[a-zA-Z0-9][a-zA-Z0-9._-]*$') { throw '构建回执运行或产物身份无效' }
  }
  if (-not (Test-PackagePathInsideRoot -Path $ArchiveRoot -Root $RepoPath)) {
    throw '构建回执归档必须位于当前仓库内'
  }
  $source = [IO.FileStream]::new($RecordPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  try {
  $algorithm = [Security.Cryptography.SHA256]::Create()
  try { $hash = ([BitConverter]::ToString($algorithm.ComputeHash($source))).Replace('-', '').ToLowerInvariant() } finally { $algorithm.Dispose() }
  if ($ExpectedSha256 -cnotmatch '^[a-f0-9]{64}$' -or $hash -cne $ExpectedSha256) {
    throw '构建回执在采集后发生变化，拒绝归档'
  }
  $artifactCount = Get-CargoStreamSummary -Source $source
  $directory = Join-Path $ArchiveRoot $RunId
  [IO.Directory]::CreateDirectory($directory) | Out-Null
  $path = Join-Path $directory "$ArtifactId.jsonl"
  $stream = $null
  try {
    $stream = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    $source.Position = 0
    $source.CopyTo($stream, 81920)
    $stream.Flush($true)
  } catch {
    if ($null -ne $stream) {
      $stream.Dispose()
      $stream = $null
      Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue
    }
    throw
  } finally {
    if ($null -ne $stream) { $stream.Dispose() }
  }
  [pscustomobject]@{
    artifact_id = $ArtifactId
    run_id = $RunId
    path = Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $path
    sha256 = $hash
    length = $source.Length
    compiler_artifact_records = $artifactCount
    success_message_verified = $true
    scope = '实际构建观察，含缓存与构建期组件；不证明运行时分发、许可证义务或来源签名'
  }
  } finally { $source.Dispose() }
}

# 消费端核对引用字节；旧报告或未采集模式明确返回未验证，不补造构建事实。
function Assert-PackageCargoBuildEvidence {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [hashtable]$ArchivePaths
  )
  $evidence = $Report.cargo_build_evidence
  if ($null -eq $evidence) {
    return [pscustomobject]@{ status = 'unavailable-legacy-report'; verified = $false; entries = @() }
  }
  if ($evidence.status -in @('not-collected', 'not-collected-no-build')) {
    if (@($evidence.entries).Count -ne 0) { throw '未采集模式不得附带已采集回执' }
    return [pscustomobject]@{ status = [string]$evidence.status; verified = $false; entries = @() }
  }
  if ($evidence.status -ne 'collected' -or @($evidence.entries).Count -eq 0) { throw '构建回执状态不完整' }
  $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  foreach ($entry in @($evidence.entries)) {
    if ($entry.run_id -cne $Report.run_id -or
        $entry.success_message_verified -isnot [bool] -or $entry.success_message_verified -ne $true -or
        ([string]$entry.artifact_id) -cnotmatch '^[a-zA-Z0-9][a-zA-Z0-9._-]*$' -or
        -not $seen.Add([string]$entry.artifact_id)) { throw '构建回执运行/产物身份不一致或重复' }
    $relative = [string]$entry.path
    if ([string]::IsNullOrWhiteSpace($relative) -or $relative -match '[\\:]|(^|/)\.\.?(/|$)' -or
        $relative.StartsWith('/') -or -not $relative.EndsWith("/$($entry.run_id)/$($entry.artifact_id).jsonl", [StringComparison]::Ordinal)) {
      throw '构建回执归档路径与身份不一致'
    }
    if ($null -ne $ArchivePaths) {
      if (-not $ArchivePaths.ContainsKey($relative)) { throw '发布归档缺少原始回执引用映射' }
      $path = [IO.Path]::GetFullPath([string]$ArchivePaths[$relative])
    } else {
      $path = [IO.Path]::GetFullPath((Join-Path $RepoPath $relative))
    }
    if (-not (Test-PackagePathInsideRoot -Path $path -Root $RepoPath)) { throw '构建回执路径逃逸' }
    $node = Get-Item -LiteralPath $path -Force
    while ($node.FullName.TrimEnd('\', '/') -ne $RepoPath.TrimEnd('\', '/')) {
      if ($node.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '构建回执不得经重解析点读取' }
      $node = Get-Item -LiteralPath (Split-Path -Parent $node.FullName) -Force
    }
    $bytes = [IO.File]::ReadAllBytes($path)
    if ($entry.length -isnot [int] -and $entry.length -isnot [long]) { throw '构建回执长度类型无效' }
    if ($bytes.Length -ne $entry.length -or
        ([string]$entry.sha256) -cnotmatch '^[a-f0-9]{64}$' -or
        (Get-IdentityHashFromBytes -Bytes $bytes) -cne $entry.sha256) { throw '归档构建回执字节身份已变化' }
  }
  [pscustomobject]@{ status = 'collected'; verified = $true; entries = @($evidence.entries) }
}

function Resolve-PackageArchiveEvidencePath {
  param([string]$RepoPath, [string]$ArchiveDirectory, [string]$Relative)
  if ([string]::IsNullOrWhiteSpace($Relative) -or $Relative -match '[\\:]|(^|/)\.\.?(/|$)|//|/$' -or $Relative.StartsWith('/')) {
    throw '发布归档引用路径不规范'
  }
  $path = [IO.Path]::GetFullPath((Join-Path $RepoPath $Relative))
  if (-not (Test-PackagePathInsideRoot -Path $path -Root $ArchiveDirectory) -or
      -not (Test-PackagePathInsideRoot -Path $path -Root $RepoPath)) { throw '发布归档引用路径逃逸' }
  $node = Get-Item -LiteralPath $path -Force
  while ($node.FullName.TrimEnd('\', '/') -ne $RepoPath.TrimEnd('\', '/')) {
    if ($node.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '发布归档不得经重解析点读取' }
    $node = Get-Item -LiteralPath (Split-Path -Parent $node.FullName) -Force
  }
  return $path
}

function Assert-ArchivedPackageCargoBuildEvidence {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$ArchiveManifestPath
  )
  $directory = Split-Path -Parent ([IO.Path]::GetFullPath($ArchiveManifestPath))
  $manifestPath = Resolve-PackageArchiveEvidencePath -RepoPath $RepoPath -ArchiveDirectory $directory `
    -Relative (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $ArchiveManifestPath)
  $utf8 = [Text.UTF8Encoding]::new($false, $true)
  $manifest = ConvertFrom-IdentityJson -Json ($utf8.GetString([IO.File]::ReadAllBytes($manifestPath)).TrimStart([char]0xfeff))
  if ($manifest.kind -ne 'package-report-archive') { throw '发布归档清单类型无效' }
  $reportPath = Resolve-PackageArchiveEvidencePath -RepoPath $RepoPath -ArchiveDirectory $directory -Relative $manifest.archived_report_path
  $bytes = [IO.File]::ReadAllBytes($reportPath)
  $hash = Get-IdentityHashFromBytes -Bytes $bytes
  if ($hash -cne $manifest.report_file_sha256) { throw '发布归档报告原始字节身份不一致' }
  $report = ConvertFrom-IdentityJson -Json ($utf8.GetString($bytes).TrimStart([char]0xfeff))
  $identity = Assert-PackageReportContentHash -Report $report
  if ($identity.report_id -cne $manifest.report_id -or $identity.content_sha256 -cne $manifest.report_content_sha256) {
    throw '发布归档报告内容身份不一致'
  }
  $paths = @{}
  foreach ($entry in @($report.cargo_build_evidence.entries)) {
    $matching = @($manifest.extra_evidence | Where-Object { $_.source_path -ceq $entry.path })
    if ($matching.Count -ne 1 -or $matching[0].file_sha256 -cne $entry.sha256) { throw '发布归档回执映射缺失、重复或哈希不一致' }
    $paths[[string]$entry.path] = Resolve-PackageArchiveEvidencePath -RepoPath $RepoPath -ArchiveDirectory $directory -Relative $matching[0].archived_path
  }
  $result = Assert-PackageCargoBuildEvidence -Report $report -RepoPath $RepoPath -ArchivePaths $paths
  $result | Add-Member -NotePropertyName report_id -NotePropertyValue $identity.report_id
  $result | Add-Member -NotePropertyName report_content_sha256 -NotePropertyValue $identity.content_sha256
  return $result
}
