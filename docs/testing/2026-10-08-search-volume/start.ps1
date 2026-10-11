$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-08-search-volume'
$taskWorkspace=Join-Path $taskFolder 'workspace'
if(Get-NetTCPConnection -LocalPort 64605 -State Listen -ErrorAction SilentlyContinue){throw '64605已有服务，保留原实例'}
$env:COOLZHU_RUNTIME_DIR=$taskWorkspace
$env:COOLZHU_WEB_SESSION_DB=Join-Path $taskWorkspace '.coolzhu/web-sessions.sqlite3'
$env:COOLZHU_WEB_SESSION_STORE=Join-Path $taskWorkspace '.coolzhu/web-sessions.json'
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $taskFolder 'isolated-input-safety'
$env:COOLZHU_LOG_DIR=Join-Path $taskFolder 'logs'
$taskBinary='C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe'
if((Get-FileHash -LiteralPath $taskBinary).Hash -ne '13cce9a95ff590a139a2fe9890045b94f0399ad88375f758535f27e49d1c0593'){throw '097正式EXE摘要不一致'}
$taskProc=Start-Process -FilePath $taskBinary -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'web-out.log') -RedirectStandardError (Join-Path $taskFolder 'web-err.log') -PassThru
$taskFresh=Get-Process -Id $taskProc.Id
@{pid=$taskFresh.Id;path=$taskFresh.Path;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');workspace=$taskWorkspace;port=64605;version='0.2.97'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskFolder 'process.json') -Encoding utf8
Write-Output '独立容量验收正式后端已启动'
