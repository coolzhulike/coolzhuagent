$ErrorActionPreference='Stop'
$taskReceipt=Get-Content -Raw -LiteralPath 'tmp/2026-10-09-release-116/original-restored-receipt.json' | ConvertFrom-Json
$taskProcesses=@()
foreach($taskKind in @('shell','web')){
 $taskPid=$taskReceipt.($taskKind+'_pid')
 $taskProcess=Get-Process -Id $taskPid
 if($taskProcess.Path -ine $taskReceipt.($taskKind+'_binary') -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.($taskKind+'_started')).ToUniversalTime().Ticks){throw '正式116进程身份不匹配，保留全部进程'}
 if((Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.($taskKind+'_sha256')){throw '正式116文件摘要不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id; Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
$taskReceipt|ConvertTo-Json|Set-Content -LiteralPath 'tmp/2026-10-09-release-117/stopped116-receipt.json' -Encoding utf8
Write-Output '身份相符的自有闲置116壳与后台已正常停止'
