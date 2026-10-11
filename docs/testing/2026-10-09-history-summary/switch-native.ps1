$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-123/installed-123-standard-processes.json') | ConvertFrom-Json
$taskProcess=Get-Process -Id $taskReceipt.shell_pid
if($taskProcess.Path -ine $taskReceipt.shell_binary -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.shell_started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.shell_sha256){throw '原壳身份不符，保留'}
Stop-Process -Id $taskProcess.Id
Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-09-history-summary'
$env:COOLZHU_RUNTIME_DIR=Join-Path $taskFolder 'candidate/workspace'
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $taskFolder 'candidate/isolated-input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8768/'
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$taskShell=Start-Process -FilePath $taskReceipt.shell_binary -ArgumentList '--show-console' -WorkingDirectory $env:COOLZHU_RUNTIME_DIR -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'candidate-shell-out.log') -RedirectStandardError (Join-Path $taskFolder 'candidate-shell-err.log') -PassThru
$taskFresh=Get-Process -Id $taskShell.Id
@{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskReceipt.shell_sha256;port=8768;workspace=$env:COOLZHU_RUNTIME_DIR} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskFolder 'candidate-shell-receipt.json') -Encoding utf8
Write-Output '已用同一正式壳正常打开候选服务；原正式后台保留'
