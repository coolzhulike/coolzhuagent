$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
. (Join-Path $PSScriptRoot 'lib/cargo-build-evidence.ps1')
$taskRoot = Join-Path $taskRepo ('tmp/cargo-evidence-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($taskRoot) | Out-Null
$record = Join-Path $taskRoot 'record.jsonl'
$valid = '{"reason":"compiler-artifact","package_id":"fixture","target":{"name":"fixture"}}' + "`n" + '{"reason":"build-finished","success":true}'
[IO.File]::WriteAllText($record, $valid, [Text.UTF8Encoding]::new($false))
$hash = Get-IdentityFileHash -Path $record
$saved = Save-PackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveRoot $taskRoot -RunId 'run-one' -ArtifactId 'fixture.bin' -RecordPath $record -ExpectedSha256 $hash
$archive = Join-Path $taskRepo $saved.path
if ((Get-IdentityFileHash -Path $archive) -ne $hash -or (Get-Item -LiteralPath $archive).Length -ne $saved.length) { throw '归档字节与绑定不一致' }
$refused = $false
try { Save-PackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveRoot $taskRoot -RunId 'run-one' -ArtifactId 'fixture.bin' -RecordPath $record -ExpectedSha256 $hash | Out-Null } catch { $refused = $true }
if (-not $refused -or (Get-IdentityFileHash -Path $archive) -ne $hash) { throw '既有归档被覆盖' }
$cases = @(
  @{ text = $valid + "`n{}"; expected = $null },
  @{ text = '{"reason":"build-finished","success":false}'; expected = $null },
  @{ text = $valid.Replace('true', '"true"'); expected = $null },
  @{ text = $valid + "`n" + '{"reason":"build-finished","success":true}'; expected = $null },
  @{ text = '{"reason":"compiler-artifact","target":{}}' + "`n" + '{"reason":"build-finished","success":true}'; expected = $null },
  @{ text = $valid + ' '; expected = $hash }
)
$index = 0
foreach ($case in $cases) {
  $index++
  [IO.File]::WriteAllText($record, $case.text, [Text.UTF8Encoding]::new($false))
  $expected = if ($case.expected) { $case.expected } else { Get-IdentityFileHash -Path $record }
  $refused = $false
  try { Save-PackageCargoBuildEvidence -RepoPath $taskRepo -ArchiveRoot $taskRoot -RunId "bad-$index" -ArtifactId 'fixture.bin' -RecordPath $record -ExpectedSha256 $expected | Out-Null } catch { $refused = $true }
  if (-not $refused -or (Test-Path -LiteralPath (Join-Path $taskRoot "bad-$index/fixture.bin.jsonl"))) { throw "负例未拒绝或发布了无效归档：$index" }
}
$report = [pscustomobject]@{ run_id = 'run-one'; cargo_build_evidence = [pscustomobject]@{ status = 'collected'; entries = @($saved) } }
$verified = Assert-PackageCargoBuildEvidence -Report $report -RepoPath $taskRepo
if (-not $verified.verified -or @($verified.entries).Count -ne 1) { throw '合法报告引用没有通过消费端校验' }
$legacy = Assert-PackageCargoBuildEvidence -Report ([pscustomobject]@{}) -RepoPath $taskRepo
if ($legacy.verified -or $legacy.status -ne 'unavailable-legacy-report') { throw '旧报告被补造验证事实' }
$changes = @(
  { param($copy) $copy.cargo_build_evidence.entries[0].run_id = 'foreign' },
  { param($copy) $copy.cargo_build_evidence.entries = @($copy.cargo_build_evidence.entries[0], $copy.cargo_build_evidence.entries[0]) },
  { param($copy) $copy.cargo_build_evidence.entries = @() },
  { param($copy) $copy.cargo_build_evidence.entries[0].length = 1 },
  { param($copy) $copy.cargo_build_evidence.entries[0].length = '123' },
  { param($copy) $copy.cargo_build_evidence.entries[0].compiler_artifact_records = 999 },
  { param($copy) $copy.cargo_build_evidence.entries[0].compiler_artifact_records = '1' },
  { param($copy) $copy.cargo_build_evidence.entries[0].path = '../run-one/fixture.bin.jsonl' },
  { param($copy) $copy.cargo_build_evidence.entries[0].success_message_verified = $false },
  { param($copy) $copy.cargo_build_evidence.entries[0].sha256 = ('0' * 64) }
)
foreach ($change in $changes) {
  $copy = $report | ConvertTo-Json -Depth 8 | ConvertFrom-Json
  & $change $copy
  $refused = $false
  try { Assert-PackageCargoBuildEvidence -Report $copy -RepoPath $taskRepo | Out-Null } catch { $refused = $true }
  if (-not $refused) { throw '无效报告引用被消费端接受' }
}
[IO.File]::AppendAllText($archive, 'tamper')
$refused = $false
try { Assert-PackageCargoBuildEvidence -Report $report -RepoPath $taskRepo | Out-Null } catch { $refused = $true }
if (-not $refused) { throw '归档正文改写未被拒绝' }
Write-Host 'PASS cargo-build-evidence (21 cases)'
