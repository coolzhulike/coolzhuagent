$ErrorActionPreference='Stop'
$taskReceipt=Get-Content -Raw 'tmp/2026-10-09-release-116/original-restored-receipt.json' | ConvertFrom-Json
$taskWeb=Get-Process -Id $taskReceipt.web_pid
if($taskWeb.Path -ine $taskReceipt.web_binary -or $taskWeb.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.web_started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskWeb.Path).Hash -ine $taskReceipt.web_sha256){throw '原后台身份不一致，保留'}
$env:COOLZHU_RUNTIME_DIR=$taskReceipt.workspace
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$taskShell=Start-Process -FilePath $taskReceipt.shell_binary -ArgumentList '--show-console' -WorkingDirectory $taskReceipt.workspace -WindowStyle Hidden -PassThru
$taskFresh=Get-Process -Id $taskShell.Id
if($taskFresh.Path -ine $taskReceipt.shell_binary -or (Get-FileHash -LiteralPath $taskFresh.Path).Hash -ine $taskReceipt.shell_sha256){throw '恢复壳身份不一致，保留'}
$taskReceipt.shell_pid=$taskFresh.Id
$taskReceipt.shell_started=$taskFresh.StartTime.ToUniversalTime().ToString('o')
$taskReceipt | ConvertTo-Json | Set-Content -Encoding utf8 'tmp/2026-10-09-release-116/original-restored-receipt.json'
Write-Output '正式116原壳已恢复，原SWE后台保持'
