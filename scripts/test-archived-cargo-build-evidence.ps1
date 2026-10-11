$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
. (Join-Path $PSScriptRoot 'lib/cargo-build-evidence.ps1')
$taskRoot = Join-Path $taskRepo ('tmp/archived-cargo-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($taskRoot) | Out-Null
$record = Join-Path $taskRoot 'record.jsonl'
[IO.File]::WriteAllText($record, '{"reason":"compiler-artifact","package_id":"fixture","target":{"name":"fixture"}}' + "`n" + '{"reason":"build-finished","success":true}')
$entry = Save-PackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveRoot (Join-Path $taskRoot 'original') -RunId 'fixture-run' -ArtifactId 'fixture.bin' -RecordPath $record -ExpectedSha256 (Get-IdentityFileHash -Path $record)
$reportPath = Join-Path $taskRoot 'report.json'
$report = [ordered]@{ run_id = 'fixture-run'; payload_inventory = @{}; package_root = 'tmp'; cargo_build_evidence = @{ status = 'collected'; entries = @($entry) } }
Write-GovernedPackageReport -Report $report -Path $reportPath -RepoPath $taskRepo -ReportId 'fixture-archive-report' | Out-Null
$archiveRoot = Join-Path $taskRoot 'published-archive'
& (Join-Path $PSScriptRoot 'package-report-retention.ps1') -Action Protect -ReportPath $reportPath -ReleaseVersion '0.0.1' -Archive `
  -ArchiveRoot $archiveRoot -ReportRoot (Join-Path $taskRoot 'report-index') -IndexPath (Join-Path $taskRoot 'report-index/index.json') `
  -Workspace $taskRepo -ExtraEvidencePath (Join-Path $taskRepo $entry.path) | Out-Null
$manifestPath = Join-Path $archiveRoot 'release-0.0.1/evidence/build-identity/fixture-archive-report/package-report-files.json'
# 仅移走本测试独占文件，保留失败证据；归档验证必须不依赖原路径。
Move-Item -LiteralPath (Join-Path $taskRepo $entry.path) -Destination (Join-Path $taskRoot 'relocated-original.jsonl')
Move-Item -LiteralPath $reportPath -Destination (Join-Path $taskRoot 'relocated-report.json')
$refused = $false
try { Assert-PackageCargoBuildEvidence -Report $report -RepoPath $taskRepo | Out-Null } catch { $refused = $true }
if (-not $refused) { throw '原始引用缺失未被拒绝' }
$verified = Assert-ArchivedPackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveManifestPath $manifestPath
if (-not $verified.verified -or @($verified.entries).Count -ne 1) { throw '独立归档未通过验证' }
function Invoke-TestRetentionVerify {
  & (Join-Path $PSScriptRoot 'package-report-retention.ps1') -Action Verify -ArchiveRoot $archiveRoot `
    -ReportRoot (Join-Path $taskRoot 'report-index') -IndexPath (Join-Path $taskRoot 'report-index/index.json') -Workspace $taskRepo
}
$retention = @(Invoke-TestRetentionVerify) | Select-Object -Last 1
if ($retention.verified -ne 1) { throw '临时引用移走后保留工具未验证归档回执' }
$originalManifest = [IO.File]::ReadAllText($manifestPath)
$changes = @(
  { param($copy) $copy.report_id = 'foreign' },
  { param($copy) $copy.report_file_sha256 = ('0' * 64) },
  { param($copy) $copy.extra_evidence = @() },
  { param($copy) $copy.extra_evidence = @($copy.extra_evidence[0], $copy.extra_evidence[0]) },
  { param($copy) $copy.extra_evidence[0].file_sha256 = ('0' * 64) },
  { param($copy) $copy.extra_evidence[0].archived_path = '../escape.jsonl' }
)
foreach ($change in $changes) {
  $copy = $originalManifest | ConvertFrom-Json
  & $change $copy
  [IO.File]::WriteAllText($manifestPath, ($copy | ConvertTo-Json -Depth 10))
  $refused = $false
  try { Assert-ArchivedPackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveManifestPath $manifestPath | Out-Null } catch { $refused = $true }
  if (-not $refused) { throw '无效归档映射被接受' }
}
[IO.File]::WriteAllText($manifestPath, $originalManifest)
$manifest = $originalManifest | ConvertFrom-Json
[IO.File]::AppendAllText((Join-Path $taskRepo $manifest.extra_evidence[0].archived_path), 'tamper')
$refused = $false
try { Assert-ArchivedPackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveManifestPath $manifestPath | Out-Null } catch { $refused = $true }
if (-not $refused) { throw '归档回执正文改写未被拒绝' }
$refused = $false
try { Invoke-TestRetentionVerify | Out-Null } catch { $refused = $true }
if (-not $refused) { throw '保留工具忽略归档回执正文改写' }
Write-Host 'PASS archived-cargo-build-evidence (11 cases)'
