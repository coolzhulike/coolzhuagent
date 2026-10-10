#requires -Version 5.1
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'lib/build-capture-bootstrap.ps1')
$root=Split-Path $PSScriptRoot -Parent
foreach($name in @('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','CARGO_BUILD_TARGET','CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS','COOLZHU_CAPTURE_TEST_FAIL_PIPE')) {
  $before=[Environment]::GetEnvironmentVariable($name)
  $evidence=Join-Path $root ('tmp/env-rejected-'+[guid]::NewGuid().ToString('N'))
  try {
    [Environment]::SetEnvironmentVariable($name,'test-unverified')
    $rejected=$false
    try {New-BuildCaptureBootstrap -RepoPath $root -TargetDirectory (Join-Path $root 'tmp/unused-target') -EvidenceDirectory $evidence | Out-Null} catch {
      if($_.Exception.Message -notlike 'BUILD-CAPTURE-ENVIRONMENT:*'){throw}
      $rejected=$true
    }
    if(-not $rejected -or (Test-Path -LiteralPath $evidence)){throw '环境拒绝未发生在副作用之前'}
    Write-Output ('PASS 启动前拒绝 '+$name)
  } finally {[Environment]::SetEnvironmentVariable($name,$before)}
}
