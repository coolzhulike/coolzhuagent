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
Write-Host 'PASS cargo-build-evidence (8 cases)'
