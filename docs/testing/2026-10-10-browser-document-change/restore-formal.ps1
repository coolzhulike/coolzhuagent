$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-10-browser-document-change'
python (Join-Path $taskRoot 'tmp/2026-10-10-release-126/preflight-idle.py')
if($LASTEXITCODE -ne 0){throw '候选任务未结束，保留候选实例'}
$taskOld=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'candidate-processes.json') | ConvertFrom-Json
$taskProcesses=@()
foreach($taskIdentity in @(@{pid=$taskOld.shell_pid;path=$taskOld.shell_binary;started=$taskOld.shell_started;sha=$taskOld.shell_sha256},@{pid=$taskOld.web_pid;path=$taskOld.web_binary;started=$taskOld.web_started;sha=$taskOld.web_sha256})) {
 $taskProcess=Get-Process -Id $taskIdentity.pid
 if($taskProcess.Path -ine $taskIdentity.path -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskIdentity.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskIdentity.sha){throw '候选实例身份不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
@{previous=$taskOld;identity_verified=$true;candidate_stopped=$true} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskTmp 'stopped-candidate.json') -Encoding utf8
& (Join-Path $taskRoot 'tmp/2026-10-10-release-126/start-standard126.ps1')
Copy-Item -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-10-release-126/installed-126-standard-processes.json') -Destination (Join-Path $taskTmp 'restored-formal126.json')
Write-Output '源码候选已按身份停止，正式126原工作区已恢复'
