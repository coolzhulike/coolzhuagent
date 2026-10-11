$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-memory-data-boundary'
$taskCleanup=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'memory-cleanup.json') | ConvertFrom-Json
if(-not $taskCleanup.passed -or -not $taskCleanup.original_ids_preserved){throw '真实任务及自有记忆清理尚未核对'}
$taskOld=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'candidate-processes.json') | ConvertFrom-Json
if($taskOld.stage -ne '记忆资料信任边界源码候选' -or $taskOld.port -ne 8765){throw '候选身份收据不符'}
$taskProcesses=@()
foreach($taskIdentity in @(@{pid=$taskOld.shell_pid;path=$taskOld.shell_binary;started=$taskOld.shell_started;sha=$taskOld.shell_sha256},@{pid=$taskOld.web_pid;path=$taskOld.web_binary;started=$taskOld.web_started;sha=$taskOld.web_sha256})) {
 $taskProcess=Get-Process -Id $taskIdentity.pid
 if($taskProcess.Path -ine $taskIdentity.path -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskIdentity.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskIdentity.sha){throw '候选进程身份不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
@{previous=$taskOld;identity_verified=$true;purpose='候选真实任务已终态，恢复Program Files正式125'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskTmp 'stopped-candidate.json') -Encoding utf8
Write-Output '自有候选后台与壳已按身份停止'
