$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
$fixture = Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/snapshot-root-junction-' + [guid]::NewGuid().ToString('N'))
$repo = Join-Path $fixture 'repo'
$outside = Join-Path $fixture 'outside'
[void][IO.Directory]::CreateDirectory($repo)
[void][IO.Directory]::CreateDirectory($outside)
[IO.File]::WriteAllText((Join-Path $outside 'outside.rs'), 'const OUTSIDE: bool = true;')
New-Item -ItemType Junction -Path (Join-Path $repo 'linked') -Target $outside | Out-Null
$manifest = [pscustomobject]@{source_snapshot=[pscustomobject]@{roots=@('linked');allow_reparse_points=@('*')}}
$scope = Get-SourceSnapshotScope -ManifestData $manifest
$result = Get-SourceSnapshotEntries -RepoPath $repo -Scope $scope
if (@($result.entries).Count -ne 0 -or @($result.skipped_reparse_points).Count -ne 1 -or @($result.missing_roots) -notcontains 'linked') {
  throw '声明根junction被追随或未标记范围不完整'
}
Write-Output 'PASS：声明根junction登记而不追随，即使allow_reparse_points放行仍不得进入外部文件'
