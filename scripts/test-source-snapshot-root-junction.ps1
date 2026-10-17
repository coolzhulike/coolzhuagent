$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
$fixture = Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/snapshot-root-junction-' + [guid]::NewGuid().ToString('N'))
$repo = Join-Path $fixture 'repo'
$outside = Join-Path $fixture 'outside'
[void][IO.Directory]::CreateDirectory($repo)
[void][IO.Directory]::CreateDirectory($outside)
[IO.File]::WriteAllText((Join-Path $outside 'outside.rs'), 'const OUTSIDE: bool = true;')
New-Item -ItemType Junction -Path (Join-Path $repo 'linked') -Target $outside | Out-Null
$manifest = [pscustomobject]@{source_snapshot=[pscustomobject]@{roots=@('linked');allow_reparse_points=@('.*')}}
$scope = Get-SourceSnapshotScope -ManifestData $manifest
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope $scope
if (@($result.entries).Count -ne 0 -or @($result.skipped_reparse_points).Count -ne 1 -or @($result.missing_roots) -notcontains 'linked') {
  throw '声明根junction被追随或未标记范围不完整'
}
Write-Output 'PASS：声明根junction登记而不追随，即使allow_reparse_points放行仍不得进入外部文件'
[void][IO.Directory]::CreateDirectory((Join-Path $outside 'nested'))
[IO.File]::WriteAllText((Join-Path $outside 'nested/outside.rs'), 'const NESTED: bool = true;')
$manifest.source_snapshot.roots = @('linked/nested')
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope (Get-SourceSnapshotScope -ManifestData $manifest)
if (@($result.entries).Count -ne 0 -or @($result.skipped_reparse_points).Count -ne 1 -or @($result.missing_roots) -notcontains 'linked/nested') {
  throw '声明根中间层junction被追随'
}
Write-Output 'PASS：声明根中间层junction不得追随'
$manifest.source_snapshot.roots = @('../outside')
$rejected = $false
try { Get-SourceSnapshotEntries -RepoPath $repo -Scope (Get-SourceSnapshotScope -ManifestData $manifest) | Out-Null }
catch { if ($_.Exception.Message -match 'SOURCE-SNAPSHOT-SCOPE-INVALID') { $rejected = $true } else { throw } }
if (-not $rejected) { throw '声明根路径越界未拒绝' }
Write-Output 'PASS：声明根路径越界被拒绝'
$manifest.source_snapshot.roots = @('absent/nested')
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope (Get-SourceSnapshotScope -ManifestData $manifest)
if (@($result.missing_roots) -notcontains 'absent/nested') { throw '缺失根未保留报告行为' }
Write-Output 'PASS：缺失根保留范围不完整报告'
[void][IO.Directory]::CreateDirectory((Join-Path $repo 'local/nested'))
[IO.File]::WriteAllText((Join-Path $repo 'local/nested/local.rs'), 'const LOCAL: bool = true;')
$manifest.source_snapshot.roots = @('local/nested')
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope (Get-SourceSnapshotScope -ManifestData $manifest)
if (@($result.entries).Count -ne 1 -or @($result.missing_roots).Count -ne 0) { throw '普通本地根被误拒绝' }
Write-Output 'PASS：普通本地嵌套根保持可枚举'
New-Item -ItemType Junction -Path (Join-Path $repo 'local/linked') -Target $outside | Out-Null
$manifest.source_snapshot.roots = @('local')
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope (Get-SourceSnapshotScope -ManifestData $manifest)
if (@($result.entries).Count -ne 1 -or @($result.skipped_reparse_points).Count -ne 1 -or @($result.missing_roots) -notcontains 'local/linked') {
  throw '子链接显式放行追随外部目标或冒充完整输入'
}
Write-Output 'PASS：显式子链接登记并拒绝完整性，不追随外部目标'
New-Item -ItemType Junction -Path (Join-Path $repo 'local/cycle') -Target (Join-Path $repo 'local') | Out-Null
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope (Get-SourceSnapshotScope -ManifestData $manifest)
if (@($result.entries).Count -ne 1 -or @($result.skipped_reparse_points).Count -ne 2 -or @($result.missing_roots) -notcontains 'local/cycle') { throw '循环链接未有限登记拒绝' }
Write-Output 'PASS：循环链接有限登记，不进入无限遍历'
New-Item -ItemType Junction -Path (Join-Path $repo 'local/node_modules') -Target $outside | Out-Null
$scope = Get-SourceSnapshotScope -ManifestData $manifest
$scope.exclude_path_patterns = @($scope.exclude_path_patterns) + @('^node_modules($|/)')
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope $scope
if (@($result.entries).Count -ne 1 -or @($result.skipped_reparse_points).Count -ne 3 -or @($result.missing_roots) -contains 'local/node_modules') { throw '明确排除链接被误算源码缺失' }
Write-Output 'PASS：明确排除的链接不冒充源码，也不误算缺失输入'
