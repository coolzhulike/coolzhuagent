$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-09-release-117/formal'
$taskReady=Get-Content -Raw (Join-Path $taskFolder 'ready.json')|ConvertFrom-Json
$taskVerified=Get-Content -Raw (Join-Path $taskRoot 'tmp/2026-10-09-release-117/installed-117-verification.json')|ConvertFrom-Json
$taskBinary='C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe'
if((Get-FileHash -LiteralPath $taskBinary).Hash -ine $taskVerified.shell_sha256){throw '正式壳摘要不符'}
$env:COOLZHU_RUNTIME_DIR=$taskReady.workspace
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT=Join-Path $taskFolder 'input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:'+$taskReady.port+'/'
foreach($taskKey in @('COOLZHU_BROWSER_NAV_DIAGNOSTICS','COOLZHU_WEB_STATIC_ROOT','WEBVIEW2_USER_DATA_FOLDER','WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')){Remove-Item ('Env:'+$taskKey) -ErrorAction SilentlyContinue}
$taskShell=Start-Process -FilePath $taskBinary -ArgumentList '--show-console' -WorkingDirectory $taskReady.workspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'native-shell-out.log') -RedirectStandardError (Join-Path $taskFolder 'native-shell-err.log') -PassThru
$taskFresh=Get-Process -Id $taskShell.Id
@{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskVerified.shell_sha256;port=$taskReady.port;workspace=$taskReady.workspace}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskFolder 'native-shell-receipt.json') -Encoding utf8
Write-Output '117正式壳已连接117正式后台的隔离验收工程'
