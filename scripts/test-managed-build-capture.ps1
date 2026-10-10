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
if($result.exit_code -ne 125 -or (Get-FileHash -LiteralPath $out).Hash -ne $before -or (Test-Path (Join-Path $root 'wrong-marker'))){throw '旧日志保护失败'}
Write-Output 'PASS 旧日志拒绝返回125，旧内容保留且命令未启动'
