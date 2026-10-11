$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-08-legacy-model-limit/ui'
$taskVerified=Get-Content -Raw (Join-Path $taskRoot 'tmp/2026-10-08-release-113/installed-113-verification.json') | ConvertFrom-Json
$taskBinary='C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe'
if((Get-FileHash -LiteralPath $taskBinary).Hash -ne $taskVerified.shell_sha256){throw '正式壳摘要不符'}
$env:COOLZHU_RUNTIME_DIR=Join-Path $taskFolder 'workspace'
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $taskFolder 'isolated-input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8768/'
Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$taskShell=Start-Process -FilePath $taskBinary -ArgumentList '--show-console' -WorkingDirectory (Join-Path $taskFolder 'workspace') -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'native-shell-out.log') -RedirectStandardError (Join-Path $taskFolder 'native-shell-err.log') -PassThru
$taskShellFresh=Get-Process -Id $taskShell.Id
@{pid=$taskShellFresh.Id;started=$taskShellFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskShellFresh.Path;sha256=$taskVerified.shell_sha256;port=8768;workspace=$env:COOLZHU_RUNTIME_DIR} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskFolder 'native-shell-receipt.json') -Encoding utf8
Write-Output '正式113壳已正常打开隔离设置验收页面'
