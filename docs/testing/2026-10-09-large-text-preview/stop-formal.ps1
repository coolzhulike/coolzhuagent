$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-large-text-preview'
$taskReady=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'candidate-preflight.json') | ConvertFrom-Json
if(-not $taskReady.passed -or $taskReady.active_session_runs -ne 0){throw '原会话状态未核对，保留正式进程'}
$taskOld=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-memory-data-boundary/restored-formal125.json') | ConvertFrom-Json
$taskProcesses=@()
foreach($taskIdentity in @(@{pid=$taskOld.shell_pid;path=$taskOld.shell_binary;started=$taskOld.shell_started;sha=$taskOld.shell_sha256},@{pid=$taskOld.web_pid;path=$taskOld.web_binary;started=$taskOld.web_started;sha=$taskOld.web_sha256})) {
 $taskProcess=Get-Process -Id $taskIdentity.pid
 if($taskProcess.Path -ine $taskIdentity.path -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskIdentity.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskIdentity.sha){throw '正式实例身份不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
@{previous=$taskOld;identity_verified=$true;purpose='释放本任务自有正式预览进程，候选使用独立SQLite副本且不调用模型'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskTmp 'stopped-formal.json') -Encoding utf8
