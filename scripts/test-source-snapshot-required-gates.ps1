$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
$complete = @('declared_source_snapshot_scope','source_input_stability','declared_roots_present') | ForEach-Object {
  New-PackageReleaseGate -Name $_ -Result 'pass' -Evidence '夹具'
}
$cases = @(
  @{ name='空门禁'; gates=@(); expected=$false },
  @{ name='错误字段不能充当门禁'; gates=@([pscustomobject]@{name='declared_source_snapshot_scope';result='pass'},[pscustomobject]@{name='source_input_stability';result='pass'},[pscustomobject]@{name='declared_roots_present';result='pass'}); expected=$false },
  @{ name='缺失门禁'; gates=@($complete[0],$complete[1]); expected=$false },
  @{ name='重复通过门禁'; gates=@($complete)+@($complete[0]); expected=$false },
  @{ name='未确认门禁'; gates=@($complete[0],$complete[1],[pscustomobject]@{gate='declared_roots_present';result='not-confirmed'}); expected=$false },
  @{ name='失败门禁'; gates=@($complete[0],$complete[1],[pscustomobject]@{gate='declared_roots_present';result='fail'}); expected=$false },
  @{ name='完整源码门禁'; gates=@($complete); expected=$true }
)
foreach ($case in $cases) {
  $receipt = New-PackageReleaseEligibility -Configuration 'fixture' -BuildSnapshotIntegrity 'not-confirmed' -Gates $case.gates
  if ($receipt.source_snapshot_verified -ne $case.expected -or $receipt.release_eligible -ne $case.expected) {
    throw ('源码快照门禁误判：' + $case.name)
  }
  Write-Output ('PASS：' + $case.name)
}
# 其它门禁失败仍拒绝发布，即使三项源码门禁齐全。
$receipt = New-PackageReleaseEligibility -Configuration 'fixture' -BuildSnapshotIntegrity 'not-confirmed' -Gates (@($complete)+@([pscustomobject]@{gate='payload';result='fail'}))
if (-not $receipt.source_snapshot_verified -or $receipt.release_eligible) { throw '载荷失败被源码门禁覆盖' }
Write-Output 'PASS：载荷失败仍拒绝发布'
