# 受管采集入口：调用方必须提供冻结构建所核验的辅助程序身份。
if (-not ('CoolzhuCapture.DiagnosticDrain' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Threading.Tasks;
namespace CoolzhuCapture {
  public static class DiagnosticDrain {
    // 写入失败后仍排空管道，避免生产者阻塞；调用方必须拒绝该回执。
    public static Task<Exception> Start(Stream source, Stream destination) {
      return Task.Run<Exception>(() => {
        var buffer = new byte[81920];
        Exception failure = null;
        int count;
        while ((count = source.Read(buffer, 0, buffer.Length)) != 0) {
          if (failure == null) {
            try { destination.Write(buffer, 0, count); }
            catch (Exception error) { failure = error; }
          }
        }
        return failure;
      });
    }
  }
}
'@
}
function Set-ManagedCaptureArguments {
  param([Diagnostics.ProcessStartInfo]$Info, [string[]]$Arguments)
  if ($Info.PSObject.Properties['ArgumentList']) {
    foreach ($arg in $Arguments) { [void]$Info.ArgumentList.Add($arg) }
  } else {
    $quoted = foreach ($arg in $Arguments) {
      if ($arg.Length -gt 0 -and $arg -notmatch '[\s"]') { $arg; continue }
      '"' + [regex]::Replace([regex]::Replace($arg, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
    }
    $Info.Arguments = $quoted -join ' '
  }
}

function Wait-ManagedCaptureDiagnostic {
  param([Diagnostics.Process]$Process, [Threading.Tasks.Task]$Drain)
  while (-not $Process.WaitForExit(100)) {
    if ($Drain.IsFaulted -or $Drain.IsCanceled) {
      # 只终止本次启动的辅助程序；Windows Job 的关句柄回收仍需独立验收。
      try { $Process.Kill() } catch { if (-not $Process.HasExited) { throw } }
      if (-not $Process.WaitForExit(10000)) { throw 'BUILD-CAPTURE-DIAGNOSTIC: 读取失败且辅助程序退出未确认' }
      throw 'BUILD-CAPTURE-DIAGNOSTIC: 诊断读取任务失败，辅助程序已停止，回执无效'
    }
  }
}

function Invoke-ManagedBuildCapture {
  param(
    [Parameter(Mandatory=$true)][string]$HelperPath,
    [Parameter(Mandatory=$true)][string]$ExpectedHelperSha256,
    [Parameter(Mandatory=$true)][string]$Command,
    [AllowEmptyCollection()][string[]]$Arguments=@(),
    [Parameter(Mandatory=$true)][string]$WorkingDirectory,
    [Parameter(Mandatory=$true)][string]$StdoutPath,
    [Parameter(Mandatory=$true)][string]$StderrPath
  )
  if ($ExpectedHelperSha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'BUILD-CAPTURE-IDENTITY: 辅助程序摘要无效' }
  $helper=(Resolve-Path -LiteralPath $HelperPath -ErrorAction Stop).Path
  # Windows 共享方式拒绝协作写入/删除；前后摘要也不能代替原始构建来源证明。
  $lease=[IO.FileStream]::new($helper,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
  $process=$null
  $diagnostic=$null
  try {
    if ((Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash -ine $ExpectedHelperSha256) { throw 'BUILD-CAPTURE-IDENTITY: 辅助程序与冻结摘要不一致，未启动命令' }
    $info=[Diagnostics.ProcessStartInfo]::new($helper)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardError=$true
    $diagnosticPath=$StdoutPath+'.helper-'+[guid]::NewGuid().ToString('N')+'.stderr.log'
    $diagnostic=[IO.FileStream]::new($diagnosticPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    $owner=[Diagnostics.Process]::GetCurrentProcess()
    try { $ownerArguments=@('--owner',[string]$owner.Id,[string]$owner.StartTime.ToFileTimeUtc()) } finally { $owner.Dispose() }
    Set-ManagedCaptureArguments -Info $info -Arguments ($ownerArguments+@($StdoutPath,$StderrPath,$WorkingDirectory,'--',$Command)+$Arguments)
    $started=(Get-Date).ToUniversalTime().ToString('o')
    $process=[Diagnostics.Process]::Start($info)
    # 仅辅助程序自身诊断进入此管道；固定缓冲异步落盘，避免等待退出时堵住诊断管道。
    $diagnosticCopy=[CoolzhuCapture.DiagnosticDrain]::Start($process.StandardError.BaseStream,$diagnostic)
    Wait-ManagedCaptureDiagnostic -Process $process -Drain $diagnosticCopy
    $diagnosticFailure=$diagnosticCopy.GetAwaiter().GetResult()
    if ($diagnosticFailure) { throw ('BUILD-CAPTURE-DIAGNOSTIC: 诊断写入失败，回执无效：'+$diagnosticFailure.Message) }
    $diagnostic.Flush($true)
    $code=$process.ExitCode
    if ((Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash -ine $ExpectedHelperSha256) { throw 'BUILD-CAPTURE-IDENTITY: 采集期间辅助程序身份变化，回执无效' }
    [pscustomobject]@{ exit_code=$code; helper_path=$helper; helper_sha256=$ExpectedHelperSha256.ToLowerInvariant(); started_utc=$started; stdout_path=$StdoutPath; stderr_path=$StderrPath; helper_diagnostic_path=$diagnosticPath }
  } finally {
    if ($process) { $process.Dispose() }
    if ($diagnostic) { $diagnostic.Dispose() }
    $lease.Dispose()
  }
}
