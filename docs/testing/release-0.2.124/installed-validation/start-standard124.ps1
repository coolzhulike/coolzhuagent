$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-release-124'
$taskWorkspace=Join-Path $taskRoot 'tmp/2026-10-04-devin-models/workspace'
$taskVerified=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'installed-124-verification.json') | ConvertFrom-Json
if($taskVerified.version -ne '0.2.124' -or -not $taskVerified.package_safe){throw '正式安装摘要尚未核验'}
if(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue){throw '8765被占用，保留现有服务'}
$taskConfig=Join-Path $taskWorkspace 'coolzhu.toml'
$taskBytes=[IO.File]::ReadAllBytes($taskConfig)
$taskGui=Join-Path ([IO.Path]::GetTempPath()) 'coolzhu-gui-web-url.txt'
$taskGuiExists=Test-Path -LiteralPath $taskGui
$taskGuiBytes=if($taskGuiExists){[IO.File]::ReadAllBytes($taskGui)}else{$null}
try {
 $taskText=[Text.Encoding]::UTF8.GetString($taskBytes)
 if(-not ($taskText.Contains('bind_addr = "127.0.0.1:8765"') -or $taskText.Contains('bind_addr = "127.0.0.1:8767"'))){throw '验收配置与预期不同'}
 [IO.File]::WriteAllText($taskConfig,$taskText.Replace('bind_addr = "127.0.0.1:8767"','bind_addr = "127.0.0.1:8765"'),[Text.UTF8Encoding]::new($false))
 Remove-Item Env:COOLZHU_BROWSER_NAV_DIAGNOSTICS -ErrorAction SilentlyContinue
 Remove-Item Env:COOLZHU_WEB_STATIC_ROOT -ErrorAction SilentlyContinue
 Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
 Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
 $env:COOLZHU_RUNTIME_DIR=$taskWorkspace
 $env:COOLZHU_INPUT_SAFETY_STATE_ROOT='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
 $env:COOLZHU_GUI_WEB_URL='http://127.0.0.1:8765/'
 $taskWebBinary='C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe'
 $taskShellBinary='C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe'
 if((Get-FileHash -LiteralPath $taskWebBinary).Hash -ne $taskVerified.web_sha256 -or (Get-FileHash -LiteralPath $taskShellBinary).Hash -ne $taskVerified.shell_sha256){throw '正式文件与核验摘要不一致'}
 $taskWeb=Start-Process -FilePath $taskWebBinary -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'installed-124-standard-web-out.log') -RedirectStandardError (Join-Path $taskTmp 'installed-124-standard-web-err.log') -PassThru
 $taskReady=$false
 for($taskPoll=0;$taskPoll -lt 30;$taskPoll++){try{Invoke-RestMethod 'http://127.0.0.1:8765/api/sessions' -TimeoutSec 2 | Out-Null;$taskReady=$true;break}catch{Start-Sleep -Milliseconds 300}}
 if(-not $taskReady){throw '安装版后台未就绪'}
 $taskShell=Start-Process -FilePath $taskShellBinary -ArgumentList '--show-console' -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskTmp 'installed-124-standard-shell-out.log') -RedirectStandardError (Join-Path $taskTmp 'installed-124-standard-shell-err.log') -PassThru
 $taskShellFresh=Get-Process -Id $taskShell.Id
 $taskWebFresh=Get-Process -Id $taskWeb.Id
 if($taskShellFresh.Path -ine [IO.Path]::GetFullPath($taskShellBinary) -or $taskWebFresh.Path -ine [IO.Path]::GetFullPath($taskWebBinary)){throw '实际EXE路径未确认，保留进程待核对'}
 @{web_pid=$taskWebFresh.Id;web_started=$taskWebFresh.StartTime.ToUniversalTime().ToString('o');web_binary=$taskWebFresh.Path;shell_pid=$taskShellFresh.Id;shell_started=$taskShellFresh.StartTime.ToUniversalTime().ToString('o');shell_binary=$taskShellFresh.Path;workspace=$taskWorkspace;port=8765;stage='0.2.124 Program Files正式安装版';web_sha256=$taskVerified.web_sha256;shell_sha256=$taskVerified.shell_sha256} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskTmp 'installed-124-standard-processes.json') -Encoding utf8
 Write-Output '0.2.124正式后台与原生窗口已启动'
}finally{
 [IO.File]::WriteAllBytes($taskConfig,$taskBytes)
 if($taskGuiExists){[IO.File]::WriteAllBytes($taskGui,$taskGuiBytes)}elseif(Test-Path -LiteralPath $taskGui){Remove-Item -LiteralPath $taskGui}
}
