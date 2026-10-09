$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskCase=if($env:SNAPSHOT_CASE){$env:SNAPSHOT_CASE}else{'candidate'}
$taskFolder=Join-Path $taskRoot "tmp/2026-10-08-config-read-snapshot/$taskCase"
$taskBinary='C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe'
$taskVerified=Get-Content -Raw (Join-Path $taskRoot 'tmp/2026-10-08-release-112/installed-112-verification.json') | ConvertFrom-Json
if((Get-FileHash -LiteralPath $taskBinary).Hash -ne $taskVerified.shell_sha256){throw '正式壳摘要不符'}
$env:COOLZHU_RUNTIME_DIR=Join-Path $taskFolder 'workspace'
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $taskFolder 'isolated-input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8768/'
Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$taskShell=Start-Process -FilePath $taskBinary -ArgumentList '--show-console' -WorkingDirectory (Join-Path $taskFolder 'workspace') -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'shell-out.log') -RedirectStandardError (Join-Path $taskFolder 'shell-err.log') -PassThru
$taskFresh=Get-Process -Id $taskShell.Id
@{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskVerified.shell_sha256;port=8768;workspace=$env:COOLZHU_RUNTIME_DIR}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskFolder 'shell-receipt.json') -Encoding utf8
Write-Output '隔离候选原生窗口已正常启动，原正式窗口未修改'
