$ErrorActionPreference='Stop'
$taskFolder='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent/tmp/2026-10-08-legacy-model-limit/ui'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskFolder 'native-shell-receipt.json') | ConvertFrom-Json
$taskProcess=Get-Process -Id $taskReceipt.pid
if($taskProcess.Path -ine $taskReceipt.binary -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.started).ToUniversalTime().Ticks){throw '隔离壳身份不匹配，保留进程'}
if((Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.sha256){throw '隔离壳摘要不匹配'}
Stop-Process -Id $taskProcess.Id
Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
Write-Output '已停止身份核验相符的隔离配置验收壳'
