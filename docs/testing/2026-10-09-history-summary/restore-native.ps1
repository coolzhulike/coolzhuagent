$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-09-history-summary'
$taskCandidate=Get-Content -Raw -LiteralPath (Join-Path $taskFolder 'candidate-shell-receipt.json') | ConvertFrom-Json
$taskOriginal=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-123/installed-123-standard-processes.json') | ConvertFrom-Json
$taskProcess=Get-Process -Id $taskCandidate.pid
if($taskProcess.Path -ine $taskCandidate.binary -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskCandidate.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskCandidate.sha256){throw '候选壳身份不符，保留'}
$taskWeb=Get-Process -Id $taskOriginal.web_pid
if($taskWeb.Path -ine $taskOriginal.web_binary -or $taskWeb.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskOriginal.web_started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskWeb.Path).Hash -ine $taskOriginal.web_sha256){throw '原正式后台身份不符，保留'}
Stop-Process -Id $taskProcess.Id
Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
$env:COOLZHU_RUNTIME_DIR=$taskOriginal.workspace
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $env:LOCALAPPDATA 'CoolzhuAgent/input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$taskShell=Start-Process -FilePath $taskOriginal.shell_binary -ArgumentList '--show-console' -WorkingDirectory $env:COOLZHU_RUNTIME_DIR -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'original-restored-shell-out.log') -RedirectStandardError (Join-Path $taskFolder 'original-restored-shell-err.log') -PassThru
$taskFresh=Get-Process -Id $taskShell.Id
@{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskOriginal.shell_sha256;port=8765;workspace=$env:COOLZHU_RUNTIME_DIR;original_web_pid=$taskWeb.Id;original_web_unchanged=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskFolder 'original-restored-receipt.json') -Encoding utf8
Set-Content -LiteralPath (Join-Path $taskFolder 'stop-native') -Value '已恢复正式123壳，停止隔离候选服务'
Set-Content -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-browser-pending123b/stop-server') -Value '本轮时序验收已完成记录，停止自己启动的页面服务器'
Write-Output '原正式123后台身份保持不变，原生壳已正常恢复至8765'
