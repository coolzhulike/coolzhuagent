$ErrorActionPreference='Stop'
$taskReceipt=Get-Content -Raw 'tmp/2026-10-08-release-115/original-restored-receipt.json' | ConvertFrom-Json
$taskProcess=Get-Process -Id $taskReceipt.shell_pid
if([IO.Path]::GetFullPath($taskProcess.Path) -ine [IO.Path]::GetFullPath($taskReceipt.shell_binary) -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.shell_started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.shell_sha256){throw '原壳身份不一致，保留'}
$taskReceipt | ConvertTo-Json | Set-Content -Encoding utf8 'tmp/2026-10-09-native-picker115/stopped-shell-receipt.json'
Stop-Process -Id $taskProcess.Id
Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
Write-Output '仅停止已核身份的闲置正式壳；后台、原文件和消息保留'
