$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
$taskRoot = Join-Path $taskRepo ('tmp/retention-extra-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($taskRoot) | Out-Null
$reportPath = Join-Path $taskRoot 'report.json'
$report = [ordered]@{ run_id = 'fixture-run'; payload_inventory = @{}; package_root = 'tmp' }
$identity = Write-GovernedPackageReport -Report $report -Path $reportPath -RepoPath $taskRepo -ReportId 'fixture-retention-report'
$archiveRoot = Join-Path $taskRoot 'archive'
$reportRoot = Join-Path $taskRoot 'reports'
$evidence = Join-Path $taskRoot 'fixture.jsonl'
[IO.File]::WriteAllText($evidence, 'original', [Text.UTF8Encoding]::new($false))
function Invoke-TestArchive {
  param([string[]]$EvidenceFiles)
  & (Join-Path $PSScriptRoot 'package-report-retention.ps1') -Action Protect -ReportPath $reportPath `
    -ReleaseVersion '0.0.1' -Archive -ArchiveRoot $archiveRoot -ReportRoot $reportRoot `
    -IndexPath (Join-Path $reportRoot 'index.json') -Workspace $taskRepo -ExtraEvidencePath $EvidenceFiles | Out-Null
}
Invoke-TestArchive @($evidence)
$saved = Join-Path $archiveRoot 'release-0.0.1/evidence/build-identity/fixture-retention-report/fixture.jsonl'
$archivedReport = Join-Path (Split-Path -Parent $saved) 'package-report.json'
$frozen = Get-IdentityFileHash -Path $saved
$reportHash = Get-IdentityFileHash -Path $archivedReport
Invoke-TestArchive @($evidence)
if ((Get-IdentityFileHash -Path $saved) -ne $frozen) { throw '重复归档改变原字节' }
$reserved = Join-Path $taskRoot 'package-report.json'
[IO.File]::WriteAllText($reserved, 'collision')
$other = Join-Path $taskRoot 'other'
[IO.Directory]::CreateDirectory($other) | Out-Null
$sameName = Join-Path $other 'fixture.jsonl'
[IO.File]::WriteAllText($sameName, 'different')
foreach ($invalid in @(@($reserved), @($evidence, $sameName),
    @((Join-Path $taskRoot 'missing.jsonl')), @((Join-Path $env:WINDIR 'win.ini')))) {
  $refused = $false
  try { Invoke-TestArchive $invalid } catch { $refused = $true }
  if (-not $refused -or (Get-IdentityFileHash -Path $saved) -ne $frozen -or
      (Get-IdentityFileHash -Path $archivedReport) -ne $reportHash) { throw '冲突未被拒绝或既有证据被改写' }
}
[IO.File]::WriteAllText($evidence, 'changed')
$refused = $false
try { Invoke-TestArchive @($evidence) } catch { $refused = $true }
if (-not $refused -or (Get-IdentityFileHash -Path $saved) -ne $frozen) { throw '既有异字节发布证据被覆盖' }
Write-Host 'PASS retention-extra-evidence (7 cases)'
