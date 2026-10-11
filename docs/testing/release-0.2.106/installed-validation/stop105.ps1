$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-08-release-106'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-08-release-105/installed-105-standard-processes.json') | ConvertFrom-Json
$taskIdle=Get-Content -Raw -LiteralPath (Join-Path $taskFolder 'idle-before-install.json') | ConvertFrom-Json
if($taskIdle.acceptance.active_runs -ne 0 -or $taskIdle.daily.active_runs -ne 0){throw '仍有实际任务，保留进程'}
$taskQuery='session_id=session-1791131217833&room_id=room-1791131523339'
if((Invoke-RestMethod ('http://127.0.0.1:8765/api/terminal?'+$taskQuery) -TimeoutSec 10).active){throw '仍有终端，保留进程'}
if((Invoke-RestMethod 'http://127.0.0.1:8765/api/lsp/status' -TimeoutSec 10).running){throw '仍有LSP，保留进程'}
$taskProcesses=@()
foreach($taskKind in @('web','shell')){
 $taskId=$taskReceipt.($taskKind+'_pid')
 $taskPath=$taskReceipt.($taskKind+'_binary')
 $taskTime=$taskReceipt.($taskKind+'_started')
 $taskSha=$taskReceipt.($taskKind+'_sha256')
 $taskProc=Get-Process -Id $taskId
 $taskMeta=Get-CimInstance Win32_Process -Filter ('ProcessId='+$taskId)
 if($taskMeta.ExecutablePath -ine $taskPath -or $taskProc.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskTime).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskPath).Hash -ne $taskSha){throw '进程身份变化，保留全部进程'}
 $taskProcesses+=,$taskProc
}
foreach($taskProc in $taskProcesses){Stop-Process -Id $taskProc.Id;$taskProc.WaitForExit(10000) | Out-Null}
Write-Output '仅停止已验证身份的105后台与桌面壳，准备正常重启验收'
