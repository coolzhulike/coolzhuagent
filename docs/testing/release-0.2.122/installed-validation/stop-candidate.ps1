$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-browser-multiline/candidate-processes.json') | ConvertFrom-Json
$taskProcesses=@()
foreach($taskKind in @('shell','web')) {
 $taskProcess=Get-Process -Id $taskReceipt.($taskKind+'_pid')
 if($taskProcess.Path -ine $taskReceipt.($taskKind+'_binary') -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.($taskKind+'_started')).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.($taskKind+'_sha256')) {throw '候选程序三项身份不符，保留全部进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses) {Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
$taskReceipt|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-122/stopped-candidate-receipt.json') -Encoding utf8
Write-Output '三项身份一致的自有候选后台与壳已停止'
