$ErrorActionPreference='Stop'
$taskDir=Split-Path -Parent $PSCommandPath
$taskReceiptName=if($args[0] -eq 'installed'){'installed-085-standard-processes.json'}elseif($args[0] -eq 'debug'){'debug-processes.json'}else{'candidate-processes.json'}
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskDir $taskReceiptName) | ConvertFrom-Json
$taskOwned=@()
foreach($taskKind in @('web','shell')){
 $taskPidProperty=$taskKind+'_pid';$taskPathProperty=$taskKind+'_binary';$taskTimeProperty=$taskKind+'_started';$taskShaProperty=$taskKind+'_sha256'
 $taskProcess=Get-Process -Id $taskReceipt.$taskPidProperty
 if($taskProcess.Path -ine $taskReceipt.$taskPathProperty -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetimeoffset]$taskReceipt.$taskTimeProperty).UtcDateTime.Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash.ToLowerInvariant() -ne $taskReceipt.$taskShaProperty){throw '自有进程身份变化，保留进程'}
 $taskOwned+=@{kind=$taskKind;pid=$taskProcess.Id;path=$taskProcess.Path;created=$taskProcess.StartTime.ToUniversalTime().ToString('o')}
}
foreach($taskClaim in @($taskOwned | Sort-Object kind -Descending)){Stop-Process -Id $taskClaim.pid}
@{stage='按完整身份停止本轮已结束任务的自有测试配套进程';stopped=$taskOwned;observed_utc=(Get-Date).ToUniversalTime().ToString('o')} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskDir ('stopped-'+$taskReceiptName)) -Encoding utf8
Write-Output '本轮自有配套进程已结束；原安全库和测试服务器保留。'
