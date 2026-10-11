$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-118/installed-118-standard-processes.json')|ConvertFrom-Json
$taskProcesses=@()
foreach($taskKind in @('shell','web')){
 $taskProcess=Get-Process -Id $taskReceipt.($taskKind+'_pid')
 if($taskProcess.Path -ine $taskReceipt.($taskKind+'_binary') -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.($taskKind+'_started')).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.($taskKind+'_sha256')){throw '正式118三项身份不符，保留全部进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
$taskReceipt|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-119/stopped118-receipt.json') -Encoding utf8
Write-Output '身份一致的自有118后台及壳已正常停止'
