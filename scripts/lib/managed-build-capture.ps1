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

# MSVC 工具链（cl/link）结束时会拉起常驻遥测服务 vctip.exe：它不是构建产物却留在
# Windows Job 内，可把根进程退出后的受管排空拖到超过硬上限而误判采集失败（172 现场）。
# vctip 自带 -unstable:mark 是幂等的受控开关，标记后工具链直接跳过拉起
#（link.exe: "Not launching VCTIP: it is flagged as unstable"）。这不按进程名放行——
# 仍进入 Job 的任何后代照旧由排空门禁计数拒绝。

# 只定位实际 linker 所属工具链的 vctip：vswhere -latest（与 cargo/cc 相同的选取口径），
# 缺失 vswhere 时退回默认安装布局的最新 MSVC 根；枚举范围限于 Hostx64\x64。
function Get-LinkerToolsetVctip {
  $roots = @()
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (Test-Path -LiteralPath $vswhere) {
    foreach ($installation in @(& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)) {
      if ($installation) { $roots += Join-Path $installation.TrimEnd('\') 'VC\Tools\MSVC' }
    }
  }
  if (-not $roots) {
    foreach ($pattern in @((Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\*\*\VC\Tools\MSVC'),
                          (Join-Path $env:ProgramFiles 'Microsoft Visual Studio\*\*\VC\Tools\MSVC'))) {
      $candidate = @(Get-Item $pattern -ErrorAction SilentlyContinue | Select-Object -Last 1)
      if ($candidate) { $roots += $candidate.FullName }
    }
  }
  foreach ($msvcRoot in ($roots | Select-Object -Unique)) {
    Get-ChildItem (Join-Path $msvcRoot '*\bin\Hostx64\x64\vctip.exe') -File -ErrorAction SilentlyContinue
  }
}

# 作用域化抑制：只处理 Get-LinkerToolsetVctip 命中的二进制，并逐条记录原状态账本。
# 原本已 unstable / 查询失败者一律不动、不猜测清除；返回账本供 Restore 选择性还原。
function Disable-VctipTelemetrySpawn {
  $entries = @()
  foreach ($helper in @(Get-LinkerToolsetVctip)) {
    $entry = [pscustomobject]@{ path = $helper.FullName; prior = 'unknown'; changed = $false; error = $null }
    $info = & $helper.FullName -unstable:info 2>&1
    $infoText = ($info | Out-String)
    if ($infoText -match "VCTIP is 'stable'") { $entry.prior = 'stable' }
    elseif ($infoText -match "VCTIP is 'unstable'") { $entry.prior = 'unstable' }
    if ($entry.prior -eq 'stable') {
      $output = & $helper.FullName -unstable:mark 2>&1
      if ($LASTEXITCODE -eq 0) { $entry.changed = $true } else { $entry.error = ('mark failed: ' + ($output -join ' ')) }
    }
    $entries += $entry
  }
  ,$entries
}

# 只还原账本中本轮 changed=$true（原 stable）的条目；已 unstable/未知/失败项绝不动。
function Restore-VctipTelemetrySpawn {
  param([object[]]$Ledger)
  foreach ($entry in @($Ledger)) {
    if (-not $entry.changed) { continue }
    $output = & $entry.path -unstable:clear 2>&1
    if ($LASTEXITCODE -ne 0) { Write-Host ('build-capture: vctip 还原失败（保持 unstable）：' + $entry.path + ' ' + ($output -join ' ')) }
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
  $vctipLedger=$null
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
    # 受控遥测配置：仅抑制实际 linker 工具链的 vctip，原状态入账、本轮标记者 finally 还原；
    # 排空门禁仍是终局裁决，绝不因遥测进程而放宽。
    $vctipLedger = @(Disable-VctipTelemetrySpawn)
    $vctipChanged = @($vctipLedger | Where-Object { $_.changed }).Count
    $vctipErrored = @($vctipLedger | Where-Object { $_.error }).Count
    if ($vctipLedger.Count -gt 0) { Write-Host ('build-capture: vctip 作用域 {0} 项，本轮标记 {1}，失败 {2}' -f $vctipLedger.Count, $vctipChanged, $vctipErrored) }
    if ($vctipErrored -gt 0) { Write-Host ('build-capture: vctip 标记失败仅告警，排空门禁不变：' + (($vctipLedger | Where-Object { $_.error } | ForEach-Object { $_.path + ' ' + $_.error }) -join '; ')) }
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
    if ($vctipLedger) { Restore-VctipTelemetrySpawn -Ledger $vctipLedger }
    if ($process) { $process.Dispose() }
    if ($diagnostic) { $diagnostic.Dispose() }
    $lease.Dispose()
  }
}
