#requires -Version 5.1
param([string]$HelperPath='tmp/cargo-target-integration/debug/build-capture.exe')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'lib/managed-build-capture.ps1')
$helper=(Resolve-Path -LiteralPath $HelperPath).Path
$sha=(Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash
$root=Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/managed capture '+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$child=Join-Path $root 'child.ps1'
[IO.File]::WriteAllText($child,@'
param([string]$Marker)
[IO.File]::WriteAllText($Marker,'started')
$bytes=[Text.UTF8Encoding]::new($false).GetBytes("中文采集`n")
[Console]::OpenStandardOutput().Write($bytes,0,$bytes.Length)
[Console]::OpenStandardError().Write($bytes,0,$bytes.Length)
exit 7
'@,[Text.UTF8Encoding]::new($false))
$marker=Join-Path $root 'started'
$out=Join-Path $root 'stdout'; $err=Join-Path $root 'stderr'
$call=@{HelperPath=$helper;ExpectedHelperSha256=$sha;Command='powershell';Arguments=@('-NoProfile','-File',$child,$marker);WorkingDirectory=$root;StdoutPath=$out;StderrPath=$err}
$result=Invoke-ManagedBuildCapture @call
if(@($result).Count -ne 1){throw '采集入口返回值被诊断任务对象污染'}
if($result.exit_code -ne 7 -or $result.helper_sha256 -ine $sha){throw '退出码或身份回执错误'}
foreach($path in @($out,$err)) {
  if([IO.File]::ReadAllText($path,[Text.UTF8Encoding]::new($false,$true)) -cne "中文采集`n"){throw '双管道字节内容错误'}
}
Write-Output 'PASS 正确摘要允许启动，UTF8双管道完整，退出码7保留'
$call.ExpectedHelperSha256='0'*64
$call.StdoutPath=Join-Path $root 'wrong-out'; $call.StderrPath=Join-Path $root 'wrong-err'
$call.Arguments=@('-NoProfile','-File',$child,(Join-Path $root 'wrong-marker'))
$rejected=$false
try { Invoke-ManagedBuildCapture @call | Out-Null } catch {
  if($_.Exception.Message -notlike 'BUILD-CAPTURE-IDENTITY:*'){throw}
  $rejected=$true
}
if(-not $rejected){throw '错误摘要未拒绝'}
foreach($path in @($call.StdoutPath,$call.StderrPath,(Join-Path $root 'wrong-marker'))) {
  if(Test-Path -LiteralPath $path){throw '身份拒绝后仍产生执行副作用'}
}
Write-Output 'PASS 错误摘要在启动前拒绝，无命令和日志副作用'
$call.ExpectedHelperSha256=$sha; $call.StdoutPath=$out
$before=(Get-FileHash -LiteralPath $out).Hash
$result=Invoke-ManagedBuildCapture @call
if(@($result).Count -ne 1){throw '失败采集入口返回值被污染'}
if($result.exit_code -ne 125 -or (Get-FileHash -LiteralPath $out).Hash -ne $before -or (Test-Path (Join-Path $root 'wrong-marker'))){throw '旧日志保护失败'}
if([IO.File]::ReadAllText($result.helper_diagnostic_path,[Text.UTF8Encoding]::new($false,$true)) -notmatch 'BUILD-CAPTURE-FAILED:'){throw '辅助程序失败诊断没有归档'}
Write-Output 'PASS 旧日志拒绝返回125，旧内容保留且命令未启动'
Write-Output 'PASS 辅助程序自身125诊断独立落盘'
$producer=Join-Path $root 'diagnostic-producer.ps1'
[IO.File]::WriteAllText($producer,@'
$buffer=New-Object byte[] 65536
$pipe=[Console]::OpenStandardError()
for($index=0;$index -lt 64;$index++){ $pipe.Write($buffer,0,$buffer.Length) }
$pipe.Flush()
exit 9
'@,[Text.UTF8Encoding]::new($false))
$info=[Diagnostics.ProcessStartInfo]::new('powershell')
$info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardError=$true
Set-ManagedCaptureArguments -Info $info -Arguments @('-NoProfile','-File',$producer)
$destination=[IO.MemoryStream]::new()
$destination.Dispose() # 确定性模拟日志写入故障，不改变生产者。
$process=[Diagnostics.Process]::Start($info)
try {
  $drain=[CoolzhuCapture.DiagnosticDrain]::Start($process.StandardError.BaseStream,$destination)
  if(-not $process.WaitForExit(10000)){throw '诊断写入故障后真实管道仍阻塞生产者'}
  if(-not $drain.Wait(10000)){throw '诊断管道未完成排空'}
  $failure=$drain.GetAwaiter().GetResult()
  if(-not $failure -or $process.ExitCode -ne 9){throw '诊断写入故障或生产者退出事实丢失'}
} finally {
  if(-not $process.HasExited){$process.Kill(); $process.WaitForExit()}
  $process.Dispose()
}
Write-Output 'PASS 真实4MiB诊断在落盘失败后仍排空，生产者退出9且故障保留'
$source=[IO.MemoryStream]::new(); $source.Dispose()
$destination=[IO.MemoryStream]::new()
$drain=[CoolzhuCapture.DiagnosticDrain]::Start($source,$destination)
$info=[Diagnostics.ProcessStartInfo]::new('powershell')
$info.UseShellExecute=$false; $info.CreateNoWindow=$true
Set-ManagedCaptureArguments -Info $info -Arguments @('-NoProfile','-Command','Start-Sleep -Seconds 30')
$process=[Diagnostics.Process]::Start($info)
try {
  $timer=[Diagnostics.Stopwatch]::StartNew()
  $rejected=$false
  try { Wait-ManagedCaptureDiagnostic -Process $process -Drain $drain } catch {
    if($_.Exception.Message -notlike 'BUILD-CAPTURE-DIAGNOSTIC:*'){throw}
    $rejected=$true
  }
  if(-not $rejected -or -not $process.HasExited -or $timer.Elapsed.TotalSeconds -gt 15){throw '读取故障未及时拒绝并停止自身生产者'}
} finally {
  if(-not $process.HasExited){$process.Kill(); $process.WaitForExit()}
  $process.Dispose(); $destination.Dispose()
}
Write-Output 'PASS 诊断读取故障及时拒绝并确认本次生产者停止'
$badOut=Join-Path $root 'bad-owner-out'; $badErr=Join-Path $root 'bad-owner-err'
$info=[Diagnostics.ProcessStartInfo]::new($helper)
$info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardError=$true
Set-ManagedCaptureArguments -Info $info -Arguments @('--owner',[string]$PID,'1',$badOut,$badErr,$root,'--','powershell','-NoProfile','-File',$child,(Join-Path $root 'bad-owner-marker'))
$process=[Diagnostics.Process]::Start($info)
try {
  $diagnosticText=$process.StandardError.ReadToEnd()
  if(-not $process.WaitForExit(10000)){throw '错误宿主身份拒绝超时'}
  if($process.ExitCode -ne 125 -or $diagnosticText -notmatch '创建时间不一致'){throw '错误宿主创建时间未拒绝'}
  foreach($path in @($badOut,$badErr,(Join-Path $root 'bad-owner-marker'))){if(Test-Path -LiteralPath $path){throw '错误宿主身份仍产生构建副作用'}}
} finally {
  if(-not $process.HasExited){$process.Kill(); $process.WaitForExit()}
  $process.Dispose()
}
Write-Output 'PASS 宿主创建时间不匹配在日志和命令启动前拒绝'
$info=[Diagnostics.ProcessStartInfo]::new($helper)
$info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardError=$true
Set-ManagedCaptureArguments -Info $info -Arguments @($badOut,$badErr,$root,'--','powershell','-NoProfile','-File',$child,(Join-Path $root 'bad-owner-marker'))
$process=[Diagnostics.Process]::Start($info)
try {
  $diagnosticText=$process.StandardError.ReadToEnd()
  if(-not $process.WaitForExit(10000) -or $process.ExitCode -ne 125 -or $diagnosticText -notmatch '缺少 --owner'){throw '无宿主旧语法未拒绝'}
  foreach($path in @($badOut,$badErr,(Join-Path $root 'bad-owner-marker'))){if(Test-Path -LiteralPath $path){throw '无宿主旧语法产生执行副作用'}}
} finally {if(-not $process.HasExited){$process.Kill();$process.WaitForExit()};$process.Dispose()}
Write-Output 'PASS 无宿主旧语法在日志和命令启动前拒绝'
$ownerInfo=[Diagnostics.ProcessStartInfo]::new('powershell')
$ownerInfo.UseShellExecute=$false; $ownerInfo.CreateNoWindow=$true
Set-ManagedCaptureArguments -Info $ownerInfo -Arguments @('-NoProfile','-Command','Start-Sleep -Milliseconds 500; exit 0')
$retiredOwner=[Diagnostics.Process]::Start($ownerInfo)
try {
  [void]$retiredOwner.Handle
  $created=$retiredOwner.StartTime.ToFileTimeUtc()
  if(-not $retiredOwner.WaitForExit(10000)){throw '测试宿主正常退出未确认'}
  $info=[Diagnostics.ProcessStartInfo]::new($helper)
  $info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardError=$true
  Set-ManagedCaptureArguments -Info $info -Arguments @('--owner',[string]$retiredOwner.Id,[string]$created,$badOut,$badErr,$root,'--','powershell','-NoProfile','-File',$child,(Join-Path $root 'bad-owner-marker'))
  $process=[Diagnostics.Process]::Start($info)
  try {
    $diagnosticText=$process.StandardError.ReadToEnd()
    if(-not $process.WaitForExit(10000) -or $process.ExitCode -ne 125 -or $diagnosticText -notmatch '调用宿主已退出'){throw '已退出但身份仍有效的宿主未拒绝'}
    foreach($path in @($badOut,$badErr,(Join-Path $root 'bad-owner-marker'))){if(Test-Path -LiteralPath $path){throw '已退出宿主产生构建副作用'}}
  } finally {if(-not $process.HasExited){$process.Kill();$process.WaitForExit()};$process.Dispose()}
} finally {
  if(-not $retiredOwner.HasExited){$retiredOwner.Kill();$retiredOwner.WaitForExit()}
  $retiredOwner.Dispose()
}
Write-Output 'PASS 宿主身份仍可查询但已退出时在日志和命令启动前拒绝'
