$ErrorActionPreference='Stop'
$taskDir=Split-Path -Parent $PSCommandPath
$taskStart=Get-Content -Raw -LiteralPath (Join-Path $taskDir 'restored-daily-085-process.json') | ConvertFrom-Json
$taskCheckPath='C:/Users/zhupu/AppData/Local/CoolzhuAgent/logs/package-launcher/package-selfcheck-last.json'
$taskCheck=Get-Content -Raw -LiteralPath $taskCheckPath | ConvertFrom-Json
$taskStartMs=([datetimeoffset]$taskStart.daily_launcher.created).ToUnixTimeMilliseconds()
if(-not $taskCheck.ok -or $taskCheck.timestamp_ms -lt $taskStartMs -or $taskCheck.resolved_launch_paths.observed.build_version -notlike 'ae87c7c83db3*'){throw '启动自检不是当前0.2.85'}
if($taskCheck.resolved_launch_paths.requested.workspace_root -ine 'C:\Users\zhupu\coolzhuagent' -or $taskCheck.resolved_launch_paths.input_safety_state_root -ine 'C:\Users\zhupu\AppData\Local\CoolzhuAgent\input-safety'){throw '工程或安全库绑定不同'}
$taskClaims=@(@{pid=$taskCheck.web_console_pid;path='C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe'},@{pid=$taskCheck.tauri_pid;path='C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe'})
$taskActual=@()
foreach($taskClaim in $taskClaims){
 $taskProcess=Get-Process -Id $taskClaim.pid
 if($taskProcess.Path -ine $taskClaim.path -or ([datetimeoffset]$taskProcess.StartTime.ToUniversalTime()).ToUnixTimeMilliseconds() -lt $taskStartMs){throw '实际进程不是本轮日常启动'}
 $taskActual+=@{pid=$taskProcess.Id;path=$taskProcess.Path;created=$taskProcess.StartTime.ToUniversalTime().ToString('o');sha256=(Get-FileHash -LiteralPath $taskProcess.Path -Algorithm SHA256).Hash.ToLowerInvariant()}
}
Copy-Item -LiteralPath $taskCheckPath -Destination (Join-Path $taskDir 'restored-daily-085-selfcheck.json')
@{stage='正常桌面启动0.2.85核验通过';observed_utc=(Get-Date).ToUniversalTime().ToString('o');processes=$taskActual;selfcheck_ok=$taskCheck.ok;source_commit='ae87c7c83db3c661eca5a0b15eb6280ab37cad2a';workspace=$taskCheck.resolved_launch_paths.observed.workspace;safety_root=$taskCheck.resolved_launch_paths.input_safety_state_root} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $taskDir 'restored-daily-085-verification.json') -Encoding utf8
Write-Output '当前桌面启动通过：Program Files进程、0.2.85源码、原工程及原输入安全库一致。'

