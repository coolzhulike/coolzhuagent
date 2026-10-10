$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskOld=Get-Content -Raw (Join-Path $taskRoot 'tmp/2026-10-09-release-123/installed-123-standard-processes.json') | ConvertFrom-Json
$taskShell=Get-Content -Raw (Join-Path $taskRoot 'tmp/2026-10-09-ui-error/original-restored-receipt.json') | ConvertFrom-Json
$taskProcesses=@()
foreach($taskIdentity in @(@{pid=$taskShell.pid;path=$taskShell.binary;started=$taskShell.started;sha=$taskShell.sha256},@{pid=$taskOld.web_pid;path=$taskOld.web_binary;started=$taskOld.web_started;sha=$taskOld.web_sha256})) {
 $taskProcess=Get-Process -Id $taskIdentity.pid
 if($taskProcess.Path -ine $taskIdentity.path -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskIdentity.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskIdentity.sha){throw '旧正式实例身份不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
@{shell=$taskShell;web_pid=$taskOld.web_pid;all_three_identity_fields_verified=$true;stopped_for_normal_install=$true} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $taskRoot 'tmp/2026-10-09-release-124/stopped-formal123.json') -Encoding utf8
Write-Output '旧123自有后台与最新壳正常停止，准备正常升级'
