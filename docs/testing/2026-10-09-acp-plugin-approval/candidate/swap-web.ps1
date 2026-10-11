$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-plugin-frozen-permission-candidate'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-117/installed-117-standard-processes.json') | ConvertFrom-Json
$taskOld=Get-Process -Id $taskReceipt.web_pid
if($taskOld.Path -ine $taskReceipt.web_binary -or $taskOld.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.web_started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskOld.Path).Hash -ine $taskReceipt.web_sha256){throw '原后台路径、摘要或启动时间不符，保留进程'}
$taskBinary=Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe'
$taskHash=(Get-FileHash -LiteralPath $taskBinary).Hash.ToLowerInvariant()
Stop-Process -Id $taskOld.Id
Wait-Process -Id $taskOld.Id -Timeout 10 -ErrorAction SilentlyContinue
if(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue){throw '端口仍被占用，不另启后台'}
$env:COOLZHU_RUNTIME_DIR=$taskReceipt.workspace
$env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
$taskConfig=Join-Path $taskReceipt.workspace 'coolzhu.toml'
$taskBytes=[IO.File]::ReadAllBytes($taskConfig)
try {
 $taskText=[Text.Encoding]::UTF8.GetString($taskBytes)
 [IO.File]::WriteAllText($taskConfig,$taskText.Replace('bind_addr = "127.0.0.1:8767"','bind_addr = "127.0.0.1:8765"'),[Text.UTF8Encoding]::new($false))
 $taskNew=Start-Process -FilePath $taskBinary -WorkingDirectory $taskReceipt.workspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'web-out.log') -RedirectStandardError (Join-Path $taskTmp 'web-err.log') -PassThru
 $taskFresh=Get-Process -Id $taskNew.Id
 @{pid=$taskFresh.Id;started=$taskFresh.StartTime.ToUniversalTime().ToString('o');binary=$taskFresh.Path;sha256=$taskHash;stage='源码候选后台，117正式壳保留';workspace=$taskReceipt.workspace;port=8765}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskTmp 'candidate-process.json') -Encoding utf8
 $taskReady=$false
 for($taskPoll=0;$taskPoll -lt 30;$taskPoll++){try{Invoke-RestMethod 'http://127.0.0.1:8765/api/sessions' -TimeoutSec 2|Out-Null;$taskReady=$true;break}catch{Start-Sleep -Milliseconds 300}}
 if(-not $taskReady){throw '候选后台未就绪'}
 Write-Output '候选后台已启动；未创建新云端会话'
}finally{[IO.File]::WriteAllBytes($taskConfig,$taskBytes)}
