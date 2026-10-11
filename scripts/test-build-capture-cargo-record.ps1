#requires -Version 5.1
param([Parameter(Mandatory=$true)][string]$BootstrapResultPath)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
. (Join-Path $PSScriptRoot 'lib/cargo-build-evidence.ps1')
. (Join-Path $PSScriptRoot 'lib/build-capture-bootstrap.ps1')
$result=Get-Content -Raw -LiteralPath $BootstrapResultPath | ConvertFrom-IdentityJson
$exe=Join-Path $result.args[9] 'debug/build-capture.exe'
$records=@(Get-Content -LiteralPath $result.raw_record_path | ForEach-Object {ConvertFrom-IdentityJson -Json $_})
Assert-BuildCaptureCargoRecord $result.raw_record_path $result.working_dir $exe
Write-Output 'PASS 实际 Cargo 完整回执匹配辅助程序'
$root=Join-Path $result.working_dir ('tmp/capture-record-test-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
foreach($case in @('missing','executable','features','duplicate')) {
  $copy=@($records | ForEach-Object {ConvertFrom-IdentityJson -Json ($_ | ConvertTo-Json -Depth 30 -Compress)})
  $bin=@($copy | Where-Object {$_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'build-capture'})[0]
  switch($case) {
    'missing' {$copy=@($copy | Where-Object {$_ -ne $bin})}
    'executable' {$bin.executable=Join-Path $root 'foreign.exe'}
    'features' {$bin.features=@('build-capture-test-support')}
    'duplicate' {$copy=@($copy[0..($copy.Count-2)])+@($bin,$copy[-1])}
  }
  $path=Join-Path $root ($case+'.jsonl')
  [IO.File]::WriteAllLines($path,[string[]]@($copy | ForEach-Object {$_ | ConvertTo-Json -Depth 30 -Compress}),[Text.UTF8Encoding]::new($false))
  $rejected=$false
  try {Assert-BuildCaptureCargoRecord $path $result.working_dir $exe} catch {$rejected=$true}
  if(-not $rejected){throw ('损坏回执未拒绝：'+$case)}
  Write-Output ('PASS 拒绝 '+$case)
}
