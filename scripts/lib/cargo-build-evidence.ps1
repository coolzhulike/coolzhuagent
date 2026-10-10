# 构建回执独占归档；原始字节身份进入包报告，不以临时日志路径代替内容绑定。
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
  $bytes = [IO.File]::ReadAllBytes($RecordPath)
  $hash = Get-IdentityHashFromBytes -Bytes $bytes
  if ($ExpectedSha256 -cnotmatch '^[a-f0-9]{64}$' -or $hash -cne $ExpectedSha256) {
    throw '构建回执在采集后发生变化，拒绝归档'
  }
  $text = [Text.UTF8Encoding]::new($false, $true).GetString($bytes).TrimStart([char]0xfeff)
  $records = @($text -split "`r?`n" | Where-Object { $_.Trim() } | ForEach-Object {
    ConvertFrom-IdentityJson -Json $_
  })
  $finished = @($records | Where-Object { $_.reason -eq 'build-finished' })
  $artifacts = @($records | Where-Object { $_.reason -eq 'compiler-artifact' })
  if ($finished.Count -ne 1 -or $finished[0].success -isnot [bool] -or $finished[0].success -ne $true -or
      $records[-1].reason -ne 'build-finished' -or $artifacts.Count -eq 0) {
    throw '构建回执缺少唯一最终成功终态或实际组件记录'
  }
  foreach ($artifact in $artifacts) {
    if ([string]::IsNullOrWhiteSpace([string]$artifact.package_id) -or -not $artifact.target) {
      throw '构建组件记录缺少包或目标身份'
    }
  }
  $directory = Join-Path $ArchiveRoot $RunId
  [IO.Directory]::CreateDirectory($directory) | Out-Null
  $path = Join-Path $directory "$ArtifactId.jsonl"
  $stream = $null
  try {
    $stream = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    $stream.Write($bytes, 0, $bytes.Length)
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
    length = $bytes.Length
    compiler_artifact_records = $artifacts.Count
    success_message_verified = $true
    scope = '实际构建观察，含缓存与构建期组件；不证明运行时分发、许可证义务或来源签名'
  }
}

# 消费端核对引用字节；旧报告或未采集模式明确返回未验证，不补造构建事实。
function Assert-PackageCargoBuildEvidence {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [Parameter(Mandatory = $true)][string]$RepoPath
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
    $path = [IO.Path]::GetFullPath((Join-Path $RepoPath $relative))
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
