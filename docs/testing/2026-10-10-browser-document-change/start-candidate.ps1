$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-10-browser-document-change'
$taskWorkspace=Join-Path $taskRoot 'tmp/2026-10-04-devin-models/workspace'
$taskBuild=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'build-receipt.json') | ConvertFrom-Json
if($taskBuild.actual_exit_code -ne 0){throw '候选构建尚未通过'}
$taskOld=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-10-release-126/installed-126-standard-processes.json') | ConvertFrom-Json
$taskBundle=Join-Path $taskTmp 'candidate-bundle'
if(Test-Path -LiteralPath $taskBundle){throw '不覆盖既有候选目录'}
New-Item -ItemType Directory -Path $taskBundle | Out-Null
Get-ChildItem -LiteralPath 'C:/Program Files/CoolzhuAgent' | Copy-Item -Destination $taskBundle -Recurse
Copy-Item -LiteralPath (Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-web-console.exe')
$taskWebBinary=Join-Path $taskBundle 'bin/coolzhu-web-console.exe'
$taskShellBinary=Join-Path $taskBundle 'bin/coolzhu-tauri-shell.exe'
$taskWebSha=(Get-FileHash -LiteralPath $taskWebBinary).Hash
if($taskWebSha -ine (Get-FileHash -LiteralPath (Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe')).Hash -or (Get-FileHash -LiteralPath $taskShellBinary).Hash -ine $taskOld.shell_sha256){throw '候选文件摘要不符'}
python (Join-Path $taskRoot 'tmp/2026-10-10-release-126/preflight-idle.py')
if($LASTEXITCODE -ne 0){throw '原工作区尚未空闲，不停止正式版'}
$taskProcesses=@()
foreach($taskIdentity in @(@{pid=$taskOld.shell_pid;path=$taskOld.shell_binary;started=$taskOld.shell_started;sha=$taskOld.shell_sha256},@{pid=$taskOld.web_pid;path=$taskOld.web_binary;started=$taskOld.web_started;sha=$taskOld.web_sha256})) {
 $taskProcess=Get-Process -Id $taskIdentity.pid
 if($taskProcess.Path -ine $taskIdentity.path -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskIdentity.started).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskIdentity.sha){throw '正式实例身份不符，保留进程'}
 $taskProcesses+=,$taskProcess
}
foreach($taskProcess in $taskProcesses){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
@{previous=$taskOld;identity_verified=$true;reason='空闲工作区切换源码候选，验证失效错误保留；不新建Devin会话'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskTmp 'stopped-formal126.json') -Encoding utf8
if(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue){throw '端口仍占用，保留现有服务'}
$taskConfig=Join-Path $taskWorkspace 'coolzhu.toml';$taskBytes=[IO.File]::ReadAllBytes($taskConfig)
$taskGui=Join-Path ([IO.Path]::GetTempPath()) 'coolzhu-gui-web-url.txt';$taskGuiExists=Test-Path -LiteralPath $taskGui
$taskGuiBytes=if($taskGuiExists){[IO.File]::ReadAllBytes($taskGui)}else{$null}
try {
 $taskText=[Text.Encoding]::UTF8.GetString($taskBytes)
 if(-not ($taskText.Contains('bind_addr = "127.0.0.1:8765"') -or $taskText.Contains('bind_addr = "127.0.0.1:8767"'))){throw '配置端口不符'}
 [IO.File]::WriteAllText($taskConfig,$taskText.Replace('bind_addr = "127.0.0.1:8767"','bind_addr = "127.0.0.1:8765"'),[Text.UTF8Encoding]::new($false))
 Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
 Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
 Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
 Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
 $env:COOLZHU_RUNTIME_DIR=$taskWorkspace;$env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety';$env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
 $taskWeb=Start-Process -FilePath $taskWebBinary -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'candidate-web-out.log') -RedirectStandardError (Join-Path $taskTmp 'candidate-web-err.log') -PassThru
 $taskReady=$false
 for($taskPoll=0;$taskPoll -lt 30;$taskPoll++){try{Invoke-RestMethod 'http://127.0.0.1:8765/api/sessions' -TimeoutSec 2 | Out-Null;$taskReady=$true;break}catch{Start-Sleep -Milliseconds 300}}
 if(-not $taskReady){throw '候选后台尚未就绪'}
 $taskShell=Start-Process -FilePath $taskShellBinary -ArgumentList '--show-console' -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'candidate-shell-out.log') -RedirectStandardError (Join-Path $taskTmp 'candidate-shell-err.log') -PassThru
 $taskFreshWeb=Get-Process -Id $taskWeb.Id;$taskFreshShell=Get-Process -Id $taskShell.Id
 @{web_pid=$taskFreshWeb.Id;web_started=$taskFreshWeb.StartTime.ToUniversalTime().ToString('o');web_binary=$taskFreshWeb.Path;shell_pid=$taskFreshShell.Id;shell_started=$taskFreshShell.StartTime.ToUniversalTime().ToString('o');shell_binary=$taskFreshShell.Path;workspace=$taskWorkspace;port=8765;stage='文档失效报告源码候选（原唯一Devin绑定）';web_sha256=$taskWebSha;shell_sha256=(Get-FileHash -LiteralPath $taskShellBinary).Hash} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskTmp 'candidate-processes.json') -Encoding utf8
 Write-Output '源码候选与原工作区已启动；不是正式安装版'
} finally {
 [IO.File]::WriteAllBytes($taskConfig,$taskBytes)
 if($taskGuiExists){[IO.File]::WriteAllBytes($taskGui,$taskGuiBytes)}elseif(Test-Path -LiteralPath $taskGui){Remove-Item -LiteralPath $taskGui}
}
