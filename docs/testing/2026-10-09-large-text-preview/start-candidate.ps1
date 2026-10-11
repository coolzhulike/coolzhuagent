$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-large-text-preview'
$taskWorkspace=Join-Path $taskTmp 'workspace'
$taskVerified=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-09-release-125/installed-125-verification.json') | ConvertFrom-Json
$taskBuild=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'build-result.json') | ConvertFrom-Json
if($taskBuild.actual_exit_code -ne 0 -or -not $taskVerified.package_safe){throw '构建或原安装摘要未确认'}
if(Get-NetTCPConnection -LocalPort 8767 -State Listen -ErrorAction SilentlyContinue){throw '8767被占用，保留既有服务'}
$taskBundle=Join-Path $taskTmp 'candidate-bundle'
if(Test-Path -LiteralPath $taskBundle){throw '候选目录已存在，不能覆盖既有收据'}
New-Item -ItemType Directory -Path $taskBundle | Out-Null
Get-ChildItem -LiteralPath 'C:/Program Files/CoolzhuAgent' | Copy-Item -Destination $taskBundle -Recurse
Copy-Item -LiteralPath (Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-web-console.exe')
$taskWebBinary=Join-Path $taskBundle 'bin/coolzhu-web-console.exe'
$taskShellBinary=Join-Path $taskBundle 'bin/coolzhu-tauri-shell.exe'
if((Get-FileHash -LiteralPath $taskWebBinary).Hash -ine $taskBuild.binary_sha256 -or (Get-FileHash -LiteralPath $taskShellBinary).Hash -ine $taskVerified.shell_sha256){throw '候选副本摘要不一致'}
$taskGui=Join-Path ([IO.Path]::GetTempPath()) 'coolzhu-gui-web-url.txt'
$taskGuiExists=Test-Path -LiteralPath $taskGui
$taskGuiBytes=if($taskGuiExists){[IO.File]::ReadAllBytes($taskGui)}else{$null}
try {
 Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
 Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
 Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
 Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
 $env:COOLZHU_RUNTIME_DIR=$taskWorkspace
 $env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
 $env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8767/'
 $taskWeb=Start-Process -FilePath $taskWebBinary -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'candidate-web-out.log') -RedirectStandardError (Join-Path $taskTmp 'candidate-web-err.log') -PassThru
 $taskReady=$false
 for($taskPoll=0;$taskPoll -lt 30;$taskPoll++){try{Invoke-RestMethod 'http://127.0.0.1:8767/api/sessions' -TimeoutSec 2 | Out-Null;$taskReady=$true;break}catch{Start-Sleep -Milliseconds 300}}
 if(-not $taskReady){throw '候选后台未就绪，保留进程待核对'}
 $taskShell=Start-Process -FilePath $taskShellBinary -ArgumentList '--show-console' -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'candidate-shell-out.log') -RedirectStandardError (Join-Path $taskTmp 'candidate-shell-err.log') -PassThru
 $taskShellFresh=Get-Process -Id $taskShell.Id
 $taskWebFresh=Get-Process -Id $taskWeb.Id
 if($taskShellFresh.Path -ine [IO.Path]::GetFullPath($taskShellBinary) -or $taskWebFresh.Path -ine [IO.Path]::GetFullPath($taskWebBinary)){throw '实际EXE路径不符'}
 @{web_pid=$taskWebFresh.Id;web_started=$taskWebFresh.StartTime.ToUniversalTime().ToString('o');web_binary=$taskWebFresh.Path;shell_pid=$taskShellFresh.Id;shell_started=$taskShellFresh.StartTime.ToUniversalTime().ToString('o');shell_binary=$taskShellFresh.Path;workspace=$taskWorkspace;port=8767;stage='大文本只读预览源码候选（真实SQLite独立副本，无模型调用）';web_sha256=$taskBuild.binary_sha256;shell_sha256=$taskVerified.shell_sha256} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskTmp 'candidate-processes.json') -Encoding utf8
 Write-Output '独立数据库候选后台与原生窗口已启动'
}finally{
 if($taskGuiExists){[IO.File]::WriteAllBytes($taskGui,$taskGuiBytes)}elseif(Test-Path -LiteralPath $taskGui){Remove-Item -LiteralPath $taskGui}
}
