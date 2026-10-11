#requires -Version 7.0
param([Parameter(Mandatory=$true)][string]$HelperPath)
$ErrorActionPreference='Stop'
$helper=(Resolve-Path $HelperPath).Path
$root=Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/capture-fault-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$script=Join-Path $root 'child.ps1'
[IO.File]::WriteAllText($script,@'
param([string]$PidFile)
$child=Start-Process powershell -ArgumentList '-NoProfile -Command "Start-Sleep -Seconds 30"' -WindowStyle Hidden -PassThru
[IO.File]::WriteAllText($PidFile,"$PID,$($child.Id)")
$out=[Console]::OpenStandardOutput(); $err=[Console]::OpenStandardError()
$bytes=[Text.UTF8Encoding]::new($false).GetBytes(('x'*4096)+"`n")
while($true){$out.Write($bytes,0,$bytes.Length);$err.Write($bytes,0,$bytes.Length)}
'@,[Text.UTF8Encoding]::new($false))
foreach($pipe in @('stdout','stderr')) {
  $pidFile=Join-Path $root ($pipe+'.pid')
  $info=[Diagnostics.ProcessStartInfo]::new($helper)
  $info.UseShellExecute=$false; $info.CreateNoWindow=$true
  $info.Environment['COOLZHU_CAPTURE_TEST_FAIL_PIPE']=$pipe
  foreach($arg in @('--owner',[string]$PID,[string]([Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTimeUtc()),(Join-Path $root ($pipe+'-out.log')),(Join-Path $root ($pipe+'-err.log')),$root,'--','powershell','-NoProfile','-File',$script,$pidFile)){[void]$info.ArgumentList.Add($arg)}
  $process=[Diagnostics.Process]::Start($info)
  try {
    if(-not $process.WaitForExit(15000)){$process.Kill($true);throw '写入故障收尾超时'}
    if($process.ExitCode -ne 125 -or -not [IO.File]::Exists($pidFile)){throw '没有触发失败或子树夹具未执行'}
    foreach($id in ([IO.File]::ReadAllText($pidFile)-split ',')) {
      $alive=$false
      try {$owned=[Diagnostics.Process]::GetProcessById([int]$id);$alive=-not $owned.HasExited;$owned.Dispose()} catch [ArgumentException] {}
      if($alive){throw "自有进程未退出：$id"}
    }
  } finally {$process.Dispose()}
  Write-Output "PASS $pipe 写入故障返回125，根与后代均退出"
}
