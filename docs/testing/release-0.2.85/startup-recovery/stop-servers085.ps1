$ErrorActionPreference='Stop'
$taskDir=Split-Path -Parent $PSCommandPath
$taskOwned=@()
foreach($taskName in @('server-process.json','boundary-server-process.json')){
 $taskClaim=Get-Content -Raw -LiteralPath (Join-Path $taskDir $taskName) | ConvertFrom-Json
 $taskProcess=Get-Process -Id $taskClaim.pid
 $taskCim=Get-CimInstance Win32_Process -Filter ('ProcessId='+$taskClaim.pid)
 $taskExpectedPath=if($taskClaim.path){$taskClaim.path}else{'C:\Python314\python.exe'}
 if($taskCim.ExecutablePath -ine $taskExpectedPath -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetimeoffset]$taskClaim.created).UtcDateTime.Ticks -or $taskCim.CommandLine -cne $taskClaim.command -or (Get-FileHash -LiteralPath $taskCim.ExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskClaim.sha256 -or (Get-FileHash -LiteralPath $taskClaim.script -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskClaim.script_sha256){throw '测试服务器完整身份改变，保留进程'}
 $taskOwned+=@{pid=$taskProcess.Id;path=$taskCim.ExecutablePath;created=$taskProcess.StartTime.ToUniversalTime().ToString('o');command=$taskCim.CommandLine;sha256=$taskClaim.sha256;script=$taskClaim.script;script_sha256=$taskClaim.script_sha256}
}
foreach($taskClaim in $taskOwned){Stop-Process -Id $taskClaim.pid}
@{stage='完成正式验收后按完整身份停止自有测试服务器';stopped=$taskOwned;observed_utc=(Get-Date).ToUniversalTime().ToString('o')} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $taskDir 'stopped-085-test-servers.json') -Encoding utf8
Write-Output '两项自有测试服务已按完整身份停止，事件及原安全库保留。'
