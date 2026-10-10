#requires -Version 5.1
param([string]$HelperPath = 'tmp/cargo-target-integration/debug/build-capture.exe')
$ErrorActionPreference='Stop'
function Set-CaptureTestArguments($Info, [string[]]$Arguments) {
  if ($Info.PSObject.Properties['ArgumentList']) {
    foreach($arg in $Arguments){[void]$Info.ArgumentList.Add($arg)}
  } else {
    # Windows CRT 参数规则：引号前与尾部反斜杠须成对保留，不能只按空格加引号。
    $quoted=foreach($arg in $Arguments) {
      if($arg.Length -gt 0 -and $arg -notmatch '[\s"]'){$arg;continue}
      '"'+[regex]::Replace([regex]::Replace($arg,'(\\*)"','$1$1\"'),'(\\+)$','$1$1')+'"'
    }
    $Info.Arguments=$quoted -join ' '
  }
}
$helper=(Resolve-Path -LiteralPath $HelperPath).Path
$root=Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/capture evidence '+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$child=Join-Path $root 'child worker.ps1'
[IO.File]::WriteAllText($child, @'
param([string]$Ready,[string]$Release,[string]$Value,[Parameter(Mandatory=$true)][AllowEmptyString()][string]$Empty)
$bytes=[Text.UTF8Encoding]::new($false).GetBytes("中文增量输出`n")
$out=[Console]::OpenStandardOutput(); $err=[Console]::OpenStandardError()
$out.Write($bytes,0,$bytes.Length); $err.Write($bytes,0,$bytes.Length)
[IO.File]::WriteAllText($Ready,($Value+'|'+$Empty))
$deadline=[DateTime]::UtcNow.AddSeconds(30)
while(-not [IO.File]::Exists($Release)) {
  if([DateTime]::UtcNow -gt $deadline){exit 97}
  Start-Sleep -Milliseconds 20
}
exit 0
'@, [Text.UTF8Encoding]::new($false))
function Start-Capture([string]$OutPath,[string]$ErrPath,[string]$Ready,[string]$Release) {
  $info=[Diagnostics.ProcessStartInfo]::new($helper)
  $info.UseShellExecute=$false; $info.CreateNoWindow=$true
  Set-CaptureTestArguments $info @($OutPath,$ErrPath,$root,'--','powershell','-NoProfile','-File',$child,$Ready,$Release,'中文 "引号" 反斜杠\','')
  return [Diagnostics.Process]::Start($info)
}
$out=Join-Path $root 'stdout.log'; $err=Join-Path $root 'stderr.log'
$ready=Join-Path $root 'ready'; $release=Join-Path $root 'release'
$process=Start-Capture $out $err $ready $release
try {
  $deadline=[DateTime]::UtcNow.AddSeconds(10)
  while(-not ([IO.File]::Exists($ready) -and (Get-Item $out).Length -gt 0 -and (Get-Item $err).Length -gt 0)) {
    if($process.HasExited -or [DateTime]::UtcNow -gt $deadline){throw '退出前双管道未增量可见'}
    Start-Sleep -Milliseconds 20
  }
  if($process.HasExited){throw '未证明构建仍在运行'}
  if([IO.File]::ReadAllText($ready) -cne '中文 "引号" 反斜杠\|'){throw '中文、引号、反斜杠或空参数被改写'}
  Write-Output 'PASS 构建等待屏障期间双管道已落盘，空格参数保持完整'
} finally {
  [IO.File]::WriteAllText($release,'release')
  if(-not $process.WaitForExit(10000)){$process.Kill($true); throw '辅助程序收尾超时'}
}
if($process.ExitCode -ne 0){throw "增量检查退出码 $($process.ExitCode)"}
$process.Dispose()
$sha=(Get-FileHash $out).Hash
$absent=Join-Path $root 'must-not-start'
$process=Start-Capture $out (Join-Path $root 'unused.log') $absent $release
try {if(-not $process.WaitForExit(10000)){$process.Kill($true);throw '独占拒绝超时'}
  if($process.ExitCode -ne 125 -or [IO.File]::Exists($absent) -or (Get-FileHash $out).Hash -ne $sha){throw '既有日志被覆盖或命令已启动'}
} finally {$process.Dispose()}
Write-Output 'PASS 既有日志拒绝，原字节不变且命令未启动'
$errSha=(Get-FileHash $err).Hash
$process=Start-Capture (Join-Path $root 'fresh-stdout.log') $err $absent $release
try {if(-not $process.WaitForExit(10000)){$process.Kill($true);throw 'stderr 独占拒绝超时'}
  if($process.ExitCode -ne 125 -or [IO.File]::Exists($absent) -or (Get-FileHash $err).Hash -ne $errSha){throw '既有 stderr 被覆盖或命令已启动'}
} finally {$process.Dispose()}
Write-Output 'PASS 既有 stderr 拒绝，原字节不变且命令未启动'
$orphanScript=Join-Path $root 'orphan root.ps1'
$pidFile=Join-Path $root 'owned-child.pid'
[IO.File]::WriteAllText($orphanScript, @'
param([string]$PidFile)
$child=Start-Process -FilePath powershell -ArgumentList '-NoProfile -Command "Start-Sleep -Seconds 30"' -PassThru -WindowStyle Hidden
[IO.File]::WriteAllText($PidFile,[string]$child.Id)
exit 0
'@, [Text.UTF8Encoding]::new($false))
$info=[Diagnostics.ProcessStartInfo]::new($helper)
$info.UseShellExecute=$false; $info.CreateNoWindow=$true
Set-CaptureTestArguments $info @((Join-Path $root 'orphan-out.log'),(Join-Path $root 'orphan-err.log'),$root,'--','powershell','-NoProfile','-File',$orphanScript,$pidFile)
$process=[Diagnostics.Process]::Start($info)
try {
  if(-not $process.WaitForExit(15000)){$process.Kill($true);throw '后代回收超时'}
  if($process.ExitCode -ne 125 -or -not [IO.File]::Exists($pidFile)){throw '未完成子树被冒充成功或夹具未启动'}
  $childId=[int][IO.File]::ReadAllText($pidFile)
  $alive=$false
  try {$owned=[Diagnostics.Process]::GetProcessById($childId); $alive=-not $owned.HasExited; $owned.Dispose()} catch [ArgumentException] {}
  if($alive){throw '自有后代未回收'}
} finally {$process.Dispose()}
Write-Output 'PASS 根退出0但后代活跃时拒绝成功，并确认自有后代已退出'
