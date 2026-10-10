$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-09-ui-error'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-attachment-integrity/original-restored-receipt.json') | ConvertFrom-Json
$taskProcess=Get-Process -Id $taskReceipt.pid
if($taskProcess.Path -ine $taskReceipt.binary -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.sha256){throw '正式壳身份不符，保留'}
Stop-Process -Id $taskProcess.Id
Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
$env:COOLZHU_RUNTIME_DIR=Join-Path $taskRoot 'tmp/2026-10-09-attachment-integrity/candidate-v2/workspace'
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $taskRoot 'tmp/2026-10-09-attachment-integrity/candidate-v2/input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8768/'
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$taskShell=Start-Process -FilePath $taskReceipt.binary -ArgumentList '--show-console' -WorkingDirectory $env:COOLZHU_RUNTIME_DIR -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'shell-out.log') -RedirectStandardError (Join-Path $taskFolder 'shell-err.log') -PassThru
$taskFresh=Get-Process -Id $taskShell.Id
@{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskReceipt.sha256;port=8768;workspace=$env:COOLZHU_RUNTIME_DIR} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskFolder 'candidate-shell-receipt.json') -Encoding utf8
Write-Output '候选壳正常打开，原正式后台保留'
