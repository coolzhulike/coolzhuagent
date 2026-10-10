#requires -Version 5.1
param([string]$HelperPath='tmp/cargo-target-build-capture/debug/build-capture.exe',[switch]$HostExit)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'lib/managed-build-capture.ps1')
$helper=(Resolve-Path -LiteralPath $HelperPath).Path
$root=Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/diagnostic-tree-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$worker=Join-Path $root 'worker.ps1'
$ready=Join-Path $root 'ready.json'
[IO.File]::WriteAllText($worker,@'
param([string]$Ready)
$child=Start-Process powershell -ArgumentList '-NoProfile -Command "Start-Sleep -Seconds 60"' -WindowStyle Hidden -PassThru
[IO.File]::WriteAllText($Ready,(@{root=$PID;child=$child.Id}|ConvertTo-Json -Compress))
Start-Sleep -Seconds 60
'@,[Text.UTF8Encoding]::new($false))
$info=[Diagnostics.ProcessStartInfo]::new($helper)
$info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardError=$true
Set-ManagedCaptureArguments -Info $info -Arguments @('--owner',[string]$PID,[string]([Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTimeUtc()),(Join-Path $root 'stdout'),(Join-Path $root 'stderr'),$root,'--','powershell','-NoProfile','-File',$worker,$ready)
if($HostExit) {
  $hostScript=Join-Path $root 'host.ps1'
  [IO.File]::WriteAllText($hostScript,@'
param([string]$Library,[string]$Helper,[string]$Root,[string]$Worker,[string]$Ready)
$ErrorActionPreference='Stop'
. $Library
Invoke-ManagedBuildCapture -HelperPath $Helper -ExpectedHelperSha256 (Get-FileHash $Helper).Hash -Command powershell -Arguments @('-NoProfile','-File',$Worker,$Ready) -WorkingDirectory $Root -StdoutPath (Join-Path $Root 'stdout') -StderrPath (Join-Path $Root 'stderr') | ConvertTo-Json | Set-Content (Join-Path $Root 'receipt.json')
'@,[Text.UTF8Encoding]::new($false))
  $info=[Diagnostics.ProcessStartInfo]::new('powershell')
  $info.UseShellExecute=$false; $info.CreateNoWindow=$true
  Set-ManagedCaptureArguments -Info $info -Arguments @('-NoProfile','-File',$hostScript,(Join-Path $PSScriptRoot 'lib/managed-build-capture.ps1'),$helper,$root,$worker,$ready)
}
$process=[Diagnostics.Process]::Start($info)
$owned=@(); $source=$null; $destination=$null
try {
  $deadline=[DateTime]::UtcNow.AddSeconds(10)
  while(-not [IO.File]::Exists($ready)) {
    if($process.HasExited -or [DateTime]::UtcNow -gt $deadline){throw '受管根及后代未到达故障屏障'}
    Start-Sleep -Milliseconds 25
  }
  $ids=[IO.File]::ReadAllText($ready)|ConvertFrom-Json
  if($HostExit) {
    $rootProcess=Get-CimInstance Win32_Process -Filter ('ProcessId='+$ids.root)
    $helperProcess=Get-CimInstance Win32_Process -Filter ('ProcessId='+$rootProcess.ParentProcessId)
    if(-not $helperProcess -or $helperProcess.ExecutablePath -ine $helper -or $helperProcess.ParentProcessId -ne $process.Id){throw '无法确认测试宿主和辅助程序的真实父子关系'}
    $entry=[Diagnostics.Process]::GetProcessById([int]$helperProcess.ProcessId)
    [void]$entry.Handle
    $owned+=,$entry
  }
  foreach($ownedId in @($ids.root,$ids.child)) {
    $entry=[Diagnostics.Process]::GetProcessById([int]$ownedId)
    [void]$entry.Handle # 保持同一进程句柄，避免PID复用影响退出判断。
    if($entry.HasExited){throw '测试进程在故障注入前已退出'}
    $owned+=,$entry
  }
  if($HostExit) {
    $process.Kill()
    if(-not $process.WaitForExit(10000)){throw '测试自身宿主退出未确认'}
    $observations=@(foreach($entry in $owned){[pscustomobject]@{pid=$entry.Id;exited_before_cleanup=$entry.WaitForExit(1000)}})
    $allExited=@($observations | Where-Object {-not $_.exited_before_cleanup}).Count -eq 0
    [ordered]@{mode='host-exit';helper_sha256=(Get-FileHash $helper).Hash;host_pid=$process.Id;processes=$observations;all_owned_processes_exited=$allExited;receipt_exists=[IO.File]::Exists((Join-Path $root 'receipt.json'));observed_at_utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 (Join-Path $root 'result.json')
    Write-Output ('证据目录：'+$root)
    if(-not $allExited){throw '宿主已终止但自有采集器或受管进程仍存活（清理前观察）'}
    Write-Output 'PASS 宿主突然终止后自有进程树全部退出'
    return
  }
  $source=[IO.MemoryStream]::new(); $source.Dispose()
  $destination=[IO.MemoryStream]::new()
  $drain=[CoolzhuCapture.DiagnosticDrain]::Start($source,$destination)
  $rejected=$false
  try { Wait-ManagedCaptureDiagnostic -Process $process -Drain $drain } catch {
    if($_.Exception.Message -notlike 'BUILD-CAPTURE-DIAGNOSTIC:*'){throw}
    $rejected=$true
  }
  if(-not $rejected -or -not $process.HasExited){throw '诊断故障没有拒绝并停止辅助程序'}
  foreach($entry in $owned) {
    if(-not $entry.WaitForExit(10000)){throw '辅助程序停止后受管根或后代仍存活'}
  }
  [ordered]@{helper_sha256=(Get-FileHash $helper).Hash; helper_pid=$process.Id; command_root_pid=$ids.root; descendant_pid=$ids.child; diagnostic_fault_rejected=$rejected; all_owned_processes_exited=$true} | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $root 'result.json')
  Write-Output 'PASS 读取任务故障拒绝回执，真实辅助程序、受管根及后代均确认退出'
  Write-Output ('证据目录：'+$root)
} finally {
  if(-not $process.HasExited){$process.Kill(); [void]$process.WaitForExit(10000)}
  foreach($entry in $owned) {
    if(-not $entry.HasExited){$entry.Kill(); [void]$entry.WaitForExit(10000)}
    $entry.Dispose()
  }
  $process.Dispose()
  if($destination){$destination.Dispose()}
}
