param([switch]$Formal)
$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-plugin-frozen-permission-candidate'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'candidate-process.json')|ConvertFrom-Json
$taskOld=Get-Process -Id $taskReceipt.pid
if($taskOld.Path -ine $taskReceipt.binary -or $taskOld.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskOld.Path).Hash -ine $taskReceipt.sha256){throw '候选后台三项身份不符，保留进程'}
$taskBinary=if($Formal){'C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe'}else{Join-Path $taskTmp 'bin/coolzhu-web-console.exe'}
$taskHash=(Get-FileHash -LiteralPath $taskBinary).Hash.ToLowerInvariant()
Stop-Process -Id $taskOld.Id
Wait-Process -Id $taskOld.Id -Timeout 10 -ErrorAction SilentlyContinue
if(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue){throw '端口仍被占用'}
$env:COOLZHU_RUNTIME_DIR=$taskReceipt.workspace
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
$taskConfig=Join-Path $taskReceipt.workspace 'coolzhu.toml'
$taskBytes=[IO.File]::ReadAllBytes($taskConfig)
try {
 [IO.File]::WriteAllText($taskConfig,[Text.Encoding]::UTF8.GetString($taskBytes).Replace('bind_addr = "127.0.0.1:8767"','bind_addr = "127.0.0.1:8765"'),[Text.UTF8Encoding]::new($false))
 $taskNew=Start-Process -FilePath $taskBinary -WorkingDirectory $taskReceipt.workspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'refreshed-out.log') -RedirectStandardError (Join-Path $taskTmp 'refreshed-err.log') -PassThru
 $taskFresh=Get-Process -Id $taskNew.Id
 $taskNewReceipt=@{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskHash;workspace=$taskReceipt.workspace;port=8765;stage=if($Formal){'恢复正式117'}else{'源码候选及匹配DSH运行资源，117正式壳保留'}}
 $taskNewReceipt|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskTmp $(if($Formal){'restored-formal-process.json'}else{'candidate-process.json'})) -Encoding utf8
 $taskReady=$false
 for($taskPoll=0;$taskPoll -lt 30;$taskPoll++){try{Invoke-RestMethod 'http://127.0.0.1:8765/api/sessions' -TimeoutSec 2|Out-Null;$taskReady=$true;break}catch{Start-Sleep -Milliseconds 300}}
 if(-not $taskReady){throw '后台未就绪'}
 if($Formal){
  $taskOriginalFile=Join-Path $taskRoot 'tmp/2026-10-09-release-117/installed-117-standard-processes.json'
  $taskOriginal=Get-Content -Raw -LiteralPath $taskOriginalFile|ConvertFrom-Json
  if($taskOriginal.web_sha256 -ine $taskHash){throw '正式摘要已改变'}
  $taskOriginal.web_pid=$taskFresh.Id;$taskOriginal.web_started=$taskNewReceipt.started
  $taskOriginal|ConvertTo-Json|Set-Content -LiteralPath $taskOriginalFile -Encoding utf8
 }
 Write-Output '匹配后台就绪'
}finally{[IO.File]::WriteAllBytes($taskConfig,$taskBytes)}
