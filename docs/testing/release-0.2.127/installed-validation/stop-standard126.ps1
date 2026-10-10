$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-10-release-127'
$taskReady=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'package-127-verification.json') | ConvertFrom-Json
if(-not $taskReady.package_safe -or $taskReady.version -ne '0.2.127'){throw '正常新包尚未核验，不停止正式126'}
$taskOld=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-10-release-126/installed-126-standard-processes.json') | ConvertFrom-Json
$taskProcesses=@()
foreach($taskIdentity in @(@{pid=$taskOld.shell_pid;path=$taskOld.shell_binary;started=$taskOld.shell_started;sha=$taskOld.shell_sha256},@{pid=$taskOld.web_pid;path=$taskOld.web_binary;started=$taskOld.web_started;sha=$taskOld.web_sha256})) {
 $taskProcess=Get-Process -Id $taskIdentity.pid
 if($taskProcess.Path -ine $taskIdentity.path -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskIdentity.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskIdentity.sha){throw '正式126实例身份不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
@{previous=$taskOld;identity_verified=$true;purpose='正常批次127安装前停止自有正式126'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskTmp 'stopped-formal126.json') -Encoding utf8
