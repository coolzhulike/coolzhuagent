$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-browser-progress'
$taskReceipt=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-122/installed-122-standard-processes.json')|ConvertFrom-Json
$taskBuild=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-browser-progress/web-build-result.json')|ConvertFrom-Json
$taskTests=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-browser-progress/web-result.json')|ConvertFrom-Json
$taskCore=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-browser-progress/shell-build-result.json')|ConvertFrom-Json
if($taskBuild.exit_code -ne 0 -or $taskTests.exit_code -ne 0 -or $taskCore.exit_code -ne 0){throw '最终编译/回归没有真实通过'}
foreach($taskCheck in @('protocol','shell','linkage')){if((Get-Content -Raw -LiteralPath (Join-Path $taskTmp ($taskCheck+'-result.json'))|ConvertFrom-Json).exit_code -ne 0){throw '候选必需回归未通过'}}
$taskSource=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'candidate-source.json')|ConvertFrom-Json
foreach($taskFile in $taskSource){if((Get-FileHash -LiteralPath (Join-Path $taskRoot $taskFile.path)).Hash -ine $taskFile.sha256){throw '候选源码变化，不能使用旧构建'}}
$taskBundle=Join-Path $taskTmp 'candidate-bundle'
if(Test-Path -LiteralPath $taskBundle){throw '候选目录已存在，禁止覆盖'}
New-Item -ItemType Directory -Path $taskBundle|Out-Null
Get-ChildItem -LiteralPath 'C:/Program Files/CoolzhuAgent'|Copy-Item -Destination $taskBundle -Recurse
Copy-Item -LiteralPath (Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-web-console.exe')
Copy-Item -LiteralPath (Join-Path $taskRoot 'modules/gui-desktop/packages/tauri-shell/src-tauri/target/debug/coolzhu-tauri-shell.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-tauri-shell.exe')
$taskWebBinary=Join-Path $taskBundle 'bin/coolzhu-web-console.exe';$taskShellBinary=Join-Path $taskBundle 'bin/coolzhu-tauri-shell.exe'
$taskWebHash=(Get-FileHash -LiteralPath $taskWebBinary).Hash.ToLowerInvariant();$taskShellHash=(Get-FileHash -LiteralPath $taskShellBinary).Hash.ToLowerInvariant()
if($taskShellHash -ine (Get-FileHash -LiteralPath (Join-Path $taskRoot 'modules/gui-desktop/packages/tauri-shell/src-tauri/target/debug/coolzhu-tauri-shell.exe')).Hash){throw '候选壳不是本轮构建'}
$taskOwned=@()
foreach($taskKind in @('shell','web')){
 $taskProcess=Get-Process -Id $taskReceipt.($taskKind+'_pid')
 if($taskProcess.Path -ine $taskReceipt.($taskKind+'_binary') -or $taskProcess.StartTime.ToUniversalTime().Ticks -ne ([datetime]$taskReceipt.($taskKind+'_started')).ToUniversalTime().Ticks -or (Get-FileHash -LiteralPath $taskProcess.Path).Hash -ine $taskReceipt.($taskKind+'_sha256')){throw '122三项身份不符，保留进程'}
 $taskOwned+=,$taskProcess
}
foreach($taskProcess in $taskOwned){Stop-Process -Id $taskProcess.Id;Wait-Process -Id $taskProcess.Id -Timeout 10 -ErrorAction SilentlyContinue}
if(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue){throw '8765仍占用，保留现有服务'}
$taskConfig=Join-Path $taskReceipt.workspace 'coolzhu.toml';$taskBytes=[IO.File]::ReadAllBytes($taskConfig)
$taskGui=Join-Path ([IO.Path]::GetTempPath()) 'coolzhu-gui-web-url.txt';$taskGuiExists=Test-Path -LiteralPath $taskGui
$taskGuiBytes=if($taskGuiExists){[IO.File]::ReadAllBytes($taskGui)}else{$null}
try{
 $taskText=[Text.Encoding]::UTF8.GetString($taskBytes)
 if(-not ($taskText.Contains('bind_addr = "127.0.0.1:8765"') -or $taskText.Contains('bind_addr = "127.0.0.1:8767"'))){throw '工程绑定不符'}
 [IO.File]::WriteAllText($taskConfig,$taskText.Replace('bind_addr = "127.0.0.1:8767"','bind_addr = "127.0.0.1:8765"'),[Text.UTF8Encoding]::new($false))
 foreach($taskEnv in @('COOLZHU_BROWSER_NAV_DIAGNOSTICS','COOLZHU_WEB_STATIC_ROOT','WEBVIEW2_USER_DATA_FOLDER','WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')){Remove-Item -LiteralPath ('Env:'+ $taskEnv) -ErrorAction SilentlyContinue}
 $env:COOLZHU_RUNTIME_DIR=$taskReceipt.workspace
 $env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
 $env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
 $taskWeb=Start-Process -FilePath $taskWebBinary -WorkingDirectory $taskReceipt.workspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'candidate-web-out.log') -RedirectStandardError (Join-Path $taskTmp 'candidate-web-err.log') -PassThru
 $taskReady=$false
 for($taskPoll=0;$taskPoll -lt 30;$taskPoll++){try{Invoke-RestMethod 'http://127.0.0.1:8765/api/sessions' -TimeoutSec 2|Out-Null;$taskReady=$true;break}catch{Start-Sleep -Milliseconds 300}}
 if(-not $taskReady){throw '候选后台未就绪'}
 $taskShell=Start-Process -FilePath $taskShellBinary -ArgumentList '--show-console' -WorkingDirectory $taskReceipt.workspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'candidate-shell-out.log') -RedirectStandardError (Join-Path $taskTmp 'candidate-shell-err.log') -PassThru
 $taskWebFresh=Get-Process -Id $taskWeb.Id;$taskShellFresh=Get-Process -Id $taskShell.Id
 if($taskWebFresh.Path -ine [IO.Path]::GetFullPath($taskWebBinary) -or $taskShellFresh.Path -ine [IO.Path]::GetFullPath($taskShellBinary)){throw '候选实际路径未确认'}
 @{web_pid=$taskWebFresh.Id;web_started=$taskWebFresh.StartTime.ToUniversalTime().ToString('o');web_binary=$taskWebFresh.Path;web_sha256=$taskWebHash;shell_pid=$taskShellFresh.Id;shell_started=$taskShellFresh.StartTime.ToUniversalTime().ToString('o');shell_binary=$taskShellFresh.Path;shell_sha256=$taskShellHash;workspace=$taskReceipt.workspace;port=8765;stage='本步效果源码候选Web与壳，非正式安装验收'}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskTmp 'candidate-processes.json') -Encoding utf8
 Write-Output '正常同目录候选后台及窗口已启动'
}finally{[IO.File]::WriteAllBytes($taskConfig,$taskBytes);if($taskGuiExists){[IO.File]::WriteAllBytes($taskGui,$taskGuiBytes)}elseif(Test-Path -LiteralPath $taskGui){Remove-Item -LiteralPath $taskGui}}
